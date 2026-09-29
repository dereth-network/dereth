// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Cryptography/BCryptProvider.cs
//! `BCryptProvider`: ACE's wrapper over `BCrypt.Net-Next` (here [`crate::bcrypt`]).
//!
//! It lives in empyrean-store rather than empyrean-common (where ACE keeps it) because its only
//! callers are the account extensions.

use crate::bcrypt;

// ACE: BCryptProvider
/// `BCryptProvider` (a static class in ACE).
#[derive(Debug)]
pub struct BCryptProvider;

impl BCryptProvider {
    // ACE: BCryptProvider.HashPassword
    /// Hashes `input` with a fresh random salt at `work_factor` (ACE's default is 10), as `$2y$`.
    ///
    /// # Panics
    /// `ArgumentOutOfRangeException` when `work_factor` is outside 4..=31, as `GenerateSalt` throws
    /// (ACE's caller clamps it first).
    #[must_use]
    pub fn hash_password(input: &str, work_factor: i32) -> String {
        // Force BCrypt.Net-Next to use 2y instead of the default 2a
        // The older bcrypt package ACE used (BCrypt.Net-Core) defaultd to 2y
        let salt = bcrypt::generate_salt(work_factor, 'y').unwrap_or_else(|e| panic!("{e}"));

        bcrypt::hash_password(input, &salt).unwrap_or_else(|e| panic!("{e}"))
    }

    // ACE: BCryptProvider.Verify
    /// Whether `text` hashes to `hash`.
    ///
    /// # Panics
    /// `SaltParseException` when `hash` is not a bcrypt hash, as `BCrypt.Verify` throws.
    #[must_use]
    pub fn verify(text: &str, hash: &str) -> bool {
        bcrypt::verify(text, hash).unwrap_or_else(|e| panic!("{e}"))
    }

    // ACE: BCryptProvider.GetPasswordWorkFactor
    /// The work factor recorded in `hash`, or 0 when its two work-factor characters are not a number.
    ///
    /// # Panics
    /// `HashInformationException` when `hash` is too short to interrogate.
    #[must_use]
    pub fn get_password_work_factor(hash: &str) -> i32 {
        let hash_information = bcrypt::interrogate_hash(hash).unwrap_or_else(|e| panic!("{e}"));

        int_try_parse(&hash_information.work_factor).unwrap_or(0)
    }
}

/// `int.TryParse(s, out var v)` with the default `NumberStyles.Integer`: optional surrounding
/// whitespace and a leading sign, then ASCII digits.
fn int_try_parse(s: &str) -> Option<i32> {
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
