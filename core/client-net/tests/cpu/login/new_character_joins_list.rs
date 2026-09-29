//! A successful char-gen response adds the new identity and raises one character-set notice;
//! repeats, unsolicited responses and refusals add nothing; add_identity dedupes by gid; opcode/ok-
//! code literals.
//! Fixture: recorded messages and synthetic state or packets.

use super::common::*;
use dereth_client_net::client_session::testing::MockTransport;
use dereth_client_net::client_session::{
    Session, SessionEvent, SessionState, CG_VERIFICATION_RESPONSE_OK,
};
use dereth_primitives::{NetQueue, ObjectId};
use dereth_protocol::login::{CharGenResult, CharacterIdentity, LoginCharacterSet};
use dereth_protocol::{write_blob, Opcode};

/// The account as it stands **before** the creation: one character, `Lark`.
fn existing_set() -> LoginCharacterSet {
    LoginCharacterSet {
        status: 0,
        characters: vec![CharacterIdentity {
            gid: ObjectId(0x5000_0001),
            name: "Lark".into(),
            seconds_greyed_out: 0,
        }],
        deleted: vec![],
        num_allowed_characters: 5,
        account: "ac01".into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 1,
    }
}

/// The new character, exactly as ACE's `GameMessageCharacterCreateResponse` writes it: the guid,
/// the name **without** a `+` even on a plussed account, and a zero `secondsGreyedOut`.
fn new_identity() -> CharacterIdentity {
    CharacterIdentity {
        gid: ObjectId(0x5000_0002),
        name: "Tarinell".into(),
        seconds_greyed_out: 0,
    }
}

/// A session that has authenticated and is sitting on character select.
fn on_character_select() -> Session<MockTransport> {
    let mut s = Session::new(MockTransport::new());
    s.transport
        .deliver_blob(NetQueue::UiQueue, &write_blob(&existing_set()).unwrap());
    s.tick(t(0.0));
    assert_eq!(s.state(), SessionState::CharacterSelect);
    assert_eq!(s.characters().characters.len(), 1);
    let _ = s.drain_events().count();
    s
}

/// Behaviour: chargen.finish.the-created-character-joins-the-list-without-a-relaunch
/// **Symptom 2, asserted on its own**: the created character is in the list, in this session, with
/// no second `0xF658` and no relaunch.
#[test]
fn the_created_character_joins_the_list_without_a_second_character_set_message() {
    let mut s = on_character_select();

    s.create_character(CharGenResult {
        name: "Tarinell".into(),
        ..CharGenResult::default()
    });
    assert_eq!(
        s.transport.sent_opcodes(),
        vec![0xF656],
        "finishing character creation sends its result"
    );

    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&response(CG_VERIFICATION_RESPONSE_OK, new_identity())).unwrap(),
    );
    s.tick(t(1.0));

    let set = s.characters();
    assert_eq!(
        set.characters.len(),
        2,
        "adding an identity appends; the account had one character and now has two"
    );
    assert_eq!(
        set.characters[0].gid,
        ObjectId(0x5000_0001),
        "the existing one keeps its slot"
    );
    assert_eq!(
        set.characters[1],
        new_identity(),
        "the new one is appended at the end, gid and name and grace period from 0xF643's own body"
    );
    // Nothing else about the account moved: adding the identity touches the character list and
    // nothing beside it.
    assert_eq!(set.account, "ac01");
    assert_eq!(set.num_allowed_characters, 5);
    assert_eq!(set.has_throne_of_destiny, 1);
    assert!(set.deleted.is_empty());
}

/// **Symptom 1's producer, asserted on its own**: the character-set notice is raised, and it is
/// raised **after** the char-gen response, which is the order the handler uses and the order the
/// wizard needs — the wizard's own verification-response notice sets its
/// awaiting-character-set flag, and only then can act on the set.
#[test]
fn the_character_set_notice_is_raised_after_the_char_gen_response() {
    let mut s = on_character_select();
    s.create_character(CharGenResult {
        name: "Tarinell".into(),
        ..CharGenResult::default()
    });
    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&response(CG_VERIFICATION_RESPONSE_OK, new_identity())).unwrap(),
    );
    s.tick(t(1.0));

    let events: Vec<SessionEvent> = s.drain_events().collect();
    let response_at = events
        .iter()
        .position(|e| matches!(e, SessionEvent::CharGenResponse(_)))
        .expect("the char-gen verification-response notice");
    let set_at = events
        .iter()
        .position(|e| matches!(e, SessionEvent::CharacterSet(_)))
        .expect(
            "the character-set notice -- without this the wizard never leaves the \
             summary page and the character list is never rebuilt",
        );
    assert!(
        response_at < set_at,
        "the char-gen notice must come first, or the wizard has not set \
         its awaiting-character-set flag when the set arrives: {events:?}"
    );

    let SessionEvent::CharacterSet(set) = &events[set_at] else {
        unreachable!()
    };
    assert!(
        set.characters.iter().any(|c| *c == new_identity()),
        "the notice carries the *updated* set, not the one 0xF658 delivered"
    );
    // Exactly one, not one per event drained: the character-set notice is guarded by `added`.
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::CharacterSet(_)))
            .count(),
        1
    );
}

/// The add-identity dedupe, which is by character id and by nothing else — so a repeated
/// `0xF643` is harmless. The asymmetric case matters here: a test that only ever sends one
/// response cannot tell an append from an append-with-dedupe.
#[test]
fn a_repeated_response_adds_nothing_and_raises_no_second_notice() {
    let mut s = on_character_select();
    s.create_character(CharGenResult {
        name: "Tarinell".into(),
        ..CharGenResult::default()
    });
    for _ in 0..3 {
        s.transport.deliver_blob(
            NetQueue::UiQueue,
            &write_blob(&response(CG_VERIFICATION_RESPONSE_OK, new_identity())).unwrap(),
        );
    }
    s.tick(t(1.0));

    assert_eq!(
        s.characters().characters.len(),
        2,
        "three responses, one character"
    );
    let events: Vec<SessionEvent> = s.drain_events().collect();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::CharacterSet(_)))
            .count(),
        1,
        "the notice is guarded by the add-identity result, so only the first response raises it"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::CharGenResponse(_)))
            .count(),
        3,
        "every response is still handed over -- the guard is on the set, not on the notice"
    );
}

/// A response with no creation outstanding adds nothing.
#[test]
fn a_response_with_no_creation_outstanding_adds_nothing() {
    let mut s = on_character_select();
    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&response(CG_VERIFICATION_RESPONSE_OK, new_identity())).unwrap(),
    );
    s.tick(t(1.0));

    assert_eq!(
        s.characters().characters.len(),
        1,
        "no create was outstanding"
    );
    let events: Vec<SessionEvent> = s.drain_events().collect();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SessionEvent::CharGenResponse(_))),
        "the response is still delivered"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::CharacterSet(_))),
        "and it raises no character-set notice"
    );
}

/// Every refusal code: the handler's `case 3` (name in use), `case 4` and the `default:` arm all
/// clear the verification state and add nothing. The wizard shows the error and stays put, which
/// the wizard's verification-response notice already implements.
#[test]
fn a_refusal_adds_nothing_and_clears_the_pending_flag() {
    for code in [0u32, 2, 3, 4, 5, 6, 7] {
        let mut s = on_character_select();
        s.create_character(CharGenResult {
            name: "Tarinell".into(),
            ..CharGenResult::default()
        });
        s.transport.deliver_blob(
            NetQueue::UiQueue,
            &write_blob(&response(code, new_identity())).unwrap(),
        );
        s.tick(t(1.0));
        assert_eq!(
            s.characters().characters.len(),
            1,
            "response {code} is not CG_VERIFICATION_RESPONSE_OK and must add nothing"
        );
        let events: Vec<SessionEvent> = s.drain_events().collect();
        assert!(!events
            .iter()
            .any(|e| matches!(e, SessionEvent::CharacterSet(_))));

        // And the latch is cleared, so a *later* stray OK cannot append on the strength of a
        // create the server already refused. The verification state is reset on every arm.
        s.transport.deliver_blob(
            NetQueue::UiQueue,
            &write_blob(&response(CG_VERIFICATION_RESPONSE_OK, new_identity())).unwrap(),
        );
        s.tick(t(2.0));
        assert_eq!(
            s.characters().characters.len(),
            1,
            "the pending latch survived a refusal of code {code}"
        );
    }
}

/// The constant, pinned against the wire rather than read back through the symbol that writes it.
///
/// Oracle: the handler's switch (`case 1:` is the OK arm) and ACE's
/// `CharacterGenerationVerificationResponse.Ok = 1`.
#[test]
fn the_ok_code_is_one_and_the_opcode_is_f643() {
    assert_eq!(CG_VERIFICATION_RESPONSE_OK, 1);
    assert_eq!(Opcode::CHARACTER_CHAR_GEN_VERIFICATION_RESPONSE.0, 0xF643);
    assert_eq!(Opcode::CHARACTER_SEND_CHAR_GEN_RESULT.0, 0xF656);
    assert_eq!(Opcode::LOGIN_LOGIN_CHARACTER_SET.0, 0xF658);
}

/// Adding an identity on its own, including the case the handler cannot reach:
/// a *different* character with a name that is already taken is still added, because the dedupe
/// looks at `gid_` and nothing else.
#[test]
fn add_identity_dedupes_by_gid_and_not_by_name() {
    let mut set = existing_set();
    assert!(!set.add_identity(&CharacterIdentity {
        gid: ObjectId(0x5000_0001),
        name: "Somebody Else".into(),
        seconds_greyed_out: 9,
    }));
    assert_eq!(set.characters.len(), 1);
    assert_eq!(
        set.characters[0].name, "Lark",
        "a refused add changes nothing"
    );

    assert!(set.add_identity(&CharacterIdentity {
        gid: ObjectId(0x5000_0003),
        name: "Lark".into(),
        seconds_greyed_out: 0,
    }));
    assert_eq!(set.characters.len(), 2, "same name, different gid: added");
}
