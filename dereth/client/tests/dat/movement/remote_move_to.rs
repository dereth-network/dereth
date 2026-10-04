//! A creature told to approach you walks up, turns to you, and stops: its heading closes on the
//! target, it arrives inside `distance_to_object`, faces you, cleans up and stays still. Also: the
//! target quantum's one-second hysteresis, a recorded fellowship turn-to-object toward a player the
//! session never listed, a style-zero header, and the standing-long-jump flag on the remote arm.
//! Fixture: long-solo-play's Sparring Golem and its recorded `0xF74C Movement_SetObjectMovement`
//! approach, and the fellowship recordings' remote arms, in a headless App on real terrain.
//!
//! # The original move-to machine
//!
//! Each physics substep handles targeting, advances the movement manager, moves the part array,
//! and then advances position management, in that order. Move-to work advances only while the
//! body has contact and a pending action with a top-level object and initialized plan. Node type 9
//! handles a turn-to-heading step; node type 7 handles a move-to-position step.
//!
//! This file derives its expectations from the two node handlers:
//!
//! * A turn node stops when `heading_greater(get_heading(), target_heading, current_command)`
//!   becomes true. The body snaps to the target heading, the node is popped, the turn stops, and
//!   the next node begins. This is a half-plane test, so the preceding frame need not approach the
//!   target monotonically; the assertions use the same half-plane rule.
//! * A position node uses a corrective-turn dead band of
//!   `diff <= 20.0 || diff >= 340.0`. It arrives when `d <= distance_to_object`, or when moving
//!   away and `d >= min_distance`. Its target
//!   quantum is rewritten **only** when `|d/|v| - get_target_quantum()| > 1.0`
//!   while the closing speed exceeds 0.1.
//!
//! The move-to-object planner checks movement-parameter bit 6 for `use_final_heading`. When set,
//! it appends a final turn whose heading is the bearing to the interpolated target plus
//! `desired_heading`, wrapped once at 360 degrees. That is the "turns to you" behaviour.
//!
//! Cleanup stops the current and auxiliary motions using default parameters with the
//! cancel-move-to bit cleared, clears a live target when both the top-level object and
//! movement type are valid, and reinitializes the manager's local state.
//!
//! # What this file drives, and through what
//!
//! A real `App` with a headless simulated presentation, `ui: true`, real `DEFAULT_LANDBLOCK` terrain and a real
//! physics world. The local body takes **long-solo-play's recorded identity** (its `LoginCreatePlayer` id and
//! its own `0xF745`), the creature is **long-solo-play's Sparring Golem `0x800009D2`, created from its
//! own recorded `0xF745`** — so its setup `0x0200078C`, motion table `0x09000101` and scale 0.9
//! are the shard's, not a fixture's — and the command is **long-solo-play's own recorded `0xF74C`
//! `MoveToObject` at t=91.501**, re-encoded only in its packed fall-through origin and its
//! movement timestamp. Its `MovementParameters` bitfield is the recording's `0x1EFFF`
//! (`use_final_heading | sticky | move_away` over the `0x1EE0F` default) and its
//! `walk_run_threshold` the recording's `1.0`.
//!
//! Everything reaches the client through `ObjectStream::apply_event` and `App::frame`, the
//! production path. `App::sync_objects` feeds the shared movement receiver, while
//! `Character::update` sweeps the physics world and reaches `SharedMotion::tick_movement` and the
//! movement-time update. Nothing in this file calls the move-to implementation directly.
//!
//! **No datagram leaves this process.** There is no `ClientNetwork` and no socket at all.

use crate::common::client_dir;

use dereth_animation::motion::moveto::{
    cylinder_distance, heading_diff, heading_greater, position_heading,
};
use dereth_animation::motion::{flags, MovementParameters as RtParams};
use dereth_animation::table::MovementType;
use dereth_animation::MotionCommand;
use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::world::{SceneConfig, DEFAULT_LANDBLOCK};
use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::{sim_present::SimPresentation, world_state::WorldState};
use dereth_primitives::{LocalTime, ObjectId, Position, Vec3};
use dereth_protocol::movement::{MoveToArm, MovementBody, MovementSetObjectMovement};
use dereth_protocol::objects::ItemCreateObject;
use dereth_protocol::{Message, Opcode};

/// long-solo-play's own character.
const PLAYER: ObjectId = ObjectId(0x5000_000a);
/// long-solo-play's Sparring Golem — the creature the recorded approach is addressed to.
const GOLEM: ObjectId = ObjectId(0x8000_09d2);
/// The recorded `MoveToObject` this file replays, by capture timestamp.
const APPROACH_MICROS: u64 = 91_500_548;
/// The original position-node handler's 20-degree corrective-turn dead band.
const DEAD_BAND: f32 = 20.0;
/// The word that closes every recorded login's hidden create -- `objects/hidden_state.rs`.
const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;

// ---------------------------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------------------------

fn long_solo_play() -> Vec<CorpusBlob> {
    Corpus::load("long-solo-play")
        .expect("the locked corpus decodes")
        .expect("long-solo-play is this file's oracle")
        .blobs
}

fn subject(row: &CorpusBlob) -> ObjectId {
    ObjectId(u32::from_le_bytes(
        row.payload[4..8].try_into().expect("an object id"),
    ))
}

fn recorded_create(rows: &[CorpusBlob], id: ObjectId) -> ItemCreateObject {
    let row = rows
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                && subject(r) == id
        })
        .unwrap_or_else(|| panic!("long-solo-play creates {id:?}"));
    ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
        .expect("the recorded 0xF745 decodes")
}

fn place(create: &mut ItemCreateObject, at: Position) {
    create.0.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
        objcell_id: at.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: at.frame.origin.x,
                y: at.frame.origin.y,
                z: at.frame.origin.z,
            },
            orientation: dereth_protocol::types::Quat {
                w: at.frame.rotation.w,
                x: at.frame.rotation.x,
                y: at.frame.rotation.y,
                z: at.frame.rotation.z,
            },
        },
    });
    create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
}

// ---------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        assert!(app.frame(), "the application keeps running");
    }
}

fn scene(app: &App) -> &WorldState {
    app.world_state().expect("a loaded scene")
}

fn simulation(app: &App) -> &SimPresentation {
    app.presentation()
        .as_any()
        .downcast_ref()
        .expect("simulated world")
}

fn movement_failures(app: &App) -> u64 {
    let sim = simulation(app);
    sim.objects.steps.remote_move_tos_failed + sim.steps.steps.remote_move_tos_failed
}

fn body_position(app: &App) -> Position {
    scene(app)
        .character
        .as_ref()
        .expect("a local body")
        .position()
}

fn feed(app: &mut App, opcode: Opcode, body: Vec<u8>, now: f64) {
    app.objects_mut()
        .apply_event(&SessionEvent::WorldObject { opcode, body }, LocalTime(now));
}

/// The App of `selection/selection_blink.rs`: UI shell up, real terrain, real
/// physics, and the local body wearing **long-solo-play's** recorded identity.
fn app_with_recorded_body() -> (App, Vec<CorpusBlob>) {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let mut app = crate::common::sim_app::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 640,
        height: 480,
        dat_dir: client_dir(),
        preferences_file: std::env::temp_dir()
            .join("dere-remote-move-to-not-created")
            .join("preferences.ini"),
        ..Config::default()
    })
    .expect("a headless App on a software GPU device and the retail dats");
    app.start_shell().expect("the UI shell comes up");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..SceneConfig::default()
    })
    .expect("real terrain and a real physics world");
    frames(&mut app, 60);

    let rows = long_solo_play();
    // The corpus login edge: the body adopts the recorded character's id and its own create.
    let mut create = recorded_create(&rows, PLAYER);
    place(&mut create, body_position(&app));
    app.objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(PLAYER), LocalTime(1.0));
    feed(
        &mut app,
        Opcode::ITEM_CREATE_OBJECT,
        dereth_protocol::write_body(&create).expect("re-encodes"),
        1.0,
    );
    frames(&mut app, 90);
    {
        let c = scene(&app).character.as_ref().expect("a local body");
        assert!(c.on_ground(), "real terrain must support the recorded body");
        assert_eq!(
            c.object_id(),
            PLAYER,
            "the body adopted long-solo-play's recorded id"
        );
    }
    (app, rows)
}

/// Create the recorded golem `metres` away from the body, on the +x axis of the block, and let
/// `ObjectStream::sync_physics` spawn its collision body.
fn place_golem(app: &mut App, rows: &[CorpusBlob], metres: f32) -> Position {
    let here = body_position(app);
    let at = Position::new(
        here.cell,
        dereth_primitives::Frame::new(
            Vec3::new(
                here.frame.origin.x + metres,
                here.frame.origin.y,
                here.frame.origin.z,
            ),
            here.frame.rotation,
        ),
    );
    let mut create = recorded_create(rows, GOLEM);
    place(&mut create, at);
    feed(
        app,
        Opcode::ITEM_CREATE_OBJECT,
        dereth_protocol::write_body(&create).expect("re-encodes"),
        2.0,
    );
    frames(app, 30);
    assert!(
        scene(app).server_object_position(GOLEM).is_some(),
        "the recorded golem is in the scene"
    );
    at
}

/// long-solo-play's recorded `MoveToObject` buffer, with only its packed fall-through origin re-pointed
/// at where the body actually stands and its movement timestamp advanced.
///
/// The origin is the arm that `unpack_movement`'s `case 6` takes **when world-object lookup misses**; the
/// player is in the object table here, so nothing reads it. It is re-pointed anyway so that a
/// regression that took the fall-through cannot pass by walking to a dungeon coordinate.
fn recorded_approach(rows: &[CorpusBlob], at: Position, stamp: u16) -> (Vec<u8>, RtParams) {
    let row = rows
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0
                && r.t_rel_micros == APPROACH_MICROS
        })
        .expect("long-solo-play's recorded approach at t=91.500548");
    assert_eq!(
        subject(row),
        GOLEM,
        "the recorded approach is addressed to the golem"
    );
    let mut msg =
        MovementSetObjectMovement::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
            .expect("the recorded 0xF74C decodes");
    let mut buf = msg.decoded_movement().expect("its movement buffer decodes");
    let Some(MoveToArm::MoveToObject {
        target,
        mut origin,
        params,
        run_rate,
    }) = buf.body.decode_move_to().expect("the arm decodes")
    else {
        panic!("t=91.500548 is a MoveToObject");
    };
    assert_eq!(
        target, PLAYER,
        "the recorded approach names long-solo-play's own character"
    );
    origin.objcell_id = at.cell.0;
    origin.origin = dereth_protocol::types::Vec3 {
        x: at.frame.origin.x,
        y: at.frame.origin.y,
        z: at.frame.origin.z,
    };
    let runtime = runtime_params(&params);
    buf.movement_timestamp = stamp;
    buf.body.unhandled = MovementBody::encode_move_to(&MoveToArm::MoveToObject {
        target,
        origin,
        params,
        run_rate,
    });
    msg.movement = MovementSetObjectMovement::encode_movement(&buf).expect("re-encodes");
    (
        dereth_protocol::write_body(&msg).expect("re-encodes"),
        runtime,
    )
}

/// The wire parameters as the movement runtime reads them. `wire_to_params` is private to
/// `world.rs`, so the four fields this file's expectations need are lifted here in the same wire
/// field order.
fn runtime_params(p: &dereth_protocol::movement::MovementParameters) -> RtParams {
    match *p {
        dereth_protocol::movement::MovementParameters::MoveTo {
            bitfield,
            distance_to_object,
            min_distance,
            fail_distance,
            speed,
            walk_run_threshold,
            desired_heading,
        } => RtParams {
            flags: bitfield,
            distance_to_object,
            min_distance,
            fail_distance,
            speed,
            walk_run_threshold: walk_run_threshold,
            desired_heading,
            ..RtParams::default()
        },
        dereth_protocol::movement::MovementParameters::TurnTo {
            bitfield,
            speed,
            desired_heading,
        } => RtParams {
            flags: bitfield,
            speed,
            desired_heading,
            ..RtParams::default()
        },
    }
}

// ---------------------------------------------------------------------------------------------
// One frame of the machine, as retail's own fields
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Sample {
    frame: usize,
    moving: bool,
    kind: MovementType,
    top: ObjectId,
    command: MotionCommand,
    aux: MotionCommand,
    nodes: usize,
    initialized: bool,
    node: Option<(MovementType, f32)>,
    aim: Option<(Position, bool, f32, f32)>,
    target: Option<ObjectId>,
    quantum: Option<f32>,
    sticky: Option<ObjectId>,
    position: Position,
    heading: f32,
    player: Position,
    /// The current move-to distance, re-derived here from the manager's four inputs. This is the
    /// value the position-node arrival rule compares against `distance_to_object`.
    /// `None` while no move-to is in flight, because `current_target_position` is then cleared.
    distance: Option<f32>,
    /// The plain body-to-body separation, so an arrival can never be proved by a cylinder
    /// convention alone.
    separation: f32,
    failures: u64,
}

impl Sample {
    fn read(app: &App, frame: usize, radius: f32, height: f32) -> Self {
        let s = scene(app);
        let (moving, kind, top) = s.server_object_move_to(GOLEM).expect("a live golem");
        let (command, aux, nodes, initialized) = s
            .server_object_move_to_state(GOLEM)
            .expect("its move-to state");
        let position = s.server_object_position(GOLEM).expect("its position");
        let player = body_position(app);
        let aim = s.server_object_move_to_aim(GOLEM);
        Self {
            frame,
            moving,
            kind,
            top,
            command,
            aux,
            nodes,
            initialized,
            node: s.server_object_move_to_node(GOLEM),
            aim,
            target: s.server_object_target(GOLEM),
            quantum: s.server_object_target_quantum(GOLEM),
            sticky: s.server_object_sticky(GOLEM),
            position,
            heading: dereth_animation::frame::get_heading(&position.frame),
            player,
            // The recorded bitfield sets `use_spheres` (`0x400`). The moving body's radius and
            // height come from its DAT part array, and the target cylinder comes from the player.
            distance: aim
                .map(|(at, _, r2, h2)| cylinder_distance(radius, height, &position, r2, h2, &at)),
            separation: dereth_animation::motion::moveto::distance(&position, &player),
            failures: movement_failures(app),
        }
    }

    fn idle(&self) {
        assert!(!self.moving, "{self:?}");
        assert_eq!(self.kind, MovementType::Invalid, "{self:?}");
        assert_eq!(self.top, ObjectId(0), "{self:?}");
        assert_eq!(
            (self.command, self.aux, self.nodes),
            (MotionCommand::NONE, MotionCommand::NONE, 0),
            "{self:?}"
        );
    }
}

fn wrap(h: f32) -> f32 {
    let mut h = h % 360.0;
    if h < 0.0 {
        h += 360.0;
    }
    h
}

fn heading_error(a: f32, b: f32) -> f32 {
    ((a - b + 180.0).rem_euclid(360.0) - 180.0).abs()
}

// ---------------------------------------------------------------------------------------------
// The acceptance
// ---------------------------------------------------------------------------------------------

/// Behaviour: movement.remote.a-creature-told-to-approach-walks-up-turns-and-stops
#[test]
fn a_recorded_move_to_object_walks_the_creature_up_turns_it_to_you_and_stops() {
    let (mut app, rows) = app_with_recorded_body();
    let start = place_golem(&mut app, &rows, 8.0);
    let (radius, height) = {
        let s = scene(&app);
        let c = s.character.as_ref().expect("a body");
        let h = c
            .world
            .by_object_id(GOLEM)
            .expect("the golem has a collision body");
        let b = c.world.get(h).expect("a live body");
        (b.radius(), b.height())
    };
    assert!(
        radius > 0.0 && height > 0.0,
        "DAT dimensions: {radius} {height}"
    );

    let (bytes, params) = recorded_approach(&rows, body_position(&app), 1);
    // The recording's own parameters, so a change in `wire_to_params` or in the capture reddens
    // here rather than silently re-aiming every expectation below.
    assert_eq!(params.flags, 0x1EFFF, "the recorded bitfield");
    assert!(
        params.has(flags::USE_FINAL_HEADING),
        "the recorded arm turns to you at the end"
    );
    assert!(
        params.has(flags::STICKY),
        "and hands off to position-manager attachment"
    );
    assert!(
        params.has(flags::USE_SPHERES),
        "and measures with cylinder_distance"
    );
    assert_eq!(params.distance_to_object, 0.6);

    let before = Sample::read(&app, 0, radius, height);
    before.idle();
    assert!(
        before.separation > 6.0,
        "the golem starts well outside its arrival radius: {before:?}"
    );

    feed(&mut app, Opcode::MOVEMENT_SET_OBJECT_MOVEMENT, bytes, 3.0);
    frames(&mut app, 1);

    let mut samples = vec![Sample::read(&app, 0, radius, height)];
    for frame in 1..=600 {
        frames(&mut app, 1);
        let s = Sample::read(&app, frame, radius, height);
        let done = !s.moving && s.sticky.is_none() && frame > 30;
        samples.push(s);
        if done {
            break;
        }
    }

    let last = *samples.last().expect("samples");
    let report = || {
        let mut out = String::new();
        for s in samples.iter().step_by(5) {
            out.push_str(&format!(
                "f{:>3} sep={:>7.3} d={:>8} h={:>7.2} cmd={:?} aux={:?} nodes={} node={:?} tgt={:?} sticky={:?}\n",
                s.frame,
                s.separation,
                s.distance.map_or_else(|| "-".to_string(), |d| format!("{d:.3}")),
                s.heading,
                s.command,
                s.aux,
                s.nodes,
                s.node,
                s.target,
                s.sticky
            ));
        }
        out
    };

    // ---- 1. the approach reaches the machine at all -----------------------------------------
    assert_eq!(
        simulation(&app).objects.steps.remote_move_tos_performed,
        1,
        "exactly the recorded buffer reached current movement execution"
    );
    let armed = samples
        .iter()
        .find(|s| s.moving)
        .unwrap_or_else(|| panic!("the recorded approach starts a move-to\n{}", report()));
    assert_eq!(
        (armed.kind, armed.top),
        (MovementType::MoveToObject, PLAYER),
        "the arm and its top-level target"
    );
    let planned = samples
        .iter()
        .find(|s| s.initialized)
        .unwrap_or_else(|| panic!("the first TargetInfo builds the plan\n{}", report()));
    assert_eq!(
        planned.target,
        Some(PLAYER),
        "the physics object's target setter named the player"
    );
    // Planning pushes an opening turn and the walk node, then appends a third turn node when
    // `use_final_heading` is set.
    assert_eq!(
        planned.nodes,
        3,
        "turn, walk, and the final heading\n{}",
        report()
    );

    // ---- 2. the position closes on the target ------------------------------------------------
    assert!(
        before.separation - last.separation > 5.0,
        "the creature must physically close on you: {:.3} -> {:.3}\n{}",
        before.separation,
        last.separation,
        report()
    );
    assert!(
        dereth_animation::motion::moveto::distance(&start, &last.position) > 5.0,
        "and it must be the body that moved, not the measurement"
    );
    // The separation never grows while the walk node is running: `get_command` was given
    // `move_towards` and this station's target does not move.
    for w in samples.windows(2) {
        let (a, b) = (w[0], w[1]);
        if matches!(a.node, Some((MovementType::MoveToPosition, _)))
            && a.command != MotionCommand::NONE
        {
            assert!(
                b.separation <= a.separation + 1e-3,
                "f{}: the approach must not retreat, {:.4} -> {:.4}\n{}",
                b.frame,
                a.separation,
                b.separation,
                report()
            );
        }
    }
    // Read the position-node arrival test at the first frame after the walk node popped -- NOT at
    // the end of the run, where the sticky hand-off has already pulled the body out to the
    // attachment manager's own stand-off, and not at the frame before, because movement time is
    // advanced once per **physics sub-step** and the closing sub-step is
    // inside the frame whose end this samples. The manager stops the body when it pops, and
    // the target aim does not move, so the distance read here is the distance the pop used.
    let arrived_at = samples
        .windows(2)
        .find(|w| {
            matches!(w[0].node, Some((MovementType::MoveToPosition, _)))
                && !matches!(w[1].node, Some((MovementType::MoveToPosition, _)))
        })
        .map(|w| w[1])
        .unwrap_or_else(|| panic!("the walk node completes\n{}", report()));
    let d = arrived_at
        .distance
        .expect("a move-to still in flight carries an aim");
    assert!(
        d <= params.distance_to_object + 1e-3,
        "arrival is `d <= distance_to_object`: {d:.4} > {} at f{}\n{}",
        params.distance_to_object,
        arrived_at.frame,
        report()
    );
    let arrival = arrived_at;

    // ---- 3. the heading follows the original half-plane rule, not "monotonic" ----------------
    //
    // A turn node pops on the FIRST frame at which
    // `heading_greater(get_heading(), head->heading, current_command)` is true, and snaps the
    // body exactly onto `head->heading` when it does. So over the sampled sequence:
    //
    //   * while a turn node is head and a turn command is in flight, the half-plane must NOT be
    //     satisfied -- if it were, the node would already have been popped in that same substep;
    //   * on the frame the node leaves, the published heading must BE the node's heading.
    //
    // Neither statement is monotonicity: the sampled heading may move either way between frames
    // and may pass the target within one frame, and both are allowed here.
    let mut turn_nodes_completed = 0;
    let mut snaps = 0;
    for w in samples.windows(2) {
        let (a, b) = (w[0], w[1]);
        if let (Some((MovementType::TurnToHeading, target)), true) = (
            a.node,
            a.command == MotionCommand::TURN_LEFT || a.command == MotionCommand::TURN_RIGHT,
        ) {
            assert!(
                !heading_greater(a.heading, target, a.command),
                "f{}: heading {:.4} has already passed {:.4} turning {:?} and the node is still \
                 head -- an active turn node must pop on the first frame it has\n{}",
                a.frame,
                a.heading,
                target,
                a.command,
                report()
            );
            if b.node != a.node {
                turn_nodes_completed += 1;
                assert!(
                    heading_error(b.heading, target) < 1e-2,
                    "f{}: a completed turn must snap the body onto {:.4}, got {:.4}\n{}",
                    b.frame,
                    target,
                    b.heading,
                    report()
                );
                snaps += 1;
            }
        }
        // The initial turn direction is left exactly when the right-handed difference exceeds
        // 180 degrees.
        if a.command == MotionCommand::NONE {
            if let (Some((MovementType::TurnToHeading, target)), true) = (
                b.node,
                b.command == MotionCommand::TURN_LEFT || b.command == MotionCommand::TURN_RIGHT,
            ) {
                let diff = heading_diff(target, a.heading, MotionCommand::TURN_RIGHT);
                let want = if diff > 180.0 {
                    MotionCommand::TURN_LEFT
                } else {
                    MotionCommand::TURN_RIGHT
                };
                assert_eq!(
                    b.command,
                    want,
                    "f{}: turning from {:.3} to {:.3} is a {want:?} (diff {diff:.3})\n{}",
                    b.frame,
                    a.heading,
                    target,
                    report()
                );
            }
        }
    }
    assert!(
        turn_nodes_completed >= 1,
        "the plan's opening turn must run and complete: {turn_nodes_completed}\n{}",
        report()
    );
    assert_eq!(snaps, turn_nodes_completed, "every completed turn snapped");
    // The opening turn is a real turn: it must have held a turn command for several frames and
    // moved the body's heading by more than the 0.0002 heading-comparison epsilon.
    let turning: Vec<&Sample> = samples
        .iter()
        .filter(|s| {
            matches!(s.node, Some((MovementType::TurnToHeading, _)))
                && (s.command == MotionCommand::TURN_LEFT || s.command == MotionCommand::TURN_RIGHT)
        })
        .collect();
    assert!(
        turning.len() >= 5,
        "the opening turn takes frames: {}\n{}",
        turning.len(),
        report()
    );
    assert!(
        heading_error(turning[0].heading, turning[turning.len() - 1].heading) > 1.0,
        "and it actually turns the body\n{}",
        report()
    );

    // The position node's corrective turn, re-derived from the same three fields the original
    // client reads. `aux_command` is the ONLY thing the dead band controls, so this is the whole rule.
    let mut walk_samples = 0;
    for w in samples.windows(2) {
        let (a, b) = (w[0], w[1]);
        let Some((MovementType::MoveToPosition, _)) = a.node else {
            continue;
        };
        if !a.moving || a.command == MotionCommand::NONE {
            continue;
        }
        let Some((aim, away, _, _)) = a.aim else {
            continue;
        };
        let want = wrap(
            position_heading(&a.position, &aim) + RtParams::get_desired_heading(a.command, away),
        );
        let diff = heading_diff(want, a.heading, MotionCommand::TURN_RIGHT);
        let expected = if diff <= DEAD_BAND || diff >= 360.0 - DEAD_BAND {
            MotionCommand::NONE
        } else if diff >= 180.0 {
            MotionCommand::TURN_LEFT
        } else {
            MotionCommand::TURN_RIGHT
        };
        // The command is set on the frame the decision is taken, so it is `b` that carries it
        // when the decision changed and `a` when it did not.
        assert!(
            b.aux == expected || a.aux == expected,
            "f{}: bearing {want:.3} against heading {:.3} is diff {diff:.3}, so the corrective \
             turn is {expected:?} (dead band {DEAD_BAND}/{}) -- got {:?} then {:?}\n{}",
            a.frame,
            a.heading,
            360.0 - DEAD_BAND,
            a.aux,
            b.aux,
            report()
        );
        walk_samples += 1;
    }
    assert!(
        walk_samples > 20,
        "the walk node must actually run: {walk_samples}\n{}",
        report()
    );

    // ---- 4. it turns to you --------------------------------------------------------------
    //
    // The move-to-object plan's `use_final_heading` node is `bearing_to_interpolated_target +
    // desired_heading`, composed at plan time. With `desired_heading == 0` that is the bearing to
    // the player, so a creature that has arrived is facing them.
    assert_eq!(
        params.desired_heading, 0.0,
        "the recorded arm asks for the plain bearing"
    );
    // The plan's FIRST node turns to the bearing. Its `use_final_heading` tail appends
    // `bearing + desired_heading`, wrapped once at 360 -- the same bearing, composed twice.
    // Both are read off the queue here.
    let (opening_kind, bearing) = planned.node.expect("the plan has a head");
    assert_eq!(
        opening_kind,
        MovementType::TurnToHeading,
        "the plan opens with a turn"
    );
    let (final_kind, final_heading) = samples
        .iter()
        .find(|s| s.frame > arrival.frame && s.nodes == 1)
        .and_then(|s| s.node)
        .unwrap_or_else(|| panic!("the final-heading node reaches the head\n{}", report()));
    assert_eq!(final_kind, MovementType::TurnToHeading);
    assert!(
        heading_error(final_heading, wrap(bearing + params.desired_heading)) < 1e-3,
        "the final heading is `bearing + desired_heading`: {final_heading} vs {bearing} + {}",
        params.desired_heading
    );
    let facing = position_heading(&last.position, &last.player);
    assert!(
        heading_error(last.heading, facing) < 15.0,
        "the arrived creature faces you: heading {:.3}, bearing {:.3}\n{}",
        last.heading,
        facing,
        report()
    );

    // ---- 5. it stops, and cleanup puts everything back ---------------------------------------
    last.idle();
    assert_eq!(
        (last.target, last.sticky),
        (None, None),
        "cleanup clears the target and the sticky hand-off expires separately\n{}",
        report()
    );
    assert_eq!(
        last.quantum, None,
        "no target subscription survives cleanup"
    );
    assert_eq!(
        last.failures,
        0,
        "no move-to failure was reported\n{}",
        report()
    );
    assert_eq!(
        scene(&app).server_object_motions_pending(GOLEM),
        Some(0),
        "no animation link is left outstanding"
    );

    // ---- 6. and it stays stopped -------------------------------------------------------------
    let stopped = last.position;
    frames(&mut app, 60);
    let after = Sample::read(&app, 999, radius, height);
    after.idle();
    assert!(
        dereth_animation::motion::moveto::distance(&stopped, &after.position) < 1e-3,
        "a finished approach must not drift: {:?} -> {:?}",
        stopped,
        after.position
    );
}

// ---------------------------------------------------------------------------------------------
// The target quantum's one-second gate
// ---------------------------------------------------------------------------------------------

/// The position node updates target quantum only when it has a nonzero top-level object, a valid
/// movement type, and a closing speed greater than 0.1. It computes `eta = d / |v|`, compares that
/// estimate with the existing quantum, and writes a new value only when the absolute difference
/// exceeds 1.0.
///
/// The absolute-difference comparison against 1.0 is a hysteresis band: the original client rewrites the extrapolation
/// lead only when the estimated time of arrival has moved by more than a second, so a target
/// subscription's quantum is a *stepped* quantity that holds for long stretches of an approach.
/// A build that writes `d / |v|` every frame has a quantum that changes on every frame in which
/// the closing speed changes at all, which is every frame of a real walk cycle.
#[test]
fn the_target_quantum_moves_only_when_the_eta_moves_by_more_than_a_second() {
    let (mut app, rows) = app_with_recorded_body();
    place_golem(&mut app, &rows, 12.0);
    let (bytes, _) = recorded_approach(&rows, body_position(&app), 1);
    feed(&mut app, Opcode::MOVEMENT_SET_OBJECT_MOVEMENT, bytes, 3.0);

    let mut changes = 0_usize;
    let mut illegal: Vec<(usize, f32, f32)> = Vec::new();
    // The positive control: frames on which the position node's quantum-update guards all held,
    // so that a run in which the node never had a real closing speed reports absence
    // instead of a clean bill of health.
    let mut reached = 0_usize;
    let mut previous: Option<f32> = None;
    for frame in 0..=600 {
        frames(&mut app, 1);
        let s = scene(&app);
        let (moving, _, top) = s.server_object_move_to(GOLEM).expect("a live golem");
        let walking = matches!(
            s.server_object_move_to_node(GOLEM),
            Some((MovementType::MoveToPosition, _))
        );
        let speed = s
            .character
            .as_ref()
            .and_then(|c| c.world.by_object_id(GOLEM))
            .and_then(|h| s.character.as_ref().and_then(|c| c.world.get(h)))
            .map_or(0.0, |b| {
                let v = b.cached_velocity;
                (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
            });
        if moving && top != ObjectId(0) && walking && speed > 0.1 {
            reached += 1;
        }
        // The sticky hand-off installs a **new** subscription with
        // `set_target(0, id, 0.5, 0.5)`, so the 0.5 it writes is a `set_target` and not a
        // `set_target_quantum`. The hand-off's own subscription is therefore not this test's
        // subject and the window closes at it.
        let q = match (
            s.server_object_target_quantum(GOLEM),
            s.server_object_sticky(GOLEM),
        ) {
            (Some(q), None) => q,
            _ => {
                previous = None;
                continue;
            }
        };
        if let Some(p) = previous {
            if (q - p).abs() > f32::EPSILON {
                changes += 1;
                // The original rule can only have written this if the new ETA was more than a second from
                // the value it replaced.
                if (q - p).abs() <= 1.0 {
                    illegal.push((frame, p, q));
                }
            }
        }
        previous = Some(q);
        if !moving && frame > 60 {
            break;
        }
    }
    assert!(
        reached > 20,
        "the approach must reach the quantum-update guards with a real closing speed: \
         {reached} qualifying frames"
    );
    // The block must actually run. The original closing-speed input is the body's cached velocity;
    // using the separate movement velocity instead measures exactly 0.0 m/s for this walking
    // creature, never clears `|v| > 0.1`, and writes the quantum
    // **zero** times over the whole approach while `reached` above still counts every frame.
    assert!(
        changes > 0,
        "the quantum was never written over {reached} frames of real closing speed -- the \
         quantum-update path must use the cached closing velocity"
    );
    assert!(
        illegal.is_empty(),
        "target-quantum writes are guarded by |eta - quantum| > 1.0; {} of {} \
         writes moved it by less: {:?}",
        illegal.len(),
        changes,
        &illegal[..illegal.len().min(8)]
    );
}

// ---------------------------------------------------------------------------------------------
// The fellowship arms: a remote player turning to a player the session's own 0xF658 never listed
// ---------------------------------------------------------------------------------------------

/// The three `fellowship-*` recordings carry traffic that reaches `move_to_target` with an id the
/// session's own `0xF658` never listed: a remote player turns to face a **different** remote
/// player — `0x50000020 -> 0x5000001F`, `0x50000020 -> 0x5000001E`, `0x5000001E -> 0x50000020`,
/// `0x5000001E -> 0x5000001F` — where the other recordings' turn arms all name the session's own
/// character.
///
/// Movement decoding's `case 8` resolves the target id through the world-object table and
/// **falls through to `case 9`'s `TurnToHeading`** when lookup misses. A resolver that knows only
/// the local player therefore turns the creature to a fixed heading instead of toward the remote
/// person. This drives the recorded arm through `App` with both ends present as ordinary remote
/// objects and reads the answer off the manager.
///
/// The planned heading is the bearing from the turner to the target plus the heading stored in
/// the manager's sought-position frame, with the result wrapped at 360 degrees. The recorded
/// `desired_heading` is zero, but the target update overwrites the current-target frame and the
/// turn planner reads the separate sought-position frame.
///
/// Nothing writes a heading onto the sought position before this runs, so its heading term is 0.
/// The target offset is nondegenerate, and the node's heading is therefore exactly the bearing:
/// the turner ends up facing the other player.
///
/// **The bench is set so the turn has work to do.** Both bodies are given explicit headings, and
/// the one the arm names is placed 90 degrees off the turner's. Without that the node pops on the
/// frame it is pushed because the heading difference is not greater than 0.0002, and the whole run
/// would be green over a machine that never turned anything.
#[test]
fn a_recorded_fellowship_turn_to_object_faces_a_player_the_session_never_listed() {
    const TURNER: ObjectId = ObjectId(0x5000_001e);
    const FACED: ObjectId = ObjectId(0x5000_001f);

    let (mut app, _) = app_with_recorded_body();
    let fellowship = Corpus::load("fellowship-one-vassal")
        .expect("the locked corpus decodes")
        .expect("fellowship is this test's oracle");

    let here = body_position(&app);
    let at = |dx: f32, dy: f32, heading: f32| {
        let mut f = dereth_primitives::Frame::new(
            Vec3::new(
                here.frame.origin.x + dx,
                here.frame.origin.y + dy,
                here.frame.origin.z,
            ),
            dereth_primitives::Quat::IDENTITY,
        );
        dereth_animation::frame::set_heading(&mut f, heading);
        Position::new(here.cell, f)
    };
    // The turner faces due north; the player it is told to face is due east of it, so the plan is
    // a 90-degree turn.
    for (id, p) in [(TURNER, at(4.0, 0.0, 0.0)), (FACED, at(10.0, 0.0, 0.0))] {
        let mut create = recorded_create(&fellowship.blobs, id);
        place(&mut create, p);
        feed(
            &mut app,
            Opcode::ITEM_CREATE_OBJECT,
            dereth_protocol::write_body(&create).expect("re-encodes"),
            2.0,
        );
    }
    frames(&mut app, 30);
    // Every recorded login's `0xF745` arrives HIDDEN; `objects/hidden_state.rs` names the state
    // word that closes it, and `selection/selection_blink.rs` sends the same one. Without it the bodies
    // are in the scene and drawn but never stepped, so the turn issues its command and the
    // heading never moves.
    for id in [TURNER, FACED] {
        let state_ts = app
            .objects()
            .presence(id)
            .expect("a recorded presence")
            .state_ts;
        feed(
            &mut app,
            dereth_protocol::objects::ItemSetState::OPCODE,
            dereth_protocol::write_body(&dereth_protocol::objects::ItemSetState {
                id,
                state: TELEPORT_UNHIDE_STATE,
                timestamps: dereth_protocol::types::PhysicsEventStamp {
                    instance: 0,
                    event: state_ts.wrapping_add(1),
                },
            })
            .expect("encodes"),
            2.5,
        );
    }
    frames(&mut app, 30);
    let turner_pos = scene(&app)
        .server_object_position(TURNER)
        .expect("the turner is in scene");
    let faced_pos = scene(&app)
        .server_object_position(FACED)
        .expect("so is the one it must face");
    assert_ne!(
        scene(&app).character.as_ref().expect("a body").object_id(),
        FACED,
        "the faced player must NOT be the local body, or the stranger path is not exercised"
    );
    let start = dereth_animation::frame::get_heading(&turner_pos.frame);
    let bearing0 = position_heading(&turner_pos, &faced_pos);
    assert!(
        heading_error(start, bearing0) > 45.0,
        "the bench must require a real turn: heading {start:.3}, bearing {bearing0:.3}"
    );

    // The recorded arm, unchanged but for its movement timestamp.
    let row = fellowship
        .blobs
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0
                && subject(r) == TURNER
                && matches!(
                    MovementSetObjectMovement::read(&mut dereth_protocol::Reader::new(&r.payload[4..]))
                        .ok()
                        .and_then(|m| m.decoded_movement().ok())
                        .and_then(|b| b.body.decode_move_to().ok().flatten()),
                    Some(MoveToArm::TurnToObject { target, .. }) if target == FACED
                )
        })
        .expect("fellowship carries 0x5000001E turning to 0x5000001F");
    let mut msg =
        MovementSetObjectMovement::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
            .expect("the recorded 0xF74C decodes");
    let mut buf = msg.decoded_movement().expect("its buffer decodes");
    let Some(MoveToArm::TurnToObject {
        target,
        desired_heading,
        params,
    }) = buf.body.decode_move_to().expect("the arm decodes")
    else {
        unreachable!("selected above")
    };
    assert_eq!(target, FACED);
    assert_eq!(
        desired_heading, 0.0,
        "the recorded arm asks for the plain bearing"
    );
    assert_eq!(
        runtime_params(&params).flags,
        0x1EE0F,
        "the recorded TurnTo bitfield matches the movement-parameter defaults"
    );
    buf.movement_timestamp = 1;
    msg.movement = MovementSetObjectMovement::encode_movement(&buf).expect("re-encodes");
    feed(
        &mut app,
        Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
        dereth_protocol::write_body(&msg).expect("re-encodes"),
        3.0,
    );
    frames(&mut app, 1);

    // `case 8` resolved the stranger rather than falling through to `case 9`.
    let st = &app.objects().stats;
    let (moving, kind, top) = scene(&app)
        .server_object_move_to(TURNER)
        .expect("a live turner");
    assert!(
        moving,
        "the recorded arm started a move-to (updates {} stale {} old_control {} performed {})",
        st.movement_updates,
        st.movement_stale,
        st.movement_old_control,
        simulation(&app).objects.steps.remote_move_tos_performed
    );
    assert_eq!(
        (kind, top),
        (MovementType::TurnToObject, FACED),
        "world-object lookup resolved the remote target; a fall-through would read \
         (TurnToHeading, 0x0)"
    );
    assert_eq!(scene(&app).server_object_target(TURNER), Some(FACED));

    let mut turned = false;
    let mut node_heading = None;
    for _ in 0..600 {
        frames(&mut app, 1);
        let s = scene(&app);
        if let Some((MovementType::TurnToHeading, h)) = s.server_object_move_to_node(TURNER) {
            node_heading = Some(h);
        }
        let (cmd, _, _, _) = s.server_object_move_to_state(TURNER).expect("state");
        turned |= cmd == MotionCommand::TURN_LEFT || cmd == MotionCommand::TURN_RIGHT;
        if !s.server_object_move_to(TURNER).expect("state").0 && turned {
            break;
        }
    }
    assert!(
        turned,
        "the turn must issue a turn command, not merely be recorded"
    );

    let s = scene(&app);
    let pos = s.server_object_position(TURNER).expect("placed");
    let faced_pos = s.server_object_position(FACED).expect("placed");
    let bearing = position_heading(&pos, &faced_pos);
    let node = node_heading.expect("a TurnToHeading node reached the head");
    assert!(
        heading_error(node, bearing) < 1e-2,
        "the turn-to-object node is the bearing: node {node:.4}, bearing \
         {bearing:.4}"
    );
    let now = dereth_animation::frame::get_heading(&pos.frame);
    assert!(
        heading_error(now, bearing) < 1e-2,
        "the turner ends facing the player it was told to face: {now:.4} vs {bearing:.4} \
         (started at {start:.4})"
    );
    assert!(
        heading_error(start, now) > 45.0,
        "and it really turned: {start:.3} -> {now:.3}"
    );

    // After the completed turn leaves the queue empty, cleanup runs. `TurnToObject` carries no
    // `sticky` bit, so this arm ends in the plain stop.
    let (moving, kind, top) = s.server_object_move_to(TURNER).expect("state");
    let (cmd, aux, nodes, _) = s.server_object_move_to_state(TURNER).expect("state");
    assert!(!moving);
    assert_eq!((kind, top), (MovementType::Invalid, ObjectId(0)));
    assert_eq!(
        (cmd, aux, nodes),
        (MotionCommand::NONE, MotionCommand::NONE, 0)
    );
    assert_eq!(
        s.server_object_target(TURNER),
        None,
        "cleanup cleared the subscription"
    );
    assert_eq!(s.server_object_sticky(TURNER), None);
    assert_eq!(movement_failures(&app), 0);
}

// ---------------------------------------------------------------------------------------------
// A style-zero header, and the standing-long-jump bit, on the remote arm
// ---------------------------------------------------------------------------------------------

/// **The style-zero header.**
///
/// The fellowship recordings carry remote `case 0` buffers whose header style word is `0` and
/// whose interpreted state carries no style at all. `command_ids[0]` is `0x80000000` and
/// style-flagged, so the original pre-switch motion attempt uses it rather than treating it as a
/// sentinel to skip.
///
/// The original decoder indexes the command table with the bare zero and attempts command
/// `0x80000000`, so the buffer is **not** skipped. Motion interpretation updates the interpreted
/// state only when the motion table accepts that command; a refusal bypasses both queue insertion
/// and state application.
///
/// No motion table carries a `0x80000000` style, so that attempt is refused. `case 0` then installs
/// the buffer's own unpacked state, and an interpreted state that carries no style
/// on the wire is left at its constructor's `NonCombat`. **So a style-zero header ends the object
/// in `NonCombat`, not in `Invalid`**, and the observable difference between "acted on" and
/// "skipped" is not the stance at all.
///
/// The test therefore puts the creature in a *different* stance first with a recorded style-61
/// buffer, so that "ends in NonCombat" is a change this test can watch happen, and asserts the
/// buffer was processed (its forward command landed) rather than dropped.
#[test]
fn a_style_zero_header_is_not_a_sentinel_and_leaves_the_stance_where_case_zero_puts_it() {
    let (mut app, rows) = app_with_recorded_body();
    place_golem(&mut app, &rows, 6.0);
    assert_eq!(
        MotionCommand::from_index(0),
        Some(MotionCommand::INVALID),
        "command_ids[0] is 0x80000000 -- the buffer's header is a real index, not an absence"
    );
    assert!(MotionCommand::INVALID.is_style(), "and it is style-flagged");

    // A recorded remote `case 0` buffer that DOES carry a style, and one whose header style is 0.
    let mut styled = None;
    let mut zero = None;
    'outer: for name in [
        "fellowship-one-vassal",
        "fellowship-two-monarch",
        "fellowship-three-vassal",
    ] {
        let Some(c) = Corpus::load(name).expect("the locked corpus decodes") else {
            continue;
        };
        for row in c.blobs.iter().filter(|r| {
            r.dir == Direction::ServerToClient && r.opcode == Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0
        }) {
            let Ok(m) = MovementSetObjectMovement::read(&mut dereth_protocol::Reader::new(
                &row.payload[4..],
            )) else {
                continue;
            };
            let Ok(buf) = m.decoded_movement() else {
                continue;
            };
            let Some(state) = buf.body.interpreted.as_ref() else {
                continue;
            };
            if buf.body.current_style == 0 && state.current_style.is_none() {
                if zero.is_none() {
                    zero = Some((m, buf));
                }
            } else if styled.is_none()
                && MotionCommand::from_index(buf.body.current_style)
                    .is_some_and(|s| s != MotionCommand::NON_COMBAT)
            {
                styled = Some((m, buf));
            }
            if zero.is_some() && styled.is_some() {
                break 'outer;
            }
        }
    }
    let (mut styled_msg, mut styled_buf) = styled
        .expect("the corpus carries a remote case-0 buffer whose style word is not NonCombat");
    let (mut zero_msg, mut zero_buf) =
        zero.expect("and one whose header style is 0 with no style in its state");

    let send = |app: &mut App,
                msg: &mut MovementSetObjectMovement,
                buf: &mut dereth_protocol::movement::MovementBuffer,
                stamp: u16,
                t: f64| {
        msg.id = GOLEM;
        buf.movement_timestamp = stamp;
        // The two buffers come from different objects in the recording, so their server-control
        // stamps are unrelated; the delivery gate compares each stamp against the receiving
        // object's stamp, and this test's subject is one object. Levelled so that the
        // gate under test is the one in `unpack_movement`, not the delivery order.
        buf.server_control_timestamp = 0;
        msg.movement = MovementSetObjectMovement::encode_movement(buf).expect("re-encodes");
        feed(
            app,
            Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
            dereth_protocol::write_body(msg).expect("re-encodes"),
            t,
        );
        frames(app, 2);
    };

    // 1. A styled buffer moves the stance off NonCombat, so the zero-header assertion below is a
    //    change rather than a coincidence.
    let before = scene(&app)
        .server_object_style(GOLEM)
        .expect("a live golem");
    let styled_style = MotionCommand::from_index(styled_buf.body.current_style)
        .expect("the recorded style index is in command_ids");
    send(&mut app, &mut styled_msg, &mut styled_buf, 1, 3.0);
    assert_eq!(
        scene(&app).server_object_style(GOLEM),
        Some(styled_style),
        "the pre-switch motion attempt applies a style the table has (was {before:?})"
    );
    assert_ne!(
        styled_style,
        MotionCommand::NON_COMBAT,
        "and it is not where we end up"
    );

    // 2. The style-zero buffer. It is delivered and applied -- its forward command lands -- and
    //    the stance goes to `NonCombat`, which is where `case 0`'s unpacked state leaves it, and
    //    NOT to the command whose style the motion table refused.
    let want_forward = zero_buf
        .body
        .interpreted
        .as_ref()
        .and_then(|s| s.forward_command)
        .and_then(MotionCommand::from_index);
    send(&mut app, &mut zero_msg, &mut zero_buf, 2, 4.0);
    assert_eq!(
        scene(&app).server_object_style(GOLEM),
        Some(MotionCommand::NON_COMBAT),
        "a style-zero header leaves the object in the unpacked state's NonCombat default; \
         `Invalid` here would mean the refused style attempt overwrote that state"
    );
    if let Some(f) = want_forward {
        assert_eq!(
            scene(&app).server_object_motion(GOLEM).map(|m| m.1),
            Some(f),
            "and the buffer was processed rather than dropped: its forward command landed"
        );
    }
}

/// **`StandingLongJump` on the remote arm.** `fellowship-two-monarch` and
/// `fellowship-three-vassal` send remote `case 0` buffers with the bit set, and
/// `world.rs::apply_movement` must read it.
///
/// Movement decoding's `case 0` tail assigns standing-long-jump state on **every** `case 0`, from
/// motion-flags bit `0x200`, whether set or clear. The assertion therefore has two halves; the
/// second catches an "assign only when set" bug. The negative half uses the same recorded buffer
/// with that one bit removed, so the two differ in nothing else.
#[test]
fn a_recorded_standing_long_jump_flag_reaches_the_remote_interpreter_and_is_cleared_again() {
    let (mut app, rows) = app_with_recorded_body();
    place_golem(&mut app, &rows, 6.0);
    assert!(
        !scene(&app).server_object_standing_longjump(GOLEM),
        "it starts clear"
    );

    let mut found = None;
    'outer: for name in ["fellowship-two-monarch", "fellowship-three-vassal"] {
        let Some(c) = Corpus::load(name).expect("the locked corpus decodes") else {
            continue;
        };
        for row in c.blobs.iter().filter(|r| {
            r.dir == Direction::ServerToClient && r.opcode == Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0
        }) {
            let Ok(m) = MovementSetObjectMovement::read(&mut dereth_protocol::Reader::new(
                &row.payload[4..],
            )) else {
                continue;
            };
            let Ok(buf) = m.decoded_movement() else {
                continue;
            };
            if buf.body.interpreted.is_none() {
                continue; // only `case 0` carries the tail
            }
            if buf.body.motion_flags & dereth_protocol::movement::motion_flags::STANDING_LONG_JUMP
                != 0
            {
                found = Some((m, buf));
                break 'outer;
            }
        }
    }
    let (mut msg, mut buf) =
        found.expect("`fellowship-two-monarch`/`fellowship-three-vassal` carry a remote case-0 buffer with StandingLongJump set");
    msg.id = GOLEM;
    buf.server_control_timestamp = 0;

    buf.movement_timestamp = 1;
    msg.movement = MovementSetObjectMovement::encode_movement(&buf).expect("re-encodes");
    feed(
        &mut app,
        Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
        dereth_protocol::write_body(&msg).expect("re-encodes"),
        3.0,
    );
    frames(&mut app, 2);
    assert!(
        scene(&app).server_object_standing_longjump(GOLEM),
        "`standing_longjump = MotionFlags & 0x200` reaches the remote interpreter"
    );

    // The same bytes with the one bit taken out.
    buf.body.motion_flags &= !dereth_protocol::movement::motion_flags::STANDING_LONG_JUMP;
    buf.movement_timestamp = 2;
    msg.movement = MovementSetObjectMovement::encode_movement(&buf).expect("re-encodes");
    feed(
        &mut app,
        Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
        dereth_protocol::write_body(&msg).expect("re-encodes"),
        4.0,
    );
    frames(&mut app, 2);
    assert!(
        !scene(&app).server_object_standing_longjump(GOLEM),
        "and the assignment is unconditional: a case-0 buffer without the bit clears it again"
    );
}
