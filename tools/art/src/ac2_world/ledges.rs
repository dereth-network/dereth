//! AC2's ledges that AC2's bodies climb and AC1's do not, made climbable in the converted world.
//!
//! **The ledge:** AC2's bodies step up 0.9 m, AC1's 0.6 m; on a static's vertical side AC1's
//! real limit is how far its foot sphere reaches over the edge, 0.58 to 0.86 m for a human by
//! speed and angle. A **band ledge** is the edge of a static's walkable collision polygon standing
//! more than [`RISE_LO`] and at most [`RISE_HI`] over the floor just outside it ([`OUT`] out): the
//! terrain, or any static's walkable polygon at or under the edge; and whose top a body stands on
//! ([`INSIDE`] in from the edge, with room over it). An edge whose outside is under the ground,
//! or has something over it within a body's height, is no approach and is left.
//!
//! **Onto the terrain** ([`swell`]): the converted land is raised at the ledge, as it is under
//! buried statics, so what is drawn and what collides stay one surface. Each corner of the squares
//! under the points [`OUT`] and [`APPROACH`] out from the edge rises to the first height of the
//! table no lower than the edge less [`CLIMB`], under the top where the table allows; never lower
//! than AC1 climbs from, nor more than [`OVER_TOP`] over the top. A corner on water, or on a
//! square held (a building's), is never raised. A raise that would bring the land over any
//! static's walkable polygon it was under (the ledge's own top, a doorway's floor, a lower step)
//! is not made.
//!
//! **Onto another static, or terrain that could not be raised** ([`ramps`]): a collision-only
//! wedge from the edge down to the floor under it, its front and its ends no steeper than 1 in
//! [`RUN`], so no face of it is one AC1 cannot walk and it narrows no way at the floor. Where the
//! floor falls away under it by more than AC1 climbs, its foot follows the floor down. A wedge
//! that would bring its surface within [`HEADROOM`] of anything over it (a doorway's lintel, a
//! passage's roof), or a body on it within [`WALL`] of a face it passed clear of on the floor (a
//! wall leaning out, a jamb), is not made.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::ac2_world::table;

/// AC1's walkable polygons: the normal's height at least this.
pub const FLOOR_Z: f64 = 0.664_174;
/// Ledges up to this high AC1's human climbs at every tested speed and angle, meters.
pub const RISE_LO: f64 = 0.55;
/// Ledges higher than this AC2's body does not climb either (on a vertical side it climbs up to
/// about 1 m by speed and angle), meters.
pub const RISE_HI: f64 = 1.05;
/// Edge samples are this far apart, meters.
pub const SAMPLE: f64 = 0.5;
/// The floor is read this far outside an edge, meters; and the approach this far.
pub const OUT: f64 = 0.3;
/// A ledge's top is a floor only where a body stands this far in from its edge, meters.
pub const INSIDE: f64 = 0.5;
/// The top there may stand this far over or under the edge (a sloping or rough top), meters.
pub const STAND_SLOPE: f64 = 0.35;
pub const APPROACH: f64 = 0.8;
/// What a fix leaves of a ledge: the land is raised to at most this under the edge, meters.
pub const CLIMB: f64 = 0.4;
/// The land raised stays at least this under the edge's top where the table allows, and at most
/// this over it, meters.
pub const UNDER_TOP: f64 = 0.03;
pub const OVER_TOP: f64 = 0.25;
/// A ramp runs this far out for each meter it falls (about 42 degrees).
pub const RUN: f64 = 1.1;
/// Room over a ramp's surface kept clear, meters (a human stands 1.835 m).
pub const HEADROOM: f64 = 1.9;
/// How near a face beside a ramp may come to a body standing on it, meters (a human's spheres
/// are 0.48 m); faces are looked for this far out from the ledge, past the ledge's own side.
pub const WALL: f64 = 0.45;
pub const BACK_CLEAR: f64 = 0.5;
/// A human's height, meters.
pub const BODY: f64 = 1.835;
/// A ramp's foot is walked from this far beyond each of its ends, meters ([`cuts_passage`]).
pub const FOOT_WALK_LEAD: f64 = 1.5;
/// A walk along a ramp's foot that got at least this far without the ramp is one a ramp may cut,
/// meters; one that got less than this far with it started inside something.
pub const FOOT_WALK_MIN: f32 = 2.0;
pub const FOOT_WALK_STUCK: f32 = 0.3;

/// Whether a ramp cuts the way along its foot: of the walks there (how far each got without the
/// ramps, and with them), one got at least [`FOOT_WALK_MIN`] without and less than half as far
/// with them (a walk that did not leave its start with them began inside something else, and
/// tells nothing).
#[must_use]
pub fn cuts_passage(walks: &[(f32, f32)]) -> bool {
    walks.iter().any(|&(without, with)| {
        without >= FOOT_WALK_MIN && with >= FOOT_WALK_STUCK && with < 0.5 * without
    })
}

/// A ramp's ridge stands level: a run along a sloping edge is cut into ramps whose tops differ
/// by at most this, meters.
pub const SLOPE_STEP: f64 = 0.1;
/// The highest ramp made, meters (a ledge over a lower floor beyond its foot is followed down).
pub const MAX_RAMP: f64 = 1.6;

type P3 = [f64; 3];

/// A collision polygon in world meters, with its plane.
#[derive(Clone, Debug)]
pub struct Poly {
    pub pts: Vec<P3>,
    pub n: P3,
    pub d: f64,
    pub lo: [f64; 2],
    pub hi: [f64; 2],
    pub zlo: f64,
    pub zhi: f64,
    /// The static it belongs to.
    pub owner: usize,
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The normal AC1 gives a polygon: the sum of the fans' cross products from the first corner,
/// facing the side its corners turn counter-clockwise to.
#[must_use]
pub fn normal(pts: &[P3]) -> Option<P3> {
    let mut n = [0.0; 3];
    for k in 1..pts.len().saturating_sub(1) {
        let c = cross(sub(pts[k], pts[0]), sub(pts[k + 1], pts[0]));
        n = [n[0] + c[0], n[1] + c[1], n[2] + c[2]];
    }
    let l = dot(n, n).sqrt();
    (l > 1e-9).then(|| [n[0] / l, n[1] / l, n[2] / l])
}

impl Poly {
    /// A polygon of `pts`, `None` when they make no plane.
    #[must_use]
    pub fn new(pts: Vec<P3>, owner: usize) -> Option<Self> {
        if pts.len() < 3 {
            return None;
        }
        let n = normal(&pts)?;
        #[allow(clippy::cast_precision_loss)]
        let d = -pts.iter().map(|p| dot(n, *p)).sum::<f64>() / pts.len() as f64;
        let mut lo = [f64::MAX; 2];
        let mut hi = [f64::MIN; 2];
        let (mut zlo, mut zhi) = (f64::MAX, f64::MIN);
        for p in &pts {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
            zlo = zlo.min(p[2]);
            zhi = zhi.max(p[2]);
        }
        Some(Self {
            pts,
            n,
            d,
            lo,
            hi,
            zlo,
            zhi,
            owner,
        })
    }

    /// Whether AC1 walks on it.
    #[must_use]
    pub fn walkable(&self) -> bool {
        self.n[2] >= FLOOR_Z
    }

    /// Whether it stands upright (seen from above, a line).
    fn upright(&self) -> bool {
        self.n[2].abs() < 0.05
    }

    /// Its plane's height over `(x, y)`.
    fn z_at(&self, x: f64, y: f64) -> f64 {
        -(self.n[0] * x + self.n[1] * y + self.d) / self.n[2]
    }

    /// How near, seen from above, it comes to `(x, y)` in the height a body standing at `z`
    /// fills: the least distance to its cut at heights through that body.
    fn reach(&self, x: f64, y: f64, z: f64) -> f64 {
        let mut best = f64::MAX;
        let n = self.pts.len();
        // Heights every tenth of a meter through the body, and its own top and foot where they
        // fall inside, so a thin face is not missed.
        let (lo, hi) = (z + 0.1, z + BODY);
        #[allow(clippy::cast_precision_loss)]
        let heights = (0..=17)
            .map(|k| lo + 0.1 * k as f64)
            .chain([self.zlo, self.zhi]);
        for h in heights {
            if h < lo || h > hi || h < self.zlo || h > self.zhi {
                continue;
            }
            // The polygon's cut at height h: where its edges cross it.
            let mut cut: Vec<(f64, f64)> = Vec::new();
            for i in 0..n {
                let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
                if (a[2] - h) * (b[2] - h) <= 0.0 {
                    let t = if (b[2] - a[2]).abs() < 1e-9 {
                        0.0
                    } else {
                        (h - a[2]) / (b[2] - a[2])
                    };
                    cut.push((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t));
                }
            }
            for i in 0..cut.len() {
                for j in i..cut.len() {
                    let (a, b) = (cut[i], cut[j]);
                    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                    let l2 = dx * dx + dy * dy;
                    let t = if l2 < 1e-12 {
                        0.0
                    } else {
                        (((x - a.0) * dx + (y - a.1) * dy) / l2).clamp(0.0, 1.0)
                    };
                    best =
                        best.min(((a.0 + t * dx - x).powi(2) + (a.1 + t * dy - y).powi(2)).sqrt());
                }
            }
        }
        best
    }

    /// Whether `(x, y)` lies inside it seen from above (convex, either winding).
    fn contains(&self, x: f64, y: f64) -> bool {
        if x < self.lo[0] - 1e-6
            || x > self.hi[0] + 1e-6
            || y < self.lo[1] - 1e-6
            || y > self.hi[1] + 1e-6
        {
            return false;
        }
        let n = self.pts.len();
        let mut sign = 0i32;
        for i in 0..n {
            let a = self.pts[i];
            let b = self.pts[(i + 1) % n];
            let c = (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
            let s = if c > 1e-9 {
                1
            } else if c < -1e-9 {
                -1
            } else {
                0
            };
            if s != 0 {
                if sign == 0 {
                    sign = s;
                } else if s != sign {
                    return false;
                }
            }
        }
        true
    }
}

/// The statics' collision polygons, filed by where they stand.
pub struct Scene {
    pub polys: Vec<Poly>,
    cells: HashMap<(i64, i64), Vec<u32>>,
}

/// The side of a [`Scene`]'s filing cells, meters.
const CELL: f64 = 4.0;

#[allow(clippy::cast_possible_truncation)]
fn cell_of(v: f64) -> i64 {
    (v / CELL).floor() as i64
}

impl Scene {
    #[must_use]
    pub fn new(polys: Vec<Poly>) -> Self {
        let mut cells: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        for (k, p) in polys.iter().enumerate() {
            for i in cell_of(p.lo[0])..=cell_of(p.hi[0]) {
                for j in cell_of(p.lo[1])..=cell_of(p.hi[1]) {
                    cells
                        .entry((i, j))
                        .or_default()
                        .push(u32::try_from(k).unwrap_or(u32::MAX));
                }
            }
        }
        Self { polys, cells }
    }

    /// The polygons whose boxes may reach `(x, y)`.
    fn at(&self, x: f64, y: f64) -> impl Iterator<Item = &Poly> {
        self.cells
            .get(&(cell_of(x), cell_of(y)))
            .into_iter()
            .flatten()
            .map(|&k| &self.polys[k as usize])
            .filter(move |p| {
                x >= p.lo[0] - 1e-6
                    && x <= p.hi[0] + 1e-6
                    && y >= p.lo[1] - 1e-6
                    && y <= p.hi[1] + 1e-6
            })
    }

    /// The polygons whose boxes reach within `r` of `(x, y)`.
    fn near(&self, x: f64, y: f64, r: f64) -> Vec<&Poly> {
        self.in_box([x - r, y - r], [x + r, y + r])
            .into_iter()
            .map(|k| &self.polys[k])
            .collect()
    }

    /// The polygons whose boxes meet the box `lo`..`hi`.
    fn in_box(&self, lo: [f64; 2], hi: [f64; 2]) -> Vec<usize> {
        let mut seen = HashSet::new();
        for i in cell_of(lo[0])..=cell_of(hi[0]) {
            for j in cell_of(lo[1])..=cell_of(hi[1]) {
                for &k in self.cells.get(&(i, j)).into_iter().flatten() {
                    let p = &self.polys[k as usize];
                    if p.hi[0] >= lo[0] && p.lo[0] <= hi[0] && p.hi[1] >= lo[1] && p.lo[1] <= hi[1]
                    {
                        seen.insert(k as usize);
                    }
                }
            }
        }
        let mut v: Vec<usize> = seen.into_iter().collect();
        v.sort_unstable();
        v
    }
}

/// The converted land as the client and Empyrean read it: rounded heights into the table, each
/// square cut one way.
pub struct Land<'a> {
    pub spacing: f64,
    pub side: usize,
    pub table: &'a [f32; table::LEN],
    pub height: &'a [u8],
    /// Each vertex's fine height byte ([`crate::ac2_world::dense::DenseWorld::fine`]).
    pub fine: &'a [u8],
    pub cut: &'a [bool],
    pub water: &'a (dyn Fn(usize) -> bool + Sync),
}

impl Land<'_> {
    /// The square under `(x, y)` and where in it, if on the land's grid.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn square(&self, x: f64, y: f64) -> Option<((usize, usize), f64, f64)> {
        let (u, v) = (x / self.spacing, y / self.spacing);
        if u < 0.0 || v < 0.0 {
            return None;
        }
        let (i, j) = (u.floor() as usize, v.floor() as usize);
        (i + 1 < self.side && j + 1 < self.side).then_some(((i, j), u - i as f64, v - j as f64))
    }

    /// Vertex `k`'s height: the entry `over` sets it to, standing at it, else as the land holds it.
    fn corner(&self, k: usize, over: &HashMap<usize, u8>) -> f64 {
        match over.get(&k) {
            Some(&idx) => f64::from(self.table[usize::from(idx)]),
            None => self.meters(k),
        }
    }

    /// Vertex `k`'s height as the land holds it: its table entry moved by its fine byte.
    fn meters(&self, k: usize) -> f64 {
        let fine = self.fine.get(k).copied().unwrap_or(0);
        f64::from(dereth_primitives::land_grid::fine_height(
            self.table,
            self.height[k],
            fine,
        ))
    }

    fn corners(&self, (i, j): (usize, usize), over: &HashMap<usize, u8>) -> [f64; 4] {
        let s = self.side;
        [
            self.corner(i * s + j, over),
            self.corner((i + 1) * s + j, over),
            self.corner((i + 1) * s + j + 1, over),
            self.corner(i * s + j + 1, over),
        ]
    }

    /// The ground's height at `(x, y)`.
    #[must_use]
    pub fn at(&self, x: f64, y: f64) -> Option<f64> {
        let (sq, fu, fv) = self.square(x, y)?;
        let cut = self.cut[sq.0 * (self.side - 1) + sq.1];
        Some(crate::ac2_world::ground::on_square(
            cut,
            self.corners(sq, &HashMap::new()),
            fu,
            fv,
        ))
    }

    /// The highest the ground could stand at `(x, y)` with the corners `over` names raised,
    /// whichever way its square is cut.
    fn highest_with(&self, x: f64, y: f64, over: &HashMap<usize, u8>) -> Option<f64> {
        let (sq, fu, fv) = self.square(x, y)?;
        let c = self.corners(sq, over);
        Some(
            crate::ac2_world::ground::on_square(true, c, fu, fv)
                .max(crate::ac2_world::ground::on_square(false, c, fu, fv)),
        )
    }
}

/// The floor just outside a ledge: the terrain, or a static's walkable polygon at or under `top`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Floor {
    Terrain(f64),
    Static(f64),
}

impl Floor {
    #[must_use]
    pub fn z(self) -> f64 {
        match self {
            Self::Terrain(z) | Self::Static(z) => z,
        }
    }
}

/// Why a point is no floor to step up from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoFloor {
    OffLand,
    Buried,
    Covered,
}

/// The floor under `(x, y)` at or under `top`; none when the ground stands over `top`, or
/// something stands over the point within a body's height.
pub fn floor(scene: &Scene, land: &Land, x: f64, y: f64, top: f64) -> Result<Floor, NoFloor> {
    let t = land.at(x, y).ok_or(NoFloor::OffLand)?;
    if t > top + 0.05 {
        return Err(NoFloor::Buried);
    }
    let mut best = Floor::Terrain(t);
    let near: Vec<&Poly> = scene
        .at(x, y)
        .filter(|p| !p.upright() && p.contains(x, y))
        .collect();
    for p in &near {
        let z = p.z_at(x, y);
        if p.walkable() && z <= top + 0.05 && z > best.z() {
            best = Floor::Static(z);
        }
    }
    let b = best.z();
    for p in &near {
        let z = p.z_at(x, y);
        if z > b + 0.3 && z <= top + 1.8 && z > top + 0.05 {
            return Err(NoFloor::Covered);
        }
        // A steep face over the floor and under the edge: a slope of a static, not open ground.
        if z > b + 0.3 && z <= top + 0.05 && !p.walkable() {
            return Err(NoFloor::Covered);
        }
    }
    Ok(best)
}

/// A band ledge: a point of a walkable polygon's edge, with the floor outside it.
#[derive(Clone, Copy, Debug)]
pub struct Ledge {
    pub owner: usize,
    /// The polygon, its edge (from corner `edge` to the next), the sample along it and how many.
    pub poly: usize,
    pub edge: usize,
    pub step: usize,
    pub steps: usize,
    pub p: P3,
    pub out: [f64; 2],
    pub floor: Floor,
}

#[cfg(test)]
impl Ledge {
    /// How far the ledge stands over the floor below it.
    fn rise(&self) -> f64 {
        self.p[2] - self.floor.z()
    }
}

/// Run `f` over `0..n` on every core, handing out work as it is done; the results in order.
fn par<T: Send, F: Fn(usize) -> T + Sync>(n: usize, f: F) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    let next = AtomicUsize::new(0);
    let parts: Vec<Vec<(usize, T)>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= n {
                            break out;
                        }
                        out.push((i, f(i)));
                    }
                })
            })
            .collect();
        hs.into_iter()
            .map(|h| h.join().expect("a worker"))
            .collect()
    });
    let mut all: Vec<(usize, T)> = parts.into_iter().flatten().collect();
    all.sort_by_key(|p| p.0);
    all.into_iter().map(|p| p.1).collect()
}

/// Every band ledge of the walkable polygons of the statics `fixable` names.
#[must_use]
pub fn band_ledges(
    scene: &Scene,
    land: &Land,
    fixable: &(dyn Fn(usize) -> bool + Sync),
) -> Vec<Ledge> {
    let chunk = 512;
    let n = scene.polys.len().div_ceil(chunk);
    par(n, |c| {
        let mut out = Vec::new();
        for k in c * chunk..((c + 1) * chunk).min(scene.polys.len()) {
            let wp = &scene.polys[k];
            if !wp.walkable() || !fixable(wp.owner) {
                continue;
            }
            out.extend(poly_ledges(scene, land, k));
        }
        out
    })
    .into_iter()
    .flatten()
    .collect()
}

/// The band ledges along polygon `k`'s edges.
fn poly_ledges(scene: &Scene, land: &Land, k: usize) -> Vec<Ledge> {
    let wp = &scene.polys[k];
    let vs = &wp.pts;
    let n = vs.len();
    #[allow(clippy::cast_precision_loss)]
    let (cx, cy) = vs.iter().fold((0.0, 0.0), |a, p| {
        (a.0 + p[0] / n as f64, a.1 + p[1] / n as f64)
    });
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (vs[i], vs[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.1 {
            continue;
        }
        let mut o = [dy / len, -dx / len];
        let mid = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
        if (mid.0 - cx) * o[0] + (mid.1 - cy) * o[1] < 0.0 {
            o = [-o[0], -o[1]];
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (len / SAMPLE).ceil().max(1.0) as usize;
        for s in 0..steps {
            #[allow(clippy::cast_precision_loss)]
            let t = (s as f64 + 0.5) / steps as f64;
            let p = [a[0] + dx * t, a[1] + dy * t, a[2] + (b[2] - a[2]) * t];
            let Ok(f) = floor(scene, land, p[0] + o[0] * OUT, p[1] + o[1] * OUT, p[2]) else {
                continue;
            };
            let rise = p[2] - f.z();
            if rise > RISE_LO
                && rise <= RISE_HI
                && standable(scene, p[0] - o[0] * INSIDE, p[1] - o[1] * INSIDE, p[2])
            {
                out.push(Ledge {
                    owner: wp.owner,
                    poly: k,
                    edge: i,
                    step: s,
                    steps,
                    p,
                    out: o,
                    floor: f,
                });
            }
        }
    }
    out
}

/// Whether a body could stand at `(x, y)` on a walkable polygon within [`STAND_SLOPE`] of `z`,
/// with room over it.
fn standable(scene: &Scene, x: f64, y: f64, z: f64) -> bool {
    let near: Vec<&Poly> = scene
        .at(x, y)
        .filter(|p| !p.upright() && p.contains(x, y))
        .collect();
    let Some(f) = near
        .iter()
        .filter(|p| p.walkable())
        .map(|p| p.z_at(x, y))
        .filter(|&pz| (pz - z).abs() <= STAND_SLOPE)
        .reduce(f64::max)
    else {
        return false;
    };
    !near.iter().any(|p| {
        let pz = p.z_at(x, y);
        pz > f + 0.05 && pz <= f + 1.8
    })
}

/// What raising the land at the ledges onto the terrain asks.
#[derive(Debug, Default)]
pub struct Swell {
    /// The heights to raise vertices to, by index (each a height of the table).
    pub wants: BTreeMap<usize, f64>,
    /// The ledges it serves, and those it could not (a corner on water or on a held square, no
    /// height of the table fitting, or a raise that would cover a static's floor).
    pub served: Vec<usize>,
    pub refused: Vec<usize>,
    /// The ledges refused, by why.
    pub why: BTreeMap<&'static str, usize>,
}

/// The land raised at each of `ledges` whose floor is the terrain; `held` names squares never
/// raised.
#[must_use]
pub fn swell(
    ledges: &[Ledge],
    scene: &Scene,
    land: &Land,
    held: &(dyn Fn(usize, usize) -> bool + Sync),
) -> Swell {
    let side = land.side;
    let mut out = Swell::default();
    // Each ledge's ask: the corners it raises and to which height of the table.
    let mut asks: Vec<(usize, Vec<(usize, u8)>)> = Vec::new();
    for (li, l) in ledges.iter().enumerate() {
        if !matches!(l.floor, Floor::Terrain(_)) {
            continue;
        }
        let (lo, hi) = (l.p[2] - CLIMB, l.p[2] - UNDER_TOP);
        let mut squares = Vec::new();
        for r in [OUT, APPROACH] {
            if let Some((sq, _, _)) = land.square(l.p[0] + l.out[0] * r, l.p[1] + l.out[1] * r) {
                if !squares.contains(&sq) {
                    squares.push(sq);
                }
            }
        }
        let mut ask = Vec::new();
        let mut ok = !squares.is_empty();
        let mut why = "off the land";
        'squares: for &(i, j) in &squares {
            for k in [
                i * side + j,
                (i + 1) * side + j,
                (i + 1) * side + j + 1,
                i * side + j + 1,
            ] {
                let cur = land.meters(k);
                if cur >= lo {
                    continue;
                }
                // The first height of the table no lower than the edge less CLIMB, under the top;
                // else the highest under the top AC1 climbs from; else the first just over the top
                // (where it is not under the top itself, which the check below refuses).
                let mut idx = usize::from(crate::ac2_world::buried::at_least(land.table, lo));
                if f64::from(land.table[idx]) > hi {
                    let below = idx.saturating_sub(1);
                    if f64::from(land.table[below]) >= l.p[2] - RISE_LO {
                        idx = below;
                    }
                }
                let h = f64::from(land.table[idx]);
                if h <= cur && cur >= l.p[2] - RISE_LO {
                    continue;
                }
                if (land.water)(k) || touches(k, side).any(|(a, b)| held(a, b)) {
                    ok = false;
                    why = if (land.water)(k) {
                        "a corner on water"
                    } else {
                        "a building's land"
                    };
                    break 'squares;
                }
                if h > l.p[2] + OVER_TOP || h <= cur || h < l.p[2] - RISE_LO {
                    ok = false;
                    why = "no height of the table fits";
                    break 'squares;
                }
                ask.push((k, u8::try_from(idx).unwrap_or(u8::MAX)));
            }
        }
        if ok {
            asks.push((li, ask));
        } else {
            out.refused.push(li);
            *out.why.entry(why).or_default() += 1;
        }
    }
    // Each raise alone, then all taken together, must cover no static's floor they were under: a
    // square where they would drops every ask raising one of its corners, until none does.
    let alone = par(asks.len(), |a| {
        let over: HashMap<usize, u8> = asks[a].1.iter().copied().collect();
        let mut squares: Vec<(usize, usize)> =
            over.keys().flat_map(|&k| touches(k, side)).collect();
        squares.sort_unstable();
        squares.dedup();
        !squares
            .iter()
            .any(|&sq| covers_a_floor(scene, land, sq, &over))
    });
    let mut alive = alone;
    loop {
        let mut over: HashMap<usize, u8> = HashMap::new();
        for (a, (_, ask)) in asks.iter().enumerate() {
            if !alive[a] {
                continue;
            }
            for &(k, idx) in ask {
                let e = over.entry(k).or_insert(idx);
                *e = (*e).max(idx);
            }
        }
        let mut squares: Vec<(usize, usize)> =
            over.keys().flat_map(|&k| touches(k, side)).collect();
        squares.sort_unstable();
        squares.dedup();
        let bad: Vec<bool> = par(squares.len(), |s| {
            covers_a_floor(scene, land, squares[s], &over)
        });
        let bad: HashSet<(usize, usize)> = squares
            .iter()
            .zip(bad)
            .filter(|(_, b)| *b)
            .map(|(s, _)| *s)
            .collect();
        if bad.is_empty() {
            break;
        }
        for (a, (_, ask)) in asks.iter().enumerate() {
            if alive[a]
                && ask
                    .iter()
                    .any(|&(k, _)| touches(k, side).any(|s| bad.contains(&s)))
            {
                alive[a] = false;
            }
        }
    }
    for (a, (li, ask)) in asks.iter().enumerate() {
        if !alive[a] {
            out.refused.push(*li);
            *out.why
                .entry("it would cover a static's floor")
                .or_default() += 1;
            continue;
        }
        out.served.push(*li);
        for &(k, idx) in ask {
            let h = f64::from(land.table[usize::from(idx)]);
            let e = out.wants.entry(k).or_insert(h);
            *e = e.max(h);
        }
    }
    out.refused.sort_unstable();
    out
}

/// The squares vertex `k` is a corner of.
fn touches(k: usize, side: usize) -> impl Iterator<Item = (usize, usize)> {
    let (a, b) = (k / side, k % side);
    [
        (a, b),
        (a.wrapping_sub(1), b),
        (a, b.wrapping_sub(1)),
        (a.wrapping_sub(1), b.wrapping_sub(1)),
    ]
    .into_iter()
    .filter(move |&(i, j)| i < side - 1 && j < side - 1)
}

/// Whether the land with `over`'s corners raised would stand over a static's walkable polygon in
/// square `sq` that it stood under.
fn covers_a_floor(
    scene: &Scene,
    land: &Land,
    (i, j): (usize, usize),
    over: &HashMap<usize, u8>,
) -> bool {
    #[allow(clippy::cast_precision_loss)]
    let lo = [i as f64 * land.spacing, j as f64 * land.spacing];
    let hi = [lo[0] + land.spacing, lo[1] + land.spacing];
    for k in scene.in_box(lo, hi) {
        let p = &scene.polys[k];
        if !p.walkable() {
            continue;
        }
        let mut pts: Vec<(f64, f64)> = p.pts.iter().map(|v| (v[0], v[1])).collect();
        let (x0, y0) = (p.lo[0].max(lo[0]), p.lo[1].max(lo[1]));
        let (x1, y1) = (p.hi[0].min(hi[0]), p.hi[1].min(hi[1]));
        let mut x = x0;
        while x <= x1 {
            let mut y = y0;
            while y <= y1 {
                pts.push((x, y));
                y += 1.0;
            }
            x += 1.0;
        }
        for (x, y) in pts {
            if x < lo[0] || x > hi[0] || y < lo[1] || y > hi[1] || !p.contains(x, y) {
                continue;
            }
            let z = p.z_at(x, y);
            let (Some(before), Some(after)) = (land.at(x, y), land.highest_with(x, y, over)) else {
                continue;
            };
            if before <= z + 0.02 && after > z + 0.02 {
                return true;
            }
        }
    }
    false
}

/// A collision-only wedge: its faces, each turned outward, world meters.
#[derive(Clone, Debug)]
pub struct Ramp {
    pub owner: usize,
    pub faces: Vec<Vec<P3>>,
    /// Its ridge's ends (on the ledge) and its foot's height.
    pub ridge: [P3; 2],
    pub out: [f64; 2],
    pub base: f64,
}

/// What [`ramps`] made and what it left.
#[derive(Debug, Default)]
pub struct Ramps {
    pub made: Vec<Ramp>,
    /// Runs of ledges no ramp was made for: something over the ramp within a body's height, or a
    /// floor falling away under it farther than [`MAX_RAMP`] from the top.
    pub blocked: usize,
    pub too_deep: usize,
}

/// One stretch of a ledge to ramp: its placement, its ridge's ends (level, at the lower top),
/// which way it faces, and the floor at its foot.
#[derive(Clone, Copy, Debug)]
struct Span {
    owner: usize,
    ra: P3,
    rb: P3,
    o: [f64; 2],
    floor: f64,
}

impl Span {
    fn top(&self) -> f64 {
        self.ra[2].min(self.rb[2])
    }
}

/// A ramp for every run of neighboring `ledges` along one edge, runs along one line joined.
#[must_use]
pub fn ramps(ledges: &[Ledge], scene: &Scene, land: &Land) -> Ramps {
    let mut runs: BTreeMap<(usize, usize), Vec<&Ledge>> = BTreeMap::new();
    for l in ledges {
        runs.entry((l.poly, l.edge)).or_default().push(l);
    }
    let mut spans: Vec<Span> = Vec::new();
    for ((poly, edge), mut v) in runs {
        v.sort_by_key(|l| l.step);
        let p = &scene.polys[poly];
        let (a, b) = (p.pts[edge], p.pts[(edge + 1) % p.pts.len()]);
        let mut cur: Vec<&Ledge> = Vec::new();
        let mut close = |cur: &mut Vec<&Ledge>| {
            let (Some(first), Some(last)) = (cur.first(), cur.last()) else {
                return;
            };
            #[allow(clippy::cast_precision_loss)]
            let at = |t: f64| {
                [
                    a[0] + (b[0] - a[0]) * t,
                    a[1] + (b[1] - a[1]) * t,
                    a[2] + (b[2] - a[2]) * t,
                ]
            };
            #[allow(clippy::cast_precision_loss)]
            let (ra, rb) = (
                at(first.step as f64 / first.steps as f64),
                at((last.step + 1) as f64 / first.steps as f64),
            );
            let top = ra[2].min(rb[2]);
            spans.push(Span {
                owner: first.owner,
                ra: [ra[0], ra[1], top],
                rb: [rb[0], rb[1], top],
                o: first.out,
                floor: cur.iter().map(|l| l.floor.z()).fold(f64::MAX, f64::min),
            });
            cur.clear();
        };
        for l in v {
            // A run along a sloping edge is cut where its top falls more than SLOPE_STEP, as each
            // ramp's ridge stands level at the lowest of its run's top.
            if cur.last().is_some_and(|c| c.step + 1 != l.step)
                || cur
                    .first()
                    .is_some_and(|c| (c.p[2] - l.p[2]).abs() > SLOPE_STEP)
            {
                close(&mut cur);
            }
            cur.push(l);
        }
        close(&mut cur);
    }
    let spans = join(spans);
    let made = par(spans.len(), |s| ramp_for(&spans[s], scene, land));
    let mut out = Ramps::default();
    for r in made {
        match r {
            Ok(r) => out.made.push(r),
            Err(Refused::Blocked) => out.blocked += 1,
            Err(Refused::TooDeep) => out.too_deep += 1,
        }
    }
    out
}

/// Spans of one placement end to end along one line, facing one way at one height, joined.
fn join(mut spans: Vec<Span>) -> Vec<Span> {
    // Each span's ridge running one way along its line: the out direction turned a quarter.
    for s in &mut spans {
        let e = [-s.o[1], s.o[0]];
        if (s.rb[0] - s.ra[0]) * e[0] + (s.rb[1] - s.ra[1]) * e[1] < 0.0 {
            std::mem::swap(&mut s.ra, &mut s.rb);
        }
    }
    let along = |s: &Span| s.ra[0] * -s.o[1] + s.ra[1] * s.o[0];
    spans.sort_by(|x, y| x.owner.cmp(&y.owner).then(along(x).total_cmp(&along(y))));
    let mut out: Vec<Span> = Vec::new();
    let mut open: Vec<Span> = Vec::new();
    let mut owner = usize::MAX;
    for s in spans {
        if s.owner != owner {
            out.append(&mut open);
            owner = s.owner;
        }
        let fits = |j: &Span| {
            let e = [-j.o[1], j.o[0]];
            let same_way = j.o[0] * s.o[0] + j.o[1] * s.o[1] > 0.999;
            let off = (s.ra[0] - j.rb[0]) * j.o[0] + (s.ra[1] - j.rb[1]) * j.o[1];
            let gap = (s.ra[0] - j.rb[0]) * e[0] + (s.ra[1] - j.rb[1]) * e[1];
            same_way
                && off.abs() < 0.05
                && (-0.05..0.3).contains(&gap)
                && (j.top() - s.top()).abs() <= SLOPE_STEP
        };
        if let Some(j) = open.iter_mut().find(|j| fits(j)) {
            let top = j.top().min(s.top());
            j.rb = s.rb;
            j.ra[2] = top;
            j.rb[2] = top;
            j.floor = j.floor.min(s.floor);
        } else {
            open.push(s);
        }
    }
    out.append(&mut open);
    out
}

#[derive(Debug, PartialEq, Eq)]
enum Refused {
    Blocked,
    TooDeep,
}

/// The ramp for one span.
fn ramp_for(span: &Span, scene: &Scene, land: &Land) -> Result<Ramp, Refused> {
    let (ra, rb, o) = (span.ra, span.rb, span.o);
    let top = span.top();
    let len = ((rb[0] - ra[0]).powi(2) + (rb[1] - ra[1]).powi(2))
        .sqrt()
        .max(1e-6);
    let e = [(rb[0] - ra[0]) / len, (rb[1] - ra[1]) / len];
    // Where the floor falls away under the ramp by more than AC1 climbs, its foot follows the
    // floor down (a ramp left standing over the fall would jut over the ground below, a wall to
    // whoever walks there).
    let mut base = span.floor - 0.02;
    for _ in 0..4 {
        if top - base > MAX_RAMP {
            return Err(Refused::TooDeep);
        }
        let low = lowest_floor(scene, land, ra, e, o, len, top, base);
        if low >= base - RISE_LO {
            return if clear(scene, land, ra, e, o, len, top, base) {
                Ok(wedge(span.owner, ra, rb, e, o, top, base))
            } else {
                Err(Refused::Blocked)
            };
        }
        base = low - 0.02;
    }
    Err(Refused::TooDeep)
}

/// The points of a ramp's surface, every quarter meter: their place and the surface's height.
#[allow(clippy::too_many_arguments)]
fn surface(
    a: P3,
    e: [f64; 2],
    o: [f64; 2],
    len: f64,
    top: f64,
    base: f64,
) -> Vec<(f64, f64, f64, f64)> {
    let run = (top - base) * RUN;
    let ends = run;
    let fall = |d: f64| top - d / RUN;
    let mut pts = Vec::new();
    let mut s = -ends;
    while s <= len + ends + 1e-9 {
        let mut u = 0.0;
        while u <= run + 1e-9 {
            let beyond = if s < 0.0 {
                -s
            } else if s > len {
                s - len
            } else {
                0.0
            };
            if beyond <= u.max(beyond) && fall(u.max(beyond)) >= base - 1e-9 {
                let (x, y) = (a[0] + e[0] * s + o[0] * u, a[1] + e[1] * s + o[1] * u);
                pts.push((x, y, u, fall(u.max(beyond))));
            }
            u += 0.25;
        }
        s += 0.25;
    }
    pts
}

/// The lowest floor under a ramp's surface.
#[allow(clippy::too_many_arguments)]
fn lowest_floor(
    scene: &Scene,
    land: &Land,
    a: P3,
    e: [f64; 2],
    o: [f64; 2],
    len: f64,
    top: f64,
    base: f64,
) -> f64 {
    let mut low = f64::MAX;
    for (x, y, u, z) in surface(a, e, o, len, top, base) {
        if u < OUT {
            continue;
        }
        let mut f = land.at(x, y).unwrap_or(f64::MIN);
        for p in scene.at(x, y) {
            if p.walkable() && p.contains(x, y) {
                let pz = p.z_at(x, y);
                if pz <= z + 0.05 && pz > f {
                    f = pz;
                }
            }
        }
        low = low.min(f);
    }
    low
}

/// Whether a ramp takes no room a body had: no face turned down (a lintel's, a roof's underside)
/// that stood at least [`HEADROOM`] over the floor before comes nearer the ramp's surface than
/// that, so it raises a doorway's or a passage's floor no nearer its lintel or roof than a body
/// needs. A face turned up over it is the top of something the ramp stands inside, which a body
/// under it passes, as AC1 collides only with a face's front. Its faces all slope no steeper than
/// AC1 walks, so it narrows no way at the floor.
#[allow(clippy::too_many_arguments)]
fn clear(
    scene: &Scene,
    land: &Land,
    a: P3,
    e: [f64; 2],
    o: [f64; 2],
    len: f64,
    top: f64,
    base: f64,
) -> bool {
    for (x, y, u, z) in surface(a, e, o, len, top, base) {
        if u < OUT {
            continue;
        }
        let over: Vec<&Poly> = scene
            .at(x, y)
            .filter(|p| !p.upright() && p.contains(x, y))
            .collect();
        for p in over.iter().filter(|p| p.n[2] < -0.05) {
            let pz = p.z_at(x, y);
            if pz <= z + 0.05 || pz > z + HEADROOM {
                continue;
            }
            // The floor under that face before the ramp.
            let mut f = land.at(x, y).unwrap_or(f64::MIN);
            for q in over.iter().filter(|q| q.walkable()) {
                let qz = q.z_at(x, y);
                if qz < pz && qz > f {
                    f = qz;
                }
            }
            if pz - f >= HEADROOM {
                return false;
            }
        }
        // A face beside the ramp a body standing on it would meet, that a body on the floor there
        // passed clear of (a wall leaning out over the foot of the ledge, a doorway's jamb).
        let f = floor_under(scene, land, x, y, z);
        if u >= BACK_CLEAR && z > f + 0.05 {
            for p in scene.near(x, y, WALL).into_iter().filter(|p| !p.walkable()) {
                if p.reach(x, y, z) < WALL && p.reach(x, y, f) >= WALL {
                    return false;
                }
            }
        }
    }
    true
}

/// The floor under `(x, y)` at or under `z`: the terrain or a static's walkable polygon.
fn floor_under(scene: &Scene, land: &Land, x: f64, y: f64, z: f64) -> f64 {
    let mut f = land.at(x, y).unwrap_or(f64::MIN);
    for p in scene.at(x, y) {
        if p.walkable() && p.contains(x, y) {
            let pz = p.z_at(x, y);
            if pz <= z + 0.05 && pz > f {
                f = pz;
            }
        }
    }
    f
}

/// A ramp's closed wedge: the ridge on the ledge from `ra` to `rb` at `top`, its front and ends
/// falling 1 in [`RUN`] to `base`.
#[allow(clippy::too_many_arguments)]
fn wedge(owner: usize, ra: P3, rb: P3, e: [f64; 2], o: [f64; 2], top: f64, base: f64) -> Ramp {
    let run = (top - base) * RUN;
    let ends = run;
    let p = |x: f64, y: f64, z: f64| [x, y, z];
    let ta = p(ra[0], ra[1], top);
    let tb = p(rb[0], rb[1], top);
    let p0 = p(ra[0] - e[0] * ends, ra[1] - e[1] * ends, base);
    let p1 = p(rb[0] + e[0] * ends, rb[1] + e[1] * ends, base);
    let p2 = p(p1[0] + o[0] * run, p1[1] + o[1] * run, base);
    let p3 = p(p0[0] + o[0] * run, p0[1] + o[1] * run, base);
    let all = [ta, tb, p0, p1, p2, p3];
    let c = all.iter().fold([0.0; 3], |s, v| {
        [s[0] + v[0] / 6.0, s[1] + v[1] / 6.0, s[2] + v[2] / 6.0]
    });
    let mut faces = vec![
        vec![ta, p3, p2, tb],
        vec![ta, p0, p3],
        vec![tb, p2, p1],
        vec![p0, ta, tb, p1],
        vec![p0, p1, p2, p3],
    ];
    for f in &mut faces {
        #[allow(clippy::cast_precision_loss)]
        let fc = f
            .iter()
            .fold([0.0; 3], |s, v| [s[0] + v[0], s[1] + v[1], s[2] + v[2]])
            .map(|v| v / f.len() as f64);
        if normal(f).is_some_and(|n| dot(n, sub(fc, c)) < 0.0) {
            f.reverse();
        }
    }
    Ramp {
        owner,
        faces,
        ridge: [ta, tb],
        out: o,
        base,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's ledges)
    use super::*;

    /// A flat land 9 x 9 vertices 8 m apart, every vertex at `h` (a table of steps of 0.1 m).
    struct Flat {
        table: [f32; table::LEN],
        height: Vec<u8>,
        fine: Vec<u8>,
        cut: Vec<bool>,
    }

    const SIDE: usize = 9;

    fn flat(h: f64) -> Flat {
        let mut t = [0f32; table::LEN];
        for (k, v) in t.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let x = k as f32 * 0.1;
            *v = x;
        }
        let idx = crate::ac2_world::buried::at_least(&t, h);
        Flat {
            table: t,
            height: vec![idx; SIDE * SIDE],
            fine: vec![0; SIDE * SIDE],
            cut: vec![true; (SIDE - 1) * (SIDE - 1)],
        }
    }

    fn dry(_: usize) -> bool {
        false
    }

    fn land(f: &Flat) -> Land<'_> {
        Land {
            spacing: 8.0,
            side: SIDE,
            table: &f.table,
            height: &f.height,
            fine: &f.fine,
            cut: &f.cut,
            water: &dry,
        }
    }

    fn quad(x0: f64, y0: f64, x1: f64, y1: f64, z: f64, owner: usize) -> Poly {
        Poly::new(
            vec![[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]],
            owner,
        )
        .unwrap()
    }

    /// An upright wall along x from `x0` to `x1` at `y`, from `z0` to `z1`.
    fn wall_x(x0: f64, x1: f64, y: f64, z0: f64, z1: f64, owner: usize) -> Poly {
        Poly::new(
            vec![[x0, y, z0], [x1, y, z0], [x1, y, z1], [x0, y, z1]],
            owner,
        )
        .unwrap()
    }

    /// A box-shaped platform's top, 4 m a side, its top `rise` over the 10 m ground.
    fn platform(rise: f64) -> Poly {
        quad(30.0, 30.0, 34.0, 34.0, 10.0 + rise, 0)
    }

    fn all(_: usize) -> bool {
        true
    }

    fn held_none(_: usize, _: usize) -> bool {
        false
    }

    #[test]
    fn a_band_ledge_onto_the_terrain_gets_the_land_raised_to_within_ac1s_climb() {
        let f = flat(10.0);
        let l = land(&f);
        let scene = Scene::new(vec![platform(0.8)]);
        let found = band_ledges(&scene, &l, &all);
        assert!(!found.is_empty());
        assert!(found
            .iter()
            .all(|x| matches!(x.floor, Floor::Terrain(_)) && (x.rise() - 0.8).abs() < 0.11));
        let sw = swell(&found, &scene, &l, &held_none);
        assert!(sw.refused.is_empty(), "{:?}", sw.refused);
        assert_eq!(sw.served.len(), found.len());
        // Raised as the converter raises it, then measured again.
        let mut raised = f.height.clone();
        for (&k, &h) in &sw.wants {
            raised[k] = crate::ac2_world::buried::at_least(&f.table, h);
        }
        let after = Land {
            height: &raised,
            fine: &f.fine,
            ..land(&f)
        };
        for x in &found {
            let g = after
                .at(x.p[0] + x.out[0] * OUT, x.p[1] + x.out[1] * OUT)
                .unwrap();
            let rise = x.p[2] - g;
            assert!((UNDER_TOP..=CLIMB + 1e-6).contains(&rise), "{rise}");
        }
        // No band ledge is left, and the land under the top stays under it.
        assert!(band_ledges(&scene, &after, &all).is_empty());
        for &k in sw.wants.keys() {
            assert!(f64::from(f.table[usize::from(raised[k])]) < 10.8);
        }
    }

    #[test]
    fn a_ledge_onto_another_static_gets_a_ramp_no_steeper_than_45_degrees() {
        let f = flat(5.0);
        let l = land(&f);
        // A wide deck at 10 m (static 1) with the platform's top 0.8 m over it (static 0).
        let scene = Scene::new(vec![platform(0.8), quad(20.0, 20.0, 44.0, 44.0, 10.0, 1)]);
        let found = band_ledges(&scene, &l, &|o| o == 0);
        assert!(!found.is_empty());
        assert!(found.iter().all(|x| matches!(x.floor, Floor::Static(_))));
        // The deck's own edge stands 5 m over the land: no band ledge.
        assert!(found.iter().all(|x| x.owner == 0));
        let r = ramps(&found, &scene, &l);
        assert_eq!(r.made.len(), 4, "{r:?}");
        for ramp in &r.made {
            for face in &ramp.faces {
                let n = normal(face).unwrap();
                // Every face slopes no steeper than 45 degrees or stands upright (its ends and back),
                // or faces down (its foot).
                assert!(
                    n[2].abs() < 1e-6 || n[2] >= std::f64::consts::FRAC_1_SQRT_2 || n[2] < -0.999,
                    "{n:?}"
                );
                assert!(n[2] < 1e-6 || n[2] >= FLOOR_Z);
            }
            assert!((ramp.ridge[0][2] - 10.8).abs() < 1e-6);
            assert!(ramp.base <= 10.0 && ramp.base > 9.9);
        }
        // With the ramps in, the platform's edges are no ledges.
        let mut polys = scene.polys.clone();
        for ramp in &r.made {
            for face in &ramp.faces {
                polys.push(Poly::new(face.clone(), 2).unwrap());
            }
        }
        let with = Scene::new(polys);
        assert!(band_ledges(&with, &l, &|o| o == 0).is_empty());
    }

    #[test]
    fn a_doorway_in_front_of_a_ledge_stays_clear() {
        let f = flat(5.0);
        let l = land(&f);
        // The deck, the platform, and a doorway's lintel 2 m over the deck 0.8 m south of the
        // platform's south edge (its top and its underside), with its jambs' walls at x 31.5 and
        // 32.5.
        let mut under = quad(31.0, 28.8, 33.0, 29.4, 12.0, 3);
        under.pts.reverse();
        let under = Poly::new(under.pts, 3).unwrap();
        assert!(under.n[2] < -0.99);
        let scene = Scene::new(vec![
            platform(0.8),
            quad(20.0, 20.0, 44.0, 44.0, 10.0, 1),
            quad(31.0, 28.8, 33.0, 29.4, 12.3, 3),
            under,
            Poly::new(
                vec![
                    [31.5, 28.8, 10.0],
                    [31.5, 29.4, 10.0],
                    [31.5, 29.4, 12.0],
                    [31.5, 28.8, 12.0],
                ],
                3,
            )
            .unwrap(),
            Poly::new(
                vec![
                    [32.5, 28.8, 10.0],
                    [32.5, 29.4, 10.0],
                    [32.5, 29.4, 12.0],
                    [32.5, 28.8, 12.0],
                ],
                3,
            )
            .unwrap(),
        ]);
        let found = band_ledges(&scene, &l, &|o| o == 0);
        let r = ramps(&found, &scene, &l);
        // The south edge's ramp would stand in the doorway, under its lintel: not made.
        assert_eq!(r.made.len(), 3, "{r:?}");
        assert_eq!(r.blocked, 1);
        for ramp in &r.made {
            for face in &ramp.faces {
                for v in face {
                    assert!(v[1] > 29.4 || v[0] < 31.0 || v[0] > 33.0, "{v:?}");
                }
            }
        }
        // And land raised for a ledge never covers a doorway's floor: a threshold at 10.1 m on the
        // land square a raise would lift past it.
        let g = flat(10.0);
        let gl = land(&g);
        let threshold = quad(34.5, 31.0, 35.5, 33.0, 10.1, 4);
        let scene = Scene::new(vec![
            platform(0.8),
            threshold,
            wall_x(34.5, 35.5, 31.0, 10.1, 12.0, 4),
        ]);
        let found = band_ledges(&scene, &gl, &|o| o == 0);
        let sw = swell(&found, &scene, &gl, &held_none);
        assert!(sw.served.is_empty() && !sw.refused.is_empty(), "{sw:?}");
        assert!(sw.wants.is_empty());
    }

    #[test]
    fn a_wall_leaning_over_a_ledges_foot_keeps_its_passage() {
        let f = flat(5.0);
        let l = land(&f);
        // A wall along the platform's south side, its foot 1.6 m out and leaning 0.32 m in for
        // each meter up: a body on the deck passes between, one raised on a ramp would not.
        let wall = Poly::new(
            vec![
                [31.0, 28.4, 10.0],
                [33.0, 28.4, 10.0],
                [33.0, 29.2, 12.5],
                [31.0, 29.2, 12.5],
            ],
            3,
        )
        .unwrap();
        let scene = Scene::new(vec![
            platform(0.8),
            quad(20.0, 20.0, 44.0, 44.0, 10.0, 1),
            wall,
        ]);
        let found = band_ledges(&scene, &l, &|o| o == 0);
        let r = ramps(&found, &scene, &l);
        assert_eq!(r.made.len(), 3, "{r:?}");
        assert_eq!(r.blocked, 1);
        assert!(r.made.iter().all(|ramp| ramp.out[1] > -0.5));
    }

    #[test]
    fn a_ramp_whose_end_stops_a_walk_along_its_foot_is_left_out() {
        // A walk along the foot gets 4.6 m on flat ground without the ramp; with it, one way it
        // goes over the ramp and on, the other it stops against the ramp's end after 0.8 m.
        let over = (4.6, 5.1);
        let stopped = (4.6, 0.8);
        assert!(cuts_passage(&[over, over, over, stopped, over, over]));
        // A ramp the walk goes over each way, or only slows a little on, stays.
        assert!(!cuts_passage(&[over, (4.6, 3.9), over, over, over, over]));
        // A walk that never left its start with the ramps began inside something else; one that
        // got nowhere without them had no way to cut.
        assert!(!cuts_passage(&[(4.6, 0.1), (1.2, 0.2)]));
    }

    #[test]
    fn edges_ac1_climbs_or_ac2_does_not_are_left_untouched() {
        let f = flat(10.0);
        let l = land(&f);
        for rise in [0.3, 0.5, 1.2, 2.0] {
            let scene = Scene::new(vec![platform(rise)]);
            assert!(band_ledges(&scene, &l, &all).is_empty(), "{rise}");
        }
        // Ground to ground: nothing to raise, no ramp.
        let scene = Scene::new(Vec::new());
        assert!(band_ledges(&scene, &l, &all).is_empty());
        // A ledge whose outside is covered (an overhang within a body's height) is no approach.
        let scene = Scene::new(vec![platform(0.8), quad(28.0, 28.0, 36.0, 36.0, 11.5, 1)]);
        assert!(band_ledges(&scene, &l, &|o| o == 0).is_empty());
    }

    #[test]
    fn a_ramp_follows_its_floor_down_where_it_falls_away() {
        let f = flat(5.0);
        let l = land(&f);
        // A tread 0.4 m wide at 10 m round the platform, then a deck at 9.3 m: a fall under the
        // ramp more than AC1 climbs.
        let scene = Scene::new(vec![
            platform(0.8),
            quad(29.6, 29.6, 34.4, 34.4, 10.0, 1),
            quad(20.0, 20.0, 44.0, 44.0, 9.3, 2),
        ]);
        let found = band_ledges(&scene, &l, &|o| o == 0);
        let r = ramps(&found, &scene, &l);
        assert_eq!(r.made.len(), 4, "{r:?}");
        for ramp in &r.made {
            assert!(ramp.base <= 9.3 && ramp.base > 9.2, "{}", ramp.base);
        }
        // A fall AC1 climbs (to a deck at 9.6 m) leaves the foot on the tread.
        let scene = Scene::new(vec![
            platform(0.8),
            quad(29.6, 29.6, 34.4, 34.4, 10.0, 1),
            quad(20.0, 20.0, 44.0, 44.0, 9.6, 2),
        ]);
        let found = band_ledges(&scene, &l, &|o| o == 0);
        let r = ramps(&found, &scene, &l);
        assert_eq!(r.made.len(), 4, "{r:?}");
        assert!(r
            .made
            .iter()
            .all(|ramp| ramp.base <= 10.0 && ramp.base > 9.9));
    }
}
