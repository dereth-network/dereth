//! The player's body walked by the controls with no window: the SDK's driver runs the client's
//! frames on a fixed-step clock against a server double that puts the player in the human training
//! hall, and its world without a window moves the body through the hall's cells.
//! Fixture: the September 2013 install (the hall's cells, the human male); a hand-written login.

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::OnceLock;

use auberean_client_sdk::contract::control::{movement, Control};
use auberean_client_sdk::contract::event::SessionEvent;
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::login::{CharacterList, DataQuery, Empty, PlayerObject};
use auberean_client_sdk::protocol::physics::CreateObject;
use auberean_client_sdk::protocol::text::{NarrowString, WideString};
use auberean_client_sdk::protocol::types::{
    CharacterIdentity, PhysicsDesc, Placement, VisualDesc, WeenieDesc,
};
use auberean_client_sdk::protocol::{ClientMessage, ServerMessage};
use auberean_client_sdk::sim::{Walk, WorldData};
use auberean_client_sdk::{
    install, Action, ClientData, ClientSettings, Driver, FixedStep, GameEvent, Port,
};
use auberean_primitives::{Frame, Quat, Vec3};
use auberean_testkit::double::script::{ConnSpec, Event, Msg, Script};
use auberean_testkit::double::ServerDouble;

pub(crate) const PLAYER: u64 = 0x2130_0000_0000_0001;
/// A creature the server puts in the hall.
const CREATURE: u64 = 0x8130_1B05_CC00_0200;
/// The hall's cell the 2017 servers kept a hall's players in, and a place on its floor.
const HALL_CELL: u32 = 0x0298_0109;
pub(crate) const HALL_AT: Vec3 = Vec3 {
    x: 59.0,
    y: 240.0,
    z: -44.6,
};
/// The human male.
const HUMAN_MALE: u32 = 0x1F00_0023;
/// Unordered, out of the world (the login connection's) and in it.
const LOGIN: u64 = 0x2300_0000_0000_0000;
const WORLD: u64 = 0x0300_0000_0000_0000;
/// A frame: a sixtieth of a second.
const FRAME: f64 = 1.0 / 60.0;

/// The install's world and the client's data (its files, no script library), opened once.
pub(crate) fn data() -> &'static (WorldData, ClientData) {
    static DATA: OnceLock<(WorldData, ClientData)> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = auberean_dat::testing::require_target_install();
        let world = install::world_data(&dir).expect("the install's world");
        let mut data = install::client_data(&dir, |_| {}).expect("the install's data");
        data.scripts = None;
        (world, data)
    })
}

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

fn s2c(t: f64, conn: usize, queue: u16, blob_id: u64, m: &ServerMessage) -> Event {
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

/// What the servers say of a player's object, as the 2017 servers sent it: its script package and
/// entity template, and its physics type (the low and high words).
fn player_weenie() -> WeenieDesc {
    let mut w = WeenieDesc {
        mask: WeenieDesc::PACKAGE | WeenieDesc::ENTITY,
        package_id: 0x37F,
        entity_did: 0x4700_0530,
        ..WeenieDesc::default()
    };
    for (bit, word) in [(0x8000, 0x0480_0020), (0x1_0000, 0)] {
        let i = WeenieDesc::NUMBER_BITS
            .iter()
            .position(|b| *b == bit)
            .expect("a physics type word");
        w.mask |= bit;
        w.numbers[i] = word;
    }
    w
}

/// A game login whose character, a human male, is put in the hall at `at`, turned by `rotation`.
pub(crate) fn script_at(at: Vec3, rotation: Quat) -> Script {
    script_with(at, rotation, None)
}

/// What the 2017 servers said a creature is, and what it passes through: its physics type (bits 5
/// and 22) and its movement mask, the low and high words of each.
fn creature_weenie() -> WeenieDesc {
    let mut w = WeenieDesc::default();
    for (bit, word) in [
        (0x8000, 1 << 22 | 1 << 5),
        (0x1_0000, 0),
        (0x2_0000, 0x4220_0010),
        (0x4_0000, 0),
    ] {
        let i = WeenieDesc::NUMBER_BITS
            .iter()
            .position(|b| *b == bit)
            .expect("a physics type word");
        w.mask |= bit;
        w.numbers[i] = word;
    }
    w
}

/// A human male the server puts in the hall at `at`, turned by `rotation`: object `id`, of what
/// `weenie` says it is.
fn human_at(id: u64, weenie: WeenieDesc, at: Vec3, rotation: Quat) -> CreateObject {
    CreateObject {
        id,
        weenie,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION,
            instance_stamp: 1,
            visual_ordering_stamp: 8,
            position: Placement {
                cell: HALL_CELL,
                frame: Frame {
                    origin: at,
                    rotation,
                },
            },
            ..PhysicsDesc::default()
        },
    }
}

/// [`script_at`], with a creature (a human male) the server puts in the hall at `other` just after
/// the character, when there is one.
fn script_with(at: Vec3, rotation: Quat, other: Option<Vec3>) -> Script {
    let list = CharacterList {
        characters: vec![CharacterIdentity {
            id: PLAYER,
            name: WideString("Tester".encode_utf16().collect()),
            seconds_greyed_out: 0,
            visual: VisualDesc::default(),
        }],
        deleted: Vec::new(),
        status: 0,
        slots: 6,
        account: NarrowString::from("tester"),
        uses_chat_service: 1,
        has_expansion: 0,
        unknown: 0,
    };
    let player = human_at(PLAYER, player_weenie(), at, rotation);
    let mut events = vec![
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
        c2s(5.00, 1, 3, 0x100D1),
        c2s(40.0, 0, 4, 0x10027),
        s2c(40.5, 0, 1, WORLD | 5, &ServerMessage::CharExitGame(Empty)),
    ];
    if let Some(o) = other {
        let other = human_at(CREATURE, creature_weenie(), o, Quat::IDENTITY);
        let m = ServerMessage::CreateObject(other);
        events.insert(8, s2c(3.12, 1, 1, WORLD | 6, &m));
    }
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

/// The double playing `script` as the client's port, and the settings that log in to it.
pub(crate) fn double_port(script: Script) -> (DoublePort, ClientSettings) {
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
    (port, settings)
}

/// Where the body stood each frame (its cell and place), the places the client reported to the
/// server, with their times on the host's clock, and where the creature's body stood last.
#[derive(Debug, Default)]
struct Track {
    frames: Vec<(f64, u32, Vec3)>,
    reports: Vec<(f64, u32, Vec3)>,
    other: Option<Vec3>,
}

/// A run of `seconds`: the client logs in, enters the world with the first character listed, and
/// from `from` to `to` seconds holds the forward control, as a script or a plugin presses it.
fn run(script: Script, from: f64, to: f64, seconds: f64) -> Track {
    let (world, data) = data();
    let (mut port, settings) = double_port(script);
    let mut driver = Driver::connect(settings, data.clone());
    let mut walk = Walk::new(world);
    let mut clock = FixedStep::new(FRAME);
    let mut track = Track::default();
    let mut asked = false;
    while clock.now() < seconds {
        let t = clock.now() + FRAME;
        if (from..from + FRAME).contains(&t) {
            driver.act(Action::Control(Control::begin(movement::FORWARD)));
        }
        if (to..to + FRAME).contains(&t) {
            driver.act(Action::Control(Control::end(movement::FORWARD)));
        }
        driver.frame_walking(&mut clock, &mut port, &mut walk);
        for e in driver.take_events() {
            if let (GameEvent::Session(SessionEvent::Characters(list)), false) = (e, asked) {
                let character = list.characters.first().expect("a character").id;
                driver.act(Action::PlayCharacter { character });
                asked = true;
            }
        }
        if let Some(p) = walk.player() {
            let at = p.body.position;
            let o = at.frame.origin;
            track
                .frames
                .push((clock.now(), at.cell.0, Vec3::new(o.x, o.y, o.z)));
        }
    }
    track.other = walk.others.body(CREATURE).map(|b| {
        let o = b.position.frame.origin;
        Vec3::new(o.x, o.y, o.z)
    });
    track.reports = driver
        .sent()
        .iter()
        .filter_map(|s| match &s.message {
            // The client's clock runs a second ahead of the host's.
            ClientMessage::Position(p) => Some((s.at - 1.0, p.cell, p.origin)),
            _ => None,
        })
        .collect();
    track
}

/// Where the body stood at `t` seconds.
fn at(track: &Track, t: f64) -> (u32, Vec3) {
    let (_, cell, o) = track
        .frames
        .iter()
        .rfind(|(when, _, _)| *when <= t)
        .copied()
        .expect("a body by then");
    (cell, o)
}

fn flat(a: Vec3, b: Vec3) -> f32 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

/// With no window, the forward control held runs the player's body north through the hall's cells
/// on its floor (it never sinks through), out through the doorway into the next cell, and stops it
/// against a wall while the control is still held: in the last second it moves less than five
/// centimeters. The client's last report is where the body stopped.
///
/// Behaviour: session.walk.with-no-window-the-body-runs-to-a-wall-and-stops-there
#[test]
fn with_no_window_the_forward_control_runs_the_body_to_a_wall_and_stops_it_there() {
    let track = run(script_at(HALL_AT, Quat::IDENTITY), 8.0, 18.0, 19.0);
    let (_, start) = at(&track, 8.0);
    assert!(
        (start.z - HALL_AT.z).abs() < 1.0,
        "standing on the hall's floor: {start:?}"
    );
    let running: Vec<_> = track
        .frames
        .iter()
        .filter(|(t, _, _)| (8.0..18.0).contains(t))
        .collect();
    assert!(
        running.iter().all(|(_, _, o)| o.z > HALL_AT.z - 0.5),
        "never through the floor"
    );
    let cells: std::collections::BTreeSet<u32> = running.iter().map(|(_, c, _)| *c).collect();
    assert!(cells.len() >= 2, "through a doorway: {cells:08X?}");
    let (_, end) = at(&track, 18.0);
    let (_, second_before) = at(&track, 17.0);
    assert!(end.y - start.y > 10.0, "it ran north: {start:?} to {end:?}");
    assert!(
        flat(second_before, end) < 0.05,
        "at rest against a wall with the control held: {second_before:?} then {end:?}"
    );
    let last = track
        .reports
        .iter()
        .rfind(|(t, _, _)| *t < 18.9)
        .expect("a report");
    assert!(
        flat(last.2, end) < 0.05,
        "the last report is where the body stopped: {last:?} {end:?}"
    );
}

/// With no window, the forward control held runs the player's body west from the hall's starting
/// place onto the platform of steps (each lower than the body's step-up height): it climbs them,
/// standing more than half a meter over the floor, and crosses onto the platform.
///
/// Behaviour: session.walk.with-no-window-the-body-climbs-the-steps
#[test]
fn with_no_window_the_forward_control_runs_the_body_up_the_steps() {
    // Facing west: a quarter turn left about the vertical.
    let west = Quat {
        w: std::f32::consts::FRAC_1_SQRT_2,
        x: 0.0,
        y: 0.0,
        z: std::f32::consts::FRAC_1_SQRT_2,
    };
    let track = run(script_at(HALL_AT, west), 8.0, 11.0, 12.0);
    let (_, start) = at(&track, 8.0);
    let highest = track
        .frames
        .iter()
        .filter(|(t, _, _)| *t >= 8.0)
        .map(|(_, _, o)| o.z)
        .fold(f32::MIN, f32::max);
    assert!(
        highest - start.z > 0.5,
        "up on the platform: the floor at {}, the highest {highest}",
        start.z
    );
    let (_, end) = at(&track, 12.0);
    assert!(
        end.x < start.x - 5.0,
        "west onto the platform: {start:?} to {end:?}"
    );
}

/// With no window, a creature the server puts in the hall 3 m ahead of the player stands where it
/// was put and holds the player's body up: the forward control held runs the body up to it and
/// stops it there, more than a meter short of the creature's middle, for half a second, before it
/// slides off round the creature; it never runs through it, and in the two seconds held it goes
/// less than half as far as with the hall clear.
///
/// Behaviour: session.walk.with-no-window-a-creature-in-the-way-holds-the-body-up
#[test]
fn with_no_window_a_creature_in_the_way_holds_the_body_up() {
    let ahead = Vec3::new(HALL_AT.x, HALL_AT.y + 3.0, HALL_AT.z);
    let clear = run(script_at(HALL_AT, Quat::IDENTITY), 8.0, 10.0, 10.5);
    let track = run(
        script_with(HALL_AT, Quat::IDENTITY, Some(ahead)),
        8.0,
        10.0,
        10.5,
    );
    let creature = track.other.expect("the creature's body");
    assert!(
        flat(creature, ahead) < 0.05,
        "the creature stands where the server put it: {creature:?}"
    );
    let (_, start) = at(&track, 8.0);
    let (_, held_from) = at(&track, 8.5);
    let (_, held_to) = at(&track, 9.0);
    assert!(
        held_from.y - start.y > 1.0 && flat(held_from, held_to) < 0.05,
        "run up to the creature and held there: {held_from:?} then {held_to:?}"
    );
    assert!(
        flat(held_to, creature) > 1.0,
        "short of the creature's middle: {held_to:?}"
    );
    let nearest = track
        .frames
        .iter()
        .filter(|(t, _, _)| *t >= 8.0)
        .map(|(_, _, o)| flat(*o, creature))
        .fold(f32::MAX, f32::min);
    assert!(nearest > 0.9, "never through the creature: {nearest} m");
    let (_, end) = at(&track, 10.5);
    let (_, clear_end) = at(&clear, 10.5);
    assert!(
        flat(start, end) * 2.0 < flat(start, clear_end),
        "less than half as far as with the hall clear: {end:?}, clear {clear_end:?}"
    );
}
