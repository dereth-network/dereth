//! The radar coordinate strip follows the player's cell: a teleport into another landblock
//! (`0xA9B4` to `0xA9B6`, 42.4N to 44.0N) and a walk across a land-cell boundary (y = 96, 42.4N
//! to 42.5N) both move it, while a walk inside one cell leaves it unchanged. The read-out is
//! computed from the cell id alone, with no origin interpolation, so its resolution is one
//! 24 m land cell (+0.1 per cell); the radar polls it each update rather than on a teleport
//! notice. The tests read the strip's stored glyph text, not its pixels.
//! Fixture: the retail dats, a headless `App` on the gameplay screen, and a socket-free peer that
//! feeds a synthetic login and a synthetic position body (both stamps advanced, flags 0, no
//! 0xF751) through the in-memory transport.
#![cfg(gpu)]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::scene::SceneConfig;
use dereth_primitives::ObjectId;
use dereth_protocol::login::LoginEnterGameServerReady;
use dereth_protocol::movement::{MovementPositionEvent, PositionPack};
use dereth_protocol::objects::{ItemCreateObject, LoginCreatePlayer, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::{Origin, PositionWire};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

/// Holtburg's landblock — the worked example throughout these suites.
const CELL_A: u32 = 0xA9B4_001D;
const POS_A: (f32, f32, f32) = (60.0, 80.0, 42.0);
/// Two landblocks north: `0xB6 - 0xB4 = 2` blocks, i.e. 16 land cells, i.e. `+1.6N`.
const CELL_B: u32 = 0xA9B6_0001;
const POS_B: (f32, f32, f32) = (60.0, 80.0, 42.0);
const PLAYER_1: ObjectId = ObjectId(0x5000_0001);

/// The two read-outs, as literals: a test that reads a constant back through the symbol it was
/// written through cannot detect a wrong constant, so neither of these is computed.
///
/// `A`: outside-cell normalization puts `(60, 80)` in cell `0x14` of `0xA9B4` — `lx = 1352 + 2 = 1354`,
/// `ly = 1440 + 3 = 1443`, so `(42.4, 33.5)`.
const READ_OUT_A: &str = "42.4N, 33.5E";
/// `B`: the same cell index in `0xA9B6` — `ly = 1456 + 3 = 1459`, so `(44.0, 33.5)`.
const READ_OUT_B: &str = "44.0N, 33.5E";
/// One land cell north of `A`: the body walks over `y = 96` into cell `0x15`, `ly = 1444`.
const READ_OUT_A_NORTH: &str = "42.5N, 33.5E";

/// Queue 9 carries UI messages and queue 10 carries world-object messages.
const UI_QUEUE: u16 = 9;
const SMARTBOX_QUEUE: u16 = 10;

// ---------------------------------------------------------------------------------------------
// The socket-free peer follows `dereth/client/tests/dat/chat/chat_entry_after_login.rs`
// and `login::second_login`,
// which is the established way to drive `App` from the wire without a socket.
// ---------------------------------------------------------------------------------------------

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "p157", "unused", 0)
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

    /// Synthetic 0xF748 forced-position case: both stamps advance to ts, flags remain 0.
    /// The encoded body enters through the in-memory transport. No 0xF751 precedes it here;
    /// this fixture is deliberately distinct from ACE's ordinary non-admin position convention.
    fn teleport(&mut self, app: &mut App, cell: u32, xyz: (f32, f32, f32), ts: u16) {
        let m = MovementPositionEvent {
            id: PLAYER_1,
            position: PositionPack {
                flags: 0,
                origin: Origin {
                    objcell_id: cell,
                    origin: dereth_protocol::types::Vec3 {
                        x: xyz.0,
                        y: xyz.1,
                        z: xyz.2,
                    },
                },
                orientation: dereth_protocol::types::Quat::default(),
                velocity: None,
                placement_id: None,
                instance_timestamp: 1,
                position_timestamp: ts,
                teleport_timestamp: ts,
                force_position_timestamp: 0,
            },
        };
        let blob = dereth_protocol::write_blob(&m).expect("the position event encodes");
        self.send(app, SMARTBOX_QUEUE, blob);
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
            .join("dereth-radar-coordinates-not-created/prefs.ini"),
        dat_dir: client_dir(),
        ..Default::default()
    })
    .expect("the application comes up with a headless GPU device");
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

/// Synthetic player 0xF745 with explicit setup and placement supplied by this fixture.
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

/// Synthetic entry handshake and create bodies fed through the socket-free transport, with
/// fixed frame intervals; this does not exercise a real server or every login stage.
fn log_on(app: &mut App, peer: &mut Peer) {
    app.replay_network_mut()
        .expect("the endpoint")
        .enter_world(PLAYER_1, "p157");
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
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    frames(&mut app, 3, "the gameplay screen");
    assert!(
        app.probe_mut().objects_mut().world.player == Some(PLAYER_1),
        "the login put a player in the world"
    );
    (app, peer)
}

/// Read stored glyph data from the combined-coordinate text handle bound during radar setup
/// from attribute 0x10000036. This inspects the text model, not its final pixels or visibility.
fn read_out(app: &mut App) -> String {
    let shell = app.ui_mut().expect("shell");
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    let g = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen is active in world");
    let h = g
        .radar
        .combined_coords
        .expect("the combined-coordinate text handle is bound during radar initialization");
    let t = shell
        .ui
        .text_element_mut(h)
        .expect("the coordinate strip is a text element");
    t.glyphs
        .glyphs
        .iter()
        .map(|g| char::from_u32(u32::from(g.data)).unwrap_or('\u{FFFD}'))
        .collect()
}

/// Current character body cell: the input to the coordinate calculation. Missing scene or
/// character returns 0 here; the tests establish the expected nonzero cell separately.
fn body_cell(app: &App) -> u32 {
    app.world_state()
        .and_then(|w| w.character.as_ref())
        .map_or(0, |c| c.position().cell.0)
}

// ---------------------------------------------------------------------------------------------
// 1. The teleport edge
// ---------------------------------------------------------------------------------------------

/// **A teleport into another landblock moves the read-out.**
///
/// Falsified by deleting `self.coords = player_coords(v.position.cell)` from `Hud::sync`: the
/// strip stays at `42.4N, 33.5E` instead of `44.0N, 33.5E`.
#[test]
fn a_teleport_into_another_landblock_moves_the_radar_read_out() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = in_world();
    frames(&mut app, 3, "settle");
    assert_eq!(
        read_out(&mut app),
        READ_OUT_A,
        "the strip shows the cell the login placed him in"
    );
    assert_eq!(
        body_cell(&app),
        0xA9B4_0014,
        "outside-cell normalization put (60, 80) in cell 0x14"
    );

    peer.teleport(&mut app, CELL_B, POS_B, 1);
    frames(&mut app, 10, "the teleport");

    assert_eq!(
        body_cell(&app),
        0xA9B6_0014,
        "the body is in the destination landblock"
    );
    assert_eq!(
        read_out(&mut app),
        READ_OUT_B,
        "and the strip followed it, in the same frame loop"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The walk
// ---------------------------------------------------------------------------------------------

/// Behaviour: panels.radar.the-coordinate-read-out-follows-the-bodys-cell
///
/// Walk across a land-cell boundary and check the glyph text at the sampled frame endpoints.
/// The intermediate y interval (88,96) is 8..16 m north of the starting 80, within cell 0x14;
/// another 120 frames must reach cell 0x15 and its literal read-out. Because the read-out is
/// computed from the cell id only, motion inside the cell does not change it. Deleting the
/// coordinate update from `Hud::sync` fails this test at its `assert_ne`.
#[test]
fn walking_across_a_land_cell_boundary_moves_the_radar_read_out() {
    use winit::keyboard::KeyCode;

    let _gpu = gpu_lock();
    let (mut app, _peer) = in_world();
    frames(&mut app, 3, "settle");
    assert_eq!(read_out(&mut app), READ_OUT_A, "the login station");
    assert_eq!(body_cell(&app), 0xA9B4_0014);

    // Feed normalized Q down/up through Pump and InputManager to toggle autorun. This uses
    // the application's message path directly, not OS keyboard dispatch or a physical key.
    let mut pump = dereth_desktop::pump::Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let mut t = 300_000u32;
    let mut key =
        |app: &mut App, code: KeyCode, down: bool, pump: &mut dereth_desktop::pump::Pump| {
            t += 10;
            let m = pump
                .key_message_for(code, down, t)
                .expect("winit maps this key");
            pump.dispatch(m);
            if let Some(input) = app.input_manager_mut() {
                input.on_message(m);
            }
        };
    key(&mut app, KeyCode::KeyQ, true, &mut pump);
    frames(&mut app, 1, "autorun down");
    key(&mut app, KeyCode::KeyQ, false, &mut pump);

    // At the intermediate sample the body is still inside cell 0x14 (y is under 96); the strip must not
    // have moved: this is the control, and without it "it changed at some point" would be the
    // whole reading.
    frames(&mut app, 120, "running inside the cell");
    let y = app
        .world_state()
        .and_then(|w| w.character.as_ref())
        .map_or(0.0, |c| c.position().frame.origin.y);
    assert!(
        y > 88.0 && y < 96.0,
        "the body has moved but has not left cell 0x14 yet (y = {y})"
    );
    assert_eq!(
        read_out(&mut app),
        READ_OUT_A,
        "the sampled walk inside one land cell leaves the read-out unchanged"
    );

    // And past `y = 96` it is cell 0x15, one cell north.
    frames(&mut app, 120, "running over the boundary");
    assert_eq!(
        body_cell(&app),
        0xA9B4_0015,
        "the body crossed into the next land cell"
    );
    assert_ne!(
        read_out(&mut app),
        READ_OUT_A,
        "the strip is not frozen at the login value"
    );
    assert_eq!(
        read_out(&mut app),
        READ_OUT_A_NORTH,
        "one land cell north is +0.1N"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. Retail read-outs, as arithmetic
// ---------------------------------------------------------------------------------------------

/// **`42.2N, 33.7E` is the read-out for cell `0xA9B40022`, and `42.2N, 33.8E` for `0xA9B4002A`.**
///
/// Both cells have retail readings: `0xA9B40022` is a teleport destination at
/// (110.20, 42.30, 97.50), and `0xA9B4002A [133.168503 27.490582 94.005005]` is a retail `@loc`
/// taken while the strip read `42.2N, 33.8E` (one cell east, hence +0.1E). The arithmetic:
///
/// ```text
/// lx = ((0xA9B40022 >> 21) & 0x7F8) + ((0x22 - 1) >> 3) = 1352 + 4 = 1356
/// ly = (((0xA9B40022 >> 16) & 0xFF) << 3) + ((0x22 - 1) & 7) = 1440 + 1 = 1441
/// N  = (1441 - 1024) * 0.1 + 0.5 = 42.2
/// E  = (1356 - 1024) * 0.1 + 0.5 = 33.7
/// ```
///
/// Falsified by changing either subtrahend or the `0.1` in `hud::player_coords`.
#[test]
fn the_read_out_is_right_for_the_cell_the_shard_sent() {
    assert_eq!(
        dereth_client_runtime::hud::player_coords(dereth_primitives::CellId(0xA9B4_0022)),
        Some((42.2, 33.7)),
        "the cell of the recorded teleport destination"
    );
    assert_eq!(
        dereth_client_runtime::hud::player_coords(dereth_primitives::CellId(0xA9B4_002A)),
        Some((42.2, 33.8)),
        "the cell of the retail @loc reading, whose strip read 42.2N, 33.8E"
    );
    // One land cell east is `+0.1E` and one land cell north is `+0.1N`: the read-out's resolution
    // is 24 m, which is the whole of why a teleport inside one cell cannot move it.
    let (n0, e0) =
        dereth_client_runtime::hud::player_coords(dereth_primitives::CellId(0xA9B4_0022))
            .expect("cell");
    let (n1, e1) =
        dereth_client_runtime::hud::player_coords(dereth_primitives::CellId(0xA9B4_0023))
            .expect("cell");
    assert!(
        (n1 - n0 - 0.1).abs() < 1e-4,
        "cell +1 in the low three bits is one cell north"
    );
    assert!((e1 - e0).abs() < 1e-6, "and no change east");
}
