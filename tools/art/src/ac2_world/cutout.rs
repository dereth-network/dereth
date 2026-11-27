//! Cut-out pictures as AC1 draws them: every texel wholly there or wholly clear, and black where
//! it is clear.
//!
//! AC1 draws a cut-out surface by testing its picture's alpha and blending what passes as if its
//! color were already multiplied by its alpha, and makes the smaller levels a distant surface is
//! drawn with by averaging the picture's texels, color and alpha alike. AC1's own cut-outs are
//! clear and black wherever they are clear, so a smaller level's texel is the surface's color
//! times how much of it is there, which is what the blend expects. AC2's foliage pictures are soft
//! at their edges and pale where they are clear: averaged, the pale texels wash a distant leaf
//! out, the test throws away most of what is left, and what passes glints and comes and goes as
//! the eye moves. So each converted cut-out picture is made as AC1's are: a texel is there where
//! AC2's alpha is at least half ([`THERE`]), and clear and black elsewhere.
//!
//! A block-compressed picture becomes the one-bit-alpha block format, which holds exactly that:
//! a block wholly there keeps its colors unchanged, and a block with clear texels keeps its two
//! end colors and draws each texel there with the nearest of them and the color halfway between.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::{Decode as _, RenderSurface, Surface, SurfaceTexture};
use dereth_primitives::DataId;

use crate::ac2_world::art::{Ids, Records};

/// The alpha at which a texel of AC2's is there, of 255.
pub const THERE: u8 = 128;

/// The pixel formats a cut-out picture is read from and written in.
const DXT1: u32 = 0x3154_5844;
const DXT3: u32 = 0x3354_5844;
const DXT5: u32 = 0x3554_5844;
const A8R8G8B8: u32 = 21;

/// The surface kinds: a cut-out, and the kinds that blend it some other way.
const CLIP_MAP: u32 = 0x4;
const BLENDS: u32 = 0x10 | 0x100 | 0x200 | 0x1_0000;

/// What was made.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CutOut {
    /// Pictures made over in place, and pictures copied for the cut-outs among their surfaces.
    pub made: usize,
    pub copied: usize,
}

/// Whether a surface of kind `surface_type` is a cut-out and nothing more.
#[must_use]
pub const fn is_cut_out(surface_type: u32) -> bool {
    surface_type & CLIP_MAP != 0 && surface_type & BLENDS == 0
}

/// Every cut-out surface among `out`'s whose picture `out` holds, its picture made as AC1's are.
/// A picture that other surfaces draw too is copied for the cut-outs, and theirs is left alone.
///
/// # Errors
/// A record of `out`'s that will not read.
pub fn cut_out_pictures(out: &mut Records, ids: &mut Ids) -> Result<CutOut, String> {
    let mut users: BTreeMap<DataId, Vec<DataId>> = BTreeMap::new();
    let mut shared: BTreeSet<DataId> = BTreeSet::new();
    for (id, bytes) in &out.0 {
        if id.0 >> 24 != 0x08 {
            continue;
        }
        let s = Surface::decode_payload(*id, bytes).map_err(|e| format!("{id:?}: {e}"))?;
        let Some(t) = s.orig_texture_id.filter(|t| out.0.contains_key(t)) else {
            continue;
        };
        if is_cut_out(s.surface_type) {
            users.entry(t).or_default().push(*id);
        } else {
            shared.insert(t);
        }
    }
    let mut made = CutOut::default();
    for (texture, surfaces) in users {
        let t = SurfaceTexture::decode_payload(texture, &out.0[&texture])
            .map_err(|e| format!("{texture:?}: {e}"))?;
        let mut levels = Vec::with_capacity(t.source_levels.len());
        for level in &t.source_levels {
            let Some(bytes) = out.0.get(level) else {
                levels.push(None);
                continue;
            };
            let rs = RenderSurface::decode_payload(*level, bytes)
                .map_err(|e| format!("{level:?}: {e}"))?;
            let px = bytes
                .get(rs.data_offset..rs.data_offset + rs.image_size as usize)
                .ok_or_else(|| format!("{level:?}: its pixels are short"))?;
            levels.push(picture(rs.format, rs.width, rs.height, px).map(|(f, p)| (rs, f, p)));
        }
        if levels.iter().all(Option::is_none) {
            continue;
        }
        let keep = shared.contains(&texture);
        let mut new_levels = Vec::with_capacity(levels.len());
        for (level, made_level) in t.source_levels.iter().zip(levels) {
            let Some((mut rs, format, px)) = made_level else {
                new_levels.push(*level);
                continue;
            };
            rs.id = if keep { ids.next(0x06) } else { *level };
            rs.format = format;
            rs.image_size = u32::try_from(px.len()).unwrap_or(u32::MAX);
            out.add(rs.id, rs.encode(&px));
            new_levels.push(rs.id);
        }
        if keep {
            let copy = SurfaceTexture {
                id: ids.next(0x05),
                source_levels: new_levels,
                ..t
            };
            for s in surfaces {
                let mut surface =
                    Surface::decode_payload(s, &out.0[&s]).map_err(|e| format!("{s:?}: {e}"))?;
                surface.orig_texture_id = Some(copy.id);
                out.add(s, surface.encode());
            }
            out.add(copy.id, copy.encode());
            made.copied += 1;
        } else {
            made.made += 1;
        }
    }
    Ok(made)
}

/// A picture of `format` as a cut-out: its format and pixels; `None` for a format it is not made
/// from, or one already made so (the one-bit-alpha blocks).
#[must_use]
pub fn picture(format: u32, width: u32, height: u32, px: &[u8]) -> Option<(u32, Vec<u8>)> {
    let blocks = (width.div_ceil(4) * height.div_ceil(4)) as usize;
    match format {
        DXT3 | DXT5 => {
            let src = px.get(..blocks * 16)?;
            let mut o = Vec::with_capacity(blocks * 8);
            for b in src.as_chunks::<16>().0 {
                let alpha = if format == DXT3 {
                    dxt3_alpha(b)
                } else {
                    dxt5_alpha(b)
                };
                let there = alpha.map(|a| a >= THERE);
                let color: [u8; 8] = b[8..].try_into().ok()?;
                o.extend_from_slice(&one_bit_block(color, there));
            }
            Some((DXT1, o))
        }
        A8R8G8B8 => {
            let n = (width * height) as usize;
            let src = px.get(..n * 4)?;
            Some((
                A8R8G8B8,
                src.as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|t| {
                        if t[3] >= THERE {
                            [t[0], t[1], t[2], 255]
                        } else {
                            [0; 4]
                        }
                    })
                    .collect(),
            ))
        }
        _ => None,
    }
}

/// A block's 16 alphas, of 255, from its explicit four-bit alphas.
fn dxt3_alpha(b: &[u8; 16]) -> [u8; 16] {
    std::array::from_fn(|i| {
        let nibble = (b[i / 2] >> (4 * (i % 2))) & 0xF;
        nibble * 17
    })
}

/// A block's 16 alphas, of 255, from its two end alphas and three-bit indices between them.
fn dxt5_alpha(b: &[u8; 16]) -> [u8; 16] {
    let (a0, a1) = (u32::from(b[0]), u32::from(b[1]));
    let steps: [u32; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
    let table: [u32; 8] = if a0 > a1 {
        steps.map(|k| match k {
            0 => a0,
            1 => a1,
            _ => ((8 - k) * a0 + (k - 1) * a1) / 7,
        })
    } else {
        steps.map(|k| match k {
            0 => a0,
            1 => a1,
            6 => 0,
            7 => 255,
            _ => ((6 - k) * a0 + (k - 1) * a1) / 5,
        })
    };
    let bits = b[2..8]
        .iter()
        .rev()
        .fold(0u64, |acc, &x| (acc << 8) | u64::from(x));
    std::array::from_fn(|i| {
        // LINT-OK: a three-bit index into the eight alphas.
        let k = ((bits >> (3 * i)) & 7) as usize;
        u8::try_from(table[k]).unwrap_or(255)
    })
}

/// A 5-6-5 color as eight bits a channel.
fn rgb(c: u16) -> [i32; 3] {
    let (r, g, b) = (
        i32::from(c >> 11),
        i32::from((c >> 5) & 0x3F),
        i32::from(c & 0x1F),
    );
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

/// A block's colors (two 5-6-5 end colors and two-bit indices, the four-color layout) as a
/// one-bit-alpha block whose texels `there` keep their colors and the rest are clear.
fn one_bit_block(color: [u8; 8], there: [bool; 16]) -> [u8; 8] {
    let c0 = u16::from_le_bytes([color[0], color[1]]);
    let c1 = u16::from_le_bytes([color[2], color[3]]);
    let idx = u32::from_le_bytes([color[4], color[5], color[6], color[7]]);
    let at = |i: usize| (idx >> (2 * i)) & 3;
    let block = |c0: u16, c1: u16, idx: u32| {
        let mut o = [0u8; 8];
        o[..2].copy_from_slice(&c0.to_le_bytes());
        o[2..4].copy_from_slice(&c1.to_le_bytes());
        o[4..].copy_from_slice(&idx.to_le_bytes());
        o
    };
    if there.iter().all(|&t| !t) {
        // Clear throughout: the third color of the three-color layout is clear black.
        return block(0, 0, u32::MAX);
    }
    if there.iter().all(|&t| t) {
        // Wholly there: the four-color layout needs its first end color the greater.
        return match c0.cmp(&c1) {
            std::cmp::Ordering::Greater => color,
            std::cmp::Ordering::Less => {
                let swapped = (0..16).fold(0u32, |acc, i| {
                    acc | ([1, 0, 3, 2][at(i) as usize] << (2 * i))
                });
                block(c1, c0, swapped)
            }
            std::cmp::Ordering::Equal => block(c0, c1, 0),
        };
    }
    // Some texels clear: the three-color layout (first end color not the greater), its third
    // color clear black. Each texel there takes whichever of the two ends and their middle is
    // nearest the color it had.
    let (p0, p1) = (rgb(c0), rgb(c1));
    let had: [[i32; 3]; 4] = [
        p0,
        p1,
        std::array::from_fn(|k| (2 * p0[k] + p1[k]) / 3),
        std::array::from_fn(|k| (p0[k] + 2 * p1[k]) / 3),
    ];
    let (lo, hi) = if c0 <= c1 { (c0, c1) } else { (c1, c0) };
    let (q0, q1) = (rgb(lo), rgb(hi));
    let now: [[i32; 3]; 3] = [q0, q1, std::array::from_fn(|k| (q0[k] + q1[k]) / 2)];
    let mut out = 0u32;
    for (i, &t) in there.iter().enumerate() {
        let k = if t {
            let want = had[at(i) as usize];
            let d = |p: &[i32; 3]| (0..3).map(|k| (p[k] - want[k]).pow(2)).sum::<i32>();
            // The first of the nearest, so a tie is settled the same everywhere.
            (0..3u32)
                .min_by_key(|&k| (d(&now[k as usize]), k))
                .unwrap_or(0)
        } else {
            3
        };
        out |= k << (2 * i);
    }
    block(lo, hi, out)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's cut-out pictures)
    use super::*;

    /// Decode one-bit-alpha block `b` as the format reads: each texel's color and whether it is
    /// there.
    fn read_one_bit(b: &[u8]) -> [([i32; 3], bool); 16] {
        let c0 = u16::from_le_bytes([b[0], b[1]]);
        let c1 = u16::from_le_bytes([b[2], b[3]]);
        let idx = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        let (p0, p1) = (rgb(c0), rgb(c1));
        std::array::from_fn(|i| match (idx >> (2 * i)) & 3 {
            0 => (p0, true),
            1 => (p1, true),
            2 if c0 > c1 => (std::array::from_fn(|k| (2 * p0[k] + p1[k]) / 3), true),
            2 => (std::array::from_fn(|k| (p0[k] + p1[k]) / 2), true),
            _ if c0 > c1 => (std::array::from_fn(|k| (p0[k] + 2 * p1[k]) / 3), true),
            _ => ([0; 3], false),
        })
    }

    /// A four-bit-alpha block: alpha `a(i)` (0 to 15) for texel `i`, end colors `c0`, `c1` and
    /// indices `idx`.
    fn dxt3_block(a: impl Fn(usize) -> u8, c0: u16, c1: u16, idx: u32) -> [u8; 16] {
        let mut b = [0u8; 16];
        for i in 0..16 {
            b[i / 2] |= (a(i) & 0xF) << (4 * (i % 2));
        }
        b[8..10].copy_from_slice(&c0.to_le_bytes());
        b[10..12].copy_from_slice(&c1.to_le_bytes());
        b[12..].copy_from_slice(&idx.to_le_bytes());
        b
    }

    #[test]
    fn a_texel_at_least_half_there_stays_and_the_rest_are_clear_and_black() {
        // Alpha 0, 7 (119 of 255), 8 (136) and 15 across the block's texels.
        let alphas = [0u8, 7, 8, 15];
        let b = dxt3_block(|i| alphas[i % 4], 0xF800, 0x07E0, 0x1B1B_1B1B);
        let (format, px) = picture(DXT3, 4, 4, &b).expect("made");
        assert_eq!(format, DXT1);
        assert_eq!(px.len(), 8);
        for (i, (color, there)) in read_one_bit(&px).iter().enumerate() {
            assert_eq!(*there, alphas[i % 4] >= 8, "texel {i}");
            if !there {
                assert_eq!(*color, [0; 3], "a clear texel is black");
            }
        }
    }

    #[test]
    fn a_block_wholly_there_keeps_every_texels_color() {
        // The first end color the lesser: kept by swapping the ends and their indices.
        for (c0, c1) in [(0xF800u16, 0x001F), (0x001F, 0xF800), (0x1234, 0x1234)] {
            let idx = 0xE4E4_E4E4; // 0, 1, 2, 3 in turn.
            let b = dxt3_block(|_| 15, c0, c1, idx);
            let (_, px) = picture(DXT3, 4, 4, &b).expect("made");
            let (p0, p1) = (rgb(c0), rgb(c1));
            let four: [[i32; 3]; 4] = [
                p0,
                p1,
                std::array::from_fn(|k| (2 * p0[k] + p1[k]) / 3),
                std::array::from_fn(|k| (p0[k] + 2 * p1[k]) / 3),
            ];
            for (i, (color, there)) in read_one_bit(&px).iter().enumerate() {
                assert!(there);
                let want = four[((idx >> (2 * i)) & 3) as usize];
                // The swapped ends read the same in either order, to a step of rounding.
                assert!(
                    (0..3).all(|k| (color[k] - want[k]).abs() <= 1),
                    "{c0:04X} {c1:04X} texel {i}: {color:?} for {want:?}"
                );
            }
        }
    }

    #[test]
    fn a_block_with_clear_texels_keeps_its_end_colors_for_the_texels_there() {
        let b = dxt3_block(|i| if i < 8 { 15 } else { 0 }, 0x001F, 0xF800, 0x0000_5500);
        let (_, px) = picture(DXT3, 4, 4, &b).expect("made");
        let read = read_one_bit(&px);
        for (color, there) in &read[..4] {
            assert!(there);
            assert_eq!(*color, rgb(0x001F));
        }
        for (color, there) in &read[4..8] {
            assert!(there);
            assert_eq!(*color, rgb(0xF800));
        }
        assert!(read[8..].iter().all(|(c, t)| !t && *c == [0; 3]));
    }

    #[test]
    fn an_interpolated_alpha_block_is_read_as_the_format_reads_it() {
        // Ends 255 and 0, eight steps: index 0 is 255, 1 is 0, 2 to 7 between, falling.
        let mut b = [0u8; 16];
        b[0] = 255;
        b[1] = 0;
        let idx: u64 = (0..16).fold(0, |acc, i| acc | (((i % 8) as u64) << (3 * i)));
        b[2..8].copy_from_slice(&idx.to_le_bytes()[..6]);
        let a = dxt5_alpha(&b);
        assert_eq!(&a[..8], &[255, 0, 218, 182, 145, 109, 72, 36]);
        let there = a.map(|v| v >= THERE);
        assert_eq!(there.iter().filter(|&&t| t).count(), 2 * 4);
    }

    #[test]
    fn a_full_color_picture_is_clear_and_black_below_half() {
        let px = [10, 20, 30, 127, 10, 20, 30, 128];
        let (format, out) = picture(A8R8G8B8, 2, 1, &px).expect("made");
        assert_eq!(format, A8R8G8B8);
        assert_eq!(out, vec![0, 0, 0, 0, 10, 20, 30, 255]);
        assert!(picture(DXT1, 4, 4, &[0; 8]).is_none(), "already one bit");
    }
}
