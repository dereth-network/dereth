//! Surface samples and a bounded grid estimate of collision outside the drawn solids.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_physics::SetupGeometry;
use dereth_primitives::frame::V3;
use dereth_primitives::num::math;
use dereth_primitives::shape::{BBox, CylSphere, Sphere};
use dereth_primitives::Vec3;

use crate::pick::{triangle_hit, Ray};

/// One collision shape in model coordinates.
#[derive(Clone, Debug)]
pub enum Shape {
    Mesh(Vec<[Vec3; 3]>),
    Sphere(Sphere),
    Cylinder(CylSphere),
}

/// The drawn parts and the collision representation selected by the client.
#[derive(Clone, Debug)]
pub struct Model {
    pub drawn: Vec<Vec<[Vec3; 3]>>,
    pub collision: Vec<Shape>,
    pub physics: Arc<SetupGeometry>,
}

/// Grid-cell centers inside collision, and those outside every drawn solid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeFit {
    pub cells: usize,
    pub collision_cells: usize,
    pub excess_cells: usize,
    pub cell_volume: f32,
}

impl VolumeFit {
    #[must_use]
    pub fn excess_volume(self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let count = self.excess_cells as f32;
        count * self.cell_volume
    }
}

/// Seven samples per drawn triangle: its vertices, edge middles and center.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceFit {
    pub samples: usize,
    pub within_five_cm: usize,
    /// `None` when no collision surface exists.
    pub largest_gap: Option<f32>,
    /// `None` when a mesh is open or non-manifold and has no reliable enclosed volume.
    pub volume: Option<VolumeFit>,
}

fn length(p: Vec3) -> f32 {
    p.mag2().sqrt()
}

fn edge_distance(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let edge = b.sub(a);
    let t = if edge.mag2() > 0.0 {
        (p.sub(a).dot(edge) / edge.mag2()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    length(p.sub(a.add(edge.mul(t))))
}

fn triangle_distance(p: Vec3, t: &[Vec3; 3]) -> f32 {
    let [a, b, c] = *t;
    let ab = b.sub(a);
    let ac = c.sub(a);
    let n = ab.cross(ac);
    let n2 = n.mag2();
    if n2 > 1e-16 {
        let q = p.sub(n.mul(p.sub(a).dot(n) / n2));
        if [
            ab.cross(q.sub(a)),
            c.sub(b).cross(q.sub(b)),
            a.sub(c).cross(q.sub(c)),
        ]
        .iter()
        .all(|edge| edge.dot(n) >= -1e-7)
        {
            return (p.sub(a).dot(n) / n2.sqrt()).abs();
        }
    }
    edge_distance(p, a, b)
        .min(edge_distance(p, b, c))
        .min(edge_distance(p, c, a))
}

fn mesh_contains(mesh: &[[Vec3; 3]], p: Vec3) -> bool {
    let ray = Ray {
        origin: p,
        dir: Vec3::new(0.837, 0.231, 0.496).normalize(),
    };
    let mut hits: Vec<_> = mesh.iter().filter_map(|t| triangle_hit(t, &ray)).collect();
    hits.sort_by(f32::total_cmp);
    hits.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
    !hits.len().is_multiple_of(2)
}

fn closed(mesh: &[[Vec3; 3]]) -> bool {
    let key = |p: Vec3| [p.x, p.y, p.z].map(|x| if x == 0.0 { 0 } else { x.to_bits() });
    let mut edges = BTreeMap::new();
    for t in mesh {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (a, b) = (key(a), key(b));
            if a == b {
                return false;
            }
            let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
            let entry = edges.entry(edge).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += sign;
        }
    }
    !edges.is_empty() && edges.values().all(|e| *e == (2, 0))
}

fn cylinder_distance(c: &CylSphere, p: Vec3) -> f32 {
    let radial = math::hypotf(p.x - c.low_pt.x, p.y - c.low_pt.y) - c.radius;
    let vertical = (p.z - c.low_pt.z - c.height * 0.5).abs() - c.height * 0.5;
    math::hypotf(radial.max(0.0), vertical.max(0.0)) + radial.max(vertical).min(0.0)
}

impl Shape {
    fn distance(&self, p: Vec3) -> f32 {
        match self {
            Self::Mesh(mesh) => mesh
                .iter()
                .map(|t| triangle_distance(p, t))
                .fold(f32::INFINITY, f32::min),
            Self::Sphere(s) => (length(p.sub(s.center)) - s.radius).abs(),
            Self::Cylinder(c) => cylinder_distance(c, p).abs(),
        }
    }

    fn contains(&self, p: Vec3) -> bool {
        match self {
            Self::Mesh(mesh) => mesh_contains(mesh, p),
            Self::Sphere(s) => p.sub(s.center).mag2() <= s.radius * s.radius,
            Self::Cylinder(c) => cylinder_distance(c, p) <= 0.0,
        }
    }

    fn bounds(&self) -> BBox {
        match self {
            Self::Mesh(mesh) => BBox::of_points(mesh.iter().flatten().copied()),
            Self::Sphere(s) => {
                let r = Vec3::new(s.radius, s.radius, s.radius);
                BBox::new(s.center.sub(r), s.center.add(r))
            }
            Self::Cylinder(c) => BBox::new(
                c.low_pt.sub(Vec3::new(c.radius, c.radius, 0.0)),
                c.low_pt.add(Vec3::new(c.radius, c.radius, c.height)),
            ),
        }
    }
}

impl Model {
    /// The largest horizontal reach of drawing or collision, from the model origin.
    #[must_use]
    pub fn reach(&self) -> f32 {
        let drawn = self
            .drawn
            .iter()
            .flatten()
            .flatten()
            .map(|p| math::hypotf(p.x, p.y));
        let collision = self.collision.iter().map(|s| {
            let b = s.bounds();
            math::hypotf(
                b.min.x.abs().max(b.max.x.abs()),
                b.min.y.abs().max(b.max.y.abs()),
            )
        });
        drawn.chain(collision).fold(0.0, f32::max)
    }
}

/// Sample the drawn surface and estimate collision volume outside the drawn solids.
///
/// Volume is a grid estimate, never an exact mesh volume. Open or non-manifold meshes leave it
/// unknown. Closed parts are treated as a union, including parts that overlap.
///
/// # Errors
/// No drawn triangles, invalid geometry, or a grid outside 4 to 64 cells per axis.
pub fn measure(model: &Model, grid: usize) -> Result<SurfaceFit, String> {
    if !(4..=64).contains(&grid) {
        return Err("volume sampling needs 4 to 64 cells per axis".into());
    }
    if model.drawn.iter().all(Vec::is_empty) {
        return Err("the model has no drawn triangles".into());
    }
    let finite = |p: &Vec3| p.x.is_finite() && p.y.is_finite() && p.z.is_finite();
    if model.drawn.iter().flatten().flatten().any(|p| !finite(p))
        || model.collision.iter().any(|s| match s {
            Shape::Mesh(m) => m.is_empty() || m.iter().flatten().any(|p| !finite(p)),
            Shape::Sphere(s) => !finite(&s.center) || !s.radius.is_finite() || s.radius < 0.0,
            Shape::Cylinder(c) => {
                !finite(&c.low_pt)
                    || !c.radius.is_finite()
                    || c.radius < 0.0
                    || !c.height.is_finite()
                    || c.height < 0.0
            }
        })
    {
        return Err("fit geometry must be finite, with nonnegative radii and heights".into());
    }
    let mut report = SurfaceFit {
        samples: 0,
        within_five_cm: 0,
        largest_gap: None,
        volume: None,
    };
    for [a, b, c] in model.drawn.iter().flatten() {
        for p in [
            *a,
            *b,
            *c,
            a.add(*b).mul(0.5),
            b.add(*c).mul(0.5),
            c.add(*a).mul(0.5),
            a.add(*b).add(*c).mul(1.0 / 3.0),
        ] {
            report.samples += 1;
            let distance = model
                .collision
                .iter()
                .map(|s| s.distance(p))
                .fold(f32::INFINITY, f32::min);
            if !distance.is_finite() && !model.collision.is_empty() {
                return Err("surface distances exceed the sampling range".into());
            }
            report.within_five_cm += usize::from(distance <= 0.05);
            if distance.is_finite() {
                report.largest_gap = Some(report.largest_gap.unwrap_or(0.0).max(distance));
            }
        }
    }
    if model.drawn.iter().any(|m| !closed(m))
        || model
            .collision
            .iter()
            .any(|s| matches!(s, Shape::Mesh(m) if !closed(m)))
    {
        return Ok(report);
    }
    let bounds = BBox::of_points(model.collision.iter().flat_map(|s| {
        let b = s.bounds();
        [b.min, b.max]
    }));
    #[allow(clippy::cast_precision_loss)]
    let size = grid as f32;
    let step = bounds.max.sub(bounds.min).mul(1.0 / size);
    let mut volume = VolumeFit {
        cells: grid * grid * grid,
        collision_cells: 0,
        excess_cells: 0,
        cell_volume: step.x * step.y * step.z,
    };
    if !volume.cell_volume.is_finite() {
        return Err("collision bounds exceed the sampling range".into());
    }
    for x in 0..grid {
        for y in 0..grid {
            for z in 0..grid {
                #[allow(clippy::cast_precision_loss)]
                let offset = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                let p = bounds.min.add(step.mul_componentwise(offset));
                if model.collision.iter().any(|s| s.contains(p)) {
                    volume.collision_cells += 1;
                    volume.excess_cells +=
                        usize::from(!model.drawn.iter().any(|m| mesh_contains(m, p)));
                }
            }
        }
    }
    report.volume = Some(volume);
    Ok(report)
}
