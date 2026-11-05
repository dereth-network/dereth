//! The logins a world holds, as Empyrean limits its sessions, the refused login closed at once, and
//! the operator's message of the day. Fixture: clients at test addresses logging in with the
//! harness's ticket, on worlds whose limits each test sets.

use std::net::{IpAddr, Ipv4Addr};

use auberean_protocol::login::CharacterRefused;
use auberean_protocol::ServerMessage;
use yalain_store::{MemoryStore, Store as _};
use yalain_testkit::play::STEPS;
use yalain_testkit::Harness;
use yalain_world::{AccountPolicy, ServerConfig};

/// A world with these settings and the test bodies.
fn world(cfg: ServerConfig) -> Harness {
    Harness::with(cfg, MemoryStore::default(), yalain_testkit::test_rules())
}

/// A client at `ip` logs in to `account`: the first message it is answered with.
fn log_in_from(h: &mut Harness, ip: Ipv4Addr, account: &str) -> (usize, ServerMessage) {
    let b = h.add_bot_from(ip);
    let now = h.now;
    h.bots[b].log_in(account, b"ticket", now);
    assert!(h.run_until(STEPS, |h| !h.bots[b].inbox.is_empty()));
    let first = h.bots[b].inbox[0].message().expect("a message");
    (b, first)
}

const fn refused(reason: u32) -> ServerMessage {
    ServerMessage::CharacterRefused(CharacterRefused { reason })
}

const HERE: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 7);
const THERE: Ipv4Addr = Ipv4Addr::new(198, 51, 100, 9);

/// A world holding as many logins as its operator lets it refuses the next one, unchecked, with
/// the client's "server full" reason (19), and closes its connection with the refusal; once a
/// login leaves, a new one is admitted. Empyrean's limit, 128, is the default.
/// Behaviour: server.operator.sessions-capped-server-full
#[test]
fn a_login_past_the_logins_the_world_holds_is_told_the_server_is_full() {
    assert_eq!(ServerConfig::default().max_sessions, 128);
    assert_eq!(ServerConfig::default().max_sessions_per_address, None);
    let mut h = world(ServerConfig {
        max_sessions: 2,
        ..ServerConfig::default()
    });
    let (one, first) = log_in_from(&mut h, HERE, "one");
    assert_ne!(first, refused(19));
    let (_, second) = log_in_from(&mut h, THERE, "two");
    assert_ne!(second, refused(19));
    let (third, answer) = log_in_from(&mut h, Ipv4Addr::LOCALHOST, "three");
    assert_eq!(answer, refused(CharacterRefused::SERVER_FULL));
    assert!(
        h.bots[third].dropped,
        "its connection closed with the refusal"
    );
    assert!(
        h.server
            .store
            .account_by_name("three")
            .is_ok_and(|a| a.is_none()),
        "refused unchecked: no account made for it"
    );
    assert_eq!(h.server.sessions.len(), 2);
    h.bots[one].leave();
    assert!(h.run_until(STEPS, |h| h.server.sessions.len() == 1));
    let (_, after) = log_in_from(&mut h, Ipv4Addr::LOCALHOST, "three");
    assert_ne!(after, refused(19), "admitted once a login has left");
}

/// One address holds as many logins as the operator lets it; its next is told the server is full,
/// while another address is admitted, and an address the operator exempts holds any number.
/// Without a limit (the default, as Empyrean's) one address holds any number.
/// Behaviour: server.operator.sessions-capped-server-full
#[test]
fn an_address_holding_its_limit_of_logins_is_told_the_server_is_full() {
    let mut h = world(ServerConfig {
        max_sessions_per_address: Some(2),
        unlimited_addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        ..ServerConfig::default()
    });
    for account in ["one", "two"] {
        assert_ne!(log_in_from(&mut h, HERE, account).1, refused(19));
    }
    let (b, answer) = log_in_from(&mut h, HERE, "three");
    assert_eq!(answer, refused(19), "the address holds its two");
    assert!(h.bots[b].dropped);
    assert_ne!(log_in_from(&mut h, THERE, "three").1, refused(19));
    for i in 0..4 {
        assert_ne!(
            log_in_from(&mut h, Ipv4Addr::LOCALHOST, &format!("free{i}")).1,
            refused(19),
            "an exempt address"
        );
    }

    let mut open = world(ServerConfig::default());
    for i in 0..5 {
        assert_ne!(
            log_in_from(&mut open, HERE, &format!("p{i}")).1,
            refused(19),
            "no limit by default"
        );
    }
}

/// A refused login (its ticket wrong) is answered with the refusal and its connection closed in
/// the same moment, the refusal first, as the January 2017 login server ended the logins it
/// refused; nothing is left open to time out.
/// Behaviour: server.operator.refused-login-closed-at-once
#[test]
fn a_refused_login_is_closed_with_its_refusal_at_once() {
    let mut h = world(ServerConfig {
        accounts: AccountPolicy::Listed,
        ..ServerConfig::default()
    });
    let b = h.add_bot_from(HERE);
    let now = h.now;
    h.bots[b].log_in("nobody", b"ticket", now);
    assert!(h.run_until(STEPS, |h| !h.bots[b].inbox.is_empty()));
    assert_eq!(h.bots[b].inbox[0].message(), Some(refused(7)));
    assert_eq!(h.bots[b].inbox.len(), 1, "the refusal alone");
    assert!(h.bots[b].dropped, "the goodbye came with it");
    assert_eq!(h.net.connection_count(), 0, "nothing left open");
}

/// The operator's message of the day is shown to each player as its character enters the world,
/// in one notice (the 2017 servers' kind for system text), its lines in order; with none (the
/// default) nothing is shown.
/// Behaviour: server.operator.motd-at-entry
#[test]
fn the_message_of_the_day_is_shown_as_each_character_enters() {
    let notices = |h: &Harness, b: usize| -> Vec<String> {
        h.world_messages(b, 0)
            .into_iter()
            .filter_map(|m| match m {
                ServerMessage::Notice(n) if n.kind == 256 => {
                    n.text.literal.map(|l| l.text().to_string())
                }
                _ => None,
            })
            .collect()
    };
    let mut h = world(ServerConfig {
        motd: "Welcome to Dawnsong.\nBe kind.".into(),
        ..ServerConfig::default()
    });
    let (b, _) = h.player_in_world("one", "Elara");
    h.run(20);
    assert_eq!(notices(&h, b), ["Welcome to Dawnsong.\nBe kind.\n"]);

    let mut plain = world(ServerConfig::default());
    let (b, _) = plain.player_in_world("one", "Elara");
    plain.run(20);
    assert!(notices(&plain, b).is_empty(), "{:?}", notices(&plain, b));
}

/// The account "boss", an admin's, with the harness's ticket and no login held.
fn an_admins_account(h: &mut Harness) {
    let (b, _) = log_in_from(h, Ipv4Addr::LOCALHOST, "boss");
    let boss = h
        .server
        .store
        .account_by_name("boss")
        .expect("reads")
        .expect("made");
    h.server
        .store
        .set_account_admin(boss.id, true)
        .expect("set");
    h.bots[b].leave();
    assert!(h.run_until(STEPS, |h| h.server.sessions.is_empty()));
}

/// A full world still lets in an admin's account with its ticket, as the game's `/userlimit` says
/// ("new non-admin users will not be able to log into the world"). With the wrong ticket it is told
/// the world is full, as anyone is, so that a full world doesn't say which accounts are admins'.
/// One address's limit holds it back as it does anyone.
/// Behaviour: server.operator.sessions-cap-lets-admins-in
#[test]
fn an_admins_account_with_its_ticket_gets_into_a_full_world() {
    let mut h = world(ServerConfig {
        max_sessions: 1,
        ..ServerConfig::default()
    });
    an_admins_account(&mut h);
    let (_, first) = log_in_from(&mut h, HERE, "one");
    assert_ne!(first, refused(19));

    let (_, player) = log_in_from(&mut h, THERE, "two");
    assert_eq!(
        player,
        refused(CharacterRefused::SERVER_FULL),
        "a player is not"
    );

    let b = h.add_bot_from(THERE);
    let now = h.now;
    h.bots[b].log_in("boss", b"not its ticket", now);
    assert!(h.run_until(STEPS, |h| !h.bots[b].inbox.is_empty()));
    assert_eq!(
        h.bots[b].inbox[0].message(),
        Some(refused(CharacterRefused::SERVER_FULL)),
        "the wrong ticket"
    );

    let (admin, answer) = log_in_from(&mut h, THERE, "boss");
    assert_ne!(answer, refused(19), "an admin passes");
    assert!(!h.bots[admin].dropped);
    assert_eq!(h.server.sessions.len(), 2);
    let (_, next) = log_in_from(&mut h, THERE, "three");
    assert_eq!(
        next,
        refused(CharacterRefused::SERVER_FULL),
        "players still not"
    );
}

/// An admin's account gets no pass from the limit on one address.
/// Behaviour: server.operator.sessions-cap-lets-admins-in
#[test]
fn an_admins_account_is_held_back_by_its_address_limit() {
    let mut h = world(ServerConfig {
        max_sessions_per_address: Some(1),
        ..ServerConfig::default()
    });
    an_admins_account(&mut h);
    let (_, first) = log_in_from(&mut h, HERE, "one");
    assert_ne!(first, refused(19));
    let (_, answer) = log_in_from(&mut h, HERE, "boss");
    assert_eq!(answer, refused(CharacterRefused::SERVER_FULL));
}
