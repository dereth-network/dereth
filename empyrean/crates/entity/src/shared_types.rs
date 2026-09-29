//! Conversions between ACE's entity types and the shared crates' types: dereth-primitives' data types
//! (`ObjectId`, `CellId`, `Vec3`, `Quat`, `Frame`, `Position`) and dereth-protocol's wire records
//! (`Vec3`, `Quat`, `Frame`, `PositionWire`, `Origin`).
//!
//! Nothing here is an ACE port and there are no ACE anchors. The conversions copy fields and do no
//! arithmetic: every calculation on a position stays in ACE's `Position` and the .NET `numerics`
//! it uses, bit for bit. `Vector3` and `Quaternion` live in `empyrean-common`, so they convert
//! through the free functions below rather than `From`.

use dereth_primitives as data;
use dereth_protocol::types::space as wire;

use crate::numerics::{Quaternion, Vector3};
use crate::{Frame, LandblockId, ObjectGuid, Position};

impl From<ObjectGuid> for data::ObjectId {
    fn from(guid: ObjectGuid) -> Self {
        data::ObjectId(guid.full())
    }
}

impl From<data::ObjectId> for ObjectGuid {
    fn from(id: data::ObjectId) -> Self {
        ObjectGuid::new(id.0)
    }
}

impl From<LandblockId> for data::CellId {
    fn from(id: LandblockId) -> Self {
        data::CellId(id.raw())
    }
}

impl From<data::CellId> for LandblockId {
    fn from(id: data::CellId) -> Self {
        LandblockId::new(id.0)
    }
}

/// A `Vector3` as the wire's `Vec3`.
#[must_use]
pub fn wire_vec3(v: Vector3) -> wire::Vec3 {
    wire::Vec3 {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

/// The wire's `Vec3` as a `Vector3`.
#[must_use]
pub fn vector3_of_wire(v: wire::Vec3) -> Vector3 {
    Vector3 {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

/// A `Quaternion` as the wire's `Quat` (which the wire orders w, x, y, z).
#[must_use]
pub fn wire_quat(q: Quaternion) -> wire::Quat {
    wire::Quat {
        w: q.w,
        x: q.x,
        y: q.y,
        z: q.z,
    }
}

/// The wire's `Quat` as a `Quaternion`.
#[must_use]
pub fn quaternion_of_wire(q: wire::Quat) -> Quaternion {
    Quaternion {
        x: q.x,
        y: q.y,
        z: q.z,
        w: q.w,
    }
}

/// A `Vector3` as dereth-primitives' `Vec3`.
#[must_use]
pub fn data_vec3(v: Vector3) -> data::Vec3 {
    data::Vec3 {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

/// dereth-primitives' `Vec3` as a `Vector3`.
#[must_use]
pub fn vector3_of_data(v: data::Vec3) -> Vector3 {
    Vector3 {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

/// A `Quaternion` as dereth-primitives' `Quat`.
#[must_use]
pub fn data_quat(q: Quaternion) -> data::Quat {
    data::Quat {
        w: q.w,
        x: q.x,
        y: q.y,
        z: q.z,
    }
}

/// dereth-primitives' `Quat` as a `Quaternion`.
#[must_use]
pub fn quaternion_of_data(q: data::Quat) -> Quaternion {
    Quaternion {
        x: q.x,
        y: q.y,
        z: q.z,
        w: q.w,
    }
}

impl From<Frame> for wire::Frame {
    fn from(f: Frame) -> Self {
        wire::Frame {
            origin: wire_vec3(f.origin),
            orientation: wire_quat(f.orientation),
        }
    }
}

impl From<wire::Frame> for Frame {
    fn from(f: wire::Frame) -> Self {
        Frame {
            origin: vector3_of_wire(f.origin),
            orientation: quaternion_of_wire(f.orientation),
        }
    }
}

impl From<Frame> for data::Frame {
    fn from(f: Frame) -> Self {
        data::Frame {
            origin: data_vec3(f.origin),
            rotation: data_quat(f.orientation),
        }
    }
}

impl From<data::Frame> for Frame {
    fn from(f: data::Frame) -> Self {
        Frame {
            origin: vector3_of_data(f.origin),
            orientation: quaternion_of_data(f.rotation),
        }
    }
}

impl Position {
    /// The position's frame on the wire: the origin and the rotation, as stored.
    #[must_use]
    pub fn wire_frame(&self) -> wire::Frame {
        wire::Frame {
            origin: wire::Vec3 {
                x: self.position_x,
                y: self.position_y,
                z: self.position_z,
            },
            orientation: wire::Quat {
                w: self.rotation_w,
                x: self.rotation_x,
                y: self.rotation_y,
                z: self.rotation_z,
            },
        }
    }
}

/// What `Position.Serialize(writer)` writes (cell, origin, rotation): the stored fields as they
/// are.
impl From<&Position> for wire::PositionWire {
    fn from(p: &Position) -> Self {
        wire::PositionWire {
            objcell_id: p.landblock_id().raw(),
            frame: p.wire_frame(),
        }
    }
}

/// What `new Position(BinaryReader)` reads: the fields as sent, with no re-homing.
impl From<wire::PositionWire> for Position {
    fn from(p: wire::PositionWire) -> Self {
        let mut pos = Position::default();
        pos.set_landblock_id(LandblockId::new(p.objcell_id));
        let (o, q) = (p.frame.origin, p.frame.orientation);
        (pos.position_x, pos.position_y, pos.position_z) = (o.x, o.y, o.z);
        (
            pos.rotation_w,
            pos.rotation_x,
            pos.rotation_y,
            pos.rotation_z,
        ) = (q.w, q.x, q.y, q.z);
        pos
    }
}

/// The cell and the origin, with no rotation.
impl From<&Position> for wire::Origin {
    fn from(p: &Position) -> Self {
        wire::Origin {
            objcell_id: p.landblock_id().raw(),
            origin: wire::Vec3 {
                x: p.position_x,
                y: p.position_y,
                z: p.position_z,
            },
        }
    }
}

impl From<&Position> for data::Position {
    fn from(p: &Position) -> Self {
        let f = p.wire_frame();
        data::Position {
            cell: data::CellId(p.landblock_id().raw()),
            frame: data::Frame {
                origin: data::Vec3 {
                    x: f.origin.x,
                    y: f.origin.y,
                    z: f.origin.z,
                },
                rotation: data::Quat {
                    w: f.orientation.w,
                    x: f.orientation.x,
                    y: f.orientation.y,
                    z: f.orientation.z,
                },
            },
        }
    }
}

impl From<data::Position> for Position {
    fn from(p: data::Position) -> Self {
        let r = p.frame.rotation;
        Position::from(wire::PositionWire {
            objcell_id: p.cell.0,
            frame: wire::Frame {
                origin: wire::Vec3 {
                    x: p.frame.origin.x,
                    y: p.frame.origin.y,
                    z: p.frame.origin.z,
                },
                orientation: wire::Quat {
                    w: r.w,
                    x: r.x,
                    y: r.y,
                    z: r.z,
                },
            },
        })
    }
}
