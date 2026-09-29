// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Auth/AccountExtensions.cs
//! `AccountExtensions`: password checks (bcrypt, or the legacy salted SHA512 that migrates to
//! bcrypt on the next good login), last-login and unban.
//!
//! ACE reads the account configuration from `ConfigManager` and saves through the global
//! `DatabaseManager.Authentication`; here both come from the [`AuthDatabase`] passed in (or the
//! configuration directly, for `SetPassword`).

use std::net::IpAddr;

use empyrean_common::account_defaults::AccountDefaults;
use sha2::{Digest, Sha512};

use crate::authentication_database::{address_bytes, AuthDatabase};
use crate::bcrypt_provider::BCryptProvider;

use super::Account;

/// The salt value that marks a bcrypt password (`passwordSalt` is otherwise unused).
pub const USE_BCRYPT: &str = "use bcrypt";

// ACE: AccountExtensions
impl Account {
    // ACE: AccountExtensions.PasswordMatches
    /// Whether `password` is this account's password. A bcrypt hash whose work factor differs from
    /// the configured one (with `ForceWorkFactorMigration`), and a legacy SHA512 hash, are rehashed
    /// with bcrypt and saved when the password matches.
    ///
    /// # Panics
    /// When a legacy salt is not base64 (`Convert.FromBase64String` throws `FormatException`), or a
    /// bcrypt hash is malformed, as ACE throws.
    pub fn password_matches(&mut self, password: &str, auth: &mut dyn AuthDatabase) -> bool {
        let config = auth.accounts_config().clone();

        if self.password_salt == USE_BCRYPT {
            // Account password is using bcrypt
            if config.force_work_factor_migration
                && (BCryptProvider::get_password_work_factor(&self.password_hash)
                    != config.password_hash_work_factor)
            {
                // Upgrade (or downgrade) Password workfactor if not the same as config specifies, ForceWorkFactorMigration is TRUE and Password Matches
                if BCryptProvider::verify(password, &self.password_hash) {
                    self.set_password(password, &config);
                    self.set_salt_for_bcrypt();

                    auth.update_account(self);

                    return true;
                }
                return false;
            }
            return BCryptProvider::verify(password, &self.password_hash);
        }

        // Account password is using SHA512 salt
        log::debug!(
            "{} password verified using SHA512 hash/salt, migrating to bcrypt.",
            self.account_name
        );

        let input = get_password_hash_legacy(self, password);

        if input == self.password_hash {
            // If password matches, migrate to bcrypt
            self.set_password(password, &config);
            self.set_salt_for_bcrypt();

            auth.update_account(self);

            return true;
        }
        false
    }

    // ACE: AccountExtensions.SetPassword
    /// Sets a bcrypt hash of `value` at the configured work factor.
    pub fn set_password(&mut self, value: &str, config: &AccountDefaults) {
        self.password_hash = get_password_hash(value, config);
    }

    // ACE: AccountExtensions.SetSalt
    pub fn set_salt(&mut self, value: &str) {
        value.clone_into(&mut self.password_salt);
    }

    // ACE: AccountExtensions.SetSaltForBCrypt
    /// Marks the password as bcrypt (for migration purposes only).
    pub fn set_salt_for_bcrypt(&mut self) {
        self.set_salt(USE_BCRYPT); // this is used just to indicate that the password is using bcrypt. For migration purposes only.
    }

    // ACE: AccountExtensions.UpdateLastLogin
    /// Records a login from `address` now and saves the account.
    pub fn update_last_login(&mut self, address: IpAddr, auth: &mut dyn AuthDatabase) {
        self.last_login_ip = Some(address_bytes(address));
        self.last_login_time = Some(auth.clock().utc_now());
        self.total_times_logged_in = self.total_times_logged_in.wrapping_add(1);

        auth.update_account(self);
    }

    // ACE: AccountExtensions.UnBan
    /// Clears the ban and saves the account.
    pub fn un_ban(&mut self, auth: &mut dyn AuthDatabase) {
        self.ban_expire_time = None;
        self.banned_by_account_id = None;
        self.banned_time = None;
        self.ban_reason = None;

        auth.update_account(self);
    }
}

// ACE: AccountExtensions.GetPasswordHash
/// A bcrypt hash of `password` at `PasswordHashWorkFactor`, clamped to 4..=31.
#[must_use]
pub fn get_password_hash(password: &str, config: &AccountDefaults) -> String {
    let mut work_factor = config.password_hash_work_factor;

    if work_factor < 4 {
        log::warn!("PasswordHashWorkFactor in config less than minimum value of 4, using 4 and continuing.");
        work_factor = 4;
    } else if work_factor > 31 {
        log::warn!("PasswordHashWorkFactor in config greater than maximum value of 31, using 31 and continuing.");
        work_factor = 31;
    }

    BCryptProvider::hash_password(password, work_factor)
}

// ACE: AccountExtensions.GetPasswordHash
/// The legacy hash: `Base64(SHA512(UTF8(password) ++ FromBase64(account.PasswordSalt)))`.
///
/// # Panics
/// When the salt is not valid base64 (`FormatException`).
#[must_use]
pub fn get_password_hash_legacy(account: &Account, password: &str) -> String {
    let password_bytes = password.as_bytes();
    let salt_bytes = base64_decode(&account.password_salt)
        .expect("FormatException: The input is not a valid Base-64 string");
    let mut buffer = password_bytes.to_vec();
    buffer.extend_from_slice(&salt_bytes);

    let hash = Sha512::digest(&buffer);

    base64_encode(&hash)
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `Convert.ToBase64String`.
#[must_use]
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let sextet = |shift: u32| char::from(B64[((n >> shift) & 0x3f) as usize]);
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { '=' });
    }
    out
}

/// `Convert.FromBase64String`: whitespace ignored, length a multiple of 4, at most two trailing
/// `=`. `None` where .NET throws `FormatException`.
#[must_use]
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let chars: Vec<u8> = text
        .bytes()
        .filter(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        .collect();
    if !chars.len().is_multiple_of(4) {
        return None;
    }
    let pad = chars.iter().rev().take_while(|&&c| c == b'=').count();
    if pad > 2 {
        return None;
    }
    let mut out = Vec::with_capacity(chars.len() / 4 * 3);
    for (q, quad) in chars.chunks(4).enumerate() {
        let last = q == chars.len() / 4 - 1;
        let mut n = 0u32;
        for (i, &c) in quad.iter().enumerate() {
            let v = if c == b'=' {
                if !last || i < 4 - pad {
                    return None;
                }
                0
            } else {
                u32::try_from(B64.iter().position(|&x| x == c)?).ok()?
            };
            n = (n << 6) | v;
        }
        let bytes = n.to_be_bytes();
        let take = if last { 3 - pad } else { 3 };
        out.extend_from_slice(&bytes[1..=take]);
    }
    Some(out)
}
