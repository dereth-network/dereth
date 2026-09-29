//! Asking a server whether it is up, without an account: the server-tracker login.
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
use dereth_transport::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};

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

    #[test]
    fn a_packet_is_a_reply_and_noise_is_not() {
        assert!(is_reply(&request()));
        assert!(!is_reply(b"hello"));
        assert!(!is_reply(&[]));
    }
}
