//! The plugin store's backing: where each plugin's saved values live between sessions.
//!
//! The host keeps a plugin's store in memory while it runs and writes it out through a
//! [`PluginStorage`] on logout, on unload and every 60 seconds. The product implements the trait
//! over its own storage (a per-user folder, the browser's private file system, memory for tests);
//! what it stores is JSON text.

use std::collections::BTreeMap;

/// Where stores are kept. Keys are opaque strings the host builds: a plugin id and a scope key
/// (`character:<name>`).
pub trait PluginStorage: std::fmt::Debug {
    /// The saved text of one store, if any.
    fn load(&mut self, plugin: &str, scope: &str) -> Option<String>;

    /// Save one store's text.
    fn save(&mut self, plugin: &str, scope: &str, text: &str);
}

/// A store in memory: nothing survives the process. For tests and hosts with nowhere to write.
#[derive(Debug, Default, Clone)]
pub struct MemoryStorage {
    pub saved: BTreeMap<(String, String), String>,
    /// How many saves were made, for tests of the flush schedule.
    pub saves: usize,
}

impl PluginStorage for MemoryStorage {
    fn load(&mut self, plugin: &str, scope: &str) -> Option<String> {
        self.saved
            .get(&(plugin.to_owned(), scope.to_owned()))
            .cloned()
    }

    fn save(&mut self, plugin: &str, scope: &str, text: &str) {
        self.saves += 1;
        self.saved
            .insert((plugin.to_owned(), scope.to_owned()), text.to_owned());
    }
}

/// A plugin's data folder: files the player puts there for it (profiles, routes in other
/// programs' formats) and files it saves there for the player. Each plugin sees only its own;
/// paths are `/`-separated and relative.
pub trait PluginFiles: std::fmt::Debug {
    /// Every file in the plugin's folder, by path.
    fn list(&mut self, plugin: &str) -> Vec<String>;

    /// One file's bytes, if it is there.
    fn read(&mut self, plugin: &str, path: &str) -> Option<Vec<u8>>;

    /// Write one file, replacing what was there.
    ///
    /// # Errors
    /// The host could not write it.
    fn write(&mut self, plugin: &str, path: &str, bytes: &[u8]) -> Result<(), String>;
}

/// No data folder: nothing to read and nowhere to write.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoFiles;

impl PluginFiles for NoFiles {
    fn list(&mut self, _plugin: &str) -> Vec<String> {
        Vec::new()
    }

    fn read(&mut self, _plugin: &str, _path: &str) -> Option<Vec<u8>> {
        None
    }

    fn write(&mut self, _plugin: &str, _path: &str, _bytes: &[u8]) -> Result<(), String> {
        Err("this host keeps no plugin data folder".into())
    }
}

/// Data folders in memory, for tests.
#[derive(Debug, Default, Clone)]
pub struct MemoryFiles {
    pub files: BTreeMap<(String, String), Vec<u8>>,
}

impl PluginFiles for MemoryFiles {
    fn list(&mut self, plugin: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|(p, _)| p == plugin)
            .map(|(_, f)| f.clone())
            .collect()
    }

    fn read(&mut self, plugin: &str, path: &str) -> Option<Vec<u8>> {
        self.files
            .get(&(plugin.to_owned(), path.to_owned()))
            .cloned()
    }

    fn write(&mut self, plugin: &str, path: &str, bytes: &[u8]) -> Result<(), String> {
        self.files
            .insert((plugin.to_owned(), path.to_owned()), bytes.to_vec());
        Ok(())
    }
}
