//! Behaviour: none (experimental Horizon rendering effects: how bright the better lighting draws
//! the outdoors against the ordinary picture, by day and at night)

use dereth_primitives::num::math;
use dereth_render_hifi::passes::lighting::{exposure, shaders, OutdoorHold};
use dereth_render_hifi::snapshot::HifiSky;
use dereth_render_hifi::HifiFrame;
use glam::Vec3;

use crate::common::wgsl_eval::{Evaluator, Value};

/// An outdoor frame with the sun `degrees` over the horizon (under it when negative), at the
/// brightness and colors of the game's evening and night.
fn outdoors(degrees: f32) -> HifiFrame {
    let e = degrees.to_radians();
    HifiFrame {
        sky: HifiSky {
            sun_direction: Vec3::new(math::cosf(e), 0.0, math::sinf(e)),
            sun_brightness: 0.4,
            sun_color: [255, 200, 150],
            ambient_level: 0.4,
            ambient_color: [120, 110, 150],
            outdoor: true,
            sees_outside: true,
            ..HifiSky::default()
        },
        ..HifiFrame::default()
    }
}

#[test]
fn once_the_sun_stands_a_few_degrees_up_the_nights_lift_of_the_exposure_is_gone() {
    let base = exposure(&outdoors(60.0), 2.6, 0.85);
    // The evening sun, five and ten degrees up, is drawn at the sun and the sky's own exposure,
    // as noon is: the dim light of a low sun is not lifted past the ordinary picture.
    for degrees in [5.0f32, 10.0, 20.0] {
        let e = exposure(&outdoors(degrees), 2.6, 0.85);
        assert!((e - base).abs() < 1e-5, "{degrees}: {e} against {base}");
    }
    // At night, and with the sun just at the horizon, the lift stays, so the night reads.
    let night = exposure(&outdoors(-10.0), 2.6, 0.85);
    assert!(night > base * 1.5, "{night} against {base}");
    let setting = exposure(&outdoors(2.0), 2.6, 0.85);
    assert!(setting > base && setting < night, "{setting}");
}

#[test]
fn by_day_the_outdoors_is_held_to_the_ordinary_pictures_brightness_and_at_night_only_from_above() {
    let day = OutdoorHold::of(&outdoors(30.0));
    assert_eq!(
        day.least, 1.0,
        "by day never darker than the ordinary picture"
    );
    assert!(
        day.most > 1.0 && day.most <= 1.15,
        "a little brighter at most: {}",
        day.most
    );
    let night = OutdoorHold::of(&outdoors(-10.0));
    assert_eq!(
        night.least, 0.0,
        "the night is drawn as dark as the light draws it"
    );
    assert_eq!(night.most, day.most);
}

#[test]
fn between_two_rungs_the_exposure_is_found_where_the_frames_brightness_meets_its_bound() {
    let source = shaders()
        .into_iter()
        .find(|(n, _)| *n == "exposure")
        .expect("the exposure shader")
        .1;
    let eval = Evaluator::new(&source);
    let between = |lo: f32, r_lo: f32, hi: f32, r_hi: f32, bound: f32| {
        eval.call(
            "between_rungs",
            &[
                Value::F32(lo),
                Value::F32(r_lo),
                Value::F32(hi),
                Value::F32(r_hi),
                Value::F32(bound),
            ],
        )
        .f32()
    };
    // At the lower rung's brightness, the lower rung; at the upper's, the upper; halfway between
    // them, halfway along the logarithm of the exposure.
    assert!((between(0.5, 1.0, 1.0, 1.4, 1.0) - 0.5).abs() < 1e-6);
    assert!((between(0.5, 1.0, 1.0, 1.4, 1.4) - 1.0).abs() < 1e-6);
    let half = between(0.5, 1.0, 1.0, 1.4, 1.2);
    assert!((half - 0.5f32.sqrt()).abs() < 1e-5, "{half}");
    // Never past either rung.
    assert!((between(0.5, 1.0, 1.0, 1.4, 2.0) - 1.0).abs() < 1e-6);
}
