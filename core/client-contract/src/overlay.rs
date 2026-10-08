//! The overlay any UI draws over the world: textures it uploads under its own keys, and a list of
//! textured, tinted triangle batches and preview spaces, drawn in order after the world.
//!
//! A front end turns its own widgets into this list; the presentation draws it on whatever device
//! it has (or counts it, with none). Nothing here names a graphics API: a vertex is clip-space
//! position, a packed colour and a texture coordinate, and a batch says only which texture, how it
//! is sampled and how it is combined with what is already drawn.

use dereth_primitives::Viewport;

/// Which family a texture key belongs to. Keys in different spaces never meet, whatever their
/// numbers, so a front end's image keys and its font keys cannot collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OverlaySpace {
    /// An image, shared with any other holder of the same key: a second upload under a key that is
    /// already resident is a cache hit, not a second texture.
    Image,
    /// A font's glyph sheet, shared the same way.
    Glyphs,
    /// A texture of this front end's own that is shared with nothing: the one white texel a flat
    /// fill samples, a movie frame. Uploading under a resident key replaces it.
    Local,
}

/// A texture, by the key its front end chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OverlayTexture {
    pub space: OverlaySpace,
    pub key: u64,
}

/// What releasing a texture did, so a release can be counted rather than assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayReleased {
    /// The texture's slot is free.
    Freed,
    /// Another holder of the same key still holds it.
    StillLinked,
    /// The device did not know the slot: a double release.
    Unknown,
    /// No texture was uploaded under that key.
    Absent,
}

/// One vertex: clip-space position (`-1..1`, `y` up), the colour as packed `0xAARRGGBB`, and the
/// texture coordinate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayVertex {
    pub position: [f32; 3],
    pub color: u32,
    pub uv: [f32; 2],
}

/// How a batch's texture is sampled: point or linear filtering, and whether each axis repeats
/// (wraps) or clamps at the edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OverlaySampler {
    pub point: bool,
    pub wrap_u: bool,
    pub wrap_v: bool,
}

impl OverlaySampler {
    /// Point filtering, clamped on both axes: one texel to one pixel.
    pub const POINT_CLAMP: Self = Self {
        point: true,
        wrap_u: false,
        wrap_v: false,
    };
}

/// How a batch combines with what is already drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverlayMaterial {
    /// The texture times the vertex colour, alpha-blended over the frame.
    Image,
    /// Text: the colour is the vertex's alone and the texture gives only coverage, so one white
    /// glyph sheet draws text in every colour.
    Text,
    /// The frame's own colour inverted wherever the batch covers it (a selection highlight).
    Invert,
}

/// A creature-mode preview space: a small 3D scene with its own camera and light, drawn into a
/// rectangle of the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PreviewSpace {
    /// The portal-space swirl.
    Portal,
    /// The character-creation preview.
    CharGen,
    /// The paper doll.
    PaperDoll,
    /// The examination portrait.
    Examine,
}

/// The one light a preview space is lit by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreviewLight {
    /// A point light.
    Point,
    /// A distant light from one direction, as the shipped spaces use.
    Directional,
    /// A spot light.
    Spot,
}

/// One step of the overlay, in drawing order.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayItem {
    /// Triangles, three vertices each, over one texture.
    Triangles {
        material: OverlayMaterial,
        texture: OverlayTexture,
        sampler: OverlaySampler,
        vertices: Vec<OverlayVertex>,
    },
    /// A preview space, drawn into `rect` (in back-buffer pixels) at this point in the order.
    Preview { space: PreviewSpace, rect: Viewport },
}

/// A picture laid on the ground under a world object and drawn with the world: on the terrain
/// or the floor under it, hidden where something stands in front of it, and under the overlay.
/// A front end that wants one asks for it each frame; asking for none takes it away.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundMarker {
    /// The object it lies under.
    pub object: dereth_primitives::ObjectId,
    /// The picture, an overlay texture the front end has uploaded, and the part of it to lay
    /// down: `[left, top, right, bottom]` in texture coordinates.
    pub texture: OverlayTexture,
    pub uv: [f32; 4],
    /// The colour the picture is multiplied by, packed `0xAARRGGBB`.
    pub tint: u32,
    /// How wide it is, as a multiple of the radius the object is selected within.
    pub scale: f32,
    /// How far it is turned about the object, in radians.
    pub turn: f32,
}
