//! ACE: Source/ACE.Server/Entity/PositionExtensions.cs::IsWalkable
//! Walkability/terrain Z/find Z/restrictable cells; spawn specific/scatter placement and slope
//! refusal; chest generator treasure in inventory; real-content Holtburg generators, spot spawns on
//! structures over steep ground, and terrain agreement.
//! Divergence: V534
//! Fixture: synthetic dats, isolated world state, retail dats or world.pack in the real-content tier.

use std::sync::Arc;

use dereth_physics::source::StaticLandSource;
use dereth_primitives::LandblockId as PLandblockId;
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::{
    TreasureDeath, Weenie as WeenieRow, WeeniePropertiesGenerator,
};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    PositionType, PropertyDataId, PropertyInt, PropertyString, RegenLocationType, WeenieType,
};
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_testkit::land;
use empyrean_world::entity::generator_profile;
use empyrean_world::entity::landblock::Landblock;
use empyrean_world::entity::position_extensions as pe;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::guid_manager as gm;
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::managers::property_manager as pm;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::container;
use empyrean_world::world_objects::world_object::CtorEnv;
use empyrean_world::world_objects::world_object_tick as tick;
use empyrean_world::World;

// ------------------------------------------------------------------------------------ harness

use empyrean_testkit::EmptyShard;

const LB: u16 = 0xA9B4;
const GEN: u32 = 0x7A9B_4200;
const SEED: u64 = 5454;

const CREATURE_WCID: u32 = 3000;
const COINSTACK_WCID: u32 = 273; // WeenieClassName.coinstack
const SPECIFIC_GEN_WCID: u32 = 1000;
const SCATTER_GEN_WCID: u32 = 1001;
const CHEST_WCID: u32 = 1002;
/// A `treasure_death` DID: tier 1, exactly two mundane items, pyreals only.
const PYREALS_DID: u32 = 9001;

fn lb_id() -> LandblockId {
    LandblockId::new(u32::from(LB) << 16 | 0xFFFF)
}

fn pyreal_profile() -> TreasureDeath {
    TreasureDeath {
        id: 1,
        treasure_type: PYREALS_DID,
        tier: 1,
        mundane_item_chance: 100,
        mundane_item_min_amount: 2,
        mundane_item_max_amount: 2,
        mundane_item_type_selection_chances: 6,
        ..TreasureDeath::default()
    }
}

fn setup_weenie(wcid: u32, class_name: &str, weenie_type: WeenieType, name: &str) -> WeenieRow {
    WeenieRow::new(wcid, class_name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, land::TEST_SETUP)
}

/// A generator of `weenie_type` at `(84, 7.1, z)` in cell 0x19 (cell x 3, y 0) with one profile
/// row: one object, created at once.
fn generator(
    wcid: u32,
    weenie_type: WeenieType,
    z: f32,
    row: WeeniePropertiesGenerator,
) -> WeenieRow {
    let mut g = setup_weenie(wcid, "gen", weenie_type, "gen")
        .with_int(PropertyInt::InitGeneratedObjects, 1)
        .with_int(PropertyInt::MaxGeneratedObjects, 1)
        .with_int(PropertyInt::ItemsCapacity, 120)
        .with_int(PropertyInt::ContainersCapacity, 10)
        .with_float(
            empyrean_entity::enums::PropertyFloat::RegenerationInterval,
            60.0,
        )
        .with_position(
            PositionType::Location,
            u32::from(LB) << 16 | 0x0019,
            [84.0, 7.1, z],
            [1.0, 0.0, 0.0, 0.0],
        );
    g.weenie_properties_generator = vec![WeeniePropertiesGenerator {
        id: 1,
        object_id: wcid,
        ..row
    }];
    g
}

/// A creature profile: `where` placement, 2 m east and 3 m north of the generator (the origin is
/// given in full: ACE's `PositionX + OriginX ?? 0` puts a null component at 0).
fn creature_row(where_create: RegenLocationType) -> WeeniePropertiesGenerator {
    WeeniePropertiesGenerator {
        probability: -1.0,
        weenie_class_id: CREATURE_WCID,
        init_create: 1,
        max_create: 1,
        where_create: where_create.0,
        origin_x: Some(2.0),
        origin_y: Some(3.0),
        origin_z: Some(0.0),
        ..Default::default()
    }
}

fn content(generator_z: f32) -> MemContent {
    let chest_treasure = WeeniePropertiesGenerator {
        probability: -1.0,
        weenie_class_id: PYREALS_DID,
        init_create: 1,
        max_create: 1,
        where_create: (RegenLocationType::Contain.0 | RegenLocationType::Treasure.0),
        ..Default::default()
    };
    MemContent::new()
        .weenie(setup_weenie(
            CREATURE_WCID,
            "drudge",
            WeenieType::Creature,
            "Drudge",
        ))
        .weenie(
            setup_weenie(COINSTACK_WCID, "coinstack", WeenieType::Coin, "Pyreal")
                .with_int(PropertyInt::StackUnitValue, 1)
                .with_int(PropertyInt::StackUnitEncumbrance, 0)
                .with_int(PropertyInt::MaxStackSize, 25000),
        )
        .weenie(generator(
            SPECIFIC_GEN_WCID,
            WeenieType::Generic,
            generator_z,
            creature_row(RegenLocationType::Specific),
        ))
        .weenie(generator(
            SCATTER_GEN_WCID,
            WeenieType::Generic,
            generator_z,
            creature_row(RegenLocationType::Scatter),
        ))
        .weenie(generator(
            CHEST_WCID,
            WeenieType::Chest,
            generator_z,
            chest_treasure,
        ))
        .treasure_death(pyreal_profile())
}

/// A ramp rising in +X by `rise` height indices per vertex column from index `base`.
fn ramp(base: u8, rise: u8) -> StaticLandSource {
    let mut s = StaticLandSource::linear();
    s.add_ramp_block(PLandblockId(LB), base, rise);
    s
}

/// A world on `land` (with the synthetic setup registered), the stat tables and `content`.
fn world(land_source: StaticLandSource, content: MemContent, dats: FakeDats) -> World {
    let clock = VirtualClock::default();
    let timers = TimersState::new(&clock);
    let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
    let dats = empyrean_testkit::dats::with_stat_tables(dats)
        .build()
        .expect("fake dats");
    let mut w = World::new(now, dats);
    w.timers = timers;
    w.content = Arc::new(content);
    gm::initialize(&mut w, &mut EmptyShard);
    phys_ext::use_land_source(&mut w, Arc::new(land_source));
    phys_ext::register_setup(&mut w, land::TEST_SETUP, land::test_setup_geometry());
    pm::initialize(&mut w, true);
    ThreadSafeRandom::seed(SEED);
    w
}

/// Places the generator `wcid` on its landblock and runs its first `GeneratorUpdate` and
/// `GeneratorRegeneration` (the initial spawn).
fn spawn_from(w: &mut World, wcid: u32) -> ObjectGuid {
    lm::get_landblock(w, lb_id(), false, false);
    let g = ObjectGuid::new(GEN);
    let o = CtorEnv::with_world(w, |env| {
        factory::create_new_world_object_by_wcid(env, wcid, g)
    })
    .expect("weenie exists");
    w.objects.insert(o).expect("fresh guid");
    assert!(lm::add_object(w, g, false), "the generator is placed");
    let now = w.now.unix_time;
    tick::generator_update(w, g, now);
    tick::generator_regeneration(w, g, now);
    g
}

fn spawned(w: &World, g: ObjectGuid) -> Vec<ObjectGuid> {
    w.objects
        .get(g)
        .unwrap()
        .wo
        .world_object_generators
        .generator_profiles[0]
        .spawned
        .keys()
        .map(|&k| ObjectGuid::new(k))
        .collect()
}

fn pos(cell: u32, x: f32, y: f32, z: f32) -> Position {
    Position::from_components(cell, x, y, z, 0.0, 0.0, 0.0, 1.0, false)
}

fn bare(land_source: StaticLandSource) -> World {
    world(land_source, MemContent::new(), FakeDats::new())
}

// ------------------------------------------------------------------------------------ IsWalkable

#[test]
fn flat_land_is_walkable() {
    let w = bare(ramp(10, 0));
    for (cell, x, y) in [
        (0x0001, 1.0, 1.0),
        (0x0019, 86.0, 10.1),
        (0x0040, 190.0, 190.0),
    ] {
        assert!(
            pe::is_walkable(&w, &pos(u32::from(LB) << 16 | cell, x, y, 20.0)),
            "cell {cell:#x}"
        );
    }
}

#[test]
fn walkability_ends_at_the_floor_z_slope() {
    // rise 13: n.z = 24 / sqrt(24^2 + 26^2) = 0.6783 >= FloorZ; rise 14: 24 / sqrt(24^2 + 28^2) = 0.6508.
    let w = bare(ramp(0, 13));
    assert!(pe::is_walkable(
        &w,
        &pos(u32::from(LB) << 16 | 0x0019, 86.0, 10.0, 0.0)
    ));
    let w = bare(ramp(0, 14));
    assert!(!pe::is_walkable(
        &w,
        &pos(u32::from(LB) << 16 | 0x0019, 86.0, 10.0, 0.0)
    ));
}

#[test]
fn walkability_reads_the_cell_the_position_names() {
    // `LScape.get_landcell(p.Cell)`: a point outside that cell's two triangles has no walkable
    // polygon, even on flat land. (`Position`'s constructor resolves the cell from the
    // coordinates, so the X is moved afterwards, as `PositionX += ..` does.)
    let w = bare(ramp(10, 0));
    let mut p = pos(u32::from(LB) << 16 | 0x0001, 1.0, 5.0, 20.0);
    assert!(pe::is_walkable(&w, &p));
    p.position_x = 30.0;
    assert_eq!(p.cell(), u32::from(LB) << 16 | 0x0001);
    assert!(!pe::is_walkable(&w, &p));
    // Indoors is always walkable; a landblock the physics land does not have is not.
    assert!(pe::is_walkable(
        &w,
        &pos(u32::from(LB) << 16 | 0x0100, 30.0, 5.0, 20.0)
    ));
    assert!(!pe::is_walkable(&w, &pos(0x1234_0001, 1.0, 1.0, 0.0)));
}

// ------------------------------------------------------------------------------------ GetTerrainZ, FindZ

#[test]
fn terrain_z_solves_the_outdoor_cells_plane() {
    // ramp(10, 3): z = 2 (10 + 3 x / 24) = 20 + x / 4.
    let w = bare(ramp(10, 3));
    let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
    let z = pe::get_terrain_z(&w, &pos(u32::from(LB) << 16 | 0x0019, 86.0, 10.0, 500.0));
    assert!(close(z, 20.0 + 86.0 / 4.0), "{z}");
    // The outdoor cell is taken from the coordinates, so an indoor position reads the terrain too.
    let z = pe::get_terrain_z(&w, &pos(u32::from(LB) << 16 | 0x0105, 48.0, 100.0, -7.0));
    assert!(close(z, 32.0), "{z}");
    // No land cell (off the physics land): the position's own Z.
    assert_eq!(pe::get_terrain_z(&w, &pos(0x1234_0001, 1.0, 1.0, 3.5)), 3.5);
}

#[test]
fn find_z_takes_the_env_cells_origin() {
    let cell = dereth_assets::EnvCell {
        id: dereth_primitives::DataId(0xA9B4_0100),
        flags: 0,
        cell_id_repeat: 0xA9B4_0100,
        surfaces: Vec::new(),
        environment: dereth_primitives::DataId(0x0D00_0001),
        cell_struct: 0,
        frame: dereth_primitives::Frame::new(
            dereth_primitives::Vec3::new(10.0, 20.0, -12.5),
            dereth_primitives::Quat::IDENTITY,
        ),
        portals: Vec::new(),
        visible_cells: Vec::new(),
        static_objects: Vec::new(),
        restriction_obj: None,
    };
    let w = world(
        ramp(10, 0),
        MemContent::new(),
        FakeDats::new().with_cell(0xA9B4_0100, cell),
    );
    let mut p = pos(0xA9B4_0100, 1.0, 2.0, 3.0);
    pe::find_z(&w, &mut p);
    assert_eq!(p.position_z, -12.5);
    // A cell missing from the dat reads as ACE's empty EnvCell: origin Z 0.
    let mut p = pos(0xA9B4_0101, 1.0, 2.0, 3.0);
    pe::find_z(&w, &mut p);
    assert_eq!(p.position_z, 0.0);
}

// ------------------------------------------------------------------------------------ IsRestrictable

#[test]
fn restrictable_cells_are_house_cells() {
    // 0x3B9C000A is the first HouseCell. Cell index 0x0A is cell x 1, y 1: (24..48, 24..48).
    let mut w = bare(ramp(10, 0));
    let mut outdoors = Landblock::new(&mut w, LandblockId::new(0x3B9C_FFFF));
    assert!(pe::is_restrictable(
        &pos(0x3B9C_000A, 30.0, 30.0, 0.0),
        &mut outdoors
    ));
    assert!(!pe::is_restrictable(
        &pos(0x3B9C_000B, 30.0, 50.0, 0.0),
        &mut outdoors
    ));
    // Not a dungeon: the outdoor cell under the coordinates, even for an indoor cell id.
    assert!(pe::is_restrictable(
        &pos(0x3B9C_0100, 30.0, 30.0, 0.0),
        &mut outdoors
    ));

    // A dungeon (all heights 0, cells, no buildings): the position's own cell.
    let info = dereth_assets::LandblockInfo {
        id: dereth_primitives::DataId(0x3B9C_FFFE),
        num_cells: 1,
        objects: Vec::new(),
        pack_mask: 0,
        buildings: Vec::new(),
        restrictions: None,
    };
    let dats = FakeDats::new()
        .with_flat_landblock(PLandblockId(0x3B9C), 0)
        .with_cell(0x3B9C_FFFE, info);
    let mut w = world(ramp(10, 0), MemContent::new(), dats);
    let mut dungeon = Landblock::new(&mut w, LandblockId::new(0x3B9C_FFFF));
    assert!(!pe::is_restrictable(
        &pos(0x3B9C_0100, 30.0, 30.0, 0.0),
        &mut dungeon
    ));
    let mut p = pos(0x3B9C_000A, 30.0, 30.0, 0.0);
    (p.position_x, p.position_y) = (90.0, 90.0); // outdoor cell 0x3B9C001C by the coordinates
    assert!(pe::is_restrictable(&p, &mut dungeon));
    assert!(!pe::is_restrictable(&p, &mut outdoors));
}

// ------------------------------------------------------------------------------------ spawning

/// Placed: the object entered the world on the test landblock (`LandblockManager.AddObject`).
fn is_on_landblock(w: &World, g: ObjectGuid) -> bool {
    w.objects
        .get(g)
        .is_some_and(|o| o.current_landblock == Some(lb_id()))
}

#[test]
fn an_outdoor_spawn_specific_places_its_creature() {
    let mut w = world(ramp(10, 0), content(20.0), FakeDats::new());
    let g = spawn_from(&mut w, SPECIFIC_GEN_WCID);
    let s = spawned(&w, g);
    assert_eq!(s.len(), 1);
    assert!(is_on_landblock(&w, s[0]), "the drudge is in the world");
    let at = w.objects.get(s[0]).unwrap().location().unwrap();
    assert_eq!(
        (at.cell(), at.position_x, at.position_y),
        (u32::from(LB) << 16 | 0x0019, 86.0, 10.1)
    );
    assert!((at.position_z - 20.0).abs() < 0.01, "on the ground: {at:?}");
}

#[test]
fn an_outdoor_spawn_scatter_places_its_creature() {
    let mut w = world(ramp(10, 0), content(20.0), FakeDats::new());
    let g = spawn_from(&mut w, SCATTER_GEN_WCID);
    let s = spawned(&w, g);
    assert_eq!(s.len(), 1);
    assert!(is_on_landblock(&w, s[0]), "the drudge is in the world");
}

#[test]
fn a_spawn_specific_on_a_steep_slope_is_refused() {
    // rise 20: n.z = 24 / sqrt(24^2 + 40^2) = 0.51. The generator (x 84) stands at z = 70.
    let mut w = world(ramp(0, 20), content(70.0), FakeDats::new());
    let g = spawn_from(&mut w, SPECIFIC_GEN_WCID);
    // FirstSpawn keeps the failed spawn in the profile (ACE's retry guard); the object is destroyed.
    let s = spawned(&w, g);
    assert_eq!(s.len(), 1);
    assert!(w.objects.get(s[0]).is_none(), "destroyed, never placed");

    // The same slope under a scatter spawn: every scatter point must stand on walkable terrain,
    // so none of the twenty tries places it either.
    let mut w = world(ramp(0, 20), content(70.0), FakeDats::new());
    let g = spawn_from(&mut w, SCATTER_GEN_WCID);
    let s = spawned(&w, g);
    assert!(w.objects.get(s[0]).is_none(), "destroyed, never placed");
}

/// A scatter generator's spawns land at random points within its radius of the generator (plus
/// the profile's offset), each set on the ground, and not all on the one spot.
#[test]
fn scatter_spawns_spread_within_the_generator_radius_on_the_ground() {
    const RADIUS: f32 = 10.0;
    let row = WeeniePropertiesGenerator {
        init_create: 6,
        max_create: 6,
        ..creature_row(RegenLocationType::Scatter)
    };
    let gen = generator(SCATTER_GEN_WCID, WeenieType::Generic, 20.0, row)
        .with_int(PropertyInt::InitGeneratedObjects, 6)
        .with_int(PropertyInt::MaxGeneratedObjects, 6)
        .with_float(
            empyrean_entity::enums::PropertyFloat::GeneratorRadius,
            f64::from(RADIUS),
        );
    let content = MemContent::new()
        .weenie(setup_weenie(
            CREATURE_WCID,
            "drudge",
            WeenieType::Creature,
            "Drudge",
        ))
        .weenie(gen);
    // a gentle ramp (rise 5: walkable), so the ground height differs from point to point
    let mut w = world(ramp(10, 5), content, FakeDats::new());
    empyrean_common::not_ported::take_local();
    let g = spawn_from(&mut w, SCATTER_GEN_WCID);
    assert!(!empyrean_common::not_ported::take_local().contains_key("ACE: WorldObject.ScatterPos"));

    let s = spawned(&w, g);
    assert_eq!(s.len(), 6);
    let mut spots = Vec::new();
    for c in &s {
        assert!(is_on_landblock(&w, *c), "every drudge is in the world");
        let at = w.objects.get(*c).unwrap().location().unwrap();
        // the generator at (84, 7.1) plus the profile's (2, 3) offset
        assert!(
            (at.position_x - 86.0).abs() <= RADIUS + 0.01
                && (at.position_y - 10.1).abs() <= RADIUS + 0.01,
            "{at:?}"
        );
        let ground = pe::get_terrain_z(&w, &at);
        assert!(
            (at.position_z - ground).abs() < 0.1,
            "on the ground: {at:?} over {ground}"
        );
        spots.push((at.position_x, at.position_y));
        assert!(
            w.objects
                .get(*c)
                .unwrap()
                .wo
                .world_object
                .scatter_pos
                .is_none(),
            "the request is cleared after EnterWorld"
        );
    }
    spots.dedup();
    assert!(spots.len() > 1, "scattered, not stacked: {spots:?}");
}

#[test]
fn a_chest_generators_treasure_lands_in_the_chests_inventory() {
    let mut w = world(ramp(10, 0), content(20.0), FakeDats::new());
    let g = spawn_from(&mut w, CHEST_WCID);
    let s = spawned(&w, g);
    assert_eq!(s.len(), 2, "two pyreal stacks");
    // `Container.TryAddToInventory` at placement 0: each new item pushes the earlier ones up.
    assert_eq!(
        container::inventory_values(&w, g),
        s,
        "in the chest's Inventory, in spawn order"
    );
    for (i, item) in s.iter().enumerate() {
        let o = w.objects.get(*item).unwrap();
        assert_eq!(o.biota.weenie_class_id, COINSTACK_WCID);
        assert_eq!(o.container_id(), Some(GEN));
        assert_eq!(o.wo.world_object_properties.container, Some(g));
        assert!(o.location().is_none(), "in the chest, not on the landscape");
        assert_eq!(
            o.placement_position(),
            Some(i32::try_from(s.len() - 1 - i).unwrap())
        );
    }
    let value: i32 = s
        .iter()
        .map(|i| w.objects.get(*i).unwrap().value().unwrap())
        .sum();
    assert_eq!(
        w.objects.get(g).unwrap().value(),
        Some(value),
        "the chest's Value counts its contents"
    );

    // `GeneratorProfile.Reset`: `wo.Container == Generator`, so each leaves the chest, then is destroyed.
    generator_profile::reset(&mut w, g, 0);
    assert!(container::inventory_values(&w, g).is_empty());
    assert!(s.iter().all(|i| w.objects.get(*i).is_none()));
    assert!(spawned(&w, g).is_empty());
}

#[test]
fn remove_treasure_empties_the_chest() {
    let mut w = world(ramp(10, 0), content(20.0), FakeDats::new());
    let g = spawn_from(&mut w, CHEST_WCID);
    let s = spawned(&w, g);
    assert_eq!(container::inventory_values(&w, g).len(), 2);
    generator_profile::remove_treasure(&mut w, g, 0);
    assert!(container::inventory_values(&w, g).is_empty());
    assert!(s.iter().all(|i| w.objects.get(*i).is_none()), "destroyed");
    assert_eq!(w.objects.get(g).unwrap().value().unwrap_or(0), 0);
}

// ------------------------------------------------------------------------------------ real content

/// The real-content tier: the retail dats under `DERETH_TEST_DAT_DIR` and
/// `world.pack` under `EMPYREAN_TEST_WORLD_PACK` (default `world.pack` in the repository).
#[cfg(feature = "real-content")]
mod real_content {

    use empyrean_content::PackContent;
    use empyrean_dat::{DatManager, RealDats};

    use super::*;

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

    /// The generator spawns ACE's slope check refuses because the terrain under them is steep,
    /// though they stand on a structure above it: the link child's instance, its weenie, the cell
    /// and spot it is recorded at, and whether retail created it there.
    const STEEP_SPAWNS: [(u32, u32, u32, f32, f32, f32, bool); 36] = [
        (
            0x7070F006, 25861, 0x070F002C, 132.618, 83.0841, 187.805, true,
        ),
        (0x70C0900C, 25860, 0x0C090024, 96.104, 84.721, 94.8, true),
        (
            0x71BBB001, 11519, 0x1BBB0030, 140.235, 172.706, 82.006, true,
        ),
        (0x71BBB002, 11519, 0x1BBB0030, 134.712, 189.78, 82.006, true),
        (0x71BBB003, 11517, 0x1BBB0030, 126.193, 187.95, 82.055, true),
        (0x71FBC001, 11695, 0x1FBC0023, 104.084, 59.292, 82.005, true),
        (
            0x71FBC002, 11695, 0x1FBC0022, 114.665, 34.5154, 82.005, true,
        ),
        (
            0x71FBC004, 11695, 0x1FBC0021, 110.323, 11.6555, 82.005, true,
        ),
        (
            0x71FBC005, 11695, 0x1FBC0021, 115.019, 1.15979, 82.005, true,
        ),
        (
            0x71FBC007, 11481, 0x1FBC0022, 119.573, 27.0413, 83.505, true,
        ),
        (
            0x7344F003, 20632, 0x344F0040, 186.349, 177.211, 139.58, false,
        ),
        (
            0x7344F004, 20633, 0x344F0040, 178.232, 172.12, 139.584, false,
        ),
        (
            0x7344F005, 20633, 0x344F0040, 174.868, 175.138, 139.584, false,
        ),
        (
            0x73550006, 20634, 0x3550000F, 35.8467, 147.486, 119.584, false,
        ),
        (
            0x73550007, 20634, 0x35500017, 59.993, 156.164, 124.599, false,
        ),
        (
            0x73550008, 20633, 0x3550000F, 44.0242, 156.26, 119.584, false,
        ),
        (
            0x7A95300E, 20633, 0xA953002D, 124.868, 101.261, 179.345, true,
        ),
        (
            0x7A95300F, 20632, 0xA953002D, 132.815, 112.456, 179.341, true,
        ),
        (
            0x7A953010, 20632, 0xA953002D, 130.355, 111.865, 179.341, true,
        ),
        (
            0x7A953011, 20632, 0xA953002D, 131.284, 103.384, 189.252, false,
        ),
        (
            0x7A953012, 20633, 0xA953002D, 131.872, 103.612, 189.58, true,
        ),
        (
            0x7A953018, 20632, 0xA953000D, 43.771, 116.585, 179.297, true,
        ),
        (
            0x7A953019, 20633, 0xA953000D, 45.9993, 112.158, 179.301, true,
        ),
        (
            0x7A95301A, 20633, 0xA953000D, 33.9455, 113.193, 179.301, true,
        ),
        (
            0x7A95301B, 20633, 0xA953000D, 41.886, 114.292, 186.973, true,
        ),
        (
            0x7A95301C, 20632, 0xA953000D, 38.1171, 110.255, 192.235, true,
        ),
        (0x71BBC004, 11519, 0x1BBC002B, 139.98, 65.7269, 82.006, true),
        (
            0x71BBC005, 11519, 0x1BBC002A, 132.037, 38.8309, 82.006, true,
        ),
        (
            0x71BBC006, 11519, 0x1BBC0029, 132.365, 9.25148, 82.006, true,
        ),
        (
            0x71BBC007, 11517, 0x1BBC002A, 127.812, 24.565, 82.0065, true,
        ),
        (
            0x71BBC008, 11520, 0x1BBC0029, 120.552, 18.6564, 83.506, true,
        ),
        (
            0x71BBC00B, 11520, 0x1BBC002B, 127.762, 57.1209, 82.006, true,
        ),
        (
            0x71BBC00C, 11520, 0x1BBC002B, 126.468, 55.5423, 82.006, true,
        ),
        (0x720BF013, 214, 0x20BF000D, 39.4272, 107.525, 82.005, true),
        (0x720BF014, 214, 0x20BF000A, 34.462, 29.6648, 83.505, false),
        (0x720BF015, 6640, 0x20BF000A, 33.7091, 40.6336, 82.005, true),
    ];

    /// A world with the real content and dats, the landblocks `blocks` loaded and ticked once, so
    /// their generators have made their first spawns; and every object on them.
    fn loaded(blocks: &[u32]) -> (World, Vec<ObjectGuid>) {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let mut w = World::new(
            ClockSnapshot::take(&clock, timers.portal_year_ticks),
            dats(),
        );
        w.timers = timers;
        w.content = Arc::new(pack());
        gm::initialize(&mut w, &mut EmptyShard);
        pm::initialize(&mut w, true);
        ThreadSafeRandom::seed(SEED);
        for b in blocks {
            lm::get_landblock(&mut w, LandblockId::new(b << 16 | 0xFFFF), false, false);
        }
        w.now = ClockSnapshot::take(&clock, w.timers.portal_year_ticks);
        let pyt = w.timers.portal_year_ticks;
        lm::tick(&mut w, pyt);
        let all = blocks
            .iter()
            .flat_map(|b| {
                w.landblock_manager
                    .landblocks
                    .expect(LandblockId::new(b << 16 | 0xFFFF))
                    .world_object_guids()
                    .copied()
                    .collect::<Vec<_>>()
            })
            .collect();
        (w, all)
    }

    /// Generator spawns standing on a structure (a tower, a hive, a platform) over ground too steep
    /// to walk on enter the world where they are recorded, the 28 retail created there among them;
    /// they are the only outdoor spot spawns on these landblocks the terrain under them would
    /// refuse. One stands a metre off its spot: a neighbour recorded 0.6 m away is placed first.
    #[test]
    fn spawns_on_structures_over_steep_ground_enter_the_world() {
        let mut blocks: Vec<u32> = STEEP_SPAWNS.iter().map(|r| r.2 >> 16).collect();
        blocks.sort_unstable();
        blocks.dedup();
        let (w, all) = loaded(&blocks);
        let objects: Vec<&empyrean_world::world_objects::world_object::WorldObject> =
            all.iter().filter_map(|g| w.objects.get(*g)).collect();
        let spawned_at = |wcid: u32, cell: u32, x: f32, y: f32, z: f32, reach: f32| {
            objects.iter().any(|o| {
                o.biota.weenie_class_id == wcid
                    && o.location().is_some_and(|l| {
                        l.landblock() == cell >> 16
                            && (l.position_x - x).powi(2) + (l.position_y - y).powi(2)
                                < reach * reach
                            && (l.position_z - z).abs() < 1.0
                    })
            })
        };
        let mut at_the_spot = 0;
        for (child, wcid, cell, x, y, z, retail) in STEEP_SPAWNS {
            assert!(
                spawned_at(wcid, cell, x, y, z, 1.5),
                "{child:08X} (weenie {wcid}, retail created it there: {retail}) is not in the world"
            );
            at_the_spot += usize::from(spawned_at(wcid, cell, x, y, z, 0.05));
        }
        assert_eq!(STEEP_SPAWNS.iter().filter(|r| r.6).count(), 28);
        assert_eq!(at_the_spot, 35);

        // The other spot spawns outdoors stand on terrain the slope check lets in, as before.
        let mut on_steep_terrain = 0;
        for gen in &objects {
            for p in &gen.wo.world_object_generators.generator_profiles {
                if !p
                    .regen_location_type()
                    .contains(RegenLocationType::Specific)
                {
                    continue;
                }
                for g in p.spawned.keys() {
                    let Some(at) = w
                        .objects
                        .get(ObjectGuid::new(*g))
                        .and_then(|o| o.location())
                    else {
                        continue;
                    };
                    if !at.indoors() && !pe::is_walkable(&w, &at) {
                        on_steep_terrain += 1;
                    }
                }
            }
        }
        assert_eq!(on_steep_terrain, 36, "the steep spawns alone");
    }

    /// Holtburgs generators spawn their objects.
    #[test]
    fn holtburgs_generators_spawn_their_objects() {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let mut w = World::new(
            ClockSnapshot::take(&clock, timers.portal_year_ticks),
            dats(),
        );
        w.timers = timers;
        w.content = Arc::new(pack());
        gm::initialize(&mut w, &mut EmptyShard);
        pm::initialize(&mut w, true);
        ThreadSafeRandom::seed(SEED);
        let lb = LandblockId::new(0xA9B4_FFFF);
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
        let (mut certain, mut specific, mut outdoor_specific, mut contain) = (0, 0, 0, 0);
        for g in objects {
            let o = w.objects.get(g).unwrap();
            for p in &o.wo.world_object_generators.generator_profiles {
                let always = p.biota.probability == -1.0 || p.biota.probability >= 1.0;
                if p.biota.weenie_class_id == 3666 || !always {
                    continue;
                }
                certain += 1;
                assert_eq!(
                    p.spawned.len(),
                    1,
                    "{g}: profile {} spawned",
                    p.biota.weenie_class_id
                );
                let s = w
                    .objects
                    .get(ObjectGuid::new(*p.spawned.keys().next().unwrap()))
                    .expect("alive, not a failed first spawn");
                let rlt = p.regen_location_type();
                if rlt.contains(RegenLocationType::Contain) {
                    contain += 1;
                    assert_eq!(s.container_id(), Some(g.full()), "in its chest");
                    continue;
                }
                assert_eq!(
                    s.current_landblock,
                    Some(lb),
                    "{g}: profile {} placed",
                    p.biota.weenie_class_id
                );
                if rlt.contains(RegenLocationType::Specific) {
                    specific += 1;
                    let at = s.location().unwrap();
                    if !at.indoors() {
                        outdoor_specific += 1;
                        assert!(pe::is_walkable(&w, &at));
                    }
                }
            }
        }
        assert_eq!((certain, specific, contain), (33, 23, 8));
        assert_eq!(
            outdoor_specific, 13,
            "outdoor Specific spawns (the other 10 are indoors)"
        );
    }

    /// Shared rules: on retail land (Holtburg, and the steep 0x94D0), `IsWalkable` agrees with the
    /// slope the shared crate's own height probe (`PhysicsWorld::terrain_height_at`) gives, by
    /// finite differences inside one terrain triangle, against `dereth_physics::is_valid_walkable`;
    /// and `GetTerrainZ` equals that probe.
    #[test]
    fn walkability_agrees_with_the_shared_crates_terrain_on_retail_land() {
        let clock = VirtualClock::default();
        let w = World::new(ClockSnapshot::take(&clock, 0.0), dats());
        let (mut walkable, mut steep) = (0, 0);
        for lb in [0xA9B4u32, 0x94D0] {
            for i in 0..48 {
                for j in 0..48 {
                    let (x, y) = (1.7 + 4.0 * i as f32, 2.3 + 4.0 * j as f32);
                    let p = pos(lb << 16 | 1, x, y, 0.0);
                    let probe = |dx: f32, dy: f32| {
                        let q = pos(lb << 16 | 1, x + dx, y + dy, 0.0);
                        w.physics
                            .terrain_height_at(&phys_ext::to_physics_position(&q))
                            .expect("terrain")
                    };
                    let h = probe(0.0, 0.0);
                    assert_eq!(pe::get_terrain_z(&w, &p), h, "GetTerrainZ at {x} {y}");
                    // Stay inside one triangle: away from the cell edges and both diagonals.
                    let (lx, ly) = (x % 24.0, y % 24.0);
                    if lx < 0.5
                        || ly < 0.5
                        || lx > 23.5
                        || ly > 23.5
                        || (lx - ly).abs() < 0.5
                        || (lx + ly - 24.0).abs() < 0.5
                    {
                        continue;
                    }
                    let e = 0.05;
                    let n = dereth_primitives::Vec3::new(
                        -(probe(e, 0.0) - h) / e,
                        -(probe(0.0, e) - h) / e,
                        1.0,
                    );
                    let nz = 1.0 / (n.x * n.x + n.y * n.y + 1.0).sqrt();
                    if (nz - dereth_physics::globals::FLOOR_Z).abs() < 1e-3 {
                        continue;
                    }
                    let expected = dereth_physics::is_valid_walkable(dereth_primitives::Vec3::new(
                        n.x * nz,
                        n.y * nz,
                        nz,
                    ));
                    assert_eq!(
                        pe::is_walkable(&w, &p),
                        expected,
                        "{lb:X} at {x} {y}: n.z {nz}"
                    );
                    if expected {
                        walkable += 1
                    } else {
                        steep += 1
                    }
                }
            }
        }
        assert!(
            walkable > 1000 && steep > 300,
            "both kinds sampled: {walkable} walkable, {steep} steep"
        );
    }
}
