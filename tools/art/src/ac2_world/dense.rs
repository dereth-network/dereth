//! AC2's ground resampled onto AC1's blocks on the dense grid: every converted vertex is a
//! function of its world position only, so neighboring blocks agree on their shared edges.

use crate::ac2_world::ground::{on_square, Ac2Ground};
use crate::ac2_world::kinds::{
    pack_word, road_bits, water_kind, water_slot, WaterKind, WATER_DEEP_SEA,
};
use crate::ac2_world::roads::road_at;
use crate::ac2_world::table::{self, lower, SEA};

/// The converted world's grid: AC1's blocks and cells, `steps` vertex steps a cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub blocks: usize,
    pub cells: usize,
    pub cell_len: f64,
    pub steps: usize,
}

impl Grid {
    /// Squares along a block's side.
    #[must_use]
    pub const fn block_squares(&self) -> usize {
        self.cells * self.steps
    }
    /// Vertices along the world's side.
    #[must_use]
    pub const fn side(&self) -> usize {
        self.blocks * self.block_squares() + 1
    }
    /// A vertex step in meters.
    #[must_use]
    pub fn spacing(&self) -> f64 {
        self.cell_len / self.steps as f64
    }
}

/// One converted vertex before its height is rounded into the table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    /// Its height in the converted world's meters (AC2's lowered, floored at 0).
    pub exact: f32,
    pub water: bool,
    pub word: u16,
    /// Land that AC2 has below its sea, floored at 0 (the Osteth basin).
    pub floored: bool,
    /// Where AC2 has no ground (a missing block, or beyond AC2's world): open sea.
    pub no_ground: bool,
    /// The kind of water a water vertex is.
    pub water_kind: Option<WaterKind>,
    /// Ground under AC2's water kept land at the water's height ([`bank_beside`]).
    pub shore: bool,
}

/// How far above the water AC2's bank may stand halfway from a vertex under its water to a land
/// neighbor before the vertex stays land, meters ([`bank_beside`]).
pub const SHORE_RISE: f64 = 0.5;

/// Is a plane AC2's sea's?
fn is_sea(plane: f64) -> bool {
    (plane - f64::from(SEA)).abs() < 0.005
}

/// Whether ground under AC2's water at plane `plane` (AC2 meters) at world point `(x, y)` stays
/// land at the water's height: whether AC2's own ground halfway to one of the eight vertices round
/// it, `step` meters apart, that are land stands more than [`SHORE_RISE`] above the water.
///
/// The converted shore is drawn halfway between a water vertex and the land beside it, in both
/// looks. Where AC2's water ends nearer the water vertex than that, as at the foot of a bank that
/// rises steeply out of the water, a water vertex there would lay the water up AC2's dry bank:
/// the shore follows AC2's own water edge instead, and the bank stays land down to the water.
fn bank_beside(g: &Ac2Ground, x: f64, y: f64, plane: f64, step: f64) -> bool {
    (-1i8..=1).any(|a| {
        (-1i8..=1).any(|b| {
            if a == 0 && b == 0 {
                return false;
            }
            let (dx, dy) = (f64::from(a) * step, f64::from(b) * step);
            let land = g
                .sample(x + dx, y + dy)
                .is_some_and(|n| n.plane.is_none_or(|q| n.ground >= q));
            land && g
                .ground(x + dx / 2.0, y + dy / 2.0)
                .is_some_and(|h| h - plane > SHORE_RISE)
        })
    })
}

/// The converted vertex at world point `(x, y)`, with the water plane `plane` (AC2 meters) there,
/// on a grid of vertices `step` meters apart.
fn vertex_with(
    g: &Ac2Ground,
    x: f64,
    y: f64,
    ground: f64,
    plane: Option<f64>,
    step: f64,
) -> Vertex {
    let under = plane.filter(|&p| ground < p);
    if let Some(p) = under.filter(|&p| !bank_beside(g, x, y, p, step)) {
        let near = g.nearest_vertex(x, y).map_or(0, |v| g.kind[v]);
        let kind = water_kind(u32::from(near), is_sea(p));
        #[allow(clippy::cast_possible_truncation)]
        return Vertex {
            exact: lower(p) as f32,
            water: true,
            word: pack_word(0, water_slot(kind, p - ground), 0, 0),
            floored: false,
            no_ground: false,
            water_kind: Some(kind),
            shore: false,
        };
    }
    let kv = g.kind_vertex(x, y).unwrap_or(0);
    let (slot, variant) = g.slot_of(kv);
    #[allow(clippy::cast_possible_truncation)]
    Vertex {
        // A shore vertex lies at the water's height, as the water beside it.
        exact: lower(under.unwrap_or(ground)) as f32,
        water: false,
        word: pack_word(0, slot, variant, u16::from(g.scene[kv])),
        floored: under.is_none() && ground < f64::from(SEA),
        no_ground: false,
        water_kind: None,
        shore: under.is_some(),
    }
}

/// Open sea where AC2 has no ground.
const NO_GROUND: Vertex = Vertex {
    exact: 0.0,
    water: true,
    word: pack_word(0, WATER_DEEP_SEA, 0, 0),
    floored: false,
    no_ground: true,
    water_kind: Some(WaterKind::Sea),
    shore: false,
};

/// The converted vertex at world point `(x, y)` on a grid of vertices `step` meters apart, with no
/// road (the grid's vertices take theirs in [`DenseWorld::sample`]), and whether the lowest-plane
/// rule changed it: whether one of the blocks the point touches would, by its own plane alone,
/// have given it another water or height (a crack between the blocks, without the rule).
#[must_use]
pub fn vertex_at(g: &Ac2Ground, x: f64, y: f64, step: f64) -> (Vertex, bool) {
    let Some(s) = g.sample(x, y) else {
        return (NO_GROUND, false);
    };
    let v = vertex_with(g, x, y, s.ground, s.plane, step);
    if !s.edge_rule {
        return (v, false);
    }
    let changed = g.planes_touching(x, y).into_iter().any(|p| {
        let o = vertex_with(g, x, y, s.ground, p, step);
        o.water != v.water || o.exact != v.exact
    });
    (v, changed)
}

/// What the ground should be at `(x, y)` in the converted world's meters: the water's surface
/// where it is under water, else the ground; 0 where AC2 has none.
#[must_use]
pub fn target_at(g: &Ac2Ground, x: f64, y: f64) -> f64 {
    let Some(s) = g.sample(x, y) else {
        return 0.0;
    };
    match s.plane {
        Some(p) if s.ground < p => lower(p),
        _ => lower(s.ground),
    }
}

/// Whether a square with corner heights south-west, south-east, north-east, north-west is better
/// cut south-west to north-east (its center on that diagonal's midpoint nearer `target`); on a
/// tie, `tie`.
#[must_use]
pub fn best_cut([q00, q10, q11, q01]: [f64; 4], target: f64, tie: bool) -> bool {
    let sw_ne = ((q00 + q11) / 2.0 - target).abs();
    let se_nw = ((q10 + q01) / 2.0 - target).abs();
    if sw_ne < se_nw {
        true
    } else if se_nw < sw_ne {
        false
    } else {
        tie
    }
}

/// Split bits for a block's squares, x major, least significant bit first: set when the square
/// is cut south-west to north-east.
#[must_use]
pub fn pack_split(cuts: &[bool]) -> Vec<u8> {
    let mut out = vec![0u8; cuts.len().div_ceil(8)];
    for (k, &c) in cuts.iter().enumerate() {
        if c {
            out[k / 8] |= 1 << (k % 8);
        }
    }
    out
}

/// The whole converted world, vertex by vertex, x major over the world's `grid.side()²`.
pub struct DenseWorld {
    pub grid: Grid,
    pub vertex: Vec<Vertex>,
    /// Vertices the lowest-plane rule changed.
    pub edge_rule: Vec<bool>,
    /// The table and each vertex's index into it, once rounded.
    pub table: [f32; table::LEN],
    pub height: Vec<u8>,
    /// Per world square, x major over `(side - 1)²`: cut south-west to north-east.
    pub cut: Vec<bool>,
}

/// Run `f` over `0..n` on all the machine's threads, in order of index, gathering the results.
pub fn par_map<T: Send, F: Fn(usize) -> T + Sync>(n: usize, f: F) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    let chunk = n.div_ceil(threads).max(1);
    let f = &f;
    std::thread::scope(|s| {
        let parts: Vec<_> = (0..n)
            .step_by(chunk)
            .map(|lo| s.spawn(move || (lo..(lo + chunk).min(n)).map(f).collect::<Vec<T>>()))
            .collect();
        parts
            .into_iter()
            .flat_map(|h| h.join().expect("a worker"))
            .collect()
    })
}

impl DenseWorld {
    /// Sample every vertex of the converted world, each land vertex with its road
    /// ([`crate::ac2_world::roads::road_at`]).
    #[must_use]
    pub fn sample(g: &Ac2Ground, grid: Grid) -> Self {
        let side = grid.side();
        let step = grid.spacing();
        let rows = par_map(side, |i| {
            (0..side)
                .map(|j| {
                    let (mut v, e) = vertex_at(g, i as f64 * step, j as f64 * step, step);
                    if !v.water {
                        #[allow(clippy::cast_possible_wrap)]
                        let road = road_at(g, step, i as i64, j as i64);
                        v.word |= road_bits(u32::from(road));
                    }
                    (v, e)
                })
                .collect::<Vec<_>>()
        });
        let mut vertex = Vec::with_capacity(side * side);
        let mut edge_rule = Vec::with_capacity(side * side);
        for row in rows {
            for (v, e) in row {
                vertex.push(v);
                edge_rule.push(e);
            }
        }
        Self {
            grid,
            vertex,
            edge_rule,
            table: [0.0; table::LEN],
            height: Vec::new(),
            cut: Vec::new(),
        }
    }

    /// Round every height into `table` and choose every square's cut against AC2's surface.
    pub fn finish(&mut self, g: &Ac2Ground, table: [f32; table::LEN]) {
        self.table = table;
        self.height = self
            .vertex
            .iter()
            .map(|v| table::quantize(&table, f64::from(v.exact)))
            .collect();
        let side = self.grid.side();
        let rows = par_map(side - 1, |i| {
            (0..side - 1)
                .map(|j| self.cut_of(g, i, j))
                .collect::<Vec<_>>()
        });
        self.cut = rows.into_iter().flatten().collect();
    }

    /// Square `(i, j)`'s cut against AC2's surface, from its rounded corners.
    fn cut_of(&self, g: &Ac2Ground, i: usize, j: usize) -> bool {
        let side = self.grid.side();
        let step = self.grid.spacing();
        let q = |i: usize, j: usize| f64::from(self.table[usize::from(self.height[i * side + j])]);
        let (cx, cy) = ((i as f64 + 0.5) * step, (j as f64 + 0.5) * step);
        let corners = [q(i, j), q(i + 1, j), q(i + 1, j + 1), q(i, j + 1)];
        best_cut(corners, target_at(g, cx, cy), ac2_cut_at(g, cx, cy))
    }

    /// Raise land vertices to at least the heights `wants` names (by vertex index, meters), each
    /// to the first height of the table at least that, and choose again the cuts of the squares
    /// they touch. Water vertices and vertices already as high are left. Returns how many rose.
    pub fn raise_to(
        &mut self,
        g: &Ac2Ground,
        wants: &std::collections::BTreeMap<usize, f64>,
    ) -> usize {
        let side = self.grid.side();
        let mut squares = std::collections::BTreeSet::new();
        let mut raised = 0;
        for (&k, &h) in wants {
            if self.vertex[k].water {
                continue;
            }
            let to = crate::ac2_world::buried::at_least(&self.table, h);
            if to <= self.height[k] {
                continue;
            }
            self.height[k] = to;
            raised += 1;
            let (a, b) = (k / side, k % side);
            for (i, j) in [
                (a, b),
                (a.wrapping_sub(1), b),
                (a, b.wrapping_sub(1)),
                (a.wrapping_sub(1), b.wrapping_sub(1)),
            ] {
                if i < side - 1 && j < side - 1 {
                    squares.insert((i, j));
                }
            }
        }
        for (i, j) in squares {
            self.cut[i * (side - 1) + j] = self.cut_of(g, i, j);
        }
        raised
    }

    /// Put the corners of `squares` back to the rounded heights `from` holds for them, and choose
    /// again the cuts of the squares they touch. Returns how many changed.
    pub fn restore(
        &mut self,
        g: &Ac2Ground,
        from: &[u8],
        squares: &std::collections::BTreeSet<(usize, usize)>,
    ) -> usize {
        let side = self.grid.side();
        let mut touched = std::collections::BTreeSet::new();
        let mut changed = 0;
        for &(i, j) in squares {
            for (a, b) in [(i, j), (i + 1, j), (i, j + 1), (i + 1, j + 1)] {
                let k = a * side + b;
                if k < from.len() && self.height[k] != from[k] {
                    self.height[k] = from[k];
                    changed += 1;
                    for (p, q) in [
                        (a, b),
                        (a.wrapping_sub(1), b),
                        (a, b.wrapping_sub(1)),
                        (a.wrapping_sub(1), b.wrapping_sub(1)),
                    ] {
                        if p < side - 1 && q < side - 1 {
                            touched.insert((p, q));
                        }
                    }
                }
            }
        }
        for (i, j) in touched {
            self.cut[i * (side - 1) + j] = self.cut_of(g, i, j);
        }
        changed
    }

    /// The converted ground's height (meters) at world point `(x, y)`, on the converted world's
    /// triangles, rounded heights when `rounded`, else the exact ones.
    #[must_use]
    pub fn height_at(&self, x: f64, y: f64, rounded: bool) -> f64 {
        let side = self.grid.side();
        let n = side - 1;
        let step = self.grid.spacing();
        let (u, v) = (x / step, y / step);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = (
            (u.floor().max(0.0) as usize).min(n - 1),
            (v.floor().max(0.0) as usize).min(n - 1),
        );
        let h = |a: usize, b: usize| {
            let k = a * side + b;
            if rounded {
                f64::from(self.table[usize::from(self.height[k])])
            } else {
                f64::from(self.vertex[k].exact)
            }
        };
        on_square(
            self.cut[i * n + j],
            [h(i, j), h(i + 1, j), h(i + 1, j + 1), h(i, j + 1)],
            u - i as f64,
            v - j as f64,
        )
    }

    /// Block `(bx, by)`'s vertices (terrain words, heights) and split bits, as the dense record
    /// holds them.
    #[must_use]
    pub fn block(&self, bx: usize, by: usize) -> (Vec<u16>, Vec<u8>, Vec<u8>) {
        let side = self.grid.side();
        let s = self.grid.block_squares();
        let mut words = Vec::with_capacity((s + 1) * (s + 1));
        let mut heights = Vec::with_capacity((s + 1) * (s + 1));
        for i in 0..=s {
            for j in 0..=s {
                let k = (bx * s + i) * side + by * s + j;
                words.push(self.vertex[k].word);
                heights.push(self.height[k]);
            }
        }
        let mut cuts = Vec::with_capacity(s * s);
        for i in 0..s {
            for j in 0..s {
                cuts.push(self.cut[(bx * s + i) * (side - 1) + by * s + j]);
            }
        }
        (words, heights, pack_split(&cuts))
    }
}

/// AC2's own cut of the square under `(x, y)`; south-east to north-west off its world.
fn ac2_cut_at(g: &Ac2Ground, x: f64, y: f64) -> bool {
    let n = g.blocks * g.squares;
    let (u, v) = (x / g.square_len, y / g.square_len);
    if u < 0.0 || v < 0.0 || u >= n as f64 || v >= n as f64 {
        return false;
    }
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a world square, a few thousand from the origin.
    (g.split)(u.floor() as i32, v.floor() as i32)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's dense land)
    use super::*;
    use crate::ac2_world::kinds::{WATER_SHALLOW_SEA, WATER_STANDING_FRESH};

    fn cut_all(_: i32, _: i32) -> bool {
        true
    }

    /// A world of `blocks` a side of one 10 m square each, every vertex at `h`, kinds 1.
    fn flat(blocks: usize, h: f32) -> Ac2Ground {
        let mut g = Ac2Ground::new(blocks, 1, 10.0, cut_all);
        for bx in 0..blocks {
            for by in 0..blocks {
                g.put_block(bx, by, &[h; 4], &[1; 4], &[3; 4], &[0; 4]);
            }
        }
        g
    }

    #[test]
    fn ground_below_its_plane_becomes_water_at_the_planes_lowered_height() {
        let mut g = flat(1, 60.0);
        g.plane = vec![Some(SEA)];
        let (v, _) = vertex_at(&g, 5.0, 5.0, 10.0);
        assert!(v.water);
        assert_eq!(v.exact, 0.0);
        // 10.95 m deep: deep sea; and the word carries no road, variant or scene.
        assert_eq!(v.word, pack_word(0, WATER_DEEP_SEA, 0, 0));
        // Shallow sea.
        let g = {
            let mut g = flat(1, 70.0);
            g.plane = vec![Some(SEA)];
            g
        };
        assert_eq!(
            (vertex_at(&g, 5.0, 5.0, 10.0).0.word >> 2) & 0x1F,
            WATER_SHALLOW_SEA
        );
        // A lake above the sea: fresh water at its own level.
        let mut g = flat(1, 100.0);
        g.plane = vec![Some(110.95)];
        let (v, _) = vertex_at(&g, 5.0, 5.0, 10.0);
        assert_eq!((v.word >> 2) & 0x1F, WATER_STANDING_FRESH);
        assert!((f64::from(v.exact) - 40.0).abs() < 1e-4);
    }

    #[test]
    fn ground_above_its_plane_stays_land_with_its_kind_and_scene() {
        let mut g = flat(1, 100.0);
        g.plane = vec![Some(SEA)];
        let (v, _) = vertex_at(&g, 5.0, 5.0, 10.0);
        assert!(!v.water && !v.floored);
        assert!((f64::from(v.exact) - 29.05).abs() < 1e-4);
        assert_eq!(v.word, pack_word(0, 1, 0, 3));
    }

    #[test]
    fn land_below_ac2s_sea_floors_at_zero_and_is_counted() {
        let mut g = flat(1, 40.0);
        g.plane = vec![Some(33.45)];
        let (v, _) = vertex_at(&g, 5.0, 5.0, 10.0);
        assert!(!v.water && v.floored);
        assert_eq!(v.exact, 0.0);
    }

    #[test]
    fn the_lower_plane_rules_a_shared_edge_and_the_change_is_reported() {
        // Two blocks east to west: a lake at 120.95 m and the sea, over ground at 100 m.
        let mut g = flat(2, 100.0);
        g.plane = vec![Some(120.95), Some(120.95), Some(SEA), Some(SEA)];
        // Inside the lake block: water.
        assert!(vertex_at(&g, 5.0, 5.0, 10.0).0.water);
        // On the edge both blocks share, the sea's lower plane: land, and the rule changed it.
        let (v, changed) = vertex_at(&g, 10.0, 5.0, 10.0);
        assert!(!v.water);
        assert!(changed);
        // The same vertex read from either block is one point: the same answer.
        assert_eq!(vertex_at(&g, 10.0, 5.0, 10.0), (v, changed));
    }

    /// One block of four 10 m squares under the sea's plane, its ground rising to the east through
    /// heights `east` (x = 0, 10, 20, 30, 40 m; the same all along y).
    fn bank(east: [f32; 5]) -> Ac2Ground {
        let mut g = Ac2Ground::new(1, 4, 10.0, cut_all);
        let mut h = [0f32; 25];
        for i in 0..5 {
            for j in 0..5 {
                h[i * 5 + j] = east[i];
            }
        }
        g.put_block(0, 0, &h, &[1; 25], &[3; 25], &[0; 25]);
        g.plane = vec![Some(SEA)];
        g
    }

    #[test]
    fn the_foot_of_a_bank_rising_steeply_out_of_the_water_stays_land_at_the_waters_height() {
        // The sea's plane at 70.95 m: under it 0.95 m at x = 20, a bank 9 m above it at x = 30.
        let g = bank([50.0, 50.0, 70.0, 80.0, 80.0]);
        let (foot, _) = vertex_at(&g, 20.0, 20.0, 10.0);
        assert!(!foot.water && foot.shore && !foot.floored);
        assert_eq!(foot.exact, 0.0);
        // Its kind and scene are the land's.
        assert_eq!(foot.word, pack_word(0, 1, 0, 3));
        // Deeper out, with water all round: water.
        let (out, _) = vertex_at(&g, 10.0, 20.0, 10.0);
        assert!(out.water && !out.shore);
        // The bank itself: land at its own height.
        let (top, _) = vertex_at(&g, 30.0, 20.0, 10.0);
        assert!(!top.water && !top.shore);
        assert!((f64::from(top.exact) - (80.0 - f64::from(SEA))).abs() < 1e-4);
    }

    #[test]
    fn under_a_gentle_shore_the_water_keeps_its_edge() {
        // 0.45 m under at x = 20, 0.55 m above at x = 30: AC2's water ends halfway.
        let g = bank([50.0, 50.0, 70.5, 71.5, 71.5]);
        let (v, _) = vertex_at(&g, 20.0, 20.0, 10.0);
        assert!(v.water && !v.shore);
        assert_eq!(v.water_kind, Some(WaterKind::Sea));
        assert_eq!(
            (v.word >> 2) & 0x1F,
            crate::ac2_world::kinds::WATER_SHALLOW_SEA
        );
    }

    #[test]
    fn beyond_ac2s_world_is_open_sea_at_zero() {
        let g = flat(1, 100.0);
        let (v, _) = vertex_at(&g, 50.0, 5.0, 10.0);
        assert!(v.water && v.no_ground);
        assert_eq!(v.exact, 0.0);
        assert_eq!((v.word >> 2) & 0x1F, WATER_DEEP_SEA);
    }

    #[test]
    fn a_square_is_cut_along_the_diagonal_whose_middle_meets_the_ground() {
        // A ridge along south-west to north-east: corners 10, 0, 10, 0; the ground's middle at 10.
        assert!(best_cut([10.0, 0.0, 10.0, 0.0], 10.0, false));
        // A valley south-east to north-west: corners 10, 0, 10, 0 again, the ground's middle at 0.
        assert!(!best_cut([10.0, 0.0, 10.0, 0.0], 0.0, true));
        // A plane: both diagonals meet it; the tie decides.
        assert!(best_cut([0.0, 1.0, 2.0, 1.0], 1.0, true));
        assert!(!best_cut([0.0, 1.0, 2.0, 1.0], 1.0, false));
    }

    #[test]
    fn split_bits_run_x_major_least_significant_bit_first() {
        // 24 x 24 squares, as a three-step block has: set square (1, 2) and (23, 23) only.
        let s = 24;
        let mut cuts = vec![false; s * s];
        cuts[s + 2] = true; // (1, 2)
        cuts[23 * s + 23] = true;
        let bits = pack_split(&cuts);
        assert_eq!(bits.len(), 72);
        assert_eq!(bits[26 / 8], 1 << (26 % 8));
        assert_eq!(bits[71], 0x80);
        // The record reads them back the same way.
        let d = dereth_assets::world::DenseLand {
            steps: 3,
            terrain: vec![0; 625].into(),
            height: vec![0; 625].into(),
            split: bits.into(),
        };
        for x in 0..s {
            for y in 0..s {
                assert_eq!(d.split_sw_to_ne(x, y), cuts[x * s + y], "({x}, {y})");
            }
        }
    }

    #[test]
    fn a_converted_world_agrees_with_itself_across_block_edges_and_follows_ac2s_triangles() {
        // AC2: 2 x 2 blocks of 2 squares of 10 m (40 m), rising to the east.
        let mut g = Ac2Ground::new(2, 2, 10.0, cut_all);
        for bx in 0..2 {
            for by in 0..2 {
                let mut h = [0f32; 9];
                for i in 0..3 {
                    for j in 0..3 {
                        h[i * 3 + j] = 100.0 + 10.0 * (bx * 2 + i) as f32;
                    }
                }
                g.put_block(bx, by, &h, &[1; 9], &[0; 9], &[0; 9]);
            }
        }
        g.plane = vec![Some(SEA); 4];
        // AC1: 3 x 3 blocks of 1 cell of 16 m at 2 steps (8 m): 48 m, past AC2's 40 m.
        let grid = Grid {
            blocks: 3,
            cells: 1,
            cell_len: 16.0,
            steps: 2,
        };
        let mut w = DenseWorld::sample(&g, grid);
        let t: [f32; table::LEN] = std::array::from_fn(|i| i as f32 * 0.25);
        w.finish(&g, t);
        // The vertex on the edge between blocks (0, 0) and (1, 0) is one value in both.
        let (_, ha, _) = w.block(0, 0);
        let (_, hb, _) = w.block(1, 0);
        for j in 0..3 {
            assert_eq!(ha[2 * 3 + j], hb[j]);
        }
        // Heights are AC2's lowered, on its plane: 8 m east of the origin, 29.05 + 8 m.
        assert!((f64::from(w.vertex[w.grid.side()].exact) - (29.05 + 8.0)).abs() < 1e-4);
        // Rounded into a 0.25 m table, within an eighth of a meter.
        assert!((w.height_at(8.0, 0.0, true) - (29.05 + 8.0)).abs() <= 0.125 + 1e-6);
        // Beyond AC2's 40 m, sea.
        assert!(w.vertex[w.grid.side() * 6].water);
    }

    #[test]
    fn raising_nothing_leaves_the_land_as_it_was_and_a_raise_reaches_a_covering_height() {
        // AC2: 2 x 2 blocks of 2 squares of 10 m, rising to the east, its west column under the
        // sea's plane (water).
        let mut g = Ac2Ground::new(2, 2, 10.0, cut_all);
        for bx in 0..2 {
            for by in 0..2 {
                let mut h = [0f32; 9];
                for i in 0..3 {
                    for j in 0..3 {
                        h[i * 3 + j] = 60.0 + 10.0 * (bx * 2 + i) as f32;
                    }
                }
                g.put_block(bx, by, &h, &[1; 9], &[0; 9], &[0; 9]);
            }
        }
        g.plane = vec![Some(SEA); 4];
        let grid = Grid {
            blocks: 3,
            cells: 1,
            cell_len: 16.0,
            steps: 2,
        };
        let make = || {
            let mut w = DenseWorld::sample(&g, grid);
            w.finish(&g, std::array::from_fn(|i| i as f32 * 0.25));
            w
        };
        let before = make();
        let mut w = make();
        assert_eq!(w.raise_to(&g, &std::collections::BTreeMap::new()), 0);
        assert_eq!(w.height, before.height);
        assert_eq!(w.cut, before.cut);

        let side = w.grid.side();
        let (dry, wet) = (2 * side + 2, 0);
        assert!(!w.vertex[dry].water && w.vertex[wet].water);
        let h0 = f64::from(w.table[usize::from(w.height[dry])]);
        let wants = std::collections::BTreeMap::from([(dry, h0 + 1.1), (wet, 50.0)]);
        assert_eq!(w.raise_to(&g, &wants), 1);
        // The first height at least 1.1 m higher: 1.25 m higher in a 0.25 m table.
        assert!((f64::from(w.table[usize::from(w.height[dry])]) - (h0 + 1.25)).abs() < 1e-6);
        assert_eq!(w.height[wet], before.height[wet]);
        // Every other vertex as it was.
        let changed = (0..w.height.len())
            .filter(|&k| w.height[k] != before.height[k])
            .count();
        assert_eq!(changed, 1);
    }
}
