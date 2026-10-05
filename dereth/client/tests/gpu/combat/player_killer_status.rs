//! Live `PlayerKillerStatus` (Int 134) updates reach the local PWD predicates: the private
//! `0x02CD` and public `0x02CE` int updates set the PK and PKLite bits that attack eligibility,
//! the radar and the selected-target indicator all read, with fellowship colours overriding the
//! PK base and friends or allegiance not. The stored Int 134 and the PWD bits agree, and a PK pair
//! passes the local pre-send attack gate.
//! Fixture: a headless `App` in gameplay with a socket-free replay endpoint fed synthetic
//! datagrams, and long-solo-play's recorded player description.

#![cfg(gpu)]

use crate::common::client_dir_required as client_dir;
use crate::common::gpu_lock;

use dereth_client_model::combat::{AttackHeight, CombatMode};
use dereth_client_model::{RecordingRequests, Request, StatKey, StatType, StatValue};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{LocalTime, ObjectId, Quat, ServerTime, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::qualities as wire;
use dereth_protocol::Opcode;
use dereth_ui::region::SurfaceOp;
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::mapradar::radar::{get_blip_color, semantic};
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};
use std::collections::BTreeMap;
use {
    dereth_client::app::App, dereth_client_runtime::config::Config,
    dereth_client_runtime::net::ClientNetwork, dereth_client_runtime::scene::SceneConfig,
};
use {dereth_rules::fellowship::Fellow, dereth_rules::fellowship::Fellowship};
use {dereth_rules::weenie::bitfield, dereth_rules::weenie::item_type};

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const TARGET: ObjectId = ObjectId(0x5000_0022);
const PLAYER_KILLER_STATUS: u32 = 134;

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
    stamp: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net =
            ClientNetwork::new("127.0.0.1:19100", 7304, "player-killer-status", "unused", 0)
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

fn target_handle(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().expect("the gameplay UI");
    let any: &dyn std::any::Any = shell.flow.current().expect("the gameplay screen");
    let root = any
        .downcast_ref::<GamePlayScreen>()
        .and_then(GamePlayScreen::root)
        .expect("the gameplay root");
    shell
        .ui
        .get_child_recursive(root, id)
        .expect("the shipped target element")
}

fn selected_corner_color(app: &App) -> u32 {
    let on = target_handle(app, window::TARGET_ON_SCREEN);
    assert!(
        app.ui()
            .expect("the gameplay UI")
            .ui
            .node(on)
            .expect("target node")
            .region
            .flags
            .visible,
        "the selected positioned player reaches the on-screen VividTargetIndicator"
    );
    let corner = target_handle(app, ElementId(0x1000_0039));
    let command = app
        .ui_draw_list()
        .iter()
        .find(|command| command.who == corner)
        .expect("the first shipped target corner is drawn");
    match command.image_op {
        Some(SurfaceOp::Colorize(color)) => color,
        ref other => panic!("the target corner uses retail Colorize, got {other:?}"),
    }
}

fn player_description() -> dereth_protocol::login::LoginPlayerDescription {
    let corpus = Corpus::shared("long-solo-play");
    let payload = corpus
        .blobs
        .iter()
        .find(|b| {
            b.dir == Direction::ServerToClient
                && b.opcode == 0xF7B0
                && b.payload.len() >= 16
                && u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes")) == 0x0013
        })
        .map(|b| b.payload.clone())
        .expect("long-solo-play carries 0x0013");
    dereth_protocol::read_body::<dereth_protocol::login::LoginPlayerDescription>(&payload[16..])
        .expect("the recorded player description decodes")
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
            .join("dereth-player-killer-status-not-created/prefs.ini"),
        ..Config::default()
    })
    .expect("an application");
    app.start_shell().expect("the shell starts");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.load_static_scene(SceneConfig {
        character: false,
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
    app.probe_mut().objects_mut().world.player = Some(PLAYER);
    for (id, name) in [(PLAYER, "Local PK"), (TARGET, "Target PK")] {
        let w = app
            .probe_mut()
            .objects_mut()
            .world
            .weenie_mut(id)
            .expect("created weenie");
        w.pwd.name = name.to_owned();
        w.pwd.obj_type = item_type::CREATURE;
        w.pwd.bitfield = bitfield::PLAYER;
        w.pwd.radar_enum = Some(4);
    }

    let desc = SessionEvent::PlayerDescription(Box::new(player_description()));
    app.probe_mut()
        .objects_mut()
        .apply_event(&desc, LocalTime(0.0));
    app.apply_hud_events(std::slice::from_ref(&desc));
    app.probe_mut()
        .apply_interaction_events(std::slice::from_ref(&desc));
    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .set_option(14, true, ServerTime(0.0));
    app.probe_mut().objects_mut().world.set_selected_object(
        Some(TARGET),
        false,
        &mut dereth_client_model::NullSink,
    );
    settle(&mut app);
    (app, peer)
}

#[test]
fn live_pk_updates_recolor_the_selected_player_with_native_social_precedence() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    assert_eq!(
        selected_corner_color(&app),
        0xFF00_0000 | semantic::DEFAULT.hex,
        "an ordinary selected player starts with the white radar color"
    );

    app.probe_mut().objects_mut().world.recv_friends_update(
        &dereth_protocol::social::SocialFriendsUpdate {
            friends: vec![dereth_protocol::social::FriendData {
                id: TARGET,
                online: 1,
                name: "Target PK".to_owned(),
                ..Default::default()
            }],
            update_type: 0,
        },
    );
    app.probe_mut()
        .objects_mut()
        .world
        .weenie_mut(PLAYER)
        .expect("player")
        .pwd
        .monarch = Some(ObjectId(0x5000_00AA));
    app.probe_mut()
        .objects_mut()
        .world
        .weenie_mut(TARGET)
        .expect("target")
        .pwd
        .monarch = Some(ObjectId(0x5000_00AA));
    settle(&mut app);
    assert_eq!(
        selected_corner_color(&app),
        0xFF00_0000 | semantic::DEFAULT.hex,
        "friends and allegiance membership do not override target-indicator color"
    );

    peer.event(&mut app, public_int(TARGET, 1, 4));
    settle(&mut app);
    assert_eq!(
        selected_corner_color(&app),
        0xFF00_0000 | semantic::PLAYER_KILLER.hex,
        "the actual public Int134 transition recolors the already-selected player red"
    );

    let mut members = BTreeMap::new();
    members.insert(
        TARGET,
        Fellow {
            name: "Target PK".to_owned(),
            ..Fellow::default()
        },
    );
    app.probe_mut().objects_mut().world.fellowship = Some(Fellowship {
        members,
        leader: TARGET,
        ..Fellowship::default()
    });
    settle(&mut app);
    assert_eq!(
        selected_corner_color(&app),
        0xFF00_0000 | semantic::FELLOWSHIP_LEADER.hex,
        "fellowship leader green overrides the PK base"
    );

    app.probe_mut().objects_mut().world.fellowship = None;
    peer.event(&mut app, public_int(TARGET, 2, 0x40));
    settle(&mut app);
    assert_eq!(
        selected_corner_color(&app),
        0xFF00_0000 | semantic::PK_LITE.hex,
        "without fellowship, the next authoritative transition exposes the PKLite base"
    );
    app.shutdown();
}

fn private_int(sequence: u8, value: i32) -> Vec<u8> {
    let mut out = Opcode::QUALITIES_PRIVATE_UPDATE_INT
        .0
        .to_le_bytes()
        .to_vec();
    out.extend_from_slice(
        &dereth_protocol::write_body(&wire::QualitiesPrivateUpdateInt(wire::PrivateUpdate {
            sequence,
            property_id: PLAYER_KILLER_STATUS,
            value,
        }))
        .expect("private Int update"),
    );
    out
}

fn public_int(object: ObjectId, sequence: u8, value: i32) -> Vec<u8> {
    let mut out = Opcode::QUALITIES_UPDATE_INT.0.to_le_bytes().to_vec();
    out.extend_from_slice(
        &dereth_protocol::write_body(&wire::QualitiesUpdateInt(wire::PublicUpdate {
            sequence,
            object,
            property_id: PLAYER_KILLER_STATUS,
            value,
        }))
        .expect("public Int update"),
    );
    out
}

fn status(app: &App, id: ObjectId) -> (bool, bool) {
    let w = app.objects().world.weenie(id).expect("weenie");
    (w.is_pk(), w.is_pk_lite())
}

/// Behaviour: combat.pk-status.live-updates-drive-attackability-radar-and-selection-colour
#[test]
fn live_pk_updates_drive_attack_eligibility_and_the_radar_from_the_same_pwd_bits() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    assert_eq!(
        status(&app, PLAYER),
        (false, false),
        "the local baseline is NPK"
    );
    assert_eq!(
        status(&app, TARGET),
        (false, false),
        "the target baseline is NPK"
    );
    assert!(
        !app.objects().world.object_is_attackable(TARGET),
        "two NPK players cannot attack"
    );

    // The local player's private form has no second dispatcher; the target's public form is the
    // ordinary object-update route. Both must land on the PWD predicates retail calls.
    peer.event(&mut app, private_int(1, 4));
    peer.event(&mut app, public_int(TARGET, 1, 4));
    settle(&mut app);
    assert_eq!(
        status(&app, PLAYER),
        (true, false),
        "private Int134 updates local IsPK"
    );
    assert_eq!(
        status(&app, TARGET),
        (true, false),
        "public Int134 updates target IsPK"
    );
    assert!(
        app.objects().world.object_is_attackable(TARGET),
        "both PK now pass the local gate"
    );
    assert_eq!(
        app.objects()
            .world
            .player_qualities()
            .expect("player descriptor")
            .get(StatKey::new(StatType::Int, PLAYER_KILLER_STATUS)),
        Some(StatValue::Int(4)),
        "the quality store and PWD mirror agree"
    );

    let target_blip = app
        .hud()
        .radar
        .iter()
        .find(|b| b.id == TARGET)
        .expect("target radar row");
    assert_eq!(
        get_blip_color(Some(target_blip)).hex,
        semantic::PLAYER_KILLER.hex,
        "a non-fellow PK uses the native red base color"
    );
    let mut fellow_control = *target_blip;
    fellow_control.is_fellow = true;
    assert_eq!(
        get_blip_color(Some(&fellow_control)).hex,
        semantic::FELLOWSHIP.hex,
        "native fellowship color overrides the PK base; this is not a PK-state defect"
    );

    app.probe_mut().objects_mut().world.combat.combat_mode = CombatMode::Melee;
    app.probe_mut()
        .objects_mut()
        .world
        .combat
        .requested_attack_power = 0.5;
    let mut requests = RecordingRequests::default();
    let refusal = app.probe_mut().objects_mut().world.execute_attack(
        &mut requests,
        AttackHeight::Medium,
        false,
        true,
    );
    assert_eq!(
        refusal, None,
        "the local attack path does not refuse the live PK pair"
    );
    assert!(
        matches!(
            requests.0.as_slice(),
            [Request::TargetedMeleeAttack(a)] if a.target == TARGET
        ),
        "the eligible pair reaches the exact targeted-melee request: {:?}",
        requests.0
    );

    // A public update naming the player is first accepted by Hud. The boundary sees the shared
    // stamp as stale, so this arm shows the first consumer itself mirrors instead of depending on
    // the second one.
    peer.event(&mut app, public_int(PLAYER, 2, 0));
    settle(&mut app);
    assert_eq!(
        status(&app, PLAYER),
        (false, false),
        "public Int134 clears the local PK bits too"
    );
    assert!(
        !app.objects().world.object_is_attackable(TARGET),
        "the attack gate follows the clear"
    );

    peer.event(&mut app, private_int(3, 0x40));
    peer.event(&mut app, public_int(TARGET, 2, 0x40));
    settle(&mut app);
    assert_eq!(
        status(&app, PLAYER),
        (false, true),
        "private Int134 also moves to PKLite"
    );
    assert_eq!(
        status(&app, TARGET),
        (false, true),
        "the target moves to PKLite"
    );
    assert!(
        app.objects().world.object_is_attackable(TARGET),
        "matching PKLite also passes"
    );
    app.shutdown();
}
