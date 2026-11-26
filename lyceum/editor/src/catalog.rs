//! What the model browser knows of every model, and the map's picture of the world, gathered off
//! the frame loop: where the data files place each model and the world drawn from above, with
//! the world's import laid over the files as its packs are compiled over them (a converted land's
//! buildings and statics are its import's), each kept in a file in the picture cache, by the
//! files' and the import's fingerprints, so the next start reads it at once; unless the command line named its own SQL, the weenies of the server database
//! `empyrean-import fetch` keeps in the user's cache folder; and every building the world places,
//! with the doors that database stands at them ([`lyceum_foundry::buildings`]), kept beside the
//! placements by the files' and the database's fingerprints; and the world's own dungeons, so the
//! Dungeons tab opens at once.
//!
//! A model is labeled with the town it stands in ([`Towns`]): the world's own places, when its
//! import names them (`places.tsv`), else the game's map's notes.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

use dereth_assets::Region;
use dereth_client_contract::panels::map::MapPlace;
use dereth_client_runtime::map_places::{Naming, WORLD_PLACES_FILE};
use dereth_primitives::EraId;
use lyceum_foundry::buildings::Catalog as Buildings;
use lyceum_foundry::dats::DatSet;
use lyceum_foundry::map::{LandBounds, WorldMap};
use lyceum_foundry::models::Placements;
use lyceum_foundry::sql_ids::{self, ServerContent};

/// What the gathering found.
#[derive(Debug)]
pub struct Ready {
    pub placements: Placements,
    /// The world as the data files hold it, from above.
    pub map: Option<WorldMap>,
    /// Where on the map its land lies, when it has land.
    pub map_land: Option<LandBounds>,
    /// The server's world: its weenies, instances and links; nothing when it was given no
    /// database.
    pub server: ServerContent,
    /// Every building the world places, what a drop copies; `None` when the files do not open.
    pub buildings: Option<Buildings>,
    /// The world's own dungeons, by landblock, with their rooms; `None` when the files do not
    /// open.
    pub dungeons: Option<std::collections::BTreeMap<u16, u32>>,
    /// The names the world's map gives the dungeons under an entrance, by landblock.
    pub dungeon_names: std::collections::BTreeMap<u16, String>,
    /// Over a land, the dungeons of the player's own files under it, with their rooms.
    pub files_dungeons: std::collections::BTreeMap<u16, u32>,
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
    /// Gather the placements of the data files of `era` in `dats` with the world's `imports` over
    /// them (and the scenery of `region`) and the map drawn with the terrain types' `colors`, kept
    /// under `cache`, the weenies of the
    /// SQL file `database`, and the building catalog with the doors of `doors` (the server
    /// database the session was given, or `database`), on a thread of its own.
    #[must_use]
    pub fn start(
        (dats, era, imports): (PathBuf, EraId, Vec<PathBuf>),
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
                    (era, &imports),
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
/// key (the pictures' own), named by the cell file's; each key the file's fingerprint and the
/// overlay over it, if any ([`lyceum_foundry::dats::files_key`]); and, for a world that names its
/// own places, by those places ([`Towns::key`]), since they label the models.
#[must_use]
pub fn placements_file(cache: &Path, set: &DatSet, towns: &Towns) -> PathBuf {
    let hex = |fp: [u8; 32]| -> String { fp[..8].iter().map(|b| format!("{b:02x}")).collect() };
    let portal = hex(lyceum_foundry::dats::files_key(&set.portal));
    let cell = hex(lyceum_foundry::dats::files_key(&set.cell));
    let named = towns.key().map(|k| format!("-{k}")).unwrap_or_default();
    cache
        .join(portal)
        .join(format!("placements-{cell}{named}.tsv"))
}

/// The land's bounds as kept beside the map's picture: `none`, or west, north, east and south.
fn land_text(bounds: Option<LandBounds>) -> String {
    bounds.map_or_else(
        || "none\n".to_owned(),
        |[w, n, e, s]| format!("{w} {n} {e} {s}\n"),
    )
}

/// The land's bounds kept at `path` ([`land_text`]); `None` when there is no such file.
fn read_land(path: &Path) -> Option<Option<LandBounds>> {
    let text = std::fs::read_to_string(path).ok()?;
    if text.trim() == "none" {
        return Some(None);
    }
    let v: Vec<u32> = text
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    <[u32; 4]>::try_from(v).ok().map(Some)
}

/// The file the map's picture of the files `set` is kept in, beside the placements.
#[must_use]
pub fn map_file(cache: &Path, set: &DatSet) -> PathBuf {
    let placements = placements_file(cache, set, &Towns::default());
    placements.with_file_name(format!(
        "map-{}.png",
        placements
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("placements-"))
            .unwrap_or("map")
    ))
}

/// The file the building catalog of `set` with the doors of the server database `doors` is kept
/// in, beside the placements: named as they are (by the cell file's fingerprint and the world's
/// own places) and by the database's (its name, length and time of change), or `none` without one.
#[must_use]
pub fn buildings_file(cache: &Path, set: &DatSet, towns: &Towns, doors: Option<&Path>) -> PathBuf {
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
    let placements = placements_file(cache, set, towns);
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
    (era, imports): (EraId, &[PathBuf]),
    region: Option<&Region>,
    database: Option<PathBuf>,
    doors: Option<&Path>,
    cache: &Path,
    colors: &[[u8; 3]],
) -> Ready {
    let mut problem = None;
    let towns = Towns::of_imports(imports);
    let town = |lb: u16| towns.town(lb);
    let opened = DatSet::open_era(dats, era).map(|set| {
        set.with_imports(imports).unwrap_or_else(|e| {
            tracing::warn!("the model browser lists the files without the world's imports: {e}");
            set
        })
    });
    let (placements, map) = match &opened {
        Ok(set) => {
            let file = placements_file(cache, set, &towns);
            let placements = Placements::load(&file).unwrap_or_else(|| {
                let p = Placements::read(set, region, &town);
                if let Err(e) = p.save(&file) {
                    tracing::warn!("{}: {e}", file.display());
                }
                p
            });
            let file = map_file(cache, set);
            let land = file.with_extension("land");
            let map = region.map(|r| {
                WorldMap::load(&file)
                    .zip(read_land(&land))
                    .unwrap_or_else(|| {
                        let (m, bounds) = WorldMap::read_framed(set, r, colors);
                        if let Err(e) = m
                            .save(&file)
                            .and_then(|()| std::fs::write(&land, land_text(bounds)))
                        {
                            tracing::warn!("{}: {e}", file.display());
                        }
                        (m, bounds)
                    })
            });
            (placements, map)
        }
        Err(e) => {
            problem = Some(format!("{}: {e}", dats.display()));
            (Placements::default(), None)
        }
    };
    let (map, map_land) = map.map_or((None, None), |(m, l)| (Some(m), l));
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
    // Over a land (an import), a building standing over a dungeon is a dungeon too (a converted
    // land's AC2 dungeon under its entrance), named by the place the world's map names nearest
    // its entrance.
    let entrances = if imports.is_empty() {
        std::collections::BTreeMap::new()
    } else {
        opened
            .as_ref()
            .ok()
            .map(|set| {
                lyceum_foundry::dungeons::under_entrances(&|id| set.cell.read(id).ok(), set.era)
            })
            .unwrap_or_default()
    };
    // Over a land, the dungeons of the player's own files, which a pull copies in.
    let files_dungeons = if imports.is_empty() {
        std::collections::BTreeMap::new()
    } else {
        DatSet::open_era(dats, era)
            .map(|set| lyceum_foundry::dungeons::retail(&|id| set.cell.read(id).ok(), set.era))
            .unwrap_or_default()
    };
    let dungeon_names = entrances
        .iter()
        .filter_map(|(lb, (_, at))| towns.place_near(*lb, *at).map(|n| (*lb, n)))
        .collect();
    let dungeons = dungeons.map(|mut d| {
        for (lb, (rooms, _)) in &entrances {
            d.entry(*lb).or_insert(*rooms);
        }
        d
    });
    let doors = doors.or(database.as_deref());
    let buildings = opened.ok().map(|set| {
        let file = buildings_file(cache, &set, &towns, doors);
        Buildings::load(&file).unwrap_or_else(|| {
            // The database the session was given on its command line, read here for its doors.
            let given = (doors.is_some() && database.as_deref() != doors)
                .then(|| doors.and_then(|p| std::fs::read(p).ok()))
                .flatten()
                .map(|b| sql_ids::server_content(&String::from_utf8_lossy(&b)));
            let content = given.as_ref().or((database.is_some()).then_some(&server));
            // Over an import, the player's own files' buildings it takes away too (AC1's on a
            // converted land).
            let files = (!imports.is_empty())
                .then(|| DatSet::open_era(dats, era).ok())
                .flatten();
            let c = match &files {
                Some(f) => Buildings::of_world(f, Some(&set), content, &town),
                None => Buildings::read(&set, content, &town),
            };
            if let Err(e) = c.save(&file) {
                tracing::warn!("{}: {e}", file.display());
            }
            c
        })
    });
    Ready {
        placements,
        map,
        map_land,
        server,
        buildings,
        dungeons,
        dungeon_names,
        files_dungeons,
        database,
        problem,
    }
}

/// How far from a world's own town or landmark a landblock's middle is still in it, in map units
/// (a landblock is 0.8).
const OWN_REACH: f64 = 3.0;

/// How far from a dungeon's entrance the place the map names it by stands at most, in map units
/// (100 meters).
const ENTRANCE_REACH: f64 = 100.0 / 240.0;

/// How a world's own places label models: by their names without their notes ([`Towns::town`]).
const LABELS: &str = "bare names";

/// A place's name without the note the map writes after it: `Haunted Cave` of `Haunted Cave
/// (Dungeon, Level 5-10)`.
fn bare_name(name: &str) -> &str {
    name.split_once(" (").map_or(name, |(n, _)| n)
}

/// What names the town a landblock is in: the world's own towns and landmarks, when its import
/// names them in a places document, else the game's map's notes ([`town_of`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Towns {
    /// The world's own places, and a short key of the document that names them.
    own: Option<(Vec<MapPlace>, String)>,
}

impl Towns {
    /// The towns of a world whose imports are `imports`: the places document of the last that
    /// holds one, as the build carries it into the world's overlay.
    #[must_use]
    pub fn of_imports(imports: &[PathBuf]) -> Self {
        let document = imports
            .iter()
            .rev()
            .find_map(|i| std::fs::read_to_string(i.join(WORLD_PLACES_FILE)).ok());
        Self::of_document(document.as_deref())
    }

    /// The towns a places document `document` names (`None`, or text that is none: the notes').
    #[must_use]
    pub fn of_document(document: Option<&str>) -> Self {
        let own = document.and_then(|text| match Naming::of(Some(text)) {
            Naming::Own(places) => {
                // With how its names label models ([`Towns::town`]), so caches labeled
                // otherwise are not taken.
                let digest =
                    dereth_dat::generations::digest([(0, text.as_bytes()), (1, LABELS.as_bytes())]);
                Some((
                    places,
                    dereth_dat::generations::hex(&digest)[..16].to_owned(),
                ))
            }
            Naming::Notes => None,
        });
        Self { own }
    }

    /// The key of the world's own places, which the caches of what they label are named by;
    /// `None` for the notes.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.own.as_ref().map(|(_, k)| k.as_str())
    }

    /// The place the world's own map names nearest point `at` of landblock `lb` (landblock
    /// meters), within [`ENTRANCE_REACH`]: its name without the note after it (`Haunted Cave` of
    /// `Haunted Cave (Dungeon, Level 5-10)`). None of the notes': they name towns, not entrances.
    #[must_use]
    pub fn place_near(&self, lb: u16, at: dereth_primitives::Vec3) -> Option<String> {
        let (places, _) = self.own.as_ref()?;
        let unit = |block: u16, m: f32| (f64::from(block) * 192.0 + f64::from(m)) / 240.0 - 101.95;
        let (ew, ns) = (unit(lb >> 8, at.x), unit(lb & 0xFF, at.y));
        places
            .iter()
            .map(|p| ((p.ew - ew).powi(2) + (p.ns - ns).powi(2), p))
            .filter(|(d2, _)| *d2 <= ENTRANCE_REACH * ENTRANCE_REACH)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| bare_name(&p.name).to_owned())
    }

    /// The town or landmark landblock `lb` is in: the world's own nearest its middle, within
    /// [`OWN_REACH`], without the note after its name (`Inner Darkness` of `Inner Darkness
    /// (Dungeon, Level 18-22)`); else the notes'.
    #[must_use]
    pub fn town(&self, lb: u16) -> Option<String> {
        let Some((places, _)) = &self.own else {
            return town_of(lb);
        };
        // The places' own map coordinates: a landblock is 192 meters, a map step 240, and 0
        // stands 101.95 steps in.
        let unit = |block: u16| (f64::from(block) * 192.0 + 96.0) / 240.0 - 101.95;
        let (ew, ns) = (unit(lb >> 8), unit(lb & 0xFF));
        places
            .iter()
            .map(|p| ((p.ew - ew).powi(2) + (p.ns - ns).powi(2), p))
            .filter(|(d2, _)| *d2 <= OWN_REACH * OWN_REACH)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| bare_name(&p.name).to_owned())
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

    /// A world that names its own places labels a landblock with its own town or landmark nearest
    /// the landblock's middle (its name, without the map's note), never the game's map's, and
    /// open country with none; one whose
    /// document names none, or that has none, has the notes. Its caches are named by its places.
    #[test]
    fn a_world_with_its_own_places_names_its_towns_by_them() {
        let doc = "dereth-places-store\t1\nworld\tElsewhere\n\
place\ttown\tpublished\t-\tKehan\t0xA9B40012 54.000 46.000 0.000\t0\t0\t-\t-\n\
place\tlandmark\tpublished\t-\tDrudge Windmill (Point of Interest)\t0xA9B90031 150.000 10.000 0.000\t0\t0\t-\t-\n";
        let own = Towns::of_document(Some(doc));
        assert_eq!(own.town(0xA9B4).as_deref(), Some("Kehan"), "not Holtburg");
        assert_eq!(own.town(0xAAB5).as_deref(), Some("Kehan"));
        assert_eq!(
            own.town(0xA9B9).as_deref(),
            Some("Drudge Windmill"),
            "without its note"
        );
        assert_eq!(own.town(0x7D64), None, "not Yaraq");
        let key = own.key().expect("named by its places").to_owned();
        assert_eq!(key.len(), 16);
        assert_ne!(
            Towns::of_document(Some(&doc.replace("Kehan", "Kehan Vale"))).key(),
            Some(key.as_str())
        );
        for notes in [
            Towns::of_document(None),
            Towns::of_document(Some("not places")),
        ] {
            assert_eq!(notes.town(0xA9B4).as_deref(), Some("Holtburg"));
            assert_eq!(notes.key(), None);
        }
        let dir = std::env::temp_dir().join(format!("lyceum-towns-{}", std::process::id()));
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join(WORLD_PLACES_FILE), doc).unwrap();
        assert_eq!(Towns::of_imports(&[a.clone(), b]), own);
        assert_eq!(Towns::of_imports(&[a]), Towns::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
