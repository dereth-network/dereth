//! In first person, or with mouse turning on, the turn keys and the mouse turn the body; in third
//! person without mouse turning, the camera key orbits the camera and the movement key turns the
//! body, as before. Camera rotation has three arms: with mouse turning on and input from the mouse,
//! it turns the player (or stops drift inside the dead zone); otherwise, at the in-head offset
//! `(0.0, 0.18, 0.0)` the body turns to its heading plus or minus 8 degrees, and anywhere else the
//! camera orbits. A camera key also reaches the pivot object and the stiffness pair. Fixture: a
//! headless `App` on the retail `DEFAULT_LANDBLOCK` with a body built from
//! `early-inventory-and-casting`'s recorded player, driven from `InputShell::inject_action` and
//! `App::cursor_moved` to the body's position and the `0xF61C Movement_MoveToState` and
//! `0xF753 Movement_AutonomousPosition` a socket-free replay endpoint emits.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::app::{frames, position, unhide_recorded_player as unhide_the_player};

use dereth_animation::MotionCommand;
use dereth_client::app::App;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::camera::IN_HEAD_OFFSET;
use dereth_client_runtime::config::Config;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::actions::unpack_action;
use dereth_protocol::movement::{MovementAutonomousPosition, MovementMoveToState};
use dereth_protocol::objects::ItemCreateObject;
use dereth_protocol::{Message, Opcode};
use {
    dereth_client_runtime::landblock::DEFAULT_LANDBLOCK, dereth_client_runtime::scene::SceneConfig,
};

/// Degrees, with 0 north and values increasing **clockwise**, matching the first-person arm's
/// 8-degree increment.
fn heading(app: &App) -> f32 {
    dereth_physics::math::get_heading(&position(app).frame)
}

/// The signed shortest way round from `a` to `b`, positive clockwise (i.e. to the player's right).
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

fn camera_offset(app: &App) -> dereth_primitives::Vec3 {
    app.world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .camera
        .manager
        .viewer_offset
}

/// The camera's own yaw, read back out of the frame the renderer is handed.
fn camera_yaw_degrees(app: &App) -> f32 {
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    let f = c
        .camera_render_frame()
        .expect("the sweep placed the camera");
    dereth_client_runtime::camera::FreeCamera::from_frame(&f)
        .yaw
        .to_degrees()
}

/// `movement::server_approach_handover`'s harness: real terrain, and the corpus login's own player
/// identity relocated onto it. `mouse_turning` is the profile's `Input.UseMouseTurning`.
fn setup(mouse_turning: bool) -> App {
    let mut app = App::new(Config {
        headless: true,
        width: 320,
        height: 240,
        sound: false,
        ui: false,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("required real DAT App and headless device");
    app.start_shell().expect("InputShell");
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        mouse_look: dereth_client_runtime::actions::camera::MouseLookPreferences {
            use_mouse_turning: mouse_turning,
            ..Default::default()
        },
        ..Default::default()
    })
    .expect("real terrain/physics scene");
    frames(&mut app, 60);

    let corpus = Corpus::load("early-inventory-and-casting")
        .expect("corpus decodes")
        .expect("early-inventory-and-casting");
    let player_row = corpus
        .blobs
        .iter()
        .find(|r| r.dir == Direction::ServerToClient && r.opcode == Opcode::LOGIN_CREATE_PLAYER.0)
        .expect("recorded player identity");
    let id = ObjectId(u32::from_le_bytes(
        player_row.payload[4..8].try_into().unwrap(),
    ));
    let row = corpus
        .blobs
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                && u32::from_le_bytes(r.payload[4..8].try_into().unwrap()) == id.0
        })
        .expect("recorded player assets");
    let mut create = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
        .expect("recorded F745");
    let here = position(&app);
    create.0.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
        objcell_id: here.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: here.frame.origin.x,
                y: here.frame.origin.y,
                z: here.frame.origin.z,
            },
            orientation: dereth_protocol::types::Quat {
                w: here.frame.rotation.w,
                x: here.frame.rotation.x,
                y: here.frame.rotation.y,
                z: here.frame.rotation.z,
            },
        },
    });
    create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
    app.probe_mut()
        .objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("constructed terrain placement"),
        },
        LocalTime(1.0),
    );
    unhide_the_player(&mut app, &corpus, id);
    frames(&mut app, 90);
    {
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        assert!(c.on_ground(), "real terrain must support the local body");
        assert_eq!(c.object_id(), id, "the body adopted the server's id");
        assert!(
            !c.world
                .get(c.handle)
                .expect("the local collision body")
                .state()
                .is_hidden(),
            "premise: the hidden login create skips the part array's complete animation \
             offset — rotation and translation — so the recorded unhide must have reached the body"
        );
    }
    app
}

/// One input action event delivered through the current input-manager queue.
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

/// The camera action map is `5`; the movement map is `4`. The shipped keymap binds
/// `DIK_NUMPAD4` to Rotate Camera Left and `DIK_A` to Turn Left.
const CAMERA_MAP: u32 = 5;
const MOVEMENT_MAP: u32 = 4;

/// A running total of how far the body has turned, sampled every frame, so that a turn larger
/// than half a circle is not read as a small turn the other way. **This matters**: a single
/// `heading_after - heading_before` on a body that turned 266 degrees reads as +94, and the first
/// draft of this file concluded from exactly that reading that the whole build's turn direction
/// was inverted. It is not. Sampled: `TurnLeft` accumulates -395 degrees over 90 frames and
/// `TurnRight` +395, i.e. left is counter-clockwise on the clockwise heading compass.
struct Turn {
    prev: f32,
    total: f32,
}

impl Turn {
    fn start(app: &App) -> Self {
        Self {
            prev: heading(app),
            total: 0.0,
        }
    }

    fn sample(&mut self, app: &App) {
        let h = heading(app);
        self.total += turned(self.prev, h);
        self.prev = h;
    }
}

/// The socket-free replay endpoint of `movement::jump_charge_release`, reduced to one question:
/// which movement events actually left the client, and what heading they carried.
///
/// **No datagram leaves this process** -- `ClientNetwork` here has no OS socket; `take_outgoing` hands
/// back the datagrams the packet controller *built*.
struct Wire {
    reassembly: dereth_transport::indicator::Indicator,
    /// `0xF61C Movement_MoveToState`, which the mouse-turning arm emits under its own 0.5-second
    /// throttle.
    move_to_states: Vec<f32>,
    /// `0xF753 Movement_AutonomousPosition`, the once-a-second position report that tells the
    /// server about a first-person turn because that arm emits no movement event of its own.
    positions: Vec<f32>,
}

impl Wire {
    fn attach(app: &mut App) -> Self {
        let mut net = dereth_client_runtime::net::ClientNetwork::new(
            "127.0.0.1:19000",
            7304,
            "turn-keys-station",
            "unused",
            0,
        )
        .unwrap();
        // The same explicit connection facts `objects::frame_phase_ordering::Peer` uses: no socket, no
        // handshake, no login.
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().unwrap()),
        );
        app.attach_replay_network(net)
            .expect("phase-owned socket-free endpoint");
        Self {
            reassembly: dereth_transport::indicator::Indicator::default(),
            move_to_states: Vec::new(),
            positions: Vec::new(),
        }
    }

    fn observe(&mut self, app: &mut App) {
        #[allow(clippy::cast_precision_loss)]
        let now = LocalTime(
            app.frames_drawn() as f64 * dereth_client_runtime::platform::clock::HEADLESS_STEP,
        );
        for (bytes, _) in app
            .replay_network_mut()
            .expect("replay endpoint")
            .take_outgoing()
        {
            let packet =
                dereth_transport::ParsedPacket::parse(&bytes).expect("a real output packet");
            for blob in
                self.reassembly
                    .check_in_packet(&packet.fragments, packet.header.rec_id, now)
            {
                if blob.payload.get(..4) != Some(0xF7B1_u32.to_le_bytes().as_slice()) {
                    continue; // login / cache / ACK housekeeping
                }
                let Ok(mut a) = unpack_action(&blob.payload) else {
                    continue;
                };
                if a.sub_type.0 == MovementMoveToState::OPCODE.0 {
                    let m = MovementMoveToState::read(&mut a.body).expect("a real 0xF61C body");
                    self.move_to_states.push(wire_heading(&m.0.position));
                } else if a.sub_type.0 == MovementAutonomousPosition::OPCODE.0 {
                    let m =
                        MovementAutonomousPosition::read(&mut a.body).expect("a real 0xF753 body");
                    self.positions.push(wire_heading(&m.0.position));
                }
            }
        }
    }

    fn frames(&mut self, app: &mut App, count: usize) {
        for _ in 0..count {
            assert!(app.frame());
            self.observe(app);
        }
    }
}

/// The heading a wire `PositionWire` carries, in the same clockwise degrees as [`heading`].
fn wire_heading(p: &dereth_protocol::types::PositionWire) -> f32 {
    dereth_physics::math::get_heading(&dereth_primitives::Frame::new(
        dereth_primitives::Vec3::new(p.frame.origin.x, p.frame.origin.y, p.frame.origin.z),
        dereth_primitives::Quat::new(
            p.frame.orientation.w,
            p.frame.orientation.x,
            p.frame.orientation.y,
            p.frame.orientation.z,
        ),
    ))
}

/// Behaviour: camera.turn.in-first-person-the-turn-keys-turn-the-body
/// **1 — first person.** With the eye in the head, *Rotate Camera Left* does not orbit anything.
/// It reads the body heading, adds 8 degrees, wraps at 360 degrees, and requests that heading from
/// the body movement interpreter.
///
/// `left` is the **camera's** sense and the body's is its opposite by construction: the action
/// handler passes `left = (action == 0x35)` for *Rotate Camera Left*, which spins the camera to
/// the player's left and therefore swings the *view* to the right, and
/// first person keeps that view sense by turning the body right: a `+8` change on the clockwise
/// heading, matching `TurnRight 0x6500000D` on the sibling arm.
///
/// The camera follows because in first person it **is** the head: `viewer_offset` never moves and
/// the drawn frame's yaw tracks the body's.
///
/// The decision reaches the body's turn-to-heading path and the body turns the way it asked; a
/// held key keeps turning (see the note under the last assertion).
#[test]
fn in_first_person_the_turn_key_turns_the_body_and_the_camera_follows() {
    let mut app = setup(false);
    let mut wire = Wire::attach(&mut app);
    wire.frames(&mut app, 4); // the bootstrap 0xF61C/0xF753 are real and are not this key's

    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::FIRST_PERSON,
        CAMERA_MAP,
        true,
    );
    frames(&mut app, 4);
    assert_eq!(
        camera_offset(&app),
        IN_HEAD_OFFSET,
        "the First Person Camera action took"
    );

    let h0 = heading(&app);
    let yaw0 = camera_yaw_degrees(&app);
    let turns0 = app.probe().camera_turns_applied();
    let sent0 = wire.positions.len();
    let mut turn = Turn::start(&app);

    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::ROTATE_LEFT,
        CAMERA_MAP,
        true,
    );
    // Every heading the body held while the key was down: the once-a-second
    // `0xF753` below reports one of *these*, not the heading at the instant the loop ends.
    let mut held: Vec<f32> = Vec::new();
    for _ in 0..120 {
        wire.frames(&mut app, 1);
        turn.sample(&app);
        held.push(heading(&app));
    }
    let h1 = heading(&app);

    assert!(
        app.probe().camera_turns_applied() > turns0,
        "camera-set rotation's first-person arm must reach the command interpreter: {turns0} -> {}",
        app.probe().camera_turns_applied()
    );
    assert!(
        turn.total > 3.0,
        "the body must turn to the player's right (clockwise, the +8 heading change): \
         {h0} -> {h1}, {} degrees accumulated",
        turn.total
    );
    assert_eq!(
        camera_offset(&app),
        IN_HEAD_OFFSET,
        "and the camera must not have orbited instead"
    );
    // The camera-follows-the-head comparison is taken after the key is released and the body has
    // settled (below): with the body turning one animation frame per sweep, any
    // single-frame read of body and camera is one sweep apart.

    // The first-person arm emits no movement event of its own — only the mouse-turning arm does —
    // so the once-a-second `0xF753` tells the server, and it must carry a heading the body held.
    assert!(
        wire.positions.len() > sent0,
        "the position reporter must keep reporting: 0xF753 count {sent0} -> {}",
        wire.positions.len()
    );
    let wired = *wire.positions.last().expect("a 0xF753 left the client");
    assert!(
        held.iter().any(|h| turned(wired, *h).abs() < 5.0),
        "the outgoing 0xF753 must carry a heading the body actually held: wire {wired}, body now \
         {h1}, held {:?}",
        held.iter().step_by(10).collect::<Vec<_>>()
    );

    // A *held* key re-aims every frame, as in retail: this arm never updates the last-rotation
    // time, so its 0.0002-second gate stays open and the camera update repeats the rotation once a
    // frame. Each re-aim cancels the previous move-to and stops the body before appending the new
    // node, and stopping completely ends by checking completed motions, so the cancellation's READY
    // nodes are popped before the new turn asks whether motions are pending and starts inside the
    // same call. Without that drain every re-aim parks the new node behind the stop it has just
    // queued, the next frame's re-aim cancels it again, and a held key produces one 8-degree step
    // (5.7 degrees measured) instead of a continuous turn. With it the body turns at the motion
    // table's own turn rate (343.8 degrees over these 120 frames). `movement::move_to_fidelity`
    // holds the station; this is the guard.
    assert!(
        turn.total > 45.0,
        "a held first-person turn key must keep turning: {} degrees over 120 frames",
        turn.total
    );

    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::ROTATE_LEFT,
        CAMERA_MAP,
        false,
    );
    frames(&mut app, 10);
    // `FreeCamera::yaw` is counter-clockwise from north and the body's heading is clockwise, so a
    // body turning +d degrees is a camera yawing -d. Read settled, see above.
    let h2 = heading(&app);
    let yaw2 = camera_yaw_degrees(&app);
    let camera_moved = turned(-yaw0, -yaw2);
    assert!(
        (camera_moved - turned(h0, h2)).abs() < 3.0,
        "the camera must follow the head it is in: body {}, camera {camera_moved}",
        turned(h0, h2)
    );
}

/// **2 — mouse turning.** `Input.UseMouseTurning` on, camera in the ordinary third-person chase
/// position: the mouse-turning test passes and the mouse path supplies parameter 0, so the player
/// movement arm turns the **body** and the camera does not orbit
/// at all. **The offset is not part of that test** — the mouse-turning arm is taken at any offset,
/// so the two arms are not conjoined.
///
/// The direction is retail's: the mouse handler passes
/// `left = fx > 0 ? InHead() : !InHead()`, so a rightward mouse in third person passes
/// `left = false`, and the movement arm turns that into `TurnLeft 0x6500000E`. That is the same view
/// sense the third-person orbit would have given (camera spins right, view swings left) — the body
/// is substituted for the camera and the substitution keeps the sense.
#[test]
fn with_mouse_turning_on_the_mouse_turns_the_body() {
    let mut app = setup(true);
    let mut wire = Wire::attach(&mut app);
    wire.frames(&mut app, 4);
    assert!(
        app.world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .camera
            .prefs
            .use_mouse_turning,
        "the profile's Input.UseMouseTurning must reach the camera"
    );
    let off0 = camera_offset(&app);
    assert_ne!(
        off0, IN_HEAD_OFFSET,
        "this station is the ordinary third-person chase camera"
    );

    let turns0 = app.probe().camera_turns_applied();
    let sent0 = wire.move_to_states.len();
    let mut turn = Turn::start(&app);

    // Hold the right button, then drag steadily right. The first five samples are the per-axis
    // warm-up, so the turn starts on the sixth.
    app.mouse_look_button(true);
    let mut x = 160.0;
    for _ in 0..90 {
        x += 60.0;
        app.cursor_moved(x, 120.0);
        wire.frames(&mut app, 1);
        turn.sample(&app);
    }
    let h1 = heading(&app);

    assert!(
        app.probe().camera_turns_applied() > turns0,
        "mouse turning must reach the command interpreter: {turns0} -> {}",
        app.probe().camera_turns_applied()
    );
    assert!(
        turn.total < -90.0,
        "a rightward mouse with mouse turning on must turn the body counter-clockwise \
         (TurnLeft): {} degrees accumulated",
        turn.total
    );
    assert_eq!(
        app.world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .driver()
            .movement
            .interp
            .raw_state
            .turn_command,
        MotionCommand::TURN_LEFT,
        "and it must be TurnLeft on the body's own raw motion state, not a camera orbit"
    );
    assert_eq!(
        camera_offset(&app),
        off0,
        "the mouse-turning arm never touches viewer_offset — the body turned instead"
    );
    assert!(
        wire.move_to_states.len() > sent0,
        "the movement-event send, plus the reporter's own change detector: 0xF61C count \
         {sent0} -> {}",
        wire.move_to_states.len()
    );
    let wired = *wire
        .move_to_states
        .last()
        .expect("a 0xF61C left the client");
    assert!(
        turned(wired, h1).abs() < 25.0,
        "the outgoing 0xF61C must carry the turning body's heading: wire {wired}, body {h1}"
    );
    app.mouse_look_button(false);
}

/// **3 — the regression guard.** Third person, mouse turning off: nothing about the ordinary
/// controls may change. The camera key orbits the camera and leaves the body alone; the movement
/// key turns the body and leaves the camera alone.
#[test]
fn in_third_person_without_mouse_turning_the_keys_do_what_they_did() {
    let mut app = setup(false);
    let off0 = camera_offset(&app);
    assert_ne!(off0, IN_HEAD_OFFSET);

    // (a) Rotate Camera Left uses the ordinary orbit arm.
    let mut turn = Turn::start(&app);
    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::ROTATE_LEFT,
        CAMERA_MAP,
        true,
    );
    for _ in 0..30 {
        frames(&mut app, 1);
        turn.sample(&app);
    }
    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::ROTATE_LEFT,
        CAMERA_MAP,
        false,
    );
    frames(&mut app, 5);
    turn.sample(&app);
    assert_ne!(
        camera_offset(&app),
        off0,
        "the camera must still orbit in third person"
    );
    assert!(
        turn.total.abs() < 1.0,
        "and it must still leave the body alone: {} degrees",
        turn.total
    );
    assert_eq!(
        app.probe().camera_turns_applied(),
        0,
        "no body turn may be taken off the camera in third person with mouse turning off"
    );

    // (b) The Turn Left movement key still turns the body. Its `0x2F` action is a motion command
    // and has never consulted the camera mode.
    let off1 = camera_offset(&app);
    let mut turn = Turn::start(&app);
    action(
        &mut app,
        dereth_client_runtime::actions::movement::action::TURN_LEFT,
        MOVEMENT_MAP,
        true,
    );
    for _ in 0..60 {
        frames(&mut app, 1);
        turn.sample(&app);
    }
    assert!(
        turn.total < -30.0,
        "Turn Left must still turn the body counter-clockwise: {} degrees",
        turn.total
    );
    assert_eq!(
        app.world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .driver()
            .movement
            .interp
            .raw_state
            .turn_command,
        MotionCommand::TURN_LEFT,
        "through the command lists, not through the camera"
    );
    assert_eq!(
        camera_offset(&app),
        off1,
        "and the movement key must not move the camera offset"
    );
    assert_eq!(
        app.probe().camera_turns_applied(),
        0,
        "nor route through camera-set effects"
    );
    action(
        &mut app,
        dereth_client_runtime::actions::movement::action::TURN_LEFT,
        MOVEMENT_MAP,
        false,
    );
    frames(&mut app, 10);
}

/// **4 — the camera's pivot object and stiffness, from a key.**
///
/// In retail, pivot selection is reached from the default-offset, farther and viewer-home paths,
/// and the two stiffness setters from six camera methods; the claim is that a **key** reaches
/// them. *Move Camera to Default* (`0x39`) reaches the default pivot and both stiffness writes,
/// and *Zoom Camera Out* (`0x34`) reaches the pivot write when leaving first person. Target
/// selection is driven by combat target tracking and is covered by `camera::chase_camera_target`.
#[test]
fn a_camera_key_reaches_set_pivot_object_and_the_stiffness_pair() {
    let mut app = setup(false);
    let body = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .object_id();
    {
        // Start from a state neither field is already in, so an assertion on the end state cannot
        // pass on a build where the key does nothing.
        let c = app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_mut()
            .unwrap();
        c.camera.manager.pivot_object_id = ObjectId(0);
        c.camera.manager.t_stiffness = 0.125;
        c.camera.manager.r_stiffness = 0.125;
    }
    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::MOVE_TO_DEFAULT,
        CAMERA_MAP,
        true,
    );
    frames(&mut app, 4);
    {
        let m = &app
            .world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .camera
            .manager;
        assert_eq!(
            (m.pivot_object_id, m.pivot_part_index),
            (body, -1),
            "Move Camera to Default must restore the player body as pivot with part index -1"
        );
        assert!(
            (m.t_stiffness - 0.125).abs() > 1e-6 && (m.r_stiffness - 0.125).abs() > 1e-6,
            "and both translational and rotational stiffness values must change from the 0.125 \
             negative control: {} / {}",
            m.t_stiffness,
            m.r_stiffness
        );
    }

    // Zooming out re-pivots only on the arm that leaves first person, so go there first.
    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::FIRST_PERSON,
        CAMERA_MAP,
        true,
    );
    frames(&mut app, 4);
    assert_eq!(camera_offset(&app), IN_HEAD_OFFSET);
    {
        let c = app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_mut()
            .unwrap();
        c.camera.manager.pivot_object_id = ObjectId(0);
    }
    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::ZOOM_OUT,
        CAMERA_MAP,
        true,
    );
    frames(&mut app, 4);
    assert_eq!(
        app.world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .camera
            .manager
            .pivot_object_id,
        body,
        "Farther's leaving-first-person arm must update the pivot object too"
    );
    assert_ne!(
        camera_offset(&app),
        IN_HEAD_OFFSET,
        "and it must have left first person"
    );
    action(
        &mut app,
        dereth_client_runtime::actions::camera::action::ZOOM_OUT,
        CAMERA_MAP,
        false,
    );
    frames(&mut app, 2);
}

/// **2b — the turn stops.** With `Input.UseMouseTurning` on, the body's mouse turn ends when the
/// mouse stops moving under mouse look (the input poll's 0.2 s idle tick stops the drift) and when
/// the right button is let go mid-drag; the body then holds its heading.
///
/// Behaviour: camera.mouse-turning.the-body-stops-turning-when-the-mouse-stops-or-the-button-is-let-go
#[test]
fn with_mouse_turning_on_the_body_stops_turning_when_the_mouse_stops_or_is_let_go() {
    let mut app = setup(true);
    let mut wire = Wire::attach(&mut app);
    wire.frames(&mut app, 4);
    let turn_command = |app: &App| {
        app.world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .driver()
            .movement
            .interp
            .raw_state
            .turn_command
    };
    let drag = |app: &mut App, wire: &mut Wire, x: &mut f64, n: usize| {
        for _ in 0..n {
            *x += 60.0;
            app.cursor_moved(*x, 120.0);
            wire.frames(app, 1);
        }
    };
    let mut x = 160.0;

    // Held, dragged, then held still: the turn stops within the idle tick.
    app.mouse_look_button(true);
    drag(&mut app, &mut wire, &mut x, 40);
    let turning = turn_command(&app) == MotionCommand::TURN_LEFT;
    wire.frames(&mut app, 30);
    let stopped_still = turn_command(&app) != MotionCommand::TURN_LEFT;
    let h = heading(&app);
    wire.frames(&mut app, 20);
    let held_still = turned(h, heading(&app)).abs() < 1.0;

    // Dragged again and let go mid-drag: the turn stops with the button.
    drag(&mut app, &mut wire, &mut x, 40);
    let turning_again = turn_command(&app) == MotionCommand::TURN_LEFT;
    app.mouse_look_button(false);
    wire.frames(&mut app, 3);
    let stopped_released = turn_command(&app) != MotionCommand::TURN_LEFT;
    let h = heading(&app);
    wire.frames(&mut app, 30);
    let held_released = turned(h, heading(&app)).abs() < 1.0;

    assert!(turning && turning_again, "the drag turns the body");
    assert!(
        stopped_still && held_still,
        "a still mouse stops the turn: stopped {stopped_still}, held {held_still}"
    );
    assert!(
        stopped_released && held_released,
        "letting the button go stops the turn: stopped {stopped_released}, held {held_released}"
    );
}
