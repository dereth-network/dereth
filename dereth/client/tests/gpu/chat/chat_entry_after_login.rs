//! The chat entry and the main chat log after a real world entry, set beside the same client with
//! no login. In the world, a line pushed onto the world scroll queue and a
//! `Communication_HearSpeech 0x02BB` off the wire both reach the main chat window's log element;
//! the overhead-bubble type `0x1A` goes to the speech-bubble strip and not to the chat window.
//! A typed `/teleloc <cell> <x> <y> <z>` leaves the client as a talk action `0x0015` carrying
//! `@teleloc ...` with the length and padding ACE's reader expects. A typed `*wave*` is the same
//! soul emote on both sides of a login (the chat pose table survives the world-reset edge of the
//! first login). And when the login blob restores a saved main-chat-window size (placement
//! options `0x10000088`/`0x10000089`), the log re-anchors to its last line, so the next line is
//! on the pane's bottom edge rather than below the fold: the window's resize keeps a log that was
//! at its end at its end, and one that was not keeps its offset.
//!
//! Fixture: a headless App logged in through `App::attach_replay_network`'s socket-free endpoint
//! (every blob handed to `Transport::feed` in memory; no datagram leaves the process), driven by a
//! pointer press on the chat entry and one character message per character; the log is read off
//! its element's glyphs.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::net::ClientNetwork;
use dereth_client::pump::{Pump, Win32Message};
use dereth_client::world::SceneConfig;
use dereth_client_model::Request;
use dereth_primitives::ObjectId;
use dereth_protocol::comms::{CommunicationHearSpeech, CommunicationTextboxString};
use dereth_protocol::login::LoginEnterGameServerReady;
use dereth_protocol::objects::{ItemCreateObject, LoginCreatePlayer, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::PositionWire;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::chat::interface::window as chatwin;
use dereth_ui_screens::hud::floaty::{WindowPlacement, WindowPlacements};
use dereth_ui_screens::screens::gameplay::{GamePlayScreen, PlayerSettingsView};

/// The chat log element `0x10000011`, where final-string notices append their glyph text.
const LOG: ElementId = ElementId(0x1000_0011);
/// The main chat window element.
const MAIN_CHAT_WINDOW: ElementId = ElementId(0x1000_0601);

/// Holtburg, the worked example throughout these suites.
const CELL_A: u32 = 0xA9B4_001D;
const POS_A: (f32, f32, f32) = (60.0, 80.0, 42.0);
const PLAYER_1: ObjectId = ObjectId(0x5000_0001);

/// Queue 9 carries UI messages and queue 10 carries world-object messages.
const UI_QUEUE: u16 = 9;
const SMARTBOX_QUEUE: u16 = 10;

/// The shard's own welcome, as a recording carries it. It ends in a newline, which makes the
/// at-the-end arithmetic tight.
const ACE_WELCOME: &str = "Welcome to Asheron's Call\n  powered by ACEmulator\n\nFor more information on commands supported by this server, type @acehelp\n";

/// Chat type speech (2).
const SPEECH: u32 = 2;
/// The overhead-bubble channel — the one type the main window's `0xFBFFFFFF` masks out.
const BUBBLE: u32 = 0x1A;

// ---------------------------------------------------------------------------------------------
// The socket-free peer. Copied in shape from `login/second_login.rs`, which is the established way
// to drive `App` from the wire without a socket.
// ---------------------------------------------------------------------------------------------

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "p155", "unused", 0)
            .expect("a socket-free client network");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().expect("a literal address")),
        );
        (
            Self {
                crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
                blob: 0,
            },
            net,
        )
    }

    fn send(&mut self, app: &mut App, queue: u16, bytes: Vec<u8>) {
        self.sequence += 1;
        self.blob += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_fragment(dereth_transport::Fragment::new(
                dereth_transport::FragmentHeader {
                    blob_id_low: self.blob,
                    blob_id_high: 0x8000_0000,
                    num_frags: 1,
                    blob_frag_size: 0,
                    blob_num: 0,
                    queue_id: queue,
                },
                bytes,
            ))
            .expect("one fragment fits");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("the envelope serialises");
        app.replay_network_mut()
            .expect("an explicitly socket-free endpoint")
            .session
            .transport
            .feed(&raw, None, dereth_primitives::LocalTime(0.0))
            .expect("the transport accepts its own envelope");
    }

    /// `0xF7E0 Communication_TextboxString` — the message ACE's `SystemChat` arrives as, which is
    /// what carries the welcome and the reply to `@acehelp`.
    fn system_chat(&mut self, app: &mut App, text: &str) {
        let blob = dereth_protocol::write_blob(&CommunicationTextboxString {
            text: text.to_owned(),
            text_type: 0,
        })
        .expect("0xF7E0 encodes");
        self.send(app, UI_QUEUE, blob);
    }
}

fn app() -> App {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats must be at {} (set DERETH_TEST_DAT_DIR)",
        client_dir().display()
    );
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        preferences_file: std::env::temp_dir()
            .join("dereth-chat-entry-after-login-not-created/prefs.ini"),
        dat_dir: client_dir(),
        ..Default::default()
    })
    .expect("the application comes up headless on WARP");
    app.start_shell().expect("the UI shell comes up");
    app
}

fn scene() -> SceneConfig {
    SceneConfig {
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..SceneConfig::default()
    }
}

/// The player's own `0xF745`, carrying the placement the server chose.
fn player_create(id: ObjectId, cell: u32, xyz: (f32, f32, f32)) -> Vec<u8> {
    let mut p = ObjectCreatePayload {
        id,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= flags::SETUP | flags::POSITION;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    p.physicsdesc.position = Some(PositionWire {
        objcell_id: cell,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: xyz.0,
                y: xyz.1,
                z: xyz.2,
            },
            ..Default::default()
        },
    });
    dereth_protocol::write_blob(&ItemCreateObject(p)).expect("the create encodes")
}

fn frames(app: &mut App, n: u32, what: &str) {
    for i in 0..n {
        assert!(app.frame(), "{what}: frame {i} does not end the client");
    }
}

/// One whole world entry, driven the way the client drives it — `login/second_login.rs`'s `log_on`.
fn log_on(app: &mut App, peer: &mut Peer) {
    app.replay_network_mut()
        .expect("the endpoint")
        .enter_world(PLAYER_1, "p155");
    frames(app, 1, "phase 1");
    peer.send(
        app,
        UI_QUEUE,
        dereth_protocol::write_blob(&LoginEnterGameServerReady).expect("0xF7DF"),
    );
    frames(app, 2, "phase 2");
    peer.send(
        app,
        SMARTBOX_QUEUE,
        dereth_protocol::write_blob(&LoginCreatePlayer {
            player_id: PLAYER_1,
        })
        .expect("0xF746"),
    );
    peer.send(app, SMARTBOX_QUEUE, player_create(PLAYER_1, CELL_A, POS_A));
    frames(app, 5, "the world entry");
}

/// A client that is **in the world, on the gameplay screen**, with a socket-free peer beside it.
fn in_world() -> (App, Peer) {
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(scene());
    log_on(&mut app, &mut peer);
    // Bring the gameplay screen up. The scripted login does not queue the mode by itself; the live
    // client reaches this state from the shell's in-world transition off character select.
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    frames(&mut app, 3, "the gameplay screen");
    assert!(
        app.objects_mut().world.player == Some(PLAYER_1),
        "the login put a player in the world"
    );
    (app, peer)
}

fn gameplay(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    (
        ui,
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen is active in world"),
    )
}

fn find(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped gameplay layout"))
}

/// The log's text **as drawn** — read off the element's own glyphs, not off any model.
fn drawn(app: &mut App) -> String {
    let log = find(app, LOG);
    let shell = app.ui_mut().expect("shell");
    let t = shell
        .ui
        .text_element_mut(log)
        .expect("the chat log is a text element");
    t.glyphs
        .glyphs
        .iter()
        .map(|g| char::from_u32(u32::from(g.data)).unwrap_or('\u{FFFD}'))
        .collect()
}

/// `(scroll offset, content height, view height, at vertical end)` for the log.
fn scroll_state(app: &mut App) -> (i32, i32, i32, bool) {
    let log = find(app, LOG);
    let shell = app.ui_mut().expect("shell");
    let screen = shell.ui.screen_box(log);
    let t = shell
        .ui
        .text_element_mut(log)
        .expect("the chat log is a text element");
    (
        t.scroll.y,
        t.scroll.height,
        screen.height(),
        t.is_at_vertical_end(screen),
    )
}

/// The world-scroll entry takes `(text, chat type, true, 0)` for every client-generated line in this
/// build; `Hud::drain_scroll` drains the resulting queue.
fn add_text_to_scroll(app: &mut App, text: &str, chat_type: u32) {
    app.objects_mut()
        .world
        .scroll
        .add_text_to_scroll(text, chat_type, true, 0);
}

// ---------------------------------------------------------------------------------------------
// The hand: a real pointer press on the chat entry, one `WM_CHAR` per character, then
// `VK_RETURN`, the same shape as `chat::slash_command_forwarding`'s. The same gesture is used on
// both sides of a login; a harness that queued `UiRequest::ChatLine` could not tell them apart.

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

/// The main chat window's text entry, element `0x10000016`.
const CHAT_ENTRY: ElementId = dereth_ui_screens::chat::window::ENTRY;

/// The same client with no login: `App::start_shell` then `queue_ui_mode(GAME_PLAY)`. Kept here
/// so the two sides of the login are measured by one file with one gesture.
fn not_in_world() -> App {
    let mut app = app();
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    frames(&mut app, 4, "the gameplay screen");
    assert!(
        app.objects_mut().world.player.is_none(),
        "this station is the no-login side; nothing has entered the world"
    );
    app
}

// =============================================================================================
// 1. The live path, in world
// =============================================================================================

/// Behaviour: chat.log.a-logged-in-client-draws-lines-on-the-main-window-and-follows
///
/// A line pushed onto the world scroll queue while the player is in the world is drawn on the
/// main chat window's log element.
///
/// The line is put on `dereth_client_model::scroll` exactly as every client-generated notice is,
/// and it is read back from the log element's glyphs after `App::frame` has run
/// `Hud::drain_scroll`, `Hud::drive` and the final-string notice path; no step is bypassed.
///
/// Falsified by making `Hud::drive` skip its `pending_chat` drain, or by `Hud::drain_scroll` not
/// queueing what it collects.
#[test]
fn a_line_pushed_in_world_is_drawn_on_the_main_chat_windows_log() {
    let _gpu = gpu_lock();
    let (mut app, _peer) = in_world();

    let before = drawn(&mut app);
    let lines_before = app.hud().stats.chat_lines;
    assert!(
        !before.contains("hello from p1.55"),
        "the log does not already hold the line"
    );

    add_text_to_scroll(&mut app, "You say, \"hello from p1.55\"", SPEECH);
    frames(&mut app, 3, "the line");

    let after = drawn(&mut app);
    assert!(
        after.contains("You say, \"hello from p1.55\""),
        "the line is on the log a player reads; the log holds {after:?}"
    );
    assert!(
        app.hud().stats.chat_lines > lines_before,
        "and `Hud::drive` counted it as taken rather than dropped ({} -> {}, dropped {})",
        lines_before,
        app.hud().stats.chat_lines,
        app.hud().stats.chat_lines_dropped
    );
    app.shutdown();
}

/// A `0x02BB Communication_HearSpeech` off the wire reaches the same log, in world.
///
/// This is the player's own speech echo: the network arm (`Hud::ui_event` -> `pending_chat`)
/// rather than the scroll arm. The blob goes through the real `Transport`, `Session` and `Hud`;
/// it is not handed to a receiver.
#[test]
fn a_network_speech_line_reaches_the_log_in_world() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = in_world();

    let blob = dereth_protocol::write_blob(&CommunicationHearSpeech {
        message: "hello there".to_owned(),
        sender_name: "Lark".to_owned(),
        sender_id: PLAYER_1,
        text_type: SPEECH,
    })
    .expect("0x02BB encodes");
    peer.send(&mut app, UI_QUEUE, blob);
    frames(&mut app, 3, "the speech");

    let after = drawn(&mut app);
    assert!(
        after.contains("You say, \"hello there\""),
        "the shard's echo of the player's own line is drawn; the log holds {after:?}"
    );
    app.shutdown();
}

/// The `0x1A` overhead-bubble channel goes to the speech-bubble strip and not to the chat window.
///
/// The main window's `0xFBFFFFFF` filter has bit 26 clear, while the speech-bubble receiver accepts
/// only type `0x1A`. Both receivers are offered every line; this asserts which one took it, in
/// world, on the live path.
#[test]
fn the_bubble_channel_goes_to_the_strip_and_not_to_the_chat_window() {
    let _gpu = gpu_lock();
    let (mut app, _peer) = in_world();

    let before = drawn(&mut app);
    let spew_before = app.hud().stats.spew_lines;
    let dropped_before = app.hud().stats.chat_lines_dropped;

    add_text_to_scroll(&mut app, "You have moved too far!", BUBBLE);
    frames(&mut app, 3, "the bubble line");

    assert_eq!(
        app.hud().stats.spew_lines,
        spew_before + 1,
        "the speech-bubble strip accepted the 0x1A line"
    );
    assert_eq!(
        app.hud().stats.chat_lines_dropped,
        dropped_before + 1,
        "and no chat window did -- the main window's 0xFBFFFFFF masks bit 26"
    );
    let after = drawn(&mut app);
    assert_eq!(after, before, "the log element is untouched by a 0x1A line");

    // And the split is a split, not a mute: an ordinary line still reaches the window afterwards.
    add_text_to_scroll(&mut app, "an ordinary notice", 0);
    frames(&mut app, 3, "an ordinary line");
    assert!(
        drawn(&mut app).contains("an ordinary notice"),
        "the window is still live after a bubble line"
    );
    app.shutdown();
}

// =============================================================================================
// 4. `/teleloc` — what this client actually puts on the wire, in world
// =============================================================================================

/// `/teleloc <cell> <x> <y> <z>` typed in world leaves this client as
/// talk action `"@teleloc <cell> <x> <y> <z>"`, and the datagram is exactly what ACE's reader
/// expects.
///
/// The client has no local handler for `teleloc`, so the command-table miss is its only path:
/// the stored command line is sent as talk event `0x0015` after the leading `/` is rewritten to
/// `@`, with the rest untouched.
///
/// The reader on the other side is ACE's `GameActionTalk.Handle`:
/// `clientMessage.Payload.ReadString16L()` -- a `u16` length, the bytes, then padding to the next
/// DWORD -- and `message.StartsWith("@")` is what makes it a command rather than speech. All three
/// are asserted separately, because a length that is right with the wrong padding and a padding
/// that is right with the wrong length look the same in a hex dump.
#[test]
fn a_slash_command_typed_in_world_goes_out_as_an_at_prefixed_event_talk() {
    let _gpu = gpu_lock();
    let (mut app, _peer) = in_world();
    let _ = app
        .replay_network_mut()
        .expect("the endpoint")
        .take_outgoing();

    let mut hand = Hand::new();
    hand.submit(&mut app, "/teleloc 0xA9B40022 110.2 42.3 97.5");

    // 1. The request. Dispatch rewrites the first character from `/` to `@`, and the original
    //    spacing survives because the table-miss path does not rejoin the parsed argument tokens.
    assert_eq!(
        app.interaction().last_sent.to_vec(),
        vec![Request::Talk(dereth_protocol::comms::CommunicationTalk {
            message: "@teleloc 0xA9B40022 110.2 42.3 97.5".to_owned()
        })],
        "the `/` became an `@` and nothing else changed"
    );

    // 2. The datagram, off the app's own socket-free endpoint rather than a re-encode.
    frames(&mut app, 2, "the send");
    let out = app
        .replay_network_mut()
        .expect("the endpoint")
        .take_outgoing();
    let body = out
        .iter()
        .map(|(bytes, _)| bytes.as_slice())
        .find(|b| b.windows(4).any(|w| w == [0x15, 0x00, 0x00, 0x00]))
        .expect("one datagram carries the 0x0015");
    let at = body
        .windows(4)
        .position(|w| w == [0x15, 0x00, 0x00, 0x00])
        .expect("found above");
    let text = "@teleloc 0xA9B40022 110.2 42.3 97.5";
    let len = u16::try_from(text.len()).expect("short");
    assert_eq!(
        &body[at..at + 4],
        &[0x15, 0x00, 0x00, 0x00],
        "the talk request writes opcode 0x0015 as a little-endian dword"
    );
    assert_eq!(
        &body[at + 4..at + 6],
        &len.to_le_bytes(),
        "ACE's public ReadString16L reader expects a u16 character count ({len}), not a byte offset"
    );
    assert_eq!(
        &body[at + 6..at + 6 + text.len()],
        text.as_bytes(),
        "and the bytes are the `@`-prefixed line, verbatim"
    );
    assert_eq!(
        body[at + 6],
        b'@',
        "`message.StartsWith(\"@\")` is what makes ACE treat it as a command"
    );
    let after = at + 6 + text.len();
    let pad = (4 - (after % 4)) % 4;
    assert_eq!(pad, 3, "2 + 35 needs three bytes to reach the next dword");
    assert!(
        body[after..after + pad].iter().all(|b| *b == 0),
        "the padding ReadString16L skips is zero"
    );
    app.shutdown();
}

// =============================================================================================
// 5. The rejecting test — `*wave*` after a login, and the table the login edge threw away
// =============================================================================================

/// The same `*wave*`, typed the same way, answers the same on both sides of a login.
///
/// Station 1 is the client with no login. Station 2 is the same gesture after a world entry:
/// `Interaction::on_end_character_session` runs on `SessionEvent::WorldReset` (the entry edge of
/// the first login), and it must carry `chat_pose_table` over. The pose lookup re-fetches the
/// enumerated database object `(7, 2, 0x11)` and fails when it is absent; with the table lost,
/// every `*...*` misses, stays in the line and goes out as speech for the rest of the process.
/// The shared cache owns that table; ending a character session resets only its own fields.
///
/// The two stations are asserted equal to each other, not each to a literal, because the defect
/// is a divergence between them: a mutation that broke both would have to break them
/// identically to survive.
#[test]
fn a_typed_pose_answers_the_same_before_and_after_a_world_entry() {
    let _gpu = gpu_lock();

    // Station 1 -- no login.
    let mut cold = not_in_world();
    assert!(
        cold.interaction().chat_pose_table.is_some(),
        "station 1: `App::start_shell` loaded the ChatPoseTable out of client_portal.dat"
    );
    Hand::new().submit(&mut cold, "*wave*");
    let cold_sent = cold.interaction().last_sent.to_vec();
    let cold_poses = cold.interaction().stats.poses_resolved;
    cold.shutdown();
    assert_eq!(
        cold_poses, 1,
        "station 1: the chat-pose table resolved `wave`"
    );
    assert_eq!(
        cold_sent,
        vec![Request::SoulEmote(
            dereth_protocol::comms::CommunicationSoulEmote {
                message: "waves.".to_owned()
            }
        )],
        "station 1: the third-person emote goes out as 0x01E1 and a bare pose says nothing out loud"
    );

    // Station 2 -- the same gesture after a world entry, the only thing the live client does
    // differently. A lost pose table shows as `pose_table=false`, nothing resolved, and
    // `Talk("*wave*")` on the wire.
    let (mut app, _peer) = in_world();
    assert!(
        app.interaction().chat_pose_table.is_some(),
        "station 2: the login edge's `on_end_character_session` did not take the ChatPoseTable \
         with it -- the `(7, 2, 0x11)` enumerated database object lives in the shared cache, which no ending owns"
    );
    let _ = app
        .replay_network_mut()
        .expect("the endpoint")
        .take_outgoing();
    Hand::new().submit(&mut app, "*wave*");

    assert_eq!(
        app.interaction().stats.poses_resolved,
        cold_poses,
        "station 2: the pose resolves in world exactly as it does out of it"
    );
    assert_eq!(
        app.interaction().last_sent.to_vec(),
        cold_sent,
        "station 2: and the same request goes out -- not `Talk(\"*wave*\")`"
    );
    assert_eq!(
        app.interaction().stats.pose_echoes_printed,
        1,
        "station 2: handling the local soul-emote echo printed the player-visible `You wave.`"
    );

    // The echo is on the log element a player actually reads, through the whole live path.
    frames(&mut app, 3, "the echo");
    let text = drawn(&mut app);
    assert!(
        text.contains("You wave."),
        "station 2: the local echo is drawn on the main chat window; the log holds {text:?}"
    );
    assert!(
        !text.contains("*wave*"),
        "station 2: and the asterisks were never spoken; the log holds {text:?}"
    );
    app.shutdown();
}

// =============================================================================================
// 6. The login blob's chat-window size, and the at-the-end guard it skipped
// =============================================================================================

/// A `PlayerSettingsView` carrying **one** placement row: a size for the main chat window, which is
/// what the stored placement width and height options `0x10000088` and `0x10000089` answer for any
/// player who has ever dragged or maximised that window.
fn player_module_with_main_chat_size(w: i32, h: i32) -> PlayerSettingsView {
    let mut rows = std::collections::BTreeMap::new();
    rows.insert(
        chatwin::MAIN,
        WindowPlacement {
            x: None,
            y: None,
            w: Some(w),
            h: Some(h),
            visible: None,
            title: None,
        },
    );
    PlayerSettingsView {
        placements: WindowPlacements { rows },
        ..PlayerSettingsView::default()
    }
}

/// After the login blob restores the main chat window's size, the log is still following the
/// conversation: the next line is on the pane's bottom edge, not below the fold.
///
/// The login placement restore reaches the main chat window's resize after loading the window's
/// stored width and height, and that resize re-anchors a log that was at its end.
///
/// The observable is the offset, not only the at-vertical-end result: the position-in-view test
/// `scrollY <= y && y + step <= scrollY + view` is also true of a log scrolled too far, so
/// `at_end` alone lets the defect survive. `y == content - view` is the one reading that says
/// "re-anchored to the last line". With the base element resize instead, the log's view grows
/// under an offset that was the end at the old height, and every later line is below the fold.
#[test]
fn a_chat_window_size_restored_from_the_login_blob_leaves_the_log_following() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = in_world();

    // Station 1 — the shard's welcome and enough after it to overflow the 73-pixel pane, exactly
    // as a real login does. The log follows it.
    peer.system_chat(&mut app, ACE_WELCOME);
    frames(&mut app, 3, "the welcome");
    for name in ["General", "Trade", "LFG", "Olthoi", "Society"] {
        peer.system_chat(&mut app, &format!("You have entered the {name} channel."));
        frames(&mut app, 2, "a channel notice");
    }
    let (y, content, view, at_end) = scroll_state(&mut app);
    assert!(
        content > view,
        "station 1: the log overflows its pane ({content} > {view})"
    );
    assert_eq!(
        y,
        content - view,
        "station 1: and it is anchored to its last line"
    );
    assert!(at_end, "station 1: the log is at its vertical end");

    // Station 2 — the login blob's placement restore. One call, the host's own, with a size for
    // window 8; the window and its log grow with it.
    let win_before = app
        .ui()
        .expect("shell")
        .ui
        .screen_box(find(&app, MAIN_CHAT_WINDOW));
    let log_before = app.ui().expect("shell").ui.screen_box(find(&app, LOG));
    let pm = player_module_with_main_chat_size(win_before.width(), win_before.height() + 200);
    {
        let (ui, screen) = gameplay(&mut app);
        assert!(
            !screen.layout_from_file,
            "rule 2 reads the blob only with no local layout file"
        );
        screen.update_from_player_module(ui, &pm);
    }
    frames(&mut app, 2, "the restore");
    let log_after = app.ui().expect("shell").ui.screen_box(find(&app, LOG));
    assert!(
        log_after.height() > log_before.height(),
        "station 2: the log grew with the window ({} -> {})",
        log_before.height(),
        log_after.height()
    );

    // Station 3 — the guard itself. The log was at its end across the resize, so the log re-anchored.
    let (y, content, view, at_end) = scroll_state(&mut app);
    assert_eq!(
        y,
        (content - view).max(0),
        "station 3: the log re-anchored to its last line across the restored chat-window resize \
         (content {content}, view {view})"
    );
    assert!(at_end, "station 3: and the vertical-end check agrees");

    // Station 4 -- the symptom: the next line the shard sends is visible.
    peer.system_chat(&mut app, "A line sent after the placement restore.");
    frames(&mut app, 3, "the line after the restore");
    let text = drawn(&mut app);
    assert!(
        text.contains("A line sent after the placement restore."),
        "station 4: the line is on the log at all"
    );
    let (y, content, view, at_end) = scroll_state(&mut app);
    assert!(
        at_end,
        "station 4: and the log is still following the conversation"
    );
    assert_eq!(
        y,
        (content - view).max(0),
        "station 4: on the pane's bottom edge"
    );
    app.shutdown();
}
