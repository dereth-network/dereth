//! Player Monarch quality updates reach the radar's allegiance shape. The radar panel listens for
//! the player's InstanceID property `0x1A` and recomputes every radar row when it changes, so a
//! change of the local monarch changes another player's blip shape.
//! Fixture: the retail dats and a headless `App` on the gameplay screen, fed the private and
//! public wire forms by a socket-free peer; the shipped radar element is read after the frame's
//! HUD projection. The target is created and positioned through the normal object path; only its
//! descriptive radar fields are filled directly.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir_required as client_dir;
use crate::common::gpu_lock;

use dereth_client::{app::App, config::Config, net::ClientNetwork, world::SceneConfig};
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_client_model::{StatKey, StatType, StatValue};
use dereth_primitives::{LocalTime, ObjectId, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::qualities as wire;
use dereth_protocol::Opcode;
use dereth_ui_screens::mapradar::radar::BlipShape;
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const TARGET: ObjectId = ObjectId(0x5000_0022);
const MONARCH: ObjectId = ObjectId(0x5000_00AA);
const OTHER_MONARCH: ObjectId = ObjectId(0x5000_00BB);
const MONARCH_PROPERTY: u32 = 0x1A;

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
    stamp: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new(
            "127.0.0.1:19100",
            7304,
            "radar-allegiance-shape",
            "unused",
            0,
        )
        .expect("a socket-free replay net");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19100".parse().expect("addr")),
        );
        (
            Self {
                crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
                blob: 0,
                stamp: 0,
            },
            net,
        )
    }

    fn event(&mut self, app: &mut App, inner: Vec<u8>) {
        self.stamp += 1;
        let mut blob = 0xF7B0_u32.to_le_bytes().to_vec();
        blob.extend_from_slice(&PLAYER.0.to_le_bytes());
        blob.extend_from_slice(&self.stamp.to_le_bytes());
        blob.extend_from_slice(&inner);
        self.send(app, 9, blob);
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
            .expect("a datagram");
        app.replay_network_mut()
            .expect("the replay endpoint")
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .expect("fed only to the socket-free endpoint");
    }
}

fn settle(app: &mut App) {
    for _ in 0..6 {
        app.frame();
    }
}

fn private_monarch(sequence: u8, value: ObjectId) -> Vec<u8> {
    let mut out = Opcode::QUALITIES_PRIVATE_UPDATE_INSTANCE_ID
        .0
        .to_le_bytes()
        .to_vec();
    out.extend_from_slice(
        &dereth_protocol::write_body(&wire::QualitiesPrivateUpdateInstanceId(
            wire::PrivateUpdate {
                sequence,
                property_id: MONARCH_PROPERTY,
                value,
            },
        ))
        .expect("private InstanceID update"),
    );
    out
}

fn public_monarch(sequence: u8, value: ObjectId) -> Vec<u8> {
    let mut out = Opcode::QUALITIES_UPDATE_INSTANCE_ID
        .0
        .to_le_bytes()
        .to_vec();
    out.extend_from_slice(
        &dereth_protocol::write_body(&wire::QualitiesUpdateInstanceId(wire::PublicUpdate {
            sequence,
            object: PLAYER,
            property_id: MONARCH_PROPERTY,
            value,
        }))
        .expect("public InstanceID update"),
    );
    out
}

fn remove_monarch(sequence: u8) -> Vec<u8> {
    let mut out = Opcode::QUALITIES_PRIVATE_REMOVE_INSTANCE_IDEVENT
        .0
        .to_le_bytes()
        .to_vec();
    out.extend_from_slice(
        &dereth_protocol::write_body(&wire::QualitiesPrivateRemoveInt(wire::PrivateRemove {
            sequence,
            property_id: MONARCH_PROPERTY,
        }))
        .expect("private InstanceID remove"),
    );
    out
}

fn setup() -> (App, Peer) {
    let mut app = App::new(Config {
        ui: true,
        headless: true,
        sound: false,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        preferences_file: std::env::temp_dir()
            .join("dereth-radar-allegiance-shape-not-created/prefs.ini"),
        ..Config::default()
    })
    .expect("an application");
    app.start_shell().expect("the shell starts");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.load_static_scene(SceneConfig {
        // A real local scene character supplies the live viewer and camera frame. The two
        // encoded session bodies below remain the authoritative radar objects; this character is
        // only the camera/origin consumer that makes their `RadarEntry::in_world` true.
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: false,
        ..SceneConfig::default()
    })
    .expect("the retail scene loads");
    for _ in 0..4 {
        app.frame();
    }
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("the replay endpoint attaches");

    assert!(
        app.objects_mut().world.set_player(PLAYER),
        "the player identity is new"
    );
    let scene = app.world_scene().expect("the retail scene");
    let camera = scene.camera.frame();
    let block = scene.viewer_block().expect("the viewer block");
    for (id, x) in [(PLAYER, -2.0), (TARGET, 2.0)] {
        let mut p = ObjectCreatePayload {
            id,
            ..Default::default()
        };
        p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP
            | dereth_protocol::types::physicsdesc::flags::POSITION;
        p.physicsdesc.setup_id = Some(0x0200_0001);
        p.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
            objcell_id: ((block.0 as u32) << 24) | ((block.1 as u32) << 16) | 1,
            frame: dereth_protocol::types::Frame {
                origin: dereth_physics::math::localtoglobal(&camera, Vec3::new(x, 18.0, -1.0))
                    .into(),
                orientation: Quat::IDENTITY.into(),
            },
        });
        p.physicsdesc.timestamps.instance = 1;
        peer.send(
            &mut app,
            10,
            dereth_protocol::write_blob(&ItemCreateObject(p)).expect("create"),
        );
        app.frame();
    }
    for (id, name) in [(PLAYER, "Local Player"), (TARGET, "Allegiance Target")] {
        let w = app
            .objects_mut()
            .world
            .weenie_mut(id)
            .expect("created weenie");
        w.pwd.name = name.to_owned();
        w.pwd.obj_type = item_type::CREATURE;
        w.pwd.bitfield = bitfield::PLAYER;
        w.pwd.radar_enum = Some(4);
    }
    app.objects_mut()
        .world
        .weenie_mut(TARGET)
        .expect("target")
        .pwd
        .monarch = Some(MONARCH);
    settle(&mut app);
    (app, peer)
}

fn target_shape(app: &App) -> BlipShape {
    let target = app
        .hud()
        .radar
        .iter()
        .find(|b| b.id == TARGET)
        .expect("target radar row");
    let player = app
        .hud()
        .radar
        .iter()
        .find(|b| b.id == PLAYER)
        .expect("player radar row");
    dereth_ui_screens::mapradar::radar::get_blip_shape(Some(target), Some(player))
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

fn stored_monarch(app: &App) -> Option<StatValue> {
    app.objects()
        .world
        .player_qualities()
        .and_then(|q| q.get(StatKey::new(StatType::Iid, MONARCH_PROPERTY)))
}

/// Behaviour: panels.radar.a-monarch-update-reshapes-another-players-blip
///
/// Initialization subscribes both player and global InstanceID `0x1A` handlers. A player-quality
/// change recomputes every radar row because the
/// player's monarch changes every other player's allegiance shape. The current HUD likewise
/// rebuilds the full radar each frame, so the observable is the actual shipped radar element:
/// a five-fill Default cross becomes the eight-fill AllegianceMember hollow box.
///
/// The equal/stale controls pin `PropertySequenceGate` (`equal` is accepted), zero is a valid
/// authoritative value, and removal deliberately has no player-description mirror.
#[test]
fn player_monarch_updates_refresh_another_visible_players_radar_shape() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    assert_eq!(
        target_shape(&app),
        BlipShape::Default,
        "no local monarch means no allegiance"
    );
    assert_eq!(
        radar_fill_count(&app),
        14,
        "five target pixels plus the nine-pixel self marker"
    );

    peer.event(&mut app, private_monarch(1, MONARCH));
    settle(&mut app);
    assert_eq!(
        stored_monarch(&app),
        Some(StatValue::Iid(MONARCH)),
        "the player description accepted 0x02D9"
    );
    assert_eq!(
        app.objects().world.weenie(PLAYER).expect("player").pwd.monarch,
        Some(MONARCH),
        "the authoritative player update mirrors into the player-description mirror used by the allegiance-membership check"
    );
    assert_eq!(target_shape(&app), BlipShape::AllegianceMember);
    assert_eq!(
        radar_fill_count(&app),
        17,
        "eight hollow-box pixels plus the self marker"
    );

    peer.event(&mut app, private_monarch(1, OTHER_MONARCH));
    settle(&mut app);
    assert_eq!(
        stored_monarch(&app),
        Some(StatValue::Iid(OTHER_MONARCH)),
        "an equal stamp is accepted: the sequence gate uses not-older, not strictly-newer"
    );
    assert_eq!(
        target_shape(&app),
        BlipShape::Default,
        "the accepted equal stamp refreshes the shape"
    );

    peer.event(&mut app, private_monarch(0, MONARCH));
    settle(&mut app);
    assert_eq!(
        stored_monarch(&app),
        Some(StatValue::Iid(OTHER_MONARCH)),
        "sequence zero is older than one under the byte-wrap rule"
    );
    assert_eq!(
        target_shape(&app),
        BlipShape::Default,
        "the stale value changed nothing"
    );

    peer.event(&mut app, public_monarch(2, ObjectId(0)));
    settle(&mut app);
    assert_eq!(
        stored_monarch(&app),
        Some(StatValue::Iid(ObjectId(0))),
        "public-player form shares the store"
    );
    assert_eq!(
        app.objects()
            .world
            .weenie(PLAYER)
            .expect("player")
            .pwd
            .monarch,
        Some(ObjectId(0)),
        "zero is stored and mirrored; it is not translated into a missing update"
    );
    assert_eq!(target_shape(&app), BlipShape::Default);
    assert_eq!(radar_fill_count(&app), 14);

    peer.event(&mut app, public_monarch(1, MONARCH));
    settle(&mut app);
    assert_eq!(
        target_shape(&app),
        BlipShape::Default,
        "an older public form is stale too"
    );

    peer.event(&mut app, public_monarch(3, MONARCH));
    settle(&mut app);
    assert_eq!(
        target_shape(&app),
        BlipShape::AllegianceMember,
        "a fresh public form mirrors once"
    );

    peer.event(&mut app, remove_monarch(4));
    settle(&mut app);
    assert_eq!(
        stored_monarch(&app),
        None,
        "the stat removal deletes the player-description key"
    );
    assert_eq!(
        app.objects()
            .world
            .weenie(PLAYER)
            .expect("player")
            .pwd
            .monarch,
        Some(MONARCH),
        "stat removal has no player-description mirror"
    );
    assert_eq!(
        target_shape(&app),
        BlipShape::AllegianceMember,
        "quality-removal handling re-reads the unchanged player description and therefore leaves the shape unchanged"
    );
    app.shutdown();
}
