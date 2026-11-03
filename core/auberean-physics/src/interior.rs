//! The world's interior cells for the shared transition: the rooms of buildings and dungeons, each
//! with its walls and floors as a tree of polygons, its inside test, and its doorways to the cells
//! beside it.
//!
//! An interior cell is the shared core's own kind of cell (its environment cell): its geometry is
//! the cell mesh's physics polygons and tree in the cell's own frame; its inside test the cell
//! mesh's cell tree; its doorways the cell's portals, each one of the cell mesh's portal polygons
//! with the cell on its far side (or none). The shared search then
//! finds the cells a body touches by walking through the doorways, as it does for the first game's
//! interiors.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::landblock::Cell;
use auberean_assets::mesh::CellMesh;
use auberean_world::records::WorldRecords;
use dereth_physics::geom::{BspTree, Polygon};
use dereth_physics::source::{CellPortal, EnvCellGeometry};
use dereth_physics::V3;
use dereth_primitives::{CellId, Frame, Quat, Vec3};

use crate::land::{segment_distance, segment_polygon, CellMaterial, Land, Solid};
use crate::mesh::{convert, physics_polygons, physics_tree, positions, Trees};

/// The lowest cell index of an interior cell; land cells are numbered 1 to 64.
pub const FIRST_INTERIOR: u32 = 0x100;
/// A doorway with no cell on its far side.
const NO_OTHER_CELL: u16 = 0x2;

/// Whether `cell` is an interior cell.
#[must_use]
pub fn is_interior(cell: CellId) -> bool {
    cell.0 & 0xFFFF >= FIRST_INTERIOR
}

fn v3(v: auberean_primitives::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// Cell `low` of block `(x, y)` as the shared core's interior cell, from the cell's record and its
/// mesh; `None` when the mesh's vertices store no positions. A cell mesh with no physics tree is
/// open air: nothing in it stops a body but the objects standing there, and its polygons are its
/// doorways. A doorway flagged as having no cell beyond leads outdoors. A cell with no doorways
/// still holds what its cell tree says it does: the training ground's closed rooms are stood in.
// D2-04243, D2-03490, D2-03492
#[must_use]
pub fn cell_geometry(
    cell: &Cell,
    mesh: &CellMesh,
    x: u8,
    y: u8,
    low: u16,
) -> Option<EnvCellGeometry> {
    let block = u32::from(x) << 24 | u32::from(y) << 16;
    let at = positions(&mesh.mesh.vertices)?;
    let polygons = physics_polygons(&mesh.mesh)?;
    let by_id: HashMap<u16, u32> = mesh
        .mesh
        .physics_polygons
        .iter()
        .enumerate()
        .filter_map(|(i, p)| Some((p.id, u32::try_from(i).ok()?)))
        .collect();
    let inside: BspTree = convert(&mesh.cell_tree, polygons.clone(), &by_id);
    let portals = cell
        .portals
        .iter()
        .filter_map(|p| {
            let mp = mesh.portals.get(usize::from(p.portal))?;
            let poly = mesh
                .mesh
                .physics_polygons
                .get(usize::from(mp.polygon))
                .and_then(|mp| {
                    let v: Option<Vec<Vec3>> = mp
                        .vertices
                        .iter()
                        .map(|&k| at.get(usize::from(k)).copied())
                        .collect();
                    v.filter(|v| v.len() >= 3).map(Polygon::new)
                })?;
            // D2-04239, D2-04240, D2-04233
            let other = if p.flags & NO_OTHER_CELL != 0 {
                crate::cells::OUTDOORS
            } else {
                block | u32::from(p.other_cell)
            };
            Some(CellPortal {
                other_cell_id: other,
                portal: poly,
                portal_side: mp.side != 0,
                other_portal_id: i32::from(p.other_portal),
                exact_match: p.flags & 0x1 != 0,
            })
        })
        .collect();
    let q = cell.frame.rotation;
    Some(EnvCellGeometry {
        id: CellId(block | u32::from(low)),
        frame: Frame::new(v3(cell.frame.origin), Quat::new(q.w, q.x, q.y, q.z)),
        portals,
        physics_polygons: polygons,
        physics_bsp: physics_tree(&mesh.mesh).map(Arc::new),
        cell_bsp: Some(Arc::new(inside)),
        stab_list: cell
            .visible
            .iter()
            .map(|&c| CellId(block | u32::from(c)))
            .collect(),
        seen_outside: cell.flags & 0x1 != 0,
        holds_without_portals: true,
        static_objects: Vec::new(),
    })
}

/// The solid parts of the entities placed in `cell`, in its landblock's frame, each where its
/// entity stands ([`crate::object::placed_solids`]).
fn cell_solids(records: &WorldRecords, trees: &mut Trees, cell: &Cell, id: CellId) -> Vec<Solid> {
    let mut out = Vec::new();
    for e in cell.entities.iter().flat_map(|g| g.entities.iter()) {
        out.extend(crate::object::placed_solids(records, trees, e, id));
    }
    out
}

/// Add every interior cell of block `(x, y)` to `land`, with the solid parts of the entities
/// placed in each and the entities that collide through cylinders or spheres; returns how many
/// cells were added.
// D2-03581, D2-03574
pub fn add_block_cells(
    land: &Land,
    records: &WorldRecords,
    trees: &mut Trees,
    x: u8,
    y: u8,
) -> usize {
    let (bx, by) = (i32::from(x), i32::from(y));
    let Some(info) = records.landblock_info(bx, by) else {
        return 0;
    };
    land.properties_or(|| records.block_properties().map(Arc::new));
    let block_properties = Arc::new(info.properties.clone());
    let mut meshes: HashMap<u32, Option<Arc<CellMesh>>> = HashMap::new();
    let mut added = 0;
    for k in 0..info.cell_count {
        let Ok(low) = u16::try_from(FIRST_INTERIOR + k) else {
            break;
        };
        let Some(cell) = records.cell(bx, by, low) else {
            continue;
        };
        let Some(mesh) = meshes
            .entry(cell.mesh.0)
            .or_insert_with(|| records.get::<CellMesh>(cell.mesh.0).map(Arc::new))
            .clone()
        else {
            continue;
        };
        if let Some(g) = cell_geometry(&cell, &mesh, x, y, low) {
            let objects = cell
                .entities
                .iter()
                .flat_map(|group| group.entities.iter())
                .filter_map(|e| crate::object::placed_entity(records, trees, e, g.id))
                .map(|(p, s)| (p, Arc::new(s)))
                .collect();
            land.insert_cell_material(
                g.id,
                CellMaterial {
                    block: Arc::clone(&block_properties),
                    own: cell.properties.clone(),
                },
            );
            land.insert_cell_objects(g.id, objects);
            land.insert_cell_solids(g.id, cell_solids(records, trees, &cell, g.id));
            land.insert_cell(g);
            added += 1;
        }
    }
    land.connect_cells(x, y, &info.outside_visible);
    added
}

/// Where the line from `from` to `to` (in the frame of `cell`'s landblock) first meets a wall,
/// floor or ceiling of interior cell `cell` or of a cell it sees, coming at its face, or, with
/// `objects`, a solid part of what stands in them: the point and the surface's normal. A cell's
/// doorways are open: only the polygons its collision tree holds stop the line.
#[must_use]
pub fn trace(
    land: &Land,
    cell: CellId,
    from: Vec3,
    to: Vec3,
    objects: bool,
) -> Option<(Vec3, Vec3)> {
    let home = land.cell(cell)?;
    let d = to.sub(from);
    let mut best: Option<(f32, Vec3)> = None;
    let ids = std::iter::once(cell).chain(home.stab_list.iter().copied());
    for id in ids {
        let solids = if objects { land.cell_solids(id) } else { None };
        for s in solids.iter().flat_map(|v| v.iter()) {
            if segment_distance(s.bounds.center, from, to) > s.bounds.radius {
                continue;
            }
            let Some(tree) = s.part.physics_bsp.as_ref() else {
                continue;
            };
            let frame = &s.part.pos.frame;
            let inv = if s.part.gfxobj_scale > 0.0 {
                1.0 / s.part.gfxobj_scale
            } else {
                1.0
            };
            let lf = dereth_physics::math::globaltolocal(frame, from).mul(inv);
            let lt = dereth_physics::math::globaltolocal(frame, to).mul(inv);
            for poly in &tree.polygons {
                if let Some(t) = segment_polygon(lf, lt, &poly.vertices) {
                    if best.is_none_or(|(b, _)| t < b) {
                        let n = dereth_physics::math::localtoglobalvec(
                            dereth_physics::math::l2g(frame.rotation),
                            poly.plane.normal,
                        );
                        best = Some((t, n));
                    }
                }
            }
        }
        let Some(g) = land.cell(id) else { continue };
        let Some(tree) = g.physics_bsp.as_ref() else {
            continue;
        };
        let lf = dereth_physics::math::globaltolocal(&g.frame, from);
        let lt = dereth_physics::math::globaltolocal(&g.frame, to);
        let ld = lt.sub(lf);
        // The tree's polygons are the walls, floors and ceilings; a doorway's polygon is not in it.
        let walls = tree
            .nodes
            .iter()
            .flat_map(|n| n.in_polys.iter())
            .filter_map(|&i| tree.polygons.get(i as usize));
        for poly in walls {
            // Only a face the line comes at from the front.
            if ld.dot(poly.plane.normal) >= 0.0 {
                continue;
            }
            if let Some(t) = segment_polygon(lf, lt, &poly.vertices) {
                if best.is_none_or(|(b, _)| t < b) {
                    let n = dereth_physics::math::localtoglobalvec(
                        dereth_physics::math::l2g(g.frame.rotation),
                        poly.plane.normal,
                    );
                    best = Some((t, n));
                }
            }
        }
    }
    best.map(|(t, n)| (from.add(d.mul(t)), n))
}
