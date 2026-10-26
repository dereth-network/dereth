//! A picture drawn larger or smaller than its own pixels, made at the size it is drawn: part of a
//! picture resampled to a whole number of pixels with the Lanczos filter the interface's art was
//! cut with, so it can be drawn texel for texel rather than stretched by the device's two-texel
//! filter, which skips texels when it shrinks a picture and smears them when it grows one.
//!
//! The colours are filtered with their alpha multiplied in, so a transparent texel lends no
//! colour to its neighbours, and nothing outside the part asked for is read: a piece cut from an
//! atlas never takes in its neighbour.

use crate::art::Image;
use crate::draw::Rect;

/// How many texels the filter reaches either side of a sample, at one texel to one pixel.
const LOBES: f32 = 3.0;

/// The Lanczos window of three lobes.
fn lanczos(x: f32) -> f32 {
    let x = x.abs();
    if x < 1e-6 {
        return 1.0;
    }
    if x >= LOBES {
        return 0.0;
    }
    let px = std::f32::consts::PI * x;
    let sin = dereth_primitives::num::math::sinf;
    LOBES * sin(px) * sin(px / LOBES) / (px * px)
}

/// The weights of one axis: each sample reads `taps` texels from its first (counted from the
/// first texel read), with these weights, zero past its own reach.
struct Axis {
    taps: usize,
    first: Vec<usize>,
    weight: Vec<f32>,
}

/// The weights for `out` samples along an axis, from the span `start..start + len` of the source,
/// read no further than `lo..hi`.
fn weights(start: f32, len: f32, out: u32, lo: i32, hi: i32) -> Axis {
    #[allow(clippy::cast_precision_loss)]
    let step = len / out.max(1) as f32;
    let spread = step.max(1.0);
    let reach = LOBES * spread;
    let n = usize::try_from(hi - lo).unwrap_or(1).max(1);
    let taps = (dereth_primitives::num::to_i32((2.0 * reach).ceil()) + 1)
        .clamp(1, i32::try_from(n).unwrap_or(i32::MAX));
    let taps = usize::try_from(taps).unwrap_or(1);
    let mut first = Vec::with_capacity(out as usize);
    let mut weight = vec![0.0_f32; out as usize * taps];
    for i in 0..out as usize {
        #[allow(clippy::cast_precision_loss)]
        let centre = start + (i as f32 + 0.5) * step;
        let from = dereth_primitives::num::to_i32((centre - reach).floor()).max(lo);
        let to = dereth_primitives::num::to_i32((centre + reach).ceil())
            .min(hi)
            .max(from + 1);
        // Moved back where the window would run past the last texel read.
        let at = usize::try_from(from - lo).unwrap_or(0).min(n - taps);
        let w = &mut weight[i * taps..(i + 1) * taps];
        for t in from..to {
            let slot = usize::try_from(t - lo).unwrap_or(0).saturating_sub(at);
            if slot < taps {
                #[allow(clippy::cast_precision_loss)]
                let v = lanczos((t as f32 + 0.5 - centre) / spread);
                w[slot] += v;
            }
        }
        let sum: f32 = w.iter().sum();
        if sum.abs() > 1e-6 {
            for v in w.iter_mut() {
                *v /= sum;
            }
        }
        first.push(at);
    }
    Axis {
        taps,
        first,
        weight,
    }
}

/// The texels of a picture `size` texels long that a span from `start`, `len` long, covers:
/// `lo..hi`, at least one.
fn texels(start: f32, len: f32, size: u32) -> (i32, i32) {
    let size = i32::try_from(size).unwrap_or(i32::MAX).max(1);
    let lo = dereth_primitives::num::to_i32((start + 1e-3).floor()).clamp(0, size - 1);
    let hi = dereth_primitives::num::to_i32((start + len - 1e-3).ceil()).clamp(lo + 1, size);
    (lo, hi)
}

/// Part `src` of `image` (in its texels) made `out` pixels across and down.
#[must_use]
pub fn resample(image: &Image, src: Rect, out: (u32, u32)) -> Image {
    let (ow, oh) = (out.0.max(1), out.1.max(1));
    let (x_lo, x_hi) = texels(src.x, src.w, image.width);
    let (y_lo, y_hi) = texels(src.y, src.h, image.height);
    let across = weights(src.x, src.w, ow, x_lo, x_hi);
    let down = weights(src.y, src.h, oh, y_lo, y_hi);
    let (cols, rows) = (
        usize::try_from(x_hi - x_lo).unwrap_or(0),
        usize::try_from(y_hi - y_lo).unwrap_or(0),
    );
    let (x0, y0, stride) = (
        usize::try_from(x_lo).unwrap_or(0),
        usize::try_from(y_lo).unwrap_or(0),
        image.width as usize,
    );
    // The part read, its colours with their alpha multiplied in.
    let mut part = Vec::with_capacity(cols * rows);
    for y in y0..y0 + rows {
        let row = &image.bgra[(y * stride + x0) * 4..(y * stride + x0 + cols) * 4];
        for p in row.as_chunks::<4>().0 {
            let a = f32::from(p[3]) / 255.0;
            let k = a / 255.0;
            part.push([
                f32::from(p[0]) * k,
                f32::from(p[1]) * k,
                f32::from(p[2]) * k,
                a,
            ]);
        }
    }
    // Across first, every row the part reads; then down.
    let width = ow as usize;
    let mut wide = vec![[0.0_f32; 4]; rows * width];
    let taps = across.taps;
    for (row, line) in part.chunks_exact(cols.max(1)).enumerate() {
        let into = &mut wide[row * width..(row + 1) * width];
        for ((first, w), sum) in across
            .first
            .iter()
            .zip(across.weight.chunks_exact(taps))
            .zip(into.iter_mut())
        {
            let mut s = [0.0_f32; 4];
            for (p, wt) in line[*first..*first + taps].iter().zip(w) {
                s[0] += p[0] * wt;
                s[1] += p[1] * wt;
                s[2] += p[2] * wt;
                s[3] += p[3] * wt;
            }
            *sum = s;
        }
    }
    // A channel from 0 to 1 as a byte, to the nearest.
    let byte = |v: f32| -> u8 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let b = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        b
    };
    let mut bgra = vec![0_u8; width * oh as usize * 4];
    let mut acc = vec![[0.0_f32; 4]; width];
    for ((first, w), out_row) in down
        .first
        .iter()
        .zip(down.weight.chunks_exact(down.taps))
        .zip(bgra.chunks_exact_mut(width * 4))
    {
        acc.fill([0.0; 4]);
        for (t, wt) in w.iter().enumerate() {
            if *wt == 0.0 {
                continue;
            }
            let row = &wide[(first + t) * width..(first + t + 1) * width];
            for (sum, p) in acc.iter_mut().zip(row) {
                sum[0] += p[0] * wt;
                sum[1] += p[1] * wt;
                sum[2] += p[2] * wt;
                sum[3] += p[3] * wt;
            }
        }
        for (sum, px) in acc.iter().zip(out_row.as_chunks_mut::<4>().0) {
            let a = sum[3].clamp(0.0, 1.0);
            if a > 1.0 / 512.0 {
                let un = 1.0 / a;
                *px = [
                    byte(sum[0] * un),
                    byte(sum[1] * un),
                    byte(sum[2] * un),
                    byte(a),
                ];
            }
        }
    }
    Image {
        width: ow,
        height: oh,
        bgra,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own widget kit)
    use super::*;

    /// A picture `w` by `h` whose texel at `(x, y)` is `f(x, y)`.
    fn picture(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Image {
        let mut bgra = Vec::new();
        for y in 0..h {
            for x in 0..w {
                bgra.extend_from_slice(&f(x, y));
            }
        }
        Image {
            width: w,
            height: h,
            bgra,
        }
    }

    #[test]
    fn a_part_made_at_its_own_size_is_its_texels() {
        let image = picture(9, 7, |x, y| {
            [
                u8::try_from(x * 20).unwrap(),
                u8::try_from(y * 30).unwrap(),
                77,
                if (x + y) % 3 == 0 { 0 } else { 255 },
            ]
        });
        let part = resample(&image, Rect::new(2.0, 1.0, 5.0, 4.0), (5, 4));
        for y in 0..4 {
            for x in 0..5 {
                let want = image.pixel(x + 2, y + 1);
                let got = part.pixel(x, y);
                if want[3] == 0 {
                    assert_eq!(got[3], 0, "({x}, {y})");
                } else {
                    assert_eq!(got, want, "({x}, {y})");
                }
            }
        }
    }

    /// Halved, a line one texel wide keeps its share of the pixels it falls in, the same wherever
    /// it stands: no texel is skipped.
    #[test]
    fn a_line_one_texel_wide_shrunk_by_half_keeps_its_weight_wherever_it_stands() {
        let sums: Vec<u32> = (4..12)
            .map(|at| {
                let image = picture(16, 4, |x, _| {
                    if x == at {
                        [255, 255, 255, 255]
                    } else {
                        [0, 0, 0, 255]
                    }
                });
                let half = resample(&image, Rect::new(0.0, 0.0, 16.0, 4.0), (8, 2));
                (0..8).map(|x| u32::from(half.pixel(x, 1)[0])).sum()
            })
            .collect();
        for sum in &sums {
            assert!(
                (110..=170).contains(sum),
                "a line sums to {sum} across its row: {sums:?}"
            );
        }
        let (lo, hi) = (sums.iter().min().unwrap(), sums.iter().max().unwrap());
        assert!(
            hi - lo <= 4,
            "the line's weight depends on where it stands: {sums:?}"
        );
    }

    /// The colour of a transparent texel lends nothing to its neighbours, and nothing outside the
    /// part is read.
    #[test]
    fn transparent_texels_and_texels_outside_the_part_lend_no_colour() {
        // A red square in a clear ground, beside a green column outside the part.
        let image = picture(12, 8, |x, y| {
            if x >= 10 {
                [0, 255, 0, 255]
            } else if (3..7).contains(&x) && (2..6).contains(&y) {
                [0, 0, 255, 255]
            } else {
                [255, 255, 255, 0]
            }
        });
        let part = resample(&image, Rect::new(0.0, 0.0, 10.0, 8.0), (7, 5));
        for y in 0..5 {
            for x in 0..7 {
                let [b, g, r, a] = part.pixel(x, y);
                if a > 8 {
                    assert!(
                        r > 200 && g < 40 && b < 40,
                        "({x}, {y}): {:?}",
                        [b, g, r, a]
                    );
                }
            }
        }
    }
}
