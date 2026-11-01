//! The game login's body.

use auberean_client_net::logon::{Logon, KEY_BLOB_HEADER, KEY_LEN};
use yalain_net::login::parse_login;

fn logon() -> Logon {
    Logon {
        account: b"tester".to_vec(),
        ticket: b"a-ticket".to_vec(),
        timestamp: 1_485_419_089,
        session_key: [0x5A; KEY_LEN],
    }
}

/// The login carries the September 2013 client's version and a ticket login with flags 1, the
/// account as typed, the session-key blob (its header, then 64 key bytes) and the ticket with
/// its 16-bit length, in the layout a server reads.
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
    // The version's bytes on the wire are scrambled, as every 2017 login's are.
    assert_eq!(body[2..6], [0x2B, 0x11, 0x87, 0xC6]);
}
