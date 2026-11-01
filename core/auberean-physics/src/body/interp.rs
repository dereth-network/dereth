//! What a body asks of its motion interpreter beyond each tick's step: leaving the ground, a move
//! interrupted, the constraint on how fast it may move, the motion the server sent for it.

use auberean_assets::animation::MotionParams;
use dereth_primitives::Vec3;

use super::Body;
use crate::geom::Heading;

impl Body {
    /// Runs `f` on the body's motion interpreter, the body as its owner.
    fn with_interp<R>(&mut self, f: impl FnOnce(&mut crate::MotionInterp, &mut Body) -> R) -> R {
        let mut mi = std::mem::take(&mut self.minterp);
        let r = f(&mut mi, self);
        self.minterp = mi;
        r
    }

    /// Tells the motion interpreter the body left the ground.
    // D2-06421
    pub(super) fn leave_ground(&mut self) {
        self.with_interp(|mi, b| mi.leave_ground(b));
    }

    /// One step of the motion interpreter.
    // D2-06486
    pub(super) fn update_motion(&mut self, now: f64, dt: f32) {
        self.with_interp(|mi, b| mi.update_movement(b, now, dt));
    }

    /// A move under way is interrupted: the behaviors a move cancels stop, and the move ends.
    // D2-05902, D2-06402
    pub fn interrupt_move_to(&mut self) {
        self.stop_move_to_cancels_behaviors();
        let now = self.clock;
        self.with_interp(|mi, b| mi.interrupt_move_to(b, now));
    }

    /// How much of its speed the body may use where it is (1 unconstrained).
    // D2-05911, D2-06438
    #[must_use]
    pub fn constraint_factor(&self) -> f32 {
        self.minterp.constraint_factor(&self.position)
    }

    /// The motion the server last sent for the body.
    // D2-05904
    #[must_use]
    pub fn remote_do_motion_values(&self) -> Vec3 {
        self.minterp.remote_motion()
    }

    /// Whether the body follows the positions the server sends.
    // D2-06127
    #[must_use]
    pub fn has_remote_do_motion_values(&self) -> bool {
        self.minterp.is_interpolating()
    }

    /// The motion values of the element and mode the body is going to, else the defaults.
    // D2-06521
    #[must_use]
    pub fn motion_values(&self) -> MotionParams {
        self.minterp.motion_values(self)
    }

    /// Moves at the asked motion `v` (sideways, forward, up, each -1 to 1) and turns to face
    /// `heading`; the body's movement wakes.
    // D2-05891, D2-06388
    pub fn do_motion(&mut self, v: Vec3, heading: Heading) {
        self.set_movement_active(true);
        self.minterp.do_motion(v);
        self.minterp.turn_to(false, heading);
    }
}
