//! A second login starts clean. World teardown is keyed on *entering* the world: logging off
//! leaves the world standing, and character-login phase 2 clears the squelch database, resets the
//! world with the full wipe selector (every object leaves the world, cells and landscape blocks are
//! released), flushes queued UI events, sends enter-world, restores talk focus to Say and restores
//! combat mode, before entering the network world. So the second session's body stands where its
//! own server placement says, the texture slots are handed back, a create-player after any kind of
//! ending is accepted (while a duplicate inside one session is still refused), and no run lock,
//! held command or armed Use cursor carries over. Ordinary frames never tear the world down.
//! Fixture: the retail dats and a headless `App` with a socket-free replay link fed two scripted
//! world entries through the real session, network and frame path; no datagram leaves the process.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::net::ClientNetwork;
use dereth_client::world::SceneConfig;
use dereth_primitives::ObjectId;
use dereth_protocol::login::{LoginEnterGameServerReady, LoginExecuteLogOff};
use dereth_protocol::objects::{ItemCreateObject, LoginCreatePlayer, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::PositionWire;

/// Session 1's landblock — Holtburg, the worked example throughout these suites.
const CELL_A: u32 = 0xA9B4_001D;
const POS_A: (f32, f32, f32) = (60.0, 80.0, 42.0);
/// Session 2's landblock, deliberately a different one so "the window never moved" is visible.
const CELL_B: u32 = 0xDA55_001D;
const POS_B: (f32, f32, f32) = (84.8, 99.0, 20.0);

const PLAYER_1: ObjectId = ObjectId(0x5000_0001);
const PLAYER_2: ObjectId = ObjectId(0x5000_0002);

/// Queue 9 is the UI queue and queue 10 is the world-object queue — see `dereth_client_net::client_session::testing`'s mapping.
/// Every login reply ACE sends arrives on **9**, not on the login queue; `dispatch::logon` discards
/// everything but `0xF7DE`.
const UI_QUEUE: u16 = 9;
const SMARTBOX_QUEUE: u16 = 10;

/// A socket-free peer: the packet envelope is built with the independently tested transport writer
/// and handed straight to `Transport::feed`.
struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "second-login", "unused", 0)
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
}

fn app() -> App {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats must be at {} (set DERETH_TEST_DAT_DIR)",
        client_dir().display()
    );
    // **`ui: true`, and it is load-bearing.** `App::process_logon_event_queue`'s `LoggedOff` arm
    // ends the main loop when there is no shell to go back to (`--no-ui`): there is no screen to
    // show the player and nothing to take their next click. A second login therefore only exists
    // in a build that has a character screen.
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        preferences_file: std::env::temp_dir().join("dereth-second-login-not-created/prefs.ini"),
        dat_dir: client_dir(),
        ..Default::default()
    })
    .expect("the application comes up headless on a software GPU device");
    app.start_shell().expect("the UI shell comes up");
    app
}

/// The switches `--connect --world` arms the deferred scene with, trimmed to what a body standing
/// in a block needs. `character: true` is what puts a body in the scene.
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
    // The shipped Aluvian setup, as every other suite in this crate spells it.
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

/// Frames, with the loop's own exit condition asserted at each.
///
/// **A blob is never acted on in the frame it is fed.** The client drains the UI queue *first*,
/// before the network step polls the transport — so a blob handed to `Transport::feed` is queued by
/// this frame's step 4 and delivered by the **next** frame's step 1. Every `peer.send` therefore
/// needs a frame of slack before its effect can be looked at.
fn frames(app: &mut App, n: u32, what: &str) {
    for i in 0..n {
        assert!(app.frame(), "{what}: frame {i} does not end the client");
    }
}

/// One whole world entry through the production session, network and application-frame components.
///
/// Character-login phase 1 (`0xF7C8` out), the server's `0xF7DF`, which is what
/// carries the flow into phase 2, then `0xF746` and the player's own create.
fn log_on(app: &mut App, peer: &mut Peer, id: ObjectId, cell: u32, xyz: (f32, f32, f32)) {
    app.replay_network_mut()
        .expect("the endpoint")
        .enter_world(id, "second-login");
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
        dereth_protocol::write_blob(&LoginCreatePlayer { player_id: id }).expect("0xF746"),
    );
    peer.send(app, SMARTBOX_QUEUE, player_create(id, cell, xyz));
    // `load_pending_scene` runs late in the frame and needs the create to have been applied, so
    // the landscape is built on a frame after the one that admitted it.
    frames(app, 5, "the world entry");
}

/// The server's `0xF653`, which arrives six seconds after the client request in the five recorded
/// logouts. The client clears login, logout, player-init, player-description, and player-id state,
/// disconnects the world transport, and tears the world down.
fn log_off(app: &mut App, peer: &mut Peer) {
    peer.send(
        app,
        UI_QUEUE,
        dereth_protocol::write_blob(&LoginExecuteLogOff).expect("0xF653"),
    );
    frames(app, 2, "the log-off");
    assert_eq!(
        app.replay_network_mut()
            .expect("the endpoint")
            .session
            .state(),
        dereth_client_net::client_session::SessionState::CharacterSelect,
        "the log-off reached the flow before the next entry was asked for"
    );
}

fn body_cell(app: &App) -> Option<dereth_primitives::CellId> {
    Some(app.world_scene()?.character.as_ref()?.position().cell)
}

fn body_origin(app: &App) -> Option<(f32, f32, f32)> {
    let p = app.world_scene()?.character.as_ref()?.position();
    Some((p.frame.origin.x, p.frame.origin.y, p.frame.origin.z))
}

/// The horizontal frame only.
///
/// `Character::teleport` follows simple position placement: the body is placed on the
/// ground of the cell it lands in, so `z` is the terrain's answer and not the server's number.
/// `x` and `y` are carried through verbatim, and they are what "the body is where the server said"
/// means for a landblock.
fn close(a: (f32, f32, f32), b: (f32, f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5
}

// ---------------------------------------------------------------------------------------------
// Where the body stands after a second login
// ---------------------------------------------------------------------------------------------

/// Behaviour: login.second-login.the-world-is-reset-on-entry-and-nothing-carries-over
///
/// **A second login stands the body at the second placement.**
///
/// Two logins, a clean `0xF653` between them, and the assertion is where the *body* stands after
/// the second: in session 2's landblock and position, with the world torn down on the log-off and
/// reset again on the entry. A client that kept session 1's world would leave the body at `CELL_A`.
#[test]
fn a_second_login_stands_the_body_where_the_server_says_and_not_where_the_first_one_left_it() {
    let _gpu = gpu_lock();
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(scene());

    // Station 1 — the first entry builds the world around the block the server chose.
    log_on(&mut app, &mut peer, PLAYER_1, CELL_A, POS_A);
    assert_eq!(
        body_cell(&app).map(|c| c.landblock()),
        Some(dereth_primitives::CellId(CELL_A).landblock()),
        "station 1: the first login stands the body in the server's landblock"
    );
    assert!(
        close(body_origin(&app).expect("a body"), POS_A),
        "station 1: at the server's position, not the scene's default -- {:?}",
        body_origin(&app)
    );
    assert_eq!(
        app.probe().world_resets(),
        1,
        "station 1: one world-controller reset on the entry edge"
    );

    // Station 2 — the server's `0xF653`, six seconds after the request in all five recordings.
    log_off(&mut app, &mut peer);
    assert_eq!(
        app.probe().world_resets(),
        2,
        "station 2: the log-off tears the world down"
    );
    assert!(
        app.world_scene().is_none(),
        "station 2: and no world stands behind character select"
    );
    assert_eq!(
        app.probe_mut().objects_mut().world.player,
        None,
        "station 2: nor any of its objects"
    );
    frames(&mut app, 10, "character select");
    assert!(
        app.world_scene().is_none(),
        "station 2: and nothing builds it again before the next entry"
    );

    // Station 3 — the second entry, into a different landblock.
    log_on(&mut app, &mut peer, PLAYER_2, CELL_B, POS_B);
    assert_eq!(
        app.probe().world_resets(),
        3,
        "station 3: the second entry wiped the world again"
    );
    assert_eq!(
        body_cell(&app).map(|c| c.landblock()),
        Some(dereth_primitives::CellId(CELL_B).landblock()),
        "station 3: the body is in the landblock the *second* server placement named"
    );
    assert!(
        close(body_origin(&app).expect("a body"), POS_B),
        "station 3: and at that placement's position, not session 1's frame -- {:?}",
        body_origin(&app)
    );
    app.shutdown();
}

/// The negative that keeps the station above from being satisfied by a deletion: thirty ordinary
/// frames and one unrelated object create contain no entry edge and must keep the current world.
///
/// Without this, "tear the world down whenever anything happens" would pass the test above and
/// would take the landscape out from under a player who merely received a packet.
#[test]
fn standing_in_the_world_never_tears_it_down() {
    let _gpu = gpu_lock();
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(scene());
    log_on(&mut app, &mut peer, PLAYER_1, CELL_A, POS_A);
    let resets = app.probe().world_resets();
    assert_eq!(resets, 1);

    // Thirty frames of a live world, plus an unrelated object create, move nothing.
    peer.send(
        &mut app,
        SMARTBOX_QUEUE,
        player_create(ObjectId(0x8900_0001), CELL_A, POS_A),
    );
    for i in 0..30 {
        assert!(app.frame(), "frame {i}");
    }
    assert_eq!(
        app.probe().world_resets(),
        resets,
        "no entry edge, no teardown"
    );
    assert!(app.world_scene().is_some(), "and the world is still there");
    assert_eq!(
        body_cell(&app).map(|c| c.landblock()),
        Some(dereth_primitives::CellId(CELL_A).landblock()),
        "with the body still standing in it"
    );
    app.shutdown();
}

/// The teardown hands the scene's textures back, which is the half that is not visible from the
/// body's position at all.
///
/// A second login that merely built a second scene would leak a whole world's descriptor links
/// (`tests/gpu/rendering/part_translucency.rs` records that more than four scenes on one device
/// exhaust the descriptor heap). Across these two logins, this test checks the live descriptor
/// count and a positive released-slot counter; it does not claim to cover every resource owned by
/// a scene.
#[test]
fn the_second_world_does_not_cost_a_second_heap() {
    let _gpu = gpu_lock();
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(scene());

    log_on(&mut app, &mut peer, PLAYER_1, CELL_A, POS_A);
    let after_first = app.renderer().descriptor_usage().live;
    assert!(after_first > 0, "the first world took descriptors");

    log_off(&mut app, &mut peer);
    log_on(&mut app, &mut peer, PLAYER_2, CELL_B, POS_B);
    let after_second = app.renderer().descriptor_usage().live;

    assert!(
        app.probe().world_textures_released() > 0,
        "the teardown handed slots back rather than merely dropping the scene"
    );
    // Software-device calibration against the shipped dats, both ways: with `release_textures`,
    // 309 -> 235 live descriptors and 295 slots handed back; with the scene dropped and the slots
    // kept, 309 -> **460**. The bound is `<= after_first` rather than a ratio because a ratio as
    // loose as 2x passes the leaking case too.
    assert!(
        after_second <= after_first,
        "two logins must not cost two heaps: {after_first} -> {after_second}"
    );
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The current-player clear, on the entry edge rather than one message
// ---------------------------------------------------------------------------------------------

/// **After an unclean ending, the next login still creates its player.**
///
/// The session's player id is also the world object's current-player id, which the client tests
/// before accepting a `0xF746` create-player message; a nonzero id returns the "already created"
/// refusal. The full world reset clears that field on the **entry** edge, so it is clear whatever
/// way the previous session ended.
///
/// A refused `0xF746` would mean no `SessionEvent::PlayerCreated` → no player id in `ObjectStream`
/// → `load_pending_scene` never fires → no body → `Teleport::player` stays `None`, so
/// `teleport_in_progress` is false and no session-2 tunnel (or departure) can start. The test
/// asserts the accepted player id, body landblock and per-session tunnel counter; it does not drive
/// a second logout or inspect departure pixels.
///
/// `0xF7DC Login_AccountBooted` is the ending used here because it is the shortest of the four the
/// census names; the transport drop, `0xF659` and the 110 s `ServerDied` all land on the same
/// `Flow::disconnect` and leave `player_id` exactly as untouched.
///
/// **The duplicate check itself is not weakened, and this test would not notice if it were** —
/// `the_duplicate_player_guard_still_refuses_a_second_create` below is what holds that.
#[test]
fn an_unclean_ending_does_not_stop_the_next_login_from_creating_a_player() {
    let _gpu = gpu_lock();
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(scene());

    // Station 1 — a normal first session, with its login tunnel.
    log_on(&mut app, &mut peer, PLAYER_1, CELL_A, POS_A);
    assert_eq!(
        app.objects().player(),
        Some(PLAYER_1),
        "station 1: the player exists"
    );
    assert!(
        app.teleport().tunnels_played > 0,
        "station 1: the first login played its tunnel"
    );

    // Station 2 — an ending that is **not** a clean `0xF653`. Account-booted raises no character
    // error and does not run the clean logoff path.
    peer.send(
        &mut app,
        UI_QUEUE,
        dereth_protocol::write_blob(&dereth_protocol::login::LoginAccountBooted { reason: None })
            .expect("0xF7DC"),
    );
    frames(&mut app, 2, "the boot");
    assert!(
        matches!(
            app.replay_network_mut()
                .expect("the endpoint")
                .session
                .state(),
            dereth_client_net::client_session::SessionState::Disconnected(_)
        ),
        "station 2: the session ended, and not through the log-off arm"
    );

    // Station 3 — the next login. These are the three observable links this test pins.
    log_on(&mut app, &mut peer, PLAYER_2, CELL_B, POS_B);
    assert_eq!(
        app.objects().player(),
        Some(PLAYER_2),
        "station 3: the `0xF746` was accepted, not refused as a duplicate"
    );
    assert_eq!(
        body_cell(&app).map(|c| c.landblock()),
        Some(dereth_primitives::CellId(CELL_B).landblock()),
        "station 3: so the landscape could be built, around the new placement"
    );
    // `Teleport::reset` is `*self = Self::new()`, so `tunnels_played` is **per session**: it was
    // zeroed by the boot and anything above zero here is session 2's own tunnel. With the player
    // refused it would stay at zero, because `Teleport::player` would never be set and
    // `teleport_in_progress` would be false for the whole session — on the way in, and on the way
    // out.
    assert!(
        app.teleport().tunnels_played > 0,
        "station 3: and the tunnel played -- `Teleport::player` is set, so \
         `teleport_in_progress` is true, which is what the departure sequence needs too"
    );
    app.shutdown();
}

/// **The guard that must stay, asserted so the fix above cannot have been a deletion.**
///
/// The create-player guard returns processed status **3** for a second `0xF746` *within one
/// session*; only the moment the current-player id is zeroed is the entry edge. A client that
/// dropped the check would pass the test above and fail this one.
#[test]
fn the_duplicate_player_guard_still_refuses_a_second_create() {
    let _gpu = gpu_lock();
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(scene());
    log_on(&mut app, &mut peer, PLAYER_1, CELL_A, POS_A);
    assert_eq!(app.objects().player(), Some(PLAYER_1));

    // A second `0xF746` inside the same session, naming a different id. It must change nothing.
    peer.send(
        &mut app,
        SMARTBOX_QUEUE,
        dereth_protocol::write_blob(&LoginCreatePlayer {
            player_id: PLAYER_2,
        })
        .expect("0xF746"),
    );
    frames(&mut app, 3, "the duplicate create");
    assert_eq!(
        app.objects().player(),
        Some(PLAYER_1),
        "the duplicate-player guard still refuses the second create"
    );
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The subsystem resets on the same entry edge
// ---------------------------------------------------------------------------------------------
//
// Besides the world, character-login phase 2 clears squelches before the wipe, then flushes queued
// UI events,
// sends enter-world, restores talk focus and combat mode, and enters the network world. The world
// reset itself clears degradation state, the current player, and the physics player's pointer
// before resetting cells.
//
// Within the reset, after the player is cleared and before cells are reset, the command interpreter
// is rebound to the same world object. That rebinding clears the command interpreter's player
// pointer, which makes position reporting inactive immediately when the new entry begins.
//
// A separate logoff-disable behavior clears all three command lists, releases held run, clears
// auto-run, and disables the interpreter, so the run lock, hold keys, and command lists do not
// survive a logout; see [`the_run_lock_does_not_survive_into_the_next_session`].
//
// **No datagram leaves this process.** Every test below is driven through
// `App::attach_replay_network` exactly as the five above it are; `NetLink::replay` binds no socket
// and everything the client "sends" stays in `ClientNetwork::take_outgoing`.

/// Somebody the player squelches in session 1. Not in the object table on purpose — a squelch is a
/// row in the squelch database, not a fact about a world object, and the squelch query never looks
/// the id up.
const SQUELCHED: ObjectId = ObjectId(0x5000_0943);

/// Speech text type is **2**. Type `1` means all channels and answers true only when **all 128
/// bits** are set, so a one-bit row is not squelched "for All".
const SPEECH: u32 = dereth_client_model::chat::text_type::SPEECH;

/// The `0x01F4 Communication_SetSquelchDB` a shard sends after every squelch change, as bytes.
///
/// Built through `dereth_protocol`'s own encoder and padded to four the way ACE's `GameMessage`
/// constructors pad — the case `read_body_padded` exists for.
fn squelch_db_blob(rows: &[(ObjectId, &str)]) -> Vec<u8> {
    use dereth_protocol::archive::PackedHash;
    use dereth_protocol::comms::{CommunicationSetSquelchDb, SquelchDb, SquelchInfo, VLong};
    let mut limbs = vec![0u32; 4];
    limbs[(SPEECH / 32) as usize] |= 1 << (SPEECH % 32);
    let characters = rows
        .iter()
        .map(|(id, name)| {
            (
                id.0,
                SquelchInfo {
                    squelch_msgs: VLong(limbs.clone()),
                    name: (*name).to_owned(),
                    is_zone_squelch: 0,
                },
            )
        })
        .collect();
    let m = CommunicationSetSquelchDb(SquelchDb {
        account_hash: PackedHash {
            table_size: 8,
            entries: Vec::new(),
        },
        character_hash: PackedHash {
            table_size: 8,
            entries: characters,
        },
        global_squelch_info: SquelchInfo {
            squelch_msgs: VLong(vec![0; 4]),
            name: String::new(),
            is_zone_squelch: 0,
        },
    });
    let mut blob = dereth_protocol::write_blob(&m).expect("the squelch database encodes");
    while blob.len() % 4 != 0 {
        blob.push(0);
    }
    blob
}

/// Ask whether this speaker is squelched for speech with no channel name — the same query the chat
/// router makes before it draws a line. This checks behavior rather than the row count alone.
fn is_squelched(app: &App, who: ObjectId) -> bool {
    app.objects().world.chat.is_squelched(who, "", SPEECH)
}

fn squelch_rows(app: &App) -> usize {
    app.objects().world.chat.squelch.accounts.len()
        + app.objects().world.chat.squelch.characters.len()
}

/// A hand on the keyboard: the fixture constructs key-down/key-up messages through the pump's key
/// mapping, dispatches them through that pump and gives them to the input manager. It does not use
/// the OS window procedure. This is still the input-map route by which the run lock is reached;
/// `walk_input_maps` is behind the chat typing barrier, and a test that set `auto_run` directly
/// would be measuring the field rather than the input path.
struct Keys {
    pump: dereth_client::pump::Pump,
    time_ms: u32,
}

impl Keys {
    fn new() -> Self {
        let mut pump = dereth_client::pump::Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 400_000,
        }
    }

    fn key(&mut self, app: &mut App, code: winit::keyboard::KeyCode, down: bool) {
        self.time_ms += 10;
        let m = self
            .pump
            .key_message_for(code, down, self.time_ms)
            .expect("winit maps this key to a virtual key and a scan code");
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
    }

    fn tap(&mut self, app: &mut App, code: winit::keyboard::KeyCode) {
        self.key(app, code, true);
        app.frame();
        self.key(app, code, false);
        app.frame();
    }
}

/// The gameplay screen must be up for these keyboard gestures; the scripted login in this file does
/// not queue it by itself.
fn gameplay(app: &mut App) {
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    frames(app, 2, "the gameplay screen");
}

/// **Clearing squelches is phase 2's first act.**
///
/// In the plain two-login shape (stations 1 and 2 below) the ending already takes the table with
/// it: the squelch DB lives on `dereth_client_model::World`, and `ObjectStream::reset` constructs a
/// new world.
///
/// Station 3 is what needs the entry-edge clear. `ObjectStream::reset` opens
///
/// ```text
/// if self.presences.is_empty() && self.world.player.is_none() { return; }
/// ```
///
/// and after an ending both of those are true — so the `SessionEvent::WorldReset` arm is a
/// **no-op on the entry edge**, and anything written into the `World` between one session's end
/// and the next one's beginning would survive. `0x01F4` is a *UI-queue* message whose arm in
/// `Hud::ui_event` has no player gate at all, so it is exactly such a write. The squelch clear is
/// unconditional and precedes the controller null checks, world reset, and enter-world request.
///
/// The clear also broadcasts a squelch-panel update, which is how the player sees an empty list on
/// the next character. This client has no squelch panel; `ChatState::recv_set_squelch_db` records
/// that deviation.
#[test]
fn a_squelch_set_in_the_first_session_is_not_still_in_force_in_the_second() {
    let _gpu = gpu_lock();
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(scene());

    // Station 1 — the shard's list arrives and the router acts on it.
    log_on(&mut app, &mut peer, PLAYER_1, CELL_A, POS_A);
    peer.send(
        &mut app,
        UI_QUEUE,
        squelch_db_blob(&[(SQUELCHED, "Loudmouth")]),
    );
    frames(&mut app, 2, "the squelch database");
    assert_eq!(
        squelch_rows(&app),
        1,
        "station 1: the shard's squelch list was applied"
    );
    assert!(
        is_squelched(&app, SQUELCHED),
        "station 1: and that speaker is muted"
    );

    // Station 2 — the ending. Constructing a new world inside `ObjectStream::reset` takes the
    // table, independently of the entry-edge squelch clear.
    log_off(&mut app, &mut peer);
    assert_eq!(
        squelch_rows(&app),
        0,
        "station 2: the ending already takes the table with the World it lives on"
    );

    // Station 3 — a `0x01F4` that arrives while **nobody is in the world**.
    // `ObjectStream::reset`'s `presences.is_empty() && player.is_none()` guard makes the entry-edge
    // reset a no-op, so only the unconditional squelch clear keeps this row from the next
    // character.
    peer.send(
        &mut app,
        UI_QUEUE,
        squelch_db_blob(&[(SQUELCHED, "Loudmouth")]),
    );
    frames(&mut app, 2, "a squelch list with nobody in the world");
    assert_eq!(
        squelch_rows(&app),
        1,
        "station 3: and it was applied, with no player to gate it"
    );

    // Station 4 — the next login.
    log_on(&mut app, &mut peer, PLAYER_2, CELL_B, POS_B);
    assert_eq!(
        squelch_rows(&app),
        0,
        "station 4: the unconditional entry-edge squelch clear leaves the next character with an empty table whatever was written while nobody was in the world"
    );
    assert!(
        !is_squelched(&app, SQUELCHED),
        "station 4: and the router does not refuse their lines on the last character's list"
    );
    app.shutdown();
}

/// **Character-login phase 2 restores talk focus to value 1.**
///
/// `1` is `TalkFocus::All`, the Say channel. The player who left session 1 talking on Allegiance
/// must not still be talking on Allegiance when a different character logs in — every line they
/// type would go to a channel that character may not even be in.
///
/// The fixture queues `UiRequest::SetTalkFocus` directly; it does not click the main-chat drop-down.
/// That is the same request the drop-down raises, and `Interaction::run_ui_requests` owns its current
/// consumer path.
#[test]
fn the_talk_focus_goes_back_to_say_on_the_next_login() {
    use dereth_client_model::chat::TalkFocus;
    let _gpu = gpu_lock();
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(scene());

    log_on(&mut app, &mut peer, PLAYER_1, CELL_A, POS_A);
    app.probe_mut().interaction_mut().queue(
        Vec::new(),
        vec![dereth_ui_screens::view::UiRequest::SetTalkFocus {
            focus: TalkFocus::Allegiance as u32,
        }],
    );
    frames(&mut app, 2, "the talk-focus drop-down");
    assert_eq!(
        app.objects().world.chat.talk_focus,
        TalkFocus::Allegiance,
        "station 1: the drop-down moved the focus"
    );

    log_off(&mut app, &mut peer);
    log_on(&mut app, &mut peer, PLAYER_2, CELL_B, POS_B);

    assert_eq!(
        app.objects().world.chat.talk_focus,
        TalkFocus::All,
        "station 2: the entry edge puts the next character back on Say"
    );
    app.shutdown();
}

/// **The command-interpreter disable behavior clears the run lock a new character would otherwise
/// inherit.**
///
/// Auto-run in session 1, log out, log in: without the disable the new character would start
/// *already running*, because `MovementCommands::lists.auto_run` would still be set and movement
/// application puts `WalkForward` in the base slot for as long as it is. The body would walk away
/// from the placement the shard just gave it, and the position reporter would say so.
///
/// Driven by the constructed pump and input-manager key-message path: `DIK_Q` is action `0x30
/// Autorun`.
#[test]
fn the_run_lock_does_not_survive_into_the_next_session() {
    let _gpu = gpu_lock();
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(scene());

    log_on(&mut app, &mut peer, PLAYER_1, CELL_A, POS_A);
    gameplay(&mut app);
    let mut keys = Keys::new();
    keys.tap(&mut app, winit::keyboard::KeyCode::KeyQ);
    assert!(
        app.probe().char_input().auto_run,
        "station 1: DIK_Q latched the run lock"
    );
    assert!(
        app.probe().char_input().forward,
        "station 1: and the character is walking on it"
    );

    log_off(&mut app, &mut peer);
    log_on(&mut app, &mut peer, PLAYER_2, CELL_B, POS_B);

    assert!(
        !app.probe().movement_commands().lists.auto_run,
        "station 2: the command-interpreter disable path cleared the run lock"
    );
    assert!(
        !app.probe().char_input().auto_run && !app.probe().char_input().forward,
        "station 2: so the new character is standing still, not running out of the landblock"
    );
    assert!(
        app.probe().movement_commands().lists.substate.is_empty(),
        "station 2: the command-interpreter disable path emptied the substate list with it"
    );
    app.shutdown();
}

/// **The next entry clears an armed Use cursor.**
///
/// The Use cursor is a *mode*: setting target mode arms it and the next click in the
/// world spends it. A mode that outlives its session means the first click a new character makes
/// in the world is a *use*, on an object they have not chosen, with a cursor that says so.
///
/// The next entry calls `Interaction::on_end_character_session`, which clears this target mode. The fixture queues `UiRequest::SetTargetMode` directly rather than clicking the
/// toolbar; it is the same request the toolbar raises and the targeted-use fixture exercises.
#[test]
fn the_use_cursor_is_not_still_armed_in_the_next_session() {
    use dereth_client::interaction::TargetMode;
    let _gpu = gpu_lock();
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(scene());

    log_on(&mut app, &mut peer, PLAYER_1, CELL_A, POS_A);
    app.probe_mut().interaction_mut().queue(
        Vec::new(),
        vec![dereth_ui_screens::view::UiRequest::SetTargetMode(
            dereth_ui_screens::view::TargetMode::Use,
        )],
    );
    frames(&mut app, 2, "the toolbar's Use button");
    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::Use,
        "station 1: the toolbar armed the use cursor"
    );

    log_off(&mut app, &mut peer);
    log_on(&mut app, &mut peer, PLAYER_2, CELL_B, POS_B);

    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::None,
        "station 2: the next character does not inherit an armed target mode"
    );
    app.shutdown();
}
