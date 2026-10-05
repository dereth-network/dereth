//! Distances and offsets between landblock-relative positions.

use crate::frame::V3;
use crate::{CellId, Position, Vec3};

/// The metre offset between landblocks; Z is always zero.
#[must_use]
pub fn block_offset(from: CellId, to: CellId) -> Vec3 {
    let (a, b) = (from.landblock(), to.landblock());
    Vec3::new(
        (f32::from(b.x()) - f32::from(a.x())) * crate::num::consts::LANDBLOCK_SIZE,
        (f32::from(b.y()) - f32::from(a.y())) * crate::num::consts::LANDBLOCK_SIZE,
        0.0,
    )
}

/// The offset between two positions, crossing landblocks.
#[must_use]
pub fn get_offset(from: &Position, to: &Position) -> Vec3 {
    block_offset(from.cell, to.cell)
        .add(to.frame.origin)
        .sub(from.frame.origin)
}

/// The full three-dimensional distance between positions.
#[must_use]
pub fn distance(a: &Position, b: &Position) -> f32 {
    get_offset(a, b).mag2().sqrt()
}

/// Cylinder distance using the three-dimensional centre distance and vertical gap.
#[must_use]
pub fn cylinder_distance(r1: f32, h1: f32, p1: &Position, r2: f32, h2: f32, p2: &Position) -> f32 {
    let d = get_offset(p1, p2).mag2().sqrt() - (r1 + r2);
    let (z1, z2) = (p1.frame.origin.z, p2.frame.origin.z);
    let gap = if z1 <= z2 {
        z2 - (z1 + h1)
    } else {
        z1 - (z2 + h2)
    };
    if gap <= 0.0 {
        if d > 0.0 {
            d
        } else {
            -(d * d + gap * gap).sqrt()
        }
    } else if d > 0.0 {
        (d * d + gap * gap).sqrt()
    } else {
        gap
    }
}

/// Convert an admitted landscape grid coordinate to north/south and east/west map values.
#[must_use]
pub fn landscape_coordinates(east_west: i32, north_south: i32) -> (f64, f64) {
    (
        f64::from(north_south - 0x400) * 0.1 + 0.5,
        f64::from(east_west - 0x400) * 0.1 + 0.5,
    )
}
