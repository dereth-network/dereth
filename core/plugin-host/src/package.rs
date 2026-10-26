//! Packages: a plugin's files as the host receives them, and where they come from.
//!
//! The host does no I/O. A product reads a package (a folder, a zip, a file the player imported)
//! and hands the host its files as bytes through a [`PluginSource`]. The package itself (its
//! files, manifest and zip form) is `dereth-plugin-package`'s.

use dereth_plugin_api::PluginId;
pub use dereth_plugin_package::{
    is_package_root, toml_to_value, Package, PackageError, ALLOWED_EXTENSIONS, MANIFEST_FILE,
    MAX_BYTES, MAX_FILES, SCRIPT_FILE,
};

/// Where packages come from. The product implements it over its own storage.
pub trait PluginSource: std::fmt::Debug {
    /// Every package the source offers.
    fn list(&mut self) -> Vec<Package>;

    /// The ids of the packages whose files changed since the last call, for a host that reloads
    /// a plugin as its author edits it. A source that cannot tell answers none.
    fn changed(&mut self) -> Vec<PluginId> {
        Vec::new()
    }

    /// The current files of the package whose manifest has `id`, for a reload.
    fn fetch(&mut self, id: &PluginId) -> Option<Package> {
        self.list()
            .into_iter()
            .find(|p| p.manifest().is_ok_and(|m| &m.id == id))
    }
}

/// A source with a fixed set of packages, for tests and for hosts that hand the host packages
/// they read themselves.
#[derive(Debug, Default, Clone)]
pub struct StaticSource {
    pub packages: Vec<Package>,
}

impl PluginSource for StaticSource {
    fn list(&mut self) -> Vec<Package> {
        self.packages.clone()
    }
}
