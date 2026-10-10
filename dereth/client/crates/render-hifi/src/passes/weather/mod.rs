//! Rain, snow, splashes and wet ground. No mist and no haze: the game's own fog for the day is
//! the only veil, so the sky and the far land are drawn as the game draws them.
//!
//! The weather falls when the game's own sky does: on a day whose weather group is a rainy one,
//! with weather drawn, and it is drawn while the viewer sees the outdoors, from outside or from a
//! room whose door or window shows it. Over land that is mostly snow and ice it falls as snow;
//! the player may ask for rain or snow whatever the land. While it falls, one full-screen pass
//! over the replayed world draws it into the picture, and the game's own falling-rain layer is
//! left out of the replay, so only one precipitation is ever seen:
//!
//! - **Wet ground.** Surfaces near the viewer are darkened and saturated the way soaked material
//!   is; surfaces facing the sky take a sheen of the sky, and flat ground gathers puddles, from a
//!   noise mask fixed to the world, that mirror the world on screen, their mirror stirred a little
//!   by the drops' rings. Where the mirror finds nothing it is sure of on screen (the reflection
//!   leaves the picture, or runs behind something nearer, such as the viewer's own body) the puddle
//!   holds the sky, fading into it rather than ending in a hole or a smear. Puddles fade with
//!   distance, and their reflection is held down at grazing angles, so far puddles never read as
//!   white sheets.
//! - **Rain.** Streaks of drops fixed in the world: every drop has its own place in the air,
//!   falls at the drops' speed and drifts with a light wind, and holds still in the world however
//!   the viewer walks or turns or swings the camera round. The drops fill boxes round the viewer,
//!   dense near and sparse far, and a drop leaving a box far off fades out before the next comes
//!   in at its other side. Each streak is as long as a camera shutter would blur it (no longer on
//!   screen near the eye than a set length), is thinned to the pixel so it never shimmers, fades
//!   to nothing within a metre of the eye, and is hidden behind anything nearer than it.
//! - **Splashes.** Short-lived crowns where the drops land on ground facing up.
//! - **Snow.** Flakes instead of rain, fixed in the world as the drops are, each of its own size
//!   in a small range and swaying as it falls, never drawn wider on screen than a set size. The
//!   tops of solid things (roofs, rocks, platforms) take a cover of snow where they face the sky:
//!   whole where they are level, thinning on a slope and gone where it is steep, a plain white lit
//!   as the surface under it is lit, carrying a little of its relief. The land the game paints as
//!   snow takes a lighter, even cover the same way; other land, open water, leaves and bodies take
//!   none.
//! - **Shelter.** A roof or a porch keeps all of it off what stands under it: a depth of the
//!   solid static geometry seen from straight above says where. From inside a room the weather
//!   is drawn over what its openings show, with the depth that was drawn there before the room
//!   was, and the room itself, under its roof, stays dry.
//!
//! Nothing switches in one frame: the rain and the snow, and the weather and a dry day, ease
//! into each other over a few seconds, and the land's share of snow must pass well over or
//! under its mark before the form changes. Every motion repeats over a long period of the
//! clock, so it never coarsens as a session goes on. Nothing but the picture changes.

pub mod cover;

use crate::graph::{EncodeCx, HifiPass, PrepareCx};
use crate::shared::camera::{render_from_clip, HEIGHT_UP};
use crate::shared::fullscreen::{COLOUR_WGSL, FULLSCREEN_WGSL};
use crate::shared::terrain_field::{self, FieldSignature, GroundClass, TerrainField, CELL_METRES};
use crate::snapshot::AskedFall;
use crate::{
    Caps, DrawAction, DrawNote, HifiError, HifiFrame, HifiSettings, Level, Mark, ReplayFilter,
    SkipRule,
};

const WEATHER_WGSL: &str = include_str!("weather.wgsl");

/// A block's side, metres.
const BLOCK: f32 = 192.0;
/// The constant block's size, bytes.
const CONSTANTS_SIZE: usize = 108 * 4;
/// Where the flag for linear light in and out sits in the constants, in floats.
const LINEAR_AT: usize = 51;
/// Where the flag for a re-shaded picture sits, in floats.
const RESHADED_AT: usize = 87;
/// The period of the world-fixed noise, metres: the shader's noise takes the render-space
/// position plus the render space's place in the world wrapped to this, and every noise lattice
/// repeats over it, so the pattern stays put in the world when render space moves.
const NOISE_PERIOD: f32 = 2000.0;
/// The period the weather's clock wraps over, seconds. The shader fits a whole number of cycles
/// of every motion into it, so the wrap is never seen, and the clock never grows large enough for
/// its steps to show.
pub const TIME_PERIOD: f64 = 1200.0;
/// How far round the viewer the land's share of snow is taken, metres; each vertex counts less
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
/// How fast the rain falls, metres a second.
pub const RAIN_SPEED: f32 = 9.0;
/// How fast the snow falls, metres a second.
pub const SNOW_SPEED: f32 = 1.1;
/// The period the drops' and flakes' lattices repeat over in the world, and their fall and drift
/// wrap over, metres: a whole number of every lattice's box, so no wrap is ever seen.
pub const LATTICE_PERIOD: f64 = 1440.0;
/// How many drops each of rain's three lattices holds at the High level, nearest first. Low draws
/// half as many, Medium three quarters, and Ultra half as many again.
pub const RAIN_PARTICLES: [u32; 3] = [1200, 4000, 5000];
/// How many flakes each of snow's holds, the same way.
pub const SNOW_PARTICLES: [u32; 3] = [3000, 8000, 12000];

/// The shader's source, complete.
#[must_use]
pub fn shader_source() -> String {
    [
        FULLSCREEN_WGSL.to_owned(),
        COLOUR_WGSL.to_owned(),
        terrain_field::wgsl(),
        WEATHER_WGSL.to_owned(),
    ]
    .join("\n")
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

/// Whether the weather falls in `frame`'s world: on a day of a rainy weather group (the
/// calendar's, or the one the player asked for), with weather drawn. Indoors it still falls
/// outside.
#[must_use]
pub fn falls(frame: &HifiFrame) -> bool {
    frame.sky.weather_enabled && frame.sky.day_group.to_ascii_lowercase().contains("rain")
}

/// The share of the land round the viewer that is snow and ice: every landscape vertex within
/// [`SNOW_REACH`] of the eye, the nearer the more it counts. `None` with no land in reach.
#[must_use]
pub fn snow_share(frame: &HifiFrame) -> Option<f32> {
    let eye = frame.camera.eye;
    let (mut snow, mut all) = (0.0f32, 0.0f32);
    for b in &frame.terrain {
        let (cx, cy) = (b.origin.x + BLOCK * 0.5, b.origin.y + BLOCK * 0.5);
        let half = BLOCK * 0.5 + SNOW_REACH;
        if (cx - eye.x).abs() > half || (cy - eye.y).abs() > half {
            continue;
        }
        for (i, w) in b.words.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)] // a vertex's place in its block, 0..9
            let (e, n) = ((i / 9) as f32, (i % 9) as f32);
            let x = b.origin.x + e * CELL_METRES - eye.x;
            let y = b.origin.y + n * CELL_METRES - eye.y;
            let weight = 1.0 - dereth_primitives::num::math::hypotf(x, y) / SNOW_REACH;
            if weight <= 0.0 {
                continue;
            }
            all += weight;
            if GroundClass::of_word(*w) == GroundClass::Snow {
                snow += weight;
            }
        }
    }
    (all > 0.0).then(|| snow / all)
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

/// What a capture may ask of the weather, read once when the pass is made, and only in a build
/// for captures: a view of the masks, and a clock that steps by frames.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Overrides {
    /// `DERETH_HIFI_WEATHER_DEBUG`: 1 the masks, 2 the shelter.
    debug: f32,
    /// `DERETH_HIFI_WEATHER_FRAME_CLOCK`: the clock steps by frames, this many a second (sixty
    /// when it names no number).
    frame_clock: Option<f64>,
}

impl Overrides {
    #[cfg(feature = "capture")]
    fn read() -> Self {
        Self {
            debug: std::env::var("DERETH_HIFI_WEATHER_DEBUG")
                .ok()
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(0.0),
            frame_clock: std::env::var("DERETH_HIFI_WEATHER_FRAME_CLOCK")
                .ok()
                .map(|v| {
                    v.trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|r| *r >= 2.0)
                        .unwrap_or(60.0)
                }),
        }
    }

    #[cfg(not(feature = "capture"))]
    fn read() -> Self {
        Self::default()
    }
}

/// The weather's clock: seconds since the pass began, wrapped to [`TIME_PERIOD`], and the step
/// since the frame before.
#[derive(Debug, Default)]
struct Clock {
    #[cfg(not(target_arch = "wasm32"))]
    start: Option<std::time::Instant>,
    frames: u64,
    last: Option<f64>,
}

impl Clock {
    /// The clock's reading, stepped one frame: by the wall clock, or by a frame at `by_frames`
    /// frames a second.
    fn tick(&mut self, by_frames: Option<f64>) -> (f32, f32) {
        self.frames += 1;
        #[cfg(not(target_arch = "wasm32"))]
        let wall = by_frames.is_none().then(|| {
            self.start
                .get_or_insert_with(std::time::Instant::now)
                .elapsed()
                .as_secs_f64()
        });
        #[cfg(target_arch = "wasm32")]
        let wall: Option<f64> = {
            // No wall clock in the browser: the frames are the clock.
            let _ = by_frames;
            None
        };
        #[allow(clippy::cast_precision_loss)] // a frame count, far below 2^52
        let now = wall.unwrap_or(self.frames as f64 / by_frames.unwrap_or(60.0));
        let reading = clock_reading(now, self.last);
        self.last = Some(now);
        reading
    }
}

/// The weather's clock at `now` seconds since it began, the time before at `last`: the time
/// within [`TIME_PERIOD`] the shader takes, and the step since `last`. Both are taken in double
/// precision before they are narrowed, so neither coarsens however long a session runs.
#[must_use]
pub fn clock_reading(now: f64, last: Option<f64>) -> (f32, f32) {
    #[allow(clippy::cast_possible_truncation)] // a time within the period, and a frame's step
    (
        now.rem_euclid(TIME_PERIOD) as f32,
        last.map_or(0.0, |l| (now - l) as f32),
    )
}

/// The wind the weather drifts on at `time_of_day` (0 to 1 through the day): a light breeze whose
/// strength and heading turn slowly with the day, metres a second east and north.
#[must_use]
pub fn wind(time_of_day: f32) -> [f32; 2] {
    use dereth_primitives::num::math::{cosf, sinf};
    let tau = std::f32::consts::TAU;
    let strength = sinf(time_of_day * tau * 3.0) * 0.25 + 0.55;
    let heading = time_of_day * tau;
    [cosf(heading) * strength, sinf(heading) * strength]
}

/// How far the rain and the snow have fallen and the wind has carried them, metres, each
/// wrapped to [`LATTICE_PERIOD`]: added to step by step, so a change of wind or of speed moves
/// the drops on from where they are rather than to somewhere else.
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
    /// Step `dt` seconds on, the wind blowing at `wind` metres a second east and north.
    pub fn step(&mut self, dt: f32, wind: [f32; 2]) {
        let dt = f64::from(dt.max(0.0));
        let on = |v: f64, by: f64| (v + by).rem_euclid(LATTICE_PERIOD);
        self.rain = on(self.rain, f64::from(RAIN_SPEED) * dt);
        self.snow = on(self.snow, f64::from(SNOW_SPEED) * dt);
        self.east = on(self.east, f64::from(wind[0]) * dt);
        self.north = on(self.north, f64::from(wind[1]) * dt);
    }
}

/// The particles of each of the three lattices drawn at `level`, from the High level's `high`.
#[must_use]
pub fn particles_at(level: Level, high: [u32; 3]) -> [u32; 3] {
    let (num, den) = match level {
        Level::Off => (0, 1),
        Level::Low => (1, 2),
        Level::Medium => (3, 4),
        Level::High => (1, 1),
        Level::Ultra => (3, 2),
    };
    high.map(|n| n * num / den)
}

fn linear(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| crate::passes::lighting::srgb_to_linear(f32::from(v) / 255.0))
}

/// Leaves the game's own falling-weather layers out of the replay while this pass draws the
/// weather: the sky objects, before the landscape and after it, that the sky marks as weather.
#[derive(Debug, Default, Clone, Copy)]
struct LegacyLayer {
    /// Whether the pass draws the weather this frame.
    replacing: bool,
    /// Inside one of the sky's passes.
    in_sky: bool,
    /// Inside one of its weather objects.
    weather: bool,
}

impl ReplayFilter for LegacyLayer {
    fn draw(&mut self, _cmd: u32, _note: Option<&DrawNote>) -> DrawAction<'_> {
        if self.replacing && self.in_sky && self.weather {
            DrawAction::Skip(SkipRule::ReplacedByPass)
        } else {
            DrawAction::Legacy
        }
    }

    fn mark(&mut self, mark: Mark, _pass: &mut wgpu::RenderPass<'_>) {
        match mark {
            Mark::SkyBegin(_) => {
                self.in_sky = true;
                self.weather = false;
            }
            Mark::SkyObject { properties, .. } if self.in_sky => {
                self.weather = properties & 4 != 0;
            }
            Mark::SkyEnd(_) | Mark::WorldBegin | Mark::WorldEnd => {
                self.in_sky = false;
                self.weather = false;
            }
            _ => {}
        }
    }
}

/// The weather pass.
#[derive(Debug)]
pub struct Weather {
    gpu: Option<Gpu>,
    constants: Vec<u8>,
    /// How the weather stands, eased between frames.
    fall: Fall,
    /// Whether it is drawn this frame: it falls, and the viewer is outdoors.
    drawn: bool,
    /// What a capture asks of it.
    overrides: Overrides,
    clock: Clock,
    /// The clock's reading this frame, seconds within its period.
    time: f32,
    /// How far the drops and flakes have fallen and drifted.
    drift: Drift,
    /// The particles drawn this frame: rain's lattices and then snow's.
    particles: u32,
    /// The landscape field under the weather, and what it was built from.
    field: Option<(FieldSignature, TerrainField)>,
    /// Whether the field changed since it was last uploaded.
    field_dirty: bool,
    /// What stands over the ground.
    cover: cover::CoverMap,
    /// Where the overhead map stands this frame.
    placement: cover::Placement,
    legacy: LegacyLayer,
}

impl Default for Weather {
    fn default() -> Self {
        Self {
            gpu: None,
            constants: Vec::new(),
            fall: Fall::default(),
            drawn: false,
            overrides: Overrides::read(),
            clock: Clock::default(),
            time: 0.0,
            drift: Drift::default(),
            particles: 0,
            field: None,
            field_dirty: false,
            cover: cover::CoverMap::default(),
            placement: cover::Placement::default(),
            legacy: LegacyLayer::default(),
        }
    }
}

#[derive(Debug)]
struct Gpu {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    /// The falling rain and snow, drawn over the picture the full-screen pass writes.
    fall: wgpu::RenderPipeline,
    constants: wgpu::Buffer,
    target: Option<(wgpu::Texture, wgpu::TextureView, (u32, u32))>,
    /// The landscape field's height, normal and class views, and its parameters.
    field: Option<([wgpu::TextureView; 3], wgpu::Buffer)>,
}

impl HifiPass for Weather {
    fn name(&self) -> &'static str {
        "weather"
    }

    // While the box is ticked the pass takes every frame's newly sent geometry, so it holds the
    // shelter of every block when the weather starts, whatever the day was when it was sent; and
    // its clock and the weather's easing run whether or not anything is drawn. It is drawn
    // while the viewer sees the outdoors: from a room, over what the room's openings show.
    fn observe(&mut self, cx: &mut PrepareCx<'_, '_>) {
        if !cx.settings.weather.is_on() {
            self.drawn = false;
            return;
        }
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        self.cover.take_feed(&device, &queue, cx.frame);
        let (time, step) = self.clock.tick(self.overrides.frame_clock);
        self.time = time;
        self.drift.step(step, wind(cx.frame.sky.time_of_day));
        self.fall.step(
            falls(cx.frame),
            snow_share(cx.frame),
            cx.frame.sky.asked_fall,
            step,
        );
        self.drawn = cx.frame.sky.sees_outside && self.fall.amount() > 0.0;
        self.legacy.replacing = self.drawn;
        if !self.drawn {
            self.particles = 0;
        }
    }

    // Only on a frame it draws: a dry day, or indoors, costs nothing past the observation.
    fn wanted(&self, s: &HifiSettings, _f: &HifiFrame, _caps: &Caps) -> bool {
        s.weather.is_on() && self.drawn
    }

    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        self.build_field(cx.frame);
        self.placement = cover::Placement::around(cx.frame.camera.eye);
        let srgb = cx.seam.surface_format().is_srgb();
        self.build_constants(
            cx.frame,
            cx.settings.weather,
            srgb,
            cx.settings.lighting.is_on(),
        );
        Ok(())
    }

    fn notes(&self, notes: &mut Vec<(&'static str, u64)>) {
        let (geometry, map) = self.cover.held_bytes();
        notes.push(("weather shelter geometry bytes", geometry));
        notes.push(("weather shelter map bytes", map));
        // How much falls and how much of it is snow, in thousandths.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..1 to 0..1000
        let thousandths = |v: f32| (v.clamp(0.0, 1.0) * 1000.0).round() as u64;
        notes.push(("weather falling", thousandths(self.fall.amount())));
        notes.push(("weather snow", thousandths(self.fall.snow())));
        notes.push(("weather particles", u64::from(self.particles)));
    }

    fn replay_filter(&mut self) -> Option<&mut dyn ReplayFilter> {
        Some(&mut self.legacy)
    }

    #[allow(clippy::too_many_lines)]
    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        if !self.drawn {
            return Ok(());
        }
        // On a re-shaded frame the weather goes into the linear picture before the tone map.
        let hdr = cx
            .reshade
            .filter(|_| !*cx.resolved)
            .map(|rt| rt.hdr.clone());
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        let format = if hdr.is_some() {
            crate::derive::pipeline::HDR_FORMAT
        } else {
            cx.seam.surface_format()
        };
        if self.gpu.as_ref().is_none_or(|g| g.format != format) {
            let field = self.gpu.take().and_then(|g| g.field);
            let mut gpu = Gpu::new(&device, format);
            gpu.field = field;
            self.gpu = Some(gpu);
        }
        if self.field_dirty || self.gpu.as_ref().is_some_and(|g| g.field.is_none()) {
            let Some((_, field)) = self.field.as_ref() else {
                return Ok(());
            };
            let [h, n, c] = field.upload(&device, &queue);
            let params = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("weather landscape field"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&params, 0, &field.params());
            if let Some(gpu) = self.gpu.as_mut() {
                gpu.field = Some(([h.1, n.1, c.1], params));
            }
            self.field_dirty = false;
        }
        // What stands over the ground, drawn from above.
        let sheltered = self.cover.holds_any();
        let cover_view = self.cover.draw(
            &device,
            &queue,
            cx.encoder,
            cx.frame,
            &self.placement,
            cx.timer,
        );
        cx.seam.count_pass();
        let gpu = self.gpu.as_mut().expect("made above");
        let mut constants = self.constants.clone();
        let at = |i: usize| i * 4..i * 4 + 4;
        if hdr.is_some() {
            // Linear light in and out.
            constants[at(LINEAR_AT)].copy_from_slice(&1.0f32.to_le_bytes());
            constants[at(RESHADED_AT)].copy_from_slice(&1.0f32.to_le_bytes());
        }
        for (i, v) in self.placement.constants(sheltered).iter().enumerate() {
            constants[at(72 + i)].copy_from_slice(&v.to_le_bytes());
        }
        queue.write_buffer(&gpu.constants, 0, &constants);
        let hdr_input = match &hdr {
            Some(_) => Some(crate::reshade::hdr_copy(cx.seam, cx.resources, cx.encoder)?),
            None => None,
        };
        let (width, height) = cx.seam.surface_size();
        if gpu.target.as_ref().is_none_or(|t| t.2 != (width, height)) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("weather picture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            gpu.target = Some((texture, view, (width, height)));
        }
        let target = gpu.target.as_ref().expect("made above").1.clone();
        let (field_views, field_params) = gpu.field.as_ref().expect("uploaded above");
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather"),
            layout: &gpu.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: gpu.constants.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        hdr_input.as_ref().unwrap_or(cx.colour),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(cx.world_depth),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&field_views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&field_views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&field_views[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: field_params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&cover_view),
                },
            ],
        });
        {
            let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("weather"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: hdr.as_ref().unwrap_or(&target),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: cx.timer.writes("weather"),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            cx.seam.count_pass();
            pass.set_bind_group(0, &bind, &[]);
            pass.set_pipeline(&gpu.pipeline);
            pass.draw(0..3, 0..1);
            // The falling rain and snow over it, in the world's viewport.
            let [x, y, w, h] = cx.frame.camera.viewport;
            let (x, y) = (x.min(width), y.min(height));
            let (w, h) = (w.min(width - x), h.min(height - y));
            if self.particles > 0 && w > 0 && h > 0 {
                #[allow(clippy::cast_precision_loss)] // pixel counts
                pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
                pass.set_pipeline(&gpu.fall);
                pass.draw(0..4, 0..self.particles);
            }
        }
        if hdr.is_none() {
            *cx.colour = target;
        }
        Ok(())
    }
}

impl Weather {
    /// How the weather stands after the frame last observed.
    #[must_use]
    pub fn fall(&self) -> Fall {
        self.fall
    }

    fn build_constants(&mut self, frame: &HifiFrame, level: Level, srgb: bool, relit: bool) {
        let mut f: Vec<f32> = Vec::with_capacity(CONSTANTS_SIZE / 4);
        let cam = &frame.camera;
        let clip_from_render = cam.projection * cam.view * HEIGHT_UP;
        f.extend_from_slice(&clip_from_render.to_cols_array());
        f.extend_from_slice(&render_from_clip(cam.view, cam.projection).to_cols_array());
        #[allow(clippy::cast_precision_loss)]
        f.extend_from_slice(&cam.viewport.map(|v| v as f32));
        f.extend_from_slice(&[cam.eye.x, cam.eye.y, cam.eye.z, self.time]);
        let sky = &frame.sky;
        let sd = sky.sun_direction;
        f.extend_from_slice(&[sd.x, sd.y, sd.z, sky.sun_brightness]);
        let sc = linear(sky.sun_color);
        let day = ((sd.z + 0.08) / 0.35).clamp(0.0, 1.0);
        f.extend_from_slice(&[sc[0], sc[1], sc[2], day]);
        let fog = sky.fog.map_or([150, 160, 170], |fg| fg.color);
        let fc = linear(fog);
        f.extend_from_slice(&[fc[0], fc[1], fc[2], if srgb { 1.0 } else { 0.0 }]);
        let ac = linear(sky.ambient_color);
        f.extend_from_slice(&[ac[0], ac[1], ac[2], sky.ambient_level]);
        let quality = match level {
            Level::Off | Level::Low => 0.0,
            Level::Medium => 1.0,
            Level::High => 2.0,
            Level::Ultra => 3.0,
        };
        #[allow(clippy::cast_precision_loss)]
        let pixel = cam.fov_y / (cam.viewport[3].max(1) as f32);
        f.extend_from_slice(&[quality, self.fall.snow(), self.fall.amount(), pixel]);
        let (fog_min, fog_max) = sky.fog.map_or((60.0, 400.0), |fg| (fg.min, fg.max));
        // The exposure the re-shaded world's linear light is shown at, when it is: a snow cover
        // there is lifted toward the white it shows as.
        let exposure = if relit {
            crate::passes::lighting::exposure(frame, 2.6, 0.85)
        } else {
            1.0
        };
        f.extend_from_slice(&[fog_min, fog_max, exposure, 0.0]);
        // Where render space sits in the world, wrapped to the noise's period: the world is
        // render space minus the shift, and the shift is a whole number of blocks.
        let wrap = |v: f32| (-v).rem_euclid(NOISE_PERIOD);
        let shift = cam.block_shift;
        #[allow(clippy::cast_possible_truncation)] // twenty minutes, exact in either width
        f.extend_from_slice(&[
            TIME_PERIOD as f32,
            wrap(shift.x),
            wrap(shift.y),
            NOISE_PERIOD,
        ]);
        // A view of the weather's masks, for tuning: at 1, the landscape, the road and the
        // puddles in red, green and blue; at 2, the shelter overhead in red.
        f.extend_from_slice(&[self.overrides.debug, 0.0, 0.0, 0.0]);
        // The overhead map, filled in as it is drawn.
        f.extend_from_slice(&[0.0; 8]);
        // The light a snow cover takes, as the frame lights the world: the sun's and the sky's,
        // in the picture's own terms. The picture is the ordinary frame's (the sun and ambient
        // as the vertex light takes them) or, when re-shaded, the linear light before exposure.
        let (sun, amb) = crate::passes::lighting::legacy_light(frame);
        let sun_up = crate::passes::lighting::sun_up(frame);
        let sun = if sun_up { sun } else { [0.0; 3] };
        f.extend_from_slice(&[sun[0], sun[1], sun[2], 0.0]);
        f.extend_from_slice(&[amb[0], amb[1], amb[2], 0.0]);
        // How far the drops and flakes have fallen and drifted, and how fast.
        let d = self.drift;
        #[allow(clippy::cast_possible_truncation)] // within the lattices' period
        f.extend_from_slice(&[d.rain as f32, d.snow as f32, d.east as f32, d.north as f32]);
        let w = wind(sky.time_of_day);
        f.extend_from_slice(&[RAIN_SPEED, SNOW_SPEED, w[0], w[1]]);
        // Where render space sits in the world, wrapped to the lattices' period.
        #[allow(clippy::cast_possible_truncation)] // the period, exact in either width
        let lattice = LATTICE_PERIOD as f32;
        let wrap = |v: f32| (-v).rem_euclid(lattice);
        f.extend_from_slice(&[wrap(shift.x), wrap(shift.y), lattice, 0.0]);
        // The particles drawn: rain's lattices, then snow's, each only while it falls.
        let ends = |counts: [u32; 3], from: u32| {
            let a = from + counts[0];
            let b = a + counts[1];
            [a, b, b + counts[2]]
        };
        let none = [0; 3];
        let rain = if self.fall.snow() < 1.0 {
            particles_at(level, RAIN_PARTICLES)
        } else {
            none
        };
        let snow = if self.fall.snow() > 0.0 {
            particles_at(level, SNOW_PARTICLES)
        } else {
            none
        };
        let rain_ends = ends(rain, 0);
        let snow_ends = ends(snow, rain_ends[2]);
        self.particles = snow_ends[2];
        #[allow(clippy::cast_precision_loss)] // counts of a few thousand
        {
            f.extend_from_slice(&rain_ends.map(|n| n as f32));
            f.push(0.0);
            f.extend_from_slice(&snow_ends.map(|n| n as f32));
            f.push(0.0);
        }
        debug_assert_eq!(f.len() * 4, CONSTANTS_SIZE);
        self.constants = f.iter().flat_map(|v| v.to_le_bytes()).collect();
    }

    /// The landscape field over the resident window, rebuilt only when a block changes.
    fn build_field(&mut self, frame: &HifiFrame) {
        let signature = TerrainField::signature(&frame.terrain);
        if self.field.as_ref().is_some_and(|(s, _)| *s == signature) {
            return;
        }
        if let Some(field) = TerrainField::build(&frame.terrain) {
            self.field = Some((signature, field));
            self.field_dirty = true;
        }
    }
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("weather"),
            source: wgpu::ShaderSource::Wgsl(shader_source().into()),
        });
        // The falling rain and snow read the constants and the overhead map as they are placed.
        let both = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let tex = |binding: u32, sample_type: wgpu::TextureSampleType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: if binding == 7 {
                both
            } else {
                wgpu::ShaderStages::FRAGMENT
            },
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("weather"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: both,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(CONSTANTS_SIZE as u64),
                    },
                    count: None,
                },
                tex(1, wgpu::TextureSampleType::Float { filterable: true }),
                tex(2, wgpu::TextureSampleType::Depth),
                tex(3, wgpu::TextureSampleType::Float { filterable: false }),
                tex(4, wgpu::TextureSampleType::Float { filterable: false }),
                tex(5, wgpu::TextureSampleType::Uint),
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(32),
                    },
                    count: None,
                },
                tex(7, wgpu::TextureSampleType::Depth),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("weather"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("weather"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_fullscreen"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_weather"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        // Each particle a quad of four corners; what it adds to the picture is mostly light, and
        // a flake hides what is behind it as far as it covers it.
        let fall = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("weather falling"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_fall"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_fall"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let constants = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weather"),
            size: CONSTANTS_SIZE as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            format,
            layout,
            pipeline,
            fall,
            constants,
            target: None,
            field: None,
        }
    }
}
