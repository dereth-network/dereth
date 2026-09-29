//! Contracts for movement recordings.
//! Fixture: shared recorded messages and synthetic state.
//! Behaviour: none (codec, fixture conformance or host-state contracts)

use std::collections::BTreeMap;

use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_protocol::movement::{movement_type, MovementSetObjectMovement};

const OP_MOVEMENT: u32 = 0xF74C;
const OP_CREATE: u32 = 0xF745;
const OP_POSITION: u32 = 0xF748;
const OP_ORDERED_EVENT: u32 = 0xF7B0;
const OP_ORDERED_ACTION: u32 = 0xF7B1;

fn corpus() -> Vec<CorpusBlob> {
    let mut all = Vec::new();
    for c in Corpus::shared_all() {
        all.extend(c.blobs.iter().cloned());
    }
    assert!(!all.is_empty());
    all
}

fn sub_opcode(b: &CorpusBlob) -> Option<u32> {
    let at = match b.opcode {
        OP_ORDERED_EVENT => 12,
        OP_ORDERED_ACTION => 8,
        _ => return None,
    };
    let p = &b.payload;
    (p.len() >= at + 4).then(|| u32::from_le_bytes([p[at], p[at + 1], p[at + 2], p[at + 3]]))
}

#[test]
fn the_initial_recordings_have_disjoint_opcode_spaces() {
    let all: Vec<_> = [
        "first-login-walk-jump",
        "early-inventory-and-casting",
        "short-second-connection",
        "login-account-booted",
        "ddd-interrogation-only",
        "long-solo-play",
        "short-play-with-training",
    ]
    .map(Corpus::shared)
    .into_iter()
    .flat_map(|c| c.blobs.iter().cloned())
    .collect();
    let whole: BTreeMap<u32, usize> = all.iter().fold(BTreeMap::new(), |mut m, b| {
        *m.entry(b.opcode).or_default() += 1;
        m
    });
    let mut events: BTreeMap<u32, usize> = BTreeMap::new();
    let mut actions: BTreeMap<u32, usize> = BTreeMap::new();
    for b in &all {
        if let Some(s) = sub_opcode(b) {
            let m = if b.opcode == OP_ORDERED_EVENT {
                &mut events
            } else {
                &mut actions
            };
            *m.entry(s).or_default() += 1;
        }
    }

    assert!(
        !whole.is_empty(),
        "whole-message space: {} distinct opcodes",
        whole.len()
    );
    assert!(
        !events.is_empty(),
        "0xF7B0 sub space: {} distinct",
        events.len()
    );
    assert!(
        !actions.is_empty(),
        "0xF7B1 sub space: {} distinct",
        actions.len()
    );

    for op in whole.keys() {
        assert!(
            !events.contains_key(op),
            "{op:#06X} is in BOTH the whole and 0xF7B0 spaces"
        );
        assert!(
            !actions.contains_key(op),
            "{op:#06X} is in BOTH the whole and 0xF7B1 spaces"
        );
    }
    for op in events.keys() {
        assert!(
            !actions.contains_key(op),
            "{op:#06X} is in BOTH sub-opcode spaces"
        );
    }

    assert!(events.get(&0x0022).is_some_and(|n| *n > 0));
    assert!(events.get(&0x01C8).is_some_and(|n| *n > 0));
    assert!(actions.get(&0xF61C).is_some_and(|n| *n > 0));
    assert!(whole.get(&0x0197).is_some_and(|n| *n > 0));
    assert!(whole.get(&OP_MOVEMENT).is_some_and(|n| *n > 0));
    assert_eq!(
        events.get(&0x0197),
        None,
        "0x0197 cannot be a game-event sub-opcode"
    );
    assert_eq!(
        whole.get(&0x0022),
        None,
        "0x0022 cannot be a whole-message opcode"
    );
}

#[test]
fn the_matchers_read_zero_over_a_corpus_with_their_subject_removed() {
    let all = corpus();

    let without: Vec<&CorpusBlob> = all.iter().filter(|b| b.opcode != OP_MOVEMENT).collect();
    assert_eq!(
        all.len() - without.len(),
        all.iter().filter(|b| b.opcode == OP_MOVEMENT).count(),
        "the removal took exactly the subject"
    );
    assert_eq!(
        without.iter().filter(|b| b.opcode == OP_MOVEMENT).count(),
        0,
        "the movement matcher still finds movement in a corpus with no movement"
    );
    assert_eq!(
        without.iter().filter(|b| b.opcode == OP_CREATE).count(),
        all.iter().filter(|b| b.opcode == OP_CREATE).count(),
        "removing 0xF74C must not disturb the 0xF745 count"
    );

    let no_events: Vec<&CorpusBlob> = all
        .iter()
        .filter(|b| b.opcode != OP_ORDERED_EVENT)
        .collect();
    let count_event = |v: &[&CorpusBlob], sub: u32| {
        v.iter()
            .filter(|b| b.opcode == OP_ORDERED_EVENT && sub_opcode(b) == Some(sub))
            .count()
    };
    let count_action = |v: &[&CorpusBlob], sub: u32| {
        v.iter()
            .filter(|b| b.opcode == OP_ORDERED_ACTION && sub_opcode(b) == Some(sub))
            .count()
    };
    let present: Vec<&CorpusBlob> = all.iter().collect();
    assert!(count_event(&present, 0x0022) > 0);
    assert_eq!(
        count_event(&no_events, 0x0022),
        0,
        "0x0022 survived the removal of its envelope"
    );
    assert_eq!(
        count_action(&no_events, 0xF61C),
        count_action(&present, 0xF61C),
        "removing the game events must not disturb the game actions"
    );
}

#[test]
fn every_populated_scene_message_is_server_to_client() {
    let all = corpus();
    for op in [OP_CREATE, OP_MOVEMENT, OP_POSITION] {
        let messages: Vec<_> = all.iter().filter(|b| b.opcode == op).collect();
        assert!(
            !messages.is_empty(),
            "the movement replay has input for {op:#06X}"
        );
        for b in messages {
            assert_eq!(b.dir, Direction::ServerToClient);
        }
    }
}

#[test]
fn the_creates_that_carry_an_embedded_movement_buffer() {
    let all = corpus();
    let mut creates = 0usize;
    let mut with_movement = 0usize;
    let mut decoded = 0usize;
    for b in all.iter().filter(|b| b.opcode == OP_CREATE) {
        creates += 1;
        let msg = dereth_protocol::read_body::<dereth_protocol::objects::ItemCreateObject>(
            &b.payload[4..],
        )
        .expect("every recorded 0xF745 decodes");
        let Some((buf, _autonomous)) = msg.0.physicsdesc.movement.clone() else {
            continue;
        };
        if buf.is_empty() {
            continue;
        }
        with_movement += 1;
        let mut r = dereth_protocol::Reader::new(&buf);
        if dereth_protocol::movement::MovementBody::read(&mut r)
            .and_then(|body| r.expect_exhausted().map(|()| body))
            .is_ok()
        {
            decoded += 1;
        }
    }
    assert_eq!(
        creates,
        all.iter().filter(|b| b.opcode == OP_CREATE).count()
    );
    assert_eq!(
        with_movement, decoded,
        "creates whose PhysicsDesc carries a non-empty movement buffer"
    );
    assert_eq!(
        decoded, with_movement,
        "and every one of them decodes and consumes exactly"
    );
}

#[test]
fn every_recorded_movement_buffer_decodes_its_arm() {
    let all = corpus();
    let mut per_arm: BTreeMap<u8, usize> = BTreeMap::new();
    let mut decoded = 0usize;
    for b in all.iter().filter(|b| b.opcode == OP_MOVEMENT) {
        let msg = dereth_protocol::read_body::<MovementSetObjectMovement>(&b.payload[4..])
            .expect("every recorded 0xF74C decodes");
        let buf = msg
            .decoded_movement()
            .expect("every recorded movement buffer consumes its bytes exactly");
        decoded += 1;
        *per_arm.entry(buf.body.movement_type).or_default() += 1;
        if buf.body.movement_type == movement_type::INVALID {
            assert!(buf.body.interpreted.is_some(), "{}/{}", b.idx, b.opcode);
            assert!(buf.body.unhandled.is_empty());
        } else {
            assert!(buf.body.interpreted.is_none());
            assert!(buf
                .body
                .decode_move_to()
                .expect("the arm decodes")
                .is_some());
        }
    }
    assert!(decoded > 0);
    assert_eq!(
        decoded,
        all.iter().filter(|b| b.opcode == OP_MOVEMENT).count()
    );

    for arm in [
        movement_type::INVALID,
        movement_type::MOVE_TO_OBJECT,
        movement_type::MOVE_TO_POSITION,
        movement_type::TURN_TO_OBJECT,
        movement_type::TURN_TO_HEADING,
    ] {
        assert!(
            per_arm.get(&arm).is_some_and(|n| *n > 0),
            "movement arm {arm} is exercised"
        );
    }
    assert_eq!(per_arm.values().sum::<usize>(), decoded);
}
