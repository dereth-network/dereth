//! What a build is (its facts, [`BuildFacts`]) and what a release says about upgrading to it (its
//! declaration, [`UpgradeDeclaration`], the `upgrade` object of the release's `release.json`).
//!
//! The facts are what an upgrade depends on and the code knows: each server's are its own, its
//! database schema versions and the world data it reads, and the server's `--release-facts` prints
//! them as JSON. The release packaging derives the same numbers from the source and writes them,
//! with what cannot be derived (the oldest version that can upgrade straight to the release, and
//! configuration keys removed or newly required), into `release.json`.

use std::fmt::Debug;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// What the updater reads of a build's facts.
pub trait BuildFacts:
    Clone + Debug + Eq + Serialize + DeserializeOwned + Send + Sync + 'static
{
    /// Its version.
    fn version(&self) -> &str;
    /// The target triple its release archive is named for.
    fn target(&self) -> &str;
    /// Takes the build for another version (the updater's own tests).
    fn set_version(&mut self, version: &str);
}

/// What the updater reads of a release's declaration, against the facts `F` of the build running.
pub trait UpgradeDeclaration<F>:
    Clone + Debug + Eq + DeserializeOwned + Send + Sync + 'static
{
    /// How the release relates to the one before it.
    fn kind(&self) -> Kind;
    /// The oldest version that can upgrade straight to the release.
    fn upgrades_from(&self) -> &str;
    /// The configuration changes since the previous release.
    fn config(&self) -> &[ConfigChange];
    /// Whether any database's schema is older than the running build's.
    fn databases_older_than(&self, mine: &F) -> bool;
    /// The databases a server running `mine` has migrated by installing the release.
    fn migrations_from(&self, mine: &F) -> Vec<Migration>;
    /// Whether a server running `mine` must have its world pack rebuilt.
    fn world_pack_rebuild_from(&self, _mine: &F) -> bool {
        false
    }
    /// The world database release to fetch before the new program starts.
    fn world_database_fetch_from(&self, _mine: &F) -> Option<String> {
        None
    }
    /// What the release's own program reports that its declaration does not say, when anything.
    fn mismatch(&self, facts: &F) -> Option<String>;
}

/// The target triple a release archive is named for: `build_target`, the one the release packaging
/// compiled the build for, or, for a build of its own, the release target of this operating system
/// and architecture.
#[must_use]
pub fn target_triple(build_target: Option<&str>) -> String {
    if let Some(t) = build_target {
        return t.to_owned();
    }
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "windows") => "x86_64-pc-windows-msvc".to_owned(),
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu".to_owned(),
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu".to_owned(),
        ("aarch64", "macos") => "aarch64-apple-darwin".to_owned(),
        (arch, os) => format!("{arch}-{os}"),
    }
}

/// How a release relates to the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The first release: nothing before it.
    Initial,
    /// Same major and minor as the release before it. Declares nothing.
    Patch,
    Minor,
    Major,
}

/// A configuration key a release renames, removes or newly requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigChange {
    /// The dotted key in the server's configuration file (`server.update`).
    pub key: String,
    /// `renamed`, `removed` or `required`.
    pub change: String,
    /// A renamed key's new name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// What the operator should know.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl ConfigChange {
    /// Whether the operator must act before the release starts (a newly required key).
    #[must_use]
    pub fn is_required(&self) -> bool {
        self.change == "required"
    }
}

/// A migration a release applies, relative to the release before it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Migration {
    pub database: String,
    pub from: i64,
    pub to: i64,
}

/// One archive a `release.json` lists.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IndexAsset {
    pub target: String,
    pub file: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
}

/// A release's `release.json`: what the updater reads of it, with `D` the server's own
/// [`UpgradeDeclaration`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(bound(deserialize = "D: DeserializeOwned"))]
pub struct ReleaseIndex<D> {
    pub version: String,
    #[serde(default)]
    pub prerelease: bool,
    pub assets: Vec<IndexAsset>,
    /// Absent from releases made before declarations existed.
    #[serde(default = "Option::default")]
    pub upgrade: Option<D>,
}

impl<D: DeserializeOwned> ReleaseIndex<D> {
    /// Reads a `release.json`.
    ///
    /// # Errors
    /// When it is not one.
    pub fn parse(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("release.json cannot be read: {e}"))
    }

    /// The archive for `target`.
    #[must_use]
    pub fn asset_for(&self, target: &str) -> Option<&IndexAsset> {
        self.assets.iter().find(|a| a.target == target)
    }
}
