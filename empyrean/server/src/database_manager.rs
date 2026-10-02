// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/DatabaseManager.cs
//! Port of `Source/ACE.Database/DatabaseManager.cs`: the composition root of the three databases.
//!
//! ACE's static properties are fields of [`World`]:
//!
//! - `DatabaseManager.World` is `World.content` (`PackContent` over `world.pack`, opened by
//!   [`crate::world_pack`]);
//! - `DatabaseManager.Shard` is `World.shard`, a [`ShardHandle`] over
//!   `ShardDatabaseWithCaching<SqliteShard>` (`shard.db`);
//! - `DatabaseManager.Authentication` and `DatabaseManager.AutoPromoteNextAccountToAdmin` are
//!   `World.auth` ([`AuthHandle`], over `SqliteAuth` on `auth.db`);
//! - `DatabaseManager.ShardConfig` is the shard backend itself (`SqliteShard` implements
//!   `ShardConfigDatabase`); nothing reads it yet.
//!
//! `DatabaseManager.InitializationFailure` is [`initialize`]'s return value.
//!
//! Not ported: `DatabaseManager.CachedServerVersionAutoDetect`, which asks a MySQL server for its
//! version; the databases here are files, so there is no server to ask.
//!
//! # Paths (not ACE)
//!
//! The database files are named by `database.shard_db_path` and `database.auth_db_path` in
//! `empyrean.toml` (default `./shard.db` and `./auth.db`). They resolve as every path in
//! `empyrean.toml` does ([`empyrean_common::config_paths`]): `~` is the home directory, a relative path
//! is relative to the configuration file's folder (the working directory without a file), so a
//! service started from another directory opens the same files. No environment variable overrides
//! them. A file that does not exist is created with the current schema.
//!
//! # Start-up checks
//!
//! ACE's `Initialize` aborts the boot when the world database has instance GUIDs outside the static
//! range, or has no `human` weenie. Those checks run here when a real `world.pack` was loaded; with
//! empty content (no pack, or tests) they are skipped with a warning, so that an empty world still
//! boots.

use std::path::PathBuf;
use std::sync::Arc;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::Clock;
use empyrean_common::config_paths::PathBase;
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_entity::enums::AccessLevel;
use empyrean_store::{
    AuthDatabase, ShardDatabase, ShardDatabaseWithCaching, ShardHandle, SqliteAuth, SqliteShard,
};
use empyrean_world::world::AuthHandle;
use empyrean_world::World;

/// Not ACE: a database file's path: `config_key`, else `default` when it is blank, resolved under
/// `base`.
#[must_use]
pub fn database_path(config_key: &str, default: &str, base: &PathBase) -> PathBuf {
    base.resolve(if config_key.trim().is_empty() {
        default
    } else {
        config_key
    })
}

/// Not ACE: the shard database's path (`database.shard_db_path`, else `./shard.db`) under `base`.
#[must_use]
pub fn configured_shard_db_path(config: &MasterConfiguration, base: &PathBase) -> PathBuf {
    database_path(
        &config.database.shard_db_path,
        empyrean_common::database_configuration::DEFAULT_SHARD_DB_PATH,
        base,
    )
}

/// Not ACE: the authentication database's path (`database.auth_db_path`, else `./auth.db`) under
/// `base`.
#[must_use]
pub fn configured_auth_db_path(config: &MasterConfiguration, base: &PathBase) -> PathBuf {
    database_path(
        &config.database.auth_db_path,
        empyrean_common::database_configuration::DEFAULT_AUTH_DB_PATH,
        base,
    )
}

/// Not ACE: opens (creating when absent) the shard and authentication database files.
///
/// DIVERGE: ACE's `Exists(true)` retries a MySQL connection every 5 s until it succeeds; a file
/// that cannot be opened (bad path, permissions, not a database) will not start working by
/// waiting, so the caller aborts the boot instead.
///
/// # Errors
/// A file that cannot be opened or migrated, with its path.
pub fn open_databases(
    shard_path: &std::path::Path,
    auth_path: &std::path::Path,
    accounts: AccountDefaults,
    clock: Arc<dyn Clock>,
) -> Result<(SqliteShard, SqliteAuth), String> {
    let shard =
        SqliteShard::open(shard_path).map_err(|e| format!("{}: {e}", shard_path.display()))?;
    log::info!("Shard database: {}", shard_path.display());
    let auth = SqliteAuth::open(auth_path, accounts, clock)
        .map_err(|e| format!("{}: {e}", auth_path.display()))?;
    log::info!("Authentication database: {}", auth_path.display());
    Ok((shard, auth))
}

/// Not ACE: what `Initialize` needs besides the backends: ACE reads these from `ConfigManager` and
/// from the world database it opened.
#[derive(Debug, Clone)]
pub struct InitializeOptions {
    /// A real `world.pack` was loaded into `World.content` (the start-up checks run only then).
    pub world_content_loaded: bool,
    /// `ConfigManager.Config.Server.ShardPlayerBiotaCacheTime`, in minutes.
    pub shard_player_biota_cache_time: u32,
    /// `ConfigManager.Config.Server.ShardNonPlayerBiotaCacheTime`, in minutes.
    pub shard_non_player_biota_cache_time: u32,
    /// The clock of the shard cache and the queue timer.
    pub clock: Arc<dyn Clock>,
    /// Run shard jobs on the "Serialized Shard Database" thread (the server), or inline with each
    /// call (tests, which stay deterministic and thread-free).
    pub threaded: bool,
}

// ACE: DatabaseManager.Initialize
/// Installs the authentication database, checks the world database, and builds the serialized,
/// caching shard database. Returns ACE's `DatabaseManager.InitializationFailure`: `true` when a
/// start-up check failed, in which case the shard is not installed (ACE returns before building it)
/// and the caller aborts the boot.
pub fn initialize<S: ShardDatabase + 'static>(
    w: &mut World,
    authentication: Box<dyn AuthDatabase>,
    shard: S,
    options: &InitializeOptions,
) -> bool {
    let authentication = AuthHandle::new(authentication);
    authentication.lock().exists(true);

    if authentication
        .lock()
        .get_listof_accounts_by_access_level(AccessLevel::Admin)
        .is_empty()
    {
        log::warn!("Authentication Database does not contain any admin accounts. The next account to be created will automatically be promoted to an Admin account.");
        authentication.set_auto_promote_next_account_to_admin(true);
    } else {
        authentication.set_auto_promote_next_account_to_admin(false);
    }
    w.auth = authentication;

    w.content.exists(true);

    // DIVERGE: ACE always runs these two checks; with empty world content they are
    // skipped (with a warning), so a world with no world.pack, and every test world, still boots.
    if options.world_content_loaded {
        if !w.content.is_world_database_guid_range_valid() {
            log::error!("World Database contains instance GUIDs outside of static range which will prevent GuidManager from properly assigning GUIDs and can result in GUID exhaustion prematurely.");
            return true;
        }

        let player_weenie_load_test = w.content.get_cached_weenie_by_class_name("human");
        if player_weenie_load_test.is_none() {
            log::error!("World Database does not contain the weenie for human (1). Characters cannot be created or logged into until the missing weenie is restored.");
            return true;
        }

        // DIVERGE: the pack must be built for the configured era, since the era's rules assume
        // its content (ACE has one era).
        let pack_era = w.content.era();
        if pack_era != w.era.id {
            log::error!(
                "World database was built for era \"{pack_era}\", but the server is configured for era \"{}\" ([era] profile in empyrean.toml). Build world.pack for this era (empyrean-import --era {}) or set the profile to \"{pack_era}\".",
                w.era.id,
                w.era.id
            );
            return true;
        }

        // Not ACE (which logs nothing here): the evidence that the checks ran.
        log::info!("World database: start-up checks passed (instance GUIDs in the static range, the human weenie exists, built for era \"{pack_era}\")");
    } else {
        log::warn!("World database has no content: the start-up checks (instance GUID range, the human weenie) are skipped");
    }

    // By default, we hold on to player biotas a little bit longer to help with offline updates like pass-up xp, allegiance updates, etc...
    let shard_db = ShardDatabaseWithCaching::new(
        shard,
        Arc::clone(&options.clock),
        TimeSpan::from_minutes(f64::from(options.shard_player_biota_cache_time)),
        TimeSpan::from_minutes(f64::from(options.shard_non_player_biota_cache_time)),
    );
    let clock = Arc::clone(&options.clock);
    w.shard = if options.threaded {
        ShardHandle::new(Box::new(shard_db), clock)
    } else {
        ShardHandle::synchronous(Box::new(shard_db), clock)
    };

    w.shard.base_database().exists(true);

    false
}

// ACE: DatabaseManager.Start
/// Starts the "Serialized Shard Database" thread.
pub fn start(w: &mut World) {
    w.shard.start();
}

// ACE: DatabaseManager.Stop
/// Finishes the queued shard jobs and stops the database thread.
pub fn stop(w: &mut World) {
    w.shard.stop();
}
