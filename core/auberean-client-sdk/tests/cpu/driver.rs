//! The driver runs the client's frames against a server double: a fixed-step clock and a network
//! in memory, so a run is the same every time; and the frame's hook sees each event once and its
//! actions are carried out.

use std::any::Any;
use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr};

use auberean_client_sdk::contract::event::SessionEvent;
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::login::{CharacterList, DataQuery, Empty, PlayerObject};
use auberean_client_sdk::protocol::physics::CreateObject;
use auberean_client_sdk::protocol::text::{NarrowString, WideString};
use auberean_client_sdk::protocol::types::{CharacterIdentity, PhysicsDesc, Placement};
use auberean_client_sdk::protocol::ServerMessage;
use auberean_client_sdk::{
    Action, ClientData, ClientSettings, Driver, FixedStep, FrameHook, GameEvent, HookFrame, Phase,
    Port,
};
use auberean_testkit::double::script::{ConnSpec, Event, Msg, Script};
use auberean_testkit::double::ServerDouble;

pub(crate) const PLAYER: u64 = 0x2130_0000_0000_0001;
/// The message the client says the scene is drawn with.
const SCENE_DRAWN: u32 = 0x100D1;
/// How many frames a run is given: a minute of a sixtieth of a second each.
pub(crate) const FRAMES: usize = 3600;

fn conn(n: usize, login: bool, port: u16, t_request: f64) -> ConnSpec {
    ConnSpec {
        n,
        login,
        server_port: port,
        server_id: if login { 0xF1 } else { 0x0B },
        iteration: if login { 5 } else { 2 },
        net_id: u32::try_from(n + 1).expect("small"),
        server_time: 1.25e8,
        t_request,
        t_connect: t_request + 0.05,
        switched: !login,
    }
}

pub(crate) fn s2c(t: f64, conn: usize, queue: u16, blob_id: u64, m: &ServerMessage) -> Event {
    Event::Msg {
        t,
        msg: Msg {
            conn,
            c2s: false,
            queue,
            blob_id,
            mtype: m.message_type(),
            raw: m.encode(),
        },
    }
}

fn c2s(t: f64, conn: usize, queue: u16, mtype: u32) -> Event {
    Event::Msg {
        t,
        msg: Msg {
            conn,
            c2s: true,
            queue,
            blob_id: 0,
            mtype,
            raw: mtype.to_le_bytes().to_vec(),
        },
    }
}

/// Unordered out of the world (the login connection's), and unordered in the world.
const LOGIN: u64 = 0x2300_0000_0000_0000;
pub(crate) const WORLD: u64 = 0x0300_0000_0000_0000;

/// A game login: the data versions asked, the character list, the entry, the referral to the
/// world server, the player made there, and the scene drawn.
fn script() -> Script {
    script_with(Vec::new())
}

/// The game login with these steps after the player is made.
pub(crate) fn script_with(mut extra: Vec<Event>) -> Script {
    let list = CharacterList {
        characters: vec![CharacterIdentity {
            id: PLAYER,
            name: WideString("Tester".encode_utf16().collect()),
            seconds_greyed_out: 0,
            visual: Default::default(),
        }],
        deleted: Vec::new(),
        status: 0,
        slots: 6,
        account: NarrowString::from("tester"),
        uses_chat_service: 1,
        has_expansion: 0,
        unknown: 0,
    };
    let mut player = CreateObject {
        id: PLAYER,
        ..CreateObject::default()
    };
    player.physics = PhysicsDesc {
        bits: PhysicsDesc::POSITION,
        instance_stamp: 1,
        visual_ordering_stamp: 8,
        position: Placement {
            cell: 0x0298_0109,
            ..Placement::default()
        },
        ..PhysicsDesc::default()
    };
    let events = vec![
        s2c(
            0.20,
            0,
            5,
            LOGIN,
            &ServerMessage::DataQuery(DataQuery {
                region: 1,
                name_language: 1,
                languages: vec![1],
            }),
        ),
        c2s(0.25, 0, 5, 0x10038),
        s2c(0.30, 0, 5, LOGIN | 1, &ServerMessage::DownloadEnds(Empty)),
        s2c(0.31, 0, 1, LOGIN | 2, &ServerMessage::CharacterList(list)),
        c2s(2.90, 0, 4, 0x1002B),
        Event::Referral {
            t: 2.95,
            from: 0,
            to: Some(1),
        },
        s2c(
            3.10,
            1,
            1,
            WORLD | 3,
            &ServerMessage::PlayerObject(PlayerObject {
                id: PLAYER,
                region: 1,
            }),
        ),
        s2c(3.11, 1, 1, WORLD | 4, &ServerMessage::CreateObject(player)),
    ];
    let mut events = events;
    events.append(&mut extra);
    events.push(c2s(5.00, 1, 3, SCENE_DRAWN));
    let s = Script {
        meta: Vec::new(),
        conns: vec![conn(0, true, 9000, 0.0), conn(1, false, 9002, 3.0)],
        events,
    };
    s.check().expect("a well-formed script");
    s
}

/// The double as the client's port: what the double sends in a frame is read in that frame.
pub(crate) struct DoublePort {
    double: ServerDouble,
    me: SocketAddr,
    inbox: VecDeque<(SocketAddr, Vec<u8>)>,
    ticked: Option<f64>,
}

impl Port for DoublePort {
    fn receive(&mut self, now: LocalTime) -> Option<(SocketAddr, Vec<u8>)> {
        if self.ticked != Some(now.0) {
            self.ticked = Some(now.0);
            self.inbox
                .extend(self.double.tick(now).into_iter().map(|d| (d.from, d.bytes)));
        }
        self.inbox.pop_front()
    }

    fn send(&mut self, to: SocketAddr, bytes: &[u8], now: LocalTime) {
        self.double.datagram(to, self.me, bytes, now);
    }
}

/// A driver with a new client, and the double it logs in to as its port.
pub(crate) fn driven() -> (Driver, DoublePort) {
    driven_by(script())
}

/// A driver with a new client, and a double playing `script` as its port.
pub(crate) fn driven_by(script: Script) -> (Driver, DoublePort) {
    let double = ServerDouble::new(script, Ipv4Addr::LOCALHOST, 1.0, 7);
    let settings = ClientSettings {
        login_server: double.login_addr(),
        account: b"tester".to_vec(),
        ticket: b"t".to_vec(),
        timestamp: 0,
        session_key: [0; 64],
        script_sends: false,
    };
    let port = DoublePort {
        double,
        me: SocketAddr::from(([127, 0, 0, 1], 50_000)),
        inbox: VecDeque::new(),
        ticked: None,
    };
    (Driver::connect(settings, ClientData::default()), port)
}

/// What one run did: the frame it entered the world on, and the types of the messages it sent with
/// the frame each went in.
#[derive(Debug, PartialEq)]
struct Run {
    in_world_at: Option<usize>,
    sent: Vec<(u32, u64)>,
}

/// The client driven on a sixtieth-of-a-second fixed step, entering the world with the first
/// character listed, until it is in the world or the minute is out.
fn run() -> Run {
    let (mut driver, mut port) = driven();
    let mut clock = FixedStep::new(1.0 / 60.0);
    let mut in_world_at = None;
    let mut asked = false;
    for n in 0..FRAMES {
        driver.frame(&mut clock, &mut port);
        for e in driver.take_events() {
            match e {
                GameEvent::Session(SessionEvent::Characters(list)) if !asked => {
                    let character = list.characters.first().expect("a character").id;
                    driver.act(Action::PlayCharacter { character });
                    asked = true;
                }
                GameEvent::Session(SessionEvent::Phase(Phase::World)) => in_world_at = Some(n),
                _ => {}
            }
        }
        if in_world_at.is_some() {
            break;
        }
    }
    let sent = driver
        .sent()
        .iter()
        .map(|s| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let frame = (s.at * 60.0).round() as u64;
            (s.message.message_type(), frame)
        })
        .collect();
    Run { in_world_at, sent }
}

/// Driven on a fixed-step clock over a network in memory, the client logs in, enters the world
/// with the character the player picks from the list (its events alone tell it when), and says
/// the scene is drawn once; and a second run does exactly the same on the same frames.
///
/// Behaviour: session.driver.a-fixed-step-run-enters-the-world-the-same-every-time
#[test]
fn the_driver_on_a_fixed_step_clock_enters_the_world_the_same_way_every_run() {
    let first = run();
    assert!(first.in_world_at.is_some(), "in the world: {first:?}");
    let count = |t: u32| first.sent.iter().filter(|(m, _)| *m == t).count();
    assert_eq!(count(0x10038), 1, "the data versions, when asked");
    assert_eq!(count(0x1002B), 1, "the entry, once");
    assert_eq!(count(SCENE_DRAWN), 1, "the scene drawn, once");
    assert_eq!(run(), first, "a second run is the same");
}

/// What the recording hook saw and did.
#[derive(Debug, Default)]
struct Recorder {
    steps: usize,
    seen: Vec<GameEvent>,
    asked: bool,
}

impl FrameHook for Recorder {
    fn step(&mut self, frame: &mut HookFrame<'_>) {
        self.steps += 1;
        self.seen.extend(frame.events.iter().cloned());
        for e in frame.events {
            if let GameEvent::Session(SessionEvent::Characters(list)) = e {
                if !self.asked {
                    let character = list.characters.first().expect("a character").id;
                    frame.act(Action::PlayCharacter { character });
                    self.asked = true;
                }
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn recorder(driver: &Driver) -> &Recorder {
    driver
        .hook()
        .and_then(|h| h.as_any().downcast_ref::<Recorder>())
        .expect("the recorder installed")
}

/// A hook installed through the driver runs once a frame and is handed every event once, in the
/// order the host takes them, those from between frames (the host closing the session) in the
/// next frame; what it asks for is carried out: entering the world when the characters are
/// listed, which it alone asks for here.
///
/// Behaviour: session.driver.a-frame-hook-sees-each-event-once-and-its-actions-are-carried-out
#[test]
fn a_frame_hook_sees_each_event_once_in_order_and_its_actions_are_carried_out() {
    let (mut driver, mut port) = driven();
    let mut clock = FixedStep::new(1.0 / 60.0);
    assert!(driver.install_hook(Box::<Recorder>::default()).is_none());
    let mut taken = Vec::new();
    let mut frames = 0;
    while frames < FRAMES {
        driver.frame(&mut clock, &mut port);
        frames += 1;
        let events = driver.take_events();
        let in_world = events.contains(&GameEvent::Session(SessionEvent::Phase(Phase::World)));
        taken.extend(events);
        if in_world {
            break;
        }
    }
    assert!(
        taken.contains(&GameEvent::Session(SessionEvent::Phase(Phase::World))),
        "the hook's entry was carried out: {taken:?}"
    );
    let entries = driver
        .sent()
        .iter()
        .filter(|s| s.message.message_type() == 0x1002B)
        .count();
    assert_eq!(entries, 1, "the hook asked to enter once");
    // The host closes the session between frames; the hook hears of it in the next frame.
    driver.disconnect(&mut port);
    taken.extend(driver.take_events());
    driver.frame(&mut clock, &mut port);
    frames += 1;
    taken.extend(driver.take_events());
    assert_eq!(
        taken.last(),
        Some(&GameEvent::Session(SessionEvent::Phase(
            Phase::Disconnected
        )))
    );
    let hook = recorder(&driver);
    assert_eq!(hook.steps, frames, "one step a frame");
    assert_eq!(hook.seen, taken, "every event once, in order");
    // Taken out, the hook is no longer run.
    let out = driver.remove_hook().expect("the hook");
    driver.frame(&mut clock, &mut port);
    let steps = out
        .as_any()
        .downcast_ref::<Recorder>()
        .expect("the recorder")
        .steps;
    assert_eq!(steps, frames);
}
