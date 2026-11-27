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

/// The file in a world's overlay folder holding the world's own rules.
pub const WORLD_RULES_FILE: &str = "world.toml";

/// A world's own rules, from [`WORLD_RULES_FILE`] in its overlay folder. A world with no overlay,
/// or none of the file, has none of them.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct WorldRules {
    /// Not ACE (V563): the world's land lies over its dungeons, so a position is in a dungeon by
    /// its cell: an indoor cell of a landblock with a dungeon.
    pub dungeons_by_cells: bool,
    /// Not ACE (V565): the world places its own things: nothing the world database places is
    /// in it ([`empyrean_content::WorldDatabase::hide_placements`]), so none of its landblock
    /// instances, encounters, house portals, points of interest or events, and its portal table
    /// and places hold only what the world's own content places.
    pub own_placements: bool,
}

impl WorldRules {
    /// Whether the world database's placements are hidden: on a world that places its own things
    /// (V565), unless the era's world database is made for such a land (V569,
    /// [`empyrean_common::era::EraRules::placements_on_own_land`]), whose placements are then the
    /// world's own.
    #[must_use]
    pub fn hides_world_placements(&self, era: &empyrean_common::era::EraRules) -> bool {
        self.own_placements && !era.placements_on_own_land
    }
}

/// The rules of the world `config` serves from under `base` ([`WorldRules`]).
///
/// # Errors
/// When the overlay's [`WORLD_RULES_FILE`] cannot be read or is not valid.
pub fn world_rules(config: &MasterConfiguration, base: &PathBase) -> Result<WorldRules, String> {
    let overlay = config.dat_overlay.path.trim();
    if overlay.is_empty() {
        return Ok(WorldRules::default());
    }
    let file = base.resolve(overlay).join(WORLD_RULES_FILE);
    match std::fs::read_to_string(&file) {
        Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(WorldRules::default()),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// The file in a world's overlay folder holding the things it places itself
/// ([`empyrean_content::own_placements`]).
pub const WORLD_PLACEMENTS_FILE: &str = empyrean_content::own_placements::FILE;

/// The world's own placements table, from [`WORLD_PLACEMENTS_FILE`] in its overlay folder, with
/// the file's path; `None` for a world with no overlay, or none of the file.
///
/// # Errors
/// When the file is there and cannot be read as text.
pub fn world_placements(
    config: &MasterConfiguration,
    base: &PathBase,
) -> Result<Option<(PathBuf, String)>, String> {
    let overlay = config.dat_overlay.path.trim();
    if overlay.is_empty() {
        return Ok(None);
    }
    let file = base.resolve(overlay).join(WORLD_PLACEMENTS_FILE);
    match std::fs::read_to_string(&file) {
        Ok(text) => Ok(Some((file, text))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// The file in a world's overlay folder holding the weenies of the things it places itself that
/// the world database has none of ([`empyrean_content::own_placements::parse_weenies`]).
pub const WORLD_WEENIES_FILE: &str = "weenies.tsv";

/// The world's own weenies table, from [`WORLD_WEENIES_FILE`] in its overlay folder, with the
/// file's path; `None` for a world with no overlay, or none of the file.
///
/// # Errors
/// When the file is there and cannot be read as text.
pub fn world_weenies(
    config: &MasterConfiguration,
    base: &PathBase,
) -> Result<Option<(PathBuf, String)>, String> {
    let overlay = config.dat_overlay.path.trim();
    if overlay.is_empty() {
        return Ok(None);
    }
    let file = base.resolve(overlay).join(WORLD_WEENIES_FILE);
    match std::fs::read_to_string(&file) {
        Ok(text) => Ok(Some((file, text))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// The file in a world's overlay folder naming its own towns and landmarks: a places document,
/// whose places the world publishes in place of the world database's
/// ([`crate::portal_table::with_own_places`]).
pub const WORLD_PLACES_FILE: &str = empyrean_world::managers::dat_overlay::WORLD_PLACES_FILE;

/// The world's own places document, from [`WORLD_PLACES_FILE`] in its overlay folder; `None` for
/// a world with no overlay, or none of the file.
///
/// # Errors
/// When the file is there and cannot be read as text.
pub fn world_places(
    config: &MasterConfiguration,
    base: &PathBase,
) -> Result<Option<String>, String> {
    let overlay = config.dat_overlay.path.trim();
    if overlay.is_empty() {
        return Ok(None);
    }
    let file = base.resolve(overlay).join(WORLD_PLACES_FILE);
    match std::fs::read_to_string(&file) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}
