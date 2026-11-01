//! Axis-aligned boxes as this world works with them. The box itself is the shared one (its least
//! and greatest corners); what this world does with one is here: growing it to take in points,
//! building it around spheres and cylinders, carrying it into another frame, its faces as planes,
//! and testing points, spheres and cylinders against it.

pub use dereth_primitives::shape::BBox;
use dereth_primitives::shape::{CylSphere, Plane, Sphere};
use dereth_primitives::{Position, Vec3};

use super::point_to_global_from;

/// What this world does with a box.
pub trait BoxOps {
    /// The box ready to be grown: its least corner at the greatest float and its greatest at the
    /// least, so the first point taken in becomes both.
    fn for_adjustment() -> BBox;

    /// Grows to take in `v`, one axis at a time.
    fn adjust_bbox(&mut self, v: Vec3);

    /// Face `i` as a plane facing out of the box: 0 and 1 the least and greatest x, 2 and 3 y, 4
    /// the greatest z, 5 the least.
    fn face(&self, i: u32) -> Plane;

    fn center(&self) -> Vec3;

    /// Whether `v` is inside, its faces included.
    fn within(&self, v: Vec3) -> bool;

    /// Whether a sphere reaches the box: the point of the box nearest its center is within its
    /// radius.
    fn intersects_sphere(&self, s: &Sphere) -> bool;

    /// Whether an upright cylinder reaches the box: their heights overlap and the point of the
    /// box's ground square nearest the cylinder's axis is within its radius.
    fn intersects_cylinder(&self, c: &CylSphere) -> bool;

    /// Grows `out` to take in this box.
    fn build_bounding_box(&self, out: &mut BBox);

    /// This box, in `from`'s frame, as a box in the landblock frame of `to`'s cell: the box over
    /// its eight corners carried across.
    fn local_to_global(&self, from: &Position, to: &Position) -> BBox;

    /// The box around spheres.
    fn of_spheres(spheres: &[Sphere]) -> BBox;

    /// The box around upright cylinders: each from its base to its top.
    fn of_cylinders(cylinders: &[CylSphere]) -> BBox;

    /// Whether two boxes overlap on every axis by more than `margin`.
    fn does_overlap(&self, other: &BBox, margin: f32) -> bool;
}

fn clamp(c: f32, lo: f32, hi: f32) -> f32 {
    if c <= hi {
        if lo <= c {
            c
        } else {
            lo
        }
    } else {
        hi
    }
}

impl BoxOps for BBox {
    // D2-13225
    fn for_adjustment() -> BBox {
        BBox::new(
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            Vec3::new(-f32::MAX, -f32::MAX, -f32::MAX),
        )
    }

    // D2-02056
    fn adjust_bbox(&mut self, v: Vec3) {
        if v.x < self.min.x {
            self.min.x = v.x;
        }
        if v.y < self.min.y {
            self.min.y = v.y;
        }
        if v.z < self.min.z {
            self.min.z = v.z;
        }
        if self.max.x < v.x {
            self.max.x = v.x;
        }
        if self.max.y < v.y {
            self.max.y = v.y;
        }
        if self.max.z < v.z {
            self.max.z = v.z;
        }
    }

    // D2-13226
    fn face(&self, i: u32) -> Plane {
        match i {
            0 => Plane::new(Vec3::new(-1.0, 0.0, 0.0), self.min.x),
            1 => Plane::new(Vec3::new(1.0, 0.0, 0.0), -self.max.x),
            2 => Plane::new(Vec3::new(0.0, -1.0, 0.0), self.min.y),
            3 => Plane::new(Vec3::new(0.0, 1.0, 0.0), -self.max.y),
            4 => Plane::new(Vec3::new(0.0, 0.0, 1.0), -self.max.z),
            _ => Plane::new(Vec3::new(0.0, 0.0, -1.0), self.min.z),
        }
    }

    // D2-13230
    fn center(&self) -> Vec3 {
        Vec3::new(
            (self.max.x + self.min.x) * 0.5,
            (self.max.y + self.min.y) * 0.5,
            (self.max.z + self.min.z) * 0.5,
        )
    }

    // D2-13231
    fn within(&self, v: Vec3) -> bool {
        self.min.x <= v.x
            && v.x <= self.max.x
            && self.min.y <= v.y
            && v.y <= self.max.y
            && self.min.z <= v.z
            && v.z <= self.max.z
    }

    // D2-13232
    fn intersects_sphere(&self, s: &Sphere) -> bool {
        let c = s.center;
        let x = clamp(c.x, self.min.x, self.max.x) - c.x;
        let y = clamp(c.y, self.min.y, self.max.y) - c.y;
        let z = clamp(c.z, self.min.z, self.max.z) - c.z;
        y * y + z * z + x * x <= s.radius * s.radius
    }

    // D2-13238
    fn intersects_cylinder(&self, c: &CylSphere) -> bool {
        let half_box = (self.max.z - self.min.z) * 0.5;
        let half_cyl = c.height * 0.5;
        if ((half_box + self.min.z) - (half_cyl + c.low_pt.z)).abs()
            > half_box.abs() + half_cyl.abs()
        {
            return false;
        }
        let (x, y) = (c.low_pt.x, c.low_pt.y);
        let nx = if x <= self.max.x {
            if x < self.min.x {
                self.min.x
            } else {
                x
            }
        } else {
            self.max.x
        };
        let ny = if y <= self.max.y {
            if y < self.min.y {
                self.min.y
            } else {
                y
            }
        } else {
            self.max.y
        };
        (ny - y) * (ny - y) + (nx - x) * (nx - x) <= c.radius * c.radius
    }

    // D2-13229
    fn build_bounding_box(&self, out: &mut BBox) {
        out.adjust_bbox(self.min);
        out.adjust_bbox(self.max);
    }

    // D2-13227
    fn local_to_global(&self, from: &Position, to: &Position) -> BBox {
        let (lo, hi) = (self.min, self.max);
        let first = point_to_global_from(to, from, lo);
        let mut b = BBox::new(first, first);
        for v in [
            hi,
            Vec3::new(hi.x, hi.y, lo.z),
            Vec3::new(hi.x, lo.y, hi.z),
            Vec3::new(lo.x, hi.y, hi.z),
            Vec3::new(lo.x, hi.y, lo.z),
            Vec3::new(hi.x, lo.y, lo.z),
            Vec3::new(lo.x, lo.y, hi.z),
        ] {
            b.adjust_bbox(point_to_global_from(to, from, v));
        }
        b
    }

    // D2-13236
    fn of_spheres(spheres: &[Sphere]) -> BBox {
        let mut b = BBox::for_adjustment();
        for s in spheres {
            let (c, r) = (s.center, s.radius);
            b.adjust_bbox(Vec3::new(c.x - r, c.y - r, c.z - r));
            b.adjust_bbox(Vec3::new(r + c.x, r + c.y, r + c.z));
        }
        b
    }

    // D2-13235
    fn of_cylinders(cylinders: &[CylSphere]) -> BBox {
        let mut b = BBox::for_adjustment();
        for c in cylinders {
            let (p, r) = (c.low_pt, c.radius);
            b.adjust_bbox(Vec3::new(p.x - r, p.y - r, p.z));
            b.adjust_bbox(Vec3::new(p.x + r, p.y + r, c.height + p.z));
        }
        b
    }

    // D2-03479
    fn does_overlap(&self, other: &BBox, margin: f32) -> bool {
        let half = |lo: f32, hi: f32| (hi - lo) * 0.5;
        let axis = |a_lo: f32, a_hi: f32, b_lo: f32, b_hi: f32| {
            let (a, b) = (half(a_lo, a_hi), half(b_lo, b_hi));
            ((b + b_lo) - (a + a_lo)).abs() + margin <= a.abs() + b.abs()
        };
        axis(self.min.x, self.max.x, other.min.x, other.max.x)
            && axis(self.min.y, self.max.y, other.min.y, other.max.y)
            && axis(self.min.z, self.max.z, other.min.z, other.max.z)
    }
}
