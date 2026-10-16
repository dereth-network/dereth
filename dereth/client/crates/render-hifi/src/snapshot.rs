//! The frame snapshot: a plain owned copy of what the presentation needs to know about the frame
//! the scene has just walked.
//!
//! The scene builds it after the frame's draw has finished, from shared reads only, so building it
//! cannot change what the frame drew or what can be picked in it. Nothing here refers back into
//! the scene: every field is owned data, and the snapshot is `Send + 'static`.

use std::ops::Range;
use std::sync::Arc;

use dereth_render::VertexFormat;
use glam::{Mat4, Quat, Vec2, Vec3};

/// The snapshot of one frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiFrame {
    /// The device's frame stamp of the frame this snapshot describes. A snapshot whose stamp is
    /// not the frame being ended is not used for it.
    pub stamp: u64,
    /// The scene's clock at this frame, seconds. What the presentation eases or animates over
    /// time is timed by it, so the same frames come out the same however fast they are drawn.
    pub time: f64,
    /// The camera.
    pub camera: HifiCamera,
    /// The sun, the ambient light, the fog and the sky.
    pub sky: HifiSky,
    /// The point lights in the world.
    pub lights: Vec<HifiLight>,
    /// The outdoor lamps of the resident blocks: lights the data places on outdoor objects, which
    /// the ordinary renderer never lights the world with.
    pub lamps: Vec<HifiLamp>,
    /// The resident landscape blocks.
    pub terrain: Vec<HifiTerrainBlock>,
    /// Retained geometry of blocks whose contents changed since the last snapshot.
    pub static_feed: Vec<HifiStaticBlock>,
    /// The art pack whose models may be drawn in place of the objects they replace, while one
    /// is open.
    pub art_pack: Option<Arc<HifiArtPack>>,
    /// Where the art pack's models stand in blocks whose contents changed since the last
    /// snapshot.
    pub pack_feed: Vec<HifiPackBlock>,
    /// The art pack's ground and what it is laid with, while a pack that lays one is open and
    /// the ground is laid as retail lays it by blending pictures.
    pub ground: Option<Arc<HifiGround>>,
    /// The art pack's materials drawn in place of retail pictures, while a pack that has them is
    /// open.
    pub materials: Option<Arc<HifiMaterials>>,
    /// The texture slots holding, this frame, a retail picture one of [`Self::materials`] stands
    /// for, each with that material's index: a draw that binds such a slot is drawn with the
    /// material.
    pub material_slots: Vec<(u32, u32)>,
}

/// An art pack's materials drawn in place of retail pictures. Made once when the pack is opened;
/// every frame shares it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiMaterials {
    /// A number of its own, so a pack opened again is told from the one before.
    pub serial: u64,
    /// The materials, by the index [`HifiFrame::material_slots`] names them with.
    pub materials: Vec<HifiTextureMaterial>,
}

/// One material drawn in place of a retail picture, on the retail surface's own coordinates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiTextureMaterial {
    /// The base color with the retail picture's alpha, read as the retail picture is: in the
    /// display's encoding, as stored, so the surface's stages compute with it exactly as they
    /// compute with the retail picture.
    pub albedo: HifiPackImage,
    /// The normal's parts along the picture's width and up it (-1..1 as 0..1), in red and green.
    pub normal: HifiPackImage,
    /// The roughness and the ambient occlusion, in red and green.
    pub surface: HifiPackImage,
}

/// The camera of the frame. The matrices are the frame's own, in the column-vector convention.
/// They take the frame's drawing space, which holds a point's height on its second axis and its
/// north on its third; every point and direction of the snapshot (the eye, block origins,
/// lights, the sun) holds height on its third axis. So a render-space point `p` is at
/// `projection * view * HEIGHT_UP * p` in clip space, with
/// [`HEIGHT_UP`](crate::shared::camera::HEIGHT_UP) the swap of the two.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HifiCamera {
    /// Drawing space to view.
    pub view: Mat4,
    /// View to clip, as the frame drew.
    pub projection: Mat4,
    /// The previous frame's world to clip, for reprojection; `None` on the first frame or after
    /// a cut.
    pub previous_view_projection: Option<Mat4>,
    /// The eye, in render space.
    pub eye: Vec3,
    /// The world viewport in target pixels: x, y, width, height.
    pub viewport: [u32; 4],
    /// The vertical field of view, radians.
    pub fov_y: f32,
    /// Width over height.
    pub aspect: f32,
    /// The near plane distance.
    pub near: f32,
    /// The far plane distance.
    pub far: f32,
    /// The landblock the viewer stands in, `0xXXYY`.
    pub viewer_landblock: u16,
    /// What render space is shifted by from world space, so far blocks keep precision.
    pub block_shift: Vec3,
    /// How far, in normalised device coordinates, the presentation shifts the frame's world on
    /// screen for its temporal anti-aliasing: a fraction of a pixel, a new one each frame. The
    /// presentation sets it itself before its passes prepare; the scene leaves it nought, and it
    /// stays nought on a frame drawn without the anti-aliasing. `projection` is never shifted:
    /// a pass that draws the world's geometry itself draws it through [`Self::jittered`].
    pub jitter: Vec2,
    /// The frame's place in the anti-aliasing's sequence for the edges of cut-out surfaces, which
    /// it dithers so that frames of them average to soft edges: 1 and over (the fraction is the
    /// place), nought on a frame drawn without the anti-aliasing.
    pub dither: f32,
}

impl HifiCamera {
    /// World to clip.
    #[must_use]
    pub fn view_projection(&self) -> Mat4 {
        self.projection * self.view
    }

    /// `clip_from` (anything to clip space, through this camera) shifted on screen by the
    /// frame's [`Self::jitter`].
    #[must_use]
    pub fn jittered(&self, clip_from: Mat4) -> Mat4 {
        Mat4::from_translation(self.jitter.extend(0.0)) * clip_from
    }
}

/// The sun, ambient light, fog and sky of the frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiSky {
    /// The direction toward the sun, unit length.
    pub sun_direction: Vec3,
    /// The sun's brightness: the length of the authored sun vector.
    pub sun_brightness: f32,
    /// The sun's colour.
    pub sun_color: [u8; 3],
    /// The ambient light level.
    pub ambient_level: f32,
    /// The ambient light colour.
    pub ambient_color: [u8; 3],
    /// The world fog, when the time of day has one.
    pub fog: Option<HifiFog>,
    /// The sky objects in view at this time of day.
    pub objects: Vec<HifiSkyObject>,
    /// The name of the day's weather group.
    pub day_group: String,
    /// Whether weather is drawn.
    pub weather_enabled: bool,
    /// Whether the viewer is outdoors.
    pub outdoor: bool,
    /// Whether the viewer sees the outdoors: outdoors, or in a room whose openings show it.
    pub sees_outside: bool,
    /// The form of the falling weather the player asked for, if any.
    pub asked_fall: AskedFall,
    /// The time of day, 0..1.
    pub time_of_day: f32,
    /// The in-game year.
    pub year: u32,
    /// The in-game day of the year.
    pub day: u32,
}

/// The form of the falling weather the player asked for, which takes the place of the land's
/// say in it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AskedFall {
    /// Nothing asked: the land round the viewer says whether rain or snow falls.
    #[default]
    Land,
    /// Rain, whatever the land.
    Rain,
    /// Snow, whatever the land.
    Snow,
}

/// The world fog.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HifiFog {
    /// Where the fog begins, metres.
    pub min: f32,
    /// Where it is full, metres.
    pub max: f32,
    /// Its colour.
    pub color: [u8; 3],
}

/// One sky object at this time of day.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HifiSkyObject {
    /// Its index in the day group.
    pub index: u32,
    /// The model drawn for it.
    pub gfx_id: u32,
    /// Its fixed heading, degrees; 0 for none.
    pub heading: f32,
    /// Its elevation sweep, degrees.
    pub rotation: f32,
    /// Transparency percent, or -1 when unset.
    pub transparent: f32,
    /// Luminosity percent, or -1 when unset.
    pub luminosity: f32,
    /// Maximum brightness percent, or -1 when unset.
    pub max_bright: f32,
    /// Its property bits.
    pub properties: u32,
}

/// One point light.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HifiLight {
    /// Its position, render space.
    pub position: Vec3,
    /// Its colour, linear 0..1.
    pub color: [f32; 3],
    /// Its intensity.
    pub intensity: f32,
    /// Its falloff distance, metres.
    pub falloff: f32,
    /// Whether it stands in a building's or dungeon's own cell, so lights only that interior.
    pub interior: bool,
    /// Whether its light is burned into the vertex colors of the rooms around it: one of the
    /// static lights the rooms' light is made from, with the color and place it was made with.
    pub burned: bool,
}

/// Where an outdoor lamp's light was found in the data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum LampSource {
    /// The object's own authored point light.
    #[default]
    Authored,
    /// A fire-coloured particle emitter the object starts: a torch, a brazier, a lamp's flame.
    Flame,
    /// A warm glowing surface: lamp glass, a lantern's panes.
    Glow,
}

/// One outdoor lamp.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HifiLamp {
    /// Its position, render space.
    pub position: Vec3,
    /// Its colour, linear, brightest channel 1.
    pub color: [f32; 3],
    /// Its authored intensity (authored lights are mostly 100), or 1 for a found one.
    pub intensity: f32,
    /// Its falloff distance, metres.
    pub falloff: f32,
    /// Where it was found.
    pub source: LampSource,
    /// Whether it flickers like a flame.
    pub flicker: bool,
    /// A number of its own, for the flicker's phase.
    pub seed: u32,
}

/// One resident landscape block.
#[derive(Debug, Clone, PartialEq)]
pub struct HifiTerrainBlock {
    /// The landblock id, `0xXXYY`.
    pub block: u16,
    /// Its detail ring: 0 full detail, higher farther.
    pub ring: u8,
    /// Its south-west corner, render space.
    pub origin: Vec3,
    /// The 9 by 9 terrain words.
    pub words: [u16; 81],
    /// Landscape vertices along one side as drawn: 9 at full detail, fewer on far rings.
    pub side: u8,
    /// The drawn height of every vertex, metres, `side` by `side`, indexed
    /// `east * side + north`.
    pub heights: Vec<f32>,
    /// Each cell's cut, `side - 1` by `side - 1` in the same order: true for the south-west to
    /// north-east diagonal.
    pub cuts: Vec<bool>,
    /// Bumped whenever the block's drawn contents are rebuilt.
    pub generation: u32,
    /// The layers of each of its 64 cells as retail lays them at full detail, in the words
    /// [`crate::passes::ground::cell_words`] makes, east by north (`east * 8 + north`), while an
    /// art pack's ground is laid.
    pub layers: Option<Arc<[[u32; 4]; 64]>>,
}

impl Default for HifiTerrainBlock {
    fn default() -> Self {
        Self {
            block: 0,
            ring: 0,
            origin: Vec3::ZERO,
            words: [0; 81],
            side: 0,
            heights: Vec::new(),
            cuts: Vec::new(),
            generation: 0,
            layers: None,
        }
    }
}

/// The retained geometry of one block, sent when its contents change.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiStaticBlock {
    /// The landblock id.
    pub block: u16,
    /// The block's generation this geometry belongs to.
    pub generation: u32,
    /// The block's static batches.
    pub batches: Vec<HifiStaticBatch>,
    /// Every landscape triangle of the block, block-local, three corners each.
    pub terrain_triangles: Vec<[Vec3; 3]>,
}

/// One static batch: vertices in one format.
#[derive(Debug, Clone, PartialEq)]
pub struct HifiStaticBatch {
    /// The vertex format.
    pub format: VertexFormat,
    /// The vertex bytes.
    pub bytes: Vec<u8>,
    /// The byte ranges of the batch's draws.
    pub ranges: Vec<Range<u32>>,
    /// Whether the batch is cut out by its texture's alpha (tested or blended), as leaves are.
    pub cutout: bool,
    /// The texture slot it is drawn with, when it has one.
    pub texture: Option<u32>,
    /// The alpha below which its pixels are cut.
    pub alpha_ref: u8,
    /// Whether it holds only objects the art pack has models for, which the pack's models stand
    /// in for while they are drawn.
    pub art_pack: bool,
}

/// An art pack as the presentation draws it: models for retail objects, each with its levels of
/// detail, and the pictures they are drawn with. Made once when the pack is opened; every frame
/// shares it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiArtPack {
    /// A number of its own, so a pack opened again is told from the one before.
    pub serial: u64,
    /// The models.
    pub models: Vec<HifiPackModel>,
    /// The pictures the models' materials name by index.
    pub images: Vec<HifiPackImage>,
}

/// One model of an art pack.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiPackModel {
    /// The retail object it is drawn in place of: a setup or a graphics object.
    pub id: u32,
    /// Cut-out growth: its cut-out materials (needles, leaves) are drawn from both sides and lit
    /// through from behind.
    pub foliage: bool,
    /// Its levels of detail, nearest first.
    pub lods: Vec<HifiPackLod>,
    /// A sphere holding every level, in the model's own space (height up): its centre and
    /// radius.
    pub bounds: (Vec3, f32),
}

/// One level of detail of a model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiPackLod {
    /// How far from the eye it is drawn, metres; infinite for a last level with no limit.
    pub distance: f32,
    /// Its parts, one material each.
    pub primitives: Vec<HifiPackPrimitive>,
    /// For a level that is pictures of the model seen from round it: how many views, evenly
    /// round its upright axis, and how many tiles to a row of its pictures. Its parts are upright
    /// quads across the axis, turned to face the eye; each part's base colour picture holds the
    /// views' colours and coverage, and its normal picture each view's normal in the model's
    /// own space and, in alpha, how far the surface stands along the view from the plane through
    /// the axis (0.5 on the plane, 0 and 1 half the quad's width nearer and farther).
    pub views: Option<(u32, u32)>,
    /// Drawn into the sun's shadows alone, where they ask for a coarser level than the view
    /// draws, and never into the view.
    pub shadows_only: bool,
}

/// One part of a level of detail: triangles of one material.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiPackPrimitive {
    /// The vertices, [`PACK_VERTEX_FLOATS`] floats each: position (3), normal (3), tangent with
    /// its handedness (4), texture coordinates (2) and colour (4), in the model's own space,
    /// height up.
    pub vertices: Arc<Vec<f32>>,
    /// Three indices a triangle, counter-clockwise seen from its front.
    pub indices: Arc<Vec<u32>>,
    /// How it is drawn.
    pub material: HifiPackMaterial,
}

/// The floats of one vertex of a pack's model.
pub const PACK_VERTEX_FLOATS: usize = 16;

/// A physically based material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HifiPackMaterial {
    /// The base colour, linear, and its alpha, which the base colour picture is multiplied by.
    pub base_color: [f32; 4],
    /// The base colour picture.
    pub base_color_image: Option<u32>,
    /// The tangent-space normal picture.
    pub normal_image: Option<u32>,
    /// How strongly the normal picture bends the normal.
    pub normal_scale: f32,
    /// The picture whose green holds the roughness and blue the metalness.
    pub metal_rough_image: Option<u32>,
    /// The metalness, which the picture's blue is multiplied by.
    pub metallic: f32,
    /// The roughness, which the picture's green is multiplied by.
    pub roughness: f32,
    /// The picture whose red holds the ambient occlusion.
    pub occlusion_image: Option<u32>,
    /// How much of the occlusion is applied, 0..1.
    pub occlusion_strength: f32,
    /// The alpha below which a pixel is cut away, for a cut-out material.
    pub alpha_cutoff: Option<f32>,
    /// Whether both faces are drawn.
    pub double_sided: bool,
}

impl Default for HifiPackMaterial {
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            base_color_image: None,
            normal_image: None,
            normal_scale: 1.0,
            metal_rough_image: None,
            metallic: 1.0,
            roughness: 1.0,
            occlusion_image: None,
            occlusion_strength: 1.0,
            alpha_cutoff: None,
            double_sided: false,
        }
    }
}

/// One picture of an art pack, with its smaller levels: eight bits a channel, or compressed in
/// blocks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiPackImage {
    /// Width of the largest level, texels.
    pub width: u32,
    /// Height of the largest level, texels.
    pub height: u32,
    /// Whether it holds colour in the display's encoding (a base colour), rather than data
    /// (normals, roughness, occlusion).
    pub srgb: bool,
    /// Every level, largest first: four bytes a texel, rows packed, or the level's blocks in
    /// [`Self::format`], rows of blocks packed.
    pub levels: Vec<Arc<Vec<u8>>>,
    /// The blocks the levels are compressed in; `None` for four bytes a texel.
    pub format: Option<dereth_render::block_codec::BlockFormat>,
}

/// An art pack's ground: a material for each retail ground, and retail's blend masks that lay
/// them. Made once when the pack is opened or the ground changes; every frame shares it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiGround {
    /// A number of its own, so a ground made again is told from the one before.
    pub serial: u64,
    /// The materials, every picture of every material one size.
    pub materials: Vec<HifiGroundMaterial>,
    /// For each retail ground (terrain types 0 to 31, and 32 the road), the index of the
    /// material drawn for it; `None` where none is, and for a ground past the list's end.
    pub of_ground: Vec<Option<u16>>,
    /// Retail's blend masks, by the index a cell's layers name them with: each picture's red
    /// keeps what lies under the layer (255) or takes the layer (0).
    pub masks: Vec<HifiPackImage>,
    /// Each retail ground's own picture, shrunk to its patches a fifth of a metre and broader, all
    /// one size, with its repeats across a cell (terrain types 0 to 31, then the road): what the
    /// pack's materials are laid out in, so they keep the character of the ground they replace.
    pub broad: Vec<(HifiPackImage, u32)>,
}

/// One ground material. Its normal picture is the size of its base colour; its surface picture
/// is half that, and every material's are the same sizes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiGroundMaterial {
    /// The base colour, in the display's encoding, with the height in alpha.
    pub albedo: HifiPackImage,
    /// The normal's east and north parts (-1..1 as 0..1), in red and green.
    pub normal: HifiPackImage,
    /// The roughness and the ambient occlusion, in red and green.
    pub surface: HifiPackImage,
    /// How many metres one repeat of the pictures covers.
    pub size: f32,
    /// How strongly the normal picture bends the normal.
    pub normal_scale: f32,
    /// Retail's own picture for a ground the pack has no material for: laid as retail lays it,
    /// its repeats square with the cells, with no detail or variation of the pack's.
    pub retail: bool,
}

/// Where an art pack's models stand in one block.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiPackBlock {
    /// The landblock id.
    pub block: u16,
    /// The block's build these placements belong to.
    pub generation: u32,
    /// The placements.
    pub placements: Vec<HifiPackPlacement>,
}

/// One object the art pack draws in place of the retail one.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HifiPackPlacement {
    /// The retail object, which names the model.
    pub id: u32,
    /// Where it stands, relative to the block's south-west corner, height up.
    pub origin: Vec3,
    /// How it is turned.
    pub rotation: Quat,
    /// How it is scaled, the same on every axis.
    pub scale: f32,
}

/// The snapshot crosses to the device by value and holds no borrow.
const _: () = {
    const fn plain<T: Send + 'static>() {}
    plain::<HifiFrame>();
};
