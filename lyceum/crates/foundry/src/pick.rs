//! Picking: what a ray from the camera meets first among the world's statics, buildings, spawns,
//! indoor cells (their walls and floors, and the statics standing in them) and ground, as the
//! session compiles them.
//!
//! Everything is in **world meters**: `x = landblock x * 192 + local x`, the same for `y`, `z` as
//! the landblock's. A model is its triangles (a GfxObj's polygons; a Setup's parts at their resting
//! placement and default scale), read once and kept ([`Models`]), and a ray meets it in its own
//! frame: its bounding sphere first, then its triangles, the nearest of which is the hit. The
//! ground is each landblock's terrain triangles, cut as the client cuts each cell, and its hit
//! names the nearest grid vertex. A spawn is its setup's selection sphere, or a meter's sphere when
//! the session does not know its setup.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_assets::{GfxObj, Setup};
use dereth_primitives::frame::{globaltolocal, globaltolocalvec, l2g, localtoglobal, V3};
use dereth_primitives::{DataId, Frame, Vec3};

use crate::brush::GRID;
use crate::dats::DatSet;
use crate::edit::Session;

/// The side of a landblock, in meters.
pub const BLOCK: f32 = 192.0;

/// How much nearer than the ground something must be met to be met first, in meters: a floor or
/// a rug laid on the ground at its height is met, not the ground under it.
const GROUND_YIELD: f32 = 0.02;

/// The placement a static's setup rests in, and the one it falls back to.
const RESTING: u32 = 0x65;

/// A ray in world meters; `dir` is a unit vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

/// What a pick met.
#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    /// A static of a landblock's compiled statics record, by its place in it.
    Static {
        landblock: u16,
        index: usize,
        model: DataId,
    },
    /// A building of a landblock's statics record, by its place in it.
    Building {
        landblock: u16,
        index: usize,
        model: DataId,
    },
    /// A static standing in an indoor cell, by its place among the cell's compiled statics.
    CellStatic {
        landblock: u16,
        cell: u16,
        index: usize,
        model: DataId,
    },
    /// A wall or floor of an indoor cell, at `point` (landblock meters).
    Cell {
        landblock: u16,
        cell: u16,
        point: Vec3,
    },
    /// A spawn, by its place in [`Session::spawns`].
    Spawn { index: usize },
    /// The ground, at `point` (world meters), nearest the grid vertex `vertex`.
    Ground { point: Vec3, vertex: (u32, u32) },
}

/// A hit, and how far along the ray.
#[derive(Clone, Debug, PartialEq)]
pub struct Picked {
    pub hit: Hit,
    pub distance: f32,
}

/// Which kinds a pick may meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mask {
    pub statics: bool,
    pub buildings: bool,
    pub spawns: bool,
    pub ground: bool,
    /// The statics standing in indoor cells.
    pub cell_statics: bool,
    /// The indoor cells' walls and floors.
    pub cells: bool,
}

impl Mask {
    pub const ALL: Self = Self {
        statics: true,
        buildings: true,
        spawns: true,
        ground: true,
        cell_statics: true,
        cells: true,
    };

    /// The ground alone.
    pub const GROUND: Self = Self {
        statics: false,
        buildings: false,
        spawns: false,
        ground: true,
        cell_statics: false,
        cells: false,
    };
}

/// A model as a pick sees it: its triangles in its own frame, and a sphere around them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub triangles: Vec<[Vec3; 3]>,
    pub center: Vec3,
    pub radius: f32,
}

impl Model {
    fn from_triangles(triangles: Vec<[Vec3; 3]>) -> Self {
        let mut lo = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut hi = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        for p in triangles.iter().flatten() {
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        if triangles.is_empty() {
            return Self::default();
        }
        let center = lo.add(hi).mul(0.5);
        let radius = triangles
            .iter()
            .flatten()
            .map(|p| p.sub(center).magnitude())
            .fold(0.0, f32::max);
        Self {
            triangles,
            center,
            radius,
        }
    }

    /// The nearest of the model's triangles `ray` (in the model's frame) meets, as a distance.
    #[must_use]
    pub fn hit(&self, ray: &Ray) -> Option<f32> {
        sphere_hit(self.center, self.radius, ray)?;
        self.triangles
            .iter()
            .filter_map(|t| triangle_hit(t, ray))
            .min_by(f32::total_cmp)
    }
}

/// Where along `ray` it first meets the sphere, or `0` from inside it.
#[must_use]
pub fn sphere_hit(center: Vec3, radius: f32, ray: &Ray) -> Option<f32> {
    let oc = ray.origin.sub(center);
    let b = oc.dot(ray.dir);
    let c = oc.dot(oc) - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let disc = b * b - c;
    if disc < 0.0 || b > 0.0 {
        return None;
    }
    Some(-b - disc.sqrt())
}

/// Where along `ray` it meets the triangle, from either side (Moller and Trumbore's test).
#[must_use]
pub fn triangle_hit(t: &[Vec3; 3], ray: &Ray) -> Option<f32> {
    let e1 = t[1].sub(t[0]);
    let e2 = t[2].sub(t[0]);
    let p = ray.dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = ray.origin.sub(t[0]);
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = ray.dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let d = e2.dot(q) * inv;
    (d > 1e-4).then_some(d)
}

/// The models read so far, by id.
#[derive(Debug, Default)]
pub struct Models {
    read: BTreeMap<u32, Option<Arc<Model>>>,
}

fn gfxobj_triangles(set: &DatSet, id: DataId) -> Option<Vec<[Vec3; 3]>> {
    let g = set.decode::<GfxObj>(id, &set.read_portal(id)?).ok()?;
    let at = |i: u16| {
        g.vertex_array
            .vertices
            .iter()
            .find(|v| v.id == i16::try_from(i).unwrap_or(-1))
            .or_else(|| g.vertex_array.vertices.get(usize::from(i)))
            .map(|v| v.position)
    };
    let mut out = Vec::new();
    let polygons = if g.polygons.is_empty() {
        &g.physics_polygons
    } else {
        &g.polygons
    };
    for p in polygons {
        let pts: Vec<Vec3> = p.vertex_ids.iter().filter_map(|i| at(*i)).collect();
        for k in 1..pts.len().saturating_sub(1) {
            out.push([pts[0], pts[k], pts[k + 1]]);
        }
    }
    Some(out)
}

impl Models {
    /// Model `id` (a GfxObj or a Setup), read from `set` the first time.
    pub fn get(&mut self, set: &DatSet, id: DataId) -> Option<Arc<Model>> {
        self.read
            .entry(id.raw())
            .or_insert_with(|| {
                let triangles = match id.raw() >> 24 {
                    0x01 => gfxobj_triangles(set, id)?,
                    0x02 => {
                        let s = set.decode::<Setup>(id, &set.read_portal(id)?).ok()?;
                        let placement = s
                            .placement_frames
                            .get(&RESTING)
                            .or_else(|| s.placement_frames.get(&0));
                        let mut out = Vec::new();
                        for (i, part) in s.parts.iter().enumerate() {
                            let frame = placement
                                .and_then(|p| p.frames.get(i).copied())
                                .unwrap_or_default();
                            let scale = s
                                .default_scale
                                .as_ref()
                                .and_then(|d| d.get(i).copied())
                                .unwrap_or(Vec3::new(1.0, 1.0, 1.0));
                            for t in gfxobj_triangles(set, *part).unwrap_or_default() {
                                out.push(t.map(|p| {
                                    localtoglobal(
                                        &frame,
                                        Vec3::new(p.x * scale.x, p.y * scale.y, p.z * scale.z),
                                    )
                                }));
                            }
                        }
                        out
                    }
                    _ => return None,
                };
                Some(Arc::new(Model::from_triangles(triangles)))
            })
            .clone()
    }
}

/// A landblock's south-west corner, in world meters.
#[must_use]
pub fn block_origin(lb: u16) -> Vec3 {
    Vec3::new(
        f32::from(lb >> 8) * BLOCK,
        f32::from(lb & 0xFF) * BLOCK,
        0.0,
    )
}

/// `ray` in the frame `f` placed in landblock `lb`.
fn into_frame(ray: &Ray, lb: u16, f: &Frame) -> Ray {
    let local = ray.origin.sub(block_origin(lb));
    Ray {
        origin: globaltolocal(f, local),
        dir: globaltolocalvec(l2g(f.rotation), ray.dir),
    }
}

/// The nearest thing `ray` meets of the kinds `mask` names, among the statics, buildings,
/// spawns and ground of `landblocks`.
pub fn pick(
    session: &Session,
    models: &mut Models,
    ray: &Ray,
    landblocks: &[u16],
    mask: Mask,
) -> Option<Picked> {
    let mut best: Option<Picked> = None;
    let mut offer = |hit: Hit, distance: f32| {
        if best.as_ref().is_none_or(|b| distance < b.distance) {
            best = Some(Picked { hit, distance });
        }
    };
    let set = session.base();
    for &lb in landblocks {
        if mask.cell_statics || mask.cells {
            let cells = session.cells(lb);
            if mask.cell_statics {
                for (n, cell) in &cells {
                    for (index, o) in cell.static_objects.iter().enumerate() {
                        let Some(m) = models.get(set, o.id) else {
                            continue;
                        };
                        if let Some(d) = m.hit(&into_frame(ray, lb, &o.frame)) {
                            offer(
                                Hit::CellStatic {
                                    landblock: lb,
                                    cell: *n,
                                    index,
                                    model: o.id,
                                },
                                d,
                            );
                        }
                    }
                }
            }
            if mask.cells && !cells.is_empty() {
                let from = ray.origin.sub(block_origin(lb));
                let env = |id: DataId| session.environment(id);
                // As the world draws them: a wall or floor met from its own room's inside.
                if let Some((cell, point, _)) =
                    crate::walk::cell_hit_facing(&cells, from, ray.dir, &env)
                {
                    offer(
                        Hit::Cell {
                            landblock: lb,
                            cell,
                            point,
                        },
                        point.sub(from).magnitude(),
                    );
                }
            }
        }
        let Some(info) = (mask.statics || mask.buildings)
            .then(|| session.statics(lb))
            .flatten()
        else {
            continue;
        };
        if mask.statics {
            for (index, o) in info.objects.iter().enumerate() {
                let Some(m) = models.get(set, o.id) else {
                    continue;
                };
                if let Some(d) = m.hit(&into_frame(ray, lb, &o.frame)) {
                    offer(
                        Hit::Static {
                            landblock: lb,
                            index,
                            model: o.id,
                        },
                        d,
                    );
                }
            }
        }
        if mask.buildings {
            for (index, b) in info.buildings.iter().enumerate() {
                let Some(m) = models.get(set, b.id) else {
                    continue;
                };
                if let Some(d) = m.hit(&into_frame(ray, lb, &b.frame)) {
                    offer(
                        Hit::Building {
                            landblock: lb,
                            index,
                            model: b.id,
                        },
                        d,
                    );
                }
            }
        }
    }
    if mask.spawns {
        for (index, s) in session.spawns().iter().enumerate() {
            if !landblocks.contains(&s.landblock()) {
                continue;
            }
            let model = s.setup.and_then(|id| models.get(set, id));
            let local = into_frame(ray, s.landblock(), &s.frame);
            let d = match model.filter(|m| !m.triangles.is_empty()) {
                Some(m) => m
                    .hit(&local)
                    .or_else(|| sphere_hit(m.center, m.radius, &local)),
                None => sphere_hit(Vec3::new(0.0, 0.0, 1.0), 1.0, &local),
            };
            if let Some(d) = d {
                offer(Hit::Spawn { index }, d);
            }
        }
    }
    if mask.ground {
        if let Some((point, d)) = ground_hit(session, ray, landblocks) {
            // A building's floor is laid on the ground, often at its very height: the floor is
            // met before the ground under it.
            offer(
                Hit::Ground {
                    point,
                    vertex: nearest_vertex(point),
                },
                d + GROUND_YIELD,
            );
        }
    }
    best
}

/// The grid vertex nearest world meters `p`.
#[must_use]
pub fn nearest_vertex(p: Vec3) -> (u32, u32) {
    let g = |v: f32| {
        u32::try_from(dereth_primitives::num::floor_to_i32((v / GRID).round()).max(0))
            .unwrap_or(0)
            .min(crate::brush::GRID_MAX)
    };
    (g(p.x), g(p.y))
}

/// Where `ray` first meets the ground of `landblocks`, and how far along it.
#[must_use]
pub fn ground_hit(session: &Session, ray: &Ray, landblocks: &[u16]) -> Option<(Vec3, f32)> {
    use dereth_terrain::land::mesh::{generate_landblock, Direction};
    let region = session.region()?;
    let mut best: Option<(Vec3, f32)> = None;
    for &lb in landblocks {
        let Some(t) = session.terrain(lb) else {
            continue;
        };
        let mesh = generate_landblock(
            &t,
            region,
            i32::from(lb >> 8),
            i32::from(lb & 0xFF),
            1,
            Direction::InViewerBlock,
        );
        let o = block_origin(lb);
        let local = Ray {
            origin: ray.origin.sub(o),
            dir: ray.dir,
        };
        for p in &mesh.polygons {
            let tri = p.v.map(|i| mesh.vertices[i as usize]);
            if let Some(d) = triangle_hit(&tri, &local) {
                if best.is_none_or(|(_, b)| d < b) {
                    best = Some((ray.origin.add(ray.dir.mul(d)), d));
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's pick geometry)
    use super::*;

    fn down(x: f32, y: f32) -> Ray {
        Ray {
            origin: Vec3::new(x, y, 100.0),
            dir: Vec3::new(0.0, 0.0, -1.0),
        }
    }

    #[test]
    fn a_ray_meets_a_triangle_from_either_side_and_misses_past_its_edges() {
        let t = [
            Vec3::new(0.0, 0.0, 10.0),
            Vec3::new(10.0, 0.0, 10.0),
            Vec3::new(0.0, 10.0, 10.0),
        ];
        assert_eq!(triangle_hit(&t, &down(2.0, 2.0)), Some(90.0));
        let up = Ray {
            origin: Vec3::new(2.0, 2.0, 0.0),
            dir: Vec3::new(0.0, 0.0, 1.0),
        };
        assert_eq!(triangle_hit(&t, &up), Some(10.0));
        assert_eq!(triangle_hit(&t, &down(6.0, 6.0)), None);
        assert_eq!(triangle_hit(&t, &down(-1.0, 2.0)), None);
    }

    #[test]
    fn a_model_is_met_at_its_nearest_triangle_and_not_at_all_outside_its_sphere() {
        // A box's top and bottom, 2 m apart.
        let quad = |z: f32| {
            [
                [
                    Vec3::new(-1.0, -1.0, z),
                    Vec3::new(1.0, -1.0, z),
                    Vec3::new(1.0, 1.0, z),
                ],
                [
                    Vec3::new(-1.0, -1.0, z),
                    Vec3::new(1.0, 1.0, z),
                    Vec3::new(-1.0, 1.0, z),
                ],
            ]
        };
        let m = Model::from_triangles(quad(0.0).into_iter().chain(quad(2.0)).collect());
        assert_eq!(m.hit(&down(0.5, 0.5)), Some(98.0));
        assert_eq!(m.hit(&down(5.0, 5.0)), None);
    }

    #[test]
    fn a_ray_in_another_landblock_is_moved_into_the_objects_frame() {
        // An object at landblock 0x0102's (10, 20, 5), turned a quarter about up.
        let f = Frame::new(
            Vec3::new(10.0, 20.0, 5.0),
            crate::compile::heading_quat(90.0),
        );
        let r = into_frame(&down(BLOCK + 10.0 + 3.0, 2.0 * BLOCK + 20.0), 0x0102, &f);
        // Three meters east of it is three meters ahead of it (north in its own frame): a heading
        // of 90 turns north to east.
        assert!(
            (r.origin.x - 0.0).abs() < 1e-4 && (r.origin.y - 3.0).abs() < 1e-4,
            "{r:?}"
        );
        assert!((r.origin.z - 95.0).abs() < 1e-4);
    }

    #[test]
    fn a_ground_point_names_its_nearest_grid_vertex() {
        assert_eq!(nearest_vertex(Vec3::new(35.9, 12.1, 0.0)), (1, 1));
        assert_eq!(nearest_vertex(Vec3::new(36.1, 11.9, 0.0)), (2, 0));
        assert_eq!(
            nearest_vertex(Vec3::new(-5.0, 1.0e6, 0.0)),
            (0, crate::brush::GRID_MAX)
        );
    }
}
