//! What a login holds before its client answers its handshake: nothing a client that answers needs,
//! nothing of a connection already up, and one answer for each time it asks.

use std::net::{SocketAddr, SocketAddrV4};

use dereth_transport::conn::ConnectRequest;
use dereth_transport::wire::{PacketFlags, ParsedPacket};
use yalain_net::endpoint::standalone;
use yalain_net::handshakes::{MOST_WAITING, WAITING_SECONDS};
use yalain_net::login::build_login;
use yalain_net::server::{CloseReason, ConnId, Datagram, NetConfig, NetEvent, Port, ServerNet};
use yalain_net::{fixed_stream, LocalTime};

const NOW: LocalTime = LocalTime(10.0);

fn addr(s: &str) -> SocketAddr {
    s.parse().expect("an address")
}

fn services() -> ServerNet {
    let world: SocketAddrV4 = "203.0.113.10:9002".parse().expect("an address");
    ServerNet::new(NetConfig::retail_ids(world), fixed_stream(7), 0.0, NOW)
}

fn login_request() -> Vec<u8> {
    standalone(
        0,
        0,
        PacketFlags::LOGIN_REQUEST,
        build_login("tester", b"a ticket", 1),
    )
}

fn connect_response(cookie: u64) -> Vec<u8> {
    standalone(
        0,
        0,
        PacketFlags::CONNECT_RESPONSE,
        cookie.to_le_bytes().to_vec(),
    )
}

fn handshakes(out: &[Datagram]) -> Vec<ConnectRequest> {
    out.iter()
        .filter_map(|d| {
            let p = ParsedPacket::parse(&d.bytes).ok()?;
            ConnectRequest::from_bytes(p.optional.get(&PacketFlags::CONNECT_REQUEST)?).ok()
        })
        .collect()
}

fn events(net: &mut ServerNet) -> Vec<NetEvent> {
    std::iter::from_fn(|| net.poll_event()).collect()
}

/// A login from `from`: the cookie it is answered with.
fn asks(net: &mut ServerNet, from: SocketAddr, now: LocalTime) -> u64 {
    net.on_datagram(Port::LOGIN_EVEN, from, &login_request(), now);
    let got = handshakes(&net.take_datagrams());
    assert_eq!(got.len(), 1, "one answer to {from}");
    got[0].cookie
}

/// `from` answers `cookie`; the connection that logs in by it.
fn answers(net: &mut ServerNet, from: SocketAddr, cookie: u64, now: LocalTime) -> Option<ConnId> {
    net.on_datagram(Port::LOGIN_ODD, from, &connect_response(cookie), now);
    events(net).into_iter().find_map(|e| match e {
        NetEvent::LoggedIn { conn, .. } => Some(conn),
        _ => None,
    })
}

/// A far address of its own for each `i`.
fn stranger(i: u32) -> SocketAddr {
    let [_, a, b, c] = i.to_be_bytes();
    SocketAddr::from(([10, a, b, c], 40_000))
}

/// However many logins go unanswered, from however many addresses, none of them is a connection, what
/// waits is bounded, and a client that answers gets in, its own world login with it.
/// Behaviour: server.net.handshake-holds-nothing-until-answered
#[test]
fn logins_nobody_answers_hold_no_place_from_a_client_that_answers() {
    let mut net = services();
    let player = addr("198.51.100.7:50000");
    let cookie = asks(&mut net, player, NOW);
    let conn = answers(&mut net, player, cookie, NOW).expect("logged in");
    let cookie = net.refer(conn, NOW).expect("a referral");
    net.take_datagrams();
    net.on_datagram(
        Port::WORLD_EVEN,
        player,
        &standalone(
            0,
            0,
            PacketFlags::WORLD_LOGIN_REQUEST,
            cookie.to_le_bytes().to_vec(),
        ),
        NOW,
    );
    let world = handshakes(&net.take_datagrams())[0].cookie;

    for i in 0..5_000 {
        asks(&mut net, stranger(i), NOW);
    }
    assert_eq!(net.connection_count(), 1, "none is a connection");
    assert!(net.handshakes_waiting() <= MOST_WAITING + 1, "bounded");

    let newcomer = addr("198.51.100.8:50000");
    let cookie = asks(&mut net, newcomer, NOW);
    assert!(
        answers(&mut net, newcomer, cookie, NOW).is_some(),
        "a client that answers gets in"
    );
    net.on_datagram(Port::WORLD_ODD, player, &connect_response(world), NOW);
    assert!(
        events(&mut net)
            .iter()
            .any(|e| matches!(e, NetEvent::WorldJoined { .. })),
        "and logins left waiting did not push out a world login"
    );

    // Ids go round: as many again log in, one after another.
    for i in 0..600 {
        let client = stranger(100_000 + i);
        let cookie = asks(&mut net, client, NOW);
        let conn = answers(&mut net, client, cookie, NOW).expect("logged in");
        net.close(conn);
        events(&mut net);
    }
}

/// Logins from one address push out only that address's own: another's handshake still holds.
/// Behaviour: server.net.handshake-holds-nothing-until-answered
#[test]
fn logins_from_one_address_do_not_push_out_anothers() {
    let mut net = services();
    let other = addr("198.51.100.7:50000");
    let cookie = asks(&mut net, other, NOW);
    let first = addr("198.51.100.9:1000");
    let oldest = asks(&mut net, first, NOW);
    for port in 1001..4000 {
        asks(&mut net, SocketAddr::new(first.ip(), port), NOW);
    }
    assert!(net.handshakes_waiting() < 64, "that address's are bounded");
    assert_eq!(
        answers(&mut net, first, oldest, NOW),
        None,
        "its oldest gave way"
    );
    assert!(
        answers(&mut net, other, cookie, NOW).is_some(),
        "the other's held"
    );
}

/// A login is answered once for each time it asks, at a pace, and never of the server's own accord,
/// so an address that never asked gets no more than was asked in its name. Asked again while it
/// waits, it is sent the same handshake, which it can still answer; past its wait it is forgotten,
/// and asks anew.
/// Behaviour: server.net.handshake-answered-once-per-ask
#[test]
fn a_login_is_answered_once_each_time_it_asks_and_never_unasked() {
    let mut net = services();
    let client = addr("198.51.100.7:50000");
    let cookie = asks(&mut net, client, NOW);
    net.on_datagram(Port::LOGIN_EVEN, client, &login_request(), NOW);
    assert!(net.take_datagrams().is_empty(), "not again at once");
    let mut sent = 0;
    let mut t = NOW.0;
    while t < NOW.0 + WAITING_SECONDS - 1.0 {
        t += 0.1;
        net.tick(LocalTime(t));
        sent += net.take_datagrams().len();
    }
    assert_eq!(sent, 0, "nothing unasked");
    let now = LocalTime(t);
    assert_eq!(asks(&mut net, client, now), cookie, "the same handshake");
    assert_eq!(net.handshakes_waiting(), 1);
    for _ in 0..1_000 {
        net.on_datagram(Port::LOGIN_EVEN, client, &login_request(), now);
    }
    assert!(
        net.take_datagrams().is_empty(),
        "at a pace, whatever it asks"
    );

    t += 2.0;
    net.tick(LocalTime(t));
    assert_eq!(net.handshakes_waiting(), 0, "its wait is over");
    assert_eq!(net.take_datagrams().len(), 0);
    let late = LocalTime(t);
    assert_eq!(
        answers(&mut net, client, cookie, late),
        None,
        "too late to answer"
    );
    let again = asks(&mut net, client, late);
    assert_ne!(again, cookie, "it asks anew");
    assert!(answers(&mut net, client, again, late).is_some());
}

/// A login naming the address of a connection already up ends nothing while it is unanswered, and
/// nothing at all if its answer never comes; answered, it is the client starting over.
/// Behaviour: server.net.handshake-replaces-only-once-answered
#[test]
fn a_login_from_an_address_in_use_replaces_its_connection_only_once_answered() {
    let mut net = services();
    let client = addr("198.51.100.7:50000");
    let cookie = asks(&mut net, client, NOW);
    let first = answers(&mut net, client, cookie, NOW).expect("logged in");

    let cookie = asks(&mut net, client, NOW);
    assert_eq!(events(&mut net), [], "nothing ended");
    assert_eq!(net.connection_count(), 1);
    assert!(net.peer(first).is_some(), "it goes on");

    // Not answered from elsewhere.
    net.on_datagram(
        Port::LOGIN_ODD,
        addr("198.51.100.200:50000"),
        &connect_response(cookie),
        NOW,
    );
    assert_eq!(events(&mut net), [], "an answer from another address");
    assert!(net.peer(first).is_some(), "it still goes on");

    net.on_datagram(Port::LOGIN_ODD, client, &connect_response(cookie), NOW);
    let got = events(&mut net);
    assert!(
        got.contains(&NetEvent::Closed {
            conn: first,
            why: CloseReason::Replaced
        }),
        "{got:?}"
    );
    assert!(
        got.iter().any(|e| matches!(e, NetEvent::LoggedIn { .. })),
        "{got:?}"
    );
}

/// A login too large to be one is not kept.
/// Behaviour: server.net.handshake-holds-nothing-until-answered
#[test]
fn a_login_far_larger_than_any_client_sends_is_ignored() {
    let mut net = services();
    let big = standalone(
        0,
        0,
        PacketFlags::LOGIN_REQUEST,
        build_login("tester", &vec![b'a'; 4_000], 1),
    );
    net.on_datagram(Port::LOGIN_EVEN, addr("198.51.100.7:50000"), &big, NOW);
    assert!(net.take_datagrams().is_empty());
    assert_eq!(net.handshakes_waiting(), 0);
}
