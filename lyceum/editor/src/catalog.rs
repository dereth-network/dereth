//! What the model browser knows of every model, and the map's picture of the world, gathered off
//! the frame loop: where the data files place each model and the world drawn from above (each
//! kept in a file in the picture cache, by the files' fingerprints, so the next start reads it at
//! once); unless the command line named its own SQL, the weenies of the server database
//! `empyrean-import fetch` keeps in the user's cache folder; and every building the world places,
//! with the doors that database stands at them ([`lyceum_foundry::buildings`]), kept beside the
//! placements by the files' and the database's fingerprints; and the world's own dungeons, so the
//! Dungeons tab opens at once.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

use dereth_assets::Region;
use dereth_primitives::EraId;
use lyceum_foundry::buildings::Catalog as Buildings;
use lyceum_foundry::dats::DatSet;
use lyceum_foundry::map::WorldMap;
use lyceum_foundry::models::Placements;
use lyceum_foundry::sql_ids::{self, ServerContent};

/// What the gathering found.
#[derive(Debug)]
pub struct Ready {
    pub placements: Placements,
    /// The world as the data files hold it, from above.
    pub map: Option<WorldMap>,
    /// The server's world: its weenies, instances and links; nothing when it was given no
    /// database.
    pub server: ServerContent,
    /// Every building the world places, what a drop copies; `None` when the files do not open.
    pub buildings: Option<Buildings>,
    /// The world's own dungeons, by landblock, with their rooms; `None` when the files do not
    /// open.
    pub dungeons: Option<std::collections::BTreeMap<u16, u32>>,
    /// The database read, and what went wrong, if anything did.
    pub database: Option<PathBuf>,
    pub problem: Option<String>,
}

/// The gathering, running, and whether it reads a server database.
#[derive(Debug)]
pub struct Catalog {
    rx: Option<Receiver<Ready>>,
    database: bool,
}

impl Catalog {
    /// Gather the placements of the data files of `era` in `dats` (and the scenery of `region`)
    /// and the map drawn with the terrain types' `colors`, kept under `cache`, the weenies of the
    /// SQL file `database`, and the building catalog with the doors of `doors` (the server
    /// database the session was given, or `database`), on a thread of its own.
    #[must_use]
    pub fn start(
        (dats, era): (PathBuf, EraId),
        region: Option<Region>,
        database: Option<PathBuf>,
        doors: Option<PathBuf>,
        cache: PathBuf,
        colors: Vec<[u8; 3]>,
    ) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let reads = database.is_some();
        let spawned = std::thread::Builder::new()
            .name("model names".into())
            .spawn(move || {
                let _ = tx.send(gather(
                    &dats,
                    era,
                    region.as_ref(),
                    database,
                    doors.as_deref(),
                    &cache,
                    &colors,
                ));
            });
        if let Err(e) = spawned {
            tracing::warn!("the model browser names no models: {e}");
            return Self {
                rx: None,
                database: false,
            };
        }
        Self {
            rx: Some(rx),
            database: reads,
        }
    }

    /// Whether it reads a server database, and has not finished.
    #[must_use]
    pub fn reading_database(&self) -> bool {
        self.database && self.rx.is_some()
    }

    /// What it found, once it has finished (once: `None` after).
    pub fn poll(&mut self) -> Option<Ready> {
        let rx = self.rx.as_ref()?;
        match rx.try_recv() {
            Ok(r) => {
                self.rx = None;
                Some(r)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                None
            }
        }
    }

    /// What it found, waiting for it to finish.
    pub fn wait(&mut self) -> Option<Ready> {
        self.rx.take()?.recv().ok()
    }

    /// Whether it is still gathering.
    #[must_use]
    pub fn running(&self) -> bool {
        self.rx.is_some()
    }
}

/// The file the placements of `set` are kept in, under `cache`: a folder by the portal file's
/// fingerprint (the pictures' own), named by the cell file's.
#[must_use]
pub fn placements_file(cache: &Path, set: &DatSet) -> PathBuf {
    let hex = |fp: [u8; 32]| -> String { fp[..8].iter().map(|b| format!("{b:02x}")).collect() };
    let portal = hex(dereth_dat::overlay::fingerprint(&set.portal));
    let cell = hex(dereth_dat::overlay::fingerprint(&set.cell));
    cache.join(portal).join(format!("placements-{cell}.tsv"))
}

/// The file the map's picture of the files `set` is kept in, beside the placements.
#[must_use]
pub fn map_file(cache: &Path, set: &DatSet) -> PathBuf {
    placements_file(cache, set).with_file_name(format!(
        "map-{}.png",
        placements_file(cache, set)
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("placements-"))
            .unwrap_or("map")
    ))
}

/// The file the building catalog of `set` with the doors of the server database `doors` is kept
/// in, beside the placements: named by the cell file's fingerprint and the database's (its name,
/// length and time of change), or `none` without one.
#[must_use]
pub fn buildings_file(cache: &Path, set: &DatSet, doors: Option<&Path>) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let db = doors.and_then(|p| {
        let meta = std::fs::metadata(p).ok()?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        p.file_name()?.hash(&mut h);
        meta.len().hash(&mut h);
        meta.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .hash(&mut h);
        Some(format!("{:016x}", h.finish()))
    });
    let placements = placements_file(cache, set);
    let cell = placements
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("placements-"))
        .unwrap_or("cell")
        .to_owned();
    placements.with_file_name(format!(
        "buildings-{cell}-{}.tsv",
        db.as_deref().unwrap_or("none")
    ))
}

fn gather(
    dats: &Path,
    era: EraId,
    region: Option<&Region>,
    database: Option<PathBuf>,
    doors: Option<&Path>,
    cache: &Path,
    colors: &[[u8; 3]],
) -> Ready {
    let mut problem = None;
    let opened = DatSet::open_era(dats, era);
    let (placements, map) = match &opened {
        Ok(set) => {
            let file = placements_file(cache, set);
            let placements = Placements::load(&file).unwrap_or_else(|| {
                let p = Placements::read(set, region, &town_of);
                if let Err(e) = p.save(&file) {
                    tracing::warn!("{}: {e}", file.display());
                }
                p
            });
            let file = map_file(cache, set);
            let map = region.map(|r| {
                WorldMap::load(&file).unwrap_or_else(|| {
                    let m = WorldMap::read(set, r, colors);
                    if let Err(e) = m.save(&file) {
                        tracing::warn!("{}: {e}", file.display());
                    }
                    m
                })
            });
            (placements, map)
        }
        Err(e) => {
            problem = Some(format!("{}: {e}", dats.display()));
            (Placements::default(), None)
        }
    };
    let mut server = ServerContent::default();
    if let Some(path) = &database {
        match std::fs::read(path) {
            Ok(bytes) => server = sql_ids::server_content(&String::from_utf8_lossy(&bytes)),
            Err(e) => problem = Some(format!("{}: {e}", path.display())),
        }
    }
    let dungeons = opened
        .as_ref()
        .ok()
        .map(|set| lyceum_foundry::dungeons::retail(&|id| set.cell.read(id).ok(), set.era));
    let doors = doors.or(database.as_deref());
    let buildings = opened.ok().map(|set| {
        let file = buildings_file(cache, &set, doors);
        Buildings::load(&file).unwrap_or_else(|| {
            // The database the session was given on its command line, read here for its doors.
            let given = (doors.is_some() && database.as_deref() != doors)
                .then(|| doors.and_then(|p| std::fs::read(p).ok()))
                .flatten()
                .map(|b| sql_ids::server_content(&String::from_utf8_lossy(&b)));
            let content = given.as_ref().or((database.is_some()).then_some(&server));
            let c = Buildings::read(&set, content, &town_of);
            if let Err(e) = c.save(&file) {
                tracing::warn!("{}: {e}", file.display());
            }
            c
        })
    });
    Ready {
        placements,
        map,
        server,
        buildings,
        dungeons,
        database,
        problem,
    }
}

/// The town landblock `lb` is in: the place on the world map whose note covers the landblock's
/// middle, as the map places a marker.
#[must_use]
pub fn town_of(lb: u16) -> Option<String> {
    let (ew, ns) = map_coords(lb);
    let (_, _, area) = dereth_ui_screens::mapradar::map::SHIPPED_NOTE_BINDING;
    let (x, y) = dereth_ui_screens::mapradar::map::place_marker_on_map(area, ew, ns, (0, 0));
    dereth_client_contract::panels::map::notes(dereth_primitives::EraId::Eor)
        .find(|n| n.contains(x, y))
        .map(|n| n.name.to_owned())
}

/// The map coordinates (east, north) of landblock `lb`'s middle.
#[must_use]
pub fn map_coords(lb: u16) -> (f32, f32) {
    // A landblock is 8 cells of the map's 2,040; the coordinate is a tenth of a cell's count from
    // the map's middle.
    let cell = |b: u16| f32::from(b) * 8.0 + 4.0;
    (
        (cell(lb >> 8) - 1024.0) / 10.0,
        (cell(lb & 0xFF) - 1024.0) / 10.0,
    )
}

/// The server database `empyrean-import fetch` keeps in the user's cache folder for a world built
/// on `era`'s files: the newest of that era's world databases there.
#[must_use]
pub fn server_database(era: EraId) -> Option<PathBuf> {
    newest_database_for(&database_cache()?, lyceum_foundry::export::Era::of(era))
}

/// The import tool's per-user cache, also used by the editor's first-use download.
pub use lyceum_foundry::export::{database_cache, newest_database, newest_database_for};

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the town a landblock is in)
    use super::*;

    #[test]
    fn a_towns_landblock_is_in_the_town_and_open_country_is_in_none() {
        // Holtburg's middle, Yaraq's and Rithwic's; the sea far to the south-east.
        assert_eq!(town_of(0xA9B4).as_deref(), Some("Holtburg"));
        assert_eq!(town_of(0x7D64).as_deref(), Some("Yaraq"));
        assert_eq!(town_of(0xC98C).as_deref(), Some("Rithwic"));
        assert_eq!(town_of(0xFE01), None);
    }
}
