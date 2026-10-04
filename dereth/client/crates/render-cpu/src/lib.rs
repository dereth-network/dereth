//! The device-free half of rendering: image codecs, the CPU-side pipeline descriptions, the font
//! atlas, the UI quad path and the camera.
//!
//! **Depends on** `dereth-primitives` and the contract (`dereth-client-contract`). **Used by** the
//! device half (`dereth-render`), which re-exports all of it.
//!
//! **Must never** touch a graphics API or use `unsafe`: it takes the workspace's
//! `forbid(unsafe_code)`, which is the point of the split, and it never reaches the client runtime
//! (`cargo xtask seams`, `seam: client crates`). Its arithmetic goes through
//! `dereth_primitives::num` wherever the original's float-to-int truncation is load-bearing.
//!
//! It holds the codecs (`jpeg`, `dxt`, `d3dx_bc`, `mip`, `pixel_format`, `palette`, `texture`), the
//! descriptions a device is configured from (`pso`, `vertex`, `surface`, `sampler`, `descriptor`),
//! the bitmap-font atlas (`font`), the 2D UI quad path (`ui`), the projection and view matrices
//! (`camera`) and the recording backend (`mock`).

#![doc(html_no_source)]

pub mod camera;
mod d3dx_bc;
pub mod descriptor;
pub mod dxt;
pub mod font;
pub mod jpeg;
pub mod mip;
pub mod palette;
pub mod pixel_format;
pub mod pso;
pub mod sampler;
pub mod shader;
pub mod surface;
pub mod texture;
pub mod ui;
pub mod vertex;

#[cfg(feature = "mock")]
pub mod mock;

pub use camera::{
    compute_aspect_for_viewport, fov_y_from_preference, projection, view_from_frame,
    AspectPreference, FogParams, LightBlock, ViewParams, Viewport,
};
pub use descriptor::{
    combined_texture_key, DescriptorAllocator, DescriptorStats, Released, TextureKey, TextureSpace,
    TextureTable, TextureTableStats, DESCRIPTORS_PER_TEXTURE, UNCACHED,
};
pub use palette::ExpandedPalette;
pub use pixel_format::{PixelFormatDesc, PixelFormatId};
pub use pso::{
    Blend, Cull, PipelineKey, PixelShader, StageOps, SurfaceContext, SurfaceState, ZFunc,
};
pub use surface::{Surface, SurfaceHandler};
pub use texture::{decode_surface, select_surface_format, Caps, SourcePixels};
pub use vertex::{VertexFormat, VertexLayoutInfo};

#[cfg(feature = "mock")]
pub use mock::{RecordedCall, RecordingBackend};

/// Everything that can go wrong inside the renderer.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// A source pixel format this build does not decode.
    #[error("unsupported pixel format {0:?}")]
    UnsupportedFormat(PixelFormatId),
    /// The source bits are shorter than `width * height * bytes_per_pixel` requires.
    #[error(
        "source data is {actual} bytes; {expected} are needed for {width}x{height} {format:?}"
    )]
    ShortSourceData {
        format: PixelFormatId,
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    /// A dimension the client would have rejected (zero, or above the 2048 surface limit).
    #[error("bad dimensions {width}x{height}: {reason}")]
    BadDimensions {
        width: u32,
        height: u32,
        reason: &'static str,
    },
    /// The JPEG decoder rejected the stream, or produced something the client would not accept.
    #[error("jpeg decode failed: {0}")]
    Jpeg(String),
    /// The glyph sheet does not fit a 256x256 atlas, which is what the client's font-texture
    /// setup returns false for.
    #[error("font atlas overflow: the glyph range does not fit 256x256")]
    FontAtlasOverflow,
    /// A Vulkan or Win32 call failed.
    #[error("graphics device error: {0}")]
    Device(String),
    /// A path the renderer deliberately does not implement; see the message for which and why.
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}

/// Per-draw dynamic values. Everything else is the [`PipelineKey`].
///
/// Per-draw dynamic values: alpha-test reference (two values), texture, sampler address mode
/// (two values) and vertex colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawConstants {
    /// 100 (palettised source) or 200 (block-compressed). Zero when the alpha test
    /// is off, where the original would have left a stale value that nothing reads.
    pub alpha_ref: u8,
    /// ARGB, as the polygon path writes it into the `D3DFVF_DIFFUSE` byte.
    pub vertex_colour: u32,
    /// The mesh buffer's UV-delta scroll, as bits so the type stays `Eq` for use in a cache key.
    ///
    /// the delta is turned into a `D3DTS_TEXTURE0` translation each
    /// time the UV animation runs with no visible multiplication by frame time, which
    /// implies a frame-rate-dependent scroll speed. Transcribed as the client does it: the caller
    /// hands over the accumulated offset and the renderer translates by it.
    pub uv_offset_bits: [u32; 2],
    /// `D3DRS_TEXTUREFACTOR`, which is where the UI's colour modulation and opacity both come
    /// from when the UI builds its material.
    pub texture_factor: u32,
}

impl Default for DrawConstants {
    fn default() -> Self {
        Self {
            alpha_ref: 0,
            vertex_colour: 0xFFFF_FFFF,
            uv_offset_bits: [0, 0],
            texture_factor: 0,
        }
    }
}

impl DrawConstants {
    #[must_use]
    pub fn uv_offset(&self) -> [f32; 2] {
        [
            f32::from_bits(self.uv_offset_bits[0]),
            f32::from_bits(self.uv_offset_bits[1]),
        ]
    }

    #[must_use]
    pub fn with_uv_offset(mut self, uv: [f32; 2]) -> Self {
        self.uv_offset_bits = [uv[0].to_bits(), uv[1].to_bits()];
        self
    }
}

/// The packed clear colour, exactly as the client builds it.
///
/// **The alpha byte is the hard-coded constant `0x66`; the supplied alpha is never read**. Each component is `c * 255.0` truncated toward zero, with no clamping. For the frame clear (colour black) the value passed to D3D is
/// therefore `0x66000000`, not `0xFF000000`.
///
/// This is invisible because `COLORWRITEENABLE = 7` masks the alpha channel, so the back buffer's
/// alpha is never written and never read. Do not "correct" it.
#[must_use]
pub fn pack_clear_colour(r: f32, g: f32, b: f32) -> u32 {
    let byte = |c: f32| -> u32 {
        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: the conversion itself is dereth_primitives::num::to_i32; this is the 8-bit store that follows.
        {
            (dereth_primitives::num::to_i32(c * 255.0) as u32) & 0xFF
        }
    };
    0x6600_0000 | (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

/// `D3DRS_COLORWRITEENABLE`: red | green | blue, with alpha masked off.
/// Keeping alpha writes disabled prevents alpha garbage from accumulating.
pub const COLOR_WRITE_ENABLE_RGB: u8 = 0x7;

/// The gamma setter's arithmetic, as the original computes it.
///
/// `v` is clamped to `[-0.2, 1.0]`, then each of the 256 entries is
/// `c = i*255 - (int)((float)(i*255) * v * -2.0f)`, i.e. `255i + 510·i·v`, clamped to `[0, 65535]`.
/// `v = 0` gives the identity ramp `255*i`, which tops out at 65025 — the client never reaches
/// 0xFFFF.
#[must_use]
pub fn gamma_ramp(brightness: f32) -> [u16; 256] {
    let v = brightness.clamp(-0.2, 1.0);
    let mut ramp = [0u16; 256];
    for (i, slot) in ramp.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let base = (i * 255) as f32;
        let t = dereth_primitives::num::to_i32(base * v * -2.0);
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        // LINT-OK: integer arithmetic on values already bounded by the clamp below.
        let c = (i as i32) * 255 - t;
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        {
            *slot = c.clamp(0, 65535) as u16;
        }
    }
    ramp
}

#[cfg(test)]
mod tests {
    use super::*;

    // The packed clear colour is 0x66000000 | (R<<16) | (G<<8) | B. For the frame clear
    // (the frame start, colour black) the value passed to D3D is therefore 0x66000000, not
    // 0xFF000000".
    #[test]
    fn the_clear_colour_packs_alpha_as_the_hard_coded_0x66() {
        assert_eq!(pack_clear_colour(0.0, 0.0, 0.0), 0x6600_0000);
        assert_eq!(pack_clear_colour(1.0, 1.0, 1.0), 0x66FF_FFFF);
        // Truncation, not rounding: 0.5 * 255 = 127.5 -> 127.
        assert_eq!(pack_clear_colour(0.5, 0.0, 0.0), 0x667F_0000);
        // No clamping in the original; the byte store keeps the low 8 bits.
        assert_eq!(pack_clear_colour(0.0, 0.0, 2.0) & 0xFF, (510 & 0xFF) as u32);
    }

    // Observed gamma arithmetic: v = 0 gives the identity ramp
    // c = 255*i (0...65025, i.e. slightly below full white at the top -- the client never reaches
    // 0xFFFF). v = 1 doubles the slope (saturating at i >= 129); v = -0.2 scales by 0.6.
    #[test]
    fn the_gamma_ramp_matches_the_recovered_arithmetic() {
        let identity = gamma_ramp(0.0);
        assert_eq!(identity[0], 0);
        assert_eq!(identity[255], 65025);
        for (i, v) in identity.iter().enumerate() {
            assert_eq!(usize::from(*v), i * 255);
        }
        // v = 1 doubles the slope and saturates once 765*i exceeds 65535, i.e. from i = 86 upward.
        let doubled = gamma_ramp(1.0);
        assert_eq!(doubled[1], 765);
        assert_eq!(doubled[255], 65535);
        // v = -0.2 scales by 1 - 0.4 = 0.6.
        let dim = gamma_ramp(-0.2);
        assert_eq!(dim[255], 39015);
        assert_eq!(u32::from(dim[255]), 65025 * 6 / 10);
        // The clamp is applied to the argument, so anything below -0.2 or above 1.0 is the endpoint.
        assert_eq!(gamma_ramp(-5.0), dim);
        assert_eq!(gamma_ramp(5.0), doubled);
    }

    // The observed colour-write mask enables RGB only: COLORWRITEENABLE = 7.
    #[test]
    fn the_alpha_channel_is_never_written() {
        assert_eq!(COLOR_WRITE_ENABLE_RGB, 0x7);
        assert_eq!(
            COLOR_WRITE_ENABLE_RGB & 0x8,
            0,
            "alpha write must stay masked off"
        );
    }

    #[test]
    fn draw_constants_round_trip_a_uv_offset() {
        let c = DrawConstants::default().with_uv_offset([0.25, -0.5]);
        assert_eq!(c.uv_offset(), [0.25, -0.5]);
        // Still hashable/comparable, which is what makes it usable as part of a cache key.
        assert_eq!(c, DrawConstants::default().with_uv_offset([0.25, -0.5]));
    }
}
