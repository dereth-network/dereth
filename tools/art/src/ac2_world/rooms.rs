//! AC2's buildings with rooms of their own, made AC1 buildings: the shell, its doorways and its
//! rooms as AC1's environment cells.
//!
//! **Which.** AC2's houses are shells with nothing inside; its rooms beside a shell are few: the
//! vault entrances' halls (one room, one doorway out), a castle's passages and two structures.
//! A building here is a placed static whose
//! origin stands on a room seen from outside; its rooms are that room and every room joined to
//! it. A group of rooms that another shell also stands on is left as statics (a castle's passage
//! runs under several), as is a shell AC2 scales. A shell joined to more than eight rooms stands
//! over a dungeon: the dungeon is its interior, each room seeing the rooms AC2 has it see and lit
//! by AC2's lights in it, as a dungeon on its own is ([`super::vaults`]).
//!
//! **A room** is AC2's cell mesh as one AC1 cell shape: its most detailed level's triangles, each
//! material drawn as the statics draw theirs ([`super::steep::surface_of`]); its doorways, each the
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
//! from every cell it reaches into. AC1 likewise enters a building's rooms only from the land
//! cell holding its origin, and a shell's origin can stand a cell from its doorway, so the
//! building stands at its doorway, its model moved to match, in the block beyond its shell's
//! where the doorway stands past the block's edge. A doorway can be wider than the land cell it
//! stands in: each other land cell of its block it spans takes a stand-in ([`add_stand_ins`]), a
//! building record with a model that draws and meets nothing, whose doorways lead into the same
//! rooms, so they are entered from that cell too.

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
/// The farthest a building's origin stands into its block, meters: short of the 192 m edge.
const IN_BLOCK: f32 = 191.99;
/// A block's side and a land cell's, meters, and the last block on a side of AC1's world.
const BLOCK: f32 = 192.0;
const LAND_CELL: f32 = 24.0;
const LAST_BLOCK: usize = 0xFE;
/// The least slope of a polygon a body stands on: its normal's height at least this.
const FLOOR_Z: f32 = 0.664_174;
/// How near a shell's origin stands to its room's.
const ON_ROOM: f32 = 0.5;
/// The most rooms a building of rooms that all see each other takes: more joined to a shell's room
/// are a dungeon under its entrance, whose rooms see as AC2 has them see.
pub(crate) const MOST_ROOMS: usize = 8;

/// The shells AC2's rooms belong to: AC2's block and the entity's index there, with its rooms by
/// their numbers in that block.
#[derive(Debug, Default)]
pub struct Found {
    pub rooms: BTreeMap<(usize, usize, usize), Vec<u16>>,
    /// Groups of rooms passed over, by why.
    pub passed: BTreeMap<&'static str, usize>,
    /// The rooms of the groups passed over: each one's AC2 block, its rooms' numbers, and why.
    pub left: Vec<((usize, usize), Vec<u16>, &'static str)>,
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
                    let why = "rooms under more than one shell";
                    *found.passed.entry(why).or_default() += 1;
                    found
                        .left
                        .push(((bx, by), group.into_iter().collect(), why));
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
    /// The shell's AC2 block and entity.
    pub ac2: (usize, usize, usize),
    pub block: (usize, usize),
    pub info: BuildInfo,
    pub rooms: Vec<EnvCell>,
    /// Its first doorway's middle, and the middle of the room it leads into, landblock meters.
    pub doorway: Option<(Vec3, Vec3)>,
    /// Its doorways out to the land (those AC2 leads outside, not into a cell left out).
    pub to_land: Vec<ToLand>,
}

/// A doorway out to the land, landblock meters: its corners, the middle of its room's points, and
/// the room's floor near it (the triangles of its collision a body stands on).
#[derive(Clone, Debug, Default)]
pub struct ToLand {
    pub corners: Vec<Vec3>,
    pub inside: Vec3,
    pub floor: Vec<[Vec3; 3]>,
}

/// How near a doorway's middle a room's floor is kept with it, meters.
const NEAR_DOORWAY: f32 = 6.0;

/// The buildings `found` names, from the statics `shells` made of their shells; each shell's
/// collision is added to `solids`, its block's statics, as a collision-only static. Each AC2 cell
/// mesh a room is drawn with is made once, into `environments`.
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
    environments: &mut BTreeMap<u32, Option<Room>>,
) -> Result<Vec<Building>, String> {
    let mut made = Vec::new();
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
        // A dungeon under its entrance: its rooms see as AC2 has them see, lit by AC2's lights.
        let dungeon = rooms.len() > MOST_ROOMS;
        let moved = |o: Vec3| {
            let (wx, wy) = (
                shell.ac2.0 as f64 * 160.0 + f64::from(o.x),
                shell.ac2.1 as f64 * 160.0 + f64::from(o.y),
            );
            #[allow(clippy::cast_possible_truncation)]
            Vec3::new(
                shell.frame.origin.x + (wx - shell.ac2_at.0) as f32,
                shell.frame.origin.y + (wy - shell.ac2_at.1) as f32,
                shell.frame.origin.z + (f64::from(o.z) - shell.ac2_at.2) as f32,
            )
        };
        let mut lit = if dungeon {
            super::vaults::lights_in(wr, (x, y), &number, &moved, ids, out)?.0
        } else {
            BTreeMap::new()
        };
        let mut cells = Vec::with_capacity(rooms.len());
        let mut doorways = Vec::new();
        // Each doorway's corners in the shell's frame, and the middle of the room it leads into.
        let mut doorway_shapes: Vec<(Vec<Vec3>, Vec3)> = Vec::new();
        // The doorways out to the land, in the shell's frame: corners, and the room's points'
        // middle.
        let mut to_land: Vec<ToLand> = Vec::new();
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
            let mut land_ways = Vec::new();
            for p in &c.portals {
                let Some(&polygon_id) = portal_polygons.get(usize::from(p.portal)) else {
                    continue;
                };
                let out_of = p.flags & NO_OTHER_CELL != 0 || !number.contains_key(&p.other_cell);
                if out_of {
                    doorways.push((k, portals.len()));
                    ways_out.push(usize::from(p.portal));
                    if p.flags & NO_OTHER_CELL != 0 {
                        land_ways.push(usize::from(p.portal));
                    }
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
            let at = moved(Vec3::new(o.x, o.y, o.z));
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
            let (points, floor) = if ways_out.is_empty() {
                (Vec::new(), Vec::new())
            } else {
                wr.get::<auberean_assets::mesh::CellMesh>(c.mesh.0)
                    .map(|cm| floor_of(&cm))
                    .unwrap_or_default()
            };
            // The room's origin can stand outside it (a hall whose doorway is its edge): the
            // middle of its points is inside.
            #[allow(clippy::cast_precision_loss)]
            let middle = points
                .iter()
                .fold(Vec3::ZERO, |m, p| m.add(*p))
                .mul(1.0 / points.len().max(1) as f32);
            for &w in &ways_out {
                let corners = portal_points.get(w).cloned().unwrap_or_default();
                doorway_shapes.push((
                    corners.into_iter().map(to_shell).collect(),
                    to_shell(middle),
                ));
            }
            if !land_ways.is_empty() {
                for &w in &land_ways {
                    let corners = portal_points.get(w).cloned().unwrap_or_default();
                    #[allow(clippy::cast_precision_loss)]
                    let mid = corners
                        .iter()
                        .fold(Vec3::ZERO, |m, p| m.add(*p))
                        .mul(1.0 / corners.len().max(1) as f32);
                    let near = |p: &Vec3| {
                        (p.x - mid.x).powi(2) + (p.y - mid.y).powi(2) <= NEAR_DOORWAY * NEAR_DOORWAY
                    };
                    to_land.push(ToLand {
                        corners: corners.into_iter().map(to_shell).collect(),
                        inside: to_shell(middle),
                        floor: floor
                            .iter()
                            .filter(|t| t.iter().any(near))
                            .map(|t| t.map(to_shell))
                            .collect(),
                    });
                }
            }
            let k16 = u16::try_from(k).unwrap_or(u16::MAX);
            let statics = lit.remove(&low).unwrap_or_default();
            cells.push(EnvCell {
                id: DataId(u32::from(k16)),
                flags: c.flags & SEEN_OUTSIDE
                    | if statics.is_empty() {
                        0
                    } else {
                        super::vaults::WITH_STATICS
                    },
                cell_id_repeat: u32::from(k16),
                surfaces: cell_surfaces,
                environment: env,
                cell_struct: 0,
                frame: Frame::new(at, r),
                portals,
                // A building's room seen from outside sees every other room of its building; a
                // dungeon's sees what AC2 has it see.
                visible_cells: if dungeon {
                    c.visible
                        .iter()
                        .filter_map(|v| number.get(v).copied())
                        .collect()
                } else {
                    (0..rooms.len())
                        .filter(|&j| j != k)
                        .map(|j| u16::try_from(j).unwrap_or(u16::MAX))
                        .collect()
                },
                static_objects: statics,
                restriction_obj: None,
            });
        }
        if !whole {
            *passed
                .entry("a room whose mesh does not convert")
                .or_default() += 1;
            continue;
        }
        // AC1 meets a building's doorways only from the land cell holding its origin, and a
        // shell's origin can stand a cell or more from its doorway (a hall whose shell stands over
        // its far end): the building stands at the middle of its first doorway out to the land,
        // level with the shell, its model moved to match, so it is entered from the cell its
        // doorway is in; past its block's edge it stands in the block beyond
        // ([`into_block_of_origin`]), kept short of the edge of the world.
        let at = to_land.first().map_or(Vec3::ZERO, |d| {
            #[allow(clippy::cast_precision_loss)]
            let mid = d
                .corners
                .iter()
                .fold(Vec3::ZERO, |m, p| m.add(*p))
                .mul(1.0 / d.corners.len().max(1) as f32);
            let o = shell.frame.origin;
            let mut to = o.add(turn(shell.frame.rotation, mid));
            let keep = |v: f32, at: usize| match at {
                0 => v.max(0.0),
                LAST_BLOCK => v.min(IN_BLOCK),
                _ => v,
            };
            to.x = keep(to.x, shell.block.0);
            to.y = keep(to.y, shell.block.1);
            to.z = o.z;
            turn(back(shell.frame.rotation), to.sub(o))
        });
        let Some(model) = shell_model(shell, &doorway_shapes, at, ids, out)? else {
            *passed.entry("a shell of more than one model").or_default() += 1;
            continue;
        };
        if let Some(solid) = shell_solid(shell, ids, out)? {
            solids.entry(shell.block).or_default().push(solid);
        }
        let all: Vec<u16> = (0..rooms.len())
            .map(|j| u16::try_from(j).unwrap_or(u16::MAX))
            .collect();
        // What a doorway sees: every room, or a dungeon's room and what it sees.
        let stab = |room: usize| -> Vec<u16> {
            if !dungeon {
                return all.clone();
            }
            let mut v = vec![u16::try_from(room).unwrap_or(u16::MAX)];
            v.extend(cells[room].visible_cells.iter().copied());
            v.sort_unstable();
            v.dedup();
            v
        };
        let info = BuildInfo {
            id: model,
            frame: Frame::new(
                shell.frame.origin.add(turn(shell.frame.rotation, at)),
                shell.frame.rotation,
            ),
            num_leaves: 0,
            portals: doorways
                .iter()
                .map(|&(room, way)| BuildingPortal {
                    flags: DOORWAY_FLAGS,
                    other_cell_id: u16::try_from(room).unwrap_or(u16::MAX),
                    other_portal_id: i16::try_from(way).unwrap_or(-1),
                    stab_list: stab(room),
                })
                .collect(),
        };
        let in_block = |p: Vec3| shell.frame.origin.add(turn(shell.frame.rotation, p));
        let doorway = doorway_shapes.first().map(|(corners, room)| {
            #[allow(clippy::cast_precision_loss)]
            let n = corners.len().max(1) as f32;
            let mid = corners
                .iter()
                .fold(Vec3::ZERO, |a, c| a.add(*c))
                .mul(1.0 / n);
            (in_block(mid), in_block(*room))
        });
        let to_land = to_land
            .into_iter()
            .map(|d| ToLand {
                corners: d.corners.into_iter().map(in_block).collect(),
                inside: in_block(d.inside),
                floor: d.floor.into_iter().map(|t| t.map(in_block)).collect(),
            })
            .collect();
        let mut b = Building {
            doorway,
            to_land,
            ac2: shell.ac2,
            block: shell.block,
            info,
            rooms: cells,
        };
        into_block_of_origin(&mut b);
        made.push(b);
    }
    Ok(made)
}

/// `b` moved into the block its origin stands in, where that is past its shell's block's edge:
/// its origin, its rooms and what stands in them, and its doorways, in meters of that block.
pub(crate) fn into_block_of_origin(b: &mut Building) {
    let o = b.info.frame.origin;
    // LINT-OK: a block step of -1, 0 or 1, and block indices 0 to 254.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let step = |v: f32| (v / BLOCK).floor() as i32;
    let (dx, dy) = (step(o.x), step(o.y));
    if (dx, dy) == (0, 0) {
        return;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    {
        b.block = (
            (b.block.0 as i32 + dx) as usize,
            (b.block.1 as i32 + dy) as usize,
        );
        let by = Vec3::new(-dx as f32 * BLOCK, -dy as f32 * BLOCK, 0.0);
        b.info.frame.origin = b.info.frame.origin.add(by);
        for r in &mut b.rooms {
            r.frame.origin = r.frame.origin.add(by);
            for s in &mut r.static_objects {
                s.frame.origin = s.frame.origin.add(by);
            }
        }
        if let Some((d, i)) = &mut b.doorway {
            *d = d.add(by);
            *i = i.add(by);
        }
        for d in &mut b.to_land {
            for c in &mut d.corners {
                *c = c.add(by);
            }
            d.inside = d.inside.add(by);
            for t in &mut d.floor {
                *t = t.map(|p| p.add(by));
            }
        }
    }
}

/// The model of every stand-in: one point and nothing else, so it draws nothing, opens no view
/// into the rooms (those are opened through a model's drawing tree) and has no collision.
///
/// # Errors
/// A record that will not encode.
pub fn stand_in_model(ids: &mut Ids, out: &mut Records) -> Result<DataId, String> {
    let id = ids.next(0x01);
    let g = GfxObj {
        id,
        flags: 0,
        surfaces: Vec::new(),
        vertex_array: dereth_assets::VertexArray {
            vertex_type: 1,
            vertices: vec![SwVertex {
                id: 0,
                position: Vec3::ZERO,
                normal: Vec3::new(0.0, 0.0, 1.0),
                uvs: Vec::new(),
            }],
        },
        physics_polygons: Vec::new(),
        physics_bsp: None,
        sort_center: Vec3::ZERO,
        polygons: Vec::new(),
        drawing_bsp: None,
        did_degrade: None,
    };
    out.add(
        id,
        g.encode_for(EraId::Eor)
            .map_err(|e| format!("{id:?}: {e}"))?,
    );
    Ok(id)
}

/// The land cell of its block holding block point `(x, y)`, numbered as AC1 numbers them (1 to
/// 64, south to north in columns west to east); `None` off the block.
fn land_cell_of(x: f32, y: f32) -> Option<u16> {
    if !(0.0..BLOCK).contains(&x) || !(0.0..BLOCK).contains(&y) {
        return None;
    }
    // LINT-OK: a land cell's column and row, 0 to 7.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (i, j) = ((x / LAND_CELL) as u16, (y / LAND_CELL) as u16);
    Some(i * 8 + j + 1)
}

/// The land cells of its block a doorway's outline crosses.
fn doorway_cells(d: &ToLand) -> BTreeSet<u16> {
    const STEP: f32 = 0.25;
    let mut cells = BTreeSet::new();
    for (k, a) in d.corners.iter().enumerate() {
        let b = d.corners[(k + 1) % d.corners.len()];
        let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        // LINT-OK: a few meters in quarter-meter steps.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (len / STEP).ceil().max(1.0) as usize;
        for s in 0..=n {
            #[allow(clippy::cast_precision_loss)]
            let t = s as f32 / n as f32;
            cells.extend(land_cell_of(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    cells
}

/// What [`add_stand_ins`] added: the stand-ins, and the doorways' cells it left to a building
/// standing there already (AC1 keeps one building a land cell, the first).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StandIns {
    pub added: usize,
    pub held: usize,
}

/// A stand-in in each land cell of `info`'s block that a doorway out to the land of one of the
/// buildings `added` spans (each with its index among `info`'s buildings), but for the cell
/// holding the building's origin and a cell holding another building: a building record with the
/// model `model` ([`stand_in_model`]), standing in that cell at its nearest point to the doorway,
/// whose doorways lead into the building's rooms. Added after every building of the block, so
/// that none of them loses its cell to one.
pub fn add_stand_ins(
    info: &mut LandblockInfo,
    added: &[(usize, &Building)],
    model: DataId,
) -> StandIns {
    let mut taken: BTreeSet<u16> = info
        .buildings
        .iter()
        .filter_map(|b| land_cell_of(b.frame.origin.x, b.frame.origin.y))
        .collect();
    let mut made = StandIns::default();
    for &(index, b) in added {
        let Some(entry) = info.buildings.get(index).cloned() else {
            continue;
        };
        // The cells this building stands in or has a stand-in in, and those it found taken.
        let mut mine: BTreeSet<u16> = land_cell_of(entry.frame.origin.x, entry.frame.origin.y)
            .into_iter()
            .collect();
        for d in &b.to_land {
            #[allow(clippy::cast_precision_loss)]
            let mid = d
                .corners
                .iter()
                .fold(Vec3::ZERO, |m, p| m.add(*p))
                .mul(1.0 / d.corners.len().max(1) as f32);
            for cell in doorway_cells(d) {
                if !mine.insert(cell) {
                    continue;
                }
                if !taken.insert(cell) {
                    made.held += 1;
                    continue;
                }
                let (i, j) = (f32::from((cell - 1) / 8), f32::from((cell - 1) % 8));
                let inset =
                    |v: f32, k: f32| v.clamp(k * LAND_CELL + 0.01, (k + 1.0) * LAND_CELL - 0.01);
                info.buildings.push(BuildInfo {
                    id: model,
                    frame: Frame::new(
                        Vec3::new(inset(mid.x, i), inset(mid.y, j), entry.frame.origin.z),
                        Quat::IDENTITY,
                    ),
                    num_leaves: 0,
                    portals: entry
                        .portals
                        .iter()
                        .map(|p| BuildingPortal {
                            stab_list: Vec::new(),
                            ..p.clone()
                        })
                        .collect(),
                });
                made.added += 1;
            }
        }
    }
    made
}

/// A room's points, and its floor: the polygons of its collision facing up or down no steeper
/// than a body stands on, in triangles; in the room's frame.
pub(crate) fn floor_of(cm: &auberean_assets::mesh::CellMesh) -> (Vec<Vec3>, Vec<[Vec3; 3]>) {
    let points: Vec<Vec3> = cm
        .mesh
        .vertices
        .attributes()
        .map(|a| {
            a.iter()
                .map(|v| Vec3::new(v.position.x, v.position.y, v.position.z))
                .collect()
        })
        .unwrap_or_default();
    let doors: BTreeSet<u16> = cm.portals.iter().map(|p| p.polygon).collect();
    let mut floor: Vec<[Vec3; 3]> = Vec::new();
    for (n, poly) in cm.mesh.physics_polygons.iter().enumerate() {
        if doors.contains(&u16::try_from(n).unwrap_or(u16::MAX)) {
            continue;
        }
        let ring: Vec<Vec3> = poly
            .vertices
            .iter()
            .filter_map(|&i| points.get(usize::from(i)).copied())
            .collect();
        if ring.len() < 3 {
            continue;
        }
        let normal = ring[1].sub(ring[0]).cross(ring[2].sub(ring[0]));
        let len = normal.dot(normal).sqrt();
        if len < 1e-6 || normal.z.abs() < FLOOR_Z * len {
            continue;
        }
        floor.extend((1..ring.len() - 1).map(|k| [ring[0], ring[k], ring[k + 1]]));
    }
    (points, floor)
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
/// middle of the room each leads into) as clear polygons facing out, the portal nodes of its
/// drawing tree, through which its rooms are drawn from outside; every point of it moved by
/// `-at`, for a building standing at `at` in the shell's frame: `None` for a shell of more than
/// one model.
fn shell_model(
    shell: &Shell,
    doorways: &[(Vec<Vec3>, Vec3)],
    at: Vec3,
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
    let moved = at != Vec3::ZERO;
    for v in &mut g.vertex_array.vertices {
        v.position = v.position.sub(at);
    }
    g.sort_center = g.sort_center.sub(at);
    if let Some(c) = &shell.collision {
        let first = g.vertex_array.vertices.len();
        for (k, p) in c.points.iter().enumerate() {
            g.vertex_array.vertices.push(SwVertex {
                id: i16::try_from(first + k).unwrap_or(i16::MAX),
                position: p.sub(at),
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
    } else if moved && g.physics_bsp.is_some() {
        g.physics_bsp = Some(dereth_builder::bsp::physics(
            &g.vertex_array,
            &g.physics_polygons,
            true,
        ));
    }
    if doorways.is_empty() && moved && g.drawing_bsp.is_some() {
        g.drawing_bsp = Some(dereth_builder::bsp::drawing(&g.vertex_array, &g.polygons));
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
            // It faces out of its room, as retail's doorways do: a building's doorway opens to a
            // viewer on its front, and its flags put the front outside.
            let n = (1..corners.len() - 1).fold(Vec3::ZERO, |n, k| {
                n.add(
                    corners[k]
                        .sub(corners[0])
                        .cross(corners[k + 1].sub(corners[0])),
                )
            });
            let mut corners = corners.clone();
            if n.dot(room.sub(mid)) > 0.0 {
                corners.reverse();
            }
            let first = g.vertex_array.vertices.len();
            for p in &corners {
                g.vertex_array.vertices.push(SwVertex {
                    id: i16::try_from(g.vertex_array.vertices.len()).unwrap_or(i16::MAX),
                    position: p.sub(at),
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
pub(crate) fn turn(q: Quat, v: Vec3) -> Vec3 {
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

    #[test]
    fn a_building_stood_at_its_doorway_keeps_its_model_and_doorway_where_they_were() {
        // A part of one triangle, drawn.
        let va = dereth_assets::VertexArray {
            vertex_type: 1,
            vertices: [(0.0, 0.0, 0.0), (4.0, 0.0, 0.0), (0.0, 4.0, 3.0)]
                .iter()
                .enumerate()
                .map(|(k, &(x, y, z))| SwVertex {
                    id: i16::try_from(k).unwrap(),
                    position: Vec3::new(x, y, z),
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    uvs: vec![(0.0, 0.0)],
                })
                .collect(),
        };
        let triangle = Polygon {
            poly_id: 0,
            num_pts: 3,
            stippling: 0,
            sides_type: 0,
            pos_surface: 0,
            neg_surface: u16::MAX,
            vertex_ids: vec![0, 1, 2],
            pos_uv_indices: Some(vec![0; 3]),
            neg_uv_indices: None,
        };
        let part = GfxObj {
            id: DataId(0x0100_0001),
            flags: 2,
            surfaces: vec![DataId(0x0800_0001)],
            drawing_bsp: Some(dereth_builder::bsp::drawing(
                &va,
                std::slice::from_ref(&triangle),
            )),
            vertex_array: va,
            physics_polygons: Vec::new(),
            physics_bsp: None,
            sort_center: Vec3::new(1.0, 1.0, 1.0),
            polygons: vec![triangle],
            did_degrade: None,
        };
        let (mut ids, mut out) = (Ids::default(), Records::default());
        out.add(part.id, part.encode_for(EraId::Eor).unwrap());
        let mut shell = plinth_shell(Some(Collision {
            points: vec![
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
            ],
            rings: vec![vec![0, 1, 2]],
        }));
        shell.parts = vec![part.id];
        // A doorway 6 m east of the shell's origin, facing the room west of it.
        let doorway = vec![
            Vec3::new(6.0, -2.0, 0.0),
            Vec3::new(6.0, 2.0, 0.0),
            Vec3::new(6.0, 2.0, 3.0),
            Vec3::new(6.0, -2.0, 3.0),
        ];
        let at = Vec3::new(6.0, 0.0, 0.0);
        let id = shell_model(&shell, &[(doorway, Vec3::ZERO)], at, &mut ids, &mut out)
            .unwrap()
            .expect("a model");
        let g = GfxObj::decode_payload(id, &out.0[&id]).unwrap();
        let p = |k: usize| g.vertex_array.vertices[k].position;
        // Every point moved by the building's place: the part's, the collision's, the doorway's.
        assert_eq!(p(1), Vec3::new(-2.0, 0.0, 0.0));
        assert_eq!(p(3), Vec3::new(-7.0, -1.0, 0.0));
        assert_eq!(g.sort_center, Vec3::new(-5.0, 1.0, 1.0));
        let door = g.polygons.last().expect("the doorway");
        for &k in &door.vertex_ids {
            assert!(
                p(usize::from(k)).x.abs() < 1e-6,
                "the doorway at the origin"
            );
        }
        // It faces out of its room, east, as retail's doorways do: its flags open it to a viewer
        // on its front, so the room is drawn for one standing outside.
        let c: Vec<Vec3> = door.vertex_ids.iter().map(|&k| p(usize::from(k))).collect();
        let n = c[1].sub(c[0]).cross(c[2].sub(c[0]));
        assert!(n.x > 0.0 && n.y.abs() < 1e-6 && n.z.abs() < 1e-6, "{n:?}");
        assert_eq!((!DOORWAY_FLAGS >> 1) & 1, 0, "the front opens it");
        // Nothing moved for a building standing where its shell does.
        let id = shell_model(&shell, &[], Vec3::ZERO, &mut ids, &mut out)
            .unwrap()
            .expect("a model");
        let g = GfxObj::decode_payload(id, &out.0[&id]).unwrap();
        assert_eq!(
            g.vertex_array.vertices[1].position,
            Vec3::new(4.0, 0.0, 0.0)
        );
        assert_eq!(g.sort_center, Vec3::new(1.0, 1.0, 1.0));
    }

    /// A building at 66A9's doorway: its origin at the doorway's middle (47, 48), which stands
    /// on the edge between land cells 11 (x 24 to 48, y 48 to 72) and 10 (y 24 to 48), the
    /// doorway 7.8 m wide across it, facing west into a room; with two rooms.
    fn across_a_cell_edge(info: &mut LandblockInfo) -> Building {
        let at = Vec3::new(47.0, 48.0, 29.0);
        let b = Building {
            ac2: (0x66, 0xA9, 6),
            block: (0x55, 0x8D),
            info: BuildInfo {
                id: DataId(0x0100_0001),
                frame: Frame::new(at, Quat::IDENTITY),
                num_leaves: 0,
                portals: vec![BuildingPortal {
                    flags: DOORWAY_FLAGS,
                    other_cell_id: 0,
                    other_portal_id: 0,
                    stab_list: vec![0, 1],
                }],
            },
            rooms: Vec::new(),
            doorway: None,
            to_land: vec![ToLand {
                corners: vec![
                    Vec3::new(47.0, 44.1, 29.0),
                    Vec3::new(47.0, 51.9, 29.0),
                    Vec3::new(47.0, 51.9, 34.0),
                    Vec3::new(47.0, 44.1, 34.0),
                ],
                inside: Vec3::new(40.0, 48.0, 29.0),
                floor: Vec::new(),
            }],
        };
        let mut entry = b.info.clone();
        entry.portals[0].other_cell_id = 0x0100;
        entry.portals[0].stab_list = vec![0x0100, 0x0101];
        info.buildings.push(entry);
        b
    }

    fn info() -> LandblockInfo {
        crate::ac2_world::cleared::empty(DataId(0x558D_FFFE))
    }

    #[test]
    fn a_doorway_across_a_land_cell_edge_has_a_stand_in_in_the_other_cell() {
        let mut info = info();
        let b = across_a_cell_edge(&mut info);
        let model = DataId(0x0100_0002);
        let made = add_stand_ins(&mut info, &[(0, &b)], model);
        assert_eq!(made, StandIns { added: 1, held: 0 });
        // The building first, as it was: AC1 keeps the first building a land cell holds.
        assert_eq!(info.buildings.len(), 2);
        assert_eq!(info.buildings[0].id, DataId(0x0100_0001));
        let s = &info.buildings[1];
        assert_eq!(s.id, model);
        // In the cell south of the edge, at its nearest point to the doorway, level with it.
        let o = s.frame.origin;
        assert_eq!(land_cell_of(o.x, o.y), Some(10));
        assert_eq!(land_cell_of(47.0, 48.0), Some(11));
        assert!((o.x - 47.0).abs() < 1e-4 && (o.y - 47.99).abs() < 1e-4 && o.z == 29.0);
        // Leading into the same rooms, opening no view of its own.
        assert_eq!(s.portals.len(), 1);
        assert_eq!(s.portals[0].other_cell_id, 0x0100);
        assert_eq!(s.portals[0].flags, DOORWAY_FLAGS);
        assert!(s.portals[0].stab_list.is_empty());
        // The block's record keeps both.
        let back = LandblockInfo::decode_payload(info.id, &info.encode()).unwrap();
        assert_eq!(back.buildings, info.buildings);
    }

    #[test]
    fn a_land_cell_another_building_holds_takes_no_stand_in() {
        let mut info = info();
        info.buildings.push(BuildInfo {
            id: DataId(0x0100_0009),
            frame: Frame::new(Vec3::new(30.0, 30.0, 20.0), Quat::IDENTITY),
            num_leaves: 0,
            portals: Vec::new(),
        });
        let b = across_a_cell_edge(&mut info);
        let made = add_stand_ins(&mut info, &[(1, &b)], DataId(0x0100_0002));
        assert_eq!(made, StandIns { added: 0, held: 1 });
        assert_eq!(info.buildings.len(), 2);
    }

    #[test]
    fn a_doorway_within_its_land_cell_takes_no_stand_in() {
        let mut info = info();
        let mut b = across_a_cell_edge(&mut info);
        for c in &mut b.to_land[0].corners {
            c.y += 12.0;
        }
        let made = add_stand_ins(&mut info, &[(0, &b)], DataId(0x0100_0002));
        assert_eq!(made, StandIns::default());
        assert_eq!(info.buildings.len(), 1);
    }

    /// What the client's cell search reads of a block's buildings: the rooms a body in each land
    /// cell can enter.
    fn rooms_entered_from(info: &LandblockInfo, cell: u16) -> Vec<u32> {
        use dereth_physics::LandSource as _;
        let table = [0.0; dereth_physics::globals::LAND_HEIGHT_TABLE_LEN];
        let mut memo = dereth_world_data::env_cells::CellBspMemo::new();
        let block = dereth_primitives::LandblockId::new(0x55, 0x8D);
        let source = dereth_world_data::env_cells::BlockCellSource::new(
            block,
            None,
            &table,
            (&[], &info.buildings),
            &mut memo,
        );
        source
            .building_cells(dereth_primitives::CellId(0x558D_0000 | u32::from(cell)))
            .into_iter()
            .map(|c| c.0)
            .collect()
    }

    #[test]
    fn the_rooms_are_entered_from_both_cells_of_the_doorway_and_no_others() {
        let mut info = info();
        let b = across_a_cell_edge(&mut info);
        assert_eq!(rooms_entered_from(&info, 11), vec![0x558D_0100]);
        assert!(rooms_entered_from(&info, 10).is_empty(), "before");
        add_stand_ins(&mut info, &[(0, &b)], DataId(0x0100_0002));
        assert_eq!(rooms_entered_from(&info, 11), vec![0x558D_0100]);
        assert_eq!(rooms_entered_from(&info, 10), vec![0x558D_0100]);
        for cell in (1..=64).filter(|c| ![10, 11].contains(c)) {
            assert!(rooms_entered_from(&info, cell).is_empty(), "cell {cell}");
        }
    }

    #[test]
    fn the_stand_ins_model_draws_nothing_opens_nothing_and_meets_nothing() {
        let (mut ids, mut out) = (Ids::default(), Records::default());
        let id = stand_in_model(&mut ids, &mut out).unwrap();
        assert_eq!(
            id.0 >> 24,
            0x01,
            "a graphics object, as a building's model is"
        );
        let g = GfxObj::decode_payload(id, &out.0[&id]).unwrap();
        // No polygon drawn; no drawing tree, whose portal nodes are what open a building's rooms
        // to view from outside; no collision.
        assert!(g.polygons.is_empty() && g.drawing_bsp.is_none());
        assert!(g.physics_polygons.is_empty() && g.physics_bsp.is_none());
        // One point, so its bounds are a point, not an empty box.
        assert_eq!(g.vertex_array.vertices.len(), 1);
        assert_eq!(g.vertex_array.vertices[0].position, Vec3::ZERO);
    }

    #[test]
    fn a_building_whose_doorway_is_past_its_block_stands_in_the_block_beyond() {
        let mut info = info();
        let mut b = across_a_cell_edge(&mut info);
        b.block = (0x7C, 0x83);
        b.info.frame.origin = Vec3::new(122.0, 193.0, 146.0);
        b.rooms = vec![EnvCell {
            id: DataId(0),
            flags: 0,
            cell_id_repeat: 0,
            surfaces: Vec::new(),
            environment: DataId(0x0D00_0001),
            cell_struct: 0,
            frame: Frame::new(Vec3::new(122.0, 186.0, 146.0), Quat::IDENTITY),
            portals: Vec::new(),
            visible_cells: Vec::new(),
            static_objects: vec![ObjectEntry {
                id: DataId(0x0200_0001),
                frame: Frame::new(Vec3::new(121.0, 180.0, 147.0), Quat::IDENTITY),
            }],
            restriction_obj: None,
        }];
        b.doorway = Some((
            Vec3::new(122.0, 193.0, 148.0),
            Vec3::new(122.0, 186.0, 146.0),
        ));
        b.to_land[0].corners = vec![
            Vec3::new(118.1, 193.0, 146.0),
            Vec3::new(125.9, 193.0, 151.0),
        ];
        b.to_land[0].inside = Vec3::new(122.0, 186.0, 146.0);
        b.to_land[0].floor = vec![[Vec3::new(120.0, 190.0, 146.0); 3]];
        into_block_of_origin(&mut b);
        assert_eq!(b.block, (0x7C, 0x84));
        // Every place of it in the block north's meters: 192 m less north.
        assert_eq!(b.info.frame.origin, Vec3::new(122.0, 1.0, 146.0));
        assert_eq!(b.rooms[0].frame.origin, Vec3::new(122.0, -6.0, 146.0));
        assert_eq!(
            b.rooms[0].static_objects[0].frame.origin,
            Vec3::new(121.0, -12.0, 147.0)
        );
        assert_eq!(
            b.doorway,
            Some((Vec3::new(122.0, 1.0, 148.0), Vec3::new(122.0, -6.0, 146.0)))
        );
        assert_eq!(b.to_land[0].corners[0], Vec3::new(118.1, 1.0, 146.0));
        assert_eq!(b.to_land[0].inside, Vec3::new(122.0, -6.0, 146.0));
        assert_eq!(b.to_land[0].floor[0][0], Vec3::new(120.0, -2.0, 146.0));
        // One within its block stays.
        let mut c = across_a_cell_edge(&mut info);
        let was = c.info.frame;
        into_block_of_origin(&mut c);
        assert_eq!((c.block, c.info.frame), ((0x55, 0x8D), was));
    }
}
