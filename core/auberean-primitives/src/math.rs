//! Vectors, rotations, frames and bounds, as the data files store them: 32-bit floats, a
//! rotation as a unit quaternion with its scalar first.

/// A point or direction in a landblock's or an object's space.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
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

    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// A rotation: a unit quaternion, scalar part `w` first as stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Quat {
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// A placement: an origin and a rotation.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Frame {
    pub origin: Vec3,
    pub rotation: Quat,
}

/// An axis-aligned box: its least and greatest corners.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BBox {
    pub min: Vec3,
    pub max: Vec3,
}

/// A sphere: center and radius.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
}

/// A color with alpha, each channel a float as stored (normally 0 to 1).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
