//! The atmosphere's numbers, worked out on the CPU from the frame's snapshot: the sky model's
//! air and haze for the day's weather, the low haze calibrated to the authored fog, how much of
//! the authored sky the physical one replaces, and the constant block every sky shader reads.

use dereth_primitives::num::math;
use glam::{Mat4, Vec3, Vec4};

use crate::snapshot::{HifiCamera, HifiFog, HifiFrame, HifiSky};
use crate::Level;

/// The planet's radius in the sky model, kilometres.
pub const PLANET_RADIUS_KM: f32 = 6360.0;
/// The atmosphere's outer radius, kilometres.
pub const TOP_RADIUS_KM: f32 = 6460.0;
/// The side of a landscape block, metres.
const BLOCK_SIDE: f32 = 192.0;
/// The fog range assumed when the time of day has none: the clear-day range.
const CLEAR_DAY_FOG: HifiFog = HifiFog {
    min: 150.0,
    max: 2400.0,
    color: [195, 200, 220],
};
/// The low haze's optical depth over the authored fog's range at night: the land at the night
/// fog's end keeps `exp(-0.75)` of its light, about half. By day the depth is the weather's
/// ([`Weather::haze_depth`]), and the two blend through dawn and dusk.
pub const NIGHT_HAZE_DEPTH: f32 = 0.75;
/// The nearest the haze begins, metres, whatever the authored fog's start: the near field is
/// drawn exactly as the unfogged world is.
pub const HAZE_NEAR: f32 = 80.0;
/// Over how many metres past its start the air eases in, rather than taking hold at a line.
pub const HAZE_SOFT_START: f32 = 150.0;
/// How far the haze thins with height, metres: a point this much higher lies in haze about a
/// third as thick.
pub const HAZE_HEIGHT_SCALE: f32 = 450.0;
/// The least and the most the haze's density along a line of sight differs from its density at
/// the eye's height, so valleys are hazier and peaks clearer without either going to extremes.
pub const HAZE_DENSITY: (f32, f32) = (0.5, 1.3);
/// How much thicker than the real air the air between the eye and the world is drawn, so the
/// authored world's short distances show the blue of distance.
pub const AERIAL_THICKNESS: f32 = 4.0;
/// Where along the way to the edge of the resident landscape the land starts to fade into the
/// haze.
pub const EDGE_FADE_START: f32 = 0.85;
/// How much of the land at the very edge of the resident landscape is taken by the haze beyond
/// what its distance gives it.
pub const EDGE_FADE_SHARE: f32 = 0.3;
/// How much of the haze's colour is the sky's own colour at the horizon, by full day; the rest is
/// the authored fog colour.
pub const PHYSICAL_HAZE_SHARE: f32 = 0.9;
/// How much brighter than the authored fog colour the haze glows looking straight at the sun,
/// by full day, over and above it.
pub const HAZE_GLOW: f32 = 1.5;
/// How far the haze's depth leans toward the blue by full clear day, 0 grey to 1 the air's own
/// Rayleigh ratio: far land loses its red last and gains the sky's blue first, so ridge behind
/// ridge goes bluer and darker against the paler sky.
pub const HAZE_BLUE: f32 = 0.75;
/// How steeply the haze thickens with distance: its depth grows as distance to this power, so the
/// near and middle ground stay crisp and the far land takes the haze.
pub const HAZE_CURVE: f32 = 1.7;
/// The distance over which the haze's glow toward the sun builds up, metres: the glow of the air
/// in front of a near thing is a small share of the glow of the air in front of the distance.
pub const HAZE_GLOW_REACH: f32 = 300.0;
/// How bright the horizon's veil over the night sky is, against the haze over the land at night:
/// darker, so the hills' line stays against the sky.
pub const NIGHT_VEIL: f32 = 0.5;
/// How wide the horizon's veil is at night, against its width by day.
pub const NIGHT_VEIL_BAND: f32 = 0.35;
/// The constant block's size in bytes: two matrices and sixteen vectors.
pub const CONSTANTS_SIZE: usize = 2 * 64 + 16 * 16;

/// The sky model's air and haze for one kind of weather.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Air {
    /// Rayleigh scattering per kilometre at the ground.
    pub rayleigh: Vec3,
    /// The Rayleigh scale height, kilometres.
    pub rayleigh_height: f32,
    /// Mie scattering per kilometre at the ground.
    pub mie_scattering: f32,
    /// Mie extinction per kilometre at the ground.
    pub mie_extinction: f32,
    /// The Mie scale height, kilometres.
    pub mie_height: f32,
    /// The Mie phase asymmetry.
    pub mie_g: f32,
    /// Ozone absorption per kilometre at the layer's peak.
    pub ozone: Vec3,
    /// The ground's albedo.
    pub ground: Vec3,
}

impl Air {
    /// Clear air: the Earth's at sea level.
    pub const CLEAR: Self = Self {
        rayleigh: Vec3::new(5.802e-3, 13.558e-3, 33.1e-3),
        rayleigh_height: 8.0,
        mie_scattering: 3.996e-3,
        mie_extinction: 4.44e-3,
        mie_height: 1.2,
        mie_g: 0.8,
        ozone: Vec3::new(0.650e-3, 1.881e-3, 0.085e-3),
        ground: Vec3::new(0.25, 0.27, 0.2),
    };

    /// The air of a day of weather named `day_group`: a crisper sky on clear days, a paler one
    /// as the haze thickens on cloudy and rainy days.
    #[must_use]
    pub fn for_weather(day_group: &str) -> Self {
        let (mie, rayleigh) = match Weather::of(day_group) {
            Weather::Clear => (0.6, 1.0),
            Weather::Sunny => (1.0, 1.0),
            Weather::Cloudy => (5.0, 0.85),
            Weather::Rainy => (9.0, 0.75),
        };
        Self {
            rayleigh: Self::CLEAR.rayleigh * rayleigh,
            mie_scattering: Self::CLEAR.mie_scattering * mie,
            mie_extinction: Self::CLEAR.mie_extinction * mie,
            ..Self::CLEAR
        }
    }
}

/// The kinds of weather the day groups are named for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weather {
    /// A clear day.
    Clear,
    /// A sunny day with clouds.
    Sunny,
    /// An overcast day.
    Cloudy,
    /// A rainy day.
    Rainy,
}

impl Weather {
    /// The low haze's optical depth over the authored fog's range by day: the land at the fog's
    /// end keeps `exp(-depth)` of its light. A sunny day's distance stays clear; a wet day's
    /// is thick.
    #[must_use]
    pub const fn haze_depth(self) -> f32 {
        match self {
            Self::Clear => 0.35,
            Self::Sunny => 0.4,
            Self::Cloudy => 0.6,
            Self::Rainy => 0.75,
        }
    }

    /// How brightly the sky of this weather is exposed, against a clear day's: an overcast sky
    /// is heavier and darker than a clear one.
    #[must_use]
    pub const fn exposure(self) -> f32 {
        match self {
            Self::Clear | Self::Sunny => 1.0,
            Self::Cloudy => 0.8,
            Self::Rainy => 0.65,
        }
    }

    /// How much of the authored sky dome the physical sky covers by full day in this weather.
    /// The rest of the dome shows through it, so the authored sky's own cast (lavender on a fair
    /// day, brown in the rain) stays in the sky and in the clouds over it; the wetter the day,
    /// the more of it.
    #[must_use]
    pub const fn sky_share(self) -> f32 {
        match self {
            Self::Clear => 0.85,
            Self::Sunny => 0.8,
            Self::Cloudy => 0.65,
            Self::Rainy => 0.5,
        }
    }

    /// How grey the sky of this weather is, 0 for a blue sky to 1 for a grey one.
    #[must_use]
    pub const fn overcast(self) -> f32 {
        match self {
            Self::Clear | Self::Sunny => 0.0,
            Self::Cloudy => 0.45,
            Self::Rainy => 0.7,
        }
    }

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
}

/// The low haze, calibrated to the authored fog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Haze {
    /// Its colour, linear.
    pub colour: Vec3,
    /// Its extinction per metre at the eye's height.
    pub extinction: f32,
    /// Where it begins, metres from the eye.
    pub start: f32,
    /// Its height scale, metres.
    pub height_scale: f32,
    /// The width of its band over the horizon, as the sine of an elevation.
    pub band: f32,
    /// The authored fog's range from where the haze begins, metres.
    pub range: f32,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Haze {
    /// The haze for `fog` (or a clear day's fog when the time of day has none) in `weather`,
    /// `day_weight` of the way from night to full day ([`day_weight`]).
    #[must_use]
    pub fn for_fog(fog: Option<&HifiFog>, weather: Weather, day_weight: f32) -> Self {
        let fog = fog.copied().unwrap_or(CLEAR_DAY_FOG);
        let start = fog.min.max(HAZE_NEAR);
        let range = (fog.max - start).max(50.0);
        let day = day_weight.clamp(0.0, 1.0);
        let band = (0.045 * (2400.0 / fog.max.max(50.0)).sqrt()).clamp(0.03, 0.08);
        Self {
            colour: srgb_bytes_to_linear(fog.color),
            // The soft start takes half its length off the way to the fog's end.
            extinction: Self::depth(weather, day) / (range - 0.5 * HAZE_SOFT_START).max(25.0),
            range: range.max(0.5 * HAZE_SOFT_START + 25.0),
            start,
            height_scale: HAZE_HEIGHT_SCALE,
            band: band * lerp(NIGHT_VEIL_BAND, 1.0, day),
        }
    }

    /// The haze's optical depth over the authored fog's range in `weather`, `day_weight` of the
    /// way from night to full day.
    #[must_use]
    pub fn depth(weather: Weather, day_weight: f32) -> f32 {
        lerp(
            NIGHT_HAZE_DEPTH,
            weather.haze_depth(),
            day_weight.clamp(0.0, 1.0),
        )
    }

    /// The fraction of a point's light that comes through the haze from `dist` metres away, the
    /// point `rise` metres higher than the eye. The same arithmetic as the shader's.
    #[must_use]
    pub fn transmittance(&self, dist: f32, rise: f32) -> f32 {
        let x = rise / self.height_scale;
        let density = if x.abs() > 1e-3 {
            (1.0 - math::expf(-x)) / x
        } else {
            1.0
        };
        let density = density.clamp(HAZE_DENSITY.0, HAZE_DENSITY.1);
        let x = (dist - self.start).max(0.0);
        let d = if x < HAZE_SOFT_START {
            x * x / (2.0 * HAZE_SOFT_START)
        } else {
            x - 0.5 * HAZE_SOFT_START
        };
        let reach = self.reach();
        math::expf(-self.extinction * reach * math::powf(d / reach, HAZE_CURVE) * density)
    }

    /// The length of air over which the haze reaches its calibrated depth, metres.
    #[must_use]
    pub fn reach(&self) -> f32 {
        self.range - 0.5 * HAZE_SOFT_START
    }
}

/// Whether the sky object drawn `index`th in the sky pass before the landscape, with
/// `properties`, is the authored sky dome. Every day group lists its dome first, always present,
/// with no property bits; the star field, the sun, the moons and the clouds follow it.
#[must_use]
pub const fn is_dome(index: u32, properties: u32) -> bool {
    index == 0 && properties == 0
}

/// How much of the authored sky the physical one replaces, 0 to 1: all of it by day, none of it
/// at night, where the authored dark sky and its stars are kept. The dome's own brightness ramp
/// says when the authored day is night, since the authored light keeps a low "sun" through the
/// night; the sun's elevation shapes dawn and dusk.
#[must_use]
pub fn day_weight(sky: &HifiSky) -> f32 {
    let elevation = math::asinf(sky.sun_direction.z.clamp(-1.0, 1.0)).to_degrees();
    let sun = smoothstep(-4.0, 6.0, elevation);
    let night = sky
        .objects
        .iter()
        .find(|o| is_dome(o.index, o.properties))
        .map_or(1.0, |dome| {
            if dome.luminosity < 0.0 {
                1.0
            } else {
                smoothstep(12.0, 40.0, dome.luminosity)
            }
        });
    sun * night
}

/// How far into the low sun of dawn or dusk the day is: 1 with the sun 4 degrees up or lower, 0
/// with it 18 degrees up or higher.
#[must_use]
pub fn twilight(sky: &HifiSky) -> f32 {
    let elevation = math::asinf(sky.sun_direction.z.clamp(-1.0, 1.0)).to_degrees();
    1.0 - smoothstep(4.0, 18.0, elevation)
}

/// The sun's illuminance as the sky is exposed for: the weather's exposure, tinted toward the
/// authored sun colour, the more so in the low sun.
#[must_use]
pub fn sun_illuminance(sky: &HifiSky) -> Vec3 {
    let tint = srgb_bytes_to_linear(sky.sun_color);
    let peak = tint.max_element().max(1e-3);
    let tint = Vec3::ONE.lerp(tint / peak, lerp(0.5, 0.7, twilight(sky)));
    tint * EXPOSURE * Weather::of(&sky.day_group).exposure()
}

/// How opaque the physical sky is drawn over the authored dome: the weather's share
/// ([`Weather::sky_share`]) of the day weight.
#[must_use]
pub fn sky_share(sky: &HifiSky) -> f32 {
    day_weight(sky) * Weather::of(&sky.day_group).sky_share()
}

/// The sky model's radiance for unit sun illuminance is scaled by this before the picture's
/// roll-off: a noon sky's horizon then sits near the top of the range without clipping.
pub const EXPOSURE: f32 = 9.5;

/// The extent of the resident landscape in render space, min x, min y, max x, max y; all zero
/// when no block is resident.
#[must_use]
pub fn window(frame: &HifiFrame) -> Vec4 {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for b in &frame.terrain {
        lo = lo.min(b.origin);
        hi = hi.max(b.origin + Vec3::new(BLOCK_SIDE, BLOCK_SIDE, 0.0));
    }
    if frame.terrain.is_empty() {
        Vec4::ZERO
    } else {
        Vec4::new(lo.x, lo.y, hi.x, hi.y)
    }
}

/// The step count of the sky-view raymarch and the sky-view table's size at `level`.
#[must_use]
pub const fn sky_view_quality(level: Level) -> (u32, u32, u32) {
    match level {
        Level::Off | Level::Low => (96, 54, 16),
        Level::Medium => (128, 72, 24),
        Level::High => (192, 108, 30),
        Level::Ultra => (256, 144, 40),
    }
}

/// What the sky tables depend on, coarsely: they are drawn again only when this changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TablesKey {
    /// The weather.
    pub weather: Weather,
}

/// What the sky-view table depends on beyond the tables, coarsely: the sun's direction to a
/// tenth of a degree and the eye's height to five metres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkyViewKey {
    /// The tables it reads.
    pub tables: TablesKey,
    /// The sun's elevation, tenths of a degree.
    pub sun_elevation: i32,
    /// The eye's height, five-metre steps.
    pub height: i32,
    /// The table's size and step count.
    pub quality: (u32, u32, u32),
}

impl SkyViewKey {
    /// The key for `frame` at `level`.
    #[must_use]
    pub fn of(frame: &HifiFrame, level: Level) -> Self {
        let elevation = math::asinf(frame.sky.sun_direction.z.clamp(-1.0, 1.0)).to_degrees();
        Self {
            tables: TablesKey {
                weather: Weather::of(&frame.sky.day_group),
            },
            sun_elevation: dereth_primitives::num::to_i32((elevation * 10.0).round()),
            height: dereth_primitives::num::to_i32((frame.camera.eye.z / 5.0).round()),
            quality: sky_view_quality(level),
        }
    }
}

/// The constant block every sky shader reads, for `frame` at `level`.
#[must_use]
pub fn constants(frame: &HifiFrame, level: Level) -> Vec<u8> {
    let cam = &frame.camera;
    let sky = &frame.sky;
    let air = Air::for_weather(&sky.day_group);
    let weather = Weather::of(&sky.day_group);
    let weight = day_weight(sky);
    let haze = Haze::for_fog(sky.fog.as_ref(), weather, weight);
    let illuminance = sun_illuminance(sky);
    let (sky_w, sky_h, steps) = sky_view_quality(level);
    let height_km = (cam.eye.z.max(0.0) + 2.0) * 0.001;
    #[allow(clippy::cast_precision_loss)] // pixel extents and table sizes
    let vectors: [Vec4; 16] = [
        cam.eye.extend(height_km),
        Vec4::new(
            cam.viewport[0] as f32,
            cam.viewport[1] as f32,
            cam.viewport[2] as f32,
            cam.viewport[3] as f32,
        ),
        sky.sun_direction
            .normalize_or(Vec3::Z)
            .extend(weather.overcast()),
        illuminance.extend(twilight(sky)),
        air.rayleigh.extend(-1.0 / air.rayleigh_height),
        Vec4::new(
            air.mie_scattering,
            air.mie_extinction,
            -1.0 / air.mie_height,
            air.mie_g,
        ),
        air.ozone.extend(0.0),
        air.ground.extend(PLANET_RADIUS_KM),
        haze.colour.extend(haze.extinction),
        Vec4::new(haze.start, haze.height_scale, haze.band, HAZE_GLOW * weight),
        window(frame),
        Vec4::new(weight, AERIAL_THICKNESS, TOP_RADIUS_KM, 0.0),
        Vec4::new(sky_w as f32, sky_h as f32, EDGE_FADE_START, steps as f32),
        Vec4::new(
            HAZE_DENSITY.0,
            HAZE_DENSITY.1,
            lerp(NIGHT_VEIL, 1.0, weight),
            HAZE_GLOW_REACH,
        ),
        Vec4::new(
            PHYSICAL_HAZE_SHARE * weight,
            EDGE_FADE_SHARE,
            weight * weather.sky_share(),
            HAZE_SOFT_START,
        ),
        Vec4::new(
            HAZE_BLUE * weight * (1.0 - 0.8 * weather.overcast()),
            HAZE_CURVE,
            haze.reach(),
            0.0,
        ),
    ];
    let mut out = Vec::with_capacity(CONSTANTS_SIZE);
    for m in [ray_from_ndc(cam), position_from_ndc(cam)] {
        for f in m.to_cols_array() {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    for v in vectors {
        for f in v.to_array() {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    out
}

/// The frame's drawing space holds the world's height on its second axis and the world's
/// north on its third; the snapshot's points and directions hold height on the third. This
/// swaps the two.
const HEIGHT_UP: Mat4 = Mat4::from_cols(Vec4::X, Vec4::Z, Vec4::Y, Vec4::W);

/// Normalised device coordinates to a render-space direction, height up: the inverse of the
/// projection times the view's rotation alone, so the eye's position does not enter.
#[must_use]
pub fn ray_from_ndc(cam: &HifiCamera) -> Mat4 {
    let mut rotation = cam.view;
    rotation.w_axis = Vec4::W;
    HEIGHT_UP * (cam.projection * rotation).inverse()
}

/// Normalised device coordinates and depth to a render-space position, height up.
#[must_use]
pub fn position_from_ndc(cam: &HifiCamera) -> Mat4 {
    HEIGHT_UP * cam.view_projection().inverse()
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

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
