//! The login's end, the login server's address, a server's errors and losing the server, against a
//! server double and hand-made server packets, on a virtual clock.

use std::net::{Ipv4Addr, SocketAddr};

use auberean_client_net::logon::{Logon, KEY_LEN};
use auberean_client_net::session::{received_rate, Event, Session, Settings};
use auberean_client_net::LocalTime;
use auberean_protocol::login::Empty;
use auberean_protocol::ServerMessage;
use auberean_testkit::double::script::{ConnSpec, Event as Row, Msg, Script};
use auberean_testkit::double::ServerDouble;
use dereth_client_net::linkstatus::{LinkStatusAverages, Snapshot};
use dereth_transport::conn::{ConnectRequest, NetErrorCode};
use dereth_transport::wire::packet::netpacket_flags;
use dereth_transport::wire::{OutPacket, PacketFlags, ProtoHeader};
use dereth_transport::CryptoSystem;

/// The login server's id in its headers, and its iteration.
const LOGIN_ID: u16 = 0xF1;
const LOGIN_ITERATION: u16 = 5;

fn logon() -> Logon {
    Logon {
        account: b"tester".to_vec(),
        ticket: b"t".to_vec(),
        timestamp: 1_485_419_089,
        session_key: [0; KEY_LEN],
    }
}

/// A login connection that sends one message and then, from `silent` on, nothing more.
fn script(silent: Option<f64>) -> Script {
    let mut events = vec![Row::Msg {
        t: 0.2,
        msg: Msg {
            conn: 0,
            c2s: false,
            queue: 1,
            blob_id: 0x0300_0000_0000_0001,
            mtype: ServerMessage::DownloadEnds(Empty).message_type(),
            raw: ServerMessage::DownloadEnds(Empty).encode(),
        },
    }];
    if let Some(t) = silent {
        events.push(Row::Silent { t, conn: 0 });
    }
    let s = Script {
        meta: Vec::new(),
        conns: vec![ConnSpec {
            n: 0,
            login: true,
            server_port: 9000,
            server_id: LOGIN_ID,
            iteration: LOGIN_ITERATION,
            net_id: 1,
            server_time: 1.25e8,
            t_request: 0.0,
            t_connect: 0.05,
            switched: false,
        }],
        events,
    };
    s.check().expect("a well-formed script");
    s
}

struct Run {
    session: Session,
    double: ServerDouble,
    events: Vec<(f64, Event)>,
    sent: Vec<(f64, SocketAddr, Vec<u8>)>,
    now: LocalTime,
    client: SocketAddr,
}

impl Run {
    fn new(silent: Option<f64>) -> Self {
        let double = ServerDouble::new(script(silent), Ipv4Addr::LOCALHOST, 1.0, 7);
        let settings = Settings {
            login_server: double.login_addr(),
            logon: logon(),
        };
        Self {
            session: Session::new(settings),
            double,
            events: Vec::new(),
            sent: Vec::new(),
            now: LocalTime(1.0),
            client: "127.0.0.1:50000".parse().expect("an address"),
        }
    }

    fn step(&mut self) {
        self.now = LocalTime(self.now.0 + 0.05);
        self.session.tick(self.now);
        while let Some((to, bytes)) = self.session.poll_transmit() {
            self.double.datagram(to, self.client, &bytes, self.now);
            self.sent.push((self.now.0, to, bytes));
        }
        for d in self.double.tick(self.now) {
            self.session.handle_datagram(d.from, &d.bytes, self.now);
        }
        self.drain();
    }

    fn drain(&mut self) {
        while let Some(e) = self.session.poll_event() {
            self.events.push((self.now.0, e));
        }
        while let Some((to, bytes)) = self.session.poll_transmit() {
            self.sent.push((self.now.0, to, bytes));
        }
    }

    fn run_until(&mut self, t: f64) {
        while self.now.0 < t {
            self.step();
        }
    }

    /// A packet from the login server, as it arrives now.
    fn hear_login_server(&mut self, bytes: &[u8]) {
        let from = self.double.login_addr();
        self.session.handle_datagram(from, bytes, self.now);
        self.drain();
    }

    fn count(&self, f: impl Fn(&Event) -> bool) -> usize {
        self.events.iter().filter(|(_, e)| f(e)).count()
    }
}

/// An unsequenced packet carrying one section, from server `rec_id`.
fn control(rec_id: u16, iteration: u16, mask: u32, body: Vec<u8>) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        rec_id,
        iteration,
        ..ProtoHeader::default()
    });
    p.add_optional_header(mask, body).expect("one section");
    // Sent in the clear with no sequence, as a section that is not disposable may be read.
    p.flags &= !netpacket_flags::NEEDS_ENCRYPTION;
    p.serialize(None).expect("a packet")
}

/// The login server's connection request, with `seed` keying its own stream.
fn connect_request(seed: u32) -> Vec<u8> {
    let request = ConnectRequest {
        server_time: 1.25e8,
        cookie: 0x0102_0304_0506_0708,
        net_id: 1,
        outgoing_seed: seed,
        incoming_seed: 0x0BAD_F00D,
    };
    control(
        LOGIN_ID,
        LOGIN_ITERATION,
        PacketFlags::CONNECT_REQUEST,
        request.to_bytes().to_vec(),
    )
}

/// The login server's first sequenced packet, carrying one section, keyed by the first value of
/// the stream `seed` starts.
fn first_sequenced(seed: u32, mask: u32, body: Vec<u8>) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: 2,
        rec_id: LOGIN_ID,
        iteration: LOGIN_ITERATION,
        ..ProtoHeader::default()
    });
    p.add_optional_header(mask, body).expect("one section");
    let key = CryptoSystem::new(seed).next();
    p.serialize(Some(key)).expect("a packet")
}

fn flags_of(datagram: &[u8]) -> u32 {
    u32::from_le_bytes([datagram[4], datagram[5], datagram[6], datagram[7]])
}

/// A connection request of a newer iteration, from the server a session is talking to: that server
/// has started over, and the session is over, once; one of the same iteration changes nothing.
///
/// Behaviour: session.connection.a-server-that-starts-over-ends-the-session
#[test]
fn a_server_that_starts_over_ends_the_session() {
    let request = |iteration: u16| {
        let r = ConnectRequest {
            server_time: 1.25e8,
            cookie: 0x0102_0304_0506_0708,
            net_id: 1,
            outgoing_seed: 7,
            incoming_seed: 0x0BAD_F00D,
        };
        control(
            LOGIN_ID,
            iteration,
            PacketFlags::CONNECT_REQUEST,
            r.to_bytes().to_vec(),
        )
    };
    let mut run = Run::new(None);
    run.run_until(3.0);
    assert!(run.session.login_connected());
    run.hear_login_server(&request(LOGIN_ITERATION));
    run.drain();
    assert_eq!(
        run.count(|e| *e == Event::ServerDied),
        0,
        "the same iteration"
    );
    run.hear_login_server(&request(LOGIN_ITERATION + 1));
    run.drain();
    assert_eq!(
        run.count(|e| *e == Event::ServerDied),
        1,
        "a newer iteration"
    );
}

/// A login server that falls silent is given 140 seconds; then the client lets it go, logs off
/// and the session is over: the server is dead, and nothing more is sent (the dead server's
/// connection is gone before the log-off, so not even a goodbye goes to it).
///
/// Behaviour: session.connection.a-silent-server-is-dead-after-140-seconds
#[test]
fn a_silent_login_server_is_dead_after_140_seconds_and_the_client_logs_off() {
    let mut run = Run::new(Some(0.5));
    run.run_until(140.0);
    assert_eq!(run.count(|e| *e == Event::LoginConnected), 1);
    assert_eq!(
        run.count(|e| *e == Event::ServerDied),
        0,
        "not yet: 140 seconds have not passed since the server was last heard"
    );
    run.run_until(143.0);
    let died: Vec<f64> = run
        .events
        .iter()
        .filter(|(_, e)| *e == Event::ServerDied)
        .map(|(t, _)| *t)
        .collect();
    assert_eq!(died.len(), 1, "the server dies once");
    assert!(run.events.iter().any(|(_, e)| *e
        == Event::ConnectionLost {
            server: LOGIN_ID,
            login: true
        }));
    run.run_until(150.0);
    assert!(
        run.sent.iter().all(|(t, _, _)| *t < died[0]),
        "nothing is sent once the server is dead"
    );
    assert!(!run
        .sent
        .iter()
        .any(|(_, _, b)| flags_of(b) & PacketFlags::DISCONNECT != 0));
    assert!(!run.session.login_connected());
    assert_eq!(run.count(|e| *e == Event::ServerDied), 1);
}

/// A net error once the login is over logs the client off (its goodbye sent) and the server is
/// dead; one while the login is still going ends the login with that error, and no login goes
/// out again.
///
/// Behaviour: session.connection.a-net-error-ends-the-login-or-logs-off
#[test]
fn a_net_error_ends_a_login_still_going_and_once_logged_in_logs_off() {
    // Logged in.
    let mut run = Run::new(None);
    run.run_until(3.0);
    assert!(run.session.login_connected());
    let before = run.sent.len();
    let error = control(
        LOGIN_ID,
        LOGIN_ITERATION,
        PacketFlags::NET_ERROR,
        NetErrorCode::ServerClosedConnection.pack().to_vec(),
    );
    run.hear_login_server(&error);
    assert_eq!(run.count(|e| *e == Event::ServerDied), 1);
    assert_eq!(run.count(|e| matches!(e, Event::LoginFailed(_))), 0);
    assert!(run.sent[before..]
        .iter()
        .any(|(_, _, b)| flags_of(b) & PacketFlags::DISCONNECT != 0));
    let after = run.sent.len();
    run.run_until(6.0);
    assert_eq!(run.sent.len(), after, "nothing more is sent");

    // Still logging in: the server has not answered.
    let server: SocketAddr = "127.0.0.1:9000".parse().expect("an address");
    let mut s = Session::new(Settings {
        login_server: server,
        logon: logon(),
    });
    s.tick(LocalTime(1.0));
    assert_eq!(s.poll_transmit().map(|(to, _)| to), Some(server));
    let error = control(
        0,
        0,
        PacketFlags::NET_ERROR,
        NetErrorCode::ServerFull.pack().to_vec(),
    );
    s.handle_datagram(server, &error, LocalTime(1.1));
    let events: Vec<Event> = std::iter::from_fn(|| s.poll_event()).collect();
    assert_eq!(events, vec![Event::LoginFailed(NetErrorCode::ServerFull)]);
    for i in 0..10 {
        s.tick(LocalTime(1.5 + f64::from(i)));
    }
    assert!(s.poll_transmit().is_none(), "no login goes out again");
    // A connection coming up after the failed login does not make it a login connected.
    let seed = 0x1234_5678;
    s.handle_datagram(server, &connect_request(seed), LocalTime(12.0));
    let sync = first_sequenced(
        seed,
        PacketFlags::TIME_SYNC,
        1.25e8f64.to_le_bytes().to_vec(),
    );
    s.handle_datagram(server, &sync, LocalTime(12.1));
    assert_eq!(s.stats().rejected, 0);
    assert!(std::iter::from_fn(|| s.poll_event()).all(|e| e != Event::LoginConnected));
}

/// The login server letting its connection go with an error is told, with the error; letting it
/// go with no error is not.
///
/// Behaviour: session.connection.a-servers-goodbye-with-an-error-is-told
#[test]
fn the_login_servers_goodbye_with_an_error_is_told_and_one_without_is_not() {
    let goodbye = |error: NetErrorCode| {
        let server: SocketAddr = "127.0.0.1:9000".parse().expect("an address");
        let mut s = Session::new(Settings {
            login_server: server,
            logon: logon(),
        });
        s.tick(LocalTime(1.0));
        let seed = 0x1234_5678;
        s.handle_datagram(server, &connect_request(seed), LocalTime(1.1));
        let bytes = first_sequenced(
            seed,
            PacketFlags::NET_ERROR_DISCONNECT,
            error.pack().to_vec(),
        );
        s.handle_datagram(server, &bytes, LocalTime(1.2));
        assert_eq!(s.stats().rejected, 0, "the goodbye is taken");
        std::iter::from_fn(|| s.poll_event())
            .filter(|e| matches!(e, Event::ConnectionError { .. }))
            .collect::<Vec<Event>>()
    };
    assert_eq!(goodbye(NetErrorCode::None), Vec::<Event>::new());
    assert_eq!(
        goodbye(NetErrorCode::PlayerAlreadyLoggedOn),
        vec![Event::ConnectionError {
            server: LOGIN_ID,
            login: true,
            error: Some(NetErrorCode::PlayerAlreadyLoggedOn),
        }]
    );
}

/// The login server's address from a server moves the login: the next login goes there.
///
/// Behaviour: session.login.a-login-server-address-moves-where-the-login-goes
#[test]
fn a_login_server_address_moves_where_the_next_login_goes() {
    let first: SocketAddr = "127.0.0.1:9000".parse().expect("an address");
    let mut s = Session::new(Settings {
        login_server: first,
        logon: logon(),
    });
    s.tick(LocalTime(1.0));
    assert_eq!(s.poll_transmit().map(|(to, _)| to), Some(first));
    // The family (2), the port in network order, the address, eight bytes of padding.
    let mut addr = 2u16.to_le_bytes().to_vec();
    addr.extend_from_slice(&9010u16.to_be_bytes());
    addr.extend_from_slice(&[127, 0, 0, 2]);
    addr.extend_from_slice(&[0; 8]);
    let moved = control(0, 0, PacketFlags::LOGON_SERVER_ADDR, addr);
    s.handle_datagram(first, &moved, LocalTime(1.5));
    s.tick(LocalTime(2.5));
    assert!(s.poll_transmit().is_none(), "two seconds between logins");
    s.tick(LocalTime(3.1));
    let to = s.poll_transmit().map(|(to, _)| to);
    assert_eq!(to, "127.0.0.2:9010".parse().ok());
}

/// The rate received is the bytes the last two measures counted over the time they spanned, and
/// 0 before any time has passed; the first measure's time counts as one second.
///
/// Behaviour: session.link.the-rate-received-is-the-last-two-measures
#[test]
fn the_rate_received_is_the_last_two_measures_bytes_over_their_time() {
    let mut l = LinkStatusAverages::default();
    assert!(received_rate(&l).abs() < f64::EPSILON, "nothing measured");
    let measure = |bytes: u32| Snapshot {
        bytes_received: bytes,
        snapshot_duration: 2.0,
        ..Snapshot::default()
    };
    l.add_snapshot(&measure(1000), 2.0);
    assert!(
        (received_rate(&l) - 1000.0).abs() < 1e-9,
        "the first: one second"
    );
    l.add_snapshot(&measure(3000), 4.0);
    assert!((received_rate(&l) - 4000.0 / 3.0).abs() < 1e-9);
    l.add_snapshot(&measure(0), 6.0);
    assert!(
        (received_rate(&l) - 750.0).abs() < 1e-9,
        "the oldest measure has left: 3000 bytes over 4 seconds"
    );
}

/// A login no server answers is sent twenty times, two seconds apart, and then fails once, as
/// the server timed out; it is never told connected.
///
/// Behaviour: session.login.an-unanswered-login-fails-once
#[test]
fn an_unanswered_login_is_sent_twenty_times_and_fails_once() {
    let server: SocketAddr = "127.0.0.1:9000".parse().expect("an address");
    let mut s = Session::new(Settings {
        login_server: server,
        logon: logon(),
    });
    let mut sent = 0;
    let mut events = Vec::new();
    for i in 0..200 {
        s.tick(LocalTime(1.0 + f64::from(i) * 0.5));
        while s.poll_transmit().is_some() {
            sent += 1;
        }
        events.extend(std::iter::from_fn(|| s.poll_event()));
    }
    assert_eq!(sent, 20);
    assert_eq!(
        events,
        vec![Event::LoginFailed(NetErrorCode::ClientTimedOutServer)]
    );
}
