//! The region's walkable floor as convex polygons: the navigation mesh a route is searched over.
//!
//! The polygons are built from the region's standing places a tile of grid columns at a time
//! ([`POLY_TILE`] a side), so no one step of the build costs much and no polygon is wider than a
//! tile:
//!
//! 1. **Runs.** Each row of the tile's columns is cut into runs: places side by side on one floor,
//!    each a step from the next, of one kind (terrain or a stored floor) and either all wading or
//!    none, and each alone on its floor in its column.
//! 2. **Floors.** The runs are stacked, row on row, into floors: a run carries on the floor of the
//!    run below it when it is the only run joined to it and every column the two share is joined.
//!    A floor is then one run a row over rows in a row, so it has no holes, and a body steps from
//!    each of its places to each of its neighbors in it.
//! 3. **Outline.** Each floor's outline is traced round the edges of its columns, each edge told
//!    by what is across it: another floor of the tile a body steps onto, a floor of the next tile,
//!    or nothing a body steps onto (a wall, a drop, the edge of the region).
//! 4. **Simplified.** The stretches of outline along nothing are straightened where the straight
//!    line keeps inside the floor's columns: never outward, so a polygon covers only places a body
//!    stands on, and the places along a slanting wall its straight edge leaves out are searched
//!    one by one.
//! 5. **Polygons.** The outline is cut into triangles and the triangles merged into convex
//!    polygons of at most [`MAX_CORNERS`] corners.
//! 6. **Links.** Each polygon is linked to those it shares an edge with: inside its floor by the
//!    edge they share, to another floor or a neighboring tile by the part of an edge both hold.
//!
//! Corners are in **grid corners**: corner `(cx, cy)` is the south-west corner of column
//! `(cx, cy)`, half a spacing west and south of its place.

use std::collections::HashMap;

use dereth_primitives::Vec3;

use crate::region::{Region, GROUND};

/// The side of a tile of grid columns the polygons are built in, in columns.
pub(crate) const POLY_TILE: i32 = 64;

/// The most corners a polygon has.
pub(crate) const MAX_CORNERS: usize = 8;

/// No polygon: a place searched by itself.
pub(crate) const NO_POLY: u32 = u32::MAX;

/// How far a straightened outline may leave the corners it replaces, in grid columns: never out
/// of the floor, and only this far into it.
const SIMPLIFY_ERROR: f64 = 1.5;

/// A convex polygon of the mesh.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NavPoly {
    /// Its corners, counterclockwise seen from above, in grid corners.
    pub corners: Box<[(i32, i32)]>,
    /// The polygons it leads into and the part of its edge each is crossed by.
    pub links: Box<[Link]>,
    /// The terrain's places, rather than stored ones.
    pub ground: bool,
    /// Terrain the body wades at.
    pub wading: bool,
    /// The tile it was built in.
    pub tile: (i32, i32),
    /// Its height, about: the mean of its places'.
    pub z: f32,
}

/// One way from a polygon into another: the polygon, and the part of the edge between them, in
/// grid corners, from `a` to `b` as the polygon's own edge runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Link {
    pub to: u32,
    pub a: (i32, i32),
    pub b: (i32, i32),
}

impl NavPoly {
    /// Whether the point `(x, y)`, in grid corners, is inside or on the edge.
    pub(crate) fn contains(&self, x: f64, y: f64) -> bool {
        let n = self.corners.len();
        for i in 0..n {
            let (ax, ay) = self.corners[i];
            let (bx, by) = self.corners[(i + 1) % n];
            let cross = (f64::from(bx) - f64::from(ax)) * (y - f64::from(ay))
                - (f64::from(by) - f64::from(ay)) * (x - f64::from(ax));
            if cross < -1e-9 {
                return false;
            }
        }
        true
    }

    /// Whether the center of column `(ix, iy)` is inside or on the edge.
    pub(crate) fn holds(&self, ix: i32, iy: i32) -> bool {
        self.contains(f64::from(ix) + 0.5, f64::from(iy) + 0.5)
    }

    /// The columns of its bounds, inclusive: `(x0, y0, x1, y1)`.
    pub(crate) fn bounds(&self) -> (i32, i32, i32, i32) {
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for &(x, y) in &*self.corners {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        (x0, y0, x1 - 1, y1 - 1)
    }

    /// The part of the segment from `pa` to `pb` (grid corners) inside the polygon, from its
    /// start: the share of the segment up to where it leaves, or `None` when it starts outside.
    pub(crate) fn leave(&self, pa: (f64, f64), pb: (f64, f64)) -> Option<f64> {
        let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
        let d = (pb.0 - pa.0, pb.1 - pa.1);
        let n = self.corners.len();
        for i in 0..n {
            let (ax, ay) = self.corners[i];
            let (bx, by) = self.corners[(i + 1) % n];
            let e = (f64::from(bx) - f64::from(ax), f64::from(by) - f64::from(ay));
            // Inside is to the left: cross(e, p - a) >= 0.
            let num = e.0 * (pa.1 - f64::from(ay)) - e.1 * (pa.0 - f64::from(ax));
            let den = e.0 * d.1 - e.1 * d.0;
            if den.abs() < 1e-12 {
                if num < -1e-9 {
                    return None;
                }
                continue;
            }
            let t = -num / den;
            if den > 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 + 1e-9 {
                return None;
            }
        }
        (t0 <= 1e-9).then_some(t1)
    }

    /// The bytes it holds.
    pub(crate) fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.corners.len() * std::mem::size_of::<(i32, i32)>()
            + self.links.len() * std::mem::size_of::<Link>()
    }
}

/// A place of a tile being built: its id, height, whether the body wades there, and whether it is
/// the terrain's.
#[derive(Debug, Clone, Copy)]
struct Spot {
    id: u32,
    z: f32,
    wet: bool,
    ground: bool,
}

/// What is across one edge of a floor's column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Across {
    /// Nothing a body steps onto.
    Wall,
    /// Another floor of this tile.
    Floor(u32),
    /// A floor of the neighboring tile.
    Border,
}

/// One run of a row: its columns `x0..=x1` and its places, west to east (each with its number
/// among the tile's places), and its floor.
#[derive(Debug, Clone)]
struct Run {
    y: i32,
    x0: i32,
    spots: Vec<Spot>,
    flat: Vec<u32>,
    floor: u32,
}

impl Run {
    fn x1(&self) -> i32 {
        self.x0 + i32::try_from(self.spots.len()).unwrap_or(1) - 1
    }

    fn at(&self, x: i32) -> Option<&Spot> {
        usize::try_from(x - self.x0)
            .ok()
            .and_then(|i| self.spots.get(i))
    }
}

/// The polygons of one tile, built: their places' polygons (stored places by id, terrain places
/// by their polygon's corners), and the polygons, numbered from `first`.
#[derive(Debug, Default)]
pub(crate) struct TilePolys {
    pub polys: Vec<NavPoly>,
    /// Each stored place of a polygon, and the polygon.
    pub stored: Vec<(u32, u32)>,
    /// The tile's clear places no polygon holds, searched by themselves.
    pub loose: Vec<u32>,
}

/// The join rules of the region's floors: neighbors a step apart at most, and another floor in
/// a column at least `apart` away.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Joins {
    pub join: f32,
    pub apart: f32,
}

impl Region {
    /// The places of column `(ix, iy)` a polygon may hold, added to `out`: clear and alone on
    /// their floor in the column, lowest first; the clear places that are not added to `loose`.
    /// `scratch` is room to work in.
    fn spots(
        &self,
        (ix, iy): (i32, i32),
        joins: Joins,
        out: &mut Vec<Spot>,
        loose: &mut Vec<u32>,
        scratch: &mut Vec<(u32, f32)>,
    ) {
        scratch.clear();
        scratch.extend(
            self.column(ix, iy)
                .filter(|&j| self.is_clear(j))
                .map(|j| (j, self.column_and_z(j).2)),
        );
        let from = out.len();
        for &(j, z) in scratch.iter() {
            let alone = !scratch
                .iter()
                .any(|&(k, zk)| k != j && (zk - z).abs() <= joins.apart);
            if alone {
                out.push(Spot {
                    id: j,
                    z,
                    wet: self.is_wading(j),
                    ground: j & GROUND != 0,
                });
            } else {
                loose.push(j);
            }
        }
        out[from..].sort_by(|a, b| a.z.total_cmp(&b.z));
    }

    /// Whether a body steps between two places of neighboring columns: a step apart at most.
    fn steps_between(a: &Spot, b: &Spot, joins: Joins) -> bool {
        (a.z - b.z).abs() <= joins.join
    }

    /// Whether two neighboring places can be of one floor: a step apart, of one kind, and both
    /// wading or neither.
    fn same_floor(a: &Spot, b: &Spot, joins: Joins) -> bool {
        Self::steps_between(a, b, joins) && a.wet == b.wet && a.ground == b.ground
    }

    /// Build the polygons of tile `(tx, ty)`, numbering them from `first`.
    pub(crate) fn build_tile_polys(
        &self,
        (tx, ty): (i32, i32),
        first: u32,
        joins: Joins,
    ) -> TilePolys {
        let (bx, by) = (tx * POLY_TILE, ty * POLY_TILE);
        // Every column's places, in one list, each column's from its start.
        let mut out = TilePolys::default();
        let area = (POLY_TILE * POLY_TILE) as usize;
        let mut flat: Vec<Spot> = Vec::with_capacity(area);
        let mut starts: Vec<u32> = Vec::with_capacity(area + 1);
        let mut scratch = Vec::new();
        for ly in 0..POLY_TILE {
            for lx in 0..POLY_TILE {
                starts.push(u32::try_from(flat.len()).unwrap_or(u32::MAX));
                self.spots(
                    (bx + lx, by + ly),
                    joins,
                    &mut flat,
                    &mut out.loose,
                    &mut scratch,
                );
            }
        }
        starts.push(u32::try_from(flat.len()).unwrap_or(u32::MAX));
        // A column's places and the number of the first.
        let col = |x: i32, y: i32| -> (u32, &[Spot]) {
            if x < bx || y < by || x >= bx + POLY_TILE || y >= by + POLY_TILE {
                return (0, &[]);
            }
            let c = usize::try_from((y - by) * POLY_TILE + (x - bx)).unwrap_or(0);
            let (a, b) = (starts[c], starts[c + 1]);
            (a, &flat[a as usize..b as usize])
        };
        // 1. Runs, row by row.
        let mut taken = vec![false; flat.len()];
        let mut rows: Vec<Vec<Run>> = Vec::with_capacity(POLY_TILE as usize);
        for ly in 0..POLY_TILE {
            let y = by + ly;
            let mut row = Vec::new();
            for lx in 0..POLY_TILE {
                let x = bx + lx;
                let (base, here) = col(x, y);
                for (k, s) in here.iter().enumerate() {
                    let at = base + u32::try_from(k).unwrap_or(0);
                    if taken[at as usize] {
                        continue;
                    }
                    taken[at as usize] = true;
                    let mut spots = vec![*s];
                    let mut idx = vec![at];
                    let mut nx = x + 1;
                    while nx < bx + POLY_TILE {
                        let last = spots[spots.len() - 1];
                        let (nbase, next_col) = col(nx, y);
                        let Some((k2, next)) = next_col.iter().enumerate().find(|&(k2, n)| {
                            !taken[(nbase as usize) + k2] && Self::same_floor(&last, n, joins)
                        }) else {
                            break;
                        };
                        let at2 = nbase + u32::try_from(k2).unwrap_or(0);
                        taken[at2 as usize] = true;
                        spots.push(*next);
                        idx.push(at2);
                        nx += 1;
                    }
                    row.push(Run {
                        y,
                        x0: x,
                        spots,
                        flat: idx,
                        floor: u32::MAX,
                    });
                }
            }
            row.sort_by_key(|r| r.x0);
            rows.push(row);
        }
        // 2. Floors: a run carries on the floor below when it is the only run joined to that
        // run and that run the only one joined to it, and every column they share is joined.
        let mut floors: Vec<Vec<(usize, usize)>> = Vec::new();
        for ly in 0..rows.len() {
            let (done, rest) = rows.split_at_mut(ly);
            let below: &[Run] = done.last().map_or(&[], Vec::as_slice);
            let here = &mut rest[0];
            let n = here.len();
            // Per run here: the runs below it is joined to at a shared column, and whether every
            // shared column with each is joined.
            let mut joined: Vec<Vec<(usize, bool)>> = vec![Vec::new(); n];
            let mut count = vec![0usize; below.len()];
            for (i, r) in here.iter().enumerate() {
                for (k, b) in below.iter().enumerate() {
                    let (lo, hi) = (r.x0.max(b.x0), r.x1().min(b.x1()));
                    if lo > hi {
                        continue;
                    }
                    let mut any = false;
                    let mut all = true;
                    for x in lo..=hi {
                        let (Some(p), Some(q)) = (r.at(x), b.at(x)) else {
                            all = false;
                            continue;
                        };
                        if Self::same_floor(p, q, joins) {
                            any = true;
                        } else {
                            all = false;
                        }
                    }
                    if any {
                        joined[i].push((k, all));
                        count[k] += 1;
                    }
                }
            }
            for i in 0..n {
                let carry = match joined[i].as_slice() {
                    [(k, true)] if count[*k] == 1 => Some(below[*k].floor),
                    _ => None,
                };
                let f = carry.unwrap_or_else(|| {
                    floors.push(Vec::new());
                    u32::try_from(floors.len() - 1).unwrap_or(u32::MAX)
                });
                here[i].floor = f;
                floors[f as usize].push((ly, i));
            }
        }
        // The floor of each of the tile's places.
        let mut floor_of = vec![u32::MAX; flat.len()];
        for row in &rows {
            for r in row {
                for &i in &r.flat {
                    floor_of[i as usize] = r.floor;
                }
            }
        }
        // What is across an edge from place `s` in column `(x, y)` to column `(nx, ny)`.
        let across = |s: &Spot, nx: i32, ny: i32| -> Across {
            let inside = nx >= bx && ny >= by && nx < bx + POLY_TILE && ny < by + POLY_TILE;
            if inside {
                let (base, there) = col(nx, ny);
                return there
                    .iter()
                    .position(|n| Self::steps_between(s, n, joins))
                    .map(|k| floor_of[base as usize + k])
                    .filter(|&f| f != u32::MAX)
                    .map_or(Across::Wall, Across::Floor);
            }
            // Beyond the region's own terrain the edge is a landblock's, whose neighbor is built
            // apart and stitched to it: kept straight, as an edge between tiles is.
            let beyond = s.ground
                && self
                    .ground
                    .as_ref()
                    .is_some_and(|g| g.index(nx, ny).is_none());
            let outside = beyond
                || self.column(nx, ny).any(|j| {
                    self.is_clear(j) && (self.column_and_z(j).2 - s.z).abs() <= joins.join
                });
            if outside {
                Across::Border
            } else {
                Across::Wall
            }
        };
        for (f, members) in floors.iter().enumerate() {
            let f = u32::try_from(f).unwrap_or(u32::MAX);
            let runs: Vec<&Run> = members.iter().map(|&(ly, i)| &rows[ly][i]).collect();
            let outline = outline(&runs, &|s, nx, ny| match across(s, nx, ny) {
                Across::Floor(g) if g == f => Across::Wall,
                a => a,
            });
            let inside = |x: i32, y: i32| -> bool {
                runs.iter().any(|r| r.y == y && x >= r.x0 && x <= r.x1())
            };
            let corners = simplify(&outline, &inside);
            let polys = checked_polygons(&corners)
                .or_else(|| checked_polygons(&outline.iter().map(|e| e.0).collect::<Vec<_>>()))
                .unwrap_or_else(|| runs.iter().map(|r| rectangle(r)).collect());
            let first_here = first + u32::try_from(out.polys.len()).unwrap_or(0);
            let spots: Vec<&Spot> = runs.iter().flat_map(|r| r.spots.iter()).collect();
            let ground = spots.first().is_some_and(|s| s.ground);
            let wading = spots.first().is_some_and(|s| s.wet);
            // Each place's polygon: the first holding its column's center.
            let mut sums = vec![(0.0_f64, 0usize); polys.len()];
            for r in &runs {
                for (k, s) in r.spots.iter().enumerate() {
                    let x = r.x0 + i32::try_from(k).unwrap_or(0);
                    if let Some(p) = polys.iter().position(|c| holds(c, x, r.y)) {
                        sums[p].0 += f64::from(s.z);
                        sums[p].1 += 1;
                        if !s.ground {
                            out.stored
                                .push((s.id, first_here + u32::try_from(p).unwrap_or(0)));
                        }
                    } else {
                        out.loose.push(s.id);
                    }
                }
            }
            for (p, c) in polys.into_iter().enumerate() {
                let (sum, n) = sums[p];
                #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
                let z = if n > 0 {
                    (sum / n as f64) as f32
                } else {
                    // A polygon too thin to hold a column's center: its floor's height nearest
                    // its middle.
                    #[allow(clippy::cast_precision_loss)]
                    let k = c.len() as f64;
                    let (mx, my) = c.iter().fold((0.0, 0.0), |(a, b), &(x, y)| {
                        (a + f64::from(x) / k, b + f64::from(y) / k)
                    });
                    runs.iter()
                        .flat_map(|r| {
                            r.spots.iter().enumerate().map(move |(i, s)| {
                                let x = f64::from(r.x0)
                                    + f64::from(u32::try_from(i).unwrap_or(0))
                                    + 0.5;
                                (
                                    dereth_primitives::num::math::hypot(
                                        x - mx,
                                        f64::from(r.y) + 0.5 - my,
                                    ),
                                    s.z,
                                )
                            })
                        })
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                        .map_or(0.0, |(_, z)| z)
                };
                out.polys.push(NavPoly {
                    corners: c.into_boxed_slice(),
                    links: Box::new([]),
                    ground,
                    wading,
                    tile: (tx, ty),
                    z,
                });
            }
        }
        out
    }
}

/// Whether the center of column `(x, y)` is inside the polygon `c` (grid corners, counterclockwise)
/// or on its edge.
fn holds(c: &[(i32, i32)], x: i32, y: i32) -> bool {
    let (px, py) = (2 * i64::from(x) + 1, 2 * i64::from(y) + 1);
    let n = c.len();
    (0..n).all(|i| {
        let (ax, ay) = (2 * i64::from(c[i].0), 2 * i64::from(c[i].1));
        let (bx, by) = (
            2 * i64::from(c[(i + 1) % n].0),
            2 * i64::from(c[(i + 1) % n].1),
        );
        (bx - ax) * (py - ay) - (by - ay) * (px - ax) >= 0
    })
}

/// A run's columns as a rectangle, counterclockwise.
fn rectangle(r: &Run) -> Vec<(i32, i32)> {
    let x1 = r.x1() + 1;
    vec![(r.x0, r.y), (x1, r.y), (x1, r.y + 1), (r.x0, r.y + 1)]
}

/// One edge of an outline: where it starts, and what is across it.
type Edge = ((i32, i32), Across);

/// The outline of a floor of `runs` (one a row, rows in a row, south to north), counterclockwise,
/// one edge per column side: each edge's start and what `across` says is over it.
fn outline(runs: &[&Run], across: &dyn Fn(&Spot, i32, i32) -> Across) -> Vec<Edge> {
    let mut runs: Vec<&Run> = runs.to_vec();
    runs.sort_by_key(|r| r.y);
    let mut out: Vec<Edge> = Vec::new();
    let bottom = |r: &Run, x: i32, out: &mut Vec<Edge>| {
        if let Some(s) = r.at(x) {
            out.push(((x, r.y), across(s, x, r.y - 1)));
        }
    };
    let top = |r: &Run, x: i32, out: &mut Vec<Edge>| {
        if let Some(s) = r.at(x) {
            out.push(((x + 1, r.y + 1), across(s, x, r.y + 1)));
        }
    };
    let n = runs.len();
    // South side, west to east.
    let first = runs[0];
    for x in first.x0..=first.x1() {
        bottom(first, x, &mut out);
    }
    // East side, south to north.
    for k in 0..n {
        let r = runs[k];
        let x = r.x1();
        if let Some(s) = r.at(x) {
            out.push(((x + 1, r.y), across(s, x + 1, r.y)));
        }
        if k + 1 < n {
            let up = runs[k + 1];
            if up.x1() > r.x1() {
                for x in r.x1() + 1..=up.x1() {
                    bottom(up, x, &mut out);
                }
            } else {
                for x in (up.x1() + 1..=r.x1()).rev() {
                    top(r, x, &mut out);
                }
            }
        }
    }
    // North side, east to west.
    let last = runs[n - 1];
    for x in (last.x0..=last.x1()).rev() {
        top(last, x, &mut out);
    }
    // West side, north to south.
    for k in (0..n).rev() {
        let r = runs[k];
        if let Some(s) = r.at(r.x0) {
            out.push(((r.x0, r.y + 1), across(s, r.x0 - 1, r.y)));
        }
        if k > 0 {
            let down = runs[k - 1];
            if down.x0 < r.x0 {
                for x in (down.x0..r.x0).rev() {
                    top(down, x, &mut out);
                }
            } else {
                for x in r.x0..down.x0 {
                    bottom(r, x, &mut out);
                }
            }
        }
    }
    out
}

/// The outline's corners once straightened: corners kept where the outline turns or what is
/// across it changes; the stretches along walls straightened where the straight line keeps inside
/// (`inside` says which columns are the floor's) and strays no more than [`SIMPLIFY_ERROR`].
fn simplify(edges: &[Edge], inside: &dyn Fn(i32, i32) -> bool) -> Vec<(i32, i32)> {
    let n = edges.len();
    if n < 3 {
        return edges.iter().map(|e| e.0).collect();
    }
    let end = |i: usize| edges[(i + 1) % n].0;
    // Where each edge runs, and the corners that must stay: a turn, or a change of what is across.
    let dir = |i: usize| {
        let (a, b) = (edges[i].0, end(i));
        ((b.0 - a.0).signum(), (b.1 - a.1).signum())
    };
    let mut keep: Vec<usize> = Vec::new();
    for i in 0..n {
        let prev = (i + n - 1) % n;
        let turns = dir(prev) != dir(i);
        let changes = edges[prev].1 != edges[i].1;
        let wall_both = edges[prev].1 == Across::Wall && edges[i].1 == Across::Wall;
        if changes || (turns && !wall_both) {
            keep.push(i);
        }
    }
    if keep.is_empty() {
        // All wall: keep its turns, at least the bottom-left corner.
        keep = (0..n).filter(|&i| dir((i + n - 1) % n) != dir(i)).collect();
    }
    if keep.is_empty() {
        return edges.iter().map(|e| e.0).collect();
    }
    let mut corners: Vec<(i32, i32)> = Vec::new();
    for (k, &i) in keep.iter().enumerate() {
        let j = keep[(k + 1) % keep.len()];
        // The stretch from corner i to corner j (exclusive), all of one kind.
        let mut chain: Vec<(i32, i32)> = Vec::new();
        let mut at = i;
        loop {
            chain.push(edges[at].0);
            at = (at + 1) % n;
            if at == j {
                break;
            }
        }
        chain.push(edges[j].0);
        let pts = if edges[i].1 == Across::Wall {
            straighten(&chain, inside)
        } else {
            turns_only(&chain)
        };
        corners.extend_from_slice(&pts[..pts.len() - 1]);
    }
    corners
}

/// A stretch of outline's corners where it turns, its ends kept.
fn turns_only(chain: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut out = vec![chain[0]];
    for w in chain.windows(3) {
        let d1 = ((w[1].0 - w[0].0).signum(), (w[1].1 - w[0].1).signum());
        let d2 = ((w[2].0 - w[1].0).signum(), (w[2].1 - w[1].1).signum());
        if d1 != d2 {
            out.push(w[1]);
        }
    }
    out.push(chain[chain.len() - 1]);
    out
}

/// A wall's stretch of outline straightened, its ends kept: Douglas and Peucker's, a straight line
/// taken only where it keeps inside the floor.
fn straighten(chain: &[(i32, i32)], inside: &dyn Fn(i32, i32) -> bool) -> Vec<(i32, i32)> {
    let base = turns_only(chain);
    if base.len() <= 2 {
        return base;
    }
    let mut keep = vec![false; base.len()];
    keep[0] = true;
    keep[base.len() - 1] = true;
    let mut stack = vec![(0usize, base.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (pa, pb) = (base[a], base[b]);
        let mut far = (0.0_f64, a);
        for (k, &p) in base.iter().enumerate().take(b).skip(a + 1) {
            let d = distance_to_line(p, pa, pb);
            if d > far.0 {
                far = (d, k);
            }
        }
        if far.0 > SIMPLIFY_ERROR || !keeps_inside(pa, pb, inside) {
            keep[far.1.max(a + 1)] = true;
            stack.push((a, far.1.max(a + 1)));
            stack.push((far.1.max(a + 1), b));
        }
    }
    base.into_iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(p))
        .collect()
}

/// How far `p` is from the line through `a` and `b`, in grid columns.
fn distance_to_line(p: (i32, i32), a: (i32, i32), b: (i32, i32)) -> f64 {
    let (dx, dy) = (f64::from(b.0 - a.0), f64::from(b.1 - a.1));
    let len = dereth_primitives::num::math::hypot(dx, dy);
    if len == 0.0 {
        return dereth_primitives::num::math::hypot(f64::from(p.0 - a.0), f64::from(p.1 - a.1));
    }
    (dx * f64::from(p.1 - a.1) - dy * f64::from(p.0 - a.0)).abs() / len
}

/// Whether the straight line from `a` to `b` (an outline running counterclockwise, the floor on its
/// left) keeps inside the floor's columns all the way, a little to its left of it.
fn keeps_inside(a: (i32, i32), b: (i32, i32), inside: &dyn Fn(i32, i32) -> bool) -> bool {
    let (dx, dy) = (f64::from(b.0 - a.0), f64::from(b.1 - a.1));
    let len = dereth_primitives::num::math::hypot(dx, dy);
    if len == 0.0 {
        return true;
    }
    let (lx, ly) = (-dy / len * 0.05, dx / len * 0.05);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (len * 8.0).ceil() as usize + 1;
    #[allow(clippy::cast_possible_truncation)]
    let at = |x: f64, y: f64| {
        inside(
            dereth_primitives::num::to_i32_f64(x.floor()),
            dereth_primitives::num::to_i32_f64(y.floor()),
        )
    };
    // The ends lie on the outline; the points between keep inside.
    (1..n).all(|k| {
        #[allow(clippy::cast_precision_loss)]
        let t = k as f64 / n as f64;
        let (x, y) = (f64::from(a.0) + dx * t, f64::from(a.1) + dy * t);
        at(x + lx, y + ly)
    })
}

/// Twice the area of an outline, counterclockwise positive.
fn area2(c: &[(i32, i32)]) -> i64 {
    (0..c.len())
        .map(|i| {
            let (a, b) = (c[i], c[(i + 1) % c.len()]);
            i64::from(a.0) * i64::from(b.1) - i64::from(b.0) * i64::from(a.1)
        })
        .sum()
}

/// `outline` cut into convex polygons ([`convex_polygons`]) only when they cover exactly its
/// area, every one counterclockwise.
fn checked_polygons(outline: &[(i32, i32)]) -> Option<Vec<Vec<(i32, i32)>>> {
    let polys = convex_polygons(outline)?;
    let whole = area2(outline);
    let parts: i64 = polys.iter().map(|p| area2(p)).sum();
    (whole > 0 && parts == whole && polys.iter().all(|p| area2(p) > 0)).then_some(polys)
}

/// Twice the signed area of the triangle `a b c`: positive when it turns counterclockwise.
fn cross(a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> i64 {
    (i64::from(b.0) - i64::from(a.0)) * (i64::from(c.1) - i64::from(a.1))
        - (i64::from(b.1) - i64::from(a.1)) * (i64::from(c.0) - i64::from(a.0))
}

/// Whether `p` is inside the triangle `a b c` (counterclockwise), its edges included.
fn in_triangle(p: (i32, i32), a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> bool {
    cross(a, b, p) >= 0 && cross(b, c, p) >= 0 && cross(c, a, p) >= 0
}

/// An outline (counterclockwise, simple) cut into convex polygons of at most [`MAX_CORNERS`]
/// corners: cut into triangles by ears, then the triangles merged, the longest shared edge
/// first. `None` when the outline cannot be cut.
fn convex_polygons(outline: &[(i32, i32)]) -> Option<Vec<Vec<(i32, i32)>>> {
    // Corners in a straight line add nothing.
    let mut pts: Vec<(i32, i32)> = outline.to_vec();
    pts.dedup();
    if pts.len() > 1 && pts[0] == pts[pts.len() - 1] {
        pts.pop();
    }
    let mut changed = true;
    while changed && pts.len() > 3 {
        changed = false;
        let n = pts.len();
        for i in 0..n {
            let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            if cross(a, b, c) == 0 {
                pts.remove(i);
                changed = true;
                break;
            }
        }
    }
    if pts.len() < 3 {
        return None;
    }
    let area: i64 = (0..pts.len())
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            i64::from(a.0) * i64::from(b.1) - i64::from(b.0) * i64::from(a.1)
        })
        .sum();
    if area <= 0 {
        return None;
    }
    // Ears.
    let mut idx: Vec<usize> = (0..pts.len()).collect();
    let mut tris: Vec<Vec<(i32, i32)>> = Vec::new();
    let mut guard = 0;
    while idx.len() > 3 {
        guard += 1;
        if guard > 10 * pts.len() * pts.len() + 100 {
            return None;
        }
        let n = idx.len();
        // The ear whose cut is shortest: long thin triangles left for last.
        let mut cut: Option<(i64, usize)> = None;
        for k in 0..n {
            let (ia, ib, ic) = (idx[(k + n - 1) % n], idx[k], idx[(k + 1) % n]);
            let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
            if cross(a, b, c) <= 0 {
                continue;
            }
            let blocked = idx.iter().any(|&j| {
                j != ia
                    && j != ib
                    && j != ic
                    && in_triangle(pts[j], a, b, c)
                    && pts[j] != a
                    && pts[j] != b
                    && pts[j] != c
            });
            if !blocked {
                let d = (i64::from(c.0) - i64::from(a.0)).pow(2)
                    + (i64::from(c.1) - i64::from(a.1)).pow(2);
                if cut.is_none_or(|(best, _)| d < best) {
                    cut = Some((d, k));
                }
            }
        }
        let (_, k) = cut?;
        let n = idx.len();
        tris.push(vec![
            pts[idx[(k + n - 1) % n]],
            pts[idx[k]],
            pts[idx[(k + 1) % n]],
        ]);
        idx.remove(k);
    }
    tris.push(idx.iter().map(|&i| pts[i]).collect());
    // Merge.
    loop {
        let mut best: Option<Merge> = None;
        for i in 0..tris.len() {
            for j in i + 1..tris.len() {
                if let Some((len, merged)) = merge(&tris[i], &tris[j]) {
                    if best.as_ref().is_none_or(|b| len > b.0) {
                        best = Some((len, i, j, merged));
                    }
                }
            }
        }
        let Some((_, i, j, merged)) = best else { break };
        tris[i] = merged;
        tris.remove(j);
    }
    Some(tris)
}

/// The best merge found: the shared edge's squared length, the two polygons and the merge.
type Merge = (i64, usize, usize, Vec<(i32, i32)>);

/// Two convex polygons sharing an edge, merged when the merge is convex and has at most
/// [`MAX_CORNERS`] corners: the shared edge's squared length and the merge.
fn merge(p: &[(i32, i32)], q: &[(i32, i32)]) -> Option<(i64, Vec<(i32, i32)>)> {
    let (np, nq) = (p.len(), q.len());
    if np + nq - 2 > MAX_CORNERS {
        return None;
    }
    for i in 0..np {
        let (a, b) = (p[i], p[(i + 1) % np]);
        for j in 0..nq {
            if q[j] == b && q[(j + 1) % nq] == a {
                // p from b round to a, then q from a round to b, leaving the shared edge out.
                let mut out = Vec::with_capacity(np + nq - 2);
                for k in 0..np {
                    out.push(p[(i + 1 + k) % np]);
                }
                for k in 0..nq - 2 {
                    out.push(q[(j + 2 + k) % nq]);
                }
                // Straight corners left out, then convex all round.
                let mut pts = out;
                let mut changed = true;
                while changed && pts.len() > 3 {
                    changed = false;
                    let n = pts.len();
                    for k in 0..n {
                        if cross(pts[(k + n - 1) % n], pts[k], pts[(k + 1) % n]) == 0 {
                            pts.remove(k);
                            changed = true;
                            break;
                        }
                    }
                }
                let n = pts.len();
                if (0..n).all(|k| cross(pts[(k + n - 1) % n], pts[k], pts[(k + 1) % n]) > 0) {
                    let len = (i64::from(b.0 - a.0)).pow(2) + (i64::from(b.1 - a.1)).pow(2);
                    return Some((len, pts));
                }
                return None;
            }
        }
    }
    None
}

/// The line an edge lies along, the same for both directions: its direction reduced and made
/// to point east (or north), and where it crosses.
fn line_key(a: (i32, i32), b: (i32, i32)) -> (i32, i32, i64) {
    let (mut dx, mut dy) = (b.0 - a.0, b.1 - a.1);
    let g = gcd(dx.unsigned_abs(), dy.unsigned_abs()).max(1);
    let g = i32::try_from(g).unwrap_or(1);
    dx /= g;
    dy /= g;
    if dx < 0 || (dx == 0 && dy < 0) {
        dx = -dx;
        dy = -dy;
    }
    (
        dx,
        dy,
        i64::from(dy) * i64::from(a.0) - i64::from(dx) * i64::from(a.1),
    )
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// The pairs of `near` (polygons by number) that hold edges along one line, at least one of the
/// two numbered `first` or after.
pub(crate) fn candidate_pairs(polys: &[NavPoly], near: &[u32], first: u32) -> Vec<(u32, u32)> {
    let mut by_line: HashMap<(i32, i32, i64), Vec<u32>> = HashMap::new();
    for &p in near {
        for (a, b) in edges(&polys[p as usize].corners) {
            let v = by_line.entry(line_key(a, b)).or_default();
            if v.last() != Some(&p) {
                v.push(p);
            }
        }
    }
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    for list in by_line.values() {
        for (k, &p) in list.iter().enumerate() {
            for &q in &list[k + 1..] {
                if p != q && (p >= first || q >= first) {
                    pairs.push((p.min(q), p.max(q)));
                }
            }
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    pairs
}

/// The links of each pair of `pairs`: where an edge of one and an edge of the other run the
/// opposite ways along one line, the part both hold, each way, where `steps(p, q, m)` says a body
/// steps from polygon `p` into polygon `q` across the middle `m` of that part (grid corners).
pub(crate) fn links_between(
    polys: &[NavPoly],
    pairs: &[(u32, u32)],
    steps: &dyn Fn(u32, u32, (f64, f64)) -> bool,
) -> Vec<(u32, Link)> {
    let mut found: Vec<(u32, Link)> = Vec::new();
    for &(p, q) in pairs {
        let (pp, qq) = (&polys[p as usize], &polys[q as usize]);
        for (a, b) in edges(&pp.corners) {
            for (c, d) in edges(&qq.corners) {
                let Some((s, e)) = overlap((a, b), (d, c)) else {
                    continue;
                };
                // The step is checked at the middle of each stretch between grid points along
                // the shared part, and each run of stretches that pass is a link of its own: a
                // link never spans a stretch whose step fails.
                for (ra, rb) in passing_runs(s, e, &|m| steps(p, q, m)) {
                    found.push((
                        p,
                        Link {
                            to: q,
                            a: ra,
                            b: rb,
                        },
                    ));
                    found.push((
                        q,
                        Link {
                            to: p,
                            a: rb,
                            b: ra,
                        },
                    ));
                }
            }
        }
    }
    found
}

/// The runs of segment `s`..`e` (grid corners) whose stretches between grid points pass `ok` at
/// their middles, each as its two ends.
fn passing_runs(
    s: (i32, i32),
    e: (i32, i32),
    ok: &dyn Fn((f64, f64)) -> bool,
) -> Vec<((i32, i32), (i32, i32))> {
    let (dx, dy) = (e.0 - s.0, e.1 - s.1);
    let g = gcd(dx.unsigned_abs(), dy.unsigned_abs()).max(1);
    let g = i32::try_from(g).unwrap_or(1);
    let (ux, uy) = (dx / g, dy / g);
    let at = |k: i32| (s.0 + ux * k, s.1 + uy * k);
    let mut runs = Vec::new();
    let mut from: Option<i32> = None;
    for k in 0..g {
        let mid = (
            f64::from(s.0) + f64::from(ux) * (f64::from(k) + 0.5),
            f64::from(s.1) + f64::from(uy) * (f64::from(k) + 0.5),
        );
        if ok(mid) {
            from.get_or_insert(k);
        } else if let Some(f) = from.take() {
            runs.push((at(f), at(k)));
        }
    }
    if let Some(f) = from {
        runs.push((at(f), e));
    }
    runs
}

/// A polygon's edges, each from its corner to the next.
fn edges(c: &[(i32, i32)]) -> impl Iterator<Item = ((i32, i32), (i32, i32))> + '_ {
    (0..c.len()).map(move |i| (c[i], c[(i + 1) % c.len()]))
}

/// The part two segments running the same way along one line share, when it has a length: as
/// it runs along the first.
fn overlap(
    (a, b): ((i32, i32), (i32, i32)),
    (c, d): ((i32, i32), (i32, i32)),
) -> Option<((i32, i32), (i32, i32))> {
    if cross(a, b, c) != 0 || cross(a, b, d) != 0 {
        return None;
    }
    let dir = (i64::from(b.0 - a.0), i64::from(b.1 - a.1));
    let along = |p: (i32, i32)| dir.0 * i64::from(p.0 - a.0) + dir.1 * i64::from(p.1 - a.1);
    let (tb, tc, td) = (along(b), along(c), along(d));
    if (i64::from(d.0 - c.0)) * dir.0 + (i64::from(d.1 - c.1)) * dir.1 <= 0 {
        return None;
    }
    let lo = tc.max(0);
    let hi = td.min(tb);
    if hi <= lo {
        return None;
    }
    let pick = |t: i64| {
        if t == 0 {
            a
        } else if t == tb {
            b
        } else if t == tc {
            c
        } else {
            d
        }
    };
    Some((pick(lo), pick(hi)))
}

/// A point of grid corners in region space, at the floor height `z`.
pub(crate) fn corner_point(spacing: f32, (x, y): (f64, f64), z: f32) -> Vec3 {
    #[allow(clippy::cast_possible_truncation)]
    Vec3::new(
        ((x - 0.5) * f64::from(spacing)) as f32,
        ((y - 0.5) * f64::from(spacing)) as f32,
        z,
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (unit checks of the polygon builder's geometry)
    use super::*;

    #[test]
    fn a_shared_edge_whose_step_fails_part_way_is_linked_only_where_it_passes() {
        // A step too high between columns 3 and 5 of a ten-column edge: two links, either side.
        let runs = passing_runs((0, 0), (10, 0), &|(x, _)| !(3.0..5.0).contains(&x));
        assert_eq!(runs, [((0, 0), (3, 0)), ((5, 0), (10, 0))]);
        // Along a diagonal, by its grid points.
        let runs = passing_runs((0, 0), (4, 2), &|(x, _)| x > 2.0);
        assert_eq!(runs, [((2, 1), (4, 2))]);
        assert!(passing_runs((0, 0), (6, 0), &|_| false).is_empty());
    }

    #[test]
    fn a_square_outline_is_one_convex_polygon() {
        let sq = vec![(0, 0), (4, 0), (4, 4), (0, 4)];
        let polys = convex_polygons(&sq).expect("cut");
        assert_eq!(polys.len(), 1);
        assert_eq!(polys[0].len(), 4);
    }

    #[test]
    fn an_l_shaped_outline_is_two_convex_polygons() {
        let l = vec![(0, 0), (6, 0), (6, 2), (2, 2), (2, 6), (0, 6)];
        let polys = convex_polygons(&l).expect("cut");
        assert_eq!(polys.len(), 2, "{polys:?}");
        for p in &polys {
            let n = p.len();
            assert!((0..n).all(|k| cross(p[(k + n - 1) % n], p[k], p[(k + 1) % n]) > 0));
        }
        let area = |p: &Vec<(i32, i32)>| -> i64 {
            (0..p.len())
                .map(|i| {
                    let (a, b) = (p[i], p[(i + 1) % p.len()]);
                    i64::from(a.0) * i64::from(b.1) - i64::from(b.0) * i64::from(a.1)
                })
                .sum()
        };
        assert_eq!(
            polys.iter().map(area).sum::<i64>(),
            2 * 20,
            "the L's area, covered once"
        );
    }

    #[test]
    fn a_staircase_wall_is_straightened_only_inside_the_floor() {
        // A floor under a staircase: the columns (x, y) with x + y < 8, x, y >= 0.
        let inside = |x: i32, y: i32| x >= 0 && y >= 0 && x + y < 8;
        let mut chain = vec![(8, 0)];
        for k in 0..8 {
            chain.push((8 - k, k + 1));
            chain.push((7 - k, k + 1));
        }
        let s = straighten(&chain, &inside);
        assert!(s.len() < chain.len() / 2, "straightened: {s:?}");
        for w in s.windows(2) {
            assert!(keeps_inside(w[0], w[1], &inside), "{w:?} strays out");
        }
    }

    #[test]
    fn the_part_two_edges_share_is_found_running_along_the_first() {
        assert_eq!(
            overlap(((0, 0), (6, 0)), ((2, 0), (9, 0))),
            Some(((2, 0), (6, 0)))
        );
        assert_eq!(overlap(((0, 0), (6, 0)), ((6, 0), (9, 0))), None);
        assert_eq!(overlap(((0, 0), (6, 0)), ((5, 0), (1, 0))), None);
    }

    #[test]
    fn a_segment_leaves_a_polygon_where_it_crosses_its_edge() {
        let p = NavPoly {
            corners: vec![(0, 0), (4, 0), (4, 4), (0, 4)].into_boxed_slice(),
            links: Box::new([]),
            ground: true,
            wading: false,
            tile: (0, 0),
            z: 0.0,
        };
        let t = p.leave((1.0, 1.0), (9.0, 1.0)).expect("starts inside");
        assert!((t - 3.0 / 8.0).abs() < 1e-9, "{t}");
        assert_eq!(p.leave((5.0, 1.0), (9.0, 1.0)), None);
        assert!(p.holds(0, 0) && p.holds(3, 3) && !p.holds(4, 0));
    }
}
