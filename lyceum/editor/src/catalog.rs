//! What the model browser knows of every model, and the map's picture of the world, gathered off
//! the frame loop: where the data files place each model and the world drawn from above (each
//! kept in a file in the picture cache, by the files' fingerprints, so the next start reads it at
//! once) and, unless the command line named its own SQL, the weenies of the server database
//! `empyrean-import fetch` keeps in the user's cache folder.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

use dereth_assets::Region;
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
    /// The database read, and what went wrong, if anything did.
    pub database: Option<PathBuf>,
    pub problem: Option<String>,
}

/// The gathering, running.
#[derive(Debug)]
pub struct Catalog {
    rx: Option<Receiver<Ready>>,
}

impl Catalog {
    /// Gather the placements of the data files in `dats` (and the scenery of `region`) and the
    /// map drawn with the terrain types' `colors`, kept under `cache`, and the weenies of the SQL
    /// file `database`, on a thread of its own.
    #[must_use]
    pub fn start(
        dats: PathBuf,
        region: Option<Region>,
        database: Option<PathBuf>,
        cache: PathBuf,
        colors: Vec<[u8; 3]>,
    ) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("model names".into())
            .spawn(move || {
                let _ = tx.send(gather(&dats, region.as_ref(), database, &cache, &colors));
            });
        if let Err(e) = spawned {
            tracing::warn!("the model browser names no models: {e}");
            return Self { rx: None };
        }
        Self { rx: Some(rx) }
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

fn gather(
    dats: &Path,
    region: Option<&Region>,
    database: Option<PathBuf>,
    cache: &Path,
    colors: &[[u8; 3]],
) -> Ready {
    let mut problem = None;
    let (placements, map) = match DatSet::open(dats) {
        Ok(set) => {
            let file = placements_file(cache, &set);
            let placements = Placements::load(&file).unwrap_or_else(|| {
                let p = Placements::read(&set, region, &town_of);
                if let Err(e) = p.save(&file) {
                    tracing::warn!("{}: {e}", file.display());
                }
                p
            });
            let file = map_file(cache, &set);
            let map = region.map(|r| {
                WorldMap::load(&file).unwrap_or_else(|| {
                    let m = WorldMap::read(&set, r, colors);
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
    Ready {
        placements,
        map,
        server,
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

/// The server database `empyrean-import fetch` keeps in the user's cache folder: the newest
/// world database there.
#[must_use]
pub fn server_database() -> Option<PathBuf> {
    newest_database(&database_folder(|n| std::env::var_os(n))?)
}

/// The folder `empyrean-import fetch` keeps world databases in, reading the environment
/// through `env`.
fn database_folder(env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let var = |name: &str| {
        env(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base = if cfg!(windows) {
        var("LOCALAPPDATA")?.join("Empyrean")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Caches/Empyrean")
    } else {
        var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".cache")))?
            .join("empyrean")
    };
    Some(base.join("world-database"))
}

/// The newest world database in `dir`: the highest release of the patched line
/// (`ACE-World-Database-v<x>.<y>.<z>.sql`), else of the original line.
#[must_use]
pub fn newest_database(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<((u8, u32, u32, u32), PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let (line, rest) = if let Some(r) = name.strip_prefix("ACE-World-Database-v") {
            (1, r)
        } else if let Some(r) = name.strip_prefix("ACE-World-16PY-db-v") {
            (0, r)
        } else {
            continue;
        };
        let Some(version) = rest.strip_suffix(".sql") else {
            continue;
        };
        let n: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
        let [a, b, c] = n[..] else { continue };
        let key = (line, a, b, c);
        if best.as_ref().is_none_or(|(k, _)| key > *k) {
            best = Some((key, e.path()));
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: where the world editor finds the server's world database, and the town a landblock is in)
    use super::*;

    #[test]
    fn the_newest_patched_world_database_is_chosen_over_older_and_original_ones() {
        let dir = std::env::temp_dir().join(format!("editor-dbs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in [
            "ACE-World-16PY-db-v0.8.8.sql",
            "ACE-World-Database-v0.9.295.sql",
            "ACE-World-Database-v0.9.30.sql",
            "ACE-World-Database-v0.10.1.sql.zip",
            "notes.txt",
        ] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        assert_eq!(
            newest_database(&dir).unwrap().file_name().unwrap(),
            "ACE-World-Database-v0.9.295.sql"
        );
        std::fs::remove_file(dir.join("ACE-World-Database-v0.9.295.sql")).unwrap();
        std::fs::remove_file(dir.join("ACE-World-Database-v0.9.30.sql")).unwrap();
        assert_eq!(
            newest_database(&dir).unwrap().file_name().unwrap(),
            "ACE-World-16PY-db-v0.8.8.sql"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_cache_folder_is_the_users_and_a_relative_or_empty_variable_counts_as_none() {
        let env = |vars: &'static [(&'static str, &'static str)]| {
            move |n: &str| {
                vars.iter()
                    .find(|(k, _)| *k == n)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        if cfg!(windows) {
            assert_eq!(
                database_folder(env(&[("LOCALAPPDATA", "C:\\Users\\a\\AppData\\Local")])),
                Some(PathBuf::from(
                    "C:\\Users\\a\\AppData\\Local\\Empyrean\\world-database"
                ))
            );
            assert_eq!(database_folder(env(&[("LOCALAPPDATA", "rel")])), None);
            assert_eq!(database_folder(env(&[("LOCALAPPDATA", "")])), None);
        } else if !cfg!(target_os = "macos") {
            assert_eq!(
                database_folder(env(&[("HOME", "/home/a")])),
                Some(PathBuf::from("/home/a/.cache/empyrean/world-database"))
            );
            assert_eq!(
                database_folder(env(&[("XDG_CACHE_HOME", "/c"), ("HOME", "/home/a")])),
                Some(PathBuf::from("/c/empyrean/world-database"))
            );
        }
    }

    #[test]
    fn a_towns_landblock_is_in_the_town_and_open_country_is_in_none() {
        // Holtburg's middle, Yaraq's and Rithwic's; the sea far to the south-east.
        assert_eq!(town_of(0xA9B4).as_deref(), Some("Holtburg"));
        assert_eq!(town_of(0x7D64).as_deref(), Some("Yaraq"));
        assert_eq!(town_of(0xC98C).as_deref(), Some("Rithwic"));
        assert_eq!(town_of(0xFE01), None);
    }
}
