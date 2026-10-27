//! Plugins the launcher installs and updates for the Dereth client, from plugin sources.
//!
//! A **source** is one list, `plugins.json` at an `https://` address, naming many plugins, each at
//! its latest release, with where its zip is and the zip's SHA-256
//! ([`dereth_plugin_package::catalog`]). The launcher starts with one, Dereth's own
//! ([`FIRST_PARTY`]), and nothing installed; the player adds others by their address, once the
//! page has said that Dereth does not review them. The plugins every source lists are offered
//! together, each saying which source lists it, and the player installs each one by itself.
//!
//! What a source lists is checked before anything is kept: the zip must be the file the entry
//! names and its manifest must say what the entry says ([`Entry::check`]), the client must run
//! the API version it asks for, and an id under `network.dereth.` installs only from Dereth's own
//! source. An installed plugin updates only from the source it came from, and only to a release
//! that takes precedence over the one installed. Updates are applied when the player asks, or on
//! their own as the setting says ([`AutoUpdate`]: none, all, or only Dereth's own); a release that
//! asks for a permission or a web domain the installed one did not never installs on its own.
//!
//! Plugins are experimental, and installing one never turns it on. Where a shelf is kept is a
//! [`Keep`]: two records and each installed plugin's zip. The launcher's is its data folder
//! ([`Folder`]): `plugins/sources.json` (the sources, the last list each gave, and the settings),
//! `plugins/installed.json` (the installed plugins) and one folder per plugin holding its
//! `current.zip`. The client is offered each installed plugin's zip (`--offer-plugins`), and lists
//! it off until the player ticks it there. The web page keeps the same shelf in the browser, with
//! a keep of its own. The fetching is the caller's: this module takes bytes, and reading every
//! source then applying the updates that apply on their own is a walk the caller drives as it
//! fetches ([`Refresh`]).
//!
//! Beside the sources, the sources' file keeps a list of **folders on this computer**
//! ([`Shelf::local`]): each one plugin's folder or a plugin repository, whose plugins are the
//! folders directly in it, as the author's tool finds a repository's
//! ([`dereth_plugin_template::packages_in`]); nothing deeper is looked at. Their plugins are offered
//! to the client as they are on disk, with no zip, no check against a source and no updates
//! ([`Shelf::local_plugins`]). One with the id of an installed plugin is offered in its place;
//! the installed one keeps updating. Taking a folder off the list deletes nothing.
//!
//! A folder the launcher's Make Your Own wrote in is the author's own, and the launcher keeps a
//! copy of the author's tool it ships with at its root ([`Shelf::tool_folders`], [`keep_tool`]),
//! with the lines that keep the copy out of git in its `.gitignore`. The copy is written again
//! whenever it is not the tool the launcher ships, which after an update it is not. A folder added
//! any other way gets no copy, and taking a folder off the list stops the copying and leaves the
//! copy where it is.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use dereth_plugin_api::manifest::PluginVersion;
use dereth_plugin_api::{ApiVersion, Manifest};
use dereth_plugin_package::catalog::{self, Catalog, Entry, Mismatch};
use serde::{Deserialize, Serialize};

/// Dereth's own source: the plugins that come from Dereth, on their own host. The one source the
/// launcher starts with.
pub const FIRST_PARTY: &str = catalog::DERETH_SOURCE;

/// The ids that install only from [`FIRST_PARTY`]: Dereth's own plugins.
pub const FIRST_PARTY_PREFIX: &str = "network.dereth.";

/// The largest zip the launcher fetches when its source does not say its size: a package unpacks
/// to at most 64 MiB, and Deflate does not grow it.
pub const MAX_DOWNLOAD: usize = 64 << 20;

/// The largest source's list the launcher reads.
pub const MAX_LIST: usize = catalog::MAX_BYTES;

/// The sources' file in the plugins folder.
pub const SOURCES_FILE: &str = "sources.json";

/// The installed plugins' file in the plugins folder.
pub const INSTALLED_FILE: &str = "installed.json";

/// The list an earlier launcher kept, of plugins installed from a zip's address. Plugins come from
/// sources now: it is dropped, with what it installed, the first time the folder is opened.
const OLD_LIST_FILE: &str = "plugins.json";

/// The installed zip's name in a plugin's folder.
const CURRENT_ZIP: &str = "current.zip";

/// Whether plugin `id` installs only from Dereth's own source.
#[must_use]
pub fn first_party_only(id: &str) -> bool {
    id.starts_with(FIRST_PARTY_PREFIX)
}

/// Which plugins update without the player. Whatever it says, the player is told of the updates
/// that wait, and an update that asks for more waits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoUpdate {
    /// None: every update waits for the player.
    None,
    /// Every plugin updates on its own.
    All,
    /// The plugins from Dereth's own source update on their own; the others wait. The setting at
    /// first.
    #[default]
    FirstParty,
}

impl AutoUpdate {
    /// The setting by its name (`none`, `all`, `first_party`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "all" => Some(Self::All),
            "first_party" => Some(Self::FirstParty),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for AutoUpdate {
    /// Any value reads: a setting's name as that setting, anything else (an earlier launcher's
    /// `off` or `notify`) as the setting at first. So a record holding one still reads, with its
    /// sources.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        Ok(value.as_str().and_then(Self::parse).unwrap_or_default())
    }
}

/// One source, and the last list it gave.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub url: String,
    /// When its list was last read whole, in seconds since the epoch.
    #[serde(default)]
    pub read_at: Option<u64>,
    /// Why the last read failed, until a read succeeds; the last list read stays in use.
    #[serde(default)]
    pub error: Option<String>,
    /// The last list read whole, as its text.
    #[serde(default)]
    list: Option<String>,
    /// That list, read.
    #[serde(skip)]
    pub catalog: Catalog,
    /// The entries of that list that did not read, one line each.
    #[serde(skip)]
    pub skipped: Vec<String>,
}

impl Source {
    fn new(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            ..Self::default()
        }
    }

    /// Whether this is Dereth's own source.
    #[must_use]
    pub fn first_party(&self) -> bool {
        self.url == FIRST_PARTY
    }

    /// What the player calls it: the name its list gives, else its address's host.
    #[must_use]
    pub fn label(&self) -> String {
        if !self.catalog.name.is_empty() {
            return self.catalog.name.clone();
        }
        if self.first_party() {
            return "Dereth".to_owned();
        }
        host_of(&self.url).to_owned()
    }

    fn take_list(&mut self, text: String) {
        match Catalog::parse(text.as_bytes()) {
            Ok(read) => {
                self.catalog = read.catalog;
                self.skipped = read.skipped;
                self.list = Some(text);
            }
            Err(_) => self.list = None,
        }
    }
}

/// The host of an `https://` address, or the address itself.
fn host_of(url: &str) -> &str {
    url.strip_prefix("https://")
        .and_then(|r| r.split(['/', '?', '#']).next())
        .unwrap_or(url)
}

/// The release of a plugin that is installed: what its package said, its zip's SHA-256 and size,
/// and what its source said of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub name: String,
    pub version: String,
    /// The API version it needs.
    pub api: String,
    /// The zip's SHA-256, lower-case hex.
    pub sha256: String,
    /// The permissions its manifest names: what it says it uses.
    pub permissions: Vec<String>,
    /// The web domains its manifest names.
    #[serde(default)]
    pub domains: Vec<String>,
    /// What its source said of it: its description, authors and homepage, the day it was
    /// released and what changed in it. Empty in a record kept before these were.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub released: String,
    #[serde(default)]
    pub notes: String,
    /// The zip's size in bytes.
    #[serde(default)]
    pub size: Option<u64>,
}

/// One installed plugin. It updates on its own only as the setting says ([`AutoUpdate`]); an
/// earlier launcher's choice for the one plugin is ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    pub id: String,
    /// The source it came from, and the only one it updates from.
    pub source: String,
    pub release: Release,
}

/// A download to make before installing: where, and at most how many bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub url: String,
    pub limit: usize,
}

/// What an install or an update came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Whether it replaced an installed release.
    pub updated: bool,
}

impl Done {
    /// What it came to, in words for the page.
    #[must_use]
    pub fn said(&self) -> String {
        if self.updated {
            format!("Updated {} to {}.", self.name, self.version)
        } else {
            format!(
                "Installed {} {}. Turn it on in the client.",
                self.name, self.version
            )
        }
    }
}

/// A newer release of an installed plugin, in its own source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pending {
    pub id: String,
    pub name: String,
    pub from: String,
    pub to: String,
    pub notes: String,
    /// The day it was released; empty when its source does not say.
    pub released: String,
    /// Its zip's size in bytes, when its source says it.
    pub size: Option<u64>,
    /// It needs a newer API version than this client has: listed, not installable.
    pub needs_newer_dereth: bool,
    /// Permissions it asks for that the installed release did not.
    pub new_permissions: Vec<String>,
    /// Web domains it names that the installed release did not.
    pub new_domains: Vec<String>,
    /// Whether it is applied without the player: the setting allows it, and it asks for nothing
    /// new.
    pub automatic: bool,
}

/// Why something was not done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// A source's address must be `https://`.
    NotHttps(String),
    AlreadyAdded(String),
    /// Dereth's own source stays.
    FirstPartyStays,
    NoSource(String),
    /// The source does not list the plugin.
    NotListed {
        source: String,
        id: String,
    },
    /// The release needs a newer API version than this client has.
    NeedsNewerDereth {
        name: String,
        api: String,
    },
    /// An id under `network.dereth.` from another source than Dereth's.
    FirstPartyOnly(String),
    /// Installed from another source.
    FromAnotherSource {
        id: String,
        source: String,
    },
    /// Nothing newer than the release installed.
    UpToDate {
        name: String,
        version: String,
    },
    /// The entry's zip address is not one the launcher downloads from.
    BadZipUrl(String),
    /// The download is not the release its source lists.
    Mismatch {
        name: String,
        why: Mismatch,
    },
    NotInstalled(String),
    /// A folder on this computer that is neither a plugin's folder nor a plugin repository: it has
    /// no `plugin.toml`, and no folder directly in it has one.
    NoPluginIn(String),
    /// A folder already on the list of folders on this computer.
    FolderListed(String),
    /// A folder not on that list.
    FolderNotListed(String),
    /// Reading or writing the plugins folder failed.
    Io(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotHttps(u) => write!(f, "{u} is not an https:// address"),
            Self::AlreadyAdded(u) => write!(f, "{u} is already a source"),
            Self::FirstPartyStays => f.write_str("Dereth's own source cannot be removed"),
            Self::NoSource(u) => write!(f, "{u} is not one of your sources"),
            Self::NotListed { source, id } => write!(f, "{source} does not list {id}"),
            Self::NeedsNewerDereth { name, api } => {
                write!(f, "{name} needs a newer Dereth (plugin API {api})")
            }
            Self::FirstPartyOnly(id) => {
                write!(
                    f,
                    "{id} is Dereth's own and installs only from Dereth's source"
                )
            }
            Self::FromAnotherSource { id, source } => write!(
                f,
                "{id} is installed from {source}; remove it first to install it from here"
            ),
            Self::UpToDate { name, version } => write!(f, "{name} {version} is up to date"),
            Self::BadZipUrl(z) => write!(
                f,
                "the source names its zip as {z:?}, not an https:// address"
            ),
            Self::Mismatch { name, why } => write!(f, "{name} was not installed: {why}"),
            Self::NotInstalled(id) => write!(f, "no plugin {id} is installed"),
            Self::NoPluginIn(d) => write!(
                f,
                "{d} has no plugin.toml, and neither do the folders in it"
            ),
            Self::FolderListed(d) => write!(f, "{d} is already on the list"),
            Self::FolderNotListed(d) => write!(f, "{d} is not on the list"),
            Self::Io(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for PluginError {}

/// The sources' file. An earlier launcher's `others_allowed` in it is ignored, and goes at the
/// next write; a file from before the folders on this computer reads as having none, and one from
/// before the author's tool was kept in them as keeping it in none. The folders it is kept in are
/// a list of their own rather than a mark on each folder's entry, so a launcher from before still
/// reads the file whole.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SourcesFile {
    #[serde(default)]
    sources: Vec<Source>,
    #[serde(default)]
    auto_update: AutoUpdate,
    #[serde(default)]
    local: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_folders: Vec<PathBuf>,
}

/// Where a shelf keeps what it holds: its two records, [`SOURCES_FILE`] and [`INSTALLED_FILE`],
/// and each installed plugin's zip. The launcher keeps them in its data folder ([`Folder`]); a web
/// page, in the browser's storage.
pub trait Keep {
    /// The record `name` as last written; `None` when there is none.
    fn read(&self, name: &str) -> Option<Vec<u8>>;

    /// Keep `bytes` as the record `name`.
    ///
    /// # Errors
    /// It could not be kept.
    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<(), PluginError>;

    /// Keep `zip` as plugin `id`'s package, in place of the one before.
    ///
    /// # Errors
    /// It could not be kept.
    fn put_zip(&mut self, id: &str, zip: &[u8]) -> Result<(), PluginError>;

    /// Delete plugin `id`'s package; one that is not there is no error.
    ///
    /// # Errors
    /// It could not be deleted.
    fn remove_zip(&mut self, id: &str) -> Result<(), PluginError>;
}

/// The launcher's plugins folder: the records as files in it, and a folder per plugin holding its
/// `current.zip`.
#[derive(Debug, Clone)]
pub struct Folder {
    dir: PathBuf,
}

impl Folder {
    /// The plugins folder at `dir`.
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn plugin_dir(&self, id: &str) -> PathBuf {
        self.dir.join(folder_name(id))
    }
}

impl Keep for Folder {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.dir.join(name)).ok()
    }

    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<(), PluginError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| io("making", &self.dir, &e))?;
        let path = self.dir.join(name);
        crate::state::write_atomic(&path, bytes).map_err(|e| io("writing", &path, &e))
    }

    fn put_zip(&mut self, id: &str, zip: &[u8]) -> Result<(), PluginError> {
        let dir = self.plugin_dir(id);
        std::fs::create_dir_all(&dir).map_err(|e| io("making", &dir, &e))?;
        let path = dir.join(CURRENT_ZIP);
        crate::state::write_atomic(&path, zip).map_err(|e| io("writing", &path, &e))
    }

    fn remove_zip(&mut self, id: &str) -> Result<(), PluginError> {
        let dir = self.plugin_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| io("removing", &dir, &e))?;
        }
        Ok(())
    }
}

/// The plugins a host installs: the sources, the installed plugins, and where they are kept (the
/// launcher's folder unless said otherwise).
#[derive(Debug, Clone)]
pub struct Shelf<K = Folder> {
    /// Where the records and the zips are kept.
    pub keep: K,
    /// The API version of the client the plugins are for: a release needing a newer one is not
    /// installed.
    pub host: ApiVersion,
    /// Dereth's own first, then the player's, in the order added.
    pub sources: Vec<Source>,
    pub auto_update: AutoUpdate,
    pub plugins: Vec<Installed>,
    /// The folders on this computer whose plugins are offered as they are on disk, in the order
    /// added.
    pub local: Vec<PathBuf>,
    /// The folders of [`Shelf::local`] that Make Your Own wrote in, where the launcher keeps a copy
    /// of the author's tool ([`keep_tool`]), in the order added.
    pub tool_folders: Vec<PathBuf>,
    /// The folders of [`Shelf::tool_folders`] whose copy could not be put there when last tried.
    /// Not kept.
    pub tool_failed: Vec<PathBuf>,
}

fn io(what: &str, path: &Path, e: &std::io::Error) -> PluginError {
    PluginError::Io(format!("{what} {}: {e}", path.display()))
}

/// A record's bytes.
fn record(value: &impl Serialize) -> Result<Vec<u8>, PluginError> {
    serde_json::to_vec_pretty(value).map_err(|e| PluginError::Io(e.to_string()))
}

/// A plugin id as a file or folder name: ids are dotted names, but a list edited by hand is not
/// trusted.
#[must_use]
pub fn folder_name(id: &str) -> String {
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

/// The names in `now` that are not in `before`.
fn added(before: &[String], now: &[String]) -> Vec<String> {
    now.iter()
        .filter(|n| !before.contains(n))
        .cloned()
        .collect()
}

impl Shelf {
    /// The shelf in `dir` (the launcher's `plugins` data folder), for a client of API version
    /// [`ApiVersion::CURRENT`]: Dereth's own source and nothing installed when the folder holds
    /// nothing yet. A file that does not read is treated as empty and replaced at the next change.
    /// An earlier launcher's list is dropped.
    #[must_use]
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        let shelf = Self::read(dir);
        shelf.drop_old_list();
        shelf
    }

    /// The shelf in `dir` as [`Shelf::open`] finds it, changing nothing on disk: for a program
    /// other than the launcher that finds the plugins the launcher installed.
    #[must_use]
    pub fn read(dir: impl Into<PathBuf>) -> Self {
        Self::load(Folder::new(dir))
    }

    /// Drop an earlier launcher's list of plugins installed from a zip's address, and their
    /// folders (none of which this shelf holds).
    fn drop_old_list(&self) {
        let old = self.keep.dir.join(OLD_LIST_FILE);
        let Ok(bytes) = std::fs::read(&old) else {
            return;
        };
        #[derive(Deserialize)]
        struct OldInstalled {
            id: String,
        }
        let listed: Vec<OldInstalled> = serde_json::from_slice(&bytes).unwrap_or_default();
        for p in listed {
            if self.get(&p.id).is_none() {
                let _ = std::fs::remove_dir_all(self.plugin_dir(&p.id));
            }
        }
        let _ = std::fs::remove_file(old);
    }

    /// The folder of plugin `id`'s zip.
    #[must_use]
    pub fn plugin_dir(&self, id: &str) -> PathBuf {
        self.keep.plugin_dir(id)
    }

    /// What the client is offered, each off there until the player ticks it: the folders of the
    /// plugins on this computer, then the installed plugins' zips, less those whose id a plugin on
    /// this computer has.
    #[must_use]
    pub fn offered_packages(&self) -> Vec<PathBuf> {
        let local = self.local_plugins();
        let mut out: Vec<PathBuf> = local
            .iter()
            .filter(|p| p.loaded)
            .map(|p| p.folder.clone())
            .collect();
        out.extend(
            self.plugins
                .iter()
                .filter(|p| {
                    !local
                        .iter()
                        .any(|l| l.loaded && l.id() == Some(p.id.as_str()))
                })
                .map(|p| self.plugin_dir(&p.id).join(CURRENT_ZIP))
                .filter(|f| f.is_file()),
        );
        out
    }
}

/// A plugin in a folder on this computer, as the client is offered it.
#[derive(Debug, Clone)]
pub struct LocalPlugin {
    /// The folder on the list it was found in.
    pub listed: PathBuf,
    /// Its own folder, the one holding its `plugin.toml`.
    pub folder: PathBuf,
    /// Its manifest, or why it does not read.
    pub manifest: Result<Manifest, String>,
    /// Whether the client is offered it: not when a plugin found before it has its id.
    pub loaded: bool,
}

impl LocalPlugin {
    /// Its id, when its manifest reads.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.manifest.as_ref().ok().map(|m| m.id.as_str())
    }
}

/// The manifest in the plugin folder `folder`, read as the client reads it.
fn manifest_in(folder: &Path) -> Result<Manifest, String> {
    let path = folder.join(dereth_plugin_package::MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let table: toml::Table = toml::from_str(&text).map_err(|e| e.to_string())?;
    let doc = dereth_plugin_package::toml_to_value(&toml::Value::Table(table));
    Manifest::from_value(&doc).map_err(|e| e.to_string())
}

/// The plugin folders in `folder`, as the author's tool finds a repository's: the folder itself
/// when it is a plugin's, else the folders directly in it that are. None when it is not there or
/// cannot be read.
fn plugins_in(folder: &Path) -> Vec<PathBuf> {
    dereth_plugin_template::packages_in(folder).unwrap_or_default()
}

/// The author's tool's file name, as Dereth ships it beside the launcher on this system and as
/// the launcher keeps its copy in a plugin repository.
pub const TOOL_FILE: &str = if cfg!(windows) {
    "dereth-plugin.exe"
} else {
    "dereth-plugin"
};

/// What a Local row says when the tool's copy in its folder could not be brought up to date.
const TOOL_FAILED: &str = "Could not update dereth-plugin in this folder.";

/// Keep a copy of the author's tool at the root of the plugin repository `folder`: `bytes` as the
/// file `name`, executable, written in place of a copy that is not those bytes (an older
/// release's, or none), and the lines of [`dereth_plugin_template::TOOL_IGNORED`] its
/// `.gitignore` lacks added to it. A copy that is the tool already is left as it is, and so is
/// the `.gitignore`. Answers whether the copy was written.
///
/// # Errors
/// The copy or the `.gitignore` could not be written.
pub fn keep_tool(folder: &Path, name: &str, bytes: &[u8]) -> std::io::Result<bool> {
    let copy = folder.join(name);
    let same = std::fs::metadata(&copy).is_ok_and(|m| m.is_file() && m.len() == bytes.len() as u64)
        && std::fs::read(&copy).is_ok_and(|b| b == bytes);
    if same {
        return Ok(false);
    }
    // Written beside it and moved over it, so a copy is never left half written.
    let mut part = copy.clone().into_os_string();
    part.push(".part");
    let part = PathBuf::from(part);
    std::fs::write(&part, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755))?;
    }
    if let Err(e) = std::fs::rename(&part, &copy) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    ignore_tool(folder)?;
    Ok(true)
}

/// Add to the `.gitignore` in `folder` (made when there is none) the lines of
/// [`dereth_plugin_template::TOOL_IGNORED`] it lacks, and only those, in the file's own line
/// endings. A line it has with or without the leading `/` counts.
fn ignore_tool(folder: &Path) -> std::io::Result<()> {
    let path = folder.join(".gitignore");
    let mut text = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e),
    };
    let had: Vec<&[u8]> = text
        .split(|b| *b == b'\n')
        .map(<[u8]>::trim_ascii)
        .collect();
    let missing: Vec<&str> = dereth_plugin_template::TOOL_IGNORED
        .iter()
        .copied()
        .filter(|want| {
            let bare = want.trim_start_matches('/');
            !had.iter()
                .any(|h| *h == want.as_bytes() || *h == bare.as_bytes())
        })
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let eol: &[u8] = if text.windows(2).any(|w| w == b"\r\n") {
        b"\r\n"
    } else {
        b"\n"
    };
    if !text.is_empty() && !text.ends_with(b"\n") {
        text.extend_from_slice(eol);
    }
    for line in missing {
        text.extend_from_slice(line.as_bytes());
        text.extend_from_slice(eol);
    }
    std::fs::write(&path, text)
}

impl<K: Keep> Shelf<K> {
    /// The shelf `keep` holds, for a client of API version [`ApiVersion::CURRENT`]: Dereth's own
    /// source and nothing installed when it holds nothing yet. A record that does not read is
    /// treated as empty and replaced at the next change.
    #[must_use]
    pub fn load(keep: K) -> Self {
        let file: SourcesFile = keep
            .read(SOURCES_FILE)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let plugins: Vec<Installed> = keep
            .read(INSTALLED_FILE)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut sources = vec![Source::new(FIRST_PARTY)];
        for mut s in file.sources {
            if let Some(text) = s.list.take() {
                s.take_list(text);
            }
            match sources.iter_mut().find(|k| k.url == s.url) {
                Some(kept) => *kept = s,
                None => sources.push(s),
            }
        }
        let mut tool_folders = file.tool_folders;
        tool_folders.retain(|f| file.local.contains(f));
        Self {
            keep,
            host: ApiVersion::CURRENT,
            sources,
            auto_update: file.auto_update,
            plugins,
            local: file.local,
            tool_folders,
            tool_failed: Vec::new(),
        }
    }

    fn save_sources(&mut self) -> Result<(), PluginError> {
        let file = SourcesFile {
            sources: self.sources.clone(),
            auto_update: self.auto_update,
            local: self.local.clone(),
            tool_folders: self.tool_folders.clone(),
        };
        self.keep.write(SOURCES_FILE, &record(&file)?)
    }

    fn save_installed(&mut self) -> Result<(), PluginError> {
        self.keep.write(INSTALLED_FILE, &record(&self.plugins)?)
    }

    /// The installed plugin `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Installed> {
        self.plugins.iter().find(|p| p.id == id)
    }

    /// The source at `url`.
    #[must_use]
    pub fn source(&self, url: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.url == url)
    }

    fn source_mut(&mut self, url: &str) -> Result<&mut Source, PluginError> {
        self.sources
            .iter_mut()
            .find(|s| s.url == url)
            .ok_or_else(|| PluginError::NoSource(url.to_owned()))
    }

    // ----- sources -------------------------------------------------------------------------------

    /// Add the source at `url` (its list is read by the caller next, [`Shelf::record_read`]): the
    /// page has the player confirm it first. Answers the address as kept.
    ///
    /// # Errors
    /// Not an `https://` address, already a source, or the folder could not be written.
    pub fn add_source(&mut self, url: &str) -> Result<String, PluginError> {
        let url = url.trim();
        let ok = url.len() > 8
            && url[..8].eq_ignore_ascii_case("https://")
            && !host_of(url).is_empty()
            && !url.chars().any(char::is_whitespace);
        if !ok {
            return Err(PluginError::NotHttps(url.to_owned()));
        }
        let url = format!("https://{}", &url[8..]);
        if self.source(&url).is_some() {
            return Err(PluginError::AlreadyAdded(url));
        }
        self.sources.push(Source::new(&url));
        self.save_sources()?;
        Ok(url)
    }

    /// Remove the source at `url`. The plugins installed from it stay, with no updates.
    ///
    /// # Errors
    /// Dereth's own source, not a source, or the folder could not be written.
    pub fn remove_source(&mut self, url: &str) -> Result<(), PluginError> {
        if url == FIRST_PARTY {
            return Err(PluginError::FirstPartyStays);
        }
        let before = self.sources.len();
        self.sources.retain(|s| s.url != url);
        if self.sources.len() == before {
            return Err(PluginError::NoSource(url.to_owned()));
        }
        self.save_sources()
    }

    /// What reading the source at `url` came to, at `now` (seconds since the epoch): its list's
    /// bytes, or why they could not be fetched. A list that reads replaces the last; one that does
    /// not leaves the last in use and says why. Answers the names of the plugins the list names
    /// that the last list read did not: none the first time a source is read.
    ///
    /// # Errors
    /// Not a source, or the folder could not be written.
    pub fn record_read(
        &mut self,
        url: &str,
        fetched: Result<Vec<u8>, String>,
        now: u64,
    ) -> Result<Vec<String>, PluginError> {
        let source = self.source_mut(url)?;
        let read = fetched.and_then(|bytes| {
            let read = Catalog::parse(&bytes).map_err(|e| e.to_string())?;
            Ok((String::from_utf8_lossy(&bytes).into_owned(), read))
        });
        let mut new = Vec::new();
        match read {
            Ok((text, read)) => {
                if source.read_at.is_some() {
                    new = read
                        .catalog
                        .plugins
                        .iter()
                        .filter(|e| source.catalog.get(e.id.as_str()).is_none())
                        .map(|e| e.name.clone())
                        .collect();
                }
                source.catalog = read.catalog;
                source.skipped = read.skipped;
                source.list = Some(text);
                source.read_at = Some(now);
                source.error = None;
            }
            Err(e) => source.error = Some(e),
        }
        self.save_sources()?;
        Ok(new)
    }

    /// What reading the source at `url` came to, in words for the page, a line each: why its list
    /// could not be read, and the plugins it lists now that it did not (`new`, as
    /// [`Shelf::record_read`] answered).
    #[must_use]
    pub fn said_of_read(&self, url: &str, new: &[String]) -> Vec<String> {
        let (failed, news) = self.read_lines(url, new);
        failed.into_iter().chain(news).collect()
    }

    /// [`Shelf::said_of_read`]'s lines apart: why the list could not be read, and what is new.
    fn read_lines(&self, url: &str, new: &[String]) -> (Option<String>, Option<String>) {
        let Some(s) = self.source(url) else {
            return (None, None);
        };
        let failed = s.error.as_ref().map(|e| format!("{}: {e}", s.label()));
        let news =
            (!new.is_empty()).then(|| format!("New from {}: {}.", s.label(), new.join(", ")));
        (failed, news)
    }

    // ----- folders on this computer -----------------------------------------------------------------

    /// Add `folder`, one plugin's folder or a plugin repository, to the folders on this computer.
    /// Answers how many plugins it holds.
    ///
    /// # Errors
    /// It is neither a plugin's folder nor a plugin repository (a folder whose plugins are all
    /// further down is not one), it is on the list already, or the folder could not be written.
    pub fn add_local(&mut self, folder: &Path) -> Result<usize, PluginError> {
        let shown = || folder.display().to_string();
        if self.local.iter().any(|f| f == folder) {
            return Err(PluginError::FolderListed(shown()));
        }
        let found = plugins_in(folder).len();
        if found == 0 {
            return Err(PluginError::NoPluginIn(shown()));
        }
        self.local.push(folder.to_owned());
        self.save_sources()?;
        Ok(found)
    }

    /// Add `folder`, which Make Your Own wrote in, to the folders on this computer if it is not on
    /// the list, and to the folders the launcher keeps the author's tool in
    /// ([`Shelf::tool_folders`]). Answers how many plugins it holds.
    ///
    /// # Errors
    /// As [`Shelf::add_local`], less its being on the list already.
    pub fn add_own(&mut self, folder: &Path) -> Result<usize, PluginError> {
        let found = plugins_in(folder).len();
        if found == 0 {
            return Err(PluginError::NoPluginIn(folder.display().to_string()));
        }
        if !self.local.iter().any(|f| f == folder) {
            self.local.push(folder.to_owned());
        }
        if !self.tool_folders.iter().any(|f| f == folder) {
            self.tool_folders.push(folder.to_owned());
        }
        self.save_sources()?;
        Ok(found)
    }

    /// Take `folder` off the folders on this computer, and so off those the launcher keeps the
    /// author's tool in. Nothing in it is touched, the tool's copy included.
    ///
    /// # Errors
    /// It is not on the list, or the folder could not be written.
    pub fn remove_local(&mut self, folder: &Path) -> Result<(), PluginError> {
        let before = self.local.len();
        self.local.retain(|f| f != folder);
        if self.local.len() == before {
            return Err(PluginError::FolderNotListed(folder.display().to_string()));
        }
        self.tool_folders.retain(|f| f != folder);
        self.tool_failed.retain(|f| f != folder);
        self.save_sources()
    }

    /// The plugins in the folders on this computer, read from disk now, folder by folder in the
    /// list's order. Of two with one id, the first is loaded; a plugin found under two folders on
    /// the list is listed once.
    #[must_use]
    pub fn local_plugins(&self) -> Vec<LocalPlugin> {
        let mut out: Vec<LocalPlugin> = Vec::new();
        for listed in &self.local {
            for folder in plugins_in(listed) {
                if out.iter().any(|p| p.folder == folder) {
                    continue;
                }
                let manifest = manifest_in(&folder);
                let taken = manifest.as_ref().is_ok_and(|m| {
                    out.iter()
                        .any(|p| p.loaded && p.id() == Some(m.id.as_str()))
                });
                out.push(LocalPlugin {
                    listed: listed.clone(),
                    folder,
                    manifest,
                    loaded: !taken,
                });
            }
        }
        out
    }

    // ----- installing ------------------------------------------------------------------------------

    /// The entry `source` lists for `id`, checked for installing here: the client runs its API
    /// version, it is Dereth's own only from Dereth's source, and it is not installed from another
    /// source.
    fn installable(&self, source: &str, id: &str) -> Result<(&Source, &Entry), PluginError> {
        let s = self
            .source(source)
            .ok_or_else(|| PluginError::NoSource(source.to_owned()))?;
        let entry = s.catalog.get(id).ok_or_else(|| PluginError::NotListed {
            source: s.label(),
            id: id.to_owned(),
        })?;
        if let Some(p) = self.get(id).filter(|p| p.source != s.url) {
            let from = self
                .source(&p.source)
                .map_or_else(|| p.source.clone(), Source::label);
            return Err(PluginError::FromAnotherSource {
                id: id.to_owned(),
                source: from,
            });
        }
        entry_installable(entry, s.first_party(), self.host)?;
        Ok((s, entry))
    }

    /// What to download to install `id` from `source`, or to update it there: checked as
    /// [`Shelf::install`] checks it, before anything is downloaded.
    ///
    /// # Errors
    /// As [`Shelf::install`], less the download's own checks.
    pub fn plan_install(&self, source: &str, id: &str) -> Result<Download, PluginError> {
        let (s, entry) = self.installable(source, id)?;
        if let Some(p) = self.get(id) {
            let have = PluginVersion::parse(&p.release.version);
            if have.is_some_and(|v| !entry.version.is_newer_than(&v)) {
                return Err(PluginError::UpToDate {
                    name: p.release.name.clone(),
                    version: p.release.version.clone(),
                });
            }
        }
        let url = catalog::resolve(&s.url, &entry.zip)
            .ok_or_else(|| PluginError::BadZipUrl(entry.zip.clone()))?;
        Ok(Download {
            url,
            limit: limit_of(entry),
        })
    }

    /// Where the zip `source` lists for `id` is, as an `https://` address; `None` when the source
    /// does not list it, or names it otherwise.
    #[must_use]
    pub fn zip_of(&self, source: &str, id: &str) -> Option<String> {
        let s = self.source(source)?;
        catalog::resolve(&s.url, &s.catalog.get(id)?.zip)
    }

    /// What to download to update the installed plugin `id` from its own source.
    ///
    /// # Errors
    /// Not installed, its source is gone or no longer lists it, nothing newer, or as
    /// [`Shelf::plan_install`].
    pub fn plan_update(&self, id: &str) -> Result<Download, PluginError> {
        let p = self
            .get(id)
            .ok_or_else(|| PluginError::NotInstalled(id.to_owned()))?;
        self.plan_install(&p.source.clone(), id)
    }

    /// Install `id` from `source`, or update it there, from `zip`, the download
    /// [`Shelf::plan_install`] named: checked again as it was planned, and the zip against the
    /// entry. The client is offered it at its next start, off until the player ticks it.
    ///
    /// # Errors
    /// [`PluginError`]: the source, the entry or the zip refused it, or the folder could not be
    /// written. Nothing changes.
    pub fn install(&mut self, source: &str, id: &str, zip: &[u8]) -> Result<Done, PluginError> {
        self.plan_install(source, id)?;
        let (s, entry) = self.installable(source, id)?;
        let source_url = s.url.clone();
        let release = checked_release(entry, zip)?;
        self.keep.put_zip(id, zip)?;
        let done = Done {
            id: id.to_owned(),
            name: release.name.clone(),
            version: release.version.clone(),
            updated: self.get(id).is_some(),
        };
        match self.plugins.iter_mut().find(|p| p.id == id) {
            Some(p) => p.release = release,
            None => self.plugins.push(Installed {
                id: id.to_owned(),
                source: source_url,
                release,
            }),
        }
        self.save_installed()?;
        Ok(done)
    }

    /// Update the installed plugin `id` from its own source with `zip`, the download
    /// [`Shelf::plan_update`] named: [`Shelf::install`] from that source.
    ///
    /// # Errors
    /// Not installed, or as [`Shelf::install`].
    pub fn update(&mut self, id: &str, zip: &[u8]) -> Result<Done, PluginError> {
        let source = self
            .get(id)
            .ok_or_else(|| PluginError::NotInstalled(id.to_owned()))?
            .source
            .clone();
        self.install(&source, id, zip)
    }

    /// Remove `id` and its zip.
    ///
    /// # Errors
    /// Not installed, or the folder could not be written.
    pub fn remove(&mut self, id: &str) -> Result<(), PluginError> {
        let i = self
            .plugins
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| PluginError::NotInstalled(id.to_owned()))?;
        self.keep.remove_zip(id)?;
        self.plugins.remove(i);
        self.save_installed()
    }

    // ----- updates ---------------------------------------------------------------------------------

    /// The auto-update setting.
    ///
    /// # Errors
    /// The folder could not be written.
    pub fn set_auto_update(&mut self, setting: AutoUpdate) -> Result<(), PluginError> {
        self.auto_update = setting;
        self.save_sources()
    }

    /// Whether the setting lets `p` update on its own.
    fn updates_on_its_own(&self, p: &Installed) -> bool {
        match self.auto_update {
            AutoUpdate::None => false,
            AutoUpdate::FirstParty => p.source == FIRST_PARTY,
            AutoUpdate::All => true,
        }
    }

    /// The newer release waiting for each installed plugin in its own source.
    #[must_use]
    pub fn pending(&self) -> Vec<Pending> {
        let mut out = Vec::new();
        for p in &self.plugins {
            let Some(entry) = self.source(&p.source).and_then(|s| s.catalog.get(&p.id)) else {
                continue;
            };
            let have = PluginVersion::parse(&p.release.version);
            if !have.is_some_and(|v| entry.version.is_newer_than(&v)) {
                continue;
            }
            let needs_newer_dereth = !entry.runs_on(self.host);
            let new_permissions = added(&p.release.permissions, &entry.permissions);
            let new_domains = added(&p.release.domains, &entry.domains);
            let automatic = !needs_newer_dereth
                && new_permissions.is_empty()
                && new_domains.is_empty()
                && self.updates_on_its_own(p);
            out.push(Pending {
                id: p.id.clone(),
                name: entry.name.clone(),
                from: p.release.version.clone(),
                to: entry.version.to_string(),
                notes: entry.notes.clone(),
                released: entry.released.clone(),
                size: entry.size,
                needs_newer_dereth,
                new_permissions,
                new_domains,
                automatic,
            });
        }
        out
    }

    /// The ids whose waiting release is applied without the player ([`Pending::automatic`]).
    #[must_use]
    pub fn automatic_updates(&self) -> Vec<String> {
        self.pending()
            .into_iter()
            .filter(|p| p.automatic)
            .map(|p| p.id)
            .collect()
    }

    /// How many updates wait for the player, in words for the page; `None` when none do.
    #[must_use]
    pub fn said_waiting(&self) -> Option<String> {
        match self.pending().iter().filter(|p| !p.automatic).count() {
            0 => None,
            1 => Some("1 plugin update is waiting.".to_owned()),
            n => Some(format!("{n} plugin updates are waiting.")),
        }
    }

    // ----- the page ----------------------------------------------------------------------------------

    /// The Plugins page's view of the shelf.
    #[must_use]
    pub fn view(&self) -> PluginsView {
        let pending = self.pending();
        let sources = self
            .sources
            .iter()
            .map(|s| SourceView {
                url: s.url.clone(),
                name: s.label(),
                first_party: s.first_party(),
                error: s.error.clone(),
                plugins: s.catalog.plugins.len(),
                skipped: s.skipped.clone(),
            })
            .collect();
        // An installed plugin is under Installed only, whichever sources list it.
        let mut available = Vec::new();
        for s in &self.sources {
            for e in &s.catalog.plugins {
                if self.get(e.id.as_str()).is_none() {
                    let state = availability(e, s.first_party(), self.host);
                    available.push(AvailableView::of((&s.url, &s.label()), e, state));
                }
            }
        }
        let found = self.local_plugins();
        let in_place = |id: &str| found.iter().any(|l| l.loaded && l.id() == Some(id));
        let installed = self
            .plugins
            .iter()
            .map(|p| InstalledView {
                local_in_its_place: in_place(&p.id),
                ..InstalledView::of(p, self.source(&p.source), &pending)
            })
            .collect();
        let mut local = Vec::new();
        for listed in &self.local {
            let first = local.len();
            let mut held = found.iter().filter(|l| &l.listed == listed).peekable();
            if held.peek().is_none() {
                local.push(LocalView::empty(listed));
            }
            for l in held {
                let installed = l.id().is_some_and(|id| self.get(id).is_some());
                local.push(LocalView::of(l, installed, self.host));
            }
            if self.tool_failed.contains(listed) {
                local[first].tool_problem = Some(TOOL_FAILED.to_owned());
            }
        }
        PluginsView {
            sources,
            available,
            installed,
            local,
            auto_update: self.auto_update,
            first_party: FIRST_PARTY,
        }
    }
}

/// Whether `entry`, listed by a source (`first_party`: Dereth's own), installs in a client of API
/// version `host`.
fn entry_installable(
    entry: &Entry,
    first_party: bool,
    host: ApiVersion,
) -> Result<(), PluginError> {
    if first_party_only(entry.id.as_str()) && !first_party {
        return Err(PluginError::FirstPartyOnly(entry.id.to_string()));
    }
    if !entry.runs_on(host) {
        return Err(PluginError::NeedsNewerDereth {
            name: entry.name.clone(),
            api: entry.api.to_string(),
        });
    }
    Ok(())
}

/// The most bytes to download for `entry`: its size when its source says it.
fn limit_of(entry: &Entry) -> usize {
    entry
        .size
        .and_then(|n| usize::try_from(n).ok())
        .map_or(MAX_DOWNLOAD, |n| n.min(MAX_DOWNLOAD))
}

/// The release `zip` is, when it is the one `entry` lists ([`Entry::check`]).
fn checked_release(entry: &Entry, zip: &[u8]) -> Result<Release, PluginError> {
    let (_, m) = entry.check(zip).map_err(|why| PluginError::Mismatch {
        name: entry.name.clone(),
        why,
    })?;
    Ok(Release {
        name: m.name.clone(),
        version: m.version.to_string(),
        api: m.api.to_string(),
        sha256: entry.sha256.clone(),
        permissions: catalog::permission_names(&m),
        domains: catalog::domain_names(&m),
        description: entry.description.clone(),
        authors: entry.authors.clone(),
        homepage: entry.homepage.clone(),
        released: entry.released.clone(),
        notes: entry.notes.clone(),
        size: u64::try_from(zip.len()).ok(),
    })
}

/// Whether `entry`, listed by a source (`first_party`: Dereth's own) and not installed, installs
/// in a client of `host`.
fn availability(entry: &Entry, first_party: bool, host: ApiVersion) -> Availability {
    match entry_installable(entry, first_party, host) {
        Err(PluginError::FirstPartyOnly(_)) => Availability::FirstPartyOnly,
        Err(_) => Availability::NeedsNewerDereth,
        Ok(()) => Availability::Install,
    }
}

// ----- refreshing ------------------------------------------------------------------------------

/// Reading every source again, then applying the updates the setting lets apply on their own: the
/// launcher's Refresh, and its read at every start. A walk the caller drives as it fetches: each
/// step takes what the last download brought and answers the next one to make, and the walk ends
/// with what is new, in words ([`Refresh::said`]): the plugins a source lists now that it did not,
/// the updates applied and waiting, and the sources that could not be read. A source that cannot
/// be read holds nothing else up. At start the failures are left out ([`Refresh::said_at_start`]):
/// a source that could not be read says so on its own row.
#[derive(Debug, Clone, Default)]
pub struct Refresh {
    /// The sources still to read.
    sources: VecDeque<String>,
    /// The updates still to apply, once every source has been read.
    updates: Option<VecDeque<String>>,
    /// What the download being made is for.
    fetching: Option<Fetching>,
    /// What the walk came to, a line each, with whether it is a failure.
    lines: Vec<(Line, String)>,
}

/// What a line of a walk's words is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    /// Something new: a plugin listed, an update applied.
    News,
    /// Something that could not be done: a source not read, an update not applied.
    Failed,
}

#[derive(Debug, Clone)]
enum Fetching {
    /// The source at this address's list.
    List(String),
    /// This installed plugin's newer release.
    Zip(String),
}

impl Refresh {
    /// A walk over every source `shelf` has now.
    #[must_use]
    pub fn new<K: Keep>(shelf: &Shelf<K>) -> Self {
        Self {
            sources: shelf.sources.iter().map(|s| s.url.clone()).collect(),
            ..Self::default()
        }
    }

    /// Take in what the last download brought (`None` before the first), at `now` (seconds since
    /// the epoch), and answer the next download; `None` once the walk is over.
    pub fn next<K: Keep>(
        &mut self,
        shelf: &mut Shelf<K>,
        fetched: Option<Result<Vec<u8>, String>>,
        now: u64,
    ) -> Option<Download> {
        let fetched = fetched.unwrap_or_else(|| Err("no answer".to_owned()));
        match self.fetching.take() {
            Some(Fetching::List(url)) => {
                let new = shelf.record_read(&url, fetched, now).unwrap_or_else(|e| {
                    self.lines.push((Line::Failed, e.to_string()));
                    Vec::new()
                });
                let (failed, news) = shelf.read_lines(&url, &new);
                self.lines.extend(failed.map(|l| (Line::Failed, l)));
                self.lines.extend(news.map(|l| (Line::News, l)));
            }
            Some(Fetching::Zip(id)) => {
                let said = fetched.and_then(|zip| {
                    shelf
                        .update(&id, &zip)
                        .map(|done| done.said())
                        .map_err(|e| e.to_string())
                });
                self.lines.push(match said {
                    Ok(l) => (Line::News, l),
                    Err(e) => (Line::Failed, e),
                });
            }
            None => {}
        }
        if let Some(url) = self.sources.pop_front() {
            self.fetching = Some(Fetching::List(url.clone()));
            return Some(Download {
                url,
                limit: MAX_LIST,
            });
        }
        let updates = self
            .updates
            .get_or_insert_with(|| shelf.automatic_updates().into());
        while let Some(id) = updates.pop_front() {
            match shelf.plan_update(&id) {
                Ok(download) => {
                    self.fetching = Some(Fetching::Zip(id));
                    return Some(download);
                }
                Err(e) => self.lines.push((Line::Failed, e.to_string())),
            }
        }
        None
    }

    /// What the walk came to, a line each, and how many updates wait for the player: empty when
    /// nothing is new.
    #[must_use]
    pub fn said<K: Keep>(self, shelf: &Shelf<K>) -> String {
        self.told(shelf, true)
    }

    /// [`Refresh::said`] less what could not be done, for the notice at start: a launcher started
    /// with no connection would otherwise say each time that no source could be read.
    #[must_use]
    pub fn said_at_start<K: Keep>(self, shelf: &Shelf<K>) -> String {
        self.told(shelf, false)
    }

    fn told<K: Keep>(self, shelf: &Shelf<K>, failures: bool) -> String {
        let mut lines: Vec<String> = self
            .lines
            .into_iter()
            .filter(|(kind, _)| failures || *kind == Line::News)
            .map(|(_, l)| l)
            .collect();
        lines.extend(shelf.said_waiting());
        lines.join("\n")
    }
}

/// Whether a listed plugin that is not installed can be installed here, and if not, why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Install,
    /// It needs a newer API version than this client has.
    NeedsNewerDereth,
    /// Dereth's own id, listed by another source.
    FirstPartyOnly,
}

/// One source, as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceView {
    pub url: String,
    pub name: String,
    pub first_party: bool,
    pub error: Option<String>,
    /// How many plugins its last list names.
    pub plugins: usize,
    pub skipped: Vec<String>,
}

/// One plugin a source lists that is not installed, as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AvailableView {
    pub source: String,
    pub source_name: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: String,
    pub description: String,
    pub authors: Vec<String>,
    pub homepage: String,
    pub permissions: Vec<String>,
    pub domains: Vec<String>,
    pub notes: String,
    pub released: String,
    pub size: Option<u64>,
    pub state: Availability,
}

impl AvailableView {
    /// Entry `e` of the source `(url, name)`, `state` here.
    fn of((url, name): (&str, &str), e: &Entry, state: Availability) -> Self {
        Self {
            source: url.to_owned(),
            source_name: name.to_owned(),
            id: e.id.to_string(),
            name: e.name.clone(),
            version: e.version.to_string(),
            api: e.api.to_string(),
            description: e.description.clone(),
            authors: e.authors.clone(),
            homepage: e.homepage.clone(),
            permissions: e.permissions.clone(),
            domains: e.domains.clone(),
            notes: e.notes.clone(),
            released: e.released.clone(),
            size: e.size,
            state,
        }
    }
}

/// One installed plugin, as the page shows it: the release installed, and what its source said
/// of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: String,
    pub description: String,
    pub authors: Vec<String>,
    pub homepage: String,
    pub permissions: Vec<String>,
    pub domains: Vec<String>,
    pub released: String,
    pub notes: String,
    pub size: Option<u64>,
    pub source: String,
    pub source_name: String,
    /// Its source was removed: it gets no updates.
    pub source_removed: bool,
    /// A newer release in its source.
    pub update: Option<Pending>,
    /// A plugin of its id in a folder on this computer is loaded in its place. It still updates.
    pub local_in_its_place: bool,
}

impl InstalledView {
    /// Installed plugin `p`, from `source` (`None` once removed), with the updates waiting.
    fn of(p: &Installed, source: Option<&Source>, pending: &[Pending]) -> Self {
        let r = &p.release;
        // A record kept before the source's words were has none: its source's are used.
        let listed = source
            .and_then(|s| s.catalog.get(&p.id))
            .filter(|_| r.description.is_empty());
        Self {
            id: p.id.clone(),
            name: r.name.clone(),
            version: r.version.clone(),
            api: r.api.clone(),
            description: listed.map_or_else(|| r.description.clone(), |e| e.description.clone()),
            authors: listed.map_or_else(|| r.authors.clone(), |e| e.authors.clone()),
            homepage: listed.map_or_else(|| r.homepage.clone(), |e| e.homepage.clone()),
            permissions: r.permissions.clone(),
            domains: r.domains.clone(),
            released: r.released.clone(),
            notes: r.notes.clone(),
            size: r.size,
            source: p.source.clone(),
            source_name: source.map_or_else(|| host_of(&p.source).to_owned(), Source::label),
            source_removed: source.is_none(),
            update: pending.iter().find(|u| u.id == p.id).cloned(),
            local_in_its_place: false,
        }
    }
}

/// A plugin in a folder on this computer, or a folder on that list holding none, as the page
/// shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalView {
    /// The folder on the list: what Remove takes off it.
    pub listed: String,
    /// The plugin's own folder; the folder on the list when it holds none.
    pub folder: String,
    /// What its manifest says; the folder's name, and nothing else, when that does not read.
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: String,
    pub description: String,
    pub authors: Vec<String>,
    pub homepage: String,
    pub permissions: Vec<String>,
    pub domains: Vec<String>,
    /// Why there is no plugin here to load, in words for the page.
    pub problem: Option<String>,
    /// It needs a newer API version than this client has.
    pub needs_newer_dereth: bool,
    /// It is loaded in place of the installed plugin of its id.
    pub in_place_of_installed: bool,
    /// Another plugin of its id, found before it, is loaded instead.
    pub id_taken: bool,
    /// On the first row of a folder the launcher keeps the author's tool in: the copy there could
    /// not be brought up to date, in words for the page.
    pub tool_problem: Option<String>,
}

impl LocalView {
    /// The folder on the list `listed`, which holds no plugin of its own.
    fn empty(listed: &Path) -> Self {
        let problem = if !listed.is_dir() {
            "This folder is not there any more."
        } else if plugins_in(listed).is_empty() {
            "This folder no longer holds a plugin."
        } else {
            "Its plugins are listed under another folder on the list."
        };
        Self {
            problem: Some(problem.to_owned()),
            ..Self::at(listed, listed)
        }
    }

    /// The folder `folder`, found in the folder on the list `listed`, with nothing read of it.
    fn at(listed: &Path, folder: &Path) -> Self {
        Self {
            listed: listed.display().to_string(),
            folder: folder.display().to_string(),
            id: String::new(),
            name: folder_label(folder),
            version: String::new(),
            api: String::new(),
            description: String::new(),
            authors: Vec::new(),
            homepage: String::new(),
            permissions: Vec::new(),
            domains: Vec::new(),
            problem: None,
            needs_newer_dereth: false,
            in_place_of_installed: false,
            id_taken: false,
            tool_problem: None,
        }
    }

    /// The plugin `l`, for a client of API version `host`; `installed`: a plugin of its id is
    /// installed.
    fn of(l: &LocalPlugin, installed: bool, host: ApiVersion) -> Self {
        let mut v = Self::at(&l.listed, &l.folder);
        match &l.manifest {
            Ok(m) => {
                v.id = m.id.to_string();
                v.name.clone_from(&m.name);
                v.version = m.version.to_string();
                v.api = m.api.to_string();
                v.description.clone_from(&m.description);
                v.authors.clone_from(&m.authors);
                v.homepage.clone_from(&m.homepage);
                v.permissions = catalog::permission_names(m);
                v.domains = catalog::domain_names(m);
                v.needs_newer_dereth = !host.runs(m.api);
                v.in_place_of_installed = l.loaded && installed;
                v.id_taken = !l.loaded;
            }
            Err(e) => v.problem = Some(format!("Its plugin.toml does not read: {e}")),
        }
        v
    }
}

/// A folder's own name, else the whole of it.
fn folder_label(folder: &Path) -> String {
    folder.file_name().map_or_else(
        || folder.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// The Plugins page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginsView {
    pub sources: Vec<SourceView>,
    /// Every source's plugins that are not installed, source by source.
    pub available: Vec<AvailableView>,
    pub installed: Vec<InstalledView>,
    /// The plugins in the folders on this computer, folder by folder.
    pub local: Vec<LocalView>,
    pub auto_update: AutoUpdate,
    pub first_party: &'static str,
}

#[cfg(test)]
mod tests;
