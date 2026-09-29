//! Both faces of an interior door stop a walking body at the same distance.
//!
//! Fixture: the retail training-academy door setup `0x0200024F` placed in the real interior cells
//! of block `0x8602`, walked at by mirrored 20-degree approaches reflected through the slab
//! plane. Indoors the cell's own collision BSP is walked alongside the door's parts and the door's
//! mesh registers in two cells; if either made the surface one-sided, the body would meet a
//! different shape from each side. The outdoor counterpart is `dereth-physics`'s door-collision
//! test.

#![allow(dead_code)]

use crate::common::collision_probe;

use std::sync::Arc;

use dereth_assets::{Decode, Setup};
use dereth_client::env_cells::EnvCellLoader;
use dereth_client::land_source::DatLandSource;
use dereth_client::object_physics::{setup_geometry_with_parts, SetupPartStats};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::source::EnvCellGeometry;
use dereth_physics::{LandSource, PhysHandle, PhysicsWorld, SetupGeometry, Sphere, V3};
use dereth_primitives::num::math;
use dereth_primitives::{CellId, DataId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};

use collision_probe::{in_the_room, inside_object};

/// The training academy's dungeon block.
const TRAINING_DUNGEON: u16 = 0x8602;
/// `door`, the plainest of the five door setups block `0x8602` places.
const DOOR: u32 = 0x0200_024F;
/// `ReportCollisions|IgnoreCollisions`, the word 491 of 542 retail door weenies carry; see
/// `door_opens_ethereal.rs::RETAIL_DOOR_STATES`.
const DOOR_STATE: u32 = 0x0000_0018;
const BODY_RADIUS: f32 = 0.5;
/// How far out each arm starts, measured from that face rather than from the object origin.
const START_DISTANCE: f32 = 1.5;
/// Off the slab normal. Oblique on purpose: an exactly head-on walk collapses through
/// the transition solver's crease branch, which is measured on its own in the
/// outdoor station and is not what this one is asking about.
const APPROACH_DEG: f32 = 20.0;

fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are required: set DERETH_TEST_DAT_DIR (looked in {})",
            dereth_dat::testing::dat_dir().display()
        )
    }))
}

fn setup(store: &RetailDatStore, id: DataId) -> Setup {
    let b = store
        .read_typed(DbType::Setup, id)
        .unwrap_or_else(|e| panic!("setup {:#010X}: {e}", id.0));
    Setup::decode_payload(id, &b).unwrap_or_else(|e| panic!("setup {:#010X}: {e}", id.0))
}

fn player_geometry() -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, BODY_RADIUS), BODY_RADIUS)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, BODY_RADIUS), 1.0),
        step_up_height: 0.3,
        step_down_height: 0.3,
        radius: BODY_RADIUS,
        height: 1.0,
        ..SetupGeometry::default()
    })
}

/// The door as an **object-stream** body, exactly as `door_opens_ethereal.rs` builds one: it is
/// `dynamic`, not `STATIC_PS`, because a door has a motion table and animates, and because the
/// ethereal exemption in object collision handling lives in the *else*-arm of its static test.
///
/// The descriptor's bits are **or**-ed in rather than written wholesale:
/// `HAS_PHYSICS_BSP_PS` records a scan of the parts and `PhysicsObj::new` sets it from the
/// geometry, while assigning the full state word would clear that bit. Keeping it here leaves this
/// station exactly one variable, the approach direction.
fn register_door(
    w: &mut PhysicsWorld,
    cell: CellId,
    frame: Frame,
    g: Arc<SetupGeometry>,
    state: u32,
) -> PhysHandle {
    let h = w.create(ObjectId(7), g, true);
    w.enter_cell(h, cell);
    if let Some(o) = w.get_mut(h) {
        o.state = dereth_physics::PhysicsState(o.state.0 | state);
        o.set_frame(frame);
        o.position = Position::new(cell, frame);
    }
    assert!(
        !w.get(h).expect("live").state.is_static(),
        "a door is never STATIC_PS"
    );
    assert!(
        w.get(h).expect("live").state.has_physics_bsp(),
        "the door must keep the BSP arm or the walk measures a fallback shape"
    );
    w.calc_cross_cells(h, true);
    h
}

fn spawn(w: &mut PhysicsWorld, cell: CellId, at: Vec3, per_substep: Vec3) -> PhysHandle {
    let h = w.create(ObjectId(1), player_geometry(), true);
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        o.position = Position::new(cell, Frame::new(at, Quat::IDENTITY));
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            vec![Frame::new(per_substep, Quat::IDENTITY); 400],
            true,
        )));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    h
}

/// A point of an interior cell a body can stand at, in **world** space.
fn a_standable_point(cell: &EnvCellGeometry) -> Option<Vec3> {
    standable_points(cell).into_iter().next()
}

/// **Every** point of an interior cell a body can stand at, in **world** space.
///
/// The station needs more than the first one. A door dropped at an arbitrary standable point has
/// its two approach corridors wherever the slab normal happens to point, and in a dungeon that is
/// usually straight into masonry: most of block `0x8602`'s cells reject the station on exactly
/// that when only the first point is tried. Returning the whole set lets the caller look for a
/// placement that actually has open floor on **both** faces, which is the only kind of placement
/// the differential means anything at.
fn standable_points(cell: &EnvCellGeometry) -> Vec<Vec3> {
    let mut out = Vec::new();
    if cell.cell_bsp.is_none() {
        return out;
    }
    for &z in &[0.5f32, 1.0] {
        for i in -12i8..=12 {
            for j in -12i8..=12 {
                let local = Vec3::new(f32::from(i) * 0.5, f32::from(j) * 0.5, z);
                if in_the_room(cell, local) {
                    out.push(dereth_physics::math::localtoglobal(&cell.frame, local));
                }
            }
        }
    }
    out
}

/// The door's slab normal in **world** space, flattened to horizontal.
///
/// Read off part 0's own vertices in its own space and only then rotated, because this setup's
/// placement frame turns part 0 by -150 degrees about z: reading a "thin axis" straight out of
/// world coordinates is a frame error that reads as a one-sided door.
fn slab_normal(g: &SetupGeometry, pos: &Position) -> Option<Vec3> {
    let part = g.placed_part(0, pos, 1.0)?;
    let tree = part.physics_bsp.as_ref()?;
    let (mut lo, mut hi) = (
        Vec3::new(f32::MAX, f32::MAX, f32::MAX),
        Vec3::new(f32::MIN, f32::MIN, f32::MIN),
    );
    for p in &tree.polygons {
        for v in &p.vertices {
            lo = Vec3::new(lo.x.min(v.x), lo.y.min(v.y), lo.z.min(v.z));
            hi = Vec3::new(hi.x.max(v.x), hi.y.max(v.y), hi.z.max(v.z));
        }
    }
    let local = if hi.x - lo.x <= hi.y - lo.y {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let m = dereth_physics::math::l2g(part.pos.frame.rotation);
    let mut n = dereth_physics::math::localtoglobalvec(m, local);
    n.z = 0.0;
    if n.normalize_check_small() {
        None
    } else {
        Some(n)
    }
}

/// The mesh's own centre in world space comes from part 0's root sphere. The
/// object's origin is not the door; this setup's part frame offsets the slab.
fn mesh_centre(g: &SetupGeometry, pos: &Position) -> Option<Vec3> {
    let part = g.placed_part(0, pos, 1.0)?;
    let root = part.physics_bsp.as_ref()?.root()?;
    let m = dereth_physics::math::l2g(part.pos.frame.rotation);
    Some(
        part.pos
            .frame
            .origin
            .add(dereth_physics::math::localtoglobalvec(
                m,
                root.sphere.center.mul(part.gfxobj_scale),
            )),
    )
}

/// What one approach produced. `closest` is sampled **per frame** rather than read off the end of
/// the run, because a body stopped by a panel keeps sliding along it and eventually rounds the
/// edge: the final position measures where it wandered to, not where it was stopped.
struct Leg {
    closest: f32,
    facing: u32,
    travel: f32,
    /// Frames the body's own collision sphere spent **inside the door's mesh**, by
    /// testing the body sphere against every part's solid BSP. This is the "was it really
    /// stopped" guard, and it is asked of the mesh directly rather than of a distance this test
    /// computed: the slab normal and the face offsets are the instrument's arithmetic, and an
    /// error there would otherwise go unseen.
    inside: u32,
}

#[allow(clippy::too_many_arguments)]
fn walk_leg(
    src: &Arc<DatLandSource>,
    cell: CellId,
    frame: Frame,
    g: &Arc<SetupGeometry>,
    centre: Vec3,
    tangent: Vec3,
    out: Vec3,
    face: f32,
    start: Vec3,
    step: Vec3,
    with_door: bool,
) -> Leg {
    let mut w = PhysicsWorld::new(Arc::clone(src) as Arc<dyn LandSource>);
    if with_door {
        let _door = register_door(&mut w, cell, frame, Arc::clone(g), DOOR_STATE);
    }
    let h = spawn(&mut w, cell, start, step);
    let pos = Position::new(cell, frame);
    let dt = 1.0 / 30.0;
    let (mut t, mut closest, mut facing, mut travel) = (0.0f64, f32::MAX, 0u32, 0.0f32);
    let mut inside = 0u32;
    for _ in 0..75 {
        t += dt;
        w.use_time(LocalTime(t), false);
        let p = w.get(h).expect("live").position.frame.origin;
        travel = travel.max(math::hypotf(p.x - start.x, p.y - start.y));
        if inside_object(g, &pos, p) {
            inside += 1;
        }
        let rel = p.add(Vec3::new(0.0, 0.0, BODY_RADIUS)).sub(centre);
        if rel.dot(tangent).abs() <= 0.45 {
            facing += 1;
            closest = closest.min(rel.dot(out) - face);
        }
    }
    Leg {
        closest,
        facing,
        travel,
        inside,
    }
}

/// One doorway's pair of mirrored approaches, or `None` when this cell cannot host the station.
#[allow(clippy::type_complexity)]
fn doorway_pair(
    src: &Arc<DatLandSource>,
    geom: &EnvCellGeometry,
    geometry: &Arc<SetupGeometry>,
    cell: CellId,
    at: Vec3,
) -> Result<(f32, f32, Leg, Leg), &'static str> {
    let frame = Frame::new(Vec3::new(at.x, at.y, at.z - 0.5), Quat::IDENTITY);
    let pos = Position::new(cell, frame);
    let normal = slab_normal(geometry, &pos).ok_or("no slab normal")?;
    let mc = mesh_centre(geometry, &pos).ok_or("no mesh centre")?;
    let tangent = Vec3::new(-normal.y, normal.x, 0.0);

    // Where this door's own faces are, by a zero-radius march out from the mesh centre. Every
    // distance below is referred to these, so "matches the mesh" means the mesh and not a number
    // this test chose.
    let probe = |dir: Vec3| -> f32 {
        let mut u = 0.0f32;
        let base = Vec3::new(mc.x, mc.y, frame.origin.z);
        while u < 3.0 {
            let p = base.add(dir.mul(u)).sub(Vec3::new(0.0, 0.0, BODY_RADIUS));
            if !inside_object(geometry, &pos, p) {
                return u;
            }
            u += 0.002;
        }
        u
    };
    let (face_pos, face_neg) = (probe(normal), probe(normal.mul(-1.0)));
    if face_pos <= 0.0 || face_neg <= 0.0 {
        return Err("mesh probe found no slab");
    }

    // The reflection is through the slab **plane**: the normal component flips with the side and
    // the tangential one does not, so the two arms are true mirror images and meet the slab at the
    // same angle. Negating both components would mirror the vector through the origin instead of
    // reflecting it through the slab plane, changing the frame under test.
    let r = APPROACH_DEG.to_radians();
    let (cos, sin, tan) = (math::cosf(r), math::sinf(r), math::tanf(r));
    let mut arms = Vec::new();
    for sign in [1.0f32, -1.0] {
        let out = normal.mul(sign);
        let face = if sign > 0.0 { face_pos } else { face_neg };
        let drift = (START_DISTANCE + face) * tan;
        let s_xy = mc
            .add(out.mul(START_DISTANCE + face))
            .sub(tangent.mul(drift * 0.5));
        let start = Vec3::new(s_xy.x, s_xy.y, frame.origin.z + 0.02);
        // Both starts must be in this cell's open floor and clear of the door, or the arm is not a
        // walk at the door and its `closest` is not a measurement of anything.
        let local = dereth_physics::math::globaltolocal(&geom.frame, start);
        if !in_the_room(geom, local) {
            return Err("a start is not in open floor");
        }
        if inside_object(geometry, &pos, start) {
            return Err("a start is inside the door");
        }
        let dir = out.mul(-cos).add(tangent.mul(sin));
        arms.push((out, face, start, dir.mul(0.12)));
    }

    let mut legs = Vec::new();
    for (out, face, start, step) in &arms {
        // **The control decides whether this arm is a walk into the door at all.** Run it with the
        // door absent: if the body does not reach the volume the door's mesh occupies, then in the
        // real run something else in the room stopped it, and its distance is a measurement of the
        // room and not of the door. `mesh_collision.rs` uses the same control for the same reason.
        let control = walk_leg(
            src, cell, frame, geometry, mc, tangent, *out, *face, *start, *step, false,
        );
        if control.inside == 0 {
            return Err("an arm never reaches the door with the door removed");
        }
        legs.push(walk_leg(
            src, cell, frame, geometry, mc, tangent, *out, *face, *start, *step, true,
        ));
    }
    let back = legs.pop().ok_or("no back leg")?;
    let front = legs.pop().ok_or("no front leg")?;
    if front.facing < 5 || back.facing < 5 {
        return Err("an arm never faced the panel");
    }
    Ok((face_pos, face_neg, front, back))
}

/// Behaviour: objects.door.both-faces-of-an-interior-door-stop-a-body-at-the-same-distance
/// **The acceptance differential, indoors.** The same door, in a real interior cell, mirrored
/// approaches: the two faces must stop the body at the same distance.
#[test]
fn an_interior_doorway_presents_the_same_surface_to_both_faces() {
    let store = store();
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let src =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the retail height table"));
    src.load_block_cells(dereth_primitives::LandblockId(TRAINING_DUNGEON));
    let decoded = setup(&store, DataId(DOOR));
    let mut stats = SetupPartStats::default();
    let geometry = Arc::new(setup_geometry_with_parts(&store, &decoded, &mut stats));
    assert!(
        geometry.spheres.is_empty()
            && geometry.cyl_spheres.is_empty()
            && geometry.caches_physics_bsp(),
        "setup {DOOR:#010X} must be BSP-only or the walk measures a fallback shape: {} spheres, \
         {} cylspheres, bsp={}",
        geometry.spheres.len(),
        geometry.cyl_spheres.len(),
        geometry.caches_physics_bsp()
    );

    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);

    let mut compared = 0usize;
    let mut worst = 0.0f32;
    let mut reasons: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    for d in &cells {
        if compared >= 8 {
            break;
        }
        let Some(geom) = LandSource::env_cell(src.as_ref(), d.id) else {
            continue;
        };
        // Try every standable point in the cell and take the first placement that has open floor
        // on both faces. A dungeon cell is tight, so most placements put one approach corridor
        // inside masonry; that is a property of where the door was dropped, not of the door.
        let points = standable_points(&geom);
        if points.is_empty() {
            *reasons.entry("no standable point").or_insert(0usize) += 1;
            continue;
        }
        let mut found = None;
        let mut last = "no standable point";
        for at in points {
            match doorway_pair(&src, &geom, &geometry, d.id, at) {
                Ok(v) => {
                    found = Some(v);
                    break;
                }
                Err(why) => last = why,
            }
        }
        let Some((face_pos, face_neg, front, back)) = found else {
            *reasons.entry(last).or_insert(0usize) += 1;
            continue;
        };
        compared += 1;
        let diff = (front.closest - back.closest).abs();
        worst = worst.max(diff);
        println!(
            "cell {:#010X}: mesh faces {face_pos:.3}/{face_neg:.3} m; front {:.4} m \
             ({} frames, travelled {:.3} m), back {:.4} m ({} frames, travelled {:.3} m), \
             difference {diff:.4} m",
            d.id.0,
            front.closest,
            front.facing,
            front.travel,
            back.closest,
            back.facing,
            back.travel
        );

        // The door must actually have stopped the body, on both faces: the walk is 75 frames of
        // 0.12 m against a start 1.5 m out, so an arm that was never stopped travels the lot.
        for (name, leg) in [("front", &front), ("back", &back)] {
            assert_eq!(
                leg.inside, 0,
                "cell {:#010X}: the body's collision sphere spent {} of 75 frames inside the \
                 door's own mesh on the {name} face",
                d.id.0, leg.inside
            );
            assert!(
                leg.travel < 7.0,
                "cell {:#010X}: the {name} arm walked {:.3} m, which is the whole scripted path: \
                 the door did not stop it at all",
                d.id.0,
                leg.travel
            );
        }

        // **The symmetry itself.** The walk advances 0.12 m per frame, so one step of granularity is the
        // most two mirrored arms may legitimately differ by.
        assert!(
            diff < 0.13,
            "cell {:#010X}: the door stops a body {:.4} m from its mesh from the front and \
             {:.4} m from the back, a {diff:.4} m difference between two mirrored approaches",
            d.id.0,
            front.closest,
            back.closest
        );
    }

    println!("cells rejected by reason: {reasons:?}");
    assert!(
        compared >= 3,
        "only {compared} doorways of block {TRAINING_DUNGEON:#06X} could host the station; with \
         fewer than three the differential is not a measurement"
    );
    println!(
        "{compared} interior doorways, worst front/back difference {worst:.4} m against a \
         0.12 m walk step"
    );
}
