//! The walk checks of a landblock's indoor cells: every portal leads somewhere and back, every
//! cell is reached from the first through portals, and every cell has a floor to stand on, its
//! own or one below an opening in it.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::world::EnvCell;
use dereth_assets::Environment;
use dereth_primitives::frame::{globaltolocal, localtoglobal, V3};
use dereth_primitives::{DataId, Vec3};

use crate::pieces::{mismatch, opening};

/// How far below a cell's middle the walk looks for its floor, in meters.
pub const FLOOR_REACH: f32 = 60.0;

/// How much nearer than a floor an opening must be to be fallen through, in meters: one that
/// meets the floor at its edge is not.
const EDGE: f32 = 0.01;

/// How high above the floor a standing body's middle is, where the game finds the room it is
/// in: its lowest sphere's middle, in meters.
const STANDING_MIDDLE: f32 = 0.5;

/// One thing the walk found wrong with a cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub cell: u16,
    pub text: String,
}

/// A polygon in landblock space.
struct Poly {
    corners: Vec<Vec3>,
    n: Vec3,
}

fn poly(corners: Vec<Vec3>) -> Option<Poly> {
    if corners.len() < 3 {
        return None;
    }
    let mut n = Vec3::ZERO;
    for i in 0..corners.len() {
        let (a, b) = (corners[i], corners[(i + 1) % corners.len()]);
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    let len = n.dot(n).sqrt();
    (len > 1e-6).then(|| Poly {
        corners,
        n: Vec3::new(n.x / len, n.y / len, n.z / len),
    })
}

/// Where a ray from `from` straight down meets `p`: how far down; `None` when it misses.
fn down_hit(p: &Poly, from: Vec3) -> Option<f32> {
    if p.n.z.abs() < 1e-4 {
        return None;
    }
    let t = p.n.dot(p.corners[0].sub(from)) / -p.n.z;
    if t < -1e-3 {
        return None;
    }
    let at = Vec3::new(from.x, from.y, from.z - t);
    let mut sign = 0.0f32;
    for i in 0..p.corners.len() {
        let (a, b) = (p.corners[i], p.corners[(i + 1) % p.corners.len()]);
        let e = b.sub(a);
        let w = at.sub(a);
        let c = Vec3::new(
            e.y * w.z - e.z * w.y,
            e.z * w.x - e.x * w.z,
            e.x * w.y - e.y * w.x,
        );
        let x = c.dot(p.n);
        if x.abs() < 1e-5 {
            continue;
        }
        if sign == 0.0 {
            sign = x.signum();
        } else if x.signum() != sign {
            return None;
        }
    }
    Some(t.max(0.0))
}

/// Each cell's floor polygons (the physics polygons) and its portal polygons with where they
/// lead, in landblock space, and points inside it.
struct Placed {
    /// Points inside the cell: its middle, and halfway from it to each corner.
    inner: Vec<Vec3>,
    solid: Vec<Poly>,
    portals: Vec<(Option<u16>, Poly)>,
}

fn place(cell: &EnvCell, env: &Environment) -> Option<Placed> {
    let cs = env.cells.get(usize::from(cell.cell_struct))?;
    let verts = &cs.vertex_array.vertices;
    let at = |i: u16| {
        verts
            .get(usize::from(i))
            .map(|v| localtoglobal(&cell.frame, v.position))
    };
    #[allow(clippy::cast_precision_loss)]
    let middle = {
        let k = verts.len().max(1) as f32;
        let s = verts.iter().fold(Vec3::ZERO, |s, v| s.add(v.position));
        localtoglobal(&cell.frame, Vec3::new(s.x / k, s.y / k, s.z / k))
    };
    let inner = std::iter::once(middle)
        .chain(verts.iter().map(|v| {
            let p = localtoglobal(&cell.frame, v.position);
            Vec3::new(
                (p.x + middle.x) / 2.0,
                (p.y + middle.y) / 2.0,
                (p.z + middle.z) / 2.0,
            )
        }))
        .collect();
    let solid = cs
        .physics_polygons
        .iter()
        .filter_map(|p| poly(p.vertex_ids.iter().map(|i| at(*i)).collect::<Option<_>>()?))
        .collect();
    let portals = cell
        .portals
        .iter()
        .filter_map(|p| {
            let pg = cs.polygons.get(usize::from(p.polygon_id))?;
            #[allow(clippy::cast_possible_truncation)]
            let to = (p.flags & 4 == 0).then_some(p.other_cell_id as u16);
            Some((
                to,
                poly(
                    pg.vertex_ids
                        .iter()
                        .map(|i| at(*i))
                        .collect::<Option<_>>()?,
                )?,
            ))
        })
        .collect();
    Some(Placed {
        inner,
        solid,
        portals,
    })
}

/// The walk's findings over `cells` (a landblock's, by index), those of `which` checked; the
/// walk starts from `first` (and checks nothing is out of its reach only when one is given);
/// `env` reads an environment.
#[must_use]
pub fn check(
    cells: &BTreeMap<u16, EnvCell>,
    which: &BTreeSet<u16>,
    first: Option<u16>,
    env: &dyn Fn(DataId) -> Option<Environment>,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut envs: BTreeMap<u32, Option<Environment>> = BTreeMap::new();
    let mut placed: BTreeMap<u16, Placed> = BTreeMap::new();
    for (c, cell) in cells {
        let e = envs
            .entry(cell.environment.raw())
            .or_insert_with(|| env(cell.environment));
        match e.as_ref().and_then(|e| place(cell, e)) {
            Some(p) => {
                placed.insert(*c, p);
            }
            None if which.contains(c) => out.push(Finding {
                cell: *c,
                text: format!(
                    "its piece {} of environment {:#010X} is not there",
                    cell.cell_struct,
                    cell.environment.raw()
                ),
            }),
            None => {}
        }
    }
    // Every portal leads somewhere, and back, through a polygon that meets its own.
    for c in which {
        let Some(cell) = cells.get(c) else { continue };
        for (i, p) in cell.portals.iter().enumerate() {
            if p.flags & 4 != 0 {
                continue;
            }
            #[allow(clippy::cast_possible_truncation)]
            let to = p.other_cell_id as u16;
            let Some(other) = cells.get(&to) else {
                out.push(Finding {
                    cell: *c,
                    text: format!("portal {i} leads to cell {to:04X}, which is not there"),
                });
                continue;
            };
            let back = usize::try_from(p.other_portal_id)
                .ok()
                .and_then(|j| other.portals.get(j));
            #[allow(clippy::cast_possible_truncation)]
            if !back.is_some_and(|b| b.flags & 4 == 0 && b.other_cell_id as u16 == *c) {
                out.push(Finding {
                    cell: *c,
                    text: format!(
                        "portal {i} leads to cell {to:04X}, whose portal does not lead back"
                    ),
                });
                continue;
            }
            let (Some(e1), Some(e2)) = (
                envs.get(&cell.environment.raw()).and_then(Option::as_ref),
                envs.get(&other.environment.raw()).and_then(Option::as_ref),
            ) else {
                continue;
            };
            let slot = |cell: &EnvCell, e: &Environment, polygon: u16| {
                e.cells
                    .get(usize::from(cell.cell_struct))?
                    .portals
                    .iter()
                    .position(|p| *p == polygon)
            };
            let j = usize::try_from(p.other_portal_id).unwrap_or(0);
            let (Some(a), Some(b)) = (
                slot(cell, e1, p.polygon_id)
                    .and_then(|s| opening(e1, cell.cell_struct, s, &cell.frame)),
                slot(other, e2, other.portals[j].polygon_id)
                    .and_then(|s| opening(e2, other.cell_struct, s, &other.frame)),
            ) else {
                continue;
            };
            // Two portals that say they match exactly must meet; others may only overlap.
            if p.flags & 1 != 0 && mismatch(&a, &b) > 0.05 {
                out.push(Finding {
                    cell: *c,
                    text: format!(
                        "portal {i} does not meet cell {to:04X}'s: their corners are {:.2} m apart",
                        mismatch(&a, &b)
                    ),
                });
            }
        }
    }
    // Every cell is reached from the first.
    if let Some(start) = first {
        let mut reached = BTreeSet::new();
        let mut todo = vec![start];
        while let Some(c) = todo.pop() {
            if !reached.insert(c) {
                continue;
            }
            for p in cells
                .get(&c)
                .map(|c| c.portals.as_slice())
                .unwrap_or_default()
            {
                if p.flags & 4 == 0 {
                    #[allow(clippy::cast_possible_truncation)]
                    todo.push(p.other_cell_id as u16);
                }
            }
        }
        for c in which {
            if !reached.contains(c) {
                out.push(Finding {
                    cell: *c,
                    text: format!("no walk from cell {start:04X} reaches it"),
                });
            }
        }
    }
    // Every cell has a floor: straight down from a point inside it (its middle, or halfway to a
    // corner), a solid polygon of its own, or one of a cell below an opening in it. An opening may
    // lead to several cells below, each under part of it: any of them will do.
    for c in which {
        let Some(start) = placed.get(c) else { continue };
        let under = start
            .inner
            .iter()
            .map(|p| floor_below(&placed, *c, *p, 0.0, &mut BTreeSet::new()))
            .min()
            .unwrap_or(Under::Nothing);
        let text = match under {
            Under::Floor => continue,
            Under::Deep => {
                format!("nothing under it for {FLOOR_REACH:.0} m: a body falls further than that")
            }
            Under::Nothing => "nothing under it to stand on: a body falls out".to_owned(),
        };
        out.push(Finding { cell: *c, text });
    }
    out
}

/// What is under a point: a floor (or the landscape, through an opening to the outdoors),
/// nothing within [`FLOOR_REACH`], or nothing at all. Ordered best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Under {
    Floor,
    Deep,
    Nothing,
}

/// What is straight down from `from` in cell `here`, having fallen `went` meters already: a solid
/// polygon of the cell or, through an opening in its floor, of one below.
fn floor_below(
    placed: &BTreeMap<u16, Placed>,
    here: u16,
    from: Vec3,
    went: f32,
    path: &mut BTreeSet<u16>,
) -> Under {
    let Some(at) = placed.get(&here) else {
        return Under::Nothing;
    };
    if !path.insert(here) {
        return Under::Nothing;
    }
    let solid = at
        .solid
        .iter()
        .filter_map(|p| down_hit(p, from))
        .fold(f32::MAX, f32::min);
    let opening = at
        .portals
        .iter()
        .filter_map(|(_, p)| down_hit(p, from))
        .fold(f32::MAX, f32::min);
    let found = if solid <= opening {
        if solid == f32::MAX {
            Under::Nothing
        } else if solid < FLOOR_REACH - went {
            Under::Floor
        } else {
            Under::Deep
        }
    } else {
        at.portals
            .iter()
            .filter_map(|(to, p)| {
                let d = down_hit(p, from)?;
                if d >= solid {
                    return None;
                }
                Some(match to {
                    Some(_) if went + d >= FLOOR_REACH => Under::Deep,
                    Some(to) => floor_below(
                        placed,
                        *to,
                        Vec3::new(from.x, from.y, from.z - d - 0.01),
                        went + d,
                        path,
                    ),
                    // The landscape is under an opening to the outdoors.
                    None => Under::Floor,
                })
            })
            .min()
            .unwrap_or(Under::Nothing)
    };
    path.remove(&here);
    found
}

/// An indoor cell's room as its cell tree bounds it: what a point in it is tested against.
#[derive(Clone, Debug)]
pub struct CellShape {
    /// The cell's place in its landblock (`0x0100` and on).
    pub index: u16,
    /// Whether the cell is seen from outside: a building's room, not a dungeon's.
    pub seen_outside: bool,
    frame: dereth_primitives::Frame,
    /// The cell tree's planes, in the cell's own space, each facing in.
    planes: Vec<(Vec3, f32)>,
}

impl CellShape {
    /// Whether point `p` (landblock meters) is in the room: in front of every plane of its cell
    /// tree, or on one, as the client tests a point against a cell.
    #[must_use]
    pub fn holds(&self, p: Vec3) -> bool {
        let q = globaltolocal(&self.frame, p);
        !self.planes.is_empty() && self.planes.iter().all(|(n, d)| n.dot(q) + d >= -0.0002)
    }
}

/// The rooms of `cells` (a landblock's, by index), each as its cell tree bounds it.
#[must_use]
pub fn cell_shapes(
    cells: &BTreeMap<u16, EnvCell>,
    env: &dyn Fn(DataId) -> Option<Environment>,
) -> Vec<CellShape> {
    let mut out = Vec::new();
    let mut envs: BTreeMap<DataId, Option<Environment>> = BTreeMap::new();
    for (index, cell) in cells {
        let e = envs
            .entry(cell.environment)
            .or_insert_with(|| env(cell.environment));
        let Some(cs) = e
            .as_ref()
            .and_then(|e| e.cells.get(usize::from(cell.cell_struct)))
        else {
            continue;
        };
        out.push(shape(*index, cell, cs));
    }
    out
}

/// Cell `cell` (of piece `cs`, at `index`) as its cell tree bounds it.
fn shape(index: u16, cell: &EnvCell, cs: &dereth_assets::geometry::CellStruct) -> CellShape {
    CellShape {
        index,
        seen_outside: cell.flags & 1 != 0,
        frame: cell.frame,
        planes: cs
            .cell_bsp
            .nodes
            .iter()
            .filter_map(|n| n.plane)
            .map(|p| (p.normal, p.d))
            .collect(),
    }
}

/// Whether cell `cell` (of piece `cs`) extends over point `p` (landblock meters): the box its
/// corners span holds it.
fn extent_holds(cell: &EnvCell, cs: &dereth_assets::geometry::CellStruct, p: Vec3) -> bool {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &cs.vertex_array.vertices {
        let q = localtoglobal(&cell.frame, v.position);
        lo = [lo[0].min(q.x), lo[1].min(q.y), lo[2].min(q.z)];
        hi = [hi[0].max(q.x), hi[1].max(q.y), hi[2].max(q.z)];
    }
    !(p.x < lo[0] || p.x > hi[0] || p.y < lo[1] || p.y > hi[1] || p.z < lo[2] || p.z > hi[2])
}

/// The cell of `cells` (a landblock's, by index) that point `p` (landblock meters) stands in, and
/// the height of the floor under it there: the cell whose extent holds the point and whose floor
/// is nearest under it.
#[must_use]
pub fn cell_at(
    cells: &BTreeMap<u16, EnvCell>,
    p: Vec3,
    env: &dyn Fn(DataId) -> Option<Environment>,
) -> Option<(u16, f32)> {
    let mut best: Option<(u16, f32)> = None;
    for (c, cell) in cells {
        let Some(e) = env(cell.environment) else {
            continue;
        };
        let Some(cs) = e.cells.get(usize::from(cell.cell_struct)) else {
            continue;
        };
        if !extent_holds(cell, cs, p) {
            continue;
        }
        let Some(placed) = place(cell, &e) else {
            continue;
        };
        let floor = placed
            .solid
            .iter()
            .filter_map(|q| down_hit(q, p))
            .fold(f32::MAX, f32::min);
        if floor < f32::MAX && best.is_none_or(|(_, d)| floor < p.z - d) {
            best = Some((*c, p.z - floor));
        }
    }
    best
}

/// The cell of `cells` (a landblock's, by index) that a body let go at point `p` (landblock
/// meters) comes to stand in, and the height of the floor it stands on: from the room the point
/// is in, straight down to the nearest floor, going through any opening in a floor on the way
/// into the room under it. A point over a floor of its own room lands there; a point in an open
/// shaft lands on the floor at its bottom.
///
/// Rooms are told apart as the game tells them: the room a point is in is the one whose cell tree
/// holds it (where none does, those whose extent holds it, as [`cell_at`] takes them), and the
/// room a body standing on a floor is in is the one that holds its middle, which on a ramp or the
/// line between two rooms need not be the room the floor belongs to.
#[must_use]
pub fn landing(
    cells: &BTreeMap<u16, EnvCell>,
    p: Vec3,
    env: &dyn Fn(DataId) -> Option<Environment>,
) -> Option<(u16, f32)> {
    let mut envs: BTreeMap<DataId, Option<Environment>> = BTreeMap::new();
    let mut placed: BTreeMap<u16, Placed> = BTreeMap::new();
    let mut shapes = Vec::new();
    // The rooms over the point, each with whether its cell tree holds it.
    let mut over = Vec::new();
    for (c, cell) in cells {
        let e = envs
            .entry(cell.environment)
            .or_insert_with(|| env(cell.environment));
        let Some(e) = e.as_ref() else { continue };
        let Some(cs) = e.cells.get(usize::from(cell.cell_struct)) else {
            continue;
        };
        let s = shape(*c, cell, cs);
        if extent_holds(cell, cs, p) {
            over.push((*c, s.holds(p)));
        }
        shapes.push(s);
        if let Some(at) = place(cell, e) {
            placed.insert(*c, at);
        }
    }
    // Of those rooms (those that hold it first), the floor nearest under the point.
    let land = |held: bool| {
        over.iter()
            .filter(|(_, h)| *h || !held)
            .filter_map(|(c, _)| land_below(&placed, *c, p, &mut BTreeSet::new()))
            .fold(None, nearest)
    };
    let (floor, z) = land(true).or_else(|| land(false))?;
    let middle = Vec3::new(p.x, p.y, z + STANDING_MIDDLE);
    let holds = |s: &&CellShape| s.holds(middle);
    let room = if shapes
        .iter()
        .filter(|s| s.index == floor)
        .any(|s| holds(&s))
    {
        floor
    } else {
        shapes.iter().find(holds).map_or(floor, |s| s.index)
    };
    Some((room, z))
}

/// Of a landing found so far and `next`, the one on the higher floor (the first found, of two
/// as high).
fn nearest(best: Option<(u16, f32)>, next: (u16, f32)) -> Option<(u16, f32)> {
    if best.is_none_or(|(_, b)| next.1 > b) {
        Some(next)
    } else {
        best
    }
}

/// Where straight down from `from` in cell `here` a body comes to stand: the cell whose floor it
/// meets first, through the openings on the way, and the floor's height. `None` when it meets no
/// floor, or goes out through an opening to the outdoors.
fn land_below(
    placed: &BTreeMap<u16, Placed>,
    here: u16,
    from: Vec3,
    path: &mut BTreeSet<u16>,
) -> Option<(u16, f32)> {
    let at = placed.get(&here)?;
    if !path.insert(here) {
        return None;
    }
    let solid = at
        .solid
        .iter()
        .filter_map(|q| down_hit(q, from))
        .fold(f32::MAX, f32::min);
    // Only an opening in a floor is fallen through, not a doorway in a wall; and a floor that an
    // opening meets at its edge, where the place is, is stood on.
    let openings = || at.portals.iter().filter(|(_, q)| q.n.z.abs() >= 0.5);
    let opening = openings()
        .filter_map(|(_, q)| down_hit(q, from))
        .fold(f32::MAX, f32::min);
    let found = if solid <= opening + EDGE {
        (solid < f32::MAX).then_some((here, from.z - solid))
    } else {
        openings()
            .filter_map(|(to, q)| Some(((*to)?, down_hit(q, from)?)))
            .filter(|(_, d)| *d + EDGE < solid)
            .filter_map(|(to, d)| {
                land_below(
                    placed,
                    to,
                    Vec3::new(from.x, from.y, from.z - d - 0.01),
                    path,
                )
            })
            .fold(None, nearest)
    };
    path.remove(&here);
    found
}

/// How far along a ray from `from` going `dir` (of length one) it meets `p`; `None` when it
/// misses or `p` is behind.
fn ray_hit(p: &Poly, from: Vec3, dir: Vec3) -> Option<f32> {
    let nd = p.n.dot(dir);
    if nd.abs() < 1e-6 {
        return None;
    }
    let t = p.n.dot(p.corners[0].sub(from)) / nd;
    if t < 0.0 {
        return None;
    }
    let at = from.add(Vec3::new(dir.x * t, dir.y * t, dir.z * t));
    let mut sign = 0.0f32;
    for i in 0..p.corners.len() {
        let (a, b) = (p.corners[i], p.corners[(i + 1) % p.corners.len()]);
        let e = b.sub(a);
        let w = at.sub(a);
        let c = Vec3::new(
            e.y * w.z - e.z * w.y,
            e.z * w.x - e.x * w.z,
            e.x * w.y - e.y * w.x,
        );
        let x = c.dot(p.n);
        if x.abs() < 1e-5 {
            continue;
        }
        if sign == 0.0 {
            sign = x.signum();
        } else if x.signum() != sign {
            return None;
        }
    }
    Some(t)
}

/// Where a ray from `from` going `dir` (landblock meters, `dir` of length one) first meets a
/// solid polygon of one of `cells`: the cell and the point.
#[must_use]
pub fn cell_hit(
    cells: &BTreeMap<u16, EnvCell>,
    from: Vec3,
    dir: Vec3,
    env: &dyn Fn(DataId) -> Option<Environment>,
) -> Option<(u16, Vec3)> {
    cell_hit_facing(cells, from, dir, env).map(|(c, p, _)| (c, p))
}

/// [`cell_hit`], meeting a polygon only from its cell's inside, as the world draws it (a floor
/// from above, a ceiling from below; a cell seen from outside shows its far walls): the cell, the
/// point, and the way the polygon faces into its cell.
#[must_use]
pub fn cell_hit_facing(
    cells: &BTreeMap<u16, EnvCell>,
    from: Vec3,
    dir: Vec3,
    env: &dyn Fn(DataId) -> Option<Environment>,
) -> Option<(u16, Vec3, Vec3)> {
    let mut best: Option<(u16, f32, Vec3)> = None;
    for (c, cell) in cells {
        let Some(placed) = env(cell.environment).and_then(|e| place(cell, &e)) else {
            continue;
        };
        let middle = placed.inner[0];
        for p in &placed.solid {
            let inward = if p.n.dot(middle.sub(p.corners[0])) < 0.0 {
                Vec3::new(-p.n.x, -p.n.y, -p.n.z)
            } else {
                p.n
            };
            if inward.dot(dir) >= 0.0 {
                continue;
            }
            if let Some(t) = ray_hit(p, from, dir) {
                if best.is_none_or(|(_, b, _)| t < b) {
                    best = Some((*c, t, inward));
                }
            }
        }
    }
    best.map(|(c, t, n)| (c, from.add(Vec3::new(dir.x * t, dir.y * t, dir.z * t)), n))
}
