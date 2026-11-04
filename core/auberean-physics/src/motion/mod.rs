//! The motion interpreter: from what a body is asked to do (forward, sideways, vertical, each -1 to
//! 1, and a turn) to the accelerations that take its velocity and rate of turn there, by the
//! body's motion values (its speeds, accelerations and slowdowns in its element and mode, on its
//! kind of ground).
//!
//! Asking is per frame; accelerating is per physics tick ([`MotionInterp::update_movement`]). The
//! asked motion comes from the player, or from a move to a place or object the server ordered
//! (the player's body only), or from the positions the server sends for anyone else (the
//! interpolation manager). Forward speeds up by the forward acceleration and brakes by the backward
//! slowdown; backward the other way round; sideways likewise. An acceleration that would pass the
//! wanted speed (or the cap) is cut to land on it in one tick. Turning accelerates toward the
//! wanted rate of turn and slows by its own slowdown. A body that hovers rather than falls is held
//! at its hover height. The interpreter also starts and stops the behaviors that show moving,
//! falling, sliding and skidding.

pub mod constraint;
pub mod interp;
pub mod moveto;
pub mod params;
pub mod values;

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::animation::{ModeKey, MotionDesc, MotionParams};
use dereth_primitives::num::math;
use dereth_primitives::{Position, Vec3};

use crate::geom::{Heading, EPS};
use constraint::Tether;
use interp::Follower;
use moveto::{MoveToHost, Mover, Steer, TargetBody};
use params::{MovementParameters, MovementReturn};
use values::{default_values, find_in_description};

/// The element of a body on land.
pub const EARTH: u32 = 0x4000_0001;
/// The element of a body in the air.
pub const AIR: u32 = 0x4000_0002;
/// The element of a body in water.
pub const WATER: u32 = 0x4000_0003;
/// The element of a body on the sea floor.
pub const SEA_FLOOR: u32 = 0x4000_0004;
/// The everyday mode, in which motion values are looked up when a mode has none of its own.
pub const PEACE: u32 = 0x4000_0001;
/// The kind of ground a body starts on.
pub const DEFAULT_TERRAIN: u32 = 0x8000_0001;
/// Ten hundredths of a second, as the client holds it.
const A_TENTH: f64 = 0.100_000_001_490_116_12;

/// The behaviors and effects the interpreter starts and stops.
pub mod shows {
    /// Moving or turning.
    pub const MOVING: u32 = 0x4000_0001;
    /// Falling a long way.
    pub const LONG_FALL: u32 = 0x4000_0009;
    /// About to land.
    pub const LANDING: u32 = 0x4000_000B;
    /// Skidding to a stop after a long run.
    pub const SKID: u32 = 0x4000_000C;
    /// Sliding down a slope.
    pub const SLIDE: u32 = 0x4000_000D;
    /// The effect of landing.
    pub const LAND_EFFECT: u32 = 0x4000_0002;
    /// The effect of leaving the ground.
    pub const JUMP_EFFECT: u32 = 0x4000_0003;
    /// The dust of a skid or a slide.
    pub const SKID_EFFECT: u32 = 0x4000_0004;
}

/// What a body is asked this frame, the player's way: sideways (right positive), forward (ahead
/// positive), each -1 to 1, and a turn (left positive).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct MotionInput {
    pub sideways: f32,
    pub forward: f32,
    pub turn: f32,
}

/// What the motion interpreter (and the managers under it) ask of the body: the interpreter is
/// handed its body each call rather than holding it.
// D2-06984, D2-06986
pub trait Owner {
    /// The body's velocity in its own frame.
    fn local_velocity(&self) -> Vec3;
    /// The body's velocity in the landblock's frame.
    fn velocity(&self) -> Vec3;
    /// The body's rate of turn, degrees a second (left positive about `z`).
    fn omega(&self) -> Vec3;
    /// Where the body faces, in the landblock's frame.
    fn facing(&self) -> Vec3;
    fn heading(&self) -> Heading;
    fn position(&self) -> Position;
    fn radius(&self) -> f32;
    fn height(&self) -> f32;
    fn element(&self) -> u32;
    fn desired_element(&self) -> u32;
    /// The element and mode the body is going to.
    // D2-06993
    fn desired_mode_state(&self) -> ModeKey;
    /// The body's own scale along its axes (its physics object's, which its qualities give, not
    /// its look's): the scale of its speeds.
    // D2-06471
    fn effects_scale(&self) -> Vec3;
    fn height_above_walkable(&self) -> f32;
    fn depth(&self) -> f32;
    fn has_water(&self) -> bool;
    fn is_sliding_down(&self) -> bool;
    fn is_player(&self) -> bool;
    fn has_contact(&self) -> bool;
    fn is_doing_behavior(&self, behavior: u32) -> bool;
    fn do_autonomous_behavior(&mut self, behavior: u32);
    fn stop_behavior(&mut self, behavior: u32);
    fn do_mode(&mut self, element: u32, mode: u32);
    fn do_fx(&mut self, fx: u32, intensity: f32);
    fn clamp_heading(&mut self, h: Heading);
    fn unclamp_heading(&mut self);
    fn clamp_omega(&mut self, w: f32);
    fn unclamp_omega(&mut self);
    /// A target object at the top of its containment, if it is in the world.
    fn target(&self, id: u64) -> Option<TargetBody>;
    fn set_target(&mut self, id: u64, radius: f32, quantum: f32);
    fn clear_target(&mut self);
    /// The move the server ordered ended.
    // D2-06997
    fn move_to_done(&mut self, result: MovementReturn);
    /// Faces the body at once and stops it turning.
    fn snap_heading(&mut self, h: Heading);
    /// Frees the body of the limits interpolation puts on its motion.
    fn release_rails(&mut self);
    /// Limits how far the body may move across the ground this tick (below zero: no limit).
    fn set_distance_cap(&mut self, cap: f32);
    /// Keeps the body's motion across the ground along a direction.
    fn set_velocity_rail(&mut self, dir: Vec3);
    /// Has the body follow the transition's result exactly, without its own step logic.
    fn set_synchronize_transition(&mut self, on: bool);
    fn set_contact(&mut self, on: bool);
    /// Gives the body a velocity in the landblock's frame.
    fn set_velocity(&mut self, v: Vec3);
    /// Gives the body an impulse at the place it is pushed from; whether it took.
    fn apply_impulse(&mut self, v: Vec3) -> bool;
    /// Puts the body at a place at once.
    fn blip_to(&mut self, p: Position);
    /// Jumps if it can, with the velocity it was given; whether it jumped.
    fn jump_logic(&mut self) -> bool;
}

/// A body's motion interpreter.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionInterp {
    /// The motion description's id, and the description once read.
    pub desc_id: u32,
    pub desc: Option<Arc<MotionDesc>>,
    /// Whether the body shows its motion with behaviors.
    pub can_animate: bool,
    /// This tick's acceleration, in the body's frame.
    pub acceleration: Vec3,
    /// This tick's turning acceleration, degrees a second squared.
    pub omega_acceleration: Vec3,
    /// The rate of turn asked for this tick.
    pub desired_omega: Vec3,
    /// The velocity asked for, in the body's frame.
    pub local_desired_velocity: Vec3,
    /// The motion asked for (sideways, forward, vertical), kept until asked again.
    pub asked: Vec3,
    pub instant_turn_to: bool,
    /// The fastest the body may turn now.
    pub max_turn: f32,
    pub time_running: f64,
    pub time_since_skid: f64,
    pub time_since_jump: f64,
    pub time_since_land: f64,
    /// The heading a turn-to is going to.
    pub turn_to: Heading,
    pub turning_to: bool,
    /// The server's speed and acceleration scales.
    pub velocity_scale: f32,
    pub acceleration_scale: f32,
    /// The hover height the server set; -1 for the motion values'.
    pub hover_height: f32,
    pub terrain_type: u32,
    pub terrain_mods: HashMap<u32, u32>,
    pub global_mod: u32,
    pub move_to: Option<Mover>,
    pub interpolation: Option<Follower>,
    // D2-07036
    pub constraint: Option<Tether>,
}

impl Default for MotionInterp {
    // D2-07041, D2-07043, D2-15434
    fn default() -> Self {
        Self {
            desc_id: 0,
            desc: None,
            can_animate: true,
            acceleration: Vec3::ZERO,
            omega_acceleration: Vec3::ZERO,
            desired_omega: Vec3::ZERO,
            local_desired_velocity: Vec3::ZERO,
            asked: Vec3::ZERO,
            instant_turn_to: false,
            max_turn: 0.0,
            time_running: 0.0,
            time_since_skid: 0.0,
            time_since_jump: 0.0,
            time_since_land: 0.0,
            turn_to: Heading::ZERO,
            turning_to: false,
            velocity_scale: 1.0,
            acceleration_scale: 1.0,
            hover_height: -1.0,
            terrain_type: DEFAULT_TERRAIN,
            terrain_mods: HashMap::new(),
            global_mod: 0,
            move_to: None,
            interpolation: None,
            constraint: None,
        }
    }
}

/// The motion interpreter and its body, seen from a move the server ordered.
struct MoveHost<'a> {
    mi: &'a mut MotionInterp,
    owner: &'a mut dyn Owner,
}

impl MoveToHost for MoveHost<'_> {
    fn position(&self) -> Position {
        self.owner.position()
    }
    fn heading(&self) -> Heading {
        self.owner.heading()
    }
    fn radius(&self) -> f32 {
        self.owner.radius()
    }
    fn height(&self) -> f32 {
        self.owner.height()
    }
    fn target(&self, id: u64) -> Option<TargetBody> {
        self.owner.target(id)
    }
    fn set_target(&mut self, id: u64, radius: f32, quantum: f32) {
        self.owner.set_target(id, radius, quantum);
    }
    fn clear_target(&mut self) {
        self.owner.clear_target();
    }
    fn affected_by_gravity(&self) -> bool {
        self.mi.is_affected_by_gravity(&*self.owner)
    }
    fn default_hover_height(&self) -> f32 {
        self.mi.default_hover_height(&*self.owner)
    }
    fn local_desired(&self, motion: Vec3) -> Vec3 {
        self.mi.local_desired(&*self.owner, motion, false, false)
    }
    fn do_motion(&mut self, motion: Vec3) {
        self.mi.do_motion(motion);
    }
    fn stop_turn_to(&mut self) {
        self.mi.stop_turn_to(&*self.owner);
    }
    fn move_to_done(&mut self, result: MovementReturn) {
        self.owner.move_to_done(result);
    }
    fn snap_heading(&mut self, h: Heading) {
        self.owner.snap_heading(h);
    }
    fn release_rails(&mut self) {
        self.owner.release_rails();
    }
}

impl MotionInterp {
    /// A motion interpreter reading a motion description.
    #[must_use]
    pub fn with_description(desc_id: u32, desc: Arc<MotionDesc>) -> Self {
        Self {
            desc_id,
            desc: Some(desc),
            ..Self::default()
        }
    }

    /// Starts on a body: facing where it faces, with a move-to manager.
    pub fn init(&mut self, owner: &dyn Owner) {
        self.turn_to = owner.heading();
        self.move_to.get_or_insert_with(Mover::default);
    }

    /// A new motion description.
    // D2-06982
    pub fn set_description(&mut self, desc_id: u32, desc: Arc<MotionDesc>) {
        self.desc_id = desc_id;
        self.desc = Some(desc);
    }

    // D2-06985
    pub fn set_terrain_type(&mut self, terrain: u32) {
        self.terrain_type = terrain;
    }

    // D2-07035
    #[must_use]
    pub const fn terrain_type(&self) -> u32 {
        self.terrain_type
    }

    /// The sea floor's slowing; none without a description.
    // D2-06987
    #[must_use]
    pub fn sea_floor_multiplier(&self) -> f32 {
        self.desc.as_ref().map_or(0.0, |d| d.sea_floor_multiplier)
    }

    /// Water's slowing; none without a description.
    // D2-06988
    #[must_use]
    pub fn water_multiplier(&self) -> f32 {
        self.desc.as_ref().map_or(0.0, |d| d.water_multiplier)
    }

    // D2-06989
    #[must_use]
    pub fn is_moving_to(&self) -> bool {
        self.move_to.as_ref().is_some_and(Mover::is_moving_to)
    }

    // D2-06990
    #[must_use]
    pub const fn is_turning_to(&self) -> bool {
        self.turning_to
    }

    // D2-06991
    #[must_use]
    pub fn reached_persistent_target(&self) -> bool {
        self.move_to
            .as_ref()
            .is_some_and(Mover::reached_persistent_target)
    }

    /// The velocity asking for `motion` gives, in the body's frame: forward at the forward speed
    /// (or the backward speed backing up); sideways at the sideways speed, or at the forward speed
    /// once forward passes 0.7; each times the server's speed scale, the body's own scale along
    /// that axis (not its look's: a character drawn shorter runs as fast), and (when
    /// `constrained`) how much motion its constraint leaves it. Vertically, at the vertical speed
    /// when `vertical`, else as forward.
    // D2-06992
    #[must_use]
    pub fn local_desired(
        &self,
        owner: &dyn Owner,
        motion: Vec3,
        constrained: bool,
        vertical: bool,
    ) -> Vec3 {
        let m = self.motion_values(owner);
        let s = owner.effects_scale();
        let k = if constrained {
            f64::from(self.constraint_factor(&owner.position()))
        } else {
            1.0
        };
        let vs = f64::from(self.velocity_scale);
        let fwd = k * f64::from(m.max_forward);
        let back = k * f64::from(m.max_backward);
        #[allow(clippy::cast_possible_truncation)]
        let strafe = f64::from((k * f64::from(m.max_strafe)) as f32);
        let base_y = vs * f64::from(motion.y) * f64::from(s.y);
        let y = if motion.y > 0.0 {
            base_y * fwd
        } else {
            base_y * back
        };
        let base_x = f64::from(motion.x) * vs * f64::from(s.x);
        let x = if motion.y > 0.7 {
            base_x * fwd
        } else {
            base_x * strafe
        };
        let z = if vertical {
            f64::from(m.max_vertical) * f64::from(motion.z)
        } else {
            f64::from(motion.z) * f64::from(m.max_forward) * k
        };
        #[allow(clippy::cast_possible_truncation)]
        Vec3::new(x as f32, y as f32, z as f32)
    }

    // D2-06994
    pub fn update_target_info(
        &mut self,
        position: Position,
        radius: f32,
        height: f32,
        status: moveto::TargetStatus,
    ) {
        if let Some(m) = self.move_to.as_mut() {
            m.update_target_info(position, radius, height, status);
        }
    }

    /// A move the server ordered.
    // D2-06995
    pub fn move_to_event(
        &mut self,
        owner: &mut dyn Owner,
        now: f64,
        params: &MovementParameters,
    ) -> bool {
        let Some(mut m) = self.move_to.take() else {
            return false;
        };
        let ok = m.move_to_event(&mut MoveHost { mi: self, owner }, now, params, false);
        self.move_to = Some(m);
        ok
    }

    // D2-06996
    pub fn interrupt_move_to(&mut self, owner: &mut dyn Owner, now: f64) {
        if let Some(mut m) = self.move_to.take() {
            m.interrupt(&mut MoveHost { mi: self, owner }, now);
            self.move_to = Some(m);
        }
    }

    /// Landing: the landing effect, as hard as the fall was fast (a fall at 6.5 m/s is full),
    /// no more than once a tenth of a second.
    // D2-06998
    pub fn hit_ground(&mut self, owner: &mut dyn Owner) {
        let v = owner.local_velocity();
        let mut f = (v.z * 0.153_846_16).abs();
        if f < 0.2 || f.is_nan() {
            f = 0.2;
        } else if f > 1.0 {
            f = 1.0;
        }
        if self.time_since_land > A_TENTH {
            owner.do_fx(shows::LAND_EFFECT, f);
            self.time_since_land = 0.0;
        }
    }

    /// Leaving the ground: the jump effect, as strong as the jump (6.28 m/s is full), no more than
    /// once a tenth of a second.
    // D2-06999
    pub fn leave_ground(&mut self, owner: &mut dyn Owner) {
        let v = owner.local_velocity();
        if self.time_since_jump > A_TENTH {
            owner.do_fx(shows::JUMP_EFFECT, (v.z * 0.159_235_66).abs());
            self.time_since_jump = 0.0;
        }
    }

    /// The forward speed of the current motion values.
    // D2-07000
    #[must_use]
    pub fn max_speed(&self, owner: &dyn Owner) -> f32 {
        self.motion_values(owner).max_forward
    }

    // D2-07002
    #[must_use]
    pub fn is_affected_by_gravity(&self, owner: &dyn Owner) -> bool {
        self.motion_values(owner).gravity != 0
    }

    // D2-07003
    pub fn reset_hover_height(&mut self) {
        self.hover_height = -1.0;
    }

    /// The hover height the server set, else the motion values'.
    // D2-07004
    #[must_use]
    pub fn get_hover_height(&self, owner: &dyn Owner) -> f32 {
        if self.hover_height == -1.0 {
            self.motion_values(owner).hover_height
        } else {
            self.hover_height
        }
    }

    // D2-07005
    #[must_use]
    pub fn default_hover_height(&self, owner: &dyn Owner) -> f32 {
        self.motion_values(owner).hover_height
    }

    /// Turns to a heading, made to a 255th of a turn; `instant` turns at once.
    // D2-07006
    pub fn turn_to(&mut self, instant: bool, h: Heading) {
        self.instant_turn_to = instant;
        self.turn_to = quantize(h);
        self.turning_to = true;
    }

    // D2-07007
    pub fn stop_turn_to(&mut self, owner: &dyn Owner) {
        self.turn_to = owner.heading();
        self.turning_to = false;
        self.instant_turn_to = false;
    }

    // D2-07008
    pub fn stop_interpolating(&mut self) {
        if let Some(i) = self.interpolation.as_mut() {
            i.stop_interpolating();
        }
    }

    // D2-07009
    #[must_use]
    pub fn is_interpolating(&self) -> bool {
        self.interpolation.as_ref().is_some_and(|i| i.active)
    }

    /// The motion the last position the server sent asked for.
    // D2-07010
    #[must_use]
    pub fn remote_motion(&self) -> Vec3 {
        self.interpolation
            .as_ref()
            .map_or(Vec3::ZERO, Follower::last_received_motion)
    }

    // D2-07011
    pub fn make_interpolation_manager(&mut self) {
        self.interpolation.get_or_insert_with(Follower::default);
    }

    /// A constraint: the player's starts slowing at 19 m and stops at 20, anyone else's at 2 and
    /// 4.
    // D2-07012
    pub fn make_constraint_manager(&mut self, id: u64, is_player: bool) {
        if self.constraint.is_none() {
            let mut c = Tether::new(id);
            c.is_player = is_player;
            if is_player {
                c.set_distances(19.0, 20.0);
            } else {
                c.set_distances(2.0, 4.0);
            }
            self.constraint = Some(c);
        }
    }

    /// Leaving the world: the move ends, the turn stops, interpolation stops, the tether lets go.
    // D2-07013
    pub fn handle_leave_world(&mut self, owner: &mut dyn Owner, now: f64) {
        self.interrupt_move_to(owner, now);
        self.stop_turn_to(&*owner);
        self.stop_interpolating();
        self.unconstrain();
    }

    // D2-07014
    pub fn constrain_to(&mut self, id: u64, is_player: bool, p: Position) {
        self.make_constraint_manager(id, is_player);
        if let Some(c) = self.constraint.as_mut() {
            c.constrain_to(p);
        }
    }

    // D2-07015
    pub fn unconstrain(&mut self) {
        if let Some(c) = self.constraint.as_mut() {
            c.release();
        }
    }

    // D2-07016
    #[must_use]
    pub fn constraint_factor(&self, p: &Position) -> f32 {
        self.constraint.as_ref().map_or(1.0, |c| c.factor(p))
    }

    /// Drops everything the body was asked and every manager.
    // D2-07018, D2-07034
    pub fn destroy(&mut self) {
        if let Some(m) = self.move_to.as_mut() {
            m.destroy();
        }
        self.local_desired_velocity = Vec3::ZERO;
        self.acceleration = Vec3::ZERO;
        self.omega_acceleration = Vec3::ZERO;
        self.velocity_scale = 1.0;
        self.acceleration_scale = 1.0;
        self.hover_height = -1.0;
        self.desc_id = 0;
        self.move_to = None;
        self.interpolation = None;
        self.constraint = None;
        self.desc = None;
    }

    /// Asks for a turn this tick: left positive, a full turn the fastest rate of turn allowed now.
    // D2-07019
    pub fn set_omega(&mut self, turn: f32) {
        let t = turn * self.max_turn + self.desired_omega.z;
        let m = self.max_turn;
        let lo = if t < m { t } else { m };
        self.desired_omega.z = if -m < lo { lo } else { -m };
    }

    /// The turning acceleration toward the rate asked for, or the slowdown toward none; a turn-to
    /// sets the rate toward its heading, and ends within a thousandth of a degree when the body has
    /// stopped turning. The rate asked for is then spent.
    // D2-07020, D2-05993, D2-06435
    pub fn calculate_omega_acceleration(&mut self, owner: &mut dyn Owner) {
        let w = owner.omega();
        let m = self.motion_values(&*owner);
        if self.turning_to {
            self.desired_omega = Vec3::ZERO;
            let f = owner.facing();
            let (x, y) = (f64::from(f.x), f64::from(f.y));
            let len = (x * x + y * y).sqrt();
            let facing = if len < f64::from(EPS) {
                Heading::ZERO
            } else {
                let inv = 1.0 / len;
                #[allow(clippy::cast_possible_truncation)]
                let xs = f64::from((x * inv) as f32);
                #[allow(clippy::cast_possible_truncation)]
                Heading::from_radians((7.853_981_633_974_483 - math::atan2(inv * y, xs)) as f32)
            };
            let d = facing.minus(self.turn_to).degrees_centered();
            if d.abs() >= 0.001 || w.z.abs() >= EPS {
                let s = if d > 0.001 {
                    1.0
                } else if d < -0.001 {
                    -1.0
                } else {
                    0.0
                };
                self.desired_omega.z += s * self.max_turn;
            } else {
                self.turning_to = false;
                owner.unclamp_heading();
                owner.unclamp_omega();
            }
        }
        let d = self.desired_omega.z;
        if d.abs() >= EPS {
            if w.z < d {
                self.omega_acceleration.z = m.turn_acceleration;
            } else if w.z > d {
                self.omega_acceleration.z = -m.turn_acceleration;
            }
        } else if w.z < 0.0 {
            self.omega_acceleration.z = m.turn_slowdown;
        } else if w.z > 0.0 {
            self.omega_acceleration.z = -m.turn_slowdown;
        }
        if !self.turning_to || (self.omega_acceleration.z == 0.0 && w.z == 0.0) {
            owner.unclamp_heading();
        } else {
            owner.clamp_heading(self.turn_to);
        }
        owner.clamp_omega(self.desired_omega.z);
        self.desired_omega = Vec3::ZERO;
    }

    /// Asks for motion: anything not a number asks for none, and a request longer than 1 is cut
    /// to length 1, so diagonals are not faster.
    // D2-07021
    pub fn do_motion(&mut self, v: Vec3) {
        let mut v = if v.x.is_nan() || v.y.is_nan() || v.z.is_nan() {
            Vec3::ZERO
        } else {
            v
        };
        let n2 = v.x * v.x + v.y * v.y + v.z * v.z;
        if n2 > 1.0 {
            let n = n2.sqrt();
            v = Vec3::new(v.x / n, v.y / n, v.z / n);
        }
        self.asked = v;
    }

    /// Caps a velocity in the body's frame at the current speeds: forward (sideways too, first),
    /// sideways at the sideways speed, or at the forward speed once forward passes 0.7 of it, and
    /// vertical at the vertical speed. Only from above.
    // D2-07022
    pub fn clamp_velocity(&self, owner: &dyn Owner, v: &mut Vec3) {
        let m = self.motion_values(owner);
        let s = owner.effects_scale();
        let vs = self.velocity_scale;
        let f = m.max_forward;
        let cap = s.y * vs * f;
        v.x = if cap < v.x { cap } else { v.x };
        let cap = if v.y > 0.0 {
            s.y * vs * f
        } else {
            s.y * vs * m.max_backward
        };
        v.y = if cap < v.y { cap } else { v.y };
        let ratio = if cap.abs() >= EPS { v.y / cap } else { 0.0 };
        let cap = if ratio > 0.7 {
            s.x * vs * f
        } else {
            s.x * vs * m.max_strafe
        };
        v.x = if cap < v.x { cap } else { v.x };
        v.z = if m.max_vertical < v.z {
            m.max_vertical
        } else {
            v.z
        };
    }

    /// A position the server sent for this body, to interpolate toward.
    // D2-07023
    pub fn interpolate_to(
        &mut self,
        time: f64,
        p: Position,
        motion: Vec3,
        heading: Heading,
        contact: bool,
    ) {
        self.make_interpolation_manager();
        if let Some(i) = self.interpolation.as_mut() {
            i.add_event(time, p, motion, heading, contact);
        }
    }

    /// A position the server sent with an impulse (a jump or a push), to interpolate toward.
    // D2-07024
    pub fn impulse_at(
        &mut self,
        time: f64,
        p: Position,
        motion: Vec3,
        heading: Heading,
        velocity: Vec3,
        jump: bool,
    ) {
        self.make_interpolation_manager();
        if let Some(i) = self.interpolation.as_mut() {
            i.add_impulse_event(time, p, motion, heading, velocity, jump);
        }
    }

    /// One tick: the asked motion from interpolation or from a move under way, the behaviors that
    /// show the body's motion, then the accelerations toward the asked velocity and the turning
    /// acceleration.
    // D2-07025, D2-06327, D2-06430
    #[allow(clippy::too_many_lines)]
    pub fn update_movement(&mut self, owner: &mut dyn Owner, now: f64, dt: f32) {
        self.time_since_jump += f64::from(dt);
        self.time_since_land += f64::from(dt);
        let mut constrained = true;
        if let Some(mut i) = self.interpolation.take() {
            if i.active {
                let mut steer = Steer {
                    motion: self.asked,
                    heading: self.turn_to,
                    instant: self.instant_turn_to,
                };
                i.perform_interpolation(self, owner, now, dt, &mut steer);
                self.asked = steer.motion;
                self.turn_to = steer.heading;
                self.instant_turn_to = steer.instant;
                constrained = false;
                self.turning_to = true;
            }
            self.interpolation = Some(i);
        }
        if owner.is_player() && self.is_moving_to() {
            if let Some(mut m) = self.move_to.take() {
                let mut steer = Steer {
                    motion: self.asked,
                    heading: self.turn_to,
                    instant: self.instant_turn_to,
                };
                m.update(&mut MoveHost { mi: self, owner }, now, dt, &mut steer);
                self.move_to = Some(m);
                self.asked = steer.motion;
                self.turn_to = steer.heading;
                self.instant_turn_to = steer.instant;
                constrained = false;
                self.turning_to = true;
            }
        }
        let a = &mut self.asked;
        if a.z * a.z + a.y * a.y + a.x * a.x > 1.0 {
            a.x = a.x.clamp(-1.0, 1.0);
            a.y = a.y.clamp(-1.0, 1.0);
            a.z = a.z.clamp(-1.0, 1.0);
        }
        self.turn_to = quantize(self.turn_to);
        self.local_desired_velocity = self.local_desired(&*owner, self.asked, constrained, true);
        let m = self.motion_values(&*owner);
        self.acceleration = Vec3::ZERO;
        self.omega_acceleration = Vec3::ZERO;
        let v = owner.local_velocity();
        let w = owner.omega();
        let desired_element = owner.desired_element();
        let element = owner.element();
        let speed2 = v.z * v.z + v.x * v.x + v.y * v.y;
        let h = owner.height_above_walkable();
        if self.can_animate {
            self.show_motion(owner, dt, v, w, speed2, h, desired_element);
        }
        let s = owner.effects_scale();
        let vs = self.velocity_scale;
        let acl = self.acceleration_scale;
        let cap_x = m.max_strafe * vs * s.x;
        let d = self.local_desired_velocity;
        // The fastest turn: the still rate standing, rising to the running rate at full speed.
        self.max_turn = if m.max_forward.abs() <= EPS {
            m.still_turn_rate
        } else {
            let f = (v.y * v.y + v.x * v.x + v.z * v.z).sqrt() / m.max_forward;
            if f <= 1.0 {
                f * m.running_turn_rate + (1.0 - f) * m.still_turn_rate
            } else {
                m.running_turn_rate
            }
        };
        let (mut strafe_acl, mut strafe_sb) =
            (m.strafe_acceleration * acl, m.strafe_slowdown * acl);
        if s.x > 1.0 {
            strafe_acl *= s.x;
            strafe_sb *= s.x;
        }
        let (mut fwd_acl, mut fwd_sb) = (m.forward_acceleration * acl, m.forward_slowdown * acl);
        let (mut back_acl, mut back_sb) =
            (m.backward_acceleration * acl, m.backward_slowdown * acl);
        if s.y > 1.0 {
            fwd_acl *= s.y;
            fwd_sb *= s.y;
            back_acl *= s.y;
            back_sb *= s.y;
        }
        // Sideways.
        if v.x < d.x {
            self.acceleration.x = if d.x == 0.0 || (v.x < 0.0 && d.x > 0.0) {
                strafe_sb
            } else {
                strafe_acl
            };
            let next = dt * self.acceleration.x + v.x;
            if cap_x < next || d.x < next {
                self.acceleration.x = (d.x - v.x) / dt;
            }
        }
        if d.x < v.x {
            self.acceleration.x = -(if d.x == 0.0 || (v.x > 0.0 && d.x < 0.0) {
                strafe_sb
            } else {
                strafe_acl
            });
            let next = dt * self.acceleration.x + v.x;
            if next < -cap_x || next < d.x {
                self.acceleration.x = (d.x - v.x) / dt;
            }
        }
        // Forward and back.
        if v.y < d.y {
            self.acceleration.y = if d.y == 0.0 || (v.y < 0.0 && d.y > 0.0) {
                fwd_sb
            } else {
                fwd_acl
            };
            let next = dt * self.acceleration.y + v.y;
            if vs * m.max_forward * s.y < next || d.y < next {
                self.acceleration.y = (d.y - v.y) / dt;
            }
        }
        if d.y < v.y {
            self.acceleration.y = -(if d.y == 0.0 || (v.y > 0.0 && d.y < 0.0) {
                back_sb
            } else {
                back_acl
            });
            let next = dt * self.acceleration.y + v.y;
            if next < -(m.max_backward * vs * s.y) || next < d.y {
                self.acceleration.y = (d.y - v.y) / dt;
            }
        }
        // Vertical.
        if m.gravity == 0 {
            // Hovering: toward the hover height, stopping on it.
            let mut h = h;
            if owner.has_water() {
                let under = -owner.depth();
                if under < h {
                    h = under;
                }
            }
            let hv = if self.hover_height.to_bits() == (-1.0f32).to_bits() {
                m.hover_height
            } else {
                self.hover_height
            };
            let mut az = m.vertical_acceleration * acl;
            if hv <= h {
                az = -az;
            }
            self.acceleration.z = az;
            let next = (self.acceleration.z + m.lift) * dt + v.z;
            let lo = -(m.max_vertical * vs);
            if next < lo {
                self.acceleration.z = (lo - v.z) / dt;
            }
            let hi = m.max_vertical * vs;
            if hi < next {
                self.acceleration.z = (hi - v.z) / dt;
            }
            let dv = (self.acceleration.z + m.lift) * dt;
            let ahead = (dv * 0.5 + v.z + dv) * dt + h;
            if ((h < hv && hv < ahead) || (hv < h && ahead < hv)) && hv - h != 0.0 {
                let gap = hv - h;
                self.acceleration.z = -((v.z * v.z) / (gap + gap));
                let cap = m.vertical_acceleration * acl;
                if cap < self.acceleration.z {
                    self.acceleration.z = cap;
                }
                if self.acceleration.z < -cap {
                    self.acceleration.z = -cap;
                }
            }
        } else if h == 0.0 && element != AIR && desired_element != AIR && v.z != 0.0 {
            // On the ground under gravity: the vertical speed is taken out, gently while moving.
            self.acceleration.z = -(v.z / dt);
            if v.x.abs() > 0.1 || v.y.abs() > 0.1 {
                if -(v.z / dt) > 3.0 {
                    self.acceleration.z = 3.0;
                }
                if self.acceleration.z < -3.0 {
                    self.acceleration.z = -3.0;
                }
            }
        }
        self.calculate_omega_acceleration(owner);
    }

    /// The behaviors and effects that show how the body moves: moving or turning; about to land,
    /// or falling far; diving into water below; sliding down a slope (with dust); skidding to a
    /// stop after running more than six seconds (with dust).
    #[allow(clippy::too_many_arguments)]
    fn show_motion(
        &mut self,
        owner: &mut dyn Owner,
        dt: f32,
        v: Vec3,
        w: Vec3,
        speed2: f32,
        h: f32,
        desired_element: u32,
    ) {
        if speed2 > EPS || w.z * w.z + w.y * w.y + w.x * w.x > 0.0 {
            owner.do_autonomous_behavior(shows::MOVING);
        } else if owner.is_doing_behavior(shows::MOVING) {
            owner.stop_behavior(shows::MOVING);
        }
        if h != 0.0 {
            let depth = -owner.depth();
            let water = owner.has_water();
            if v.z < -2.0 {
                if f64::from(h) < 1.3 {
                    owner.do_autonomous_behavior(shows::LANDING);
                } else if depth > 2.0 {
                    if depth < h && water {
                        let mode = owner.desired_mode_state().mode;
                        if mode != WATER && self.is_affected_by_gravity(&*owner) {
                            owner.do_mode(0, AIR);
                        }
                    } else if h > 50.0 || (h > 10.0 && v.z < -19.0) {
                        owner.do_autonomous_behavior(shows::LONG_FALL);
                    }
                }
            }
        }
        let mut sliding = false;
        if owner.is_sliding_down() && f64::from(speed2) > 2.0 && desired_element == EARTH {
            sliding = true;
            if self.time_since_skid > 1.0 || self.time_since_skid == 0.0 {
                owner.do_fx(shows::SKID_EFFECT, speed2);
                self.time_since_skid = 0.0;
            }
            self.time_since_skid += f64::from(dt);
            owner.do_autonomous_behavior(shows::SLIDE);
        } else if owner.is_doing_behavior(shows::SLIDE) {
            owner.stop_behavior(shows::SLIDE);
        }
        if f64::from(speed2) > 2.0 {
            self.time_running += f64::from(dt);
        } else {
            self.time_running = 0.0;
        }
        if f64::from(v.y) > 1.5
            && self.local_desired_velocity.y <= 0.0
            && 6.0 < self.time_running
            && h == 0.0
        {
            owner.do_autonomous_behavior(shows::SKID);
            if self.time_since_skid > 1.0 || (self.time_since_skid == 0.0 && v.y > 2.5) {
                #[allow(clippy::cast_possible_truncation)]
                owner.do_fx(
                    shows::SKID_EFFECT,
                    (f64::from(v.y) * f64::from(0.181_818_19_f32) - f64::from(0.2_f32)) as f32,
                );
                self.time_since_skid = 0.0;
            }
            self.time_since_skid += f64::from(dt);
        } else {
            if owner.is_doing_behavior(shows::SKID) {
                owner.stop_behavior(shows::SKID);
            }
            if !sliding {
                self.time_since_skid = 0.0;
            }
        }
    }

    /// The motion values for an element and mode on the current ground: none without a
    /// description (or none it gives).
    // D2-07028
    #[must_use]
    pub fn find_motion_values(&self, key: ModeKey) -> Option<MotionParams> {
        let desc = self.desc.as_ref()?;
        let terrain_mod = self.terrain_mods.get(&self.terrain_type).copied();
        find_in_description(desc, key, self.global_mod, self.terrain_type, terrain_mod).copied()
    }

    /// The motion values of the element and mode the body is going to, else the defaults.
    // D2-07030, D2-07029
    #[must_use]
    pub fn motion_values(&self, owner: &dyn Owner) -> MotionParams {
        self.find_motion_values(owner.desired_mode_state())
            .unwrap_or_else(default_values)
    }

    /// The motion values of an element and mode, else of the element in the everyday mode, else
    /// the defaults.
    // D2-07031
    #[must_use]
    pub fn desired_motion_values(&self, key: ModeKey) -> MotionParams {
        self.find_motion_values(key)
            .or_else(|| {
                self.find_motion_values(ModeKey {
                    element: key.element,
                    mode: PEACE,
                })
            })
            .unwrap_or_else(default_values)
    }

    // D2-07032
    pub fn clear_terrain_modifiers(&mut self) {
        self.terrain_mods.clear();
    }

    /// A modifier on one kind of ground.
    // D2-07033
    pub fn add_terrain_modifier(&mut self, terrain: u32, modifier: u32) {
        self.terrain_mods.insert(terrain, modifier);
    }

    // D2-07038
    #[must_use]
    pub const fn get_acceleration(&self) -> Vec3 {
        self.acceleration
    }

    // D2-07040
    #[must_use]
    pub const fn get_acceleration_scale(&self) -> f32 {
        self.acceleration_scale
    }

    /// Which of the interpreter's state differs from a fresh one, as saved.
    // D2-07026
    #[must_use]
    pub fn pack_bits(&self) -> u32 {
        let big = |v: Vec3| v.x.abs() >= EPS || v.y.abs() >= EPS || v.z.abs() >= EPS;
        let mut b = 0;
        if big(self.acceleration) {
            b |= 1;
        }
        if big(self.omega_acceleration) {
            b |= 2;
        }
        if big(self.local_desired_velocity) {
            b |= 4;
        }
        if (self.velocity_scale - 1.0).abs() >= EPS {
            b |= 0x20;
        }
        if (self.acceleration_scale - 1.0).abs() >= EPS {
            b |= 0x40;
        }
        if (self.hover_height - -1.0).abs() >= EPS {
            b |= 0x80;
        }
        if self.interpolation.is_some() {
            b |= 0x100;
        }
        if big(self.asked) {
            b |= 0x200;
        }
        if self.global_mod != 0 {
            b |= 0x400;
        }
        if !self.terrain_mods.is_empty() {
            b |= 0x800;
        }
        if self.terrain_type != DEFAULT_TERRAIN {
            b |= 0x1000;
        }
        b
    }

    /// Asks for movement and a turn the player's way.
    pub fn steer(&mut self, input: MotionInput) {
        self.do_motion(Vec3::new(input.sideways, input.forward, 0.0));
        self.set_omega(input.turn);
    }
}

/// A heading made to a 255th of a turn below it, as the client keeps turn-to headings.
// D2-13300, D2-13321
#[must_use]
pub fn quantize(h: Heading) -> Heading {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = (f64::from(h.degrees()) * f64::from(0.002_777_777_8_f32) * 255.0) as i64 as u8;
    Heading::from_f64(f64::from(b) * f64::from(1.0_f32 / 255.0) * 360.0)
}
