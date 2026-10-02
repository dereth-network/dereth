//! Divergence: V391
//! The server's dat layer on the February 2005 dat set (portal.dat and cell.dat, before Throne of
//! Destiny): the files open with their header iterations, the tables ACE reads up front decode in
//! their own layouts (the skill table keeps its own weapon skills), and a character's body enters
//! the world in the outdoor starter areas and in a dungeon cell.
//! Fixture: the February 2005 portal and cell files (`DERETH_TEST_PRETOD_DAT_DIR`).

use std::sync::{Arc, OnceLock};

use dereth_physics::source::LandSource;
use dereth_physics::PhysicsWorld;
use dereth_primitives::{CellId, ContainerEra, LandblockId, ObjectId, Position};
use empyrean_dat::dat_manager::{ITERATION_CELL_FEBRUARY_2005, ITERATION_PORTAL_FEBRUARY_2005};
use empyrean_dat::file_types::LandblockInfo;
use empyrean_dat::physics::{DatLandSource, SetupGeometryCache};
use empyrean_dat::{DatDatabaseType, DatManager, DatSource, RealDats};

fn dats() -> Arc<DatManager> {
    static DATS: OnceLock<Arc<DatManager>> = OnceLock::new();
    Arc::clone(DATS.get_or_init(|| {
        if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
            panic!("{msg}");
        }
        let dir = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default();
        let real = RealDats::open(&dir)
            .unwrap_or_else(|e| panic!("the February 2005 dats under {}: {e}", dir.display()));
        DatManager::initialize(Arc::new(real)).expect("the February 2005 dats initialize")
    }))
}

#[test]
fn the_february_2005_dats_open_with_their_header_iterations_and_era_tables() {
    let dats = dats();
    assert_eq!(dats.portal_dat().container_era(), ContainerEra::PreTod);
    assert_eq!(dats.cell_dat().iteration(), ITERATION_CELL_FEBRUARY_2005);
    assert_eq!(
        dats.portal_dat().iteration(),
        ITERATION_PORTAL_FEBRUARY_2005
    );
    assert!(dats.high_res_dat().is_none());
    // The language reads are the portal file's.
    assert_eq!(
        dats.language_dat().file_path(),
        dats.portal_dat().file_path()
    );

    let portal = dats.portal_dat();
    assert_eq!(portal.char_gen().heritage_groups.len(), 3);
    assert_eq!(portal.char_gen().starter_areas.len(), 6);
    // The table's own 36 skills, the weapon skills among them, and nothing added.
    let skills = &portal.skill_table().skills;
    assert_eq!(skills.len(), 36);
    assert_eq!(skills[&1].name, "Axe");
    assert!(skills[&11].trained_cost > 0, "Sword has its own cost");
    assert_eq!(portal.xp_table().level_xp.len(), 127);
    assert_eq!(portal.spell_table().spells.len(), 3737);
    // The tables that began at Throne of Destiny are absent, and nothing asks for them.
    assert_eq!(
        portal.missing_tables(),
        [
            "ContractTable",
            "MasterProperty",
            "NameFilterTable",
            "TabooTable"
        ]
    );
    assert!(dats.language_dat().try_character_titles().is_none());
}

/// Every heritage's body enters the world at each of the six outdoor starter areas.
#[test]
fn every_february_2005_body_enters_the_world_at_every_starter_area() {
    let dats = dats();
    let char_gen = dats.portal_dat().char_gen();
    let setups = SetupGeometryCache::new(Arc::clone(&dats));
    let mut n = 0u32;
    for area in &char_gen.starter_areas {
        let start: Position = area.locations[0];
        let land = Arc::new(DatLandSource::new(Arc::clone(&dats)).expect("the region"));
        assert!(land.load_landblock(start.cell.landblock()), "{}", area.name);
        let mut world = PhysicsWorld::new(land);
        for hg in char_gen.heritage_groups.values() {
            for sex in hg.sexes.values() {
                let g = setups.get(sex.setup.0).expect("the body's setup");
                n += 1;
                let h = world.create(ObjectId(0x5000_0000 + n), g, true);
                assert!(
                    world.enter_world(h, &start),
                    "{} at {}",
                    sex.setup,
                    area.name
                );
                let placed = world.get(h).expect("placed").position;
                assert_eq!(placed.cell.landblock(), start.cell.landblock());
            }
        }
    }
    assert_eq!(n, 6 * 6);
}

/// A dungeon of the February 2005 cell file (a landblock of interior cells and no buildings)
/// loads its cells for collision, and a body placed in one stands in that cell.
#[test]
fn a_body_enters_a_february_2005_dungeon_cell() {
    let dats = dats();
    let char_gen = dats.portal_dat().char_gen();
    let body = char_gen.heritage_groups[&1].sexes[&1].setup.0;
    let setups = SetupGeometryCache::new(Arc::clone(&dats));
    let mut entered = 0;
    for block in 0x0100..=0x01FF_u16 {
        let id = LandblockId(block);
        let Some(lbi) = dats
            .cell_dat()
            .read_from_dat::<LandblockInfo>(id.info_id().0)
        else {
            continue;
        };
        if lbi.num_cells == 0 || !lbi.buildings.is_empty() {
            continue;
        }
        let land = Arc::new(DatLandSource::new(Arc::clone(&dats)).expect("the region"));
        land.load_landblock(id);
        for i in 0..lbi.num_cells.min(8) {
            let cell = CellId((u32::from(block) << 16) | (0x100 + i));
            let Some(geometry) = land.env_cell(cell) else {
                continue;
            };
            let mut world = PhysicsWorld::new(Arc::clone(&land) as Arc<dyn LandSource>);
            let g = setups.get(body).expect("the body");
            let h = world.create(ObjectId(0x5000_1000 + entered), g, true);
            let mut at = Position::new(cell, geometry.frame);
            at.frame.origin.z += 0.5;
            if world.enter_world(h, &at) {
                let placed = world.get(h).expect("placed").position;
                assert!(!placed.cell.is_outdoor(), "{cell:?} placed outdoors");
                entered += 1;
            }
        }
        if entered >= 3 {
            break;
        }
    }
    assert!(entered >= 3, "bodies entered {entered} dungeon cells");
}

/// The February 2005 files read through a source that records every id asked of it that the files
/// do not have: the ids the server reports as missing records.
#[derive(Debug)]
struct AbsentReads {
    real: RealDats,
    absent: std::sync::Mutex<Vec<u32>>,
}

impl DatSource for AbsentReads {
    fn has_database(&self, db: DatDatabaseType) -> bool {
        self.real.has_database(db)
    }
    fn file_ids(&self, db: DatDatabaseType) -> Vec<u32> {
        self.real.file_ids(db)
    }
    fn contains(&self, db: DatDatabaseType, id: u32) -> bool {
        self.real.contains(db, id)
    }
    fn read(&self, db: DatDatabaseType, id: u32) -> Option<Vec<u8>> {
        let bytes = self.real.read(db, id);
        if bytes.is_none() && id & 0xFFFF != 0xFFFE {
            self.absent.lock().expect("the list").push(id);
        }
        bytes
    }
    fn file_iteration(&self, db: DatDatabaseType, id: u32) -> Option<u32> {
        self.real.file_iteration(db, id)
    }
    fn describe(&self, db: DatDatabaseType) -> String {
        self.real.describe(db)
    }
    fn container_era(&self) -> ContainerEra {
        self.real.container_era()
    }
    fn header_iteration(&self, db: DatDatabaseType) -> Option<u32> {
        self.real.header_iteration(db)
    }
}

/// Opening the February 2005 files and building a run of objects' collision geometry asks for no
/// record the files lack: the tables that began at Throne of Destiny are not read, and an older
/// graphics object's implied detail record is looked for only where it exists, while the objects
/// that have one still read it.
#[test]
fn the_february_2005_dats_are_never_asked_for_a_record_they_lack() {
    if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
        panic!("{msg}");
    }
    let dir = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default();
    let source = Arc::new(AbsentReads {
        real: RealDats::open(&dir).expect("the February 2005 dats"),
        absent: std::sync::Mutex::new(Vec::new()),
    });
    let dats = DatManager::initialize(Arc::clone(&source) as Arc<dyn DatSource>)
        .expect("the February 2005 dats initialize");
    let setups = SetupGeometryCache::new(Arc::clone(&dats));
    let ids: Vec<u32> = dats
        .portal_dat()
        .all_files()
        .into_iter()
        .filter(|id| id >> 24 == 0x02)
        .take(400)
        .collect();
    let mut with_detail = 0;
    for id in ids {
        if let Some(g) = setups.get(id) {
            with_detail += g
                .parts
                .iter()
                .filter(|p| p.first_degrade_mode.is_some())
                .count();
        }
    }
    let absent = source.absent.lock().expect("the list").clone();
    assert!(
        absent.is_empty(),
        "asked for {} absent records, first {:08X?}",
        absent.len(),
        &absent[..absent.len().min(8)]
    );
    assert!(with_detail > 0, "no part read its implied detail record");
}
