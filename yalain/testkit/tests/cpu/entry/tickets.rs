//! The secret an account logs in with, in each form a client carries it: the September 2013
//! client's game login, our client's game login and the friends ticket.

use auberean_client_sdk::net::logon::{Logon, KEY_LEN};
use auberean_friends::Ticket;
use auberean_primitives::scramble;
use auberean_protocol::login::CharacterRefused;
use auberean_protocol::ServerMessage;
use yalain_store::MemoryStore;
use yalain_testkit::Harness;
use yalain_world::login::{add_account, admits};
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
    let b = h.add_bot();
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
        add_account(&mut store, &format!("probe{i}"), secret, 1).expect("made");
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
