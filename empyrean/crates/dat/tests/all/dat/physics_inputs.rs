//! Vectors: local synthetic dat landblock and setup geometry cases in this module
//! Flat land source, dat land source over a fake block, missing region, setup geometry built once
//! per id, a setup enters a physics world.
//! Fixture: synthetic dat records and locally constructed geometry.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_assets::Sphere as DatSphere;
use dereth_physics::source::LandSource;
use dereth_physics::PhysicsWorld;
use dereth_primitives::{CellId, DataId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};
use empyrean_dat::file_types::SetupModel;
use empyrean_dat::physics::{
    flat_land_source, linear_height_table, DatLandSource, SetupGeometryCache,
};
use empyrean_dat::FakeDats;

const HOLTBURG: LandblockId = LandblockId(0xA9B4);

/// A setup with one 0.5 m sphere at 0.5 m up and no parts: the shape of a small creature.
fn sphere_setup(id: u32) -> SetupModel {
    let s = |z, r| DatSphere {
        center: Vec3::new(0.0, 0.0, z),
        radius: r,
    };
    SetupModel {
        id: DataId(id),
        flags: 0,
        parts: Vec::new(),
        parent_index: None,
        default_scale: None,
        allow_free_heading: false,
        has_physics_bsp: false,
        holding_locations: BTreeMap::new(),
        connection_points: BTreeMap::new(),
        placement_frames: BTreeMap::new(),
        cylspheres: Vec::new(),
        spheres: vec![s(0.5, 0.5)],
        height: 1.0,
        radius: 0.5,
        step_up_height: 0.4,
        step_down_height: 0.4,
        sorting_sphere: s(0.5, 1.0),
        selection_sphere: s(0.5, 1.0),
        lights: BTreeMap::new(),
        default_anim_id: DataId(0),
        default_script_id: DataId(0),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
    }
}

#[test]
fn the_flat_source_stands_every_vertex_at_twice_the_index() {
    let s = flat_land_source(&[HOLTBURG], 10);
    let b = s.landblock(HOLTBURG).expect("the flat block");
    assert!(b.vertices.iter().all(|v| v.z == 20.0));
    assert!(s.landblock(LandblockId(0xA9B5)).is_none());
    assert_eq!(linear_height_table()[10], 20.0);
    assert_eq!(s.height_table(), &linear_height_table());
}

#[test]
fn the_dat_land_source_builds_a_fake_flat_block_like_the_flat_source() {
    let dats = FakeDats::new()
        .with_flat_landblock(HOLTBURG, 10)
        .build()
        .expect("build");
    let land = DatLandSource::with_height_table(Arc::clone(&dats), linear_height_table());
    assert!(
        !land.landblock_resident(HOLTBURG),
        "not resident before load"
    );
    assert!(land.load_landblock(HOLTBURG), "the cell dat has the block");
    assert!(land.landblock_resident(HOLTBURG));
    let from_dat = land.landblock(HOLTBURG).expect("terrain");
    let flat = flat_land_source(&[HOLTBURG], 10)
        .landblock(HOLTBURG)
        .expect("terrain");
    assert_eq!(from_dat.vertices, flat.vertices);
    assert!(Arc::ptr_eq(
        &from_dat,
        &land.landblock(HOLTBURG).expect("cached")
    ));
    assert!(
        land.env_cell(CellId(0xA9B4_0100)).is_none(),
        "no LandblockInfo, so no cells"
    );

    assert!(
        !land.load_landblock(LandblockId(0x0101)),
        "not in the cell dat"
    );
    assert!(land.landblock(LandblockId(0x0101)).is_none());

    land.unload_landblock(HOLTBURG);
    assert!(!land.landblock_resident(HOLTBURG));
    assert_eq!(from_dat.id, HOLTBURG, "a held block survives the unload");
    assert_eq!(land.loaded_landblocks(), vec![LandblockId(0x0101)]);
}

#[test]
fn a_dat_land_source_without_a_region_says_so() {
    let dats = FakeDats::new().build().expect("build");
    assert!(matches!(
        DatLandSource::new(dats),
        Err(empyrean_dat::physics::LandSourceError::NoRegion)
    ));
}

#[test]
fn setup_geometry_is_built_once_per_id_and_rejects_other_ids() {
    let dats = FakeDats::new()
        .with_portal(0x0200_0001, sphere_setup(0x0200_0001))
        .build()
        .expect("build");
    let cache = SetupGeometryCache::new(Arc::clone(&dats));
    let g = cache.get(0x0200_0001).expect("the setup");
    assert_eq!(g.spheres.len(), 1);
    assert_eq!(g.spheres[0].radius, 0.5);
    assert_eq!(
        (g.step_up_height, g.step_down_height, g.radius, g.height),
        (0.4, 0.4, 0.5, 1.0)
    );
    assert!(g.parts.is_empty(), "no placement frames, so no parts");
    assert!(Arc::ptr_eq(&g, &cache.get(0x0200_0001).expect("cached")));
    assert!(cache.get(0x0200_0002).is_none(), "absent from the dat");
    assert!(
        cache.get(0x0300_0001).is_none(),
        "not a setup or a graphics object"
    );
}

/// A synthetic creature setup loads into physics through a fake
/// flat world and lands on the ground.
#[test]
fn a_setup_enters_a_physics_world_over_the_dat_land_source() {
    let dats = FakeDats::new()
        .with_flat_landblock(HOLTBURG, 10)
        .with_portal(0x0200_0001, sphere_setup(0x0200_0001))
        .build()
        .expect("build");
    let land = Arc::new(DatLandSource::with_height_table(
        Arc::clone(&dats),
        linear_height_table(),
    ));
    assert!(land.load_landblock(HOLTBURG));
    let geometry = SetupGeometryCache::new(Arc::clone(&dats))
        .get(0x0200_0001)
        .expect("setup");
    let mut world = PhysicsWorld::new(land);
    let h = world.create(ObjectId(0x5000_0001), geometry, true);
    let pos = Position::new(
        CellId(0xA9B4_0019),
        Frame::new(Vec3::new(84.0, 7.1, 20.0), Quat::IDENTITY),
    );
    assert!(
        world.enter_world(h, &pos),
        "placement on flat ground succeeds"
    );
    let obj = world.get(h).expect("the object");
    assert_eq!(obj.position.cell, CellId(0xA9B4_0019));
    assert!(
        (obj.position.frame.origin.z - 20.0).abs() < 0.05,
        "{:?}",
        obj.position
    );
}
