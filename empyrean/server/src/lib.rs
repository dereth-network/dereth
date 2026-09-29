//! The Empyrean server: ACE's `Program` start-up around the world loop, configured by one
//! `empyrean.toml`.
//!
//! **Depends on** the server crates it starts: `empyrean-common`, `empyrean-entity`,
//! `empyrean-content`, `empyrean-store`, `empyrean-dat`, `empyrean-net`, `empyrean-world` and
//! `empyrean-command`, the shared `dereth-dat` for its search of the dat folder, and the shared
//! `dereth-transport` for the WebSocket frame; the updater adds an HTTP client (`ureq`, over
//! rustls) and the archive readers (`zip`, `flate2`). **Used by** nothing: it is the `empyrean-server`
//! binary, and this library half holds what the binary uses that its integration tests also reach.
//!
//! **Must never** be configured by anything but its configuration file and command line: no
//! environment variable configures the server, and ACE's `Config.js` is read only by
//! `--write-config --from`, which converts it once. The guide is `empyrean/SETUP.md`.
//!
//! [`config_file`] finds and loads `empyrean.toml`, [`dat_directory`] and [`world_pack`] locate the
//! data files and the content pack, [`database_manager`] opens the stores, [`guid_manager_boot`]
//! seeds the object-id allocator, [`status_endpoint`] serves the status query, and [`websocket`]
//! is the WebSocket endpoint, beside the UDP listeners. [`update`] finds, checks and installs new
//! releases (`server.update`, `empyrean-server update`). The ported `Program.*` modules stay in
//! the binary.

pub mod config_file;
pub mod dat_directory;
pub mod database_manager;
pub mod guid_manager_boot;
pub mod status_endpoint;
pub mod update;
pub mod websocket;
pub mod world_pack;
