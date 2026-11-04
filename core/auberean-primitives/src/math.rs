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

/// A placement: an origin and a rotation. A new one stands at the origin, unturned; a copy is the
/// same origin and rotation.
// D2-00139, D2-01792, D2-00143, D2-00140
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Frame {
    pub origin: Vec3,
    pub rotation: Quat,
}

/// An axis-aligned box: its least and greatest corners.
// D2-13221, D2-02067
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BBox {
    pub min: Vec3,
    pub max: Vec3,
}

/// A sphere: center and radius. A copy has the same center and radius.
// D2-03511
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

/// How close the client counts two floats as the same in many places.
pub const SAME_FLOAT: f32 = 0.0002;

/// Whether `x` is within `eps` of zero (exclusive).
#[must_use]
pub fn is_float_zero(x: f32, eps: f32) -> bool {
    x.abs() < eps
}

/// Whether `a` and `b` are within `eps` of each other (exclusive).
// D2-03469
#[must_use]
pub fn are_float_equal(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() < eps
}

fn wide(x: f32) -> f64 {
    f64::from(x)
}

#[allow(clippy::cast_possible_truncation)]
fn narrow(x: f64) -> f32 {
    x as f32
}

impl Rgba {
    /// Raises each channel below 0 to 0.
    pub fn cap_low(&mut self) {
        for c in [&mut self.a, &mut self.r, &mut self.g, &mut self.b] {
            if *c < 0.0 {
                *c = 0.0;
            }
        }
    }

    /// Lowers each channel above 1 to 1.
    pub fn cap_high(&mut self) {
        for c in [&mut self.a, &mut self.r, &mut self.g, &mut self.b] {
            if 1.0 < *c {
                *c = 1.0;
            }
        }
    }

    /// Scales the color (not its alpha) so its brightest channel is 1; black stays black. The
    /// brightest is the first of red, green and blue that is at least each of the others; when
    /// none is (a channel not a number), nothing changes.
    // D2-10861
    pub fn normalize(&mut self) {
        let (r, g, b) = (self.r, self.g, self.b);
        if r == 0.0 && g == 0.0 && b == 0.0 {
            return;
        }
        let top = if r >= g && r >= b {
            r
        } else if g >= r && g >= b {
            g
        } else if b >= r && b >= g {
            b
        } else {
            return;
        };
        let k = 1.0 / wide(top);
        self.r = narrow(k * wide(r));
        self.g = narrow(k * wide(g));
        self.b = narrow(k * wide(b));
    }

    /// Whether no channel lies below 0 or above 1 by more than 0.0002 (a channel that is not a
    /// number lies neither).
    // D2-10862
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let lo = -SAME_FLOAT;
        let hi = narrow(wide(SAME_FLOAT) + 1.0);
        let c = [self.a, self.r, self.g, self.b];
        !c.iter().any(|&v| v < lo) && !c.iter().any(|&v| v > hi)
    }

    /// Whether two colors are the same, each channel within 0.0002.
    // D2-13883
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        are_float_equal(self.r, other.r, SAME_FLOAT)
            && are_float_equal(self.g, other.g, SAME_FLOAT)
            && are_float_equal(self.b, other.b, SAME_FLOAT)
            && are_float_equal(self.a, other.a, SAME_FLOAT)
    }

    /// The color as one word: alpha, red, green and blue from the top byte down, each channel
    /// times 255 cut toward zero, the four ORed together unmasked (a channel past 0 to 1 spills
    /// into its neighbors).
    #[must_use]
    pub fn color32(&self) -> u32 {
        // Cut toward zero to a 64-bit whole number; one out of its range (or not a number) is
        // the processor's "indefinite", whose low word is zero.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let part = |c: f32| {
            let v = wide(c) * 255.0;
            if v.abs() < 9.223_372_036_854_776e18 {
                v as i64 as u32
            } else {
                0
            }
        };
        let mut v = part(self.a);
        v = (v << 8) | part(self.r);
        v = (v << 8) | part(self.g);
        (v << 8) | part(self.b)
    }

    /// Writes the color's binary form, the one word of [`Rgba::color32`] little-endian, at the
    /// start of `buf` when it holds four bytes, and answers its size, four, either way.
    // D2-13244
    pub fn pack_into(&self, buf: &mut [u8]) -> usize {
        if let Some(out) = buf.get_mut(..4) {
            out.copy_from_slice(&self.color32().to_le_bytes());
        }
        4
    }
}
