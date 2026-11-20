//! The interval a server's packets carry in their headers, and the clock they carry on it: a
//! connection counts from 1, two to a second, from when its handshake was answered, and sends its
//! clock with its first data and at each fortieth interval.

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

fn services() -> (ServerNet, SocketAddr) {
    let world: SocketAddrV4 = "203.0.113.10:9002".parse().expect("an address");
    let net = ServerNet::new(NetConfig::retail_ids(world), fixed_stream(7), 0.0, at(0.0));
    (net, "198.51.100.7:50000".parse().expect("an address"))
}

/// The player logs in: its handshake at the start, its answer 0.15 s later.
fn logged_in(net: &mut ServerNet, player: SocketAddr) -> ConnId {
    net.on_datagram(
        Port::LOGIN_EVEN,
        player,
        &request(
            PacketFlags::LOGIN_REQUEST,
            build_login("tester", b"a ticket", 1),
        ),
        at(0.0),
    );
    let cookie = the_cookie(&sent_to(net, player)[0]);
    net.on_datagram(
        Port::LOGIN_ODD,
        player,
        &request(PacketFlags::CONNECT_RESPONSE, cookie.to_le_bytes().to_vec()),
        at(0.15),
    );
    joined(net).expect("logged in")
}

/// The intervals of the packets that carry the clock.
fn clocks(packets: &[ParsedPacket]) -> Vec<u16> {
    packets
        .iter()
        .filter(|p| p.optional.contains_key(&PacketFlags::TIME_SYNC))
        .map(|p| p.header.interval)
        .collect()
}

/// A login sent a message every half second for a minute carries the clock on its first data and
/// then on intervals 40, 80 and 120: on a grid of forty intervals from the start, not twenty
/// seconds after the last, which would put them on 42, 82 and 122.
/// Behaviour: server.net.clock-every-fortieth-interval
#[test]
fn the_server_sends_its_clock_with_its_first_data_and_at_each_fortieth_interval() {
    let (mut net, player) = services();
    let login = logged_in(&mut net, player);
    let mut sent = Vec::new();
    for k in 0..124u8 {
        let t = at(0.15 + f64::from(k) * 0.5);
        net.send(login, 5, BlobKind::LOGIN, vec![k; 40]);
        net.tick(t);
        sent.extend(sent_to(&mut net, player));
    }
    assert_eq!(intervals(&sent)[..2], [2, 3], "one packet an interval");
    assert_eq!(clocks(&sent), [2, 40, 80, 120]);
}

/// The player logs in and enters the world: its world login at 1.0 s, its answer at 1.05 s.
fn in_the_world(net: &mut ServerNet, player: SocketAddr) -> ConnId {
    let login = logged_in(net, player);
    let referral = net.refer(login, at(0.2)).expect("a referral");
    net.tick(at(0.2));
    sent_to(net, player);
    net.on_datagram(
        Port::WORLD_EVEN,
        player,
        &request(
            PacketFlags::WORLD_LOGIN_REQUEST,
            referral.to_le_bytes().to_vec(),
        ),
        at(1.0),
    );
    let cookie = the_cookie(&sent_to(net, player)[0]);
    net.on_datagram(
        Port::WORLD_ODD,
        player,
        &request(PacketFlags::CONNECT_RESPONSE, cookie.to_le_bytes().to_vec()),
        at(1.05),
    );
    joined(net).expect("in the world")
}

fn game_time(p: &ParsedPacket) -> f64 {
    let b: [u8; 8] = p.optional[&PacketFlags::TIME_SYNC][..8]
        .try_into()
        .expect("eight bytes");
    f64::from_le_bytes(b)
}

/// A world sent a message too long for its window, whose client acknowledges none of it: the
/// message stops when the window is full, and the clock due at interval 40 does not wait for it.
/// It goes on the next packet sent, the server's acknowledgement, with the game time it is sent
/// at, not the time it came due.
/// Behaviour: server.net.clock-rides-the-next-packet-sent
#[test]
fn a_clock_due_while_the_window_is_full_rides_the_next_acknowledgement_with_its_send_time() {
    let (mut net, player) = services();
    let world = in_the_world(&mut net, player);
    sent_to(&mut net, player);
    net.send(world, 5, BlobKind::PRIVATE, vec![0x55; 2_000_000]);
    let mut sent = Vec::new();
    for k in 0..100u32 {
        let t = 1.05 + f64::from(k) * 0.25;
        net.tick(at(t));
        sent.extend(
            net.take_datagrams()
                .into_iter()
                .filter(|d| d.to == player && d.from == Port::WORLD_ODD)
                .map(|d| (t, ParsedPacket::parse(&d.bytes).expect("a packet"))),
        );
    }
    let (t, clock) = sent
        .iter()
        .filter(|(_, p)| p.optional.contains_key(&PacketFlags::TIME_SYNC))
        .nth(1)
        .expect("the clock after the first");
    let interval = clock.header.interval;
    assert!(interval > 40, "it waited for a packet to go: {interval}");
    assert!(
        !sent
            .iter()
            .any(|(_, p)| (40..=interval).contains(&p.header.interval) && !p.fragments.is_empty()),
        "the window is full: no part of the message goes from when the clock is due"
    );
    assert!(
        clock.optional.contains_key(&PacketFlags::ACK_SEQUENCE) && clock.fragments.is_empty(),
        "it rides the acknowledgement"
    );
    assert!(
        (game_time(clock) - t).abs() < 1e-6,
        "the game time it is sent at, {t}, not {}",
        game_time(clock)
    );
}

/// A world whose first data is a message long enough to go by the pace sends its clock on the
/// first packet after the server switch, ahead of the message, not after it.
/// Behaviour: server.net.world-clock-on-its-first-data
#[test]
fn a_worlds_clock_goes_first_after_its_switch_even_ahead_of_a_paced_message() {
    let (mut net, player) = services();
    let world = in_the_world(&mut net, player);
    net.send(world, 5, BlobKind::PRIVATE, vec![0x44; 70_000]);
    net.tick(at(1.05));
    let sent = sent_to(&mut net, player);
    assert!(sent[0].optional.contains_key(&PacketFlags::SERVER_SWITCH));
    assert!(sent.len() > 2, "the message's first packets");
    assert!(
        sent[1].optional.contains_key(&PacketFlags::TIME_SYNC),
        "the clock first"
    );
    assert_eq!(
        sent[2].fragments[0].header.blob_num, 0,
        "a whole fragment does not fit beside it: the message's first goes next"
    );
    assert_eq!(clocks(&sent).len(), 1, "once");
}
