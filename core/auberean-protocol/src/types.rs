//! The structures messages share: object ids with stamps, placements, how an object looks
//! ([`VisualDesc`]), how it moves and is placed ([`PhysicsDesc`]), what it is ([`WeenieDesc`]),
//! behaviors, movement orders, combat stories, quality tables and character identities.
//!
//! Most structures open with a flags word saying which fields follow. Each type here keeps that
//! word as sent and reads and writes exactly the fields it names, in the sender's order; a field
//! whose flag is clear holds its default. So a decoded value encodes back to the same bytes, and a
//! value built here sets the flag of every field it fills.

use alloc::vec::Vec;

use auberean_primitives::{Frame, Quat, Vec3};

use crate::table::Table;
use crate::text::{NarrowString, StringInfo};
use crate::wire::{read_list, write_list, Field, Reader, Result, Writer};

impl Field for Vec3 {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.vec3()
    }
    fn write(&self, w: &mut Writer) {
        w.vec3(*self);
    }
}

/// An object named with its stamps: the 64-bit instance id, the instance stamp, and a second
/// stamp whose meaning depends on the message (visual ordering or position).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ObjectStamp {
    pub id: u64,
    pub instance_stamp: u16,
    pub other_stamp: u16,
}

impl Field for ObjectStamp {
    // D2-03939
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            id: r.u64()?,
            instance_stamp: r.u16()?,
            other_stamp: r.u16()?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.id);
        w.u16(self.instance_stamp);
        w.u16(self.other_stamp);
    }
}

/// A place: a cell and a frame in its landblock's space (origin, then the rotation scalar
/// first).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    pub cell: u32,
    pub frame: Frame,
}

impl Field for Placement {
    // D2-13186, D2-13167
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let cell = r.u32()?;
        let origin = r.vec3()?;
        let rotation = Quat {
            w: r.f32()?,
            x: r.f32()?,
            y: r.f32()?,
            z: r.f32()?,
        };
        Ok(Self {
            cell,
            frame: Frame { origin, rotation },
        })
    }
    // D2-13185, D2-13166
    fn write(&self, w: &mut Writer) {
        w.u32(self.cell);
        w.vec3(self.frame.origin);
        let q = self.frame.rotation;
        w.f32(q.w);
        w.f32(q.x);
        w.f32(q.y);
        w.f32(q.z);
    }
}

/// A table of numbers by key: appearance strengths, physique values.
pub type FloatTable = Table<f32>;

fn has(bits: u32, bit: u32) -> bool {
    bits & bit != 0
}

/// How an object plays a behavior: which, at what speed, aimed at what.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BehaviorParams {
    pub bits: u32,
    pub behavior: u32,
    pub time_scale: f32,
    pub mode: u32,
    pub hold_cycles: u32,
    pub fx_script: u32,
    pub fx: u32,
    pub target: u64,
    pub camera_target: u64,
    pub camera_behavior: u32,
    pub clone_visual: u32,
    pub clone_appearance: u32,
    pub clone_strengths: FloatTable,
    pub impulse: Vec3,
    /// One word for each of [`BehaviorParams::WORD_BITS`] that is set, in that order (unset
    /// ones hold zero).
    pub words: [u32; 13],
    pub context: u32,
}

impl BehaviorParams {
    pub const BEHAVIOR: u32 = 0x1;
    pub const MODE: u32 = 0x2;
    pub const TIME_SCALE: u32 = 0x4;
    pub const HOLD_CYCLES: u32 = 0x10;
    pub const FX_SCRIPT: u32 = 0x1000;
    pub const TARGET: u32 = 0x2000;
    pub const IMPULSE: u32 = 0x4000;
    pub const CLONE: u32 = 0x1_0000;
    pub const FX: u32 = 0x10_0000;
    pub const CAMERA: u32 = 0x20_0000;
    pub const CONTEXT: u32 = 0x40_0000;
    /// The flags each followed by one more word, in the order the words come.
    pub const WORD_BITS: [u32; 13] = [
        0x8, 0x20, 0x80, 0x40, 0x4_0000, 0x8_0000, 0x100, 0x400, 0x800, 0x200, 0x2_0000, 0x8000,
        0x80_0000,
    ];

    /// Play one behavior, nothing else.
    #[must_use]
    pub fn behavior(behavior: u32) -> Self {
        Self {
            bits: Self::BEHAVIOR,
            behavior,
            ..Self::default()
        }
    }
}

impl Field for BehaviorParams {
    // D2-14911, D2-04073
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let bits = r.u32()?;
        let mut p = Self {
            bits,
            ..Self::default()
        };
        if has(bits, Self::BEHAVIOR) {
            p.behavior = r.u32()?;
        }
        if has(bits, Self::TIME_SCALE) {
            p.time_scale = r.f32()?;
        }
        if has(bits, Self::MODE) {
            p.mode = r.u32()?;
        }
        if has(bits, Self::HOLD_CYCLES) {
            p.hold_cycles = r.u32()?;
        }
        if has(bits, Self::FX_SCRIPT) {
            p.fx_script = r.u32()?;
        }
        if has(bits, Self::FX) {
            p.fx = r.u32()?;
        }
        if has(bits, Self::TARGET) {
            p.target = r.u64()?;
        }
        if has(bits, Self::CAMERA) {
            p.camera_target = r.u64()?;
            p.camera_behavior = r.u32()?;
        }
        if has(bits, Self::CLONE) {
            p.clone_visual = r.u32()?;
            p.clone_appearance = r.u32()?;
            p.clone_strengths = FloatTable::read(r)?;
        }
        if has(bits, Self::IMPULSE) {
            p.impulse = r.vec3()?;
        }
        for (i, bit) in Self::WORD_BITS.iter().enumerate() {
            if has(bits, *bit) {
                p.words[i] = r.u32()?;
            }
        }
        if has(bits, Self::CONTEXT) {
            p.context = r.u32()?;
        }
        Ok(p)
    }

    fn write(&self, w: &mut Writer) {
        let bits = self.bits;
        w.u32(bits);
        if has(bits, Self::BEHAVIOR) {
            w.u32(self.behavior);
        }
        if has(bits, Self::TIME_SCALE) {
            w.f32(self.time_scale);
        }
        if has(bits, Self::MODE) {
            w.u32(self.mode);
        }
        if has(bits, Self::HOLD_CYCLES) {
            w.u32(self.hold_cycles);
        }
        if has(bits, Self::FX_SCRIPT) {
            w.u32(self.fx_script);
        }
        if has(bits, Self::FX) {
            w.u32(self.fx);
        }
        if has(bits, Self::TARGET) {
            w.u64(self.target);
        }
        if has(bits, Self::CAMERA) {
            w.u64(self.camera_target);
            w.u32(self.camera_behavior);
        }
        if has(bits, Self::CLONE) {
            w.u32(self.clone_visual);
            w.u32(self.clone_appearance);
            self.clone_strengths.write(w);
        }
        if has(bits, Self::IMPULSE) {
            w.vec3(self.impulse);
        }
        for (i, bit) in Self::WORD_BITS.iter().enumerate() {
            if has(bits, *bit) {
                w.u32(self.words[i]);
            }
        }
        if has(bits, Self::CONTEXT) {
            w.u32(self.context);
        }
    }
}

/// What a movement aims at: an object, or a place.
#[derive(Clone, Debug, PartialEq)]
pub enum MoveTarget {
    Object(u64),
    Place(Placement),
}

/// A movement order: speeds and distances, the target, waypoints.
#[derive(Clone, Debug, PartialEq)]
pub struct MovementParams {
    pub flags: u32,
    pub desired_velocity: f32,
    pub fail_distance: f32,
    pub desired_distance: f32,
    pub max_success_distance: f32,
    pub min_success_distance: f32,
    pub target: MoveTarget,
    pub offset: Vec3,
    pub offset_id: u32,
    pub waypoints: Vec<Placement>,
    pub context: u32,
}

impl MovementParams {
    pub const MAX_SUCCESS: u32 = 0x1;
    pub const MIN_SUCCESS: u32 = 0x4;
    pub const TO_OBJECT: u32 = 0x20;
    pub const OFFSET: u32 = 0x100;
    pub const OFFSET_ID: u32 = 0x200;
}

impl Field for MovementParams {
    // D2-15513, D2-04065
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let flags = r.u32()?;
        let (mut dv, mut fd, mut dd, mut maxs, mut mins) = (0.0, 0.0, 0.0, 0.0, 0.0);
        if flags & (Self::MAX_SUCCESS | Self::MIN_SUCCESS) != 0 {
            dv = r.f32()?;
            fd = r.f32()?;
            dd = r.f32()?;
        }
        if has(flags, Self::MAX_SUCCESS) {
            maxs = r.f32()?;
        }
        if has(flags, Self::MIN_SUCCESS) {
            mins = r.f32()?;
        }
        let target = if has(flags, Self::TO_OBJECT) {
            MoveTarget::Object(r.u64()?)
        } else {
            MoveTarget::Place(Placement::read(r)?)
        };
        let offset = if has(flags, Self::OFFSET) {
            r.vec3()?
        } else {
            Vec3::ZERO
        };
        let offset_id = if has(flags, Self::OFFSET_ID) {
            r.u32()?
        } else {
            0
        };
        let waypoints = read_list(r)?;
        let context = r.u32()?;
        Ok(Self {
            flags,
            desired_velocity: dv,
            fail_distance: fd,
            desired_distance: dd,
            max_success_distance: maxs,
            min_success_distance: mins,
            target,
            offset,
            offset_id,
            waypoints,
            context,
        })
    }

    // D2-04063, D2-04064
    fn write(&self, w: &mut Writer) {
        let flags = self.flags;
        w.u32(flags);
        if flags & (Self::MAX_SUCCESS | Self::MIN_SUCCESS) != 0 {
            w.f32(self.desired_velocity);
            w.f32(self.fail_distance);
            w.f32(self.desired_distance);
        }
        if has(flags, Self::MAX_SUCCESS) {
            w.f32(self.max_success_distance);
        }
        if has(flags, Self::MIN_SUCCESS) {
            w.f32(self.min_success_distance);
        }
        match &self.target {
            MoveTarget::Object(id) if has(flags, Self::TO_OBJECT) => w.u64(*id),
            MoveTarget::Place(p) if !has(flags, Self::TO_OBJECT) => p.write(w),
            MoveTarget::Object(_) => Placement::default().write(w),
            MoveTarget::Place(_) => w.u64(0),
        }
        if has(flags, Self::OFFSET) {
            w.vec3(self.offset);
        }
        if has(flags, Self::OFFSET_ID) {
            w.u32(self.offset_id);
        }
        write_list(w, &self.waypoints);
        w.u32(self.context);
    }
}

/// One step of a combat story: what struck what, and with what result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoryHook {
    /// The kind is the low four bits; the rest say which fields follow.
    pub bits: u32,
    pub target: u64,
    pub weapon: u64,
    /// One number for each of [`StoryHook::NUMBER_BITS`] that is set (attack result, animation
    /// hook, attack hook, target health change, attacker health change, the two player-killer
    /// damage changes).
    pub numbers: [i32; 7],
}

impl StoryHook {
    pub const TARGET: u32 = 0x100;
    pub const WEAPON: u32 = 0x20;
    pub const NUMBER_BITS: [u32; 7] = [0x80, 0x200, 0x400, 0x800, 0x1000, 0x2000, 0x4000];
}

impl Field for StoryHook {
    // D2-15862
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let bits = r.u32()?;
        let mut h = Self {
            bits,
            ..Self::default()
        };
        if has(bits, Self::TARGET) {
            h.target = r.u64()?;
        }
        if has(bits, Self::WEAPON) {
            h.weapon = r.u64()?;
        }
        for (i, bit) in Self::NUMBER_BITS.iter().enumerate() {
            if has(bits, *bit) {
                h.numbers[i] = r.i32()?;
            }
        }
        Ok(h)
    }

    fn write(&self, w: &mut Writer) {
        w.u32(self.bits);
        if has(self.bits, Self::TARGET) {
            w.u64(self.target);
        }
        if has(self.bits, Self::WEAPON) {
            w.u64(self.weapon);
        }
        for (i, bit) in Self::NUMBER_BITS.iter().enumerate() {
            if has(self.bits, *bit) {
                w.i32(self.numbers[i]);
            }
        }
    }
}

/// A combat exchange as observers see it: the attacker, how it attacked, and each hook.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhysicsStory {
    pub bits: u32,
    pub attacker: u64,
    pub attacker_behavior: BehaviorParams,
    pub attack_context: u32,
    pub skill: u32,
    pub hooks: Vec<StoryHook>,
}

impl PhysicsStory {
    pub const ATTACKER: u32 = 0x2;
    pub const BEHAVIOR: u32 = 0x4;
    pub const CONTEXT: u32 = 0x8;
    pub const SKILL: u32 = 0x10;
}

impl Field for PhysicsStory {
    // D2-15880, D2-15876
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let bits = r.u32()?;
        let mut s = Self {
            bits,
            ..Self::default()
        };
        if has(bits, Self::ATTACKER) {
            s.attacker = r.u64()?;
        }
        if has(bits, Self::BEHAVIOR) {
            s.attacker_behavior = BehaviorParams::read(r)?;
        }
        if has(bits, Self::CONTEXT) {
            s.attack_context = r.u32()?;
        }
        if has(bits, Self::SKILL) {
            s.skill = r.u32()?;
        }
        s.hooks = read_list(r)?;
        Ok(s)
    }

    fn write(&self, w: &mut Writer) {
        w.u32(self.bits);
        if has(self.bits, Self::ATTACKER) {
            w.u64(self.attacker);
        }
        if has(self.bits, Self::BEHAVIOR) {
            self.attacker_behavior.write(w);
        }
        if has(self.bits, Self::CONTEXT) {
            w.u32(self.attack_context);
        }
        if has(self.bits, Self::SKILL) {
            w.u32(self.skill);
        }
        write_list(w, &self.hooks);
    }
}

/// How a group of an object's parts looks: which setup and animations it uses, and the
/// appearance strengths applied to it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartGroupDesc {
    pub bits: u32,
    pub key: u32,
    pub parent_key: u32,
    pub connection_point: u32,
    pub setup: u32,
    pub anim_map: u32,
    /// Per appearance key, the strength of each of its modifiers.
    pub appearance: Table<FloatTable>,
    pub fx_table: u32,
    pub startup_fx: FloatTable,
}

impl PartGroupDesc {
    pub const KEY: u32 = 0x1;
    pub const PARENT_KEY: u32 = 0x2;
    pub const CONNECTION_POINT: u32 = 0x4;
    pub const SETUP: u32 = 0x8;
    pub const ANIM_MAP: u32 = 0x10;
    pub const APPEARANCE: u32 = 0x20;
    pub const FX_TABLE: u32 = 0x40;
    pub const STARTUP_FX: u32 = 0x80;
    /// Effect overrides: no recorded message sets it, and its layout is not known.
    pub const FX_OVERRIDES: u32 = 0x100;
}

impl Field for PartGroupDesc {
    // D2-14041
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let bits = r.u32()?;
        let mut d = Self {
            bits,
            ..Self::default()
        };
        if has(bits, Self::KEY) {
            d.key = r.u32()?;
        }
        if has(bits, Self::PARENT_KEY) {
            d.parent_key = r.u32()?;
        }
        if has(bits, Self::CONNECTION_POINT) {
            d.connection_point = r.u32()?;
        }
        if has(bits, Self::SETUP) {
            d.setup = r.u32()?;
        }
        if has(bits, Self::ANIM_MAP) {
            d.anim_map = r.u32()?;
        }
        if has(bits, Self::APPEARANCE) {
            d.appearance = Table::read(r)?;
        }
        if has(bits, Self::FX_TABLE) {
            d.fx_table = r.u32()?;
        }
        if has(bits, Self::STARTUP_FX) {
            d.startup_fx = FloatTable::read(r)?;
        }
        if has(bits, Self::FX_OVERRIDES) {
            return Err(r.invalid("part group effect overrides"));
        }
        Ok(d)
    }

    fn write(&self, w: &mut Writer) {
        let bits = self.bits;
        w.u32(bits);
        for (bit, v) in [
            (Self::KEY, self.key),
            (Self::PARENT_KEY, self.parent_key),
            (Self::CONNECTION_POINT, self.connection_point),
            (Self::SETUP, self.setup),
            (Self::ANIM_MAP, self.anim_map),
        ] {
            if has(bits, bit) {
                w.u32(v);
            }
        }
        if has(bits, Self::APPEARANCE) {
            self.appearance.write(w);
        }
        if has(bits, Self::FX_TABLE) {
            w.u32(self.fx_table);
        }
        if has(bits, Self::STARTUP_FX) {
            self.startup_fx.write(w);
        }
    }
}

/// One layer of an icon: three ids.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IconLayer(pub u32, pub u32, pub u32);

impl Field for IconLayer {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(r.u32()?, r.u32()?, r.u32()?))
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.0);
        w.u32(self.1);
        w.u32(self.2);
    }
}

/// How an object looks: its visual description, scale, icon, and changes to its part groups.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisualDesc {
    pub bits: u32,
    pub did: u32,
    /// The visual description this one starts from.
    pub parent: u32,
    pub motion_interp: u32,
    pub behavior_table: u32,
    pub modes: u32,
    pub scale: Vec3,
    pub child_scale: f32,
    pub particle_scale: f32,
    pub icon: Vec<IconLayer>,
    /// Changes applied to the whole object.
    pub global: PartGroupDesc,
    /// Changes per part group, by key.
    pub part_groups: Table<PartGroupDesc>,
}

impl VisualDesc {
    pub const DID: u32 = 0x1;
    pub const PARENT: u32 = 0x2;
    pub const MOTION_INTERP: u32 = 0x4;
    pub const BEHAVIOR_TABLE: u32 = 0x8;
    pub const MODES: u32 = 0x10;
    pub const SCALE: u32 = 0x20;
    pub const CHILD_SCALE: u32 = 0x40;
    pub const ICON: u32 = 0x80;
    pub const GLOBAL: u32 = 0x100;
    pub const PART_GROUPS: u32 = 0x200;
    pub const PARTICLE_SCALE: u32 = 0x400;
}

impl Field for VisualDesc {
    // D2-13814, D2-04041
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let bits = r.u32()?;
        let mut d = Self {
            bits,
            ..Self::default()
        };
        if has(bits, Self::DID) {
            d.did = r.u32()?;
        }
        if has(bits, Self::PARENT) {
            d.parent = r.u32()?;
        }
        if has(bits, Self::MOTION_INTERP) {
            d.motion_interp = r.u32()?;
        }
        if has(bits, Self::BEHAVIOR_TABLE) {
            d.behavior_table = r.u32()?;
        }
        if has(bits, Self::MODES) {
            d.modes = r.u32()?;
        }
        if has(bits, Self::SCALE) {
            d.scale = r.vec3()?;
        }
        if has(bits, Self::CHILD_SCALE) {
            d.child_scale = r.f32()?;
        }
        if has(bits, Self::PARTICLE_SCALE) {
            d.particle_scale = r.f32()?;
        }
        if has(bits, Self::ICON) {
            d.icon = read_list(r)?;
        }
        if has(bits, Self::GLOBAL) {
            d.global = PartGroupDesc::read(r)?;
        }
        if has(bits, Self::PART_GROUPS) {
            d.part_groups = Table::read(r)?;
        }
        Ok(d)
    }

    fn write(&self, w: &mut Writer) {
        let bits = self.bits;
        w.u32(bits);
        for (bit, v) in [
            (Self::DID, self.did),
            (Self::PARENT, self.parent),
            (Self::MOTION_INTERP, self.motion_interp),
            (Self::BEHAVIOR_TABLE, self.behavior_table),
            (Self::MODES, self.modes),
        ] {
            if has(bits, bit) {
                w.u32(v);
            }
        }
        if has(bits, Self::SCALE) {
            w.vec3(self.scale);
        }
        if has(bits, Self::CHILD_SCALE) {
            w.f32(self.child_scale);
        }
        if has(bits, Self::PARTICLE_SCALE) {
            w.f32(self.particle_scale);
        }
        if has(bits, Self::ICON) {
            write_list(w, &self.icon);
        }
        if has(bits, Self::GLOBAL) {
            self.global.write(w);
        }
        if has(bits, Self::PART_GROUPS) {
            self.part_groups.write(w);
        }
    }
}

/// An object's parent link: the parent, where on it, and the link's stamp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParentLink {
    pub parent: u64,
    pub location: u32,
    pub stamp: u16,
}

/// An effect running on an object: which, its strength, and the object it aims at.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RunningFx {
    pub fx: u32,
    pub scalar: f32,
    pub target: u64,
}

impl Field for RunningFx {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            fx: r.u32()?,
            scalar: r.f32()?,
            target: r.u64()?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.fx);
        w.f32(self.scalar);
        w.u64(self.target);
    }
}

/// A slider setting: which slider, its value, how long it takes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SliderSetting {
    pub slider: u32,
    pub value: f32,
    pub time: f32,
}

impl Field for SliderSetting {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            slider: r.u32()?,
            value: r.f32()?,
            time: r.f32()?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.slider);
        w.f32(self.value);
        w.f32(self.time);
    }
}

/// How an object is placed and moving when it is created: its stamps, mode, behaviors,
/// position or parent, velocities, what it looks at, effects.
///
/// A field the bits leave out keeps the value a new description has: every scale (speed, jump and
/// the two head factors) 1, everything else 0 or empty. Its readers take each field as it stands,
/// whatever the bits say.
// D2-13877, D2-13878
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsDesc {
    pub bits: u32,
    pub instance_stamp: u16,
    pub visual_ordering_stamp: u16,
    // D2-13868
    pub mode: u32,
    // D2-13869
    pub behaviors: Vec<BehaviorParams>,
    // D2-13870
    pub sliders: Vec<SliderSetting>,
    pub anim_frame: u32,
    pub position: Placement,
    pub parent: ParentLink,
    pub orientation_id: u32,
    pub velocity: Vec3,
    pub external_acceleration: Vec3,
    pub velocity_scale: f32,
    pub jump_scale: f32,
    pub acceleration: Vec3,
    pub omega: Vec3,
    pub look_at: u64,
    pub head_x: f32,
    pub head_z: f32,
    pub target: u64,
    pub target_position: Placement,
    pub target_offset: Vec3,
    // D2-13871
    pub fx: Vec<RunningFx>,
    /// Four more stamps, sent last.
    pub stamps: [u16; 4],
}

impl Default for PhysicsDesc {
    // D2-13881
    fn default() -> Self {
        Self {
            bits: 0,
            instance_stamp: 0,
            visual_ordering_stamp: 0,
            mode: 0,
            behaviors: Vec::new(),
            sliders: Vec::new(),
            anim_frame: 0,
            position: Placement::default(),
            parent: ParentLink::default(),
            orientation_id: 0,
            velocity: Vec3::ZERO,
            external_acceleration: Vec3::ZERO,
            velocity_scale: 1.0,
            jump_scale: 1.0,
            acceleration: Vec3::ZERO,
            omega: Vec3::ZERO,
            look_at: 0,
            head_x: 1.0,
            head_z: 1.0,
            target: 0,
            target_position: Placement::default(),
            target_offset: Vec3::ZERO,
            fx: Vec::new(),
            stamps: [0; 4],
        }
    }
}

impl PhysicsDesc {
    /// One of the four stamps sent last (position, teleport, forced position, and the fourth),
    /// by its number.
    // D2-13864
    #[must_use]
    pub fn stamp(&self, which: usize) -> u16 {
        self.stamps[which]
    }

    /// Where the object is: its cell and frame (cell 0, nowhere, when the description has none).
    // D2-05659
    #[must_use]
    pub fn position(&self) -> Placement {
        self.position
    }

    /// The object it hangs from (0 for none).
    // D2-13865
    #[must_use]
    pub fn parent_id(&self) -> u64 {
        self.parent.parent
    }

    /// The instance stamp of the object it hangs from.
    // D2-13866
    #[must_use]
    pub fn parent_instance_stamp(&self) -> u16 {
        self.parent.stamp
    }

    /// Which of its parent's locations it hangs from.
    // D2-13867
    #[must_use]
    pub fn location_id(&self) -> u32 {
        self.parent.location
    }
    pub const BEHAVIORS: u32 = 0x1;
    pub const SLIDERS: u32 = 0x2;
    pub const VELOCITY: u32 = 0x4;
    pub const ACCELERATION: u32 = 0x8;
    pub const OMEGA: u32 = 0x10;
    pub const PARENT: u32 = 0x20;
    pub const MODE: u32 = 0x80;
    pub const ORIENTATION: u32 = 0x100;
    pub const FX: u32 = 0x200;
    pub const TARGET: u32 = 0x400;
    pub const TARGET_POSITION: u32 = 0x800;
    pub const TARGET_OFFSET: u32 = 0x1000;
    /// The offset from the target it aims at grows with the target's scale.
    pub const TARGET_SCALED: u32 = 0x4000;
    /// It flies as a missile now.
    pub const MISSILE_MOVING: u32 = 0x8_0000;
    /// It is lit as a missile.
    pub const MISSILE_ACTIVATED: u32 = 0x10_0000;
    pub const POSITION: u32 = 0x8000;
    pub const ANIM_FRAME: u32 = 0x2_0000;
    pub const EXTERNAL_ACCELERATION: u32 = 0x40_0000;
    pub const VELOCITY_SCALE: u32 = 0x80_0000;
    pub const JUMP_SCALE: u32 = 0x100_0000;
    pub const LOOK_AT: u32 = 0x200_0000;
    pub const HEAD_X: u32 = 0x400_0000;
    pub const HEAD_Z: u32 = 0x800_0000;
}

/// The most sliders an object is created with; a larger count is a corrupt message.
const MAX_SLIDERS: usize = 256;

impl Field for PhysicsDesc {
    // D2-13879, D2-06348, D2-13875, D2-13876
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let bits = r.u32()?;
        let mut d = Self {
            bits,
            instance_stamp: r.u16()?,
            visual_ordering_stamp: r.u16()?,
            ..Self::default()
        };
        if has(bits, Self::MODE) {
            d.mode = r.u32()?;
        }
        if has(bits, Self::BEHAVIORS) {
            d.behaviors = read_list(r)?;
        }
        if has(bits, Self::SLIDERS) {
            let at = r.pos();
            d.sliders = read_list(r)?;
            if d.sliders.len() > MAX_SLIDERS {
                return Err(crate::wire::DecodeError::Invalid {
                    offset: at,
                    what: "slider count",
                });
            }
        }
        if has(bits, Self::ANIM_FRAME) {
            d.anim_frame = r.u32()?;
        }
        if has(bits, Self::POSITION) {
            d.position = Placement::read(r)?;
        }
        if has(bits, Self::PARENT) {
            d.parent = ParentLink {
                parent: r.u64()?,
                location: r.u32()?,
                stamp: r.u16()?,
            };
            r.align()?;
        }
        if has(bits, Self::ORIENTATION) {
            d.orientation_id = r.u32()?;
        }
        if has(bits, Self::VELOCITY) {
            d.velocity = r.vec3()?;
        }
        if has(bits, Self::EXTERNAL_ACCELERATION) {
            d.external_acceleration = r.vec3()?;
        }
        if has(bits, Self::VELOCITY_SCALE) {
            d.velocity_scale = r.f32()?;
        }
        if has(bits, Self::JUMP_SCALE) {
            d.jump_scale = r.f32()?;
        }
        if has(bits, Self::ACCELERATION) {
            d.acceleration = r.vec3()?;
        }
        if has(bits, Self::OMEGA) {
            d.omega = r.vec3()?;
        }
        if has(bits, Self::LOOK_AT) {
            d.look_at = r.u64()?;
        }
        if has(bits, Self::HEAD_X) {
            d.head_x = r.f32()?;
        }
        if has(bits, Self::HEAD_Z) {
            d.head_z = r.f32()?;
        }
        if has(bits, Self::TARGET) {
            d.target = r.u64()?;
        }
        if has(bits, Self::TARGET_POSITION) {
            d.target_position = Placement::read(r)?;
        }
        if has(bits, Self::TARGET_OFFSET) {
            d.target_offset = r.vec3()?;
        }
        if has(bits, Self::FX) {
            d.fx = read_list(r)?;
        }
        d.stamps = [r.u16()?, r.u16()?, r.u16()?, r.u16()?];
        r.align()?;
        Ok(d)
    }

    fn write(&self, w: &mut Writer) {
        let bits = self.bits;
        w.u32(bits);
        w.u16(self.instance_stamp);
        w.u16(self.visual_ordering_stamp);
        if has(bits, Self::MODE) {
            w.u32(self.mode);
        }
        if has(bits, Self::BEHAVIORS) {
            write_list(w, &self.behaviors);
        }
        if has(bits, Self::SLIDERS) {
            write_list(w, &self.sliders);
        }
        if has(bits, Self::ANIM_FRAME) {
            w.u32(self.anim_frame);
        }
        if has(bits, Self::POSITION) {
            self.position.write(w);
        }
        if has(bits, Self::PARENT) {
            w.u64(self.parent.parent);
            w.u32(self.parent.location);
            w.u16(self.parent.stamp);
            w.align();
        }
        if has(bits, Self::ORIENTATION) {
            w.u32(self.orientation_id);
        }
        if has(bits, Self::VELOCITY) {
            w.vec3(self.velocity);
        }
        if has(bits, Self::EXTERNAL_ACCELERATION) {
            w.vec3(self.external_acceleration);
        }
        if has(bits, Self::VELOCITY_SCALE) {
            w.f32(self.velocity_scale);
        }
        if has(bits, Self::JUMP_SCALE) {
            w.f32(self.jump_scale);
        }
        if has(bits, Self::ACCELERATION) {
            w.vec3(self.acceleration);
        }
        if has(bits, Self::OMEGA) {
            w.vec3(self.omega);
        }
        if has(bits, Self::LOOK_AT) {
            w.u64(self.look_at);
        }
        if has(bits, Self::HEAD_X) {
            w.f32(self.head_x);
        }
        if has(bits, Self::HEAD_Z) {
            w.f32(self.head_z);
        }
        if has(bits, Self::TARGET) {
            w.u64(self.target);
        }
        if has(bits, Self::TARGET_POSITION) {
            self.target_position.write(w);
        }
        if has(bits, Self::TARGET_OFFSET) {
            w.vec3(self.target_offset);
        }
        if has(bits, Self::FX) {
            write_list(w, &self.fx);
        }
        for s in self.stamps {
            w.u16(s);
        }
        w.align();
    }
}

/// What an object is to the game: its package and entity, name, icon, owners and numbers.
///
/// A new description holds nothing (its mask is 0); its quantity is 1, its package all ones (none)
/// and every other field 0 or empty, which a field the mask leaves out keeps.
// D2-12013
#[derive(Clone, Debug, PartialEq)]
pub struct WeenieDesc {
    pub mask: u32,
    pub package_id: u32,
    pub entity_did: u32,
    pub bitfield: u32,
    pub name: StringInfo,
    pub plural_name: StringInfo,
    pub icon: u32,
    /// One object id for each of [`WeenieDesc::OBJECT_BITS`] that is set (container, wielder,
    /// monarch, originator, claimant, killer, pet summoner).
    pub objects: [u64; 7],
    pub scale: f32,
    /// One number for each of [`WeenieDesc::NUMBER_BITS`] that is set (quantity, value, faction,
    /// the two player-killer flags, the two physics types, movement and placement
    /// ethereality, current and maximum durability).
    pub numbers: [i32; 13],
}

impl Default for WeenieDesc {
    // D2-12014, D2-12068, D2-00743, D2-07981
    fn default() -> Self {
        let mut numbers = [0; 13];
        numbers[0] = 1;
        Self {
            mask: 0,
            package_id: u32::MAX,
            entity_did: 0,
            bitfield: 0,
            name: StringInfo::default(),
            plural_name: StringInfo::default(),
            icon: 0,
            objects: [0; 7],
            scale: 0.0,
            numbers,
        }
    }
}

impl WeenieDesc {
    /// Set the package: held when it is one (not all ones), else marked not held.
    // D2-12022
    pub fn set_package_id(&mut self, package: u32) {
        self.package_id = package;
        if package == u32::MAX {
            self.mask &= !Self::PACKAGE;
        } else {
            self.mask |= Self::PACKAGE;
        }
    }

    /// Set the entity: held when it is one (not 0), else marked not held.
    // D2-12024
    pub fn set_entity_did(&mut self, entity: u32) {
        self.entity_did = entity;
        if entity == 0 {
            self.mask &= !Self::ENTITY;
        } else {
            self.mask |= Self::ENTITY;
        }
    }

    /// Take every field of `other`: its mask, then its package and entity as `other` answers
    /// them (all ones and 0 when it does not hold them), so each is held only when it is one; its
    /// flag word, marked held when any of its four low flags is on; and the rest as they stand.
    // D2-12070, D2-12074
    pub fn assign(&mut self, other: &Self) {
        self.mask = other.mask;
        self.set_entity_did(if other.mask & Self::ENTITY != 0 {
            other.entity_did
        } else {
            0
        });
        self.set_package_id(if other.mask & Self::PACKAGE != 0 {
            other.package_id
        } else {
            u32::MAX
        });
        self.bitfield = other.bitfield;
        if other.bitfield & 0xF != 0 {
            self.mask |= Self::BITFIELD;
        }
        self.name = other.name.clone();
        self.plural_name = other.plural_name.clone();
        self.icon = other.icon;
        self.objects = other.objects;
        self.scale = other.scale;
        self.numbers = other.numbers;
    }

    /// A new description that has taken every field of this one ([`WeenieDesc::assign`]).
    // D2-12073, D2-12072, D2-00744, D2-07982
    #[must_use]
    pub fn copy(&self) -> Self {
        let mut d = Self::default();
        d.assign(self);
        d
    }

    pub const PACKAGE: u32 = 0x1;
    pub const BITFIELD: u32 = 0x2;
    pub const NAME: u32 = 0x4;
    pub const PLURAL_NAME: u32 = 0x8;
    pub const ICON: u32 = 0x10;
    pub const SCALE: u32 = 0x80;
    pub const ENTITY: u32 = 0x200_0000;
    pub const OBJECT_BITS: [u32; 7] = [
        0x20,
        0x40,
        0x100,
        0x200,
        0x2000_0000,
        0x4000_0000,
        0x400_0000,
    ];
    pub const NUMBER_BITS: [u32; 13] = [
        0x800,
        0x1000,
        0x2000,
        0x20_0000,
        0x40_0000,
        0x8000,
        0x1_0000,
        0x2_0000,
        0x4_0000,
        0x8_0000,
        0x10_0000,
        0x800_0000,
        0x1000_0000,
    ];
}

impl Field for WeenieDesc {
    // D2-12071
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let m = r.u32()?;
        let mut d = Self {
            mask: m,
            ..Self::default()
        };
        if has(m, Self::PACKAGE) {
            d.package_id = r.u32()?;
        }
        if has(m, Self::ENTITY) {
            d.entity_did = r.u32()?;
        }
        if has(m, Self::BITFIELD) {
            d.bitfield = r.u32()?;
        }
        if has(m, Self::NAME) {
            d.name = StringInfo::read(r)?;
        }
        if has(m, Self::PLURAL_NAME) {
            d.plural_name = StringInfo::read(r)?;
        }
        if has(m, Self::ICON) {
            d.icon = r.u32()?;
        }
        for (i, bit) in Self::OBJECT_BITS.iter().enumerate() {
            if has(m, *bit) {
                d.objects[i] = r.u64()?;
            }
        }
        if has(m, Self::SCALE) {
            d.scale = r.f32()?;
        }
        for (i, bit) in Self::NUMBER_BITS.iter().enumerate() {
            if has(m, *bit) {
                d.numbers[i] = r.i32()?;
            }
        }
        Ok(d)
    }

    fn write(&self, w: &mut Writer) {
        let m = self.mask;
        w.u32(m);
        if has(m, Self::PACKAGE) {
            w.u32(self.package_id);
        }
        if has(m, Self::ENTITY) {
            w.u32(self.entity_did);
        }
        if has(m, Self::BITFIELD) {
            w.u32(self.bitfield);
        }
        if has(m, Self::NAME) {
            self.name.write(w);
        }
        if has(m, Self::PLURAL_NAME) {
            self.plural_name.write(w);
        }
        if has(m, Self::ICON) {
            w.u32(self.icon);
        }
        for (i, bit) in Self::OBJECT_BITS.iter().enumerate() {
            if has(m, *bit) {
                w.u64(self.objects[i]);
            }
        }
        if has(m, Self::SCALE) {
            w.f32(self.scale);
        }
        for (i, bit) in Self::NUMBER_BITS.iter().enumerate() {
            if has(m, *bit) {
                w.i32(self.numbers[i]);
            }
        }
    }
}

/// An object's qualities: its description and its tables of numbers, flags, times, texts, ids,
/// positions and packages, each present when its bit of the header is set. A new one is empty.
// D2-07252
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Qualities {
    pub header: u32,
    pub did: u32,
    pub weenie: WeenieDesc,
    pub ints: Table<i32>,
    pub bools: Table<i32>,
    pub floats: Table<f32>,
    pub timestamps: Table<f64>,
    pub strings: Table<NarrowString>,
    pub data_ids: Table<u32>,
    pub instance_ids: Table<u64>,
    pub positions: Table<Placement>,
    pub string_infos: Table<StringInfo>,
    pub package_ids: Table<u32>,
    pub long_ints: Table<u64>,
}

impl Qualities {
    pub const WEENIE: u32 = 0x1;
    pub const INTS: u32 = 0x2;
    pub const BOOLS: u32 = 0x4;
    pub const FLOATS: u32 = 0x8;
    pub const TIMESTAMPS: u32 = 0x10;
    pub const DATA_IDS: u32 = 0x20;
    pub const INSTANCE_IDS: u32 = 0x40;
    pub const STRINGS: u32 = 0x80;
    pub const POSITIONS: u32 = 0x100;
    pub const STRING_INFOS: u32 = 0x200;
    pub const PACKAGE_IDS: u32 = 0x400;
    pub const LONG_INTS: u32 = 0x800;

    /// The header as the client works it out when it packs qualities: the description's bit
    /// when they hold one (here: when the header says so), and each table's bit when the table
    /// holds an entry.
    // D2-07263
    #[must_use]
    pub fn pack_header(&self) -> u32 {
        let mut h = self.header & Self::WEENIE;
        for (bit, held) in [
            (Self::INTS, !self.ints.is_empty()),
            (Self::LONG_INTS, !self.long_ints.is_empty()),
            (Self::BOOLS, !self.bools.is_empty()),
            (Self::FLOATS, !self.floats.is_empty()),
            (Self::TIMESTAMPS, !self.timestamps.is_empty()),
            (Self::STRINGS, !self.strings.is_empty()),
            (Self::DATA_IDS, !self.data_ids.is_empty()),
            (Self::INSTANCE_IDS, !self.instance_ids.is_empty()),
            (Self::POSITIONS, !self.positions.is_empty()),
            (Self::STRING_INFOS, !self.string_infos.is_empty()),
            (Self::PACKAGE_IDS, !self.package_ids.is_empty()),
        ] {
            if held {
                h |= bit;
            }
        }
        h
    }

    /// How many bytes the qualities pack to.
    // D2-07264
    #[must_use]
    pub fn pack_size(&self) -> usize {
        let mut w = Writer::default();
        self.write(&mut w);
        w.finish().len()
    }
}

impl Field for Qualities {
    // D2-07350, D2-07353
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let header = r.u32()?;
        if header & !0xFFF != 0 {
            return Err(r.invalid("qualities header"));
        }
        let mut q = Self {
            header,
            did: r.u32()?,
            ..Self::default()
        };
        let h = |bit| has(header, bit);
        if h(Self::WEENIE) {
            q.weenie = WeenieDesc::read(r)?;
        }
        if h(Self::INTS) {
            q.ints = Table::read(r)?;
        }
        if h(Self::BOOLS) {
            q.bools = Table::read(r)?;
        }
        if h(Self::FLOATS) {
            q.floats = Table::read(r)?;
        }
        if h(Self::TIMESTAMPS) {
            q.timestamps = Table::read(r)?;
        }
        // Texts come before data ids, out of bit order.
        if h(Self::STRINGS) {
            q.strings = Table::read(r)?;
        }
        if h(Self::DATA_IDS) {
            q.data_ids = Table::read(r)?;
        }
        if h(Self::INSTANCE_IDS) {
            q.instance_ids = Table::read(r)?;
        }
        if h(Self::POSITIONS) {
            q.positions = Table::read(r)?;
        }
        if h(Self::STRING_INFOS) {
            q.string_infos = Table::read(r)?;
        }
        if h(Self::PACKAGE_IDS) {
            q.package_ids = Table::read(r)?;
        }
        if h(Self::LONG_INTS) {
            q.long_ints = Table::read(r)?;
        }
        Ok(q)
    }

    // D2-07265
    fn write(&self, w: &mut Writer) {
        let header = self.header;
        let h = |bit| has(header, bit);
        w.u32(header);
        w.u32(self.did);
        if h(Self::WEENIE) {
            self.weenie.write(w);
        }
        if h(Self::INTS) {
            self.ints.write(w);
        }
        if h(Self::BOOLS) {
            self.bools.write(w);
        }
        if h(Self::FLOATS) {
            self.floats.write(w);
        }
        if h(Self::TIMESTAMPS) {
            self.timestamps.write(w);
        }
        if h(Self::STRINGS) {
            self.strings.write(w);
        }
        if h(Self::DATA_IDS) {
            self.data_ids.write(w);
        }
        if h(Self::INSTANCE_IDS) {
            self.instance_ids.write(w);
        }
        if h(Self::POSITIONS) {
            self.positions.write(w);
        }
        if h(Self::STRING_INFOS) {
            self.string_infos.write(w);
        }
        if h(Self::PACKAGE_IDS) {
            self.package_ids.write(w);
        }
        if h(Self::LONG_INTS) {
            self.long_ints.write(w);
        }
    }
}

/// A character as the character list shows it: its id, name, how long it stays greyed out after
/// a deletion, and how it looks. A new one names no character (the invalid id, 0); a copy is a copy
/// of every part.
// D2-13093, D2-13094, D2-00079
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharacterIdentity {
    pub id: u64,
    pub name: crate::text::WideString,
    /// Seconds until a deleted character is gone; 0 for one in play.
    pub seconds_greyed_out: u32,
    pub visual: VisualDesc,
}

impl Field for CharacterIdentity {
    // D2-13095
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            id: r.u64()?,
            name: crate::text::WideString::read(r)?,
            seconds_greyed_out: r.u32()?,
            visual: VisualDesc::read(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.id);
        self.name.write(w);
        w.u32(self.seconds_greyed_out);
        self.visual.write(w);
    }
}

/// One element of an iteration set: a value, or a run of consecutive values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IterationRun {
    /// One iteration (a value with bit 30 set stands for a negative one).
    One(i32),
    /// `count` consecutive iterations from `first`.
    Run { count: u32, first: u32 },
}

/// The set of iterations a client has of one data file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IterationSet {
    /// How many iterations the runs stand for.
    pub total: u32,
    pub runs: Vec<IterationRun>,
}

/// The most iterations a set stands for; more is a corrupt message.
const MAX_ITERATIONS: u32 = 100_000;

impl Field for IterationSet {
    // D2-14026
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let total = r.u32()?;
        if total > MAX_ITERATIONS {
            return Err(r.invalid("iteration count"));
        }
        let mut got: u64 = 0;
        let mut runs = Vec::new();
        while got < u64::from(total) {
            let e = r.i32()?;
            if e < 0 {
                let count = e.unsigned_abs();
                runs.push(IterationRun::Run {
                    count,
                    first: r.u32()?,
                });
                got += u64::from(count);
            } else {
                runs.push(IterationRun::One(e));
                got += 1;
            }
        }
        if got != u64::from(total) {
            return Err(r.invalid("iteration run"));
        }
        Ok(Self { total, runs })
    }

    fn write(&self, w: &mut Writer) {
        w.u32(self.total);
        for run in &self.runs {
            match *run {
                IterationRun::One(v) => w.i32(v),
                IterationRun::Run { count, first } => {
                    w.i32(0i32.wrapping_sub_unsigned(count));
                    w.u32(first);
                }
            }
        }
    }
}
