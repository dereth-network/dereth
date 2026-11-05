//! The two ends of a connection, the game login body, and where a referral sends a client.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

mod cookies;
mod hub_challenge;
mod pace;
mod status_ping;
mod websocket;

use dereth_transport::conn::ConnectSeeds;
use dereth_transport::wire::ParsedPacket;
use yalain_net::endpoint::{Endpoint, Role, Stats};
use yalain_net::login::{build_login, parse_login, TICKET};
use yalain_net::referral::world_ip;
use yalain_net::LocalTime;

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

/// Run both ends on a virtual clock, losing every `drop_every`th datagram, until every message
/// has arrived or 600 virtual seconds pass.
fn exchange(drop_every: usize) -> (Stats, Stats) {
    let mut now = LocalTime(1.0);
    let (mut server, mut client) = pair(now);
    let to_client: Vec<Vec<u8>> = (0..40u8)
        .map(|i| vec![i; 4 + usize::from(i) * 37])
        .collect();
    let to_server: Vec<Vec<u8>> = (0..10u8).map(|i| vec![i; 44]).collect();
    for (i, m) in to_client.iter().enumerate() {
        server.send_blob(1, 0x23F1_0000_0000_0000 | i as u64, m.clone());
    }
    for (i, m) in to_server.iter().enumerate() {
        client.send_blob(3, 0x0300_0000_0000_0000 | i as u64, m.clone());
    }
    let (mut got_c, mut got_s) = (Vec::new(), Vec::new());
    let mut n = 0usize;
    while now.0 < 600.0 && (got_c.len() < to_client.len() || got_s.len() < to_server.len()) {
        for dg in server.tick(now) {
            n += 1;
            if n.is_multiple_of(drop_every) {
                continue;
            }
            let p = ParsedPacket::parse(&dg).expect("the client reads what the server sends");
            if let Ok(inc) = client.feed(&p, dg.len(), now) {
                got_c.extend(inc.blobs.into_iter().map(|b| b.payload));
            }
        }
        for dg in client.tick(now) {
            n += 1;
            if n.is_multiple_of(drop_every) {
                continue;
            }
            let p = ParsedPacket::parse_from_client(&dg)
                .expect("the server reads what the client sends");
            if let Ok(inc) = server.feed(&p, dg.len(), now) {
                got_s.extend(inc.blobs.into_iter().map(|b| b.payload));
            }
        }
        now = LocalTime(now.0 + 0.1);
    }
    got_c.sort();
    got_s.sort();
    assert_eq!(got_c, to_client, "every server message arrives intact");
    assert_eq!(got_s, to_server, "every client message arrives intact");
    (server.stats, client.stats)
}

/// Behaviour: server.net.lost-packets-resent
#[test]
fn lost_datagrams_are_asked_for_again_and_resent_under_their_original_key() {
    let (s, c) = exchange(7);
    assert!(c.naks_sent > 0, "the client asked for what it missed");
    assert!(s.retransmitted > 0, "and the server resent it");
    assert_eq!(
        s.rejected + c.rejected,
        0,
        "no checksum failed: a resend keeps its key"
    );
    let (s, c) = exchange(usize::MAX);
    assert_eq!(
        s.retransmitted + c.retransmitted + s.rejected + c.rejected,
        0
    );
}

/// The game login reads back to its version, authentication, account and ticket; its length
/// word counts exactly the bytes after it.
/// Behaviour: protocol.string.scrambled-on-the-wire
#[test]
fn a_game_login_reads_back_to_what_was_written() {
    let body = build_login("tester", b"a ticket", 1_485_419_089);
    let l = parse_login(&body).expect("reads");
    assert_eq!(
        (l.version.as_str(), l.auth_type, l.account.as_str()),
        ("1802", TICKET, "tester")
    );
    assert_eq!(l.ticket, b"a ticket");
    assert_eq!(l.session_key.len(), 76);
    assert_eq!(
        &body[..8],
        &[4, 0, 0x2B, 0x11, 0x87, 0xC6, 0, 0],
        "the version, scrambled and padded"
    );
    assert!(parse_login(&body[..body.len() - 1]).is_none());
}

/// A client close by is referred to an address on its own side, and anyone else to the address
/// named for players from elsewhere: services on one address refer a client close by to it; services
/// on every address to this machine's own address on the way to the client when that is on the
/// client's kind of network (loopback, a home network, a shared network), and otherwise to the
/// named address (a server in a container, behind the host's address translation); a client on
/// IPv6, or from the internet, to the named address, without a route asked for.
/// Behaviour: server.net.nearby-clients-referred-where-they-reached-the-world
#[test]
fn a_client_close_by_is_referred_to_an_address_on_its_side_and_anyone_else_to_the_named_one() {
    let ip = |s: &str| -> IpAddr { s.parse().expect("an address") };
    let v4 = |s: &str| -> Ipv4Addr { s.parse().expect("an address") };
    let named = v4("203.0.113.10");
    let every = ip("0.0.0.0");
    let unasked = |_: Ipv4Addr| -> Option<Ipv4Addr> { panic!("no route is needed") };

    // One address listened on: the only one a client close by can have reached.
    let one = ip("192.168.1.20");
    assert_eq!(
        world_ip(one, named, ip("192.168.1.30"), unasked),
        v4("192.168.1.20")
    );
    assert_eq!(
        world_ip(one, named, ip("10.8.0.5"), unasked),
        v4("192.168.1.20")
    );
    assert_eq!(world_ip(one, named, ip("198.51.100.7"), unasked), named);

    // Every address listened on: this machine's own address on the way to the client.
    let routes = |client: Ipv4Addr| -> Option<Ipv4Addr> {
        Some(match client.octets() {
            [127, ..] => v4("127.0.0.1"),
            [100, ..] => v4("100.101.7.8"),
            _ => v4("192.168.1.20"),
        })
    };
    assert_eq!(
        world_ip(every, named, ip("127.0.0.1"), routes),
        v4("127.0.0.1")
    );
    assert_eq!(
        world_ip(every, named, ip("::ffff:127.0.0.1"), routes),
        v4("127.0.0.1")
    );
    assert_eq!(
        world_ip(ip("::"), named, ip("::ffff:127.0.0.1"), routes),
        v4("127.0.0.1")
    );
    assert_eq!(
        world_ip(every, named, ip("192.168.1.30"), routes),
        v4("192.168.1.20")
    );
    assert_eq!(
        world_ip(every, named, ip("100.102.3.4"), routes),
        v4("100.101.7.8")
    );
    assert_eq!(world_ip(every, named, ip("169.254.9.9"), routes), named);
    assert_eq!(
        world_ip(every, named, ip("10.0.0.9"), |_| Some(v4("172.17.0.2"))),
        named
    );
    assert_eq!(
        world_ip(every, named, ip("127.0.0.1"), |_| None),
        v4("127.0.0.1")
    );
    assert_eq!(world_ip(every, named, ip("192.168.1.30"), |_| None), named);

    // From elsewhere, or on IPv6: the named address.
    assert_eq!(world_ip(every, named, ip("198.51.100.7"), unasked), named);
    assert_eq!(world_ip(every, named, ip("2001:db8::7"), unasked), named);
    assert_eq!(
        world_ip(ip("2001:db8::1"), named, ip("192.168.1.30"), unasked),
        named
    );
}
