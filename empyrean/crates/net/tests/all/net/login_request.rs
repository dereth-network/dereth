//! Vectors: local login-request payloads, account records and connection refusal cases
//! ACE's login parse agrees with the retail layout; account-to-login-as only when flagged; any-
//! length passwords; over-long names, wrong client version, missing password, full server, per-
//! address limit and shutdown refuse; shutdown disconnects all.
//! Fixture: locally constructed packets and session state on a virtual clock.

use std::time::Duration;

use crate::common::OPCODE_CHARACTER_ERROR;
use dereth_transport::conn::LoginRequest;
use dereth_transport::conn::{
    build_login_request, ConnectionAuthenticator, NetAuthType as WireAuthType,
};
use empyrean_net::packets::packet_inbound_login_request::PacketInboundLoginRequest;
use empyrean_net::{
    AccountSelect, CharacterError, ClockSnapshot, ClockSnapshotExt, Event, NetAuthType, NetConfig,
    Outgoing, PortKind, ServerNet, SessionTerminationReason,
};

use crate::common::{client_addr, find_connect_request, login_request_datagram, parse_all, server};

/// The single-fragment messages in `out`: (queue, opcode, the dword after it).
fn messages(out: &[Outgoing]) -> Vec<(u16, u32, Option<u32>)> {
    parse_all(out)
        .iter()
        .flat_map(|p| p.fragments.clone())
        .filter(|f| f.header.num_frags == 1)
        .map(|f| {
            let d = &f.payload;
            let opcode = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
            let arg = d
                .get(4..8)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            (f.header.queue_id, opcode, arg)
        })
        .collect()
}

fn body(auth: &ConnectionAuthenticator) -> Vec<u8> {
    build_login_request(auth)
}

fn retail_auth(account: &str, password: &str) -> ConnectionAuthenticator {
    let mut auth = ConnectionAuthenticator::account_password(account, password);
    auth.connection_sequence_number = 0x1234;
    auth
}

/// ACE's `PacketInboundLoginRequest` and the retail-layout decoder (`dereth-transport`'s `LoginRequest`) agree on every
/// field of a retail login.
#[test]
fn ace_parse_agrees_with_the_retail_layout() {
    let b = body(&retail_auth("someone", "secret"));
    let ace = PacketInboundLoginRequest::new(&b).expect("ACE parse");
    let retail = LoginRequest::parse(&b).expect("retail decode");
    assert_eq!(ace.client_version, "1802");
    assert_eq!(ace.client_version, retail.client_version);
    assert_eq!(ace.account, retail.auth.account);
    assert_eq!(ace.net_auth_type, NetAuthType::AccountPassword);
    assert_eq!(retail.auth.auth_type, WireAuthType::AccountPassword);
    assert_eq!(ace.timestamp, retail.auth.connection_sequence_number);
    assert_eq!(ace.password.as_deref(), Some("secret"));
    let mut r = dereth_protocol::Reader::new(&retail.auth.extra_data);
    assert_eq!(r.astring().expect("archive string"), "secret");
}

/// V303: the account to log in as is read only when
/// `AuthFlags & 2` says it is there, as the client writes it. Without the flag (every retail
/// login) the request carries no such field and parses; with it (which the retail client never
/// sets) the field is read and the password after it still reads right, where ACE's parse slipped.
#[test]
fn account_to_login_as_is_read_only_when_the_flag_says_it_is_there() {
    let without = retail_auth("someone", "secret");
    assert_eq!(without.auth_flags & 2, 0);
    let got = PacketInboundLoginRequest::new(&body(&without)).expect("parse without the field");
    assert_eq!(
        (got.account.as_str(), got.password.as_deref()),
        ("someone", Some("secret"))
    );

    let mut with = retail_auth("someone", "secret");
    with.auth_flags = 2;
    with.account_to_logon_as = Some("admin".into());
    let b = body(&with);
    assert_eq!(
        LoginRequest::parse(&b)
            .expect("retail decode")
            .auth
            .account_to_logon_as
            .as_deref(),
        Some("admin")
    );
    let got = PacketInboundLoginRequest::new(&b).expect("parse with the field");
    assert_eq!(
        (got.account.as_str(), got.password.as_deref(), got.timestamp),
        ("someone", Some("secret"), 0x1234)
    );
}

/// V232: `ReadString32L` reads the client's compressed length, two bytes from
/// 128. ACE switched at 255, so a 130-character password came out with its second length byte
/// (0x82, U+FFFD) in front; now every length reads as sent.
#[test]
fn passwords_of_any_length_read_as_the_client_sends_them() {
    for n in [1, 127, 128, 130, 254, 255, 300] {
        let pw = "p".repeat(n);
        let got =
            PacketInboundLoginRequest::new(&body(&retail_auth("someone", &pw))).expect("parse");
        assert_eq!(got.password.as_deref(), Some(pw.as_str()), "{n} characters");
    }
}

fn login(
    net: &mut ServerNet,
    host: u8,
    port: u16,
    account: &str,
    version: &str,
    now: ClockSnapshot,
) -> Vec<Outgoing> {
    net.on_datagram(
        PortKind::C2S,
        client_addr(host, port),
        &login_request_datagram(account, "pw", version),
        now,
    );
    net.poll(now).collect()
}

/// An account name over 50 characters: `SendLoginRequestReject(AccountInvalid)` at once (a
/// ConnectRequest, then the error), and the session terminates; the world never hears of it.
#[test]
fn an_over_long_account_name_is_rejected_by_the_transport() {
    let mut net = server(NetConfig::default());
    let now = ClockSnapshot::at_seconds(5.0);
    let out = login(&mut net, 50, 40_000, &"a".repeat(51), "1802", now);
    assert!(
        find_connect_request(&out).is_some(),
        "the ConnectRequest comes first"
    );
    assert_eq!(
        messages(&out),
        vec![(
            9,
            OPCODE_CHARACTER_ERROR,
            Some(CharacterError::AccountInvalid as u32)
        )]
    );
    assert!(net.events().next().is_none());
    let s = net.sessions().next().expect("terminating");
    assert_eq!(
        s.core.pending_termination.as_ref().map(|p| p.reason),
        Some(SessionTerminationReason::AccountInformationInvalid)
    );
}

fn select(net: &mut ServerNet, now: ClockSnapshot) -> (empyrean_net::SessionId, AccountSelect) {
    let Some(Event::LoginRequest { session, .. }) = net.events().next() else {
        panic!("login event")
    };
    (session, net.account_select_callback(session, now))
}

/// A client version other than "1802": refused before any ConnectRequest.
///
/// V269: ACE sends its boot message in an encrypted packet
/// with sequence 0, which a client with no connection drops unread. A client with no connection
/// reads only an unencrypted, unsequenced packet, and only its optional headers, so the refusal
/// goes out as one such packet carrying a lone NetError header: "You do not have the current
/// version of the client installed." The client's own transport accepts it and reads the code.
#[test]
fn a_wrong_client_version_is_refused_in_the_form_a_client_reads_before_connecting() {
    use dereth_transport::conn::NetErrorCode;
    use dereth_transport::wire::{PacketFlags, ParsedPacket};

    let mut net = server(NetConfig::default());
    let now = ClockSnapshot::at_seconds(5.0);
    let _ = login(&mut net, 51, 40_000, "old", "1801", now);
    let (session, verdict) = select(&mut net, now);
    assert_eq!(verdict, AccountSelect::Terminated);
    let out: Vec<_> = net.poll(now).collect();
    assert!(find_connect_request(&out).is_none());
    assert_eq!(
        out.len(),
        1,
        "the refusal alone: nothing the client could not read"
    );
    assert_eq!(out[0].to, client_addr(51, 40_000));

    let p = ParsedPacket::parse(&out[0].bytes).expect("the client's parser accepts the refusal");
    assert_eq!(p.header.seq_id, 0);
    assert!(!p.header.header.is_encrypted());
    assert!(p.checksum_ok(None), "a plaintext checksum");
    assert_eq!(
        p.header.header.0,
        PacketFlags::NET_ERROR,
        "a lone NetError header"
    );
    assert!(p.fragments.is_empty());
    let code = p
        .optional
        .get(&PacketFlags::NET_ERROR)
        .and_then(|b| NetErrorCode::unpack(b));
    assert_eq!(code, Some(NetErrorCode::NetVersionMismatch));

    // The client's transport, with no connection yet, takes it.
    let mut client = dereth_client_net::Net::new(dereth_client_net::NetConfig::default());
    let from = std::net::SocketAddr::new(crate::common::server_ip(), 9000);
    assert_eq!(
        client.feed(&out[0].bytes, Some(from), dereth_primitives::LocalTime(1.0)),
        Ok(())
    );

    // Once only, however long the termination runs.
    let later: Vec<_> = net.poll(now.advanced(Duration::from_millis(500))).collect();
    assert!(later.is_empty(), "{} more datagrams", later.len());

    let reason = net
        .session(session)
        .and_then(|s| s.core.pending_termination.as_ref().map(|p| p.reason));
    assert_eq!(
        reason,
        Some(SessionTerminationReason::ClientVersionIncorrect)
    );
}

/// No password (`NetAuthType < AccountPassword`): the ConnectRequest, then `AccountInvalid`; the
/// server-tracker account gets its pong (`ServerCrash1`) instead.
#[test]
fn a_login_without_a_password_is_refused_and_the_tracker_gets_its_pong() {
    for (account, reason, error) in [
        (
            "nopass",
            SessionTerminationReason::NotAuthorizedNoPasswordOrGlsTicketIncludedInLoginReq,
            CharacterError::AccountInvalid,
        ),
        (
            "acservertracker:jj9h26hcsggc",
            SessionTerminationReason::PongSentClosingConnection,
            CharacterError::ServerCrash1,
        ),
    ] {
        let mut net = server(NetConfig::default());
        let now = ClockSnapshot::at_seconds(5.0);
        let mut auth = retail_auth(account, "");
        auth.account = account.to_string();
        auth.auth_type = WireAuthType::Account;
        auth.extra_data.clear();
        let mut p = dereth_transport::wire::OutPacket::new(Default::default());
        p.add_optional_header(
            dereth_transport::wire::PacketFlags::LOGIN_REQUEST,
            body(&auth),
        )
        .expect("section");
        net.on_datagram(
            PortKind::C2S,
            client_addr(52, 40_000),
            &p.serialize(None).expect("bytes"),
            now,
        );
        let (session, verdict) = select(&mut net, now);
        assert_eq!(verdict, AccountSelect::Terminated, "{account}");
        let out: Vec<_> = net.poll(now).collect();
        assert!(
            find_connect_request(&out).is_some(),
            "{account}: the ConnectRequest goes first"
        );
        assert_eq!(
            messages(&out),
            vec![(9, OPCODE_CHARACTER_ERROR, Some(error as u32))],
            "{account}"
        );
        let got = net
            .session(session)
            .and_then(|s| s.core.pending_termination.as_ref().map(|p| p.reason));
        assert_eq!(got, Some(reason), "{account}");
    }
}

/// A full server: the LoginRequest is answered by a throwaway session (client id one past the
/// map) with a ConnectRequest and `LogonServerFull`; no session is created.
#[test]
fn a_full_server_refuses_logins() {
    let mut net = server(NetConfig {
        maximum_allowed_sessions: 1,
        ..NetConfig::default()
    });
    let now = ClockSnapshot::at_seconds(5.0);
    let _ = login(&mut net, 53, 40_000, "first", "1802", now);
    let (session, verdict) = select(&mut net, now);
    assert_eq!(verdict, AccountSelect::Continue);
    net.accept_login(session, 1, "first".into(), 1);
    let _ = net.poll(now).count();

    let out = login(&mut net, 54, 40_000, "second", "1802", now);
    let (_, cr, _) = find_connect_request(&out).expect("ConnectRequest");
    assert_eq!(cr.net_id, 2, "sessionMap.Length + 1");
    assert_eq!(
        messages(&out),
        vec![(
            9,
            OPCODE_CHARACTER_ERROR,
            Some(CharacterError::LogonServerFull as u32)
        )]
    );
    assert_eq!(net.get_session_count(), 1);
    assert!(net.events().next().is_none());
}

/// `MaximumAllowedSessionsPerIPAddress`: a second session from the same address is refused unless
/// the address is on the unlimited list.
#[test]
fn the_per_address_limit_applies() {
    for (unlimited, expect_sessions) in [(false, 1), (true, 2)] {
        let ip = client_addr(55, 0).ip();
        let mut net = server(NetConfig {
            maximum_allowed_sessions_per_ip_address: 1,
            allow_unlimited_sessions_from_ip_addresses: if unlimited { vec![ip] } else { vec![] },
            ..NetConfig::default()
        });
        let now = ClockSnapshot::at_seconds(5.0);
        let _ = login(&mut net, 55, 40_000, "one", "1802", now);
        let out = login(&mut net, 55, 40_001, "two", "1802", now);
        assert_eq!(
            net.get_session_count(),
            expect_sessions,
            "unlimited = {unlimited}"
        );
        if !unlimited {
            assert_eq!(
                messages(&out),
                vec![(
                    9,
                    OPCODE_CHARACTER_ERROR,
                    Some(CharacterError::LogonServerFull as u32)
                )]
            );
        }
    }
}

/// Shutting down, or shutting down within two minutes: `ServerCrash1`. Further out: accepted.
#[test]
fn logins_are_refused_during_shutdown() {
    let now = ClockSnapshot::at_seconds(1000.0);
    for (in_progress, shutdown_in, refused) in [
        (true, None, true),
        (false, Some(Duration::from_secs(119)), true),
        (false, Some(Duration::from_secs(121)), false),
    ] {
        let mut net = server(NetConfig::default());
        net.shutdown_in_progress = in_progress;
        net.shutdown_time = shutdown_in.map(|d| now.advanced(d).utc);
        let out = login(&mut net, 56, 40_000, "late", "1802", now);
        if refused {
            assert_eq!(
                messages(&out),
                vec![(
                    9,
                    OPCODE_CHARACTER_ERROR,
                    Some(CharacterError::ServerCrash1 as u32)
                )]
            );
            assert_eq!(net.get_session_count(), 0);
        } else {
            assert_eq!(net.get_session_count(), 1);
        }
    }
}

/// `DisconnectAllSessionsForShutdown`: every session is told `ServerCrash1` and terminated.
///
/// These two are still waiting for their ConnectRequest, so (V269) each is told in the form a
/// client reads before connecting: a lone NetError, "Server has closed this connection".
#[test]
fn shutdown_disconnects_every_session() {
    use dereth_transport::conn::NetErrorCode;
    use dereth_transport::wire::PacketFlags;

    let mut net = server(NetConfig::default());
    let now = ClockSnapshot::at_seconds(5.0);
    let _ = login(&mut net, 57, 40_000, "a", "1802", now);
    let _ = login(&mut net, 58, 40_000, "b", "1802", now);
    let _ = net.events().count();
    net.disconnect_all_sessions_for_shutdown(now);
    let out: Vec<_> = net.poll(now).collect();
    let refusals: Vec<_> = parse_all(&out)
        .iter()
        .map(|p| {
            p.optional
                .get(&PacketFlags::NET_ERROR)
                .and_then(|b| NetErrorCode::unpack(b))
        })
        .collect();
    assert_eq!(
        refusals,
        vec![Some(NetErrorCode::ServerClosedConnection); 2]
    );
    let later = now.advanced(Duration::from_millis(2010));
    let _ = net.poll(later).count();
    let reasons: Vec<_> = net
        .events()
        .filter_map(|e| match e {
            Event::Disconnected { reason, .. } => Some(reason),
            _ => None,
        })
        .collect();
    assert_eq!(
        reasons,
        vec![SessionTerminationReason::ServerShuttingDown; 2]
    );
}
