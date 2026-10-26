//! A compiled plugin's source hash: what the author's tool writes into a module it builds, so a
//! module can be checked against the sources it claims to come from.

use sha2::{Digest, Sha256};

/// The custom section a compiled plugin's module carries its source hash in.
pub const SOURCE_SECTION: &str = "dereth-source-sha256";

/// Lower-case hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The SHA-256 of a compiled plugin's source files (by `/`-separated path relative to its crate):
/// the tag `dereth-plugin-source-v1` and a zero byte, then each file in path order as its path, a
/// zero byte, its length as eight little-endian bytes and its bytes.
#[must_use]
pub fn source_hash(files: &std::collections::BTreeMap<String, Vec<u8>>) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"dereth-plugin-source-v1");
    h.update([0]);
    for (path, bytes) in files {
        h.update(path.as_bytes());
        h.update([0]);
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    }
    h.finalize().into()
}
