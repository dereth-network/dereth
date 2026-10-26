//! The front door to the client crates: one dependency that re-exports the runtime, the model, the
//! contract, the network session, audio and the shared crates.
//!
//! **Depends on** every crate it re-exports: `dereth-client-runtime`, `dereth-client-model`,
//! `dereth-client-contract`, `dereth-client-net`, `dereth-audio`, and the shared crates their types
//! are expressed in (`dereth-primitives`, `dereth-dat`, `dereth-assets`, `dereth-physics`,
//! `dereth-animation`, `dereth-world-data`, `dereth-landscape`, `dereth-transport`,
//! `dereth-protocol`, `dereth-rules`), and the plugin host (`dereth-plugin-api`,
//! `dereth-plugin-host`, `dereth-plugin-wasm`, and with the `lua` feature `dereth-plugin-lua`). **Used by** the headless client (`dereth-headless`), and by
//! any other client or tool built on this code.
//!
//! **Must never** reach the product: nothing under `dereth/client/` is in any of its dependency
//! tables (`cargo xtask seams`, `seam: client-sdk deps`), so the device input (`dereth-input`) is
//! not part of it, and its code names no key or device message (`seam: action seam code`). A client
//! built on the SDK drives the player with actions. It adds no code of its own.
//!
//! Each crate is a module under its short name: [`runtime`], [`model`], [`contract`], [`net`],
//! [`audio`], and the shared [`primitives`] (with [`num`] and [`text`] also at the top), [`dat`],
//! [`assets`], [`physics`], [`animation`], [`world_data`], [`landscape`], [`transport`],
//! [`protocol`] and [`rules`]. [`actions`] is the action seam: the retail action vocabulary and the
//! handlers the runtime runs actions through. [`plugins`] is the plugin host, [`plugin_api`] the
//! plugin interface, [`plugin_wasm`] the compiled plugins' engine and, with the `lua` feature,
//! `plugin_lua` the Luau engine. Every re-export is the crate itself, so
//! `dereth_client_sdk::protocol::X` and `dereth_protocol::X` are the same type.

pub use dereth_animation as animation;
pub use dereth_assets as assets;
pub use dereth_audio as audio;
pub use dereth_client_contract as contract;
pub use dereth_client_model as model;
pub use dereth_client_net as net;
pub use dereth_client_runtime as runtime;
/// The action seam: the action vocabulary and the handlers the runtime runs actions through.
pub use dereth_client_runtime::actions;
pub use dereth_dat as dat;
pub use dereth_landscape as landscape;
pub use dereth_physics as physics;
pub use dereth_plugin_api as plugin_api;
pub use dereth_plugin_host as plugins;
#[cfg(feature = "lua")]
pub use dereth_plugin_lua as plugin_lua;
pub use dereth_plugin_wasm as plugin_wasm;
pub use dereth_primitives as primitives;
pub use dereth_primitives::num;
pub use dereth_primitives::text;
pub use dereth_protocol as protocol;
pub use dereth_rules as rules;
pub use dereth_transport as transport;
pub use dereth_world_data as world_data;

/// The retail data files, as the plugin host reads the game's data for plugins (`game.data`).
#[derive(Debug, Clone)]
pub struct DatGameData(pub std::sync::Arc<dereth_dat::RetailDatStore>);

impl dereth_plugin_host::gamedata::PluginGameData for DatGameData {
    fn read_cell(&self, id: u32) -> Option<Vec<u8>> {
        self.0.read_cell(dereth_primitives::DataId(id)).ok()
    }

    fn read_portal(&self, id: u32) -> Option<Vec<u8>> {
        self.0.read_portal(dereth_primitives::DataId(id)).ok()
    }
}
