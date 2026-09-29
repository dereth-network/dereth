//! The port of ACE's `Source/ACE.Common`, with the .NET-semantics shims, clocks, configuration and
//! the not-ported instrument every server crate uses.
//!
//! **Depends on** the shared `dereth-primitives`, `dereth-protocol` and `dereth-transport`. **Used
//! by** every other server crate: `empyrean-entity`, `empyrean-tables`, `empyrean-content`,
//! `empyrean-store`, `empyrean-dat`, `empyrean-net`, `empyrean-world`, `empyrean-command`,
//! `empyrean-server`, `empyrean-import` and `empyrean-testkit`.
//!
//! **Must never** depend on another server crate, since every one of them depends on it, or on a
//! client-only (`dereth/`) crate (`cargo xtask separation`).
//!
//! Module map (ACE file → module):
//! * `ThreadSafeRandom.cs` → [`thread_safe_random`] (generator in [`random`]);
//! * `Time.cs` → [`time`] (clocks in [`clock`]);
//! * `DerethDateTime.cs` → [`dereth_date_time`];
//! * `ConfigManager.cs`, `MasterConfiguration.cs`, `GameConfiguration.cs`, `NetworkSettings.cs`,
//!   `AccountDefaults.cs`, `PreloadedLandblock.cs`, `DatabaseConfiguration.cs`,
//!   `OfflineConfiguration.cs`, `DDDConfiguration.cs` → the module of the same name in snake case
//!   (`ThreadConfiguration.cs` and `MySqlConfiguration.cs` are not ported: their settings are not
//!   part of Empyrean's configuration, see [`toml_config::REMOVED`]);
//! * `Extensions/*` → [`extensions`], `Performance/*` → [`performance`], `Cryptography/*` →
//!   [`cryptography`];
//! * ACE.Server's `ServerBuildInfo_Static.cs` / `_Dynamic.cs` → [`server_build_info`] (here so
//!   `empyrean-world` and `empyrean-command` can reach it).
//!
//! Not ACE: [`backups`] (the copies kept of a data file before it is upgraded or replaced),
//! [`brand`] (the server software's own names and addresses), [`dotnet`] (runtime
//! semantics), [`json`] (the `System.Text.Json` options), [`clock`], [`random`]'s generator,
//! [`math`] (portable transcendental functions), [`not_ported`](mod@not_ported), [`toml_config`] (`empyrean.toml`,
//! the native configuration), [`config_paths`] (how its paths resolve), [`test_paths`] (where
//! tests find inputs that are not committed) and [`world_release`] (the ACE-World release this build
//! is tested against, and where `empyrean-import fetch` caches it).

pub mod account_defaults;
pub mod backups;
pub mod brand;
pub mod clock;
pub mod config_manager;
pub mod config_paths;
pub mod cryptography;
pub mod database_configuration;
pub mod ddd_configuration;
pub mod dereth_date_time;
pub mod dotnet;
pub mod extensions;
pub mod game_configuration;
pub mod json;
pub mod master_configuration;
pub mod math;
pub mod network_settings;
pub mod not_ported;
pub mod offline_configuration;
pub mod performance;
pub mod preloaded_landblock;
pub mod random;
pub mod server_build_info;
pub mod test_paths;
pub mod thread_safe_random;
pub mod time;
pub mod toml_config;
pub mod vectors;
pub mod web_socket_settings;
pub mod world_release;
