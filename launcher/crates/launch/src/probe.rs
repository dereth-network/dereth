//! Asking a server whether it is up, without an account: the status ping, and the server-tracker
//! login.
//!
//! **The status ping** (`dereth_transport::status_ping`) is Empyrean's: a hello on the game port
//! is answered with a token, and an ask carrying the token with the world's live status (whether it
//! is open, how many are on, its era and systems, the software and its version, its name). Other
//! servers drop both datagrams unread, so silence means "ask the other way", not "down".
//!
//! **The server-tracker login** works on every server of the later wire.
//!
//! ACE, GDLE and Empyrean all answer one particular login, the account
//! `acservertracker:jj9h26hcsggc` sent with no password, and answer nothing else about it: ACE and
//! Empyrean send the first handshake reply and close the session, and GDLE sends a login failure.
//! Neither creates an account (there is no password to create one with) and neither logs it as a
//! failed login. Server trackers and launchers have used it for years.
//!
//! **A server of the November 1999 wire** looks for the same account, but where the 1999 client
//! writes its account ([`request_1999`]). The later form never reaches that check there: it is
//! refused as a client of the wrong version, with the packet in the server's log. So such a server
//! is asked in its own form, and answers with a login failure that the later wire can't read
//! ([`is_reply_1999`]).
//!
//! So any reply from the server's address means it is up, and silence means it is down or
//! unreachable. The reply carries no player count: a server says how many are on only after a real
//! login.
//!
//! This module builds the request and recognises a reply; sending and waiting are the caller's.

use dereth_transport::conn::{build_login_request, ConnectionAuthenticator, NetAuthType};
use dereth_transport::hash32;
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
        account: TRACKER_ACCOUNT.as_bytes().to_vec(),
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

/// The 1999 wire's packet header: sequence, flags, recipient, checksum, length, time and table.
const HEADER_1999: usize = 20;
/// The flag that says a 1999 packet's checksum is present.
const CHECKSUM_1999: u32 = 0x2;
/// What stands in the checksum field while a 1999 header is hashed.
const CHECKSUM_SEED_1999: u32 = 0x0329_1975;
/// A 1999 login's first word.
const LOGIN_MAGIC_1999: u32 = 0x075B_CD15;
/// The word that opens each account field of a 1999 login.
const ACCOUNT_MARKER_1999: u32 = 0xBEEF_BEEF;
/// How many characters a 1999 login's account field holds: the account is cut to fit, and only its
/// name, up to the colon, is what the server looks for.
const ACCOUNT_CHARS_1999: usize = 20;
/// The version a 1999 login names.
const CLIENT_VERSION_1999: &str = "4.35752";
/// The data file iterations a 1999 login reports: the retail CD's, as a client of that wire would. A
/// server of that wire logs the whole of a login that reports none.
const ITERATIONS_1999: (u32, u32) = (102, 67);

/// One account field of a 1999 login: the marker, the characters zero-padded, and a reserved word.
fn account_field_1999(account: &[u8]) -> [u8; 28] {
    let mut field = [0; 28];
    field[..4].copy_from_slice(&ACCOUNT_MARKER_1999.to_le_bytes());
    let n = account.len().min(ACCOUNT_CHARS_1999);
    field[4..4 + n].copy_from_slice(&account[..n]);
    field
}

/// The server-tracker login as the November 1999 client lays a login out, for a server of that wire:
/// one whole datagram, as the first packet of any login is.
#[must_use]
pub fn request_1999() -> Vec<u8> {
    let mut body = LOGIN_MAGIC_1999.to_le_bytes().to_vec();
    let version = CLIENT_VERSION_1999.as_bytes();
    let length = u16::try_from(version.len() + 1).unwrap_or(0);
    body.extend_from_slice(&length.to_le_bytes());
    body.extend_from_slice(version);
    body.push(0);
    body.resize(body.len().next_multiple_of(4), 0);
    body.extend_from_slice(&account_field_1999(TRACKER_ACCOUNT.as_bytes()));
    body.extend_from_slice(&account_field_1999(&[]));
    let (portal, cell) = ITERATIONS_1999;
    for word in [1, 0, portal, cell, 0, u32::MAX] {
        body.extend_from_slice(&word.to_le_bytes());
    }
    let length = u16::try_from(body.len()).unwrap_or(0);
    let mut packet = [0; HEADER_1999].to_vec();
    packet[..4].copy_from_slice(&1_u32.to_le_bytes());
    packet[4..8].copy_from_slice(&CHECKSUM_1999.to_le_bytes());
    packet[12..16].copy_from_slice(&CHECKSUM_SEED_1999.to_le_bytes());
    packet[16..18].copy_from_slice(&length.to_le_bytes());
    // The header's hash, and the body's without its length term.
    let checksum =
        hash32(&packet).wrapping_add(hash32(&body).wrapping_sub(u32::from(length) << 16));
    packet[12..16].copy_from_slice(&checksum.to_le_bytes());
    packet.extend_from_slice(&body);
    packet
}

/// Whether `datagram`, received from a server of the 1999 wire's address, is a reply: a whole packet
/// of that wire, with some flag set. A login failure doesn't set the checksum flag.
#[must_use]
pub fn is_reply_1999(datagram: &[u8]) -> bool {
    let Some(header) = datagram.get(..HEADER_1999) else {
        return false;
    };
    let flags = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    let length = usize::from(u16::from_le_bytes([header[16], header[17]]));
    flags != 0 && datagram.len() >= HEADER_1999 + length
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
        assert_eq!(login.client_version, b"1802");
        assert_eq!(login.auth.auth_type, NetAuthType::Account);
        assert_eq!(login.auth.account, TRACKER_ACCOUNT.as_bytes());
        assert!(login.auth.extra_data.is_empty(), "no password");
    }

    /// The 1999 form: the account where that wire's server looks for it, whole enough for its name to
    /// be found, the iterations it would otherwise log the packet for, and nothing else of anyone's.
    #[test]
    fn the_1999_request_carries_the_tracker_account_where_that_wire_reads_it() {
        let bytes = request_1999();
        assert_eq!(bytes.len(), HEADER_1999 + 96, "a 1999 login with no ticket");
        assert_eq!(&bytes[4..8], &CHECKSUM_1999.to_le_bytes());
        assert_eq!(&bytes[16..18], &96_u16.to_le_bytes());
        let body = &bytes[HEADER_1999..];
        assert_eq!(&body[..4], &LOGIN_MAGIC_1999.to_le_bytes());
        let marker = ACCOUNT_MARKER_1999.to_le_bytes();
        let first = body.windows(4).position(|w| w == marker).unwrap();
        let account = &body[first + 4..first + 4 + ACCOUNT_CHARS_1999];
        assert_eq!(account, b"acservertracker:jj9h");
        let second = first + 28;
        assert_eq!(&body[second..second + 4], &marker);
        assert!(body[second + 4..second + 24].iter().all(|b| *b == 0));
        let word = |at: usize| u32::from_le_bytes(body[at..at + 4].try_into().unwrap());
        assert_eq!((word(second + 36), word(second + 40)), ITERATIONS_1999);
    }

    /// The checksum is the header's hash with the seed in the field, and the body's less its length.
    #[test]
    fn the_1999_request_carries_its_checksum() {
        let mut bytes = request_1999();
        let said = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        bytes[12..16].copy_from_slice(&CHECKSUM_SEED_1999.to_le_bytes());
        let (header, body) = bytes.split_at(HEADER_1999);
        let length = u32::try_from(body.len()).unwrap();
        assert_eq!(
            said,
            hash32(header).wrapping_add(hash32(body).wrapping_sub(length << 16))
        );
    }

    /// A login failure of the 1999 wire, as a server of it sends one, is a reply; part of one isn't,
    /// and nor are bytes with no flag set.
    #[test]
    fn a_1999_reply_is_a_whole_packet_of_that_wire() {
        let mut failure = [0_u8; HEADER_1999 + 8];
        failure[4..8].copy_from_slice(&0x10_0000_u32.to_le_bytes());
        failure[8..12].copy_from_slice(&0xB_u32.to_le_bytes());
        failure[16..18].copy_from_slice(&8_u16.to_le_bytes());
        assert!(is_reply_1999(&failure));
        assert!(!is_reply_1999(&failure[..HEADER_1999 + 7]), "cut short");
        assert!(!is_reply_1999(&failure[..10]), "no header");
        failure[4..8].copy_from_slice(&0_u32.to_le_bytes());
        assert!(!is_reply_1999(&failure), "no flag");
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

    /// The hub asks a world with its own copy of both probes: the same tracker login and
    /// status-ping hello, byte for byte, the same ask for a token, and the same reading of a
    /// world's status, its era and systems included.
    #[test]
    fn the_hubs_probes_are_the_launchers() {
        use dereth_network_protocol::probe as hub;
        use dereth_transport::status_ping::{StatusReply, Token, WorldState};
        assert_eq!(hub::TRACKER_LOGIN.as_slice(), request().as_slice());
        assert_eq!(hub::status_hello(), status_hello());
        let token = Token {
            window: 9,
            mac: [5; 16],
        };
        let reply = status_ping::token_reply(token);
        let hub_token = hub::status_token(&reply).expect("the hub reads the token");
        assert_eq!(Some(hub::status_ask(&hub_token)), status_ask(&reply));
        assert_eq!(hub::status_token_reply(&hub_token), reply);
        let status = StatusReply {
            format_version: 1,
            state: WorldState::Open,
            players: 4,
            era: "eor".into(),
            era_table_version: 1,
            era_features: vec![0xff, 0x0f],
            software: "Empyrean".into(),
            software_version: "0.3.3".into(),
            world_name: "Loopback".into(),
        };
        let read = hub::Status::parse(&status.encode()).expect("the hub reads the status");
        assert_eq!((read.state, read.players), (1, 4));
        assert_eq!(
            (read.software.as_str(), read.version.as_str()),
            ("Empyrean", "0.3.3")
        );
        assert_eq!(read.name, "Loopback");
        assert_eq!(
            (read.era.as_str(), read.table, read.systems.as_slice()),
            ("eor", 1, [0xff, 0x0f].as_slice())
        );
        let stand_in = hub::Status {
            state: 1,
            players: 2,
            era: "infiltration".into(),
            table: 1,
            systems: vec![0x00, 0x00, 0xff],
            software: "Empyrean".into(),
            version: "1.0.0".into(),
            name: "Stand-in".into(),
        };
        let live = status_reply(&stand_in.encode()).expect("a stand-in's answer is a status");
        assert_eq!(live.world_name.as_deref(), Some("Stand-in"));
        assert_eq!(live.era.as_deref(), Some("infiltration"));
        assert!(live.era_features.is_some(), "its systems are read");
    }

    #[test]
    fn a_packet_is_a_reply_and_noise_is_not() {
        assert!(is_reply(&request()));
        assert!(!is_reply(b"hello"));
        assert!(!is_reply(&[]));
    }
}
