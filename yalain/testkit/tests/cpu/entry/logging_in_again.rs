//! Logging in again: a character whose client went away without logging off lingers and leaves
//! by itself; a new login of its account logs it off; an account has one character in the world
//! at a time, entries refused with reason 11 until its character has left; a log-off at the
//! character list is not answered. Fixture: bots of the test world, one of them gone silent as a
//! client that was killed (its address unreachable), another watching from beside it.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use auberean_protocol::login::{CharacterRef, CharacterRefused, PlayCharacter};
use auberean_protocol::text::NarrowString;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_testkit::bot::Received;
use yalain_testkit::Harness;
use yalain_world::world::{log_off, ARRIVAL_FX};

/// Run the world until `seconds` after `since`.
fn until(h: &mut Harness, since: f64, seconds: f64) {
    let t = since + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// A client's process ends without a word: nothing more reaches the server from it, nor it from
/// the server.
fn killed(h: &mut Harness, b: usize) {
    let port = 59_000 + u16::try_from(b).unwrap_or(0);
    h.bots[b].addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
}

fn play(account: &str, character: u64) -> ClientMessage {
    ClientMessage::PlayCharacter(PlayCharacter {
        character,
        account: NarrowString::from(account),
    })
}

/// The answer to an entry asked for now: a refusal's reason, or `None` for a referral.
fn ask_entry(h: &mut Harness, b: usize, account: &str, character: u64) -> Option<u32> {
    let before = h.bots[b].inbox.len();
    let referrals = h.bots[b].referrals.len();
    h.bots[b].send(false, queue::LOGON, &play(account, character));
    let answered = h.run_until(400, |h| {
        h.bots[b].referrals.len() > referrals
            || h.bots[b].inbox[before..]
                .iter()
                .any(|r| matches!(r.message(), Some(ServerMessage::CharacterRefused(_))))
    });
    assert!(answered, "the entry is answered");
    h.bots[b].inbox[before..]
        .iter()
        .find_map(|r| match r.message() {
            Some(ServerMessage::CharacterRefused(CharacterRefused { reason })) => Some(reason),
            _ => None,
        })
}

/// When (the clock) a watcher first received a world message `pick` accepts since `from`.
fn seen_at(h: &mut Harness, b: usize, from: usize, pick: impl Fn(&ServerMessage) -> bool) -> f64 {
    let give_up = h.now.0 + 60.0;
    let mut i = from;
    loop {
        while i < h.bots[b].inbox.len() {
            let r: &Received = &h.bots[b].inbox[i];
            if r.world && r.message().is_some_and(|m| pick(&m)) {
                return h.now.0;
            }
            i += 1;
        }
        assert!(
            h.now.0 < give_up,
            "not seen within a minute of the world's time"
        );
        h.step();
    }
}

/// The rest of a login begun with [`yalain_testkit::bot::Bot::log_in`]: the data query answered,
/// the character list in.
fn to_the_list(h: &mut Harness, b: usize) {
    assert!(h.run_until(400, |h| h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::DataQuery(_)))));
    let held = ClientMessage::DataHeld(auberean_protocol::login::DataHeld {
        language: 0x8000_0001,
        files: Vec::new(),
    });
    h.bots[b].send(false, queue::DATABASE, &held);
    assert!(h.run_until(400, |h| h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::CharacterList(_)))));
}

fn glows(id: u64) -> impl Fn(&ServerMessage) -> bool {
    move |m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == id && e.action.fx == ARRIVAL_FX)
}

fn behaves(id: u64) -> impl Fn(&ServerMessage) -> bool {
    move |m| matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == id && e.action.behavior == log_off::BEHAVIOR)
}

fn leaves(id: u64) -> impl Fn(&ServerMessage) -> bool {
    move |m| matches!(m, ServerMessage::ObjectLeaves(e) if e.sender.id == id)
}

/// A client killed in the world, then the account logged in again from a new client: at the new
/// login the character left in the world begins to log off as any logging off does (glowing at
/// once, its leaving behavior at 15.2 s, out at 19 s); entering it again is refused with 11 until
/// it has left, and then goes through.
/// Behaviour: server.login.logging-in-again-logs-off-the-character-left-in-the-world
#[test]
fn a_new_login_logs_off_the_character_a_killed_client_left_in_the_world() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (watcher, _) = h.player_in_world("two", "Torvald");
    killed(&mut h, a);
    h.run(100);
    let from = h.bots[watcher].inbox.len();
    let b = h.add_bot();
    let now = h.now;
    h.bots[b].log_in("one", b"ticket", now);
    let glow = seen_at(&mut h, watcher, from, glows(ida));
    assert!(glow - now.0 < 1.0, "logged off at the new login");
    to_the_list(&mut h, b);
    assert_eq!(
        ask_entry(&mut h, b, "one", ida),
        Some(11),
        "still in the world"
    );
    let behavior = seen_at(&mut h, watcher, from, behaves(ida));
    assert!(
        (behavior - glow - log_off::BEHAVIOR_AFTER).abs() < 0.3,
        "its leaving behavior at {:.2} s",
        behavior - glow
    );
    assert_eq!(
        ask_entry(&mut h, b, "one", ida),
        Some(11),
        "still in the world"
    );
    let gone = seen_at(&mut h, watcher, from, leaves(ida));
    assert!(
        (gone - glow - log_off::OUT_AFTER).abs() < 0.3,
        "out at {:.2} s",
        gone - glow
    );
    assert_eq!(ask_entry(&mut h, b, "one", ida), None, "entered again");
    assert!(h.run_until(400, |h| h.bots[b].in_world()));
}

/// A client that says goodbye with its character in the world leaves the character lingering,
/// glowing at once, its leaving behavior at 15.2 s, and gone just after it, without the trip into
/// portal space a logging off makes; its account's entries are refused until then. A client whose
/// world connection alone goes away leaves its character the same way, and its login is told the
/// character is out.
/// Behaviour: server.world.a-client-gone-leaves-its-character-lingering
#[test]
fn a_character_whose_client_said_goodbye_lingers_and_leaves_at_its_leaving_behavior() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (watcher, _) = h.player_in_world("two", "Torvald");
    let from = h.bots[watcher].inbox.len();
    h.bots[a].leave();
    let left = h.now.0;
    let glow = seen_at(&mut h, watcher, from, glows(ida));
    assert!(glow - left < 1.0, "glows at once");
    assert!(
        h.server.players.values().any(|p| p.character == ida),
        "still in the world"
    );
    let b = h.at_character_list("one");
    assert_eq!(ask_entry(&mut h, b, "one", ida), Some(11));
    let behavior = seen_at(&mut h, watcher, from, behaves(ida));
    let gone = seen_at(&mut h, watcher, from, leaves(ida));
    assert!(
        (behavior - glow - log_off::BEHAVIOR_AFTER).abs() < 0.3,
        "its leaving behavior at {:.2} s",
        behavior - glow
    );
    assert!(
        gone - behavior < 0.5,
        "gone {:.2} s after its leaving behavior",
        gone - behavior
    );
    let looks =
        |m: &ServerMessage| matches!(m, ServerMessage::UpdateVisualDesc(e) if e.sender.id == ida);
    assert!(
        !h.world_messages(watcher, from).iter().any(looks),
        "no trip into portal space, so no look given back"
    );
    assert_eq!(ask_entry(&mut h, b, "one", ida), None, "entered again");

    // A client whose world connection alone goes away: the same leaving, and its login is told
    // the character is out.
    let (c, idc) = h.player_in_world("three", "Wrenna");
    let from = h.bots[watcher].inbox.len();
    let login_from = h.bots[c].inbox.len();
    h.bots[c].leave_world();
    let glow = seen_at(&mut h, watcher, from, glows(idc));
    let behavior = seen_at(&mut h, watcher, from, behaves(idc));
    let gone = seen_at(&mut h, watcher, from, leaves(idc));
    assert!(
        (behavior - glow - log_off::BEHAVIOR_AFTER).abs() < 0.3 && gone - behavior < 0.5,
        "lingers to its leaving behavior, then gone"
    );
    assert!(
        h.run_until(400, |h| h.bots[c].inbox[login_from..].iter().any(|r| !r
            .world
            && matches!(r.message(), Some(ServerMessage::CharExitGame(_)))))
    );
}

/// An account has one character in the world at a time: while one is in the world, lingering
/// included, entering another of its characters is refused with 11; once it has left, the other
/// enters.
/// Behaviour: server.world.one-character-of-an-account-at-a-time
#[test]
fn a_second_character_of_the_account_waits_until_the_first_has_left() {
    let mut h = Harness::new();
    let a = h.at_character_list("one");
    let first = h.create_character(a, "one", "Elara");
    let second = h.create_character(a, "one", "Wrenna");
    h.enter_world(a, "one", first);
    let (watcher, _) = h.player_in_world("two", "Torvald");
    let from = h.bots[watcher].inbox.len();
    h.bots[a].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: first }),
    );
    let b = h.at_character_list("one");
    assert_eq!(ask_entry(&mut h, b, "one", second), Some(11));
    seen_at(&mut h, watcher, from, leaves(first));
    assert_eq!(ask_entry(&mut h, b, "one", second), None, "entered");
    assert!(h.run_until(400, |h| h.bots[b].in_world()));
}

/// Two sessions in a row: a client logs off, is told the character is out, and goes away without a
/// goodbye; a new client of the account enters the same character at once.
/// Behaviour: server.login.a-new-session-enters-after-a-log-off
#[test]
fn a_new_session_after_a_log_off_enters_the_character_at_once() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let before = h.bots[a].inbox.len();
    h.bots[a].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: ida }),
    );
    let out = |h: &Harness| {
        h.bots[a].inbox[before..]
            .iter()
            .any(|r| matches!(r.message(), Some(ServerMessage::CharExitGame(_))))
    };
    let asked = h.now.0;
    until(&mut h, asked, log_off::OUT_AFTER + 1.0);
    assert!(out(&h), "told it is out");
    killed(&mut h, a);
    let b = h.at_character_list("one");
    assert_eq!(ask_entry(&mut h, b, "one", ida), None, "entered");
    assert!(h.run_until(400, |h| h.bots[b].in_world()));
}

/// A log-off at the character list (the client sends one as it leaves its play screen after a
/// refused entry) is not answered: no exit, no lists.
/// Behaviour: server.login.log-off-at-the-list-unanswered
#[test]
fn a_log_off_at_the_character_list_is_not_answered() {
    let mut h = Harness::new();
    let b = h.at_character_list("one");
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: 0 }),
    );
    h.run(200);
    let answered: Vec<ServerMessage> = h.bots[b].inbox[before..]
        .iter()
        .filter_map(Received::message)
        .collect();
    assert!(answered.is_empty(), "answered with {answered:?}");
}
