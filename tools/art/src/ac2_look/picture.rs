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

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 look's pictures)
    use super::*;

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
