//! The G-buffer's shared layout: its targets, their formats and the light classes its material
//! target carries. A game's remaster draws its surfaces into it, and every shared pass reads it.
//!
//! The material target's first byte is the surface's light class ([`class`]); its other bytes are
//! the game's own. The Slang side of the same vocabulary is `shaders/frame.slang`.

use dereth_gfx::{Format, Texture};

/// The G-buffer's targets and the world's colors, at the world's size.
#[derive(Debug, Clone, Copy)]
pub struct Targets {
    pub width: u32,
    pub height: u32,
    pub albedo: Texture,
    pub normal: Texture,
    pub light: Texture,
    pub material: Texture,
    pub depth: Texture,
    pub color: Texture,
    /// The outdoor surfaces' depth, kept when the viewer stands inside, before the interior
    /// clears it; outdoors, the depth before the grass, which the frame's depth is made from.
    pub outdoor_depth: Texture,
    /// Each one's index in the table, in the order above.
    pub index: [u32; 7],
}

/// The formats of the G-buffer's targets.
pub const ALBEDO: Format = Format::Rgba16Float;
pub const NORMAL: Format = Format::Rg16Snorm;
pub const LIGHT: Format = Format::Rgba16Float;
pub const MATERIAL: Format = Format::Rgba8Unorm;
pub const DEPTH: Format = Format::D32Float;
/// The world's colors.
pub const COLOR: Format = Format::Rgba8Unorm;

/// The light classes, as `shaders/frame.slang` names them: what the material target's first byte
/// says a surface is (0 is no surface).
pub mod class {
    pub const GROUND: u32 = 1;
    pub const SUN: u32 = 2;
    pub const INDOOR: u32 = 3;
    pub const CELL: u32 = 4;
    pub const SKY: u32 = 5;
}
