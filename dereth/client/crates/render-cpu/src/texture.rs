//! Texture decode and the format remap.
//!
//! Palette and surface records are described in `docs/formats/13-palette-and-surfaces.md`. This
//! module preserves surface creation, format selection and image copying.
//!
//! Everything here produces `dereth_primitives::TextureData`, whose `Bgra8` is D3D9's `A8R8G8B8` in memory
//! order (B, G, R, A bytes) — the format the client's `X8R8G8B8`/`A8R8G8B8` surfaces already are.

use dereth_primitives::{TextureData, TextureFormat};

use crate::palette::ExpandedPalette;
use crate::pixel_format::{is_d3d_format, PixelFormatDesc, PixelFormatId};
use crate::RenderError;

/// A source image, tagged by how it must be decoded.
#[derive(Debug, Clone, Copy)]
pub enum SourcePixels<'a> {
    /// A format `is_d3d_format` accepts: the client memcpys it row by row and hands it to D3D.
    Direct {
        format: PixelFormatId,
        bits: &'a [u8],
        pitch: usize,
    },
    /// `PFID_P8` — 8-bit palettised.
    Palettised8 {
        indices: &'a [u8],
        palette: &'a ExpandedPalette,
        clip_map: bool,
    },
    /// `PFID_INDEX16` — 16-bit palettised, indexing straight into the 2048-entry table.
    Palettised16 {
        indices: &'a [u16],
        palette: &'a ExpandedPalette,
        clip_map: bool,
    },
    /// `PFID_DXT1..DXT5` — copied verbatim; the GPU decodes them.
    Dxt {
        format: PixelFormatId,
        blocks: &'a [u8],
    },
    /// `PFID_CUSTOM_RAW_JPEG`.
    Jpeg(&'a [u8]),
    /// `PFID_CUSTOM_LSCAPE_R8G8B8` — 24-bit BGR-ordered landscape texture.
    LandscapeRgb(&'a [u8]),
    /// `PFID_CUSTOM_LSCAPE_ALPHA` — 8-bit alpha-only landscape mask.
    LandscapeAlpha(&'a [u8]),
}

/// The subset of the client's device capabilities and display information that the surface- and
/// texture-format selection read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// Whether compressed textures are supported, set at device start-up. If **any** of DXT1-5
    /// is rejected the flag is false for all of them.
    pub compressed_textures: bool,
    /// The display's RGB texture format.
    pub pf_rgb_textures: PixelFormatId,
    /// The display's ARGB texture format.
    pub pf_argb_textures: PixelFormatId,
    /// The display's alpha texture format.
    pub pf_alpha_textures: PixelFormatId,
    /// The display's ARGB surface format, which is what the format query returns. On a 32-bit
    /// desktop this is `A8R8G8B8`.
    pub pf_argb_surfaces: PixelFormatId,
    /// Destination alpha — cleared whenever the back buffer has no alpha channel, which is
    /// the normal case because the back buffer is `X8R8G8B8`.
    pub destination_alpha: bool,
    /// Simple non-power-of-two textures — **hard-coded false** in the client, which is why every
    /// UI surface is a power-of-two texture.
    pub simple_non_power_of_two_textures: bool,
    /// Automatic mip generation (`D3DCAPS2_CANAUTOGENMIPMAP`) — how a non-compressed texture gets
    /// its mip chain when the device uploads it.
    pub auto_gen_mipmaps: bool,
}

impl Default for Caps {
    /// The caps the retail client sees on any 32-bit desktop: DXT supported, ARGB textures
    /// `A8R8G8B8`, RGB textures `X8R8G8B8`, alpha textures `A8`, no destination alpha (the back
    /// buffer is `X8R8G8B8`), non-power-of-two textures unavailable, autogen mips available.
    fn default() -> Self {
        Self {
            compressed_textures: true,
            pf_rgb_textures: PixelFormatId::X8R8G8B8,
            pf_argb_textures: PixelFormatId::A8R8G8B8,
            pf_alpha_textures: PixelFormatId::A8,
            pf_argb_surfaces: PixelFormatId::A8R8G8B8,
            destination_alpha: false,
            simple_non_power_of_two_textures: false,
            auto_gen_mipmaps: true,
        }
    }
}

/// Data category 6 is the UI category. Every UI surface is forced to the ARGB surface format
/// so the window-surface blitter and the UI's alpha compositing work.
pub const DATA_CATEGORY_UI: u32 = 6;
/// Data category 10 — conversion is suppressed entirely.
///
/// which dat category 10 *is*, and therefore which
/// surfaces keep their authored format, was not traced. The client branches on exactly 6 and
/// 10, and so do this constant and [`select_surface_format`].
pub const DATA_CATEGORY_UNCONVERTED: u32 = 10;

/// The surface-format selection, verbatim:
///
/// ```text
/// src not a D3D format                     -> src                  // keep P8/INDEX16/LSCAPE
/// data category 6                          -> pf_argb_surfaces     // the UI surface format
/// data category 10                         -> src                  // untouched category
/// src compressed, no compressed support    -> pf_argb_textures
/// src compressed                           -> src
/// src has alpha and RGB                    -> pf_argb_textures
/// src has alpha only                       -> pf_alpha_textures
/// src has RGB only                         -> pf_rgb_textures
/// otherwise                                -> src
/// ```
///
/// The whole function runs only when the database cache is in runtime mode, a renderer exists, and
/// a format-conversion global is set (it is **true**); a caller outside those conditions
/// keeps the source format, which is the identity this function does not model.
#[must_use]
pub fn select_surface_format(src: PixelFormatId, data_category: u32, caps: &Caps) -> PixelFormatId {
    if !is_d3d_format(src) {
        return src;
    }
    if data_category == DATA_CATEGORY_UI {
        return caps.pf_argb_surfaces;
    }
    if data_category == DATA_CATEGORY_UNCONVERTED {
        return src;
    }
    let Some(desc) = PixelFormatDesc::for_format(src) else {
        // SetFormat returned false; there is nothing to convert to.
        return src;
    };
    if desc.is_compressed() {
        return if caps.compressed_textures {
            src
        } else {
            caps.pf_argb_textures
        };
    }
    if desc.has_alpha() && desc.has_rgb() {
        return caps.pf_argb_textures;
    }
    if desc.has_alpha() {
        return caps.pf_alpha_textures;
    }
    if desc.has_rgb() {
        return caps.pf_rgb_textures;
    }
    src
}

/// The *texture*-side remap, which differs
/// from the surface one: it has no data-category cases and leaves compressed formats alone.
///
/// | Source `PixelFormatDesc.flags` | Becomes |
/// |---|---|
/// | RGB and alpha | [`Caps::pf_argb_textures`] |
/// | alpha only | [`Caps::pf_alpha_textures`] |
/// | RGB only | [`Caps::pf_rgb_textures`] |
/// | compressed | unchanged (DXT stays DXT) |
#[must_use]
pub fn select_texture_format(src: PixelFormatId, caps: &Caps) -> PixelFormatId {
    let Some(desc) = PixelFormatDesc::for_format(src) else {
        return src;
    };
    if desc.is_compressed() {
        return src;
    }
    if desc.has_rgb() && desc.has_alpha() {
        caps.pf_argb_textures
    } else if desc.has_alpha() {
        caps.pf_alpha_textures
    } else if desc.has_rgb() {
        caps.pf_rgb_textures
    } else {
        src
    }
}

/// The power-of-two rounder the UI uses:
/// `0 → 0`, `>= 2048 → 2048`, otherwise the next power of two at or above the value.
///
/// Because [`Caps::simple_non_power_of_two_textures`] is hard-coded false, **every UI surface is a
/// power-of-two texture** with the logical content in its top-left corner. That is *why* the UV rule
/// in [`crate::ui`] has its `−1`s.
#[must_use]
pub fn best_width_height(v: u32) -> u32 {
    if v == 0 {
        0
    } else if v >= 2048 {
        2048
    } else {
        v.next_power_of_two()
    }
}

/// `ImageScaleType` — the value is a **right shift count** applied to width and height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageScale {
    #[default]
    FullRes = 0,
    HalfRes = 1,
    QuarterRes = 2,
    EighthRes = 3,
}

/// The floor on a downscale -- a downscaled texture is never smaller than 8 on a side
/// unless the source already is.
pub const MIN_TEX_SIZE: u32 = 8;

/// The scaled dimensions the client computes: `w = width >> scale` and `h = height >> scale`;
/// unless the scale is `FULL_RES`, a `w` below [`MIN_TEX_SIZE`] becomes
/// `min(MIN_TEX_SIZE, width)`, and likewise `h` with the height.
#[must_use]
pub fn scaled_dimensions(width: u32, height: u32, scale: ImageScale) -> (u32, u32) {
    let shift = scale as u32;
    let mut w = width >> shift;
    let mut h = height >> shift;
    if scale != ImageScale::FullRes {
        if w < MIN_TEX_SIZE {
            w = MIN_TEX_SIZE.min(width);
        }
        if h < MIN_TEX_SIZE {
            h = MIN_TEX_SIZE.min(height);
        }
    }
    (w, h)
}

/// The texture-scale selector's three sources, as the preference update writes them:
/// `Render.EnvironmentTextureDetail` becomes `max(detail, 1) - 1` and lands in **all three** of
/// the clip-map, RGBA and indexed texture scales in one go. A detail of 0 selects full
/// resolution; any other value shifts by `detail - 1`. When the preference has not changed since
/// the last pass nothing happens at all; when it has, the graphics resources are flushed as well.
///
/// `RenderPreferences::image_scale` is the same arithmetic on the client side of
/// the seam; this is the enum it selects.
#[must_use]
pub const fn image_scale_from_shift(shift: u32) -> ImageScale {
    match shift {
        0 => ImageScale::FullRes,
        1 => ImageScale::HalfRes,
        2 => ImageScale::QuarterRes,
        _ => ImageScale::EighthRes,
    }
}

/// Apply the current texture scale to an already-decoded surface: the image that reaches the
/// device is `scaled_dimensions(w, h, scale)` rather than the source extent.
///
/// This is the consumer of `Render.EnvironmentTextureDetail`: without it [`scaled_dimensions`]
/// would be correct but unused, and the preference would change nothing on screen.
///
/// **The shift is a shift, and it applies to every format.** The client takes the current
/// texture scale as a shift count and shifts *both* the source width and the source height
/// by it, with no format test in between,
///
/// so a block-compressed surface is downscaled here too, and this does the same: the uncompressed
/// arm is a repeated [`crate::mip::box_filter_bgra8`] halving and the compressed arm is a repeated
/// `crate::d3dx_bc` halving — the **same** two codecs
/// [`crate::mip::compressed_system_chain`] already builds this build's mip chains with, so a
/// scaled level 0 is bit-identical to the mip level it replaces.
///
/// Only level 0 comes back. The device resource is re-created at the shifted
/// extent, after which the client's high-detail drop -- which drops the top
/// `EnvironmentTextureDetail` levels of a **multi-level** source, or nothing at detail 0 -- has
/// nothing left to
/// drop, exactly as it has nothing to drop for the single-level surfaces this dat ships
/// (the drop returns immediately on a one-level chain). `upload_imgtex_keyed` rebuilds
/// whatever levels the device wants below this seam.
///
/// # Errors
/// [`RenderError::ShortSourceData`] when a level is smaller than its declared extent requires, and
/// [`RenderError::Unsupported`] for a format whose halving codec this build does not have.
pub fn scale_surface(t: &TextureData, scale: ImageScale) -> Result<TextureData, RenderError> {
    if scale == ImageScale::FullRes {
        return Ok(t.clone());
    }
    let (tw, th) = scaled_dimensions(t.width, t.height, scale);
    if (tw, th) == (t.width, t.height) {
        return Ok(t.clone());
    }
    let Some(level0) = t.levels.first() else {
        return Ok(t.clone());
    };
    let (mut bytes, mut w, mut h) = (level0.clone(), t.width, t.height);
    // `w > tw || h > th` and not a fixed shift count: `scaled_dimensions`' minimum-texture-size
    // floor stops a small source short of the full shift, and halving past the floor
    // would undershoot it.
    while w > tw || h > th {
        let (nw, nh) = (crate::mip::half(w), crate::mip::half(h));
        if (nw, nh) == (w, h) {
            break; // 1x1 cannot halve further.
        }
        bytes = match t.format {
            TextureFormat::Bgra8 => crate::mip::box_filter_bgra8(&bytes, w, h)?.0,
            TextureFormat::Bc1 => crate::d3dx_bc::half_bc1(&bytes, w, h)?,
            TextureFormat::Bc2 => crate::d3dx_bc::half_bc2(&bytes, w, h)?,
            TextureFormat::Bc3 => crate::d3dx_bc::half_bc3(&bytes, w, h)?,
            TextureFormat::Bc2Premultiplied => {
                crate::d3dx_bc::half_bc2_premultiplied(&bytes, w, h)?
            }
            TextureFormat::Bc3Premultiplied => {
                crate::d3dx_bc::half_bc3_premultiplied(&bytes, w, h)?
            }
            // `dereth_primitives::TextureFormat` is `#[non_exhaustive]`; a format with no halving codec
            // is a change to that shared type and not something to guess at. Returning the source unscaled
            // would be a silent hole in the preference.
            _ => {
                return Err(RenderError::Unsupported(
                    "no halving codec for this texture format",
                ))
            }
        };
        w = nw;
        h = nh;
    }
    Ok(TextureData {
        width: w,
        height: h,
        format: t.format,
        levels: vec![bytes],
    })
}

/// Decode one source image into something the GPU accepts.
///
/// The block-compressed arm passes the blocks through untouched, which is what
/// the retail texture loader does ("copied verbatim ... no row loop, because DXT rows are 4-pixel
/// blocks"). Everything else lands as BGRA8.
///
/// # Errors
/// [`RenderError::ShortSourceData`] when the payload is smaller than the dimensions require,
/// [`RenderError::UnsupportedFormat`] for a format the format decoder rejects, and
/// [`RenderError::Jpeg`] for a stream the client itself would refuse.
pub fn decode_surface(
    src: SourcePixels<'_>,
    width: u32,
    height: u32,
) -> Result<TextureData, RenderError> {
    // **`PFID_CUSTOM_RAW_JPEG` is the one format whose header carries no extent**, so its arm runs
    // before the zero-extent check and answers with the stream's own dimensions.
    //
    // In the JPEG variant described in [`docs/formats/14-textures.md`], `width`, `height` and
    // `bitsPerPixel` are **0 in the header** — they come from the JPEG. The record's width and
    // height fields are documented as "0 for `PFID_CUSTOM_RAW_JPEG`". The client agrees:
    // it sets the decoded bitmap width and height **from the
    // JPEG** rather than from the record. All 79 shipped JPEG surfaces have a 0×0 header, so
    // requiring the header to match the stream rejected every one of them — and requiring a
    // non-zero extent rejected them before the arm was even reached.
    //
    // `0x06007576`, a character-management background, is one such JPEG surface.
    if let SourcePixels::Jpeg(bytes) = src {
        let img = crate::jpeg::decode(bytes)?;
        // A header that *does* carry an extent must still agree with the stream; the client would
        // create a surface of one size and blit another into it.
        if (width != 0 && width != img.width) || (height != 0 && height != img.height) {
            return Err(RenderError::Jpeg(format!(
                "stream is {}x{}; the surface header says {width}x{height}",
                img.width, img.height
            )));
        }
        let n = img.width as usize * img.height as usize;
        // The decode lands as 24-bit BGR (PFID_R8G8B8); widen to BGRA with an opaque alpha.
        let mut out = vec![0u8; n * 4];
        for (i, px) in img.bgr.as_chunks::<3>().0.iter().enumerate() {
            out[i * 4] = px[0];
            out[i * 4 + 1] = px[1];
            out[i * 4 + 2] = px[2];
            out[i * 4 + 3] = 0xFF;
        }
        return Ok(TextureData {
            width: img.width,
            height: img.height,
            format: TextureFormat::Bgra8,
            levels: vec![out],
        });
    }

    if width == 0 || height == 0 {
        return Err(RenderError::BadDimensions {
            width,
            height,
            reason: "a surface must have a non-zero extent",
        });
    }
    let n = width as usize * height as usize;

    let (format, bits) = match src {
        SourcePixels::Dxt { format, blocks } => {
            let desc = PixelFormatDesc::for_format(format)
                .ok_or(RenderError::UnsupportedFormat(format))?;
            if !desc.is_compressed() {
                return Err(RenderError::UnsupportedFormat(format));
            }
            let expected = desc.image_bytes(width, height);
            if blocks.len() < expected {
                return Err(RenderError::ShortSourceData {
                    format,
                    width,
                    height,
                    expected,
                    actual: blocks.len(),
                });
            }
            let tf = match format {
                PixelFormatId::Dxt1 => TextureFormat::Bc1,
                PixelFormatId::Dxt2 => TextureFormat::Bc2Premultiplied,
                PixelFormatId::Dxt3 => TextureFormat::Bc2,
                PixelFormatId::Dxt4 => TextureFormat::Bc3Premultiplied,
                PixelFormatId::Dxt5 => TextureFormat::Bc3,
                _ => unreachable!("compressed source descriptor only accepts DXT1-5"),
            };
            return Ok(TextureData {
                width,
                height,
                format: tf,
                levels: vec![blocks[..expected].to_vec()],
            });
        }
        // Handled above: a JPEG surface's extent comes from the stream, not from the header.
        SourcePixels::Jpeg(_) => unreachable!("the JPEG arm returns before the format match"),
        SourcePixels::Palettised8 {
            indices,
            palette,
            clip_map,
        } => {
            if indices.len() < n {
                return Err(RenderError::ShortSourceData {
                    format: PixelFormatId::P8,
                    width,
                    height,
                    expected: n,
                    actual: indices.len(),
                });
            }
            let mut out = vec![0u8; n * 4];
            for (i, idx) in indices[..n].iter().enumerate() {
                let argb = palette.expand(u16::from(*idx), clip_map);
                out[i * 4..i * 4 + 4].copy_from_slice(&argb.to_le_bytes());
            }
            (TextureFormat::Bgra8, out)
        }
        SourcePixels::Palettised16 {
            indices,
            palette,
            clip_map,
        } => {
            if indices.len() < n {
                return Err(RenderError::ShortSourceData {
                    format: PixelFormatId::Index16,
                    width,
                    height,
                    expected: n * 2,
                    actual: indices.len() * 2,
                });
            }
            let mut out = vec![0u8; n * 4];
            for (i, idx) in indices[..n].iter().enumerate() {
                let argb = palette.expand(*idx, clip_map);
                out[i * 4..i * 4 + 4].copy_from_slice(&argb.to_le_bytes());
            }
            (TextureFormat::Bgra8, out)
        }
        SourcePixels::LandscapeRgb(bits) => {
            // PFID_CUSTOM_LSCAPE_R8G8B8 shares R8G8B8's descriptor: 24 bits, R = 0x00FF0000, so the
            // bytes are B, G, R. The loader expands it to X8R8G8B8.
            let expected = n * 3;
            if bits.len() < expected {
                return Err(RenderError::ShortSourceData {
                    format: PixelFormatId::CustomLscapeR8G8B8,
                    width,
                    height,
                    expected,
                    actual: bits.len(),
                });
            }
            let mut out = vec![0u8; n * 4];
            for (i, px) in bits[..expected].as_chunks::<3>().0.iter().enumerate() {
                out[i * 4] = px[0];
                out[i * 4 + 1] = px[1];
                out[i * 4 + 2] = px[2];
                out[i * 4 + 3] = 0xFF;
            }
            (TextureFormat::Bgra8, out)
        }
        SourcePixels::LandscapeAlpha(bits) => {
            // PFID_CUSTOM_LSCAPE_ALPHA has the same flags/bpp as PFID_A8: alpha only.
            if bits.len() < n {
                return Err(RenderError::ShortSourceData {
                    format: PixelFormatId::CustomLscapeAlpha,
                    width,
                    height,
                    expected: n,
                    actual: bits.len(),
                });
            }
            let mut out = vec![0u8; n * 4];
            for (i, a) in bits[..n].iter().enumerate() {
                out[i * 4 + 3] = *a;
            }
            (TextureFormat::Bgra8, out)
        }
        SourcePixels::Direct {
            format,
            bits,
            pitch,
        } => {
            let desc = PixelFormatDesc::for_format(format)
                .ok_or(RenderError::UnsupportedFormat(format))?;
            let row = desc.row_bytes(width);
            let pitch = if pitch == 0 { row } else { pitch };
            let expected = pitch * (height as usize - 1) + row;
            if bits.len() < expected {
                return Err(RenderError::ShortSourceData {
                    format,
                    width,
                    height,
                    expected,
                    actual: bits.len(),
                });
            }
            let mut out = vec![0u8; n * 4];
            for y in 0..height as usize {
                let src_row = &bits[y * pitch..y * pitch + row];
                for x in 0..width as usize {
                    let argb = read_pixel(&desc, src_row, x)?;
                    let d = (y * width as usize + x) * 4;
                    out[d..d + 4].copy_from_slice(&argb.to_le_bytes());
                }
            }
            (TextureFormat::Bgra8, out)
        }
    };

    Ok(TextureData {
        width,
        height,
        format,
        levels: vec![bits],
    })
}

/// The pixel unpack: read one pixel through the source
/// descriptor's masks and rescale each channel to 8 bits.
///
/// The client's rescale is "unpack with the source masks/offsets/max, rescale each channel to the
/// destination's max, repack". For an 8-bit destination that is `value * 255 / max`, which for the
/// power-of-two channel widths in the table is exactly bit replication.
fn read_pixel(desc: &PixelFormatDesc, row: &[u8], x: usize) -> Result<u32, RenderError> {
    let bpp = desc.bits_per_pixel;
    let raw: u32 = match bpp {
        8 => u32::from(row[x]),
        16 => u32::from(u16::from_le_bytes([row[x * 2], row[x * 2 + 1]])),
        24 => {
            let b = &row[x * 3..x * 3 + 3];
            u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16)
        }
        32 => u32::from_le_bytes([row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]]),
        _ => return Err(RenderError::UnsupportedFormat(desc.format)),
    };
    let chan = |mask: u32, default: u32| -> u32 {
        if mask == 0 {
            return default;
        }
        let shift = mask.trailing_zeros();
        let max = mask >> shift;
        let v = (raw & mask) >> shift;
        // v * 255 / max, exact for every width in the table.
        (v * 255 + max / 2) / max
    };
    let r = chan(desc.red_mask, 0);
    let g = chan(desc.green_mask, 0);
    let b = chan(desc.blue_mask, 0);
    // A format with no alpha channel reads as fully opaque, which is what an X8 surface means.
    let a = chan(desc.alpha_mask, 255);
    Ok((a << 24) | (r << 16) | (g << 8) | b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The JPEG variant in [`docs/formats/14-textures.md`] stores zero header `width`,
    /// `height` and `bitsPerPixel`; they come from the JPEG. This matches the field
    /// table's "0 for `PFID_CUSTOM_RAW_JPEG`" rows at +0x08 and +0x0C and the worked example
    /// `0x06005F18` ("header `width = 0, height = 0, format = 500`"). All 79 shipped JPEG surfaces
    /// have a 0×0 header.
    ///
    /// An arm that required the header to equal the stream, or ran only after a non-zero-extent
    /// check, would decode **no retail JPEG surface at all**. The live counterpart is
    /// `dereth_client::textures`'s `the_retail_jpeg_surface_the_character_screen_uses_decodes`,
    /// which reads `0x06007576` out of `client_portal.dat`.
    #[test]
    fn a_jpeg_surface_takes_its_extent_from_the_stream_because_the_header_carries_none() {
        let bytes = crate::jpeg::tests::bytes(crate::jpeg::tests::RED_8X8_BASELINE);
        let t = decode_surface(SourcePixels::Jpeg(&bytes), 0, 0)
            .expect("a 0x0 header is the shipped form and must decode");
        assert_eq!(
            (t.width, t.height),
            (8, 8),
            "the extent comes from the stream"
        );
        assert_eq!(t.format, TextureFormat::Bgra8);
        assert_eq!(t.levels[0].len(), 8 * 8 * 4);

        // A header that *does* carry an extent must still agree with the stream: the client would
        // otherwise create a surface of one size and blit another into it.
        assert!(decode_surface(SourcePixels::Jpeg(&bytes), 8, 8).is_ok());
        assert!(matches!(
            decode_surface(SourcePixels::Jpeg(&bytes), 16, 16),
            Err(RenderError::Jpeg(_))
        ));
    }

    fn ramp_palette() -> ExpandedPalette {
        let src: Vec<u32> = (0..256u32)
            .map(|i| 0xFF00_0000 | (i << 16) | (i << 8) | i)
            .collect();
        ExpandedPalette::from_dat(&src).unwrap()
    }

    // The surface-format selection is quoted line by line in the module docs
    // acceptance test: "select_surface_format reproduces both data-category special cases
    // (6 -> UI format, 10 -> untouched)".
    #[test]
    fn select_surface_format_reproduces_both_data_category_cases() {
        let caps = Caps::default();
        // Category 6 forces the UI surface format whatever the source is.
        for src in [
            PixelFormatId::R5G6B5,
            PixelFormatId::A8,
            PixelFormatId::X8R8G8B8,
            PixelFormatId::Dxt1,
        ] {
            assert_eq!(
                select_surface_format(src, 6, &caps),
                caps.pf_argb_surfaces,
                "{src:?}"
            );
        }
        // Category 10 suppresses conversion entirely.
        for src in [
            PixelFormatId::R5G6B5,
            PixelFormatId::A8,
            PixelFormatId::Dxt5,
        ] {
            assert_eq!(select_surface_format(src, 10, &caps), src, "{src:?}");
        }
        // The CPU-decoded formats short-circuit *before* both category cases, because the
        // not-a-D3D-format test is the first line of the function.
        for src in [
            PixelFormatId::P8,
            PixelFormatId::Index16,
            PixelFormatId::CustomLscapeR8G8B8,
            PixelFormatId::CustomLscapeAlpha,
        ] {
            assert_eq!(
                select_surface_format(src, 6, &caps),
                src,
                "{src:?} under category 6"
            );
            assert_eq!(
                select_surface_format(src, 0, &caps),
                src,
                "{src:?} under category 0"
            );
        }
    }

    // Oracle: the same transcription, the ordinary categories. Also exercises the compressed branch
    // and its no-compressed-support fallback, which is the only way a DXT source ever changes format.
    #[test]
    fn select_surface_format_maps_each_flag_combination() {
        let caps = Caps::default();
        // Every data_category value 0-10 that is neither 6 nor 10 takes the ordinary path; the dat
        // uses 0-10 across the 20 684 0x06 files (open questions #47, #95, #113).
        for cat in [0u32, 1, 2, 3, 4, 5, 7, 8, 9] {
            // RGB + alpha -> the device's ARGB texture format.
            assert_eq!(
                select_surface_format(PixelFormatId::A1R5G5B5, cat, &caps),
                caps.pf_argb_textures
            );
            // Alpha only -> the device's alpha texture format.
            assert_eq!(
                select_surface_format(PixelFormatId::A8, cat, &caps),
                caps.pf_alpha_textures
            );
            // RGB only -> the device's RGB texture format.
            assert_eq!(
                select_surface_format(PixelFormatId::R5G6B5, cat, &caps),
                caps.pf_rgb_textures
            );
            // Compressed stays compressed while the device supports it.
            assert_eq!(
                select_surface_format(PixelFormatId::Dxt3, cat, &caps),
                PixelFormatId::Dxt3
            );
        }
        // ..and falls back to the ARGB texture format when it does not.
        let no_dxt = Caps {
            compressed_textures: false,
            ..caps
        };
        assert_eq!(
            select_surface_format(PixelFormatId::Dxt1, 0, &no_dxt),
            no_dxt.pf_argb_textures
        );
    }

    // Check the observed four-row texture-format remap. It has no category cases, which is
    // the difference worth asserting.
    #[test]
    fn the_texture_side_remap_has_no_data_category_cases() {
        let caps = Caps::default();
        assert_eq!(
            select_texture_format(PixelFormatId::A8R8G8B8, &caps),
            caps.pf_argb_textures
        );
        assert_eq!(
            select_texture_format(PixelFormatId::R8G8B8, &caps),
            caps.pf_rgb_textures
        );
        assert_eq!(
            select_texture_format(PixelFormatId::A8, &caps),
            caps.pf_alpha_textures
        );
        assert_eq!(
            select_texture_format(PixelFormatId::Dxt1, &caps),
            PixelFormatId::Dxt1
        );
    }

    // Oracle: the client's indexed-image copy --
    //   idx = (src format == PFID_INDEX16) ? ((uint16*)srcRow)[x] : ((uint8*)srcRow)[x]
    //   dst32[x] = (bClipMap && idx <= 7) ? 0x00000000 : palette->ARGB[idx]
    // plus the 8x replication during palette loading. Getting the replication wrong "shifts every
    // colour in every indexed texture".
    #[test]
    fn palettised_sources_expand_through_the_2048_entry_table() {
        let pal = ramp_palette();
        // `PixelFormatId::P8` indexes the *expanded* table directly: the texel copy reads
        //   idx = ((uint8*)srcRow)[x]; dst32[x] = palette->ARGB[idx]
        // with no scaling, so an 8-bit index can only reach entries 0..255 -- which, after the 8x
        // replication, is the first 32 authored colours, each of them eight times. That is a
        // consequence of the two documented behaviours together, not an extra rule, and it is why
        // the game's own index images are INDEX16 rather than `PixelFormatId::P8`.
        let indices: Vec<u8> = vec![0, 1, 8, 9, 255];
        let t = decode_surface(
            SourcePixels::Palettised8 {
                indices: &indices,
                palette: &pal,
                clip_map: false,
            },
            5,
            1,
        )
        .unwrap();
        assert_eq!(t.format, TextureFormat::Bgra8);
        assert_eq!(t.levels[0].len(), 20);
        // BGRA order: entry 0xFF010101 is B=1, G=1, R=1, A=255.
        assert_eq!(&t.levels[0][0..4], &[0, 0, 0, 255], "index 0 -> slot 0");
        assert_eq!(
            &t.levels[0][4..8],
            &[0, 0, 0, 255],
            "index 1 still lands in slot 0"
        );
        assert_eq!(
            &t.levels[0][8..12],
            &[1, 1, 1, 255],
            "index 8 is the first of slot 1"
        );
        assert_eq!(
            &t.levels[0][12..16],
            &[1, 1, 1, 255],
            "index 9 shares slot 1"
        );
        assert_eq!(
            &t.levels[0][16..20],
            &[31, 31, 31, 255],
            "index 255 -> slot 31"
        );

        // INDEX16 goes straight into the 2048-entry table: index 8 is the *second* source colour,
        // and index 9 is the same colour again (the 8x replication).
        let indices: Vec<u16> = vec![0, 7, 8, 9, 2047, 16];
        let t = decode_surface(
            SourcePixels::Palettised16 {
                indices: &indices,
                palette: &pal,
                clip_map: false,
            },
            6,
            1,
        )
        .unwrap();
        assert_eq!(&t.levels[0][0..4], &[0, 0, 0, 255], "index 0");
        assert_eq!(&t.levels[0][4..8], &[0, 0, 0, 255], "index 7 shares slot 0");
        assert_eq!(&t.levels[0][8..12], &[1, 1, 1, 255], "index 8 is slot 1");
        assert_eq!(
            &t.levels[0][12..16],
            &[1, 1, 1, 255],
            "index 9 shares slot 1"
        );
        assert_eq!(
            &t.levels[0][16..20],
            &[255, 255, 255, 255],
            "index 2047 is slot 255"
        );
        assert_eq!(&t.levels[0][20..24], &[2, 2, 2, 255], "index 16 is slot 2");
    }

    // Oracle: the same function's clip-map term -- "Palette indices 0-7 are the transparent range
    // for clip-mapped surfaces ... which is what the alpha test (reference 100/255) then cuts away."
    #[test]
    fn a_clip_map_makes_indices_zero_to_seven_transparent_black() {
        let pal = ramp_palette();
        let indices: Vec<u16> = vec![0, 7, 8];
        let t = decode_surface(
            SourcePixels::Palettised16 {
                indices: &indices,
                palette: &pal,
                clip_map: true,
            },
            3,
            1,
        )
        .unwrap();
        assert_eq!(&t.levels[0][0..4], &[0, 0, 0, 0]);
        assert_eq!(&t.levels[0][4..8], &[0, 0, 0, 0]);
        assert_eq!(&t.levels[0][8..12], &[1, 1, 1, 255], "index 8 survives");
        // The same data without the clip-map flag keeps index 0 as an ordinary colour with alpha.
        let t = decode_surface(
            SourcePixels::Palettised16 {
                indices: &indices,
                palette: &pal,
                clip_map: false,
            },
            3,
            1,
        )
        .unwrap();
        assert_eq!(t.levels[0][3], 255);
    }

    // The original direct-format path copies source bits into the locked surface. Here those
    // pixels are widened to BGRA8 for a modern GPU, using the observed channel masks.
    #[test]
    fn direct_formats_decode_through_their_documented_masks() {
        // A1R5G5B5: alpha 0x8000, red 0x7C00, green 0x03E0, blue 0x001F.
        // alpha 0x8000 | red 31 | green 0 | blue 31 -- opaque magenta.
        let px: u16 = 0x8000 | (31 << 10) | 31;
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::A1R5G5B5,
                bits: &px.to_le_bytes(),
                pitch: 0,
            },
            1,
            1,
        )
        .unwrap();
        assert_eq!(&t.levels[0][..4], &[255, 0, 255, 255]);

        // X1R5G5B5 has no alpha mask, so it reads as fully opaque even with the top bit clear.
        let px: u16 = 31 << 10;
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::X1R5G5B5,
                bits: &px.to_le_bytes(),
                pitch: 0,
            },
            1,
            1,
        )
        .unwrap();
        assert_eq!(t.levels[0][3], 255, "an X-format surface is opaque");
        assert_eq!(t.levels[0][2], 255, "red is saturated");

        // A8R8G8B8 round-trips byte for byte, because Bgra8 *is* A8R8G8B8's memory order.
        let src = [0x11u8, 0x22, 0x33, 0x44]; // B, G, R, A
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::A8R8G8B8,
                bits: &src,
                pitch: 0,
            },
            1,
            1,
        )
        .unwrap();
        assert_eq!(&t.levels[0][..4], &src);

        // A8 is alpha-only: RGB reads as zero.
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::A8,
                bits: &[0x7F],
                pitch: 0,
            },
            1,
            1,
        )
        .unwrap();
        assert_eq!(&t.levels[0][..4], &[0, 0, 0, 0x7F]);

        // R5G6B5's green channel is six bits: 0x3F must reach 255, not 252.
        let px: u16 = 0x07E0;
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::R5G6B5,
                bits: &px.to_le_bytes(),
                pitch: 0,
            },
            1,
            1,
        )
        .unwrap();
        assert_eq!(t.levels[0][1], 255);
    }

    // Oracle: the same table -- "row by row when the source pitch differs from the destination
    // pitch". A surface whose rows are padded must not smear.
    #[test]
    fn a_source_pitch_wider_than_the_row_is_honoured() {
        // Two rows of two A8 pixels each, with two bytes of padding per row.
        let bits = [1u8, 2, 0xEE, 0xEE, 3, 4, 0xEE, 0xEE];
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::A8,
                bits: &bits,
                pitch: 4,
            },
            2,
            2,
        )
        .unwrap();
        let alphas: Vec<u8> = t.levels[0]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        assert_eq!(alphas, vec![1, 2, 3, 4]);
    }

    // The observed landscape RGB format is 24-bit BGR (red bit mask = 0x00FF0000), expanded
    // to X8R8G8B8. The landscape alpha format is an 8-bit alpha-only mask with the same
    // flags and bit depth as PFID_A8.
    #[test]
    fn the_landscape_formats_decode_as_their_documented_twins() {
        let bits = [0x10u8, 0x20, 0x30, 0x40, 0x50, 0x60];
        let t = decode_surface(SourcePixels::LandscapeRgb(&bits), 2, 1).unwrap();
        assert_eq!(&t.levels[0][..4], &[0x10, 0x20, 0x30, 0xFF]);
        assert_eq!(&t.levels[0][4..8], &[0x40, 0x50, 0x60, 0xFF]);

        let t = decode_surface(SourcePixels::LandscapeAlpha(&[0x00, 0x80, 0xFF]), 3, 1).unwrap();
        let alphas: Vec<u8> = t.levels[0]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        assert_eq!(alphas, vec![0x00, 0x80, 0xFF]);
        // ..and the colour channels stay zero, as an alpha-only surface has no colour.
        assert_eq!(&t.levels[0][..3], &[0, 0, 0]);
    }

    // Oracle: the retail texture loader's DXT row -- "copied verbatim (flags & 4: the whole image
    // size in one block copy; no row loop)". The renderer must pass the blocks to the GPU, not
    // expand them.
    #[test]
    fn dxt_sources_pass_through_as_blocks() {
        let blocks = vec![0xABu8; 8];
        let t = decode_surface(
            SourcePixels::Dxt {
                format: PixelFormatId::Dxt1,
                blocks: &blocks,
            },
            4,
            4,
        )
        .unwrap();
        assert_eq!(t.format, TextureFormat::Bc1);
        assert_eq!(t.levels[0], blocks);
        // DXT2 retains source premultiplication; layout/bytes stay BC2. DXT4/5
        // provenance remains a separate BC3 follow-up, unchanged here.
        let blocks: Vec<u8> = (0..16).collect();
        for (f, want) in [
            (PixelFormatId::Dxt2, TextureFormat::Bc2Premultiplied),
            (PixelFormatId::Dxt3, TextureFormat::Bc2),
            (PixelFormatId::Dxt4, TextureFormat::Bc3Premultiplied),
            (PixelFormatId::Dxt5, TextureFormat::Bc3),
        ] {
            let t = decode_surface(
                SourcePixels::Dxt {
                    format: f,
                    blocks: &blocks,
                },
                4,
                4,
            )
            .unwrap();
            assert_eq!(t.format, want, "{f:?}");
            assert_eq!(
                t.levels,
                std::slice::from_ref(&blocks),
                "{f:?} is never converted/recompressed here"
            );
        }
    }

    // Oracle: the UI's power-of-two rounder -- "0 -> 0, >= 2048 -> 2048, otherwise the
    // next power of two at or above the value", asserted on its own.
    #[test]
    fn the_power_of_two_rounder_matches_get_best_width_height() {
        assert_eq!(best_width_height(0), 0);
        assert_eq!(best_width_height(1), 1);
        assert_eq!(best_width_height(2), 2);
        assert_eq!(best_width_height(3), 4);
        assert_eq!(best_width_height(5), 8);
        assert_eq!(best_width_height(129), 256);
        assert_eq!(
            best_width_height(256),
            256,
            "an exact power of two is unchanged"
        );
        assert_eq!(best_width_height(1025), 2048);
        assert_eq!(best_width_height(2048), 2048);
        assert_eq!(
            best_width_height(4096),
            2048,
            ">= 2048 clamps rather than growing"
        );
    }

    // Check the observed scaling policy and minimum texture size of 8.
    #[test]
    fn the_texture_scale_is_a_shift_with_an_eight_texel_floor() {
        assert_eq!(scaled_dimensions(256, 128, ImageScale::FullRes), (256, 128));
        assert_eq!(scaled_dimensions(256, 128, ImageScale::HalfRes), (128, 64));
        assert_eq!(
            scaled_dimensions(256, 128, ImageScale::QuarterRes),
            (64, 32)
        );
        assert_eq!(scaled_dimensions(256, 128, ImageScale::EighthRes), (32, 16));
        // The floor: 32 >> 3 = 4, raised back to 8.
        assert_eq!(scaled_dimensions(32, 32, ImageScale::EighthRes), (8, 8));
        // ..unless the source was already smaller, in which case the source wins.
        assert_eq!(scaled_dimensions(4, 4, ImageScale::EighthRes), (4, 4));
        // At FULL_RES the floor is not applied at all, so a 1x1 source stays 1x1.
        assert_eq!(scaled_dimensions(1, 1, ImageScale::FullRes), (1, 1));
    }

    // Oracle: "Parsers return Result; they do not panic on malformed input, because
    // they will meet malformed input."
    #[test]
    fn short_or_impossible_input_is_an_error_not_a_panic() {
        let pal = ramp_palette();
        assert!(matches!(
            decode_surface(
                SourcePixels::Palettised8 {
                    indices: &[0],
                    palette: &pal,
                    clip_map: false
                },
                4,
                4
            ),
            Err(RenderError::ShortSourceData { .. })
        ));
        assert!(matches!(
            decode_surface(SourcePixels::LandscapeRgb(&[0, 0]), 4, 4),
            Err(RenderError::ShortSourceData { .. })
        ));
        assert!(matches!(
            decode_surface(
                SourcePixels::Direct {
                    format: PixelFormatId::Other(27),
                    bits: &[0; 64],
                    pitch: 0
                },
                4,
                4
            ),
            Err(RenderError::UnsupportedFormat(_))
        ));
        assert!(matches!(
            decode_surface(SourcePixels::LandscapeAlpha(&[]), 0, 4),
            Err(RenderError::BadDimensions { .. })
        ));
        // A non-compressed format offered on the Dxt arm is a caller error, not a decode.
        assert!(matches!(
            decode_surface(
                SourcePixels::Dxt {
                    format: PixelFormatId::A8,
                    blocks: &[0; 64]
                },
                4,
                4
            ),
            Err(RenderError::UnsupportedFormat(_))
        ));
    }

    // The observed device hard-codes simple non-power-of-two textures as unsupported.
    // The default Caps must reflect that device, or
    // every UI test built on it is testing a device the client never had.
    #[test]
    fn the_default_caps_are_the_shipped_devices() {
        let c = Caps::default();
        assert!(
            !c.simple_non_power_of_two_textures,
            "hard-coded false in the device-capability detection"
        );
        assert!(
            !c.destination_alpha,
            "the back buffer is X8R8G8B8, so it has no alpha"
        );
        assert_eq!(c.pf_argb_surfaces, PixelFormatId::A8R8G8B8);
    }
}

/// The source decoder for a block-compressed GPU format, preserving premultiplied alpha.
#[must_use]
pub const fn block_source_format(
    format: dereth_primitives::TextureFormat,
) -> Option<crate::PixelFormatId> {
    use crate::PixelFormatId as P;
    use dereth_primitives::TextureFormat as T;
    match format {
        T::Bc1 => Some(P::Dxt1),
        T::Bc2 => Some(P::Dxt3),
        T::Bc2Premultiplied => Some(P::Dxt2),
        T::Bc3 => Some(P::Dxt5),
        T::Bc3Premultiplied => Some(P::Dxt4),
        _ => None,
    }
}

#[cfg(test)]
mod block_format_tests {
    //! Behaviour: none (block decoder selection preserves premultiplication).
    use super::*;
    #[test]
    fn compressed_formats_select_the_matching_dxt_alpha_convention() {
        use crate::PixelFormatId as P;
        use dereth_primitives::TextureFormat as T;
        for (gpu, source) in [
            (T::Bc1, P::Dxt1),
            (T::Bc2, P::Dxt3),
            (T::Bc2Premultiplied, P::Dxt2),
            (T::Bc3, P::Dxt5),
            (T::Bc3Premultiplied, P::Dxt4),
        ] {
            assert_eq!(block_source_format(gpu), Some(source));
        }
        assert_eq!(block_source_format(T::Bgra8), None);
    }
}
