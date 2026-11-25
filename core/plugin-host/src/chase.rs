//! Following something that moves (`nav.follow`): where the one followed is going, how fast, and
//! the way after it.
//!
//! The host looks at the followed object's place every frame. From the places it changed to, and
//! when, it reckons the object's pace, where it has got to between two updates of its place, and
//! whether it stands. The character is steered by the journeys' follower over a route that keeps
//! up with the object:
//!
//! - **While the line to it is clear,** straight at where it is going: its place a moment ahead
//!   at its pace, so the character does not chase where it was.
//! - **While something stands between,** along the places it went through, laid as it goes from
//!   the last place it was seen at: round the corner it turned, not into the wall. Where the host
//!   cannot tell whether the line is clear, the trail is followed too.
//!
//! The follower turns while it runs, and stops to turn only at a corner sharper than 45° (as a
//! move along does). Near the range the character eases off to a walk while the object moves away,
//! and runs again once it has fallen back a metre behind the range; it stands only within the
//! range of an object that is not moving away, or far too close to it.

use std::collections::VecDeque;

use dereth_navigation::{FollowConfig, Follower, NavPath, Steer, Waypoint};
use dereth_primitives::Position;

use crate::places;

/// Seconds of the object's changes of place its pace is reckoned over.
pub const PACE_SECS: f64 = 1.5;
/// An object whose place has not changed for this many of the spells between its changes stands,
/// and for this long at least and at most, seconds.
pub const STILL_SPELLS: f64 = 2.5;
pub const STILL_LEAST: f64 = 0.3;
pub const STILL_MOST: f64 = 1.5;
/// The longest the object's place is carried on at its pace past its last change, seconds: no
/// longer than half a spell between its changes, either, so a stop is not overrun far.
pub const RECKON_SECS: f64 = 0.5;
/// A change of place this many spells late may mean the object has stopped: until it comes, the
/// object is taken to stand where it was last seen.
pub const LATE_SPELLS: f64 = 1.5;
/// How far ahead of an object in sight the character aims, in seconds at its pace, and at most
/// how many metres.
pub const LEAD_SECS: f64 = 0.5;
pub const LEAD_MOST: f64 = 6.0;
/// Metres between two places laid on the object's trail.
pub const TRAIL_STEP: f64 = 2.0;
/// How near a place on the trail the character passes for it to count, metres.
pub const TRAIL_REACH: f32 = 1.5;
/// The most places the route holds; past it, the character heads straight for the object.
pub const TRAIL_MOST: usize = 256;
/// A change of place longer than this, metres, is a portal's or a recall's: the trail starts
/// again.
pub const CARRIED: f64 = 40.0;
/// Seconds between two looks along the line to the object.
pub const SIGHT_SECS: f64 = 0.2;
/// Metres past the range the character falls back, walking, before it runs again.
pub const EASE_BAND: f64 = 1.0;
/// Metres past the range a standing character lets the object get before it sets off again.
pub const REST_BAND: f64 = 1.0;
/// The pace, metres a second, at which an object counts as moving away.
pub const AWAY_PACE: f64 = 0.5;
/// The share of the range within which the character stands whatever the object does.
pub const TOO_CLOSE: f64 = 0.5;

/// What the character's controls do this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Drive {
    /// Run (the run lock on), or stand.
    pub run: bool,
    /// Hold the walk key: walk where it would run.
    pub walk: bool,
    /// Hold the turn-right key (`Some(true)`), the turn-left key (`Some(false)`), or neither.
    pub turn: Option<bool>,
    /// Face this heading at once, degrees clockwise from north.
    pub face: Option<f32>,
}

/// A follow under way.
#[derive(Debug, Clone, Default)]
pub struct Chase {
    /// The object's places as they changed, and when, the newest last.
    changes: VecDeque<(f64, Position)>,
    /// The steering, once it has set out.
    follower: Option<Follower>,
    /// The last place laid on the trail.
    laid: Option<Position>,
    /// The last look along the line to the object: when, and whether it was clear.
    look: Option<(f64, Option<bool>)>,
    /// Where the object was at the last look that found the line clear.
    in_sight: Option<Position>,
    /// Where the character was last frame.
    was: Option<Position>,
    /// Whether the character walks, easing off near the range.
    walking: bool,
    /// Whether the character stands within the range.
    standing: bool,
}

/// `p` moved `dx`, `dy` metres on the ground.
#[allow(clippy::cast_possible_truncation)]
fn shifted(p: &Position, dx: f64, dy: f64) -> Position {
    let mut out = *p;
    out.frame.origin.x += dx as f32;
    out.frame.origin.y += dy as f32;
    out
}

fn ground_from(a: &Position, b: &Position) -> (f64, f64) {
    let (ax, ay, _) = places::global(a);
    let (bx, by, _) = places::global(b);
    (bx - ax, by - ay)
}

impl Chase {
    /// Whether the character is standing within range.
    #[must_use]
    pub const fn standing(&self) -> bool {
        self.standing
    }

    /// Whether the character is walking, easing off near the range.
    #[must_use]
    pub const fn walking(&self) -> bool {
        self.walking
    }

    /// The steering, once set out: its route ends where the character is headed.
    #[must_use]
    pub const fn follower(&self) -> Option<&Follower> {
        self.follower.as_ref()
    }

    /// Start the way after the object again (it, or the character, was carried).
    fn start_again(&mut self) {
        self.follower = None;
        self.laid = None;
        self.in_sight = None;
        self.look = None;
    }

    /// The object seen at `at` at `now`.
    fn saw(&mut self, at: Position, now: f64) {
        match self.changes.back() {
            Some((_, last)) if places::distance(last, &at) < 0.01 => return,
            Some((_, last)) if places::distance(last, &at) > CARRIED => {
                self.changes.clear();
                self.start_again();
            }
            _ => {}
        }
        self.changes.push_back((now, at));
        while self.changes.len() > 1
            && self
                .changes
                .front()
                .is_some_and(|(t, _)| *t < now - PACE_SECS)
        {
            self.changes.pop_front();
        }
    }

    /// The seconds between the object's changes of place, as they have come lately.
    fn spell(&self) -> Option<f64> {
        let (&(t0, _), &(t1, _)) = (self.changes.front()?, self.changes.back()?);
        #[allow(clippy::cast_precision_loss)]
        let n = (self.changes.len() - 1) as f64;
        (n > 0.0 && t1 > t0).then(|| (t1 - t0) / n)
    }

    /// The object's pace on the ground, metres a second east and north: nothing once it has
    /// stood still a while.
    #[must_use]
    pub fn pace(&self, now: f64) -> (f64, f64) {
        let (Some(&(t0, ref p0)), Some(&(t1, ref p1)), Some(spell)) =
            (self.changes.front(), self.changes.back(), self.spell())
        else {
            return (0.0, 0.0);
        };
        if now - t1 > (STILL_SPELLS * spell).clamp(STILL_LEAST, STILL_MOST) {
            return (0.0, 0.0);
        }
        let (dx, dy) = ground_from(p0, p1);
        (dx / (t1 - t0), dy / (t1 - t0))
    }

    /// Whether the object's next change of place is late at `now`.
    fn late(&self, now: f64) -> bool {
        match (self.changes.back(), self.spell()) {
            (Some(&(t, _)), Some(spell)) => now - t > LATE_SPELLS * spell,
            _ => true,
        }
    }

    /// Where the object has got to by `now`: its last place, carried on at its pace a little.
    fn reckoned(&self, now: f64) -> Option<Position> {
        let &(t, ref p) = self.changes.back()?;
        let (vx, vy) = self.pace(now);
        let most = (0.5 * self.spell().unwrap_or(0.0)).min(RECKON_SECS);
        let ahead = (now - t).clamp(0.0, most);
        Some(shifted(p, vx * ahead, vy * ahead))
    }

    /// One frame of the follow: the character at `me` after the object seen at `at`, within
    /// `range` metres, at `now` (seconds, any steady clock). `sight` says whether a straight line
    /// between two places is clear, `None` where it cannot tell; it is asked a few times a second.
    #[allow(clippy::cast_possible_truncation)]
    pub fn step(
        &mut self,
        me: &Position,
        at: Position,
        range: f64,
        now: f64,
        sight: impl FnOnce(&Position, &Position) -> Option<bool>,
    ) -> Drive {
        if self
            .was
            .replace(*me)
            .is_some_and(|w| places::distance(&w, me) > CARRIED)
        {
            self.start_again();
        }
        self.saw(at, now);
        let late = self.late(now);
        let (vx, vy) = if late { (0.0, 0.0) } else { self.pace(now) };
        let leader = if late {
            at
        } else {
            self.reckoned(now).unwrap_or(at)
        };
        let d = places::ground_distance(me, &leader);
        let (ex, ey) = ground_from(me, &leader);
        let away = if d > 0.01 {
            (vx * ex + vy * ey) / d
        } else {
            0.0
        };
        let moving_away = away > AWAY_PACE;
        let rest = if self.standing {
            range + REST_BAND
        } else {
            range
        };
        if d <= TOO_CLOSE * range || (d <= rest && !moving_away) {
            self.standing = true;
            self.walking = false;
            if let Some(f) = self.follower.as_mut() {
                f.held_up(now);
            }
            return Drive::default();
        }
        self.standing = false;
        self.walking = if self.walking {
            d < range + EASE_BAND
        } else {
            d <= range
        };

        if self.look.is_none_or(|(t, _)| now - t >= SIGHT_SECS) {
            let clear = sight(me, &at);
            self.look = Some((now, clear));
            if clear == Some(true) {
                self.in_sight = Some(at);
            }
        }
        let clear = self.look.and_then(|(_, c)| c) == Some(true);
        let aim = if clear {
            let (mut lx, mut ly) = (vx * LEAD_SECS, vy * LEAD_SECS);
            let lead = (lx * lx + ly * ly).sqrt();
            if lead > LEAD_MOST {
                (lx, ly) = (lx * LEAD_MOST / lead, ly * LEAD_MOST / lead);
            }
            shifted(&leader, lx, ly)
        } else {
            leader
        };
        let f = self.follower.get_or_insert_with(|| {
            let path = NavPath {
                waypoints: [*me, aim]
                    .into_iter()
                    .map(|position| Waypoint {
                        position,
                        action: None,
                    })
                    .collect(),
                legs: Vec::new(),
                goal_offset: 0.0,
                cuts: Vec::new(),
            };
            Follower::new(path, FollowConfig::strict(TRAIL_REACH, f64::INFINITY))
        });
        if clear {
            f.straight_to_end(*me);
            self.laid = Some(at);
        } else {
            // Out of sight: the way round goes from where it was last seen, along where it went.
            if let Some(seen) = self.in_sight.take() {
                f.lay_before_end(seen);
                self.laid = Some(seen);
            }
            if self
                .laid
                .is_none_or(|l| places::ground_distance(&l, &at) >= TRAIL_STEP)
            {
                f.lay_before_end(at);
                self.laid = Some(at);
            }
            f.forget_passed();
            if f.path().waypoints.len() > TRAIL_MOST {
                f.straight_to_end(*me);
            }
        }
        f.move_end(aim);
        let heading = places::heading(me) as f32;
        let mut steer = f.update(me, heading, now);
        if steer == Steer::Arrived {
            // On the place aimed at: on from here toward it again.
            f.straight_to_end(*me);
            steer = f.update(me, heading, now);
        }
        match steer {
            Steer::Go {
                forward,
                turn,
                face,
                ..
            } => Drive {
                run: forward,
                walk: forward && self.walking,
                turn,
                face,
            },
            Steer::Arrived | Steer::Stuck { .. } | Steer::OpenDoor(_) => Drive::default(),
        }
    }
}
