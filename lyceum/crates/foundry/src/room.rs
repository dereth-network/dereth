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
//! meet at a doorway or share a wall fit, and two that share space do not. A piece that reaches a
//! little past its doorways (some of AC2's kit pieces, [`Shape::reaching`]) is compared that much
//! further in; every other piece, and every one of the world's own, reaches none.
//!
//! **Voxels.** A piece's cell tree may be no more than the box about it, as an AC2 piece's is, and
//! a piece that bends or forks (AC2's caves) fills little of its box: its neighbors stand in the
//! rest. Such a piece's space is the box's [`VOXEL`]-meter cells its mesh walls in
//! ([`Shape::of_voxels`]): those reached from just inside its doorways without crossing a cell its
//! mesh passes through. Two spaces either of which is so are compared cell by cell where their
//! boxes meet.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use dereth_assets::world::{CellLandblock, EnvCell};
use dereth_assets::Environment;
use dereth_primitives::frame::{globaltolocal, localtoglobal, V3};
use dereth_primitives::{Frame, Vec3};

/// How far inside a room's walls its points are sampled, and another room's walls tested: rooms
/// that touch do not share space closer than this to their walls.
pub const MARGIN: f32 = 0.25;

/// How far a room's space may stand out of the ground over it before it pokes out, in meters.
pub const GROUND_TOLERANCE: f32 = 1.0;

/// The side of the cells a space of voxels is made of, in meters ([`Shape::of_voxels`]).
pub const VOXEL: f32 = 0.25;

/// How many points at most are sampled in one room.
const MOST_SAMPLES: f32 = 3000.0;

/// A landblock's side, in meters.
const SIDE: f32 = 192.0;

/// One room's space, placed: its walls' planes in its own space (each facing in), its frame, its
/// extent in landblock meters, points inside it there, and its [`Shape::reaching`].
#[derive(Clone, Debug)]
pub struct Solid {
    frame: Frame,
    planes: Vec<(Vec3, f32)>,
    pub lo: Vec3,
    pub hi: Vec3,
    pub samples: Vec<Vec3>,
    reach: f32,
    voxels: Option<Arc<Voxels>>,
}

/// A piece's space in its own frame, sampled once: placed again at any frame by [`Self::placed`].
#[derive(Clone, Debug)]
pub struct Shape {
    planes: Vec<(Vec3, f32)>,
    corners: Vec<Vec3>,
    samples: Vec<Vec3>,
    reach: f32,
    voxels: Option<Arc<Voxels>>,
}

/// A piece's space as the cells of its box its mesh walls in ([`Shape::of_voxels`]): the box's
/// corner, how many cells it is each way, and which are in the space.
#[derive(Debug)]
pub struct Voxels {
    lo: Vec3,
    n: [usize; 3],
    filled: Vec<u64>,
}

impl Voxels {
    fn index(&self, p: Vec3) -> Option<usize> {
        let at = |v: f32, lo: f32, n: usize| {
            let i = dereth_primitives::num::floor_to_i32((v - lo) / VOXEL);
            usize::try_from(i).ok().filter(|i| *i < n)
        };
        let (i, j, k) = (
            at(p.x, self.lo.x, self.n[0])?,
            at(p.y, self.lo.y, self.n[1])?,
            at(p.z, self.lo.z, self.n[2])?,
        );
        Some((k * self.n[1] + j) * self.n[0] + i)
    }

    /// Whether local point `p` is in the space.
    fn holds(&self, p: Vec3) -> bool {
        self.index(p)
            .is_some_and(|i| self.filled[i / 64] & (1 << (i % 64)) != 0)
    }

    /// How much of its box the space fills.
    fn share(&self) -> f32 {
        let filled: u32 = self.filled.iter().map(|w| w.count_ones()).sum();
        #[allow(clippy::cast_precision_loss)]
        let all = (self.n[0] * self.n[1] * self.n[2]) as f32;
        #[allow(clippy::cast_precision_loss)]
        let filled = filled as f32;
        filled / all
    }

    /// The space of `cs` within the box from `lo` to `hi`: filled from `seeds` (local points just
    /// inside its doorways), walled by every cell its polygons pass through.
    fn of(cs: &dereth_assets::geometry::CellStruct, lo: Vec3, hi: Vec3, seeds: &[Vec3]) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = |len: f32| ((len / VOXEL).ceil() as usize).max(1);
        let n = [count(hi.x - lo.x), count(hi.y - lo.y), count(hi.z - lo.z)];
        let mut grid = Self {
            lo,
            n,
            filled: vec![0; (n[0] * n[1] * n[2]).div_ceil(64)],
        };
        let all = n[0] * n[1] * n[2];
        let mut wall = vec![false; all];
        let at: HashMap<i16, Vec3> = cs
            .vertex_array
            .vertices
            .iter()
            .map(|v| (v.id, v.position))
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let corner = |i: usize, j: usize, k: usize| {
            Vec3::new(
                lo.x + i as f32 * VOXEL,
                lo.y + j as f32 * VOXEL,
                lo.z + k as f32 * VOXEL,
            )
        };
        let cell = |v: f32, lo: f32, n: usize| {
            let i = dereth_primitives::num::floor_to_i32((v - lo) / VOXEL).max(0);
            usize::try_from(i).unwrap_or(0).min(n - 1)
        };
        for p in &cs.polygons {
            let v: Vec<Vec3> = p
                .vertex_ids
                .iter()
                .filter_map(|i| at.get(&i16::try_from(*i).ok()?).copied())
                .collect();
            for t in 1..v.len().saturating_sub(1) {
                let tri = [v[0], v[t], v[t + 1]];
                let (min, max) = tri.iter().fold(
                    (
                        Vec3::new(f32::MAX, f32::MAX, f32::MAX),
                        Vec3::new(f32::MIN, f32::MIN, f32::MIN),
                    ),
                    |(a, b), p| {
                        (
                            Vec3::new(a.x.min(p.x), a.y.min(p.y), a.z.min(p.z)),
                            Vec3::new(b.x.max(p.x), b.y.max(p.y), b.z.max(p.z)),
                        )
                    },
                );
                for k in cell(min.z, lo.z, n[2])..=cell(max.z, lo.z, n[2]) {
                    for j in cell(min.y, lo.y, n[1])..=cell(max.y, lo.y, n[1]) {
                        for i in cell(min.x, lo.x, n[0])..=cell(max.x, lo.x, n[0]) {
                            let at = (k * n[1] + j) * n[0] + i;
                            if !wall[at] && tri_meets_cell(&tri, corner(i, j, k)) {
                                wall[at] = true;
                            }
                        }
                    }
                }
            }
        }
        let mut todo: Vec<usize> = Vec::new();
        for s in seeds {
            if let Some(i) = grid.index(*s) {
                if !wall[i] {
                    todo.push(i);
                }
            }
        }
        let mut seen = vec![false; all];
        while let Some(i) = todo.pop() {
            if seen[i] || wall[i] {
                continue;
            }
            seen[i] = true;
            grid.filled[i / 64] |= 1 << (i % 64);
            let (x, rest) = (i % n[0], i / n[0]);
            let (y, z) = (rest % n[1], rest / n[1]);
            if x > 0 {
                todo.push(i - 1);
            }
            if x + 1 < n[0] {
                todo.push(i + 1);
            }
            if y > 0 {
                todo.push(i - n[0]);
            }
            if y + 1 < n[1] {
                todo.push(i + n[0]);
            }
            if z > 0 {
                todo.push(i - n[0] * n[1]);
            }
            if z + 1 < n[2] {
                todo.push(i + n[0] * n[1]);
            }
        }
        grid
    }
}

/// Whether triangle `tri` passes through the cell whose low corner is `lo` ([`VOXEL`] a side): by
/// the separating axes of a triangle and a box.
fn tri_meets_cell(tri: &[Vec3; 3], lo: Vec3) -> bool {
    let h = VOXEL * 0.5;
    let c = Vec3::new(lo.x + h, lo.y + h, lo.z + h);
    let v = [tri[0].sub(c), tri[1].sub(c), tri[2].sub(c)];
    let e = [v[1].sub(v[0]), v[2].sub(v[1]), v[0].sub(v[2])];
    let axes = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let cross = |a: Vec3, b: Vec3| {
        Vec3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    };
    let apart_on = |axis: Vec3| {
        let p = [v[0].dot(axis), v[1].dot(axis), v[2].dot(axis)];
        let r = h * (axis.x.abs() + axis.y.abs() + axis.z.abs());
        p.iter().copied().fold(f32::MAX, f32::min) > r
            || p.iter().copied().fold(f32::MIN, f32::max) < -r
    };
    if axes.iter().any(|a| apart_on(*a)) {
        return false;
    }
    if apart_on(cross(e[0], e[1])) {
        return false;
    }
    !axes
        .iter()
        .any(|a| e.iter().any(|ed| apart_on(cross(*a, *ed))))
}

/// Points of the box from `lo` to `hi`, about `MOST_SAMPLES` of them evenly, that `inside` keeps.
fn sample(lo: Vec3, hi: Vec3, inside: impl Fn(Vec3) -> bool) -> Vec<Vec3> {
    let size = hi.sub(lo);
    let volume = (size.x * size.y * size.z).max(1.0);
    let step = dereth_primitives::num::math::powf(volume / MOST_SAMPLES, 1.0 / 3.0).max(0.5);
    let mut samples = Vec::new();
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
    samples
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
        let samples = sample(lo, hi, |q| {
            planes.iter().all(|(n, d)| n.dot(q) + d >= MARGIN)
        });
        let corners = [lo.x, hi.x]
            .into_iter()
            .flat_map(|x| [lo.y, hi.y].into_iter().map(move |y| (x, y)))
            .flat_map(|(x, y)| [lo.z, hi.z].into_iter().map(move |z| Vec3::new(x, y, z)))
            .collect();
        Some(Self {
            planes,
            corners,
            samples,
            reach: 0.0,
            voxels: None,
        })
    }

    /// Piece `piece` of `env` as the cells of its box its mesh walls in: filled from a little
    /// inside each of its doorways, never through a cell one of its polygons passes through, so
    /// that a piece that bends or forks holds its own space and not the rest of its box. `None`
    /// for a piece the environment does not hold.
    #[must_use]
    pub fn of_voxels(env: &Environment, piece: u16) -> Option<Self> {
        let mut shape = Self::of(env, piece)?;
        let cs = env.cells.get(usize::from(piece))?;
        let (lo, hi) = shape.corners.iter().fold(
            (
                Vec3::new(f32::MAX, f32::MAX, f32::MAX),
                Vec3::new(f32::MIN, f32::MIN, f32::MIN),
            ),
            |(lo, hi), p| {
                (
                    Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
                    Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
                )
            },
        );
        let here = Frame::new(Vec3::ZERO, dereth_primitives::Quat::IDENTITY);
        let seeds: Vec<Vec3> = (0..cs.portals.len())
            .filter_map(|slot| crate::pieces::opening(env, piece, slot, &here))
            .flat_map(|o| (2..8u8).map(move |k| o.middle.sub(o.out.mul(f32::from(k) * VOXEL))))
            .collect();
        let voxels = Voxels::of(cs, lo, hi, &seeds);
        shape.samples = sample(lo, hi, |q| voxels.holds(q));
        shape.voxels = Some(Arc::new(voxels));
        Some(shape)
    }

    /// How much of its box its space fills, for a space of voxels ([`Self::of_voxels`]).
    #[must_use]
    pub fn filled_share(&self) -> Option<f32> {
        self.voxels.as_ref().map(|v| v.share())
    }

    /// This space, for a piece that reaches `reach` meters past its doorways (as some AC2 kit
    /// pieces do, a little into the piece joined there): another space shares space with it only
    /// deeper in than that ([`Solid::overlaps`]). Every other space reaches none.
    #[must_use]
    pub fn reaching(mut self, reach: f32) -> Self {
        self.reach = reach;
        self
    }

    /// How far past its doorways it reaches ([`Self::reaching`]), in meters.
    #[must_use]
    pub fn reach(&self) -> f32 {
        self.reach
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
            reach: self.reach,
            voxels: self.voxels.clone(),
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
        self.holds_in(p, MARGIN)
    }

    /// Whether point `p` is in this space, `margin` in from its walls.
    fn holds_in(&self, p: Vec3, margin: f32) -> bool {
        self.holds_local(globaltolocal(&self.frame, p), margin)
    }

    /// Whether this space and `other` share space: a point of either inside the other, a
    /// [`MARGIN`] in, or as much further as the farther-reaching of the two reaches
    /// ([`Shape::reaching`]).
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        let apart = self.hi.x < other.lo.x
            || other.hi.x < self.lo.x
            || self.hi.y < other.lo.y
            || other.hi.y < self.lo.y
            || self.hi.z < other.lo.z
            || other.hi.z < self.lo.z;
        let margin = MARGIN + self.reach.max(other.reach);
        if apart {
            return false;
        }
        if self.voxels.is_some() || other.voxels.is_some() {
            return self.shares_a_cell(other, margin);
        }
        self.samples.iter().any(|p| other.holds_in(*p, margin))
            || other.samples.iter().any(|p| self.holds_in(*p, margin))
    }

    /// Whether some point [`VOXEL`] meters from the next, where this space's box and `other`'s
    /// meet, is in both, `margin` in from their boxes.
    fn shares_a_cell(&self, other: &Self, margin: f32) -> bool {
        let lo = Vec3::new(
            self.lo.x.max(other.lo.x),
            self.lo.y.max(other.lo.y),
            self.lo.z.max(other.lo.z),
        );
        let hi = Vec3::new(
            self.hi.x.min(other.hi.x),
            self.hi.y.min(other.hi.y),
            self.hi.z.min(other.hi.z),
        );
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = |len: f32| (len / VOXEL).ceil().max(0.0) as u32;
        let (nx, ny, nz) = (count(hi.x - lo.x), count(hi.y - lo.y), count(hi.z - lo.z));
        let first = Vec3::new(lo.x + VOXEL * 0.5, lo.y + VOXEL * 0.5, lo.z + VOXEL * 0.5);
        // Each space's own coordinates step evenly along the landblock's axes.
        let steps = |s: &Self| {
            let at = globaltolocal(&s.frame, first);
            let step = |d: Vec3| globaltolocal(&s.frame, first.add(d)).sub(at);
            (
                at,
                step(Vec3::new(VOXEL, 0.0, 0.0)),
                step(Vec3::new(0.0, VOXEL, 0.0)),
                step(Vec3::new(0.0, 0.0, VOXEL)),
            )
        };
        let (a0, ax, ay, az) = steps(self);
        let (b0, bx, by, bz) = steps(other);
        #[allow(clippy::cast_precision_loss)]
        let along = |o: Vec3, x: Vec3, y: Vec3, z: Vec3, i: u32, j: u32, k: u32| {
            o.add(x.mul(i as f32))
                .add(y.mul(j as f32))
                .add(z.mul(k as f32))
        };
        (0..nz).any(|k| {
            (0..ny).any(|j| {
                (0..nx).any(|i| {
                    self.holds_local(along(a0, ax, ay, az, i, j, k), margin)
                        && other.holds_local(along(b0, bx, by, bz, i, j, k), margin)
                })
            })
        })
    }

    /// Whether point `q` of its own frame is in this space, `margin` in from its walls.
    fn holds_local(&self, q: Vec3, margin: f32) -> bool {
        self.planes.iter().all(|(n, d)| n.dot(q) + d >= margin)
            && self.voxels.as_ref().is_none_or(|v| v.holds(q))
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
    terrain.grid_height().iter().all(|h| *h == 0)
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

    /// A box of space from `lo` to `hi` (landblock meters), sampled a meter apart, reaching
    /// `reach`.
    fn boxed(lo: Vec3, hi: Vec3, reach: f32) -> Solid {
        let planes = vec![
            (Vec3::new(1.0, 0.0, 0.0), -lo.x),
            (Vec3::new(-1.0, 0.0, 0.0), hi.x),
            (Vec3::new(0.0, 1.0, 0.0), -lo.y),
            (Vec3::new(0.0, -1.0, 0.0), hi.y),
            (Vec3::new(0.0, 0.0, 1.0), -lo.z),
            (Vec3::new(0.0, 0.0, -1.0), hi.z),
        ];
        let mut samples = Vec::new();
        let steps = |a: f32, b: f32| {
            // LINT-OK: a few tens of meters.
            #[allow(clippy::cast_possible_truncation)]
            let n = (b - a).round() as i32;
            (0..=n).map(move |k| a + MARGIN + (b - a - 2.0 * MARGIN) * k as f32 / n as f32)
        };
        for x in steps(lo.x, hi.x) {
            for y in steps(lo.y, hi.y) {
                for z in steps(lo.z, hi.z) {
                    samples.push(Vec3::new(x, y, z));
                }
            }
        }
        Solid {
            frame: Frame::new(Vec3::ZERO, dereth_primitives::Quat::IDENTITY),
            planes,
            lo,
            hi,
            samples,
            reach,
            voxels: None,
        }
    }

    #[test]
    fn a_space_reaching_past_its_doorway_shares_only_that_much_with_the_next() {
        // Two 20 m halls end to end, the west one reaching `into` meters into the east one.
        let pair = |into: f32, reach: f32| {
            let west = boxed(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(20.0 + into, 10.0, 10.0),
                reach,
            );
            let east = boxed(Vec3::new(20.0, 0.0, 0.0), Vec3::new(40.0, 10.0, 10.0), 0.0);
            (west.overlaps(&east), east.overlaps(&west))
        };
        // Spaces that reach none, as every one of the world's own: 0.84 m shared is a clash, as
        // it always was, and meeting at the wall is not.
        assert_eq!(pair(0.84, 0.0), (true, true));
        assert_eq!(pair(0.0, 0.0), (false, false));
        // Reaching 0.84 m: that much shared fits, from either side; more does not.
        assert_eq!(pair(0.84, 0.84), (false, false));
        assert_eq!(pair(2.0, 0.84), (true, true));
    }

    fn block(heights: u8) -> CellLandblock {
        CellLandblock {
            id: dereth_primitives::DataId(0x1234_FFFF),
            lbi_exists: 0,
            terrain: [0; 81],
            height: [heights; 81],
            dense: None,
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
