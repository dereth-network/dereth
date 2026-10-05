use std::sync::Arc;

use dereth_animation::data::AnimAssets;
use dereth_animation::motion::{InterpretedMotionState, MoveToRequest, MovementParameters};
use dereth_animation::{MotionCommand, MotionDriver};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_physics::math::V3 as _;
use dereth_physics::pmanager::FALLBACK_SPEED;
use dereth_primitives::{LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
use dereth_protocol::Message;
use dereth_testkit::HeadlessClient;
use dereth_world_data::anim_assets::DatAnimAssets;
use {
    dereth_client_runtime::character::Character,
    dereth_client_runtime::character::ALUVIAN_MALE_MOTION_TABLE,
    dereth_client_runtime::character::ALUVIAN_MALE_SCALE,
    dereth_client_runtime::character::ALUVIAN_MALE_SETUP,
};

use super::support::{body_origin, create_event, fresh_local_body, settled_at, store, DT, PLAYER};

/// The remote body every correction scenario below moves.
const VICTIM: ObjectId = ObjectId(0x8000_114D);
/// An id the client never sees, so a move-to issued at it records a target and starts no
/// motion of its own -- which is the state "this body is on its way somewhere" means, with
/// nothing else touching the body.
const NEVER_SEEN: ObjectId = ObjectId(0x7000_0BAD);

/// An ordinary creature's run rate, and the two factors the walk speed is built from. They
/// are the client's own numbers, written as independent literals because
/// reading them back through the constant would not notice a wrong constant.
const RUN_RATE: f32 = 1.5;
const RUN_ANIM_SPEED: f32 = 4.0;
const INTERP_DOUBLE: f32 = 2.0;
/// The rate a creature the shard has said nothing about moves at.
const DEFAULT_RUN_RATE: f32 = 1.0;

fn walk_speed(rate: f32) -> f32 {
    rate * RUN_ANIM_SPEED * INTERP_DOUBLE
}

/// A yaw-only rotation, the only one a standing body ever carries.
fn yaw(degrees: f32) -> Quat {
    let half = degrees.to_radians() * 0.5;
    Quat {
        w: dereth_primitives::num::math::cosf(half),
        x: 0.0,
        y: 0.0,
        z: dereth_primitives::num::math::sinf(half),
    }
}

fn body_heading(c: &Character, id: ObjectId) -> f32 {
    let h = c.world.by_object_id(id).expect("the body exists");
    dereth_physics::math::get_heading(&c.world.get(h).expect("the body exists").position.frame)
}

/// One position correction carrying a **real** facing: only two components are declared
/// absent, so the heading under test really crosses the wire.
///
/// `support::position_event` declares the whole orientation absent, which is right for the
/// fifteen scenarios above and wrong for the two here whose subject *is* the facing.
fn correction_with_facing(id: ObjectId, at: &Position, stamp: u16) -> SessionEvent {
    let f = position_flags::ORIENTATION_HAS_NO_X
        | position_flags::ORIENTATION_HAS_NO_Y
        | position_flags::IS_GROUNDED;
    let msg = MovementPositionEvent {
        id,
        position: PositionPack {
            flags: f,
            origin: dereth_protocol::types::Origin {
                objcell_id: at.cell.raw(),
                origin: dereth_protocol::types::Vec3 {
                    x: at.frame.origin.x,
                    y: at.frame.origin.y,
                    z: at.frame.origin.z,
                },
            },
            orientation: dereth_protocol::types::Quat {
                w: at.frame.rotation.w,
                x: 0.0,
                y: 0.0,
                z: at.frame.rotation.z,
            },
            instance_timestamp: 0,
            position_timestamp: stamp,
            teleport_timestamp: 0,
            ..PositionPack::default()
        },
    };
    let body = dereth_protocol::write_body(&msg).expect("the update encodes");
    let decoded = MovementPositionEvent::read(&mut dereth_protocol::Reader::new(&body))
        .expect("the encoded update round trips");
    assert!(
        decoded.position.has_contact(),
        "the contact flag did not encode"
    );
    assert!(
        (decoded.position.orientation.z - at.frame.rotation.z).abs() < 1e-6,
        "the wire facing did not encode"
    );
    SessionEvent::WorldObject {
        opcode: MovementPositionEvent::OPCODE,
        body,
    }
}

/// A real motion driver on a remote body: a part array, the shipped animation table, and
/// whichever of the two roads a rate can reach it by.
///
/// `weenie_rate` is the one the body's own record carries -- which only the player's own body
/// ever has. `interpreted_run` is the one an ordinary movement message carries for a creature
/// the shard has running. `moving_to` issues a move-to at an object this client has never
/// seen, which records the target and starts no motion.
fn driver_on(
    c: &mut Character,
    store: &Arc<RetailDatStore>,
    id: ObjectId,
    weenie_rate: Option<f32>,
    interpreted_run: Option<f32>,
    moving_to: bool,
) {
    let assets = Arc::new(DatAnimAssets::new(Arc::clone(store)));
    let setup = assets
        .setup(ALUVIAN_MALE_SETUP)
        .expect("the shipped setup decodes");
    let mut driver = MotionDriver::new(Arc::clone(&assets) as Arc<dyn AnimAssets>);
    assert!(driver.set_setup(setup), "the part array is built");
    assert!(
        driver.set_motion_table(ALUVIAN_MALE_MOTION_TABLE),
        "the animation table loads"
    );
    driver.scale = ALUVIAN_MALE_SCALE;
    driver.env.run_rate = weenie_rate;
    if let Some(rate) = interpreted_run {
        let state = InterpretedMotionState {
            current_style: MotionCommand::NON_COMBAT,
            forward_command: MotionCommand::RUN_FORWARD,
            forward_speed: rate,
            ..InterpretedMotionState::default()
        };
        driver.unpack_interpreted_movement(MotionCommand::NON_COMBAT, &state, false);
    }
    if moving_to {
        driver.with_movement(|m, ctx| {
            m.perform_movement(
                &MoveToRequest::MoveToObject {
                    object_id: NEVER_SEEN,
                    top_level_id: NEVER_SEEN,
                    radius: 0.5,
                    height: 1.0,
                },
                &MovementParameters::default(),
                ctx,
            );
        });
        assert!(
            driver.movement.is_moving_to(),
            "the body must be on its way somewhere"
        );
    } else {
        assert!(
            !driver.movement.is_moving_to(),
            "the control body goes nowhere of its own"
        );
    }
    let h = c.world.by_object_id(id).expect("the body exists");
    c.world
        .get_mut(h)
        .expect("the body exists")
        .set_motion(Box::new(driver));
}

/// The scene every correction scenario runs in: a remote body standing on real ground, a
/// destination three metres away, and the local body parked out of the way.
struct Scene {
    c: Character,
    stream: ObjectStream,
    t: f64,
    start: Position,
    destination: Position,
}

fn scene(
    store: &Arc<RetailDatStore>,
    weenie_rate: Option<f32>,
    interpreted_run: Option<f32>,
    moving_to: bool,
    start_yaw: f32,
    wire_yaw: f32,
) -> Scene {
    let mut c = fresh_local_body(store);
    let mut t = 3.0;
    let mut start = settled_at(&mut c, 96.0, 96.0, &mut t);
    let mut destination = settled_at(&mut c, 99.0, 96.0, &mut t);
    let _ = settled_at(&mut c, 96.0, 84.0, &mut t);
    start.frame.rotation = yaw(start_yaw);
    destination.frame.rotation = yaw(wire_yaw);

    let mut stream = ObjectStream::new();
    stream.apply_event(
        &create_event(VICTIM, Some(start), PLAYER, "Victim"),
        LocalTime(t),
    );
    stream.sync_physics(store, &mut c.world);
    let gap = body_origin(&c, VICTIM)
        .expect("the body is in a cell")
        .sub(start.frame.origin)
        .mag2()
        .sqrt();
    assert!(gap < 1e-4, "the create was deflected by {gap:.4} m");
    driver_on(
        &mut c,
        store,
        VICTIM,
        weenie_rate,
        interpreted_run,
        moving_to,
    );

    // A body's distance to the player is unknown until it has been stepped once.
    for _ in 0..4 {
        t += DT;
        c.update(LocalTime(t));
    }
    Scene {
        c,
        stream,
        t,
        start,
        destination,
    }
}

/// What one sub-step of a correction did.
struct SubStep {
    moved: f32,
    cached_speed: f32,
    max_speed: Option<f32>,
}

fn one_substep(s: &mut Scene, store: &Arc<RetailDatStore>) -> SubStep {
    let before = body_origin(&s.c, VICTIM).expect("the body is in a cell");
    s.stream.apply_event(
        &correction_with_facing(VICTIM, &s.destination, 1),
        LocalTime(s.t),
    );
    s.stream.sync_physics(store, &mut s.c.world);
    s.t += DT;
    s.c.update(LocalTime(s.t));
    let after = body_origin(&s.c, VICTIM).expect("the body is in a cell");
    let h = s.c.world.by_object_id(VICTIM).expect("the body exists");
    SubStep {
        moved: after.sub(before).mag2().sqrt(),
        cached_speed: s
            .c
            .world
            .get(h)
            .expect("the body exists")
            .velocity()
            .mag2()
            .sqrt(),
        max_speed: s
            .c
            .world
            .interpolation(h)
            .expect("a manager was made")
            .max_speed,
    }
}

// ---------------------------------------------------------------------------------------
// movement.correction.a-body-walks-a-correction-at-its-own-speed-and-not-a-default
// ---------------------------------------------------------------------------------------

/// **A body walks a correction at its own speed.** One sub-step covers the body's own
/// quantum and not the client's fallback, and the whole walk is finished in the number of
/// sub-steps that speed implies -- which is far fewer than the fallback would need.
pub fn a_correction_runs_at_the_bodys_own_speed() {
    let store = store();
    let native = walk_speed(RUN_RATE);
    #[allow(clippy::cast_possible_truncation)]
    let native_step = native * DT as f32;
    #[allow(clippy::cast_possible_truncation)]
    let fallback_step = FALLBACK_SPEED * DT as f32;

    // One sub-step.
    let mut s = scene(&store, Some(RUN_RATE), None, false, 0.0, 0.0);
    let step = one_substep(&mut s, &store);
    let h = s.c.world.by_object_id(VICTIM).expect("the body exists");
    let still_walking = s.c.world.is_interpolating(h);
    // The sweep achieves a little under the quantum it is handed, because it asks for
    // `speed * quantum` along the straight line and walks the body over real ground: every
    // such walk lands a few per cent short for that reason. The bound is therefore relative,
    // and the discriminator is that it is nowhere near the fallback's quantum.
    let ratio = step.moved / native_step;
    let quantum_is_the_bodys_own = (0.90..=1.001).contains(&ratio)
        && step.moved > fallback_step * 1.3
        && step.max_speed == Some(RUN_RATE * RUN_ANIM_SPEED)
        && (step.cached_speed - native).abs() < native * 0.1
        && step.cached_speed <= dereth_physics::globals::MAX_VELOCITY
        && still_walking;

    // And the whole walk.
    let mut s = scene(&store, Some(RUN_RATE), None, false, 0.0, 0.0);
    let before = body_origin(&s.c, VICTIM).expect("the body is in a cell");
    let distance = before.sub(s.destination.frame.origin).mag2().sqrt();
    assert!(
        distance > 2.0,
        "the walk under test is only {distance:.3} m long"
    );
    s.stream.apply_event(
        &correction_with_facing(VICTIM, &s.destination, 1),
        LocalTime(s.t),
    );
    s.stream.sync_physics(&store, &mut s.c.world);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let native_steps = (distance / native_step).ceil() as usize + 1;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fallback_steps = (distance / fallback_step).ceil() as usize + 1;
    assert!(
        native_steps + 1 < fallback_steps,
        "the two speeds are not far enough apart to tell apart: {native_steps} vs \
             {fallback_steps}"
    );
    for _ in 0..native_steps {
        s.t += DT;
        s.c.update(LocalTime(s.t));
    }
    let h = s.c.world.by_object_id(VICTIM).expect("the body exists");
    let offset = body_origin(&s.c, VICTIM)
        .expect("the body is in a cell")
        .sub(s.destination.frame.origin)
        .mag2()
        .sqrt();
    let finished_in_its_own_time =
        !s.c.world.is_interpolating(h) && offset < dereth_physics::pmanager::CLOSE_ENOUGH;

    println!(
        "sub-step: one sub-step moved {:.6} m ({native_step:.6} its own, \
             {fallback_step:.6} the fallback), max_speed {:?}; the whole walk finished in \
             {native_steps} sub-steps ({fallback_steps} at the fallback), {offset:.6} m short",
        step.moved, step.max_speed
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.correction.a-body-walks-a-correction-at-its-own-speed-and-not-a-default",
        move |_| quantum_is_the_bodys_own && finished_in_its_own_time,
    );
}

dereth_testkit::scenarios! {
    scenario_a_correction_runs_at_the_bodys_own_speed => a_correction_runs_at_the_bodys_own_speed ["movement.correction.a-body-walks-a-correction-at-its-own-speed-and-not-a-default"],
    scenario_a_body_on_its_way_keeps_its_own_facing => a_body_on_its_way_keeps_its_own_facing ["movement.correction.a-body-on-its-way-somewhere-takes-the-place-and-keeps-its-own-facing"],
    scenario_a_running_creature_walks_at_the_rate_the_shard_sent => a_running_creature_walks_at_the_rate_the_shard_sent ["movement.correction.a-creature-the-shard-says-is-running-walks-at-the-rate-the-shard-gave-it"],
    scenario_a_creature_the_shard_said_nothing_about_walks_at_the_default => a_creature_the_shard_said_nothing_about_walks_at_the_default ["movement.correction.a-creature-the-shard-has-said-nothing-about-walks-at-the-clients-own-rate"],
    scenario_the_approach_is_the_shards_own => the_approach_is_the_shards_own ["movement.approach.is-the-shards-and-the-client-never-starts-one-itself"],
    scenario_an_approach_walks_only_to_what_is_out_of_reach => an_approach_walks_only_to_what_is_out_of_reach ["movement.approach.walks-to-what-is-out-of-reach-and-never-to-what-is-already-in-reach"],
    scenario_an_approach_ends_by_stopping_within_reach => an_approach_ends_by_stopping_within_reach ["movement.approach.ends-by-stopping-within-reach-and-nothing-else-happens"],
    scenario_after_an_approach_the_player_has_his_body_back => after_an_approach_the_player_has_his_body_back ["movement.approach.once-it-is-over-the-player-can-walk-the-body-himself-again"],
    scenario_a_target_that_leaves_the_world_ends_the_approach => a_target_that_leaves_the_world_ends_the_approach ["movement.approach.a-target-that-leaves-the-world-ends-it-and-says-which-way-it-went"],
    scenario_an_approach_gives_up_only_on_straying_too_far => an_approach_gives_up_only_on_straying_too_far ["movement.approach.gives-up-only-when-the-body-has-strayed-further-than-the-shard-allowed"],
    scenario_a_second_approach_replaces_the_first => a_second_approach_replaces_the_first ["movement.approach.a-second-one-replaces-the-first-rather-than-queueing"],
    scenario_the_target_is_re_read_on_a_gate => the_target_is_re_read_on_a_gate ["movement.approach.the-target-is-re-read-on-a-gate-and-only-when-it-has-moved"],
    scenario_an_exact_approach_does_not_go_through_what_is_in_the_way => an_exact_approach_does_not_go_through_what_is_in_the_way ["movement.move-to.an-exact-approach-does-not-carry-the-body-through-what-is-in-the-way"],
    scenario_a_replaced_animation_table_does_not_strand_the_body_in_walk => a_replaced_animation_table_does_not_strand_the_body_in_walk ["movement.run.a-body-whose-animation-table-is-replaced-still-runs-when-the-player-said-run"],
    scenario_a_walked_approach_leaves_the_players_own_movement_alone => a_walked_approach_leaves_the_players_own_movement_alone ["movement.approach.a-walked-one-leaves-the-players-own-way-of-moving-alone"],
    scenario_a_body_in_the_air_turns_but_does_not_walk => a_body_in_the_air_turns_but_does_not_walk ["movement.jump.a-body-in-the-air-turns-but-does-not-walk-and-takes-up-the-held-key-when-it-lands"],
    scenario_the_thrown_weapon_stance_reaches_the_body => the_thrown_weapon_stance_reaches_the_body ["movement.stance.the-one-the-shard-sends-for-a-thrown-weapon-reaches-the-body"],
    scenario_a_body_in_its_stance_can_attack_and_leave_combat => a_body_in_its_stance_can_attack_and_leave_combat ["movement.stance.a-body-that-is-in-its-stance-can-attack-and-can-leave-combat-mode"],
    scenario_a_delayed_scenery_effect_survives_the_frames_drain => a_delayed_scenery_effect_survives_the_frames_drain ["scenery.animation.a-delayed-effect-still-happens-when-the-frame-empties-the-queue"],
}

// ---------------------------------------------------------------------------------------
// movement.correction.a-body-on-its-way-somewhere-takes-the-place-and-keeps-its-own-facing
// ---------------------------------------------------------------------------------------

/// **A body already on its way somewhere keeps its own facing.** It walks onto the place the
/// shard named without turning to the way the shard was facing; a body that is going nowhere
/// of its own does turn. Both arms are here, because either alone would pass on a client that
/// always did one of them.
pub fn a_body_on_its_way_keeps_its_own_facing() {
    let store = store();

    // `(its own facing, the shard's, the node's, where it ended, it kept its own, it arrived)`
    let arm = |moving_to: bool| -> (f32, f32, f32, f32, bool, bool) {
        let mut s = scene(&store, Some(RUN_RATE), None, moving_to, 30.0, 210.0);
        let own = body_heading(&s.c, VICTIM);
        let wire = dereth_physics::math::get_heading(&s.destination.frame);
        assert!(
            (own - dereth_physics::math::get_heading(&s.start.frame)).abs() < 1.0,
            "the body was not created on the facing the scenario asked for: {own:.3}"
        );
        assert!(
            (own - wire).abs() > 90.0,
            "the two facings are too close to tell apart"
        );

        s.stream.apply_event(
            &correction_with_facing(VICTIM, &s.destination, 1),
            LocalTime(s.t),
        );
        s.stream.sync_physics(&store, &mut s.c.world);

        let h = s.c.world.by_object_id(VICTIM).expect("the body exists");
        let m = s.c.world.interpolation(h).expect("a manager was made");
        let kept = m.keep_heading;
        let node = m
            .position_queue
            .first()
            .copied()
            .expect("a node was queued");
        let node_heading = dereth_physics::math::get_heading(&node.pos.frame);

        for _ in 0..20 {
            s.t += DT;
            s.c.update(LocalTime(s.t));
        }
        let after = body_heading(&s.c, VICTIM);
        let offset = body_origin(&s.c, VICTIM)
            .expect("the body is in a cell")
            .sub(s.destination.frame.origin)
            .mag2()
            .sqrt();
        (own, wire, node_heading, after, kept, offset < 0.2)
    };

    let (own_a, wire_a, node_a, after_a, kept_a, arrived_a) = arm(true);
    let (own_b, wire_b, node_b, after_b, kept_b, arrived_b) = arm(false);
    println!(
        "sub-step facing: on its way -- node {node_a:.3}, body ended {after_a:.3} \
             (its own {own_a:.3}, the shard's {wire_a:.3}); going nowhere -- node {node_b:.3}, \
             body ended {after_b:.3} (its own {own_b:.3}, the shard's {wire_b:.3})"
    );

    // The body on its way somewhere is marked as keeping its facing, the queued node carries
    // the body's own facing rather than the shard's, and the walked body ends on it.
    let keeps_its_own =
        kept_a && arrived_a && (node_a - own_a).abs() < 1.0 && (after_a - own_a).abs() < 1.0;
    // And the control is marked the other way and ends on the shard's.
    let takes_the_shards =
        !kept_b && arrived_b && (node_b - wire_b).abs() < 1.0 && (after_b - wire_b).abs() < 1.0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.correction.a-body-on-its-way-somewhere-takes-the-place-and-keeps-its-own-facing",
        move |_| keeps_its_own && takes_the_shards,
    );
}

// ---------------------------------------------------------------------------------------
// movement.correction.a-creature-the-shard-says-is-running-walks-at-the-rate-the-shard-gave-it
// ---------------------------------------------------------------------------------------

/// **The rate the shard sent is the rate the body walks at.** A remote creature's own record
/// carries no rate at all -- only the player's own body ever has one -- so the only road a
/// creature's speed has is the movement message the shard sends about it, and this is that
/// road end to end: the message is unpacked, the rate is stored, and the next correction runs
/// at it.
pub fn a_running_creature_walks_at_the_rate_the_shard_sent() {
    let store = store();
    let mut s = scene(&store, None, Some(RUN_RATE), false, 0.0, 0.0);
    let step = one_substep(&mut s, &store);
    let want = walk_speed(RUN_RATE);
    #[allow(clippy::cast_possible_truncation)]
    let achieved = (f64::from(step.moved) / DT) as f32;

    // And the store itself, in isolation, so a regression in the walk and a regression in the
    // store are told apart.
    let assets = Arc::new(DatAnimAssets::new(Arc::clone(&store)));
    let setup = assets
        .setup(ALUVIAN_MALE_SETUP)
        .expect("the shipped setup decodes");
    let mut driver = MotionDriver::new(Arc::clone(&assets) as Arc<dyn AnimAssets>);
    assert!(driver.set_setup(setup), "the part array is built");
    assert!(
        driver.set_motion_table(ALUVIAN_MALE_MOTION_TABLE),
        "the animation table loads"
    );
    let started_at_the_default =
        (driver.movement.interp.my_run_rate - DEFAULT_RUN_RATE).abs() < 1e-6;
    driver.unpack_interpreted_movement(
        MotionCommand::NON_COMBAT,
        &InterpretedMotionState {
            current_style: MotionCommand::NON_COMBAT,
            forward_command: MotionCommand::RUN_FORWARD,
            forward_speed: RUN_RATE,
            ..InterpretedMotionState::default()
        },
        false,
    );
    let env = driver.env;
    let stored = (driver.movement.interp.my_run_rate - RUN_RATE).abs() < 1e-6
        && (driver.movement.interp.get_adjusted_max_speed(&env) - RUN_RATE * RUN_ANIM_SPEED).abs()
            < 1e-4
        && (driver.movement.interp.get_max_speed(&env) - RUN_RATE * RUN_ANIM_SPEED).abs() < 1e-4;

    println!(
        "cached speed running: moved {:.6} m = {achieved:.3} m/s, cached {:.3}, max_speed \
             {:?} (want {want:.3} m/s); the store started at the default: \
             {started_at_the_default}",
        step.moved, step.cached_speed, step.max_speed
    );

    let walked_at_the_shards_rate = step.max_speed == Some(RUN_RATE * RUN_ANIM_SPEED)
        && (step.cached_speed - want).abs() < want * 0.1
        && (achieved - want).abs() < want * 0.1;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
            "movement.correction.a-creature-the-shard-says-is-running-walks-at-the-rate-the-shard-gave-it",
            move |_| walked_at_the_shards_rate && started_at_the_default && stored,
        );
}

// ---------------------------------------------------------------------------------------
// movement.correction.a-creature-the-shard-has-said-nothing-about-walks-at-the-clients-own-rate
// ---------------------------------------------------------------------------------------

/// **The control, and the point of the pair.** A creature the shard has said nothing about is
/// corrected at the client's own starting rate. That is not a rate the client failed to fetch
/// -- a remote body's record never answers with one -- so it is the right answer and not a
/// stand-in for a missing push.
///
/// The client's default rate and its no-interpreter fallback are close enough that the number
/// alone would not tell them apart. What tells them apart is that this body has a real
/// interpreter and answered with a speed of its own, which the fallback arm never does.
pub fn a_creature_the_shard_said_nothing_about_walks_at_the_default() {
    let store = store();
    let mut s = scene(&store, None, None, false, 0.0, 0.0);
    let step = one_substep(&mut s, &store);
    let want = walk_speed(DEFAULT_RUN_RATE);
    #[allow(clippy::cast_possible_truncation)]
    let achieved = (f64::from(step.moved) / DT) as f32;

    println!(
        "cached speed idle: moved {:.6} m = {achieved:.3} m/s, cached {:.3}, max_speed {:?} \
             (want {want:.3} m/s)",
        step.moved, step.cached_speed, step.max_speed
    );

    let at_the_default = step.max_speed == Some(DEFAULT_RUN_RATE * RUN_ANIM_SPEED)
        && (step.cached_speed - want).abs() < want * 0.1
        && (step.cached_speed - walk_speed(RUN_RATE)).abs() > 1.0
        && step.max_speed.is_some();

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
            "movement.correction.a-creature-the-shard-has-said-nothing-about-walks-at-the-clients-own-rate",
            move |_| at_the_default,
        );
}

// =======================================================================================
// The approach.
//
// The client never walks the player anywhere of its own accord. The player's use goes to the
// shard, and the shard answers with a movement message addressed to the player's own body
// telling it to walk to the thing. Everything below is that message being executed.
// =======================================================================================

use dereth_animation::motion::flags as move_flags;
use dereth_primitives::{Frame, Position as Pos};
use dereth_protocol::movement::{
    movement_type, MoveToArm, MovementBody, MovementParameters as WireParams,
    MovementSetObjectMovement,
};
use dereth_protocol::Opcode;
use {
    dereth_client_runtime::character::CharacterInput,
    dereth_client_runtime::character::MovementCommands,
};

/// The thing the player used, and a second one for the scenario that replaces an approach.
const TARGET: ObjectId = ObjectId(0x8000_0997);
const OTHER_TARGET: ObjectId = ObjectId(0x8000_0998);

/// A settled local body on the default landblock, as every approach scenario starts from.
fn local_body(store: &Arc<RetailDatStore>) -> Character {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let mut c = Character::new(
        store,
        &region,
        dereth_world_data::landblock::DEFAULT_LANDBLOCK,
        (96.0, 96.0),
    )
    .expect("the body is created");
    for i in 1..=60 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    c
}

/// A target `metres` due east of the body, at the same height, in the same cell.
fn target_east(c: &Character, metres: f32) -> Pos {
    let p = c.position();
    let o = p.frame.origin;
    Pos::new(
        p.cell,
        Frame::new(Vec3::new(o.x + metres, o.y, o.z), Quat::IDENTITY),
    )
}

/// What the shard sends for a use at `reach`: the client's own default word, with the
/// target's own reach in it.
fn use_params(reach: f32) -> MovementParameters {
    MovementParameters {
        distance_to_object: reach,
        ..MovementParameters::default()
    }
}

fn move_to(target: ObjectId) -> MoveToRequest {
    // A loose item on the ground has no part array of its own worth speaking of.
    MoveToRequest::MoveToObject {
        object_id: target,
        top_level_id: target,
        radius: 0.0,
        height: 0.0,
    }
}

/// Run `seconds` of frames, feeding the target watcher whenever it asks -- which is what the
/// world does with the answers the shard sends about the thing being walked to.
fn walk(c: &mut Character, t0: f64, seconds: f64, target: Pos) -> f64 {
    let mut t = t0;
    #[allow(clippy::cast_possible_truncation)]
    let steps = (seconds * 30.0).round() as i64;
    for _ in 0..steps {
        t += 1.0 / 30.0;
        c.update(LocalTime(t));
        if c.wanted_target().is_some() {
            c.update_target(target, Vec3::ZERO, true);
        }
    }
    t
}

// ---------------------------------------------------------------------------------------
// movement.approach.is-the-shards-and-the-client-never-starts-one-itself
// ---------------------------------------------------------------------------------------

/// One movement message the shard sent, and the characters that recording's login named --
/// the only way to tell "the player" from "a monster".
struct ShardMovement {
    session: &'static str,
    mover: ObjectId,
    autonomous: bool,
    body: MovementBody,
    characters: Vec<ObjectId>,
}

/// Every walk-or-turn instruction in the locked corpus, read off the recordings through the
/// client's own endpoint.
///
/// The replay loop is the smallest one in this crate, built on `dereth_testkit::replay`. The
/// recordings are the ones the corpus index names, read at run time; no count below is pinned.
fn corpus_movements() -> Vec<ShardMovement> {
    let mut out = Vec::new();
    for (id, _slug) in dereth_client_net::client_session::testing::session_index() {
        let records = dereth_testkit::replay::records(id);
        // Named with its own account, which is what this walk was written against; the
        // endpoint itself is the harness's.
        let mut net = dereth_testkit::replay::recorded_endpoint_as(
            &records,
            "dereth-testkit",
            "dereth-testkit",
        );
        for r in records.iter().filter(|r| !r.c2s) {
            net.feed(&r.raw, r.peer(), LocalTime(r.t));
        }
        // The character list is the **whole recording's**, attached after the walk below:
        // a login's character set does not have to reach the session layer before the first
        // movement message does, and pairing each instruction with the list as it stood at
        // that moment would file the shard's own walks as somebody else's.
        let mut characters: Vec<ObjectId> = Vec::new();
        let first = out.len();
        while let Some(m) = dereth_primitives::Transport::poll(&mut net.session.transport) {
            if m.opcode == Opcode::LOGIN_LOGIN_CHARACTER_SET.0 {
                let mut r = dereth_protocol::Reader::new(&m.body);
                if let Ok(set) = dereth_protocol::login::LoginCharacterSet::read(&mut r) {
                    for c in &set.characters {
                        if !characters.contains(&c.gid) {
                            characters.push(c.gid);
                        }
                    }
                }
                continue;
            }
            if m.opcode != Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0 {
                continue;
            }
            let mut r = dereth_protocol::Reader::new(&m.body);
            let msg = MovementSetObjectMovement::read(&mut r)
                .expect("a recorded movement message decodes");
            let buf = msg.decoded_movement().expect("its movement buffer decodes");
            if buf.body.movement_type == movement_type::INVALID {
                continue;
            }
            out.push(ShardMovement {
                session: id,
                mover: msg.id,
                autonomous: buf.autonomous,
                body: buf.body,
                characters: Vec::new(),
            });
        }
        for m in &mut out[first..] {
            m.characters.clone_from(&characters);
        }
    }
    assert!(
        !out.is_empty(),
        "the corpus carries no walk-or-turn instruction at all"
    );
    out
}

/// **The approach belongs to the shard.** Every walk the corpus records being asked of the
/// player's own body was asked by the shard and not taken by the client; every one of them
/// re-encodes to the bytes the recording carries; every one measures its reach as a clearance
/// rather than a centre distance; every one carries the client's own walk-or-run threshold;
/// and the reaches differ from thing to thing, so the reach is the *thing's* and not a
/// constant the client could have assumed.
///
/// **Two clauses are deliberately not asserted**: that every such walk carries the "move
/// towards" direction, and that it carries the client's own walk-or-run threshold. Neither
/// holds over the corpus as it now stands, so both are counted and printed here and asserted
/// nowhere; they are a finding about the recordings, not a claim about the client.
pub fn the_approach_is_the_shards_own() {
    let all = corpus_movements();
    let mut re_encoded = true;
    let mut player_moves = 0usize;
    let mut all_non_autonomous = true;
    let mut all_cylinder = true;
    let mut reaches: Vec<String> = Vec::new();
    // Two things the corpus does not bear out for every such walk. They are counted here and
    // asserted nowhere.
    let mut moves_towards = 0usize;
    let mut clients_own_threshold = 0usize;

    for m in &all {
        let arm = m
            .body
            .decode_move_to()
            .unwrap_or_else(|e| {
                panic!(
                    "{}: a recorded instruction does not decode: {e:?}",
                    m.session
                )
            })
            .expect("a walk-or-turn instruction carries an arm");
        re_encoded &= MovementBody::encode_move_to(&arm) == m.body.unhandled;

        if m.body.movement_type != movement_type::MOVE_TO_OBJECT || !m.characters.contains(&m.mover)
        {
            continue;
        }
        let Some(MoveToArm::MoveToObject { params, .. }) =
            m.body.decode_move_to().expect("decodes")
        else {
            panic!("a walk-to-a-thing decoded as something else")
        };
        let WireParams::MoveTo {
            bitfield,
            distance_to_object,
            walk_run_threshold,
            ..
        } = params
        else {
            panic!("a walk-to-a-thing carries the walk parameter block")
        };
        all_non_autonomous &= !m.autonomous;
        all_cylinder &= bitfield & move_flags::USE_SPHERES != 0;
        moves_towards += usize::from(
            bitfield & move_flags::MOVE_TOWARDS != 0 && bitfield & move_flags::MOVE_AWAY == 0,
        );
        clients_own_threshold += usize::from((walk_run_threshold - 15.0).abs() < 1e-6);
        reaches.push(format!("{distance_to_object}"));
        player_moves += 1;
    }
    reaches.sort();
    reaches.dedup();
    println!(
        "approach: {} recorded walk-or-turn instructions, {player_moves} of them the shard \
             walking the player himself, {} distinct reaches; {moves_towards} of those walks \
             move towards their target and {clients_own_threshold} carry the client's own \
             walk-or-run threshold",
        all.len(),
        reaches.len()
    );

    let held =
        re_encoded && player_moves >= 5 && all_non_autonomous && all_cylinder && reaches.len() >= 3;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.is-the-shards-and-the-client-never-starts-one-itself",
        move |_| held,
    );
}

// ---------------------------------------------------------------------------------------
// movement.approach.walks-to-what-is-out-of-reach-and-never-to-what-is-already-in-reach
// ---------------------------------------------------------------------------------------

/// **Out of reach walks; in reach does not.** A body told to walk to something eight metres
/// away closes the distance; told to walk to something already within the reach the shard
/// named it does not take a step, and the instruction is over on the first answer about the
/// thing. The test is on the reach itself and it is strict: exactly at the reach is arrived.
pub fn an_approach_walks_only_to_what_is_out_of_reach() {
    let store = store();

    // Out of reach.
    let mut c = local_body(&store);
    let start = c.position();
    let target = target_east(&c, 8.0);
    c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
    let recorded_before_any_answer = c.is_moving_to()
        && c.stats.move_tos_performed == 1
        && c.wanted_target().is_some()
        && dereth_animation::motion::moveto::distance(&start, &c.position()) < 0.01;
    let _t = walk(&mut c, 2.0, 6.0, target);
    let moved = dereth_animation::motion::moveto::distance(&start, &c.position());
    let closed = dereth_animation::motion::moveto::distance(&c.position(), &target)
        < dereth_animation::motion::moveto::distance(&start, &target);
    let walked = c.stats.target_updates > 0 && moved > 1.0 && closed;

    // Already in reach.
    let mut c = local_body(&store);
    let start = c.position();
    let near = target_east(&c, 0.2);
    c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
    c.update_target(near, Vec3::ZERO, true);
    let over_at_once = !c.is_moving_to() && c.wanted_target().is_none();
    let _t = walk(&mut c, 2.0, 2.0, near);
    let stood_still = dereth_animation::motion::moveto::distance(&start, &c.position()) < 0.05;

    // The boundary, driven directly: placing a body at exactly the reach through real terrain
    // is not reproducible to the last float, and the boundary is what is under test.
    let p = use_params(0.6);
    let mut strict = p.get_command(0.6).0 == MotionCommand::NONE
        && p.get_command(0.599_9).0 == MotionCommand::NONE
        && p.get_command(0.600_1).0 == MotionCommand::WALK_FORWARD;
    for r in [0.5_f32, 0.8, 1.0, 2.0] {
        let p = use_params(r);
        strict &= p.get_command(r).0 == MotionCommand::NONE
            && p.get_command(r + 0.01).0 == MotionCommand::WALK_FORWARD;
    }

    println!(
        "approach reach: walked {moved:.3} m to a thing 8 m away; stood still for one \
             already in reach: {stood_still}; the reach test is strict: {strict}"
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.walks-to-what-is-out-of-reach-and-never-to-what-is-already-in-reach",
        move |_| recorded_before_any_answer && walked && over_at_once && stood_still && strict,
    );
}

// ---------------------------------------------------------------------------------------
// movement.approach.ends-by-stopping-within-reach-and-nothing-else-happens
// ---------------------------------------------------------------------------------------

/// **Arriving is just stopping.** The body comes to rest within the reach the shard named,
/// the thing stops being watched, nothing is counted as a failure, and the client sends
/// nothing: whatever the player asked for fires on the shard, which is watching for the walk
/// to be over.
pub fn an_approach_ends_by_stopping_within_reach() {
    let store = store();
    let mut c = local_body(&store);
    let target = target_east(&c, 3.0);

    c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
    let _t = walk(&mut c, 2.0, 12.0, target);

    let clearance = dereth_animation::motion::moveto::cylinder_distance(
        c.radius(),
        c.height(),
        &c.position(),
        0.0,
        0.0,
        &target,
    );
    println!("approach arrival: stopped with {clearance:.3} m clearance for a 0.6 m reach");

    let held = !c.is_moving_to()
        && c.wanted_target().is_none()
        && c.stats.move_tos_failed == 0
        && use_params(0.6).has(move_flags::USE_SPHERES)
        && clearance <= 0.6;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.ends-by-stopping-within-reach-and-nothing-else-happens",
        move |_| held,
    );
}

// ---------------------------------------------------------------------------------------
// movement.approach.once-it-is-over-the-player-can-walk-the-body-himself-again
// ---------------------------------------------------------------------------------------

/// **The player gets his body back.** While the shard is walking it, the player's keys do
/// nothing and the body is not handed back on its own -- not while no key is held. The first
/// movement key he presses afterwards takes it back, and the body walks.
pub fn after_an_approach_the_player_has_his_body_back() {
    use dereth_client_runtime::actions::movement::{command, CmdStruct, MovementAction};

    let store = store();
    let mut c = local_body(&store);
    let target = target_east(&c, 3.0);
    let mut commands = MovementCommands::default();
    let mut input = CharacterInput::default();

    c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
    commands.lose_control_to_server(&mut input);
    let mut retakes = 0;
    for frame in 1..=360 {
        if c.wanted_target().is_some() {
            c.update_target(target, Vec3::ZERO, true);
        }
        let (pending, moving) = {
            let d = c.driver();
            (d.movement.motions_pending(), d.movement.is_moving_to())
        };
        if commands.use_time(pending, moving, &mut input) {
            c.take_control_from_server();
            retakes += 1;
        }
        c.input = input;
        c.update(LocalTime(2.0 + f64::from(frame) / 30.0));
    }
    let still_the_shards = retakes == 0
        && commands.lists.controlled_by_server
        && !c.is_moving_to()
        && !c.driver().movement.motions_pending()
        && c.driver().movement.interp.interpreted_state.forward_command == MotionCommand::READY;
    let stopped = c.position();

    assert!(commands.on_action(
        MovementAction::SetMotion(CmdStruct {
            command: command::WALK_FORWARD,
            extent: None,
            start: Some(true),
        }),
        &mut input
    ));
    let retaken = commands.take_control_retake_pending();
    c.take_control_from_server();
    let his_again = retaken
        && !commands.lists.controlled_by_server
        && commands.control_retakes_from_commands == 1;
    c.input = input;
    for frame in 1..=60 {
        c.update(LocalTime(14.0 + f64::from(frame) / 30.0));
    }
    let walked = dereth_animation::motion::moveto::distance(&stopped, &c.position()) > 1.0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.once-it-is-over-the-player-can-walk-the-body-himself-again",
        move |_| still_the_shards && his_again && walked,
    );
}

// ---------------------------------------------------------------------------------------
// movement.approach.a-target-that-leaves-the-world-ends-it-and-says-which-way-it-went
// ---------------------------------------------------------------------------------------

/// **A thing that is not there ends the walk, and the two cases are told apart.** Gone before
/// the first answer about it, the walk ends saying there was no such thing; gone after the
/// walk had started, it ends saying somebody else got there first.
pub fn a_target_that_leaves_the_world_ends_the_approach() {
    const NO_SUCH_OBJECT: u32 = 0x38;
    const OBJECT_GONE: u32 = 0x37;

    let store = store();

    let mut c = local_body(&store);
    let target = target_east(&c, 8.0);
    c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
    c.update_target(target, Vec3::ZERO, false);
    let never_there = c.stats.last_move_to_error == NO_SUCH_OBJECT && !c.is_moving_to();

    let mut c = local_body(&store);
    let target = target_east(&c, 8.0);
    c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
    c.update_target(target, Vec3::ZERO, true);
    let started = c.is_moving_to();
    let _t = walk(&mut c, 2.0, 1.0, target);
    c.update_target(target, Vec3::ZERO, false);
    let taken = c.stats.last_move_to_error == OBJECT_GONE && !c.is_moving_to();

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.a-target-that-leaves-the-world-ends-it-and-says-which-way-it-went",
        move |_| never_there && started && taken,
    );
}

// ---------------------------------------------------------------------------------------
// movement.approach.gives-up-only-when-the-body-has-strayed-further-than-the-shard-allowed
// ---------------------------------------------------------------------------------------

/// **The one thing a walk gives up on.** A player shuffling against a wall is never abandoned
/// by the client -- what stops him is the shard's own patience or his own hands. The client
/// abandons a walk on exactly one condition: the body has strayed further from where it started
/// than the shard allowed, and then it says so.
///
/// How many recorded walks carry a finite allowance is a count of the recordings and not a
/// claim about the client, so it is printed here and asserted nowhere.
pub fn an_approach_gives_up_only_on_straying_too_far() {
    const YOU_CHARGED_TOO_FAR: u32 = 0x3D;

    let mut finite = 0usize;
    let mut seen = 0usize;
    for m in corpus_movements() {
        let arm = m.body.decode_move_to().expect("decodes").expect("an arm");
        let (MoveToArm::MoveToObject { params, .. } | MoveToArm::MoveToPosition { params, .. }) =
            arm
        else {
            continue;
        };
        let WireParams::MoveTo { fail_distance, .. } = params else {
            continue;
        };
        finite += usize::from(fail_distance != f32::MAX);
        seen += 1;
    }
    println!(
        "approach: {finite} of {seen} recorded walks carry a finite allowance; the rest say \
             never give up"
    );

    let store = store();
    let mut c = local_body(&store);
    let target = target_east(&c, 8.0);
    let params = MovementParameters {
        fail_distance: 1.0,
        ..use_params(0.6)
    };
    c.perform_move_to(&move_to(TARGET), &params, Some(1.0));
    let _t = walk(&mut c, 2.0, 8.0, target);
    let gave_up = c.stats.last_move_to_error == YOU_CHARGED_TOO_FAR && !c.is_moving_to();
    let corpus_read = seen > 0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.gives-up-only-when-the-body-has-strayed-further-than-the-shard-allowed",
        move |_| gave_up && corpus_read,
    );
}

// ---------------------------------------------------------------------------------------
// movement.approach.a-second-one-replaces-the-first-rather-than-queueing
// ---------------------------------------------------------------------------------------

/// **Using a second thing while walking to the first turns the body round.** The first walk
/// is reported cancelled, the second is the one in flight, and the thing being watched is the
/// new one.
pub fn a_second_approach_replaces_the_first() {
    const ACTION_CANCELLED: u32 = 0x36;

    let store = store();
    let mut c = local_body(&store);
    let target = target_east(&c, 8.0);

    c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
    c.update_target(target, Vec3::ZERO, true);
    let first_in_flight = c.is_moving_to() && c.stats.move_tos_failed == 0;

    c.perform_move_to(&move_to(OTHER_TARGET), &use_params(0.6), Some(1.0));
    let replaced = c.stats.move_tos_failed == 1
        && c.stats.last_move_to_error == ACTION_CANCELLED
        && c.is_moving_to()
        && c.wanted_target().map(|t| t.id) == Some(OTHER_TARGET);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.a-second-one-replaces-the-first-rather-than-queueing",
        move |_| first_in_flight && replaced,
    );
}

// ---------------------------------------------------------------------------------------
// movement.approach.the-target-is-re-read-on-a-gate-and-only-when-it-has-moved
// ---------------------------------------------------------------------------------------

/// **A walking client does not ask about the thing every frame.** The first answer is never
/// held back; after that the body asks again only once a gate's worth of time has passed
/// *and* the thing has actually drifted. A thing that stays where it is says nothing more.
pub fn the_target_is_re_read_on_a_gate() {
    let store = store();
    let mut c = local_body(&store);
    let target = target_east(&c, 8.0);
    c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));

    let snap = |at: Pos| {
        Some(dereth_animation::motion::moveto::TargetSnapshot {
            object_id: TARGET,
            ok: true,
            position: at,
            velocity: Vec3::ZERO,
        })
    };
    let updates = |c: &Character| c.driver().target_updates;

    let subscribed = c.wanted_target().is_some();
    c.set_target_snapshot(snap(target));
    c.deliver_first_target_update(LocalTime(2.0));
    let first_went_at_once = updates(&c) == 1 && c.is_moving_to();

    let mut further = target;
    further.frame.origin.x += 2.0;
    c.set_target_snapshot(snap(further));
    let mut t = 2.0;
    for _ in 0..8 {
        t += 1.0 / 30.0;
        c.update(LocalTime(t));
    }
    let inside_the_gate = updates(&c) == 1;
    for _ in 0..16 {
        t += 1.0 / 30.0;
        c.update(LocalTime(t));
    }
    let outside_the_gate = updates(&c) == 2;
    for _ in 0..24 {
        t += 1.0 / 30.0;
        c.update(LocalTime(t));
    }
    let a_still_thing_says_nothing = updates(&c) == 2;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.the-target-is-re-read-on-a-gate-and-only-when-it-has-moved",
        move |_| {
            subscribed
                && first_went_at_once
                && inside_the_gate
                && outside_the_gate
                && a_still_thing_says_nothing
        },
    );
}

// =======================================================================================
// movement.move-to.an-exact-approach-does-not-carry-the-body-through-what-is-in-the-way
// Start walking to something, then mash the forward key.
// =======================================================================================

/// The obstacle's radius. A chest is smaller; this is sized so that a body which goes through
/// it cannot be mistaken for one that grazed past.
const OBSTACLE_RADIUS: f32 = 1.5;
/// How far ahead the obstacle stands, and on what bearing. **Not a cardinal**: a body starts
/// facing north, so an obstacle due north would need no turn at all and the calibration below
/// would be measuring a body that was already pointed the right way.
const OBSTACLE_OFFSET: (f32, f32) = (3.0, 5.196_152_4);
/// A flat patch of the default landblock. A body walking off a ridge is airborne, passes
/// *under* an obstacle anchored to the old height and never touches it -- which reads as "the
/// obstacle stopped nothing" for a reason that has nothing to do with the obstacle.
const FLAT_GROUND: (f32, f32) = (20.0, 52.0);
/// One sweep step of the walk. Under this is the spheres touching; the defect this is about
/// puts the body metres in.
const CONTACT_TOLERANCE: f32 = 0.10;

fn walking_body(store: &Arc<RetailDatStore>) -> Character {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let mut c = Character::new(
        store,
        &region,
        dereth_world_data::landblock::DEFAULT_LANDBLOCK,
        FLAT_GROUND,
    )
    .expect("a body on real terrain");
    run_body(&mut c, 0.0, 3.0, CharacterInput::default());
    assert!(
        c.on_ground(),
        "real terrain must support the body before the walk starts"
    );
    c
}

/// Drive the body for `seconds` at the client's own rate, returning the new clock and the
/// path. One clock per scenario, carried across phases: the physics pass steps nothing on a
/// backwards clock, so restarting at zero is a silent no-op.
fn run_body(c: &mut Character, t0: f64, seconds: f64, input: CharacterInput) -> (f64, Vec<Vec3>) {
    let dt = 1.0 / 30.0;
    let mut t = t0;
    let end = t0 + seconds;
    let mut path = Vec::new();
    while t < end {
        t += dt;
        c.input = input;
        c.update(LocalTime(t));
        path.push(c.position().frame.origin);
    }
    (t, path)
}

/// A solid, immovable piece of world furniture standing on the obstacle's bearing.
fn obstacle(c: &mut Character) -> Vec3 {
    use dereth_physics::{SetupGeometry, Sphere};
    let here = c.position();
    let at = Vec3::new(
        here.frame.origin.x + OBSTACLE_OFFSET.0,
        here.frame.origin.y + OBSTACLE_OFFSET.1,
        here.frame.origin.z,
    );
    let geometry = Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(
            Vec3::new(0.0, 0.0, OBSTACLE_RADIUS),
            OBSTACLE_RADIUS,
        )],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, OBSTACLE_RADIUS), OBSTACLE_RADIUS * 2.0),
        radius: OBSTACLE_RADIUS,
        height: 2.0 * OBSTACLE_RADIUS,
        ..SetupGeometry::default()
    });
    let h = c.world.create(ObjectId(0x7000_0001), geometry, true);
    let mut cell = here.cell;
    let mut origin = at;
    dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin);
    c.world.enter_cell(h, cell);
    if let Some(o) = c.world.get_mut(h) {
        let frame = Frame::new(at, Quat::IDENTITY);
        o.state = dereth_physics::PhysicsState(o.state.0 | 0x0000_0001);
        o.set_frame(frame);
        o.position = Pos::new(cell, frame);
        o.transient_state.set_active_bit(false);
    }
    c.world.calc_cross_cells(h, true);
    at
}

/// The walk the shard sends when the player uses something out of reach: to a point **beyond**
/// the obstacle, so the approach runs straight through where it stands.
fn walk_past(c: &mut Character, obstacle_at: Vec3) -> Pos {
    let here = c.position();
    let goal = Pos::new(
        here.cell,
        Frame::new(
            Vec3::new(
                obstacle_at.x + OBSTACLE_OFFSET.0,
                obstacle_at.y + OBSTACLE_OFFSET.1,
                obstacle_at.z,
            ),
            Quat::IDENTITY,
        ),
    );
    c.perform_move_to(
        &MoveToRequest::MoveToPosition { pos: goal },
        &MovementParameters::default(),
        Some(1.0),
    );
    goal
}

/// How far the body is **inside** the obstacle, in metres; zero when they do not overlap. A
/// depth rather than a yes-or-no, because a body sliding round a rounded obstacle rests on its
/// surface and dips a centimetre or two inside it at the sweep's granularity -- which is
/// contact and not passing through.
fn depth_inside(p: Vec3, obstacle_at: Vec3, body_radius: f32) -> f32 {
    let centre = p.add(Vec3::new(0.0, 0.0, body_radius));
    let o = obstacle_at.add(Vec3::new(0.0, 0.0, OBSTACLE_RADIUS));
    let d = centre.sub(o).mag2().sqrt();
    (OBSTACLE_RADIUS + body_radius - d).max(0.0)
}

/// **Start walking to something, then mash the forward key.** The body must not end up inside
/// what is in the way, and must not end up past it.
///
/// A walk the shard sends aims the body *exactly* at where it is going, and that exactness is
/// what no key press can produce -- which is why the sequence reproduces this on demand and a
/// driven client holding a key does not. The control is the same obstacle walked at five
/// degrees off the same line by hand: it is turned aside before this behaviour and after it,
/// which is what makes the first half a measurement of the exact case rather than of whether
/// the obstacle is solid at all.
pub fn an_exact_approach_does_not_go_through_what_is_in_the_way() {
    let store = store();

    // The sequence: a walk the shard sent, then the forward key.
    let mut c = walking_body(&store);
    let radius = c.radius();
    assert!(
        radius > 0.1 && radius < 1.0,
        "the body's own radius is {radius}"
    );
    let at = obstacle(&mut c);
    let start = c.position().frame.origin;
    let _goal = walk_past(&mut c, at);
    let running = c.is_moving_to();
    let (t, _) = run_body(&mut c, 0.0, 1.0, CharacterInput::default());
    let forward = CharacterInput {
        forward: true,
        ..CharacterInput::default()
    };
    let (_, path) = run_body(&mut c, t, 8.0, forward);
    let end = *path.last().expect("the walk produced frames");
    let deepest = path
        .iter()
        .map(|p| depth_inside(*p, at, radius))
        .fold(0.0_f32, f32::max);
    let beyond = end
        .sub(at)
        .dot(Vec3::new(OBSTACLE_OFFSET.0, OBSTACLE_OFFSET.1, 0.0))
        / 6.0;
    let set_off = end.sub(start).mag2().sqrt() > 1.0;

    // The calibration: the walk really did turn the body, and it snapped it onto the bearing.
    let mut c = walking_body(&store);
    let at2 = obstacle(&mut c);
    let before = dereth_physics::math::get_heading(&c.position().frame);
    let goal = walk_past(&mut c, at2);
    let _ = run_body(&mut c, 0.0, 3.0, CharacterInput::default());
    let after = dereth_physics::math::get_heading(&c.position().frame);
    let want = dereth_animation::motion::moveto::position_heading(&c.position(), &goal);
    let wrap = |d: f32| {
        let d = d.rem_euclid(360.0);
        if d > 180.0 {
            360.0 - d
        } else {
            d
        }
    };
    let err = wrap(after - want);
    let turned = wrap(after - before);

    // The control: five degrees off the same bearing, aimed by hand.
    let mut c = walking_body(&store);
    let radius3 = c.radius();
    let at3 = obstacle(&mut c);
    let bearing = dereth_primitives::num::math::atan2f(OBSTACLE_OFFSET.0, OBSTACLE_OFFSET.1);
    let half = (bearing + 5.0_f32.to_radians()) / 2.0;
    let mut here = c.position();
    here.frame.rotation = Quat::new(
        dereth_primitives::num::math::cosf(half),
        0.0,
        0.0,
        dereth_primitives::num::math::sinf(half),
    );
    c.teleport(here);
    let (t, _) = run_body(&mut c, 0.0, 1.0, CharacterInput::default());
    let (_, path3) = run_body(&mut c, t, 8.0, forward);
    let deepest3 = path3
        .iter()
        .map(|p| depth_inside(*p, at3, radius3))
        .fold(0.0_f32, f32::max);

    println!(
        "wall: the exact approach ended {deepest:.4} m inside the obstacle at its worst \
             and {beyond:.3} m beyond it; the walk turned the body {turned:.4} degrees onto the \
             bearing with a {err:.6} degree error; the hand-aimed control got {deepest3:.4} m in"
    );

    let held = running
        && set_off
        && deepest < CONTACT_TOLERANCE
        && beyond < 0.0
        && turned > 20.0
        && err < 0.01
        && deepest3 < CONTACT_TOLERANCE;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.move-to.an-exact-approach-does-not-carry-the-body-through-what-is-in-the-way",
        move |_| held,
    );
}

// =======================================================================================
// The walk bit: the body's walk-or-run modifier against the player's own run word.
// =======================================================================================

use dereth_animation::motion::HoldKey;
use dereth_client_runtime::actions::movement::{action, on_action};
use dereth_dat::DbType;
use dereth_input::ActionId;
use dereth_primitives::DataId;

fn key(a: ActionId, start: bool) -> dereth_client_runtime::actions::Action {
    dereth_client_runtime::actions::Action {
        id: a,
        phase: if start {
            dereth_client_runtime::actions::ActionPhase::Begin
        } else {
            dereth_client_runtime::actions::ActionPhase::End
        },
        extent: 1.0,
        repeats: 0,
    }
}

/// The modifier the whole subject is about: whether the body thinks it is running or walking.
fn hold_key(c: &Character) -> HoldKey {
    c.driver().movement.interp.raw_state.current_holdkey
}

/// What the body is actually playing, which is the observable the modifier decides.
fn playing(c: &Character) -> MotionCommand {
    c.driver().movement.interp.interpreted_state.forward_command
}

fn settled_body(store: &Arc<RetailDatStore>) -> Character {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let mut c = Character::new(
        store,
        &region,
        dereth_world_data::landblock::DEFAULT_LANDBLOCK,
        (96.0, 96.0),
    )
    .expect("the body");
    for i in 1..=60 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        c.on_ground(),
        "the body must settle before anything is measured"
    );
    c
}

/// A settled body that runs rather than walks. *Run as default movement* is on, as it is on
/// every shipped character, so the key-up state is the running one.
fn running_body(store: &Arc<RetailDatStore>) -> (Character, MovementCommands, CharacterInput) {
    let mut c = settled_body(store);
    let mut mc = MovementCommands::default();
    let mut input = CharacterInput::default();
    mc.ui_toggles_run = true;
    assert!(mc.on_action(
        on_action(&key(action::TOGGLE_RUN_WALK, false), |_| None),
        &mut input
    ));
    assert!(
        input.run,
        "running is the default state and the key walks you"
    );
    let _ = mc.take_control_retake_pending();
    c.input = input;
    for i in 1..=4 {
        c.update(LocalTime(2.0 + f64::from(i) / 30.0));
    }
    assert_eq!(
        hold_key(&c),
        HoldKey::Run,
        "the premise: the body agrees it runs"
    );
    (c, mc, input)
}

fn press_forward(
    c: &mut Character,
    mc: &mut MovementCommands,
    input: &mut CharacterInput,
    t0: f64,
) {
    use dereth_client_runtime::actions::movement::{command, CmdStruct, MovementAction};
    assert!(mc.on_action(
        MovementAction::SetMotion(CmdStruct {
            command: command::WALK_FORWARD,
            extent: None,
            start: Some(true),
        }),
        input
    ));
    if mc.take_control_retake_pending() {
        c.take_control_from_server();
    }
    c.input = *input;
    for i in 1..=6 {
        c.update(LocalTime(t0 + f64::from(i) / 30.0));
    }
}

fn release_forward(
    c: &mut Character,
    mc: &mut MovementCommands,
    input: &mut CharacterInput,
    t0: f64,
) {
    use dereth_client_runtime::actions::movement::{command, CmdStruct, MovementAction};
    assert!(mc.on_action(
        MovementAction::SetMotion(CmdStruct {
            command: command::WALK_FORWARD,
            extent: None,
            start: Some(false),
        }),
        input
    ));
    c.input = *input;
    for i in 1..=6 {
        c.update(LocalTime(t0 + f64::from(i) / 30.0));
    }
}

/// Another shipped animation table that really loads, so that the scenario writes no id of
/// its own.
fn another_animation_table(store: &Arc<RetailDatStore>) -> DataId {
    use dereth_animation::data::AnimAssets as _;
    let assets = DatAnimAssets::new(Arc::clone(store));
    for id in store.ids_of(DbType::MTable) {
        if id != ALUVIAN_MALE_MOTION_TABLE && assets.motion_table(id).is_some() {
            return id;
        }
    }
    panic!("the shipped data holds no second animation table");
}

// ---------------------------------------------------------------------------------------
// movement.run.a-body-whose-animation-table-is-replaced-still-runs-when-the-player-said-run
// ---------------------------------------------------------------------------------------

/// **The defect guarded: the body gets stuck walking and nothing clears it.**
///
/// Any description the shard sends that names an animation table rebuilds the body's movement
/// and puts its *walk or run* modifier back to walk -- while the player's own run word has not
/// moved, so nothing the player did could tell the two apart. The next forward key must
/// re-derive the modifier from what the player is actually holding rather than remember what
/// it last wrote, and then the body runs again without the player touching the run key.
pub fn a_replaced_animation_table_does_not_strand_the_body_in_walk() {
    let store = store();
    let (mut c, mut mc, mut input) = running_body(&store);

    press_forward(&mut c, &mut mc, &mut input, 3.0);
    let ran_first = playing(&c) == MotionCommand::RUN_FORWARD;
    release_forward(&mut c, &mut mc, &mut input, 4.0);

    // The producer, and it is the client's own: the body is given another animation table and
    // then its own back, so nothing but the movement state has changed.
    let other = another_animation_table(&store);
    assert!(c.set_setup_id(ALUVIAN_MALE_SETUP, Some(other)).is_ok());
    assert!(c
        .set_setup_id(ALUVIAN_MALE_SETUP, Some(ALUVIAN_MALE_MOTION_TABLE))
        .is_ok());
    let back_to_its_own = c.motion_table_id() == ALUVIAN_MALE_MOTION_TABLE;
    let disagreed = hold_key(&c) == HoldKey::None && input.run;

    press_forward(&mut c, &mut mc, &mut input, 5.0);
    let re_derived = hold_key(&c) == HoldKey::Run && playing(&c) == MotionCommand::RUN_FORWARD;

    println!(
        "walk bit: ran first {ran_first}, the table swap put the modifier back to \
             walk with the player's run word unmoved {disagreed}, and the next forward key \
             re-derived it {re_derived}"
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.run.a-body-whose-animation-table-is-replaced-still-runs-when-the-player-said-run",
        move |_| ran_first && back_to_its_own && disagreed && re_derived,
    );
}

// ---------------------------------------------------------------------------------------
// movement.approach.a-walked-one-leaves-the-players-own-way-of-moving-alone
// ---------------------------------------------------------------------------------------

/// **The guard, and it is green on both sides of the defect above.** The stuck walk was
/// reported against a shard-driven approach, so the named trigger is checked rather than
/// assumed: an approach close enough to be walked rather than run chooses its own walk for
/// the duration and leaves the player's own way of moving exactly as it was.
pub fn a_walked_approach_leaves_the_players_own_movement_alone() {
    let store = store();
    let (mut c, mut mc, mut input) = running_body(&store);
    let p = c.position();
    let o = p.frame.origin;
    let target = Pos::new(
        p.cell,
        Frame::new(Vec3::new(o.x + 3.0, o.y, o.z), Quat::IDENTITY),
    );

    // Three metres is well inside the threshold at which a walk becomes a run.
    c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
    mc.lose_control_to_server(&mut input);
    let mut t = 3.0;
    for _ in 0..360 {
        if c.wanted_target().is_some() {
            c.update_target(target, Vec3::ZERO, true);
        }
        let (pending, moving) = {
            let d = c.driver();
            (d.movement.motions_pending(), d.movement.is_moving_to())
        };
        if mc.use_time(pending, moving, &mut input) {
            c.take_control_from_server();
        }
        c.input = input;
        t += 1.0 / 30.0;
        c.update(LocalTime(t));
    }
    let finished = !c.is_moving_to();
    let modifier_untouched = hold_key(&c) == HoldKey::Run;

    press_forward(&mut c, &mut mc, &mut input, t);
    let still_runs = playing(&c) == MotionCommand::RUN_FORWARD;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.a-walked-one-leaves-the-players-own-way-of-moving-alone",
        move |_| finished && modifier_untouched && still_runs,
    );
}

// =======================================================================================
// movement.jump.a-body-in-the-air-turns-but-does-not-walk-and-takes-up-the-held-key-when-it-lands
// =======================================================================================

/// **What a body may do while its feet are off the ground.** It may turn, and it may not
/// walk; the forward key it is holding is remembered rather than thrown away, and the moment
/// the real floor comes back under it the body walks -- with no second press, no timeout and
/// nothing forcing it back to standing. Let go and it stops; press back and it backs up.
///
/// The floor is a real one, at a place a recording says a player really stood.
pub fn a_body_in_the_air_turns_but_does_not_walk() {
    use dereth_animation::motion::MotionInterp;

    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let mut c = Character::new(&store, &region, 0x7f03, (96.0, 96.0))
        .expect("a body in the training academy's landblock");

    // Where a recorded player stood, taken out of the recording rather than chosen.
    let corpus =
        dereth_client_net::client_session::testing::Corpus::load("early-inventory-and-casting")
            .expect("the corpus parses")
            .expect("the corpus is generated");
    let row = corpus
        .blobs
        .iter()
        .find(|r| r.idx == 143)
        .expect("the recorded position");
    let mut a = dereth_protocol::actions::unpack_action(&row.payload).expect("it unpacks");
    let p = dereth_protocol::movement::MovementMoveToState::read(&mut a.body)
        .expect("it decodes")
        .0
        .position;
    let pos = Pos::new(
        dereth_primitives::CellId(p.objcell_id),
        Frame::new(
            Vec3::new(p.frame.origin.x, p.frame.origin.y, p.frame.origin.z),
            Quat::new(
                p.frame.orientation.w,
                p.frame.orientation.x,
                p.frame.orientation.y,
                p.frame.orientation.z,
            ),
        ),
    );
    c.land().load_block_cells(pos.cell.landblock());
    c.teleport(pos);
    c.stop_completely_from_action();

    let mut frame = 0u32;
    let mut agreed = true;
    let step = |c: &mut Character, frame: &mut u32, agreed: &mut bool| {
        *frame += 1;
        c.update(LocalTime(f64::from(*frame) / 30.0));
        let body = c.world.get(c.handle).expect("the body is live");
        let in_contact = body.transient_state.in_contact();
        let in_cell = body.cell.is_some();
        let on_ground = c.on_ground();
        let d = c.driver();
        *agreed &=
            d.env.contact == in_contact && d.env.on_ground == on_ground && d.env.in_cell == in_cell;
    };
    for _ in 0..60 {
        step(&mut c, &mut frame, &mut agreed);
    }
    let landed_first = c.on_ground();

    c.input.jump = true;
    step(&mut c, &mut frame, &mut agreed);
    c.input.jump = false;
    let left_the_floor = !c.on_ground();

    c.input.forward = true;
    c.input.turn_left = true;
    let airborne_heading = dereth_physics::math::get_heading(&c.position().frame);
    let mut air = 0;
    let mut could_not_walk = true;
    let mut could_turn = true;
    let mut key_remembered = true;
    while !c.on_ground() && air < 120 {
        step(&mut c, &mut frame, &mut agreed);
        air += 1;
        if !c.on_ground() {
            let d = c.driver();
            could_not_walk &=
                !MotionInterp::contact_allows_move(MotionCommand::WALK_FORWARD, &d.env);
            could_turn &= MotionInterp::contact_allows_move(MotionCommand::TURN_LEFT, &d.env);
            key_remembered &=
                d.movement.interp.raw_state.forward_command == MotionCommand::WALK_FORWARD;
        }
    }
    let came_back_down = air > 1 && c.on_ground();
    let turned = (dereth_physics::math::get_heading(&c.position().frame) - airborne_heading).abs()
        > f32::EPSILON;

    c.input.turn_left = false;
    let at_landing = c.position();
    for _ in 0..30 {
        step(&mut c, &mut frame, &mut agreed);
    }
    let walked_on_landing =
        dereth_animation::motion::moveto::distance(&at_landing, &c.position()) > 0.5;

    c.input = CharacterInput::default();
    for _ in 0..60 {
        step(&mut c, &mut frame, &mut agreed);
    }
    let stopped = c.position();
    for _ in 0..30 {
        step(&mut c, &mut frame, &mut agreed);
    }
    let stayed_stopped = dereth_animation::motion::moveto::distance(&stopped, &c.position()) < 0.05
        && !c.driver().movement.motions_pending()
        && c.driver().motion_table.pending_len() == 0;

    c.input.back = true;
    for _ in 0..30 {
        step(&mut c, &mut frame, &mut agreed);
    }
    let backed_up = dereth_animation::motion::moveto::distance(&stopped, &c.position()) > 0.3;
    c.input = CharacterInput::default();
    for _ in 0..60 {
        step(&mut c, &mut frame, &mut agreed);
    }
    let back_to_standing =
        c.driver().movement.interp.interpreted_state.forward_command == MotionCommand::READY;

    println!("door: {air} frames in the air, then the floor came back");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
            "movement.jump.a-body-in-the-air-turns-but-does-not-walk-and-takes-up-the-held-key-when-it-lands",
            move |_| {
                agreed
                    && landed_first
                    && left_the_floor
                    && could_not_walk
                    && could_turn
                    && key_remembered
                    && came_back_down
                    && turned
                    && walked_on_landing
                    && stayed_stopped
                    && backed_up
                    && back_to_standing
            },
        );
}

// =======================================================================================
// The stance a thrown weapon puts the body in.
//
// These two rows sit with the motion vocabulary; see the note at the top of this block.
// =======================================================================================

/// What the shard sends for a body that is standing in its stance.
const READY_INDEX: u16 = 3;
/// The two stances, as the low half of the word the shard sends. A bow's is the same number
/// in both the shipped vocabulary and the one the reference server writes from; a thrown
/// weapon's is not, which is what made the defect this came from.
const BOW_INDEX: u16 = 0x003F;
const THROWN_INDEX: u16 = 0x013B;

/// The exact message the shard sends when the player's stance changes, built as **bytes** so
/// that the word under test is the word that crosses the wire.
fn stance_message(object: ObjectId, stance_index: u16) -> Vec<u8> {
    use dereth_protocol::archive::Writer;
    let mut w = Writer::new();
    w.u32(0xF74C);
    w.u32(object.0);
    w.u16(1); // the object-instance sequence the message header carries
    w.u16(4); // movement timestamp
    w.u16(2); // the shard's own control timestamp
    w.u8(0); // not an echo of the player's own move: this is the shard moving him
    w.align4();
    w.u16(0); // no walk-or-turn instruction, and no flags
    w.u16(stance_index); // the style word, before the interpreted state
    w.u32(0b11); // the interpreted state carries a style and a forward command
    w.u16(stance_index);
    w.u16(READY_INDEX);
    w.align4();
    w.into_inner()
}

/// The two statements the world makes with a stance-carrying message, on a body.
fn apply_shard_stance(c: &mut Character, blob: &[u8]) {
    use dereth_protocol::archive::Reader as ArchiveReader;
    let mut r = ArchiveReader::new(blob);
    r.u32().expect("opcode");
    r.u32().expect("object");
    r.u16().expect("instance");
    let buf = dereth_protocol::movement::MovementBuffer::read(&mut r).expect("the buffer decodes");
    assert!(
        !buf.autonomous,
        "the shard's own move, not an echo of the player's"
    );

    let wire = buf
        .body
        .interpreted
        .as_ref()
        .expect("this message carries one");
    let cmd = |i: Option<u16>, fallback: MotionCommand| {
        i.and_then(MotionCommand::from_index).unwrap_or(fallback)
    };
    let base = InterpretedMotionState::default();
    let state = InterpretedMotionState {
        current_style: cmd(wire.current_style, base.current_style),
        forward_command: cmd(wire.forward_command, base.forward_command),
        ..base
    };
    if let Some(style) = MotionCommand::from_index(buf.body.current_style) {
        c.apply_movement_style(style);
    }
    c.move_to_interpreted_state(&state);
    for i in 1..=30 {
        c.update(LocalTime(2.0 + f64::from(i) / 30.0));
    }
}

/// A player in missile combat with something attackable picked out, carrying the combat table
/// a real character is born with.
fn world_with_target() -> (dereth_client_model::World, ObjectId) {
    use dereth_client_model::combat::{CombatMode, COMBAT_TABLE_DID};
    use dereth_rules::slots::loc;
    let mut w = dereth_client_model::World::new();
    let player = ObjectId(0x5000_0476);
    let monster = ObjectId(0x8000_0777);
    w.player = Some(player);
    let mut pw = dereth_client_model::Weenie::new(player);
    pw.pwd.name = "Aldis".into();
    pw.qualities
        .get_or_insert_with(dereth_client_model::Qualities::new)
        .set(
            dereth_client_model::StatKey::new(dereth_client_model::StatType::Did, COMBAT_TABLE_DID),
            dereth_client_model::StatValue::Did(DataId(0x3000_0000)),
        );
    w.tables.weenies.insert(player, pw);
    w.tables.inventories.insert(
        player,
        dereth_client_model::objects::ObjectInventory::new(player),
    );
    let mut m = dereth_client_model::Weenie::new(monster);
    m.pwd.name = "Mosswart".into();
    m.pwd.obj_type = dereth_rules::weenie::item_type::CREATURE;
    m.pwd.bitfield |= dereth_rules::weenie::bitfield::ATTACKABLE;
    w.tables.weenies.insert(monster, m);
    w.set_selected_object(Some(monster), false, &mut dereth_client_model::NullSink);
    w.inventory_mask = loc::MISSILE_WEAPON | loc::MELEE_WEAPON | loc::HELD;
    w.combat.combat_mode = CombatMode::Missile;
    (w, monster)
}

/// What one weapon's whole story produces, so that a bow and a thrown weapon are read the
/// same way.
struct Story {
    stance: u32,
    ready_to_change_mode: bool,
    ready_to_attack: bool,
    left_combat: bool,
    mode_changes_sent: usize,
    attacks_sent: usize,
}

fn play(stance_index: u16) -> Story {
    use dereth_client_model::combat::{AttackHeight, CombatMode};
    use dereth_client_model::{RecordingRequests, Request};

    let store = store();
    let mut c = settled_body(&store);
    let (mut w, _monster) = world_with_target();
    let player = w.player.expect("seeded");

    apply_shard_stance(&mut c, &stance_message(player, stance_index));
    {
        let d = c.driver();
        w.combat.current_style = d.movement.interp.interpreted_state.current_style.0;
        w.combat.forward_command = d.movement.interp.interpreted_state.forward_command.0;
    }

    let pending = c.driver().movement.motions_pending();
    let ready_to_change_mode = w.player_in_ready_position(false, Some(pending));
    let ready_to_attack = w.player_in_ready_position(true, Some(pending));

    let mut attack = RecordingRequests::default();
    let _ = w.execute_attack(&mut attack, AttackHeight::Medium, true, ready_to_attack);
    let attacks_sent = attack
        .0
        .iter()
        .filter(|r| matches!(r, Request::TargetedMissileAttack(_)))
        .count();

    let mut req = RecordingRequests::default();
    let mut out = dereth_client_model::NullSink;
    let (to, _refusal) = w.toggle_combat_mode_target(false);
    let _ = w.set_combat_mode(&mut req, &mut out, to, true, ready_to_change_mode, false);
    // Ten frames of the retry, because a defect that only delays the send by a frame must not
    // read as the one this came from.
    for _ in 0..10 {
        let _ = w.combat_use_time(&mut req, &mut out, ready_to_change_mode);
    }
    let mode_changes_sent = req
            .0
            .iter()
            .filter(|r| {
                matches!(r, Request::ChangeCombatMode(m) if m.combat_mode == CombatMode::NonCombat.raw())
            })
            .count();

    let stance = c.driver().movement.interp.interpreted_state.current_style.0;
    Story {
        stance,
        ready_to_change_mode,
        ready_to_attack,
        left_combat: w.combat.combat_mode == CombatMode::NonCombat,
        mode_changes_sent,
        attacks_sent,
    }
}

// ---------------------------------------------------------------------------------------
// movement.stance.the-one-the-shard-sends-for-a-thrown-weapon-reaches-the-body
// ---------------------------------------------------------------------------------------

/// **The first symptom: "it just shows me standing".** The stance the shard sends for a thrown
/// weapon has to arrive as a *stance* and not be filed as some unrelated pose, or the player
/// never gets into it.
pub fn the_thrown_weapon_stance_reaches_the_body() {
    let s = play(THROWN_INDEX);
    println!(
        "stance: the thrown-weapon stance arrived as {:#010X}",
        s.stance
    );
    let held = s.stance == 0x8000_013B;
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.stance.the-one-the-shard-sends-for-a-thrown-weapon-reaches-the-body",
        move |_| held,
    );
}

// ---------------------------------------------------------------------------------------
// movement.stance.a-body-that-is-in-its-stance-can-attack-and-can-leave-combat-mode
// ---------------------------------------------------------------------------------------

/// **The other two symptoms: no throw, and no way out of combat.** Both are decided by the same
/// question -- is the body in its stance? -- so a body that is gets both, and the observable is
/// the message that reaches the shard rather than a local field: the way out of combat is
/// parked rather than refused when the body is not ready, so asking whether the call succeeded
/// would be green against the defect.
///
/// A bow is the control. Its stance is the same word in both vocabularies, so its half passes
/// whatever happens to a thrown weapon's -- which is what says this can express a success at
/// all.
pub fn a_body_in_its_stance_can_attack_and_leave_combat() {
    let bow = play(BOW_INDEX);
    let thrown = play(THROWN_INDEX);
    println!(
        "stance: the bow control -- stance {:#010X}, {} attack(s) and {} way(s) out sent; \
             the thrown weapon -- stance {:#010X}, {} and {}",
        bow.stance,
        bow.attacks_sent,
        bow.mode_changes_sent,
        thrown.stance,
        thrown.attacks_sent,
        thrown.mode_changes_sent
    );
    let ok = |s: &Story, want: u32| {
        s.stance == want
            && s.ready_to_change_mode
            && s.ready_to_attack
            && s.attacks_sent == 1
            && s.mode_changes_sent == 1
            && s.left_combat
    };
    let held = ok(&bow, 0x8000_003F) && ok(&thrown, 0x8000_013B);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.stance.a-body-that-is-in-its-stance-can-attack-and-can-leave-combat-mode",
        move |_| held,
    );
}

// =======================================================================================
// scenery.animation.a-delayed-effect-still-happens-when-the-frame-empties-the-queue
// =======================================================================================

/// **A torch keeps flickering.** A piece of scenery whose shipped animation script schedules
/// its next effect for later still produces it, although the frame takes everything the
/// object has raised off it every single tick. The timer and the raised effects are two
/// different lists, and emptying one must not empty the other -- an implementation that
/// reached for the wrong one would stop the scenery for ever and nothing else in the suite
/// would notice.
///
/// The script is a shipped one, and the delayed kind on purpose: the scenery of an ordinary
/// outdoor landblock uses the immediate kind, so it exercises the re-arming loop and not the
/// timer.
pub fn a_delayed_scenery_effect_survives_the_frames_drain() {
    use dereth_animation::{AnimAssets as AnimAssetsTrait, AnimEvent};
    use dereth_primitives::ServerTime;

    /// One of the shipped four-script ambient rings: each makes an effect at once and asks
    /// for the next script after a rolled pause.
    const RING: DataId = DataId(0x3300_11C3);
    const RING_CHILD: DataId = DataId(0x3300_11C4);

    let store = store();
    let assets: Arc<dyn AnimAssetsTrait> = Arc::new(DatAnimAssets::new(Arc::clone(&store)));
    let mut d = MotionDriver::new(assets);
    // A placed piece of scenery is always in a cell, and the delayed arm reads that when it
    // fires.
    d.env.in_cell = true;
    d.cur_time = ServerTime(0.0);
    assert!(
        d.play_script_internal(RING),
        "the shipped ring script queues"
    );

    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut armed: Option<(f64, f32)> = None;
    let mut effects: Vec<f64> = Vec::new();
    let mut high_water = 0usize;
    let mut drained_clean = true;

    for _ in 0..60 {
        d.cur_time = ServerTime(t);
        d.update_scripts();
        d.update_particles(false);
        d.update_fp_hooks();
        // The frame's own drain, verbatim: take everything, every tick, and nothing else.
        high_water = high_water.max(d.pending_events());
        for e in d.take_events() {
            match e {
                AnimEvent::CallPes { script, pause } => {
                    if armed.is_none() && script == RING_CHILD {
                        assert!(pause > 0.0, "the ring's hook is the delayed kind");
                        armed = Some((t, pause));
                    }
                }
                AnimEvent::CreateParticleEmitter { .. } => effects.push(t),
                _ => {}
            }
        }
        drained_clean &= d.pending_events() == 0;
        t += dt;
    }

    let (arm_t, delay) = armed.expect("the ring script's delayed request never fired");
    let after: Vec<f64> = effects.iter().copied().filter(|x| *x > arm_t).collect();
    println!(
        "host event drain: armed at {arm_t:.4} s for {delay} s, {} effect(s) afterwards, queue \
             high-water {high_water}",
        after.len()
    );
    let due = arm_t + f64::from(delay);
    let fired_on_time = !after.is_empty() && after[0] >= due - 1e-9 && after[0] <= due + 3.0 * dt;
    // Draining every tick means the queue never holds more than one tick's worth.
    let keeps_up = high_water <= 8;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "scenery.animation.a-delayed-effect-still-happens-when-the-frame-empties-the-queue",
        move |_| drained_clean && fired_on_time && keeps_up,
    );
}
