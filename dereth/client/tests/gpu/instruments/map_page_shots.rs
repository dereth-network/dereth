//! Frames of the classic and the modern interfaces' map pages on a world whose land is its own,
//! drawn through the client's own presentation on a hardware device, in the world after a login by
//! a socket-free peer: each page as it first opens, before the world's frame is worked out, and
//! again framed to the world's land.
//!
//! The world is the overlay folder named by `DERETH_TEST_MAP_OVERLAY` (its data files and its
//! `places.tsv`) over the retail dats, with the player at `DERETH_TEST_MAP_CELL` (a cell and a place
//! in it, `0x5D8D0022 114 38 30`; default the middle of the cell `0x7F7F0001`). Each frame is
//! written to `DERETH_TEST_MAP_SHOTS` (default: `map-pages/` in cargo's scratch folder for
//! integration tests) as `classic-whole.png`, `classic-framed.png`, `modern-whole.png` and
//! `modern-framed.png`. They assert nothing; the claims are the pages' own tests.

#![cfg(gpu)]

use crate::common::{client_dir, gpu_lock};

use dereth_client::app::App;
use dereth_client_contract::options::interface::{Interface, INTERFACE};
use dereth_client_contract::options::store;
use dereth_client_contract::PrefValue;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::scene::SceneConfig;
use dereth_primitives::ObjectId;
use dereth_protocol::login::LoginEnterGameServerReady;
use dereth_protocol::objects::{ItemCreateObject, LoginCreatePlayer, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::PositionWire;
use dereth_ui::framework::mode;

/// The input action that shows and hides the map page on either interface.
const TOGGLE_MAP_PANEL: u32 = 0x1000_0017;

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const UI_QUEUE: u16 = 9;
const SMARTBOX_QUEUE: u16 = 10;

/// The socket-free peer, in the shape `options_shots.rs` logs in with.
struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "p1136", "unused", 0)
            .expect("a socket-free network client");
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

/// Where the player stands: `DERETH_TEST_MAP_CELL`, or the middle of the world.
fn standing() -> (u32, (f32, f32, f32)) {
    let Some(text) = std::env::var_os("DERETH_TEST_MAP_CELL") else {
        return (0x7F7F_0001, (12.0, 12.0, 0.0));
    };
    let text = text.to_string_lossy().into_owned();
    let parts: Vec<&str> = text.split_whitespace().collect();
    let cell = u32::from_str_radix(parts[0].trim_start_matches("0x"), 16).expect("a cell in hex");
    let at = |i: usize| parts[i].parse::<f32>().expect("a place in meters");
    (cell, (at(1), at(2), at(3)))
}

fn player_create() -> Vec<u8> {
    let (cell, (x, y, z)) = standing();
    let mut p = ObjectCreatePayload {
        id: PLAYER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= flags::SETUP | flags::POSITION;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    p.physicsdesc.position = Some(PositionWire {
        objcell_id: cell,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 { x, y, z },
            ..Default::default()
        },
    });
    dereth_protocol::write_blob(&ItemCreateObject(p)).expect("the create encodes")
}

fn out_dir() -> std::path::PathBuf {
    std::env::var_os("DERETH_TEST_MAP_SHOTS").map_or_else(
        || std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("map-pages"),
        std::path::PathBuf::from,
    )
}

/// The frame as the device drew it, written as `<name>.png`.
fn shoot(app: &mut App, name: &str) {
    let (w, h, bgra) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame reads back");
    let rgba: Vec<u8> = bgra
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[2], p[1], p[0], 255])
        .collect();
    let dir = out_dir();
    std::fs::create_dir_all(&dir).expect("the output folder");
    let path = dir.join(format!("{name}.png"));
    let file = std::fs::File::create(&path).expect("png file");
    let mut png = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header()
        .expect("png header")
        .write_image_data(&rgba)
        .expect("png pixels");
    eprintln!("wrote {}", path.display());
}

/// A headless `App` on the retail dats under the overlay, in the world; the older portal beside the
/// dats when `classic`.
fn in_the_world(overlay: &std::path::Path, classic: bool) -> App {
    let classic_dat_dir = classic.then(|| {
        if let Some(why) = dereth_dat::testing::classic_shortfall() {
            panic!("the classic interface's files are this instrument's fixture: {why}");
        }
        dereth_dat::testing::classic_dat_dir().unwrap_or_default()
    });
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: client_dir(),
        overlay_dat_dir: Some(overlay.to_path_buf()),
        classic_dat_dir,
        width: 800,
        height: 600,
        preferences_file: std::env::temp_dir().join("dereth-map-shots-not-created/prefs.ini"),
        ..Config::default()
    })
    .unwrap_or_else(|e| panic!("a headless App on a GPU device: {e:?}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the UI shell starts: {e:?}"));
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(SceneConfig {
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        time_of_day: Some(0.5),
        ..SceneConfig::default()
    });
    app.replay_network_mut()
        .expect("the endpoint")
        .enter_world(PLAYER, "p1136");
    frames(&mut app, 1);
    peer.send(
        &mut app,
        UI_QUEUE,
        dereth_protocol::write_blob(&LoginEnterGameServerReady).expect("0xF7DF"),
    );
    frames(&mut app, 2);
    peer.send(
        &mut app,
        SMARTBOX_QUEUE,
        dereth_protocol::write_blob(&LoginCreatePlayer { player_id: PLAYER }).expect("0xF746"),
    );
    peer.send(&mut app, SMARTBOX_QUEUE, player_create());
    frames(&mut app, 2);
    // The character's description, which puts the client in the world.
    let mut event = 0xF7B0_u32.to_le_bytes().to_vec();
    event.extend_from_slice(&PLAYER.0.to_le_bytes());
    event.extend_from_slice(&1_u32.to_le_bytes());
    event.extend_from_slice(
        &dereth_protocol::Opcode::LOGIN_PLAYER_DESCRIPTION
            .0
            .to_le_bytes(),
    );
    event.extend_from_slice(
        &dereth_protocol::write_body(&dereth_protocol::login::LoginPlayerDescription::default())
            .expect("an empty description encodes"),
    );
    peer.send(&mut app, UI_QUEUE, event);
    frames(&mut app, 5);
    app.queue_ui_mode(mode::GAME_PLAY);
    frames(&mut app, 8);
    app
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.frame();
    }
}

/// Frames until the world's frame is worked out, a minute at most.
fn until_framed(app: &mut App) {
    let start = std::time::Instant::now();
    while app.hud().era.map_frame.is_none() {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(60),
            "the world's frame was not worked out: is its land its own, and does it name its places?"
        );
        app.frame();
    }
    frames(app, 8);
}

#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_TEST_MAP_OVERLAY and DERETH_TEST_MAP_SHOTS set and --ignored"]
fn draw_the_map_pages_of_a_world_framed_to_its_land() {
    let _gpu = gpu_lock();
    let overlay = std::path::PathBuf::from(
        std::env::var_os("DERETH_TEST_MAP_OVERLAY")
            .expect("DERETH_TEST_MAP_OVERLAY names the world"),
    );

    // The classic page first, as its key opens it.
    let mut app = in_the_world(&overlay, true);
    store::set_value(INTERFACE, PrefValue::Int(Interface::Classic.value()));
    for _ in 0..600 {
        app.frame();
        if app.shown_interface() == Interface::Classic {
            break;
        }
    }
    assert_eq!(app.shown_interface(), Interface::Classic);
    app.classic_ui_mut()
        .expect("the classic interface")
        .dispatch_input_action(TOGGLE_MAP_PANEL);
    frames(&mut app, 4);
    if app.hud().era.map_frame.is_none() {
        shoot(&mut app, "classic-whole");
    }
    until_framed(&mut app);
    shoot(&mut app, "classic-framed");
    drop(app);

    // The modern page.
    store::set_value(INTERFACE, PrefValue::Int(Interface::Modern.value()));
    let mut app = in_the_world(&overlay, false);
    app.ui_mut()
        .expect("a UI shell")
        .ui
        .dispatch_input_action(TOGGLE_MAP_PANEL);
    frames(&mut app, 4);
    if app.hud().era.map_frame.is_none() {
        shoot(&mut app, "modern-whole");
    }
    until_framed(&mut app);
    shoot(&mut app, "modern-framed");
}
