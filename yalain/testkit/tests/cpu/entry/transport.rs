//! The connections under loss, logins read together after a long pass, a held game clock, and the
//! datagrams of a fixed session.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use auberean_protocol::login::DataHeld;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use dereth_transport::conn::ConnectSeeds;
use dereth_transport::wire::ParsedPacket;
use yalain_net::endpoint::{Endpoint, Role};
use yalain_net::{
    fixed_stream, BlobKind, CloseReason, ConnId, LocalTime, NetConfig, NetEvent, Port, ServerNet,
};
use yalain_testkit::bot::Bot;
use yalain_testkit::{port_addr, Harness, WORLD_PORT};

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

/// The fixed session's virtual clock: when it starts, how long it runs, how often the server's
/// loop passes and how often each client runs a frame.
const SESSION_START: f64 = 1000.0;
const SESSION_LENGTH: f64 = 170.0;
const SERVER_PASS: f64 = 0.002;
const CLIENT_FRAME: f64 = 0.016;

/// The session's clients; the first two are on a link that loses datagrams.
const CLIENTS: usize = 6;
/// The client that falls silent, and when.
const SILENT: usize = 4;
const SILENT_AT: f64 = 20.0;
/// The client whose world connection the server ends, and when.
const ENDED: usize = 3;
const ENDED_AT: f64 = 60.0;

/// The fixed session's datagrams as recorded: their digest, and how many.
const RECORDED_SESSION: (u64, usize) = (0x6525_C6C1_88F0_9BAE, 14_703);

/// A 64-bit FNV-1a digest.
struct Digest(u64);

impl Digest {
    fn eat(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01B3);
        }
    }
}

/// The lossy link: one datagram in eleven to or from the first two clients is lost.
#[derive(Default)]
struct Lossy {
    seen: usize,
    lost: usize,
}

impl Lossy {
    fn loses(&mut self, client: usize) -> bool {
        if client >= 2 {
            return false;
        }
        self.seen += 1;
        let lost = self.seen.is_multiple_of(11);
        self.lost += usize::from(lost);
        lost
    }
}

/// What the fixed session ended with.
struct Session {
    /// Every datagram the server sent: the pass it went in, the port it went from, where it went,
    /// and its bytes.
    digest: u64,
    datagrams: usize,
    bots: Vec<Bot>,
    closed: Vec<CloseReason>,
    lost: usize,
}

/// One scripted session on a virtual clock, the server's loop passing every 2 ms and each client
/// running a frame every 16 ms. Six clients log in a little apart; the server answers each login
/// with three messages, one of them large, and sends it to the world, where it is sent a large
/// message, a message every quarter second and one on an ephemeral stream every tenth; each
/// client in the world sends a message a second, and the server answers every message. The
/// fifth client falls silent after 20 s, and the server ends the fourth's world connection at
/// 60 s.
#[allow(clippy::too_many_lines)]
fn scripted_session() -> Session {
    let world = SocketAddrV4::new(Ipv4Addr::LOCALHOST, WORLD_PORT);
    let mut net = ServerNet::new(
        NetConfig::retail_ids(world),
        fixed_stream(0x5EED),
        1.25e8,
        LocalTime(SESSION_START),
    );
    let addr =
        |i: usize| SocketAddr::from((Ipv4Addr::LOCALHOST, 50_000 + u16::try_from(i).expect("few")));
    let client_of = |a: SocketAddr| usize::from(a.port() - 50_000);
    let mut bots: Vec<Bot> = (0..CLIENTS)
        .map(|i| Bot::new(addr(i), port_addr(Port::LOGIN_EVEN)))
        .collect();
    let mut to_server: Vec<(SocketAddr, SocketAddr, Vec<u8>)> = Vec::new();
    let mut to_bots: Vec<Vec<(SocketAddr, Vec<u8>)>> = vec![Vec::new(); CLIENTS];
    let mut lossy = Lossy::default();
    let mut logged_in = [false; CLIENTS];
    let mut next_report: [Option<f64>; CLIENTS] = [None; CLIENTS];
    let mut refer_at: Vec<(ConnId, f64)> = Vec::new();
    let mut in_world: Vec<(ConnId, u32)> = Vec::new();
    let mut ended = false;
    let mut closed = Vec::new();
    let (mut next_frame, mut next_quarter, mut next_tenth) =
        (SESSION_START, SESSION_START, SESSION_START);
    let mut digest = Digest(0xCBF2_9CE4_8422_2325);
    let mut datagrams = 0;
    let mut pass: u32 = 0;
    let mut t = SESSION_START;
    while t < SESSION_START + SESSION_LENGTH {
        let now = LocalTime(t);
        for (from, to, bytes) in to_server.drain(..) {
            let port = Port::ALL
                .into_iter()
                .find(|p| port_addr(*p) == to)
                .expect("one of the server's ports");
            net.on_datagram(port, from, &bytes, now);
        }
        while let Some(e) = net.poll_event() {
            match e {
                NetEvent::LoggedIn { conn, .. } => {
                    net.send(conn, 5, BlobKind::LOGIN, vec![0x11; 60]);
                    net.send(conn, 5, BlobKind::LOGIN, vec![0x22; 900]);
                    net.send(conn, 5, BlobKind::LOGIN, vec![0x33; 40_000]);
                    refer_at.push((conn, t + 0.4));
                }
                NetEvent::WorldJoined { conn, .. } => {
                    let stream = net.new_stream(conn).expect("a stream");
                    net.send(conn, 5, BlobKind::PRIVATE, vec![0x44; 70_000]);
                    in_world.push((conn, stream));
                }
                NetEvent::Message {
                    conn, queue, bytes, ..
                } => {
                    let mut answer: Vec<u8> = bytes.iter().rev().copied().collect();
                    answer.push(0xAB);
                    net.send(conn, queue, BlobKind::UNORDERED, answer);
                }
                NetEvent::Closed { conn, why } => {
                    closed.push(why);
                    in_world.retain(|(c, _)| *c != conn);
                }
            }
        }
        refer_at.retain(|&(conn, at)| {
            if t < at {
                return true;
            }
            net.refer(conn, now).expect("a referral");
            false
        });
        if t >= next_quarter {
            next_quarter += 0.25;
            for &(conn, _) in &in_world {
                net.send(conn, 3, BlobKind::PRIVATE, vec![pass.to_le_bytes()[0]; 120]);
            }
        }
        if t >= next_tenth {
            next_tenth += 0.1;
            for &(conn, stream) in &in_world {
                net.send_ephemeral(conn, 4, stream, vec![pass.to_le_bytes()[1]; 40]);
            }
        }
        if !ended && t >= SESSION_START + ENDED_AT {
            ended = true;
            let (conn, _) = *in_world
                .iter()
                .find(|(c, _)| net.peer(*c) == Some(addr(ENDED)))
                .expect("the client's world connection");
            net.end(conn, now);
        }
        net.tick(now);
        for d in net.take_datagrams() {
            let from = port_addr(d.from);
            digest.eat(&pass.to_le_bytes());
            digest.eat(&from.port().to_le_bytes());
            digest.eat(d.to.to_string().as_bytes());
            digest.eat(&u32::try_from(d.bytes.len()).expect("small").to_le_bytes());
            digest.eat(&d.bytes);
            datagrams += 1;
            let i = client_of(d.to);
            let silent = i == SILENT && t >= SESSION_START + SILENT_AT;
            if !lossy.loses(i) && !silent {
                to_bots[i].push((from, d.bytes));
            }
        }
        if t >= next_frame {
            next_frame += CLIENT_FRAME;
            for (i, b) in bots.iter_mut().enumerate() {
                if i == SILENT && t >= SESSION_START + SILENT_AT {
                    continue;
                }
                for (from, bytes) in to_bots[i].drain(..) {
                    b.on_datagram(from, &bytes, now);
                }
                #[allow(clippy::cast_precision_loss)]
                if !logged_in[i] && t >= SESSION_START + 0.137 * i as f64 {
                    logged_in[i] = true;
                    b.log_in(&format!("wire{i}"), b"ticket", now);
                }
                if b.in_world() {
                    let due = next_report[i].get_or_insert(t + 1.0);
                    if t >= *due {
                        *due += 1.0;
                        b.send_bytes(true, 3, vec![u8::try_from(i).expect("few"); 30 + i]);
                    }
                }
                for (to, bytes) in b.tick(now) {
                    if !lossy.loses(i) {
                        to_server.push((b.addr, to, bytes));
                    }
                }
            }
        }
        pass += 1;
        t = SESSION_START + f64::from(pass) * SERVER_PASS;
    }
    Session {
        digest: digest.0,
        datagrams,
        bots,
        closed,
        lost: lossy.lost,
    }
}

/// The server's datagrams in the fixed session are the ones recorded when the session was
/// written, byte for byte and each in the same pass of the loop: how the connections are run may
/// change, what each client is sent and when may not. The session itself completes: every client
/// enters the world and takes its large messages whole, the silent one times out and the ended
/// one is told.
/// Behaviour: server.net.fixed-session-sends-the-same-datagrams
#[test]
fn a_fixed_session_sends_the_same_datagrams_in_the_same_passes() {
    let s = scripted_session();
    assert!(s.lost > 0, "the lossy link lost datagrams");
    for (i, b) in s.bots.iter().enumerate() {
        let whole = |world: bool, len: usize| {
            b.inbox
                .iter()
                .any(|r| r.world == world && r.bytes.len() == len)
        };
        assert!(
            whole(false, 40_000),
            "client {i} took the login's large message"
        );
        assert!(
            whole(true, 70_000),
            "client {i} took the world's large message"
        );
        let answers = b.inbox.iter().filter(|r| r.world && r.queue == 3).count();
        assert!(
            answers > 10,
            "client {i} was sent the world's messages: {answers}"
        );
        match i {
            ENDED => assert!(
                !b.in_world() && b.logged_in(),
                "the ended client left the world"
            ),
            SILENT => {}
            _ => assert!(b.in_world(), "client {i} is in the world"),
        }
    }
    assert_eq!(
        s.closed,
        [
            CloseReason::Closed,
            CloseReason::TimedOut,
            CloseReason::TimedOut
        ],
        "the ended world connection, then the silent client's two"
    );
    assert_eq!(
        (s.digest, s.datagrams),
        RECORDED_SESSION,
        "the server's datagrams differ from the recorded session's"
    );
}
