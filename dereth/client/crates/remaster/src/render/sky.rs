//! The sky and its atmosphere: a physical sky drawn over the authored sky dome by day, and a veil
//! of haze over the horizon, both from the lookup tables of a scattering atmosphere (after
//! Hillaire's model), calibrated to the day's weather and the fog the frame's intent gives.
//!
//! - **The tables.** The transmittance from any height toward any zenith angle and the light
//!   scattered more than once depend only on the air, which depends on the day's weather: they
//!   are drawn when it changes. The sky as seen from the eye depends on the sun and the eye's
//!   height: it is drawn when the sun moves a tenth of a degree or the eye five metres.
//! - **The sky.** The authored dome is drawn first, as retail draws it; then the physical sky over
//!   it, as opaque as the weather's share of the day (none at night, where the authored dark sky
//!   and its stars stay); then the stars, the sun, the moons and the clouds, as retail draws them;
//!   then the veil over the horizon, toward the sky's own colour there by day and the fog's by
//!   night. All of it in display values, as the rest of the world.

use dereth_gfx::{
    Address, Blend, ColorAttachment, ColorTarget, Cull, Device, Draw, Factor, Filter, Format,
    GraphicsDesc, Load, Pipeline, RasterPass, Recorder, Rect, Sampler, SamplerDesc, Shader,
    Texture, TextureDesc, TextureUsage,
};
use dereth_primitives::num::math;
use dereth_primitives::num::to_i32;
use glam::{Mat4, Vec2, Vec3, Vec4};

use super::scene::NONE;
use super::PerFrame;
use crate::scene::{Camera, FrameDelta, SkyObject};
use crate::shaders::{spirv, Entry};

/// The planet's radius in the sky model, kilometres.
pub const PLANET_RADIUS_KM: f32 = 6360.0;
/// The atmosphere's outer radius, kilometres.
pub const TOP_RADIUS_KM: f32 = 6460.0;
/// The sky model's radiance for unit sun illuminance is scaled by this before the picture's
/// roll-off: a noon sky's horizon then sits near the top of the range without clipping.
pub const EXPOSURE: f32 = 9.5;
/// The transmittance table's size, the multiple-scattering table's, and the sky-view table's with
/// its raymarch's steps.
const TRANSMITTANCE: (u32, u32) = (256, 64);
const MULTIPLE: (u32, u32) = (32, 32);
const SKY_VIEW: (u32, u32, u32) = (192, 108, 30);
/// How bright the horizon's veil is at night against the fog, and how wide, against by day.
const NIGHT_VEIL: f32 = 0.5;
const NIGHT_VEIL_BAND: f32 = 0.35;
/// The tables' format.
const TABLE: Format = Format::Rgba16Float;

/// The sky's settings.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkySettings {
    /// The physical sky over the authored dome, and the veil over the horizon.
    pub atmosphere: bool,
    /// The fog drawn from its volume. With no glow it is retail's range fog.
    pub fog: bool,
    /// How strongly the fog scatters the sun toward the eye where the sun reaches it: 0 none.
    pub glow: f32,
    /// How much of what lies at and past the fog's end comes through it, 0 none: the far ground
    /// drawn past where the game's fog ends keeps its shape and color.
    pub far_keep: f32,
    /// How deep the clear air is that the sun shows shafts in, besides the fog, across the reach
    /// of the sun's shadows: 0 none. Deeper as the sun sinks, and where the eye stands in shade.
    pub air: f32,
}

/// God Rays' low and high: the fog's glow toward the sun, and the clear air's depth.
const GOD_RAYS: [(f32, f32); 2] = [(0.04, 0.5), (0.08, 1.0)];
/// How many times deeper the clear air is with the sun on the horizon than overhead, besides
/// once, and where all round the eye stands in shade than in the open.
const AIR_LOW_SUN: f32 = 1.5;
const AIR_IN_SHADE: f32 = 1.0;

impl SkySettings {
    /// These settings with God Rays at `level`, 0 off, 1 low, 2 high: the glow and the clear air
    /// its strength gives, none while off.
    #[must_use]
    pub fn with_god_rays(self, level: u32) -> Self {
        let (glow, air) = match level {
            0 => (0.0, 0.0),
            1 => GOD_RAYS[0],
            _ => GOD_RAYS[1],
        };
        Self { glow, air, ..self }
    }
}

/// The fog volume's columns, rows and slices, its nearest distance, and the forward
/// scattering's asymmetry.
const FOG_GRID: (u32, u32, u32) = (160, 90, 64);
const FOG_NEAR: f32 = 1.0;
const FOG_G: f32 = 0.6;
/// How far past its end, as a share of it, the fog's volume reaches while the far ground keeps
/// its color through it.
const FAR_SPAN: f32 = 4.0;

/// With an art pack, the low haze: where it begins however near the fog starts, over how many
/// metres past that it eases in, how far it thins with height, the least and the most its density
/// differs from its density at the eye's height, its depth at night over the fog's range, how
/// thicker than the real air the air is drawn, and how steeply the haze thickens with distance.
const HAZE_NEAR: f32 = 80.0;
const HAZE_SOFT_START: f32 = 150.0;
const HAZE_HEIGHT_SCALE: f32 = 450.0;
const HAZE_DENSITY: (f32, f32) = (0.5, 1.3);
const NIGHT_HAZE_DEPTH: f32 = 0.75;
const AERIAL_THICKNESS: f32 = 4.0;
const HAZE_CURVE: f32 = 1.7;
/// How much of the haze's color is the sky's by full day, how much brighter it glows looking at
/// the sun, over how many metres that glow builds up, and how far its depth leans toward the blue.
const PHYSICAL_HAZE_SHARE: f32 = 0.9;
const HAZE_GLOW: f32 = 1.5;
const HAZE_GLOW_REACH: f32 = 300.0;
const HAZE_BLUE: f32 = 0.75;
/// Where along the way to where the land ends it starts to fade, and how much of it the haze takes
/// at its very end; and how much of the far ground's horizon the haze takes its depth over.
const EDGE_FADE_START: f32 = 0.85;
const EDGE_FADE_SHARE: f32 = 0.3;
const FAR_HAZE_REACH: f32 = 0.85;
/// The fog the haze is set by on an hour that has none: a clear day's.
const CLEAR_DAY_FOG: crate::scene::Fog = crate::scene::Fog {
    start: 150.0,
    end: 2400.0,
    color: [195, 200, 220],
};
/// The clouds' shadow map: its texels a side, how far from the eye it reaches each way, meters, and
/// the sun's height, degrees, over which the shadows come in, a lower sun's lying past its reach.
const CLOUD_MAP_SIZE: u32 = 1024;
const CLOUD_MAP_REACH: f32 = 12_000.0;
const CLOUD_SHADOW_SUN: (f32, f32) = (8.0, 20.0);

/// The kinds of weather the day groups are named for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weather {
    Clear,
    Sunny,
    Cloudy,
    Rainy,
}

impl Weather {
    /// The weather of a day group by its name; an unknown name is sunny.
    #[must_use]
    pub fn of(day_group: &str) -> Self {
        let name = day_group.trim().to_ascii_lowercase();
        if name.starts_with("rain") || name.starts_with("storm") || name.starts_with("snow") {
            Self::Rainy
        } else if name.starts_with("cloud") || name.starts_with("overcast") {
            Self::Cloudy
        } else if name.starts_with("clear") {
            Self::Clear
        } else {
            Self::Sunny
        }
    }

    /// How brightly the sky of this weather is exposed, against a clear day's.
    #[must_use]
    pub const fn exposure(self) -> f32 {
        match self {
            Self::Clear | Self::Sunny => 1.0,
            Self::Cloudy => 0.8,
            Self::Rainy => 0.65,
        }
    }

    /// How much of the authored dome the physical sky covers by full day: the rest shows
    /// through, so the authored sky's own cast stays, the more so the wetter the day.
    #[must_use]
    pub const fn sky_share(self) -> f32 {
        match self {
            Self::Clear => 0.85,
            Self::Sunny => 0.8,
            Self::Cloudy => 0.65,
            Self::Rainy => 0.5,
        }
    }

    /// The low haze's depth by day over the fog's range: a sunny day's distance stays clear, a
    /// wet day's is thick.
    #[must_use]
    pub const fn haze_depth(self) -> f32 {
        match self {
            Self::Clear => 0.35,
            Self::Sunny => 0.4,
            Self::Cloudy => 0.6,
            Self::Rainy => 0.75,
        }
    }

    /// How grey the sky is, 0 blue to 1 grey.
    #[must_use]
    pub const fn overcast(self) -> f32 {
        match self {
            Self::Clear | Self::Sunny => 0.0,
            Self::Cloudy => 0.45,
            Self::Rainy => 0.7,
        }
    }

    /// The air's scattering, as factors on a clear day's: its haze (Mie) and its air (Rayleigh).
    #[must_use]
    pub const fn air(self) -> (f32, f32) {
        match self {
            Self::Clear => (0.6, 1.0),
            Self::Sunny => (1.0, 1.0),
            Self::Cloudy => (5.0, 0.85),
            Self::Rainy => (9.0, 0.75),
        }
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The sun's elevation, degrees, from its direction toward it.
fn elevation(sun: Vec3) -> f32 {
    math::asinf(sun.normalize_or(Vec3::Z).z.clamp(-1.0, 1.0)).to_degrees()
}

/// Whether a sky object is the authored dome: every day group draws its dome first, always
/// present, with no property bits.
#[must_use]
pub fn is_dome(index: usize, o: &SkyObject) -> bool {
    index == 0 && o.properties == 0
}

/// How much of the day it is, 0 night to 1 day: the sun's elevation shapes dawn and dusk, and
/// the dome's own brightness says when the authored day is night (retail keeps a low sun through
/// the night).
#[must_use]
pub fn day_weight(frame: &FrameDelta) -> f32 {
    let sun = smoothstep(-4.0, 6.0, elevation(frame.light.sun));
    let night = frame
        .sky
        .first()
        .filter(|o| is_dome(0, o))
        .and_then(|o| o.material)
        .map_or(1.0, |m| {
            if m.luminosity < 0.0 {
                1.0
            } else {
                smoothstep(0.12, 0.40, m.luminosity)
            }
        });
    sun * night
}

/// How far into the low sun of dawn or dusk the day is: 1 with the sun 4 degrees up or lower, 0
/// with it 18 degrees up or higher.
#[must_use]
pub fn twilight(frame: &FrameDelta) -> f32 {
    1.0 - smoothstep(4.0, 18.0, elevation(frame.light.sun))
}

/// An sRGB-encoded colour's bytes in linear light.
#[must_use]
pub fn srgb_bytes_to_linear(c: [u8; 3]) -> Vec3 {
    let f = |b: u8| {
        let c = f32::from(b) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            math::powf((c + 0.055) / 1.055, 2.4)
        }
    };
    Vec3::new(f(c[0]), f(c[1]), f(c[2]))
}

/// The sun's illuminance as the sky is exposed for: the weather's exposure, tinted toward the
/// sun's colour, the more so in the low sun.
#[must_use]
pub fn sun_illuminance(frame: &FrameDelta, weather: Weather) -> Vec3 {
    let tint = srgb_bytes_to_linear(frame.light.sun_color);
    let peak = tint.max_element().max(1e-3);
    let t = 0.5 + (0.7 - 0.5) * twilight(frame);
    let tint = Vec3::ONE.lerp(tint / peak, t);
    tint * EXPOSURE * weather.exposure()
}

/// The direction in the frame's space through each point of the screen: the inverse of the
/// projection times the view's rotation alone.
#[must_use]
pub fn ray_from_ndc(camera: &Camera) -> Mat4 {
    let mut rotation = camera.view;
    rotation.w_axis = Vec4::W;
    (camera.projection * rotation).inverse()
}

/// What the tables depend on: the air.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TablesKey(Weather);

/// What the sky-view table depends on: the air, the sun to a tenth of a degree (elevation and
/// heading) and the eye's height to five metres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewKey {
    tables: TablesKey,
    sun: [i32; 3],
    height: i32,
}

/// The constant rows every sky shader reads, for `frame`.
#[must_use]
#[allow(clippy::cast_precision_loss)] // a target's extent and table sizes
pub fn constants(frame: &FrameDelta, extent: (u32, u32), linear: bool) -> Vec<u8> {
    let cam = &frame.camera;
    let weather = Weather::of(&frame.day_group);
    let (mie, rayleigh) = weather.air();
    let day = day_weight(frame);
    let fog_max = frame.light.fog.map_or(2400.0, |f| f.end);
    let band = (0.045 * (2400.0 / fog_max.max(50.0)).sqrt()).clamp(0.03, 0.08)
        * (NIGHT_VEIL_BAND + (1.0 - NIGHT_VEIL_BAND) * day);
    let fog_color = frame.light.fog.map_or(Vec3::new(0.55, 0.58, 0.7), |f| {
        srgb_bytes_to_linear(f.color)
    });
    let rows: [Vec4; 12] = [
        cam.eye.extend((cam.eye.z.max(0.0) + 2.0) * 0.001),
        Vec4::new(extent.0 as f32, extent.1 as f32, 0.0, 0.0),
        frame
            .light
            .sun
            .normalize_or(Vec3::Z)
            .extend(weather.overcast()),
        sun_illuminance(frame, weather).extend(twilight(frame)),
        (Vec3::new(5.802e-3, 13.558e-3, 33.1e-3) * rayleigh).extend(-1.0 / 8.0),
        Vec4::new(3.996e-3 * mie, 4.44e-3 * mie, -1.0 / 1.2, 0.8),
        Vec4::new(0.650e-3, 1.881e-3, 0.085e-3, 0.0),
        Vec3::new(0.25, 0.27, 0.2).extend(PLANET_RADIUS_KM),
        fog_color.extend(0.0),
        Vec4::new(
            day * weather.sky_share(),
            if linear { 1.0 } else { 0.0 },
            TOP_RADIUS_KM,
            0.0,
        ),
        Vec4::new(SKY_VIEW.0 as f32, SKY_VIEW.1 as f32, 0.0, SKY_VIEW.2 as f32),
        Vec4::new(band, day, NIGHT_VEIL + (1.0 - NIGHT_VEIL) * day, 0.0),
    ];
    let mut out = Vec::with_capacity(16 * 16);
    for f in ray_from_ndc(cam).to_cols_array() {
        out.extend_from_slice(&f.to_le_bytes());
    }
    for r in rows {
        for f in r.to_array() {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    out
}

/// Where the clouds' shadow map lies and what it is read from, for a frame whose clouds shade the
/// land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloudMap {
    /// Its center in the frame's space, on the world's grid of its texels, so the shadows hold still
    /// on the land as the eye moves.
    pub center: glam::Vec2,
    /// The height of the clouds' base in the frame's space.
    pub base: f32,
    /// How much of the sun the shadows take.
    pub strength: f32,
    /// Retail's picture's coordinates over the frame's meters, as high as the clouds lie.
    pub laying: [f32; 4],
    /// How far the picture has drifted, and where the frame's origin lies in the world, within a
    /// period of the picture.
    pub drift: [f32; 4],
    /// How much of the sky they cover and how thick they are.
    pub cover: f32,
    pub depth: f32,
    /// How much of them retail's hours leave showing.
    pub showing: f32,
    /// The sky object they are read from.
    pub gfx: u32,
}

/// The clouds' shadow map for `frame`: none with no deck, no clouds, or the sun too low.
#[must_use]
pub fn cloud_map(frame: &FrameDelta) -> Option<CloudMap> {
    let drawn = frame.drawn.as_ref()?;
    let knobs = drawn.knobs.clouds?;
    let (o, deck) = frame
        .sky
        .iter()
        .enumerate()
        .filter(|(i, o)| !is_dome(*i, o) && o.properties & 4 == 0)
        .find_map(|(_, o)| Some((o, o.deck?)))?;
    let rise = ((elevation(frame.light.sun) - CLOUD_SHADOW_SUN.0)
        / (CLOUD_SHADOW_SUN.1 - CLOUD_SHADOW_SUN.0))
        .clamp(0.0, 1.0);
    let strength = knobs.shadows * rise * rise * (3.0 - 2.0 * rise);
    if strength <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // a texture's size
    let texel = 2.0 * CLOUD_MAP_REACH / CLOUD_MAP_SIZE as f32;
    let eye = frame.camera.eye;
    let world = glam::Vec2::new(eye.x, eye.y) + drawn.global;
    let k = knobs.height / deck.height.max(50.0) * knobs.scale.max(1e-3);
    let [a, b, c, e] = deck.laying;
    let laying = [a, b / k, c, e / k];
    let wrap = |v: f32, period: f32| v.rem_euclid(period);
    Some(CloudMap {
        center: (world / texel).round() * texel - drawn.global,
        base: eye.z + knobs.height,
        strength,
        laying,
        drift: [
            wrap(o.uv_offset[0], 1.0),
            wrap(o.uv_offset[1], 1.0),
            wrap(drawn.global.x, 1.0 / laying[1].abs().max(1e-9)),
            wrap(drawn.global.y, 1.0 / laying[3].abs().max(1e-9)),
        ],
        cover: knobs.cover,
        depth: knobs.depth,
        showing: o
            .material
            .map_or(1.0, |m| (1.0 - m.translucency).clamp(0.0, 1.0)),
        gfx: o.gfx,
    })
}

/// How far round the eye the low fog looks for the lowest ground, metres.
const LOW_FOG_REACH: f32 = 400.0;

/// The lowest ground of the blocks within [`LOW_FOG_REACH`] of `frame`'s eye, the eye's own height
/// when there is none: what the low fog lies over.
#[must_use]
pub fn lowest_ground(scene: &super::scene::GpuScene, frame: &FrameDelta) -> f32 {
    let eye = frame.camera.eye;
    scene
        .ground
        .iter()
        .filter(|(id, _)| {
            #[allow(clippy::cast_precision_loss)] // a few hundred blocks
            let (x, y) = (
                (i32::from(*id >> 8) - frame.origin[0]) as f32 * 192.0 + 96.0,
                (i32::from(*id & 0xFF) - frame.origin[1]) as f32 * 192.0 + 96.0,
            );
            (x - eye.x).abs() < LOW_FOG_REACH + 96.0 && (y - eye.y).abs() < LOW_FOG_REACH + 96.0
        })
        .map(|(_, g)| g.z[0])
        .reduce(f32::min)
        .unwrap_or(eye.z)
}

/// The low fog's row of the sky's constants: x its extinction per metre where it lies, y how many
/// metres up it thins by e, z the height it lies over, w 1 while it is drawn. A dry day's lies
/// thin and shallow; a wet one's thick and deep; dawn and dusk gather it in the low ground, and the
/// night keeps some.
#[must_use]
pub fn low_fog_row(frame: &FrameDelta, base: f32) -> Vec4 {
    let (extinction, height) = match Weather::of(&frame.day_group) {
        Weather::Clear => (0.0005, 14.0),
        Weather::Sunny => (0.0007, 18.0),
        Weather::Cloudy => (0.0009, 22.0),
        Weather::Rainy => (0.0011, 26.0),
    };
    let day = day_weight(frame);
    let gathered = 1.0 + 1.6 * twilight(frame) * day + 0.4 * (1.0 - day);
    Vec4::new(extinction * gathered, height, base, 1.0)
}

/// The rows the air and the clouds' shadows add to the sky's constants, for `frame`: the clouds'
/// shadow map at `map`, by its index in the table, when they shade the land.
#[must_use]
#[allow(clippy::cast_precision_loss)] // a texture's size
fn air_rows(frame: &FrameDelta, map: Option<(&CloudMap, u32)>) -> [Vec4; 9] {
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let weather = Weather::of(&frame.day_group);
    let day = day_weight(frame);
    let fog = frame.light.fog.unwrap_or(CLEAR_DAY_FOG);
    let start = fog.start.max(HAZE_NEAR);
    let range = (fog.end - start).max(50.0);
    let mut extinction = lerp(NIGHT_HAZE_DEPTH, weather.haze_depth(), day)
        / (range - 0.5 * HAZE_SOFT_START).max(25.0);
    let mut reach = range.max(0.5 * HAZE_SOFT_START + 25.0) - 0.5 * HAZE_SOFT_START;
    let eye = frame.camera.eye;
    let mut land = Vec4::ZERO;
    // Ground drawn to a horizon past the fog's end takes the haze's depth over its own length, which
    // the haze would otherwise wash out long before it, and ends there.
    if let Some(drawn) = frame.drawn.as_ref() {
        if let Some((_, _, _, horizon)) = drawn.knobs.far {
            let far = (horizon * FAR_HAZE_REACH).max(reach);
            extinction = extinction * reach * drawn.knobs.haze.max(0.0) / far;
            reach = far;
            land = Vec4::new(
                eye.x - horizon,
                eye.y - horizon,
                eye.x + horizon,
                eye.y + horizon,
            );
        }
    }
    let (cloud_map, cloud_shade) = map.map_or((Vec4::ZERO, Vec4::ZERO), |(m, index)| {
        (
            Vec4::new(m.center.x, m.center.y, CLOUD_MAP_REACH, m.base),
            Vec4::new(
                m.strength,
                f32::from_bits(index),
                CLOUD_MAP_SIZE as f32,
                0.0,
            ),
        )
    });
    [
        srgb_bytes_to_linear(fog.color).extend(extinction),
        Vec4::new(start, HAZE_HEIGHT_SCALE, 0.0, HAZE_GLOW * day),
        land,
        Vec4::new(day, AERIAL_THICKNESS, 1.0, EDGE_FADE_START),
        Vec4::new(HAZE_DENSITY.0, HAZE_DENSITY.1, 0.0, HAZE_GLOW_REACH),
        Vec4::new(
            PHYSICAL_HAZE_SHARE * day,
            EDGE_FADE_SHARE,
            0.0,
            HAZE_SOFT_START,
        ),
        Vec4::new(
            HAZE_BLUE * day * (1.0 - 0.8 * weather.overcast()),
            HAZE_CURVE,
            reach,
            0.0,
        ),
        cloud_map,
        cloud_shade,
    ]
}

/// The sky's pipelines.
#[derive(Debug, Clone, Copy)]
struct Pipelines {
    transmittance: Pipeline,
    multiple: Pipeline,
    sky_view: Pipeline,
    /// The sky and the veil, for display targets and for the post passes' linear ones.
    sky: [Pipeline; 2],
    veil: [Pipeline; 2],
    fog: Pipeline,
    /// The air over the land, into the post passes' linear target; and the clouds' shadow map.
    air: Pipeline,
    cloud_shadow: Pipeline,
}

/// The sky's resources and state.
#[derive(Debug)]
pub struct SkyPass {
    pub settings: SkySettings,
    pipelines: Pipelines,
    sampler: (Sampler, u32),
    tables: Option<[(Texture, u32); 3]>,
    constants: PerFrame,
    /// The last frame's constants, for views of the sky other than the frame's own.
    last_rows: Vec<u8>,
    fog_constants: PerFrame,
    fog_volume: Option<(dereth_gfx::Buffer, u32)>,
    tables_key: Option<TablesKey>,
    view_key: Option<ViewKey>,
    /// The copy of the world the air reads, with its index and size.
    air_copy: Option<(Texture, u32, (u32, u32))>,
    /// The clouds' shadow map, and the constants it is worked out from.
    cloud_shadow: Option<(Texture, u32)>,
    cloud_constants: PerFrame,
    /// Times the air's tables and the sky-view table were drawn.
    pub tables_drawn: u64,
    pub views_drawn: u64,
}

/// What the sky draws this frame.
#[derive(Debug, Clone)]
pub struct SkyPlan {
    pipelines: Pipelines,
    post: bool,
    constants: u32,
    sampler: u32,
    tables: [(Texture, u32); 3],
    draw_tables: bool,
    draw_view: bool,
    /// The air over the land, with an art pack.
    air: Option<AirPlan>,
}

/// What the air and the clouds' shadows draw this frame.
#[derive(Debug, Clone, Copy)]
struct AirPlan {
    copy: (Texture, u32),
    /// The clouds' shadow map, its constants, retail's picture and the sampler it is read with,
    /// by their indices in the table.
    clouds: Option<((Texture, u32), [u32; 3])>,
}

/// The low fog: what it lies over and what lights it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LowFog {
    /// The lowest ground round the eye: the fog is thicker over ground near it.
    pub base: f32,
    /// The frame's origin, world meters; the landscape heights' index in the table; and the sun's
    /// shadow cascades' constants (`NONE`: the sun reaches all of it).
    pub global: Vec2,
    pub heights: u32,
    pub cascades: u32,
}

/// What a frame draws the air with.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AirAsk {
    /// The low fog, when it is drawn: what it lies over and what lights it.
    pub low_fog: Option<LowFog>,
    /// The scene's colors' size.
    pub size: (u32, u32),
    /// Retail's cloud picture, with the clouds' shadow map's place for it, and the sampler that
    /// repeats it, by their indices in the table: none when the clouds do not shade the land.
    pub clouds: Option<(CloudMap, u32, u32)>,
}

fn shader(e: Entry) -> Shader<'static> {
    Shader {
        spirv: spirv(e),
        dxil: e.dxil_bytes(),
        entry: e.name(),
    }
}

fn words(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|x| x.to_le_bytes()).collect()
}

impl SkyPass {
    /// The sky's atmosphere, off, its pipelines made now; the sky is drawn into targets of
    /// `color`, or of `linear` when the post passes run.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(d: &mut Device, color: Format, linear: Format) -> Result<Self, dereth_gfx::Error> {
        let table = |d: &mut Device, label, e: Entry| {
            d.create_graphics_pipeline(&GraphicsDesc {
                label,
                vertex: shader(Entry::FullscreenVsMain),
                fragment: Some(shader(e)),
                color: &[ColorTarget {
                    format: TABLE,
                    blend: None,
                    write_alpha: true,
                }],
                depth: None,
                cull: Cull::None,
            })
        };
        let over = |d: &mut Device, label, e: Entry, color: Format| {
            d.create_graphics_pipeline(&GraphicsDesc {
                label,
                vertex: shader(Entry::FullscreenVsMain),
                fragment: Some(shader(e)),
                color: &[ColorTarget {
                    format: color,
                    blend: Some(Blend {
                        src: Factor::SrcAlpha,
                        dst: Factor::OneMinusSrcAlpha,
                    }),
                    write_alpha: false,
                }],
                depth: None,
                cull: Cull::None,
            })
        };
        let pipelines = Pipelines {
            transmittance: table(
                d,
                "remaster sky transmittance",
                Entry::SkyTablesFsTransmittance,
            )?,
            multiple: table(
                d,
                "remaster sky multiple scattering",
                Entry::SkyTablesFsMultiple,
            )?,
            sky_view: table(d, "remaster sky view", Entry::SkyTablesFsSkyView)?,
            sky: [
                over(d, "remaster physical sky", Entry::SkyFsSky, color)?,
                over(d, "remaster physical sky, linear", Entry::SkyFsSky, linear)?,
            ],
            veil: [
                over(d, "remaster horizon veil", Entry::SkyFsVeil, color)?,
                over(d, "remaster horizon veil, linear", Entry::SkyFsVeil, linear)?,
            ],
            fog: d.create_compute_pipeline(&dereth_gfx::ComputeDesc {
                label: "remaster fog volume",
                shader: shader(Entry::FogVolumeCsMain),
            })?,
            air: d.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster air",
                vertex: shader(Entry::FullscreenVsMain),
                fragment: Some(shader(Entry::AirFsAir)),
                color: &[ColorTarget {
                    format: linear,
                    blend: None,
                    write_alpha: true,
                }],
                depth: None,
                cull: Cull::None,
            })?,
            cloud_shadow: d.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster cloud shadow",
                vertex: shader(Entry::FullscreenVsMain),
                fragment: Some(shader(Entry::CloudShadowFsMain)),
                color: &[ColorTarget {
                    format: Format::R8Unorm,
                    blend: None,
                    write_alpha: true,
                }],
                depth: None,
                cull: Cull::None,
            })?,
        };
        let sampler = d.create_sampler(&SamplerDesc {
            filter: Filter::Linear,
            mip: Filter::Nearest,
            address: Address::Clamp,
            address_v: Address::Clamp,
            ..SamplerDesc::default()
        })?;
        let sampler_index = d.bind_sampler(sampler)?;
        Ok(Self {
            settings: SkySettings::default(),
            pipelines,
            sampler: (sampler, sampler_index),
            tables: None,
            constants: PerFrame::new(d, "remaster sky constants", 512, Default::default())?,
            last_rows: Vec::new(),
            fog_constants: PerFrame::new(d, "remaster fog constants", 80, Default::default())?,
            air_copy: None,
            cloud_shadow: None,
            cloud_constants: PerFrame::new(d, "remaster cloud constants", 48, Default::default())?,
            fog_volume: None,
            tables_key: None,
            view_key: None,
            tables_drawn: 0,
            views_drawn: 0,
        })
    }

    fn tables(&mut self, d: &mut Device) -> Result<[(Texture, u32); 3], dereth_gfx::Error> {
        if let Some(t) = self.tables {
            return Ok(t);
        }
        let mut make = |label, (w, h): (u32, u32)| -> Result<(Texture, u32), dereth_gfx::Error> {
            let t = d.create_texture(&TextureDesc {
                label,
                width: w,
                height: h,
                mips: 1,
                format: TABLE,
                usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
            })?;
            Ok((t, d.bind_texture(t)?))
        };
        let t = [
            make("remaster sky transmittance", TRANSMITTANCE)?,
            make("remaster sky multiple scattering", MULTIPLE)?,
            make("remaster sky view", (SKY_VIEW.0, SKY_VIEW.1))?,
        ];
        self.tables = Some(t);
        Ok(t)
    }

    /// The sky's constants the frame was drawn with.
    #[must_use]
    pub(crate) fn rows(&self) -> &[u8] {
        &self.last_rows
    }

    /// Work out the frame's fog volume, when the fog is drawn from one and the frame has fog:
    /// its constants written. `cascades` is the shadows' cascades' constants and maps, when they
    /// are drawn, for the shafts.
    ///
    /// # Errors
    /// What the device refuses.
    #[allow(clippy::cast_precision_loss)] // the grid's sizes
    pub(crate) fn prepare_fog(
        &mut self,
        d: &mut Device,
        frame: &FrameDelta,
        cascades: Option<(u32, Vec<Texture>)>,
    ) -> Result<Option<FogPlan>, dereth_gfx::Error> {
        let Some(f) = frame.light.fog.filter(|_| self.settings.fog) else {
            return Ok(None);
        };
        let (gw, gh, gd) = FOG_GRID;
        let (volume, index) = match self.fog_volume {
            Some(v) => v,
            None => {
                let b = d.create_buffer(&dereth_gfx::BufferDesc {
                    label: "remaster fog volume",
                    size: u64::from(gw * gh * gd) * 16,
                    usage: dereth_gfx::BufferUsage::STORAGE,
                    location: dereth_gfx::Location::Gpu,
                })?;
                let v = (b, d.bind_buffer(b)?);
                self.fog_volume = Some(v);
                v
            }
        };
        let sun = frame.light.sun_color.map(|b| f32::from(b) / 255.0);
        let bright = frame.light.sun.length().min(1.0);
        let shadow = cascades.as_ref().map_or(NONE, |(i, _)| *i);
        let mut rows = Vec::with_capacity(80);
        for v in [
            [
                FOG_NEAR,
                f.end.max(FOG_NEAR * 2.0)
                    * if self.settings.far_keep > 0.0 {
                        FAR_SPAN
                    } else {
                        1.0
                    },
                gd as f32,
                self.settings.glow.max(0.0),
            ],
            [sun[0] * bright, sun[1] * bright, sun[2] * bright, FOG_G],
            [gw as f32, gh as f32, f32::from_bits(shadow), 1.0],
            [self.settings.far_keep.clamp(0.0, 1.0), f.end, 0.0, 0.0],
            [self.settings.air.max(0.0), AIR_LOW_SUN, AIR_IN_SHADE, 0.0],
        ] {
            for x in v {
                rows.extend_from_slice(&x.to_le_bytes());
            }
        }
        self.fog_constants.write(d, &rows)?;
        Ok(Some(FogPlan {
            pipeline: self.pipelines.fog,
            constants: self.fog_constants.index,
            volume,
            volume_index: index,
            maps: cascades.map(|(_, m)| m).unwrap_or_default(),
        }))
    }

    /// Work out the frame's sky: its constants written, and which tables are drawn again.
    /// `post` is whether the post passes run (the scene is then drawn into their target),
    /// `linear` whether the frame's colors are linear, and `air` what the air is drawn with, when
    /// it is.
    ///
    /// # Errors
    /// What the device refuses.
    #[allow(clippy::cast_possible_truncation)] // tenths of a degree and five-metre steps
    pub(crate) fn prepare(
        &mut self,
        d: &mut Device,
        frame: &FrameDelta,
        extent: (u32, u32),
        post: bool,
        linear: bool,
        air: Option<AirAsk>,
    ) -> Result<Option<SkyPlan>, dereth_gfx::Error> {
        if !self.settings.atmosphere || frame.sky.is_empty() || !frame.landscape {
            return Ok(None);
        }
        let tables = self.tables(d)?;
        let mut rows = constants(frame, extent, linear);
        let air = match air {
            Some(ask) => Some(self.prepare_air(d, frame, ask, &mut rows)?),
            None => {
                // The rows the shaders read to know the air is not drawn.
                rows.resize(rows.len() + 9 * 16, 0);
                None
            }
        };
        self.constants.write(d, &rows)?;
        self.last_rows = rows;
        let tables_key = TablesKey(Weather::of(&frame.day_group));
        let sun = frame.light.sun.normalize_or(Vec3::Z);
        let view_key = ViewKey {
            tables: tables_key,
            sun: (sun * 1800.0 / std::f32::consts::PI)
                .to_array()
                .map(|v| to_i32(v.round())),
            height: to_i32((frame.camera.eye.z / 5.0).round()),
        };
        let draw_tables = self.tables_key != Some(tables_key);
        let draw_view = draw_tables || self.view_key != Some(view_key);
        self.tables_key = Some(tables_key);
        self.view_key = Some(view_key);
        self.tables_drawn += u64::from(draw_tables);
        self.views_drawn += u64::from(draw_view);
        Ok(Some(SkyPlan {
            pipelines: self.pipelines,
            post,
            constants: self.constants.index,
            sampler: self.sampler.1,
            tables,
            draw_tables,
            draw_view,
            air,
        }))
    }

    /// The air's copy of the world and the clouds' shadow map, made as they are wanted, and the
    /// rows they add to the sky's constants.
    #[allow(clippy::cast_precision_loss)] // a texture's size
    fn prepare_air(
        &mut self,
        d: &mut Device,
        frame: &FrameDelta,
        ask: AirAsk,
        rows: &mut Vec<u8>,
    ) -> Result<AirPlan, dereth_gfx::Error> {
        let copy = match self.air_copy {
            Some(c) if c.2 == ask.size => c,
            old => {
                if let Some(c) = old {
                    d.destroy_texture(c.0);
                }
                let format = super::post::scene_format(d);
                let texture = d.create_texture(&TextureDesc {
                    label: "remaster air copy",
                    width: ask.size.0,
                    height: ask.size.1,
                    mips: 1,
                    format,
                    usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                })?;
                let c = (texture, d.bind_texture(texture)?, ask.size);
                self.air_copy = Some(c);
                c
            }
        };
        let clouds = match ask.clouds {
            Some((map, picture, sampler)) => {
                let target = match self.cloud_shadow {
                    Some(t) => t,
                    None => {
                        let t = d.create_texture(&TextureDesc {
                            label: "remaster cloud shadow",
                            width: CLOUD_MAP_SIZE,
                            height: CLOUD_MAP_SIZE,
                            mips: 1,
                            format: Format::R8Unorm,
                            usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
                        })?;
                        let t = (t, d.bind_texture(t)?);
                        self.cloud_shadow = Some(t);
                        t
                    }
                };
                let mut bytes = Vec::with_capacity(48);
                for v in [
                    map.laying,
                    map.drift,
                    [map.cover, map.depth, map.showing, CLOUD_MAP_SIZE as f32],
                ] {
                    for x in v {
                        bytes.extend_from_slice(&x.to_le_bytes());
                    }
                }
                self.cloud_constants.write(d, &bytes)?;
                Some((map, target, [self.cloud_constants.index, picture, sampler]))
            }
            None => None,
        };
        let mut air = air_rows(frame, clouds.as_ref().map(|(m, t, _)| (m, t.1))).to_vec();
        air.push(
            ask.low_fog
                .map_or(Vec4::ZERO, |f| low_fog_row(frame, f.base)),
        );
        air.push(ask.low_fog.map_or(Vec4::ZERO, |f| {
            Vec4::new(
                f.global.x,
                f.global.y,
                f32::from_bits(f.heights),
                f32::from_bits(f.cascades),
            )
        }));
        for v in air {
            for f in v.to_array() {
                rows.extend_from_slice(&f.to_le_bytes());
            }
        }
        Ok(AirPlan {
            copy: (copy.0, copy.1),
            clouds: clouds.map(|(_, t, w)| (t, w)),
        })
    }
}

impl SkyPlan {
    /// Draw the tables that changed.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn record_tables(&self, r: &mut Recorder<'_>) -> Result<(), dereth_gfx::Error> {
        let [transmittance, multiple, view] = self.tables;
        let table = |r: &mut Recorder<'_>,
                     name: &str,
                     target: Texture,
                     size: (u32, u32),
                     p: Pipeline,
                     reads: &[Texture]|
         -> Result<(), dereth_gfx::Error> {
            let touches: Vec<dereth_gfx::Touch> = reads
                .iter()
                .map(|t| dereth_gfx::Touch::Texture(*t, dereth_gfx::Use::Sampled))
                .collect();
            r.raster(
                &RasterPass {
                    name,
                    color: &[ColorAttachment {
                        texture: target,
                        load: Load::Discard,
                    }],
                    depth: None,
                    touches: &touches,
                    area: Some(Rect {
                        x: 0,
                        y: 0,
                        width: size.0,
                        height: size.1,
                    }),
                },
                |d| {
                    d.set_pipeline(p)?;
                    #[allow(clippy::cast_precision_loss)] // a table's size
                    let push = [
                        words(&[self.constants, transmittance.1, multiple.1, self.sampler]),
                        (size.0 as f32).to_le_bytes().to_vec(),
                        (size.1 as f32).to_le_bytes().to_vec(),
                    ]
                    .concat();
                    d.push(0, &push)?;
                    d.draw(3, 1, 0, 0);
                    Ok(())
                },
            )
        };
        if self.draw_tables {
            r.timestamp("sky tables");
            table(
                r,
                "sky transmittance",
                transmittance.0,
                TRANSMITTANCE,
                self.pipelines.transmittance,
                &[],
            )?;
            table(
                r,
                "sky multiple scattering",
                multiple.0,
                MULTIPLE,
                self.pipelines.multiple,
                &[transmittance.0],
            )?;
        }
        if self.draw_view {
            r.timestamp("sky view");
            table(
                r,
                "sky view",
                view.0,
                (SKY_VIEW.0, SKY_VIEW.1),
                self.pipelines.sky_view,
                &[transmittance.0, multiple.0],
            )?;
        }
        Ok(())
    }

    /// Whether the air is drawn over the land this frame.
    #[must_use]
    pub fn air(&self) -> bool {
        self.air.is_some()
    }

    /// The sky's constants, when they lay the clouds' shadows, and the map, for the resolve.
    #[must_use]
    pub fn cloud_shadows(&self) -> Option<(u32, Texture)> {
        self.air
            .and_then(|a| a.clouds)
            .map(|(map, _)| (self.constants, map.0))
    }

    /// Work out the clouds' shadow map, when they shade the land.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn record_cloud_shadow(
        &self,
        r: &mut Recorder<'_>,
    ) -> Result<(), dereth_gfx::Error> {
        let Some((map, [clouds, picture, sampler])) = self.air.and_then(|a| a.clouds) else {
            return Ok(());
        };
        r.timestamp("cloud shadow");
        r.raster(
            &RasterPass {
                name: "cloud shadow",
                color: &[ColorAttachment {
                    texture: map.0,
                    load: Load::Discard,
                }],
                depth: None,
                touches: &[],
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: CLOUD_MAP_SIZE,
                    height: CLOUD_MAP_SIZE,
                }),
            },
            |d| {
                d.set_pipeline(self.pipelines.cloud_shadow)?;
                d.push(0, &words(&[self.constants, clouds, picture, sampler]))?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }

    /// Draw the air over what the scene's colors hold, by the depth `depth`. `frame` is the frame's
    /// constants' index.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn record_air(
        &self,
        r: &mut Recorder<'_>,
        t: &super::Targets,
        frame: u32,
        depth: (Texture, u32),
        reads: &[dereth_gfx::Touch],
    ) -> Result<(), dereth_gfx::Error> {
        let Some(air) = self.air else {
            return Ok(());
        };
        let mut touches = vec![
            dereth_gfx::Touch::Texture(air.copy.0, dereth_gfx::Use::Sampled),
            dereth_gfx::Touch::Texture(depth.0, dereth_gfx::Use::Sampled),
            dereth_gfx::Touch::Texture(t.material, dereth_gfx::Use::Sampled),
            dereth_gfx::Touch::Texture(self.tables[2].0, dereth_gfx::Use::Sampled),
        ];
        touches.extend_from_slice(reads);
        r.timestamp("air");
        r.copy_texture(t.color, air.copy.0)?;
        r.raster(
            &RasterPass {
                name: "air",
                color: &[ColorAttachment {
                    texture: t.color,
                    load: Load::Discard,
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
            |d| {
                d.set_pipeline(self.pipelines.air)?;
                d.push(
                    0,
                    &words(&[
                        frame,
                        self.constants,
                        self.tables[2].1,
                        self.sampler,
                        air.copy.1,
                        depth.1,
                        t.index[3],
                    ]),
                )?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }

    /// What the sky pass touches of the sky's: the sky-view table.
    #[must_use]
    pub fn view_table(&self) -> Texture {
        self.tables[2].0
    }

    /// What another pass reads to find the sky's own light any way it looks: the sky's
    /// constants, its sky-view table and the sampler it is read with, by their indices in the
    /// table, and the table itself.
    #[must_use]
    pub(crate) fn view_inputs(&self) -> ([u32; 3], Texture) {
        (
            [self.constants, self.tables[2].1, self.sampler],
            self.tables[2].0,
        )
    }

    /// Draw the physical sky over what is drawn, in the sky pass.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn draw_sky(&self, d: &mut Draw<'_>) -> Result<(), dereth_gfx::Error> {
        self.fullscreen(d, self.pipelines.sky[usize::from(self.post)])
    }

    /// Draw the veil over the horizon, in the sky pass, last.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn draw_veil(&self, d: &mut Draw<'_>) -> Result<(), dereth_gfx::Error> {
        self.fullscreen(d, self.pipelines.veil[usize::from(self.post)])
    }

    /// Draw the physical sky, then (`veil`) the veil, seen as the sky's constants `constants`
    /// say: for a view of the sky other than the frame's own.
    pub(crate) fn draw_seen(
        &self,
        d: &mut Draw<'_>,
        constants: u32,
        veil: bool,
    ) -> Result<(), dereth_gfx::Error> {
        let p = if veil {
            self.pipelines.veil[usize::from(self.post)]
        } else {
            self.pipelines.sky[usize::from(self.post)]
        };
        d.set_pipeline(p)?;
        d.push(0, &words(&[constants, self.tables[2].1, self.sampler]))?;
        d.draw(3, 1, 0, 0);
        Ok(())
    }

    fn fullscreen(&self, d: &mut Draw<'_>, p: Pipeline) -> Result<(), dereth_gfx::Error> {
        d.set_pipeline(p)?;
        d.push(0, &words(&[self.constants, self.tables[2].1, self.sampler]))?;
        d.draw(3, 1, 0, 0);
        Ok(())
    }
}

/// The frame's fog volume.
#[derive(Debug, Clone)]
pub struct FogPlan {
    pipeline: Pipeline,
    /// The fog's constants and its volume, by their index in the table.
    pub constants: u32,
    pub volume: dereth_gfx::Buffer,
    pub volume_index: u32,
    maps: Vec<Texture>,
}

impl FogPlan {
    /// Fill the volume, down every column of slices. `frame` is the frame's constants.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn record(&self, r: &mut Recorder<'_>, frame: u32) -> Result<(), dereth_gfx::Error> {
        let mut touches = vec![dereth_gfx::Touch::Buffer(
            self.volume,
            dereth_gfx::Use::StorageWrite,
        )];
        touches.extend(
            self.maps
                .iter()
                .map(|m| dereth_gfx::Touch::Texture(*m, dereth_gfx::Use::Sampled)),
        );
        r.timestamp("fog volume");
        r.compute(&touches, |c| {
            c.set_pipeline(self.pipeline)?;
            c.push(0, &words(&[frame, self.constants, self.volume_index]))?;
            c.dispatch(FOG_GRID.0.div_ceil(8), FOG_GRID.1.div_ceil(8), 1);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// A day group's name says its weather; the wetter the day, the more of the authored sky stays.
    #[test]
    fn the_wetter_the_day_the_more_of_the_authored_sky_stays() {
        assert_eq!(Weather::of("Sunny"), Weather::Sunny);
        assert_eq!(Weather::of(" rainy 2"), Weather::Rainy);
        assert_eq!(Weather::of("Cloudy"), Weather::Cloudy);
        assert_eq!(Weather::of("Clear"), Weather::Clear);
        assert_eq!(Weather::of("anything"), Weather::Sunny);
        assert!(Weather::Rainy.sky_share() < Weather::Cloudy.sky_share());
        assert!(Weather::Cloudy.sky_share() < Weather::Sunny.sky_share());
    }

    fn with_deck(sun: Vec3) -> FrameDelta {
        let mut f = FrameDelta::default();
        f.light.sun = sun;
        f.camera.eye = Vec3::new(10.0, 20.0, 30.0);
        let knobs = crate::scene::DrawnKnobs {
            clouds: Some(crate::scene::DrawnClouds {
                cover: 1.0,
                height: 1500.0,
                depth: 900.0,
                scale: 1.0,
                shadows: 0.5,
            }),
            ..crate::scene::DrawnKnobs::default()
        };
        f.drawn = Some(crate::scene::DrawnGround {
            global: glam::Vec2::new(5000.0, 7000.0),
            knobs,
            ..crate::scene::DrawnGround::default()
        });
        for (gfx, deck) in [
            (1, None),
            (
                2,
                Some(crate::scene::SkyDeck {
                    laying: [0.0, 0.001, 0.0, 0.001],
                    height: 750.0,
                }),
            ),
        ] {
            f.sky.push(SkyObject {
                gfx,
                properties: 0,
                frame: Mat4::IDENTITY,
                before: true,
                after: false,
                material: None,
                uv_offset: [1.25, -0.25],
                fogged: false,
                deck,
            });
        }
        f
    }

    /// The clouds' shadows come from the deck, laid as large as retail shows it at the clouds'
    /// height, the map's center on the world's grid of its texels.
    #[test]
    fn the_clouds_shadow_map_lies_on_the_worlds_grid_under_the_deck() {
        let f = with_deck(Vec3::new(0.3, 0.0, 0.9));
        let m = cloud_map(&f).expect("a high sun casts them");
        assert_eq!(m.gfx, 2);
        assert!((m.laying[1] - 0.0005).abs() < 1e-7, "{:?}", m.laying);
        assert!((m.strength - 0.5).abs() < 1e-6);
        assert!((m.base - 1530.0).abs() < 1e-3);
        assert!((m.drift[0] - 0.25).abs() < 1e-6 && (m.drift[1] - 0.75).abs() < 1e-6);
        let texel = 2.0 * CLOUD_MAP_REACH / 1024.0;
        let on_grid = (m.center.x + 5000.0) / texel;
        assert!((on_grid - on_grid.round()).abs() < 1e-2, "{on_grid}");
        assert!((m.center.x - 10.0).abs() <= texel, "{:?}", m.center);
    }

    /// A low sun's shadows would lie past the map's reach; with no deck nothing casts them.
    #[test]
    fn a_low_sun_or_no_deck_casts_no_clouds_shadow() {
        assert!(cloud_map(&with_deck(Vec3::new(1.0, 0.0, 0.1))).is_none());
        let mut f = with_deck(Vec3::new(0.3, 0.0, 0.9));
        f.sky.truncate(1);
        assert!(cloud_map(&f).is_none());
    }

    /// Over ground drawn to a far horizon the haze takes its depth over that length, not the fog's,
    /// so land short of the horizon stays clear.
    #[test]
    fn the_haze_reaches_its_depth_at_the_far_grounds_horizon() {
        let mut f = FrameDelta::default();
        f.light.sun = Vec3::new(0.3, 0.0, 0.9);
        f.day_group = "Sunny".to_owned();
        let near = air_rows(&f, None);
        assert_eq!(near[2], Vec4::ZERO, "no land's end with no far ground");
        f.drawn = Some(crate::scene::DrawnGround {
            knobs: crate::scene::DrawnKnobs {
                far: Some((1.0, 1.5, 96.0, 30_000.0)),
                ..crate::scene::DrawnKnobs::default()
            },
            ..crate::scene::DrawnGround::default()
        });
        let far = air_rows(&f, None);
        assert!((far[6].z - 25_500.0).abs() < 1e-2, "{}", far[6].z);
        assert!(
            (far[0].w * far[6].z - near[0].w * near[6].z).abs() < 1e-4,
            "the same depth, over its own length"
        );
    }

    /// The low fog lies thicker and deeper on a wet day than a dry one, gathers in the low sun of
    /// dawn and dusk, and lies over the height it is given.
    #[test]
    fn the_low_fog_is_thicker_on_wet_days_and_at_dawn_and_dusk() {
        let mut f = FrameDelta::default();
        f.light.sun = Vec3::new(0.3, 0.0, 0.9);
        f.day_group = "Sunny".to_owned();
        let dry = low_fog_row(&f, 12.0);
        f.day_group = "Rainy".to_owned();
        let wet = low_fog_row(&f, 12.0);
        assert!(wet.x > dry.x && wet.y > dry.y, "{dry} {wet}");
        assert_eq!(dry.z, 12.0);
        assert_eq!(dry.w, 1.0);
        f.day_group = "Sunny".to_owned();
        f.light.sun = Vec3::new(1.0, 0.0, 0.1);
        let dawn = low_fog_row(&f, 12.0);
        assert!(dawn.x > dry.x, "{dry} {dawn}");
    }

    /// A sun under the horizon is night, and a dome whose authored brightness is low is night
    /// whatever the sun; a high sun over a bright dome is full day.
    #[test]
    fn night_is_a_low_sun_or_a_dark_dome() {
        let mut f = FrameDelta::default();
        f.light.sun = Vec3::new(0.3, 0.0, 0.9);
        assert!((day_weight(&f) - 1.0).abs() < 1e-6);
        f.light.sun = Vec3::new(1.0, 0.0, -0.2);
        assert!(day_weight(&f) < 1e-6);
        f.light.sun = Vec3::new(0.3, 0.0, 0.9);
        f.sky.push(SkyObject {
            gfx: 1,
            properties: 0,
            frame: Mat4::IDENTITY,
            before: true,
            after: false,
            material: Some(crate::scene::Material {
                translucency: 0.0,
                diffusion: 1.0,
                luminosity: 0.05,
            }),
            uv_offset: [0.0; 2],
            fogged: false,
            deck: None,
        });
        assert!(day_weight(&f) < 1e-6, "a dark dome");
    }
}
