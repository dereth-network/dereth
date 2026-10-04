//! The sound half of `0xEA60 Admin_Environs`: an encoded admin-environs sound reaches the mixer,
//! and the handler's guards (no player body, an option with no case, a short body) stay silent.
//!
//! Fixture: encrypted queue-9 bytes fed through a socket-free client endpoint into a headless app
//! over the retail dats, measured at the software mixer's samples. The real `0xF746`/`0xF745`
//! player create also starts the portal model; that model is reset only after the player and body
//! are asserted, so its sound cannot contaminate the measurement.

use crate::common::client_dir;

use dereth_client::present::NullPresentation;
use dereth_client::{app::App, config::Config, net::ClientNetwork};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::{
    admin::AdminEnvirons,
    objects::{ItemCreateObject, LoginCreatePlayer, ObjectCreatePayload},
};

const PLAYER: ObjectId = ObjectId(0x5000_00EA);

struct TempDir(dereth_dat::testing::ScratchDir);

impl TempDir {
    fn new() -> Self {
        Self(
            dereth_dat::testing::ScratchDir::new("admin-environs-sound")
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
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "admin-environs-sound", "unused", 0)
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

fn setup() -> (TempDir, App, Peer) {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail DAT is the sound oracle"
    );
    let temp = TempDir::new();
    let mut app = App::with_presentation(
        Config {
            headless: true,
            sound: true,
            ui: true,
            world: false,
            character: false,
            dat_dir: client_dir(),
            preferences_file: temp.0.path().join("prefs.ini"),
            ..Config::default()
        },
        Box::new(NullPresentation::new(800, 600)),
    )
    .expect("the WARP application starts");
    app.start_shell().expect("the sound subsystem starts");
    let (peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("the socket-free endpoint attaches");
    (temp, app, peer)
}

fn settle_ui_message(app: &mut App) {
    // Receive fills queue 9 in one frame; the UI queue admits it on the next.
    assert!(app.frame());
    assert!(app.frame());
}

fn add_player(app: &mut App, peer: &mut Peer) {
    peer.send(
        app,
        10,
        dereth_protocol::write_blob(&LoginCreatePlayer { player_id: PLAYER })
            .expect("F746 encodes"),
    );
    let mut payload = ObjectCreatePayload {
        id: PLAYER,
        ..ObjectCreatePayload::default()
    };
    payload.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    payload.physicsdesc.setup_id = Some(0x0200_0001);
    payload.physicsdesc.timestamps.instance = 1;
    peer.send(
        app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(payload)).expect("F745 encodes"),
    );
    assert!(app.frame());
    assert_eq!(
        app.objects().player(),
        Some(PLAYER),
        "F746 installed the current player"
    );
    assert!(
        app.objects().world.physics(PLAYER).is_some(),
        "the player's F745 made the physics body required by retail"
    );

    // F746 also starts the already-covered portal tunnel animation, whose EnterPortal sound is
    // unrelated to EA60 and can begin on the following frame. Stop that independent model after
    // the encoded player/body assertions, then consume its already-started finite sample before
    // measuring the admin switch. ObjectStream's real F746/F745 player and body remain installed.
    app.probe_mut().teleport_mut().reset();
    let audio = app.audio_mut().expect("audio");
    let mut block = vec![0.0_f32; 2 * 2048];
    for _ in 0..32 {
        if audio.active_voices() == 0 {
            break;
        }
        audio.mix(&mut block);
    }
    assert_eq!(
        audio.active_voices(),
        0,
        "the finite portal sample drains before EA60 controls"
    );
}

/// Behaviour: audio.environs.an-admin-environs-sound-reaches-the-mixer
/// The admin-environs handler requires a player body and UI sound table, applies the exact switch,
/// then plays centered entry 7. The final oracle is the decoded retail wave, not a call count.
#[test]
fn an_encoded_admin_environs_sound_reaches_the_mixer_and_keeps_the_native_guards() {
    let (_temp, mut app, mut peer) = setup();
    assert_eq!(app.audio_mut().expect("audio").active_voices(), 0);

    // The outer range accepts this value, but the handler returns before its switch when the
    // player has no physics body.
    peer.environs(&mut app, 0x65);
    settle_ui_message(&mut app);
    assert_eq!(
        app.audio_mut().expect("audio").active_voices(),
        0,
        "no player body is silent"
    );

    add_player(&mut app, &mut peer);

    // These are in `0x65..=0x7C`, but have no switch case in retail.
    for option in [0x73, 0x74, 0x7C] {
        peer.environs(&mut app, option);
        settle_ui_message(&mut app);
    }
    assert_eq!(
        app.audio_mut().expect("audio").active_voices(),
        0,
        "default arms are silent"
    );

    // An opcode followed by half of its one-dword body reaches the generic UI event but must not
    // manufacture an option or a sound.
    let mut malformed = 0xEA60_u32.to_le_bytes().to_vec();
    malformed.extend_from_slice(&0x65_u16.to_le_bytes());
    peer.send(&mut app, 9, malformed);
    settle_ui_message(&mut app);
    assert_eq!(
        app.audio_mut().expect("audio").active_voices(),
        0,
        "short EA60 is silent"
    );

    peer.environs(&mut app, 0x65);
    settle_ui_message(&mut app);
    let audio = app.audio_mut().expect("audio");
    assert_eq!(
        audio.active_voices(),
        1,
        "the UI roar sound starts one centered voice"
    );
    let mut block = vec![0.0_f32; 2 * 2048];
    audio.mix(&mut block);
    let peak = block.iter().fold(0.0_f32, |m, sample| m.max(sample.abs()));
    assert!(peak > 0.0, "the Admin_Environs voice mixed only silence");
    eprintln!("EA60 option 0x65 mixed peak {peak:.6}");

    app.shutdown();
}
