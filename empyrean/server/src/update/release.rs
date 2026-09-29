//! What a build is (its [`Facts`]) and what a release says about upgrading to it (its
//! [`Declaration`], the `upgrade` object of the release's `release.json`).
//!
//! The facts are what an upgrade depends on and the code knows: the shard and authentication
//! schema versions this build brings its databases to, and the `world.pack` format and record
//! schema it reads. `empyrean-server --release-facts` prints them as JSON; the release packaging
//! derives the same numbers from the source and writes them, with what cannot be derived (the
//! oldest version that can upgrade straight to the release, and configuration keys removed or
//! newly required), into `release.json`.

use serde::{Deserialize, Serialize};

use super::version::Version;

/// The schema versions a build brings its databases to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Databases {
    pub shard: i64,
    pub auth: i64,
}

/// The `world.pack` a build reads: its container format and record schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldPack {
    pub format: u32,
    pub schema: u32,
}

/// What a build is, as far as an upgrade is concerned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    pub version: String,
    pub target: String,
    pub databases: Databases,
    pub world_pack: WorldPack,
}

impl Facts {
    /// This build's facts.
    #[must_use]
    pub fn this_build() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            target: target_triple(),
            databases: Databases {
                shard: empyrean_store::sqlite_shard::SHARD_SCHEMA.current(),
                auth: empyrean_store::sqlite_auth::AUTH_SCHEMA.current(),
            },
            world_pack: WorldPack {
                format: empyrean_content::pack::format::FORMAT_VERSION,
                schema: empyrean_content::pack::format::SCHEMA_VERSION,
            },
        }
    }

    /// As one line of JSON (what `--release-facts` prints).
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Reads [`Facts::to_json`]'s output.
    ///
    /// # Errors
    /// When `text` is not such JSON.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text.trim()).map_err(|e| format!("not a build's facts: {e}"))
    }
}

/// The target triple this build's release archive is named for: the one the release packaging
/// compiled it for, or, for a build of its own, the release target of this operating system and
/// architecture.
#[must_use]
pub fn target_triple() -> String {
    if let Some(t) = option_env!("EMPYREAN_BUILD_TARGET") {
        return t.to_owned();
    }
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "windows") => "x86_64-pc-windows-msvc".to_owned(),
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu".to_owned(),
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu".to_owned(),
        ("aarch64", "macos") => "aarch64-apple-darwin".to_owned(),
        ("x86_64", "macos") => "x86_64-apple-darwin".to_owned(),
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
    /// The dotted `empyrean.toml` key (`server.update`).
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

/// The `upgrade` object of a release's `release.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declaration {
    pub kind: Kind,
    /// The release before this one, when there is one.
    pub previous: Option<String>,
    /// The oldest version that can upgrade straight to this one.
    pub upgrades_from: String,
    /// The schema versions this release brings the databases to.
    pub databases: Databases,
    /// The world pack this release reads.
    pub world_pack: WorldPack,
    /// The migrations since the previous release.
    #[serde(default)]
    pub migrations: Vec<Migration>,
    /// Whether `world.pack` must be rebuilt since the previous release.
    #[serde(default)]
    pub world_pack_rebuild: bool,
    /// The configuration changes since the previous release.
    #[serde(default)]
    pub config: Vec<ConfigChange>,
    /// Free text for the operator.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

impl Declaration {
    /// [`Declaration::upgrades_from`], read.
    ///
    /// # Errors
    /// When it is not a version.
    pub fn upgrades_from(&self) -> Result<Version, String> {
        Version::parse(&self.upgrades_from)
    }
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

/// A release's `release.json`: what the updater reads of it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReleaseIndex {
    pub version: String,
    #[serde(default)]
    pub prerelease: bool,
    pub assets: Vec<IndexAsset>,
    /// Absent from releases made before declarations existed.
    #[serde(default)]
    pub upgrade: Option<Declaration>,
}

impl ReleaseIndex {
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
