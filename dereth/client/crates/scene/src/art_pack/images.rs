//! An art pack's pictures: PNG files read into eight bits a channel, with every smaller level
//! made from the one above, then compressed in blocks ([`compressed`]) where they are drawn from
//! blocks.
//!
//! A colour picture's levels are averaged in linear light, so a level is not darker than the
//! picture it shrinks. A cut-out picture's alpha is scaled at each level so the share of it that
//! passes the cut stays the share the full picture passes: needles and leaves keep their
//! coverage with distance instead of thinning away.

use std::sync::Arc;

pub use dereth_render::block_codec::BlockFormat;

/// A decoded picture with its levels: four bytes a texel, or compressed in blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The largest level's width, texels.
    pub width: u32,
    /// The largest level's height, texels.
    pub height: u32,
    /// Whether it holds colour in the display's encoding, rather than data.
    pub srgb: bool,
    /// Every level, largest first: four bytes a texel, or the level's blocks in [`Self::format`].
    pub levels: Vec<Arc<Vec<u8>>>,
    /// The blocks the levels are compressed in; `None` for four bytes a texel.
    pub format: Option<BlockFormat>,
}

/// `image` with every level compressed to `format`: BC4 keeps red, BC5 red and green, BC7 all
/// four. A picture already compressed, or whose sides are not whole blocks of four (which the
/// device does not take in blocks), is returned as it is.
#[must_use]
pub fn compressed(image: Image, format: BlockFormat) -> Image {
    if image.format.is_some() || !image.width.is_multiple_of(4) || !image.height.is_multiple_of(4) {
        return image;
    }
    let levels = (0u32..)
        .zip(&image.levels)
        .map(|(k, level)| {
            let (w, h) = ((image.width >> k).max(1), (image.height >> k).max(1));
            Arc::new(dereth_render::block_codec::encode(format, w, h, level))
        })
        .collect();
    Image {
        levels,
        format: Some(format),
        ..image
    }
}

/// The PNG in `bytes`, as four bytes a texel.
///
/// # Errors
/// What the PNG decoder said, as a sentence.
pub fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| format!("not a PNG: {e}"))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("the PNG will not decode: {e}"))?;
    let (w, h) = (info.width, info.height);
    let texels = (w as usize) * (h as usize);
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..texels * 4].to_vec(),
        png::ColorType::Rgb => buf[..texels * 3]
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf[..texels * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => buf[..texels]
            .iter()
            .flat_map(|g| [*g, *g, *g, 255])
            .collect(),
        png::ColorType::Indexed => return Err("an indexed PNG was not expanded".into()),
    };
    Ok((w, h, rgba))
}

/// `rgba` (`width` by `height`) stretched to the next whole block of four texels on each side,
/// each texel the bilinear mix of the four nearest, clamped at the edges; as it is where its
/// sides are whole blocks already. A model's coordinates into its picture run from edge to edge,
/// so the stretched picture lies where the picture did, and the device takes it in blocks.
#[must_use]
pub fn to_whole_blocks(width: u32, height: u32, rgba: Vec<u8>) -> (u32, u32, Vec<u8>) {
    let (nw, nh) = (width.next_multiple_of(4), height.next_multiple_of(4));
    if (nw, nh) == (width, height) || width == 0 || height == 0 {
        return (width, height, rgba);
    }
    (nw, nh, stretched(width, height, &rgba, nw, nh))
}

/// `rgba` (`width` by `height`) stretched to `nw` by `nh`, each texel the bilinear mix of the
/// four nearest, clamped at the edges: the picture lies edge to edge where it did.
#[must_use]
pub fn stretched(width: u32, height: u32, rgba: &[u8], nw: u32, nh: u32) -> Vec<u8> {
    if (nw, nh) == (width, height) || width == 0 || height == 0 {
        return rgba.to_vec();
    }
    let (w, h) = (width as usize, height as usize);
    let mut out = Vec::with_capacity((nw as usize) * (nh as usize) * 4);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    // Texel coordinates of pictures far smaller than a float's exact range.
    for y in 0..nh as usize {
        let fy = ((y as f32 + 0.5) * h as f32 / nh as f32 - 0.5).clamp(0.0, (h - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy.fract());
        let y1 = (y0 + 1).min(h - 1);
        for x in 0..nw as usize {
            let fx = ((x as f32 + 0.5) * w as f32 / nw as f32 - 0.5).clamp(0.0, (w - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx.fract());
            let x1 = (x0 + 1).min(w - 1);
            for c in 0..4 {
                let at = |xx: usize, yy: usize| f32::from(rgba[(yy * w + xx) * 4 + c]);
                let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
                let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
                out.push((top + (bottom - top) * ty + 0.5).clamp(0.0, 255.0) as u8);
            }
        }
    }
    out
}

/// The width and height of the PNG in `bytes`, from its header alone.
///
/// # Errors
/// What the PNG decoder said, as a sentence.
pub fn png_size(bytes: &[u8]) -> Result<(u32, u32), String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder.read_info().map_err(|e| format!("not a PNG: {e}"))?;
    let info = reader.info();
    Ok((info.width, info.height))
}

/// `f` of each of `items`, in their order, the items shared out among the machine's threads as
/// each finishes (on the calling thread alone where there are none to share).
pub fn par_map<T: Send, R: Send>(items: Vec<T>, f: &(dyn Fn(T) -> R + Sync)) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, PoisonError};
    #[cfg(target_arch = "wasm32")]
    let threads = 1;
    #[cfg(not(target_arch = "wasm32"))]
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let threads = threads.min(items.len());
    if threads <= 1 {
        return items.into_iter().map(f).collect();
    }
    let n = items.len();
    let given: Vec<Mutex<Option<T>>> = items.into_iter().map(|t| Mutex::new(Some(t))).collect();
    let made: Vec<Mutex<Option<R>>> = (0..n).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= n {
                    break;
                }
                let item = given[i]
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                if let Some(item) = item {
                    let r = f(item);
                    *made[i].lock().unwrap_or_else(PoisonError::into_inner) = Some(r);
                }
            });
        }
    });
    made.into_iter()
        .filter_map(|m| m.into_inner().unwrap_or_else(PoisonError::into_inner))
        .collect()
}

fn to_linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        dereth_primitives::num::math::powf((c + 0.055) / 1.055, 2.4)
    }
}

fn to_byte(linear: f32, srgb: bool) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let e = if !srgb {
        c
    } else if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * dereth_primitives::num::math::powf(c, 1.0 / 2.4) - 0.055
    };
    // LINT-OK: a 0..1 value scaled to 0..255 and rounded. Not a truncation of anything signed.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = (e * 255.0 + 0.5) as u8;
    b
}

/// The level below `level` (`w` by `h`): each texel the average of the two by two above it, in
/// linear light for a colour picture.
fn shrink(level: &[u8], w: u32, h: u32, srgb: bool) -> (u32, u32, Vec<u8>) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    let lut: Vec<f32> = (0..=255u8).map(to_linear).collect();
    for y in 0..nh {
        for x in 0..nw {
            let mut sum = [0.0f32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let sx = (x * 2 + dx).min(w - 1);
                let sy = (y * 2 + dy).min(h - 1);
                let i = ((sy * w + sx) * 4) as usize;
                for c in 0..3 {
                    sum[c] += if srgb {
                        lut[level[i + c] as usize]
                    } else {
                        f32::from(level[i + c]) / 255.0
                    };
                }
                sum[3] += f32::from(level[i + 3]) / 255.0;
            }
            for (c, s) in sum.iter().enumerate() {
                out.push(to_byte(s / 4.0, srgb && c < 3));
            }
        }
    }
    (nw, nh, out)
}

/// The share of `level`'s texels whose alpha, scaled by `scale`, passes `cutoff`.
fn coverage(level: &[u8], cutoff: f32, scale: f32) -> f32 {
    let texels = level.len() / 4;
    if texels == 0 {
        return 0.0;
    }
    let passing = level
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|t| f32::from(t[3]) / 255.0 * scale >= cutoff)
        .count();
    #[allow(clippy::cast_precision_loss)] // a share of texel counts
    let share = passing as f32 / texels as f32;
    share
}

/// `level`'s alpha scaled so the share passing `cutoff` is `wanted`, as near as a search finds.
fn keep_coverage(level: &mut [u8], cutoff: f32, wanted: f32) {
    let (mut lo, mut hi) = (0.0f32, 4.0f32);
    for _ in 0..12 {
        let mid = (lo + hi) * 0.5;
        if coverage(level, cutoff, mid) < wanted {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    for t in level.as_chunks_mut::<4>().0.iter_mut() {
        t[3] = to_byte(f32::from(t[3]) / 255.0 * hi, false);
    }
}

/// `rgba` (`width` by `height`) with every smaller level down to one texel. A colour picture
/// (`srgb`) is averaged in linear light; a cut-out picture (`cutoff`) keeps its coverage.
#[must_use]
pub fn with_levels(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    srgb: bool,
    cutoff: Option<f32>,
) -> Image {
    let wanted = cutoff.map(|c| coverage(&rgba, c, 1.0));
    let mut levels = vec![Arc::new(rgba)];
    let (mut w, mut h) = (width, height);
    while w > 1 || h > 1 {
        let (nw, nh, mut next) =
            shrink(levels.last().map_or(&[][..], |l| l.as_slice()), w, h, srgb);
        if let (Some(c), Some(share)) = (cutoff, wanted) {
            keep_coverage(&mut next, c, share);
        }
        levels.push(Arc::new(next));
        (w, h) = (nw, nh);
    }
    Image {
        width,
        height,
        srgb,
        levels,
        format: None,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;

    #[test]
    fn every_level_down_to_one_texel_is_made_and_colour_is_averaged_in_linear_light() {
        // Black and white texels in a checker: the level below is mid-grey in light, which the
        // display's encoding writes as 188, not 128.
        let mut rgba = Vec::new();
        for y in 0..4u32 {
            for x in 0..4u32 {
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                rgba.extend([v, v, v, 255]);
            }
        }
        let image = with_levels(4, 4, rgba.clone(), true, None);
        assert_eq!(image.levels.len(), 3);
        assert_eq!(image.levels[1].len(), 2 * 2 * 4);
        assert_eq!(image.levels[2].len(), 4);
        assert!(
            (186..=190).contains(&image.levels[1][0]),
            "{}",
            image.levels[1][0]
        );
        let data = with_levels(4, 4, rgba, false, None);
        assert!(
            (127..=128).contains(&data.levels[1][0]),
            "{}",
            data.levels[1][0]
        );
        // An odd size halves down to one on each side.
        assert_eq!(
            with_levels(5, 3, vec![0; 5 * 3 * 4], false, None)
                .levels
                .len(),
            3
        );
    }

    #[test]
    fn a_cut_out_picture_keeps_the_share_that_passes_the_cut_at_every_level() {
        // Thin opaque lines one texel wide in every fourth row: a quarter of the picture passes
        // the cut. Plain averaging leaves no texel of the next level but one passing.
        let (w, h) = (16u32, 16u32);
        let mut rgba = Vec::new();
        for y in 0..h {
            for _ in 0..w {
                rgba.extend([90, 140, 60, if y % 4 == 0 { 255 } else { 0 }]);
            }
        }
        let plain = with_levels(w, h, rgba.clone(), true, None);
        let kept = with_levels(w, h, rgba, true, Some(0.6));
        let share = |l: &[u8]| coverage(l, 0.6, 1.0);
        assert!((share(&kept.levels[0]) - 0.25).abs() < 1e-6);
        assert!(
            share(&plain.levels[1]) < 0.01,
            "{}",
            share(&plain.levels[1])
        );
        // Every smaller level still passes at least the share the full picture does.
        for l in &kept.levels[1..] {
            assert!(share(l) >= 0.25, "{}", share(l));
        }
    }

    #[test]
    fn a_picture_of_whole_blocks_is_compressed_level_by_level_and_others_are_kept() {
        let rgba: Vec<u8> = (0..=255u8).flat_map(|k| [k, k / 2, 255 - k, 255]).collect();
        let image = with_levels(16, 16, rgba, true, None);
        let bc7 = compressed(image.clone(), BlockFormat::Bc7);
        assert_eq!(bc7.format, Some(BlockFormat::Bc7));
        assert_eq!(bc7.levels.len(), image.levels.len());
        // Sixteen texels a side: four by four blocks of sixteen bytes, a quarter of the bytes;
        // the levels below four texels take a whole block.
        let sizes: Vec<usize> = bc7.levels.iter().map(|l| l.len()).collect();
        assert_eq!(sizes, [256, 64, 16, 16, 16]);
        assert!(bc7.srgb);
        // Already compressed: kept.
        assert_eq!(compressed(bc7.clone(), BlockFormat::Bc5), bc7);
        // Sides not whole blocks: kept.
        let odd = with_levels(6, 6, vec![7; 6 * 6 * 4], false, None);
        assert_eq!(compressed(odd.clone(), BlockFormat::Bc7), odd);
    }

    #[test]
    fn a_picture_of_part_blocks_is_stretched_to_whole_blocks_keeping_its_edges() {
        // Five by two: a ramp across, black to white, and two rows alike.
        let rgba: Vec<u8> = (0..2)
            .flat_map(|_| (0..5u8).flat_map(|x| [x * 60, x * 60, x * 60, 255]))
            .collect();
        let (w, h, out) = to_whole_blocks(5, 2, rgba.clone());
        assert_eq!((w, h), (8, 4));
        assert_eq!(out.len(), 8 * 4 * 4);
        // The first and last columns keep the picture's edges; the ramp still climbs.
        assert_eq!(out[0], 0);
        assert_eq!(out[7 * 4], 240);
        let row: Vec<u8> = out[..8 * 4].chunks(4).map(|t| t[0]).collect();
        assert!(row.windows(2).all(|p| p[0] <= p[1]), "{row:?}");
        // Whole blocks already: as it is.
        let whole = vec![9; 4 * 4 * 4];
        assert_eq!(to_whole_blocks(4, 4, whole.clone()), (4, 4, whole));
    }

    #[test]
    fn a_png_reads_into_four_bytes_a_texel() {
        let mut bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bytes, 2, 1);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("header");
            w.write_image_data(&[10, 20, 30, 40, 50, 60]).expect("data");
        }
        assert_eq!(
            decode_png(&bytes),
            Ok((2, 1, vec![10, 20, 30, 255, 40, 50, 60, 255]))
        );
        assert!(decode_png(b"not a png").is_err());
    }
}
