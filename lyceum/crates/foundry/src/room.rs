//! Room for a dungeon: where a new one can go, and whether a room placed fits where it stands.
//!
//! **Where.** Retail builds nearly all its dungeons (1,751 of its 1,970 dungeon landblocks) in
//! landblocks of the open sea floor: every vertex at height zero, under deep sea, with no statics
//! of their own. Their rooms sit around that floor, from tens of meters under it to tens over, out
//! of everyone's sight: the client draws a dungeon's rooms only from inside, and no landscape from
//! there. The rest lie under the land, below the ground of their landblock. So a new dungeon goes
//! in a landblock of the sea floor that nothing uses yet ([`room_for_dungeon`]).
//!
//! **Fit.** A room fits where its space is its own: no other room's ([`Solid::overlaps`]), and,
//! in a landblock with land, not across its ground ([`Solid::crosses_ground`]). Rooms are compared by points
//! sampled inside each, a little in from its walls, against the other's walls, so two rooms that
//! meet at a doorway or share a wall fit, and two that share space do not.

use std::collections::BTreeMap;

use dereth_assets::world::{CellLandblock, EnvCell};
use dereth_assets::Environment;
use dereth_primitives::frame::{globaltolocal, localtoglobal, V3};
use dereth_primitives::{Frame, Vec3};

/// How far inside a room's walls its points are sampled, and another room's walls tested: rooms
/// that touch do not share space closer than this to their walls.
pub const MARGIN: f32 = 0.25;

/// How far a room's space may stand out of the ground over it before it pokes out, in meters.
pub const GROUND_TOLERANCE: f32 = 1.0;

/// How many points at most are sampled in one room.
const MOST_SAMPLES: f32 = 3000.0;

/// A landblock's side, in meters.
const SIDE: f32 = 192.0;

/// One room's space, placed: its walls' planes in its own space (each facing in), its frame, its
/// extent in landblock meters, and points inside it there.
#[derive(Clone, Debug)]
pub struct Solid {
    frame: Frame,
    planes: Vec<(Vec3, f32)>,
    pub lo: Vec3,
    pub hi: Vec3,
    pub samples: Vec<Vec3>,
}

/// A piece's space in its own frame, sampled once: placed again at any frame by [`Self::placed`].
#[derive(Clone, Debug)]
pub struct Shape {
    planes: Vec<(Vec3, f32)>,
    corners: Vec<Vec3>,
    samples: Vec<Vec3>,
}

impl Shape {
    /// Piece `piece` of `env`: its cell tree's planes, and points of its space a [`MARGIN`] in
    /// from them. `None` for a piece the environment does not hold.
    #[must_use]
    pub fn of(env: &Environment, piece: u16) -> Option<Self> {
        let cs = env.cells.get(usize::from(piece))?;
        let planes: Vec<(Vec3, f32)> = cs
            .cell_bsp
            .nodes
            .iter()
            .filter_map(|n| n.plane)
            .map(|p| (p.normal, p.d))
            .collect();
        let (mut lo, mut hi) = (
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        );
        for v in &cs.vertex_array.vertices {
            let p = v.position;
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        if planes.is_empty() || lo.x > hi.x {
            return None;
        }
        let size = hi.sub(lo);
        let volume = (size.x * size.y * size.z).max(1.0);
        let step = dereth_primitives::num::math::powf(volume / MOST_SAMPLES, 1.0 / 3.0).max(0.5);
        let mut samples = Vec::new();
        let inside = |q: Vec3| planes.iter().all(|(n, d)| n.dot(q) + d >= MARGIN);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = |len: f32| ((len / step).ceil() as u32).max(1);
        let (nx, ny, nz) = (count(size.x), count(size.y), count(size.z));
        for i in 0..nx {
            for j in 0..ny {
                for k in 0..nz {
                    #[allow(clippy::cast_precision_loss)]
                    let q = Vec3::new(
                        lo.x + (i as f32 + 0.5) * size.x / nx as f32,
                        lo.y + (j as f32 + 0.5) * size.y / ny as f32,
                        lo.z + (k as f32 + 0.5) * size.z / nz as f32,
                    );
                    if inside(q) {
                        samples.push(q);
                    }
                }
            }
        }
        let corners = [lo.x, hi.x]
            .into_iter()
            .flat_map(|x| [lo.y, hi.y].into_iter().map(move |y| (x, y)))
            .flat_map(|(x, y)| [lo.z, hi.z].into_iter().map(move |z| Vec3::new(x, y, z)))
            .collect();
        Some(Self {
            planes,
            corners,
            samples,
        })
    }

    /// This space placed at `frame` (landblock meters).
    #[must_use]
    pub fn placed(&self, frame: &Frame) -> Solid {
        let (mut lo, mut hi) = (
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        );
        for c in &self.corners {
            let p = localtoglobal(frame, *c);
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        Solid {
            frame: *frame,
            planes: self.planes.clone(),
            lo,
            hi,
            samples: self
                .samples
                .iter()
                .map(|s| localtoglobal(frame, *s))
                .collect(),
        }
    }
}

impl Solid {
    /// Cell `cell`'s space, built of `env`.
    #[must_use]
    pub fn of_cell(cell: &EnvCell, env: &Environment) -> Option<Self> {
        Shape::of(env, cell.cell_struct).map(|s| s.placed(&cell.frame))
    }

    /// Whether point `p` (landblock meters) is in this space, a [`MARGIN`] in from its walls.
    #[must_use]
    pub fn holds(&self, p: Vec3) -> bool {
        let q = globaltolocal(&self.frame, p);
        self.planes.iter().all(|(n, d)| n.dot(q) + d >= MARGIN)
    }

    /// Whether this space and `other` share space: a point of either inside the other.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        let apart = self.hi.x < other.lo.x
            || other.hi.x < self.lo.x
            || self.hi.y < other.lo.y
            || other.hi.y < self.lo.y
            || self.hi.z < other.lo.z
            || other.hi.z < self.lo.z;
        !apart
            && (self.samples.iter().any(|p| other.holds(*p))
                || other.samples.iter().any(|p| self.holds(*p)))
    }

    /// Whether this space crosses the ground of a landblock with land, `ground` answering its
    /// height at landblock meters `(x, y)` inside the landblock: some of it more than
    /// [`GROUND_TOLERANCE`] over the ground there and some more than that under it. A room wholly
    /// under the ground, or wholly over it as retail's are in the sky, does not.
    #[must_use]
    pub fn crosses_ground(&self, ground: &dyn Fn(f32, f32) -> Option<f32>) -> bool {
        let (mut over, mut under) = (false, false);
        for p in &self.samples {
            if !((0.0..SIDE).contains(&p.x) && (0.0..SIDE).contains(&p.y)) {
                continue;
            }
            let Some(g) = ground(p.x, p.y) else { continue };
            over |= p.z > g + GROUND_TOLERANCE;
            under |= p.z < g - GROUND_TOLERANCE;
            if over && under {
                return true;
            }
        }
        false
    }
}

/// Whether a landblock's terrain is the open sea floor: every vertex at height zero.
#[must_use]
pub fn sea_floor(terrain: &CellLandblock) -> bool {
    terrain.height.iter().all(|h| *h == 0)
}

/// What uses a landblock, so that a new dungeon has no room there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taken {
    /// Its ground is land, not the open sea floor.
    Land,
    /// It has statics: buildings or scenery.
    Statics,
    /// It has rooms: another dungeon, or a building's.
    Rooms,
    /// It is on the world's edge, or has no terrain record.
    Edge,
}

/// Whether landblock `lb` has room for a new dungeon: open sea floor (`terrain`), with no
/// statics (`statics`, its statics record, if any, has none) and no rooms (`rooms`, how many
/// cells it has). See the module documentation for why.
///
/// # Errors
/// What uses it.
pub fn room_for_dungeon(
    lb: u16,
    terrain: Option<&CellLandblock>,
    statics: Option<&dereth_assets::world::LandblockInfo>,
    rooms: usize,
) -> Result<(), Taken> {
    if (lb >> 8) >= 0xFF || (lb & 0xFF) >= 0xFF {
        return Err(Taken::Edge);
    }
    let terrain = terrain.ok_or(Taken::Edge)?;
    if !sea_floor(terrain) {
        return Err(Taken::Land);
    }
    if rooms > 0 || statics.is_some_and(|s| s.num_cells > 0) {
        return Err(Taken::Rooms);
    }
    if statics.is_some_and(|s| !s.objects.is_empty() || !s.buildings.is_empty()) {
        return Err(Taken::Statics);
    }
    Ok(())
}

/// What a room placed in a landblock clashes with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clash {
    /// It crosses the land's ground.
    Ground,
    /// It shares space with this room (by its place in the landblock).
    Room(u16),
}

/// What `solid`, placed in a landblock whose rooms are `rooms` (by index, each as built of the
/// environment `env` names), clashes with: the rooms it shares space with, but those of
/// `ignore`; and the ground, when `ground` answers the landblock's land (none for the sea floor).
#[must_use]
pub fn clashes(
    solid: &Solid,
    rooms: &BTreeMap<u16, Solid>,
    ignore: &[u16],
    ground: Option<&dyn Fn(f32, f32) -> Option<f32>>,
) -> Vec<Clash> {
    let mut out: Vec<Clash> = rooms
        .iter()
        .filter(|(n, _)| !ignore.contains(n))
        .filter(|(_, r)| solid.overlaps(r))
        .map(|(n, _)| Clash::Room(*n))
        .collect();
    if ground.is_some_and(|g| solid.crosses_ground(g)) {
        out.push(Clash::Ground);
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: where a world builder's dungeon has room)
    use super::*;

    fn block(heights: u8) -> CellLandblock {
        CellLandblock {
            id: dereth_primitives::DataId(0x1234_FFFF),
            lbi_exists: 0,
            terrain: [0; 81],
            height: [heights; 81],
        }
    }

    #[test]
    fn a_new_dungeon_has_room_only_on_the_sea_floor_with_nothing_else_there() {
        assert_eq!(room_for_dungeon(0x1234, Some(&block(0)), None, 0), Ok(()));
        assert_eq!(
            room_for_dungeon(0x1234, Some(&block(3)), None, 0),
            Err(Taken::Land)
        );
        assert_eq!(
            room_for_dungeon(0x1234, Some(&block(0)), None, 4),
            Err(Taken::Rooms)
        );
        assert_eq!(
            room_for_dungeon(0x12FF, Some(&block(0)), None, 0),
            Err(Taken::Edge)
        );
        assert_eq!(room_for_dungeon(0x1234, None, None, 0), Err(Taken::Edge));
    }
}
