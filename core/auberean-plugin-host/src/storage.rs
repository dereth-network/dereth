//! Where each plugin's saved values and its data folder live between sessions.
//!
//! The host keeps a plugin's store in memory while it runs and writes it out through a
//! [`PluginStorage`] when it unloads, when the player logs off and every minute; what it writes is
//! JSON text. A plugin's data folder (files the player puts there for it, and files it saves for
//! the player) is a [`PluginFiles`]. The product implements both over its own storage: a folder per
//! user, the browser's private files, memory for tests.

use std::collections::BTreeMap;

/// Where stores are kept. The keys are the host's: a plugin's id and a scope's key.
pub trait PluginStorage: core::fmt::Debug {
    /// The saved text of one store, if any.
    fn load(&mut self, plugin: &str, scope: &str) -> Option<String>;

    /// Save one store's text.
    fn save(&mut self, plugin: &str, scope: &str, text: &str);
}

/// Stores in memory: nothing outlives the host. For tests and hosts with nowhere to write.
#[derive(Debug, Default, Clone)]
pub struct MemoryStorage {
    pub saved: BTreeMap<(String, String), String>,
    /// How many saves were made.
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

/// A plugin's data folder. Each plugin sees only its own; paths are `/`-separated and relative.
pub trait PluginFiles: core::fmt::Debug {
    /// Every file in the plugin's folder, by path.
    fn list(&mut self, plugin: &str) -> Vec<String>;

    /// One file's bytes, if it is there.
    fn read(&mut self, plugin: &str, path: &str) -> Option<Vec<u8>>;

    /// Write one file, replacing what was there.
    ///
    /// # Errors
    /// The product could not write it.
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
