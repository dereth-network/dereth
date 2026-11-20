//! The recorder on loopback between a stand-in client and stand-in servers: what it passes on,
//! what it writes, what it refuses, and the referral it points back at itself. Each side sends
//! only once what it waits for has arrived, so the order is the test's; waits end on a datagram,
//! and their timeout only guards against a hang.
//! Behaviour: none (a tool's contract, not game behavior)

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::Path;

use dereth_pcap::link::{Datagram, Link};
use dereth_pcap::pcap::CaptureReader;
use dereth_transport::conn::Referral;
use dereth_transport::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};

use super::*;
use crate::testing::{client, first_port, receive, server_pair, v4, Scratch};

fn start(server: SocketAddrV4, out: &Path) -> Recorder {
    Recorder::start(&Options {
        listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server,
        out: out.to_owned(),
    })
    .expect("the recorder starts")
}

/// Every byte value, and a datagram of the size the game sends at most.
fn payloads() -> [Vec<u8>; 4] {
    [
        (0..=255).collect(),
        vec![0xA5; 484],
        b"the second port".to_vec(),
        (0..300u32).map(|i| (i * 7).to_le_bytes()[0]).collect(),
    ]
}

/// What one exchange through the recorder looked like from each end.
struct Exchange {
    client: SocketAddrV4,
    server: SocketAddrV4,
    /// What the servers' sockets received, by port.
    at_server: [Vec<u8>; 2],
    /// What the client received, and from where.
    at_client: [(Vec<u8>, SocketAddrV4); 2],
    listen: SocketAddrV4,
    counts: Counts,
}

/// A client sends on each of the two ports and each server port answers.
fn exchange(out: &Path) -> Exchange {
    let servers = server_pair();
    let server = first_port(&servers);
    let recorder = start(server, out);
    let listen = recorder.listen();
    let c = client();
    let [a, b, cc, d] = payloads();

    c.send_to(&a, listen).expect("sent");
    let (got_a, upstream0) = receive(&servers[0]);
    servers[0].send_to(&b, upstream0).expect("sent");
    let at_client0 = receive(&c);

    c.send_to(&cc, above(listen, 1)).expect("sent");
    let (got_c, upstream1) = receive(&servers[1]);
    servers[1].send_to(&d, upstream1).expect("sent");
    let at_client1 = receive(&c);

    let counts = recorder.stop().expect("the file is finished");
    Exchange {
        client: v4(c.local_addr().expect("bound")),
        server,
        at_server: [got_a, got_c],
        at_client: [at_client0, at_client1],
        listen,
        counts,
    }
}

fn recorded(path: &Path) -> Vec<Datagram> {
    let file = std::fs::File::open(path).expect("the file");
    let mut link = Link::new();
    CaptureReader::new(std::io::BufReader::new(file))
        .expect("a capture")
        .map(|r| {
            let r = r.expect("a whole record");
            link.frame(r.link_type, r.ts, &r.data).expect("a datagram")
        })
        .collect()
}

fn ends(a: SocketAddrV4) -> (Ipv4Addr, u16) {
    (*a.ip(), a.port())
}

#[test]
fn datagrams_pass_through_both_ways_unchanged() {
    let scratch = Scratch::new();
    let x = exchange(&scratch.file());
    let [a, b, c, d] = payloads();
    assert_eq!(x.at_server, [a, c]);
    assert_eq!(x.at_client[0].0, b);
    assert_eq!(x.at_client[1].0, d);
    // Each answer comes back from the port of the recorder's the client sent to.
    assert_eq!(x.at_client[0].1, x.listen);
    assert_eq!(x.at_client[1].1, above(x.listen, 1));
    assert_eq!(
        x.counts,
        Counts {
            to_server: 2,
            to_client: 2,
            referrals: 0
        }
    );
}

#[test]
fn the_file_holds_every_datagram_that_passed_between_client_and_server() {
    let scratch = Scratch::new();
    let x = exchange(&scratch.file());
    let [a, b, c, d] = payloads();
    let got = recorded(&scratch.file());
    let rows: Vec<_> = got
        .iter()
        .map(|g| (g.src, g.dst, g.payload.clone()))
        .collect();
    let (client, p, p1) = (ends(x.client), ends(x.server), ends(above(x.server, 1)));
    assert_eq!(
        rows,
        vec![
            (client, p, a),
            (p, client, b),
            (client, p1, c),
            (p1, client, d)
        ]
    );
    assert!(got.windows(2).all(|w| w[0].ts <= w[1].ts));
}

#[test]
fn a_listen_address_off_loopback_is_refused() {
    let scratch = Scratch::new();
    for listen in ["0.0.0.0:0", "192.0.2.1:9100", "[::1]:0"] {
        let e = Recorder::start(&Options {
            listen: listen.parse().expect("an address"),
            server: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9000),
            out: scratch.file(),
        })
        .expect_err(listen);
        assert!(e.contains("loopback"), "{listen}: {e}");
        assert!(!scratch.file().exists(), "{listen} made a file");
    }
}

#[test]
fn a_recording_never_writes_over_a_file() {
    let scratch = Scratch::new();
    std::fs::write(scratch.file(), b"kept").expect("a file");
    let server = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9000);
    let e = Recorder::start(&Options {
        listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server,
        out: scratch.file(),
    })
    .expect_err("an existing file");
    assert!(e.contains("already exists"), "{e}");
    assert_eq!(std::fs::read(scratch.file()).expect("the file"), b"kept");
}

/// A server's referral to `world`, encrypted under `key` as a server sends one.
fn referral(world: SocketAddrV4, key: u32) -> Vec<u8> {
    let body = Referral {
        cookie: 0x0123_4567_89AB_CDEF,
        addr: world,
        id_server: 0x18,
        family: 2,
    }
    .to_bytes();
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: 2,
        rec_id: 0,
        ..ProtoHeader::default()
    });
    p.add_optional_header(PacketFlags::REFERRAL, body.to_vec())
        .expect("a referral");
    p.serialize(Some(key)).expect("serialises")
}

#[test]
fn a_referral_brings_the_client_through_the_recorder_to_the_world() {
    let scratch = Scratch::new();
    let login = server_pair();
    let world = server_pair();
    let world_at = first_port(&world);
    let recorder = start(first_port(&login), &scratch.file());
    let c = client();

    c.send_to(b"log me in", recorder.listen()).expect("sent");
    let (_, upstream) = receive(&login[0]);
    let key = 0x5EED_1234;
    let sent = referral(world_at, key);
    login[1].send_to(&sent, upstream).expect("sent");
    let (got, _) = receive(&c);

    // The client is referred to the recorder, under the checksum the same key gives.
    let p = ParsedPacket::parse(&got).expect("a datagram the transport reads");
    assert!(p.checksum_ok(Some(key)), "the checksum no longer verifies");
    let r = Referral::from_bytes(&p.optional[&PacketFlags::REFERRAL]).expect("a referral");
    assert_eq!(r.addr.ip(), &Ipv4Addr::LOCALHOST);
    assert_ne!(r.addr, world_at);
    assert_eq!(r.cookie, 0x0123_4567_89AB_CDEF);

    // And the recorder takes the client on to the world, on both its ports.
    c.send_to(b"world login", r.addr).expect("sent");
    let (to_world, from) = receive(&world[0]);
    assert_eq!(to_world, b"world login");
    world[0].send_to(b"welcome", from).expect("sent");
    assert_eq!(receive(&c), (b"welcome".to_vec(), r.addr));
    c.send_to(b"connect response", above(r.addr, 1))
        .expect("sent");
    assert_eq!(receive(&world[1]).0, b"connect response");

    let counts = recorder.stop().expect("the file is finished");
    assert_eq!(counts.referrals, 1);
    // The file holds the referral as the server sent it, and the world's traffic at the world.
    let got = recorded(&scratch.file());
    assert_eq!(got[1].payload, sent);
    assert_eq!(got[2].dst, ends(world_at));
    assert_eq!(got[3].src, ends(world_at));
    assert_eq!(got[4].dst, ends(above(world_at, 1)));
}

#[test]
fn a_datagram_with_no_referral_is_not_changed() {
    let mut p = OutPacket::new(ProtoHeader::default());
    p.add_optional_header(PacketFlags::CONNECT_REQUEST, vec![0; 32])
        .expect("a connect request");
    let plain = p.serialize(None).expect("serialises");
    let to = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9102);
    assert_eq!(referred_to(&plain), None);
    assert_eq!(refer_to(&plain, to), None);
    assert_eq!(refer_to(b"not a datagram", to), None);
}
