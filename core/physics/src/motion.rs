//! The physics/animation seam.
//!
//! `UpdateObjectInternal` calls into the animation layer twice per
//! sub-step, at two different points, and the animation layer calls back on ground transitions;
//! that is a genuinely bidirectional seam and it needs a trait.
//!
//! The physics update needs `hit_ground` / `leave_ground` / `tick_movement` / `process_hooks` /
//! `has_collision_geometry`, and it needs `advance` to return a **`Frame`** because the update
//! scales the animation *origin* by `scale` or by `0.0` and leaves the *rotation* alone — which is
//! impossible to express if only a velocity crosses the seam.

use dereth_primitives::{Frame, LocalTime};

// The trait itself lives in `dereth-primitives` as a shared seam, and the animation crate
// implements it. The test doubles below are this crate's, not part of the contract.
pub use dereth_primitives::MotionSource;

/// The do-nothing implementation: an object with no part array. Returns the identity frame, so a
/// `NullMotion` object coasts on `velocity_vector` alone.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullMotion {
    /// Whether this object is to be treated as having collision geometry. `false` reproduces the
    /// "no part array or no spheres" branch of `UpdateObjectInternal`.
    pub has_geometry: bool,
    /// Ground-transition callback counters, so a test can assert the edges fired.
    pub hit_ground_count: u32,
    /// As above.
    pub leave_ground_count: u32,
}

impl NullMotion {
    #[must_use]
    pub const fn with_geometry() -> Self {
        Self {
            has_geometry: true,
            hit_ground_count: 0,
            leave_ground_count: 0,
        }
    }
}

impl MotionSource for NullMotion {
    fn advance(&mut self, _quantum: f64) -> Frame {
        Frame::default()
    }
    fn tick_movement(&mut self, _now: LocalTime) {}
    fn hit_ground(&mut self) {
        self.hit_ground_count += 1;
    }
    fn leave_ground(&mut self) {
        self.leave_ground_count += 1;
    }
    fn process_hooks(&mut self) {}
    fn has_collision_geometry(&self) -> bool {
        self.has_geometry
    }
}

/// Replays a recorded offset-per-sub-step sequence — exactly what the physics trace fixtures carry
/// in their `.input.json`. Past the end of the sequence it behaves as [`NullMotion`].
#[derive(Debug, Clone)]
pub struct ScriptedMotion {
    offsets: Vec<Frame>,
    cursor: usize,
    has_geometry: bool,
    /// Every `quantum` the world asked for, in order — the cheapest way for a test to assert the
    /// sub-step ladder without instrumenting the world.
    pub quanta: Vec<f64>,
    pub hit_ground_count: u32,
    pub leave_ground_count: u32,
    pub movement_ticks: u32,
    pub hook_flushes: u32,
}

impl ScriptedMotion {
    #[must_use]
    pub fn new(offsets: Vec<Frame>, has_geometry: bool) -> Self {
        Self {
            offsets,
            cursor: 0,
            has_geometry,
            quanta: Vec::new(),
            hit_ground_count: 0,
            leave_ground_count: 0,
            movement_ticks: 0,
            hook_flushes: 0,
        }
    }

    /// How many offsets have been consumed, i.e. how many sub-steps have run.
    #[must_use]
    pub const fn steps(&self) -> usize {
        self.cursor
    }
}

impl MotionSource for ScriptedMotion {
    fn advance(&mut self, quantum: f64) -> Frame {
        self.quanta.push(quantum);
        let f = self.offsets.get(self.cursor).copied().unwrap_or_default();
        self.cursor += 1;
        f
    }
    fn tick_movement(&mut self, _now: LocalTime) {
        self.movement_ticks += 1;
    }
    fn hit_ground(&mut self) {
        self.hit_ground_count += 1;
    }
    fn leave_ground(&mut self) {
        self.leave_ground_count += 1;
    }
    fn process_hooks(&mut self) {
        self.hook_flushes += 1;
    }
    fn has_collision_geometry(&self) -> bool {
        self.has_geometry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::{Quat, Vec3};

    #[test]
    fn the_seam_is_object_safe() {
        fn _assert(_: &mut dyn MotionSource) {}
        let mut n = NullMotion::default();
        _assert(&mut n);
    }

    #[test]
    fn scripted_motion_replays_then_falls_back_to_identity() {
        let a = Frame::new(Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY);
        let mut m = ScriptedMotion::new(vec![a], true);
        assert_eq!(m.advance(0.1), a);
        assert_eq!(m.advance(0.1), Frame::default());
        assert_eq!(m.quanta, vec![0.1, 0.1]);
        assert_eq!(m.steps(), 2);
    }

    #[test]
    fn null_motion_counts_ground_edges() {
        let mut m = NullMotion::default();
        assert!(!m.has_collision_geometry());
        m.hit_ground();
        m.leave_ground();
        m.leave_ground();
        assert_eq!((m.hit_ground_count, m.leave_ground_count), (1, 2));
    }
}
