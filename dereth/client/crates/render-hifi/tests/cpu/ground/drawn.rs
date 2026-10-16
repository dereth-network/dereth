//! Behaviour: none (the drawn ground, an art-pack presentation's own: the presentation draws the
//! ground from the same numbers the scene moves what stands on it by)

use dereth_render_hifi::passes::drawn::shader_source;
use dereth_terrain::land::drawn::{far_relief, lattice_hash, value_noise};
use dereth_terrain::land::mesh::sw_to_ne_cut;

use crate::common::wgsl_eval::{Evaluator, Value};

#[test]
fn the_drawn_ground_cuts_each_cell_as_retail_does() {
    let shader = Evaluator::new(&shader_source());
    for (x, y) in [
        (0, 0),
        (1, 0),
        (0, 1),
        (1355, 1440),
        (2039, 2039),
        (812, 77),
        (77, 812),
    ] {
        let got = shader
            .call(
                "sw_to_ne",
                &[Value::Vector(vec![Value::I32(x), Value::I32(y)])],
            )
            .bool();
        assert_eq!(got, sw_to_ne_cut(x, y), "cell ({x}, {y})");
    }
}

#[test]
fn the_far_grounds_relief_is_the_scenes_to_the_float() {
    let shader = Evaluator::new(&shader_source());
    for (i, j) in [
        (-3, 5),
        (0, 0),
        (7, 11),
        (40_123, 38_004),
        (-1, -1),
        (65_535, 2),
    ] {
        let got = shader
            .call(
                "lattice_hash",
                &[Value::Vector(vec![Value::I32(i), Value::I32(j)])],
            )
            .f32();
        assert!((got - lattice_hash(i, j)).abs() < 1e-7, "({i}, {j})");
    }
    for (x, y, cell) in [
        (23_311.4, 30_233.9, 6.0),
        (100.25, 7.75, 96.0),
        (48_000.0, 120.5, 24.0),
    ] {
        let got = shader
            .call("value_noise", &[Value::vector(&[x, y]), Value::F32(cell)])
            .f32();
        assert!((got - value_noise(x, y, cell)).abs() < 1e-5, "({x}, {y})");
    }
    for (x, y, slope) in [
        (28_115.0, 40_140.0, 0.05),
        (23_311.4, 30_233.9, 0.9),
        (1_000.5, 47_000.25, 2.0),
    ] {
        let got = shader
            .call(
                "far_relief",
                &[
                    Value::vector(&[x, y]),
                    Value::F32(96.0),
                    Value::F32(1.5),
                    Value::F32(slope),
                ],
            )
            .f32();
        let want = far_relief(x, y, 96.0, 1.5, slope);
        assert!(
            (got - want).abs() < 1e-4,
            "({x}, {y}): {got} against {want}"
        );
    }
}

#[test]
fn the_fine_relief_is_gone_where_the_finest_level_meets_the_next() {
    let shader = Evaluator::new(&shader_source());
    let share = |level: f32, k: f32| {
        shader
            .call("fine_share_of", &[Value::F32(level), Value::F32(k)])
            .f32()
    };
    assert!(
        (share(0.0, 0.0) - 1.0).abs() < 1e-6,
        "whole inside the ring"
    );
    assert!((share(0.0, 0.5) - 0.5).abs() < 1e-6);
    assert!(share(0.0, 1.0).abs() < 1e-6, "none at the ring's edge");
    assert!(share(1.0, 0.0).abs() < 1e-6, "none on a coarser level");
}

#[test]
fn the_drawn_grounds_mesh_slides_its_vertices_as_the_patches_are_checked() {
    use dereth_render_hifi::passes::drawn::{slid, slide};
    use glam::Vec2;
    let shader = Evaluator::new(&shader_source());
    for (p, morph, eye) in [
        ((20_016.0, 19_904.0), (64.0, 80.0), (20_000.0, 20_000.0)),
        ((20_100.5, 20_000.0), (64.0, 80.0), (20_000.0, 20_000.0)),
        ((31_000.0, 8_000.0), (409.6, 512.0), (31_744.0, 8_192.0)),
    ] {
        let got = shader
            .call(
                "slid",
                &[
                    Value::vector(&[p.0, p.1]),
                    Value::vector(&[morph.0, morph.1]),
                    Value::vector(&[eye.0, eye.1]),
                ],
            )
            .f32();
        let want = slid(Vec2::new(p.0, p.1), morph, Vec2::new(eye.0, eye.1));
        assert!((got - want).abs() < 1e-5, "{p:?}: {got} against {want}");
    }
    for (grid, k) in [((3.0, 4.0), 0.5), ((16.0, 7.0), 1.0), ((5.0, 5.0), 0.25)] {
        let got = shader
            .call(
                "slide",
                &[
                    Value::vector(&[1_000.0, 2_000.0]),
                    Value::vector(&[grid.0, grid.1]),
                    Value::F32(4.0),
                    Value::F32(k),
                ],
            )
            .floats();
        let want = slide(
            Vec2::new(1_000.0, 2_000.0),
            Vec2::new(grid.0, grid.1),
            4.0,
            k,
        );
        assert!(
            (got[0] - want.x).abs() < 1e-3 && (got[1] - want.y).abs() < 1e-3,
            "{grid:?}: {got:?} against {want}"
        );
    }
}

#[test]
fn a_cut_out_edge_blended_over_the_drawn_ground_keeps_its_own_color() {
    let drawn = Evaluator::new(&shader_source());
    let ground = Evaluator::new(&dereth_render_hifi::passes::ground::shader_source());
    for exact in [0.6f32, 13.25, 63.9, 64.0, 911.4, 29_870.0] {
        let code = drawn.call("distance_code", &[Value::F32(exact)]).floats();
        // Read back where nothing was drawn over it.
        let back = ground.call("coded_distance", &[Value::vector(&code)]).f32();
        assert!(
            (back - exact).abs() < 0.01,
            "{exact} m reads back as {back}"
        );
        let whole = ground.call("ground_share", &[Value::vector(&code)]).f32();
        assert!(
            (whole - 1.0).abs() < 1e-6,
            "the ground's whole share: {whole}"
        );
        // A leaf's edge, white-green and a share of the pixel, blended over it: the distance
        // carries under two hundredths into its color, and the edge's own color is read back.
        let leaf = [0.62f32, 0.71, 0.66];
        for a in [0.15f32, 0.5, 0.9] {
            let blended: Vec<f32> = (0..3)
                .map(|c| leaf[c] * a + code[c] * (1.0 - a))
                .chain([1.0])
                .collect();
            for c in 0..2 {
                assert!(
                    (blended[c] - leaf[c] * a).abs() < 0.02,
                    "{exact} m under an edge of {a}: channel {c} carries {}",
                    blended[c] - leaf[c] * a
                );
            }
            let share = ground
                .call("ground_share", &[Value::vector(&blended)])
                .f32();
            assert!((share - (1.0 - a)).abs() < 0.01, "{a}: share {share}");
            // Its red and green, where it covers a fair share of the pixel (the ground's material
            // is laid over a thinner edge): its blue told the ground's share, and is taken from
            // the leaf beside it as the pixel is laid.
            if a < 0.5 {
                continue;
            }
            let own = ground
                .call("edge_colour", &[Value::vector(&blended)])
                .floats();
            for c in 0..2 {
                assert!(
                    (own[c] - leaf[c]).abs() < 0.03,
                    "{exact} m under an edge of {a}: {own:?} for {leaf:?}"
                );
            }
        }
    }
}
