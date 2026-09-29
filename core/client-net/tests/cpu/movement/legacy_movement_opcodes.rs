//! Contracts for legacy movement opcodes.
//! Fixture: shared recorded messages and synthetic state.

use std::collections::BTreeMap;

use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};

fn u32_at(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

const OP_ORDERED_ACTION: u32 = 0xF7B1;

fn client_actions() -> BTreeMap<u32, usize> {
    let mut out: BTreeMap<u32, usize> = BTreeMap::new();
    let mut blobs = 0usize;
    for c in Corpus::shared_all() {
        for b in &c.blobs {
            blobs += 1;
            if b.dir != Direction::ClientToServer || b.opcode != OP_ORDERED_ACTION {
                continue;
            }
            assert!(b.payload.len() >= 12, "a game action carries a sub-opcode");
            *out.entry(u32_at(&b.payload, 8)).or_default() += 1;
        }
    }
    assert!(blobs > 0);
    out
}

/// Behaviour: movement.legacy-opcodes.the-client-never-sends-the-four-legacy-movement-commands
#[test]
fn the_recorded_client_sends_none_of_the_four() {
    let actions = client_actions();

    assert!(actions.values().sum::<usize>() > 0);
    assert!(!actions.is_empty());
    assert!(actions.get(&0xF61C).is_some_and(|n| *n > 0));
    assert!(actions.get(&0xF753).is_some_and(|n| *n > 0));
    assert!(actions.get(&0xF61B).is_some_and(|n| *n > 0));

    for (op, name) in [
        (0xF61Eu32, "Movement_DoMovementCommand"),
        (0xF661, "Movement_StopMovementCommand"),
        (0xF649, "Movement_TurnToEvent"),
        (0xF752, "Movement_AutonomyLevel"),
    ] {
        assert_eq!(
            actions.get(&op),
            None,
            "{op:#06X} {name} was never recorded"
        );
    }
}

#[test]
fn the_sub_opcode_offset_this_census_reads_is_the_right_one() {
    let c = Corpus::shared("first-login-walk-jump");
    let first: &CorpusBlob = c
        .blobs
        .iter()
        .find(|b| b.dir == Direction::ClientToServer && b.opcode == OP_ORDERED_ACTION)
        .expect("first-login-walk-jump has client game actions");
    assert_eq!(
        u32_at(&first.payload, 0),
        OP_ORDERED_ACTION,
        "the blob opens with its own opcode"
    );
    let sub = u32_at(&first.payload, 8);
    assert!(
        (0x0001..=0xF7FF).contains(&sub),
        "sub-opcode {sub:#06X} is not an opcode at all"
    );
}
