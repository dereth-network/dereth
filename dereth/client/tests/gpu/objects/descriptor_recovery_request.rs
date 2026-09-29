//! A duplicate create for an object the client already holds, arriving with a changed container
//! (the item was dropped), makes the client ask once for the authoritative descriptor with the bare
//! eight-byte `0xF6EA` Control payload. The ask is sent at the message boundary where it was raised
//! and is never replayed to a session attached later.
//!
//! Fixture: long-solo-play's Sack login and drop `Item_CreateObject 0xF745` messages, fed through a
//! socket-free `App` endpoint so session admission, the duplicate-create merge and the Control
//! sender all run.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::{app::App, config::Config, net::ClientNetwork};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_primitives::{LocalTime, ObjectId, ServerTime};
use dereth_protocol::objects::ItemCreateObject;
use dereth_protocol::Message;
use dereth_transport::wire::ParsedPacket;

const SACK: ObjectId = ObjectId(0x8000_0997);
const FORCE_OBJDESC: [u8; 4] = [0xEA, 0xF6, 0x00, 0x00];

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "descriptor-recovery", "unused", 0)
                .expect("a socket-free endpoint");
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

    fn send(&mut self, app: &mut App, bytes: Vec<u8>) {
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
                    queue_id: 10,
                },
                bytes,
            ))
            .expect("one-fragment packet");
        let raw = packet.serialize(Some(self.crypto.next())).expect("packet");
        app.replay_network_mut()
            .expect("attached replay endpoint")
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .expect("feed succeeds");
    }
}

fn sack_creates() -> [ItemCreateObject; 2] {
    let corpus = Corpus::load("long-solo-play")
        .expect("fixture lookup")
        .expect("long-solo-play is promoted");
    let mut creates = corpus.blobs.iter().filter_map(|blob| {
        (blob.dir == Direction::ServerToClient
            && blob.opcode == 0xF745
            && blob.payload.get(4..8) == Some(SACK.0.to_le_bytes().as_slice()))
        .then(|| {
            ItemCreateObject::read(&mut dereth_protocol::Reader::new(&blob.payload[4..]))
                .expect("recorded Sack create decodes")
        })
    });
    let login = creates.next().expect("login create");
    let drop = creates.next().expect("drop create");
    assert!(creates.next().is_none(), "exactly two Sack creates");
    assert_eq!(login.0.wdesc.container_id, Some(ObjectId(0x5000_000A)));
    assert_ne!(drop.0.wdesc.container_id, login.0.wdesc.container_id);
    assert_eq!(
        drop.0.physicsdesc.timestamps.instance,
        login.0.physicsdesc.timestamps.instance
    );
    [login, drop]
}

fn app() -> App {
    App::new(Config {
        headless: true,
        sound: false,
        ui: false,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dere-descriptor-recovery-not-created/prefs.ini"),
        ..Config::default()
    })
    .expect("headless App")
}

fn force_objdesc_payloads(app: &mut App) -> Vec<Vec<u8>> {
    app.replay_network_mut()
        .expect("endpoint")
        .take_outgoing()
        .into_iter()
        .filter_map(|(raw, _)| ParsedPacket::parse(&raw).ok())
        .flat_map(|packet| packet.fragments.into_iter())
        .filter_map(|fragment| {
            (fragment.payload.get(..4) == Some(FORCE_OBJDESC.as_slice()))
                .then_some(fragment.payload)
        })
        .collect()
}

/// Behaviour: objects.descriptor-recovery.a-losing-duplicate-create-asks-for-the-description-once
///
/// The losing order is the ground descriptor before `0x019A`: retail asks for the authoritative
/// descriptor, latches the ask, restores the old containment, and only then lets the following
/// message run. The request is a bare eight-byte Control payload.
#[test]
fn duplicate_drop_create_queues_descriptor_recovery_at_its_message_boundary() {
    let [login, drop] = sack_creates();
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("replay link attaches");

    peer.send(
        &mut app,
        dereth_protocol::write_blob(&login).expect("login create"),
    );
    assert!(app.frame());
    let _ = app.replay_network_mut().expect("endpoint").take_outgoing();
    assert_eq!(
        app.objects()
            .world
            .weenie(SACK)
            .expect("Sack")
            .pwd
            .container_id,
        login.0.wdesc.container_id
    );

    peer.send(
        &mut app,
        dereth_protocol::write_blob(&drop).expect("drop create"),
    );
    assert!(app.frame());

    assert_eq!(
        app.objects()
            .world
            .weenie(SACK)
            .expect("merged Sack")
            .pwd
            .container_id
            .unwrap_or_default(),
        ObjectId(0),
        "the transient restore lets the movement event consume the old owner, then install ground"
    );
    assert_eq!(
        app.objects().stats.objdesc_asks,
        1,
        "the sender accepted it at this boundary"
    );
    assert_eq!(app.objects().stats.objdesc_asks_undeliverable, 0);
    assert!(
        force_objdesc_payloads(&mut app).is_empty(),
        "this fixture admits the world controller after the packet-processing stage, so serialization belongs next frame"
    );
    assert!(
        app.frame(),
        "one empty frame reaches packet-processing stage"
    );
    assert_eq!(
        force_objdesc_payloads(&mut app),
        vec![vec![0xEA, 0xF6, 0x00, 0x00, 0x97, 0x09, 0x00, 0x80]],
        "the descriptor recovery ask reaches the existing bare Control wire payload"
    );
}

/// Behaviour: objects.descriptor-recovery.a-losing-duplicate-create-asks-for-the-description-once
///
/// The component adapter has no session at its message boundary. It reports and discards that
/// ask there; attaching an unrelated session at a later maintenance tick must not replay it.
#[test]
fn sessionless_component_does_not_replay_descriptor_ask_later() {
    let [login, drop] = sack_creates();
    let mut objects = dereth_client::objects::ObjectStream::new();
    for (time, create) in [(1.0, login), (2.0, drop)] {
        objects.apply_event(
            &dereth_client_net::client_session::SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body: dereth_protocol::write_body(&create).expect("body"),
            },
            LocalTime(time),
        );
    }
    assert_eq!(objects.stats.objdesc_asks, 0);
    assert_eq!(objects.stats.objdesc_asks_undeliverable, 1);

    let mut session = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );
    objects.use_time(ServerTime(3.0), Some(&mut session));
    assert!(
        session.transport.sent.is_empty(),
        "no stale descriptor ask reaches a later session"
    );
}
