//! The world connection's objects and movement: creating and destroying objects, positions,
//! behaviors, effects, movement orders, combat stories, parenting.
//!
//! Most of these are *object events*: the object the event is about, with its stamps
//! ([`ObjectEvent::sender`]), then one action.

use alloc::vec::Vec;

use auberean_primitives::Vec3;

use crate::types::{
    BehaviorParams, MovementParams, ObjectStamp, PhysicsDesc, PhysicsStory, Placement, VisualDesc,
    WeenieDesc,
};
use crate::wire::{read_list, write_list, Field, Reader, Result, Writer};

/// An event about one object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectEvent<A> {
    pub sender: ObjectStamp,
    pub action: A,
}

impl<A: Field> Field for ObjectEvent<A> {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            sender: ObjectStamp::read(r)?,
            action: A::read(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        self.sender.write(w);
        self.action.write(w);
    }
}

/// An action with nothing in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoAction;

impl Field for NoAction {
    // D2-04011, D2-04036, D2-04040
    fn read(_: &mut Reader<'_>) -> Result<Self> {
        Ok(Self)
    }
    fn write(&self, _: &mut Writer) {}
}

/// One word (a behavior, a mode, an effect...).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Word(pub u32);

impl Field for Word {
    // D2-04037, D2-04018, D2-04034, D2-04038, D2-04033, D2-04039, D2-04017, D2-04030
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let v = r.u32()?;
        r.align()?;
        Ok(Self(v))
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.0);
        w.align();
    }
}

/// One number (a speed or height scale).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scale(pub f32);

impl Field for Scale {
    // D2-04035, D2-04029, D2-04032, D2-04031
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let v = r.f32()?;
        r.align()?;
        Ok(Self(v))
    }
    fn write(&self, w: &mut Writer) {
        w.f32(self.0);
        w.align();
    }
}

/// An object to look at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Target(pub u64);

impl Field for Target {
    // D2-04024, D2-04043, D2-04042
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let v = r.u64()?;
        r.align()?;
        Ok(Self(v))
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.0);
        w.align();
    }
}

/// A slider moved to a value over a time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slide {
    pub slider: u32,
    pub value: f32,
    pub time: f32,
}

impl Field for Slide {
    // D2-04019
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            slider: Word::read(r)?.0,
            value: Scale::read(r)?.0,
            time: Scale::read(r)?.0,
        })
    }
    fn write(&self, w: &mut Writer) {
        Word(self.slider).write(w);
        Scale(self.value).write(w);
        Scale(self.time).write(w);
    }
}

/// A movement order and its stamp.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveOrder {
    pub params: MovementParams,
    pub stamp: u16,
}

impl Field for MoveOrder {
    // D2-04071
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let params = MovementParams::read(r)?;
        let stamp = r.u16()?;
        r.align()?;
        Ok(Self { params, stamp })
    }
    fn write(&self, w: &mut Writer) {
        self.params.write(w);
        w.u16(self.stamp);
        w.align();
    }
}

/// The object is attached to another, at a location and orientation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attach {
    pub parent: ObjectStamp,
    pub location: u32,
    pub orientation: u32,
}

impl Field for Attach {
    // D2-04026
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            parent: ObjectStamp::read(r)?,
            location: Word::read(r)?.0,
            orientation: Word::read(r)?.0,
        })
    }
    fn write(&self, w: &mut Writer) {
        self.parent.write(w);
        Word(self.location).write(w);
        Word(self.orientation).write(w);
    }
}

/// A push in a direction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Impulse {
    pub direction: Vec3,
    pub contact: u32,
}

impl Field for Impulse {
    // D2-04046
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let direction = Vec3::new(Scale::read(r)?.0, Scale::read(r)?.0, Scale::read(r)?.0);
        Ok(Self {
            direction,
            contact: Word::read(r)?.0,
        })
    }
    fn write(&self, w: &mut Writer) {
        for v in [self.direction.x, self.direction.y, self.direction.z] {
            Scale(v).write(w);
        }
        Word(self.contact).write(w);
    }
}

/// A push toward another object.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TargetedImpulse {
    pub target: u64,
    pub magnitude: f32,
    pub falloff: u32,
    pub vertical: f32,
}

impl Field for TargetedImpulse {
    // D2-04009
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            target: Target::read(r)?.0,
            magnitude: Scale::read(r)?.0,
            falloff: Word::read(r)?.0,
            vertical: Scale::read(r)?.0,
        })
    }
    fn write(&self, w: &mut Writer) {
        Target(self.target).write(w);
        Scale(self.magnitude).write(w);
        Word(self.falloff).write(w);
        Scale(self.vertical).write(w);
    }
}

/// A held behavior let go.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Release {
    pub behavior: u32,
    pub params: BehaviorParams,
}

impl Field for Release {
    // D2-04072
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            behavior: Word::read(r)?.0,
            params: BehaviorParams::read(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        Word(self.behavior).write(w);
        self.params.write(w);
    }
}

/// An effect at a strength.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fx {
    pub fx: u32,
    pub scalar: f32,
}

impl Field for Fx {
    // D2-04016
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            fx: Word::read(r)?.0,
            scalar: Scale::read(r)?.0,
        })
    }
    fn write(&self, w: &mut Writer) {
        Word(self.fx).write(w);
        Scale(self.scalar).write(w);
    }
}

/// An effect aimed at another object.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TargetedFx {
    pub fx: u32,
    pub scalar: f32,
    pub target: u64,
}

impl Field for TargetedFx {
    // D2-04021
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            fx: Word::read(r)?.0,
            scalar: Scale::read(r)?.0,
            target: Target::read(r)?.0,
        })
    }
    fn write(&self, w: &mut Writer) {
        Word(self.fx).write(w);
        Scale(self.scalar).write(w);
        Target(self.target).write(w);
    }
}

/// A change to the terrain under an object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainModifier {
    pub terrain: u32,
    pub modifier: u32,
}

impl Field for TerrainModifier {
    // D2-04010
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            terrain: Word::read(r)?.0,
            modifier: Word::read(r)?.0,
        })
    }
    fn write(&self, w: &mut Writer) {
        Word(self.terrain).write(w);
        Word(self.modifier).write(w);
    }
}

/// Where a head turns.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeadDirection {
    pub x: f32,
    pub y: f32,
}

impl Field for HeadDirection {
    // D2-04025, D2-04044
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            x: Scale::read(r)?.0,
            y: Scale::read(r)?.0,
        })
    }
    fn write(&self, w: &mut Writer) {
        Scale(self.x).write(w);
        Scale(self.y).write(w);
    }
}

/// The object leaves the world (its stamp).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Leave {
    pub stamp: u16,
}

impl Field for Leave {
    // D2-04023
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let stamp = r.u16()?;
        r.align()?;
        Ok(Self { stamp })
    }
    fn write(&self, w: &mut Writer) {
        w.u16(self.stamp);
        w.align();
    }
}

/// A missile's flight: where it starts, how fast, at what.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Missile {
    pub position: Placement,
    pub velocity: Vec3,
    pub target: u64,
    pub target_offset: Placement,
    pub target_velocity: Vec3,
    pub initialized: u32,
}

impl Field for Missile {
    // D2-04022
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            position: Placement::read(r)?,
            velocity: r.vec3()?,
            target: r.u64()?,
            target_offset: Placement::read(r)?,
            target_velocity: r.vec3()?,
            initialized: r.u32()?,
        })
    }
    fn write(&self, w: &mut Writer) {
        self.position.write(w);
        w.vec3(self.velocity);
        w.u64(self.target);
        self.target_offset.write(w);
        w.vec3(self.target_velocity);
        w.u32(self.initialized);
    }
}

/// A new object for the client: how it looks, how it is placed, what it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateObject {
    pub id: u64,
    pub visual: VisualDesc,
    pub physics: PhysicsDesc,
    pub weenie: WeenieDesc,
}

impl Field for CreateObject {
    // D2-00240
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.u64()?;
        r.align()?;
        Ok(Self {
            id,
            visual: VisualDesc::read(r)?,
            physics: PhysicsDesc::read(r)?,
            weenie: WeenieDesc::read(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.id);
        w.align();
        self.visual.write(w);
        self.physics.write(w);
        self.weenie.write(w);
    }
}

/// The client is to forget an object, or put it in a container.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjectOnly(pub ObjectStamp);

impl Field for ObjectOnly {
    // D2-04015, D2-04012
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(ObjectStamp::read(r)?))
    }
    fn write(&self, w: &mut Writer) {
        self.0.write(w);
    }
}

/// Effects shown to the player alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrivateFx(pub Vec<u32>);

impl Field for PrivateFx {
    // D2-04048
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(read_list(r)?))
    }
    fn write(&self, w: &mut Writer) {
        write_list(w, &self.0);
    }
}

/// Flag bits of a position.
pub mod position_flags {
    /// A jump: a velocity follows.
    pub const JUMP: u32 = 0x2;
    /// Touching the ground.
    pub const CONTACT: u32 = 0x4;
    /// An impulse: a velocity follows.
    pub const IMPULSE: u32 = 0x8;
}

/// Where an object is, as the server says: its place, heading and motion, flags and stamps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ObjectPosition {
    pub sender: ObjectStamp,
    /// The server's clock, in seconds.
    pub time: f64,
    pub cell: u32,
    pub origin: Vec3,
    /// The heading in the top byte (degrees times 255/360), the motion direction in the low 24
    /// bits.
    pub heading_motion: u32,
    pub flags: u32,
    pub position_stamp: u16,
    pub force_position_stamp: u16,
    pub teleport_stamp: u16,
    /// Sent when the flags hold a jump or an impulse.
    pub velocity: Vec3,
}

impl Field for ObjectPosition {
    // D2-04027, D2-04028, D2-15431
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let mut p = Self {
            sender: ObjectStamp::read(r)?,
            time: r.f64()?,
            cell: r.u32()?,
            origin: r.vec3()?,
            heading_motion: r.u32()?,
            flags: r.u32()?,
            position_stamp: r.u16()?,
            force_position_stamp: r.u16()?,
            teleport_stamp: r.u16()?,
            velocity: Vec3::ZERO,
        };
        let (jump, impulse) = (
            p.flags & position_flags::JUMP != 0,
            p.flags & position_flags::IMPULSE != 0,
        );
        if jump && impulse {
            return Err(r.invalid("position with a jump and an impulse"));
        }
        if jump || impulse {
            p.velocity = r.vec3()?;
        }
        r.align()?;
        Ok(p)
    }
    fn write(&self, w: &mut Writer) {
        self.sender.write(w);
        w.f64(self.time);
        w.u32(self.cell);
        w.vec3(self.origin);
        w.u32(self.heading_motion);
        w.u32(self.flags);
        w.u16(self.position_stamp);
        w.u16(self.force_position_stamp);
        w.u16(self.teleport_stamp);
        if self.flags & (position_flags::JUMP | position_flags::IMPULSE) != 0 {
            w.vec3(self.velocity);
        }
        w.align();
    }
}

/// Where the player is, as the client reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClientPosition {
    pub time: f64,
    pub cell: u32,
    pub origin: Vec3,
    pub heading_motion: u32,
    pub flags: u32,
    pub instance_stamp: u16,
    pub teleport_stamp: u16,
    pub force_position_stamp: u16,
    pub move_to_stamp: u16,
    /// Sent when the flags hold a jump.
    pub jump_velocity: Vec3,
}

impl Field for ClientPosition {
    // D2-04045, D2-15433
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let mut p = Self {
            time: r.f64()?,
            cell: r.u32()?,
            origin: r.vec3()?,
            heading_motion: r.u32()?,
            flags: r.u32()?,
            instance_stamp: r.u16()?,
            teleport_stamp: r.u16()?,
            force_position_stamp: r.u16()?,
            move_to_stamp: r.u16()?,
            jump_velocity: Vec3::ZERO,
        };
        if p.flags & position_flags::JUMP != 0 {
            p.jump_velocity = r.vec3()?;
        }
        r.align()?;
        Ok(p)
    }
    fn write(&self, w: &mut Writer) {
        w.f64(self.time);
        w.u32(self.cell);
        w.vec3(self.origin);
        w.u32(self.heading_motion);
        w.u32(self.flags);
        w.u16(self.instance_stamp);
        w.u16(self.teleport_stamp);
        w.u16(self.force_position_stamp);
        w.u16(self.move_to_stamp);
        if self.flags & position_flags::JUMP != 0 {
            w.vec3(self.jump_velocity);
        }
        w.align();
    }
}

/// Behaviors played on an object.
pub type Behavior = BehaviorParams;
/// A combat story.
pub type Story = PhysicsStory;
/// A changed look.
pub type Look = VisualDesc;
