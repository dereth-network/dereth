//! ACE: Source/ACE.Server/Network/NetworkSession.cs::NetworkSession
//! Three-way handshake completes; first timesync waits a tick; ConnectRequest is ACE's and re-
//! sent every second until the ConnectResponse (stopping at auth timeout); retransmitted response
//! ignored; wrong cookie times out; lost ConnectRequest recovered.
//! Fixture: locally constructed packets and session state on a virtual clock.

use std::time::Duration;

use dereth_transport::wire::{PacketFlags, ParsedPacket};
use empyrean_net::testing::{ClientStatus, TestAccounts, TestClient};
use empyrean_net::{
    AccountSelect, ClockSnapshot, ClockSnapshotExt, Event, PortKind, SessionState,
    SessionTerminationReason,
};

use empyrean_net::driver::memory::{LinkModel, MemoryNet};

use crate::common::{
    client_addr, connect, connect_response_datagram, find_connect_request, harness,
    login_request_datagram, lossy_harness, server, server_ip, session_of, TICK,
};

/// End to end against the shared client transport over a link with 50 ms of one-way latency:
/// the client is connected after exactly four one-way trips (LoginRequest, ConnectRequest,
/// ConnectResponse, TimeSync), so the server adds no delay to any leg.
#[test]
fn handshake_completes_with_zero_added_delay() {
    let link = LinkModel {
        latency: Duration::from_millis(50),
        ..LinkModel::perfect()
    };
    let mut h = lossy_harness(link, link);
    let c = h.add_client(client_addr(1, 50_000), "alpha", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));

    let client = &h.clients[c];
    let close = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-9);
    assert!(
        close(client.connect_request_at, 0.1),
        "ConnectRequest one round trip after the LoginRequest: {:?}",
        client.connect_request_at
    );
    assert!(close(client.connect_response_sent_at, 0.1));
    assert!(
        close(client.connected_at, 0.2),
        "TimeSync one round trip after the ConnectResponse: {:?}",
        client.connected_at
    );
    assert_eq!(client.login_requests_sent, 1);
    assert_eq!(
        client.connect_responses_sent, 1,
        "no ConnectResponse had to be repeated"
    );

    let id = session_of(&h, c).expect("session");
    let s = h.net.server.session(id).expect("live");
    assert_eq!(s.core.state, SessionState::AuthConnected);
    assert_eq!(s.core.end_point_s2c, Some(client.addr));
    assert_eq!(s.core.account.as_deref(), Some("alpha"));
    assert_eq!(
        client.net_id(),
        id.client_id,
        "the ConnectRequest carries the session's ClientId"
    );
    assert!(h.events.contains(&Event::ConnectResponse { session: id }));
}

/// With zero latency the ConnectRequest still goes out at the instant of the LoginRequest; the
/// TimeSync goes one world tick later, because ACE's `UtcNow > nextResync` is strict and the
/// session was created at that same instant (on a real network the round trip separates them).
#[test]
fn with_zero_latency_the_first_timesync_waits_one_tick() {
    let mut h = harness();
    let c = h.add_client(client_addr(7, 50_000), "golf", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    let client = &h.clients[c];
    assert_eq!(client.connect_request_at, Some(0.0));
    assert_eq!(client.connect_response_sent_at, Some(0.0));
    assert_eq!(client.connected_at, Some(TICK.as_secs_f64()));
}

/// The ConnectRequest's layout: unsequenced, plaintext, from server id 0xB, iteration 1, carrying
/// the portal-year time, the cookie and the client id.
#[test]
fn the_connect_request_is_aces() {
    let mut net = server(Default::default());
    let from = client_addr(2, 40_000);
    let now = ClockSnapshot::at_seconds(1234.5);
    net.on_datagram(
        PortKind::C2S,
        from,
        &login_request_datagram("bravo", "pw", "1802"),
        now,
    );
    let Some(Event::LoginRequest {
        session, request, ..
    }) = net.events().next()
    else {
        panic!("login request event")
    };
    assert_eq!(request.account, "bravo");
    assert_eq!(request.password.as_deref(), Some("pw"));
    assert_eq!(request.client_version, "1802");
    assert_eq!(
        net.account_select_callback(session, now),
        AccountSelect::Continue
    );
    net.accept_login(session, 7, "bravo".into(), 1);
    let cookie = net
        .session(session)
        .expect("live")
        .network
        .connection_data
        .connection_cookie;
    let out: Vec<_> = net.poll(now).collect();
    let (header, cr, checksum_ok) = find_connect_request(&out).expect("ConnectRequest sent");
    assert!(checksum_ok, "plaintext checksum");
    assert_eq!(
        header.seq_id, 0,
        "UIntSequence(false): the first NextValue is 0"
    );
    assert_eq!(header.header.0, PacketFlags::CONNECT_REQUEST);
    assert_eq!(header.rec_id, 0xB, "NetworkManager.ServerId");
    assert_eq!(header.iteration, 1);
    assert_eq!(header.interval, 1234, "(ushort)PortalYearTicks");
    assert!((cr.server_time - 1234.5).abs() < f64::EPSILON);
    assert_eq!(cr.cookie, cookie);
    assert_eq!(cr.net_id, u32::from(session.client_id));
    assert!(
        out.iter()
            .all(|o| o.via_port_kind == PortKind::C2S && o.to == from),
        "before the ConnectResponse, P replies to P"
    );
    let s = net.session(session).expect("live");
    assert_eq!(s.core.state, SessionState::AuthConnectResponse);
    assert!(
        s.network.connection_data.server_seed.is_none(),
        "seeds discarded once sent"
    );
}

/// The client repeats its ConnectResponse until it hears from the server; the repeats find no
/// session in `AuthConnectResponse` and change nothing.
#[test]
fn a_retransmitted_connect_response_is_ignored() {
    let mut h = harness();
    let c = h.add_client(client_addr(3, 50_000), "charlie", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    let id = session_of(&h, c).expect("session");
    let cookie = h
        .net
        .server
        .session(id)
        .expect("live")
        .network
        .connection_data
        .connection_cookie;
    let before = h
        .events
        .iter()
        .filter(|e| matches!(e, Event::ConnectResponse { .. }))
        .count();

    let p1 = h.net.server_addr(PortKind::S2C);
    for _ in 0..3 {
        h.net.client_send(
            h.clients[c].addr,
            p1,
            connect_response_datagram(cookie, id.client_id),
        );
        h.step();
        h.net.advance(TICK);
    }
    let after = h
        .events
        .iter()
        .filter(|e| matches!(e, Event::ConnectResponse { .. }))
        .count();
    assert_eq!(before, 1);
    assert_eq!(after, 1, "no second handshake completion");
    assert_eq!(
        h.net.server.session(id).expect("live").core.state,
        SessionState::AuthConnected
    );
}

/// A ConnectResponse with the wrong cookie, or from another address, matches no session; the
/// session keeps waiting and times out on the 15-second auth timeout.
#[test]
fn a_wrong_cookie_is_ignored_and_the_session_times_out() {
    let mut net = server(Default::default());
    let from = client_addr(4, 40_000);
    let mut now = ClockSnapshot::at_seconds(10.0);
    net.on_datagram(
        PortKind::C2S,
        from,
        &login_request_datagram("delta", "pw", "1802"),
        now,
    );
    let Some(Event::LoginRequest { session, .. }) = net.events().next() else {
        panic!("login request event")
    };
    assert_eq!(
        net.account_select_callback(session, now),
        AccountSelect::Continue
    );
    net.accept_login(session, 1, "delta".into(), 1);
    let _ = net.poll(now).count();
    let cookie = net
        .session(session)
        .expect("live")
        .network
        .connection_data
        .connection_cookie;

    let p1_from = std::net::SocketAddr::new(from.ip(), 40_001);
    net.on_datagram(
        PortKind::S2C,
        p1_from,
        &connect_response_datagram(cookie ^ 1, session.client_id),
        now,
    );
    let other_host = client_addr(5, 40_001);
    net.on_datagram(
        PortKind::S2C,
        other_host,
        &connect_response_datagram(cookie, session.client_id),
        now,
    );
    assert_eq!(net.events().count(), 0, "neither completes the handshake");
    assert_eq!(
        net.session(session).expect("live").core.state,
        SessionState::AuthConnectResponse
    );

    // 15 s after the LoginRequest the auth timeout fires; 2 s later the session is dropped.
    let mut dropped = None;
    for _ in 0..(18 * 100) {
        now = now.advanced(Duration::from_millis(10));
        let _ = net.poll(now).count();
        if let Some(e) = net
            .events()
            .find(|e| matches!(e, Event::Disconnected { .. }))
        {
            dropped = Some((now.monotonic.as_secs_f64(), e));
            break;
        }
    }
    let (t, e) = dropped.expect("dropped");
    assert!(matches!(
        e,
        Event::Disconnected {
            reason: SessionTerminationReason::NetworkTimeout,
            ..
        }
    ));
    assert!(
        (t - 27.0).abs() < 0.02,
        "15 s auth timeout + 2 s termination window, got {t}"
    );
    assert!(net.session(session).is_none());
}

/// Every server packet of the handshake passes the shared client transport's checks: this is
/// what proves the ISAAC stream and sequencing agree with the client.
#[test]
fn the_client_transport_accepts_every_server_packet() {
    let mut h = harness();
    let c = h.add_client(client_addr(6, 50_000), "echo", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    assert_eq!(h.clients[c].status(), ClientStatus::Connected);
    assert_eq!(
        h.clients[c].rejected, 0,
        "the client transport accepted every server packet"
    );
}

/// Drives a server by hand to the point where its `ConnectRequest` has gone out at `start`.
fn answered_login(
    net: &mut empyrean_net::ServerNet,
    from: std::net::SocketAddr,
    start: ClockSnapshot,
) -> (empyrean_net::SessionId, Vec<u8>) {
    net.on_datagram(
        PortKind::C2S,
        from,
        &login_request_datagram("timer", "pw", "1802"),
        start,
    );
    let Some(Event::LoginRequest { session, .. }) = net.events().next() else {
        panic!("login request event")
    };
    assert_eq!(
        net.account_select_callback(session, start),
        AccountSelect::Continue
    );
    net.accept_login(session, 1, "timer".into(), 1);
    let out: Vec<_> = net.poll(start).collect();
    let first = out
        .iter()
        .find(|o| find_connect_request(std::slice::from_ref(o)).is_some())
        .expect("ConnectRequest sent")
        .bytes
        .clone();
    (session, first)
}

/// Polls every 10 ms from `from_ms` to `to_ms` after `start`; the times (seconds after
/// `start`) and bytes of every ConnectRequest sent.
fn connect_requests_between(
    net: &mut empyrean_net::ServerNet,
    start: ClockSnapshot,
    from_ms: u64,
    to_ms: u64,
) -> Vec<(f64, Vec<u8>)> {
    let mut seen = Vec::new();
    let mut ms = from_ms;
    while ms <= to_ms {
        let now = start.advanced(Duration::from_millis(ms));
        for o in net.poll(now).collect::<Vec<_>>() {
            if find_connect_request(std::slice::from_ref(&o)).is_some() {
                #[allow(clippy::cast_precision_loss)]
                seen.push((ms as f64 / 1000.0, o.bytes));
            }
        }
        ms += 10;
    }
    seen
}

/// The connection timer (V268/V282): after the `ConnectRequest` the server sends the same
/// bytes again every 1.0 s, with no help from the client, until the `ConnectResponse` arrives;
/// then it stops.
#[test]
fn the_connect_request_is_re_sent_every_second_until_the_connect_response() {
    let mut net = server(Default::default());
    let from = client_addr(9, 40_000);
    let start = ClockSnapshot::at_seconds(50.0);
    let (session, first) = answered_login(&mut net, from, start);

    let seen = connect_requests_between(&mut net, start, 10, 4_500);
    let times: Vec<f64> = seen.iter().map(|(t, _)| *t).collect();
    assert_eq!(times, vec![1.0, 2.0, 3.0, 4.0], "one re-send a second");
    assert!(
        seen.iter().all(|(_, b)| *b == first),
        "every re-send is the same bytes"
    );

    let cookie = net
        .session(session)
        .expect("live")
        .network
        .connection_data
        .connection_cookie;
    let p1 = std::net::SocketAddr::new(from.ip(), from.port() + 1);
    let at = start.advanced(Duration::from_millis(4_500));
    net.on_datagram(
        PortKind::S2C,
        p1,
        &connect_response_datagram(cookie, session.client_id),
        at,
    );
    assert!(
        matches!(net.events().next(), Some(Event::ConnectResponse { session: s }) if s == session)
    );
    let after = connect_requests_between(&mut net, start, 4_510, 8_000);
    assert!(
        after.is_empty(),
        "no re-send after the ConnectResponse: {:?}",
        after.iter().map(|(t, _)| t).collect::<Vec<_>>()
    );
}

/// With no `ConnectResponse` the re-sends run until the 15 s auth timeout ends the session, and
/// stop there.
#[test]
fn the_connect_request_re_sends_stop_at_the_auth_timeout() {
    let mut net = server(Default::default());
    let start = ClockSnapshot::at_seconds(50.0);
    let (session, _) = answered_login(&mut net, client_addr(10, 40_000), start);
    let seen = connect_requests_between(&mut net, start, 10, 20_000);
    let times: Vec<f64> = seen.iter().map(|(t, _)| *t).collect();
    let expected: Vec<f64> = (1..=14).map(f64::from).collect();
    assert_eq!(
        times, expected,
        "one a second until the auth timeout at 15 s"
    );
    assert!(net.events().any(|e| matches!(
        e,
        Event::Disconnected {
            reason: SessionTerminationReason::NetworkTimeout,
            ..
        }
    )));
    assert!(net.session(session).is_none());
}

/// When the ConnectRequest is lost, the server's own re-send a second later recovers it (V268/V282,
/// retail's), with no LoginRequest copy: the client connects about 1 s in, on the first session.
/// ACE removed the session as a bad handshake on the client's copy ("the client will start a new
/// session") and connected on the third LoginRequest, 4 s in; the same removal aborted a login
/// whose answer was only slow.
#[test]
fn a_lost_connect_request_is_recovered_by_the_server_timer() {
    let mut net = MemoryNet::new(server(Default::default()), server_ip());
    let server_c2s = net.server_addr(PortKind::C2S);
    let mut client = TestClient::new(client_addr(8, 50_000), server_c2s, "hotel", "pw");
    let mut accounts = TestAccounts::default();
    let mut events = Vec::new();
    let mut dropped = 0;
    while client.status() != ClientStatus::Connected && net.now.monotonic < Duration::from_secs(30)
    {
        let now = net.now.monotonic.as_secs_f64();
        client.tick(now);
        for (to, bytes) in client.take_outgoing() {
            net.client_send(client.addr, to, bytes);
        }
        net.pump();
        while let Some(d) = net.client_recv(client.addr) {
            let is_connect_request = ParsedPacket::parse(&d.bytes)
                .is_ok_and(|p| p.header.header.contains(PacketFlags::CONNECT_REQUEST));
            if is_connect_request && dropped == 0 {
                dropped += 1;
                continue;
            }
            client.handle_datagram(d.from, &d.bytes, now);
        }
        for e in net.server.events().collect::<Vec<_>>() {
            if let Event::LoginRequest {
                session, request, ..
            } = &e
            {
                let now = net.now;
                accounts.answer(&mut net.server, *session, request, now);
            }
            events.push(e);
        }
        net.advance(TICK);
    }
    assert_eq!(dropped, 1);
    assert_eq!(client.status(), ClientStatus::Connected);
    assert_eq!(
        client.login_requests_sent, 1,
        "no LoginRequest copy was needed"
    );
    let at = client.connect_request_at.expect("a ConnectRequest arrived");
    assert!(
        (at - 1.0).abs() < 0.025,
        "the ConnectRequest re-sent by the server 1 s later: {at}"
    );
    let logins = events
        .iter()
        .filter(|e| matches!(e, Event::LoginRequest { .. }))
        .count();
    assert_eq!(logins, 1, "the copy started no second login");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::Disconnected { .. })),
        "nothing was dropped: {events:?}"
    );
}
