//! A scaled object is registered in the cells its setup-sized spheres reach: the sorting sphere a
//! moving object without cylinder spheres is placed by, and the cylinder spheres any object
//! carrying them is placed by, moving or static, are searched at the size the setup gives them,
//! whatever the object's scale. A body three times its setup's size near a cell line is not
//! registered across it, and a body half its setup's size is registered across a line only its
//! setup-sized sphere reaches.
//! Fixture: nine flat synthetic landblocks around `0xA9B4` and synthetic bodies whose spheres are
//! one metre across at scale 1.

use std::sync::Arc;

use dereth_physics::geom::Sphere;
use dereth_physics::{landdefs, CylSphere, PhysHandle, PhysicsWorld, SetupGeometry};
use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};

use crate::common::physics_fixture::{flat_world, sphere_body};

const HOME: LandblockId = LandblockId(0xA9B4);
/// Ground level of the flat world.
const GROUND: f32 = 20.0;
/// The x line between two land cells of the home block.
const LINE: f32 = 96.0;

/// A body whose only cell-search sphere is a one-metre cylinder sphere.
fn cylinder_body() -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
        cyl_spheres: vec![CylSphere {
            low_pt: Vec3::ZERO,
            radius: 1.0,
            height: 2.0,
        }],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 1.0), 4.0),
        ..SetupGeometry::default()
    })
}

/// Make a body at `x` (on `y = 100`, the middle of a cell row), give it `scale`, and register it
/// by the moving rule, or by the static rule when `dynamic` is false.
fn registered(geometry: Arc<SetupGeometry>, dynamic: bool, x: f32, scale: f32) -> Vec<CellId> {
    let mut w: PhysicsWorld = flat_world();
    let origin = Vec3::new(x, 100.0, GROUND);
    let mut cell = HOME.cell(1);
    let mut o = origin;
    assert!(landdefs::adjust_to_outside(&mut cell, &mut o));
    let h: PhysHandle = w.create(ObjectId(0x5000_0001), geometry, dynamic);
    w.enter_cell(h, cell);
    {
        let body = w.get_mut(h).expect("live");
        body.position = Position::new(cell, Frame::new(origin, Quat::IDENTITY));
        body.scale = scale;
    }
    if dynamic {
        w.calc_cross_cells(h, false);
    } else {
        w.calc_cross_cells_static(h);
    }
    w.get(h)
        .expect("live")
        .shadow_objects
        .iter()
        .map(|s| s.cell_id)
        .collect()
}

fn cell_at(x: f32) -> CellId {
    landdefs::get_outside_cell_id(HOME.cell(1), Vec3::new(x, 100.0, GROUND))
}

/// Behaviour: physics.cells.a-scaled-object-is-registered-by-its-setup-sized-spheres
#[test]
fn a_scaled_body_is_registered_in_the_cells_its_setup_sized_spheres_reach() {
    let east = cell_at(LINE + 1.5);
    let west = cell_at(LINE - 1.5);
    assert_ne!(east, west, "the line divides two cells");

    // A one-metre sorting sphere 1.5 m east of the line: three times the size it would reach
    // 1.5 m past the line, but it is searched at one metre.
    assert_eq!(
        registered(sphere_body(0.5, 0.3, 1.0), true, LINE + 1.5, 3.0),
        vec![east],
        "a body at three times its setup's size is registered by its one-metre sorting sphere"
    );
    // Half its setup's size 0.75 m from the line, the sphere it is searched at still reaches.
    assert_eq!(
        registered(sphere_body(0.5, 0.3, 1.0), true, LINE + 0.75, 0.5),
        vec![east, west],
        "a body at half its setup's size is registered across the line its setup-sized sphere \
         reaches"
    );
    // The same for cylinder spheres, by the moving rule and by the static rule.
    for dynamic in [true, false] {
        assert_eq!(
            registered(cylinder_body(), dynamic, LINE + 1.5, 3.0),
            vec![east],
            "a cylinder at three times its setup's size is registered by its one-metre radius \
             (dynamic: {dynamic})"
        );
        assert_eq!(
            registered(cylinder_body(), dynamic, LINE + 0.75, 0.5),
            vec![east, west],
            "a cylinder at half its setup's size reaches the line by its one-metre radius \
             (dynamic: {dynamic})"
        );
    }
}
