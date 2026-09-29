//! Contracts for force objdesc.
//! Fixture: shared recorded messages and synthetic state.

use dereth_client_net::client_session::testing::{Corpus, Direction, MockTransport};
use dereth_client_net::client_session::Session;
use dereth_primitives::{NetQueue, ObjectId};
use dereth_protocol::objects::ObjectSendForceObjdesc;

const OPCODE_BYTES: [u8; 4] = [0xEA, 0xF6, 0x00, 0x00];

const BLOB_LEN: usize = 8;

const CONTROL_QUEUE_ID: u32 = 2;

const NULL_TABLE_INTERVAL: f64 = 20.0;

struct Blob {
    session: String,
    idx: usize,
    t_rel: f64,
    dir: char,
    queue: u32,
    opcode: u32,
    payload: Vec<u8>,
}

fn recorded_blob_totals() -> (usize, usize) {
    let all = Corpus::shared_all();
    let total: usize = all.iter().map(|c| c.blobs.len()).sum();
    let client: usize = all
        .iter()
        .map(|c| {
            c.blobs
                .iter()
                .filter(|b| b.dir == Direction::ClientToServer)
                .count()
        })
        .sum();
    (total, client)
}

fn corpus() -> Vec<Blob> {
    Corpus::shared_all()
        .iter()
        .flat_map(|c| {
            c.blobs.iter().map(|b| Blob {
                session: c.name.clone(),
                idx: b.idx,
                t_rel: b.t_rel_micros as f64 / 1_000_000.0,
                dir: if b.dir == Direction::ClientToServer {
                    'c'
                } else {
                    's'
                },
                queue: u32::from(dereth_client_net::client_session::testing::queue_id(
                    b.queue,
                )),
                opcode: b.opcode,
                payload: b.payload.clone(),
            })
        })
        .collect()
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

#[test]
fn the_bare_sender_writes_the_eight_bytes_send_force_objdesc_builds() {
    let mut s: Session<MockTransport> = Session::new(MockTransport::new());
    s.send_force_objdesc(ObjectId(0x8000_09DE));

    assert_eq!(s.transport.sent.len(), 1, "one blob, and only one");
    let b = &s.transport.sent[0];

    assert_eq!(b.queue, NetQueue::Control, "sent on the control queue");
    assert!(
        !b.ordered,
        "a Control blob carries no ordered-action header"
    );

    assert_eq!(b.payload.len(), BLOB_LEN, "an 8-byte buffer, sent whole");
    assert_eq!(
        &b.payload[..4],
        &OPCODE_BYTES,
        "bytes 0..4 = EA F6 00 00, written a byte at a time"
    );
    assert_eq!(
        b.payload,
        vec![0xEA, 0xF6, 0x00, 0x00, 0xDE, 0x09, 0x00, 0x80],
        "the whole blob, little-endian id"
    );
}

#[test]
fn asking_for_an_objdesc_does_not_consume_an_action_stamp() {
    let mut s: Session<MockTransport> = Session::new(MockTransport::new());
    let before = s
        .send_action(&dereth_protocol::comms::CommunicationTalk {
            message: "one".into(),
        })
        .expect("talk encodes");
    s.send_force_objdesc(ObjectId(0x8000_0A6C));
    s.send_force_objdesc(ObjectId(0x8000_09E9));
    let after = s
        .send_action(&dereth_protocol::comms::CommunicationTalk {
            message: "two".into(),
        })
        .expect("talk encodes");
    assert_eq!(
        after,
        before + 1,
        "the global UI counter is untouched by a Control send"
    );
}

/// Behaviour: objects.force-objdesc.leaves-on-the-control-queue
#[test]
fn every_recorded_force_objdesc_is_eight_control_bytes_the_sender_reproduces() {
    let blobs = corpus();
    let (recorded, recorded_client) = recorded_blob_totals();
    assert!(
        recorded > 0 && recorded_client > 0,
        "{recorded} blobs is not the corpus"
    );
    assert_eq!(
        blobs.len(),
        recorded,
        "the corpus, independently re-derived"
    );
    assert_eq!(
        blobs.iter().filter(|b| b.dir == 'c').count(),
        recorded_client,
        "client blobs — the population this opcode is 38 of"
    );

    let asks: Vec<&Blob> = blobs.iter().filter(|b| b.opcode == 0xF6EA).collect();
    assert!(!asks.is_empty());

    for a in &asks {
        assert_eq!(a.dir, 'c', "blob {}: client to server", a.idx);
        assert_eq!(
            a.queue, CONTROL_QUEUE_ID,
            "blob {}: the Control queue",
            a.idx
        );
        assert_eq!(a.payload.len(), BLOB_LEN, "blob {}: eight bytes", a.idx);
        assert_eq!(
            &a.payload[..4],
            &OPCODE_BYTES,
            "blob {}: EA F6 00 00",
            a.idx
        );

        let m = dereth_protocol::read_body::<ObjectSendForceObjdesc>(&a.payload[4..])
            .unwrap_or_else(|e| panic!("blob {}: {e}", a.idx));
        let mut s: Session<MockTransport> = Session::new(MockTransport::new());
        s.send_force_objdesc(m.id);
        assert_eq!(s.transport.sent.len(), 1, "blob {}: one blob out", a.idx);
        assert_eq!(
            s.transport.sent[0].payload, a.payload,
            "blob {}: byte-identical to the recording",
            a.idx
        );
        assert_eq!(
            s.transport.sent[0].queue,
            NetQueue::Control,
            "blob {}",
            a.idx
        );
    }
}

#[test]
fn each_recorded_ask_is_either_the_twenty_second_sweep_or_the_weenie_desc_merge() {
    let blobs = corpus();
    let all_asks: Vec<&Blob> = blobs.iter().filter(|b| b.opcode == 0xF6EA).collect();
    assert!(!all_asks.is_empty());
    let asks: Vec<&Blob> = all_asks
        .iter()
        .copied()
        .filter(|b| b.session == "long-solo-play")
        .collect();
    let merged: Vec<&Blob> = all_asks
        .iter()
        .copied()
        .filter(|b| b.session == "house-purchase-and-trade")
        .collect();
    let unanswered: Vec<&Blob> = all_asks
        .iter()
        .copied()
        .filter(|b| b.session == "post-relog-attribute-training")
        .collect();
    assert_eq!(
        asks.len() + merged.len() + unanswered.len(),
        all_asks.len(),
        "35 sweep asks, 3 answered merge asks and 1 unanswered merge ask"
    );

    assert!(!asks.is_empty() && !merged.is_empty() && !unanswered.is_empty());
    let t0 = merged[0].t_rel;
    for a in &merged {
        assert_eq!(a.dir, 'c', "blob {}: client to server", a.idx);
        assert!(
            (a.t_rel - t0).abs() < f64::EPSILON,
            "blob {}: all three at one instant",
            a.idx
        );
        let id = le32(&a.payload, 4);
        let create = blobs
            .iter()
            .rfind(|b| {
                b.session == a.session
                    && b.opcode == 0xF745
                    && b.payload.len() >= 8
                    && le32(&b.payload, 4) == id
                    && b.t_rel <= a.t_rel
            })
            .unwrap_or_else(|| panic!("blob {}: no 0xF745 for {id:#010X} before it", a.idx));
        let gap = a.t_rel - create.t_rel;
        assert!(
            (0.19..0.21).contains(&gap),
            "blob {}: the descriptor for {id:#010X} arrived {gap:.3} s before the ask; the \
             merge path answers the arrival, so this gap is the mechanism",
            a.idx
        );
        assert!(
            blobs.iter().any(|b| {
                b.session == a.session
                    && b.opcode == 0xF625
                    && b.payload.len() >= 8
                    && le32(&b.payload, 4) == id
                    && (b.t_rel - a.t_rel) >= 0.0
                    && (b.t_rel - a.t_rel) <= 0.030
            }),
            "blob {}: the merge ask for {id:#010X} is answered like the sweep ones are",
            a.idx
        );
    }

    for a in &unanswered {
        let id = le32(&a.payload, 4);
        assert_eq!(a.dir, 'c', "blob {}: client to server", a.idx);
        let create = blobs
            .iter()
            .rfind(|b| {
                b.session == a.session
                    && b.opcode == 0xF745
                    && b.payload.len() >= 8
                    && le32(&b.payload, 4) == id
                    && b.t_rel <= a.t_rel
            })
            .unwrap_or_else(|| panic!("blob {}: no 0xF745 for {id:#010X} before it", a.idx));
        let gap = a.t_rel - create.t_rel;
        assert!(
            gap < 0.05,
            "blob {}: the descriptor for {id:#010X} arrived {gap:.3} s before the ask, which is \
             neither the merge gap nor the sweep interval",
            a.idx
        );
        assert!(
            !blobs.iter().any(|b| {
                b.session == a.session
                    && b.opcode == 0xF625
                    && b.payload.len() >= 8
                    && le32(&b.payload, 4) == id
                    && b.t_rel >= a.t_rel
            }),
            "blob {}: something answered the ask for {id:#010X}; this block asserts that nothing \
             ever does, and a reply means the corpus moved rather than that the test is stale",
            a.idx
        );
    }

    let mut by_id: std::collections::BTreeMap<u32, Vec<f64>> = std::collections::BTreeMap::new();
    for a in &asks {
        by_id.entry(le32(&a.payload, 4)).or_default().push(a.t_rel);
    }
    assert!(!by_id.is_empty());
    let mut gaps = 0usize;
    for (id, ts) in &by_id {
        for w in ts.windows(2) {
            let gap = w[1] - w[0];
            assert!(
                (gap - NULL_TABLE_INTERVAL).abs() < 0.01,
                "{id:#010X}: consecutive asks {gap:.3} s apart, not {NULL_TABLE_INTERVAL}"
            );
            gaps += 1;
        }
    }
    assert!(gaps > 0);
    assert_eq!(gaps, asks.len() - by_id.len());

    for a in &asks {
        let id = le32(&a.payload, 4);
        let near = blobs.iter().any(|b| {
            b.session == a.session
                && b.opcode == 0xF745
                && b.payload.len() >= 8
                && le32(&b.payload, 4) == id
                && (a.t_rel - b.t_rel) >= 0.0
                && (a.t_rel - b.t_rel) <= 10.0
        });
        assert!(
            !near,
            "blob {}: a 0xF745 for {id:#010X} arrived within 10 s before the ask, so this one \
             could be the weenie-description merge path after all",
            a.idx
        );
    }

    let mut answered = 0usize;
    let mut unanswered: Vec<(usize, u32)> = Vec::new();
    for a in &asks {
        let id = le32(&a.payload, 4);
        if blobs.iter().any(|b| {
            b.session == a.session
                && b.opcode == 0xF625
                && b.payload.len() >= 8
                && le32(&b.payload, 4) == id
                && (b.t_rel - a.t_rel) >= 0.0
                && (b.t_rel - a.t_rel) <= 0.030
        }) {
            answered += 1;
        } else {
            unanswered.push((a.idx, id));
        }
    }
    assert!(answered > 0 && !unanswered.is_empty());
    assert_eq!(answered + unanswered.len(), asks.len());
    for (idx, id) in &unanswered {
        let last = by_id[id].last().copied().expect("at least one ask");
        let t = asks.iter().find(|a| a.idx == *idx).expect("the ask").t_rel;
        assert!(
            (t - last).abs() < f64::EPSILON,
            "blob {idx}: unanswered but not the last ask for {id:#010X}"
        );
    }
}
