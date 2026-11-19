//! The interfaces' ways of moving the player against the server's executable on loopback: the
//! game's own steering, and the orbit camera's camera-based and character-based movement, each
//! driving the same session of controls through the SDK's driver and its world without a window,
//! as a headless client drives it. The session walks, runs, backs up, turns in place, strafes
//! both ways, runs while turning (by the keys, and under the orbit camera while the camera
//! swings), jumps standing and running, starts and stops, crosses a block's edge and back, and
//! closes in on a creature and fights it. The server's corrections of the player's own place are
//! counted: a place forced on it with a newer stamp, a placement or throw it did not ask for, and
//! the server's own log of the reports it did not take. None may come. Each activity's reports
//! and corrections are printed as a table row (run with `--nocapture` to read them). Its tests are
//! the session's (`session::ways_of_moving`).

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use auberean_client_sdk::contract::control::{movement, Control, ControlId};
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::ClientMessage;
use auberean_client_sdk::sim::orbit::{MovementMode, OrbitSettings};
use auberean_client_sdk::sim::{Walk, WorldData};
use auberean_client_sdk::{install, Action, Client, Driver, Port};
use dereth_primitives::num::math;
use yalain_testkit::authored;
use yalain_testkit::session::{self as play, Drive};

use super::session::{must, scratch, serving, start_server, LIFE};

/// Shift: with forward, a walk.
const SHIFT: ControlId = ControlId(0x4000_000E);
/// A frame: about a sixtieth of a second, on the real clock.
const FRAME: Duration = Duration::from_millis(16);
/// A block's side, meters.
const BLOCK: f32 = 160.0;
/// How many times a session is played before a lost fight fails it.
const FIGHTS: u32 = 3;

/// The server's log lines that say it did not take a report of the player's place.
const REFUSALS: [&str; 4] = [
    "a place not taken",
    "a place the player's body did not reach",
    "a position that is no place",
    "a place reported with an old teleport stamp",
];

/// A way of moving the player: the game's own, or the orbit camera's two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scheme {
    Game,
    Camera,
    Character,
}

impl Scheme {
    fn settings(self) -> Option<OrbitSettings> {
        let movement = match self {
            Self::Game => return None,
            Self::Camera => MovementMode::Camera,
            Self::Character => MovementMode::Character,
        };
        Some(OrbitSettings {
            movement,
            ..OrbitSettings::default()
        })
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Game => "game's own",
            Self::Camera => "orbit, camera-based",
            Self::Character => "orbit, character-based",
        }
    }
}

/// The install's world, read once.
fn world() -> &'static WorldData {
    static WORLD: OnceLock<WorldData> = OnceLock::new();
    WORLD.get_or_init(|| {
        let dir = auberean_dat::testing::require_target_install();
        install::world_data(&dir).expect("the install's world")
    })
}

/// A UDP socket on loopback as the driver's port.
struct Socket {
    socket: UdpSocket,
    buf: Vec<u8>,
}

impl Port for Socket {
    fn receive(&mut self, _now: LocalTime) -> Option<(SocketAddr, Vec<u8>)> {
        match self.socket.recv_from(&mut self.buf) {
            Ok((n, from)) => Some((from, self.buf[..n].to_vec())),
            // Nothing more waiting, or a reset from a port not answering yet: the client resends.
            Err(e) if e.kind() == ErrorKind::WouldBlock => None,
            Err(_) => None,
        }
    }

    fn send(&mut self, to: SocketAddr, bytes: &[u8], _now: LocalTime) {
        let _ = self.socket.send_to(bytes, to);
    }
}

/// The client on the SDK's driver, its body walked through the world with no window, on the real
/// clock; the controls it holds, and the camera's part a mouse would play.
struct Schemed {
    driver: Driver,
    port: Socket,
    walk: Walk<'static>,
    scheme: Scheme,
    start: Instant,
    held: BTreeSet<u32>,
    /// The way the orbit camera is turned to each frame, as a mouse would turn it.
    aim: Option<f32>,
    /// Degrees a second the orbit camera swings, as a mouse dragged sideways would.
    swing: f32,
    last: f64,
    /// The most forward the controls asked in a frame since this was last cleared.
    most_forward: f32,
    /// The server's orders to move the player's body: its move-to stamp's changes.
    move_orders: u32,
}

impl Drive for Schemed {
    fn client(&mut self) -> &mut Client {
        self.driver.client_mut()
    }

    fn client_ref(&self) -> &Client {
        self.driver.client()
    }

    fn step(&mut self) {
        let now = self.start.elapsed().as_secs_f64();
        #[allow(clippy::cast_possible_truncation)]
        let dt = (now - self.last) as f32;
        self.last = now;
        self.driver.receive(now, &mut self.port);
        let controls = self.driver.take_control_frame();
        self.most_forward = self.most_forward.max(controls.forward);
        if let Some(o) = self.walk.orbit_mut() {
            if let Some(h) = self.aim {
                o.heading = h.rem_euclid(360.0);
            }
            o.heading = (o.heading + self.swing * dt).rem_euclid(360.0);
        }
        let stamp = self.driver.client().body.move_to_stamp;
        self.walk.update(self.driver.client_mut(), &controls, now);
        self.driver.finish(&mut self.port);
        if self.driver.client().body.move_to_stamp != stamp {
            self.move_orders += 1;
        }
        std::thread::sleep(FRAME);
    }

    fn now(&self) -> f64 {
        self.last
    }
}

impl Schemed {
    /// Hold exactly these controls from now on: the others let go.
    fn hold(&mut self, ids: &[ControlId]) {
        let want: BTreeSet<u32> = ids.iter().map(|c| c.0).collect();
        for gone in self.held.difference(&want).copied().collect::<Vec<_>>() {
            self.driver
                .act(Action::Control(Control::end(ControlId(gone))));
        }
        for new in want.difference(&self.held).copied().collect::<Vec<_>>() {
            self.driver
                .act(Action::Control(Control::begin(ControlId(new))));
        }
        self.held = want;
    }

    /// Hold `ids` for `seconds`, then let them all go and stand for half a second.
    fn hold_for(&mut self, ids: &[ControlId], seconds: f64) {
        self.hold(ids);
        play::wait(self, seconds);
        self.hold(&[]);
        play::wait(self, 0.5);
    }

    /// A jump pressed, with `ids` held: the press for its frame.
    fn jump(&mut self, ids: &[ControlId]) {
        let mut with: Vec<ControlId> = ids.to_vec();
        with.push(movement::JUMP);
        self.hold(&with);
        self.step();
        self.hold(ids);
    }

    /// Where the player stands on the world's ground (east, north), across blocks, and its heading.
    fn ground(&self) -> ([f32; 2], f32) {
        let b = &self.driver.client().body;
        #[allow(clippy::cast_precision_loss)]
        let (bx, by) = ((b.cell >> 24) as f32, ((b.cell >> 16) & 0xFF) as f32);
        (
            [bx * BLOCK + b.origin.x, by * BLOCK + b.origin.y],
            b.heading,
        )
    }

    /// Steer to within `reach` meters of `to` on the world's ground, as a player of this scheme
    /// would: the game's own and character-based by the turns, then forward; camera-based by
    /// turning the camera that way and running forward.
    fn go_to(&mut self, to: [f32; 2], reach: f32, seconds: f64) -> Result<(), String> {
        let end = self.now() + seconds;
        loop {
            let (at, heading) = self.ground();
            let (dx, dy) = (to[0] - at[0], to[1] - at[1]);
            if math::hypotf(dx, dy) <= reach {
                break;
            }
            if self.now() >= end {
                self.hold(&[]);
                self.aim = None;
                return Err(format!(
                    "not within {reach} m of {to:?} in {seconds} s: at {at:?}"
                ));
            }
            let bearing = math::atan2f(dx, dy).to_degrees().rem_euclid(360.0);
            if self.scheme == Scheme::Camera {
                self.aim = Some(bearing);
                self.hold(&[movement::FORWARD]);
            } else {
                let off = (bearing - heading + 540.0).rem_euclid(360.0) - 180.0;
                let mut ids = Vec::new();
                if off > 6.0 {
                    ids.push(movement::TURN_RIGHT);
                } else if off < -6.0 {
                    ids.push(movement::TURN_LEFT);
                }
                if off.abs() < 40.0 {
                    ids.push(movement::FORWARD);
                }
                self.hold(&ids);
            }
            self.step();
        }
        self.hold(&[]);
        self.aim = None;
        play::wait(self, 0.5);
        Ok(())
    }

    /// Steer to within `reach` meters of where the client has object `id`.
    fn close_in(&mut self, id: u64, reach: f32) -> Result<(), String> {
        let place = |c: &Client| {
            c.model.object(id).and_then(|o| o.place).map(|p| {
                #[allow(clippy::cast_precision_loss)]
                let (bx, by) = ((p.cell >> 24) as f32, ((p.cell >> 16) & 0xFF) as f32);
                [bx * BLOCK + p.origin.x, by * BLOCK + p.origin.y]
            })
        };
        let to = place(self.client_ref()).ok_or("the target is not placed")?;
        self.go_to(to, reach, 30.0)
    }
}

/// An activity of the session: what it does with the controls.
type Activity = Box<dyn Fn(&mut Schemed) -> Result<(), String>>;

/// What the server and the client did over one activity.
#[derive(Debug, Default)]
struct Row {
    activity: &'static str,
    seconds: f64,
    reports: usize,
    jumps: usize,
    /// Reports whose heading moved while no motion was asked.
    turned_standing: usize,
    /// Reports asking a sideways motion, and a backward one.
    sideways: usize,
    backward: usize,
    forced: u32,
    placed: u32,
    thrown: u32,
    move_orders: u32,
    refusals: usize,
}

impl Row {
    /// Every place the server put the player at: a forced place and a throw each put it
    /// somewhere, and so does anything else it did not ask for.
    fn corrections(&self) -> u32 {
        self.placed.max(self.forced + self.thrown)
    }
}

/// The counts the client and the server's log hold now, to measure an activity from.
struct Mark {
    at: f64,
    sent: usize,
    forced: u32,
    placed: u32,
    thrown: u32,
    move_orders: u32,
    refusals: usize,
}

fn refusals(dir: &Path) -> usize {
    let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
    log.lines()
        .filter(|l| REFUSALS.iter().any(|r| l.contains(r)))
        .count()
}

fn mark(s: &Schemed, dir: &Path) -> Mark {
    let c = s.client_ref();
    Mark {
        at: s.now(),
        sent: c.sent().len(),
        forced: c.body.forced_moves(),
        placed: c.body.placements(),
        thrown: c.body.thrown().0,
        move_orders: s.move_orders,
        refusals: refusals(dir),
    }
}

/// The row for `activity` from `m` to now. A placement the activity asked for (a portal's) is
/// left out by the caller.
fn row(s: &Schemed, dir: &Path, m: &Mark, activity: &'static str) -> Row {
    let c = s.client_ref();
    let mut r = Row {
        activity,
        seconds: s.now() - m.at,
        forced: c.body.forced_moves().wrapping_sub(m.forced),
        placed: c.body.placements().wrapping_sub(m.placed),
        thrown: c.body.thrown().0.wrapping_sub(m.thrown),
        move_orders: s.move_orders - m.move_orders,
        refusals: refusals(dir) - m.refusals,
        ..Row::default()
    };
    let mut last_heading: Option<u32> = None;
    for sent in &c.sent()[m.sent..] {
        let ClientMessage::Position(p) = &sent.message else {
            continue;
        };
        r.reports += 1;
        let heading = p.heading_motion >> 24;
        let (side, ahead) = (
            (p.heading_motion >> 16) & 0xFF,
            (p.heading_motion >> 8) & 0xFF,
        );
        if p.flags & 2 != 0 {
            r.jumps += 1;
        }
        if side != 127 {
            r.sideways += 1;
        }
        if ahead < 127 {
            r.backward += 1;
        }
        if side == 127 && ahead == 127 && last_heading.is_some_and(|h| h != heading) {
            r.turned_standing += 1;
        }
        last_heading = Some(heading);
    }
    r
}

/// One activity measured into `rows`.
fn measure(
    s: &mut Schemed,
    dir: &Path,
    rows: &mut Vec<Row>,
    activity: &'static str,
    run: impl FnOnce(&mut Schemed) -> Result<(), String>,
) -> Result<(), String> {
    let m = mark(s, dir);
    let r = run(s);
    rows.push(row(s, dir, &m, activity));
    r.map_err(|e| format!("{activity}: {e}"))
}

/// The session played with `scheme`: the rows, or none when the player lost the fight.
fn plays(scheme: Scheme, name: &str) -> Option<Vec<Row>> {
    let dir = scratch(&format!("yalain-live-schemes-{name}"));
    let (port, _server, ()) = serving(&dir, |port| (start_server(&dir, port, LIFE), ()));
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    socket.set_nonblocking(true).expect("nonblocking");
    let login = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut walk = Walk::new(world());
    walk.set_scheme(scheme.settings());
    let mut s = Schemed {
        driver: Driver::new(play::client(login, name)),
        port: Socket {
            socket,
            buf: vec![0; 65_536],
        },
        walk,
        scheme,
        start: Instant::now(),
        held: BTreeSet::new(),
        aim: None,
        swing: 0.0,
        last: 0.0,
        most_forward: 0.0,
        move_orders: 0,
    };
    must(play::character_list(&mut s), &dir);
    let me = must(play::create(&mut s, &format!("Scheme{name}")), &dir);
    must(play::enter(&mut s, me), &dir);
    assert_eq!(s.client_ref().body.cell, authored::HALL, "at the start");
    must(
        play::see(&mut s, &[authored::SHARD, authored::SENTINEL]),
        &dir,
    );
    must(play::wield(&mut s, authored::SWORD), &dir);
    let mut rows = Vec::new();

    // In the hall: close in on the sentinel and fight it.
    let sentinel = play::find(s.client_ref(), authored::SENTINEL).expect("a sentinel");
    let fought = measure(&mut s, &dir, &mut rows, "close in and fight", |s| {
        s.close_in(sentinel.id, 1.5)?;
        s.client().act(Action::Select {
            target: sentinel.id,
        });
        play::attack_until_dead(s, sentinel.id)
    });
    match fought {
        Err(e) if e.contains(play::LOST_THE_FIGHT) => {
            eprintln!("{e}; played again");
            return None;
        }
        r => must(r, &dir),
    }
    s.client().act(Action::Attack { start: false });

    // To the town through the shard: the trip's placement is the server's to make.
    let shard = play::find(s.client_ref(), authored::SHARD).expect("the shard");
    // The shard is solid: the body stops short of the spot beside it.
    let at = authored::SHARD_AT;
    let spot = [
        (authored::HALL >> 24) as f32 * BLOCK + at[0],
        ((authored::HALL >> 16) & 0xFF) as f32 * BLOCK + at[1],
    ];
    must(
        measure(&mut s, &dir, &mut rows, "close in on the portal", |s| {
            s.go_to(spot, 3.0, 30.0)
        }),
        &dir,
    );
    must(play::take_portal(&mut s, shard.id, authored::TOWN), &dir);
    play::wait(&mut s, 2.0);
    let (home, _) = s.ground();

    let back = |s: &mut Schemed| s.go_to(home, 1.0, 30.0);
    let steps: Vec<(&'static str, Activity)> = vec![
        (
            "walk",
            Box::new(|s: &mut Schemed| {
                s.most_forward = 0.0;
                s.hold(&[SHIFT]);
                s.step();
                s.hold_for(&[SHIFT, movement::FORWARD], 2.0);
                let walked = s.most_forward;
                if walked >= 0.99 {
                    return Err(format!("Shift with forward asked {walked}, not a walk"));
                }
                Ok(())
            }),
        ),
        (
            "run",
            Box::new(|s: &mut Schemed| {
                s.hold_for(&[movement::FORWARD], 2.0);
                Ok(())
            }),
        ),
        (
            "back up",
            Box::new(|s: &mut Schemed| {
                s.hold_for(&[movement::BACKWARD], 1.5);
                Ok(())
            }),
        ),
        (
            "turn in place",
            Box::new(|s: &mut Schemed| {
                s.hold_for(&[movement::TURN_LEFT], 1.5);
                s.hold_for(&[movement::TURN_RIGHT], 1.5);
                Ok(())
            }),
        ),
        (
            "strafe both ways",
            Box::new(|s: &mut Schemed| {
                s.hold_for(&[movement::STRAFE_RIGHT], 1.5);
                s.hold_for(&[movement::STRAFE_LEFT], 1.5);
                Ok(())
            }),
        ),
        (
            "run while turning",
            Box::new(|s: &mut Schemed| {
                s.hold_for(&[movement::FORWARD, movement::TURN_RIGHT], 2.5);
                Ok(())
            }),
        ),
        (
            "run, camera swinging",
            Box::new(|s: &mut Schemed| {
                s.swing = 120.0;
                s.hold_for(&[movement::FORWARD], 2.5);
                s.swing = 0.0;
                Ok(())
            }),
        ),
        (
            "strafe-run",
            Box::new(|s: &mut Schemed| {
                s.hold_for(&[movement::FORWARD, movement::STRAFE_RIGHT], 1.5);
                Ok(())
            }),
        ),
        (
            "jump",
            Box::new(|s: &mut Schemed| {
                s.jump(&[]);
                play::wait(s, 2.0);
                s.hold(&[movement::FORWARD]);
                play::wait(s, 0.5);
                s.jump(&[movement::FORWARD]);
                play::wait(s, 1.5);
                s.hold(&[]);
                play::wait(s, 1.0);
                Ok(())
            }),
        ),
        (
            "start and stop",
            Box::new(|s: &mut Schemed| {
                for _ in 0..6 {
                    s.hold(&[movement::FORWARD]);
                    play::wait(s, 0.4);
                    s.hold(&[]);
                    play::wait(s, 0.4);
                }
                Ok(())
            }),
        ),
    ];
    for (activity, step) in &steps {
        must(
            measure(&mut s, &dir, &mut rows, activity, |s| step(s)),
            &dir,
        );
        must(
            measure(&mut s, &dir, &mut rows, "back to the start", back),
            &dir,
        );
    }

    // Across the block's southern edge and back.
    let south = [home[0], home[1] - 25.0];
    must(
        measure(&mut s, &dir, &mut rows, "across a block's edge", |s| {
            s.go_to(south, 1.0, 30.0)?;
            let crossed = s.client_ref().body.cell >> 16;
            if crossed == authored::TOWN >> 16 {
                return Err(format!("still in block {crossed:04X}"));
            }
            s.go_to(home, 1.0, 30.0)
        }),
        &dir,
    );
    must(play::log_off(&mut s), &dir);
    Some(rows)
}

/// The rows as a table, and the totals of the corrections and refusals.
fn report(scheme: Scheme, rows: &[Row]) -> (u32, usize) {
    eprintln!("| {} | seconds | reports | a second | jumps | turned standing | sideways | backward | server moves | corrections | refusals |", scheme.name());
    eprintln!("|---|---|---|---|---|---|---|---|---|---|---|");
    let mut merged: Vec<Row> = Vec::new();
    for r in rows {
        match merged.iter_mut().find(|m| m.activity == r.activity) {
            Some(m) => {
                m.seconds += r.seconds;
                m.reports += r.reports;
                m.jumps += r.jumps;
                m.turned_standing += r.turned_standing;
                m.sideways += r.sideways;
                m.backward += r.backward;
                m.forced += r.forced;
                m.placed += r.placed;
                m.thrown += r.thrown;
                m.move_orders += r.move_orders;
                m.refusals += r.refusals;
            }
            None => merged.push(Row { ..*r }),
        }
    }
    let (mut corrections, mut refused) = (0, 0);
    for r in &merged {
        eprintln!(
            "| {} | {:.1} | {} | {:.1} | {} | {} | {} | {} | {} | {} ({} forced, {} thrown) | {} |",
            r.activity,
            r.seconds,
            r.reports,
            r.reports as f64 / r.seconds.max(0.001),
            r.jumps,
            r.turned_standing,
            r.sideways,
            r.backward,
            r.move_orders,
            r.corrections(),
            r.forced,
            r.thrown,
            r.refusals,
        );
        corrections += r.corrections();
        refused += r.refusals;
    }
    (corrections, refused)
}

/// The session with `scheme`, played again from the start on a server of its own when the
/// player loses the fight, asserted to draw no correction or refusal.
pub(crate) fn draws_no_corrections(scheme: Scheme, name: &str) {
    for _ in 0..FIGHTS {
        if let Some(rows) = plays(scheme, name) {
            let (corrections, refused) = report(scheme, &rows);
            assert_eq!(
                (corrections, refused),
                (0, 0),
                "{}: the server corrected the player or refused its reports",
                scheme.name()
            );
            return;
        }
    }
    panic!("the player lost the fight {FIGHTS} times");
}
