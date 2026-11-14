//! What the look does to a picture: a ground's tint toward its group's coloring, the relief a
//! ground is given (a height from its picture's fine shading, and a normal from that height), and
//! writing a picture as PNG.

use super::source::Rgba;

/// The picture with each texel's color times `tint` (red, green, blue), rounded and held to 255.
#[must_use]
pub fn tinted(p: &Rgba, tint: [f64; 3]) -> Rgba {
    let mut texels = p.texels.clone();
    for t in texels.as_chunks_mut::<4>().0 {
        for c in 0..3 {
            t[c] = byte((f64::from(t[c]) * tint[c] + 0.5).min(255.0));
        }
    }
    Rgba {
        texels,
        ..p.clone()
    }
}

/// A number from 0 to 255 as a byte, its fraction dropped.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// LINT-OK: every caller holds the number to a byte's range first.
const fn byte(v: f64) -> u8 {
    v as u8
}

/// A color premultiplied by its alpha, rounded.
fn premultiplied(c: u8, a: u8) -> u8 {
    u8::try_from((u32::from(c) * u32::from(a) + 127) / 255).unwrap_or(u8::MAX)
}

/// A box blur of radius `r` across and then down, both wrapping round the edges, as a running sum.
fn blur(s: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let wrap = |k: isize, n: usize| {
        let n = isize::try_from(n).unwrap_or(isize::MAX);
        usize::try_from(k.rem_euclid(n)).unwrap_or(0)
    };
    #[allow(clippy::cast_precision_loss)] // LINT-OK: a blur's few texels across.
    let span = (2 * r + 1) as f32;
    let ri = isize::try_from(r).unwrap_or(0);
    let mut t = s.to_vec();
    for y in 0..h {
        let mut acc: f32 = 0.0;
        for k in -ri..=ri {
            acc += s[y * w + wrap(k, w)];
        }
        for x in 0..w {
            t[y * w + x] = acc / span;
            let xi = isize::try_from(x).unwrap_or(0);
            acc += s[y * w + wrap(xi + ri + 1, w)] - s[y * w + wrap(xi - ri, w)];
        }
    }
    let mut o = t.clone();
    for x in 0..w {
        let mut acc: f32 = 0.0;
        for k in -ri..=ri {
            acc += t[wrap(k, h) * w + x];
        }
        for y in 0..h {
            o[y * w + x] = acc / span;
            let yi = isize::try_from(y).unwrap_or(0);
            acc += t[wrap(yi + ri + 1, h) * w + x] - t[wrap(yi - ri, h) * w + x];
        }
    }
    o
}

/// A ground picture's relief, both tiling: its height (the picture's fine luminance, its colors
/// premultiplied by their alpha, less its broad shading, spread over 0 to 255) and a tangent-space normal from that height, bent by
/// `strength`. Each is returned as a picture: `(height, normal)`.
#[must_use]
pub fn relief(p: &Rgba, strength: f32) -> (Rgba, Rgba) {
    let (w, h) = (p.width as usize, p.height as usize);
    let lum: Vec<f32> = p
        .texels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|t| {
            // Each color as drawn over nothing: premultiplied by its alpha.
            let c = |k: usize| f32::from(premultiplied(t[k], t[3]));
            (0.2126 * c(0) + 0.7152 * c(1) + 0.0722 * c(2)) / 255.0
        })
        .collect();
    let fine = blur(&lum, w, h, 1);
    let broad = blur(&lum, w, h, (w / 32).max(4));
    let mut hgt: Vec<f32> = fine.iter().zip(&broad).map(|(f, b)| f - b).collect();
    let (mut lo, mut hi) = (9.0f32, -9.0f32);
    for v in &hgt {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    for v in &mut hgt {
        *v = (*v - lo) / (hi - lo).max(1e-4);
    }
    let mut hp = vec![255u8; w * h * 4];
    let mut np = vec![255u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let v = byte(f64::from((hgt[i] * 255.0).clamp(0.0, 255.0)));
            hp[4 * i..4 * i + 3].copy_from_slice(&[v, v, v]);
            let dx = (hgt[y * w + (x + 1) % w] - hgt[y * w + (x + w - 1) % w]) * strength;
            let dy = (hgt[((y + 1) % h) * w + x] - hgt[((y + h - 1) % h) * w + x]) * strength;
            let l = (dx * dx + dy * dy + 1.0).sqrt();
            np[4 * i] = byte(f64::from((-dx / l * 0.5 + 0.5) * 255.0));
            np[4 * i + 1] = byte(f64::from((dy / l * 0.5 + 0.5) * 255.0));
            np[4 * i + 2] = byte(f64::from((1.0 / l * 0.5 + 0.5) * 255.0));
        }
    }
    let of = |texels| Rgba {
        width: p.width,
        height: p.height,
        texels,
    };
    (of(hp), of(np))
}

/// The picture as a PNG file's bytes.
///
/// # Errors
/// The encoder fails.
pub fn png(p: &Rgba) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut bytes, p.width, p.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(&p.texels).map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

/// How much each of red, green and blue counts toward a texel's lightness.
const LUM: [f32; 3] = [0.299, 0.587, 0.114];

/// A number held to a byte's range, its fraction dropped.
fn byte_of(v: f32) -> u8 {
    byte(f64::from(v.clamp(0.0, 255.0)))
}

/// The picture with every texel opaque.
#[must_use]
pub fn opaque(p: &Rgba) -> Rgba {
    let mut out = p.clone();
    for t in out.texels.as_chunks_mut::<4>().0 {
        t[3] = 255;
    }
    out
}

/// The bicubic weight of a texel `x` texels from a sample's center.
fn cubic(x: f64) -> f64 {
    const A: f64 = -0.5;
    let x = x.abs();
    if x < 1.0 {
        ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0
    } else if x < 2.0 {
        (((x - 5.0) * x + 8.0) * x - 4.0) * A
    } else {
        0.0
    }
}

/// Each output texel's first input texel and weights, for `from` texels drawn as `to`: a bicubic
/// filter widened by however much the picture shrinks, so every input texel counts.
fn resample_weights(from: usize, to: usize) -> Vec<(usize, Vec<f64>)> {
    let scale = from as f64 / to as f64;
    let filter = scale.max(1.0);
    let support = 2.0 * filter;
    (0..to)
        .map(|x| {
            let center = (x as f64 + 0.5) * scale;
            // LINT-OK: both ends are held within the picture.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (lo, hi) = (
                (center - support + 0.5).floor().max(0.0) as usize,
                ((center + support + 0.5).floor().max(0.0) as usize).min(from),
            );
            let mut w: Vec<f64> = (lo..hi)
                .map(|j| cubic((j as f64 - center + 0.5) / filter))
                .collect();
            let sum: f64 = w.iter().sum();
            if sum != 0.0 {
                for v in &mut w {
                    *v /= sum;
                }
            }
            (lo, w)
        })
        .collect()
}

/// The picture drawn `width` by `height` texels: bicubic, across and then down, each pass rounded
/// to bytes.
#[must_use]
pub fn resized(p: &Rgba, width: u32, height: u32) -> Rgba {
    if (p.width, p.height) == (width, height) {
        return p.clone();
    }
    let (w0, h0) = (p.width as usize, p.height as usize);
    let (w1, h1) = (width as usize, height as usize);
    let across = resample_weights(w0, w1);
    let mut mid = vec![0u8; w1 * h0 * 4];
    for y in 0..h0 {
        for (x, (lo, w)) in across.iter().enumerate() {
            for c in 0..4 {
                let v: f64 = w
                    .iter()
                    .enumerate()
                    .map(|(k, wk)| wk * f64::from(p.texels[((y * w0) + lo + k) * 4 + c]))
                    .sum();
                mid[(y * w1 + x) * 4 + c] = byte(v.round().clamp(0.0, 255.0));
            }
        }
    }
    let down = resample_weights(h0, h1);
    let mut texels = vec![0u8; w1 * h1 * 4];
    for (y, (lo, w)) in down.iter().enumerate() {
        for x in 0..w1 {
            for c in 0..4 {
                let v: f64 = w
                    .iter()
                    .enumerate()
                    .map(|(k, wk)| wk * f64::from(mid[((lo + k) * w1 + x) * 4 + c]))
                    .sum();
                texels[(y * w1 + x) * 4 + c] = byte(v.round().clamp(0.0, 255.0));
            }
        }
    }
    Rgba {
        width,
        height,
        texels,
    }
}

/// The mean color of the picture's texels that show (alpha over 8), and its lightness.
#[must_use]
pub fn mean_color(p: &Rgba) -> ([f32; 3], f32) {
    let shown: Vec<&[u8; 4]> = p
        .texels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|t| t[3] > 8)
        .collect();
    let all: Vec<&[u8; 4]> = if shown.is_empty() {
        p.texels.as_chunks::<4>().0.iter().collect()
    } else {
        shown
    };
    let n = all.len().max(1) as f32;
    let mut mean = [0f32; 3];
    for t in &all {
        for c in 0..3 {
            mean[c] += f32::from(t[c]);
        }
    }
    let mean = mean.map(|v| v / n);
    let lum = mean[0] * LUM[0] + mean[1] * LUM[1] + mean[2] * LUM[2];
    (mean, lum)
}

/// The picture multiplied by `tint`, where `mask` (one weight a texel, 0 to 1) is white; all over
/// without one.
#[must_use]
pub fn multiplied(p: &Rgba, tint: [f64; 3], mask: Option<&[f32]>) -> Rgba {
    let mut out = p.clone();
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a tint is a small factor.
    let k = tint.map(|v| v as f32);
    for (i, t) in out.texels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let m = mask.map_or(1.0, |m| m[i]);
        for c in 0..3 {
            t[c] = byte_of(f32::from(t[c]) * (1.0 + m * (k[c] - 1.0)));
        }
    }
    out
}

/// Every texel of the picture in one color, each as light or dark as it was against the picture's
/// mean: the color `tint` makes of the mean, or with `like` (another picture's [`mean_color`] times
/// its tint, and its lightness) that color, as much lighter or darker as this picture's mean is.
#[must_use]
pub fn colorized(p: &Rgba, tint: [f64; 3], like: Option<([f32; 3], f32)>) -> Rgba {
    let (mean, mean_light) = mean_color(p);
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a tint is a small factor.
    let want = match like {
        Some((color, light)) => color.map(|c| c * mean_light / light.max(1.0)),
        None => [0, 1, 2].map(|c| mean[c] * tint[c] as f32),
    };
    let lum = |t: &[u8; 4]| {
        f32::from(t[0]) * LUM[0] + f32::from(t[1]) * LUM[1] + f32::from(t[2]) * LUM[2]
    };
    let shown: Vec<f32> = p
        .texels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|t| t[3] > 8)
        .map(lum)
        .collect();
    let all_light = if shown.is_empty() {
        let l: Vec<f32> = p.texels.as_chunks::<4>().0.iter().map(lum).collect();
        l.iter().sum::<f32>() / l.len().max(1) as f32
    } else {
        shown.iter().sum::<f32>() / shown.len() as f32
    };
    let mut out = p.clone();
    for t in out.texels.as_chunks_mut::<4>().0 {
        let l = lum(t) / all_light.max(1.0);
        for c in 0..3 {
            t[c] = byte_of(l * want[c]);
        }
    }
    out
}

/// The mean color `tint` makes of the picture and the picture's lightness: what [`colorized`]
/// gives a body's other pictures (`like`) to take this one's color.
#[must_use]
pub fn colored(p: &Rgba, tint: [f64; 3]) -> ([f32; 3], f32) {
    let (mean, light) = mean_color(p);
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a tint is a small factor.
    let color = [0, 1, 2].map(|c| mean[c] * tint[c] as f32);
    (color, light)
}

/// A blur of each row of `s` (`w` wide) by a box `r` texels each side, a fraction of the next texel
/// counted at each end, the edges held; rounded to bytes.
fn box_rows(s: &[f32], w: usize, h: usize, r: f32) -> Vec<f32> {
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: the radius is small and positive.
    let whole = r.floor() as isize;
    let part = r - r.floor();
    let mut out = vec![0f32; s.len()];
    for y in 0..h {
        let row = &s[y * w..(y + 1) * w];
        // LINT-OK: an index held within the row.
        let at = |x: isize| row[x.clamp(0, w as isize - 1) as usize];
        for x in 0..w {
            let x = x as isize;
            let mut sum: f32 = (-whole..=whole).map(|k| at(x + k)).sum();
            sum += part * (at(x - whole - 1) + at(x + whole + 1));
            out[y * w + x as usize] = (sum / (2.0 * r + 1.0)).round();
        }
    }
    out
}

/// `s` (`w` by `h`) turned over its diagonal.
fn transposed(s: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0f32; s.len()];
    for y in 0..h {
        for x in 0..w {
            out[x * h + y] = s[y * w + x];
        }
    }
    out
}

/// A Gaussian blur of `radius`, as three box blurs each way.
fn gaussian(s: &[f32], w: usize, h: usize, radius: f32) -> Vec<f32> {
    const PASSES: f32 = 3.0;
    let sigma2 = radius * radius / PASSES;
    let l = ((12.0 * sigma2 + 1.0).sqrt() - 1.0) / 2.0;
    let l = l.floor();
    let a =
        (2.0 * l + 1.0) * (l * (l + 1.0) - 3.0 * sigma2) / (6.0 * (sigma2 - (l + 1.0) * (l + 1.0)));
    let r = l + a;
    let mut v = s.to_vec();
    for _ in 0..3 {
        v = box_rows(&v, w, h, r);
    }
    let mut t = transposed(&v, w, h);
    for _ in 0..3 {
        t = box_rows(&t, h, w, r);
    }
    transposed(&t, h, w)
}

/// Where pictures of one body differ from each other: a weight a texel, 1 where some picture's
/// color stands 50 or more from their median, 0 where none stands more than 20, blurred a little.
/// Every picture is drawn at the first's size; the weights are at that size.
#[must_use]
pub fn differences(pictures: &[Rgba]) -> (u32, u32, Vec<f32>) {
    let Some(first) = pictures.first() else {
        return (0, 0, Vec::new());
    };
    let (w, h) = (first.width, first.height);
    let drawn: Vec<Rgba> = pictures.iter().map(|p| resized(&opaque(p), w, h)).collect();
    let n = (w * h) as usize;
    let mut weights = vec![0f32; n];
    let mut column = Vec::with_capacity(drawn.len());
    for (i, weight) in weights.iter_mut().enumerate() {
        let mut most = 0f32;
        for c in 0..3 {
            column.clear();
            column.extend(drawn.iter().map(|p| f32::from(p.texels[i * 4 + c])));
            column.sort_by(f32::total_cmp);
            let k = column.len();
            let median = if k % 2 == 1 {
                column[k / 2]
            } else {
                (column[k / 2 - 1] + column[k / 2]) / 2.0
            };
            for v in &column {
                most = most.max((v - median).abs());
            }
        }
        *weight = f32::from(byte_of(((most - 20.0) / 30.0).clamp(0.0, 1.0) * 255.0));
    }
    let blurred = gaussian(&weights, w as usize, h as usize, 1.5);
    (
        w,
        h,
        blurred
            .iter()
            .map(|v| v.clamp(0.0, 255.0) / 255.0)
            .collect(),
    )
}

/// `weights` (`w` by `h`) drawn at `width` by `height`, as a gray picture is.
#[must_use]
pub fn weights_at(w: u32, h: u32, weights: &[f32], width: u32, height: u32) -> Vec<f32> {
    let gray = Rgba {
        width: w,
        height: h,
        texels: weights
            .iter()
            .flat_map(|v| {
                let b = byte_of((v * 255.0).round());
                [b, b, b, 255]
            })
            .collect(),
    };
    let drawn = resized(&gray, width, height);
    drawn
        .texels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|t| f32::from(t[0]) / 255.0)
        .collect()
}

/// The picture with `detail` laid over it: the detail drawn a quarter of its size, repeated four
/// times each way, each texel times it over 128.
#[must_use]
pub fn detailed(p: &Rgba, detail: &Rgba) -> Rgba {
    const TILES: u32 = 4;
    let (w, h) = (p.width, p.height);
    let d = resized(&opaque(detail), (w / TILES).max(1), (h / TILES).max(1));
    let mut out = p.clone();
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x % d.width, y % d.height);
            let i = ((y * w + x) * 4) as usize;
            let j = ((dy * d.width + dx) * 4) as usize;
            for c in 0..3 {
                out.texels[i + c] =
                    byte_of(f32::from(p.texels[i + c]) * f32::from(d.texels[j + c]) / 128.0);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 look's pictures)
    use super::*;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Rgba {
        Rgba {
            width: w,
            height: h,
            texels: c.repeat((w * h) as usize),
        }
    }

    #[test]
    fn a_creature_picture_resized_keeps_its_color_and_takes_the_size_asked() {
        let p = solid(8, 4, [200, 100, 50, 255]);
        let r = resized(&p, 3, 6);
        assert_eq!((r.width, r.height), (3, 6));
        assert!(r
            .texels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|t| *t == [200, 100, 50, 255]));
        assert_eq!(resized(&p, 8, 4), p);
    }

    #[test]
    fn a_tint_is_laid_only_where_the_mask_is_white_and_dropped_to_a_whole_byte() {
        let p = solid(2, 1, [100, 100, 100, 255]);
        let all = multiplied(&p, [1.5, 0.5, 3.0], None);
        assert_eq!(&all.texels[..4], &[150, 50, 255, 255]);
        let half = multiplied(&p, [1.5, 0.5, 1.0], Some(&[0.0, 0.5]));
        assert_eq!(&half.texels[..3], &[100, 100, 100]);
        assert_eq!(&half.texels[4..7], &[125, 75, 100]);
    }

    #[test]
    fn a_colorized_picture_takes_one_color_each_texel_as_light_as_it_was() {
        let p = Rgba {
            width: 2,
            height: 1,
            texels: vec![50, 50, 50, 255, 150, 150, 150, 255],
        };
        let c = colorized(&p, [2.0, 1.0, 0.5], None);
        // The mean is 100 gray; its color times the tint is (200, 100, 50), the darker texel half
        // of it and the lighter one and a half times.
        assert_eq!(&c.texels[..3], &[100, 50, 25]);
        assert_eq!(&c.texels[4..7], &[255, 150, 75]);
        let (color, light) = colored(&p, [2.0, 1.0, 0.5]);
        assert_eq!(color, [200.0, 100.0, 50.0]);
        assert!((light - 100.0).abs() < 1e-3);
    }

    #[test]
    fn pictures_that_agree_have_no_differences_and_one_that_stands_apart_has_them() {
        let a = solid(6, 6, [80, 80, 80, 255]);
        let (w, h, same) = differences(&[a.clone(), a.clone(), a.clone()]);
        assert_eq!((w, h), (6, 6));
        assert!(same.iter().all(|v| *v == 0.0));
        let b = solid(6, 6, [200, 80, 80, 255]);
        let (_, _, apart) = differences(&[a.clone(), a, b]);
        assert!(apart.iter().all(|v| *v > 0.99), "{apart:?}");
    }

    #[test]
    fn a_detail_picture_over_128_darkens_and_over_it_lightens() {
        let p = solid(8, 8, [100, 100, 100, 255]);
        let dark = detailed(&p, &solid(4, 4, [64, 64, 64, 255]));
        assert!(dark
            .texels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|t| t[..3] == [50, 50, 50]));
        let light = detailed(&p, &solid(4, 4, [192, 192, 192, 255]));
        assert!(light
            .texels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|t| t[..3] == [150, 150, 150]));
    }

    fn flat(w: u32, h: u32, f: impl Fn(u32, u32) -> u8) -> Rgba {
        Rgba {
            width: w,
            height: h,
            texels: (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .flat_map(|(x, y)| {
                    let v = f(x, y);
                    [v, v, v, 255]
                })
                .collect(),
        }
    }

    #[test]
    fn a_tint_scales_each_color_and_holds_it_to_white() {
        let p = Rgba {
            width: 1,
            height: 1,
            texels: vec![100, 200, 50, 7],
        };
        assert_eq!(tinted(&p, [0.5, 2.0, 1.01]).texels, [50, 255, 51, 7]);
    }

    #[test]
    fn an_even_picture_has_no_relief_and_a_ridge_bends_the_normal_away() {
        let (height, normal) = relief(&flat(16, 16, |_, _| 90), 6.0);
        assert!(height.texels.chunks(4).all(|t| t[0] == 0));
        assert!(normal.texels.chunks(4).all(|t| t[..3] == [127, 127, 255]));
        let (height, normal) = relief(&flat(16, 16, |x, _| if x == 8 { 255 } else { 0 }), 6.0);
        let at = |p: &Rgba, x: usize| p.texels[4 * x];
        assert_eq!(at(&height, 8), 255);
        assert!(at(&normal, 7) < 127 && at(&normal, 9) > 127);
    }

    #[test]
    fn a_clear_picture_has_no_relief_whatever_its_colors() {
        let mut p = flat(16, 16, |x, _| if x == 8 { 255 } else { 0 });
        for t in p.texels.as_chunks_mut::<4>().0 {
            t[3] = 0;
        }
        let (height, normal) = relief(&p, 6.0);
        assert!(height.texels.chunks(4).all(|t| t[0] == 0));
        assert!(normal.texels.chunks(4).all(|t| t[..3] == [127, 127, 255]));
        assert_eq!(premultiplied(200, 128), 100);
    }

    #[test]
    fn a_picture_writes_as_a_png_that_reads_back() {
        let p = flat(4, 2, |x, y| u8::try_from(x * 10 + y).expect("small"));
        let bytes = png(&p).expect("encodes");
        let mut r = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .expect("a header");
        let mut buf = vec![0; r.output_buffer_size()];
        r.next_frame(&mut buf).expect("a frame");
        assert_eq!(buf, p.texels);
    }
}
