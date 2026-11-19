//! A world map's picture drawn on the CPU from a world's land: the land lit by its relief, a window
//! of the picture or the whole shrunk, the drawing shared among the machine's threads, and a build
//! that runs off the frame.
//!
//! A game's map says how a row of its picture is drawn ([`Rows`]); this crate cuts the picture
//! into windows and tiles ([`window`]) and shrinks it to an overview ([`shrink`], or a band at a
//! time with [`shrink_rows`]), both shared among the machine's threads ([`in_bands`]). The land's
//! light comes of its slope ([`relief_light`], [`lit`]): lit from the north-west, steeper than it
//! is so the relief reads at a map's scale. A map is built in steps ([`build::Steps`]), on a thread
//! of its own or, where there are no threads, a share of each frame ([`build::OffFrame`]).
//!
//! **Depends on** nothing of either game. **Used by** both games' world maps: Dereth's scene
//! (`dereth-scene`) and Auberean's Horizon (`auberean-horizon`), which each read their own world's
//! land and say how its picture is coloured.
//!
//! **Must never** name either game, read a file or know about a device: what it draws is the
//! caller's land, and what it gives back is pixels.

#![forbid(unsafe_code)]

use std::sync::{Mutex, PoisonError};

pub mod build;

/// Pixels of a picture, four bytes to a pixel in blue, green, red, alpha order, row by row from
/// the top.
#[derive(Clone, PartialEq, Eq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pixels({}x{})", self.width, self.height)
    }
}

impl Pixels {
    /// `bgra` as a `width`×`height` picture.
    #[must_use]
    pub fn new((width, height): (usize, usize), bgra: Vec<u8>) -> Self {
        Self {
            width: u32::try_from(width).unwrap_or(0),
            height: u32::try_from(height).unwrap_or(0),
            bgra,
        }
    }
}

/// A picture drawn a row at a time.
pub trait Rows: Sync {
    /// Its width and height, in its pixels.
    fn size(&self) -> (usize, usize);

    /// Row `row` (from the top), from column `first` on, into `out` at four bytes a pixel in blue,
    /// green, red, alpha order: as many pixels as `out` holds.
    fn draw_row(&self, row: usize, first: usize, out: &mut [u8]);
}

/// The `w`×`h` window of `picture` at pixel `(x0, y0)`, cut short at its east and south edges.
#[must_use]
pub fn window<R: Rows + ?Sized>(
    picture: &R,
    (x0, y0): (usize, usize),
    (w, h): (usize, usize),
) -> Pixels {
    let (width, height) = picture.size();
    let (w, h) = (w.min(width - x0.min(width)), h.min(height - y0.min(height)));
    let mut bgra = vec![0u8; w * h * 4];
    in_bands(&mut bgra, w * 4, 64, &|row, band| {
        for (r, out) in band.chunks_mut(w * 4).enumerate() {
            picture.draw_row(y0 + row + r, x0, out);
        }
    });
    Pixels::new((w, h), bgra)
}

/// For each of `to` pixels shrunk from `from`, the source pixels it covers and by how much.
#[must_use]
pub fn spans(from: usize, to: usize) -> Vec<Vec<(usize, f64)>> {
    #[allow(clippy::cast_precision_loss)] // LINT-OK: sides of a few thousand pixels.
    let scale = from as f64 / to as f64;
    (0..to)
        .map(|o| {
            #[allow(clippy::cast_precision_loss)] // LINT-OK: as above.
            let (a, b) = (o as f64 * scale, (o + 1) as f64 * scale);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: within 0..=from.
            let (first, last) = (a.floor() as usize, (b.ceil() as usize).min(from));
            #[allow(clippy::cast_precision_loss)] // LINT-OK: as above.
            (first..last)
                .map(|s| (s, (b.min((s + 1) as f64) - a.max(s as f64)) / scale))
                .collect()
        })
        .collect()
}

/// Rows from `first` of `picture` shrunk, each drawn from the picture's own rows it covers, into
/// `out`: `across` and `down` are the shrink's [`spans`] along each axis.
pub fn shrink_rows<R: Rows + ?Sized>(
    picture: &R,
    across: &[Vec<(usize, f64)>],
    down: &[Vec<(usize, f64)>],
    first: usize,
    out: &mut [u8],
) {
    let width = across.len();
    let mut src = vec![0u8; picture.size().0 * 4];
    let mut acc = vec![0.0f64; width * 3];
    for (o, row) in out.chunks_mut(width * 4).enumerate() {
        acc.fill(0.0);
        for &(s, cover) in &down[first + o] {
            picture.draw_row(s, 0, &mut src);
            for (x, span) in across.iter().enumerate() {
                for c in 0..3 {
                    let mut sum = 0.0f64;
                    for &(si, part) in span {
                        sum += f64::from(src[si * 4 + 2 - c]) * part;
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    // LINT-OK: kept to a single's precision, as the two-pass shrink keeps it.
                    let single = sum as f32;
                    acc[x * 3 + c] += f64::from(single) * cover;
                }
            }
        }
        for (px, a) in row
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(acc.as_chunks::<3>().0)
        {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: a colour channel, clamped to its range.
            let byte = |c: f64| c.round().clamp(0.0, 255.0) as u8;
            *px = [byte(a[2]), byte(a[1]), byte(a[0]), 255];
        }
    }
}

/// The whole of `picture` shrunk to `to`, in bands of sixteen rows shared among the machine's
/// threads.
#[must_use]
pub fn shrink<R: Rows + ?Sized>(picture: &R, to: (usize, usize)) -> Pixels {
    let (width, height) = picture.size();
    let (across, down) = (spans(width, to.0), spans(height, to.1));
    let mut bgra = vec![0u8; to.0 * to.1 * 4];
    in_bands(&mut bgra, to.0 * 4, 16, &|row, band| {
        shrink_rows(picture, &across, &down, row, band);
    });
    Pixels::new(to, bgra)
}

/// Fill `out`, rows of `row_len` bytes, by `rows(first_row, band)` over bands of `band` rows,
/// shared among the machine's threads (on the calling thread alone where it has none to share).
pub fn in_bands(
    out: &mut [u8],
    row_len: usize,
    band: usize,
    rows: &(dyn Fn(usize, &mut [u8]) + Sync),
) {
    #[cfg(target_arch = "wasm32")]
    let threads = 1;
    #[cfg(not(target_arch = "wasm32"))]
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let len = row_len * band;
    let bands = out.len().div_ceil(len.max(1));
    if threads <= 1 || bands <= 1 {
        for (b, chunk) in out.chunks_mut(len.max(1)).enumerate() {
            rows(b * band, chunk);
        }
        return;
    }
    let queue = Mutex::new(out.chunks_mut(len).enumerate());
    std::thread::scope(|scope| {
        for _ in 0..threads.min(bands) {
            scope.spawn(|| loop {
                let next = queue.lock().unwrap_or_else(PoisonError::into_inner).next();
                let Some((b, chunk)) = next else { break };
                rows(b * band, chunk);
            });
        }
    });
}

/// The way the map's light comes from: the north-west, half way up the sky, as a unit vector (x
/// east, y north, z up).
fn light() -> (f32, f32, f32) {
    let (x, y, z) = (-0.5f32, 0.5f32, std::f32::consts::FRAC_1_SQRT_2);
    let n = (x * x + y * y + z * z).sqrt();
    (x / n, y / n, z / n)
}

/// The light on ground rising `dzdx` to the east and `dzdy` to the north, drawn `steepen` times
/// steeper than it is: 1 on the flat, more on a slope facing the north-west and less on one facing
/// away, from 0.45 to 1.45.
#[must_use]
pub fn lit(dzdx: f32, dzdy: f32, steepen: f32) -> f32 {
    lit_from(light(), dzdx, dzdy, steepen)
}

fn lit_from(light: (f32, f32, f32), dzdx: f32, dzdy: f32, steepen: f32) -> f32 {
    let (nx, ny, nz) = (-steepen * dzdx, -steepen * dzdy, 1.0f32);
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    let facing = (nx * light.0 + ny * light.1 + nz * light.2) / len / light.2;
    (0.3 + 0.7 * facing).clamp(0.45, 1.45)
}

/// Each vertex's light ([`lit`]) over a grid of `size` vertices, x east then y north, `spacing`
/// apart, of heights `height(x, y)`: its slope taken across its neighbors on either side (one side
/// at the grid's edge).
#[must_use]
pub fn relief_light(
    (width, height): (usize, usize),
    spacing: f32,
    steepen: f32,
    height_at: impl Fn(usize, usize) -> f32,
) -> Vec<f32> {
    let light = light();
    let (last_x, last_y) = (width.saturating_sub(1), height.saturating_sub(1));
    let mut out = vec![1.0; width * height];
    for (y, row) in out.chunks_mut(width.max(1)).enumerate() {
        for (x, lit) in row.iter_mut().enumerate() {
            let (xl, xr) = (x.saturating_sub(1), (x + 1).min(last_x));
            let (yd, yu) = (y.saturating_sub(1), (y + 1).min(last_y));
            #[allow(clippy::cast_precision_loss)] // LINT-OK: one or two vertex steps.
            let dzdx = (height_at(xr, y) - height_at(xl, y)) / ((xr - xl).max(1) as f32 * spacing);
            #[allow(clippy::cast_precision_loss)] // LINT-OK: one or two vertex steps.
            let dzdy = (height_at(x, yu) - height_at(x, yd)) / ((yu - yd).max(1) as f32 * spacing);
            *lit = lit_from(light, dzdx, dzdy, steepen);
        }
    }
    out
}

/// Each grid vertex's distance, in vertex steps, from the nearest vertex `source` holds (two
/// chamfer passes, a diagonal step √2), at most `cap`; `cap` everywhere where none is.
#[must_use]
pub fn distance_from(
    (width, height): (usize, usize),
    cap: f32,
    source: impl Fn(usize) -> bool,
) -> Vec<f32> {
    let mut d: Vec<f32> = (0..width * height)
        .map(|i| if source(i) { 0.0 } else { f32::MAX })
        .collect();
    let diagonal = std::f32::consts::SQRT_2;
    let (last_x, last_y) = (width.saturating_sub(1), height.saturating_sub(1));
    let at = |x: usize, y: usize| y * width + x;
    for y in 0..height {
        for x in 0..width {
            let mut best = d[at(x, y)];
            if x > 0 {
                best = best.min(d[at(x - 1, y)] + 1.0);
            }
            if y > 0 {
                best = best.min(d[at(x, y - 1)] + 1.0);
                if x > 0 {
                    best = best.min(d[at(x - 1, y - 1)] + diagonal);
                }
                if x < last_x {
                    best = best.min(d[at(x + 1, y - 1)] + diagonal);
                }
            }
            d[at(x, y)] = best;
        }
    }
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let mut best = d[at(x, y)];
            if x < last_x {
                best = best.min(d[at(x + 1, y)] + 1.0);
            }
            if y < last_y {
                best = best.min(d[at(x, y + 1)] + 1.0);
                if x < last_x {
                    best = best.min(d[at(x + 1, y + 1)] + diagonal);
                }
                if x > 0 {
                    best = best.min(d[at(x - 1, y + 1)] + diagonal);
                }
            }
            d[at(x, y)] = best;
        }
    }
    for v in &mut d {
        *v = v.min(cap);
    }
    d
}

/// The smooth step from 0 at `e0` to 1 at `e1`.
#[must_use]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `a` mixed toward `b` by `t`.
#[must_use]
pub fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

#[cfg(test)]
mod tests;
