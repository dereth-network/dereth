//! The connections under loss, and a held game clock.

use std::net::SocketAddr;

use auberean_protocol::login::DataHeld;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use dereth_transport::conn::ConnectSeeds;
use dereth_transport::wire::ParsedPacket;
use yalain_net::endpoint::{Endpoint, Role};
use yalain_net::{fixed_stream, LocalTime, NetConfig, ServerNet};
use yalain_testkit::Harness;

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
