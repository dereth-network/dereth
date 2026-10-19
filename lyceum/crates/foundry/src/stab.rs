//! The stab lists: for each indoor cell, the cells that may be seen from where it can see, and
//! for each doorway of a building, the building's cells.
//!
//! The client bounds what it draws indoors by these lists, finds the cell a moving point entered
//! among them, and the server tells a player of the objects in the cells of the player's list. A
//! world builder's new cells need them, and nothing in the client makes them.
//!
//! **What retail's lists hold**, as measured over every retail cell (the dat tier's calibration):
//! - **A cell's list** holds the cells seen from it, and the cells seen from those: two steps of
//!   sight, never the cell itself. One step is a line of sight through the portals between two
//!   cells, either way; the outdoors carries no sight.
//! - **A building's cell that is seen from outside** (it, or a cell of its list, sees out
//!   through an opening to the outdoors) also holds every other cell of its building.
//! - **A doorway's list** holds every cell of its building, in ascending order.
//!
//! **The walk** that finds a line of sight is portal flow. From each portal of a cell, the cells
//! beyond are walked depth first; a cell's neighbors are always seen. A cell further on is seen
//! when what is left of its portal, clipped to the planes that separate the first portal from the
//! last one passed, is not empty, and what is left of the first portal is clipped the other way
//! in turn: some line of sight then runs from the first cell through every portal on the way.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::{EnvCell, Environment};
use dereth_primitives::frame::localtoglobal;
use dereth_primitives::Vec3;

type V = [f64; 3];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn scale(a: V, s: f64) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// A plane, `n · p = d`, its normal of length one.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Plane {
    n: V,
    d: f64,
}

impl Plane {
    fn dist(&self, p: V) -> f64 {
        dot(self.n, p) - self.d
    }
    fn flip(self) -> Self {
        Self {
            n: scale(self.n, -1.0),
            d: -self.d,
        }
    }
    /// The plane of a polygon, by Newell's method; `None` for one with no area.
    fn of(poly: &[V]) -> Option<Self> {
        let mut n = [0.0; 3];
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        let len = dot(n, n).sqrt();
        if len < 1e-9 {
            return None;
        }
        let n = scale(n, 1.0 / len);
        let c = scale(
            poly.iter().fold([0.0; 3], |s, p| add(s, *p)),
            1.0 / poly.len() as f64,
        );
        Some(Self { n, d: dot(n, c) })
    }
}

/// How far from a plane a point still counts as on it, in meters.
pub const ON_PLANE: f64 = 0.001;

/// The part of `w` on the plane's front side (within [`ON_PLANE`] counts as on it, and is kept);
/// `None` when nothing is in front.
fn chop(w: &[V], p: &Plane) -> Option<Vec<V>> {
    let d: Vec<f64> = w.iter().map(|v| p.dist(*v)).collect();
    if d.iter().all(|x| *x >= -ON_PLANE) {
        return Some(w.to_vec());
    }
    if d.iter().all(|x| *x <= ON_PLANE) {
        return None;
    }
    let mut out = Vec::with_capacity(w.len() + 2);
    for i in 0..w.len() {
        let j = (i + 1) % w.len();
        let (a, b, da, db) = (w[i], w[j], d[i], d[j]);
        if da >= -ON_PLANE {
            out.push(a);
        }
        if (da > ON_PLANE && db < -ON_PLANE) || (da < -ON_PLANE && db > ON_PLANE) {
            let t = da / (da - db);
            out.push(add(a, scale(sub(b, a), t)));
        }
    }
    (out.len() >= 3).then_some(out)
}

/// `target` clipped to the planes through an edge of `source` and a vertex of `pass` that put
/// `source` and `pass` on opposite sides: the region a line through both can reach. `flip`
/// keeps the other side, for the planes made the other way round.
fn clip_to_separators(source: &[V], pass: &[V], target: Vec<V>, flip: bool) -> Option<Vec<V>> {
    let mut target = target;
    for i in 0..source.len() {
        let l = (i + 1) % source.len();
        let v1 = sub(source[l], source[i]);
        for j in 0..pass.len() {
            let v2 = sub(pass[j], source[i]);
            let n = cross(v1, v2);
            let len = dot(n, n).sqrt();
            if len < ON_PLANE {
                continue;
            }
            let mut plane = Plane {
                n: scale(n, 1.0 / len),
                d: 0.0,
            };
            plane.d = dot(pass[j], plane.n);
            // Which side the source is on.
            let mut flip_test = None;
            for (k, s) in source.iter().enumerate() {
                if k == i || k == l {
                    continue;
                }
                let d = plane.dist(*s);
                if d < -ON_PLANE {
                    flip_test = Some(false);
                    break;
                } else if d > ON_PLANE {
                    flip_test = Some(true);
                    break;
                }
            }
            let Some(flip_test) = flip_test else {
                continue; // the source lies in the plane
            };
            if flip_test {
                plane = plane.flip();
            }
            // Every other vertex of the pass on the front: a separating plane.
            let mut front = 0;
            let mut separating = true;
            for (k, p) in pass.iter().enumerate() {
                if k == j {
                    continue;
                }
                let d = plane.dist(*p);
                if d < -ON_PLANE {
                    separating = false;
                    break;
                } else if d > ON_PLANE {
                    front += 1;
                }
            }
            if !separating || front == 0 {
                continue;
            }
            if flip {
                plane = plane.flip();
            }
            target = chop(&target, &plane)?;
        }
    }
    Some(target)
}

/// Where a portal leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    Cell(u16),
    Outdoors,
}

/// A portal as the walk goes through it: where it leads, its polygon in landblock space, and its
/// plane facing where it leads.
#[derive(Clone, Debug)]
struct Way {
    to: u16,
    poly: Vec<V>,
    plane: Plane,
}

/// A landblock's cells as the walk sees them: the ways out of each.
#[derive(Clone, Debug, Default)]
pub struct Cells {
    ways: BTreeMap<u16, Vec<Way>>,
    /// Every cell each cell's portals lead to, those with a polygon too small to walk through
    /// among them.
    neighbors: BTreeMap<u16, Vec<u16>>,
}

/// One cell's portal polygons and where they lead, in landblock space, and a point inside it.
#[derive(Clone, Debug)]
pub struct CellShape {
    pub id: u16,
    /// The middle of the cell's vertices.
    pub inside: Vec3,
    /// Each portal of the cell record: where it leads, and its polygon.
    pub portals: Vec<(Place, Vec<Vec3>)>,
}

/// The shape of cell `cell` of the landblock, its portal polygons placed by its frame; `None`
/// when its environment or cell struct is missing.
#[must_use]
pub fn shape(cell: &EnvCell, env: &Environment) -> Option<CellShape> {
    let cs = env.cells.get(usize::from(cell.cell_struct))?;
    let place = |p: Vec3| localtoglobal(&cell.frame, p);
    let verts = &cs.vertex_array.vertices;
    if verts.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let inside = {
        let n = verts.len() as f32;
        let s = verts.iter().fold(Vec3::ZERO, |s, v| {
            Vec3::new(s.x + v.position.x, s.y + v.position.y, s.z + v.position.z)
        });
        place(Vec3::new(s.x / n, s.y / n, s.z / n))
    };
    let mut portals = Vec::new();
    for p in &cell.portals {
        let to = if p.flags & 4 != 0 {
            Place::Outdoors
        } else {
            #[allow(clippy::cast_possible_truncation)]
            Place::Cell(p.other_cell_id as u16)
        };
        let poly: Vec<Vec3> = cs
            .polygons
            .get(usize::from(p.polygon_id))
            .map(|poly| {
                poly.vertex_ids
                    .iter()
                    .filter_map(|i| verts.get(usize::from(*i)).map(|v| place(v.position)))
                    .collect()
            })
            .unwrap_or_default();
        portals.push((to, poly));
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(CellShape {
        id: (cell.id.raw() & 0xFFFF) as u16,
        inside,
        portals,
    })
}

fn v(p: Vec3) -> V {
    [f64::from(p.x), f64::from(p.y), f64::from(p.z)]
}

/// The outdoors, as the walk marks it seen.
const OUT: u16 = u16::MAX;

/// One step of the walk: what is left of the first portal, of the last portal passed (none at
/// the first), and the last portal's plane.
struct Step {
    source: Vec<V>,
    pass: Option<Vec<V>>,
    plane: Plane,
}

fn near(a: V, b: V) -> bool {
    (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4 && (a[2] - b[2]).abs() < 1e-4
}

impl Cells {
    /// The walk's view of `shapes`, a landblock's cells.
    #[must_use]
    pub fn new(shapes: &[CellShape]) -> Self {
        Self::widened(shapes, 0.0)
    }

    /// The walk's view of `shapes`, each portal widened by `widen` meters at every corner, away
    /// from its middle in its own plane: sight that grazes an edge is taken.
    #[must_use]
    pub fn widened(shapes: &[CellShape], widen: f64) -> Self {
        let mut c = Self::default();
        for s in shapes {
            let inside = v(s.inside);
            let mut mine = Vec::new();
            for (to, poly) in &s.portals {
                let to = match to {
                    Place::Cell(to) => {
                        c.neighbors.entry(s.id).or_default().push(*to);
                        to
                    }
                    // The outdoors, as one more place the walk can see into.
                    Place::Outdoors => &OUT,
                };
                let poly: Vec<V> = poly.iter().map(|p| v(*p)).collect();
                let poly = widen_poly(&poly, widen);
                let Some(mut plane) = (poly.len() >= 3).then(|| Plane::of(&poly)).flatten() else {
                    continue;
                };
                // Facing where it leads: away from the cell's inside.
                if plane.dist(inside) > 0.0 {
                    plane = plane.flip();
                }
                mine.push(Way {
                    to: *to,
                    poly,
                    plane,
                });
            }
            c.ways.insert(s.id, mine);
        }
        c
    }

    /// The cells seen from cell `id`: every cell a line of sight from it reaches, in the order
    /// the walk first reaches them (never the cell itself).
    #[must_use]
    pub fn seen(&self, id: u16) -> Vec<u16> {
        self.sight(id).0
    }

    /// Whether a line of sight from cell `id` reaches an opening to the outdoors.
    #[must_use]
    pub fn sees_out(&self, id: u16) -> bool {
        self.sight(id).1
    }

    /// The cells seen from cell `id`, and whether it sees out.
    #[must_use]
    pub fn sight(&self, id: u16) -> (Vec<u16>, bool) {
        let mut seen = Vec::new();
        let mut found = BTreeSet::from([id]);
        for w in self.ways.get(&id).into_iter().flatten() {
            let mut path = vec![id];
            self.flow(
                w.to,
                &Step {
                    source: w.poly.clone(),
                    pass: None,
                    plane: w.plane,
                },
                &w.plane,
                &mut path,
                &mut found,
                &mut seen,
            );
        }
        // A neighbor is always seen, through any portal.
        for n in self.neighbors.get(&id).into_iter().flatten() {
            if found.insert(*n) {
                seen.push(*n);
            }
        }
        let out = seen.contains(&OUT);
        seen.retain(|c| *c != OUT);
        (seen, out)
    }

    fn flow(
        &self,
        at: u16,
        prev: &Step,
        head: &Plane,
        path: &mut Vec<u16>,
        found: &mut BTreeSet<u16>,
        seen: &mut Vec<u16>,
    ) {
        if found.insert(at) {
            seen.push(at);
        }
        path.push(at);
        for w in self.ways.get(&at).into_iter().flatten() {
            if path.contains(&w.to) {
                continue;
            }
            // Not back out through the face it came in by.
            let back = w.plane.flip();
            if near(back.n, prev.plane.n) && (back.d - prev.plane.d).abs() < ON_PLANE {
                continue;
            }
            let Some(pass) = chop(&w.poly, head) else {
                continue;
            };
            let Some(source) = chop(&prev.source, &back) else {
                continue;
            };
            let Some(prev_pass) = &prev.pass else {
                // The second cell can only be hidden by lying in the first portal's plane.
                self.flow(
                    w.to,
                    &Step {
                        source,
                        pass: Some(pass),
                        plane: w.plane,
                    },
                    head,
                    path,
                    found,
                    seen,
                );
                continue;
            };
            let Some(pass) = chop(&pass, &prev.plane) else {
                continue;
            };
            let Some(pass) = clip_to_separators(&source, prev_pass, pass, false) else {
                continue;
            };
            let Some(pass) = clip_to_separators(prev_pass, &source, pass, true) else {
                continue;
            };
            // The first portal narrowed to what sees through the last passed to what is left of
            // this one, and back.
            let Some(source) = clip_to_separators(&pass, prev_pass, source, false) else {
                continue;
            };
            let Some(source) = clip_to_separators(prev_pass, &pass, source, true) else {
                continue;
            };
            self.flow(
                w.to,
                &Step {
                    source,
                    pass: Some(pass),
                    plane: w.plane,
                },
                head,
                path,
                found,
                seen,
            );
        }
        path.pop();
    }
}

/// `poly` with each corner moved `by` meters further from its middle.
fn widen_poly(poly: &[V], by: f64) -> Vec<V> {
    if by <= 0.0 || poly.is_empty() {
        return poly.to_vec();
    }
    #[allow(clippy::cast_precision_loss)]
    let c = scale(
        poly.iter().fold([0.0; 3], |s, p| add(s, *p)),
        1.0 / poly.len() as f64,
    );
    poly.iter()
        .map(|p| {
            let d = sub(*p, c);
            let len = dot(d, d).sqrt();
            if len < 1e-9 {
                *p
            } else {
                add(*p, scale(d, by / len))
            }
        })
        .collect()
}

/// The cells of each building of a landblock: those its doorways lead into (`doorways`, one
/// list for each building), and every cell joined to those by cell portals.
#[must_use]
pub fn buildings(shapes: &BTreeMap<u16, CellShape>, doorways: &[Vec<u16>]) -> Vec<BTreeSet<u16>> {
    doorways
        .iter()
        .map(|d| {
            let mut out = BTreeSet::new();
            let mut todo = d.clone();
            while let Some(c) = todo.pop() {
                if !out.insert(c) {
                    continue;
                }
                for (to, _) in shapes
                    .get(&c)
                    .map(|s| s.portals.as_slice())
                    .unwrap_or_default()
                {
                    if let Place::Cell(n) = to {
                        todo.push(*n);
                    }
                }
            }
            out
        })
        .collect()
}

/// The stab lists of the cells `which` of a landblock whose cells are `shapes` and whose
/// buildings hold the cells `buildings` ([`buildings`]), nearest first (never the cell itself):
/// the cells seen from each and seen from those, and, for a building's cell seen from outside
/// (it, or a cell of its list, sees out through an opening), the rest of its building. Each comes
/// with whether the cell is seen from outside.
#[must_use]
pub fn lists(
    shapes: &BTreeMap<u16, CellShape>,
    buildings: &[BTreeSet<u16>],
    which: &BTreeSet<u16>,
) -> BTreeMap<u16, (Vec<u16>, bool)> {
    lists_widened(shapes, buildings, which, WIDEN)
}

/// How far each portal is widened for the walk's sight ([`Cells::widened`]), in meters: none, so
/// a line of sight that only grazes the edge between two portals is not taken. Retail's lists
/// take some such lines and not others; taking them all would list many cells retail does not.
pub const WIDEN: f64 = 0.0;

/// [`lists`], the portals widened by `widen` meters.
#[must_use]
pub fn lists_widened(
    shapes: &BTreeMap<u16, CellShape>,
    buildings: &[BTreeSet<u16>],
    which: &BTreeSet<u16>,
    widen: f64,
) -> BTreeMap<u16, (Vec<u16>, bool)> {
    let all: Vec<CellShape> = shapes.values().cloned().collect();
    let g = Cells::widened(&all, widen);
    // Sight both ways: what a cell sees, and every cell that sees it.
    let mut one: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
    let mut out_seen: BTreeSet<u16> = BTreeSet::new();
    for c in shapes.keys() {
        let (seen, out) = g.sight(*c);
        if out {
            out_seen.insert(*c);
        }
        for x in &seen {
            one.entry(*x).or_default().push(*c);
        }
        one.entry(*c).or_default().extend(seen);
    }
    for l in one.values_mut() {
        l.sort_unstable();
        l.dedup();
    }
    let seen_from = |c: u16, one: &mut BTreeMap<u16, Vec<u16>>| -> Vec<u16> {
        one.get(&c).cloned().unwrap_or_default()
    };
    let mut out: BTreeMap<u16, (BTreeSet<u16>, bool)> = BTreeMap::new();
    for a in which {
        let first = seen_from(*a, &mut one);
        let mut set: BTreeSet<u16> = first.iter().copied().collect();
        for c in &first {
            set.extend(seen_from(*c, &mut one));
        }
        set.remove(a);
        // Seen from outside: it, or a cell of its list, sees out through an opening.
        let outside = out_seen.contains(a) || set.iter().any(|c| out_seen.contains(c));
        if outside {
            for b in buildings.iter().filter(|b| b.contains(a)) {
                set.extend(b.iter().copied());
            }
            set.remove(a);
        }
        out.insert(*a, (set, outside));
    }
    which
        .iter()
        .filter_map(|a| {
            let (set, outside) = out.remove(a)?;
            let here = shapes.get(a).map(|s| v(s.inside));
            let mut list: Vec<u16> = set.into_iter().collect();
            if let Some(h) = here {
                let d = |c: &u16| {
                    shapes.get(c).map_or(f64::MAX, |s| {
                        let p = sub(v(s.inside), h);
                        dot(p, p)
                    })
                };
                list.sort_by(|x, y| d(x).total_cmp(&d(y)).then(x.cmp(y)));
            }
            Some((*a, (list, outside)))
        })
        .collect()
}

/// The list of a doorway into a building whose cells are `building`: all of them, ascending.
#[must_use]
pub fn doorway_list(building: &BTreeSet<u16>) -> Vec<u16> {
    building.iter().copied().collect()
}
