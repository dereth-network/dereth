//! Plugins in the headless client: packages read from folders and zips, stores kept in a folder or
//! in memory, the host built with the Luau and the compiled plugins' engines, and the lines that
//! manage them.
//!
//! A package is a folder holding `plugin.toml`, or its zip. `--plugins <dir>` names one package or
//! a folder of packages, loaded at the start; `plugin load <dir>` loads one more.

use std::path::Path;

use auberean_client_sdk::plugin_lua::LuauEngine;
use auberean_client_sdk::plugin_wasm::WasmEngine;
use auberean_client_sdk::plugins::{bindings, HostConfig, PluginHost, PluginStorage, VmClock};

pub use auberean_client_sdk::plugin_folders::{
    read, read_package, DirSource, DirStorage, MachineClock,
};

/// A host for the headless client: Auberean's Luau engine and the compiled plugins' interpreter,
/// timing plugins on `clock`, keeping stores in `storage`, reloading from `source`.
#[must_use]
pub fn host(
    clock: Box<dyn VmClock>,
    storage: Box<dyn PluginStorage>,
    source: DirSource,
) -> PluginHost {
    let config = HostConfig::new("auberean-headless", env!("CARGO_PKG_VERSION"));
    let engine: LuauEngine = bindings::engine();
    PluginHost::new(config, clock, storage)
        .with_engine(Box::new(engine))
        .with_engine(Box::new(WasmEngine::interpreted()))
        .with_source(Box::new(source))
}

/// Carry out one line of the headless client's plugin language, answering what to print:
///
/// - `plugin load <dir or zip>`, `plugin unload <id>`, `plugin reload <id>` (each done at the next
///   frame), `plugin list`, `plugin console <id>`;
/// - `ui click <plugin> <window> <node>`, `ui open <plugin> <window>` (the player's press and
///   opening), `ui dump <plugin> <window>` (the window's tree);
/// - a line starting `/`: the host's own `/plugin` or a plugin's command.
///
/// # Errors
/// The line is none of these, or what it names is not there.
pub fn line(host: &mut PluginHost, text: &str) -> Result<Vec<String>, String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    match words.as_slice() {
        ["plugin", "load", path] => {
            host.request_load(read(Path::new(path))?);
            Ok(Vec::new())
        }
        ["plugin", "unload", id] => {
            host.request_unload(id);
            Ok(Vec::new())
        }
        ["plugin", "reload", id] => {
            host.request_reload(id);
            Ok(Vec::new())
        }
        ["plugin", "list"] => Ok(host
            .plugins()
            .into_iter()
            .map(|p| {
                let why = p.why.map_or(String::new(), |w| format!(": {w}"));
                let active = if p.active { "active" } else { "inactive" };
                format!("{} {} {} {active}{why}", p.id, p.version, p.status)
            })
            .collect()),
        ["plugin", "console", id] => host
            .console(id)
            .ok_or_else(|| format!("no plugin {id} is loaded")),
        ["ui", "click", plugin, window, node] => {
            host.click(plugin, window, node).map(|()| Vec::new())
        }
        ["ui", "open", plugin, window] => host.open_window(plugin, window).map(|()| Vec::new()),
        ["ui", "dump", plugin, window] => host
            .window(plugin, window)
            .map(|w| {
                let open = if w.open { "open" } else { "closed" };
                let mut out = vec![format!("{plugin} {window} ({open})")];
                out.extend(w.tree.dump().lines().map(str::to_owned));
                out
            })
            .ok_or_else(|| format!("{plugin} has no window {window}")),
        _ if text.starts_with('/') => {
            if host.command(text) {
                Ok(Vec::new())
            } else {
                Err(format!("no plugin claims {text}"))
            }
        }
        _ => Err(format!("not a plugin line: {text}")),
    }
}
