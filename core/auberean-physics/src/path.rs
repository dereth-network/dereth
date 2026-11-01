//! The steering under a move: heading for a place in a straight line ([`SimplePath`]), turning to a
//! heading ([`Turner`]), and noticing a body that is not getting anywhere
//! ([`Headway`]).

use std::collections::VecDeque;

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::geom::{self, Heading, EPS};

/// How a step of steering went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Not there yet.
    Continue = 1,
    /// Arrived.
    Success = 2,
    /// Too far, or stuck under or over the destination.
    Failure = 3,
}

/// Heading straight for a place: walking toward it while facing within a quarter turn of it,
/// slowing on the last stretch, until within the desired distance.
// D2-15810
#[derive(Clone, Debug, PartialEq)]
pub struct SimplePath {
    pub destination: Position,
    /// The mover's radius and height.
    pub radius: f32,
    pub height: f32,
    /// The destination's radius and height, when it is an object.
    pub target_radius: f32,
    pub target_height: f32,
    pub always_turn: bool,
    pub use_cylinders: bool,
    pub desired_distance: f32,
    pub max_success_distance: f32,
    pub fail_distance: f32,
    pub desired_velocity: f32,
}

impl Default for SimplePath {
    /// Distances between cylinders: arrive within 0.1 m, succeed within 5 m, fail past 100 m.
    // D2-15806
    fn default() -> Self {
        Self {
            destination: Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY)),
            radius: 0.0,
            height: 0.0,
            target_radius: 0.0,
            target_height: 0.0,
            always_turn: false,
            use_cylinders: true,
            desired_distance: 0.1,
            max_success_distance: 5.0,
            fail_distance: 100.0,
            desired_velocity: 0.0,
        }
    }
}

impl SimplePath {
    /// The mover's size and the move's distances and speed.
    // D2-15807
    pub fn set_parameters(
        &mut self,
        radius: f32,
        height: f32,
        desired_distance: f32,
        max_success_distance: f32,
        fail_distance: f32,
        desired_velocity: f32,
    ) {
        self.radius = radius;
        self.height = height;
        self.desired_distance = desired_distance;
        self.max_success_distance = max_success_distance;
        self.fail_distance = fail_distance;
        self.desired_velocity = desired_velocity;
    }

    // D2-15808
    pub fn set_always_turn(&mut self, on: bool) {
        self.always_turn = on;
    }

    // D2-15809
    pub fn set_use_cylinders(&mut self, on: bool) {
        self.use_cylinders = on;
    }

    /// Where to go, and the size of what stands there.
    // D2-15812
    pub fn set_destination(&mut self, p: Position, radius: f32, height: f32) {
        self.destination = p;
        self.target_radius = radius;
        self.target_height = height;
    }

    fn distance(&self, from: &Position) -> f32 {
        if self.use_cylinders {
            geom::cylinder_distance(
                self.radius,
                self.height,
                from,
                self.target_radius,
                self.target_height,
                &self.destination,
            )
        } else {
            geom::distance(from, &self.destination)
        }
    }

    /// The distance to the destination across the ground alone.
    // D2-15813
    #[must_use]
    pub fn xy_distance(&self, from: &Position) -> f32 {
        let mut level = *from;
        level.frame.origin.z = self.destination.frame.origin.z;
        self.distance(&level)
    }

    /// One step from `from`: the motion to ask for (forward and sideways, each at most the
    /// desired velocity) and the heading to turn to, and whether the move goes on.
    ///
    /// The body walks only while it faces within a quarter turn of the destination, straight for
    /// it; within half the success distance it slows in proportion, to no less than three tenths.
    /// It has arrived within the desired distance; it has failed past the failing distance, or
    /// when right over or under the destination (across the ground under a fifth of the desired
    /// distance) yet farther than the success distance.
    // D2-15814
    pub fn update(&self, from: &Position) -> (Step, Vec3, Heading) {
        let dist = self.distance(from);
        let to = geom::heading_to(from, &self.destination);
        let facing = geom::frame_heading(&from.frame);
        let mut motion = Vec3::ZERO;
        if dist > self.desired_distance && facing.is_in_range(to, Heading::WALK_CONE) {
            let g = geom::global_to_local(&from.frame, geom::offset(from, &self.destination));
            let (x, y) = (g.x, g.y);
            let len = (x * x + y * y).sqrt();
            motion = if len < EPS {
                Vec3::ZERO
            } else {
                let k = 1.0 / len;
                Vec3::new(k * x, k * y, k * 0.0)
            };
            let half = self.max_success_distance * 0.5;
            if dist < half && dist > self.desired_distance {
                let f = ((dist - self.desired_distance) / (half - self.desired_distance)).max(0.3);
                motion = Vec3::new(f * motion.x, f * motion.y, f * motion.z);
            }
        }
        let v = self.desired_velocity;
        motion = Vec3::new(
            if motion.x < v { motion.x } else { v },
            if motion.y < v { motion.y } else { v },
            if motion.z < v { motion.z } else { v },
        );
        let step = if dist >= self.fail_distance || dist.is_nan() {
            Step::Failure
        } else if dist <= self.desired_distance {
            Step::Success
        } else if self.xy_distance(from) >= self.desired_distance * 0.2
            || dist <= self.max_success_distance
        {
            Step::Continue
        } else {
            Step::Failure
        };
        (step, motion, to)
    }
}

/// Turning to a heading.
// D2-15818
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Turner {
    pub desired: Heading,
}

impl Turner {
    // D2-15817
    pub fn set_desired_heading(&mut self, h: Heading) {
        self.desired = h;
    }

    /// The heading to turn to, and whether a body facing `current` is done turning: within ten
    /// degrees.
    // D2-15816, D2-15815
    #[must_use]
    pub fn update(&self, current: Heading) -> (Step, Heading) {
        let left = self.desired.minus(current).abs();
        let step = if left.is_inside(Heading::TURN_CLOSE_ENOUGH) {
            Step::Success
        } else {
            Step::Continue
        };
        (step, self.desired)
    }
}

/// Speeds below this are no motion to check.
pub const MINIMUM_SPEED: f32 = 0.0002;

/// Watches a moving body's speed toward its goal over the last second against the speed it asks
/// for: one making less than a tenth of the progress it asks for is stuck.
// D2-15829, D2-15832
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Headway {
    /// Speeds made, with when.
    pub history: VecDeque<(f64, f32)>,
    /// Speeds asked for, with when.
    pub desired: VecDeque<(f64, f32)>,
    pub start: f64,
}

impl Headway {
    // D2-15831
    #[must_use]
    pub fn new(now: f64) -> Self {
        Self {
            history: VecDeque::new(),
            desired: VecDeque::new(),
            start: now,
        }
    }

    // D2-15826
    pub fn reset(&mut self, now: f64) {
        self.start = now;
        self.history.clear();
        self.desired.clear();
    }

    /// A speed made toward the goal.
    // D2-15833
    pub fn push(&mut self, now: f64, speed: f32) {
        self.history.push_back((now, speed));
    }

    /// Takes the speed asked for now, and says whether the body is getting anywhere: nothing
    /// asked is fine; with more than four of each over the last second, the speed made must be at
    /// least a tenth of the speed asked for.
    // D2-15834
    pub fn check_progress(&mut self, now: f64, speed: f32) -> bool {
        if speed < MINIMUM_SPEED {
            return true;
        }
        #[allow(clippy::cast_possible_truncation)]
        let cutoff = f64::from(now as f32) - 1.0;
        while self.history.front().is_some_and(|(t, _)| *t < cutoff) {
            self.history.pop_front();
        }
        while self.desired.front().is_some_and(|(t, _)| *t < cutoff) {
            self.desired.pop_front();
        }
        self.desired.push_back((now, speed));
        if self.history.len() > 4 && self.desired.len() > 4 {
            let made: f32 = self.history.iter().map(|(_, s)| *s).sum();
            let asked: f32 = self.desired.iter().map(|(_, s)| *s).sum();
            #[allow(clippy::cast_precision_loss)]
            let (n, m) = (self.history.len() as f32, self.desired.len() as f32);
            if (asked / m) * 0.1 > made / n {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the steering pieces, held to hand-worked cases)
    use super::*;

    fn at(x: f32, y: f32, heading: f32) -> Position {
        let mut f = Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY);
        geom::set_heading(&mut f, Heading::new(heading));
        Position::new(CellId(0x1010_0001), f)
    }

    #[test]
    fn a_path_walks_only_when_facing_its_destination() {
        let mut p = SimplePath::default();
        p.set_parameters(0.5, 2.0, 1.0, 4.0, 100.0, 1.0);
        p.set_destination(at(50.0, 60.0, 0.0), 0.5, 2.0);
        let (step, motion, to) = p.update(&at(50.0, 40.0, 0.0));
        assert_eq!(step, Step::Continue);
        assert!(to.degrees().abs() < 1e-3);
        assert!((motion.y - 1.0).abs() < 1e-5 && motion.x.abs() < 1e-5);
        let (_, motion, _) = p.update(&at(50.0, 40.0, 180.0));
        assert_eq!(motion, Vec3::ZERO);
        let (step, _, _) = p.update(&at(50.0, 59.0, 0.0));
        assert_eq!(step, Step::Success);
    }

    #[test]
    fn a_turn_is_done_within_ten_degrees() {
        let t = Turner {
            desired: Heading::new(90.0),
        };
        assert_eq!(t.update(Heading::new(81.0)).0, Step::Success);
        assert_eq!(t.update(Heading::new(79.0)).0, Step::Continue);
    }

    #[test]
    fn a_body_making_under_a_tenth_of_its_asked_speed_is_stuck() {
        let mut c = Headway::new(0.0);
        let mut ok = true;
        for i in 0..6 {
            let now = f64::from(i) * 0.1;
            c.push(now, 0.05);
            ok = c.check_progress(now, 1.0);
        }
        assert!(!ok);
        let mut c = Headway::new(0.0);
        for i in 0..6 {
            let now = f64::from(i) * 0.1;
            c.push(now, 0.5);
            ok = c.check_progress(now, 1.0);
        }
        assert!(ok);
    }
}
