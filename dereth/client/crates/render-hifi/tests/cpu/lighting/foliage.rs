//! Behaviour: none (experimental Horizon rendering effects: how an art pack's leaf carries the
//! sun it was drawn with and how open its place in its crown is to the light per pixel)

use dereth_render_hifi::passes::art_pack::shader_source;
use dereth_render_hifi::passes::lighting::shaders;

use crate::common::wgsl_eval::{Evaluator, Value};

fn relight() -> Evaluator {
    let source = shaders()
        .into_iter()
        .find(|(n, _)| *n == "relight")
        .expect("the relight shader")
        .1;
    Evaluator::new(&source)
}

#[test]
fn an_art_pack_leaf_carries_its_sun_and_its_openness_exactly_through_a_half_float() {
    let pack = Evaluator::new(shader_source());
    let relight = relight();
    for step in 0..16u8 {
        for open in 0..16u8 {
            let share = f32::from(step) / 15.0;
            let class = pack
                .call(
                    "leaf_class",
                    &[Value::F32(share), Value::F32(f32::from(open))],
                )
                .f32();
            // A multiple of 1/256 between 5 and 6, which a half float holds exactly.
            assert!((5.0..6.0).contains(&class), "{class}");
            let code = class * 256.0;
            assert!((code - code.round()).abs() < 1e-3, "{class}");
            assert!(relight.call("pack_leaf", &[Value::F32(class)]).bool());
            let got = relight.call("leaf_of", &[Value::F32(class)]).floats();
            assert!((got[0] - share).abs() < 1e-6, "{step} {open}: {got:?}");
            assert!(
                (got[1] - f32::from(open) / 15.0).abs() < 1e-6,
                "{step} {open}: {got:?}"
            );
        }
    }
}

#[test]
fn the_worlds_own_leaves_are_taken_as_open_and_keep_the_sun_they_carry() {
    let relight = relight();
    let near = |kind: f32, want: [f32; 2]| {
        let got = relight.call("leaf_of", &[Value::F32(kind)]).floats();
        assert!(
            (got[0] - want[0]).abs() < 1e-5 && (got[1] - want[1]).abs() < 1e-5,
            "{kind}: {got:?}"
        );
    };
    // Half the sun, carried; and none carried, taken as a little over half.
    near(4.25, [0.5, 1.0]);
    near(4.0, [0.55, 1.0]);
    assert!(!relight.call("pack_leaf", &[Value::F32(4.4)]).bool());
    assert!(!relight.call("pack_leaf", &[Value::F32(1.1)]).bool());
}

#[test]
fn an_art_pack_leaf_in_shade_keeps_the_sun_its_crowns_scatter_more_on_their_outside() {
    let pack = Evaluator::new(shader_source());
    let relight = relight();
    // The light per pixel's leaf numbers: the sky a deep leaf sees, the world's own leaves' share
    // of the sun in shade, and an art pack's leaf's on the outside of its crown and deep inside.
    let numbers = [0.15, 0.4, 0.3, 0.5];
    let sun = |kind: f32, open: f32, shadow: f32| {
        relight
            .call(
                "leaf_sun",
                &[
                    Value::F32(kind),
                    Value::F32(open),
                    Value::F32(shadow),
                    Value::vector(&numbers),
                ],
            )
            .f32()
    };
    let near = |got: f32, want: f32| assert!((got - want).abs() < 1e-5, "{got} against {want}");
    let leaf = pack
        .call("leaf_class", &[Value::F32(0.5), Value::F32(15.0)])
        .f32();
    // In the sun, every leaf takes all of it.
    near(sun(leaf, 1.0, 1.0), 1.0);
    near(sun(leaf, 0.0, 1.0), 1.0);
    near(sun(4.25, 1.0, 1.0), 1.0);
    // In shade, an art pack's leaf keeps the scattered share: all of it on its crown's outside,
    // half of it deep inside, and the share in between where the sun reaches it in part.
    near(sun(leaf, 1.0, 0.0), 0.3);
    near(sun(leaf, 0.0, 0.0), 0.15);
    near(sun(leaf, 1.0, 0.5), 0.65);
    // The world's own leaves keep their fixed share, whatever is open about them.
    near(sun(4.25, 1.0, 0.0), 0.4);
    near(sun(4.25, 0.0, 0.0), 0.4);
}
