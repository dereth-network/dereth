//! The shapes a detection volume can take and their tests against one another: a sphere, an
//! upright cylinder, a box, a sphere cut to a cone, and a shell cut to ranges of heading and
//! elevation. Each answers whether it reaches a sphere, a cylinder or a box, and gives the box
//! around itself in another frame; spheres and cylinders also answer where a ray first meets them.
//!
//! The sphere and the cylinder are the shared ones; what this world does with them is here.

use dereth_primitives::num::math;
// D2-03796, D2-15597
pub use dereth_primitives::shape::{CylSphere, Sphere};
use dereth_primitives::{Position, Vec3};

use super::bbox::{BBox, BoxOps};
use super::{heading_of, point_to_global_from, rotation_axes, Heading, Ray, EPS};

/// A double rounded to a 32-bit float, as retail stores a value its 53-bit arithmetic made.
#[allow(clippy::cast_possible_truncation)]
fn narrow(v: f64) -> f32 {
    v as f32
}

/// A vector brought to unit length, the zero vector left as it is.
fn normalized(v: Vec3) -> Vec3 {
    let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if len == 0.0 {
        return v;
    }
    Vec3::new(v.x / len, v.y / len, v.z / len)
}

/// `v` clamped into `[lo, hi]` the way the client clamps a height: the low bound first.
fn clamp_height(v: f32, lo: f32, hi: f32) -> f32 {
    let mut out = lo;
    if lo <= v {
        out = v;
        if hi < v {
            out = hi;
        }
    }
    out
}

/// Whether the column of spheres of radius `r` (at least 0.0002) along a cylinder's axis, from
/// height `z0` to `z1` in up to ten even steps of about the cylinder's radius, meets a shape.
fn sweep_column(c: &CylSphere, z0: f32, z1: f32, mut hit: impl FnMut(&Sphere) -> bool) -> bool {
    let r = if c.radius < EPS { EPS } else { c.radius };
    let at = |z: f32| Sphere::new(Vec3::new(c.low_pt.x, c.low_pt.y, z), r);
    if z0 == z1 {
        return hit(&at(z0));
    }
    #[allow(clippy::cast_possible_truncation)]
    let mut n = f64::from((z1 - z0).abs() / c.radius).ceil() as f32;
    // Negated so that a NaN count takes the least, as the client's compare does.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(1.0 <= n) {
        n = 1.0;
    } else if n > 10.0 {
        n = 10.0;
    }
    let step = (z1 - z0) / n;
    let mut k = 0.0_f32;
    while k <= n + EPS {
        if hit(&at(k * step + z0)) {
            return true;
        }
        k += 1.0;
    }
    false
}

/// What this world does with a sphere.
pub trait SphereOps {
    /// Whether the sphere reaches an upright cylinder. Plainly (`edge` false): the distance across
    /// the ground to its axis and the height to the nearest point of its axis, against the two
    /// radii together. With `edge`: the nearest point of the cylinder's side within the sphere's
    /// own radius.
    fn intersects_cylinder(&self, c: &CylSphere, edge: bool) -> bool;

    fn intersects_box(&self, b: &BBox) -> bool;

    /// Where a ray first meets the sphere, as a fraction of its length: 0 from inside; none when
    /// it heads away, misses, or does not reach.
    fn trace_ray(&self, ray: &Ray) -> Option<f32>;

    /// The box around the sphere at `from`, in the landblock frame of `to`'s cell.
    fn bounding_box(&self, to: &Position, from: &Position) -> BBox;
}

impl SphereOps for Sphere {
    // D2-13144
    fn intersects_cylinder(&self, c: &CylSphere, edge: bool) -> bool {
        let (lo, hi) = (c.low_pt.z, c.low_pt.z + c.height);
        let cz = self.center.z;
        let z = if cz <= hi {
            if lo <= cz {
                cz
            } else {
                lo
            }
        } else {
            hi
        };
        let dx = c.low_pt.x - self.center.x;
        let (across, up, reach) = if edge {
            let dy = c.low_pt.y - self.center.y;
            let d2 = dx * dx + dy * dy;
            let r2 = c.radius * c.radius;
            let (x, y) = if d2 > r2 {
                let k = c.radius / d2.sqrt();
                (dx * k + c.low_pt.x, dy * k + c.low_pt.y)
            } else {
                (self.center.x, self.center.y)
            };
            let (ey, ez) = (y - self.center.y, z - self.center.z);
            let ex = x - self.center.x;
            (ey * ey + ez * ez, ex * ex, self.radius)
        } else {
            let dy = c.low_pt.y - self.center.y;
            let dz = self.center.z - z;
            (dy * dy + dx * dx, dz * dz, c.radius + self.radius)
        };
        up + across < reach * reach
    }

    // D2-13145
    fn intersects_box(&self, b: &BBox) -> bool {
        b.intersects_sphere(self)
    }

    // D2-13146
    fn trace_ray(&self, ray: &Ray) -> Option<f32> {
        // In double precision, rounded to a 32-bit float where retail keeps a value.
        let w = f64::from;
        let dx = w(ray.point.x) - w(self.center.x);
        let dy = w(ray.point.y) - w(self.center.y);
        let dz = w(ray.point.z) - w(self.center.z);
        let d2 = narrow((dz * dz + dx * dx) + dy * dy);
        let r2 = narrow(w(self.radius) * w(self.radius));
        if d2 < r2 {
            return Some(0.0);
        }
        let along = (dz * w(ray.dir.z) + dy * w(ray.dir.y)) + dx * w(ray.dir.x);
        if along >= 0.0 {
            return None;
        }
        let along = w(narrow(along));
        let disc = along * along - (w(d2) - w(r2));
        let reach = w(ray.length) + w(self.radius);
        // Negated so that a NaN goes on, as the client's compares do.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if disc <= 0.0 || !(reach * reach > w(d2)) {
            return None;
        }
        let t = (-along - w(narrow(disc)).sqrt()) / w(ray.length);
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(t < 1.0) {
            return None;
        }
        Some(if t < 0.0 { 0.0 } else { narrow(t) })
    }

    // D2-13371
    fn bounding_box(&self, to: &Position, from: &Position) -> BBox {
        let p = point_to_global_from(to, from, self.center);
        let r = self.radius;
        let mut b = BBox::new(
            Vec3::new(p.x + r, p.y + r, p.z + r),
            Vec3::new(p.x + r, p.y + r, p.z + r),
        );
        b.adjust_bbox(Vec3::new(p.x - r, p.y - r, p.z - r));
        b
    }
}

/// What this world does with an upright cylinder.
pub trait CylinderOps {
    /// The box around the cylinder at `from`, in the landblock frame of `to`'s cell: around its
    /// ends carried across when its axis stays upright there, else the box around it in its own
    /// frame carried across by its corners.
    fn bounding_box(&self, to: &Position, from: &Position) -> BBox;

    fn intersects_sphere(&self, s: &Sphere) -> bool;

    /// Whether two upright cylinders' heights overlap and their axes are within their radii.
    fn intersects_cylinder(&self, other: &CylSphere) -> bool;

    fn intersects_box(&self, b: &BBox) -> bool;

    /// Whether a point is within the radius of the axis (its height barely matters: see the body).
    fn within(&self, p: Vec3) -> bool;

    /// Where a ray first meets the cylinder, as a fraction of its length.
    fn trace_ray(&self, ray: &Ray) -> Option<f32>;

    /// Scales the cylinder: its low point along each axis, its height by the up factor, its
    /// radius by the mean of the two across the ground.
    fn scale(&mut self, s: Vec3);
}

impl CylinderOps for CylSphere {
    // D2-13154
    fn scale(&mut self, s: Vec3) {
        let w = f64::from;
        #[allow(clippy::cast_possible_truncation)]
        let n = |v: f64| v as f32;
        let p = self.low_pt;
        self.low_pt = Vec3::new(p.x * s.x, s.y * p.y, s.z * p.z);
        self.radius = n((w(s.y) + w(s.x)) * w(self.radius) * 0.5);
        self.height *= s.z;
    }

    // D2-13148
    fn bounding_box(&self, to: &Position, from: &Position) -> BBox {
        let r = self.radius;
        let m = rotation_axes(from.frame.rotation);
        let tilt = ((m[3] + m[0]) * 0.0 + self.height * m[6]).abs();
        if tilt < EPS {
            let low = point_to_global_from(to, from, self.low_pt);
            let top = point_to_global_from(
                to,
                from,
                Vec3::new(self.low_pt.x, self.low_pt.y, self.height + self.low_pt.z),
            );
            let mut b = BBox::new(
                Vec3::new(low.x + r, low.y + r, low.z),
                Vec3::new(low.x + r, low.y + r, low.z),
            );
            b.adjust_bbox(Vec3::new(top.x - r, top.y - r, top.z));
            return b;
        }
        let p = self.low_pt;
        let corner = Vec3::new(r + p.x, r + p.y, p.z);
        let mut local = BBox::new(corner, corner);
        local.adjust_bbox(Vec3::new(p.x - r, p.y - r, self.height + p.z));
        local.local_to_global(from, to)
    }

    // D2-13149
    fn intersects_sphere(&self, s: &Sphere) -> bool {
        s.intersects_cylinder(self, false)
    }

    // D2-13150
    fn intersects_cylinder(&self, other: &CylSphere) -> bool {
        let a = self.height * 0.5;
        let b = other.height * 0.5;
        if ((a + self.low_pt.z) - (b + other.low_pt.z)).abs() > a.abs() + b.abs() {
            return false;
        }
        let dx = self.low_pt.x - other.low_pt.x;
        let dy = self.low_pt.y - other.low_pt.y;
        let reach = other.radius + self.radius;
        dy * dy + dx * dx <= reach * reach
    }

    // D2-13151
    fn intersects_box(&self, b: &BBox) -> bool {
        b.intersects_cylinder(self)
    }

    /// Retail's height test is "above the base or below the top", which every point passes; only
    /// the distance from the axis decides.
    // D2-13152
    fn within(&self, p: Vec3) -> bool {
        if !(self.low_pt.z < p.z || p.z < self.low_pt.z + self.height) {
            return false;
        }
        let dx = self.low_pt.x - p.x;
        let dy = self.low_pt.y - p.y;
        dy * dy + dx * dx < self.radius * self.radius
    }

    // D2-13155
    fn trace_ray(&self, ray: &Ray) -> Option<f32> {
        // In double precision, rounded to a 32-bit float where retail keeps a value.
        let w = f64::from;
        let (px, py, pz) = (w(ray.point.x), w(ray.point.y), w(ray.point.z));
        let (rx, ry, rz) = (w(ray.dir.x), w(ray.dir.y), w(ray.dir.z));
        let len = w(ray.length);
        let (lx, ly, base) = (w(self.low_pt.x), w(self.low_pt.y), w(self.low_pt.z));
        let r = w(self.radius);
        let eps = w(EPS);
        let dx = w(narrow(px - lx));
        let dy = w(narrow(py - ly));
        let reach = len + r;
        // A ray of no length starting inside.
        if len < eps && r * r >= dy * dy + dx * dx && pz >= base && w(self.height) + base >= pz {
            return Some(0.0);
        }
        let d2 = w(narrow(dy * dy + dx * dx));
        if d2 > reach * reach {
            return None;
        }
        let r2 = w(narrow(r * r));
        // Negated so that a NaN takes the outside arm, as the client's compare does.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(d2 < r2) {
            // Outside the side: where the ray meets the side, or the top or base it crosses.
            let ground = w(narrow(ry * ry + rx * rx));
            let along = w(narrow(ry * dy + rx * dx));
            if eps * eps > ground || along >= 0.0 {
                return None;
            }
            let disc = along * along - (d2 - r * r) * ground;
            if disc <= eps {
                return None;
            }
            let root = w(narrow(disc.sqrt()));
            let inv = w(narrow(1.0 / ground));
            let t_in = (-along - root) * inv;
            if t_in > len {
                return None;
            }
            let t_in = w(narrow(t_in));
            let z_in = t_in * rz + pz;
            let z_out = w(narrow((root - along) * inv * rz + pz));
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            let flat = !(rz.abs() > eps);
            if z_in < base && !flat {
                #[allow(clippy::neg_cmp_op_on_partial_ord)]
                if !(z_out >= base) {
                    return None;
                }
                let t = (base - pz) / rz;
                return (t < len).then(|| narrow(t / len));
            }
            let top = w(narrow(w(self.height) + base));
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(z_in > top) || flat {
                return Some(narrow(t_in / len));
            }
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(z_out <= top) {
                return None;
            }
            let t = (top - pz) / rz;
            return (t < len).then(|| narrow(t / len));
        }
        // Inside the side: inside, or where the ray crosses the top or the base.
        let cap = if base <= pz || pz.is_nan() || base.is_nan() {
            let top = w(self.height) + base;
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(top < pz) {
                return Some(0.0);
            }
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(-eps >= rz) {
                return None;
            }
            w(narrow(top))
        } else {
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(rz >= eps) {
                return None;
            }
            base
        };
        let t = (cap - pz) / rz;
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(t < len) {
            return None;
        }
        let t = w(narrow(t));
        let ex = w(narrow(rx * t + px)) - lx;
        let ey = (ry * t + py) - ly;
        (ey * ey + ex * ex < r2).then(|| narrow(t / len))
    }
}

/// A sphere cut to a cone: a center and radius, the cone's axis and half-angle (radians), and the
/// half-angle's cosine.
// D2-13373
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cone {
    pub center: Vec3,
    pub radius: f32,
    pub dir: Vec3,
    pub angle: f32,
    pub cos_angle: f32,
}

impl Default for Cone {
    // D2-13372
    fn default() -> Self {
        Self {
            center: Vec3::ZERO,
            radius: 0.0,
            dir: Vec3::ZERO,
            angle: 0.0,
            cos_angle: 1.0,
        }
    }
}

impl Cone {
    // D2-13374
    #[must_use]
    pub fn new(center: Vec3, radius: f32, dir: Vec3, angle: f32) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let cos_angle = math::cos(f64::from(angle)) as f32;
        Self {
            center,
            radius,
            dir,
            angle,
            cos_angle,
        }
    }

    fn sphere(&self) -> Sphere {
        Sphere::new(self.center, self.radius)
    }

    /// Whether a direction lies inside the cone's angle. A direction longer than a unit (by more
    /// than 0.0002 squared) is brought to unit length first; a shorter one is taken as it is.
    // D2-13375
    #[must_use]
    pub fn angle_check(&self, v: Vec3) -> bool {
        let v = if (v.z * v.z + v.x * v.x + v.y * v.y) - 1.0 > EPS {
            normalized(v)
        } else {
            v
        };
        v.z * self.dir.z + v.y * self.dir.y + v.x * self.dir.x > self.cos_angle
    }

    /// Whether the cone reaches a sphere: within the radii, and either the direction to it inside
    /// the angle or the sphere's own angular size reaching over the cone's edge.
    // D2-13376
    #[must_use]
    pub fn intersects_sphere(&self, s: &Sphere) -> bool {
        let dx = self.center.x - s.center.x;
        let dy = self.center.y - s.center.y;
        let dz = self.center.z - s.center.z;
        let reach = self.radius + s.radius;
        // Negated so that a NaN distance fails, as the client's compare does.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !((dz * dz + dy * dy + dx * dx) - reach * reach < EPS) {
            return false;
        }
        let mut v = Vec3::new(
            s.center.x - self.center.x,
            s.center.y - self.center.y,
            s.center.z - self.center.z,
        );
        let len = (v.z * v.z + v.y * v.y + v.x * v.x).sqrt();
        if len > EPS {
            let k = 1.0 / len;
            v = Vec3::new(v.x * k, v.y * k, v.z * k);
        }
        if self.angle_check(v) {
            return true;
        }
        if s.radius < EPS {
            return false;
        }
        if self.radius < EPS {
            return true;
        }
        let dot = f64::from(v.z) * f64::from(self.dir.z)
            + f64::from(v.y) * f64::from(self.dir.y)
            + f64::from(v.x) * f64::from(self.dir.x);
        let off_axis = math::acos(dot);
        let (cr, sr, l) = (f64::from(self.radius), f64::from(s.radius), f64::from(len));
        let half = l * cr;
        let spread = math::acos(((cr * cr + l * l) - sr * sr) / (half + half));
        spread + f64::from(self.angle) > off_axis
    }

    /// Whether the cone reaches an upright cylinder: its bounding sphere does, and either its axis
    /// (as a ray of its radius) meets the cylinder or a column of spheres along the cylinder's
    /// axis, between the heights of the cone's center and tip, meets the cone.
    // D2-13377
    #[must_use]
    pub fn intersects_cylinder(&self, c: &CylSphere) -> bool {
        if !self.sphere().intersects_cylinder(c, false) {
            return false;
        }
        let axis = Ray {
            point: self.center,
            dir: self.dir,
            length: self.radius,
        };
        if c.trace_ray(&axis).is_some() {
            return true;
        }
        let (lo, hi) = (c.low_pt.z, c.height + c.low_pt.z);
        let z0 = clamp_height(self.center.z, lo, hi);
        let z1 = clamp_height(self.dir.z * self.radius + self.center.z, lo, hi);
        sweep_column(c, z0, z1, |s| self.intersects_sphere(s))
    }

    /// Whether the cone reaches a box: its bounding sphere does, and its tip or center is in the
    /// box, or the box's center lies inside its angle with the point the cone's radius along
    /// that direction in the box.
    // D2-13378
    #[must_use]
    pub fn intersects_box(&self, b: &BBox) -> bool {
        if !self.sphere().intersects_box(b) {
            return false;
        }
        let r = self.radius;
        let tip = Vec3::new(
            r * self.dir.x + self.center.x,
            r * self.dir.y + self.center.y,
            r * self.dir.z + self.center.z,
        );
        if b.within(tip) || b.within(self.center) {
            return true;
        }
        let c = b.center();
        let v = Vec3::new(
            c.x - self.center.x,
            c.y - self.center.y,
            c.z - self.center.z,
        );
        if !self.angle_check(v) {
            return false;
        }
        let v = normalized(v);
        b.within(Vec3::new(
            v.x * r + self.center.x,
            v.y * r + self.center.y,
            v.z * r + self.center.z,
        ))
    }
}

/// A shell between two radii, cut to a range of headings (theta, degrees) and of elevations
/// (phi), with the sines of the elevation bounds kept as the heights of the cut. A heading range
/// whose least bound is the greater wraps through north.
// D2-13383
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneCone {
    pub center: Vec3,
    pub min_radius: f32,
    pub max_radius: f32,
    pub min_theta: f32,
    pub max_theta: f32,
    pub min_phi: f32,
    pub max_phi: f32,
    pub min_z: f32,
    pub max_z: f32,
}

impl Default for PlaneCone {
    // D2-13382
    fn default() -> Self {
        Self {
            center: Vec3::ZERO,
            min_radius: 0.0,
            max_radius: 0.0,
            min_theta: 0.0,
            max_theta: 0.0,
            min_phi: 0.0,
            max_phi: 0.0,
            min_z: 0.0,
            max_z: 0.0,
        }
    }
}

impl PlaneCone {
    /// The heights are the sines of the elevation bounds as given (the attack cone gives them in
    /// degrees, and retail takes their sines as radians).
    // D2-13384
    #[must_use]
    pub fn new(
        center: Vec3,
        min_radius: f32,
        max_radius: f32,
        min_theta: f32,
        max_theta: f32,
        min_phi: f32,
        max_phi: f32,
    ) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let sin = |x: f32| math::sin(f64::from(x)) as f32;
        Self {
            center,
            min_radius,
            max_radius,
            min_theta,
            max_theta,
            min_phi,
            max_phi,
            min_z: sin(min_phi),
            max_z: sin(max_phi),
        }
    }

    fn bounds(&self) -> Sphere {
        Sphere::new(self.center, self.max_radius)
    }

    /// Whether a unit direction lies in the cut: its height between the cut's, and its heading in
    /// the heading range (any heading when the bounds are equal).
    // D2-13385
    #[must_use]
    pub fn check_angle(&self, v: Vec3) -> bool {
        let deg = heading_of(v.x, v.y).degrees();
        if v.z < self.min_z || self.max_z < v.z {
            return false;
        }
        let (lo, hi) = (self.min_theta, self.max_theta);
        if lo != hi {
            if lo > hi {
                if deg >= lo {
                    return true;
                }
            } else if deg < lo {
                return false;
            }
            if hi < deg {
                return false;
            }
        }
        true
    }

    /// Whether the shell reaches a sphere: within the outer radius, reaching past the inner, and
    /// either the direction to it in the cut or the sphere met by the ray along the nearest
    /// direction the cut allows (the heading and height brought to the cut's nearest bounds) from
    /// the inner radius to the outer.
    // D2-13386
    #[must_use]
    pub fn intersects_sphere(&self, s: &Sphere) -> bool {
        let dx = self.center.x - s.center.x;
        let dy = self.center.y - s.center.y;
        let dz = self.center.z - s.center.z;
        let reach = self.max_radius + s.radius;
        // Negated so that a NaN distance fails, as the client's compare does.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !((dz * dz + dy * dy + dx * dx) - reach * reach < EPS) {
            return false;
        }
        let mut v = Vec3::new(
            s.center.x - self.center.x,
            s.center.y - self.center.y,
            s.center.z - self.center.z,
        );
        let len = (v.y * v.y + v.x * v.x + v.z * v.z).sqrt();
        if len > EPS {
            let k = 1.0 / len;
            v = Vec3::new(v.x * k, v.y * k, k * v.z);
        }
        if len + s.radius < self.min_radius {
            return false;
        }
        if self.check_angle(v) {
            return true;
        }
        let mut deg = heading_of(v.x, v.y).degrees();
        let z = if v.z < self.min_z {
            self.min_z
        } else if self.max_z < v.z {
            self.max_z
        } else {
            v.z
        };
        let (lo, hi) = (self.min_theta, self.max_theta);
        if lo != hi {
            if lo < hi {
                if deg < lo || hi < deg {
                    let mut a = lo - deg;
                    if a < 0.0 {
                        a += 360.0;
                    }
                    let mut b = deg - hi;
                    if b < 0.0 {
                        b += 360.0;
                    }
                    deg = if a < b { lo } else { hi };
                }
            } else if deg < lo && hi < deg {
                deg = if deg - hi <= lo - deg { hi } else { lo };
            }
        }
        let h = Heading::new(deg).radians();
        #[allow(clippy::cast_possible_truncation)]
        let (sx, cy) = (
            math::sin(f64::from(h)) as f32,
            math::cos(f64::from(h)) as f32,
        );
        let flat = (1.0 - z * z).sqrt();
        let dir = Vec3::new(sx * flat, cy * flat, z);
        let r0 = self.min_radius;
        let ray = Ray {
            point: Vec3::new(
                dir.x * r0 + self.center.x,
                dir.y * r0 + self.center.y,
                r0 * z + self.center.z,
            ),
            dir,
            length: self.max_radius - self.min_radius,
        };
        s.trace_ray(&ray).is_some()
    }

    /// Whether the shell reaches an upright cylinder: its bounding sphere does, and a column of
    /// spheres along the cylinder's axis, between the heights of the cut's elevation bounds at
    /// the outer radius, meets the shell.
    // D2-13387
    #[must_use]
    pub fn intersects_cylinder(&self, c: &CylSphere) -> bool {
        if !self.bounds().intersects_cylinder(c, false) {
            return false;
        }
        let (lo, hi) = (c.low_pt.z, c.height + c.low_pt.z);
        let at = |phi: f32| {
            #[allow(clippy::cast_possible_truncation)]
            let z = (math::sin(f64::from(phi)) * f64::from(self.max_radius)
                + f64::from(self.center.z)) as f32;
            clamp_height(z, lo, hi)
        };
        let (z0, z1) = (at(self.min_phi), at(self.max_phi));
        sweep_column(c, z0, z1, |s| self.intersects_sphere(s))
    }

    /// Whether the shell's bounding sphere reaches a box.
    // D2-13381
    #[must_use]
    pub fn intersects_box(&self, b: &BBox) -> bool {
        self.bounds().intersects_box(b)
    }

    /// The box around the shell's bounding sphere at `from`, in the landblock frame of `to`'s
    /// cell.
    // D2-13380
    #[must_use]
    pub fn bounding_box(&self, to: &Position, from: &Position) -> BBox {
        self.bounds().bounding_box(to, from)
    }
}
