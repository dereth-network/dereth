//! The physics/animation interface, which runs in both directions.
//!
//! The physics update drives an object's animation layer through [`MotionSource`]: twice per
//! sub-step it asks the animation for its frame offset and runs the movement tick, and on a ground
//! transition it tells the animation that the object landed or left the ground. Physics publishes
//! the facts the movement layer reads live in [`MotionPhysicsState`]. Physics holds the animation
//! side as a `Box<dyn MotionSource>`, so neither crate names the other's concrete types, and a
//! collision body with no animation at all implements it with the defaults.

use crate::space::Frame;
use crate::time::LocalTime;

/// Physics-owned facts consumed by a movement manager during a substep.
#[derive(Debug, Clone, Copy)]
pub struct MotionPhysicsState {
    pub object_id: crate::ObjectId,
    pub position: crate::Position,
    /// The integrator's own velocity. The acceleration pass zeroes it for a body in walkable
    /// contact, so a body standing on the ground reports zero here however it got there.
    pub velocity: crate::Vec3,
    /// What a velocity query answers with: the offset the object actually achieved over the last
    /// quantum, divided by that quantum, and zero on every arm that committed no transition.
    /// The movement layer's move-to handler and its interpolated-position query both read this
    /// one and not `velocity`, so it crosses the interface as its own fact.
    pub cached_velocity: crate::Vec3,
    pub radius: f32,
    pub height: f32,
    pub in_cell: bool,
    pub contact: bool,
    pub on_ground: bool,
    pub gravity_affected: bool,
}

/// The bidirectional interface between physics and animation.
///
/// The physics update calls into the animation layer twice per sub-step, at two different points, and
/// the animation layer calls back on ground transitions. Physics is the caller, the animation layer
/// the implementor. The shape follows the client's own call sites: [`advance`](Self::advance)
/// returns a whole [`Frame`] rather than a displacement, because the caller scales the *origin* and
/// never the rotation, and a vector could not express that rule.
// `Debug` is required so a physics object holding a `Box<dyn MotionSource>` can satisfy the
// workspace's missing_debug_implementations lint.
pub trait MotionSource: std::fmt::Debug {
    /// Physics facts read live by the retail movement manager. Refreshed before animation and
    /// after each achieved transition, not merely once at the start of an application frame.
    fn sync_physics_state(&mut self, _state: MotionPhysicsState) {}

    /// Adjust the frame offset, after animation-origin scaling and before frame composition,
    /// physics and collision. An object with no position manager leaves the offset alone.
    fn adjust_position_offset(&mut self, _offset: &mut Frame, _quantum: f64) {}

    /// Advance the animation player by `quantum` seconds and return the
    /// object-local frame offset it contributes: per-frame root motion composed with the motion
    /// table's constant velocity and omega, integrated once per animation frame crossed.
    ///
    /// Returns the identity frame when the object has no part array. **The caller scales the origin**
    /// by the object's scale when on a walkable surface, by zero otherwise — and never touches the
    /// rotation.
    fn advance(&mut self, quantum: f64) -> Frame;

    /// The movement tick followed by the part array's movement handling, run *after* the object
    /// has already been moved for this quantum.
    fn tick_movement(&mut self, now: LocalTime);

    /// Fired when the object becomes seated on a walkable surface.
    fn hit_ground(&mut self);

    /// Fired when the object stops being seated on a walkable surface.
    fn leave_ground(&mut self);

    /// Execute every animation hook queued during [`advance`], in
    /// queue order, then clear the queue.
    ///
    /// [`advance`]: MotionSource::advance
    fn process_hooks(&mut self);

    /// True when the object has no collision geometry at all: no part array, or zero spheres. The
    /// physics update then teleports straight to the new frame with no transition.
    fn has_collision_geometry(&self) -> bool;

    /// The motion interpreter's maximum speed: the adjusted one when `use_adjusted` is set (the
    /// client's own flag for that selection is on), otherwise the plain one. Both are
    /// `run_rate * 4.0`.
    ///
    /// The interpolation pass reads this **every sub-step** and doubles it to get the speed at
    /// which a server correction is walked off; a doubled result under `EPSILON` falls back to
    /// 7.5 instead.
    ///
    /// `None` means the object carries no motion interpreter at all, in which case the client
    /// leaves the speed at 0.0 and takes that same fallback. The default is what every
    /// implementor with no interpreter (a test stub, a raw collision body) answers.
    fn motion_max_speed(&self, _use_adjusted: bool) -> Option<f32> {
        None
    }

    /// True when the object has a movement manager at all **and** that manager says a `MoveTo`
    /// is in flight. The two are checked in that order: with no movement manager the answer is
    /// `false` and the second question is never asked.
    ///
    /// A server correction passes this flag through as `keepHeading`, so a body walking a
    /// `MoveTo` of its own takes the server's *place* without its *facing*. `false` is the
    /// client's own answer for an object with no movement manager.
    fn is_moving_to(&self) -> bool {
        false
    }
}
