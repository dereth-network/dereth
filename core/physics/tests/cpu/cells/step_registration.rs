//! After a step, a body without a physics mesh is registered in the cells its move found its own
//! collision spheres in, not in the cells its sorting sphere reaches: a body whose sorting sphere
//! is far larger than its collision sphere, standing 1.5 m from a cell line, is in its own cell
//! alone, and a body whose collision sphere reaches past a line its small sorting sphere does not
//! is in both cells. A moving body with a physics mesh is registered again by its parts, in the
//! cells their boxes reach, and a moving particle emitter is in its own cell alone.
//! Fixture: nine flat synthetic landblocks around `0xA9B4` and synthetic bodies.

use std::sync::Arc;

use dereth_physics::geom::bsp::{BspNode, BspNodeKind, BspTree};
use dereth_physics::geom::{BBox, Plane, Polygon, Sphere};
use dereth_physics::source::SetupPart;
use dereth_physics::{landdefs, PhysicsObj, PhysicsWorld, ScriptedMotion, SetupGeometry};
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

use crate::common::physics_fixture::flat_world;

const HOME: LandblockId = LandblockId(0xA9B4);
/// Ground level of the flat world.
const GROUND: f32 = 20.0;
/// The x line between two land cells of the home block.
const LINE: f32 = 96.0;

/// A body with one collision sphere of `radius` resting on the ground and a sorting sphere of
/// `sorting` about the same centre.
fn body(radius: f32, sorting: f32) -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, radius), radius)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, radius), sorting),
        step_up_height: 0.3,
        step_down_height: 0.3,
        radius,
        height: 2.0 * radius,
        ..SetupGeometry::default()
    })
}

fn cell_at(x: f32) -> CellId {
    landdefs::get_outside_cell_id(HOME.cell(1), Vec3::new(x, 100.0, GROUND))
}

/// A one-leaf physics mesh: a two-metre square at the part's origin.
fn mesh() -> Arc<BspTree> {
    let square = Polygon::new(vec![
        Vec3::new(-1.0, -1.0, 0.0),
        Vec3::new(1.0, -1.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(-1.0, 1.0, 0.0),
    ]);
    Arc::new(BspTree {
        nodes: vec![BspNode {
            sphere: Sphere::new(Vec3::ZERO, 1.5),
            splitting_plane: Plane {
                normal: Vec3::new(0.0, 0.0, 1.0),
                d: 1000.0,
            },
            pos_child: None,
            neg_child: None,
            kind: BspNodeKind::Leaf {
                leaf_index: 0,
                solid: true,
            },
            in_polys: vec![0],
        }],
        polygons: vec![square],
    })
}

/// [`body`] with one part that carries a physics mesh and whose box reaches `west` metres to the
/// west of the body's origin and half a metre in every other direction.
fn meshed_body(radius: f32, sorting: f32, west: f32) -> Arc<SetupGeometry> {
    let mut g = (*body(radius, sorting)).clone();
    g.parts = vec![SetupPart {
        physics_bsp: Some(mesh()),
        bound_box: Some(BBox::new(
            Vec3::new(-west, -0.5, 0.0),
            Vec3::new(0.5, 0.5, 1.0),
        )),
        ..SetupPart::default()
    }];
    Arc::new(g)
}

/// Stand a body at `x` (on `y = 100`), let it settle, walk it ten 5 cm steps north (along the
/// line), and answer the cells it is registered in.
fn registered_after_steps(geometry: Arc<SetupGeometry>, x: f32) -> Vec<CellId> {
    registered_after_steps_as(geometry, x, |_| {})
}

/// [`registered_after_steps`] for a body `prepare` has set up first.
fn registered_after_steps_as(
    geometry: Arc<SetupGeometry>,
    x: f32,
    prepare: impl FnOnce(&mut PhysicsObj),
) -> Vec<CellId> {
    let mut w: PhysicsWorld = flat_world();
    let origin = Vec3::new(x, 100.0, GROUND + 0.1);
    let cell = cell_at(x);
    let h = w.create(ObjectId(0x5000_0001), geometry, true);
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        prepare(o);
        o.position = Position::new(cell, Frame::new(origin, Quat::IDENTITY));
        let mut steps = vec![Frame::default(); 30];
        steps.extend(vec![
            Frame::new(Vec3::new(0.0, 0.05, 0.0), Quat::IDENTITY);
            10
        ]);
        o.set_motion(Box::new(ScriptedMotion::new(steps, true)));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    let mut t = 0.0;
    for _ in 0..40 {
        t += 0.034;
        w.use_time(LocalTime(t), false);
    }
    let o = w.get(h).expect("live");
    assert!(
        (o.position.frame.origin.y - 100.5).abs() < 0.01,
        "the body walked half a metre ({:?})",
        o.position.frame.origin
    );
    o.shadow_objects.iter().map(|s| s.cell_id).collect()
}

/// Behaviour: physics.cells.a-moved-body-is-registered-where-its-collision-spheres-are
#[test]
fn a_body_without_a_physics_mesh_is_registered_where_its_step_found_its_collision_spheres() {
    let east = cell_at(LINE + 1.5);
    let west = cell_at(LINE - 1.5);
    assert_ne!(east, west, "the line divides two cells");

    // A half-metre collision sphere with a three-metre sorting sphere, 1.5 m east of the line.
    assert_eq!(
        registered_after_steps(body(0.5, 3.0), LINE + 1.5),
        vec![east],
        "the body's collision sphere stays in its cell, so it is registered there alone"
    );
    // A one-metre collision sphere with a quarter-metre sorting sphere, 0.75 m from the line.
    assert_eq!(
        registered_after_steps(body(1.0, 0.25), LINE + 0.75),
        vec![east, west],
        "the body's collision sphere reaches over the line, so it is registered in both cells"
    );
}

/// Behaviour: physics.cells.a-moved-body-with-a-physics-mesh-is-registered-by-its-parts
#[test]
fn a_moving_body_with_a_physics_mesh_is_registered_where_its_parts_reach_after_a_step() {
    let east = cell_at(LINE + 1.5);
    let west = cell_at(LINE - 1.5);
    let g = meshed_body(0.5, 0.5, 3.0);
    assert!(g.caches_physics_bsp(), "premise: the body carries a mesh");
    // Its collision sphere stays in the east cell; its part's box reaches 1.5 m over the line.
    assert_eq!(
        registered_after_steps(g, LINE + 1.5),
        vec![east, west],
        "the body is registered in the cells its part's box reaches, not where its step found \
         its collision sphere"
    );
}

/// Behaviour: physics.cells.a-moving-particle-emitter-is-registered-in-its-own-cell
#[test]
fn a_moving_particle_emitter_is_registered_in_its_own_cell_alone_after_a_step() {
    let east = cell_at(LINE + 0.75);
    // A one-metre collision sphere 0.75 m from the line: its step finds it in both cells.
    assert_eq!(
        registered_after_steps_as(body(1.0, 0.25), LINE + 0.75, |o| {
            o.state.set_particle_emitter(true);
        }),
        vec![east],
        "a particle emitter is registered in the cell it is in, whatever its spheres reach"
    );
}
