//! The dungeon kit's pieces: every piece of every environment retail's cells are built from, what
//! each is like (its shape, the looks retail draws it in, the pieces retail joins it to), the
//! surfaces retail draws it with, and the landblocks that use it; and how a new piece is placed so
//! that one of its portals meets another cell's.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::geometry::CellStruct;
use dereth_assets::world::EnvCell;
use dereth_assets::{Environment, Surface};
use dereth_dat::{DbType, ModernDat};
use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::{DataId, Frame, Vec3};

use crate::compile::heading_quat;
use crate::dats::DatSet;

/// One piece: an environment's cell struct.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub environment: DataId,
    pub piece: u16,
    /// Its portals.
    pub portals: usize,
    /// Its extent, x, y and z, in meters.
    pub size: [f32; 3],
    /// The surfaces the first retail cell built of it is drawn with.
    pub surfaces: Vec<DataId>,
    /// How many retail cells are built of it, and in which landblocks (with how many there).
    pub cells: usize,
    pub landblocks: BTreeMap<u16, usize>,
    pub shape: Shape,
    /// The looks retail draws it in, most cells first.
    pub looks: Vec<Look>,
    /// The pieces retail joins it to through its portals, by their place in the catalog, with
    /// how many times, most first.
    pub joins: Vec<(usize, usize)>,
}

impl Piece {
    /// A short name: the environment's low half and the piece, `0094/0`.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{:04X}/{}", self.environment.raw() & 0xFFFF, self.piece)
    }
}

/// Original pieces that have no retail look or placement yet.
#[must_use]
pub fn original(env: &Environment, surfaces: &[DataId]) -> Vec<Piece> {
    env.cells
        .iter()
        .enumerate()
        .filter_map(|(i, cs)| {
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for v in &cs.vertex_array.vertices {
                for (k, q) in [v.position.x, v.position.y, v.position.z]
                    .into_iter()
                    .enumerate()
                {
                    lo[k] = lo[k].min(q);
                    hi[k] = hi[k].max(q);
                }
            }
            Some(Piece {
                environment: env.id,
                piece: u16::try_from(i).ok()?,
                portals: cs.portals.len(),
                size: std::array::from_fn(|k| (hi[k] - lo[k]).max(0.0)),
                surfaces: surfaces.to_vec(),
                cells: 0,
                landblocks: BTreeMap::new(),
                shape: shape_of(cs),
                looks: Vec::new(),
                joins: Vec::new(),
            })
        })
        .collect()
}

/// What a piece is for, from its portals and its shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Shape {
    /// Wide both ways.
    Room,
    /// Two ways out, narrow.
    Corridor,
    /// Three ways out or more, narrow.
    Junction,
    /// Its ways out at heights a body climbs between, or a sloped floor.
    Stairs,
    /// One way out (or none).
    DeadEnd,
}

impl Shape {
    pub const ALL: [Self; 5] = [
        Self::Room,
        Self::Corridor,
        Self::Junction,
        Self::Stairs,
        Self::DeadEnd,
    ];

    /// Its name's key.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Room => "room",
            Self::Corridor => "corridor",
            Self::Junction => "junction",
            Self::Stairs => "stairs",
            Self::DeadEnd => "dead_end",
        }
    }
}

/// How much higher one of a piece's portals stands than another for the piece to be stairs, in
/// meters.
pub const STAIRS_RISE: f32 = 1.5;

/// How wide a piece is both ways to be a room, in meters.
pub const ROOM_WIDTH: f32 = 9.0;

/// What a cell struct's shape is judged by: how far apart its portals' bottoms are, its level
/// and its sloped floor and ceiling area (square meters), and its narrower width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measures {
    pub portals: usize,
    pub rise: f32,
    pub level: f32,
    pub sloped: f32,
    pub narrow: f32,
}

/// A cell struct's shape: a room when it is [`ROOM_WIDTH`] wide both ways; stairs when the
/// bottoms of its ways out stand [`STAIRS_RISE`] apart (a ramp, a stair, a shaft); else by its
/// ways out.
#[must_use]
pub fn shape_of(cs: &CellStruct) -> Shape {
    let m = measures(cs);
    if m.narrow >= ROOM_WIDTH {
        Shape::Room
    } else if m.rise >= STAIRS_RISE {
        Shape::Stairs
    } else {
        match m.portals {
            0 | 1 => Shape::DeadEnd,
            2 => Shape::Corridor,
            _ => Shape::Junction,
        }
    }
}

/// What [`shape_of`] judges `cs` by.
#[must_use]
pub fn measures(cs: &CellStruct) -> Measures {
    let vs = &cs.vertex_array.vertices;
    let corners = |p: &dereth_assets::Polygon| -> Vec<Vec3> {
        p.vertex_ids
            .iter()
            .filter_map(|i| vs.get(usize::from(*i)).map(|v| v.position))
            .collect()
    };
    let heights: Vec<f32> = cs
        .portals
        .iter()
        .filter_map(|p| cs.polygons.get(usize::from(*p)))
        .map(|p| corners(p).iter().map(|v| v.z).fold(f32::MAX, f32::min))
        .collect();
    let rise = heights.iter().copied().fold(f32::MIN, f32::max)
        - heights.iter().copied().fold(f32::MAX, f32::min);
    let portals: std::collections::BTreeSet<u16> = cs.portals.iter().copied().collect();
    // Level and sloped floor (and ceiling) areas: a polygon's normal by Newell's method, its
    // length twice the area.
    let (mut level, mut sloped) = (0.0f32, 0.0f32);
    for (i, p) in cs.polygons.iter().enumerate() {
        if u16::try_from(i).is_ok_and(|i| portals.contains(&i)) {
            continue;
        }
        let c = corners(p);
        let mut n = Vec3::ZERO;
        for k in 0..c.len() {
            let (a, b) = (c[k], c[(k + 1) % c.len()]);
            n.x += (a.y - b.y) * (a.z + b.z);
            n.y += (a.z - b.z) * (a.x + b.x);
            n.z += (a.x - b.x) * (a.y + b.y);
        }
        let len = n.dot(n).sqrt();
        if len < 1e-6 {
            continue;
        }
        let up = (n.z / len).abs();
        if up > 0.98 {
            level += len / 2.0;
        } else if up > 0.3 {
            sloped += len / 2.0;
        }
    }
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for v in vs {
        lo = [lo[0].min(v.position.x), lo[1].min(v.position.y)];
        hi = [hi[0].max(v.position.x), hi[1].max(v.position.y)];
    }
    Measures {
        portals: portals.len(),
        rise: if heights.is_empty() { 0.0 } else { rise },
        level,
        sloped,
        narrow: (hi[0] - lo[0]).min(hi[1] - lo[1]),
    }
}

/// A look retail draws cells in: the textures a set of its landblocks share. Each landblock's
/// look is the textures at least [`LOOK_SHARE`] of its cells are drawn with; two landblocks whose
/// looks share [`LOOK_ALIKE`] of their textures are one family, a texture most landblocks use
/// (a portal's fill, plain black) taken out first.
#[derive(Clone, Debug, PartialEq)]
pub struct Family {
    /// Its most used textures (or plain surfaces), most first.
    pub textures: Vec<DataId>,
    /// Its landblocks, with how many of their cells, most first.
    pub landblocks: Vec<(u16, usize)>,
    /// How many retail cells are drawn in it, and how many pieces.
    pub cells: usize,
    pub pieces: usize,
}

/// The share of a landblock's cells a texture is drawn on to be part of its look.
pub const LOOK_SHARE: f32 = 0.2;

/// The share of two looks' textures they have in common to be one family.
pub const LOOK_ALIKE: f32 = 0.6;

/// The share of landblocks whose look holds a texture for it to say nothing of a look.
pub const LOOK_COMMON: f32 = 0.03;

/// The share of a landblock's look a family's looks hold for a landblock alike to no other to join
/// it.
pub const LOOK_HELD: f32 = 0.5;

/// A look a piece is drawn in: the family, how many retail cells, and the surfaces most of them
/// are drawn with.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub family: usize,
    pub cells: usize,
    pub surfaces: Vec<DataId>,
}

/// Every piece retail's cells are built of, and the families of looks they are drawn in; and the
/// dungeon kit: every environment the world's dungeons place whole ([`crate::kit`]), with what each
/// family of looks draws each role with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    pub pieces: Vec<Piece>,
    pub families: Vec<Family>,
    pub kit: Vec<crate::kit::KitPiece>,
    pub dress: Vec<crate::kit::Dress>,
}

/// What one retail cell says: its piece, its landblock, its surfaces, the cells its portals
/// join, its number, where it stands (to the tenth of a meter and the thousandth of its turn), and
/// whether it is seen from outside.
struct SeenCell {
    key: (u32, u16),
    landblock: u16,
    number: u16,
    surfaces: Vec<DataId>,
    joins: Vec<u16>,
    at: [i32; 7],
    outside: bool,
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Every piece retail's cells are built from, in environment order, with its shape, the looks
/// it is drawn in and the pieces retail joins it to; and the families of looks. Read from the
/// files `set`: a read of every cell, every environment and every surface they name.
#[must_use]
pub fn catalog(set: &DatSet) -> Catalog {
    catalog_with(set, LOOK_SHARE, LOOK_ALIKE, LOOK_COMMON)
}

/// [`catalog`], its looks judged by these shares in place of [`LOOK_SHARE`], [`LOOK_ALIKE`] and
/// [`LOOK_COMMON`].
#[must_use]
pub fn catalog_with(set: &DatSet, share: f32, alike: f32, common_share: f32) -> Catalog {
    let Some(cells) = set.file(ModernDat::Cell) else {
        return Catalog::default();
    };
    let mut seen: Vec<SeenCell> = Vec::new();
    for id in cells.iter_ids() {
        if dereth_dat::classify_cell_id(id) != Some(DbType::Cell) {
            continue;
        }
        let Some(cell) = cells
            .read(id)
            .ok()
            .and_then(|b| set.decode::<EnvCell>(id, &b).ok())
        else {
            continue;
        };
        #[allow(clippy::cast_possible_truncation)]
        let (landblock, number) = ((id.raw() >> 16) as u16, (id.raw() & 0xFFFF) as u16);
        let (o, q) = (cell.frame.origin, cell.frame.rotation);
        let round = |v: f32, k: f32| dereth_primitives::num::floor_to_i32(v * k + 0.5);
        seen.push(SeenCell {
            at: [
                round(o.x, 10.0),
                round(o.y, 10.0),
                round(o.z, 10.0),
                round(q.w, 1000.0),
                round(q.x, 1000.0),
                round(q.y, 1000.0),
                round(q.z, 1000.0),
            ],
            outside: cell.flags & 1 != 0,
            key: (cell.environment.raw(), cell.cell_struct),
            landblock,
            number,
            surfaces: cell.surfaces.clone(),
            joins: cell
                .portals
                .iter()
                .filter_map(|p| u16::try_from(p.other_cell_id).ok())
                .filter(|o| *o >= crate::cells::FIRST_CELL && *o != 0xFFFF)
                .collect(),
        });
    }
    // The texture each surface lays (a plain one is its own).
    let mut texture_of: BTreeMap<u32, DataId> = BTreeMap::new();
    let mut texture = |s: DataId| -> DataId {
        *texture_of.entry(s.raw()).or_insert_with(|| {
            set.read_portal(s)
                .and_then(|b| set.decode::<Surface>(s, &b).ok())
                .and_then(|x| x.orig_texture_id)
                .unwrap_or(s)
        })
    };
    // Each landblock's cells, and the cells each texture is drawn on.
    let mut block_cells: BTreeMap<u16, usize> = BTreeMap::new();
    let mut block_textures: BTreeMap<u16, BTreeMap<u32, usize>> = BTreeMap::new();
    for c in &seen {
        *block_cells.entry(c.landblock).or_default() += 1;
        let ts: BTreeSet<u32> = c.surfaces.iter().map(|s| texture(*s).raw()).collect();
        let m = block_textures.entry(c.landblock).or_default();
        for t in ts {
            *m.entry(t).or_default() += 1;
        }
    }
    // Each landblock's look, the textures said of almost every look taken out.
    #[allow(clippy::cast_precision_loss)]
    let looks: BTreeMap<u16, BTreeSet<u32>> = block_textures
        .iter()
        .map(|(lb, ts)| {
            let n = block_cells[lb] as f32;
            let look = ts
                .iter()
                .filter(|(_, k)| **k as f32 >= share * n)
                .map(|(t, _)| *t)
                .collect();
            (*lb, look)
        })
        .collect();
    let mut holders: BTreeMap<u32, Vec<u16>> = BTreeMap::new();
    for (lb, look) in &looks {
        for t in look {
            holders.entry(*t).or_default().push(*lb);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let common: BTreeSet<u32> = holders
        .iter()
        .filter(|(_, lbs)| lbs.len() as f32 > common_share * looks.len() as f32)
        .map(|(t, _)| *t)
        .collect();
    let looks: BTreeMap<u16, BTreeSet<u32>> = looks
        .into_iter()
        .map(|(lb, l)| (lb, l.difference(&common).copied().collect()))
        .collect();
    // Landblocks whose looks are alike, joined.
    let blocks: Vec<u16> = looks.keys().copied().collect();
    let place: BTreeMap<u16, usize> = blocks.iter().enumerate().map(|(i, lb)| (*lb, i)).collect();
    let mut parent: Vec<usize> = (0..blocks.len()).collect();
    for (t, lbs) in &holders {
        if common.contains(t) {
            continue;
        }
        for (i, a) in lbs.iter().enumerate() {
            for b in &lbs[i + 1..] {
                let (la, lbb) = (&looks[a], &looks[b]);
                let shared = la.intersection(lbb).count();
                let all = la.union(lbb).count().max(1);
                #[allow(clippy::cast_precision_loss)]
                if shared as f32 >= alike * all as f32 {
                    let (x, y) = (find(&mut parent, place[a]), find(&mut parent, place[b]));
                    parent[x] = y;
                }
            }
        }
    }
    // The families, largest first; a landblock alike to no other joins the family whose looks
    // hold most of its own.
    let mut members: BTreeMap<usize, Vec<u16>> = BTreeMap::new();
    for (i, lb) in blocks.iter().enumerate() {
        members.entry(find(&mut parent, i)).or_default().push(*lb);
    }
    let unions: BTreeMap<usize, BTreeSet<u32>> = members
        .iter()
        .filter(|(_, lbs)| lbs.len() > 1)
        .map(|(root, lbs)| {
            (
                *root,
                lbs.iter()
                    .flat_map(|lb| looks[lb].iter().copied())
                    .collect(),
            )
        })
        .collect();
    let single: Vec<(usize, u16)> = members
        .iter()
        .filter(|(_, lbs)| lbs.len() == 1)
        .map(|(root, lbs)| (*root, lbs[0]))
        .collect();
    for (root, lb) in single {
        let look = &looks[&lb];
        if look.is_empty() {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let best = unions
            .iter()
            .map(|(r, u)| (look.intersection(u).count() as f32 / look.len() as f32, *r))
            .filter(|(held, _)| *held >= LOOK_HELD)
            .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
        if let Some((_, r)) = best {
            members.remove(&root);
            members.entry(r).or_default().push(lb);
        }
    }
    let mut families: Vec<Family> = members
        .into_values()
        .map(|lbs| {
            let mut textures: BTreeMap<u32, usize> = BTreeMap::new();
            for lb in &lbs {
                for (t, k) in &block_textures[lb] {
                    if !common.contains(t) {
                        *textures.entry(*t).or_default() += k;
                    }
                }
            }
            let mut textures: Vec<(u32, usize)> = textures.into_iter().collect();
            textures.sort_by_key(|(t, k)| (std::cmp::Reverse(*k), *t));
            let mut landblocks: Vec<(u16, usize)> =
                lbs.iter().map(|lb| (*lb, block_cells[lb])).collect();
            landblocks.sort_by_key(|(lb, k)| (std::cmp::Reverse(*k), *lb));
            Family {
                textures: textures.iter().take(6).map(|(t, _)| DataId(*t)).collect(),
                cells: landblocks.iter().map(|(_, k)| k).sum(),
                landblocks,
                pieces: 0,
            }
        })
        .collect();
    families.sort_by_key(|f| {
        (
            std::cmp::Reverse(f.cells),
            f.landblocks.first().map(|l| l.0),
        )
    });
    let family_of: BTreeMap<u16, usize> = families
        .iter()
        .enumerate()
        .flat_map(|(i, f)| f.landblocks.iter().map(move |(lb, _)| (*lb, i)))
        .collect();
    // The pieces: where each is used, in which looks, and what it joins.
    let cell_key: BTreeMap<(u16, u16), (u32, u16)> = seen
        .iter()
        .map(|c| ((c.landblock, c.number), c.key))
        .collect();
    let mut by_piece: BTreeMap<(u32, u16), Piece> = BTreeMap::new();
    type LookUse = BTreeMap<usize, (usize, BTreeMap<Vec<DataId>, usize>)>;
    let mut look_uses: BTreeMap<(u32, u16), LookUse> = BTreeMap::new();
    let mut joins: BTreeMap<(u32, u16), BTreeMap<(u32, u16), usize>> = BTreeMap::new();
    for c in &seen {
        let p = by_piece.entry(c.key).or_insert_with(|| Piece {
            environment: DataId(c.key.0),
            piece: c.key.1,
            portals: 0,
            size: [0.0; 3],
            surfaces: c.surfaces.clone(),
            cells: 0,
            landblocks: BTreeMap::new(),
            shape: Shape::DeadEnd,
            looks: Vec::new(),
            joins: Vec::new(),
        });
        p.cells += 1;
        *p.landblocks.entry(c.landblock).or_default() += 1;
        let fam = family_of.get(&c.landblock).copied().unwrap_or(0);
        let u = look_uses.entry(c.key).or_default().entry(fam).or_default();
        u.0 += 1;
        *u.1.entry(c.surfaces.clone()).or_default() += 1;
        for o in &c.joins {
            if let Some(k) = cell_key.get(&(c.landblock, *o)) {
                *joins.entry(c.key).or_default().entry(*k).or_default() += 1;
            }
        }
    }
    let index: BTreeMap<(u32, u16), usize> =
        by_piece.keys().enumerate().map(|(i, k)| (*k, i)).collect();
    let mut envs: BTreeMap<u32, Option<Environment>> = BTreeMap::new();
    for (key, p) in &mut by_piece {
        let e = envs.entry(p.environment.raw()).or_insert_with(|| {
            set.read_portal(p.environment)
                .and_then(|b| set.decode::<Environment>(p.environment, &b).ok())
        });
        if let Some(cs) = e.as_ref().and_then(|e| e.cells.get(usize::from(p.piece))) {
            p.portals = cs.portals.len();
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for v in &cs.vertex_array.vertices {
                let q = [v.position.x, v.position.y, v.position.z];
                for k in 0..3 {
                    lo[k] = lo[k].min(q[k]);
                    hi[k] = hi[k].max(q[k]);
                }
            }
            if lo[0] <= hi[0] {
                p.size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
            }
            p.shape = shape_of(cs);
        }
        let mut looks: Vec<Look> = look_uses
            .remove(key)
            .unwrap_or_default()
            .into_iter()
            .map(|(family, (cells, lists))| Look {
                family,
                cells,
                surfaces: lists
                    .into_iter()
                    .max_by_key(|(_, k)| *k)
                    .map(|(l, _)| l)
                    .unwrap_or_default(),
            })
            .collect();
        looks.sort_by_key(|l| (std::cmp::Reverse(l.cells), l.family));
        if let Some(l) = looks.first() {
            p.surfaces.clone_from(&l.surfaces);
        }
        p.looks = looks;
        let mut j: Vec<(usize, usize)> = joins
            .remove(key)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, n)| index.get(&k).map(|i| (*i, n)))
            .collect();
        j.sort_by_key(|(i, n)| (std::cmp::Reverse(*n), *i));
        p.joins = j;
    }
    let pieces: Vec<Piece> = by_piece.into_values().collect();
    for p in &pieces {
        for l in &p.looks {
            if let Some(f) = families.get_mut(l.family) {
                f.pieces += 1;
            }
        }
    }
    let kit = kit_pieces(set, &seen, &family_of, &mut envs);
    let dress = crate::kit::dresses(&kit, families.len());
    Catalog {
        pieces,
        families,
        kit,
        dress,
    }
}

/// The dungeon kit: every environment the world's dungeons place (the landblocks whose first cell
/// is not seen from outside), each placement its cells of one environment at one frame; where
/// and in which looks each is placed, and the pieces its doorways join.
fn kit_pieces(
    set: &DatSet,
    seen: &[SeenCell],
    family_of: &BTreeMap<u16, usize>,
    envs: &mut BTreeMap<u32, Option<Environment>>,
) -> Vec<crate::kit::KitPiece> {
    type Uses = (
        usize,
        BTreeMap<u16, usize>,
        BTreeMap<usize, BTreeMap<Vec<DataId>, usize>>,
    );
    let dungeons: BTreeSet<u16> = seen
        .iter()
        .filter(|c| c.number == crate::cells::FIRST_CELL && !c.outside)
        .map(|c| c.landblock)
        .collect();
    // Each placement: its landblock, environment and frame, and its cells.
    let mut placements: BTreeMap<(u16, u32, [i32; 7]), Vec<usize>> = BTreeMap::new();
    for (i, c) in seen.iter().enumerate() {
        if dungeons.contains(&c.landblock) {
            placements
                .entry((c.landblock, c.key.0, c.at))
                .or_default()
                .push(i);
        }
    }
    let place_of: BTreeMap<(u16, u16), usize> = placements
        .values()
        .enumerate()
        .flat_map(|(k, cells)| {
            cells
                .iter()
                .map(move |i| ((seen[*i].landblock, seen[*i].number), k))
        })
        .collect();
    let env_of: Vec<u32> = placements.keys().map(|(_, e, _)| *e).collect();
    let mut uses: BTreeMap<u32, Uses> = BTreeMap::new();
    let mut joins: BTreeMap<u32, BTreeMap<u32, usize>> = BTreeMap::new();
    for (k, ((lb, env, _), cells)) in placements.iter().enumerate() {
        let u = uses.entry(*env).or_default();
        u.0 += 1;
        *u.1.entry(*lb).or_default() += 1;
        // Its surfaces: its first cell's, as its other parts' are the same.
        if let Some(i) = cells.iter().min_by_key(|i| seen[**i].number) {
            let fam = family_of.get(lb).copied().unwrap_or(0);
            *u.2.entry(fam)
                .or_default()
                .entry(seen[*i].surfaces.clone())
                .or_default() += 1;
        }
        for i in cells {
            for o in &seen[*i].joins {
                if let Some(q) = place_of.get(&(*lb, *o)).filter(|q| **q != k) {
                    *joins
                        .entry(*env)
                        .or_default()
                        .entry(env_of[*q])
                        .or_default() += 1;
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut env_joins = Vec::new();
    for (env, (placed, landblocks, looks)) in uses {
        let Some(e) = envs
            .entry(env)
            .or_insert_with(|| {
                set.read_portal(DataId(env))
                    .and_then(|b| set.decode::<Environment>(DataId(env), &b).ok())
            })
            .as_ref()
        else {
            continue;
        };
        let Some(form) = crate::kit::form(e) else {
            continue;
        };
        let mut looks: Vec<Look> = looks
            .into_iter()
            .map(|(family, lists)| Look {
                family,
                cells: lists.values().sum(),
                surfaces: lists
                    .into_iter()
                    .max_by_key(|(_, n)| *n)
                    .map(|(l, _)| l)
                    .unwrap_or_default(),
            })
            .collect();
        looks.sort_by_key(|l| (std::cmp::Reverse(l.cells), l.family));
        env_joins.push(joins.remove(&env).unwrap_or_default());
        out.push(crate::kit::KitPiece {
            environment: DataId(env),
            surfaces: looks
                .first()
                .map(|l| l.surfaces.clone())
                .unwrap_or_default(),
            form,
            placed,
            landblocks,
            looks,
            joins: Vec::new(),
        });
    }
    let index: BTreeMap<u32, usize> = out
        .iter()
        .enumerate()
        .map(|(i, p)| (p.environment.raw(), i))
        .collect();
    for (p, j) in out.iter_mut().zip(env_joins) {
        let mut j: Vec<(usize, usize)> = j
            .into_iter()
            .filter_map(|(o, n)| index.get(&o).map(|i| (*i, n)))
            .collect();
        j.sort_by_key(|(i, n)| (std::cmp::Reverse(*n), *i));
        p.joins = j;
    }
    out
}

/// A portal of a placed piece: its corners in landblock space, their middle, and the way it
/// faces out of the piece.
#[derive(Clone, Debug, PartialEq)]
pub struct Opening {
    pub corners: Vec<Vec3>,
    pub middle: Vec3,
    pub out: Vec3,
}

/// Portal `slot` of piece `piece` of `env`, placed by `frame`.
#[must_use]
pub fn opening(env: &Environment, piece: u16, slot: usize, frame: &Frame) -> Option<Opening> {
    let cs = env.cells.get(usize::from(piece))?;
    let poly = cs.polygons.get(usize::from(*cs.portals.get(slot)?))?;
    let corners: Vec<Vec3> = poly
        .vertex_ids
        .iter()
        .map(|i| {
            cs.vertex_array
                .vertices
                .get(usize::from(*i))
                .map(|v| localtoglobal(frame, v.position))
        })
        .collect::<Option<_>>()?;
    if corners.len() < 3 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let k = corners.len() as f32;
    let s = corners.iter().fold(Vec3::ZERO, |s, p| s.add(*p));
    let middle = Vec3::new(s.x / k, s.y / k, s.z / k);
    // The piece's middle, to face the portal away from it.
    #[allow(clippy::cast_precision_loss)]
    let inside = {
        let n = cs.vertex_array.vertices.len().max(1) as f32;
        let s = cs
            .vertex_array
            .vertices
            .iter()
            .fold(Vec3::ZERO, |s, v| s.add(v.position));
        localtoglobal(frame, Vec3::new(s.x / n, s.y / n, s.z / n))
    };
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
    let mut out = Vec3::new(n.x / len, n.y / len, n.z / len);
    if out.dot(inside.sub(middle)) > 0.0 {
        out = Vec3::new(-out.x, -out.y, -out.z);
    }
    Some(Opening {
        corners,
        middle,
        out,
    })
}

/// How far apart the corners of two openings are: for each corner of one, the nearest of the
/// other, at most.
#[must_use]
pub fn mismatch(a: &Opening, b: &Opening) -> f32 {
    a.corners
        .iter()
        .map(|p| {
            b.corners
                .iter()
                .map(|q| {
                    let d = p.sub(*q);
                    d.dot(d).sqrt()
                })
                .fold(f32::MAX, f32::min)
        })
        .fold(0.0, f32::max)
}

/// Where piece `piece` of `env` stands so that its portal `slot` meets `target` (another cell's
/// opening), turned in steps of `step` degrees from `heading`: the frame, and how far its
/// corners are from the target's (zero when they coincide). The turn nearest `heading` among
/// those whose opening faces the target's and fits it best wins.
#[must_use]
pub fn snap(
    env: &Environment,
    piece: u16,
    slot: usize,
    target: &Opening,
    heading: f32,
    step: f32,
) -> Option<(Frame, f32)> {
    let steps = (360.0 / step.max(1.0)).round().max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = steps as u32;
    let mut best: Option<(Frame, f32, f32)> = None;
    for k in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let h = heading + k as f32 * step;
        let q = heading_quat(h);
        let local = opening(env, piece, slot, &Frame::new(Vec3::ZERO, q))?;
        // Facing the target: the two openings face opposite ways.
        if local.out.dot(target.out) > -0.99 {
            continue;
        }
        let origin = target.middle.sub(local.middle);
        let frame = Frame::new(origin, q);
        let placed = opening(env, piece, slot, &frame)?;
        let miss = mismatch(&placed, target);
        // The least miss; then the least turn from the heading asked for.
        #[allow(clippy::cast_precision_loss)]
        let turn = (k as f32 * step).min(360.0 - k as f32 * step);
        if best
            .as_ref()
            .is_none_or(|(_, m, t)| miss < *m - 1e-4 || ((miss - *m).abs() <= 1e-4 && turn < *t))
        {
            best = Some((frame, miss, turn));
        }
    }
    best.map(|(f, m, _)| (f, m))
}

/// Where piece `piece` of `env` stands to take cell `old`'s place (built of `old_env`): turned
/// in steps of `step` degrees to retain as many doorways as possible, then minimize their
/// mismatch. Links are `Keep` at matched doorways and `Shut` elsewhere. A piece with no
/// matching doorway keeps the old frame. `None` means an invalid piece or opening.
#[must_use]
pub fn reshape_fit(
    old: &EnvCell,
    old_env: &Environment,
    env: &Environment,
    piece: u16,
    step: f32,
) -> Option<(Frame, Vec<crate::source::Link>)> {
    use crate::source::Link;
    let old_cs = old_env.cells.get(usize::from(old.cell_struct))?;
    // The cell's ways, as openings facing into the cell, as the new piece's will.
    let ways: Vec<Opening> = old
        .portals
        .iter()
        .map(|p| {
            let slot = old_cs.portals.iter().position(|q| *q == p.polygon_id)?;
            let mut o = opening(old_env, old.cell_struct, slot, &old.frame)?;
            o.out = Vec3::new(-o.out.x, -o.out.y, -o.out.z);
            Some(o)
        })
        .collect::<Option<_>>()?;
    let slots = env.cells.get(usize::from(piece))?.portals.len();
    let mut best: Option<(Frame, Vec<Link>, usize, f32)> = None;
    // Anchor each candidate at each old doorway; the first doorway may be dropped.
    let turns = (360.0 / step.max(1.0)).round().max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let turns = turns as u32;
    let mut frames = vec![old.frame];
    for way in &ways {
        for slot in 0..slots {
            for k in 0..turns {
                #[allow(clippy::cast_precision_loss)]
                let q = heading_quat(k as f32 * step);
                let Some(local) = opening(env, piece, slot, &Frame::new(Vec3::ZERO, q)) else {
                    continue;
                };
                if local.out.dot(way.out) > -0.99 {
                    continue;
                }
                frames.push(Frame::new(way.middle.sub(local.middle), q));
            }
        }
    }
    for frame in frames {
        let mut links = vec![Link::Shut; slots];
        let mut worst = 0.0f32;
        let mut matched = 0;
        for w in &ways {
            let found = (0..slots)
                .filter(|s| links[*s] == Link::Shut)
                .filter_map(|s| opening(env, piece, s, &frame).map(|o| (s, mismatch(&o, w))))
                .filter(|(_, m)| *m < crate::cells::KEEP)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let Some((s, m)) = found else {
                continue;
            };
            matched += 1;
            links[s] = Link::Keep;
            worst = worst.max(m);
        }
        if best
            .as_ref()
            .is_none_or(|b| matched > b.2 || (matched == b.2 && worst < b.3))
        {
            best = Some((frame, links, matched, worst));
        }
    }
    best.map(|(f, l, _, _)| (f, l))
}
