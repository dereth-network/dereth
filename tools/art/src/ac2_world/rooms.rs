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
use auberean_world::place::DRAWN_KINDS;
use auberean_world::records::WorldRecords;
use dereth_assets::world::{BuildInfo, BuildingPortal, CellPortal, LandblockInfo, ObjectEntry};
use dereth_assets::{BspNode, BspTree, Decode, EnvCell, GfxObj, Polygon, SwVertex};
use dereth_primitives::frame::V3;
use dereth_primitives::{DataId, EraId, Frame, Quat, Vec3};

use crate::ac2_world::art::{Ids, Pictures, Records};
use crate::ac2_world::statics::{self, Shell};
pub use dereth_ac2_convert::cell::{clear_surface, environment, Room};

/// A cell flag AC2 sets on a room seen from outside, and a portal flag AC2 sets on a way out.
const SEEN_OUTSIDE: u32 = 1;
const NO_OTHER_CELL: u16 = 2;
/// AC1's flags on a room's way out (an exact match, to the outdoors), and on a building's
/// doorway, as retail's cottages have them.
const OUT_FLAGS: u16 = 1 | 4;
const DOORWAY_FLAGS: u16 = 3;
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
