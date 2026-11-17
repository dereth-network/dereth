//! Frames of the modern and the classic interfaces' Client Options pages, drawn through the
//! client's own presentation on a hardware device, in the world at Holtburg after a login by a
//! socket-free peer: the modern page scrolled to its last heading and to its end, and the classic
//! page whole, in a window tall enough for all of it with the interface stretched. In a build with the remaster the last heading is the remaster's, and NVIDIA's
//! DLSS is named under it as a client that carries the library names it.
//!
//! Each frame is written to `DERETH_TEST_OPTIONS_SHOTS` (default: `options/` in cargo's scratch
//! folder for integration tests) as `modern.png`, `modern-end.png` and `classic.png`. They assert nothing; the
//! claims are the pages' own tests.

#![cfg(gpu)]

use crate::common::{client_dir, gpu_lock};

use dereth_client::app::App;
use dereth_client_contract::options::interface::{Interface, INTERFACE};
use dereth_client_contract::options::{classic, remaster, store};
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
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

/// The input action that opens the Client Options page on either interface.
const TOGGLE_CONFIG_OPTIONS_PANEL: u32 = 0x1000_001D;

/// Holtburg, and the player put there.
const CELL: u32 = 0xA9B4_001D;
const POS: (f32, f32, f32) = (60.0, 80.0, 42.0);
const PLAYER: ObjectId = ObjectId(0x5000_0001);
const UI_QUEUE: u16 = 9;
const SMARTBOX_QUEUE: u16 = 10;

/// The socket-free peer, in the shape `panels/character_options.rs` logs in with.
struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "p165", "unused", 0)
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

fn player_create() -> Vec<u8> {
    let mut p = ObjectCreatePayload {
        id: PLAYER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= flags::SETUP | flags::POSITION;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    p.physicsdesc.position = Some(PositionWire {
        objcell_id: CELL,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: POS.0,
                y: POS.1,
                z: POS.2,
            },
            ..Default::default()
        },
    });
    dereth_protocol::write_blob(&ItemCreateObject(p)).expect("the create encodes")
}

fn out_dir() -> std::path::PathBuf {
    std::env::var_os("DERETH_TEST_OPTIONS_SHOTS").map_or_else(
        || std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("options"),
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

/// A headless `App` on the retail dats on a display `size` pixels across and down, in the world
/// at Holtburg, whose client carries NVIDIA's DLSS library and no art pack; the older portal
/// beside the dats when `classic`.
fn in_the_world_at((width, height): (u32, u32), classic: bool) -> App {
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
        classic_dat_dir,
        width,
        height,
        preferences_file: std::env::temp_dir().join("dereth-options-shots-not-created/prefs.ini"),
        ..Config::default()
    })
    .unwrap_or_else(|e| panic!("a headless App on a GPU device: {e:?}"));
    remaster::set_carried(remaster::Carried {
        dlss: true,
        art_pack: false,
    });
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
        .enter_world(PLAYER, "p165");
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

#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_TEST_OPTIONS_SHOTS set and --ignored"]
fn draw_the_client_options_pages_of_the_modern_and_classic_interfaces() {
    let _gpu = gpu_lock();

    // The modern page, opened as its key opens it, scrolled to the end and then back to its last
    // heading, so the heading stands at the top of the list.
    let mut app = in_the_world_at((1024, 768), false);
    app.ui_mut()
        .expect("a UI shell")
        .ui
        .dispatch_input_action(TOGGLE_CONFIG_OPTIONS_PANEL);
    frames(&mut app, 4);
    {
        let shell = app.ui_mut().expect("a UI shell");
        let screen = shell.flow.current_mut().expect("a current screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        let s = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen is current");
        let ui = &mut shell.ui;
        let headings = s.config_page.headers;
        let list = s.config_page.option_box.as_mut().expect("the option box");
        let last = list.items.len() - 1;
        // The last heading: the last row that is a bare text row and a heading, counted down
        // from the page's own number of them.
        let heading = list
            .items
            .iter()
            .enumerate()
            .filter(|(_, h)| {
                ui.node(**h).is_some_and(|n| {
                    n.element_id() == dereth_ui::ElementId(0x1000_0216)
                        && ui.children(**h).is_empty()
                })
            })
            .map(|(i, _)| i)
            .nth(headings - 1)
            .unwrap_or(last);
        list.scroll_to_view(ui, last);
        list.scroll_to_view(ui, heading);
    }
    frames(&mut app, 4);
    shoot(&mut app, "modern");
    {
        let shell = app.ui_mut().expect("a UI shell");
        let screen = shell.flow.current_mut().expect("a current screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        let s = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen is current");
        let list = s.config_page.option_box.as_mut().expect("the option box");
        let last = list.items.len() - 1;
        list.scroll_to_view(&mut shell.ui, last);
    }
    frames(&mut app, 4);
    shoot(&mut app, "modern-end");
    drop(app);

    // The classic page, whole: the interface stretched down a tall window, the page opened as its
    // key opens it.
    let mut app = in_the_world_at((1024, 2200), true);
    store::set_value(INTERFACE, PrefValue::Int(Interface::Classic.value()));
    for _ in 0..600 {
        app.frame();
        if app.shown_interface() == Interface::Classic {
            break;
        }
    }
    assert_eq!(app.shown_interface(), Interface::Classic);
    store::set_value(classic::STRETCH_UI, PrefValue::Bool(true));
    frames(&mut app, 4);
    app.classic_ui_mut()
        .expect("the classic interface")
        .dispatch_input_action(TOGGLE_CONFIG_OPTIONS_PANEL);
    frames(&mut app, 8);
    shoot(&mut app, "classic");
    remaster::set_carried(remaster::Carried::default());
}
