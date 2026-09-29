// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/GameConfiguration.cs
//! `GameConfiguration` (`Config.js` → `Server`).

use serde::{Deserialize, Serialize};

use crate::account_defaults::AccountDefaults;
use crate::json;
use crate::network_settings::NetworkSettings;
use crate::preloaded_landblock::PreloadedLandblocks;
use crate::web_socket_settings::WebSocketSettings;

/// Not ACE: the default [`GameConfiguration::world_pack_path`].
pub const DEFAULT_WORLD_PACK_PATH: &str = "./world.pack";

// ACE: GameConfiguration
/// The `Server` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameConfiguration {
    // ACE: GameConfiguration.WorldName
    /// The world's name.
    #[serde(rename = "WorldName")]
    pub world_name: String,

    // ACE: GameConfiguration.Network
    /// Listener settings.
    #[serde(rename = "Network")]
    pub network: NetworkSettings,

    /// Not ACE: the WebSocket endpoint, beside the UDP listeners.
    /// DIVERGE: an extra section; ACE speaks UDP only.
    #[serde(rename = "WebSocket")]
    pub web_socket: WebSocketSettings,

    // ACE: GameConfiguration.Accounts
    /// Account defaults.
    #[serde(rename = "Accounts")]
    pub accounts: AccountDefaults,

    // ACE: GameConfiguration.DatFilesDirectory
    /// The directory holding the client dat files. Empty: the working directory, then the server
    /// executable's directory.
    #[serde(rename = "DatFilesDirectory")]
    pub dat_files_directory: String,

    // ACE: GameConfiguration.ShutdownInterval
    /// Seconds to wait before shutting down (default 60).
    #[serde(rename = "ShutdownInterval", deserialize_with = "json::num_u32")]
    pub shutdown_interval: u32,

    // ACE: GameConfiguration.ServerPerformanceMonitorAutoStart
    /// Start the performance monitor with the server.
    #[serde(rename = "ServerPerformanceMonitorAutoStart")]
    pub server_performance_monitor_auto_start: bool,

    // ACE: GameConfiguration.ShardPlayerBiotaCacheTime
    /// Minutes a player's biota stays cached (default 31).
    #[serde(
        rename = "ShardPlayerBiotaCacheTime",
        deserialize_with = "json::num_u32"
    )]
    pub shard_player_biota_cache_time: u32,

    // ACE: GameConfiguration.ShardNonPlayerBiotaCacheTime
    /// Minutes a non-player biota stays cached (default 11).
    #[serde(
        rename = "ShardNonPlayerBiotaCacheTime",
        deserialize_with = "json::num_u32"
    )]
    pub shard_non_player_biota_cache_time: u32,

    // ACE: GameConfiguration.LandblockPreloading
    /// Honour `PreloadedLandblocks`.
    #[serde(rename = "LandblockPreloading")]
    pub landblock_preloading: bool,

    // ACE: GameConfiguration.PreloadedLandblocks
    /// Landblocks to load at startup; a list in the file replaces this default list entirely.
    #[serde(rename = "PreloadedLandblocks")]
    pub preloaded_landblocks: Vec<PreloadedLandblocks>,

    /// Not ACE: the world database file (`world.pack`), which replaces ACE's `MySql.World`
    /// connection. Resolved as every configured path is (`config_paths`); the default value is
    /// also looked for beside the server executable.
    /// DIVERGE: an extra key; ACE reads the world database from MySQL.
    #[serde(rename = "WorldPackPath")]
    pub world_pack_path: String,

    /// Not ACE: the content overlay, an SQLite file that ACE's developer content
    /// commands (`import-sql`, `createinst`, …) write to and the world database reads before
    /// `world.pack`. Empty: no overlay, and those commands report that none is configured.
    /// DIVERGE: an extra key; ACE's commands write to the MySQL world database.
    #[serde(rename = "WorldOverlayPath")]
    pub world_overlay_path: String,

    /// Not ACE: the ACE world-database SQL dump `world.pack` was built from. The
    /// overlay loads it on its first write, so a write runs exactly as an `empyrean-import` patch does.
    #[serde(rename = "WorldBaseSql")]
    pub world_base_sql: String,

    /// Not ACE: the `empyrean-import` content inputs `world.pack` was built from, after
    /// the dump, in order: a file or folder of SQL patches, or `json:<path>` for ACE JSON content.
    #[serde(rename = "WorldBasePatches")]
    pub world_base_patches: Vec<String>,

    /// Not ACE: the server log's level: `error`, `warn`, `info` (the default), `debug` or `trace`.
    /// Any other value is warned about at startup and logs at `info`.
    /// DIVERGE: an extra key; ACE's log levels live in its log4net configuration file.
    #[serde(rename = "LogLevel")]
    pub log_level: String,

    /// Not ACE: the address (`ip:port`) of the HTTP status endpoint (empyrean-server); empty leaves it
    /// off. The `--status` command-line option wins.
    /// DIVERGE: an extra key; ACE has no status endpoint.
    #[serde(rename = "StatusAddress")]
    pub status_address: String,

    /// Not ACE: whether the server reads console commands from its standard input (default true).
    /// DIVERGE: ACE turns its console off with the environment variable `ACE_NONINTERACTIVE_CONSOLE`.
    #[serde(rename = "InteractiveConsole")]
    pub interactive_console: bool,

    /// Not ACE: where this server's source is offered to its players (the login welcome, `@source`,
    /// the version report and the status endpoint). Empty means the build's own repository
    /// (`brand::SOURCE_URL`). An operator running modified code sets it to where that code is.
    /// DIVERGE: an extra key; ACE makes no source offer.
    #[serde(rename = "SourceUrl")]
    pub source_url: String,

    /// Not ACE: which new releases the server installs by itself: `off` (the default), `patch`
    /// (patch releases of this version's minor line) or `minor` (also minor releases, with their
    /// database migrations and world-pack rebuilds). Majors and pre-releases are only reported.
    /// Any other value is warned about at startup and means `off`.
    #[serde(rename = "Update")]
    pub update: String,

    /// Not ACE: hours between the server's checks for a new release while `update` is not `off`.
    #[serde(rename = "UpdateCheckHours", deserialize_with = "json::num_u32")]
    pub update_check_hours: u32,

    /// Not ACE: seconds of in-game warning (the shutdown countdown) before the server restarts
    /// into a new release.
    #[serde(rename = "UpdateWarningSeconds", deserialize_with = "json::num_u32")]
    pub update_warning_seconds: u32,

    /// Not ACE: where new releases are looked for: empty means the build's own GitHub repository;
    /// otherwise a GitHub repository address, or a mirror serving GitHub's releases API for one
    /// (`.../repos/<owner>/<name>`).
    #[serde(rename = "UpdateSource")]
    pub update_source: String,
}

/// Not ACE: the default [`GameConfiguration::log_level`].
pub const DEFAULT_LOG_LEVEL: &str = "info";

/// Not ACE: the default [`GameConfiguration::update`].
pub const DEFAULT_UPDATE: &str = "off";

fn preload(
    id: &str,
    description: &str,
    include_adjacents: bool,
    enabled: bool,
) -> PreloadedLandblocks {
    PreloadedLandblocks {
        id: Some(id.to_owned()),
        description: Some(description.to_owned()),
        permaload: true,
        include_adjacents,
        enabled,
    }
}

impl Default for GameConfiguration {
    fn default() -> Self {
        Self {
            // DIVERGE: ACE's default world name is its own name; ours is Empyrean's (brand). A configured name is untouched.
            world_name: crate::brand::DEFAULT_WORLD_NAME.to_owned(),
            network: NetworkSettings::default(),
            web_socket: WebSocketSettings::default(),
            accounts: AccountDefaults::default(),
            // DIVERGE: ACE's default is a Windows drive path; ours is empty, looked up in the working directory then beside the executable (brand, platform-neutral).
            dat_files_directory: String::new(),
            shutdown_interval: 60,
            server_performance_monitor_auto_start: false,
            shard_player_biota_cache_time: 31,
            shard_non_player_biota_cache_time: 11,
            landblock_preloading: true,
            preloaded_landblocks: vec![
                preload("E74EFFFF", "Hebian-To (Global Events)", false, true),
                preload("A9B4FFFF", "Holtburg", true, false),
                preload("DA55FFFF", "Shoushi", true, false),
                preload("7D64FFFF", "Yaraq", true, false),
                preload("0007FFFF", "Town Network", false, false),
                preload("00000000", "Apartment Landblocks", false, false),
            ],
            world_pack_path: DEFAULT_WORLD_PACK_PATH.to_owned(),
            world_overlay_path: String::new(),
            world_base_sql: String::new(),
            world_base_patches: Vec::new(),
            log_level: DEFAULT_LOG_LEVEL.to_owned(),
            status_address: String::new(),
            interactive_console: true,
            source_url: String::new(),
            update: DEFAULT_UPDATE.to_owned(),
            update_check_hours: 6,
            update_warning_seconds: 300,
            update_source: String::new(),
        }
    }
}
