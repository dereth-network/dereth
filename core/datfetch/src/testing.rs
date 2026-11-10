//! A file kept as the community archive's host keeps one, for tests of the steps of getting a set
//! (this crate's, and a front end's that runs them): encrypted, with its link and the API's answer.

use aes::cipher::{BlockEncrypt, KeyInit};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;

use crate::mega::{FileKey, FileRef};

/// One archive as the host keeps it: its handle, its link's key and its encrypted bytes.
#[derive(Debug)]
pub struct Hosted {
    pub handle: String,
    pub aes: [u8; 16],
    pub key: FileKey,
    pub cipher: Vec<u8>,
    pub name: String,
}

/// Encrypt `plain` as the host keeps a file: a key whose MAC is the file's.
pub fn host(handle: &str, name: &str, plain: &[u8], seed: u8) -> Hosted {
    let aes: [u8; 16] =
        std::array::from_fn(|i| u8::try_from(i).unwrap().wrapping_mul(31).wrapping_add(seed));
    let nonce: [u8; 8] =
        std::array::from_fn(|i| u8::try_from(i).unwrap().wrapping_mul(17).wrapping_add(seed));
    let link = |k: &FileKey| format!("https://mega.nz/file/{handle}#{}", k.to_text());
    let draft = FileRef::parse(&link(&FileKey::compose(aes, nonce, [0; 8])), None).unwrap();
    let mut mac = draft.mac();
    mac.update(plain);
    let key = FileKey::compose(aes, nonce, mac.finish());
    let file = FileRef::parse(&link(&key), None).unwrap();
    let mut cipher = plain.to_vec();
    file.decryptor().apply(0, &mut cipher);
    Hosted {
        handle: handle.into(),
        aes,
        key,
        cipher,
        name: name.into(),
    }
}

impl Hosted {
    pub fn link(&self) -> String {
        format!(
            "https://mega.nz/file/{}#{}",
            self.handle,
            self.key.to_text()
        )
    }

    /// The file's attributes as the host keeps them: `MEGA{"n":name}`, AES-CBC from zero.
    pub fn attributes(&self) -> String {
        let aes = aes::Aes128::new(&self.aes.into());
        let mut plain = format!("MEGA{{\"n\":\"{}\"}}", self.name).into_bytes();
        plain.resize(plain.len().div_ceil(16) * 16, 0);
        let mut prev = [0u8; 16];
        for chunk in plain.as_chunks_mut::<16>().0 {
            for (a, b) in chunk.iter_mut().zip(prev) {
                *a ^= b;
            }
            let mut block = (*chunk).into();
            aes.encrypt_block(&mut block);
            chunk.copy_from_slice(&block);
            prev = *chunk;
        }
        B64.encode(plain)
    }

    /// The host's API answer for this file, its download at `url`.
    #[must_use]
    pub fn api_reply(&self, url: &str) -> String {
        format!(
            r#"[{{"s":{},"at":"{}","g":"{url}"}}]"#,
            self.cipher.len(),
            self.attributes()
        )
    }
}
