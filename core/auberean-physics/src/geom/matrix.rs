//! The 4 x 4 matrix the client builds transforms with: sixteen floats in memory order, each row a
//! basis vector and the last row the translation (a point is a row vector multiplied on the left).
//!
//! Every sum is worked in double precision and rounded to a float only where the client stores
//! one, as the client's floating-point unit does.

use dereth_primitives::num::math;
use dereth_primitives::{Quat, Vec3};

use super::{rotation_axes, rotation_from_axes, EPS};

fn w(x: f32) -> f64 {
    f64::from(x)
}

#[allow(clippy::cast_possible_truncation)]
fn n(x: f64) -> f32 {
    x as f32
}

/// A 4 x 4 matrix, its floats in memory order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix4(pub [f32; 16]);

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

impl Default for Matrix4 {
    fn default() -> Self {
        Self(IDENTITY)
    }
}

impl Matrix4 {
    /// The identity.
    pub const IDENTITY: Self = Self(IDENTITY);

    /// Becomes a translation by `(x, y, z)`.
    // D2-13272
    pub fn translate(&mut self, x: f32, y: f32, z: f32) {
        self.0 = IDENTITY;
        self.0[12] = x;
        self.0[13] = y;
        self.0[14] = z;
    }

    /// Becomes a translation by `v`.
    pub fn translate_vec(&mut self, v: Vec3) {
        self.translate(v.x, v.y, v.z);
    }

    /// Becomes the 3 x 3 translation of a plane's points by `(x, y)`: the third row carries it.
    pub fn translate3(&mut self, x: f32, y: f32) {
        self.0 = IDENTITY;
        self.0[8] = x;
        self.0[9] = y;
    }

    /// Becomes a turn of `angle` radians about the x axis.
    pub fn rotate_x(&mut self, angle: f32) {
        let (c, s) = (n(math::cos(w(angle))), n(math::sin(w(angle))));
        self.0 = IDENTITY;
        self.0[5] = c;
        self.0[6] = s;
        self.0[9] = -s;
        self.0[10] = c;
    }

    /// Becomes a turn of `angle` radians about the y axis.
    pub fn rotate_y(&mut self, angle: f32) {
        let (c, s) = (n(math::cos(w(angle))), n(math::sin(w(angle))));
        self.0 = IDENTITY;
        self.0[0] = c;
        self.0[2] = -s;
        self.0[8] = s;
        self.0[10] = c;
    }

    /// Becomes a turn of `angle` radians about the z axis.
    pub fn rotate_z(&mut self, angle: f32) {
        let (c, s) = (n(math::cos(w(angle))), n(math::sin(w(angle))));
        self.0 = IDENTITY;
        self.0[0] = c;
        self.0[1] = s;
        self.0[4] = -s;
        self.0[5] = c;
    }

    /// Becomes a scale by `(x, y, z)`.
    // D2-13278
    pub fn scale(&mut self, x: f32, y: f32, z: f32) {
        self.0 = IDENTITY;
        self.0[0] = x;
        self.0[5] = y;
        self.0[10] = z;
    }

    /// The determinant, unrounded.
    fn determinant_wide(&self) -> f64 {
        let m = |i: usize| w(self.0[i]);
        let a = (m(8) * m(13) - m(9) * m(12)) * m(7)
            + ((m(9) * m(15) - m(13) * m(11)) * m(4) - (m(8) * m(15) - m(12) * m(11)) * m(5));
        let b = (m(9) * m(14) - m(13) * m(10)) * m(7)
            + ((m(10) * m(15) - m(14) * m(11)) * m(5) - (m(9) * m(15) - m(13) * m(11)) * m(6));
        let c = (m(8) * m(14) - m(10) * m(12)) * m(7)
            + ((m(10) * m(15) - m(14) * m(11)) * m(4) - (m(8) * m(15) - m(12) * m(11)) * m(6));
        let d = (m(8) * m(13) - m(9) * m(12)) * m(6)
            + ((m(9) * m(14) - m(13) * m(10)) * m(4) - (m(8) * m(14) - m(12) * m(10)) * m(5));
        (a * m(2) + (b * m(0) - c * m(1))) - d * m(3)
    }

    /// The determinant.
    // D2-13279
    #[must_use]
    pub fn determinant(&self) -> f32 {
        n(self.determinant_wide())
    }

    /// Becomes the adjoint of `src`: the transposed matrix of its cofactors.
    // D2-13280
    pub fn adjoint(&mut self, src: &Self) {
        let s = |i: usize| w(src.0[i]);
        let (a0, a1, a2, a3) = (s(0), s(1), s(2), s(3));
        let (a4, a5, a6, a7) = (s(4), s(5), s(6), s(7));
        let (a8, a9, a10, a11) = (s(8), s(9), s(10), s(11));
        let (a12, a13, a14, a15) = (s(12), s(13), s(14), s(15));
        let o = &mut self.0;
        // Each 2 x 2 minor is stored as a float; the rest stays wide.
        let p = w(n(a15 * a10 - a14 * a11));
        let q = w(n(a15 * a9 - a13 * a11));
        let r = w(n(a14 * a9 - a13 * a10));
        o[0] = n(r * a7 + (p * a5 - q * a6));
        let t = w(n(a15 * a8 - a12 * a11));
        let u = w(n(a14 * a8 - a12 * a10));
        o[4] = n(-(u * a7 + (p * a4 - t * a6)));
        let v = w(n(a13 * a8 - a12 * a9));
        o[8] = n(v * a7 + (q * a4 - t * a5));
        o[12] = n(-(v * a6 + (r * a4 - u * a5)));
        o[1] = n(-(r * a3 + (p * a1 - q * a2)));
        o[5] = n(u * a3 + (p * a0 - t * a2));
        o[9] = n(-(v * a3 + (q * a0 - t * a1)));
        o[13] = n(v * a2 + (r * a0 - u * a1));
        let p = w(n(a15 * a6 - a14 * a7));
        let q = w(n(a15 * a5 - a13 * a7));
        let r = w(n(a14 * a5 - a13 * a6));
        o[2] = n(r * a3 + (p * a1 - q * a2));
        let t = w(n(a15 * a4 - a12 * a7));
        let u = w(n(a14 * a4 - a12 * a6));
        o[6] = n(-(u * a3 + (p * a0 - t * a2)));
        let v = w(n(a13 * a4 - a12 * a5));
        o[10] = n(v * a3 + (q * a0 - t * a1));
        o[14] = n(-(v * a2 + (r * a0 - u * a1)));
        let p = w(n(a11 * a6 - a10 * a7));
        let q = w(n(a11 * a5 - a9 * a7));
        let r = w(n(a10 * a5 - a9 * a6));
        o[3] = n(-(r * a3 + (p * a1 - q * a2)));
        let t = w(n(a11 * a4 - a8 * a7));
        let u = w(n(a10 * a4 - a8 * a6));
        o[7] = n(u * a3 + (p * a0 - t * a2));
        let v = w(n(a9 * a4 - a8 * a5));
        o[11] = n(-(v * a3 + (q * a0 - t * a1)));
        o[15] = n(v * a2 + (r * a0 - u * a1));
    }

    /// Becomes the inverse of `src`'s upper 3 x 3 (the rest the identity's), answering the
    /// reciprocal of its determinant; a determinant of exactly zero leaves the identity and
    /// answers zero.
    pub fn inverse3(&mut self, src: &Self) -> f32 {
        let s = |i: usize| w(src.0[i]);
        self.0 = IDENTITY;
        // The two products of the leading term are stored as floats; the rest stays wide.
        let p = w(n(s(5) * s(10)));
        let q = w(n(s(9) * s(6)));
        let det = (((s(9) * s(2) * s(4) + s(6) * s(8) * s(1)) + p * s(0)) - q * s(0))
            - s(10) * s(1) * s(4)
            - s(5) * s(8) * s(2);
        if det == 0.0 {
            return 0.0;
        }
        let k = 1.0 / det;
        let o = &mut self.0;
        o[0] = n((p - q) * k);
        o[4] = n((s(6) * s(8) - s(10) * s(4)) * k);
        o[8] = n((s(9) * s(4) - s(5) * s(8)) * k);
        o[1] = n((s(9) * s(2) - s(10) * s(1)) * k);
        o[5] = n((s(10) * s(0) - s(8) * s(2)) * k);
        o[9] = n((s(8) * s(1) - s(9) * s(0)) * k);
        o[2] = n((s(6) * s(1) - s(5) * s(2)) * k);
        o[6] = n((s(2) * s(4) - s(6) * s(0)) * k);
        o[10] = n((s(5) * s(0) - s(1) * s(4)) * k);
        n(k)
    }

    /// The rotation this matrix's upper 3 x 3 holds, its rows the axes, made unit length.
    #[must_use]
    pub fn quaternion_from_rotation(&self) -> Quat {
        let m = &self.0;
        set_rotation_axes_and_normalize(&[m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]])
    }

    /// Becomes the transpose of `src`.
    // D2-13284
    pub fn transpose(&mut self, src: &Self) {
        for r in 0..4 {
            for c in 0..4 {
                self.0[r * 4 + c] = src.0[c * 4 + r];
            }
        }
    }

    /// Becomes `src`'s adjoint divided by the adjoint's own determinant (the client takes the
    /// determinant of the matrix it has just written), answering that determinant; one under
    /// 0.0002 across leaves the adjoint and answers zero.
    // D2-13285
    pub fn inverse(&mut self, src: &Self) -> f32 {
        self.adjoint(src);
        let det = self.determinant_wide();
        if det.abs() < w(EPS) {
            return 0.0;
        }
        let k = 1.0 / det;
        for v in &mut self.0 {
            *v = n(k * w(*v));
        }
        n(det)
    }

    /// Becomes the rotation `q`: its rows the rotation's axes.
    pub fn rotation_from_quaternion(&mut self, q: Quat) {
        let a = rotation_axes(q);
        self.rotate_basis([a[0], a[1], a[2]], [a[3], a[4], a[5]], [a[6], a[7], a[8]]);
    }

    /// Becomes the rotation with rows `x`, `y` and `z`.
    fn rotate_basis(&mut self, x: [f32; 3], y: [f32; 3], z: [f32; 3]) {
        self.0 = IDENTITY;
        self.0[0..3].copy_from_slice(&x);
        self.0[4..7].copy_from_slice(&y);
        self.0[8..11].copy_from_slice(&z);
    }

    /// Becomes the frame at `eye` looking at `at`, kept upright toward `up` as far as the
    /// direction allows (rows: right, the direction looked along, up; the last row `eye`). A
    /// direction of no length, or one with no up square to it, leaves the rotation as it was;
    /// the eye is set either way.
    // D2-13289
    pub fn look_at(&mut self, eye: Vec3, at: Vec3, up: Vec3) -> bool {
        let d = Vec3::new(at.x - eye.x, at.y - eye.y, at.z - eye.z);
        if let Some((right, along, top)) = direction_frame(d, up) {
            self.rotate_basis(right, along, top);
        }
        self.0[12] = eye.x;
        self.0[13] = eye.y;
        self.0[14] = eye.z;
        true
    }
}

/// The frame looking along `d` with `up` as near up as allowed: right, along and up, each unit
/// length (up the given up with its share along the direction taken out, else the world's up,
/// else north). None for a direction under 0.0002 long or no up to be found.
fn direction_frame(d: Vec3, up: Vec3) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
    let len = n((w(d.z) * w(d.z) + w(d.y) * w(d.y) + w(d.x) * w(d.x)).sqrt());
    if len < EPS {
        return None;
    }
    let k = w(n(1.0 / w(len)));
    let (ax, ay, az) = (n(k * w(d.x)), n(k * w(d.y)), n(k * w(d.z)));
    let dot = w(ay) * w(up.y) + w(az) * w(up.z) + w(ax) * w(up.x);
    let mut ux = n(w(up.x) - w(ax) * dot);
    let mut uy = n(w(up.y) - w(ay) * dot);
    let mut uz = n(w(up.z) - w(az) * dot);
    let length = |x: f32, y: f32, z: f32| n((w(x) * w(x) + w(y) * w(y) + w(z) * w(z)).sqrt());
    let mut l = length(ux, uy, uz);
    if l < EPS {
        ux = n(-(w(az) * w(ax)));
        uy = n(-(w(az) * w(ay)));
        uz = n(1.0 - w(az) * w(az));
        l = length(ux, uy, uz);
        if l < EPS {
            ux = n(-(w(ay) * w(ax)));
            uy = n(1.0 - w(ay) * w(ay));
            uz = n(-(w(ay) * w(az)));
            l = length(ux, uy, uz);
            if l < EPS {
                return None;
            }
        }
    }
    let k = w(n(1.0 / w(l)));
    let (ux, uy, uz) = (n(k * w(ux)), n(w(uy) * k), n(w(uz) * k));
    let right = [
        n(w(uz) * w(ay) - w(az) * w(uy)),
        n(w(az) * w(ux) - w(uz) * w(ax)),
        n(w(ax) * w(uy) - w(ux) * w(ay)),
    ];
    Some((right, [ax, ay, az], [ux, uy, uz]))
}

/// The rotation whose axes are the columns of `m` (laid out as [`rotation_axes`] gives them),
/// made unit length when its squared length is over 0.0002 squared (both kept wide).
// D2-13256
#[must_use]
pub fn set_rotation_axes_and_normalize(m: &[f32; 9]) -> Quat {
    let q = rotation_from_axes(m);
    let n2 = ((w(q.w) * w(q.w) + w(q.x) * w(q.x)) + w(q.y) * w(q.y)) + w(q.z) * w(q.z);
    if n2 > w(EPS) * w(EPS) {
        let k = 1.0 / n2.sqrt();
        Quat::new(n(k * w(q.w)), n(k * w(q.x)), n(k * w(q.y)), n(k * w(q.z)))
    } else {
        q
    }
}
