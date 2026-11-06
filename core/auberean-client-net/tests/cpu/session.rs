//! The session against a server double: the login, the character list's exchange, a referral into
//! the world, a hand-off to a second world server, a switch back to the first, leaving.

use std::net::{Ipv4Addr, SocketAddr};

use auberean_client_net::logon::{Logon, KEY_LEN};
use auberean_client_net::session::{Event, Incoming, Session, Settings};
use auberean_client_net::LocalTime;
use auberean_protocol::login::{DataHeld, DataQuery, Empty, PlayCharacter, PlayerObject};
use auberean_protocol::physics::HeadDirection;
use auberean_protocol::text::NarrowString;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_testkit::double::script::{ConnSpec, Event as Row, Msg, Script};
use auberean_testkit::double::ServerDouble;

const PLAYER: u64 = 0x2130_0000_0000_0001;

fn conn(n: usize, login: bool, port: u16, t_request: f64) -> ConnSpec {
    ConnSpec {
        n,
        login,
        server_port: port,
        server_id: if login {
            0xF1
        } else {
            0x0B + (port - 9002) / 2
        },
        iteration: if login { 5 } else { 2 },
        net_id: u32::try_from(n + 1).expect("small"),
        server_time: 1.25e8,
        t_request,
        t_connect: t_request + 0.05,
        switched: !login,
    }
}

fn s2c(t: f64, conn: usize, queue: u16, stamp: u64, m: &ServerMessage) -> Row {
    let raw = m.encode();
    Row::Msg {
        t,
        msg: Msg {
            conn,
            c2s: false,
            queue,
            blob_id: 0x0300_0000_0000_0000 | stamp,
            mtype: m.message_type(),
            raw,
        },
    }
}

fn c2s(t: f64, conn: usize, queue: u16, m: &ClientMessage) -> Row {
    let raw = m.encode();
    Row::Msg {
        t,
        msg: Msg {
            conn,
            c2s: true,
            queue,
            blob_id: 0,
            mtype: m.message_type(),
            raw,
        },
    }
}

fn play() -> ClientMessage {
    ClientMessage::PlayCharacter(PlayCharacter {
        character: PLAYER,
        account: NarrowString::from("tester"),
    })
}

/// The login connection, then world 9002, a hand-off to world 9004, a switch back to 9002 (a
/// referral to a server the client is still connected to), and the log-off's answer.
fn script() -> Script {
    let query = ServerMessage::DataQuery(DataQuery {
        region: 1,
        name_language: 1,
        languages: vec![1],
    });
    let held = ClientMessage::DataHeld(DataHeld {
        language: 0x8000_0001,
        files: Vec::new(),
    });
    let player = ServerMessage::PlayerObject(PlayerObject {
        id: PLAYER,
        region: 1,
    });
    let events = vec![
        s2c(0.20, 0, 5, 1, &query),
        c2s(0.25, 0, 5, &held),
        s2c(0.30, 0, 5, 2, &ServerMessage::DownloadEnds(Empty)),
        c2s(2.90, 0, 4, &play()),
        Row::Referral {
            t: 2.95,
            from: 0,
            to: Some(1),
        },
        s2c(3.10, 1, 1, 3, &player),
        Row::Referral {
            t: 10.0,
            from: 1,
            to: Some(2),
        },
        s2c(10.30, 2, 1, 4, &ServerMessage::DownloadEnds(Empty)),
        Row::Referral {
            t: 20.0,
            from: 2,
            to: Some(1),
        },
        Row::Switch { t: 20.2, conn: 1 },
        s2c(21.0, 1, 1, 5, &ServerMessage::DownloadEnds(Empty)),
        c2s(
            30.0,
            0,
            4,
            &ClientMessage::LogOff(auberean_protocol::login::CharacterRef { character: PLAYER }),
        ),
        s2c(30.5, 0, 1, 6, &ServerMessage::CharExitGame(Empty)),
    ];
    let s = Script {
        meta: Vec::new(),
        conns: vec![
            conn(0, true, 9000, 0.0),
            conn(1, false, 9002, 3.0),
            conn(2, false, 9004, 10.1),
        ],
        events,
    };
    s.check().expect("a well-formed script");
    s
}

struct Run {
    session: Session,
    double: ServerDouble,
    events: Vec<Event>,
    now: LocalTime,
    client: SocketAddr,
}

impl Run {
    fn new() -> Self {
        let double = ServerDouble::new(script(), Ipv4Addr::LOCALHOST, 1.0, 7);
        let settings = Settings {
            login_server: double.login_addr(),
            logon: Logon {
                account: b"tester".to_vec(),
                ticket: b"t".to_vec(),
                timestamp: 1_485_419_089,
                session_key: [0; KEY_LEN],
            },
        };
        Self {
            session: Session::new(settings),
            double,
            events: Vec::new(),
            now: LocalTime(1.0),
            client: "127.0.0.1:50000".parse().expect("an address"),
        }
    }

    /// One 50 ms step: the client's clock, its datagrams to the double, the double's back.
    fn step(&mut self) {
        self.now = LocalTime(self.now.0 + 0.05);
        self.session.tick(self.now);
        while let Some((to, bytes)) = self.session.poll_transmit() {
            self.double.datagram(to, self.client, &bytes, self.now);
        }
        for d in self.double.tick(self.now) {
            self.session.handle_datagram(d.from, &d.bytes, self.now);
        }
        while let Some(e) = self.session.poll_event() {
            if let Event::Message(r) = &e {
                if let Incoming::Message(m) = &r.incoming {
                    match &**m {
                        ServerMessage::DataQuery(_) => {
                            self.session.send(&ClientMessage::DataHeld(DataHeld {
                                language: 0x8000_0001,
                                files: Vec::new(),
                            }));
                        }
                        ServerMessage::DownloadEnds(_) if r.server == 0xF1 => {
                            self.session.send(&play());
                            self.session.enter_world();
                        }
                        ServerMessage::CharExitGame(_) => self.session.exit_world(),
                        _ => {}
                    }
                }
            }
            self.events.push(e);
        }
    }

    fn run_until(&mut self, t: f64) {
        while self.now.0 < t {
            self.step();
        }
    }
}

/// The session holds its login connection from the login to the end, follows each referral to a
/// new world server with a world login, takes a referral to a server it is still connected to as a
/// switch back to it, sends its world messages to the current world server, and drops the world
/// connections when the character leaves the world.
///
/// Behaviour: session.connection.the-login-connection-is-held-and-every-referral-followed
#[test]
fn the_login_connection_is_held_and_every_referral_is_followed() {
    let mut run = Run::new();
    run.run_until(25.0);
    assert!(run.session.login_connected(), "the login connection is up");
    assert_eq!(
        run.session.stats().world_logins,
        2,
        "two world logins, none for the switch back"
    );
    let switched: Vec<u16> = run
        .events
        .iter()
        .filter_map(|e| match e {
            Event::WorldSwitched { server } => Some(*server),
            _ => None,
        })
        .collect();
    assert_eq!(switched, vec![0x0B, 0x0C, 0x0B]);
    // A world message now goes to the first world server.
    run.session
        .send(&ClientMessage::TurnHead(HeadDirection { x: 0.5, y: 0.25 }));
    run.run_until(30.0);
    run.session.send(&ClientMessage::LogOff(
        auberean_protocol::login::CharacterRef { character: PLAYER },
    ));
    run.run_until(31.0);
    let report = run.double.report();
    assert_eq!(report.connected, vec![0, 1, 2]);
    assert_eq!(report.referrals, 3);
    assert!(report.received[1]
        .iter()
        .any(|m| m.raw[..4] == 0x100C8u32.to_le_bytes()));
    assert!(!report.received[2]
        .iter()
        .any(|m| m.raw[..4] == 0x100C8u32.to_le_bytes()));
    // The log-off went to the login server, and its answer left the world.
    assert!(report.received[0]
        .iter()
        .any(|m| m.raw[..4] == 0x10027u32.to_le_bytes()));
    run.run_until(33.0);
    assert!(
        run.session.login_connected(),
        "the login connection outlives the world"
    );
    assert_eq!(run.session.connections().count(), 1);
    assert_eq!(run.session.world_server(), 0);
    assert_eq!(run.session.stats().undecodable, 0);
}

/// The client receives on the control, events, logon and data queues, and reads them data first,
/// then events, control (thrown away) and logon (only the chat service's messages kept); its own
/// messages go out on their type's queue.
///
/// Behaviour: session.queues.data-before-events-and-the-logon-queue-keeps-only-chat
#[test]
fn the_client_reads_the_data_queue_first_and_keeps_only_chat_from_the_logon_queue() {
    let queues = auberean_client_net::queues::receive_queues();
    let ids: Vec<u16> = queues.registered.iter().map(|q| q.id()).collect();
    assert_eq!(ids, vec![2, 1, 4, 5]);
    let order: Vec<u16> = queues.drain_order.iter().map(|q| q.id()).collect();
    assert_eq!(order, vec![5, 1, 2, 4]);
    assert!(auberean_client_net::queues::is_kept(4, 0x100D3));
    assert!(!auberean_client_net::queues::is_kept(4, 0x10055));
    assert!(!auberean_client_net::queues::is_kept(2, 0x100D3));
    assert!(auberean_client_net::queues::is_kept(1, 0x10055));
    assert_eq!(auberean_client_net::queues::queue_of(&play()), 4);
    assert_eq!(
        auberean_client_net::queues::queue_of(&ClientMessage::TurnHead(HeadDirection {
            x: 0.0,
            y: 0.0
        })),
        3
    );
}

/// **Saying it keeps a world's overlays takes no stamp**: the data counter's next message carries the
/// stamp it would have had without it, and the two keep their own sequence numbers.
///
/// Behaviour: data.overlay.says-it-keeps-them
#[test]
fn saying_it_keeps_overlays_leaves_the_data_counter_where_it_was() {
    let mut run = Run::new();
    run.run_until(25.0);
    let says = ClientMessage::OverlayHeld(auberean_protocol::login::OverlayHeld::default());
    let held = ClientMessage::DataHeld(DataHeld {
        language: 0x8000_0001,
        files: Vec::new(),
    });
    assert!(run.session.send(&says));
    assert!(run.session.send(&held));
    run.run_until(30.0);
    let sent: Vec<(u32, u64, u64)> = run
        .double
        .report()
        .received
        .iter()
        .flatten()
        .filter(|m| m.blob_id >> 56 == 0x23)
        .map(|m| {
            let mtype = u32::from_le_bytes([m.raw[0], m.raw[1], m.raw[2], m.raw[3]]);
            (mtype, (m.blob_id >> 32) & 0xFFFF, m.blob_id & 0xFFFF_FFFF)
        })
        .collect();
    let [.., (said_type, said_stamp, said_seq), (held_type, held_stamp, held_seq)] = sent[..]
    else {
        panic!("both reached the server: {sent:?}");
    };
    assert_eq!(said_type, says.message_type());
    assert_eq!(held_type, held.message_type());
    // The session's login has already sent its data versions, at stamp 0.
    assert_eq!(said_stamp, 1, "the data counter's next stamp");
    assert_eq!(held_stamp, 1, "the same stamp, since the first took none");
    assert_ne!(said_seq, held_seq);
}

/// Each message the client sends is stamped from its counter: the data versions from the data
/// counter, from 0; the entry into the world and the log-off from the unordered counter, from 1;
/// world messages from the ordered counter, which starts again when the client enters the world.
/// The blob sequence is one for every connection and starts at 0.
///
/// Behaviour: session.send.every-message-stamped-from-its-counter
#[test]
fn every_message_is_stamped_from_its_counter_and_the_sequence_starts_at_zero() {
    let mut run = Run::new();
    run.run_until(25.0);
    let turn = |x: f32| ClientMessage::TurnHead(HeadDirection { x, y: 0.25 });
    let log_off =
        ClientMessage::LogOff(auberean_protocol::login::CharacterRef { character: PLAYER });
    assert!(
        run.session.send(&turn(0.5)),
        "queued on the world connection"
    );
    assert!(run.session.send(&turn(0.25)));
    run.run_until(30.0);
    assert!(run.session.send(&log_off));
    run.run_until(31.0);
    let mut stamps: Vec<(u32, u64, u64, u64)> = run
        .double
        .report()
        .received
        .iter()
        .flatten()
        .map(|m| {
            let mtype = u32::from_le_bytes([m.raw[0], m.raw[1], m.raw[2], m.raw[3]]);
            (
                mtype,
                m.blob_id >> 56,
                (m.blob_id >> 32) & 0xFFFF,
                m.blob_id & 0xFFFF_FFFF,
            )
        })
        .collect();
    stamps.sort_by_key(|s| s.3);
    let held = ClientMessage::DataHeld(DataHeld {
        language: 0x8000_0001,
        files: Vec::new(),
    });
    assert_eq!(
        stamps,
        vec![
            (held.message_type(), 0x23, 0, 0),
            (play().message_type(), 0x03, 1, 1),
            (turn(0.0).message_type(), 0x01, 1, 2),
            (turn(0.0).message_type(), 0x01, 2, 3),
            (log_off.message_type(), 0x03, 2, 4),
        ],
        "(type, ordering type, stamp, sequence) of every message, in sequence order"
    );
}
