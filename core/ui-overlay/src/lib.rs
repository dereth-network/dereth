//! The overlay an interface draws over the world: the textures it uploads and the textured triangle
//! batches and preview spaces a presentation draws in order, naming no game and no graphics API.
//!
//! A front end turns its own widgets into a list of textured, tinted triangle batches and preview
//! spaces over textures it uploads under its own keys; the presentation draws the list in order
//! after the world, on whatever device it has (or counts it, with none). A vertex is clip-space
//! position, a packed colour and a texture coordinate, and a batch says only which texture, how it
//! is sampled and how it is combined with what is already drawn.
//!
//! **Depends on** `dereth-primitives` (the rectangle a preview space is drawn into). **Used by**
//! both games' interfaces and presentations: the first game's contract re-exports it
//! (`dereth-client-contract`), and the widget kit (`dereth-ui-kit`) lowers its draw lists to it.
//!
//! **Must never** name either game, a graphics API or the platform (`cargo xtask seams`, `seam:
//! the drawing both games share names no game`).

/// The rectangle of the back buffer a preview space is drawn into.
pub use dereth_primitives::Viewport;

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

/// Whether a batch is hidden behind the world it is drawn over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OverlayDepth {
    /// Drawn over everything, whatever depth its vertices carry.
    #[default]
    Over,
    /// Hidden, pixel by pixel, wherever the world drawn this frame stands in front of it. Each
    /// vertex's `position[2]` is its depth as the world's own projection gives it: `z / w`, 0 at
    /// the near plane and 1 at the far. It writes no depth, so it hides nothing drawn after it. A
    /// presentation whose world leaves no such depth draws it over everything.
    World,
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
    /// A tool's own, which no game screen draws: the world builder's look at one object.
    Tool,
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
        depth: OverlayDepth,
        vertices: Vec<OverlayVertex>,
    },
    /// A preview space, drawn into `rect` (in back-buffer pixels) at this point in the order.
    Preview { space: PreviewSpace, rect: Viewport },
}

/// A picture laid on the ground at a place and drawn with the world, as a [`GroundMarker`] is:
/// on the terrain there outdoors, following its shape, or level with the place on a floor indoors;
/// hidden where something stands in front of it, and under the overlay. A front end that wants
/// one asks for it each frame. `P` is the game's own name for a place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundMark<P> {
    /// Where its centre lies.
    pub at: P,
    /// How far it reaches from its centre to each side, in metres.
    pub radius: f32,
    /// The picture, an overlay texture the front end has uploaded, and the part of it to lay
    /// down: `[left, top, right, bottom]` in texture coordinates; the picture's top points north
    /// before it is turned.
    pub texture: OverlayTexture,
    pub uv: [f32; 4],
    /// The colour the picture is multiplied by, packed `0xAARRGGBB`.
    pub tint: u32,
    /// How far it is turned about its centre, in radians, counter-clockwise seen from above, as a
    /// [`GroundMarker`] is turned: a picture turned by minus a heading's angle has its top toward
    /// that heading.
    pub turn: f32,
}

/// One point of a [`WorldBand`]: the place it is over, and how it is drawn there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandPoint<P> {
    /// The place under it.
    pub at: P,
    /// How far over the ground or the floor under `at` it is held, metres.
    pub lift: f32,
    /// How far it reaches to each side of its line, metres.
    pub half_width: f32,
    /// The colour its picture is multiplied by there, packed `0xAARRGGBB`.
    pub tint: u32,
    /// How far along its picture it is there (the picture's `u`).
    pub u: f32,
}

/// A band drawn in the world through a line of points, turned about its line to face the camera:
/// held over the ground under each point as a [`GroundMark`] lies on it (on the terrain outdoors,
/// or its place's own height on a floor indoors), with its picture's `v` running from one edge
/// to the other and its `u` along it. Drawn with the world, hidden where something stands in
/// front of it, written into no depth. A front end that wants one asks for it each frame.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldBand<P> {
    pub points: Vec<BandPoint<P>>,
    /// The picture, an overlay texture the front end has uploaded.
    pub texture: OverlayTexture,
}

/// A picture laid on the ground under a world object and drawn with the world: on the terrain
/// or the floor under it, hidden where something stands in front of it, and under the overlay.
/// A front end that wants one asks for it each frame; asking for none takes it away. `O` is the
/// game's own name for an object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundMarker<O> {
    /// The object it lies under.
    pub object: O,
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
