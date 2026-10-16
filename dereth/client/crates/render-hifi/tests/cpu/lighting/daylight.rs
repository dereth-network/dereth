//! Behaviour: none (experimental Horizon rendering effects: how the day's weather and the height
//! of the sun share its light between its beam and the sky, and how fine the sun's shadow maps
//! are drawn)

use dereth_primitives::num::math;
use dereth_render_hifi::passes::lighting::{shadow_size, DayLight};
use dereth_render_hifi::snapshot::{HifiFog, HifiSky};
use dereth_render_hifi::{HifiFrame, HifiSettings, Level};
use glam::Vec3;

/// An outdoor frame on a day of `group` with the sun `degrees` up and the fog whole at `fog`
/// metres.
fn day(group: &str, degrees: f32, fog: f32) -> HifiFrame {
    let e = degrees.to_radians();
    HifiFrame {
        sky: HifiSky {
            sun_direction: Vec3::new(math::cosf(e), 0.0, math::sinf(e)),
            sun_brightness: 0.7,
            sun_color: [255, 220, 180],
            ambient_level: 0.35,
            ambient_color: [230, 230, 255],
            fog: Some(HifiFog {
                min: fog * 0.1,
                max: fog,
                color: [195, 200, 220],
            }),
            day_group: group.to_owned(),
            outdoor: true,
            sees_outside: true,
            ..HifiSky::default()
        },
        ..HifiFrame::default()
    }
}

#[test]
fn under_a_clear_sky_with_the_sun_high_its_beam_is_whole() {
    let light = DayLight::of(&day("Sunny", 60.0, 2400.0));
    assert_eq!(light, DayLight::CLEAR);
    assert_eq!(light.penumbra(), 1.0);
    assert_eq!(light.sky_light([1.0; 3], 2.6), (0.0, 0.0));
}

#[test]
fn a_rainy_days_cloud_takes_most_of_the_beam_and_the_sky_gives_most_of_it_back() {
    let rain = DayLight::of(&day("Rainy", 60.0, 1500.0));
    let cloudy = DayLight::of(&day("Cloudy", 60.0, 2400.0));
    assert_eq!(rain.cover, 1.0);
    assert!(rain.sun < 0.2, "{rain:?}");
    assert!(cloudy.cover > 0.4 && cloudy.cover < 0.8, "{cloudy:?}");
    assert!(cloudy.sun > rain.sun && cloudy.sun < 1.0, "{cloudy:?}");
    // What the beam loses, the sky gives back in part, from all round and about the sun.
    let (all_round, about_sun) = rain.sky_light([1.0; 3], 1.0);
    let lost = 1.0 - rain.sun;
    assert!(all_round > 0.0 && about_sun > 0.0);
    assert!(all_round + about_sun < lost && all_round + about_sun > lost * 0.4);
    // Its shadows soften the more the greyer the sky.
    assert!(rain.penumbra() > cloudy.penumbra() && cloudy.penumbra() > 1.0);
}

#[test]
fn a_low_sun_loses_some_of_its_beam_to_the_air_but_its_shadows_stay_sharp() {
    let evening = DayLight::of(&day("Sunny", 5.0, 600.0));
    assert!(evening.sun < 0.85 && evening.sun > 0.6, "{evening:?}");
    // The evening's own close haze is not a grey sky.
    assert_eq!(evening.cover, 0.0);
    assert_eq!(evening.penumbra(), 1.0);
}

#[test]
fn a_fog_that_closes_in_by_day_is_a_grey_sky() {
    let fog = DayLight::of(&day("Sunny", 60.0, 200.0));
    assert!(fog.cover > 0.9, "{fog:?}");
    assert!(fog.sun < 0.3, "{fog:?}");
}

#[test]
fn the_sun_shadow_maps_are_drawn_as_fine_as_the_picture_needs_and_no_finer() {
    let on = HifiSettings {
        lighting: Level::Low,
        shadows: Level::Low,
        ..HifiSettings::default()
    };
    assert_eq!(shadow_size(&on, 720), 2048);
    assert_eq!(shadow_size(&on, 1080), 2048);
    assert_eq!(shadow_size(&on, 1440), 2048);
    assert_eq!(shadow_size(&on, 2160), 4096);
    assert_eq!(shadow_size(&on, 4320), 4096);
    // No shadow: a map of one texel, only so the light has one to bind.
    let off = HifiSettings {
        lighting: Level::Low,
        ..HifiSettings::default()
    };
    assert_eq!(shadow_size(&off, 1080), 1);
}
