//! ACE: Source/ACE.Server/Physics/Common/Landblock.cs::GetZ
//! Landblock GetZ on terrain plane, adjust_to_outside, building-cell encounter destroyed, boot
//! preload, unloading a player landblock is a bug; real-content Holtburg terrain/building
//! agreement.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state, retail dats or world.pack in the real-content tier.

use std::sync::Arc;
use std::time::Duration;

use dereth_assets::world::BuildInfo;
use dereth_physics::land::{split_hash, LandblockCollision};
use dereth_physics::source::StaticLandSource;
use dereth_primitives::{DataId, Frame, LandblockId as PLandblockId, Quat, Vec3};
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::config_manager::ConfigManager;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_content::models::world::{Encounter, Weenie};
use empyrean_content::MemContent;
use empyrean_dat::file_types::LandblockInfo;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{PropertyDataId, PropertyString, WeenieType};
use empyrean_entity::numerics::Vector3;
use empyrean_entity::{LandblockId, ObjectGuid};
use empyrean_testkit::land;
use empyrean_world::entity::landblock;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::managers::world_manager::{self as wm, NoWire, WorldHost};
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }

    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

const LB: u16 = 0xA9B4;
const ENCOUNTER_WCID: u32 = 2002;

fn id() -> LandblockId {
    LandblockId::new(u32::from(LB) << 16 | 0xFFFF)
}

/// A ramp rising in +X: height index `10 + 3 i` at vertex column `i`, so
/// `z(x, y) = 2 * (10 + 3 x / 24) = 20 + x / 4` everywhere on the block.
fn ramp() -> StaticLandSource {
    let mut s = StaticLandSource::linear();
    s.add_ramp_block(PLandblockId(LB), 10, 3);
    s
}

/// All vertices at index 0 except vertex (1, 1), the NE corner of cell (0, 0), at index 5 (10 m).
fn one_raised_vertex() -> StaticLandSource {
    let mut s = StaticLandSource::linear();
    let mut height = [0u8; 81];
    height[9 + 1] = 5;
    let table = *dereth_physics::source::LandSource::height_table(&s);
    let block = LandblockCollision::build(
        PLandblockId(LB),
        Box::new(height),
        Box::new([0; 81]),
        false,
        8,
        &table,
    )
    .expect("a full-detail block");
    s.add_block(block);
    s
}

fn world(
    dats: Arc<empyrean_dat::DatManager>,
    content: MemContent,
    land_source: StaticLandSource,
) -> (World, VirtualClock) {
    let clock = VirtualClock::default();
    let timers = TimersState::new(&clock);
    let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
    let mut w = World::new(now, dats);
    w.timers = timers;
    w.content = Arc::new(content);
    gm::initialize(&mut w, &mut EmptyShard);
    phys_ext::use_land_source(&mut w, Arc::new(land_source));
    phys_ext::register_setup(&mut w, land::TEST_SETUP, land::test_setup_geometry());
    (w, clock)
}

fn bare(land_source: StaticLandSource) -> World {
    world(
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .unwrap(),
        MemContent::new(),
        land_source,
    )
    .0
}

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

// ---------------------------------------------------------------------------------- GetZ

#[test]
fn get_z_solves_the_terrain_plane_on_a_ramp() {
    let w = bare(ramp());
    for (x, y, want) in [
        (0.5, 0.5, 20.125),
        (100.0, 37.0, 45.0),
        (191.5, 191.5, 67.875),
        (48.0, 72.0, 32.0),
        (0.0, 0.0, 20.0),
    ] {
        let z = landblock::physics_landblock_get_z(&w, id(), v(x, y, 0.0));
        assert!(close(z, want), "({x}, {y}): {z}, want {want}");
    }
}

#[test]
fn get_z_picks_the_triangle_under_the_point() {
    // Cell (0, 0): SW (0,0,0), SE (24,0,0), NE (24,24,10), NW (0,24,0). Split SW-NE: the SE
    // triangle (SW, SE, NE) is z = 10 y / 24 and the NW one (SW, NE, NW) is z = 10 x / 24. Split
    // SE-NW: the SW triangle (SW, SE, NW) is flat at 0 and the NE one (NE, NW, SE) is
    // z = 10 (x + y - 24) / 24.
    let w = bare(one_raised_vertex());
    let sw_to_ne = split_hash(u32::from(LB >> 8) * 8, u32::from(LB & 0xFF) * 8);
    let (p, q) = (v(18.0, 10.0, 0.0), v(4.0, 6.0, 0.0));
    let (want_p, want_q) = if sw_to_ne {
        (10.0 * 10.0 / 24.0, 10.0 * 4.0 / 24.0)
    } else {
        (10.0 * 4.0 / 24.0, 0.0)
    };
    assert!(
        close(landblock::physics_landblock_get_z(&w, id(), p), want_p),
        "split sw_to_ne = {sw_to_ne}"
    );
    assert!(
        close(landblock::physics_landblock_get_z(&w, id(), q), want_q),
        "split sw_to_ne = {sw_to_ne}"
    );
}

#[test]
fn get_z_is_the_points_own_z_off_the_block_or_without_land() {
    let w = bare(ramp());
    // GetCell: outside [0, 192] there is no cell.
    assert_eq!(
        landblock::physics_landblock_get_z(&w, id(), v(-0.5, 10.0, 7.0)),
        7.0
    );
    assert_eq!(
        landblock::physics_landblock_get_z(&w, id(), v(10.0, 192.5, 7.0)),
        7.0
    );
    // x = 192 addresses cell index 0x41 and up: no land cell there.
    assert_eq!(
        landblock::physics_landblock_get_z(&w, id(), v(192.0, 10.0, 7.0)),
        7.0
    );
    // No landblock in the land source.
    let other = LandblockId::new(0x1234_FFFF);
    assert_eq!(
        landblock::physics_landblock_get_z(&w, other, v(10.0, 10.0, 7.0)),
        7.0
    );
}

// ---------------------------------------------------------------------------------- encounters

fn creature(wcid: u32) -> Weenie {
    Weenie::new(wcid, "encounter", WeenieType::Creature)
        .with_string(PropertyString::Name, "encounter")
        .with_did(PropertyDataId::Setup, land::TEST_SETUP)
}

fn encounter(id: u32, cell_x: i32, cell_y: i32) -> Encounter {
    Encounter {
        id,
        landblock: i32::from(LB),
        weenie_class_id: ENCOUNTER_WCID,
        cell_x,
        cell_y,
        ..Encounter::default()
    }
}

/// Loads the landblock and runs one `LandblockManager.Tick` (the load's delegates, which place
/// the encounters). Returns the encounters on the landblock, in its order.
fn load_and_tick(w: &mut World, clock: &VirtualClock) -> Vec<ObjectGuid> {
    lm::get_landblock(w, id(), false, false);
    w.now = ClockSnapshot::take(clock, w.timers.portal_year_ticks);
    let pyt = w.timers.portal_year_ticks;
    lm::tick(w, pyt);
    w.landblock_manager
        .landblocks
        .expect(id())
        .world_object_guids()
        .copied()
        .collect()
}

#[test]
fn encounters_stand_on_the_terrain() {
    let content = MemContent::new()
        .weenie(creature(ENCOUNTER_WCID))
        .encounter(encounter(1, 2, 3))
        .encounter(encounter(2, 8, 0));
    let (mut w, clock) = world(
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .unwrap(),
        content,
        ramp(),
    );
    let placed = load_and_tick(&mut w, &clock);
    assert_eq!(placed.len(), 2);

    // (2, 3): origin (48, 72); z = 20 + 48 / 4 = 32. (The cell is the physics world's once the
    // body has entered it: the origin is on a cell corner.)
    let at = w.objects.get(placed[0]).unwrap().location().unwrap();
    assert_eq!(at.cell() >> 16, u32::from(LB));
    assert!(
        close(at.position_x, 48.0) && close(at.position_y, 72.0),
        "{at:?}"
    );
    assert!((at.position_z - 32.0).abs() < 0.01, "{at:?}");

    // (8, 0): x = 192 clamps to 191.5; z = 20 + 191.5 / 4.
    let at = w.objects.get(placed[1]).unwrap().location().unwrap();
    assert!(
        close(at.position_x, 191.5) && close(at.position_y, 0.5),
        "{at:?}"
    );
    assert!(
        (at.position_z - (20.0 + 191.5 / 4.0)).abs() < 0.01,
        "{at:?}"
    );
}

#[test]
fn adjust_to_outside_names_the_land_cell_under_the_origin() {
    let start = u32::from(LB) << 16 | 1;
    type Case = ((f32, f32), u32, (f32, f32));
    let cases: [Case; 5] = [
        // cell (2, 3): index 2 * 8 + 3 + 1 = 0x14
        ((48.0, 72.0), 0xA9B4_0014, (48.0, 72.0)),
        // cell (7, 0): index 0x39
        ((191.5, 0.5), 0xA9B4_0039, (191.5, 0.5)),
        // past the east edge: landblock AAB4, rebased by 192
        ((200.0, 10.0), 0xAAB4_0001, (8.0, 10.0)),
        // past the west edge: landblock A8B4
        ((-5.0, 10.0), 0xA8B4_0039, (187.0, 10.0)),
        // |x| under EPSILON snaps to 0
        ((0.000_01, 30.0), 0xA9B4_0002, (0.0, 30.0)),
    ];
    for ((x, y), want_cell, (wx, wy)) in cases {
        let mut origin = v(x, y, 5.0);
        let cell = landblock::physics_position_adjust_to_outside(start, &mut origin);
        assert_eq!(cell, want_cell, "({x}, {y})");
        assert!(
            close(origin.x, wx) && close(origin.y, wy) && origin.z == 5.0,
            "({x}, {y}) -> {origin:?}"
        );
    }
    // Off the world: the cell id is 0.
    let mut origin = v(-1.0, 10.0, 0.0);
    assert_eq!(
        landblock::physics_position_adjust_to_outside(0x0010_0001, &mut origin),
        0
    );
}

#[test]
fn an_encounter_in_a_cell_with_a_building_is_destroyed() {
    // A building whose frame origin (60, 84) is in cell (2, 3); the encounter at (2, 3) lands in
    // the same land cell and is destroyed; the one at (4, 4) stays.
    let info = LandblockInfo {
        id: DataId(u32::from(LB) << 16 | 0xFFFE),
        num_cells: 0,
        objects: vec![],
        pack_mask: 0,
        buildings: vec![BuildInfo {
            id: DataId(0x0100_0001),
            frame: Frame::new(Vec3::new(60.0, 84.0, 0.0), Quat::IDENTITY),
            num_leaves: 0,
            portals: vec![],
        }],
        restrictions: None,
    };
    let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_cell(u32::from(LB) << 16 | 0xFFFE, info)
        .build()
        .unwrap();
    let content = MemContent::new()
        .weenie(creature(ENCOUNTER_WCID))
        .encounter(encounter(1, 2, 3))
        .encounter(encounter(2, 4, 4));
    let (mut w, clock) = world(dats, content, ramp());
    let placed = load_and_tick(&mut w, &clock);
    assert_eq!(
        placed.len(),
        1,
        "only the encounter outside the building's cell is placed"
    );
    let at = w.objects.get(placed[0]).unwrap().location().unwrap();
    assert!(
        close(at.position_x, 96.0) && close(at.position_y, 96.0),
        "{at:?}"
    );

    assert!(landblock::lscape_get_landcell_has_building(
        &w,
        id(),
        u32::from(LB) << 16 | 0x14
    ));
    assert!(!landblock::lscape_get_landcell_has_building(
        &w,
        id(),
        u32::from(LB) << 16 | 0x13
    ));
    // An interior cell is not a SortCell.
    assert!(!landblock::lscape_get_landcell_has_building(
        &w,
        id(),
        u32::from(LB) << 16 | 0x0100
    ));
}

// ---------------------------------------------------------------------------------- preload

/// A host that stops the loop after its first iteration.
struct StopAfterOne;

impl WorldHost for StopAfterOne {
    fn sleep(&mut self, _duration: Duration) {}

    fn between_iterations(&mut self, w: &mut World) {
        wm::stop_world(w);
    }
}

#[test]
fn the_world_thread_preloads_the_configured_landblocks_before_its_loop() {
    ConfigManager::initialize(MasterConfiguration::default());
    let clock = VirtualClock::default();
    let mut w = bare(ramp());
    let hebian_to = LandblockId::new(0xE74E_FFFF);
    assert!(w.landblock_manager.landblocks.get(hebian_to).is_none());

    wm::world_thread(&mut w, &mut NoWire, &clock, &mut StopAfterOne);

    let l = w
        .landblock_manager
        .landblocks
        .get(hebian_to)
        .expect("Hebian-To is loaded");
    assert!(l.permaload);
    assert_eq!(
        lm::get_loaded_landblocks(&w).len(),
        1,
        "no adjacents, no disabled entries"
    );
}

/// Unloading a landblock that holds a player is a porting bug.

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "unloaded while holding player")]
fn unloading_a_landblock_that_holds_a_player_is_a_porting_bug() {
    // ACE never unloads a landblock with a player on it (players keep it awake): reaching Unload
    // with one is a porting bug, loud in debug builds.
    let mut w = bare(ramp());
    lm::get_landblock(&mut w, id(), false, false);
    let g = ObjectGuid::new(0x5000_0001);
    let mut p = WorldObject {
        guid: g,
        creature: Some(Box::default()),
        container: Some(Box::default()),
        player: Some(Box::default()),
        ..WorldObject::default()
    };
    p.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    p.set_heartbeat_interval(Some(0.0));
    empyrean_world::world_objects::world_object_tick::world_object_initialize_heartbeats(
        &mut p, 0.0,
    );
    p.wo.world_object_database.biota_originated_from_database = true;
    w.objects.insert(p).unwrap();
    w.objects
        .get_mut(g)
        .unwrap()
        .set_location(Some(empyrean_entity::Position::from_components(
            u32::from(LB) << 16 | 1,
            10.0,
            10.0,
            25.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        )));
    assert!(lm::add_object(&mut w, g, false));
    landblock::unload(&mut w, id());
}

// ---------------------------------------------------------------------------------- real content

/// The real-content tier: the retail dats under `DERETH_TEST_DAT_DIR` and
/// `world.pack` under `EMPYREAN_TEST_WORLD_PACK` (default `world.pack` in the repository).
#[cfg(feature = "real-content")]
mod real_content {

    use empyrean_content::PackContent;
    use empyrean_dat::{DatManager, RealDats};

    use super::*;

    /// Landblock 0x45F6: outdoor, the most encounters of any landblock in ACE's world database (37).
    const REAL_LB: u32 = 0x45F6_FFFF;

    fn dats() -> Arc<DatManager> {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        DatManager::initialize(Arc::new(source)).expect("retail dats")
    }

    fn pack() -> PackContent {
        let path = empyrean_common::test_paths::world_pack();
        PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        })
    }

    #[test]
    fn encounters_on_a_real_landblock_stand_on_the_shared_crates_terrain() {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let mut w = World::new(
            ClockSnapshot::take(&clock, timers.portal_year_ticks),
            dats(),
        );
        w.timers = timers;
        let content = pack();
        let encounters = empyrean_content::WorldDatabase::get_cached_encounters_by_landblock(
            &content,
            (REAL_LB >> 16) as u16,
        );
        assert!(
            encounters.len() > 10,
            "0x45F6 has encounters in world.pack: {}",
            encounters.len()
        );
        let encounter_count = u32::try_from(encounters.len()).unwrap();
        w.content = Arc::new(content);
        gm::initialize(&mut w, &mut EmptyShard);

        let lb = LandblockId::new(REAL_LB);
        lm::get_landblock(&mut w, lb, false, false);
        w.now = ClockSnapshot::take(&clock, w.timers.portal_year_ticks);
        let pyt = w.timers.portal_year_ticks;
        lm::tick(&mut w, pyt);

        let objects: Vec<ObjectGuid> = w
            .landblock_manager
            .landblocks
            .expect(lb)
            .world_object_guids()
            .copied()
            .collect();
        let mut checked = 0;
        let mut above_zero = 0;
        for g in objects {
            // The encounters drew the first dynamic guids at load, one each, in order (the
            // landblock's own instances are static; later dynamic guids are what the encounters'
            // generators spawned, placed by physics around them).
            if !(ObjectGuid::DYNAMIC_MIN..ObjectGuid::DYNAMIC_MIN + encounter_count)
                .contains(&g.full())
            {
                continue;
            }
            let at = w.objects.get(g).unwrap().location().unwrap();
            assert!(at.cell() & 0xFFFF < 0x100, "outdoors: {at:?}");
            let ground = w
                .physics
                .terrain_height_at(&phys_ext::to_physics_position(&at))
                .expect("terrain under it");
            assert!(
                (at.position_z - ground).abs() < 0.01,
                "{g}: z {} vs terrain {ground}",
                at.position_z
            );
            checked += 1;
            if at.position_z > 1.0 {
                above_zero += 1;
            }
        }
        assert_eq!(
            checked, encounter_count,
            "every encounter is placed (no building cells on 0x45F6)"
        );
        assert_eq!(
            above_zero, checked,
            "every encounter stands on the ground, not at z = 0"
        );
    }

    #[test]
    fn holtburgs_building_cells_match_the_dat_land_sources_buildings() {
        // Landblock A9B4 (Holtburg): the land cells SortCell.has_building answers for are the
        // cells the shared land source keeps a building shell in.
        let clock = VirtualClock::default();
        let mut w = World::new(ClockSnapshot::take(&clock, 0.0), dats());
        gm::initialize(&mut w, &mut EmptyShard);
        let lb = LandblockId::new(0xA9B4_FFFF);
        lm::get_landblock(&mut w, lb, false, false);
        let mut ours = Vec::new();
        let mut shells = Vec::new();
        for index in 1..=64u32 {
            let cell = 0xA9B4_0000 | index;
            if landblock::lscape_get_landcell_has_building(&w, lb, cell) {
                ours.push(cell);
            }
            if dereth_physics::source::LandSource::building(
                w.physics.land(),
                dereth_primitives::CellId(cell),
            )
            .is_some()
            {
                shells.push(cell);
            }
        }
        assert!(ours.len() > 5, "Holtburg has buildings: {ours:X?}");
        assert_eq!(ours, shells);
    }
}
