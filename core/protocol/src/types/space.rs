//! The wire forms of the spatial primitives.
//!
//! `dereth_primitives` has `Vec3`, `Quat`, `Frame` and `Position` already, but those are the *engine's*
//! types, carrying whatever representation physics wants. These are the *wire* forms, so that a
//! codec never has to guess whether a quaternion is stored `wxyz` or `xyzw` — on the wire it is
//! `w x y z`, verified against the client's frame unpacking.

use crate::archive::{Reader, Writer};
use crate::error::MessageError;

/// Three floats, in x/y/z order.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            x: r.f32()?,
            y: r.f32()?,
            z: r.f32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.x);
        w.f32(self.y);
        w.f32(self.z);
    }
}

impl From<Vec3> for dereth_primitives::Vec3 {
    fn from(v: Vec3) -> Self {
        Self {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}

// The reverse direction, and the same pair for `Quat`, `Frame` and `PositionWire`.
//
// These crates keep separate geometry types on purpose: the wire types pin the field order a codec
// must not guess (`w` first for a quaternion), while `dereth_primitives` is the engine's vocabulary. But the
// orphan rule means only this crate can write the conversions, so without them every consumer
// reimplements the same four helpers privately. Requested by the server rebuild after it had done
// exactly that.

impl From<dereth_primitives::Vec3> for Vec3 {
    fn from(v: dereth_primitives::Vec3) -> Self {
        Self {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}

impl From<Quat> for dereth_primitives::Quat {
    fn from(q: Quat) -> Self {
        Self {
            w: q.w,
            x: q.x,
            y: q.y,
            z: q.z,
        }
    }
}

impl From<dereth_primitives::Quat> for Quat {
    fn from(q: dereth_primitives::Quat) -> Self {
        Self {
            w: q.w,
            x: q.x,
            y: q.y,
            z: q.z,
        }
    }
}

impl From<Frame> for dereth_primitives::Frame {
    fn from(f: Frame) -> Self {
        Self {
            origin: f.origin.into(),
            rotation: f.orientation.into(),
        }
    }
}

impl From<dereth_primitives::Frame> for Frame {
    fn from(f: dereth_primitives::Frame) -> Self {
        Self {
            origin: f.origin.into(),
            orientation: f.rotation.into(),
        }
    }
}

impl From<PositionWire> for dereth_primitives::Position {
    fn from(p: PositionWire) -> Self {
        Self {
            cell: dereth_primitives::CellId(p.objcell_id),
            frame: p.frame.into(),
        }
    }
}

impl From<dereth_primitives::Position> for PositionWire {
    fn from(p: dereth_primitives::Position) -> Self {
        Self {
            objcell_id: p.cell.0,
            frame: p.frame.into(),
        }
    }
}

#[cfg(test)]
mod conv_tests {
    use super::*;

    // Oracle: the field orders documented in the recovered client math behavior and
    // docs/networking/messages/02-world-objects.md. A round trip cannot detect a consistent
    // mix-up, so each field is named explicitly.
    #[test]
    fn geometry_conversions_preserve_every_field_in_both_directions() {
        let w = Frame {
            origin: Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            orientation: Quat {
                w: 0.5,
                x: 0.6,
                y: 0.7,
                z: 0.8,
            },
        };
        let e: dereth_primitives::Frame = w.into();
        assert_eq!((e.origin.x, e.origin.y, e.origin.z), (1.0, 2.0, 3.0));
        assert_eq!(
            (e.rotation.w, e.rotation.x, e.rotation.y, e.rotation.z),
            (0.5, 0.6, 0.7, 0.8)
        );
        let back: Frame = e.into();
        assert_eq!(back, w);

        let p = PositionWire {
            objcell_id: 0xA9B4_0100,
            frame: w,
        };
        let ep: dereth_primitives::Position = p.into();
        assert_eq!(ep.cell.0, 0xA9B4_0100);
        let pb: PositionWire = ep.into();
        assert_eq!(pb, p);
    }
}

/// A quaternion, stored `w` first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Self {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }
}

impl Quat {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            w: r.f32()?,
            x: r.f32()?,
            y: r.f32()?,
            z: r.f32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.w);
        w.f32(self.x);
        w.f32(self.y);
        w.f32(self.z);
    }
}

/// `Frame`: an origin then an orientation. 28 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Frame {
    pub origin: Vec3,
    pub orientation: Quat,
}

impl Frame {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            origin: Vec3::read(r)?,
            orientation: Quat::read(r)?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        self.origin.write(w);
        self.orientation.write(w);
    }
}

/// A cell id followed by a frame: 32 bytes.
///
/// Named `PositionWire` rather than `Position` so it cannot be confused with
/// [`dereth_primitives::Position`], which is the engine's.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PositionWire {
    pub objcell_id: u32,
    pub frame: Frame,
}

impl PositionWire {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            objcell_id: r.u32()?,
            frame: Frame::read(r)?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.objcell_id);
        self.frame.write(w);
    }
}

/// `Origin`: a cell id and a position, with no orientation. 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Origin {
    pub objcell_id: u32,
    pub origin: Vec3,
}

impl Origin {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            objcell_id: r.u32()?,
            origin: Vec3::read(r)?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.objcell_id);
        self.origin.write(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: `docs/networking/messages/02-world-objects.md` §3.2, which gives a
    /// position as `uint32 objcell_id` + `Frame`, and
    /// the recovered quaternion wire layout for the `w x y z` order.
    #[test]
    fn a_position_is_thirty_two_bytes_with_w_first() {
        let p = PositionWire {
            objcell_id: 0x00A9_0125,
            frame: Frame {
                origin: Vec3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
                orientation: Quat {
                    w: 0.5,
                    x: 0.0,
                    y: 0.0,
                    z: -0.5,
                },
            },
        };
        let mut w = Writer::new();
        p.write(&mut w);
        assert_eq!(w.len(), 32);
        // The quaternion's first float after the origin is w.
        assert_eq!(&w.as_slice()[16..20], &0.5f32.to_le_bytes());
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(PositionWire::read(&mut r).unwrap(), p);
        r.expect_exhausted().unwrap();
    }
}
