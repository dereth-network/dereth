//! `PhysicsDesc` — an object's physical behaviour, and the two-timestamp pack every WorldObjects event
//! carries.
//!
//! Source: `docs/networking/messages/02-world-objects.md` §3.2, transcribing the
//! physics description's own unpack. The generated physics-description catalogue matches it,
//! verified.

use super::space::{PositionWire, Vec3};
use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use dereth_primitives::ObjectId;

/// The `bitfield` gates. Note that the **wire order is not bit order**; see the profile reader.
pub mod flags {
    pub const SETUP: u32 = 0x0000_0001;
    pub const MTABLE: u32 = 0x0000_0002;
    pub const VELOCITY: u32 = 0x0000_0004;
    pub const ACCELERATION: u32 = 0x0000_0008;
    pub const OMEGA: u32 = 0x0000_0010;
    pub const PARENT: u32 = 0x0000_0020;
    pub const CHILDREN: u32 = 0x0000_0040;
    pub const OBJSCALE: u32 = 0x0000_0080;
    pub const FRICTION: u32 = 0x0000_0100;
    pub const ELASTICITY: u32 = 0x0000_0200;
    pub const TIMESTAMPS: u32 = 0x0000_0400;
    pub const STABLE: u32 = 0x0000_0800;
    pub const PETABLE: u32 = 0x0000_1000;
    pub const DEFAULT_SCRIPT: u32 = 0x0000_2000;
    pub const DEFAULT_SCRIPT_INTENSITY: u32 = 0x0000_4000;
    pub const POSITION: u32 = 0x0000_8000;
    pub const MOVEMENT: u32 = 0x0001_0000;
    pub const ANIMFRAME: u32 = 0x0002_0000;
    pub const TRANSLUCENCY: u32 = 0x0004_0000;
}

/// The nine `uint16` timestamps that close every `PhysicsDesc`.
///
/// They map one-to-one onto physics timestamps and the object's update-time array. `INSTANCE` is the
/// one the WorldObjects gate reads first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhysicsTimestamps {
    pub position: u16,
    pub movement: u16,
    pub state: u16,
    pub vector: u16,
    pub teleport: u16,
    pub server_controlled_move: u16,
    pub force_position: u16,
    pub objdesc: u16,
    pub instance: u16,
}

impl PhysicsTimestamps {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            position: r.u16()?,
            movement: r.u16()?,
            state: r.u16()?,
            vector: r.u16()?,
            teleport: r.u16()?,
            server_controlled_move: r.u16()?,
            force_position: r.u16()?,
            objdesc: r.u16()?,
            instance: r.u16()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        for v in [
            self.position,
            self.movement,
            self.state,
            self.vector,
            self.teleport,
            self.server_controlled_move,
            self.force_position,
            self.objdesc,
            self.instance,
        ] {
            w.u16(v);
        }
    }
}

/// The physics timestamp pack: two `uint16`s then align 4.
///
/// **`ts1` is always the instance sequence** and **`ts2` is the event's own sequence**. Every
/// WorldObjects event carrying this pack runs the same three-way instance gate on `ts1` before looking
/// at anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhysicsEventStamp {
    pub instance: u16,
    pub event: u16,
}

impl PhysicsEventStamp {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let p = Self {
            instance: r.u16()?,
            event: r.u16()?,
        };
        r.align4()?;
        Ok(p)
    }

    pub fn write(&self, w: &mut Writer) {
        w.u16(self.instance);
        w.u16(self.event);
        w.align4();
    }
}

/// One entry of the `0x00000040` child list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChildLink {
    pub child_id: ObjectId,
    pub location_id: u32,
}

/// The physics description.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PhysicsDesc {
    pub bitfield: u32,
    /// `PhysicsState`; also the whole payload of `0xF74B Item_SetState`.
    pub state: u32,
    /// `0x00010000`: the raw movement buffer and its autonomy flag.
    pub movement: Option<(Vec<u8>, u32)>,
    /// `0x00020000`: mutually exclusive with `movement`.
    pub animframe_id: Option<u32>,
    pub position: Option<PositionWire>,
    pub mtable_id: Option<u32>,
    pub stable_id: Option<u32>,
    pub phstable_id: Option<u32>,
    pub setup_id: Option<u32>,
    pub parent: Option<(ObjectId, u32)>,
    pub children: Option<Vec<ChildLink>>,
    pub object_scale: Option<f32>,
    pub friction: Option<f32>,
    pub elasticity: Option<f32>,
    pub translucency: Option<f32>,
    pub velocity: Option<Vec3>,
    pub acceleration: Option<Vec3>,
    pub omega: Option<Vec3>,
    pub default_script: Option<u32>,
    pub default_script_intensity: Option<f32>,
    pub timestamps: PhysicsTimestamps,
}

impl PhysicsDesc {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let bitfield = r.u32()?;
        let state = r.u32()?;
        let mut d = Self {
            bitfield,
            state,
            ..Self::default()
        };
        let has = |b: u32| bitfield & b != 0;

        // 1 / 1': the movement buffer and the animation frame are mutually exclusive.
        if has(flags::MOVEMENT) {
            let len = r.u32()? as usize;
            if len > 0 {
                let buf = r.bytes(len)?.to_vec();
                let autonomous = r.u32()?;
                d.movement = Some((buf, autonomous));
            } else {
                // Length zero: the client reads no buffer and no autonomy flag.
                d.movement = Some((Vec::new(), 0));
            }
        } else if has(flags::ANIMFRAME) {
            d.animframe_id = Some(r.u32()?);
        }
        if has(flags::POSITION) {
            d.position = Some(PositionWire::read(r)?);
        }
        // The DataIDs here are plain 32-bit, not packed.
        if has(flags::MTABLE) {
            d.mtable_id = Some(r.u32()?);
        }
        if has(flags::STABLE) {
            d.stable_id = Some(r.u32()?);
        }
        if has(flags::PETABLE) {
            d.phstable_id = Some(r.u32()?);
        }
        if has(flags::SETUP) {
            d.setup_id = Some(r.u32()?);
        }
        if has(flags::PARENT) {
            d.parent = Some((ObjectId(r.u32()?), r.u32()?));
        }
        if has(flags::CHILDREN) {
            let n = r.u32()? as usize;
            if n * 8 > r.remaining() {
                return Err(MessageError::LengthOverrun {
                    field: "physics-description child count",
                    len: n,
                    available: r.remaining(),
                });
            }
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(ChildLink {
                    child_id: ObjectId(r.u32()?),
                    location_id: r.u32()?,
                });
            }
            d.children = Some(v);
        }
        if has(flags::OBJSCALE) {
            d.object_scale = Some(r.f32()?);
        }
        if has(flags::FRICTION) {
            d.friction = Some(r.f32()?);
        }
        if has(flags::ELASTICITY) {
            d.elasticity = Some(r.f32()?);
        }
        if has(flags::TRANSLUCENCY) {
            d.translucency = Some(r.f32()?);
        }
        if has(flags::VELOCITY) {
            d.velocity = Some(Vec3::read(r)?);
        }
        if has(flags::ACCELERATION) {
            d.acceleration = Some(Vec3::read(r)?);
        }
        if has(flags::OMEGA) {
            d.omega = Some(Vec3::read(r)?);
        }
        if has(flags::DEFAULT_SCRIPT) {
            d.default_script = Some(r.u32()?);
        }
        if has(flags::DEFAULT_SCRIPT_INTENSITY) {
            d.default_script_intensity = Some(r.f32()?);
        }
        d.timestamps = PhysicsTimestamps::read(r)?;
        r.align4()?;
        Ok(d)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.bitfield);
        w.u32(self.state);
        if let Some((buf, autonomous)) = &self.movement {
            let n = u32::try_from(buf.len()).map_err(|_| MessageError::Unencodable {
                field: "physics-description buffer length",
                reason: "movement buffer longer than 4 GiB",
            })?;
            w.u32(n);
            if n > 0 {
                w.bytes(buf);
                w.u32(*autonomous);
            }
        } else if let Some(f) = self.animframe_id {
            w.u32(f);
        }
        if let Some(p) = &self.position {
            p.write(w);
        }
        for v in [
            self.mtable_id,
            self.stable_id,
            self.phstable_id,
            self.setup_id,
        ]
        .into_iter()
        .flatten()
        {
            w.u32(v);
        }
        if let Some((p, loc)) = self.parent {
            w.u32(p.0);
            w.u32(loc);
        }
        if let Some(children) = &self.children {
            let n = u32::try_from(children.len()).map_err(|_| MessageError::Unencodable {
                field: "physics-description child count",
                reason: "more than 4 G children",
            })?;
            w.u32(n);
            for c in children {
                w.u32(c.child_id.0);
                w.u32(c.location_id);
            }
        }
        for f in [
            self.object_scale,
            self.friction,
            self.elasticity,
            self.translucency,
        ]
        .into_iter()
        .flatten()
        {
            w.f32(f);
        }
        for v in [self.velocity, self.acceleration, self.omega]
            .into_iter()
            .flatten()
        {
            v.write(w);
        }
        if let Some(s) = self.default_script {
            w.u32(s);
        }
        if let Some(i) = self.default_script_intensity {
            w.f32(i);
        }
        self.timestamps.write(w);
        w.align4();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the physics description's 19-step wire order
    /// (`docs/networking/messages/02-world-objects.md` §3.2).
    #[test]
    fn physicsdesc_round_trips_with_position_setup_and_timestamps() {
        let d = PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP | flags::OBJSCALE,
            state: 0x0000_0400,
            position: Some(PositionWire {
                objcell_id: 0x00A9_0125,
                ..PositionWire::default()
            }),
            setup_id: Some(0x0200_0001),
            object_scale: Some(1.5),
            timestamps: PhysicsTimestamps {
                instance: 3,
                position: 7,
                ..Default::default()
            },
            ..PhysicsDesc::default()
        };
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(PhysicsDesc::read(&mut r).unwrap(), d);
        r.expect_exhausted().unwrap();
    }

    /// The nine timestamps close every descriptor and are 18 bytes, so the trailing align to 4 is
    /// always two bytes when the rest is dword-aligned. That pad is the easiest thing to lose.
    #[test]
    fn the_nine_timestamps_are_followed_by_a_pad() {
        let d = PhysicsDesc::default();
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        // 4 bitfield + 4 state + 18 timestamps = 26, padded to 28.
        assert_eq!(w.len(), 28);
    }

    /// Oracle: `docs/networking/messages/02-world-objects.md` §2 and §4 — the timestamp pack is
    /// two `uint16`s then align 4, and `ts1` is always the instance sequence.
    #[test]
    fn timestamp_pack_is_two_shorts_then_a_pad() {
        let p = PhysicsEventStamp {
            instance: 0x1234,
            event: 0x5678,
        };
        let mut w = Writer::new();
        p.write(&mut w);
        assert_eq!(w.as_slice(), &[0x34, 0x12, 0x78, 0x56]);
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(PhysicsEventStamp::read(&mut r).unwrap(), p);
        r.expect_exhausted().unwrap();
    }
}
