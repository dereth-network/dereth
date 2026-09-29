// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/DatabaseConfiguration.cs
//! `DatabaseConfiguration` (`empyrean.toml` → `[database]`; ACE's `Config.js` → `MySql`).
//!
//! DIVERGE: ACE's section holds three MySQL connections (`Authentication`, `Shard`, `World`).
//! Empyrean's databases are files: the shard and authentication databases are the SQLite files
//! named here, and the world database is `world.pack` (`Server.WorldPackPath`). The connections are
//! not part of the configuration.

use serde::{Deserialize, Serialize};

// ACE: DatabaseConfiguration
/// The database files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfiguration {
    /// Not ACE: the shard database file (SQLite), which replaces ACE's `Shard` connection.
    /// Resolved as every configured path is (`config_paths`): a relative path is relative to the
    /// configuration file's folder.
    #[serde(rename = "ShardDbPath")]
    pub shard_db_path: String,

    /// Not ACE: the authentication database file (SQLite), which replaces ACE's `Authentication`
    /// connection. Resolved as `ShardDbPath` is. It may name the same file as `ShardDbPath`.
    #[serde(rename = "AuthDbPath")]
    pub auth_db_path: String,
}

/// Not ACE: the default [`DatabaseConfiguration::shard_db_path`].
pub const DEFAULT_SHARD_DB_PATH: &str = "./shard.db";

/// Not ACE: the default [`DatabaseConfiguration::auth_db_path`].
pub const DEFAULT_AUTH_DB_PATH: &str = "./auth.db";

impl Default for DatabaseConfiguration {
    fn default() -> Self {
        Self {
            shard_db_path: DEFAULT_SHARD_DB_PATH.to_owned(),
            auth_db_path: DEFAULT_AUTH_DB_PATH.to_owned(),
        }
    }
}
