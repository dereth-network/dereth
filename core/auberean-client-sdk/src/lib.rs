//! One dependency for whatever builds on the client: the contract (its actions, the session and
//! the events), the driver that runs the client's frames, and finding an install.
//!
//! **Depends on** the runtime (`auberean-client-runtime`) and the crates whose types it hands out:
//! the actions (`auberean-client-contract`), the model (`auberean-client-model`), the session
//! (`auberean-client-net`), the messages (`auberean-protocol`), the data files (`auberean-dat`),
//! the script machine (`auberean-script`) and localized text (`auberean-text`); the world without
//! a window (`auberean-client-sim`), and the world's records it is opened from (`auberean-world`,
//! `auberean-assets`); and the plugin host (`auberean-plugin-host`, on `dereth-plugin-api`,
//! `dereth-plugin-wasm`, and with the `lua` feature `dereth-plugin-lua`). **Used by** the
//! headless client (`auberean-headless`), the client's shell, scene and interface, and the
//! browser's client.
//!
//! What a host drives the game with:
//! - [`contract`]: the actions in, the session and the events out;
//! - [`driver`]: the [`Driver`] that owns the [`Client`] and runs its frames on a [`Clock`] and a
//!   [`Port`], and installs the frame's hook ([`runtime::frame_hook`]);
//! - [`sim`]: the world without a window, where the player's body walks by its controls
//!   ([`sim::Walk`], run each frame by [`Driver::frame_walking`]);
//! - [`dat`] and, with the `fs` feature, [`install`]: an install found and opened as the client's
//!   data and the world's;
//! - with the `net` feature, a native host's port and clock ([`udp::UdpPort`],
//!   [`host_clock::HostClock`]);
//! - [`plugins`]: the plugin host, installed as the frame's hook; [`plugin_api`] the plugin
//!   interface, [`plugin_wasm`] the compiled plugins' engine and, with the `lua` feature,
//!   `plugin_lua` the Luau engine; with the `fs` feature, plugins kept in folders
//!   (`plugin_folders`).
//!
//! The rest of the re-exports ([`runtime`], [`model`], [`net`], [`protocol`], [`script`], [`text`])
//! are what the window's front end still reads past the contract, until the game's views are part
//! of it; a program without a window names none of them (`cargo xtask seams`).
//!
//! **Must never** read a device, do I/O outside the `fs` and `net` features, or reach the client's
//! crates (`cargo xtask seams`).

pub mod driver;
#[cfg(feature = "net")]
pub mod host_clock;
#[cfg(feature = "fs")]
pub mod install;
#[cfg(feature = "fs")]
pub mod plugin_folders;
#[cfg(feature = "net")]
pub mod udp;

pub use auberean_client_contract as contract;
pub use auberean_client_model as model;
pub use auberean_client_net as net;
pub use auberean_client_runtime as runtime;
pub use auberean_client_sim as sim;
pub use auberean_dat as dat;
pub use auberean_plugin_host as plugins;
pub use auberean_protocol as protocol;
pub use auberean_script as script;
pub use auberean_text as text;
pub use dereth_plugin_api as plugin_api;
#[cfg(feature = "lua")]
pub use dereth_plugin_lua as plugin_lua;
pub use dereth_plugin_wasm as plugin_wasm;

pub use auberean_client_contract::event::GameEvent;
pub use auberean_client_contract::session::Phase;
pub use auberean_client_contract::{Action, Motion, NewCharacter};
pub use auberean_client_runtime::frame_hook::{FrameHook, HookFrame};
pub use auberean_client_runtime::{Client, ClientData, ClientSettings};
pub use driver::{no_icons, Clock, Driver, FixedStep, Port};
