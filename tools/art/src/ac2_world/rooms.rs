//! AC2's buildings with rooms of their own, made AC1 buildings: the shell, its doorways and its
//! rooms as AC1's environment cells.
//!
//! **Which.** AC2's houses are shells with nothing inside; its rooms beside a shell are few: the
//! vault entrances' halls (one room, one doorway out), a castle's passages and two structures.
//! A building here is a placed static whose
//! origin stands on a room seen from outside; its rooms are that room and every room joined to
//! it. A group of rooms that another shell also stands on is left as statics (a castle's passage
//! runs under several), as is a shell AC2 scales, and one joined to more than eight rooms (a
//! dungeon under its entrance, the dungeon kit's to take).
//!
//! **A room** is AC2's cell mesh as one AC1 cell shape: its most detailed level's triangles, each
//! material drawn as the statics draw theirs ([`statics::surface_of`]); its doorways, each the
//! physics polygon AC2's portal names, a portal polygon as retail's are; the rest of its physics
//! polygons to collide with; and its trees ([`dereth_builder::bsp::cell_struct`]).
//! It stands where AC2 has it, moved with its shell into the converted world.
//!
//! **The shell** is the static's model with its collision in it, as AC1's shells carry theirs; its
//! doorways are the rooms' ways out, each naming the room, the way's place among the room's, and
//! every room of the building. AC1 meets a building's collision only from the land cell that holds
//! its origin, and AC2's vault entrances stand on plinths reaching out of that cell, so a body
//! walked in from the cell beside it passes under the plinth's steps and is caught inside them.
//! So the shell's collision is placed again beside it as a collision-only static, which AC1 meets
//! from every cell it reaches into.

use std::collections::{BTreeMap, BTreeSet};

use crate::ac2_look::source::Ac2;
use auberean_assets::mesh::CellMesh;
use auberean_world::place::DRAWN_KINDS;
use auberean_world::records::WorldRecords;
use dereth_assets::geometry::CellStruct;
use dereth_assets::world::{BuildInfo, BuildingPortal, CellPortal, LandblockInfo, ObjectEntry};
use dereth_assets::{
    BspNode, BspTree, Decode, EnvCell, Environment, GfxObj, Polygon, Surface, SwVertex, VertexArray,
};
use dereth_primitives::frame::V3;
use dereth_primitives::{DataId, EraId, Frame, Quat, Vec3};

use crate::ac2_world::art::{Ids, Pictures, Records};
use crate::ac2_world::statics::{self, Shell};

/// A room's environment, the surfaces it draws with, the polygon of its shape each of AC2's
/// portals is, and each portal's corners in the room's frame.
pub(crate) type Room = (DataId, Vec<DataId>, Vec<u16>, Vec<Vec<Vec3>>);

/// A cell flag AC2 sets on a room seen from outside, and a portal flag AC2 sets on a way out.
const SEEN_OUTSIDE: u32 = 1;
const NO_OTHER_CELL: u16 = 2;
/// AC1's flags on a room's way out (an exact match, to the outdoors), and on a building's
/// doorway, as retail's cottages have them.
const OUT_FLAGS: u16 = 1 | 4;
const DOORWAY_FLAGS: u16 = 3;
/// A drawn polygon whose picture repeats past its edges.
const TILED: u8 = 1;
/// How near a shell's origin stands to its room's.
const ON_ROOM: f32 = 0.5;
/// The most rooms a building takes: more joined to a shell's room are a dungeon under an entrance,
/// which are written as a dungeon of their own ([`super::vaults`]).
const MOST_ROOMS: usize = 8;

/// The shells AC2's rooms belong to: AC2's block and the entity's index there, with its rooms by
/// their numbers in that block.
#[derive(Debug, Default)]
pub struct Found {
    pub rooms: BTreeMap<(usize, usize, usize), Vec<u16>>,
    /// Groups of rooms passed over, by why.
    pub passed: BTreeMap<&'static str, usize>,
}

impl Found {
    #[must_use]
    pub fn shells(&self) -> BTreeSet<(usize, usize, usize)> {
        self.rooms.keys().copied().collect()
    }
}

/// Every building of AC2's with rooms of its own ([`Found`]).
#[must_use]
pub fn find(wr: &WorldRecords) -> Found {
    let mut found = Found::default();
    for bx in 0..255usize {
        for by in 0..255usize {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            let (x, y) = (bx as i32, by as i32);
            let Some(info) = wr.landblock_info(x, y) else {
                continue;
            };
            if info.cell_count == 0 {
                continue;
            }
            let cells: BTreeMap<u16, auberean_assets::landblock::Cell> = (0..info.cell_count)
                .filter_map(|i| {
                    let low = 0x0100 + u16::try_from(i).ok()?;
                    Some((low, wr.cell(x, y, low)?))
                })
                .collect();
            // The shells standing on a room seen from outside.
            let mut on: BTreeMap<u16, usize> = BTreeMap::new();
            for (index, e) in info.entities.entities.iter().enumerate() {
                if !e.kind.is_some_and(|k| DRAWN_KINDS.contains(&k)) || e.object.is_none() {
                    continue;
                }
                let Some(o) = e.offset.as_ref().map(|f| f.frame.origin) else {
                    continue;
                };
                for (low, c) in &cells {
                    let p = c.frame.origin;
                    let d =
                        ((p.x - o.x).powi(2) + (p.y - o.y).powi(2) + (p.z - o.z).powi(2)).sqrt();
                    if c.flags & SEEN_OUTSIDE != 0 && d <= ON_ROOM {
                        on.insert(*low, index);
                    }
                }
            }
            let mut taken: BTreeSet<u16> = BTreeSet::new();
            for (&low, &index) in &on {
                if taken.contains(&low) {
                    continue;
                }
                // The room and every room joined to it.
                let mut group = BTreeSet::from([low]);
                let mut todo = vec![low];
                while let Some(n) = todo.pop() {
                    for p in cells.get(&n).map_or(&[][..], |c| c.portals.as_slice()) {
                        if p.flags & NO_OTHER_CELL == 0
                            && cells.contains_key(&p.other_cell)
                            && group.insert(p.other_cell)
                        {
                            todo.push(p.other_cell);
                        }
                    }
                }
                taken.extend(group.iter().copied());
                let shells: BTreeSet<usize> =
                    group.iter().filter_map(|r| on.get(r)).copied().collect();
                if shells.len() > 1 {
                    *found
                        .passed
                        .entry("rooms under more than one shell")
                        .or_default() += 1;
                    continue;
                }
                if group.len() > MOST_ROOMS {
                    *found
                        .passed
                        .entry("a dungeon under an entrance")
                        .or_default() += 1;
                    continue;
                }
                found
                    .rooms
                    .insert((bx, by, index), group.into_iter().collect());
            }
        }
    }
    found
}

/// A building made: its AC1 block, its entry and its rooms, the rooms numbered from 0 in its
/// entry, their portals and their lists (renumbered when it is added, [`add`]).
#[derive(Clone, Debug)]
pub struct Building {
    pub block: (usize, usize),
    pub info: BuildInfo,
    pub rooms: Vec<EnvCell>,
}

/// The buildings `found` names, from the statics `shells` made of their shells; each shell's
/// collision is added to `solids`, its block's statics, as a collision-only static.
///
/// # Errors
/// A record that will not encode, or one of the materials' pictures that will not convert.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn make(
    found: &Found,
    shells: &[Shell],
    wr: &WorldRecords,
    ac2: &Ac2,
    pictures: &mut Pictures,
    ids: &mut Ids,
    out: &mut Records,
    passed: &mut BTreeMap<&'static str, usize>,
    solids: &mut BTreeMap<(usize, usize), Vec<ObjectEntry>>,
) -> Result<Vec<Building>, String> {
    let mut made = Vec::new();
    // Each cell mesh made once: its environment, its surfaces, and its portals' polygons.
    let mut environments: BTreeMap<u32, Option<Room>> = BTreeMap::new();
    let mut surfaces = BTreeMap::new();
    for shell in shells {
        let Some(rooms) = found.rooms.get(&shell.ac2) else {
            continue;
        };
        if shell.scale != [1000, 1000, 1000] {
            *passed.entry("a shell AC2 scales").or_default() += 1;
            continue;
        }
        if shell.parts.len() != 1 {
            *passed.entry("a shell of more than one model").or_default() += 1;
            continue;
        }
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        // LINT-OK: an AC2 block index, 0 to 254.
        let (x, y) = (shell.ac2.0 as i32, shell.ac2.1 as i32);
        let number: BTreeMap<u16, u16> = rooms
            .iter()
            .enumerate()
            .map(|(k, &low)| (low, u16::try_from(k).unwrap_or(u16::MAX)))
            .collect();
        let mut cells = Vec::with_capacity(rooms.len());
        let mut doorways = Vec::new();
        // Each doorway's corners in the shell's frame, and the middle of the room it leads into.
        let mut doorway_shapes: Vec<(Vec<Vec3>, Vec3)> = Vec::new();
        let mut whole = true;
        for (k, &low) in rooms.iter().enumerate() {
            let Some(c) = wr.cell(x, y, low) else {
                whole = false;
                break;
            };
            if let std::collections::btree_map::Entry::Vacant(slot) = environments.entry(c.mesh.0) {
                slot.insert(environment(
                    wr,
                    ac2,
                    c.mesh.0,
                    pictures,
                    ids,
                    out,
                    &mut surfaces,
                )?);
            }
            let Some((env, cell_surfaces, portal_polygons, portal_points)) =
                environments[&c.mesh.0].clone()
            else {
                whole = false;
                break;
            };
            let mut portals = Vec::with_capacity(c.portals.len());
            let mut ways_out = Vec::new();
            for p in &c.portals {
                let Some(&polygon_id) = portal_polygons.get(usize::from(p.portal)) else {
                    continue;
                };
                let out_of = p.flags & NO_OTHER_CELL != 0 || !number.contains_key(&p.other_cell);
                if out_of {
                    doorways.push((k, portals.len()));
                    ways_out.push(usize::from(p.portal));
                    portals.push(CellPortal {
                        flags: OUT_FLAGS,
                        polygon_id,
                        other_cell_id: 0xFFFF_FFFF,
                        other_portal_id: -1,
                    });
                } else {
                    portals.push(CellPortal {
                        flags: u16::from(p.flags & 1 != 0),
                        polygon_id,
                        other_cell_id: u32::from(number[&p.other_cell]),
                        other_portal_id: i16::try_from(p.other_portal).unwrap_or(-1),
                    });
                }
            }
            // Where it stands: moved with its shell, its height over the shell's kept.
            let o = c.frame.origin;
            let (wx, wy) = (
                shell.ac2.0 as f64 * 160.0 + f64::from(o.x),
                shell.ac2.1 as f64 * 160.0 + f64::from(o.y),
            );
            #[allow(clippy::cast_possible_truncation)]
            let at = Vec3::new(
                shell.frame.origin.x + (wx - shell.ac2_at.0) as f32,
                shell.frame.origin.y + (wy - shell.ac2_at.1) as f32,
                shell.frame.origin.z + (f64::from(o.z) - shell.ac2_at.2) as f32,
            );
            let r = c.frame.rotation;
            let r = Quat {
                w: r.w,
                x: r.x,
                y: r.y,
                z: r.z,
            };
            let to_shell = |p: Vec3| {
                let world = at.add(turn(r, p));
                turn(back(shell.frame.rotation), world.sub(shell.frame.origin))
            };
            for &w in &ways_out {
                let corners = portal_points.get(w).cloned().unwrap_or_default();
                doorway_shapes.push((
                    corners.into_iter().map(to_shell).collect(),
                    to_shell(Vec3::ZERO),
                ));
            }
            let k16 = u16::try_from(k).unwrap_or(u16::MAX);
            cells.push(EnvCell {
                id: DataId(u32::from(k16)),
                flags: c.flags & SEEN_OUTSIDE,
                cell_id_repeat: u32::from(k16),
                surfaces: cell_surfaces,
                environment: env,
                cell_struct: 0,
                frame: Frame::new(at, r),
                portals,
                // A building's room seen from outside sees every other room of its building.
                visible_cells: (0..rooms.len())
                    .filter(|&j| j != k)
                    .map(|j| u16::try_from(j).unwrap_or(u16::MAX))
                    .collect(),
                static_objects: Vec::new(),
                restriction_obj: None,
            });
        }
        if !whole {
            *passed
                .entry("a room whose mesh does not convert")
                .or_default() += 1;
            continue;
        }
        let Some(model) = shell_model(shell, &doorway_shapes, ids, out)? else {
            *passed.entry("a shell of more than one model").or_default() += 1;
            continue;
        };
        if let Some(solid) = shell_solid(shell, ids, out)? {
            solids.entry(shell.block).or_default().push(solid);
        }
        let all: Vec<u16> = (0..rooms.len())
            .map(|j| u16::try_from(j).unwrap_or(u16::MAX))
            .collect();
        let info = BuildInfo {
            id: model,
            frame: shell.frame,
            num_leaves: 0,
            portals: doorways
                .iter()
                .map(|&(room, way)| BuildingPortal {
                    flags: DOORWAY_FLAGS,
                    other_cell_id: u16::try_from(room).unwrap_or(u16::MAX),
                    other_portal_id: i16::try_from(way).unwrap_or(-1),
                    stab_list: all.clone(),
                })
                .collect(),
        };
        made.push(Building {
            block: shell.block,
            info,
            rooms: cells,
        });
    }
    Ok(made)
}

/// The shell's collision again, as a collision-only static where the shell stands, met from every
/// land cell it reaches into (AC1 meets a building's own only from the cell holding its origin);
/// `None` for a shell with no collision.
///
/// # Errors
/// A record that will not encode.
pub(crate) fn shell_solid(
    shell: &Shell,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Option<ObjectEntry>, String> {
    let Some(c) = &shell.collision else {
        return Ok(None);
    };
    let id = statics::collision_setup(c, ids, out)?;
    Ok(Some(ObjectEntry {
        id,
        frame: shell.frame,
    }))
}

/// The shell's model with its collision in it, and its doorways (corners in its frame, and the
/// middle of the room each leads into) as clear polygons facing in, the portal nodes of its
/// drawing tree, through which its rooms are drawn from outside: `None` for a shell of more than
/// one model.
fn shell_model(
    shell: &Shell,
    doorways: &[(Vec<Vec3>, Vec3)],
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Option<DataId>, String> {
    let [part] = shell.parts.as_slice() else {
        return Ok(None);
    };
    let bytes = out
        .0
        .get(part)
        .ok_or_else(|| format!("{part:?} is not made"))?;
    let mut g = GfxObj::decode_payload(*part, bytes).map_err(|e| format!("{part:?}: {e:?}"))?;
    if let Some(c) = &shell.collision {
        let first = g.vertex_array.vertices.len();
        for (k, p) in c.points.iter().enumerate() {
            g.vertex_array.vertices.push(SwVertex {
                id: i16::try_from(first + k).unwrap_or(i16::MAX),
                position: *p,
                normal: Vec3::new(0.0, 0.0, 1.0),
                uvs: Vec::new(),
            });
        }
        g.physics_polygons = c
            .rings
            .iter()
            .enumerate()
            .map(|(n, ring)| Polygon {
                poly_id: i16::try_from(n).unwrap_or(i16::MAX),
                num_pts: u8::try_from(ring.len()).unwrap_or(u8::MAX),
                stippling: 4,
                sides_type: 0,
                pos_surface: 0,
                neg_surface: 0,
                vertex_ids: ring
                    .iter()
                    .map(|&i| u16::try_from(first + usize::from(i)).unwrap_or(u16::MAX))
                    .collect(),
                pos_uv_indices: None,
                neg_uv_indices: None,
            })
            .collect();
        g.physics_bsp = Some(dereth_builder::bsp::physics(
            &g.vertex_array,
            &g.physics_polygons,
            true,
        ));
        g.flags |= 1;
    }
    if !doorways.is_empty() {
        let clear = clear_surface(ids, out);
        let clear_slot = u16::try_from(g.surfaces.len()).map_err(|e| e.to_string())?;
        g.surfaces.push(clear);
        let mut door_polys = Vec::new();
        for (corners, room) in doorways {
            if corners.len() < 3 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let mid = corners
                .iter()
                .fold(Vec3::ZERO, |a, p| a.add(*p))
                .mul(1.0 / corners.len() as f32);
            let n = corners[1].sub(corners[0]).cross(corners[2].sub(corners[0]));
            let mut corners = corners.clone();
            if n.dot(room.sub(mid)) < 0.0 {
                corners.reverse();
            }
            let first = g.vertex_array.vertices.len();
            for p in &corners {
                g.vertex_array.vertices.push(SwVertex {
                    id: i16::try_from(g.vertex_array.vertices.len()).unwrap_or(i16::MAX),
                    position: *p,
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    uvs: Vec::new(),
                });
            }
            door_polys.push(u16::try_from(g.polygons.len()).map_err(|e| e.to_string())?);
            g.polygons.push(Polygon {
                poly_id: i16::try_from(g.polygons.len()).unwrap_or(i16::MAX),
                num_pts: u8::try_from(corners.len()).unwrap_or(u8::MAX),
                stippling: 4,
                sides_type: 0,
                pos_surface: clear_slot,
                neg_surface: u16::MAX,
                vertex_ids: (0..corners.len())
                    .map(|k| u16::try_from(first + k).unwrap_or(u16::MAX))
                    .collect(),
                pos_uv_indices: None,
                neg_uv_indices: None,
            });
        }
        let mut drawing = dereth_builder::bsp::drawing(&g.vertex_array, &g.polygons);
        for (slot, poly) in door_polys.into_iter().enumerate() {
            portal_node(&mut drawing, poly, i16::try_from(slot).unwrap_or(i16::MAX))?;
        }
        g.drawing_bsp = Some(drawing);
    }
    let id = ids.next(0x01);
    g.id = id;
    g.did_degrade = None;
    g.flags &= !8;
    out.add(
        id,
        g.encode_for(EraId::Eor)
            .map_err(|e| format!("{id:?}: {e}"))?,
    );
    Ok(Some(id))
}

/// AC2's cell mesh `mesh` as an AC1 environment of one cell shape: its id, the surfaces its cell
/// draws with, and the polygon of the shape each of AC2's portals is; `None` for a mesh that
/// will not read.
#[allow(clippy::too_many_arguments)]
pub(crate) fn environment(
    wr: &WorldRecords,
    ac2: &Ac2,
    mesh: u32,
    pictures: &mut Pictures,
    ids: &mut Ids,
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
    for f in best.map_or(&[][..], |b| m.levels[b].fragments.as_slice()) {
        let Some(&mi) = m.materials.get(f.material as usize) else {
            continue;
        };
        let (surface, _, _) =
            statics::surface_of(ac2, mi.0, m.kind, None, pictures, ids, out, made)?;
        let Some((surface, both)) = surface else {
            continue;
        };
        let s = surfaces
            .iter()
            .position(|x| *x == surface)
            .unwrap_or_else(|| {
                surfaces.push(surface);
                surfaces.len() - 1
            });
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
    let clear_slot = u16::try_from(surfaces.len()).map_err(|e| e.to_string())?;
    surfaces.push(clear_surface(ids, out));
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
    // of AC2's carved rooms takes the tree builder minutes), cut at its doorways so it ends where
    // its neighbors begin.
    let cell_bsp = cell_tree(&va, &portal_points);
    let shape: CellStruct =
        dereth_builder::bsp::cell_struct(va, polygons, portal_polygons.clone(), physics, cell_bsp)?;
    let id = ids.next(0x0D);
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
fn box_tree(va: &VertexArray) -> dereth_assets::BspTree {
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

/// `b` added to `info`'s block: its rooms numbered after `info`'s cells, its entry added; its
/// rooms returned as records.
#[must_use]
pub fn add(info: &mut LandblockInfo, b: &Building) -> Vec<(DataId, Vec<u8>)> {
    let first = 0x0100 + info.num_cells;
    let n = |k: u32| first + k;
    let mut entry = b.info.clone();
    for d in &mut entry.portals {
        d.other_cell_id = u16::try_from(n(u32::from(d.other_cell_id))).unwrap_or(u16::MAX);
        d.stab_list = d
            .stab_list
            .iter()
            .map(|&k| u16::try_from(n(u32::from(k))).unwrap_or(u16::MAX))
            .collect();
    }
    info.buildings.push(entry);
    add_cells(info, &b.rooms)
}

/// `cells` (numbered from 0 among themselves) added to the block's cells after those it has:
/// their ids, links and lists renumbered, each with its record.
pub fn add_cells(info: &mut LandblockInfo, cells: &[EnvCell]) -> Vec<(DataId, Vec<u8>)> {
    let block = info.id.raw() & 0xFFFF_0000;
    let first = 0x0100 + info.num_cells;
    let n = |k: u32| first + k;
    info.num_cells += u32::try_from(cells.len()).unwrap_or(0);
    cells
        .iter()
        .map(|c| {
            let id = block | n(c.id.raw());
            let mut c = c.clone();
            c.id = DataId(id);
            c.cell_id_repeat = id;
            for p in &mut c.portals {
                if p.flags & 4 == 0 {
                    p.other_cell_id = n(p.other_cell_id);
                }
            }
            c.visible_cells = c
                .visible_cells
                .iter()
                .map(|&k| u16::try_from(n(u32::from(k))).unwrap_or(u16::MAX))
                .collect();
            (c.id, c.encode())
        })
        .collect()
}

/// `v` turned by `q`.
fn turn(q: Quat, v: Vec3) -> Vec3 {
    let (w, x, y, z) = (q.w, q.x, q.y, q.z);
    let t = Vec3::new(
        2.0 * (y * v.z - z * v.y),
        2.0 * (z * v.x - x * v.z),
        2.0 * (x * v.y - y * v.x),
    );
    Vec3::new(
        v.x + w * t.x + (y * t.z - z * t.y),
        v.y + w * t.y + (z * t.x - x * t.z),
        v.z + w * t.z + (x * t.y - y * t.x),
    )
}

/// The conjugate (the inverse turn) of `q`.
fn back(q: Quat) -> Quat {
    Quat {
        w: q.w,
        x: -q.x,
        y: -q.y,
        z: -q.z,
    }
}
/// A wholly clear surface, as a doorway's polygon draws with.
fn clear_surface(ids: &mut Ids, out: &mut Records) -> DataId {
    let id = ids.next(0x08);
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

/// The drawing tree's node that lists `poly` made a portal node: the polygon taken out of the
/// ones it draws and named as the building's portal `slot`, with a leaf on each side, as retail's
/// building shells have their doorways.
fn portal_node(drawing: &mut BspTree, poly: u16, slot: i16) -> Result<(), String> {
    let poly = i16::try_from(poly).map_err(|e| e.to_string())?;
    let i = drawing
        .nodes
        .iter()
        .position(|n| n.in_polys.contains(&poly))
        .ok_or("the doorway is in no node of the shell's drawing tree")?;
    drawing.nodes[i].in_polys.retain(|p| *p != poly);
    drawing.nodes[i].tag = 0x504F_5254;
    drawing.nodes[i].in_portals.push((poly, slot));
    for positive in [true, false] {
        let child = if positive {
            drawing.nodes[i].pos_child
        } else {
            drawing.nodes[i].neg_child
        };
        if child.is_none() {
            let index = u32::try_from(drawing.nodes.len()).map_err(|e| e.to_string())?;
            drawing.nodes.push(BspNode {
                tag: 0x4C45_4146,
                leaf_index: Some(0),
                ..BspNode::default()
            });
            if positive {
                drawing.nodes[i].pos_child = Some(index);
            } else {
                drawing.nodes[i].neg_child = Some(index);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's buildings with rooms)
    use super::*;
    use dereth_assets::Setup;
    use statics::Collision;

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

    /// A vault entrance's plinth: a step 7 m out from its origin, which stands 4 m inside its
    /// 24 m land cell, so the step reaches 3 m into the cell beside it.
    fn plinth_shell(collision: Option<Collision>) -> Shell {
        Shell {
            ac2: (0x62, 0x58, 7),
            block: (0x62, 0x58),
            frame: Frame::new(
                Vec3::new(76.0, 123.0, 20.5),
                Quat {
                    w: std::f32::consts::FRAC_1_SQRT_2,
                    x: 0.0,
                    y: 0.0,
                    z: std::f32::consts::FRAC_1_SQRT_2,
                },
            ),
            ac2_at: (0.0, 0.0, 0.0),
            scale: [1000, 1000, 1000],
            parts: Vec::new(),
            collision,
        }
    }

    #[test]
    fn a_vault_entrances_collision_stands_again_as_a_static_met_from_every_cell_it_reaches() {
        let step = Collision {
            points: vec![
                Vec3::new(-7.0, -7.0, 0.5),
                Vec3::new(7.0, -7.0, 0.5),
                Vec3::new(7.0, -1.5, 0.5),
                Vec3::new(-7.0, -1.5, 0.5),
            ],
            rings: vec![vec![0, 1, 2, 3]],
        };
        let shell = plinth_shell(Some(step));
        let (mut ids, mut out) = (Ids::default(), Records::default());
        let solid = shell_solid(&shell, &mut ids, &mut out)
            .unwrap()
            .expect("a static");
        // Where the shell stands, turned as it is.
        assert_eq!(solid.frame, shell.frame);
        // A setup of one part, drawing nothing and colliding with the shell's polygons.
        let setup = Setup::decode_payload(solid.id, &out.0[&solid.id]).unwrap();
        assert_eq!(setup.parts.len(), 1);
        let part = setup.parts[0];
        let g = GfxObj::decode_payload(part, &out.0[&part]).unwrap();
        assert!(g.polygons.is_empty(), "draws nothing");
        assert_eq!(g.physics_polygons.len(), 1);
        // No collision, no static.
        assert!(shell_solid(&plinth_shell(None), &mut ids, &mut out)
            .unwrap()
            .is_none());
    }
}
