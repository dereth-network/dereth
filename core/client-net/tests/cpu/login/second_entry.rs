//! In world, out, in again: the second entry succeeds, its first action carries stamp 1, the
//! counter resets even without a log-off, transport calls are reported, and nothing of the first
//! session survives.
//! Fixture: recorded messages and synthetic state or packets.

use super::common::*;
use dereth_client_net::client_session::testing::MockTransport;
use dereth_client_net::client_session::{Session, SessionEvent, SessionState};
use dereth_primitives::{NetQueue, ObjectId};
use dereth_protocol::login::{
    CharacterIdentity, LoginCharacterSet, LoginEnterGameServerReady, LoginExecuteLogOff,
};
use dereth_protocol::{write_blob, Message};

const LARK: ObjectId = ObjectId(0x5000_0001);
const ALDIS: ObjectId = ObjectId(0x5000_0002);

fn character_set() -> LoginCharacterSet {
    LoginCharacterSet {
        status: 0,
        characters: vec![
            CharacterIdentity {
                gid: LARK,
                name: "Lark".into(),
                seconds_greyed_out: 0,
            },
            CharacterIdentity {
                gid: ALDIS,
                name: "Aldis".into(),
                seconds_greyed_out: 0,
            },
        ],
        deleted: vec![],
        num_allowed_characters: 5,
        account: "ac01".into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 0,
    }
}

fn deliver<M: Message>(s: &mut Session<MockTransport>, m: &M) {
    s.transport
        .deliver_blob(NetQueue::UiQueue, &write_blob(m).expect("encodes"));
}

/// Connect and land on character select.
fn at_character_select() -> Session<MockTransport> {
    let mut s = session();
    deliver(&mut s, &character_set());
    s.tick(t(0.0));
    assert_eq!(s.state(), SessionState::CharacterSelect);
    s
}

/// The whole two-step entry: `0xF7C8` out, `0xF7DF` in, `0xF657` out, `0x0013` in.
fn enter_world(s: &mut Session<MockTransport>, who: ObjectId, now: f64) {
    s.enter_world(who, "ac01");
    deliver(s, &LoginEnterGameServerReady);
    s.tick(t(now));
    deliver(s, &player_description());
    s.tick(t(now + 1.0));
    assert_eq!(
        s.state(),
        SessionState::Playable,
        "0x0013 is what makes the client in world"
    );
}

/// The server's answer, six seconds later: `0xF653` and `0xF658` in the same instant.
fn server_logs_us_off(s: &mut Session<MockTransport>, now: f64) {
    deliver(s, &LoginExecuteLogOff);
    deliver(s, &character_set());
    s.tick(t(now));
}

// ---------------------------------------------------------------------------------------------
// The acceptance: a second entry from the same client session
// ---------------------------------------------------------------------------------------------

/// Behaviour: login.second-entry.a-second-entry-from-the-same-session-succeeds-with-fresh-state
/// **The acceptance behavior, asserted rather than observed:** a second entry to the world from
/// the same client session succeeds.
///
/// Three stations. The second character is deliberately a **different** one, because the failure
/// mode this is guarding against — `Flow::selected_character` surviving the log-off — passes a test
/// that re-enters as the same character.
#[test]
fn a_second_entry_to_the_world_from_the_same_session_succeeds() {
    let mut s = at_character_select();

    // Station 1 — in the world as Lark.
    enter_world(&mut s, LARK, 10.0);
    let first_entry: Vec<u32> = s.transport.sent_opcodes();
    assert_eq!(
        first_entry,
        vec![0xF7C8, 0xF657],
        "the two-step exchange, in order"
    );

    // The player asks to leave. The client stays in the world: nothing changes here but the
    // request going out.
    s.transport.sent.clear();
    s.log_off();
    assert_eq!(s.transport.sent_opcodes(), vec![0xF653]);
    assert_eq!(
        s.state(),
        SessionState::Playable,
        "asking to log off does not leave the world -- the recorded client waits 6 s"
    );

    // Station 2 — the server answers.
    s.transport.sent.clear();
    server_logs_us_off(&mut s, 20.0);
    assert_eq!(s.state(), SessionState::CharacterSelect);
    assert!(s.player_id().is_none(), "the player id is cleared");
    assert!(
        s.transport.sent.is_empty(),
        "the teardown puts nothing on the wire; all five recordings end silent"
    );

    // Station 3 — in again, as somebody else.
    enter_world(&mut s, ALDIS, 30.0);
    assert_eq!(
        s.transport.sent_opcodes(),
        vec![0xF7C8, 0xF657],
        "the second entry is the same two-step exchange as the first"
    );
    // And it is addressed to the character that was picked the *second* time.
    assert_eq!(
        s.transport.sent[1].payload,
        write_blob(&dereth_protocol::login::LoginSendEnterWorld {
            character: ALDIS,
            account: "ac01".into(),
        })
        .expect("encodes"),
        "0xF657 names Aldis, not the character the previous session was"
    );
}

/// The session-visible half of:
/// resetting the UI event counter to 0, and therefore **the second session's first game
/// action carries stamp 1**, exactly as the first session's did.
///
/// The stamp is asserted as a *literal* at all three stations rather than as a difference, because
/// a test that compares the counter with itself cannot see a wrong reset. A test that reads a
/// constant through the same symbol it writes through cannot validate that constant.
#[test]
fn the_second_sessions_first_action_carries_stamp_one() {
    let mut s = at_character_select();
    enter_world(&mut s, LARK, 10.0);

    // Station 1 — the first session climbs.
    for want in 1..=5u32 {
        let got = s
            .send_action(&dereth_protocol::comms::CommunicationTalk {
                message: "hi".into(),
            })
            .expect("a game action");
        assert_eq!(got, want, "first session");
    }
    assert_eq!(s.next_action_stamp(), 6);

    // Station 2 — out.
    s.log_off();
    server_logs_us_off(&mut s, 20.0);
    assert_eq!(
        s.next_action_stamp(),
        1,
        "the action counter is reset to 0, so the next value issued is 1"
    );

    // Station 3 — in again.
    enter_world(&mut s, ALDIS, 30.0);
    let first = s
        .send_action(&dereth_protocol::comms::CommunicationTalk {
            message: "hi again".into(),
        })
        .expect("a game action");
    assert_eq!(
        first, 1,
        "the second session opens at 1, like every recorded session does"
    );
}

/// An entry that follows no log off still resets the counter.
#[test]
fn an_entry_that_follows_no_log_off_still_resets_the_counter() {
    let mut s = at_character_select();

    // Spend some stamps at character select. `early-inventory-and-casting` of the corpus does
    // exactly this: its one post-log-off client action is a `0xF7B1` sent from the character
    // screen.
    for _ in 0..7 {
        s.send_action(&dereth_protocol::comms::CommunicationTalk {
            message: "x".into(),
        })
        .expect("a game action");
    }
    assert_eq!(s.next_action_stamp(), 8, "station 1: the counter has moved");

    // Station 2 — phase 1 of a world entry, and nothing else.
    s.enter_world(LARK, "ac01");
    assert_eq!(
        s.next_action_stamp(),
        1,
        "the character log-on's phase 1 exit-world disconnect ran before the 0xF7C8"
    );

    // Station 3 — and the `0xF7DF` handler runs it a second time, which is why a counter that
    // moved between the two phases is still 0 by the time the world is entered.
    s.send_action(&dereth_protocol::comms::CommunicationTalk {
        message: "y".into(),
    })
    .expect("a game action");
    assert_eq!(s.next_action_stamp(), 2);
    deliver(&mut s, &LoginEnterGameServerReady);
    s.tick(t(11.0));
    assert_eq!(
        s.next_action_stamp(),
        1,
        "the enter-game server-ready handler's exit-world disconnect"
    );
}

/// The transport half of the same three edges is the host's to make, because [`Session`] reaches
/// its transport only through the two-method `Transport` seam. This asserts the **count** the
/// session hands over, at each station, so a host that stops applying them shows up here rather
/// than as a silence.
///
/// Oracle: the four exit-world disconnect sites and the single enter-world transport call.
#[test]
fn the_host_is_told_about_every_call_the_client_makes_on_the_transport() {
    let mut s = at_character_select();
    assert_eq!(s.take_transport_calls(), (0, 0), "nothing yet");

    // Phase 1.
    s.enter_world(LARK, "ac01");
    assert_eq!(s.take_transport_calls(), (1, 0), "character log-on phase 1");

    // `0xF7DF` — the second teardown, then phase 2's `EnterWorld`.
    deliver(&mut s, &LoginEnterGameServerReady);
    s.tick(t(10.0));
    assert_eq!(
        s.take_transport_calls(),
        (1, 1),
        "the server-ready handler runs before transport enter-world"
    );

    deliver(&mut s, &player_description());
    s.tick(t(11.0));
    s.log_off();
    assert_eq!(
        s.take_transport_calls(),
        (0, 0),
        "asking to log off tears nothing down"
    );

    server_logs_us_off(&mut s, 20.0);
    assert_eq!(
        s.take_transport_calls(),
        (1, 0),
        "executing logoff disconnects from the world"
    );
}

/// The residual state, named field by field.
///
/// The row asks *"name what state was retained"*, so this asserts the state that must **not** be
/// retained rather than only that the next entry works — a session could pass the acceptance above
/// while carrying stale ordering windows, and the failure would surface much later as a dropped
/// event for an object that no longer exists.
#[test]
fn nothing_of_the_first_session_survives_into_the_second() {
    let mut s = at_character_select();
    enter_world(&mut s, LARK, 10.0);

    // Give the session something to hold: a player id from `0xF746`, and a stamp on the counter.
    s.transport.deliver_blob(
        NetQueue::WorldObjects,
        &write_blob(&dereth_protocol::objects::LoginCreatePlayer { player_id: LARK })
            .expect("encodes"),
    );
    s.tick(t(12.0));
    s.send_action(&dereth_protocol::comms::CommunicationTalk {
        message: "hi".into(),
    })
    .expect("a game action");

    // Station 1.
    assert_eq!(s.player_id(), Some(LARK));
    assert_eq!(s.next_action_stamp(), 2);
    assert_eq!(s.state(), SessionState::Playable);

    // Station 2.
    s.log_off();
    s.transport.sent.clear();
    server_logs_us_off(&mut s, 20.0);
    let events: Vec<SessionEvent> = s.drain_events().collect();
    assert!(events.contains(&SessionEvent::LoggedOff));
    assert!(events.contains(&SessionEvent::StateChanged(SessionState::CharacterSelect)));
    assert_eq!(s.player_id(), None, "the player id is cleared to 0");
    assert_eq!(s.next_action_stamp(), 1, "the action counter is reset to 0");
    // The account survives on purpose: the account field is not cleared by
    // the log-off, and `0xF657` needs it.
    assert_eq!(
        s.account(),
        "ac01",
        "the account is *not* residual state -- re-entry needs it"
    );
    assert_eq!(
        s.characters().characters.len(),
        2,
        "nor is the character list"
    );

    // Station 3.
    enter_world(&mut s, ALDIS, 30.0);
    assert_eq!(s.state(), SessionState::Playable);
}
