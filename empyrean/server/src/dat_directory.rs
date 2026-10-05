//! Where the server finds the retail dat files. Not ACE.
//!
//! Only `server.dat_files_directory` names it; the server reads no environment variable. The value
//! resolves as every path in `empyrean.toml` does ([`empyrean_common::config_paths`]): `~` is the home
//! directory, an absolute folder is used as it is, a relative one is relative to the configuration
//! file's folder (the working directory without a file). An empty `dat_files_directory` (the
//! default) searches: the configuration file's folder (the working directory without a file),
//! then the folder the server executable is in; the first that holds `client_portal.dat` (else the
//! first holding the older set, `portal.dat` and `cell.dat`). ACE's
//! default was a Windows drive path, which is wrong on the Linux and macOS builds; the default here
//! is the same on every platform.
//!
//! The search is `dereth-dat`'s ([`dereth_dat::locate_modern_dats`]), the one the client uses for
//! its own candidates; this module only decides which folders are candidates.

use std::path::PathBuf;

use empyrean_common::config_paths::PathBase;
use empyrean_common::master_configuration::MasterConfiguration;

/// The folders the configured `dat_files_directory` allows under `base`, in order: the one folder
/// a set key names, or the search folders of an empty one.
#[must_use]
pub fn dat_directory_candidates(configured: &str, base: &PathBase) -> Vec<PathBuf> {
    if configured.trim().is_empty() {
        base.search_candidates("")
    } else {
        vec![base.resolve(configured)]
    }
}

/// The dat directory for the configured `dat_files_directory` under `base`: the first candidate
/// holding the dats, or -- when none does -- the first candidate, so the error names it.
#[must_use]
pub fn dat_directory(configured: &str, base: &PathBase) -> PathBuf {
    let candidates = dat_directory_candidates(configured, base);
    match dereth_dat::locate_modern_dats(&candidates) {
        Ok(dir) => dir.into_path_buf(),
        // The dat set from before Throne of Destiny (`portal.dat`, `cell.dat`) marks a folder too.
        Err(_) => candidates
            .iter()
            .find(|d| dereth_dat::holds_classic_dats(d))
            .or(candidates.first())
            .cloned()
            .unwrap_or_default(),
    }
}

/// The dat set the world is drawn from: the one the world's data overlay (`[dat_overlay] path`)
/// was made against when there is one, else the era's (`[era] profile`). The era is only the
/// default: a world made outside Dereth may play an early era over the later files.
#[must_use]
pub fn world_set(config: &MasterConfiguration, base: &PathBase) -> dereth_dat::ContainerEra {
    let overlay = config.dat_overlay.path.trim();
    let from_overlay = (!overlay.is_empty())
        .then(|| dereth_dat::overlay::OverlayDir::new(&base.resolve(overlay)).ok())
        .flatten()
        .and_then(|d| d.base_era());
    from_overlay.unwrap_or_else(|| config.era.profile.container_era())
}

/// [`dat_directory`] for this process: the loaded configuration under `base`. A world drawn from
/// the files before Throne of Destiny ([`world_set`]) takes the first candidate holding
/// `portal.dat` and `cell.dat` instead, since one folder may hold both sets.
#[must_use]
pub fn configured_dat_directory(config: &MasterConfiguration, base: &PathBase) -> PathBuf {
    let configured = &config.server.dat_files_directory;
    if world_set(config, base) == dereth_dat::ContainerEra::Classic {
        let candidates = dat_directory_candidates(configured, base);
        if let Some(dir) = candidates
            .iter()
            .find(|d| dereth_dat::holds_classic_dats(d))
        {
            return dir.clone();
        }
    }
    dat_directory(configured, base)
}
