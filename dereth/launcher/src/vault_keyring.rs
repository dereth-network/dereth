//! Passwords in the system's secret store: the login Keychain on macOS, Credential Manager on
//! Windows, the desktop's keyring (the Secret Service: GNOME Keyring, KWallet) on Linux.
//!
//! One entry per account per world: the service is the launcher's target name
//! (`dereth:world/<slug>/<account>`) and the account is the account name, so Keychain Access (or
//! Credential Manager's Windows Credentials) shows each one plainly and the player can delete it
//! there.

use dereth_launch::vault::{Vault, VaultError};

#[derive(Debug, Default)]
pub struct SystemVault;

fn entry(target: &str, username: &str) -> Result<keyring::Entry, VaultError> {
    keyring::Entry::new(target, username).map_err(|e| VaultError(format!("the secret store: {e}")))
}

/// The account part of a target, which the store needs to find the entry again.
fn username_of(target: &str) -> &str {
    target.rsplit('/').next().unwrap_or(target)
}

impl Vault for SystemVault {
    fn get(&self, target: &str) -> Result<Option<String>, VaultError> {
        match entry(target, username_of(target))?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(VaultError(format!(
                "The secret store could not read a password: {e}"
            ))),
        }
    }

    fn set(&mut self, target: &str, _username: &str, secret: &str) -> Result<(), VaultError> {
        // Keyed by the lower-cased name in the target, so a later lookup finds it however the
        // account was typed.
        entry(target, username_of(target))?
            .set_password(secret)
            .map_err(|e| VaultError(format!("The secret store could not keep a password: {e}")))
    }

    fn delete(&mut self, target: &str) -> Result<(), VaultError> {
        match entry(target, username_of(target))?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(VaultError(format!(
                "The secret store could not delete a password: {e}"
            ))),
        }
    }

    /// The store cannot be searched by prefix through this interface; the backend forgets
    /// passwords account by account instead.
    fn list(&self, _prefix: &str) -> Result<Vec<String>, VaultError> {
        Ok(Vec::new())
    }
}
