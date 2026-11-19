//! Plugins kept in folders on the player's machine, for a native product: packages read from
//! folders and zips, stores kept as files in a folder, and the machine's steady clock for the time
//! plugins take. The product builds its plugin host over them ([`crate::plugins`]).
//!
//! A package is a folder holding `plugin.toml`, or its zip.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dereth_plugin_api::PluginId;

use crate::plugins::package::{is_package_root, MANIFEST_FILE};
use crate::plugins::{Package, PluginSource, PluginStorage, VmClock};

/// The most files one folder package may hold before it is refused unread.
const MAX_FILES: usize = 2_000;

/// Read one package folder: every file under it, by `/`-separated relative path, hidden files and
/// folders (a leading `.`) left out.
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

fn is_zip(p: &Path) -> bool {
    p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// Read a package folder or zip.
///
/// # Errors
/// It cannot be read, or is no package.
pub fn read(p: &Path) -> Result<Package, String> {
    if is_zip(p) {
        let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
        Package::from_zip(&p.display().to_string(), &bytes).map_err(|e| e.to_string())
    } else {
        read_package(p)
    }
}

/// Packages under folders: each root is a package itself (a folder or a zip), or holds packages
/// one level down. A package that cannot be read is left out, and why is told to `report`.
#[derive(Debug, Clone)]
pub struct DirSource {
    pub roots: Vec<PathBuf>,
    pub report: fn(&str),
}

impl Default for DirSource {
    fn default() -> Self {
        Self::new(Vec::new(), |_| {})
    }
}

impl DirSource {
    /// The packages under `roots`, what cannot be read told to `report`.
    #[must_use]
    pub fn new(roots: Vec<PathBuf>, report: fn(&str)) -> Self {
        Self { roots, report }
    }
}

impl PluginSource for DirSource {
    fn list(&mut self) -> Vec<Package> {
        let mut out = Vec::new();
        for root in &self.roots {
            if is_package_root(|f| root.join(f).is_file()) || is_zip(root) {
                match read(root) {
                    Ok(p) => out.push(p),
                    Err(e) => (self.report)(&format!("plugin package {e}")),
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
                    Err(e) => (self.report)(&format!("plugin package {e}")),
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

/// Stores as JSON files: `<root>/<plugin id>/<scope>.json`, a scope's `:` written as `_`. A store
/// that cannot be written is told to `report`.
#[derive(Debug, Clone)]
pub struct DirStorage {
    pub root: PathBuf,
    pub report: fn(&str),
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
            (self.report)(&format!("plugin store {}: {e}", p.display()));
        }
    }
}

/// The machine's steady clock, for the time plugins take.
#[derive(Debug)]
pub struct MachineClock(Instant);

impl Default for MachineClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl VmClock for MachineClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}
