//! Decorating a dungeon: what is placed in a room snaps to it (a door into the room's doorway
//! nearest it, standing in it and facing through it; anything else on the floor, or against the
//! wall it was set on and facing out of it), and the selected statics and spawns copied beside
//! themselves, the copies selected.

use std::collections::BTreeSet;

use dereth_primitives::frame::V3;
use dereth_primitives::{Frame, Vec3};
use lyceum_foundry::edit::Op;
use lyceum_foundry::source::{Height, Place, Rotation};

use super::{EditorApp, Selection};

/// How far from a doorway a door set in a room still snaps into it, in meters.
pub const DOOR_REACH: f32 = 4.0;

/// How far beside itself a copy is set, in meters.
pub const BESIDE: f32 = 1.0;

/// The heading, degrees clockwise from north, that faces the level direction `d`.
fn heading_toward(d: Vec3) -> f32 {
    dereth_primitives::num::math::atan2f(d.x, d.y)
        .to_degrees()
        .rem_euclid(360.0)
}

impl EditorApp {
    /// Where something set at `at` (landblock meters), on the wall or floor of room `cell` of
    /// landblock `lb` that faces into it along `facing`, stands, and its heading: a door
    /// (`door`) in the room's doorway nearest it, on its sill, facing through it; anything else
    /// on the floor, or against a wall, facing out of it; a spawn (`standing`) always on the floor
    /// under it.
    pub(super) fn snapped(
        &mut self,
        lb: u16,
        cell: u16,
        at: Vec3,
        facing: Vec3,
        door: bool,
        standing: bool,
    ) -> (Vec3, f32) {
        if door {
            if let Some((p, h)) = self.doorway_near(lb, cell, at) {
                return (p, h);
            }
        }
        if facing.z > 0.7 {
            return (at, 0.0);
        }
        let wall = facing.z.abs() < 0.3;
        if wall && !standing {
            return (at, heading_toward(facing));
        }
        // On the floor under it, a little in from the wall.
        let inside = at.add(facing.mul(0.4));
        let floor = self
            .session
            .cell_at(lb, inside)
            .filter(|(c, _)| *c == cell)
            .map_or(at.z, |(_, z)| z);
        let h = if wall { heading_toward(facing) } else { 0.0 };
        (Vec3::new(inside.x, inside.y, floor), h)
    }

    /// The level doorway of room `cell` of landblock `lb` nearest `at` within [`DOOR_REACH`]: the
    /// middle of its sill, and the heading facing out through it.
    fn doorway_near(&mut self, lb: u16, cell: u16, at: Vec3) -> Option<(Vec3, f32)> {
        let c = self.cells_of(lb).get(&cell)?.clone();
        let env = self.environment(c.environment)?.clone();
        let slots = env.cells.get(usize::from(c.cell_struct))?.portals.len();
        (0..slots)
            .filter_map(|s| lyceum_foundry::pieces::opening(&env, c.cell_struct, s, &c.frame))
            .filter(|o| o.out.z.abs() < 0.5)
            .map(|o| {
                let sill = o.corners.iter().map(|p| p.z).fold(f32::MAX, f32::min);
                let p = Vec3::new(o.middle.x, o.middle.y, sill);
                let d = p.sub(at);
                (d.dot(d).sqrt(), p, heading_toward(o.out))
            })
            .filter(|(d, ..)| *d <= DOOR_REACH)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p, h)| (p, h))
    }

    /// Copy every selected static and spawn beside itself (a meter along the camera's right), as
    /// one step; the copies selected after.
    ///
    /// # Errors
    /// Nothing that can be copied is selected, or the pack would not take it.
    pub fn duplicate_selected(&mut self) -> Result<(), String> {
        let all: Vec<Selection> = self.selection.iter().chain(&self.also).cloned().collect();
        let r = self.camera.right();
        let by = Vec3::new(r.x * BESIDE, r.y * BESIDE, 0.0);
        let quat = |f: &Frame| {
            let q = f.rotation;
            Rotation::Quat([q.w, q.x, q.y, q.z])
        };
        let mut ops = Vec::new();
        let mut blocks: BTreeSet<u16> = BTreeSet::new();
        let mut rooms: BTreeSet<(u16, u16)> = BTreeSet::new();
        for s in &all {
            match s {
                Selection::Static { landblock, index } => {
                    let Some((model, f)) = self.static_frame(*landblock, *index) else {
                        continue;
                    };
                    let to = f.origin.add(by);
                    if !(0.0..192.0).contains(&to.x) || !(0.0..192.0).contains(&to.y) {
                        continue;
                    }
                    blocks.insert(*landblock);
                    ops.push(Op::PlaceStatic {
                        landblock: *landblock,
                        model,
                        at: Place {
                            x: to.x,
                            y: to.y,
                            z: Height::At(to.z),
                        },
                        rotation: quat(&f),
                        name: None,
                    });
                }
                Selection::CellStatic {
                    landblock,
                    cell,
                    index,
                } => {
                    let Some((model, f)) = self.cell_static_frame(*landblock, *cell, *index) else {
                        continue;
                    };
                    let to = f.origin.add(by);
                    rooms.insert((*landblock, *cell));
                    ops.push(match self.session.cell_names(*landblock).remove(cell) {
                        Some(name) => Op::PlaceCellStatic {
                            landblock: *landblock,
                            cell: name,
                            model,
                            at: [to.x, to.y, to.z],
                            rotation: quat(&f),
                        },
                        None => Op::AddCellStatic {
                            landblock: *landblock,
                            cell: *cell,
                            model,
                            at: [to.x, to.y, to.z],
                            rotation: quat(&f),
                        },
                    });
                }
                Selection::Spawn { file, guid } => {
                    let Some(sp) = self
                        .session
                        .spawns()
                        .into_iter()
                        .find(|x| x.file == *file && x.guid == *guid)
                    else {
                        continue;
                    };
                    let Some(weenie) = self.session.spawn_weenie(&sp) else {
                        continue;
                    };
                    let heading = crate::gizmo::heading_of(sp.frame.rotation);
                    let origin = sp.frame.origin.add(by);
                    let cell = u16::try_from(sp.cell & 0xFFFF).unwrap_or(0);
                    ops.push(if cell >= lyceum_foundry::cells::FIRST_CELL {
                        Op::PlaceSpawnInCell {
                            landblock: sp.landblock(),
                            cell,
                            weenie,
                            origin,
                            heading,
                        }
                    } else {
                        Op::PlaceSpawn {
                            landblock: sp.landblock(),
                            weenie,
                            origin,
                            heading,
                        }
                    });
                }
                _ => {}
            }
        }
        if ops.is_empty() {
            return Err(self.say("messages.nothing_to_copy"));
        }
        // What stands now, to tell the copies from.
        let statics_before: Vec<(u16, usize)> = blocks
            .iter()
            .map(|lb| (*lb, self.session.origins(*lb).len()))
            .collect();
        let rooms_before: Vec<(u16, u16, usize)> = rooms
            .iter()
            .map(|(lb, c)| {
                (
                    *lb,
                    *c,
                    self.cells_of(*lb)
                        .get(c)
                        .map_or(0, |x| x.static_objects.len()),
                )
            })
            .collect();
        let spawns_before: BTreeSet<(std::path::PathBuf, String)> = self
            .session
            .spawns()
            .into_iter()
            .map(|s| (s.file, s.guid))
            .collect();
        let n = ops.len();
        self.apply(&Op::Batch(ops), false)?;
        let mut copies: Vec<Selection> = Vec::new();
        for (lb, before) in statics_before {
            let now = self.session.origins(lb).len();
            copies.extend((before..now).map(|index| Selection::Static {
                landblock: lb,
                index,
            }));
        }
        for (lb, cell, before) in rooms_before {
            let now = self
                .cells_of(lb)
                .get(&cell)
                .map_or(0, |x| x.static_objects.len());
            copies.extend((before..now).map(|index| Selection::CellStatic {
                landblock: lb,
                cell,
                index,
            }));
        }
        for s in self.session.spawns() {
            if !spawns_before.contains(&(s.file.clone(), s.guid.clone())) {
                copies.push(Selection::Spawn {
                    file: s.file,
                    guid: s.guid,
                });
            }
        }
        self.selection = copies.first().cloned();
        self.also = copies.into_iter().skip(1).collect();
        self.panels.message = self.say_with("messages.copied", &[("n", &n)]);
        Ok(())
    }
}
