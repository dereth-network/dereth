//! Rain, snow, splashes and wet ground, and the share of the flat ground that is puddle. No mist
//! and no haze: the game's own fog for the day is the only veil, so the sky and the far land are
//! drawn as the game draws them.
//!
//! The weather falls when the game's own does: on a rainy day, with the game's weather drawn,
//! while the viewer sees the outdoors. Over land that is mostly snow and ice it falls as snow; the
//! player may ask for rain or snow whatever the land. While it falls, the game's own falling
//! weather is left out of the sky, so only one precipitation is ever seen.
//!
//! - **Wet.** The G-buffer's surfaces near the viewer that stand out in the rain are darkened and
//!   deepened in color as soaked material is, and their roughness lowered (`weather.slang`), while
//!   the rain falls. The puddles alone leave everything but themselves as it is.
//! - **Puddles.** Flat ground gathers them where its terrain takes them, from a mask fixed to the
//!   world, written into the ground's material for the water pass to draw. They are the water's
//!   own switch: they gather on a rainy day whether the rain is drawn or not, and not under
//!   falling snow.
//! - **Rain and snow.** Particles fixed in the world, drawn over the scene's colors after the
//!   forward pass (`rain.slang`); splashes where the drops land near the viewer.
//! - **Shelter.** A depth of the resident statics seen from straight above round the eye, drawn
//!   again only when the eye moves far enough or what is resident changes, keeps all of it off
//!   what stands under a roof.
//!
//! Nothing switches in one frame: the rain and the snow, and the weather and a dry day, ease into
//! each other over a few seconds, and the land's share of snow must pass well over or under its
//! mark before the form changes. Every motion is timed by the scene's clock and repeats over a
//! long period of it, so a held clock draws a still frame and nothing coarsens as a session goes
//! on.

use dereth_gfx::{
    Blend, BufferUsage, ColorAttachment, ColorTarget, Compare, Cull, DepthAttachment, DepthState,
    Device, Factor, Format, GraphicsDesc, IndexType, Load, Pipeline, RasterPass, Recorder, Rect,
    Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::{Mat4, Vec3, Vec4};

use super::light::Rows;
use super::scene::{Bucket, GpuScene};
use super::{push_words, shader, PerFrame, Targets};
use crate::scene::{AskedFall, BlockId, Camera, FrameDelta};
use crate::shaders::Entry;

/// A block's side, meters.
const BLOCK: f32 = 192.0;
/// A landscape cell's side, meters.
const CELL: f32 = 24.0;
/// The period the weather's clock wraps over, seconds. The shaders fit a whole number of cycles
/// of every motion into it, so the wrap is never seen.
pub const TIME_PERIOD: f64 = 1200.0;
/// How far round the viewer the land's share of snow is taken, meters; each vertex counts less
/// the farther it is.
pub const SNOW_REACH: f32 = BLOCK * 1.5;
/// The share of snow and ice in the land round the viewer over which rain turns to snow.
pub const SNOW_ABOVE: f32 = 0.45;
/// The share under which snow turns back to rain.
pub const RAIN_BELOW: f32 = 0.35;
/// The share that decides the form when the weather starts.
const SNOW_START: f32 = 0.4;
/// How long the weather takes to come or go, or to turn between rain and snow, seconds.
pub const EASE_SECONDS: f32 = 3.0;
/// How fast the rain falls, meters a second.
pub const RAIN_SPEED: f32 = 9.0;
/// How fast the snow falls, meters a second.
pub const SNOW_SPEED: f32 = 1.1;
/// The period the drops' and flakes' lattices repeat over in the world, and their fall and drift
/// wrap over, meters: a whole number of every lattice's box, so no wrap is ever seen.
pub const LATTICE_PERIOD: f64 = 1440.0;
/// How many drops each of the rain's three lattices holds, nearest first.
pub const RAIN_PARTICLES: [u32; 3] = [1200, 4000, 5000];
/// How many flakes each of the snow's holds.
pub const SNOW_PARTICLES: [u32; 3] = [3000, 8000, 12000];
/// Where the wet starts to fade with distance, and where it is gone, meters.
pub const WET_FADE: [f32; 2] = [120.0, 260.0];
/// The overhead map's side, texels.
pub const COVER_SIDE: u32 = 2048;
/// How far the overhead map reaches from its centre, meters.
pub const COVER_REACH: f32 = 128.0;
/// The steps the overhead map's centre moves in, meters: it is drawn again only when the eye
/// crosses one.
pub const COVER_STEP: f32 = 32.0;
/// How far above and below the eye the overhead map sees, meters, and the steps its top moves in.
const COVER_DEPTH_REACH: f32 = 300.0;
const COVER_DEPTH_STEP: f32 = 64.0;
/// The overhead map's depth.
const COVER_FORMAT: Format = Format::D32Float;
/// The particles' quads: two triangles each.
const QUAD_VERTICES: u32 = 6;

/// How the weather is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WeatherSettings {
    /// Whether the rain and the snow fall on the game's rainy days, with the rain's splashes and
    /// the wet it leaves. The puddles are the water's own switch.
    pub on: bool,
}

/// What falls from the sky.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precipitation {
    /// Nothing.
    #[default]
    None,
    /// Rain.
    Rain,
    /// Snow.
    Snow,
}

/// Whether the weather falls in `frame`'s world on a day that is `rainy`: with the game's
/// weather drawn, and the outdoors seen.
#[must_use]
pub fn falls(frame: &FrameDelta, rainy: bool) -> bool {
    rainy && frame.weather.enabled && frame.landscape
}

/// The share of the land round `eye` (the frame's space, its origin at block `origin`) that is
/// snow and ice: every landscape vertex of the held blocks within [`SNOW_REACH`] of the eye, the
/// nearer the more it counts. `None` with no land in reach.
#[must_use]
pub fn snow_share<'a>(
    blocks: impl Iterator<Item = (BlockId, &'a [u16; 81])>,
    eye: Vec3,
    origin: [i32; 2],
) -> Option<f32> {
    let (mut snow, mut all) = (0.0f32, 0.0f32);
    for (id, words) in blocks {
        let o = block_offset(id, origin);
        let (cx, cy) = (o.x + BLOCK * 0.5, o.y + BLOCK * 0.5);
        let half = BLOCK * 0.5 + SNOW_REACH;
        if (cx - eye.x).abs() > half || (cy - eye.y).abs() > half {
            continue;
        }
        for (i, w) in words.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)] // a vertex's place in its block, 0..9
            let (e, n) = ((i / 9) as f32, (i % 9) as f32);
            let x = o.x + e * CELL - eye.x;
            let y = o.y + n * CELL - eye.y;
            let weight = 1.0 - (x * x + y * y).sqrt() / SNOW_REACH;
            if weight <= 0.0 {
                continue;
            }
            all += weight;
            if is_snow(*w) {
                snow += weight;
            }
        }
    }
    (all > 0.0).then(|| snow / all)
}

/// Whether a landscape vertex of terrain word `word` is snow or ice (and no road).
#[must_use]
pub fn is_snow(word: u16) -> bool {
    word & 3 == 0 && matches!((word >> 2) & 0x1F, 2 | 15)
}

/// How readily ground of terrain type `t` gathers puddles, 0 to 3: a road the most, dirt and
/// marsh less, grass a little; rock, sand, snow, ice and water never.
#[must_use]
pub fn puddle_weight(t: u32) -> u32 {
    match t {
        // The road's own terrain type.
        0x20 => 3,
        // Marsh, rich dirt, packed dirt, patchy dirt.
        4 | 5 | 7 | 8 => 2,
        // Grassland, lush grass, patchy grassland.
        1 | 3 | 9 => 1,
        _ => 0,
    }
}

/// How readily ground of landscape vertex word `word` gathers puddles, 0 to 3: its road's, or
/// its terrain type's.
#[must_use]
pub fn word_puddle_weight(word: u16) -> u32 {
    if word & 3 != 0 {
        3
    } else {
        puddle_weight(u32::from((word >> 2) & 0x1F))
    }
}

/// How readily a cell with corners of terrain words `corners` gathers puddles, 0 to 3: their
/// weights' mean, rounded.
#[must_use]
pub fn cell_puddle_weight(corners: [u16; 4]) -> u32 {
    let sum: u32 = corners.iter().map(|w| word_puddle_weight(*w)).sum();
    (sum + 2) / 4
}

/// How the weather stands, eased from frame to frame so that nothing about it switches in one
/// frame: how much falls, and how much of it is snow.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Fall {
    amount: f32,
    snow: f32,
    snowy: bool,
    started: bool,
}

impl Fall {
    /// How much falls: 0 a dry day, 1 the weather in full.
    #[must_use]
    pub fn amount(&self) -> f32 {
        self.amount
    }

    /// How much of what falls is snow: 0 rain, 1 snow.
    #[must_use]
    pub fn snow(&self) -> f32 {
        self.snow
    }

    /// What mostly falls.
    #[must_use]
    pub fn precipitation(&self) -> Precipitation {
        if self.amount <= 0.0 {
            Precipitation::None
        } else if self.snow >= 0.5 {
            Precipitation::Snow
        } else {
            Precipitation::Rain
        }
    }

    /// Step `dt` seconds toward the weather `falls` asks for, over land whose share of snow is
    /// `share` (`None` where there is no land to judge by: the form is kept), or in the form the
    /// player asked for. The first step takes the weather as it stands at once; every later one
    /// eases toward it.
    pub fn step(&mut self, falls: bool, share: Option<f32>, asked: AskedFall, dt: f32) {
        self.snowy = match (asked, share) {
            (AskedFall::Rain, _) => false,
            (AskedFall::Snow, _) => true,
            (AskedFall::Land, Some(s)) if !self.started => s >= SNOW_START,
            (AskedFall::Land, Some(s)) if s >= SNOW_ABOVE => true,
            (AskedFall::Land, Some(s)) if s <= RAIN_BELOW => false,
            (AskedFall::Land, _) => self.snowy,
        };
        let amount = if falls { 1.0 } else { 0.0 };
        let snow = if self.snowy { 1.0 } else { 0.0 };
        if !self.started {
            self.started = true;
            self.amount = amount;
            self.snow = snow;
            return;
        }
        let by = (dt / EASE_SECONDS).max(0.0);
        let toward = |from: f32, to: f32| from + (to - from).clamp(-by, by);
        self.amount = toward(self.amount, amount);
        self.snow = toward(self.snow, snow);
    }
}

/// The weather's clock: the scene's seconds since the weather first ticked, and the step since
/// the frame before. It follows the scene's clock, so the same frames fall alike however fast
/// they are drawn, and a held clock holds the weather still.
#[derive(Debug, Default, Clone, Copy)]
pub struct Clock {
    /// The scene's clock when the weather's began.
    start: Option<f64>,
    /// The scene's clock at the frame before.
    last: Option<f64>,
}

impl Clock {
    /// The clock's reading at the scene's `now`, seconds: the time within [`TIME_PERIOD`] and the
    /// step since the last reading. A scene clock that went back (a new scene) starts the
    /// weather's again.
    pub fn tick(&mut self, now: f64) -> (f32, f32) {
        if self.last.is_some_and(|last| now < last) {
            *self = Self::default();
        }
        let start = *self.start.get_or_insert(now);
        let reading = clock_reading(now - start, self.last.map(|last| last - start));
        self.last = Some(now);
        reading
    }
}

/// The weather's clock at `now` seconds since it began, the time before at `last`: the time
/// within [`TIME_PERIOD`] the shaders take, and the step since `last`. Both are taken in double
/// precision before they are narrowed, so neither coarsens however long a session runs.
#[must_use]
pub fn clock_reading(now: f64, last: Option<f64>) -> (f32, f32) {
    #[allow(clippy::cast_possible_truncation)] // a time within the period, and a frame's step
    (
        now.rem_euclid(TIME_PERIOD) as f32,
        last.map_or(0.0, |l| (now - l) as f32),
    )
}

/// The wind the weather drifts on at the scene's clock `now`, seconds: a light breeze whose
/// strength and heading turn slowly, meters a second east and north.
#[must_use]
pub fn wind(now: f64) -> [f32; 2] {
    use dereth_primitives::num::math::{cosf, sinf};
    let tau = std::f32::consts::TAU;
    #[allow(clippy::cast_possible_truncation)] // a share of a period
    let turn = (now / (TIME_PERIOD * 4.0)).rem_euclid(1.0) as f32;
    let strength = sinf(turn * tau * 3.0) * 0.25 + 0.55;
    let heading = turn * tau;
    [cosf(heading) * strength, sinf(heading) * strength]
}

/// How far the rain and the snow have fallen and the wind has carried them, meters, each
/// wrapped to [`LATTICE_PERIOD`]: added to step by step, so a change of wind moves the drops on
/// from where they are rather than to somewhere else.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Drift {
    /// How far the rain has fallen.
    pub rain: f64,
    /// How far the snow has fallen.
    pub snow: f64,
    /// How far the wind has carried both, east and north.
    pub east: f64,
    pub north: f64,
}

impl Drift {
    /// Step `dt` seconds on, the wind blowing at `wind` meters a second east and north.
    pub fn step(&mut self, dt: f32, wind: [f32; 2]) {
        let dt = f64::from(dt.max(0.0));
        let on = |v: f64, by: f64| (v + by).rem_euclid(LATTICE_PERIOD);
        self.rain = on(self.rain, f64::from(RAIN_SPEED) * dt);
        self.snow = on(self.snow, f64::from(SNOW_SPEED) * dt);
        self.east = on(self.east, f64::from(wind[0]) * dt);
        self.north = on(self.north, f64::from(wind[1]) * dt);
    }
}

/// Where the overhead map stands: its centre (the frame's space) in whole steps of
/// [`COVER_STEP`], how far it reaches, and the heights its depth spans.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cover {
    pub centre: [f32; 2],
    pub reach: f32,
    /// The height its depth starts at: depth 0 is this high, depth 1 is `span` below it.
    pub top: f32,
    pub span: f32,
}

impl Cover {
    /// The map round `eye`.
    #[must_use]
    pub fn around(eye: Vec3) -> Self {
        let snap = |v: f32| (v / COVER_STEP).round() * COVER_STEP;
        Self {
            centre: [snap(eye.x), snap(eye.y)],
            reach: COVER_REACH,
            top: ((eye.z + COVER_DEPTH_REACH) / COVER_DEPTH_STEP).ceil() * COVER_DEPTH_STEP,
            span: 2.0 * COVER_DEPTH_REACH,
        }
    }

    /// The view and projection that draw the map: straight down, the frame's `x` across and `y`
    /// up the map, the depth 0 at [`Self::top`].
    #[must_use]
    pub fn view_proj(&self) -> Mat4 {
        let (r, s) = (self.reach, self.span);
        Mat4::from_cols(
            Vec4::new(1.0 / r, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0 / r, 0.0, 0.0),
            Vec4::new(0.0, 0.0, -1.0 / s, 0.0),
            Vec4::new(-self.centre[0] / r, -self.centre[1] / r, self.top / s, 1.0),
        )
    }

    /// Whether a sphere at `centre` of `radius` reaches into the map.
    #[must_use]
    pub fn reaches(&self, centre: Vec3, radius: f32) -> bool {
        (centre.x - self.centre[0]).abs() <= self.reach + radius
            && (centre.y - self.centre[1]).abs() <= self.reach + radius
            && centre.z - radius <= self.top
            && centre.z + radius >= self.top - self.span
    }
}

/// What placed the overhead map last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CoverKey {
    centre: [u32; 2],
    top: u32,
    resident: u64,
    origin: [i32; 2],
}

impl CoverKey {
    fn of(c: &Cover, resident: u64, origin: [i32; 2]) -> Self {
        Self {
            centre: c.centre.map(f32::to_bits),
            top: c.top.to_bits(),
            resident,
            origin,
        }
    }
}

/// What the weather counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WeatherStats {
    /// Frames the weather's constants were written for, and frames the rain or the snow fell in.
    pub frames: u64,
    pub falling_frames: u64,
    /// The last frame's: how much fell and how much of it was snow, in thousandths.
    pub falling: u32,
    pub snow: u32,
    /// The particles it drew, and the game's falling-weather sky objects it left out.
    pub particles: u32,
    pub sky_left_out: u32,
    /// How many times the overhead map was drawn, and the items it was last drawn with.
    pub cover_drawn: u64,
    pub cover_items: u32,
}

/// The overhead map's resources.
#[derive(Debug, Clone, Copy)]
struct CoverMap {
    map: Texture,
    index: u32,
}

/// The weather's pass.
#[derive(Debug)]
pub struct WeatherPass {
    settings: WeatherSettings,
    caster: Pipeline,
    /// The splashes and the falling particles, for colors in display values, and for the post
    /// passes' linear colors.
    splash: [(Format, Pipeline); 2],
    fall_pipelines: [(Format, Pipeline); 2],
    cover: Option<CoverMap>,
    key: Option<CoverKey>,
    cover_frame: PerFrame,
    cover_commands: PerFrame,
    constants: PerFrame,
    fall: Fall,
    drift: Drift,
    clock: Clock,
    /// The clock's reading this frame, seconds within its period.
    time: f32,
    /// Whether the rain or the snow falls this frame.
    drawing: bool,
    pub stats: WeatherStats,
}

/// One frame of the weather, as [`WeatherPass::prepare`] made it.
#[derive(Debug, Clone)]
pub struct WeatherPlan {
    /// The weather's constants, for the G-buffer's shaders: their index in the table.
    pub table: u32,
    cover: CoverMap,
    /// The overhead map's frame constants and commands, when it is drawn again this frame.
    redraw: Option<(u32, dereth_gfx::Buffer, u32, u32)>,
    caster: Pipeline,
    /// The splashes' and the particles' pipelines and count, when the rain or the snow falls.
    falling: Option<(Pipeline, Pipeline, u32)>,
}

impl WeatherPass {
    /// The weather, off, its pipelines made now.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(d: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let caster = d.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster weather cover",
            vertex: shader(Entry::GbufferVsMain),
            fragment: Some(shader(Entry::GbufferFsCaster)),
            color: &[],
            depth: Some(DepthState {
                format: COVER_FORMAT,
                compare: Some(Compare::LessEqual),
                write: true,
            }),
            cull: Cull::None,
        })?;
        let linear = super::post::scene_format(d);
        let mut make = |label, vertex, fragment, format, blend| {
            d.create_graphics_pipeline(&GraphicsDesc {
                label,
                vertex: shader(vertex),
                fragment: Some(shader(fragment)),
                color: &[ColorTarget {
                    format,
                    blend: Some(blend),
                    write_alpha: false,
                }],
                depth: None,
                cull: Cull::None,
            })
            .map(|p| (format, p))
        };
        let add = Blend {
            src: Factor::One,
            dst: Factor::One,
        };
        let over = Blend {
            src: Factor::One,
            dst: Factor::OneMinusSrcAlpha,
        };
        let splash = [
            make(
                "remaster splashes",
                Entry::FullscreenVsMain,
                Entry::RainFsSplash,
                super::COLOR,
                add,
            )?,
            make(
                "remaster splashes",
                Entry::FullscreenVsMain,
                Entry::RainFsSplash,
                linear,
                add,
            )?,
        ];
        let fall_pipelines = [
            make(
                "remaster rain",
                Entry::RainVsFall,
                Entry::RainFsFall,
                super::COLOR,
                over,
            )?,
            make(
                "remaster rain",
                Entry::RainVsFall,
                Entry::RainFsFall,
                linear,
                over,
            )?,
        ];
        Ok(Self {
            settings: WeatherSettings::default(),
            caster,
            splash,
            fall_pipelines,
            cover: None,
            key: None,
            cover_frame: PerFrame::new(
                d,
                "remaster weather cover frame",
                512,
                BufferUsage::default(),
            )?,
            cover_commands: PerFrame::new(
                d,
                "remaster weather cover statics",
                1 << 16,
                BufferUsage::INDIRECT,
            )?,
            constants: PerFrame::new(d, "remaster weather constants", 256, BufferUsage::default())?,
            fall: Fall::default(),
            drift: Drift::default(),
            clock: Clock::default(),
            time: 0.0,
            drawing: false,
            stats: WeatherStats::default(),
        })
    }

    /// How the weather is drawn from the next frame on.
    pub fn set_settings(&mut self, settings: WeatherSettings) {
        self.settings = settings;
    }

    /// How the weather is drawn.
    #[must_use]
    pub fn settings(&self) -> WeatherSettings {
        self.settings
    }

    /// How the weather stands.
    #[must_use]
    pub fn fall(&self) -> Fall {
        self.fall
    }

    /// Take the frame's weather before anything of it is drawn: the clock, the drift and how
    /// much falls, eased, over the held blocks `ground` (each block's terrain words), on a day
    /// that is `rainy`. While the weather is off, nothing of it moves.
    pub fn observe<'a>(
        &mut self,
        frame: &FrameDelta,
        rainy: bool,
        ground: impl Iterator<Item = (BlockId, &'a [u16; 81])>,
    ) {
        if !self.settings.on {
            self.drawing = false;
            return;
        }
        let (time, step) = self.clock.tick(frame.clock);
        self.time = time;
        self.drift.step(step, wind(frame.clock));
        let share = snow_share(ground, frame.camera.eye, frame.origin);
        self.fall
            .step(falls(frame, rainy), share, frame.weather.asked, step);
        self.drawing = frame.landscape && self.fall.amount() > 0.0;
    }

    /// Whether the sky object of `properties` is left out of the sky: the game's own falling
    /// weather, while the rain or the snow falls.
    #[must_use]
    pub fn leaves_out(&self, properties: u32) -> bool {
        self.drawing && properties & 4 != 0
    }

    /// Write the frame's weather to the device: its constants, and the overhead map's commands
    /// when it is drawn again; `None` when nothing of it is drawn. `frame_rows` are the frame's
    /// constants, `camera` the view it is drawn from, `puddles` whether the flat ground gathers
    /// puddles this frame, `linear` whether the post passes draw the scene's colors, and
    /// `height` the world's height in pixels.
    ///
    /// # Errors
    /// What the device refuses.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(
        &mut self,
        d: &mut Device,
        scene: &GpuScene,
        frame: &FrameDelta,
        camera: &Camera,
        frame_rows: &[u8],
        puddles: bool,
        linear: bool,
        height: u32,
        shelter: bool,
    ) -> Result<Option<WeatherPlan>, dereth_gfx::Error> {
        let puddles = puddles && frame.landscape;
        // The overhead map is kept on a dry day too while something else asks what stands
        // overhead.
        if !self.drawing && !puddles && !(shelter && frame.landscape) {
            return Ok(None);
        }
        let cover = self.cover_map(d)?;
        let placed = Cover::around(camera.eye);
        let key = CoverKey::of(&placed, statics_held(scene), frame.origin);
        let redraw = if self.key == Some(key) {
            None
        } else {
            let mut rows = frame_rows.to_vec();
            if rows.len() >= 64 {
                rows[..64].copy_from_slice(&floats(&placed.view_proj().to_cols_array()));
            }
            self.cover_frame.write(d, &rows)?;
            let stride = super::command_bytes(d);
            let commands = cover_commands(d.backend(), scene, &placed, frame.origin);
            let count = u32::try_from(commands.len() / stride as usize).unwrap_or(u32::MAX);
            self.cover_commands.write(d, &commands)?;
            self.key = Some(key);
            self.stats.cover_drawn += 1;
            self.stats.cover_items = count;
            Some((
                self.cover_frame.index,
                self.cover_commands.buffer,
                count,
                stride,
            ))
        };

        let amount = if self.drawing {
            self.fall.amount()
        } else {
            0.0
        };
        let snow = self.fall.snow();
        // The ground gathers puddles on a rainy day, but not under falling snow; it is wet only
        // while the rain falls: the puddles change nothing but the ground they lie on.
        let gathers = match (puddles, self.drawing) {
            (false, _) => 0.0,
            (true, true) => 1.0 - snow,
            (true, false) => 1.0,
        };
        let wet = amount * (1.0 - snow);
        let rain_ends = ends(RAIN_PARTICLES, 0);
        let snow_ends = ends(SNOW_PARTICLES, rain_ends[2]);
        let particles = if !self.drawing {
            0
        } else if snow > 0.0 {
            snow_ends[2]
        } else {
            rain_ends[2]
        };
        let origin = [
            f64::from(frame.origin[0]) * f64::from(BLOCK),
            f64::from(frame.origin[1]) * f64::from(BLOCK),
        ];
        let w = wind(frame.clock);
        // Radians a pixel, from the projection's vertical scale.
        #[allow(clippy::cast_precision_loss)] // a target's height
        let pixel = 2.0 / (camera.projection.y_axis.y.abs().max(1e-6) * height.max(1) as f32);
        let mut rows = Rows::default();
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        // Meters and seconds narrowed for the device; counts of particles.
        {
            rows.0
                .push([wet, gathers, self.time, snow].map(f32::to_bits));
            rows.0.push(
                [origin[0] as f32, origin[1] as f32, WET_FADE[0], WET_FADE[1]].map(f32::to_bits),
            );
            rows.0.push(
                [
                    placed.centre[0],
                    placed.centre[1],
                    placed.reach,
                    COVER_SIDE as f32,
                ]
                .map(f32::to_bits),
            );
            rows.0.push([
                placed.top.to_bits(),
                placed.span.to_bits(),
                cover.index,
                1.0f32.to_bits(),
            ]);
            rows.0.push(
                [
                    self.drift.rain as f32,
                    self.drift.snow as f32,
                    self.drift.east as f32,
                    self.drift.north as f32,
                ]
                .map(f32::to_bits),
            );
            rows.0
                .push([RAIN_SPEED, SNOW_SPEED, w[0], w[1]].map(f32::to_bits));
            rows.0.push(
                [
                    origin[0].rem_euclid(LATTICE_PERIOD) as f32,
                    origin[1].rem_euclid(LATTICE_PERIOD) as f32,
                    LATTICE_PERIOD as f32,
                    TIME_PERIOD as f32,
                ]
                .map(f32::to_bits),
            );
            rows.0.push(
                [
                    rain_ends[0] as f32,
                    rain_ends[1] as f32,
                    rain_ends[2] as f32,
                    amount,
                ]
                .map(f32::to_bits),
            );
            rows.0.push(
                [
                    snow_ends[0] as f32,
                    snow_ends[1] as f32,
                    snow_ends[2] as f32,
                    pixel,
                ]
                .map(f32::to_bits),
            );
        }
        self.constants.write(d, &rows.bytes())?;

        let format = if linear {
            super::post::scene_format(d)
        } else {
            super::COLOR
        };
        let pick =
            |set: &[(Format, Pipeline); 2]| set.iter().find(|(f, _)| *f == format).map(|p| p.1);
        let falling = match (pick(&self.splash), pick(&self.fall_pipelines)) {
            (Some(s), Some(f)) if self.drawing => Some((s, f, particles)),
            _ => None,
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..1 to 0..1000
        let thousandths = |v: f32| (v.clamp(0.0, 1.0) * 1000.0).round() as u32;
        self.stats.frames += 1;
        self.stats.falling_frames += u64::from(self.drawing);
        self.stats.falling = thousandths(amount);
        self.stats.snow = if self.drawing { thousandths(snow) } else { 0 };
        self.stats.particles = particles;
        self.stats.sky_left_out = u32::try_from(
            frame
                .sky
                .iter()
                .filter(|o| o.before && self.leaves_out(o.properties))
                .count(),
        )
        .unwrap_or(u32::MAX);
        Ok(Some(WeatherPlan {
            table: self.constants.index,
            cover,
            redraw,
            caster: self.caster,
            falling,
        }))
    }

    /// The overhead map, made on first use.
    fn cover_map(&mut self, d: &mut Device) -> Result<CoverMap, dereth_gfx::Error> {
        if let Some(c) = self.cover {
            return Ok(c);
        }
        let map = d.create_texture(&TextureDesc {
            label: "remaster weather cover",
            width: COVER_SIDE,
            height: COVER_SIDE,
            mips: 1,
            format: COVER_FORMAT,
            usage: TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED,
        })?;
        let c = CoverMap {
            map,
            index: d.bind_texture(map)?,
        };
        self.cover = Some(c);
        Ok(c)
    }
}

/// What of the statics the scene holds: each held block and its generation, folded together. The
/// overhead map is drawn again when it changes, and not when only a block's ground does, which
/// the map never holds.
fn statics_held(scene: &GpuScene) -> u64 {
    scene
        .blocks
        .iter()
        .fold(0xCBF2_9CE4_8422_2325, |h, (id, b)| {
            (h ^ (u64::from(*id) << 32 | u64::from(b.generation))).wrapping_mul(0x0100_0000_01B3)
        })
}

/// Where each of three lattices of `counts` ends among the instances, counted on from `from`.
fn ends(counts: [u32; 3], from: u32) -> [u32; 3] {
    let a = from + counts[0];
    let b = a + counts[1];
    [a, b, b + counts[2]]
}

/// The overhead map's commands: every held block's static parts that reach into it, at their
/// finest level, their solid surfaces only. Leaves and other cut-out cards are left out: from
/// above they are a scatter of crossed cards whose outline would print on the ground as a
/// hard-edged dry patch.
fn cover_commands(
    backend: dereth_gfx::Backend,
    scene: &GpuScene,
    cover: &Cover,
    origin: [i32; 2],
) -> Vec<u8> {
    let mut out = Vec::new();
    for (id, b) in &scene.blocks {
        let offset = block_offset(*id, origin);
        for p in &b.parts {
            let centre = p.transform.transform_point3(Vec3::from(p.sphere.0)) + offset;
            let scale = p
                .transform
                .x_axis
                .truncate()
                .length()
                .max(p.transform.y_axis.truncate().length())
                .max(p.transform.z_axis.truncate().length());
            if !cover.reaches(centre, p.sphere.1 * scale) {
                continue;
            }
            let Some(items) = p.levels.first() else {
                continue;
            };
            for i in items.clone() {
                let solid = matches!(
                    scene.buckets.get(i as usize),
                    Some(Bucket::Opaque | Bucket::OpaqueTwoSided)
                );
                let Some(d) = scene.item_draws.get(i as usize).filter(|d| !d.alpha_test) else {
                    continue;
                };
                if solid {
                    out.extend(super::command(backend, d.index_count, d.first_index, i));
                }
            }
        }
    }
    out
}

/// One block's offset from the frame's origin.
#[allow(clippy::cast_precision_loss)] // a few hundred blocks
fn block_offset(id: BlockId, origin: [i32; 2]) -> Vec3 {
    let (x, y) = (i32::from(id >> 8), i32::from(id & 0xFF));
    Vec3::new(
        (x - origin[0]) as f32 * BLOCK,
        (y - origin[1]) as f32 * BLOCK,
        0.0,
    )
}

fn floats(f: &[f32]) -> Vec<u8> {
    f.iter().flat_map(|x| x.to_le_bytes()).collect()
}

impl WeatherPlan {
    /// The overhead map and its index in the table, which the G-buffer's passes read.
    #[must_use]
    pub fn cover(&self) -> Texture {
        self.cover.map
    }

    /// Draw the overhead map again, where it moved: the statics' solid surfaces from straight
    /// above. `caster_push` gives the meshes' push constants for the map's frame constants, and
    /// `indices` is the scene's index buffer.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn record_cover(
        &self,
        r: &mut Recorder<'_>,
        indices: dereth_gfx::Buffer,
        caster_push: &dyn Fn(u32) -> Vec<u8>,
    ) -> Result<(), dereth_gfx::Error> {
        let Some((frame, commands, count, stride)) = self.redraw else {
            return Ok(());
        };
        r.timestamp("weather cover");
        r.raster(
            &RasterPass {
                name: "weather cover",
                color: &[],
                depth: Some(DepthAttachment {
                    texture: self.cover.map,
                    clear: Some(1.0),
                    write: true,
                }),
                touches: &[
                    Touch::Buffer(commands, Use::Indirect),
                    Touch::Buffer(indices, Use::Index),
                ],
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: COVER_SIDE,
                    height: COVER_SIDE,
                }),
            },
            |dr| {
                if count > 0 {
                    dr.set_pipeline(self.caster)?;
                    dr.push(0, &caster_push(frame))?;
                    dr.bind_index_buffer(indices, 0, IndexType::U32)?;
                    dr.draw_indexed_indirect(commands, 0, count, stride)?;
                }
                Ok(())
            },
        )
    }

    /// Draw the splashes and the falling rain and snow over the scene's colors in `t`, when they
    /// fall: `frame` the frame's constants' index, `outdoor_depth` the depth the outdoors are
    /// placed by.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn record_fall(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        outdoor_depth: (Texture, u32),
    ) -> Result<(), dereth_gfx::Error> {
        let Some((splash, fall, particles)) = self.falling else {
            return Ok(());
        };
        r.timestamp("rain");
        let mut touches = vec![
            Touch::Texture(t.depth, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
            Touch::Texture(t.normal, Use::Sampled),
            Touch::Texture(self.cover.map, Use::Sampled),
        ];
        if outdoor_depth.0 != t.depth {
            touches.push(Touch::Texture(outdoor_depth.0, Use::Sampled));
        }
        let push = push_words(&[
            frame,
            self.table,
            t.index[4],
            outdoor_depth.1,
            t.index[3],
            t.index[1],
        ]);
        r.raster(
            &RasterPass {
                name: "rain",
                color: &[ColorAttachment {
                    texture: t.color,
                    load: Load::Keep,
                }],
                depth: None,
                touches: &touches,
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: t.width,
                    height: t.height,
                }),
            },
            |dr| {
                dr.set_pipeline(splash)?;
                dr.push(0, &push)?;
                dr.draw(3, 1, 0, 0);
                if particles > 0 {
                    dr.set_pipeline(fall)?;
                    dr.push(0, &push)?;
                    dr.draw(QUAD_VERTICES, particles, 0, 0);
                }
                Ok(())
            },
        )
    }
}

impl super::Renderer {
    /// How the weather is drawn from the next frame on.
    pub fn set_weather(&mut self, settings: WeatherSettings) {
        self.weather.set_settings(settings);
    }

    /// How the weather is drawn.
    #[must_use]
    pub fn weather(&self) -> WeatherSettings {
        self.weather.settings()
    }

    /// What the weather counts.
    #[must_use]
    pub fn weather_stats(&self) -> WeatherStats {
        self.weather.stats
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    fn frame(rainy_group: bool, enabled: bool, landscape: bool) -> (FrameDelta, bool) {
        let mut f = FrameDelta {
            landscape,
            ..FrameDelta::default()
        };
        f.weather.enabled = enabled;
        (f, rainy_group)
    }

    /// The weather falls on a rainy day with the game's weather drawn and the outdoors seen, and
    /// on no other.
    #[test]
    fn it_falls_on_a_rainy_day_with_the_games_weather_drawn_outdoors() {
        for (rainy, enabled, landscape, want) in [
            (true, true, true, true),
            (false, true, true, false),
            (true, false, true, false),
            (true, true, false, false),
        ] {
            let (f, r) = frame(rainy, enabled, landscape);
            assert_eq!(falls(&f, r), want, "{rainy} {enabled} {landscape}");
        }
    }

    /// The first step takes the weather at once; later ones ease over [`EASE_SECONDS`], so it never
    /// switches in one frame.
    #[test]
    fn the_weather_eases_in_and_out() {
        let mut f = Fall::default();
        f.step(true, Some(0.0), AskedFall::Land, 0.0);
        assert!((f.amount() - 1.0).abs() < 1e-6);
        assert_eq!(f.precipitation(), Precipitation::Rain);
        f.step(false, Some(0.0), AskedFall::Land, 1.0);
        assert!((f.amount() - (1.0 - 1.0 / EASE_SECONDS)).abs() < 1e-5);
        for _ in 0..10 {
            f.step(false, Some(0.0), AskedFall::Land, 1.0);
        }
        assert_eq!(f.amount(), 0.0);
        assert_eq!(f.precipitation(), Precipitation::None);
    }

    /// Over the land, the form turns to snow only above [`SNOW_ABOVE`] and back to rain only
    /// below [`RAIN_BELOW`]; asked for, it falls in the form asked whatever the land.
    #[test]
    fn the_form_follows_the_land_with_a_margin_or_the_ask() {
        let mut f = Fall::default();
        f.step(true, Some(0.2), AskedFall::Land, 0.0);
        assert_eq!(f.snow(), 0.0);
        for share in [0.4, 0.44] {
            f.step(true, Some(share), AskedFall::Land, 10.0);
            assert_eq!(f.snow(), 0.0, "{share}");
        }
        f.step(true, Some(0.46), AskedFall::Land, 10.0);
        assert_eq!(f.snow(), 1.0);
        f.step(true, Some(0.36), AskedFall::Land, 10.0);
        assert_eq!(f.snow(), 1.0);
        f.step(true, Some(0.34), AskedFall::Land, 10.0);
        assert_eq!(f.snow(), 0.0);
        f.step(true, Some(0.0), AskedFall::Snow, 10.0);
        assert_eq!(f.precipitation(), Precipitation::Snow);
        f.step(true, Some(1.0), AskedFall::Rain, 10.0);
        assert_eq!(f.precipitation(), Precipitation::Rain);
        // With no land to judge by, the form is kept.
        f.step(true, None, AskedFall::Land, 10.0);
        assert_eq!(f.precipitation(), Precipitation::Rain);
    }

    /// The land's share of snow counts its snow and ice vertices, the nearer the more, and no
    /// land in reach says nothing.
    #[test]
    fn the_lands_snow_share_counts_the_snowy_vertices_near_the_eye() {
        let snow_word = 15 << 2;
        let grass_word = 1 << 2;
        let snowy = [snow_word; 81];
        let grassy = [grass_word; 81];
        let id: BlockId = 0x1020;
        let eye = Vec3::new(96.0, 96.0, 0.0);
        let all = snow_share([(id, &snowy)].into_iter(), eye, [0x10, 0x20]).expect("land");
        assert!((all - 1.0).abs() < 1e-6);
        let none = snow_share([(id, &grassy)].into_iter(), eye, [0x10, 0x20]).expect("land");
        assert_eq!(none, 0.0);
        let far = snow_share([(id, &snowy)].into_iter(), eye, [0x40, 0x20]);
        assert_eq!(far, None);
        // A road over snow is a road.
        assert!(!is_snow(snow_word | 1));
        assert!(is_snow(2 << 2));
    }

    /// Roads gather the most puddles, then dirt, then grass; rock, sand, snow and water none; a
    /// cell takes its corners' mean.
    #[test]
    fn roads_gather_the_most_puddles_and_rock_sand_snow_and_water_none() {
        assert_eq!(puddle_weight(0x20), 3);
        assert_eq!(puddle_weight(7), 2);
        assert_eq!(puddle_weight(3), 1);
        for t in [0, 2, 6, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20] {
            assert_eq!(puddle_weight(t), 0, "{t}");
        }
        assert_eq!(word_puddle_weight(15 << 2 | 2), 3);
        assert_eq!(cell_puddle_weight([1 << 2; 4]), 1);
        assert_eq!(cell_puddle_weight([7 << 2, 7 << 2, 0, 0]), 1);
        assert_eq!(cell_puddle_weight([1, 1, 1, 1]), 3);
    }

    /// The weather's clock follows the scene's: its step is the scene's, it starts again when the
    /// scene's goes back, and its reading wraps without losing precision.
    #[test]
    fn the_clock_follows_the_scenes_and_wraps() {
        let mut c = Clock::default();
        assert_eq!(c.tick(5000.0), (0.0, 0.0));
        let (t, dt) = c.tick(5000.5);
        assert!((t - 0.5).abs() < 1e-6 && (dt - 0.5).abs() < 1e-6);
        assert_eq!(c.tick(10.0), (0.0, 0.0));
        let (t, _) = clock_reading(TIME_PERIOD * 1000.0 + 0.25, None);
        assert!((t - 0.25).abs() < 1e-3);
    }

    /// The drift is added to step by step and wraps over the lattices' period.
    #[test]
    fn the_drift_steps_on_and_wraps() {
        let mut d = Drift::default();
        d.step(2.0, [1.0, -1.0]);
        assert!((d.rain - f64::from(RAIN_SPEED) * 2.0).abs() < 1e-9);
        assert!((d.east - 2.0).abs() < 1e-9);
        assert!((d.north - (LATTICE_PERIOD - 2.0)).abs() < 1e-9);
    }

    /// The lattices' boxes each fit the lattices' period a whole number of times, so no wrap is
    /// ever seen.
    #[test]
    fn every_lattice_box_fits_the_period_whole() {
        for size in [
            12.0, 36.0, 96.0, 12.0, 24.0, 48.0, 10.0, 30.0, 80.0, 20.0, 40.0,
        ] {
            let n = LATTICE_PERIOD / size;
            assert!((n - n.round()).abs() < 1e-9, "{size}");
        }
    }

    /// The overhead map stands still while the eye moves within a step, moves by whole steps,
    /// and draws straight down: a point under its centre at its top is at depth 0, one a span
    /// below at depth 1.
    #[test]
    fn the_overhead_map_moves_in_steps_and_looks_straight_down() {
        let a = Cover::around(Vec3::new(10.0, 10.0, 50.0));
        let b = Cover::around(Vec3::new(14.0, 12.0, 52.0));
        assert_eq!(a, b);
        let c = Cover::around(Vec3::new(50.0, 10.0, 50.0));
        assert_eq!(c.centre, [64.0, 0.0]);
        let m = a.view_proj();
        let top = m * Vec4::new(a.centre[0], a.centre[1], a.top, 1.0);
        let bottom = m * Vec4::new(a.centre[0], a.centre[1], a.top - a.span, 1.0);
        let corner = m * Vec4::new(a.centre[0] + a.reach, a.centre[1] + a.reach, a.top, 1.0);
        assert!(top.z.abs() < 1e-6 && (bottom.z - 1.0).abs() < 1e-6);
        assert!((corner.x - 1.0).abs() < 1e-6 && (corner.y - 1.0).abs() < 1e-6);
        assert!(a.reaches(Vec3::new(a.centre[0] + 130.0, a.centre[1], 50.0), 3.0));
        assert!(!a.reaches(Vec3::new(a.centre[0] + 140.0, a.centre[1], 50.0), 3.0));
    }

    /// Off, nothing of the weather runs or moves.
    #[test]
    fn off_nothing_falls() {
        assert!(!WeatherSettings::default().on);
    }
}
