//! Plugins the launcher installs and updates from a release URL, for the Dereth client.
//!
//! A plugin is installed from the URL of its package's zip, published with its SHA-256 beside it
//! (`<url>.sha256`, the `sha256sum` line) or given by the player. There is no index: the player
//! brings the URL, and an update fetches the same URL again (a release's "latest" link, say).
//!
//! What was fetched is checked before anything is kept: the zip's SHA-256 against the published
//! one, the package's manifest, and for an update the same plugin id. A new release is **staged**:
//! kept beside the installed one and not offered to the client until the player accepts it. The
//! permissions its manifest names are shown, as what the plugin says it uses.
//!
//! Plugins are experimental, and installing one never turns it on. On disk, under the launcher's
//! data folder: `plugins/plugins.json` (the list) and one folder per plugin,
//! `plugins/<id>/current.zip` and `staged.zip`. The client is offered each installed plugin's
//! `current.zip` (`--offer-plugins`), and lists it off until the player ticks it there. The
//! fetching is the caller's: this module takes bytes.

use std::path::{Path, PathBuf};

use dereth_plugin_package::{source, Package};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The largest zip the launcher fetches: a package unpacks to at most 64 MiB, and Deflate does not
/// grow it.
pub const MAX_DOWNLOAD: usize = 64 << 20;

/// The list's file name in the plugins folder.
pub const LIST_FILE: &str = "plugins.json";

/// One release of a plugin: what its package said, and its zip's SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub name: String,
    pub version: String,
    /// The zip's SHA-256, lower-case hex.
    pub sha256: String,
    /// The permissions its manifest names: what it says it uses.
    pub permissions: Vec<String>,
}

/// One installed plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    pub id: String,
    /// Where its releases are fetched from.
    pub url: String,
    /// The release the client is offered; none until the player accepts the first.
    pub current: Option<Release>,
    /// A release fetched and checked, waiting for the player to accept it.
    pub staged: Option<Release>,
}

/// What fetching a release came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// A release was staged; the client is offered it once the player accepts it.
    Staged { id: String, version: String },
    /// The URL serves the release already installed (or already staged).
    UpToDate { id: String },
}

/// Why a release was not kept, or a change not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// No SHA-256 was published beside the zip, and the player gave none.
    NoHash,
    /// The zip is not the file its published SHA-256 names.
    HashMismatch { expected: String, got: String },
    /// Larger than [`MAX_DOWNLOAD`].
    TooLarge,
    /// Not a package: not a zip, no manifest, or a manifest that does not read.
    NotAPackage(String),
    /// The URL now serves another plugin.
    OtherPlugin { expected: String, got: String },
    /// No plugin by that id is installed.
    NotInstalled(String),
    /// Nothing is staged for it.
    NothingStaged(String),
    /// Reading or writing the plugins folder failed.
    Io(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHash => f.write_str(
                "no SHA-256 was published beside the zip (<url>.sha256); give the release's hash",
            ),
            Self::HashMismatch { expected, got } => write!(
                f,
                "the download's SHA-256 is {got}, not the published {expected}"
            ),
            Self::TooLarge => write!(f, "the download is larger than {} MiB", MAX_DOWNLOAD >> 20),
            Self::NotAPackage(e) => write!(f, "not a plugin package: {e}"),
            Self::OtherPlugin { expected, got } => {
                write!(f, "that URL now serves plugin {got}, not {expected}")
            }
            Self::NotInstalled(id) => write!(f, "no plugin {id} is installed"),
            Self::NothingStaged(id) => write!(f, "no new release of {id} is waiting"),
            Self::Io(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for PluginError {}

/// The SHA-256 in a published sum: the first word of a `sha256sum` line, or the hash alone.
#[must_use]
pub fn parse_sha256(text: &str) -> Option<String> {
    let word = text.split_whitespace().next()?;
    (word.len() == 64 && word.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| word.to_ascii_lowercase())
}

/// Where a release's published SHA-256 is: beside the zip.
#[must_use]
pub fn sum_url(url: &str) -> String {
    format!("{url}.sha256")
}

fn sha256_hex(bytes: &[u8]) -> String {
    source::hex(&Sha256::digest(bytes))
}

/// The launcher's installed plugins: the list and the folder that holds their zips.
#[derive(Debug, Clone)]
pub struct Shelf {
    dir: PathBuf,
    pub plugins: Vec<Installed>,
}

fn io(what: &str, path: &Path, e: &std::io::Error) -> PluginError {
    PluginError::Io(format!("{what} {}: {e}", path.display()))
}

/// A plugin id as a folder name: ids are dotted names, but a list edited by hand is not trusted.
fn folder_name(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl Shelf {
    /// The shelf in `dir` (the launcher's `plugins` data folder); empty when it has no list yet.
    /// A list that does not read is treated as empty, and replaced at the next change.
    #[must_use]
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let plugins = std::fs::read(dir.join(LIST_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { dir, plugins }
    }

    fn save(&self) -> Result<(), PluginError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| io("making", &self.dir, &e))?;
        let path = self.dir.join(LIST_FILE);
        let text =
            serde_json::to_vec_pretty(&self.plugins).map_err(|e| PluginError::Io(e.to_string()))?;
        crate::state::write_atomic(&path, &text).map_err(|e| io("writing", &path, &e))
    }

    /// The folder of plugin `id`'s zips.
    #[must_use]
    pub fn plugin_dir(&self, id: &str) -> PathBuf {
        self.dir.join(folder_name(id))
    }

    /// The installed plugin `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Installed> {
        self.plugins.iter().find(|p| p.id == id)
    }

    fn index(&self, id: &str) -> Result<usize, PluginError> {
        self.plugins
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| PluginError::NotInstalled(id.to_owned()))
    }

    /// The zips the client is offered, each off there until the player ticks it: each installed
    /// plugin's current release.
    #[must_use]
    pub fn offered_packages(&self) -> Vec<PathBuf> {
        self.plugins
            .iter()
            .filter(|p| p.current.is_some())
            .map(|p| self.plugin_dir(&p.id).join("current.zip"))
            .filter(|f| f.is_file())
            .collect()
    }

    /// Check a release fetched from `url` (`zip`, and `published` the SHA-256 published for it or
    /// given by the player) and stage it: a new plugin is added with no release offered yet, an
    /// installed one keeps its current release until the player accepts the new one. `expect_id`
    /// is the plugin an update is for.
    ///
    /// # Errors
    /// [`PluginError`]: the release is not kept.
    pub fn stage(
        &mut self,
        url: &str,
        zip: &[u8],
        published: Option<&str>,
        expect_id: Option<&str>,
    ) -> Result<Fetched, PluginError> {
        if zip.len() > MAX_DOWNLOAD {
            return Err(PluginError::TooLarge);
        }
        let expected = published
            .and_then(parse_sha256)
            .ok_or(PluginError::NoHash)?;
        let got = sha256_hex(zip);
        if got != expected {
            return Err(PluginError::HashMismatch { expected, got });
        }
        let package =
            Package::from_zip(url, zip).map_err(|e| PluginError::NotAPackage(e.to_string()))?;
        let manifest = package
            .manifest()
            .map_err(|e| PluginError::NotAPackage(e.to_string()))?;
        let id = manifest.id.to_string();
        if let Some(want) = expect_id.filter(|w| *w != id) {
            return Err(PluginError::OtherPlugin {
                expected: want.to_owned(),
                got: id,
            });
        }
        let release = Release {
            name: manifest.name.clone(),
            version: manifest.version.to_string(),
            sha256: got,
            permissions: manifest
                .permissions
                .keys()
                .map(ToString::to_string)
                .collect(),
        };
        let existing = self.plugins.iter().position(|p| p.id == id);
        if let Some(i) = existing {
            let p = &self.plugins[i];
            let same = |r: &Option<Release>| r.as_ref().is_some_and(|r| r.sha256 == release.sha256);
            if same(&p.current) || same(&p.staged) {
                return Ok(Fetched::UpToDate { id });
            }
        }
        let dir = self.plugin_dir(&id);
        std::fs::create_dir_all(&dir).map_err(|e| io("making", &dir, &e))?;
        let staged = dir.join("staged.zip");
        crate::state::write_atomic(&staged, zip).map_err(|e| io("writing", &staged, &e))?;
        let version = release.version.clone();
        match existing {
            Some(i) => {
                self.plugins[i].staged = Some(release);
                url.clone_into(&mut self.plugins[i].url);
            }
            None => self.plugins.push(Installed {
                id: id.clone(),
                url: url.to_owned(),
                current: None,
                staged: Some(release),
            }),
        }
        self.save()?;
        Ok(Fetched::Staged { id, version })
    }

    /// Make the staged release of `id` the current one, the one the client is offered. It turns
    /// nothing on: the player ticks the plugin in the client.
    ///
    /// # Errors
    /// Not installed, nothing staged, or the folder could not be written.
    pub fn accept_staged(&mut self, id: &str) -> Result<(), PluginError> {
        let i = self.index(id)?;
        if self.plugins[i].staged.is_none() {
            return Err(PluginError::NothingStaged(id.to_owned()));
        }
        let dir = self.plugin_dir(id);
        let (from, to) = (dir.join("staged.zip"), dir.join("current.zip"));
        std::fs::rename(&from, &to).map_err(|e| io("moving", &from, &e))?;
        let p = &mut self.plugins[i];
        p.current = p.staged.take();
        self.save()
    }

    /// Drop the staged release of `id`.
    ///
    /// # Errors
    /// Not installed, or the folder could not be written.
    pub fn discard_staged(&mut self, id: &str) -> Result<(), PluginError> {
        let i = self.index(id)?;
        let _ = std::fs::remove_file(self.plugin_dir(id).join("staged.zip"));
        self.plugins[i].staged = None;
        if self.plugins[i].current.is_none() {
            return self.remove(id);
        }
        self.save()
    }

    /// Remove `id` and its zips.
    ///
    /// # Errors
    /// Not installed, or the folder could not be written.
    pub fn remove(&mut self, id: &str) -> Result<(), PluginError> {
        let i = self.index(id)?;
        let dir = self.plugin_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| io("removing", &dir, &e))?;
        }
        self.plugins.remove(i);
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_sha256, sha256_hex, Fetched, PluginError, Shelf};
    use dereth_plugin_package::Package;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dereth-launch-plugins-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn package(version: &str, permissions: &[&str]) -> Package {
        let mut manifest = format!(
            "id = \"dev.example.meter\"\nname = \"Meter\"\nversion = \"{version}\"\napi = \"0.4\"\n"
        );
        if !permissions.is_empty() {
            manifest.push_str("[permissions]\n");
            for p in permissions {
                manifest.push_str(&format!("\"{p}\" = \"why\"\n"));
            }
        }
        Package {
            origin: String::new(),
            files: [
                ("plugin.toml".to_owned(), manifest.into_bytes()),
                ("main.lua".to_owned(), b"print(1)".to_vec()),
            ]
            .into(),
        }
    }

    const URL: &str = "https://example.org/meter/latest/meter.zip";

    /// Behaviour: none (the launcher's own plumbing: checking and staging a fetched release).
    #[test]
    fn a_release_is_kept_only_when_its_published_hash_matches_and_it_is_a_package() {
        let dir = scratch("hash");
        let mut shelf = Shelf::open(&dir);
        let zip = package("1.0.0", &[]).to_zip();
        let sum = format!("{}  meter.zip\n", sha256_hex(&zip));
        assert_eq!(
            parse_sha256(&sum).as_deref(),
            Some(sha256_hex(&zip).as_str())
        );
        assert_eq!(shelf.stage(URL, &zip, None, None), Err(PluginError::NoHash));
        let wrong = "0".repeat(64);
        assert!(matches!(
            shelf.stage(URL, &zip, Some(&wrong), None),
            Err(PluginError::HashMismatch { .. })
        ));
        let junk = b"not a zip".to_vec();
        assert!(matches!(
            shelf.stage(URL, &junk, Some(&sha256_hex(&junk)), None),
            Err(PluginError::NotAPackage(_))
        ));
        assert!(shelf.plugins.is_empty());
        assert_eq!(
            shelf.stage(URL, &zip, Some(&sum), None),
            Ok(Fetched::Staged {
                id: "dev.example.meter".into(),
                version: "1.0.0".into()
            })
        );
        // Staged: the client is offered nothing until the player accepts it.
        assert!(shelf.offered_packages().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Behaviour: none (the launcher's own plumbing: a package in the client's own namespace is
    /// not kept).
    #[test]
    fn a_package_whose_id_is_in_the_clients_own_namespace_is_not_installed() {
        let dir = scratch("reserved");
        let mut shelf = Shelf::open(&dir);
        let mut p = package("1.0.0", &[]);
        let manifest = String::from_utf8(p.files["plugin.toml"].clone())
            .unwrap()
            .replace("dev.example.meter", "dereth.plugins");
        p.files.insert("plugin.toml".into(), manifest.into_bytes());
        let zip = p.to_zip();
        let refused = shelf.stage(URL, &zip, Some(&sha256_hex(&zip)), None);
        assert!(
            matches!(&refused, Err(PluginError::NotAPackage(e)) if e.contains("kept for the client")),
            "{refused:?}"
        );
        assert!(shelf.plugins.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Behaviour: none (the launcher's own plumbing: an update waits beside the release in use
    /// until the player accepts it, whatever it says it uses).
    #[test]
    fn an_update_waits_beside_the_current_release_until_it_is_accepted() {
        let dir = scratch("update");
        let mut shelf = Shelf::open(&dir);
        let v1 = package("1.0.0", &["chat.filter"]).to_zip();
        shelf
            .stage(URL, &v1, Some(&sha256_hex(&v1)), None)
            .expect("staged");
        assert_eq!(
            shelf.plugins[0]
                .staged
                .as_ref()
                .map(|r| r.permissions.clone()),
            Some(vec!["chat.filter".to_owned()]),
            "what it says it uses is shown"
        );
        shelf.accept_staged("dev.example.meter").expect("accepted");
        let current = shelf.offered_packages();
        assert_eq!(current.len(), 1);
        assert_eq!(std::fs::read(&current[0]).expect("zip"), v1);
        // The same release again: nothing to do.
        assert_eq!(
            shelf.stage(URL, &v1, Some(&sha256_hex(&v1)), Some("dev.example.meter")),
            Ok(Fetched::UpToDate {
                id: "dev.example.meter".into()
            })
        );
        // A new release using more: staged, the old one still offered until it is accepted.
        let v2 = package("1.1.0", &["chat.filter", "auto.loot"]).to_zip();
        shelf
            .stage(URL, &v2, Some(&sha256_hex(&v2)), Some("dev.example.meter"))
            .expect("staged");
        assert_eq!(
            std::fs::read(&shelf.offered_packages()[0]).expect("zip"),
            v1
        );
        shelf.accept_staged("dev.example.meter").expect("accepted");
        assert_eq!(
            std::fs::read(&shelf.offered_packages()[0]).expect("zip"),
            v2
        );
        // Kept across a reopen; removed, the client is offered nothing.
        let mut again = Shelf::open(&dir);
        assert_eq!(again.plugins, shelf.plugins);
        again.remove("dev.example.meter").expect("removed");
        assert!(again.offered_packages().is_empty());
        assert!(Shelf::open(&dir).plugins.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Behaviour: none (the launcher's own plumbing: an update must be the same plugin; a package
    /// carrying a `plugin.sig` file is read as any other).
    #[test]
    fn an_update_of_another_plugin_is_refused_and_a_signature_file_is_just_a_file() {
        let dir = scratch("other");
        let mut shelf = Shelf::open(&dir);
        let v1 = package("1.0.0", &[]).to_zip();
        shelf
            .stage(URL, &v1, Some(&sha256_hex(&v1)), None)
            .expect("staged");
        shelf.accept_staged("dev.example.meter").expect("accepted");
        let v2 = package("1.1.0", &[]).to_zip();
        assert!(matches!(
            shelf.stage(URL, &v2, Some(&sha256_hex(&v2)), Some("dev.example.other")),
            Err(PluginError::OtherPlugin { .. })
        ));
        let mut with_sig = package("1.1.0", &[]);
        with_sig.files.insert(
            "plugin.sig".into(),
            b"key = \"00\"\nsignature = \"00\"\n".to_vec(),
        );
        let with_sig = with_sig.to_zip();
        assert!(matches!(
            shelf.stage(
                URL,
                &with_sig,
                Some(&sha256_hex(&with_sig)),
                Some("dev.example.meter")
            ),
            Ok(Fetched::Staged { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
