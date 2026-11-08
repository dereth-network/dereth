//! The connections under loss, logins read together after a long pass, and a held game clock.

use std::net::SocketAddr;

use auberean_protocol::login::DataHeld;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use dereth_transport::conn::ConnectSeeds;
use dereth_transport::wire::ParsedPacket;
use yalain_net::endpoint::{Endpoint, Role};
use yalain_net::{fixed_stream, LocalTime, NetConfig, Port, ServerNet};
use yalain_testkit::{port_addr, Harness};

fn pair(now: LocalTime) -> (Endpoint, Endpoint) {
    let seeds = ConnectSeeds {
        server_to_client: 0x1234_5678,
        client_to_server: 0x9ABC_DEF0,
    };
    let client: SocketAddr = "127.0.0.1:50000".parse().expect("addr");
    let server: SocketAddr = "127.0.0.1:9000".parse().expect("addr");
    (
        Endpoint::new(Role::Server, 0xF1, 5, 5, seeds, client, 1.25e8, now),
        Endpoint::new(Role::Client, 5, 0xF1, 5, seeds, server, 0.0, now),
    )
}

/// With every fifth datagram lost both ways, a client still logs in and gets its list: lost
/// packets are asked for again and resent under their original keys.
/// Behaviour: server.net.lost-packets-resent
#[test]
fn a_login_completes_over_a_link_that_loses_every_fifth_datagram() {
    let mut h = Harness::new();
    h.wire.drop_every = 5;
    let b = h.add_bot();
    let now = h.now;
    h.bots[b].log_in("tester", b"ticket", now);
    assert!(h.run_until(4000, |h| h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::DataQuery(_)))));
    let held = ClientMessage::DataHeld(DataHeld {
        language: 0x8000_0001,
        files: Vec::new(),
    });
    h.bots[b].send(false, queue::DATABASE, &held);
    assert!(h.run_until(4000, |h| h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::CharacterList(_)))));
    assert!(h.wire.dropped > 0);
}

/// Clients that log in one after another from one address while the server is busy for three
/// seconds, as it is in its first step, are read together once it is free, their resends among them.
/// Each is sent a handshake the server keeps until it is answered, or none until there is room, and
/// every client logs in, none left answering a handshake the server has let go.
/// Behaviour: server.net.logins-read-together-all-get-in
#[test]
fn clients_that_log_in_while_the_server_is_busy_all_get_in() {
    const CLIENTS: usize = 30;
    const EVERY: f64 = 0.1;
    let mut h = Harness::new();
    for _ in 0..CLIENTS {
        h.add_bot();
    }
    let send = |h: &mut Harness| {
        let now = h.now;
        for b in &mut h.bots {
            for (to, bytes) in b.tick(now) {
                h.wire.send(b.addr, to, bytes);
            }
        }
    };
    // The first is read as the pass begins; its handshake leaves when the pass ends.
    let begun = h.now;
    h.bots[0].log_in("busy0", b"ticket", begun);
    send(&mut h);
    for d in h.wire.take() {
        h.net.on_datagram(Port::LOGIN_EVEN, d.from, &d.bytes, begun);
    }
    for i in 1..CLIENTS {
        h.now = LocalTime(h.now.0 + EVERY);
        let now = h.now;
        h.bots[i].log_in(&format!("busy{i}"), b"ticket", now);
        send(&mut h);
    }
    h.now = LocalTime(h.now.0 + EVERY);
    send(&mut h);
    h.net.tick(h.now);
    for d in h.net.take_datagrams() {
        h.wire.send(port_addr(d.from), d.to, d.bytes);
    }

    let all_in = |h: &Harness| {
        h.bots.iter().all(|b| {
            b.messages(false)
                .iter()
                .any(|m| matches!(m, ServerMessage::DataQuery(_)))
        })
    };
    let in_time = h.run_until(400, all_in);
    let out: Vec<usize> = (0..CLIENTS)
        .filter(|&i| {
            !h.bots[i]
                .messages(false)
                .iter()
                .any(|m| matches!(m, ServerMessage::DataQuery(_)))
        })
        .collect();
    assert!(in_time, "every client logs in; not: {out:?}");
    assert_eq!(h.net.connection_count(), CLIENTS);
    assert_eq!(
        h.net.handshakes_waiting(),
        0,
        "no handshake left unanswered"
    );
}

/// A server whose game clock is held reads the same time from then on, and every connection's
/// clock it sends says that time: the client's clock stands still too.
/// Behaviour: server.net.game-clock-held
#[test]
fn a_held_game_clock_stands_still_for_the_server_and_in_what_it_sends() {
    let world: std::net::SocketAddrV4 = "127.0.0.1:9002".parse().expect("addr");
    let mut net = ServerNet::new(
        NetConfig::retail_ids(world),
        fixed_stream(7),
        1.25e8,
        LocalTime(1.0),
    );
    assert!((net.game_time(LocalTime(11.0)) - (1.25e8 + 10.0)).abs() < 1e-6);
    net.hold_clock(LocalTime(11.0));
    assert!(net.clock_held());
    assert!((net.game_time(LocalTime(900.0)) - (1.25e8 + 10.0)).abs() < 1e-6);

    let mut now = LocalTime(1.0);
    let (mut server, mut client) = pair(now);
    server.hold_clock(LocalTime(3.0));
    let held = 1.25e8 + 2.0;
    let mut syncs = Vec::new();
    while now.0 < 70.0 {
        for dg in server.tick(now) {
            let p = ParsedPacket::parse(&dg).expect("the client reads what the server sends");
            if let Ok(inc) = client.feed(&p, dg.len(), now) {
                syncs.extend(inc.time_sync);
            }
        }
        for dg in client.tick(now) {
            let p = ParsedPacket::parse_from_client(&dg).expect("the server reads it");
            let _ = server.feed(&p, dg.len(), now);
        }
        now = LocalTime(now.0 + 0.5);
    }
    assert!(syncs.len() >= 3, "a clock every 20 s: {syncs:?}");
    assert!(
        syncs.iter().skip(1).all(|t| (t - held).abs() < 1e-6),
        "{syncs:?}"
    );
    assert!((client.game_time(now) - held).abs() < 1e-6);
}
