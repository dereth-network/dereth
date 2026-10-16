//! Behaviour: none (the water, an art-pack presentation's own: the bed, the edge and the ripples
//! are the same arithmetic wherever they are drawn)

use dereth_render_hifi::passes::water::field::bed_depth;
use dereth_render_hifi::passes::water::{layer_constants, ripples, shader_source};
use dereth_render_hifi::snapshot::HifiWater;

use crate::common::wgsl_eval::{Evaluator, Value};

#[test]
fn the_bed_lies_as_deep_in_every_shader_as_the_field_makes_it() {
    let shaders = [
        shader_source(),
        dereth_render_hifi::passes::drawn::shader_source(),
        dereth_render_hifi::passes::ground::shader_source(),
    ];
    for source in &shaders {
        let shader = Evaluator::new(source);
        for (inside, depth, shelf) in [
            (-3.0, 2.5, 30.0),
            (0.0, 2.5, 30.0),
            (0.5, 2.5, 30.0),
            (12.0, 2.5, 30.0),
            (29.0, 2.5, 30.0),
            (400.0, 2.5, 30.0),
            (6.0, 0.0, 30.0),
            (6.0, 4.0, 1.0),
        ] {
            let got = shader
                .call(
                    "bed_depth",
                    &[Value::F32(inside), Value::F32(depth), Value::F32(shelf)],
                )
                .f32();
            let want = bed_depth(inside, depth, shelf);
            assert!((got - want).abs() < 1e-5, "{inside}: {got} against {want}");
        }
    }
}

#[test]
fn the_bed_falls_away_from_the_waters_edge_steepest_there_and_levels_out_at_its_depth() {
    assert!(bed_depth(0.0, 2.5, 30.0).abs() < 1e-6, "none at the edge");
    assert!(bed_depth(-5.0, 2.5, 30.0).abs() < 1e-6, "none on land");
    assert!((bed_depth(30.0, 2.5, 30.0) - 2.5).abs() < 1e-6);
    assert!((bed_depth(300.0, 2.5, 30.0) - 2.5).abs() < 1e-6);
    let steps: Vec<f32> = (0..30u8)
        .map(|k| {
            let s = f32::from(k);
            bed_depth(s + 1.0, 2.5, 30.0) - bed_depth(s, 2.5, 30.0)
        })
        .collect();
    assert!(steps.iter().all(|d| *d >= 0.0), "deeper inward");
    assert!(
        steps.windows(2).all(|w| w[1] <= w[0] + 1e-6),
        "steepest at the edge"
    );
}

#[test]
fn the_water_thins_to_nothing_at_a_shelving_edge_and_is_whole_past_a_banks_lip() {
    let shader = Evaluator::new(&shader_source());
    let share = |inside: f32, soft: f32, deep: f32| {
        shader
            .call(
                "edge_share",
                &[
                    Value::F32(inside),
                    Value::F32(soft),
                    Value::F32(deep),
                    Value::F32(deep * 3.0),
                ],
            )
            .f32()
    };
    // A shore shelving into the water: shallow at its edge.
    assert!(share(0.0, 1.5, 0.1).abs() < 1e-6, "nothing at the edge");
    assert!(share(-2.0, 1.5, 0.1).abs() < 1e-6, "nothing on land");
    assert!(share(0.5, 1.5, 0.0).abs() < 1e-6, "nothing over no depth");
    assert!(
        (share(1.5, 1.5, 0.1) - 1.0).abs() < 1e-6,
        "whole past the softness"
    );
    let mid = share(0.75, 1.5, 0.1);
    assert!(mid > 0.2 && mid < 0.8, "{mid}");
    // A softer shore comes in farther out.
    assert!(share(1.0, 4.0, 0.1) < share(1.0, 1.0, 0.1));
    // Past a bank's lip, the bed seen lies well under the surface: the water is whole there, so
    // no pale line of the far bed shows through it at its edge.
    assert!((share(0.05, 1.5, 1.5) - 1.0).abs() < 1e-6);
}

#[test]
fn the_ripples_hold_still_while_the_clock_does_and_drift_while_it_runs() {
    let water = HifiWater::default();
    let clock = 98_765.25;
    assert_eq!(
        layer_constants(&water, clock),
        layer_constants(&water, clock)
    );
    let later = layer_constants(&water, clock + 1.0 / 30.0);
    assert_ne!(layer_constants(&water, clock), later);
    for l in later {
        assert!((0.0..1.0).contains(&l[3]), "{l:?}");
    }
    // Held at no speed, they stand still however the clock runs.
    let still = HifiWater {
        ripple_speed: 0.0,
        ..water
    };
    assert_eq!(
        layer_constants(&still, 10.0),
        layer_constants(&still, 5_000.0)
    );
    // A layer drifts a whole repeat and is where it started: no jump however long the clock runs.
    let layer = ripples::LAYERS[0];
    let period = f64::from(layer.repeat) / f64::from(layer.drift);
    let a = ripples::drift(&layer, 1.0e6, 1.0, 1.0);
    let b = ripples::drift(&layer, 1.0e6 + period, 1.0, 1.0);
    assert!(
        (a - b).abs() < 1e-4 || (a - b).abs() > 1.0 - 1e-4,
        "{a} {b}"
    );
}

#[test]
fn the_rings_round_a_wading_body_spread_outward_with_the_clock_and_fade_within_a_few_meters() {
    let shader = Evaluator::new(&shader_source());
    let rise = |r: f32, clock: f32| {
        shader
            .call("ring_rise", &[Value::F32(r), Value::F32(clock)])
            .f32()
    };
    // Where the rings' slope changes sign going out, millimeter by millimeter, from 1 m to 4 m.
    let crossings = |clock: f32| -> Vec<f32> {
        let mut out = Vec::new();
        let mut last = rise(1.0, clock);
        for k in 1..=3000u16 {
            let r = 1.0 + f32::from(k) * 0.001;
            let now = rise(r, clock);
            if (now > 0.0) != (last > 0.0) {
                out.push(r);
            }
            last = now;
        }
        out
    };
    let clock = 123.5;
    let now = crossings(clock);
    assert!(now.len() >= 8, "{now:?}");
    // The same clock, the same rings; a twentieth of a second later every ring has spread
    // outward by as far as rings 0.6 m apart, 0.8 a second, spread in it.
    assert_eq!(now, crossings(clock));
    let later = crossings(clock + 0.05);
    for r in &now[..now.len() - 1] {
        let moved = later
            .iter()
            .map(|q| q - r)
            .filter(|d| *d > 0.0)
            .fold(f32::MAX, f32::min);
        assert!(
            (moved - 0.024).abs() < 0.002,
            "the ring at {r} m spread {moved} m"
        );
    }
    // None at the legs or past a few meters, and less the farther out.
    let most = |from: f32, to: f32| {
        (0..=200u16)
            .map(|k| rise(from + (to - from) * f32::from(k) / 200.0, clock).abs())
            .fold(0.0f32, f32::max)
    };
    assert!(most(0.0, 0.2) == 0.0 && most(6.0, 9.0) == 0.0);
    assert!(
        most(0.6, 1.6) > 2.0 * most(3.0, 4.0),
        "{} {}",
        most(0.6, 1.6),
        most(3.0, 4.0)
    );
}

#[test]
fn the_ripples_picture_repeats_seamlessly_down_to_one_texel() {
    let levels = ripples::picture();
    let side = ripples::SIDE as usize;
    assert_eq!(levels.len(), side.trailing_zeros() as usize + 1);
    assert_eq!(levels[0].len(), side * side * 4);
    assert_eq!(levels.last().map(Vec::len), Some(4));
    let at = |x: usize, y: usize, c: usize| i32::from(levels[0][(y * side + x) * 4 + c]);
    // Across the seam the slopes change no more than between neighbors inside the picture.
    let step = |x0: usize, y0: usize, x1: usize, y1: usize| {
        (0..2)
            .map(|c| (at(x0, y0, c) - at(x1, y1, c)).abs())
            .max()
            .unwrap_or(0)
    };
    let inside = (0..side)
        .flat_map(|y| (0..side - 1).map(move |x| (x, y)))
        .map(|(x, y)| step(x, y, x + 1, y))
        .chain(
            (0..side - 1)
                .flat_map(|y| (0..side).map(move |x| (x, y)))
                .map(|(x, y)| step(x, y, x, y + 1)),
        )
        .max()
        .unwrap_or(0);
    let seam_x = (0..side)
        .map(|y| step(side - 1, y, 0, y))
        .max()
        .unwrap_or(0);
    let seam_y = (0..side)
        .map(|x| step(x, side - 1, x, 0))
        .max()
        .unwrap_or(0);
    assert!(
        seam_x <= inside && seam_y <= inside,
        "{seam_x} {seam_y} against {inside}"
    );
    // Level on the whole: its one-texel level is a flat surface's.
    let mean = &levels[levels.len() - 1];
    assert!((i32::from(mean[0]) - 128).abs() <= 3 && (i32::from(mean[1]) - 128).abs() <= 3);
}
