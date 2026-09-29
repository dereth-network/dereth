//! Block-compressed (DXT / S3TC) decoding.
//!
//! The client itself **never decodes DXT**: it copies
//! the blocks verbatim into the locked surface ("the whole image size in one block copy; no row
//! loop, because DXT rows are 4-pixel blocks") and D3D consumes them. Two paths still need a decoder:
//!
//! * the client's fallback -- when compressed textures are unsupported it uses the display's ARGB
//!   texture format, which requires expanding the blocks on the CPU;
//! * the texture parity harness and the headless capture backend, which compare pixels.
//!
//! The layout is the S3TC one: 8 bytes per DXT1 block, 16 per DXT2-5 (a 8-byte alpha block followed
//! by the 8-byte DXT1 colour block). `PixelFormatDesc` records 4 bits per pixel for DXT1 and 8 for
//! the rest, which is the same statement.
//!
//! DXT2/DXT4 are the premultiplied-alpha variants; they decode identically to DXT3/DXT5 and differ
//! only in how the result is interpreted, which is why the format decoder gives them the
//! same descriptor.

use crate::pixel_format::PixelFormatId;
use crate::RenderError;

/// Which alpha encoding a block carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlphaKind {
    /// DXT1: one bit of alpha, carried by the colour block's `c0 <= c1` case.
    OneBit,
    /// DXT2/3: four bits per texel, stored directly.
    Explicit4,
    /// DXT4/5: two endpoints plus a 3-bit index per texel.
    Interpolated,
}

/// Expand a 5-bit channel to 8 bits by bit replication: `(v << 3) | (v >> 2)`.
const fn e5(v: u32) -> u32 {
    (v << 3) | (v >> 2)
}
/// Expand a 6-bit channel to 8 bits by bit replication: `(v << 2) | (v >> 4)`.
const fn e6(v: u32) -> u32 {
    (v << 2) | (v >> 4)
}

/// Unpack one RGB565 endpoint to `(r, g, b)` bytes.
const fn rgb565(v: u16) -> (u32, u32, u32) {
    let v = v as u32;
    (e5((v >> 11) & 0x1F), e6((v >> 5) & 0x3F), e5(v & 0x1F))
}

/// Decode a DXT surface to BGRA8 (the byte order `dereth_primitives::TextureFormat::Bgra8` names, and the
/// order D3D9's `A8R8G8B8` has in memory).
///
/// `width`/`height` are the *pixel* dimensions; blocks that hang off the right or bottom edge have
/// their surplus texels dropped, which is what the hardware does.
pub fn decode(
    format: PixelFormatId,
    blocks: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, RenderError> {
    let (block_bytes, alpha) = match format {
        PixelFormatId::Dxt1 => (8usize, AlphaKind::OneBit),
        PixelFormatId::Dxt2 | PixelFormatId::Dxt3 => (16usize, AlphaKind::Explicit4),
        PixelFormatId::Dxt4 | PixelFormatId::Dxt5 => (16usize, AlphaKind::Interpolated),
        other => return Err(RenderError::UnsupportedFormat(other)),
    };
    if width == 0 || height == 0 {
        return Err(RenderError::BadDimensions {
            width,
            height,
            reason: "a compressed surface must have a non-zero extent",
        });
    }
    let bw = width.div_ceil(4) as usize;
    let bh = height.div_ceil(4) as usize;
    let expected = bw * bh * block_bytes;
    if blocks.len() < expected {
        return Err(RenderError::ShortSourceData {
            format,
            width,
            height,
            expected,
            actual: blocks.len(),
        });
    }

    let w = width as usize;
    let h = height as usize;
    let mut out = vec![0u8; w * h * 4];
    let mut texels = [[0u8; 4]; 16];
    for by in 0..bh {
        for bx in 0..bw {
            let base = (by * bw + bx) * block_bytes;
            let block = &blocks[base..base + block_bytes];
            decode_block(block, alpha, &mut texels);
            for ty in 0..4 {
                let y = by * 4 + ty;
                if y >= h {
                    break;
                }
                for tx in 0..4 {
                    let x = bx * 4 + tx;
                    if x >= w {
                        break;
                    }
                    let d = (y * w + x) * 4;
                    out[d..d + 4].copy_from_slice(&texels[ty * 4 + tx]);
                }
            }
        }
    }
    Ok(out)
}

/// Decode one 8- or 16-byte block into 16 BGRA texels.
fn decode_block(block: &[u8], alpha: AlphaKind, out: &mut [[u8; 4]; 16]) {
    let (alpha_block, color_block) = match alpha {
        AlphaKind::OneBit => (None, block),
        _ => (Some(&block[..8]), &block[8..16]),
    };

    let c0 = u16::from_le_bytes([color_block[0], color_block[1]]);
    let c1 = u16::from_le_bytes([color_block[2], color_block[3]]);
    let (r0, g0, b0) = rgb565(c0);
    let (r1, g1, b1) = rgb565(c1);

    // The four palette entries. The `c0 > c1` test only selects the 1-bit-alpha mode for DXT1;
    // DXT2-5 always use the four-colour form regardless of endpoint order (S3TC spec, and what
    // every D3D9-era decoder does).
    let four_colour = alpha != AlphaKind::OneBit || c0 > c1;
    let mut palette = [[0u8; 4]; 4];
    let put = |slot: &mut [u8; 4], r: u32, g: u32, b: u32, a: u32| {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: every operand is an 8-bit channel produced by integer arithmetic.
        {
            *slot = [b as u8, g as u8, r as u8, a as u8];
        }
    };
    put(&mut palette[0], r0, g0, b0, 255);
    put(&mut palette[1], r1, g1, b1, 255);
    if four_colour {
        // 1/3 and 2/3 points, integer, truncating -- the classic S3TC reference form.
        put(
            &mut palette[2],
            (2 * r0 + r1) / 3,
            (2 * g0 + g1) / 3,
            (2 * b0 + b1) / 3,
            255,
        );
        put(
            &mut palette[3],
            (r0 + 2 * r1) / 3,
            (g0 + 2 * g1) / 3,
            (b0 + 2 * b1) / 3,
            255,
        );
    } else {
        // Midpoint, and a fully transparent black fourth entry.
        put(
            &mut palette[2],
            (r0 + r1) / 2,
            (g0 + g1) / 2,
            (b0 + b1) / 2,
            255,
        );
        palette[3] = [0, 0, 0, 0];
    }

    let bits = u32::from_le_bytes([
        color_block[4],
        color_block[5],
        color_block[6],
        color_block[7],
    ]);
    for (i, texel) in out.iter_mut().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a two-bit field.
        let idx = ((bits >> (2 * i)) & 0x3) as usize;
        *texel = palette[idx];
    }

    match (alpha, alpha_block) {
        (AlphaKind::Explicit4, Some(ab)) => {
            // Four bits per texel, low nibble first, expanded by replication: `a | (a << 4)`.
            for (i, texel) in out.iter_mut().enumerate() {
                let byte = ab[i / 2];
                let nibble = if i % 2 == 0 { byte & 0x0F } else { byte >> 4 };
                texel[3] = nibble | (nibble << 4);
            }
        }
        (AlphaKind::Interpolated, Some(ab)) => {
            let a0 = u32::from(ab[0]);
            let a1 = u32::from(ab[1]);
            let mut lut = [0u8; 8];
            #[allow(clippy::cast_possible_truncation)]
            {
                // LINT-OK: integer interpolation of 8-bit endpoints.
                lut[0] = a0 as u8;
                lut[1] = a1 as u8;
                if a0 > a1 {
                    for (i, slot) in lut.iter_mut().enumerate().take(8).skip(2) {
                        let k = i as u32 - 1; // 1..=6
                        *slot = (((7 - k) * a0 + k * a1) / 7) as u8;
                    }
                } else {
                    for (i, slot) in lut.iter_mut().enumerate().take(6).skip(2) {
                        let k = i as u32 - 1; // 1..=4
                        *slot = (((5 - k) * a0 + k * a1) / 5) as u8;
                    }
                    lut[6] = 0;
                    lut[7] = 255;
                }
            }
            // The 16 three-bit indices are a 48-bit little-endian field starting at byte 2.
            let mut idx_bits = 0u64;
            for (i, b) in ab[2..8].iter().enumerate() {
                idx_bits |= u64::from(*b) << (8 * i);
            }
            for (i, texel) in out.iter_mut().enumerate() {
                let idx = ((idx_bits >> (3 * i)) & 0x7) as usize;
                texel[3] = lut[idx];
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DXT1 block: two endpoints and a 32-bit index word.
    fn dxt1_block(c0: u16, c1: u16, indices: u32) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0..2].copy_from_slice(&c0.to_le_bytes());
        b[2..4].copy_from_slice(&c1.to_le_bytes());
        b[4..8].copy_from_slice(&indices.to_le_bytes());
        b
    }

    // Oracle: the S3TC/DXT1 block format uses 4-pixel blocks; the format table states that DXT1 is
    // 4 bits per pixel, DXT2-5 are 8). The endpoint values here are chosen so the expansion is
    // exact: 0xF800 is pure red, 0x001F pure blue.
    #[test]
    fn dxt1_endpoints_expand_by_bit_replication() {
        // index word 0b...00_01_10_11 for the first four texels.
        let block = dxt1_block(0xF800, 0x001F, 0b1110_0100);
        let px = decode(PixelFormatId::Dxt1, &block, 4, 4).unwrap();
        // BGRA order.
        assert_eq!(&px[0..4], &[0, 0, 255, 255], "texel 0 is endpoint 0 (red)");
        assert_eq!(&px[4..8], &[255, 0, 0, 255], "texel 1 is endpoint 1 (blue)");
        // c0 > c1, so the four-colour mode: texel 2 is (2*c0 + c1)/3.
        let third = |a: u32, b: u32| -> u8 { u8::try_from((2 * a + b) / 3).unwrap() };
        assert_eq!(&px[8..12], &[third(0, 255), 0, third(255, 0), 255]);
        assert_eq!(&px[12..16], &[third(255, 0), 0, third(0, 255), 255]);
    }

    // Oracle: the S3TC rule that a DXT1 block with c0 <= c1 carries one bit of alpha, and its fourth
    // palette entry is transparent black. This is the encoding behind the clip-map alpha reference
    // of 200.
    #[test]
    fn dxt1_with_c0_not_greater_than_c1_has_a_transparent_fourth_entry() {
        let block = dxt1_block(0x001F, 0xF800, 0b1110_0100);
        let px = decode(PixelFormatId::Dxt1, &block, 4, 4).unwrap();
        assert_eq!(&px[0..4], &[255, 0, 0, 255], "endpoint 0 is blue");
        assert_eq!(&px[4..8], &[0, 0, 255, 255], "endpoint 1 is red");
        // Midpoint, then transparent black.
        assert_eq!(&px[8..12], &[127, 0, 127, 255]);
        assert_eq!(&px[12..16], &[0, 0, 0, 0]);
        // Equal endpoints also take the three-colour branch.
        let block = dxt1_block(0x1234, 0x1234, 0xFFFF_FFFF);
        let px = decode(PixelFormatId::Dxt1, &block, 4, 4).unwrap();
        assert_eq!(&px[0..4], &[0, 0, 0, 0], "index 3 is transparent");
    }

    // Oracle: the DXT3 explicit-alpha encoding -- four bits per texel, low nibble first, expanded by
    // replication so 0xF becomes 255 and 0x0 becomes 0.
    #[test]
    fn dxt3_alpha_is_four_bits_per_texel_expanded_by_replication() {
        let mut block = [0u8; 16];
        // Texels 0..3: alpha nibbles F, 0, 8, 1.
        block[0] = 0x0F;
        block[1] = 0x18;
        block[8..16].copy_from_slice(&dxt1_block(0xFFFF, 0x0000, 0));
        let px = decode(PixelFormatId::Dxt3, &block, 4, 4).unwrap();
        assert_eq!(px[3], 255);
        assert_eq!(px[7], 0);
        assert_eq!(px[11], 0x88);
        assert_eq!(px[15], 0x11);
        // DXT2 shares the encoding (only the premultiplication convention differs), which is why
        // PixelFormatDesc gives them the same descriptor.
        let px2 = decode(PixelFormatId::Dxt2, &block, 4, 4).unwrap();
        assert_eq!(px, px2);
        // The colour half still uses the four-colour rule even though c0 > c1 is the only case:
        // for DXT2-5 the three-colour branch does not exist.
        let mut block = [0u8; 16];
        block[0..8].fill(0xFF);
        block[8..16].copy_from_slice(&dxt1_block(0x0000, 0xFFFF, 0xFFFF_FFFF));
        let px = decode(PixelFormatId::Dxt3, &block, 4, 4).unwrap();
        assert_eq!(
            px[3], 255,
            "alpha comes from the alpha block, never from index 3"
        );
        assert_ne!(
            &px[0..3],
            &[0, 0, 0],
            "index 3 is an interpolated colour, not transparent black"
        );
    }

    // Oracle: the DXT5 interpolated-alpha encoding -- eight entries, the 6-value form with an
    // explicit 0 and 255 when a0 <= a1.
    #[test]
    fn dxt5_alpha_uses_both_interpolation_modes() {
        let colour = dxt1_block(0xFFFF, 0x0000, 0);
        // a0 = 255 > a1 = 0: eight evenly spaced values, k/7.
        let mut block = [0u8; 16];
        block[0] = 255;
        block[1] = 0;
        // indices 0,1,2,3,4,5,6,7 for the first eight texels: 3 bits each in a 48-bit LE field.
        let mut idx: u64 = 0;
        for i in 0..8u64 {
            idx |= i << (3 * i);
        }
        block[2..8].copy_from_slice(&idx.to_le_bytes()[..6]);
        block[8..16].copy_from_slice(&colour);
        let px = decode(PixelFormatId::Dxt5, &block, 4, 4).unwrap();
        let alphas: Vec<u8> = (0..8).map(|i| px[i * 4 + 3]).collect();
        let lerp = |k: u32| -> u8 { u8::try_from(((7 - k) * 255) / 7).unwrap() };
        assert_eq!(
            alphas,
            vec![255, 0, lerp(1), lerp(2), lerp(3), lerp(4), lerp(5), lerp(6)]
        );

        // a0 = 0 <= a1 = 255: six interpolated values plus explicit 0 and 255 at indices 6 and 7.
        let mut block = [0u8; 16];
        block[0] = 0;
        block[1] = 255;
        block[2..8].copy_from_slice(&idx.to_le_bytes()[..6]);
        block[8..16].copy_from_slice(&colour);
        let px = decode(PixelFormatId::Dxt5, &block, 4, 4).unwrap();
        let alphas: Vec<u8> = (0..8).map(|i| px[i * 4 + 3]).collect();
        assert_eq!(alphas[0], 0);
        assert_eq!(alphas[1], 255);
        assert_eq!(
            alphas[6], 0,
            "index 6 is an explicit 0 in the six-value mode"
        );
        assert_eq!(
            alphas[7], 255,
            "index 7 is an explicit 255 in the six-value mode"
        );
        // DXT4 shares the encoding with DXT5.
        assert_eq!(decode(PixelFormatId::Dxt4, &block, 4, 4).unwrap(), px);
    }

    // Oracle: the 4x4 block layout. A surface whose extent is not a multiple of four still occupies
    // whole blocks, as the image-byte calculation requires, and the surplus texels are dropped.
    #[test]
    fn partial_blocks_are_cropped_not_rejected() {
        let block = dxt1_block(0xF800, 0x001F, 0);
        let px = decode(PixelFormatId::Dxt1, &block, 3, 2).unwrap();
        assert_eq!(px.len(), 3 * 2 * 4);
        // Every texel is endpoint 0 (index word is all zeroes).
        for chunk in px.as_chunks::<4>().0 {
            assert_eq!(chunk, &[0, 0, 255, 255]);
        }
    }

    // Oracle: the brief's rule that a parser meets malformed input and must not panic.
    #[test]
    fn short_and_unsupported_input_is_an_error_not_a_panic() {
        let err = decode(PixelFormatId::Dxt1, &[0u8; 4], 4, 4).unwrap_err();
        assert!(matches!(
            err,
            RenderError::ShortSourceData {
                expected: 8,
                actual: 4,
                ..
            }
        ));
        let err = decode(PixelFormatId::A8R8G8B8, &[0u8; 64], 4, 4).unwrap_err();
        assert!(matches!(err, RenderError::UnsupportedFormat(_)));
        let err = decode(PixelFormatId::Dxt1, &[0u8; 8], 0, 4).unwrap_err();
        assert!(matches!(err, RenderError::BadDimensions { .. }));
    }

    // Oracle: an independent property rather than a restatement -- a block whose two endpoints are
    // identical must decode to that one colour at every index in the four-colour mode, because all
    // four palette entries collapse. This catches a swapped interpolation weight, which the
    // endpoint tests above cannot.
    #[test]
    fn identical_endpoints_collapse_the_whole_palette() {
        // Use DXT3 so the four-colour rule always applies even with c0 == c1.
        let mut block = [0u8; 16];
        block[0..8].fill(0xFF);
        block[8..16].copy_from_slice(&dxt1_block(0x8410, 0x8410, 0xFFFF_FFFF));
        let px = decode(PixelFormatId::Dxt3, &block, 4, 4).unwrap();
        let first = &px[0..4];
        for chunk in px.as_chunks::<4>().0 {
            assert_eq!(chunk, first);
        }
    }
}
