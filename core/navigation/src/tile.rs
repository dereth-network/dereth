//! Outdoor tiles: each landblock's places and polygons built once on their own (with the solids
//! of the landblocks round it, so its edge is decided as a window's would be), kept, and put
//! together into the window a route needs, stitched across the landblocks' edges. A walk that
//! goes on into the next window builds only the landblocks it has not been through before, and a
//! landblock built once can be kept between sessions ([`TileStore`]) and read back rather than
//! built again.

use std::collections::{BTreeSet, HashMap, HashSet};

use dereth_primitives::num::floor_to_i32;
use dereth_primitives::CellId;

use crate::poly::{candidate_pairs, links_between, NavPoly, POLY_TILE};
use crate::region::{
    joins, poly_square, storage_order, BuildReport, Columns, Ground, Node, Region, BLOCK_LENGTH,
    GROUND, NO_PATCH,
};

/// Where the landblocks built once are kept between sessions: bytes by a name. The client keeps
/// them in a cache of the player's own, under the data files' fingerprints, so a change to the
/// data is never answered from what an older one built.
pub trait TileStore: Send + std::fmt::Debug {
    /// What was kept under `key`, if anything.
    fn load(&self, key: &str) -> Option<Vec<u8>>;
    /// Keep `bytes` under `key`.
    fn save(&self, key: &str, bytes: &[u8]);
    /// Let go of what is kept under `key`: it did not read back whole.
    fn forget(&self, _key: &str) {}
}

/// The columns a landblock is wide.
pub(crate) fn block_columns(spacing: f32) -> i32 {
    floor_to_i32(BLOCK_LENGTH / spacing)
}

impl Region {
    /// The landblocks `blocks` put together from `tiles`, each landblock's region built on its
    /// own, numbered by its block coordinates; a landblock with none adds nothing. The polygons
    /// are linked across the edges between the landblocks and the places along them searched by
    /// themselves given their steps. The terrain is kept a landblock at a time, so the landblocks
    /// along a long route cost no more than they are.
    pub(crate) fn assemble(
        blocks: &BTreeSet<(i32, i32)>,
        tiles: &[((i32, i32), &Region)],
        config: &crate::source::NavConfig,
    ) -> Self {
        let x0 = blocks.iter().map(|b| b.0).min().unwrap_or(0);
        let y0 = blocks.iter().map(|b| b.1).min().unwrap_or(0);
        let x1 = blocks.iter().map(|b| b.0).max().unwrap_or(0);
        let y1 = blocks.iter().map(|b| b.1).max().unwrap_or(0);
        let window = (x0, y0, x1, y1);
        let spacing = config.spacing;
        let per = block_columns(spacing);
        let mut cells = BTreeSet::new();
        let mut cell_ids: Vec<CellId> = Vec::new();
        let mut nodes: Vec<Node> = Vec::new();
        // Each node's tile and number there, to put the steps out of polygons right.
        let mut came: Vec<(usize, u32)> = Vec::new();
        let mut polys: Vec<NavPoly> = Vec::new();
        let mut poly_tiles = HashMap::new();
        let mut exits_raw: Vec<(usize, u32, u32, u32)> = Vec::new();
        let rel: Vec<(i32, i32)> = blocks.iter().map(|&(bx, by)| (bx - x0, by - y0)).collect();
        let mut ground = Ground::sparse(per, &rel);
        let mut report = BuildReport::default();
        for (k, &((bx, by), t)) in tiles.iter().enumerate() {
            let (dx, dy) = ((bx - x0) * per, (by - y0) * per);
            let slot_off = u16::try_from(cell_ids.len()).unwrap_or(u16::MAX);
            let poly_off = u32::try_from(polys.len()).unwrap_or(u32::MAX);
            cells.extend(t.cells.iter().copied());
            cell_ids.extend(t.cell_ids.iter().copied());
            for (i, n) in t.nodes.iter().enumerate() {
                nodes.push(Node {
                    ix: i16::try_from(i32::from(n.ix) + dx).unwrap_or(i16::MAX),
                    iy: i16::try_from(i32::from(n.iy) + dy).unwrap_or(i16::MAX),
                    z: n.z,
                    cell: n.cell.saturating_add(slot_off),
                    flags: n.flags,
                    patch: if n.patch == NO_PATCH {
                        NO_PATCH
                    } else {
                        n.patch + poly_off
                    },
                });
                came.push((k, u32::try_from(i).unwrap_or(u32::MAX)));
            }
            let (ptx, pty) = (dx / POLY_TILE, dy / POLY_TILE);
            for p in &t.polys {
                let mut q = p.clone();
                q.corners = p
                    .corners
                    .iter()
                    .map(|&(cx, cy)| (cx + dx, cy + dy))
                    .collect();
                q.links = p
                    .links
                    .iter()
                    .map(|l| crate::poly::Link {
                        to: l.to + poly_off,
                        a: (l.a.0 + dx, l.a.1 + dy),
                        b: (l.b.0 + dx, l.b.1 + dy),
                    })
                    .collect();
                q.tile = (p.tile.0 + ptx, p.tile.1 + pty);
                polys.push(q);
            }
            for (tile, range) in &t.poly_tiles {
                poly_tiles.insert(
                    (tile.0 + ptx, tile.1 + pty),
                    range.start + poly_off..range.end + poly_off,
                );
            }
            for (p, list) in &t.exits {
                for &(a, b) in list {
                    exits_raw.push((k, p + poly_off, a, b));
                }
            }
            if let Some(g) = t.ground.as_ref() {
                ground.copy_from(g, dx, dy);
            }
            report.add(&t.report);
        }
        // The places in the order their columns are kept.
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        order.sort_by_key(|&i| storage_order(&(i32::from(nodes[i].ix), i32::from(nodes[i].iy))));
        let mut new_of: HashMap<(usize, u32), u32> = HashMap::with_capacity(nodes.len());
        let mut sorted: Vec<Node> = Vec::with_capacity(nodes.len());
        let mut counts: Vec<((i32, i32), u32)> = Vec::new();
        for &i in &order {
            let n = nodes[i];
            new_of.insert(came[i], u32::try_from(sorted.len()).unwrap_or(u32::MAX));
            let key = (i32::from(n.ix), i32::from(n.iy));
            match counts.last_mut() {
                Some((k, c)) if *k == key => *c += 1,
                _ => counts.push((key, 1)),
            }
            sorted.push(n);
        }
        // A tile's place, as the window numbers it.
        let place = |k: usize, id: u32| -> Option<u32> {
            if id & GROUND != 0 {
                let ((bx, by), t) = tiles[k];
                let g = t.ground.as_ref()?;
                let (ix, iy) = g.column_of(id & !GROUND);
                let (dx, dy) = ((bx - x0) * per, (by - y0) * per);
                return ground.index(ix + dx, iy + dy).map(|i| i | GROUND);
            }
            new_of.get(&(k, id)).copied()
        };
        let mut exits: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
        for (k, p, a, b) in exits_raw {
            if let (Some(a), Some(b)) = (place(k, a), place(k, b)) {
                exits.entry(p).or_default().push((a, b));
            }
        }
        let mut poly_index: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (id, p) in polys.iter().enumerate() {
            let (px0, py0, px1, py1) = p.bounds();
            let (s0, s1) = (poly_square(px0, py0), poly_square(px1, py1));
            for sy in s0.1..=s1.1 {
                for sx in s0.0..=s1.0 {
                    poly_index
                        .entry((sx, sy))
                        .or_default()
                        .push(u32::try_from(id).unwrap_or(u32::MAX));
                }
            }
        }
        report.landblocks = ground.land_count();
        let mut region = Self {
            origin: (x0, y0),
            window: Some(window),
            blocks: Some(blocks.clone()),
            cells,
            cell_ids,
            spacing,
            columns: Columns::new(&counts),
            nodes: sorted,
            ground: Some(ground),
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
            drop_links: Vec::new(),
        };
        region.stitch(per, config);
        region.report.polygons = region.polys.len();
        region
    }

    /// Link the polygons across the landblocks' edges, and find the steps out of them to the
    /// places along the edges searched by themselves.
    fn stitch(&mut self, per: i32, config: &crate::source::NavConfig) {
        let (join, apart) = joins(config);
        let joins = crate::poly::Joins { join, apart };
        let Some((x0, y0, _, _)) = self.window else {
            return;
        };
        let held: BTreeSet<(i32, i32)> = self
            .blocks
            .as_ref()
            .map(|b| b.iter().map(|&(bx, by)| (bx - x0, by - y0)).collect())
            .unwrap_or_default();
        let block_of = |p: &NavPoly| {
            let (cx, cy) = p.corners[0];
            let (bx, by, ex, ey) = p.bounds();
            let _ = (cx, cy);
            (
                (bx + ex).div_euclid(2).div_euclid(per),
                (by + ey).div_euclid(2).div_euclid(per),
            )
        };
        // The polygons touching an edge of their landblock.
        let near: Vec<u32> = (0..self.polys.len())
            .filter(|&i| {
                let (bx, by, ex, ey) = self.polys[i].bounds();
                bx % per == 0 || (ex + 1) % per == 0 || by % per == 0 || (ey + 1) % per == 0
            })
            .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
            .collect();
        let pairs: Vec<(u32, u32)> = candidate_pairs(&self.polys, &near, 0)
            .into_iter()
            .filter(|&(p, q)| {
                block_of(&self.polys[p as usize]) != block_of(&self.polys[q as usize])
            })
            .collect();
        let steps = |p: u32, q: u32, m: (f64, f64)| -> bool {
            let (Some(a), Some(b)) = (self.poly_place_inside(p, m), self.poly_place_inside(q, m))
            else {
                return false;
            };
            (self.column_and_z(a).2 - self.column_and_z(b).2).abs() <= joins.join
        };
        let found = links_between(&self.polys, &pairs, &steps);
        for (p, l) in found {
            let poly = &mut self.polys[p as usize];
            if !poly.links.contains(&l) {
                let mut links = poly.links.to_vec();
                links.push(l);
                poly.links = links.into_boxed_slice();
            }
        }
        // The places along the edges between held landblocks no polygon holds.
        let mut todo = Vec::new();
        for &(bx, by) in &held {
            if held.contains(&(bx + 1, by)) {
                for ix in [(bx + 1) * per - 1, (bx + 1) * per] {
                    for iy in by * per..(by + 1) * per {
                        todo.extend(
                            self.column(ix, iy)
                                .filter(|&j| self.is_clear(j) && self.patch_of(j).is_none()),
                        );
                    }
                }
            }
            if held.contains(&(bx, by + 1)) {
                for iy in [(by + 1) * per - 1, (by + 1) * per] {
                    for ix in bx * per..(bx + 1) * per {
                        todo.extend(
                            self.column(ix, iy)
                                .filter(|&j| self.is_clear(j) && self.patch_of(j).is_none()),
                        );
                    }
                }
            }
        }
        self.loose_todo = todo;
        self.add_exits();
    }
}

impl BuildReport {
    /// Another region's counts added to these.
    pub(crate) fn add(&mut self, o: &Self) {
        self.landblocks += o.landblocks;
        self.cells += o.cells;
        self.missing_cells.extend(o.missing_cells.iter().copied());
        self.outdoor_portals += o.outdoor_portals;
        self.buildings += o.buildings;
        self.floors += o.floors;
        self.solids += o.solids;
        self.statics += o.statics;
        self.statics_unresolved += o.statics_unresolved;
        self.nodes += o.nodes;
        self.clear_nodes += o.clear_nodes;
        self.static_blocked_nodes += o.static_blocked_nodes;
        self.ground += o.ground;
        self.clear_ground += o.clear_ground;
        self.steep_ground += o.steep_ground;
        self.water_ground += o.water_ground;
        self.wading_ground += o.wading_ground;
        self.polygons += o.polygons;
    }
}
