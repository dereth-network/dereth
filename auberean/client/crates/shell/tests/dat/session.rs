//! A whole session through the shell: a server double plays the servers' side of a game login
//! (the character list, the world entry into the human training hall, the exit); the shell's host
//! carries its datagrams on a clock set by hand.
//! Fixture: the September 2013 install (the hall's cells, the human male); a hand-written login.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::DatFile;
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::login::{CharacterList, DataQuery, Empty, PlayerObject};
use auberean_client_sdk::protocol::physics::{CreateObject, ObjectPosition};
use auberean_client_sdk::protocol::text::{NarrowString, WideString};
use auberean_client_sdk::protocol::types::{
    CharacterIdentity, ObjectStamp, PhysicsDesc, Placement, VisualDesc,
};
use auberean_client_sdk::protocol::{ClientMessage, ServerMessage};
use auberean_client_sdk::runtime::data::{DataFiles, ENGLISH};
use auberean_client_sdk::ClientData;
use auberean_primitives::{Frame, Quat, Vec3};
use auberean_scene::seen::SeenCamera;
use auberean_scene::WorldData;
use auberean_shell::dev::DevLogin;
use auberean_shell::input::{Event, Key};
use auberean_shell::interface::{Game, Interface, Phase, Request};
use auberean_shell::{Host, Shell};
use auberean_testkit::double::script::{ConnSpec, Event as Step, Msg, Script};
use auberean_testkit::double::ServerDouble;

const PLAYER: u64 = 0x2130_0000_0000_0001;
/// The hall's cell the 2017 servers kept a hall's players in, and a place on its floor.
const HALL_CELL: u32 = 0x0298_0109;
const HALL_AT: Vec3 = Vec3 {
    x: 59.0,
    y: 240.0,
    z: -44.6,
};
/// The human male.
const HUMAN_MALE: u32 = 0x1F00_0023;

/// The world and the client's data, opened once.
pub(crate) fn data() -> &'static (WorldData, ClientData) {
    static DATA: OnceLock<(WorldData, ClientData)> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = auberean_dat::testing::require_target_install();
        let found = install_dats(&dir).expect("list the install");
        let open = |want: &dyn Fn(&DatName) -> bool| -> Option<DatFile> {
            let (_, p) = found.iter().find(|(n, _)| want(n))?;
            DatFile::open(p).ok()
        };
        let portal = open(&|n| *n == DatName::Portal).expect("a portal file");
        let cell = open(&|n| *n == DatName::Cell(1)).expect("a cell file");
        let ctx = auberean_assets::Context::from_portal(&portal).expect("the property list");
        let records = auberean_world::records::WorldRecords::new(
            portal,
            cell,
            open(&|n| *n == DatName::HighRes),
            ctx,
        );
        let world = WorldData::new(records).expect("the region");
        let portal = Arc::new(open(&|n| *n == DatName::Portal).expect("a portal file"));
        let data = ClientData {
            files: DataFiles {
                files: vec![portal],
                language: ENGLISH,
            },
            scripts: None,
        };
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

fn s2c(t: f64, conn: usize, queue: u16, blob_id: u64, m: &ServerMessage) -> Step {
    Step::Msg {
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

fn c2s(t: f64, conn: usize, queue: u16, mtype: u32) -> Step {
    Step::Msg {
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

const LOGIN: u64 = 0x2300_0000_0000_0000;
const WORLD: u64 = 0x0300_0000_0000_0000;

/// The login: a list with one character, the world entry into the hall as a human male, the
/// scene drawn, the log-off answered.
fn script() -> Script {
    script_at(HALL_CELL, HALL_AT, Quat::IDENTITY)
}

/// The login with the player put in `cell` at `at`, turned by `rotation`.
fn script_at(cell: u32, at: Vec3, rotation: Quat) -> Script {
    script_with(cell, at, rotation, &[])
}

/// The login with the player put in `cell` at `at`, turned by `rotation`, and `others` made just
/// after it.
fn script_with(cell: u32, at: Vec3, rotation: Quat, others: &[CreateObject]) -> Script {
    script_then(cell, at, rotation, others, &[])
}

/// The login of [`script_with`], then `later` messages at their times on the world connection.
fn script_then(
    cell: u32,
    at: Vec3,
    rotation: Quat,
    others: &[CreateObject],
    later: &[(f64, ServerMessage)],
) -> Script {
    script_until(cell, at, rotation, others, later, 20.0)
}

/// The login of [`script_then`], the character logging off at `end` seconds.
fn script_until(
    cell: u32,
    at: Vec3,
    rotation: Quat,
    others: &[CreateObject],
    later: &[(f64, ServerMessage)],
    end: f64,
) -> Script {
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
    let player = CreateObject {
        id: PLAYER,
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
                cell,
                frame: Frame {
                    origin: at,
                    rotation,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
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
        Step::Referral {
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
    for (i, o) in (0u64..).zip(others) {
        events.push(s2c(
            3.12,
            1,
            1,
            WORLD | (0x10 + i),
            &ServerMessage::CreateObject(o.clone()),
        ));
    }
    for (i, (t, m)) in (0u64..).zip(later) {
        events.push(s2c(*t, 1, 1, WORLD | (0x40 + i), m));
    }
    events.extend([
        c2s(5.00, 1, 3, 0x100D1),
        c2s(end, 0, 4, 0x10027),
        s2c(
            end + 0.5,
            0,
            1,
            WORLD | 5,
            &ServerMessage::CharExitGame(Empty),
        ),
    ]);
    let s = Script {
        meta: Vec::new(),
        conns: vec![conn(0, true, 9000, 0.0), conn(1, false, 9002, 3.0)],
        events,
    };
    s.check().expect("a well-formed script");
    s
}

/// The host: a clock set by hand, the datagrams carried to and from the double.
struct DoubleHost {
    time: f64,
    double: ServerDouble,
    inbox: VecDeque<(SocketAddr, Vec<u8>)>,
    /// The settings the player kept, if any.
    kept: Option<String>,
}

fn me() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 50_000))
}

impl Host for DoubleHost {
    fn now(&self) -> f64 {
        self.time
    }
    fn unix_time(&self) -> u32 {
        0
    }
    fn random(&mut self, out: &mut [u8]) {
        out.fill(7);
    }
    fn connect(&mut self, _server: SocketAddr) -> Result<(), String> {
        Ok(())
    }
    fn send(&mut self, to: SocketAddr, bytes: &[u8]) {
        let t = LocalTime(1.0 + self.time);
        self.double.datagram(to, me(), bytes, t);
    }
    fn receive(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        self.inbox.pop_front()
    }
    fn hold_pointer(&mut self, _held: bool) {}
    fn set_title(&mut self, _title: &str) {}
    fn load_settings(&self) -> Option<String> {
        self.kept.clone()
    }
    fn save_settings(&mut self, _text: &str) {}
}

/// Run the shell a frame of 1/60 s: the clock on, the double's datagrams in, the frame.
fn step(shell: &mut Shell<'_, DoubleHost>) -> bool {
    let h = &mut shell.host;
    h.time += 1.0 / 60.0;
    let t = LocalTime(1.0 + h.time);
    for d in h.double.tick(t) {
        h.inbox.push_back((d.from, d.bytes));
    }
    shell.frame().quit
}

fn shell(interface: Box<dyn Interface + 'static>) -> Shell<'static, DoubleHost> {
    shell_with(script(), interface)
}

fn shell_with(
    script: Script,
    interface: Box<dyn Interface + 'static>,
) -> Shell<'static, DoubleHost> {
    let (world, data) = data();
    let double = ServerDouble::new(script, std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
    let host = DoubleHost {
        time: 0.0,
        double,
        inbox: VecDeque::new(),
        kept: None,
    };
    Shell::new(host, world, data.clone(), interface)
}

/// The graphics preferences the player kept draw the world from the shell's first frame, as the
/// client reads them when it starts: simple shadows and no light source effects kept, the world is
/// made with them, with nothing drawn yet; a fresh install's draw volumetric shadows and place the
/// effects' lights.
///
/// Behaviour: desktop.shell.the-world-is-drawn-by-the-kept-preferences-from-the-first-frame
#[test]
fn the_world_is_drawn_by_the_kept_preferences_from_the_first_frame() {
    let (world, data) = data();
    let made = |kept: Option<&str>| {
        let double = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
        let login = DevLogin::new(double.login_addr(), "tester".into(), "t".into(), None);
        let host = DoubleHost {
            time: 0.0,
            double,
            inbox: VecDeque::new(),
            kept: kept.map(str::to_owned),
        };
        Shell::new(host, world, data.clone(), Box::new(login))
    };
    let s = made(Some(
        "[Graphics]
OverallGraphicsQuality=0
DynamicObjectShadows=1
LightSourceEffects=0
",
    ));
    assert_eq!(
        s.scene.object_shadows(),
        auberean_draw::volume::setting::SIMPLE
    );
    assert!(!s.scene.light_source_effects());
    let fresh = made(None);
    assert_eq!(
        fresh.scene.object_shadows(),
        auberean_draw::volume::setting::VOLUMETRIC
    );
    assert!(fresh.scene.light_source_effects());
}

fn reports(shell: &Shell<'_, DoubleHost>) -> Vec<(f64, Vec3, u32)> {
    shell
        .client
        .as_ref()
        .map(|c| {
            c.sent()
                .iter()
                .filter_map(|s| match &s.message {
                    // The client's clock runs a second ahead of the host's.
                    ClientMessage::Position(p) => Some((s.at - 1.0, p.origin, p.cell)),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The command-line login takes the shell into the world as the character named; the player's
/// body is made in the hall and stands on its floor; W held runs it through the hall at the
/// running speed, its place reported by the report rule; closing the window in the world ends
/// the program at once with a goodbye on every connection and no log-off first.
///
/// Behaviour: desktop.shell.the-body-runs-with-the-default-keys-and-is-reported
#[test]
fn the_body_runs_with_the_default_keys_and_is_reported() {
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(DevLogin::new(
        server,
        "tester".into(),
        "t".into(),
        Some("tester".into()),
    )));
    let mut quit_at = None;
    let mut entered_at = None;
    let mut before_run = None;
    let mut after_run = None;
    while s.host.time < 30.0 && quit_at.is_none() {
        let t = s.host.time;
        if entered_at.is_none() && s.phase() == Phase::World && s.scene.has_player() {
            entered_at = Some(t);
        }
        if (8.0..8.02).contains(&t) {
            before_run = Some(s.scene.status.origin);
            s.event(&Event::Key {
                key: Key::W,
                down: true,
                repeat: false,
            });
        }
        if (10.0..10.02).contains(&t) {
            s.event(&Event::Key {
                key: Key::W,
                down: false,
                repeat: false,
            });
        }
        if (12.0..12.02).contains(&t) {
            after_run = Some(s.scene.status.origin);
        }
        if (19.5..19.52).contains(&t) {
            s.event(&Event::Close);
        }
        if step(&mut s) {
            quit_at = Some(s.host.time);
        }
    }
    let entered = entered_at.expect("in the world with a body");
    assert!(entered < 5.0, "entered at {entered}");
    let (a, b) = (before_run.expect("before"), after_run.expect("after"));
    assert!(
        (a.z - HALL_AT.z).abs() < 1.0,
        "standing on the hall's floor: {a:?}"
    );
    let ran = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    assert!(
        (8.0..14.0).contains(&ran),
        "two seconds at 6.5 m/s, less the start: {ran} m"
    );
    assert!(b.y > a.y, "north, the way it faces: {a:?} to {b:?}");
    let r = reports(&s);
    let running: Vec<_> = r
        .iter()
        .filter(|(t, _, _)| (8.0..10.5).contains(t))
        .collect();
    assert!(
        running.len() >= 10,
        "a report every 0.2 s or so while running: {}",
        running.len()
    );
    let last = r.iter().rfind(|(t, _, _)| *t < 19.0).expect("a report");
    assert!(
        ((last.1.x - b.x).powi(2) + (last.1.y - b.y).powi(2)).sqrt() < 0.05,
        "the last report is where the body stopped: {last:?} {b:?}"
    );
    assert_eq!(last.2 >> 16, HALL_CELL >> 16, "in the hall's block");
    let sent: Vec<u32> = s
        .client
        .as_ref()
        .expect("the session")
        .sent()
        .iter()
        .map(|m| m.message.message_type())
        .collect();
    assert!(!sent.contains(&0x10027), "no log-off");
    let q = quit_at.expect("closed");
    assert!((19.5..19.6).contains(&q), "at once: {q}");
    let report = s.host.double.report();
    assert_eq!(
        report.client_goodbyes,
        vec![true, true],
        "a goodbye on the login and the world connections"
    );
    let goodbyes: Vec<f64> = report
        .log
        .iter()
        .filter(|(_, _, what)| *what == "the client said goodbye")
        .map(|(t, _, _)| *t)
        .collect();
    // The double's clock runs a second ahead of the shell's.
    assert!(
        goodbyes.len() == 2 && goodbyes.iter().all(|t| (t - 1.0 - q).abs() < 0.05),
        "both as the program closes: {goodbyes:?}"
    );
}

/// Entering the world, the client is in portal space from the moment it asks to enter, before
/// the player is placed, and goes out over two seconds (the tunnel widening into white, then the
/// world out of the white) once the server has the player in the world and the client its body.
///
/// Behaviour: desktop.shell.entering-the-world-goes-through-portal-space
#[test]
fn entering_the_world_goes_through_portal_space() {
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(DevLogin::new(
        server,
        "tester".into(),
        "t".into(),
        Some("tester".into()),
    )));
    let mut entering_in_portal = None;
    let mut body_at = None;
    let mut out_at = None;
    while s.host.time < 12.0 && out_at.is_none() {
        let t = s.host.time;
        if entering_in_portal.is_none() && s.phase() == Phase::Entering {
            entering_in_portal = Some(s.scene.in_portal_space());
        }
        if body_at.is_none() && s.scene.has_player() {
            assert!(
                s.scene.in_portal_space(),
                "still in portal space when the body is made"
            );
            body_at = Some(t);
        }
        if body_at.is_some() && !s.scene.in_portal_space() {
            out_at = Some(t);
        }
        step(&mut s);
    }
    assert_eq!(
        entering_in_portal,
        Some(true),
        "in portal space while entering"
    );
    let (b, o) = (
        body_at.expect("a body"),
        out_at.expect("out of portal space"),
    );
    assert!(
        (2.0..2.2).contains(&(o - b)),
        "two seconds going out: {b} to {o}"
    );
}

/// An interface that takes every key.
struct TakesKeys(DevLogin);

impl Interface for TakesKeys {
    fn event(&mut self, event: &Event, _game: &Game<'_>) -> bool {
        matches!(event, Event::Key { .. })
    }
    fn update(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        self.0.update(game, requests);
    }
}

/// The interface sees every event before the world's controls: a key it takes never moves the
/// body.
///
/// Behaviour: desktop.shell.the-interface-takes-input-first
#[test]
fn the_interface_takes_input_first() {
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(TakesKeys(DevLogin::new(
        server,
        "tester".into(),
        "t".into(),
        None,
    ))));
    let mut before = None;
    while s.host.time < 12.0 {
        let t = s.host.time;
        if (8.0..8.02).contains(&t) {
            before = Some(s.scene.status.origin);
            s.event(&Event::Key {
                key: Key::W,
                down: true,
                repeat: false,
            });
        }
        step(&mut s);
    }
    assert_eq!(s.phase(), Phase::World);
    let (a, b) = (before.expect("in the world"), s.scene.status.origin);
    assert!(
        ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt() < 0.01,
        "never moved: {a:?} {b:?}"
    );
}

/// The cameras an interface saw: at each frame's update and after the world's frame was built.
#[derive(Default)]
struct CamerasSeen {
    at_update: Vec<Option<SeenCamera>>,
    after_world: Vec<Option<SeenCamera>>,
}

/// An interface that notes the camera of the world it is shown, before the world moves on and
/// after the world's frame is built.
struct WatchesTheCamera(DevLogin, Rc<RefCell<CamerasSeen>>);

impl Interface for WatchesTheCamera {
    fn update(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        self.1.borrow_mut().at_update.push(game.seen.camera);
        self.0.update(game, requests);
    }
    fn after_world(&mut self, game: &Game<'_>, _requests: &mut Vec<Request>) {
        self.1.borrow_mut().after_world.push(game.seen.camera);
    }
}

/// While the body runs, what is placed over the world each frame is placed from the camera the
/// world is drawn from that frame: after the world moved on and its frame was built, the camera
/// the interface is shown is the drawn frame's; the update before it is shown the last frame's.
///
/// Behaviour: desktop.shell.what-is-placed-over-the-world-takes-the-frames-own-camera
#[test]
fn what_is_placed_over_the_world_takes_the_frames_own_camera() {
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let seen = Rc::new(RefCell::new(CamerasSeen::default()));
    let mut s = shell(Box::new(WatchesTheCamera(
        DevLogin::new(server, "tester".into(), "t".into(), None),
        Rc::clone(&seen),
    )));
    while s.host.time < 8.0 {
        step(&mut s);
    }
    assert_eq!(s.phase(), Phase::World);
    s.event(&Event::Key {
        key: Key::W,
        down: true,
        repeat: false,
    });
    let mut drawn = Vec::new();
    for _ in 0..60 {
        let h = &mut s.host;
        h.time += 1.0 / 60.0;
        let t = LocalTime(1.0 + h.time);
        for d in h.double.tick(t) {
            h.inbox.push_back((d.from, d.bytes));
        }
        let out = s.frame();
        drawn.push(out.world.map(|f| (f.camera.position, f.camera.heading)));
    }
    let seen = seen.borrow();
    let after: Vec<_> = seen.after_world[seen.after_world.len() - 60..]
        .iter()
        .map(|c| c.map(|c| (c.position, c.heading)))
        .collect();
    let before: Vec<_> = seen.at_update[seen.at_update.len() - 60..]
        .iter()
        .map(|c| c.map(|c| (c.position, c.heading)))
        .collect();
    assert!(drawn.iter().all(Option::is_some), "the world is drawn");
    assert_eq!(after, drawn, "after the world, the drawn frame's camera");
    let moved = drawn.windows(2).filter(|w| w[0] != w[1]).count();
    assert!(moved > 30, "the camera follows the running body: {moved}");
    assert_eq!(
        before[1..],
        drawn[..59],
        "at the update before, the last frame's camera"
    );
}

/// Where the retail client stood the player the server put in the start hall beside a fir
/// (`0x02980120` (101, 247.5, -44.9) facing south, moved where there was room), its heading as
/// the client held it, and where its camera stood (both read from the client's memory).
const BY_THE_FIR: (u32, Vec3, f32) = (
    0x0298_0120,
    Vec3 {
        x: 101.385,
        y: 247.278,
        z: -44.215,
    },
    179.3,
);
const RETAIL_EYE: Vec3 = Vec3 {
    x: 100.886,
    y: 247.719,
    z: -42.502,
};

/// The camera keeps off solid objects: beside the start hall's fir the line from the player's
/// right shoulder back to where the camera wants to be meets the fir's collision cylinder about
/// half a meter out, and the camera stands there, where the retail client's camera stands from the
/// same place, instead of 4.5 m behind.
///
/// Behaviour: desktop.camera.solid-objects-stop-the-camera
#[test]
fn solid_objects_stop_the_camera() {
    let (cell, at, heading) = BY_THE_FIR;
    // Turned clockwise from north by the heading.
    let half = heading.to_radians() / 2.0;
    let facing = Quat {
        w: dereth_primitives::num::math::cosf(half),
        x: 0.0,
        y: 0.0,
        z: -dereth_primitives::num::math::sinf(half),
    };
    let script = script_at(cell, at, facing);
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    while s.host.time < 6.0 {
        step(&mut s);
    }
    let out = s.frame();
    let camera = out.world.expect("the world drawn").camera;
    let eye = camera.position;
    let off = ((eye.x - RETAIL_EYE.x).powi(2)
        + (eye.y - RETAIL_EYE.y).powi(2)
        + (eye.z - RETAIL_EYE.z).powi(2))
    .sqrt();
    assert!(
        off < 0.02,
        "where the retail camera stands: {eye:?}, {off} m off"
    );
    assert!(
        (camera.heading - heading).abs() < 0.5,
        "looking the way the player faces: {}",
        camera.heading
    );
}

/// A player the server puts where it does not fit (in the start hall, against the fir's collision
/// cylinder) stands where there is room nearest that place, as the retail client stood it: 0.685 m
/// up on the same spot the client chose, within 6 cm across the ground.
///
/// Behaviour: desktop.shell.a-placed-player-stands-where-there-is-room
#[test]
fn a_placed_player_stands_where_there_is_room() {
    let (cell, retail, _) = BY_THE_FIR;
    let put = Vec3 {
        x: 101.0,
        y: 247.5,
        z: -44.9,
    };
    let south = Quat {
        w: 0.0,
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };
    let script = script_at(cell, put, south);
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    while s.host.time < 6.0 {
        step(&mut s);
    }
    let at = s.scene.status.origin;
    assert!(
        (at.z - retail.z).abs() < 0.002,
        "up where retail stood it: {at:?}"
    );
    let across = ((at.x - retail.x).powi(2) + (at.y - retail.y).powi(2)).sqrt();
    assert!(
        across < 0.06,
        "beside where retail stood it: {at:?}, {across} m"
    );
}

/// A body far into an interior is drawn. The human training hall's cells reach more than 500 m
/// north of its landblock's corner; a creature beside the player at the hall's straw drudges, some
/// 338 m north of it, is drawn, as the retail client draws the drudges there.
///
/// Behaviour: desktop.others.bodies-anywhere-in-an-interior-are-drawn
#[test]
fn a_body_far_into_the_hall_is_drawn() {
    const DRUDGES_CELL: u32 = 0x0298_0121;
    const CREATURE: u64 = 0x8130_1B05_CC00_018A;
    let creature = CreateObject {
        id: CREATURE,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            position: Placement {
                cell: DRUDGES_CELL,
                frame: Frame {
                    origin: Vec3 {
                        x: -3.9,
                        y: 337.5,
                        z: -44.7,
                    },
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let at = Vec3 {
        x: 2.0,
        y: 330.0,
        z: -44.7,
    };
    let script = script_with(DRUDGES_CELL, at, Quat::IDENTITY, &[creature]);
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    while s.host.time < 6.0 {
        step(&mut s);
    }
    assert!(s.frame().world.is_some(), "the world drawn");
    assert_eq!(s.scene.status.others, 1, "the creature has a body");
    let drawn: Vec<u64> = s.scene.seen.bodies.iter().map(|b| b.id).collect();
    assert!(
        drawn.contains(&CREATURE),
        "the creature among the bodies drawn: {drawn:x?}"
    );
}

/// Another object's creation sent again for the incarnation the client has puts its body where
/// the creation says; a newer incarnation's creation makes the body anew where it was created,
/// with the new incarnation's stamps (so its first positions are taken, not held against the old
/// one's).
///
/// Behaviour: desktop.others.a-creation-again-puts-the-body-and-a-new-incarnation-replaces-it
#[test]
fn a_creation_again_puts_the_body_and_a_new_incarnation_replaces_it() {
    use auberean_client_sdk::model::Object;
    // One creature has its creation sent again, the other a newer incarnation.
    const AGAIN: u64 = 0x8130_1B05_CC00_0100;
    const ANEW: u64 = 0x8130_1B05_CC00_0101;
    let made = |id: u64, instance: u16, at: Vec3, stamp: u16| CreateObject {
        id,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION,
            instance_stamp: instance,
            visual_ordering_stamp: 1,
            position: Placement {
                cell: HALL_CELL,
                frame: Frame {
                    origin: at,
                    rotation: Quat::IDENTITY,
                },
            },
            stamps: [stamp, 0, 0, 0],
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let a = Vec3::new(62.0, 240.0, -44.6);
    let b = Vec3::new(64.0, 244.0, -44.6);
    let a2 = Vec3::new(57.0, 240.0, -44.6);
    let c = Vec3::new(60.0, 236.0, -44.6);
    let d = Vec3::new(61.0, 236.5, -44.6);
    let moved = ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: ANEW,
            instance_stamp: 2,
            other_stamp: 0,
        },
        time: 0.0,
        cell: HALL_CELL,
        origin: d,
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: 0,
        position_stamp: 1,
        force_position_stamp: 0,
        teleport_stamp: 1,
        velocity: Vec3::new(0.0, 0.0, 0.0),
    });
    let later = [
        (10.0, ServerMessage::CreateObject(made(AGAIN, 1, b, 9))),
        (13.0, ServerMessage::CreateObject(made(ANEW, 2, c, 0))),
        (16.0, moved),
    ];
    let first = [made(AGAIN, 1, a, 9), made(ANEW, 1, a2, 9)];
    let script = script_then(HALL_CELL, HALL_AT, Quat::IDENTITY, &first, &later);
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    // A message about `id` takes effect in the client, then a few frames pass: where it is drawn.
    let at = |s: &mut Shell<'_, DoubleHost>, id: u64, taken: &dyn Fn(&Object) -> bool| {
        let here = |s: &Shell<'_, DoubleHost>| {
            s.client
                .as_ref()
                .and_then(|c| c.model.object(id))
                .is_some_and(taken)
        };
        while !here(s) || s.scene.seen.body(id).is_none() {
            assert!(s.host.time < 30.0, "the message never took effect");
            step(s);
        }
        for _ in 0..3 {
            step(s);
        }
        let o = s.scene.seen.body(id).expect("the creature drawn").origin;
        (o.x, o.y)
    };
    let near = |p: (f32, f32), q: Vec3| (p.0 - q.x).abs() < 0.05 && (p.1 - q.y).abs() < 0.05;
    let first = at(&mut s, AGAIN, &|o| o.creations == 1);
    assert!(near(first, a), "made where it was created: {first:?}");
    let again = at(&mut s, AGAIN, &|o| o.creations == 2);
    assert!(
        near(again, b),
        "put where its creation, sent again, says: {again:?}"
    );
    let anew = at(&mut s, ANEW, &|o| o.instance_stamp == 2);
    assert!(
        near(anew, c),
        "the new incarnation where it was created: {anew:?}"
    );
    let next = at(&mut s, ANEW, &|o| {
        o.place.is_some_and(|p| p.position_stamp == 1)
    });
    assert!(
        near(next, d),
        "the new incarnation's first position taken: {next:?}"
    );
}

/// Space pressed with the player standing in the hall: its figure plays the jump's lead-in while
/// the body stays on the ground, until the lead-in's animation sets the jump off; then the body
/// leaves the ground at the jump's speed with its figure in the air, the server is told of the
/// jump then, and the body comes down on the floor with its figure back on the earth.
///
/// Behaviour: desktop.shell.a-standing-jump-leads-in-then-leaves-the-ground
#[test]
fn a_standing_jump_leads_in_then_leaves_the_ground() {
    use auberean_physics::body::JUMP_BEHAVIOR;
    use auberean_physics::motion::{AIR, EARTH};
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(DevLogin::new(
        server,
        "tester".into(),
        "t".into(),
        Some("tester".into()),
    )));
    while s.host.time < 8.0 {
        step(&mut s);
    }
    assert!(s.scene.status.contact, "standing on the floor");
    let pressed = s.host.time;
    for down in [true, false] {
        s.event(&Event::Key {
            key: Key::Space,
            down,
            repeat: false,
        });
        step(&mut s);
    }
    assert!(
        s.scene
            .player_figure()
            .expect("the figure")
            .is_doing_behavior(JUMP_BEHAVIOR),
        "the lead-in plays"
    );
    assert!(s.scene.status.contact, "still on the ground");
    while s.scene.status.contact && s.host.time < pressed + 3.0 {
        step(&mut s);
    }
    let off = s.host.time;
    assert!(!s.scene.status.contact, "the body left the ground");
    assert!(
        off - pressed > 0.1,
        "after the lead-in, not at once: {} s",
        off - pressed
    );
    let air =
        |s: &Shell<'_, DoubleHost>| s.scene.player_figure().expect("the figure").element == AIR;
    assert_eq!(
        s.scene
            .player_figure()
            .expect("the figure")
            .desired_mode_state()
            .element,
        AIR,
        "the figure asked into the air"
    );
    while !air(&s) && s.host.time < off + 0.3 {
        step(&mut s);
    }
    assert!(air(&s), "the figure in the air");
    let jumped: Vec<f64> = s
        .client
        .as_ref()
        .expect("the session")
        .sent()
        .iter()
        .filter_map(|m| match &m.message {
            auberean_client_sdk::protocol::ClientMessage::Position(p)
                if p.flags & auberean_client_sdk::protocol::physics::position_flags::JUMP != 0 =>
            {
                Some(m.at - 1.0)
            }
            _ => None,
        })
        .collect();
    assert_eq!(jumped.len(), 1, "one jump told: {jumped:?}");
    assert!(
        jumped[0] >= off - 0.05,
        "told once the body jumped ({off}): {jumped:?}"
    );
    let rise = s.scene.status.origin.z;
    while !s.scene.status.contact && s.host.time < off + 3.0 {
        step(&mut s);
    }
    assert!(s.scene.status.contact, "down again");
    assert!(
        (s.scene.status.origin.z - HALL_AT.z).abs() < 1.0 && rise > s.scene.status.origin.z,
        "on the floor: {:?}",
        s.scene.status.origin
    );
    for _ in 0..30 {
        step(&mut s);
    }
    assert_eq!(
        s.scene.player_figure().expect("the figure").element,
        EARTH,
        "the figure on the earth"
    );
}

/// With the camera at the eyes the player's own body shows as the avatar option says: during
/// combat only, the client's default. At peace it is not drawn; in a combat mode it is, its head
/// left out (the drawn body ends at the shoulders). While a behavior that moves the first-person
/// camera plays, the camera rides the head bone, a fifth of a meter ahead of it and up, and comes
/// back to the eyes when it ends; at peace the same behavior leaves the camera at the eyes.
///
/// Behaviour: desktop.camera.the-avatar-shows-in-first-person-during-combat-and-the-camera-rides-its-head
#[test]
fn the_avatar_shows_in_first_person_during_combat_and_the_camera_rides_its_head() {
    use auberean_scene::camera::{Press, HEAD_BONE, HEAD_OFFSET, PEACE_MODE};
    /// A combat mode of the human's, and a behavior there (and at peace) that moves the
    /// first-person camera.
    const COMBAT: u32 = 0x4100_0014;
    const MOVES_THE_CAMERA: u32 = 0x4100_0022;
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(DevLogin::new(
        server,
        "tester".into(),
        "t".into(),
        Some("tester".into()),
    )));
    while s.host.time < 6.0 {
        step(&mut s);
    }
    let anims = auberean_animation::CachedRecords::new(&data().0.records);
    // The player's drawn body this frame: whether it was drawn, and the top of its box.
    let drawn = |s: &mut Shell<'_, DoubleHost>| {
        let _ = s.frame();
        s.scene
            .seen
            .bodies
            .iter()
            .find(|b| b.id == PLAYER)
            .map(|b| b.bounds.map_or(0.0, |(_, hi)| hi.z))
    };
    let behind = drawn(&mut s).expect("drawn from behind");
    assert!(behind > 1.6, "the whole body, head and all: {behind}");
    s.scene.camera.press(Press::FirstPerson);
    for _ in 0..10 {
        step(&mut s);
    }
    assert_eq!(
        s.scene
            .player_figure()
            .expect("the figure")
            .modes
            .desired_mode,
        PEACE_MODE
    );
    assert_eq!(drawn(&mut s), None, "not drawn at peace");
    let eyes = s.frame().world.expect("the world").camera.position;
    let feet = s.scene.status.origin;

    // At peace, a behavior that moves the first-person camera leaves it at the eyes.
    assert!(s.scene.player_do_behavior(MOVES_THE_CAMERA), "it plays");
    step(&mut s);
    assert!(s
        .scene
        .player_figure()
        .expect("the figure")
        .modes
        .dynamic_fp_camera());
    let still = s.frame().world.expect("the world").camera.position;
    let moved =
        |a: Vec3, b: Vec3| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
    assert!(moved(still, eyes) < 0.01, "at the eyes: {still:?} {eyes:?}");
    while s
        .scene
        .player_figure()
        .expect("the figure")
        .modes
        .dynamic_fp_camera()
    {
        step(&mut s);
    }

    // In combat the body shows without its head.
    assert!(s.scene.player_do_mode(COMBAT), "into combat");
    let asked = s.host.time;
    while s.host.time < asked + 3.0 {
        step(&mut s);
    }
    let top = drawn(&mut s).expect("drawn in combat");
    s.scene.camera.press(Press::FirstPerson);
    for _ in 0..5 {
        step(&mut s);
    }
    let whole = drawn(&mut s).expect("drawn from behind in combat");
    s.scene.camera.press(Press::FirstPerson);
    for _ in 0..5 {
        step(&mut s);
    }
    assert!(
        top > 1.0 && top < whole - 0.1,
        "the head left out: the box ends at {top}, the whole body at {whole}"
    );

    // While the behavior plays the camera rides the head bone.
    assert!(s.scene.player_do_behavior(MOVES_THE_CAMERA), "it plays");
    let mut rode = 0;
    let played = s.host.time;
    while s.host.time < played + 20.0 {
        step(&mut s);
        let f = s.scene.player_figure().expect("the figure");
        if !f.modes.dynamic_fp_camera() {
            break;
        }
        let bone = f.bone_type_matrix(&anims, HEAD_BONE).expect("a head bone");

        let [x, y, z] = HEAD_OFFSET;
        let at = auberean_world::math::Mat4::translation(Vec3 { x, y, z })
            .then(&bone)
            .point(Vec3::ZERO);
        let camera = s.frame().world.expect("the world").camera.position;
        let body = s
            .scene
            .seen
            .bodies
            .iter()
            .find(|b| b.id == PLAYER)
            .expect("drawn")
            .clone();
        let want = body.to_block(at);
        assert!(
            moved(camera, want) < 0.005,
            "on the head: {camera:?}, the head bone's place {want:?}"
        );
        rode += 1;
    }
    assert!(rode > 5, "it rode the head for a while: {rode} frames");
    assert!(
        !s.scene
            .player_figure()
            .expect("the figure")
            .modes
            .dynamic_fp_camera(),
        "the behavior ended"
    );
    // Where the eyes are over the feet, wherever the behavior took the body.
    let back = s.frame().world.expect("the world").camera.position;
    let now = s.scene.status.origin;
    let over = |e: Vec3, f: Vec3| Vec3 {
        x: e.x - f.x,
        y: e.y - f.y,
        z: e.z - f.z,
    };
    assert!(
        moved(over(back, now), over(eyes, feet)) < 0.01,
        "back at the eyes: {back:?} over {now:?}, before {eyes:?} over {feet:?}"
    );
}

/// An effect played on an object plays its camera part from the player's own effect table: a shake
/// (`0x4100034A`, the record `0x55000000`: 0.3 for half a second) moves the camera from where it
/// stands while it lasts, then it stands there again; a screen flash for the player only
/// (`0x8100030C`) starts when the player plays it, and not when another human does, and lays
/// its color over the world as it ramps up.
///
/// Behaviour: desktop.camera.effects-played-on-objects-play-their-camera-part
#[test]
fn effects_played_on_objects_play_their_camera_part() {
    use auberean_scene::camera::Effect;
    const SHAKE: u32 = 0x4100_034A;
    const FLASH: u32 = 0x8100_030C;
    const OTHER: u64 = 0x8130_1B05_CC00_0042;
    let other = CreateObject {
        id: OTHER,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            position: Placement {
                cell: HALL_CELL,
                frame: Frame {
                    origin: Vec3 {
                        x: 62.0,
                        y: 240.0,
                        z: -44.6,
                    },
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let script = script_with(HALL_CELL, HALL_AT, Quat::IDENTITY, &[other]);
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    while s.host.time < 8.0 {
        step(&mut s);
    }
    let still = s.frame().world.expect("the world").camera.position;
    step(&mut s);
    let again = s.frame().world.expect("the world").camera.position;
    let apart =
        |a: Vec3, b: Vec3| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
    assert!(
        apart(still, again) < 1e-4,
        "it stands still: {still:?} {again:?}"
    );

    s.scene.do_fx(PLAYER, SHAKE, 1.0);
    step(&mut s);
    let shakes: Vec<_> = s
        .scene
        .camera
        .effects
        .tracks
        .iter()
        .filter(|t| t.kind == Effect::Shake)
        .copied()
        .collect();
    assert_eq!(shakes.len(), 1, "one shake");
    assert!(
        (shakes[0].intensity - 0.3).abs() < 1e-6 && (shakes[0].period - 0.5).abs() < 1e-6,
        "the record's: {:?}",
        shakes[0]
    );
    assert_eq!(shakes[0].source, PLAYER);
    let mut most = 0.0f32;
    let started = s.host.time;
    while s.host.time < started + 0.45 {
        step(&mut s);
        let at = s.frame().world.expect("the world").camera.position;
        most = most.max(apart(at, still));
    }
    assert!(most > 0.005, "the camera shook: at most {most} m off");
    while s.host.time < started + 1.0 {
        step(&mut s);
    }
    let after = s.frame().world.expect("the world").camera.position;
    assert!(
        apart(after, still) < 1e-4,
        "still again: {after:?} {still:?}"
    );

    let flashes = |s: &Shell<'_, DoubleHost>| {
        s.scene
            .camera
            .effects
            .tracks
            .iter()
            .filter(|t| matches!(t.kind, Effect::Flash(_)))
            .count()
    };
    s.scene.do_fx(OTHER, FLASH, 1.0);
    step(&mut s);
    assert_eq!(flashes(&s), 0, "the player's only");
    s.scene.do_fx(PLAYER, FLASH, 1.0);
    step(&mut s);
    assert_eq!(flashes(&s), 1, "the player's own");
    // It lays its white over the world as it ramps up (half a second).
    let flashed = |s: &mut Shell<'_, DoubleHost>| s.frame().veil.map_or(0.0, |c| c[3]);
    let early = flashed(&mut s);
    for _ in 0..15 {
        step(&mut s);
    }
    let later = flashed(&mut s);
    assert!(
        later > early && later > 0.2 && later <= 1.0,
        "laid over the world, growing: {early} then {later}"
    );
}

/// The lights the bodies' own effects place are the frame's lights with those of the objects placed
/// with the world: an orange light played on another human in the hall stands with it, at its place
/// (its effect names no offset), as long as the human; a white light played on the player goes after its
/// two seconds. With light source effects off, no effect places a light.
///
/// Behaviour: desktop.scene.the-bodies-effects-place-lights-among-the-frames
#[test]
fn the_bodies_effects_place_lights_among_the_frames() {
    // Of the effects every body's table takes from the common one.
    const ORANGE: u32 = 0x4100_0574;
    const WHITE: u32 = 0x8100_026C;
    const OTHER: u64 = 0x8130_1B05_CC00_0042;
    let other_at = Vec3 {
        x: 62.0,
        y: 240.0,
        z: -44.6,
    };
    let other = CreateObject {
        id: OTHER,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            position: Placement {
                cell: HALL_CELL,
                frame: Frame {
                    origin: other_at,
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let script = script_with(HALL_CELL, HALL_AT, Quat::IDENTITY, &[other]);
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    while s.host.time < 8.0 {
        step(&mut s);
    }
    let before = s.scene.carried_lights().len();
    let colored = |s: &Shell<'_, DoubleHost>, g: f32| {
        s.scene
            .carried_lights()
            .iter()
            .filter(|l| (l.color.g - g).abs() < 0.01 && l.color.r > 0.99)
            .cloned()
            .collect::<Vec<_>>()
    };
    s.scene.do_fx(OTHER, ORANGE, 1.0);
    for _ in 0..3 {
        step(&mut s);
    }
    let orange = colored(&s, 0.498);
    assert_eq!(orange.len(), 1, "{:?}", s.scene.carried_lights());
    let l = &orange[0];
    assert_eq!(l.cell, HALL_CELL);
    assert!(l.range > 0.0 && l.intensity > 0.0, "{l:?}");
    assert!(
        (l.origin.x - other_at.x).abs() < 0.05
            && (l.origin.y - other_at.y).abs() < 0.05
            && (l.origin.z - other_at.z).abs() < 0.05,
        "at the human: {:?}",
        l.origin
    );
    s.scene.do_fx(PLAYER, WHITE, 1.0);
    for _ in 0..3 {
        step(&mut s);
    }
    assert_eq!(
        s.scene.carried_lights().len(),
        before + 2,
        "the player's too"
    );
    let started = s.host.time;
    while s.host.time < started + 2.2 {
        step(&mut s);
    }
    assert_eq!(
        s.scene.carried_lights().len(),
        before + 1,
        "the player's gone after its two seconds"
    );
    assert_eq!(colored(&s, 0.498).len(), 1, "the human's stays");

    let mut g = auberean_draw::quality::Graphics::first_run();
    assert!(g.set(auberean_draw::quality::key::LIGHT_SOURCE_EFFECTS, 0.0));
    s.scene.init_preferences(&g);
    s.scene.do_fx(PLAYER, WHITE, 1.0);
    for _ in 0..3 {
        step(&mut s);
    }
    assert_eq!(
        s.scene.carried_lights().len(),
        before + 1,
        "none placed with the preference off"
    );
}

/// The land the client keeps is the near window of landblocks about the player's cell: the player's
/// block and the eight beside it. A teleport one block east lets go of the west column and makes the
/// east one; going back finds the blocks let go of as they were (kept among the free blocks). Inside
/// the start hall, whose cells do not see the outdoors, no land is kept.
///
/// Behaviour: desktop.shell.the-land-kept-is-the-near-window-about-the-player
#[test]
fn the_land_kept_is_the_near_window_about_the_player() {
    let (world, _) = data();
    let exists = |x: u8, y: u8| {
        world
            .records
            .landblock(i32::from(x), i32::from(y))
            .is_some()
    };
    let (bx, by) = (0x02u8, 0x98u8);
    assert!(exists(bx, by) && exists(bx + 1, by), "outdoor land there");
    let outdoors = |x: u8| u32::from(x) << 24 | u32::from(by) << 16 | 0x0001;
    let at = Vec3 {
        x: 12.0,
        y: 12.0,
        z: 150.0,
    };
    let put = |cell: u32, origin: Vec3, stamp: u16| {
        ServerMessage::Position(ObjectPosition {
            sender: ObjectStamp {
                id: PLAYER,
                instance_stamp: 1,
                other_stamp: 0,
            },
            time: 0.0,
            cell,
            origin,
            heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
            flags: 0,
            position_stamp: stamp,
            force_position_stamp: 0,
            teleport_stamp: stamp,
            velocity: Vec3::new(0.0, 0.0, 0.0),
        })
    };
    let later = [
        (8.0, put(outdoors(bx + 1), at, 1)),
        (12.0, put(outdoors(bx), at, 2)),
        (16.0, put(HALL_CELL, HALL_AT, 3)),
    ];
    let script = script_then(outdoors(bx), at, Quat::IDENTITY, &[], &later);
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    let held = |s: &Shell<'_, DoubleHost>| {
        let mut out = Vec::new();
        for x in bx.saturating_sub(3)..=bx + 4 {
            for y in by - 3..=by + 3 {
                if s.scene.land().block(x, y).is_some() {
                    out.push((x, y));
                }
            }
        }
        out
    };
    let window = |x: u8| {
        let mut out = Vec::new();
        for wx in x - 1..=x + 1 {
            for wy in by - 1..=by + 1 {
                if exists(wx, wy) {
                    out.push((wx, wy));
                }
            }
        }
        out
    };
    // Each place in turn, as the server puts the player there.
    let until = |s: &mut Shell<'_, DoubleHost>, cell: u32| {
        while s.scene.status.cell != cell && s.host.time < 30.0 {
            step(s);
        }
        for _ in 0..3 {
            step(s);
        }
        assert_eq!(s.scene.status.cell, cell, "put there");
    };
    until(&mut s, outdoors(bx));
    assert_eq!(held(&s), window(bx), "about the player's block");
    let west = s.scene.land().block(bx - 1, by).expect("the west block");
    until(&mut s, outdoors(bx + 1));
    assert_eq!(held(&s), window(bx + 1), "the window moved east");
    until(&mut s, outdoors(bx));
    assert_eq!(held(&s), window(bx), "and back");
    let again = s
        .scene
        .land()
        .block(bx - 1, by)
        .expect("the west block again");
    assert!(
        std::sync::Arc::ptr_eq(&west, &again),
        "found again as it was, not made anew"
    );
    until(&mut s, HALL_CELL);
    assert!(held(&s).is_empty(), "no land kept: {:x?}", held(&s));
}

/// A body whose cell the land no longer holds leaves the world and is to be destroyed 25 seconds
/// later; entering the world again first calls that off; left that long, it is let go of. (A
/// creature on the land of block `0x0298`; the player put three blocks east, back, east again.)
///
/// Behaviour: desktop.others.a-body-out-of-the-world-is-let-go-of-after-25-seconds
#[test]
fn a_body_out_of_the_world_is_let_go_of_after_25_seconds() {
    const CREATURE: u64 = 0x8130_1B05_CC00_0077;
    let (world, _) = data();
    let by = 0x98u8;
    let exists = |x: u8| {
        world
            .records
            .landblock(i32::from(x), i32::from(by))
            .is_some()
    };
    assert!(exists(0x02) && exists(0x05), "outdoor land there");
    let outdoors = |x: u8| u32::from(x) << 24 | u32::from(by) << 16 | 0x0001;
    let at = Vec3 {
        x: 12.0,
        y: 12.0,
        z: 150.0,
    };
    let creature = CreateObject {
        id: CREATURE,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            position: Placement {
                cell: outdoors(0x02),
                frame: Frame {
                    origin: Vec3 {
                        x: 16.0,
                        y: 12.0,
                        z: 150.0,
                    },
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let put = |cell: u32, stamp: u16| {
        ServerMessage::Position(ObjectPosition {
            sender: ObjectStamp {
                id: PLAYER,
                instance_stamp: 1,
                other_stamp: 0,
            },
            time: 0.0,
            cell,
            origin: at,
            heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
            flags: 0,
            position_stamp: stamp,
            force_position_stamp: 0,
            teleport_stamp: stamp,
            velocity: Vec3::new(0.0, 0.0, 0.0),
        })
    };
    let later = [
        (8.0, put(outdoors(0x05), 1)),
        (12.0, put(outdoors(0x02), 2)),
        (16.0, put(outdoors(0x05), 3)),
    ];
    let script = script_until(
        outdoors(0x02),
        at,
        Quat::IDENTITY,
        &[creature],
        &later,
        60.0,
    );
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    let until = |s: &mut Shell<'_, DoubleHost>, cell: u32| {
        while s.scene.status.cell != cell && s.host.time < 60.0 {
            step(s);
        }
        for _ in 0..3 {
            step(s);
        }
        assert_eq!(s.scene.status.cell, cell, "put there");
    };
    let due = |s: &Shell<'_, DoubleHost>| {
        s.client
            .as_ref()
            .expect("the session")
            .queue
            .to_be_destroyed(CREATURE)
    };
    until(&mut s, outdoors(0x02));
    assert!(s.scene.status.others >= 1, "the creature has a body");
    assert_eq!(due(&s), None, "kept while in the world");
    until(&mut s, outdoors(0x05));
    let left = s.client.as_ref().expect("the session").local_time();
    let when = due(&s).expect("to be destroyed once out of the world");
    assert!(
        (when - left).abs() < 25.0 + 0.2 && when - left > 25.0 - 0.2,
        "25 seconds on: {when} from {left}"
    );
    until(&mut s, outdoors(0x02));
    assert_eq!(due(&s), None, "called off on entering the world again");
    until(&mut s, outdoors(0x05));
    let gone = s.host.time;
    while s.host.time < gone + 24.0 {
        step(&mut s);
    }
    let has = |s: &Shell<'_, DoubleHost>| {
        s.client
            .as_ref()
            .expect("the session")
            .model
            .object(CREATURE)
            .is_some()
    };
    assert!(has(&s), "not yet");
    while s.host.time < gone + 26.0 {
        step(&mut s);
    }
    assert!(s.scene.has_player(), "still in the world");
    assert!(!has(&s), "let go of");
}

/// Outdoors the sky stands at the camera every frame, wherever the camera moves within its cell,
/// and moves on with the moment every frame, as do the light and fog; the ground keeps the
/// lighting it was lit with until the light's tick, 30 seconds of game time after the last.
/// (The player put 150 m up over the land of block `0x0298` in the morning, falling there; the
/// world's frame read every frame for 40 seconds.)
///
/// Behaviour: desktop.world.the-sky-follows-the-camera-every-frame-and-the-ground-is-lit-on-the-lights-tick
#[test]
fn the_sky_follows_the_camera_every_frame_and_the_ground_is_lit_on_the_lights_tick() {
    let cell = 0x0298_0001;
    let at = Vec3 {
        x: 12.0,
        y: 12.0,
        z: 150.0,
    };
    // Three tenths into a day of the calendar (zero time 4893 s, days of 7347 s).
    let morning = 0.30 * 7347.0 - 4893.0 + 7347.0;
    let clock = ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        time: morning,
        cell,
        origin: at,
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: 0,
        position_stamp: 1,
        force_position_stamp: 0,
        teleport_stamp: 1,
        velocity: Vec3::new(0.0, 0.0, 0.0),
    });
    let script = script_until(cell, at, Quat::IDENTITY, &[], &[(4.0, clock)], 45.0);
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    struct Seen {
        t: f64,
        camera: Vec3,
        sky: Vec<Vec3>,
        turns: Vec<[f32; 4]>,
        light: Vec3,
        ground: [f32; 4],
    }
    let mut seen = Vec::new();
    while s.host.time < 40.0 {
        let h = &mut s.host;
        h.time += 1.0 / 60.0;
        let t = LocalTime(1.0 + h.time);
        for d in h.double.tick(t) {
            h.inbox.push_back((d.from, d.bytes));
        }
        let now = s.host.time;
        let out = s.frame();
        let Some(f) = out.world else {
            continue;
        };
        if now < 5.0 || f.terrain.is_empty() {
            continue;
        }
        let at =
            |b: &auberean_draw::scene::ObjectBatch| b.transform.point(Vec3::new(0.0, 0.0, 0.0));
        seen.push(Seen {
            t: now,
            camera: f.camera.position,
            sky: f.sky_before.iter().chain(&f.sky_after).map(at).collect(),
            turns: f
                .sky_before
                .iter()
                .chain(&f.sky_after)
                .map(|b| b.transform.0[0])
                .collect(),
            light: f.light.light_direction,
            ground: f.terrain[0].vertices[0].color,
        });
    }
    assert!(seen.len() > 1500, "{} frames of the world", seen.len());
    let near = |a: Vec3, b: Vec3| (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs() < 1e-3;
    let gap = |a: Vec3, b: Vec3| Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
    let mut moved = 0;
    for w in seen.windows(2) {
        let (f, g) = (&w[0], &w[1]);
        assert!(!g.sky.is_empty(), "the sky at {:.2}", g.t);
        if g.sky.len() != f.sky.len() {
            // A sky object came or went with the moment.
            continue;
        }
        // Every piece of the sky moves as the camera moves (a sky object far out moves on with the
        // moment by a centimeter or so a frame besides).
        let step = gap(f.camera, g.camera);
        moved += usize::from(step.x.abs() + step.y.abs() + step.z.abs() > 0.05);
        for (a, b) in f.sky.iter().zip(&g.sky) {
            let d = gap(*a, *b);
            assert!(
                (d.x - step.x).abs() + (d.y - step.y).abs() + (d.z - step.z).abs() < 0.03,
                "the sky follows the camera at {:.2}: it moved {d:?}, the camera {step:?}",
                g.t
            );
        }
    }
    assert!(moved > 30, "the camera moved in {moved} frames");
    let when = |t: f64| seen.iter().find(|f| f.t >= t).expect("a frame then");
    let (a, b) = (when(10.0), when(20.0));
    assert_ne!(a.turns, b.turns, "the sky turned on with the moment");
    assert!(!near(a.light, b.light), "and the light with it");
    assert_eq!(
        a.ground, b.ground,
        "the ground lit as it was until the tick"
    );
    let c = when(36.0);
    assert_ne!(b.ground, c.ground, "lit again on the light's tick");
}

/// The world's frame carries the client's running time every frame: the clouds' shadows slide, the
/// water's ripples move and the sun's flare fades by it. (The player put 150 m up over the land of
/// block `0x0298`, the frame read every frame for 12 seconds.)
///
/// Behaviour: desktop.world.the-worlds-frame-carries-the-clients-running-time
#[test]
fn the_worlds_frame_carries_the_clients_running_time() {
    let cell = 0x0298_0001;
    let at = Vec3 {
        x: 12.0,
        y: 12.0,
        z: 150.0,
    };
    let script = script_until(cell, at, Quat::IDENTITY, &[], &[], 15.0);
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    let mut seen: Vec<(f64, f64)> = Vec::new();
    while s.host.time < 12.0 {
        let h = &mut s.host;
        h.time += 1.0 / 60.0;
        let t = LocalTime(1.0 + h.time);
        for d in h.double.tick(t) {
            h.inbox.push_back((d.from, d.bytes));
        }
        let now = s.host.time;
        let out = s.frame();
        if let Some(f) = out.world {
            seen.push((now, f.uptime));
        }
    }
    assert!(seen.len() > 300, "{} frames of the world", seen.len());
    let (first, last) = (seen[0], seen[seen.len() - 1]);
    assert!(
        last.1 - first.1 > 0.9 * (last.0 - first.0),
        "the running time moved on with the clock: {first:?} to {last:?}"
    );
    for w in seen.windows(2) {
        assert!(w[1].1 >= w[0].1, "never back: {w:?}");
    }
}
