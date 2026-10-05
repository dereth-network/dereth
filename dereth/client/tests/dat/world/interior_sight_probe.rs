//! A sight probe crossing an interior wall is blocked, and a short probe inside the same room is
//! clear. Every station asserts both directions, because a blocked-only test passes on an
//! implementation that blocks everything. The probe's endpoints are landblock-space origins: a
//! cell's physics polygons are in its own cell-local space, and `EnvCellGeometry::frame` maps
//! between them. A probe built in the wrong space passes 175 m from the room and reads clear.
//! This covers interior walls only, not bodies that block a line of sight.
//!
//! Fixture: interior rooms, walls and BSPs from `client_cell_1.dat` and `client_portal.dat` in
//! `$DERETH_TEST_DAT_DIR`, found rather than named, swept through the physics transition path.

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_physics::cell::Cell;
use dereth_physics::source::EnvCellGeometry;
use dereth_physics::transition::Transition;
use dereth_physics::{LandSource, PhysicsWorld, SetupGeometry};
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};
use dereth_world_data::land_source::DatLandSource;

/// The cell the wrong-space reproduction uses. Everything about it — bounds, walls, frame — is
/// read from the dat.
const MEASURED_CELL: CellId = CellId(0x0007_0103);

/// The five interior landblocks the corpus tests sweep. `0x8602` is the training academy; the rest are ordinary dungeons, and `0x0007` is where `MEASURED_CELL` lives.
const BLOCKS: [u16; 5] = [0x0007, 0x8602, 0x016C, 0x00DA, 0x01E4];

/// `SetupGeometry::dummy`'s sphere sits this far above the object's own position, so *this* is the
/// point that has to be inside a room, not the position itself.
const EYE: f32 = dereth_physics::globals::DUMMY_SPHERE_CENTER_Z;

/// The retail store, or **fail**: a missing install is a failure, never a skip.
fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

fn source(store: &Arc<RetailDatStore>) -> Option<Arc<DatLandSource>> {
    let region = dereth_client_runtime::landblock::load_region(store).ok()?;
    let src = Arc::new(DatLandSource::new(Arc::clone(store), &region).ok()?);
    for b in BLOCKS {
        src.load_block_cells(LandblockId(b));
    }
    Some(src)
}

/// A room described by **its own polygon bounds**, in the cell's own space. Used as a position
/// origin without the cell frame, it reproduces the wrong-space probe.
fn bounds(g: &EnvCellGeometry) -> Option<(Vec3, Vec3)> {
    if g.physics_polygons.is_empty() {
        return None;
    }
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for p in &g.physics_polygons {
        for v in &p.vertices {
            lo = Vec3::new(lo.x.min(v.x), lo.y.min(v.y), lo.z.min(v.z));
            hi = Vec3::new(hi.x.max(v.x), hi.y.max(v.y), hi.z.max(v.z));
        }
    }
    let half = Vec3::new(
        (hi.x - lo.x) * 0.5,
        (hi.y - lo.y) * 0.5,
        (hi.z - lo.z) * 0.5,
    );
    Some((Vec3::new(lo.x + half.x, lo.y + half.y, lo.z + half.z), half))
}

/// Apply the cell frame: cell-local -> landblock, which is the space a `Position` origin is in.
fn to_landblock(g: &EnvCellGeometry, v: Vec3) -> Vec3 {
    dereth_physics::math::localtoglobal(&g.frame, v)
}

/// Reduce the complete sight stepper to its one physics operation: a missile-flagged, gravity-free
/// transition. Returning the `Transition` keeps its collision counters available to the tests.
///
/// `from` and `to` are **landblock**-space origins, which is what a `Position` carries.
fn probe(src: &Arc<DatLandSource>, cell: CellId, from: Vec3, to: Vec3) -> Option<Transition> {
    let mut w = PhysicsWorld::new(Arc::clone(src) as Arc<dyn LandSource>);
    let h = w.create(ObjectId(0), Arc::new(SetupGeometry::dummy()), true);
    w.enter_cell(h, cell);
    let start = Position::new(cell, Frame::new(from, Quat::IDENTITY));
    let target = Position::new(cell, Frame::new(to, Quat::IDENTITY));
    if let Some(o) = w.get_mut(h) {
        o.position = start;
        // The whole point: a missile transition. `sight.rs` sets both of these and so must this.
        o.state.set_missile(true);
        o.state.set_gravity(false);
        o.transient_state.set_active_bit(true);
    }
    w.calc_cross_cells(h, false);
    w.transition(h, &start, &target, false)
}

/// Did the sweep report the environment as an obstruction? This is the field consumed by the
/// complete sight query, not the `Option`: `None` is *no opinion*, and a probe that read the
/// `Option` as "clear" would read clear whatever the geometry said.
fn blocked(t: &Option<Transition>) -> Option<bool> {
    t.as_ref()
        .map(|t| t.collision_info.collided_with_environment)
}

/// Each room candidate in [`BLOCKS`] with an index from `0x0100` through `0x017f`, sufficient
/// half extents, and an eye point inside the cell, as
/// `(cell, geometry, cell-local centre, cell-local half extent)`.
///
/// The cell-membership test is on the **eye** point rather than on the position, and it is not a
/// nicety: a sloping ramp cell's bounding-box centre sits on its own
/// ceiling, so the probe's sphere starts outside the cell and the sweep — correctly — tracks no
/// cell at all. Eight of `0x01E4`'s stair cells are exactly that, and including them would have
/// made this test assert against a subject that was never in a room.
fn rooms(src: &Arc<DatLandSource>) -> Vec<(CellId, Arc<EnvCellGeometry>, Vec3, Vec3)> {
    let mut out = Vec::new();
    for lb in BLOCKS {
        for index in 0x0100..0x0180u32 {
            let cell = CellId((u32::from(lb) << 16) | index);
            let Some(g) = LandSource::env_cell(src.as_ref(), cell) else {
                continue;
            };
            let Some((centre, half)) = bounds(&g) else {
                continue;
            };
            // Big enough that a 0.8 m calibration probe is genuinely inside it.
            if half.x.min(half.y).min(half.z) < 1.5 {
                continue;
            }
            let w = to_landblock(&g, centre);
            let c = Cell::Env {
                id: cell,
                geom: Arc::clone(&g),
            };
            if !c.point_in_cell(Vec3::new(w.x, w.y, w.z + EYE)) {
                continue;
            }
            out.push((cell, g, centre, half));
        }
    }
    out
}

/// Wall polygons of a cell, as cell-local centroids: near-vertical, and far enough from `centre`
/// that a probe aimed at one has somewhere to travel.
///
/// Near-vertical because a floor is not a wall: a swept sphere aimed downward at a walkable floor
/// polygon lands on it through walkable-surface handling, which is a different question from
/// wall blocking and would make this test about walkability.
fn wall_centroids(g: &EnvCellGeometry, centre: Vec3) -> Vec<Vec3> {
    g.physics_polygons
        .iter()
        .filter(|p| p.vertices.len() >= 3 && p.plane.normal.z.abs() <= 0.5)
        .filter_map(|p| {
            #[allow(clippy::cast_precision_loss)]
            let n = p.vertices.len() as f32;
            let mut c = Vec3::ZERO;
            for v in &p.vertices {
                c = Vec3::new(c.x + v.x, c.y + v.y, c.z + v.z);
            }
            let c = Vec3::new(c.x / n, c.y / n, c.z / n);
            let d = Vec3::new(c.x - centre.x, c.y - centre.y, c.z - centre.z);
            (d.x.mul_add(d.x, d.y.mul_add(d.y, d.z * d.z)).sqrt() >= 1.0).then_some(c)
        })
        .collect()
}

/// The endpoint of a probe aimed just past `target`, in the same space as `centre`.
fn just_past(centre: Vec3, target: Vec3) -> Vec3 {
    let d = Vec3::new(
        target.x - centre.x,
        target.y - centre.y,
        target.z - centre.z,
    );
    Vec3::new(
        centre.x + d.x * 1.05,
        centre.y + d.y * 1.05,
        centre.z + d.z * 1.05,
    )
}

// ---------------------------------------------------------------------------------------------
// 1. The wrong-space probe, and the same probe in landblock space
// ---------------------------------------------------------------------------------------------

/// **A probe built in cell-local space reads clear; the same probe in landblock space blocks.**
///
/// Oracle: `client_cell_1.dat`. The source/decode and measured-cell lookups are the only skips in
/// this file; the corpus tests below fail when their source is unavailable. For this measured cell,
/// the polygon-space centre is asserted outside the room and its 12 m probe is asserted clear. The
/// short probe in that wrong space is printed only. After frame mapping, the short probe is asserted
/// clear and the same 12 m crossing is asserted blocked.
#[test]
fn a_probe_in_cell_local_space_reads_clear_where_the_landblock_space_probe_blocks() {
    let store = store();
    let Some(src) = source(&store) else {
        eprintln!("skipping: the region does not decode");
        return;
    };
    let Some(g) = LandSource::env_cell(src.as_ref(), MEASURED_CELL) else {
        eprintln!("skipping: {MEASURED_CELL:?} is not resident");
        return;
    };
    let (centre, half) = bounds(&g).expect("the measured cell has polygons");
    let c = Cell::Env {
        id: MEASURED_CELL,
        geom: Arc::clone(&g),
    };
    let mapped = to_landblock(&g, centre);
    println!(
        "{MEASURED_CELL:?}: {} polygons, half {half:?}, cell frame {:?}\n  \
         polygon-space centre {centre:?} -> landblock {mapped:?}",
        g.physics_polygons.len(),
        g.frame,
    );

    // --- the space itself -------------------------------------------------------------------
    assert!(
        !c.point_in_cell(centre),
        "the polygon-space centre {centre:?} is inside the cell when read as a landblock \
         coordinate. If the dat has changed so that the two spaces coincide for this cell, this \
         test can no longer distinguish them: pick a cell whose frame is not near the origin."
    );
    assert!(
        c.point_in_cell(Vec3::new(mapped.x, mapped.y, mapped.z + EYE)),
        "the centre taken through `interior-cell position transform` is NOT inside the cell. That is the mapping \
         itself failing, and every other assertion here rests on it."
    );

    // --- the probe in cell-local space ------------------------------------------------------
    let reach = (half.x.min(half.y) * 0.4).min(1.0);
    let their_clear = probe(
        &src,
        MEASURED_CELL,
        Vec3::new(centre.x - reach, centre.y, centre.z),
        Vec3::new(centre.x + reach, centre.y, centre.z),
    );
    let their_crossing = probe(
        &src,
        MEASURED_CELL,
        centre,
        Vec3::new(centre.x + 12.0, centre.y, centre.z),
    );
    println!(
        "  in cell-local space: clear {:?}, crossing-12m {:?}",
        blocked(&their_clear),
        blocked(&their_crossing)
    );
    assert_eq!(
        blocked(&their_crossing),
        Some(false),
        "the wrong-space probe no longer reads clear. It is the control that makes the \
         landblock-space case below meaningful."
    );
    // The wrong-space sweep still enters the interior branch and walks the tree; it simply does
    // so 175 m from the room.
    let t = their_crossing
        .as_ref()
        .expect("the mis-placed probe still positions");
    assert!(
        t.counters.env_bsp_walked >= 1,
        "the interior tree was walked even out there"
    );
    assert_eq!(
        t.counters.env_cells_without_bsp, 0,
        "and it was never a cell with no tree"
    );

    // --- the same probe, in landblock space --------------------------------------------------
    let up = |v: Vec3| to_landblock(&g, v);
    let fixed_clear = probe(
        &src,
        MEASURED_CELL,
        up(Vec3::new(centre.x - reach, centre.y, centre.z)),
        up(Vec3::new(centre.x + reach, centre.y, centre.z)),
    );
    let fixed_crossing = probe(
        &src,
        MEASURED_CELL,
        up(centre),
        up(Vec3::new(centre.x + 12.0, centre.y, centre.z)),
    );
    println!(
        "  in landblock space: clear {:?}, crossing-12m {:?}",
        blocked(&fixed_clear),
        blocked(&fixed_crossing)
    );

    // BOTH assertions: a blocked-only test passes on an implementation that blocks everything.
    assert_eq!(
        blocked(&fixed_clear),
        Some(false),
        "two points {reach} m either side of the room centre reported blocked. The crossing case \
         below proves nothing while this fails: an implementation that blocks every probe would \
         satisfy it."
    );
    assert_eq!(
        blocked(&fixed_crossing),
        Some(true),
        "a probe from the room centre to 12 m out was NOT blocked once its endpoints were in the \
         right space: an interior wall does not block sight."
    );

    // And it stops at the wall rather than somewhere arbitrary: the room is `half.x` to the wall
    // and the probe's sphere is `DUMMY_SPHERE_RADIUS`, so it rests tangent.
    let end = fixed_crossing
        .expect("positioned")
        .sphere_path
        .curr_pos
        .frame
        .origin;
    let travelled = math::hypotf(end.x - mapped.x, end.y - mapped.y);
    let expected = half.x - dereth_physics::globals::DUMMY_SPHERE_RADIUS;
    assert!(
        (travelled - expected).abs() < 0.05,
        "the probe stopped {travelled} m from the centre; the wall is {} m away and the sphere is \
         {} m, so it should rest at {expected} m",
        half.x,
        dereth_physics::globals::DUMMY_SPHERE_RADIUS,
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The acceptance line, over the corpus
// ---------------------------------------------------------------------------------------------

/// Behaviour: world.sight.a-probe-across-an-interior-wall-is-blocked
/// **The acceptance line.** A probe aimed at an interior wall is blocked; a short probe inside the
/// same room is not. Every filtered room candidate in the five queried landblocks is checked in
/// both directions, with no exceptions among that selected set.
///
/// Oracle: the retail dats. The rooms are found rather than named, the walls are the cells' own
/// near-vertical physics polygons, and the verdict is the current
/// `CollisionInfo::collided_with_environment` field consumed by the complete sight query.
#[test]
fn an_interior_probe_across_a_wall_blocks_and_a_short_one_does_not() {
    let store = store();
    let src = source(&store).expect("the landscape source from the retail dats");
    let rooms = rooms(&src);
    assert!(
        rooms.len() >= 300,
        "only {} interior rooms resolved from {BLOCKS:?}; this test is a corpus test and a corpus \
         this small means the dats or the loader changed, not that walls stopped blocking",
        rooms.len()
    );

    let mut clear_ok = 0usize;
    let mut clear_bad = Vec::new();
    let mut walls = 0usize;
    let mut wall_bad = Vec::new();
    for (cell, g, centre, half) in &rooms {
        let l2w = |v: Vec3| to_landblock(g, v);

        // --- the calibration: 0.8 m across the middle of the room, which must be clear ---------
        let r = (half.x.min(half.y) * 0.4).min(0.4);
        match blocked(&probe(
            &src,
            *cell,
            l2w(Vec3::new(centre.x - r, centre.y, centre.z)),
            l2w(Vec3::new(centre.x + r, centre.y, centre.z)),
        )) {
            Some(false) => clear_ok += 1,
            other => clear_bad.push((*cell, other)),
        }

        // --- the crossing: from the centre, just past a wall polygon's own centroid ------------
        for w in wall_centroids(g, *centre) {
            walls += 1;
            match blocked(&probe(
                &src,
                *cell,
                l2w(*centre),
                l2w(just_past(*centre, w)),
            )) {
                Some(true) => {}
                other => wall_bad.push((*cell, w, other)),
            }
        }
    }
    println!(
        "{} rooms: calibration clear {clear_ok}, not-clear {}\n{walls} wall shots: blocked {}, \
         not-blocked {}",
        rooms.len(),
        clear_bad.len(),
        walls - wall_bad.len(),
        wall_bad.len()
    );

    // BOTH directions, and the clear one first because it is what makes the other mean anything.
    assert!(
        clear_bad.is_empty(),
        "{} of {} rooms reported a short probe through their own middle as blocked (or had no \
         opinion): {:?}. Nothing below this line means anything while that is true — an \
         implementation that blocks every probe would satisfy the wall assertion.",
        clear_bad.len(),
        rooms.len(),
        &clear_bad[..clear_bad.len().min(6)]
    );
    assert!(
        walls >= 2000,
        "only {walls} wall polygons to shoot at; the corpus has thinned"
    );
    assert!(
        wall_bad.is_empty(),
        "{} of {walls} probes aimed at an interior wall were not blocked: {:?}",
        wall_bad.len(),
        &wall_bad[..wall_bad.len().min(6)]
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The collision counters: entered, walked, or entered with nothing to walk
// ---------------------------------------------------------------------------------------------

/// **The interior branch is entered, and it walks a tree.**
///
/// `CollisionCounters`' three env fields separate *never entered* (`env_cells_visited == 0`) from
/// *entered with nothing to walk* (`env_cells_without_bsp`) from *walked* (`env_bsp_walked`). This
/// checks them on retail dungeon geometry, so that a zero from one of them can be trusted.
///
/// Asserted on **both** probes, so that "walked and found nothing" is a state this test can see.
#[test]
fn the_interior_branch_is_entered_and_walks_the_tree() {
    let store = store();
    let src = source(&store).expect("the landscape source from the retail dats");
    let rooms = rooms(&src);
    assert!(!rooms.is_empty(), "no rooms resolved");

    let (mut clear_walked, mut cross_walked, mut nobsp, mut checked) = (0u32, 0u32, 0u32, 0u32);
    for (cell, g, centre, half) in rooms.iter().take(40) {
        let l2w = |v: Vec3| to_landblock(g, v);
        let r = (half.x.min(half.y) * 0.4).min(0.4);
        let clear = probe(
            &src,
            *cell,
            l2w(Vec3::new(centre.x - r, centre.y, centre.z)),
            l2w(Vec3::new(centre.x + r, centre.y, centre.z)),
        )
        .expect("the calibration probe positions");
        let Some(w) = wall_centroids(g, *centre).into_iter().next() else {
            continue;
        };
        let cross = probe(&src, *cell, l2w(*centre), l2w(just_past(*centre, w)))
            .expect("the crossing probe positions");
        checked += 1;

        // A clear probe is the interesting one: it is "the branch ran and found nothing", which
        // without the counters would be indistinguishable from "the branch never ran".
        assert!(
            clear.counters.env_cells_visited > 0,
            "{cell:?}: the calibration probe never entered `interior-cell collision lookup`"
        );
        assert_eq!(
            clear.counters.env_bsp_walked, clear.counters.env_cells_visited,
            "{cell:?}: an interior cell was entered without its tree being walked"
        );
        assert!(
            !clear.collision_info.collided_with_environment,
            "{cell:?}: calibration blocked"
        );
        assert!(
            cross.counters.env_bsp_walked > 0 && cross.collision_info.collided_with_environment,
            "{cell:?}: the crossing probe walked {} trees and reported env={}",
            cross.counters.env_bsp_walked,
            cross.collision_info.collided_with_environment
        );
        clear_walked += clear.counters.env_bsp_walked;
        cross_walked += cross.counters.env_bsp_walked;
        nobsp += clear.counters.env_cells_without_bsp + cross.counters.env_cells_without_bsp;
    }
    println!(
        "{checked} rooms: {clear_walked} trees walked by clear probes, {cross_walked} by crossing \
         probes, {nobsp} cells entered with no tree"
    );
    assert!(checked >= 20, "only {checked} rooms had a wall to shoot at");
    assert!(clear_walked > 0 && cross_walked > 0);
    assert_eq!(
        nobsp, 0,
        "an interior cell was handed to the sweep with `physics_bsp: None`. That is the loader \
         dropping the tree, not the sweep ignoring it."
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Portal traversal, and what the visited-cell counter counts
// ---------------------------------------------------------------------------------------------

/// **A sweep crosses portals, and the visited-cell counter counts entries, not cells.**
///
/// The sweep builds its transit-cell list directly from the environment portals and resolves the
/// adjacent cell for either portal representation, so a probe aimed through a portal records
/// traversal beyond its starting cell. And a blocking camera sweep records fewer cell-id
/// observations than its `env_cells_visited` counter reads, so that counter does not distinguish
/// blocking from not blocking.
///
/// A blocking camera sweep places the sphere at the exact touch and lets it continue along the
/// wall, so its final cell can differ from the body's cell; the fn name's "visits one cell" is
/// the counter claim, which is what the second half asserts.
#[test]
fn the_sweep_crosses_portals_and_the_blocking_camera_sweep_still_visits_one_cell() {
    let store = store();
    let src = source(&store).expect("the landscape source from the retail dats");

    // --- half one: a probe aimed through a portal records traversal beyond its starting cell --
    let (mut examined, mut crossed) = (0usize, 0usize);
    for (cell, g, centre, _half) in rooms(&src) {
        for p in &g.portals {
            if p.other_cell_id == 0xFFFF_FFFF || p.portal.vertices.len() < 3 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let n = p.portal.vertices.len() as f32;
            let mut pc = Vec3::ZERO;
            for v in &p.portal.vertices {
                pc = Vec3::new(pc.x + v.x, pc.y + v.y, pc.z + v.z);
            }
            let pc = Vec3::new(pc.x / n, pc.y / n, pc.z / n);
            let d = Vec3::new(pc.x - centre.x, pc.y - centre.y, pc.z - centre.z);
            if d.x.mul_add(d.x, d.y.mul_add(d.y, d.z * d.z)).sqrt() < 1.0 {
                continue;
            }
            // Well past the portal plane, so a sweep that can cross one will.
            let far = Vec3::new(
                centre.x + d.x * 1.8,
                centre.y + d.y * 1.8,
                centre.z + d.z * 1.8,
            );
            examined += 1;
            let l2w = |v: Vec3| to_landblock(&g, v);
            let Some(t) = probe(&src, cell, l2w(centre), l2w(far)) else {
                continue;
            };
            let ended = t.sphere_path.curr_pos.cell;
            if ended != cell || t.cell_array.cells.iter().any(|c| c.cell_id != cell) {
                crossed += 1;
            }
        }
    }
    println!("{examined} portal shots: {crossed} left the starting cell");
    assert!(
        examined >= 500,
        "only {examined} portals to aim at; the corpus has thinned"
    );
    assert!(
        crossed * 10 >= examined * 9,
        "only {crossed} of {examined} portal shots recorded traversal beyond the starting cell. A \
         shot counts when its end cell differs or its cell array contains another cell; if this \
         rate has regressed, interior traversal has regressed with it."
    );

    // --- half two: the camera sweep blocks, and entry count exceeds cell observations --------
    let g = LandSource::env_cell(src.as_ref(), MEASURED_CELL)
        .expect("the measured env cell is in the retail dats");
    let (centre, half) = bounds(&g).expect("polygons");
    let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
    let h = w.create(ObjectId(1), Arc::new(SetupGeometry::dummy()), true);
    w.enter_cell(h, MEASURED_CELL);
    let from = Position::new(
        MEASURED_CELL,
        Frame::new(to_landblock(&g, centre), Quat::IDENTITY),
    );
    if let Some(o) = w.get_mut(h) {
        o.position = from;
        o.transient_state.set_active_bit(true);
    }
    w.calc_cross_cells(h, false);
    let to = Position::new(
        MEASURED_CELL,
        Frame::new(
            to_landblock(
                &g,
                Vec3::new(centre.x + half.x * 2.0 + 2.0, centre.y, centre.z),
            ),
            Quat::IDENTITY,
        ),
    );
    // The viewer-state bit and 0.3 m sphere used by the camera path, not the missile probe.
    let cam = w
        .sweep_sphere(
            h,
            dereth_physics::globals::VIEWER_OBJECT_INFO_STATE,
            dereth_physics::geom::Sphere::new(Vec3::ZERO, 0.3),
            MEASURED_CELL,
            &from,
            &to,
        )
        .expect("the camera sweep positions");
    // These are the cell-id observations in the transition array; this vector is not deduplicated.
    let distinct: Vec<CellId> = cam.cell_array.cells.iter().map(|c| c.cell_id).collect();
    println!(
        "camera sweep: env={} visited={} walked={} cell observations {distinct:?}",
        cam.collision_info.collided_with_environment,
        cam.counters.env_cells_visited,
        cam.counters.env_bsp_walked,
    );
    assert!(
        cam.collision_info.collided_with_environment,
        "the camera sweep did not block against the same wall the missile probe does: a \
         regression in the viewer arm, not in this test."
    );
    // Collision resolution bisects to the exact touch, places the sphere against the wall and lets
    // the walk carry on along it, so the sweep need not end in the cell it started in. The claim is
    // the counter one: it counts entries, not cell-array observations, so a reading of 1 does not
    // distinguish a blocking sweep from a non-blocking one.
    assert!(
        distinct.contains(&MEASURED_CELL),
        "the blocking camera sweep never entered the cell it started in: {distinct:?}"
    );
    assert!(
        usize::try_from(cam.counters.env_cells_visited).unwrap_or(usize::MAX) > distinct.len(),
        "`env_cells_visited` ({}) is not larger than the {} cell-array observation(s) the sweep \
         recorded, so this run cannot show that the counter counts entries rather than cells",
        cam.counters.env_cells_visited,
        distinct.len()
    );
}
