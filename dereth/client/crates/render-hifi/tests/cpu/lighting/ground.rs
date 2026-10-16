//! Behaviour: none (experimental hi-fi art packs: how an art pack's ground takes the old light
//! that the light per pixel divides out)

use dereth_render_hifi::passes::ground::shader_source;
use dereth_render_hifi::shared::fullscreen::COLOUR_WGSL;

use crate::common::wgsl_eval::{Evaluator, Value};

/// The old light at dusk on level ground, 0..1 a channel as the vertex light holds it: a purple
/// ambient (200, 100, 255) at 0.45 and a sun five degrees up (235, 220, 202) at 0.36.
const DUSK: [f32; 3] = [0.382, 0.203, 0.475];
/// The old light at noon on level ground.
const NOON: [f32; 3] = [0.9, 0.88, 0.85];

/// Linear colors of the ground: grass, dirt, sand, snow and dark rock.
const COLORS: [[f32; 3]; 5] = [
    [0.17, 0.21, 0.02],
    [0.12, 0.07, 0.03],
    [0.55, 0.42, 0.22],
    [0.8, 0.82, 0.86],
    [0.05, 0.05, 0.06],
];

#[test]
fn an_art_pack_ground_under_the_old_light_is_what_the_re_shade_makes_of_a_retail_picture_of_its_color(
) {
    let ground = Evaluator::new(&shader_source());
    let color = Evaluator::new(COLOUR_WGSL);
    for light in [DUSK, NOON] {
        for albedo in COLORS {
            let got = ground
                .call(
                    "lit_as_drawn",
                    &[Value::vector(&albedo), Value::vector(&light)],
                )
                .floats();
            // Retail's ground: its picture in the display's encoding times the vertex light,
            // decoded as the re-shaded world decodes every color it draws.
            let shown = color
                .call("linear_to_srgb", &[Value::vector(&albedo)])
                .floats();
            let lit: Vec<f32> = shown.iter().zip(light).map(|(c, l)| c * l).collect();
            let want = color
                .call("srgb_to_linear", &[Value::vector(&lit)])
                .floats();
            for k in 0..3 {
                assert!(
                    (got[k] - want[k]).abs() <= want[k] * 1e-4 + 1e-7,
                    "{albedo:?} under {light:?}: {got:?} against {want:?}"
                );
            }
        }
    }
    // Under the dim purple light of dusk, that keeps a fifth more of grass's red than the
    // decoded color times the decoded light, and over half again its green: the share the light
    // per pixel leaves in retail's ground when it divides the old light out.
    let grass = ground
        .call(
            "lit_as_drawn",
            &[Value::vector(&COLORS[0]), Value::vector(&DUSK)],
        )
        .floats();
    let apart: Vec<f32> = COLORS[0]
        .iter()
        .zip(DUSK)
        .map(|(c, l)| c * dereth_primitives::num::math::powf(l, 2.2))
        .collect();
    assert!(grass[0] > apart[0] * 1.15, "{grass:?} {apart:?}");
    assert!(grass[1] > apart[1] * 1.5, "{grass:?} {apart:?}");
}
