//! Where cookies and checksum seeds come from, and what a referral's cookie lets in.

use std::net::{SocketAddr, SocketAddrV4};
use std::sync::{Arc, Mutex};

use dereth_transport::conn::{ConnectRequest, Referral};
use dereth_transport::wire::{PacketFlags, ParsedPacket};
use yalain_net::endpoint::standalone;
use yalain_net::login::build_login;
use yalain_net::server::{CloseReason, ConnId, Datagram, NetConfig, NetEvent, Port, ServerNet};
use yalain_net::{fixed_stream, LocalTime};

const NOW: LocalTime = LocalTime(10.0);

fn addr(s: &str) -> SocketAddr {
    s.parse().expect("an address")
}

fn services(random: impl FnMut() -> Result<u64, String> + Send + 'static) -> ServerNet {
    let world: SocketAddrV4 = "203.0.113.10:9002".parse().expect("an address");
    ServerNet::new(NetConfig::retail_ids(world), random, 0.0, NOW)
}

/// What a client sends: an unsequenced packet of one section.
fn packet(mask: u32, body: Vec<u8>) -> Vec<u8> {
    standalone(0, 0, mask, body)
}

fn login_request() -> Vec<u8> {
    packet(
        PacketFlags::LOGIN_REQUEST,
        build_login("tester", b"a ticket", 1),
    )
}

fn world_login(cookie: u64) -> Vec<u8> {
    packet(
        PacketFlags::WORLD_LOGIN_REQUEST,
        cookie.to_le_bytes().to_vec(),
    )
}

fn connect_response(cookie: u64) -> Vec<u8> {
    packet(PacketFlags::CONNECT_RESPONSE, cookie.to_le_bytes().to_vec())
}

/// The handshake in the datagrams, if there is one.
fn handshake(out: &[Datagram]) -> Option<ConnectRequest> {
    out.iter().find_map(|d| {
        let p = ParsedPacket::parse(&d.bytes).ok()?;
        ConnectRequest::from_bytes(p.optional.get(&PacketFlags::CONNECT_REQUEST)?).ok()
    })
}

/// The referral in the datagrams, if there is one.
fn referral(out: &[Datagram]) -> Option<Referral> {
    out.iter().find_map(|d| {
        let p = ParsedPacket::parse(&d.bytes).ok()?;
        Referral::from_bytes(p.optional.get(&PacketFlags::REFERRAL)?).ok()
    })
}

fn events(net: &mut ServerNet) -> Vec<NetEvent> {
    std::iter::from_fn(|| net.poll_event()).collect()
}

/// `client` logs in; its login connection, and the cookie of the handshake it answered.
fn logged_in(net: &mut ServerNet, client: SocketAddr) -> (ConnId, u64) {
    net.on_datagram(Port::LOGIN_EVEN, client, &login_request(), NOW);
    let cookie = handshake(&net.take_datagrams())
        .expect("a handshake")
        .cookie;
    net.on_datagram(Port::LOGIN_ODD, client, &connect_response(cookie), NOW);
    let conn = events(net)
        .into_iter()
        .find_map(|e| match e {
            NetEvent::LoggedIn { conn, .. } => Some(conn),
            _ => None,
        })
        .expect("logged in");
    (conn, cookie)
}

/// `client` logs in and is sent to the world; its login connection and the referral's cookie.
fn referred(net: &mut ServerNet, client: SocketAddr) -> (ConnId, u64) {
    let (conn, _) = logged_in(net, client);
    let cookie = net.refer(conn, NOW).expect("a referral");
    net.take_datagrams();
    (conn, cookie)
}

/// A world login from `from`: the handshake it was answered with, if any.
fn world_handshake(net: &mut ServerNet, from: SocketAddr, cookie: u64) -> Option<ConnectRequest> {
    net.on_datagram(Port::WORLD_EVEN, from, &world_login(cookie), NOW);
    handshake(&net.take_datagrams())
}

/// Every cookie and checksum seed a client is sent is taken from the source the host gave, whole:
/// a login's handshake, the referral and the world's handshake.
/// Behaviour: server.net.cookies-and-seeds-from-the-hosts-source
#[test]
fn every_cookie_and_checksum_seed_sent_comes_from_the_source_the_host_gave() {
    let mut numbers = [
        0x1111_1111_2222_2222_u64,
        0x3333_3333_3333_3333,
        0x4444_4444_4444_4444,
        0x5555_5555_6666_6666,
        0x7777_7777_7777_7777,
    ]
    .into_iter();
    let mut net = services(move || numbers.next().ok_or_else(|| "out of numbers".to_owned()));
    let client = addr("198.51.100.7:50000");

    net.on_datagram(Port::LOGIN_EVEN, client, &login_request(), NOW);
    let login = handshake(&net.take_datagrams()).expect("a handshake");
    assert_eq!(login.cookie, 0x3333_3333_3333_3333);
    let seeds = login.seeds();
    assert_eq!(
        (seeds.server_to_client, seeds.client_to_server),
        (0x1111_1111, 0x2222_2222)
    );
    net.on_datagram(
        Port::LOGIN_ODD,
        client,
        &connect_response(login.cookie),
        NOW,
    );
    let conn = events(&mut net)
        .into_iter()
        .find_map(|e| match e {
            NetEvent::LoggedIn { conn, .. } => Some(conn),
            _ => None,
        })
        .expect("logged in");

    assert_eq!(net.refer(conn, NOW), Some(0x4444_4444_4444_4444));
    net.tick(LocalTime(11.0));
    let sent = referral(&net.take_datagrams()).expect("the referral goes");
    assert_eq!(sent.cookie, 0x4444_4444_4444_4444);

    let world = world_handshake(&mut net, client, sent.cookie).expect("a handshake");
    assert_eq!(world.cookie, 0x7777_7777_7777_7777);
    let seeds = world.seeds();
    assert_eq!(
        (seeds.server_to_client, seeds.client_to_server),
        (0x5555_5555, 0x6666_6666)
    );
}

/// A source that fails: a login gets no handshake and makes no connection, and the client's resend
/// is served once the source is back; a world login gets none either and keeps its cookie, so its
/// resend gets in; a referral with no cookie to give closes the login, telling the client. Nothing
/// is sent that the source did not give.
/// Behaviour: server.net.failed-random-source-makes-no-connection
#[test]
fn a_failed_random_source_gives_no_handshake_and_no_number_of_its_own() {
    let failing = Arc::new(Mutex::new(true));
    let source = failing.clone();
    let mut stream = fixed_stream(9);
    let mut net = services(move || {
        if *source.lock().expect("not poisoned") {
            Err("no entropy".to_owned())
        } else {
            stream()
        }
    });
    let set = |on: bool| *failing.lock().expect("not poisoned") = on;
    let client = addr("198.51.100.7:50000");

    // A login, then its resend once the source is back.
    net.on_datagram(Port::LOGIN_EVEN, client, &login_request(), NOW);
    assert!(net.take_datagrams().is_empty(), "no handshake");
    assert_eq!(net.connection_count(), 0, "no connection");
    set(false);
    let (conn, _) = logged_in(&mut net, client);

    // A referral: the login ends, telling the client.
    set(true);
    assert_eq!(net.refer(conn, NOW), None);
    assert_eq!(
        events(&mut net),
        [NetEvent::Closed {
            conn,
            why: CloseReason::Closed
        }]
    );
    assert_eq!(net.take_datagrams().len(), 1, "the goodbye");

    // A world login: no handshake, and its cookie still good for its resend.
    set(false);
    let (_, cookie) = referred(&mut net, client);
    set(true);
    assert!(world_handshake(&mut net, client, cookie).is_none());
    assert_eq!(net.connection_count(), 1, "the login alone");
    set(false);
    assert!(
        world_handshake(&mut net, client, cookie).is_some(),
        "the resend gets in"
    );
}

/// A referral's cookie is good once. A resend while its handshake waits makes no second connection
/// and leaves the handshake good; a world login with it after that, from the client's address (a
/// resend that arrived late) or another port or address behind it, makes no connection and ends
/// nothing.
/// Behaviour: server.net.referral-cookie-used-once
#[test]
fn a_referrals_cookie_lets_in_one_world_connection_and_a_resend_changes_nothing() {
    let mut net = services(fixed_stream(1));
    let client = addr("198.51.100.7:50000");
    let (login, cookie) = referred(&mut net, client);

    let first = world_handshake(&mut net, client, cookie).expect("let in");
    assert_eq!(net.connection_count(), 2, "the login and the world");
    assert!(
        world_handshake(&mut net, client, cookie).is_none(),
        "a resend while the handshake waits: the handshake goes on its own clock"
    );
    assert_eq!(net.connection_count(), 2);
    assert!(
        world_handshake(&mut net, addr("198.51.100.7:50001"), cookie).is_none(),
        "the same address, another port, while the handshake waits"
    );
    assert_eq!(net.connection_count(), 2);

    net.on_datagram(
        Port::WORLD_ODD,
        client,
        &connect_response(first.cookie),
        NOW,
    );
    let joined = events(&mut net);
    assert!(
        matches!(joined.as_slice(), [NetEvent::WorldJoined { login: l, cookie: c, .. }]
            if *l == login && *c == cookie),
        "{joined:?}"
    );

    for from in [client, addr("198.51.100.7:50001")] {
        assert!(
            world_handshake(&mut net, from, cookie).is_none(),
            "{from}: used"
        );
    }
    assert_eq!(net.connection_count(), 2);
    assert_eq!(events(&mut net), [], "the world connection goes on");

    // A handshake's own cookie is good once too.
    net.on_datagram(
        Port::WORLD_ODD,
        client,
        &connect_response(first.cookie),
        NOW,
    );
    assert_eq!(events(&mut net), [], "no second arrival in the world");
}

/// A cookie read off the wire lets no other address in: a world login with a client's cookie from
/// another address is refused, leaves the cookie unspent, and the client itself still gets in. A
/// player whose address changed between its login and the world is refused the same way, and gets
/// in by logging in again from the new one.
/// Behaviour: server.net.world-login-from-its-logins-address
#[test]
fn a_world_login_from_another_address_than_its_login_is_refused_and_spends_nothing() {
    let mut net = services(fixed_stream(2));
    let client = addr("198.51.100.7:50000");
    let (_, cookie) = referred(&mut net, client);

    assert!(
        world_handshake(&mut net, addr("198.51.100.200:50000"), cookie).is_none(),
        "another address on the internet"
    );
    assert!(
        world_handshake(&mut net, addr("192.168.1.30:50000"), cookie).is_none(),
        "a referral to the internet's address says nothing of a network close by"
    );
    assert_eq!(net.connection_count(), 1);
    assert!(world_handshake(&mut net, client, cookie).is_some());

    let moved_from = addr("198.51.100.8:50000");
    let moved_to = addr("198.51.100.9:50000");
    let (_, cookie) = referred(&mut net, moved_from);
    assert!(world_handshake(&mut net, moved_to, cookie).is_none());
    let (_, cookie) = referred(&mut net, moved_to);
    assert!(world_handshake(&mut net, moved_to, cookie).is_some());
}
