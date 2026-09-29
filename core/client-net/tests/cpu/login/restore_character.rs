//! A restore response clears the grace period in place and raises the character-set notice; create
//! and restore branches stay apart; unsolicited replies replace nothing; the slot is consumed once;
//! a refusal leaves the row; a body-less refusal decodes.
//! Fixture: recorded messages and synthetic state or packets.

use super::common::*;
use dereth_client_net::client_session::testing::MockTransport;
use dereth_client_net::client_session::{
    Session, SessionEvent, SessionState, CG_VERIFICATION_RESPONSE_OK,
};
use dereth_primitives::{NetQueue, ObjectId};
use dereth_protocol::login::{CharGenResult, CharacterIdentity, LoginCharacterSet};
use dereth_protocol::{write_blob, Opcode};

const KEEPER: ObjectId = ObjectId(0x5000_0001);
const DOOMED: ObjectId = ObjectId(0x5000_0002);

/// The account as ACE's `GameMessageCharacterList` writes it once a delete is pending: **both**
/// characters are in the first list, and the one being deleted carries a non-zero
/// `secondsGreyedOut`. The second list is the one ACE always writes as empty.
fn set_with_a_pending_delete() -> LoginCharacterSet {
    LoginCharacterSet {
        status: 0,
        characters: vec![
            CharacterIdentity {
                gid: KEEPER,
                name: "Larktest".into(),
                seconds_greyed_out: 0,
            },
            CharacterIdentity {
                gid: DOOMED,
                name: "Tarinell".into(),
                seconds_greyed_out: 3_600,
            },
        ],
        deleted: vec![],
        num_allowed_characters: 5,
        account: "acct0001".into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 1,
    }
}

/// ACE's `GameMessageCharacterRestore(guid, name, 0u)` — `[1u OK][guid][name][0u]`, which is
/// `CG_VERIFICATION_RESPONSE_OK` followed by a `CharacterIdentity` whose grace period is zero.
fn restored_identity() -> CharacterIdentity {
    CharacterIdentity {
        gid: DOOMED,
        name: "Tarinell".into(),
        seconds_greyed_out: 0,
    }
}

/// A session sitting on character select with one character pending deletion.
fn with_a_deleted_character() -> Session<MockTransport> {
    let mut s = Session::new(MockTransport::new());
    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&set_with_a_pending_delete()).unwrap(),
    );
    s.tick(t(0.0));
    assert_eq!(s.state(), SessionState::CharacterSelect);
    assert_eq!(
        s.characters().characters[1].seconds_greyed_out,
        3_600,
        "the premise: the second character is greyed out, which is what offers *Restore*"
    );
    let _ = s.drain_events().count();
    s
}

/// Behaviour: character-select.restore.the-answer-overwrites-the-place-it-was-asked-about-and-never-appends
/// A restore clears the characters grace period.
#[test]
fn a_restore_clears_the_characters_grace_period() {
    let mut s = with_a_deleted_character();

    s.restore_character(DOOMED);
    assert_eq!(
        s.transport.sent_opcodes(),
        vec![0xF7D9],
        "the restore request, on the Control queue"
    );

    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&response(CG_VERIFICATION_RESPONSE_OK, restored_identity())).unwrap(),
    );
    s.tick(t(1.0));

    let set = s.characters();
    assert_eq!(
        set.characters.len(),
        2,
        "GetIdentity + UnPack replaces a row; it does not append"
    );
    assert_eq!(
        set.characters[0].gid, KEEPER,
        "the other character keeps its slot"
    );
    assert_eq!(
        set.characters[1].seconds_greyed_out, 0,
        "the whole point: `restore: greyed == Some(true)` is what makes the screen show *Restore* \
         instead of *Delete* and *Enter*"
    );
    assert_eq!(set.characters[1], restored_identity());
}

/// A restore raises the character set notice.
#[test]
fn a_restore_raises_the_character_set_notice() {
    let mut s = with_a_deleted_character();
    s.restore_character(DOOMED);
    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&response(CG_VERIFICATION_RESPONSE_OK, restored_identity())).unwrap(),
    );
    s.tick(t(1.0));

    let events: Vec<SessionEvent> = s.drain_events().collect();
    let response_at = events
        .iter()
        .position(|e| matches!(e, SessionEvent::CharGenResponse(_)))
        .expect("the char-gen verification-response notice");
    let set_at = events.iter().position(|e| matches!(e, SessionEvent::CharacterSet(_))).expect(
        "the character-set notice -- without it the list is never rebuilt, and the please-wait \
         modal stays up forever because a wait dialog has no buttons to close it",
    );
    assert!(
        response_at < set_at,
        "the handler's order, as established it: {events:?}"
    );

    let SessionEvent::CharacterSet(set) = &events[set_at] else {
        unreachable!()
    };
    assert_eq!(
        set.characters[1].seconds_greyed_out, 0,
        "the notice carries the *updated* set, not the one 0xF658 delivered"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::CharacterSet(_)))
            .count(),
        1,
        "the character-set notice is guarded by `added`, so exactly one"
    );
}

/// The create branch and the restore branch stay apart.
#[test]
fn the_create_branch_and_the_restore_branch_stay_apart() {
    // Create: the wizard is pending, so the identity is appended.
    let mut s = with_a_deleted_character();
    s.create_character(CharGenResult {
        name: "Third".into(),
        ..CharGenResult::default()
    });
    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&response(
            CG_VERIFICATION_RESPONSE_OK,
            CharacterIdentity {
                gid: ObjectId(0x5000_0003),
                name: "Third".into(),
                seconds_greyed_out: 0,
            },
        ))
        .unwrap(),
    );
    s.tick(t(1.0));
    assert_eq!(
        s.characters().characters.len(),
        3,
        "PENDING -> adding the identity appends"
    );
    assert_eq!(
        s.characters().characters[1].seconds_greyed_out,
        3_600,
        "and the pending delete is untouched"
    );

    // Restore: nothing pending, so the row at the recorded slot is replaced.
    let mut s = with_a_deleted_character();
    s.restore_character(DOOMED);
    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&response(CG_VERIFICATION_RESPONSE_OK, restored_identity())).unwrap(),
    );
    s.tick(t(1.0));
    assert_eq!(
        s.characters().characters.len(),
        2,
        "not PENDING -> look up the slot and unpack over it"
    );
}

/// A `0xF643` that nothing asked for changes nothing. The character-generation slot is `-1` until a
/// selection or a restore stamps it, and retail finds no identity at slot `-1` — it
/// compares the slot unsigned against the row count, so a negative slot fails the bounds test rather than indexing
/// backwards.
#[test]
fn an_unsolicited_response_replaces_nothing() {
    let mut s = with_a_deleted_character();
    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&response(CG_VERIFICATION_RESPONSE_OK, restored_identity())).unwrap(),
    );
    s.tick(t(1.0));

    assert_eq!(
        s.characters().characters[1].seconds_greyed_out,
        3_600,
        "no slot was stamped, so GetIdentity returned null and nothing was written"
    );
    let events: Vec<SessionEvent> = s.drain_events().collect();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::CharacterSet(_))),
        "`added` is false, so the notice is not raised: {events:?}"
    );
}

/// One reply consumes one request. The verification-state reset and the slot clear to -1 both
/// run on every arm of the handler, so a second `0xF643` after the same restore finds no slot.
#[test]
fn the_slot_is_consumed_by_the_reply_that_uses_it() {
    let mut s = with_a_deleted_character();
    s.restore_character(DOOMED);
    for _ in 0..2 {
        s.transport.deliver_blob(
            NetQueue::UiQueue,
            &write_blob(&response(
                CG_VERIFICATION_RESPONSE_OK,
                CharacterIdentity {
                    gid: DOOMED,
                    name: "Renamed".into(),
                    seconds_greyed_out: 99,
                },
            ))
            .unwrap(),
        );
        s.tick(t(1.0));
    }
    assert_eq!(
        s.characters().characters[1].name,
        "Renamed",
        "the first reply landed"
    );
    assert_eq!(
        s.characters().characters[1].seconds_greyed_out,
        99,
        "and the second found no slot, so it did not land twice"
    );
    let sets = s
        .drain_events()
        .filter(|e| matches!(e, SessionEvent::CharacterSet(_)))
        .count();
    assert_eq!(sets, 1, "one notice, not two");
}

/// A refusal writes nothing. Result 3 `NameInUse` and result 4 `NameBanned` both clear the slot and
/// leave `added` false; ACE reaches the first of them from
/// `CharacterHandler.CharacterRestore`'s `IsCharacterNameAvailable` callback, so it is a live arm
/// on this shard and not a theoretical one.
#[test]
fn a_refused_restore_leaves_the_row_alone() {
    const NAME_IN_USE: u32 = 3;
    let mut s = with_a_deleted_character();
    s.restore_character(DOOMED);
    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&response(NAME_IN_USE, restored_identity())).unwrap(),
    );
    s.tick(t(1.0));

    assert_eq!(
        s.characters().characters[1].seconds_greyed_out,
        3_600,
        "somebody took the name while it was deleted; the character stays deleted"
    );
    let events: Vec<SessionEvent> = s.drain_events().collect();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SessionEvent::CharGenResponse(_))),
        "the screen is still told, so it can show the error: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::CharacterSet(_))),
        "but no set is raised: {events:?}"
    );
}

/// Behaviour: character-select.restore.the-request-that-leaves-the-client-is-the-characters-own-id-on-the-control-queue
/// The request is still the documented three field message.
#[test]
fn the_request_is_still_the_documented_three_field_message() {
    let mut s = with_a_deleted_character();
    s.restore_character(DOOMED);
    assert_eq!(s.transport.sent_opcodes(), vec![Opcode(0xF7D9).0]);
}

/// A body less refusal decodes and reaches the screen.
#[test]
fn a_body_less_refusal_decodes_and_reaches_the_screen() {
    const NAME_IN_USE: u32 = 3;
    let mut s = with_a_deleted_character();
    s.restore_character(DOOMED);

    // ACE's bytes, built by hand so that our own writer cannot define the oracle.
    let mut blob = Vec::new();
    blob.extend_from_slice(&0xF643_u32.to_le_bytes());
    blob.extend_from_slice(&NAME_IN_USE.to_le_bytes());
    assert_eq!(blob.len(), 8);
    s.transport.deliver_blob(NetQueue::UiQueue, &blob);
    s.tick(t(1.0));

    let events: Vec<SessionEvent> = s.drain_events().collect();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::Dropped { .. })),
        "the refusal must not be dropped as malformed: {events:?}"
    );
    let got = events
        .iter()
        .find_map(|e| match e {
            SessionEvent::CharGenResponse(r) => Some(r.response_type),
            _ => None,
        })
        .expect("the char-gen verification-response notice runs on every arm");
    assert_eq!(got, NAME_IN_USE);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::CharacterSet(_))),
        "and `added` is false, so no set is raised: {events:?}"
    );

    // And our own writer produces exactly those bytes, so a fixture built the ordinary way is
    // ACE's message.
    assert_eq!(
        write_blob(&response(NAME_IN_USE, restored_identity())).unwrap(),
        blob,
        "write is symmetric with read: no identity on a refusal"
    );
}
