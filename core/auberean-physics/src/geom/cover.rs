//! The rays a line-of-sight check casts from a detection's origin to cover an object: to the middle
//! of each of its spheres and to points around it across the line of sight, or to the middle of the
//! box around it and to the corners, edge middles and face middles of that box that face the
//! origin. The more rays the check's resolution asks for, the more of the object they cover.
//!
//! The client works these out at double precision and stores single floats where it stores them;
//! the sums and products here follow it.

use dereth_primitives::shape::Sphere;
use dereth_primitives::{Position, Vec3};

use super::{point_local_to_local, point_to_global_from, BBox, Ray, EPS};

fn wide(v: f32) -> f64 {
    f64::from(v)
}

#[allow(clippy::cast_possible_truncation)]
fn narrow(v: f64) -> f32 {
    v as f32
}

/// A squared length under this (2^-46) leaves no direction to scale.
const TINY: f32 = f32::from_bits(0x2880_0000);

/// A vector scaled to unit length by a reciprocal square root and one refining step, as the
/// client's vector library does it; one whose squared length is under 2^-46 becomes zero.
fn unit(v: Vec3) -> Vec3 {
    let n = (v.x * v.x + v.y * v.y) + v.z * v.z;
    if TINY > n {
        return Vec3::ZERO;
    }
    let r = 1.0 / n.sqrt();
    let k = (0.5 * r) * (3.0 - (n * r) * r);
    Vec3::new(k * v.x, k * v.y, k * v.z)
}

/// The ray from `from`'s place to the point `offset` of `to`'s frame, in `from`'s landblock frame.
// D2-13190
#[must_use]
pub fn create_ray(from: &Position, to: &Position, offset: Vec3) -> Ray {
    let g = point_to_global_from(from, to, offset);
    let o = from.frame.origin;
    Ray::from_offset(o, Vec3::new(g.x - o.x, g.y - o.y, g.z - o.z))
}

/// The rays from `origin` covering sphere `s` of an object standing at `at` (the sphere as it is,
/// not scaled): from inside the sphere one ray of no length; else one to its middle, and with a
/// resolution over 1 (and the middle not on the origin) four more to its rim across the line of
/// sight, with a resolution over 2 four more at 0.7 of its radius between those, and with a
/// resolution over 3 eight more at half of each of those.
///
/// Across the line of sight the first direction is level (or, looking straight up or down, in
/// the line's own vertical plane) and the second is the line crossed with it.
// D2-13147
pub fn sphere_cover_rays(
    s: &Sphere,
    at: &Position,
    origin: &Position,
    resolution: u32,
    out: &mut Vec<Ray>,
) {
    let c = point_to_global_from(origin, at, s.center);
    let o = origin.frame.origin;
    let d = Vec3::new(c.x - o.x, c.y - o.y, c.z - o.z);
    let d2 = (wide(d.z) * wide(d.z) + wide(d.y) * wide(d.y)) + wide(d.x) * wide(d.x);
    let r = wide(s.radius);
    if r * r > d2 {
        out.push(Ray::from_offset(o, Vec3::ZERO));
        return;
    }
    out.push(Ray::from_offset(o, d));
    // A length within 0.0002 leaves no direction; one that is not a number goes on.
    if resolution <= 1 || d2 < wide(EPS) {
        return;
    }
    let u = unit(d);
    let v = if EPS < u.x.abs() || EPS < u.y.abs() {
        unit(Vec3::new(-u.y, u.x, 0.0))
    } else {
        unit(Vec3::new(-u.z, 0.0, u.x))
    };
    let w = Vec3::new(
        narrow(wide(v.z) * wide(u.y) - wide(v.y) * wide(u.z)),
        narrow(wide(v.x) * wide(u.z) - wide(v.z) * wide(u.x)),
        narrow(wide(v.y) * wide(u.x) - wide(v.x) * wide(u.y)),
    );
    let k = s.radius;
    let big_v = Vec3::new(v.x * k, v.y * k, v.z * k);
    let big_w = Vec3::new(w.x * k, w.y * k, w.z * k);
    let mut ray_to = |p: Vec3| out.push(Ray::segment(o, p));
    let plus = |a: Vec3| Vec3::new(c.x + a.x, c.y + a.y, c.z + a.z);
    let minus = |a: Vec3| Vec3::new(c.x - a.x, c.y - a.y, c.z - a.z);
    ray_to(plus(big_v));
    ray_to(minus(big_v));
    ray_to(plus(big_w));
    ray_to(minus(big_w));
    if resolution == 2 {
        return;
    }
    // Between the four: 0.7 of each, summed and differenced; the client keeps some of the
    // products whole and stores others first, as here.
    let seven = 0.7_f32;
    let whole = |a: f32| wide(a) * wide(seven);
    let stored = |a: f32| wide(narrow(wide(a) * wide(seven)));
    let a = Vec3::new(
        narrow(whole(big_v.x) + stored(big_w.x)),
        narrow(whole(big_v.y) + whole(big_w.y)),
        narrow(stored(big_v.z) + whole(big_w.z)),
    );
    let b = Vec3::new(
        narrow(stored(big_v.x) - stored(big_w.x)),
        narrow(stored(big_v.y) - whole(big_w.y)),
        narrow(stored(big_v.z) - stored(big_w.z)),
    );
    ray_to(plus(a));
    ray_to(minus(a));
    ray_to(plus(b));
    ray_to(minus(b));
    if resolution == 3 {
        return;
    }
    let half = |a: Vec3| Vec3::new(a.x * 0.5, a.y * 0.5, a.z * 0.5);
    for p in [half(big_v), half(big_w), half(a), half(b)] {
        ray_to(plus(p));
        ray_to(minus(p));
    }
}

/// The rays from `origin` covering box `b` of an object standing at `at` (the box in the object's
/// frame): one to its middle; with a resolution over 1, from inside the box one ray of no length;
/// else one to each corner on a side the origin is beyond or not short of, and with a resolution
/// over 2 one to the middle of each edge both of whose corners were taken and of each face all
/// four of whose corners were.
///
/// A corner is taken on an axis when the origin lies beyond the box's greatest side by more than
/// 0.0002 (the greatest corners) or not (the least corners). The client takes the least-`x`,
/// greatest-`y`, least-`z` corner where the greatest-`z` one belongs when the origin is above the
/// box, and never the least-`x`, greatest-`y`, greatest-`z` corner for its height alone; kept.
// D2-13239
pub fn box_cover_rays(
    b: &BBox,
    at: &Position,
    origin: &Position,
    resolution: u32,
    out: &mut Vec<Ray>,
) {
    let (lo, hi) = (b.min, b.max);
    let mid = |a: f32, c: f32| (a + c) * 0.5;
    let (mx, my, mz) = (mid(lo.x, hi.x), mid(lo.y, hi.y), mid(lo.z, hi.z));
    out.push(create_ray(origin, at, Vec3::new(mx, my, mz)));
    if resolution == 1 {
        return;
    }
    let l = point_local_to_local(at, origin, Vec3::ZERO);
    let past = |v: f32, lo: f32, hi: f32| {
        let mut d = 0.0;
        if v > hi {
            d = v - hi;
        }
        if v < lo {
            d = v - lo;
        }
        d
    };
    let dx = past(l.x, lo.x, hi.x);
    let dy = past(l.y, lo.y, hi.y);
    // The height's difference is kept whole.
    let mut dz = 0.0_f64;
    if l.z > hi.z {
        dz = wide(l.z) - wide(hi.z);
    }
    if l.z < lo.z {
        dz = wide(l.z) - wide(lo.z);
    }
    if EPS > dx.abs() && EPS > dy.abs() && wide(EPS) > dz.abs() {
        out.push(create_ray(origin, origin, Vec3::ZERO));
        return;
    }
    let mut ray_to = |p: Vec3| out.push(create_ray(origin, at, p));
    let e = wide(EPS);
    let (px, nx) = (dx > EPS, dx < EPS);
    let (py, ny) = (dy > EPS, dy < EPS);
    let (pz, nz) = (dz > e, dz < e);
    let c000 = nx || ny || nz;
    let c001 = nx || ny || pz;
    let c010 = nx || py || pz || nz;
    let c011 = nx || py;
    let c100 = px || ny || nz;
    let c101 = px || ny || pz;
    let c110 = px || py || nz;
    let c111 = px || py || pz;
    let corners = [
        (c000, Vec3::new(lo.x, lo.y, lo.z)),
        (c001, Vec3::new(lo.x, lo.y, hi.z)),
        (c010, Vec3::new(lo.x, hi.y, lo.z)),
        (c011, Vec3::new(lo.x, hi.y, hi.z)),
        (c100, Vec3::new(hi.x, lo.y, lo.z)),
        (c101, Vec3::new(hi.x, lo.y, hi.z)),
        (c110, Vec3::new(hi.x, hi.y, lo.z)),
        (c111, Vec3::new(hi.x, hi.y, hi.z)),
    ];
    for (taken, p) in corners {
        if taken {
            ray_to(p);
        }
    }
    if resolution == 2 {
        return;
    }
    let middles = [
        (c000 && c001, Vec3::new(lo.x, lo.y, mz)),
        (c010 && c011, Vec3::new(lo.x, hi.y, mz)),
        (c100 && c101, Vec3::new(hi.x, lo.y, mz)),
        (c110 && c111, Vec3::new(hi.x, hi.y, mz)),
        (c000 && c010, Vec3::new(lo.x, my, lo.z)),
        (c001 && c011, Vec3::new(lo.x, my, hi.z)),
        (c100 && c110, Vec3::new(hi.x, my, lo.z)),
        (c101 && c111, Vec3::new(hi.x, my, hi.z)),
        (c000 && c100, Vec3::new(mx, lo.y, lo.z)),
        (c001 && c101, Vec3::new(mx, lo.y, hi.z)),
        (c010 && c110, Vec3::new(mx, hi.y, lo.z)),
        (c011 && c111, Vec3::new(mx, hi.y, hi.z)),
        (c000 && c001 && c010 && c011, Vec3::new(lo.x, my, mz)),
        (c100 && c101 && c110 && c111, Vec3::new(hi.x, my, mz)),
        (c000 && c001 && c100 && c101, Vec3::new(mx, lo.y, mz)),
        (c010 && c011 && c110 && c111, Vec3::new(mx, hi.y, mz)),
        (c000 && c010 && c100 && c110, Vec3::new(mx, my, lo.z)),
        (c001 && c011 && c101 && c111, Vec3::new(mx, my, hi.z)),
    ];
    for (taken, p) in middles {
        if taken {
            ray_to(p);
        }
    }
}
