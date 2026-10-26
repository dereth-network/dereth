//! Solid geometry in landblock space, indexed for clearance queries, and the floors it stands on.
//!
//! Three kinds of solid, the same three a body collides with: polygons (a cell's walls and
//! ceilings, and the polygons of an object's parts), spheres and vertical cylinders (an object
//! without part polygons). Walkable polygons are kept apart as floors: a body stands on them, so
//! they are never in its way.

use std::collections::HashMap;

use dereth_physics::source::SetupGeometry;
use dereth_primitives::frame::localtoglobal;
use dereth_primitives::num::floor_to_i32;
use dereth_primitives::{Frame, Position, Vec3};

use crate::vmath::{add, cross, dot, len, point_segment_dist_sq, scale, sub};

/// A planar convex polygon with its plane and bounds.
#[derive(Debug, Clone)]
pub(crate) struct Poly {
    pub verts: Vec<Vec3>,
    pub normal: Vec3,
    pub d: f32,
    pub min: Vec3,
    pub max: Vec3,
}

impl Poly {
    /// A polygon over `verts`, or `None` when they enclose no area.
    pub fn new(verts: Vec<Vec3>) -> Option<Self> {
        if verts.len() < 3 {
            return None;
        }
        let mut n = Vec3::ZERO;
        let v0 = verts[0];
        for k in 1..verts.len() - 1 {
            n = add(n, cross(sub(verts[k], v0), sub(verts[k + 1], v0)));
        }
        let l = len(n);
        if l < 1e-8 {
            return None;
        }
        let normal = scale(n, 1.0 / l);
        let mut mean = 0.0;
        let mut min = v0;
        let mut max = v0;
        for v in &verts {
            mean += dot(normal, *v);
            min = Vec3::new(min.x.min(v.x), min.y.min(v.y), min.z.min(v.z));
            max = Vec3::new(max.x.max(v.x), max.y.max(v.y), max.z.max(v.z));
        }
        #[allow(clippy::cast_precision_loss)]
        let d = -mean / verts.len() as f32;
        Some(Self {
            verts,
            normal,
            d,
            min,
            max,
        })
    }

    /// The squared distance from `p` to the nearest point of the polygon.
    pub fn dist_sq(&self, p: Vec3) -> f32 {
        let h = dot(self.normal, p) + self.d;
        let q = sub(p, scale(self.normal, h));
        let n = self.verts.len();
        let mut inside = true;
        for i in 0..n {
            let a = self.verts[i];
            let b = self.verts[(i + 1) % n];
            if dot(cross(sub(b, a), sub(q, a)), self.normal) < 0.0 {
                inside = false;
                break;
            }
        }
        if inside {
            return h * h;
        }
        let mut best = f32::MAX;
        for i in 0..n {
            best = best.min(point_segment_dist_sq(
                p,
                self.verts[i],
                self.verts[(i + 1) % n],
            ));
        }
        best
    }

    /// Whether `(x, y)` lies inside the polygon's projection onto the ground, edges included.
    pub fn contains_xy(&self, x: f32, y: f32) -> bool {
        let n = self.verts.len();
        let mut sign = 0.0_f32;
        for i in 0..n {
            let a = self.verts[i];
            let b = self.verts[(i + 1) % n];
            let c = (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x);
            if c.abs() <= 1e-5 {
                continue;
            }
            if sign == 0.0 {
                sign = c.signum();
            } else if c.signum() != sign {
                return false;
            }
        }
        true
    }

    /// The height of the polygon's plane above `(x, y)`.
    pub fn z_at(&self, x: f32, y: f32) -> f32 {
        -(self.d + self.normal.x * x + self.normal.y * y) / self.normal.z
    }
}

/// One solid shape.
#[derive(Debug, Clone)]
pub(crate) enum Shape {
    Poly(Poly),
    Sphere {
        center: Vec3,
        radius: f32,
    },
    /// A vertical cylinder from `low` up `height`.
    Cylinder {
        low: Vec3,
        radius: f32,
        height: f32,
    },
}

impl Shape {
    /// The squared distance from `p` to the shape, zero inside it.
    pub fn dist_sq(&self, p: Vec3) -> f32 {
        match self {
            Self::Poly(poly) => poly.dist_sq(p),
            Self::Sphere { center, radius } => {
                let d = (len(sub(p, *center)) - radius).max(0.0);
                d * d
            }
            Self::Cylinder {
                low,
                radius,
                height,
            } => {
                let dx = p.x - low.x;
                let dy = p.y - low.y;
                let dh = ((dx * dx + dy * dy).sqrt() - radius).max(0.0);
                let dv = (low.z - p.z).max(p.z - (low.z + height)).max(0.0);
                dh * dh + dv * dv
            }
        }
    }

    /// The shape's bounds.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        match self {
            Self::Poly(p) => (p.min, p.max),
            Self::Sphere { center, radius } => (
                sub(*center, Vec3::new(*radius, *radius, *radius)),
                add(*center, Vec3::new(*radius, *radius, *radius)),
            ),
            Self::Cylinder {
                low,
                radius,
                height,
            } => (
                sub(*low, Vec3::new(*radius, *radius, 0.0)),
                add(*low, Vec3::new(*radius, *radius, *height)),
            ),
        }
    }
}

/// What a solid belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Owner {
    /// A cell's own walls, floors and ceilings.
    Cell,
    /// A building's shell, which stops a body standing outdoors and none in the building's rooms.
    Shell,
    /// A static object baked into a cell, by its placement index.
    Static(u32),
}

/// The edge length of an index bucket, in metres.
const BUCKET: f32 = 2.0;

/// Solids bucketed on the ground plane. A shape is entered into every bucket its bounds, grown by
/// the query reach, overlap, so one bucket answers a query at any point inside it.
#[derive(Debug, Default)]
pub(crate) struct SolidIndex {
    pub shapes: Vec<(Shape, Owner)>,
    buckets: HashMap<(i32, i32), Vec<u32>>,
    reach: f32,
}

fn bucket_of(x: f32, y: f32) -> (i32, i32) {
    (floor_to_i32(x / BUCKET), floor_to_i32(y / BUCKET))
}

impl SolidIndex {
    pub fn new(reach: f32) -> Self {
        Self {
            shapes: Vec::new(),
            buckets: HashMap::new(),
            reach,
        }
    }

    pub fn push(&mut self, shape: Shape, owner: Owner) {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a shape count, far below u32::MAX.
        let id = self.shapes.len() as u32;
        let (min, max) = shape.bounds();
        let (x0, y0) = bucket_of(min.x - self.reach, min.y - self.reach);
        let (x1, y1) = bucket_of(max.x + self.reach, max.y + self.reach);
        for bx in x0..=x1 {
            for by in y0..=y1 {
                self.buckets.entry((bx, by)).or_default().push(id);
            }
        }
        self.shapes.push((shape, owner));
    }

    /// The shapes that can lie within the reach of `p`.
    pub fn near(&self, p: Vec3) -> &[u32] {
        self.buckets
            .get(&bucket_of(p.x, p.y))
            .map_or(&[], Vec::as_slice)
    }

    /// Whether every shape is at least `clear` from each of `points`, ignoring shapes whose
    /// owner `skip` rejects.
    pub fn clear_of(&self, points: &[Vec3], clear: f32, skip: impl Fn(Owner) -> bool) -> bool {
        let c2 = clear * clear;
        let Some(first) = points.first() else {
            return true;
        };
        let zlo = points.iter().map(|p| p.z).fold(f32::MAX, f32::min) - clear;
        let zhi = points.iter().map(|p| p.z).fold(f32::MIN, f32::max) + clear;
        for &i in self.near(*first) {
            let (shape, owner) = &self.shapes[i as usize];
            if skip(*owner) {
                continue;
            }
            let (min, max) = shape.bounds();
            if max.z < zlo || min.z > zhi {
                continue;
            }
            if points.iter().any(|p| shape.dist_sq(*p) < c2) {
                return false;
            }
        }
        true
    }
}

/// The world-space polygons of one physics tree placed at `frame` and scaled by `s`; walkable ones
/// are handed to `floor` instead of returned.
pub(crate) fn bsp_shapes(
    bsp: &dereth_physics::BspTree,
    frame: &Frame,
    s: f32,
    walkable_z: f32,
    mut floor: impl FnMut(Poly),
) -> Vec<Shape> {
    let mut out = Vec::new();
    for poly in &bsp.polygons {
        let verts: Vec<Vec3> = poly
            .vertices
            .iter()
            .map(|v| localtoglobal(frame, scale(*v, s)))
            .collect();
        if let Some(p) = Poly::new(verts) {
            if p.normal.z >= walkable_z {
                floor(p);
            } else {
                out.push(Shape::Poly(p));
            }
        }
    }
    out
}

/// The world-space shapes of one object, as its collision sees it: the parts' polygons when any
/// part carries a physics tree, otherwise its cylinders, otherwise its spheres. Walkable part
/// polygons are handed to `floor` instead of returned.
pub(crate) fn object_shapes(
    g: &SetupGeometry,
    frame: &Frame,
    obj_scale: f32,
    walkable_z: f32,
    mut floor: impl FnMut(Poly),
) -> Vec<Shape> {
    let mut out = Vec::new();
    if g.caches_physics_bsp() {
        let pos = Position::new(dereth_primitives::CellId(0), *frame);
        for i in 0..g.parts.len() {
            let Some(part) = g.placed_part(i, &pos, obj_scale) else {
                continue;
            };
            let Some(bsp) = part.physics_bsp.as_ref() else {
                continue;
            };
            out.extend(bsp_shapes(
                bsp,
                &part.pos.frame,
                part.gfxobj_scale,
                walkable_z,
                &mut floor,
            ));
        }
        return out;
    }
    if !g.cyl_spheres.is_empty() {
        for c in &g.cyl_spheres {
            out.push(Shape::Cylinder {
                low: localtoglobal(frame, scale(c.low_pt, obj_scale)),
                radius: c.radius * obj_scale,
                height: c.height * obj_scale,
            });
        }
        return out;
    }
    for s in &g.spheres {
        out.push(Shape::Sphere {
            center: localtoglobal(frame, scale(s.center, obj_scale)),
            radius: s.radius * obj_scale,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks the planner's distance geometry).
    use super::*;

    fn square(z: f32) -> Poly {
        Poly::new(vec![
            Vec3::new(0.0, 0.0, z),
            Vec3::new(1.0, 0.0, z),
            Vec3::new(1.0, 1.0, z),
            Vec3::new(0.0, 1.0, z),
        ])
        .expect("a square has area")
    }

    #[test]
    fn a_point_above_a_square_is_its_height_away_and_one_beside_it_is_measured_to_the_edge() {
        let p = square(0.0);
        assert!((p.dist_sq(Vec3::new(0.5, 0.5, 2.0)) - 4.0).abs() < 1e-5);
        assert!((p.dist_sq(Vec3::new(2.0, 0.5, 0.0)) - 1.0).abs() < 1e-5);
        assert!(p.contains_xy(0.5, 0.5));
        assert!(p.contains_xy(1.0, 0.5), "edges are inside");
        assert!(!p.contains_xy(1.1, 0.5));
        assert!((p.z_at(0.3, 0.3)).abs() < 1e-6);
    }

    #[test]
    fn a_cylinder_is_measured_sideways_and_above() {
        let c = Shape::Cylinder {
            low: Vec3::new(0.0, 0.0, 0.0),
            radius: 1.0,
            height: 2.0,
        };
        assert!((c.dist_sq(Vec3::new(3.0, 0.0, 1.0)) - 4.0).abs() < 1e-5);
        assert!((c.dist_sq(Vec3::new(0.0, 0.0, 5.0)) - 9.0).abs() < 1e-5);
        assert_eq!(c.dist_sq(Vec3::new(0.5, 0.0, 1.0)), 0.0);
    }
}
