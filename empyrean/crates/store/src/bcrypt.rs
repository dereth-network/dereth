//! bcrypt (Provos and Mazières, "A Future-Adaptable Password Scheme", 1999): the EksBlowfish
//! password hash, in the `$2a$` / `$2b$` / `$2y$` modular-crypt format.
//!
//! Not ACE code. ACE hashes account passwords with the `BCrypt.Net-Next` 4.2.0 package
//! (`ACE.Common/Cryptography/BCryptProvider.cs`, ported in [`crate::bcrypt_provider`]); no bcrypt
//! crate is available offline, so the algorithm is implemented here from the paper and the OpenBSD
//! reference, and the string handling follows `BCrypt.Net-Next`:
//!
//! * the salt string is `$2<rev>$<cost>$<22 salt chars>`; anything after the 22 salt chars (the hash,
//!   when a stored hash is passed as the salt) is ignored;
//! * revisions `a`, `b`, `x` and `y` (and none: `$2$`) are accepted and computed identically, except
//!   that a revision letter appends a NUL to the key. `BCrypt.Net-Next` does not reproduce the
//!   crypt_blowfish `$2x$` sign-extension bug, and neither does this;
//! * the key is the UTF-8 password; key expansion cycles over it, so only its first 72 bytes
//!   (including the NUL) matter;
//! * the output re-encodes the decoded salt, so a non-canonical final salt character is normalised.
//!
//! Tested against the OpenBSD / jBCrypt and OpenWall crypt_blowfish published vectors
//! (`empyrean/crates/store/tests/all/persistence/bcrypt.rs`).

use crate::blowfish_tables::{P_INIT, S_INIT};

/// The bcrypt base64 alphabet (not RFC 4648: `./` first, then `A-Z`, `a-z`, `0-9`, no padding).
const BASE64_CODE: &[u8; 64] = b"./ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// "OrpheanBeholderScryDoubt", the plaintext bcrypt encrypts 64 times.
const BF_CRYPT_CIPHERTEXT: [u32; 6] = [
    0x4f72_7068,
    0x6561_6e42,
    0x6568_6f6c,
    0x6465_7253,
    0x6372_7944,
    0x6f75_6274,
];

/// Salt length in bytes.
pub const BCRYPT_SALT_LEN: usize = 16;

/// The minimum and maximum work factor `GenerateSalt` accepts.
pub const MIN_ROUNDS: i32 = 4;
/// See [`MIN_ROUNDS`].
pub const MAX_ROUNDS: i32 = 31;

/// Why a salt or hash string could not be used: the .NET exception `BCrypt.Net-Next` throws.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BCryptError {
    /// `BCrypt.Net.SaltParseException`.
    #[error("SaltParseException: {0}")]
    SaltParse(&'static str),
    /// `BCrypt.Net.HashInformationException`.
    #[error("HashInformationException: Error handling string interrogation")]
    HashInformation,
    /// `System.ArgumentException` (an empty salt, or a work factor below 4).
    #[error("ArgumentException: {0}")]
    Argument(&'static str),
    /// `System.ArgumentOutOfRangeException` (a salt too short for its 22 characters, or a bad
    /// `GenerateSalt` work factor).
    #[error("ArgumentOutOfRangeException: {0}")]
    ArgumentOutOfRange(&'static str),
    /// `System.IndexOutOfRangeException` (a salt shorter than its prefix, or an empty key).
    #[error("IndexOutOfRangeException")]
    IndexOutOfRange,
    /// `System.FormatException` (non-numeric work factor digits).
    #[error("FormatException: {0}")]
    Format(&'static str),
}

impl BCryptError {
    /// The full name of the .NET exception type.
    #[must_use]
    pub fn dotnet_type(&self) -> &'static str {
        match self {
            Self::SaltParse(_) => "BCrypt.Net.SaltParseException",
            Self::HashInformation => "BCrypt.Net.HashInformationException",
            Self::Argument(_) => "System.ArgumentException",
            Self::ArgumentOutOfRange(_) => "System.ArgumentOutOfRangeException",
            Self::IndexOutOfRange => "System.IndexOutOfRangeException",
            Self::Format(_) => "System.FormatException",
        }
    }
}

/// `BCrypt.InterrogateHash`'s result (`HashInformation`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashInformation {
    /// Everything before the last `$`: `$2y$10`.
    pub settings: String,
    /// `2y` (or `2` for a hash without a revision letter).
    pub version: String,
    /// The two work-factor digits.
    pub work_factor: String,
    /// The salt and hash after the last `$`.
    pub raw_hash: String,
}

/// Blowfish state for one bcrypt computation.
struct Blowfish {
    p: [u32; 18],
    s: [[u32; 256]; 4],
}

impl Blowfish {
    fn new() -> Self {
        Self {
            p: P_INIT,
            s: S_INIT,
        }
    }

    #[inline]
    fn f(&self, x: u32) -> u32 {
        let [a, b, c, d] = x.to_be_bytes();
        (self.s[0][usize::from(a)].wrapping_add(self.s[1][usize::from(b)])
            ^ self.s[2][usize::from(c)])
        .wrapping_add(self.s[3][usize::from(d)])
    }

    /// Encrypts the 64-bit block `(l, r)`.
    #[inline]
    fn encipher(&self, l: &mut u32, r: &mut u32) {
        let mut xl = *l ^ self.p[0];
        let mut xr = *r;
        let mut i = 1;
        while i <= 16 {
            xr ^= self.f(xl) ^ self.p[i];
            xl ^= self.f(xr) ^ self.p[i + 1];
            i += 2;
        }
        *l = xr ^ self.p[17];
        *r = xl;
    }

    /// The next four bytes of `data`, big-endian, wrapping around at its end.
    #[inline]
    fn stream_to_word(data: &[u8], offset: &mut usize) -> u32 {
        let mut word = 0u32;
        for _ in 0..4 {
            word = (word << 8) | u32::from(data[*offset]);
            *offset = (*offset + 1) % data.len();
        }
        word
    }

    /// Standard Blowfish key schedule (`Key`).
    fn key(&mut self, key: &[u8]) {
        let mut koff = 0;
        for i in 0..18 {
            self.p[i] ^= Self::stream_to_word(key, &mut koff);
        }
        let (mut l, mut r) = (0u32, 0u32);
        for i in (0..18).step_by(2) {
            self.encipher(&mut l, &mut r);
            self.p[i] = l;
            self.p[i + 1] = r;
        }
        for sbox in 0..4 {
            for i in (0..256).step_by(2) {
                self.encipher(&mut l, &mut r);
                self.s[sbox][i] = l;
                self.s[sbox][i + 1] = r;
            }
        }
    }

    /// The salted key schedule (`EKSKey`).
    fn eks_key(&mut self, salt: &[u8], key: &[u8]) {
        let mut koff = 0;
        for i in 0..18 {
            self.p[i] ^= Self::stream_to_word(key, &mut koff);
        }
        let mut soff = 0;
        let (mut l, mut r) = (0u32, 0u32);
        for i in (0..18).step_by(2) {
            l ^= Self::stream_to_word(salt, &mut soff);
            r ^= Self::stream_to_word(salt, &mut soff);
            self.encipher(&mut l, &mut r);
            self.p[i] = l;
            self.p[i + 1] = r;
        }
        for sbox in 0..4 {
            for i in (0..256).step_by(2) {
                l ^= Self::stream_to_word(salt, &mut soff);
                r ^= Self::stream_to_word(salt, &mut soff);
                self.encipher(&mut l, &mut r);
                self.s[sbox][i] = l;
                self.s[sbox][i + 1] = r;
            }
        }
    }
}

/// `CryptRaw`: the 24-byte bcrypt output for a key, a 16-byte salt and a work factor.
fn crypt_raw(key: &[u8], salt: &[u8], work_factor: i32) -> Result<[u8; 24], BCryptError> {
    if salt.len() != BCRYPT_SALT_LEN {
        return Err(BCryptError::Argument("Bad salt length"));
    }
    if !(MIN_ROUNDS..=MAX_ROUNDS).contains(&work_factor) {
        return Err(BCryptError::Argument("Bad number of rounds"));
    }
    if key.is_empty() {
        // StreamToWord indexes the empty key array.
        return Err(BCryptError::IndexOutOfRange);
    }
    let rounds = 1u64 << work_factor;

    let mut bf = Blowfish::new();
    bf.eks_key(salt, key);
    for _ in 0..rounds {
        bf.key(key);
        bf.key(salt);
    }

    let mut cdata = BF_CRYPT_CIPHERTEXT;
    for _ in 0..64 {
        for j in 0..3 {
            let (a, b) = cdata.split_at_mut(2 * j + 1);
            bf.encipher(&mut a[2 * j], &mut b[0]);
        }
    }

    let mut out = [0u8; 24];
    for (i, word) in cdata.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    Ok(out)
}

/// `EncodeBase64`: the first `len` bytes of `data` in the bcrypt alphabet, unpadded.
#[must_use]
pub fn encode_base64(data: &[u8], len: usize) -> String {
    let mut out = String::with_capacity(len.div_ceil(3) * 4);
    let mut off = 0;
    let char_of = |v: u8| char::from(BASE64_CODE[usize::from(v & 0x3f)]);
    while off < len {
        let mut c1 = data[off];
        off += 1;
        out.push(char_of(c1 >> 2));
        c1 = (c1 & 0x03) << 4;
        if off >= len {
            out.push(char_of(c1));
            break;
        }
        let mut c2 = data[off];
        off += 1;
        c1 |= c2 >> 4;
        out.push(char_of(c1));
        c1 = (c2 & 0x0f) << 2;
        if off >= len {
            out.push(char_of(c1));
            break;
        }
        c2 = data[off];
        off += 1;
        c1 |= c2 >> 6;
        out.push(char_of(c1));
        out.push(char_of(c2));
    }
    out
}

fn char64(c: u8) -> Option<u8> {
    BASE64_CODE
        .iter()
        .position(|&x| x == c)
        .and_then(|p| u8::try_from(p).ok())
}

/// `DecodeBase64`: `max_len` bytes from bcrypt-base64 text. Decoding stops at the first invalid
/// character and the rest of the array stays zero, as in `BCrypt.Net-Next`.
#[must_use]
pub fn decode_base64(text: &[char], max_len: usize) -> Vec<u8> {
    let c64 = |c: char| u8::try_from(c).ok().and_then(char64);
    let mut out = vec![0u8; max_len];
    let mut olen = 0;
    let mut off = 0;
    while off + 1 < text.len() && olen < max_len {
        let c1 = c64(text[off]);
        let c2 = c64(text[off + 1]);
        off += 2;
        let (Some(c1), Some(c2)) = (c1, c2) else {
            break;
        };
        out[olen] = (c1 << 2) | ((c2 & 0x30) >> 4);
        olen += 1;
        if olen >= max_len || off >= text.len() {
            break;
        }
        let Some(c3) = c64(text[off]) else { break };
        off += 1;
        out[olen] = ((c2 & 0x0f) << 4) | ((c3 & 0x3c) >> 2);
        olen += 1;
        if olen >= max_len || off >= text.len() {
            break;
        }
        let c4 = c64(text[off]).unwrap_or(0xff);
        off += 1;
        out[olen] = ((c3 & 0x03) << 6) | c4;
        olen += 1;
    }
    out
}

/// `Convert.ToInt32(string)`: `NumberStyles.Integer` (surrounding whitespace, a leading sign).
fn parse_int(s: &str) -> Option<i32> {
    let t = s.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\u{b}' | '\u{c}' | '\r'));
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let v: i64 = digits.parse().ok()?;
    i32::try_from(if neg { -v } else { v }).ok()
}

/// `BCrypt.HashPassword(inputKey, salt)`: hashes `input` with the settings in `salt`
/// (`$2y$10$` and 22 salt characters; a whole stored hash also works).
///
/// # Errors
/// The exception `BCrypt.Net-Next` throws for a malformed salt (see [`BCryptError`]).
pub fn hash_password(input: &str, salt: &str) -> Result<String, BCryptError> {
    if salt.is_empty() {
        return Err(BCryptError::Argument(
            "Invalid salt: salt cannot be null or empty",
        ));
    }
    let sc: Vec<char> = salt.chars().collect();
    let at = |i: usize| sc.get(i).copied().ok_or(BCryptError::IndexOutOfRange);

    if at(0)? != '$' || at(1)? != '2' {
        return Err(BCryptError::SaltParse("Invalid salt version"));
    }
    // (char)0 when the salt has no revision letter; it is still written into the result.
    let mut minor = '\0';
    let starting_offset = if at(2)? == '$' {
        3
    } else {
        minor = at(2)?;
        if !matches!(minor, 'a' | 'b' | 'x' | 'y') || at(3)? != '$' {
            return Err(BCryptError::SaltParse("Invalid salt revision"));
        }
        4
    };

    if at(starting_offset + 2)? > '$' {
        return Err(BCryptError::SaltParse("Missing salt rounds"));
    }
    let rounds: String = sc[starting_offset..starting_offset + 2].iter().collect();
    let work_factor = parse_int(&rounds).ok_or(BCryptError::Format(
        "Input string was not in a correct format.",
    ))?;
    if !(1..=31).contains(&work_factor) {
        return Err(BCryptError::SaltParse("Salt rounds out of range"));
    }

    let extracted_salt = sc
        .get(starting_offset + 3..starting_offset + 3 + 22)
        .ok_or(BCryptError::ArgumentOutOfRange("length"))?;

    let mut input_bytes = input.as_bytes().to_vec();
    if minor >= 'a' {
        input_bytes.push(0);
    }
    let salt_bytes = decode_base64(extracted_salt, BCRYPT_SALT_LEN);
    let hashed = crypt_raw(&input_bytes, &salt_bytes, work_factor)?;

    let mut result = String::with_capacity(60);
    result.push_str("$2");
    result.push(minor);
    result.push('$');
    result.push_str(&format!("{work_factor:02}"));
    result.push('$');
    result.push_str(&encode_base64(&salt_bytes, salt_bytes.len()));
    result.push_str(&encode_base64(&hashed, BF_CRYPT_CIPHERTEXT.len() * 4 - 1));
    Ok(result)
}

/// `BCrypt.GenerateSalt(workFactor, bcryptMinorRevision)` with the 16 salt bytes supplied.
///
/// # Errors
/// A work factor outside 4..=31 (`ArgumentOutOfRangeException`).
pub fn generate_salt_from(
    work_factor: i32,
    minor: char,
    salt: &[u8; BCRYPT_SALT_LEN],
) -> Result<String, BCryptError> {
    if !(MIN_ROUNDS..=MAX_ROUNDS).contains(&work_factor) {
        return Err(BCryptError::ArgumentOutOfRange("workFactor"));
    }
    Ok(format!(
        "$2{minor}${work_factor:02}${}",
        encode_base64(salt, salt.len())
    ))
}

/// `BCrypt.GenerateSalt(workFactor, bcryptMinorRevision)`: 16 bytes from the operating system's
/// cryptographic random source (`RandomNumberGenerator` in .NET).
///
/// # Errors
/// A work factor outside 4..=31.
///
/// # Panics
/// If the operating system's random source fails (.NET throws `CryptographicException`).
pub fn generate_salt(work_factor: i32, minor: char) -> Result<String, BCryptError> {
    let mut salt = [0u8; BCRYPT_SALT_LEN];
    getrandom::fill(&mut salt).expect("CryptographicException: the OS random source failed");
    generate_salt_from(work_factor, minor, &salt)
}

/// `BCrypt.Verify(text, hash)`: hashes `text` with `hash`'s settings and compares in constant time.
///
/// # Errors
/// A malformed `hash` (`SaltParseException`).
pub fn verify(text: &str, hash: &str) -> Result<bool, BCryptError> {
    let computed = hash_password(text, hash)?;
    Ok(secure_equals(hash.as_bytes(), computed.as_bytes()))
}

fn secure_equals(a: &[u8], b: &[u8]) -> bool {
    let mut diff = u32::from(a.len() != b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= u32::from(x ^ y);
    }
    diff == 0
}

/// `BCrypt.InterrogateHash(hash)`: splits a stored hash into its parts.
///
/// `BCrypt.Net-Next` 4.2.0 accepts a 59- or 60-character string of the form
/// `$2[abxy]?$DD$[./A-Za-z0-9]+` (established by probing the shipped assembly; the probe cases
/// are in the `store/bcrypt_provider_get_password_work_factor` vectors).
///
/// # Errors
/// Anything else (`HashInformationException`).
pub fn interrogate_hash(hash: &str) -> Result<HashInformation, BCryptError> {
    let hc: Vec<char> = hash.chars().collect();
    if hc.len() != 59 && hc.len() != 60 {
        return Err(BCryptError::HashInformation);
    }
    if hc[0] != '$' || hc[1] != '2' {
        return Err(BCryptError::HashInformation);
    }
    let version_end = if matches!(hc[2], 'a' | 'b' | 'x' | 'y') {
        3
    } else {
        2
    };
    if hc[version_end] != '$' {
        return Err(BCryptError::HashInformation);
    }
    let wf = &hc[version_end + 1..version_end + 3];
    if !wf.iter().all(char::is_ascii_digit) || hc[version_end + 3] != '$' {
        return Err(BCryptError::HashInformation);
    }
    let body = &hc[version_end + 4..];
    if !body
        .iter()
        .all(|&c| c == '.' || c == '/' || c.is_ascii_alphanumeric())
    {
        return Err(BCryptError::HashInformation);
    }
    Ok(HashInformation {
        settings: hc[..version_end + 3].iter().collect(),
        version: hc[1..version_end].iter().collect(),
        work_factor: wf.iter().collect(),
        raw_hash: body.iter().collect(),
    })
}
