//! A ray: a start, a unit direction and a length.

use auberean_assets::Context;
use auberean_dat::reader::Result;
use dereth_primitives::Vec3;

use super::EPS;
use crate::pack::{read_vec3, write_vec3, Pack, Reader, Writer};

/// A start, a unit direction and a length.
// D2-03509
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub point: Vec3,
    pub dir: Vec3,
    pub length: f32,
}

impl Default for Ray {
    /// At the origin, without a direction, and as long as a ray gets (100 km).
    // D2-03474
    fn default() -> Self {
        Self {
            point: Vec3::ZERO,
            dir: Vec3::ZERO,
            length: 100_000.0,
        }
    }
}

impl Ray {
    /// From a start and an offset to the end: the offset's direction and length, or straight
    /// along `+y` with no length when the offset is shorter than 0.0002 on every axis.
    // D2-03475
    pub fn init_from_offset(&mut self, start: Vec3, offset: Vec3) {
        self.point = start;
        if offset.x.abs() < EPS && offset.y.abs() < EPS && offset.z.abs() < EPS {
            self.length = 0.0;
            self.dir = Vec3::new(0.0, 1.0, 0.0);
            return;
        }
        // Worked as the client's floating-point unit works it: the length summed wide and
        // stored, its reciprocal kept wide.
        let w = f64::from;
        let o = offset;
        #[allow(clippy::cast_possible_truncation)]
        let n = |v: f64| v as f32;
        self.length = n(((w(o.z) * w(o.z) + w(o.y) * w(o.y)) + w(o.x) * w(o.x)).sqrt());
        let k = 1.0 / w(self.length);
        self.dir = Vec3::new(n(k * w(o.x)), n(k * w(o.y)), n(k * w(o.z)));
    }

    /// From one point to another.
    // D2-03476
    pub fn init_line_segment(&mut self, a: Vec3, b: Vec3) {
        self.init_from_offset(a, Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z));
    }

    /// The ray from `start` along `offset` ([`Ray::init_from_offset`]).
    #[must_use]
    pub fn from_offset(start: Vec3, offset: Vec3) -> Self {
        let mut r = Self::default();
        r.init_from_offset(start, offset);
        r
    }

    /// The ray from `a` to `b` ([`Ray::init_line_segment`]).
    #[must_use]
    pub fn segment(a: Vec3, b: Vec3) -> Self {
        let mut r = Self::default();
        r.init_line_segment(a, b);
        r
    }

    // D2-03477
    #[must_use]
    pub fn end_point(&self) -> Vec3 {
        let l = self.length;
        Vec3::new(
            l * self.dir.x + self.point.x,
            l * self.dir.y + self.point.y,
            l * self.dir.z + self.point.z,
        )
    }

    // D2-03793
    #[must_use]
    pub fn center_point(&self) -> Vec3 {
        let l = self.length;
        Vec3::new(
            l * self.dir.x * 0.5 + self.point.x,
            l * self.dir.y * 0.5 + self.point.y,
            l * self.dir.z * 0.5 + self.point.z,
        )
    }
}

impl Pack for Ray {
    /// The start, the direction, then the length.
    // D2-13393
    fn pack(&self, w: &mut Writer) {
        write_vec3(w, self.point);
        write_vec3(w, self.dir);
        w.f32(self.length);
    }

    fn unpack(r: &mut Reader<'_>, _ctx: &Context) -> Result<Self> {
        Ok(Self {
            point: read_vec3(r)?,
            dir: read_vec3(r)?,
            length: r.f32()?,
        })
    }
}
