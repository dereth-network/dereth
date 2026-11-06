//! The game login's body.

use auberean_client_net::logon::{Logon, KEY_BLOB_HEADER, KEY_LEN};
use auberean_primitives::scramble;
use dereth_transport::conn::NetAuthType;
use yalain_net::login::parse_login;

fn logon() -> Logon {
    Logon {
        account: b"tester".to_vec(),
        ticket: b"a-ticket".to_vec(),
        timestamp: 1_485_419_089,
        session_key: [0x5A; KEY_LEN],
    }
}

/// With no ticket the login is an account login: the account and the session-key blob, nothing in
/// the ticket's place.
///
/// Behaviour: session.logon.no-ticket-is-an-account-login
#[test]
fn a_login_with_no_ticket_is_an_account_login() {
    let auth = Logon {
        ticket: Vec::new(),
        ..logon()
    }
    .authenticator();
    assert_eq!(auth.auth_type, NetAuthType::Account);
    assert!(auth.extra_data.is_empty());
    assert!(!auth.crypto_data.is_empty());
    assert_eq!(logon().authenticator().auth_type, NetAuthType::GlsTicket);
}

/// The login carries the September 2013 client's version and a ticket login with flags 1, the
/// account as typed, the session-key blob (its header, then 64 key bytes) and the ticket in its
/// counted blob as a string (16-bit count, scrambled bytes, padding to four), in the layout a
/// server reads.
///
/// Behaviour: session.logon.the-login-carries-version-ticket-account-and-key-blob
#[test]
fn the_login_carries_version_ticket_account_and_key_blob() {
    let body = logon().body();
    let read = parse_login(&body).expect("a server reads it");
    assert_eq!(read.version, "1802");
    assert_eq!(read.auth_type, 0x4000_0002);
    assert_eq!(read.auth_flags, 1);
    assert_eq!(read.timestamp, 1_485_419_089);
    assert_eq!(read.account, "tester");
    assert_eq!(read.session_key.len(), 12 + KEY_LEN);
    assert_eq!(read.session_key[..12], KEY_BLOB_HEADER);
    assert_eq!(read.ticket, b"a-ticket");
    let mut blob = vec![12, 0, 0, 0, 8, 0];
    blob.extend_from_slice(&scramble::conceal(b"a-ticket"));
    blob.extend_from_slice(&[0, 0]);
    assert_eq!(
        body[body.len() - 16..],
        blob,
        "the ticket's counted blob ends the body"
    );
    // The version's bytes on the wire are scrambled, as every 2017 login's are.
    assert_eq!(body[2..6], [0x2B, 0x11, 0x87, 0xC6]);
}
