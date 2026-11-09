//! The scene as the remaster is handed it: what changed since the last frame ([`SceneDelta`]) and
//! the frame's own state ([`FrameDelta`]), as plain owned data.
//!
//! The scene publishes both after each frame's walk, from shared reads, so nothing here can change
//! what the frame drew, what can be picked in it or what the server is told. A block or an object
//! is sent whole when it arrives and again whole when it is built anew; the frame says only what
//! moves from one frame to the next: the camera, the light, the parts that move, the level each
//! degrading placement draws at, and what the walk drew.
//!
//! **Spaces.** Everything is in the scene's space, with height on the third axis. A block's
//! placements, cells and lights are block-local: the block's south-west corner is their origin,
//! and the block stands at `(x − origin.x, y − origin.y) × 192` metres for the frame's
//! [`FrameDelta::origin`]. An object's parts are in the frame's space directly. The camera's
//! [`Camera::view`] takes a point of the scene's space straight to the eye.
//!
//! **Ids** are retail's: a GfxObj, Setup, Environment or surface id as the dats name it, a cell by
//! its full id, a block as `(x << 8) | y`.
//!
//! [`Retained`] is the retained scene these build, with no device: what a renderer holds, counted.

mod retained;

pub use retained::{Retained, RetainedCounts};

use glam::{Mat4, Vec3};

/// A landblock, as `(x << 8) | y`.
pub type BlockId = u16;

/// The block `(x, y)` names.
#[must_use]
pub fn block_id(x: u8, y: u8) -> BlockId {
    (u16::from(x) << 8) | u16::from(y)
}

/// One change to the scene, in the order the scene made it.
#[derive(Debug, Clone, PartialEq)]
pub enum SceneDelta {
    /// A block entered the window, with everything it holds.
    BlockIn(Box<Block>),
    /// A block the remaster holds was built again: everything it held is replaced by this.
    BlockChanged(Box<Block>),
    /// A block the remaster holds was built again with only its ground changed (its level of
    /// detail or its stitching): its ground is replaced, and everything else it holds is kept.
    GroundChanged(Box<GroundDelta>),
    /// A block left the window: everything it held goes.
    BlockOut(BlockId),
    /// An object was built.
    ObjectBuilt(Box<Object>),
    /// An object the remaster holds was built again, with another look: it is replaced by this.
    ObjectChanged(Box<Object>),
    /// An object was removed.
    ObjectRemoved(ObjectKey),
}

/// A block's ground, built again.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundDelta {
    pub block: BlockId,
    /// The build of the block it comes from.
    pub generation: u32,
    pub terrain: Terrain,
}

/// Everything one block holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub block: BlockId,
    /// Which build of the block this is: a later build of the same block has a larger number.
    pub generation: u32,
    pub terrain: Terrain,
    /// The block's own placements: its scenery, its buildings' shells and its statics, in the
    /// order they were baked. Empty on a block drawn farther off than its objects are.
    pub placements: Vec<Placement>,
    /// The degrade records of the block's degrading parts: the block's own placements' first,
    /// then each cell's in [`Self::rooms`] order. A [`Part::degrade`] indexes it, and so does
    /// [`BlockLevels::levels`].
    pub degrades: Vec<Degrade>,
    /// The block's interior cells.
    pub rooms: Vec<Room>,
    /// The lights its cells' statics give off.
    pub lights: Vec<StaticLight>,
    /// The lights its own placements carry outdoors.
    pub lamps: Vec<Lamp>,
}

/// One light a block's outdoor placement carries: a lamp post's, a lantern's, a torch's. Retail
/// lights the outdoors with the sun alone and draws none of them; the remaster can light the
/// night with them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lamp {
    /// What it is, which says how it is lit and how large it glows.
    pub source: LampSource,
    /// Whether it flickers as a flame does, and a number of its own, so that no two flicker alike.
    pub flicker: bool,
    pub seed: u32,
    /// Where it is, block-local.
    pub position: Vec3,
    /// How far its placement is drawn above where it stands: on the near ground, and on the far.
    pub lifts: [f32; 2],
    pub color: [f32; 3],
    pub intensity: f32,
    pub falloff: f32,
}

/// What gives an outdoor lamp its light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LampSource {
    /// A light its object's setup carries.
    Authored,
    /// A fire-colored emitter its object starts.
    Flame,
    /// A warm luminous surface: lamp glass, a lantern's pane.
    Glow,
}

/// A block's ground as it is drawn: a grid of `side × side` cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    /// Cells a side: 8 at full detail, 4, 2 or 1 farther off.
    pub side: u8,
    /// The grid's `(side + 1)²` vertices, block-local, the one `i` cells east and `j` north at
    /// `i * (side + 1) + j`.
    pub vertices: Vec<Vec3>,
    /// For each drawn cell, at `i * side + j`, whether its diagonal runs from the south-west
    /// corner to the north-east one.
    pub cuts: Vec<bool>,
    /// The block's terrain words, one per vertex of the full-detail 9 × 9 grid, row-major as the
    /// dat stores them.
    pub words: [u16; 81],
}

/// What kind of placement a [`Placement`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlacementKind {
    /// Scenery or a static object outdoors, or a static object in a cell.
    Object,
    /// A building's shell, drawn with the building pass's detail texture.
    Building,
}

/// One object placed in a block or a cell, as its parts are drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// The Setup or GfxObj id placed.
    pub id: u32,
    pub kind: PlacementKind,
    /// Where it stands, block-local, its scale left out.
    pub frame: Mat4,
    /// Its uniform scale.
    pub scale: f32,
    /// How far it is drawn above where it stands on the ground drawn over retail's surface: on
    /// the near ground and on the far, mixed by its distance from the eye as the ground mixes
    /// them. Nought while no offset is drawn.
    pub lifts: [f32; 2],
    /// The parts drawn. A part whose nearest level draws nothing is left out.
    pub parts: Vec<Part>,
}

/// One drawn part of a [`Placement`].
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// The GfxObj the part names: what it draws when it does not degrade, and what its degrade
    /// record is found by when it does.
    pub gfxobj: u32,
    /// Where it stands, block-local, its scale left out.
    pub frame: Mat4,
    /// The scale its mesh is drawn at, per axis.
    pub scale: Vec3,
    /// Its degrade record in the block's [`Block::degrades`], when it degrades.
    pub degrade: Option<u32>,
}

impl Part {
    /// The part's mesh transform, block-local: its frame with its scale applied first.
    #[must_use]
    pub fn transform(&self) -> Mat4 {
        self.frame * Mat4::from_scale(self.scale)
    }
}

/// One degrading part's levels.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Degrade {
    /// The GfxObj each level draws, nearest first; 0 is a level that draws nothing.
    pub levels: Vec<u32>,
    /// Whether any level turns toward the viewer: then the frame says how the part is turned
    /// while it draws such a level ([`BlockLevels::turned`]).
    pub billboards: bool,
    /// Each level's distances as retail's record gives them: the nearest, the ideal and the
    /// farthest it hands over to the next level at, before retail's bias and its degrade
    /// distance ([`DegradeBias`]) move them.
    pub bands: Vec<[f32; 3]>,
    /// Each level's turn toward the viewer, as retail's record names it: 1 none, 2 wholly, 3, 4
    /// and 5 about its x, y and z axis.
    pub modes: Vec<u8>,
    /// The part's z scale: retail measures the distance over it, so a part drawn larger hands
    /// over farther away.
    pub scale_z: f32,
    /// The point retail measures the part's distance from, block-local.
    pub centre: Vec3,
}

/// One interior cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Room {
    /// The cell's full id.
    pub id: u32,
    /// The Environment its geometry comes from, and the piece of it.
    pub environment: u32,
    pub structure: u16,
    /// The surfaces its polygons name, by their index in this list.
    pub surfaces: Vec<u32>,
    /// Where it stands, block-local.
    pub frame: Mat4,
    /// Whether the outdoors can be seen from it: then it is lit with the outdoor ambient.
    pub seen_outside: bool,
    pub portals: Vec<Portal>,
    /// Its statics, in the cell's order.
    pub placements: Vec<Placement>,
}

/// One opening of a cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Portal {
    /// The cell on the other side, by its full id, or `None` for the outdoors.
    pub other: Option<u32>,
    /// The opening's polygon, in the cell's own space.
    pub polygon: Vec<Vec3>,
}

/// What kind of light a light is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LightKind {
    Point,
    Directional,
    Spot,
}

/// One light a cell's static gives off, as the static was placed.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticLight {
    /// The cell it was registered in.
    pub cell: u32,
    pub kind: LightKind,
    /// Where it is, block-local.
    pub position: Vec3,
    /// Which way it points, for a directional or spot light.
    pub direction: Vec3,
    pub color: [f32; 3],
    pub intensity: f32,
    pub falloff: f32,
    pub cone_angle: f32,
    /// Whether it is a static light, burned into its rooms' walls; a dynamic one lights them as
    /// they are drawn.
    pub burned: bool,
}

/// Which object an [`Object`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectKey {
    /// An object the server put in the world, by its id.
    Server(u32),
    /// The player's own body.
    Body,
    /// A static placed in a block that plays an animation, so its parts move: the `index`th of
    /// its block's.
    Animated { block: BlockId, index: u32 },
}

/// One object whose parts move.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub key: ObjectKey,
    /// The Setup it is built on.
    pub setup: u32,
    /// Its parts, in the order the frame poses them.
    pub parts: Vec<ObjectPart>,
}

/// One part of an [`Object`].
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectPart {
    /// The GfxObj each degrade level draws, nearest first, one entry for a part that does not
    /// degrade; 0 is a level that draws nothing. Empty for a part that never draws.
    pub levels: Vec<u32>,
    /// What the object's description does to the part's surfaces, when it does anything.
    pub look: Option<PartLook>,
}

/// What an object's description does to one part's surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PartLook {
    /// Each texture replaced, and what replaces it.
    pub textures: Vec<(u32, u32)>,
    /// The palette its colors are shifted from.
    pub palette: Option<u32>,
    /// The palette ranges laid over it, in order: the palette set, the first color and the
    /// count.
    pub ranges: Vec<(u32, u32, u32)>,
}

/// One frame's state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrameDelta {
    /// Counts the frames published, from 1.
    pub serial: u64,
    /// The block whose south-west corner is the frame's origin.
    pub origin: [i32; 2],
    pub camera: Camera,
    pub light: RetailLight,
    /// Whether the walk drew the outdoors: the landscape and the sky. Indoors it does only where
    /// an opening shows the outdoors.
    pub landscape: bool,
    /// The blocks the walk drew outdoors, in the order it drew them: each one's ground, and every
    /// placement of its own at its level. A block's interior cells are drawn as
    /// [`Self::drawn_cells`] says.
    pub drawn_blocks: Vec<BlockId>,
    /// The sky's objects, in the order they are drawn, each saying which of the passes before
    /// and after the ground draws it; neither is drawn without [`Self::landscape`].
    pub sky: Vec<SkyObject>,
    /// The level each degrading placement draws at, for every block whose levels changed since
    /// the last frame, and for every block sent in this frame's deltas.
    pub levels: Vec<BlockLevels>,
    /// Every object's pose, every frame.
    pub objects: Vec<ObjectPose>,
    /// Every interior cell the walk drew.
    pub drawn_cells: Vec<u32>,
    /// Every part of a cell's statics the walk drew: the cell, and the part's place among the
    /// parts of the cell's placements, in their order.
    pub drawn_cell_parts: Vec<(u32, u32)>,
    /// Every object part the walk drew, with the lights it was drawn with.
    pub drawn_parts: Vec<DrawnPart>,
    /// Every particle the walk drew, in its order.
    pub particles: Vec<ParticleDraw>,
    /// The name of the day group the sky follows today, as the region names it ("Sunny", "Rainy",
    /// ...); empty when the region has no sky.
    pub day_group: String,
    /// The scene's clock, seconds: what the remaster's own motion (the water's ripples) is timed
    /// by, so a held clock draws a still frame.
    pub clock: f64,
    /// The lights of the moving objects that stand outdoors with their light on, in the frame's
    /// space. Retail's light pools leave them out: the outdoors is lit by the sun alone.
    pub lamps: Vec<Light>,
    /// The ground drawn over retail's with a visual offset, while the scene draws what stands on
    /// the ground on it; `None` while it does not.
    pub drawn: Option<DrawnGround>,
    /// The art pack's far trees and forest cover, while a pack with far pictures is drawn.
    pub forest: Option<Forest>,
    /// How retail moves its degrade distances this frame.
    pub degrade: DegradeBias,
    /// The game's weather: whether it is drawn, and the form the player asked it to fall in.
    pub weather: FrameWeather,
    /// The hue an art pack's leaves are pulled toward, in turns, and how far (0 not at all): the
    /// look's.
    pub foliage: [f32; 2],
}

/// The game's weather, as a frame carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameWeather {
    /// Whether the game draws its weather.
    pub enabled: bool,
    /// The form the player asked the weather to fall in.
    pub asked: AskedFall,
}

/// The form the player asked the weather to fall in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AskedFall {
    /// As the land round the viewer has it: snow over snowy land, rain elsewhere.
    #[default]
    Land,
    /// Rain, whatever the land.
    Rain,
    /// Snow, whatever the land.
    Snow,
}

/// The ground drawn over retail's with a visual offset, as the scene works it out: what stands
/// on the ground is drawn moved by the same numbers, so it stands on what is drawn. Collision,
/// picking and the server keep retail's ground.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawnGround {
    /// The whole world's landscape vertices, sent again only when they are read again.
    pub world: std::sync::Arc<DrawnWorld>,
    /// The offset of each block round the viewer that has one.
    pub tiles: Vec<DrawnTile>,
    /// How many blocks the tiles reach from the viewer's block, each way.
    pub reach: u8,
    /// Where the frame's space's origin lies in the world, meters east and north: a point `p` of
    /// the frame's space stands over the world's point `p.xy + global`.
    pub global: glam::Vec2,
    pub knobs: DrawnKnobs,
    /// Where the bodies standing on the ground stand, the frame's space: the fine relief is
    /// leveled under each.
    pub bodies: Vec<Vec3>,
}

/// The clouds that shade the land.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DrawnClouds {
    /// How much of the sky they cover, against retail's own picture.
    pub cover: f32,
    /// How high over the eye their base lies, and how thick they are, meters.
    pub height: f32,
    pub depth: f32,
    /// How large retail's picture is laid over the sky.
    pub scale: f32,
    /// How much they shade the land under them.
    pub shadows: f32,
}

/// One tree far off: where retail's scenery places it, world meters, standing on the far ground.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FarTree {
    pub at: Vec3,
    pub rotation: glam::Quat,
    pub scale: f32,
    /// The retail object it stands for.
    pub id: u32,
    /// A number of its own, the same whenever it is worked out.
    pub seed: u32,
}

/// The far trees within reach, block by block, each block's list handed over as it is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FarTreeSet {
    /// A number of its own, so a set made again is told from the one before.
    pub serial: u64,
    pub blocks: Vec<(BlockId, std::sync::Arc<Vec<FarTree>>)>,
}

/// One model of a pack's far pictures.
#[derive(Debug, Clone, PartialEq)]
pub struct FarPackModel {
    /// The retail object it stands for.
    pub id: u32,
    /// The centre of its bounds in its own space, and their radius.
    pub bounds: (Vec3, f32),
    /// Its quad's half width, and the heights of its foot and its top.
    pub quad: [f32; 3],
    /// The views of it, and the views to a row of its pictures.
    pub views: [u32; 2],
    /// Its color factor, in display values, and where its pictures' coverage is cut.
    pub color: [f32; 4],
    pub cut: f32,
    /// Whether it is foliage, its pictures pulled toward the look's foliage hue.
    pub foliage: bool,
}

/// Where a pack's far pictures are read from: a model's color (coverage in alpha) and its normals
/// (depth along the view in alpha), each made when the renderer wants it.
pub trait FarPictures: std::fmt::Debug + Send + Sync {
    /// The pictures of the `model`th of [`FarPack::models`]; `None` while they are still being
    /// made, and they are asked for again.
    fn pictures(&self, model: usize) -> Option<[crate::assets::Texture; 2]>;
}

/// A pack's far pictures.
#[derive(Debug, Clone)]
pub struct FarPack {
    /// The pack's own number: packs with the same number are the same pack.
    pub serial: u64,
    pub models: Vec<FarPackModel>,
    pub pictures: std::sync::Arc<dyn FarPictures>,
}

impl PartialEq for FarPack {
    fn eq(&self, other: &Self) -> bool {
        self.serial == other.serial
    }
}

/// The forest cover, a texel a landscape vertex, rows from the south.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ForestCover {
    /// A number of its own.
    pub serial: u64,
    /// Vertices along a side.
    pub side: u32,
    /// Each vertex's crown color and the share covered, four bytes in display values.
    pub color: std::sync::Arc<Vec<u8>>,
    /// Each vertex's canopy: the share covered times the crowns' height, quarter meters.
    pub canopy: std::sync::Arc<Vec<u8>>,
}

/// How the far trees and the forest cover are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FarKnobs {
    /// Whether the trees are drawn, and how far.
    pub trees: bool,
    pub reach: f32,
    /// Where they start to thin out, how much wider those left stand, and how many levels coarser
    /// their pictures are read.
    pub thin_from: f32,
    pub widen: f32,
    pub soften: f32,
    /// How much of its size a far tree standing on the skyline keeps, from far enough off.
    pub skyline: f32,
    /// How strongly the cover tints the far ground, and how much its canopy raises it.
    pub cover: f32,
    pub canopy: f32,
}

/// The far forest of an art pack, as a frame carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct Forest {
    pub pack: std::sync::Arc<FarPack>,
    pub trees: std::sync::Arc<FarTreeSet>,
    pub cover: Option<std::sync::Arc<ForestCover>>,
    pub knobs: FarKnobs,
}

/// The world's landscape vertices.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawnWorld {
    /// Bumped whenever the vertices are read again.
    pub serial: u64,
    /// Vertices along a side.
    pub side: u32,
    /// Each vertex's height, meters, row by row from the south: `north * side + east`.
    pub heights: std::sync::Arc<Vec<f32>>,
    /// Each vertex's terrain word, in the same order.
    pub words: std::sync::Arc<Vec<u16>>,
    /// Where the water lies about each vertex, in the same order: how far inside it, meters
    /// (negative outside it), and how far below the vertex its level lies, meters. Empty for a
    /// world with no water.
    pub water: std::sync::Arc<Vec<[f32; 2]>>,
}

/// One block's offset: a sample a meter, 193 a side with both edges, row by row from the south.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnTile {
    /// The landblock, `0xXXYY`.
    pub block: u16,
    /// Bumped whenever the offset is worked out again.
    pub generation: u64,
    /// The offset bodies stand on, meters.
    pub offset: std::sync::Arc<Vec<f32>>,
    /// How much of the offset the pins leave: 0 at a building, 1 free.
    pub weight: std::sync::Arc<Vec<f32>>,
}

/// The numbers the drawn ground is drawn with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnKnobs {
    /// How densely the ground is meshed: 1 a meter a quad near the eye, 2 half a meter.
    pub detail: f32,
    /// The fine relief's height and size, meters.
    pub micro: (f32, f32),
    /// The fine relief and the terraces leveled under bodies: the level round's radius and fade,
    /// meters.
    pub level: Option<(f32, f32)>,
    /// Terraces on steep walkable ground: their height and cap, meters, and the slope they start
    /// at, degrees.
    pub terraces: Option<(f32, f32, f32)>,
    /// Where the near ground starts and ends giving way to the far, meters from the eye.
    pub morph: (f32, f32),
    /// The far ground: the share of the rounding kept far off, the eroded relief's height and
    /// size, meters, and how far the ground is drawn.
    pub far: Option<(f32, f32, f32, f32)>,
    /// How thick the air over the far ground is.
    pub haze: f32,
    /// The clouds that shade the land, with an art pack.
    pub clouds: Option<DrawnClouds>,
    /// The bed lowered under the water: its deepest, and how far from the shore it gets there,
    /// meters; `None` while the water is not drawn over its bed.
    pub water: Option<(f32, f32)>,
    /// The scree and rock a pack's ground lays by its slope; `None` while none is laid.
    pub slope: Option<DrawnSlope>,
    /// What a pack's ground lays by the water, while the water is drawn over its bed: how much
    /// grass grows in over a muddy bank, and how much darker the wet ground along the water is.
    pub bank: (f32, f32),
}

/// The numbers a pack's ground lays scree and rock by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnSlope {
    /// The slope each is laid from and the slopes it blends in over, degrees.
    pub scree: (f32, f32),
    pub rock: (f32, f32),
    /// How much of them is laid over the ground's own material.
    pub strength: f32,
    /// How much the slope that lays them is the smooth surface's.
    pub smooth: f32,
    /// How much of them shows through snow and ice.
    pub on_snow: f32,
}

impl Default for DrawnKnobs {
    fn default() -> Self {
        Self {
            detail: 1.0,
            micro: (0.0, 0.7),
            level: None,
            terraces: None,
            morph: (300.0, 600.0),
            far: None,
            water: None,
            slope: None,
            haze: 1.0,
            clouds: None,
            bank: (0.0, 0.0),
        }
    }
}

/// How retail moves the distances of a degrade record's levels: a level hands over to the next
/// once the distance, less [`Self::distance`] and over the part's z scale, passes its threshold,
/// which [`Self::bias`] slides from its ideal toward its nearest (below 0) or farthest (above).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DegradeBias {
    pub bias: f32,
    /// The distance taken off every distance first, metres.
    pub distance: f32,
    /// Every part draws its first level.
    pub disabled: bool,
    /// Every part draws this level, or the last it has; negative when none is forced.
    pub force: i32,
}

impl Default for DegradeBias {
    fn default() -> Self {
        Self {
            bias: 0.0,
            distance: 50.0,
            disabled: false,
            force: -1,
        }
    }
}

/// Where the frame is seen from.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Camera {
    /// From the scene's space to the eye's.
    pub view: Mat4,
    /// From the eye's space to the clip space Retail Look draws in.
    pub projection: Mat4,
    /// The last frame's view and projection together, in this frame's space; `None` on the
    /// first frame.
    pub previous_view_projection: Option<Mat4>,
    /// The eye, in the scene's space.
    pub eye: Vec3,
    /// The world's rectangle on the back buffer: x, y, width and height.
    pub viewport: [u32; 4],
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

/// The light inputs of the frame, exactly as Retail Look reads them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RetailLight {
    /// The sun: its direction scaled by its brightness, not normalized.
    pub sun: Vec3,
    pub sun_color: [u8; 3],
    /// The landscape's ambient level and color.
    pub ambient_level: f32,
    pub ambient_color: [u8; 3],
    /// The ambient objects are drawn with, as the device is given it.
    pub object_ambient: [f32; 3],
    /// The world's fog, when it is on.
    pub fog: Option<Fog>,
    /// Whether the viewer is outdoors.
    pub outdoor: bool,
    /// Whether the viewer's cell sees the outdoors (always, outdoors).
    pub sees_outside: bool,
    /// The time of day, 0 to 1.
    pub time_of_day: f32,
    /// The light pools: the static lights, then the dynamic ones, each in the order the pool
    /// sorted them. [`DrawnPart::lights`] indexes it.
    pub lights: Vec<Light>,
    /// How many of [`Self::lights`] are static.
    pub statics: usize,
}

/// The world's fog.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Fog {
    pub start: f32,
    pub end: f32,
    pub color: [u8; 3],
}

/// One light of the light pools, in the frame's space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Light {
    pub kind: LightKind,
    /// The cell it is in.
    pub cell: u32,
    pub position: Vec3,
    pub direction: Vec3,
    /// Its color, as the pool rounds it.
    pub color: [f32; 3],
    pub intensity: f32,
    pub falloff: f32,
    pub cone_angle: f32,
}

/// One object of the sky.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyObject {
    /// The GfxObj or Setup it draws.
    pub gfx: u32,
    pub properties: u32,
    /// Where it stands this frame, in the frame's space.
    pub frame: Mat4,
    /// Whether it is drawn before the ground, and after it.
    pub before: bool,
    pub after: bool,
    /// Its translucency, diffusion and luminosity, when any is not its default.
    pub material: Option<Material>,
    /// How far its texture has scrolled.
    pub uv_offset: [f32; 2],
    /// Whether it is drawn fogged: retail fogs the whole sky while the region's visual
    /// environment override is on, whatever its surfaces say.
    pub fogged: bool,
    /// Its picture laid flat from above, for a deck of cloud.
    pub deck: Option<SkyDeck>,
}

/// A deck whose picture is laid flat from above.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyDeck {
    /// `u = a + b x` and `v = c + d y` over the frame's meters, as `[a, b, c, d]`.
    pub laying: [f32; 4],
    /// How high over the viewer it lies at its highest, meters.
    pub height: f32,
}

/// One particle the walk drew, in its order: far to near, merged with the parts it draws among.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleDraw {
    /// The card's graphics object.
    pub gfx: u32,
    /// Where it stands as drawn, turned to the viewer and scaled, in the frame's space.
    pub transform: Mat4,
    /// How far it has faded (0 opaque, 1 gone), and the diffusion and luminosity its card is lit
    /// with.
    pub material: Material,
    /// Whether it is lit by the outdoor light, the sun alone.
    pub outdoors: bool,
    /// Whether it is lit at all.
    pub lit: bool,
    /// Indoors, the lights it is lit by, as indices into [`RetailLight::lights`], and how many.
    pub lights: [u16; 8],
    pub count: u8,
}

/// A part's translucency, diffusion and luminosity, as an animation or the server set them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    pub translucency: f32,
    pub diffusion: f32,
    pub luminosity: f32,
}

/// The level each degrading placement of one block draws at this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockLevels {
    pub block: BlockId,
    /// One per [`Block::degrades`], in its order.
    pub levels: Vec<u8>,
    /// The degrading parts turned toward the viewer this frame: the record's index, and where
    /// the part stands as it is drawn, block-local, its scale left out.
    pub turned: Vec<(u32, Mat4)>,
}

/// One object's pose this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectPose {
    pub key: ObjectKey,
    /// The cell it stands in.
    pub cell: u32,
    /// One per [`Object::parts`].
    pub parts: Vec<PartPose>,
}

/// One object part's pose this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartPose {
    /// Its mesh transform this frame, its scale applied first.
    pub transform: Mat4,
    /// Its mesh transform last frame, in this frame's space; this frame's on its first.
    pub previous: Mat4,
    /// The level it draws at.
    pub level: u8,
    /// Whether it is hidden.
    pub hidden: bool,
    pub material: Option<Material>,
}

/// One object part the walk drew.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnPart {
    pub key: ObjectKey,
    pub part: u32,
    /// Whether it was drawn with the outdoor light, the sun alone.
    pub outdoors: bool,
    /// The lights it was drawn with indoors, as indices into [`RetailLight::lights`]: the first
    /// `count` of them.
    pub lights: [u16; 8],
    pub count: u8,
}

/// What takes the scene's changes and its frames: a renderer, or a [`Retained`] scene.
pub trait Receiver: std::fmt::Debug {
    /// One frame: the changes since the last, in order, then the frame's own state.
    fn receive(&mut self, deltas: Vec<SceneDelta>, frame: FrameDelta);
}

/// A receiver shared with whoever else reads it: each frame goes to the one it holds.
impl<R: Receiver> Receiver for std::rc::Rc<std::cell::RefCell<R>> {
    fn receive(&mut self, deltas: Vec<SceneDelta>, frame: FrameDelta) {
        self.borrow_mut().receive(deltas, frame);
    }
}
