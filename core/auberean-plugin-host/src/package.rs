//! Packages: a plugin's files as the host receives them, and where they come from.
//!
//! The host does no I/O. A product reads a package (a folder, a zip) and hands the host its files
//! as bytes, at once ([`crate::PluginHost::request_load`]) or through a [`PluginSource`] the host
//! asks again for a reload. The package itself (its files, its manifest, its zip form) is
//! `dereth-plugin-package`'s.

use dereth_plugin_api::PluginId;
pub use dereth_plugin_package::{is_package_root, Package, PackageError, MANIFEST_FILE};

/// Where packages come from. The product implements it over its own storage.
pub trait PluginSource: core::fmt::Debug {
    /// Every package the source offers.
    fn list(&mut self) -> Vec<Package>;

    /// The current files of the package whose manifest has `id`, for a reload.
    fn fetch(&mut self, id: &PluginId) -> Option<Package> {
        self.list()
            .into_iter()
            .find(|p| p.manifest().is_ok_and(|m| &m.id == id))
    }
}

/// A source with a fixed set of packages, for tests and for products that read their packages
/// themselves.
#[derive(Debug, Default, Clone)]
pub struct StaticSource {
    pub packages: Vec<Package>,
}

impl PluginSource for StaticSource {
    fn list(&mut self) -> Vec<Package> {
        self.packages.clone()
    }
}
