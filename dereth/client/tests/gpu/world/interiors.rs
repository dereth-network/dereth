//! Interiors are loaded, drawn and collided against: a body inside a building sees the interior
//! and is stopped by its walls. The walk test compares indoor trials in twenty Holtburg rooms
//! with an outdoor control and independently probes the geometry each frame (is the body inside a
//! wall?); the rendering test checks indoor residency, traversal and one lit frame. The last
//! tests are geometry oracles over the same rooms.
//!
//! Fixture: `client_cell_1.dat` and `client_portal.dat` (decoded environment cells and
//! environments), Holtburg's window, on a software device. Fails without the dats or a device.

#![cfg(gpu)]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_assets::Decode;
use dereth_client_runtime::character::CharacterInput;
use dereth_dat::RetailDatStore;
use dereth_physics::LandSource;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
use {
    dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene,
    dereth_world_data::landblock::DEFAULT_LANDBLOCK,
};
use {dereth_world_data::env_cells::physics_geometry, dereth_world_data::env_cells::EnvCellLoader};

/// The retail store, or a failed test.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn len(v: Vec3) -> f32 {
    v.x.mul_add(v.x, v.y.mul_add(v.y, v.z * v.z)).sqrt()
}

/// The body's own scaled collision spheres, matching the geometry its collision queries use.
fn body_spheres(store: &RetailDatStore) -> Vec<dereth_physics::geom::Sphere> {
    let id = dereth_client_runtime::character::ALUVIAN_MALE_SETUP;
    let bytes = store
        .read_typed(dereth_dat::DbType::Setup, id)
        .expect("the Aluvian male setup is in the retail dats");
    let setup =
        dereth_assets::Setup::decode_payload(id, &bytes).expect("the Aluvian male setup decodes");
    let g = dereth_world_data::setup::setup_geometry(&setup);
    let s = dereth_client_runtime::character::ALUVIAN_MALE_SCALE;
    g.path_spheres()
        .iter()
        .map(|x| {
            dereth_physics::geom::Sphere::new(
                Vec3::new(x.center.x * s, x.center.y * s, x.center.z * s),
                x.radius * s,
            )
        })
        .collect()
}

/// One interior cell of the block with **both** representations of what is solid in it.
///
/// The collision model uses the setups' spheres (`character::setup_geometry(...).path_spheres()`)
/// and the placed part BSPs (`object_physics::setup_geometry_with_parts`); about half of
/// Holtburg's placements are solid only through the second. Without it the oracle would call a
/// body wedged in a mesh "stopped by nothing", so both are needed to judge the geometry the body
/// meets.
struct CellObstacles {
    geom: dereth_physics::source::EnvCellGeometry,
    /// The statics' setup collision spheres, posed in landblock space.
    spheres: Vec<dereth_physics::geom::Sphere>,
    /// The same statics' placed part BSPs, through
    /// [`dereth_physics::source::SetupGeometry::placed_part`]: object frame composed with part frame.
    ///
    /// Registration is deliberately narrower than the collision world's shadow registration.
    /// This oracle files a static under its authored cell and checks it when the body's origin is
    /// inside that cell. The collision world registers every cell reached by the sorting sphere.
    /// The two agree for objects contained in one cell; a static crossing a boundary can differ
    /// (the training-dungeon door is one). Rebuilding shadow registration with a PhysicsWorld would make this oracle depend on the
    /// subject, so that boundary case remains an explicit limitation.
    parts: Vec<dereth_physics::source::PhysicsPart>,
}

/// Does a body sphere of radius `r` centred at `centre` (landblock space) meet the solid of any of
/// these placed parts?
///
/// The collision query's transform: inverse-transform the centre into the part frame, divide
/// centre and radius by the part's scale, then query BSP solid intersection. The current part
/// carries one scalar scale. Test the local bounding sphere first, as retail's broad-phase guard
/// does (it is a guard, not merely a speed optimization).
fn part_blocks(parts: &[dereth_physics::source::PhysicsPart], centre: Vec3, r: f32) -> bool {
    parts.iter().any(|part| {
        let Some(tree) = part.physics_bsp.as_ref() else {
            return false;
        };
        let Some(root) = part.physics_sphere() else {
            return false;
        };
        let inv = 1.0 / part.gfxobj_scale;
        let m = dereth_physics::math::l2g(part.pos.frame.rotation);
        let local = dereth_physics::math::globaltolocalvec(
            m,
            Vec3::new(
                centre.x - part.pos.frame.origin.x,
                centre.y - part.pos.frame.origin.y,
                centre.z - part.pos.frame.origin.z,
            ),
        );
        let ls = dereth_physics::geom::Sphere::new(
            Vec3::new(local.x * inv, local.y * inv, local.z * inv),
            r * inv,
        );
        dereth_physics::geom::bsp::spheres_intersect(&root, &ls)
            && tree.sphere_intersects_solid(&ls, false)
    })
}

/// Every Holtburg interior cell that has a standable point, with that point in **block-local**
/// space. Used by the collision test, which wants more than one room.
///
/// The cell's own membership BSP finds points inside the room rather than relying on a guessed
/// position. Even a point inside the cell can overlap a wall sconce or furniture, so the statics
/// are obstacles too: their setup spheres and their part BSPs.
fn rooms(store: &RetailDatStore, want: usize) -> Vec<(CellId, Vec3)> {
    let mut out = Vec::new();
    let mut loader = EnvCellLoader::new();
    let body = body_spheres(store);
    // The start chooser tests the body's spheres against every cell containing the point, using
    // cell membership and solid-intersection queries: a start free in its own cell can be solid
    // in a neighbour (`0xA9B40104`'s is), and a flood fill from it would expand no node and call
    // the room sealed.
    let all_cells = block_cells(store);
    for d in loader.load_block(store, DEFAULT_LANDBLOCK) {
        if out.len() >= want {
            break;
        }
        // The cell's authored static placements are expressed in block space. One list, built
        // once by `block_cells` with both representations, so a BSP-only static cannot be
        // invisible to the start-point chooser.
        let Some(here) = all_cells.iter().find(|c| c.geom.id == d.id) else {
            continue;
        };
        let g = physics_geometry(&d);
        let Some(bsp) = g.cell_bsp.as_ref() else {
            continue;
        };
        // Sample the cell's own space around its origin. A room is a few metres across, and the
        // frame origin sits at its reference corner.
        'cell: for &z in &[0.5f32, 0.75, 1.0, 1.25, 1.5] {
            for i in -10i8..=10 {
                for j in -10i8..=10 {
                    let local = Vec3::new(f32::from(i) * 0.5, f32::from(j) * 0.5, z);
                    if !bsp.point_inside_cell_bsp(local) {
                        continue;
                    }
                    // Not inside anything solid, either — a point in the wall is "in the cell".
                    //
                    // Query the body's spheres (two of 0.480 m at z 0.475 and 1.350) against
                    // solid geometry, as placement insertion does, rather than a zero-size point:
                    // a body that begins interpenetrating (head in the ceiling) cannot be
                    // inserted anywhere, stands still for the whole walk and would score as
                    // *confined*. The centres lie on the body's z axis (x=y=0), so yaw cannot
                    // move them and this check covers all four headings.
                    let world = dereth_physics::math::localtoglobal(&g.frame, local);
                    let (free_here, inside_here) = body_free_at(&all_cells, &body, world);
                    if !free_here || !inside_here {
                        continue;
                    }
                    // Nor inside the furniture, by both representations, so a start point cannot
                    // be chosen inside a table's mesh (as `A9B4013F`'s would be).
                    //
                    // Redundant given `body_free_at` above, which asks the same two questions of
                    // **every** cell the point is inside, this one included; removing the arm
                    // from `body_free_at` instead reddens. It stays as the statement about this
                    // cell's own statics, so a future change to `body_free_at`'s scope cannot
                    // silently drop it.
                    let clear = body.iter().all(|b| {
                        let wc = Vec3::new(
                            world.x + b.center.x,
                            world.y + b.center.y,
                            world.z + b.center.z,
                        );
                        here.spheres.iter().all(|o| {
                            let (dx, dy, dz) =
                                (wc.x - o.center.x, wc.y - o.center.y, wc.z - o.center.z);
                            let r = b.radius + o.radius;
                            dx.mul_add(dx, dy.mul_add(dy, dz * dz)) > r * r
                        }) && !part_blocks(&here.parts, wc, b.radius)
                    });
                    if !clear {
                        continue;
                    }
                    out.push((g.id, world));
                    break 'cell;
                }
            }
        }
    }
    out
}

/// The first such room, for the tests that only need one.
fn a_room(store: &RetailDatStore) -> Option<(CellId, Vec3)> {
    rooms(store, 1).into_iter().next()
}

/// Residency, as in retail: landblock prefetch loads environment cells; lookup only reports
/// what is already resident. Here env_cell must answer for a loaded block and not for a distant
/// unloaded block. Loading during lookup would block the physics step on disk I/O.
#[test]
fn env_cells_become_visible_because_the_landblock_path_prefetched_them() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());

    let stats = land.cell_stats();
    assert!(
        stats.loaded > 0,
        "no interior cell was prefetched for the loaded window"
    );
    assert_eq!(stats.undecodable, 0, "an environment cell would not decode");
    assert_eq!(
        stats.missing, 0,
        "num_cells named an id the cell dat does not carry"
    );
    assert_eq!(
        stats.no_environment, 0,
        "a cell's environment record is missing"
    );
    // With release accounting, resident==loaded holds only before anything leaves;
    // `CellLoadStats::released` is the release counter. This scene has not scrolled:
    // require zero releases and resident==loaded-released independently.
    assert_eq!(
        stats.released, 0,
        "nothing has scrolled off this window, so nothing was released"
    );
    assert_eq!(land.resident_cells() as u64, stats.loaded - stats.released);

    // Holtburg's own first interior cell is resident...
    let (room, _) = a_room(&store).expect("Holtburg has an interior cell");
    assert!(
        land.env_cell(room).is_some(),
        "{room:?} is in a loaded block and is not visible"
    );

    // A distant block was not prefetched, and resident lookup does not load it.
    let far = CellId(0x0102_0100);
    assert!(
        land.env_cell(far).is_none(),
        "env_cell loaded a block outside the window"
    );
}

/// Behaviour: world.interiors.a-body-inside-a-building-is-stopped-by-its-walls
///
/// Compare identical body/input/terrain setups indoors and outdoors, using DAT cell geometry.
/// Outdoor displacement proves the script can walk; per-frame penetration and independent
/// geometry probes below distinguish valid wall contact from spurious immobilization. Stopping
/// indoors by itself would not establish that a wall caused the stop.
#[test]
fn a_body_inside_a_holtburg_building_is_stopped_by_its_walls() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    // Twenty rooms, not one: Holtburg's interiors are a mixture of sealed rooms, halls that run
    // the length of a building, and covered walkways with an opening on every side. A test that
    // picked one would be measuring which kind it happened to get.
    let found = rooms(&store, 20);
    assert!(
        found.len() >= 20,
        "only {} standable rooms in Holtburg",
        found.len()
    );

    // Reuse one scene and teleport between trials, rather than building twenty worlds on one
    // device; this test does not measure the allocator's reclamation.
    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    // The shipped geometry of every interior cell of the block, so that each frame of the walk
    // can be asked the question this test is actually about: is the body inside a wall?
    let cells = block_cells(&store);
    let body = body_spheres(&store);

    // Run the same script from a position and report how far the body travelled, and **how far
    // into solid geometry it ever got**. "Confined" is a proxy; a body that ends up
    // inside a wall has passed through it whatever distance it covered, and a body that slides
    // along a wall and out of the door has not, however far it went.
    let walk = |scene: &mut WorldScene, start: Position| -> Trial {
        {
            let c = scene.character.as_mut().expect("a body");
            c.teleport(start);
            // Each trial needs independent contact state. Teleporting the reused scene leaves
            // sliding normals, contact planes and transient contact/sliding bits from the
            // previous wall; an anti-parallel normal can collapse the first crease-adjusted offset
            // before a geometry query, and zero completed substeps then refuse the transition (a
            // refusal keeps rotation, not translation). Resetting is trial hygiene.
            //
            // World departure applies transient mask 0xFFFFFE0B (`TransientState::CLEAR_MASK`),
            // which clears sliding. The explicit contact, normal, velocity and plane resets below
            // also restore fresh-body trial state.
            let h = c.handle;
            if let Some(o) = c.world.get_mut(h) {
                o.clear_transient_states();
                o.transient_state.set_contact(false);
                o.transient_state.set_on_walkable_bit(false);
                o.transient_state.set_sliding(false);
                o.transient_state.set_active_bit(true);
                o.sliding_normal = Vec3::ZERO;
                o.contact_plane = dereth_physics::geom::plane::Plane::default();
                o.contact_plane_cell_id = CellId(0);
                o.cached_velocity = Vec3::ZERO;
                o.velocity_vector = Vec3::ZERO;
                o.colliding_with_environment = false;
            }
        }
        scene.follow_character_now();
        let from = scene
            .character
            .as_ref()
            .expect("a body")
            .position()
            .frame
            .origin;
        // The body's own heading, as a direction, so that "how far did it get" can be asked
        // along the axis the geometry blocks rather than as a straight line from the start --
        // a body deflected sideways by a wall is displaced further from its start than one
        // walking into a doorway, which makes a plain distance metric a proxy.
        let hm = dereth_physics::math::l2g(start.frame.rotation);
        let hdir = dereth_physics::math::localtoglobalvec(hm, Vec3::new(0.0, 1.0, 0.0));
        let mut stalled = 0usize;
        let mut prev = from;
        let input = CharacterInput {
            forward: true,
            run: true,
            ..CharacterInput::default()
        };
        let step = dereth_physics::globals::MIN_QUANTUM;
        let mut now = 0.0f64;
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a fixed simulation step in seconds, narrowed for the debug camera's f32 delta.
        let dt = step as f32;
        let mut worst = 0.0f32;
        // Whether the body was *ever* outdoors, sampled every frame. See the
        // note on assertion 1: the cell the body happens to stand in when a fixed ten-second
        // clock runs out is not the same question as whether it got out of the building.
        let mut left_building = false;
        let mut first_pen = -1.0f32;
        let mut blame = CellId(0);
        let mut blame_own = CellId(0);
        for _ in 0..300 {
            now += step;
            scene.update(
                dereth_client_runtime::camera::CameraInput::default(),
                input,
                LocalTime(now),
                dt,
            );
            let pos = scene.character.as_ref().expect("a body").position();
            left_building |= dereth_physics::landdefs::is_outdoors(pos.cell);
            let o = pos.frame.origin;
            if len(Vec3::new(o.x - prev.x, o.y - prev.y, 0.0)) < 0.001 {
                stalled += 1;
            } else {
                stalled = 0;
            }
            prev = o;
            let (dep, who) = depth_in_solid(&cells, &body, pos.frame.origin);
            if dep > worst {
                worst = dep;
            }
            if dep > SOLID_TOLERANCE && first_pen < 0.0 {
                first_pen = dep;
                blame = who;
                blame_own = pos.cell;
            }
        }
        let c = scene.character.as_ref().expect("a body");
        let to = c.position().frame.origin;
        if worst > SOLID_TOLERANCE {
            eprintln!(
                "PENETRATION first={first_pen:.3} max={worst:.3} inside={:08X} body_cell_then={:08X} own={}",
                blame.0, blame_own.0, blame.0 == blame_own.0
            );
        }
        // Where the body finished, ask the shipped geometry -- and nothing else -- whether it could have kept going:
        // step along the body's **own heading** and require both that the body's spheres stay
        // clear of solid geometry and that the point is still inside an interior cell, so that
        // a body which walked out of the door and stopped on the terrain (which neither oracle
        // models) cannot read as stopped by nothing.
        let mut ahead = 0.0f32;
        for k in 1..=AHEAD_PROBES {
            #[allow(clippy::cast_precision_loss)] // a bounded probe counter
            let d = STEP_AHEAD * k as f32;
            let q = Vec3::new(to.x + hdir.x * d, to.y + hdir.y * d, to.z);
            let (free, inside) = body_free_at(&cells, &body, q);
            if free && inside {
                ahead = d;
            } else {
                break;
            }
        }
        Trial {
            moved: len(Vec3::new(to.x - from.x, to.y - from.y, 0.0)),
            end: c.position().cell,
            depth: worst,
            stalled,
            ahead,
            left_building,
        }
    };

    // The control: the same ten seconds on the open ground of the same landblock.
    let outdoor_cell = dereth_primitives::LandblockId(DEFAULT_LANDBLOCK).cell(1);
    let outdoors = walk(
        &mut scene,
        Position::new(
            outdoor_cell,
            Frame::new(Vec3::new(96.0, 96.0, found[0].1.z), Quat::IDENTITY),
        ),
    )
    .moved;
    eprintln!("CONTROL outdoors={outdoors:.3}");
    assert!(
        outdoors > 20.0,
        "the control run only covered {outdoors:.1} m, so the script is not walking"
    );

    // Every room, four headings each -- twice.
    //
    // At exactly 0/90/180/270 degrees axis-aligned wall contact gives a normal anti-parallel to
    // motion. The crease axis cross(floor_normal, wall_normal) has an exactly-zero dot product
    // with that motion in these IEEE computations, collapsing the offset; with zero completed
    // substeps the transition is refused, and a refusal restores the old origin while keeping
    // rotation. Both the zero- and one-degree passes require no penetration. The skewed pass
    // also calibrates the stalled and clear-ahead counts; the head-on pin is below.
    //
    // Which of the twenty rooms a body of this width can leave **by any route** (a flood fill)
    // is the judge that licenses a walk that ends outdoors: an escape from a room with no route
    // out would be a body passing through something; an escape from a room with one is a player
    // walking out of a door.
    let route_out: Vec<bool> = found
        .iter()
        .map(|&(_, at)| reachable_exit(&cells, &body, at, 24.0).0)
        .collect();
    let rooms_open = route_out.iter().filter(|x| **x).count();
    eprintln!("ROUTES open={rooms_open} of {}", found.len());

    let mut summary: Vec<Pass> = Vec::new();
    // Trial order is calibrated: leftover contact state from a previous trial can make one
    // subject give three answers in three runs. Both the zero- and one-degree passes run in
    // natural and fixed-shuffled order on the same scene with the same reset, compared trial by
    // trial.
    let seed = 0x0361_0361_u64;
    for &(skew_deg, order_name) in &[
        (0.0f32, ""),
        (0.0f32, "shuffled"),
        (1.0f32, ""),
        (1.0f32, "shuffled"),
    ] {
        let mut confined = 0usize;
        let mut trials = 0usize;
        let sealed;
        let mut in_solid = 0usize;
        let mut deepest = 0.0f32;
        let mut clear = 0usize;
        let mut clear_not_out = 0usize;
        let mut blocked = 0usize;
        let mut open = 0usize;
        let mut stuck = 0usize;
        // `stuck` is a conjunction, and a pinned conjunction alone leaves no reading that shows
        // the judge is alive. Each half is therefore counted on its own: some trials stall, some finish with a full
        // body-diameter free ahead of them, and the claim is that none does both. Two non-zero
        // components with a zero conjunction is a measurement; a zero conjunction with two zero
        // components is an instrument that never looked.
        let mut stalled_any = 0usize;
        let mut ahead_full_any = 0usize;
        let mut escaped_sealed = 0usize;
        let mut stuck_named: Vec<String> = Vec::new();
        // How many of the eighty never reached the outdoors at all, and which of
        // the clear-exit headings did not. `never_out` is assertion 1's **reverse calibration**:
        // a `left_building` stuck true would make assertion 1 unfailable, and this is what says so.
        let mut never_out = 0usize;
        let mut clear_held_named: Vec<String> = Vec::new();
        let mut per_room = vec![0usize; found.len()];
        let mut per_trial: Vec<TrialAnswer> = Vec::new();
        let plan: Vec<(usize, u32)> = {
            let natural: Vec<(usize, u32)> = (0..found.len())
                .flat_map(|r| (0..4u32).map(move |q| (r, q)))
                .collect();
            if order_name.is_empty() {
                natural
            } else {
                let mixed: Vec<(usize, u32)> = shuffled_order(natural.len(), seed)
                    .into_iter()
                    .map(|i| natural[i])
                    .collect();
                // Reject an identity-like shuffle so order-dependence cannot hide behind no reorder.
                // Require at least three quarters displaced. A uniform 80-element permutation has
                // one fixed point on average (79 displaced); this floor is deliberately lower.
                let moved = mixed.iter().zip(&natural).filter(|(a, b)| a != b).count();
                assert!(
                moved * 4 >= natural.len() * 3,
                "the shuffled order left {} of {} trials in their original position, so it is not \
                 a shuffle and the agreement below would mean nothing",
                natural.len() - moved,
                natural.len()
            );
                mixed
            }
        };
        {
            for &(r, quarter) in &plan {
                let (room, inside) = found[r];
                // Quarter-turn yaw about z, with skew applied to the entire set. Derive both the walk
                // and independent oracle directions from this same quaternion.
                #[allow(clippy::cast_precision_loss)] // 0..4
                let half = (quarter as f32)
                    .mul_add(std::f32::consts::FRAC_PI_4, skew_deg.to_radians() * 0.5);
                let heading = Quat::new(math::cosf(half), 0.0, 0.0, math::sinf(half));
                let t = walk(&mut scene, Position::new(room, Frame::new(inside, heading)));
                let (moved, cell, depth) = (t.moved, t.end, t.depth);
                let fate = straight_fate(&cells, inside, heading, &body, 40.0);
                trials += 1;
                if depth > deepest {
                    deepest = depth;
                }
                if depth > SOLID_TOLERANCE {
                    in_solid += 1;
                }
                let out = dereth_physics::landdefs::is_outdoors(cell);
                // A printed description, asserted nowhere: `moved < 12 m && !is_outdoors(end)`
                // is a threshold on a side effect, and it scores a body that never moved as
                // maximally *confined*.
                let conf = moved < 12.0 && !out;
                // Count a defect when the body has stalled for the last second despite a whole
                // body-diameter probe being free ahead along its heading. Requiring this count to
                // match the declared pin exposes spurious freezing rather than rewarding it.
                let did_stall = t.stalled >= STALL_FRAMES;
                let is_clear_ahead = t.ahead >= AHEAD_FULL - 1e-3;
                if did_stall {
                    stalled_any += 1;
                }
                if is_clear_ahead {
                    ahead_full_any += 1;
                }
                let stuck_here = did_stall && is_clear_ahead;
                eprintln!(
                "TRIAL skew={skew_deg} room={:08X} q={} moved={:.3} end={:08X} same={} outdoors={out} conf={conf} depth={depth:.3} stalled={} ahead={:.3} stuck={stuck_here}",
                room.0,
                quarter,
                moved,
                cell.0,
                cell.0 == room.0,
                t.stalled,
                t.ahead
            );
                if stuck_here {
                    stuck += 1;
                    stuck_named.push(format!("{:08X}/q{quarter}", room.0));
                }
                if !t.left_building {
                    never_out += 1;
                }
                // `left_building`, not `out`: a body that popped outside a sealed room and walked
                // back in passed through something just as surely as one that finished outside,
                // and reading only the last frame could not see it. On these twenty rooms the
                // flood fill gives every room a route out, so the two forms agree; assertion 4
                // pins that vacuity with `rooms_open == found.len()`.
                if t.left_building && !route_out[r] {
                    escaped_sealed += 1;
                    eprintln!(
                        "ESCAPEDSEALED skew={skew_deg} room={:08X} q={quarter} moved={moved:.3}",
                        room.0
                    );
                }
                if conf {
                    confined += 1;
                    per_room[r] += 1;
                }
                match fate {
                    Fate::ClearExit(exit_at) => {
                        clear += 1;
                        if !t.left_building {
                            clear_not_out += 1;
                            clear_held_named.push(format!("{:08X}/q{quarter}", room.0));
                        }
                        // Printed for every clear-exit heading, not only the failing ones, because
                        // the two numbers that matter here are the oracle's exit distance and how
                        // far the walk went -- see assertion 1.
                        eprintln!(
                        "CLEAREXIT skew={skew_deg} room={:08X} q={quarter} exit_at={exit_at:.3} moved={moved:.3} left={} end={:08X} end_outdoors={out}",
                        room.0, t.left_building, cell.0
                    );
                    }
                    Fate::Blocked(..) => blocked += 1,
                    Fate::Open => open += 1,
                }
                per_trial.push(TrialAnswer {
                    room: r,
                    quarter,
                    moved: t.moved,
                    stuck: stuck_here,
                });
            }
            sealed = per_room.iter().filter(|n| **n == 4).count();
        }
        eprintln!(
            "SUMMARY skew={skew_deg} order={} confined={confined}/{trials} in_solid={in_solid} \
deepest={deepest:.3} sealed={sealed}/{} oracle clear={clear} blocked={blocked} open={open}",
            if order_name.is_empty() {
                "natural"
            } else {
                order_name
            },
            found.len()
        );
        eprintln!(
            "JUDGED skew={skew_deg} order={} stuck_on_nothing={stuck}/{trials} [{}] \
clear_never_left={clear_not_out}/{clear} [{}] never_left_at_all={never_out}/{trials} \
escaped_a_sealed_room={escaped_sealed} stalled_at_all={stalled_any}/{trials} \
clear_ahead_at_all={ahead_full_any}/{trials}",
            if order_name.is_empty() {
                "natural"
            } else {
                order_name
            },
            stuck_named.join(" "),
            clear_held_named.join(" ")
        );
        assert_eq!(
            clear + blocked + open,
            trials,
            "the oracle did not answer for every trial"
        );
        summary.push(Pass {
            skew: skew_deg,
            order: order_name,
            per_trial,
            trials,
            confined,
            in_solid,
            clear,
            clear_not_out,
            clear_held_named,
            never_out,
            deepest,
            blocked,
            stuck,
            stalled_any,
            ahead_full_any,
            escaped_sealed,
        });
    }

    let at = |s: f32, order: &str| -> &Pass {
        summary
            .iter()
            .find(|r| (r.skew - s).abs() < 1e-6 && r.order == order)
            .unwrap_or_else(|| panic!("no {order:?} pass at {s} degrees"))
    };
    let one = at(1.0, "");
    let head = at(0.0, "");

    // Assertion 0 precedes the outcome judgments: each of the same eighty trials must agree
    // between natural and fixed-shuffled order. Otherwise the suite measures its own history.
    // Both heading skews are checked, rather than comparing only their aggregate totals.
    let mut moved_apart: Vec<String> = Vec::new();
    let mut worst_delta = 0.0f32;
    for skew in [0.0f32, 1.0] {
        let natural = at(skew, "");
        let mixed = at(skew, "shuffled");
        for a in &natural.per_trial {
            let (r, q) = (a.room, a.quarter);
            let Some(b) = mixed
                .per_trial
                .iter()
                .find(|x| x.room == r && x.quarter == q)
            else {
                panic!("the shuffled pass at {skew} degrees did not run room {r} q{q}");
            };
            let delta = (a.moved - b.moved).abs();
            if delta > worst_delta {
                worst_delta = delta;
            }
            // One metre is 1000 times the 0.001 m stall threshold and about 1/40 of an
            // unobstructed 40 m walk. Print the worst delta rather than hiding a near miss;
            // compare stuck exactly because assertion 5 reads that boolean.
            if delta > 1.0 || a.stuck != b.stuck {
                moved_apart.push(format!(
                    "{:08X}/q{q} at {skew} deg: {:.3} m stuck={} in room order, {:.3} m stuck={} \
                     shuffled",
                    found[r].0 .0, a.moved, a.stuck, b.moved, b.stuck
                ));
            }
        }
    }
    eprintln!(
        "SHUFFLE disagreements={} worst_delta={worst_delta:.3} [{}]",
        moved_apart.len(),
        moved_apart.join("; ")
    );
    assert!(
        moved_apart.is_empty(),
        "{} of {} trials per pass answer differently when the eighty are run in a shuffled order \
         (worst {worst_delta:.3} m), so the suite is measuring its own history and no per-trial \
         figure from it is that trial's own: {}",
        moved_apart.len(),
        one.per_trial.len(),
        moved_apart.join("; ")
    );
    let (trials, in_solid, clear, deepest, blocked) = (
        one.trials,
        one.in_solid,
        one.clear,
        one.deepest,
        one.blocked,
    );
    let (head_confined, head_in_solid, head_deepest, head_blocked) =
        (head.confined, head.in_solid, head.deepest, head.blocked);

    // The judges ask the shipped geometry through `straight_fate`, without running the
    // transition collision resolver under test.
    //
    // Assertion 1: a heading with a clear body-width route straight out must reach outdoors at
    // some frame. `straight_fate` returns `ClearExit(since)` as soon as the marched body has been
    // outside every interior cell for [`CLEAR_RUN`] = 1 m -- a question about the **first
    // metre** past the doorway -- so the walk is asked whether it ever got outside, not which
    // cell it stands in when the 300-frame clock stops some 40 m of running later (by then it
    // may have climbed into another building's cells). This uses the one-degree pass; the
    // head-on checks are separate. It is not satisfiable by a body that never moved: a body that
    // starts indoors and stays there is never outdoors.
    assert_eq!(
        one.clear_not_out,
        0,
        "{} of the {clear} headings with a clear body-width line out of the building never got \
         out of it at any frame ({}): the build is stopping bodies retail lets through",
        one.clear_not_out,
        one.clear_held_named.join(" ")
    );
    assert!(
        clear > 0,
        "no heading has a clear line out, so assertion 1 is vacuous"
    );
    // Reverse calibration: left_building is an OR over 300 frames. If stuck true, it would make
    // assertion 1 unfailable, so both heading passes must also contain trials that never leave.
    // Most of the eighty never leave; the check is a positive floor rather than an equality, so
    // a legitimate correction need not remove the calibration.
    assert!(
        one.never_out > 0 && head.never_out > 0,
        "every one of the {trials} trials reached the outdoors at some frame ({} at one degree \
         and {} at zero never did), so `left_building` cannot read false and assertion 1 above \
         is unfailable",
        one.never_out,
        head.never_out
    );

    // **The oracle's own answer, pinned.** Without the part-BSP arm of `straight_fate`, **9**
    // head-on headings have a clear body-width line out of the building instead of **8**,
    // because one of the twenty rooms' lines runs through a static whose only geometry is a part
    // BSP; nothing else in this file would see the oracle change. A judge nobody checks is not a
    // judge.
    //
    // These counts describe the shipped geometry and body under cell-membership and solid
    // queries, independent of the transition code; a changed count means the inputs or the
    // instrument changed.
    assert_eq!(
        (head.clear, one.clear),
        (8, 7),
        "the shipped geometry gives {} headings at 0 degrees and {} at 1 degree a clear \
         body-width straight line out of the building, against 8 and 7 with both the cell \
         shells and the statics' part BSPs counted",
        head.clear,
        one.clear
    );

    // Assertion 2 requires no penetration on the one-degree pass.
    //
    // Animation displacement is zeroed when the walkable transient flag is clear, but the
    // animation and movement forwarding paths do not inspect a transition result; retail stops a
    // body at a wall at hand-set headings rather than creeping. The independent geometry station
    // is `dereth-physics`'s two-sided door test.
    assert_eq!(
        in_solid, 0,
        "{in_solid} of {trials} one-degree runs walked more than {SOLID_TOLERANCE} m into solid \
         geometry (deepest {deepest:.3} m); the geometry blocks {blocked} of them"
    );

    // **3. The head-on pass.**
    //
    // With the correct world normal the axis-aligned crease projection cancels exactly, and zero
    // completed substeps refuse the transition. A refused transition restores the old origin
    // before committing the requested frame (retail does; ACE's `PhysicsObj.UpdateObjectInternal`
    // agrees): rotation is kept and translation discarded, holding the body at the wall. So the
    // head-on pass penetrates nothing, like the oblique ones. `dereth-physics`'s failed-transition
    // test is the focused refusal case; this checks the integrated wall behaviour.
    assert_eq!(
        head_in_solid, 0,
        "the exactly-perpendicular pass put {head_in_solid} of {trials} bodies inside solid \
         geometry (deepest {head_deepest:.3} m, {head_confined} confined of {head_blocked} \
         blocked). A refused transition must not translate the body"
    );

    // Assertion 4: escape is suspicious when no body-width route exists. The flood fill finds
    // all twenty sampled rooms open, so this sealed-room implication is vacuous on this set;
    // route existence alone does not prove the actual trajectory followed that route.
    // Independent per-frame depth checks remain essential. A wider-body calibration below
    // demonstrates that the fill can also report rooms that hold a body which fits inside.
    assert_eq!(rooms_open, found.len(), "the flood fill says {rooms_open} of {} rooms have a route out; with fewer, assertion 4 below is not vacuous and its count must be read", found.len());
    assert_eq!(
        one.escaped_sealed + head.escaped_sealed,
        0,
        "a body left a room the shipped geometry gives it no route out of, so it passed \
         through something"
    );

    // **5. How many bodies were stopped by nothing.**
    //
    // A confinement floor (`confined` is `moved < 12 m && !is_outdoors(end)`) would score a
    // spuriously **stuck** body as maximally confined and reward the defect. Instead: did the
    // body stall for the last second while a full body-diameter probe ahead is free and indoors?
    // A spurious freeze raises this count.
    //
    // The constants below pin one stuck trial on each pass. They are defect pins, not targets:
    // a change either way is read from the named trials.
    assert_eq!(
        (one.stuck, head.stuck),
        (STUCK_ON_NOTHING_SKEWED, STUCK_ON_NOTHING_HEAD_ON),
        "bodies stopped by nothing: {} at one degree and {} at zero, against \
         {STUCK_ON_NOTHING_SKEWED} and {STUCK_ON_NOTHING_HEAD_ON} on the landed composition. A \
         fall is a defect leaving and a rise is one arriving; either way read the \
         named trials rather than the count. Confined-by-the-old-metric was {} of {trials} at one \
         degree against the {blocked} the geometry blocks, and the same script covers \
         {outdoors:.1} m outdoors.",
        one.stuck,
        head.stuck,
        one.confined
    );

    // Assertion 5 calibration counts the two halves separately: stalled>=STALL_FRAMES and
    // ahead>=AHEAD_FULL. Two independently positive halves distinguish a working judge from one
    // that never looked. This calibration is on the skewed pass; the head-on halves are pinned
    // exactly below.
    assert!(
        one.stalled_any >= STALLED_AND_CLEAR_ARE_BOTH_LIVE
            && one.ahead_full_any >= STALLED_AND_CLEAR_ARE_BOTH_LIVE,
        "the one-degree pass has {} trials that stalled for {STALL_FRAMES} frames and {} that \
         finished with a full {AHEAD_FULL:.3} m free along their own heading, of {}; `stuck` is \
         the conjunction of those two and it cannot be read as 0 unless both halves are shown to \
         fire (the 0-degree pass reads {} and {}, pinned below)",
        one.stalled_any,
        one.ahead_full_any,
        one.trials,
        head.stalled_any,
        head.ahead_full_any,
    );

    // Pin the head-on halves: they are the only reading that sees the second-sphere slide's
    // local-to-world contact-normal transform. The slide is reached about 1,220 times on this
    // walk with four normals; rotation changes three of them, and the fourth, (0, 0, -1), is
    // fixed by z rotations (it accounts for all of `world::cell_statics`'s hits, which therefore
    // cannot distinguish the transform). Without the transform the pair reads 65 and 2 against
    // 67 and 1.
    //
    // These are composition pins: a change requires interpreting the readings, not adjusting an
    // unexplained expected number, and a re-pinned value must be re-checked against the same
    // deletion.
    assert_eq!(
        (head.stalled_any, head.ahead_full_any),
        (HEAD_ON_STALLED, HEAD_ON_CLEAR_AHEAD),
        "the 0-degree pass has {} trials that stalled and {} that finished with a full \
         {AHEAD_FULL:.3} m free ahead, against {HEAD_ON_STALLED} and {HEAD_ON_CLEAR_AHEAD} on the \
         landed composition. Deleting the rotation in the BSP collision slide reads \
         65 and 2, and this is the only \
         assertion in the tree that can see it",
        head.stalled_any,
        head.ahead_full_any,
    );
}

/// Interior traversal reaches resident room geometry and produces a visible frame. This checks
/// the initial outdoor traversal count, teleports indoors, checks the reached geometry, then
/// renders one frame with more than half its pixels lit. It does not render an outdoor comparison
/// frame or isolate interior-only pixels.
#[test]
fn standing_inside_a_building_draws_its_interior() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let (room, inside) = a_room(&store).expect("Holtburg has an interior cell");

    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let (resident, _) = scene.env_cell_counts();
    assert!(resident > 0, "the scene baked no interior geometry at all");

    // The initial outdoor viewer has not selected an indoor traversal.
    assert_eq!(scene.env_cell_counts().1, 0, "the body starts outdoors");

    // Stand in the room, and put the camera in it too.
    //
    // The camera is placed explicitly, to inspect drawing rather than the collision-aware camera
    // sweep's choice; the gpu camera tests' `a_frame_from_inside_a_holtburg_room_is_of_the_room`
    // covers it without the manual placement. Before the sweep has a viewer cell, traversal falls back to the body's cell.
    scene
        .character
        .as_mut()
        .expect("a body")
        .teleport(Position::new(room, Frame::new(inside, Quat::IDENTITY)));
    scene.follow_character_now();
    assert!(
        scene.env_cell_counts().1 > 0,
        "the viewer is in {room:?} and the indoor path found no geometry for it"
    );

    let body = scene.character.as_ref().expect("a body").render_frame();
    scene.camera.position = Vec3::new(body.origin.x, body.origin.y, body.origin.z + 1.2);
    gpu.begin_frame().expect("begin");
    scene.draw(&mut gpu).expect("draw");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    let lit = image
        .to_rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) > 24)
        .count();
    let total = (image.width * image.height) as usize;
    assert!(
        lit * 2 > total,
        "only {lit} of {total} pixels are lit from inside the room: the interior is not drawn"
    );
}

/// Land-cell transit includes outdoor neighbours and building portals; a building
/// portal can add its interior cell to the candidate list. Without that branch the list contains
/// only land cells, preventing normal entry or collision with the building from outside.
/// This constructed placement checks cell reassociation while standing still, not a complete
/// walk through the doorway.
#[test]
fn a_body_standing_where_a_building_is_transits_into_its_interior_cell() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let (room, inside) = a_room(&store).expect("Holtburg has an interior cell");

    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    // Deliberately pair the interior point with its outdoor cell id. This exercises the building
    // transit candidate path; it is a constructed inconsistent placement, not a natural doorway walk.
    let outdoor = dereth_physics::landdefs::get_outside_cell_id(room, inside);
    assert!(
        dereth_physics::landdefs::is_outdoors(outdoor),
        "get_outside_cell_id did not produce a land cell"
    );
    {
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(outdoor, Frame::new(inside, Quat::IDENTITY)));
    }

    // Run thirty empty-input ticks. The object's clock restarts at zero after teleport, so the
    // first tick charges no elapsed time. Position/collision updates can change its cell even
    // though no walking input is supplied.
    let step = dereth_physics::globals::MIN_QUANTUM;
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a fixed simulation step in seconds, narrowed for the debug camera's f32 delta.
    let dt = step as f32;
    let mut now = 0.0f64;
    for _ in 0..30 {
        now += step;
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            dt,
        );
    }

    let cell = scene.character.as_ref().expect("a body").position().cell;
    assert_eq!(
        cell, room,
        "the body standing in {room:?} was left in {cell:?}: the building-portal half of \
         the land-cell transit search did not run"
    );
}

// ---------------------------------------------------------------------------------------------
// The geometry oracle the walks are judged by
// ---------------------------------------------------------------------------------------------

/// A straight march judged from shipped cell membership, shell BSPs and baked static geometry.
/// It uses point-membership and sphere-solid queries, not the transition collision resolver
/// whose output this suite judges. The geometry loaders are shared; the transition path is not.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Fate {
    /// The body's own spheres met solid geometry this far along the heading, in the named
    /// cell, against its `physics_bsp` (`true`) or against one of its baked statics.
    Blocked(f32, CellId, bool),
    /// The body's own spheres left every interior cell of the block, at body width, without ever
    /// meeting solid geometry: a clear straight line out through a doorway.
    ClearExit(f32),
    /// Neither, within the distance the outdoor control run covers.
    Open,
}

/// Every interior cell with its shell geometry and statics' placed spheres and part BSPs.
fn block_cells(store: &RetailDatStore) -> Vec<CellObstacles> {
    let mut loader = EnvCellLoader::new();
    let mut out = Vec::new();
    let mut stats = dereth_world_data::setup::SetupPartStats::default();
    for d in loader.load_block(store, DEFAULT_LANDBLOCK) {
        let mut spheres: Vec<dereth_physics::geom::Sphere> = Vec::new();
        let mut parts: Vec<dereth_physics::source::PhysicsPart> = Vec::new();
        for s in dereth_world_data::env_cells::cell_statics(&d) {
            // Both representations, from the same loaders as the scene. A 0x01-prefixed
            // graphics-object id takes the simple-setup route; other ids decode a setup and load
            // its parts. Sphere-less geometry must not disappear from the oracle.
            let g = if s.id.0 >> 24 == 0x01 {
                dereth_world_data::setup::simple_setup_geometry(store, s.id, &mut stats)
            } else {
                store
                    .read_typed(dereth_dat::DbType::Setup, s.id)
                    .ok()
                    .and_then(|b| dereth_assets::Setup::decode_payload(s.id, &b).ok())
                    .map(|setup| {
                        dereth_world_data::setup::setup_geometry_with_parts(
                            store, &setup, &mut stats,
                        )
                    })
            };
            let Some(g) = g else { continue };
            let m = dereth_physics::math::l2g(s.frame.rotation);
            for x in g.path_spheres() {
                let c = dereth_physics::math::localtoglobalvec(m, x.center);
                spheres.push(dereth_physics::geom::Sphere::new(
                    Vec3::new(
                        c.x + s.frame.origin.x,
                        c.y + s.frame.origin.y,
                        c.z + s.frame.origin.z,
                    ),
                    x.radius,
                ));
            }
            // Static placement uses scale 1.0. `placed_part` composes the
            // object's frame with the authored part frame at that scale.
            let pos = Position::new(d.id, s.frame);
            for i in 0..g.parts.len() {
                if let Some(pp) = g.placed_part(i, &pos, 1.0) {
                    if pp.physics_bsp.is_some() {
                        parts.push(pp);
                    }
                }
            }
        }
        out.push(CellObstacles {
            geom: physics_geometry(&d),
            spheres,
            parts,
        });
    }
    out
}

/// March the body's **own** spheres, in the body's **own** pose, along `heading` from `start`.
///
/// Derive both travel direction and sphere offsets from the heading quaternion instead of
/// assuming an axis from the trial index.
///
/// "Left the building" is only reported after `CLEAR_RUN` metres outside every cell, so the few
/// centimetres of no-man's-land at a portal boundary cannot be mistaken for an exit.
fn straight_fate(
    cells: &[CellObstacles],
    start: Vec3,
    heading: Quat,
    body: &[dereth_physics::geom::Sphere],
    limit: f32,
) -> Fate {
    const STEP: f32 = 0.05;
    // On these eighty headings 0.05 m and 1.0 m give the same answers, so the sampled marches do
    // not distinguish this guard; it stays for tighter portal geometry.
    const CLEAR_RUN: f32 = 1.0;
    let m = dereth_physics::math::l2g(heading);
    let dir = dereth_physics::math::localtoglobalvec(m, Vec3::new(0.0, 1.0, 0.0));
    let posed: Vec<dereth_physics::geom::Sphere> = body
        .iter()
        .map(|b| {
            dereth_physics::geom::Sphere::new(
                dereth_physics::math::localtoglobalvec(m, b.center),
                b.radius,
            )
        })
        .collect();
    let mut out_since: Option<f32> = None;
    let mut i = 0i32;
    loop {
        #[allow(clippy::cast_precision_loss)] // a bounded march counter
        let d = STEP * (i as f32);
        if d > limit {
            return Fate::Open;
        }
        let p = Vec3::new(
            start.x + dir.x * d,
            start.y + dir.y * d,
            start.z + dir.z * d,
        );
        let mut inside_any = false;
        for c in cells {
            let g = &c.geom;
            let Some(cb) = g.cell_bsp.as_ref() else {
                continue;
            };
            if !cb.point_inside_cell_bsp(dereth_physics::math::globaltolocal(&g.frame, p)) {
                continue;
            }
            inside_any = true;
            for b in &posed {
                let wc = Vec3::new(p.x + b.center.x, p.y + b.center.y, p.z + b.center.z);
                if let Some(pb) = g.physics_bsp.as_ref() {
                    let lc = dereth_physics::math::globaltolocal(&g.frame, wc);
                    if pb.sphere_intersects_solid(
                        &dereth_physics::geom::Sphere::new(lc, b.radius),
                        false,
                    ) {
                        return Fate::Blocked(d, g.id, true);
                    }
                }
                for o in &c.spheres {
                    let (dx, dy, dz) = (wc.x - o.center.x, wc.y - o.center.y, wc.z - o.center.z);
                    let r = b.radius + o.radius;
                    if dx.mul_add(dx, dy.mul_add(dy, dz * dz)) <= r * r {
                        return Fate::Blocked(d, g.id, false);
                    }
                }
                // A BSP-only static stops the body and contributes no sphere, so without this
                // arm the march walks through it.
                if part_blocks(&c.parts, wc, b.radius) {
                    return Fate::Blocked(d, g.id, false);
                }
            }
        }
        if inside_any {
            out_since = None;
        } else {
            let since = *out_since.get_or_insert(d);
            if d - since >= CLEAR_RUN {
                return Fate::ClearExit(since);
            }
        }
        i += 1;
    }
}

/// How deep the body's own spheres are inside solid geometry at `p`, in metres, or `0.0`.
///
/// Contact alone is not penetration. Bisect the radius with a fixed sphere centre to find the
/// smallest radius that still intersects shell or part-BSP solid; original radius minus that
/// radius estimates depth. This function omits the separate static-sphere list, unlike the free-
/// space helpers, and its ten iterations limit resolution.
fn depth_in_solid(
    cells: &[CellObstacles],
    body: &[dereth_physics::geom::Sphere],
    p: Vec3,
) -> (f32, CellId) {
    let mut worst = 0.0f32;
    let mut who = CellId(0);
    for c in cells {
        let g = &c.geom;
        let Some(cb) = g.cell_bsp.as_ref() else {
            continue;
        };
        if !cb.point_inside_cell_bsp(dereth_physics::math::globaltolocal(&g.frame, p)) {
            continue;
        }
        for b in body {
            let wc = Vec3::new(p.x + b.center.x, p.y + b.center.y, p.z + b.center.z);
            let lc = dereth_physics::math::globaltolocal(&g.frame, wc);
            // Placed part BSPs as well as the cell shell. The body's origin p and
            // sphere offset b.center stay fixed during bisection; only query radius changes in
            // both representations.
            let hit = |r: f32| {
                g.physics_bsp.as_ref().is_some_and(|pb| {
                    pb.sphere_intersects_solid(&dereth_physics::geom::Sphere::new(lc, r), false)
                }) || part_blocks(&c.parts, wc, r)
            };
            if !hit(b.radius) {
                continue;
            }
            let (mut lo, mut hi) = (0.0f32, b.radius);
            for _ in 0..10 {
                let mid = f32::midpoint(lo, hi);
                if hit(mid) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            if b.radius - hi > worst {
                worst = b.radius - hi;
                who = g.id;
            }
        }
    }
    (worst, who)
}

/// Contact alone is not a defect. The 0.05 m penetration threshold is about one tenth of the
/// body's 0.480 m radius and 250 times the 0.0002 slide-degeneracy guard; those are distinct tolerances.
const SOLID_TOLERANCE: f32 = 0.05;

/// Approximate per-frame distance of an unobstructed 300-frame walk at MIN_QUANTUM (about
/// 39.9 m), used as the fixed spacing of the forward geometry probe.
const STEP_AHEAD: f32 = 0.133;

/// How many of those steps the probe looks ahead. Eight of them is 1.064 m, which is more
/// than the body's own **diameter** (2 x 0.480 m) -- so "free ahead" means the whole body
/// could stand one body-length further on, not that it could edge forward.
const AHEAD_PROBES: u32 = 8;

/// The full probe, in metres.
const AHEAD_FULL: f32 = STEP_AHEAD * AHEAD_PROBES as f32;

/// Thirty consecutive frames whose individual horizontal displacement is below one millimetre.
/// At MIN_QUANTUM this is one second; it does not require total displacement below a millimetre.
const STALL_FRAMES: usize = 30;

/// The head-on pass puts no body inside solid geometry; assertion 3 checks the zero directly.
#[allow(dead_code)]
const HEAD_ON_IN_SOLID: usize = 0;

/// The head-on stalled/clear-ahead pair, which is what sees the second contact-normal rotation
/// (part space to world space during second-sphere sliding); see the assertion above. A refused
/// transition keeps the body at the wall, so most head-on trials stall. The one trial that stalls
/// with a full probe free ahead is `A9B40100/q3`, resting at about 5.25 m with 1.064 m clear:
/// transitional insertion returns the state edge sliding wrote, so validation restores the last
/// contact plane and undoes the substep there, and the body stops in the open.
const HEAD_ON_STALLED: usize = 67;
const HEAD_ON_CLEAR_AHEAD: usize = 1;

/// Stuck trials on the head-on pass. Separate constants keep differences between the two passes
/// visible rather than hiding them in one number. Both passes name the same trial,
/// `A9B40100/q3`: a remaining crease-adjustment defect (the body stops in the open, at depth
/// 0.000 m on every frame), pinned rather than targeted.
const STUCK_ON_NOTHING_HEAD_ON: usize = 1;

/// Stuck trials on the one-degree pass: `A9B40100/q3`, which walks about 5.23 m and stalls for
/// most of the 300 frames with 1.064 m clear ahead and depth 0.000 m throughout. A body stopped
/// by nothing is the same shape of defect as having to jump to climb a stair, seen from the other
/// end. Each trial's placement commits with its actual starting contact, so the count is the
/// trial's own and not the previous position's.
const STUCK_ON_NOTHING_SKEWED: usize = 1;

/// Positive floor for each component of stuck: neither predicate may be dead. A floor rather than
/// a pin of either count, so a legitimate correction need not remove the calibration.
const STALLED_AND_CLEAR_ARE_BOTH_LIVE: usize = 1;

/// What one walk reports back.
struct Trial {
    /// Straight-line distance from the start, in the horizontal plane.
    moved: f32,
    /// The cell the body ends in.
    end: CellId,
    /// The deepest the body's spheres ever got inside solid geometry.
    depth: f32,
    /// How many frames the body finished without moving.
    stalled: usize,
    /// How far along its own heading the shipped geometry leaves it free, and still inside
    /// the building, from where it stopped -- capped at [`AHEAD_FULL`].
    ahead: f32,
    /// **Whether the body ever got out**, at any frame of the walk, rather than whether it
    /// happened to be outside when the clock stopped; see assertion 1.
    left_building: bool,
}

/// A deterministic permutation of `0..n` -- Fisher-Yates driven by a fixed 64-bit LCG (Knuth's
/// multiplier), so the shuffled pass below is the same eighty trials in the same order on every
/// run and a disagreement can be reproduced and bisected. Not `rand`: a test whose order changes
/// between runs cannot be the judge of an order-dependence claim.
fn shuffled_order(n: usize, seed: u64) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    let mut state = seed;
    let mut i = n;
    while i > 1 {
        i -= 1;
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        #[allow(clippy::cast_possible_truncation)] // an index into a bounded vector
        let j = ((state >> 33) as usize) % (i + 1);
        v.swap(i, j);
    }
    v
}

/// One trial's own answer, with the key the shuffled pass is compared on.
#[derive(Clone, Copy)]
struct TrialAnswer {
    room: usize,
    quarter: u32,
    moved: f32,
    stuck: bool,
}

/// One eighty-trial pass, at one heading skew.
struct Pass {
    skew: f32,
    /// Which order the eighty trials ran in -- `""` for the natural one, a name for a shuffle.
    order: &'static str,
    /// Per-trial answers compare the same eighty trials by identity after shuffling,
    /// instead of allowing changed individual outcomes to cancel in an aggregate total.
    per_trial: Vec<TrialAnswer>,
    trials: usize,
    confined: usize,
    in_solid: usize,
    clear: usize,
    clear_not_out: usize,
    /// Which clear-exit headings never got out.
    clear_held_named: Vec<String>,
    /// How many of the eighty never reached the outdoors at any frame: assertion 1's reverse
    /// calibration.
    never_out: usize,
    deepest: f32,
    blocked: usize,
    stuck: usize,
    /// How many trials stalled for [`STALL_FRAMES`], and how many finished with a full
    /// [`AHEAD_FULL`] of free in-building space along their own heading. The two halves of
    /// `stuck`, counted separately so a zero conjunction can be told from a dead judge.
    stalled_any: usize,
    ahead_full_any: usize,
    escaped_sealed: usize,
}

/// Calibration for the representation the floor and open-ground probes cannot exercise: a static
/// whose only geometry is a part BSP. Every representation needs a known case that exercises it.
///
/// Find sphere-less statics with part BSPs. Bounded searches must find some solid points and a
/// far-away negative; for each successfully probed static, at least one solid point must also
/// be called free by the old sphere-only arm. Not every point of the bounding sphere is solid.
#[test]
fn the_free_space_oracle_can_see_a_static_that_has_no_spheres_at_all() {
    let store = store();
    let cells = block_cells(&store);
    let body = body_spheres(&store);
    let r = body.iter().map(|b| b.radius).fold(0.0f32, f32::max);
    assert!(r > 0.0, "the body has no radius");

    // Census placements with no path spheres but a cached physics BSP: the representation a
    // sphere-only oracle cannot see.
    let mut placements = 0usize;
    let mut bsp_only: Vec<(
        CellId,
        dereth_primitives::DataId,
        dereth_physics::source::PhysicsPart,
    )> = Vec::new();
    let mut stats = dereth_world_data::setup::SetupPartStats::default();
    let mut loader = EnvCellLoader::new();
    for d in loader.load_block(&store, DEFAULT_LANDBLOCK) {
        for st in dereth_world_data::env_cells::cell_statics(&d) {
            placements += 1;
            let g = if st.id.0 >> 24 == 0x01 {
                dereth_world_data::setup::simple_setup_geometry(&store, st.id, &mut stats)
            } else {
                store
                    .read_typed(dereth_dat::DbType::Setup, st.id)
                    .ok()
                    .and_then(|b| dereth_assets::Setup::decode_payload(st.id, &b).ok())
                    .map(|setup| {
                        dereth_world_data::setup::setup_geometry_with_parts(
                            &store, &setup, &mut stats,
                        )
                    })
            };
            let Some(g) = g else { continue };
            if !g.path_spheres().is_empty() || !g.caches_physics_bsp() {
                continue;
            }
            let pos = Position::new(d.id, st.frame);
            for i in 0..g.parts.len() {
                if let Some(pp) = g.placed_part(i, &pos, 1.0) {
                    if pp.physics_bsp.is_some() {
                        bsp_only.push((d.id, st.id, pp));
                    }
                }
            }
        }
    }
    let all_parts: usize = cells.iter().map(|c| c.parts.len()).sum();
    // Count nonunit scalar part scales: the local-space division can only be distinguished
    // by these inputs when a scale differs from 1.0.
    let scaled = cells
        .iter()
        .flat_map(|c| &c.parts)
        .filter(|p| (p.gfxobj_scale - 1.0).abs() > 1e-6)
        .count();
    eprintln!(
        "BSPCENSUS placements={placements} placed_part_bsps={all_parts} \
bsp_only_parts={} (statics with no collision sphere at all) parts_not_at_unit_scale={scaled}",
        bsp_only.len()
    );
    assert!(
        !bsp_only.is_empty(),
        "Holtburg has no BSP-only static, so this calibration cannot be taken here; it must be \
         taken somewhere it can be -- an uncalibrated oracle judges nothing"
    );

    // The known positive, in both directions, for each of the first few such statics.
    let mut probed = 0usize;
    let mut solid_and_sphere_free = 0usize;
    for (cell, setup, part) in bsp_only.iter().take(8) {
        let Some(root) = part.physics_sphere() else {
            continue;
        };
        let m = dereth_physics::math::l2g(part.pos.frame.rotation);
        let sc = part.gfxobj_scale;
        let c = dereth_physics::math::localtoglobalvec(
            m,
            Vec3::new(root.center.x * sc, root.center.y * sc, root.center.z * sc),
        );
        let centre = Vec3::new(
            c.x + part.pos.frame.origin.x,
            c.y + part.pos.frame.origin.y,
            c.z + part.pos.frame.origin.z,
        );
        let here = cells
            .iter()
            .find(|x| x.geom.id == *cell)
            .expect("the static's own cell");

        // Positive: somewhere within the part's own bounding sphere the mesh is solid to a body
        // sphere. A bounding-sphere centre is not necessarily inside the mesh -- a doorway's is
        // not -- so this is a bounded search rather than a single point, and its count is stated.
        let mut hits = 0usize;
        let mut sphere_blind = 0usize;
        let step = (root.radius * part.gfxobj_scale / 4.0).max(0.1);
        for i in -4i32..=4 {
            for j in -4i32..=4 {
                for k in -4i32..=4 {
                    #[allow(clippy::cast_precision_loss)] // -4..=4
                    let q = Vec3::new(
                        centre.x + i as f32 * step,
                        centre.y + j as f32 * step,
                        centre.z + k as f32 * step,
                    );
                    if !part_blocks(&here.parts, q, r) {
                        continue;
                    }
                    hits += 1;
                    // ... and a sphere-only arm says the same point is free: the blindness this
                    // calibration exists to show.
                    let sphere_free = here.spheres.iter().all(|o| {
                        let (dx, dy, dz) = (q.x - o.center.x, q.y - o.center.y, q.z - o.center.z);
                        let rr = r + o.radius;
                        dx.mul_add(dx, dy.mul_add(dy, dz * dz)) > rr * rr
                    });
                    if sphere_free {
                        sphere_blind += 1;
                    }
                }
            }
        }
        // Negative: far enough away, the same instrument answers free. Without this the "solid"
        // above could be a function that returns `true`.
        let away = Vec3::new(centre.x + 40.0, centre.y + 40.0, centre.z);
        assert!(
            !part_blocks(&here.parts, away, r),
            "cell {:08X} setup {:#010X}: the part arm calls a point 56 m away solid, so it is not \
             measuring geometry",
            cell.0,
            setup.0
        );
        eprintln!(
            "BSPCALIB cell={:08X} setup={:#010X} at=({:.3},{:.3},{:.3}) r={:.3} solid_points={hits} \
of_which_the_sphere_arm_calls_free={sphere_blind}",
            cell.0, setup.0, centre.x, centre.y, centre.z, root.radius
        );
        if hits > 0 {
            probed += 1;
            if sphere_blind > 0 {
                solid_and_sphere_free += 1;
            }
        }
    }
    assert!(
        probed > 0,
        "the part arm found no solid point inside any BSP-only static's own bounding sphere, so \
         it has not been shown to be able to answer `solid` at all"
    );
    assert_eq!(
        probed,
        solid_and_sphere_free,
        "on {} of {probed} BSP-only statics the sphere-only arm agreed with the part arm, which \
         it cannot do -- these setups have no spheres, so the agreement means the probe is not \
         reading what it claims to",
        probed - solid_and_sphere_free
    );
}

/// Independent geometry evidence for judging the collision contact path. For each of eighty
/// room/heading pairs, march the body
/// through cell membership, shell solid and static-obstacle queries without invoking transition
/// collision resolution. Calibrate downward against every start's floor and outward against
/// open ground; the separate BSP-only test covers the representation those checks missed.
#[test]
fn the_shipped_geometry_says_what_is_in_front_of_the_body_in_each_of_the_eighty_trials() {
    let store = store();
    let cells = block_cells(&store);
    assert!(!cells.is_empty(), "Holtburg has no interior cells at all");
    let body = body_spheres(&store);
    assert!(!body.is_empty(), "the body has no path spheres");
    let found = rooms(&store, 20);
    assert!(
        found.len() >= 20,
        "only {} standable rooms in Holtburg",
        found.len()
    );

    // Calibration, positive: the floor is under every one of the twenty start points. An
    // instrument that cannot see the floor cannot see a wall either. A quarter turn about x
    // points the heading's own +y at -z.
    let q = std::f32::consts::FRAC_PI_4;
    let down = Quat::new(math::cosf(q), -math::sinf(q), 0.0, 0.0);
    let mut floors = 0usize;
    for &(_, inside) in &found {
        if let Fate::Blocked(d, _, _) = straight_fate(&cells, inside, down, &body, 4.0) {
            assert!(d <= 2.5, "the floor under a start point is {d:.2} m down");
            floors += 1;
        }
    }
    assert_eq!(
        floors,
        found.len(),
        "the marcher found no floor under some start point"
    );

    // Calibration, negative: a point on the open ground of the same landblock is outside every
    // interior cell at once, so "outside the building" is a state this instrument can report.
    let ground = Vec3::new(96.0, 96.0, found[0].1.z);
    assert_eq!(
        straight_fate(&cells, ground, Quat::IDENTITY, &body, 4.0),
        Fate::ClearExit(0.0),
        "the marcher thinks the open ground is inside a building"
    );

    let mut clear = 0usize;
    let mut blocked = 0usize;
    let mut open = 0usize;
    for &(room, inside) in &found {
        for quarter in 0..4u32 {
            #[allow(clippy::cast_precision_loss)] // 0..4
            let half = (quarter as f32) * std::f32::consts::FRAC_PI_4;
            let heading = Quat::new(math::cosf(half), 0.0, 0.0, math::sinf(half));
            let fate = straight_fate(&cells, inside, heading, &body, 40.0);
            match fate {
                Fate::Blocked(..) => blocked += 1,
                Fate::ClearExit(_) => clear += 1,
                Fate::Open => open += 1,
            }
            eprintln!("FATE room={:08X} q={quarter} fate={fate:?}", room.0);
        }
    }
    eprintln!("ORACLE clear={clear} blocked={blocked} open={open} of 80");
    assert_eq!(
        clear + blocked + open,
        80,
        "the oracle did not answer for all eighty trials"
    );
    assert!(
        clear > 0,
        "no heading in twenty Holtburg rooms has a clear line out of the building"
    );
}

/// Is the body's pose at `p` free of solid geometry, and is `p` inside any interior cell?
///
/// The same membership and solid-obstacle questions as straight_fate, evaluated at one point.
/// Both include cell shells, static spheres and part BSPs; the flood fill and straight march
/// ask different reachability questions of that same geometry instrument.
fn body_free_at(
    cells: &[CellObstacles],
    body: &[dereth_physics::geom::Sphere],
    p: Vec3,
) -> (bool, bool) {
    let mut inside_any = false;
    for c in cells {
        let g = &c.geom;
        let Some(cb) = g.cell_bsp.as_ref() else {
            continue;
        };
        if !cb.point_inside_cell_bsp(dereth_physics::math::globaltolocal(&g.frame, p)) {
            continue;
        }
        inside_any = true;
        for b in body {
            let wc = Vec3::new(p.x + b.center.x, p.y + b.center.y, p.z + b.center.z);
            if let Some(pb) = g.physics_bsp.as_ref() {
                let lc = dereth_physics::math::globaltolocal(&g.frame, wc);
                if pb.sphere_intersects_solid(
                    &dereth_physics::geom::Sphere::new(lc, b.radius),
                    false,
                ) {
                    return (false, inside_any);
                }
            }
            for o in &c.spheres {
                let (dx, dy, dz) = (wc.x - o.center.x, wc.y - o.center.y, wc.z - o.center.z);
                let r = b.radius + o.radius;
                if dx.mul_add(dx, dy.mul_add(dy, dz * dz)) <= r * r {
                    return (false, inside_any);
                }
            }
            // The part-BSP arm, which the collision system reaches: without it `A9B4013F/q1`
            // reads 1.064 m free ahead where it has 0.000 m.
            if part_blocks(&c.parts, wc, b.radius) {
                return (false, inside_any);
            }
        }
    }
    (true, inside_any)
}

/// Does a body of this width have **any** route out of the building from `start`, by any path
/// rather than along one heading?
///
/// `straight_fate` answers a question about a *line*; a body that meets a wall slides along it and
/// leaves through a door in some other direction, which no line test can see. So a walk that ends
/// outdoors on a heading the straight oracle calls `Blocked` is **not** evidence that the body
/// passed through anything; this tells the two apart. This breadth-first search uses body_free_at on a bounded GRID lattice at the start's
/// fixed height. It reports free nodes visited until the first verified exit, or exhaustion;
/// the returned count is not all reachable points when an exit is found.
///
/// "Genuinely outside" is not "outside every cell at this point": a portal seam is a few
/// centimetres of no-man's-land between two cells and would read as an exit. A node counts only
/// when the four cardinal probes at `CLEAR_PROBE` metres are **also** outside every cell, which is
/// the flood-fill form of `straight_fate`'s `CLEAR_RUN` guard.
fn reachable_exit(
    cells: &[CellObstacles],
    body: &[dereth_physics::geom::Sphere],
    start: Vec3,
    half_extent: f32,
) -> (bool, usize) {
    const GRID: f32 = 0.15;
    const CLEAR_PROBE: f32 = 1.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (half_extent / GRID) as i32;
    let idx = |i: i32, j: i32| -> usize {
        #[allow(clippy::cast_sign_loss)]
        {
            ((i + n) as usize) * (2 * n as usize + 1) + (j + n) as usize
        }
    };
    let at = |i: i32, j: i32| -> Vec3 {
        #[allow(clippy::cast_precision_loss)]
        Vec3::new(
            start.x + i as f32 * GRID,
            start.y + j as f32 * GRID,
            start.z,
        )
    };
    let mut seen = vec![false; (2 * n as usize + 1) * (2 * n as usize + 1)];
    let mut queue = std::collections::VecDeque::new();
    seen[idx(0, 0)] = true;
    queue.push_back((0i32, 0i32));
    let mut visited = 0usize;
    let mut exit = false;
    while let Some((i, j)) = queue.pop_front() {
        let p = at(i, j);
        let (free, inside) = body_free_at(cells, body, p);
        if !free {
            continue;
        }
        visited += 1;
        if !inside {
            // A node outside every cell only counts as an exit when a metre in each of the four
            // cardinal directions is outside every cell too, so a portal seam cannot be one.
            let clear = [
                Vec3::new(p.x + CLEAR_PROBE, p.y, p.z),
                Vec3::new(p.x - CLEAR_PROBE, p.y, p.z),
                Vec3::new(p.x, p.y + CLEAR_PROBE, p.z),
                Vec3::new(p.x, p.y - CLEAR_PROBE, p.z),
            ]
            .iter()
            .all(|q| !body_free_at(cells, body, *q).1);
            // On these rooms requiring any one probe instead of all four changes only search
            // counts, not outcomes, so the sampled rooms do not distinguish the four-probe guard;
            // it stays for narrower portal seams.
            if clear {
                exit = true;
                return (exit, visited);
            }
            // Outside every cell but not yet clear of the building: continue over the bounded
            // lattice. This oracle sees no cell geometry there; it does not model outdoor terrain.
        }
        for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (a, b) = (i + di, j + dj);
            if a.abs() > n || b.abs() > n {
                continue;
            }
            if !seen[idx(a, b)] {
                seen[idx(a, b)] = true;
                queue.push_back((a, b));
            }
        }
    }
    (exit, visited)
}

/// Which of these twenty rooms a body of this width can leave, by any route.
///
/// A straight blocked heading does not rule out sliding to a doorway along another route. This
/// fixed-height lattice search provides a sampled route-existence check from the same geometry,
/// without invoking transition resolution. It finds exits for all 20 sampled rooms. Existence of
/// some route does not prove a particular simulated trajectory followed it; depth checks remain
/// separate.
///
/// `rooms()` asks the same question of a start point as these oracles do (every cell the point
/// is inside, not only its own), so no start is solid in a neighbour's tree and every fill
/// expands from a free node (`A9B40104`'s expands about 4,500).
#[test]
fn the_shipped_geometry_says_which_rooms_a_body_can_leave_by_any_route() {
    /// How much wider than the retail body the calibration body is.
    const FAT: f32 = 1.6;
    let store = store();
    let cells = block_cells(&store);
    assert!(!cells.is_empty(), "Holtburg has no interior cells at all");
    let body = body_spheres(&store);
    let found = rooms(&store, 20);
    assert!(
        found.len() >= 20,
        "only {} standable rooms in Holtburg",
        found.len()
    );

    // Positive calibration: open ground must report an exit, so true is a reachable answer.
    let ground = Vec3::new(96.0, 96.0, found[0].1.z);
    let (out, seen) = reachable_exit(&cells, &body, ground, 4.0);
    assert!(
        out,
        "the flood fill cannot find open ground outside every building ({seen} nodes)"
    );
    // The wider-body checks below provide negative results, including bodies that can occupy
    // some nodes but cannot reach an exit.

    // Increase radii by 1.6 while leaving sphere centres fixed. Fewer starts must reach an exit,
    // and some must fit without escaping. This independently checks body-width sensitivity.
    let fat: Vec<dereth_physics::geom::Sphere> = body
        .iter()
        .map(|s| dereth_physics::geom::Sphere::new(s.center, s.radius * FAT))
        .collect();
    let mut fat_open = 0usize;
    let mut fat_stuck = 0usize;
    for &(_, inside) in &found {
        let (exit, visited) = reachable_exit(&cells, &fat, inside, 24.0);
        if exit {
            fat_open += 1;
        } else if visited > 0 {
            // It fits where it starts and still cannot get out -- a real sealed reading rather
            // than a body that fits nowhere.
            fat_stuck += 1;
        }
    }
    eprintln!("REACHWIDE scale={FAT} open={fat_open} sealed_with_room_to_move={fat_stuck}");

    let mut open = 0usize;
    let mut sealed_rooms = Vec::new();
    for &(room, inside) in &found {
        let (exit, visited) = reachable_exit(&cells, &body, inside, 24.0);
        eprintln!("REACH room={:08X} exit={exit} nodes={visited}", room.0);
        if exit {
            open += 1;
        } else {
            sealed_rooms.push((room, visited));
        }
    }
    eprintln!(
        "REACHSUMMARY open={open} sealed={} of {}",
        found.len() - open,
        found.len()
    );

    // Negative calibration: the 1.6x-radius body must reach fewer exits than the normal body,
    // and at least one of its searches must find room to move and no exit.
    assert!(
        fat_open < open,
        "a body {FAT}x as wide got out of {fat_open} rooms against the real body's {open}; this \
         fill is not answering about body width at all"
    );
    assert!(
        fat_stuck > 0,
        "no room holds the {FAT}x body anywhere it fits, so every negative above could be \
         'the body fits nowhere' rather than 'the building is closed'"
    );

    // The load-bearing claim: a walk that ends outside one of these buildings is not, on its
    // own, evidence that the body passed through anything.
    //
    // Every one of the twenty rooms has a route out: **20 of 20**.
    assert_eq!(
        open,
        found.len(),
        "{open} of {} Holtburg rooms have a body-width route out, and {} do not ({:?}). Every \
         escape the walk suite records is read against this: a body that leaves a room with \
         no route out has passed through something.",
        found.len(),
        sealed_rooms.len(),
        sealed_rooms
            .iter()
            .map(|(c, n)| format!("{:08X}/{n} nodes", c.0))
            .collect::<Vec<_>>()
    );
}
