//! On a 24 m line the container rule differs from the floor; a walk resting on the x line, y line
//! or corner reports the same cell from every direction; the cell array is in client order; the
//! viewer sweep reports the container cell; census of resting positions.
//! Fixture: synthetic state, geometry and reference vectors.

use std::sync::Arc;

use dereth_physics::geom::Sphere;
use dereth_physics::{globals, landdefs, LandSource, PhysHandle, PhysicsWorld, SetupGeometry, V3};
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

/// Outdoor cell width in metres.
const CELL: f32 = 24.0;
/// The home block of every station below.
const HOME: LandblockId = LandblockId(0xA9B4);
/// Ground level of [`flat_world`]: `add_flat_block(id, 10)` and the linear `2 * i` height table.
const GROUND: f32 = 20.0;

// =================================================================================================
// The world
// =================================================================================================

// Nine flat landblocks around `0xA9B4`, ground at `z = 20`. Handed to both the world and this
// file's oracle, so the two consult the same geometry.

// A one-sphere body: the `0.5` radius `walk_scenarios` uses, so that `lo = 23.5` and `hi = 0.5`
// and a station on a line is well clear of both.

fn spawn(w: &mut PhysicsWorld, id: u32, origin: Vec3) -> PhysHandle {
    let h = w.create(ObjectId(id), player_geometry(), true);
    let cell = {
        let mut c = HOME.cell(1);
        let mut o = origin;
        assert!(
            landdefs::adjust_to_outside(&mut c, &mut o),
            "{origin:?} is off the world"
        );
        c
    };
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        o.position = Position::new(cell, Frame::new(origin, Quat::IDENTITY));
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            Vec::new(),
            true,
        )));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    h
}

/// Give the body an animation-driven walk of exactly `steps` sub-steps of `per_substep`, and
/// nothing after them — so it comes to rest at `start + steps * per_substep` and stays there
/// while the world keeps running.
fn walk(w: &mut PhysicsWorld, h: PhysHandle, per_substep: Vec3, steps: usize) {
    let offsets = vec![Frame::new(per_substep, Quat::IDENTITY); steps];
    w.get_mut(h)
        .expect("live")
        .set_motion(Box::new(dereth_physics::ScriptedMotion::new(offsets, true)));
}

fn run(w: &mut PhysicsWorld, seconds: f64, fps: f64) {
    let dt = 1.0 / fps;
    let mut t = 0.0;
    while t < seconds {
        t += dt;
        w.use_time(LocalTime(t), false);
    }
}

// =================================================================================================
// The oracle: the cell-list search's container arm, written from retail behaviour
// =================================================================================================

/// The array the client builds for **one** sphere, in order,
/// deduplicated the way the client does (by id, first add wins its
/// place) and bounds-checked the way the client does
/// (`-1 < c && c < 0x7f8` on both coordinates).
///
/// Deliberately **not** a call into `CellResolver::find_cell_list`: this is the rule re-derived
/// independently so that the comparison below is between two readings that could
/// have disagreed.
fn client_cell_array(base: CellId, center: Vec3, radius: f32) -> Vec<CellId> {
    let mut id = base;
    let mut o = center;
    if !landdefs::adjust_to_outside(&mut id, &mut o) {
        return Vec::new();
    }
    let (cx, cy) = (landdefs::cell_of(o.x), landdefs::cell_of(o.y));
    #[allow(clippy::cast_precision_loss)]
    let local = Vec3::new(o.x - cx as f32 * CELL, o.y - cy as f32 * CELL, 0.0);
    let (lo, hi) = (CELL - radius, radius);
    let Some((gx, gy)) = landdefs::gid_to_lcoord(id) else {
        return Vec::new();
    };

    let mut out: Vec<CellId> = Vec::new();
    let push = |x: i32, y: i32, out: &mut Vec<CellId>| {
        if !(0..0x7F8).contains(&x) || !(0..0x7F8).contains(&y) {
            return;
        }
        let cid = landdefs::lcoord_to_gid(x, y);
        if !out.contains(&cid) {
            out.push(cid);
        }
    };
    push(gx, gy, &mut out);
    // in its own order.
    if lo < local.x {
        push(gx + 1, gy, &mut out);
        if lo < local.y {
            push(gx + 1, gy + 1, &mut out);
        }
        if local.y < hi {
            push(gx + 1, gy - 1, &mut out);
        }
    }
    if local.x < hi {
        push(gx - 1, gy, &mut out);
        if lo < local.y {
            push(gx - 1, gy + 1, &mut out);
        }
        if local.y < hi {
            push(gx - 1, gy - 1, &mut out);
        }
    }
    if lo < local.y {
        push(gx, gy + 1, &mut out);
    }
    if local.y < hi {
        push(gx, gy - 1, &mut out);
    }
    out
}

/// `find_cell_list`'s container loop for an outdoor position: walk the array, and keep the
/// **last** land cell whose `point_in_cell` answers yes.
///
/// `point_in_cell` itself is the shipped `find_terrain_poly`, deliberately — an oracle must
/// consult the same geometry the code under test consults. What is
/// independently derived here is the array order and the last-match rule, which is where the
/// answer on a line actually comes from; the absolute pin in
/// [`the_container_rule_disagrees_with_a_floor_only_on_a_line`] is what covers the polygon test.
fn client_container_cell(
    land: &dyn LandSource,
    base: CellId,
    center: Vec3,
    radius: f32,
) -> Option<CellId> {
    let mut container = None;
    for cid in client_cell_array(base, center, radius) {
        let Some(block) = land.landblock(cid.landblock()) else {
            continue;
        };
        let p = center.sub(landdefs::get_block_offset(base, cid));
        if block.find_terrain_poly(cid.index(), p).is_some() {
            container = Some(cid);
        }
    }
    container
}

fn floor_cell(base: CellId, center: Vec3) -> CellId {
    landdefs::get_outside_cell_id(base, center)
}

/// A cell id as the `(x & 7, y & 7)` pair masks it down to —
/// what is actually handed.
fn sq(id: CellId) -> (i32, i32) {
    let (x, y) = landdefs::gid_to_lcoord(id).expect("an outdoor cell id");
    (x & 7, y & 7)
}

// =================================================================================================
// 1. The oracle, calibrated in both directions
// =================================================================================================

/// **§7.8 and the reverse calibration.** An oracle that can only produce one answer is not an
/// oracle. This asserts both halves as literals derived by hand:
///
/// * four metres inside a cell the container rule **is** a floor — `x = 100.0` is `floor 4`, and
///   `lo = 23.5 < 4.0` is false and `4.0 < hi = 0.5` is false, so the array holds one cell and the
///   last match is the only match;
/// * on the line `x = 96.0` the array is `[(4, cy), (3, cy)]` — `local.x` is exactly `0.0`, which
///   is `< 0.5` — both contain the point because `point_in_poly2D`'s reject is `v > 0`, and the
///   last match is `(3, cy)`.
///
/// The block's south-west cell coordinate is `0xA9 * 8 = 1352` on x and `0xB4 * 8 = 1440` on y, so
/// those two cells are global `(1356, 1444)` and `(1355, 1444)`, i.e. SqCoords `(4, 4)` and
/// `(3, 4)`. Every one of those numbers is arithmetic on this file's own constants.
#[test]
fn the_container_rule_disagrees_with_a_floor_only_on_a_line() {
    let land = flat_land();
    let base = HOME.cell(1);
    let r = 0.5_f32;

    // Inside a cell: one cell in the array, and it is the floor.
    let inside = Vec3::new(100.0, 100.0, GROUND);
    let arr = client_cell_array(base, inside, r);
    assert_eq!(
        arr.len(),
        1,
        "four metres inside a cell the array is one cell: {arr:?}"
    );
    let got = client_container_cell(&*land, base, inside, r).expect("a container");
    assert_eq!(
        got,
        floor_cell(base, inside),
        "inside a cell the container rule is a floor"
    );
    assert_eq!(
        sq(got),
        (4, 4),
        "and the SqCoord is the one hand arithmetic gives"
    );

    // On the x line: two cells, and the container is the *lower* one.
    let on_line = Vec3::new(96.0, 100.0, GROUND);
    let arr = client_cell_array(base, on_line, r);
    assert_eq!(
        arr.iter().map(|c| sq(*c)).collect::<Vec<_>>(),
        vec![(4, 4), (3, 4)],
        "on the line the floor cell is added first and its west neighbour second"
    );
    let got = client_container_cell(&*land, base, on_line, r).expect("a container");
    assert_eq!(
        sq(got),
        (3, 4),
        "the last match wins, and it is the west cell"
    );
    assert_eq!(
        sq(floor_cell(base, on_line)),
        (4, 4),
        "while a floor names the east one"
    );
    assert_ne!(
        got,
        floor_cell(base, on_line),
        "the oracle must be able to disagree with a floor, or a run of agreements means nothing"
    );

    // And the same on y, so that neither axis is carrying the result alone.
    let on_y = Vec3::new(100.0, 96.0, GROUND);
    let got = client_container_cell(&*land, base, on_y, r).expect("a container");
    assert_eq!(sq(got), (4, 3));
    assert_eq!(sq(floor_cell(base, on_y)), (4, 4));
}

// =================================================================================================
// 2. The row's premise: both directions of approach
// =================================================================================================

/// Where a walk of `steps` sub-steps of `step` from `start` came to rest, and what cell it
/// reported.
struct Rest {
    origin: Vec3,
    cell: CellId,
}

/// Walk a body from `start` by `steps` sub-steps of `step`, let it come to rest, and read back
/// what committed out of `sphere_path.curr_pos`.
fn walk_and_rest(start: Vec3, step: Vec3, steps: usize) -> Rest {
    let mut w = flat_world();
    let h = spawn(&mut w, 1, start);
    run(&mut w, 1.0, 30.0);
    walk(&mut w, h, step, steps);
    #[allow(clippy::cast_precision_loss)]
    let seconds = (steps as f64) / 30.0 + 1.0;
    run(&mut w, seconds, 30.0);
    let o = w.get(h).expect("live");
    Rest {
        origin: o.position.frame.origin,
        cell: o.position.cell,
    }
}

/// Behaviour: physics.cells.a-body-resting-on-a-cell-line-reports-the-same-cell-from-both-directions
/// **The row, put directly.** Two walks that end on the same point from opposite sides.
///
/// The exact-rest premise is asserted before anything is concluded: 0.5 m sub-steps from an
/// exactly representable start accumulate exactly in `f32`, and `adjust_offset`'s projection onto
/// a `(0, 0, 1)` contact plane leaves a horizontal offset untouched — so both bodies must be at
/// `x = 96.0` **bit for bit**, or the two arms are not standing at the same place and the
/// comparison is void.
#[test]
fn a_walk_resting_on_a_24_m_line_reports_the_same_cell_from_both_directions() {
    let base = HOME.cell(1);
    let land = flat_land();

    let east = walk_and_rest(Vec3::new(93.0, 100.0, GROUND), Vec3::new(0.5, 0.0, 0.0), 6);
    let west = walk_and_rest(Vec3::new(99.0, 100.0, GROUND), Vec3::new(-0.5, 0.0, 0.0), 6);

    // Premise: both really rest on the line, and they really approached from opposite sides.
    for (name, r) in [("east-bound", &east), ("west-bound", &west)] {
        assert_eq!(
            r.origin.x, 96.0,
            "the {name} walk must rest exactly on the 4 * 24 line; it rests at {:?}",
            r.origin
        );
        assert_eq!(
            r.origin.y, 100.0,
            "and must not have drifted on y: {:?}",
            r.origin
        );
    }

    eprintln!(
        "x-line: east-bound rests at {:?} in cell {:?} (SqCoord {:?}); west-bound at {:?} \
         in cell {:?} (SqCoord {:?}); a floor of the same point says {:?}",
        east.origin,
        east.cell,
        sq(east.cell),
        west.origin,
        west.cell,
        sq(west.cell),
        sq(floor_cell(base, east.origin))
    );

    // The finding. The row says this pair should differ; it does not.
    assert_eq!(
        east.cell, west.cell,
        "the reported cell is a function of the resting position, not of the approach"
    );
    // And it is the client's own container cell, not a floor.
    let want = client_container_cell(&*land, base, east.origin, 0.5).expect("a container");
    assert_eq!(east.cell, want, "and it is `find_cell_list`'s container");
    assert_eq!(sq(east.cell), (3, 4), "which on this line is the west cell");
    assert_ne!(
        east.cell,
        floor_cell(base, east.origin),
        "the station is only interesting because a floor answers differently here"
    );

    let e2 = walk_and_rest(Vec3::new(97.0, 100.0, GROUND), Vec3::new(0.5, 0.0, 0.0), 6);
    let w2 = walk_and_rest(
        Vec3::new(103.0, 100.0, GROUND),
        Vec3::new(-0.5, 0.0, 0.0),
        6,
    );
    assert_eq!(e2.origin.x, 100.0, "{:?}", e2.origin);
    assert_eq!(w2.origin.x, 100.0, "{:?}", w2.origin);
    assert_eq!(
        e2.cell, w2.cell,
        "four metres inside a cell the two directions also agree"
    );
    assert_eq!(
        e2.cell,
        floor_cell(base, e2.origin),
        "and there the container rule and a floor are the same answer"
    );
    assert_eq!(sq(e2.cell), (4, 4));
}

/// The y axis and the corner, because a single axis cannot show that the tie-break is the array
/// order rather than a coincidence of x.
///
/// At the corner `(96.0, 96.0)` both `local.x` and `local.y` are `0.0`, so
/// `check_add_cell_boundary`'s second block adds `(gx-1, gy)` then `(gx-1, gy-1)`, and the last
/// block adds `(gx, gy-1)` — array `[(4,4), (3,4), (3,3), (4,3)]`. All four contain the point, so
/// the container is the **last** of them, `(4, 3)`. That is a different answer from either
/// single-axis case and from a floor, which is what makes the corner the discriminating station.
#[test]
fn the_same_holds_on_the_y_line_and_in_a_corner() {
    let base = HOME.cell(1);
    let land = flat_land();

    let north = walk_and_rest(Vec3::new(100.0, 93.0, GROUND), Vec3::new(0.0, 0.5, 0.0), 6);
    let south = walk_and_rest(Vec3::new(100.0, 99.0, GROUND), Vec3::new(0.0, -0.5, 0.0), 6);
    assert_eq!(north.origin.y, 96.0, "{:?}", north.origin);
    assert_eq!(south.origin.y, 96.0, "{:?}", south.origin);
    assert_eq!(
        north.cell, south.cell,
        "the y line agrees from both directions too"
    );
    assert_eq!(sq(north.cell), (4, 3), "and names the south cell");
    assert_ne!(north.cell, floor_cell(base, north.origin));

    // The corner, from all four diagonals.
    let corner = [
        (
            "from the south-west",
            Vec3::new(93.0, 93.0, GROUND),
            Vec3::new(0.5, 0.5, 0.0),
        ),
        (
            "from the south-east",
            Vec3::new(99.0, 93.0, GROUND),
            Vec3::new(-0.5, 0.5, 0.0),
        ),
        (
            "from the north-west",
            Vec3::new(93.0, 99.0, GROUND),
            Vec3::new(0.5, -0.5, 0.0),
        ),
        (
            "from the north-east",
            Vec3::new(99.0, 99.0, GROUND),
            Vec3::new(-0.5, -0.5, 0.0),
        ),
    ];
    let mut seen: Vec<(&str, CellId)> = Vec::new();
    for (name, start, step) in corner {
        let r = walk_and_rest(start, step, 6);
        assert_eq!(
            (r.origin.x, r.origin.y),
            (96.0, 96.0),
            "{name}: must rest exactly on the corner, not {:?}",
            r.origin
        );
        seen.push((name, r.cell));
    }
    eprintln!(
        "corner: {}",
        seen.iter()
            .map(|(n, c)| format!("{n} -> {:?}", sq(*c)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let first = seen[0].1;
    for (name, c) in &seen {
        assert_eq!(*c, first, "{name} disagreed with the south-west approach");
    }
    let want =
        client_container_cell(&*land, base, Vec3::new(96.0, 96.0, GROUND), 0.5).expect("container");
    assert_eq!(
        first, want,
        "the corner's container is the array's last match"
    );
    assert_eq!(
        sq(first),
        (4, 3),
        "which is the diagonal-free south cell, not the diagonal"
    );
    assert_ne!(first, floor_cell(base, Vec3::new(96.0, 96.0, GROUND)));
}

// =================================================================================================
// 3b. The array itself, in order — because the last-match rule is a claim about an order
// =================================================================================================

/// The container is *the last match in the array*, so the array's **contents and order** are
/// load-bearing and not an implementation detail. This asserts the shipped
/// `CellResolver::find_cell_list` builds exactly the sequence this file's oracle derives, at five
/// stations chosen to exercise each of `check_add_cell_boundary`'s four branches:
///
/// | station | `local` | branches taken |
/// |---|---|---|
/// | `(100.0, 100.0)` | `(4.0, 4.0)` | none — one cell |
/// | `(96.0, 100.0)` | `(0.0, 4.0)` | `x < hi` |
/// | `(100.0, 96.0)` | `(4.0, 0.0)` | `y < hi` |
/// | `(96.0, 96.0)` | `(0.0, 0.0)` | `x < hi`, `y < hi` and the `-1,-1` diagonal |
/// | `(95.75, 95.75)` | `(23.75, 23.75)` | `x > lo`, `y > lo` and the `+1,+1` diagonal |
///
/// The last row is why this test exists rather than being folded into the container assertions:
/// the `+1` neighbours are **added and never win**, because a point that has not crossed a cell's
/// far edge is not inside the cell beyond it. Nothing that only reads the container can see them,
/// and they are still the client's array.
#[test]
fn the_cell_array_is_the_clients_own_array_in_the_clients_own_order() {
    use dereth_physics::cell::{CellArray, CellResolver};

    let land = flat_land();
    let base = HOME.cell(1);
    let r = globals::VIEWER_SPHERE_RADIUS;
    let resolver = CellResolver::new(&*land);

    let stations = [
        ("inside a cell", Vec3::new(100.0, 100.0, GROUND)),
        ("on the x line", Vec3::new(96.0, 100.0, GROUND)),
        ("on the y line", Vec3::new(100.0, 96.0, GROUND)),
        ("in the corner", Vec3::new(96.0, 96.0, GROUND)),
        ("just short of both lines", Vec3::new(95.75, 95.75, GROUND)),
    ];
    let mut widths = Vec::new();
    for (name, centre) in stations {
        let pos = Position::new(base, Frame::new(centre, Quat::IDENTITY));
        let mut arr = CellArray::default();
        let mut hits_interior = false;
        resolver.find_cell_list(
            &pos,
            &[Sphere::new(centre, r)],
            &mut arr,
            false,
            &mut hits_interior,
        );
        let got: Vec<CellId> = arr.cells.iter().map(|c| c.cell_id).collect();
        let want = client_cell_array(base, centre, r);
        assert_eq!(
            got.iter().map(|c| sq(*c)).collect::<Vec<_>>(),
            want.iter().map(|c| sq(*c)).collect::<Vec<_>>(),
            "{name}: the shipped array and the one derived from the client's rule differ"
        );
        widths.push(format!("{name} -> {} cell(s)", got.len()));
    }
    eprintln!("arrays: {}", widths.join(", "));
    // The premise: the five stations really do exercise different branch combinations, so a
    // single wrong branch cannot hide behind four identical arrays.
    let lens: Vec<usize> = stations
        .iter()
        .map(|(_, c)| client_cell_array(base, *c, r).len())
        .collect();
    assert_eq!(
        lens,
        vec![1, 2, 2, 4, 4],
        "the five stations build arrays of five known widths"
    );
}

// =================================================================================================
// 4. The subject the row names: 's own sweep
// =================================================================================================

/// One frame of the camera sweep, exactly as `CameraControl::update_viewer` drives it: the 0.3 m
/// `viewer_sphere`, object-info state `0x5C`, from the pivot to the sought eye.
fn viewer_sweep(
    w: &mut PhysicsWorld,
    player: PhysHandle,
    from: Vec3,
    to: Vec3,
) -> (Vec3, CellId, Option<CellId>) {
    let cell = w
        .get(player)
        .and_then(|o| o.cell)
        .expect("the player is in a cell");
    let f = Position::new(cell, Frame::new(from, Quat::IDENTITY));
    let t = Position::new(cell, Frame::new(to, Quat::IDENTITY));
    let tr = w
        .sweep_sphere(
            player,
            globals::VIEWER_OBJECT_INFO_STATE,
            Sphere::new(Vec3::ZERO, globals::VIEWER_SPHERE_RADIUS),
            cell,
            &f,
            &t,
        )
        .expect("the viewer sweep finds a valid position");
    let p = tr.sphere_path.curr_pos;
    (p.frame.origin, p.cell, tr.sphere_path.curr_cell)
}

/// The viewer sweep reports the container cell from both directions.
#[test]
fn the_viewer_sweep_reports_the_container_cell_from_both_directions() {
    let land = flat_land();
    let mut w = PhysicsWorld::new(Arc::clone(&land));
    let base = HOME.cell(1);
    // The pivot object stands well inside a cell so that its own cell is never in question.
    let player = spawn(&mut w, 1, Vec3::new(100.0, 100.0, GROUND));
    run(&mut w, 1.0, 30.0);

    let eye = GROUND + 1.5;
    let (o_e, cell_e, curr_e) = viewer_sweep(
        &mut w,
        player,
        Vec3::new(95.75, 100.0, eye),
        Vec3::new(96.0, 100.0, eye),
    );
    let (o_w, cell_w, curr_w) = viewer_sweep(
        &mut w,
        player,
        Vec3::new(96.25, 100.0, eye),
        Vec3::new(96.0, 100.0, eye),
    );

    // Premise: both sweeps reached the line exactly, and neither was blocked short of it.
    assert_eq!(o_e.x, 96.0, "the west-to-east sweep stopped at {o_e:?}");
    assert_eq!(o_w.x, 96.0, "the east-to-west sweep stopped at {o_w:?}");

    eprintln!(
        "viewer sweep: west-to-east -> {:?} / curr_cell {:?}; east-to-west -> {:?} / \
         curr_cell {:?}; a floor says {:?}",
        sq(cell_e),
        curr_e.map(sq),
        sq(cell_w),
        curr_w.map(sq),
        sq(floor_cell(base, o_e))
    );

    assert_eq!(
        cell_e, cell_w,
        "`curr_pos.cell` is the same from both directions"
    );
    assert_eq!(curr_e, curr_w, "and so is `curr_cell`");
    assert_eq!(
        curr_e,
        Some(cell_e),
        "the two are one answer, as `validate_transition` writes them"
    );

    // The viewer sphere is 0.3 m, so `hi = 0.3` rather than the body's `0.5` — a different array
    // threshold reaching the same conclusion, which is what shows the rule and not the fixture is
    // doing the work.
    let want = client_container_cell(&*land, base, o_e, globals::VIEWER_SPHERE_RADIUS)
        .expect("a container");
    assert_eq!(
        cell_e, want,
        "and it is `find_cell_list`'s container for the 0.3 m sphere"
    );
    assert_eq!(sq(cell_e), (3, 4));
    assert_ne!(
        cell_e,
        floor_cell(base, o_e),
        "a floor of the same point says otherwise"
    );

    let (o2, c2, _) = viewer_sweep(
        &mut w,
        player,
        Vec3::new(99.75, 100.0, eye),
        Vec3::new(100.0, 100.0, eye),
    );
    let (o3, c3, _) = viewer_sweep(
        &mut w,
        player,
        Vec3::new(100.25, 100.0, eye),
        Vec3::new(100.0, 100.0, eye),
    );
    assert_eq!(o2.x, 100.0);
    assert_eq!(o3.x, 100.0);
    assert_eq!(c2, c3, "four metres inside a cell the two directions agree");
    assert_eq!(c2, floor_cell(base, o2), "and agree with a floor");

    // **The near-miss station, stated rather than avoided.** A 6 m sweep aimed at the same line
    // overshoots it in `f32` and therefore reports the cell **east** of it. This control makes
    // that numerical fixture hazard explicit.
    let (o4, c4, _) = viewer_sweep(
        &mut w,
        player,
        Vec3::new(90.0, 100.0, eye),
        Vec3::new(96.0, 100.0, eye),
    );
    eprintln!(
        "near miss: a 6 m viewer sweep aimed at x = 96.0 lands at {} and reports {:?}",
        o4.x,
        sq(c4)
    );
    assert!(
        o4.x > 96.0 && o4.x - 96.0 < 1e-3,
        "the 6 m sweep lands just past the line: {o4:?}"
    );
    assert_eq!(
        sq(c4),
        (4, 4),
        "and a point past the line is in the cell past the line"
    );
    assert_eq!(c4, floor_cell(base, o4));
}

/// **A denominator, and the count that differs from a floor stated rather than assumed.**
///
/// Every 0.25 m from `x = 90.0` to `x = 102.0` at `y = 100.0`, plus the exact line, swept onto
/// from the **west** and from the **east**: the two directions must agree at every station, the
/// answer must be `find_cell_list`'s container at every station, and the number of stations where
/// a floor gives a different answer is reported.
#[test]
fn a_census_of_resting_positions_across_a_cell_line() {
    let land = flat_land();
    let mut w = PhysicsWorld::new(Arc::clone(&land));
    let base = HOME.cell(1);
    let player = spawn(&mut w, 1, Vec3::new(100.0, 100.0, GROUND));
    run(&mut w, 1.0, 30.0);
    let eye = GROUND + 1.5;

    let mut stations = 0_usize;
    let mut direction_disagreements = 0_usize;
    let mut oracle_disagreements = 0_usize;
    let mut floor_disagreements = 0_usize;
    let mut on_the_line = 0_usize;

    for i in 0..=48_u32 {
        let x = 90.0 + f32::from(u16::try_from(i).expect("small")) * 0.25;
        let target = Vec3::new(x, 100.0, eye);
        let (o_e, cell_e, _) =
            viewer_sweep(&mut w, player, Vec3::new(x - 0.25, 100.0, eye), target);
        let (o_w, cell_w, _) =
            viewer_sweep(&mut w, player, Vec3::new(x + 0.25, 100.0, eye), target);
        assert_eq!(o_e.x, x, "the west-to-east sweep did not reach {x}");
        assert_eq!(o_w.x, x, "the east-to-west sweep did not reach {x}");
        stations += 1;
        if cell_e != cell_w {
            direction_disagreements += 1;
        }
        let want = client_container_cell(&*land, base, o_e, globals::VIEWER_SPHERE_RADIUS)
            .expect("a container");
        if cell_e != want {
            oracle_disagreements += 1;
        }
        if cell_e != floor_cell(base, o_e) {
            floor_disagreements += 1;
        }
        if (x / CELL).fract() == 0.0 {
            on_the_line += 1;
        }
    }

    eprintln!(
        "census: {stations} station(s) from x = 90.00 to 102.00 at 0.25 m, each swept from \
         both sides; {direction_disagreements} differ by direction, {oracle_disagreements} differ \
         from `find_cell_list`'s container, {floor_disagreements} differ from a floor \
         ({on_the_line} station(s) sit exactly on a 24 m line)"
    );

    assert_eq!(stations, 49, "the denominator");
    assert_eq!(
        on_the_line, 1,
        "exactly one station of the sweep is exactly on a cell line"
    );
    assert_eq!(
        direction_disagreements, 0,
        "no station's answer depends on which side it was approached from"
    );
    assert_eq!(
        oracle_disagreements, 0,
        "every station is `find_cell_list`'s container cell"
    );
    // The calibration that stops the two zeros above being a silence: the container rule is not a
    // floor, and this says at how many of the 49 it is not.
    assert_eq!(
        floor_disagreements, 1,
        "exactly the on-the-line station may differ from a floor; a zero here would mean the \
         census never reached the case it exists to measure"
    );
}

use dereth_physics::cell::{CellArray, CellResolver};

const BODY_RADIUS: f32 = 0.48;

fn body_geometry() -> Arc<SetupGeometry> {
    crate::common::physics_fixture::sphere_body(BODY_RADIUS, 0.3, 2.0 * BODY_RADIUS)
}

fn transition_derivation(
    land: &dyn LandSource,
    base: CellId,
    centre: Vec3,
    radius: f32,
) -> (Vec<CellId>, Option<CellId>) {
    let resolver = CellResolver::new(land);
    let pos = Position::new(base, Frame::new(centre, Quat::IDENTITY));
    let mut arr = CellArray::default();
    let mut hits_interior = false;
    let got = resolver.find_cell_list(
        &pos,
        &[Sphere::new(centre, radius)],
        &mut arr,
        true,
        &mut hits_interior,
    );
    (
        arr.cells.iter().map(|c| c.cell_id).collect(),
        got.map(|c| c.id()),
    )
}

fn floor_derivation(base: CellId, centre: Vec3) -> CellId {
    landdefs::get_outside_cell_id(base, centre)
}

/// **The row's question, answered.** A 0.48 m sphere centred exactly on `x = 96.0` (`4 * 24`):
///
/// * the array is `[(4, 4), (3, 4)]` — `local.x` is exactly `0.0`, `0.0 < 0.48` is the strict `<`
///   so the west neighbour is added **after** the floor cell;
/// * both cells contain the point, because `point_in_poly2D`'s accept is `v <= 0`;
/// * the container is therefore `(3, 4)`, the **lower** cell;
/// * while the floor derivation names `(4, 4)`.
///
/// The controls at `95.99` and `96.01` are the discriminator: at both the array still holds **two**
/// cells — the boundary band is `radius`-wide on each side — but only one of them contains the
/// point, so the two derivations **agree**. The disagreement is a property of the exact line.
#[test]
fn on_the_x_line_the_transition_takes_the_lower_cell_and_a_floor_takes_the_upper() {
    let land = flat_land();
    let base = HOME.cell(1);

    // Four metres inside a cell: one cell, and the two derivations are the same function.
    let inside = Vec3::new(100.0, 100.0, GROUND);
    let (arr, got) = transition_derivation(&*land, base, inside, BODY_RADIUS);
    assert_eq!(arr.len(), 1, "inside a cell the array is one cell: {arr:?}");
    assert_eq!(sq(got.expect("a container")), (4, 4));
    assert_eq!(got.expect("a container"), floor_derivation(base, inside));

    // Exactly on the line.
    let on_line = Vec3::new(96.0, 100.0, GROUND);
    assert_eq!(
        on_line.x,
        4.0 * CELL,
        "the station is the exact cell line, not near it"
    );
    let (arr, got) = transition_derivation(&*land, base, on_line, BODY_RADIUS);
    assert_eq!(
        arr.iter().map(|c| sq(*c)).collect::<Vec<_>>(),
        vec![(4, 4), (3, 4)],
        "the cell-list build seeds the floor cell and its `local.x < radius` adds the west one \
         after it"
    );
    let got = got.expect("a container");
    assert_eq!(
        sq(got),
        (3, 4),
        "the last match wins: retail commits the LOWER cell"
    );
    assert_eq!(
        sq(floor_derivation(base, on_line)),
        (4, 4),
        "while a floor names the upper one"
    );
    assert_ne!(
        got,
        floor_derivation(base, on_line),
        "the two derivations disagree on the line"
    );

    // The controls, one hundredth of a metre either side.
    for (name, x, want) in [
        ("just west", 95.99_f32, (3, 4)),
        ("just east", 96.01_f32, (4, 4)),
    ] {
        let p = Vec3::new(x, 100.0, GROUND);
        let (arr, got) = transition_derivation(&*land, base, p, BODY_RADIUS);
        assert_eq!(
            arr.len(),
            2,
            "{name}: the boundary band still puts two cells in the array at {x}: {:?}",
            arr.iter().map(|c| sq(*c)).collect::<Vec<_>>()
        );
        let got = got.expect("a container");
        assert_eq!(
            sq(got),
            want,
            "{name}: only one of the two contains the point"
        );
        assert_eq!(
            got,
            floor_derivation(base, p),
            "{name}: off the exact line the two derivations agree, so the disagreement above is \
             the tie and not the neighbourhood"
        );
    }
}

/// Behaviour: physics.cells.a-body-resting-on-a-cell-line-reports-the-same-cell-from-both-directions
/// A placement on the line commits the transitions cell not the floors.
#[test]
fn a_placement_on_the_line_commits_the_transitions_cell_not_the_floors() {
    let land = flat_land();
    let mut w = PhysicsWorld::new(land);

    fn place(w: &mut PhysicsWorld, id: u32, x: f32, y: f32) -> (PhysHandle, CellId, CellId) {
        let h = w.create(ObjectId(id), body_geometry(), true);
        let origin = Vec3::new(x, y, GROUND + BODY_RADIUS);
        let pos = Position::new(HOME.cell(1), Frame::new(origin, Quat::IDENTITY));
        assert!(
            w.enter_world(h, &pos),
            "({x}, {y}) is a placeable destination"
        );
        let o = w.get(h).expect("live");
        (h, o.position.cell, floor_derivation(HOME.cell(1), origin))
    }

    let (_, committed, floored) = place(&mut w, 1, 96.0, 100.0);
    eprintln!(
        "placement at x = 96.0: committed {committed:?} = {:?}, floor {floored:?} = {:?}",
        sq(committed),
        sq(floored)
    );
    assert_eq!(
        sq(committed),
        (3, 4),
        "the placement commits the LOWER cell"
    );
    assert_eq!(
        sq(floored),
        (4, 4),
        "the floor derivation names the upper one"
    );
    assert_ne!(
        committed, floored,
        "this is the disagreement, reproduced through a placement"
    );

    // The bracket, the same placement one hundredth of a metre east.
    let (_, committed, floored) = place(&mut w, 2, 96.01, 100.0);
    assert_eq!(
        committed, floored,
        "off the line a placement and a floor name the same cell, so the station above is \
         measuring the tie"
    );
    assert_eq!(sq(committed), (4, 4));
}

use crate::common::physics_fixture::{flat_land, flat_world, player_geometry};
