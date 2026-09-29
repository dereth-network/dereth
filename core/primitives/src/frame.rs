//! Frame arithmetic: the rotation matrix a quaternion caches, composing, rotating and heading
//! frames, and the vector operations they are built from, each in the client's own operand order.
//!
//! One implementation for every crate that moves a frame: physics, the animation player and the
//! landscape's placement all call it. It is pure arithmetic over [`Vec3`], [`Quat`] and [`Frame`]
//! (no collision, no cells, no engine state); what a frame *means* to a subsystem stays there, and
//! so do the operations only one subsystem uses.
//!
//! **The cached matrix.** The client caches a 3x3 local-to-global matrix inside each frame, and
//! frame assignment copies it rather than re-deriving it. [`Frame`] is a quaternion plus an origin
//! with no matrix, so [`l2g`] re-derives it from the quaternion at every use. The two differ only
//! for a frame whose matrix was written directly and whose quaternion was then back-derived, which
//! a caller does only through its own quaternion (the landscape's billboarding writes axes into a
//! matrix and turns it back into a quaternion before storing it).
//!
//! **Two choices that were measured, not assumed.** The small-vector check
//! ([`V3::normalize_check_small`]) takes the length and compares it with `0.0002`, as the client
//! does; comparing the squared length with `0.0002²` instead gives the same answer for every `f32`
//! (the tests below scan the threshold). [`set_vector_heading`] turns the pitch into degrees and
//! back, as the client does; the round trip gives the same `f32` as the bare arcsine for every
//! input in `[-1, 1]`. Both earlier copies of this arithmetic produced bit-identical results over
//! every frame, placement, keyframe and composition in the retail data.

use crate::num::consts::EPSILON;
use crate::num::math;
use crate::space::{Frame, Quat, Vec3};

const DEG_PER_RAD: f64 = 57.295_779_513_082_32;
const RAD_PER_DEG: f64 = 0.017_453_292_519_943_295;

// ---------------------------------------------------------------------------------------------
// Vector3
// ---------------------------------------------------------------------------------------------

/// Vector operators as an extension trait, so [`Vec3`] itself stays plain data.
pub trait V3: Copy {
    #[must_use]
    fn add(self, o: Self) -> Self;
    #[must_use]
    fn sub(self, o: Self) -> Self;
    /// Scalar multiplication.
    #[must_use]
    fn mul(self, s: f32) -> Self;
    #[must_use]
    fn mul_componentwise(self, o: Self) -> Self;
    #[must_use]
    fn negate(self) -> Self;
    #[must_use]
    fn cross(self, o: Self) -> Self;
    /// `|v|` squared, summed `x² + y² + z²`.
    #[must_use]
    fn mag2(self) -> f32;
    /// **Every component's magnitude** below `2e-4`.
    #[must_use]
    fn is_zero(self) -> bool;
    /// Per component, `|a - b| < 2e-4`.
    #[must_use]
    fn eq_eps(self, o: Self) -> bool;
    /// Unguarded: divides by the length whatever it is.
    #[must_use]
    fn normalize(self) -> Self;
    /// The small-vector normalise check: returns **true when the vector was too small to
    /// normalise** (its length below `2e-4`), leaving it untouched. Note the sense, which reads
    /// backwards and is easy to invert: a `false` return means the normalise happened.
    fn normalize_check_small(&mut self) -> bool;
}

impl V3 for Vec3 {
    #[inline]
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    #[inline]
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    #[inline]
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
    #[inline]
    fn mul_componentwise(self, o: Self) -> Self {
        Self::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }
    #[inline]
    fn negate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
    #[inline]
    fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    #[inline]
    fn mag2(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }
    #[inline]
    fn is_zero(self) -> bool {
        self.x.abs() < EPSILON && self.y.abs() < EPSILON && self.z.abs() < EPSILON
    }
    #[inline]
    fn eq_eps(self, o: Self) -> bool {
        (self.x - o.x).abs() < EPSILON
            && (self.y - o.y).abs() < EPSILON
            && (self.z - o.z).abs() < EPSILON
    }
    #[inline]
    fn normalize(self) -> Self {
        let inv = 1.0 / self.mag2().sqrt();
        self.mul(inv)
    }
    #[inline]
    fn normalize_check_small(&mut self) -> bool {
        let len = self.mag2().sqrt();
        if len < EPSILON {
            return true;
        }
        *self = self.mul(1.0 / len);
        false
    }
}

// ---------------------------------------------------------------------------------------------
// The cached matrix
// ---------------------------------------------------------------------------------------------

/// The cached local-to-global rotation, nine floats in the client's index order: the local X
/// axis in world space is `(m[0], m[1], m[2])`, local Y is `(m[3], m[4], m[5])` and local Z is
/// `(m[6], m[7], m[8])`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3(pub [f32; 9]);

impl Mat3 {
    /// The identity, which is what the identity quaternion **and** the all-zero quaternion give.
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
}

/// The local-to-global matrix of a quaternion, in the client's exact operand order.
///
/// Note `m[4]` and `m[8]` share the sub-expression `1 - 2*qx*qx`, which is how the client computes
/// them; keeping that shared is what makes the two results bit-identical to the client's.
#[must_use]
pub fn l2g(q: Quat) -> Mat3 {
    let x2 = q.x + q.x;
    let y2 = q.y + q.y;
    let z2 = q.z + q.z;
    let wx = q.w * x2;
    let wy = q.w * y2;
    let wz = q.w * z2;
    let xy = q.x * y2;
    let xz = q.x * z2;
    let yy = q.y * y2;
    let yz = q.y * z2;
    let zz = q.z * z2;
    let one_minus_xx = 1.0 - q.x * x2;
    Mat3([
        (1.0 - yy) - zz,
        xy + wz,
        xz - wy,
        xy - wz,
        one_minus_xx - zz,
        yz + wx,
        xz + wy,
        yz - wx,
        one_minus_xx - yy,
    ])
}

/// Local to global, vector: rotation only.
#[must_use]
pub fn localtoglobalvec(m: Mat3, v: Vec3) -> Vec3 {
    let m = m.0;
    Vec3::new(
        m[0] * v.x + m[3] * v.y + m[6] * v.z,
        m[1] * v.x + m[4] * v.y + m[7] * v.z,
        m[2] * v.x + m[5] * v.y + m[8] * v.z,
    )
}

/// Global to local, vector: the transpose.
#[must_use]
pub fn globaltolocalvec(m: Mat3, v: Vec3) -> Vec3 {
    let m = m.0;
    Vec3::new(
        m[0] * v.x + m[1] * v.y + m[2] * v.z,
        m[3] * v.x + m[4] * v.y + m[5] * v.z,
        m[6] * v.x + m[7] * v.y + m[8] * v.z,
    )
}

/// Local to global, point.
#[must_use]
pub fn localtoglobal(f: &Frame, v: Vec3) -> Vec3 {
    localtoglobalvec(l2g(f.rotation), v).add(f.origin)
}

/// Global to local, point.
#[must_use]
pub fn globaltolocal(f: &Frame, v: Vec3) -> Vec3 {
    globaltolocalvec(l2g(f.rotation), v.sub(f.origin))
}

// ---------------------------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------------------------

/// No NaN anywhere, and `|q|² - 1` within `0.0009999999`.
#[must_use]
pub fn frame_is_valid(f: &Frame) -> bool {
    let finite = !f.origin.x.is_nan()
        && !f.origin.y.is_nan()
        && !f.origin.z.is_nan()
        && !f.rotation.w.is_nan()
        && !f.rotation.x.is_nan()
        && !f.rotation.y.is_nan()
        && !f.rotation.z.is_nan();
    let q = f.rotation;
    let mag = q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z;
    if mag.is_nan() || (mag - 1.0).abs() > 0.000_999_999_9 {
        return false;
    }
    finite
}

/// Normalise, assign, validate — and **restore the old quaternion if the result is invalid**,
/// which is not what a naive `q = normalize(q)` does.
pub fn set_rotate(f: &mut Frame, w: f32, x: f32, y: f32, z: f32) {
    let old = f.rotation;
    let inv = 1.0 / (w * w + x * x + y * y + z * z).sqrt();
    f.rotation = Quat::new(w * inv, x * inv, y * inv, z * inv);
    if !frame_is_valid(f) {
        f.rotation = old;
    }
}

/// `out.q = a.q * b.q`, `out.origin = a.origin + a.l2g * b.origin`.
#[must_use]
pub fn combine(a: &Frame, b: &Frame) -> Frame {
    let m = l2g(a.rotation);
    let origin = a.origin.add(localtoglobalvec(m, b.origin));
    let mut out = Frame::new(origin, Quat::IDENTITY);
    let (p, q) = (a.rotation, b.rotation);
    set_rotate(
        &mut out,
        p.w * q.w - p.x * q.x - p.y * q.y - p.z * q.z,
        p.w * q.x + p.x * q.w + p.y * q.z - p.z * q.y,
        p.w * q.y + p.y * q.w + p.z * q.x - p.x * q.z,
        p.w * q.z + p.z * q.w + p.x * q.y - p.y * q.x,
    );
    out
}

/// Rotate about a **global** axis by `|v|` radians. A no-op when `|v|² < 2e-4 * 2e-4`, which is
/// the guard that keeps a zero rotation (a zero omega) from producing NaN through the reciprocal.
pub fn grotate(f: &mut Frame, v: Vec3) {
    let mag2 = v.mag2();
    if mag2 < EPSILON * EPSILON {
        return;
    }
    let len = mag2.sqrt();
    let inv = 1.0 / len;
    let half = len * 0.5;
    let s = math::sinf(half);
    let rw = math::cosf(half);
    let rx = s * v.x * inv;
    let ry = inv * s * v.y;
    let rz = s * inv * v.z;
    let q = f.rotation;
    set_rotate(
        f,
        ((rw * q.w - rx * q.x) - ry * q.y) - rz * q.z,
        (rx * q.w + ry * q.z + rw * q.x) - rz * q.y,
        ry * q.w + rz * q.x + (rw * q.y - rx * q.z),
        rz * q.w + ((rw * q.z + rx * q.y) - ry * q.x),
    );
}

/// [`grotate`] with the axis given in **local** space: pushed through the frame's local-to-global
/// matrix, in the same element order as [`localtoglobalvec`], then handed to [`grotate`]. The
/// local and global readings coincide on an unrotated frame; the distinction shows only on one
/// that is already rotated.
pub fn rotate(f: &mut Frame, v: Vec3) {
    let global = localtoglobalvec(l2g(f.rotation), v);
    grotate(f, global);
}

/// The frame's local Y axis in world space: a dot product with `(0, 1, 0)`, so just the second
/// matrix column.
#[must_use]
pub fn get_vector_heading(f: &Frame) -> Vec3 {
    let m = l2g(f.rotation).0;
    Vec3::new(m[3], m[4], m[5])
}

/// The compass heading of a bare vector, in **degrees**: 0 = +Y (north), increasing clockwise.
/// The same arithmetic as [`get_heading`], on a vector rather than on a frame's axes.
#[must_use]
pub fn vector_get_heading(v: Vec3) -> f32 {
    let a = math::atan2(f64::from(v.y), f64::from(v.x));
    #[allow(clippy::cast_possible_truncation)]
    {
        ((450.0 - a * DEG_PER_RAD) % 360.0) as f32
    }
}

/// The frame's heading in **degrees**: 0 = +Y (north), increasing clockwise. (The client's
/// underlying frame-heading calculation returns radians instead; mixing them is the classic bug.)
#[must_use]
pub fn get_heading(f: &Frame) -> f32 {
    let m = l2g(f.rotation).0;
    let a = math::atan2(f64::from(m[4]), f64::from(m[3]));
    #[allow(clippy::cast_possible_truncation)]
    {
        ((450.0 - a * DEG_PER_RAD) % 360.0) as f32
    }
}

/// Point the frame along `v`: pitch from the Z component, yaw from the XY direction, roll zero. A
/// degenerate vector leaves the frame untouched.
///
/// The yaw is `-fmod(450 - atan2 * 180/pi, 360) * (pi/180)`: the same compass conversion
/// [`get_heading`] uses, which is what makes the two self-consistent (identity in, identity out),
/// with the sign flipped because heading grows clockwise while a positive rotation about +Z is
/// counter-clockwise. The pitch is the arcsine turned into degrees and back, as the client writes
/// it.
pub fn set_vector_heading(f: &mut Frame, v: Vec3) {
    let mut v = v;
    if v.normalize_check_small() {
        return;
    }
    let a = math::atan2(f64::from(v.y), f64::from(v.x));
    let heading_deg = (450.0 - a * DEG_PER_RAD) % 360.0;
    let yaw = -heading_deg * RAD_PER_DEG;
    let pitch = math::asin(f64::from(v.z)) * DEG_PER_RAD * RAD_PER_DEG;
    #[allow(clippy::cast_possible_truncation)]
    euler_set_rotate(f, pitch as f32, 0.0, yaw as f32);
}

/// Set the heading in degrees, **keeping the current forward vector's Z**: the new direction is
/// `(sin h, cos h, z)`, normalised, so a pitched frame stays pitched.
pub fn set_heading(f: &mut Frame, degrees: f32) {
    let z = l2g(f.rotation).0[5];
    let r = f64::from(degrees) * RAD_PER_DEG;
    #[allow(clippy::cast_possible_truncation)]
    let v = Vec3::new(math::sin(r) as f32, math::cos(r) as f32, z);
    set_vector_heading(f, v);
}

/// Set the rotation from Euler angles, with the order argument fixed at `0`, the only value any
/// client caller passes.
///
/// The client uses Shoemake's Euler-to-quaternion conversion (Graphics Gems IV). Order `0` decodes
/// to `EulOrdXYZs`: static frame, even parity, no repeat, X first — so the composite is
/// `Rz(yaw) * Ry(roll) * Rx(pitch)`, and the parity, repeat and frame fix-ups are all no-ops. The
/// four products (`cc`, `cs`, `sc`, `ss`) are kept in the client's operand order.
pub fn euler_set_rotate(f: &mut Frame, pitch: f32, roll: f32, yaw: f32) {
    let (ci, si) = (math::cosf(pitch * 0.5), math::sinf(pitch * 0.5));
    let (cj, sj) = (math::cosf(roll * 0.5), math::sinf(roll * 0.5));
    let (ch, sh) = (math::cosf(yaw * 0.5), math::sinf(yaw * 0.5));
    let cc = ch * ci;
    let cs = ci * sh;
    let sc = si * ch;
    let ss = sh * si;
    set_rotate(
        f,
        cc * cj + ss * sj,
        sc * cj - cs * sj,
        ss * cj + cc * sj,
        cs * cj - sc * sj,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The client compares the vector's length with `0.0002`; comparing the squared length with
    /// `0.0002²` answers the same for every `f32` squared length, which the scan below walks
    /// through the whole neighbourhood of the threshold (outside it both comparisons are
    /// monotonic and agree). The two forms were written separately and must never be told apart.
    #[test]
    fn the_length_and_the_squared_length_thresholds_agree_on_every_f32() {
        let eps2 = EPSILON * EPSILON;
        let lo = (eps2 * 0.99).to_bits();
        let hi = (eps2 * 1.01).to_bits();
        let mut scanned = 0;
        for bits in lo..=hi {
            let m2 = f32::from_bits(bits);
            assert_eq!(m2.sqrt() < EPSILON, m2 < eps2, "squared length {m2:e}");
            scanned += 1;
        }
        assert!(scanned > 20_000);
        // Just below and above the threshold, along an axis and along a diagonal.
        let mut v = Vec3::new(0.000_199_9, 0.0, 0.0);
        assert!(v.normalize_check_small());
        let mut v = Vec3::new(0.000_200_1, 0.0, 0.0);
        assert!(!v.normalize_check_small());
        let mut v = Vec3::new(0.000_116, 0.000_116, 0.000_116);
        assert!(!v.normalize_check_small());
        let mut v = Vec3::new(0.000_115, 0.000_115, 0.000_1);
        assert!(v.normalize_check_small());
    }

    /// The pitch's round trip through degrees gives the same `f32` as the bare arcsine: sampled
    /// here over `[-1, 1]` (every input was checked when the copies were merged).
    #[test]
    fn the_pitch_round_trip_through_degrees_is_the_bare_arcsine_in_f32() {
        let one = 1.0f32.to_bits();
        let mut n = 0;
        for bits in (0..=one).step_by(997) {
            for z in [f32::from_bits(bits), -f32::from_bits(bits)] {
                let a = math::asin(f64::from(z));
                #[allow(clippy::cast_possible_truncation)]
                let (bare, trip) = (a as f32, (a * DEG_PER_RAD * RAD_PER_DEG) as f32);
                assert_eq!(bare.to_bits(), trip.to_bits(), "z = {z:e}");
                n += 1;
            }
        }
        assert!(n > 2_000_000);
    }

    /// Set the heading, read it back: the compass conversion is its own inverse, and a pitched
    /// frame's forward Z goes into the new direction before it is normalised.
    #[test]
    fn set_heading_keeps_the_forward_z_and_get_heading_reads_it_back() {
        for h in [0.0f32, 12.5, 90.0, 181.0, 359.5] {
            let mut f = Frame::default();
            set_heading(&mut f, h);
            assert!((get_heading(&f) - h).abs() < 1e-3, "{h}");
        }
        let mut f = Frame::default();
        set_vector_heading(&mut f, Vec3::new(0.0, 1.0, 1.0));
        let z = l2g(f.rotation).0[5];
        set_heading(&mut f, 200.0);
        let want = z / (1.0 + z * z).sqrt();
        assert!((l2g(f.rotation).0[5] - want).abs() < 1e-5);
        assert!((get_heading(&f) - 200.0).abs() < 1e-3);
    }

    fn close(a: [f32; 3], b: [f32; 3]) {
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 1e-5, "{a:?} != {b:?}");
        }
    }

    fn axis(f: &Frame, i: usize) -> [f32; 3] {
        let m = l2g(f.rotation).0;
        [m[3 * i], m[3 * i + 1], m[3 * i + 2]]
    }

    /// A quarter turn about each world axis moves the other two axes where a right-handed rotation
    /// puts them; a local rotation on a turned frame turns about the frame's own axis.
    #[test]
    fn grotate_and_rotate_turn_about_the_world_and_the_local_axis() {
        let q = std::f32::consts::FRAC_PI_2;
        let mut f = Frame::default();
        grotate(&mut f, Vec3::new(0.0, 0.0, q));
        close(axis(&f, 0), [0.0, 1.0, 0.0]);
        let mut f = Frame::default();
        grotate(&mut f, Vec3::new(0.0, q, 0.0));
        close(axis(&f, 2), [1.0, 0.0, 0.0]);
        let mut f = Frame::default();
        grotate(&mut f, Vec3::new(q, 0.0, 0.0));
        close(axis(&f, 1), [0.0, 0.0, 1.0]);
        // Turned a quarter about Z, the local X axis is world Y: a local turn about X is a world
        // turn about Y.
        let mut f = Frame::default();
        grotate(&mut f, Vec3::new(0.0, 0.0, q));
        rotate(&mut f, Vec3::new(q, 0.0, 0.0));
        close(axis(&f, 0), [0.0, 1.0, 0.0]);
        close(axis(&f, 2), [1.0, 0.0, 0.0]);
    }

    /// The Euler rotation is `Rz(yaw) * Ry(roll) * Rx(pitch)`: the same frame as turning about
    /// world X, then world Y, then world Z.
    #[test]
    fn euler_set_rotate_is_pitch_then_roll_then_yaw_about_the_world_axes() {
        for (p, r, y) in [
            (0.3f32, -0.7f32, 1.9f32),
            (-1.2, 0.4, -2.6),
            (0.05, 1.1, 0.0),
        ] {
            let mut e = Frame::default();
            euler_set_rotate(&mut e, p, r, y);
            let mut g = Frame::default();
            grotate(&mut g, Vec3::new(p, 0.0, 0.0));
            grotate(&mut g, Vec3::new(0.0, r, 0.0));
            grotate(&mut g, Vec3::new(0.0, 0.0, y));
            for i in 0..3 {
                close(axis(&e, i), axis(&g, i));
            }
        }
    }

    /// Pointing a frame along a climbing vector pitches its local Y axis up to that vector.
    #[test]
    fn set_vector_heading_points_the_local_y_axis_along_the_vector() {
        let h = std::f32::consts::FRAC_1_SQRT_2;
        for v in [
            Vec3::new(0.0, 1.0, 1.0),
            Vec3::new(1.0, 0.0, -1.0),
            Vec3::new(-3.0, 0.0, 0.0),
        ] {
            let mut f = Frame::default();
            set_vector_heading(&mut f, v);
            let n = v.normalize();
            close(axis(&f, 1), [n.x, n.y, n.z]);
        }
        let mut f = Frame::default();
        set_vector_heading(&mut f, Vec3::new(0.0, 1.0, 1.0));
        close(axis(&f, 1), [0.0, h, h]);
    }
}
