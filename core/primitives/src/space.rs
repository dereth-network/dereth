//! Space: Z-up, right-handed, metres.
//!
//! The client's world is Z-up; the mapping into Direct3D's left-handed space is a Y/Z swap done in
//! the renderer, not here; this crate only carries the shared coordinate types.

use crate::ids::CellId;

/// A point or direction in world space, in metres.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[inline]
    #[must_use]
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    #[inline]
    #[must_use]
    pub fn magnitude(self) -> f32 {
        self.dot(self).sqrt()
    }
}

/// A rotation, stored in the original's component order: w first.
///
/// The order matters because quaternions are read straight out of the dat files in this order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    #[inline]
    #[must_use]
    pub const fn new(w: f32, x: f32, y: f32, z: f32) -> Self {
        Self { w, x, y, z }
    }
}

/// An origin plus a rotation. This is the 28-byte record the dat files store, in that field order.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Frame {
    pub origin: Vec3,
    pub rotation: Quat,
}

impl Frame {
    #[inline]
    #[must_use]
    pub const fn new(origin: Vec3, rotation: Quat) -> Self {
        Self { origin, rotation }
    }
}

/// A cell plus a frame within it. Distances between positions in different landblocks have to go
/// through the 192 m block origins; that arithmetic belongs to the physics track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub cell: CellId,
    pub frame: Frame,
}

impl Position {
    #[inline]
    #[must_use]
    pub const fn new(cell: CellId, frame: Frame) -> Self {
        Self { cell, frame }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quaternion_default_is_identity_with_w_first() {
        let q = Quat::default();
        assert_eq!(q.w, 1.0);
        assert_eq!((q.x, q.y, q.z), (0.0, 0.0, 0.0));
    }

    #[test]
    fn frame_is_28_bytes_of_payload_in_dat_order() {
        // Not a layout assertion (Rust makes no repr promise here); a reminder that the wire order is
        // origin xyz then quaternion wxyz, which is what the dat readers must produce.
        let f = Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::new(0.5, 0.6, 0.7, 0.8));
        assert_eq!(f.origin.x, 1.0);
        assert_eq!(f.rotation.w, 0.5);
    }

    #[test]
    fn magnitude_matches_the_obvious_definition() {
        assert_eq!(Vec3::new(3.0, 4.0, 0.0).magnitude(), 5.0);
    }
}
