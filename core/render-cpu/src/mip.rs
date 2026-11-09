//! System-memory mip policy (BOX/MIRROR, compressed sources capped at four) and the separate
//! runtime image-texture eligibility.
//!
//! Texture records are described in `docs/formats/14-textures.md`. Upload, scaling and mipmap
//! generation preserve the verified level policy below.
//!
//! The policy in one place:
//!
//! ```text
//! levels = 1
//! if (max(w, h) > 1) { levels = 1 + floor(log2(max(w, h))); if (levels > 4) levels = 4 }
//! if (!(format.flags & 4 /*compressed*/)) levels = 1
//! ```
//!
//! then `D3DXLoadSurfaceFromSurface(..., D3DX_FILTER_BOX | D3DX_FILTER_MIRROR /* 0x00070005 */)`
//! and `D3DXFilterTexture(tex, NULL, 0, D3DX_FILTER_BOX | D3DX_FILTER_MIRROR)`.
//!
//! Non-compressed sources get **one** level here and rely on `D3DUSAGE_AUTOGENMIPMAP` for the
//! video-memory copy. The client's one-level system-memory policy does
//! NOT forbid runtime mips for expanded indexed world textures. The D3D12 image-texture upload generates
//! that separate chain on the device; it does not use the CPU BOX rounding below.

use dereth_primitives::{TextureData, TextureFormat};

use crate::RenderError;

/// `D3DX_FILTER_BOX | D3DX_FILTER_MIRROR`, the exact filter word the client passes.
pub const D3DX_FILTER_BOX_MIRROR: u32 = 0x0007_0005;

/// The cap in `if (levels > 4) levels = 4`.
pub const MAX_MIP_LEVELS: u32 = 4;

/// The per-channel tolerance the mip parity harness allows against a 2003-era D3DX 9.0 build.
///
/// the exact per-format rounding constants in D3DX's
/// `BltBox2D_*` were not transcribed, so whether the reference rounds half up, half to even, or
/// through a float multiply-add is not settled. The value here is deliberately a *named constant*
/// rather than a hand-tuned number: do **not** hand-tune it until a reference fixture exists.
/// Zero means "we currently claim exact agreement"; when the fixture lands, either
/// this holds or the rounding in [`box_filter_bgra8`] is wrong and should be corrected rather than
/// the tolerance raised.
pub const MAX_MIP_CHANNEL_DELTA: u8 = 0;

/// `1 + floor(log2(max(w, h)))`, capped at four, and forced to one for a non-compressed source.
///
/// `is_compressed` is the pixel format's `flags & 4`, i.e. DXT1-5 and nothing else.
#[must_use]
pub fn level_count(width: u32, height: u32, is_compressed: bool) -> u32 {
    if !is_compressed {
        return 1;
    }
    let m = width.max(height);
    if m <= 1 {
        return 1;
    }
    // 1 + floor(log2(m)); ilog2 is exactly floor(log2()) for a non-zero integer.
    (1 + m.ilog2()).min(MAX_MIP_LEVELS)
}

/// Runtime image-texture copy, distinct from the compressed system-memory cap in [`level_count`].
/// Retail's D3D texture fetch requests AUTOGEN only for one uncompressed system-memory level and
/// supported hardware. D3D9 sublevels are driver-owned and extend down to 1x1. Only BGRA8 is
/// implemented by the current device path; BC and explicit chains pass through untouched.
#[must_use]
pub fn runtime_level_count(source: &TextureData, supported: bool) -> usize {
    if supported
        && source.format == TextureFormat::Bgra8
        && source.levels.len() == 1
        && source.width != 0
        && source.height != 0
    {
        (1 + source.width.max(source.height).ilog2()) as usize
    } else {
        source.levels.len()
    }
}

/// Compressed image-texture system-memory preparation, before device upload. DXT1-5
/// are numerically ported from bundled D3DX. DXT2/4 preserve source provenance
/// and filter through their unpremultiply/BOX/premultiply codec wrappers.
/// Provided chains are a no-op. Only the explicit image-texture upload owner calls this
/// before its device copy; generic UI/font/movie uploads do not call it.
///
/// # Errors
/// Invalid dimensions or short block data return the normal source error.
pub fn compressed_system_chain(source: &TextureData) -> Result<Option<TextureData>, RenderError> {
    if !matches!(
        source.format,
        TextureFormat::Bc1
            | TextureFormat::Bc2
            | TextureFormat::Bc3
            | TextureFormat::Bc2Premultiplied
            | TextureFormat::Bc3Premultiplied
    ) || source.levels.len() != 1
    {
        return Ok(None);
    }
    let levels = level_count(source.width, source.height, true);
    if levels <= 1 {
        return Ok(None);
    }
    let mut result = source.clone();
    let (mut width, mut height) = (source.width, source.height);
    for _ in 1..levels {
        let previous = result.levels.last().unwrap();
        let next = match source.format {
            TextureFormat::Bc1 => crate::d3dx_bc::half_bc1(previous, width, height)?,
            TextureFormat::Bc2 => crate::d3dx_bc::half_bc2(previous, width, height)?,
            TextureFormat::Bc3 => crate::d3dx_bc::half_bc3(previous, width, height)?,
            TextureFormat::Bc2Premultiplied => {
                crate::d3dx_bc::half_bc2_premultiplied(previous, width, height)?
            }
            TextureFormat::Bc3Premultiplied => {
                crate::d3dx_bc::half_bc3_premultiplied(previous, width, height)?
            }
            _ => unreachable!("only verified system codecs enter this path"),
        };
        result.levels.push(next);
        width = half(width);
        height = half(height);
    }
    Ok(Some(result))
}

/// Half a dimension the way a mip chain does, never going below one.
#[must_use]
pub const fn half(v: u32) -> u32 {
    if v > 1 {
        v / 2
    } else {
        1
    }
}

/// Mirror a coordinate back into `[0, extent)`, which is `D3DX_FILTER_MIRROR`.
///
/// Mirroring only bites when the source extent is odd, because a 2×2 box on the last column then
/// reaches one texel past the edge; for the power-of-two textures the client mostly ships it never
/// fires at all. It is kept because the UI surfaces are power-of-two but their *content* is not,
/// and because getting it wrong is invisible until it isn't.
#[must_use]
pub const fn mirror(coord: i64, extent: u32) -> u32 {
    let e = extent as i64;
    if e <= 1 {
        return 0;
    }
    let mut c = coord;
    if c < 0 {
        c = -c - 1;
    }
    if c >= e {
        c = 2 * e - c - 1;
    }
    if c < 0 {
        c = 0;
    }
    if c >= e {
        c = e - 1;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: the clamps above bound c into [0, extent).
    {
        c as u32
    }
}

/// One box-filter halving step over BGRA8 with mirror edge handling.
///
/// The 2×2 average is `(a + b + c + d + 2) / 4`, i.e. the mean rounded half up. See
/// [`MAX_MIP_CHANNEL_DELTA`] for the allowed difference from device-generated pixels.
///
/// # Errors
/// Returns [`RenderError::ShortSourceData`] when `src` is smaller than `width * height * 4`.
pub fn box_filter_bgra8(
    src: &[u8],
    width: u32,
    height: u32,
) -> Result<(Vec<u8>, u32, u32), RenderError> {
    let expected = width as usize * height as usize * 4;
    if src.len() < expected {
        return Err(RenderError::ShortSourceData {
            format: crate::PixelFormatId::A8R8G8B8,
            width,
            height,
            expected,
            actual: src.len(),
        });
    }
    let (dw, dh) = (half(width), half(height));
    let mut out = vec![0u8; dw as usize * dh as usize * 4];
    let at = |x: u32, y: u32, c: usize| -> u32 {
        u32::from(src[((y as usize * width as usize) + x as usize) * 4 + c])
    };
    for y in 0..dh {
        for x in 0..dw {
            // The four source texels this destination texel averages, mirrored at the edges.
            let sx0 = mirror(i64::from(x) * 2, width);
            let sx1 = mirror(i64::from(x) * 2 + 1, width);
            let sy0 = mirror(i64::from(y) * 2, height);
            let sy1 = mirror(i64::from(y) * 2 + 1, height);
            let d = ((y as usize * dw as usize) + x as usize) * 4;
            for c in 0..4 {
                let sum = at(sx0, sy0, c) + at(sx1, sy0, c) + at(sx0, sy1, c) + at(sx1, sy1, c);
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: the sum of four bytes plus 2, divided by 4, is at most 255.
                {
                    out[d + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
    }
    Ok((out, dw, dh))
}

/// Build the mip chain for a level-0 image, following the client's policy exactly.
///
/// Returns the whole chain **including level 0**, so the result is never empty. A non-compressed
/// source comes back as a one-element chain, which is what the client's texture creation produces and what
/// `D3DUSAGE_AUTOGENMIPMAP` then completes on the GPU.
///
/// # Errors
/// BC1/DXT3 use the same bundled-D3DX numeric implementation as production image-texture upload.
/// DXT2/BC3 encoding is still pending and returns [`RenderError::Unsupported`].
pub fn build_mip_chain(level0: &TextureData) -> Result<Vec<TextureData>, RenderError> {
    if let Some(chain) = compressed_system_chain(level0)? {
        return Ok(chain
            .levels
            .into_iter()
            .enumerate()
            .map(|(i, bytes)| TextureData {
                width: (chain.width >> i).max(1),
                height: (chain.height >> i).max(1),
                format: chain.format,
                levels: vec![bytes],
            })
            .collect());
    }
    let is_compressed = matches!(
        level0.format,
        TextureFormat::Bc1
            | TextureFormat::Bc2
            | TextureFormat::Bc2Premultiplied
            | TextureFormat::Bc3
            | TextureFormat::Bc3Premultiplied
    );
    let levels = level_count(level0.width, level0.height, is_compressed);
    if levels == 1 {
        return Ok(vec![level0.clone()]);
    }
    if is_compressed {
        return Err(RenderError::Unsupported(
            "this source format needs its own verified bundled D3DX system mip codec",
        ));
    }
    let mut chain = Vec::with_capacity(levels as usize);
    chain.push(level0.clone());
    let mut bits = level0.levels.first().cloned().unwrap_or_default();
    let (mut w, mut h) = (level0.width, level0.height);
    for _ in 1..levels {
        let (next, nw, nh) = box_filter_bgra8(&bits, w, h)?;
        chain.push(TextureData {
            width: nw,
            height: nh,
            format: level0.format,
            levels: vec![next.clone()],
        });
        bits = next;
        w = nw;
        h = nh;
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_imgtex_policy_is_separate_from_system_and_provided_chains() {
        let mut t = TextureData {
            width: 256,
            height: 128,
            format: TextureFormat::Bgra8,
            levels: vec![vec![]],
        };
        assert_eq!(level_count(t.width, t.height, false), 1);
        assert_eq!(runtime_level_count(&t, true), 9);
        assert_eq!(runtime_level_count(&t, false), 1);
        t.levels.push(vec![]);
        assert_eq!(
            runtime_level_count(&t, true),
            2,
            "provided levels are not replaced"
        );
        t.levels.truncate(1);
        for format in [
            TextureFormat::Bc1,
            TextureFormat::Bc2,
            TextureFormat::Bc2Premultiplied,
            TextureFormat::Bc3,
            TextureFormat::Bc3Premultiplied,
        ] {
            t.format = format;
            assert_eq!(
                runtime_level_count(&t, true),
                1,
                "BC system chain is a separate port"
            );
        }
        t.format = TextureFormat::Bgra8;
        for (w, h, n) in [(1, 1, 1), (1, 16, 5), (7, 3, 3), (0, 8, 1)] {
            t.width = w;
            t.height = h;
            assert_eq!(runtime_level_count(&t, true), n);
        }
        t.levels.clear();
        assert_eq!(
            runtime_level_count(&t, true),
            0,
            "upload validates missing data"
        );
    }

    // The client's verified level policy is:
    //   levels = 1
    //   if (max(w,h) > 1) { levels = 1 + floor(log2(max(w,h))); if (levels > 4) levels = 4 }
    //   if (!(compressed)) levels = 1
    // Compressed sources retain their authored mip chain.
    #[test]
    fn the_level_count_is_capped_at_four_and_only_for_compressed_sources() {
        // Compressed sources: 1 + floor(log2(max)), capped at 4.
        assert_eq!(level_count(1, 1, true), 1);
        assert_eq!(level_count(2, 1, true), 2);
        assert_eq!(level_count(2, 2, true), 2);
        assert_eq!(level_count(4, 4, true), 3);
        assert_eq!(level_count(8, 8, true), 4);
        // The cap bites from 16 upward: 1 + log2(16) = 5 would be a full chain.
        assert_eq!(level_count(16, 16, true), 4);
        assert_eq!(level_count(256, 256, true), 4);
        assert_eq!(level_count(1024, 64, true), 4);
        // Non-square: the larger extent decides.
        assert_eq!(level_count(8, 1, true), 4);
        assert_eq!(level_count(1, 4, true), 3);
        // A non-power-of-two extent uses floor(log2()): 1 + floor(log2(5)) = 3.
        assert_eq!(level_count(5, 3, true), 3);

        // Non-compressed sources get exactly one level, whatever their size. Adding
        // mips to the 16-bit indexed world textures is a deviation, not an improvement.
        for (w, h) in [(1u32, 1u32), (8, 8), (256, 256), (1024, 1024)] {
            assert_eq!(level_count(w, h, false), 1, "{w}x{h}");
        }
    }

    // Oracle: the same quoted code -- the filter word is D3DX_FILTER_BOX | D3DX_FILTER_MIRROR,
    // written out in the document as the literal 0x00070005.
    #[test]
    fn the_filter_word_is_the_documented_literal() {
        assert_eq!(D3DX_FILTER_BOX_MIRROR, 0x0007_0005);
        assert_eq!(MAX_MIP_LEVELS, 4);
    }

    // Oracle: D3DX_FILTER_MIRROR's definition -- a coordinate outside the extent is reflected about
    // the edge, with the edge texel repeated (the "half-sample symmetric" convention D3DX uses).
    #[test]
    fn mirror_reflects_about_the_edges() {
        // Inside the extent, nothing happens.
        for c in 0..5u32 {
            assert_eq!(mirror(i64::from(c), 5), c);
        }
        // One past each end reflects back onto the last valid texel.
        assert_eq!(mirror(-1, 5), 0);
        assert_eq!(mirror(5, 5), 4);
        assert_eq!(mirror(-2, 5), 1);
        assert_eq!(mirror(6, 5), 3);
        // A degenerate extent has only one texel.
        assert_eq!(mirror(7, 1), 0);
        assert_eq!(mirror(-7, 1), 0);
        // Far out of range still lands inside rather than panicking.
        assert!(mirror(1_000_000, 5) < 5);
        assert!(mirror(-1_000_000, 5) < 5);
    }

    // Oracle: the box filter's definition -- each destination texel is the mean of the 2x2 source
    // texels beneath it. Asserted on values chosen so the mean is exact, so the test proves the
    // *footprint* (which four texels) independently of the rounding, which is what open question
    // #187 leaves open.
    #[test]
    fn the_box_filter_averages_each_two_by_two_group() {
        // A 4x2 BGRA image whose blue channel is 0,4,8,12 / 4,8,12,16.
        let mut src = Vec::new();
        for y in 0..2u8 {
            for x in 0..4u8 {
                src.extend_from_slice(&[x * 4 + y * 4, 0, 0, 255]);
            }
        }
        let (out, w, h) = box_filter_bgra8(&src, 4, 2).unwrap();
        assert_eq!((w, h), (2, 1));
        // Group 0 covers blues 0, 4 (row 0) and 4, 8 (row 1) -> mean 4.
        assert_eq!(out[0], 4);
        // Group 1 covers 8, 12 and 12, 16 -> mean 12.
        assert_eq!(out[4], 12);
        // Alpha is filtered like any other channel.
        assert_eq!(out[3], 255);
    }

    // Oracle: D3DX_FILTER_MIRROR again -- an odd source extent makes the last 2x2 box reach past
    // the edge, and the mirror rule says it re-reads the edge texel. Without the mirror the last
    // column would either wrap or clamp, both of which give a different texel.
    #[test]
    fn an_odd_extent_makes_the_mirror_rule_observable() {
        // A 3x1 image, blues 0, 100, 200. The destination is 1x1 wide (half(3) == 1).
        let src = vec![0u8, 0, 0, 255, 100, 0, 0, 255, 200, 0, 0, 255];
        let (out, w, h) = box_filter_bgra8(&src, 3, 1).unwrap();
        assert_eq!((w, h), (1, 1));
        // Only source columns 0 and 1 are covered; the height of 1 makes both rows the same texel,
        // so the mean is (0 + 100 + 0 + 100 + 2) / 4 = 50.
        assert_eq!(out[0], 50);

        // A 1x3 image exercises the same rule on the other axis.
        let src = vec![0u8, 0, 0, 255, 100, 0, 0, 255, 200, 0, 0, 255];
        let (out, w, h) = box_filter_bgra8(&src, 1, 3).unwrap();
        assert_eq!((w, h), (1, 1));
        assert_eq!(out[0], 50);
    }

    // The CPU filter rounds the mean half up. MAX_MIP_CHANNEL_DELTA bounds the allowed
    // difference from device-generated pixels.
    #[test]
    fn the_mean_is_rounded_half_up() {
        // Four texels summing to 2 -> mean 0.5 -> 1 under round-half-up, 0 under truncation.
        let src = vec![
            1u8, 0, 0, 0, //
            1, 0, 0, 0, //
            0, 0, 0, 0, //
            0, 0, 0, 0,
        ];
        let (out, _, _) = box_filter_bgra8(&src, 2, 2).unwrap();
        assert_eq!(out[0], 1, "the mean of 1,1,0,0 is 0.5 and rounds up");
        assert_eq!(
            MAX_MIP_CHANNEL_DELTA, 0,
            "not hand-tuned; see open question #187"
        );
    }

    // Oracle: the policy above, applied end to end. A non-compressed source gets one level, which
    // is the compressed-source level policy.
    #[test]
    fn a_non_compressed_source_gets_exactly_one_level() {
        let t = TextureData {
            width: 64,
            height: 64,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0x40; 64 * 64 * 4]],
        };
        let chain = build_mip_chain(&t).unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].width, 64);
    }

    // Every DXT codec retains source identity and its own numeric path.
    #[test]
    fn all_bc_source_kinds_build_four_system_levels_and_keep_provided_chains() {
        let t = TextureData {
            width: 64,
            height: 64,
            format: TextureFormat::Bc1,
            levels: vec![vec![0; 64 * 64 / 2]],
        };
        assert_eq!(level_count(t.width, t.height, true), 4);
        let chain = build_mip_chain(&t).unwrap();
        assert_eq!(chain.len(), 4);
        assert_eq!(chain[0].levels[0], t.levels[0]);
        assert_eq!(chain[3].width, 8);
        assert_eq!(chain[3].levels[0].len(), 32);
        for format in [
            TextureFormat::Bc2,
            TextureFormat::Bc3,
            TextureFormat::Bc2Premultiplied,
            TextureFormat::Bc3Premultiplied,
        ] {
            let source = TextureData {
                format,
                levels: vec![vec![0; 64 * 64]],
                ..t.clone()
            };
            let chain = compressed_system_chain(&source).unwrap().unwrap();
            assert_eq!(chain.levels.len(), 4);
            assert_eq!(chain.format, format);
            assert_eq!(chain.levels[0], source.levels[0]);
            assert_eq!(chain.levels[3].len(), 64);
            let provided = TextureData {
                levels: vec![source.levels[0].clone(), vec![0; 32 * 32]],
                ..source
            };
            assert!(
                compressed_system_chain(&provided).unwrap().is_none(),
                "provided chains are untouched"
            );
        }
        // A 1x1 block-compressed texture wants one level and therefore succeeds trivially.
        let t = TextureData {
            width: 1,
            height: 1,
            format: TextureFormat::Bc1,
            levels: vec![vec![0; 8]],
        };
        assert_eq!(build_mip_chain(&t).unwrap().len(), 1);
    }

    // Oracle: the halving rule -- extents halve per level and never reach zero.
    #[test]
    fn dimensions_halve_and_stop_at_one() {
        assert_eq!(half(8), 4);
        assert_eq!(half(1), 1);
        assert_eq!(half(0), 1);
        assert_eq!(half(3), 1);
    }

    #[test]
    fn short_input_is_an_error_not_a_panic() {
        assert!(box_filter_bgra8(&[0u8; 8], 4, 4).is_err());
    }
}
