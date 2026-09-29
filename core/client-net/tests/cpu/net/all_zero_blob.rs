//! Contracts for all zero blob.
//! Fixture: shared recorded messages and synthetic state.

use dereth_client_net::client_session::dispatch::world_objects::{dispatch, InstanceTable};
use dereth_client_net::client_session::ordering::ParkedBlobs;
use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_client_net::client_session::{DropReason, SessionEvent};
use dereth_primitives::{IncomingMessage, NetBlobId, NetQueue, RecipientId};
use dereth_protocol::objects::ObjectDispatchOutcome;

const ZERO_IDX: usize = 2600;
const ZERO_LEN: usize = 44;
const ZERO_BLOB_ID: u64 = 0x8000_0000_0000_08BF;
const ZERO_T_MICROS: u64 = 554_539_634;

fn load(scen: &str) -> Vec<CorpusBlob> {
    Corpus::shared(scen).blobs.clone()
}

#[test]
fn the_ace_zero_blob_has_exactly_this_shape() {
    let blobs = load("requested-death-vitae-salvage");
    let b = blobs
        .iter()
        .find(|b| b.idx == ZERO_IDX)
        .expect("requested carries blob 2600");

    assert_eq!(b.opcode, 0x0000, "its opcode dword is zero");
    assert_eq!(
        b.payload.len(),
        ZERO_LEN,
        "44 bytes, the length of a GameMessageUpdatePosition"
    );
    assert!(
        b.payload.iter().all(|&x| x == 0),
        "and every one of them is zero"
    );
    assert_eq!(b.dir, Direction::ServerToClient, "server to client");
    assert_eq!(
        b.queue,
        NetQueue::WorldObjects,
        "queue 10 -- ACE's SmartboxQueue"
    );
    assert_eq!(
        b.blob_id, ZERO_BLOB_ID,
        "ACE's constant 0x80000000 id, fragment sequence 0x8BF"
    );
    assert_eq!(b.t_rel_micros, ZERO_T_MICROS, "t = 554.539634 s");

    let s2c: Vec<&CorpusBlob> = blobs
        .iter()
        .filter(|b| b.dir == Direction::ServerToClient)
        .collect();
    let at = s2c
        .iter()
        .position(|b| b.idx == ZERO_IDX)
        .expect("it is a server blob");
    assert_eq!(
        s2c[at - 1].blob_id,
        ZERO_BLOB_ID - 1,
        "the previous server blob is 0x8BE"
    );
    assert_eq!(
        s2c[at + 1].blob_id,
        ZERO_BLOB_ID + 1,
        "the next one is 0x8C0, with no gap"
    );
    assert_eq!(
        b.blob_id >> 32,
        0x8000_0000,
        "the high dword is ACE's constant: ephemeral bit set, ordering type 0, stamp 0"
    );
}

#[test]
fn every_recorded_zero_blob_is_a_server_world_object_message() {
    let zeros: Vec<_> = Corpus::shared_all()
        .into_iter()
        .flat_map(|c| &c.blobs)
        .filter(|b| !b.payload.is_empty() && b.payload.iter().all(|x| *x == 0))
        .collect();
    assert!(!zeros.is_empty());
    for b in zeros {
        assert_eq!(b.dir, Direction::ServerToClient);
        assert_eq!(b.queue, NetQueue::WorldObjects);
        assert_eq!(b.payload.len(), ZERO_LEN);
    }
}

#[test]
fn a_44_byte_queue_10_server_blob_is_a_position_event() {
    let mut by_opcode: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for corpus in Corpus::shared_all() {
        for b in &corpus.blobs {
            if b.dir == Direction::ServerToClient
                && b.queue == NetQueue::WorldObjects
                && b.payload.len() == ZERO_LEN
            {
                *by_opcode.entry(b.opcode).or_default() += 1;
            }
        }
    }
    assert!(by_opcode.get(&0xF748).is_some_and(|n| *n > 0));
    assert!(by_opcode.get(&0xF74C).is_some_and(|n| *n > 0));
    assert!(by_opcode.get(&0x0000).is_some_and(|n| *n > 0));
    assert_eq!(
        by_opcode.keys().copied().collect::<Vec<_>>(),
        vec![0, 0xF748, 0xF74C],
        "nothing else is 44 bytes on queue 10: {by_opcode:02X?}"
    );

    let blobs = load("requested-death-vitae-salvage");
    let window: Vec<&CorpusBlob> = blobs
        .iter()
        .filter(|b| b.t_rel_micros > 540_000_000 && b.t_rel_micros < 570_000_000)
        .collect();
    assert!(
        window.iter().any(|b| b
            .payload
            .windows(4)
            .any(|w| w == 0x5000_0025u32.to_le_bytes())),
        "a second player's object id appears in the same half-minute"
    );
}

/// Behaviour: link.world-view.an-all-zero-server-blob-is-refused-as-retail-refuses-it
#[test]
fn the_world_view_dispatcher_refuses_it_exactly_as_retail_does() {
    let blobs = load("requested-death-vitae-salvage");
    let zero = blobs.iter().find(|b| b.idx == ZERO_IDX).expect("blob 2600");

    let msg = |b: &CorpusBlob| IncomingMessage {
        opcode: b.opcode,
        queue: b.queue,
        sender: RecipientId::default(),
        blob_id: NetBlobId(b.blob_id),
        body: b.payload[4..].to_vec(),
    };

    let mut table = InstanceTable::new();
    let mut parked = ParkedBlobs::new();
    let player = dereth_primitives::ObjectId(0x5000_001D);

    let got = dispatch(&mut table, &mut parked, Some(player), &msg(zero));
    assert_eq!(
        got.outcome,
        ObjectDispatchOutcome::Error,
        "NETBLOB_ERROR, as the client's default arm returns"
    );
    assert_eq!(
        got.outcome.code(),
        3,
        "and 3 is the value retail returns there"
    );
    assert!(
        matches!(
            got.event,
            Some(SessionEvent::Dropped {
                queue: NetQueue::WorldObjects,
                reason: DropReason::NoHandler,
                ..
            })
        ),
        "dropped for want of a switch arm, which is what the client's own fall-through is: {:?}",
        got.event
    );
    assert_eq!(
        parked.parked_on(dereth_primitives::ObjectId(0)),
        0,
        "and nothing was parked: the blob names no object, and the refusal happens before the \
         leading-object-id read"
    );

    let real = blobs
        .iter()
        .find(|b| {
            b.opcode == 0xF748 && b.queue == NetQueue::WorldObjects && b.payload.len() == ZERO_LEN
        })
        .expect("requested carries 44-byte position events");
    let got = dispatch(&mut table, &mut parked, Some(player), &msg(real));
    assert_ne!(
        got.outcome,
        ObjectDispatchOutcome::Error,
        "a real 0xF748 is not an error"
    );
    assert!(
        !matches!(
            got.event,
            Some(SessionEvent::Dropped {
                reason: DropReason::NoHandler,
                ..
            })
        ),
        "and it is not dropped for want of a handler: {:?}",
        got.event
    );
}
