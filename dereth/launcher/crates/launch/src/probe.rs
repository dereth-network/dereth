//! Asking a server whether it is up, without an account: the status ping, and the server-tracker
//! login.
//!
//! **The status ping** (`dereth_transport::status_ping`) is Empyrean's: a hello on the game port
//! is answered with a token, and an ask carrying the token with the world's live status (whether it
//! is open, how many are on, its era and systems, the software and its version, its name). Other
//! servers drop both datagrams unread, so silence means "ask the other way", not "down".
//!
//! **The server-tracker login** works everywhere.
//!
//! ACE, GDLE and Empyrean all answer one particular login, the account
//! `acservertracker:jj9h26hcsggc` sent with no password, and answer nothing else about it: ACE and
//! Empyrean send the first handshake reply and close the session, and GDLE sends a login failure.
//! Neither creates an account (there is no password to create one with) and neither logs it as a
//! failed login. Server trackers and launchers have used it for years.
//!
//! So any reply from the server's address means it is up, and silence means it is down or
//! unreachable. The reply carries no player count: a server says how many are on only after a real
//! login.
//!
//! This module builds the request and recognises a reply; sending and waiting are the caller's.

use dereth_transport::conn::{build_login_request, ConnectionAuthenticator, NetAuthType};
/// The status ping's datagrams, for a caller that answers them (a test's stand-in server).
pub use dereth_transport::status_ping;
use dereth_transport::status_ping::StatusReply;
use dereth_transport::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};

use crate::status::LiveStatus;

/// The account every server answers without a password.
pub const TRACKER_ACCOUNT: &str = "acservertracker:jj9h26hcsggc";

/// The login request to send: one whole datagram, unsequenced and in plain text, as the first
/// packet of any login is.
#[must_use]
pub fn request() -> Vec<u8> {
    let auth = ConnectionAuthenticator {
        auth_type: NetAuthType::Account,
        auth_flags: 0,
        connection_sequence_number: 0,
        account: TRACKER_ACCOUNT.to_owned(),
        account_to_logon_as: None,
        crypto_data: Vec::new(),
        extra_data: Vec::new(),
    };
    let mut packet = OutPacket::new(ProtoHeader::default());
    // A login request is well under any size limit, so neither step can refuse it.
    if packet
        .add_optional_header(PacketFlags::LOGIN_REQUEST, build_login_request(&auth))
        .is_err()
    {
        return Vec::new();
    }
    packet.serialize(None).unwrap_or_default()
}

/// Whether `datagram`, received from the server's address, is a reply: any well-formed packet is.
#[must_use]
pub fn is_reply(datagram: &[u8]) -> bool {
    ParsedPacket::parse(datagram).is_ok()
}

/// The status ping's first step: the hello.
#[must_use]
pub fn status_hello() -> Vec<u8> {
    status_ping::hello()
}

/// The second step, for the answer to the hello: the ask carrying its token. `None` when
/// `datagram` is not that answer.
#[must_use]
pub fn status_ask(datagram: &[u8]) -> Option<Vec<u8>> {
    status_ping::parse_token_reply(datagram).map(status_ping::ask)
}

/// The world's live status, from the answer to the ask. `None` when `datagram` is not that
/// answer.
#[must_use]
pub fn status_reply(datagram: &[u8]) -> Option<LiveStatus> {
    StatusReply::parse(datagram).map(|r| crate::status::from_status_reply(&r))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_transport::conn::LoginRequest;

    #[test]
    fn the_request_is_a_plain_login_for_the_tracker_account_with_no_password() {
        let bytes = request();
        let parsed = ParsedPacket::parse(&bytes).unwrap();
        assert_eq!(parsed.header.header.0, PacketFlags::LOGIN_REQUEST);
        assert!(parsed.checksum_ok(None));
        let body = &bytes[20..];
        let login = LoginRequest::parse(body).unwrap();
        assert_eq!(login.client_version, "1802");
        assert_eq!(login.auth.auth_type, NetAuthType::Account);
        assert_eq!(login.auth.account, TRACKER_ACCOUNT);
        assert!(login.auth.extra_data.is_empty(), "no password");
    }

    /// The two steps, against a stand-in for the server's side of them.
    #[test]
    fn the_status_ping_asks_with_the_token_it_was_given_and_reads_the_status() {
        use dereth_transport::status_ping::{Request, Token, WorldState};
        let token = Token {
            window: 7,
            mac: [3; 16],
        };
        assert_eq!(
            status_ping::parse_request(&status_hello()),
            Some(Request::Hello)
        );
        let ask = status_ask(&status_ping::token_reply(token)).expect("an ask");
        assert_eq!(status_ping::parse_request(&ask), Some(Request::Ask(token)));
        assert_eq!(status_ask(&request()), None, "a login is no token");
        let reply = StatusReply {
            format_version: 1,
            state: WorldState::Open,
            players: 2,
            era: "eor".into(),
            era_table_version: 1,
            era_features: vec![0xff, 0xff, 0x5f],
            software: "Empyrean".into(),
            software_version: "0.2.0".into(),
            world_name: "Loopback".into(),
        };
        let live = status_reply(&reply.encode()).expect("the status");
        assert_eq!(live.players, Some(2));
        assert_eq!(live.world_name.as_deref(), Some("Loopback"));
        assert_eq!(status_reply(&status_ping::token_reply(token)), None);
        // Neither step is a packet a server reads as a login.
        assert!(!is_reply(&status_hello()));
        assert!(!is_reply(&ask));
    }

    #[test]
    fn a_packet_is_a_reply_and_noise_is_not() {
        assert!(is_reply(&request()));
        assert!(!is_reply(b"hello"));
        assert!(!is_reply(&[]));
    }
}
