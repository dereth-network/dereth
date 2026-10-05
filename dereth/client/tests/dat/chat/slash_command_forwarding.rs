//! An unknown slash command reaches the shard, and `*wave*` moves the body.
//!
//! The chat-command dispatcher indexes a table by the first character of the trimmed line: slash,
//! colon and semicolon, at-sign and the other accepted characters each have an arm, and anything
//! else is ordinary talk. Slash's arm overwrites the character with `@` before joining the same
//! command path a typed `@` enters. A registered handler receives the parsed arguments and either
//! completes silently or, returning `false`, prints the local failure `0x26` ("That is not a valid
//! command."). A word with no registered handler takes the forwarding tail instead: the channel
//! pre-pass declines it (`argc <= 0`, channel id 0 and the `@help` channel id `0x400` all
//! decline), and the unchanged stored command line goes out as talk event `0x0015`. So:
//!
//! 1. the text on the wire is the stored command line, `@`-prefixed, with its original spacing
//!    (not re-joined from parsed tokens as a handled command's arguments are):
//!    `/teleloc 0xA9B40022 110.2 42.3 97.5` goes out as `"@teleloc 0xA9B40022 110.2 42.3 97.5"`;
//! 2. the forward depends on the table miss, not on which sigil was typed;
//! 3. the client prints nothing for a table miss; the invalid-command sentence a player sees for
//!    `/wave` is the shard's own reply.
//!
//! A typed `*wave*` plays its motion locally and immediately: the pose's motion command is queued
//! for `App::apply_input_actions` and handed to the same movement-command path the `J` key feeds,
//! and this file asserts the state the body itself then carries (the `0xF61C` raw motion state's
//! action list), because a queue entry is not a wave.
//!
//! Fixture: a headless App whose requests are encoded through a `MockTransport` (no socket, no
//! datagram leaves the process), driven by a pointer press on the chat entry and one character
//! message per character; the emote station logs in off `long-solo-play`'s recorded player create.

use dereth_client_model::Request;
use dereth_client_net::client_session::{testing::MockTransport, Session};
use dereth_client_runtime::requests::send_request;
use dereth_primitives::NetQueue;
use dereth_ui::{framework::mode, ElementId};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use {
    dereth_client::app::App, dereth_client_runtime::config::Config, dereth_desktop::pump::Pump,
    dereth_input::win32::Win32Message,
};

/// The main chat window's text entry and log element.
const CHAT_ENTRY: ElementId = dereth_ui_screens::chat::window::ENTRY;
const CHAT_LOG: ElementId = dereth_ui_screens::chat::window::LOG;
/// The speech-bubble list — the only shipped surface whose filter accepts chat type `0x1A`, which
/// carries the local invalid-command refusal `0x26`.
const SPEW_LIST: ElementId = dereth_ui_screens::hud::speech_bubbles::LIST_BOX;
/// The shipped wave motion command.
const MOTION_WAVE: u32 = 0x1300_0087;

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        assert!(app.frame());
    }
}

fn app() -> App {
    let mut app = crate::common::sim_app::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dereth-slash-command-forwarding/preferences.ini"),
        ..Config::default()
    })
    .expect("required retail DATs and a headless graphics device");
    app.start_shell().expect("the UI shell");
    app.queue_ui_mode(mode::GAME_PLAY);
    frames(&mut app, 4);
    app
}

// ---------------------------------------------------------------------------------------------
// The hand: a real pointer press on the chat entry, one WM_CHAR per character, then VK_RETURN.
// ---------------------------------------------------------------------------------------------

struct Hand {
    pump: Pump,
    time: u32,
}

impl Hand {
    fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time: 400_000,
        }
    }

    fn send(&mut self, app: &mut App, message: Win32Message) {
        self.pump.dispatch(message);
        app.input_manager_mut()
            .expect("the input shell exists in a UI build")
            .on_message(message);
    }

    /// The production input path. A test that queues `UiRequest::ChatLine` cannot tell a wired
    /// path from one whose entry box the player cannot reach.
    fn submit(&mut self, app: &mut App, text: &str) {
        let bounds = {
            let shell = app.ui().expect("shell");
            let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
            let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
            let entry = shell
                .ui
                .get_child_recursive(screen.root().expect("root"), CHAT_ENTRY)
                .expect("the chat entry is in the shipped layout");
            shell.ui.screen_box(entry)
        };
        self.time += 10;
        let message = self.pump.mouse_move_message(
            f64::from((bounds.x0 + bounds.x1) / 2),
            f64::from((bounds.y0 + bounds.y1) / 2),
            self.time,
        );
        self.send(app, message);
        for down in [true, false] {
            self.time += 10;
            let message = self
                .pump
                .mouse_button_message(winit::event::MouseButton::Left, down, self.time)
                .expect("the left button is in the 0x201 block");
            self.send(app, message);
        }
        assert!(app.frame());
        for byte in text.bytes() {
            assert!(
                byte.is_ascii(),
                "this station is deliberately ASCII: {text:?}"
            );
            self.time += 10;
            self.send(
                app,
                Win32Message::new(
                    dereth_input::win32::msg::WM_CHAR,
                    usize::from(byte),
                    0,
                    self.time,
                ),
            );
        }
        assert!(app.frame());
        for (msg, lp) in [
            (dereth_input::win32::msg::WM_KEYDOWN, 0x001c_0001_u32),
            (dereth_input::win32::msg::WM_KEYUP, 0xc01c_0001_u32),
        ] {
            self.time += 10;
            self.send(app, Win32Message::new(msg, 13, lp as isize, self.time));
        }
        assert!(app.frame());
    }
}

// ---------------------------------------------------------------------------------------------
// Readers and the byte oracle
// ---------------------------------------------------------------------------------------------

/// The text of every live speech bubble — what a player actually reads, rather than what a counter
/// says was queued.
fn bubbles(app: &mut App) -> Vec<String> {
    let list = {
        let shell = app.ui().expect("shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
        shell
            .ui
            .get_child_recursive(screen.root().expect("root"), SPEW_LIST)
            .expect("the spew box list is in the shipped layout")
    };
    let shell = app.ui_mut().expect("shell");
    let kids = shell.ui.children(list);
    kids.into_iter()
        .filter_map(|h| {
            shell
                .ui
                .text_element_mut(h)
                .map(|t| t.glyphs.inq_text(false))
        })
        .collect()
}

/// The main chat window's log element, read as glyphs.
fn log_text(app: &mut App) -> String {
    let log = {
        let shell = app.ui().expect("shell");
        let root = shell.flow.current().expect("a screen").roots()[0];
        shell
            .ui
            .get_child_recursive(root, CHAT_LOG)
            .expect("the chat log is in the shipped layout")
    };
    app.ui_mut()
        .expect("shell")
        .ui
        .text_element_mut(log)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// Everything the player can read on either surface.
fn everything_on_screen(app: &mut App) -> String {
    let mut s = log_text(app);
    s.push('\n');
    s.push_str(&bubbles(app).join("\n"));
    s
}

/// The packed-string wire form: a `u16` count, the characters, and zero padding to four bytes.
fn pstr(s: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&u16::try_from(s.len()).expect("short").to_le_bytes());
    v.extend_from_slice(s.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

/// Game-action framing: header, sequence stamp, opcode, then body.
fn action(stamp: u32, opcode: u32, body: &[u8]) -> Vec<u8> {
    let mut v = vec![0xb1, 0xf7, 0x00, 0x00];
    v.extend_from_slice(&stamp.to_le_bytes());
    v.extend_from_slice(&opcode.to_le_bytes());
    v.extend_from_slice(body);
    v
}

// ---------------------------------------------------------------------------------------------
// 1. The unknown command reaches the shard
// ---------------------------------------------------------------------------------------------

/// Behaviour: chat.commands.a-verb-the-client-knows-answers-it-and-a-word-it-does-not-know-is-passed-on
///
/// `/teleloc 0xA9B40022 110.2 42.3 97.5` sends `@teleloc 0xA9B40022 110.2 42.3 97.5` as a
/// `0x0015 Talk`, and prints nothing.
///
/// Asserted one at a time:
///
/// * the wire carries the `@`-rewritten line verbatim, spacing and sigil included;
/// * the talk-event opcode is `0x0015` and the body is one packed string;
/// * the invalid-command sentence is not printed locally: neither the chat log nor the
///   speech-bubble list gains it, and neither refusal counter moves, because local failure `0x26`
///   is unreachable from a table miss.
#[test]
fn an_unknown_slash_command_is_forwarded_verbatim_and_prints_nothing() {
    let mut app = app();
    let mut hand = Hand::new();

    let refused = app.interaction().stats.chat_commands_refused;
    let unimplemented = app.interaction().stats.chat_commands_unimplemented;
    let before = everything_on_screen(&mut app);
    assert!(
        !before.contains(dereth_client_model::cmd::NOT_A_VALID_COMMAND),
        "the instrument starts clean, so a hit below is this line's: {before:?}"
    );

    hand.submit(&mut app, "/teleloc 0xA9B40022 110.2 42.3 97.5");

    // 1. The wire.
    let sent = app.interaction().last_sent.to_vec();
    assert_eq!(
        sent,
        vec![Request::Talk(dereth_protocol::comms::CommunicationTalk {
            message: "@teleloc 0xA9B40022 110.2 42.3 97.5".to_owned(),
        })],
        "a command-table miss forwards the last command line after the leading `/` has been \
         rewritten to `@`"
    );

    // 2. The bytes, composed here rather than by the encoder under test.
    let mut session = Session::new(MockTransport::new());
    assert!(
        send_request(&mut session, &sent[0]),
        "the sender has a Talk arm"
    );
    let packet = session
        .transport
        .sent
        .last()
        .expect("one datagram per request");
    assert_eq!((packet.queue, packet.ordered), (NetQueue::Weenie, true));
    assert_eq!(
        packet.payload,
        action(1, 0x0015, &pstr("@teleloc 0xA9B40022 110.2 42.3 97.5")),
        "the talk request writes opcode bytes `15 00 00 00` and one packed string"
    );

    // 3. The local invalid-command sentence is not printed. A client that printed it and also
    //    forwarded would show two lines; one that printed instead of forwarding would never reach
    //    the shard at all.
    frames(&mut app, 3);
    let after = everything_on_screen(&mut app);
    assert!(
        !after.contains(dereth_client_model::cmd::NOT_A_VALID_COMMAND),
        "a table miss bypasses the local invalid-command arm, which is reserved for a registered \
         handler that returned false. Got {after:?}"
    );
    assert_eq!(
        (
            app.interaction().stats.chat_commands_refused,
            app.interaction().stats.chat_commands_unimplemented
        ),
        (refused, unimplemented),
        "neither refusal counter may move for a word that is not in INITIALIZE_COMMANDS"
    );
}

/// `/wave` is forwarded too, and the sentence a player sees is the shard's.
///
/// `wave` is not one of `INITIALIZE_COMMANDS`' names (the table has no `/wave`-style emote
/// command), so it takes the same table-miss tail as `@teleloc`, and the client prints nothing.
///
/// The instrument is proved able to see a local refusal in the same pass: `@afk wibble` is the one
/// arm in the shipped table that really does return `false`, and it prints.
#[test]
fn slash_wave_is_forwarded_and_the_only_local_refusal_comes_from_a_registered_handler() {
    let mut app = app();
    let mut hand = Hand::new();

    hand.submit(&mut app, "/wave");
    assert_eq!(
        app.interaction().last_sent.to_vec(),
        vec![Request::Talk(dereth_protocol::comms::CommunicationTalk {
            message: "@wave".to_owned()
        })],
        "an argument-less unknown command still forwards: channel predispatch declines argc <= 0, \
         then command dispatch falls through to the talk request"
    );
    frames(&mut app, 3);
    let after = everything_on_screen(&mut app);
    assert!(
        !after.contains(dereth_client_model::cmd::NOT_A_VALID_COMMAND),
        "`/wave`'s refusal is the shard's 0x26, not the client's: {after:?}"
    );

    // The negative control. An empty answer is not a negative result until the instrument has been
    // proved able to look: the registered `@afk wibble` handler returns `false`.
    hand.submit(&mut app, "@afk wibble");
    frames(&mut app, 3);
    let after = everything_on_screen(&mut app);
    assert!(
        after.contains(dereth_client_model::cmd::NOT_A_VALID_COMMAND),
        "the reader can see the sentence when a REGISTERED handler returns false: {after:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. `*wave*` moves the body
// ---------------------------------------------------------------------------------------------

/// Behaviour: chat.pose.a-run-between-stars-is-performed-and-the-rest-of-the-line-is-spoken
///
/// Typing `*wave*` puts the wave motion into the body's own motion state.
///
/// This asserts the state the body carries -- `MotionDriver::movement.interp.raw_state.actions`,
/// the action list encoded by the `0xF61C` raw-motion-state message -- because that is the
/// observable that answers "does my character wave", and a queue entry is not a wave.
///
/// The station is a corpus login edge, not a bare local body: real terrain, real physics, then
/// `SessionEvent::PlayerCreated` plus `long-solo-play`'s own recorded `0xF745` for the player, so
/// the body wears a server-assigned id (`0x5000000A`) as it does after a real login. A body that
/// still has the placeholder `0x60000000` cannot tell "the pose reaches the player" from "the
/// pose reaches a placeholder".
#[test]
fn a_typed_pose_reaches_the_local_body_and_the_wire() {
    use dereth_animation::MotionCommand;
    use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_primitives::{LocalTime, ObjectId};
    use dereth_protocol::objects::ItemCreateObject;
    use dereth_protocol::{Message, Opcode};
    use {
        dereth_client_runtime::landblock::DEFAULT_LANDBLOCK,
        dereth_client_runtime::scene::SceneConfig,
    };

    /// long-solo-play's own character.
    const PLAYER: ObjectId = ObjectId(0x5000_000a);

    let mut app = app();
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
    .expect("real terrain and an animated body");
    frames(&mut app, 60);

    fn body(a: &App) -> std::cell::Ref<'_, dereth_animation::driver::MotionDriver> {
        a.world_state()
            .expect("a world")
            .character
            .as_ref()
            .expect("a body")
            .driver()
    }

    // The login edge: the recorded player-created and create-object events, verbatim.
    let rows: &[CorpusBlob] = &Corpus::shared("long-solo-play").blobs;
    let row = rows
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                && ObjectId(u32::from_le_bytes(
                    r.payload[4..8].try_into().expect("an id"),
                )) == PLAYER
        })
        .expect("long-solo-play creates its own character");
    let mut create = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
        .expect("the recorded 0xF745 decodes");
    let here = app
        .world_state()
        .expect("a world")
        .character
        .as_ref()
        .expect("a body")
        .position();
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
        .apply_event(&SessionEvent::PlayerCreated(PLAYER), LocalTime(1.0));
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("re-encodes"),
        },
        LocalTime(1.0),
    );
    // The create above is the login-tunnel create: `long-solo-play`'s `0xF745` for `PLAYER`
    // (idx 23, `t_rel 4.451`) carries `state = 0x00404410` (hidden, gravity, ignore collisions,
    // edge slide), and the recording unhides the body 6.8 s later with the `0xF74B Item_SetState`
    // at idx 80, `t_rel 11.239`, `state = 0x00400408`. Object creation applies that state word to
    // the player's physics body without an exemption, and while the hidden bit is set, position
    // updating skips the part array's whole animation offset, so the login edge needs both halves
    // of this state transition or it is a login that never finished.
    let unhide = rows
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == 0xf74b
                && ObjectId(u32::from_le_bytes(
                    r.payload[4..8].try_into().expect("an id"),
                )) == PLAYER
                && u32::from_le_bytes(r.payload[8..12].try_into().expect("a state word"))
                    & dereth_physics::PhysicsState::HIDDEN_PS
                    == 0
        })
        .expect("long-solo-play's recorded 0xF74B unhide for the player");
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_SET_STATE,
            body: unhide.payload[4..].to_vec(),
        },
        LocalTime(1.0),
    );
    frames(&mut app, 90);
    {
        let c = app
            .world_state()
            .expect("a world")
            .character
            .as_ref()
            .expect("a body");
        assert_eq!(
            c.object_id(),
            PLAYER,
            "the body adopted long-solo-play's recorded id, so this is a logged-in player and not the \
             `0x60000000 -- no server id yet` placeholder"
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

    assert_eq!(
        body(&app).movement.interp.raw_state.actions.len(),
        0,
        "the action queue starts empty, so a hit below is this typed line's"
    );
    assert_eq!(
        body(&app).movement.interp.interpreted_state.current_style,
        MotionCommand::NON_COMBAT,
        "the motion path refuses an emote outside NonCombat with 0x42, so the stance is a tested \
         precondition"
    );

    let mut hand = Hand::new();
    hand.submit(&mut app, "*wave*");

    // 1. The wire: the soul emote and nothing else. A bare pose has one parsed term, so it does not
    //    compose a separate talk request.
    //    `last_sent` is the frame's window, so it is read before the frames below are pumped.
    let sent = app.interaction().last_sent.to_vec();
    assert_eq!(
        sent,
        vec![Request::SoulEmote(dereth_protocol::comms::CommunicationSoulEmote {
            message: "waves.".to_owned()
        })],
        "the pose's emote from the shipped ChatPoseTable goes out as 0x01E1 and no talk is composed"
    );
    let mut session = Session::new(MockTransport::new());
    assert!(
        send_request(&mut session, &sent[0]),
        "the sender has a SoulEmote arm"
    );
    let packet = session
        .transport
        .sent
        .last()
        .expect("one datagram per request");
    assert_eq!((packet.queue, packet.ordered), (NetQueue::Weenie, true));
    assert_eq!(
        packet.payload,
        action(1, 0x01E1, &pstr("waves.")),
        "the soul-emote request writes opcode `0x01E1` little-endian and the packed message body"
    );

    // 2. The body. The current pose route queues the motion through the same input-command path the
    //    `J` key reaches; the next frames apply it to the body.
    frames(&mut app, 2);
    let waved = body(&app)
        .movement
        .interp
        .raw_state
        .actions
        .iter()
        .any(|n| n.action == MotionCommand(MOTION_WAVE));
    assert!(
        waved,
        "the local body must carry wave motion 0x13000087 after a typed `*wave*`, not merely the \
         MovementCommands queue; got {:?}",
        body(&app).movement.interp.raw_state.actions
    );
}
