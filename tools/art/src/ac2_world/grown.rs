//! AC2's own cover, grown by AC1's scenery rules: AC2's land grows its trees, rocks, plants and
//! grass from its scenery kinds, each a set of scenes AC2's rules scatter about every 10 m vertex.
//! The converted world grows the same kinds from AC1's scene records instead, so AC1's generator
//! (the client's and Empyrean's alike) places them and no record of AC1's own is named.
//!
//! For each of AC2's cover kinds ([`Key`]: a scenery kind, and whether the vertex is under AC2's
//! water), AC2's own rules are run over AC2's world ([`census`]): how many vertices hold the kind,
//! and what each object AC2 places there stands on (the slope, how far into the ground). Then, at
//! up to [`TILES_PER_KEY`] places where the kind covers a whole AC1 land cell's width, off AC2's
//! roads and clear of its placed objects, the objects AC2 places in a [`TILE`] meter square
//! ([`tiles`]) become one AC1 scene, each object where AC2 places it about the square's middle, at
//! AC2's heading and scale. AC1 grows one scene at every land cell corner, 24 m apart, picking among
//! the kind's scenes by its own hash: each corner grows a square of AC2's own cover, at AC2's
//! density, and squares meet without gaps or overlap.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use crate::ac2_world::ground::Ac2Ground;
use crate::ac2_world::scenes::{pick_apart, spot_hash, Key};
use auberean_assets::landblock::Landblock;
use auberean_primitives::DataId as Ac2Id;
use auberean_world::records::{RegionSet, WorldRecords};
use auberean_world::scenery::{block_scenery, KeptClear, SceneryInput, SceneryObject};
use auberean_world::terrain::build_block;

/// The side of the square of AC2's cover one AC1 scene holds: AC1's land cell, meters.
pub const TILE: f64 = 24.0;
/// The most scenes each cover kind is given.
pub const TILES_PER_KEY: usize = 24;
/// How many candidate places are kept for each kind, the lowest by hash.
const CANDIDATES: usize = 512;
/// How far a sample's middle stands from every object AC2's landblock places, meters.
const CLEAR_OF_PLACED: f64 = 24.0;
/// AC2's vertices a block side.
const SIDE: usize = 17;
/// How far a sample's middle stands from its block's edge, in vertices: the square stays inside
/// the block, and so is grown by that block's vertices alone.
const EDGE: usize = 2;
/// The slope histogram's bins (the ground's normal height, hundredths).
const SLOPE_BINS: usize = 101;
/// The lift histogram: how far an object's origin stands above the surface under it, over its
/// height scale, in [`LIFT_STEP`] meter bins from `-LIFT_RANGE` to `LIFT_RANGE`.
const LIFT_STEP: f64 = 0.02;
const LIFT_RANGE: f64 = 4.0;
const LIFT_BINS: usize = 401;

/// How far a frill's edge may stand off the ground beside it before it is given bands of slope,
/// meters ([`bands`]).
pub const GROUND_FIT: f32 = 0.1;
/// The steepest fall, meters a meter, on which a frill stands upright at AC2's heading.
pub const UPRIGHT_FALL: f64 = 0.05;
/// How much more each steeper band falls than the one before, meters a meter.
pub const BAND_FALL: f64 = 0.1;
/// The most bands of slope a frill is given.
pub const MOST_BANDS: usize = 8;
/// The fall of the steepest ground a frill that grows on ground of any slope is given bands for.
const STEEPEST_FALL: f64 = 1.0;

/// One band of the slopes a frill grows on: the ground's normal heights it takes (as AC1's
/// scenes give them, least and most), whether it is turned to face down the slope, and how far it
/// leans there, meters of height a meter forward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub min_slope: f32,
    pub max_slope: f32,
    pub aligned: bool,
    pub fall: f32,
}

impl Band {
    /// Upright on every slope down to `least`, at AC2's heading.
    #[must_use]
    pub const fn upright(least: f32) -> Self {
        Self {
            min_slope: least,
            max_slope: 90.0,
            aligned: false,
            fall: 0.0,
        }
    }
}

/// The ground's normal height where it falls `fall` meters a meter.
fn normal_of(fall: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let n = (1.0 / (1.0 + fall * fall).sqrt()) as f32;
    n
}

/// AC1 stands every object upright, where AC2 leans a frill (a mat of grass, a fern) onto the
/// slope it grows on: a frill reaching `reach` meters from its middle stands that far times the
/// slope off the ground at its downhill edge. So a frill that would stand more than
/// [`GROUND_FIT`] off the ground on the steepest slope it grows on (down to normal height `least`)
/// is grown as several objects, one for each band of those slopes, only one of which grows at any
/// place. On ground falling at most [`UPRIGHT_FALL`] it stands upright at AC2's heading; each
/// steeper band, [`BAND_FALL`] wide, faces down the slope and leans along it by the fall in its
/// middle, the last of [`MOST_BANDS`] taking every steeper slope. The bands are the same for every
/// frill, so each frill is made leaning only a few ways.
#[must_use]
pub fn bands(reach: f32, least: f32) -> Vec<Band> {
    let steepest = if least > 0.0 {
        let l = f64::from(least);
        (1.0 / (l * l) - 1.0).sqrt()
    } else {
        STEEPEST_FALL
    };
    if f64::from(reach) * steepest <= f64::from(GROUND_FIT) || steepest <= UPRIGHT_FALL {
        return vec![Band::upright(least)];
    }
    let mut v = vec![Band {
        min_slope: normal_of(UPRIGHT_FALL),
        ..Band::upright(least)
    }];
    for k in 0..MOST_BANDS - 1 {
        #[allow(clippy::cast_precision_loss)]
        let lo = UPRIGHT_FALL + BAND_FALL * k as f64;
        let hi = lo + BAND_FALL;
        let last = hi >= steepest || k + 2 == MOST_BANDS;
        // A slope on the line between two bands is the gentler one's.
        let max_slope = f32::from_bits(normal_of(lo).to_bits() - 1);
        #[allow(clippy::cast_possible_truncation)]
        v.push(Band {
            min_slope: if last { least } else { normal_of(hi) },
            max_slope,
            aligned: true,
            fall: ((lo + hi) / 2.0) as f32,
        });
        if last {
            break;
        }
    }
    v
}

/// A fall as a whole number of thousandths, to key what is made for it by.
fn band_key(fall: f32) -> i32 {
    // LINT-OK: a fall of a few meters a meter at most, in thousandths.
    #[allow(clippy::cast_possible_truncation)]
    let k = (f64::from(fall) * 1000.0).round() as i32;
    k
}

/// What AC2 places of one object, world-wide.
#[derive(Clone, Debug)]
pub struct ObjectStats {
    pub placed: u64,
    pub frill: bool,
    pub slope: Vec<u32>,
    pub lift: Vec<u32>,
}

impl Default for ObjectStats {
    fn default() -> Self {
        Self {
            placed: 0,
            frill: false,
            slope: vec![0; SLOPE_BINS],
            lift: vec![0; LIFT_BINS],
        }
    }
}

impl ObjectStats {
    fn add(&mut self, o: &Self) {
        self.placed += o.placed;
        self.frill |= o.frill;
        for (a, b) in self.slope.iter_mut().zip(&o.slope) {
            *a += b;
        }
        for (a, b) in self.lift.iter_mut().zip(&o.lift) {
            *a += b;
        }
    }

    /// The least ground normal height it is found on, leaving out the steepest hundredth of its
    /// places, less a margin for the converted ground's own triangles; 0 for anything it is found
    /// on nearly everywhere.
    #[must_use]
    pub fn min_slope(&self) -> f32 {
        let total: u64 = self.slope.iter().map(|&n| u64::from(n)).sum();
        let mut acc = 0u64;
        for (b, &n) in self.slope.iter().enumerate() {
            acc += u64::from(n);
            if acc * 100 >= total {
                #[allow(clippy::cast_precision_loss)]
                let v = b as f32 / 100.0 - 0.03;
                return if v <= 0.05 { 0.0 } else { v.min(1.0) };
            }
        }
        0.0
    }

    /// The median of how far its origin stands above the surface under it, over its height scale:
    /// meters in its own frame.
    #[must_use]
    pub fn lift(&self) -> f32 {
        let total: u64 = self.lift.iter().map(|&n| u64::from(n)).sum();
        let mut acc = 0u64;
        for (b, &n) in self.lift.iter().enumerate() {
            acc += u64::from(n);
            if acc * 2 >= total && total > 0 {
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                return (b as f64 * LIFT_STEP - LIFT_RANGE) as f32;
            }
        }
        0.0
    }
}

/// AC2's cover world-wide.
#[derive(Clone, Debug, Default)]
pub struct Census {
    /// Vertices growing each kind (every block's 17 x 17, as AC2 grows them).
    pub vertices: BTreeMap<Key, u64>,
    /// Per kind and object: objects and frills placed.
    pub placed: BTreeMap<(Key, u32), [u64; 2]>,
    pub objects: BTreeMap<u32, ObjectStats>,
    /// Per kind: candidate sample middles, world meters, with their hash.
    pub candidates: BTreeMap<Key, Vec<(u32, f64, f64)>>,
    /// Objects and frills placed in all, and those placed at a scale not the same along each axis.
    pub total: [u64; 2],
    pub uneven: u64,
    pub blocks: u64,
}

impl Census {
    fn add(&mut self, o: Self) {
        for (k, n) in o.vertices {
            *self.vertices.entry(k).or_default() += n;
        }
        for (k, n) in o.placed {
            let e = self.placed.entry(k).or_default();
            e[0] += n[0];
            e[1] += n[1];
        }
        for (k, s) in o.objects {
            self.objects.entry(k).or_default().add(&s);
        }
        for (k, mut c) in o.candidates {
            let e = self.candidates.entry(k).or_default();
            e.append(&mut c);
            keep_lowest(e);
        }
        self.total[0] += o.total[0];
        self.total[1] += o.total[1];
        self.uneven += o.uneven;
        self.blocks += o.blocks;
    }
}

fn keep_lowest(v: &mut Vec<(u32, f64, f64)>) {
    if v.len() > CANDIDATES {
        v.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.total_cmp(&b.2))
        });
        v.truncate(CANDIDATES);
    }
}

/// The scenery kinds that grow something: a scene type with a scene in it.
pub fn growing(region: &RegionSet) -> std::collections::BTreeSet<u32> {
    region
        .scenes
        .iter()
        .flat_map(|s| &s.scene_types)
        .filter(|t| t.scenes.iter().any(|s| !s.scene.is_none()))
        .map(|t| t.index)
        .collect()
}

/// Which scenery kind each terrain kind grows under each scene field: its kind's scenery kind for
/// the field (or the kind's default), when that grows anything.
pub struct KindTable(Vec<Option<u32>>);

impl KindTable {
    #[must_use]
    pub fn new(region: &RegionSet) -> Self {
        let grows = growing(region);
        let terrain = &region.terrain;
        let mut v = vec![None; 256 * 256];
        for kind in 0..256u32 {
            let Some(t) = terrain
                .terrains
                .iter()
                .find(|t| t.index == kind)
                .or(terrain.default_terrain.as_ref())
            else {
                continue;
            };
            for field in 0..256u32 {
                let scenery = t
                    .scenery
                    .iter()
                    .find(|s| s.index == field)
                    .or(t.default_scenery.as_ref());
                v[(kind * 256 + field) as usize] =
                    scenery.map(|s| s.kind).filter(|k| grows.contains(k));
            }
        }
        Self(v)
    }

    /// The cover kind AC2 vertex `v` grows, and whether the vertex is under AC2's water.
    #[must_use]
    pub fn key(&self, g: &Ac2Ground, v: usize) -> Option<Key> {
        if g.height[v].is_nan() {
            return None;
        }
        let kind = self.0[usize::from(g.kind[v]) * 256 + usize::from(g.scene[v])]?;
        let side = g.side();
        Some((kind, !g.is_land(v / side, v % side)))
    }
}

/// What AC2 grows on block `(bx, by)`, frills among them, kept clear of what its landblock places
/// (the client's own scenery), with the places it places them.
fn grow(
    wr: &WorldRecords,
    region: &RegionSet,
    bx: i32,
    by: i32,
) -> (Vec<SceneryObject>, Vec<(f64, f64)>) {
    let mut land: HashMap<(i32, i32), Landblock> = HashMap::new();
    for dx in -1..=1 {
        for dy in -1..=1 {
            if let Some(l) = wr.landblock(bx + dx, by + dy) {
                land.insert((bx + dx, by + dy), l);
            }
        }
    }
    let (Some(lb), Some(scenes)) = (land.get(&(bx, by)).cloned(), region.scenes.as_ref()) else {
        return (Vec::new(), Vec::new());
    };
    let Some(tb) = build_block(&region.land, region.surfaces(), &land, bx, by) else {
        return (Vec::new(), Vec::new());
    };
    let lookup = |id: u32| auberean_world::place::PlaceRecords::entity(wr, Ac2Id(id));
    let info = wr.landblock_info(bx, by);
    let obstacles = info.as_ref().map_or_else(Vec::new, |i| {
        KeptClear::of_placed(&i.entities.entities, &lookup)
    });
    let placed: Vec<(f64, f64)> = info
        .iter()
        .flat_map(|i| &i.entities.entities)
        .filter_map(|e| e.offset.as_ref())
        .map(|o| (f64::from(o.frame.origin.x), f64::from(o.frame.origin.y)))
        .collect();
    let water = region
        .water
        .as_ref()
        .and_then(|w| {
            let low = tb
                .geometry
                .vertices
                .iter()
                .map(|v| v.z)
                .fold(f32::INFINITY, f32::min);
            w.block(&region.land, tb.x, tb.y, low)
        })
        .map(|w| w.height);
    let input = SceneryInput {
        land: &region.land,
        terrain: &region.terrain,
        scenes,
        vertices: &lb.vertices,
        block: &tb,
        obstacles: &obstacles,
        degrade: 1.0,
        water,
    };
    (block_scenery(&input, wr), placed)
}

/// The surface under world point `(x, y)` as the converted world has it: AC2's ground, or its
/// water where the ground is under it. AC2 meters.
fn surface(g: &Ac2Ground, x: f64, y: f64) -> Option<f64> {
    let s = g.sample(x, y)?;
    Some(s.plane.map_or(s.ground, |p| s.ground.max(p)))
}

/// The ground's normal height at world point `(x, y)`, across a meter of AC2's triangles.
fn normal_z(g: &Ac2Ground, x: f64, y: f64) -> Option<f64> {
    let dx = (g.ground(x + 0.5, y)? - g.ground(x - 0.5, y)?) / 1.0;
    let dy = (g.ground(x, y + 0.5)? - g.ground(x, y - 0.5)?) / 1.0;
    Some(1.0 / (1.0 + dx * dx + dy * dy).sqrt())
}

fn uneven(s: auberean_primitives::Vec3) -> bool {
    (s.x - s.z).abs() > 1e-3 * s.z.abs() || (s.y - s.z).abs() > 1e-3 * s.z.abs()
}

/// AC2's cover over every block AC2 holds, `skip` aside (the placeholder land left out).
#[must_use]
pub fn census(
    wr: &WorldRecords,
    region: &RegionSet,
    g: &Ac2Ground,
    skip: &(dyn Fn(usize, usize) -> bool + Sync),
) -> Census {
    let table = KindTable::new(region);
    let side = g.side();
    let step = f64::from(region.land.grid().cell_length)
        / region.land.grid().vertices_per_cell as f64
        - 0.0002;
    let rows = crate::ac2_world::dense::par_map(g.blocks, |bx| {
        let mut c = Census::default();
        for by in 0..g.blocks {
            if !g.have[bx * g.blocks + by] || skip(bx, by) {
                continue;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let (objects, placed) = grow(wr, region, bx as i32, by as i32);
            c.blocks += 1;
            let (x0, y0) = (bx as f64 * g.block_len(), by as f64 * g.block_len());
            let at = |i: usize, j: usize| (bx * g.squares + i) * side + by * g.squares + j;
            let keys: Vec<Option<Key>> = (0..SIDE * SIDE)
                .map(|k| table.key(g, at(k / SIDE, k % SIDE)))
                .collect();
            for k in keys.iter().flatten() {
                *c.vertices.entry(*k).or_default() += 1;
            }
            for o in &objects {
                let (lx, ly) = (f64::from(o.origin.x), f64::from(o.origin.y));
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (i, j) = (
                    ((lx / step).round().max(0.0) as usize).min(SIDE - 1),
                    ((ly / step).round().max(0.0) as usize).min(SIDE - 1),
                );
                let Some(key) = keys[i * SIDE + j] else {
                    continue;
                };
                let (wx, wy) = (x0 + lx, y0 + ly);
                let e = c.placed.entry((key, o.object.0)).or_default();
                e[usize::from(o.frill)] += 1;
                c.total[usize::from(o.frill)] += 1;
                c.uneven += u64::from(uneven(o.scale));
                let s = c.objects.entry(o.object.0).or_default();
                s.placed += 1;
                s.frill |= o.frill;
                if let Some(n) = normal_z(g, wx, wy) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let b = ((n * 100.0).round() as usize).min(SLOPE_BINS - 1);
                    s.slope[b] += 1;
                }
                if let (Some(h), true) = (surface(g, wx, wy), o.scale.z > 0.0) {
                    let r = (f64::from(o.origin.z) - h) / f64::from(o.scale.z);
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let b = (((r + LIFT_RANGE) / LIFT_STEP).round().max(0.0) as usize)
                        .min(LIFT_BINS - 1);
                    s.lift[b] += 1;
                }
            }
            // Sample middles: a vertex whose neighbors grow its kind, off the road, clear of what
            // the landblock places.
            for i in EDGE..SIDE - EDGE {
                for j in EDGE..SIDE - EDGE {
                    let Some(key) = keys[i * SIDE + j] else {
                        continue;
                    };
                    let pure = (i - 1..=i + 1).all(|a| {
                        (j - 1..=j + 1)
                            .all(|b| keys[a * SIDE + b] == Some(key) && g.road[at(a, b)] == 0)
                    });
                    #[allow(clippy::cast_precision_loss)]
                    let (cx, cy) = (i as f64 * step, j as f64 * step);
                    let clear = placed.iter().all(|&(px, py)| {
                        dereth_primitives::num::math::hypot(px - cx, py - cy) >= CLEAR_OF_PLACED
                    });
                    if pure && clear {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                        let h = spot_hash((bx * g.squares + i) as i32, (by * g.squares + j) as i32);
                        c.candidates
                            .entry(key)
                            .or_default()
                            .push((h, x0 + cx, y0 + cy));
                    }
                }
            }
            for v in c.candidates.values_mut() {
                keep_lowest(v);
            }
        }
        c
    });
    let mut all = Census::default();
    for c in rows {
        all.add(c);
    }
    all
}

/// Each kind's samples: the objects of each.
pub type Samples = BTreeMap<Key, Vec<Vec<TileObject>>>;
/// Each kind's sample middles, world meters.
pub type Middles = BTreeMap<Key, Vec<(f64, f64)>>;
/// A sample to take: its kind, its number, and its middle.
type Wanted = (Key, usize, f64, f64);

/// One object of a sample square: what it is, where it stands about the square's middle (meters),
/// how far above the surface under it, and its turn and scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileObject {
    pub object: u32,
    pub dx: f32,
    pub dy: f32,
    pub rotation: auberean_primitives::Quat,
    pub scale: auberean_primitives::Vec3,
    pub frill: bool,
}

/// Up to [`TILES_PER_KEY`] sample squares of each kind in `keys`, each the objects AC2 places in
/// the [`TILE`] meter square about the sample's middle; the middles kept two squares apart. With
/// each kind's middles, world meters.
#[must_use]
pub fn tiles(
    wr: &WorldRecords,
    region: &RegionSet,
    g: &Ac2Ground,
    census: &Census,
    keys: &[Key],
) -> (Samples, Middles) {
    let len = g.block_len();
    let mut middles = Middles::new();
    let mut by_block: BTreeMap<(usize, usize), Vec<Wanted>> = BTreeMap::new();
    let mut out = Samples::new();
    for key in keys {
        let Some(c) = census.candidates.get(key) else {
            continue;
        };
        let picks = pick_apart(c, TILES_PER_KEY, 2.0 * TILE);
        out.insert(*key, vec![Vec::new(); picks.len()]);
        middles.insert(*key, picks.clone());
        for (n, (x, y)) in picks.into_iter().enumerate() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let b = ((x / len).floor() as usize, (y / len).floor() as usize);
            by_block.entry(b).or_default().push((*key, n, x, y));
        }
    }
    let blocks: Vec<((usize, usize), Vec<Wanted>)> = by_block.into_iter().collect();
    let grown = crate::ac2_world::dense::par_map(blocks.len(), |k| {
        let ((bx, by), list) = &blocks[k];
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let (objects, _) = grow(wr, region, *bx as i32, *by as i32);
        let (x0, y0) = (*bx as f64 * len, *by as f64 * len);
        list.iter()
            .map(|&(key, n, x, y)| {
                let half = TILE / 2.0;
                let mut v: Vec<TileObject> = objects
                    .iter()
                    .filter_map(|o| {
                        let (dx, dy) = (
                            x0 + f64::from(o.origin.x) - x,
                            y0 + f64::from(o.origin.y) - y,
                        );
                        #[allow(clippy::cast_possible_truncation)]
                        ((-half..half).contains(&dx) && (-half..half).contains(&dy)).then_some(
                            TileObject {
                                object: o.object.0,
                                dx: dx as f32,
                                dy: dy as f32,
                                rotation: o.rotation,
                                scale: o.scale,
                                frill: o.frill,
                            },
                        )
                    })
                    .collect();
                // Frills last, so a scene's objects come before its decoration as AC2 places them.
                v.sort_by_key(|t| t.frill);
                (key, n, v)
            })
            .collect::<Vec<_>>()
    });
    for (key, n, v) in grown.into_iter().flatten() {
        if let Some(slot) = out.get_mut(&key).and_then(|t| t.get_mut(n)) {
            *slot = v;
        }
    }
    (out, middles)
}

/// The heading alone of an AC2 turn: the turn about the upright that points the object's own x
/// axis where the turn points it, across the ground. AC1 drops the object onto its own ground;
/// the tilt AC2 gives an object leaning onto its sample's slope is not kept.
#[must_use]
pub fn heading_of(q: auberean_primitives::Quat) -> dereth_primitives::Quat {
    let m = auberean_world::math::Mat4::from_parts(
        auberean_primitives::Vec3::new(0.0, 0.0, 0.0),
        q,
        auberean_primitives::Vec3::new(1.0, 1.0, 1.0),
    );
    let x = m.point(auberean_primitives::Vec3::new(1.0, 0.0, 0.0));
    let a = dereth_primitives::num::math::atan2f(x.y, x.x);
    if !a.is_finite() || (x.x.abs() < 1e-6 && x.y.abs() < 1e-6) {
        return dereth_primitives::Quat::IDENTITY;
    }
    dereth_primitives::Quat::new(
        dereth_primitives::num::math::cosf(a / 2.0),
        0.0,
        0.0,
        dereth_primitives::num::math::sinf(a / 2.0),
    )
}

/// The AC1 scenes made of the samples: each kind's scene records, and what was made.
pub struct Made {
    pub scenes_of: BTreeMap<Key, Vec<dereth_primitives::DataId>>,
    pub scenes: usize,
    /// Objects and frills the scenes hold.
    pub objects: [usize; 2],
    pub models: usize,
    pub setups: usize,
    /// Sample objects left out, by why.
    pub left_out: BTreeMap<&'static str, usize>,
    /// The objects whose idle bends them in AC2, made as built.
    pub bent: Vec<u32>,
    /// The frills' blended surfaces made cut-outs.
    pub cut_out: usize,
    /// A line per AC1 setup made: its AC2 object, its axes' scales over its height's, the lift and
    /// least slope it takes, and how many sample objects it draws.
    pub tsv: String,
}

/// A scale's two ground axes over its height, in thousandths, to a twentieth; the same along each
/// axis within a thousandth reads as even.
fn aspect(s: auberean_primitives::Vec3) -> [i32; 3] {
    let q = |v: f32| {
        let r = f64::from(v) / f64::from(s.z);
        #[allow(clippy::cast_possible_truncation)]
        if (r - 1.0).abs() < 1e-3 {
            1000
        } else {
            // LINT-OK: a lift in centimeters, a few meters at most.
            ((r * 20.0).round() * 50.0) as i32
        }
    };
    [q(s.x), q(s.y), 1000]
}

/// Every sample made an AC1 scene: each object an AC1 setup of its AC2 model (one per object and
/// shape, raised by the object's lift, at its least slope), placed where the sample places it about
/// the corner, at its heading and its height scale. Frills are left out unless `frills`.
#[allow(clippy::too_many_arguments)]
pub fn make_scenes(
    tiles: &Samples,
    census: &Census,
    wr: &WorldRecords,
    ac2: &crate::ac2_look::source::Ac2,
    pictures: &mut crate::ac2_world::art::Pictures,
    ids: &mut crate::ac2_world::art::Ids,
    out: &mut crate::ac2_world::art::Records,
    levels: usize,
    frills: bool,
) -> Result<Made, String> {
    use dereth_primitives::{Frame, Vec3};
    let mut made = Made {
        scenes_of: BTreeMap::new(),
        scenes: 0,
        objects: [0, 0],
        models: 0,
        setups: 0,
        left_out: BTreeMap::new(),
        bent: Vec::new(),
        cut_out: 0,
        tsv: String::new(),
    };
    // By object and whether it is a frill (made as built, as AC2 draws its frills).
    let mut models: BTreeMap<(u32, bool), Result<crate::ac2_world::statics::Model, &'static str>> =
        BTreeMap::new();
    // By object, frill and shape: the setup, its lift and least slope, and the objects it draws.
    type Made1 = (dereth_primitives::DataId, f32, f32, usize);
    let mut setups: BTreeMap<(u32, bool, [i32; 3], i32), Made1> = BTreeMap::new();
    // By object, frill and how far it leans: the model leaning so, or `None` where it cannot.
    type Leaning = Option<crate::ac2_world::statics::Model>;
    let mut sheared: BTreeMap<(u32, bool, i32), Leaning> = BTreeMap::new();
    let mut left_of: BTreeMap<(u32, bool, &'static str), usize> = BTreeMap::new();
    for (key, list) in tiles {
        let mut ids_of = Vec::with_capacity(list.len());
        for tile in list {
            let mut objects = Vec::with_capacity(tile.len());
            for t in tile {
                if t.frill && !frills {
                    *made.left_out.entry("a frill, frills off").or_default() += 1;
                    continue;
                }
                let mk = (t.object, t.frill);
                if let std::collections::btree_map::Entry::Vacant(slot) = models.entry(mk) {
                    let mut m = crate::ac2_world::statics::make_model(
                        wr, ac2, t.object, t.frill, pictures, ids, out, levels,
                    )?;
                    // AC1 bends no model: one whose idle bends it stands as built.
                    if matches!(m, Err(why) if why == crate::ac2_world::statics::MOVES_BENDING) {
                        m = crate::ac2_world::statics::make_model(
                            wr, ac2, t.object, true, pictures, ids, out, levels,
                        )?;
                        made.bent.push(t.object);
                    }
                    slot.insert(m);
                }
                let model = match &models[&mk] {
                    Ok(m) => m,
                    Err(why) => {
                        *made.left_out.entry(why).or_default() += 1;
                        *left_of.entry((t.object, t.frill, why)).or_default() += 1;
                        continue;
                    }
                };
                let s = t.scale;
                if !(s.z > 0.0 && s.z.is_finite() && s.x > 0.0 && s.y > 0.0) {
                    *made.left_out.entry("no scale of its own").or_default() += 1;
                    continue;
                }
                let shape = aspect(s);
                if model.moves() && shape != [1000, 1000, 1000] {
                    *made
                        .left_out
                        .entry(crate::ac2_world::statics::MOVES_UNEVEN)
                        .or_default() += 1;
                    *left_of
                        .entry((t.object, t.frill, crate::ac2_world::statics::MOVES_UNEVEN))
                        .or_default() += 1;
                    continue;
                }
                let stats = census.objects.get(&t.object).cloned().unwrap_or_default();
                let (lift, least) = (stats.lift(), stats.min_slope());
                // A frill lies along the ground where AC2 leans it onto its slope: a band of
                // slopes each, the steeper ones leaning down the slope ([`bands`]).
                #[allow(clippy::cast_precision_loss)]
                let wide = |k: i32| k as f32 / 1000.0;
                let reach = model.reach() * s.z * wide(shape[0]).max(wide(shape[1]));
                // A model with coarser levels, or one that cannot lean, stands upright.
                let kept = if t.frill && model.leans() && levels <= 1 {
                    bands(reach, least)
                } else {
                    vec![Band::upright(least)]
                };
                for band in &kept {
                    let fall_milli = band_key(band.fall);
                    let sk = (t.object, t.frill, shape, fall_milli);
                    let entry = match setups.get_mut(&sk) {
                        Some(e) => e,
                        None => {
                            let leaning = if band.fall > 0.0 {
                                let mk = (t.object, t.frill, band_key(band.fall * wide(shape[1])));
                                if let std::collections::btree_map::Entry::Vacant(slot) =
                                    sheared.entry(mk)
                                {
                                    slot.insert(model.sheared(
                                        band.fall * wide(shape[1]),
                                        ids,
                                        out,
                                    )?);
                                }
                                sheared[&mk].as_ref()
                            } else {
                                None
                            };
                            let id = crate::ac2_world::statics::setup_for(
                                leaning.unwrap_or(model),
                                shape,
                                lift,
                                ids,
                                out,
                            )?;
                            setups.entry(sk).or_insert((id, lift, least, 0))
                        }
                    };
                    entry.3 += 1;
                    objects.push(dereth_assets::world::ObjectDesc {
                        obj_id: entry.0,
                        base_loc: Frame::new(
                            Vec3::new(t.dx, t.dy, 0.0),
                            if band.aligned {
                                dereth_primitives::Quat::IDENTITY
                            } else {
                                heading_of(t.rotation)
                            },
                        ),
                        freq: 1.0,
                        displace_x: 0.0,
                        displace_y: 0.0,
                        min_scale: s.z,
                        max_scale: s.z,
                        max_rot: 0.0,
                        min_slope: band.min_slope,
                        max_slope: band.max_slope,
                        align: i32::from(band.aligned),
                        orient: 0,
                        weenie_obj: 0,
                    });
                }
                made.objects[usize::from(t.frill)] += 1;
            }
            let id = ids.next(0x12);
            out.add(id, crate::ac2_world::scenes::encode_scene(id, &objects));
            ids_of.push(id);
            made.scenes += 1;
        }
        made.scenes_of.insert(*key, ids_of);
    }
    // A frill's pictures AC2 blends are cut out at AC1's alpha reference, as AC1's own grass is.
    for ((_, frill), m) in &models {
        let Ok(m) = m else { continue };
        if *frill {
            made.cut_out += cut_out_blends(m.parts(), out)?;
        }
    }
    made.models = models.values().filter(|m| m.is_ok()).count();
    made.setups = setups.len();
    let _ = writeln!(
        made.tsv,
        "ac2_object\tfrill\tshape_milli\tfall_milli\tac1_setup\tlift_m\tleast_slope\tcollides\tsample_objects\tac2_placed"
    );
    for ((object, frill, shape, fall), (id, lift, slope, n)) in &setups {
        let collides = models.get(&(*object, *frill)).is_some_and(|m| {
            m.as_ref()
                .is_ok_and(crate::ac2_world::statics::Model::collides)
        });
        let placed = census.objects.get(object).map_or(0, |o| o.placed);
        let _ = writeln!(
            made.tsv,
            "{object:08X}\t{frill}\t{},{},{}\t{fall}\t{:08X}\t{lift:.2}\t{slope:.2}\t{collides}\t{n}\t{placed}",
            shape[0],
            shape[1],
            shape[2],
            id.raw()
        );
    }
    for ((object, frill, why), n) in &left_of {
        let _ = writeln!(
            made.tsv,
            "{object:08X}\t{frill}\tleft out: {why}\t\t\t\t\t\t{n}\t"
        );
    }
    Ok(made)
}

/// The blended picture surfaces of `parts` (records in `out`) made cut-outs; how many.
fn cut_out_blends(
    parts: &[dereth_primitives::DataId],
    out: &mut crate::ac2_world::art::Records,
) -> Result<usize, String> {
    use dereth_assets::Decode as _;
    const BLENDED: u32 = 0x102;
    const CUT_OUT: u32 = 4;
    let mut n = 0;
    for part in parts {
        let Some(bytes) = out.0.get(part) else {
            continue;
        };
        let g = dereth_assets::GfxObj::decode_payload(*part, bytes)
            .map_err(|e| format!("{part:?}: {e:?}"))?;
        for sid in g.surfaces {
            let Some(sb) = out.0.get(&sid) else { continue };
            let mut surface = dereth_assets::Surface::decode_payload(sid, sb)
                .map_err(|e| format!("{sid:?}: {e:?}"))?;
            if surface.surface_type == BLENDED {
                surface.surface_type = CUT_OUT;
                out.add(sid, surface.encode());
                n += 1;
            }
        }
    }
    Ok(n)
}

/// A line per cover kind: its vertices, objects and frills AC2 grows on it, and its samples.
#[must_use]
pub fn kinds_tsv(census: &Census, tiles: &Samples) -> String {
    let mut by_key: BTreeMap<Key, [u64; 2]> = BTreeMap::new();
    for ((k, _), n) in &census.placed {
        let e = by_key.entry(*k).or_default();
        e[0] += n[0];
        e[1] += n[1];
    }
    let mut s = String::from(
        "ac2_scenery_kind\tunder_water\tvertices\tobjects\tfrills\tobjects_per_ac1_cell\tsamples\tsample_objects\tsample_frills\n",
    );
    for (k, v) in &census.vertices {
        let n = by_key.get(k).copied().unwrap_or_default();
        let t = tiles.get(k);
        let (so, sf) = t.map_or((0, 0), |t| {
            (
                t.iter().flatten().filter(|o| !o.frill).count(),
                t.iter().flatten().filter(|o| o.frill).count(),
            )
        });
        #[allow(clippy::cast_precision_loss)]
        let per_cell = (n[0] + n[1]) as f64 / *v as f64 * (TILE * TILE / 100.0);
        let _ = writeln!(
            s,
            "{:08X}\t{}\t{v}\t{}\t{}\t{per_cell:.2}\t{}\t{so}\t{sf}",
            k.0,
            k.1,
            n[0],
            n[1],
            t.map_or(0, Vec::len)
        );
    }
    s
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's grown cover)
    use super::*;

    /// The bands of `bands` that take ground of normal height `nz`, by AC1's scenes' own test.
    fn taking(bands: &[Band], nz: f32) -> Vec<Band> {
        bands
            .iter()
            .filter(|b| {
                let mut o = dereth_terrain::scenery::default_object_desc(
                    dereth_primitives::DataId(0x0200_0001),
                );
                o.min_slope = b.min_slope;
                o.max_slope = b.max_slope;
                dereth_terrain::scenery::check_slope(&o, nz)
            })
            .copied()
            .collect()
    }

    #[test]
    fn a_wide_frill_lies_within_a_band_of_the_ground_on_every_slope_it_grows_on() {
        // A grass mat reaching 2 m, grown by AC2 down to a normal height of 0.75 (41 degrees).
        let (reach, least) = (2.0f32, 0.75f32);
        let bands = bands(reach, least);
        assert!(bands.len() > 1 && bands.len() <= MOST_BANDS, "{bands:?}");
        let steepest = (1.0 / f64::from(least).powi(2) - 1.0).sqrt();
        let mut worst = 0.0f64;
        for step in 0..=1000 {
            let fall = steepest * f64::from(step) / 1000.0;
            let nz = normal_of(fall);
            let took = taking(&bands, nz);
            assert_eq!(
                took.len(),
                1,
                "one band grows at a fall of {fall}: {took:?}"
            );
            let b = took[0];
            assert_eq!(
                b.aligned,
                b.fall > 0.0,
                "a leaning band faces down the slope"
            );
            // Its downhill edge, `reach` ahead of its middle, against the ground there.
            let off = f64::from(reach) * (fall - f64::from(b.fall)).abs();
            if fall <= UPRIGHT_FALL + BAND_FALL * (MOST_BANDS - 1) as f64 {
                worst = worst.max(off);
            }
        }
        assert!(
            worst <= f64::from(GROUND_FIT) + 1e-6,
            "{worst} m off the ground"
        );
        // Ground steeper than it grows on takes none.
        assert!(taking(&bands, least - 0.01).is_empty());
        // Every slope's band is the same for every frill.
        assert_eq!(bands, super::bands(3.0, least));
    }

    #[test]
    fn a_small_frill_stands_upright_at_its_own_heading_on_every_slope() {
        // Reaching 10 cm, it stands at most 9 cm off the ground at 41 degrees.
        assert_eq!(bands(0.1, 0.75), vec![Band::upright(0.75)]);
        assert_eq!(bands(2.0, 1.0), vec![Band::upright(1.0)]);
    }

    #[test]
    fn an_objects_least_slope_leaves_out_its_steepest_hundredth() {
        let mut s = ObjectStats::default();
        s.slope[90] = 99;
        s.slope[40] = 1;
        assert!((s.min_slope() - 0.37).abs() < 1e-6);
        s.slope[40] = 5;
        // The steepest places are more than a hundredth now.
        assert!((s.min_slope() - 0.37).abs() < 1e-6);
        let mut flat = ObjectStats::default();
        flat.slope[3] = 10;
        assert_eq!(flat.min_slope(), 0.0);
    }

    #[test]
    fn an_objects_lift_is_the_median_of_its_places() {
        let mut s = ObjectStats::default();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bin = |m: f64| ((m + LIFT_RANGE) / LIFT_STEP).round() as usize;
        s.lift[bin(-0.5)] = 3;
        s.lift[bin(0.0)] = 1;
        s.lift[bin(1.0)] = 1;
        assert!((s.lift() + 0.5).abs() < 1e-4);
        assert_eq!(ObjectStats::default().lift(), 0.0);
    }

    #[test]
    fn a_frills_blended_surfaces_become_cut_outs_and_the_rest_stay() {
        use dereth_assets::Decode as _;
        use dereth_primitives::{DataId, EraId};
        let surface = |t: u32| dereth_assets::Surface {
            surface_type: t,
            orig_texture_id: Some(DataId(0x0500_C000)),
            orig_palette_id: Some(DataId(0)),
            color_value: None,
            translucency: 0.0,
            luminosity: 0.0,
            diffuse: 1.0,
        };
        let mut out = crate::ac2_world::art::Records::default();
        out.add(DataId(0x0800_C000), surface(0x102).encode());
        out.add(DataId(0x0800_C001), surface(2).encode());
        let g = dereth_assets::GfxObj {
            id: DataId(0x0100_C000),
            flags: 0,
            surfaces: vec![DataId(0x0800_C000), DataId(0x0800_C001)],
            vertex_array: dereth_assets::VertexArray {
                vertex_type: 1,
                vertices: Vec::new(),
            },
            physics_polygons: Vec::new(),
            physics_bsp: None,
            sort_center: dereth_primitives::Vec3::ZERO,
            polygons: Vec::new(),
            drawing_bsp: None,
            did_degrade: None,
        };
        out.add(g.id, g.encode_for(EraId::Eor).unwrap());
        assert_eq!(cut_out_blends(&[g.id], &mut out).unwrap(), 1);
        let t = |id: u32| {
            dereth_assets::Surface::decode_payload(DataId(id), &out.0[&DataId(id)])
                .unwrap()
                .surface_type
        };
        assert_eq!((t(0x0800_C000), t(0x0800_C001)), (4, 2));
    }

    #[test]
    fn a_heading_keeps_the_turn_about_the_upright_and_drops_the_tilt() {
        let half = std::f32::consts::FRAC_PI_4;
        // A quarter turn about the upright.
        let q = auberean_primitives::Quat {
            w: dereth_primitives::num::math::cosf(half),
            x: 0.0,
            y: 0.0,
            z: dereth_primitives::num::math::sinf(half),
        };
        let h = heading_of(q);
        assert!(
            (h.w - dereth_primitives::num::math::cosf(half)).abs() < 1e-5
                && (h.z - dereth_primitives::num::math::sinf(half)).abs() < 1e-5
        );
        assert!(h.x.abs() < 1e-6 && h.y.abs() < 1e-6);
        // A tilt about x alone has no heading.
        let t = auberean_primitives::Quat {
            w: 0.95f32.sqrt(),
            x: 0.05f32.sqrt(),
            y: 0.0,
            z: 0.0,
        };
        let h = heading_of(t);
        assert!((h.w - 1.0).abs() < 1e-5 && h.z.abs() < 1e-5);
    }
}
