//! The Navigator's acceptance for each era of the world: from every town's drop point to a fixed
//! list of destinations (towns, and dungeons by the portals that lead into them), at a few
//! character levels, over the portal table and places a launcher builds from the era's public
//! world database. Every destination the era has is reached, through portals where it takes them,
//! or refused with the reason: the level, or what a portal asks.
//!
//! The world databases are read from `$DERETH_TEST_WORLD_DATABASES`, else the per-user folder
//! Empyrean's import tool and `dereth-places` keep them in (`%LOCALAPPDATA%\Empyrean\world-database`
//! on Windows). `dereth-places fetch --era <era>` fills it. Each era's own data files tell a
//! building's inside (where many portals stand) from a dungeon: the end of retail's from
//! `$DERETH_TEST_DAT_DIR`, February 2005's from `$DERETH_TEST_PRETOD_DAT_DIR`, and October 1999's
//! from the `1999-10-09` capture under `$DERETH_TEST_DAT_CAPTURES_DIR`.

// The dat tests' fixture: an era's landblocks loaded as the client loads them, with a planner over
// them. Not every helper is used here.
#[allow(dead_code)]
#[path = "../dat/common.rs"]
mod common;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_world_data::env_cells::EnvCellLoader;

use dereth_navigation::{
    land_realm, reach_for, JourneyConfig, Leg, PlaceKind, PlaceStore, PortalStore, Reach, Realm,
    Refusal, Traveller,
};
use dereth_primitives::{EraId, Position};
use dereth_world_places::eras::EraDatabase;
use dereth_world_places::{Source, WorldPlaces};

/// The folder the world databases are in.
fn databases() -> PathBuf {
    if let Some(d) = std::env::var_os("DERETH_TEST_WORLD_DATABASES") {
        return PathBuf::from(d);
    }
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let found = if cfg!(windows) {
        var("LOCALAPPDATA").map(|d| d.join("Empyrean").join("world-database"))
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Caches/Empyrean/world-database"))
    } else {
        var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".cache")))
            .map(|d| d.join("empyrean").join("world-database"))
    };
    found.expect("no world-database folder: set DERETH_TEST_WORLD_DATABASES")
}

/// The era's database in `dir`: the pinned release's, or the newest Launch one kept there.
fn database(dir: &Path, era: EraId) -> PathBuf {
    let db = EraDatabase::of(era);
    if db.pinned() {
        let p = dir.join(db.sql_name(db.tag));
        assert!(
            p.is_file(),
            "{} is not there: dereth-places fetch --era {}",
            p.display(),
            era.name()
        );
        return p;
    }
    let mut kept: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("the world-database folder")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(db.asset_prefix) && n.ends_with(".sql"))
        })
        .collect();
    kept.sort();
    kept.pop().unwrap_or_else(|| {
        panic!(
            "no {} database in {}: dereth-places fetch --era {}",
            era.name(),
            dir.display(),
            era.name()
        )
    })
}

/// An era's portals and places, as a launcher builds them for a world of that era.
struct Package {
    portals: PortalStore,
    places: PlaceStore,
}

fn package(era: EraId) -> Package {
    let path = database(&databases(), era);
    let file = std::fs::File::open(&path).expect("the database");
    let content = dereth_world_places::sql::read(std::io::BufReader::with_capacity(1 << 20, file))
        .expect("the database reads");
    let docs = WorldPlaces::of(&content).documents("Dereth", Source::Hint);
    Package {
        portals: PortalStore::from_text(&docs.portals).expect("the portal table reads"),
        places: PlaceStore::from_text(&docs.places).expect("the places read"),
    }
}

/// Where the journeys go: towns by name, and dungeons by the portal that leads in.
const TOWNS: [&str; 6] = [
    "Holtburg",
    "Shoushi",
    "Yaraq",
    "Arwic",
    "Cragstone",
    "Zaikhal",
];
const DUNGEONS: [&str; 6] = [
    "Drudge Hideout",
    "Colier Mine",
    "Bellig Tower",
    "Crater Lair",
    "Cave of Alabree",
    "Dry Well Portal",
];

/// The levels the characters are.
const LEVELS: [i32; 4] = [1, 20, 60, 126];

/// The era's data files.
fn store(era: EraId) -> RetailDatStore {
    match era {
        EraId::Eor | EraId::NewDereth => dereth_dat::testing::open_store_or_fail(),
        EraId::Infiltration => dereth_dat::testing::open_classic_store_or_fail(),
        EraId::Launch => {
            let dir = std::env::var_os("DERETH_TEST_DAT_CAPTURES_DIR")
                .map(PathBuf::from)
                .expect("DERETH_TEST_DAT_CAPTURES_DIR names the dat captures")
                .join("1999-10-09");
            RetailDatStore::open_classic_dir(&dir)
                .unwrap_or_else(|e| panic!("the October 1999 files in {}: {e}", dir.display()))
        }
    }
}

/// The realms of an era's world: land is outdoors, and so is the inside of a building (an interior
/// cell whose cells open onto the land somewhere); any other interior cell is its landblock's
/// dungeon.
struct Realms {
    store: RetailDatStore,
    loader: RefCell<EnvCellLoader>,
    known: RefCell<HashMap<u32, Realm>>,
}

impl Realms {
    fn of(era: EraId) -> Self {
        Self {
            store: store(era),
            loader: RefCell::new(EnvCellLoader::new()),
            known: RefCell::new(HashMap::new()),
        }
    }

    fn realm(&self, p: &Position) -> Option<Realm> {
        if let Some(r) = land_realm(p.cell) {
            return Some(r);
        }
        if let Some(r) = self.known.borrow().get(&p.cell.0) {
            return Some(*r);
        }
        let block = p.cell.0 & 0xFFFF_0000;
        let dungeon = Realm::Dungeon(u16::try_from(p.cell.0 >> 16).unwrap_or(0));
        let mut seen = std::collections::BTreeSet::from([p.cell.0]);
        let mut queue = std::collections::VecDeque::from([p.cell.0]);
        let mut found = dungeon;
        'flood: while let Some(id) = queue.pop_front() {
            let Some(d) = self
                .loader
                .borrow_mut()
                .load_cell(&self.store, dereth_primitives::CellId(id))
            else {
                continue;
            };
            for portal in &d.cell.portals {
                if portal.other_cell_id == 0xFFFF_FFFF {
                    found = Realm::Outdoors;
                    break 'flood;
                }
                let next = block | (portal.other_cell_id & 0xFFFF);
                if seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        for id in seen {
            self.known.borrow_mut().insert(id, found);
        }
        Some(found)
    }
}

/// A destination: its name, where it is, and the portal that leads there (none for a town).
struct Goal {
    name: String,
    at: Position,
    portal: Option<String>,
}

fn goals(p: &Package) -> (Vec<Goal>, Vec<&'static str>) {
    let mut out = Vec::new();
    let mut missing = Vec::new();
    for town in TOWNS {
        match p
            .places
            .places()
            .iter()
            .find(|k| k.kind == PlaceKind::Town && k.name == town)
        {
            Some(k) => out.push(Goal {
                name: town.to_owned(),
                at: k.at,
                portal: None,
            }),
            None => missing.push(town),
        }
    }
    for name in DUNGEONS {
        match p
            .portals
            .portals()
            .iter()
            .find(|e| e.name == name && e.to.is_some_and(|t| t.cell.0 & 0xFFFF >= 0x0100))
        {
            Some(e) => out.push(Goal {
                name: name.to_owned(),
                at: e.to.expect("a destination"),
                portal: Some(name.to_owned()),
            }),
            None => missing.push(name),
        }
    }
    (out, missing)
}

/// One era's matrix: for each level, how many journeys were planned, refused (by reason) and
/// found no way; and every refusal checked against the portals' own limits.
fn matrix(era: EraId) {
    let p = package(era);
    let realms = Realms::of(era);
    let realm = |at: &Position| realms.realm(at);
    let starts: Vec<(&str, Position)> = p
        .places
        .places()
        .iter()
        .filter(|k| k.kind == PlaceKind::Town)
        .map(|k| (k.name.as_str(), k.at))
        .collect();
    let (goals, missing) = goals(&p);
    assert!(starts.len() >= 20, "{}: {} towns", era.name(), starts.len());
    let config = JourneyConfig::default();
    println!(
        "{}: {} towns, {} destinations ({} not in this era: {missing:?}), {} portals",
        era.name(),
        starts.len(),
        goals.len(),
        missing.len(),
        p.portals.portals().len()
    );
    // The levels at which each destination is reached from every town.
    let mut everywhere: BTreeMap<String, Vec<i32>> = BTreeMap::new();
    for level in LEVELS {
        let mut traveller = Traveller::at_level(Some(level));
        // A new character holds no quest flags.
        traveller.quests = Some(std::collections::BTreeSet::new());
        let (mut planned, mut no_way) = (0, Vec::new());
        let mut refused: BTreeMap<String, usize> = BTreeMap::new();
        for goal in &goals {
            let mut reached = 0;
            for (town, from) in &starts {
                match reach_for(from, &goal.at, &p.portals, &[], &traveller, &realm, &config) {
                    Reach::Planned(j) => {
                        planned += 1;
                        reached += 1;
                        // A dungeon is entered by a portal on the way.
                        if goal.portal.is_some() {
                            assert!(
                                j.legs.iter().any(|l| matches!(l, Leg::Portal { .. })),
                                "{}: {town} to {} without a portal",
                                era.name(),
                                goal.name
                            );
                        }
                    }
                    Reach::Refused(why) => {
                        for (portal, r) in &why {
                            // Every reason is the portal's own: its levels, or what it asks.
                            let edge = p
                                .portals
                                .portals()
                                .iter()
                                .find(|e| &e.name == portal)
                                .expect("a portal the table holds");
                            match r {
                                Refusal::BelowLevel(min) => {
                                    assert!(level < *min && edge.levels.0 == Some(*min));
                                }
                                Refusal::AboveLevel(max) => {
                                    assert!(level > *max && edge.levels.1 == Some(*max));
                                }
                                Refusal::Requires(q) => assert!(edge.requires.contains(q)),
                                Refusal::Retired(_) => panic!("nothing is retired in a package"),
                            }
                            let kind = match r {
                                Refusal::BelowLevel(_) => "below level".to_owned(),
                                Refusal::AboveLevel(_) => "above level".to_owned(),
                                Refusal::Requires(q) => format!("asks {q}"),
                                Refusal::Retired(_) => "retired".to_owned(),
                            };
                            *refused.entry(kind).or_default() += 1;
                        }
                    }
                    Reach::NoWay => no_way.push(format!("{town} to {}", goal.name)),
                }
            }
            if reached == starts.len() {
                everywhere.entry(goal.name.clone()).or_default().push(level);
            }
        }
        println!(
            "  level {level}: {planned} planned, refused {refused:?}, no way {}",
            no_way.len()
        );
        assert!(
            no_way.is_empty(),
            "{}: no way at level {level}: {no_way:?}",
            era.name()
        );
    }
    // Every destination the era has is reached from every one of its towns, at some level: one
    // for characters of a level range is refused the others, each with its reason above.
    for goal in &goals {
        assert!(
            everywhere.contains_key(&goal.name),
            "{}: {} is reached from every town at none of the levels {LEVELS:?}",
            era.name(),
            goal.name
        );
    }
    println!("  reached from every town at levels {everywhere:?}");
}

/// Behaviour: none (acceptance: the Navigator's journeys per era over the era's place package)
#[test]
fn every_end_of_retail_town_reaches_the_destinations_or_says_why() {
    matrix(EraId::Eor);
}

/// Behaviour: none (acceptance: the Navigator's journeys per era over the era's place package)
#[test]
fn every_infiltration_town_reaches_the_destinations_or_says_why() {
    matrix(EraId::Infiltration);
}

/// Behaviour: none (acceptance: the Navigator's journeys per era over the era's place package)
#[test]
fn every_launch_town_reaches_the_destinations_or_says_why() {
    matrix(EraId::Launch);
}

/// Behaviour: none (acceptance: an era's place package holds only the portals that work in it)
#[test]
fn the_end_of_retails_destroyed_portals_are_left_out_and_work_in_infiltration() {
    let destroyed = |name: &str| name.starts_with("Destroyed ");
    let eor = package(EraId::Eor);
    let portals: Vec<_> = eor
        .portals
        .portals()
        .iter()
        .filter(|e| destroyed(&e.name))
        .map(|e| e.name.as_str())
        .collect();
    assert!(portals.is_empty(), "portals: {portals:?}");
    let places: Vec<_> = eor
        .places
        .places()
        .iter()
        .filter(|k| destroyed(&k.name))
        .map(|k| (k.kind, k.name.as_str()))
        .collect();
    assert!(places.is_empty(), "places: {places:?}");
    // Zaikhal is still the portals' that work.
    assert!(eor
        .portals
        .portals()
        .iter()
        .any(|e| e.name == "Zaikhal Portal"));
    // In February 2005 the portal to Zaikhal the end of retail destroys still works.
    let infiltration = package(EraId::Infiltration);
    assert!(infiltration
        .portals
        .portals()
        .iter()
        .any(|e| e.wcid == Some(1032) && e.name == "Portal to Zaikhal"));
}

/// One era's walks: inside each destination dungeon, from where its portal sets the character
/// down to the place farthest from there by walking; and outdoors, from the nearest town's drop
/// point to the dungeon's portal, where both are in one planning window.
fn walks(era: EraId) {
    let p = package(era);
    let store = Arc::new(store(era));
    let (goals, _) = goals(&p);
    let towns: Vec<(&str, Position)> = p
        .places
        .places()
        .iter()
        .filter(|k| k.kind == PlaceKind::Town)
        .map(|k| (k.name.as_str(), k.at))
        .collect();
    let config = dereth_navigation::NavConfig::default();
    let (mut inside, mut outside, mut beyond) = (0, 0, Vec::new());
    for goal in goals.iter().filter(|g| g.portal.is_some()) {
        // Inside: the arrival, and the farthest place a walk reaches from it.
        let block = u16::try_from(goal.at.cell.0 >> 16).expect("a landblock");
        let mut d = common::load_from(Arc::clone(&store), &[block], false, config.clone());
        let (from, to, steps) = {
            let region = d.nav.region(goal.at.cell).unwrap_or_else(|e| {
                panic!("{}: {}'s dungeon {block:04X}: {e:?}", era.name(), goal.name)
            });
            let arrival = region.nearest_place(&goal.at, 3.0).unwrap_or_else(|| {
                panic!(
                    "{}: nowhere to stand where {} sets one down",
                    era.name(),
                    goal.name
                )
            });
            let far = common::farthest_from(region, arrival);
            (
                region.position(arrival),
                region.position(far),
                far != arrival,
            )
        };
        if steps {
            let route = d.nav.plan(&from, &to).unwrap_or_else(|e| {
                panic!(
                    "{}: inside {} from where it sets one down to its farthest place: {e:?}",
                    era.name(),
                    goal.name
                )
            });
            println!(
                "  {}: inside {block:04X}, to {:08X}, {:.0} m, {} legs",
                goal.name,
                to.cell.0,
                route.length(),
                route.waypoints.len() - 1
            );
        }
        inside += 1;
        // Outside: from the nearest town to the portal.
        let portal = p
            .portals
            .portals()
            .iter()
            .find(|e| Some(&e.name) == goal.portal.as_ref() && e.to == Some(goal.at))
            .expect("the portal");
        let (town, start) = towns
            .iter()
            .min_by(|a, b| {
                dereth_navigation::global_distance(&a.1, &portal.from)
                    .total_cmp(&dereth_navigation::global_distance(&b.1, &portal.from))
            })
            .expect("a town");
        let (sx, sy) = (
            i32::try_from(start.cell.0 >> 24).expect("x"),
            i32::try_from((start.cell.0 >> 16) & 0xFF).expect("y"),
        );
        let (px, py) = (
            i32::try_from(portal.from.cell.0 >> 24).expect("x"),
            i32::try_from((portal.from.cell.0 >> 16) & 0xFF).expect("y"),
        );
        let (x0, x1, y0, y1) = (
            sx.min(px) - 1,
            sx.max(px) + 1,
            sy.min(py) - 1,
            sy.max(py) + 1,
        );
        let area = (x1 - x0 + 1) * (y1 - y0 + 1);
        if area > config.max_blocks {
            beyond.push(format!("{town} to {}", goal.name));
            continue;
        }
        let first = u16::try_from((sx << 8) | sy).expect("a landblock");
        let mut blocks = vec![first];
        for x in x0..=x1 {
            for y in y0..=y1 {
                let b = u16::try_from((x << 8) | y).expect("a landblock");
                if b != first {
                    blocks.push(b);
                }
            }
        }
        let mut d = common::load_from(Arc::clone(&store), &blocks, true, config.clone());
        let route = d.nav.plan(start, &portal.from).unwrap_or_else(|e| {
            panic!(
                "{}: from {town} to {}'s portal: {e:?}",
                era.name(),
                goal.name
            )
        });
        println!(
            "  {}: from {town}, {:.0} m, {} legs",
            goal.name,
            route.length(),
            route.waypoints.len() - 1
        );
        outside += 1;
    }
    println!(
        "{}: {inside} dungeons walked inside, {outside} portals walked to from a town, beyond one \
         window: {beyond:?}",
        era.name()
    );
}

/// Behaviour: none (acceptance: the Navigator's walks per era on the era's data files)
#[test]
fn the_end_of_retails_destination_dungeons_are_walked_inside_and_to() {
    walks(EraId::Eor);
}

/// Behaviour: none (acceptance: the Navigator's walks per era on the era's data files)
#[test]
fn infiltrations_destination_dungeons_are_walked_inside_and_to() {
    walks(EraId::Infiltration);
}

/// Behaviour: none (acceptance: the Navigator's walks per era on the era's data files)
#[test]
fn launchs_destination_dungeons_are_walked_inside_and_to() {
    walks(EraId::Launch);
}
