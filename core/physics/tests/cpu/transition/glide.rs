//! Behaviour: none (tooling: the world editor's flying camera)
//! The gliding sphere sweep against flat ground, static rocks and slabs, beside the viewer's own
//! sweep: what the viewer stops at, the gliding sphere slides along, and it stops only where two
//! surfaces meet against its move. Moving along the ground it steps up onto a slab low enough for
//! the mover's step height, one step after another, and never down again; a taller slab, or the
//! same one met flying clear of the ground, it slides along or stops at, its height unchanged.
//! Fixture: a flat world, synthetic spherical rocks and synthetic slabs of physics polygons.

use std::sync::Arc;

use dereth_physics::geom::bsp::{BspNode, BspNodeKind, BspTree};
use dereth_physics::geom::{BBox, CylSphere, Plane, Polygon, Sphere};
use dereth_physics::source::SetupPart;
use dereth_physics::{globals, landdefs, PhysHandle, PhysicsWorld, SetupGeometry, V3};
use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};

use crate::common::physics_fixture::{flat_world, sphere_body};

/// The flat world's ground.
const GROUND: f32 = 20.0;

/// The viewer's sphere.
const R: f32 = globals::VIEWER_SPHERE_RADIUS;

/// The land cell `p` (meters in block `0xA9B4`) stands in.
fn cell_at(p: Vec3) -> CellId {
    let mut c = LandblockId::new(0xA9, 0xB4).cell(1);
    let mut o = p;
    assert!(landdefs::adjust_to_outside(&mut c, &mut o), "{p:?}");
    c
}

/// The body the sweeps are made for, standing well away from everything, with a walking body's
/// 0.6 m step height.
fn mover(w: &mut PhysicsWorld) -> PhysHandle {
    mover_stepping(w, 0.6)
}

/// [`mover`] with a step height of `step` meters.
fn mover_stepping(w: &mut PhysicsWorld, step: f32) -> PhysHandle {
    let p = Vec3::new(40.0, 40.0, GROUND);
    let h = w.create(ObjectId(1), sphere_body(0.5, step, 1.0), true);
    let c = cell_at(p);
    w.enter_cell(h, c);
    w.get_mut(h).expect("live").position = Position::new(c, Frame::new(p, Quat::IDENTITY));
    h
}

/// A static rock: one sphere of `radius` centered at `at`.
fn rock(w: &mut PhysicsWorld, id: u32, at: Vec3, radius: f32) {
    let geometry = Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::ZERO, radius)],
        sorting_sphere: Sphere::new(Vec3::ZERO, radius),
        radius,
        height: radius * 2.0,
        ..SetupGeometry::default()
    });
    let c = cell_at(at);
    let h = w.create(ObjectId(id), geometry, false);
    w.enter_cell(h, c);
    w.get_mut(h).expect("live").position = Position::new(c, Frame::new(at, Quat::IDENTITY));
    w.calc_cross_cells(h, false);
}

/// Fly the viewer's sphere from `from` by `step` a frame for `frames` frames, gliding or with
/// the viewer's own sweep: where it was after each frame.
fn fly(
    w: &mut PhysicsWorld,
    h: PhysHandle,
    from: Vec3,
    step: Vec3,
    frames: usize,
    glide: bool,
) -> Vec<Vec3> {
    let mut at = from;
    let mut path = vec![at];
    for _ in 0..frames {
        let cell = cell_at(at);
        let here = Position::new(cell, Frame::new(at, Quat::IDENTITY));
        let there = Position::new(cell, Frame::new(at.add(step), Quat::IDENTITY));
        let sphere = Sphere::new(Vec3::ZERO, R);
        let state = globals::VIEWER_OBJECT_INFO_STATE;
        let t = if glide {
            w.glide_sphere(h, state, sphere, cell, &here, &there)
        } else {
            w.sweep_sphere(h, state, sphere, cell, &here, &there)
        }
        .expect("a valid position");
        assert_eq!(
            t.sphere_path.curr_pos.cell.0 >> 16,
            0xA9B4,
            "still in the block"
        );
        at = t.sphere_path.curr_pos.frame.origin;
        path.push(at);
    }
    path
}

fn distance(a: Vec3, b: Vec3) -> f32 {
    a.sub(b).mag2().sqrt()
}

#[test]
fn flown_down_onto_the_ground_at_a_shallow_angle_the_gliding_sphere_slides_along_it_where_the_viewer_stops(
) {
    let mut w = flat_world();
    let h = mover(&mut w);
    // A tenth of a meter east and two centimeters down a frame: it reaches the ground (its
    // center a radius over it) after 35 frames, and flies on for 165 more.
    let from = Vec3::new(100.0, 100.0, GROUND + 1.0);
    let step = Vec3::new(0.1, 0.0, -0.02);
    let viewer = fly(&mut w, h, from, step, 200, false);
    let glide = fly(&mut w, h, from, step, 200, true);

    let stop = viewer[viewer.len() - 1];
    assert!(
        stop.x < 104.0,
        "the viewer stops where it touches: {stop:?}"
    );
    let end = glide[glide.len() - 1];
    assert!(
        end.x > 119.9,
        "the gliding sphere slides on east along the ground: {end:?}"
    );
    assert!(
        (end.z - (GROUND + R)).abs() < 0.01,
        "on the ground: {end:?}"
    );
    for p in &glide {
        assert!(p.z > GROUND + R - 0.01, "never under the ground: {p:?}");
    }
    // Every frame after it reached the ground took it a tenth of a meter on, none held.
    for pair in glide[40..].windows(2) {
        let moved = pair[1].x - pair[0].x;
        assert!((moved - 0.1).abs() < 1e-3, "{moved} from {:?}", pair[0]);
    }
}

#[test]
fn flown_at_a_rock_off_its_center_the_gliding_sphere_slides_round_it_and_never_into_it() {
    let mut w = flat_world();
    let h = mover(&mut w);
    let center = Vec3::new(105.0, 100.0, GROUND + 3.0);
    rock(&mut w, 2, center, 2.0);
    // East at the rock, half a meter north of its center, at its center's height.
    let from = Vec3::new(100.0, 100.5, GROUND + 3.0);
    let step = Vec3::new(0.1, 0.0, 0.0);
    let viewer = fly(&mut w, h, from, step, 150, false);
    let glide = fly(&mut w, h, from, step, 150, true);

    let stop = viewer[viewer.len() - 1];
    assert!(stop.x < 103.0, "the viewer stops at the rock: {stop:?}");
    let end = glide[glide.len() - 1];
    assert!(
        end.x > 110.0,
        "the gliding sphere went round it and on east: {end:?}"
    );
    assert!(end.y > 100.5 + 1.0, "round its north side: {end:?}");
    for p in &glide {
        assert!(
            distance(*p, center) > 2.0 + R - 0.01,
            "never into the rock: {p:?}"
        );
    }
}

#[test]
fn flown_into_the_notch_between_two_rocks_the_gliding_sphere_stops_there() {
    let mut w = flat_world();
    let h = mover(&mut w);
    // Two rocks overlapping north and south of the line it flies along: a corner, met head on.
    let north = Vec3::new(105.0, 101.4, GROUND + 3.0);
    let south = Vec3::new(105.0, 98.6, GROUND + 3.0);
    rock(&mut w, 2, north, 2.0);
    rock(&mut w, 3, south, 2.0);
    let from = Vec3::new(100.0, 100.0, GROUND + 3.0);
    let glide = fly(&mut w, h, from, Vec3::new(0.1, 0.0, 0.0), 150, true);

    let end = glide[glide.len() - 1];
    assert!(end.x < 104.0, "it stopped in the notch: {end:?}");
    assert!(
        distance(glide[100], end) < 1e-3,
        "and stayed there: {:?} then {end:?}",
        glide[100]
    );
    for p in &glide {
        assert!(
            distance(*p, north) > 2.0 + R - 0.01,
            "never into the north rock: {p:?}"
        );
        assert!(
            distance(*p, south) > 2.0 + R - 0.01,
            "never into the south rock: {p:?}"
        );
    }
}

/// A static slab `top` meters high standing on the ground over `x` east and `y` north (meters in
/// the block): a box of physics polygons, its top and its four sides, each facing out.
fn slab(w: &mut PhysicsWorld, id: u32, x: (f32, f32), y: (f32, f32), top: f32) {
    slab_on(w, id, x, y, 0.0, top);
}

/// [`slab`], standing `base` meters over the ground.
fn slab_on(w: &mut PhysicsWorld, id: u32, x: (f32, f32), y: (f32, f32), base: f32, top: f32) {
    let (hx, hy) = ((x.1 - x.0) * 0.5, (y.1 - y.0) * 0.5);
    // Corners as signs of the half sizes, and 0 or 1 of the height.
    let face = |c: [[f32; 3]; 4]| {
        Polygon::new(
            c.iter()
                .map(|p| Vec3::new(p[0] * hx, p[1] * hy, p[2] * top))
                .collect(),
        )
    };
    let faces = vec![
        face([[-1., -1., 1.], [1., -1., 1.], [1., 1., 1.], [-1., 1., 1.]]),
        face([[-1., -1., 0.], [-1., -1., 1.], [-1., 1., 1.], [-1., 1., 0.]]),
        face([[1., -1., 0.], [1., 1., 0.], [1., 1., 1.], [1., -1., 1.]]),
        face([[-1., -1., 0.], [1., -1., 0.], [1., -1., 1.], [-1., -1., 1.]]),
        face([[-1., 1., 0.], [-1., 1., 1.], [1., 1., 1.], [1., 1., 0.]]),
    ];
    let out = [
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, -1.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ];
    for (f, n) in faces.iter().zip(out) {
        assert!(f.plane.normal.sub(n).mag2() < 1e-8, "{:?}", f.plane.normal);
    }
    let bound = Sphere::new(
        Vec3::new(0.0, 0.0, top * 0.5),
        (hx * hx + hy * hy + top * top).sqrt(),
    );
    let tree = BspTree {
        nodes: vec![BspNode {
            sphere: bound,
            splitting_plane: Plane {
                normal: Vec3::new(0.0, 0.0, 1.0),
                d: 1000.0,
            },
            pos_child: None,
            neg_child: None,
            kind: BspNodeKind::Leaf {
                leaf_index: 0,
                solid: false,
            },
            in_polys: (0..5).collect(),
        }],
        polygons: faces,
    };
    let geometry = Arc::new(SetupGeometry {
        parts: vec![SetupPart {
            physics_bsp: Some(Arc::new(tree)),
            bound_box: Some(BBox::new(Vec3::new(-hx, -hy, 0.0), Vec3::new(hx, hy, top))),
            ..SetupPart::default()
        }],
        sorting_sphere: bound,
        radius: bound.radius,
        height: top,
        ..SetupGeometry::default()
    });
    let at = Vec3::new((x.0 + x.1) * 0.5, (y.0 + y.1) * 0.5, GROUND + base);
    let c = cell_at(at);
    let h = w.create(ObjectId(id), geometry, false);
    w.enter_cell(h, c);
    w.get_mut(h).expect("live").position = Position::new(c, Frame::new(at, Quat::IDENTITY));
    w.calc_cross_cells(h, false);
}

/// Where the sphere starts: half a centimeter over the flat ground, as a slide can leave it.
const LOW: f32 = GROUND + R + 0.005;

/// A tenth of a meter east a frame.
const EAST: Vec3 = Vec3::new(0.1, 0.0, 0.0);

/// The slab the steps are tried on: its west side 3 m east of where the sphere starts, its top
/// `top` meters over the ground.
fn slab_ahead(w: &mut PhysicsWorld, top: f32) {
    slab(w, 2, (103.0, 109.0), (95.0, 105.0), top);
}

/// The world with [`slab_ahead`] `top` meters high and a mover with a step height of `step`:
/// the gliding sphere flown east from `(100, 100, z)` for `frames` frames.
fn glide_at_slab(top: f32, step: f32, z: f32, frames: usize) -> Vec<Vec3> {
    let mut w = flat_world();
    let h = mover_stepping(&mut w, step);
    slab_ahead(&mut w, top);
    fly(&mut w, h, Vec3::new(100.0, 100.0, z), EAST, frames, true)
}

#[test]
fn moving_east_along_the_ground_into_a_slab_a_fifth_of_a_meter_high_the_gliding_sphere_steps_up_onto_it_and_flies_on_at_that_height(
) {
    let glide = glide_at_slab(0.2, 0.6, LOW, 120);
    for (i, p) in glide.iter().enumerate().skip(24).take(8) {
        println!("frame {i}: {:.3} {:.3} {:.3}", p.x, p.y, p.z);
    }
    // Every frame took it a tenth of a meter east, the one it stepped in too.
    for pair in glide.windows(2) {
        let moved = pair[1].x - pair[0].x;
        assert!((moved - 0.1).abs() < 1e-3, "{moved} from {:?}", pair[0]);
    }
    let up = GROUND + 0.2 + R;
    let stepped: Vec<usize> = (1..glide.len())
        .filter(|&i| glide[i].z - glide[i - 1].z > 1e-3)
        .collect();
    assert_eq!(stepped.len(), 1, "one step up: {stepped:?}");
    let at = glide[stepped[0]];
    assert!(
        at.x > 103.0 - R && at.x < 103.0 - R + 0.1 + 1e-3,
        "where it met the slab's side: {at:?}"
    );
    assert!((at.z - up).abs() < 1e-3, "onto its top: {at:?}");
    // On over the top and off its far side at the height it stepped up to: nothing takes it back
    // down.
    let end = glide[glide.len() - 1];
    assert!(end.x > 111.9, "{end:?}");
    for p in &glide[stepped[0]..] {
        assert!((p.z - up).abs() < 1e-3, "at the top's height: {p:?}");
    }
}

#[test]
fn moving_along_the_ground_up_a_stair_of_two_slabs_the_gliding_sphere_steps_up_each_in_turn() {
    let mut w = flat_world();
    let h = mover(&mut w);
    slab(&mut w, 2, (103.0, 106.0), (95.0, 105.0), 0.2);
    slab(&mut w, 3, (106.0, 109.0), (95.0, 105.0), 0.4);
    let glide = fly(&mut w, h, Vec3::new(100.0, 100.0, LOW), EAST, 100, true);
    let end = glide[glide.len() - 1];
    assert!(end.x > 109.9, "{end:?}");
    assert!(
        (end.z - (GROUND + 0.4 + R)).abs() < 1e-3,
        "on the upper step: {end:?}"
    );
    let lower = glide.iter().find(|p| p.x > 104.0 && p.x < 105.0).unwrap();
    assert!(
        (lower.z - (GROUND + 0.2 + R)).abs() < 1e-3,
        "on the lower step first: {lower:?}"
    );
}

#[test]
fn the_viewer_sweep_still_stops_at_the_low_slab_the_gliding_sphere_steps_onto() {
    let mut w = flat_world();
    let h = mover(&mut w);
    slab_ahead(&mut w, 0.2);
    let viewer = fly(&mut w, h, Vec3::new(100.0, 100.0, LOW), EAST, 60, false);
    let stop = viewer[viewer.len() - 1];
    assert!(stop.x < 103.0, "the viewer stops at the slab: {stop:?}");
    assert!(
        (stop.z - LOW).abs() < 1e-3,
        "and steps up nowhere: {stop:?}"
    );
}

#[test]
fn a_slab_taller_than_the_sphere_stops_the_gliding_sphere_head_on_and_met_at_an_angle_it_slides_along_its_side_at_its_own_height(
) {
    let head_on = glide_at_slab(0.7, 0.6, LOW, 60);
    let stop = head_on[head_on.len() - 1];
    assert!(
        stop.x > 103.0 - R - 0.1 && stop.x < 103.0 - R + 1e-3,
        "stopped at its side: {stop:?}"
    );
    for p in &head_on {
        assert!((p.z - LOW).abs() < 1e-4, "at its own height: {p:?}");
    }
    let mut w = flat_world();
    let h = mover(&mut w);
    slab_ahead(&mut w, 0.7);
    let angled = fly(
        &mut w,
        h,
        Vec3::new(100.0, 98.0, LOW),
        Vec3::new(0.1, 0.1, 0.0),
        60,
        true,
    );
    let end = angled[angled.len() - 1];
    assert!(
        end.x < 103.0 - R + 1e-3 && end.y > 102.0,
        "slid north along its side: {end:?}"
    );
    for p in &angled {
        assert!((p.z - LOW).abs() < 1e-4, "at its own height: {p:?}");
    }
}

#[test]
fn with_a_step_height_under_the_slab_the_gliding_sphere_stops_at_it_and_with_one_over_it_steps_up()
{
    // A tenth of a meter does not reach a fifth of a meter; six tenths does.
    let short = glide_at_slab(0.2, 0.1, LOW, 60);
    let end = short[short.len() - 1];
    assert!(
        end.x < 103.0 - R + 1e-3 && (end.z - LOW).abs() < 1e-4,
        "stopped: {end:?}"
    );
    let tall = glide_at_slab(0.2, 0.6, LOW, 60);
    let end = tall[tall.len() - 1];
    assert!(
        end.x > 105.0 && (end.z - (GROUND + 0.2 + R)).abs() < 1e-3,
        "stepped up: {end:?}"
    );
}

#[test]
fn flown_clear_of_the_ground_the_gliding_sphere_passes_over_the_low_slab_and_stops_at_a_tall_one_never_stepping_up(
) {
    // Its underside 0.4 m over the ground, more than its radius: not moving along the ground.
    let high = GROUND + R + 0.4;
    let over = glide_at_slab(0.2, 0.6, high, 120);
    assert!(over[over.len() - 1].x > 111.9, "{:?}", over[over.len() - 1]);
    for p in &over {
        assert!((p.z - high).abs() < 1e-4, "its height unchanged: {p:?}");
    }
    // A slab whose top is level with its center, which it reaches over.
    let into = glide_at_slab(0.7, 0.6, high, 60);
    let stop = into[into.len() - 1];
    assert!(stop.x < 103.0 - R + 1e-3, "stopped at the slab: {stop:?}");
    for p in &into {
        assert!((p.z - high).abs() < 1e-4, "its height unchanged: {p:?}");
    }
    // The same slab met with its underside a fifth of a meter over the ground, within its
    // radius: it steps up.
    let near = glide_at_slab(0.7, 0.6, GROUND + R + 0.2, 60);
    let end = near[near.len() - 1];
    assert!(
        end.x > 105.0 && (end.z - (GROUND + 0.7 + R)).abs() < 1e-3,
        "stepped up: {end:?}"
    );
}

#[test]
fn a_stack_of_low_slabs_the_gliding_sphere_climbs_in_one_step_up_to_one_and_a_half_times_its_step_height_where_one_slab_as_tall_stops_it(
) {
    // A step height of half a meter. Each slab's top lifts it once, and a step's lifts together
    // reach one and a half times the step height: two 0.3 m slabs stacked, 0.6 m, it climbs, three,
    // 0.9 m, it does not.
    for (slabs, climbs) in [(2_u8, true), (3, false)] {
        let mut w = flat_world();
        let h = mover_stepping(&mut w, 0.5);
        for i in 0..slabs {
            let base = 0.3 * f32::from(i);
            slab_on(
                &mut w,
                2 + u32::from(i),
                (103.0, 109.0),
                (95.0, 105.0),
                base,
                0.3,
            );
        }
        let glide = fly(&mut w, h, Vec3::new(100.0, 100.0, LOW), EAST, 60, true);
        let end = glide[glide.len() - 1];
        if climbs {
            let top = GROUND + 0.3 * f32::from(slabs) + R;
            assert!(
                end.x > 105.0 && (end.z - top).abs() < 1e-3,
                "{slabs}: {end:?}"
            );
        } else {
            assert!(
                end.x < 103.0 - R + 1e-3 && (end.z - LOW).abs() < 1e-4,
                "{slabs}: {end:?}"
            );
        }
    }
    // One slab 0.6 m high: its top is past where the sphere reaches over its side.
    let one = glide_at_slab(0.6, 0.5, LOW, 60);
    let end = one[one.len() - 1];
    assert!(
        end.x < 103.0 - R + 1e-3 && (end.z - LOW).abs() < 1e-4,
        "{end:?}"
    );
}

/// A static stump: a cylinder `height` meters high and a meter across, standing on the ground
/// at `at`.
fn stump(w: &mut PhysicsWorld, id: u32, at: Vec3, height: f32) {
    let geometry = Arc::new(SetupGeometry {
        cyl_spheres: vec![CylSphere {
            low_pt: Vec3::ZERO,
            height,
            radius: 0.5,
        }],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, height * 0.5), 1.0),
        radius: 0.5,
        height,
        ..SetupGeometry::default()
    });
    let c = cell_at(at);
    let h = w.create(ObjectId(id), geometry, false);
    w.enter_cell(h, c);
    w.get_mut(h).expect("live").position = Position::new(c, Frame::new(at, Quat::IDENTITY));
    w.calc_cross_cells(h, false);
}

#[test]
fn moving_along_the_ground_into_a_low_stump_the_gliding_sphere_steps_up_onto_it_and_into_a_tall_one_it_does_not(
) {
    for (height, climbs) in [(0.25, true), (1.0, false)] {
        let mut w = flat_world();
        let h = mover(&mut w);
        stump(&mut w, 2, Vec3::new(104.0, 100.0, GROUND), height);
        let glide = fly(&mut w, h, Vec3::new(100.0, 100.0, LOW), EAST, 60, true);
        let top = glide.iter().map(|p| p.z).fold(f32::MIN, f32::max);
        let end = glide[glide.len() - 1];
        if climbs {
            assert!(
                (top - (GROUND + height + R)).abs() < 1e-3,
                "onto its top: {top}"
            );
            assert!(end.x > 105.9, "and on: {end:?}");
        } else {
            assert!((top - LOW).abs() < 1e-4, "never up: {top}");
            assert!(end.x < 104.0 - 0.5 - R + 1e-3, "stopped at it: {end:?}");
        }
    }
}

#[test]
fn in_one_long_move_the_gliding_sphere_steps_up_onto_a_short_slab_and_flies_off_its_far_side_at_the_height_of_its_top(
) {
    // A slab a meter long, and one move of two and a half meters, nine steps of the move: up
    // onto it, over it and off it, then on over the ground.
    let mut w = flat_world();
    let h = mover(&mut w);
    slab(&mut w, 2, (103.0, 104.0), (95.0, 105.0), 0.2);
    let off = fly(
        &mut w,
        h,
        Vec3::new(102.0, 100.0, LOW),
        Vec3::new(2.5, 0.0, 0.0),
        1,
        true,
    );
    let end = off[1];
    assert!((end.x - 104.5).abs() < 1e-3, "{off:?}");
    assert!(
        (end.z - (GROUND + 0.2 + R)).abs() < 1e-3,
        "nothing took it back down: {end:?}"
    );
}

/// A static ramp: one polygon facing up, falling from `high` meters over the ground at `x.0`
/// to the ground at `x.1`, across `y`.
fn ramp(w: &mut PhysicsWorld, id: u32, x: (f32, f32), y: (f32, f32), high: f32) {
    let (hx, hy) = ((x.1 - x.0) * 0.5, (y.1 - y.0) * 0.5);
    let face = Polygon::new(vec![
        Vec3::new(-hx, -hy, high),
        Vec3::new(hx, -hy, 0.0),
        Vec3::new(hx, hy, 0.0),
        Vec3::new(-hx, hy, high),
    ]);
    assert!(
        face.plane.normal.z > 0.0 && face.plane.normal.x > 0.0,
        "{:?}",
        face.plane
    );
    let bound = Sphere::new(
        Vec3::new(0.0, 0.0, high * 0.5),
        (hx * hx + hy * hy + high * high).sqrt(),
    );
    let tree = BspTree {
        nodes: vec![BspNode {
            sphere: bound,
            splitting_plane: Plane {
                normal: Vec3::new(0.0, 0.0, 1.0),
                d: 1000.0,
            },
            pos_child: None,
            neg_child: None,
            kind: BspNodeKind::Leaf {
                leaf_index: 0,
                solid: false,
            },
            in_polys: vec![0],
        }],
        polygons: vec![face],
    };
    let geometry = Arc::new(SetupGeometry {
        parts: vec![SetupPart {
            physics_bsp: Some(Arc::new(tree)),
            bound_box: Some(BBox::new(Vec3::new(-hx, -hy, 0.0), Vec3::new(hx, hy, high))),
            ..SetupPart::default()
        }],
        sorting_sphere: bound,
        radius: bound.radius,
        height: high,
        ..SetupGeometry::default()
    });
    let at = Vec3::new((x.0 + x.1) * 0.5, (y.0 + y.1) * 0.5, GROUND);
    let c = cell_at(at);
    let h = w.create(ObjectId(id), geometry, false);
    w.enter_cell(h, c);
    w.get_mut(h).expect("live").position = Position::new(c, Frame::new(at, Quat::IDENTITY));
    w.calc_cross_cells(h, false);
}

#[test]
fn lowered_straight_down_onto_a_walkable_ramp_the_gliding_sphere_slides_down_it_to_the_ground_and_steps_nowhere(
) {
    // Two meters down over four east, walkable; lowered a tenth of a meter a frame from a meter
    // over it.
    let mut w = flat_world();
    let h = mover(&mut w);
    ramp(&mut w, 2, (100.0, 104.0), (95.0, 105.0), 2.0);
    let from = Vec3::new(101.0, 100.0, GROUND + 1.5 + 1.0);
    let glide = fly(&mut w, h, from, Vec3::new(0.0, 0.0, -0.1), 120, true);
    for p in &glide {
        // Its distance from the ramp's plane, which falls half a meter a meter east to the
        // ground at x = 104.
        let off_ramp = ((p.x - 104.0) + 2.0 * (p.z - GROUND)) / 5.0_f32.sqrt();
        assert!(off_ramp > R - 0.01, "never into the ramp: {p:?}");
        assert!(p.z > GROUND + R - 0.01, "never into the ground: {p:?}");
        assert!(p.z <= from.z + 1e-4, "never up: {p:?}");
    }
    let end = glide[glide.len() - 1];
    assert!(end.x > 103.0, "slid down it, east: {end:?}");
    assert!(end.z < GROUND + R + 0.3, "to the ground: {end:?}");
}

#[test]
fn a_millimeter_into_the_ground_and_moving_along_it_level_or_rising_off_it_by_a_hair_the_gliding_sphere_moves_by_its_own_move_and_is_never_thrown_along_it(
) {
    // As a slide along the ground leaves it: its center a millimeter short of a radius over the
    // ground, and its move along the ground. Rising off it by a hundred-thousandth of a meter a
    // tenth of a meter on, the move nears the ground by next to nothing.
    for rise in [0.0, 1e-5] {
        let mut w = flat_world();
        let h = mover(&mut w);
        let from = Vec3::new(100.0, 100.0, GROUND + R - 0.001);
        let step = Vec3::new(0.1, 0.0, rise);
        let glide = fly(&mut w, h, from, step, 30, true);
        for pair in glide.windows(2) {
            let moved = distance(pair[1], pair[0]);
            assert!(
                (moved - 0.1).abs() < 1e-3,
                "rising {rise}: moved {moved} from {:?} to {:?}",
                pair[0],
                pair[1]
            );
        }
        let end = glide[glide.len() - 1];
        assert!((end.x - 103.0).abs() < 1e-3, "rising {rise}: {end:?}");
    }
}

/// How steeply the creased world's ground rises east of its crease: two meters a vertex column.
const RISE: u8 = 2;

/// Blocks `0xA9B4` and `0xAAB4`: level ground at 20 m as far east as x = 96 in `0xA9B4`, and from
/// there ground rising east two meters every 24, on across `0xAAB4`.
fn creased_world() -> PhysicsWorld {
    let mut land = dereth_physics::StaticLandSource::linear();
    let table = *dereth_physics::LandSource::height_table(&land);
    for (bx, from) in [(0xA9_u8, 0_u8), (0xAA, 8)] {
        let mut h = [10_u8; dereth_physics::land::VERTEX_COUNT];
        for i in 0..9_u8 {
            for j in 0..9_usize {
                let column = from + i;
                h[usize::from(i) * 9 + j] = 10 + column.saturating_sub(4) * RISE;
            }
        }
        let block = dereth_physics::LandblockCollision::build(
            LandblockId::new(bx, 0xB4),
            Box::new(h),
            Box::new([0; dereth_physics::land::VERTEX_COUNT]),
            false,
            8,
            &table,
        )
        .expect("a full-detail block");
        land.add_block(block);
    }
    PhysicsWorld::new(Arc::new(land))
}

/// The creased world's ground under `x` meters east of `0xA9B4`'s west side.
fn creased_ground(x: f32) -> f32 {
    GROUND + (x - 96.0).max(0.0) * f32::from(RISE) * 2.0 / 24.0
}

/// [`fly`] gliding across landblocks: positions in `0xA9B4`'s meters, whichever block the
/// sphere is in.
fn glide_across(
    w: &mut PhysicsWorld,
    h: PhysHandle,
    from: Vec3,
    step: Vec3,
    frames: usize,
) -> Vec<Vec3> {
    let home = LandblockId::new(0xA9, 0xB4).cell(1);
    let mut at = from;
    let mut path = vec![at];
    for _ in 0..frames {
        let mut cell = home;
        let mut local = at;
        assert!(landdefs::adjust_to_outside(&mut cell, &mut local), "{at:?}");
        let here = Position::new(cell, Frame::new(local, Quat::IDENTITY));
        let there = Position::new(cell, Frame::new(local.add(step), Quat::IDENTITY));
        let sphere = Sphere::new(Vec3::ZERO, R);
        let state = globals::VIEWER_OBJECT_INFO_STATE;
        let t = w
            .glide_sphere(h, state, sphere, cell, &here, &there)
            .expect("a valid position");
        let p = t.sphere_path.curr_pos;
        at = p.frame.origin.add(landdefs::get_block_offset(home, p.cell));
        path.push(at);
    }
    path
}

#[test]
fn flown_along_level_ground_into_ground_rising_from_a_crease_and_on_into_the_next_landblock_the_gliding_sphere_slides_up_it_every_frame_and_never_into_it(
) {
    // A tenth of a meter a frame, and a meter and a half, several steps of the move.
    for ahead in [0.1_f32, 1.5] {
        let mut w = creased_world();
        let h = mover(&mut w);
        let from = Vec3::new(80.0, 100.0, GROUND + R + 0.005);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = (130.0 / ahead) as usize;
        let glide = glide_across(&mut w, h, from, Vec3::new(ahead, 0.0, 0.0), frames);
        let slope = f32::from(RISE) * 2.0 / 24.0;
        let cos = 1.0 / (1.0 + slope * slope).sqrt();
        for pair in glide.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let moved = distance(b, a);
            assert!(
                moved < ahead + 1e-3,
                "{ahead}: moved {moved} from {a:?} to {b:?}"
            );
            // On it never holds: up the slope a move east goes on east by all but what the
            // slope takes of it.
            assert!(b.x - a.x > 0.9 * ahead, "{ahead}: held from {a:?} to {b:?}");
            // Never into the ground, away from the crease where the sphere meets both.
            let off = if b.x < 96.0 - R {
                b.z - GROUND
            } else if b.x > 96.0 + R {
                (b.z - creased_ground(b.x)) * cos
            } else {
                continue;
            };
            assert!(off > R - 1e-3, "{ahead}: {off} off the ground at {b:?}");
        }
        let end = glide[glide.len() - 1];
        assert!(
            end.x > 205.0,
            "{ahead}: on into the next landblock: {end:?}"
        );
    }
}
