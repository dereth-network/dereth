//! Move-to fidelity: the local body's cached velocity reaches its own move-to manager, a moving
//! target is led by velocity times the quantum, target processing runs once per physics sub-step,
//! and a held first-person turn key keeps turning the body. Fixture: real DAT terrain, the local
//! body wearing **long-solo-play**'s recorded identity, the session's Sparring Golem from its
//! recorded `0xF745`, and its recorded `0xF74C` approach, in a headless App; one test drives a
//! bare `Character` instead. No datagram leaves this process.
//!
//! The four behaviours, each an original-client rule:
//!
//! 1. Each physics update writes `cached_velocity` as
//!    `get_offset(position, sphere_path_position) / quantum`. The move-to manager reads that value;
//!    above 0.1 it derives an ETA from distance / speed and writes the subscription quantum, for
//!    the **local body's** own approach as well as a server object's.
//! 2. Target interpolation leads the raw position by the target's
//!    `cached_velocity * quantum`.
//! 3. Target processing runs once per admitted physics sub-step inside the object update, before
//!    the original movement-manager time step, rather than once after the sub-step ladder.
//! 4. Each original camera update with the rotate flag held requests a heading eight degrees ahead,
//!    and the move-to queue appends the request without a coalescing walk, so the body keeps turning.
//!
//! App-level tests use a headless simulated presentation and fail on startup errors. The
//! `Character`-level test supplies the multi-sub-step contrast; App assertions count admitted
//! physics sweeps rather than assuming one sweep per rendered frame.

use crate::common::client_dir;

use std::sync::Arc;

use dereth_animation::frame::V3;
use dereth_animation::motion::moveto::TargetSnapshot;
use dereth_animation::motion::{MoveToRequest, MovementParameters};
use dereth_client::app::App;
use dereth_client::camera::IN_HEAD_OFFSET;
use dereth_client::character::{Character, CharacterInput};
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
/// long-solo-play's Sparring Golem.
const GOLEM: ObjectId = ObjectId(0x8000_09d2);
/// The recorded `MoveToObject` this file replays, by capture timestamp.
const APPROACH_MICROS: u64 = 91_500_548;
/// The camera action map is `5`; the movement map is `4`, as established by the binding fixtures.
const CAMERA_MAP: u32 = 5;
const MOVEMENT_MAP: u32 = 4;

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

fn body(app: &App) -> &Character {
    scene(app).character.as_ref().expect("a local body")
}

fn body_position(app: &App) -> Position {
    body(app).position()
}

fn heading(app: &App) -> f32 {
    dereth_physics::math::get_heading(&body_position(app).frame)
}

/// The signed shortest way round from `a` to `b`, positive clockwise.
fn turned(a: f32, b: f32) -> f32 {
    let mut d = b - a;
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d
}

fn feed(app: &mut App, opcode: Opcode, body: Vec<u8>, now: f64) {
    app.objects_mut()
        .apply_event(&SessionEvent::WorldObject { opcode, body }, LocalTime(now));
}

/// One current injected action event, matching the shape used by the camera-turn tests.
fn action(app: &mut App, id: dereth_input::ActionId, map: u32, start: bool) {
    app.input_manager_mut()
        .unwrap()
        .inject_action(dereth_input::InputEvent {
            action: id,
            input_map: dereth_input::InputMapId(map),
            toggle: dereth_input::ToggleType::Hold,
            extent: 1.0,
            start,
            repeat_delta: 0,
            repeat_total: 0,
            from_key_down: start,
        });
    frames(app, 1);
}

/// The shared App harness uses real terrain and physics with the local body wearing long-solo-play's
/// recorded identity. It relocates the recorded create onto the loaded terrain while retaining its
/// recorded physics-state word and timestamps; the later recorded unhide remains necessary. `ui`
/// is true for the object-stream tests and false for the camera test.
fn app_with_recorded_body(ui: bool) -> (App, Vec<CorpusBlob>) {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let mut app = crate::common::sim_app::new(Config {
        headless: true,
        sound: false,
        ui,
        width: 640,
        height: 480,
        dat_dir: client_dir(),
        preferences_file: std::env::temp_dir()
            .join("dere-move-to-fidelity-not-created")
            .join("preferences.ini"),
        ..Config::default()
    })
    .expect("a headless App on a software GPU device and the retail dats");
    app.start_shell().expect("the UI shell comes up");
    if ui {
        app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    }
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
    unhide_the_player(&mut app, &rows);
    frames(&mut app, 90);
    {
        let c = body(&app);
        assert!(c.on_ground(), "real terrain must support the recorded body");
        assert_eq!(
            c.object_id(),
            PLAYER,
            "the body adopted long-solo-play's recorded id"
        );
        assert!(
            !c.world
                .get(c.handle)
                .expect("the local collision body")
                .state()
                .is_hidden(),
            "premise: the login create arrives with HIDDEN_PS and a hidden body never \
             advances its animation offset, so the recorded unhide has to have reached it"
        );
    }
    (app, rows)
}

/// long-solo-play's recorded `0xF745` for `PLAYER` (idx 23, `t_rel = 4.451`) is the
/// *login-tunnel* create, exactly as early-inventory-and-casting's is: its physics-state word is `0x00404410` —
/// `HIDDEN_PS | GRAVITY_PS | IGNORE_COLLISIONS_PS | EDGE_SLIDE_PS` — with instance timestamp 2.
/// Retail unhides the body 6.8 s later with the `0xF74B Item_SetState` at idx 80, `t_rel = 11.239`,
/// `state = 0x00400408`, pack `(instance = 2, event = 1)`. Those values passed the session instance
/// and state timestamp gates in the recorded flow; this fixture feeds the event directly at the
/// current object/state seam.
///
/// The original object-create path hands that word to the player's physics body, whose position
/// update skips the part array's animation offset while the hidden bit is set. A station that
/// replays the create without the unhide therefore drives a body that can never move. The fuller
/// byte-level note is in `movement::run_speed`.
fn unhide_the_player(app: &mut App, rows: &[CorpusBlob]) {
    let row = rows
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == 0xf74b
                && subject(r) == PLAYER
                && u32::from_le_bytes(r.payload[8..12].try_into().expect("a state word"))
                    & dereth_physics::PhysicsState::HIDDEN_PS
                    == 0
        })
        .expect("long-solo-play's recorded 0xF74B unhide for the player");
    feed(app, Opcode::ITEM_SET_STATE, row.payload[4..].to_vec(), 1.0);
}

/// Create the recorded golem `metres` away from the body on the block's +x axis. Its position is
/// relocated while the other recorded create fields remain; current object-stream synchronization
/// spawns its collision body.
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

/// Re-address long-solo-play's recorded `MoveToObject` buffer (golem -> player at t=91.500548).
/// The helper changes the message subject, target, origin and movement timestamp while preserving
/// the recorded parameters and run rate. `(GOLEM, PLAYER)` restores the recorded roles; the
/// `(PLAYER, GOLEM)` form matches an out-of-range use approach and current player-movement routing
/// feeds it to `Character::perform_move_to` through the same request shape as the remote arm.
fn recorded_approach(
    rows: &[CorpusBlob],
    subject_id: ObjectId,
    target_id: ObjectId,
    at: Position,
    stamp: u16,
) -> Vec<u8> {
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
    buf.movement_timestamp = stamp;
    buf.body.unhandled = MovementBody::encode_move_to(&MoveToArm::MoveToObject {
        target: target_id,
        origin,
        params,
        run_rate,
    });
    msg.id = subject_id;
    msg.movement = MovementSetObjectMovement::encode_movement(&buf).expect("re-encodes");
    dereth_protocol::write_body(&msg).expect("re-encodes")
}

fn mag(v: Vec3) -> f32 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

// ---------------------------------------------------------------------------------------------
// 1. The local body's own move-to: cached velocity reaches its manager, and the quantum block
//    writes.
// ---------------------------------------------------------------------------------------------

/// A use-object approach is the server's `0xF74C MoveToObject` addressed to the
/// player. The original move-to handler's third block reads the physics body's cached velocity,
/// requires magnitude above 0.1, derives ETA as distance / speed and writes the target quantum.
///
/// ```text
/// velocity = physics body's cached velocity
/// if |velocity| > 0.1:
///     eta = distance / |velocity|
///     set target quantum from eta
/// ```
///
/// If the local body's `MotionEnv::cached_velocity` were never written, the magnitude test would
/// never be true for the player's own approach. The test observes the maximum cached velocities
/// above 0.5 and the handler's output quantum; those maxima need not be equal on every frame. The
/// subscription begins with quantum 0.0, giving the write a negative control.
#[test]
fn the_local_body_carries_its_cached_velocity_and_its_own_move_to_writes_the_quantum() {
    let (mut app, rows) = app_with_recorded_body(true);
    let golem_at = place_golem(&mut app, &rows, 9.0);
    let performed = body(&app).stats.move_tos_performed;

    let bytes = recorded_approach(&rows, PLAYER, GOLEM, body_position(&app), 1);
    feed(&mut app, Opcode::MOVEMENT_SET_OBJECT_MOVEMENT, bytes, 3.0);
    frames(&mut app, 1);
    assert_eq!(
        body(&app).stats.move_tos_performed,
        performed + 1,
        "the player's 0xF74C reaches Character::perform_move_to"
    );
    assert!(body(&app).is_moving_to(), "and it starts a move-to");
    assert_eq!(
        body(&app).driver().target.map(|t| t.quantum),
        Some(0.0),
        "the move-to subscription starts with quantum 0.0"
    );

    let mut walking_frames = 0usize;
    let mut env_velocity_seen = 0.0f32;
    let mut body_velocity_seen = 0.0f32;
    let mut quanta: Vec<f32> = Vec::new();
    let mut closest = f32::MAX;
    let start_sep = dereth_animation::motion::moveto::distance(&body_position(&app), &golem_at);
    for _ in 0..240 {
        frames(&mut app, 1);
        let c = body(&app);
        let d = c.driver();
        let sep = dereth_animation::motion::moveto::distance(&c.position(), &golem_at);
        closest = closest.min(sep);
        let v_body = mag(c.velocity());
        let v_env = mag(d.env.cached_velocity);
        if let Some(q) = d.target.map(|t| t.quantum) {
            if quanta.last() != Some(&q) {
                quanta.push(q);
            }
        }
        if c.is_moving_to() && v_body > 0.1 {
            walking_frames += 1;
            body_velocity_seen = body_velocity_seen.max(v_body);
            env_velocity_seen = env_velocity_seen.max(v_env);
        }
        if !c.is_moving_to() && sep < start_sep - 2.0 {
            break;
        }
    }
    eprintln!(
        "local approach: walking frames {walking_frames}, |body cached velocity| max {body_velocity_seen:.3}, |driver environment cached velocity| max {env_velocity_seen:.3}, separation {start_sep:.3} -> {closest:.3}, quanta {quanta:?}"
    );

    // The positive control: the body really walked, and the physics object's own field really
    // carried the achieved velocity.
    assert!(
        walking_frames > 10,
        "the approach must walk the body: {walking_frames} frames"
    );
    assert!(
        closest < start_sep - 2.0,
        "and close on the golem: {start_sep:.3} -> {closest:.3}"
    );
    assert!(
        body_velocity_seen > 0.5,
        "body cached velocity: {body_velocity_seen:.3}"
    );
    // The claim: the manager's copy carries the same fact...
    assert!(
        env_velocity_seen > 0.5,
        "the local driver's cached velocity must exceed 0.5 while the body walks (body maximum {body_velocity_seen:.3}); it is {env_velocity_seen:.3}"
    );
    // ...and the quantum block wrote on the player's own approach.
    assert!(
        quanta.iter().any(|q| *q > 0.5),
        "the local move-to must write a target quantum above 0.5; values stayed at {quanta:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. A moving target is led by its velocity.
// ---------------------------------------------------------------------------------------------

/// Behaviour: movement.move-to.a-moving-targets-position-leads-by-velocity-times-the-quantum
/// Original target interpolation computes:
///
/// ```text
/// origin.x = (float)quantum * get_velocity().x + position.origin.x   (and y, z)
/// ```
///
/// using the **target's** cached velocity. Here the target is the walking local player and the
/// watcher is long-solo-play's golem on its recorded approach. The golem's move-to writes an ETA-sized
/// quantum. Among updates where both quantum and speed exceed 0.5, every observed lead must be
/// nonzero and at least one must agree with `velocity * quantum` within 35% plus 0.05; the test
/// does not require every frame's independently sampled values to be identical.
#[test]
fn a_moving_targets_interpolated_position_leads_by_its_velocity_times_the_quantum() {
    let (mut app, rows) = app_with_recorded_body(true);
    let _ = place_golem(&mut app, &rows, 10.0);
    let bytes = recorded_approach(&rows, GOLEM, PLAYER, body_position(&app), 1);
    feed(&mut app, Opcode::MOVEMENT_SET_OBJECT_MOVEMENT, bytes, 3.0);
    frames(&mut app, 1);
    assert_eq!(
        simulation(&app).objects.steps.remote_move_tos_performed,
        1,
        "the recorded approach reached the golem"
    );

    // The player walks away along +y (heading 0) while the golem closes from +x.
    action(
        &mut app,
        dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
        MOVEMENT_MAP,
        true,
    );

    #[derive(Debug, Clone, Copy)]
    struct Led {
        frame: usize,
        lead: Vec3,
        player_v: Vec3,
        quantum: f32,
    }
    let mut updates = 0usize;
    let mut led: Vec<Led> = Vec::new();
    let mut last_info = scene(&app).server_object_last_target_info(GOLEM);
    let mut player_moved = 0.0f32;
    let start = body_position(&app);
    for frame in 0..150 {
        frames(&mut app, 1);
        let s = scene(&app);
        let info = s.server_object_last_target_info(GOLEM);
        player_moved = dereth_animation::motion::moveto::distance(&start, &body_position(&app));
        if info != last_info {
            last_info = info;
            updates += 1;
            if let Some(i) = info {
                let lead = dereth_animation::motion::moveto::get_offset(
                    &i.target_position,
                    &i.interpolated_position,
                );
                led.push(Led {
                    frame,
                    lead,
                    player_v: body(&app).velocity(),
                    quantum: s.server_object_target_quantum(GOLEM).unwrap_or(0.0),
                });
            }
        }
    }
    action(
        &mut app,
        dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
        MOVEMENT_MAP,
        false,
    );
    eprintln!("moving target: {updates} target updates; player moved {player_moved:.3} m; leads:");
    for l in &led {
        eprintln!(
            "  f{:>3} lead=({:.3},{:.3},{:.3}) |v|={:.3} q={:.3}",
            l.frame,
            l.lead.x,
            l.lead.y,
            l.lead.z,
            mag(l.player_v),
            l.quantum
        );
    }

    // Positive controls: the player really moved, the tick really delivered updates, and the
    // golem's own quantum block really wrote an ETA (so a zero lead cannot be a zero quantum).
    assert!(
        player_moved > 2.0,
        "the player must walk: {player_moved:.3} m"
    );
    assert!(
        updates >= 2,
        "the 0.5 s tick must deliver updates to the golem: {updates}"
    );
    let with_quantum: Vec<&Led> = led
        .iter()
        .filter(|l| l.quantum > 0.5 && mag(l.player_v) > 0.5)
        .collect();
    assert!(
        !with_quantum.is_empty(),
        "no update arrived while the quantum was set and the player was moving:\n{led:#?}"
    );
    // The claim: at least one update agrees with `interpolated - target == v * quantum` within the
    // stated sampling tolerance, and every qualifying update has a nonzero lead.
    let ok = with_quantum.iter().filter(|l| {
        let want = l.player_v.mul(l.quantum);
        let err = mag(l.lead.sub(want));
        err < 0.35 * mag(want) + 0.05
    });
    assert!(
        ok.count() >= 1,
        "at least one qualifying update must lead by velocity * quantum within tolerance; none did:\n{with_quantum:#?}"
    );
    assert!(
        with_quantum.iter().all(|l| mag(l.lead) > 0.1),
        "a moving target's lead must be non-zero:\n{with_quantum:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. Targetting runs once per physics sub-step.
// ---------------------------------------------------------------------------------------------

/// A `Character` on real Holtburg terrain, with no GPU required.
fn character() -> Character {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let mut c = Character::new(&store, &region, DEFAULT_LANDBLOCK, (20.0, 52.0))
        .expect("a body on real Holtburg terrain");
    let mut t = 0.0;
    while t < 3.0 {
        t += 1.0 / 30.0;
        c.input = CharacterInput::default();
        c.update(LocalTime(t));
    }
    assert!(c.on_ground(), "real terrain must support the body");
    c
}

/// The original object-update tail performs target processing once per sub-step of
/// the physics ladder, immediately before advancing the movement manager:
///
/// ```text
/// process target subscriptions
/// advance the movement manager
/// ```
///
/// A 0.5 s step is three sub-steps (`0.2 + 0.2 + 0.1`: `MAX_QUANTUM = 0.2`, and the remainder
/// is above `MIN_QUANTUM`), and the tested 0.04 s step is one. The count is read off the driver, and the
/// delivery it makes is asserted to have landed by the time the same `update` returns because the
/// manager is then initialized. That by-return observation does not independently prove the
/// before-movement-manager ordering; the ordering is the original client's.
#[test]
fn targetting_runs_once_per_physics_sub_step_before_use_time() {
    let mut c = character();
    let target = ObjectId(0x7000_0002);
    let mut goal = c.position();
    goal.frame.origin.y += 12.0;
    c.perform_move_to(
        &MoveToRequest::MoveToObject {
            object_id: target,
            top_level_id: target,
            radius: 0.5,
            height: 1.5,
        },
        &MovementParameters::default(),
        None,
    );
    assert_eq!(
        c.wanted_target().map(|t| t.id),
        Some(target),
        "the move-to subscribed"
    );
    assert!(
        !c.driver().movement.moveto.initialized,
        "nothing has arrived yet"
    );
    c.set_target_snapshot(Some(TargetSnapshot {
        object_id: target,
        ok: true,
        position: goal,
        velocity: Vec3::ZERO,
    }));

    let (ticks0, updates0) = {
        let d = c.driver();
        (d.targetting_ticks, d.target_updates)
    };
    // One 0.5 s step: three sub-steps.
    c.input = CharacterInput::default();
    c.update(LocalTime(3.5));
    let (ticks1, updates1, initialized) = {
        let d = c.driver();
        (
            d.targetting_ticks,
            d.target_updates,
            d.movement.moveto.initialized,
        )
    };
    eprintln!("sub-steps: 0.5 s step: ticks {ticks0}->{ticks1}, updates {updates0}->{updates1}");
    assert_eq!(
        ticks1 - ticks0,
        3,
        "target processing must run once per sub-step: a 0.5 s step is 0.2 + 0.2 + 0.1"
    );
    assert_eq!(
        updates1 - updates0,
        1,
        "the first target update is delivered once"
    );
    assert!(
        initialized,
        "the target update initialized the move-to by return from the same update"
    );

    // One 0.04 s step: one sub-step (above the physics scheduler's 1/30 s gate, which a step of
    // exactly `1/30` falls under by float rounding, and below `MAX_QUANTUM`), and no second update
    // from a target that has not moved because its displacement remains below the update radius.
    c.update(LocalTime(3.54));
    let (ticks2, updates2) = {
        let d = c.driver();
        (d.targetting_ticks, d.target_updates)
    };
    assert_eq!(ticks2 - ticks1, 1, "a 0.04 s step is one sub-step");
    assert_eq!(
        updates2 - updates1,
        0,
        "a stationary target sends nothing after its first update"
    );

    // At App level each admitted physics sweep is exactly one sub-step (`HEADLESS_STEP ==
    // MIN_QUANTUM`, so no admitted sweep's elapsed exceeds `MAX_QUANTUM`) -- and the scheduler's
    // 1/30 s gate admits only every other headless frame by float rounding (the scene's
    // `updates_without_a_sweep`), so the count is against **sweeps**, not frames.
    let (mut app, rows) = app_with_recorded_body(true);
    let _ = place_golem(&mut app, &rows, 8.0);
    let bytes = recorded_approach(&rows, GOLEM, PLAYER, body_position(&app), 1);
    feed(&mut app, Opcode::MOVEMENT_SET_OBJECT_MOVEMENT, bytes, 3.0);
    frames(&mut app, 1);
    let (t0, _) = scene(&app)
        .server_object_targetting(GOLEM)
        .expect("a live golem");
    // The physics clock itself is the oracle: an admitted sweep moves `last_physics_time`, and at
    // this step size each admitted sweep is one sub-step.
    let mut clock = body(&app).world.last_physics_time();
    let mut physics_ticks = 0u32;
    for _ in 0..30 {
        frames(&mut app, 1);
        let t = body(&app).world.last_physics_time();
        if t != clock {
            clock = t;
            physics_ticks += 1;
        }
    }
    let (t1, u1) = scene(&app)
        .server_object_targetting(GOLEM)
        .expect("a live golem");
    eprintln!("sub-steps: 30 App frames: {physics_ticks} physics ticks; golem ticks {t0}->{t1}, updates {u1}");
    assert!(
        physics_ticks >= 10,
        "the physics world ticked: {physics_ticks} in 30 frames"
    );
    assert_eq!(
        t1 - t0,
        physics_ticks,
        "one target-processing tick per admitted physics tick, each one sub-step"
    );
    assert!(u1 >= 1, "and the golem was fed");
}

// ---------------------------------------------------------------------------------------------
// 4. A held first-person turn key turns continuously.
// ---------------------------------------------------------------------------------------------

/// The original camera update reads the held rotate-left flag and calls the rotation
/// path on every update while it remains set:
///
/// ```text
/// if rotate_left:
///     rotate(left = true, extent = 1.0, held = true)
/// ```
///
/// The first-person arm leaves its time gate open, and the move-to manager appends each requested
/// heading without walking the pending queue for a duplicate. Each request aims eight degrees
/// ahead of the heading at issue time, so a body that turned only one step would show a few
/// degrees over a 120-frame hold. This test includes helper frames for the press and release,
/// requires at least 100 applied turns during the interval, and requires more than 45 degrees of
/// accumulated motion. Those checks establish sustained turning, not exactly one request per loop
/// iteration or a uniform per-frame angular rate.
#[test]
fn a_held_first_person_turn_key_turns_the_body_continuously() {
    let (mut app, _rows) = app_with_recorded_body(false);
    frames(&mut app, 4);
    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::FIRST_PERSON,
        CAMERA_MAP,
        true,
    );
    frames(&mut app, 4);
    let off = body(&app).camera.manager.viewer_offset;
    assert_eq!(off, IN_HEAD_OFFSET, "the First Person Camera action took");

    let h0 = heading(&app);
    let turns0 = app.camera_turns_applied();
    let mut prev = h0;
    let mut total = 0.0f32;
    let mut trace: Vec<String> = Vec::new();
    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::ROTATE_LEFT,
        CAMERA_MAP,
        true,
    );
    for frame in 0..120 {
        frames(&mut app, 1);
        let h = heading(&app);
        total += turned(prev, h);
        prev = h;
        if frame % 5 == 0 || frame < 12 {
            let c = body(&app);
            let d = c.driver();
            let m = &d.movement.moveto;
            trace.push(format!(
                "f{:>3} h={:>8.3} total={:>8.3} nodes={} front={:?} cmd={:?} turn={:?} pending_motions={} tbl_pending={} anim_frame={}",
                frame,
                h,
                total,
                m.pending_actions.len(),
                m.pending_actions.front().map(|n| n.heading),
                m.current_command,
                d.movement.interp.interpreted_state.turn_command,
                d.movement.interp.pending_motions.len(),
                d.motion_table.pending_len(),
                d.sequence.curr_frame_number(),
            ));
        }
    }
    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::ROTATE_LEFT,
        CAMERA_MAP,
        false,
    );
    let turns = app.camera_turns_applied() - turns0;
    eprintln!("held turn: {turns} turn commands applied across the held-key interval; {total:.3} degrees accumulated");
    for l in &trace {
        eprintln!("  {l}");
    }

    assert!(
        turns >= 100,
        "the held-key interval must apply at least 100 turn commands: {turns}"
    );
    assert!(
        total > 0.0,
        "and the accumulated turn follows the positive eight-degree requests: {total}"
    );
    // The claim. The helper adds press/release frames around the 120-frame loop; this measures
    // sustained accumulated turning across the interval rather than an exact per-frame rate.
    assert!(
        total > 45.0,
        "a held first-person turn key must turn the body continuously, not one 8-degree step: \
         {total:.3} degrees over 120 frames\n{}",
        trace.join("\n")
    );
}
