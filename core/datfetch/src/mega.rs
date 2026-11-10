//! The archive host's public links: what a link names, the request that asks for the file, the
//! reply, and the file's decryption and integrity check.
//!
//! A public link carries the file's key after `#`, so the host never sees it. The key is 32 bytes:
//! the AES-128 key is its two halves XORed together, the counter's first 8 bytes are bytes 16..24
//! (the rest is the block number, big-endian), and bytes 24..32 are the file's MAC.
//!
//! The MAC is CBC-MAC over chunks of the plain file. Chunk `i` (from 1) is `min(i, 8)` × 128 KiB
//! long; each chunk's MAC starts from the counter's 8 bytes twice, and the file's MAC is CBC-MAC
//! over the chunk MACs from zero, folded to 8 bytes (`a ^ b`, `c ^ d` of its four words).

use aes::cipher::{BlockEncrypt, KeyInit};
use aes::Aes128;
use base64::engine::general_purpose::URL_SAFE_NO_PAD_INDIFFERENT as B64;
use base64::Engine as _;
use sha2::{Digest, Sha256};

/// The host's API, which answers where a file can be downloaded from.
pub const API: &str = "https://g.api.mega.co.nz/cs";

/// A file's 32-byte key, from its link (or, for a file in a shared folder, from the folder's
/// listing).
#[derive(Clone, PartialEq, Eq)]
pub struct FileKey([u8; 32]);

impl std::fmt::Debug for FileKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileKey(..)")
    }
}

impl FileKey {
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> FileKey {
        FileKey(bytes)
    }

    /// The key as a link writes it: URL-safe base64 of the 32 bytes.
    pub fn parse(text: &str) -> Result<FileKey, LinkError> {
        let bytes = B64.decode(text.trim()).map_err(|_| LinkError::BadKey)?;
        <[u8; 32]>::try_from(bytes.as_slice())
            .map(FileKey)
            .map_err(|_| LinkError::BadKey)
    }

    /// A key from its parts: the AES key, the counter's first 8 bytes and the file's MAC. The
    /// link's first 16 bytes are the AES key XORed with the last 16.
    #[must_use]
    pub fn compose(aes: [u8; 16], nonce: [u8; 8], mac: [u8; 8]) -> FileKey {
        let mut k = [0u8; 32];
        k[16..24].copy_from_slice(&nonce);
        k[24..].copy_from_slice(&mac);
        for i in 0..16 {
            k[i] = aes[i] ^ k[i + 16];
        }
        FileKey(k)
    }

    /// The key as a link writes it.
    #[must_use]
    pub fn to_text(&self) -> String {
        B64.encode(self.0)
    }

    fn aes_key(&self) -> [u8; 16] {
        let mut k = [0u8; 16];
        for (i, b) in k.iter_mut().enumerate() {
            *b = self.0[i] ^ self.0[i + 16];
        }
        k
    }

    fn aes(&self) -> Aes128 {
        Aes128::new(&self.aes_key().into())
    }

    fn nonce(&self) -> [u8; 8] {
        let mut n = [0u8; 8];
        n.copy_from_slice(&self.0[16..24]);
        n
    }

    fn meta_mac(&self) -> [u8; 8] {
        let mut m = [0u8; 8];
        m.copy_from_slice(&self.0[24..32]);
        m
    }
}

/// Why a link could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// Not a link this reader knows: `https://mega.nz/file/<handle>#<key>`, the older
    /// `https://mega.nz/#!<handle>!<key>`, or a file in a shared folder,
    /// `https://mega.nz/folder/<folder>#<folder key>/file/<file>`.
    NotALink,
    /// The key is not 32 bytes of URL-safe base64.
    BadKey,
    /// A file in a shared folder, with no key of its own given: its key is only in the folder's
    /// listing.
    NoFileKey,
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            LinkError::NotALink => "not a link to a file on mega.nz",
            LinkError::BadKey => "the link's key is not a file key",
            LinkError::NoFileKey => "a file in a shared folder needs its own key",
        })
    }
}

impl std::error::Error for LinkError {}

/// One file the host keeps, as a request names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRef {
    /// The shared folder the file is in, when it is one of a folder's files rather than a file
    /// shared on its own.
    pub folder: Option<String>,
    /// The file's handle.
    pub handle: String,
    pub key: FileKey,
}

fn is_handle(s: &str) -> bool {
    s.len() == 8
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl FileRef {
    /// Read a link. A file in a shared folder takes its own key in `file_key` (URL-safe base64,
    /// as a file link writes it); a file shared on its own carries its key in the link.
    pub fn parse(link: &str, file_key: Option<&str>) -> Result<FileRef, LinkError> {
        let link = link.trim();
        let rest = ["https://mega.nz/", "https://mega.co.nz/", "mega.nz/"]
            .iter()
            .find_map(|p| link.strip_prefix(p))
            .ok_or(LinkError::NotALink)?;
        if let Some(r) = rest.strip_prefix("file/") {
            let (handle, key) = r.split_once('#').ok_or(LinkError::NotALink)?;
            if !is_handle(handle) {
                return Err(LinkError::NotALink);
            }
            return Ok(FileRef {
                folder: None,
                handle: handle.to_owned(),
                key: FileKey::parse(key)?,
            });
        }
        if let Some(r) = rest.strip_prefix("#!") {
            let (handle, key) = r.split_once('!').ok_or(LinkError::NotALink)?;
            if !is_handle(handle) {
                return Err(LinkError::NotALink);
            }
            return Ok(FileRef {
                folder: None,
                handle: handle.to_owned(),
                key: FileKey::parse(key)?,
            });
        }
        if let Some(r) = rest.strip_prefix("folder/") {
            let (folder, after) = r.split_once('#').ok_or(LinkError::NotALink)?;
            let (_, file) = after.split_once("/file/").ok_or(LinkError::NotALink)?;
            if !is_handle(folder) || !is_handle(file) {
                return Err(LinkError::NotALink);
            }
            let key = FileKey::parse(file_key.ok_or(LinkError::NoFileKey)?)?;
            return Ok(FileRef {
                folder: Some(folder.to_owned()),
                handle: file.to_owned(),
                key,
            });
        }
        Err(LinkError::NotALink)
    }

    /// The request that asks where to download the file: a POST of `body` to `url`. `seq` is the
    /// request's sequence number, any number the caller has not used recently.
    #[must_use]
    pub fn info_request(&self, seq: u64) -> ApiRequest {
        let mut url = format!("{API}?id={seq}");
        let field = match &self.folder {
            Some(f) => {
                url.push_str("&n=");
                url.push_str(f);
                "n"
            }
            None => "p",
        };
        let body = serde_json::json!([{ "a": "g", "g": 1, "ssl": 2, field: self.handle }]);
        ApiRequest {
            url,
            body: body.to_string(),
        }
    }

    /// Read the API's answer to [`FileRef::info_request`].
    pub fn read_info(&self, body: &str) -> Result<FileInfo, ApiError> {
        let v: serde_json::Value =
            serde_json::from_str(body).map_err(|e| ApiError::Malformed(e.to_string()))?;
        // A whole-request error is a bare number; a per-command one is a number in the list.
        let first = match &v {
            serde_json::Value::Array(a) => a.first().cloned().unwrap_or(serde_json::Value::Null),
            other => other.clone(),
        };
        if let Some(code) = first.as_i64() {
            return Err(ApiError::from_code(code));
        }
        let size = first["s"]
            .as_u64()
            .ok_or_else(|| ApiError::Malformed("no size".into()))?;
        let url = first["g"]
            .as_str()
            .filter(|u| u.starts_with("https://"))
            .ok_or_else(|| ApiError::Malformed("no download address".into()))?
            .to_owned();
        let name = first["at"]
            .as_str()
            .and_then(|at| attributes_name(at, &self.key));
        Ok(FileInfo { size, url, name })
    }

    /// The decryption of this file's bytes.
    #[must_use]
    pub fn decryptor(&self) -> Decryptor {
        Decryptor {
            aes: self.key.aes(),
            nonce: self.key.nonce(),
        }
    }

    /// The check of this file's plain bytes against the MAC its key carries.
    #[must_use]
    pub fn mac(&self) -> MacCheck {
        MacCheck::new(&self.key)
    }
}

/// A POST to the API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRequest {
    pub url: String,
    pub body: String,
}

/// Where a file can be downloaded from, as the API answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    /// The file's size in bytes.
    pub size: u64,
    /// Where to download it: a GET of `<url>/<first>-<last>` answers that range of the
    /// encrypted file (both ends included).
    pub url: String,
    /// The file's name, from its attributes, when the key opens them: a wrong key reads none.
    pub name: Option<String>,
}

/// What the API refused, by its error code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// The file is gone (`-9`): deleted, or the link was never right.
    Gone,
    /// The host took the file down (`-16`).
    TakenDown,
    /// The transfer quota is spent (`-17`).
    OverQuota,
    /// The host is busy, or asks the caller to slow down (`-3`, `-4`, `-6`, `-18`): try again
    /// later.
    TryAgain,
    /// Any other refusal, by its code.
    Refused(i64),
    /// An answer that is not the API's.
    Malformed(String),
}

impl ApiError {
    #[must_use]
    pub fn from_code(code: i64) -> ApiError {
        match code {
            -9 => ApiError::Gone,
            -16 => ApiError::TakenDown,
            -17 => ApiError::OverQuota,
            -3 | -4 | -6 | -18 => ApiError::TryAgain,
            c => ApiError::Refused(c),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Gone => f.write_str("the file is no longer there"),
            ApiError::TakenDown => f.write_str("the host has taken the file down"),
            ApiError::OverQuota => {
                f.write_str("the host's download limit for this connection is spent")
            }
            ApiError::TryAgain => f.write_str("the host is busy; try again later"),
            ApiError::Refused(c) => write!(f, "the host refused the request (error {c})"),
            ApiError::Malformed(why) => write!(f, "the host's answer could not be read ({why})"),
        }
    }
}

impl std::error::Error for ApiError {}

/// The name in a file's encrypted attributes: AES-CBC from a zero IV, `MEGA{json}` inside.
fn attributes_name(at: &str, key: &FileKey) -> Option<String> {
    let mut bytes = B64.decode(at).ok()?;
    if bytes.is_empty() || bytes.len() % 16 != 0 {
        return None;
    }
    let aes = key.aes();
    let mut prev = [0u8; 16];
    for chunk in bytes.as_chunks_mut::<16>().0 {
        let cipher = *chunk;
        let mut b = cipher.into();
        aes::cipher::BlockDecrypt::decrypt_block(&aes, &mut b);
        for (i, x) in b.iter().enumerate() {
            chunk[i] = x ^ prev[i];
        }
        prev = cipher;
    }
    let text = std::str::from_utf8(&bytes).ok()?;
    let json = text.strip_prefix("MEGA")?.trim_end_matches('\0');
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    v["n"].as_str().map(str::to_owned)
}

/// AES-128 in counter mode, from any offset of the file.
#[derive(Clone)]
pub struct Decryptor {
    aes: Aes128,
    nonce: [u8; 8],
}

impl std::fmt::Debug for Decryptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Decryptor(..)")
    }
}

impl Decryptor {
    /// Decrypt (or encrypt: it is the same) `data` in place, `data[0]` being byte `offset` of the
    /// file.
    pub fn apply(&self, offset: u64, data: &mut [u8]) {
        let mut block = offset / 16;
        let mut skip = usize::try_from(offset % 16).unwrap_or(0);
        let mut at = 0;
        while at < data.len() {
            let mut ks = [0u8; 16];
            ks[..8].copy_from_slice(&self.nonce);
            ks[8..].copy_from_slice(&block.to_be_bytes());
            let mut b = ks.into();
            self.aes.encrypt_block(&mut b);
            let take = (16 - skip).min(data.len() - at);
            for i in 0..take {
                data[at + i] ^= b[skip + i];
            }
            at += take;
            skip = 0;
            block += 1;
        }
    }
}

/// The length of MAC chunk `index` (from 0): 128 KiB, 256 KiB, ... up to 1 MiB, then 1 MiB each.
#[must_use]
pub fn mac_chunk_len(index: u64) -> u64 {
    (index + 1).min(8) * 128 * 1024
}

/// The file's MAC, computed as its plain bytes are given in order.
#[derive(Clone)]
pub struct MacCheck {
    aes: Aes128,
    nonce: [u8; 8],
    expected: [u8; 8],
    chunk: [u8; 16],
    in_chunk: u64,
    chunk_index: u64,
    file: [u8; 16],
    pending: [u8; 16],
    pending_len: usize,
    total: u64,
}

impl std::fmt::Debug for MacCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MacCheck")
            .field("total", &self.total)
            .finish_non_exhaustive()
    }
}

impl MacCheck {
    fn new(key: &FileKey) -> MacCheck {
        let nonce = key.nonce();
        MacCheck {
            aes: key.aes(),
            nonce,
            expected: key.meta_mac(),
            chunk: Self::start(nonce),
            in_chunk: 0,
            chunk_index: 0,
            file: [0; 16],
            pending: [0; 16],
            pending_len: 0,
            total: 0,
        }
    }

    fn start(nonce: [u8; 8]) -> [u8; 16] {
        let mut m = [0u8; 16];
        m[..8].copy_from_slice(&nonce);
        m[8..].copy_from_slice(&nonce);
        m
    }

    fn encrypt(&self, block: &mut [u8; 16]) {
        let mut b = (*block).into();
        self.aes.encrypt_block(&mut b);
        block.copy_from_slice(&b);
    }

    fn block(&mut self, block: &[u8; 16]) {
        let mut m = self.chunk;
        for (a, b) in m.iter_mut().zip(block) {
            *a ^= b;
        }
        self.encrypt(&mut m);
        self.chunk = m;
        self.in_chunk += 16;
        if self.in_chunk == mac_chunk_len(self.chunk_index) {
            self.close_chunk();
        }
    }

    fn close_chunk(&mut self) {
        let mut f = self.file;
        for (a, b) in f.iter_mut().zip(&self.chunk) {
            *a ^= b;
        }
        self.encrypt(&mut f);
        self.file = f;
        self.chunk = Self::start(self.nonce);
        self.in_chunk = 0;
        self.chunk_index += 1;
    }

    /// How many bytes it has been given.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.total
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// The next plain bytes of the file.
    pub fn update(&mut self, mut data: &[u8]) {
        self.total += data.len() as u64;
        if self.pending_len > 0 {
            let take = (16 - self.pending_len).min(data.len());
            self.pending[self.pending_len..self.pending_len + take].copy_from_slice(&data[..take]);
            self.pending_len += take;
            data = &data[take..];
            if self.pending_len < 16 {
                return;
            }
            let p = self.pending;
            self.block(&p);
            self.pending_len = 0;
        }
        let (blocks, rest) = data.as_chunks::<16>();
        for b in blocks {
            self.block(b);
        }
        self.pending[..rest.len()].copy_from_slice(rest);
        self.pending_len = rest.len();
    }

    /// The file's MAC, from everything given.
    #[must_use]
    pub fn finish(mut self) -> [u8; 8] {
        if self.pending_len > 0 {
            let mut p = [0u8; 16];
            p[..self.pending_len].copy_from_slice(&self.pending[..self.pending_len]);
            self.pending_len = 0;
            // The last block, zero-padded; it may itself close the chunk.
            self.block(&p);
        }
        if self.in_chunk > 0 {
            self.close_chunk();
        }
        self.fold()
    }

    fn fold(&self) -> [u8; 8] {
        let f = self.file;
        let mut out = [0u8; 8];
        for i in 0..4 {
            out[i] = f[i] ^ f[i + 4];
            out[i + 4] = f[i + 8] ^ f[i + 12];
        }
        out
    }

    /// Whether everything given is the file the key was made for.
    #[must_use]
    pub fn matches(self) -> bool {
        let expected = self.expected;
        self.finish() == expected
    }
}

/// Answer the API's proof-of-work challenge (its `X-Hashcash` header,
/// `1:<easiness>:<time>:<token>`): the value to send back in an `X-Hashcash` header with the same request. `None` for a challenge
/// this does not understand.
///
/// The work is a SHA-256 over a 4-byte counter and the 48-byte token repeated 262,144 times, until
/// the hash's first word is at most the threshold the easiness sets.
#[must_use]
pub fn hashcash(challenge: &str) -> Option<String> {
    let mut parts = challenge.trim().split(':');
    let version: u32 = parts.next()?.parse().ok()?;
    let easiness: u32 = parts.next()?.parse().ok()?;
    let _time = parts.next()?;
    let token_text = parts.next()?;
    if version != 1 {
        return None;
    }
    let token = B64.decode(token_text).ok()?;
    if token.len() != 48 {
        return None;
    }
    let threshold = hashcash_threshold(easiness);
    let mut buf = vec![0u8; 4 + 262_144 * 48];
    for chunk in buf[4..].as_chunks_mut::<48>().0 {
        chunk.copy_from_slice(&token);
    }
    for counter in 0..=u32::MAX {
        // The counter's bytes are little-endian: the first byte turns over fastest.
        buf[..4].copy_from_slice(&counter.to_le_bytes());
        let h = Sha256::digest(&buf);
        let first = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
        if first <= threshold {
            return Some(format!("1:{token_text}:{}", B64.encode(&buf[..4])));
        }
    }
    None
}

/// The largest first word a hashcash answer may hash to, for `easiness`.
#[must_use]
pub fn hashcash_threshold(easiness: u32) -> u32 {
    let base = ((easiness & 63) << 1) + 1;
    let shifts = (easiness >> 6) * 7 + 3;
    base.checked_shl(shifts).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the archive host's public-link format and its encryption, as the host defines them).
    use super::*;

    /// A key made for these tests (no file of the host's): bytes 0, 1, 2, ... 31.
    fn test_key() -> FileKey {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = u8::try_from(i).unwrap();
        }
        FileKey(k)
    }

    /// A file link, its older form and a file in a shared folder all read; the folder's file needs
    /// its own key.
    #[test]
    fn reads_file_and_folder_links() {
        let key = B64.encode(test_key().0);
        let a = FileRef::parse(&format!("https://mega.nz/file/AbCd-_12#{key}"), None).unwrap();
        assert_eq!((a.folder.as_deref(), a.handle.as_str()), (None, "AbCd-_12"));
        let b = FileRef::parse(&format!("https://mega.nz/#!AbCd-_12!{key}"), None).unwrap();
        assert_eq!(a, b);
        let link = "https://mega.nz/folder/Fold1234#Zm9sZGVya2V5LXNpeHRlZW4/file/File5678";
        assert_eq!(FileRef::parse(link, None), Err(LinkError::NoFileKey));
        let c = FileRef::parse(link, Some(&key)).unwrap();
        assert_eq!(
            (c.folder.as_deref(), c.handle.as_str()),
            (Some("Fold1234"), "File5678")
        );
        assert_eq!(
            FileRef::parse("https://example.com/file/AbCd-_12#x", None),
            Err(LinkError::NotALink)
        );
        assert_eq!(
            FileRef::parse("https://mega.nz/file/AbCd-_12#short", None),
            Err(LinkError::BadKey)
        );
    }

    /// The request names a file by `p`, a folder's file by `n` with the folder in the query.
    #[test]
    fn info_request_names_the_file() {
        let key = B64.encode(test_key().0);
        let f = FileRef::parse(&format!("https://mega.nz/file/AbCd-_12#{key}"), None).unwrap();
        let r = f.info_request(7);
        assert_eq!(r.url, "https://g.api.mega.co.nz/cs?id=7");
        assert_eq!(r.body, r#"[{"a":"g","g":1,"p":"AbCd-_12","ssl":2}]"#);
        let link = "https://mega.nz/folder/Fold1234#Zm9sZGVya2V5LXNpeHRlZW4/file/File5678";
        let n = FileRef::parse(link, Some(&key)).unwrap().info_request(8);
        assert_eq!(n.url, "https://g.api.mega.co.nz/cs?id=8&n=Fold1234");
        assert_eq!(n.body, r#"[{"a":"g","g":1,"n":"File5678","ssl":2}]"#);
    }

    /// The API's errors are read by code, and a good answer gives the size, the address and the
    /// name from the attributes the key opens.
    #[test]
    fn reads_the_api_answer() {
        let key = B64.encode(test_key().0);
        let f = FileRef::parse(&format!("https://mega.nz/file/AbCd-_12#{key}"), None).unwrap();
        assert_eq!(f.read_info("[-9]"), Err(ApiError::Gone));
        assert_eq!(f.read_info("-3"), Err(ApiError::TryAgain));
        assert_eq!(f.read_info("[-16]"), Err(ApiError::TakenDown));
        // Attributes `MEGA{"n":"a.zip"}`, zero-padded and AES-CBC encrypted under the test key,
        // made by an implementation other than this one (Node's crypto).
        let at = "44WMb4VJ24vX3BR7PMJTOLQyTG2-DoIoTjEzRzWVmNk";
        let body =
            format!(r#"[{{"s":1234,"at":"{at}","g":"https://gfs1.userstorage.mega.co.nz/dl/x"}}]"#);
        let info = f.read_info(&body).unwrap();
        assert_eq!(info.size, 1234);
        assert_eq!(info.url, "https://gfs1.userstorage.mega.co.nz/dl/x");
        assert_eq!(info.name.as_deref(), Some("a.zip"));
        // A plain-HTTP address is refused rather than followed.
        let body = r#"[{"s":1,"g":"http://gfs1.userstorage.mega.co.nz/dl/x"}]"#;
        assert!(matches!(f.read_info(body), Err(ApiError::Malformed(_))));
    }

    /// The pattern the cross-checked vectors below are made from: byte `i` is `i * 7 + 3`.
    fn pattern(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| i.wrapping_mul(7).wrapping_add(3).to_le_bytes()[0])
            .collect()
    }

    /// Decryption from any offset is the keystream at that offset: decrypting in odd pieces gives
    /// the same bytes as in one, and the first block matches the vector made with Node's crypto.
    #[test]
    fn decrypts_from_any_offset() {
        let f = FileRef {
            folder: None,
            handle: "AbCd-_12".into(),
            key: test_key(),
        };
        let d = f.decryptor();
        let plain = pattern(1000);
        let mut whole = plain.clone();
        d.apply(0, &mut whole);
        let mut pieces = plain.clone();
        let mut at = 0;
        for len in [1usize, 15, 16, 17, 3, 500, 448] {
            d.apply(at as u64, &mut pieces[at..at + len]);
            at += len;
        }
        assert_eq!(at, 1000);
        assert_eq!(whole, pieces);
        assert_eq!(hex(&whole[..16]), "9e88c970e80679600797e3e2d04c58cc");
        d.apply(0, &mut whole);
        assert_eq!(whole, plain);
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// The MAC of files of several lengths, across the growing chunks and a last partial block,
    /// matches the values Node's crypto computes for the same bytes and key; given in odd pieces
    /// it is the same.
    #[test]
    fn mac_matches_an_independent_implementation() {
        let f = FileRef {
            folder: None,
            handle: "AbCd-_12".into(),
            key: test_key(),
        };
        let cases: [(usize, &str); 5] = [
            (0, "0000000000000000"),
            (1, "c67b0b414d0b63a3"),
            (131_072, "ba5a622d1b414e6a"),
            (131_072 + 262_144 + 5, "b752c0c404612992"),
            (5_000_000, "fc647dddf590d6b1"),
        ];
        for (len, want) in cases {
            let data = pattern(len);
            let mut m = f.mac();
            m.update(&data);
            assert_eq!(hex(&m.finish()), want, "{len} bytes in one piece");
            let mut m = f.mac();
            for piece in data.chunks(65_537) {
                m.update(piece);
            }
            assert_eq!(hex(&m.finish()), want, "{len} bytes in pieces");
        }
    }

    /// A file whose key carries its MAC matches; one changed byte does not.
    #[test]
    fn mac_check_catches_a_changed_byte() {
        let t = test_key();
        let key = FileKey::compose(
            t.aes_key(),
            t.nonce(),
            [0xb7, 0x52, 0xc0, 0xc4, 0x04, 0x61, 0x29, 0x92],
        );
        let f = FileRef {
            folder: None,
            handle: "AbCd-_12".into(),
            key,
        };
        let mut data = pattern(131_072 + 262_144 + 5);
        let mut m = f.mac();
        m.update(&data);
        assert!(m.matches());
        data[200_000] ^= 1;
        let mut m = f.mac();
        m.update(&data);
        assert!(!m.matches());
    }

    /// The chunk lengths grow by 128 KiB to 1 MiB, then stay.
    #[test]
    fn mac_chunks_grow_to_a_mebibyte() {
        let lens: Vec<u64> = (0..10).map(mac_chunk_len).collect();
        assert_eq!(lens[0], 131_072);
        assert_eq!(lens[7], 1_048_576);
        assert_eq!(lens[9], 1_048_576);
    }

    /// The hashcash answer hashes under the challenge's threshold, and repeats the token.
    #[test]
    fn hashcash_answers_an_easy_challenge() {
        // The easiest challenge: easiness 255 gives base 127 and 24 shifts, so about half of all
        // hashes answer it.
        assert_eq!(hashcash_threshold(255), 127 << 24);
        let token = B64.encode([5u8; 48]);
        let answer = hashcash(&format!("1:255:1700000000:{token}")).unwrap();
        let (prefix, counter) = answer.rsplit_once(':').unwrap();
        assert_eq!(prefix, format!("1:{token}"));
        let mut buf = B64.decode(counter).unwrap();
        assert_eq!(buf.len(), 4);
        for _ in 0..262_144 {
            buf.extend_from_slice(&[5u8; 48]);
        }
        let h = Sha256::digest(&buf);
        assert!(u32::from_be_bytes([h[0], h[1], h[2], h[3]]) <= hashcash_threshold(255));
        assert_eq!(hashcash("2:1:1:x"), None);
    }
}
