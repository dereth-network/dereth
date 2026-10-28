//! Plugins on the desktop: packages found in the plugins folder in the settings folder, in each
//! `--offer-plugins` folder (the launcher's installs) and among the plugins that come with the
//! client, packages named with `--plugins`, stores kept beside them, and the host installed as the
//! runtime's frame hook.
//!
//! A package is a folder holding `plugin.toml`. A found plugin loads once it is enabled in the
//! plugin panel (`/plugin`); once loaded it may do what every permission covers. The plugins the
//! launcher offers (`--offer-plugins`: its installs and the player's own folders) are enabled the
//! first time the client sees them, and the others wait for the player's Enable; a Disable is kept
//! as an Enable is. A `--plugins` folder's plugins load at once: the command line names them. With
//! `--plugin-dev` a plugin is reloaded whenever one of its files changes. Plugins beside the
//! program are found too ([`bundled_dir_for`](crate::plugins::bundled_dir_for)): releases carry
//! none (players install Dereth's from its plugin source, through the launcher). A client built
//! from the repository finds what any client finds and no more: a developer names the samples'
//! folder with `--plugins`, or adds it as a folder on the launcher's Plugins page.
//!
//! The desktop offers every drawn capability (windows, HUDs, markers, the arrow, radar and map
//! marks), movement, the character list, a bus between the player's clients on this machine
//! ([`LoopbackPeers`](crate::plugins::LoopbackPeers): loopback datagrams, each client's port listed in the settings folder), and
//! HTTPS off the frame's thread ([`ThreadNet`](crate::plugins::ThreadNet)). The world's policy comes from `--plugin-policy`,
//! a file or an `https://` URL, which the launcher passes from the world's entry. Each plugin's
//! data folder ([`DirFiles`](crate::plugins::DirFiles)) is `plugin-data/<id>` in the settings
//! folder: the player puts files there for it (a profile, a route) and it saves files there.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use dereth_plugin_api::{HostCapability, PluginId};
use dereth_plugin_host::package::{is_package_root, MANIFEST_FILE};
use dereth_plugin_host::{
    HostConfig, Package, PluginHook, PluginHost, PluginSource, PluginStorage,
};
use dereth_plugin_host::{HttpRequest, HttpResponse, PeerMessage, PluginNet, PluginPeers};

/// The plugins folder's name in the settings folder, and of the folder of plugins that come with
/// the client, beside its program.
pub const PLUGINS_DIR_NAME: &str = "plugins";

/// The folder of the plugins that come with the client, for the client's program at `exe`: beside
/// the program (the Windows release's folder), among a macOS bundle's resources, or in the
/// AppImage's own folder of the app's files. None when there is none: a build of one's own, run
/// from a source checkout's `target` folder, finds none of the checkout's plugins by itself.
#[must_use]
pub fn bundled_dir_for(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    [
        dir.join(PLUGINS_DIR_NAME),
        dir.join("..").join("Resources").join(PLUGINS_DIR_NAME),
        dir.join("..")
            .join("lib")
            .join("Dereth")
            .join(PLUGINS_DIR_NAME),
    ]
    .into_iter()
    .find(|d| d.is_dir())
}

/// The plugin stores' folder's name in the settings folder.
pub const STORE_DIR_NAME: &str = "plugin-store";
/// The most files one package may hold before it is refused unread.
const MAX_FILES: usize = 2_000;

/// Every file of a package folder by `/`-separated relative path, with the newest modification
/// time among them. Hidden files and folders (a leading `.`) are skipped.
fn read_dir_package(dir: &Path) -> Result<(Package, Option<SystemTime>), String> {
    if !is_package_root(|f| dir.join(f).is_file()) {
        return Err(format!("{} has no {MANIFEST_FILE}", dir.display()));
    }
    let mut files = BTreeMap::new();
    let mut newest = None;
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let entries = std::fs::read_dir(dir.join(&rel)).map_err(|e| e.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = rel.join(&name);
            let meta = entry.metadata().map_err(|e| e.to_string())?;
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                if files.len() >= MAX_FILES {
                    return Err(format!(
                        "{} holds more than {MAX_FILES} files",
                        dir.display()
                    ));
                }
                newest = newest.max(meta.modified().ok());
                let bytes = std::fs::read(dir.join(&path)).map_err(|e| e.to_string())?;
                let key = path
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                files.insert(key, bytes);
            }
        }
    }
    Ok((
        Package {
            origin: dir.display().to_string(),
            files,
        },
        newest,
    ))
}

fn is_zip(p: &Path) -> bool {
    p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// A zip package file, with its modification time.
fn read_zip_package(file: &Path) -> Result<(Package, Option<SystemTime>), String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let when = std::fs::metadata(file).and_then(|m| m.modified()).ok();
    let p = Package::from_zip(&file.display().to_string(), &bytes).map_err(|e| e.to_string())?;
    Ok((p, when))
}

/// Packages under folders: each root is a package itself (a folder or a zip), or holds packages
/// (folders and zips) one level down.
#[derive(Debug, Default)]
pub struct DirSource {
    pub roots: Vec<PathBuf>,
    /// Each package's newest file time when last read, by id.
    seen: BTreeMap<String, SystemTime>,
}

impl DirSource {
    #[must_use]
    pub fn new(mut roots: Vec<PathBuf>) -> Self {
        // A folder named twice is read once: its packages would otherwise be offered twice, and
        // the second of each refused as already loaded.
        let mut seen = std::collections::BTreeSet::new();
        roots.retain(|r| seen.insert(r.clone()));
        Self {
            roots,
            seen: BTreeMap::new(),
        }
    }

    /// The package folders and zip files: each root, or what it holds one level down.
    fn package_dirs(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for root in &self.roots {
            if is_package_root(|f| root.join(f).is_file()) || is_zip(root) {
                out.push(root.clone());
                continue;
            }
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            let mut found: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| is_package_root(|f| p.join(f).is_file()) || is_zip(p))
                .collect();
            found.sort();
            out.extend(found);
        }
        out
    }

    fn read_all(&mut self) -> Vec<(Package, Option<SystemTime>)> {
        let mut out = Vec::new();
        for d in self.package_dirs() {
            let read = if is_zip(&d) {
                read_zip_package(&d)
            } else {
                read_dir_package(&d)
            };
            match read {
                Ok(p) => out.push(p),
                Err(e) => tracing::warn!("plugin package {e}"),
            }
        }
        out
    }
}

impl PluginSource for DirSource {
    fn list(&mut self) -> Vec<Package> {
        let all = self.read_all();
        for (p, t) in &all {
            if let (Ok(m), Some(t)) = (p.manifest(), t) {
                self.seen.insert(m.id.to_string(), *t);
            }
        }
        all.into_iter().map(|(p, _)| p).collect()
    }

    fn changed(&mut self) -> Vec<PluginId> {
        let mut out = Vec::new();
        for (p, t) in self.read_all() {
            let (Ok(m), Some(t)) = (p.manifest(), t) else {
                continue;
            };
            let before = self.seen.insert(m.id.to_string(), t);
            if before.is_some_and(|b| b != t) {
                out.push(m.id);
            }
        }
        out
    }
}

/// Stores as JSON files: `<root>/<plugin id>/<scope>.json`, a scope's `:` written as `_`.
#[derive(Debug, Clone)]
pub struct DirStorage {
    pub root: PathBuf,
}

impl DirStorage {
    fn path(&self, plugin: &str, scope: &str) -> PathBuf {
        let safe: String = scope
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '.' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.root.join(plugin).join(format!("{safe}.json"))
    }
}

impl PluginStorage for DirStorage {
    fn load(&mut self, plugin: &str, scope: &str) -> Option<String> {
        std::fs::read_to_string(self.path(plugin, scope)).ok()
    }

    fn save(&mut self, plugin: &str, scope: &str, text: &str) {
        let p = self.path(plugin, scope);
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&p, text) {
            tracing::warn!("plugin store {}: {e}", p.display());
        }
    }
}

/// The plugins' data folders' folder's name in the settings folder.
pub const DATA_DIR_NAME: &str = "plugin-data";

/// Each plugin's data folder, `<root>/<plugin id>/`: the files the player puts there for it and
/// the files it saves. Paths stay inside the folder (the host checks them before they get here).
#[derive(Debug, Clone)]
pub struct DirFiles {
    pub root: PathBuf,
}

impl dereth_plugin_host::PluginFiles for DirFiles {
    fn list(&mut self, plugin: &str) -> Vec<String> {
        let base = self.root.join(plugin);
        let mut out = Vec::new();
        let mut stack = vec![PathBuf::new()];
        while let Some(rel) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(base.join(&rel)) else {
                continue;
            };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                let path = rel.join(&name);
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    stack.push(path);
                } else {
                    out.push(
                        path.components()
                            .map(|c| c.as_os_str().to_string_lossy().into_owned())
                            .collect::<Vec<_>>()
                            .join("/"),
                    );
                }
            }
        }
        out
    }

    fn read(&mut self, plugin: &str, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(plugin).join(path)).ok()
    }

    fn write(&mut self, plugin: &str, path: &str, bytes: &[u8]) -> Result<(), String> {
        let p = self.root.join(plugin).join(path);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, bytes).map_err(|e| format!("{}: {e}", p.display()))
    }
}

/// The folder, in the settings folder, where each client lists its bus port.
pub const PEERS_DIR_NAME: &str = "plugin-peers";

/// How long a client's port listing lives without being refreshed, in seconds.
const PEER_STALE_SECS: u64 = 60;

/// The bus between the player's clients on this machine: each client binds a loopback datagram
/// socket and lists its port as a file in the settings folder's `plugin-peers` folder; a message
/// is sent to every other listed port. Nothing leaves the machine.
#[derive(Debug)]
pub struct LoopbackPeers {
    socket: std::net::UdpSocket,
    dir: PathBuf,
    port: u16,
    refreshed: std::time::Instant,
}

impl LoopbackPeers {
    /// Bind and list this client.
    ///
    /// # Errors
    /// The socket or the listing could not be made.
    pub fn open(settings_dir: &Path) -> std::io::Result<Self> {
        let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        socket.set_nonblocking(true)?;
        let port = socket.local_addr()?.port();
        let dir = settings_dir.join(PEERS_DIR_NAME);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(format!("{port}.peer")), b"")?;
        Ok(Self {
            socket,
            dir,
            port,
            refreshed: std::time::Instant::now(),
        })
    }

    /// The other clients' ports, dropping listings nobody has refreshed for a minute.
    fn others(&self) -> Vec<u16> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for e in entries.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(port) = name
                .strip_suffix(".peer")
                .and_then(|p| p.parse::<u16>().ok())
            else {
                continue;
            };
            if port == self.port {
                continue;
            }
            let stale = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_none_or(|age| age.as_secs() > PEER_STALE_SECS);
            if stale {
                let _ = std::fs::remove_file(e.path());
            } else {
                out.push(port);
            }
        }
        out
    }
}

impl Drop for LoopbackPeers {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.dir.join(format!("{}.peer", self.port)));
    }
}

impl PluginPeers for LoopbackPeers {
    fn send(&mut self, m: &PeerMessage) {
        let text = dereth_plugin_api::Value::map()
            .with("from", m.from.as_str())
            .with("plugin", m.plugin.as_str())
            .with("channel", m.channel.as_str())
            .with("data", m.data.as_str())
            .to_json();
        for port in self.others() {
            let _ = self
                .socket
                .send_to(text.as_bytes(), (std::net::Ipv4Addr::LOCALHOST, port));
        }
    }

    fn receive(&mut self) -> Vec<PeerMessage> {
        if self.refreshed.elapsed().as_secs() >= 10 {
            self.refreshed = std::time::Instant::now();
            let _ = std::fs::write(self.dir.join(format!("{}.peer", self.port)), b"");
        }
        let mut out = Vec::new();
        let mut buf = vec![0_u8; 65_536];
        for _ in 0..256 {
            match self.socket.recv_from(&mut buf) {
                Ok((n, from)) if from.ip().is_loopback() => {
                    let Ok(v) = std::str::from_utf8(&buf[..n])
                        .map_err(|_| ())
                        .and_then(|t| dereth_plugin_api::Value::from_json(t).map_err(|_| ()))
                    else {
                        continue;
                    };
                    let s = |k: &str| v.get(k).as_str().unwrap_or_default().to_owned();
                    out.push(PeerMessage {
                        from: s("from"),
                        plugin: s("plugin"),
                        channel: s("channel"),
                        data: s("data"),
                    });
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                // A datagram to a client that has gone comes back as a reset on some systems.
                Err(_) => {}
            }
        }
        out
    }
}

/// HTTPS off the frame's thread: each request on a thread of its own, its answer collected at a
/// later frame.
#[derive(Debug)]
pub struct ThreadNet {
    tx: std::sync::mpsc::Sender<(i64, Result<HttpResponse, String>)>,
    rx: std::sync::mpsc::Receiver<(i64, Result<HttpResponse, String>)>,
}

impl Default for ThreadNet {
    fn default() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self { tx, rx }
    }
}

/// The system's resolver, keeping only the addresses on the public internet: a plugin's request
/// never reaches this machine or a private network, whatever its host name resolves to, and the
/// connection is made to the address checked.
#[derive(Debug, Default)]
struct PublicOnly(ureq::unversioned::resolver::DefaultResolver);

impl ureq::unversioned::resolver::Resolver for PublicOnly {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let all = self.0.resolve(uri, config, timeout)?;
        let mut out = self.empty();
        for a in all
            .iter()
            .filter(|a| dereth_plugin_host::net::is_public_address(a.ip()))
        {
            out.push(*a);
        }
        if out.is_empty() {
            return Err(ureq::Error::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "the host resolves only to addresses of this machine or a private network",
            )));
        }
        Ok(out)
    }
}

/// One request, made now; a plugin's (`public_only`) reaches only addresses on the public
/// internet.
fn fetch(r: &HttpRequest, public_only: bool) -> Result<HttpResponse, String> {
    let mut config = ureq::Agent::config_builder()
        .user_agent(concat!("dereth-plugins/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .http_status_as_error(false)
        .max_redirects(0);
    if public_only {
        // Straight to the address checked: a proxy would resolve the host itself.
        config = config.proxy(None);
    }
    let config = config.build();
    let agent: ureq::Agent = if public_only {
        ureq::Agent::with_parts(
            config,
            ureq::unversioned::transport::DefaultConnector::default(),
            PublicOnly::default(),
        )
    } else {
        config.into()
    };
    let result = match (r.method.as_str(), &r.body) {
        ("GET", _) => {
            let mut q = agent.get(&r.url);
            for (k, v) in &r.headers {
                q = q.header(k, v);
            }
            q.call()
        }
        ("DELETE", _) => {
            let mut q = agent.delete(&r.url);
            for (k, v) in &r.headers {
                q = q.header(k, v);
            }
            q.call()
        }
        (m, body) => {
            let mut q = if m == "PUT" {
                agent.put(&r.url)
            } else {
                agent.post(&r.url)
            };
            for (k, v) in &r.headers {
                q = q.header(k, v);
            }
            q.send(body.clone().unwrap_or_default())
        }
    };
    let mut resp = result.map_err(|e| e.to_string())?;
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body = resp
        .body_mut()
        .with_config()
        .limit(dereth_plugin_host::net::MAX_BODY as u64)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    Ok(HttpResponse {
        status: resp.status().as_u16(),
        content_type,
        body,
    })
}

impl PluginNet for ThreadNet {
    fn start(&mut self, id: i64, request: HttpRequest) {
        let tx = self.tx.clone();
        let spawned = std::thread::Builder::new()
            .name("plugin-https".into())
            .spawn(move || {
                let _ = tx.send((id, fetch(&request, true)));
            });
        if let Err(e) = spawned {
            let _ = self.tx.send((id, Err(e.to_string())));
        }
    }

    fn poll(&mut self) -> Vec<(i64, Result<HttpResponse, String>)> {
        self.rx.try_iter().collect()
    }
}

/// The world's policy document from `--plugin-policy`: a file's text, or an `https://` URL's.
fn policy_text(source: &str) -> Result<String, String> {
    if source.starts_with("https://") {
        fetch_text(source)
    } else {
        std::fs::read_to_string(source).map_err(|e| format!("{source}: {e}"))
    }
}

/// A text a client switch names: an `http://` or `https://` URL's, or a file's.
///
/// # Errors
/// The fetch or the read failed, the server answered other than 200, or the text is not UTF-8.
pub fn fetch_text(source: &str) -> Result<String, String> {
    if source.starts_with("https://") || source.starts_with("http://") {
        let r = fetch(
            &HttpRequest {
                method: "GET".into(),
                url: source.to_owned(),
                headers: Vec::new(),
                body: None,
            },
            false,
        )?;
        if r.status != 200 {
            return Err(format!("{source} answered HTTP {}", r.status));
        }
        String::from_utf8(r.body).map_err(|_| format!("{source} is not text"))
    } else {
        std::fs::read_to_string(source).map_err(|e| format!("{source}: {e}"))
    }
}

/// What the desktop's plugin host is, and where it looks.
#[derive(Debug, Clone, Default)]
pub struct DesktopPlugins {
    /// The settings folder; its `plugins` folder is always where plugins are found, and
    /// `plugin-store` holds the stores.
    pub settings_dir: Option<PathBuf>,
    /// `--plugins` folders: their plugins load at once.
    pub extra_dirs: Vec<PathBuf>,
    /// `--offer-plugins` folders (the launcher's installs and the player's own folders): their
    /// plugins are found, each ticked the first time the client sees it.
    pub offered_dirs: Vec<PathBuf>,
    /// The plugins that come with the client ([`bundled_dir_for`]): found, after the others.
    pub bundled_dir: Option<PathBuf>,
    /// `--plugin-dev`.
    pub watch: bool,
    /// `--plugin-policy`: the world's policy document, a file or an `https://` URL.
    pub policy: Option<String>,
}

impl DesktopPlugins {
    /// The host: the `--plugins` packages queued for the first frame, and the found ones listed in
    /// this order: the plugins folder's, each off until the player ticks it; the launcher's, each
    /// ticked the first time the client sees it; and the ones that come with the client, each off
    /// until the player ticks it.
    #[must_use]
    pub fn host(&self) -> PluginHost {
        let mut own = Vec::new();
        if let Some(dir) = &self.settings_dir {
            let plugins = dir.join(PLUGINS_DIR_NAME);
            let _ = std::fs::create_dir_all(&plugins);
            own.push(plugins);
        }
        let storage: Box<dyn PluginStorage> = match &self.settings_dir {
            Some(dir) => Box::new(DirStorage {
                root: dir.join(STORE_DIR_NAME),
            }),
            None => Box::new(dereth_plugin_host::MemoryStorage::default()),
        };
        let mut config = HostConfig::new(
            "dereth-desktop",
            vec![
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
                HostCapability::PeersLocal,
                HostCapability::NetHttp,
                HostCapability::Messages,
                HostCapability::UiIcons,
                HostCapability::RetailUi,
                HostCapability::GameData,
            ],
        );
        config.watch = self.watch;
        // `DERETH_PLUGIN_PROFILE=1`: each Lua plugin writes where its time goes to its console
        // (and so to the log, `dereth::plugin` at debug) every half minute.
        config.limits.profile =
            std::env::var_os("DERETH_PLUGIN_PROFILE").is_some_and(|v| !v.is_empty() && v != "0");
        let mut host = PluginHost::new(
            config,
            Box::new(dereth_plugin_lua::LuauEngine),
            Box::new(DirSource::new(self.extra_dirs.clone())),
            storage,
        )
        .with_engine(Box::new(dereth_plugin_wasm::WasmEngine::interpreted()))
        .with_net(Box::new(ThreadNet::default()))
        .with_found(Box::new(DirSource::new(own)))
        .with_offered(Box::new(DirSource::new(self.offered_dirs.clone())))
        .with_found(Box::new(DirSource::new(
            self.bundled_dir.iter().cloned().collect(),
        )));
        if let Some(dir) = &self.settings_dir {
            host = host.with_files(Box::new(DirFiles {
                root: dir.join(DATA_DIR_NAME),
            }));
        }
        if let Some(dir) = &self.settings_dir {
            match LoopbackPeers::open(dir) {
                Ok(p) => host = host.with_peers(Box::new(p)),
                Err(e) => tracing::warn!("the plugin bus between clients is off: {e}"),
            }
        }
        if let Some(source) = &self.policy {
            match policy_text(source).and_then(|t| dereth_plugin_host::policy_doc::parse(&t)) {
                Ok(p) => host.set_world_policy(p),
                Err(e) => tracing::warn!("--plugin-policy: {e}; the defaults apply"),
            }
        }
        host.load_all();
        host
    }

    /// Install the host into the application's frame, reading the game's data through the
    /// application's data files.
    pub fn install(&self, app: &mut crate::app::CoreApp) {
        let host = self
            .host()
            .with_game_data(Box::new(StoreData(app.store.clone())));
        app.install_frame_hook(Box::new(PluginHook { host }));
    }
}

/// The retail data files, as the plugin host reads the game's data for plugins.
#[derive(Debug, Clone)]
struct StoreData(std::sync::Arc<dereth_dat::RetailDatStore>);

impl dereth_plugin_host::gamedata::PluginGameData for StoreData {
    fn read_cell(&self, id: u32) -> Option<Vec<u8>> {
        self.0.read_cell(dereth_primitives::DataId(id)).ok()
    }

    fn read_portal(&self, id: u32) -> Option<Vec<u8>> {
        self.0.read_portal(dereth_primitives::DataId(id)).ok()
    }
}

/// Write every plugin store out, at shutdown.
pub fn flush(app: &mut crate::app::CoreApp) {
    if let Some(hook) = app
        .frame_hook_mut()
        .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
    {
        hook.host.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::{DirSource, DirStorage};
    use dereth_plugin_host::{PluginSource, PluginStorage};

    /// Behaviour: none (the desktop's own transport: a plugin's request reaches no address of this
    /// machine or a private network).
    #[test]
    fn a_plugins_request_to_a_name_of_this_machine_is_not_made() {
        use ureq::unversioned::resolver::Resolver as _;
        let uri: ureq::http::Uri = "https://localhost:443/".parse().unwrap();
        let config = ureq::config::Config::default();
        let timeout = ureq::unversioned::transport::NextTimeout {
            after: ureq::unversioned::transport::time::Duration::NotHappening,
            reason: ureq::Timeout::Resolve,
        };
        let refused = super::PublicOnly::default().resolve(&uri, &config, timeout);
        assert!(
            matches!(&refused, Err(ureq::Error::Io(e)) if e.kind() == std::io::ErrorKind::PermissionDenied),
            "{refused:?}"
        );
        let r = super::fetch(
            &dereth_plugin_host::HttpRequest {
                method: "GET".into(),
                url: "https://localhost:1/".into(),
                headers: Vec::new(),
                body: None,
            },
            true,
        );
        assert!(
            r.as_ref().is_err_and(|e| e.contains("private network")),
            "{r:?}"
        );
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dereth-plugins-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("scratch");
        d
    }

    #[test]
    fn a_folder_of_packages_is_read_and_an_edited_one_is_reported_changed() {
        let root = scratch("source");
        let pkg = root.join("xp");
        std::fs::create_dir_all(pkg.join(".git")).expect("dirs");
        std::fs::write(
            pkg.join("plugin.toml"),
            "id = \"t.xp\"\nname = \"X\"\nversion = \"1.0.0\"\napi = \"0.1\"\n",
        )
        .expect("write");
        std::fs::write(pkg.join("main.lua"), "print(1)").expect("write");
        std::fs::write(pkg.join(".git/HEAD"), "x").expect("write");
        let mut s = DirSource::new(vec![root.clone()]);
        let list = s.list();
        assert_eq!(list.len(), 1);
        assert_eq!(
            list[0].files.keys().cloned().collect::<Vec<_>>(),
            ["main.lua", "plugin.toml"]
        );
        assert!(s.changed().is_empty());
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        let f = std::fs::File::options()
            .write(true)
            .open(pkg.join("main.lua"))
            .expect("open");
        f.set_modified(later).expect("touch");
        assert_eq!(
            s.changed()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["t.xp"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn two_clients_on_the_loopback_bus_hear_each_other_and_not_themselves() {
        use dereth_plugin_host::{PeerMessage, PluginPeers};
        let root = scratch("peers");
        let mut a = super::LoopbackPeers::open(&root).expect("a");
        let mut b = super::LoopbackPeers::open(&root).expect("b");
        let m = PeerMessage {
            from: "Leader".into(),
            plugin: "network.dereth.follow".into(),
            channel: "where".into(),
            data: "{\"n\":1}".into(),
        };
        a.send(&m);
        // Polled until it arrives; the bound only ends a hang.
        let give_up = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut heard = Vec::new();
        while heard.is_empty() && std::time::Instant::now() < give_up {
            heard.extend(b.receive());
            if heard.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        assert_eq!(heard, vec![m]);
        assert!(a.receive().is_empty(), "the sender does not hear itself");
        drop(a);
        drop(b);
        assert_eq!(
            std::fs::read_dir(root.join(super::PEERS_DIR_NAME))
                .map(Iterator::count)
                .unwrap_or(0),
            0,
            "each client takes its listing down"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_store_round_trips_through_its_file() {
        let root = scratch("store");
        let mut s = DirStorage { root: root.clone() };
        s.save("t.xp", "character:Aldric", "{\"n\":1}");
        assert_eq!(
            s.load("t.xp", "character:Aldric").as_deref(),
            Some("{\"n\":1}")
        );
        assert!(root.join("t.xp").join("character_Aldric.json").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The plugins that come with the client are found where each release puts them for its
    /// program: beside it (the Windows folder), among a macOS bundle's resources, in the
    /// AppImage's own folder of the app's files; a build with none finds none, and a build of
    /// one's own in a source checkout finds none of the checkout's plugins by itself.
    /// Behaviour: plugins.load.a-found-plugin-is-listed-off-until-the-player-ticks-it
    #[test]
    fn the_plugins_that_come_with_the_client_are_found_where_each_release_puts_them() {
        let root = scratch("bundled-dirs");
        let windows = root.join("dereth-0.5.0");
        std::fs::create_dir_all(windows.join("plugins")).expect("dirs");
        assert_eq!(
            super::bundled_dir_for(&windows.join("dereth-client.exe")),
            Some(windows.join("plugins"))
        );
        let mac = root.join("Dereth.app").join("Contents");
        std::fs::create_dir_all(mac.join("MacOS")).expect("dirs");
        std::fs::create_dir_all(mac.join("Resources").join("plugins")).expect("dirs");
        let found = super::bundled_dir_for(&mac.join("MacOS").join("dereth-client"));
        assert!(found.is_some_and(|d| d.ends_with("Resources/plugins") && d.is_dir()));
        let appimage = root.join("squashfs-root").join("usr");
        std::fs::create_dir_all(appimage.join("bin")).expect("dirs");
        std::fs::create_dir_all(appimage.join("lib").join("Dereth").join("plugins")).expect("dirs");
        let found = super::bundled_dir_for(&appimage.join("bin").join("dereth-client"));
        assert!(found.is_some_and(|d| d.ends_with("lib/Dereth/plugins") && d.is_dir()));
        let own_build = root.join("target").join("release");
        std::fs::create_dir_all(&own_build).expect("dirs");
        assert_eq!(
            super::bundled_dir_for(&own_build.join("dereth-client.exe")),
            None
        );
        // A build of one's own in a source checkout's `target` folder finds none of the
        // checkout's plugins, for its profile's folder and for a target's: it finds what any
        // client finds.
        let checkout = root.join("checkout");
        std::fs::create_dir_all(checkout.join("dereth").join("plugins")).expect("dirs");
        for build in [
            checkout.join("target").join("release"),
            checkout
                .join("target")
                .join("x86_64-pc-windows-msvc")
                .join("release"),
        ] {
            std::fs::create_dir_all(&build).expect("dirs");
            assert_eq!(
                super::bundled_dir_for(&build.join("dereth-client.exe")),
                None
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// One frame of `host`, at `t` seconds, with no character in the world: answers the commands
    /// the host claims.
    fn step(host: &mut dereth_plugin_host::PluginHost, t: f64) -> Vec<String> {
        let view = dereth_client_contract::EmptyGameView;
        let world = dereth_client_model::World::new();
        let table = dereth_client_model::cmd::CommandInterp::new();
        let mut frame = dereth_client_runtime::frame_hook::HookFrame::new(
            &view,
            &world,
            &table,
            t,
            t,
            &[],
            &[],
        );
        host.step(&mut frame);
        frame.into_output().verbs.unwrap_or_default()
    }

    /// A plugin package folder `dir`/`folder` of id `id`, active before the world too.
    fn plugin_at(dir: &std::path::Path, folder: &str, id: &str) {
        let at = dir.join(folder);
        std::fs::create_dir_all(&at).expect("dirs");
        std::fs::write(
            at.join("plugin.toml"),
            format!(
                "id = \"{id}\"\nname = \"{folder}\"\nversion = \"1.0.0\"\napi = \"0.1\"\ncontexts = [\"pregame\", \"world\"]\n"
            ),
        )
        .expect("write");
        std::fs::write(at.join("main.lua"), "print('up')").expect("write");
    }

    /// Every way the desktop finds a plugin (its settings folder's `plugins`, a folder the
    /// launcher offers with `--offer-plugins`, the plugins that come with the client) lists it,
    /// and `/plugin` is the client's from the first frame, with nothing to turn on first. The
    /// launcher's plugin loads at the first start with no tick; the others wait for theirs, and a
    /// `--plugins` folder's plugin loads at once. Each tick and untick is kept in the settings
    /// folder for the next start: the launcher's plugin, unticked, stays off though the launcher
    /// offers it still.
    /// Behaviour: plugins.load.a-plugin-a-launcher-offers-starts-on-and-an-untick-is-kept
    #[test]
    fn the_launchers_plugins_load_at_first_and_the_others_wait_for_their_tick() {
        let root = scratch("found-sources");
        let settings = root.join("settings");
        plugin_at(&settings.join("plugins"), "mine", "t.mine");
        let offered = root.join("launcher");
        plugin_at(&offered, "installed", "t.installed");
        let bundled = root.join("bundled");
        plugin_at(&bundled, "sample", "t.sample");
        let named = root.join("named");
        plugin_at(&named, "asked", "t.asked");
        let desktop = super::DesktopPlugins {
            settings_dir: Some(settings),
            extra_dirs: vec![named],
            offered_dirs: vec![offered],
            bundled_dir: Some(bundled),
            ..super::DesktopPlugins::default()
        };
        let loaded = |h: &dereth_plugin_host::PluginHost| -> Vec<String> {
            h.plugins().into_iter().map(|p| p.id).collect()
        };
        let listed = |h: &dereth_plugin_host::PluginHost| -> Vec<(String, bool)> {
            h.found().into_iter().map(|f| (f.id, f.on)).collect()
        };
        let mut host = desktop.host();
        let verbs = step(&mut host, 1.0);
        assert!(
            verbs.iter().any(|v| v == dereth_plugin_host::HOST_VERB),
            "{verbs:?}"
        );
        for t in 2..4 {
            step(&mut host, f64::from(t));
        }
        assert_eq!(loaded(&host), ["t.asked", "t.installed"]);
        assert_eq!(
            listed(&host),
            [
                ("t.mine".to_owned(), false),
                ("t.installed".to_owned(), true),
                ("t.sample".to_owned(), false)
            ]
        );
        // The plugins folder's, enabled: it loads.
        assert!(host.set_ticked("t.mine", true).is_ok());
        step(&mut host, 5.0);
        assert_eq!(loaded(&host), ["t.asked", "t.installed", "t.mine"]);
        // The launcher's, disabled: it unloads.
        assert!(host.set_ticked("t.installed", false).is_ok());
        step(&mut host, 6.0);
        step(&mut host, 7.0);
        assert_eq!(loaded(&host), ["t.asked", "t.mine"]);
        drop(host);
        // The next start keeps both as the player left them.
        let mut again = desktop.host();
        step(&mut again, 1.0);
        step(&mut again, 2.0);
        assert_eq!(loaded(&again), ["t.asked", "t.mine"]);
        assert_eq!(
            listed(&again),
            [
                ("t.mine".to_owned(), true),
                ("t.installed".to_owned(), false),
                ("t.sample".to_owned(), false)
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
