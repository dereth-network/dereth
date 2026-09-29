//! Contracts for request lock guard.
//! Fixture: shared recorded messages and synthetic state.

use dereth_client_model::inventory::requests::{InventoryRequest, RequestLock};
use dereth_client_model::{Notice, RecordingSink, World};
use dereth_client_net::client_session::testing::Corpus;
use dereth_primitives::{ObjectId, ServerTime};

const LOCKED: ObjectId = ObjectId(0x8000_0111);
const OTHER: ObjectId = ObjectId(0x8000_0222);

fn held() -> RequestLock {
    let mut l = RequestLock::default();
    l.record(LOCKED, InventoryRequest::Move, ServerTime(12.5));
    l
}

fn retail_clear(lock: &mut RequestLock, id: ObjectId) {
    lock.clear_if_matches(id);
}

fn arm_resolves(lock: &RequestLock, message_names: ObjectId) -> ObjectId {
    match lock.object {
        Some(own) => own,      // the held lock: the client's own id wins
        None => message_names, // the idle lock: the id the message names
    }
}

#[test]
fn the_guard_and_the_unconditional_clear_disagree_on_a_foreign_id() {
    let mut guarded = held();
    retail_clear(&mut guarded, OTHER);
    assert_eq!(
        guarded.object,
        Some(LOCKED),
        "retail: the guard skips the four stores, the lock survives"
    );
    assert_eq!(guarded.pending, InventoryRequest::Move);
    assert_eq!(
        guarded.at,
        ServerTime(12.5),
        "the request time survives too"
    );

    let mut ours = held();
    ours.clear();
    assert!(
        ours.is_idle() && ours.object.is_none(),
        "this build: the lock is released whatever the id"
    );
    assert_eq!(
        ours.at,
        ServerTime(0.0),
        "and all three variables go to zero, matching the four stores of zero the guarded body \
         skips -- the request time's two halves included. Asserted here rather than only inside \
         the differential below, because a differential in which the subject is present in BOTH \
         arms is blind to it (`CONVENTIONS.md`)."
    );

    assert_ne!(
        guarded, ours,
        "the two versions are NOT equal in general -- the equivalence is a claim about which \
         inputs can reach them, and that claim is the test below"
    );
}

/// Behaviour: inventory.request-lock.an-attempt-failed-always-releases-a-held-lock-whatever-object-it-names
#[test]
fn the_00a0_arm_never_produces_the_case_the_guard_rejects() {
    let mut reached_the_disagreement = 0usize;
    let mut idle_cases = 0usize;
    for lock_held in [true, false] {
        for named in [LOCKED, OTHER, ObjectId(0)] {
            let base = if lock_held {
                held()
            } else {
                RequestLock::default()
            };
            let resolved = arm_resolves(&base, named);

            let mut guarded = base;
            retail_clear(&mut guarded, resolved);
            let mut ours = base;
            ours.clear();

            assert_eq!(
                guarded, ours,
                "held={lock_held} named={named:?}: the two versions must agree once the arm's own \
                 substitution is applied"
            );
            if lock_held {
                assert_eq!(
                    resolved, LOCKED,
                    "a held lock always wins over the message's id"
                );
                assert!(guarded.is_idle(), "and the clear therefore always fires");
            } else {
                idle_cases += 1;
                assert!(
                    guarded.is_idle() && ours.is_idle(),
                    "an idle lock: the guard skips and the clear is a no-op, so both \
                     versions leave it idle"
                );
            }
            if resolved != LOCKED && lock_held {
                reached_the_disagreement += 1;
            }
        }
    }
    assert_eq!(
        reached_the_disagreement, 0,
        "no (lock, message) pair reaches the disagreement through the arm"
    );
    assert_eq!(
        idle_cases, 3,
        "and the idle half of the matrix was exercised"
    );

    let mut without = 0usize;
    for named in [LOCKED, OTHER, ObjectId(0)] {
        let mut guarded = held();
        retail_clear(&mut guarded, named); // the message's id, unsubstituted
        let mut ours = held();
        ours.clear();
        if guarded != ours {
            without += 1;
        }
    }
    assert_eq!(
        without, 2,
        "two of the three message ids disagree when the substitution is removed, which is what \
         makes the zero above a measurement of the substitution rather than of nothing"
    );
}

#[test]
fn this_build_can_reach_the_case_retail_cannot() {
    let mut w = World::new();
    w.request_lock
        .record(LOCKED, InventoryRequest::Move, ServerTime(12.5));

    let mut out = RecordingSink::default();
    w.server_says_attempt_failed(OTHER, 0x1D, &mut out);

    assert!(
        w.request_lock.is_idle(),
        "the lock is released -- which is the retail outcome, reached for a different reason"
    );

    let object = out
        .0
        .iter()
        .find_map(|n| match n {
            Notice::AttemptFailed { object, .. } => Some(*object),
            _ => None,
        })
        .expect("the arm emits an AttemptFailed notice");
    assert_eq!(
        object, OTHER,
        "DEVIATION (), CLOSED AT THE CALLER (): this function has no substitution in \
         retail either, so an unsubstituted id run through it names the wrong object -- which is \
         what this asserts. the substitution is in the net-blob dispatch's arm, and \
         `interaction.rs` now performs it, so no message can arrive here with {OTHER:?} while the \
         lock names {LOCKED:?}."
    );

    let mut w2 = World::new();
    let mut out2 = RecordingSink::default();
    w2.server_says_attempt_failed(OTHER, 0x1D, &mut out2);
    assert!(w2.request_lock.is_idle());
    let object2 = out2
        .0
        .iter()
        .find_map(|n| match n {
            Notice::AttemptFailed { object, .. } => Some(*object),
            _ => None,
        })
        .expect("notice");
    assert_eq!(
        object2, OTHER,
        "with an idle lock retail keeps the message's own id too (the)"
    );
}

fn netblob_opcodes() -> (usize, Vec<(u32, u32)>) {
    use dereth_client_net::client_session::testing::Corpus;
    let rows: Vec<_> = Corpus::shared_all()
        .iter()
        .flat_map(|c| c.blobs.iter())
        .map(|b| {
            (
                b.opcode,
                u32::from(dereth_client_net::client_session::testing::queue_id(
                    b.queue,
                )),
            )
        })
        .collect();
    (rows.len(), rows)
}

#[test]
fn the_corpus_has_no_recorded_attempt_failed_and_the_census_can_see_ones_that_exist() {
    let (total, rows) = netblob_opcodes();
    assert_eq!(
        total,
        rows.len(),
        "whole-message blobs in the locked fixtures/message-corpus corpus"
    );

    let count = |op: u32| rows.iter().filter(|&&(o, _)| o == op).count();

    assert_eq!(
        count(0x00A0),
        0,
        "0x00A0 Character_ServerSaysAttemptFailed: 0 in the WHOLE-MESSAGE space of fixtures/message-corpus"
    );

    assert!(count(0x0197) > 0);
    assert!(count(0x0024) > 0);
    assert!(
        rows.iter().any(|&(o, q)| o == 0x0197 && q == 9),
        "and they really are on the UI queue (9), which is the queue 0x00A0 would arrive on"
    );

    let distinct: std::collections::BTreeSet<u32> = rows.iter().map(|&(o, _)| o).collect();
    assert!(!distinct.is_empty());
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Space {
    Whole,
    Event,
    Action,
}

fn netblob_spaces() -> (usize, usize, Vec<(Space, u32)>) {
    let (mut total, mut unexaminable) = (0usize, 0usize);
    let mut rows = Vec::new();
    for c in [
        "first-login-walk-jump",
        "early-inventory-and-casting",
        "short-second-connection",
        "login-account-booted",
        "ddd-interrogation-only",
        "long-solo-play",
        "short-play-with-training",
    ]
    .map(Corpus::shared)
    {
        for blob in &c.blobs {
            total += 1;
            let bytes = &blob.payload;
            let dword = |off: usize| -> Option<u32> {
                bytes
                    .get(off..off + 4)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            };
            match dword(0) {
                None => unexaminable += 1,
                Some(0xF7B0) => match dword(12) {
                    Some(sub) => rows.push((Space::Event, sub)),
                    None => unexaminable += 1,
                },
                Some(0xF7B1) => match dword(8) {
                    Some(sub) => rows.push((Space::Action, sub)),
                    None => unexaminable += 1,
                },
                Some(op) => rows.push((Space::Whole, op)),
            }
        }
    }
    (total, unexaminable, rows)
}

#[test]
fn the_attempt_failed_census_re_taken_in_both_envelopes() {
    let (total, unexaminable, rows) = netblob_spaces();
    assert_eq!(
        total,
        rows.len(),
        "the locked fixtures/message-corpus corpus"
    );
    assert_eq!(unexaminable, 0, "blobs too short to examine");

    let n = |space: Space, op: u32| rows.iter().filter(|&&(s, o)| s == space && o == op).count();
    let both = |op: u32| (n(Space::Whole, op), n(Space::Event, op));

    assert_eq!(both(0x00A0).0, 0);
    assert!(both(0x00A0).1 > 0);

    assert_eq!(both(0x0196).0, 0);
    assert!(both(0x0196).1 > 0);
    assert_eq!(both(0x0022).0, 0);
    assert!(both(0x0022).1 > 0);

    assert!(both(0x0197).0 > 0);
    assert_eq!(both(0x0197).1, 0);
    assert!(both(0x0024).0 > 0);
    assert_eq!(both(0x0024).1, 0);

    for (op, whole) in [
        (0x0022, false),
        (0x0023, false),
        (0x0024, true),
        (0x00A0, false),
        (0x0196, false),
        (0x0197, true),
        (0x019A, false),
    ] {
        let (w, e) = both(op);
        assert_eq!(
            (w > 0, e > 0),
            (whole, !whole),
            "{op:#06X}: envelope is independent of queue"
        );
    }
    let mut seen: std::collections::BTreeMap<u32, std::collections::BTreeSet<u8>> =
        std::collections::BTreeMap::new();
    for &(s, op) in &rows {
        seen.entry(op).or_default().insert(s as u8);
    }
    let shared: Vec<u32> = seen
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(&k, _)| k)
        .collect();
    assert!(
        shared.is_empty(),
        "opcodes in more than one space: {shared:#06X?}"
    );
    for space in [Space::Whole, Space::Event, Space::Action] {
        assert!(seen.values().any(|v| v.contains(&(space as u8))));
    }
}
