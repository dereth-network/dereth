//! The physical sky over the day's own: the shared passes' scattering atmosphere
//! ([`dereth_gfx_passes::sky`]) drawn over the day's dome and under its other objects (the haze,
//! the sun, the clouds), as opaque as the day is bright and dry; then the veil of haze over the
//! horizon, after retail's own veil. All of it in display values, as retail draws the sky, so a
//! frame in linear light lays it in with the rest of the sky's picture.
//!
//! What lights it is the moment's outdoor light: the sun where its light comes from, its color,
//! the fog's end and color, and the rain amount the streaks are drawn by, which thickens the air
//! and grays the sky. Night is when the dome's own opacity says so (retail fades the dome out at
//! night, and lights the land by the moon from above), and then the physical sky is gone and the
//! dome's dark sky and its stars stay.

use auberean_draw::scene::Frame;
use auberean_world::sky::DayLight;
use dereth_gfx_passes::sky::{Air, SkyLight};
use dereth_primitives::num::math;
use glam::{Mat4, Vec3};

/// The physical sky's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkySettings {
    /// The physical sky over the day's dome, and the veil over the horizon.
    pub atmosphere: bool,
}

/// The air of a dry moment and of one raining as hard as it can, and how much of the dome the
/// physical sky covers by full day in each: a wet sky lets more of the dome's own cast through.
const DRY: Air = Air {
    mie: 1.0,
    rayleigh: 1.0,
    overcast: 0.0,
    exposure: 1.0,
};
const WET: Air = Air {
    mie: 9.0,
    rayleigh: 0.75,
    overcast: 0.7,
    exposure: 0.65,
};
const COVER: (f32, f32) = (0.8, 0.5);
/// The rain is taken in this many steps from dry to wet, so the air's tables are drawn again only
/// as it moves a step.
const RAIN_STEPS: f32 = 16.0;

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The sun's elevation, degrees, from the direction toward it.
fn elevation(sun: Vec3) -> f32 {
    math::asinf(sun.normalize_or(Vec3::Z).z.clamp(-1.0, 1.0)).to_degrees()
}

/// A display value taken to linear light.
fn linear(c: f32) -> f32 {
    super::to_linear(c.clamp(0.0, 1.0))
}

/// How wet a moment of rain amount `rain` is, 0 dry to 1 raining as hard as it can, in steps.
#[must_use]
pub fn wetness(rain: f32) -> f32 {
    (rain.clamp(0.0, 1.0) * RAIN_STEPS).round() / RAIN_STEPS
}

/// The air of a moment `wet` (0 dry to 1 wet), and how much of the dome the physical sky covers
/// by full day.
#[must_use]
pub fn air(wet: f32) -> (Air, f32) {
    let lerp = |a: f32, b: f32| a + (b - a) * wet;
    (
        Air {
            mie: lerp(DRY.mie, WET.mie),
            rayleigh: lerp(DRY.rayleigh, WET.rayleigh),
            overcast: lerp(DRY.overcast, WET.overcast),
            exposure: lerp(DRY.exposure, WET.exposure),
        },
        lerp(COVER.0, COVER.1),
    )
}

/// The direction toward the sun in `light`: against the way its light travels.
#[must_use]
pub fn toward_sun(light: &DayLight) -> Vec3 {
    let d = light.light_direction;
    -Vec3::new(d.x, d.y, d.z)
}

/// The dome's opacity in `frame`: none with no dome.
#[must_use]
pub fn dome(frame: &Frame) -> Option<f32> {
    frame
        .sky_before
        .first()
        .filter(|_| frame.sky_dome > 0)
        .map(|b| b.tint.a)
}

/// How much of the day it is, 0 night to 1 day, with the sun toward `sun` and the dome as opaque
/// as `dome`: the sun's elevation shapes dawn and dusk, and the dome's opacity (none at night)
/// says when the day is night. No dome, no day.
#[must_use]
pub fn day(dome: Option<f32>, sun: Vec3) -> f32 {
    smoothstep(-4.0, 6.0, elevation(sun)) * dome.map_or(0.0, |a| a.clamp(0.0, 1.0))
}

/// What the physical sky is drawn from for `frame` seen through `view` and `projection` from
/// `eye`, into a target `extent` pixels: the outdoors' light, the rain's air, and how much of the
/// day it is.
#[must_use]
pub fn light(
    frame: &Frame,
    view: Mat4,
    projection: Mat4,
    eye: Vec3,
    extent: (u32, u32),
) -> SkyLight {
    let outdoors = frame.outside.as_ref().unwrap_or(&frame.light);
    let sun = toward_sun(outdoors);
    let (air, cover) = air(wetness(frame.rain));
    let day = day(dome(frame), sun);
    let c = outdoors.direct_color;
    let f = outdoors.fog_color;
    SkyLight {
        view,
        projection,
        eye,
        extent,
        sun,
        sun_color: Vec3::new(linear(c.r), linear(c.g), linear(c.b)),
        twilight: dereth_gfx_passes::sky::twilight(elevation(sun)),
        air,
        day,
        cover: day * cover,
        fog: Some((
            outdoors.fog_max,
            Vec3::new(linear(f.r), linear(f.g), linear(f.b)),
        )),
        linear: false,
        sky_veil: 0.0,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// A wet moment's air scatters more and its sky is grayer and lets more of the dome through;
    /// the rain moves the air in steps.
    #[test]
    fn a_wet_moments_sky_is_hazier_grayer_and_covers_less_of_the_dome() {
        let (dry, dry_cover) = air(0.0);
        let (wet, wet_cover) = air(1.0);
        assert_eq!(dry, DRY);
        assert_eq!(wet, WET);
        assert!(wet.mie > dry.mie && wet.overcast > dry.overcast);
        assert!(wet_cover < dry_cover);
        assert!((wetness(0.51) - 0.5).abs() < 1e-6, "{}", wetness(0.51));
        assert_eq!(wetness(3.0), 1.0);
    }

    /// The sun's light travels down by day: the sun stands up the other way.
    #[test]
    fn the_sun_stands_against_the_way_its_light_travels() {
        let mut l = DayLight::DEFAULT;
        l.light_direction = auberean_primitives::Vec3::new(0.0, 0.6, -0.8);
        assert!((toward_sun(&l) - Vec3::new(0.0, -0.6, 0.8)).length() < 1e-6);
    }

    /// A high sun over an opaque dome is full day; a dome faded out is night whatever the sun;
    /// with no dome there is no day to draw the physical sky by.
    #[test]
    fn night_is_a_faded_dome_or_a_low_sun_and_no_dome_is_no_day() {
        let high = Vec3::new(0.0, 0.3, 0.9);
        assert!((day(Some(1.0), high) - 1.0).abs() < 1e-6);
        assert_eq!(day(Some(0.0), high), 0.0);
        assert_eq!(day(Some(1.0), Vec3::new(0.0, 1.0, -0.2)), 0.0);
        assert_eq!(day(None, high), 0.0);
    }
}
