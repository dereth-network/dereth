//! The water as the drawn ground lays it: where the world's water lies and the level it lies at,
//! worked out once from the landscape vertices, a value pair a vertex; the bed lowered under it;
//! and the floor the land beside it keeps. The presentation's shaders do the same arithmetic over
//! the same values, so what stands in the water or on its banks is drawn where the ground is.
//!
//! - **How far inside the water**, meters: a water vertex lies as far inside it as the nearest
//!   vertex that is not water lies from it, less half a cell; a vertex that is not water lies
//!   outside it by as far as the nearest water vertex, less half a cell. So the water's edge runs
//!   halfway between a water vertex and the land beside it, where retail's blend between their
//!   pictures is even, and the edge is rounded about lone water vertices as the distance is.
//!   Between the vertices the distance is drawn smoothly through them, and the edge is broken by
//!   a little noise, so it neither runs straight along the cells nor in the same curve about every
//!   lone water vertex.
//! - **The water's level**, as how far below the vertex it lies: nought at a water vertex, whose
//!   own height retail draws its water at; on land within three vertices of water, as far as the
//!   mean height of the nearest water vertices lies below the vertex, so the water stays level up
//!   to and past its edge where the bank rises, and slopes with the ground where the land beside
//!   it lies lower (a river on a hillside).
//! - **The values are held as half floats**, as the presentation holds them ([`held`]), so the
//!   scene and the presentation read the same numbers.

use super::{lattice_hash, smoothstep};
use dereth_primitives::num::floor_to_i32;

/// A landscape cell's side, meters.
pub const CELL: f32 = 24.0;
/// The farthest inside or outside the water the field tells apart, meters: past it every value is
/// the same.
pub const REACH: f32 = 960.0;
/// How far outside the water a point lies at most, meters, for the smooth distance and its noise:
/// farther out, the edge is not near, and the land keeps no floor.
pub const NEAR: f32 = 40.0;
/// How far below the water's level the land beside it may lie, meters for each meter it lies
/// outside the water's edge: none at the edge, so the bank never dips under the water beside it,
/// and soon none that a rounded bank reaches.
pub const FALL: f32 = 0.25;

/// Whether terrain word `word` names water.
#[must_use]
pub const fn is_water(word: u16) -> bool {
    let t = (word >> 2) & 0x1F;
    t >= 16 && t <= 20
}

/// How far from the water, in vertices, the land takes the water's level as its own level: so
/// near the water's edge the level is the water's, whichever side of the edge a point lies, and
/// where the water is met along a line of sight does not hang on which side a point was taken.
const LEVEL_REACH: i64 = 3;

/// Squared distance standing for "none": far past any the world holds, and exact in an f64 with
/// a squared distance added to it.
const NONE: f64 = 1e15;

#[derive(Default)]
struct Scratch {
    f: Vec<f64>,
    v: Vec<usize>,
    z: Vec<f64>,
}

/// The squared distance, in vertices, from each of the `n` points `stride` apart from `start` in
/// `grid` to the nearest point along their line at the squared distance `grid` holds for it,
/// written over them: one pass of the exact distance transform (the lower envelope of parabolas).
fn transform_line(grid: &mut [f64], start: usize, stride: usize, n: usize, scratch: &mut Scratch) {
    let Scratch { f, v, z } = scratch;
    f.clear();
    f.extend((0..n).map(|i| grid[start + i * stride]));
    v.clear();
    v.resize(n, 0);
    z.clear();
    z.resize(n + 1, 0.0);
    // LINT-OK: vertex indices, far inside f64's exact range.
    #[allow(clippy::cast_precision_loss)]
    let at = |i: usize| i as f64;
    let meet = |f: &[f64], q: usize, p: usize| {
        ((f[q] + at(q) * at(q)) - (f[p] + at(p) * at(p))) / (2.0 * at(q) - 2.0 * at(p))
    };
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let mut s = meet(f, q, v[k]);
        while s <= z[k] {
            k -= 1;
            s = meet(f, q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    let mut k = 0usize;
    for q in 0..n {
        while z[k + 1] < at(q) {
            k += 1;
        }
        let p = v[k];
        grid[start + q * stride] = ((at(q) - at(p)) * (at(q) - at(p)) + f[p]).min(NONE);
    }
}

/// The squared distance, in vertices, from each vertex of a `side` by `side` grid to the nearest
/// vertex `target` says yes to; [`NONE`] or more where there is none.
fn squared_distances(side: usize, target: impl Fn(usize) -> bool) -> Vec<f64> {
    let mut grid: Vec<f64> = (0..side * side)
        .map(|i| if target(i) { 0.0 } else { NONE })
        .collect();
    let mut scratch = Scratch::default();
    for row in 0..side {
        transform_line(&mut grid, row * side, 1, side, &mut scratch);
    }
    for column in 0..side {
        transform_line(&mut grid, column, side, side, &mut scratch);
    }
    grid
}

/// For each vertex of a `side` by `side` grid of landscape vertices, row by row from the south
/// (`north * side + east`), with heights `heights` and terrain words `words`: how far inside the
/// water it lies, meters (negative outside it, within [`REACH`] either way), and how far the
/// water's level there lies below it, meters (nought or more).
#[must_use]
pub fn water_field(side: usize, heights: &[f32], words: &[u16]) -> Vec<[f32; 2]> {
    let count = side * side;
    if side == 0 || heights.len() < count || words.len() < count {
        return vec![[-REACH, 0.0]; count];
    }
    let wet = |i: usize| is_water(words[i]);
    let to_water = squared_distances(side, wet);
    let to_land = squared_distances(side, |i| !wet(i));
    // LINT-OK: three steps squared, exact in f64.
    #[allow(clippy::cast_precision_loss)]
    let level_reach = (LEVEL_REACH * LEVEL_REACH) as f64;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        // LINT-OK: a distance within the field's reach, meters.
        #[allow(clippy::cast_possible_truncation)]
        let inside = if wet(i) {
            ((to_land[i].sqrt() - 0.5) as f32 * CELL).min(REACH)
        } else {
            (-(to_water[i].sqrt() - 0.5) as f32 * CELL).max(-REACH)
        };
        let mut below = 0.0;
        if !wet(i) && to_water[i] <= level_reach {
            // The nearest water vertices about it: those no farther than the nearest by more than
            // a step (so a vertex beside the water takes the water beside it and on its
            // diagonals).
            let (x, y) = (i % side, i / side);
            let mut sum = 0.0;
            let mut n = 0.0;
            for dy in -LEVEL_REACH..=LEVEL_REACH {
                for dx in -LEVEL_REACH..=LEVEL_REACH {
                    // LINT-OK: vertex coordinates, a few thousand.
                    #[allow(clippy::cast_possible_wrap)]
                    let (Ok(nx), Ok(ny)) = (
                        usize::try_from(x as i64 + dx),
                        usize::try_from(y as i64 + dy),
                    ) else {
                        continue;
                    };
                    if nx >= side || ny >= side {
                        continue;
                    }
                    let j = ny * side + nx;
                    // LINT-OK: a few steps squared, exact in f64.
                    #[allow(clippy::cast_precision_loss)]
                    let d2 = (dx * dx + dy * dy) as f64;
                    if wet(j) && d2 <= to_water[i] + 1.0 {
                        sum += heights[j];
                        n += 1.0;
                    }
                }
            }
            if n > 0.0 {
                below = (heights[i] - sum / n).max(0.0);
            }
        }
        out.push([inside, below]);
    }
    out
}

/// `v` rounded to the nearest half float, as the presentation's texture holds it.
#[must_use]
pub fn half(v: f32) -> f32 {
    // A single-precision float's 23 stored mantissa bits, and the leading one they leave out.
    const MANTISSA: u32 = (1 << 23) - 1;
    const LEADING: u32 = 1 << 23;
    let bits = v.to_bits();
    let sign = if bits >> 31 == 0 { 1.0 } else { -1.0 };
    // LINT-OK: an 8-bit exponent field.
    #[allow(clippy::cast_possible_wrap)]
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & MANTISSA;
    if exp == 0xFF {
        return v;
    }
    let e = exp - 127 + 15;
    if e >= 0x1F {
        return sign * f32::INFINITY;
    }
    if e <= 0 {
        if e < -10 {
            return sign * 0.0;
        }
        // LINT-OK: a shift of 1 to 11.
        #[allow(clippy::cast_sign_loss)]
        let m = (mant | LEADING) >> (1 - e);
        let units = (m + 0x1000) >> 13;
        // LINT-OK: under 0x400 plus a carry, exact in f32.
        #[allow(clippy::cast_precision_loss)]
        return sign * units as f32 * (2.0f32).powi(-24);
    }
    let m = mant + 0x1000;
    let (e, m) = if m & LEADING != 0 { (e + 1, 0) } else { (e, m) };
    if e >= 0x1F {
        return sign * f32::INFINITY;
    }
    // LINT-OK: ten bits of mantissa, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let fraction = 1.0 + (m >> 13) as f32 / 1024.0;
    sign * fraction * (2.0f32).powi(e - 15)
}

/// The field as the presentation holds it: each value rounded to a half float.
#[must_use]
pub fn held(field: &[[f32; 2]]) -> Vec<[f32; 2]> {
    field.iter().map(|[a, b]| [half(*a), half(*b)]).collect()
}

/// The made-up depth of the bed below the water's surface `inside` meters inside the water: none
/// at its edge, falling away steepest there and leveling out at `depth` meters `shelf` meters in.
/// The shaders' own arithmetic.
#[must_use]
pub fn bed_depth(inside: f32, depth: f32, shelf: f32) -> f32 {
    if inside <= 0.0 {
        return 0.0;
    }
    let t = 1.0 - (inside / shelf.max(1e-3)).min(1.0);
    depth * (1.0 - t * t)
}

/// The lowest the land `inside` meters inside the water (negative outside it) is drawn, where the
/// water's level there is `level`: the level at the edge, falling away by [`FALL`] a meter out.
#[must_use]
pub fn floor(level: f32, inside: f32) -> f32 {
    level + inside.min(0.0) * FALL
}

/// Catmull-Rom weights at `t`.
fn catmull(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

/// Smooth noise over a lattice `cell` meters wide, 0..1, as the shore's shader works it.
fn noise(x: f32, y: f32, cell: f32) -> f32 {
    let (qx, qy) = (x / cell, y / cell);
    let (ix, iy) = (floor_to_i32(qx), floor_to_i32(qy));
    let (fx, fy) = (qx - qx.floor(), qy - qy.floor());
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let mix = |a: f32, b: f32, t: f32| a * (1.0 - t) + b * t;
    let a = lattice_hash(ix, iy);
    let b = lattice_hash(ix + 1, iy);
    let c = lattice_hash(ix, iy + 1);
    let d = lattice_hash(ix + 1, iy + 1);
    mix(mix(a, b, sx), mix(c, d, sx), sy)
}

/// How much of a wander `cell` meters across shows in a pixel that covers `foot` meters: all of it
/// while the pixel is a quarter of it or less, none once it is half of it.
fn resolved(cell: f32, foot: f32) -> f32 {
    1.0 - smoothstep(0.25 * cell, 0.5 * cell, foot)
}

/// How much the water's edge is moved at world `(x, y)`, meters, out into the land where
/// positive, as a pixel covering `foot` meters sees it (nought for all of it).
fn wander(x: f32, y: f32, foot: f32) -> f32 {
    (noise(x, y, 9.0) - 0.5) * 6.0 * resolved(9.0, foot)
        + (noise(x + 31.7, y + 12.9, 3.5) - 0.5) * 2.0 * resolved(3.5, foot)
}

/// The water of a world of landscape vertices, as the drawn ground lays it.
#[derive(Debug, Clone, Copy)]
pub struct Shore<'a> {
    /// Vertices a side.
    pub side: usize,
    /// Each vertex's height, row by row from the south.
    pub heights: &'a [f32],
    /// Each vertex's pair of the field, as [`held`] holds it, in the same order.
    pub field: &'a [[f32; 2]],
}

impl Shore<'_> {
    /// The field's pair at vertex `(gx, gy)`, clamped to the world.
    fn texel(&self, gx: i32, gy: i32) -> [f32; 2] {
        // LINT-OK: the side, a few thousand.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let last = self.side as i32 - 1;
        // LINT-OK: clamped to the world.
        #[allow(clippy::cast_sign_loss)]
        let k = gy.clamp(0, last.max(0)) as usize * self.side + gx.clamp(0, last.max(0)) as usize;
        self.field.get(k).copied().unwrap_or([-REACH, 0.0])
    }

    /// The height of vertex `(gx, gy)`, clamped to the world.
    fn height(&self, gx: i32, gy: i32) -> f32 {
        // LINT-OK: the side, a few thousand.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let last = self.side as i32 - 1;
        // LINT-OK: clamped to the world.
        #[allow(clippy::cast_sign_loss)]
        let k = gy.clamp(0, last.max(0)) as usize * self.side + gx.clamp(0, last.max(0)) as usize;
        self.heights.get(k).copied().unwrap_or(0.0)
    }

    /// The cell `(x, y)` lies in and where in it, 0..1 each way.
    fn cell_of(x: f32, y: f32) -> ((i32, i32), (f32, f32)) {
        let (qx, qy) = (x / CELL, y / CELL);
        let c = (floor_to_i32(qx), floor_to_i32(qy));
        // LINT-OK: cell coordinates of points inside the world, exact in f32.
        #[allow(clippy::cast_precision_loss)]
        let f = (
            (qx - c.0 as f32).clamp(0.0, 1.0),
            (qy - c.1 as f32).clamp(0.0, 1.0),
        );
        (c, f)
    }

    /// How far inside the water world `(x, y)` lies, meters, negative on land, as a pixel
    /// covering `foot` meters sees it (nought for all of it); far from any water, the field's own
    /// coarse value.
    #[must_use]
    pub fn distance(&self, x: f32, y: f32, foot: f32) -> f32 {
        let ((cx, cy), (fx, fy)) = Self::cell_of(x, y);
        let a = self.texel(cx, cy)[0];
        let b = self.texel(cx + 1, cy)[0];
        let d = self.texel(cx, cy + 1)[0];
        let e = self.texel(cx + 1, cy + 1)[0];
        let nearest = a.max(b).max(d.max(e));
        let mix = |p: f32, q: f32, t: f32| p * (1.0 - t) + q * t;
        if nearest < -NEAR {
            return mix(mix(a, b, fx), mix(d, e, fx), fy);
        }
        let (wx, wy) = (catmull(fx), catmull(fy));
        let mut s = 0.0;
        for (i, wxi) in wx.iter().enumerate() {
            let mut column = 0.0;
            for (j, wyj) in wy.iter().enumerate() {
                // LINT-OK: a stencil index, 0..4.
                #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
                let (di, dj) = (i as i32 - 1, j as i32 - 1);
                column += wyj * self.texel(cx + di, cy + dj)[0];
            }
            s += wxi * column;
        }
        s + wander(x, y, foot)
    }

    /// The water's level at world `(x, y)`: each vertex's level (its height, less how far below it
    /// the water lies) across retail's triangles, as retail draws its surface.
    #[must_use]
    pub fn level(&self, x: f32, y: f32) -> f32 {
        let ((cx, cy), (fx, fy)) = Self::cell_of(x, y);
        let at = |gx: i32, gy: i32| self.height(gx, gy) - self.texel(gx, gy)[1];
        let (h00, h10, h01, h11) = (
            at(cx, cy),
            at(cx + 1, cy),
            at(cx, cy + 1),
            at(cx + 1, cy + 1),
        );
        if crate::land::mesh::sw_to_ne_cut(cx, cy) {
            if fx >= fy {
                return h00 + fx * (h10 - h00) + fy * (h11 - h10);
            }
            return h00 + fy * (h01 - h00) + fx * (h11 - h01);
        }
        if fx + fy <= 1.0 {
            return h00 + fx * (h10 - h00) + fy * (h01 - h00);
        }
        h11 + (1.0 - fx) * (h01 - h11) + (1.0 - fy) * (h10 - h11)
    }
}

#[cfg(test)]
mod tests;
