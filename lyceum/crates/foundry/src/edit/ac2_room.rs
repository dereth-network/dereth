//! A room of an AC2 kit's tiles laid in one step: a rectangle of its grid's squares, a floor tile
//! on each and a ceiling (or roof) tile over it, each turned to open on the squares beside it; where a free
//! doorway of the dungeon meets the room's side, the kit's tile with a doorway stands there. A kit
//! whose halls reach its rooms through a room entrance (the tomb) gets one: a room laid against a
//! hall stands a square back, the entrance between, its first doorway to the room as in every one of
//! AC2's own tombs.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::world::EnvCell;
use dereth_assets::Environment;
use dereth_primitives::{Frame, Vec3};

use super::{NewPiece, Op, Session};
use crate::ac2_kit::Kit;
use crate::error::Error;
use crate::kit::Side;
use crate::room::{Shape, Solid};
use crate::source::Rotation;

/// A room to lay: the landblock, the kit and the look by their names, the middle of its
/// south-west square (landblock meters, a grid square's middle), the height of its floor, and how
/// many squares it is east and north (2 or more each).
#[derive(Clone, Debug, PartialEq)]
pub struct Ac2Room {
    pub landblock: u16,
    pub kit: Kit,
    pub look: String,
    pub south_west: (f32, f32),
    pub floor: f32,
    pub east: u8,
    pub north: u8,
}

/// The kit's tiles for a room's floor and ceiling: a corner, a side, the middle, and the side
/// with a doorway out, when the kit has one (the tomb's tiles open on whole sides, as wide as its
/// doorways, so a doorway is one more open side).
fn tiles(kit: Kit) -> Option<([&'static str; 3], Option<&'static str>, [&'static str; 3])> {
    Some(match kit {
        Kit::Basic => (
            ["room floor corner", "room floor side", "room floor center"],
            Some("room floor side doorway"),
            [
                "room ceiling corner",
                "room ceiling side",
                "room ceiling center",
            ],
        ),
        Kit::Fancy => (
            ["room floor corner", "room floor side", "room floor center"],
            Some("room floor doorway"),
            [
                "room ceiling corner",
                "room ceiling side",
                "room ceiling center",
            ],
        ),
        Kit::Tomb => (
            ["room floor corner", "room floor side", "room floor open"],
            None,
            ["room roof corner", "room roof side", "room roof open"],
        ),
        Kit::Mine | Kit::Castle | Kit::Cave => return None,
    })
}

/// The sides `env`'s doorways open on at `frame`, their openings up and down left out.
fn sides_at(env: &Environment, form: &crate::kit::Form, frame: &Frame) -> BTreeSet<Side> {
    (0..form.doors.len())
        .filter_map(|slot| crate::pieces::opening(env, 0, slot, frame))
        .filter(|o| o.out.z.abs() <= 0.5)
        .map(|o| Side::of(o.out))
        .collect()
}

impl Session {
    /// The open rooms of landblock `lb`, each cell's space.
    fn rooms_of(&self, cells: &BTreeMap<u16, EnvCell>) -> BTreeMap<u16, Solid> {
        cells
            .iter()
            .filter(|(_, c)| c.frame.origin.z > crate::cells::CLOSED_DEPTH + 1.0)
            .filter_map(|(n, c)| {
                Some((
                    *n,
                    self.ac2_shape(c.environment, c.cell_struct)?
                        .placed(&c.frame),
                ))
            })
            .collect()
    }

    /// Kit piece `label` of `kit` at `at`, turned the first quarter turn whose doorways open on
    /// `sides` and that fits the dungeon as it stands, every doorway it meets joined.
    fn lay_tile(
        &mut self,
        room: &Ac2Room,
        label: &str,
        at: Vec3,
        sides: &BTreeSet<Side>,
        facing: Option<(usize, Side)>,
    ) -> Result<(), Error> {
        let piece = self
            .compiled
            .ac2_pieces
            .iter()
            .find(|p| p.kit == room.kit && p.label == label)
            .cloned()
            .ok_or_else(|| {
                Error::Refused(format!(
                    "{} has no {label}: ask for the kit in a pack ([[ac2-kit]])",
                    room.kit.name()
                ))
            })?;
        let env = self.environment(piece.environment).ok_or_else(|| {
            Error::Refused(format!("{label}: its environment is not the world's"))
        })?;
        let form = crate::kit::form_on(&env, &room.kit.grid())
            .ok_or_else(|| Error::Refused(format!("{label}: no shape")))?;
        let parts: Vec<Shape> = (0..form.parts())
            .filter_map(|k| self.ac2_shape(piece.environment, k))
            .collect();
        let surfaces = crate::kit::Ac2Looks {
            kit: piece.kit.name(),
            label: piece.label,
            looks: piece.looks.clone(),
        }
        .surfaces(&room.look);
        for turn in 0..4u8 {
            let frame = Frame::new(at, crate::compile::heading_quat(90.0 * f32::from(turn)));
            if sides_at(&env, &form, &frame) != *sides {
                continue;
            }
            if let Some((slot, side)) = facing {
                let faces = crate::pieces::opening(&env, 0, slot, &frame)
                    .is_some_and(|o| Side::of(o.out) == side);
                if !faces {
                    continue;
                }
            }
            let cells = self.cells(room.landblock);
            let rooms = self.rooms_of(&cells);
            let free = self.free_doorways(room.landblock, &[]);
            let dungeon = crate::grid::Dungeon {
                rooms: &rooms,
                doorways: &free,
                ground: None,
                pitch: room.kit.grid().pitch,
            };
            let Ok(fit) = crate::grid::fit_piece(&form, &parts, &frame, &dungeon) else {
                continue;
            };
            let q = frame.rotation;
            self.apply_steps(&Op::PlacePieces {
                landblock: room.landblock,
                pieces: vec![NewPiece {
                    environment: piece.environment,
                    surfaces,
                    at: [at.x, at.y, at.z],
                    rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
                    parts: fit
                        .links
                        .iter()
                        .enumerate()
                        .map(|(k, l)| (u16::try_from(k).unwrap_or(0), l.clone()))
                        .collect(),
                }],
                joins: Vec::new(),
            })?;
            return Ok(());
        }
        Err(Error::Refused(format!(
            "the room's {label} does not fit at {:.0}, {:.0}",
            at.x, at.y
        )))
    }

    /// Lay `room`: its floor tiles, then its ceiling tiles a floor up, as one change that one undo
    /// takes back. A free doorway of the dungeon on the room's side, in the middle of one of its
    /// squares' sides, gets the kit's tile with a doorway there. Answers the cells laid.
    ///
    /// # Errors
    /// A room under two squares either way, a kit the packs do not ask for, a doorway at a corner
    /// of the room, a room of a kit entered through a room entrance (the tomb) that meets more than
    /// one hall, or a tile that does not fit where it goes (nothing is laid then).
    pub fn lay_ac2_room(&mut self, room: &Ac2Room) -> Result<Vec<u16>, Error> {
        let Some(entrance) = room.kit.room_entrance() else {
            return self.lay_room_of_tiles(room);
        };
        let (i, j, side) = match self.halls_against(room)[..] {
            [] => return self.lay_room_of_tiles(room),
            [one] => one,
            _ => return Err(Error::Refused("A tomb room can meet only one hall.".into())),
        };
        // The entrance on the room's square against the hall, its second doorway to the hall as
        // AC2's tombs set it, and the room a square back from it.
        let pitch = room.kit.grid().pitch;
        let at = Vec3::new(
            room.south_west.0 + pitch * f32::from(i),
            room.south_west.1 + pitch * f32::from(j),
            room.floor,
        );
        let back = side.turned(2);
        let step = |s: Side| match s {
            Side::East => (pitch, 0.0),
            Side::West => (-pitch, 0.0),
            Side::North => (0.0, pitch),
            Side::South => (0.0, -pitch),
        };
        let start = self.history.len();
        self.lay_tile(
            room,
            entrance,
            at,
            &BTreeSet::from([side, back]),
            Some((1, side)),
        )?;
        let (dx, dy) = step(back);
        let behind = Ac2Room {
            south_west: (room.south_west.0 + dx, room.south_west.1 + dy),
            ..room.clone()
        };
        match self.lay_room_of_tiles(&behind) {
            Ok(mut laid) => {
                self.history.merge_since(start);
                self.history.clear_redo();
                let cells: Vec<u16> = self.cells(room.landblock).keys().copied().collect();
                laid.extend(
                    cells
                        .get(cells.len().saturating_sub(laid.len() + 1))
                        .copied(),
                );
                Ok(laid)
            }
            Err(e) => {
                self.roll_back_to(start);
                Err(e)
            }
        }
    }

    /// Where the free doorways of halls (no room entrance) meet `room`'s sides: each square and
    /// side, in the room's order (south to north, west to east).
    fn halls_against(&self, room: &Ac2Room) -> Vec<(u8, u8, Side)> {
        let mut out = Vec::new();
        let Some(label) = room.kit.room_entrance() else {
            return out;
        };
        let pitch = room.kit.grid().pitch;
        let entrances: BTreeSet<dereth_primitives::DataId> = self
            .compiled
            .ac2_pieces
            .iter()
            .filter(|p| p.kit == room.kit && p.label.starts_with(label))
            .map(|p| p.environment)
            .collect();
        let cells = self.cells(room.landblock);
        let free = self.free_doorways(room.landblock, &[]);
        for j in 0..room.north {
            for i in 0..room.east {
                let x = room.south_west.0 + pitch * f32::from(i);
                let y = room.south_west.1 + pitch * f32::from(j);
                for side in Side::ALL {
                    let edge = match side {
                        Side::East => i + 1 == room.east,
                        Side::West => i == 0,
                        Side::North => j + 1 == room.north,
                        Side::South => j == 0,
                    };
                    if !edge {
                        continue;
                    }
                    let (fx, fy) = match side {
                        Side::East => (x + pitch / 2.0, y),
                        Side::West => (x - pitch / 2.0, y),
                        Side::North => (x, y + pitch / 2.0),
                        Side::South => (x, y - pitch / 2.0),
                    };
                    let hall = free.iter().any(|d| {
                        let m = d.opening.middle;
                        (m.x - fx).abs() < 1.0
                            && (m.y - fy).abs() < 1.0
                            && m.z > room.floor
                            && m.z < room.floor + room.kit.rise()
                            && !cells.values().any(|c| {
                                entrances.contains(&c.environment) && {
                                    let (dx, dy) = (m.x - c.frame.origin.x, m.y - c.frame.origin.y);
                                    (dx * dx + dy * dy).sqrt() < pitch / 2.0 + 1.0
                                }
                            })
                    });
                    if hall {
                        out.push((i, j, side));
                    }
                }
            }
        }
        out
    }

    /// [`Self::lay_ac2_room`]'s room itself, its tiles only.
    fn lay_room_of_tiles(&mut self, room: &Ac2Room) -> Result<Vec<u16>, Error> {
        if room.east < 2 || room.north < 2 {
            return Err(Error::Refused(
                "a room of tiles is two squares or more each way".into(),
            ));
        }
        let pitch = room.kit.grid().pitch;
        let (x0, y0) = room.south_west;
        let middle = |i: u8, j: u8| (x0 + pitch * f32::from(i), y0 + pitch * f32::from(j));
        // The free doorways on the room's sides, by square and side; in a kit whose rooms are
        // entered through a room entrance, only an entrance's.
        let free = self.free_doorways(room.landblock, &[]);
        let entrances: Vec<Vec3> = match room.kit.room_entrance() {
            None => Vec::new(),
            Some(label) => {
                let envs: BTreeSet<dereth_primitives::DataId> = self
                    .compiled
                    .ac2_pieces
                    .iter()
                    .filter(|p| p.kit == room.kit && p.label.starts_with(label))
                    .map(|p| p.environment)
                    .collect();
                self.cells(room.landblock)
                    .values()
                    .filter(|c| envs.contains(&c.environment))
                    .map(|c| c.frame.origin)
                    .collect()
            }
        };
        let through_entrance = |m: Vec3| {
            room.kit.room_entrance().is_none()
                || entrances.iter().any(|o| {
                    let (dx, dy) = (m.x - o.x, m.y - o.y);
                    (dx * dx + dy * dy).sqrt() < pitch / 2.0 + 1.0
                })
        };
        let door_at = |i: u8, j: u8, side: Side| {
            let (x, y) = middle(i, j);
            let (fx, fy) = match side {
                Side::East => (x + pitch / 2.0, y),
                Side::West => (x - pitch / 2.0, y),
                Side::North => (x, y + pitch / 2.0),
                Side::South => (x, y - pitch / 2.0),
            };
            free.iter().any(|d| {
                let m = d.opening.middle;
                (m.x - fx).abs() < 1.0
                    && (m.y - fy).abs() < 1.0
                    && m.z > room.floor
                    && m.z < room.floor + room.kit.rise()
                    && through_entrance(m)
            })
        };
        let (floor_tiles, doorway_tile, ceiling_tiles) = tiles(room.kit).ok_or_else(|| {
            Error::Refused(format!("the {} kit has no room tiles", room.kit.name()))
        })?;
        let mut plan: Vec<(u8, u8, BTreeSet<Side>, Option<Side>)> = Vec::new();
        for j in 0..room.north {
            for i in 0..room.east {
                let mut sides = BTreeSet::new();
                if j + 1 < room.north {
                    sides.insert(Side::North);
                }
                if j > 0 {
                    sides.insert(Side::South);
                }
                if i + 1 < room.east {
                    sides.insert(Side::East);
                }
                if i > 0 {
                    sides.insert(Side::West);
                }
                let outward: Vec<Side> = Side::ALL
                    .into_iter()
                    .filter(|s| !sides.contains(s) && door_at(i, j, *s))
                    .collect();
                let door = match outward.as_slice() {
                    [] => None,
                    [s] if sides.len() == 3 || doorway_tile.is_none() => Some(*s),
                    _ => {
                        return Err(Error::Refused(
                            "a doorway meets the room at a corner: the kit has no corner tile \
                             with a doorway"
                                .into(),
                        ))
                    }
                };
                plan.push((i, j, sides, door));
            }
        }
        // Laid outward from the doorway (from the south-west corner in a dungeon with no rooms
        // yet), so each tile joins one laid before it, as a piece in a dungeon must.
        let first = plan
            .iter()
            .position(|(_, _, _, d)| d.is_some())
            .or_else(|| self.cells(room.landblock).is_empty().then_some(0))
            .ok_or_else(|| {
                Error::Refused(
                    "the room meets no free doorway of the dungeon: lay it against one".into(),
                )
            })?;
        let mut order = vec![first];
        let mut k = 0;
        while k < order.len() {
            let (i, j, _, _) = plan[order[k]];
            for (n, (a, b, _, _)) in plan.iter().enumerate() {
                if !order.contains(&n) && a.abs_diff(i) + b.abs_diff(j) == 1 {
                    order.push(n);
                }
            }
            k += 1;
        }
        let plan: Vec<_> = order.into_iter().map(|n| plan[n].clone()).collect();
        let start = self.history.len();
        let laid = (|| -> Result<(), Error> {
            for (i, j, sides, door) in &plan {
                let (x, y) = middle(*i, *j);
                let mut open = sides.clone();
                open.extend(*door);
                let label = match (door, doorway_tile, open.len()) {
                    (Some(_), Some(t), _) => t,
                    (_, _, 2) => floor_tiles[0],
                    (_, _, 3) => floor_tiles[1],
                    _ => floor_tiles[2],
                };
                self.lay_tile(room, label, Vec3::new(x, y, room.floor), &open, None)?;
            }
            for (i, j, sides, _) in &plan {
                let (x, y) = middle(*i, *j);
                let label = match sides.len() {
                    2 => ceiling_tiles[0],
                    3 => ceiling_tiles[1],
                    _ => ceiling_tiles[2],
                };
                let z = room.floor + room.kit.layer();
                self.lay_tile(room, label, Vec3::new(x, y, z), sides, None)?;
            }
            Ok(())
        })();
        if let Err(e) = laid {
            self.roll_back_to(start);
            return Err(e);
        }
        self.history.merge_since(start);
        self.history.clear_redo();
        let cells: Vec<u16> = self.cells(room.landblock).keys().copied().collect();
        let n = plan.len() * 2;
        Ok(cells[cells.len().saturating_sub(n)..].to_vec())
    }
}
