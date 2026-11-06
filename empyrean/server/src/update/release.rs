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

use dereth_server_update::release::{BuildFacts, UpgradeDeclaration};

use super::version::Version;

pub use dereth_server_update::release::{ConfigChange, IndexAsset, Kind, Migration};

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
    dereth_server_update::release::target_triple(option_env!("EMPYREAN_BUILD_TARGET"))
}

impl BuildFacts for Facts {
    fn version(&self) -> &str {
        &self.version
    }

    fn target(&self) -> &str {
        &self.target
    }

    fn set_version(&mut self, version: &str) {
        version.clone_into(&mut self.version);
    }
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

impl UpgradeDeclaration<Facts> for Declaration {
    fn kind(&self) -> Kind {
        self.kind
    }

    fn upgrades_from(&self) -> &str {
        &self.upgrades_from
    }

    fn config(&self) -> &[ConfigChange] {
        &self.config
    }

    fn databases_older_than(&self, mine: &Facts) -> bool {
        self.databases.shard < mine.databases.shard || self.databases.auth < mine.databases.auth
    }

    fn migrations_from(&self, mine: &Facts) -> Vec<Migration> {
        let mut migrations = Vec::new();
        for (database, from, to) in [
            ("shard", mine.databases.shard, self.databases.shard),
            ("authentication", mine.databases.auth, self.databases.auth),
        ] {
            if to != from {
                migrations.push(Migration {
                    database: database.to_owned(),
                    from,
                    to,
                });
            }
        }
        migrations
    }

    fn world_pack_rebuild_from(&self, mine: &Facts) -> bool {
        self.world_pack != mine.world_pack
    }

    fn mismatch(&self, facts: &Facts) -> Option<String> {
        (facts.databases != self.databases || facts.world_pack != self.world_pack).then(|| {
            format!(
                "server reports databases {:?} and world pack {:?}, but its release declares {:?} and {:?}",
                facts.databases, facts.world_pack, self.databases, self.world_pack
            )
        })
    }
}

/// A release's `release.json`: what the updater reads of it.
pub type ReleaseIndex = dereth_server_update::release::ReleaseIndex<Declaration>;
