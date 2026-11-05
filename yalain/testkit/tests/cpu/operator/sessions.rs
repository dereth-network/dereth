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

/// Accounts made beforehand, so that their logins make none.
fn accounts(h: &mut Harness, names: impl IntoIterator<Item = String>) {
    for name in names {
        yalain_world::login::add_account(&mut h.server.store, &name, b"ticket", 4, 1)
            .expect("made");
    }
}

/// Logins from these addresses, all at the same moment: how many of them are told the world has no
/// room, by the end. Each account its own, made beforehand.
fn together(h: &mut Harness, from: &[(Ipv4Addr, String)]) -> usize {
    accounts(h, from.iter().map(|(_, name)| name.clone()));
    let bots: Vec<usize> = from
        .iter()
        .map(|(ip, name)| {
            let b = h.add_bot_from(*ip);
            let now = h.now;
            h.bots[b].log_in(name, b"ticket", now);
            b
        })
        .collect();
    h.run(10);
    bots.iter()
        .filter(|b| {
            h.bots[**b].messages(false).iter().any(
                |m| matches!(m, ServerMessage::CharacterRefused(r) if r.reason == CharacterRefused::SERVER_FULL),
            )
        })
        .count()
}

/// Many logins at once from one address: at least what the limit allows go through, a household's
/// or a shared address's; what is past it is told the world has no room. Another address is not held back, the limit comes back with
/// time, and an address the operator exempts, or this machine's own, is not held to it.
/// Behaviour: server.accounts.secret-checks-bounded
#[test]
fn logins_past_the_secrets_an_address_may_have_checked_are_told_the_world_has_no_room() {
    let (burst, per_second) = yalain_world::login::ADDRESS_CHECKS;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let burst = burst as usize;
    let cfg = ServerConfig {
        secret_cost: 4,
        ..ServerConfig::default()
    };
    let mut h = world(cfg.clone());
    let names = |prefix: &str, ip: Ipv4Addr, n: usize| -> Vec<(Ipv4Addr, String)> {
        (0..n).map(|i| (ip, format!("{prefix}{i}"))).collect()
    };
    assert_eq!(together(&mut h, &names("few", HERE, burst)), 0, "a few");
    let over = together(&mut h, &names("many", HERE, burst));
    assert!(over >= burst - 2, "{over} of {burst} past the limit");
    assert_eq!(
        together(&mut h, &names("there", THERE, burst)),
        0,
        "another address"
    );
    h.advance(10.0 / per_second);
    assert_eq!(
        together(&mut h, &names("later", HERE, 10)),
        0,
        "it comes back with time"
    );

    let exempt = |mut h: Harness, ip: Ipv4Addr, what: &str| {
        let before = h.server.sessions.len();
        for round in 0..3 {
            let refused = together(&mut h, &names(&format!("r{round}x"), ip, burst));
            assert_eq!(refused, 0, "{what}");
        }
        let all = before + 3 * burst;
        assert!(
            h.run_until(STEPS, |h| h.server.sessions.len() == all),
            "{what}"
        );
    };
    exempt(h, Ipv4Addr::LOCALHOST, "this machine");
    let h = world(ServerConfig {
        unlimited_addresses: vec![IpAddr::V4(HERE)],
        ..cfg
    });
    exempt(h, HERE, "exempted by the operator");
}

/// Logins from these addresses, each its own, at the same moment.
fn all_at_once(h: &mut Harness, n: usize) -> Vec<usize> {
    accounts(h, (0..n).map(|i| format!("p{i}")));
    (0..n)
        .map(|i| {
            let [_, _, a, b] = u32::try_from(i).expect("few").to_be_bytes();
            let c = h.add_bot_from(Ipv4Addr::new(10, 7, a, b));
            let now = h.now;
            h.bots[c].log_in(&format!("p{i}"), b"ticket", now);
            c
        })
        .collect()
}

/// However many addresses log in at once, secrets are checked at a pace with the world's work
/// between them, nobody refused for waiting: each gets in as its turn comes. A higher cost goes
/// slower, for the same share of the world's time.
/// Behaviour: server.accounts.secret-checks-bounded
#[test]
fn logins_at_once_wait_their_turn_and_each_gets_in() {
    use yalain_world::login::{CHECKS_A_SECOND, SECRET_COST};
    let at_cost = |cost: u32, n: usize| -> f64 {
        let mut h = world(ServerConfig {
            secret_cost: cost,
            ..ServerConfig::default()
        });
        all_at_once(&mut h, n);
        assert!(h.run_until(STEPS, |h| !h.server.sessions.is_empty()));
        let start = h.now.0;
        let in_time = h.run_until(STEPS, |h| h.server.sessions.len() == n);
        assert!(in_time, "all {n} in: {}", h.server.sessions.len());
        h.now.0 - start
    };
    #[allow(clippy::cast_precision_loss)]
    let least = |n: usize, work: f64| (n as f64 - 2.0) * work / CHECKS_A_SECOND;
    let took = at_cost(SECRET_COST, 8);
    assert!(took >= least(8, 1.0), "{took} s at its pace");
    let took = at_cost(SECRET_COST + 1, 9);
    assert!(took >= least(9, 2.0), "{took} s, each counting for two");
}

/// A login that goes away while it waits its turn is not checked and holds no place in the world.
/// Behaviour: server.accounts.secret-checks-bounded
#[test]
fn a_login_gone_before_its_turn_is_not_checked() {
    let mut h = world(ServerConfig::default());
    let bots = all_at_once(&mut h, 3);
    assert!(h.run_until(STEPS, |h| !h.server.sessions.is_empty()));
    h.bots[bots[1]].leave();
    h.bots[bots[2]].leave();
    h.run(40);
    assert_eq!(h.server.sessions.len(), 1);
}

/// Past as many as may wait, a login is told the world has no room. An admin's account still gets
/// in, and goes first.
/// Behaviour: server.accounts.secret-checks-bounded
#[test]
fn logins_past_as_many_as_may_wait_are_told_it_has_no_room_but_an_admins_goes_first() {
    use yalain_world::login::MOST_LOGINS_WAITING;
    let mut h = world(ServerConfig {
        max_sessions: 4 * yalain_world::login::MAX_SESSIONS,
        ..ServerConfig::default()
    });
    let store = &mut h.server.store;
    let admin = yalain_world::login::add_account(store, "boss", b"ticket", 4, 1).expect("made");
    store.set_account_admin(admin.id, true).expect("set");
    let bots = all_at_once(&mut h, MOST_LOGINS_WAITING + 40);
    h.run(8);
    let refused_now = bots
        .iter()
        .filter(|b| {
            h.bots[**b].messages(false).iter().any(
                |m| matches!(m, ServerMessage::CharacterRefused(r) if r.reason == CharacterRefused::SERVER_FULL),
            )
        })
        .count();
    assert!(
        (30..=40).contains(&refused_now),
        "{refused_now} past the limit"
    );
    let waiting = h.server.sessions.len();
    let c = h.add_bot_from(THERE);
    let now = h.now;
    h.bots[c].log_in("boss", b"ticket", now);
    assert!(h.run_until(5, |h| h
        .server
        .sessions
        .values()
        .any(|x| x.account.id == admin.id)));
    assert!(
        h.server.sessions.len() < waiting + 8,
        "ahead of those waiting"
    );
}

/// An open world makes a bounded number of accounts for one address, then tells it the world has no
/// room and makes none, while its accounts already made still log in and another address still
/// makes one; the limit comes back with time. A listed world makes none anyway, and looks no further.
/// Behaviour: server.accounts.new-accounts-bounded
#[test]
fn new_names_past_what_an_address_may_make_are_told_the_world_has_no_room() {
    let (burst, per_second) = yalain_world::login::ADDRESS_NEW_ACCOUNTS;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let burst = burst as usize;
    let mut h = world(ServerConfig {
        secret_cost: 4,
        ..ServerConfig::default()
    });
    let made = |h: &mut Harness, name: &str| {
        h.server
            .store
            .account_by_name(name)
            .is_ok_and(|a| a.is_some())
    };
    for i in 0..burst {
        let name = format!("mine{i}");
        assert_ne!(log_in_from(&mut h, HERE, &name).1, refused(19), "{name}");
        assert!(made(&mut h, &name), "{name}");
    }
    assert_eq!(log_in_from(&mut h, HERE, "one more").1, refused(19));
    assert!(!made(&mut h, "one more"), "none made");
    assert_ne!(log_in_from(&mut h, HERE, "mine0").1, refused(19), "its own");
    assert_ne!(
        log_in_from(&mut h, THERE, "theirs").1,
        refused(19),
        "another"
    );
    h.advance(1.0 / per_second);
    assert_ne!(
        log_in_from(&mut h, HERE, "one more").1,
        refused(19),
        "later"
    );
    assert!(made(&mut h, "one more"));
}

/// However many addresses bring new names at once, the world makes a bounded number of accounts,
/// and an old name is not held back by it.
/// Behaviour: server.accounts.new-accounts-bounded
#[test]
fn new_names_past_what_the_world_may_make_are_refused_and_old_ones_still_get_in() {
    use yalain_world::admin::{ticket, Ticket};
    let (burst, _) = yalain_world::login::WORLD_NEW_ACCOUNTS;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let burst = burst as usize;
    let mut h = world(ServerConfig {
        secret_cost: 4,
        ..ServerConfig::default()
    });
    let unix = h.unix;
    let ticket_from = |h: &mut Harness, name: &str, i: usize| {
        let [_, _, a, b] = u32::try_from(i).expect("few").to_be_bytes();
        ticket(&mut h.server, name, b"s", &format!("10.7.{a}.{b}"), unix)
    };
    let old = ticket_from(&mut h, "old", 0);
    assert!(matches!(old, Ticket::Admitted(_)));
    let mut admitted = 0;
    for i in 1..burst + 100 {
        if matches!(
            ticket_from(&mut h, &format!("n{i}"), i),
            Ticket::Admitted(_)
        ) {
            admitted += 1;
        }
    }
    assert_eq!(admitted, burst - 1, "as many as it may make");
    assert!(
        h.server
            .store
            .account_by_name(&format!("n{}", burst + 50))
            .is_ok_and(|a| a.is_none()),
        "none past that"
    );
    assert!(matches!(
        ticket_from(&mut h, "old", 40_000),
        Ticket::Admitted(_)
    ));
}

/// A friends ticket counts among the secrets checked as a login does: past its address's limit even
/// its right secret is refused unchecked, and the next waits its turn.
/// Behaviour: server.accounts.secret-checks-bounded
#[test]
fn a_friends_ticket_past_the_secrets_an_address_may_have_checked_is_refused() {
    use yalain_world::admin::{ticket, Ticket};
    let mut h = world(ServerConfig {
        secret_cost: 4,
        ..ServerConfig::default()
    });
    let mut admitted = 0;
    let unix = h.unix;
    for _ in 0..200 {
        if matches!(
            ticket(&mut h.server, "one", b"its secret", "192.0.2.7", unix),
            Ticket::Admitted(_)
        ) {
            admitted += 1;
        }
    }
    let (burst, _) = yalain_world::login::ADDRESS_CHECKS;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let burst = burst as usize;
    assert_eq!(admitted, burst, "as many as its limit");
    assert!(
        !yalain_world::login::secret_check_due(&h.server),
        "the next waits its turn"
    );
    assert!(
        matches!(
            ticket(&mut h.server, "one", b"its secret", "198.51.100.9", unix),
            Ticket::Admitted(_)
        ),
        "another address"
    );
}
