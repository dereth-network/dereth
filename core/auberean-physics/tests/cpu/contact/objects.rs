//! Objects of the world against bodies, the cells an object reaches, the box and polygon geometry
//! that finds them, and putting a body somewhere among them; and an object's parts put together
//! from its look, with what is asked of them.
//! Fixture: a flat 160 m block at height 10; a body of one 0.48 m sphere with the player's ground
//! and air motion values; an upright cylinder 1 m across and 2 m high standing in the block. For
//! the parts: a made-up body setup of two meshes on two bones (the second turned a quarter) with a
//! connection point on its second bone, a made-up head setup of one mesh on one bone, made-up
//! meshes (one with a physics tree), and made-up physics trees (a floor solid below `z = 0` within
//! 10 m of the origin, a solid ball around the origin).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use auberean_assets::animation::{AnimMap, MotionParams};
use auberean_assets::mesh::{DetailLevel, Mesh};
use auberean_assets::setup::{Bone, BoneTransform, Cylinder, Location, Pose, Setup};
use auberean_assets::visual::{Appearance, VisualDesc};
use auberean_physics::body::placing::error;
use auberean_physics::body::profile::ProfileQualities;
use auberean_physics::body::{Body, BodyConfig};
use auberean_physics::cells::{find_cells, Sidedness, WorkPolygon};
use auberean_physics::geom::{BBox, BoxOps, Ray};
use auberean_physics::land::{land_cell_id, Block, Land};
use auberean_physics::motion::MotionInput;
use auberean_physics::object::Shape;
use auberean_physics::parts::{
    trace_ray, PartArray, PartGroup, PartMesh, PartRecords, PhysicsPart,
};
use auberean_physics::place::{flags, SetPosition};
use auberean_physics::world::{World, WorldObject};
use auberean_primitives::DataId;
use auberean_world::look::{Atoms, Group, Look, LookRecords, MaterialAtom, MeshAtom, OpacityAtom};
use auberean_world::math::Mat4;
use auberean_world::place::{LoadedSetup, PlaceRecords};
use dereth_physics::arena::Arena;
use dereth_physics::geom::{BspNode, BspNodeKind, BspTree, Polygon, Sphere};
use dereth_physics::transition::{Transition, TransitionCtx, TransitionState};
use dereth_physics::PhysicsObj;
use dereth_primitives::shape::{CylSphere, Plane};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

pub(crate) const GROUND_Z: f32 = 10.0;

pub(crate) fn ground() -> MotionParams {
    MotionParams {
        terminal_speed: 50.0,
        gravity: 1,
        max_forward: 6.5,
        max_backward: 4.0,
        max_strafe: 4.5,
        still_turn_rate: 90.0,
        running_turn_rate: 120.0,
        forward_slowdown: 18.0,
        backward_slowdown: 25.0,
        strafe_slowdown: 25.0,
        turn_slowdown: 1850.0,
        forward_acceleration: 15.0,
        backward_acceleration: 15.0,
        strafe_acceleration: 15.0,
        turn_acceleration: 1000.0,
        ..MotionParams::default()
    }
}

pub(crate) fn land() -> Land {
    let land = Land::new();
    land.insert(Block::flat(0x40, 0x40, GROUND_Z));
    land
}

pub(crate) fn at(x: f32, y: f32) -> Position {
    let (cx, cy) = auberean_physics::land::cell_of(Vec3::new(x, y, 0.0)).expect("in the block");
    let cell = land_cell_id(0x40, 0x40, cx, cy);
    Position::new(cell, Frame::new(Vec3::new(x, y, GROUND_Z), Quat::IDENTITY))
}

/// A body at `(x, y)` facing north.
pub(crate) fn body(x: f32, y: f32) -> Body {
    let config = BodyConfig {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.48), 0.48)],
        step_up: 0.6,
        step_down: 0.6,
        scale: 1.0,
        ground: ground(),
        air: ground(),
    };
    Body::new(config, at(x, y), 0.0)
}

/// An upright cylinder 1 m across and 2 m high, at unit scale, drawn as wide and as high.
pub(crate) fn post() -> Arc<Shape> {
    Arc::new(Shape {
        trees: Vec::new(),
        cylinders: vec![CylSphere {
            low_pt: Vec3::ZERO,
            radius: 0.5,
            height: 2.0,
        }],
        spheres: Vec::new(),
        physics_box: BBox::default(),
        scale: Vec3::new(1.0, 1.0, 1.0),
        containment_radius: 0.5,
        render_box: BBox::new(Vec3::new(-0.5, -0.5, 0.0), Vec3::new(0.5, 0.5, 2.0)),
        properties: Vec::new(),
    })
}

/// Run `b` north for `ticks` tenths of a second in `world`; the most northern point it reached.
pub(crate) fn run_north(b: &mut Body, land: &Land, world: &mut World, ticks: u32) -> f32 {
    b.steer(MotionInput {
        forward: 1.0,
        ..MotionInput::default()
    });
    let mut farthest = f32::MIN;
    for _ in 0..ticks {
        b.tick_in(land, world, 0.1);
        farthest = farthest.max(b.position.frame.origin.y);
    }
    farthest
}

/// A solid standing near a block's edge and reaching over it is met from the land cells it reaches
/// in the block next to it, and only those, once that block is in; a solid well inside its block
/// reaches no other.
///
/// Behaviour: collision.objects.a-solid-over-a-block-edge-is-in-the-next-blocks-cells
#[test]
fn a_solid_over_a_block_edge_is_in_the_next_blocks_cells() {
    use auberean_physics::land::Solid;
    let solid = |x: f32, r: f32| Solid {
        part: dereth_physics::source::PhysicsPart::default(),
        scale: Vec3::new(1.0, 1.0, 1.0),
        bounds: Sphere::new(Vec3::new(x, 90.0, GROUND_Z), r),
        render_box: BBox::default(),
        properties: Vec::new(),
        marked: false,
    };
    let land = Land::new();
    let mut west = Block::flat(0x40, 0x40, GROUND_Z);
    west.add_solid(solid(159.0, 3.0));
    west.add_solid(solid(80.0, 3.0));
    land.insert(west);
    land.insert(Block::flat(0x41, 0x40, GROUND_Z));
    let (near, far) = ((0, 4), (1, 4));
    let over = |cx, cy| land.overhanging(0x41, 0x40, cx, cy);
    assert_eq!(over(near.0, near.1).len(), 1, "the cell next to the edge");
    assert_eq!(over(near.0, near.1)[0].1, 0, "the solid by the edge");
    assert!(over(far.0, far.1).is_empty(), "beyond its reach");
    assert!(over(near.0, 0).is_empty(), "north and south of its reach");
    assert!(
        land.overhanging(0x40, 0x40, 7, 4).is_empty(),
        "its own block's cells hold it themselves"
    );
    assert!(
        land.block(0x40, 0x40).expect("in").cell_solids[7 * 8 + 4].contains(&0),
        "and it is in them"
    );
}

/// A moving object reaching over a block's edge stands in the lists of the next block's cells
/// whether or not that block is in, so nothing is to be done for it when the block comes in.
///
/// Behaviour: collision.objects.a-body-is-in-the-lists-of-cells-not-yet-in
#[test]
fn a_body_is_in_the_lists_of_cells_not_yet_in() {
    let land = land();
    let mut world = World::new();
    let mut near_edge = body(159.8, 70.0);
    world.insert_body(&land, 0x50, &mut near_edge);
    let at = land_cell_id(0x41, 0x40, 0, 3);
    assert_eq!(world.collision_list(at), vec![0x50], "before it comes in");
    land.insert(Block::flat(0x41, 0x40, GROUND_Z));
    assert_eq!(world.collision_list(at), vec![0x50], "and after");
}

/// A body running at an object the host put in the world stops against it, its sphere outside
/// the object's cylinder; with the object taken out, a body making the same run goes on past
/// where it stood.
///
/// Behaviour: collision.objects.a-body-stops-at-an-object-put-in-the-world
#[test]
fn a_body_stops_at_an_object_put_in_the_world() {
    let land = land();
    let mut world = World::new();
    let key = world.insert(
        &land,
        0x77,
        &WorldObject {
            shape: post(),
            position: at(70.0, 75.0),
            is_static: true,
            passable: false,
        },
    );
    assert_eq!(world.find(0x77), Some(key));
    let mut b = body(70.0, 70.0);
    let farthest = run_north(&mut b, &land, &mut world, 30);
    assert!(
        farthest <= 75.0 - 0.5 - 0.48 + 0.01,
        "stopped short of the post: {farthest}"
    );
    assert!(farthest > 73.5, "it ran up to the post: {farthest}");
    assert!(world.remove(key));
    assert_eq!(world.find(0x77), None);
    // The first runner is still in the world, as solid as any body: out of the way first.
    assert!(world.remove(b.key().expect("in the world")));
    let mut again = body(70.0, 70.0);
    let past = run_north(&mut again, &land, &mut world, 20);
    assert!(past > 76.0, "on past the post once it is gone: {past}");
}

/// A passable object does not stop a body, and a body made solid stops another: bodies put in
/// one world meet each other there, each moving its own entry as it moves.
///
/// Behaviour: collision.objects.bodies-in-one-world-meet-each-other
#[test]
fn bodies_in_one_world_meet_each_other() {
    let land = land();
    let mut world = World::new();
    world.insert(
        &land,
        0,
        &WorldObject {
            shape: post(),
            position: at(70.0, 74.0),
            is_static: true,
            passable: true,
        },
    );
    let mut standing = body(70.0, 80.0);
    let key = world.insert_body(&land, 0x50, &mut standing);
    assert_eq!(standing.key(), Some(key));
    world.set_passable(key, false);
    let mut runner = body(70.0, 70.0);
    let farthest = run_north(&mut runner, &land, &mut world, 40);
    assert!(farthest > 75.0, "through the passable post: {farthest}");
    assert!(
        farthest <= 80.0 - 0.96 + 0.01,
        "stopped by the solid body: {farthest}"
    );
    // The standing body moves, and its entry with it: a runner setting off now gets farther.
    standing.steer(MotionInput {
        forward: 1.0,
        ..MotionInput::default()
    });
    for _ in 0..20 {
        standing.tick_in(&land, &mut world, 0.1);
    }
    let moved = world
        .position(key)
        .expect("still in the world")
        .frame
        .origin;
    assert!(
        (moved.y - standing.position.frame.origin.y).abs() < 1e-4 && moved.y > 85.0,
        "the entry is where the body is: {moved:?}"
    );
    assert!(world.remove(runner.key().expect("in the world")));
    let mut second = body(70.0, 70.0);
    let further = run_north(&mut second, &land, &mut world, 40);
    assert!(further > farthest + 2.0, "the way is open: {further}");
}

/// An object outside stands in every land cell the box around it reaches: a post at the corner
/// of four land cells is in all four, one in the middle of a cell only in that one.
///
/// Behaviour: collision.objects.an-object-outside-stands-in-every-land-cell-under-it
#[test]
fn an_object_outside_stands_in_every_land_cell_under_it() {
    let land = land();
    let shape = post();
    let corner = shape.cross_cells(&land, &at(60.2, 39.8));
    let want: Vec<CellId> = vec![
        land_cell_id(0x40, 0x40, 2, 1),
        land_cell_id(0x40, 0x40, 2, 2),
        land_cell_id(0x40, 0x40, 3, 1),
        land_cell_id(0x40, 0x40, 3, 2),
    ];
    let mut got = corner.clone();
    got.sort_by_key(|c| c.0);
    assert_eq!(got, want);
    assert_eq!(
        shape.cross_cells(&land, &at(70.0, 70.0)),
        vec![land_cell_id(0x40, 0x40, 3, 3)]
    );
    // Across the block's east edge, the cells of the block beside it.
    let edge = find_cells(
        &land,
        land_cell_id(0x40, 0x40, 7, 3),
        BBox::new(Vec3::new(159.5, 70.0, 0.0), Vec3::new(160.5, 70.5, 1.0)),
    );
    assert!(
        edge.contains(&land_cell_id(0x41, 0x40, 0, 3)),
        "{edge:08X?}"
    );
}

/// A box carried into a turned frame is the box over its corners there; a box grows to take in a
/// point; a sphere and an upright cylinder reach a box when their nearest points are within their
/// radius.
///
/// Behaviour: collision.objects.boxes-turn-grow-and-meet-spheres-and-cylinders
#[test]
fn boxes_turn_grow_and_meet_spheres_and_cylinders() {
    let local = BBox::new(Vec3::new(-1.0, -2.0, 0.0), Vec3::new(1.0, 2.0, 3.0));
    // A quarter turn about z: x goes to y.
    let h = std::f32::consts::FRAC_1_SQRT_2;
    let p = Position::new(
        CellId(0x4040_0001),
        Frame::new(Vec3::new(10.0, 20.0, 5.0), Quat::new(h, 0.0, 0.0, h)),
    );
    let g = local.local_to_global(&p, &p);
    let near = |a: Vec3, b: Vec3| (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4;
    assert!(near(g.min, Vec3::new(8.0, 19.0, 5.0)), "{g:?}");
    assert!(near(g.max, Vec3::new(12.0, 21.0, 8.0)), "{g:?}");
    let mut b = BBox::for_adjustment();
    b.adjust_bbox(Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(
        (b.min, b.max),
        (Vec3::new(1.0, 2.0, 3.0), Vec3::new(1.0, 2.0, 3.0))
    );
    let unit = BBox::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
    assert!(unit.intersects_sphere(&Sphere::new(Vec3::new(1.5, 0.5, 0.5), 0.5)));
    assert!(!unit.intersects_sphere(&Sphere::new(Vec3::new(1.6, 1.6, 0.5), 0.5)));
    let cyl = |x: f32, z: f32| CylSphere {
        low_pt: Vec3::new(x, 0.5, z),
        radius: 0.5,
        height: 1.0,
    };
    assert!(unit.intersects_cylinder(&cyl(1.4, 0.5)));
    assert!(!unit.intersects_cylinder(&cyl(1.6, 0.5)));
    assert!(!unit.intersects_cylinder(&cyl(0.5, 2.1)), "above the box");
}

/// A polygon clipped to a plane's positive side keeps its corners there and gains the crossings
/// of the edges that cross it; one wholly behind the plane is negative; one lying in it is in the
/// plane only when corners in the plane are kept.
///
/// Behaviour: collision.objects.a-polygon-clips-to-a-plane
#[test]
fn a_polygon_clips_to_a_plane() {
    let square = || WorkPolygon {
        corners: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(2.0, 2.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ],
    };
    // x >= 1.
    let plane = Plane::new(Vec3::new(1.0, 0.0, 0.0), -1.0);
    let mut s = square();
    assert_eq!(s.clip(&plane, true, 0.01), Sidedness::Crossing);
    assert_eq!(
        s.corners,
        vec![
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(2.0, 2.0, 0.0),
            Vec3::new(1.0, 2.0, 0.0),
        ]
    );
    let behind = Plane::new(Vec3::new(1.0, 0.0, 0.0), -3.0);
    assert_eq!(square().clip(&behind, true, 0.01), Sidedness::Negative);
    let flat = Plane::new(Vec3::new(0.0, 0.0, 1.0), 0.0);
    assert_eq!(square().clip(&flat, true, 0.01), Sidedness::InPlane);
    assert_eq!(square().clip(&flat, false, 0.01), Sidedness::Negative);
    let ahead = Plane::new(Vec3::new(1.0, 0.0, 0.0), 1.0);
    assert_eq!(square().clip(&ahead, true, 0.01), Sidedness::Positive);
}

/// An order to put a body at `p` with flags `f`.
fn order(p: Position, f: u32) -> SetPosition {
    SetPosition {
        position: p,
        flags: f,
        ..SetPosition::default()
    }
}

/// A body put on open ground is put there, and the place is kept as its placement when the order
/// asks; a place naming no cell has no cell.
///
/// Behaviour: motion.placing.a-body-put-on-open-ground-lands-there
#[test]
fn a_body_put_on_open_ground_lands_there() {
    let land = land();
    let mut world = World::new();
    let mut b = body(10.0, 10.0);
    let r = b.set_position(
        &land,
        &mut world,
        &order(at(40.0, 50.0), flags::PLACEMENT),
        &mut || 0.0,
    );
    assert_eq!(r, error::OK);
    let o = b.position.frame.origin;
    assert!(
        (o.x - 40.0).abs() < 0.05 && (o.y - 50.0).abs() < 0.05,
        "{o:?}"
    );
    assert_eq!(b.placement, Some(at(40.0, 50.0)));
    let nowhere = Position::new(CellId(0x4040_0080), at(1.0, 1.0).frame);
    assert_eq!(
        b.set_position(&land, &mut world, &order(nowhere, 0), &mut || 0.0),
        error::NO_CELL
    );
}

/// A place another object takes is refused (the object hit is kept as touched); forced, the body
/// goes there anyway; told to scatter when blocked, it takes the first place around it that is
/// free, each roll moving it along x by the first radius and along y by the second.
///
/// Behaviour: motion.placing.a-taken-place-is-refused-forced-or-scattered
#[test]
fn a_taken_place_is_refused_forced_or_scattered() {
    let land = land();
    let mut world = World::new();
    world.insert(
        &land,
        0x77,
        &WorldObject {
            shape: post(),
            position: at(40.0, 50.0),
            is_static: true,
            passable: false,
        },
    );
    let mut b = body(10.0, 10.0);
    let r = b.set_position(&land, &mut world, &order(at(40.0, 50.0), 0), &mut || 0.0);
    assert!(
        r == error::COLLIDED || r == error::NO_VALID_POSITION,
        "refused: {r}"
    );
    let r = b.set_position(
        &land,
        &mut world,
        &order(at(40.0, 50.0), flags::FORCE),
        &mut || 0.0,
    );
    assert_eq!(r, error::OK);
    assert_eq!(b.position.frame.origin, at(40.0, 50.0).frame.origin);

    let mut c = body(10.0, 10.0);
    let mut s = order(at(40.0, 50.0), flags::SCATTER_IF_BLOCKED);
    s.radius_min = 3.0;
    s.radius_max = 0.0;
    s.tries = 3;
    let mut rolls = [0.1_f32, 0.0, 1.0, 0.0].into_iter();
    let r = c.set_position(&land, &mut world, &s, &mut || rolls.next().unwrap_or(0.0));
    assert_eq!(r, error::OK);
    let o = c.position.frame.origin;
    assert!((o.x - 43.0).abs() < 0.05, "the second roll's place: {o:?}");
}

/// A body is placed by its spheres at its whole scale, its look's times its own: its 0.48 m sphere
/// at its own scale of 1.5 is refused 1.15 m from the middle of the 1 m post and put 1.3 m from
/// it, where at its own scale of 1 it is put 1.15 m from it.
///
/// Behaviour: motion.placing.a-body-is-placed-at-its-whole-scale
#[test]
fn a_body_is_placed_at_its_whole_scale() {
    let land = land();
    let put = |own: f32, x: f32| {
        let mut world = World::new();
        world.insert(
            &land,
            0x77,
            &WorldObject {
                shape: post(),
                position: at(40.0, 50.0),
                is_static: true,
                passable: false,
            },
        );
        let mut b = body(10.0, 10.0);
        b.set_scale(own);
        b.set_position(
            &land,
            &mut world,
            &order(at(40.0 + x, 50.0), flags::PLACEMENT),
            &mut || 0.0,
        )
    };
    assert_ne!(put(1.5, 1.15), error::OK, "grown, too near");
    assert_eq!(put(1.5, 1.3), error::OK, "grown, clear");
    assert_eq!(put(1.0, 1.15), error::OK, "its own size, clear");
}

/// A missile held in its holder's hand and then put in the world lets go of its holder first: the
/// holder, told, takes it off its list of what it holds; it is in the world, holding nothing.
///
/// Behaviour: motion.placing.a-missile-put-in-the-world-is-let-go-of
#[test]
fn a_missile_put_in_the_world_is_let_go_of() {
    use auberean_physics::body::missile::NoObjects;
    use auberean_physics::body::parent::{set_parent, TakeUp, OWN_FRAME};
    let land = land();
    let mut world = World::new();
    let mut holder = body(30.0, 30.0);
    holder.id = 0x5000_0001;
    world.insert_body(&land, holder.id, &mut holder);
    let mut arrow = body(30.0, 30.0);
    arrow.id = 0x1300_0000_0000_0000;
    arrow.in_world = false;
    let how = TakeUp {
        location: OWN_FRAME,
        holds: true,
        parent_mode: 0,
        child_is_temporary: true,
    };
    assert!(set_parent(
        &mut arrow,
        &mut holder,
        how,
        &land,
        &mut world,
        &mut NoObjects
    ));
    assert!(holder.look_up_in_children(arrow.id) && arrow.has_parent);
    assert!(arrow.missile_init_end(&land, &mut world, &at(32.0, 30.0)));
    for n in arrow.take_requests() {
        holder.deliver(&n);
    }
    assert!(!holder.look_up_in_children(arrow.id), "still on the list");
    assert!(arrow.in_world && !arrow.has_parent && arrow.parent == 0);
}

/// A body of no collision spheres (a missile made from a look whose setup has none) is placed and
/// moved as one sphere of radius 0.1 resting on its origin: put on the ground it stands there; it
/// is refused 0.55 m from the middle of the 1 m post and put 0.65 m from it; dropped, it comes to
/// rest with its origin on the ground.
///
/// Behaviour: motion.placing.a-body-of-no-spheres-is-placed-as-a-small-sphere
#[test]
fn a_body_of_no_spheres_is_placed_as_a_small_sphere() {
    let land = land();
    let mut world = World::new();
    world.insert(
        &land,
        0x77,
        &WorldObject {
            shape: post(),
            position: at(40.0, 50.0),
            is_static: true,
            passable: false,
        },
    );
    let bare = || {
        let mut b = body(10.0, 10.0);
        b.config.spheres.clear();
        b
    };
    let mut b = bare();
    let r = b.set_position(
        &land,
        &mut world,
        &order(at(20.0, 20.0), flags::PLACEMENT),
        &mut || 0.0,
    );
    assert_eq!(r, error::OK, "on open ground");
    assert!((b.position.frame.origin.z - GROUND_Z).abs() < 0.01);
    let mut near = bare();
    let r = near.set_position(
        &land,
        &mut world,
        &order(at(40.55, 50.0), flags::PLACEMENT),
        &mut || 0.0,
    );
    assert_ne!(r, error::OK, "within its radius of the post");
    let mut clear = bare();
    let r = clear.set_position(
        &land,
        &mut world,
        &order(at(40.65, 50.0), flags::PLACEMENT),
        &mut || 0.0,
    );
    assert_eq!(r, error::OK, "beyond its radius of the post");
    let mut dropped = bare();
    let mut high = at(30.0, 30.0);
    high.frame.origin.z += 2.0;
    dropped.place(high, 0.0);
    dropped.contact = false;
    dropped.on_walkable = false;
    for _ in 0..40 {
        dropped.tick(&land, 0.1);
    }
    let z = dropped.position.frame.origin.z;
    assert!((z - GROUND_Z).abs() < 0.05, "at rest on the ground: {z}");
}

// An object's parts.

const BODY: u32 = 0x0400_0001;
const HEAD: u32 = 0x0400_0002;
/// A setup whose second of three meshes is not in the data.
const HOLEY: u32 = 0x0400_0003;
/// A setup of no bones: one drawn mesh and one that draws nothing.
const BONELESS: u32 = 0x0400_0004;
/// The body's setup with its one mesh weighted to its two bones.
const SKINNED: u32 = 0x0400_0005;
const TORSO: u32 = 0x0600_0001;
const ARM: u32 = 0x0600_0002;
const FACE: u32 = 0x0600_0003;
const HELM: u32 = 0x0600_0004;
const UNDRAWN: u32 = 0x0600_0005;
const SKIN: u32 = 0x0600_0006;
const MESH_GONE: u32 = 0x0600_00FF;
const MAP: u32 = 0x2400_0001;
const OTHER_MAP: u32 = 0x2400_0002;
const MODIFIER: u32 = 0x3100_0001;
/// The body's connection point the head hangs from, on its second bone.
const NECK: u32 = 7;
/// The meshes' kinds.
const TORSO_KIND: u32 = 2;
const HEAD_KIND: u32 = 4;

fn wv(x: f32, y: f32, z: f32) -> auberean_primitives::Vec3 {
    auberean_primitives::Vec3::new(x, y, z)
}

/// A bone moved by `(x, y, z)` from its parent, unscaled and unturned.
fn moved(x: f32, y: f32, z: f32) -> BoneTransform {
    BoneTransform {
        offset: wv(x, y, z),
        scale: wv(1.0, 1.0, 1.0),
        rotation: auberean_primitives::Quat::IDENTITY,
    }
}

/// A bone moved by `(x, y, z)` from its parent and turned a quarter left (its `x` axis to its
/// parent's `y`).
fn moved_turned(x: f32, y: f32, z: f32) -> BoneTransform {
    let h = std::f32::consts::FRAC_1_SQRT_2;
    BoneTransform {
        rotation: auberean_primitives::Quat {
            w: h,
            x: 0.0,
            y: 0.0,
            z: h,
        },
        ..moved(x, y, z)
    }
}

/// A mesh of `kind` within `half` of its axis, from its base to `2 half` up.
fn made_up_mesh(kind: u32, half: f32, drawn: bool, weighted: bool) -> Arc<Mesh> {
    let mut m = Mesh {
        kind,
        levels: if drawn {
            vec![DetailLevel::default()]
        } else {
            Vec::new()
        },
        ..Mesh::default()
    };
    m.vertices.bounds = auberean_primitives::BBox {
        min: wv(-half, -half, 0.0),
        max: wv(half, half, 2.0 * half),
    };
    if weighted {
        m.vertices.format = 0x0001_0000;
    }
    Arc::new(m)
}

fn leaf(solid: bool, in_polys: Vec<u32>, radius: f32) -> BspNode {
    BspNode {
        sphere: Sphere::new(Vec3::ZERO, radius),
        splitting_plane: Plane::default(),
        pos_child: None,
        neg_child: None,
        kind: BspNodeKind::Leaf {
            leaf_index: 0,
            solid,
        },
        in_polys,
    }
}

/// A tree split by the plane `z = 0`: open above, solid below, within 10 m of the origin.
fn floor_tree() -> Arc<BspTree> {
    Arc::new(BspTree {
        nodes: vec![
            BspNode {
                sphere: Sphere::new(Vec3::ZERO, 10.0),
                splitting_plane: Plane {
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    d: 0.0,
                },
                pos_child: Some(1),
                neg_child: Some(2),
                kind: BspNodeKind::Node,
                in_polys: Vec::new(),
            },
            leaf(false, Vec::new(), 10.0),
            leaf(true, Vec::new(), 10.0),
        ],
        polygons: Vec::new(),
    })
}

/// A solid ball's tree: one solid leaf of one polygon, within a meter of the origin.
fn ball_tree() -> Arc<BspTree> {
    Arc::new(BspTree {
        nodes: vec![leaf(true, vec![0], 1.0)],
        polygons: vec![Polygon::new(vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ])],
    })
}

/// Made-up records: setups, meshes (each with its physics tree when it has one), animation maps
/// and material modifiers.
#[derive(Default)]
struct Kit {
    setups: HashMap<u32, Arc<LoadedSetup>>,
    meshes: HashMap<u32, (Arc<Mesh>, Option<Arc<BspTree>>)>,
    maps: HashSet<u32>,
    materials: HashSet<u32>,
}

impl LookRecords for Kit {
    fn visual(&self, _id: DataId) -> Option<Arc<VisualDesc>> {
        None
    }
    fn appearance(&self, _id: DataId) -> Option<Arc<Appearance>> {
        None
    }
}

impl PlaceRecords for Kit {
    fn setup(&self, id: DataId) -> Option<Arc<Setup>> {
        self.setups.get(&id.0).map(|s| s.get())
    }
    fn entity(&self, _id: DataId) -> Option<Arc<auberean_assets::entity::Entity>> {
        None
    }
}

impl PartRecords for Kit {
    fn loaded_setup(&self, id: DataId) -> Option<Arc<LoadedSetup>> {
        self.setups.get(&id.0).cloned()
    }
    fn mesh(&self, id: DataId) -> Option<Arc<Mesh>> {
        self.meshes.get(&id.0).map(|m| Arc::clone(&m.0))
    }
    fn physics_tree(&self, id: DataId) -> Option<Arc<BspTree>> {
        self.meshes.get(&id.0).and_then(|m| m.1.clone())
    }
    fn anim_map(&self, id: DataId) -> Option<Arc<AnimMap>> {
        self.maps
            .contains(&id.0)
            .then(|| Arc::new(AnimMap::default()))
    }
    fn has_material(&self, id: DataId) -> bool {
        self.materials.contains(&id.0)
    }
}

fn kit() -> Kit {
    let mut k = Kit::default();
    let body = Setup {
        flags: 0x4,
        meshes: vec![DataId(TORSO), DataId(ARM)],
        bones: vec![
            Bone {
                parent: u32::MAX,
                id: 10,
            },
            Bone { parent: 0, id: 11 },
        ],
        poses: vec![
            Pose {
                kind: 0,
                bones: vec![moved(0.0, 0.0, 1.0), moved_turned(0.0, 1.0, 0.0)],
            },
            Pose {
                kind: 5,
                bones: vec![moved(0.0, 0.0, 2.0), moved_turned(0.0, 2.0, 0.0)],
            },
        ],
        connection_points: vec![Location {
            kind: NECK,
            bone: 1,
            transform: moved(0.5, 0.0, 0.0),
        }],
        cylinders: vec![Cylinder {
            low: wv(0.0, 0.0, 0.0),
            radius: 0.4,
            height: 1.8,
        }],
        spheres: vec![auberean_primitives::Sphere {
            center: wv(0.0, 0.0, 0.5),
            radius: 0.5,
        }],
        height: 1.8,
        radius: 0.6,
        step_up_height: 0.5,
        step_down_height: 0.7,
        bounding_sphere: auberean_primitives::Sphere {
            center: wv(0.0, 0.0, 0.9),
            radius: 1.0,
        },
        selection_sphere: auberean_primitives::Sphere {
            center: wv(0.0, 0.0, 1.0),
            radius: 0.8,
        },
        ..Setup::default()
    };
    let one_bone = |meshes: Vec<DataId>, at: f32| Setup {
        meshes,
        bones: vec![Bone {
            parent: u32::MAX,
            id: 20,
        }],
        poses: vec![Pose {
            kind: 0,
            bones: vec![moved(0.0, 0.0, at)],
        }],
        ..Setup::default()
    };
    let skinned = Setup {
        meshes: vec![DataId(SKIN)],
        ..body.clone()
    };
    let loaded = |s: Setup| Arc::new(LoadedSetup::new(Arc::new(s)));
    k.setups.insert(BODY, loaded(body));
    k.setups
        .insert(HEAD, loaded(one_bone(vec![DataId(FACE)], 0.2)));
    k.setups.insert(
        HOLEY,
        loaded(one_bone(
            vec![DataId(TORSO), DataId(MESH_GONE), DataId(ARM)],
            0.0,
        )),
    );
    k.setups.insert(
        BONELESS,
        loaded(Setup {
            meshes: vec![DataId(TORSO), DataId(UNDRAWN)],
            ..Setup::default()
        }),
    );
    k.setups.insert(SKINNED, loaded(skinned));
    k.meshes
        .insert(TORSO, (made_up_mesh(TORSO_KIND, 0.5, true, false), None));
    k.meshes
        .insert(ARM, (made_up_mesh(3, 1.0, true, false), Some(floor_tree())));
    k.meshes
        .insert(FACE, (made_up_mesh(HEAD_KIND, 0.25, true, false), None));
    k.meshes.insert(
        HELM,
        (made_up_mesh(HEAD_KIND, 0.3, true, false), Some(ball_tree())),
    );
    k.meshes
        .insert(UNDRAWN, (made_up_mesh(TORSO_KIND, 5.0, false, false), None));
    k.meshes
        .insert(SKIN, (made_up_mesh(TORSO_KIND, 0.5, true, true), None));
    k.maps.insert(MAP);
    k.maps.insert(OTHER_MAP);
    k.materials.insert(MODIFIER);
    k
}

/// A look of the body (key 1) with the head (key 2) hung from its neck.
fn body_look(map: u32) -> Look {
    let mut look = Look::default();
    look.add_group(&Group {
        key: 1,
        setup: DataId(BODY),
        anim_map: DataId(map),
        ..Group::default()
    })
    .expect("the body");
    look.add_group(&Group {
        key: 2,
        parent_key: 1,
        connection: NECK,
        setup: DataId(HEAD),
        ..Group::default()
    })
    .expect("the head");
    look
}

/// The kit body's part groups put together.
pub(crate) fn body_parts() -> PartGroup {
    let mut g = PartGroup::default();
    assert!(g.init_look(&kit(), &body_look(MAP)));
    g
}

/// A body whose setup lets it turn freely (and one with no part group) goes into a move having
/// taken its end facing; one whose setup does not turns along the way.
///
/// Behaviour: parts.body.a-body-that-turns-freely-takes-its-end-facing-first
#[test]
fn a_body_that_turns_freely_takes_its_end_facing_first() {
    use dereth_physics::transition::{ObjectInfoState, Transition};
    let free =
        |b: &Body| b.object_info(&mut Transition::default()) & ObjectInfoState::FREE_ROTATE != 0;
    assert!(free(&body(50.0, 50.0)));
    let mut turning = body(50.0, 50.0);
    turning.set_parts(body_parts());
    assert!(free(&turning), "the setup turns freely");
    let mut k = kit();
    let mut setup = (*k.setups[&BODY].get()).clone();
    setup.flags &= !0x4;
    k.setups
        .insert(BODY, Arc::new(LoadedSetup::new(Arc::new(setup))));
    let mut g = PartGroup::default();
    assert!(g.init_look(&k, &body_look(MAP)));
    let mut stiff = body(50.0, 50.0);
    stiff.set_parts(g);
    assert!(!free(&stiff), "this setup does not");
}

/// A body put together from its look steps as its setup says, and a change to its step-down
/// height is its setup's, reaching every body of that setup; asked to stand in another of its
/// setup's poses it places its bones so and asks its figure to stand in it.
///
/// Behaviour: parts.body.a-body-steps-as-its-setup-says
#[test]
fn a_body_steps_as_its_setup_says() {
    use auberean_physics::body::{Animate, Notice};
    let kit = kit();
    let parts = || {
        let mut g = PartGroup::default();
        assert!(g.init_look(&kit, &body_look(MAP)));
        g
    };
    let (mut a, mut b) = (body(50.0, 50.0), body(52.0, 50.0));
    a.set_parts(parts());
    b.set_parts(parts());
    assert_eq!(a.step_up_height(), 0.5);
    a.set_step_down_height(0.3);
    assert_eq!(b.step_down_height(), 0.3, "the setup's, for both");
    assert!(a.set_placement_frame(5));
    let bone = a.part_group().expect("parts").array.bone_matrices[0];
    assert!((bone.0[3][2] - 2.0).abs() < 1e-5, "{:?}", bone.0[3]);
    assert!(a
        .take_requests()
        .contains(&Notice::Animate(Animate::PlacementFrame(5))));
    assert!(!body(0.0, 0.0).set_placement_frame(5), "no parts");
}

/// Made-up qualities of an object, as a profile asks them.
#[derive(Default)]
struct Quals {
    truths: BTreeMap<u32, bool>,
    ints: BTreeMap<u32, i32>,
    floats: BTreeMap<u32, f32>,
    ids: BTreeMap<u32, u32>,
}

impl ProfileQualities for Quals {
    fn truth(&mut self, key: u32) -> Option<bool> {
        self.truths.get(&key).copied()
    }
    fn int(&mut self, key: u32) -> Option<i32> {
        self.ints.get(&key).copied()
    }
    fn float(&mut self, key: u32) -> Option<f32> {
        self.floats.get(&key).copied()
    }
    fn data_id(&mut self, key: u32) -> Option<u32> {
        self.ids.get(&key).copied()
    }
}

/// A body takes what its qualities say of it when it is made. With none it reports and ignores
/// collisions and falls, unlit, not inelastic and not sliding along edges, in pose 0 at its own
/// scale 1; the player's qualities (it does not ignore collisions, it reports them, it slides along
/// edges) make it slide along edges; other qualities light it, make it inelastic, stop its fall,
/// stop its reports, place it in a pose and scale it. Scaled unevenly, a body with a physics tree
/// is marked so (one without is not).
///
/// Behaviour: motion.element.a-body-takes-what-its-qualities-say-of-it
#[test]
fn a_body_takes_what_its_qualities_say_of_it() {
    use auberean_physics::body::profile::{quality, Profile};
    use auberean_physics::body::{flags, Animate, Notice};
    let none = Profile::of(&mut Quals::default());
    assert_eq!(
        none.state,
        flags::REPORT_COLLISIONS | flags::IGNORE_COLLISIONS | flags::GRAVITY
    );
    assert_eq!(
        (none.physics, none.placement, none.scale),
        (0, 0, 1.0),
        "{none:?}"
    );
    assert_eq!(
        (none.friction, none.elasticity, none.mass),
        (0.95, 0.005, 1.0)
    );

    let mut b = body(50.0, 50.0);
    b.set_parts(body_parts());
    assert!(b.does_edge_slide(), "a new body slides along edges");
    b.apply_profile(&none);
    assert!(!b.does_edge_slide() && b.ignores_collisions() && b.reports_collisions());
    assert!(!b.lights_on() && !b.is_inelastic() && b.is_affected_by_gravity());
    assert!(b.is_mobile(), "whether it moves is not the profile's");

    let mut player = Quals::default();
    player.truths.extend([
        (quality::IGNORE_COLLISIONS, false),
        (quality::REPORT_COLLISIONS, true),
        (quality::EDGE_SLIDE, true),
    ]);
    b.apply_profile(&Profile::of(&mut player));
    assert!(b.does_edge_slide() && !b.ignores_collisions() && b.reports_collisions());

    let mut other = Quals::default();
    other.truths.extend([
        (quality::REPORT_COLLISIONS, false),
        (quality::GRAVITY, false),
        (quality::LIGHTS, true),
        (quality::INELASTIC, true),
        (quality::EDGE_SLIDE, false),
    ]);
    other.ints.insert(quality::PLACEMENT, 5);
    other.floats.extend([
        (quality::SCALE, 1.5),
        (quality::FRICTION, 0.5),
        (quality::ELASTICITY, 0.1),
        (quality::MASS, 80.0),
    ]);
    other.ids.insert(quality::PHYSICS, 0x4700_08CF);
    let p = Profile::of(&mut other);
    assert_eq!(
        p.state,
        flags::IGNORE_COLLISIONS | flags::LIGHTS | flags::INELASTIC
    );
    assert_eq!(
        (
            p.physics,
            p.placement,
            p.scale,
            p.friction,
            p.elasticity,
            p.mass
        ),
        (0x4700_08CF, 5, 1.5, 0.5, 0.1, 80.0)
    );
    b.take_requests();
    b.apply_profile(&p);
    assert!(!b.reports_collisions() && !b.is_affected_by_gravity());
    assert!(b.lights_on() && b.is_inelastic() && !b.does_edge_slide());
    assert_eq!(b.object_scale, Vec3::new(1.5, 1.5, 1.5));
    assert!(b
        .take_requests()
        .contains(&Notice::Animate(Animate::PlacementFrame(5))));

    assert!(b.has_physics_tree());
    assert_eq!(b.state() & flags::NON_UNIFORM_SCALE, 0, "even");
    b.set_scale_vector(Vec3::new(1.0, 1.0002, 1.0));
    assert_ne!(b.state() & flags::NON_UNIFORM_SCALE, 0, "uneven");
    b.set_scale(2.0);
    assert_eq!(b.state() & flags::NON_UNIFORM_SCALE, 0, "even again");
    let mut plain = body(0.0, 0.0);
    plain.set_scale_vector(Vec3::new(1.0, 2.0, 1.0));
    assert_eq!(plain.state() & flags::NON_UNIFORM_SCALE, 0, "no tree");
}

/// A body answers for its shape from its part group: its setup's cylinders, whether it turns
/// freely, its setup's bounding sphere to be sorted by, and its setup's selection sphere scaled by
/// its look's scale and then its own; with no part group none of them (it turns freely, its
/// sorting sphere has no size). It is the top of what holds it unless something holds it; it
/// stands for its cell at its first sphere's center; it takes a new cell, parts or not. Made with
/// a part group, it reports impacts no slower than its standard forward speed along the ground and
/// its jump impulse (times its jump scale) up and down, each plus 0.1.
///
/// Behaviour: parts.body.a-body-answers-for-its-shape-from-its-part-group
#[test]
fn a_body_answers_for_its_shape_from_its_part_group() {
    let mut bare = body(50.0, 50.0);
    assert!(bare.cylinders().is_empty() && bare.allows_free_heading());
    assert_eq!(bare.sorting_sphere().radius, 0.0);
    assert!(bare.selection_sphere().is_none());

    let mut b = body(50.0, 50.0);
    assert!(!b.init_object_end(), "nothing to end without parts");
    b.set_parts(body_parts());
    b.jump_scale = 2.0;
    assert!(b.init_object_end());
    let (xy, z) = b.min_reported_impact();
    assert!(
        (xy - (6.5 + 0.1)).abs() < 1e-5,
        "its standard forward speed: {xy}"
    );
    let jump = b.standard_motion_values().jump_impulse;
    assert!(
        (z - (jump * 2.0 + 0.1)).abs() < 1e-5,
        "its jump impulse twice: {z}"
    );
    let cyl = b.cylinders();
    assert_eq!(cyl.len(), 1);
    assert!((cyl[0].radius - 0.4).abs() < 1e-6 && (cyl[0].height - 1.8).abs() < 1e-6);
    assert!(b.allows_free_heading(), "the setup turns freely");
    let sorting = b.sorting_sphere();
    assert!((sorting.radius - 1.0).abs() < 1e-6 && (sorting.center.z - 0.9).abs() < 1e-6);
    b.look_scale = Vec3::new(2.0, 2.0, 2.0);
    b.object_scale = Vec3::new(1.0, 1.0, 0.5);
    let sel = b.selection_sphere().expect("a setup");
    assert!(
        (sel.center.z - 1.0).abs() < 1e-6,
        "1 m up, twice, then half: {sel:?}"
    );
    assert!(
        (sel.radius - 0.8).abs() < 1e-6,
        "0.8 m, twice, then half: {sel:?}"
    );

    b.id = 7;
    assert_eq!(b.top_level_object(), 7);
    b.has_parent = true;
    b.parent = 9;
    assert_eq!(b.top_level_object(), 9);

    let at = b.cell_position();
    let first = b.spheres()[0].center;
    assert!((at.frame.origin.z - (b.position.frame.origin.z + first.z)).abs() < 1e-5);
    bare.set_cell_id(CellId(0x4040_0002));
    assert_eq!(bare.position.cell, CellId(0x4040_0002));
    b.set_cell_id(CellId(0x4040_0003));
    assert_eq!(b.position.cell, CellId(0x4040_0003));
}

fn origin(m: &Mat4) -> [f32; 3] {
    [m.0[3][0], m.0[3][1], m.0[3][2]]
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-5)
}

fn part_origin(g: &PartGroup, i: usize) -> [f32; 3] {
    origin(&g.array.parts[i].as_ref().expect("a part").matrix)
}

/// A part array answers from its setup: its spheres, cylinders, radius, height, step heights,
/// whether it turns freely, its selection and bounding spheres. Without a setup it steps up and
/// down a hundredth of a meter, has none of the rest, and its step-down height and spheres cannot
/// be set. A setup not in the data changes nothing; a setup with a mesh not in the data keeps the
/// parts made before that mesh.
///
/// Behaviour: parts.array.a-part-array-answers-from-its-setup
#[test]
fn a_part_array_answers_from_its_setup() {
    let kit = kit();
    let mut a = PartArray::default();
    assert_eq!((a.step_up_height(), a.step_down_height()), (0.01, 0.01));
    assert!(a.spheres().is_empty() && a.cylinders().is_empty());
    assert_eq!(a.selection_sphere(), None);
    assert_eq!(a.bounding_sphere(), auberean_primitives::Sphere::default());
    assert!(!a.set_step_down_height(0.3) && !a.set_spheres(&[]));
    assert!(a.set_setup(&kit, DataId(BODY)));
    assert_eq!(a.parts.len(), 2);
    assert_eq!(
        (
            a.radius(),
            a.height(),
            a.step_up_height(),
            a.step_down_height()
        ),
        (0.6, 1.8, 0.5, 0.7)
    );
    assert!(a.allows_free_heading());
    assert_eq!((a.spheres().len(), a.cylinders()[0].height), (1, 1.8));
    assert_eq!(a.selection_sphere().map(|s| s.radius), Some(0.8));
    assert_eq!(a.bounding_sphere().radius, 1.0);
    assert!(a.set_step_down_height(0.3));
    assert_eq!(a.step_down_height(), 0.3);
    assert!(a.set_spheres(&[]) && a.spheres().is_empty());
    assert!(!a.set_setup(&kit, DataId(0x0400_00FF)), "not in the data");
    assert_eq!(a.setup_id, DataId(BODY));
    assert!(!a.set_setup(&kit, DataId(HOLEY)), "a mesh not in the data");
    assert_eq!(a.setup_id, DataId(HOLEY));
    assert!(
        a.parts[0].is_some() && a.parts[1].is_none() && a.parts[2].is_none(),
        "the part before the missing mesh is kept"
    );
}

/// What one part array changes in its setup every array of that setup sees, the setup being the
/// one the records keep: the step-down height and the spheres set through one array are the
/// other's too, and an array made from the setup afterwards takes them; the rest of the setup is
/// as it was, and an array of another setup keeps its own.
///
/// Behaviour: parts.array.a-change-to-a-setup-reaches-every-array-of-it
#[test]
fn a_change_to_a_setup_reaches_every_array_of_it() {
    let kit = kit();
    let (mut a, mut b, mut other) = (
        PartArray::default(),
        PartArray::default(),
        PartArray::default(),
    );
    assert!(a.set_setup(&kit, DataId(BODY)) && b.set_setup(&kit, DataId(BODY)));
    assert!(other.set_setup(&kit, DataId(SKINNED)));
    let ball = auberean_primitives::Sphere {
        center: wv(0.0, 0.0, 1.0),
        radius: 0.25,
    };
    assert!(a.set_step_down_height(0.3) && a.set_spheres(&[ball, ball]));
    assert_eq!(b.step_down_height(), 0.3);
    assert_eq!(b.spheres(), vec![ball, ball]);
    let mut later = PartArray::default();
    assert!(later.set_setup(&kit, DataId(BODY)));
    assert_eq!((later.step_down_height(), later.spheres().len()), (0.3, 2));
    assert_eq!((b.step_up_height(), b.cylinders()[0].height), (0.5, 1.8));
    assert_eq!(
        (other.step_down_height(), other.spheres().len()),
        (0.7, 1),
        "another setup keeps its own"
    );
}

/// A look puts its part groups together: each group takes its setup, a part for each mesh and its
/// bones placed in the setup's pose 0, and hangs from its connection point on the parent's bone. A
/// part's transform is its bone's, then its group's frame (the connection point's transform, then
/// the parent's bone, then the parent's frame). Placing the bones in another pose moves the parts;
/// a group whose setup lacks that pose takes its pose 0. A group whose animation map is not in the
/// data cannot be put together.
///
/// Behaviour: parts.group.a-look-hangs-each-group-from-its-connection-point
#[test]
fn a_look_hangs_each_group_from_its_connection_point() {
    let kit = kit();
    let mut g = PartGroup::default();
    assert!(g.init_look(&kit, &body_look(MAP)));
    assert_eq!((g.key, g.children.len(), g.parented), (1, 1, false));
    let head = &g.children[0];
    assert_eq!((head.key, head.connection, head.parent_bone), (2, NECK, 1));
    assert!(head.parented);
    let mut n = 0;
    g.count_groups(&mut n);
    assert_eq!(n, 2);
    g.set_owner(0x5000_0001);
    assert!(g.children[0]
        .array
        .parts
        .iter()
        .flatten()
        .all(|p| p.owner == 0x5000_0001));
    g.refresh_if_needed(None);
    assert!(!g.needs_refresh);
    assert!(near(part_origin(&g, 0), [0.0, 0.0, 1.0]));
    assert!(near(part_origin(&g, 1), [0.0, 1.0, 1.0]));
    assert!(near(part_origin(&g.children[0], 0), [0.0, 1.5, 1.2]));

    assert!(g.set_placement_frame(5));
    assert_eq!((g.array.placement, g.children[0].array.placement), (5, 0));
    g.update_bones();
    g.refresh_if_needed(None);
    assert!(near(part_origin(&g, 1), [0.0, 2.0, 2.0]));
    assert!(near(part_origin(&g.children[0], 0), [0.0, 2.5, 2.2]));

    let mut lost = PartGroup::default();
    assert!(!lost.init_look(&kit, &body_look(0x2400_00FF)));
}

/// The box around what a group draws takes in every part of it and of the groups under it: a
/// part's mesh's box as it is when its setup has bones; carried through the part's transform when
/// it has none, where a mesh that draws nothing adds nothing.
///
/// Behaviour: parts.group.the-box-around-what-is-drawn-takes-in-every-part
#[test]
fn the_box_around_what_is_drawn_takes_in_every_part() {
    let kit = kit();
    let mut g = PartGroup::default();
    assert!(g.init_look(&kit, &body_look(MAP)));
    let b = g.array.render_bounds;
    assert_eq!(
        (b.min, b.max),
        (Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 2.0))
    );
    let mut plain = PartGroup::default();
    assert!(plain.init_setup(&kit, DataId(BONELESS)));
    plain.refresh_matrices(None, &Mat4::translation(wv(10.0, 0.0, 0.0)));
    plain.cache_render_box();
    let b = plain.array.render_bounds;
    assert_eq!(
        (b.min, b.max),
        (Vec3::new(9.5, -0.5, 0.0), Vec3::new(10.5, 0.5, 1.0))
    );
}

/// What a look does to a group's parts: a mesh swap changes the mesh of each part of the swap's
/// kind, and with it the part's physics tree; the last opacity stays; each material change whose
/// modifier is in the data reaches every part. Applying the look's changes again first puts back
/// the setup's meshes and drops the material changes, while the opacity stays as it was.
///
/// Behaviour: parts.group.a-looks-changes-swap-meshes-and-materials
#[test]
fn a_looks_changes_swap_meshes_and_materials() {
    let kit = kit();
    let mut g = PartGroup::default();
    assert!(g.init_look(&kit, &body_look(MAP)));
    let head = &mut g.children[0];
    let changes = Atoms {
        meshes: vec![
            MeshAtom {
                priority: 1.0,
                mesh_kind: HEAD_KIND,
                mesh: DataId(HELM),
            },
            MeshAtom {
                priority: 1.0,
                mesh_kind: TORSO_KIND,
                mesh: DataId(ARM),
            },
        ],
        opacities: vec![
            OpacityAtom {
                priority: 1.0,
                opacity: 0.25,
            },
            OpacityAtom {
                priority: 1.0,
                opacity: 0.5,
            },
        ],
        materials: vec![
            MaterialAtom {
                modifier: DataId(MODIFIER),
                ..MaterialAtom::default()
            },
            MaterialAtom {
                modifier: DataId(0x3100_00FF),
                ..MaterialAtom::default()
            },
        ],
        ..Atoms::default()
    };
    assert!(head.apply_atoms(&kit, &changes));
    let part = head.array.parts[0].as_ref().expect("the face");
    assert_eq!(part.mesh.as_ref().map(|m| m.id), Some(DataId(HELM)));
    assert_eq!(part.materials.len(), 1);
    assert_eq!(head.array.opacity(), 0.5);
    assert!(head.cache_has_physics_tree(), "the helm has a tree");
    assert!(head.apply_atoms(&kit, &Atoms::default()));
    let part = head.array.parts[0].as_ref().expect("the face");
    assert_eq!(part.mesh.as_ref().map(|m| m.id), Some(DataId(FACE)));
    assert!(part.materials.is_empty());
    assert_eq!(head.array.opacity(), 0.5);
    assert!(!head.cache_has_physics_tree());
}

/// A ray through a physics tree stops where it first reaches the tree's solid side, the cut
/// plane's normal facing back along the ray; one starting on the solid side stops at once, and
/// one that stays clear reaches nothing. A ray through an object's parts is brought into the
/// object's frame across landblocks and shrunk by its scale, and every part it reaches multiplies
/// the fraction in turn (two parts met halfway leave a quarter); parts not yet found to have
/// physics trees are not reached.
///
/// Behaviour: parts.ray.a-ray-stops-at-the-solid-side-and-parts-multiply
#[test]
fn a_ray_stops_at_the_solid_side_and_parts_multiply() {
    let tree = floor_tree();
    let down = Ray::from_offset(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -10.0));
    let (mut f, mut n) = (0.0, Vec3::ZERO);
    assert!(trace_ray(&tree, &down, &mut f, &mut n));
    assert_eq!((f, n), (0.5, Vec3::new(0.0, 0.0, 1.0)));
    let up = Ray::from_offset(Vec3::new(0.0, 0.0, -5.0), Vec3::new(0.0, 0.0, 10.0));
    assert!(trace_ray(&tree, &up, &mut f, &mut n));
    assert_eq!((f, n), (0.0, Vec3::new(0.0, 0.0, -1.0)));
    let clear = Ray::from_offset(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -4.0));
    assert!(!trace_ray(&tree, &clear, &mut f, &mut n));
    assert_eq!(f, 1.0);

    let part = PhysicsPart {
        mesh: Some(PartMesh {
            id: DataId(ARM),
            kind: 3,
            tree: Some(Arc::clone(&tree)),
            bounds: BBox::default(),
            default_bounds: BBox::default(),
            drawn: true,
            weighted: false,
        }),
        ..PhysicsPart::default()
    };
    let at = Position::new(
        CellId(0x4040_0001),
        Frame::new(Vec3::new(10.0, 20.0, 0.0), Quat::IDENTITY),
    );
    // The ray is given in the next block east: 160 m back, 10 m up, falling 20 m.
    let ray = Ray::from_offset(Vec3::new(-150.0, 20.0, 10.0), Vec3::new(0.0, 0.0, -20.0));
    let east = CellId(0x4140_0001);
    let half = Vec3::new(0.5, 0.5, 0.5);
    let mut f = 1.0;
    assert!(part.ray_through_tree(&at, half, &ray, east, &mut f, &mut n));
    assert_eq!(f, 0.5);

    let mut g = PartGroup::default();
    g.array.parts = vec![Some(part.clone()), Some(part)];
    let two = Vec3::new(2.0, 2.0, 2.0);
    let mut f = 1.0;
    assert!(!g.ray_through_trees(&at, two, &ray, east, &mut f, &mut n));
    assert_eq!(f, 1.0);
    assert!(g.cache_has_physics_tree());
    assert!(g.ray_through_trees(&at, two, &ray, east, &mut f, &mut n));
    assert_eq!(f, 0.25);
}

/// A sphere with its base at `pos`, tested only for whether it overlaps something solid (as a
/// placement is).
fn placing_at(pos: &Position) -> Transition {
    let mut t = Transition::default();
    t.init();
    t.set_rules(auberean_physics::rules::rules());
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
    t.sphere_path.init_path(Some(pos.cell), Some(*pos), pos);
    t.sphere_path.set_check_pos(pos, Some(pos.cell));
    t.sphere_path.obstruction_ethereal = true;
    t
}

/// An object's parts meet a sphere placed in their physics trees, every group's parts tested in
/// the object's own frame (the head, hung well above the body, is tested where the object
/// stands); a sphere placed away from them, or parts not yet found to have trees, meet nothing.
///
/// Behaviour: parts.collision.the-parts-trees-meet-a-sphere-in-the-objects-frame
#[test]
fn the_parts_trees_meet_a_sphere_in_the_objects_frame() {
    let kit = kit();
    let mut g = PartGroup::default();
    assert!(g.init_look(&kit, &body_look(MAP)));
    g.children[0]
        .array
        .swap_meshes(&kit, HEAD_KIND, DataId(HELM));
    // The arm's floor would meet anything placed low: only the helm is tested.
    g.array.parts[1] = None;
    let land = Land::new();
    let objects: Arena<PhysicsObj> = Arena::default();
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    let here = Position::new(
        CellId(0x4040_0001),
        Frame::new(Vec3::new(50.0, 50.0, 10.0), Quat::IDENTITY),
    );
    let one = Vec3::new(1.0, 1.0, 1.0);
    let mut t = placing_at(&here);
    assert_eq!(
        g.find_obj_collisions(&ctx, &mut t, &here, one),
        TransitionState::Ok
    );
    assert!(g.cache_has_physics_tree());
    let mut t = placing_at(&here);
    assert_eq!(
        g.find_obj_collisions(&ctx, &mut t, &here, one),
        TransitionState::Collided
    );
    let away = Position::new(
        here.cell,
        Frame::new(Vec3::new(55.0, 50.0, 10.0), Quat::IDENTITY),
    );
    let mut t = placing_at(&away);
    assert_eq!(
        g.find_obj_collisions(&ctx, &mut t, &here, one),
        TransitionState::Ok
    );
}

/// The bone nearest a frame (by the bones' origins) gives its index, and the turn from the bone's
/// rotation to the frame's, at the bone's origin; an array without bones gives none. The bones as
/// points (each bone's place and its parent) are worked out only for parts weighted to bones.
///
/// Behaviour: parts.bones.the-nearest-bone-and-the-bones-as-points
#[test]
fn the_nearest_bone_and_the_bones_as_points() {
    let kit = kit();
    let mut a = PartArray::default();
    let quarter = Quat::new(
        std::f32::consts::FRAC_1_SQRT_2,
        0.0,
        0.0,
        std::f32::consts::FRAC_1_SQRT_2,
    );
    // Turned a half: a quarter more than the nearest bone.
    let frame = Frame::new(Vec3::new(0.0, 0.9, 1.1), Quat::new(0.0, 0.0, 0.0, 1.0));
    let (mut bone, mut offset) = (99, Frame::new(Vec3::ZERO, Quat::IDENTITY));
    assert!(!a.nearest_bone(&frame, &mut bone, &mut offset));
    assert!(a.set_setup(&kit, DataId(BODY)));
    assert!(a.nearest_bone(&frame, &mut bone, &mut offset));
    assert_eq!(bone, 1);
    assert_eq!(offset.origin, Vec3::ZERO);
    let q = offset.rotation;
    assert!(
        (q.w - quarter.w).abs() < 1e-5 && (q.z - quarter.z).abs() < 1e-5,
        "{q:?}"
    );
    assert!(!a.work_out_bone_points(), "not weighted to bones");
    assert!(a.set_setup(&kit, DataId(SKINNED)));
    assert!(a.has_bone_weights);
    assert!(a.work_out_bone_points());
    let points: Vec<([f32; 3], u32)> = a
        .bone_points
        .iter()
        .map(|p| ([p.origin.x, p.origin.y, p.origin.z], p.parent))
        .collect();
    assert_eq!(
        points,
        vec![([0.0, 0.0, 1.0], u32::MAX), ([0.0, 1.0, 1.0], 0)]
    );
}

/// A changed look brings the tree up to date: a group whose animation map changed is made anew
/// (without the groups under it, which are then made again), a group the look adds is made and
/// hung, a group the look no longer has goes, and a group the look leaves as it was keeps its
/// parts.
///
/// Behaviour: parts.group.a-changed-look-remakes-adds-and-drops-groups
#[test]
fn a_changed_look_remakes_adds_and_drops_groups() {
    let kit = kit();
    let mut g = PartGroup::default();
    assert!(g.init_look(&kit, &body_look(MAP)));
    g.children[0].array.set_owner(0x77);
    assert!(g.update_to_look(&kit, None, &body_look(MAP)));
    assert_eq!(g.children.len(), 1);
    assert!(
        g.children[0]
            .array
            .parts
            .iter()
            .flatten()
            .all(|p| p.owner == 0x77),
        "the head was left as it was"
    );

    assert!(g.update_to_look(&kit, None, &body_look(OTHER_MAP)));
    assert_eq!(g.array.anim_map_id(), DataId(OTHER_MAP));
    assert_eq!(g.children.len(), 1);
    assert!(
        g.children[0]
            .array
            .parts
            .iter()
            .flatten()
            .all(|p| p.owner == 0),
        "the head was made again"
    );

    let mut new_head = Look::default();
    new_head
        .add_group(&Group {
            key: 1,
            setup: DataId(BODY),
            anim_map: DataId(OTHER_MAP),
            ..Group::default()
        })
        .expect("the body");
    new_head
        .add_group(&Group {
            key: 3,
            parent_key: 1,
            connection: NECK,
            setup: DataId(HEAD),
            ..Group::default()
        })
        .expect("a new head");
    assert!(g.update_to_look(&kit, None, &new_head));
    let keys: Vec<u32> = g.children.iter().map(|c| c.key).collect();
    assert_eq!(keys, vec![3]);
    assert!(g.part_group(2).is_none() && g.part_group(3).is_some());
}
