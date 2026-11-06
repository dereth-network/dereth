//! What the client does with a position (a cell and a frame in its landblock): its heading, its
//! pitch toward another, where another stands around it, the land cell outside it, and the cell
//! and offset a position is sent as.

use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use super::{block_offset, frame_heading, offset, point_local_to_local, set_heading, Heading};
use crate::cells;

fn w(x: f32) -> f64 {
    f64::from(x)
}

#[allow(clippy::cast_possible_truncation)]
fn n(x: f64) -> f32 {
    x as f32
}

/// A position: `frame` in `cell`'s landblock. A copy of a position is its cell and frame.
#[must_use]
pub fn position_at(cell: CellId, frame: Frame) -> Position {
    Position::new(cell, frame)
}

/// A position not yet anywhere: cell 0, at the origin, unturned.
// D2-10274
#[must_use]
pub fn nowhere() -> Position {
    Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY))
}

/// The heading the position faces.
#[must_use]
pub fn position_heading(p: &Position) -> Heading {
    frame_heading(&p.frame)
}

/// Faces the position toward a heading, keeping its tilt.
pub fn set_position_heading(p: &mut Position, h: Heading) {
    set_heading(&mut p.frame, h);
}

/// Whether two positions differ: another cell, or an origin or rotation part more than 0.0002
/// off.
#[must_use]
pub fn positions_differ(a: &Position, b: &Position) -> bool {
    let near = |x: f32, y: f32| (x - y).abs() < super::EPS;
    let (p, q) = (&a.frame, &b.frame);
    let same_frame = near(p.origin.x, q.origin.x)
        && near(p.origin.y, q.origin.y)
        && near(p.origin.z, q.origin.z)
        && near(p.rotation.w, q.rotation.w)
        && near(p.rotation.x, q.rotation.x)
        && near(p.rotation.y, q.rotation.y)
        && near(p.rotation.z, q.rotation.z);
    !(a.cell == b.cell && same_frame)
}

/// Brings the position into the land cell its origin lies over, the origin carried into that
/// cell's landblock; false (and cell 0) off the map.
pub fn adjust_position_to_outside(p: &mut Position) -> bool {
    cells::adjust_to_outside(&mut p.cell, &mut p.frame.origin)
}

/// The land cell the position's origin lies over; 0 off the map.
// D2-10760
#[must_use]
pub fn outside_cell(p: &Position) -> CellId {
    let (mut cell, mut origin) = (p.cell, p.frame.origin);
    if cells::adjust_to_outside(&mut cell, &mut origin) {
        cell
    } else {
        CellId(0)
    }
}

/// `other`'s origin in the landblock frame of `p`'s cell.
#[must_use]
pub fn origin_in_block_of(p: &Position, other: &Position) -> Vec3 {
    let b = block_offset(p.cell, other.cell);
    let o = other.frame.origin;
    Vec3::new(b.x + o.x, b.y + o.y, b.z + o.z)
}

/// How far `other` lies above the level from `p`, in degrees; 0 for one closer than 0.0002.
#[must_use]
pub fn pitch_to(p: &Position, other: &Position) -> f32 {
    let o = offset(p, other);
    let len = ((w(o.x) * w(o.x) + w(o.y) * w(o.y)) + w(o.z) * w(o.z)).sqrt();
    if len < w(super::EPS) {
        return 0.0;
    }
    let z = n((1.0 / len) * w(o.z));
    n(math::asin(w(z)) * 57.295_779_513_082_32)
}

/// Moves the position to map coordinates (east, north): into the land cell there, its origin
/// across the ground to that cell's middle (its height and rotation kept); off the map, its cell
/// becomes 0 and false.
// D2-13200
pub fn set_from_map_coords(p: &mut Position, east: f32, north: f32) -> bool {
    let Some(cell) = cells::map_coords_to_cell(east, north) else {
        p.cell = CellId(0);
        return false;
    };
    p.cell = cell;
    let c = (cell.0 & 0xFFFF) - 1;
    #[allow(clippy::cast_precision_loss)]
    let at = |k: u32| n(w(k as f32) * 20.0 + 10.0);
    p.frame.origin.x = at(c >> 3);
    p.frame.origin.y = at(c & 7);
    true
}

/// The quadrant flags: high, medium and low thirds of a height, left and right, front and back.
pub mod quadrant {
    pub const HIGH: u32 = 1;
    pub const MEDIUM: u32 = 2;
    pub const LOW: u32 = 4;
    pub const LEFT: u32 = 8;
    pub const RIGHT: u32 = 16;
    pub const FRONT: u32 = 32;
    pub const BACK: u32 = 64;
}

/// Where `other`'s origin lies around `p`, seen in `p`'s frame: left or right, front or back, and
/// in which third of `height` it stands (under a third low, under two thirds medium, else high).
#[must_use]
pub fn quadrant_of(p: &Position, height: f32, other: &Position) -> u32 {
    let v = point_local_to_local(p, other, Vec3::ZERO);
    let mut q = if 0.0 <= v.x {
        quadrant::RIGHT
    } else {
        quadrant::LEFT
    };
    q |= if 0.0 <= v.y {
        quadrant::FRONT
    } else {
        quadrant::BACK
    };
    let z = w(v.z);
    if z < w(height) * w(0.333_333_34_f32) {
        q | quadrant::LOW
    } else if z < w(height) * w(0.666_666_7_f32) {
        q | quadrant::MEDIUM
    } else {
        q | quadrant::HIGH
    }
}

/// A position as it is sent in short: its cell and its origin, without a rotation.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PositionOffset {
    pub cell: CellId,
    pub offset: Vec3,
}

impl PositionOffset {
    /// The cell and origin of `p`.
    #[must_use]
    pub fn from_position(p: &Position) -> Self {
        Self {
            cell: p.cell,
            offset: p.frame.origin,
        }
    }

    /// The position at this cell and origin, unturned.
    // D2-13389, D2-13176
    #[must_use]
    pub fn to_position(&self) -> Position {
        Position::new(self.cell, Frame::new(self.offset, Quat::IDENTITY))
    }
}
