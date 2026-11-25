//! A follow behind a scripted leader, on a scripted body, at a fixed frame clock: the leader runs
//! a course at a set pace and its place reaches the follower in updates a fifth of a second
//! apart; the body runs and walks by the run lock and the walk key, turns with the turn keys at a
//! set rate, and takes a heading at once.

use std::collections::BTreeSet;

use dereth_client_model::cmd::CommandInterp;
use dereth_client_model::World;
use dereth_client_runtime::frame_hook::{HookFrame, NavKey, NavRequest, Places};
use dereth_primitives::{ObjectId, Position};

use super::{Goal, Nav};
use crate::places;

const LEADER: ObjectId = ObjectId(0x8000_0040);
const PLUGIN: &str = "test.follow";

/// A view with nothing in it: the follow reads only places.
#[derive(Debug)]
struct NoView;
impl dereth_client_contract::GameView for NoView {}

/// The middle of a landblock well inside the world, metres from its corner.
const BASE: f64 = 100.0 * places::BLOCK + 96.0;

/// The outdoor place `x` metres east and `y` metres north of [`BASE`], facing `heading`.
fn place(x: f64, y: f64, heading: f64) -> Position {
    let (gx, gy) = (BASE + x, BASE + y);
    let mut p = places::from_coords(
        gy / places::UNIT - places::ORIGIN,
        gx / places::UNIT - places::ORIGIN,
        0.0,
    )
    .expect("inside the world");
    #[allow(clippy::cast_possible_truncation)]
    dereth_primitives::frame::set_heading(&mut p.frame, heading as f32);
    p
}

fn xy(p: &Position) -> (f64, f64) {
    let (gx, gy, _) = places::global(p);
    (gx - BASE, gy - BASE)
}

/// Whether the segments `a`–`b` and `c`–`d` cross.
fn crosses(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
    let side = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    };
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// `n`, a count of steps held as a number, as a whole number.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole(n: f64) -> u32 {
    n as u32
}

/// A course the leader runs: places along it, each with how far along it lies.
struct Course {
    points: Vec<(f64, f64)>,
    along: Vec<f64>,
}

impl Course {
    fn new(points: Vec<(f64, f64)>) -> Self {
        let mut along = vec![0.0];
        for w in points.windows(2) {
            let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            along.push(along.last().copied().unwrap_or(0.0) + d);
        }
        Self { points, along }
    }

    fn length(&self) -> f64 {
        self.along.last().copied().unwrap_or(0.0)
    }

    /// The place `s` metres along, and the heading there.
    fn at(&self, s: f64) -> ((f64, f64), f64) {
        let s = s.clamp(0.0, self.length());
        let i = self
            .along
            .windows(2)
            .position(|w| s <= w[1])
            .unwrap_or(self.points.len().saturating_sub(2));
        let (a, b) = (self.points[i], self.points[i + 1]);
        let span = (self.along[i + 1] - self.along[i]).max(1e-9);
        let t = (s - self.along[i]) / span;
        let heading = dereth_primitives::num::math::atan2(b.0 - a.0, b.1 - a.1)
            .to_degrees()
            .rem_euclid(360.0);
        ((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t), heading)
    }

    /// Straight from `from` by each leg: a heading change (degrees, clockwise) then metres.
    fn legs(from: (f64, f64), heading: f64, legs: &[(f64, f64)]) -> Self {
        let mut points = vec![from];
        let (mut x, mut y, mut h) = (from.0, from.1, heading);
        for &(turn, metres) in legs {
            h += turn;
            let steps = (metres / 0.5).ceil().max(1.0);
            let r = h.to_radians();
            for _ in 0..whole(steps) {
                x += dereth_primitives::num::math::sin(r) * metres / steps;
                y += dereth_primitives::num::math::cos(r) * metres / steps;
                points.push((x, y));
            }
        }
        Self::new(points)
    }

    /// An arc of `degrees` (clockwise when positive) of radius `radius` after the course's end,
    /// in half-metre steps.
    fn arc(mut self, radius: f64, degrees: f64) -> Self {
        let n = self.points.len();
        let (a, b) = (self.points[n - 2], self.points[n - 1]);
        let mut h = dereth_primitives::num::math::atan2(b.0 - a.0, b.1 - a.1).to_degrees();
        let steps = (radius * degrees.abs().to_radians() / 0.5).ceil().max(1.0);
        let turn = degrees / steps;
        let step = radius * degrees.abs().to_radians() / steps;
        let (mut x, mut y) = b;
        for _ in 0..whole(steps) {
            h += turn;
            let r = h.to_radians();
            x += dereth_primitives::num::math::sin(r) * step;
            y += dereth_primitives::num::math::cos(r) * step;
            self.points.push((x, y));
        }
        Self::new(self.points)
    }

    /// Straight on from the course's end for `metres`.
    fn straight(self, metres: f64) -> Self {
        let n = self.points.len();
        let (a, b) = (self.points[n - 2], self.points[n - 1]);
        let h = dereth_primitives::num::math::atan2(b.0 - a.0, b.1 - a.1).to_degrees();
        let mut more = Self::legs(b, h, &[(0.0, metres)]).points;
        let mut points = self.points;
        more.remove(0);
        points.extend(more);
        Self::new(points)
    }
}

/// The scripted world: the follower's body and the leader as the follower's client knows it.
struct Scene {
    me: Position,
    autorun: bool,
    held: BTreeSet<NavKey>,
    turning_to: Option<f64>,
    run_speed: f64,
    walk_speed: f64,
    turn_rate: f64,
    /// The leader's place as the follower's client last heard it.
    leader: Position,
    /// Walls, which stop the body and cut the line of sight; and whether lines of sight are
    /// known at all.
    walls: Vec<((f64, f64), (f64, f64))>,
    sight_known: bool,
    /// Times the body ran into a wall.
    blocked: u32,
}

impl Places for Scene {
    fn player(&self) -> Option<Position> {
        Some(self.me)
    }

    fn object(&self, id: ObjectId) -> Option<Position> {
        (id == LEADER).then_some(self.leader)
    }

    fn autorun(&self) -> bool {
        self.autorun
    }

    fn sight(&self, from: &Position, to: &Position) -> Option<bool> {
        let (a, b) = (xy(from), xy(to));
        self.sight_known
            .then(|| !self.walls.iter().any(|&(c, d)| crosses(a, b, c, d)))
    }
}

impl Scene {
    fn new(me: (f64, f64, f64), leader: (f64, f64)) -> Self {
        Self {
            me: place(me.0, me.1, me.2),
            autorun: false,
            held: BTreeSet::new(),
            turning_to: None,
            run_speed: 8.5,
            walk_speed: 3.1,
            turn_rate: 135.0,
            leader: place(leader.0, leader.1, 0.0),
            walls: Vec::new(),
            sight_known: true,
            blocked: 0,
        }
    }

    /// Carry the body on by `dt` seconds.
    fn carry(&mut self, dt: f64) {
        let mut h = places::heading(&self.me);
        if self.held.contains(&NavKey::TurnRight) {
            h += self.turn_rate * dt;
        } else if self.held.contains(&NavKey::TurnLeft) {
            h -= self.turn_rate * dt;
        } else if let Some(to) = self.turning_to {
            let turn = places::turn(h, to);
            let step = self.turn_rate * dt;
            if turn.abs() <= step {
                h = to;
                self.turning_to = None;
            } else {
                h += step.copysign(turn);
            }
        }
        let (x, y) = xy(&self.me);
        let (mut nx, mut ny) = (x, y);
        if self.autorun {
            let speed = if self.held.contains(&NavKey::Walk) {
                self.walk_speed
            } else {
                self.run_speed
            };
            let r = h.to_radians();
            nx += dereth_primitives::num::math::sin(r) * speed * dt;
            ny += dereth_primitives::num::math::cos(r) * speed * dt;
        }
        if self
            .walls
            .iter()
            .any(|&(c, d)| crosses((x, y), (nx, ny), c, d))
        {
            self.blocked += 1;
            (nx, ny) = (x, y);
        }
        self.me = place(nx, ny, h.rem_euclid(360.0));
    }

    /// What the follow asked of the body this frame.
    fn apply(&mut self, nav: &[NavRequest], log: &mut Log, now: f64) {
        for r in nav {
            match *r {
                NavRequest::Face(h) => {
                    log.faces += 1;
                    self.turning_to = Some(f64::from(h));
                }
                NavRequest::Heading(h) => {
                    let (x, y) = xy(&self.me);
                    self.me = place(x, y, f64::from(h));
                }
                NavRequest::Autorun(on) => {
                    if self.autorun && !on {
                        log.stops.push(now);
                    }
                    self.autorun = on;
                }
                NavRequest::Hold(key, down) => {
                    if down {
                        if key == NavKey::Walk {
                            log.walks += 1;
                        }
                        self.held.insert(key);
                    } else {
                        self.held.remove(&key);
                    }
                }
                NavRequest::CancelJump => {}
            }
        }
    }
}

/// What a follow did, frame by frame.
#[derive(Debug, Default)]
struct Log {
    /// When the run lock was let go.
    stops: Vec<f64>,
    /// Turns asked of the client's turn-to-heading.
    faces: u32,
    /// Presses of the walk key.
    walks: u32,
    /// Per frame: the time, the gap to the leader's true place, whether the leader was moving,
    /// and whether the body ran.
    frames: Vec<(f64, f64, bool, bool)>,
}

impl Log {
    /// The stops made while the leader was moving, after `from` seconds.
    fn stops_while_moving(&self, from: f64) -> Vec<f64> {
        self.stops
            .iter()
            .copied()
            .filter(|&t| t >= from && self.frames.iter().find(|f| f.0 >= t).is_some_and(|f| f.2))
            .collect()
    }

    /// The least and the most gap over frames between `from` and `to` seconds.
    fn gap(&self, from: f64, to: f64) -> (f64, f64) {
        self.frames
            .iter()
            .filter(|f| f.0 >= from && f.0 <= to)
            .fold((f64::MAX, f64::MIN), |(lo, hi), f| {
                (lo.min(f.1), hi.max(f.1))
            })
    }
}

/// Follow `course`, run from `start` seconds at `pace` metres a second, within `range`, for
/// `secs` seconds at `fps` frames a second: the leader's place reaches the follower every fifth
/// of a second.
fn follow(
    scene: &mut Scene,
    course: &Course,
    start: f64,
    pace: f64,
    range: f64,
    secs: f64,
    fps: f64,
) -> Log {
    let view = NoView;
    let world = World::new();
    let table = CommandInterp::new();
    let mut nav = Nav::default();
    let mut log = Log::default();
    let dt = 1.0 / fps;
    let mut now = 0.0;
    let mut heard = f64::MIN;
    assert!(nav
        .start(
            PLUGIN,
            1,
            Goal::Follow {
                object: LEADER,
                range,
            },
            now
        )
        .is_none());
    let along = |t: f64| ((t - start).max(0.0) * pace).min(course.length());
    while now < secs {
        now += dt;
        scene.carry(dt);
        let s = along(now);
        let ((lx, ly), lh) = course.at(s);
        let moving = now > start && s < course.length();
        if now - heard >= 0.2 {
            scene.leader = place(lx, ly, lh);
            heard = now;
        }
        let truth = place(lx, ly, lh);
        let mut frame =
            HookFrame::new(&view, &world, &table, now, now, &[], &[]).with_places(&*scene);
        let ended = nav.step(&mut frame);
        assert!(ended.is_none(), "a follow never ends by itself: {ended:?}");
        let out = frame.into_output();
        scene.apply(&out.nav, &mut log, now);
        log.frames.push((
            now,
            places::ground_distance(&scene.me, &truth),
            moving,
            scene.autorun,
        ));
    }
    log
}

/// An open-ground loop: two long straights joined by two half circles, run to the right.
fn open_loop() -> Course {
    Course::legs((0.0, 0.0), 0.0, &[(0.0, 60.0)])
        .arc(15.0, 180.0)
        .straight(60.0)
        .arc(15.0, 180.0)
        .straight(30.0)
}

/// Behaviour: plugins.nav.a-follow-steers-while-running-and-keeps-pace
#[test]
fn a_follower_runs_a_loop_behind_its_leader_without_stopping_or_falling_behind() {
    for fps in [30.0, 10.0] {
        // The leader sets off 8 m ahead at the follower's own pace.
        let mut scene = Scene::new((0.0, -8.0, 0.0), (0.0, 0.0));
        let course = open_loop();
        let secs = course.length() / 8.5;
        let log = follow(&mut scene, &course, 0.5, 8.5, 3.0, secs, fps);
        assert_eq!(
            log.faces, 0,
            "at {fps} fps it never turns standing by the turn-to-heading"
        );
        assert!(
            log.stops_while_moving(1.0).is_empty(),
            "at {fps} fps it never stops while the leader runs: stops at {:?}",
            log.stops
        );
        let (_, worst) = log.gap(2.0, secs);
        assert!(
            worst <= 14.0,
            "at {fps} fps it keeps up through the bends at the same pace: worst gap {worst:.1} m"
        );
    }
}

/// Behaviour: plugins.nav.a-follow-steers-while-running-and-keeps-pace
#[test]
fn a_follower_turns_through_a_bend_under_45_degrees_without_stopping() {
    for (bend, sight) in [(40.0, true), (-40.0, true), (40.0, false), (30.0, false)] {
        let mut scene = Scene::new((0.0, -6.0, 0.0), (0.0, 0.0));
        scene.sight_known = sight;
        let course = Course::legs((0.0, 0.0), 0.0, &[(0.0, 40.0), (bend, 40.0), (-bend, 30.0)]);
        let secs = course.length() / 8.0 + 0.5;
        let log = follow(&mut scene, &course, 0.5, 8.0, 3.0, secs, 30.0);
        assert!(
            log.stops_while_moving(1.0).is_empty(),
            "a {bend}° bend (lines of sight known: {sight}) is turned through running: stops at {:?}",
            log.stops
        );
        assert_eq!(log.faces, 0);
    }
}

/// Behaviour: plugins.nav.a-follow-steers-while-running-and-keeps-pace
#[test]
fn a_faster_follower_holds_its_gap_near_the_range_by_walking_without_stopping() {
    let mut scene = Scene::new((0.0, -20.0, 0.0), (0.0, 0.0));
    scene.run_speed = 9.5;
    let course = Course::legs((0.0, 0.0), 0.0, &[(0.0, 400.0)]);
    let log = follow(&mut scene, &course, 0.5, 8.5, 3.0, 40.0, 30.0);
    assert!(
        log.stops_while_moving(1.0).is_empty(),
        "stops at {:?}",
        log.stops
    );
    let (least, most) = log.gap(25.0, 40.0);
    assert!(
        least >= 2.0 && most <= 5.5,
        "the gap holds about the range once caught up: {least:.1}..{most:.1} m"
    );
    assert!(log.walks > 0, "it eases off to a walk near the range");
}

/// Behaviour: plugins.nav.a-follow-steers-while-running-and-keeps-pace
#[test]
fn a_follower_stands_within_range_when_its_leader_stands() {
    let mut scene = Scene::new((0.0, -10.0, 0.0), (0.0, 0.0));
    let course = Course::legs((0.0, 0.0), 0.0, &[(0.0, 30.0), (60.0, 20.0)]);
    let stop = 0.5 + course.length() / 8.5;
    let log = follow(&mut scene, &course, 0.5, 8.5, 3.0, stop + 6.0, 30.0);
    let at_stop = log
        .frames
        .iter()
        .filter(|f| f.0 > stop + 3.0)
        .map(|f| (f.1, f.3))
        .collect::<Vec<_>>();
    assert!(
        at_stop
            .iter()
            .all(|&(gap, ran)| (1.0..=3.5).contains(&gap) && !ran),
        "it stands within range once the leader stands: {:?}",
        at_stop.first()
    );
    assert!(
        log.stops_while_moving(1.0).is_empty(),
        "stops at {:?}",
        log.stops
    );
    assert!(scene.held.is_empty(), "nothing is held while it stands");
}

/// Behaviour: plugins.nav.a-follow-steers-while-running-and-keeps-pace
#[test]
fn a_follower_goes_round_the_corner_its_leader_turned_behind_a_wall() {
    // A wall east of the leader's way north, ending at y = 30; the leader runs round its end and
    // back south behind it, with the follower far enough behind that the straight line to it
    // soon crosses the wall.
    let mut scene = Scene::new((0.0, -20.0, 0.0), (0.0, 0.0));
    scene.walls.push(((3.0, -100.0), (3.0, 30.0)));
    let course = Course::legs((0.0, 0.0), 0.0, &[(0.0, 34.0), (90.0, 7.0), (90.0, 40.0)]);
    let stop = 0.5 + course.length() / 8.0;
    let log = follow(&mut scene, &course, 0.5, 8.0, 3.0, stop + 8.0, 30.0);
    assert_eq!(scene.blocked, 0, "it never runs into the wall");
    let (_, end) = log.gap(stop + 6.0, stop + 8.0);
    assert!(
        end <= 4.0,
        "it reaches the leader behind the wall: {end:.1} m"
    );
}
