//! AC2's placeholder land, left out as sea before the resampling: each stretch of land that copies
//! another (its landblocks laid out as the other's, its heights the other's at all but a
//! hundredth of their vertices: the islands in the far north are copies of one island, touched up
//! here and there), and each speck of dead-flat ground, one height wherever it is dry, more than
//! [`SPECK_GAP`] landblocks from all the larger land left. The same rule as the AC2 client's
//! Horizon map, here on AC2's own ground.

use std::collections::HashMap;

use crate::ac2_world::dense::par_map;
use crate::ac2_world::ground::Ac2Ground;
use crate::ac2_world::table::SEA;

/// How far, in landblocks, a speck of flat ground lies from any larger land to be left out.
pub const SPECK_GAP: usize = 32;

/// Why a stretch was left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// It copies another stretch.
    Copy,
    /// A speck of dead-flat ground far from any larger land.
    Speck,
}

/// A stretch of land left out: why, its landblocks, and its south-west and north-east blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placeholder {
    pub why: Why,
    pub blocks: usize,
    pub south_west: (usize, usize),
    pub north_east: (usize, usize),
}

/// Every vertex's land, x major.
fn land_mask(g: &Ac2Ground) -> Vec<bool> {
    let side = g.side();
    par_map(side, |gx| {
        (0..side).map(|gy| g.is_land(gx, gy)).collect::<Vec<_>>()
    })
    .into_iter()
    .flatten()
    .collect()
}

/// The stretches of land: blocks holding a land vertex, joined through edges and corners. Each is
/// its blocks as `(x, y)`, the stretches in the order they begin, row by row from the south.
fn stretches(g: &Ac2Ground, land: &[bool]) -> Vec<Vec<(usize, usize)>> {
    let (b, n, side) = (g.blocks, g.squares, g.side());
    let dry = |bx: usize, by: usize| {
        (0..=n).any(|i| (0..=n).any(|j| land[(bx * n + i) * side + by * n + j]))
    };
    let mut is_dry = vec![false; b * b];
    for by in 0..b {
        for bx in 0..b {
            is_dry[by * b + bx] = dry(bx, by);
        }
    }
    let mut seen = vec![false; b * b];
    let mut out = Vec::new();
    for start in 0..b * b {
        if seen[start] || !is_dry[start] {
            continue;
        }
        seen[start] = true;
        let mut todo = vec![start];
        let mut blocks = Vec::new();
        while let Some(k) = todo.pop() {
            let (x, y) = (k % b, k / b);
            blocks.push((x, y));
            for ny in y.saturating_sub(1)..=(y + 1).min(b - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(b - 1) {
                    let m = ny * b + nx;
                    if is_dry[m] && !seen[m] {
                        seen[m] = true;
                        todo.push(m);
                    }
                }
            }
        }
        out.push(blocks);
    }
    out
}

/// The height of vertex `(x, y)` of the block at `at` moved by `(dx, dy)`; `None` off the world.
fn height_at(
    g: &Ac2Ground,
    at: (usize, usize),
    d: (usize, usize),
    i: usize,
    j: usize,
) -> Option<u32> {
    let (n, side) = (g.squares, g.side());
    let (x, y) = ((at.0 + d.0) * n + i, (at.1 + d.1) * n + j);
    (x < side && y < side).then(|| g.height[x * side + y].to_bits())
}

/// Whether the blocks `shape` lays out from `a` and from `b` hold the same heights at all but a
/// hundredth of their vertices.
fn heights_match(
    g: &Ac2Ground,
    a: (usize, usize),
    b: (usize, usize),
    shape: &[(usize, usize)],
) -> bool {
    let n = g.squares;
    let (mut same, mut all) = (0usize, 0usize);
    for &d in shape {
        for i in 0..=n {
            for j in 0..=n {
                all += 1;
                let ha = height_at(g, a, d, i, j);
                same += usize::from(ha.is_some() && ha == height_at(g, b, d, i, j));
            }
        }
    }
    all > 0 && same * 100 >= all * 99
}

/// Whether the ground of the blocks `shape` lays out from `at` is one height wherever it is land.
fn dead_flat(g: &Ac2Ground, land: &[bool], at: (usize, usize), shape: &[(usize, usize)]) -> bool {
    let (n, side) = (g.squares, g.side());
    let mut first = None;
    for &(dx, dy) in shape {
        for i in 0..=n {
            for j in 0..=n {
                let (x, y) = ((at.0 + dx) * n + i, (at.1 + dy) * n + j);
                if x >= side || y >= side || !land[x * side + y] {
                    continue;
                }
                let h = g.height[x * side + y].to_bits();
                if *first.get_or_insert(h) != h {
                    return false;
                }
            }
        }
    }
    true
}

/// Leave the placeholder land out: every vertex of its blocks that is land or stands above the
/// sea (under a lake of its own) drops to AC2's lowest height, under the sea, and its blocks'
/// water becomes the sea's. Only those blocks' own land
/// changes: a vertex such a block shares with another stretch's block would put both in one
/// stretch. Returns what was left out, in the order the stretches begin.
pub fn drop_placeholder_land(g: &mut Ac2Ground) -> Vec<Placeholder> {
    let land = land_mask(g);
    let stretches = stretches(g, &land);
    let laid_out: Vec<(Placeholder, Vec<(usize, usize)>)> = stretches
        .iter()
        .map(|blocks| {
            let sw = (
                blocks.iter().map(|b| b.0).min().unwrap_or(0),
                blocks.iter().map(|b| b.1).min().unwrap_or(0),
            );
            let ne = (
                blocks.iter().map(|b| b.0).max().unwrap_or(0),
                blocks.iter().map(|b| b.1).max().unwrap_or(0),
            );
            let mut shape: Vec<_> = blocks.iter().map(|b| (b.0 - sw.0, b.1 - sw.1)).collect();
            shape.sort_unstable();
            let p = Placeholder {
                why: Why::Copy,
                blocks: blocks.len(),
                south_west: sw,
                north_east: ne,
            };
            (p, shape)
        })
        .collect();
    let mut alike: HashMap<&[(usize, usize)], Vec<usize>> = HashMap::new();
    for (k, (_, shape)) in laid_out.iter().enumerate() {
        alike.entry(shape.as_slice()).or_default().push(k);
    }
    let mut copy = vec![false; laid_out.len()];
    for same in alike.values().filter(|s| s.len() > 1) {
        for (i, &a) in same.iter().enumerate() {
            for &b in &same[i + 1..] {
                let (from, to) = (laid_out[a].0.south_west, laid_out[b].0.south_west);
                if heights_match(g, from, to, &laid_out[a].1) {
                    (copy[a], copy[b]) = (true, true);
                }
            }
        }
    }
    // Landblocks between two stretches' bounds, along whichever way they are further apart.
    let gap = |a: &Placeholder, b: &Placeholder| {
        let across = (a.south_west.0.saturating_sub(b.north_east.0))
            .max(b.south_west.0.saturating_sub(a.north_east.0));
        let up = (a.south_west.1.saturating_sub(b.north_east.1))
            .max(b.south_west.1.saturating_sub(a.north_east.1));
        across.max(up)
    };
    let mut why: Vec<Option<Why>> = copy.iter().map(|&c| c.then_some(Why::Copy)).collect();
    for (k, (stretch, shape)) in laid_out.iter().enumerate() {
        let mut larger = laid_out
            .iter()
            .enumerate()
            .filter(|&(o, (other, _))| !copy[o] && other.blocks > stretch.blocks)
            .peekable();
        let alone = larger.peek().is_some()
            && larger.all(|(_, (other, _))| gap(stretch, other) > SPECK_GAP);
        if !copy[k] && alone && dead_flat(g, &land, stretch.south_west, shape) {
            why[k] = Some(Why::Speck);
        }
    }
    let (n, side, b) = (g.squares, g.side(), g.blocks);
    let floor = g
        .height
        .iter()
        .copied()
        .filter(|h| !h.is_nan())
        .fold(f32::MAX, f32::min);
    let floor = floor.min(SEA - 1.0);
    let mut dropped = Vec::new();
    for ((mut p, shape), w) in laid_out.into_iter().zip(why) {
        let Some(w) = w else {
            continue;
        };
        p.why = w;
        for &(dx, dy) in &shape {
            let (bx, by) = (p.south_west.0 + dx, p.south_west.1 + dy);
            for i in 0..=n {
                for j in 0..=n {
                    let k = (bx * n + i) * side + by * n + j;
                    if land[k] || g.height[k] >= SEA {
                        g.height[k] = floor;
                    }
                }
            }
            // The sea's plane, not a lake's: lowered ground under a lake would be a lake.
            g.plane[bx * b + by] = Some(SEA);
        }
        dropped.push(p);
    }
    dropped
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's placeholder land)
    use super::*;

    fn cut_all(_: i32, _: i32) -> bool {
        true
    }

    /// A sea world of `blocks` a side, one 10 m square a block, the sea's plane everywhere, the
    /// ground at 0 m.
    fn sea(blocks: usize) -> Ac2Ground {
        let mut g = Ac2Ground::new(blocks, 1, 10.0, cut_all);
        for bx in 0..blocks {
            for by in 0..blocks {
                g.put_block(bx, by, &[0.0; 4], &[0; 4], &[0; 4], &[0; 4]);
            }
        }
        g.plane = vec![Some(SEA); blocks * blocks];
        g
    }

    /// Raise the interior vertex grid of the blocks `x0..x0+w` x `y0..y0+h`'s south-west corners
    /// so that each of them is dry, heights from `f`.
    fn raise(
        g: &mut Ac2Ground,
        (x0, y0): (usize, usize),
        (w, h): (usize, usize),
        f: impl Fn(usize, usize) -> f32,
    ) {
        let side = g.side();
        for i in 0..w {
            for j in 0..h {
                g.height[(x0 + i) * side + y0 + j] = f(i, j);
            }
        }
    }

    fn dry(g: &Ac2Ground) -> usize {
        let side = g.side();
        (0..side)
            .flat_map(|x| (0..side).map(move |y| (x, y)))
            .filter(|&(x, y)| g.is_land(x, y))
            .count()
    }

    #[test]
    fn two_stretches_laid_out_alike_with_the_same_heights_are_both_left_out() {
        let mut g = sea(60);
        let island = |i: usize, j: usize| 80.0 + (i * 3 + j) as f32;
        // A large continent, and two copies of one island, one touched up at a single vertex of
        // its 100 (within a hundredth).
        raise(&mut g, (2, 2), (20, 20), |i, j| 90.0 + (i + j) as f32);
        raise(&mut g, (40, 5), (10, 10), island);
        raise(&mut g, (40, 30), (10, 10), island);
        let side = g.side();
        g.height[45 * side + 35] = 200.0;
        // A lake on one copy over four blocks, covering the island's ground at the vertex they
        // share (43, 33), 92 m.
        for (bx, by) in [(42, 32), (43, 32), (42, 33), (43, 33)] {
            g.plane[bx * 60 + by] = Some(99.5);
        }
        assert!(!g.is_land(43, 33));
        let continent = (0..side)
            .flat_map(|x| (0..side).map(move |y| (x, y)))
            .filter(|&(x, y)| x < 30 && g.is_land(x, y))
            .count();
        let out = drop_placeholder_land(&mut g);
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(out.iter().all(|p| p.why == Why::Copy));
        // Each copy's 10 x 10 land vertices touch an 11 x 11 of blocks.
        assert!(out.iter().all(|p| p.blocks == 121));
        // Nothing of either copy is left above the sea, the ground under the lake included.
        assert_eq!(dry(&g), continent);
        // Where the copy stood is open sea, not a lake left floating over the lowered ground.
        let (v, _) = crate::ac2_world::dense::vertex_at(&g, 445.0, 345.0, 10.0);
        assert!(v.water);
        assert_eq!(v.exact, 0.0);
        assert_ne!(
            (v.word >> 2) & 0x1F,
            crate::ac2_world::kinds::WATER_STANDING_FRESH
        );
        // The continent is still there.
        assert!(g.is_land(10, 10));
    }

    #[test]
    fn a_stretch_laid_out_alike_but_with_other_heights_stays() {
        let mut g = sea(60);
        raise(&mut g, (2, 2), (20, 20), |i, j| 90.0 + (i + j) as f32);
        raise(&mut g, (40, 5), (10, 10), |i, j| 80.0 + (i * 3 + j) as f32);
        // The same shape, its heights off at more than a hundredth of the vertices.
        raise(&mut g, (40, 30), (10, 10), |i, j| {
            80.0 + (i * 3 + j) as f32 + if i < 2 { 1.0 } else { 0.0 }
        });
        assert!(drop_placeholder_land(&mut g).is_empty());
    }

    #[test]
    fn a_dead_flat_speck_far_from_larger_land_is_left_out_and_one_near_it_stays() {
        let mut g = sea(80);
        raise(&mut g, (2, 2), (10, 10), |i, j| 90.0 + (i + j) as f32);
        // Far: 33 blocks and more from the continent's bounds.
        raise(&mut g, (60, 60), (1, 1), |_, _| 75.0);
        // Near: within 32 blocks (at another height, so it is no copy of the far one).
        raise(&mut g, (30, 5), (1, 1), |_, _| 76.0);
        let out = drop_placeholder_land(&mut g);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].why, Why::Speck);
        assert_eq!(out[0].south_west, (59, 59));
        assert!(!g.is_land(60, 60));
        assert!(g.is_land(30, 5));
    }

    #[test]
    fn a_far_speck_that_is_not_flat_stays() {
        let mut g = sea(80);
        raise(&mut g, (2, 2), (10, 10), |i, j| 90.0 + (i + j) as f32);
        raise(&mut g, (60, 60), (2, 1), |i, _| 75.0 + i as f32);
        assert!(drop_placeholder_land(&mut g).is_empty());
    }
}
