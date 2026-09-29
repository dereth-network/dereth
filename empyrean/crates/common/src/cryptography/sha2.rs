// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Cryptography/SHA2.cs
//! `ACE.Common.Cryptography.SHA2`: a lowercase hex SHA-256 or SHA-512 digest of a string's UTF-8
//! bytes. Nothing in ACE calls it; it is ported for completeness.

use ::sha2::{Digest, Sha256, Sha512};
use std::fmt::Write as _;

// ACE: SHA2Type
/// `SHA2Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sha2Type {
    /// `SHA256`.
    Sha256,
    /// `SHA512`.
    Sha512,
}

// ACE: SHA2.Hash
/// `BitConverter.ToString(digest).Replace("-", "").ToLower()` of the digest of `data`'s UTF-8
/// bytes. ACE's `default: return ""` arm and its exception handler cannot be reached here (the
/// enum is closed and hashing does not fail).
#[must_use]
pub fn hash(ty: Sha2Type, data: &str) -> String {
    let buffer = data.as_bytes();

    let digest: Vec<u8> = match ty {
        Sha2Type::Sha256 => Sha256::digest(buffer).to_vec(),
        Sha2Type::Sha512 => Sha512::digest(buffer).to_vec(),
    };

    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}
