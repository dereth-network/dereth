//! The material description and surface flags that drive the PSO catalogue.
//!
//! This module models the surface flags and render fields from the original 144-byte layout.
//!
//! **Why the struct is declared here.** `PipelineKey::from_surface` wants a surface type, but
//! `dereth-primitives` defines only `ids`, `space`, `time` and the interface traits, and the decoded
//! dat structs live in `dereth-assets`, which the renderer does not depend on. The struct is
//! therefore declared here, with the field
//! names shared with the decoded surface record. If `Surface` is ever promoted into
//! `dereth-primitives` this becomes a re-export.

/// Surface-type bits stored in the dat payload.
///
/// Surface flags that select material and pass behavior.
pub mod surface_type {
    /// The surface is a flat colour (`color_value`), no texture.
    pub const BASE1_SOLID: u32 = 0x0000_0001;
    /// The surface has a texture map.
    pub const BASE1_IMAGE: u32 = 0x0000_0002;
    /// The texture has a 1-bit cutout: alpha test on.
    pub const BASE1_CLIPMAP: u32 = 0x0000_0004;
    /// Use `translucency` as a constant alpha.
    pub const TRANSLUCENT: u32 = 0x0000_0010;
    /// (data-side) the `diffuse` scalar is meaningful.
    pub const DIFFUSE: u32 = 0x0000_0020;
    /// (data-side) the `luminosity` scalar is meaningful.
    pub const LUMINOUS: u32 = 0x0000_0040;
    /// src = `SRCALPHA`.
    pub const ALPHA: u32 = 0x0000_0100;
    /// src = `INVSRCALPHA`.
    pub const INVALPHA: u32 = 0x0000_0200;
    /// dst = `ONE` (and fog is suppressed).
    pub const ADDITIVE: u32 = 0x0001_0000;
    /// (data-side) marks a detail surface.
    pub const DETAIL: u32 = 0x0002_0000;
    /// Always forced on when a surface is installed.
    pub const GOURAUD: u32 = 0x1000_0000;
    /// Reused at runtime as "this texture is tiled (WRAP addressing)".
    pub const STIPPLED: u32 = 0x4000_0000;
    /// Legacy software-rasteriser flag; ignored by the D3D path.
    pub const PERSPECTIVE: u32 = 0x8000_0000;
}

/// Surface-handler kind, describing how the texture is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurfaceHandler {
    #[default]
    Unknown = 0,
    /// A plain dat texture.
    Database = 1,
    /// Palette-shifted: the clothing/skin recolour path.
    PalShift = 2,
    /// Two textures merged.
    TexMerge = 3,
    CustomDb = 4,
}

impl SurfaceHandler {
    #[must_use]
    pub const fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::Database,
            2 => Self::PalShift,
            3 => Self::TexMerge,
            4 => Self::CustomDb,
            _ => Self::Unknown,
        }
    }
}

/// The surface fields (dat type 0x08) that the render path reads.
///
/// Original-client layout offsets: `type` @ 0x58, `handler` @ 0x5C,
/// `color_value` @ 0x60, `translucency` @ 0x74, `luminosity` @ 0x78, `diffuse` @ 0x7C.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surface {
    /// Surface type represented by the [`surface_type`] bitfield.
    pub r#type: u32,
    /// Optional surface handler id.
    pub handler: SurfaceHandler,
    /// Flat color for an untextured surface.
    pub color_value: u32,
    /// Surface translucency.
    pub translucency: f32,
    /// Surface luminosity.
    pub luminosity: f32,
    /// Surface diffuse color.
    pub diffuse: f32,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            r#type: surface_type::BASE1_IMAGE,
            handler: SurfaceHandler::Database,
            color_value: 0,
            translucency: 0.0,
            luminosity: 0.0,
            diffuse: 1.0,
        }
    }
}

impl Surface {
    /// Test one or more [`surface_type`] bits.
    #[inline]
    #[must_use]
    pub const fn has(&self, bits: u32) -> bool {
        self.r#type & bits != 0
    }
}
