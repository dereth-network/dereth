//! Synthetic rotated surfaces and step-height fixtures.

pub(crate) use std::collections::BTreeMap;
pub(crate) use std::sync::Arc;

pub(crate) use dereth_physics::arena::Arena;
pub(crate) use dereth_physics::geom::bsp::{BspNode, BspNodeKind, BspTree};
pub(crate) use dereth_physics::geom::{Plane, Polygon, Sphere};
pub(crate) use dereth_physics::transition::collide;
pub(crate) use dereth_physics::transition::objectinfo::ObjectInfoState;
pub(crate) use dereth_physics::transition::TransitionCtx;
pub(crate) use dereth_physics::{SetupGeometry, StaticLandSource, Transition, TransitionState, V3};
pub(crate) use dereth_primitives::num::math;
pub(crate) use dereth_primitives::{
    CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3,
};

// =================================================================================================
// (a) the rotation, asserted directly
// =================================================================================================

/// A quarter turn about **x**, which is the whole point: it is not a rotation about z, so it does
/// not fix `(0, 0, ±1)`.
///
///  turns it into the matrix `l2g` builds, and
///  then maps `(x, y, z) -> (x, -z, y)`. Both are asserted in
/// [`the_placement_rotation_is_off_z_and_moves_the_normal_this_fixture_uses`] before anything is
/// concluded from them, because a fixture whose rotation is secretly an identity is exactly the
/// hole this row exists to close.
pub(crate) fn quarter_turn_about_x() -> Quat {
    let h = std::f32::consts::FRAC_1_SQRT_2;
    Quat::new(h, h, 0.0, 0.0)
}

pub(crate) fn one_square_tree() -> BspTree {
    let square = Polygon::new(vec![
        Vec3::new(-5.0, -5.0, 0.0),
        Vec3::new(5.0, -5.0, 0.0),
        Vec3::new(5.0, 5.0, 0.0),
        Vec3::new(-5.0, 5.0, 0.0),
    ]);
    assert!(
        (square.plane.normal.sub(Vec3::new(0.0, 0.0, 1.0))).mag2() < 1e-8,
        "the winding must give +z, or the movement gate below is the wrong way round: {:?}",
        square.plane.normal
    );
    let bound = Sphere::new(Vec3::ZERO, 7.2);
    BspTree {
        nodes: vec![
            BspNode {
                sphere: bound,
                splitting_plane: Plane {
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    d: 0.0,
                },
                pos_child: Some(1),
                neg_child: Some(2),
                kind: BspNodeKind::Node,
                in_polys: vec![],
            },
            BspNode {
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
                in_polys: vec![],
            },
            BspNode {
                sphere: bound,
                splitting_plane: Plane {
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    d: 1000.0,
                },
                pos_child: None,
                neg_child: None,
                kind: BspNodeKind::Leaf {
                    leaf_index: 1,
                    solid: true,
                },
                in_polys: vec![0],
            },
        ],
        polygons: vec![square],
    }
}

pub(crate) const CELL: CellId = CellId(0x00A9_B401);

// =================================================================================================
// (b) the step-up budget, made falsifiable outside `--lib`
// =================================================================================================

pub(crate) const STEP_UP_HEIGHT: f32 = 0.600;

/// **The pair that straddles it: 0.560 m is climbed, 0.640 m is not**, and the only difference
/// between the two obstacles is 8 cm of height.
///
/// **Why the budget binds, as arithmetic rather than as a measurement**, because *that* is what
/// makes this a fixture instead of another `max_lift` reading. The sphere's step up opens
/// with
///
/// `text
/// if (object_info.step_up_height < (sum + EPSILON) - delta.z) -> slide, do not climb
/// `
///
/// where `sum` is the two radii added and `delta` is the mover's centre minus the obstacle's.
/// With the mover resting on flat ground at `z = GROUND + r_m`:
///
/// `text
/// sum - delta.z = (r_m + r_o) - ((GROUND + r_m) - z_o) = (z_o + r_o) - GROUND
/// `
///
/// -- the mover's own radius cancels, and what is left is **exactly the height of the obstacle's
/// top above the floor**. So in the geometry's own terms the branch reads *`step_up_height <
/// obstacle top above the floor`*: 0.600 against 0.560 is false and the body climbs; 0.600
/// against 0.640 is true and it does not. Nothing else about the two obstacles differs, so
/// nothing else can be what separated them.
pub(crate) const CLIMBABLE_TOP: f32 = 0.560;
pub(crate) const TOO_HIGH_TOP: f32 = 0.640;

/// The pair has to straddle the budget or the whole file is measuring something else. Stated at
/// compile time rather than inside a test, because as a run-time assertion over three constants it
/// is one `clippy::assertions_on_constants` finding and no protection at all if the test is not run.
const _: () = assert!(
    CLIMBABLE_TOP < STEP_UP_HEIGHT && STEP_UP_HEIGHT < TOO_HIGH_TOP,
    "CLIMBABLE_TOP must sit under STEP_UP_HEIGHT and TOO_HIGH_TOP over it"
);

pub(crate) const GROUND: f32 = 20.0;

pub(crate) const OBSTACLE_RADIUS: f32 = 3.0;

/// The per-frame offset the walk script hands the body, east. Named because the smallest lateral
/// offset that escapes the head-on degeneracy is derived from it -- see [`OFF_AXIS`].
pub(crate) const SCRIPT_STEP: f32 = 0.06;

/// A body carrying the retail `step_up_height`. Everything else is `walk_scenarios.rs`' player.
pub(crate) fn body(step_up_height: f32) -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0),
        step_up_height,
        step_down_height: 0.3,
        radius: 0.5,
        height: 1.0,
        ..SetupGeometry::default()
    })
}

/// What one walk produced.
pub(crate) struct Walk {
    /// How far the body rose over the walk.
    pub(crate) rise: f32,
    /// The **deepest** it ever got inside the obstacle over the whole walk, as
    /// `centre distance - (r_obstacle + r_body)`. Negative is inside the sphere.
    pub(crate) deepest: f32,
    /// The same quantity on the **last** frame: where the body came to rest relative to the
    /// obstacle's surface.
    pub(crate) gap: f32,
    /// Where the body ended.
    pub(crate) end: Vec3,
    /// How far it travelled over the last second of the walk, i.e. thirty-one frames at the 30 Hz
    /// these walks run at. Under a centimetre is treated as at rest.
    pub(crate) last_second: f32,
}

/// Walk a body east at a static sphere whose top stands `top` metres above the floor, straight at
/// its centre.
pub(crate) fn walk_at_an_obstacle(top: f32, step_up_height: f32) -> Walk {
    walk_past_an_obstacle(top, step_up_height, 0.0)
}

/// How long each walk lasts: 136 steps of [`SCRIPT_STEP`], 8.1 m, which reaches an obstacle's
/// face about halfway through and then pushes against it for the rest.
pub(crate) const WALK_SECONDS: f64 = 4.5;

/// The same walk, started `lateral` metres to the north so the approach is **not** exactly through
/// the obstacle's centre.
///
/// **Why the parameter exists:** at `lateral == 0.0` the body's heading, the
/// obstacle's centre and the world's `x` axis are the same line, so the sliding normal comes out
/// as exactly `(-1, 0, 0)` and the ground's contact normal as exactly `(0, 0, 1)`, and
/// the offset adjustment's crease branch projects the requested step onto an
/// axis exactly perpendicular to it and returns exactly zero. That is a coincidence of the
/// arithmetic, not a property of the obstacle -- see
/// [`the_exactly_head_on_approach_is_a_degeneracy_and_the_budget_is_not_what_decides_it`], which
/// pins it and shows it is identical for an obstacle no budget could ever lift.
pub(crate) fn walk_past_an_obstacle(top: f32, step_up_height: f32, lateral: f32) -> Walk {
    let mut w = flat_world();
    //  picks the land cell a point falls in. Naming a cell
    // by hand instead puts the body in a cell it is not standing in and the obstacle never enters
    // its cell list -- measured: the body then walks straight through it, rise 0.000 m at every
    // height, which reads exactly like a budget that binds everywhere.
    let cell_at = |x: f32, y: f32, z: f32| {
        let mut c = LandblockId::new(0xA9, 0xB4).cell(1);
        let mut origin = Vec3::new(x, y, z);
        dereth_physics::landdefs::adjust_to_outside(&mut c, &mut origin);
        c
    };
    let start = Vec3::new(100.0, 100.0 + lateral, GROUND);
    let cell = cell_at(start.x, start.y, GROUND);

    let mover = w.create(ObjectId(1), body(step_up_height), true);
    w.enter_cell(mover, cell);
    {
        let o = w.get_mut(mover).expect("live");
        o.position = Position::new(cell, Frame::new(start, Quat::IDENTITY));
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            Vec::new(),
            true,
        )));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(mover, false);

    // The obstacle: one sphere, sunk into the floor until its top stands `top` above it. Sinking
    // it rather than shrinking it keeps the surface curvature -- and therefore the walkable-slope
    // ceiling above -- identical between the two arms, so the only difference between them is the
    // height.
    let obstacle = Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::ZERO, OBSTACLE_RADIUS)],
        sorting_sphere: Sphere::new(Vec3::ZERO, OBSTACLE_RADIUS),
        radius: OBSTACLE_RADIUS,
        height: OBSTACLE_RADIUS * 2.0,
        ..SetupGeometry::default()
    });
    let z = GROUND + top - OBSTACLE_RADIUS;
    let obstacle_cell = cell_at(105.0, 100.0, z);
    let blocker = w.create(ObjectId(2), obstacle, false);
    w.enter_cell(blocker, obstacle_cell);
    {
        let o = w.get_mut(blocker).expect("live");
        o.position = Position::new(
            obstacle_cell,
            Frame::new(Vec3::new(105.0, 100.0, z), Quat::IDENTITY),
        );
        assert!(
            o.state.is_static(),
            "a STATIC_PS obstacle is what a retail stair-step is"
        );
    }
    w.calc_cross_cells(blocker, false);

    let offsets = vec![Frame::new(Vec3::new(SCRIPT_STEP, 0.0, 0.0), Quat::IDENTITY); 400];
    w.get_mut(mover)
        .expect("live")
        .set_motion(Box::new(dereth_physics::ScriptedMotion::new(offsets, true)));

    let centre = Vec3::new(105.0, 100.0, z);
    let gap_at = |p: Vec3| {
        let d = p.add(Vec3::new(0.0, 0.0, 0.5)).sub(centre);
        d.dot(d).sqrt() - (OBSTACLE_RADIUS + 0.5)
    };
    let mut peak = f32::MIN;
    let mut floor = f32::MAX;
    let mut deepest = f32::MAX;
    let mut path: Vec<Vec3> = Vec::new();
    let mut t = 0.0;
    while t < WALK_SECONDS {
        t += 1.0 / 30.0;
        w.use_time(LocalTime(t), false);
        let p = w.get(mover).expect("live").position.frame.origin;
        floor = floor.min(p.z);
        peak = peak.max(p.z);
        deepest = deepest.min(gap_at(p));
        path.push(p);
    }
    let n = path.len();
    let back = 31.min(n - 1);
    let tail = path[n - 1].sub(path[n - 1 - back]);
    Walk {
        rise: peak - floor,
        deepest,
        gap: gap_at(path[n - 1]),
        end: path[n - 1],
        last_second: tail.dot(tail).sqrt(),
    }
}

/// Two centimetres north of the obstacle's centre line -- enough that the approach is not exactly
/// through the centre, small enough that the body still runs squarely into the obstacle.
///
/// **The lower edge is arithmetic, not taste, and it is asserted at each station.** The crease in
/// offset adjustment returns `axis * (axis . v)`, and the transitional-position search
/// abandons the sub-step when that comes out
/// under `EPSILON` in magnitude. With the body `lateral` metres off the line and first contact at a
/// horizontal distance `dx`, the surviving component is `|v| * lateral / dx`, so the offset has to
/// clear
///
/// `text
/// lateral > EPSILON * dx / |v|
/// `
///
/// -- 0.0067 m for the 0.640 m obstacle (`dx` = 2.018 m) and 0.0115 m for the 3 m one
/// (`dx` = 3.464 m), at the 0.06 m script step. One centimetre therefore clears the first and
/// **not** the second, which is measured in
/// [`the_exactly_head_on_approach_is_a_degeneracy_and_the_budget_is_not_what_decides_it`] rather
/// than worked around. Two clears both.
///
/// The upper edge is behavioural: by 0.25 m the body slides *around* the obstacle instead of
/// stopping against it, so a station much larger would be measuring a graze.
/// [`the_off_axis_offset_is_bracketed_on_both_sides`] holds both edges.
pub(crate) const OFF_AXIS: f32 = 0.02;

/// Forty metres north: the same world, the same script and the same obstacle, placed where the body
/// can never reach it. This is what *unimpeded* looks like, so "stopped short" below is a
/// comparison rather than a guess.
pub(crate) const OUT_OF_REACH: f32 = 40.0;

/// The discriminating station of
/// [`the_exactly_head_on_approach_is_a_degeneracy_and_the_budget_is_not_what_decides_it`]: a
/// sphere whose top stands a full radius above the floor, so its equator is at ground level. Its
/// first contact normal has `n.z = (sum - top) / sum` = 0.143 against the 0.6642 walkable floor,
/// which refuses at **any** `step_up_height`.
pub(crate) const UNCLIMBABLE_TOP: f32 = OBSTACLE_RADIUS;

/// It has to stand far above any budget this client could carry, or it is not a station about
/// which the budget is provably irrelevant. Stated at compile time for the reason the pair above
/// is: as a run-time assertion over two constants it is one `clippy::assertions_on_constants`
/// finding and no protection at all if the test is not run.
const _: () = assert!(
    UNCLIMBABLE_TOP > STEP_UP_HEIGHT * 4.0,
    "UNCLIMBABLE_TOP must stand far above any step_up_height this client could carry"
);

pub(crate) use crate::common::physics_fixture::one_block_world as flat_world;
