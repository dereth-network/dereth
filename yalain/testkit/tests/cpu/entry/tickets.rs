//! The secret an account logs in with, in each form a client carries it: the September 2013
//! client's game login, our client's game login and the friends ticket.

use std::net::Ipv4Addr;

use auberean_client_sdk::net::logon::{Logon, KEY_LEN};
use auberean_friends::Ticket;
use auberean_primitives::scramble;
use auberean_protocol::login::CharacterRefused;
use auberean_protocol::ServerMessage;
use yalain_store::MemoryStore;
use yalain_store::Store;
use yalain_testkit::Harness;
use yalain_world::login::{
    account_name_fits, add_account, admits, secret_is_current, ticket_matches, SECRET_COST,
};
use yalain_world::{AccountPolicy, ServerConfig};

use super::character_select::STEPS;

/// A string as the September 2013 client writes it into a message: a 16-bit count, the bytes
/// scrambled, then padding to four bytes from where `out` started. The padding is not zeroed (the
/// client leaves whatever its buffer held there), so a reader that looked at it would fail.
fn retail_string(out: &mut Vec<u8>, plain: &[u8]) {
    out.extend_from_slice(&u16::try_from(plain.len()).expect("short").to_le_bytes());
    out.extend_from_slice(&scramble::conceal(plain));
    while !out.len().is_multiple_of(4) {
        out.push(0xCD);
    }
}

/// The September 2013 client's game login for an account and the secret given as its ticket
/// option, field by field: the version, the length of the rest, a ticket login with flags 1 and a
/// time, the account, the session-key blob, and the ticket's counted blob holding the secret as a
/// string, no terminator.
fn retail_login(account: &str, secret: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    retail_string(&mut body, b"1802");
    let rest_at = body.len();
    body.extend_from_slice(&[0; 4]);
    body.extend_from_slice(&0x4000_0002u32.to_le_bytes());
    body.extend_from_slice(&1u32.to_le_bytes());
    body.extend_from_slice(&1_485_419_089u32.to_le_bytes());
    retail_string(&mut body, account.as_bytes());
    body.extend_from_slice(&76u32.to_le_bytes());
    body.extend_from_slice(&[1, 2, 0, 0, 1, 0x68, 0, 0, 0, 0xA4, 0, 0]);
    body.extend_from_slice(&[0x5A; 64]);
    let mut ticket = Vec::new();
    retail_string(&mut ticket, secret);
    body.extend_from_slice(&u32::try_from(ticket.len()).expect("short").to_le_bytes());
    body.extend_from_slice(&ticket);
    let rest = u32::try_from(body.len() - rest_at - 4).expect("short");
    body[rest_at..rest_at + 4].copy_from_slice(&rest.to_le_bytes());
    body
}

/// Our client's game login for an account and secret.
fn auberean_login(account: &str, secret: &[u8]) -> Vec<u8> {
    Logon {
        account: account.as_bytes().to_vec(),
        ticket: secret.to_vec(),
        timestamp: 1_485_419_089,
        session_key: [0x5A; KEY_LEN],
    }
    .body()
}

/// The friends ticket's account and secret, as the friends service hands them to the world.
fn friends_ticket(account: &str, secret: &str) -> Ticket {
    let text = Ticket::new(account, secret).encode().expect("a ticket");
    Ticket::decode(&text).expect("reads back")
}

/// Is a login body admitted: does the server answer it with anything but the refusal?
fn admitted(h: &mut Harness, body: Vec<u8>) -> bool {
    admitted_from(h, Ipv4Addr::LOCALHOST, body)
}

/// [`admitted`], for a client at this address.
fn admitted_from(h: &mut Harness, ip: Ipv4Addr, body: Vec<u8>) -> bool {
    let b = h.add_bot_from(ip);
    let now = h.now;
    h.bots[b].log_in_with(body, now);
    assert!(h.run_until(STEPS, |h| !h.bots[b].inbox.is_empty()));
    h.bots[b].inbox[0].message()
        != Some(ServerMessage::CharacterRefused(CharacterRefused {
            reason: 7,
        }))
}

/// The September 2013 client's game login, written field by field as it writes it for a secret
/// given on its command line (the secret scrambled as any string, unterminated, its padding left
/// unzeroed), is admitted to an account listed with that secret as plain text, at every length of
/// padding; a wrong secret written so is refused.
/// Behaviour: server.accounts.the-retail-login-presents-the-plain-secret
#[test]
fn the_retail_login_is_admitted_with_the_plain_secret_it_was_given() {
    let cfg = ServerConfig {
        accounts: AccountPolicy::Listed,
        ..ServerConfig::default()
    };
    let secrets: [&[u8]; 4] = [b"abc123", b"abcd", b"abcde", b"abcdefg"];
    let mut store = MemoryStore::default();
    for (i, secret) in secrets.iter().enumerate() {
        add_account(&mut store, &format!("probe{i}"), secret, SECRET_COST, 1).expect("made");
    }
    let mut h = Harness::with(cfg, store, yalain_testkit::test_rules());
    for (i, secret) in secrets.iter().enumerate() {
        let account = format!("probe{i}");
        assert!(
            admitted(&mut h, retail_login(&account, secret)),
            "{account}: its own secret"
        );
        assert!(
            !admitted(&mut h, retail_login(&account, b"abc124")),
            "{account}: another secret"
        );
    }
}

/// One account and one secret: on an open world, whichever form presents the secret first (the
/// friends ticket, or a game login), the September 2013 client's game login, our client's game
/// login and the friends ticket are each admitted after it; a wrong secret is refused in each form.
/// Behaviour: server.accounts.one-secret-in-every-form
#[test]
fn one_account_is_admitted_from_every_form_of_its_secret_and_a_wrong_one_from_none() {
    for first in ["friends", "retail", "auberean"] {
        let mut store = MemoryStore::default();
        if first == "friends" {
            let t = friends_ticket("probe", "abc123");
            assert!(
                admits(
                    AccountPolicy::Open,
                    SECRET_COST,
                    &mut store,
                    &t.account,
                    t.secret.as_bytes(),
                    1
                )
                .is_some(),
                "the friends ticket makes the account"
            );
        }
        let mut h = Harness::with(ServerConfig::default(), store, yalain_testkit::test_rules());
        match first {
            "retail" => assert!(admitted(&mut h, retail_login("probe", b"abc123"))),
            "auberean" => assert!(admitted(&mut h, auberean_login("probe", b"abc123"))),
            _ => {}
        }
        assert!(
            admitted(&mut h, retail_login("probe", b"abc123")),
            "{first} first: the retail login"
        );
        assert!(
            admitted(&mut h, auberean_login("probe", b"abc123")),
            "{first} first: our client's login"
        );
        let t = friends_ticket("probe", "abc123");
        let unix = h.unix;
        assert!(
            admits(
                AccountPolicy::Open,
                SECRET_COST,
                &mut h.server.store,
                &t.account,
                t.secret.as_bytes(),
                unix
            )
            .is_some(),
            "{first} first: the friends ticket"
        );
        assert!(
            !admitted(&mut h, retail_login("probe", b"abc124")),
            "{first} first: a wrong secret in the retail login"
        );
        assert!(
            !admitted(&mut h, auberean_login("probe", b"abc124")),
            "{first} first: a wrong secret in our client's login"
        );
        let t = friends_ticket("probe", "abc124");
        assert!(
            admits(
                AccountPolicy::Open,
                SECRET_COST,
                &mut h.server.store,
                &t.account,
                t.secret.as_bytes(),
                unix
            )
            .is_none(),
            "{first} first: a wrong secret in the friends ticket"
        );
    }
}

/// A secret as earlier versions kept it, for the account `elder` and the ticket `abc123`: a
/// 16-byte salt, then the SHA-256 of the salt and the ticket.
const OLDER_SECRET: &str = "f725a8f86c34c57d058da82ee60c94c8f13800f153f823f121cd9395e6bf6823\
                            ed9c604696dd34532dd6c93f02d96f62";

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn secret_of(store: &mut MemoryStore, name: &str) -> Vec<u8> {
    store
        .account_by_name(name)
        .expect("reads")
        .expect("an account")
        .secret
        .expect("a secret")
}

/// An account's secret is kept as a bcrypt hash at the world's cost, its salt its own (one ticket
/// kept for two accounts is kept as two different secrets), and never as the ticket: whether the
/// account is made at its first login on an open world or beforehand by the operator. A secret
/// in no form the server keeps (the ticket itself, kept by something else) matches nothing.
/// Behaviour: server.accounts.secrets-kept-as-salted-bcrypt
#[test]
fn an_accounts_secret_is_kept_as_a_salted_bcrypt_hash_and_never_as_its_ticket() {
    let mut store = MemoryStore::default();
    for name in ["first", "second"] {
        assert!(admits(AccountPolicy::Open, 5, &mut store, name, b"abc123", 1).is_some());
    }
    add_account(&mut store, "listed", b"abc123", 5, 1).expect("made");
    let kept: Vec<Vec<u8>> = ["first", "second", "listed"]
        .iter()
        .map(|n| secret_of(&mut store, n))
        .collect();
    for (name, secret) in ["first", "second", "listed"].iter().zip(&kept) {
        assert!(secret.starts_with(b"$2b$05$"), "{name}: bcrypt at cost 5");
        assert_eq!(secret.len(), 60, "{name}");
        assert!(secret_is_current(secret, 5), "{name}");
        assert!(ticket_matches(secret, b"abc123"), "{name}: its ticket");
        assert!(!ticket_matches(secret, b"abc124"), "{name}: another");
        assert!(
            !secret.windows(6).any(|w| w == b"abc123"),
            "{name}: the ticket is not in it"
        );
    }
    assert_ne!(kept[0], kept[1], "each its own salt");
    assert_ne!(kept[0], kept[2], "each its own salt");
    assert!(!ticket_matches(b"abc123", b"abc123"), "a plain secret");
    let plain = format!("$2b$05${}", "a".repeat(53));
    assert!(!ticket_matches(plain.as_bytes(), b"abc123"), "no hash");
}

/// A secret kept as earlier versions kept it (a salted SHA-256) admits its ticket once more and is
/// then kept as bcrypt at the world's cost, on an open world and on a world of listed accounts
/// alike, as Empyrean moves an older password hash to bcrypt at its next good login; a wrong
/// ticket is refused and leaves the secret as it was.
/// Behaviour: server.accounts.older-secrets-kept-again-as-bcrypt
#[test]
fn an_older_salted_sha256_secret_admits_its_ticket_and_is_kept_as_bcrypt_after() {
    for policy in [AccountPolicy::Open, AccountPolicy::Listed] {
        let mut store = MemoryStore::default();
        let older = unhex(OLDER_SECRET);
        store
            .create_account("elder", Some(older.clone()), 1)
            .expect("made");
        assert!(
            admits(policy, 5, &mut store, "elder", b"abc124", 2).is_none(),
            "{policy:?}: a wrong ticket"
        );
        assert_eq!(
            secret_of(&mut store, "elder"),
            older,
            "{policy:?}: untouched"
        );
        let a = admits(policy, 5, &mut store, "elder", b"abc123", 2);
        assert!(a.is_some(), "{policy:?}: its own ticket");
        let now = secret_of(&mut store, "elder");
        assert!(secret_is_current(&now, 5), "{policy:?}: kept as bcrypt");
        assert_eq!(a.and_then(|a| a.secret), Some(now.clone()), "{policy:?}");
        assert!(
            admits(policy, 5, &mut store, "elder", b"abc123", 3).is_some(),
            "{policy:?}: its ticket after"
        );
        assert!(
            admits(policy, 5, &mut store, "elder", b"abc124", 3).is_none(),
            "{policy:?}: a wrong ticket after"
        );
        assert_eq!(secret_of(&mut store, "elder"), now, "{policy:?}: kept once");
    }
}

/// A secret kept at a cost other than the world's is kept again at the world's cost when its
/// ticket logs in, whichever client's login carries it, as Empyrean rehashes a password whose work
/// factor differs from its setting; a wrong ticket is refused and leaves it as it was.
/// Behaviour: server.accounts.secrets-kept-again-at-the-worlds-cost
#[test]
fn a_secret_kept_at_another_cost_is_kept_again_at_the_worlds_cost_by_its_login() {
    for retail in [true, false] {
        let cfg = ServerConfig {
            accounts: AccountPolicy::Listed,
            secret_cost: 5,
            ..ServerConfig::default()
        };
        let mut store = MemoryStore::default();
        add_account(&mut store, "probe", b"abc123", 4, 1).expect("made");
        let before = secret_of(&mut store, "probe");
        let mut h = Harness::with(cfg, store, yalain_testkit::test_rules());
        let login = |secret: &[u8]| {
            if retail {
                retail_login("probe", secret)
            } else {
                auberean_login("probe", secret)
            }
        };
        assert!(!admitted(&mut h, login(b"abc124")), "a wrong ticket");
        assert_eq!(
            secret_of(&mut h.server.store, "probe"),
            before,
            "left at cost 4"
        );
        assert!(admitted(&mut h, login(b"abc123")), "its ticket");
        let after = secret_of(&mut h.server.store, "probe");
        assert!(after.starts_with(b"$2b$05$"), "kept again at cost 5");
        assert!(ticket_matches(&after, b"abc123"));
    }
}

/// Five logins refused from one address within a minute (a wrong secret, or an account that is
/// not there) have its logins refused unchecked for a minute after, its own secret too, with the
/// same answer as any refusal; another address is not held back, refusals older than a minute
/// are forgotten, and a minute on its own secret is admitted again from the first address.
/// Behaviour: server.accounts.refused-logins-hold-back-their-address
#[test]
fn five_refused_logins_from_one_address_hold_its_logins_back_for_a_minute() {
    let cfg = ServerConfig {
        accounts: AccountPolicy::Listed,
        ..ServerConfig::default()
    };
    let mut store = MemoryStore::default();
    add_account(&mut store, "probe", b"abc123", SECRET_COST, 1).expect("made");
    let mut h = Harness::with(cfg, store, yalain_testkit::test_rules());
    let here = Ipv4Addr::new(192, 0, 2, 7);
    let there = Ipv4Addr::new(198, 51, 100, 9);
    for _ in 0..4 {
        assert!(!admitted_from(
            &mut h,
            here,
            auberean_login("probe", b"abc124")
        ));
    }
    h.advance(61.0);
    assert!(!admitted_from(
        &mut h,
        here,
        auberean_login("probe", b"abc124")
    ));
    assert!(
        admitted_from(&mut h, here, auberean_login("probe", b"abc123")),
        "the four a minute before are forgotten"
    );
    for _ in 0..3 {
        assert!(!admitted_from(
            &mut h,
            here,
            auberean_login("probe", b"abc124")
        ));
    }
    assert!(!admitted_from(
        &mut h,
        here,
        auberean_login("nobody", b"abc123")
    ));
    assert!(
        !admitted_from(&mut h, here, auberean_login("probe", b"abc123")),
        "held back: its own secret refused"
    );
    assert!(
        !admitted_from(&mut h, here, retail_login("probe", b"abc123")),
        "held back: in the retail login too"
    );
    assert!(
        admitted_from(&mut h, there, auberean_login("probe", b"abc123")),
        "another address"
    );
    h.advance(59.0);
    assert!(
        !admitted_from(&mut h, here, auberean_login("probe", b"abc123")),
        "still held back"
    );
    h.advance(2.0);
    assert!(
        admitted_from(&mut h, here, auberean_login("probe", b"abc123")),
        "a minute on"
    );
}

/// An account name longer than 50 (in UTF-16 units) is refused, and no account is made for it on
/// an open world, as Empyrean refuses one; one of 50 is admitted. The operator cannot make one
/// either.
/// Behaviour: server.accounts.names-at-most-50
#[test]
fn an_account_name_longer_than_fifty_is_refused_and_makes_no_account() {
    let mut h = Harness::with(
        ServerConfig::default(),
        MemoryStore::default(),
        yalain_testkit::test_rules(),
    );
    let long = "a".repeat(51);
    let fits = "a".repeat(50);
    assert!(!admitted(&mut h, auberean_login(&long, b"abc123")), "51");
    assert_eq!(h.server.store.account_by_name(&long).expect("reads"), None);
    assert!(admitted(&mut h, auberean_login(&fits, b"abc123")), "50");
    assert!(
        !account_name_fits(&"\u{10400}".repeat(26)),
        "52 units in 26 letters"
    );
    assert!(account_name_fits(&"\u{10400}".repeat(25)), "50 units");
    let mut store = MemoryStore::default();
    assert!(add_account(&mut store, &long, b"abc123", SECRET_COST, 1).is_err());
    assert!(add_account(&mut store, &fits, b"abc123", SECRET_COST, 1).is_ok());
}
