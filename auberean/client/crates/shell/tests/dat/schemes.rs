//! The orbit camera and its movement, in the hall of the September 2013 world against a server
//! double: with no window, on the SDK's driver and its world without a window, and in the shell,
//! where an interface asks for it; and the player's orbit preferences laid over what it asks; and
//! the orbit camera a plugin chooses.

use std::collections::{BTreeMap, VecDeque};
use std::net::SocketAddr;
use std::time::Duration;

use auberean_client_sdk::contract::control::{movement, Control};
use auberean_client_sdk::contract::event::SessionEvent;
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::plugin_api::{
    Event as PluginEvent, HostCalls, Limits, ScriptEngine, ScriptError, ScriptErrorKind,
    ScriptInstance, Value,
};
use auberean_client_sdk::plugins::{HostConfig, MemoryStorage, Package, PluginHost, VmClock};
use auberean_client_sdk::sim::Walk;
use auberean_client_sdk::{Action, ClientSettings, Driver, FixedStep, GameEvent};
use auberean_options::page::{section, Change, Request as PageRequest};
use auberean_shell::dev::DevLogin;
use auberean_shell::input::{Event, Key};
use auberean_shell::interface::{
    CameraRequest, Game, Interface, MovementMode, OrbitSettings, Phase, Request,
};
use auberean_testkit::double::ServerDouble;

use crate::session::{camera_steps, data, script, shell, shell_with, step, EAST_EDGE};

/// Degrees apart, either way round.
fn apart(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// The double as the port of a driver with no window: what the double sends in a frame is read in
/// that frame.
struct DoublePort {
    double: ServerDouble,
    inbox: VecDeque<(SocketAddr, Vec<u8>)>,
    ticked: Option<f64>,
}

impl auberean_client_sdk::Port for DoublePort {
    fn receive(&mut self, now: LocalTime) -> Option<(SocketAddr, Vec<u8>)> {
        if self.ticked != Some(now.0) {
            self.ticked = Some(now.0);
            self.inbox
                .extend(self.double.tick(now).into_iter().map(|d| (d.from, d.bytes)));
        }
        self.inbox.pop_front()
    }

    fn send(&mut self, to: SocketAddr, bytes: &[u8], now: LocalTime) {
        self.double
            .datagram(to, SocketAddr::from(([127, 0, 0, 1], 50_000)), bytes, now);
    }
}

/// The body's heading, degrees, and where it stood on the ground (east, north).
type Stood = (f32, (f32, f32));

/// A run with no window under the orbit camera moving as `mode` says: the client logs in and
/// enters the hall facing north; at 7.5 s the orbit camera is chosen, and once it has been set
/// behind the player it is turned a quarter left, to look west; forward is held from 8 s to 9 s.
/// The body's heading and place before forward is pressed and when it is let go.
fn walk_under(mode: MovementMode) -> (Stood, Stood) {
    let (world, data) = data();
    let double = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
    let settings = ClientSettings {
        login_server: double.login_addr(),
        account: b"tester".to_vec(),
        ticket: b"t".to_vec(),
        timestamp: 0,
        session_key: [7; 64],
        script_sends: false,
    };
    let mut port = DoublePort {
        double,
        inbox: VecDeque::new(),
        ticked: None,
    };
    let mut driver = Driver::connect(settings, data.clone());
    let mut walk = Walk::new(world);
    let mut clock = FixedStep::new(1.0 / 60.0);
    let mut asked = false;
    let mut before = None;
    let body = |w: &Walk<'_>| {
        let p = w.player().expect("the player's body");
        let o = p.body.position.frame.origin;
        (p.body.heading, (o.x, o.y))
    };
    while clock.now() < 9.0 {
        let t = clock.now();
        if (7.5..7.52).contains(&t) {
            walk.set_scheme(Some(OrbitSettings {
                movement: mode,
                ..OrbitSettings::default()
            }));
        }
        if (7.6..7.62).contains(&t) {
            let o = walk.orbit_mut().expect("the orbit camera");
            assert!(o.placed, "set behind the player by now");
            o.heading = (o.heading - 90.0).rem_euclid(360.0);
        }
        if (8.0..8.02).contains(&t) {
            before = Some(body(&walk));
            driver.act(Action::Control(Control::begin(movement::FORWARD)));
        }
        driver.frame_walking(&mut clock, &mut port, &mut walk);
        for e in driver.take_events() {
            if let (GameEvent::Session(SessionEvent::Characters(list)), false) = (e, asked) {
                let character = list.characters.first().expect("a character").id;
                driver.act(Action::PlayCharacter { character });
                asked = true;
            }
        }
    }
    (before.expect("forward pressed"), body(&walk))
}

/// With no window, under the orbit camera turned a quarter left of the player's facing: forward,
/// camera-based, turns the body to face where the camera looks (west) and runs it there;
/// character-based, it runs the body on the way it faced (north), the camera left as it was.
///
/// Behaviour: desktop.orbit.with-no-window-forward-runs-the-way-each-movement-says
#[test]
fn with_no_window_forward_runs_where_the_camera_looks_only_camera_based() {
    let ((h0, (x0, y0)), (h1, (x1, y1))) = walk_under(MovementMode::Camera);
    assert!(apart(h0, 0.0) < 1.0, "facing north first: {h0}");
    assert!(apart(h1, 270.0) < 1.0, "turned west, where it looks: {h1}");
    assert!(
        x0 - x1 > 3.0 && (y1 - y0).abs() < 1.0,
        "ran west: ({x0}, {y0}) to ({x1}, {y1})"
    );

    let ((h0, (x0, y0)), (h1, (x1, y1))) = walk_under(MovementMode::Character);
    assert!(
        apart(h0, 0.0) < 1.0 && apart(h1, 0.0) < 1.0,
        "still north: {h0} {h1}"
    );
    assert!(
        y1 - y0 > 3.0 && (x1 - x0).abs() < 1.0,
        "ran north: ({x0}, {y0}) to ({x1}, {y1})"
    );
}

/// An interface that logs in and, in the world, asks for the orbit camera every frame.
struct AsksForTheOrbit(DevLogin, OrbitSettings);

impl Interface for AsksForTheOrbit {
    fn update(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        self.0.update(game, requests);
        if game.phase == Phase::World {
            requests.push(Request::Camera(CameraRequest::Scheme(Some(self.1))));
        }
    }
}

/// The shell over the double with an interface that asks for the orbit camera, in the world.
fn orbiting(settings: OrbitSettings) -> auberean_shell::Shell<'static, crate::session::DoubleHost> {
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(AsksForTheOrbit(
        DevLogin::new(server, "tester".into(), "t".into(), Some("tester".into())),
        settings,
    )));
    while s.host.time < 7.5 {
        step(&mut s);
    }
    s
}

fn key(s: &mut auberean_shell::Shell<'_, crate::session::DoubleHost>, key: Key, down: bool) {
    s.event(&Event::Key {
        key,
        down,
        repeat: false,
    });
}

/// An interface that asks for the orbit camera shows the world through it: the camera stands
/// behind the player at the orbit camera's distance, looking the orbit camera's way, and turned a
/// quarter left (west) it stands east of the player. The player's W, camera-based, then turns the
/// body west and runs it there, in the window's scene as with no window.
///
/// Behaviour: desktop.orbit.an-interface-shows-the-world-through-the-orbit-camera
#[test]
fn an_interface_asking_for_the_orbit_camera_shows_the_world_through_it() {
    let mut s = orbiting(OrbitSettings::default());
    let o = s.scene.orbit.expect("the orbit camera in use");
    assert!(
        o.placed && apart(o.heading, 0.0) < 1.0,
        "behind the player: {o:?}"
    );
    assert!(!s.scene.camera.mouse_look, "no mouse look under it");
    let body = s.scene.status.origin;
    let eye = |s: &auberean_shell::Shell<'_, _>| {
        s.scene
            .camera_stands
            .expect("the camera placed")
            .current
            .frame
            .origin
    };
    let at = eye(&s);
    let back = ((body.x - at.x).powi(2) + (body.y - at.y).powi(2)).sqrt();
    assert!(
        at.y < body.y && back > 2.0 && back <= o.distance + 0.01,
        "behind: {at:?} {body:?}"
    );
    s.scene.orbit.as_mut().expect("in use").heading = 270.0;
    step(&mut s);
    let at = eye(&s);
    assert!(
        at.x > body.x + 2.0,
        "east of the player, looking west: {at:?} {body:?}"
    );
    let start = s.scene.status.origin;
    key(&mut s, Key::W, true);
    while s.host.time < 8.5 {
        step(&mut s);
    }
    key(&mut s, Key::W, false);
    step(&mut s);
    let end = s.scene.status.origin;
    assert!(apart(s.scene.status.heading, 270.0) < 1.0, "turned west");
    assert!(start.x - end.x > 2.0, "ran west: {start:?} to {end:?}");
}

/// The player's orbit preferences (`[Orbit] Movement`, `ReverseX`, `ReverseY`), as applied, are
/// laid over what the interface asks of the orbit camera, the camera keeping its place: applied
/// character-based, W runs the body on its way, not the camera's.
///
/// Behaviour: desktop.orbit.the-orbit-preferences-are-laid-over-what-the-interface-asks
#[test]
fn the_orbit_preferences_are_laid_over_what_the_interface_asks() {
    let mut s = orbiting(OrbitSettings::default());
    s.scene.orbit.as_mut().expect("in use").heading = 270.0;
    s.preferences
        .handle(&PageRequest::Attach(vec![section::ORBIT]));
    let rows = s
        .preferences
        .listing()
        .and_then(|l| l.sections.iter().find(|(at, _)| *at == section::ORBIT))
        .map(|(_, rows)| rows.clone())
        .expect("the orbit section listed");
    assert_eq!(rows.len(), 3, "movement and the two reversals: {rows:?}");
    for row in rows {
        let set = match row {
            Change::Choice(mut c) => {
                assert!(c.set_value(MovementMode::Character.value()));
                Change::Choice(c)
            }
            Change::Flag(mut f) => {
                f.set_value(true);
                Change::Flag(f)
            }
            Change::Number(n) => Change::Number(n),
        };
        s.preferences.handle(&PageRequest::Change(set));
    }
    s.preferences.handle(&PageRequest::Apply(section::ORBIT));
    step(&mut s);
    let o = s.scene.orbit.expect("still in use");
    assert_eq!(o.settings.movement, MovementMode::Character);
    assert!(o.settings.reverse_x && o.settings.reverse_y);
    assert!(apart(o.heading, 270.0) < 1.0, "its place kept");
    let start = s.scene.status.origin;
    key(&mut s, Key::W, true);
    while s.host.time < 8.5 {
        step(&mut s);
    }
    let end = s.scene.status.origin;
    assert!(apart(s.scene.status.heading, 0.0) < 1.0, "still north");
    assert!(end.y - start.y > 2.0, "ran north: {start:?} to {end:?}");
}

/// A clock that stands still: no plugin's entry takes any time.
#[derive(Debug)]
struct Still;

impl VmClock for Still {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
}

/// An engine whose plugin chooses the orbit camera as it loads, as `act.camera({})` would.
#[derive(Debug)]
struct ChoosesTheOrbit;

impl ScriptEngine for ChoosesTheOrbit {
    fn name(&self) -> &'static str {
        "chooses-the-orbit"
    }

    fn instantiate(&self, _: Limits) -> Result<Box<dyn ScriptInstance>, ScriptError> {
        Ok(Box::new(ChoosesTheOrbit))
    }
}

impl ScriptInstance for ChoosesTheOrbit {
    fn load(&mut self, host: &mut dyn HostCalls, _: &str, _: &str) -> Result<(), ScriptError> {
        host.call("game.camera", &[Value::map()])
            .map(|_| ())
            .map_err(|e| ScriptError::new(ScriptErrorKind::Runtime, format!("{e:?}")))
    }

    fn deliver(&mut self, _: &mut dyn HostCalls, _: &PluginEvent) -> Result<Value, ScriptError> {
        Ok(Value::Nil)
    }

    fn memory_used(&self) -> usize {
        0
    }
}

/// A plugin choosing the orbit camera has the world shown through it, under an interface that
/// asks for none.
///
/// Behaviour: desktop.orbit.a-plugin-choosing-the-orbit-camera-shows-the-world-through-it
#[test]
fn a_plugin_choosing_the_orbit_camera_shows_the_world_through_it() {
    let server = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
    let mut s = shell(Box::new(DevLogin::new(
        server,
        "tester".into(),
        "t".into(),
        Some("tester".into()),
    )));
    let mut host = PluginHost::new(
        HostConfig::new("test-host", "1.0"),
        Box::new(Still),
        Box::<MemoryStorage>::default(),
    )
    .with_engine(Box::new(ChoosesTheOrbit));
    let mut files = BTreeMap::new();
    files.insert(
        "plugin.toml".to_owned(),
        b"id = \"test.eye\"\nname = \"Eye\"\nversion = \"1.0.0\"\napi = \"0.7\"\ngame = \"auberean\"\n"
            .to_vec(),
    );
    files.insert("main.lua".to_owned(), Vec::new());
    host.request_load(Package {
        origin: "test.eye".to_owned(),
        files,
    });
    s.set_plugins(host);
    while s.host.time < 7.5 {
        step(&mut s);
    }
    assert!(
        s.plugin_host().is_some_and(|h| !h.plugins().is_empty()),
        "the plugin loaded"
    );
    assert!(s.scene.orbit.is_some(), "the orbit camera in use");
}

/// Under the orbit camera, running east over land that bends between the body's ticks and over a
/// block's edge, the camera moves on smoothly every frame, never further in a frame than the body
/// moves in a tick: behind the player, and turned by the mouse with the right button held.
///
/// Behaviour: desktop.orbit.the-orbit-camera-moves-smoothly-over-the-land-and-a-blocks-edge
#[test]
fn the_orbit_camera_moves_smoothly_over_the_land_and_a_blocks_edge() {
    use auberean_shell::input::Button;
    for mouse in [false, true] {
        let (cell, at) = EAST_EDGE;
        let script = crate::session::script_at(cell, at, auberean_primitives::Quat::IDENTITY);
        let server =
            ServerDouble::new(script.clone(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7).login_addr();
        let mut s = shell_with(
            script,
            Box::new(AsksForTheOrbit(
                DevLogin::new(server, "tester".into(), "t".into(), Some("tester".into())),
                OrbitSettings::default(),
            )),
        );
        while s.host.time < 7.5 {
            step(&mut s);
        }
        s.scene.orbit.as_mut().expect("in use").heading = 90.0;
        step(&mut s);
        if mouse {
            s.event(&Event::Button {
                button: Button::Right,
                down: true,
            });
        }
        key(&mut s, Key::W, true);
        let (most, ran, blocks) = camera_steps(&mut s, 11.0, |s| {
            if mouse {
                s.event(&Event::Motion { dx: 0.25, dy: 0.0 });
            }
        });
        assert_eq!(blocks, [0xC156, 0xC256], "ran east over the block's edge");
        assert!(
            most < ran + 0.05,
            "the camera stepped {most} m in a frame, the body {ran} m in a tick (mouse: {mouse})"
        );
    }
}
