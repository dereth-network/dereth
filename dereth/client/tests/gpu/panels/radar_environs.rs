//! What `Admin_Environs 0xEA60` does on screen: preset 6 blanks the radar's target blips, fogs
//! the landscape and sky to black and ramps the override in over the landscape ticks; an unknown
//! value is a no-op; the evolved override survives a scene replacement; and preset 0 restores the
//! blips and clears the override.
//! Fixture: the retail dats and a headless `App` on the gameplay screen, fed encrypted queue-9
//! bytes by a socket-free peer. The player and target arrive through the normal object path; only
//! their descriptive radar fields are seeded afterwards.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir_required as client_dir;
use crate::common::gpu_lock;
use dereth_client::world::{SceneReads, SceneWrites};

use dereth_client::{app::App, config::Config, net::ClientNetwork, world::SceneConfig};
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_primitives::{LocalTime, ObjectId, Quat, Vec3};
use dereth_protocol::{
    admin::AdminEnvirons,
    objects::{ItemCreateObject, LoginCreatePlayer, ObjectCreatePayload},
};
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

const PLAYER: ObjectId = ObjectId(0x5000_0EA6);
const TARGET: ObjectId = ObjectId(0x5000_0EA7);

struct TempDir(dereth_dat::testing::ScratchDir);

impl TempDir {
    fn new() -> Self {
        Self(
            dereth_dat::testing::ScratchDir::new("radar-environs")
                .expect("create disposable directory"),
        )
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        self.0
            .cleanup()
            .expect("remove the disposable profile directory");
    }
}

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "radar-environs", "unused", 0)
            .expect("a socket-free client net");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().expect("peer address")),
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
            .expect("one fragment");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("an encrypted datagram");
        app.replay_network_mut()
            .expect("the replay endpoint")
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .expect("the datagram is admitted");
    }

    fn environs(&mut self, app: &mut App, option: i32) {
        self.send(
            app,
            9,
            dereth_protocol::write_blob(&AdminEnvirons {
                environ_option: option,
            })
            .expect("Admin_Environs encodes"),
        );
    }
}

fn settle(app: &mut App) {
    for _ in 0..6 {
        assert!(app.frame());
    }
}

fn radar_fill_count(app: &App) -> usize {
    let shell = app.ui().expect("the gameplay UI");
    let screen: &dyn std::any::Any = shell.flow.current().expect("the gameplay screen");
    let root = screen
        .downcast_ref::<GamePlayScreen>()
        .and_then(GamePlayScreen::root)
        .expect("the gameplay root");
    let radar = shell
        .ui
        .get_child_recursive(root, window::RADAR)
        .expect("the shipped radar");
    shell
        .ui
        .node(radar)
        .expect("the radar node")
        .region
        .surface_fills
        .len()
}

fn scene_config() -> SceneConfig {
    SceneConfig {
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: false,
        game_time: Some(303_449_042.0),
        ..SceneConfig::default()
    }
}

fn setup() -> (TempDir, App, Peer) {
    let temp = TempDir::new();
    let mut app = App::new(Config {
        ui: true,
        headless: true,
        sound: false,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        preferences_file: temp.0.path().join("prefs.ini"),
        ..Config::default()
    })
    .expect("the WARP application starts");
    app.start_shell().expect("the UI shell starts");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.load_static_scene(scene_config())
        .expect("the retail scene loads");
    settle(&mut app);

    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("the socket-free endpoint attaches");
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&LoginCreatePlayer { player_id: PLAYER })
            .expect("F746 encodes"),
    );

    let scene = app.world_scene().expect("the retail world");
    let camera = scene.camera.frame();
    let block = scene.viewer_block().expect("the viewer block");
    for (id, x) in [(PLAYER, -2.0), (TARGET, 2.0)] {
        let mut payload = ObjectCreatePayload {
            id,
            ..ObjectCreatePayload::default()
        };
        payload.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP
            | dereth_protocol::types::physicsdesc::flags::POSITION;
        payload.physicsdesc.setup_id = Some(0x0200_0001);
        payload.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
            objcell_id: ((block.0 as u32) << 24) | ((block.1 as u32) << 16) | 1,
            frame: dereth_protocol::types::Frame {
                origin: dereth_physics::math::localtoglobal(&camera, Vec3::new(x, 18.0, -1.0))
                    .into(),
                orientation: Quat::IDENTITY.into(),
            },
        });
        payload.physicsdesc.timestamps.instance = 1;
        peer.send(
            &mut app,
            10,
            dereth_protocol::write_blob(&ItemCreateObject(payload)).expect("F745 encodes"),
        );
    }
    settle(&mut app);
    assert_eq!(
        app.objects().player(),
        Some(PLAYER),
        "F746 installed the current player"
    );
    for (id, name) in [(PLAYER, "Local Player"), (TARGET, "Radar Target")] {
        let w = app
            .probe_mut()
            .objects_mut()
            .world
            .weenie_mut(id)
            .expect("the encoded object exists");
        w.pwd.name = name.to_owned();
        w.pwd.obj_type = item_type::CREATURE;
        w.pwd.bitfield = bitfield::PLAYER;
        w.pwd.radar_enum = Some(4);
    }
    settle(&mut app);
    (temp, app, peer)
}

/// Behaviour: panels.radar.environs-six-blanks-the-radar-and-zero-restores-it
///
/// Admin-environs value 6 installs preset 6 and blanks the radar; value 0 disables the override
/// and reaches the common unblank store. The observable is the fills on the shipped radar: the nine
/// centre-marker pixels remain, while the five-pixel target blip disappears and returns.
#[test]
fn encoded_environs_six_blanks_the_radar_and_zero_restores_it() {
    let _gpu = gpu_lock();
    let (_temp, mut app, mut peer) = setup();
    // F746 starts the independently covered portal transition. End only that model so the world
    // remains drawn for this environment station.
    app.probe_mut().teleport_mut().reset();
    settle(&mut app);
    assert_eq!(
        radar_fill_count(&app),
        14,
        "target cross plus the local nine-pixel marker"
    );

    // Unknown values change neither the landscape override nor the radar flag.
    peer.environs(&mut app, 7);
    settle(&mut app);
    assert_eq!(
        radar_fill_count(&app),
        14,
        "an unknown Admin_Environs value is a no-op"
    );

    peer.environs(&mut app, 6);
    settle(&mut app);
    assert_eq!(
        radar_fill_count(&app),
        9,
        "preset 6 did not blank the target blip"
    );

    // Drive only the existing landscape tick consumer. Both its light and fog arms advance the
    // one native transition; after fourteen separated ticks the preset is installed exactly.
    for tick in 1..=14 {
        app.world_scene_mut().expect("the landscape").update(
            Default::default(),
            Default::default(),
            LocalTime(100.0 * f64::from(tick)),
            0.0,
        );
    }
    let scene = app.world_scene().expect("the landscape");
    assert_eq!(scene.landscape_lighting().ambient_level, 0.8);
    assert_eq!(scene.landscape_lighting().ambient_color, [150, 150, 150]);
    assert_eq!(
        scene.view_params(800, 600).fog,
        dereth_render::camera::FogParams {
            color: 0x6400_0000,
            near: 0.0,
            far: 40.0,
            enabled: true,
        },
        "preset 6 did not reach the existing per-frame fog constants",
    );
    let evolved = scene.environment_override_transition();
    assert!(
        evolved >= 1.0,
        "both native tick consumers did not complete the shared ramp"
    );

    // Sky drawing temporarily enables the override fog for both passes. The central upper
    // band is above this fixed camera's horizon, while x=240..560 excludes the left chat stack and
    // right radar/panel column. Its sky geometry lies far beyond preset 6's 40-unit FOGEND and
    // must therefore reach black in the actual WARP render, not merely in ViewParams.
    assert!(app.frame());
    let (width, height, pixels) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the override frame");
    assert_eq!((width, height), (800, 600));
    let mut sampled = 0_usize;
    let mut black = 0_usize;
    for y in 10_usize..180 {
        for x in 240_usize..560 {
            let i = (y * width as usize + x) * 4;
            sampled += 1;
            black += usize::from(pixels[i] <= 2 && pixels[i + 1] <= 2 && pixels[i + 2] <= 2);
        }
    }
    eprintln!("preset-6 fogged sky: {black}/{sampled} black pixels in the central upper band");
    assert!(
        black * 4 > sampled * 3,
        "the sky draw keys did not receive Admin_Environs override fog",
    );

    // These are process globals in retail. Replacing the WorldScene must transfer the evolved
    // state, not replay option 6 and restart its transition at zero.
    app.load_static_scene(scene_config())
        .expect("the replacement landscape loads");
    assert_eq!(
        app.world_state()
            .expect("the replacement landscape")
            .environment_override_transition(),
        evolved,
        "scene replacement restarted the process-owned transition",
    );
    peer.environs(&mut app, 7);
    settle(&mut app);
    assert_eq!(
        app.world_state()
            .expect("the replacement landscape")
            .environment_override_transition(),
        evolved,
        "an unknown option changed the retained override",
    );

    peer.environs(&mut app, 0);
    settle(&mut app);
    assert_eq!(
        radar_fill_count(&app),
        14,
        "preset 0 did not restore the target blip"
    );
    assert_eq!(
        app.world_state()
            .expect("the replacement landscape")
            .environment_override_transition(),
        0.0,
        "preset 0 did not clear the process-owned override",
    );
    app.shutdown();
}
