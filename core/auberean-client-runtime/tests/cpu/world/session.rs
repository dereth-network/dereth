//! The session's end as the client keeps it, and the connection's measure it reports, against a
//! server double on a virtual clock.

use std::net::{Ipv4Addr, SocketAddr};

use auberean_client_net::LocalTime;
use auberean_client_runtime::{Client, ClientData, ClientSettings, SessionEnd};
use auberean_protocol::login::{Empty, PlayerObject};
use auberean_protocol::physics::CreateObject;
use auberean_protocol::quality::PrivateQuality;
use auberean_protocol::ServerMessage;
use auberean_testkit::double::script::{ConnSpec, Event as Row, Msg, Script};
use auberean_testkit::double::ServerDouble;

/// A login connection that sends a message every half second until `until`, then `last` (the
/// server ending the connection, or falling silent).
fn script(until: f64, last: Row) -> Script {
    let mut events = Vec::new();
    let mut t = 0.2;
    let mut n = 1;
    while t < until {
        events.push(Row::Msg {
            t,
            msg: Msg {
                conn: 0,
                c2s: false,
                queue: 1,
                blob_id: 0x0300_0000_0000_0000 | n,
                mtype: ServerMessage::DownloadEnds(Empty).message_type(),
                raw: ServerMessage::DownloadEnds(Empty).encode(),
            },
        });
        t += 0.5;
        n += 1;
    }
    events.push(last);
    let s = Script {
        meta: Vec::new(),
        conns: vec![ConnSpec {
            n: 0,
            login: true,
            server_port: 9000,
            server_id: 0xF1,
            iteration: 5,
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
    client: Client,
    double: ServerDouble,
    now: LocalTime,
    me: SocketAddr,
}

impl Run {
    fn new(script: Script) -> Self {
        let double = ServerDouble::new(script, Ipv4Addr::LOCALHOST, 1.0, 7);
        let client = Client::new(
            ClientSettings {
                login_server: double.login_addr(),
                account: b"tester".to_vec(),
                ticket: b"t".to_vec(),
                timestamp: 1_485_419_089,
                session_key: [0; 64],
                script_sends: false,
            },
            ClientData::default(),
        );
        Self {
            client,
            double,
            now: LocalTime(1.0),
            me: "127.0.0.1:50000".parse().expect("an address"),
        }
    }

    /// A frame of a twentieth of a second.
    fn step(&mut self) {
        self.now = LocalTime(self.now.0 + 0.05);
        for d in self.double.tick(self.now) {
            self.client.handle_datagram(d.from, &d.bytes, self.now);
        }
        self.client.frame(self.now);
        while let Some((to, bytes)) = self.client.poll_transmit() {
            self.double.datagram(to, self.me, &bytes, self.now);
        }
    }

    /// Frames until the client's clock passes `t` (the script's time, a second behind it), or
    /// `done`.
    fn until(&mut self, t: f64, mut done: impl FnMut(&Client) -> bool) -> bool {
        while self.now.0 < t + 1.0 {
            if done(&self.client) {
                return true;
            }
            self.step();
        }
        done(&self.client)
    }
}

/// A private message of a type the client does not know, coming after the one stamped next to it,
/// comes from the network through to the private stream and lets that one take effect.
///
/// Behaviour: model.events.a-private-message-that-does-not-read-holds-nothing-back
#[test]
fn a_private_message_the_client_cannot_read_holds_up_nothing_after_it() {
    const PLAYER: u64 = 0x2130_0000_0000_0001;
    const QUALITY: u32 = 280;
    let msg = |t: f64, kind: u64, stamp: u64, raw: Vec<u8>| Row::Msg {
        t,
        msg: Msg {
            conn: 0,
            c2s: false,
            queue: 1,
            blob_id: (kind << 56) | (stamp << 32) | (0x10 * stamp),
            mtype: u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
            raw,
        },
    };
    let update = |value| {
        ServerMessage::UpdateInt(PrivateQuality {
            quality: QUALITY,
            value,
        })
        .encode()
    };
    let made = CreateObject {
        id: PLAYER,
        ..CreateObject::default()
    };
    let end = Row::End {
        t: 60.0,
        conn: 0,
        c2s: false,
    };
    let mut s = script(0.0, end.clone());
    s.events = vec![
        msg(
            0.2,
            3,
            0,
            ServerMessage::PlayerObject(PlayerObject {
                id: PLAYER,
                region: 1,
            })
            .encode(),
        ),
        msg(0.3, 3, 0, ServerMessage::CreateObject(made).encode()),
        msg(0.4, 1, 1, update(1)),
        msg(0.5, 1, 3, update(3)),
        msg(0.6, 1, 2, 0x0001_FFF1u32.to_le_bytes().to_vec()),
        end,
    ];
    s.check().expect("a well-formed script");
    let mut r = Run::new(s);
    r.until(3.0, |_| false);
    assert_eq!(r.client.int_quality(PLAYER, QUALITY), Some(3));
}

/// The server ending the login connection (the client answers its goodbye) ends the session: the
/// server is gone, and nothing failed before it.
///
/// Behaviour: script.model.a-server-gone-ends-the-session
#[test]
fn a_server_ending_the_connection_ends_the_session() {
    let mut r = Run::new(script(
        2.0,
        Row::End {
            t: 3.0,
            conn: 0,
            c2s: false,
        },
    ));
    r.until(2.5, |_| false);
    assert_eq!(
        r.client.session_end(),
        None,
        "going on while the server is there"
    );
    assert!(
        r.until(10.0, |c| c.session_end().is_some()),
        "the session ended"
    );
    assert_eq!(
        r.client.session_end(),
        Some(SessionEnd {
            connect_failed: None,
            login_failed: false,
            server_died: true,
        })
    );
}

/// A login server that falls silent ends the session 140 seconds after it was last heard, not
/// before.
///
/// Behaviour: script.model.a-silent-server-is-gone-after-140-seconds
#[test]
fn a_silent_server_ends_the_session_after_140_seconds() {
    let mut r = Run::new(script(2.0, Row::Silent { t: 2.0, conn: 0 }));
    r.until(2.0, |_| false);
    assert!(!r.until(140.0, |c| c.session_end().is_some()), "not yet");
    assert!(
        r.until(144.0, |c| c.session_end().is_some_and(|e| e.server_died)),
        "the server is gone"
    );
}

/// The connection's measure carries the rate received over the last two measures (their bytes
/// over their time) and the packets the last one counted.
///
/// Behaviour: script.model.the-connections-measure-carries-the-rate-over-two-measures
#[test]
fn the_connections_measure_carries_the_rate_received_over_two_measures() {
    let mut r = Run::new(script(
        30.0,
        Row::End {
            t: 31.0,
            conn: 0,
            c2s: false,
        },
    ));
    assert!(
        r.until(20.0, |c| c.heartbeat().is_some_and(|h| h.count >= 3)),
        "measured three times"
    );
    let h = r.client.heartbeat().expect("a measure");
    let link = r.client.session.link_status();
    let time = link.time_diffs.total();
    assert!(
        time > f64::from(h.span),
        "two measures span more than the last"
    );
    let rate = link.bytes_received.total() / time;
    assert!(
        (h.rate_in - rate).abs() < 1e-9,
        "{} against {rate}",
        h.rate_in
    );
    assert_eq!(h.packets_in, link.snapshot.pkts_received);
    assert!(h.packets_in > 0, "packets came");
}
