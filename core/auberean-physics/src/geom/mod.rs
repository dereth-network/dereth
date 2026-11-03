//! The geometry the motion code reasons in: headings (degrees clockwise from north, kept in
//! `[0, 360)`), and offsets, distances, directions and blends between positions in this world's
//! 160 m landblocks.
//!
//! A position's frame turns its local axes into the landblock's: the local `+y` axis is where the
//! body faces, `+x` its right, `+z` up. The rotation's matrix is worked out from its quaternion
//! here, in this world's own operand order.
//!
//! The shapes everything else tests with are here too: rays ([`ray`]), planes ([`plane`]) and
//! boxes ([`bbox`]); and the 4 x 4 matrix transforms are built with ([`matrix`]).

use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::rules::BLOCK_LENGTH;

pub mod bbox;
pub mod cover;
pub mod frame;
pub mod matrix;
pub mod plane;
pub mod polygon;
pub mod position;
pub mod ray;
pub mod shape;
pub mod sphere;
pub mod tree;

pub use bbox::{BBox, BoxOps};
pub use frame::{
    frame_facing, frame_matrix, frame_pitch, frame_slerp, orient_frame, quat_transform,
    rotation_axis, set_pitch,
};
pub use matrix::Matrix4;
pub use plane::{polygon_trace_ray, side_of, Plane, PlaneOps, Polygon, Side};
pub use ray::Ray;
pub use shape::{Cone, CylSphere, CylinderOps, PlaneCone, Sphere, SphereOps};

/// Below this a length counts as zero.
pub const EPS: f32 = 0.0002;
/// Below this a squared length counts as zero.
pub const EPS_SQ: f32 = 3.999_999_6e-8;
/// Degrees in a radian, as this world stores it.
const DEGREES_PER_RADIAN: f32 = 57.295_78;
/// Radians in a degree, as this world stores it.
const RADIANS_PER_DEGREE: f32 = 0.017_453_292;
/// Five quarter turns, in radians: a direction's angle from east, taken from this, is its heading.
const FIVE_QUARTERS: f64 = 7.853_981_633_974_483;

/// Degrees brought into `[0, 360)`: the remainder of a division by 360, a negative one turned
/// positive, and a value that rounds up to a full turn read as none.
fn wrap(degrees: f64) -> f32 {
    let mut d = degrees % 360.0;
    if d < 0.0 || d.is_nan() {
        d += 360.0;
        if d >= 360.0 {
            d = 0.0;
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    {
        d as f32
    }
}

/// A number as C's `%.3f` writes it in the client's library, infinities and not-a-numbers rounded
/// at the same place.
#[must_use]
pub fn fixed_three(v: f32) -> String {
    if v.is_nan() {
        return if v.is_sign_negative() {
            "-1.#IO"
        } else {
            "1.#QO"
        }
        .to_owned();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-1.#IO" } else { "1.#IO" }.to_owned();
    }
    format!("{:.3}", f64::from(v))
}

/// A vector as text: `(x, y, z)`, three places each.
// D2-13128, D2-13129
#[must_use]
pub fn vector_text(v: Vec3) -> String {
    format!(
        "({}, {}, {})",
        fixed_three(v.x),
        fixed_three(v.y),
        fixed_three(v.z)
    )
}

/// A plane as text: its normal, then its distance, and the end of the line.
#[must_use]
pub fn plane_text(plane: &Plane) -> String {
    format!(
        "N = {}, d = {}\n",
        vector_text(plane.normal),
        fixed_three(plane.d)
    )
}

/// A heading: degrees clockwise from north. A copy is the same heading.
// D2-13349, D2-13350
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Heading(f32);

impl Heading {
    /// North.
    // D2-13356
    pub const ZERO: Self = Self(0.0);
    /// How close two headings must be to be the same: 0.0002 radians.
    pub const EPSILON: Self = Self(0.011_459_156);
    /// How far off its course a body may face and still walk on toward its destination.
    pub const WALK_CONE: Self = Self(90.0);
    /// How close a turn must come to its heading to be done.
    pub const TURN_CLOSE_ENOUGH: Self = Self(10.0);

    /// A heading of `degrees`, brought into `[0, 360)`.
    // D2-13354, D2-13357, D2-13353
    #[must_use]
    pub fn new(degrees: f32) -> Self {
        Self(wrap(f64::from(degrees)))
    }

    /// A heading of `degrees` worked out in double precision, brought into `[0, 360)`.
    #[must_use]
    pub fn from_f64(degrees: f64) -> Self {
        Self(wrap(degrees))
    }

    /// The heading as a byte, a 255th of a turn each, rounded down.
    // D2-13348
    #[must_use]
    pub fn to_u8(self) -> u8 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let b = (f64::from(self.0) * f64::from(0.002_777_777_8_f32) * 255.0) as i64 as u8;
        b
    }

    /// The heading a byte stands for.
    // D2-13355
    #[must_use]
    pub fn from_u8(b: u8) -> Self {
        Self::from_f64(f64::from(b) * f64::from(1.0_f32 / 255.0) * 360.0)
    }

    /// A heading of `radians`.
    #[must_use]
    pub fn from_radians(radians: f32) -> Self {
        Self::new(radians * DEGREES_PER_RADIAN)
    }

    /// The heading's degrees, `[0, 360)`.
    // D2-15464
    #[must_use]
    pub const fn degrees(self) -> f32 {
        self.0
    }

    /// The heading's degrees from -180 to 180.
    // D2-13345
    #[must_use]
    pub fn degrees_centered(self) -> f32 {
        if self.0 > 180.0 {
            self.0 - 360.0
        } else {
            self.0
        }
    }

    // D2-13346
    #[must_use]
    pub fn radians(self) -> f32 {
        self.0 * RADIANS_PER_DEGREE
    }

    /// The heading's radians from -pi to pi: past half a turn it counts back from a whole one.
    // D2-13347
    #[must_use]
    pub fn radians_centered(self) -> f32 {
        let r = f64::from(RADIANS_PER_DEGREE) * f64::from(self.0);
        let r = if r > core::f64::consts::PI {
            r - f64::from(core::f32::consts::TAU)
        } else {
            r
        };
        #[allow(clippy::cast_possible_truncation)]
        {
            r as f32
        }
    }

    /// This heading turned on by `other`, `[0, 360)`.
    // D2-13351
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self(wrap(f64::from(self.0) + f64::from(other.0)))
    }

    /// How far this heading is turned from `other`, `[0, 360)`.
    // D2-13352
    #[must_use]
    pub fn minus(self, other: Self) -> Self {
        Self(wrap(f64::from(self.0) - f64::from(other.0)))
    }

    /// The size of a turn, whichever way: 0 to 180.
    // D2-13358
    #[must_use]
    pub fn abs(self) -> Self {
        let d = if self.0 > 180.0 {
            f64::from(self.0) - 360.0
        } else {
            f64::from(self.0)
        };
        Self(wrap(d.abs()))
    }

    /// The heading `t` of the way from `a` to `b`, the short way round.
    // D2-13359
    #[must_use]
    pub fn lerp(a: Self, b: Self, t: f32) -> Self {
        let mut d = wrap(f64::from(b.0) - f64::from(a.0));
        if d > 180.0 {
            #[allow(clippy::cast_possible_truncation)]
            {
                d = (f64::from(d) - 360.0) as f32;
            }
        }
        Self(wrap(f64::from(d) * f64::from(t) + f64::from(a.0)))
    }

    /// Whether this heading is within `tolerance` of `center`, either way.
    // D2-13360
    #[must_use]
    pub fn is_in_range(self, center: Self, tolerance: Self) -> bool {
        self.minus(center).abs().0 < tolerance.0
    }

    /// Whether this turn is smaller than `tolerance`.
    // D2-13361
    #[must_use]
    pub fn is_inside(self, tolerance: Self) -> bool {
        self.is_in_range(Self::ZERO, tolerance)
    }

    // D2-13362
    #[must_use]
    pub fn equals(self, other: Self) -> bool {
        self.is_in_range(other, Self::EPSILON)
    }

    // D2-13363
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.is_in_range(Self::ZERO, Self::EPSILON)
    }

    /// The compass point nearest this heading: within 11.25 degrees of north, south, east or west,
    /// its letter; else, in the quarter it falls in, the two letters of the point between them, within
    /// 11.25 degrees of it, or those with the nearer of the two letters first.
    // D2-13364
    #[must_use]
    pub fn compass(self) -> String {
        const WITHIN: f32 = 11.25;
        for (center, letter) in [(0.0, "N"), (180.0, "S"), (90.0, "E"), (270.0, "W")] {
            if self.is_in_range(Self(center), Self(WITHIN)) {
                return letter.to_owned();
            }
        }
        let north = self.abs().0 < 90.0;
        let centered = |h: f32| if h > 180.0 { h - 360.0 } else { h };
        let east = centered(self.0) > 0.0;
        let middle = Self::lerp(
            Self(if north { 0.0 } else { 180.0 }),
            Self(if east { 90.0 } else { 270.0 }),
            0.5,
        );
        let past = centered(self.minus(middle).0);
        let side = if north { 'N' } else { 'S' };
        let along = if east { 'E' } else { 'W' };
        if past > WITHIN {
            // Clockwise of the middle of the quarter.
            let first = match (north, east) {
                (true, true) => 'E',
                (true, false) => 'N',
                (false, true) => 'S',
                (false, false) => 'W',
            };
            format!("{first}{side}{along}")
        } else if past < -WITHIN {
            let first = match (north, east) {
                (true, true) => 'N',
                (true, false) => 'W',
                (false, true) => 'E',
                (false, false) => 'S',
            };
            format!("{first}{side}{along}")
        } else {
            format!("{side}{along}")
        }
    }

    /// The heading of a direction in the ground plane (`+y` north, `+x` east), from its angle.
    #[must_use]
    fn of_angle(angle: f64) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        Self::from_radians((FIVE_QUARTERS - angle) as f32)
    }
}

/// The offset from one landblock's frame to another's (cell id 0 standing for the block at the
/// map's corner); none within a block.
// D2-01968
#[must_use]
pub fn block_offset(from: CellId, to: CellId) -> Vec3 {
    if (from.0 == 0 && to.0 == 0) || (from.0 != 0 && to.0 != 0 && from.0 >> 16 == to.0 >> 16) {
        return Vec3::ZERO;
    }
    let (fx, fy) = (from.0 >> 24, (from.0 >> 16) & 0xFF);
    let (tx, ty) = (to.0 >> 24, (to.0 >> 16) & 0xFF);
    let (dx, dy) = (f64::from(tx) - f64::from(fx), f64::from(ty) - f64::from(fy));
    #[allow(clippy::cast_possible_truncation)]
    Vec3::new(dx as f32 * BLOCK_LENGTH, dy as f32 * BLOCK_LENGTH, 0.0)
}

/// From `from` to `to`, in `from`'s landblock frame.
// D2-03291
#[must_use]
pub fn offset(from: &Position, to: &Position) -> Vec3 {
    let b = block_offset(from.cell, to.cell);
    let (f, t) = (from.frame.origin, to.frame.origin);
    Vec3::new((b.x + t.x) - f.x, (b.y + t.y) - f.y, (b.z + t.z) - f.z)
}

/// The straight-line distance between two positions.
// D2-13177
#[must_use]
pub fn distance(a: &Position, b: &Position) -> f32 {
    let o = offset(a, b);
    (o.z * o.z + o.y * o.y + o.x * o.x).sqrt()
}

/// The distance between two upright cylinders (radius, height, standing at a position): the
/// gap across the ground plane past both radii and the gap in height past the lower one's top,
/// each at least zero, put together.
// D2-13187
#[must_use]
pub fn cylinder_distance(r1: f32, h1: f32, p1: &Position, r2: f32, h2: f32, p2: &Position) -> f32 {
    let o = offset(p1, p2);
    let across = ((o.x * o.x + o.y * o.y).sqrt() - (r1 + r2)).max(0.0);
    let (z1, z2) = (p1.frame.origin.z, p2.frame.origin.z);
    let (top, high) = if z1 <= z2 {
        (z1 + h1, z2)
    } else {
        (z2 + h2, z1)
    };
    let up = (high - top).max(0.0);
    (across * across + up * up).sqrt()
}

/// The heading from `from` toward `to` across the ground; north when they stand over each other.
// D2-13181
#[must_use]
pub fn heading_to(from: &Position, to: &Position) -> Heading {
    let o = offset(from, to);
    heading_of(o.x, o.y)
}

/// The heading of a direction across the ground (`+y` north, `+x` east); north when it is shorter
/// than 0.0002. Worked out in wide precision, the east part rounded to a float on the way.
#[must_use]
pub fn heading_of(x: f32, y: f32) -> Heading {
    let (x, y) = (f64::from(x), f64::from(y));
    let len = (x * x + y * y).sqrt();
    if len >= f64::from(EPS) {
        let inv = 1.0 / len;
        #[allow(clippy::cast_possible_truncation)]
        let xs = f64::from((x * inv) as f32);
        Heading::of_angle(math::atan2(inv * y, xs))
    } else {
        Heading::ZERO
    }
}

/// The rotation's matrix: three columns, the local `x`, `y` and `z` axes in the landblock's
/// frame, as nine numbers `[x.x, x.y, x.z, y.x, y.y, y.z, z.x, z.y, z.z]`.
// D2-13253, D2-13159
#[must_use]
pub fn rotation_axes(q: Quat) -> [f32; 9] {
    let (x2, y2, z2) = (q.x + q.x, q.y + q.y, q.z + q.z);
    // Six products are kept as 32-bit floats; the three of `y` and `z` with themselves are kept
    // whole (in double precision they are exact), and each sum is rounded once.
    let wide = f64::from;
    let (wx, wy, wz) = (wide(x2 * q.w), wide(y2 * q.w), wide(z2 * q.w));
    let (xx, xy, xz) = (wide(x2 * q.x), wide(y2 * q.x), wide(z2 * q.x));
    let yy = wide(y2) * wide(q.y);
    let yz = wide(z2) * wide(q.y);
    let zz = wide(z2) * wide(q.z);
    #[allow(clippy::cast_possible_truncation)]
    let f = |v: f64| v as f32;
    [
        f(1.0 - (zz + yy)),
        f(xy + wz),
        f(xz - wy),
        f(xy - wz),
        f(1.0 - (zz + xx)),
        f(wx + yz),
        f(xz + wy),
        f(yz - wx),
        f(1.0 - (yy + xx)),
    ]
}

/// The rotation whose matrix has these columns, worked in double precision as the client's
/// floating-point unit works it (each part rounded once, as it is stored); the sum of the last
/// two diagonal terms, kept as a float, serves the branches where the trace is negative.
// D2-13257, D2-13160, D2-13252
#[must_use]
pub fn rotation_from_axes(m: &[f32; 9]) -> Quat {
    let w = f64::from;
    #[allow(clippy::cast_possible_truncation)]
    let n = |v: f64| v as f32;
    let t = w(m[8]) + w(m[4]);
    let tr = t + w(m[0]);
    if tr >= 0.0 {
        let s = (tr + 1.0).sqrt();
        let f = 0.5 / s;
        return Quat::new(
            n(0.5 * s),
            n((w(m[5]) - w(m[7])) * f),
            n((w(m[6]) - w(m[2])) * f),
            n((w(m[1]) - w(m[3])) * f),
        );
    }
    let t = w(n(t));
    let big = if m[4] <= m[0] { m[0] } else { m[4] };
    let big = if big < m[8] { m[8] } else { big };
    if big == m[0] {
        let s = ((w(m[0]) - t) + 1.0).sqrt();
        let f = 0.5 / s;
        Quat::new(
            n((w(m[5]) - w(m[7])) * f),
            n(0.5 * s),
            n((w(m[1]) + w(m[3])) * f),
            n((w(m[2]) + w(m[6])) * f),
        )
    } else if big == m[4] {
        let s = ((w(m[4]) - (w(m[0]) + w(m[8]))) + 1.0).sqrt();
        let f = 0.5 / s;
        Quat::new(
            n((w(m[6]) - w(m[2])) * f),
            n((w(m[1]) + w(m[3])) * f),
            n(0.5 * s),
            n((w(m[5]) + w(m[7])) * f),
        )
    } else {
        let s = ((w(m[8]) - (w(m[0]) + w(m[4]))) + 1.0).sqrt();
        let f = 0.5 / s;
        Quat::new(
            n((w(m[1]) - w(m[3])) * f),
            n((w(m[2]) + w(m[6])) * f),
            n((w(m[5]) + w(m[7])) * f),
            n(0.5 * s),
        )
    }
}

/// The sum of three products, worked in double precision and rounded once, as the client's
/// floating-point unit sums a row of a matrix times a vector.
fn dot3(a: f32, x: f32, b: f32, y: f32, c: f32, z: f32) -> f64 {
    let w = f64::from;
    (w(a) * w(x) + w(b) * w(y)) + w(c) * w(z)
}

#[allow(clippy::cast_possible_truncation)]
fn narrow(v: f64) -> f32 {
    v as f32
}

/// A local vector turned into the landblock's frame.
// D2-04111, D2-03471
#[must_use]
pub fn local_to_global(frame: &Frame, v: Vec3) -> Vec3 {
    let m = rotation_axes(frame.rotation);
    Vec3::new(
        narrow(dot3(m[6], v.z, m[0], v.x, m[3], v.y)),
        narrow(dot3(m[7], v.z, m[1], v.x, m[4], v.y)),
        narrow(dot3(m[8], v.z, m[2], v.x, m[5], v.y)),
    )
}

/// A vector in the landblock's frame turned into the frame's own.
// D2-05809, D2-01969
#[must_use]
pub fn global_to_local(frame: &Frame, v: Vec3) -> Vec3 {
    let m = rotation_axes(frame.rotation);
    Vec3::new(
        narrow(dot3(m[2], v.z, m[0], v.x, m[1], v.y)),
        narrow(dot3(m[5], v.z, m[3], v.x, m[4], v.y)),
        narrow(dot3(m[8], v.z, m[6], v.x, m[7], v.y)),
    )
}

/// A point in a frame turned into the landblock's frame: turned, then moved by the frame's origin.
// D2-03795, D2-01970
#[must_use]
pub fn point_to_global(frame: &Frame, v: Vec3) -> Vec3 {
    let m = rotation_axes(frame.rotation);
    let o = frame.origin;
    let w = f64::from;
    Vec3::new(
        narrow(dot3(m[6], v.z, m[0], v.x, m[3], v.y) + w(o.x)),
        narrow(dot3(m[7], v.z, m[1], v.x, m[4], v.y) + w(o.y)),
        narrow(dot3(m[8], v.z, m[2], v.x, m[5], v.y) + w(o.z)),
    )
}

/// A point in `from`'s frame turned into the landblock frame of `to`'s cell.
// D2-04210
#[must_use]
pub fn point_to_global_from(to: &Position, from: &Position, v: Vec3) -> Vec3 {
    let m = rotation_axes(from.frame.rotation);
    let o = from.frame.origin;
    let wide = |a: f32, b: f32, c: f32, origin: f32| {
        let (a, b, c) = (f64::from(a), f64::from(b), f64::from(c));
        #[allow(clippy::cast_possible_truncation)]
        let g = (((a * f64::from(v.y)) + b * f64::from(v.x))
            + c * f64::from(v.z)
            + f64::from(origin)) as f32;
        g
    };
    let g = Vec3::new(
        wide(m[3], m[0], m[6], o.x),
        wide(m[4], m[1], m[7], o.y),
        wide(m[5], m[2], m[8], o.z),
    );
    let b = block_offset(to.cell, from.cell);
    Vec3::new(g.x + b.x, g.y + b.y, g.z + b.z)
}

/// A point in the landblock's frame turned into a frame's own: moved by the frame's origin (each
/// difference kept as a float), then turned back.
// D2-01971
#[must_use]
pub fn point_to_local(frame: &Frame, v: Vec3) -> Vec3 {
    let m = rotation_axes(frame.rotation);
    let o = frame.origin;
    let (dx, dy, dz) = (v.x - o.x, v.y - o.y, v.z - o.z);
    Vec3::new(
        narrow(dot3(dx, m[0], dz, m[2], dy, m[1])),
        narrow(dot3(dx, m[3], dz, m[5], dy, m[4])),
        narrow(dot3(dx, m[6], dz, m[8], dy, m[7])),
    )
}

/// A point in `from`'s frame turned into `to`'s frame, across landblocks.
// D2-01972
#[must_use]
pub fn point_local_to_local(to: &Position, from: &Position, v: Vec3) -> Vec3 {
    point_to_local(&to.frame, point_to_global_from(to, from, v))
}

/// Moves a position by a vector in its own frame.
// D2-07044
pub fn add_local_offset(p: &mut Position, v: Vec3) {
    let g = local_to_global(&p.frame, v);
    p.frame.origin = Vec3::new(
        g.x + p.frame.origin.x,
        g.y + p.frame.origin.y,
        g.z + p.frame.origin.z,
    );
}

/// The point `t` of the way from `a` to `b`, in `a`'s cell.
// D2-05829
#[must_use]
pub fn lerp_origin(a: &Position, b: &Position, t: f32) -> Vec3 {
    let o = block_offset(a.cell, b.cell);
    let (p, q) = (a.frame.origin, b.frame.origin);
    Vec3::new(
        ((o.x + q.x) - p.x) * t + p.x,
        ((o.y + q.y) - p.y) * t + p.y,
        ((o.z + q.z) - p.z) * t + p.z,
    )
}

/// The place `t` of the way from `a` to `b`, in `a`'s cell: the point along the line, the rotation
/// along the shorter arc.
// D2-05830
#[must_use]
pub fn slerp_position(a: &Position, b: &Position, t: f32) -> Position {
    let mut p = *a;
    p.frame.origin = lerp_origin(a, b, t);
    slerp_rotation(&mut p.frame, &a.frame, &b.frame, t);
    p
}

/// The heading a frame faces.
// D2-13164
#[must_use]
pub fn frame_heading(frame: &Frame) -> Heading {
    let m = rotation_axes(frame.rotation);
    Heading::of_angle(math::atan2(f64::from(m[4]), f64::from(m[3])))
}

/// Turns a frame to face along a direction, keeping it upright as far as the direction allows;
/// a direction of no length leaves it unturned. Worked as the client's floating-point unit works
/// it: the facing and the up axis's parts rounded where the client stores them, the up axis's
/// first two parts kept wide for the right axis's cross product.
// D2-13172
pub fn set_vector_heading(frame: &mut Frame, v: Vec3) {
    let w = f64::from;
    let eps = w(EPS);
    let len2 = (w(v.x) * w(v.x) + w(v.y) * w(v.y)) + w(v.z) * w(v.z);
    if len2 < eps * eps {
        set_rotate(frame, Quat::IDENTITY);
        return;
    }
    let len = ((w(v.z) * w(v.z) + w(v.x) * w(v.x)) + w(v.y) * w(v.y)).sqrt();
    if len < eps {
        set_rotate(frame, Quat::IDENTITY);
        return;
    }
    let k = 1.0 / len;
    let (fx, fy, fz) = (narrow(k * w(v.x)), narrow(w(v.y) * k), narrow(w(v.z) * k));
    // Up, made square to the facing.
    let d = (w(fy) * 0.0 + w(fx) * 0.0) + w(fz);
    let ux = narrow(-(w(fx) * d));
    let uy = -narrow(w(fy) * d);
    let uz_wide = 1.0 - w(narrow(w(fz) * d));
    let uz = narrow(uz_wide);
    let l = narrow(((uz_wide * w(uz)) + w(uy) * w(uy) + w(ux) * w(ux)).sqrt());
    let (upx, upy, upz) = if l >= EPS {
        let k = 1.0 / w(l);
        let kf = w(narrow(k));
        (w(ux) * k, w(uy) * kf, narrow(w(uz) * kf))
    } else {
        // Facing straight up or down: the up axis is taken from north instead.
        let d = (w(fx) * 0.0 + w(fz) * 0.0) + w(fy);
        let ux = -(w(fx) * d);
        let uy = narrow(1.0 - w(narrow(w(fy) * d)));
        let uz = -narrow(w(fz) * d);
        let l = ((w(uz) * w(uz) + w(uy) * w(uy)) + ux * ux).sqrt();
        if l < eps {
            return;
        }
        let kf = w(narrow(1.0 / l));
        (ux * kf, w(uy) * kf, narrow(w(uz) * kf))
    };
    let m = [
        narrow(w(upz) * w(fy) - upy * w(fz)),
        narrow(w(fz) * upx - w(upz) * w(fx)),
        narrow(upy * w(fx) - upx * w(fy)),
        fx,
        fy,
        fz,
        narrow(upx),
        narrow(upy),
        upz,
    ];
    frame.rotation = rotation_from_axes(&m);
}

/// Faces a frame toward a heading, keeping its tilt.
// D2-13173, D2-05828
pub fn set_heading(frame: &mut Frame, h: Heading) {
    let m = rotation_axes(frame.rotation);
    let r = h.radians();
    set_vector_heading(frame, Vec3::new(math::sinf(r), math::cosf(r), m[5]));
}

/// Turns a frame by the rotation vector `v` (its direction the axis, its length the angle in
/// radians) after its rotation, kept unit length. A vector shorter than 0.0002 leaves it.
// D2-13168
pub fn grotate(frame: &mut Frame, v: Vec3) {
    let wide = f64::from;
    #[allow(clippy::cast_possible_truncation)]
    let narrow = |x: f64| x as f32;
    let (x, y, z) = (wide(v.x), wide(v.y), wide(v.z));
    let len2 = x * x + y * y + z * z;
    let least = wide(0.0002_f32);
    if len2 < least * least {
        return;
    }
    let len = len2.sqrt();
    let (sin, cos) = (math::sin(len * 0.5) / len, math::cos(len * 0.5));
    let (ax, ay, az) = (x * sin, y * sin, z * sin);
    let b = frame.rotation;
    let (bw, bx, by, bz) = (wide(b.w), wide(b.x), wide(b.y), wide(b.z));
    set_rotate(
        frame,
        Quat::new(
            narrow(cos * bw - ax * bx - ay * by - az * bz),
            narrow(ax * bw + cos * bx + ay * bz - az * by),
            narrow(ay * bw + cos * by + az * bx - ax * bz),
            narrow(az * bw + cos * bz + ax * by - ay * bx),
        ),
    );
}

/// Sets a frame's rotation, made unit length.
// D2-05771
pub fn set_rotate(frame: &mut Frame, q: Quat) {
    let n2 = q.z * q.z + q.y * q.y + q.x * q.x + q.w * q.w;
    frame.rotation = if n2 > EPS_SQ {
        let k = 1.0 / n2.sqrt();
        Quat::new(k * q.w, k * q.x, k * q.y, k * q.z)
    } else {
        q
    };
}

/// The rotation `t` of the way from `a` to `b`, along the shorter arc.
// D2-13169
pub fn slerp_rotation(frame: &mut Frame, a: &Frame, b: &Frame, t: f32) {
    let (p, mut q) = (a.rotation, b.rotation);
    let mut cos = p.x * q.x + p.y * q.y + p.z * q.z + p.w * q.w;
    if cos < 0.0 {
        cos = -cos;
        q = Quat::new(-q.w, -q.x, -q.y, -q.z);
    }
    let (wa, wb) = if 1.0 - cos > 0.001 {
        let theta = math::acosf(cos);
        let s = math::sinf(theta);
        (math::sinf((1.0 - t) * theta) / s, math::sinf(t * theta) / s)
    } else {
        (1.0 - t, t)
    };
    set_rotate(
        frame,
        Quat::new(
            wa * p.w + wb * q.w,
            wa * p.x + wb * q.x,
            wa * p.y + wb * q.y,
            wa * p.z + wb * q.z,
        ),
    );
}

/// A direction of length at most one as three bytes, its parts held to -1 to 1 (a part that is not
/// a number counting as -1) and each mapped to 0 to 255: east in the high byte, up in the low.
#[must_use]
pub fn unit_vector_to_u24(v: Vec3) -> u32 {
    let byte = |c: f32| {
        let c = if c <= -1.0 || c.is_nan() {
            -1.0
        } else if c >= 1.0 {
            1.0
        } else {
            c
        };
        // Cut toward zero; the low byte kept.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let b = ((f64::from(c) + 1.0) * 127.5) as i64 as u32 & 0xFF;
        b
    };
    byte(v.x) << 16 | byte(v.y) << 8 | byte(v.z)
}

/// Whether a cell id names a cell inside the map: a land cell, an interior cell or a whole block
/// (the block coordinates always lie on the map).
// D2-02541
#[must_use]
pub const fn inbound_valid_cell(cell: CellId) -> bool {
    let low = cell.0 & 0xFFFF;
    (low >= 1 && low <= 64) || (low >= 0x100 && low <= 0xFFFD) || low == 0xFFFF
}

/// Whether a frame is one: no part of its origin or rotation is not a number, and its rotation
/// is a unit within 0.0002 (its square length).
// D2-13157
#[must_use]
pub fn frame_is_valid(frame: &Frame) -> bool {
    let o = frame.origin;
    if o.x.is_nan() || o.y.is_nan() || o.z.is_nan() {
        return false;
    }
    let q = frame.rotation;
    let numbers = !(q.w.is_nan() || q.x.is_nan() || q.y.is_nan() || q.z.is_nan());
    let n = q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z;
    !n.is_nan() && (n - 1.0).abs() < EPS && numbers
}

/// Whether a place is in the world: its cell inside the map and its frame a frame.
// D2-13189
#[must_use]
pub fn position_is_valid(p: &Position) -> bool {
    inbound_valid_cell(p.cell) && frame_is_valid(&p.frame)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the geometry helpers, held to hand-worked values)
    use super::*;

    fn at(cell: u32, x: f32, y: f32, z: f32) -> Position {
        Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
    }

    // Headings wrap into a turn and subtract the short way.
    #[test]
    fn headings_wrap_and_compare() {
        assert_eq!(Heading::new(-90.0).degrees(), 270.0);
        assert_eq!(Heading::new(720.0).degrees(), 0.0);
        assert_eq!(
            Heading::new(350.0).minus(Heading::new(10.0)).degrees(),
            340.0
        );
        assert_eq!(
            Heading::new(350.0)
                .minus(Heading::new(10.0))
                .abs()
                .degrees(),
            20.0
        );
        assert!(Heading::new(355.0).is_in_range(Heading::new(5.0), Heading::new(10.1)));
        assert!(!Heading::new(355.0).is_in_range(Heading::new(5.0), Heading::new(10.0)));
        assert_eq!(
            Heading::lerp(Heading::new(350.0), Heading::new(10.0), 0.5).degrees(),
            0.0
        );
    }

    // Offsets cross 160 m landblocks; the heading from one point to another is clockwise from north.
    #[test]
    fn offsets_cross_blocks_and_headings_run_clockwise() {
        let a = at(0x1010_0001, 150.0, 10.0, 0.0);
        let b = at(0x1110_0001, 10.0, 10.0, 0.0);
        assert_eq!(offset(&a, &b), Vec3::new(20.0, 0.0, 0.0));
        assert_eq!(distance(&a, &b), 20.0);
        assert!((heading_to(&a, &b).degrees() - 90.0).abs() < 1e-4);
        let north = at(0x1011_0001, 150.0, 5.0, 0.0);
        assert!(heading_to(&a, &north).degrees().abs() < 1e-4);
    }

    // A frame turned to a heading faces it, and the matrix round-trips through the quaternion.
    #[test]
    fn frames_face_the_heading_they_are_set_to() {
        let mut f = Frame::new(Vec3::ZERO, Quat::IDENTITY);
        set_heading(&mut f, Heading::new(30.0));
        assert!((frame_heading(&f).degrees() - 30.0).abs() < 1e-3);
        let back = rotation_from_axes(&rotation_axes(f.rotation));
        assert!((back.w - f.rotation.w).abs() < 1e-6 && (back.z - f.rotation.z).abs() < 1e-6);
    }

    // Cylinders apart across the ground and in height add up as the two sides of a triangle.
    #[test]
    fn cylinder_distance_is_the_gap_past_radii_and_heights() {
        let a = at(0x1010_0001, 0.0, 0.0, 0.0);
        let b = at(0x1010_0001, 4.0, 0.0, 6.0);
        assert_eq!(cylinder_distance(0.5, 2.0, &a, 0.5, 2.0, &b), 5.0);
        assert_eq!(cylinder_distance(3.0, 7.0, &a, 3.0, 2.0, &b), 0.0);
    }

    /// A frame turned by a rotation vector stays a unit, and a tiny one leaves it be.
    #[test]
    fn a_frame_turns_by_a_rotation_vector() {
        let mut f = Frame::new(Vec3::ZERO, Quat::IDENTITY);
        grotate(&mut f, Vec3::new(0.0, 0.0, std::f32::consts::FRAC_PI_2));
        let half = std::f32::consts::FRAC_1_SQRT_2;
        assert!((f.rotation.w - half).abs() < 1e-5, "{:?}", f.rotation);
        assert!((f.rotation.z - half).abs() < 1e-5, "{:?}", f.rotation);
        let before = f.rotation;
        grotate(&mut f, Vec3::new(0.0, 0.0001, 0.0));
        assert_eq!(f.rotation, before);
    }
}
