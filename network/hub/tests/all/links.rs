//! The listener's own limits on a running hub: a link lives only as long as its key, a link
//! attempt takes from its address's allowance, one address cannot hold every connection, and a
//! world that stops taking its frames is dropped.
//! Fixture: the conformance fixture's hub, spoken to over loopback.

use std::io::Write;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use dereth_network_conformance::hub::{Link, Open};
use dereth_network_hub::server::{HubConfig, Running, MAX_CONNECTIONS_PER_ADDRESS};
use dereth_network_protocol::limits::{Limits, Rate};
use dereth_network_protocol::{Address, Message};

/// Spec: hub: rotating or revoking a world's key closes the link opened with the old one.
#[test]
fn a_live_link_closes_when_its_key_is_rotated_or_revoked() {
    let (running, worlds, _) = super::conformance::hub();
    let base = format!("ws://{}", running.local_addr());
    let (aa, ka) = &worlds[0];
    let (bb, kb) = &worlds[1];
    let mut la = Link::open(&base, aa, ka).expect("aa links");
    la.greet().expect("aa is welcomed");
    let mut lb = Link::open(&base, bb, kb).expect("bb links");
    lb.greet().expect("bb is welcomed");
    running.registry().revoke_key(aa).expect("revoked");
    let rotated = running.registry().rotate_key(bb).expect("rotated");
    assert!(la.closes_within(Duration::from_secs(5)), "revoked");
    assert!(lb.closes_within(Duration::from_secs(5)), "rotated");
    let mut again = Link::open(&base, bb, &rotated).expect("the new key links");
    again.greet().expect("welcomed");
    assert!(!again.closes_within(Duration::from_secs(2)));
    running.shutdown();
}

/// Spec: hub: a link attempt takes from its client address's request allowance.
#[test]
fn link_attempts_with_a_bad_key_are_limited_as_requests_are() {
    // An allowance of five a minute: twenty attempts outrun its refill however slowly they go.
    let (running, _, _) = super::conformance::hub_with(HubConfig {
        http_per_minute: 5,
        ..HubConfig::default()
    });
    let base = format!("ws://{}", running.local_addr());
    let bad = format!("drth_{}", "A".repeat(43));
    let (mut refused, mut limited) = (0, 0);
    for _ in 0..20 {
        match Link::open(&base, "aa", &bad) {
            Err(Open::Refused(429)) => limited += 1,
            Err(Open::Refused(401)) => refused += 1,
            other => panic!("{other:?}"),
        }
    }
    assert!(
        refused > 0,
        "within the allowance, a bad key is refused 401"
    );
    assert!(limited > 0, "past the allowance, an attempt is refused 429");
    running.shutdown();
}

/// Spec: hub: one address cannot hold more than its share of the hub's connections.
#[test]
fn one_address_cannot_hold_every_connection() {
    let (running, _, _) = super::conformance::hub();
    let mut held = Vec::new();
    for _ in 0..(MAX_CONNECTIONS_PER_ADDRESS + 10) {
        let mut s = TcpStream::connect(running.local_addr()).expect("connect");
        // A request head that never ends.
        let _ = s.write_all(b"GET /healthz HTTP/1.1\r\nHost: x\r\n");
        held.push(s);
    }
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(running.open_connections(), MAX_CONNECTIONS_PER_ADDRESS);
    drop(held);
    running.shutdown();
}

/// Limits that hold back nothing these tests send.
fn roomy() -> Limits {
    let big = Rate {
        burst: 1e9,
        per_sec: 1e9,
    };
    Limits {
        tells_per_character: big,
        messages_per_world: big,
        tells_per_recipient: big,
        ..Limits::DEFAULT
    }
}

/// `aa` links and then takes nothing more; `bb` tells `aa`'s characters as fast as it can, until
/// it is told `aa` is offline or `within` has passed. How long that took, if it came, and both
/// links as the worlds hold them (`aa`'s never read, never closed).
fn flood_a_world_that_stops_reading(
    running: &Running,
    worlds: &[(String, String)],
    within: Duration,
) -> (Option<Duration>, Link, Link) {
    let base = format!("ws://{}", running.local_addr());
    let (aa, ka) = &worlds[0];
    let (bb, kb) = &worlds[1];
    let mut la = Link::open(&base, aa, ka).expect("aa links");
    la.greet().expect("aa is welcomed");
    let mut lb = Link::open(&base, bb, kb).expect("bb links");
    lb.greet().expect("bb is welcomed");
    let (from, to) = (
        Address::new("Bob", bb).expect("an address"),
        Address::new("Ann", aa).expect("an address"),
    );
    let text = "x".repeat(1000);
    let start = Instant::now();
    let mut n = 0u64;
    while start.elapsed() < within {
        for _ in 0..200 {
            n += 1;
            let tell = Message::Tell {
                id: format!("t{n}"),
                from: from.clone(),
                to: to.clone(),
                text: text.clone(),
                friend: false,
            };
            if let Err(e) = lb.send(&tell) {
                eprintln!("bb's send failed: {e}");
                return (None, la, lb);
            }
        }
        while let Ok(Some(m)) = lb.recv(Duration::ZERO) {
            if matches!(&m, Message::World(w) if w.slug == *aa && !w.online) {
                return (Some(start.elapsed()), la, lb);
            }
        }
    }
    (None, la, lb)
}

/// Spec: hub: a world that stops taking its frames is dropped once a write to it has waited the
/// write timeout: the other worlds are told it is offline, and its connection is freed.
#[test]
fn a_world_that_stops_reading_is_dropped_after_the_write_timeout() {
    let (running, worlds, _) = super::conformance::hub_with(HubConfig {
        limits: roomy(),
        link_write_timeout: Duration::from_secs(1),
        // Only the write timeout can drop it.
        link_queue_bytes: usize::MAX / 2,
        ..HubConfig::default()
    });
    let (took, _aa, _bb) =
        flood_a_world_that_stops_reading(&running, &worlds, Duration::from_secs(60));
    assert!(took.is_some(), "the stalled world is never dropped");
    assert!(!running.linked().contains(&worlds[0].0));
    let until = Instant::now() + Duration::from_secs(5);
    while running.open_connections() > 1 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(running.open_connections(), 1, "only bb's link is left");
    running.shutdown();
}

/// Spec: hub: a world that takes its frames more slowly than they come is dropped once its waiting
/// frames reach the hub's bound, long before a write times out: the other worlds are told it is
/// offline, and its connection is freed.
#[test]
fn a_world_whose_frames_pile_up_is_dropped_at_the_bound() {
    let (running, worlds, _) = super::conformance::hub_with(HubConfig {
        limits: roomy(),
        link_write_timeout: Duration::from_secs(120),
        // More than a world reading as it should ever has waiting, so the link is dropped only
        // once its thread is held in a write.
        link_queue_bytes: 8 * 1024 * 1024,
        ..HubConfig::default()
    });
    let (took, _aa, _bb) =
        flood_a_world_that_stops_reading(&running, &worlds, Duration::from_secs(60));
    let took = took.expect("the world is dropped");
    assert!(took < Duration::from_secs(60), "{took:?}");
    assert!(!running.linked().contains(&worlds[0].0));
    let until = Instant::now() + Duration::from_secs(5);
    while running.open_connections() > 1 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        running.open_connections(),
        1,
        "the dropped link's thread and slot are freed at once"
    );
    running.shutdown();
}
