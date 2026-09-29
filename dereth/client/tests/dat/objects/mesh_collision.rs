//! An object with a physics BSP collides against its loaded part geometry even when it has no
//! ordinary sphere, and an object whose only volume is a cylinder stops a body too. The geometry
//! choice gives a part BSP priority, then cylindrical spheres, then ordinary spheres. In the
//! training-dungeon block 26 placements carry a physics BSP and no ordinary sphere, among them all
//! five door setups, and 53 reach the cylinder arm. Each walk is a differential: the same object
//! and room with the geometry loaded and with it withheld. The shared collision probe samples
//! positions against the current geometry; it checks sampled frames, not continuous paths.
//!
//! Fixture: the retail cell and portal dats in `$DERETH_TEST_DAT_DIR` (missing data fails),
//! decoded into the `SetupGeometry` the physics world uses. No GPU.

use crate::common::collision_probe;

use std::sync::Arc;

use dereth_assets::{Decode, Setup};
use dereth_client::env_cells::{cell_statics, EnvCellLoader};
use dereth_client::land_source::DatLandSource;
use dereth_client::object_physics::{setup_geometry_with_parts, SetupPartStats};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::source::EnvCellGeometry;
use dereth_physics::{LandSource, PhysHandle, PhysicsWorld, SetupGeometry, Sphere, V3};
use dereth_primitives::num::math;
use dereth_primitives::{CellId, DataId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};

/// The shared collision-walk probe: `inside_object` is the primitive the sampled frames-in-mesh
/// judgment uses.
use collision_probe::{in_the_room, inside_object};

/// The first Holtburg starter area's training-academy block used by the placement census.
const TRAINING_DUNGEON: u16 = 0x8602;

/// The retail store, or **fail**: a missing install is a failure, never a skip.
fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

fn setup(store: &RetailDatStore, id: DataId) -> Option<Setup> {
    let b = store.read_typed(DbType::Setup, id).ok()?;
    Setup::decode_payload(id, &b).ok()
}

// ---------------------------------------------------------------------------------------------
// 1. The census: which geometry arm each placement takes
// ---------------------------------------------------------------------------------------------

/// Oracle: required retail cell and portal DATs through the current decoders.
///
/// Assert the 557 usable-placement census for block `0x8602`: 204 with spheres, 88 with
/// cylinders and no sphere, 26 BSP-only, and 239 with nothing. The raw count is 644 before setup
/// filtering. The runtime BSP flag comes from scanning loaded parts, not the serialized summary.
#[test]
fn the_training_dungeon_census_and_where_the_bsp_flag_comes_from() {
    let store = store();
    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);
    let mut stats = SetupPartStats::default();

    let (mut placements, mut spheres, mut cyl_only, mut bsp_only, mut nothing) = (0, 0, 0, 0, 0);
    let mut flag_disagrees = 0usize;
    // The original physics setup paths ask for placement **`0x65`** and fall back to `0`; the
    // current model draw path asks for `0` directly. If
    // any setup's two entries differed, the drawn pose and the collided pose would differ with it,
    // so the question is measured rather than assumed.
    let mut both_keys = 0usize;
    let mut keys_differ = 0usize;
    for d in &cells {
        for s in cell_statics(d) {
            placements += 1;
            if s.id.0 >> 24 != 0x02 {
                continue;
            }
            let Some(decoded) = setup(&store, s.id) else {
                continue;
            };
            if let (Some(a), Some(b)) = (
                decoded.placement_frames.get(&0x65),
                decoded.placement_frames.get(&0),
            ) {
                both_keys += 1;
                if a.frames != b.frames {
                    keys_differ += 1;
                }
            }
            let g = setup_geometry_with_parts(&store, &decoded, &mut stats);
            if g.caches_physics_bsp() != g.has_physics_bsp {
                flag_disagrees += 1;
            }
            if !g.spheres.is_empty() {
                spheres += 1;
            } else if !g.cyl_spheres.is_empty() {
                cyl_only += 1;
            } else if g.caches_physics_bsp() {
                bsp_only += 1;
            } else {
                nothing += 1;
            }
        }
    }
    eprintln!(
        "block {TRAINING_DUNGEON:04X}: {placements} placements -- {spheres} with spheres, \
         {cyl_only} cylsphere-only, {bsp_only} BSP-only, {nothing} with no geometry at all; \
         {} parts of which {} carry a tree, over {} setup loads, {} undecodable parts, {} with no \
         placement frame at any key, {} carrying a 0x65 placement frame, {flag_disagrees} whose \
         setup record's physics-tree flag disagrees with derived part-tree availability",
        stats.parts,
        stats.parts_with_bsp,
        stats.setups,
        stats.part_undecodable,
        stats.no_placement_frame,
        stats.placement_frame_65,
    );
    eprintln!(
        "{both_keys} of those loads carry a placement frame at BOTH 0x65 and 0, and \
         {keys_differ} of them differ -- this comparison shows whether physics-object finalization \
         and rendering choose the same pose"
    );

    // The block's placement census. A drift in any of these means the decoders or the cell walk
    // moved, not that collision changed.
    assert_eq!(placements, 644, "the block holds 644 baked objects");
    assert_eq!(
        (spheres, cyl_only, bsp_only, nothing),
        (204, 88, 26, 239),
        "the block's collision-shape census"
    );
    assert_eq!(
        stats.part_undecodable, 0,
        "a part graphics-object record of the training dungeon would not decode"
    );
    assert_eq!(
        stats.no_placement_frame, 0,
        "a training-dungeon setup has no placement frame at any key, so its parts would sit at \
         the identity in cell 0"
    );
    // The loaded-part scan is the runtime answer and does not read the serialized summary field;
    // over this block they happen to agree, which is worth knowing and is not what the branch uses.
    assert_eq!(flag_disagrees, 0);
}

// ---------------------------------------------------------------------------------------------
// 2. The walk
// ---------------------------------------------------------------------------------------------

fn player_geometry() -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0),
        step_up_height: 0.3,
        step_down_height: 0.3,
        radius: 0.5,
        height: 1.0,
        ..SetupGeometry::default()
    })
}

/// A body at `at` in the interior cell `cell`, walking `per_substep` every animation frame.
///
/// The cell is named directly rather than resolved to the outdoor land cell beneath the dungeon.
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

fn run(w: &mut PhysicsWorld, seconds: f64) {
    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    while t < seconds {
        t += dt;
        w.use_time(LocalTime(t), false);
    }
}

/// Register one placement's setup geometry directly in its cell so the differential has exactly
/// one object variable.
fn register(w: &mut PhysicsWorld, cell: CellId, frame: Frame, g: Arc<SetupGeometry>) {
    let h = w.create(ObjectId(0), g, false);
    w.enter_cell(h, cell);
    if let Some(o) = w.get_mut(h) {
        o.set_frame(frame);
        o.position = Position::new(cell, frame);
    }
    w.calc_cross_cells(h, true);
}

struct Approach {
    start: Vec3,
    step: Vec3,
}

/// Every clear run-up at `origin`: twenty-four headings, three ranges and both tangent signs,
/// each forty degrees off the line of approach.
///
/// Nothing about the room is assumed: every sampled point is asked of the cell's own `cell_bsp`
/// and `physics_bsp`, so an approach that would start inside a wall or cross one is rejected. The
/// caller runs the **control** down each in turn and keeps the first that actually reaches the
/// object, because an approach that stops on a chair is not a walk into this object.
fn approaches(cell: &EnvCellGeometry, origin: Vec3, z: f32) -> Vec<Approach> {
    let to_local = |p: Vec3| {
        let m = dereth_physics::math::l2g(cell.frame.rotation);
        dereth_physics::math::globaltolocalvec(m, p.sub(cell.frame.origin))
    };
    let mut out_v = Vec::new();
    for &range in &[2.0f32, 2.5, 3.0] {
        for k in 0..24_i8 {
            let a = f32::from(k) * std::f32::consts::TAU / 24.0;
            let out = Vec3::new(math::cosf(a), math::sinf(a), 0.0);
            let start = Vec3::new(origin.x, origin.y, z).add(out.mul(range));
            if !in_the_room(cell, to_local(start)) {
                continue;
            }
            // Sample the radial line every 0.1 metres inside the room. This rejects obvious wall
            // crossings but is not a proof about every point on a continuous path.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: `range / 0.1` is at most 30 and the count is only a loop bound.
            let steps = (range / 0.1).ceil() as i32;
            let clear = (0..=steps).all(|i| {
                #[allow(clippy::cast_precision_loss)]
                let d = i as f32 * 0.1;
                in_the_room(cell, to_local(start.sub(out.mul(d))))
            });
            if !clear {
                continue;
            }
            // Forty degrees off the line of approach, for the reason the module note gives.
            let tangent = Vec3::new(-out.y, out.x, 0.0);
            for sign in [1.0f32, -1.0] {
                let dir = out.mul(-0.766_044_4).add(tangent.mul(0.642_787_6 * sign));
                out_v.push(Approach {
                    start,
                    step: dir.mul(0.12),
                });
            }
        }
    }
    out_v
}

/// Behaviour: objects.collision.an-object-with-only-a-physics-bsp-stops-a-body
/// **The acceptance line.** A body walking at a training-dungeon object that carries a physics BSP
/// and no ordinary spheres ends outside its geometry with the parts loaded and inside it without.
/// A subject may also carry cylindrical spheres; BSP priority is what this differential tests.
///
/// Oracle: the retail DATs decoded into current geometry. The subject objects are chosen by BSP
/// plus no ordinary spheres; the approach and final solid query use the shared current probe.
#[test]
fn a_bsp_only_dungeon_object_stops_a_body_that_used_to_walk_through_it() {
    let store = store();
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let src =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the retail height table"));
    src.load_block_cells(dereth_primitives::LandblockId(TRAINING_DUNGEON));

    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);
    let mut stats = SetupPartStats::default();

    let mut compared = 0usize;
    let mut worst_inside = 0.0f32;
    for d in &cells {
        let Some(geom) = LandSource::env_cell(src.as_ref(), d.id) else {
            continue;
        };
        for s in cell_statics(d) {
            if s.id.0 >> 24 != 0x02 {
                continue;
            }
            let Some(decoded) = setup(&store, s.id) else {
                continue;
            };
            let with_parts = Arc::new(setup_geometry_with_parts(&store, &decoded, &mut stats));
            // The subject: objects with a part BSP and no ordinary sphere.
            if !with_parts.spheres.is_empty() || !with_parts.caches_physics_bsp() {
                continue;
            }
            // The control is the same object built with no parts, so derived part-tree
            // availability is false and the sphere arm runs over an empty sphere list.
            let without = Arc::new(dereth_client::character::setup_geometry(&decoded));
            assert!(!without.caches_physics_bsp() && without.spheres.is_empty());

            let pos = Position::new(d.id, s.frame);
            let trial = |g: &Arc<SetupGeometry>, a: &Approach| -> Vec3 {
                let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
                register(&mut w, d.id, s.frame, Arc::clone(g));
                let h = spawn(&mut w, d.id, a.start, a.step);
                run(&mut w, 4.0);
                w.get(h).expect("live").position.frame.origin
            };

            // The control decides which approach reaches this object. Its endpoint must be inside,
            // and it must travel more than 0.5 metres. The enabled endpoint must remain outside;
            // this BSP station does not assert that every sampled frame stayed outside.
            let mut chosen = None;
            for a in approaches(&geom, s.frame.origin, s.frame.origin.z) {
                if inside_object(&with_parts, &pos, a.start) {
                    continue; // the run-up starts inside the object; nothing to conclude
                }
                let through = trial(&without, &a);
                let travelled = math::hypotf(through.x - a.start.x, through.y - a.start.y);
                if travelled > 0.5 && inside_object(&with_parts, &pos, through) {
                    chosen = Some((a, through, travelled));
                    break;
                }
            }
            let Some((a, through, travelled)) = chosen else {
                continue;
            };
            let stopped = trial(&with_parts, &a);
            let stopped_travel = math::hypotf(stopped.x - a.start.x, stopped.y - a.start.y);
            assert!(
                !inside_object(&with_parts, &pos, stopped),
                "cell {:#010X} setup {:#010X}: the body walked into the object's own mesh at \
                 {stopped:?}; the control reached {through:?}",
                d.id.0,
                s.id.0
            );
            let gained = math::hypotf(through.x - stopped.x, through.y - stopped.y);
            worst_inside = worst_inside.max(gained);
            if compared == 0 {
                eprintln!(
                    "cell {:#010X} setup {:#010X} at ({:.2}, {:.2}, {:.2}): from ({:.2}, {:.2}) \
                     the control walked {travelled:.2} m to ({:.2}, {:.2}), INSIDE the object's \
                     mesh; with the parts loaded the body walked {stopped_travel:.2} m to \
                     ({:.2}, {:.2}) and stopped {gained:.3} m short of it, outside",
                    d.id.0,
                    s.id.0,
                    s.frame.origin.x,
                    s.frame.origin.y,
                    s.frame.origin.z,
                    a.start.x,
                    a.start.y,
                    through.x,
                    through.y,
                    stopped.x,
                    stopped.y
                );
            }
            compared += 1;
        }
    }
    eprintln!(
        "{compared} BSP-only training-dungeon object(s) gave a usable approach; the largest \
         difference the mesh made was {worst_inside:.3} m"
    );
    assert!(
        compared >= 1,
        "no BSP-only object of block {TRAINING_DUNGEON:04X} gave a usable approach, so the test \
         is not measuring anything"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The doors
// ---------------------------------------------------------------------------------------------

/// The five public `WeenieType.Door (19)` setups ACE's `landblock_instance` places in the training
/// dungeon:
/// `door`, `doorprison`, `doorolthoi`, `doormetalcave` and `dooracademya`.
const DOOR_SETUPS: [(u32, &str); 5] = [
    (0x0200_024F, "door"),
    (0x0200_0281, "doorprison"),
    (0x0200_05F2, "doorolthoi"),
    (0x0200_05F1, "doormetalcave"),
    (0x0200_05DA, "dooracademya"),
];

/// Oracle: `client_portal.dat`, through the setup and graphics-object decoders.
///
/// Every one of the five door setups carries a part geometry object with a physics BSP and **no
/// ordinary sphere**, so only the BSP arm can stop a body at a dungeon door. This does not exclude
/// cylindrical spheres.
#[test]
fn every_dungeon_door_setup_is_bsp_only_and_now_has_a_tree() {
    let store = store();
    let mut stats = SetupPartStats::default();
    for (id, name) in DOOR_SETUPS {
        let did = DataId(id);
        let decoded = setup(&store, did).unwrap_or_else(|| panic!("{name} {id:#010X} decodes"));
        let g = setup_geometry_with_parts(&store, &decoded, &mut stats);
        let trees = g.parts.iter().filter(|p| p.physics_bsp.is_some()).count();
        eprintln!(
            "{name} {id:#010X}: {} sphere(s), {} cylsphere(s), {} part(s) of which {trees} carry a \
             physics BSP; setup record's physics-tree flag = {}, derived part-tree availability = {}",
            g.spheres.len(),
            g.cyl_spheres.len(),
            g.parts.len(),
            g.has_physics_bsp,
            g.caches_physics_bsp()
        );
        assert!(
            g.spheres.is_empty(),
            "{name} has a spherical collision shape after all"
        );
        assert!(
            g.has_physics_bsp,
            "the physics-tree flag for {name}'s setup record is clear"
        );
        assert!(
            trees > 0,
            "{name}: no part carries a physics BSP, so the BSP arm has nothing"
        );
        assert!(
            g.caches_physics_bsp(),
            "{name}: no decoded part caches a physics BSP"
        );
    }
    // The two plain wooden ones carry no cylspheres either, so **no** arm of the three-way test
    // could have touched them.
    for (id, name) in [DOOR_SETUPS[0], DOOR_SETUPS[1]] {
        let decoded = setup(&store, DataId(id)).expect("decodes");
        assert!(
            decoded.cylspheres.is_empty(),
            "{name} has a cylindrical collision shape after all"
        );
    }
    assert_eq!(stats.part_undecodable, 0);
}

/// A point of one interior cell a body can stand at, in the block's own space. The cell's own
/// `cell_bsp` and `physics_bsp` decide; this is `WorldScene::standable_point`'s scan, kept local so
/// that the test needs no GPU.
fn a_standable_point(cell: &EnvCellGeometry) -> Option<Vec3> {
    cell.cell_bsp.as_ref()?;
    for &z in &[0.5f32, 1.0] {
        for i in -12i8..=12 {
            for j in -12i8..=12 {
                let local = Vec3::new(f32::from(i) * 0.5, f32::from(j) * 0.5, z);
                if in_the_room(cell, local) {
                    return Some(dereth_physics::math::localtoglobal(&cell.frame, local));
                }
            }
        }
    }
    None
}

/// **The acceptance line's other half: "a door stops you".**
///
/// Oracle: retail door setup `0x0200024F` and its part BSP, placed upright and unrotated in each of
/// the first three usable training-dungeon rooms. The placement is synthetic — a door's own
/// position comes from the server, which an offline run has no access to, and standing the retail
/// mesh in a retail room is what makes the claim checkable without one. Everything that decides
/// the outcome — the mesh, the part's placement frame, the room's floor and walls, the transition
/// — is the client's.
#[test]
fn a_body_walking_into_a_retail_door_is_stopped_by_it() {
    let store = store();
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let src =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the retail height table"));
    src.load_block_cells(dereth_primitives::LandblockId(TRAINING_DUNGEON));

    let did = DataId(DOOR_SETUPS[0].0);
    let decoded = setup(&store, did).expect("the door setup decodes");
    let mut stats = SetupPartStats::default();
    let with_parts = Arc::new(setup_geometry_with_parts(&store, &decoded, &mut stats));
    let without = Arc::new(dereth_client::character::setup_geometry(&decoded));
    assert!(
        without.spheres.is_empty() && !without.caches_physics_bsp(),
        "the control is inert"
    );

    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);
    let mut compared = 0usize;
    let mut best = String::new();
    let mut per_cell: Vec<String> = Vec::new();
    for d in &cells {
        if compared >= 3 {
            break;
        }
        let Some(geom) = LandSource::env_cell(src.as_ref(), d.id) else {
            continue;
        };
        let Some(at) = a_standable_point(&geom) else {
            continue;
        };
        // The door stands on the floor of the room, upright and unrotated.
        let frame = Frame::new(Vec3::new(at.x, at.y, at.z - 0.5), Quat::IDENTITY);
        let pos = Position::new(d.id, frame);

        let trial = |g: &Arc<SetupGeometry>, a: &Approach| -> Vec3 {
            let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
            register(&mut w, d.id, frame, Arc::clone(g));
            let h = spawn(&mut w, d.id, a.start, a.step);
            run(&mut w, 4.0);
            w.get(h).expect("live").position.frame.origin
        };

        for a in approaches(&geom, frame.origin, at.z) {
            if inside_object(&with_parts, &pos, a.start) {
                continue;
            }
            let through = trial(&without, &a);
            let travelled = math::hypotf(through.x - a.start.x, through.y - a.start.y);
            if travelled <= 0.5 || !inside_object(&with_parts, &pos, through) {
                continue;
            }
            let stopped = trial(&with_parts, &a);
            let stopped_travel = math::hypotf(stopped.x - a.start.x, stopped.y - a.start.y);
            assert!(
                !inside_object(&with_parts, &pos, stopped),
                "cell {:#010X}: the body walked through the door to {stopped:?}; with no door \
                 registered it reached {through:?}",
                d.id.0
            );
            // The same walk is also driven head-on and printed, not asserted: a refused frame
            // discards its translation while animation keeps feeding offsets, so it is not the
            // acceptance predicate.
            if best.is_empty() {
                let out = a.start.sub(frame.origin);
                let mut dir = Vec3::new(-out.x, -out.y, 0.0);
                if !dir.normalize_check_small() {
                    let head_on = Approach {
                        start: a.start,
                        step: dir.mul(0.12),
                    };
                    let p_off = trial(&without, &head_on);
                    let p = trial(&with_parts, &head_on);
                    eprintln!(
                        "head-on probe, the same door and the same start driven HEAD ON: with the \
                         door registered the body walked {:.2} m to ({:.2}, {:.2}), {} its mesh; \
                         the control walked {:.2} m to ({:.2}, {:.2}), {} it",
                        math::hypotf(p.x - a.start.x, p.y - a.start.y),
                        p.x,
                        p.y,
                        if inside_object(&with_parts, &pos, p) {
                            "INSIDE"
                        } else {
                            "outside"
                        },
                        math::hypotf(p_off.x - a.start.x, p_off.y - a.start.y),
                        p_off.x,
                        p_off.y,
                        if inside_object(&with_parts, &pos, p_off) {
                            "INSIDE"
                        } else {
                            "outside"
                        }
                    );
                }
            }
            if best.is_empty() {
                best = format!(
                    "cell {:#010X}: a retail `door` at ({:.2}, {:.2}, {:.2}) -- from ({:.2}, \
                     {:.2}) the control walked {travelled:.2} m to ({:.2}, {:.2}), INSIDE its \
                     mesh; with the part BSP the body walked {stopped_travel:.2} m to \
                     ({:.2}, {:.2}), {:.3} m short of the control and outside the door",
                    d.id.0,
                    frame.origin.x,
                    frame.origin.y,
                    frame.origin.z,
                    a.start.x,
                    a.start.y,
                    through.x,
                    through.y,
                    stopped.x,
                    stopped.y,
                    math::hypotf(through.x - stopped.x, through.y - stopped.y)
                );
            }
            // Print one line per compared cell. A door on a cell boundary is registered in both
            // cells its mesh spans. This test does not assert that a particular room is among the
            // three usable rooms it compares.
            per_cell.push(format!(
                "cell {:#010X}: control {travelled:.3} m to ({:.3}, {:.3}), door \
                 {stopped_travel:.3} m to ({:.3}, {:.3}), {:.3} m short",
                d.id.0,
                through.x,
                through.y,
                stopped.x,
                stopped.y,
                math::hypotf(through.x - stopped.x, through.y - stopped.y)
            ));
            compared += 1;
            break;
        }
    }
    eprintln!("{best}");
    for line in &per_cell {
        eprintln!("door walk: {line}");
    }
    assert!(
        compared >= 3,
        "only {compared} rooms gave a usable walk into a door"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The cylsphere arm
// ---------------------------------------------------------------------------------------------

/// The same discrete physics walk as [`run`], recording each sampled resting origin.
///
/// The cylinder arm can slide a glancing body **around**
/// a cylinder rather than stopping it dead, which is what a round column should do, so the claim
/// "the object was there" has to be made about the whole path. A body that walked through a
/// column and out the far side has a final position outside it and has still proved the column
/// was not there.
fn run_sampled(w: &mut PhysicsWorld, h: PhysHandle, seconds: f64) -> Vec<Vec3> {
    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut out = Vec::new();
    while t < seconds {
        t += dt;
        w.use_time(LocalTime(t), false);
        out.push(w.get(h).expect("live").position.frame.origin);
    }
    out
}

/// Is the sampled center `p + z*0.5` inside any scaled finite vertical cylinder, using inclusive
/// radial and height bounds in the object's frame?
///
/// This point-in-solid shape is deliberate. A contact predicate cannot serve as the verdict: a
/// body correctly stopped *touching* a post satisfies it: a body resting against the 0.271 m post
/// `0x02001009` sits at exactly the contact distance and would be reported as having walked into
/// it. Contact and
/// interior have to be told apart, and the body's **centre** being inside the solid column is the
/// unambiguous half.
fn inside_cylspheres(g: &SetupGeometry, pos: &Position, scale: f32, p: Vec3) -> bool {
    let m = dereth_physics::math::l2g(pos.frame.rotation);
    let centre = p.add(Vec3::new(0.0, 0.0, 0.5));
    g.cyl_spheres.iter().any(|c| {
        let low =
            dereth_physics::math::localtoglobalvec(m, c.low_pt.mul(scale)).add(pos.frame.origin);
        let d = centre.sub(low);
        let r = c.radius * scale;
        d.x * d.x + d.y * d.y <= r * r && d.z >= 0.0 && d.z <= c.height * scale
    })
}

/// Oracle: the required retail portal DAT through the current setup decoder.
///
/// The census bucket of cylinders-and-no-sphere contains 88 placements with cylinders and no ordinary spheres. Of those,
/// 35 also have a BSP and take the higher-priority BSP arm; 53 reach the cylinder arm. The arm is
/// selected by a nonempty cylinder list after BSP priority, even if ordinary spheres also exist.
#[test]
fn the_training_dungeons_cylsphere_placements_and_which_arm_each_takes() {
    let store = store();
    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);
    let mut stats = SetupPartStats::default();

    let (mut cyl_only, mut cyl_and_spheres, mut cyl_arm, mut bsp_arm_with_cyl) = (0, 0, 0, 0);
    let mut most = 0usize;
    for d in &cells {
        for s in cell_statics(d) {
            if s.id.0 >> 24 != 0x02 {
                continue;
            }
            let Some(decoded) = setup(&store, s.id) else {
                continue;
            };
            let g = setup_geometry_with_parts(&store, &decoded, &mut stats);
            if g.cyl_spheres.is_empty() {
                continue;
            }
            most = most.max(g.cyl_spheres.len());
            if g.caches_physics_bsp() {
                // Part-BSP presence wins outright over primitive geometry.
                bsp_arm_with_cyl += 1;
                continue;
            }
            cyl_arm += 1;
            if g.spheres.is_empty() {
                cyl_only += 1;
            } else {
                cyl_and_spheres += 1;
            }
        }
    }
    eprintln!(
        "block {TRAINING_DUNGEON:04X}: {cyl_arm} placements take the cylsphere arm -- {cyl_only} \
         that carry no spherical collision shape at all and {cyl_and_spheres} that carry spheres the client never \
         looks at; a further {bsp_arm_with_cyl} carry cylspheres but take the part-BSP arm \
         instead; the largest cylsphere count on any of them is {most}"
    );
    // The census buckets a placement as "cylsphere-only" on `spheres.is_empty() &&
    // !cyl_spheres.is_empty()`, which does not exclude a part BSP. BSP presence has priority over
    // a nonempty cylinder list, so of those 88, **35 take the part-BSP arm** and only **53** reach
    // the cylinder arm.
    assert_eq!(
        cyl_only + bsp_arm_with_cyl,
        88,
        "the block holds 88 placements with a cylsphere and no spherical collision shape"
    );
    assert_eq!(
        cyl_arm, 53,
        "and 53 of them have no part BSP either, so they reach this arm"
    );
    assert_eq!(
        cyl_and_spheres, 0,
        "no placement of this block carries both, so 53 is also the whole of the arm's traffic \
         here -- elsewhere, three of the five shipped door setups carry both"
    );
}

/// Behaviour: objects.collision.a-cylsphere-only-object-stops-a-body
/// **Cylinder acceptance.** A training-dungeon object whose active collision volume is a cylinder
/// keeps every sampled body center outside it, where the same object with its cylinder list empty
/// (then it has no spheres and no part BSP either) lets the body walk straight through.
///
/// Both runs are this binary, this world and this object; the single variable is
/// `SetupGeometry::cyl_spheres`.
#[test]
fn a_cylsphere_only_dungeon_object_stops_a_body_that_used_to_walk_through_it() {
    let store = store();
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let src =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the retail height table"));
    src.load_block_cells(dereth_primitives::LandblockId(TRAINING_DUNGEON));

    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);
    let mut stats = SetupPartStats::default();

    let mut compared = 0usize;
    let mut worst = 0.0f32;
    let mut reported = false;
    for d in &cells {
        let Some(geom) = LandSource::env_cell(src.as_ref(), d.id) else {
            continue;
        };
        for s in cell_statics(d) {
            if s.id.0 >> 24 != 0x02 {
                continue;
            }
            let Some(decoded) = setup(&store, s.id) else {
                continue;
            };
            let with_cyl = Arc::new(setup_geometry_with_parts(&store, &decoded, &mut stats));
            // Subject: one of the 53 no-sphere, no-BSP placements with at least one cylinder.
            if !with_cyl.spheres.is_empty()
                || with_cyl.caches_physics_bsp()
                || with_cyl.cyl_spheres.is_empty()
            {
                continue;
            }
            // The control is the same object with the one field emptied: it then falls to the
            // sphere loop and runs zero iterations.
            let without = Arc::new(SetupGeometry {
                cyl_spheres: Vec::new(),
                ..(*with_cyl).clone()
            });

            let pos = Position::new(d.id, s.frame);
            let trial = |g: &Arc<SetupGeometry>, a: &Approach| -> Vec<Vec3> {
                let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
                register(&mut w, d.id, s.frame, Arc::clone(g));
                let h = spawn(&mut w, d.id, a.start, a.step);
                run_sampled(&mut w, h, 4.0)
            };
            let inside_frames = |path: &[Vec3]| {
                path.iter()
                    .filter(|p| inside_cylspheres(&with_cyl, &pos, 1.0, **p))
                    .count()
            };

            let mut chosen = None;
            for a in approaches(&geom, s.frame.origin, s.frame.origin.z) {
                if inside_cylspheres(&with_cyl, &pos, 1.0, a.start) {
                    continue; // the run-up starts inside the object; nothing to conclude
                }
                let through = trial(&without, &a);
                let end = *through.last().expect("frames");
                let travelled = math::hypotf(end.x - a.start.x, end.y - a.start.y);
                if travelled > 0.5 && inside_frames(&through) > 0 {
                    chosen = Some((a, through, travelled));
                    break;
                }
            }
            let Some((a, through, travelled)) = chosen else {
                continue;
            };
            let stopped = trial(&with_cyl, &a);
            let end_through = *through.last().expect("frames");
            let end_stopped = *stopped.last().expect("frames");
            assert_eq!(
                inside_frames(&stopped),
                0,
                "cell {:#010X} setup {:#010X}: the body entered the object's cylspheres, ending \
                 at {end_stopped:?}; the control's path went inside on {} of {} frames",
                d.id.0,
                s.id.0,
                inside_frames(&through),
                through.len()
            );
            let gained = math::hypotf(end_through.x - end_stopped.x, end_through.y - end_stopped.y);
            worst = worst.max(gained);
            if !reported {
                reported = true;
                eprintln!(
                    "cylsphere walk: cell {:#010X} setup {:#010X} ({} cylsphere(s)) at ({:.2}, {:.2}, \
                     {:.2}): from ({:.2}, {:.2}) the control walked {travelled:.2} m to \
                     ({:.2}, {:.2}), passing INSIDE the object's cylspheres on {} of {} frames; \
                     with the cylsphere arm the body walked {:.2} m to ({:.2}, {:.2}), never \
                     inside, and ended {gained:.3} m from where the control finished",
                    d.id.0,
                    s.id.0,
                    with_cyl.cyl_spheres.len(),
                    s.frame.origin.x,
                    s.frame.origin.y,
                    s.frame.origin.z,
                    a.start.x,
                    a.start.y,
                    end_through.x,
                    end_through.y,
                    inside_frames(&through),
                    through.len(),
                    math::hypotf(end_stopped.x - a.start.x, end_stopped.y - a.start.y),
                    end_stopped.x,
                    end_stopped.y,
                );
            }
            compared += 1;
        }
    }
    eprintln!(
        "{compared} cylsphere-only training-dungeon object(s) gave a usable approach; the largest \
         difference the arm made to where the body finished was {worst:.3} m"
    );
    assert!(
        compared >= 1,
        "no cylsphere-only object of block {TRAINING_DUNGEON:04X} gave a usable approach, so the \
         test is not measuring anything"
    );
}
