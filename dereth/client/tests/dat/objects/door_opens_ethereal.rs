//! An animation's ethereal hook reaches physics: a retail door that opens stops blocking, and a
//! closing door will not close on a body standing in it, retrying every physics frame until the
//! body leaves. The motion tables that carry the ethereal hook put it on the `On`/`Off` pair (the
//! hook census is `animation_hook_census`); the mechanism is essentially the
//! door's, but some doors, walls and barriers are typed as creatures, so nothing here special-cases
//! the door weenie type. The module also pins how a door's cell registration is decided (the part
//! bounding boxes), because that is what makes the closing refusal work on both sides of a portal.
//! [`set_state`] drives the same door open by an encoded `0xF74B Item_SetState` instead.
//!
//! Fixture: `client_portal.dat` for the animations, motion tables and door setup `0x0200024F`, and
//! `client_cell_1.dat` for the training academy's rooms (missing dats fail); a constructed walking
//! body driven through the shared collision-walk probe. No GPU.

use crate::common::collision_probe;

use std::sync::Arc;

use dereth_animation::{AnimAssets, AnimEvent, MotionCommand, MotionDriver};
use dereth_assets::{Decode, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::source::EnvCellGeometry;
use dereth_physics::{
    EtherealResult, LandSource, MotionSource, PhysHandle, PhysicsWorld, SetupGeometry, Sphere, V3,
};
use dereth_primitives::num::math;
use dereth_primitives::{CellId, DataId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_world_data::anim_assets::DatAnimAssets;
use dereth_world_data::env_cells::EnvCellLoader;
use dereth_world_data::land_source::DatLandSource;
use {
    dereth_world_data::setup::setup_geometry_with_parts, dereth_world_data::setup::SetupPartStats,
};

/// The shared collision-walk probe, also used by `door_set_state`; its own calibration pins
/// `came_to_rest`'s contract.
use collision_probe::{came_to_rest, frames_inside, in_the_room, inside_object};

/// The training academy, whose doors and rooms every test here uses.
const TRAINING_DUNGEON: u16 = 0x8602;

/// `door`, the plainest of the five door setups block `0x8602` places. All five carry a part
/// physics BSP and **zero** spheres (`mesh_collision` asserts it).
const DOOR: u32 = 0x0200_024F;

fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the door tests need the retail dats under {} (set DERETH_TEST_DAT_DIR); every assertion here \
             takes its oracle from them, so there is nothing to run without them",
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

// =============================================================================================
// 2. The emitter, over the real animation the real door plays
// =============================================================================================

/// Behaviour: objects.animation.the-ethereal-hook-reaches-physics
/// Oracle: the retail motion tables of `client_portal.dat`, driven through the animation crate's
/// `MotionDriver` exactly as [`dereth_scene::world_scene`]'s own `step_animation` does — `advance`,
/// `tick_movement`, `process_hooks`.
///
/// **The motion table is found, not named.** A door's `MotionTableDID` comes from the *server*'s
/// `PhysicsDesc`, not from the setup's default motion-table ID (which is `0` for every door setup —
/// measured, not assumed, and asserted below). So the test sweeps every motion table in the dat,
/// plays `On` and then `Off` through each, and keeps the ones that raise ethereal events: 37 of
/// the 46 ethereal motion tables put the hook on the `On`/`Off` pair, and this re-derives that
/// rather than hard-coding an id.
///
/// What is asserted is the **pairing**, which is the part a door depends on: a table whose `On`
/// raises `SetEthereal(true)` must have an `Off` that raises `SetEthereal(false)`. A table that
/// opened and never closed would be a door that is ethereal for ever.
#[test]
fn the_retail_on_and_off_motions_raise_a_matched_pair_of_ethereal_events() {
    let store = store();
    let assets: Arc<dyn AnimAssets> = Arc::new(DatAnimAssets::new(Arc::clone(&store)));

    // The door setup carries no motion table of its own. Stated as an assertion because the
    // obvious way to write this test is to reach for `default_mtable_id`, and it is silently 0.
    let decoded = setup(&store, DataId(DOOR));
    assert_eq!(
        decoded.default_mtable_id.0, 0,
        "the door setup now carries a default motion table; the server's PhysicsDesc was \
         the only source and this test's premise needs re-reading"
    );
    let setup_data = AnimAssets::setup(assets.as_ref(), DataId(DOOR))
        .unwrap_or_else(|| panic!("door setup {DOOR:#010X} does not load through AnimAssets"));

    let play = |driver: &mut MotionDriver, cmd: MotionCommand| -> Vec<bool> {
        let params = dereth_animation::motion::MovementParameters::default();
        driver.do_interpreted_motion(cmd, &params);
        let mut out = Vec::new();
        let mut t = 0.0;
        for _ in 0..90 {
            t += 1.0 / 30.0;
            MotionSource::advance(driver, 1.0 / 30.0);
            driver.tick_movement(LocalTime(t));
            driver.process_hooks();
            for e in driver.take_events() {
                if let AnimEvent::SetEthereal(on) = e {
                    out.push(on);
                }
            }
        }
        out
    };

    let tables = store.ids_of(DbType::MTable);
    assert!(
        tables.len() > 400,
        "only {} motion table(s) enumerated; the retail figure is 436",
        tables.len()
    );
    let (mut loaded, mut opened, mut paired) = (0u32, 0u32, 0u32);
    let mut first = String::new();
    // Tables whose `On` opened but whose `Off` raised nothing inside the window. Recorded and
    // reported rather than asserted away — see the note on this test.
    let mut unpaired: Vec<DataId> = Vec::new();
    for mt in &tables {
        let mut driver = MotionDriver::new(Arc::clone(&assets));
        assert!(
            driver.set_setup(Arc::clone(&setup_data)),
            "part-array setup creation refused the door setup"
        );
        if !driver.set_motion_table(*mt) {
            continue;
        }
        loaded += 1;
        let on = play(&mut driver, MotionCommand::ON);
        if on.first() != Some(&true) {
            continue;
        }
        opened += 1;
        let off = play(&mut driver, MotionCommand::OFF);
        if off.first() == Some(&false) {
            paired += 1;
            if first.is_empty() {
                first = format!("{:#010X}: On raised {on:?}, Off raised {off:?}", mt.0);
            }
        } else {
            unpaired.push(*mt);
        }
    }
    eprintln!(
        "ethereal emitter: {loaded} of {} motion table(s) loaded against the door setup; {opened} \
         raise SetEthereal(true) on ON, {paired} of those raise SetEthereal(false) on OFF, \
         {} did not inside a 3 s window. First pair: {first}. Unpaired: {:?}",
        tables.len(),
        unpaired.len(),
        unpaired
            .iter()
            .take(6)
            .map(|d| format!("{:#010X}", d.0))
            .collect::<Vec<_>>()
    );
    assert!(
        opened >= 20,
        "only {opened} motion table(s) raised SetEthereal(true) on ON; the census counted 37 \
         carrying the hook on the ON/OFF pair, so the emitter is not reaching the data at all"
    );
    assert!(
        paired >= 20,
        "only {paired} motion table(s) raised SetEthereal(false) on OFF. The OFF direction is \
         the half a screenshot cannot see: a door that opens and never closes is worse than one \
         that never opens"
    );
}

// =============================================================================================
// 3. The differential — a body walking at a retail door, open and closed
// =============================================================================================

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

/// The door as an **object-stream** body: `ObjectPhysics::spawn` reduced to one object. It is
/// `dynamic` — not `STATIC_PS` — because a door has a motion table and animates, and because the
/// object-collision search's ethereal exemption applies only to a non-static candidate (the
/// physics crate's `a_static_ethereal_candidate_still_blocks`).
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
        // The descriptor's bits are **or**-ed in rather than written wholesale.
        //
        // `HAS_PHYSICS_BSP_PS` comes from scanning the parts for a physics BSP, and object
        // creation sets it from the geometry; replacing the whole state word would clear it. No
        // door weenie's stored `PhysicsState` in the ACE world database carries the bit (0 of
        // 542). Keeping it here leaves the differential exactly one variable, `ETHEREAL_PS`.
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
        "the door must keep the BSP arm or the control walks through a closed door"
    );
    w.calc_cross_cells(h, true);
    h
}

/// **The two `PhysicsState` words retail doors are actually created with**, from an independent
/// census of ACE World Database v0.9.294: of 542 `WeenieType::Door` weenies, 491 carry
/// `0x18` and 38 carry `0x08` (the remaining 13 are `0x1C` and `0x818` — these two plus bits that
/// do not change the collision answer). **`Static` is 0 of 542**, and `HasPhysicsBSP` is 0 of 542 because the BSP arm is reached by
/// scanning the parts rather than by reading the descriptor bit.
///
/// Both are run, because once the door is ethereal they take **different collision branches**:
///
/// * `0x18` carries `IGNORE_COLLISIONS_PS`, so `ETHEREAL_PS | IGNORE_COLLISIONS_PS` matches the
///   function's **first statement** and the candidate is skipped before any sphere work;
/// * `0x08` does not, so it checks the ethereal obstruction and records the collision instead.
///
/// A build that only worked for one of them would look entirely correct on 491 doors out of 542.
const RETAIL_DOOR_STATES: [(u32, &str); 2] = [
    (
        0x0000_0018,
        "ReportCollisions|IgnoreCollisions -- 491 of 542 doors",
    ),
    (0x0000_0008, "ReportCollisions -- 38 of 542 doors"),
];

fn run(w: &mut PhysicsWorld, seconds: f64) {
    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    while t < seconds {
        t += dt;
        w.use_time(LocalTime(t), false);
    }
}

/// How far a body at `p` got **along its own approach** from `start`, in metres.
///
/// A straight-line displacement `hypot(p.x - start.x, p.y - start.y)` is a proxy and not the
/// quantity the door blocks: a body **deflected sideways** by a closed door is displaced further
/// from its start than one that walked into the doorway, so it can read "the open door stopped
/// it" of a walk that went straight through. Calibrated in both directions by
/// [`the_projected_advance_and_a_straight_line_displacement_part_company_for_a_deflected_body`].
fn advance_along(start: Vec3, step: Vec3, p: Vec3) -> f32 {
    p.sub(start).dot(step.normalize())
}

/// The same walk, sampling the body's z every step, and returning the highest it ever got above
/// the lowest it ever held.
///
/// Without the rise, the horizontal plane alone cannot tell a body that **climbed over** the
/// door's mesh from one that **walked through** it, and with the step-up path the control can
/// climb this door.
///
/// The whole path is returned too, so a caller can ask about **every** frame rather than the
/// last one: did the control ever get inside the door's mesh, and did it actually stop. A body
/// that walks *through* a door ends outside its mesh exactly as a body stopped by it does, and a
/// body that slides past one is still moving when the clock stops.
fn run_tracking_path(w: &mut PhysicsWorld, h: PhysHandle, seconds: f64) -> (Vec3, f32, Vec<Vec3>) {
    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut path: Vec<Vec3> = Vec::new();
    let mut end = Vec3::ZERO;
    while t < seconds {
        t += dt;
        w.use_time(LocalTime(t), false);
        end = w.get(h).expect("live").position.frame.origin;
        path.push(end);
    }
    let zs: Vec<f32> = path.iter().map(|p| p.z).collect();
    (end, rise_after_landing(&zs), path)
}

/// The reduction: how far the body rose above the floor **after it first reached the floor**.
///
/// Kept separate from [`run_tracking_path`] so its calibration can test it directly: the walks
/// compared here rise 0.000 m and land on frame 0, so they cannot see the tolerance.
///
/// **The `<= floor + 0.01` is load-bearing and an exact match is wrong.** A body that climbs the
/// Holtburg dinnertable and walks off settles at **66.00490** against the **66.00500** it landed
/// at, so `z == floor` picks a frame *after* the climb and reports a rise of exactly zero. That
/// series is the calibration below.
fn rise_after_landing(zs: &[f32]) -> f32 {
    let floor = zs.iter().copied().fold(f32::MAX, f32::min);
    let landed = zs.iter().position(|z| *z <= floor + 0.01).unwrap_or(0);
    let peak = zs[landed..].iter().copied().fold(f32::MIN, f32::max);
    peak - floor
}

/// **The z reduction, calibrated.**
///
/// Three series, each one a real shape the walk produces, and the reduction must tell them
/// apart. The middle one matters: a body that climbs something and comes back off it settles a
/// ten-thousandth of a metre **below** the z it landed at, so an exact `z == floor` picks a frame
/// after the climb and reports **no rise at all**.
#[test]
fn the_rise_metric_reads_a_climb_and_not_the_drop_a_spawn_leaves() {
    // A body that spawns half a metre up, falls, and then walks on the flat. The drop is not a
    // rise: everything after the landing frame is level, so the answer is zero.
    let dropped = [66.505, 66.300, 66.005, 66.005, 66.005, 66.005];
    assert!(
        rise_after_landing(&dropped).abs() < 1e-3,
        "{}",
        rise_after_landing(&dropped)
    );

    // The dinnertable: land at 66.00500, climb to 66.92500, walk off, settle at 66.00490 --
    // a hair BELOW where it landed, which is what makes the tolerance load-bearing.
    let climbed = [66.505, 66.005, 66.530, 66.925, 66.925, 66.400, 66.004_9];
    let rise = rise_after_landing(&climbed);
    assert!(
        (rise - 0.92010).abs() < 1e-3,
        "the climb must read as 0.920 m above the floor it landed on, not {rise}"
    );

    // And with the tolerance removed the same series reads zero, which is the failure this
    // guards: the only frame at exactly the minimum is the last one, after the climb.
    let floor = climbed.iter().copied().fold(f32::MAX, f32::min);
    let exact = climbed.iter().position(|z| *z <= floor).unwrap_or(0);
    assert_eq!(
        exact,
        climbed.len() - 1,
        "the exact-match landing frame is the one after the climb"
    );

    // A body that only ever falls -- `teleport`'s half-metre drop with no walk afterwards --
    // must not read as a rise either.
    let fell = [66.505, 66.400, 66.200, 66.005];
    assert!(rise_after_landing(&fell).abs() < 1e-3);
}

/// **The calibration for [`advance_along`], in both directions.**
///
/// A judge nobody checks is not a judge: without this the projected advance could be replaced by
/// the straight-line displacement it was written to remove and every assertion in this file would
/// stay green, because on the compared corpus the two happen to agree. So this states the case
/// where they *must* disagree, with numbers that are arithmetic rather than measurement.
///
/// A body walking 3 m north and pushed 4 m east by a closed door is **5.000 m** from where it
/// started and has advanced **3.000 m** along its approach. A body that walked straight through
/// the doorway to 4 m north is **4.000 m** away and has advanced **4.000 m** -- less displaced
/// and further along, which is the inversion that made the proxy wrong. And a body driven
/// *backwards* advances a negative distance where a displacement is unsigned.
#[test]
fn the_projected_advance_and_a_straight_line_displacement_part_company_for_a_deflected_body() {
    let start = Vec3::new(10.0, 20.0, -12.0);
    let step = Vec3::new(0.0, 0.4, 0.0);
    let displaced = |p: Vec3| math::hypotf(p.x - start.x, p.y - start.y);

    let deflected = Vec3::new(14.0, 23.0, -12.0);
    let through = Vec3::new(10.0, 24.0, -12.0);

    assert!((advance_along(start, step, deflected) - 3.0).abs() < 1e-4);
    assert!((displaced(deflected) - 5.0).abs() < 1e-4);
    assert!((advance_along(start, step, through) - 4.0).abs() < 1e-4);
    assert!((displaced(through) - 4.0).abs() < 1e-4);

    // The inversion, stated as the comparison this file actually makes.
    assert!(
        advance_along(start, step, through) > advance_along(start, step, deflected),
        "the body that walked through the doorway must advance further than the deflected one"
    );
    assert!(
        displaced(through) < displaced(deflected),
        "and the straight-line displacement says the opposite, which is why it was a proxy"
    );

    // Backwards is negative, where a displacement cannot be.
    let pushed_back = Vec3::new(10.0, 18.5, -12.0);
    assert!((advance_along(start, step, pushed_back) + 1.5).abs() < 1e-4);
    assert!(displaced(pushed_back) > 0.0);

    // The step's magnitude must not reach the answer: the same approach at a tenth the sub-step
    // size is the same approach.
    let tenth = Vec3::new(0.0, 0.04, 0.0);
    assert!(
        (advance_along(start, step, deflected) - advance_along(start, tenth, deflected)).abs()
            < 1e-4
    );
}

struct Approach {
    start: Vec3,
    step: Vec3,
}

/// Every clear run-up at `origin`, forty degrees off the line of approach, as in
/// `collision_obstacles` and `mesh_collision`: a head-on walk needs an exactly flat contact plane
/// and an exactly perpendicular heading to meet the collision adjustment's crease case, which is
/// not what this module tests. The animation offset is zeroed only when `ON_WALKABLE_TS` is clear;
/// the stages that forward it never read a transition result.
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

/// A point of an interior cell a body can stand at.
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

/// Behaviour: objects.door.an-opened-door-stops-blocking
/// **The acceptance line.** The same body, the same room, the same retail door, the same walk —
/// the single variable is `ETHEREAL_PS`, and it is changed only through the physics world's
/// ethereal-state operation.
///
/// Closed, the body stops short of the mesh. Ethereal, it walks through and ends **inside** the
/// door's own geometry, which is what "the door no longer blocks you" means when the door is
/// still standing there and still drawn.
///
/// And back again, which is the half a screenshot cannot see: with the body standing in the
/// doorway the door is **refused** the return to solid and latched for retry; once the body is
/// out, one physics sweep clears it with nothing else asking. A door that goes ethereal and never
/// comes back is worse than one that never opens.
#[test]
fn an_opened_retail_door_stops_blocking_and_a_closing_one_will_not_close_on_a_body() {
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let src =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the retail height table"));
    src.load_block_cells(dereth_primitives::LandblockId(TRAINING_DUNGEON));

    let decoded = setup(&store, DataId(DOOR));
    let mut stats = SetupPartStats::default();
    let geometry = Arc::new(setup_geometry_with_parts(&store, &decoded, &mut stats));
    assert!(
        geometry.spheres.is_empty() && geometry.caches_physics_bsp(),
        "the retail door is BSP-only; mesh_collision's census says so and this test relies on it"
    );

    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);

    // Both of the state words retail creates doors with; see [`RETAIL_DOOR_STATES`].
    for (door_state, label) in RETAIL_DOOR_STATES {
        let mut compared = 0usize;
        let mut deferrals_seen = 0u64;
        let mut best = String::new();

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
            let frame = Frame::new(Vec3::new(at.x, at.y, at.z - 0.5), Quat::IDENTITY);
            let pos = Position::new(d.id, frame);

            // One trial: build the world, register the door, optionally hand it the hook the opening
            // animation raises, then walk.
            let trial = |ethereal: bool, a: &Approach| -> (Vec3, f32, Vec<Vec3>) {
                let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
                let door = register_door(&mut w, d.id, frame, Arc::clone(&geometry), door_state);
                if ethereal {
                    // The opening hook enables ethereal state with a literal false second flag.
                    assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
                }
                let h = spawn(&mut w, d.id, a.start, a.step);
                run_tracking_path(&mut w, h, 4.0)
            };

            for a in approaches(&geom, frame.origin, at.z) {
                if inside_object(&geometry, &pos, a.start) {
                    continue;
                }
                // How far the body got **along its own approach**, which is the axis the door
                // blocks, not the straight-line displacement from the start (see
                // [`advance_along`]).
                let advance = |p: Vec3| advance_along(a.start, a.step, p);

                // The CLOSED run is the control and it decides whether this approach is a walk into
                // *this* door: it must travel, and it must be stopped short of the mesh.
                let (closed, closed_rise, closed_path) = trial(false, &a);
                let closed_travel = advance(closed);
                // The third clause is the one that makes this a control. "Travelled, and did not
                // end inside the mesh" is satisfied by a body that walked straight past the door --
                // see [`came_to_rest`]. A closed door is a control for "the closed door
                // stopped it" only if the body it is compared against actually **stopped**.
                if closed_travel <= 0.5
                    || inside_object(&geometry, &pos, closed)
                    || !came_to_rest(&closed_path)
                {
                    continue;
                }
                let (open, open_rise, open_path) = trial(true, &a);
                let open_travel = advance(open);
                let closed_in_mesh = frames_inside(&geometry, &pos, &closed_path);
                let open_in_mesh = frames_inside(&geometry, &pos, &open_path);
                if !inside_object(&geometry, &pos, open) {
                    // The walk did reach the door when it was solid, but with the door ethereal the
                    // body ended somewhere else in the room rather than inside the doorway. That is
                    // not a failure of the mechanism, it is an approach that grazes; try the next.
                    continue;
                }

                // A control that climbed over the door's own geometry is not a control for `the
                // closed door stopped it`. It is asserted **here**, of the pair that is actually
                // compared, and not of every run-up: the training-academy cells have ramps, and a
                // body walking up one legitimately rises as much as **2.355 m** on run-ups the two
                // guards above then discard. On the compared pair the closed control rises
                // **0.000 m**.
                assert!(
                closed_rise <= 0.05,
                "cell {:#010X} ({label}): the control body rose {closed_rise:.3} m walking at a \
                 CLOSED door while the ethereal one rose {open_rise:.3} m, so it went over the \
                 mesh rather than being stopped by it and the differential below is not about \
                 the door",
                d.id.0
            );
                // The pair this loop actually compares, printed whether or not it passes; the
                // travel figures are printed beside the frames-in-mesh counts that judge it.
                eprintln!(
                    "door pair: cell={:#010X} closed_travel={closed_travel:.4} \
open_travel={open_travel:.4} closed_rise={closed_rise:.4} open_rise={open_rise:.4} \
closed_in_mesh={closed_in_mesh}/{} open_in_mesh={open_in_mesh}/{} \
start=({:.3},{:.3},{:.3}) closed_end=({:.3},{:.3},{:.3}) open_end=({:.3},{:.3},{:.3}) \
door=({:.3},{:.3},{:.3})",
                    d.id.0,
                    closed_path.len(),
                    open_path.len(),
                    a.start.x,
                    a.start.y,
                    a.start.z,
                    closed.x,
                    closed.y,
                    closed.z,
                    open.x,
                    open.y,
                    open.z,
                    frame.origin.x,
                    frame.origin.y,
                    frame.origin.z,
                );
                // **The direct measurement.** A closed door is solid on every frame of the walk,
                // not merely on the last one, and this is asserted rather than filtered: the guard
                // above only looks at where the control ended, so a control that passed *through*
                // the door and out the far side would satisfy it.
                assert_eq!(
                    closed_in_mesh,
                    0,
                    "cell {:#010X} ({label}): the CLOSED door was penetrated on {closed_in_mesh} \
                 of {} frames, so the control walked through it",
                    d.id.0,
                    closed_path.len(),
                );
                assert!(
                    open_in_mesh > 0,
                    "cell {:#010X} ({label}): the ethereal body was never inside the door's mesh \
                 on any of {} frames, so the hook changed nothing",
                    d.id.0,
                    open_path.len(),
                );
                // **And the differential itself is that same direct reading, not a displacement.**
                // Even the advance along the approach axis misjudges this fixture in both
                // directions. Cell `0x86020105`: the closed control advances 4.893 m *past* the
                // door and is never inside it, while the ethereal body advances 2.470 m, is inside
                // the mesh on 84 frames and stops on the doorway's 0.409 m threshold. Cell
                // `0x86020108`: the ethereal body ends inside the mesh on 100 of 121 frames and the
                // closed one never, yet the closed one projects 7 mm further along the approach
                // axis, because the two paths curve differently.
                //
                // The property meant is *the body got into the doorway, and closed it could not*,
                // and that is what is asserted. It is not vacuous: the guard above only requires
                // the ethereal body's **last** frame to be inside the mesh, and `compared >= 3`
                // below requires three separate rooms to have produced such a pair at all, so a
                // build in which the hook did nothing reaches neither.
                // `the_projected_advance_and_a_straight_line_displacement_part_company_for_a_deflected_body`
                // in this file is the general statement of why a displacement cannot be the judge.
                assert!(
                    open_in_mesh > closed_in_mesh,
                    "cell {:#010X} ({label}): the ethereal door still stopped the body -- it was \
                 inside the door's own mesh on {open_in_mesh} of {} frames against \
                 {closed_in_mesh} closed ({open_travel:.2} m along the approach against \
                 {closed_travel:.2} m; the open body rose {open_rise:.3} m and the closed one \
                 {closed_rise:.3} m, so the closed one {})",
                    d.id.0,
                    open_path.len(),
                    if closed_rise > 0.05 {
                        "CLIMBED the door's geometry"
                    } else {
                        "stayed at floor level"
                    }
                );

                // ---- and back again, in the same world ------------------------------------------
                let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
                let door = register_door(&mut w, d.id, frame, Arc::clone(&geometry), door_state);
                assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
                if let Some(o) = w.get_mut(door) {
                    o.transient_state.set_active_bit(true);
                    o.update_time = 0.0;
                }
                let h = spawn(&mut w, d.id, a.start, a.step);
                run(&mut w, 4.0);
                let resting = w.get(h).expect("live").position.frame.origin;
                assert!(
                    inside_object(&geometry, &pos, resting),
                    "the body was meant to be standing in the open doorway"
                );

                // The closing animation's hook fires while the body is in the doorway.
                let before = w.ethereal_deferrals();
                assert_eq!(
                    w.set_ethereal(door, false, false),
                    EtherealResult::Deferred,
                    "cell {:#010X}: the door closed on top of a body standing in it",
                    d.id.0
                );
                assert!(
                    w.get(door).expect("live").state.is_ethereal(),
                    "the bit must be put back, not left clear"
                );
                assert!(w.get(door).expect("live").transient_state.check_ethereal());
                assert!(w.ethereal_deferrals() > before);
                deferrals_seen += w.ethereal_deferrals() - before;

                // The body walks out. Nothing tells the door anything; the retry inside the
                // per-frame physics-object update is what makes it solid again.
                let out_of_the_way = Position::new(d.id, Frame::new(a.start, Quat::IDENTITY));
                w.force_into_cell(h, &out_of_the_way);
                // Well clear of where `run` left the world clock, and in 0.05 s steps: below
                // `MIN_QUANTUM` (1/30 s) `use_time` sweeps **nothing** and answers `false`, and its
                // return is checked rather than discarded so that "the retry ran and refused" cannot
                // be confused with "the retry never ran".
                let mut t = 10.0;
                for _ in 0..4 {
                    t += 0.05;
                    assert!(
                        w.use_time(LocalTime(t), false),
                        "the sweep that must clear it never ran"
                    );
                }
                assert!(
                    !w.get(door).expect("live").state.is_ethereal(),
                    "cell {:#010X}: the door never came back -- it is ethereal for ever, which is \
                 worse than never opening",
                    d.id.0
                );
                assert!(!w.get(door).expect("live").transient_state.check_ethereal());

                if best.is_empty() {
                    best = format!(
                        "cell {:#010X}: a retail `door` at ({:.2}, {:.2}, {:.2}) -- from ({:.2}, \
                     {:.2}) the CLOSED door stopped the body after {closed_travel:.2} m at \
                     ({:.2}, {:.2}), outside its mesh; with the opening animation's ethereal hook \
                     applied the same walk covered {open_travel:.2} m to ({:.2}, {:.2}), INSIDE \
                     the doorway. Closing it while the body stood there was refused and latched; \
                     one sweep after the body left, the door was solid again.",
                        d.id.0,
                        frame.origin.x,
                        frame.origin.y,
                        frame.origin.z,
                        a.start.x,
                        a.start.y,
                        closed.x,
                        closed.y,
                        open.x,
                        open.y
                    );
                }
                compared += 1;
                break;
            }
        }

        eprintln!("state {door_state:#010X} ({label}) -- {best}");
        eprintln!(
            "door state {door_state:#010X}: {compared} room(s) gave a usable walk; \
         {deferrals_seen} refusal(s) observed"
        );
        assert!(
        compared >= 3,
        "state {door_state:#010X} ({label}): only {compared} room(s) gave a usable walk into a \
         door, so the differential is not measuring enough to be a measurement"
    );
        assert!(
            deferrals_seen >= 3,
            "state {door_state:#010X}: the overlap guard never fired: {deferrals_seen} refusal(s)"
        );
    }
}
// =============================================================================================
// 4. What decides the refusal: where the body rests
// =============================================================================================

/// `set_ethereal(false)`'s refusal is a function of **where the body rests**, and of nothing
/// else — not of anything in the collision tree's `CONTACT` arm.
///
/// Two resting positions of the acceptance walk in cell `0x86020100`, `y = -235.032928` and
/// `y = -236.100128` (1.067 m apart, the second past the `0x86020100`/`0x86020101` boundary the
/// door stands on), both refuse, with the body's cell held at `0x86020101`. The door is
/// registered in both cells because mesh objects take their cell list from their part bounding
/// boxes, so the answer does not depend on which side of the boundary the body came to rest.
///
/// The ethereal collision check asks two position questions in series: is the body registered in
/// one of the **door's** shadow cells at all, and if so does a zero-length `PLACEMENT_INSERT` say
/// the two overlap. **Both are decided by where the body is standing and neither reads the
/// `CONTACT` arm**, because the collision tree's first branch answers a placement insert with a
/// solid-sphere intersection test and returns.
///
/// The two questions are *not* the same, which is why both are counted here: over the sweep the
/// body never leaves the door's shadow cells (42 of 42) and yet the refusal stops (7 of 42), and
/// `inside_object` below — this file's own solid-sphere overlap question — disagrees with the
/// shadow membership on exactly those 7. What stops the refusal on those 7 is `check_collision`,
/// not registration: a door refuses to close on a body it overlaps, not on a body that happens to
/// share a cell with it.
///
/// The sweep is what makes it a measurement rather than two anecdotes: the body is parked at
/// 0.1 m intervals along the walk direction, the answer must flip **once** and never back, and
/// both answers must occur, with the denominator asserted.
#[test]
fn the_doors_refusal_is_a_function_of_where_the_body_rests_and_flips_once_along_the_walk() {
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let src =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the retail height table"));
    src.load_block_cells(dereth_primitives::LandblockId(TRAINING_DUNGEON));

    let decoded = setup(&store, DataId(DOOR));
    let mut stats = SetupPartStats::default();
    let geometry = Arc::new(setup_geometry_with_parts(&store, &decoded, &mut stats));

    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);
    let door_state = RETAIL_DOOR_STATES[0].0;

    let (mut rooms, mut samples, mut deferred) = (0usize, 0usize, 0usize);
    let (mut mesh_disagrees, mut flips, mut shadowed_count) = (0usize, 0usize, 0usize);
    let mut first = String::new();

    for d in &cells {
        if rooms >= 2 {
            break;
        }
        let Some(geom) = LandSource::env_cell(src.as_ref(), d.id) else {
            continue;
        };
        let Some(at) = a_standable_point(&geom) else {
            continue;
        };
        let frame = Frame::new(Vec3::new(at.x, at.y, at.z - 0.5), Quat::IDENTITY);
        let pos = Position::new(d.id, frame);

        // The acceptance walk's own approach selection, so the sweep starts where that test
        // leaves the body rather than at a point invented here.
        let mut chosen = None;
        for a in approaches(&geom, frame.origin, at.z) {
            if inside_object(&geometry, &pos, a.start) {
                continue;
            }
            let closed = {
                let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
                register_door(&mut w, d.id, frame, Arc::clone(&geometry), door_state);
                let h = spawn(&mut w, d.id, a.start, a.step);
                run(&mut w, 4.0);
                w.get(h).expect("live").position.frame.origin
            };
            let travel = math::hypotf(closed.x - a.start.x, closed.y - a.start.y);
            if travel <= 0.5 || inside_object(&geometry, &pos, closed) {
                continue;
            }
            let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
            let door = register_door(&mut w, d.id, frame, Arc::clone(&geometry), door_state);
            assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
            let h = spawn(&mut w, d.id, a.start, a.step);
            run(&mut w, 4.0);
            let body = w.get(h).expect("live");
            let (resting, cell) = (body.position.frame.origin, body.position.cell);
            if !inside_object(&geometry, &pos, resting) {
                continue;
            }
            chosen = Some((a, resting, cell));
            break;
        }
        let Some((a, resting, walk_cell)) = chosen else {
            continue;
        };
        rooms += 1;

        let mut along = Vec3::new(a.step.x, a.step.y, 0.0);
        assert!(
            !along.normalize_check_small(),
            "the approach has a horizontal direction"
        );

        let mut prev: Option<bool> = None;
        for k in 0..=20u8 {
            let p = resting.add(along.mul(f32::from(k) * 0.1));
            let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
            let door = register_door(&mut w, d.id, frame, Arc::clone(&geometry), door_state);
            assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
            let h = spawn(&mut w, d.id, p, a.step);
            // Park it. `force_into_cell` re-runs `calc_cross_cells`, so the shadow list read
            // below is the one the geometry implies rather than one a walk happened to leave.
            w.force_into_cell(h, &Position::new(walk_cell, Frame::new(p, Quat::IDENTITY)));

            let door_cell = w.get(door).expect("live").position.cell;
            let shadowed = w
                .get(h)
                .expect("live")
                .shadow_objects
                .iter()
                .any(|s| s.cell_id == door_cell);
            let answer = w.set_ethereal(door, false, false);
            let in_the_mesh = inside_object(&geometry, &pos, p);

            samples += 1;
            let refused = answer == EtherealResult::Deferred;
            if prev.is_some_and(|q| q != refused) {
                flips += 1;
            }
            prev = Some(refused);
            if shadowed {
                shadowed_count += 1;
            }
            if answer == EtherealResult::Deferred {
                deferred += 1;
            }
            if in_the_mesh != shadowed {
                mesh_disagrees += 1;
            }
            if !refused && first.is_empty() {
                first = format!(
                    "cell {:#010X}: at ({:.3}, {:.3}, {:.3}), {:.1} m along the walk from the \
                     resting position, the door is allowed to close -- the body is {}in the \
                     door's own mesh and {}in its shadow cell {:#010X}",
                    d.id.0,
                    p.x,
                    p.y,
                    p.z,
                    f32::from(k) * 0.1,
                    if in_the_mesh { "still " } else { "no longer " },
                    if shadowed { "still " } else { "no longer " },
                    door_cell.0
                );
            }
        }
    }

    eprintln!(
        "refusal sweep: {rooms} room(s), {samples} parked sample(s); {deferred} refused, {} allowed, \
         {flips} flip(s) along the sweeps; still in the door's shadow cell on {shadowed_count}; \
         `inside_object` disagrees with the shadow membership on {mesh_disagrees}",
        samples - deferred
    );
    eprintln!("refusal sweep: {first}");

    assert!(
        rooms >= 2,
        "only {rooms} room(s) gave a walk that ends in a doorway"
    );
    assert!(
        samples >= 40,
        "only {samples} parked sample(s): the sweep is not a measurement"
    );
    // Both directions are required: a sweep that only ever refused, or only ever allowed, is not
    // measuring the thing that changes.
    assert!(
        deferred >= 1,
        "no sample refused: the sweep never stood in the doorway"
    );
    assert!(
        samples - deferred >= 1,
        "every sample refused: the sweep never cleared the door"
    );
    // **The answer is a function of how far along the walk the body is.** One transition per
    // sweep, never back again -- a refusal that alternated with distance would not be a position
    // question at all.
    assert!(
        flips >= 1 && flips <= rooms,
        "{flips} flip(s) across {rooms} sweep(s): the refusal is not a monotone function of how \
         far the body has walked, so it is not the resting position that decides it"
    );
    // The refusal is **not** `inside_object`'s question. Measured here they disagree on 7 of 42.
    assert!(
        mesh_disagrees >= 1,
        "`inside_object` and the shadow membership agreed on all {samples} sample(s), so this \
         sweep cannot tell the two questions apart"
    );

    // ---- the literal pin: two resting positions, 1.067 m apart ------------------------------
    //
    // `y = -235.032928` and `y = -236.100128` in cell `0x86020100`'s first usable approach; the
    // second is 1.1 m past the `0x86020100`/`0x86020101` boundary the door stands on. Both are
    // `Deferred`: the door's bounding-box cell list gives it **both** cells its parts reach, so the
    // answer does not depend on which side of the boundary the body rests. Asserting them as
    // literals makes that falsifiable: if a later change moves either answer, this says so.
    let walked_cell = CellId(0x8602_0101);
    let door_room = cells
        .iter()
        .find(|c| c.id.0 == 0x8602_0100)
        .expect("cell 0x86020100");
    let geom = LandSource::env_cell(src.as_ref(), door_room.id).expect("its geometry");
    let at = a_standable_point(&geom).expect("a standable point in it");
    let frame = Frame::new(Vec3::new(at.x, at.y, at.z - 0.5), Quat::IDENTITY);
    for (label, y) in [
        ("as found", -235.032_93_f32),
        ("coordinate space corrected", -236.100_13_f32),
    ] {
        let p = Vec3::new(150.161_71, y, -12.0);
        let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
        let door = register_door(
            &mut w,
            door_room.id,
            frame,
            Arc::clone(&geometry),
            door_state,
        );
        assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
        let h = spawn(&mut w, door_room.id, p, Vec3::new(0.0, 0.0, 0.0));
        w.force_into_cell(
            h,
            &Position::new(walked_cell, Frame::new(p, Quat::IDENTITY)),
        );
        let got = w.set_ethereal(door, false, false);
        eprintln!("resting-position pin: {label} resting position y={y:.6} -> {got:?}");
        assert_eq!(
            got,
            EtherealResult::Deferred,
            "the {label} resting position ({:.6}, {y:.6}, {:.1}) answered {got:?}. Both stations \
             are inside the door's mesh, so both must refuse; `Applied` would be a door closing on \
             a body standing in it",
            p.x,
            p.z
        );
    }
}

// =============================================================================================
// 5. What makes the refusal work: the door's cell registration
// =============================================================================================

/// `set_ethereal(false)`'s refusal is decided by **cell registration** -- and the retail door is
/// registered in **two** cells because its mesh takes the bounding-box arm before the sorting
/// sphere can be considered.
///
/// The door's `sorting_sphere` is `(0,0,0) r = 0` -- pinned below -- but **this door does not take
/// the sphere arm**. The calculation has three arms, in this order of precedence:
///
/// 1. `HAS_PHYSICS_BSP_PS` selects the bounding-box cell list. **This is the first test**, so it
///    beats both sphere arms; scanning the parts raises the bit for any object whose parts carry
///    a mesh. Every door does.
/// 2. An object with collision cylinders supplies **all** of them to the cell-list calculation.
/// 3. Otherwise the object supplies exactly one sorting sphere, including a zero-radius sphere.
///
/// The bounding-box arm adds the object's own cell and then checks the parts against every cell
/// in the array as it grows. In an interior cell, it compares each part's graphics bounding box
/// with each portal plane and then tests that box in the cell beyond the portal.
///
/// So the door's shadow set is **its own cell plus every cell its mesh reaches through a
/// portal**: the door stays ethereal, with `CHECK_ETHEREAL_TS` pending, until nothing is inside
/// it -- and "inside it" is decided by geometry, not by which side of a doorway the body's cell id
/// says it is on. With a one-cell registration, a body 1.067 m past the boundary would be in a
/// cell nothing searched, and the door would close on it.
#[test]
fn the_refusal_is_decided_by_cell_registration_and_the_doors_mesh_registers_it_in_two_cells() {
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let src =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the retail height table"));
    src.load_block_cells(dereth_primitives::LandblockId(TRAINING_DUNGEON));

    let decoded = setup(&store, DataId(DOOR));
    let mut stats = SetupPartStats::default();
    let geometry = Arc::new(setup_geometry_with_parts(&store, &decoded, &mut stats));

    // **1. The shipped values, as literals.** The sorting sphere is zero and pinned: a decoder
    // change that gave this door a bounding sphere would say so here. It is **not what selects the
    // arm** -- `caches_physics_bsp` is, and it is asserted beside it so that the two cannot drift
    // apart silently.
    assert_eq!(
        (
            decoded.sorting_sphere.radius,
            decoded.radius,
            decoded.height
        ),
        (0.0, 0.0, 0.0),
        "the retail door {DOOR:#010X} carries no sorting sphere, no radius and no height"
    );
    assert!(
        decoded.selection_sphere.radius > 1.0,
        "the selection sphere is {:.3}; if it were zero too this would be a decoder fault \
         rather than the shipped geometry",
        decoded.selection_sphere.radius
    );
    assert!(
        geometry.path_spheres().is_empty(),
        "the door is BSP-only, as mesh_collision's census says"
    );
    assert!(
        geometry.cyl_spheres.is_empty(),
        "a cylsphere would take arm 2 of `calc_cross_cells` and this test would be about that"
    );
    assert!(
        geometry.caches_physics_bsp(),
        "scanning the part array for physics BSPs is what raises `HAS_PHYSICS_BSP_PS`, and that \
         bit selects `calc_cross_cells`'s FIRST arm. Without it this \
         door would fall through to the sorting sphere and occupy one cell"
    );
    // The bounding box is the only geometry the bbox arm reads, so it is pinned too. Zero here
    // would make `find_bbox_cell_list` a no-op that looks exactly like a one-cell answer.
    let boxed = geometry
        .parts
        .iter()
        .filter(|p| p.bound_box.is_some_and(|b| b.max.y > b.min.y));
    assert_eq!(
        boxed.count(),
        3,
        "all three of the door's parts must carry a non-degenerate `gfx_bound_box`; \
         graphics-object finalization computes it from the vertex array"
    );

    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);
    let door_state = RETAIL_DOOR_STATES[0].0;
    let door_room = cells
        .iter()
        .find(|c| c.id.0 == 0x8602_0100)
        .expect("cell 0x86020100");
    let geom = LandSource::env_cell(src.as_ref(), door_room.id).expect("its geometry");
    let at = a_standable_point(&geom).expect("a standable point in it");
    let frame = Frame::new(Vec3::new(at.x, at.y, at.z - 0.5), Quat::IDENTITY);
    let pos = Position::new(door_room.id, frame);

    // The two resting positions of the acceptance walk, and the two cells the body can be
    // registered in there. `force_into_cell` re-runs `calc_cross_cells`,
    // so each shadow list below is the one the geometry implies.
    let probe = |p: Vec3, forced: CellId| -> (EtherealResult, Vec<CellId>, Vec<CellId>) {
        let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
        let door = register_door(
            &mut w,
            door_room.id,
            frame,
            Arc::clone(&geometry),
            door_state,
        );
        assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
        let h = spawn(&mut w, door_room.id, p, Vec3::ZERO);
        w.force_into_cell(h, &Position::new(forced, Frame::new(p, Quat::IDENTITY)));
        let door_shadows = w
            .get(door)
            .expect("live")
            .shadow_objects
            .iter()
            .map(|s| s.cell_id)
            .collect();
        let body_shadows = w
            .get(h)
            .expect("live")
            .shadow_objects
            .iter()
            .map(|s| s.cell_id)
            .collect();
        (
            w.set_ethereal(door, false, false),
            door_shadows,
            body_shadows,
        )
    };

    let walked = CellId(0x8602_0101);
    let doors_own = CellId(0x8602_0100);
    let as_found = Vec3::new(150.161_71, -235.032_93, -12.0);
    let corrected = Vec3::new(150.161_71, -236.100_13, -12.0);

    // **2. The door occupies TWO cells, and the second is the one the walk ends in.**
    let (found_ans, door_shadows, found_body) = probe(as_found, walked);
    assert_eq!(
        door_shadows,
        vec![doors_own, walked],
        "the door's shadow set is {door_shadows:?}. This door stands with its origin on the \
         `0x86020100`/`0x86020101` boundary and its mesh in both, and the bounding-box portal \
         walk gives it both -- `[0x86020100]` alone is a one-cell answer, which would let a \
         door close on a player"
    );

    // **3. Both resting positions are inside the door's own mesh.** So the thing that differs
    // between them is not whether the body is standing in the door.
    for p in [as_found, corrected] {
        assert!(
            inside_object(&geometry, &pos, p),
            "({:.3}, {:.3}) is not inside the door's mesh, so this test is not about the \
             doorway at all",
            p.x,
            p.y
        );
    }

    // **4. And the answer does not follow the body's cell id.** Both stations refuse, and the
    // second does so while registered in `0x86020101` only.
    assert_eq!(found_ans, EtherealResult::Deferred);
    assert!(found_body.contains(&doors_own));

    let (corrected_ans, _, corrected_body) = probe(corrected, walked);
    assert!(
        !corrected_body.contains(&doors_own),
        "1.067 m further along the body no longer reaches the door's own cell; shadows are \
         {corrected_body:?}. That is the *precondition* for this assertion, not its subject: if \
         the body still reached back into `0x86020100` the next line would pass for the old \
         reason"
    );
    assert_eq!(
        corrected_ans,
        EtherealResult::Deferred,
        "the body is standing in the door's mesh and registered only in {walked:?}, and the \
         door's own shadow set reaches {walked:?} too, so the close must be refused"
    );

    // **5. The differential, kept as the discriminator.** Forcing the same corrected position
    // back into the door's own cell must also refuse -- a one-cell registration refuses here
    // too, so on its own it proves nothing. Its value is that it fails if the mechanism moves to
    // something other than registration plus overlap.
    let (forced_ans, _, forced_body) = probe(corrected, doors_own);
    assert!(forced_body.contains(&doors_own));
    assert_eq!(forced_ans, EtherealResult::Deferred);
}

// =============================================================================================
// 6. The three stations: nobody there, a body in the doorway, and the release
// =============================================================================================

/// Behaviour: objects.door.a-closing-door-will-not-close-on-a-body
/// A door cannot be closed on a body standing in its doorway, **on either side of the cell
/// boundary it stands on**, and it *can* be closed once the body leaves. In retail, a closing door
/// with something intersecting it keeps ethereal on, marks a pending flag and re-checks every
/// frame until nothing is inside it.
///
/// That is three stations, and a test that asserts only the refusal cannot see the third. The
/// order is:
///
/// * Disabling ethereal state clears `ETHEREAL_PS`; when the object has no parent and has a cell,
///   it then checks for collisions. On a hit it puts `ETHEREAL_PS` **back**, raises
///   `CHECK_ETHEREAL_TS (0x100)`, and returns failure.
/// * The per-frame object update retries that close only when the object is active, still has a
///   cell, and has the bit-8 `CHECK_ETHEREAL_TS` flag. The retry uses the same false/false flags.
///
/// So `Deferred` is a **pending flag re-tested every physics frame**, not a refusal that is
/// forgotten, and the third station below is what proves the flag clears. A flag that never
/// cleared would leave every door in the game permanently ethereal.
///
/// **Why both sides of the boundary.** This fixture's door stands with its origin exactly on the
/// `0x86020100`/`0x86020101` portal plane -- `a_standable_point` scans from the room's corner and
/// the first point inside the cell BSP is on it, as with `mesh_collision`'s door. A door
/// registered in one cell would answer by which side the body's own cell id put it on, with the
/// body inside the mesh in both; asserting one side only cannot see that.
#[test]
fn a_door_refuses_to_close_on_a_body_on_either_side_of_its_cell_boundary_and_closes_once_it_leaves()
{
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let src =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the retail height table"));
    src.load_block_cells(dereth_primitives::LandblockId(TRAINING_DUNGEON));

    let decoded = setup(&store, DataId(DOOR));
    let mut stats = SetupPartStats::default();
    let geometry = Arc::new(setup_geometry_with_parts(&store, &decoded, &mut stats));

    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);
    let door_room = cells
        .iter()
        .find(|c| c.id.0 == 0x8602_0100)
        .expect("cell 0x86020100");
    let geom = LandSource::env_cell(src.as_ref(), door_room.id).expect("its geometry");
    let at = a_standable_point(&geom).expect("a standable point in it");
    let frame = Frame::new(Vec3::new(at.x, at.y, at.z - 0.5), Quat::IDENTITY);
    let pos = Position::new(door_room.id, frame);

    // The two cells the door's mesh spans, and which of them a point is in. Asked of the cell
    // BSPs themselves, not of the door.
    let near = CellId(0x8602_0100);
    let far = CellId(0x8602_0101);
    let in_cell = |id: CellId, p: Vec3| -> bool {
        LandSource::env_cell(src.as_ref(), id).is_some_and(|g| {
            g.cell_bsp.as_ref().is_some_and(|b| {
                b.point_inside_cell_bsp(dereth_physics::math::globaltolocal(&g.frame, p))
            })
        })
    };

    // Walk the door's own y axis and take the first station inside the mesh on each side. The
    // search is asserted to have found both, so a fixture change that moved the door off the
    // boundary fails loudly instead of quietly testing one side twice.
    let (mut station_near, mut station_far) = (None, None);
    let (mut in_mesh, mut only_near, mut only_far) = (0u32, 0u32, 0u32);
    for i in -16i32..=16 {
        for k in -40i32..=40 {
            #[allow(clippy::cast_precision_loss)]
            let p = Vec3::new(
                frame.origin.x + i as f32 * 0.05,
                frame.origin.y + k as f32 * 0.05,
                frame.origin.z,
            );
            if !inside_object(&geometry, &pos, p) {
                continue;
            }
            in_mesh += 1;
            let (n, f) = (in_cell(near, p), in_cell(far, p));
            if n && !f {
                only_near += 1;
                if station_near.is_none() {
                    station_near = Some(p);
                }
            }
            if f && !n {
                only_far += 1;
                if station_far.is_none() {
                    station_far = Some(p);
                }
            }
        }
    }
    eprintln!(
        "door boundary grid: {in_mesh} grid point(s) inside the door mesh; {only_near} in 0x86020100 alone, \
         {only_far} in 0x86020101 alone"
    );
    // Assert the denominators: a grid that found two stations out of two candidates is not the
    // same measurement as one that found them out of two hundred, and a door moved off the
    // boundary would still produce *a* pair.
    assert!(
        in_mesh > 200 && only_near > 5 && only_far > 5,
        "{in_mesh} grid point(s) in the mesh, {only_near} in 0x86020100 alone and {only_far} in \
         0x86020101 alone: this door is meant to straddle the boundary with room on both sides"
    );
    let station_near = station_near.expect(
        "no point inside the door's mesh lies in cell 0x86020100 alone; this fixture's door is \
         meant to straddle the boundary",
    );
    let station_far = station_far.expect(
        "no point inside the door's mesh lies in cell 0x86020101 alone; this fixture's door is \
         meant to straddle the boundary -- a one-cell registration closes the door on this side",
    );
    eprintln!(
        "door stations: near {:?} in 0x86020100, far {:?} in 0x86020101, {:.3} m apart",
        station_near,
        station_far,
        (station_far.y - station_near.y).abs()
    );

    for (door_state, label) in RETAIL_DOOR_STATES {
        for (side, station, cell) in [("near", station_near, near), ("far", station_far, far)] {
            // ---- station 1: nobody in the doorway. The close is allowed. -------------------
            let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
            let door = register_door(
                &mut w,
                door_room.id,
                frame,
                Arc::clone(&geometry),
                door_state,
            );
            assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
            assert_eq!(
                w.set_ethereal(door, false, false),
                EtherealResult::Applied,
                "{label} / {side}: with no body anywhere the door must be able to close. If this \
                 refuses, the refusal below is not about the body and this test is measuring \
                 nothing"
            );
            assert!(!w.get(door).expect("live").state.is_ethereal());
            assert!(!w.get(door).expect("live").transient_state.check_ethereal());

            // ---- station 2: a body standing in the doorway. The close is deferred. ----------
            let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
            let door = register_door(
                &mut w,
                door_room.id,
                frame,
                Arc::clone(&geometry),
                door_state,
            );
            assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
            let h = spawn(&mut w, door_room.id, station, Vec3::ZERO);
            w.force_into_cell(h, &Position::new(cell, Frame::new(station, Quat::IDENTITY)));
            let before = w.ethereal_deferrals();
            assert_eq!(
                w.set_ethereal(door, false, false),
                EtherealResult::Deferred,
                "{label} / {side} station ({:.3}, {:.3}, {:.3}) in cell {:#010X}: the body is \
                 inside the door's mesh and the door closed on it",
                station.x,
                station.y,
                station.z,
                cell.0
            );
            assert!(
                w.get(door).expect("live").state.is_ethereal(),
                "{label} / {side}: collision refusal puts `ETHEREAL_PS` back; the door must stay \
                 walk-through while the close is pending"
            );
            assert!(
                w.get(door).expect("live").transient_state.check_ethereal(),
                "{label} / {side}: collision refusal raises `CHECK_ETHEREAL_TS`, the pending flag \
                 that the per-frame object update re-tests. Without it the door is stuck \
                 ethereal for ever"
            );
            assert_eq!(w.ethereal_deferrals(), before + 1);

            // ---- station 3: the body leaves, and the pending flag resolves. -----------------
            //
            // Nothing tells the door anything. The per-frame update's retry is the only thing
            // that can clear it, so this is an assertion about that retry and not the initial
            // `set_ethereal` call.
            let away = Position::new(
                door_room.id,
                Frame::new(
                    Vec3::new(frame.origin.x, frame.origin.y + 8.0, frame.origin.z),
                    Quat::IDENTITY,
                ),
            );
            w.force_into_cell(h, &away);
            if let Some(o) = w.get_mut(door) {
                o.transient_state.set_active_bit(true);
                o.update_time = 0.0;
            }
            let mut t = 10.0;
            let mut cleared = false;
            for _ in 0..4 {
                t += 0.05;
                assert!(
                    w.use_time(LocalTime(t), false),
                    "the sweep that must clear it never ran"
                );
                if !w.get(door).expect("live").transient_state.check_ethereal() {
                    cleared = true;
                    break;
                }
            }
            assert!(
                cleared,
                "{label} / {side}: the body left and the door is still pending. A \
                 `CHECK_ETHEREAL_TS` that never clears leaves the door permanently ethereal, \
                 which is the failure the per-frame retry exists to prevent"
            );
            assert!(
                !w.get(door).expect("live").state.is_ethereal(),
                "{label} / {side}: the flag cleared but the door is still ethereal, so the \
                 retry cleared the bit without doing the close"
            );
        }
    }
}

// =============================================================================================
// 7. The sphere arm takes the sorting sphere, never the path spheres
// =============================================================================================

/// The cross-cell calculation **never** hands its cell-list routine an object's path spheres.
///
/// The three arms, in their precedence, are:
///
/// | order | test | calculation | geometry passed |
/// |---|---|---|---|
/// | 1 | `HAS_PHYSICS_BSP_PS` | bounding-box cell list | part bounding **boxes** |
/// | 2 | collision-cylinder count is non-zero | multi-sphere cell list | every cylinder sphere, count clamped to **10** |
/// | 3 | otherwise | single-sphere cell list | the sorting sphere, **whatever its radius**, count **1** |
///
/// Arm 3 has **no radius test of any kind**: it passes the setup's sorting sphere directly. A
/// zero sphere reaches through no portal, so such an object occupies one cell. That is a property
/// of the shipped data, not a fallback to something else.
///
/// **The census, so that the claim has a denominator.** Over all **5,935** setup records in
/// `client_portal.dat`, every one of which decodes:
///
/// * **1,781** carry a zero sorting sphere;
/// * **678** carry at least one cylinder sphere (arm 2);
/// * **530** have a part with a physics BSP (arm 1);
/// * **166** carry at least one collision sphere **and** a zero sorting sphere;
/// * of those 166, **2** take arm 1 and **1** takes arm 2, leaving **163** that reach arm 3 with a
///   zero sorting sphere and a non-empty sphere list. Falling back to the path spheres would give
///   those 163 a **larger shadow set than the client's**.
///
/// The retail door is not one of them -- it has no collision sphere at all, so it cannot tell the
/// two readings apart; this test uses a synthetic object of the 163's shape instead.
#[test]
fn calc_cross_cells_takes_the_sorting_sphere_and_never_the_path_spheres() {
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let src =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the retail height table"));
    src.load_block_cells(dereth_primitives::LandblockId(TRAINING_DUNGEON));

    // ---- the census, from the shipped dat -------------------------------------------------
    let (mut total, mut zero_sorting, mut with_cyl, mut bsp_arm) = (0u32, 0u32, 0u32, 0u32);
    let (mut spheres_and_zero, mut reach_arm_three) = (0u32, 0u32);
    for id in store.ids_of(DbType::Setup) {
        let Ok(b) = store.read_typed(DbType::Setup, id) else {
            continue;
        };
        let Ok(d) = Setup::decode_payload(id, &b) else {
            continue;
        };
        total += 1;
        let zero = d.sorting_sphere.radius <= 0.0;
        let has_bsp = d.parts.iter().any(|p| {
            store
                .read_typed(DbType::GfxObj, *p)
                .ok()
                .and_then(|g| dereth_assets::GfxObj::decode_payload(*p, &g).ok())
                .is_some_and(|g| g.physics_bsp.is_some())
        });
        if zero {
            zero_sorting += 1;
        }
        if !d.cylspheres.is_empty() {
            with_cyl += 1;
        }
        if has_bsp {
            bsp_arm += 1;
        }
        if zero && !d.spheres.is_empty() {
            spheres_and_zero += 1;
            if !has_bsp && d.cylspheres.is_empty() {
                reach_arm_three += 1;
            }
        }
    }
    eprintln!(
        "cross-cell census: {total} setups; {zero_sorting} zero sorting sphere; {with_cyl} cylsphere; \
         {bsp_arm} part BSP; {spheres_and_zero} collision spheres+zero sorting, {reach_arm_three} of them \
         on arm 3"
    );
    // Literals, not ratios: a decoder change that stopped reading setup path spheres would take
    // `spheres_and_zero` to 0 and every other assertion in this file would stay green.
    assert_eq!(
        total, 5935,
        "the setup denominator moved; every count below is relative to it"
    );
    assert_eq!(spheres_and_zero, 166);
    assert_eq!(reach_arm_three, 163);
    assert_eq!(bsp_arm, 530);
    assert_eq!(with_cyl, 678);

    // ---- the behaviour, on an object of exactly that shape ---------------------------------
    //
    // Two path spheres 6 m apart on the y axis, straddling the `0x86020100`/`0x86020101`
    // boundary, and a zero sorting sphere. The client registers it in ONE cell; falling back to
    // the path spheres would register it in two.
    let door_room = cells_of(&store).expect("the block's cells");
    let geom = LandSource::env_cell(src.as_ref(), door_room).expect("cell geometry");
    let at = a_standable_point(&geom).expect("a standable point");
    let frame = Frame::new(Vec3::new(at.x, at.y, at.z - 0.5), Quat::IDENTITY);

    let shadows = |g: SetupGeometry| -> Vec<CellId> {
        let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
        let h = w.create(ObjectId(9), Arc::new(g), true);
        w.enter_cell(h, door_room);
        if let Some(o) = w.get_mut(h) {
            o.set_frame(frame);
            o.position = Position::new(door_room, frame);
        }
        w.calc_cross_cells(h, false);
        w.get(h)
            .expect("live")
            .shadow_objects
            .iter()
            .map(|s| s.cell_id)
            .collect()
    };

    let reaching = SetupGeometry {
        spheres: vec![
            Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5),
            Sphere::new(Vec3::new(0.0, -6.0, 0.5), 0.5),
        ],
        sorting_sphere: Sphere::new(Vec3::ZERO, 0.0),
        ..SetupGeometry::default()
    };
    let got = shadows(reaching.clone());
    assert_eq!(
        got,
        vec![door_room],
        "shadow set {got:?}. This object's *path* spheres reach 6 m into the next cell and its \
         sorting sphere is zero; `calc_cross_cells` passes that sorting sphere and \
         nothing else, so it occupies one cell. A fallback to the path spheres gives it \
         two -- a larger shadow set than the client's, on 163 shipped setups"
    );

    // The calibration uses the same object with a real sorting sphere, which MUST span both cells,
    // so the answer above is a measurement and not a dead probe.
    let sorted = SetupGeometry {
        sorting_sphere: Sphere::new(Vec3::new(0.0, -3.0, 0.5), 4.0),
        ..reaching
    };
    let got = shadows(sorted);
    assert!(
        got.len() > 1 && got.contains(&door_room),
        "shadow set {got:?}: a 4 m sorting sphere centred 3 m into the doorway must reach the \
         next cell. If this is one cell the probe above is measuring nothing"
    );
}

/// The cell `calc_cross_cells_takes_the_sorting_sphere_and_never_the_path_spheres` works in --
/// the same room the rest of this file uses.
fn cells_of(store: &RetailDatStore) -> Option<CellId> {
    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(store, TRAINING_DUNGEON);
    cells.iter().find(|c| c.id.0 == 0x8602_0100).map(|c| c.id)
}

/// **The discriminating cases for the bounding-box arm**, which the retail door cannot exercise.
/// Its box straddles the one portal it reaches, so the box/plane intersection answers `CROSSING`
/// and the `portal_side` comparison never decides anything. Both of its box-bearing parts carry a
/// physics BSP, so the physics-sphere-or-drawing-sphere fallback is never needed. Its box reaches
/// exactly one cell away, so the growing-array walk never needs a second round.
///
/// Both objects below are synthetic, and both are built out of the **retail** door's own physics
/// tree so that scanning their parts really does raise `HAS_PHYSICS_BSP_PS` and the object really
/// does take arm 1.
///
/// The training academy's rooms are a 10 m grid: `0x86020100` is `x 145..155, y -235..-225`,
/// `0x86020101` is the room through its `y = -235` portal, and `0x86020103` is one further on,
/// reachable only through `0x86020101` -- `0x86020100`'s portal list is `0x86020163`,
/// `0x86020101`, `0x86020102` and does not contain it.
#[test]
fn the_bbox_arm_reads_the_portal_side_the_drawing_sphere_and_the_second_round() {
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let src =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the retail height table"));
    src.load_block_cells(dereth_primitives::LandblockId(TRAINING_DUNGEON));
    let decoded = setup(&store, DataId(DOOR));
    let mut stats = SetupPartStats::default();
    let door_geometry = setup_geometry_with_parts(&store, &decoded, &mut stats);
    let tree = door_geometry.parts[0]
        .physics_bsp
        .clone()
        .expect("the door's part 0 tree");

    let start = CellId(0x8602_0100);
    let next = CellId(0x8602_0101);
    let beyond = CellId(0x8602_0103);
    let geom = LandSource::env_cell(src.as_ref(), start).expect("cell 0x86020100");
    assert!(
        !geom.portals.iter().any(|p| p.other_cell_id == beyond.0),
        "0x86020100 has a direct portal to 0x86020103, so reaching it says nothing about the \
         second round of the expansion"
    );

    // The object stands just inside `0x86020100`, half a metre short of the `y = -235` portal.
    let origin = Vec3::new(150.0, -234.5, -11.5);
    let frame = Frame::new(origin, Quat::IDENTITY);

    // Part 0 anchors the object on arm 1 and nothing else: a box 0.2 m across, entirely on this
    // cell's side of every portal, so every cell added below is added by part 1.
    let anchor = dereth_physics::source::SetupPart {
        placement_frame: Frame::new(Vec3::ZERO, Quat::IDENTITY),
        default_scale: Vec3::new(1.0, 1.0, 1.0),
        physics_bsp: Some(Arc::clone(&tree)),
        bound_box: Some(dereth_physics::geom::BBox::new(
            Vec3::new(-0.2, -0.2, -0.2),
            Vec3::new(0.2, 0.2, 0.2),
        )),
        drawing_sphere: None,
        // Not a billboard: the billboard test needs degrade information, while this interior
        // bounding-box arm never asks for it.
        first_degrade_mode: None,
    };
    // Part 1 has **no physics BSP**, so its only bounding sphere is the drawing one. Twelve
    // metres, which is what lets the prescreen pass at a portal ten metres away.
    let reacher = |lo: Vec3, hi: Vec3| dereth_physics::source::SetupPart {
        placement_frame: Frame::new(Vec3::ZERO, Quat::IDENTITY),
        default_scale: Vec3::new(1.0, 1.0, 1.0),
        physics_bsp: None,
        bound_box: Some(dereth_physics::geom::BBox::new(lo, hi)),
        drawing_sphere: Some(Sphere::new(Vec3::ZERO, 12.0)),
        first_degrade_mode: None,
    };

    let shadows = |parts: Vec<dereth_physics::source::SetupPart>| -> Vec<CellId> {
        let g = SetupGeometry {
            parts,
            ..SetupGeometry::default()
        };
        assert!(
            g.caches_physics_bsp(),
            "the object must take arm 1 or this test is about arm 3"
        );
        let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
        let h = w.create(ObjectId(11), Arc::new(g), false);
        w.enter_cell(h, start);
        if let Some(o) = w.get_mut(h) {
            o.set_frame(frame);
            o.position = Position::new(start, frame);
        }
        assert!(w.get(h).expect("live").state.has_physics_bsp());
        w.calc_cross_cells(h, false);
        w.get(h)
            .expect("live")
            .shadow_objects
            .iter()
            .map(|s| s.cell_id)
            .collect()
    };

    // ---- A. a box that is ENTIRELY past the portal plane, never crossing it ------------------
    //
    // World `x 149..151, y -237..-236, z -11.5..-10.5`: a metre inside `0x86020101` with a metre
    // of clear air behind it, so the box/plane intersection answers a definite `NEGATIVE` rather
    // than `CROSSING`. The other cell is added when that answer differs from `portal_side`, and
    // this portal's side is the client's `POSITIVE` -- so the comparison, and only the comparison,
    // is what puts `0x86020101` in the set.
    let a = shadows(vec![
        anchor.clone(),
        reacher(Vec3::new(-1.0, -2.5, 0.0), Vec3::new(1.0, -1.5, 1.0)),
    ]);
    assert_eq!(
        a,
        vec![start, next],
        "shadow set {a:?}. A box lying wholly beyond the `y = -235` portal must carry the object \
         into 0x86020101: the box/plane intersection is NEGATIVE, the portal's side is POSITIVE, \
         and differing sides add the next cell. It is also the only part with a sphere, so if the \
         `physics_sphere ?: drawing_sphere` fallback is gone the prescreen never runs"
    );

    // The calibration for the same object pulls the box back to this side of the plane, where the
    // cell must NOT be added. Without this, "the box reached" and "the arm adds every neighbour"
    // read alike.
    let b = shadows(vec![
        anchor.clone(),
        reacher(Vec3::new(-1.0, -0.4, 0.0), Vec3::new(1.0, -0.1, 1.0)),
    ]);
    assert_eq!(
        b,
        vec![start],
        "shadow set {b:?}: a box entirely on this side of the portal plane must add nothing"
    );

    // ---- B. a box that needs the SECOND round of the growing array ---------------------------
    //
    // World `x 150..159, y -242.5..-235.5`: through `0x86020101` and on into `0x86020103`, which
    // `0x86020100` has no portal to. The bounding-box calculation walks its cell array while it
    // grows, so `0x86020101` is added in round 1 and its own transit-cell pass in round 2 is the
    // only thing that can reach `0x86020103`.
    let c = shadows(vec![
        anchor,
        reacher(Vec3::new(0.0, -8.0, 0.0), Vec3::new(9.0, -1.0, 1.0)),
    ]);
    assert!(
        c.contains(&next),
        "shadow set {c:?}: round 1 must reach 0x86020101"
    );
    assert!(
        c.contains(&beyond),
        "shadow set {c:?}: 0x86020103 is two portals away and `0x86020100` has no portal to it, \
         so it can only arrive on the second round of `find_bbox_cell_list`'s walk over an array \
         that grows while it is being iterated. A loop that stops after the object's own cell \
         gets everything else right and loses this"
    );
}

// =============================================================================================
// The state change that opens a door reaches its collision body
// =============================================================================================

mod set_state {
    //! An encoded `0xF74B Item_SetState` message changes a live door's collision state: the object
    //! stream replaces the whole state word of an existing body (behind the state timestamp gate),
    //! and physics synchronization applies it, so a door given the open word lets a body into its
    //! geometry where the same door left closed stops it. The state words are the two a door is
    //! created and opened with in the recorded corpus, written here as literal masks.
    //!
    //! Fixture: retail door setup `0x0200024F` and the training academy's rooms from the retail dats
    //! (missing dats fail), synthetic encoded create and state bodies fed through `ObjectStream`, and a
    //! constructed walking body driven through the shared collision-walk probe. No capture replay.
    use crate::common::collision_probe;

    use std::sync::Arc;

    use dereth_assets::{Decode, Setup};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_client_runtime::objects::ObjectStream;
    use dereth_dat::{DbType, RetailDatStore};
    use dereth_physics::{LandSource, PhysHandle, PhysicsWorld, V3};
    use dereth_primitives::{
        CellId, DataId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3,
    };
    use dereth_protocol::objects::ItemSetState;
    use dereth_world_data::land_source::DatLandSource;
    use {
        dereth_world_data::setup::setup_geometry_with_parts,
        dereth_world_data::setup::SetupPartStats,
    };

    /// The shared collision-walk probe, also used by `door_opens_ethereal`.
    use collision_probe::{came_to_rest, frames_inside, inside_object};

    /// The walk fixtures this module shares with the rest of the file.
    use super::{a_standable_point, approaches, player_geometry, Approach};

    // =============================================================================================
    // The masks and indices, as literals
    // =============================================================================================

    /// Ethereal state mask 4 bypasses object collision. This literal is the sole bit toggled by
    /// the two state words below; it is intentionally independent of the production enum constant.
    const ETHEREAL_PS: u32 = 0x0000_0004;

    /// The retail door setup the other door tests use. The test requires a cached part physics BSP and
    /// zero spheres. It does not check cylinders, so this predicate alone is not a BSP-only proof.
    const DOOR_SETUP: u32 = 0x0200_024F;

    /// The training academy, the block the other door and collision tests use.
    const TRAINING_DUNGEON: u16 = 0x8602;

    fn store() -> RetailDatStore {
        dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the geometry oracle is the retail dats under {} (set DERETH_TEST_DAT_DIR)",
                dereth_dat::testing::dat_dir().display()
            )
        })
    }

    // =============================================================================================
    // The messages the walk below is driven with
    // =============================================================================================

    /// Encode a synthetic create body with setup, full position and explicit scale 1. These provide
    /// placement information for the object-stream/physics seam; there is no transport replay here.
    fn door_create_body(id: u32, state: u32, setup: u32, cell: CellId, frame: Frame) -> Vec<u8> {
        use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
        dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id: ObjectId(id),
                objdesc: ObjDesc::default(),
                physicsdesc: PhysicsDesc {
                    bitfield: flags::SETUP | flags::POSITION | flags::OBJSCALE,
                    state,
                    setup_id: Some(setup),
                    object_scale: Some(1.0),
                    position: Some(dereth_protocol::types::PositionWire {
                        objcell_id: cell.0,
                        frame: dereth_protocol::types::Frame {
                            origin: frame.origin.into(),
                            orientation: frame.rotation.into(),
                        },
                    }),
                    ..PhysicsDesc::default()
                },
                wdesc: PublicWeenieDesc::default(),
            },
        ))
        .expect("encode")
    }

    fn set_state_body(id: u32, state: u32, instance: u16, event: u16) -> Vec<u8> {
        dereth_protocol::write_body(&ItemSetState {
            id: ObjectId(id),
            state,
            timestamps: dereth_protocol::types::PhysicsEventStamp { instance, event },
        })
        .expect("encode")
    }

    fn feed(s: &mut ObjectStream, opcode: dereth_protocol::Opcode, body: Vec<u8>) {
        s.apply_event(&SessionEvent::WorldObject { opcode, body }, LocalTime(0.0));
    }

    fn create(s: &mut ObjectStream, body: Vec<u8>) {
        feed(s, dereth_protocol::Opcode::ITEM_CREATE_OBJECT, body);
    }

    fn set_state(s: &mut ObjectStream, body: Vec<u8>) {
        feed(s, dereth_protocol::Opcode::ITEM_SET_STATE, body);
    }
    // =============================================================================================
    // 5. The collision differential
    // =============================================================================================

    fn walker(w: &mut PhysicsWorld, cell: CellId, at: Vec3, per_substep: Vec3) -> PhysHandle {
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

    /// Sample z after each 30 Hz physics update. Find the minimum over the entire path, then the
    /// first sample within 0.01 m of that floor and the peak from that sample onward. Returned rise
    /// is that later peak minus the floor, not an unconditional maximum over the whole path.
    ///
    /// A horizontal-only projection cannot distinguish climbing over the door from walking through
    /// it; projected distance alone does not identify whether the body entered the door's collision
    /// geometry.
    fn run_tracking_z(w: &mut PhysicsWorld, h: PhysHandle, seconds: f64) -> (Vec3, f32, Vec<Vec3>) {
        let dt = 1.0 / 30.0;
        let mut t = 0.0;
        let mut path: Vec<Vec3> = Vec::new();
        let mut end = Vec3::ZERO;
        while t < seconds {
            t += dt;
            w.use_time(LocalTime(t), false);
            end = w.get(h).expect("live").position.frame.origin;
            path.push(end);
        }
        let zs: Vec<f32> = path.iter().map(|p| p.z).collect();
        let floor = zs.iter().copied().fold(f32::MAX, f32::min);
        // `<= floor + 0.01` and not `== floor`: a body that climbs something and comes back off it
        // settles a ten-thousandth of a metre lower than it started, and an exact match then picks a
        // frame **after** the climb and reports no rise at all.
        let landed = zs.iter().position(|z| *z <= floor + 0.01).unwrap_or(0);
        let peak = zs[landed..].iter().copied().fold(f32::MIN, f32::max);
        // The whole path is kept because two of the questions below -- did the control ever get
        // inside the door's mesh, and did the control actually stop -- are not visible in the final
        // position alone. A body that walks *through* a
        // door ends outside its mesh exactly as a body stopped by it does, and a body that slides
        // past one is still moving when the clock stops. See [`came_to_rest`].
        (end, peak - floor, path)
    }

    /// Behaviour: object.set-state.a-change-reaches-a-live-doors-collision
    /// Find one paired approach where a synthetic encoded state change admits the body into the
    /// door geometry. Both trials create the same door/room/walker; only the open trial then feeds
    /// ItemSetState with instance 0/event 1 through ObjectStream before physics synchronization.
    /// The open word 0x0001001C is the one the recorded corpus carries. The create/state bodies are
    /// encoded here and decoded/applied through the object-stream seam; no direct physics-state
    /// setter, network transport or session replay supplies the change. Removing the state
    /// application from physics synchronization turns this test red.
    ///
    /// Select the first pair whose closed path advances, finishes outside, satisfies the rest filter
    /// and has rise <= 0.05, while the open path ends inside. Require zero closed inside samples and
    /// more open inside samples. Open-path rest is not asserted, and unselected approaches/cells
    /// are not universal collision coverage. `door_opens_ethereal` exercises the ethereal animation
    /// route.
    #[test]
    fn a_recorded_set_state_word_changes_a_live_doors_collision_answer() {
        const DOOR_ID: u32 = 0x7000_0009;
        /// HAS_PHYSICS_BSP_PS | IGNORE_COLLISIONS_PS | REPORT_COLLISIONS_PS: the word every recorded
        /// create of this setup carries.
        const CLOSED: u32 = 0x0001_0018;
        /// Same word plus ETHEREAL_PS: the recorded open word.
        const OPEN: u32 = 0x0001_001C;

        assert_eq!(
            OPEN ^ CLOSED,
            ETHEREAL_PS,
            "the two corpus words differ in exactly ETHEREAL_PS"
        );

        let store = Arc::new(store());
        let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
        let src = Arc::new(
            DatLandSource::new(Arc::clone(&store), &region).expect("the retail height table"),
        );
        src.load_block_cells(LandblockId(TRAINING_DUNGEON));

        let decoded = {
            let did = DataId(DOOR_SETUP);
            let bytes = store
                .read_typed(DbType::Setup, did)
                .expect("the door setup");
            Setup::decode_payload(did, &bytes).expect("decodes")
        };
        let mut stats = SetupPartStats::default();
        let geometry = Arc::new(setup_geometry_with_parts(&store, &decoded, &mut stats));
        assert!(
            geometry.spheres.is_empty() && geometry.caches_physics_bsp(),
            "the door must have no spheres and must cache a physics BSP; cylinder geometry is not checked here"
        );

        let mut loader = dereth_world_data::env_cells::EnvCellLoader::new();
        let cells = loader.load_block(&store, TRAINING_DUNGEON);
        assert!(!cells.is_empty(), "the training academy's interior cells");

        // One synthetic trial: create the door through the object stream at `frame`, then optionally feed
        // `0xF74B`, then walk a body at it.
        let trial =
            |open: bool, cell: CellId, frame: Frame, a: &Approach| -> (Vec3, f32, Vec<Vec3>) {
                let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
                let mut s = ObjectStream::new();
                create(
                    &mut s,
                    door_create_body(DOOR_ID, CLOSED, DOOR_SETUP, cell, frame),
                );
                s.sync_physics(&store, &mut w);
                let h = s
                    .physics
                    .handle(ObjectId(DOOR_ID))
                    .expect("the door got a body");
                assert!(
                    w.get(h).expect("live").state().has_physics_bsp(),
                    "the door must keep the BSP arm or the control walks through a closed door"
                );
                if open {
                    set_state(&mut s, set_state_body(DOOR_ID, OPEN, 0, 1));
                    s.sync_physics(&store, &mut w);
                }
                assert_eq!(
                    w.get(h).expect("live").state().0,
                    if open { OPEN } else { CLOSED },
                    "the body's state word must equal the encoded message word"
                );
                let body = walker(&mut w, cell, a.start, a.step);
                run_tracking_z(&mut w, body, 4.0)
            };

        let mut compared = 0usize;
        let mut climbed_away = 0usize;
        let mut best = String::new();
        'cells: for d in &cells {
            let Some(geom) = LandSource::env_cell(src.as_ref(), d.id) else {
                continue;
            };
            let Some(at) = a_standable_point(&geom) else {
                continue;
            };
            let frame = Frame::new(Vec3::new(at.x, at.y, at.z - 0.5), Quat::IDENTITY);
            let pos = Position::new(d.id, frame);
            for a in approaches(&geom, frame.origin, at.z) {
                if inside_object(&geometry, &pos, a.start) {
                    continue;
                }
                // How far the body got **along the approach**, which is the axis the door blocks. A
                // straight-line displacement would mislead: a body deflected sideways by a closed door
                // is displaced further than one that walked into the doorway.
                let dir = a.step.normalize();
                let advance = |p: Vec3| p.sub(a.start).dot(dir);

                // The CLOSED run is the control and it decides whether this approach is a walk into
                // *this* door: it must travel, and it must be stopped short of the mesh.
                let (closed, closed_rise, closed_path) = trial(false, d.id, frame, &a);
                let closed_travel = advance(closed);
                // The third clause is the one that makes this a control. "Travelled, and did not end
                // inside the mesh" is satisfied by a body that walked
                // straight past the door -- see [`came_to_rest`]. A closed door is a control for
                // "the closed door stopped it" only if the body it stopped actually **stopped**.
                if closed_travel <= 0.5
                    || inside_object(&geometry, &pos, closed)
                    || !came_to_rest(&closed_path)
                {
                    continue;
                }
                // A control that climbed over the door's own geometry is not a control for "the
                // closed door stopped it".
                //
                // This is a skip, not an assertion: in retail a body walks up onto chairs and tables
                // (stairs are built that way), so climbing is valid behaviour. What a climbing control
                // disqualifies is **this approach**: a control that went over the mesh does not
                // isolate its blocking effect for the sampled-intersection comparison. Try the next
                // approach.
                //
                // This cannot become a silence. `compared` must reach exactly 1 at the end of the
                // loop, so an approach set in which *every* control climbs fails the test instead of
                // passing it vacuously, and the count of skipped approaches is printed either way.
                if closed_rise > 0.05 {
                    climbed_away += 1;
                    continue;
                }
                let (opened, open_rise, open_path) = trial(true, d.id, frame, &a);
                if !inside_object(&geometry, &pos, opened) {
                    // The open trial did not finish inside the door geometry. That result is not
                    // the pair this judge selects; try another approach without assigning a cause.
                    continue;
                }
                let open_travel = advance(opened);
                let closed_in_mesh = frames_inside(&geometry, &pos, &closed_path);
                let open_in_mesh = frames_inside(&geometry, &pos, &open_path);
                // Require no sampled intersection with the closed door, not merely an outside end.
                // It is asserted rather than filtered: the guard above only looks at
                // where the control ended, so a control that passed *through* the door and out the
                // far side would satisfy it.
                assert_eq!(
                    closed_in_mesh,
                    0,
                    "cell {:#010X}: the CLOSED door was penetrated on {closed_in_mesh} of {} frames, \
                     so the control walked through it",
                    d.id.0,
                    closed_path.len(),
                );
                // Judge sampled geometry intersection rather than displacement. Advance along the
                // approach still misreads a step-up: in cell 0x86020105 the closed control can advance
                // past the door without entering it while the open body enters the mesh, climbs its
                // 0.409 m threshold and stops there.
                //
                // The open-end guard plus compared==1 requires one accepted pair, while this direct
                // sample-count comparison distinguishes the two runs. The open path's rest is
                // not asserted here; only the closed path is passed to came_to_rest.
                assert!(
                    open_in_mesh > closed_in_mesh,
                    "cell {:#010X}: the door given the encoded open-state message did not admit more inside samples -- it was \
                     inside the door's own mesh on {open_in_mesh} of {} frames against \
                     {closed_in_mesh} closed ({open_travel:.2} m along the approach against \
                     {closed_travel:.2} m; the open body rose {open_rise:.3} m and the closed one \
                     {closed_rise:.3} m, so the closed one {})",
                    d.id.0,
                    open_path.len(),
                    if closed_rise > 0.05 { "CLIMBED the door's geometry" } else { "stayed at floor level" },
                );
                best = format!(
                    "cell {:#010X}: closed {closed_travel:.2} m along the approach and stopped short \
                     of the mesh without ever being inside it ({closed_in_mesh} of {} frames), \
                     rising {closed_rise:.3} m; after the 0xF74B, {open_travel:.2} m, rising \
                     {open_rise:.3} m, inside the door's own geometry on {open_in_mesh} frames and \
                     ending there",
                    d.id.0,
                    closed_path.len(),
                );
                compared += 1;
                break 'cells;
            }
        }
        println!(
            "set-state walk: {best} ({climbed_away} approach(es) skipped because the CLOSED control \
    climbed rather than being stopped -- retail behaviour, but not a control for this differential)"
        );
        assert_eq!(
            compared, 1,
            "no approach in the training academy produced a control that walked into a closed door, \
             so the differential observed nothing and must not be reported as a pass \
             ({climbed_away} of the approaches were skipped because their control climbed the \
             geometry instead of being stopped by it)"
        );
    }
}
