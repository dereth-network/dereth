//! The shared collision-walk probe: is a walking body inside an object's mesh, and did it stop?
//!
//! The judge is a count of the frames of a walk on which the body's sphere is inside the object's
//! own physics BSP ([`frames_inside`]). A displacement proxy ("how far did the body get") is not
//! used: a body that walks past a door and one the door stops can both end outside its mesh, and
//! a body that climbs a threshold and stops can travel less than one that walked past.
//! [`came_to_rest`] qualifies a control, and [`in_the_room`] rejects sampled start points that
//! are in a wall or in open air. These are test instruments, not client behaviour.
//! Fixture: synthetic BSP trees here; the callers bring retail door and room geometry.
//!
//! Behaviour: none (the collision-walk instrument's own calibration).

#![allow(dead_code)]

use dereth_physics::source::EnvCellGeometry;
use dereth_physics::{SetupGeometry, Sphere, V3};
use dereth_primitives::{Position, Vec3};

/// Test every placed part's solid BSP with the centre check on, for one frame of a walk: is the
/// body inside this object's own physics mesh?
///
/// The part is placed with [`SetupGeometry::placed_part`], as the collision path places it. The
/// body's sphere is transformed into the part's local space and divided by `gfxobj_scale`.
/// **The frame is derived from the part's own transform and never named by index**: a retail
/// door's setup turns part 0 by −150 degrees about z, so taking the slab's thin axis to be world
/// `y` fabricates an asymmetry that is not there.
pub fn inside_object(g: &SetupGeometry, pos: &Position, p: Vec3) -> bool {
    (0..g.parts.len()).any(|i| {
        let Some(part) = g.placed_part(i, pos, 1.0) else {
            return false;
        };
        let Some(tree) = part.physics_bsp.as_ref() else {
            return false;
        };
        let inv = 1.0 / part.gfxobj_scale;
        let m = dereth_physics::math::l2g(part.pos.frame.rotation);
        let centre = p.add(Vec3::new(0.0, 0.0, 0.5));
        let local = dereth_physics::math::globaltolocalvec(m, centre.sub(part.pos.frame.origin));
        tree.sphere_intersects_solid(&Sphere::new(local.mul(inv), 0.5 * inv), true)
    })
}

/// Is `local` -- a point in the **cell's own space** -- somewhere in this room a body could
/// stand, as opposed to open air outside it or the inside of its masonry?
///
/// Two questions use different structures in the cell's geometry: the cell BSP says whether the
/// point is in the room, and the physics BSP says whether it is inside the walls. A cell that
/// carries neither answers `false` and `true` respectively, which is the client's degenerate
/// reading: no cell BSP means nothing is inside the cell.
///
/// Every caller uses it the same way -- to reject a sampled approach point that would start
/// inside a wall or in open air -- so there is one contract to keep, not four.
pub fn in_the_room(cell: &EnvCellGeometry, local: Vec3) -> bool {
    cell.cell_bsp
        .as_ref()
        .is_some_and(|b| b.point_inside_cell_bsp(local))
        && !cell
            .physics_bsp
            .as_ref()
            .is_some_and(|b| b.point_intersects_solid(local))
}

/// **[`in_the_room`]'s own calibration, in both directions.** Its callers use the answer only to
/// reject sampled approach points, and accepting an extra one changes no walk verdict, so
/// dropping the masonry half (reducing it to `point_inside_cell_bsp` alone) survives every caller.
/// The predicate's contract has to be asserted here, where the answer itself is the subject.
///
/// The room is a 6 m cube of cell BSP with a solid half-space of physics BSP cutting through it,
/// so the three regions -- in the room and clear, in the room and inside the masonry, outside the
/// room -- are all reachable by a point, and each has a different answer.
#[test]
fn in_the_room_separates_clear_floor_from_masonry_and_from_open_air() {
    use std::sync::Arc;

    use dereth_physics::geom::bsp::{BspNode, BspNodeKind, BspTree};
    use dereth_physics::geom::{Plane, Polygon};
    use dereth_primitives::{Frame, Quat};

    const HALF: f32 = 3.0;
    let bound = Sphere::new(Vec3::ZERO, HALF * 3.0_f32.sqrt());

    // The room: six half-spaces then a leaf. Cell-BSP point traversal follows positive children,
    // and running out of them means "inside", so a point is in the room exactly when it is in
    // front of all six planes.
    let face = |n: Vec3, next: u32| BspNode {
        sphere: bound,
        splitting_plane: Plane { normal: n, d: HALF },
        pos_child: Some(next),
        neg_child: Some(next),
        kind: BspNodeKind::Node,
        in_polys: vec![],
    };
    let leaf = |solid: bool, polys: Vec<u32>| BspNode {
        sphere: bound,
        splitting_plane: Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: 1000.0,
        },
        pos_child: None,
        neg_child: None,
        kind: BspNodeKind::Leaf {
            leaf_index: 0,
            solid,
        },
        in_polys: polys,
    };
    let cell_bsp = Arc::new(BspTree {
        nodes: vec![
            face(Vec3::new(1.0, 0.0, 0.0), 1),
            face(Vec3::new(-1.0, 0.0, 0.0), 2),
            face(Vec3::new(0.0, 1.0, 0.0), 3),
            face(Vec3::new(0.0, -1.0, 0.0), 4),
            face(Vec3::new(0.0, 0.0, 1.0), 5),
            face(Vec3::new(0.0, 0.0, -1.0), 6),
            leaf(false, vec![]),
        ],
        polygons: vec![],
    });

    // The masonry: everything with `x < -1`. Physics-BSP point intersection treats a leaf that
    // carries polygons as solid, so the negative child is the one that does.
    let physics_bsp = Arc::new(BspTree {
        nodes: vec![
            BspNode {
                sphere: bound,
                splitting_plane: Plane {
                    normal: Vec3::new(1.0, 0.0, 0.0),
                    d: 1.0,
                },
                pos_child: Some(1),
                neg_child: Some(2),
                kind: BspNodeKind::Node,
                in_polys: vec![],
            },
            leaf(false, vec![]),
            leaf(true, vec![0]),
        ],
        polygons: vec![Polygon::new(vec![
            Vec3::new(-1.0, -HALF, -HALF),
            Vec3::new(-1.0, HALF, -HALF),
            Vec3::new(-1.0, HALF, HALF),
            Vec3::new(-1.0, -HALF, HALF),
        ])],
    });

    let cell = EnvCellGeometry {
        frame: Frame::new(Vec3::new(50.0, 60.0, 10.0), Quat::IDENTITY),
        cell_bsp: Some(Arc::clone(&cell_bsp)),
        physics_bsp: Some(Arc::clone(&physics_bsp)),
        ..EnvCellGeometry::default()
    };

    // The premises, so the three stations really are three regions and not three copies of one.
    assert!(
        cell_bsp.point_inside_cell_bsp(Vec3::ZERO),
        "the room contains its own centre"
    );
    assert!(
        cell_bsp.point_inside_cell_bsp(Vec3::new(-2.0, 0.0, 0.0)),
        "and the masonry station"
    );
    assert!(
        !cell_bsp.point_inside_cell_bsp(Vec3::new(10.0, 0.0, 0.0)),
        "and not the outside one"
    );
    assert!(
        !physics_bsp.point_intersects_solid(Vec3::ZERO),
        "the centre is clear of the masonry"
    );
    assert!(
        physics_bsp.point_intersects_solid(Vec3::new(-2.0, 0.0, 0.0)),
        "and the masonry station is inside it"
    );

    // The three answers.
    assert!(
        in_the_room(&cell, Vec3::ZERO),
        "clear floor inside the room"
    );
    assert!(
        !in_the_room(&cell, Vec3::new(-2.0, 0.0, 0.0)),
        "a point inside the masonry is not somewhere a body can stand, even though the cell BSP \
         says it is in the room -- this is the half a `point_inside_cell_bsp`-only predicate loses"
    );
    assert!(
        !in_the_room(&cell, Vec3::new(10.0, 0.0, 0.0)),
        "open air outside the room"
    );

    // A cell with no physics BSP has no masonry, so the second clause is the identity: the same
    // point that was refused above is now accepted. Without this, "false inside the masonry" is
    // satisfied by a predicate that is false everywhere its second clause is consulted.
    let hollow = EnvCellGeometry {
        physics_bsp: None,
        ..EnvCellGeometry::clone(&cell)
    };
    assert!(
        in_the_room(&hollow, Vec3::new(-2.0, 0.0, 0.0)),
        "no masonry, no refusal"
    );

    // And the degenerate cell -- no cell BSP at all -- is `false`, not `true`. A cell that cannot
    // say where its room is has no room, which is what the callers need: a sampler must not accept
    // a start point from a cell it knows nothing about.
    let blind = EnvCellGeometry {
        cell_bsp: None,
        ..EnvCellGeometry::clone(&cell)
    };
    assert!(
        !in_the_room(&blind, Vec3::ZERO),
        "a cell with no cell BSP contains nothing"
    );
}

/// How many frames of a walk the body spent inside the object's own physics mesh.
///
/// **This is the judge**, and the reason it is a count over the path rather than a test of the
/// last frame is that a body which walks *through* a door ends outside its mesh exactly as a body
/// stopped by it does. A closed door must be penetrated on **none** of them.
pub fn frames_inside(geometry: &SetupGeometry, pos: &Position, path: &[Vec3]) -> usize {
    path.iter()
        .filter(|p| inside_object(geometry, pos, **p))
        .count()
}

/// Did the body come to rest? Its displacement over the last second of the walk, against a
/// centimetre.
///
/// **This is control quality and not the judge.** "Travelled more than 0.5 m and did not end
/// inside the mesh" is satisfied by a body that walked straight *past* the door as comfortably as
/// by one the door stopped, which is exactly what cell `0x86020105` did. A closed door is a
/// control for *the closed door stopped it* only if the body it is compared against actually
/// stopped.
///
/// Loosening the threshold to 100 m — accepting a body that never stopped — is a **surviving**
/// mutation of the door modules: cell `0x86020105` is then admitted as a station and *passes*, 84
/// frames inside the mesh against 0. The direct judge
/// gets the right answer with or without the guard. It is kept because a control that walked past
/// the door is not a control, and a later change to the judge should not silently inherit one.
///
/// Its own contract is pinned by [`the_rest_filter_separates_a_body_that_stopped_from_one_still_moving`],
/// so the function is falsifiable here even though its effect on the door verdict is not.
pub fn came_to_rest(path: &[Vec3]) -> bool {
    let n = path.len();
    if n < 2 {
        return false;
    }
    let back = 31.min(n - 1);
    let d = path[n - 1].sub(path[n - 1 - back]);
    d.dot(d).sqrt() < 0.01
}

/// **[`came_to_rest`]'s own calibration, in both directions** — a door walk's verdict does not
/// depend on it (see the note above), so nothing else can falsify it. The threshold is therefore tested here on
/// both a moving control and a stopped control instead of being accepted as an assumption.
///
/// The numbers are the measured ones. The closed control at cell `0x86020105` moved **1.157 m**
/// over its last second and must read as *still moving*; a body a door has stopped moves nothing
/// at all and must read as *at rest*. The threshold is a centimetre over the trailing 31 frames,
/// which at the 30 Hz the walks run at is one second.
#[test]
fn the_rest_filter_separates_a_body_that_stopped_from_one_still_moving() {
    // 120 frames, the last 31 of which advance a total of 1.157 m: cell `0x86020105`'s closed
    // control, which a displacement filter accepted as a body the door had stopped.
    let mut walking: Vec<Vec3> = Vec::new();
    for i in 0..120_i16 {
        walking.push(Vec3::new(f32::from(i) * (1.157 / 31.0), 4.0, 66.0));
    }
    assert!(
        !came_to_rest(&walking),
        "a body still advancing 1.157 m in its last second has not been stopped by anything"
    );

    // The same walk, brought to a halt for its last second. Only the tail may decide.
    let mut stopped = walking.clone();
    let held = stopped[120 - 32];
    for p in stopped.iter_mut().skip(120 - 31) {
        *p = held;
    }
    assert!(
        came_to_rest(&stopped),
        "a body that has not moved for a second is at rest"
    );

    // The window is the last second and not the whole path: a body that walked four metres and
    // then stopped is at rest, which is the case the door modules rely on.
    assert!(
        stopped[119]
            .sub(stopped[0])
            .dot(stopped[119].sub(stopped[0]))
            .sqrt()
            > 3.0,
        "the resting path must still be a walk, or this asserts nothing about the window"
    );

    // Just inside and just outside the centimetre, so the constant is load-bearing rather than
    // decorative. 31 frames of 0.0002 m is 0.0062 m; 31 of 0.0004 m is 0.0124 m.
    let creep = |per_frame: f32| -> Vec<Vec3> {
        (0..120_i16)
            .map(|i| Vec3::new(f32::from(i) * per_frame, 4.0, 66.0))
            .collect()
    };
    assert!(
        came_to_rest(&creep(0.000_2)),
        "6.2 mm over the last second is at rest"
    );
    assert!(
        !came_to_rest(&creep(0.000_4)),
        "12.4 mm over the last second is not"
    );

    // A path too short to have a last second cannot be said to have stopped. This is the
    // degenerate answer and it is `false`, so a truncated walk is discarded rather than accepted.
    assert!(!came_to_rest(&[]));
    assert!(!came_to_rest(&[Vec3::ZERO]));
    // ...but two frames a metre apart are still "not at rest", so the guard is not merely a
    // length check.
    assert!(!came_to_rest(&[Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0)]));
}
