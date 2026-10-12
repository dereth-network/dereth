//! Behaviour: none (experimental Horizon rendering effects: how the exposure is keyed to a frame
//! indoors and how the eye adjusts to it from one frame to the next)

use dereth_render_hifi::passes::lighting::{exposure_step, shaders, Adaptation, ExposureStep};

use crate::common::wgsl_eval::{Evaluator, Value};

fn exposure_shader() -> Evaluator {
    let source = shaders()
        .into_iter()
        .find(|(n, _)| *n == "exposure")
        .expect("the exposure shader")
        .1;
    Evaluator::new(&source)
}

fn keyed(eval: &Evaluator, mean: f32, a: &Adaptation) -> f32 {
    eval.call(
        "keyed",
        &[
            Value::F32(mean),
            Value::F32(a.key),
            Value::F32(a.strength),
            Value::F32(a.least),
            Value::F32(a.most),
        ],
    )
    .f32()
}

fn adapt(eval: &Evaluator, before: f32, goal: f32, dt: f32, a: &Adaptation) -> f32 {
    eval.call(
        "adapt",
        &[
            Value::F32(before),
            Value::F32(goal),
            Value::F32(dt),
            Value::F32(a.into_dark),
            Value::F32(a.into_light),
        ],
    )
    .f32()
}

#[test]
fn indoors_the_exposure_is_keyed_to_the_frame_and_eased_toward() {
    // The first frame, and any after a gap the eye has not kept its adaptation over, takes its
    // exposure at once.
    assert_eq!(
        exposure_step(true, None, None),
        ExposureStep {
            keyed: true,
            at_once: true,
            dt: 0.0
        }
    );
    assert!(exposure_step(true, Some(2.0), Some(2.0)).at_once);
    // Frame after frame it eases.
    let step = exposure_step(true, Some(0.016), Some(0.016));
    assert!(step.keyed && !step.at_once);
    assert!((step.dt - 0.016).abs() < 1e-6);
    // A long frame counts for a quarter of a second at most.
    assert_eq!(exposure_step(true, Some(0.4), Some(0.4)).dt, 0.25);
}

#[test]
fn outdoors_the_exposure_is_the_sun_and_skys_at_once_unless_the_viewer_has_just_come_out() {
    let never_in = exposure_step(false, Some(0.016), None);
    assert!(!never_in.keyed && never_in.at_once);
    let just_out = exposure_step(false, Some(0.016), Some(0.5));
    assert!(!just_out.keyed && !just_out.at_once);
    let long_out = exposure_step(false, Some(0.016), Some(4.0));
    assert!(long_out.at_once);
}

#[test]
fn a_darker_frame_is_exposed_brighter_within_the_least_and_the_greatest_exposure() {
    let eval = exposure_shader();
    let a = Adaptation::indoors();
    let means = [0.002f32, 0.01, 0.03, 0.08, 0.2, 0.6, 2.0];
    let exposures: Vec<f32> = means.iter().map(|m| keyed(&eval, *m, &a)).collect();
    for w in exposures.windows(2) {
        assert!(w[1] <= w[0], "{exposures:?}");
    }
    for e in &exposures {
        assert!(*e >= a.least && *e <= a.most, "{exposures:?}");
    }
    assert_eq!(
        exposures[0], a.most,
        "a black frame is not pushed past the greatest exposure"
    );
    assert_eq!(*exposures.last().unwrap(), a.least);
    // A frame already at the key keeps an exposure of one.
    let at_key = keyed(&eval, a.key, &a);
    assert!((at_key - 1.0).abs() < 1e-5, "{at_key}");
}

#[test]
fn the_eye_eases_toward_the_exposure_more_slowly_into_the_dark_and_stays_once_there() {
    let eval = exposure_shader();
    let a = Adaptation::indoors();
    // Reached, it stays exactly where it is.
    assert_eq!(adapt(&eval, 1.8, 1.8, 0.016, &a), 1.8);
    // A frame's step goes part of the way, in the logarithm of the exposure.
    let up = adapt(&eval, 1.0, 4.0, 0.1, &a);
    let down = adapt(&eval, 4.0, 1.0, 0.1, &a);
    assert!(up > 1.0 && up < 4.0, "{up}");
    assert!(down < 4.0 && down > 1.0, "{down}");
    // Into the dark (a brighter exposure) is slower than into the light.
    let ln = dereth_primitives::num::math::logf;
    let up_share = ln(up) / ln(4.0);
    let down_share = ln(4.0 / down) / ln(4.0);
    assert!(
        up_share < down_share,
        "{up_share} into the dark, {down_share} into the light"
    );
    // Over some seconds it gets there.
    let mut e = 1.0f32;
    for _ in 0..(10 * 60) {
        e = adapt(&eval, e, 4.0, 1.0 / 60.0, &a);
    }
    assert!((e - 4.0).abs() < 0.05, "{e}");
}
