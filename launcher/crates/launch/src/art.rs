//! Art packs the launcher installs and updates for the Dereth client, from art pack sources.
//!
//! A **source** is one list, `art.json` at an `https://` address, naming many packs, each at its
//! latest version, with where its packed file is and the file's SHA-256
//! ([`dereth_art_catalog`]). The launcher starts with one, Dereth's own ([`FIRST_PARTY`]),
//! and nothing installed; the player adds others by their address, once the page has said that
//! Dereth does not review them. The packs every source lists are offered together, each saying
//! which source lists it.
//!
//! A source may also be a **GitHub repository**, `https://github.com/<owner>/<repo>`, whose
//! releases are one pack's versions ([`github`]): what its releases come to is kept and checked as
//! an `art.json` is, and the pack is installed under the id its manifest gives.
//!
//! What a source lists is checked before anything is kept: the file must be the one the entry
//! names, its size and its SHA-256 ([`Entry::check_download`]), and the pack in it must be the one
//! the entry lists ([`Entry::check_packed`]); an id under `network.dereth.` installs only from
//! Dereth's own source. An installed pack updates only from the source it came from, and only to
//! a higher version. Updates are applied when the player asks, or on their own while the Library's
//! setting to update components is on.
//!
//! **The stack.** The installed packs are kept in an order, and each is on or off. The client is
//! given the packs that are on, in that order ([`Shelf::stack`]), and where two replace the same
//! thing (an object, a ground, a retail picture, a lighting key), the later one wins. A pack just
//! installed goes last, so it wins; the player moves packs and turns them off in the Library.
//! Packs are the player's, not a world's: every world the Dereth client plays draws them, the
//! worlds that play the 1999 data among them, since a pack replaces things by their ids.
//!
//! **Where they are kept**: the launcher's data folder, `art/`: `sources.json` (the sources and
//! the last list each gave), `installed.json` (the installed packs in their order) and one folder
//! per pack holding its `current.dat`. A pack runs to 2 GiB, so the fetching streams it to a file
//! of its own beside that one ([`Download::partial`]), hashing as it goes, and the shelf takes the
//! file ([`Shelf::finish`]); the fetching is the caller's.

use std::path::{Path, PathBuf};

use dereth_art_catalog::{self as art_catalog, Catalog, Entry, Mismatch};
use serde::{Deserialize, Serialize};

use crate::plugins::folder_name;

pub mod github;

/// What a GitHub repository's releases came to: the list, as an `art.json`'s bytes, and the tag of
/// GitHub's answer it came from.
pub type GithubList = (Vec<u8>, Option<String>);

/// Dereth's own source, the one the launcher starts with.
pub const FIRST_PARTY: &str = art_catalog::DERETH_SOURCE;

/// The largest source's list the launcher reads.
pub const MAX_LIST: usize = art_catalog::MAX_BYTES;

/// The sources' file in the art folder.
pub const SOURCES_FILE: &str = "sources.json";

/// The installed packs' file in the art folder.
pub const INSTALLED_FILE: &str = "installed.json";

/// The installed file's name in a pack's folder.
pub const PACK_FILE: &str = "current.dat";

/// A download's file in a pack's folder while it is made: kept when it stops part way, so the next
/// try goes on from where it stopped.
pub const PARTIAL_FILE: &str = "download.partial";

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
    /// The last list read whole, as its text: for a GitHub repository, the list its releases came
    /// to ([`github::catalog`]).
    #[serde(default)]
    list: Option<String>,
    /// For a GitHub repository, the tag GitHub gave the answer that list came from, which the next
    /// ask sends so an unchanged answer costs nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
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

    /// The owner and repository, when this source is a GitHub repository ([`github`]).
    #[must_use]
    pub fn github(&self) -> Option<(String, String)> {
        github::repository(&self.url)
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
        if let Ok(read) = Catalog::parse(text.as_bytes()) {
            self.catalog = read.catalog;
            self.skipped = read.skipped;
            self.list = Some(text);
        }
    }
}

fn host_of(url: &str) -> &str {
    url.strip_prefix("https://")
        .and_then(|r| r.split(['/', '?', '#']).next())
        .unwrap_or(url)
}

/// What an installed pack's source said of the version installed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub size: u64,
    /// The hash of the folder it was packed from.
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub made_for: Vec<String>,
    #[serde(default)]
    pub released: String,
    #[serde(default)]
    pub notes: String,
}

impl Release {
    fn of(e: &Entry) -> Self {
        Self {
            name: e.name.clone(),
            version: e.version.clone(),
            sha256: e.sha256.clone(),
            size: e.size,
            hash: e.hash.clone(),
            description: e.description.clone(),
            authors: e.authors.clone(),
            license: e.license.clone(),
            homepage: e.homepage.clone(),
            made_for: e.made_for.clone(),
            released: e.released.clone(),
            notes: e.notes.clone(),
        }
    }
}

/// One installed pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    pub id: String,
    /// The source it came from, and the only one it updates from.
    pub source: String,
    pub release: Release,
    /// Whether the client is given it.
    #[serde(default = "yes")]
    pub on: bool,
}

fn yes() -> bool {
    true
}

/// A download to make before installing: from where, how large and with which SHA-256 the file
/// must be, and the file to stream it into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub id: String,
    pub name: String,
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    /// Where it is streamed to: [`PARTIAL_FILE`] in the pack's folder.
    pub partial: PathBuf,
    /// The source it is installed from.
    pub source: String,
}

/// What an install or an update came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub name: String,
    pub version: String,
    pub updated: bool,
}

impl Done {
    /// What it came to, in words for the page.
    #[must_use]
    pub fn said(&self) -> String {
        if self.updated {
            format!("Updated {} to {}.", self.name, self.version)
        } else {
            format!("Installed {} {}.", self.name, self.version)
        }
    }
}

/// Why something was not done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtError {
    NotHttps(String),
    AlreadyAdded(String),
    FirstPartyStays,
    NoSource(String),
    NotListed {
        source: String,
        id: String,
    },
    FirstPartyOnly(String),
    NotInstalled(String),
    /// A pack of that id is installed from another source.
    InstalledFrom {
        id: String,
        source: String,
    },
    NothingNewer(String),
    /// Its file is not where the source says: an address that does not resolve to `https://`.
    BadAddress(String),
    Mismatch(Mismatch),
    Io(String),
}

impl std::fmt::Display for ArtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotHttps(u) => write!(f, "{u} is not an https:// address"),
            Self::AlreadyAdded(u) => write!(f, "{u} is a source already"),
            Self::FirstPartyStays => write!(f, "Dereth's own source can't be removed"),
            Self::NoSource(u) => write!(f, "{u} is not a source"),
            Self::NotListed { source, id } => write!(f, "{source} does not list {id}"),
            Self::FirstPartyOnly(id) => {
                write!(
                    f,
                    "{id} is one of Dereth's own packs, and installs only from Dereth"
                )
            }
            Self::NotInstalled(id) => write!(f, "{id} is not installed"),
            Self::InstalledFrom { id, source } => {
                write!(
                    f,
                    "{id} is installed from {source} already; remove it first"
                )
            }
            Self::NothingNewer(name) => write!(f, "{name} is up to date"),
            Self::BadAddress(u) => write!(f, "{u} is not an https:// address"),
            Self::Mismatch(m) => write!(f, "{m}"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ArtError {}

fn io(what: &str, path: &Path, e: &std::io::Error) -> ArtError {
    ArtError::Io(format!("{what} {}: {e}", path.display()))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SourcesFile {
    #[serde(default)]
    sources: Vec<Source>,
}

/// The art packs the launcher installs: the sources, the installed packs in their order, and the
/// folder they are kept in.
#[derive(Debug, Clone)]
pub struct Shelf {
    pub dir: PathBuf,
    /// Dereth's own first, then the player's, in the order added.
    pub sources: Vec<Source>,
    /// In the stack's order: a later one wins over an earlier one.
    pub packs: Vec<Installed>,
}

impl Shelf {
    /// The shelf in `dir` (the launcher's `art` data folder): Dereth's own source and nothing
    /// installed when it holds nothing yet. A file that does not read is treated as empty and
    /// replaced at the next change. An installed pack whose file is gone is dropped.
    #[must_use]
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let file: SourcesFile = std::fs::read(dir.join(SOURCES_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut packs: Vec<Installed> = std::fs::read(dir.join(INSTALLED_FILE))
            .ok()
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
        let mut seen = Vec::new();
        packs.retain(|p| {
            let keep =
                !seen.contains(&p.id) && dir.join(folder_name(&p.id)).join(PACK_FILE).is_file();
            seen.push(p.id.clone());
            keep
        });
        Self {
            dir,
            sources,
            packs,
        }
    }

    fn save_sources(&self) -> Result<(), ArtError> {
        let file = SourcesFile {
            sources: self.sources.clone(),
        };
        self.write(SOURCES_FILE, &file)
    }

    fn save_installed(&self) -> Result<(), ArtError> {
        self.write(INSTALLED_FILE, &self.packs)
    }

    fn write(&self, name: &str, value: &impl Serialize) -> Result<(), ArtError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| io("making", &self.dir, &e))?;
        let bytes = serde_json::to_vec_pretty(value).map_err(|e| ArtError::Io(e.to_string()))?;
        let path = self.dir.join(name);
        crate::state::write_atomic(&path, &bytes).map_err(|e| io("writing", &path, &e))
    }

    /// The folder pack `id` is kept in.
    #[must_use]
    pub fn pack_dir(&self, id: &str) -> PathBuf {
        self.dir.join(folder_name(id))
    }

    /// The installed pack `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Installed> {
        self.packs.iter().find(|p| p.id == id)
    }

    /// The source at `url`.
    #[must_use]
    pub fn source(&self, url: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.url == url)
    }

    /// The files of the packs that are on, in the stack's order: what the client is given.
    #[must_use]
    pub fn stack(&self) -> Vec<PathBuf> {
        self.packs
            .iter()
            .filter(|p| p.on)
            .map(|p| self.pack_dir(&p.id).join(PACK_FILE))
            .collect()
    }

    // ----- sources -------------------------------------------------------------------------------

    /// Add the source at `url`; the page has the player confirm it first, and its list is read by
    /// the caller next ([`Shelf::record_read`]). Answers the address as kept.
    ///
    /// # Errors
    /// Not an `https://` address, already a source, or the folder could not be written.
    pub fn add_source(&mut self, url: &str) -> Result<String, ArtError> {
        let url = url.trim();
        let ok = url.len() > 8
            && url[..8].eq_ignore_ascii_case("https://")
            && !host_of(url).is_empty()
            && !url.chars().any(char::is_whitespace);
        if !ok {
            return Err(ArtError::NotHttps(url.to_owned()));
        }
        let url = format!("https://{}", &url[8..]);
        let url = match github::repository(&url) {
            Some((owner, repo)) => github::source_url(&owner, &repo),
            None => url,
        };
        if self
            .sources
            .iter()
            .any(|s| s.url.eq_ignore_ascii_case(&url))
        {
            return Err(ArtError::AlreadyAdded(url));
        }
        self.sources.push(Source::new(&url));
        self.save_sources()?;
        Ok(url)
    }

    /// Remove the source at `url`. The packs installed from it stay, with no updates.
    ///
    /// # Errors
    /// Dereth's own source, not a source, or the folder could not be written.
    pub fn remove_source(&mut self, url: &str) -> Result<(), ArtError> {
        if url == FIRST_PARTY {
            return Err(ArtError::FirstPartyStays);
        }
        let before = self.sources.len();
        self.sources.retain(|s| s.url != url);
        if self.sources.len() == before {
            return Err(ArtError::NoSource(url.to_owned()));
        }
        self.save_sources()
    }

    /// What reading the source at `url` came to, at `now` (seconds since the epoch): its list's
    /// bytes, or why they could not be fetched. A list that reads replaces the last; one that does
    /// not leaves the last in use and says why. Answers the names of the packs it lists that the
    /// last list read did not: none the first time a source is read.
    ///
    /// # Errors
    /// Not a source, or the folder could not be written.
    pub fn record_read(
        &mut self,
        url: &str,
        fetched: Result<Vec<u8>, String>,
        now: u64,
    ) -> Result<Vec<String>, ArtError> {
        let source = self
            .sources
            .iter_mut()
            .find(|s| s.url == url)
            .ok_or_else(|| ArtError::NoSource(url.to_owned()))?;
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
                        .packs
                        .iter()
                        .filter(|e| source.catalog.get(&e.id).is_none())
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

    /// What to ask GitHub for the source at `url`, when it is a GitHub repository: the releases
    /// API's address, and the tag of the answer its kept list came from, sent so an unchanged answer
    /// costs nothing. No tag when no list is kept.
    #[must_use]
    pub fn github_ask(&self, url: &str) -> Option<(String, Option<String>)> {
        let s = self.source(url)?;
        let (owner, repo) = s.github()?;
        let etag = s.etag.clone().filter(|_| s.list.is_some());
        Some((github::api_url(&owner, &repo), etag))
    }

    /// What asking GitHub for the source at `url` came to, at `now`: not modified since the list
    /// kept (`Ok(None)`), a list its releases came to with the answer's tag, or why not. A failure
    /// keeps the last list in use, says why, and forgets the tag, so the next ask is whole.
    ///
    /// # Errors
    /// Not a source, or the folder could not be written.
    pub fn record_github(
        &mut self,
        url: &str,
        answer: Result<Option<GithubList>, String>,
        now: u64,
    ) -> Result<Vec<String>, ArtError> {
        match answer {
            Ok(None) => {
                let s = self
                    .sources
                    .iter_mut()
                    .find(|s| s.url == url)
                    .ok_or_else(|| ArtError::NoSource(url.to_owned()))?;
                s.read_at = Some(now);
                s.error = None;
                self.save_sources()?;
                Ok(Vec::new())
            }
            Ok(Some((list, etag))) => {
                let new = self.record_read(url, Ok(list), now)?;
                if let Some(s) = self.sources.iter_mut().find(|s| s.url == url) {
                    s.etag = if s.error.is_none() { etag } else { None };
                }
                self.save_sources()?;
                Ok(new)
            }
            Err(e) => {
                let new = self.record_read(url, Err(e), now)?;
                if let Some(s) = self.sources.iter_mut().find(|s| s.url == url) {
                    s.etag = None;
                }
                self.save_sources()?;
                Ok(new)
            }
        }
    }

    // ----- installing ----------------------------------------------------------------------------

    fn entry(&self, source: &str, id: &str) -> Result<&Entry, ArtError> {
        let s = self
            .source(source)
            .ok_or_else(|| ArtError::NoSource(source.to_owned()))?;
        if art_catalog::first_party_only(id) && !s.first_party() {
            return Err(ArtError::FirstPartyOnly(id.to_owned()));
        }
        // A GitHub repository's releases are one pack, whatever id its manifest gave it.
        if s.github().is_some() {
            if let Some(only) = s.catalog.packs.first() {
                return Ok(only);
            }
        }
        s.catalog.get(id).ok_or_else(|| ArtError::NotListed {
            source: s.label(),
            id: id.to_owned(),
        })
    }

    fn download_of(&self, source: &str, e: &Entry) -> Result<Download, ArtError> {
        let url = dereth_plugin_package::catalog::resolve(source, &e.file)
            .ok_or_else(|| ArtError::BadAddress(e.file.clone()))?;
        Ok(Download {
            id: e.id.clone(),
            name: e.name.clone(),
            version: e.version.clone(),
            url,
            size: e.size,
            sha256: e.sha256.clone(),
            partial: self.pack_dir(&e.id).join(PARTIAL_FILE),
            source: source.to_owned(),
        })
    }

    /// The download that installs pack `id` from `source`.
    ///
    /// # Errors
    /// Not a source, not listed by it, or one of Dereth's own packs from another source.
    pub fn plan_install(&self, source: &str, id: &str) -> Result<Download, ArtError> {
        let e = self.entry(source, id)?;
        self.download_of(source, e)
    }

    /// The download that updates pack `id` from its own source.
    ///
    /// # Errors
    /// Not installed, its source gone or no longer listing it, or nothing newer there.
    pub fn plan_update(&self, id: &str) -> Result<Download, ArtError> {
        let p = self
            .get(id)
            .ok_or_else(|| ArtError::NotInstalled(id.to_owned()))?;
        let e = self.entry(&p.source, id)?;
        if !art_catalog::is_newer(&e.version, &p.release.version) {
            return Err(ArtError::NothingNewer(p.release.name.clone()));
        }
        self.download_of(&p.source, e)
    }

    /// Take the file `download` was streamed into, `size` bytes with SHA-256 `sha256`: checked
    /// against what its source lists, then kept as the pack, in place of the version installed. A
    /// new pack goes last in the stack, and on; an updated one keeps its place. A file that fails
    /// a check is deleted, and what was installed stays.
    ///
    /// # Errors
    /// Its source no longer lists that version, a check failed, or it could not be kept.
    pub fn finish(
        &mut self,
        download: &Download,
        size: u64,
        sha256: &str,
    ) -> Result<Done, ArtError> {
        let refuse = |e: ArtError| {
            let _ = std::fs::remove_file(&download.partial);
            e
        };
        let e = self.entry(&download.source, &download.id)?.clone();
        e.check_download(size, sha256)
            .map_err(|m| refuse(ArtError::Mismatch(m)))?;
        let packed = dereth_assets::packed_art::read_manifest(&download.partial)
            .map_err(|why| refuse(ArtError::Mismatch(Mismatch::NotAPack(why))))?;
        let github = self
            .source(&download.source)
            .is_some_and(|s| s.github().is_some());
        let (id, release) = if github {
            self.github_pack(download, &e, &packed).map_err(refuse)?
        } else {
            e.check_packed(&packed)
                .map_err(|m| refuse(ArtError::Mismatch(m)))?;
            (e.id.clone(), Release::of(&e))
        };
        let dir = self.pack_dir(&id);
        std::fs::create_dir_all(&dir).map_err(|err| io("making", &dir, &err))?;
        let to = dir.join(PACK_FILE);
        std::fs::rename(&download.partial, &to).map_err(|err| io("keeping", &to, &err))?;
        // A GitHub repository's pack kept under another id before (its manifest's id changed, or it
        // named none) is this one now.
        let earlier = self
            .packs
            .iter()
            .position(|p| p.id == id || (github && p.source == download.source));
        if let Some(i) = earlier.filter(|i| self.packs[*i].id != id) {
            let old = self.pack_dir(&self.packs[i].id);
            let _ = std::fs::remove_dir_all(old);
            self.packs[i].id.clone_from(&id);
        }
        let name = release.name.clone();
        let updated = match earlier.map(|i| &mut self.packs[i]) {
            Some(p) => {
                p.source.clone_from(&download.source);
                p.release = release;
                true
            }
            None => {
                self.packs.push(Installed {
                    id: id.clone(),
                    source: download.source.clone(),
                    release,
                    on: true,
                });
                false
            }
        };
        self.save_installed()?;
        Ok(Done {
            name,
            version: e.version,
            updated,
        })
    }

    /// The id and the release a GitHub repository's download `download` of `e` keeps, from the
    /// pack in it, `packed`: the manifest's id where it names one (else the list's), the manifest's
    /// name where it names one (else the release's title). A version the manifest names must be the
    /// release's.
    fn github_pack(
        &self,
        download: &Download,
        e: &Entry,
        packed: &dereth_assets::packed_art::Packed,
    ) -> Result<(String, Release), ArtError> {
        let m =
            dereth_assets::art_pack::ArtPackManifest::parse(&packed.manifest).map_err(|err| {
                ArtError::Mismatch(Mismatch::NotAPack(format!(
                    "its manifest will not read: {}",
                    err.0
                )))
            })?;
        if !m.about.version.is_empty() && m.about.version != e.version {
            return Err(ArtError::Mismatch(Mismatch::Says {
                field: "version",
                listed: e.version.clone(),
                pack: m.about.version,
            }));
        }
        let id = if m.about.id.is_empty() {
            e.id.clone()
        } else {
            m.about.id.clone()
        };
        if art_catalog::first_party_only(&id) {
            return Err(ArtError::FirstPartyOnly(id));
        }
        if let Some(p) = self.get(&id).filter(|p| p.source != download.source) {
            return Err(ArtError::InstalledFrom {
                id,
                source: p.source.clone(),
            });
        }
        let mut release = Release::of(e);
        release.hash.clone_from(&packed.contents.hash);
        if !m.name.trim().is_empty() {
            release.name = m.name.trim().to_owned();
        }
        let about = m.about;
        if !about.authors.is_empty() {
            release.authors = about.authors;
        }
        if !about.license.is_empty() {
            release.license = about.license;
        }
        if !about.made_for.is_empty() {
            release.made_for = about.made_for;
        }
        Ok((id, release))
    }

    /// Remove pack `id` and its file.
    ///
    /// # Errors
    /// Not installed, or its folder could not be deleted.
    pub fn remove(&mut self, id: &str) -> Result<(), ArtError> {
        if self.get(id).is_none() {
            return Err(ArtError::NotInstalled(id.to_owned()));
        }
        let dir = self.pack_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| io("removing", &dir, &e))?;
        }
        self.packs.retain(|p| p.id != id);
        self.save_installed()
    }

    /// Turn pack `id` on or off.
    ///
    /// # Errors
    /// Not installed, or the folder could not be written.
    pub fn set_on(&mut self, id: &str, on: bool) -> Result<(), ArtError> {
        let p = self
            .packs
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| ArtError::NotInstalled(id.to_owned()))?;
        p.on = on;
        self.save_installed()
    }

    /// Move pack `id` one place later in the stack (`later`, so it wins over one more) or earlier.
    /// At either end it stays.
    ///
    /// # Errors
    /// Not installed, or the folder could not be written.
    pub fn move_pack(&mut self, id: &str, later: bool) -> Result<(), ArtError> {
        let at = self
            .packs
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| ArtError::NotInstalled(id.to_owned()))?;
        let to = if later {
            (at + 1).min(self.packs.len() - 1)
        } else {
            at.saturating_sub(1)
        };
        self.packs.swap(at, to);
        self.save_installed()
    }

    // ----- updates -------------------------------------------------------------------------------

    /// The installed packs with a newer version in their own source, in the stack's order.
    #[must_use]
    pub fn pending(&self) -> Vec<Pending> {
        self.packs
            .iter()
            .filter_map(|p| {
                let e = self.entry(&p.source, &p.id).ok()?;
                art_catalog::is_newer(&e.version, &p.release.version).then(|| Pending {
                    id: p.id.clone(),
                    name: p.release.name.clone(),
                    from: p.release.version.clone(),
                    to: e.version.clone(),
                    size: e.size,
                    notes: e.notes.clone(),
                    released: e.released.clone(),
                })
            })
            .collect()
    }

    // ----- the page ------------------------------------------------------------------------------

    /// What the page shows: the sources, the installed packs (the one that wins first) and the
    /// packs the sources list that are not installed.
    #[must_use]
    pub fn view(&self) -> ArtView {
        let pending = self.pending();
        let label = |url: &str| {
            self.source(url)
                .map_or_else(|| host_of(url).to_owned(), Source::label)
        };
        let installed = self
            .packs
            .iter()
            .rev()
            .map(|p| InstalledView {
                id: p.id.clone(),
                source: p.source.clone(),
                source_name: label(&p.source),
                source_removed: self.source(&p.source).is_none(),
                on: p.on,
                release: p.release.clone(),
                update: pending.iter().find(|u| u.id == p.id).cloned(),
            })
            .collect();
        let mut available = Vec::new();
        for s in &self.sources {
            // A GitHub repository's pack is installed under its manifest's id.
            let from_here = s.github().is_some() && self.packs.iter().any(|p| p.source == s.url);
            for e in &s.catalog.packs {
                if from_here || self.get(&e.id).is_some() {
                    continue;
                }
                available.push(AvailableView {
                    id: e.id.clone(),
                    source: s.url.clone(),
                    source_name: s.label(),
                    installable: !art_catalog::first_party_only(&e.id) || s.first_party(),
                    release: Release::of(e),
                });
            }
        }
        let sources = self
            .sources
            .iter()
            .map(|s| SourceView {
                url: s.url.clone(),
                name: s.label(),
                first_party: s.first_party(),
                github: s.github().is_some(),
                packs: s.catalog.packs.len(),
                error: s.error.clone(),
                skipped: s.skipped.clone(),
            })
            .collect();
        ArtView {
            sources,
            installed,
            available,
        }
    }
}

// ----- streaming ---------------------------------------------------------------------------------

/// Where a download into `partial` of a file of `size` bytes goes on from: the length of what an
/// earlier try left there, when that is shorter than the whole; otherwise nothing, and anything
/// there is deleted.
#[must_use]
pub fn resume_at(partial: &Path, size: u64) -> u64 {
    match std::fs::metadata(partial) {
        Ok(m) if m.is_file() && m.len() < size => m.len(),
        Ok(_) => {
            let _ = std::fs::remove_file(partial);
            0
        }
        Err(_) => 0,
    }
}

/// Stream `body` into `partial`, after the `at` bytes already there (`at` 0 starts the file
/// afresh), at most `size` bytes in all, telling `progress` how far it has got of `size`. What was
/// there is hashed first, so the answer is the whole file's length and SHA-256 (lower-case hex).
///
/// # Errors
/// The file will not write, `body` stops with an error (what was written stays, for the next try
/// to go on from), or it holds more than `size` bytes (the file is deleted).
pub fn stream(
    partial: &Path,
    at: u64,
    size: u64,
    body: &mut dyn std::io::Read,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(u64, String), String> {
    use sha2::{Digest, Sha256};
    use std::io::{Read as _, Write as _};
    let shown = || partial.display().to_string();
    if let Some(dir) = partial.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("making {}: {e}", dir.display()))?;
    }
    let mut hash = Sha256::new();
    let mut buf = vec![0; 1 << 20];
    let mut done = 0u64;
    if at > 0 {
        let mut kept = std::fs::File::open(partial).map_err(|e| format!("{}: {e}", shown()))?;
        while done < at {
            let want = usize::try_from((at - done).min(buf.len() as u64)).unwrap_or(buf.len());
            let n = kept
                .read(&mut buf[..want])
                .map_err(|e| format!("{}: {e}", shown()))?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
            done += n as u64;
        }
    }
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(at > 0)
        .truncate(at == 0)
        .open(partial)
        .map_err(|e| format!("{}: {e}", shown()))?;
    progress(done, size);
    loop {
        let n = body.read(&mut buf).map_err(|_| {
            "The download stopped part way. Try again to go on from where it stopped.".to_owned()
        })?;
        if n == 0 {
            break;
        }
        if done + n as u64 > size {
            drop(out);
            let _ = std::fs::remove_file(partial);
            return Err("The download was larger than its source lists, so it was stopped.".into());
        }
        out.write_all(&buf[..n])
            .map_err(|e| format!("writing {}: {e}", shown()))?;
        hash.update(&buf[..n]);
        done += n as u64;
        progress(done, size);
    }
    out.flush()
        .map_err(|e| format!("writing {}: {e}", shown()))?;
    let hex = hash
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        });
    Ok((done, hex))
}

/// Whether `free` bytes of room is enough for a download of `size` bytes, `at` of which are there
/// already: what is left of it and a margin of 64 MiB. Room unknown is room enough.
#[must_use]
pub fn room_for(free: Option<u64>, size: u64, at: u64) -> bool {
    free.is_none_or(|free| free >= size.saturating_sub(at).saturating_add(64 << 20))
}

/// A newer version of an installed pack, in its own source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pending {
    pub id: String,
    pub name: String,
    pub from: String,
    pub to: String,
    pub size: u64,
    pub notes: String,
    pub released: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceView {
    pub url: String,
    pub name: String,
    pub first_party: bool,
    /// It is a GitHub repository, whose releases are one pack's versions.
    pub github: bool,
    pub packs: usize,
    pub error: Option<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledView {
    pub id: String,
    pub source: String,
    pub source_name: String,
    /// The player removed its source, so it gets no updates.
    pub source_removed: bool,
    pub on: bool,
    pub release: Release,
    pub update: Option<Pending>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AvailableView {
    pub id: String,
    pub source: String,
    pub source_name: String,
    /// False for one of Dereth's own ids listed by another source.
    pub installable: bool,
    pub release: Release,
}

/// The art packs as the page shows them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ArtView {
    pub sources: Vec<SourceView>,
    /// The one that wins first.
    pub installed: Vec<InstalledView>,
    pub available: Vec<AvailableView>,
}

#[cfg(test)]
mod tests;
