//! How a missile flies: its speed limits, how hard it may turn, speed up and slow down, its fuel
//! and lifespan, what pulls on it, how it keeps to the ground, how it reacts to its target, and
//! when it goes off. Each group is marked set or not in a bit word; only the set groups are
//! copied onto another missile's settings and sent.

use auberean_assets::Context;
use auberean_dat::reader::Result;

use crate::pack::{Pack, Reader, Writer};

/// Which groups of [`MissileParameters`] are set.
pub mod set {
    /// Top and bottom speeds.
    pub const SPEED_LIMITS: u32 = 0x1;
    /// The six greatest accelerations.
    pub const MAX_ACCEL: u32 = 0x2;
    pub const FUEL: u32 = 0x4;
    pub const LIFESPAN: u32 = 0x8;
    pub const DRAG: u32 = 0x10;
    pub const AFFECTED_GRAVITY: u32 = 0x20;
    pub const AFFECTED_WIND: u32 = 0x40;
    pub const FOLLOW_GROUND: u32 = 0x80;
    pub const MAINT_DIST: u32 = 0x200;
    pub const MISSILE_REACTION: u32 = 0x400;
    pub const TURN_METHOD: u32 = 0x800;
    /// Conserving fuel and taking the surroundings' top speeds share this bit: the client sets,
    /// copies and sends both groups under it, and never uses 0x100.
    pub const CONSERVE_FUEL: u32 = 0x1000;
    pub const ENV_MAX_VELOCITY: u32 = 0x1000;
    pub const ALIGN_TYPE: u32 = 0x2000;
    pub const STICKY: u32 = 0x4000;
    pub const CAN_LAND: u32 = 0x8000;
    pub const DETONATES_ON_MISS: u32 = 0x1_0000;
    pub const DETONATES_ON_PROXIMITY: u32 = 0x2_0000;
}

/// How a missile keeps to the ground.
pub mod ground {
    /// It does not.
    pub const NONE: u32 = 0;
    /// It climbs to stay at least its height above the ground.
    pub const AT_LEAST: u32 = 1;
    /// It holds its height above the ground.
    pub const HOLD: u32 = 2;
}

/// A missile's flight settings.
// D2-15892
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MissileParameters {
    /// See [`set`].
    pub bits: u32,
    pub max_speed: f32,
    pub min_speed: f32,
    pub max_forward_accel: f32,
    pub max_decel: f32,
    pub max_left_accel: f32,
    pub max_right_accel: f32,
    pub max_up_accel: f32,
    pub max_down_accel: f32,
    pub total_fuel: f32,
    pub uses_fuel: bool,
    pub report_no_fuel: bool,
    /// Seconds.
    pub lifespan: f32,
    pub has_lifespan: bool,
    /// How much of its velocity it loses to the air each second.
    pub drag: f32,
    pub gravity: bool,
    pub wind: bool,
    /// See [`ground`].
    pub ground_follow: u32,
    pub height_above_ground: f32,
    /// Which way a missile keeping its distance circles its target: the other way (0), as worked
    /// out (1), not sideways at all when it has no least speed (2), else the way it is flying, or
    /// at random at launch.
    pub turn_method: u32,
    pub conserve_fuel: bool,
    pub consider_remaining_time: bool,
    pub min_catchup_speed: f32,
    pub maintain_distance: f32,
    pub use_maintain_distance: bool,
    /// Seconds between corrections of its course.
    pub reaction_interval: f32,
    /// Seconds it takes to reach the velocity it wants.
    pub reaction_time: f32,
    pub correct_for_target_velocity: bool,
    pub env_max_speeds: bool,
    pub align: u32,
    pub spin_rate: f32,
    pub sticky: bool,
    pub can_land: bool,
    pub detonates_on_miss: bool,
    pub detonation_proximity: f32,
    pub detonation_chance: f32,
}

impl Default for MissileParameters {
    /// Nothing set: 1 to 50 m/s, accelerations of 1 m/s², no fuel used, no lifespan, nothing
    /// pulling, 1.5 m above the ground, turning by heading, reacting every half second over one.
    // D2-15767
    fn default() -> Self {
        Self {
            bits: 0,
            max_speed: 50.0,
            min_speed: 1.0,
            max_forward_accel: 1.0,
            max_decel: 1.0,
            max_left_accel: 1.0,
            max_right_accel: 1.0,
            max_up_accel: 1.0,
            max_down_accel: 1.0,
            total_fuel: 0.0,
            uses_fuel: false,
            report_no_fuel: true,
            lifespan: 0.0,
            has_lifespan: false,
            drag: 0.0,
            gravity: false,
            wind: false,
            ground_follow: ground::NONE,
            height_above_ground: 1.5,
            turn_method: 2,
            conserve_fuel: false,
            consider_remaining_time: false,
            min_catchup_speed: 2.0,
            maintain_distance: 5.0,
            use_maintain_distance: false,
            reaction_interval: 0.5,
            reaction_time: 1.0,
            correct_for_target_velocity: true,
            env_max_speeds: true,
            align: 0,
            spin_rate: 0.0,
            sticky: false,
            can_land: false,
            detonates_on_miss: false,
            detonation_proximity: 0.0,
            detonation_chance: 0.0,
        }
    }
}

impl MissileParameters {
    /// Whether any of `bits` is set.
    // D2-15764
    #[must_use]
    pub fn is_set(&self, bits: u32) -> bool {
        self.bits & bits != 0
    }

    // D2-15765
    pub fn set_detonates_on_miss(&mut self, on: bool) {
        self.detonates_on_miss = on;
        self.bits |= set::DETONATES_ON_MISS;
    }

    // D2-15766
    pub fn set_detonation_proximity(&mut self, proximity: f32, chance: f32) {
        self.detonation_proximity = proximity;
        self.detonation_chance = chance;
        self.bits |= set::DETONATES_ON_PROXIMITY;
    }

    // D2-15770
    pub fn set_speed_limits(&mut self, max: f32, min: f32) {
        self.max_speed = max;
        self.min_speed = min;
        self.bits |= set::SPEED_LIMITS;
    }

    // D2-15771
    pub fn set_max_acceleration(
        &mut self,
        forward: f32,
        decel: f32,
        left: f32,
        right: f32,
        up: f32,
        down: f32,
    ) {
        self.max_forward_accel = forward;
        self.max_decel = decel;
        self.max_left_accel = left;
        self.max_right_accel = right;
        self.max_up_accel = up;
        self.max_down_accel = down;
        self.bits |= set::MAX_ACCEL;
    }

    // D2-15788
    pub fn set_max_acceleration_uniform(&mut self, a: f32) {
        self.set_max_acceleration(a, a, a, a, a, a);
    }

    /// Fuel to burn: setting it also says the missile uses fuel.
    // D2-15772
    pub fn set_total_fuel(&mut self, fuel: f32, report_no_fuel: bool) {
        self.total_fuel = fuel;
        self.uses_fuel = true;
        self.report_no_fuel = report_no_fuel;
        self.bits |= set::FUEL;
    }

    // D2-15773
    pub fn set_use_no_fuel(&mut self) {
        self.uses_fuel = false;
        self.bits |= set::FUEL;
    }

    // D2-15774
    pub fn set_total_lifespan(&mut self, seconds: f32) {
        self.lifespan = seconds;
        self.has_lifespan = true;
        self.bits |= set::LIFESPAN;
    }

    // D2-15775
    pub fn set_has_no_lifespan(&mut self) {
        self.has_lifespan = false;
        self.bits |= set::LIFESPAN;
    }

    // D2-15776
    pub fn set_drag(&mut self, drag: f32) {
        self.drag = drag;
        self.bits |= set::DRAG;
    }

    // D2-15777
    pub fn set_affected_by_gravity(&mut self, on: bool) {
        self.gravity = on;
        self.bits |= set::AFFECTED_GRAVITY;
    }

    // D2-15778
    pub fn set_follow_ground(&mut self, kind: u32, height: f32) {
        self.ground_follow = kind;
        self.height_above_ground = height;
        self.bits |= set::FOLLOW_GROUND;
    }

    // D2-15779
    pub fn set_conserve_fuel(&mut self, on: bool, min_catchup_speed: f32, remaining_time: bool) {
        self.conserve_fuel = on;
        self.consider_remaining_time = remaining_time;
        self.min_catchup_speed = min_catchup_speed;
        self.bits |= set::CONSERVE_FUEL;
    }

    /// Keeps `distance` from the target rather than striking it.
    // D2-15780
    pub fn set_maintain_distance(&mut self, distance: f32) {
        self.maintain_distance = distance;
        self.use_maintain_distance = true;
        self.bits |= set::MAINT_DIST;
    }

    /// Strikes the target rather than keeping a distance from it.
    // D2-15781
    pub fn set_homing(&mut self) {
        self.use_maintain_distance = false;
        self.bits |= set::MAINT_DIST;
    }

    // D2-15782
    pub fn set_reaction(&mut self, interval: f32, time: f32, correct_for_target_velocity: bool) {
        self.reaction_interval = interval;
        self.correct_for_target_velocity = correct_for_target_velocity;
        self.reaction_time = time;
        self.bits |= set::MISSILE_REACTION;
    }

    // D2-15783
    pub fn set_turn_method(&mut self, method: u32) {
        self.turn_method = method;
        self.bits |= set::TURN_METHOD;
    }

    // D2-15784
    pub fn use_env_max_speeds(&mut self, on: bool) {
        self.env_max_speeds = on;
        self.bits |= set::ENV_MAX_VELOCITY;
    }

    // D2-15785
    pub fn set_align(&mut self, align: u32, spin_rate: f32) {
        self.align = align;
        self.spin_rate = spin_rate;
        self.bits |= set::ALIGN_TYPE;
    }

    // D2-15786
    pub fn set_sticky(&mut self, on: bool) {
        self.sticky = on;
        self.bits |= set::STICKY;
    }

    // D2-15787
    pub fn set_can_land(&mut self, on: bool) {
        self.can_land = on;
        self.bits |= set::CAN_LAND;
    }

    /// Copies each group set here onto `to`, marking it set there. The conserving group and the
    /// top-speeds flag go together, as their bit is one.
    // D2-15768
    pub fn copy_into(&self, to: &mut Self) -> bool {
        let has = |b: u32| self.bits & b != 0;
        if has(set::SPEED_LIMITS) {
            to.max_speed = self.max_speed;
            to.min_speed = self.min_speed;
            to.bits |= set::SPEED_LIMITS;
        }
        if has(set::MAX_ACCEL) {
            to.max_forward_accel = self.max_forward_accel;
            to.max_decel = self.max_decel;
            to.max_left_accel = self.max_left_accel;
            to.max_right_accel = self.max_right_accel;
            to.max_up_accel = self.max_up_accel;
            to.max_down_accel = self.max_down_accel;
            to.bits |= set::MAX_ACCEL;
        }
        if has(set::FUEL) {
            to.total_fuel = self.total_fuel;
            to.uses_fuel = self.uses_fuel;
            to.report_no_fuel = self.report_no_fuel;
            to.bits |= set::FUEL;
        }
        if has(set::LIFESPAN) {
            to.lifespan = self.lifespan;
            to.has_lifespan = self.has_lifespan;
            to.bits |= set::LIFESPAN;
        }
        if has(set::DRAG) {
            to.drag = self.drag;
            to.bits |= set::DRAG;
        }
        if has(set::AFFECTED_GRAVITY) {
            to.gravity = self.gravity;
            to.bits |= set::AFFECTED_GRAVITY;
        }
        if has(set::AFFECTED_WIND) {
            to.wind = self.wind;
            to.bits |= set::AFFECTED_WIND;
        }
        if has(set::FOLLOW_GROUND) {
            to.ground_follow = self.ground_follow;
            to.height_above_ground = self.height_above_ground;
            to.bits |= set::FOLLOW_GROUND;
        }
        if has(set::CONSERVE_FUEL) {
            to.conserve_fuel = self.conserve_fuel;
            to.min_catchup_speed = self.min_catchup_speed;
            to.consider_remaining_time = self.consider_remaining_time;
            to.bits |= set::CONSERVE_FUEL;
        }
        if has(set::MAINT_DIST) {
            to.maintain_distance = self.maintain_distance;
            to.use_maintain_distance = self.use_maintain_distance;
            to.bits |= set::MAINT_DIST;
        }
        if has(set::MISSILE_REACTION) {
            to.reaction_interval = self.reaction_interval;
            to.reaction_time = self.reaction_time;
            to.correct_for_target_velocity = self.correct_for_target_velocity;
            to.bits |= set::MISSILE_REACTION;
        }
        if has(set::TURN_METHOD) {
            to.turn_method = self.turn_method;
            to.bits |= set::TURN_METHOD;
        }
        if has(set::ENV_MAX_VELOCITY) {
            to.env_max_speeds = self.env_max_speeds;
            to.bits |= set::ENV_MAX_VELOCITY;
        }
        if has(set::ALIGN_TYPE) {
            to.align = self.align;
            to.spin_rate = self.spin_rate;
            to.bits |= set::ALIGN_TYPE;
        }
        if has(set::STICKY) {
            to.sticky = self.sticky;
            to.bits |= set::STICKY;
        }
        if has(set::CAN_LAND) {
            to.can_land = self.can_land;
            to.bits |= set::CAN_LAND;
        }
        if has(set::DETONATES_ON_MISS) {
            to.detonates_on_miss = self.detonates_on_miss;
            to.bits |= set::DETONATES_ON_MISS;
        }
        if has(set::DETONATES_ON_PROXIMITY) {
            to.detonation_proximity = self.detonation_proximity;
            to.detonation_chance = self.detonation_chance;
            to.bits |= set::DETONATES_ON_PROXIMITY;
        }
        true
    }
}

impl Pack for MissileParameters {
    /// The bit word, then each set group in the order the client sends them (the conserving group
    /// after following the ground, the top-speeds flag after turning).
    // D2-15769
    fn pack(&self, w: &mut Writer) {
        let has = |b: u32| self.bits & b != 0;
        w.u32(self.bits);
        if has(set::SPEED_LIMITS) {
            w.f32(self.max_speed);
            w.f32(self.min_speed);
        }
        if has(set::MAX_ACCEL) {
            for a in [
                self.max_forward_accel,
                self.max_decel,
                self.max_left_accel,
                self.max_right_accel,
                self.max_up_accel,
                self.max_down_accel,
            ] {
                w.f32(a);
            }
        }
        if has(set::FUEL) {
            w.f32(self.total_fuel);
            w.bool32(self.uses_fuel);
            w.bool32(self.report_no_fuel);
        }
        if has(set::LIFESPAN) {
            w.f32(self.lifespan);
            w.bool32(self.has_lifespan);
        }
        if has(set::DRAG) {
            w.f32(self.drag);
        }
        if has(set::AFFECTED_GRAVITY) {
            w.bool32(self.gravity);
        }
        if has(set::AFFECTED_WIND) {
            w.bool32(self.wind);
        }
        if has(set::FOLLOW_GROUND) {
            w.u32(self.ground_follow);
            w.f32(self.height_above_ground);
        }
        if has(set::CONSERVE_FUEL) {
            w.bool32(self.conserve_fuel);
            w.bool32(self.consider_remaining_time);
            w.f32(self.min_catchup_speed);
        }
        if has(set::MAINT_DIST) {
            w.f32(self.maintain_distance);
            w.bool32(self.use_maintain_distance);
        }
        if has(set::MISSILE_REACTION) {
            w.f32(self.reaction_interval);
            w.f32(self.reaction_time);
            w.bool32(self.correct_for_target_velocity);
        }
        if has(set::TURN_METHOD) {
            w.u32(self.turn_method);
        }
        if has(set::ENV_MAX_VELOCITY) {
            w.bool32(self.env_max_speeds);
        }
        if has(set::ALIGN_TYPE) {
            w.u32(self.align);
            w.f32(self.spin_rate);
        }
        if has(set::STICKY) {
            w.bool32(self.sticky);
        }
        if has(set::CAN_LAND) {
            w.bool32(self.can_land);
        }
        if has(set::DETONATES_ON_MISS) {
            w.bool32(self.detonates_on_miss);
        }
        if has(set::DETONATES_ON_PROXIMITY) {
            w.f32(self.detonation_proximity);
            w.f32(self.detonation_chance);
        }
    }

    /// Read over the defaults: a group not sent keeps its default.
    fn unpack(r: &mut Reader<'_>, _ctx: &Context) -> Result<Self> {
        let mut p = Self {
            bits: r.u32()?,
            ..Self::default()
        };
        let has = |b: u32| p.bits & b != 0;
        let (speed, accel, fuel, life) = (
            has(set::SPEED_LIMITS),
            has(set::MAX_ACCEL),
            has(set::FUEL),
            has(set::LIFESPAN),
        );
        let (drag, gravity, wind, follow) = (
            has(set::DRAG),
            has(set::AFFECTED_GRAVITY),
            has(set::AFFECTED_WIND),
            has(set::FOLLOW_GROUND),
        );
        let (conserve, maint, reaction, turn) = (
            has(set::CONSERVE_FUEL),
            has(set::MAINT_DIST),
            has(set::MISSILE_REACTION),
            has(set::TURN_METHOD),
        );
        let (align, sticky, land, miss, proximity) = (
            has(set::ALIGN_TYPE),
            has(set::STICKY),
            has(set::CAN_LAND),
            has(set::DETONATES_ON_MISS),
            has(set::DETONATES_ON_PROXIMITY),
        );
        if speed {
            p.max_speed = r.f32()?;
            p.min_speed = r.f32()?;
        }
        if accel {
            p.max_forward_accel = r.f32()?;
            p.max_decel = r.f32()?;
            p.max_left_accel = r.f32()?;
            p.max_right_accel = r.f32()?;
            p.max_up_accel = r.f32()?;
            p.max_down_accel = r.f32()?;
        }
        if fuel {
            p.total_fuel = r.f32()?;
            p.uses_fuel = r.bool32()?;
            p.report_no_fuel = r.bool32()?;
        }
        if life {
            p.lifespan = r.f32()?;
            p.has_lifespan = r.bool32()?;
        }
        if drag {
            p.drag = r.f32()?;
        }
        if gravity {
            p.gravity = r.bool32()?;
        }
        if wind {
            p.wind = r.bool32()?;
        }
        if follow {
            p.ground_follow = r.u32()?;
            p.height_above_ground = r.f32()?;
        }
        if conserve {
            p.conserve_fuel = r.bool32()?;
            p.consider_remaining_time = r.bool32()?;
            p.min_catchup_speed = r.f32()?;
        }
        if maint {
            p.maintain_distance = r.f32()?;
            p.use_maintain_distance = r.bool32()?;
        }
        if reaction {
            p.reaction_interval = r.f32()?;
            p.reaction_time = r.f32()?;
            p.correct_for_target_velocity = r.bool32()?;
        }
        if turn {
            p.turn_method = r.u32()?;
        }
        if conserve {
            p.env_max_speeds = r.bool32()?;
        }
        if align {
            p.align = r.u32()?;
            p.spin_rate = r.f32()?;
        }
        if sticky {
            p.sticky = r.bool32()?;
        }
        if land {
            p.can_land = r.bool32()?;
        }
        if miss {
            p.detonates_on_miss = r.bool32()?;
        }
        if proximity {
            p.detonation_proximity = r.f32()?;
            p.detonation_chance = r.f32()?;
        }
        Ok(p)
    }
}
