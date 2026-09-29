//! Where passwords live: the operating system's secret store, and nowhere else.
//!
//! The launcher's own files never hold a password (see [`crate::state`]). The Windows front end
//! keeps them in Credential Manager, where the player can see and delete them; a macOS or Linux
//! front end would use the Keychain or libsecret through the same trait.

/// The entry name for one account on one world: `dereth:world/<slug>/<username>`. The account name
/// is lower-cased, as the client does, so one account is one entry however it was typed.
pub fn target(world_slug: &str, username: &str) -> String {
    format!("dereth:world/{world_slug}/{}", username.to_lowercase())
}

/// The prefix every entry this launcher writes starts with, for "Forget all passwords".
pub const TARGET_PREFIX: &str = "dereth:world/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultError(pub String);

impl core::fmt::Display for VaultError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for VaultError {}

/// A secret store.
pub trait Vault: core::fmt::Debug {
    fn get(&self, target: &str) -> Result<Option<String>, VaultError>;
    fn set(&mut self, target: &str, username: &str, secret: &str) -> Result<(), VaultError>;
    /// Deleting an entry that is not there is not an error.
    fn delete(&mut self, target: &str) -> Result<(), VaultError>;
    /// The targets under a prefix.
    fn list(&self, prefix: &str) -> Result<Vec<String>, VaultError>;

    /// Delete every entry this launcher wrote.
    fn forget_all(&mut self) -> Result<usize, VaultError> {
        let all = self.list(TARGET_PREFIX)?;
        for t in &all {
            self.delete(t)?;
        }
        Ok(all.len())
    }
}

/// A vault that forgets everything when dropped. For tests, and for a front end with no store at
/// all, where "remember" simply cannot be offered.
#[derive(Debug, Default, Clone)]
pub struct MemoryVault {
    entries: std::collections::BTreeMap<String, String>,
}

impl Vault for MemoryVault {
    fn get(&self, target: &str) -> Result<Option<String>, VaultError> {
        Ok(self.entries.get(target).cloned())
    }

    fn set(&mut self, target: &str, _username: &str, secret: &str) -> Result<(), VaultError> {
        self.entries.insert(target.to_owned(), secret.to_owned());
        Ok(())
    }

    fn delete(&mut self, target: &str) -> Result<(), VaultError> {
        self.entries.remove(target);
        Ok(())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, VaultError> {
        Ok(self
            .entries
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_account_is_one_entry_however_it_is_typed() {
        assert_eq!(target("eulmore", "Player"), "dereth:world/eulmore/player");
        assert_eq!(target("eulmore", "PLAYER"), target("eulmore", "player"));
        assert_ne!(target("eulmore", "player"), target("achard", "player"));
    }

    #[test]
    fn forget_all_takes_only_our_entries() {
        let mut v = MemoryVault::default();
        v.set(&target("a", "x"), "x", "1").unwrap();
        v.set(&target("b", "y"), "y", "2").unwrap();
        v.set("someone-else", "z", "3").unwrap();
        assert_eq!(v.forget_all().unwrap(), 2);
        assert_eq!(v.get("someone-else").unwrap().as_deref(), Some("3"));
        v.delete("not-there").unwrap();
    }
}
