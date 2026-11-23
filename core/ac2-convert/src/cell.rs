//! An AC2 cell mesh as an AC1 environment of one cell shape: its drawn polygons with AC1 surfaces
//! made from AC2's materials, its doorways as portal polygons drawn clear, its physics polygons,
//! and a cell tree of the box about it.

use std::collections::{BTreeMap, BTreeSet};

use auberean_assets::mesh::CellMesh;
use auberean_world::records::WorldRecords;
use dereth_assets::geometry::CellStruct;
use dereth_assets::{Environment, Polygon, Surface, SwVertex, VertexArray};
use dereth_primitives::{DataId, Vec3};

use auberean_world::look::PartLook;

use crate::records::{NewIds, Pictures, Records};
use crate::source::Ac2;
use crate::surface::{self as statics};

/// A room's environment, the surfaces it draws with, the polygon of its shape each of AC2's
/// portals is, and each portal's corners in the room's frame.
pub type Room = (DataId, Vec<DataId>, Vec<u16>, Vec<Vec<Vec3>>);

/// A drawn polygon whose picture repeats past its edges.
pub const TILED: u8 = 1;
/// How an environment's polygons name their surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slots {
    /// One slot for each surface its materials make, as one cell draws it: a material drawn clear
    /// draws nothing.
    BySurface,
    /// One slot for each of the mesh's materials, in its order, whatever it is drawn with: the
    /// environment serves every look, a look being a list of surfaces ([`look_surfaces`]).
    ByMaterial,
}

/// AC2's cell mesh `mesh` as an AC1 environment of one cell shape: its id, the surfaces its cell
/// draws with, and the polygon of the shape each of AC2's portals is; `None` for a mesh that
/// will not read. Its cell tree is cut at its doorways, so it ends where the room beyond begins.
#[allow(clippy::too_many_arguments)]
pub fn environment(
    wr: &WorldRecords,
    ac2: &Ac2,
    mesh: u32,
    pictures: &mut Pictures,
    ids: &mut dyn NewIds,
    out: &mut Records,
    made: &mut BTreeMap<statics::MadeKey, statics::Made>,
) -> Result<Option<Room>, String> {
    shape(
        wr,
        ac2,
        mesh,
        Slots::BySurface,
        true,
        pictures,
        ids,
        out,
        made,
    )
}

/// [`environment`] with one surface slot for each of the mesh's materials in its order, then the
/// clear one its doorways draw with: the surfaces answered are the mesh's own look's, and
/// [`look_surfaces`] gives another look's for the same environment. Its cell tree is the box about
/// it, uncut.
#[allow(clippy::too_many_arguments)]
pub fn environment_by_material(
    wr: &WorldRecords,
    ac2: &Ac2,
    mesh: u32,
    pictures: &mut Pictures,
    ids: &mut dyn NewIds,
    out: &mut Records,
    made: &mut BTreeMap<statics::MadeKey, statics::Made>,
) -> Result<Option<Room>, String> {
    shape(
        wr,
        ac2,
        mesh,
        Slots::ByMaterial,
        false,
        pictures,
        ids,
        out,
        made,
    )
}

/// The surfaces an environment made by [`environment_by_material`] from `mesh` draws with in
/// the look a cell naming appearance `key` has (at strength 0, as AC2's dungeons name them;
/// `None`: the mesh's own materials): one for each of its materials (the clear one where the look
/// draws it clear), then the clear one. `None` for a mesh that will not read.
///
/// # Errors
/// A picture that will not read.
#[allow(clippy::too_many_arguments)]
pub fn look_surfaces(
    wr: &WorldRecords,
    ac2: &Ac2,
    mesh: u32,
    key: Option<u32>,
    pictures: &mut Pictures,
    ids: &mut dyn NewIds,
    out: &mut Records,
    made: &mut BTreeMap<statics::MadeKey, statics::Made>,
) -> Result<Option<Vec<DataId>>, String> {
    let Some(cm) = wr.get::<CellMesh>(mesh) else {
        return Ok(None);
    };
    let m = &cm.mesh;
    let look: Option<PartLook> =
        key.and_then(|k| auberean_world::place::appearance_look(wr, k, 0.0));
    let look = look.as_ref();
    let clear = clear_surface(ids, out);
    let mut surfaces = Vec::with_capacity(m.materials.len() + 1);
    for mi in &m.materials {
        let (surface, _, _) =
            statics::surface_of(ac2, mi.0, m.kind, look, pictures, ids, out, made)?;
        surfaces.push(surface.map_or(clear, |(s, _)| s));
    }
    surfaces.push(clear);
    Ok(Some(surfaces))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn shape(
    wr: &WorldRecords,
    ac2: &Ac2,
    mesh: u32,
    slots: Slots,
    cut_at_doorways: bool,
    pictures: &mut Pictures,
    ids: &mut dyn NewIds,
    out: &mut Records,
    made: &mut BTreeMap<statics::MadeKey, statics::Made>,
) -> Result<Option<Room>, String> {
    let Some(cm) = wr.get::<CellMesh>(mesh) else {
        return Ok(None);
    };
    let m = &cm.mesh;
    let Some(attrs) = m.vertices.attributes() else {
        return Ok(None);
    };
    let va = VertexArray {
        vertex_type: 1,
        vertices: attrs
            .iter()
            .enumerate()
            .map(|(k, v)| SwVertex {
                id: i16::try_from(k).unwrap_or(i16::MAX),
                position: Vec3::new(v.position.x, v.position.y, v.position.z),
                normal: v
                    .normal
                    .map_or(Vec3::new(0.0, 0.0, 1.0), |n| Vec3::new(n.x, n.y, n.z)),
                uvs: vec![{
                    let uv = v.uv.unwrap_or([0.0, 0.0]);
                    (uv[0], uv[1])
                }],
            })
            .collect(),
    };
    let total = |l: &auberean_assets::mesh::DetailLevel| {
        l.fragments.iter().map(|f| f.indices.len()).sum::<usize>()
    };
    // Its most detailed level; a mesh with none draws nothing.
    let best = (0..m.levels.len()).max_by_key(|&k| (total(&m.levels[k]), usize::MAX - k));
    let mut surfaces: Vec<DataId> = Vec::new();
    let mut polygons: Vec<Polygon> = Vec::new();
    // By material, the slots are the materials' own, filled as each is met; one drawn clear in
    // the mesh's own look keeps its polygons, drawn with the clear surface.
    let mut by_material: Vec<Option<DataId>> = vec![None; m.materials.len()];
    for f in best.map_or(&[][..], |b| m.levels[b].fragments.as_slice()) {
        let Some(&mi) = m.materials.get(f.material as usize) else {
            continue;
        };
        let (surface, _, _) =
            statics::surface_of(ac2, mi.0, m.kind, None, pictures, ids, out, made)?;
        let (s, both) = match (slots, surface) {
            (Slots::BySurface, None) => continue,
            (Slots::BySurface, Some((surface, both))) => {
                let s = surfaces
                    .iter()
                    .position(|x| *x == surface)
                    .unwrap_or_else(|| {
                        surfaces.push(surface);
                        surfaces.len() - 1
                    });
                (s, both)
            }
            (Slots::ByMaterial, surface) => {
                let k = f.material as usize;
                if let Some(slot) = by_material.get_mut(k) {
                    *slot = Some(surface.map_or(DataId(0), |(s, _)| s));
                }
                (k, surface.is_some_and(|(_, both)| both))
            }
        };
        let s = u16::try_from(s).unwrap_or(0);
        for tri in f.indices.as_chunks::<3>().0 {
            if tri.iter().any(|&i| usize::from(i) >= va.vertices.len()) {
                continue;
            }
            polygons.push(Polygon {
                poly_id: 0,
                num_pts: 3,
                // Its picture repeats past its edges, as AC2's do; else it clamps.
                stippling: TILED,
                sides_type: i32::from(both),
                pos_surface: s,
                neg_surface: if both { s } else { u16::MAX },
                vertex_ids: tri.to_vec(),
                pos_uv_indices: Some(vec![0; 3]),
                neg_uv_indices: both.then(|| vec![0; 3]),
            });
        }
    }
    // A mesh that draws nothing still makes a cell when it joins others: its portals are drawn.
    if polygons.is_empty() && cm.portals.is_empty() {
        return Ok(None);
    }
    // The doorways: each portal's physics polygon, a portal polygon as retail's are.
    let doorway: BTreeSet<u16> = cm.portals.iter().map(|p| p.polygon).collect();
    let mut portal_polygons = Vec::with_capacity(cm.portals.len());
    let mut portal_points = Vec::with_capacity(cm.portals.len());
    // A portal draws wholly clear, or a doorway shows as a slab.
    let clear = clear_surface(ids, out);
    if slots == Slots::ByMaterial {
        surfaces = by_material
            .iter()
            .map(|s| s.filter(|s| s.0 != 0).unwrap_or(clear))
            .collect();
    }
    let clear_slot = u16::try_from(surfaces.len()).map_err(|e| e.to_string())?;
    surfaces.push(clear);
    for p in &cm.portals {
        let Some(poly) = m.physics_polygons.get(usize::from(p.polygon)) else {
            return Ok(None);
        };
        portal_polygons.push(u16::try_from(polygons.len()).map_err(|e| e.to_string())?);
        portal_points.push(
            poly.vertices
                .iter()
                .filter_map(|&i| va.vertices.get(usize::from(i)).map(|v| v.position))
                .collect::<Vec<Vec3>>(),
        );
        polygons.push(Polygon {
            poly_id: 0,
            num_pts: u8::try_from(poly.vertices.len()).unwrap_or(u8::MAX),
            stippling: 4,
            sides_type: 0,
            pos_surface: clear_slot,
            neg_surface: u16::MAX,
            vertex_ids: poly.vertices.clone(),
            pos_uv_indices: None,
            neg_uv_indices: None,
        });
    }
    let physics: Vec<Polygon> = m
        .physics_polygons
        .iter()
        .enumerate()
        .filter(|(k, _)| !doorway.contains(&u16::try_from(*k).unwrap_or(u16::MAX)))
        .map(|(_, p)| Polygon {
            poly_id: 0,
            num_pts: u8::try_from(p.vertices.len()).unwrap_or(u8::MAX),
            stippling: 4,
            sides_type: 0,
            pos_surface: 0,
            neg_surface: 0,
            vertex_ids: p.vertices.clone(),
            pos_uv_indices: None,
            neg_uv_indices: None,
        })
        .collect();
    for (k, p) in polygons.iter_mut().enumerate() {
        p.poly_id = i16::try_from(k).unwrap_or(i16::MAX);
    }
    let mut physics = physics;
    for (k, p) in physics.iter_mut().enumerate() {
        p.poly_id = i16::try_from(k).unwrap_or(i16::MAX);
    }
    // Vertex ids stay the mesh's: one array serves the drawn and the physics polygons.
    // The cell tree: the box about the room, as AC2's own cell tree is (the hull of every vertex
    // of AC2's carved rooms takes the tree builder minutes), cut at its doorways when asked so it
    // ends where its neighbors begin.
    let cell_bsp = if cut_at_doorways {
        cell_tree(&va, &portal_points)
    } else {
        box_tree(&va)
    };
    let shape: CellStruct =
        dereth_builder::bsp::cell_struct(va, polygons, portal_polygons.clone(), physics, cell_bsp)?;
    let id = ids.next_for(0x0D, &format!("environment {mesh:08X} {slots:?}"));
    let env = Environment {
        id,
        cells: vec![shape],
    };
    out.add(id, env.encode());
    Ok(Some((id, surfaces, portal_polygons, portal_points)))
}

/// How far past its own doorway a cell's tree reaches: a point on the doorway is in both cells.
const DOOR_SLACK: f64 = 0.05;
/// A doorway with its own cell's vertices further than this behind its plane does not cut the
/// tree: the cell bends back behind its own doorway, and the cut would take some of it away.
const BEHIND_DOOR: f64 = 1.0;

type P = [f64; 3];

fn p_sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn p_dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn p_cross(a: P, b: P) -> P {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn p_of(v: Vec3) -> P {
    [f64::from(v.x), f64::from(v.y), f64::from(v.z)]
}

/// The plane of a polygon's corners by Newell's method, as `(normal, d)` with `normal . p + d`
/// the distance from it; none for a degenerate polygon.
fn plane_of(corners: &[P]) -> Option<(P, f64)> {
    let mut n = [0.0; 3];
    for (i, a) in corners.iter().enumerate() {
        let b = corners[(i + 1) % corners.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    // Plain arithmetic and a square root: the same bits on every host.
    let len = p_dot(n, n).sqrt();
    if corners.len() < 3 || len < 1e-9 {
        return None;
    }
    let n = [n[0] / len, n[1] / len, n[2] / len];
    let k = 1.0 / corners.len() as f64;
    let mut c = [0.0; 3];
    for q in corners {
        c = [c[0] + q[0] * k, c[1] + q[1] * k, c[2] + q[2] * k];
    }
    Some((n, -p_dot(n, c)))
}

/// The cell tree of the box about `va`'s vertices, cut by the plane of each of the cell's
/// `doorways` on the cell's own side, [`DOOR_SLACK`] to spare: a box alone reaches past a diagonal
/// doorway well into the cell beyond it, which then holds points the next cell draws, and a body
/// that went through the doorway is still held by the cell it came from. The box and its cuts are
/// one convex shape, its faces facing in.
fn cell_tree(va: &VertexArray, doorways: &[Vec<Vec3>]) -> dereth_assets::BspTree {
    let own: Vec<P> = va.vertices.iter().map(|v| p_of(v.position)).collect();
    if own.is_empty() {
        return box_tree(va);
    }
    let k = 1.0 / own.len() as f64;
    let mut mean = [0.0; 3];
    for q in &own {
        mean = [mean[0] + q[0] * k, mean[1] + q[1] * k, mean[2] + q[2] * k];
    }
    // The cuts: each doorway's plane facing the cell, unless the cell lies behind it.
    let mut cuts: Vec<(P, f64)> = Vec::new();
    for door in doorways {
        let corners: Vec<P> = door.iter().map(|v| p_of(*v)).collect();
        let Some((mut n, mut d)) = plane_of(&corners) else {
            continue;
        };
        if p_dot(n, mean) + d < 0.0 {
            n = [-n[0], -n[1], -n[2]];
            d = -d;
        }
        if own.iter().any(|q| p_dot(n, *q) + d < -BEHIND_DOOR) {
            continue;
        }
        cuts.push((n, d + DOOR_SLACK));
    }
    if cuts.is_empty() {
        return box_tree(va);
    }
    // The box's faces, each corner list facing in, as `box_tree` draws them.
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for q in &own {
        for a in 0..3 {
            lo[a] = lo[a].min(q[a]);
            hi[a] = hi[a].max(q[a]);
        }
    }
    let corner = |i: usize| -> P {
        [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ]
    };
    let mut faces: Vec<Vec<P>> = [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ]
    .iter()
    .map(|f| f.iter().map(|&i| corner(i)).collect())
    .collect();
    for (n, d) in cuts {
        let mut cap: Vec<P> = Vec::new();
        let mut kept = Vec::new();
        for face in faces {
            let mut out: Vec<P> = Vec::new();
            for (i, &a) in face.iter().enumerate() {
                let b = face[(i + 1) % face.len()];
                let (da, db) = (p_dot(n, a) + d, p_dot(n, b) + d);
                if da >= 0.0 {
                    out.push(a);
                }
                if (da >= 0.0) != (db >= 0.0) {
                    let t = da / (da - db);
                    let x = [
                        a[0] + (b[0] - a[0]) * t,
                        a[1] + (b[1] - a[1]) * t,
                        a[2] + (b[2] - a[2]) * t,
                    ];
                    out.push(x);
                    cap.push(x);
                }
            }
            if out.len() >= 3 {
                kept.push(out);
            }
        }
        faces = kept;
        // The cut's own face: its points in turn about their middle, so it is one simple polygon
        // (the tree builder faces each face in by the points it holds, whichever way it winds).
        let mut ring: Vec<P> = Vec::new();
        for q in cap {
            if !ring
                .iter()
                .any(|r| p_dot(p_sub(*r, q), p_sub(*r, q)) < 1e-12)
            {
                ring.push(q);
            }
        }
        if ring.len() >= 3 {
            let kr = 1.0 / ring.len() as f64;
            let mut c = [0.0; 3];
            for q in &ring {
                c = [c[0] + q[0] * kr, c[1] + q[1] * kr, c[2] + q[2] * kr];
            }
            let u = p_sub(ring[0], c);
            let w = p_cross(n, u);
            // Which half of the turn, then the turn from one to the other: no angle is worked out.
            let half = |q: &P| {
                let (x, y) = (p_dot(p_sub(*q, c), u), p_dot(p_sub(*q, c), w));
                (y < 0.0 || (y == 0.0 && x < 0.0), x, y)
            };
            ring.sort_by(|a, b| {
                let (ha, xa, ya) = half(a);
                let (hb, xb, yb) = half(b);
                ha.cmp(&hb).then_with(|| (xb * ya).total_cmp(&(xa * yb)))
            });
            faces.push(ring);
        }
    }
    if faces.len() < 4 {
        return box_tree(va);
    }
    // One vertex array for the shape, each face's corners by index.
    let mut points: Vec<P> = Vec::new();
    let mut polygons = Vec::new();
    for face in &faces {
        let mut ids = Vec::new();
        for q in face {
            let i = points
                .iter()
                .position(|r| p_dot(p_sub(*r, *q), p_sub(*r, *q)) < 1e-12)
                .unwrap_or_else(|| {
                    points.push(*q);
                    points.len() - 1
                });
            let i = u16::try_from(i).unwrap_or(u16::MAX);
            if ids.last() != Some(&i) && ids.first() != Some(&i) {
                ids.push(i);
            }
        }
        if ids.len() >= 3 {
            polygons.push(Polygon {
                poly_id: i16::try_from(polygons.len()).unwrap_or(0),
                num_pts: u8::try_from(ids.len()).unwrap_or(u8::MAX),
                stippling: 4,
                sides_type: 0,
                pos_surface: 0,
                neg_surface: 0,
                vertex_ids: ids,
                pos_uv_indices: None,
                neg_uv_indices: None,
            });
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    let shape = VertexArray {
        vertex_type: 1,
        vertices: points
            .iter()
            .enumerate()
            .map(|(i, q)| SwVertex {
                id: i16::try_from(i).unwrap_or(0),
                position: Vec3::new(q[0] as f32, q[1] as f32, q[2] as f32),
                normal: Vec3::new(0.0, 0.0, 1.0),
                uvs: Vec::new(),
            })
            .collect(),
    };
    dereth_builder::bsp::cell(&shape, &polygons)
}

/// The cell tree of the box about `va`'s vertices: its six faces, facing in.
pub fn box_tree(va: &VertexArray) -> dereth_assets::BspTree {
    let (mut lo, mut hi) = (
        Vec3::new(f32::MAX, f32::MAX, f32::MAX),
        Vec3::new(f32::MIN, f32::MIN, f32::MIN),
    );
    for v in &va.vertices {
        let p = v.position;
        lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    let corner = |i: u16| {
        Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    };
    let corners = VertexArray {
        vertex_type: 1,
        vertices: (0..8u16)
            .map(|i| SwVertex {
                id: i16::try_from(i).unwrap_or(0),
                position: corner(i),
                normal: Vec3::new(0.0, 0.0, 1.0),
                uvs: Vec::new(),
            })
            .collect(),
    };
    // Each face's corners in the order that faces it inward.
    let faces: [[u16; 4]; 6] = [
        [0, 1, 3, 2], // floor, facing up
        [4, 6, 7, 5], // ceiling, facing down
        [0, 4, 5, 1], // south, facing north
        [2, 3, 7, 6], // north, facing south
        [0, 2, 6, 4], // west, facing east
        [1, 5, 7, 3], // east, facing west
    ];
    let polygons: Vec<Polygon> = faces
        .iter()
        .enumerate()
        .map(|(k, f)| Polygon {
            poly_id: i16::try_from(k).unwrap_or(0),
            num_pts: 4,
            stippling: 4,
            sides_type: 0,
            pos_surface: 0,
            neg_surface: 0,
            vertex_ids: f.to_vec(),
            pos_uv_indices: None,
            neg_uv_indices: None,
        })
        .collect();
    dereth_builder::bsp::cell(&corners, &polygons)
}

/// A wholly clear surface, as a doorway's polygon draws with.
pub fn clear_surface(ids: &mut dyn NewIds, out: &mut Records) -> DataId {
    let id = ids.next_for(0x08, "clear");
    out.add(
        id,
        Surface {
            surface_type: 0x11,
            orig_texture_id: None,
            orig_palette_id: None,
            color_value: Some(0xFF00_0000),
            translucency: 1.0,
            luminosity: 0.0,
            diffuse: 1.0,
        }
        .encode(),
    );
    id
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: an AC2 cell's tree as AC1's)

    use super::*;

    /// Whether a cell tree holds `p`, as the client reads one: no plane of its chain puts the
    /// point behind it by more than the client's slack.
    fn holds(tree: &dereth_assets::BspTree, p: Vec3) -> bool {
        tree.nodes.iter().filter_map(|n| n.plane).all(|pl| {
            pl.normal.x * p.x + pl.normal.y * p.y + pl.normal.z * p.z + pl.d
                >= -dereth_primitives::num::consts::EPSILON
        })
    }

    /// A cell's vertices: `corners` in plan, from the floor to 5 m up.
    fn cell(corners: &[(f32, f32)]) -> VertexArray {
        VertexArray {
            vertex_type: 1,
            vertices: [0.0, 5.0]
                .iter()
                .flat_map(|&z| corners.iter().map(move |&(x, y)| Vec3::new(x, y, z)))
                .enumerate()
                .map(|(i, position)| SwVertex {
                    id: i16::try_from(i).unwrap_or(0),
                    position,
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    uvs: Vec::new(),
                })
                .collect(),
        }
    }

    /// A doorway in the plane x + y = 30, from (20, 10) to (10, 20), floor to ceiling.
    fn diagonal_doorway() -> Vec<Vec3> {
        vec![
            Vec3::new(20.0, 10.0, 0.0),
            Vec3::new(10.0, 20.0, 0.0),
            Vec3::new(10.0, 20.0, 5.0),
            Vec3::new(20.0, 10.0, 5.0),
        ]
    }

    #[test]
    fn a_cells_tree_ends_at_its_diagonal_doorway_where_its_box_reached_into_the_next_cell() {
        // A room cut off at a diagonal doorway, and the room beyond it.
        let room = cell(&[
            (0.0, 0.0),
            (20.0, 0.0),
            (20.0, 10.0),
            (10.0, 20.0),
            (0.0, 20.0),
        ]);
        let beyond = cell(&[(20.0, 10.0), (30.0, 20.0), (20.0, 30.0), (10.0, 20.0)]);
        let door = vec![diagonal_doorway()];
        // In the room, 6 m short of the doorway: the next room's box reaches there.
        let in_the_room = Vec3::new(12.0, 12.0, 1.0);
        assert!(
            holds(&box_tree(&beyond), in_the_room),
            "the fault: the box reaches back"
        );
        assert!(!holds(&cell_tree(&beyond, &door), in_the_room));
        assert!(holds(&cell_tree(&room, &door), in_the_room));
        // Past the doorway, in the next room: the room's box reached there; its tree does not.
        let beyond_the_door = Vec3::new(19.0, 19.0, 1.0);
        assert!(holds(&box_tree(&room), beyond_the_door));
        assert!(!holds(&cell_tree(&room, &door), beyond_the_door));
        assert!(holds(&cell_tree(&beyond, &door), beyond_the_door));
        // On the doorway itself, both hold it; each room holds its own far corner and ceiling.
        let on = Vec3::new(15.0, 15.0, 2.5);
        assert!(holds(&cell_tree(&room, &door), on) && holds(&cell_tree(&beyond, &door), on));
        assert!(holds(&cell_tree(&room, &door), Vec3::new(0.5, 0.5, 4.9)));
        assert!(holds(
            &cell_tree(&beyond, &door),
            Vec3::new(29.0, 20.0, 4.9)
        ));
        // Outside the box is outside still.
        assert!(!holds(&cell_tree(&room, &door), Vec3::new(-1.0, 5.0, 1.0)));
        assert!(!holds(&cell_tree(&room, &door), Vec3::new(5.0, 5.0, 6.0)));
    }

    #[test]
    fn a_doorway_with_its_own_cell_behind_it_does_not_cut_the_tree() {
        // The room bends round behind its doorway's plane: 10 m of it lies past x + y = 30.
        let room = cell(&[
            (0.0, 0.0),
            (20.0, 0.0),
            (30.0, 20.0),
            (20.0, 20.0),
            (0.0, 20.0),
        ]);
        let door = vec![diagonal_doorway()];
        let behind = Vec3::new(24.0, 15.0, 1.0);
        assert!(holds(&cell_tree(&room, &door), behind));
        assert_eq!(cell_tree(&room, &door), box_tree(&room));
        // With no doorway at all, the tree is the box.
        assert_eq!(cell_tree(&room, &[]), box_tree(&room));
    }
}
