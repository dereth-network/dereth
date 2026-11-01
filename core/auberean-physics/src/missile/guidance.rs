//! A missile's guidance: placing it, launching it toward its target, steering it every reaction
//! interval toward the velocity it wants within its turning and speed limits, burning its fuel,
//! moving it each step under its acceleration, gravity, drag and wind, and deciding when a
//! collision sets it off.

use auberean_assets::Context;
use auberean_dat::reader::Result;
use dereth_primitives::num::math;
use dereth_primitives::{Frame, Position, Quat, Vec3};

use super::{aim, ground, set, MissileParameters, MissileTargetingParameters};
use crate::collision::CollisionReport;
use crate::geom::{self, Heading, EPS, EPS_SQ};
use crate::pack::{read_vec3, write_vec3, Pack, Reader, Writer};
use crate::target::{Aim, NO_OBJECT};

/// What stopped a step of the guidance.
pub mod fail {
    pub const NONE: u32 = 0;
    /// There is no target to aim at.
    pub const NO_TARGET: u32 = 1;
    /// The missile could not be placed.
    pub const NO_PLACE: u32 = 2;
    /// No course reaches the target, or the target is past the missile's range.
    pub const NO_COURSE: u32 = 3;
    /// Its settings could not be taken.
    pub const BAD_SETTINGS: u32 = 4;
    /// Nothing has been heard of where the target is.
    pub const TARGET_UNHEARD: u32 = 6;
}

/// The effect a flying missile plays.
pub const FLIGHT_FX: u32 = 0x4000_0001;

/// How far a missile turns its target's course, seconds: the steps a span of flight times is cut
/// into, the shortest step, and the narrowest span searched.
const STEPS: u32 = 5;
const MIN_STEP: f32 = 0.1;
const TIME_LIMIT: f32 = 0.001;

/// The world as one missile's guidance sees it: the missile's body, the objects it aims at and
/// strikes, and the clock.
pub trait GuidanceHost {
    /// The clock, seconds.
    fn now(&self) -> f64;
    fn position(&self) -> Position;
    /// Where the body is drawn: where it is held, while it has a holder.
    fn drawing_position(&self) -> Position;
    fn has_parent(&self) -> bool;
    fn is_in_world(&self) -> bool;
    /// Whether a place is in the world: a cell of the map and a frame of numbers.
    fn is_valid(&self, p: &Position) -> bool;
    fn gravity(&self) -> Vec3;
    fn wind(&self) -> Vec3;
    /// How high the body is over the walkable ground under it, when there is any.
    fn height_above_walkable(&self) -> Option<f32>;
    /// The missile's velocity, set outright.
    fn set_missile_velocity(&mut self, v: Vec3);
    /// Turns the body to face along its path as it flies.
    fn align_to_path(&mut self, on: bool);
    fn report_out_of_fuel(&mut self);
    fn report_expired(&mut self);
    /// Puts the body in the world at its starting place: false when it cannot be.
    fn init_end(&mut self, at: &Position) -> bool;
    /// The object the body tracks.
    fn target_id(&self) -> u64;
    /// Tracks an object (see the tracking of bodies).
    fn set_target(&mut self, context: u32, object: u64, radius: f32, quantum: f32);
    fn start_streak(&mut self);
    fn stop_streak(&mut self);
    fn do_fx(&mut self, fx: u32, intensity: f32);
    fn stop_fx(&mut self, fx: u32);
    /// Fades the body out and removes it; a missile that goes off asks with 0.5 and 0.
    fn delete_self(&mut self, fade: f64, delay: f64);
    /// Sticks the body to an object it struck.
    fn stick_to(&mut self, object: u64);
    /// True with the given chance.
    fn random_check(&mut self, chance: f32) -> bool;

    fn object_exists(&self, object: u64) -> bool;
    fn object_position(&self, object: u64) -> Option<Position>;
    fn object_heading(&self, object: u64) -> Option<Heading>;
    /// A place on an object: `height` of the way up it (its height scaled), `offset` from there
    /// (by its scale when `scaled`), turned with it; none when the place cannot be had.
    fn object_offset_position(
        &self,
        object: u64,
        height: f32,
        scaled: bool,
        offset: Vec3,
    ) -> Option<Position>;
    fn object_velocity(&self, object: u64) -> Vec3;
    fn object_is_dynamic(&self, object: u64) -> bool;
    /// Whether an object is itself a guided missile.
    fn object_has_guidance(&self, object: u64) -> bool;
    /// Whether an object's game object is a player or a creature.
    fn object_is_player_or_creature(&self, object: u64) -> bool;
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn len_sq(v: Vec3) -> f32 {
    v.x * v.x + v.y * v.y + v.z * v.z
}

fn scale(v: Vec3, k: f32) -> Vec3 {
    Vec3::new(v.x * k, v.y * k, v.z * k)
}

/// A velocity of `speed` at `heading` degrees and `pitch` degrees above level.
fn spherical(speed: f32, heading: f32, pitch: f32) -> Vec3 {
    let k = f64::from(0.017_453_292_f32);
    let (h, p) = (f64::from(heading) * k, f64::from(pitch) * k);
    let (s, cp) = (f64::from(speed), math::cos(p));
    #[allow(clippy::cast_possible_truncation)]
    Vec3::new(
        (math::sin(h) * cp * s) as f32,
        (math::cos(h) * cp * s) as f32,
        (math::sin(p) * s) as f32,
    )
}

/// The heading toward `to` as seen from `from`, turned with it: north is straight ahead.
// D2-07152
fn local_heading(from: &Position, to: &Position) -> Heading {
    let b = geom::block_offset(from.cell, to.cell);
    let o = to.frame.origin;
    let l = geom::point_to_local(&from.frame, Vec3::new(b.x + o.x, b.y + o.y, b.z + o.z));
    geom::heading_of(l.x, l.y)
}

/// Where a position faces: its frame's forward axis.
// D2-07151
fn forward(p: &Position) -> Vec3 {
    let m = geom::rotation_axes(p.frame.rotation);
    Vec3::new(
        (m[6] + m[0]) * 0.0 + m[3],
        (m[7] + m[1]) * 0.0 + m[4],
        (m[8] + m[2]) * 0.0 + m[5],
    )
}

/// The shortest time to close `distance` on one axis at `speed` against a target moving `along`
/// it, kept in `t` when longer than what is there; false when the missile cannot close it.
// D2-07141
fn min_time(distance: f32, along: f32, t: &mut f32, speed: f32) -> bool {
    let k = if distance > 0.0 {
        if speed <= along {
            return false;
        }
        distance / (speed - along)
    } else if distance < 0.0 {
        if -speed >= along {
            return false;
        }
        distance / (-speed - along)
    } else {
        return true;
    };
    if k > *t {
        *t = k;
    }
    true
}

/// The longest time a missile launched at `speed` under `gravity` on one axis takes to meet a
/// target `distance` off moving `along` it: the later root, kept in `t`; false when there is
/// none or it is past.
// D2-07142
fn max_time(distance: f32, along: f32, gravity: f32, t: &mut f32, speed: f32) -> bool {
    let s = if gravity > 0.0 { -speed } else { speed };
    let e = s - along;
    let d = e * e + (distance * gravity + distance * gravity);
    if d < 0.0 {
        return false;
    }
    let r = d.sqrt();
    let inv = 1.0 / gravity;
    let late = (r - e) * inv;
    let early = (-e - r) * inv;
    *t = if late < early { early } else { late };
    *t >= 0.0
}

/// The launch velocity that meets a target `offset` away moving at `velocity` after `t` seconds
/// under `gravity`; none for no time.
// D2-07143
fn gravitational_velocity(offset: Vec3, velocity: Vec3, gravity: Vec3, t: f32) -> Option<Vec3> {
    if t <= 0.0 {
        return None;
    }
    let half = Vec3::new(
        t * gravity.x * 0.5,
        t * gravity.y * 0.5,
        t * gravity.z * 0.5,
    );
    let inv = 1.0 / t;
    Some(Vec3::new(
        (inv * offset.x + velocity.x) - half.x,
        (inv * offset.y + velocity.y) - half.y,
        (inv * offset.z + velocity.z) - half.z,
    ))
}

/// The shortest and longest flight times to a target: the shortest from the axes gravity does
/// not pull along (at least 0.0002 s), the longest from the first axis it does (z, y, x), or
/// twice the shortest. False when an axis cannot be closed or the longest is shorter.
// D2-07158
fn min_max_time(
    offset: Vec3,
    velocity: Vec3,
    gravity: Vec3,
    speed: f32,
    tmin: &mut f32,
    tmax: &mut f32,
) -> bool {
    *tmin = EPS;
    let mut ok = true;
    let axes = [
        (offset.z, velocity.z, gravity.z),
        (offset.y, velocity.y, gravity.y),
        (offset.x, velocity.x, gravity.x),
    ];
    for (o, v, g) in axes {
        if g.abs() < EPS {
            ok &= min_time(o, v, tmin, speed);
        }
    }
    *tmax = *tmin + *tmin;
    if let Some((o, v, g)) = axes.into_iter().find(|(_, _, g)| g.abs() > EPS) {
        ok &= max_time(o, v, g, tmax, speed);
    }
    if *tmax < *tmin {
        return false;
    }
    ok
}

/// Whether a launch velocity is pitched above `sine`. Only a velocity of no length can be: the
/// client compares its upward part without dividing by its length.
// D2-07160
fn greater_pitch(v: Vec3, sine: f32) -> bool {
    if len_sq(v).sqrt() >= EPS {
        return false;
    }
    v.z > sine
}

/// Heading toward a target `offset` away moving at `velocity`, at `speed`: halfway between the
/// way to the target and the way it moves. No offset gives the target's velocity; a target that
/// does not move gives the way to it at unit length (the speed is not applied).
// D2-07161
fn chase_velocity(offset: Vec3, velocity: Vec3, speed: f32) -> Vec3 {
    let d = len_sq(offset).sqrt();
    if d < EPS {
        return velocity;
    }
    let n = scale(offset, 1.0 / d);
    let vl = len_sq(velocity).sqrt();
    if vl < EPS {
        return n;
    }
    let u = scale(velocity, 1.0 / vl);
    let s = Vec3::new(n.x + u.x, n.y + u.y, n.z + u.z);
    let l = len_sq(s).sqrt();
    let s = if l == 0.0 {
        Vec3::ZERO
    } else {
        scale(s, 1.0 / l)
    };
    scale(s, speed)
}

/// A missile's guidance.
// D2-07153, D2-07154
#[derive(Clone, Debug, PartialEq)]
pub struct Guidance {
    pub params: MissileParameters,
    pub targeting: MissileTargetingParameters,
    /// Where the target is, how it moves, and whether that has been heard.
    pub target: Aim,
    /// 1 once a target is known.
    pub target_status: u32,
    pub accel_offset: Vec3,
    pub desired_velocity: Vec3,
    pub last_update: f64,
    pub fuel: f32,
    pub fuel_reported: bool,
    pub expire_time: f64,
    pub expire_reported: bool,
    /// How high it last was over the walkable ground.
    pub height: f32,
    pub acceleration: Vec3,
    // D2-07166
    pub velocity: Vec3,
    pub steps: u32,
    pub min_step: f32,
    pub time_limit: f32,
    pub can_land: bool,
    pub activated: bool,
    pub ready_to_place: bool,
    pub ready_to_fire: bool,
    pub moving: bool,
}

impl Default for Guidance {
    fn default() -> Self {
        Self {
            params: MissileParameters::default(),
            targeting: MissileTargetingParameters::default(),
            target: Aim::default(),
            target_status: 0,
            accel_offset: Vec3::ZERO,
            desired_velocity: Vec3::ZERO,
            last_update: 0.0,
            fuel: 0.0,
            fuel_reported: false,
            expire_time: 0.0,
            expire_reported: false,
            height: 0.0,
            acceleration: Vec3::ZERO,
            velocity: Vec3::ZERO,
            steps: STEPS,
            min_step: MIN_STEP,
            time_limit: TIME_LIMIT,
            can_land: false,
            activated: true,
            ready_to_place: false,
            ready_to_fire: false,
            moving: false,
        }
    }
}

impl Guidance {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the missile goes off near `object`: it detonates by proximity, the chance its
    /// aim drew is within its own, and the object is its target.
    // D2-05836
    #[must_use]
    pub fn detonates_on_proximity_to(&self, object: u64) -> bool {
        self.params.is_set(set::DETONATES_ON_PROXIMITY)
            && self.targeting.detonation_chance <= self.params.detonation_chance
            && object == self.targeting.target
    }

    // D2-07137
    pub fn place(&mut self) {
        self.ready_to_place = true;
    }

    // D2-07138
    pub fn fire(&mut self) {
        self.ready_to_fire = true;
    }

    // D2-07155
    pub fn place_and_fire(&mut self) {
        self.ready_to_place = true;
        self.ready_to_fire = true;
    }

    // D2-07148
    pub fn set_activated(&mut self, on: bool) {
        self.activated = on;
    }

    /// Starts or stops flying: the flight effect and the streak go with it.
    // D2-07149
    pub fn set_moving(&mut self, host: &mut dyn GuidanceHost, on: bool) {
        self.moving = on;
        if on {
            host.start_streak();
            host.do_fx(FLIGHT_FX, 1.0);
        } else {
            host.stop_fx(FLIGHT_FX);
            host.stop_streak();
        }
    }

    // D2-07140
    #[must_use]
    pub fn is_in_world(host: &dyn GuidanceHost) -> bool {
        host.is_in_world()
    }

    /// The place it starts from when it has no source object.
    // D2-07171
    #[must_use]
    pub fn start_position(&self) -> Position {
        self.targeting.base_position
    }

    fn start_lifespan_and_fuel(&mut self, now: f64, p: &MissileParameters) {
        if p.has_lifespan {
            self.expire_reported = false;
            self.expire_time = f64::from(self.params.lifespan) + now;
        }
        if p.uses_fuel {
            self.fuel_reported = false;
            self.fuel = self.params.total_fuel;
        }
    }

    /// Takes new flight settings; its lifespan and fuel start again from now when they have them.
    // D2-07139
    pub fn client_init_missile_parameters(
        &mut self,
        host: &dyn GuidanceHost,
        p: &MissileParameters,
    ) -> u32 {
        if !p.copy_into(&mut self.params) {
            return fail::BAD_SETTINGS;
        }
        self.start_lifespan_and_fuel(host.now(), p);
        fail::NONE
    }

    /// Takes new aim and finds its target.
    // D2-07186
    pub fn client_init_targeting_parameters(
        &mut self,
        host: &mut dyn GuidanceHost,
        t: &MissileTargetingParameters,
    ) -> u32 {
        self.targeting = *t;
        self.setup_target(host)
    }

    /// Takes flight settings and aim and finds its target.
    // D2-07184
    pub fn initialize(
        &mut self,
        host: &mut dyn GuidanceHost,
        p: &MissileParameters,
        t: &MissileTargetingParameters,
    ) -> u32 {
        let a = p.copy_into(&mut self.params);
        let b = t.copy_into(&mut self.targeting);
        if a && b {
            return self.setup_target(host);
        }
        fail::BAD_SETTINGS
    }

    /// Takes new settings and aim in flight: the velocity it wants is the one it has, it stops
    /// accelerating, and its lifespan and fuel start again when the new settings have them.
    // D2-07185
    pub fn retarget(
        &mut self,
        host: &mut dyn GuidanceHost,
        p: &MissileParameters,
        t: &MissileTargetingParameters,
    ) -> u32 {
        self.desired_velocity = self.velocity;
        self.acceleration = Vec3::ZERO;
        let a = p.copy_into(&mut self.params);
        let b = t.copy_into(&mut self.targeting);
        if !(a && b) {
            return fail::BAD_SETTINGS;
        }
        let status = self.setup_target(host);
        if status == fail::NONE {
            self.start_lifespan_and_fuel(host.now(), p);
        }
        status
    }

    /// Aims at the target object, or else at the place it aims at already (the client resets the
    /// aim to its own place, not to the aim's target place), then looks for it.
    // D2-07183
    pub fn setup_target(&mut self, host: &mut dyn GuidanceHost) -> u32 {
        if self.targeting.has(aim::TARGET_OBJECT) {
            self.target.object = self.targeting.target;
            self.target.valid = false;
        } else {
            let p = self.target.position;
            self.target.reset(p);
        }
        self.target_status = 0;
        self.target_update(host)
    }

    /// Looks for the target: an object found is aimed at where on it the aim says, moving as it
    /// moves, and the body tracks it. A bare place is known at once when it is in a cell.
    // D2-07178
    pub fn target_update(&mut self, host: &mut dyn GuidanceHost) -> u32 {
        let id = self.target.object;
        if id == NO_OBJECT {
            if self.target.position.cell.0 != 0 {
                self.target_status = 1;
                return fail::NONE;
            }
            return fail::NO_TARGET;
        }
        if host.object_exists(id) {
            let t = &self.targeting;
            let p =
                host.object_offset_position(id, t.target_height, t.target_scaled, t.target_offset);
            let v = host.object_velocity(id);
            if let Some(p) = p {
                self.target.set_target_info(p, v);
                self.target_status = 1;
            }
        }
        if host.target_id() != id {
            host.set_target(0, id, 1.0, 0.0);
        }
        fail::NONE
    }

    /// Takes where its target is and flies at `v`, from now.
    // D2-07179
    pub fn client_activate(&mut self, host: &mut dyn GuidanceHost, v: Vec3, target: &Aim) {
        self.target = *target;
        self.velocity = v;
        host.set_missile_velocity(v);
        self.desired_velocity = v;
        self.last_update = host.now();
        self.moving = true;
        host.start_streak();
        host.do_fx(FLIGHT_FX, 1.0);
    }

    /// Takes new flight settings and where its target is, then steps; true when the settings
    /// were taken.
    // D2-07194
    pub fn client_retarget(
        &mut self,
        host: &mut dyn GuidanceHost,
        p: &MissileParameters,
        target: &Aim,
    ) -> bool {
        let ok = p.copy_into(&mut self.params);
        self.target = *target;
        self.last_update = host.now();
        self.update(host);
        ok
    }

    /// Burns fuel for `dt` seconds at the rate of its acceleration, down to none.
    // D2-07145
    pub fn update_fuel_use(&mut self, dt: f64) {
        let a = len_sq(self.acceleration).sqrt();
        #[allow(clippy::cast_possible_truncation)]
        let f = self.fuel - a * dt as f32;
        self.fuel = if f < 0.0 { 0.0 } else { f };
    }

    // D2-07146
    #[must_use]
    pub fn out_of_fuel(&self) -> bool {
        self.params.uses_fuel && self.fuel == 0.0
    }

    // D2-07147
    pub fn report_out_of_fuel(&mut self, host: &mut dyn GuidanceHost) {
        host.report_out_of_fuel();
        self.fuel_reported = true;
    }

    /// Keeps a velocity's speed between the missile's least and greatest; a velocity of no length
    /// becomes straight north at the limit. The greatest is tested against the speed before the
    /// least was applied.
    // D2-07144
    pub fn adjust_speed(&self, v: &mut Vec3) {
        let (max, min) = (self.params.max_speed, self.params.min_speed);
        let s2 = len_sq(*v);
        if s2 < min * min {
            if EPS_SQ > s2 {
                *v = Vec3::new(0.0, min, 0.0);
            } else {
                *v = scale(*v, min / s2.sqrt());
            }
        }
        if max * max < s2 {
            if EPS_SQ > s2 {
                *v = Vec3::new(0.0, max, 0.0);
                return;
            }
            *v = scale(*v, max / len_sq(*v).sqrt());
        }
    }

    /// Keeps to the ground: holds the climb to its height over the ground, or climbs at least to it.
    // D2-07162
    pub fn adjust_height(&self, v: &mut Vec3) {
        let h = self.params.height_above_ground - self.height;
        match self.params.ground_follow {
            ground::HOLD => v.z = h,
            ground::AT_LEAST if h > v.z => v.z = h,
            _ => {}
        }
    }

    /// The side a missile circling its target turns to: the other way (method 0), as given (1),
    /// none when it has no least speed (2), else the way it is flying, or at random at launch.
    // D2-07176
    pub fn perp_adjust(&self, host: &mut dyn GuidanceHost, v: &mut Vec3, launch: bool) {
        let flip = match self.params.turn_method {
            0 => true,
            1 => false,
            2 if self.params.min_speed == 0.0 => {
                *v = scale(*v, 0.0);
                false
            }
            _ if launch => host.random_check(0.5),
            _ => dot(self.velocity, *v) < 0.0,
        };
        if flip {
            *v = scale(*v, -1.0);
        }
    }

    /// The speed it wants: its starting speed at launch when it has one, else its top speed;
    /// conserving fuel, the target's speed toward it and its catch-up speed (at least its least
    /// speed), or what reaches the target before its lifespan is out (less a second), when more.
    // D2-07163
    #[must_use]
    pub fn desired_speed(&self, now: f64, offset: Vec3, velocity: Vec3, launch: bool) -> f32 {
        let (max, min) = (self.params.max_speed, self.params.min_speed);
        if launch {
            if self.targeting.has(aim::INITIAL_SPEED) {
                return self.targeting.initial_speed;
            }
            return max;
        }
        if !self.params.conserve_fuel {
            return max;
        }
        let d = len_sq(offset).sqrt();
        let inv = 1.0 / d;
        let closing =
            inv * offset.x * velocity.x + inv * offset.y * velocity.y + inv * offset.z * velocity.z;
        let catch_up = self.params.min_catchup_speed + closing;
        let mut s = if catch_up > min { catch_up } else { min };
        if self.params.consider_remaining_time {
            let left = self.expire_time - now - 1.0;
            if left > f64::from(EPS) {
                #[allow(clippy::cast_possible_truncation)]
                let q = (f64::from(d) / left + f64::from(closing)) as f32;
                if q > s {
                    s = q;
                }
            }
        }
        if s <= max {
            s
        } else {
            max
        }
    }

    /// The velocity that meets the target (`offset` from the target to the missile, the target
    /// moving at `velocity`) at the wanted speed: the target's velocity less the offset over the
    /// sooner positive meeting time. With no such time the client works out a chase velocity and
    /// drops it, leaving `out` as it was; then the height and speed rules apply.
    // D2-07164
    pub fn collide_velocity(
        &self,
        now: f64,
        offset: Vec3,
        velocity: Vec3,
        launch: bool,
        out: &mut Vec3,
        keep_height: bool,
    ) {
        let s = self.desired_speed(now, offset, velocity, launch);
        let a = f64::from(s * s - len_sq(velocity));
        let b = f64::from(dot(velocity, offset) * 2.0);
        let c = f64::from(len_sq(offset));
        let d = b * b - (-c) * a * 4.0;
        let (mut t1, mut t2) = (-1.0, -1.0);
        if d >= 0.0 {
            let r = d.sqrt();
            let inv = 1.0 / (a + a);
            t1 = (r - b) * inv;
            t2 = (-b - r) * inv;
        }
        #[allow(clippy::cast_possible_truncation)]
        let t = if t2 >= 0.0 && (t1 < 0.0 || t2 < t1) {
            t2 as f32
        } else {
            t1 as f32
        };
        if t > EPS {
            let inv = 1.0 / t;
            *out = Vec3::new(
                velocity.x - inv * offset.x,
                velocity.y - inv * offset.y,
                velocity.z - inv * offset.z,
            );
        } else {
            let _dropped = chase_velocity(offset, velocity, s);
        }
        if !keep_height {
            self.adjust_height(out);
        }
        self.adjust_speed(out);
    }

    /// The velocity that keeps the missile its distance from the target (`offset` from the
    /// target to the missile): circling it sideways across the ground, closing or opening the gap,
    /// moving with the target, and climbing by the height it is above it.
    // D2-07182
    pub fn maint_dist_velocity(
        &self,
        host: &mut dyn GuidanceHost,
        offset: Vec3,
        velocity: Vec3,
        launch: bool,
        out: &mut Vec3,
        keep_height: bool,
    ) {
        let keep = self.params.maintain_distance;
        let across = (offset.x * offset.x + offset.y * offset.y).sqrt();
        let (x, y) = if across >= EPS {
            let inv = 1.0 / across;
            let (ux, uy) = (offset.x * inv, offset.y * inv);
            let mut side = Vec3::new(-uy, ux, inv * 0.0);
            self.perp_adjust(host, &mut side, launch);
            let m = if across <= keep { across } else { keep };
            let gap = keep - across;
            (side.x * m + ux * gap, uy * gap + side.y * m)
        } else if len_sq(velocity) <= EPS {
            (1.0, 0.0)
        } else {
            (-velocity.y, velocity.x)
        };
        let mut v = Vec3::new(x + velocity.x, y + velocity.y, offset.z + velocity.z);
        if !keep_height {
            self.adjust_height(&mut v);
        }
        self.adjust_speed(&mut v);
        *out = v;
    }

    /// The velocity toward the target from `from`: keeping its distance, or meeting it.
    // D2-07188
    pub fn intercept_velocity(
        &self,
        host: &mut dyn GuidanceHost,
        from: &Position,
        launch: bool,
        out: &mut Vec3,
        keep_height: bool,
    ) {
        let offset = geom::offset(&self.target.position, from);
        let velocity = self.target.offset;
        if self.params.use_maintain_distance {
            self.maint_dist_velocity(host, offset, velocity, launch, out, keep_height);
        } else {
            self.collide_velocity(host.now(), offset, velocity, launch, out, keep_height);
        }
    }

    /// Steps the flight times from `t0` to `t1` for the launch velocity nearest the one it flies
    /// at, and narrows the span about the nearest until it is under the time limit; then answers
    /// with the span's start, unless the velocity it flies at is no faster than that, when with
    /// its end (the client compares speeds there, not nearness).
    // D2-07157
    #[allow(clippy::too_many_arguments)]
    fn gravitational_closest(
        &self,
        t0: f32,
        t1: f32,
        offset: Vec3,
        velocity: Vec3,
        gravity: Vec3,
        current: Vec3,
        v0: Vec3,
        v1: Vec3,
    ) -> Vec3 {
        if t1 - t0 < self.time_limit {
            return if len_sq(current) <= len_sq(v0) {
                v1
            } else {
                v0
            };
        }
        let near = |v: Vec3| len_sq(Vec3::new(v.x - current.x, v.y - current.y, v.z - current.z));
        #[allow(clippy::cast_precision_loss)]
        let step = ((t1 - t0) + EPS) / self.steps as f32;
        let (mut ta, mut va) = (t0, v0);
        let (mut tb, mut vb) = (t0, v0);
        let mut best = near(v0);
        let mut t = t0 + step;
        while t < t1 {
            let v = gravitational_velocity(offset, velocity, gravity, t).unwrap_or(Vec3::ZERO);
            let d = near(v);
            if d > best {
                return self
                    .gravitational_closest(ta, t, offset, velocity, gravity, current, va, v);
            }
            best = d;
            (ta, va) = (tb, vb);
            (tb, vb) = (t, v);
            t += step;
        }
        if near(v1) < best {
            self.gravitational_closest(tb, t1, offset, velocity, gravity, current, vb, v1)
        } else {
            self.gravitational_closest(ta, t1, offset, velocity, gravity, current, va, v1)
        }
    }

    /// Steps the flight times from `t0` to `t1` for where the launch speed crosses `speed`, and
    /// narrows the span about the crossing until it is under the time limit; then answers with the
    /// slower end. None when the speed is not crossed.
    // D2-07159
    #[allow(clippy::too_many_arguments)]
    fn gravitational_pitch(
        &self,
        t0: f32,
        t1: f32,
        step: f32,
        offset: Vec3,
        velocity: Vec3,
        gravity: Vec3,
        speed: f32,
        v0: Vec3,
        v1: Vec3,
    ) -> Option<Vec3> {
        let s2 = speed * speed;
        if t1 - t0 < self.time_limit {
            return Some(if len_sq(v0) >= s2 { v1 } else { v0 });
        }
        let below = len_sq(v0) < s2;
        let crossed = |v: Vec3| {
            if below {
                len_sq(v) > s2
            } else {
                len_sq(v) < s2
            }
        };
        let (mut tp, mut vp) = (t0, v0);
        let mut t = t0 + step;
        while t < t1 {
            let v = gravitational_velocity(offset, velocity, gravity, t).unwrap_or(Vec3::ZERO);
            if crossed(v) {
                let half = ((t - tp) + EPS) * 0.5;
                return self
                    .gravitational_pitch(tp, t, half, offset, velocity, gravity, speed, vp, v);
            }
            (tp, vp) = (t, v);
            t += step;
        }
        if !crossed(v1) {
            return None;
        }
        let half = ((t1 - tp) + EPS) * 0.5;
        self.gravitational_pitch(tp, t1, half, offset, velocity, gravity, speed, vp, v1)
    }

    /// As for a speed, for a pitch above level of sine `sine`: the crossing is searched by pitch,
    /// then narrowed as for a speed of `sine`. As no launch velocity of any length counts as
    /// pitched above (see [`greater_pitch`]), this finds a crossing only toward a standstill.
    // D2-07175
    #[allow(clippy::too_many_arguments)]
    fn gravitational_speed(
        &self,
        t0: f32,
        t1: f32,
        step: f32,
        offset: Vec3,
        velocity: Vec3,
        gravity: Vec3,
        sine: f32,
        v0: Vec3,
        v1: Vec3,
    ) -> Option<Vec3> {
        if t1 - t0 < self.time_limit {
            return Some(if len_sq(v1) <= len_sq(v0) { v1 } else { v0 });
        }
        let above = greater_pitch(v0, sine);
        let (mut tp, mut vp) = (t0, v0);
        let mut t = t0 + step;
        while t < t1 {
            let v = gravitational_velocity(offset, velocity, gravity, t).unwrap_or(Vec3::ZERO);
            if greater_pitch(v, sine) != above {
                let half = ((t - tp) + EPS) * 0.5;
                return self
                    .gravitational_pitch(tp, t, half, offset, velocity, gravity, sine, vp, v);
            }
            (tp, vp) = (t, v);
            t += step;
        }
        if greater_pitch(v1, sine) == above {
            return None;
        }
        let half = ((t1 - tp) + EPS) * 0.5;
        self.gravitational_pitch(tp, t1, half, offset, velocity, gravity, sine, vp, v1)
    }

    /// A lobbed launch velocity toward a target `offset` away moving at `velocity`: at `value`
    /// m/s, or at `value` degrees of pitch at its top speed. When no flight time serves, a chase
    /// heading at 45 degrees up at that speed.
    // D2-07181
    pub fn gravitational_launch(
        &self,
        host: &dyn GuidanceHost,
        value: f32,
        offset: Vec3,
        velocity: Vec3,
        pitched: bool,
    ) -> Vec3 {
        let speed = if pitched {
            self.params.max_speed
        } else {
            value
        };
        let g = host.gravity();
        let (mut tmin, mut tmax) = (0.0, 0.0);
        if min_max_time(offset, velocity, g, speed, &mut tmin, &mut tmax) {
            let v0 = gravitational_velocity(offset, velocity, g, tmin).unwrap_or(Vec3::ZERO);
            let v1 = gravitational_velocity(offset, velocity, g, tmax).unwrap_or(Vec3::ZERO);
            #[allow(clippy::cast_precision_loss)]
            let mut step = ((tmax - tmin) + EPS) / self.steps as f32;
            if step < self.min_step {
                step = self.min_step;
            }
            let found = if pitched {
                #[allow(clippy::cast_possible_truncation)]
                let sine = math::sin(f64::from(value) * 0.017_453_292_519_943_295) as f32;
                self.gravitational_speed(tmin, tmax, step, offset, velocity, g, sine, v0, v1)
            } else {
                self.gravitational_pitch(tmin, tmax, step, offset, velocity, g, value, v0, v1)
            };
            if let Some(v) = found {
                return v;
            }
        }
        // The client tests whether it may refuse a far target here and ignores the answer.
        let c = chase_velocity(offset, velocity, self.params.max_speed);
        spherical(speed, geom::heading_of(c.x, c.y).degrees(), 45.0)
    }

    /// Whether the missile starts facing its target from `from`: when the aim says so always, or
    /// when the target is within 45 degrees of where its source object (or else `from`) faces.
    // D2-07167
    pub fn init_heading_toward_target(
        &self,
        host: &dyn GuidanceHost,
        from: &Position,
        target: &Position,
    ) -> bool {
        if !self.targeting.has(aim::TOWARD_TARGET) {
            return false;
        }
        if self.targeting.has(aim::ALWAYS_TOWARD_TARGET) {
            return true;
        }
        let at = host.object_position(self.targeting.source).unwrap_or(*from);
        local_heading(&at, target).is_inside(Heading::new(45.0))
    }

    /// A lobbed launch from `from`: by pitch when the aim has one, else by its starting or top
    /// speed. Unless it starts toward its target, the launch is turned to where its source object
    /// (or else `from`) faces, keeping its speed across the ground.
    // D2-07187
    pub fn initial_gravitational_flight(
        &self,
        host: &dyn GuidanceHost,
        from: &Position,
        out: &mut Vec3,
    ) -> bool {
        let velocity = self.target.offset;
        let offset = geom::offset(from, &self.target.position);
        let t = &self.targeting;
        let (value, pitched) = if t.has(aim::INITIAL_PITCH) {
            (t.initial_pitch, true)
        } else if t.has(aim::INITIAL_SPEED) {
            (t.initial_speed, false)
        } else {
            (self.params.max_speed, false)
        };
        *out = self.gravitational_launch(host, value, offset, velocity, pitched);
        if !self.init_heading_toward_target(host, from, &self.target.position) {
            let h = host
                .object_heading(self.targeting.source)
                .unwrap_or_else(|| geom::frame_heading(&from.frame));
            let across = (out.y * out.y + out.x * out.x).sqrt();
            let r = f64::from(h.radians());
            #[allow(clippy::cast_possible_truncation)]
            {
                out.x = (math::sin(r) * f64::from(across)) as f32;
                out.y = (math::cos(r) * f64::from(across)) as f32;
            }
        }
        true
    }

    /// The launch velocity from `from`. Lobbed when gravity pulls it and it does not keep its
    /// distance; else toward its target when it starts so, or straight ahead at its starting or
    /// top speed. Past its range it is not launched.
    // D2-07190
    pub fn calc_initial_velocity(
        &self,
        host: &mut dyn GuidanceHost,
        from: &Position,
        out: &mut Vec3,
    ) -> u32 {
        *out = Vec3::ZERO;
        if !host.is_valid(from) {
            return fail::NO_PLACE;
        }
        if !self.target.valid {
            return fail::TARGET_UNHEARD;
        }
        if self.params.gravity && !self.params.use_maintain_distance {
            if !self.initial_gravitational_flight(&*host, from, out) {
                return fail::NO_COURSE;
            }
        } else if self.init_heading_toward_target(&*host, from, &self.target.position) {
            self.intercept_velocity(host, from, true, out, true);
        } else {
            let speed = if self.targeting.has(aim::INITIAL_SPEED) {
                self.targeting.initial_speed
            } else {
                self.params.max_speed
            };
            *out = scale(forward(from), speed);
        }
        if self.targeting.has(aim::MAX_RANGE) {
            let o = geom::offset(from, &self.target.position);
            let r = self.targeting.max_range;
            if r * r < len_sq(o) {
                return fail::NO_COURSE;
            }
        }
        fail::NONE
    }

    /// Launches: the launch velocity is the one it has and wants, it faces along its path unless
    /// it aligns otherwise, and it starts flying from now.
    // D2-07191
    pub fn start_movement(&mut self, host: &mut dyn GuidanceHost) -> u32 {
        let from = host.position();
        let mut v = Vec3::ZERO;
        let status = self.calc_initial_velocity(host, &from, &mut v);
        if status != fail::NONE {
            return status;
        }
        if self.params.align == 0 {
            host.align_to_path(true);
        }
        self.velocity = v;
        host.set_missile_velocity(v);
        self.desired_velocity = v;
        self.last_update = host.now();
        self.moving = true;
        host.start_streak();
        host.do_fx(FLIGHT_FX, 1.0);
        fail::NONE
    }

    /// The direction it would launch in. The client works the launch out and then answers north
    /// and level whatever it found; only its failure is kept.
    // D2-07192
    pub fn inquire_direction(&self, host: &mut dyn GuidanceHost) -> (Heading, f32, u32) {
        let from = host.position();
        let mut v = Vec3::ZERO;
        let status = self.calc_initial_velocity(host, &from, &mut v);
        (Heading::ZERO, 0.0, status)
    }

    /// Where it starts: where its holder holds it, else its source object's place for it, else
    /// its base place.
    // D2-07174
    pub fn init_position(&self, host: &dyn GuidanceHost) -> core::result::Result<Position, u32> {
        if host.has_parent() {
            let p = host.drawing_position();
            if host.is_valid(&p) {
                return Ok(p);
            }
        }
        let p = if self.targeting.has(aim::SOURCE_OBJECT) {
            let t = &self.targeting;
            if !host.object_exists(t.source) {
                return Err(fail::NO_PLACE);
            }
            host.object_offset_position(t.source, t.source_height, t.source_scaled, t.source_offset)
                .ok_or(fail::NO_PLACE)?
        } else {
            self.start_position()
        };
        if host.is_valid(&p) {
            Ok(p)
        } else {
            Err(fail::NO_PLACE)
        }
    }

    /// Puts the missile in the world at its start; its lifespan and fuel start from now.
    // D2-07180
    pub fn attempt_world_placement(&mut self, host: &mut dyn GuidanceHost) -> u32 {
        let p = match self.init_position(&*host) {
            Ok(p) => p,
            Err(status) => return status,
        };
        let params = self.params;
        self.start_lifespan_and_fuel(host.now(), &params);
        if !host.init_end(&p) {
            return fail::NO_PLACE;
        }
        fail::NONE
    }

    /// Accelerates toward the velocity it wants over its reaction time, with its offset; conserving
    /// fuel, it does not push against an axis its drag already slows faster. The push is limited in
    /// the missile's own frame (forward along its flight, up as near the world's up as that allows)
    /// to its greatest accelerations each way; out of fuel it keeps what it had.
    // D2-07177
    pub fn calc_acceleration(&mut self) {
        let p = &self.params;
        if p.uses_fuel && self.fuel == 0.0 {
            return;
        }
        let cur = self.velocity;
        let diff = Vec3::new(
            self.desired_velocity.x - cur.x,
            self.desired_velocity.y - cur.y,
            self.desired_velocity.z - cur.z,
        );
        let k = 1.0 / p.reaction_time;
        let mut a = Vec3::new(
            diff.x * k + self.accel_offset.x,
            k * diff.y + self.accel_offset.y,
            k * diff.z + self.accel_offset.z,
        );
        if p.conserve_fuel && p.drag != 0.0 {
            let slow = |c: f32, d: f32, a: &mut f32| {
                if c * d < 0.0 && c * p.drag * 1.5 > *a {
                    *a = 0.0;
                }
            };
            slow(cur.x, diff.x, &mut a.x);
            slow(cur.y, diff.y, &mut a.y);
            slow(cur.z, diff.z, &mut a.z);
        }
        let s = len_sq(cur).sqrt();
        let mut fwd = if s < EPS {
            Vec3::new(0.0, 1.0, 0.0)
        } else {
            scale(cur, 1.0 / s)
        };
        let h2 = fwd.x * fwd.x + fwd.y * fwd.y;
        let up = if h2 < EPS {
            fwd = Vec3::new(0.0, 0.0, 1.0);
            Vec3::new(0.0, 1.0, 0.0)
        } else if fwd.z == 0.0 {
            Vec3::new(0.0, 0.0, 1.0)
        } else {
            let u = Vec3::new(-fwd.x, -fwd.y, h2 / fwd.z);
            let l = len_sq(u).sqrt();
            if l >= EPS {
                scale(u, 1.0 / l)
            } else {
                u
            }
        };
        let right = Vec3::new(
            fwd.y * up.z - up.y * fwd.z,
            up.x * fwd.z - fwd.x * up.z,
            up.y * fwd.x - up.x * fwd.y,
        );
        let mut l = Vec3::new(dot(right, a), dot(fwd, a), dot(up, a));
        let mut clamped = false;
        let mut cap = |v: &mut f32, hi: f32, lo: f32| {
            if *v > hi {
                *v = hi;
                clamped = true;
            }
            if *v < -lo {
                *v = -lo;
                clamped = true;
            }
        };
        cap(&mut l.x, p.max_right_accel, p.max_left_accel);
        cap(&mut l.y, p.max_forward_accel, p.max_decel);
        cap(&mut l.z, p.max_up_accel, p.max_down_accel);
        if clamped {
            a = Vec3::new(
                up.x * l.z + right.x * l.x + fwd.x * l.y,
                up.y * l.z + right.y * l.x + fwd.y * l.y,
                up.z * l.z + right.z * l.x + fwd.z * l.y,
            );
        }
        self.acceleration = a;
    }

    /// Every reaction interval: burns the fuel of the time since, notes its height over the
    /// ground, and works out the velocity it wants (lobbed when gravity pulls it and it does not
    /// keep its distance, else toward the target) and its acceleration toward it.
    // D2-07189
    pub fn missile_info_update(&mut self, host: &mut dyn GuidanceHost) -> u32 {
        let now = host.now();
        #[allow(clippy::cast_possible_truncation)]
        let dt = (now - self.last_update) as f32;
        if self.params.reaction_interval >= dt {
            return fail::NONE;
        }
        self.last_update = now;
        if !self.target.valid {
            return fail::TARGET_UNHEARD;
        }
        self.update_fuel_use(f64::from(dt));
        if let Some(h) = host.height_above_walkable() {
            self.height = h;
        }
        if self.params.gravity && !self.params.use_maintain_distance {
            let p = host.position();
            let offset = geom::offset(&p, &self.target.position);
            let velocity = self.target.offset;
            let g = host.gravity();
            let speed = self.params.max_speed;
            let (mut tmin, mut tmax) = (0.0, 0.0);
            self.desired_velocity =
                if min_max_time(offset, velocity, g, speed, &mut tmin, &mut tmax) {
                    let v0 =
                        gravitational_velocity(offset, velocity, g, tmin).unwrap_or(Vec3::ZERO);
                    let v1 =
                        gravitational_velocity(offset, velocity, g, tmax).unwrap_or(Vec3::ZERO);
                    self.gravitational_closest(
                        tmin,
                        tmax,
                        offset,
                        velocity,
                        g,
                        self.velocity,
                        v0,
                        v1,
                    )
                } else {
                    let c = chase_velocity(offset, velocity, speed);
                    spherical(speed, geom::heading_of(c.x, c.y).degrees(), 45.0)
                };
        } else {
            let p = host.position();
            let mut v = self.desired_velocity;
            self.intercept_velocity(host, &p, false, &mut v, false);
            self.desired_velocity = v;
        }
        self.calc_acceleration();
        fail::NONE
    }

    /// One step of the guidance: looks for the target; once it is known and heard, places the
    /// missile and launches it when they are due, then steers it while it flies, and reports it
    /// out of fuel once.
    // D2-07193
    pub fn update(&mut self, host: &mut dyn GuidanceHost) -> u32 {
        if !self.activated {
            return fail::NONE;
        }
        let status = self.target_update(host);
        if status != fail::NONE || self.target_status == 0 {
            return status;
        }
        if self.target_status != 1 {
            return fail::NO_TARGET;
        }
        if !self.target.valid {
            return fail::NONE;
        }
        if self.ready_to_place {
            let status = self.attempt_world_placement(host);
            if status != fail::NONE {
                return status;
            }
            self.ready_to_place = false;
        }
        if !host.is_in_world() {
            return fail::NONE;
        }
        if self.ready_to_fire {
            let status = self.start_movement(host);
            if status != fail::NONE {
                return status;
            }
            self.ready_to_fire = false;
        }
        if !self.moving {
            return fail::NONE;
        }
        let status = self.missile_info_update(host);
        if status == fail::NONE
            && self.params.uses_fuel
            && self.out_of_fuel()
            && self.params.report_no_fuel
            && !self.fuel_reported
        {
            self.report_out_of_fuel(host);
        }
        status
    }

    /// Moves the missile `dt` seconds at velocity `v` under its acceleration, gravity, drag (taken
    /// from the velocity before the wind) and wind, into `frame` (the step's offset); its new
    /// velocity is set. A missile not flying moves nowhere. Its lifespan ends here, reported once.
    // D2-07165
    pub fn update_missile_physics(
        &mut self,
        host: &mut dyn GuidanceHost,
        v: Vec3,
        dt: f32,
        frame: &mut Frame,
    ) {
        if !self.moving {
            *frame = Frame::new(Vec3::ZERO, Quat::IDENTITY);
            return;
        }
        let mut v = v;
        let mut a = self.acceleration;
        if self.params.gravity {
            let g = host.gravity();
            a = Vec3::new(a.x + g.x, a.y + g.y, a.z + g.z);
        }
        let drag = self.params.drag;
        if drag != 0.0 {
            a = Vec3::new(a.x - v.x * drag, a.y - v.y * drag, a.z - v.z * drag);
        }
        if self.params.wind {
            let w = host.wind();
            v = Vec3::new(v.x + w.x, v.y + w.y, v.z + w.z);
        }
        let half = scale(a, 0.5);
        let o = &mut frame.origin;
        o.x += v.x * dt + half.x * dt * dt;
        o.y += v.y * dt + half.y * dt * dt;
        o.z += v.z * dt + half.z * dt * dt;
        let nv = Vec3::new(a.x * dt + v.x, a.y * dt + v.y, a.z * dt + v.z);
        self.velocity = nv;
        host.set_missile_velocity(nv);
        if self.params.has_lifespan && host.now() > self.expire_time && !self.expire_reported {
            host.report_expired();
            self.expire_reported = true;
        }
    }

    /// What a collision sets the missile off on, if anything: the first object it began touching
    /// that it does not pass through; else, of those it passes through other than its source, its
    /// target, or (when it goes off on a miss) any player or creature; else a collision normal
    /// reported (with no object). `Some(None)` goes off on no object.
    // D2-07150
    pub fn should_detonate(
        &self,
        host: &dyn GuidanceHost,
        report: &CollisionReport,
    ) -> Option<Option<u64>> {
        for r in &report.records {
            if !r.begin {
                continue;
            }
            let obj = host.object_exists(r.object).then_some(r.object);
            if !r.ethereal {
                return Some(obj);
            }
            if r.object == self.targeting.source {
                continue;
            }
            if self.params.detonates_on_miss {
                if obj.is_some() && host.object_is_player_or_creature(r.object) {
                    return Some(obj);
                }
            } else if r.object == self.targeting.target {
                return Some(obj);
            }
        }
        if !report.is_end_collision() && report.collided().is_some() {
            return Some(None);
        }
        None
    }

    /// A collision: when it sets the missile off, the missile stops flying and fades out, unless
    /// it sticks, when it sticks to a moving object it struck that is not itself a missile. What it
    /// went off on is answered (`Some(None)` for no object).
    // D2-07168
    pub fn handle_collision(
        &mut self,
        host: &mut dyn GuidanceHost,
        report: &CollisionReport,
    ) -> Option<Option<u64>> {
        let hit = self.should_detonate(&*host, report)?;
        self.activated = false;
        self.moving = false;
        host.stop_fx(FLIGHT_FX);
        host.stop_streak();
        if !self.params.sticky {
            host.delete_self(0.5, 0.0);
        } else if let Some(o) = hit {
            if host.object_is_dynamic(o) && !host.object_has_guidance(o) {
                host.stick_to(o);
            }
        }
        Some(hit)
    }
}

impl Pack for Guidance {
    /// Its settings, aim and target, then its flight state in the client's order.
    // D2-07173
    fn pack(&self, w: &mut Writer) {
        self.params.pack(w);
        self.targeting.pack(w);
        self.target.write(w);
        w.u32(self.target_status);
        write_vec3(w, self.accel_offset);
        write_vec3(w, self.desired_velocity);
        w.f64(self.last_update);
        w.f32(self.fuel);
        w.bool32(self.fuel_reported);
        w.f64(self.expire_time);
        w.bool32(self.expire_reported);
        w.f32(self.height);
        write_vec3(w, self.acceleration);
        write_vec3(w, self.velocity);
        w.u32(self.steps);
        w.f32(self.min_step);
        w.f32(self.time_limit);
        w.bool32(self.can_land);
        w.bool32(self.ready_to_place);
        w.bool32(self.ready_to_fire);
        w.bool32(self.activated);
        w.bool32(self.moving);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let params = MissileParameters::unpack(r, ctx)?;
        let targeting = MissileTargetingParameters::unpack(r, ctx)?;
        let target = Aim::read(r)?;
        Ok(Self {
            params,
            targeting,
            target,
            target_status: r.u32()?,
            accel_offset: read_vec3(r)?,
            desired_velocity: read_vec3(r)?,
            last_update: r.f64()?,
            fuel: r.f32()?,
            fuel_reported: r.bool32()?,
            expire_time: r.f64()?,
            expire_reported: r.bool32()?,
            height: r.f32()?,
            acceleration: read_vec3(r)?,
            velocity: read_vec3(r)?,
            steps: r.u32()?,
            min_step: r.f32()?,
            time_limit: r.f32()?,
            can_land: r.bool32()?,
            ready_to_place: r.bool32()?,
            ready_to_fire: r.bool32()?,
            activated: r.bool32()?,
            moving: r.bool32()?,
        })
    }
}
