//! A room's floor, and where a placement in a room stands on it.
//!
//! A room's floor is the polygons of its collision that a body can stand on: those facing up, no
//! steeper than the client lets a body stand, with something of the room over them (a polygon
//! facing down, or an opening to a room above); one with nothing over it is the top of the room's
//! shell, outside it. A room's own middle is no safe place to stand in: many rooms have their
//! floor well above the middle's height, and ledges, corners and the rooms around a pit have no
//! floor there at all.
//!
//! A placement in a room ([`Floor::stand`]):
//! - **stands** when its room's floor is straight under it (a body let go there drops onto it; a
//!   thing hung on a wall, such as a house's hook, stands over the floor at any height), or when
//!   it is no more than [`SUNK`] under the floor (the rounding of a height written on the floor);
//! - is **under its floor** when no floor of its room is under it but one is over it: a body
//!   there falls out of the room;
//! - is **over no floor** otherwise: nothing of its room under it, so it falls into whatever is
//!   below, or out of the world.
//!
//! "Straight under" reaches [`EDGE`] past the floor's edge, as a body standing at the edge still
//! rests on it: a door on a doorway's sill stands. The check ([`check_packs`]) names each
//! placement that doesn't stand, with its room and the nearest point of the room's floor: a fault
//! in the editor's checks, which know every weenie's type and so what hangs rather than stands,
//! and a warning in a build's, which know only the packs' own weenies. A new
//! placement in a room is set on its floor ([`Floor::snap`]): on the floor under the point it was
//! given (or just over it), or, given none or by no floor, at the point of the floor farthest from
//! any of its edges.

use std::collections::BTreeMap;

use dereth_assets::world::EnvCell;
use dereth_assets::Environment;
use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::num::consts::FLOOR_Z;
use dereth_primitives::num::{floor_to_i32, to_i32};
use dereth_primitives::{DataId, Frame, Vec3};

use crate::dats::DatSet;
use crate::footprint::Environments;
use crate::lock::Lock;
use crate::source::Pack;
use crate::sql_ids;
use crate::symbol;

/// How far under its floor a placement may be and still stand on it, in meters: the rounding of
/// a height written on the floor.
pub const SUNK: f32 = 0.05;

/// How far under a floor a new placement's point still means that floor, and how far over or
/// under its room's box a placement is still about the room, in meters.
pub const REACH: f32 = 2.0;

/// How far past a floor's edge a placement still stands on it, in meters: a body standing there
/// rests on the edge.
pub const EDGE: f32 = 0.25;

/// The weenie types that hang rather than stand, on a wall or under a ceiling: a house's hook.
pub const HUNG: [u32; 1] = [56];

/// The weenie type of each of `packs`' own weenies, as their SQL says: by class (a server class
/// a pack writes again) and by symbol in full; for [`check_packs`].
#[must_use]
pub fn pack_kinds(packs: &[Pack]) -> (BTreeMap<u32, u32>, BTreeMap<String, u32>) {
    let (mut classes, mut symbols) = (BTreeMap::new(), BTreeMap::new());
    for p in packs {
        for f in &p.sql {
            for (w, x) in sql_ids::weenie_facts(&f.text) {
                let Some(k) = x.kind else { continue };
                if let Some(c) = sql_ids::number(&w) {
                    classes.insert(c, k);
                } else if let Some(sym) = crate::edit::token_symbol(&p.name, &w) {
                    symbols.insert(sym, k);
                }
            }
        }
    }
    (classes, symbols)
}

/// How far a point beside a floor's edge is looked for another polygon of the floor, to tell an
/// edge of the floor from a seam between two of its polygons, in meters.
const PROBE: f32 = 0.05;

/// How far apart in height two polygons of a floor may meet at a seam, in meters; a greater
/// step is an edge.
const SEAM_STEP: f32 = 0.5;

/// One polygon of a room, in landblock meters.
#[derive(Clone, Debug)]
struct Face {
    corners: Vec<Vec3>,
    /// The way its front faces (a floor's up, into the room), of length one.
    n: Vec3,
}

impl Face {
    fn new(corners: Vec<Vec3>) -> Option<Self> {
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
        if len < 1e-6 {
            return None;
        }
        let n = Vec3::new(n.x / len, n.y / len, n.z / len);
        Some(Self { corners, n })
    }

    /// Whether a ray straight up from `from` meets the polygon, from either side.
    fn above(&self, from: Vec3) -> bool {
        self.n.z.abs() > 1e-4
            && self.holds(from.x, from.y)
            && self.height_at(from.x, from.y) > from.z
    }

    /// The height of the polygon's plane over `(x, y)`.
    fn height_at(&self, x: f32, y: f32) -> f32 {
        let c = self.corners[0];
        c.z - (self.n.x * (x - c.x) + self.n.y * (y - c.y)) / self.n.z
    }

    fn edges(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        let k = self.corners.len();
        (0..k).map(move |i| (self.corners[i], self.corners[(i + 1) % k]))
    }

    /// Whether `(x, y)` is inside the polygon, seen from above.
    fn holds(&self, x: f32, y: f32) -> bool {
        let mut inside = false;
        for (a, b) in self.edges() {
            if (a.y > y) != (b.y > y) {
                let t = (y - a.y) / (b.y - a.y);
                if x < a.x + t * (b.x - a.x) {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// How far `(x, y)` is from the polygon, seen from above: 0 inside it.
    fn reach(&self, x: f32, y: f32) -> f32 {
        if self.holds(x, y) {
            return 0.0;
        }
        self.edges()
            .map(|(a, b)| flat_distance(x, y, a, b))
            .fold(f32::MAX, f32::min)
    }

    /// The point of the polygon nearest `p`.
    fn nearest(&self, p: Vec3) -> Vec3 {
        if self.holds(p.x, p.y) {
            return Vec3::new(p.x, p.y, self.height_at(p.x, p.y));
        }
        let mut best = (f32::MAX, self.corners[0]);
        for (a, b) in self.edges() {
            let q = nearest_on(p, a, b);
            let d = q.sub(p);
            let d = d.dot(d);
            if d < best.0 {
                best = (d, q);
            }
        }
        best.1
    }

    fn middle(&self) -> Vec3 {
        #[allow(clippy::cast_precision_loss)]
        let k = 1.0 / self.corners.len() as f32;
        let s = self.corners.iter().fold(Vec3::ZERO, |s, c| s.add(*c));
        Vec3::new(s.x * k, s.y * k, s.z * k)
    }
}

/// How far `(x, y)` is from the segment from `a` to `b`, seen from above.
fn flat_distance(x: f32, y: f32, a: Vec3, b: Vec3) -> f32 {
    let (ex, ey) = (b.x - a.x, b.y - a.y);
    let len = ex * ex + ey * ey;
    let t = if len < 1e-12 {
        0.0
    } else {
        (((x - a.x) * ex + (y - a.y) * ey) / len).clamp(0.0, 1.0)
    };
    let (dx, dy) = (a.x + t * ex - x, a.y + t * ey - y);
    (dx * dx + dy * dy).sqrt()
}

/// The point of the segment from `a` to `b` nearest `p`.
fn nearest_on(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let e = b.sub(a);
    let len = e.dot(e);
    let t = if len < 1e-12 {
        0.0
    } else {
        (p.sub(a).dot(e) / len).clamp(0.0, 1.0)
    };
    a.add(e.mul(t))
}

/// Where a placement stands in its room ([`Floor::stand`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stand {
    /// Over its floor by this much, or on it.
    On(f32),
    /// Under its floor by this much, with none of it under it.
    Under(f32),
    /// Over no floor of its room, and under none.
    Over,
}

/// A room's floor, in landblock meters.
#[derive(Clone, Debug, Default)]
pub struct Floor {
    faces: Vec<Face>,
    /// The box the room's corners span, low and high corner; none known for a floor made of
    /// polygons alone.
    bounds: Option<(Vec3, Vec3)>,
}

impl Floor {
    /// The floor of room `cell`, its piece one of `env`'s; `None` when `env` has no such piece.
    #[must_use]
    pub fn of(cell: &EnvCell, env: &Environment) -> Option<Self> {
        Self::of_piece(env, cell.cell_struct, &cell.frame)
    }

    /// The floor of piece `piece` of `env` set at `frame`; `None` when `env` has no such piece.
    #[must_use]
    pub fn of_piece(env: &Environment, piece: u16, frame: &Frame) -> Option<Self> {
        let cs = env.cells.get(usize::from(piece))?;
        let verts = &cs.vertex_array.vertices;
        let at = |i: &u16| {
            verts
                .get(usize::from(*i))
                .map(|v| localtoglobal(frame, v.position))
        };
        let face = |p: &dereth_assets::Polygon| {
            Face::new(p.vertex_ids.iter().map(at).collect::<Option<_>>()?)
        };
        let solid: Vec<Face> = cs.physics_polygons.iter().filter_map(face).collect();
        // What closes a room over its floor: a polygon facing down into it, or an opening to the
        // room above.
        let over: Vec<&Face> = solid.iter().filter(|f| f.n.z < -1e-3).collect();
        let openings: Vec<Face> = cs
            .portals
            .iter()
            .filter_map(|i| cs.polygons.get(usize::from(*i)))
            .filter_map(face)
            .collect();
        // A floor faces up into the room: something of the room is over it. A polygon facing
        // up with nothing over it is the outside of the room's shell, its top.
        let faces = solid
            .iter()
            .filter(|f| f.n.z >= FLOOR_Z)
            .filter(|f| {
                let m = f.middle();
                let from = Vec3::new(m.x, m.y, m.z + 0.02);
                over.iter().any(|o| o.above(from)) || openings.iter().any(|o| o.above(from))
            })
            .cloned()
            .collect();
        let (mut lo, mut hi) = (
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        );
        for v in verts {
            let q = localtoglobal(frame, v.position);
            lo = Vec3::new(lo.x.min(q.x), lo.y.min(q.y), lo.z.min(q.z));
            hi = Vec3::new(hi.x.max(q.x), hi.y.max(q.y), hi.z.max(q.z));
        }
        Some(Self {
            faces,
            bounds: (!verts.is_empty()).then_some((lo, hi)),
        })
    }

    /// Whether the room has no floor at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Whether `p` is in or about the room: inside the box its corners span, or within [`EDGE`]
    /// of its sides or [`REACH`] of its top and bottom. A floor of polygons alone holds everything.
    #[must_use]
    pub fn about(&self, p: Vec3) -> bool {
        self.bounds.is_none_or(|(lo, hi)| {
            (lo.x - EDGE..=hi.x + EDGE).contains(&p.x)
                && (lo.y - EDGE..=hi.y + EDGE).contains(&p.y)
                && (lo.z - REACH..=hi.z + REACH).contains(&p.z)
        })
    }

    /// The heights of the floor straight under and over `(x, y)`, reaching [`EDGE`] past its
    /// edges.
    fn heights(&self, x: f32, y: f32) -> impl Iterator<Item = f32> + '_ {
        self.faces
            .iter()
            .filter(move |f| f.reach(x, y) <= EDGE)
            .map(move |f| f.height_at(x, y))
    }

    /// The floor straight under `p`, however far down: where a body let go at `p` lands in the
    /// room. `None` when no floor of the room is under it.
    #[must_use]
    pub fn under(&self, p: Vec3) -> Option<Vec3> {
        self.heights(p.x, p.y)
            .filter(|h| *h <= p.z + SUNK)
            .reduce(f32::max)
            .map(|h| Vec3::new(p.x, p.y, h))
    }

    /// Where a placement at `p` stands in the room.
    #[must_use]
    pub fn stand(&self, p: Vec3) -> Stand {
        if let Some(q) = self.under(p) {
            return Stand::On((p.z - q.z).max(0.0));
        }
        match self
            .heights(p.x, p.y)
            .filter(|h| *h > p.z + SUNK)
            .reduce(f32::min)
        {
            Some(h) => Stand::Under(h - p.z),
            None => Stand::Over,
        }
    }

    /// The point of the floor nearest `p`; `None` for a room with no floor.
    #[must_use]
    pub fn nearest(&self, p: Vec3) -> Option<Vec3> {
        self.faces.iter().map(|f| f.nearest(p)).min_by(|a, b| {
            let (da, db) = (a.sub(p), b.sub(p));
            da.dot(da).total_cmp(&db.dot(db))
        })
    }

    /// The edges of the floor, seen from above: each polygon's edge with no other polygon of
    /// the floor just beyond it at about its height.
    fn outline(&self) -> Vec<(Vec3, Vec3)> {
        let mut out = Vec::new();
        for (i, f) in self.faces.iter().enumerate() {
            let m = f.middle();
            for (a, b) in f.edges() {
                let (ex, ey) = (b.x - a.x, b.y - a.y);
                let len = (ex * ex + ey * ey).sqrt();
                if len < 1e-4 {
                    continue;
                }
                let mid = Vec3::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0, (a.z + b.z) / 2.0);
                // Away from the polygon's own middle.
                let (mut ox, mut oy) = (ey / len, -ex / len);
                if ox * (m.x - mid.x) + oy * (m.y - mid.y) > 0.0 {
                    (ox, oy) = (-ox, -oy);
                }
                let (px, py) = (mid.x + ox * PROBE, mid.y + oy * PROBE);
                let seam = self.faces.iter().enumerate().any(|(j, g)| {
                    j != i && g.holds(px, py) && (g.height_at(px, py) - mid.z).abs() <= SEAM_STEP
                });
                if !seam {
                    out.push((a, b));
                }
            }
        }
        out
    }

    /// The point of the floor farthest from any of its edges (seen from above), on the floor; the
    /// first found of points as far. `None` for a room with no floor.
    #[must_use]
    pub fn middle(&self) -> Option<Vec3> {
        let first = self.faces.first()?;
        let outline = self.outline();
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for c in self.faces.iter().flat_map(|f| &f.corners) {
            lo = [lo[0].min(c.x), lo[1].min(c.y)];
            hi = [hi[0].max(c.x), hi[1].max(c.y)];
        }
        let step = ((hi[0] - lo[0]).max(hi[1] - lo[1]) / 64.0).clamp(0.1, 0.5);
        let clear = |x: f32, y: f32| {
            outline
                .iter()
                .map(|(a, b)| flat_distance(x, y, *a, *b))
                .fold(f32::MAX, f32::min)
        };
        let mut best: Option<(f32, Vec3)> = None;
        let mut consider = |f: &Face, x: f32, y: f32| {
            if !f.holds(x, y) {
                return;
            }
            let d = clear(x, y);
            if best.is_none_or(|(b, _)| d > b) {
                best = Some((d, Vec3::new(x, y, f.height_at(x, y))));
            }
        };
        for f in &self.faces {
            let m = f.middle();
            consider(f, m.x, m.y);
            let (mut flo, mut fhi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for c in &f.corners {
                flo = [flo[0].min(c.x), flo[1].min(c.y)];
                fhi = [fhi[0].max(c.x), fhi[1].max(c.y)];
            }
            // The grid is the floor's, so neighboring polygons share its points.
            let (i0, i1, j0, j1) = (
                floor_to_i32((flo[0] - lo[0]) / step),
                to_i32(((fhi[0] - lo[0]) / step).ceil()),
                floor_to_i32((flo[1] - lo[1]) / step),
                to_i32(((fhi[1] - lo[1]) / step).ceil()),
            );
            for i in i0..=i1 {
                for j in j0..=j1 {
                    #[allow(clippy::cast_precision_loss)]
                    let (x, y) = (lo[0] + i as f32 * step, lo[1] + j as f32 * step);
                    consider(f, x, y);
                }
            }
        }
        Some(best.map_or_else(|| first.middle(), |(_, p)| p))
    }

    /// Where a new placement in the room goes: on the floor straight under `at`; else on the
    /// floor straight over it within [`REACH`] (a point just under the floor means the floor
    /// there); else, or given no point, at the floor's [`Self::middle`]. `None` for a room with no
    /// floor.
    #[must_use]
    pub fn snap(&self, at: Option<Vec3>) -> Option<Vec3> {
        at.and_then(|p| {
            self.under(p).or_else(|| {
                self.heights(p.x, p.y)
                    .filter(|h| *h > p.z && *h <= p.z + REACH)
                    .reduce(f32::min)
                    .map(|h| Vec3::new(p.x, p.y, h))
            })
        })
        .or_else(|| self.middle())
    }

    /// What is wrong with a placement at `p` in the room, in words; `None` when it stands.
    #[must_use]
    pub fn fault(&self, p: Vec3) -> Option<String> {
        let what = match self.stand(p) {
            Stand::On(_) => return None,
            Stand::Under(by) => format!("stands {by:.2} m under its room's floor"),
            Stand::Over => "stands over no floor of its room".to_owned(),
        };
        Some(match self.nearest(p) {
            Some(q) => format!(
                "{what}; the nearest floor is at [{:.2}, {:.2}, {:.2}]",
                q.x, q.y, q.z
            ),
            None => format!("{what}; its room has no floor"),
        })
    }
}

/// The placements of `packs` in rooms that don't stand on their room's floor
/// ([`Floor::fault`]), over the files `set` (with the packs' overlay laid over them): one line
/// each, `file:line: ...`, naming the placement, its room and the nearest point of the room's
/// floor. A pack's new room is named by its symbol, numbered by `lock`. A placement whose room
/// the files don't hold is left to the check of what SQL names, and one that stands outside its
/// room altogether ([`Floor::about`]) is no body in the room. Nor is a spawn of a weenie that
/// hangs ([`HUNG`]), its type as `kind` knows it (of the pack placing it, and the weenie as
/// written).
#[must_use]
pub fn check_packs(
    packs: &[Pack],
    lock: &Lock,
    set: &DatSet,
    kind: &dyn Fn(&Pack, &str) -> Option<u32>,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut envs = Environments::default();
    let mut floors: BTreeMap<u32, Option<Floor>> = BTreeMap::new();
    for p in packs {
        for f in &p.sql {
            for i in sql_ids::instances(&f.text) {
                let cell = i
                    .cell_symbol
                    .as_deref()
                    .and_then(|s| symbol::qualify(&p.name, s).ok())
                    .and_then(|s| {
                        let lb = u16::try_from(i.cell >> 16).ok()?;
                        lock.get(symbol::Kind::Cell, &symbol::canonical_instance(&s, lb))
                    })
                    .unwrap_or(i.cell);
                if !(u32::from(crate::cells::FIRST_CELL)..0xFFFE).contains(&(cell & 0xFFFF))
                    || kind(p, &i.weenie).is_some_and(|k| HUNG.contains(&k))
                {
                    continue;
                }
                let floor = floors.entry(cell).or_insert_with(|| {
                    let id = DataId(cell);
                    let c = set.decode::<EnvCell>(id, &set.cell.read(id).ok()?).ok()?;
                    Floor::of(&c, envs.get(set, c.environment)?)
                });
                let Some(floor) = floor else { continue };
                let at = Vec3::new(i.origin[0], i.origin[1], i.origin[2]);
                // A spawn standing elsewhere than its room altogether is no body in it: a house
                // portal's spot names the basement's room but stands where the portal leads.
                if !floor.about(at) {
                    continue;
                }
                if let Some(why) = floor.fault(at) {
                    out.push(format!(
                        "{}:{}: spawn {} in room 0x{cell:08X} {why}",
                        f.path.display(),
                        i.line,
                        i.guid
                    ));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the arithmetic of a floor's polygons; the rule over real rooms is
    //! the dat tests')
    use super::*;

    /// A floor of one square `side` meters a side at height `z`, its corner at the origin,
    /// wound to face up.
    fn square(side: f32, z: f32) -> Face {
        Face::new(vec![
            Vec3::new(0.0, 0.0, z),
            Vec3::new(side, 0.0, z),
            Vec3::new(side, side, z),
            Vec3::new(0.0, side, z),
        ])
        .unwrap()
    }

    #[test]
    fn a_polygon_wound_clockwise_seen_from_above_faces_down() {
        let f = Face::new(vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ])
        .unwrap();
        assert!(f.n.z < -0.99, "{:?}", f.n);
        assert!(f.above(Vec3::new(0.2, 0.5, -1.0)) && !f.above(Vec3::new(0.2, 0.5, 1.0)));
    }

    #[test]
    fn a_point_over_or_just_into_the_floor_stands_and_one_under_or_beside_it_does_not() {
        let f = Floor {
            faces: vec![square(4.0, 1.0)],
            bounds: None,
        };
        assert_eq!(f.stand(Vec3::new(2.0, 2.0, 1.0)), Stand::On(0.0));
        assert_eq!(f.stand(Vec3::new(2.0, 2.0, 0.97)), Stand::On(0.0));
        assert!(matches!(f.stand(Vec3::new(2.0, 2.0, 2.5)), Stand::On(_)));
        assert!(
            matches!(f.stand(Vec3::new(2.0, 2.0, 9.0)), Stand::On(h) if (h - 8.0).abs() < 1e-4)
        );
        assert!(
            matches!(f.stand(Vec3::new(2.0, 2.0, 0.5)), Stand::Under(d) if (d - 0.5).abs() < 1e-4)
        );
        // At the edge, a body still rests on it; a step beyond, it doesn't.
        assert!(matches!(f.stand(Vec3::new(4.2, 2.0, 1.0)), Stand::On(_)));
        assert_eq!(f.stand(Vec3::new(5.0, 2.0, 1.0)), Stand::Over);
        let why = f.fault(Vec3::new(2.0, 2.0, 0.5)).unwrap();
        assert!(
            why.contains("0.50 m under") && why.contains("[2.00, 2.00, 1.00]"),
            "{why}"
        );
    }

    #[test]
    fn a_point_is_about_its_room_inside_its_box_or_just_beyond_it_and_not_far_off() {
        let f = Floor {
            faces: vec![square(4.0, 0.0)],
            bounds: Some((Vec3::new(0.0, 0.0, 0.0), Vec3::new(4.0, 4.0, 3.0))),
        };
        assert!(f.about(Vec3::new(2.0, 2.0, 1.0)));
        assert!(f.about(Vec3::new(4.2, 2.0, 4.5)));
        assert!(!f.about(Vec3::new(2.0, 2.0, 6.0)));
        assert!(!f.about(Vec3::new(40.0, 2.0, 1.0)));
    }

    #[test]
    fn the_middle_of_a_floor_of_two_polygons_is_the_farthest_from_its_outline_not_their_seam() {
        // Two 4 m squares side by side, stretched to 8 m north, make an 8 m square floor: its
        // middle is on the seam, 4 m from every edge, where each half's own is 2 m from one.
        let tall = |east: f32| {
            let mut f = square(4.0, 0.0);
            for c in &mut f.corners {
                c.x += east;
                c.y *= 2.0;
            }
            f
        };
        let f = Floor {
            faces: vec![tall(0.0), tall(4.0)],
            bounds: None,
        };
        let m = f.middle().unwrap();
        assert!(
            (m.x - 4.0).abs() <= 0.15 && (m.y - 4.0).abs() <= 0.15,
            "{m:?}"
        );
        // Over the floor, a placement drops onto it, and just under it, it rises onto it; beside
        // it, or far under it, it goes to the middle.
        assert_eq!(
            f.snap(Some(Vec3::new(1.0, 1.0, 3.0))),
            Some(Vec3::new(1.0, 1.0, 0.0))
        );
        assert_eq!(
            f.snap(Some(Vec3::new(1.0, 1.0, -0.5))),
            Some(Vec3::new(1.0, 1.0, 0.0))
        );
        assert_eq!(f.snap(Some(Vec3::new(1.0, 1.0, -5.0))), Some(m));
        assert_eq!(f.snap(Some(Vec3::new(20.0, 1.0, 3.0))), Some(m));
        assert_eq!(f.snap(None), Some(m));
    }
}
