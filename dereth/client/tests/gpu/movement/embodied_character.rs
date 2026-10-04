//! The player's own body over Holtburg: it settles on the terrain polygon physics builds, which is
//! the same ground the renderer draws; it walks along its heading, jumps and lands through the
//! animation/physics callbacks, stands in its ready cycle rather than the falling one, runs physics
//! at 30 Hz whatever the frame rate, places its parts as a step function of time, and is drawn in
//! front of the camera with its palette. Two runs of one script give the same physics trace (the
//! physics crate's trace format and comparison), which is reproducibility, not fidelity: no
//! recorded retail trace exists to compare against.
//! Fixture: `Character` and `WorldScene` on Holtburg's landblock from the retail dats; the drawn
//! checks use a headless software device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;

use dereth_client::character::{Character, CharacterInput};
use dereth_client::world::{SceneConfig, WorldScene, DEFAULT_LANDBLOCK};
use dereth_dat::RetailDatStore;
use dereth_physics::trace::{compare, TraceRecord};
use dereth_physics::{PlaneExt, TransitionState};
use dereth_primitives::{LandblockId, LocalTime, Vec3};
use dereth_render::device::Gpu;

/// Where the character spawns: the middle of Holtburg's own landblock.
const SPAWN: (f32, f32) = (96.0, 96.0);

/// `b - a`. `dereth_primitives::Vec3` deliberately carries no arithmetic, so the test does its own.
fn diff(b: Vec3, a: Vec3) -> Vec3 {
    Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z)
}

fn len(v: Vec3) -> f32 {
    v.x.mul_add(v.x, v.y.mul_add(v.y, v.z * v.z)).sqrt()
}

/// The retail store, or fail: a missing dat is a failure, never a skip.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn warp() -> Gpu {
    crate::common::software_gpu(800, 600)
}

fn character(store: &Arc<RetailDatStore>) -> Character {
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    Character::new(store, &region, DEFAULT_LANDBLOCK, SPAWN).expect("the character is created")
}

/// Drive `seconds` of wall clock at a fixed frame rate from `t0`, returning the new clock and how
/// many times the 30 Hz gate opened. Shaped like `tests/cpu/walk_scenarios.rs`'s own `run`.
fn run(c: &mut Character, t0: f64, seconds: f64, fps: f64) -> (f64, u32) {
    let dt = 1.0 / fps;
    let (mut t, mut ticks) = (t0, 0);
    while t < t0 + seconds {
        t += dt;
        if c.update(LocalTime(t)) {
            ticks += 1;
        }
    }
    (t, ticks)
}

/// The terrain height the physics crate's own landblock puts under a block-local `(x, y)`, computed here
/// rather than asked of the code under test.
fn physics_ground(store: &Arc<RetailDatStore>, block: LandblockId, x: f32, y: f32) -> f32 {
    use dereth_assets::world::CellLandblock;
    use dereth_assets::Decode;
    use dereth_dat::DbType;

    let region = dereth_client::world::load_region(store).expect("region");
    let table =
        dereth_physics::landdefs::validate_height_table(&region.land_defs.land_height_table)
            .expect("the retail height table is valid");
    let did = dereth_client::world::landblock_did(block.0);
    let bytes = store.read_typed(DbType::LandBlock, did).expect("landblock");
    let lb = CellLandblock::decode_payload(did, &bytes).expect("decodes");
    let col = dereth_physics::LandblockCollision::build(
        block,
        Box::new(lb.height),
        Box::new(lb.terrain),
        lb.lbi_exists != 0,
        8,
        &table,
    )
    .expect("full detail");

    let mut cell = block.cell(1);
    let mut local = Vec3::new(x, y, 0.0);
    dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut local);
    let poly = col
        .find_terrain_poly(cell.index(), local)
        .expect("a terrain polygon");
    let mut p = Vec3::new(x, y, 0.0);
    assert!(
        poly.plane.set_height(&mut p),
        "the terrain plane is not vertical"
    );
    p.z
}

/// Oracle: `dereth_physics::LandblockCollision`, whose own gate covers all 65,025 retail landblocks,
/// plus the Aluvian male's collision spheres as the dat decodes them.
///
/// The lowest sphere is centred at z = 0.475 with radius 0.48, so its bottom is 0.005 m **below**
/// the object's origin; an object at rest therefore sits with `origin.z = ground + 0.005`. That
/// number is derived from the dat, not chosen: it is `radius - center.z`.
#[test]
fn the_body_settles_exactly_where_the_terrain_polygon_puts_it() {
    let store = store();
    let mut c = character(&store);
    let (_, ticks) = run(&mut c, 0.0, 2.0, 30.0);
    assert!(ticks > 0, "the 30 Hz gate never opened");

    let ground = physics_ground(&store, LandblockId(DEFAULT_LANDBLOCK), SPAWN.0, SPAWN.1);
    let pos = c.position();
    // The origin has not wandered in x or y: nothing was asked of it.
    assert!(
        (pos.frame.origin.x - SPAWN.0).abs() < 1e-3,
        "{:?}",
        pos.frame.origin
    );
    assert!(
        (pos.frame.origin.y - SPAWN.1).abs() < 1e-3,
        "{:?}",
        pos.frame.origin
    );

    let geom = c.world.get(c.handle).expect("live").geometry.clone();
    let lowest = geom
        .path_spheres()
        .iter()
        .map(|s| s.radius - s.center.z)
        .fold(f32::MIN, f32::max);
    let expect = ground + lowest;
    assert!(
        (pos.frame.origin.z - expect).abs() < 0.01,
        "resting z {} but the physics terrain polygon plus the sphere offset {lowest} is {expect}",
        pos.frame.origin.z
    );

    // And it is *standing*, not merely somewhere: contact, walkable, and the contact plane is a
    // walkable one by `is_valid_walkable`, which is the bare comparison against the baked FLOOR_Z.
    let o = c.world.get(c.handle).expect("live");
    assert!(
        o.transient_state.in_contact(),
        "no contact: {:?}",
        o.transient_state
    );
    assert!(o.transient_state.on_walkable());
    assert!(
        dereth_physics::is_valid_walkable(o.contact_plane.normal),
        "{:?}",
        o.contact_plane
    );
    assert!(c.on_ground());
}

/// Behaviour: movement.ground.the-body-stands-on-the-ground-the-renderer-draws
/// Oracle: the renderer's land mesh and the physics crate's collision landblock, differentially;
/// neither crate's own suite can see the other.
///
/// If they ever disagree the player stands on ground that is not drawn, which is the failure the
/// split-rule check in `dereth/client/tests/cpu/rendering/terrain_split_rule.rs` guards one level down. This is the same
/// question asked of the *height*: over a dense sweep of Holtburg, the z of the physics polygon and
/// the z of the rendered triangle under the same point must be the same number.
#[test]
fn the_ground_physics_stands_on_is_the_ground_the_renderer_draws() {
    use dereth_assets::world::CellLandblock;
    use dereth_assets::Decode;
    use dereth_dat::DbType;
    use dereth_world_render::land::mesh::{generate_landblock_with_table, height_table};

    let store = store();
    let block = LandblockId(DEFAULT_LANDBLOCK);
    let region = dereth_client::world::load_region(&store).expect("region");
    let did = dereth_client::world::landblock_did(block.0);
    let bytes = store.read_typed(DbType::LandBlock, did).expect("landblock");
    let lb = CellLandblock::decode_payload(did, &bytes).expect("decodes");

    // The renderer's mesh, at full detail and unstitched — the ring the viewer's own block is in.
    let mesh = generate_landblock_with_table(
        &lb,
        &region,
        &height_table(&region),
        i32::from(block.x()),
        i32::from(block.y()),
        1,
        dereth_world_render::land::mesh::Direction::InViewerBlock,
    );
    assert_eq!(mesh.side_cell_count, 8);

    // The physics collision block, built independently from the same record.
    let table =
        dereth_physics::landdefs::validate_height_table(&region.land_defs.land_height_table)
            .expect("valid");
    let col = dereth_physics::LandblockCollision::build(
        block,
        Box::new(lb.height),
        Box::new(lb.terrain),
        lb.lbi_exists != 0,
        8,
        &table,
    )
    .expect("full detail");

    // A dense sweep, deliberately off the cell corners and off the diagonal so the point lands
    // inside one triangle and the two implementations have to have picked the *same* one.
    let mut checked = 0u32;
    for i in 0..8 {
        for j in 0..8 {
            for (fx, fy) in [(0.2f32, 0.3f32), (0.7, 0.2), (0.3, 0.8), (0.8, 0.75)] {
                #[allow(clippy::cast_precision_loss)] // 0..8
                let (x, y) = ((i as f32 + fx) * 24.0, (j as f32 + fy) * 24.0);

                let mut cell = block.cell(1);
                let mut local = Vec3::new(x, y, 0.0);
                dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut local);
                let poly = col
                    .find_terrain_poly(cell.index(), local)
                    .expect("a physics polygon");
                let mut p = Vec3::new(x, y, 0.0);
                assert!(poly.plane.set_height(&mut p));

                // The renderer's answer: the triangle of the same cell whose 2D footprint contains the
                // point, solved on its own plane.
                let (a, b) = mesh.cell_polygons(i, j);
                let z = [a, b]
                    .iter()
                    .find_map(|t| {
                        let v: Vec<Vec3> = t.v.iter().map(|k| mesh.vertices[*k as usize]).collect();
                        point_in_triangle_2d(x, y, &v).then(|| {
                            let n = t.plane.normal;
                            -(t.plane.d + n.x * x + n.y * y) / n.z
                        })
                    })
                    .expect("a rendered triangle covers the point");

                assert!(
                    (p.z - z).abs() < 1e-3,
                    "at ({x}, {y}) physics says z = {} and the renderer draws z = {z}",
                    p.z
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 8 * 8 * 4);
}

/// A point-in-triangle test in the xy plane, inclusive of the edges so a point on the shared
/// diagonal is claimed by whichever triangle is tried first.
fn point_in_triangle_2d(x: f32, y: f32, v: &[Vec3]) -> bool {
    let cross = |a: Vec3, b: Vec3| (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x);
    let (d0, d1, d2) = (cross(v[0], v[1]), cross(v[1], v[2]), cross(v[2], v[0]));
    let neg = d0 < -1e-4 || d1 < -1e-4 || d2 < -1e-4;
    let pos = d0 > 1e-4 || d1 > 1e-4 || d2 > 1e-4;
    !(neg && pos)
}

/// Oracle: `dereth_animation`'s motion table and `dereth_physics`'s update, joined by the seam. What is
/// asserted is what neither crate can assert alone: that the animation's own forward offset is
/// what carries the body across the physics terrain.
///
/// Nothing here names a speed: the distance covered is compared against the offsets `advance`
/// actually returned, so a change to the retail walk animation changes both sides together.
#[test]
fn walking_carries_the_body_along_its_own_heading_and_it_stays_on_the_ground() {
    let store = store();
    let mut c = character(&store);
    let (t, _) = run(&mut c, 0.0, 1.0, 30.0);
    let start = c.position().frame.origin;
    assert!(c.on_ground(), "it has to be standing before it can walk");

    c.input = CharacterInput {
        forward: true,
        ..CharacterInput::default()
    };
    let (t, ticks) = run(&mut c, t, 4.0, 30.0);
    let end = c.position().frame.origin;

    // It went north: the body spawns unrotated, with heading 0 pointing along +y.
    let d = Vec3::new(end.x - start.x, end.y - start.y, end.z - start.z);
    assert!(d.y > 3.0, "four seconds of walking moved it {d:?}");
    assert!(d.x.abs() < 0.5, "and it drifted sideways: {d:?}");
    // Holtburg's middle slopes, so the body followed the terrain rather than the xy plane.
    assert!(
        d.z.abs() > 0.1,
        "the body did not follow the terrain: {d:?}"
    );
    let ground = physics_ground(&store, LandblockId(DEFAULT_LANDBLOCK), end.x, end.y);
    assert!(
        (end.z - ground).abs() < 0.05,
        "it ended at z = {} with the terrain at {ground}",
        end.z
    );
    assert!(c.on_ground(), "and it never left the ground");
    assert_eq!(
        c.stats.motions_refused, 0,
        "the motion table refused a command"
    );
    assert_eq!(
        c.stats.motions_issued, 1,
        "one motion issue for one key press"
    );

    // The reported velocity is the **achieved** velocity, so an unobstructed walk
    // reports a non-zero one pointing where the body is facing.
    assert!(c.velocity().y > 0.5, "achieved velocity {:?}", c.velocity());

    // Releasing the key stops the interpreted motion and restores the Ready cycle.
    c.input = CharacterInput::default();
    let (_, _) = run(&mut c, t, 2.0, 30.0);
    let after = c.position().frame.origin;
    let (_, _) = run(&mut c, t + 2.0, 1.0, 30.0);
    let settled = c.position().frame.origin;
    let drift = Vec3::new(
        settled.x - after.x,
        settled.y - after.y,
        settled.z - after.z,
    );
    assert!(
        len(drift) < 0.05,
        "it kept sliding after the key came up: {drift:?}"
    );
    assert!(ticks > 0);
}

/// Behaviour: movement.tick.physics-runs-at-thirty-hertz-whatever-the-frame-rate
/// Oracle: `core/physics/tests/cpu/transition/walk_scenarios.rs`'s own gate test, reproduced over real terrain and
/// through the real animation. Its bounds are used verbatim, including the "slightly less than 30"
/// allowance for the accumulated wall clock straddling `1/30`.
///
/// **The gate is not a frame-rate cap.** Whatever the frame rate, physics runs at most 30 times a
/// second and the distance walked tracks the tick count, not the frame count. Conflating the two is
/// the trap this test exists for.
#[test]
fn the_thirty_hertz_gate_bounds_physics_and_the_frame_rate_does_not() {
    let store = store();
    let mut results = Vec::new();
    for fps in [250.0_f64, 120.0, 60.0] {
        let mut c = character(&store);
        let (t, _) = run(&mut c, 0.0, 1.0, fps);
        c.input = CharacterInput {
            forward: true,
            ..CharacterInput::default()
        };
        let start = c.position().frame.origin;
        let (_, ticks) = run(&mut c, t, 2.0, fps);
        let moved = len(diff(c.position().frame.origin, start));
        assert!(
            ticks <= 61,
            "{fps} fps produced {ticks} physics ticks in 2 s; the gate is 30 Hz"
        );
        assert!(ticks >= 40, "{fps} fps produced only {ticks} ticks");
        results.push((fps, ticks, moved));
    }
    // Every frame rate walked the same distance to within a sub-step, because the distance is a
    // function of the ticks and not of the frames.
    let d0 = results[0].2;
    for (fps, ticks, moved) in &results {
        assert!(
            (moved - d0).abs() < 0.5,
            "{fps} fps ({ticks} ticks) walked {moved} m where 250 fps walked {d0} m"
        );
    }
}

/// Oracle: the ground-transition callbacks in `dereth_primitives::motion`. In the original physics path,
/// changing walkability is the sole source of both ground-edge callbacks; an unchanged value
/// fires neither. This checks the same edge-only behavior through the current seam.
///
/// This is the half of the bidirectional seam a one-way test cannot see: `advance` being called is
/// obvious from the body moving, but `hit_ground` firing is not.
#[test]
fn physics_calls_back_into_the_animation_layer_when_the_body_reaches_the_ground() {
    let store = store();
    let mut c = character(&store);
    assert_eq!(
        c.ground_edges(),
        dereth_client::character::GroundEdges::default(),
        "none yet"
    );
    run(&mut c, 0.0, 1.0, 30.0);
    let e = c.ground_edges();
    assert_eq!(e.hit, 1, "contact is established exactly once: {e:?}");
    assert_eq!(
        e.left, 0,
        "and standing still never leaves the ground: {e:?}"
    );

    // And the edge really is an edge: two more seconds of standing fires nothing further.
    run(&mut c, 1.0, 2.0, 30.0);
    assert_eq!(
        c.ground_edges(),
        e,
        "a level surface fired a second transition"
    );
}

/// Oracle: `dereth_animation::motion::get_jump_height`, which preserves the original height calculation's
/// **0.35 m floor applied last**, so a
/// zero-skill jump reaches exactly 0.35 m; and `dereth_physics::globals::GRAVITY`,
/// which brings the body back down.
///
/// This is the round trip through the *whole* seam in the other direction: the animation layer asks
/// physics to clear walkability, physics sends the leave-ground callback to the animation layer,
/// which computes the impulse and requests a local velocity, and physics integrates it. Nothing
/// in this file computes a trajectory.
#[test]
fn a_jump_leaves_the_ground_through_the_seam_and_gravity_brings_it_back() {
    let store = store();
    let mut c = character(&store);
    run(&mut c, 0.0, 2.0, 30.0);
    let resting = c.position().frame.origin.z;
    assert!(c.on_ground());
    let before = c.ground_edges();

    let (load, skill, scale) = {
        let d = c.driver();
        (d.env.load, d.env.jump_skill, d.scale)
    };
    let expect = dereth_animation::motion::get_jump_height(
        load,
        skill,
        dereth_client::character::FULL_JUMP_EXTENT,
        scale,
    );

    c.input = CharacterInput {
        jump: true,
        ..CharacterInput::default()
    };
    let mut t = 2.0;
    t += 1.0 / 30.0;
    c.update(LocalTime(t));
    c.input = CharacterInput::default();
    assert_eq!(
        c.stats.motions_refused, 0,
        "the jump was refused with {:#04X}",
        c.stats.last_refusal
    );

    let mut peak = resting;
    let mut left_ground = false;
    for _ in 0..90 {
        t += 1.0 / 30.0;
        c.update(LocalTime(t));
        peak = peak.max(c.position().frame.origin.z);
        left_ground |= !c.on_ground();
    }
    assert!(left_ground, "the body never left the ground");
    // The apex is sampled at 30 Hz, so the measured peak is up to one sub-step's rise below the
    // true one: `v^2 / 2g` with `v` the residual speed at the sample, at most `g/30` -> ~5 mm.
    let rise = peak - resting;
    assert!(
        (rise - expect).abs() < 0.02,
        "jumped {rise} m where the jump-height calculation says {expect} m",
    );
    // Both ground edges fired: clearing walkability left the ground, and the transition system
    // finding the ground produced the landing callback.
    let after = c.ground_edges();
    assert_eq!(
        after.left,
        before.left + 1,
        "ground exit: {before:?} -> {after:?}"
    );
    assert_eq!(
        after.hit,
        before.hit + 1,
        "ground entry: {before:?} -> {after:?}"
    );
    assert!(c.on_ground(), "it did not land");
    // ...and it landed on the terrain, checked against the same oracle as the settle test rather
    // than against the height it left from: an unpowered jump on a slope does not have to come
    // back to the same z, and asserting that it does would be asserting a belief.
    let end = c.position().frame.origin;
    let ground = physics_ground(&store, LandblockId(DEFAULT_LANDBLOCK), end.x, end.y);
    assert!(
        (end.z - ground).abs() < 0.05,
        "it landed at {} with the physics terrain polygon at {ground}",
        end.z
    );
}

/// Oracle: the retail motion table `0x09000001`, whose `style_defaults` give **every** style the
/// substate `Ready (0x41000003)`, and the original movement interpreter's rule that issues
/// `Falling (0x40000015)` only for an object that is **not** on the ground.
///
/// `MotionEnv` is a copy of physics state taken once per frame, but the hit-ground callback fires
/// inside the physics step, so a stale copy says "airborne" at the moment the body lands and the
/// movement layer puts a standing character into the falling cycle (a body frozen with its arms
/// out). `SharedMotion::hit_ground` marks contact and on-ground true before forwarding the call,
/// matching the physics state at that ground edge.
#[test]
fn a_body_standing_on_the_ground_plays_its_ready_cycle_and_not_the_falling_one() {
    let store = store();
    let mut c = character(&store);
    let substate = |c: &Character| c.driver().motion_table.state.substate;
    let anim = |c: &Character| c.driver().sequence.nodes().first().map(|n| n.anim_id);

    let ready = dereth_animation::MotionCommand::READY;
    assert_eq!(
        substate(&c),
        ready,
        "the default state is the style's default substate"
    );
    let standing = anim(&c).expect("a standing animation");

    // Four seconds of standing on level ground, over which the landing edge fires.
    run(&mut c, 0.0, 4.0, 30.0);
    assert!(c.on_ground());
    assert_eq!(c.ground_edges().hit, 1);
    assert_eq!(
        substate(&c),
        ready,
        "a body on the ground is in {:?}, not its style's default substate",
        substate(&c)
    );
    assert_ne!(
        substate(&c),
        dereth_animation::MotionCommand::FALLING,
        "a body resting on Holtburg's turf is playing the falling cycle"
    );
    assert_eq!(
        anim(&c),
        Some(standing),
        "the standing animation was replaced"
    );

    // Walking and stopping returns to the same cycle, which is the path that masked the defect:
    // Stopping interpreted motion reinstalls Ready, so a body that has walked once looks right even
    // when a body that has only stood does not.
    c.input = CharacterInput {
        forward: true,
        ..CharacterInput::default()
    };
    let (t, _) = run(&mut c, 4.0, 2.0, 30.0);
    assert_eq!(substate(&c), dereth_animation::MotionCommand::WALK_FORWARD);
    c.input = CharacterInput::default();
    run(&mut c, t, 2.0, 30.0);
    assert_eq!(substate(&c), ready);
    assert_eq!(anim(&c), Some(standing));
}

/// Oracle: `dereth_physics::trace`'s own record shape and `compare`, with its tolerance rule
/// (`1e-5` for the first 100 records, `1e-4` after; cell and both state words exactly).
///
/// **This is reproducibility, not fidelity.** No recorded retail physics trace exists
/// (`fixtures/physics/traces/` is absent), so there is nothing to match; what is shown
/// is that two runs of the same script through the whole stack — dat read, motion table, animation
/// player, seam, transition system — produce matching traces under those comparison rules, a
/// precondition for a captured fixture ever being comparable.
#[test]
fn two_runs_of_the_same_script_produce_the_same_physics_trace() {
    let store = store();

    let script = |c: &mut Character| -> Vec<TraceRecord> {
        let mut out = Vec::new();
        let mut t = 0.0;
        for step in 0..180 {
            // Stand, walk, turn, run — every slot of the interpreted state gets used.
            c.input = match step {
                0..=29 => CharacterInput::default(),
                30..=89 => CharacterInput {
                    forward: true,
                    ..CharacterInput::default()
                },
                90..=119 => CharacterInput {
                    turn_right: true,
                    ..CharacterInput::default()
                },
                _ => CharacterInput {
                    forward: true,
                    run: true,
                    ..CharacterInput::default()
                },
            };
            t += 1.0 / 30.0;
            c.update(LocalTime(t));
            let o = c.world.get(c.handle).expect("live");
            out.push(TraceRecord {
                t,
                update_time: o.update_time,
                cell: o.position.cell,
                origin: o.position.frame.origin,
                quat: o.position.frame.rotation,
                velocity: o.velocity_vector,
                cached_velocity: o.cached_velocity,
                contact_plane: o.transient_state.in_contact().then_some(o.contact_plane),
                state: o.state.0,
                transient_state: o.transient_state.0,
                // The per-sub-step transition result is not exposed by `PhysicsObj`, so this field
                // is a constant here and contributes nothing to the comparison. Named rather than
                // quietly filled: a captured fixture would carry the real value.
                transition_state: TransitionState::Ok,
            });
        }
        out
    };

    let a = script(&mut character(&store));
    let b = script(&mut character(&store));
    let d = compare(&a, &b);
    assert!(d.is_empty(), "the same script diverged: {d:?}");

    // And the script actually exercised something: the body moved, turned and stayed in one piece.
    let first = a.first().expect("records");
    let last = a.last().expect("records");
    assert!(
        len(diff(last.origin, first.origin)) > 5.0,
        "the script barely moved the body"
    );
    assert_ne!(last.quat, first.quat, "the turn did not rotate the body");
    // Contact takes a sub-step or two to establish -- the body is dropped at ground level and the
    // transition system has to find the plane -- and is then held for the whole script.
    let first_contact = a
        .iter()
        .position(|r| r.contact_plane.is_some())
        .expect("it never touched the ground");
    assert!(
        first_contact <= 3,
        "contact took {first_contact} frames to establish"
    );
    assert!(
        a[first_contact..].iter().all(|r| r.contact_plane.is_some()),
        "the body left the ground part-way through a walk on level-enough terrain"
    );
}

/// Oracle: the discrete animation-frame rule. The part update reads
/// `part_frames[floor(frame_number)]` without interpolation, so a part's placement is a **step
/// function** of time with a small number of distinct values, not a continuum.
///
/// The animation crate asserts this over its own player; what is added here is that the placement
/// reaching the renderer has the same property, i.e. that nothing in the wiring smooths it.
#[test]
fn the_body_animates_and_the_part_placement_is_a_step_function() {
    let store = store();
    let mut c = character(&store);
    run(&mut c, 0.0, 1.0, 30.0);
    c.input = CharacterInput {
        forward: true,
        ..CharacterInput::default()
    };

    // Sample one hand's placement *relative to the body*, so walking forward does not swamp the
    // animation with translation.
    let mut poses: Vec<[i32; 3]> = Vec::new();
    let mut t = 1.0;
    // Twelve samples per animation frame at 30 fps: far more than the number of distinct
    // placements, unless something is interpolating.
    for _ in 0..360 {
        t += 1.0 / 360.0;
        c.update(LocalTime(t));
        // `Character::place_parts` is called by the frame loop, not `Character::update`:
        // `WorldScene::place_local_body` makes it after `recenter()` has chosen the viewer block,
        // whose origin is folded into the part frames. A `Character` driven on its own has to make
        // it here, and `place_parts` takes the `RenderSpace` that `set_viewer_block` returns, so
        // this line states which block the placement is expressed in: the body's own block, which
        // is what `WorldScene::recenter` would choose with no window scrolling under it.
        let space = c.set_viewer_block(dereth_client::world::block_xy(DEFAULT_LANDBLOCK));
        c.place_parts(space);
        let d = c.driver();
        let body = c.render_frame().origin;
        let hand = d.part_array.parts[12].pos.origin;
        // **The premise, and it is not decoration.** Without a placement `parts[12].pos` is
        // the default frame at the render-space origin, and `hand - body` below silently becomes
        // `-body`, i.e. this test starts measuring the *body's* walk instead of the hand's
        // animation. Both assertions at the bottom still pass on that reading (the body moves, and
        // the 30 Hz gate gives it about thirty distinct positions a second), so the failure would
        // be completely silent. A placed part is inside the body's own bounding volume.
        assert!(
            (hand.x - body.x).abs() < 2.0
                && (hand.y - body.y).abs() < 2.0
                && (hand.z - body.z).abs() < 3.0,
            "part 12 is {:?} from the body at {body:?}, which is not a placement: this sample \
             would be the body's own position wearing a part's name",
            (hand.x - body.x, hand.y - body.y, hand.z - body.z)
        );
        // Quantised to 0.1 mm so float noise in the composition does not create false distinctness.
        poses.push([
            dereth_primitives::num::to_i32((hand.x - body.x) * 10_000.0),
            dereth_primitives::num::to_i32((hand.y - body.y) * 10_000.0),
            dereth_primitives::num::to_i32((hand.z - body.z) * 10_000.0),
        ]);
    }
    let moved = poses.iter().filter(|p| **p != poses[0]).count();
    assert!(
        moved > 100,
        "the part never moved: the animation is not playing"
    );

    let mut distinct = poses.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(
        distinct.len() < 120,
        "{} distinct placements over 360 samples: that is a continuum, not a step function",
        distinct.len()
    );
    // ...and consecutive samples repeat, which is what "snaps" looks like from the outside.
    let repeats = poses.windows(2).filter(|w| w[0] == w[1]).count();
    assert!(
        repeats > 200,
        "only {repeats} of 359 consecutive samples repeated"
    );
}

/// Oracle: the retail dats, rendered. The claim is that the body is **drawn**, in front of the
/// camera, over the terrain, not merely simulated.
///
/// A silhouette would pass a "the frame is not black" check, so what is measured is that the pixels
/// the body covers are lit *and coloured*: the Aluvian male's parts are `PFID_INDEX16` and resolve
/// through the palette to skin and cloth, so their channels are not equal to each other.
#[test]
fn the_body_is_drawn_in_front_of_the_camera_and_carries_its_palette() {
    let store = store();
    let mut gpu = warp();
    let region = dereth_client::world::load_region(&store).expect("region");

    // The control is the **same scene from the same chase camera** with the body hidden, so the
    // only difference between the two frames is the body itself. The control sets the no-draw
    // flag also used by `NoDrawHook`, exercising the draw loop's `no_draw` branch.
    let shoot = |gpu: &mut Gpu, with_body: bool| {
        let mut scene = WorldScene::load(&store, gpu, SceneConfig::default()).expect("loads");
        scene
            .attach_character(&store, &region, gpu)
            .expect("the character attaches");
        let mut t = 0.0;
        for _ in 0..30 {
            t += 1.0 / 30.0;
            scene.update(
                dereth_client::camera::CameraInput::default(),
                CharacterInput::default(),
                LocalTime(t),
                1.0 / 30.0,
            );
        }
        if !with_body {
            let c = scene.character.as_ref().expect("attached");
            c.driver_mut().part_array.set_no_draw_internal(true);
        }
        scene.reserve_upload_arena(gpu).expect("arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        gpu.capture().expect("capture")
    };

    let body = shoot(&mut gpu, true);
    let bare = shoot(&mut gpu, false);
    assert_eq!((body.width, body.height), (800, 600));
    assert_ne!(body, bare, "the body changed nothing on screen");

    // The pixels that differ are the body. There should be a person's worth of them, they should
    // be in the middle of the frame, and they should be coloured rather than a black cut-out.
    let (a, b) = (body.bgra.as_chunks::<4>().0, bare.bgra.as_chunks::<4>().0);
    let (mut n, mut coloured, mut min_x, mut max_x) = (0usize, 0usize, u32::MAX, 0u32);
    for y in 0..body.height {
        for x in 0..body.width {
            let i = (y * body.width + x) as usize;
            if a[i] == b[i] {
                continue;
            }
            n += 1;
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            let (bl, r) = (i32::from(a[i][0]), i32::from(a[i][2]));
            if (r - bl).abs() > 12 && r > 40 {
                coloured += 1;
            }
        }
    }
    assert!(
        n > 1_500,
        "only {n} pixels changed; a body at 4.5 m should cover more"
    );
    assert!(
        n < 120_000,
        "{n} pixels changed; that is not a body, that is the scene"
    );
    assert!(
        coloured * 4 > n,
        "only {coloured} of {n} body pixels are warm-toned; the palette did not reach them"
    );
    // The chase camera puts it in the middle third of the frame.
    assert!(
        min_x > 200 && max_x < 600,
        "the body spans x {min_x}..{max_x}"
    );
}

/// Oracle: the same scene, built twice in one process and stepped identically. The frame the
/// headless gate captures has to be a function of the dat and the step count alone.
#[test]
fn two_identically_stepped_scenes_render_the_same_frame() {
    let store = store();
    let mut gpu = warp();
    let region = dereth_client::world::load_region(&store).expect("region");
    let mut shots = Vec::new();
    for _ in 0..2 {
        let mut scene = WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("loads");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("attaches");
        let mut t = 0.0;
        for _ in 0..45 {
            t += dereth_client::app::HEADLESS_STEP;
            scene.update(
                dereth_client::camera::CameraInput::default(),
                CharacterInput {
                    forward: true,
                    ..CharacterInput::default()
                },
                LocalTime(t),
                #[allow(clippy::cast_possible_truncation)]
                {
                    dereth_client::app::HEADLESS_STEP as f32
                },
            );
        }
        scene.reserve_upload_arena(&mut gpu).expect("arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        shots.push(gpu.capture().expect("capture"));
    }
    assert_eq!(shots[0], shots[1]);
}
