//! Following a route: given where the body is and which way it faces, the keys to hold next.
//!
//! The follower holds no body and moves nothing. Each frame its owner tells it where the body is,
//! which way it faces and when, and it answers whether to hold the forward key and which turn key,
//! or says the route is done, a door is in the way, or the body has stopped getting anywhere.
//!
//! It steers the way a player at the keyboard does, at any frame rate:
//!
//! - **It aims ahead.** The body heads for a point a little further along the route than where
//!   it stands (about as far as it turns round in at a run), so it runs through a bend without
//!   stopping. It cuts across a corner only as far as the planner found the line across
//!   walkable, and never past a door or the route's end.
//! - **A waypoint is passed by the ground covered since the last frame**, not only by where the
//!   body stands now: a frame long enough to carry the body past a waypoint still counts it
//!   reached, and the body does not turn back for it.
//! - **A turn key is let go before it overshoots.** A key holds for a whole frame, and the body
//!   turns at its own rate all that frame, so a turn smaller than half of one frame's turning is
//!   left alone. Its speed and turn rate are learned from how far it moved and turned while the
//!   keys were held.
//! - **The forward key is held only while the point aimed at can be run onto**, as near as the
//!   body's turning circle allows; otherwise it turns standing.
//! - **A leg it cannot walk straight is walked through its places**: when the body stops getting
//!   closer on a leg the planner straightened, it follows the places the search went through
//!   there instead, and only when that fails too says it is stuck.

use dereth_primitives::num::math::{atan2f, sinf};
use dereth_primitives::{ObjectId, Position, Vec3};

use crate::path::{global, NavPath, WaypointAction};
use crate::vmath::{dist_xy, point_segment_dist_sq};

/// What the body should do now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Steer {
    /// Hold the forward key or not, and a turn key or neither.
    Go {
        /// The heading of the point aimed at (degrees clockwise from north, the client's
        /// heading).
        heading: f32,
        /// How far across the current waypoint is.
        distance: f32,
        /// The waypoint being walked to.
        waypoint: usize,
        /// Hold the forward key.
        forward: bool,
        /// Hold the turn-right key (`Some(true)`), the turn-left key (`Some(false)`), or neither.
        turn: Option<bool>,
    },
    /// Open this door, then call again.
    OpenDoor(ObjectId),
    /// The route is done.
    Arrived,
    /// The body has not got closer to its waypoint for a while, through the leg's places too; the
    /// owner should plan again from where it is.
    Stuck {
        /// The waypoint it was walking to.
        waypoint: usize,
    },
}

/// The follower's settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FollowConfig {
    /// How close, across, counts as reaching a waypoint.
    pub arrive: f32,
    /// How close the last waypoint has to be.
    pub arrive_last: f32,
    /// Seconds without getting closer that count as stuck.
    pub stuck_after: f64,
    /// How far off, in degrees, the body may face the point it aims at before a turn key is held.
    pub turn_slack: f32,
    /// A corner turning more than this, in degrees, is stopped at and turned at standing; a
    /// gentler one is run through.
    pub sharp_turn: f32,
    /// The body's speed with the forward key held, metres a second, until it has been seen.
    pub speed: f32,
    /// The body's turn rate with a turn key held, degrees a second, until it has been seen.
    pub turn_rate: f32,
}

impl Default for FollowConfig {
    fn default() -> Self {
        Self {
            arrive: 0.6,
            arrive_last: 0.35,
            stuck_after: 2.0,
            turn_slack: 6.0,
            sharp_turn: 45.0,
            speed: 8.0,
            turn_rate: 135.0,
        }
    }
}

/// How far past a waypoint, across, the body may be and still count it passed by crossing the
/// line that halves the corner there.
const PASS_REACH: f32 = 2.5;

/// How far above or below a waypoint the body may be and still count it reached: a body's height.
const REACH_RISE: f32 = 2.0;

/// How far from the leg it is on, across, or above or below it, the body is off the route and
/// the owner should plan again from where it is.
const LOST_ACROSS: f32 = 4.0;
const LOST_RISE: f32 = 2.5;

/// The nearest point aimed at on a leg walked through its places, metres.
const NEAR_AIM: f32 = 0.75;

/// The farthest point aimed at, metres.
const FAR_AIM: f32 = 6.0;

/// What the body did in the last frame.
#[derive(Debug, Clone, Copy)]
struct Seen {
    here: Vec3,
    facing: f32,
    now: f64,
    forward: bool,
    turn: Option<bool>,
}

/// A route being followed.
#[derive(Debug, Clone)]
pub struct Follower {
    path: NavPath,
    next: usize,
    config: FollowConfig,
    best: f32,
    best_at: f64,
    door_done: Option<usize>,
    last: Option<Seen>,
    /// Per waypoint, whether it is one of the places a leg is walked through.
    fine: Vec<bool>,
    /// The body's speed and turn rate as seen, and the length of the last frame.
    speed: f32,
    turn_rate: f32,
    frame: f64,
    /// Ground covered and seconds spent running, and degrees turned and seconds spent turning,
    /// since the speed and the turn rate were last learned from them.
    ran: (f32, f64),
    turned: (f32, f64),
    /// Times the follower has reported [`Steer::Stuck`].
    pub stuck_events: u32,
    /// Legs walked through their places after the body stopped on them.
    pub refined_legs: u32,
}

/// `a - b` in degrees, in `(-180, 180]`.
fn turn_between(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d <= -180.0 {
        d += 360.0;
    }
    d
}

/// The heading from `a` to `b`, degrees clockwise from north (+y).
fn heading_to(a: Vec3, b: Vec3) -> f32 {
    let h = atan2f(b.x - a.x, b.y - a.y).to_degrees();
    if h < 0.0 {
        h + 360.0
    } else {
        h
    }
}

/// `a` flattened onto the floor of `b`.
const fn flat(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x, a.y, b.z)
}

impl Follower {
    /// Follow `path` from its start.
    #[must_use]
    pub fn new(path: NavPath, config: FollowConfig) -> Self {
        Self {
            fine: vec![false; path.waypoints.len()],
            path,
            next: 1,
            config,
            best: f32::MAX,
            best_at: 0.0,
            door_done: None,
            last: None,
            speed: config.speed,
            turn_rate: config.turn_rate,
            frame: 1.0 / 30.0,
            ran: (0.0, 0.0),
            turned: (0.0, 0.0),
            stuck_events: 0,
            refined_legs: 0,
        }
    }

    /// The route.
    #[must_use]
    pub const fn path(&self) -> &NavPath {
        &self.path
    }

    /// The waypoint being walked to.
    #[must_use]
    pub const fn next_waypoint(&self) -> usize {
        self.next
    }

    /// Say a door was dealt with; the follower walks on through it.
    pub fn door_opened(&mut self) {
        self.door_done = Some(self.next);
    }

    /// The body's speed and turn rate as seen so far: metres a second, degrees a second.
    #[must_use]
    pub const fn pace(&self) -> (f32, f32) {
        (self.speed, self.turn_rate)
    }

    /// Learn the frame's length, and the body's speed and turn rate from what it did while the
    /// keys were held: over a fifth of a second at least, as the world moves a body in steps of
    /// its own that a short frame may fall between.
    fn learn(&mut self, here: Vec3, facing: f32, now: f64) {
        const WINDOW: f64 = 0.2;
        let Some(l) = self.last else { return };
        let dt = now - l.now;
        if dt <= 1e-4 {
            return;
        }
        self.frame = dt.clamp(1.0 / 240.0, 0.5);
        if l.forward {
            self.ran.0 += dist_xy(here, l.here);
            self.ran.1 += dt;
            if self.ran.1 >= WINDOW {
                #[allow(clippy::cast_possible_truncation)]
                let v = self.ran.0 / self.ran.1 as f32;
                if (0.5..30.0).contains(&v) {
                    self.speed = 0.5 * self.speed + 0.5 * v;
                }
                self.ran = (0.0, 0.0);
            }
        } else {
            self.ran = (0.0, 0.0);
        }
        if l.turn.is_some() {
            self.turned.0 += turn_between(facing, l.facing).abs();
            self.turned.1 += dt;
            if self.turned.1 >= WINDOW {
                #[allow(clippy::cast_possible_truncation)]
                let w = self.turned.0 / self.turned.1 as f32;
                if (10.0..720.0).contains(&w) {
                    self.turn_rate = 0.5 * self.turn_rate + 0.5 * w;
                }
                self.turned = (0.0, 0.0);
            }
        } else {
            self.turned = (0.0, 0.0);
        }
    }

    /// How far the body runs in one frame.
    #[allow(clippy::cast_possible_truncation)]
    fn stride(&self) -> f32 {
        self.speed * self.frame as f32
    }

    /// The radius of the body's turning circle at a run.
    fn turning_radius(&self) -> f32 {
        self.speed / self.turn_rate.max(1.0).to_radians()
    }

    /// Whether the route turns more than the sharp-turn angle at waypoint `i`.
    fn sharp(&self, i: usize) -> bool {
        let w = &self.path.waypoints;
        if i == 0 || i + 1 >= w.len() {
            return false;
        }
        let (a, b, c) = (
            global(&w[i - 1].position),
            global(&w[i].position),
            global(&w[i + 1].position),
        );
        if dist_xy(a, b) < 1e-3 || dist_xy(b, c) < 1e-3 {
            return false;
        }
        turn_between(heading_to(b, c), heading_to(a, b)).abs() > self.config.sharp_turn
    }

    /// Whether waypoint `i` is one to stop at: the last, or one with something to do there.
    fn stops_at(&self, i: usize) -> bool {
        i + 1 >= self.path.waypoints.len() || self.path.waypoints[i].action.is_some()
    }

    /// Whether waypoint `i` is one of the places a leg is walked through.
    fn fine(&self, i: usize) -> bool {
        self.fine.get(i).copied().unwrap_or(false)
    }

    /// Whether a body at `here` has crossed the line through waypoint `i` that halves the corner
    /// there (square to the route where it runs straight on), near it.
    fn crossed(&self, i: usize, here: Vec3) -> bool {
        let w = &self.path.waypoints;
        let (a, b) = (global(&w[i - 1].position), global(&w[i].position));
        let c = w.get(i + 1).map_or(b, |n| global(&n.position));
        let unit = |x: f32, y: f32| {
            let l = (x * x + y * y).sqrt();
            if l > 1e-4 {
                (x / l, y / l)
            } else {
                (0.0, 0.0)
            }
        };
        let (ix, iy) = unit(b.x - a.x, b.y - a.y);
        let (ox, oy) = unit(c.x - b.x, c.y - b.y);
        let (nx, ny) = (ix + ox, iy + oy);
        if nx.abs() + ny.abs() < 1e-3 {
            return false;
        }
        (here.x - b.x) * nx + (here.y - b.y) * ny >= 0.0
            && dist_xy(here, b) <= PASS_REACH.max(self.path.cut(i) + 1.0)
            && (here.z - b.z).abs() <= REACH_RISE
    }

    /// How far a body at `here` is from the leg into waypoint `next`, across and up or down.
    fn off_leg(&self, here: Vec3) -> (f32, f32) {
        let w = &self.path.waypoints;
        let a = global(&w[self.next - 1].position);
        let b = global(&w[self.next].position);
        let across = point_segment_dist_sq(flat(here, a), a, flat(b, a)).sqrt();
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l2 = dx * dx + dy * dy;
        let t = if l2 > 0.0 {
            (((here.x - a.x) * dx + (here.y - a.y) * dy) / l2).clamp(0.0, 1.0)
        } else {
            1.0
        };
        (across, here.z - (a.z + (b.z - a.z) * t))
    }

    /// The point a body at `here` aims at: `reach` along the route from where it stands across
    /// the leg it is on, past a corner only once within the corner's cut of it and then no
    /// further past it than that, and never past a waypoint it stops at.
    fn aim(&self, here: Vec3, reach: f32) -> Vec3 {
        let w = &self.path.waypoints;
        let last = w.len() - 1;
        let a = global(&w[self.next - 1].position);
        let b = global(&w[self.next].position);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l2 = dx * dx + dy * dy;
        let t = if l2 > 0.0 {
            (((here.x - a.x) * dx + (here.y - a.y) * dy) / l2).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let mut from = Vec3::new(a.x + dx * t, a.y + dy * t, a.z + (b.z - a.z) * t);
        let mut left = reach;
        let mut i = self.next;
        loop {
            let to = global(&w[i].position);
            let seg = dist_xy(from, to);
            if seg >= left && seg > 0.0 {
                let f = left / seg;
                return Vec3::new(
                    from.x + (to.x - from.x) * f,
                    from.y + (to.y - from.y) * f,
                    from.z + (to.z - from.z) * f,
                );
            }
            let cut = self.path.cut(i);
            if i == last || self.stops_at(i) || seg > cut {
                return to;
            }
            left = (left - seg).min(cut);
            from = to;
            i += 1;
        }
    }

    /// The step for a body at `at`, facing `facing` (degrees clockwise from north), at time `now`
    /// (seconds, any steady clock).
    pub fn update(&mut self, at: &Position, facing: f32, now: f64) -> Steer {
        let here = global(at);
        self.learn(here, facing, now);
        let prev = self.last.map_or(here, |l| l.here);
        if self.last.is_some_and(|l| !l.forward) {
            // Turning standing, or waiting at a door: no lack of progress.
            self.best_at = now;
        }
        let stride = self.stride();
        let last = self.path.waypoints.len().saturating_sub(1);
        loop {
            if self.next > last {
                self.last = None;
                return Steer::Arrived;
            }
            let w = &self.path.waypoints[self.next];
            let p = global(&w.position);
            let stop = self.stops_at(self.next);
            let sharp = self.sharp(self.next) && self.path.cut(self.next) <= 0.0;
            let mut close = if self.next == last {
                self.config.arrive_last
            } else {
                self.config.arrive
            };
            if stop || sharp {
                // A frame's run cannot be cut short: within half of one is as near as it gets.
                close = close.max(0.5 * stride);
            }
            let level = (here.z - p.z).abs() <= REACH_RISE;
            let reached = (level
                && (dist_xy(here, p) <= close
                    || point_segment_dist_sq(flat(p, here), flat(prev, here), here)
                        <= close * close))
                || (!stop && !sharp && self.crossed(self.next, here));
            if !reached {
                break;
            }
            // Reached: a door here is opened before walking on.
            if let Some(WaypointAction::OpenDoor(id)) = w.action {
                if self.door_done != Some(self.next) {
                    self.last = Some(Seen {
                        here,
                        facing,
                        now,
                        forward: false,
                        turn: None,
                    });
                    return Steer::OpenDoor(id);
                }
            }
            self.next += 1;
            self.best = f32::MAX;
            self.best_at = now;
        }
        let (across, rise) = self.off_leg(here);
        if across > LOST_ACROSS || rise.abs() > LOST_RISE {
            // Off the route, on another floor or far to one side: no steering brings it back.
            self.stuck_events += 1;
            self.last = None;
            return Steer::Stuck {
                waypoint: self.next,
            };
        }
        let target = global(&self.path.waypoints[self.next].position);
        let d = dist_xy(here, target);
        if d + 0.05 < self.best {
            self.best = d;
            self.best_at = now;
        } else if now - self.best_at > self.config.stuck_after {
            self.best_at = now;
            let leg = self.next - 1;
            if self.path.legs.get(leg).is_some_and(|l| l.len() >= 2) {
                // Walk the leg through the places the search went through, from the one nearest
                // the body.
                let places = self.path.legs[leg].len();
                self.path.densify_leg(leg);
                self.fine
                    .splice(leg + 1..=leg + 1, std::iter::repeat_n(true, places));
                self.refined_legs += 1;
                self.next = (leg + 1..=leg + places)
                    .min_by(|&i, &j| {
                        let di = dist_xy(here, global(&self.path.waypoints[i].position));
                        let dj = dist_xy(here, global(&self.path.waypoints[j].position));
                        di.total_cmp(&dj)
                    })
                    .unwrap_or(leg + 1);
                self.best = f32::MAX;
            } else {
                self.stuck_events += 1;
                self.last = None;
                return Steer::Stuck {
                    waypoint: self.next,
                };
            }
        }
        let reach = if self.fine(self.next) {
            NEAR_AIM.max(1.2 * stride)
        } else {
            self.turning_radius().max(1.5 * stride).min(FAR_AIM)
        };
        let aim = self.aim(here, reach);
        let heading = heading_to(here, aim);
        let off = turn_between(heading, facing);
        // A turn key held a whole frame turns the body this far.
        #[allow(clippy::cast_possible_truncation)]
        let per_frame = self.turn_rate * self.frame as f32;
        let slack = self.config.turn_slack.max(0.5 * per_frame);
        let turn = if off > slack {
            Some(true)
        } else if off < -slack {
            Some(false)
        } else {
            None
        };
        // Run while the point aimed at lies on or outside the turning circle that sets off along
        // the body's facing: a circle through it, tangent to the facing, has radius
        // `distance / (2 sin off)`. Running, the body keeps on until it is well inside; standing,
        // it sets off once it is well outside, so it does not stop and start frame by frame.
        let across = dist_xy(here, aim);
        let s = sinf(off.abs().to_radians());
        let running = self.last.is_some_and(|l| l.forward);
        let room = if running { 0.6 } else { 0.9 };
        let forward = off.abs() <= slack
            || (off.abs() < 90.0 && across >= 2.0 * room * self.turning_radius() * s);
        self.last = Some(Seen {
            here,
            facing,
            now,
            forward,
            turn,
        });
        Steer::Go {
            heading,
            distance: d,
            waypoint: self.next,
            forward,
            turn,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks the follower's steering arithmetic).
    use super::*;
    use crate::path::Waypoint;
    use dereth_primitives::{CellId, Frame, Quat};

    fn at(x: f32, y: f32) -> Position {
        Position::new(
            CellId(0x0101_0100),
            Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
        )
    }

    fn route_through(points: &[(f32, f32)]) -> NavPath {
        let waypoints: Vec<Waypoint> = points
            .iter()
            .map(|&(x, y)| Waypoint {
                position: at(x, y),
                action: None,
            })
            .collect();
        // Each leg straightened from places the search went through: its middle and its end.
        let legs = waypoints
            .windows(2)
            .map(|w| {
                let (a, b) = (w[0].position.frame.origin, w[1].position.frame.origin);
                vec![at((a.x + b.x) / 2.0, (a.y + b.y) / 2.0), w[1].position]
            })
            .collect();
        // Open ground: every corner may be cut as far as the planner tries.
        let cuts = vec![3.0; waypoints.len()];
        NavPath {
            waypoints,
            legs,
            goal_offset: 0.0,
            cuts,
        }
    }

    fn route() -> NavPath {
        route_through(&[(0.0, 0.0), (0.0, 10.0), (10.0, 10.0)])
    }

    /// A body driven by the follower's keys, with no walls: the forward key runs it at `speed`,
    /// a turn key turns it at `turn_rate`, each held for the whole of the next frame.
    struct Body {
        x: f32,
        y: f32,
        heading: f32,
        now: f64,
        speed: f32,
        turn_rate: f32,
    }

    impl Body {
        fn frame(&mut self, dt: f64, forward: bool, turn: Option<bool>) {
            #[allow(clippy::cast_possible_truncation)]
            let t = dt as f32;
            if let Some(right) = turn {
                let d = self.turn_rate * t;
                self.heading = (self.heading + if right { d } else { -d }).rem_euclid(360.0);
            }
            if forward {
                let h = self.heading.to_radians();
                self.x += sinf(h) * self.speed * t;
                self.y += dereth_primitives::num::math::cosf(h) * self.speed * t;
            }
            self.now += dt;
        }
    }

    /// What driving a route came to.
    struct Drive {
        arrived: bool,
        seconds: f64,
        /// The farthest the body strayed from the route.
        strayed: f32,
        /// Times it stopped running before the end.
        stops: u32,
        end: (f32, f32),
    }

    fn drive(path: &NavPath, fps: f64, speed: f32, turn_rate: f32) -> Drive {
        let pts: Vec<Vec3> = path
            .waypoints
            .iter()
            .map(|w| w.position.frame.origin)
            .collect();
        let mut body = Body {
            x: pts[0].x,
            y: pts[0].y,
            heading: 0.0,
            now: 0.0,
            speed,
            turn_rate,
        };
        let mut f = Follower::new(path.clone(), FollowConfig::default());
        let (mut strayed, mut stops, mut forward) = (0.0_f32, 0, false);
        let dt = 1.0 / fps;
        let mut arrived = false;
        while body.now < 120.0 {
            let here = Vec3::new(body.x, body.y, 0.0);
            let off = pts
                .windows(2)
                .map(|s| point_segment_dist_sq(here, s[0], s[1]).sqrt())
                .fold(f32::MAX, f32::min);
            strayed = strayed.max(off);
            match f.update(&at(body.x, body.y), body.heading, body.now) {
                Steer::Arrived => {
                    arrived = true;
                    break;
                }
                Steer::Go {
                    forward: go, turn, ..
                } => {
                    if forward && !go {
                        stops += 1;
                    }
                    forward = go;
                    body.frame(dt, go, turn);
                }
                s => panic!("{s:?} at {fps} frames a second"),
            }
        }
        Drive {
            arrived,
            seconds: body.now,
            strayed,
            stops,
            end: (body.x, body.y),
        }
    }

    #[test]
    fn the_follower_heads_north_then_east_and_arrives() {
        let mut f = Follower::new(route(), FollowConfig::default());
        match f.update(&at(0.0, 0.0), 0.0, 0.0) {
            Steer::Go {
                heading, forward, ..
            } => {
                assert!(heading.abs() < 1e-3, "{heading}");
                assert!(forward, "facing the way already");
            }
            s => panic!("{s:?}"),
        }
        match f.update(&at(0.0, 9.8), 0.0, 1.0) {
            Steer::Go { waypoint, .. } => assert_eq!(waypoint, 2),
            s => panic!("{s:?}"),
        }
        assert_eq!(f.update(&at(9.9, 10.0), 90.0, 2.0), Steer::Arrived);
    }

    #[test]
    fn a_body_that_stops_getting_closer_is_reported_stuck_once_per_wait() {
        // Legs of one place each: nothing finer to walk through.
        let mut path = route();
        for (leg, w) in path.legs.iter_mut().zip(&path.waypoints[1..]) {
            *leg = vec![w.position];
        }
        let mut f = Follower::new(path, FollowConfig::default());
        assert!(matches!(
            f.update(&at(0.0, 0.0), 0.0, 0.0),
            Steer::Go { forward: true, .. }
        ));
        assert!(matches!(
            f.update(&at(0.0, 0.0), 0.0, 1.0),
            Steer::Go { .. }
        ));
        assert_eq!(
            f.update(&at(0.0, 0.0), 0.0, 2.5),
            Steer::Stuck { waypoint: 1 }
        );
        assert!(matches!(
            f.update(&at(0.0, 0.0), 0.0, 3.0),
            Steer::Go { .. }
        ));
        assert_eq!(f.stuck_events, 1);
    }

    #[test]
    fn a_body_stopped_on_a_straightened_leg_walks_it_through_its_places_before_it_is_stuck() {
        let mut path = route();
        // The first leg was straightened from the places the search went through, a sidestep
        // to the east on the way.
        path.legs[0] = vec![at(0.5, 3.0), at(0.5, 6.0), at(0.0, 10.0)];
        let mut f = Follower::new(path, FollowConfig::default());
        assert!(matches!(
            f.update(&at(0.0, 0.0), 0.0, 0.0),
            Steer::Go { .. }
        ));
        // Stopped two metres along.
        assert!(matches!(
            f.update(&at(0.0, 2.0), 0.0, 0.5),
            Steer::Go { .. }
        ));
        match f.update(&at(0.0, 2.0), 0.0, 3.0) {
            Steer::Go { waypoint, .. } => assert_eq!(
                f.path().waypoints[waypoint].position.frame.origin,
                Vec3::new(0.5, 3.0, 0.0),
                "on to the nearest of the leg's places"
            ),
            s => panic!("{s:?}"),
        }
        assert_eq!(f.refined_legs, 1);
        assert_eq!(f.stuck_events, 0);
    }

    #[test]
    fn a_waypoint_passed_between_two_frames_counts_as_reached() {
        let mut f = Follower::new(
            route_through(&[(0.0, 0.0), (0.0, 10.0), (0.0, 20.0), (10.0, 20.0)]),
            FollowConfig::default(),
        );
        assert!(matches!(
            f.update(&at(0.0, 9.0), 0.0, 0.0),
            Steer::Go { waypoint: 1, .. }
        ));
        // One long frame carries the body from a metre short of the waypoint to a metre past
        // it: it walks on to the next, not back.
        match f.update(&at(0.0, 11.0), 0.0, 0.25) {
            Steer::Go {
                waypoint, heading, ..
            } => {
                assert_eq!(waypoint, 2);
                assert!(heading.abs() < 1.0, "still north: {heading}");
            }
            s => panic!("{s:?}"),
        }
    }

    #[test]
    fn the_end_passed_between_two_frames_counts_as_arrived() {
        let mut f = Follower::new(
            route_through(&[(0.0, 0.0), (0.0, 10.0)]),
            FollowConfig::default(),
        );
        assert!(matches!(
            f.update(&at(0.0, 8.75), 0.0, 0.0),
            Steer::Go { forward: true, .. }
        ));
        // A long frame carries the body from short of the end to well past it.
        assert_eq!(f.update(&at(0.0, 12.5), 0.0, 0.25), Steer::Arrived);
    }

    #[test]
    fn a_turn_smaller_than_half_a_frames_turning_is_left_alone() {
        let f = Follower::new(
            route_through(&[(0.0, 0.0), (0.0, 50.0)]),
            FollowConfig::default(),
        );
        // At five frames a second a turn key held one frame turns the body 27 degrees: a body
        // 10 degrees off holds no key, while at 60 frames a second it turns.
        let turn_at = |dt: f64| {
            let mut f = f.clone();
            let _ = f.update(&at(0.0, 0.0), 10.0, 0.0);
            match f.update(&at(0.0, 0.0), 10.0, dt) {
                Steer::Go { turn, .. } => turn,
                s => panic!("{s:?}"),
            }
        };
        assert_eq!(turn_at(0.2), None);
        assert_eq!(turn_at(1.0 / 60.0), Some(false));
    }

    #[test]
    fn a_route_of_gentle_bends_and_sharp_corners_is_followed_to_its_end_at_any_frame_rate() {
        // Bends of 30 degrees run through, and corners of 90 and 135 degrees turned at.
        let path = route_through(&[
            (0.0, 0.0),
            (0.0, 12.0),
            (6.0, 22.4),
            (6.0, 34.0),
            (20.0, 34.0),
            (10.0, 44.0),
            (10.0, 52.0),
        ]);
        let mut path = path;
        // Walls close in at the two sharp corners: they may be cut across only a little.
        path.cuts[3] = 0.75;
        path.cuts[4] = 0.75;
        let length = path.length();
        for fps in [60.0, 20.0, 10.0, 5.0] {
            let d = drive(&path, fps, 8.0, 135.0);
            assert!(d.arrived, "{fps} frames a second: did not arrive");
            let ideal = f64::from(length / 8.0);
            assert!(
                d.seconds < ideal * 1.25 + 3.0,
                "{fps} frames a second: {:.1} s against {ideal:.1} s",
                d.seconds
            );
            assert!(
                d.strayed < 1.0,
                "{fps} frames a second: strayed {:.2} m from the route",
                d.strayed
            );
            assert!(
                d.stops <= 4,
                "{fps} frames a second: stopped {} times for two sharp corners",
                d.stops
            );
            let (ex, ey) = d.end;
            let gap = ((ex - 10.0).powi(2) + (ey - 52.0).powi(2)).sqrt();
            assert!(
                gap <= 0.85,
                "{fps} frames a second: ended {gap:.2} m from the end"
            );
        }
    }
}
