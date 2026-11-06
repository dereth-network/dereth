//! Pushing, turning and measuring a body from outside its tick: impulses (in the landblock's frame,
//! the body's own, or away from another object), velocities set outright, its heading set to the
//! nearest 255th of a turn, and its distance to places and objects.

use dereth_physics::V3;
use dereth_primitives::{Position, Vec3};

use super::Body;
use crate::geom::{self, Heading};
use crate::motion::params::MovementParameters;
use crate::motion::AIR;

/// Below this a velocity's square counts as still.
const STILL_SQ: f32 = 0.0002;

impl Body {
    /// An impulse: the body leaves the ground (its other updates woken), goes into the air and takes
    /// the impulse as its velocity. Always takes.
    // D2-06124, D2-06117
    pub fn add_impulse(&mut self, v: Vec3) -> bool {
        self.set_contact_state(false);
        self.set_misc_active(true);
        self.do_element(AIR);
        crate::motion::Owner::set_velocity(self, v);
        true
    }

    /// An impulse in the body's own frame.
    // D2-05880
    pub fn add_impulse_local(&mut self, v: Vec3) -> bool {
        let g = geom::local_to_global(&self.position.frame, v);
        self.add_impulse(g)
    }

    /// An impulse of `speed` away from the object `target` (one the host keeps), plus `up` straight
    /// up. With `again`, the push is divided by the distance once more, so it weakens with
    /// distance. Under 0.0002 m from the object only `up` is given; with no such object nothing is.
    // D2-06125
    pub fn add_targeted_impulse(&mut self, target: u64, speed: f32, again: bool, up: f32) -> bool {
        let Some(t) = self.targets.get(&target).copied() else {
            return false;
        };
        let to = geom::offset(&self.position, &t.position);
        let away = Vec3::new(-to.x, -to.y, -to.z);
        let len2 = away.z * away.z + away.y * away.y + away.x * away.x;
        let len = len2.sqrt();
        let mut v = Vec3::ZERO;
        if len >= 0.0002 {
            let k = 1.0 / len;
            let (dx, dy) = (away.x * k, away.y * k);
            let mut z = k * away.z * speed;
            let (mut x, mut y) = (dx * speed, dy * speed);
            if again {
                let k2 = 1.0 / len2.sqrt();
                x = k2 * dx * speed;
                y = dy * speed * k2;
                z *= k2;
            }
            v = Vec3::new(x, y, z);
        }
        v.z += up;
        self.add_impulse(v)
    }

    /// A move to a place or object: handed to the motion interpreter; when it takes, the body's
    /// movement updates run.
    // D2-06126, D2-06401
    pub fn move_to(&mut self, params: &MovementParameters) -> bool {
        let now = self.clock;
        let mut mi = std::mem::take(&mut self.minterp);
        let took = mi.move_to_event(self, now, params);
        self.minterp = mi;
        if took {
            self.set_movement_active(true);
        }
        true
    }

    /// A move the server orders: only the player's body takes it, keeping the order's stamp and
    /// stopping the behaviors a move cancels.
    // D2-05858
    pub fn receive_move_to(&mut self, params: &MovementParameters, stamp: u16) -> bool {
        if !self.is_player {
            return false;
        }
        self.update_times[super::MOVE_TO_STAMP] = stamp;
        self.stop_move_to_cancels_behaviors();
        let now = self.clock;
        let mut mi = std::mem::take(&mut self.minterp);
        let took = mi.move_to_event(self, now, params);
        self.minterp = mi;
        took
    }

    /// A position the server sent for the body, with the motion it asked for, its heading and
    /// whether it touches the ground. Within 15 m, the body's movement wakes and it is interpolated
    /// toward it; farther (or flying as a missile), it is put there.
    // D2-05909
    pub fn interpolate_to(
        &mut self,
        time: f64,
        p: Position,
        motion: Vec3,
        heading: Heading,
        contact: bool,
    ) {
        if !self.in_world {
            return;
        }
        let d = geom::offset(&self.position, &p);
        if d.mag2() <= 225.0 {
            self.set_movement_active(true);
            if self.missile.is_none() {
                self.minterp
                    .interpolate_to(time, p, motion, heading, contact);
                return;
            }
        }
        self.place(p, heading.degrees());
    }

    /// A position the server sent with an impulse (a jump or a push): the body's movement wakes and
    /// it is interpolated toward it with the impulse; a missile is put there.
    // D2-05908
    pub fn interpolate_and_impulse(
        &mut self,
        time: f64,
        p: Position,
        motion: Vec3,
        heading: Heading,
        velocity: Vec3,
        jump: bool,
    ) {
        if !self.in_world {
            return;
        }
        self.set_movement_active(true);
        if self.missile.is_some() {
            self.place(p, heading.degrees());
            return;
        }
        self.minterp
            .impulse_at(time, p, motion, heading, velocity, jump);
    }

    // D2-05910
    pub fn stop_interpolating(&mut self) {
        self.minterp.stop_interpolating();
    }

    /// Whether ground with normal `n` is walkable for the body: steep up to this world's limit.
    // D2-05973, D2-05822
    #[must_use]
    pub fn is_valid_walkable(&self, n: Vec3) -> bool {
        crate::rules::rules().is_valid_walkable(n)
    }

    /// The least upward share of a walkable ground's normal.
    // D2-05974, D2-05824
    #[must_use]
    pub const fn walkable_z(&self) -> f32 {
        crate::rules::WALKABLE_Z
    }

    /// A missile's velocity: through the velocity setter when asked, else straight in.
    // D2-05983
    pub fn set_missile_velocity(&mut self, v: Vec3, checked: bool) {
        if checked {
            crate::motion::Owner::set_velocity(self, v);
        } else {
            self.velocity = v;
        }
    }

    /// A velocity "in the body's frame": it is set as the landblock-frame velocity as it is, not
    /// turned by the body's heading, and kept as the body's animation velocity.
    pub fn set_local_velocity(&mut self, v: Vec3) {
        crate::motion::Owner::set_velocity(self, v);
        self.anim_velocity = v;
    }

    /// The rate of turn, degrees a second; a body turns only about the vertical, so only `z` is
    /// kept.
    // D2-05984
    pub fn set_omega_velocity(&mut self, w: Vec3) {
        self.turn_rate = w.z;
    }

    /// A rate of turn in the body's frame, turned into the landblock's.
    pub fn set_local_omega_velocity(&mut self, w: Vec3) {
        let g = geom::local_to_global(&self.position.frame, w);
        self.set_omega_velocity(g);
    }

    /// Whether the body moves faster than about 0.014 m/s.
    // D2-05982
    #[must_use]
    pub fn is_moving(&self) -> bool {
        self.velocity.mag2() > STILL_SQ
    }

    /// The heading the body's frame faces.
    // D2-05992, D2-06436, D2-02015
    #[must_use]
    pub fn frame_heading(&self) -> Heading {
        geom::frame_heading(&self.position.frame)
    }

    /// The heading from the body to a place.
    #[must_use]
    pub fn calc_heading(&self, to: &Position) -> Heading {
        geom::heading_to(&self.position, to)
    }

    /// Faces the body at a heading made to a 255th of a turn below it (as headings are sent).
    // D2-06145
    pub fn set_heading(&mut self, h: Heading) {
        let h = Heading::from_u8(h.to_u8());
        geom::set_heading(&mut self.position.frame, h);
        self.heading = h.degrees();
    }

    /// How high the body steps up: its setup's, through its part group when it has one.
    // D2-05870, D2-06460
    #[must_use]
    pub fn step_up_height(&self) -> f32 {
        self.parts
            .as_deref()
            .map_or(self.config.step_up, |p| p.array.step_up_height())
    }

    /// How far the body steps down: its setup's step-down height (through its part group when it
    /// has one), but never more than its height less a centimeter.
    // D2-06118
    #[must_use]
    pub fn step_down_height(&self) -> f32 {
        let own = self
            .parts
            .as_deref()
            .map_or(self.config.step_down, |p| p.array.step_down_height());
        let cap = crate::motion::Owner::height(self) - 0.01;
        if own <= cap {
            own
        } else {
            cap
        }
    }

    /// The body's step-down height becomes `h`: through its part group, its setup's, for every
    /// object made from that setup.
    // D2-05871, D2-06464
    pub fn set_step_down_height(&mut self, h: f32) {
        match self.parts.as_deref_mut() {
            Some(p) => {
                p.array.set_step_down_height(h);
            }
            None => self.config.step_down = h,
        }
    }

    /// The distance between the body's cylinder and another's (its radius and height at `p`).
    #[must_use]
    pub fn distance_to_object(&self, p: &Position, radius: f32, height: f32) -> f32 {
        geom::cylinder_distance(
            crate::motion::Owner::radius(self),
            crate::motion::Owner::height(self),
            &self.position,
            radius,
            height,
            p,
        )
    }
}
