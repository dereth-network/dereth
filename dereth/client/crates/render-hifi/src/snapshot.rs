//! The frame snapshot: a plain owned copy of what the presentation needs to know about the frame
//! the scene has just walked.
//!
//! The scene builds it after the frame's draw has finished, from shared reads only, so building it
//! cannot change what the frame drew or what can be picked in it. Nothing here refers back into
//! the scene: every field is owned data, and the snapshot is `Send + 'static`.

use std::ops::Range;

use dereth_render::VertexFormat;
use glam::{Mat4, Vec3};

/// The snapshot of one frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HifiFrame {
    /// The device's frame stamp of the frame this snapshot describes. A snapshot whose stamp is
    /// not the frame being ended is not used for it.
    pub stamp: u64,
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
}

impl HifiCamera {
    /// World to clip.
    #[must_use]
    pub fn view_projection(&self) -> Mat4 {
        self.projection * self.view
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
}

/// The snapshot crosses to the device by value and holds no borrow.
const _: () = {
    const fn plain<T: Send + 'static>() {}
    plain::<HifiFrame>();
};
