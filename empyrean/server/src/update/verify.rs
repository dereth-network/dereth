//! Checking a download before anything is installed from it: the release's `SHA256SUMS` names
//! every archive, `release.json` and the source, and a download whose SHA-256 is not the one
//! listed (or is not listed) is refused, as is an archive whose SHA-256 differs from the one
//! `release.json` gives. Nothing is signed: the checksums come from the same place as the files, so
//! they catch a damaged or altered download, not a release published by someone else.

use sha2::{Digest, Sha256};

/// A file's SHA-256, lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The lines of a `SHA256SUMS` file: (SHA-256, file name).
#[must_use]
pub fn parse_sums(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let (sum, name) = l.trim().split_once(char::is_whitespace)?;
            let name = name.trim_start().trim_start_matches('*');
            (sum.len() == 64 && sum.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| (sum.to_ascii_lowercase(), name.to_owned()))
        })
        .collect()
}

/// Checks `bytes`, downloaded as `name`, against `sums`. Its SHA-256.
///
/// # Errors
/// When `name` is not listed or its SHA-256 differs.
pub fn check_sum(name: &str, bytes: &[u8], sums: &[(String, String)]) -> Result<String, String> {
    let actual = sha256_hex(bytes);
    match sums.iter().find(|(_, n)| n == name) {
        None => Err(format!("{name} is not listed in SHA256SUMS: refused")),
        Some((expected, _)) if *expected != actual => Err(format!(
            "{name}: SHA-256 {actual} is not the {expected} SHA256SUMS lists: refused (a damaged or altered download)"
        )),
        Some(_) => Ok(actual),
    }
}
