//! The interval a server's packets carry in their headers: a connection counts from 1, two to a
//! second, from when its handshake was answered.

use std::net::{SocketAddr, SocketAddrV4};

use dereth_transport::conn::ConnectRequest;
use dereth_transport::wire::{PacketFlags, ParsedPacket};
use yalain_net::endpoint::standalone;
use yalain_net::login::build_login;
use yalain_net::server::{ConnId, Datagram, NetConfig, NetEvent, Port, ServerNet};
use yalain_net::{fixed_stream, BlobKind, LocalTime};

const HANDSHAKE: f64 = 10.0;

fn at(seconds_after_the_handshake: f64) -> LocalTime {
    LocalTime(HANDSHAKE + seconds_after_the_handshake)
}

fn request(flag: u32, body: Vec<u8>) -> Vec<u8> {
    standalone(0, 0, flag, body)
}

/// The packets sent to `to`, parsed.
fn sent_to(net: &mut ServerNet, to: SocketAddr) -> Vec<ParsedPacket> {
    net.take_datagrams()
        .into_iter()
        .filter(|d: &Datagram| d.to == to)
        .map(|d| ParsedPacket::parse(&d.bytes).expect("a packet"))
        .collect()
}

fn the_cookie(p: &ParsedPacket) -> u64 {
    ConnectRequest::from_bytes(&p.optional[&PacketFlags::CONNECT_REQUEST])
        .expect("a handshake")
        .cookie
}

fn intervals(packets: &[ParsedPacket]) -> Vec<u16> {
    packets.iter().map(|p| p.header.interval).collect()
}

fn joined(net: &mut ServerNet) -> Option<ConnId> {
    std::iter::from_fn(|| net.poll_event()).find_map(|e| match e {
        NetEvent::LoggedIn { conn, .. } | NetEvent::WorldJoined { conn, .. } => Some(conn),
        _ => None,
    })
}

/// A handshake carries interval 0; the first data a login is sent, a round trip later, carries 2;
/// ten seconds and a tenth after the handshake the count is 22, not 21 as it would be counted from
/// the answer. A world's server switch, sent as its answer arrives, carries 1 and its first data 2.
/// Behaviour: server.net.interval-counts-from-the-handshake
#[test]
fn a_connection_counts_its_intervals_from_one_and_from_its_handshake() {
    let world: SocketAddrV4 = "203.0.113.10:9002".parse().expect("an address");
    let mut net = ServerNet::new(NetConfig::retail_ids(world), fixed_stream(7), 0.0, at(0.0));
    let player: SocketAddr = "198.51.100.7:50000".parse().expect("an address");

    net.on_datagram(
        Port::LOGIN_EVEN,
        player,
        &request(
            PacketFlags::LOGIN_REQUEST,
            build_login("tester", b"a ticket", 1),
        ),
        at(0.0),
    );
    let handshake = sent_to(&mut net, player);
    assert_eq!(intervals(&handshake), [0], "the login's handshake");
    let cookie = the_cookie(&handshake[0]);
    net.on_datagram(
        Port::LOGIN_ODD,
        player,
        &request(PacketFlags::CONNECT_RESPONSE, cookie.to_le_bytes().to_vec()),
        at(0.15),
    );
    let login = joined(&mut net).expect("logged in");
    net.send(login, 5, BlobKind::LOGIN, vec![0x11; 40]);
    net.tick(at(0.15));
    let first = sent_to(&mut net, player);
    assert!(
        first[0].optional.contains_key(&PacketFlags::TIME_SYNC),
        "the first data carries the clock"
    );
    assert_eq!(intervals(&first), [2], "the login's first data");
    net.send(login, 5, BlobKind::LOGIN, vec![0x22; 40]);
    net.tick(at(10.1));
    assert_eq!(
        intervals(&sent_to(&mut net, player)),
        [22],
        "two to a second from the handshake"
    );

    let referral = net.refer(login, at(10.1)).expect("a referral");
    net.tick(at(10.1));
    sent_to(&mut net, player);
    net.on_datagram(
        Port::WORLD_EVEN,
        player,
        &request(
            PacketFlags::WORLD_LOGIN_REQUEST,
            referral.to_le_bytes().to_vec(),
        ),
        at(11.0),
    );
    let handshake = sent_to(&mut net, player);
    assert_eq!(intervals(&handshake), [0], "the world's handshake");
    let cookie = the_cookie(&handshake[0]);
    net.on_datagram(
        Port::WORLD_ODD,
        player,
        &request(PacketFlags::CONNECT_RESPONSE, cookie.to_le_bytes().to_vec()),
        at(11.05),
    );
    let in_world = joined(&mut net).expect("in the world");
    let switch = sent_to(&mut net, player);
    assert!(switch[0].optional.contains_key(&PacketFlags::SERVER_SWITCH));
    assert_eq!(intervals(&switch), [1], "the server switch");
    net.send(in_world, 3, BlobKind::PRIVATE, vec![0x33; 40]);
    net.tick(at(11.1));
    assert_eq!(
        intervals(&sent_to(&mut net, player)),
        [2],
        "the world's first data"
    );
}
