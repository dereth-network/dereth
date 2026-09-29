//! ACE: Source/ACE.Server/Network/NetworkSession.cs::NetworkSession
//! Idle timeout, auth-timeout from handshake, explicit disconnect, reconnect from new address,
//! two clients per address, second login boots the first, repeated LoginRequest, speed-hack
//! report, dropping pass still counts the session.
//! Fixture: locally constructed packets and session state on a virtual clock.

use std::time::Duration;

use empyrean_net::{
    AccountSelect, ClockSnapshot, ClockSnapshotExt, Event, GameMessageGroup, OutboundMessage,
    PortKind, SessionState, SessionTerminationReason,
};

use crate::common::{
    client_addr, connect, connect_response_datagram, find_connect_request, harness,
    login_request_datagram, run_for, server, session_of, TICK,
};

fn disconnected(
    h: &empyrean_net::testing::Harness,
) -> Vec<(
    empyrean_net::SessionId,
    SessionTerminationReason,
    Option<String>,
)> {
    h.events
        .iter()
        .filter_map(|e| match e {
            Event::Disconnected {
                session,
                reason,
                account,
                ..
            } => Some((*session, *reason, account.clone())),
            _ => None,
        })
        .collect()
}

/// A client that goes silent is timed out 60 s (`DefaultSessionTimeout`) after its last
/// processed packet, and dropped 2 s after that.
#[test]
fn a_silent_client_times_out() {
    let mut h = harness();
    let c = h.add_client(client_addr(40, 50_000), "idle", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    let id = session_of(&h, c).expect("session");
    // Long enough for the client's 2-second ACKs to move the session to the long timeout.
    run_for(&mut h, Duration::from_secs(5));
    let silent_from = h.net.now.utc;
    let timeout_at = h.net.server.session(id).expect("live").network.timeout_tick;
    let lead = (timeout_at - silent_from).total_seconds();
    assert!(
        (58.0..=60.0).contains(&lead),
        "60 s after the last packet: {lead}"
    );
    let _gone = h.clients.remove(c);

    let mut dropped_at = None;
    h.run_until(Duration::from_secs(70), TICK, |h| {
        if disconnected(h).iter().any(|(s, _, _)| *s == id) {
            dropped_at = Some(h.net.now.utc);
            return true;
        }
        false
    });
    assert_eq!(
        disconnected(&h),
        vec![(
            id,
            SessionTerminationReason::NetworkTimeout,
            Some("idle".into())
        )]
    );
    let after = (dropped_at.expect("dropped") - timeout_at).total_seconds();
    assert!(
        (2.0..=2.02).contains(&after),
        "dropped 2 s after the timeout: {after}"
    );
    assert!(h.net.server.session(id).is_none());
}

/// Until the session processes a packet after the handshake it keeps the 15-second auth timeout
/// set by its `LoginRequest`: the `ConnectResponse` is handled by `NetworkManager`, not by the
/// session.
#[test]
fn a_session_silent_since_the_handshake_keeps_the_auth_timeout() {
    let mut h = harness();
    let c = h.add_client(client_addr(49, 50_000), "quiet", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    let id = session_of(&h, c).expect("session");
    let timeout_at = h.net.server.session(id).expect("live").network.timeout_tick;
    assert_eq!(
        timeout_at,
        ClockSnapshot::at_seconds(15.0).utc,
        "15 s after the LoginRequest at t = 0"
    );
    let _gone = h.clients.remove(c);
    h.run_until(Duration::from_secs(20), TICK, |h| {
        !disconnected(h).is_empty()
    });
    assert_eq!(
        disconnected(&h),
        vec![(
            id,
            SessionTerminationReason::NetworkTimeout,
            Some("quiet".into())
        )]
    );
}

/// The client's `Disconnect` ends the session at once (`PacketHeaderDisconnect`); it is dropped
/// after the 2-second termination window.
#[test]
fn an_explicit_disconnect_drops_the_session() {
    let mut h = harness();
    let c = h.add_client(client_addr(41, 50_000), "bye", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    let id = session_of(&h, c).expect("session");
    h.clients[c].log_off();
    h.step();
    let terminating_at = h.now_seconds();
    assert_eq!(
        h.net.server.session(id).expect("still there").core.state,
        SessionState::TerminationStarted
    );
    h.run_until(Duration::from_secs(5), TICK, |h| {
        !disconnected(h).is_empty()
    });
    assert_eq!(
        disconnected(&h),
        vec![(
            id,
            SessionTerminationReason::PacketHeaderDisconnect,
            Some("bye".into())
        )]
    );
    let t = h.now_seconds() - terminating_at;
    assert!(
        (2.0..=2.02).contains(&t),
        "dropped after the 2 s window: {t}"
    );
}

/// After a disconnect the same account logs in again from a new address and port; the new session
/// has a new handle, and the old handle reaches nothing.
#[test]
fn the_same_account_reconnects_from_a_new_address() {
    let mut h = harness();
    let c = h.add_client(client_addr(42, 50_000), "again", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    let old = session_of(&h, c).expect("session");
    h.clients[c].log_off();
    h.run_until(Duration::from_secs(5), TICK, |h| {
        !disconnected(h).is_empty()
    });

    let c2 = h.add_client(client_addr(43, 50_001), "again", "pw");
    assert!(connect(&mut h, c2, Duration::from_secs(5)));
    let new = session_of(&h, c2).expect("session");
    assert_ne!(new, old);
    assert_eq!(new.client_id, old.client_id, "the freed slot is reused");
    assert!(new.generation > old.generation);
    assert_eq!(
        h.net
            .server
            .session(new)
            .expect("live")
            .core
            .account
            .as_deref(),
        Some("again")
    );

    // A send through the stale handle goes nowhere.
    h.net.server.send(
        old,
        OutboundMessage {
            group: GameMessageGroup::UIQueue,
            data: vec![1, 2, 3, 4],
        },
    );
    assert!(h.net.server.session(old).is_none());
}

/// Two clients behind one address, on two ports, get two sessions: sessions are keyed by the full
/// source address.
#[test]
fn two_clients_behind_one_address_get_two_sessions() {
    let mut h = harness();
    let a = h.add_client(client_addr(44, 50_000), "nat1", "pw");
    let b = h.add_client(client_addr(44, 50_001), "nat2", "pw");
    assert!(connect(&mut h, a, Duration::from_secs(5)));
    assert!(connect(&mut h, b, Duration::from_secs(5)));
    let (sa, sb) = (session_of(&h, a).expect("a"), session_of(&h, b).expect("b"));
    assert_ne!(sa, sb);
    assert_eq!(h.net.server.get_unique_session_endpoint_count(), 1);
    assert_eq!(
        h.net
            .server
            .get_session_endpoint_total_by_address_count(client_addr(44, 0).ip()),
        2
    );
}

/// `account_login_boots_in_use` (true by default): a second login to a live account boots the
/// old session (`AccountLoggedIn`) and refuses the new one (`AccountInUse`).
#[test]
fn a_second_login_boots_the_first() {
    let mut h = harness();
    let c = h.add_client(client_addr(45, 50_000), "twice", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    let first = session_of(&h, c).expect("session");
    let c2 = h.add_client(client_addr(46, 50_000), "twice", "pw");
    h.run_until(Duration::from_secs(5), TICK, |h| disconnected(h).len() >= 2);
    let second = h.events.iter().find_map(|e| match e {
        Event::Disconnected {
            session,
            reason: SessionTerminationReason::AccountInUse,
            ..
        } => Some(*session),
        _ => None,
    });
    let d = disconnected(&h);
    assert!(
        d.contains(&(
            first,
            SessionTerminationReason::AccountLoggedIn,
            Some("twice".into())
        )),
        "{d:?}"
    );
    assert!(second.is_some(), "{d:?}");
    let _ = c2;
}

/// A repeated `LoginRequest` for the login in progress is the same login and is ignored (V268/V282,
/// retail's): the client re-sends it every 2 s until the `ConnectRequest` arrives, so a copy lands
/// whenever the answer takes longer. ACE aborted the login on it (a second `DoLogin` whose
/// `AccountSelectCallback` found the seeds already sent, or `NetworkManager` dropping the
/// answered session as a bad handshake). A copy before the world answers, one after the
/// `ConnectRequest` (between the server's one-second re-sends) and one after the handshake each
/// start nothing and draw nothing; the session keeps waiting for the `ConnectResponse`.
#[test]
fn a_repeated_login_request_does_not_abort_the_login() {
    let mut net = server(Default::default());
    let from = client_addr(47, 40_000);
    let mut now = ClockSnapshot::at_seconds(1.0);
    let login = login_request_datagram("dup", "pw", "1802");
    net.on_datagram(PortKind::C2S, from, &login, now);
    let Some(Event::LoginRequest { session, .. }) = net.events().next() else {
        panic!()
    };

    // Before the world answers.
    now = now.advanced(Duration::from_secs(2));
    net.on_datagram(PortKind::C2S, from, &login, now);
    assert_eq!(net.events().count(), 0, "the copy starts no second login");
    assert_eq!(net.poll(now).count(), 0, "the copy draws no answer");
    assert_eq!(
        net.account_select_callback(session, now),
        AccountSelect::Continue
    );
    net.accept_login(session, 1, "dup".into(), 1);
    let out: Vec<_> = net.poll(now).collect();
    assert!(find_connect_request(&out).is_some());

    // After the ConnectRequest, before the ConnectResponse, between the timer's re-sends: a copy
    // is not answered.
    for _ in 0..2 {
        now = now.advanced(Duration::from_millis(300));
        net.on_datagram(PortKind::C2S, from, &login, now);
        assert!(
            find_connect_request(&net.poll(now).collect::<Vec<_>>()).is_none(),
            "a copy draws no ConnectRequest"
        );
        assert_eq!(net.events().count(), 0, "a copy starts no second login");
    }
    let live = net.session(session).expect("still live");
    assert_eq!(live.core.state, SessionState::AuthConnectResponse);
    assert!(live.core.pending_termination.is_none());
    let cookie = live.network.connection_data.connection_cookie;
    assert_eq!(net.events().count(), 0);

    let p1 = std::net::SocketAddr::new(from.ip(), from.port() + 1);
    net.on_datagram(
        PortKind::S2C,
        p1,
        &connect_response_datagram(cookie, session.client_id),
        now,
    );
    assert!(
        matches!(net.events().next(), Some(Event::ConnectResponse { session: s }) if s == session)
    );
    assert_eq!(
        net.session(session).expect("live").core.state,
        SessionState::AuthConnected
    );

    // After the handshake: ignored, and the kept ConnectRequest (with its seeds) is gone.
    net.on_datagram(PortKind::C2S, from, &login, now);
    let out: Vec<_> = net.poll(now).collect();
    assert!(
        find_connect_request(&out).is_none(),
        "no ConnectRequest once connected"
    );
    assert_eq!(net.events().count(), 0);
    let live = net.session(session).expect("live");
    assert_eq!(live.core.state, SessionState::AuthConnected);
    assert!(live.core.pending_termination.is_none());
}

/// A `LoginRequest` for another account from the endpoint of a login in progress is not a copy:
/// it keeps ACE's handling (here, before the world answers, a second login whose callback finds
/// the seeds already sent). A new login from the same endpoint once the session is gone starts
/// afresh.
#[test]
fn another_login_from_the_same_endpoint_is_not_a_copy() {
    let mut net = server(Default::default());
    let from = client_addr(49, 40_000);
    let mut now = ClockSnapshot::at_seconds(1.0);
    net.on_datagram(
        PortKind::C2S,
        from,
        &login_request_datagram("first", "pw", "1802"),
        now,
    );
    net.on_datagram(
        PortKind::C2S,
        from,
        &login_request_datagram("second", "pw", "1802"),
        now,
    );
    let events: Vec<Event> = net.events().collect();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::LoginRequest { .. }))
            .count(),
        2
    );
    let Event::LoginRequest { session, .. } = events[0].clone() else {
        panic!()
    };
    assert_eq!(
        net.account_select_callback(session, now),
        AccountSelect::Continue
    );
    assert_eq!(
        net.account_select_callback(session, now),
        AccountSelect::Terminated
    );

    // Once the session is dropped, the same login from the same endpoint is a new session.
    for _ in 0..(3 * 100) {
        now = now.advanced(Duration::from_millis(10));
        let _ = net.poll(now).count();
    }
    let _ = net.events().count();
    assert!(net.session(session).is_none(), "dropped");
    net.on_datagram(
        PortKind::C2S,
        from,
        &login_request_datagram("first", "pw", "1802"),
        now,
    );
    let Some(Event::LoginRequest { session: again, .. }) = net.events().next() else {
        panic!("a new login")
    };
    assert_ne!(again, session);
    assert_eq!(
        net.account_select_callback(again, now),
        AccountSelect::Continue
    );
}

/// A client whose echo clock runs 50% fast is flagged as speed hacking once the drift has grown
/// ten times past 2 s; only while the world says a player is active.
#[test]
fn a_fast_client_clock_is_reported_as_a_speed_hack() {
    let mut h = harness();
    let c = h.add_client(client_addr(48, 50_000), "fast", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    let id = session_of(&h, c).expect("session");
    h.clients[c].clock_scale = 1.5;
    run_for(&mut h, Duration::from_secs(30));
    assert!(
        !h.events
            .iter()
            .any(|e| matches!(e, Event::SpeedHack { .. })),
        "no player, no check"
    );

    h.net.server.set_player_active(id, true);
    let flagged = h.run_until(Duration::from_secs(120), TICK, |h| {
        h.events
            .iter()
            .any(|e| matches!(e, Event::SpeedHack { .. }))
    });
    assert!(flagged);
    assert!(h.events.contains(&Event::SpeedHack { session: id }));
}

/// F15: ACE's `DoSessionWork` counts every session it walks, including one it drops in that pass,
/// and the world loop sleeps 1 ms (not 10) while that count is non-zero. `do_session_work` plus
/// `drain_outgoing` give a driver that count; `get_session_count` afterwards would say 0.
#[test]
fn the_pass_that_drops_a_session_still_counts_it() {
    let (mut net, session, _from, mut now) = crate::common::raw_connected();
    net.terminate(
        session,
        SessionTerminationReason::PacketHeaderDisconnect,
        None,
        String::new(),
        now,
    );
    let mut counts = Vec::new();
    for _ in 0..300 {
        counts.push(net.do_session_work(now));
        let _ = net.drain_outgoing().count();
        if net.get_session_count() == 0 {
            break;
        }
        now = now.advanced(TICK);
    }
    assert_eq!(
        net.get_session_count(),
        0,
        "the session is dropped within 3 s"
    );
    assert!(counts.len() > 1, "the termination takes more than one pass");
    assert!(
        counts.iter().all(|&n| n == 1),
        "every pass counts the session, the dropping one too: {counts:?}"
    );
    assert_eq!(net.drain_outgoing().count(), 0, "drained");
}

/// `MemoryNet::pump` returns the pass's session count (ACE's `DoSessionWork` result).
#[test]
fn memory_pump_returns_the_session_count_of_its_pass() {
    let mut h = harness();
    let c = h.add_client(client_addr(47, 50_000), "count", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    let id = session_of(&h, c).expect("session");
    assert_eq!(h.net.pump(), 1);
    h.clients[c].log_off();
    h.step();
    let mut last = None;
    for _ in 0..300 {
        h.net.advance(TICK);
        last = Some(h.net.pump());
        if h.net.server.session(id).is_none() {
            break;
        }
    }
    assert!(h.net.server.session(id).is_none(), "dropped");
    assert_eq!(last, Some(1), "the dropping pass counts it");
    assert_eq!(h.net.pump(), 0);
}
