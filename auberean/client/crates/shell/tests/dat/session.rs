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
    CharacterIdentity, ObjectStamp, PhysicsDesc, Placement, VisualDesc, WeenieDesc,
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
        let cell = Arc::new(open(&|n| *n == DatName::Cell(1)).expect("a cell file"));
        let data = ClientData {
            files: DataFiles {
                files: vec![portal, cell],
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

/// What the servers say of a player's object, as the 2017 servers sent it: its script package and
/// entity template (whose qualities record says it moves), and its physics type (the low and high
/// words), one the camera's lines of sight pass through.
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
        weenie: player_weenie(),
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
    /// The sound device, when the test gives it one.
    sound: Option<auberean_shell::host::TestSound>,
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
    fn sound_output(&mut self) -> Option<&mut dyn dereth_audio::output::AudioOutput> {
        self.sound
            .as_mut()
            .map(|s| s as &mut dyn dereth_audio::output::AudioOutput)
    }
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

fn shell_with_sound(
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
        sound: Some(auberean_shell::host::TestSound::default()),
    };
    Shell::new(host, world, data.clone(), interface)
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
        sound: None,
    };
    Shell::new(host, world, data.clone(), interface)
}

/// The window is titled as Dereth's client titles its own: the client's name before a login, then
/// the client's name and the account the player logged in with, as typed.
///
/// Behaviour: desktop.shell.the-window-title-names-the-account
#[test]
fn the_window_title_names_the_account_the_player_logged_in_with() {
    let (world, data) = data();
    let double = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
    let login = DevLogin::new(double.login_addr(), "Tester".into(), "t".into(), None);
    let host = DoubleHost {
        time: 0.0,
        double,
        inbox: VecDeque::new(),
        kept: None,
        sound: None,
    };
    let mut s = Shell::new(host, world, data.clone(), Box::new(login));
    assert_eq!(s.title(), "Auberean");
    for _ in 0..3 {
        step(&mut s);
    }
    assert_eq!(s.title(), "Auberean | Tester");
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
            sound: None,
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

/// An interface that asks to leave the server from the world, once.
struct LeavesAtOnce(DevLogin, bool);

impl Interface for LeavesAtOnce {
    fn update(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        self.0.update(game, requests);
        if !self.1 && game.phase == Phase::World {
            self.1 = true;
            requests.push(Request::Disconnect);
        }
    }
}

/// Leaving the server says goodbye on the login and the world connections, and the session goes.
///
/// Behaviour: desktop.session.leaving-the-server-says-goodbye
#[test]
fn leaving_the_server_says_goodbye_on_every_connection() {
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(LeavesAtOnce(
        DevLogin::new(server, "tester".into(), "t".into(), None),
        false,
    )));
    while s.client.is_some() || s.host.time < 1.0 {
        assert!(s.host.time < 20.0, "left the server");
        step(&mut s);
    }
    assert_eq!(s.phase(), Phase::Disconnected);
    assert_eq!(s.host.double.report().client_goodbyes, vec![true, true]);
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

/// The server hears the scene is ready only once a frame of the world has been drawn, so what it
/// keeps going until then (the mist of arrival) is seen.
///
/// Behaviour: desktop.shell.the-scene-is-reported-after-a-frame-of-the-world
#[test]
fn the_scene_is_reported_after_a_frame_of_the_world() {
    use auberean_client_sdk::protocol::ClientMessage;
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(DevLogin::new(
        server,
        "tester".into(),
        "t".into(),
        Some("tester".into()),
    )));
    let mut drawn = false;
    let mut reported = false;
    while s.host.time < 12.0 && !reported {
        reported = s.client.as_ref().is_some_and(|c| {
            c.sent()
                .iter()
                .any(|m| matches!(m.message, ClientMessage::SceneRenderingComplete(_)))
        });
        assert!(!reported || drawn, "reported at {} unseen", s.host.time);
        step(&mut s);
        drawn |= s.frame().world.is_some();
    }
    assert!(reported, "it is reported");
}

/// An effect the server asks of the player's body while the world is out of sight (portal space,
/// here as the player enters) plays at once, not when the body is next drawn: stopped in the same
/// moment it would never have shown.
///
/// Behaviour: desktop.bodies.an-effect-asked-out-of-sight-plays-at-once
#[test]
fn an_effect_asked_out_of_sight_plays_at_once() {
    use auberean_client_sdk::protocol::physics::{Fx, ObjectEvent};
    const ARRIVAL: u32 = 0x4100_0010;
    let later = [(
        0.5,
        ServerMessage::DoFx(ObjectEvent {
            sender: ObjectStamp {
                id: PLAYER,
                instance_stamp: 1,
                other_stamp: 0,
            },
            action: Fx {
                fx: ARRIVAL,
                scalar: 1.0,
            },
        }),
    )];
    let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &later, 40.0);
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
    let mut unseen = false;
    while s.host.time < 4.0 && !unseen {
        step(&mut s);
        unseen = s.frame().world.is_none()
            && s.scene
                .player_effects()
                .iter()
                .any(|(fx, _)| *fx == ARRIVAL);
    }
    assert!(unseen, "playing while the world is out of sight");
}

/// An effect asked again while it still plays does not start its camera part again: the camera
/// keeps the one track it started.
///
/// Behaviour: desktop.camera.an-effect-asked-again-does-not-start-its-camera-part-again
#[test]
fn an_effect_asked_again_does_not_start_its_camera_part_again() {
    use auberean_client_sdk::protocol::physics::{Fx, ObjectEvent};
    const HELD: u32 = 0x4100_0612;
    let ask = |at: f64| {
        (
            at,
            ServerMessage::DoFx(ObjectEvent {
                sender: ObjectStamp {
                    id: PLAYER,
                    instance_stamp: 1,
                    other_stamp: 0,
                },
                action: Fx {
                    fx: HELD,
                    scalar: 1.0,
                },
            }),
        )
    };
    let later = [ask(3.0), ask(3.2)];
    let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &later, 40.0);
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
    let (mut most, mut playing_at) = (0, None);
    while s.host.time < 6.0 {
        step(&mut s);
        most = most.max(s.scene.camera.effects.tracks.len());
        if playing_at.is_none() && s.scene.player_effects().iter().any(|(fx, _)| *fx == HELD) {
            playing_at = Some(s.host.time);
        }
    }
    assert!(playing_at.is_some(), "the effect plays");
    assert_eq!(most, 1, "one track, started once");
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

struct WatchesPointer(Rc<RefCell<Vec<(f32, f32)>>>);

impl Interface for WatchesPointer {
    fn event(&mut self, event: &Event, _game: &Game<'_>) -> bool {
        if let Event::Pointer { x, y } = event {
            self.0.borrow_mut().push((*x, *y));
        }
        false
    }
    fn update(&mut self, _game: &Game<'_>, _requests: &mut Vec<Request>) {}
}

/// The shell lets the interface and the scene hear the pointer only while the window is active
/// (they hear where it went when it comes back) and the pointer is not held; mouse look holds it,
/// and puts it at the middle of the display.
///
/// Behaviour: desktop.input.the-shell-passes-events-through-the-gate
#[test]
fn the_shell_passes_events_through_the_gate() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut s = shell(Box::new(WatchesPointer(Rc::clone(&seen))));
    s.size = (801, 601);
    s.event(&Event::Pointer { x: 1.0, y: 2.0 });
    s.event(&Event::Focus(false));
    s.event(&Event::Pointer { x: 3.0, y: 4.0 });
    assert_eq!(*seen.borrow(), [(1.0, 2.0)]);
    assert_eq!(s.scene.pointer.map(|p| p.0), Some((1.0, 2.0)));
    s.event(&Event::Focus(true));
    assert_eq!(*seen.borrow(), [(1.0, 2.0), (3.0, 4.0)]);
    assert_eq!(s.scene.pointer.map(|p| p.0), Some((3.0, 4.0)));
    s.scene.camera.set_mouse_look(true, 0.0);
    step(&mut s);
    assert_eq!(seen.borrow().last(), Some(&(400.0, 300.0)));
    assert_eq!(s.scene.pointer.map(|p| p.0), Some((400.0, 300.0)));
    let heard = seen.borrow().len();
    s.event(&Event::Pointer { x: 5.0, y: 6.0 });
    step(&mut s);
    assert_eq!(seen.borrow().len(), heard, "held, and heard once");
    assert_eq!(s.scene.pointer.map(|p| p.0), Some((400.0, 300.0)));
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

/// While the cells about the player the server just placed are asked for, the world stands
/// still and the client's objects wait with it (their upkeep does not run); once the cells are
/// in, both go on.
///
/// Behaviour: desktop.world.the-clients-objects-wait-with-the-world-for-its-cells
#[test]
fn the_clients_objects_wait_with_the_world_for_its_cells() {
    let (cell, _, _) = BY_THE_FIR;
    let put = Vec3 {
        x: 101.0,
        y: 247.5,
        z: -44.9,
    };
    let script = script_at(cell, put, Quat::IDENTITY);
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
    let mut waited = 0;
    while s.host.time < 6.0 {
        step(&mut s);
        let objects_wait = s
            .client
            .as_ref()
            .is_some_and(|c| c.world.blocking_for_cells);
        if s.scene.cells_pending() && objects_wait {
            waited += 1;
            assert_eq!(
                s.scene.arrivals(),
                0,
                "not arrived while its cells are asked for"
            );
        }
    }
    assert!(waited > 0, "the objects waited with the world");
    assert!(!s.scene.cells_pending());
    assert_eq!(s.scene.arrivals(), 1, "arrived once they came");
    assert!(s
        .client
        .as_ref()
        .is_some_and(|c| !c.world.blocking_for_cells));
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

/// A player the server puts where there is no room for the body stays in portal space and out of
/// the world: it does not fall and nothing is reported of it. A teleport to a place with room brings
/// it out.
///
/// Behaviour: desktop.shell.a-player-put-where-there-is-no-room-stays-in-portal-space
#[test]
fn a_player_put_where_there_is_no_room_stays_in_portal_space() {
    const NO_ROOM: (u32, Vec3) = (
        0x0298_010C,
        Vec3 {
            x: 14.2,
            y: 425.0,
            z: -43.1,
        },
    );
    let there = ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        time: 0.0,
        cell: HALL_CELL,
        origin: HALL_AT,
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: 0,
        position_stamp: 1,
        force_position_stamp: 0,
        teleport_stamp: 1,
        velocity: Vec3::new(0.0, 0.0, 0.0),
    });
    let mut script = script_then(NO_ROOM.0, NO_ROOM.1, Quat::IDENTITY, &[], &[(8.0, there)]);
    // The scene is never reported from portal space, so the double doesn't wait for it.
    script
        .events
        .retain(|e| !matches!(e, Step::Msg { msg, .. } if msg.c2s && msg.mtype == 0x100D1));
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
    let taken = |s: &Shell<'_, DoubleHost>| {
        s.client
            .as_ref()
            .is_some_and(|c| c.body.teleport_stamp == 1)
    };
    while !taken(&s) {
        assert!(s.host.time < 30.0, "the teleport never took effect");
        if s.scene.has_player() {
            assert!(s.scene.in_portal_space(), "at {}", s.host.time);
            assert_eq!(s.scene.status.origin, NO_ROOM.1, "it does not fall");
        }
        step(&mut s);
    }
    assert!(s.scene.has_player(), "the body was made");
    assert_eq!(s.scene.arrivals(), 0, "not arrived");
    assert_eq!(reports(&s), vec![], "nothing reported");
    let came_out = s.host.time + 4.0;
    while s.scene.in_portal_space() {
        assert!(s.host.time < came_out, "still in portal space");
        step(&mut s);
    }
    assert_eq!(s.scene.arrivals(), 1, "arrived");
    assert!(!reports(&s).is_empty(), "reported again");
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
        z: -43.76,
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

/// A position of the player's own with an impulse throws the body from where it puts it: it leaves
/// the floor going up, and rises.
///
/// Behaviour: desktop.player.an-impulse-of-the-servers-throws-the-body
#[test]
fn a_position_of_the_players_own_with_an_impulse_throws_the_body() {
    use auberean_client_sdk::protocol::physics::position_flags;
    let thrown = ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        time: 0.0,
        cell: HALL_CELL,
        origin: HALL_AT,
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: position_flags::IMPULSE,
        position_stamp: 1000,
        force_position_stamp: 0,
        teleport_stamp: 0,
        velocity: Vec3::new(0.0, 0.0, 6.0),
    });
    let script = script_then(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &[(5.0, thrown)]);
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
    let taken =
        |s: &Shell<'_, DoubleHost>| s.client.as_ref().is_some_and(|c| c.body.thrown().0 == 1);
    while !taken(&s) {
        assert!(s.host.time < 30.0, "the position never took effect");
        step(&mut s);
    }
    step(&mut s);
    let (from, going) = s.scene.player_motion().expect("the player's body");
    assert!(going.z > 0.0, "going up: {going:?}");
    for _ in 0..12 {
        step(&mut s);
    }
    let (now, _) = s.scene.player_motion().expect("the player's body");
    assert!(
        now.frame.origin.z > from.frame.origin.z,
        "rose: {} to {}",
        from.frame.origin.z,
        now.frame.origin.z
    );
}

/// The most the camera moves in a frame as a newer forced Position sends the player 4 m along, in
/// the view asked for.
fn camera_jump_on_a_forced_position(press: Option<auberean_scene::camera::Press>) -> f32 {
    let forced = ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        time: 0.0,
        cell: HALL_CELL,
        origin: Vec3::new(HALL_AT.x + 4.0, HALL_AT.y, HALL_AT.z),
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: 0,
        position_stamp: 1000,
        force_position_stamp: 1,
        teleport_stamp: 0,
        velocity: Vec3::ZERO,
    });
    let script = script_then(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &[(12.0, forced)]);
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
    while s.host.time < 5.0 {
        step(&mut s);
    }
    if let Some(p) = press {
        s.scene.camera.press(p);
    }
    while s.host.time < 8.0 {
        step(&mut s);
    }
    assert_eq!(
        s.client.as_ref().map(|c| c.body.forced_moves()),
        Some(0),
        "not yet"
    );
    let taken = |s: &Shell<'_, DoubleHost>| {
        s.client
            .as_ref()
            .is_some_and(|c| c.body.forced_moves() == 1)
    };
    let mut jump = 0.0f32;
    let mut before: Option<Vec3> = None;
    let mut after = 0;
    while after < 30 {
        assert!(s.host.time < 19.0, "the position never took effect");
        let h = &mut s.host;
        h.time += 1.0 / 60.0;
        let t = LocalTime(1.0 + h.time);
        for d in h.double.tick(t) {
            h.inbox.push_back((d.from, d.bytes));
        }
        let now = s.frame().world.expect("the world is drawn").camera.position;
        if let Some(b) = before.replace(now) {
            let (dx, dy, dz) = (now.x - b.x, now.y - b.y, now.z - b.z);
            jump = jump.max((dx * dx + dy * dy + dz * dz).sqrt());
        }
        if taken(&s) {
            after += 1;
        }
    }
    jump
}

/// A newer forced Position outside first person turns the camera's transition on, so the camera
/// glides to the new place rather than jumping; at the eyes it goes with the body.
///
/// Behaviour: desktop.camera.a-forced-position-glides-outside-first-person
#[test]
fn a_forced_position_glides_the_camera_outside_first_person() {
    use auberean_scene::camera::Press;
    let behind = camera_jump_on_a_forced_position(None);
    assert!(behind < 2.0, "it glides: {behind} m in the frame");
    let eyes = camera_jump_on_a_forced_position(Some(Press::FirstPerson));
    assert!(
        eyes > 3.0,
        "at the eyes it goes at once: {eyes} m in the frame"
    );
}

/// Two men made beside the player, one of an entity template whose qualities record the data
/// holds (it says he moves), the other of a template with no record, are each sent a place 2 m
/// on: the first walks there, the second stays where he was made.
///
/// Behaviour: desktop.others.an-object-whose-qualities-record-is-missing-stays-where-it-was-made
#[test]
fn an_object_whose_qualities_record_is_missing_stays_where_it_was_made() {
    const MOVES: u64 = 0x8130_1B05_CC00_0201;
    const STILL: u64 = 0x8130_1B05_CC00_0202;
    let made = |id: u64, entity: u32, at: Vec3| CreateObject {
        id,
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
                    origin: at,
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        weenie: WeenieDesc {
            mask: WeenieDesc::ENTITY,
            entity_did: entity,
            ..WeenieDesc::default()
        },
    };
    let sent = |id: u64, to: Vec3| {
        ServerMessage::Position(ObjectPosition {
            sender: ObjectStamp {
                id,
                instance_stamp: 1,
                other_stamp: 0,
            },
            time: 0.0,
            cell: HALL_CELL,
            origin: to,
            heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
            flags: 0,
            position_stamp: 1,
            force_position_stamp: 0,
            teleport_stamp: 0,
            velocity: Vec3::new(0.0, 0.0, 0.0),
        })
    };
    let (a, a2) = (Vec3::new(62.0, 240.0, -44.6), Vec3::new(62.0, 242.0, -44.6));
    let (b, b2) = (Vec3::new(60.0, 236.0, -44.6), Vec3::new(60.0, 238.0, -44.6));
    let first = [made(MOVES, 0x4700_0530, a), made(STILL, 0x47FF_FFF0, b)];
    let later = [(6.0, sent(MOVES, a2)), (6.0, sent(STILL, b2))];
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
    while s.host.time < 12.0 {
        step(&mut s);
    }
    let drawn = |id: u64| {
        let o = s.scene.seen.body(id).expect("drawn").origin;
        (o.x, o.y)
    };
    let near = |p: (f32, f32), q: Vec3| (p.0 - q.x).abs() < 0.05 && (p.1 - q.y).abs() < 0.05;
    let (m, st) = (drawn(MOVES), drawn(STILL));
    assert!(
        near(m, a2),
        "the man whose record says he moves walked there: {m:?}"
    );
    assert!(near(st, b), "the man with no record stayed: {st:?}");
}

/// At 144 frames a second, with the player turning on the spot, the camera is placed each time
/// the body ticks (ten times a second) and each frame drawn between its places at the last two
/// ticks by the share of the tick gone: along the straight line between them, about 14 frames to
/// a tick, not round the arc the body turns on (a centimeter out at its middle, three and a half
/// meters back).
///
/// Behaviour: desktop.camera.the-camera-is-drawn-between-its-places-at-the-last-two-ticks
#[test]
fn the_camera_is_drawn_between_its_places_at_the_last_two_ticks() {
    const FAST: f64 = 1.0 / 144.0;
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(DevLogin::new(
        server,
        "tester".into(),
        "t".into(),
        Some("tester".into()),
    )));
    while !(s.phase() == Phase::World && s.scene.has_player()) {
        assert!(s.host.time < 10.0, "in the world");
        step(&mut s);
    }
    while s.host.time < 6.0 {
        step(&mut s);
    }
    s.event(&Event::Key {
        key: Key::A,
        down: true,
        repeat: false,
    });
    // The eyes drawn between two of the body's ticks (its heading turns each tick): one run of
    // frames to a tick.
    let mut runs: Vec<Vec<Vec3>> = Vec::new();
    let mut last = None;
    let start = s.host.time;
    while s.host.time < start + 1.5 {
        let h = &mut s.host;
        h.time += FAST;
        let t = LocalTime(1.0 + h.time);
        for d in h.double.tick(t) {
            h.inbox.push_back((d.from, d.bytes));
        }
        let eye = s.frame().world.expect("the world drawn").camera.position;
        let ticked = s.scene.status.heading;
        if last != Some(ticked) {
            runs.push(Vec::new());
            last = Some(ticked);
        }
        runs.last_mut().expect("a run").push(eye);
    }
    // Each whole run (not the first or the last) lies along the line from its first eye to its
    // last, which the camera crosses in a tick.
    let sub = |a: Vec3, b: Vec3| Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z);
    let len = |v: Vec3| (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    let (mut judged, mut widest, mut moved) = (0, 0.0f32, 0.0f32);
    for run in &runs[1..runs.len() - 1] {
        let (a, b) = (run[0], run[run.len() - 1]);
        let ab = sub(b, a);
        let l = len(ab);
        if run.len() < 8 || l < 0.05 {
            continue;
        }
        judged += 1;
        moved = moved.max(l);
        for e in run {
            let ae = sub(*e, a);
            let across = Vec3::new(
                ae.y * ab.z - ae.z * ab.y,
                ae.z * ab.x - ae.x * ab.z,
                ae.x * ab.y - ae.y * ab.x,
            );
            widest = widest.max(len(across) / l);
        }
    }
    eprintln!(
        "{} runs of frames, {judged} judged; the camera crossed up to {moved} m a tick, at most \
         {widest} m off the line",
        runs.len()
    );
    assert!(
        (12..=16).contains(&runs.len()),
        "ten placements a second: {}",
        runs.len()
    );
    assert!(judged >= 8, "{judged} runs judged");
    assert!(widest < 1e-4, "{widest} m off the line between its places");
}

/// At 144 frames a second the player's figure, running, is animated only on the frames a
/// thirtieth of a second or more after its last animation step, by the time since (about 29 steps
/// a second, every fifth frame), not on every frame; its body still moves every tick.
///
/// Behaviour: desktop.shell.figures-animate-a-thirtieth-of-a-second-apart-at-any-frame-rate
#[test]
fn figures_animate_a_thirtieth_of_a_second_apart_at_any_frame_rate() {
    const FAST: f64 = 1.0 / 144.0;
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(DevLogin::new(
        server,
        "tester".into(),
        "t".into(),
        Some("tester".into()),
    )));
    while !(s.phase() == Phase::World && s.scene.has_player()) {
        assert!(s.host.time < 10.0, "in the world");
        step(&mut s);
    }
    s.event(&Event::Key {
        key: Key::W,
        down: true,
        repeat: false,
    });
    let animated = |s: &Shell<'_, DoubleHost>| {
        let f = s.scene.figure(PLAYER).expect("the player's figure");
        (f.modes.clone(), f.sliders.clone())
    };
    let mut last = animated(&s);
    let mut changed_at = Vec::new();
    let mut frames = 0;
    let start = s.host.time;
    while s.host.time < start + 2.0 {
        let h = &mut s.host;
        h.time += FAST;
        let t = LocalTime(1.0 + h.time);
        for d in h.double.tick(t) {
            h.inbox.push_back((d.from, d.bytes));
        }
        let _ = s.frame();
        frames += 1;
        let now = animated(&s);
        if now != last {
            changed_at.push(s.host.time);
            last = now;
        }
    }
    let gaps: Vec<f64> = changed_at.windows(2).map(|w| w[1] - w[0]).collect();
    let fewest = gaps.iter().copied().fold(f64::MAX, f64::min);
    eprintln!(
        "{frames} frames, {} animated, gaps {fewest} .. {}",
        changed_at.len(),
        gaps.iter().copied().fold(0.0, f64::max)
    );
    assert!(
        fewest >= 1.0 / 30.0 - 1e-6,
        "a thirtieth of a second apart at least: {fewest}"
    );
    assert!(
        (50..=60).contains(&changed_at.len()),
        "about 29 steps a second over two seconds: {}",
        changed_at.len()
    );
    assert!(
        s.scene.status.speed > 6.0,
        "running: {}",
        s.scene.status.speed
    );
}

/// A body whose animation is frozen, as it is through a portal, plays no behavior and takes no
/// mode, and neither waits for the thaw.
///
/// Behaviour: desktop.bodies.a-frozen-body-drops-behaviors-and-modes
#[test]
fn a_frozen_body_drops_behaviors_and_modes() {
    use auberean_client_sdk::runtime::host::Behavior;
    use auberean_client_sdk::script::native_object::BehaviorParams;
    use auberean_physics::body::JUMP_BEHAVIOR;
    const CROUCH: u32 = 0x4000_0013;
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
    let player = s
        .client
        .as_ref()
        .and_then(|c| c.model.player)
        .expect("the player");
    let doing = |s: &Shell<'_, DoubleHost>| {
        s.scene
            .player_figure()
            .expect("the figure")
            .is_doing_behavior(JUMP_BEHAVIOR)
    };
    let play = |s: &mut Shell<'_, DoubleHost>| {
        s.scene.carry_out_behavior(&Behavior::Play {
            object: player,
            params: BehaviorParams {
                behavior: JUMP_BEHAVIOR,
                ..BehaviorParams::default()
            },
        });
    };
    let mode_before = s.scene.player_figure().expect("the figure").mode;
    s.scene
        .body_mut(player)
        .expect("its body")
        .set_animation_frozen(true);
    play(&mut s);
    assert!(s.scene.player_do_mode(CROUCH), "answered as if taken");
    assert!(!doing(&s), "frozen, the behavior does not start");
    assert_eq!(
        s.scene.player_figure().expect("the figure").mode,
        mode_before,
        "nor the mode"
    );
    s.scene
        .body_mut(player)
        .expect("its body")
        .set_animation_frozen(false);
    for _ in 0..4 {
        step(&mut s);
        assert!(!doing(&s), "thawed, nothing was kept for it");
    }
    play(&mut s);
    assert!(doing(&s), "thawed, a new one plays");
}

/// A behavior asked with its actions locked holds the player's own movement while it plays: forward
/// held does not move the body until the behavior is over, then it does.
///
/// Behaviour: desktop.shell.a-behavior-that-locks-actions-holds-the-player
#[test]
fn a_behavior_that_locks_actions_holds_the_player_until_it_ends() {
    use auberean_client_sdk::runtime::host::Behavior;
    use auberean_client_sdk::script::native_object::BehaviorParams;
    use auberean_physics::body::JUMP_BEHAVIOR;
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
    let player = s
        .client
        .as_ref()
        .and_then(|c| c.model.player)
        .expect("the player");
    s.scene.carry_out_behavior(&Behavior::Play {
        object: player,
        params: BehaviorParams {
            behavior: JUMP_BEHAVIOR,
            lock_actions: 1,
            ..BehaviorParams::default()
        },
    });
    step(&mut s);
    step(&mut s);
    let doing = |s: &Shell<'_, DoubleHost>| {
        s.scene
            .player_figure()
            .expect("the figure")
            .is_doing_behavior(JUMP_BEHAVIOR)
    };
    assert!(doing(&s), "it plays");
    assert!(s.scene.actions_locked(), "and holds the player");
    s.event(&Event::Key {
        key: Key::W,
        down: true,
        repeat: false,
    });
    let start = s.host.time;
    while doing(&s) && s.host.time < start + 5.0 {
        step(&mut s);
        if doing(&s) {
            assert!(
                s.scene.status.speed < 0.01,
                "held while it plays: {}",
                s.scene.status.speed
            );
        }
    }
    assert!(!doing(&s), "it ends");
    assert!(!s.scene.actions_locked(), "and lets go");
    let over = s.host.time;
    while s.scene.status.speed < 1.0 && s.host.time < over + 5.0 {
        step(&mut s);
    }
    assert!(
        s.scene.status.speed >= 1.0,
        "runs once it is over: {}",
        s.scene.status.speed
    );
}

/// Portal space is heard: as the player starts entering the world its start sound plays and its two
/// loops stay on while it lasts; as it goes, its stop sound plays and the loops stop.
///
/// Behaviour: desktop.sound.portal-space-is-heard
#[test]
fn portal_space_is_heard() {
    const START: u32 = 0x2A00_0316;
    const STOP: u32 = 0x2A00_0317;
    const LOOPS: [u32; 2] = [0x2A00_0318, 0x2A00_0319];
    let script = script();
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with_sound(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    let heard = |s: &Shell<'_, DoubleHost>, id: u32| {
        s.sound
            .manager
            .playing()
            .iter()
            .any(|&(_, playing, _)| playing == id)
    };
    while !s.scene.in_portal_space() && s.host.time < 10.0 {
        step(&mut s);
    }
    assert!(s.scene.in_portal_space(), "entering the world");
    step(&mut s);
    assert!(heard(&s, START), "the start sound");
    assert!(LOOPS.iter().all(|l| heard(&s, *l)), "both loops");
    let mut stopped = false;
    while s.host.time < 15.0 && !stopped {
        step(&mut s);
        stopped = heard(&s, STOP);
    }
    assert!(stopped, "the stop sound");
    assert!(!LOOPS.iter().any(|l| heard(&s, *l)), "the loops stopped");
    assert!(s.scene.in_portal_space(), "as it goes, not after");
}

/// A jump is heard: the body leaving the ground plays the jump effect's sound, and coming down
/// plays the landing effect's.
///
/// Behaviour: desktop.sound.a-jump-and-its-landing-are-heard
#[test]
fn a_jump_and_its_landing_are_heard() {
    const JUMPS: std::ops::RangeInclusive<u32> = 0x2A00_03B4..=0x2A00_03B9;
    const LANDS: std::ops::RangeInclusive<u32> = 0x2A00_03BA..=0x2A00_03C0;
    let script = script();
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with_sound(
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
    assert!(s.scene.status.contact, "standing on the floor");
    let heard = |s: &Shell<'_, DoubleHost>, ids: std::ops::RangeInclusive<u32>| {
        s.sound
            .manager
            .playing()
            .iter()
            .any(|&(_, id, _)| ids.contains(&id))
    };
    assert!(!heard(&s, JUMPS) && !heard(&s, LANDS), "quiet at rest");
    for down in [true, false] {
        s.event(&Event::Key {
            key: Key::Space,
            down,
            repeat: false,
        });
        step(&mut s);
    }
    let pressed = s.host.time;
    let (mut jumped, mut landed) = (false, false);
    while !landed && s.host.time < pressed + 4.0 {
        step(&mut s);
        jumped |= heard(&s, JUMPS);
        landed |= heard(&s, LANDS);
    }
    assert!(jumped, "leaving the ground is heard");
    assert!(landed, "coming down is heard");
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

/// The widest a held sword's streak ribbon runs from its blade's one end to its other, the sword
/// held in the player's right hand at `scale` times its size, its streak turned on by the swing's
/// effect and the player turning on the spot for a second.
fn streak_width(scale: f32) -> f32 {
    use auberean_client_sdk::protocol::types::ParentLink;
    const SWORD: u64 = 0x8130_1B05_CC00_0077;
    const SWORD_LOOK: u32 = 0x1F00_0056;
    const RIGHT_HAND: u32 = 1;
    const SWING_STARTS_STREAK: u32 = 0x8100_02AE;
    let sword = CreateObject {
        id: SWORD,
        visual: VisualDesc {
            bits: VisualDesc::PARENT | VisualDesc::SCALE,
            parent: SWORD_LOOK,
            scale: Vec3 {
                x: scale,
                y: scale,
                z: scale,
            },
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::PARENT,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            parent: ParentLink {
                parent: PLAYER,
                location: RIGHT_HAND,
                stamp: 1,
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let script = script_with(HALL_CELL, HALL_AT, Quat::IDENTITY, &[sword]);
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
    s.scene.do_fx(PLAYER, SWING_STARTS_STREAK, 1.0);
    s.event(&Event::Key {
        key: Key::A,
        down: true,
        repeat: false,
    });
    let mut widest = 0.0f32;
    let start = s.host.time;
    while s.host.time < start + 1.0 {
        step(&mut s);
        let world = s.frame().world.expect("the world drawn");
        for b in world
            .objects
            .iter()
            .filter(|b| b.step == Some(auberean_draw::scene::Pass::Streaks))
        {
            for pair in b.vertices.as_chunks::<2>().0 {
                let [a, c] = [pair[0].position, pair[1].position];
                let d =
                    ((a[0] - c[0]).powi(2) + (a[1] - c[1]).powi(2) + (a[2] - c[2]).powi(2)).sqrt();
                widest = widest.max(d);
            }
        }
    }
    widest
}

/// A held sword's streak, turned on by its holder's swing, runs along the blade at the sword's
/// size: twice as wide at twice the size (the place it is laid from carries no scale of its own,
/// as the client takes the drawing frame's turn alone and the largest of the scales once).
///
/// Behaviour: draw.streak.a-held-streak-runs-the-blade-at-its-size
#[test]
fn a_held_streak_runs_the_blade_at_its_size() {
    let (one, two) = (streak_width(1.0), streak_width(2.0));
    eprintln!("ribbon {one} m wide at its size, {two} m at twice");
    assert!(one > 0.3, "a ribbon along the blade: {one}");
    assert!((two / one - 2.0).abs() < 0.01, "{two} for {one}");
}

/// A body's own streak is drawn where the body is: a sword lying on the hall's floor, its body
/// asking its streak on as a missile does, draws its ribbon where it lies from the next frame;
/// asked off, the ribbon fades over the streak's trail time (a fifth of a second) and is gone.
///
/// Behaviour: draw.streak.a-bodys-own-streak-is-drawn-where-it-is
#[test]
fn a_bodys_own_streak_is_drawn_where_it_is() {
    const SWORD: u64 = 0x8130_1B05_CC00_0078;
    let sword = CreateObject {
        id: SWORD,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: 0x1F00_0056,
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
                        x: HALL_AT.x + 1.5,
                        y: HALL_AT.y + 2.0,
                        z: HALL_AT.z + 0.1,
                    },
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let script = script_with(HALL_CELL, HALL_AT, Quat::IDENTITY, &[sword]);
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
    assert!(
        s.scene.body(SWORD).is_some_and(|b| b.in_world),
        "the sword lies in the world"
    );
    // The ribbons drawn this frame, and the nearest of their points to where the sword lies.
    let ribbons = |s: &mut Shell<'_, DoubleHost>| {
        let at = s.scene.body(SWORD).expect("its body").position.frame.origin;
        let world = s.frame().world.expect("the world drawn");
        let mut n = 0;
        let mut nearest = f32::MAX;
        for b in world
            .objects
            .iter()
            .filter(|b| b.step == Some(auberean_draw::scene::Pass::Streaks))
        {
            n += 1;
            for v in b.vertices.iter() {
                let p = b.transform.point(Vec3 {
                    x: v.position[0],
                    y: v.position[1],
                    z: v.position[2],
                });
                let d = ((p.x - at.x).powi(2) + (p.y - at.y).powi(2) + (p.z - at.z).powi(2)).sqrt();
                nearest = nearest.min(d);
            }
        }
        (n, nearest)
    };
    assert_eq!(ribbons(&mut s).0, 0, "off, none");
    // As a missile asks when it starts flying.
    let body = s.scene.body_mut(SWORD).expect("its body");
    body.notices.push(auberean_physics::body::Notice::StreakOn);
    step(&mut s);
    let (on, nearest) = ribbons(&mut s);
    assert!(on >= 1, "on, its ribbon: {on}");
    assert!(nearest < 1.5, "where it lies: {nearest} m off");
    let body = s.scene.body_mut(SWORD).expect("its body");
    body.notices.push(auberean_physics::body::Notice::StreakOff);
    for _ in 0..30 {
        step(&mut s);
    }
    assert_eq!(ribbons(&mut s).0, 0, "faded and gone");
}

/// A behavior of the player's that names another object for the camera has that object play the
/// behavior it names, and the camera hangs from one of its bones and looks at another; when the
/// object isn't there, or can't play it, the camera stays on the player.
///
/// Behaviour: desktop.camera.a-behavior-naming-another-object-gives-it-the-camera
#[test]
fn a_behavior_naming_another_object_gives_it_the_camera() {
    use auberean_client_sdk::runtime::host::Behavior;
    use auberean_client_sdk::script::native_object::BehaviorParams;
    const OTHER: u64 = 0x8130_1B05_CC00_0100;
    /// A behavior of the human's.
    const PLAYED: u32 = 0x4100_0022;
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
                    origin: Vec3::new(62.0, 240.0, -44.6),
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let script = script_with(
        HALL_CELL,
        HALL_AT,
        Quat::IDENTITY,
        std::slice::from_ref(&other),
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
    while s.host.time < 6.0 {
        step(&mut s);
    }
    let asked = |target: u64, behavior: u32| Behavior::Play {
        object: PLAYER,
        params: BehaviorParams {
            behavior: PLAYED,
            camera_target: target,
            camera_behavior: behavior,
            ..BehaviorParams::default()
        },
    };

    s.scene
        .carry_out_behavior(&asked(0x8130_1B05_CC00_0199, PLAYED));
    for _ in 0..30 {
        step(&mut s);
    }
    assert_ne!(s.scene.camera.targeting.parent, 0x8130_1B05_CC00_0199);
    while s.scene.player_figure_plays(PLAYED) {
        step(&mut s);
    }

    s.scene.carry_out_behavior(&asked(OTHER, PLAYED));
    let mut hung = false;
    for _ in 0..30 {
        step(&mut s);
        hung |= s.scene.camera.targeting.parent == OTHER
            && s.scene.camera.targeting.parent_bone == 0x4000_0002;
    }
    assert!(hung, "the camera hangs from the object");
    assert!(
        s.scene.camera.no_ray_trace(),
        "nothing keeps it off the world"
    );
}

/// With the camera at the eyes the player's own body shows as the avatar option says: during
/// combat only, the client's default. At peace it is not drawn; in a combat mode it is, its head
/// left out (the drawn body ends at the shoulders). While a behavior that moves the first-person
/// camera plays, the camera rides the head bone, a fifth of a meter ahead of it and up (placed on
/// it each time the body ticks and drawn between, so within 30 cm of the head as drawn), and comes
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
    // The camera is placed at the eyes at the next tick and drawn there once a tick more has
    // passed.
    for _ in 0..30 {
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
    for _ in 0..30 {
        step(&mut s);
    }
    let whole = drawn(&mut s).expect("drawn from behind in combat");
    s.scene.camera.press(Press::FirstPerson);
    for _ in 0..30 {
        step(&mut s);
    }
    assert!(
        top > 1.0 && top < whole - 0.1,
        "the head left out: the box ends at {top}, the whole body at {whole}"
    );

    // While the behavior plays the camera rides the head bone: placed on it each time the body
    // ticks and drawn between those places, so once there it keeps within a tick and a half of
    // the head's travel of where the head is drawn, and catches up with it where the head slows.
    assert!(s.scene.player_do_behavior(MOVES_THE_CAMERA), "it plays");
    let (mut rode, mut arrived, mut widest, mut nearest) = (0, false, 0.0f32, f32::MAX);
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
        let off = moved(camera, body.to_block(at));
        arrived |= off < 0.05;
        if arrived {
            widest = widest.max(off);
            nearest = nearest.min(off);
            rode += 1;
        }
    }
    eprintln!("rode the head {rode} frames, {nearest} to {widest} m from it");
    assert!(rode > 5, "it rode the head for a while: {rode} frames");
    assert!(
        widest < 0.3 && nearest < 0.015,
        "on the head: {nearest} to {widest} m from it"
    );
    assert!(
        !s.scene
            .player_figure()
            .expect("the figure")
            .modes
            .dynamic_fp_camera(),
        "the behavior ended"
    );
    // Where the eyes are over the feet, wherever the behavior took the body, once the camera has
    // been placed there and drawn there.
    for _ in 0..15 {
        step(&mut s);
    }
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
    let (whole, set_opacity) = (l.intensity, |s: &mut Shell<'_, DoubleHost>, o: f32| {
        s.scene
            .figure_mut(OTHER)
            .expect("its figure")
            .sliders
            .do_slider(0, 0x4000_0015, o, 0.0);
    });
    set_opacity(&mut s, 0.25);
    for _ in 0..3 {
        step(&mut s);
    }
    let faded = colored(&s, 0.498)[0].intensity;
    assert!(
        (faded - whole * 0.25).abs() < whole * 0.05,
        "as the body fades: {faded} of {whole}"
    );
    set_opacity(&mut s, 1.0);
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

/// The day's wind blows on the world's missiles: while it rains the day's own wind, level (none on
/// this install's rainy days); on a dry moment a wind that wanders, in all three directions.
///
/// Behaviour: desktop.world.the-days-wind-blows-in-the-world
#[test]
fn the_days_wind_blows_in_the_world() {
    let at = Vec3 {
        x: 122.842,
        y: 89.758,
        z: 80.0,
    };
    let script = script_at(0xC156_0035, at, Quat::IDENTITY);
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with_sound(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    let run = |s: &mut Shell<'_, DoubleHost>, game_time: f64, until: f64| {
        while s.host.time < until {
            let h = &mut s.host;
            h.time += 1.0 / 60.0;
            let t = LocalTime(1.0 + h.time);
            for d in h.double.tick(t) {
                h.inbox.push_back((d.from, d.bytes));
            }
            if let Some(c) = s.client.as_mut() {
                c.sync_time(game_time);
            }
            s.frame();
        }
    };
    let (world, _) = data();
    let region = &world.region;
    let (calendar, sky) = (
        region.calendar.as_ref().expect("a calendar"),
        region.sky.as_ref().expect("a sky"),
    );
    let rain_at = |at: f64| {
        let c = auberean_world::sky::day_clock(calendar, at);
        let day =
            auberean_world::sky::day_for(sky, c.year * i64::from(calendar.days_per_year) + c.day);
        day.and_then(|d| world.records.day(d))
            .map_or((0.0, 0.0, 0.0), |d| {
                auberean_world::sky::rain(&d, c.fraction)
            })
    };
    run(&mut s, 0.0, 3.0);
    let now = s.scene.game_time + 1000.0;
    let mut times = (0..2000).map(|k| now + f64::from(k) * 300.0);
    let dry = times
        .find(|t| rain_at(*t).0 == 0.0 && rain_at(*t + 300.0).0 == 0.0)
        .expect("a dry moment");
    run(&mut s, dry, 5.0);
    let w = s.scene.wind();
    assert!(w.x != 0.0 && w.y != 0.0 && w.z != 0.0, "{w:?}");
    let wet = times
        .find(|t| rain_at(*t).0 == 1.0 && rain_at(*t + 300.0).0 == 1.0)
        .expect("a wet moment");
    run(&mut s, wet, 7.0);
    let moment = s.scene.game_time;
    let (_, x, y) = rain_at(moment);
    assert_eq!(s.scene.wind(), auberean_primitives::Vec3::new(x, y, 0.0));
}

/// On a rainy day the rain falls about the camera: 2,500 streaks at the day's whole amount, its
/// sound heard as loud; on a dry moment none, and the sound stops.
///
/// Behaviour: desktop.world.rain-falls-on-a-rainy-day
#[test]
fn rain_falls_on_a_rainy_day() {
    let at = Vec3 {
        x: 122.842,
        y: 89.758,
        z: 80.0,
    };
    let script = script_at(0xC156_0035, at, Quat::IDENTITY);
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with_sound(
        script,
        Box::new(DevLogin::new(
            server,
            "tester".into(),
            "t".into(),
            Some("tester".into()),
        )),
    );
    let run = |s: &mut Shell<'_, DoubleHost>, game_time: f64, until: f64| {
        let mut streaks = 0;
        let mut drawn = false;
        while s.host.time < until {
            let h = &mut s.host;
            h.time += 1.0 / 60.0;
            let t = LocalTime(1.0 + h.time);
            for d in h.double.tick(t) {
                h.inbox.push_back((d.from, d.bytes));
            }
            if let Some(c) = s.client.as_mut() {
                c.sync_time(game_time);
            }
            let out = s.frame();
            if let Some(f) = out.world {
                drawn = true;
                streaks = f
                    .objects
                    .iter()
                    .filter(|b| {
                        b.mesh == 0 && b.step == Some(auberean_draw::scene::Pass::Particles)
                    })
                    .filter(|b| b.vertices.len() == 10_000)
                    .count();
            }
        }
        assert!(drawn);
        streaks
    };
    // The next moments the day's rain is none and all, by the region's own days.
    let (world, _) = data();
    let region = &world.region;
    let (calendar, sky) = (
        region.calendar.as_ref().expect("a calendar"),
        region.sky.as_ref().expect("a sky"),
    );
    let rain_at = |at: f64| {
        let c = auberean_world::sky::day_clock(calendar, at);
        let day =
            auberean_world::sky::day_for(sky, c.year * i64::from(calendar.days_per_year) + c.day);
        day.and_then(|d| world.records.day(d))
            .map_or(0.0, |d| auberean_world::sky::rain(&d, c.fraction).0)
    };
    assert_eq!(run(&mut s, 0.0, 3.0), 0, "whatever the moment, it is drawn");
    let now = s.scene.game_time + 1000.0;
    let mut times = (0..2000).map(|k| now + f64::from(k) * 300.0);
    let dry = times
        .find(|t| rain_at(*t) == 0.0 && rain_at(*t + 300.0) == 0.0)
        .expect("a dry moment");
    assert_eq!(run(&mut s, dry, 6.0), 0);
    assert_eq!(s.scene.rain_heard(), 0.0);
    const STEADY: u32 = 0x2A00_039E;
    let plays = |s: &Shell<'_, DoubleHost>| {
        s.sound
            .manager
            .playing()
            .iter()
            .any(|&(_, id, _)| id == STEADY)
    };
    assert!(!plays(&s), "no rain, no sound");
    let wet = times
        .find(|t| rain_at(*t) == 1.0 && rain_at(*t + 300.0) == 1.0)
        .expect("a wet moment");
    assert!(run(&mut s, wet, 8.0) >= 1, "it rains");
    assert_eq!(s.scene.rain_heard(), 1.0);
    assert!(plays(&s), "the rain is heard");
    let dry_again = (0..4000)
        .map(|k| s.scene.game_time + 1000.0 + f64::from(k) * 300.0)
        .find(|t| rain_at(*t) == 0.0 && rain_at(*t + 300.0) == 0.0)
        .expect("a dry moment");
    assert_eq!(run(&mut s, dry_again, 10.0), 0);
    assert!(!plays(&s), "the sound stops with the rain");
}

/// The objects placed on a landblock taken up fade in with it: put a block east of the camp, each
/// of the frame's objects standing on the land is drawn no more opaque than its block's opacity,
/// and whole a second on.
///
/// Behaviour: desktop.world.a-blocks-placed-objects-fade-in-with-it
#[test]
fn a_blocks_placed_objects_fade_in_with_it() {
    let at = Vec3 {
        x: 122.842,
        y: 89.758,
        z: 80.0,
    };
    let east = ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        time: 0.0,
        cell: 0xC256_0035,
        origin: at,
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: 0,
        position_stamp: 1,
        force_position_stamp: 0,
        teleport_stamp: 1,
        velocity: Vec3::new(0.0, 0.0, 0.0),
    });
    let script = script_then(0xC156_0035, at, Quat::IDENTITY, &[], &[(5.5, east)]);
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
    let (mut faded, mut whole) = (0, 0);
    while s.host.time < 8.0 {
        let h = &mut s.host;
        h.time += 1.0 / 60.0;
        let t = LocalTime(1.0 + h.time);
        for d in h.double.tick(t) {
            h.inbox.push_back((d.from, d.bytes));
        }
        let out = s.frame();
        let Some(f) = out.world else {
            continue;
        };
        let opacity_of = |x: i32, y: i32| {
            out.fade
                .iter()
                .find(|(c, _)| (i32::from(c.0), i32::from(c.1)) == (x, y))
                .map_or(1.0, |(_, o)| *o)
        };
        for b in &f.objects {
            let Some((x, y)) = b.block else { continue };
            let o = opacity_of(x, y);
            assert!(b.tint.a <= o + 1e-4, "{} over {o}", b.tint.a);
            if o < 1.0 && b.tint.a < 1.0 {
                faded += 1;
            }
            if o >= 1.0
                && b.visible
                    .is_none_or(|v| b.distance_from(f.camera.position) < 0.9 * v)
            {
                assert_eq!(b.tint.a, 1.0);
                whole += 1;
            }
        }
    }
    assert!(faded > 100 && whole > 100, "{faded} faded, {whole} whole");
}

/// A landblock's objects fade in over the second after the near window takes it up, made or found
/// again among the free blocks: each frame lists the blocks taken up less than a second before
/// (not at the very moment) with the time since as their objects' opacity, and a body standing on
/// such a block's land is drawn at that opacity. The blocks the window keeps are not listed, and
/// the first window's second is over before portal space shows the world. (The player put one
/// block east of block `0x0298`, then back; a creature on the land two blocks east.)
///
/// Behaviour: desktop.world.a-landblocks-objects-fade-in-over-a-second-as-it-is-taken-up
#[test]
fn a_landblocks_objects_fade_in_over_a_second_as_it_is_taken_up() {
    const CREATURE: u64 = 0x8130_1B05_CC00_0078;
    let (world, _) = data();
    let (bx, by) = (0x02u8, 0x98u8);
    let exists = |x: u8, y: u8| {
        world
            .records
            .landblock(i32::from(x), i32::from(y))
            .is_some()
    };
    assert!(
        exists(bx - 1, by) && exists(bx + 2, by),
        "outdoor land there"
    );
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
                cell: outdoors(bx + 2),
                frame: Frame {
                    origin: Vec3 {
                        x: 2.0,
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
        (8.0, put(outdoors(bx + 1), 1)),
        (16.0, put(outdoors(bx), 2)),
    ];
    let script = script_until(outdoors(bx), at, Quat::IDENTITY, &[creature], &later, 30.0);
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
    // The column each move takes up: east of the first window, then its west column found again.
    let column = |x: u8| -> Vec<(u8, u8)> {
        (by - 1..=by + 1)
            .filter(|y| exists(x, *y))
            .map(|y| (x, y))
            .collect()
    };
    let (mut listed, mut creature_faded) = (Vec::new(), 0);
    let mut east_taken = None;
    while s.host.time < 24.0 {
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
        let fade = out.fade.clone();
        let objects: Vec<f32> = f.objects.iter().map(|b| b.tint.a).collect();
        for (block, opacity) in &fade {
            let taken = s.scene.block_taken_up(*block).expect("held");
            #[allow(clippy::cast_possible_truncation)]
            let since = (now - taken) as f32;
            assert!(
                (opacity - since).abs() < 1e-4 && *opacity > 0.0 && *opacity <= 1.0,
                "{block:x?} {opacity} {since} at {now}"
            );
            if *block == (bx + 2, by) {
                if let Some(r) = s.scene.body_batches(CREATURE) {
                    assert!(objects[r].iter().all(|a| (a - opacity).abs() < 1e-4));
                    creature_faded += 1;
                }
            }
        }
        if east_taken.is_none() {
            east_taken = s.scene.block_taken_up((bx + 2, by));
        }
        let player_x = u8::try_from(s.scene.status.cell >> 24).unwrap_or(0);
        listed.push((
            now,
            player_x,
            fade.into_iter().map(|(b, _)| b).collect::<Vec<_>>(),
        ));
    }
    // The stretches of the run: before the first move, one block east, and back.
    let east_from = listed
        .iter()
        .position(|(_, x, _)| *x == bx + 1)
        .expect("moved east");
    let back_from = east_from
        + listed[east_from..]
            .iter()
            .position(|(_, x, _)| *x == bx)
            .expect("moved back");
    let blocks_in = |from: usize, to: usize| {
        let mut all: Vec<(u8, u8)> = listed[from..to]
            .iter()
            .flat_map(|(_, _, b)| b.iter().copied())
            .collect();
        all.sort_unstable();
        all.dedup();
        all
    };
    let frames_in = |from: usize, to: usize| {
        listed[from..to]
            .iter()
            .filter(|(_, _, b)| !b.is_empty())
            .count()
    };
    assert_eq!(
        blocks_in(0, east_from),
        [],
        "the first window, in portal space"
    );
    assert_eq!(
        blocks_in(east_from, back_from),
        column(bx + 2),
        "the east column taken up, the blocks kept not"
    );
    // Listed from its taking up, never a second after, whenever the world was drawn.
    let times: Vec<f64> = listed[east_from..back_from]
        .iter()
        .filter(|(_, _, b)| b.contains(&(bx + 2, by)))
        .map(|(t, _, _)| *t)
        .collect();
    let taken = east_taken.expect("the east column taken up");
    let (first_t, last_t) = (times[0], times[times.len() - 1]);
    assert!(
        first_t > taken && first_t < taken + 2.0 / 60.0 && last_t < taken + 1.0,
        "{taken}: {first_t} to {last_t}"
    );
    assert!(frames_in(east_from, back_from) > 20);
    assert_eq!(
        blocks_in(back_from, listed.len()),
        column(bx - 1),
        "found again, faded in again"
    );
    assert!(
        creature_faded > 30,
        "the creature drawn faded: {creature_faded} frames"
    );
}

/// The player put in a cell that isn't there yet, with no portal space on the way: while its cells
/// are waited for no world is drawn at all, and once they are in it is drawn from there.
///
/// Behaviour: desktop.world.no-world-is-drawn-from-a-cell-still-waited-for
#[test]
fn no_world_is_drawn_from_a_cell_still_waited_for() {
    let (caverns, caverns_at) = (0x04AB_0101, Vec3::new(-205.86, 281.62, 0.5));
    let put = ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        time: 0.0,
        cell: caverns,
        origin: caverns_at,
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: 0,
        position_stamp: 1,
        force_position_stamp: 0,
        teleport_stamp: 1,
        velocity: Vec3::new(0.0, 0.0, 0.0),
    });
    let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &[(8.0, put)], 40.0);
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
    while s.host.time < 4.0 {
        step(&mut s);
    }
    let (mut waited, mut drawn_while_waiting, mut drawn_there) = (0, 0, 0);
    while s.host.time < 8.0 {
        s.scene.portal_scene_playing = true;
        step(&mut s);
        let blocking = s.scene.seen.hearing.as_ref().is_some_and(|h| h.blocking);
        let drawn = s.frame().world.is_some();
        if blocking {
            waited += 1;
            drawn_while_waiting += i32::from(drawn);
        } else if s.scene.status.cell == caverns && drawn {
            drawn_there += 1;
        }
    }
    assert!(waited > 0, "the cells were waited for");
    assert_eq!(drawn_while_waiting, 0);
    assert!(drawn_there > 0, "drawn once they are in");
}

/// What the ambient sound hears about the player follows the cells about it each frame: outdoors
/// (block `0x0298`), its land cell and place, the landscape heard, the near window's nine blocks
/// and no interior cell seen from outside about them; in the start hall, whose cells do not see
/// the outdoors, its interior cell and the cells it sees, no landscape and no blocks; in a cell of
/// block `0x04AB` that sees the outdoors, the cell at its own place and the landscape with the
/// window's blocks; and while the cells a placement asks for are waited for, it says so.
///
/// Behaviour: desktop.world.what-the-ambient-sound-hears-follows-the-players-cells
#[test]
fn what_the_ambient_sound_hears_follows_the_players_cells() {
    let (caverns, caverns_at) = (0x04AB_0101, Vec3::new(-205.86, 281.62, 0.5));
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
    let at = Vec3::new(12.0, 12.0, 150.0);
    let later = [
        (8.0, put(HALL_CELL, HALL_AT, 1)),
        (16.0, put(caverns, caverns_at, 2)),
    ];
    let script = script_until(0x0298_0001, at, Quat::IDENTITY, &[], &later, 40.0);
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
    let (mut outdoors, mut hall, mut cave, mut blocking) = (None, None, None, 0);
    // When each place was first heard: each is taken a second after, its cells settled.
    let mut since: [Option<f64>; 3] = [None; 3];
    while s.host.time < 30.0 && (outdoors.is_none() || hall.is_none() || cave.is_none()) {
        step(&mut s);
        let Some(h) = s.scene.seen.hearing.clone() else {
            continue;
        };
        assert_eq!(h.cell, s.scene.status.cell, "the player's cell");
        if h.blocking {
            blocking += 1;
            continue;
        }
        if s.scene.cells_pending() {
            continue;
        }
        let now = s.host.time;
        let (place, slot) = match h.cell {
            0x0298_0001..=0x0298_0040 => (0, &mut outdoors),
            HALL_CELL => (1, &mut hall),
            c if c >> 16 == 0x04AB && c & 0xFFFF >= 0x100 => (2, &mut cave),
            _ => continue,
        };
        if now - *since[place].get_or_insert(now) > 1.0 && slot.is_none() {
            *slot = Some(h);
        }
    }
    let outdoors = outdoors.expect("outdoors");
    assert!(outdoors.outside && outdoors.cells.is_empty());
    assert_eq!(outdoors.blocks.len(), 9, "{:x?}", outdoors.blocks);
    assert!((outdoors.place.x - 12.0).abs() < 1.0 && (outdoors.place.y - 12.0).abs() < 1.0);
    let hall = hall.expect("in the hall");
    assert!(!hall.outside && hall.blocks.is_empty(), "shut in");
    assert!(hall.cells.len() > 1, "its cell and those it sees");
    let of_kind = |kind: u32| hall.cells.iter().filter(|(k, _)| *k == kind).count();
    assert_eq!(
        [of_kind(2), of_kind(0x8100_000F), of_kind(0x8100_001A)],
        [12, 14, 1],
        "the kinds its cells name"
    );
    let cave = cave.expect("in the caverns");
    assert!(
        cave.outside && !cave.blocks.is_empty(),
        "the landscape heard: {cave:?}"
    );
    assert!(
        cave.cells
            .iter()
            .any(|(_, c)| (c.x + 205.861).abs() < 0.01 && (c.y - 281.616).abs() < 0.01),
        "the cells' own places"
    );
    assert!(blocking > 0, "the placements' waits");
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
    // Each place in turn, as the server puts the player there, once its cells are in.
    let until = |s: &mut Shell<'_, DoubleHost>, cell: u32| {
        while (s.scene.status.cell != cell || s.scene.cells_pending()) && s.host.time < 30.0 {
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
        while (s.scene.status.cell != cell || s.scene.cells_pending()) && s.host.time < 60.0 {
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

/// The player moves while its body does, falling as much as running: dropped 150 m over the dry land
/// of block `0xC156`, it moves as it falls straight down with no speed along the ground, and stops
/// moving once it stands there.
///
/// Behaviour: desktop.bodies.the-player-moves-while-its-body-does-falling-too
#[test]
fn the_player_moves_while_its_body_does_falling_too() {
    let cell = 0xC156_0035;
    let at = Vec3 {
        x: 122.84,
        y: 89.76,
        z: 231.0,
    };
    let script = script_until(cell, at, Quat::IDENTITY, &[], &[], 30.0);
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
    let (mut falling, mut landed) = (0, None);
    while s.host.time < 30.0 && landed.is_none() {
        step(&mut s);
        let st = &s.scene.status;
        if !s.scene.has_player() || s.scene.cells_pending() {
            continue;
        }
        if !st.contact && st.origin.z < 221.0 {
            assert!(st.moving && st.speed < 0.01, "falling: {} m/s", st.speed);
            falling += 1;
        } else if st.contact && falling > 0 {
            landed = Some(s.host.time);
        }
    }
    let landed = landed.expect("it landed");
    while s.host.time < landed + 2.0 {
        step(&mut s);
    }
    assert!(falling > 60, "{falling} frames falling");
    assert!(!s.scene.status.moving, "standing");
}

/// Taken from the hall through the menhir portal to the training ground's second area, a closed
/// room with no doorways (cell `0x0298014D`), the player stands where it is put and runs on the
/// room's floor when the key is held, never leaving the cell or sinking through the floor.
///
/// Behaviour: desktop.world.the-player-arrives-and-runs-in-a-room-with-no-doorways
#[test]
fn the_player_arrives_and_runs_in_a_room_with_no_doorways() {
    const ROOM: u32 = 0x0298_014D;
    let arrival = Vec3::new(1.147_527_9, -7.586_839_7, 1.304_330_3);
    let put = ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        time: 0.0,
        cell: ROOM,
        origin: arrival,
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: 0,
        position_stamp: 1,
        force_position_stamp: 0,
        teleport_stamp: 1,
        velocity: Vec3::new(0.0, 0.0, 0.0),
    });
    let later = [(8.0, put)];
    let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &later, 40.0);
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
    let mut settled = None;
    while s.host.time < 30.0 && settled.is_none() {
        step(&mut s);
        let st = &s.scene.status;
        if st.cell == ROOM && st.contact && !s.scene.cells_pending() {
            settled = Some(s.host.time);
        }
    }
    let settled = settled.expect("it arrived and stands");
    while s.host.time < settled + 1.0 {
        step(&mut s);
    }
    let stood = s.scene.status.origin;
    assert!(
        (stood.y - arrival.y).abs() < 0.05,
        "where it was put: {stood:?}"
    );
    assert!(
        (stood.z - arrival.z).abs() < 0.05,
        "on the floor: {stood:?}"
    );
    s.event(&Event::Key {
        key: Key::W,
        down: true,
        repeat: false,
    });
    let until = s.host.time + 3.0;
    while s.host.time < until {
        step(&mut s);
        let st = &s.scene.status;
        assert_eq!(st.cell, ROOM, "still in the room");
        assert!(st.origin.z > 0.95, "above the floor: {:?}", st.origin);
    }
    let ran = s.scene.status.origin.y - stood.y;
    assert!(ran > 10.0, "it ran: {ran} m");
    assert!(s.scene.status.contact, "on the floor");
}

/// What the server asks of the player's body, it does: a push takes it off the ground and up; a
/// slider the network moves goes to its value, one the character template leaves to the client does
/// not; a list of effects meant only for the player plays. Fixture: a server double playing the
/// messages at set times, the player standing in the hall.
///
/// Behaviour: desktop.bodies.the-players-body-does-what-the-server-asks
#[test]
fn the_players_body_does_what_the_server_asks() {
    use auberean_client_sdk::protocol::physics::{Impulse, ObjectEvent, Slide};
    const NETWORK: u32 = 0x4000_001D;
    const THE_CLIENTS: u32 = 0x4000_0016;
    let sender = ObjectStamp {
        id: PLAYER,
        instance_stamp: 1,
        other_stamp: 0,
    };
    let slide = |slider: u32, value: f32| {
        ServerMessage::MoveSlider(ObjectEvent {
            sender,
            action: Slide {
                slider,
                value,
                time: 0.0,
            },
        })
    };
    let later = [
        (8.0, slide(NETWORK, 0.25)),
        (8.0, slide(THE_CLIENTS, 0.25)),
        (
            9.0,
            ServerMessage::Push(ObjectEvent {
                sender,
                action: Impulse {
                    direction: Vec3::new(0.0, 0.0, 8.0),
                    contact: 0,
                },
            }),
        ),
    ];
    let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &later, 40.0);
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
    let mut standing = None;
    let mut highest = f32::MIN;
    let mut left_the_ground = false;
    while s.host.time < 14.0 {
        step(&mut s);
        let st = &s.scene.status;
        if !s.scene.has_player() || s.scene.cells_pending() {
            continue;
        }
        if st.contact && standing.is_none() {
            standing = Some(st.origin.z);
        }
        if standing.is_some() {
            highest = highest.max(st.origin.z);
            left_the_ground |= !st.contact;
        }
    }
    let before = standing.expect("it stood");
    let figure = s.scene.player_figure().expect("the player's figure");
    assert!(
        (figure.sliders.desired(NETWORK) - 0.25).abs() < 1e-3,
        "the network's slider moved: {}",
        figure.sliders.desired(NETWORK)
    );
    let local = figure.sliders.desired(THE_CLIENTS);
    assert!((local - 0.25).abs() > 1e-3, "the client's stays: {local}");
    assert!(left_the_ground, "the push took it off the ground");
    assert!(highest > before + 1.0, "up: {before} to {highest}");
}

/// An object the server creates takes up what its description says: a slider goes to its value at
/// once when no speed is given.
/// Fixture: the human figure created in the hall's cell.
///
/// Behaviour: desktop.bodies.an-object-takes-up-its-description
#[test]
fn an_object_takes_up_its_description() {
    use auberean_client_sdk::protocol::types::SliderSetting;
    const DRUDGES_CELL: u32 = 0x0298_0121;
    const CREATURE: u64 = 0x8130_1B05_CC00_0191;
    const SLIDER: u32 = 0x4000_0016;
    let start = Vec3 {
        x: -3.9,
        y: 337.5,
        z: -44.7,
    };
    let creature = CreateObject {
        id: CREATURE,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION | PhysicsDesc::SLIDERS,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            position: Placement {
                cell: DRUDGES_CELL,
                frame: Frame {
                    origin: start,
                    rotation: Quat::IDENTITY,
                },
            },
            sliders: vec![SliderSetting {
                slider: SLIDER,
                value: 0.25,
                time: 0.0,
            }],
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let at = Vec3 {
        x: 2.0,
        y: 330.0,
        z: -43.76,
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
    while s.host.time < 12.0 {
        step(&mut s);
    }
    let figure = s.scene.figure(CREATURE).expect("its figure");
    let value = figure.sliders.desired(SLIDER);
    assert!((value - 0.25).abs() < 1e-3, "its slider: {value}");
}

/// A body is drawn in the color and at the opacity its sliders hold: red, green, blue and alpha
/// multiply its draws, the opacity its alpha too; moved over a time they go there as it passes.
/// Fixture: the human figure created in the hall's cell, its red slider 0.25.
///
/// Behaviour: desktop.bodies.a-body-is-drawn-in-the-color-its-sliders-hold
#[test]
fn a_body_is_drawn_in_the_color_its_sliders_hold() {
    use auberean_client_sdk::protocol::types::SliderSetting;
    use auberean_physics::body::parent::FADE_SLIDER;
    const DRUDGES_CELL: u32 = 0x0298_0121;
    const CREATURE: u64 = 0x8130_1B05_CC00_0191;
    const RED: u32 = 0x4000_0016;
    const ALPHA: u32 = 0x4000_001A;
    let creature = CreateObject {
        id: CREATURE,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION | PhysicsDesc::SLIDERS,
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
            sliders: vec![SliderSetting {
                slider: RED,
                value: 0.25,
                time: 0.0,
            }],
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let at = Vec3 {
        x: 2.0,
        y: 330.0,
        z: -43.76,
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
    while s.host.time < 12.0 {
        step(&mut s);
    }
    // The player's own draws stay white; only the creature's red is 0.25.
    fn reds(s: &mut Shell<'static, DoubleHost>) -> Vec<(f32, f32)> {
        let now = s.host.time;
        let frame = s.scene.frame(now).expect("the world drawn");
        frame
            .objects
            .iter()
            .filter(|b| b.mesh != 0 && b.tint.r < 0.99)
            .map(|b| (b.tint.r, b.tint.a))
            .collect()
    }
    let at_first = reds(&mut s);
    assert!(
        !at_first.is_empty() && at_first.iter().all(|(r, a)| *r == 0.25 && *a == 1.0),
        "{at_first:?}"
    );
    let figure = s.scene.figure_mut(CREATURE).expect("its figure");
    figure.sliders.do_slider(0, ALPHA, 0.5, 0.0);
    figure.sliders.do_slider(0, FADE_SLIDER, 0.5, 0.0);
    let faded = reds(&mut s);
    assert!(
        !faded.is_empty() && faded.iter().all(|(r, a)| *r == 0.25 && *a == 0.25),
        "{faded:?}"
    );
}

/// An effect aimed at another object acts on it: the entry that hides its target hides it, and it
/// shows again when the effect is over. Fixture: the human figure created in the hall's cell, a
/// server double asking the player's body for the effect at a set time.
///
/// Behaviour: desktop.bodies.an-aimed-effect-acts-on-its-target
#[test]
fn an_aimed_effect_acts_on_its_target() {
    use auberean_client_sdk::protocol::physics::{ObjectEvent, TargetedFx};
    const DRUDGES_CELL: u32 = 0x0298_0121;
    const CREATURE: u64 = 0x8130_1B05_CC00_0191;
    const HIDES_ITS_TARGET: u32 = 0x8100_02AD;
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
        z: -43.76,
    };
    let later = [(
        9.0,
        ServerMessage::DoTargetedFx(ObjectEvent {
            sender: ObjectStamp {
                id: PLAYER,
                instance_stamp: 1,
                other_stamp: 0,
            },
            action: TargetedFx {
                fx: HIDES_ITS_TARGET,
                scalar: 1.0,
                target: CREATURE,
            },
        }),
    )];
    let script = script_until(DRUDGES_CELL, at, Quat::IDENTITY, &[creature], &later, 40.0);
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
    let mut hid = false;
    while s.host.time < 14.0 {
        step(&mut s);
        hid |= s.scene.body(CREATURE).is_some_and(|b| b.is_no_draw());
    }
    assert!(hid, "the effect hid what it aims at");
    while s.host.time < 34.0 && s.scene.body(CREATURE).is_some_and(|b| b.is_no_draw()) {
        step(&mut s);
    }
    assert!(
        !s.scene.body(CREATURE).expect("its body").is_no_draw(),
        "it shows again when the effect is over"
    );
}

/// An object held by a body is drawn from where the holder is drawn, between its ticks, at a scale
/// the holder's whole scale and child scale give it. One told to fade with no delay goes at once.
/// Fixture: the look whose startup effects make two held objects, made beside the player; then the
/// player running while an effect has it hold another.
///
/// Behaviour: desktop.bodies.what-a-body-holds-is-drawn-with-it
#[test]
fn what_a_body_holds_is_drawn_with_it() {
    use auberean_client_sdk::protocol::physics::{Fx, ObjectEvent};
    const CELL: u32 = 0xC156_0035;
    const OBJECT: u64 = 0x8130_1B05_CC00_0191;
    const MAKES_TWO: u32 = 0x1F00_0CBE;
    const MAKES_BLADES: u32 = 0x8100_0366;
    let object = CreateObject {
        id: OBJECT,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: MAKES_TWO,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            position: Placement {
                cell: CELL,
                frame: Frame {
                    origin: Vec3 {
                        x: 126.0,
                        y: 92.0,
                        z: 80.0,
                    },
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let at = Vec3 {
        x: 122.842,
        y: 89.758,
        z: 80.0,
    };
    let later = [(
        22.0,
        ServerMessage::DoFx(ObjectEvent {
            sender: ObjectStamp {
                id: PLAYER,
                instance_stamp: 1,
                other_stamp: 0,
            },
            action: Fx {
                fx: MAKES_BLADES,
                scalar: 1.0,
            },
        }),
    )];
    let script = script_until(CELL, at, Quat::IDENTITY, &[object], &later, 40.0);
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
    while s.host.time < 14.0 {
        step(&mut s);
    }
    let held = s.scene.made_objects();
    assert_eq!(held.len(), 2, "the object holds its two");
    let first = held[0];

    // Scale: the holder's whole scale reaches what it holds.
    s.scene
        .body_mut(OBJECT)
        .expect("the holder")
        .set_scale_vector(dereth_primitives::Vec3::new(2.0, 2.0, 2.0));
    let holder = s.scene.body(OBJECT).expect("the holder");
    let given = (holder.total_scale(None), holder.child_scale);
    let own = s.scene.body(first).expect("what it holds");
    let (alone, wanted) = (own.total_scale(None), own.total_scale(Some(given)));
    assert!((wanted.x - alone.x).abs() > 0.1, "{wanted:?} {alone:?}");
    let (_, _, scale) = s
        .scene
        .temporaries_placed(0.5, true)
        .into_iter()
        .find(|(id, _, _)| *id == first)
        .expect("it is drawn");
    assert!((scale.x - wanted.x).abs() < 1e-4, "{scale:?} {wanted:?}");

    // Fading with no delay, it goes at once.
    s.scene
        .body_mut(first)
        .expect("what it holds")
        .delete_self(0.5, 0.0);
    let start = s.host.time;
    while s.scene.made_objects().contains(&first) && s.host.time < start + 2.0 {
        step(&mut s);
    }
    assert!(
        s.host.time - start < 0.1,
        "gone after {} s",
        s.host.time - start
    );

    // Place: what the running player holds moves between the player's ticks.
    s.event(&Event::Key {
        key: Key::W,
        down: true,
        repeat: false,
    });
    let mut between = false;
    while s.host.time < 22.0 {
        step(&mut s);
        let at = |s: &Shell<_>, alpha: f32| {
            s.scene
                .temporaries_placed(alpha, true)
                .into_iter()
                .find(|(id, _, _)| *id != held[1])
                .map(|(_, p, _)| p.frame.origin)
        };
        if let (Some(a), Some(b)) = (at(&s, 0.0), at(&s, 1.0)) {
            between |= (a.x - b.x).abs() + (a.y - b.y).abs() > 0.05;
        }
    }
    assert!(between, "what it holds is drawn between its ticks");
}

/// An object the server made prints on the sun's shadow map, where it stands, once the light ticks
/// after its drawing properties say it casts on the land, and not otherwise. Fixture: a server
/// double making a man beside the player at the camp, the property set on his entry in the world.
///
/// Behaviour: desktop.world.a-server-object-that-says-so-casts-on-the-land
#[test]
fn a_server_object_that_says_so_casts_on_the_land() {
    use auberean_physics::world::graphics;
    const CELL: u32 = 0xC156_0035;
    const OBJECT: u64 = 0x8130_1B05_CC00_0191;
    let printed = |casts: bool| {
        let object = CreateObject {
            id: OBJECT,
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
                    cell: CELL,
                    frame: Frame {
                        origin: Vec3 {
                            x: 126.0,
                            y: 92.0,
                            z: 80.0,
                        },
                        rotation: Quat::IDENTITY,
                    },
                },
                ..PhysicsDesc::default()
            },
            ..CreateObject::default()
        };
        let at = Vec3 {
            x: 122.842,
            y: 89.758,
            z: 80.0,
        };
        let script = script_until(CELL, at, Quat::IDENTITY, &[object], &[], 200.0);
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
        let mut set = false;
        let mut seen = 0;
        while s.host.time < 120.0 {
            step(&mut s);
            if !set {
                if let Some(k) = s.scene.table().find(OBJECT) {
                    set = true;
                    s.scene
                        .table_mut()
                        .set_graphics_state(k, graphics::LANDSCAPE_SHADOW, casts);
                }
            }
            let now = s.host.time;
            if let Some(map) = s.scene.frame(now).and_then(|f| f.shadow.as_ref()) {
                let here = map.casters.iter().filter(|c| {
                    let o = c.transform.point(auberean_primitives::Vec3::ZERO);
                    (o.x - 126.0).abs() < 0.01 && (o.y - 92.0).abs() < 0.01
                });
                seen = seen.max(here.count());
            }
        }
        assert!(set, "he stands in the world");
        seen
    };
    assert_eq!(printed(false), 0, "he does not say so");
    assert!(printed(true) > 0, "he says so");
}

/// An object whose look's startup effects make objects has them as it arrives, each hanging from it
/// at its holding location; one whose look's do not has none. Fixture: a server double making the
/// object beside the player in the hall.
///
/// Behaviour: desktop.bodies.an-objects-startup-effects-make-their-objects
#[test]
fn an_objects_startup_effects_make_their_objects() {
    const CELL: u32 = 0x0298_0121;
    const OBJECT: u64 = 0x8130_1B05_CC00_0191;
    const MAKES_TWO: u32 = 0x1F00_0CBE;
    let made_for = |look: u32| {
        let object = CreateObject {
            id: OBJECT,
            visual: VisualDesc {
                bits: VisualDesc::DID,
                did: look,
                ..VisualDesc::default()
            },
            physics: PhysicsDesc {
                bits: PhysicsDesc::POSITION,
                instance_stamp: 1,
                visual_ordering_stamp: 1,
                position: Placement {
                    cell: CELL,
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
            z: -43.76,
        };
        let script = script_until(CELL, at, Quat::IDENTITY, &[object], &[], 40.0);
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
        while s.host.time < 14.0 {
            step(&mut s);
        }
        let made = s.scene.made_objects();
        let held = made
            .iter()
            .filter(|m| s.scene.body(**m).is_some_and(|b| b.has_parent))
            .count();
        (made.len(), held)
    };
    assert_eq!(made_for(HUMAN_MALE), (0, 0), "this look makes none");
    assert_eq!(
        made_for(MAKES_TWO),
        (2, 2),
        "this one makes two and holds them"
    );
}

/// A behavior that names a target and a look to clone gives them to the effects its animation sets
/// off: putting an item down, the player's effects hide the item and take a clone of it in hand. The
/// clone has never stood in the world, so the entry that lets go of what it holds leaves it held.
/// As the effect ends the item shows again, its marks having stayed, and the clone fades out in the
/// hand and goes three seconds later. Fixture: the human figure in the hall's cell
/// standing in for the item, a server double asking the player's body for the behavior at a set
/// time.
///
/// Behaviour: desktop.bodies.a-behaviors-target-and-clone-go-to-its-effects
#[test]
fn a_behaviors_target_and_clone_go_to_its_effects() {
    use auberean_client_sdk::protocol::physics::ObjectEvent;
    use auberean_client_sdk::protocol::types::BehaviorParams;
    const DRUDGES_CELL: u32 = 0x0298_0121;
    const ITEM: u64 = 0x8130_1B05_CC00_0191;
    const ITEM_LOOK: u32 = 0x1F00_0069;
    const PUTTING_DOWN: u32 = 0x4100_0021;
    const PUT_DOWN_SCRIPT: u32 = 0x5700_0005;
    let item = CreateObject {
        id: ITEM,
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
        z: -43.76,
    };
    let later = [(
        9.0,
        ServerMessage::PlayBehavior(ObjectEvent {
            sender: ObjectStamp {
                id: PLAYER,
                instance_stamp: 1,
                other_stamp: 0,
            },
            action: BehaviorParams {
                bits: BehaviorParams::BEHAVIOR
                    | BehaviorParams::FX_SCRIPT
                    | BehaviorParams::TARGET
                    | BehaviorParams::CLONE,
                behavior: PUTTING_DOWN,
                fx_script: PUT_DOWN_SCRIPT,
                target: ITEM,
                clone_visual: ITEM_LOOK,
                ..BehaviorParams::default()
            },
        }),
    )];
    let script = script_until(DRUDGES_CELL, at, Quat::IDENTITY, &[item], &later, 40.0);
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
    let (mut hid, mut held, mut clone) = (false, false, None);
    let mut marked = true;
    let mut let_go = false;
    let (mut faded_from, mut faded_out) = (None, None);
    let mut gone_at = None;
    while s.host.time < 34.0 {
        step(&mut s);
        let hidden = s.scene.body(ITEM).is_some_and(|b| b.is_no_draw());
        hid |= hidden;
        if hidden {
            marked &= s.scene.seen.body(ITEM).is_some_and(|b| b.bounds.is_some());
        }
        if clone.is_none() {
            clone = s.scene.made_objects().first().copied();
        }
        let Some(m) = clone else { continue };
        if !s.scene.made_objects().contains(&m) {
            gone_at = gone_at.or(Some(s.host.time));
            if !hidden {
                break;
            }
            continue;
        }
        let b = s.scene.body(m);
        held |= b.is_some_and(|b| b.has_parent);
        let_go |= held && b.is_some_and(|b| !b.has_parent || b.in_world);
        let opacity = s.scene.temporary_opacity(m).unwrap_or(1.0);
        if opacity < 1.0 {
            faded_from = faded_from.or(Some(s.host.time));
        }
        if opacity <= 0.01 {
            faded_out = faded_out.or(Some(s.host.time));
        }
    }
    assert!(hid, "the behavior's effects hide the item");
    assert!(clone.is_some(), "they make a clone of it");
    assert!(held, "and take the clone up");
    assert!(!let_go, "it stays in the hand");
    assert!(marked, "what marks the item stays while it is hidden");
    let from = faded_from.expect("it fades out");
    let out = faded_out.expect("all the way");
    assert!(
        (0.3..=0.7).contains(&(out - from)),
        "over half a second: {}",
        out - from
    );
    let gone = gone_at.expect("the clone goes");
    assert!(
        (2.5..=3.5).contains(&(gone - from)),
        "three seconds later: {}",
        gone - from
    );
    assert!(
        !s.scene.body(ITEM).expect("its body").is_no_draw(),
        "and the item shows again"
    );
}

/// An effect that makes an object and names a behavior: the object hangs from the player at the
/// entry's holding location, is hidden at once as its behavior ends, and goes three seconds later,
/// past the effect's own second. Fixture: a server double asking the player's body for the effect
/// at a set time.
///
/// Behaviour: desktop.bodies.an-object-an-effect-makes-goes-with-its-behavior
#[test]
fn an_object_an_effect_makes_goes_with_its_behavior() {
    use auberean_client_sdk::protocol::physics::{Fx, ObjectEvent};
    const MAKES_BLADES: u32 = 0x8100_0366;
    const OPACITY: u32 = 0x4000_0015;
    let later = [(
        9.0,
        ServerMessage::DoFx(ObjectEvent {
            sender: ObjectStamp {
                id: PLAYER,
                instance_stamp: 1,
                other_stamp: 0,
            },
            action: Fx {
                fx: MAKES_BLADES,
                scalar: 1.0,
            },
        }),
    )];
    let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &later, 40.0);
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
    let mut made = None;
    while made.is_none() && s.host.time < 20.0 {
        step(&mut s);
        made = s.scene.made_objects().first().copied();
    }
    let made = made.expect("the effect made its object");
    assert!(
        s.scene.body(made).is_some_and(|b| b.has_parent),
        "it hangs from the player"
    );
    let start = s.host.time;
    let shown = |s: &Shell<_>| {
        s.scene
            .figure(made)
            .is_some_and(|f| f.sliders.current(OPACITY) > 0.0)
    };
    while shown(&s) && s.host.time < start + 20.0 {
        step(&mut s);
    }
    let hidden_after = s.host.time - start;
    assert!(
        s.scene.made_objects().contains(&made),
        "hidden, not yet gone"
    );
    assert!(
        (0.3..2.0).contains(&hidden_after),
        "hidden as its behavior ends, 0.6 s in: {hidden_after}"
    );
    let hidden_at = s.host.time;
    while s.scene.made_objects().contains(&made) && s.host.time < hidden_at + 20.0 {
        step(&mut s);
    }
    let gone_after = s.host.time - hidden_at;
    assert!(
        (2.5..3.5).contains(&gone_after),
        "it goes three seconds later: {gone_after}"
    );
    let holder = s.scene.body(PLAYER).expect("the player's body");
    assert!(
        holder
            .children
            .as_ref()
            .is_none_or(|c| c.children.iter().all(|c| c.id != made)),
        "the player no longer holds it"
    );
}

/// An effect whose entry makes an object, names no behavior and gives no lifetime has it for 15
/// seconds, then it fades out; one giving a lifetime below zero keeps it. Fixture: a server double
/// asking the player's body for each effect at a set time, in sessions of their own.
///
/// Behaviour: desktop.bodies.an-object-an-effect-makes-lasts-as-its-entry-says
#[test]
fn an_object_an_effect_makes_lasts_as_its_entry_says() {
    use auberean_client_sdk::protocol::physics::{Fx, ObjectEvent};
    const OPACITY: u32 = 0x4000_0015;
    // How long after it was made the effect's object starts to fade, if within 30 s.
    let fades_after = |effect: u32| -> Option<f64> {
        let later = [(
            9.0,
            ServerMessage::DoFx(ObjectEvent {
                sender: ObjectStamp {
                    id: PLAYER,
                    instance_stamp: 1,
                    other_stamp: 0,
                },
                action: Fx {
                    fx: effect,
                    scalar: 1.0,
                },
            }),
        )];
        let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &later, 60.0);
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
        let mut made = None;
        while made.is_none() && s.host.time < 20.0 {
            step(&mut s);
            made = s.scene.made_objects().first().copied();
        }
        let made = made.expect("the effect made its object");
        let start = s.host.time;
        while s.host.time < start + 30.0 {
            step(&mut s);
            let figure = s.scene.figure(made).expect("its figure");
            if figure.sliders.desired(OPACITY) == 0.0 {
                return Some(s.host.time - start);
            }
        }
        None
    };
    let fades = fades_after(0x4100_0672).expect("an entry that says nothing ends");
    assert!((14.5..16.0).contains(&fades), "15 seconds: {fades}");
    assert_eq!(fades_after(0x4100_05BE), None, "below zero: it stays");
}

/// A hold the server starts on the player loops until each of the ways the server has of ending it
/// ends it: letting it go, stopping all behaviors (a hold still within the cycles it was asked for,
/// as they stop only what someone waits on), stopping those a move cancels, ending its layer.
/// Fixture: a server double asking at set times.
///
/// Behaviour: desktop.bodies.a-behavior-the-server-started-it-can-stop-or-let-go
#[test]
fn a_behavior_the_server_started_it_can_stop_or_let_go() {
    use auberean_client_sdk::protocol::physics::{NoAction, ObjectEvent, Release, Word};
    use auberean_client_sdk::protocol::types::BehaviorParams;
    /// A hold of the human's, on layer 6.
    const HOLD: u32 = 0x4100_0125;
    const LAYER: u32 = 6;
    const MOVE_TO_CANCELS_WORD: usize = 1;
    // Each event newer than the last.
    let sender = |n: u16| ObjectStamp {
        id: PLAYER,
        instance_stamp: 1,
        other_stamp: n,
    };
    let hold = |n: u16, move_to_cancels: bool, waited_on: bool| {
        let mut p = BehaviorParams {
            bits: BehaviorParams::BEHAVIOR,
            behavior: HOLD,
            ..BehaviorParams::default()
        };
        if move_to_cancels {
            p.bits |= BehaviorParams::WORD_BITS[MOVE_TO_CANCELS_WORD];
            p.words[MOVE_TO_CANCELS_WORD] = 1;
        }
        if waited_on {
            p.bits |= BehaviorParams::HOLD_CYCLES;
            p.hold_cycles = 1000;
        }
        ServerMessage::PlayBehavior(ObjectEvent {
            sender: sender(n),
            action: p,
        })
    };
    let ends: [ServerMessage; 4] = [
        ServerMessage::LetGoBehavior(ObjectEvent {
            sender: sender(2),
            action: Release {
                behavior: HOLD,
                params: BehaviorParams::default(),
            },
        }),
        ServerMessage::StopAllBehaviors(ObjectEvent {
            sender: sender(4),
            action: NoAction,
        }),
        ServerMessage::StopMoveCancelsBehaviors(ObjectEvent {
            sender: sender(6),
            action: NoAction,
        }),
        ServerMessage::EndLayer(ObjectEvent {
            sender: sender(8),
            action: Word(LAYER),
        }),
    ];
    // A hold every ten seconds from 9 s, ended 5 s on.
    let mut later = Vec::new();
    let mut at = 9.0;
    for (k, end) in (0u16..).zip(&ends) {
        later.push((at, hold(2 * k + 1, k == 2, k == 1)));
        later.push((at + 5.0, end.clone()));
        at += 10.0;
    }
    let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &later, 60.0);
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
    let doing = |s: &Shell<'_, DoubleHost>| {
        s.scene
            .figure(PLAYER)
            .is_some_and(|f| f.is_doing_behavior(HOLD))
    };
    while !doing(&s) && s.host.time < 30.0 {
        step(&mut s);
    }
    for k in 0..ends.len() {
        assert!(doing(&s), "hold {k} plays");
        let started = s.host.time;
        // It is still looping a second before the end is asked for.
        while s.host.time < started + 3.5 {
            step(&mut s);
        }
        assert!(doing(&s), "hold {k} loops until it is ended");
        while doing(&s) && s.host.time < started + 9.5 {
            step(&mut s);
        }
        assert!(!doing(&s), "end {k} ended the hold");
        while !doing(&s) && k + 1 < ends.len() && s.host.time < started + 20.0 {
            step(&mut s);
        }
    }
}

/// The rest the server asks of a body by itself: an effect script plays on the figure, a new motion
/// description takes over, and a private effect plays on the player: this one makes an object.
/// Fixture: a server double asking at set times.
///
/// Behaviour: desktop.bodies.a-body-takes-a-script-a-motion-description-and-private-effects
#[test]
fn a_body_takes_a_script_a_motion_description_and_private_effects() {
    use auberean_client_sdk::protocol::physics::{ObjectEvent, PrivateFx, Word};
    const SCRIPT: u32 = 0x5700_0005;
    const MOTION: u32 = 0x3200_0001;
    const MAKES_BLADES: u32 = 0x8100_0366;
    let sender = ObjectStamp {
        id: PLAYER,
        instance_stamp: 1,
        other_stamp: 0,
    };
    let later = [
        (
            9.0,
            ServerMessage::DoFxScript(ObjectEvent {
                sender,
                action: Word(SCRIPT),
            }),
        ),
        (
            9.0,
            ServerMessage::MotionInterpChange(ObjectEvent {
                sender,
                action: Word(MOTION),
            }),
        ),
        (9.0, ServerMessage::PrivateFx(PrivateFx(vec![MAKES_BLADES]))),
    ];
    let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &later, 40.0);
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
    let (mut played, mut made) = (false, false);
    while s.host.time < 14.0 {
        step(&mut s);
        played |= s
            .scene
            .figure(PLAYER)
            .is_some_and(|f| f.scripts.tracks.iter().any(|t| t.script == SCRIPT));
        made |= !s.scene.made_objects().is_empty();
    }
    assert!(played, "the effect script played");
    assert!(made, "the private effect played on the player");
    let body = s.scene.body(PLAYER).expect("the player's body");
    assert_eq!(body.minterp.desc_id, MOTION, "the motion description");
}

/// A creature created with behaviors and effects in its description plays them: the hold plays on
/// its figure and the effect makes its object. Fixture: the human figure created in the hall's cell.
///
/// Behaviour: desktop.bodies.an-object-starts-the-behaviors-and-effects-its-description-gives
#[test]
fn an_object_starts_the_behaviors_and_effects_its_description_gives() {
    use auberean_client_sdk::protocol::types::{BehaviorParams, RunningFx};
    const DRUDGES_CELL: u32 = 0x0298_0121;
    const CREATURE: u64 = 0x8130_1B05_CC00_0191;
    const HOLD: u32 = 0x4100_0125;
    const MAKES_BLADES: u32 = 0x8100_0366;
    let creature = CreateObject {
        id: CREATURE,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION | PhysicsDesc::BEHAVIORS | PhysicsDesc::FX,
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
            behaviors: vec![BehaviorParams {
                bits: BehaviorParams::BEHAVIOR,
                behavior: HOLD,
                ..BehaviorParams::default()
            }],
            fx: vec![RunningFx {
                fx: MAKES_BLADES,
                scalar: 1.0,
                target: 0,
            }],
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let at = Vec3 {
        x: 2.0,
        y: 330.0,
        z: -43.76,
    };
    let script = script_until(DRUDGES_CELL, at, Quat::IDENTITY, &[creature], &[], 30.0);
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
    let (mut plays, mut made) = (false, false);
    while s.host.time < 14.0 {
        step(&mut s);
        plays |= s
            .scene
            .figure(CREATURE)
            .is_some_and(|f| f.is_doing_behavior(HOLD));
        made |= !s.scene.made_objects().is_empty();
    }
    assert!(plays, "the behavior its description lists plays");
    assert!(made, "the effect its description lists makes its object");
}

/// A behavior the server starts on one of its own objects that says to destroy it as it ends hides
/// the object and takes it away three seconds on. Fixture: a server double asking at a set time.
///
/// Behaviour: desktop.bodies.a-behavior-that-destroys-its-object-takes-a-server-object-away
#[test]
fn a_behavior_that_destroys_its_object_takes_a_server_object_away() {
    use auberean_client_sdk::protocol::physics::ObjectEvent;
    use auberean_client_sdk::protocol::types::BehaviorParams;
    const DRUDGES_CELL: u32 = 0x0298_0121;
    const CREATURE: u64 = 0x8130_1B05_CC00_018A;
    const OPACITY: u32 = 0x4000_0015;
    /// Flag 0x8000 is the twelfth of the flags followed by a word.
    const DESTROYS_WORD: usize = 11;
    let at = Vec3 {
        x: 2.0,
        y: 330.0,
        z: -43.76,
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
                cell: DRUDGES_CELL,
                frame: Frame {
                    origin: Vec3 {
                        x: 4.0,
                        y: 330.0,
                        z: -44.7,
                    },
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let mut once = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR | BehaviorParams::WORD_BITS[DESTROYS_WORD],
        behavior: 0x4100_0083,
        ..BehaviorParams::default()
    };
    once.words[DESTROYS_WORD] = 1;
    let later = [(
        9.0,
        ServerMessage::PlayBehavior(ObjectEvent {
            sender: ObjectStamp {
                id: CREATURE,
                instance_stamp: 1,
                other_stamp: 1,
            },
            action: once,
        }),
    )];
    let script = script_until(DRUDGES_CELL, at, Quat::IDENTITY, &[creature], &later, 60.0);
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
    let mut hidden_at = None;
    let mut gone_at = None;
    let mut seen = false;
    while s.host.time < 40.0 && gone_at.is_none() {
        step(&mut s);
        match s.scene.figure(CREATURE) {
            Some(f) => {
                seen = true;
                if hidden_at.is_none() && f.sliders.desired(OPACITY) == 0.0 {
                    hidden_at = Some(s.host.time);
                }
            }
            None if seen => gone_at = Some(s.host.time),
            None => {}
        }
    }
    let hidden_at = hidden_at.expect("hidden as the behavior ends");
    let gone_at = gone_at.expect("taken away");
    let after = gone_at - hidden_at;
    assert!((2.5..4.0).contains(&after), "three seconds on: {after}");
}

/// A behavior that fades its object's children fades what the body holds as it starts, and brings
/// it back as it ends. Fixture: a server double asking at set times, the player holding a sword.
///
/// Behaviour: desktop.bodies.a-behavior-that-fades-children-fades-what-the-body-holds
#[test]
fn a_behavior_that_fades_children_fades_what_the_body_holds() {
    use auberean_client_sdk::protocol::physics::{ObjectEvent, Release};
    use auberean_client_sdk::protocol::types::{BehaviorParams, ParentLink};
    const SWORD: u64 = 0x8130_1B05_CC00_0077;
    const HOLD: u32 = 0x4100_0125;
    const OPACITY: u32 = 0x4000_0015;
    /// Flag 0x100 is the seventh of the flags followed by a word.
    const FADES_WORD: usize = 6;
    let sword = CreateObject {
        id: SWORD,
        visual: VisualDesc {
            bits: VisualDesc::PARENT,
            parent: 0x1F00_0056,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::PARENT,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            parent: ParentLink {
                parent: PLAYER,
                location: 1,
                stamp: 1,
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let mut fades = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR | BehaviorParams::WORD_BITS[FADES_WORD],
        behavior: HOLD,
        ..BehaviorParams::default()
    };
    fades.words[FADES_WORD] = 1;
    let again = fades.clone();
    let sender = |other_stamp: u16| ObjectStamp {
        id: PLAYER,
        instance_stamp: 1,
        other_stamp,
    };
    let later = [
        (
            9.0,
            ServerMessage::PlayBehavior(ObjectEvent {
                sender: sender(1),
                action: fades,
            }),
        ),
        (
            12.0,
            ServerMessage::LetGoBehavior(ObjectEvent {
                sender: sender(2),
                action: Release {
                    behavior: HOLD,
                    params: BehaviorParams::default(),
                },
            }),
        ),
        (
            16.0,
            ServerMessage::PlayBehavior(ObjectEvent {
                sender: sender(3),
                action: again,
            }),
        ),
    ];
    let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[sword], &later, 40.0);
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
    let asked = |s: &Shell<'_, DoubleHost>| {
        s.scene
            .figure(SWORD)
            .map(|f| f.sliders.desired(OPACITY))
            .expect("the sword")
    };
    while s.host.time < 8.0 {
        step(&mut s);
    }
    while asked(&s) != 1.0 && s.host.time < 14.0 {
        step(&mut s);
    }
    assert_eq!(asked(&s), 1.0, "back as the first ends");
    while asked(&s) != 0.0 && s.host.time < 20.0 {
        step(&mut s);
    }
    assert_eq!(asked(&s), 0.0, "faded as the second starts");
}

/// The ground under a body names a physics material whose properties give a terrain type: the body's
/// motion takes it, and its effects play their variants for it. Fixture: a server double, the
/// player at the camp.
///
/// Behaviour: desktop.bodies.the-ground-gives-a-body-its-terrain-type
#[test]
fn the_ground_gives_a_body_its_terrain_type() {
    let at = Vec3 {
        x: 122.842,
        y: 89.758,
        z: 79.968,
    };
    let script = script_until(0xC156_0035, at, Quat::IDENTITY, &[], &[], 30.0);
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
    s.event(&Event::Key {
        key: Key::W,
        down: true,
        repeat: false,
    });
    let until = s.host.time + 2.0;
    while s.host.time < until {
        step(&mut s);
    }
    let (motion, effects) = s.scene.player_terrain().expect("the player");
    assert_eq!(motion, effects, "both take the one terrain type");
    assert_ne!(
        motion >> 24,
        0,
        "the kinds are enumeration values: {motion:#x}"
    );
    assert_ne!(motion, 0x8000_0001, "not the no-terrain value");
}

/// An object fades in as it enters the world. Fixture: a server double making one object.
///
/// Behaviour: desktop.bodies.an-object-fades-in-as-it-enters-the-world
#[test]
fn an_object_fades_in_as_it_enters_the_world() {
    const CREATURE: u64 = 0x8130_1B05_CC00_018A;
    const OPACITY: u32 = 0x4000_0015;
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
                cell: HALL_CELL,
                frame: Frame {
                    origin: Vec3 {
                        x: HALL_AT.x + 3.0,
                        y: HALL_AT.y,
                        z: HALL_AT.z,
                    },
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[creature], &[], 30.0);
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
    while s.scene.figure(CREATURE).is_none() && s.host.time < 10.0 {
        step(&mut s);
    }
    let until = s.host.time + 1.0;
    while s.host.time < until {
        step(&mut s);
    }
    let f = s.scene.figure(CREATURE).expect("its figure");
    assert_eq!(f.sliders.desired(OPACITY), 1.0);
}

/// A body run into the teaching stone at the start, a little to one side, slides round it and goes
/// on, never inside it, though the stone reaches a cell beyond its own and was made before its cell
/// was read; it holds against the stone a moment first and passes where retail's body does, from
/// the rotation of exactly a quarter turn a new character is made with. Fixture: a server double
/// making the stone at its place in the hall, the player facing it from where a new character
/// starts; the points are the places retail's reports gave on the same walk.
///
/// Behaviour: desktop.bodies.a-body-goes-round-a-stone-in-the-way
#[test]
fn a_body_goes_round_a_stone_in_the_way() {
    const STONE: u64 = 0x8130_1B05_CC00_018A;
    const HALL: u32 = 0x0298_0109;
    const STONE_AT: Vec3 = Vec3 {
        x: 43.8744,
        y: 240.1739,
        z: -44.2061,
    };
    let stone = CreateObject {
        id: STONE,
        visual: VisualDesc {
            bits: VisualDesc::PARENT,
            parent: 0x1F00_02F4,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION | PhysicsDesc::MODE,
            mode: 0x4000_0001,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            position: Placement {
                cell: HALL,
                frame: Frame {
                    origin: STONE_AT,
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let at = Vec3 {
        x: 59.060577,
        y: 240.199,
        z: -44.606205,
    };
    let facing = Quat {
        w: std::f32::consts::FRAC_1_SQRT_2,
        x: 0.0,
        y: 0.0,
        z: std::f32::consts::FRAC_1_SQRT_2,
    };
    let script = script_until(HALL, at, facing, &[stone], &[], 60.0);
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
    s.event(&Event::Key {
        key: Key::W,
        down: true,
        repeat: false,
    });
    let until = s.host.time + 7.0;
    let mut nearest = f32::MAX;
    let mut retails = [
        (52.7607, 240.1602, f32::MAX),
        (45.6109, 240.1163, f32::MAX),
        (45.5686, 239.7589, f32::MAX),
        (45.4590, 239.4034, f32::MAX),
        (42.7566, 238.5059, f32::MAX),
        (31.7069, 238.4379, f32::MAX),
    ];
    while s.host.time < until {
        step(&mut s);
        let o = s.scene.status.origin;
        let (dx, dy) = (o.x - STONE_AT.x, o.y - STONE_AT.y);
        nearest = nearest.min((dx * dx + dy * dy).sqrt());
        for (x, y, closest) in &mut retails {
            let (dx, dy) = (o.x - *x, o.y - *y);
            *closest = closest.min((dx * dx + dy * dy).sqrt());
        }
    }
    let o = s.scene.status.origin;
    assert!(nearest > 1.5, "never inside the stone: {nearest}");
    for (x, y, closest) in retails {
        assert!(closest < 0.01, "by retail's ({x}, {y}): {closest} off");
    }
    assert!(o.x < STONE_AT.x - 5.0, "it went on past: {o:?}");
    assert!(o.y < STONE_AT.y - 1.5, "round its left: {o:?}");
}

/// An effect with a decal lays it where its object stands: on the land outdoors, on its cell's
/// ground inside, and it is drawn with the frame. Fixture: the server double asks the player's body
/// for the effect at a set time, at the camp and in the hall.
///
/// Behaviour: desktop.world.an-effects-decal-is-laid-where-its-object-stands
#[test]
fn an_effects_decal_is_laid_where_its_object_stands() {
    use auberean_client_sdk::protocol::physics::{ObjectEvent, TargetedFx};
    use auberean_draw::material::{draw_layers, instance_material_with, DEVICE_FLAGS};
    const LAYS: u32 = 0x8100_02C7;
    let (world, _) = data();
    let of_the_effect: Vec<_> = [0x3100_03DC, 0x3100_0519]
        .into_iter()
        .filter_map(|m| instance_material_with(&world.records, m, None, &[]))
        .flat_map(|m| draw_layers(&m, DEVICE_FLAGS))
        .collect();
    let drawn_with = |cell: u32, at: Vec3, asked: bool| {
        // Asked a few times: each is a random size, which may not reach the ground.
        let later: Vec<_> = [9.0, 11.0, 13.0, 15.0]
            .into_iter()
            .map(|at| {
                (
                    at,
                    ServerMessage::DoTargetedFx(ObjectEvent {
                        sender: ObjectStamp {
                            id: PLAYER,
                            instance_stamp: 1,
                            other_stamp: 0,
                        },
                        action: TargetedFx {
                            fx: LAYS,
                            scalar: 1.0,
                            target: 0,
                        },
                    }),
                )
            })
            .collect();
        let later: &[_] = if asked { &later } else { &[] };
        let script = script_until(cell, at, Quat::IDENTITY, &[], later, 40.0);
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
        let mut drawn = false;
        while s.host.time < 20.0 {
            step(&mut s);
            let now = s.host.time;
            drawn |= s.scene.frame(now).is_some_and(|f| {
                f.decals
                    .iter()
                    .any(|b| !b.vertices.is_empty() && of_the_effect.contains(&b.layer))
            });
        }
        drawn
    };
    for (cell, at) in [
        (
            0xC156_0035,
            Vec3 {
                x: 122.842,
                y: 89.758,
                z: 80.0,
            },
        ),
        (HALL_CELL, HALL_AT),
    ] {
        assert!(
            !drawn_with(cell, at, false),
            "nothing asked, cell {cell:#x}"
        );
        assert!(drawn_with(cell, at, true), "asked, cell {cell:#x}");
    }
}

/// The server asks the rest of what it can of a body, and the body does each: a global modifier and
/// a terrain modifier are kept (and the terrain modifiers cleared), a new motion description is
/// taken, a push toward an object gives it that speed away from the object, and a missile launched flies at the velocity given with
/// a target to fly at. Fixture: a server double playing the messages at set times, the player
/// standing in the hall, the human figure in the hall's cell.
///
/// Behaviour: desktop.bodies.a-body-does-the-rest-of-what-the-server-asks
#[test]
fn a_body_does_the_rest_of_what_the_server_asks() {
    use auberean_client_sdk::protocol::physics::{
        Missile, NoAction, ObjectEvent, TargetedImpulse, TerrainModifier, Word,
    };
    const DRUDGES_CELL: u32 = 0x0298_0121;
    const CREATURE: u64 = 0x8130_1B05_CC00_0191;
    let sender = ObjectStamp {
        id: CREATURE,
        instance_stamp: 1,
        other_stamp: 0,
    };
    let place = Placement {
        cell: DRUDGES_CELL,
        frame: Frame {
            origin: Vec3 {
                x: -3.9,
                y: 337.5,
                z: -44.7,
            },
            rotation: Quat::IDENTITY,
        },
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
            position: place,
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let at = Vec3 {
        x: 2.0,
        y: 330.0,
        z: -43.76,
    };
    let event = |t: f64, m: ServerMessage| (t, m);
    let later = [
        event(
            8.0,
            ServerMessage::GlobalModifier(ObjectEvent {
                sender,
                action: Word(7),
            }),
        ),
        event(
            8.0,
            ServerMessage::TerrainChange(ObjectEvent {
                sender,
                action: TerrainModifier {
                    terrain: 0x4000_0002,
                    modifier: 0x4000_0003,
                },
            }),
        ),
        event(
            8.5,
            ServerMessage::ClearTerrainChanges(ObjectEvent {
                sender,
                action: NoAction,
            }),
        ),
        event(
            9.0,
            ServerMessage::PushToward(ObjectEvent {
                sender,
                action: TargetedImpulse {
                    target: PLAYER,
                    magnitude: 6.0,
                    falloff: 0,
                    vertical: 0.0,
                },
            }),
        ),
        event(
            11.0,
            ServerMessage::InitializeMissile(ObjectEvent {
                sender,
                action: Missile {
                    position: place,
                    velocity: Vec3::new(0.0, 5.0, 0.0),
                    target: PLAYER,
                    target_offset: place,
                    target_velocity: Vec3::ZERO,
                    initialized: 1,
                },
            }),
        ),
    ];
    let script = script_until(DRUDGES_CELL, at, Quat::IDENTITY, &[creature], &later, 40.0);
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
    let mut cleared = false;
    let mut had_terrain = false;
    let mut pushed = (0.0f32, 0.0f32);
    while s.host.time < 11.0 {
        step(&mut s);
        if let Some(b) = s.scene.body(CREATURE) {
            if pushed.0 == 0.0 {
                pushed = (b.velocity.x, b.velocity.y);
            }
            let n = b.minterp.terrain_mods.len();
            had_terrain |= n != 0;
            cleared |= had_terrain && n == 0;
        }
    }
    let b = s.scene.body(CREATURE).expect("its body");
    assert_eq!(b.minterp.global_mod, 7, "the global modifier");
    assert!(had_terrain, "the terrain modifier was kept");
    assert!(cleared, "and cleared");
    // The player stands 5.9 m east and 7.1 m south of it.
    assert!(
        (pushed.0 + 3.83).abs() < 0.01 && (pushed.1 - 4.58).abs() < 0.01,
        "pushed away from the player at the speed asked: {pushed:?}"
    );
    while s.host.time < 13.0 {
        step(&mut s);
    }
    let b = s.scene.body(CREATURE).expect("its body");
    let g = b.missile.as_deref().expect("it flies");
    assert!(g.moving, "in flight");
    assert_eq!(g.target.object, PLAYER, "at its target");
}

/// What an object's description says of it beyond its sliders, it takes up: its four stamps, and,
/// described as a missile, its aim and that it flies. Fixture: an object described so, created in
/// the hall's cell.
///
/// Behaviour: desktop.bodies.an-object-takes-up-its-stamps-and-flight-from-its-description
#[test]
fn an_object_takes_up_its_stamps_and_flight_from_its_description() {
    const DRUDGES_CELL: u32 = 0x0298_0121;
    const CREATURE: u64 = 0x8130_1B05_CC00_0191;
    let place = Placement {
        cell: DRUDGES_CELL,
        frame: Frame {
            origin: Vec3 {
                x: -3.9,
                y: 337.5,
                z: -44.7,
            },
            rotation: Quat::IDENTITY,
        },
    };
    let creature = CreateObject {
        id: CREATURE,
        visual: VisualDesc {
            bits: VisualDesc::DID,
            did: HUMAN_MALE,
            ..VisualDesc::default()
        },
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION
                | PhysicsDesc::TARGET
                | PhysicsDesc::TARGET_POSITION
                | PhysicsDesc::TARGET_OFFSET
                | PhysicsDesc::MISSILE_MOVING
                | PhysicsDesc::MISSILE_ACTIVATED,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            position: place,
            target: PLAYER,
            target_position: place,
            target_offset: Vec3::new(0.0, 0.0, 1.0),
            stamps: [3, 4, 5, 6],
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let at = Vec3 {
        x: 2.0,
        y: 330.0,
        z: -43.76,
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
    while s.host.time < 12.0 {
        step(&mut s);
    }
    let b = s.scene.body(CREATURE).expect("its body");
    assert_eq!(b.update_times, [3, 4, 5, 6], "its stamps");
    let g = b.missile.as_deref().expect("it flies");
    assert!(g.activated && g.moving, "lit and in flight");
    assert_eq!(g.targeting.target, PLAYER, "at the object described");
    assert_eq!(g.targeting.target_offset.z, 1.0, "and its offset");
}

/// A player in deep water swims: on the bottom of a lake 20 m deep (block `0x9889`, its surface at
/// 139.45 m) the water lifts it, its water line coming to the surface, and holding the key moves it
/// at the water's speed, its description's, not the ground's or none.
///
/// Behaviour: desktop.world.a-player-in-deep-water-rises-and-swims
#[test]
fn a_player_in_deep_water_rises_and_swims() {
    const SURFACE: f32 = 139.45;
    let at = Vec3::new(22.0, 50.0, 125.0);
    let put = ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        time: 0.0,
        cell: 0x9889_000B,
        origin: at,
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: 0,
        position_stamp: 1,
        force_position_stamp: 0,
        teleport_stamp: 1,
        velocity: Vec3::new(0.0, 0.0, 0.0),
    });
    let script = script_until(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &[(8.0, put)], 40.0);
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
    while s.host.time < 25.0 {
        step(&mut s);
    }
    let floating = s.scene.status.origin;
    assert!(
        floating.z > SURFACE - 3.0 && floating.z < SURFACE,
        "the water line near the surface: {floating:?}"
    );
    s.event(&Event::Key {
        key: Key::W,
        down: true,
        repeat: false,
    });
    while s.host.time < 28.0 {
        step(&mut s);
    }
    let swum = s.scene.status.origin;
    let (dx, dy) = (swum.x - floating.x, swum.y - floating.y);
    let far = (dx * dx + dy * dy).sqrt();
    assert!(far > 8.0, "it swam {far} m: {floating:?} to {swum:?}");
    assert!(
        s.scene.status.speed > 4.0 && s.scene.status.speed < 5.0,
        "at the water's speed: {}",
        s.scene.status.speed
    );
}

/// With the camera under water the fog is the water's at the camera's depth, the light the same:
/// on the bottom of a lake 20 m deep (block `0x9889`, its surface at 139.45 m), the fog reaches as
/// far as the water's table says between its depths 0 (100 m, 0.8 of the sky) and 30 (48 m, half
/// the sky), by how deep the camera is; on dry land (the camp in block `0xC156`) it is the day's.
///
/// Behaviour: desktop.world.under-water-the-fog-is-the-waters-at-the-cameras-depth
#[test]
fn under_water_the_fog_is_the_waters_at_the_cameras_depth() {
    const SURFACE: f32 = 139.45;
    const CAMP: u32 = 0xC156_0000;
    let lake = 0x9889_000B;
    let bottom = Vec3 {
        x: 22.0,
        y: 50.0,
        z: 121.0,
    };
    let dry = ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        time: 0.0,
        cell: CAMP >> 16 << 16 | 0x0035,
        origin: Vec3::new(122.84, 89.76, 81.0),
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: 0,
        position_stamp: 1,
        force_position_stamp: 0,
        teleport_stamp: 1,
        velocity: Vec3::new(0.0, 0.0, 0.0),
    });
    let script = script_until(lake, bottom, Quat::IDENTITY, &[], &[(8.0, dry)], 30.0);
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
    let (mut under, mut dry_frames) = (0, 0);
    while s.host.time < 20.0 {
        let h = &mut s.host;
        h.time += 1.0 / 60.0;
        let t = LocalTime(1.0 + h.time);
        for d in h.double.tick(t) {
            h.inbox.push_back((d.from, d.bytes));
        }
        let out = s.frame();
        let Some(f) = out.world.filter(|f| !f.terrain.is_empty()) else {
            continue;
        };
        let (l, camera_z, reflection) = (f.light, f.camera.position.z, f.reflection);
        // The block of the cell the camera stands in, before and after the frame's moment.
        let stands = s
            .scene
            .camera_stands
            .map(|c| (c.previous.cell.0, c.current.cell.0));
        let block = match stands {
            Some((a, b)) if a >> 16 == b >> 16 => b >> 16,
            _ => continue,
        };
        if block == lake >> 16 {
            let depth = SURFACE - camera_z;
            assert!(depth > 5.0 && depth < 25.0, "the camera {depth} m down");
            assert!(reflection.is_none(), "no reflection under water");
            // Between the table's depths 0 and 30: the shallower's share (30 - depth) / 30.
            let k = (30.0 - depth) / 30.0;
            let max = 100.0 * k + 48.0 * (1.0 - k);
            let sky = 0.8 * k + 0.5 * (1.0 - k);
            assert!(
                (l.fog_max - max).abs() < 0.01
                    && (l.fog_sky - sky).abs() < 1e-4
                    && l.fog_min == 0.0,
                "{depth} m: {} to {}, sky {}",
                l.fog_min,
                l.fog_max,
                l.fog_sky
            );
            under += 1;
        } else if block == CAMP >> 16 {
            assert!(l.fog_max > 100.0, "the day's fog: {}", l.fog_max);
            dry_frames += 1;
        }
    }
    assert!(under > 60 && dry_frames > 60, "{under} {dry_frames}");
}

/// Outdoors the sky stands at the camera every frame, wherever the camera moves within its cell,
/// and moves on with the moment every frame, as do the light and fog; the ground keeps the
/// lighting it was lit with until the light's tick, 30 seconds after the last.
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

/// An admin moving the time of day has the ground lit for it on the next frame, long before the
/// light's tick, and the tick booked from there is shortened by the rate the world's time runs at:
/// at twice the rate, 15 seconds. (The player put 150 m up over the land of block `0x0298`, the
/// ground's first corner read every frame.)
///
/// Behaviour: desktop.world.a-moved-game-clock-relights-the-ground
#[test]
fn a_moved_game_clock_relights_the_ground_at_once_and_a_faster_one_more_often() {
    let cell = 0x0298_0001;
    let at = Vec3 {
        x: 12.0,
        y: 12.0,
        z: 150.0,
    };
    let script = script_until(cell, at, Quat::IDENTITY, &[], &[], 45.0);
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
    const MOVED: f64 = 12.0;
    let mut moved = false;
    // The ground's first corner by frame, after the move.
    let mut seen: Vec<(f64, [f32; 4])> = Vec::new();
    while s.host.time < 32.0 {
        let h = &mut s.host;
        h.time += 1.0 / 60.0;
        let t = LocalTime(1.0 + h.time);
        for d in h.double.tick(t) {
            h.inbox.push_back((d.from, d.bytes));
        }
        let now = s.host.time;
        if now >= MOVED && !moved {
            moved = true;
            let c = s.client.as_mut().expect("a client");
            let t = c.game_time();
            let clock = c.game_clock_mut().expect("the world's clock");
            assert!(clock.dilate(2.0, t), "the rate taken");
            clock.use_time(t);
            let to = (clock.time_of_day() + 0.25).fract();
            clock.alter_time_of_day(to, t);
        }
        let out = s.frame();
        let Some(f) = out.world else {
            continue;
        };
        if now >= MOVED - 1.0 && !f.terrain.is_empty() {
            seen.push((now, f.terrain[0].vertices[0].color));
        }
    }
    let changes: Vec<f64> = seen
        .windows(2)
        .filter(|w| w[0].1 != w[1].1)
        .map(|w| w[1].0)
        .collect();
    assert!(changes.len() >= 2, "lit again: {changes:?}");
    assert!(
        changes[0] >= MOVED && changes[0] - MOVED < 0.1,
        "lit on the frame after the move, not at {changes:?}"
    );
    assert!(
        (changes[1] - changes[0] - 15.0).abs() < 0.1,
        "the next tick half of 30 s on, not at {changes:?}"
    );
    assert!(
        !s.client
            .as_ref()
            .and_then(|c| c.game_clock())
            .expect("the world's clock")
            .needs_light(),
        "the clock told the land has been lit"
    );
}

/// A press of the pointer falls on the nearest body whose drawn triangles lie under it; failing any,
/// on a body whose box lies under it, so a press just off a body's arm still finds it. The player's
/// own body is found only by its triangles and behind everything else met, and not with the camera
/// at its eyes. (A figure standing just ahead of the player on the land of block `0xC156`; every
/// other pixel over their boxes.)
///
/// Behaviour: desktop.world.a-press-falls-on-the-body-under-it-or-failing-that-on-its-box
#[test]
fn a_press_falls_on_the_body_under_it_or_failing_that_on_its_box() {
    use auberean_scene::camera::{AvatarInFirstPerson, Press};
    use auberean_scene::seen::{self, SeenCamera};
    const CREATURE: u64 = 0x8130_1B05_CC00_0078;
    let cell = 0xC156_0035;
    let at = Vec3 {
        x: 122.84,
        y: 87.98,
        z: 80.2,
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
                cell,
                frame: Frame {
                    origin: Vec3 {
                        x: 122.84,
                        y: 90.98,
                        z: 80.2,
                    },
                    rotation: Quat::IDENTITY,
                },
            },
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    };
    let script = script_until(cell, at, Quat::IDENTITY, &[creature], &[], 20.0);
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
    let size = (s.size.0 as f32, s.size.1 as f32);
    let screen_box = |camera: &SeenCamera, body: &seen::SeenBody| {
        let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for c in body.corners(body.bounds.expect("drawn")) {
            let (x, y, _) = camera.project(c, size.0, size.1).expect("in front");
            lo = (lo.0.min(x), lo.1.min(y));
            hi = (hi.0.max(x), hi.1.max(y));
        }
        (lo, hi)
    };
    let (mut by_box, mut by_triangles, mut player_only, mut player_box, mut before_player) =
        (0, 0, 0, 0, 0);
    for _ in 0..600 {
        step(&mut s);
        let drawn = |id: u64| s.scene.seen.body(id).is_some_and(|b| b.bounds.is_some());
        if drawn(CREATURE) && drawn(PLAYER) {
            break;
        }
    }
    let objects = s.frame().world.expect("the world").objects.clone();
    let camera = s.scene.seen.camera.expect("a camera");
    let body = |id: u64| s.scene.seen.body(id).cloned().expect("drawn");
    let (them, me) = (body(CREATURE), body(PLAYER));
    let range_of = |id: u64| vec![(id, s.scene.drawn_batches(id).expect("drawn"))];
    let (theirs, mine) = (range_of(CREATURE), range_of(PLAYER));
    let (a, b) = (screen_box(&camera, &them), screen_box(&camera, &me));
    let lo = (a.0 .0.min(b.0 .0).max(1.0), a.0 .1.min(b.0 .1).max(1.0));
    let hi = (
        a.1 .0.max(b.1 .0).min(size.0 - 1.0),
        a.1 .1.max(b.1 .1).min(size.1 - 1.0),
    );
    let inside = |r: ((f32, f32), (f32, f32)), p: (f32, f32), margin: f32| {
        p.0 >= r.0 .0 + margin
            && p.0 <= r.1 .0 - margin
            && p.1 >= r.0 .1 + margin
            && p.1 <= r.1 .1 - margin
    };
    let mut y = lo.1.floor();
    while y <= hi.1 {
        let mut x = lo.0.floor();
        while x <= hi.0 {
            let p = (x, y);
            let picked = s.scene.pick_world(p, s.size);
            let on_them = seen::pick(&camera, size, p, &objects, &theirs).is_some();
            let on_me = seen::pick(&camera, size, p, &objects, &mine).is_some();
            if on_them {
                assert_eq!(picked, Some(CREATURE), "on the figure at {p:?}");
                by_triangles += 1;
                before_player += usize::from(on_me);
            } else if on_me {
                assert_eq!(picked, Some(PLAYER), "on the player alone at {p:?}");
                player_only += 1;
            } else if inside(a, p, 2.0) {
                assert_eq!(picked, Some(CREATURE), "in the figure's box at {p:?}");
                by_box += 1;
            } else if inside(b, p, 2.0) && !inside(a, p, -2.0) {
                assert_eq!(picked, None, "in the player's box alone at {p:?}");
                player_box += 1;
            }
            x += 2.0;
        }
        y += 2.0;
    }
    assert!(
        by_triangles > 20 && by_box > 20 && player_only > 20 && player_box > 20,
        "{by_triangles} {by_box} {player_only} {player_box}"
    );
    assert!(before_player > 0, "the figure is met through the player");
    // With the camera at the player's eyes looking down at its body, drawn: never the player.
    s.scene.camera.options.avatar_in_first_person = AvatarInFirstPerson::Always;
    s.scene.camera.press(Press::FirstPerson);
    for _ in 0..30 {
        step(&mut s);
    }
    s.scene.camera.press(Press::MouseLook);
    s.scene.camera.mouse(0.0, 600.0);
    for _ in 0..30 {
        step(&mut s);
    }
    let objects = s.frame().world.expect("the world").objects.clone();
    let camera = s.scene.seen.camera.expect("a camera");
    let mine = range_of_at_eyes(&s);
    let mut on_me = 0;
    for y in (0..s.size.1).step_by(8) {
        for x in (0..s.size.0).step_by(8) {
            #[allow(clippy::cast_precision_loss)]
            let p = (x as f32, y as f32);
            on_me += usize::from(seen::pick(&camera, size, p, &objects, &mine).is_some());
            assert_ne!(s.scene.pick_world(p, s.size), Some(PLAYER), "at {p:?}");
        }
    }
    assert!(on_me > 0, "the ray meets the body drawn at {camera:?}");
}

/// The batches of the player's body in the last world frame.
fn range_of_at_eyes(s: &Shell<'_, DoubleHost>) -> Vec<(u64, std::ops::Range<usize>)> {
    vec![(PLAYER, s.scene.drawn_batches(PLAYER).expect("drawn"))]
}

/// The groove level the radar works out reaches the music: the play screen's request becomes a
/// sound order, and the order sets the level the music's patterns follow.
///
/// Behaviour: desktop.sound.the-radars-groove-level-reaches-the-music
#[test]
fn the_radars_groove_level_reaches_the_music() {
    use auberean_shell::interface::SoundOrder;
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(DevLogin::new(
        server,
        "tester".into(),
        "t".into(),
        Some("tester".into()),
    )));
    step(&mut s);
    assert_eq!(s.sound.manager.groove(), 0);
    s.sound.request(SoundOrder::Groove(12));
    assert_eq!(s.sound.manager.groove(), 12);
}

/// Outdoors the camera's own object keeps its sphere, a quarter of a meter, off the land: tilted to
/// look up from under the ground, the camera stands that far above where its line of sight met the
/// land. (The player on the land of block `0xC156`, the mouse pushing the view all the way up.)
///
/// Behaviour: desktop.camera.outdoors-the-cameras-own-object-keeps-it-off-the-land
#[test]
fn outdoors_the_cameras_own_object_keeps_it_off_the_land() {
    use auberean_scene::camera::Press;
    let cell = 0xC156_0035;
    let at = Vec3 {
        x: 122.84,
        y: 87.98,
        z: 80.2,
    };
    let script = script_until(cell, at, Quat::IDENTITY, &[], &[], 20.0);
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
    s.scene.camera.press(Press::MouseLook);
    for _ in 0..120 {
        s.scene.camera.mouse(0.0, -400.0);
        step(&mut s);
    }
    let camera = s.scene.seen.camera.expect("a camera");
    let feet = s.scene.status.origin;
    let above = camera.position.z - feet.z;
    assert!(camera.pitch > 10.0, "looking up: {}", camera.pitch);
    // The land falls a little between the feet and the camera.
    assert!(above > 0.1 && above < 0.5, "{above} m above the feet");
}

/// The world hidden while a story plays is not drawn, and is drawn again when shown.
///
/// Behaviour: desktop.world.the-world-hidden-is-not-drawn
#[test]
fn the_world_hidden_is_not_drawn_and_shown_again_is() {
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
    assert!(s.frame().world.is_some(), "drawn");
    s.scene.show_world(false);
    step(&mut s);
    assert!(s.frame().world.is_none(), "hidden");
    s.scene.show_world(true);
    step(&mut s);
    assert!(s.frame().world.is_some(), "drawn again");
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

/// While a move the server ordered runs the player's body to a place 8 m ahead, the client's
/// reports on the way carry the motion that move asks of the body (running forward, the forward
/// byte near its top), not the keys, which are up: the others the server tells see the player
/// running.
///
/// Behaviour: desktop.shell.the-reports-carry-the-motion-the-body-was-asked
#[test]
fn the_reports_carry_the_motion_a_move_ordered_asks() {
    use auberean_client_sdk::protocol::physics::{MoveOrder, ObjectEvent};
    use auberean_client_sdk::protocol::types::{MoveTarget, MovementParams};
    let order = ServerMessage::MoveTo(ObjectEvent {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        action: MoveOrder {
            params: MovementParams {
                flags: MovementParams::MAX_SUCCESS | 0x2,
                desired_velocity: 1.0,
                fail_distance: f32::MAX,
                desired_distance: 0.0,
                max_success_distance: 0.2,
                min_success_distance: 0.0,
                target: MoveTarget::Place(Placement {
                    cell: HALL_CELL,
                    frame: Frame {
                        origin: Vec3::new(HALL_AT.x, HALL_AT.y + 8.0, HALL_AT.z),
                        rotation: Quat::IDENTITY,
                    },
                }),
                offset: Vec3::ZERO,
                offset_id: 0,
                waypoints: Vec::new(),
                context: 0,
            },
            stamp: 1,
        },
    });
    // The double's clock runs a second ahead of the shell's: the order comes at 8 s.
    let script = script_then(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &[(9.0, order)]);
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
    while s.host.time < 12.0 {
        step(&mut s);
    }
    let sent: Vec<(f64, u32, Vec3)> = s
        .client
        .as_ref()
        .expect("the session")
        .sent()
        .iter()
        .filter_map(|m| match &m.message {
            ClientMessage::Position(p) => Some((m.at - 1.0, p.heading_motion, p.origin)),
            _ => None,
        })
        .collect();
    // On the way there: from a meter out until the body first comes within a meter of the place.
    let running: Vec<_> = sent
        .iter()
        .take_while(|(_, _, o)| o.y < HALL_AT.y + 7.0)
        .filter(|(_, _, o)| o.y > HALL_AT.y + 1.0)
        .collect();
    assert!(running.len() >= 3, "reports on the way: {sent:?}");
    for (t, hm, o) in &running {
        let forward = (hm >> 8) & 0xFF;
        assert!(
            forward >= 0xF0,
            "at {t:.2} ({o:?}) the report asks forward {forward:#04x}"
        );
    }
}

/// A run Q latched ends once a move the server ordered takes the body, so the body does not run
/// on when the move is done.
///
/// Behaviour: desktop.controls.a-move-of-the-bodys-own-ends-the-latched-run
#[test]
fn a_move_ordered_ends_the_latched_run() {
    use auberean_client_sdk::protocol::physics::{MoveOrder, ObjectEvent};
    use auberean_client_sdk::protocol::types::{MoveTarget, MovementParams};
    let order = ServerMessage::MoveTo(ObjectEvent {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        action: MoveOrder {
            params: MovementParams {
                flags: MovementParams::MAX_SUCCESS | 0x2,
                desired_velocity: 1.0,
                fail_distance: f32::MAX,
                desired_distance: 0.0,
                max_success_distance: 0.2,
                min_success_distance: 0.0,
                target: MoveTarget::Place(Placement {
                    cell: HALL_CELL,
                    frame: Frame {
                        origin: Vec3::new(HALL_AT.x, HALL_AT.y + 8.0, HALL_AT.z),
                        rotation: Quat::IDENTITY,
                    },
                }),
                offset: Vec3::ZERO,
                offset_id: 0,
                waypoints: Vec::new(),
                context: 0,
            },
            stamp: 1,
        },
    });
    // The double's clock runs a second ahead of the shell's: the order comes at 8 s.
    let script = script_then(HALL_CELL, HALL_AT, Quat::IDENTITY, &[], &[(9.0, order)]);
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
    // The order reaches the body a little before 6 s.
    while s.host.time < 5.0 {
        step(&mut s);
    }
    assert_eq!(s.phase(), Phase::World);
    assert!(!s.scene.player_moving_to());
    for down in [true, false] {
        s.event(&Event::Key {
            key: Key::Q,
            down,
            repeat: false,
        });
    }
    step(&mut s);
    assert!(s.controls.autorun(), "Q latched the run");
    while !s.scene.player_moving_to() {
        assert!(s.controls.autorun(), "only the move ends the run");
        assert!(s.host.time < 12.0, "the order never took the body");
        step(&mut s);
    }
    step(&mut s);
    assert!(!s.controls.autorun(), "the move ended the run");
}

/// A teleport ends a latched run, so the player does not run on from where it arrives. Fixture: a
/// server double putting the player down again with a new teleport stamp at a set time.
///
/// Behaviour: desktop.controls.a-teleport-ends-the-latched-run
#[test]
fn a_teleport_ends_the_latched_run() {
    let put = ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        time: 0.0,
        cell: HALL_CELL,
        origin: HALL_AT,
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: 0,
        position_stamp: 1,
        force_position_stamp: 0,
        teleport_stamp: 1,
        velocity: Vec3::new(0.0, 0.0, 0.0),
    });
    let script = script_until(
        HALL_CELL,
        HALL_AT,
        Quat::IDENTITY,
        &[],
        &[(12.0, put)],
        30.0,
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
    while s.host.time < 6.0 {
        step(&mut s);
    }
    assert_eq!(s.phase(), Phase::World);
    for down in [true, false] {
        s.event(&Event::Key {
            key: Key::Q,
            down,
            repeat: false,
        });
    }
    step(&mut s);
    assert!(s.controls.autorun(), "Q latched the run");
    let stamp = |s: &Shell<'_, DoubleHost>| s.client.as_ref().map_or(0, |c| c.body.teleport_stamp);
    while stamp(&s) == 0 {
        assert!(s.controls.autorun(), "only the teleport ends the run");
        assert!(s.host.time < 20.0, "the teleport never came");
        step(&mut s);
    }
    step(&mut s);
    assert!(!s.controls.autorun(), "the teleport ended the run");
}

/// The window losing the keyboard lets each action in force go as its key would: a strafe held
/// then ends, and the next reports carry no sideways motion; the run Q latched goes on, its
/// reports still running forward.
///
/// Behaviour: desktop.shell.losing-the-keyboard-ends-held-moves-and-keeps-a-latched-run
#[test]
fn losing_the_keyboard_ends_a_held_strafe_and_keeps_the_latched_run() {
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(DevLogin::new(
        server,
        "tester".into(),
        "t".into(),
        Some("tester".into()),
    )));
    let key = |s: &mut Shell<'_, DoubleHost>, key: Key, down: bool| {
        s.event(&Event::Key {
            key,
            down,
            repeat: false,
        });
    };
    while s.host.time < 8.0 {
        step(&mut s);
    }
    assert_eq!(s.phase(), Phase::World);
    key(&mut s, Key::Q, true);
    key(&mut s, Key::Q, false);
    key(&mut s, Key::Z, true);
    while s.host.time < 8.6 {
        step(&mut s);
    }
    s.event(&Event::Focus(false));
    while s.host.time < 9.6 {
        step(&mut s);
    }
    let sent: Vec<(f64, u32, u32)> = s
        .client
        .as_ref()
        .expect("the session")
        .sent()
        .iter()
        .filter_map(|m| match &m.message {
            ClientMessage::Position(p) => Some((m.at - 1.0, p.heading_motion, p.flags)),
            _ => None,
        })
        .collect();
    let sideways = |hm: u32| (hm >> 16) & 0xFF;
    let forward = |hm: u32| (hm >> 8) & 0xFF;
    let before: Vec<_> = sent
        .iter()
        .filter(|(t, _, _)| (8.1..8.6).contains(t))
        .collect();
    assert!(!before.is_empty(), "reports while strafing: {sent:?}");
    for (t, hm, _) in &before {
        assert!(
            sideways(*hm) < 0x40 && forward(*hm) > 0xC0,
            "at {t:.2} running and strafing left (the two cut to length 1): {hm:#010x}"
        );
    }
    let after: Vec<_> = sent
        .iter()
        .filter(|(t, _, _)| (8.7..9.6).contains(t))
        .collect();
    assert!(
        after.len() >= 3,
        "reports after the keyboard went: {sent:?}"
    );
    for (t, hm, flags) in &after {
        assert_eq!(
            sideways(*hm),
            0x7F,
            "at {t:.2} the strafe ended: {hm:#010x}"
        );
        assert!(
            forward(*hm) >= 0xF0 && flags & 4 != 0,
            "at {t:.2} still running: {hm:#010x} {flags:#x}"
        );
    }
}

/// In the hall, the kind most of its cells name dominates and its place music (`2C000169`, a
/// primary piece) plays from an ambient sound's object, its piece on the performance.
///
/// Behaviour: audio.ambient.a-session-hears-its-place
#[test]
fn in_the_hall_the_interiors_place_music_plays() {
    const HALL_KIND: u32 = 0x8100_000F;
    const HALL_MUSIC: u32 = 0x2C00_0169;
    let script = script();
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with_sound(
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
    assert_eq!(s.sound.ambient().dominant_kind(), Some(HALL_KIND));
    let playing = s.sound.manager.playing();
    assert!(
        playing.iter().any(|&(_, id, _)| id == HALL_MUSIC),
        "the hall's music plays: {playing:x?}"
    );
    assert!(s.sound.manager.stats.pieces >= 1, "its piece started");
    assert!(s
        .sound
        .manager
        .emitters()
        .iter()
        .any(|e| auberean_audio::ambient::is_emitter(*e)));
}

/// An effect played on an object plays its sound, made by the object and heard: the player's
/// effect `0x81000001` plays its sound setting `2A000016` from the player's body, through the
/// device, on the frame it is played.
///
/// Behaviour: audio.effects.an-effect-plays-its-sound-from-its-object
#[test]
fn effects_played_on_objects_play_their_sounds() {
    const EFFECT: u32 = 0x8100_0001;
    const SOUND: u32 = 0x2A00_0016;
    let script = script();
    let server =
        ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with_sound(
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
    let device = s.host.sound.clone().expect("a device");
    let _ = device.pull(4800);
    // Only the place's ambient music plays yet: no sound setting, and every maker an ambient
    // sound's object.
    let playing = s.sound.manager.playing();
    assert!(
        playing.iter().all(|&(_, id, _)| id >> 24 == 0x2C),
        "nothing but music plays yet: {playing:x?}"
    );
    assert!(
        s.sound
            .manager
            .emitters()
            .iter()
            .all(|e| auberean_audio::ambient::is_emitter(*e)),
        "made only by the ambient sound's objects"
    );
    s.scene.do_fx(PLAYER, EFFECT, 1.0);
    step(&mut s);
    let playing = s.sound.manager.playing();
    assert!(
        playing.iter().any(|&(_, id, _)| id == SOUND),
        "the effect's sound plays: {playing:x?}"
    );
    let objects: Vec<u64> = s
        .sound
        .manager
        .emitters()
        .into_iter()
        .filter(|e| !auberean_audio::ambient::is_emitter(*e))
        .collect();
    assert_eq!(objects, vec![PLAYER], "made by the player");
    let heard = device.pull(4800);
    assert!(
        heard.iter().any(|v| v.abs() > 1e-3),
        "a tenth of a second of it is heard"
    );
}

/// The graphics preferences the player applies reach the world's next frame: in the training hall
/// at a fresh install's quality the frame's textures are taken at high detail, trilinear, not
/// anisotropic; applied at very low detail with neither filtering and the materials at low, the
/// next frame takes its textures so and draws the hall's materials' low layers in place of their
/// high ones.
///
/// Behaviour: desktop.scene.the-applied-graphics-preferences-reach-the-next-frame
#[test]
fn the_applied_graphics_preferences_reach_the_next_frame() {
    use auberean_draw::quality::{key, Graphics};
    use auberean_draw::scene::Textures;
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell_with(
        script(),
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
    let layers = |s: &mut Shell<'_, DoubleHost>| {
        s.host.time += 1.0 / 60.0;
        let out = s.frame();
        let f = out.world.expect("the hall drawn");
        let layers: Vec<_> = f
            .objects
            .iter()
            .filter(|b| b.shown())
            .map(|b| b.layer.clone())
            .collect();
        (f.textures, layers)
    };
    let (textures, high) = layers(&mut s);
    assert_eq!(
        textures,
        Textures {
            detail: 3,
            trilinear: true,
            anisotropic: false
        }
    );
    let mut g = Graphics::first_run();
    for (k, v) in [
        (key::TEXTURE_DETAIL, 0.0),
        (key::TRILINEAR_FILTERING, 0.0),
        (key::MATERIAL_DETAIL, 0.0),
    ] {
        assert!(g.set(k, v));
    }
    s.scene.init_preferences(&g);
    let (textures, low) = layers(&mut s);
    assert_eq!(
        textures,
        Textures {
            detail: 0,
            trilinear: false,
            anisotropic: false
        }
    );
    assert_eq!(low.len(), high.len(), "a layer for each");
    let changed = high.iter().zip(&low).filter(|(h, l)| h != l).count();
    assert!(
        changed * 2 > high.len(),
        "{changed} of {} changed",
        high.len()
    );
}

/// How the game is shown goes to the host and into the settings as the interface asks: a flip of
/// full screen is the interface's to answer, and what it answers is carried out.
///
/// Behaviour: desktop.shell.the-host-shows-the-game-as-the-interface-asks
#[test]
fn the_host_shows_the_game_as_the_interface_asks() {
    use auberean_shell::host::TestHost;
    use auberean_shell::Presentation;
    struct Flips;
    impl Interface for Flips {
        fn update(&mut self, _game: &Game<'_>, _requests: &mut Vec<Request>) {}
        fn toggle_full_screen(&mut self, requests: &mut Vec<Request>) {
            requests.push(Request::Presentation(Presentation {
                size: (800, 600),
                full_screen: true,
                triple_buffering: true,
                sync_to_display: false,
                antialiasing: false,
            }));
        }
    }
    let (world, data) = data();
    let mut s = Shell::new(TestHost::default(), world, data.clone(), Box::new(Flips));
    s.toggle_full_screen();
    assert_eq!(s.host.presentations.len(), 1);
    assert_eq!(s.settings.presentation(), s.host.presentations[0]);
    assert_eq!(s.settings.window, (800, 600));
    assert!(s.settings.full_screen);
}
