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
//! - **A turn key is held through a turn, not tapped along it.** Standing, a key is pressed for
//!   a turn past the turn slack; running, only for one past a wider band, twenty degrees as the
//!   game's own move-to has it. Either is then held until what is left is less than the body
//!   turns in one step, so it never overshoots to be turned back.
//! - **A bend inside that band is steered through.** Running, the heading is eased round a
//!   little each frame, gathering its pace and losing it, and never faster than the body turns
//!   with a key held. At a frame rate too low to ease over, what is left of a turn is faced at
//!   once. The body's speed and turn rate are learned from how far it moved and turned while the
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
        /// Face this heading at once (degrees clockwise from north), with neither turn key held:
        /// this frame's part of a bend steered through, or the last of a turn.
        face: Option<f32>,
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
    /// How far off, in degrees, a standing body may face the point it aims at before a turn key
    /// is held.
    pub turn_slack: f32,
    /// A corner turning more than this, in degrees, is stopped at and turned at standing; a
    /// gentler one is run through.
    pub sharp_turn: f32,
    /// The body's speed with the forward key held, metres a second, until it has been seen.
    pub speed: f32,
    /// The body's turn rate with a turn key held, degrees a second, until it has been seen.
    pub turn_rate: f32,
    /// The route is someone's own points, to be walked strictly ([`FollowConfig::strict()`]): how
    /// near each the body must come.
    pub strict: Option<f32>,
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
            strict: None,
        }
    }
}

impl FollowConfig {
    /// For a route of someone's own points, with nothing known of the ground between them. No
    /// corner of it is cut: the body runs along each leg at its point, and counts it reached as
    /// near as it gets, within `range` metres across. Heights are not looked at, and there is no
    /// being lost: a body that stops getting closer for `stuck_after` seconds is stuck.
    #[must_use]
    pub fn strict(range: f32, stuck_after: f64) -> Self {
        Self {
            strict: Some(range.max(MIN_RANGE)),
            stuck_after,
            ..Self::default()
        }
    }
}

/// How far past a waypoint, across, the body may be and still count it passed by crossing the
/// line that halves the corner there.
const PASS_REACH: f32 = 2.5;

/// How far above or below a waypoint the body may be and still count it reached: a body's height.
const REACH_RISE: f32 = 2.0;

/// How near the door's own place a body at a closed door counts the waypoint before it reached,
/// metres. That waypoint may lie in the shut door itself, nearer than a body can stand: the door
/// stops a body some 0.7 m short of it, and the door is opened from there.
const DOOR_REACH: f32 = 1.5;

/// How far from the leg it is on, across, or above or below it, the body is off the route and
/// the owner should plan again from where it is.
const LOST_ACROSS: f32 = 4.0;
const LOST_RISE: f32 = 2.5;

/// How far off, in degrees, a body may face the point it aims at before its heading is set
/// toward it.
const FACE_SLACK: f32 = 1.0;

/// How near facing the point an easing under way carries the body, degrees.
const EASED: f32 = 0.1;

/// How far off, in degrees, a running body may face the point it aims at before a turn key is
/// pressed: the dead band of the game's own move-to.
const RUN_BAND: f32 = 20.0;

/// The seconds over which a running body eases out most of a turn inside that band.
const EASE_SECS: f32 = 0.2;

/// The seconds over which that easing gathers and loses its pace: a quarter of the time it
/// eases over, so it settles without swinging past.
const EASE_GATHER_SECS: f32 = 0.05;

/// The longest the world goes between two steps that turn a body by a held key, at a frame rate
/// above its own: a thirtieth of a second.
const WORLD_STEP: f64 = 1.0 / 30.0;

/// The least range a strictly walked route's point is reached within, metres.
const MIN_RANGE: f32 = 0.25;

/// How much further off than a sharp corner, in degrees, a body already running on a strict
/// route may come to face before it stops to turn: what it lags by coming out of a bend.
const BEND_LAG: f32 = 15.0;

/// How long a body pressing on within range of a strict route's point may get no nearer before
/// the point counts reached: something stands on it.
const PRESSED_SECS: f64 = 0.5;

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
    /// The pace the heading is being eased at, degrees a second, clockwise positive.
    swing: f32,
    /// Whether the body was running when this route was taken up from the one before.
    under_way: bool,
    /// On a strict route: whether it is turning standing at a sharp corner.
    corner: bool,
    /// Since when the forward key has been held.
    running_since: f64,
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
            swing: 0.0,
            under_way: false,
            corner: false,
            running_since: 0.0,
            stuck_events: 0,
            refined_legs: 0,
        }
    }

    /// Follow `path` next, on from this route with the body under way: what was learned of its
    /// pace carries over, and so does a run, which a bend between the two is then run through.
    #[must_use]
    pub fn then(&self, path: NavPath, config: FollowConfig) -> Self {
        Self {
            speed: self.speed,
            turn_rate: self.turn_rate,
            frame: self.frame,
            swing: self.swing,
            under_way: self.last.map_or(self.under_way, |l| l.forward),
            ..Self::new(path, config)
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

    /// Say the body was held up (standing at a door, say) until `now`: it is not stuck for that,
    /// and sets off from standing.
    pub fn held_up(&mut self, now: f64) {
        self.best_at = now;
        self.running_since = now;
        self.swing = 0.0;
        if let Some(l) = self.last.as_mut() {
            l.forward = false;
            l.turn = None;
        }
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

    /// Whether the body's forward key is held, as far as the follower knows.
    fn running(&self) -> bool {
        self.last.map_or(self.under_way, |l| l.forward)
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

    /// Whether a body at `here`, last at `prev`, has reached waypoint `i` of a strict route of
    /// `range`: it came onto it; or it passed within range as near as it was going to; or it has
    /// pressed on within range a while and got no nearer.
    fn honored(&self, i: usize, here: Vec3, prev: Vec3, now: f64, range: f32) -> bool {
        let p = flat(global(&self.path.waypoints[i].position), here);
        let mut close = self.config.arrive.min(range);
        if self.stops_at(i) || self.sharp(i) {
            close = close.max((0.5 * self.stride()).min(range));
        }
        let d = dist_xy(here, p);
        let nearest = point_segment_dist_sq(p, flat(prev, here), here).sqrt();
        if nearest <= close {
            return true;
        }
        // Turning at a sharp corner it has passed nothing: the point after may lie behind it,
        // near as it is, and is still to be run to.
        if !self.running() || self.corner {
            return false;
        }
        let passed = d > dist_xy(prev, p) + 1e-3 && nearest <= PASS_REACH.min(range);
        let pressed = d <= range
            && self.best < f32::MAX
            && now - self.best_at.max(self.running_since) >= PRESSED_SECS;
        passed || pressed
    }

    /// Whether a body on a strict route of `range` runs on, `d` from its point and `to_point`
    /// degrees off facing it, `off` degrees off the point it aims at: it does through a bend it
    /// can turn in and still pass the point within range, and it stops to turn at a sharp corner.
    fn runs_on(&mut self, range: f32, d: f32, to_point: f32, off: f32, slack: f32) -> bool {
        let (off, to_point) = (off.abs(), to_point.abs());
        if self.corner && off > slack {
            return false;
        }
        self.corner = false;
        if off <= slack {
            return true;
        }
        let running = self.running();
        let reach = PASS_REACH.min(range);
        if running && d <= reach && to_point < 90.0 {
            // As good as there: it passes the point within range.
            return true;
        }
        let (limit, room) = if running {
            (self.config.sharp_turn + BEND_LAG, 1.0)
        } else {
            (self.config.sharp_turn, 0.7)
        };
        // Turning as hard as it can, the body runs round a circle, and misses a point inside it
        // by the point's distance from the circle.
        let r = self.turning_radius();
        let to_center =
            (d * d + r * r - 2.0 * d * r * sinf(to_point.min(90.0).to_radians())).max(0.0);
        let miss = r - to_center.sqrt();
        off <= limit && miss <= room * reach.max(self.config.arrive.min(range))
    }

    /// The heading to set this frame to ease a running body round toward one `off` degrees away,
    /// the turn needing `least` degrees a second at the least; `None` where it is left be.
    #[allow(clippy::cast_possible_truncation)]
    fn ease(&mut self, facing: f32, off: f32, least: f32) -> Option<f32> {
        let settled = if self.swing == 0.0 { FACE_SLACK } else { EASED };
        if off.abs() <= settled {
            self.swing = 0.0;
            return None;
        }
        if self.swing * off < 0.0 {
            self.swing = 0.0;
        }
        let dt = self.frame as f32;
        let wanted = (off.abs() / EASE_SECS)
            .max(least)
            .min(self.turn_rate)
            .copysign(off);
        self.swing += (wanted - self.swing) * (dt / EASE_GATHER_SECS).min(1.0);
        let step = self.swing * dt;
        let by = if step.abs() + 1e-3 >= off.abs() {
            off
        } else {
            step
        };
        Some((facing + by).rem_euclid(360.0))
    }

    /// How far a body at `here` is from the leg into waypoint `next`, across and up or down.
    fn off_leg(&self, here: Vec3) -> (f32, f32) {
        let w = &self.path.waypoints;
        let a = global(&w[self.next - 1].position);
        let b = global(&w[self.next].position);
        let across = point_segment_dist_sq(flat(here, a), a, flat(b, a)).sqrt();
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l2 = dx * dx + dy * dy;
        // The floor's height there: the leg's own places' where the search went through them (a
        // leg that runs level and then down a ramp is not a straight line in height), else the
        // straight line's.
        let places = self.path.legs.get(self.next - 1).filter(|l| !l.is_empty());
        if let Some(places) = places {
            let floor = places
                .iter()
                .map(global)
                .chain(std::iter::once(a))
                .min_by(|p, q| dist_xy(here, *p).total_cmp(&dist_xy(here, *q)))
                .map_or(a.z, |p| p.z);
            return (across, here.z - floor);
        }
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

    /// Whether a body at `here` stands at the closed door waypoint `i` is before: within
    /// [`DOOR_REACH`] of the door's place, the waypoint after it, on its level.
    fn at_shut_door(&self, i: usize, here: Vec3) -> bool {
        let w = &self.path.waypoints;
        matches!(w[i].action, Some(WaypointAction::OpenDoor(_)))
            && self.door_done != Some(i)
            && w.get(i + 1).is_some_and(|d| {
                let d = global(&d.position);
                (here.z - d.z).abs() <= REACH_RISE && dist_xy(here, d) <= DOOR_REACH
            })
    }

    /// The step for a body at `at`, facing `facing` (degrees clockwise from north), at time `now`
    /// (seconds, any steady clock).
    pub fn update(&mut self, at: &Position, facing: f32, now: f64) -> Steer {
        let here = global(at);
        self.learn(here, facing, now);
        let prev = self.last.map_or(here, |l| l.here);
        let strict = self.config.strict;
        if strict.is_none() && self.last.is_some_and(|l| !l.forward) {
            // Turning standing, or waiting at a door: no lack of progress. On a strict route,
            // which is never planned again, a body that cannot so much as turn is stuck.
            self.best_at = now;
        }
        let stride = self.stride();
        let last = self.path.waypoints.len().saturating_sub(1);
        if let (Some(_), None, Some(first)) = (strict, self.last, self.path.waypoints.get(1)) {
            // Setting out: a first point far round to one side is a corner like another, and
            // taken up on the run a bend as far round as one run out of.
            self.best_at = now;
            self.running_since = now;
            let p = global(&first.position);
            let sharp = self.config.sharp_turn + if self.under_way { BEND_LAG } else { 0.0 };
            self.corner = dist_xy(here, p) > self.config.arrive
                && turn_between(heading_to(here, p), facing).abs() > sharp;
        }
        loop {
            if self.next > last {
                // Whether it arrived running, for a route taken up from here.
                self.under_way = self.running();
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
            let reached = match strict {
                Some(range) => self.honored(self.next, here, prev, now, range),
                None => {
                    (level
                        && (dist_xy(here, p) <= close
                            || point_segment_dist_sq(flat(p, here), flat(prev, here), here)
                                <= close * close))
                        || (!stop && !sharp && self.crossed(self.next, here))
                        || self.at_shut_door(self.next, here)
                }
            };
            if !reached {
                break;
            }
            if strict.is_some() {
                self.corner = sharp;
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
        if strict.is_none() && (across > LOST_ACROSS || rise.abs() > LOST_RISE) {
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
            if strict.is_none() && self.path.legs.get(leg).is_some_and(|l| l.len() >= 2) {
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
        let mut heading = heading_to(here, aim);
        let to_point = turn_between(heading_to(here, target), facing);
        let running = self.running();
        if let Some(range) = strict {
            // Close on a point it will pass as it faces, the body does not chase it round.
            if running
                && d <= 2.0 * self.config.arrive
                && to_point.abs() < 90.0
                && d * sinf(to_point.abs().to_radians()) <= 0.8 * self.config.arrive.min(range)
            {
                heading = facing;
            }
        }
        let off = turn_between(heading, facing);
        #[allow(clippy::cast_possible_truncation)]
        let (per_frame, per_step) = (
            self.turn_rate * self.frame as f32,
            self.turn_rate * self.frame.max(WORLD_STEP) as f32,
        );
        let slack = self.config.turn_slack.max(per_frame);
        let across = dist_xy(here, aim);
        let sine = sinf(off.abs().to_radians());
        let forward = match strict {
            Some(range) => self.runs_on(range, d, to_point, off, slack),
            // Run while the point aimed at lies on or outside the turning circle that sets off
            // along the body's facing: a circle through it, tangent to the facing, has radius
            // `distance / (2 sin off)`. Running, the body keeps on until it is well inside;
            // standing, it sets off once it is well outside, so it does not stop and start frame
            // by frame.
            None => {
                let room = if running { 0.6 } else { 0.9 };
                off.abs() <= slack
                    || (off.abs() < 90.0 && across >= 2.0 * room * self.turning_radius() * sine)
            }
        };
        if forward && !running {
            self.running_since = now;
        }
        // A key is pressed for a turn past the band, and only for one at least a frame's turning,
        // or it overshoots and is turned back. Once down it is held until what is left is less
        // than one of the world's steps turns the body.
        let band = if forward {
            RUN_BAND.max(per_frame)
        } else {
            slack
        };
        let held = self
            .last
            .and_then(|l| l.turn)
            .is_some_and(|right| right == (off > 0.0) && off.abs() > per_step);
        let turn = (held || off.abs() > band).then_some(off > 0.0);
        let face = if let Some(right) = turn {
            self.swing = if right {
                self.turn_rate
            } else {
                -self.turn_rate
            };
            None
        } else if forward {
            // At a run the turn keeps pace with the arc that carries the body onto its aim.
            let least = if strict.is_some() && across > self.config.arrive {
                (2.0 * self.speed * sine / across).to_degrees()
            } else {
                0.0
            };
            self.ease(facing, off, least)
        } else {
            self.swing = 0.0;
            // Standing, the last of a turn is faced at once where a key held one frame would
            // overshoot it; otherwise a turn that small is left be.
            (per_frame > self.config.turn_slack && off.abs() > FACE_SLACK).then_some(heading)
        };
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
            face,
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
    /// a turn key turns it at `turn_rate`. As in the client, a frame's time passes, the follower
    /// is asked about the body as the last frame left it, and the world then moves the body
    /// through the frame with the keys it asked for, in thirtieths of a second or less, turning
    /// and running in each.
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
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let steps = (dt * 30.0).ceil().max(1.0) as u32;
            #[allow(clippy::cast_possible_truncation)]
            let t = (dt / f64::from(steps)) as f32;
            for _ in 0..steps {
                if let Some(right) = turn {
                    let d = self.turn_rate * t;
                    self.heading = (self.heading + if right { d } else { -d }).rem_euclid(360.0);
                }
                if forward {
                    let h = self.heading.to_radians();
                    self.x += sinf(h) * self.speed * t;
                    self.y += dereth_primitives::num::math::cosf(h) * self.speed * t;
                }
            }
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
        /// Degrees it turned, either way.
        turned: f32,
        /// How far its heading swung from side to side along the last leg, clear of its ends.
        last_leg_swing: f32,
        end: (f32, f32),
    }

    fn drive(path: &NavPath, fps: f64, speed: f32, turn_rate: f32) -> Drive {
        drive_frames(path, || 1.0 / fps, speed, turn_rate)
    }

    /// Drive `path` with frames as long as `frame` says, one after another.
    fn drive_frames(
        path: &NavPath,
        mut frame: impl FnMut() -> f64,
        speed: f32,
        turn_rate: f32,
    ) -> Drive {
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
        let (mut strayed, mut stops, mut forward, mut turned) = (0.0_f32, 0, false, 0.0_f32);
        let mut last_leg = (f32::MAX, f32::MIN);
        let mut arrived = false;
        while body.now < 120.0 {
            let here = Vec3::new(body.x, body.y, 0.0);
            let off = pts
                .windows(2)
                .map(|s| point_segment_dist_sq(here, s[0], s[1]).sqrt())
                .fold(f32::MAX, f32::min);
            strayed = strayed.max(off);
            let dt = frame();
            body.now += dt;
            match f.update(&at(body.x, body.y), body.heading, body.now) {
                Steer::Arrived => {
                    arrived = true;
                    break;
                }
                Steer::Go {
                    forward: go,
                    turn,
                    face,
                    ..
                } => {
                    if forward && !go {
                        stops += 1;
                    }
                    forward = go;
                    let facing = body.heading;
                    if let Some(h) = face {
                        body.heading = h;
                    }
                    body.frame(dt, go, turn);
                    turned += turn_between(body.heading, facing).abs();
                    // Along the last leg, clear of its ends: the body should hold one heading.
                    let (a, b) = (pts[pts.len() - 2], pts[pts.len() - 1]);
                    let at = Vec3::new(body.x, body.y, 0.0);
                    if f.next_waypoint() == pts.len() - 1
                        && dist_xy(at, a) > 8.0
                        && dist_xy(at, b) > 2.0
                    {
                        let d = turn_between(body.heading, heading_to(a, b));
                        last_leg.0 = last_leg.0.min(d);
                        last_leg.1 = last_leg.1.max(d);
                    }
                }
                s => panic!("{s:?} at {:.1} s", body.now),
            }
        }
        Drive {
            arrived,
            seconds: body.now,
            strayed,
            stops,
            turned,
            last_leg_swing: (last_leg.1 - last_leg.0).max(0.0),
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

    /// A closed door's waypoint as the planner puts it, a quarter metre before the door's place:
    /// inside the shut door, where no body stands. The door stops the body 0.67 m short of the
    /// waypoint, and the follower asks for the door to be opened there, at once, instead of
    /// pressing into it until it is stuck; once it is open the body walks on through it.
    ///
    /// Behaviour: plugins.nav.a-walk-meeting-a-closed-door-opens-it-at-once
    #[test]
    fn a_walk_meeting_a_closed_door_opens_it_at_once() {
        const DOOR: ObjectId = ObjectId(0x7019_E00C);
        let points = [
            (10.0, -40.0),
            (15.25, -40.0),
            (15.5, -40.0),
            (16.0, -40.0),
            (23.25, -39.75),
        ];
        let waypoints: Vec<Waypoint> = points
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| Waypoint {
                position: at(x, y),
                action: (i == 1).then_some(WaypointAction::OpenDoor(DOOR)),
            })
            .collect();
        let legs = waypoints[1..].iter().map(|w| vec![w.position]).collect();
        let cuts = vec![0.0; waypoints.len()];
        let path = NavPath {
            waypoints,
            legs,
            goal_offset: 0.0,
            cuts,
        };
        // The shut door: the body's front stops it at x 14.58.
        const STOPPED: f32 = 14.58;
        let mut body = Body {
            x: 10.0,
            y: -40.0,
            heading: 90.0,
            now: 0.0,
            speed: 3.0,
            turn_rate: 135.0,
        };
        let mut f = Follower::new(path, FollowConfig::default());
        let (mut open, mut stopped_at, mut opened_at) = (false, None, None);
        while body.now < 30.0 {
            let dt = 1.0 / 30.0;
            body.now += dt;
            match f.update(&at(body.x, body.y), body.heading, body.now) {
                Steer::Arrived => break,
                Steer::OpenDoor(door) => {
                    assert_eq!(door, DOOR);
                    assert!(!open, "asked again for an open door");
                    opened_at = Some(body.now);
                    open = true;
                    f.door_opened();
                }
                Steer::Go {
                    forward,
                    turn,
                    face,
                    ..
                } => {
                    if let Some(h) = face {
                        body.heading = h;
                    }
                    body.frame(dt, forward, turn);
                    if !open && body.x >= STOPPED {
                        body.x = STOPPED;
                        stopped_at.get_or_insert(body.now);
                    }
                }
                s => panic!("{s:?} at {:.2} s, x {:.2}", body.now, body.x),
            }
        }
        let opened = opened_at.expect("the door was opened");
        if let Some(stopped) = stopped_at {
            assert!(
                opened - stopped < 0.1,
                "pressed into the shut door for {:.2} s",
                opened - stopped
            );
        }
        assert!(
            body.x > 22.5,
            "walked on through the door to x {:.2}",
            body.x
        );
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
    fn a_body_on_the_level_part_of_a_leg_that_ends_down_a_ramp_is_on_the_route() {
        let place = |x: f32, z: f32| {
            Position::new(
                CellId(0x0101_0100),
                Frame::new(Vec3::new(x, 0.0, z), Quat::IDENTITY),
            )
        };
        // Level for twelve metres, then six metres down a ramp eight metres long.
        let mut path = route_through(&[(0.0, 0.0), (20.0, 0.0)]);
        path.waypoints[1].position = place(20.0, -6.0);
        path.legs[0] = (1..=20)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let x = i as f32;
                place(x, if x <= 12.0 { 0.0 } else { -(x - 12.0) * 0.75 })
            })
            .collect();
        let mut f = Follower::new(path, FollowConfig::default());
        // At the ramp's head, still on the level: four and a half metres above the straight line
        // between the leg's ends, and on the floor the search went across.
        for (i, x) in [0.0_f32, 4.0, 8.0, 11.5].into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let now = i as f64 * 0.5;
            let s = f.update(&place(x, 0.0), 90.0, now);
            assert!(matches!(s, Steer::Go { .. }), "at {x} m: {s:?}");
        }
        assert_eq!(f.stuck_events, 0);
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
    fn a_turn_smaller_than_a_frames_turning_is_finished_by_facing_the_point() {
        let f = Follower::new(
            route_through(&[(0.0, 0.0), (0.0, 50.0)]),
            FollowConfig::default(),
        );
        let steer_at = |facing: f32, dt: f64| {
            let mut f = f.clone();
            let _ = f.update(&at(0.0, 0.0), facing, 0.0);
            match f.update(&at(0.0, 0.0), facing, dt) {
                Steer::Go { turn, face, .. } => (turn, face),
                s => panic!("{s:?}"),
            }
        };
        // At five frames a second a turn key held one frame turns the body 27 degrees: a body
        // 10 degrees off faces north at once, while at 60 frames a second it is eased round,
        // less in the frame than a key held would turn it.
        assert_eq!(steer_at(10.0, 0.2), (None, Some(0.0)));
        match steer_at(10.0, 1.0 / 60.0) {
            (None, Some(h)) => assert!((10.0 - 135.0 / 60.0..10.0).contains(&h), "{h}"),
            other => panic!("{other:?}"),
        }
        // A body further off than a frame's turning turns, and one barely off is left be.
        assert_eq!(steer_at(40.0, 0.2), (Some(false), None));
        assert_eq!(steer_at(0.5, 0.2), (None, None));
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

    #[test]
    fn at_a_low_frame_rate_the_body_turns_no_more_than_the_route_needs() {
        // Bends both ways, a sharp corner, and long legs whose headings no whole number of a low
        // frame rate's turns from north lines up with: the last runs 40 metres at 7 degrees.
        let path = route_through(&[
            (0.0, 0.0),
            (0.0, 12.0),
            (6.0, 22.4),
            (6.0, 34.0),
            (20.0, 34.0),
            (10.0, 44.0),
            (10.0, 52.0),
            (40.0, 60.0),
            (45.0, 100.0),
        ]);
        let smooth = drive(&path, 60.0, 8.0, 129.0);
        assert!(smooth.arrived);
        // Frames that swing from 0.4 to 1.6 times their mean length, from a fixed sequence.
        let uneven = |mean: f64| {
            let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
            move || {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                #[allow(clippy::cast_precision_loss)]
                let r = (seed >> 11) as f64 / (1_u64 << 53) as f64;
                mean * (0.4 + 1.2 * r)
            }
        };
        let drives = [
            ("10 frames a second", drive(&path, 10.0, 8.0, 129.0)),
            ("7 frames a second", drive(&path, 7.0, 8.0, 129.0)),
            ("5 frames a second", drive(&path, 5.0, 8.0, 129.0)),
            (
                "uneven frames, 10 a second",
                drive_frames(&path, uneven(0.1), 8.0, 129.0),
            ),
            (
                "uneven frames, 5 a second",
                drive_frames(&path, uneven(0.2), 8.0, 129.0),
            ),
        ];
        for (how, d) in drives {
            assert!(d.arrived, "{how}: did not arrive");
            // Turning back what it has just turned, over and over, is swinging from side to side.
            assert!(
                d.turned < 1.15 * smooth.turned,
                "{how}: turned {:.0} degrees against {:.0} at 60 frames a second",
                d.turned,
                smooth.turned
            );
            // Along a leg that lies between two headings a whole frame's turning apart (13 to 26
            // degrees here), the body holds the one between rather than flicking from one to the
            // other.
            assert!(
                d.last_leg_swing < 4.0,
                "{how}: swung {:.1} degrees from side to side along the last leg",
                d.last_leg_swing
            );
        }
    }

    /// What driving someone's own points strictly came to.
    struct Walked {
        arrived: bool,
        stuck: bool,
        /// The waypoints in the order they were counted reached.
        reached: Vec<usize>,
        /// Per point, how near the body came before it was counted reached.
        nearest: Vec<f32>,
        stops: u32,
        stood_frames: u32,
    }

    /// Drive `points` strictly from the first, facing north.
    fn walk_strictly(points: &[(f32, f32)], range: f32, fps: f64, speed: f32) -> Walked {
        let pts: Vec<Vec3> = points.iter().map(|&(x, y)| Vec3::new(x, y, 0.0)).collect();
        let mut path = route_through(points);
        path.cuts.clear();
        path.legs.clear();
        let mut body = Body {
            x: pts[0].x,
            y: pts[0].y,
            heading: 0.0,
            now: 0.0,
            speed,
            turn_rate: 129.0,
        };
        let config = FollowConfig {
            speed,
            turn_rate: 129.0,
            ..FollowConfig::strict(range, 6.0)
        };
        let mut f = Follower::new(path, config);
        let mut out = Walked {
            arrived: false,
            stuck: false,
            reached: Vec::new(),
            nearest: vec![f32::MAX; pts.len()],
            stops: 0,
            stood_frames: 0,
        };
        let (mut running, mut prev) = (false, pts[0]);
        while body.now < 300.0 {
            let here = Vec3::new(body.x, body.y, 0.0);
            let next = f.next_waypoint();
            for (p, nearest) in pts.iter().zip(&mut out.nearest).skip(next) {
                *nearest = nearest.min(point_segment_dist_sq(*p, prev, here).sqrt());
            }
            prev = here;
            let dt = 1.0 / fps;
            body.now += dt;
            let step = f.update(&at(body.x, body.y), body.heading, body.now);
            out.reached.extend(next..f.next_waypoint());
            match step {
                Steer::Arrived => {
                    out.arrived = true;
                    break;
                }
                Steer::Go {
                    forward,
                    turn,
                    face,
                    ..
                } => {
                    out.stops += u32::from(running && !forward);
                    out.stood_frames += u32::from(!forward);
                    running = forward;
                    if let Some(h) = face {
                        body.heading = h;
                    }
                    body.frame(dt, forward, turn);
                }
                Steer::Stuck { .. } => {
                    out.stuck = true;
                    break;
                }
                s @ Steer::OpenDoor(_) => panic!("{s:?}"),
            }
        }
        out
    }

    /// Points `gap` apart from the origin, the route turning by each of `bends` at the next.
    fn bending(gap: f32, bends: &[f32]) -> Vec<(f32, f32)> {
        let (mut x, mut y, mut h) = (0.0_f32, 0.0_f32, 0.0_f32);
        let mut out = vec![(x, y)];
        for &bend in std::iter::once(&0.0).chain(bends) {
            h += bend;
            x += gap * sinf(h.to_radians());
            y += gap * dereth_primitives::num::math::cosf(h.to_radians());
            out.push((x, y));
        }
        out
    }

    /// Behaviour: plugins.nav.a-path-move-honors-every-point-within-its-range
    #[test]
    fn a_strict_route_comes_within_range_of_every_point_in_order_at_any_frame_rate() {
        let routes = [
            bending(4.0, &[10.0; 12]),
            bending(6.5, &[9.5; 37]),
            bending(10.0, &[20.0, -40.0, 30.0, -25.0, 44.0, -30.0]),
            bending(8.0, &[90.0, -135.0, 60.0, 0.0, 180.0, 75.0]),
            bending(3.0, &[35.0, 35.0, -70.0, 20.0]),
        ];
        for (n, points) in routes.iter().enumerate() {
            for range in [0.5_f32, 1.0, 3.0] {
                for fps in [144.0, 60.0, 30.0, 15.0] {
                    let w = walk_strictly(points, range, fps, 11.4);
                    let case = format!("route {n}, range {range}, {fps} frames a second");
                    assert!(w.arrived && !w.stuck, "{case}: {:?}", w.reached);
                    assert_eq!(w.reached, (1..points.len()).collect::<Vec<_>>(), "{case}");
                    for (i, d) in w.nearest.iter().enumerate().skip(1) {
                        assert!(*d <= range + 1e-3, "{case}: {d:.2} m from point {i}");
                    }
                }
            }
        }
    }

    /// Behaviour: plugins.nav.a-path-move-cuts-no-corner
    #[test]
    fn a_strict_route_runs_up_to_a_corner_before_it_turns_however_wide_its_range() {
        // Legs long enough to settle on, and corners a planned route would cut three metres off.
        let points = bending(14.0, &[90.0, -90.0, 120.0]);
        for fps in [144.0, 30.0] {
            let w = walk_strictly(&points, 5.0, fps, 8.0);
            assert!(w.arrived, "{fps}");
            for (i, d) in w.nearest.iter().enumerate().skip(1) {
                assert!(*d <= 0.7, "{fps} frames a second: {d:.2} m from point {i}");
            }
            assert!(w.stops >= 3, "{fps}: each sharp corner is turned standing");
        }
    }

    /// Behaviour: plugins.nav.a-path-move-cuts-no-corner
    #[test]
    fn a_strict_route_runs_out_each_short_leg_of_a_zigzag_within_its_range() {
        // Two metres between corners of 120 degrees, each next point behind and within range.
        let points = bending(2.0, &[0.0, 0.0, 120.0, -120.0, 120.0, -120.0]);
        for fps in [144.0, 60.0, 30.0] {
            let w = walk_strictly(&points, 3.0, fps, 8.0);
            assert!(w.arrived, "{fps}");
            for (i, d) in w.nearest.iter().enumerate().skip(1) {
                assert!(*d <= 0.7, "{fps} frames a second: {d:.2} m from point {i}");
            }
        }
    }

    /// Behaviour: plugins.nav.a-path-move-runs-through-bends-without-stopping
    #[test]
    fn a_strict_route_runs_through_bends_under_the_sharp_angle_without_stopping() {
        let routes = [
            (bending(5.0, &[9.5; 37]), 0.5),
            (bending(4.0, &[10.0; 20]), 1.0),
            (bending(9.0, &[25.0, -30.0, 20.0, -25.0]), 2.0),
            (bending(14.0, &[44.0, -40.0, 35.0, -44.0]), 3.0),
        ];
        for (n, (points, range)) in routes.iter().enumerate() {
            for fps in [144.0, 60.0, 30.0] {
                let w = walk_strictly(points, *range, fps, 11.4);
                assert!(w.arrived, "route {n}, {fps}");
                assert_eq!(
                    (w.stops, w.stood_frames),
                    (0, 0),
                    "route {n}, {fps} frames a second: stops, frames standing"
                );
            }
        }
    }

    #[test]
    fn a_strict_route_taken_up_on_the_run_goes_on_running_through_the_bend_between() {
        let mut first = Follower::new(
            route_through(&[(0.0, 0.0), (0.0, 10.0)]),
            FollowConfig::strict(2.0, 6.0),
        );
        assert!(matches!(
            first.update(&at(0.0, 9.0), 0.0, 1.0),
            Steer::Go { forward: true, .. }
        ));
        assert_eq!(first.update(&at(0.0, 9.6), 0.0, 1.05), Steer::Arrived);
        // Fifty degrees round: further than a standing body sets off at.
        let next = route_through(&[(0.0, 9.6), (9.0, 17.1)]);
        let config = FollowConfig::strict(3.0, 6.0);
        let mut fresh = Follower::new(next.clone(), config);
        assert!(matches!(
            fresh.update(&at(0.0, 9.6), 0.0, 1.1),
            Steer::Go { forward: false, .. }
        ));
        // Taken up on the run, it is run through.
        let mut carried = first.then(next, config);
        assert_eq!(carried.pace(), first.pace());
        assert!(matches!(
            carried.update(&at(0.0, 9.6), 0.0, 1.1),
            Steer::Go { forward: true, .. }
        ));
    }

    /// Behaviour: plugins.nav.a-path-move-honors-every-point-within-its-range
    #[test]
    fn a_point_passed_is_counted_only_within_range() {
        // Running north, a metre and a half west of the first point as it goes by.
        let pass = |range: f32| {
            let mut f = Follower::new(
                route_through(&[(0.0, 0.0), (1.5, 10.0), (1.5, 30.0)]),
                FollowConfig::strict(range, 6.0),
            );
            let _ = f.update(&at(0.0, 0.0), 8.5, 0.0);
            f.last = f.last.map(|l| Seen { forward: true, ..l });
            let (by, past) = (global(&at(0.0, 10.0)), global(&at(0.0, 10.4)));
            f.honored(1, past, by, 1.0, range)
        };
        assert!(pass(2.0), "passed within two metres: reached");
        assert!(!pass(1.0), "passed beyond a metre: still to reach");
    }

    /// Behaviour: plugins.nav.a-path-move-honors-every-point-within-its-range
    #[test]
    fn a_point_something_stands_on_is_reached_within_range_and_stuck_beyond_it() {
        // The body is stopped a metre and a half short of the first point.
        let blocked = |range: f32| {
            let mut f = Follower::new(
                route_through(&[(0.0, 0.0), (0.0, 10.0), (10.0, 10.0)]),
                FollowConfig::strict(range, 6.0),
            );
            let (mut now, mut y) = (0.0, 0.0_f32);
            while now < 20.0 {
                now += 1.0 / 30.0;
                y = (y + 8.0 / 30.0).min(8.5);
                match f.update(&at(0.0, y), 0.0, now) {
                    Steer::Go { waypoint: 2, .. } => return Ok(now),
                    Steer::Stuck { waypoint } => return Err((waypoint, now)),
                    _ => {}
                }
            }
            panic!("neither reached nor stuck");
        };
        assert!(blocked(2.0).is_ok_and(|t| t < 2.5));
        assert!(blocked(1.0).is_err_and(|(i, t)| i == 1 && (6.0..9.0).contains(&t)));
    }

    #[test]
    fn a_strict_route_looks_at_no_heights_and_is_never_lost() {
        let mut path = route_through(&[(0.0, 0.0), (0.0, 20.0)]);
        path.waypoints[1].position.frame.origin.z = 84.0;
        let mut f = Follower::new(path, FollowConfig::strict(1.0, 6.0));
        // Pushed ten metres to one side, it walks back to its point.
        assert!(matches!(
            f.update(&at(0.0, 2.0), 0.0, 0.1),
            Steer::Go { .. }
        ));
        assert!(matches!(
            f.update(&at(10.0, 2.0), 0.0, 0.2),
            Steer::Go { .. }
        ));
        assert_eq!(f.update(&at(0.0, 19.8), 0.0, 3.0), Steer::Arrived);
    }

    /// Behaviour: plugins.nav.steering-through-a-bend-never-taps-the-turn-keys
    #[test]
    fn a_curve_is_steered_through_without_tapping_the_turn_keys_or_snapping_round() {
        let points = bending(6.5, &[9.5; 20]);
        let strictly = FollowConfig::strict(1.0, 6.0);
        for (cut, config) in [(3.0, FollowConfig::default()), (0.0, strictly)] {
            let mut path = route_through(&points);
            path.cuts = vec![cut; points.len()];
            for fps in [144.0, 60.0, 30.0] {
                let mut f = Follower::new(path.clone(), config);
                let mut body = Body {
                    x: 0.0,
                    y: 0.0,
                    heading: 0.0,
                    now: 0.0,
                    speed: 11.4,
                    turn_rate: 129.0,
                };
                let (mut presses, mut held, mut most) = (0, None, 0.0_f32);
                while body.now < 60.0 {
                    body.now += 1.0 / fps;
                    let Steer::Go {
                        forward,
                        turn,
                        face,
                        ..
                    } = f.update(&at(body.x, body.y), body.heading, body.now)
                    else {
                        break;
                    };
                    presses += u32::from(turn.is_some() && turn != held);
                    held = turn;
                    if let Some(h) = face {
                        most = most.max(turn_between(h, body.heading).abs());
                        body.heading = h;
                    }
                    body.frame(1.0 / fps, forward, turn);
                }
                assert!(f.next_waypoint() >= points.len(), "{fps}: not arrived");
                assert_eq!(presses, 0, "cut {cut}, {fps} frames a second: keys tapped");
                #[allow(clippy::cast_possible_truncation)]
                let turns_a_frame = 136.0 / fps as f32;
                assert!(most <= turns_a_frame, "cut {cut}, {fps}: snapped {most:.1}");
            }
        }
    }

    /// Behaviour: plugins.nav.steering-through-a-bend-never-taps-the-turn-keys
    #[test]
    fn a_turn_past_the_band_holds_its_key_until_a_steps_turning_is_left() {
        let mut f = Follower::new(
            route_through(&[(0.0, 0.0), (0.0, 80.0)]),
            FollowConfig::default(),
        );
        let mut body = Body {
            x: 0.0,
            y: 0.0,
            heading: 35.0,
            now: 0.0,
            speed: 8.0,
            turn_rate: 135.0,
        };
        let mut presses = Vec::new();
        let mut held = None;
        while body.now < 2.0 {
            body.now += 1.0 / 60.0;
            let Steer::Go {
                forward,
                turn,
                face,
                ..
            } = f.update(&at(body.x, body.y), body.heading, body.now)
            else {
                break;
            };
            if turn != held {
                presses.push((turn, turn_between(body.heading, 0.0)));
                held = turn;
            }
            if let Some(h) = face {
                body.heading = h;
            }
            body.frame(1.0 / 60.0, forward, turn);
        }
        assert_eq!(presses.len(), 2, "pressed once, let go once: {presses:?}");
        assert_eq!(presses[0].0, Some(false));
        assert!(presses[1].1 < 10.0, "held nearly round: {presses:?}");
    }
}
