//! Auberean's layer of the plugin API's Lua bindings: the game's own namespaces over the bindings
//! every game shares, as text ([`LAYER`]), and with the `lua` feature the Luau engine that runs
//! them (`engine`).

/// The layer's name, which names it in tracebacks.
pub const NAME: &str = "auberean";

/// The layer: a chunk run once per VM, after the shared bindings, with the API table and the
/// shared bindings' kit.
pub const LAYER: &str = include_str!("auberean.lua");

/// Auberean's bindings, for the Luau engine.
#[cfg(feature = "lua")]
pub const AUBEREAN: dereth_plugin_lua::Bindings = dereth_plugin_lua::Bindings {
    name: NAME,
    layer: LAYER,
    libraries: &[],
};

/// The Luau engine running Auberean's bindings.
#[cfg(feature = "lua")]
#[must_use]
pub const fn engine() -> dereth_plugin_lua::LuauEngine {
    dereth_plugin_lua::LuauEngine::new(AUBEREAN)
}
