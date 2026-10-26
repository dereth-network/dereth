//! Walking a route with the body's own movement: the check that turns "the planner says so" into
//! "the body gets there".
//!
//! [`PhysicsWalker`] steps a body through a [`PhysicsWorld`] exactly as the world steps any moving
//! object: a per-frame offset handed to the object's motion, gravity, the collision transition with
//! its step up, step down, slide and walkable-slope rules, at the world's 30 Hz. Nothing here
//! decides whether a move is allowed; the world does.

use std::sync::Arc;

use dereth_physics::source::SetupGeometry;
use dereth_physics::{PhysHandle, PhysicsWorld, ScriptedMotion};
use dereth_primitives::{Frame, LocalTime, ObjectId, Position, Quat, Vec3};

use crate::path::global;
use crate::vmath::{dist, dist_xy};

/// What one walk did.
#[derive(Debug, Clone, PartialEq)]
pub struct WalkOutcome {
    /// Whether the body reached the target.
    pub reached: bool,
    /// Where it ended.
    pub at: Position,
    /// How far it moved.
    pub distance: f32,
    /// How long it took, in seconds of world time.
    pub seconds: f64,
}

/// Something that can move a body: put it somewhere, and walk it straight at a target.
pub trait SegmentCheck {
    /// Put the body at `at`. `false` when it cannot stand there.
    fn place(&mut self, at: &Position) -> bool;
    /// Walk the body from where it is straight toward `to`, until it arrives or stops making
    /// progress.
    fn walk(&mut self, to: &Position) -> WalkOutcome;
}

/// What checking a route did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CheckReport {
    /// Routes planned (one, plus one per repair).
    pub plans: usize,
    /// Legs walked, refinements included.
    pub legs: usize,
    /// Legs the body could not walk straight.
    pub failed_legs: usize,
    /// Of those, legs walked again through their places.
    pub refined_legs: usize,
    /// Places taken out where even that failed.
    pub repaired_places: usize,
    /// Whether the final route was walked end to end.
    pub walked: bool,
    /// Distance walked, in metres, over every attempt.
    pub distance: f32,
    /// World time spent walking, in seconds, over every attempt.
    pub seconds: f64,
}

/// One world frame: the world steps at 30 Hz; a hair over a thirtieth of a second lands every call
/// past its gate.
const FRAME: f64 = 1.0 / 29.5;

/// The settings of a [`PhysicsWalker`].
#[derive(Debug, Clone, PartialEq)]
pub struct WalkerConfig {
    /// Walking speed, metres per second.
    pub speed: f32,
    /// How close, across, counts as arrived.
    pub arrive: f32,
    /// How close, up or down, counts as arrived.
    pub arrive_z: f32,
    /// Seconds without getting closer that count as stuck.
    pub stuck_after: f64,
}

impl Default for WalkerConfig {
    fn default() -> Self {
        Self {
            speed: 4.0,
            arrive: 0.2,
            arrive_z: 0.9,
            stuck_after: 1.0,
        }
    }
}

/// A body in a physics world, walked by handing its motion one offset per frame.
#[derive(Debug)]
pub struct PhysicsWalker<'w> {
    world: &'w mut PhysicsWorld,
    handle: PhysHandle,
    now: f64,
    config: WalkerConfig,
    /// Every place the body stood at the end of a frame, for a trace.
    pub trail: Vec<Position>,
    /// Whether to keep [`Self::trail`].
    pub keep_trail: bool,
}

impl<'w> PhysicsWalker<'w> {
    /// A new body of `setup` in `world`, made the world's player so it is always active.
    pub fn new(
        world: &'w mut PhysicsWorld,
        setup: Arc<SetupGeometry>,
        config: WalkerConfig,
    ) -> Self {
        let handle = world.create(ObjectId(0x5000_0001), setup, true);
        world.set_player(handle);
        Self {
            world,
            handle,
            now: 0.0,
            config,
            trail: Vec::new(),
            keep_trail: false,
        }
    }

    /// The body.
    #[must_use]
    pub const fn handle(&self) -> PhysHandle {
        self.handle
    }

    /// Where the body is.
    #[must_use]
    pub fn position(&self) -> Option<Position> {
        self.world.get(self.handle).map(|o| o.position)
    }

    /// One world frame with the body moving `offset`.
    fn frame(&mut self, offset: Vec3) {
        if let Some(o) = self.world.get_mut(self.handle) {
            o.set_motion(Box::new(ScriptedMotion::new(
                vec![Frame::new(offset, Quat::IDENTITY); 2],
                true,
            )));
            o.transient_state.set_active_bit(true);
        }
        self.now += FRAME;
        self.world.use_time(LocalTime(self.now), false);
        if self.keep_trail {
            if let Some(p) = self.position() {
                self.trail.push(p);
            }
        }
    }
}

impl Drop for PhysicsWalker<'_> {
    /// The body leaves the world with the walker, so it is not in the way of the next one.
    fn drop(&mut self) {
        self.world.destroy(self.handle);
    }
}

impl SegmentCheck for PhysicsWalker<'_> {
    fn place(&mut self, at: &Position) -> bool {
        let mut pos = *at;
        pos.frame.rotation = Quat::IDENTITY;
        if !self.world.set_position(self.handle, &pos) {
            return false;
        }
        if let Some(o) = self.world.get_mut(self.handle) {
            o.velocity_vector = Vec3::ZERO;
            o.transient_state.set_active_bit(true);
            o.calc_acceleration();
            o.update_time = self.now;
        }
        // Let it settle onto the floor.
        for _ in 0..8 {
            self.frame(Vec3::ZERO);
        }
        self.position()
            .is_some_and(|p| dist(global(&p), global(at)) < 1.0)
    }

    fn walk(&mut self, to: &Position) -> WalkOutcome {
        let start = self.position().unwrap_or(*to);
        let begun = self.now;
        let mut moved = 0.0;
        let mut last = start;
        let mut best = dist_xy(global(&start), global(to));
        let mut best_at = self.now;
        #[allow(clippy::cast_possible_truncation)]
        let step = (f64::from(self.config.speed) * FRAME) as f32;
        let limit = f64::from(best / self.config.speed) * 3.0 + 3.0;
        loop {
            let here = self.position().unwrap_or(last);
            let gap = dist_xy(global(&here), global(to));
            if gap <= self.config.arrive
                && (here.frame.origin.z - to.frame.origin.z).abs() <= self.config.arrive_z
            {
                return WalkOutcome {
                    reached: true,
                    at: here,
                    distance: moved,
                    seconds: self.now - begun,
                };
            }
            if gap + 0.02 < best {
                best = gap;
                best_at = self.now;
            }
            if self.now - best_at > self.config.stuck_after || self.now - begun > limit {
                return WalkOutcome {
                    reached: false,
                    at: here,
                    distance: moved,
                    seconds: self.now - begun,
                };
            }
            let dx = global(to).x - global(&here).x;
            let dy = global(to).y - global(&here).y;
            let s = step.min(gap) / gap.max(1e-6);
            self.frame(Vec3::new(dx * s, dy * s, 0.0));
            let now = self.position().unwrap_or(here);
            moved += dist(global(&now), global(&here));
            last = now;
        }
    }
}
