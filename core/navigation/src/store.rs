//! A landblock's tile as bytes, to keep between sessions ([`crate::TileStore`]): its places, the
//! terrain's bits, its polygons and their links, under the format's version and the settings it
//! was built with, so a tile built for another body or by another version is never read back.
//! The landblock's own data (its terrain's heights) is not kept: it is read from the data files
//! again.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use dereth_physics::land::LandblockCollision;
use dereth_primitives::CellId;

use crate::poly::{Link, NavPoly};
use crate::region::{poly_square, BuildReport, Columns, Ground, Node, Region, GROUND};
use crate::source::NavConfig;

/// The first bytes of a kept tile.
const MAGIC: &[u8; 8] = b"DNAVTILE";

/// The format's version: a change to what a tile holds, or to how its polygons are made, makes
/// every kept tile one to build again.
pub(crate) const TILE_FORMAT: u32 = 3;

/// The checksum of `bytes`: FNV-1a, 64 bits.
fn checksum(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The most grid columns a kept tile's terrain may hold: a landblock's, and some room.
const MOST_GROUND: i64 = 4 * 768 * 768;

/// The settings a tile was built with, and the format, as one number.
#[must_use]
pub(crate) fn settings_key(config: &NavConfig) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bits: u64| {
        for b in bits.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    eat(u64::from(TILE_FORMAT));
    // The planner's own version too, so a tile built by another version is never read back.
    for b in env!("CARGO_PKG_VERSION").bytes() {
        eat(u64::from(b));
    }
    let a = &config.agent;
    for f in [
        a.radius,
        a.low_center,
        a.top_center,
        a.step_up,
        a.step_down,
        config.spacing,
        config.margin,
        config.step_allowance,
        config.max_rise,
        config.max_drop,
        config.wade_cost,
    ] {
        eat(u64::from(f.to_bits()));
    }
    eat(u64::from(config.statics));
    eat(u64::from(config.wade));
    h
}

/// Bytes being written.
#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    fn len(&mut self, n: usize) {
        self.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }
}

/// Bytes being read.
struct R<'a>(&'a [u8]);

impl R<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> Option<u64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Some(u64::from_le_bytes(a))
    }
    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| i32::from_le_bytes(v.to_le_bytes()))
    }
    fn i16(&mut self) -> Option<i16> {
        self.u16().map(|v| i16::from_le_bytes(v.to_le_bytes()))
    }
    fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }
    /// A count, no more than the bytes left could hold at `each` bytes apiece.
    fn len(&mut self, each: usize) -> Option<usize> {
        let n = usize::try_from(self.u32()?).ok()?;
        (n.saturating_mul(each.max(1)) <= self.0.len()).then_some(n)
    }
}

impl Region {
    /// The tile as bytes, under `config`'s settings.
    pub(crate) fn tile_bytes(&self, config: &NavConfig) -> Vec<u8> {
        let mut w = W::default();
        w.0.extend_from_slice(MAGIC);
        w.u32(TILE_FORMAT);
        w.u64(settings_key(config));
        w.i32(self.origin.0);
        w.i32(self.origin.1);
        let (x0, y0, x1, y1) = self.window.unwrap_or((0, 0, -1, -1));
        for v in [x0, y0, x1, y1] {
            w.i32(v);
        }
        w.len(self.cells.len());
        for &c in &self.cells {
            w.u32(c);
        }
        w.len(self.cell_ids.len());
        for c in &self.cell_ids {
            w.u32(c.0);
        }
        w.len(self.nodes.len());
        for n in &self.nodes {
            w.i16(n.ix);
            w.i16(n.iy);
            w.f32(n.z);
            w.u16(n.cell);
            w.u8(n.flags);
            w.u32(n.patch);
        }
        match self.ground.as_ref() {
            Some(g) => {
                w.u8(1);
                for v in [g.x0, g.y0, g.w, g.h] {
                    w.i32(v);
                }
                for bits in [&g.clear, &g.static_blocked, &g.water] {
                    w.len(bits.len());
                    for &b in bits {
                        w.u64(b);
                    }
                }
            }
            None => w.u8(0),
        }
        self.report.write(&mut w);
        w.len(self.polys.len());
        for p in &self.polys {
            w.len(p.corners.len());
            for &(x, y) in &*p.corners {
                w.i32(x);
                w.i32(y);
            }
            w.len(p.links.len());
            for l in &*p.links {
                w.u32(l.to);
                for v in [l.a.0, l.a.1, l.b.0, l.b.1] {
                    w.i32(v);
                }
            }
            w.u8(u8::from(p.ground) | (u8::from(p.wading) << 1));
            w.i32(p.tile.0);
            w.i32(p.tile.1);
            w.f32(p.z);
        }
        let mut tiles: Vec<(&(i32, i32), &std::ops::Range<u32>)> = self.poly_tiles.iter().collect();
        tiles.sort_by_key(|(t, _)| **t);
        w.len(tiles.len());
        for (t, r) in tiles {
            w.i32(t.0);
            w.i32(t.1);
            w.u32(r.start);
            w.u32(r.end);
        }
        w.len(self.drop_links.len());
        for l in &self.drop_links {
            w.u32(l.from);
            w.u32(l.to);
        }
        let mut exits: Vec<(&u32, &Vec<(u32, u32)>)> = self.exits.iter().collect();
        exits.sort();
        w.len(exits.len());
        for (p, list) in exits {
            w.u32(*p);
            w.len(list.len());
            for &(a, b) in list {
                w.u32(a);
                w.u32(b);
            }
        }
        let sum = checksum(&w.0);
        w.u64(sum);
        w.0
    }

    /// A tile read back from `bytes`, when they hold one built with `config`'s settings; `lands`
    /// gives the terrain of its landblocks again.
    pub(crate) fn from_tile_bytes(
        bytes: &[u8],
        config: &NavConfig,
        lands: &dyn Fn((i32, i32)) -> Option<Arc<LandblockCollision>>,
    ) -> Option<Self> {
        // The checksum last: a file cut short or garbled is none.
        let body = bytes.len().checked_sub(8)?;
        let (bytes, sum) = bytes.split_at(body);
        let mut want = [0u8; 8];
        want.copy_from_slice(sum);
        if checksum(bytes) != u64::from_le_bytes(want) {
            return None;
        }
        let mut r = R(bytes);
        if r.take(8)? != MAGIC || r.u32()? != TILE_FORMAT || r.u64()? != settings_key(config) {
            return None;
        }
        let origin = (r.i32()?, r.i32()?);
        let (x0, y0, x1, y1) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
        let window = (x1 >= x0 && y1 >= y0).then_some((x0, y0, x1, y1));
        let mut cells = BTreeSet::new();
        for _ in 0..r.len(4)? {
            cells.insert(r.u32()?);
        }
        let mut cell_ids = Vec::new();
        for _ in 0..r.len(4)? {
            cell_ids.push(CellId(r.u32()?));
        }
        let n = r.len(15)?;
        let mut nodes = Vec::with_capacity(n);
        let mut counts: Vec<((i32, i32), u32)> = Vec::new();
        for _ in 0..n {
            let node = Node {
                ix: r.i16()?,
                iy: r.i16()?,
                z: r.f32()?,
                cell: r.u16()?,
                flags: r.u8()?,
                patch: r.u32()?,
            };
            let key = (i32::from(node.ix), i32::from(node.iy));
            match counts.last_mut() {
                Some((k, c)) if *k == key => *c += 1,
                _ => counts.push((key, 1)),
            }
            nodes.push(node);
        }
        let ground = if r.u8()? == 1 {
            let mut land = HashMap::new();
            if let Some((bx0, by0, bx1, by1)) = window {
                for bx in bx0..=bx1 {
                    for by in by0..=by1 {
                        if let Some(l) = lands((bx, by)) {
                            land.insert((bx, by), l);
                        }
                    }
                }
            }
            let (gx, gy, gw, gh) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
            let area = i64::from(gw).checked_mul(i64::from(gh))?;
            if gw < 0 || gh < 0 || area > MOST_GROUND {
                return None;
            }
            let mut g = Ground::empty(gx, gy, gw, gh);
            for bits in [&mut g.clear, &mut g.static_blocked, &mut g.water] {
                let n = r.len(8)?;
                if n != bits.len() {
                    return None;
                }
                for b in bits.iter_mut() {
                    *b = r.u64()?;
                }
            }
            g.lands = land;
            Some(g)
        } else {
            None
        };
        let report = BuildReport::read(&mut r)?;
        let np = r.len(8)?;
        let mut polys = Vec::with_capacity(np);
        let mut poly_index: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for id in 0..np {
            let mut corners = Vec::new();
            for _ in 0..r.len(8)? {
                corners.push((r.i32()?, r.i32()?));
            }
            let mut links = Vec::new();
            for _ in 0..r.len(20)? {
                links.push(Link {
                    to: r.u32()?,
                    a: (r.i32()?, r.i32()?),
                    b: (r.i32()?, r.i32()?),
                });
            }
            let bits = r.u8()?;
            let p = NavPoly {
                corners: corners.into_boxed_slice(),
                links: links.into_boxed_slice(),
                ground: bits & 1 != 0,
                wading: bits & 2 != 0,
                tile: (r.i32()?, r.i32()?),
                z: r.f32()?,
            };
            if p.corners.len() < 3 {
                return None;
            }
            let (px0, py0, px1, py1) = p.bounds();
            let (s0, s1) = (poly_square(px0, py0), poly_square(px1, py1));
            for sy in s0.1..=s1.1 {
                for sx in s0.0..=s1.0 {
                    poly_index
                        .entry((sx, sy))
                        .or_default()
                        .push(u32::try_from(id).ok()?);
                }
            }
            polys.push(p);
        }
        let mut poly_tiles = HashMap::new();
        for _ in 0..r.len(16)? {
            poly_tiles.insert((r.i32()?, r.i32()?), r.u32()?..r.u32()?);
        }
        let mut drop_links = Vec::new();
        for _ in 0..r.len(8)? {
            drop_links.push(crate::region::DropLink {
                from: r.u32()?,
                to: r.u32()?,
            });
        }
        let mut exits = HashMap::new();
        for _ in 0..r.len(8)? {
            let p = r.u32()?;
            let mut list = Vec::new();
            for _ in 0..r.len(8)? {
                list.push((r.u32()?, r.u32()?));
            }
            exits.insert(p, list);
        }
        if !r.0.is_empty() {
            return None;
        }
        // Every number the tile names something by names something it holds.
        let np32 = u32::try_from(polys.len()).ok()?;
        let nodes_len = u32::try_from(nodes.len()).ok()?;
        let ground_len = ground.as_ref().map_or(0, Ground::len);
        let place_ok = |id: u32| {
            if id & GROUND != 0 {
                id & !GROUND < ground_len
            } else {
                id < nodes_len
            }
        };
        let fits = nodes.iter().all(|n| {
            (n.patch == crate::region::NO_PATCH || n.patch < np32)
                && usize::from(n.cell) < cell_ids.len()
        }) && polys.iter().all(|p| p.links.iter().all(|l| l.to < np32))
            && exits.iter().all(|(&p, list)| {
                p < np32 && list.iter().all(|&(a, b)| place_ok(a) && place_ok(b))
            })
            && poly_tiles
                .values()
                .all(|r| r.start <= r.end && r.end <= np32)
            && drop_links
                .iter()
                .all(|l| place_ok(l.from) && place_ok(l.to));
        if !fits {
            return None;
        }
        Some(Self {
            origin,
            window,
            blocks: None,
            cells,
            cell_ids,
            spacing: config.spacing,
            columns: Columns::new(&counts),
            nodes,
            ground,
            steps: crate::search::Steps {
                max_rise: config.max_rise,
                max_drop: config.max_drop,
            },
            wade_cost: config.wade_cost,
            report,
            serial: 0,
            polys,
            poly_index,
            poly_tiles,
            exits,
            loose_todo: Vec::new(),
            dirty: HashSet::new(),
            drop_links,
            fences: Vec::new(),
        })
    }
}

impl BuildReport {
    fn write(&self, w: &mut W) {
        for v in self.counts() {
            w.u64(u64::try_from(v).unwrap_or(u64::MAX));
        }
        w.len(self.missing_cells.len());
        for c in &self.missing_cells {
            w.u32(c.0);
        }
    }

    fn read(r: &mut R<'_>) -> Option<Self> {
        let mut out = Self::default();
        let mut vals = [0usize; 17];
        for v in &mut vals {
            *v = usize::try_from(r.u64()?).ok()?;
        }
        out.set_counts(vals);
        for _ in 0..r.len(4)? {
            out.missing_cells.push(CellId(r.u32()?));
        }
        Some(out)
    }

    /// Every count but the missing cells, in one order.
    fn counts(&self) -> [usize; 17] {
        [
            self.landblocks,
            self.cells,
            self.outdoor_portals,
            self.buildings,
            self.floors,
            self.solids,
            self.statics,
            self.statics_unresolved,
            self.nodes,
            self.clear_nodes,
            self.static_blocked_nodes,
            self.ground,
            self.clear_ground,
            self.steep_ground,
            self.water_ground,
            self.wading_ground,
            self.polygons,
        ]
    }

    fn set_counts(&mut self, v: [usize; 17]) {
        [
            self.landblocks,
            self.cells,
            self.outdoor_portals,
            self.buildings,
            self.floors,
            self.solids,
            self.statics,
            self.statics_unresolved,
            self.nodes,
            self.clear_nodes,
            self.static_blocked_nodes,
            self.ground,
            self.clear_ground,
            self.steep_ground,
            self.water_ground,
            self.wading_ground,
            self.polygons,
        ] = v;
    }
}
