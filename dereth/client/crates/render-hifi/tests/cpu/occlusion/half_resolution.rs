//! Behaviour: none (experimental Horizon rendering effects: the ambient occlusion's search at
//! half resolution on a large picture, and the pixels' term taken back from it across edges)

use dereth_render_hifi::passes::gtao::{search_grid, shader_source};

use crate::common::wgsl_eval::{Evaluator, Value};

/// The visibility the shader gives a pixel `dist` metres away, from texels of visibility `ao` at
/// distances `at`, weighted by nearness `near`.
fn upsample(dist: f32, ao: [f32; 4], at: [f32; 4], near: [f32; 4]) -> f32 {
    Evaluator::new(shader_source())
        .call(
            "upsample",
            &[
                Value::F32(dist),
                Value::vector(&ao),
                Value::vector(&at),
                Value::vector(&near),
            ],
        )
        .f32()
}

/// A pixel's shares of nearness to the four texels round it, the first nearest and the last
/// farthest.
const QUARTER: [f32; 4] = [9.0 / 16.0, 3.0 / 16.0, 3.0 / 16.0, 1.0 / 16.0];

#[test]
fn the_search_works_at_half_resolution_only_on_a_picture_larger_than_1440p() {
    for (w, h) in [(1280, 720), (1920, 1080), (2560, 1080), (2560, 1440)] {
        assert_eq!(search_grid(w, h), 1, "{w}x{h}");
    }
    for (w, h) in [(3440, 1440), (3840, 2160), (5120, 2160), (7680, 4320)] {
        assert_eq!(search_grid(w, h), 2, "{w}x{h}");
    }
}

#[test]
fn a_pixel_on_a_near_edge_takes_none_of_the_open_ground_behind_it() {
    // A post five metres off, its texels shaded; behind it the open ground forty metres off. The
    // pixel on the post, nearest the ground's texels, keeps the post's shade.
    let ao = upsample(5.0, [1.0, 1.0, 0.4, 0.4], [40.0, 40.0, 5.0, 5.0], QUARTER);
    assert!((ao - 0.4).abs() < 0.01, "the post's pixel took {ao}");
}

#[test]
fn a_pixel_behind_a_near_edge_takes_none_of_its_shade() {
    // The same edge from the far side: the ground's pixel, nearest the post's shaded texel, stays
    // open.
    let ao = upsample(40.0, [0.4, 1.0, 1.0, 0.4], [5.0, 40.0, 40.0, 5.0], QUARTER);
    assert!((ao - 1.0).abs() < 0.01, "the ground's pixel took {ao}");
}

#[test]
fn on_one_surface_the_texels_blend_by_nearness() {
    let (ao, near) = ([0.2, 0.6, 1.0, 0.8], QUARTER);
    let want: f32 = ao.iter().zip(near).map(|(a, n)| a * n).sum();
    let got = upsample(10.0, ao, [10.0, 10.1, 9.9, 10.0], near);
    assert!((got - want).abs() < 0.01, "{got} against {want}");
}

#[test]
fn a_pixel_no_texel_lies_beside_takes_the_texel_nearest_it_in_distance() {
    // A thin thing twelve metres off, too thin for a texel of its own, between a wall at ten
    // metres and ground at thirty: it takes the wall's term, the nearer in distance.
    let ao = upsample(
        12.0,
        [0.5, 0.9, 0.5, 0.9],
        [10.0, 30.0, 10.0, 30.0],
        [0.1, 0.4, 0.1, 0.4],
    );
    assert!((ao - 0.5).abs() < 0.01, "{ao}");
}

#[test]
fn a_pixel_among_texels_none_of_which_was_searched_is_open() {
    let ao = upsample(5.0, [1.0; 4], [-1.0; 4], QUARTER);
    assert!((ao - 1.0).abs() < 1e-6, "{ao}");
    // One searched texel, however far, is taken over none.
    let ao = upsample(5.0, [1.0, 1.0, 1.0, 0.3], [-1.0, -1.0, -1.0, 50.0], QUARTER);
    assert!((ao - 0.3).abs() < 0.01, "{ao}");
}
