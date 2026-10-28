//! Plugins in the browser: packages the player added to the page, kept in the browser's private
//! file system, and each plugin's store kept there too.
//!
//! The added packages, and the plugins installed from plugin sources (kept as zips beside them,
//! [`crate::plugin_sources`]), are the plugins the page offers the client, as the launcher
//! offers the desktop client its own: each is ticked in the plugin panel (`/plugin`) the first
//! time the client sees it, and loads while it is ticked; one the player unticks stays off.
//!
//! The page keeps added plugins in its private file system, one folder per package or a
//! package's zip file as it came; the worker
//! reads them all before the client starts and hands them over as one bundle, in the settings
//! store's format ([`crate::settings::decode`]), each file named `<folder>/<path>`. A plugin's
//! store is a file in the client's own file store (`/plugin-store/<id>/<scope>.json`), which the
//! page keeps in the same private file system and saves after every write.

use std::collections::BTreeMap;
use std::path::PathBuf;

use dereth_client_runtime::platform::files;
use dereth_client_sdk::plugin_api::HostCapability;
use dereth_client_sdk::plugins::package::{is_package_root, MANIFEST_FILE};
use dereth_client_sdk::plugins::{
    HostConfig, Package, PluginHook, PluginHost, PluginStorage, StaticSource,
};

/// Where the stores are kept in the client's file store.
pub const STORE_DIR: &str = "/plugin-store";

/// The packages in a bundle: files grouped by their first path component, each group a package
/// when it holds a manifest (or is a script written for UtilityBelt's scripting, an `index.lua`);
/// and each zip file at the top, a package of its own. A zip that does
/// not read is left out (and logged).
#[must_use]
pub fn packages(bundle: &[u8]) -> Vec<Package> {
    let mut by_folder: BTreeMap<String, BTreeMap<String, Vec<u8>>> = BTreeMap::new();
    let mut zips = Vec::new();
    for (path, bytes) in crate::settings::decode(bundle) {
        let path = path.to_string_lossy().replace('\\', "/");
        let path = path.trim_start_matches('/');
        let Some((folder, rest)) = path.split_once('/') else {
            if path.to_ascii_lowercase().ends_with(".zip") {
                match Package::from_zip(&format!("browser:{path}"), &bytes) {
                    Ok(p) if is_package_root(|f| p.files.contains_key(f)) => zips.push(p),
                    Ok(_) => tracing::warn!("plugin {path}: no {MANIFEST_FILE} in the zip"),
                    Err(e) => tracing::warn!("plugin {path}: {e}"),
                }
            }
            continue;
        };
        by_folder
            .entry(folder.to_owned())
            .or_default()
            .insert(rest.to_owned(), bytes);
    }
    by_folder
        .into_iter()
        .filter(|(_, files)| is_package_root(|f| files.contains_key(f)))
        .map(|(folder, files)| Package {
            origin: format!("browser:{folder}"),
            files,
        })
        .chain(zips)
        .collect()
}

/// Stores as files in the client's file store.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileStorage;

impl FileStorage {
    fn path(plugin: &str, scope: &str) -> PathBuf {
        let safe = |s: &str| -> String {
            s.chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '.' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect()
        };
        PathBuf::from(STORE_DIR)
            .join(safe(plugin))
            .join(format!("{}.json", safe(scope)))
    }
}

impl PluginStorage for FileStorage {
    fn load(&mut self, plugin: &str, scope: &str) -> Option<String> {
        files::read_to_string(&Self::path(plugin, scope)).ok()
    }

    fn save(&mut self, plugin: &str, scope: &str, text: &str) {
        if let Err(e) = files::write(&Self::path(plugin, scope), text) {
            tracing::warn!("plugin store {plugin} {scope}: {e}");
        }
    }
}

/// What the page hands the host beyond the packages: its bus between tabs, its fetch, and the
/// world's policy document if the page was given one.
#[derive(Debug, Default)]
pub struct Extras {
    pub peers: Option<Box<dyn dereth_client_sdk::plugins::PluginPeers>>,
    pub net: Option<Box<dyn dereth_client_sdk::plugins::PluginNet>>,
    pub policy: Option<String>,
}

/// The browser's plugin host over `packages`, the ones kept in the page: the page offers them, each
/// ticked the first time the client sees it. It offers what the desktop offers: the page draws the
/// same interface.
#[must_use]
pub fn host(packages: Vec<Package>, extras: Extras) -> PluginHost {
    let mut caps = vec![
        HostCapability::Core,
        HostCapability::UiWindow,
        HostCapability::UiHud,
        HostCapability::OverlayWorld,
        HostCapability::OverlayArrow,
        HostCapability::OverlayRadar,
        HostCapability::OverlayMap,
        HostCapability::MapClicks,
        HostCapability::NavMoveto,
        HostCapability::NavJourney,
        HostCapability::NavPath,
        HostCapability::SessionPregame,
        HostCapability::Messages,
        HostCapability::UiIcons,
        HostCapability::RetailUi,
        HostCapability::GameData,
    ];
    if extras.peers.is_some() {
        caps.push(HostCapability::PeersLocal);
    }
    if extras.net.is_some() {
        caps.push(HostCapability::NetHttp);
    }
    let mut host = PluginHost::new(
        HostConfig::new("dereth-web", caps),
        Box::new(dereth_client_sdk::plugin_lua::LuauEngine),
        Box::new(StaticSource::default()),
        Box::new(FileStorage),
    )
    .with_offered(Box::new(StaticSource { packages }));
    // Compiled plugins run in the browser's own WebAssembly.
    #[cfg(target_arch = "wasm32")]
    {
        host = host.with_engine(Box::new(dereth_client_sdk::plugin_wasm::WasmEngine::new(
            std::rc::Rc::new(crate::plugin_wasm::BrowserWasm),
        )));
    }
    if let Some(p) = extras.peers {
        host = host.with_peers(p);
    }
    if let Some(n) = extras.net {
        host = host.with_net(n);
    }
    if let Some(text) = extras.policy {
        match dereth_client_sdk::plugins::policy_doc::parse(&text) {
            Ok(p) => host.set_world_policy(p),
            Err(e) => tracing::warn!("the world's plugin policy: {e}; the defaults apply"),
        }
    }
    host.load_all();
    host
}

/// The plugin engine's self-check: a VM made, a chunk run with the string library and a caught
/// error, then a runaway loop stopped by the time budget and a runaway allocation stopped by the
/// memory limit. Answers one line per step; a line that does not end in `ok` names the failure.
/// It proves the engine in the build it is run in, the browser's included.
#[must_use]
pub fn engine_check() -> Vec<String> {
    use dereth_client_sdk::plugin_api::{
        CallError, HostCalls, Limits, ScriptEngine, ScriptErrorKind, Value,
    };

    #[derive(Debug, Default)]
    struct Console(Vec<String>);
    impl HostCalls for Console {
        fn call(&mut self, op: &str, _: &[Value]) -> Result<Value, CallError> {
            Err(CallError::Unknown(op.to_owned()))
        }
        fn console(&mut self, line: &str) {
            self.0.push(line.to_owned());
        }
        fn module_source(&mut self, _: &str) -> Option<String> {
            None
        }
    }

    let engine = dereth_client_sdk::plugin_lua::LuauEngine;
    let limits = Limits {
        memory_bytes: 4 << 20,
        call_budget_ms: 20.0,
        profile: false,
    };
    let mut out = Vec::new();
    let mut run = |what: &str, source: &str, want: Option<ScriptErrorKind>| {
        let mut host = Console::default();
        // The memory step fills the cap, which takes longer than a budget on a slow machine: its
        // time is not what it checks.
        let limits = if what == "memory" {
            Limits {
                call_budget_ms: 5000.0,
                ..limits
            }
        } else {
            limits
        };
        let result = engine
            .instantiate(limits)
            .and_then(|mut vm| vm.load(&mut host, "check", source));
        let line = match (result, want) {
            (Ok(()), None) => format!("{what}: {} ok", host.0.join(" / ")),
            (Err(e), Some(k)) if e.kind == k => format!("{what}: stopped ok"),
            (other, _) => format!("{what}: {other:?}"),
        };
        out.push(line);
    };
    run(
        "chunk",
        r#"local ok, err = pcall(function() error("boom") end)
           print(string.format("%d %.2f %s", 1 + 2, 3.14159, tostring(not ok and string.find(err, "boom") ~= nil)))"#,
        None,
    );
    run("budget", "while true do end", Some(ScriptErrorKind::Budget));
    run(
        "memory",
        "local t = {} for i = 1, 1e8 do t[i] = tostring(i) end",
        Some(ScriptErrorKind::Memory),
    );
    out
}

/// Install the host into the application's frame.
pub fn install(app: &mut crate::play::App, packages: Vec<Package>, extras: Extras) {
    let host = host(packages, extras)
        .with_game_data(Box::new(dereth_client_sdk::DatGameData(app.store.clone())));
    app.install_frame_hook(Box::new(PluginHook { host }));
}

/// Write every plugin store out now (the client is shutting down).
pub fn flush(app: &mut crate::play::App) {
    if let Some(hook) = app
        .frame_hook_mut()
        .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
    {
        hook.host.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::{packages, FileStorage, Package, STORE_DIR};
    use dereth_client_sdk::plugins::PluginStorage;
    use std::path::PathBuf;

    /// Behaviour: none (the browser's own plumbing: a bundle's folders become packages).
    #[test]
    fn a_bundle_of_folders_becomes_one_package_per_folder_with_a_manifest() {
        let mut files = crate::settings::Files::new();
        files.insert(PathBuf::from("xp/plugin.toml"), b"id = \"a.b\"".to_vec());
        files.insert(PathBuf::from("xp/lib/util.lua"), b"return 1".to_vec());
        files.insert(PathBuf::from("stray/readme.txt"), b"no manifest".to_vec());
        let list = packages(&crate::settings::encode(&files));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].origin, "browser:xp");
        assert_eq!(
            list[0].files.keys().cloned().collect::<Vec<_>>(),
            ["lib/util.lua", "plugin.toml"]
        );
    }

    /// Behaviour: none (the browser's own plumbing: a zip added to the page is a package).
    #[test]
    fn a_zip_at_the_top_of_a_bundle_is_a_package_and_a_broken_one_is_left_out() {
        let bytes = Package {
            origin: String::new(),
            files: [
                ("plugin.toml".to_owned(), b"id = \"a.b\"".to_vec()),
                ("main.lua".to_owned(), b"print(1)".to_vec()),
            ]
            .into(),
        }
        .to_zip();
        let mut files = crate::settings::Files::new();
        files.insert(PathBuf::from("xp-meter.zip"), bytes);
        files.insert(PathBuf::from("broken.zip"), b"not a zip".to_vec());
        files.insert(PathBuf::from("notes.txt"), b"not a package".to_vec());
        let list = packages(&crate::settings::encode(&files));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].origin, "browser:xp-meter.zip");
        assert_eq!(
            list[0].files.keys().cloned().collect::<Vec<_>>(),
            ["main.lua", "plugin.toml"]
        );
    }

    /// Behaviour: none (the engine's self-check, which the browser's build runs too).
    #[test]
    fn the_engine_self_check_passes_every_step() {
        let lines = super::engine_check();
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|l| l.ends_with(" ok")), "{lines:?}");
        assert_eq!(lines[0], "chunk: 3 3.14 true ok");
    }

    /// A plugin added to the page or installed there is offered to the client as the launcher
    /// offers the desktop client its own: on the first time the client finds it.
    /// Behaviour: plugins.load.a-plugin-a-launcher-offers-starts-on-and-an-untick-is-kept
    #[test]
    fn a_plugin_kept_in_the_page_is_on_the_first_time_the_client_finds_it() {
        crate::settings::install(&[], None);
        let kept = Package {
            origin: "browser:xp".to_owned(),
            files: [
                (
                    "plugin.toml".to_owned(),
                    b"id = \"a.b\"\nname = \"xp\"\nversion = \"1.0.0\"\napi = \"0.1\"\n".to_vec(),
                ),
                ("main.lua".to_owned(), b"print(1)".to_vec()),
            ]
            .into(),
        };
        let host = super::host(vec![kept], super::Extras::default());
        let listed: Vec<(String, bool)> = host.found().into_iter().map(|f| (f.id, f.on)).collect();
        assert_eq!(listed, [("a.b".to_owned(), true)]);
    }

    /// Behaviour: none (the browser's own plumbing: a store is a file in the client's store).
    #[test]
    fn a_store_is_kept_as_a_file_in_the_clients_file_store() {
        crate::settings::install(&[], None);
        let mut s = FileStorage;
        s.save("a.b", "character:W:acct:Name", "{\"n\":1}");
        assert_eq!(
            s.load("a.b", "character:W:acct:Name").as_deref(),
            Some("{\"n\":1}")
        );
        assert!(crate::settings::paths()
            .iter()
            .any(|p| p.starts_with(STORE_DIR) && p.ends_with("character_W_acct_Name.json")));
    }
}
