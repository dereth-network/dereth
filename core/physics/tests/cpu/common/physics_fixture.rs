//! Flat land and single-sphere collision fixtures.

use dereth_physics::geom::Sphere;
use dereth_physics::{LandSource, PhysicsWorld, SetupGeometry, StaticLandSource};
use dereth_primitives::{LandblockId, Vec3};
use std::sync::Arc;

pub fn flat_land() -> Arc<dyn LandSource> {
    let mut land = StaticLandSource::linear();
    for dx in -1_i32..=1 {
        for dy in -1_i32..=1 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let id = LandblockId::new((0xA9 + dx) as u8, (0xB4 + dy) as u8);
            land.add_flat_block(id, 10);
        }
    }
    Arc::new(land)
}

pub fn flat_world() -> PhysicsWorld {
    PhysicsWorld::new(flat_land())
}

pub fn one_block_world() -> PhysicsWorld {
    let mut land = StaticLandSource::linear();
    land.add_flat_block(LandblockId::new(0xA9, 0xB4), 10);
    PhysicsWorld::new(Arc::new(land))
}

pub fn sphere_body(radius: f32, step: f32, height: f32) -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, radius), radius)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, radius), 1.0),
        step_up_height: step,
        step_down_height: step,
        radius,
        height,
        ..SetupGeometry::default()
    })
}

pub fn player_geometry() -> Arc<SetupGeometry> {
    sphere_body(0.5, 0.3, 1.0)
}
