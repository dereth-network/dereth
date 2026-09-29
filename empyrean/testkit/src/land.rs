//! Physics land for world tests: flat synthetic landblocks plus one registered synthetic setup.
//! An object joins a landblock only if its body enters the physics world, so a test
//! that places anything needs land under it and collision geometry for its setup. Nothing here is
//! ported from ACE.

use std::sync::Arc;

use dereth_physics::geom::Sphere;
use dereth_physics::SetupGeometry;
use dereth_primitives::{LandblockId, Vec3};
use empyrean_dat::physics::flat_land_source;
use empyrean_world::physics::phys_ext;
use empyrean_world::World;

/// The setup id [`use_flat_land_with_test_setup`] registers; give a test object this
/// `PropertyDataId::Setup`.
pub const TEST_SETUP: u32 = 0x0200_1000;

/// The synthetic body: a 0.5 m sphere standing on its base, 1 m tall, with a 1 m sorting sphere.
#[must_use]
pub fn test_setup_geometry() -> SetupGeometry {
    SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0),
        radius: 0.5,
        height: 1.0,
        ..SetupGeometry::default()
    }
}

/// Replaces `w`'s physics land with flat blocks at height-table index `height_index` (the linear
/// table: `2 * height_index` metres). Every body is lost (`phys_ext::use_land_source`).
pub fn use_flat_land(w: &mut World, landblocks: &[u16], height_index: u8) {
    let ids: Vec<LandblockId> = landblocks.iter().map(|&b| LandblockId(b)).collect();
    phys_ext::use_land_source(w, Arc::new(flat_land_source(&ids, height_index)));
}

/// [`use_flat_land`], then registers `geometry` under `setup_id`.
pub fn use_flat_land_with_setup(
    w: &mut World,
    landblocks: &[u16],
    height_index: u8,
    setup_id: u32,
    geometry: SetupGeometry,
) {
    use_flat_land(w, landblocks, height_index);
    phys_ext::register_setup(w, setup_id, geometry);
}

/// [`use_flat_land`], then registers [`test_setup_geometry`] under [`TEST_SETUP`].
pub fn use_flat_land_with_test_setup(w: &mut World, landblocks: &[u16], height_index: u8) {
    use_flat_land_with_setup(
        w,
        landblocks,
        height_index,
        TEST_SETUP,
        test_setup_geometry(),
    );
}
