//! Where passwords live: the operating system's secret store, and nowhere else.
//!
//! The launcher's own files never hold a password (see [`crate::state`]). The Windows front end
//! keeps them in Credential Manager, where the player can see and delete them; a macOS or Linux
//! front end would use the Keychain or libsecret through the same trait.
//!
//! One file of the launcher's own holds passwords: the ones earlier launchers made for accounts on
//! worlds of Asheron's Call 2's on a system with no secret store (a Linux desktop with no keyring),
//! the player's alone ([`SecretFile`]). They are read so those accounts keep playing, and are shown
//! to the player when asked; no launcher makes a password any more.

use std::collections::BTreeMap;
use std::path::PathBuf;

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

/// Passwords earlier launchers made themselves, in a file of their own: where an Asheron's Call 2
/// account's password was kept on a system with no secret store. Never a password the player
/// typed. On Unix the file is readable and writable by its owner alone.
#[derive(Debug, Clone)]
pub struct SecretFile {
    path: PathBuf,
}

impl SecretFile {
    /// The secrets kept in the file at `path`, which is made when the first is kept.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Every entry; none when the file is missing or unreadable.
    fn entries(&self) -> BTreeMap<String, String> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Write `entries` in place of the file, by a new file renamed over it, made the owner's
    /// alone before anything is written into it; no file at all when there are none.
    fn write(&self, entries: &BTreeMap<String, String>) -> Result<(), VaultError> {
        let failed = |e: std::io::Error| {
            VaultError(format!(
                "The launcher could not keep a secret in {}: {e}",
                self.path.display()
            ))
        };
        if entries.is_empty() {
            return match std::fs::remove_file(&self.path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(failed(e)),
                _ => Ok(()),
            };
        }
        let bytes = serde_json::to_vec_pretty(entries).map_err(|e| VaultError(e.to_string()))?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(failed)?;
        }
        let new = self.path.with_extension("new");
        let _ = std::fs::remove_file(&new);
        let mut open = std::fs::OpenOptions::new();
        open.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
        {
            let mut file = open.open(&new).map_err(failed)?;
            std::io::Write::write_all(&mut file, &bytes).map_err(failed)?;
            file.sync_all().map_err(failed)?;
        }
        std::fs::rename(&new, &self.path).map_err(failed)
    }
}

impl Vault for SecretFile {
    fn get(&self, target: &str) -> Result<Option<String>, VaultError> {
        Ok(self.entries().remove(target))
    }

    fn set(&mut self, target: &str, _username: &str, secret: &str) -> Result<(), VaultError> {
        let mut all = self.entries();
        all.insert(target.to_owned(), secret.to_owned());
        self.write(&all)
    }

    fn delete(&mut self, target: &str) -> Result<(), VaultError> {
        let mut all = self.entries();
        if all.remove(target).is_none() {
            return Ok(());
        }
        self.write(&all)
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, VaultError> {
        Ok(self
            .entries()
            .into_keys()
            .filter(|k| k.starts_with(prefix))
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

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dereth-vault-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_secret_kept_in_the_file_is_there_for_the_next_launcher_until_it_is_forgotten() {
        let d = tmp("file");
        let path = d.join("auberean").join("secrets.json");
        let mut first = SecretFile::new(&path);
        assert_eq!(first.get(&target("local", "player")).unwrap(), None);
        first.delete(&target("local", "player")).unwrap();
        assert!(!path.exists(), "nothing kept, no file");
        first
            .set(&target("local", "player"), "player", "s3cret")
            .unwrap();
        first.set(&target("local", "alt"), "alt", "other").unwrap();
        let mut next = SecretFile::new(&path);
        assert_eq!(
            next.get(&target("local", "PLAYER")).unwrap().as_deref(),
            Some("s3cret")
        );
        assert_eq!(next.list(TARGET_PREFIX).unwrap().len(), 2);
        next.delete(&target("local", "player")).unwrap();
        assert_eq!(first.get(&target("local", "player")).unwrap(), None);
        assert_eq!(next.forget_all().unwrap(), 1);
        assert!(!path.exists(), "every secret forgotten, no file");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn the_secrets_file_is_its_owners_alone() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("mode");
        let path = d.join("secrets.json");
        let mut file = SecretFile::new(&path);
        file.set(&target("local", "player"), "player", "s3cret")
            .unwrap();
        file.set(&target("local", "alt"), "alt", "other").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
