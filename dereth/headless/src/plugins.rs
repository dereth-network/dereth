//! Plugins in the headless client: packages read from folders, stores kept in a folder or in
//! memory, and the host installed as the runtime's frame hook.
//!
//! A package is a folder holding `plugin.toml`. `--plugins <dir>` names either one package or a
//! folder of packages; `plugin load <dir>` in a script loads one more.

use std::path::{Path, PathBuf};

use dereth_client_sdk::plugin_api::{HostCapability, PluginId};
use dereth_client_sdk::plugins::package::{is_package_root, MANIFEST_FILE};
use dereth_client_sdk::plugins::{HostConfig, Package, PluginSource, PluginStorage};

/// The most files one folder package may hold before it is refused unread.
const MAX_FILES: usize = 2_000;

/// Read one package folder: every file under it, by `/`-separated relative path. Hidden files and
/// folders (a leading `.`) are skipped.
///
/// # Errors
/// The folder has no `plugin.toml`, holds too many files, or cannot be read.
pub fn read_package(dir: &Path) -> Result<Package, String> {
    if !is_package_root(|f| dir.join(f).is_file()) {
        return Err(format!("{} has no {MANIFEST_FILE}", dir.display()));
    }
    let mut files = std::collections::BTreeMap::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let entries = std::fs::read_dir(dir.join(&rel))
            .map_err(|e| format!("{}: {e}", dir.join(&rel).display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = rel.join(&name);
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                if files.len() >= MAX_FILES {
                    return Err(format!(
                        "{} holds more than {MAX_FILES} files",
                        dir.display()
                    ));
                }
                let bytes = std::fs::read(dir.join(&path))
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                let key = path
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                files.insert(key, bytes);
            }
        }
    }
    Ok(Package {
        origin: dir.display().to_string(),
        files,
    })
}

/// Read a zip package file.
///
/// # Errors
/// The file cannot be read, or is not a package's zip.
pub fn read_zip_package(file: &Path) -> Result<Package, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    Package::from_zip(&file.display().to_string(), &bytes).map_err(|e| e.to_string())
}

fn is_zip(p: &Path) -> bool {
    p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// Packages under folders: each root is a package itself (a folder or a zip), or holds packages
/// (folders and zips) one level down.
#[derive(Debug, Default, Clone)]
pub struct DirSource {
    pub roots: Vec<PathBuf>,
}

impl PluginSource for DirSource {
    fn list(&mut self) -> Vec<Package> {
        let mut out = Vec::new();
        let read = |p: &Path| {
            if is_zip(p) {
                read_zip_package(p)
            } else {
                read_package(p)
            }
        };
        for root in &self.roots {
            if is_package_root(|f| root.join(f).is_file()) || is_zip(root) {
                match read(root) {
                    Ok(p) => out.push(p),
                    Err(e) => tracing::warn!("plugin package {e}"),
                }
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
            for d in found {
                match read(&d) {
                    Ok(p) => out.push(p),
                    Err(e) => tracing::warn!("plugin package {e}"),
                }
            }
        }
        out
    }

    fn fetch(&mut self, id: &PluginId) -> Option<Package> {
        self.list()
            .into_iter()
            .find(|p| p.manifest().is_ok_and(|m| &m.id == id))
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

/// Each plugin's data folder, `<root>/<plugin id>/` (`--plugin-data`): files the player puts there
/// for it, and files it saves.
#[derive(Debug, Clone)]
pub struct DirFiles {
    pub root: PathBuf,
}

impl dereth_client_sdk::plugins::PluginFiles for DirFiles {
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

/// The host this client is: no windows drawn, but their trees kept, so `ui dump` and `ui click`
/// can test a plugin's window logic; the character list and logging in and out; a bus between
/// clients (in this process, or with `--peers <dir>` the headless clients naming that folder); and,
/// with a world (`--world`), the character's movement. Nothing is
/// drawn, so no overlays, and there is no network for plugins.
#[must_use]
pub fn host_config(world: bool) -> HostConfig {
    let mut caps = vec![
        HostCapability::Core,
        HostCapability::UiWindow,
        HostCapability::UiHud,
        HostCapability::SessionPregame,
        HostCapability::PeersLocal,
        HostCapability::Messages,
    ];
    if world {
        caps.push(HostCapability::NavMoveto);
        caps.push(HostCapability::NavJourney);
        caps.push(HostCapability::NavPath);
        caps.push(HostCapability::GameData);
    }
    HostConfig::new("dereth-headless", caps)
}

/// How long a client's port listing lives without being refreshed, in seconds.
const PEER_STALE_SECS: u64 = 60;

/// The bus between headless clients on this machine (`--peers <dir>`): each client binds a
/// loopback datagram socket and lists its port as a file in the folder; a message is sent to
/// every other listed port. Nothing leaves the machine.
#[derive(Debug)]
pub struct LoopbackPeers {
    socket: std::net::UdpSocket,
    dir: PathBuf,
    port: u16,
    refreshed: std::time::Instant,
}

impl LoopbackPeers {
    /// Bind and list this client in `dir`.
    ///
    /// # Errors
    /// The socket or the listing could not be made.
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        socket.set_nonblocking(true)?;
        let port = socket.local_addr()?.port();
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(format!("{port}.peer")), b"")?;
        Ok(Self {
            socket,
            dir: dir.to_owned(),
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

impl dereth_client_sdk::plugins::PluginPeers for LoopbackPeers {
    fn send(&mut self, m: &dereth_client_sdk::plugins::PeerMessage) {
        let text = dereth_client_sdk::plugin_api::Value::map()
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

    fn receive(&mut self) -> Vec<dereth_client_sdk::plugins::PeerMessage> {
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
                        .and_then(|t| {
                            dereth_client_sdk::plugin_api::Value::from_json(t).map_err(|_| ())
                        })
                    else {
                        continue;
                    };
                    let s = |k: &str| v.get(k).as_str().unwrap_or_default().to_owned();
                    out.push(dereth_client_sdk::plugins::PeerMessage {
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

#[cfg(test)]
mod tests {
    //! Behaviour: none (the headless client's own transport between its processes; the bus's
    //! behavior is pinned by `peers/message`).
    use super::LoopbackPeers;
    use dereth_client_sdk::plugins::{PeerMessage, PluginPeers};

    #[test]
    fn two_clients_naming_one_folder_hear_each_other_and_not_themselves() {
        let dir =
            std::env::temp_dir().join(format!("dereth-headless-peers-{}", std::process::id()));
        let mut a = LoopbackPeers::open(&dir).expect("bound");
        let mut b = LoopbackPeers::open(&dir).expect("bound");
        let m = PeerMessage {
            from: "Leader".into(),
            plugin: "network.dereth.tank".into(),
            channel: "fellow".into(),
            data: "{\"n\":1}".into(),
        };
        a.send(&m);
        let mut heard = Vec::new();
        for _ in 0..100 {
            heard.extend(b.receive());
            if !heard.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(heard, vec![m]);
        assert!(a.receive().is_empty(), "the sender does not hear itself");
        drop((a, b));
        let left = std::fs::read_dir(&dir).map(Iterator::count).unwrap_or(0);
        let _ = std::fs::remove_dir(&dir);
        assert_eq!(left, 0, "each client's listing goes with it");
    }
}
