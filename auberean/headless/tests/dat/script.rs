//! Scripts run with the install's data: controls the action map reads, and the player's body
//! walked through the human training hall with no window, against a server double on a fixed-step
//! clock.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::time::Duration;

use auberean_client_sdk::contract::orbit::MovementMode;
use auberean_client_sdk::plugins::{MemoryStorage, Package, PluginHook, VmClock};
use auberean_client_sdk::protocol::ClientMessage;
use auberean_client_sdk::runtime::calls::function;
use auberean_client_sdk::sim::Walk;
use auberean_client_sdk::{Driver, FixedStep, Phase};
use auberean_headless::builtin::{self, Flags};
use auberean_headless::plugins::{host, DirSource};
use auberean_headless::run::Runner;
use auberean_headless::script::{self, Command};
use auberean_primitives::Quat;

use crate::walk::{data, double_port, script_at, HALL_AT};

/// A frame: a sixtieth of a second; the most a run is given: a minute.
const FRAME: f64 = 1.0 / 60.0;
const FRAMES: usize = 3600;

/// A run of `commands` against the double putting the player in the hall facing north; the body
/// walked through the hall's world when `walking`.
fn runner(commands: Vec<Command>, walking: bool) -> (Runner<'static>, crate::walk::DoublePort) {
    let (world, data) = data();
    let (port, settings) = double_port(script_at(HALL_AT, Quat::IDENTITY));
    let driver = Driver::connect(settings, data.clone());
    let walk = walking.then(|| Walk::new(world));
    (Runner::new(driver, walk, commands), port)
}

/// Where each timed `position` line told the body stood: the seconds since the entry, the cell,
/// the place and the heading.
fn positions(lines: &[String]) -> Vec<(f64, u32, [f32; 3], f32)> {
    lines
        .iter()
        .filter(|l| l.contains("position: ") && !l.starts_with("position:"))
        .map(|l| {
            let words: Vec<&str> = l
                .split(|c: char| c.is_whitespace() || "(),".contains(c))
                .filter(|w| !w.is_empty())
                .collect();
            let n = |i: usize| words[i].parse::<f32>().expect("a number");
            (
                words[0].parse().expect("seconds"),
                u32::from_str_radix(words[2], 16).expect("a cell"),
                [n(3), n(4), n(5)],
                n(7),
            )
        })
        .collect()
}

/// With the action map read from the data, `begin` presses a control and `end` lets it go, `hold`
/// keeps it pressed for its seconds and `press` for one frame: the movement the controls in force
/// ask for each frame says so.
///
/// Behaviour: session.headless.a-control-pressed-held-and-let-go-is-in-force-for-its-frames
#[test]
fn a_control_begun_held_pressed_and_let_go_is_in_force_for_its_frames() {
    let commands = script::parse(
        "enter\nbegin forward\ntick 2\nend forward\ntick 2\nhold strafe-right 0.5\ntick 1\n\
         press backward\ntick 3\nquit\n",
    )
    .expect("a script");
    let (mut r, mut port) = runner(commands, false);
    let mut clock = FixedStep::new(FRAME);
    let mut asked = Vec::new();
    for _ in 0..FRAMES {
        if !r.step(&mut clock, &mut port).expect("a step") {
            break;
        }
        if r.phase() == Phase::World {
            let c = r.controls();
            asked.push((c.forward, c.sideways));
        }
    }
    let count = |f: &dyn Fn(&(f32, f32)) -> bool| asked.iter().filter(|a| f(a)).count();
    assert_eq!(
        count(&|a| a.0 > 0.0),
        2,
        "forward the two frames it was in: {asked:?}"
    );
    assert_eq!(
        count(&|a| a.0 < 0.0),
        1,
        "backward the frame it was pressed"
    );
    assert_eq!(
        count(&|a| a.1 > 0.0),
        30,
        "strafing right for half a second"
    );
    assert_eq!(asked.last(), Some(&(0.0, 0.0)), "nothing in force after");
}

/// The messages of the server's own calls the client made, by function.
fn calls(d: &Driver) -> Vec<u32> {
    d.sent()
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::ServerCall(c) => Some(c.function.0),
            _ => None,
        })
        .collect()
}

/// With no window, a script walks the player's body forward through the hall to a wall (more than
/// ten meters north), selects the player, uses it and says a line:
/// the view says what is selected, and the client makes the server's calls for the selection and
/// the use.
///
/// Behaviour: session.headless.a-script-walks-selects-uses-and-says-with-no-window
#[test]
fn a_script_walks_the_body_through_the_hall_selects_the_player_uses_it_and_says_a_line() {
    let commands = script::parse(
        "login\nenter\nrun 5\nposition\nwalk forward 10\nposition\nselect self\ntick 1\nuse\n\
         say well met\ntick 1\nsnapshot\nquit\n",
    )
    .expect("a script");
    let (mut r, mut port) = runner(commands, true);
    let mut clock = FixedStep::new(FRAME);
    assert!(r.run_to_end(&mut clock, &mut port, FRAMES).expect("runs"));
    let lines = r.take_lines();
    let p = positions(&lines);
    assert_eq!(p.len(), 2, "{lines:#?}");
    let ((_, _, from, _), (_, _, to, _)) = (p[0], p[1]);
    assert!(
        to[1] - from[1] > 10.0,
        "north through the hall: {from:?} {to:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("selection: 0x2130000000000001")),
        "the player selected: {lines:#?}"
    );
    assert_eq!(calls(r.driver()), [function::SELECT, function::USE]);
    assert!(lines.iter().any(|l| l.ends_with("saying: well met")));
}

/// The built-in script of `--walk` does what the flag did: from the second second the body runs
/// forward for two seconds, then lets the run go and turns right for one.
///
/// Behaviour: session.headless.the-walk-flag-runs-and-turns-with-no-window
#[test]
fn the_built_in_walk_runs_the_body_forward_for_two_seconds_then_turns_for_one() {
    let commands = builtin::script(&Flags {
        walk: true,
        seconds: 6.0,
        ..Flags::default()
    });
    let (mut r, mut port) = runner(commands, true);
    let mut clock = FixedStep::new(FRAME);
    assert!(r.run_to_end(&mut clock, &mut port, FRAMES).expect("runs"));
    let lines = r.take_lines();
    let p = positions(&lines);
    let at: Vec<f64> = p.iter().map(|(t, ..)| *t).collect();
    assert_eq!(at.len(), 3, "{lines:#?}");
    assert!(
        at.iter()
            .zip([2.0, 4.0, 5.0])
            .all(|(t, want)| (want..want + 0.05).contains(t)),
        "{at:?}"
    );
    let flat = |a: [f32; 3], b: [f32; 3]| ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    assert!(
        p[1].2[1] - p[0].2[1] > 3.0,
        "two seconds running north: {p:?}"
    );
    assert!(flat(p[1].2, p[2].2) < 1.0, "turning, the run let go: {p:?}");
    let turned = (p[2].3 - p[1].3).rem_euclid(360.0);
    assert!((45.0..180.0).contains(&turned), "turned right: {p:?}");
    assert!(lines.iter().any(|l| l.contains("logging off")));
}

/// A clock that stands still: no plugin's entry takes any time.
#[derive(Debug, Default)]
struct Still(Cell<Duration>);

impl VmClock for Still {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

/// A camera a plugin chooses once the player is in the world is the walk's: the orbit camera with
/// the plugin's way of moving.
///
/// Behaviour: plugins.headless.a-plugins-camera-is-the-walks
#[test]
fn a_camera_a_plugin_chooses_is_the_walks_from_the_next_frame() {
    let commands = script::parse("login\nenter\nrun 1\nquit\n").expect("a script");
    let (mut r, mut port) = runner(commands, true);
    let mut h = host(
        Box::<Still>::default(),
        Box::<MemoryStorage>::default(),
        DirSource::default(),
    );
    let mut files = BTreeMap::new();
    files.insert(
        "plugin.toml".to_owned(),
        b"id = \"test.eye\"\nname = \"Eye\"\nversion = \"1.0.0\"\napi = \"0.7\"\ngame = \"auberean\"\n"
            .to_vec(),
    );
    files.insert(
        "main.lua".to_owned(),
        b"on('session.enter_world', function() act.camera({ movement = 'character' }) end)"
            .to_vec(),
    );
    h.request_load(Package {
        origin: "test.eye".to_owned(),
        files,
    });
    r.driver_mut().install_hook(Box::new(PluginHook(h)));
    let mut clock = FixedStep::new(FRAME);
    assert!(r.run_to_end(&mut clock, &mut port, FRAMES).expect("runs"));
    let orbit = r.walk().and_then(Walk::orbit).expect("the orbit camera");
    assert_eq!(orbit.settings.movement, MovementMode::Character);
}
