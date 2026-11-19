//! The desktop's plugins: the packages in the `plugins` folder beside the preferences, each a
//! folder or a zip, loaded as the client starts; their stores kept in `plugin-data` beside it; and
//! the host they run in, with the Luau engine and the compiled plugins' interpreter. The shell
//! installs the host for each session.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use auberean_client_sdk::driver::CLIENT_CLOCK_START;
use auberean_client_sdk::plugin_folders::{DirSource, DirStorage, MachineClock};
use auberean_client_sdk::plugin_wasm::WasmEngine;
use auberean_client_sdk::plugins::{bindings, HostConfig, PluginHost, PluginSource};

/// The folder beside the preferences the plugins are in, and the one their stores are kept in.
pub const PLUGINS: &str = "plugins";
pub const PLUGIN_DATA: &str = "plugin-data";

/// What went wrong with a plugin's package or store, logged.
fn told(what: &str) {
    tracing::warn!("{what}");
}

/// The plugin host over the plugins in `settings`' `plugins` folder, each asked to load; none
/// when there is no such folder.
pub fn host(settings: &Path) -> Option<PluginHost> {
    let folder = settings.join(PLUGINS);
    if !folder.is_dir() {
        return None;
    }
    let mut source = DirSource::new(vec![folder], told);
    let found = source.list();
    let storage = DirStorage {
        root: settings.join(PLUGIN_DATA),
        report: told,
    };
    let config = HostConfig::new("auberean", env!("CARGO_PKG_VERSION"));
    // The frame clock is the client's own, which reads this much as the program starts.
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    let mut host = PluginHost::new(config, Box::<MachineClock>::default(), Box::new(storage))
        .with_engine(Box::new(bindings::engine()))
        .with_engine(Box::new(WasmEngine::interpreted()))
        .with_source(Box::new(source))
        .with_wall_clock(unix - CLIENT_CLOCK_START);
    tracing::info!(plugins = found.len(), "plugins found");
    for p in found {
        host.request_load(p);
    }
    Some(host)
}
