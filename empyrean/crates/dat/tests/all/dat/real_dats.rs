//! Vectors: retail dat tables, compared with the shared client decoders
//! Every table in the retail dats decodes; starting areas resolve; creature setup loads; spell
//! formulas agree with the shared decoder; taboo shape; start landblock cells; server land source
//! and setup geometry match the shared ones.
//! Fixture: retail dat files and the shared client decoders.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use dereth_dat::{divine_type, DbType};
use dereth_physics::source::LandSource;
use dereth_physics::PhysicsWorld;
use dereth_primitives::{CellId, DataId, ObjectId, Position};
use empyrean_dat::dat_manager::{
    ITERATION_CELL, ITERATION_HIRES, ITERATION_LANGUAGE, ITERATION_PORTAL,
};
use empyrean_dat::file_types::spell_table::{compute_hash, SpellBaseExt};
use empyrean_dat::file_types::taboo_table::{TabooTableExt, OTHER_REGEX_METACHARACTERS};
use empyrean_dat::file_types::{
    ClothingTable, CombatManeuverTable, DualDidMapper, EnumMapper, EnvCell, GfxObj, LandblockInfo,
    MotionTable, PaletteSet, QualityFilter, RegionDesc, Scene, SetupModel, Texture, Wave,
};
use empyrean_dat::physics::{DatLandSource, SetupGeometryCache};
use empyrean_dat::{DatDatabase, DatFileType, DatManager, RealDats};

fn client_dir() -> PathBuf {
    dereth_dat::testing::dat_dir()
}

/// One `DatManager` for the whole binary, as the server has one per process.
fn dats() -> Arc<DatManager> {
    static DATS: OnceLock<Arc<DatManager>> = OnceLock::new();
    Arc::clone(DATS.get_or_init(|| {
        let dir = client_dir();
        let real = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        DatManager::initialize(Arc::new(real)).expect("the retail dats initialize")
    }))
}

fn ids_of(db: &DatDatabase, kind: DbType) -> Vec<u32> {
    db.all_files()
        .into_iter()
        .filter(|id| divine_type(DataId(*id)) == Some(kind))
        .collect()
}

/// Decode `ids` as `T`; answer how many decoded, and fail naming the first that did not.
fn decode_all<T: DatFileType>(db: &DatDatabase, ids: &[u32], what: &str) -> usize {
    assert!(
        !ids.is_empty(),
        "{what}: the dat has none, so this checked nothing"
    );
    let bad: Vec<u32> = ids
        .iter()
        .copied()
        .filter(|id| db.read_from_dat::<T>(*id).is_none())
        .collect();
    assert!(
        bad.is_empty(),
        "{what}: {} of {} do not decode, first 0x{:08X}",
        bad.len(),
        ids.len(),
        bad[0]
    );
    ids.len()
}

#[test]
fn every_table_in_the_inventory_decodes() {
    let t0 = Instant::now();
    let dats = dats();
    let portal = dats.portal_dat();
    assert!(
        portal.missing_tables().is_empty(),
        "{:?}",
        portal.missing_tables()
    );
    assert!(
        dats.language_dat().try_character_titles().is_some(),
        "CharacterTitles"
    );
    assert_eq!(portal.iteration(), ITERATION_PORTAL);
    assert_eq!(dats.cell_dat().iteration(), ITERATION_CELL);
    assert_eq!(dats.language_dat().iteration(), ITERATION_LANGUAGE);
    let hires = dats
        .high_res_dat()
        .expect("client_highres.dat is in the retail install");
    assert_eq!(hires.iteration(), ITERATION_HIRES);

    // ReadFromDat<T> call sites in ACE.Server (see the receipt's inventory): every file of each type.
    let mut n = 0;
    n += decode_all::<MotionTable>(portal, &ids_of(portal, DbType::MTable), "MotionTable");
    n += decode_all::<PaletteSet>(portal, &ids_of(portal, DbType::PalSet), "PaletteSet");
    n += decode_all::<SetupModel>(portal, &ids_of(portal, DbType::Setup), "SetupModel");
    n += decode_all::<ClothingTable>(portal, &ids_of(portal, DbType::Clothing), "ClothingTable");
    n += decode_all::<QualityFilter>(
        portal,
        &ids_of(portal, DbType::QualityFilter),
        "QualityFilter",
    );
    n += decode_all::<DualDidMapper>(
        portal,
        &ids_of(portal, DbType::DualDidMapper),
        "DualDidMapper",
    );
    n += decode_all::<CombatManeuverTable>(
        portal,
        &ids_of(portal, DbType::CombatTable),
        "CombatManeuverTable",
    );
    n += decode_all::<Scene>(portal, &ids_of(portal, DbType::Scene), "Scene");
    n += decode_all::<RegionDesc>(portal, &ids_of(portal, DbType::Region), "RegionDesc");
    n += decode_all::<GfxObj>(portal, &ids_of(portal, DbType::GfxObj), "GfxObj");
    n += decode_all::<EnumMapper>(portal, &ids_of(portal, DbType::EnumMapper), "EnumMapper");
    n += decode_all::<Wave>(portal, &ids_of(portal, DbType::Wave), "Wave");
    // Textures are only read by developer commands; the first 200 of each file stand for them.
    let tex: Vec<u32> = ids_of(portal, DbType::RenderSurface)
        .into_iter()
        .take(200)
        .collect();
    n += decode_all::<Texture>(portal, &tex, "Texture (portal)");
    let tex: Vec<u32> = ids_of(hires, DbType::RenderSurface)
        .into_iter()
        .take(200)
        .collect();
    n += decode_all::<Texture>(hires, &tex, "Texture (highres)");
    // CellDat: ReadFromDat<CellLandblock> / <LandblockInfo> / EnvCells, around Holtburg.
    let cell = dats.cell_dat();
    for x in 0xA8..=0xAA_u32 {
        for y in 0xB3..=0xB5_u32 {
            let block = (x << 8) | y;
            n += decode_all::<empyrean_dat::file_types::CellLandblock>(
                cell,
                &[(block << 16) | 0xFFFF],
                "CellLandblock",
            );
            if let Some(lbi) = cell.read_from_dat::<LandblockInfo>((block << 16) | 0xFFFE) {
                let ids: Vec<u32> = (0..lbi.num_cells)
                    .map(|i| (block << 16) | (0x100 + i))
                    .collect();
                if !ids.is_empty() {
                    n += decode_all::<EnvCell>(cell, &ids, "EnvCell");
                }
            }
        }
    }
    eprintln!(
        "every_table_in_the_inventory_decodes: {n} files in {:.1?}",
        t0.elapsed()
    );
}

#[test]
fn char_gen_starting_areas_resolve_to_valid_cells() {
    let dats = dats();
    let char_gen = dats.portal_dat().char_gen();
    let land = DatLandSource::new(Arc::clone(&dats)).expect("the retail region");
    let mut checked = 0;
    assert!(!char_gen.starter_areas.is_empty());
    for area in &char_gen.starter_areas {
        assert!(!area.locations.is_empty(), "{} has no locations", area.name);
        for loc in &area.locations {
            let cell = loc.cell;
            let block = cell.landblock();
            assert!(
                land.load_landblock(block),
                "{}: landblock {block} is not in the cell dat",
                area.name
            );
            if cell.is_outdoor() {
                let world = PhysicsWorld::new(Arc::new(
                    DatLandSource::new(Arc::clone(&dats)).expect("region"),
                ));
                let h = world.terrain_height_at(loc);
                assert!(h.is_some(), "{}: no terrain under {cell:?}", area.name);
            } else {
                assert!(
                    land.env_cell(cell).is_some(),
                    "{}: env cell {cell:?} did not load",
                    area.name
                );
            }
            checked += 1;
        }
    }
    for (key, hg) in &char_gen.heritage_groups {
        for &i in hg
            .primary_start_areas
            .iter()
            .chain(&hg.secondary_start_areas)
        {
            assert!(
                (i as usize) < char_gen.starter_areas.len(),
                "heritage {key} names start area {i}"
            );
        }
    }
    eprintln!("char_gen_starting_areas_resolve_to_valid_cells: {checked} locations");
}

/// Every heritage's body setup (the player creature) loads into physics and enters the world at
/// an outdoor starting location.
#[test]
fn a_common_creature_setup_loads_into_physics() {
    let dats = dats();
    let char_gen = dats.portal_dat().char_gen();
    let start: Position = *char_gen
        .starter_areas
        .iter()
        .flat_map(|a| &a.locations)
        .find(|p| p.cell.is_outdoor())
        .expect("an outdoor starting location");
    let land = Arc::new(DatLandSource::new(Arc::clone(&dats)).expect("region"));
    assert!(land.load_landblock(start.cell.landblock()));
    let setups = SetupGeometryCache::new(Arc::clone(&dats));
    let mut world = PhysicsWorld::new(land);
    let mut n = 0u32;
    for hg in char_gen.heritage_groups.values() {
        for sex in hg.sexes.values() {
            let g = setups
                .get(sex.setup.0)
                .unwrap_or_else(|| panic!("setup {}", sex.setup));
            assert!(
                !g.spheres.is_empty() || !g.cyl_spheres.is_empty(),
                "{}: no collision shape",
                sex.setup
            );
            n += 1;
            let h = world.create(ObjectId(0x5000_0000 + n), g, true);
            assert!(
                world.enter_world(h, &start),
                "{} did not enter the world at {start:?}",
                sex.setup
            );
            let placed = world.get(h).expect("placed").position;
            assert_eq!(placed.cell.landblock(), start.cell.landblock());
        }
    }
    assert!(n > 0);
}

/// Ace spell formulas agree with the shared decoder.
#[test]
fn ace_spell_formulas_agree_with_the_shared_decoder() {
    let dats = dats();
    let spells = &dats.portal_dat().spell_table().spells;
    let comps = dats.portal_dat().spell_components_table();
    let mut words_missing = 0;
    for (id, s) in spells {
        let key = (compute_hash(&s.name) % 0x1210_7680)
            .wrapping_add(compute_hash(&s.description) % 0xBEAD_CF45);
        assert_eq!(key, s.comp_key, "spell {id} {:?}: ACE's key", s.name);
        let f = s.formula();
        assert_eq!(f, s.comps, "spell {id} {:?}", s.name);
        assert!(f.iter().all(|c| *c <= 198), "spell {id}: {f:?}");
        if s.get_spell_words(comps).is_err() {
            words_missing += 1;
        }
    }
    assert_eq!(
        words_missing, 0,
        "spells whose formula names a component the table lacks"
    );
    eprintln!(
        "ace_spell_formulas_agree_with_the_shared_decoder: {} spells",
        spells.len()
    );
}

/// The taboo table has the shape under which ACE's reading of it and the shared decoder's agree,
/// and its patterns use no regex syntax beyond `*` and `.`.
#[test]
fn the_taboo_table_has_the_shape_ace_reads() {
    let dats = dats();
    let t = dats.portal_dat().taboo_table();
    assert!(!t.audiences.is_empty());
    for (key, audience) in &t.audiences {
        assert_eq!(
            audience.len(),
            1,
            "audience {key:#X} holds {} lists",
            audience.len()
        );
        for p in &audience[0].1 {
            assert!(
                !p.contains(OTHER_REGEX_METACHARACTERS),
                "pattern {p:?} has regex syntax"
            );
            assert_eq!(p, &p.to_lowercase(), "patterns are lower case");
        }
    }
    let first = t.first_entry_banned_patterns().expect("patterns");
    assert!(!first.is_empty());
    // Every whole-word pattern (no '*') bans itself.
    let word = first
        .iter()
        .find(|p| !p.contains('*') && !p.contains('.'))
        .expect("a plain pattern");
    assert!(t.contains_bad_word(&format!("Sir {}", word.to_uppercase())));
    assert!(!t.contains_bad_word("Holtburg"));
}

#[test]
fn a_starting_landblock_loads_its_cells_and_buildings() {
    let dats = dats();
    let land = DatLandSource::new(Arc::clone(&dats)).expect("region");
    let holtburg = dereth_primitives::LandblockId(0xA9B4);
    assert!(land.load_landblock(holtburg));
    let lbi = dats
        .cell_dat()
        .read_from_dat::<LandblockInfo>(holtburg.info_id().0)
        .expect("Holtburg LBI");
    assert!(lbi.num_cells > 0 && !lbi.buildings.is_empty());
    for i in 0..lbi.num_cells {
        let id = CellId(holtburg.cell(0x100).0 + i);
        assert!(land.env_cell(id).is_some(), "{id:?}");
    }
    let with_building = (1..=0x40)
        .filter(|i| land.building(holtburg.cell(*i)).is_some())
        .count();
    assert!(
        with_building > 0,
        "no building shell registered in Holtburg"
    );
}

/// The retail dats as the shared `dereth-world-data` adapters read them: straight from the store,
/// with no `DatManager` in between.
fn shared_store() -> Arc<dereth_dat::RetailDatStore> {
    Arc::new(dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("the retail dats open"))
}

/// The server land source builds what the shared one builds.
#[test]
fn the_server_land_source_builds_what_the_shared_one_builds() {
    use dereth_world_data::land_source::DatLandSource as SharedLandSource;
    let dats = dats();
    let store = shared_store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let shared = SharedLandSource::new(Arc::clone(&store), &region).expect("the shared source");
    let server = DatLandSource::new(Arc::clone(&dats)).expect("the server source");
    assert_eq!(server.height_table(), shared.height_table());
    let (mut cells, mut buildings) = (0, 0);
    for block in [0xA9B4_u16, 0x8602, 0x0007, 0xC6A9, 0x7D64] {
        let block = dereth_primitives::LandblockId(block);
        assert!(server.load_landblock(block), "{block}: not in the cell dat");
        shared.load_block_cells(block);
        let a = server.landblock(block).expect("server terrain");
        let b = shared.landblock(block).expect("shared terrain");
        assert_eq!(*a, *b, "{block}: terrain differs");
        let lbi = dats
            .cell_dat()
            .read_from_dat::<LandblockInfo>(block.info_id().0)
            .expect("LBI");
        for i in 0..lbi.num_cells {
            let id = CellId(block.cell(0x100).0 + i);
            let (a, b) = (server.env_cell(id), shared.env_cell(id));
            assert_eq!(
                format!("{a:?}"),
                format!("{b:?}"),
                "{id:?}: interior cell differs"
            );
            cells += usize::from(a.is_some());
        }
        for i in 1..=0x40 {
            let c = block.cell(i);
            assert_eq!(
                server.building_cells(c),
                shared.building_cells(c),
                "{c:?}: building cells"
            );
            let (a, b) = (server.building(c), shared.building(c));
            assert_eq!(
                format!("{a:?}"),
                format!("{b:?}"),
                "{c:?}: building shell differs"
            );
            buildings += usize::from(a.is_some());
        }
    }
    assert!(
        cells > 0 && buildings > 0,
        "compared {cells} cells and {buildings} buildings"
    );
}

/// The server setup geometry matches the shared adapter over every setup.
#[test]
fn the_server_setup_geometry_matches_the_shared_adapter_over_every_setup() {
    use dereth_world_data::setup::{
        setup_geometry_with_parts_at, simple_setup_geometry, SetupPartStats,
        PLACEMENT_FRAME_DEFAULT,
    };
    let dats = dats();
    let store = shared_store();
    let mut stats = SetupPartStats::default();
    let ids = ids_of(dats.portal_dat(), DbType::Setup);
    assert!(!ids.is_empty());
    for &id in &ids {
        let server = empyrean_dat::physics::load_setup_geometry(&dats, id);
        let setup = dats
            .portal_dat()
            .read_from_dat::<SetupModel>(id)
            .expect("setup decodes");
        let shared =
            setup_geometry_with_parts_at(&store, &setup, PLACEMENT_FRAME_DEFAULT, &mut stats);
        assert_eq!(
            format!("{server:?}"),
            format!("{:?}", Some(shared)),
            "setup 0x{id:08X}"
        );
    }
    let lbi = dats
        .cell_dat()
        .read_from_dat::<LandblockInfo>(0xA9B4_FFFE)
        .expect("Holtburg LBI");
    for b in lbi.buildings.iter().filter(|b| b.id.0 >> 24 == 0x01) {
        let server = empyrean_dat::physics::load_setup_geometry(&dats, b.id.0);
        let shared = simple_setup_geometry(&store, b.id, &mut stats);
        assert_eq!(
            format!("{server:?}"),
            format!("{shared:?}"),
            "gfxobj {:?}",
            b.id
        );
    }
    eprintln!("compared {} setups", ids.len());
}

/// The shared world data types are send and sync.
#[test]
fn the_shared_world_data_types_are_send_and_sync() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<dereth_world_data::land_source::DatLandSource>();
    send_sync::<dereth_world_data::anim_assets::DatAnimAssets>();
    send_sync::<dereth_world_data::env_cells::DecodedCell>();
    send_sync::<dereth_world_data::env_cells::EnvCellLoader>();
    send_sync::<DatLandSource>();
    send_sync::<SetupGeometryCache>();
}
