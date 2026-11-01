//! What a missile aims at and where it starts: an object (with a height on it and an offset) or a
//! place to strike, an object or a place to start from, a starting speed or pitch, how far it may
//! be fired, and its chance to go off near its target. A bit word marks what is set.

use auberean_assets::Context;
use auberean_dat::reader::Result;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::pack::{read_position, read_vec3, write_position, write_vec3, Pack, Reader, Writer};
use crate::target::NO_OBJECT;

/// What [`MissileTargetingParameters`] holds.
pub mod aim {
    /// It strikes an object.
    pub const TARGET_OBJECT: u32 = 0x1;
    /// It strikes a place.
    pub const TARGET_POSITION: u32 = 0x2;
    pub const INITIAL_SPEED: u32 = 0x4;
    /// It starts from a place.
    pub const BASE_POSITION: u32 = 0x8;
    /// It starts from an object.
    pub const SOURCE_OBJECT: u32 = 0x10;
    /// It starts where the object holding it is.
    pub const PARENT_BASE: u32 = 0x20;
    /// It starts facing its target when that is within 45 degrees of its own facing.
    pub const TOWARD_TARGET: u32 = 0x40;
    pub const INITIAL_PITCH: u32 = 0x80;
    /// It is not fired at a target past its range.
    pub const HOLD_FIRE_PAST_RANGE: u32 = 0x100;
    pub const MAX_RANGE: u32 = 0x200;
    /// It always starts facing its target.
    pub const ALWAYS_TOWARD_TARGET: u32 = 0x400;
    pub const DETONATION_CHANCE: u32 = 0x800;
}

fn origin() -> Position {
    Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY))
}

/// What a missile aims at and where it starts from.
// D2-07156, D2-15789
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MissileTargetingParameters {
    /// See [`aim`].
    pub bits: u32,
    pub target: u64,
    /// Where on the target it aims, as a fraction of the target's height.
    pub target_height: f32,
    /// Whether the offset is scaled by the target's scale.
    pub target_scaled: bool,
    pub target_offset: Vec3,
    pub target_position: Position,
    pub base_position: Position,
    pub source: u64,
    pub source_height: f32,
    pub source_scaled: bool,
    pub source_offset: Vec3,
    pub initial_speed: f32,
    /// Degrees above level.
    pub initial_pitch: f32,
    pub max_range: f32,
    pub detonation_chance: f32,
}

impl Default for MissileTargetingParameters {
    /// Nothing set: no objects, aimed three quarters of the way up either.
    // D2-15791
    fn default() -> Self {
        Self {
            bits: 0,
            target: NO_OBJECT,
            target_height: 0.75,
            target_scaled: false,
            target_offset: Vec3::ZERO,
            target_position: origin(),
            base_position: origin(),
            source: NO_OBJECT,
            source_height: 0.75,
            source_scaled: false,
            source_offset: Vec3::ZERO,
            initial_speed: 0.0,
            initial_pitch: 0.0,
            max_range: 0.0,
            detonation_chance: 0.0,
        }
    }
}

impl MissileTargetingParameters {
    /// Whether any of `bits` is set.
    // D2-15790
    #[must_use]
    pub fn has(&self, bits: u32) -> bool {
        self.bits & bits != 0
    }

    /// Strikes an object: `height` of the way up it, `offset` from there.
    // D2-15793
    pub fn set_target_object(&mut self, object: u64, height: f32, scaled: bool, offset: Vec3) {
        self.target = object;
        self.target_height = height;
        self.target_scaled = scaled;
        self.target_offset = offset;
        self.bits |= aim::TARGET_OBJECT;
    }

    // D2-15794
    pub fn set_target_position(&mut self, p: Position) {
        self.target_position = p;
        self.bits |= aim::TARGET_POSITION;
    }

    // D2-15795
    pub fn set_initial_speed(&mut self, speed: f32) {
        self.initial_speed = speed;
        self.bits |= aim::INITIAL_SPEED;
    }

    // D2-15796
    pub fn set_initial_pitch(&mut self, degrees: f32) {
        self.initial_pitch = degrees;
        self.bits |= aim::INITIAL_PITCH;
    }

    // D2-15797
    pub fn set_base_position(&mut self, p: Position) {
        self.base_position = p;
        self.bits |= aim::BASE_POSITION;
    }

    // D2-15798
    pub fn use_parent_for_base(&mut self) {
        self.bits |= aim::PARENT_BASE;
    }

    /// Starts from an object: `height` of the way up it, `offset` from there.
    // D2-15799
    pub fn use_source_for_base(&mut self, object: u64, height: f32, scaled: bool, offset: Vec3) {
        self.source = object;
        self.source_height = height;
        self.source_scaled = scaled;
        self.source_offset = offset;
        self.bits |= aim::SOURCE_OBJECT;
    }

    // D2-15800
    pub fn set_no_fire_if_too_far(&mut self) {
        self.bits |= aim::HOLD_FIRE_PAST_RANGE;
    }

    // D2-15801
    pub fn set_max_range(&mut self, range: f32) {
        self.max_range = range;
        self.bits |= aim::MAX_RANGE;
    }

    // D2-15802
    pub fn set_detonation_chance(&mut self, chance: f32) {
        self.detonation_chance = chance;
        self.bits |= aim::DETONATION_CHANCE;
    }

    // D2-15803
    pub fn set_start_toward_target(&mut self) {
        self.bits |= aim::TOWARD_TARGET;
    }

    // D2-15804
    pub fn set_always_start_toward_target(&mut self) {
        self.bits |= aim::ALWAYS_TOWARD_TARGET;
    }

    /// Copies what is set here onto `to`. A target object replaces a target place there and the
    /// other way round, and likewise a source object and a base place. Starting where the holder
    /// is is not copied.
    // D2-15805
    pub fn copy_into(&self, to: &mut Self) -> bool {
        if self.has(aim::TARGET_OBJECT) {
            to.target = self.target;
            to.target_height = self.target_height;
            to.target_scaled = self.target_scaled;
            to.target_offset = self.target_offset;
            to.bits = (to.bits & !aim::TARGET_POSITION) | aim::TARGET_OBJECT;
        } else if self.has(aim::TARGET_POSITION) {
            to.target_position = self.target_position;
            to.bits = (to.bits & !aim::TARGET_OBJECT) | aim::TARGET_POSITION;
        }
        if self.has(aim::INITIAL_SPEED) {
            to.initial_speed = self.initial_speed;
            to.bits |= aim::INITIAL_SPEED;
        }
        if self.has(aim::INITIAL_PITCH) {
            to.initial_pitch = self.initial_pitch;
            to.bits |= aim::INITIAL_PITCH;
        }
        if self.has(aim::SOURCE_OBJECT) {
            to.source = self.source;
            to.source_height = self.source_height;
            to.source_scaled = self.source_scaled;
            to.source_offset = self.source_offset;
            to.bits = (to.bits & !aim::BASE_POSITION) | aim::SOURCE_OBJECT;
        } else if self.has(aim::BASE_POSITION) {
            to.base_position = self.base_position;
            to.bits = (to.bits & !aim::SOURCE_OBJECT) | aim::BASE_POSITION;
        }
        if self.has(aim::HOLD_FIRE_PAST_RANGE) {
            to.bits |= aim::HOLD_FIRE_PAST_RANGE;
        }
        if self.has(aim::MAX_RANGE) {
            to.max_range = self.max_range;
            to.bits |= aim::MAX_RANGE;
        }
        if self.has(aim::TOWARD_TARGET) {
            to.bits |= aim::TOWARD_TARGET;
        }
        if self.has(aim::ALWAYS_TOWARD_TARGET) {
            to.bits |= aim::ALWAYS_TOWARD_TARGET;
        }
        if self.has(aim::DETONATION_CHANCE) {
            to.detonation_chance = self.detonation_chance;
            to.bits |= aim::DETONATION_CHANCE;
        }
        true
    }
}

impl Pack for MissileTargetingParameters {
    /// The bit word; the target object (id, height, scaled, offset) or else the target place;
    /// the source object or else the base place; then the starting speed, pitch, range and
    /// detonation chance, each when set.
    // D2-15792
    fn pack(&self, w: &mut Writer) {
        w.u32(self.bits);
        if self.has(aim::TARGET_OBJECT) {
            w.u64(self.target);
            w.f32(self.target_height);
            w.bool32(self.target_scaled);
            write_vec3(w, self.target_offset);
        } else if self.has(aim::TARGET_POSITION) {
            write_position(w, &self.target_position);
        }
        if self.has(aim::SOURCE_OBJECT) {
            w.u64(self.source);
            w.f32(self.source_height);
            w.bool32(self.source_scaled);
            write_vec3(w, self.source_offset);
        } else if self.has(aim::BASE_POSITION) {
            write_position(w, &self.base_position);
        }
        if self.has(aim::INITIAL_SPEED) {
            w.f32(self.initial_speed);
        }
        if self.has(aim::INITIAL_PITCH) {
            w.f32(self.initial_pitch);
        }
        if self.has(aim::MAX_RANGE) {
            w.f32(self.max_range);
        }
        if self.has(aim::DETONATION_CHANCE) {
            w.f32(self.detonation_chance);
        }
    }

    fn unpack(r: &mut Reader<'_>, _ctx: &Context) -> Result<Self> {
        let mut t = Self {
            bits: r.u32()?,
            ..Self::default()
        };
        if t.has(aim::TARGET_OBJECT) {
            t.target = r.u64()?;
            t.target_height = r.f32()?;
            t.target_scaled = r.bool32()?;
            t.target_offset = read_vec3(r)?;
        } else if t.has(aim::TARGET_POSITION) {
            t.target_position = read_position(r)?;
        }
        if t.has(aim::SOURCE_OBJECT) {
            t.source = r.u64()?;
            t.source_height = r.f32()?;
            t.source_scaled = r.bool32()?;
            t.source_offset = read_vec3(r)?;
        } else if t.has(aim::BASE_POSITION) {
            t.base_position = read_position(r)?;
        }
        if t.has(aim::INITIAL_SPEED) {
            t.initial_speed = r.f32()?;
        }
        if t.has(aim::INITIAL_PITCH) {
            t.initial_pitch = r.f32()?;
        }
        if t.has(aim::MAX_RANGE) {
            t.max_range = r.f32()?;
        }
        if t.has(aim::DETONATION_CHANCE) {
            t.detonation_chance = r.f32()?;
        }
        Ok(t)
    }
}
