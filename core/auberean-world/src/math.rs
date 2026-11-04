//! The little vector and matrix arithmetic the world needs: points, quaternion rotations and 4 x 4
//! transforms in the row-vector convention (`p' = p · M`, so `A · B` applies `A` first).

use auberean_primitives::{Quat, Vec3};

/// Component-wise vector helpers on the shared vector type.
#[must_use]
pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

#[must_use]
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

#[must_use]
pub fn scale(a: Vec3, s: f32) -> Vec3 {
    Vec3::new(a.x * s, a.y * s, a.z * s)
}

#[must_use]
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

#[must_use]
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

#[must_use]
pub fn length(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

/// `a / |a|`, or `a` itself when it has no length.
#[must_use]
pub fn normalize(a: Vec3) -> Vec3 {
    let l = length(a);
    if l > 0.0 {
        scale(a, 1.0 / l)
    } else {
        a
    }
}

/// A 4 x 4 transform, row-vector convention: rows 0-2 are the images of the x, y and z axes,
/// row 3 is the translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [[f32; 4]; 4]);

impl Mat4 {
    pub const IDENTITY: Self = Self([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    /// Scale, then rotate by `q`, then move to `origin`: `diag(s) · R(q) · T(origin)`.
    // D2-13266
    #[must_use]
    pub fn from_parts(origin: Vec3, q: Quat, s: Vec3) -> Self {
        let r = rotation_rows(q);
        Self([
            [r[0][0] * s.x, r[0][1] * s.x, r[0][2] * s.x, 0.0],
            [r[1][0] * s.y, r[1][1] * s.y, r[1][2] * s.y, 0.0],
            [r[2][0] * s.z, r[2][1] * s.z, r[2][2] * s.z, 0.0],
            [origin.x, origin.y, origin.z, 1.0],
        ])
    }

    /// A pure translation.
    #[must_use]
    pub fn translation(t: Vec3) -> Self {
        Self::from_parts(t, Quat::IDENTITY, Vec3::new(1.0, 1.0, 1.0))
    }

    /// A point with its own last part through the matrix: each part the row's sum.
    #[must_use]
    pub fn transform4(&self, v: [f32; 4]) -> [f32; 4] {
        let m = &self.0;
        let row = |k: usize| m[0][k] * v[0] + m[1][k] * v[1] + m[2][k] * v[2] + m[3][k] * v[3];
        [row(0), row(1), row(2), row(3)]
    }

    /// `self · other`: `self` applied first.
    #[must_use]
    pub fn then(&self, other: &Self) -> Self {
        let (a, b) = (&self.0, &other.0);
        let mut out = [[0.0f32; 4]; 4];
        for (i, row) in out.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = (0..4).map(|k| a[i][k] * b[k][j]).sum();
            }
        }
        Self(out)
    }

    /// The image of point `p`.
    #[must_use]
    #[inline]
    pub fn point(&self, p: Vec3) -> Vec3 {
        let m = &self.0;
        let mut out = [0.0f32; 4];
        // A column at a time, the four kept in step so that they go through together.
        for i in 0..4 {
            out[i] = p.x * m[0][i] + p.y * m[1][i] + p.z * m[2][i] + m[3][i];
        }
        Vec3::new(out[0], out[1], out[2])
    }

    /// The point whose image is `p` under this transform, when it is a rotation and a translation
    /// (its first three rows orthonormal).
    #[must_use]
    pub fn rigid_preimage(&self, p: Vec3) -> Vec3 {
        let m = &self.0;
        let q = Vec3::new(p.x - m[3][0], p.y - m[3][1], p.z - m[3][2]);
        Vec3::new(
            q.x * m[0][0] + q.y * m[0][1] + q.z * m[0][2],
            q.x * m[1][0] + q.y * m[1][1] + q.z * m[1][2],
            q.x * m[2][0] + q.y * m[2][1] + q.z * m[2][2],
        )
    }

    /// The image of direction `d` (no translation).
    #[must_use]
    #[inline]
    pub fn direction(&self, d: Vec3) -> Vec3 {
        let m = &self.0;
        let mut out = [0.0f32; 4];
        for i in 0..4 {
            out[i] = d.x * m[0][i] + d.y * m[1][i] + d.z * m[2][i];
        }
        Vec3::new(out[0], out[1], out[2])
    }
}

/// The rotation of unit quaternion `q` (w, x, y, z) as rows: row `i` is the image of axis `i`.
#[must_use]
pub fn rotation_rows(q: Quat) -> [[f32; 3]; 3] {
    let (w, x, y, z) = (q.w, q.x, q.y, q.z);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + w * z),
            2.0 * (x * z - w * y),
        ],
        [
            2.0 * (x * y - w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + w * x),
        ],
        [
            2.0 * (x * z + w * y),
            2.0 * (y * z - w * x),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

/// The Hamilton product `a ⊗ b`: rotate by `b`, then by `a`.
#[must_use]
pub fn quat_mul(a: Quat, b: Quat) -> Quat {
    Quat {
        w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
        x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
        y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
        z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
    }
}

/// The rotation by `angle` radians about the unit `axis`.
#[must_use]
pub fn quat_axis_angle(axis: Vec3, angle: f32) -> Quat {
    let (s, c) = dereth_primitives::num::math::sin_cosf(angle * 0.5);
    Quat {
        w: c,
        x: axis.x * s,
        y: axis.y * s,
        z: axis.z * s,
    }
}

/// How far outside an edge a point may lie, in the edge's own units (its length times the
/// distance), and still count as over a polygon.
pub const OVER_BAND: f32 = 0.0002;

/// One edge's value at `p`, seen from above: the edge from `prev` to `cur` turned a quarter turn,
/// dotted with `p`'s offset from `cur`. Negative inside a counterclockwise polygon; its size is the
/// edge's length times `p`'s distance from the edge's line. Worked in double precision.
fn edge_value(prev: [f32; 3], cur: [f32; 3], p: [f32; 3]) -> (f64, f64, f64) {
    let a = f64::from(prev[1]) - f64::from(cur[1]);
    let b = f64::from(cur[0]) - f64::from(prev[0]);
    let v = b * f64::from(p[1])
        + a * f64::from(p[0])
        + (-(a * f64::from(cur[0])) - b * f64::from(cur[1]));
    (a, b, v)
}

/// Whether `p` lies over polygon `v` seen from above: on the inner side of every edge, or no more
/// than [`OVER_BAND`] outside it. `facing_up` takes the inner side of a counterclockwise polygon,
/// otherwise of a clockwise one. The edges are walked from the first vertex back through the last.
// D2-04165
#[must_use]
pub fn over_polygon(v: &[[f32; 3]], p: [f32; 3], facing_up: bool) -> bool {
    let Some(&first) = v.first() else {
        return true;
    };
    let mut prev = first;
    for &cur in v.iter().rev() {
        let (_, _, value) = edge_value(prev, cur, p);
        let band = f64::from(OVER_BAND);
        if facing_up {
            if value > band {
                return false;
            }
        } else if value < -band {
            return false;
        }
        prev = cur;
    }
    true
}

/// Whether a circle of radius `r` about `p` reaches polygon `v` (whose normal's height is
/// `normal_z`) seen from above, as the land's slope check asks it. A vertical polygon is never
/// reached. A vertex within `r` of `p` (in all three dimensions) reaches it; otherwise, walking the
/// edges from the first vertex back through the last, an edge whose value at `p` exceeds `r` (the
/// value grows with the edge's length, so a long edge stops the circle short of its own reach)
/// keeps it off, and one `p` is outside of by less, with `p` beside it rather than past either
/// end, reaches it; `p` inside every edge is over it. `facing_up` as for [`over_polygon`].
// D2-04166
#[must_use]
pub fn circle_over_polygon(
    v: &[[f32; 3]],
    normal_z: f32,
    p: [f32; 3],
    facing_up: bool,
    r: f32,
) -> bool {
    if normal_z == 0.0 {
        return false;
    }
    let Some(&first) = v.first() else {
        return true;
    };
    let r = f64::from(r);
    let gap = |q: [f32; 3]| {
        let d = |k: usize| f64::from(q[k]) - f64::from(p[k]);
        d(0) * d(0) + d(1) * d(1) + d(2) * d(2)
    };
    if gap(first) < r * r {
        return true;
    }
    let sign = if facing_up { 1.0 } else { -1.0 };
    let (px, py) = (f64::from(p[0]), f64::from(p[1]));
    let mut inside = true;
    let mut prev = first;
    for &cur in v.iter().rev() {
        if gap(cur) < r * r {
            return true;
        }
        let (a, b, value) = edge_value(prev, cur, p);
        let value = value * sign;
        if r < value {
            return false;
        }
        if value > 0.0 {
            inside = false;
            let (bx, ay) = (b * px, a * py);
            let past_start = ((bx - ay) + (a * f64::from(prev[1]) - b * f64::from(prev[0]))) * sign;
            let short_of_end = ((ay - bx) + (b * f64::from(cur[0]) - a * f64::from(cur[1]))) * sign;
            if past_start >= 0.0 && short_of_end >= 0.0 {
                return true;
            }
        }
        prev = cur;
    }
    inside
}
