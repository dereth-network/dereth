//! The lights objects carry, reaching a draw, the lights lit particles take inside, and a cell's
//! lights baked as it is made ready.

use std::collections::HashMap;

use auberean_primitives::random::Random;
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::inside::{
    carried_in_reach, particle_light, BlockLights, Light, LightKind, ParticleLight,
};

fn light(cell: u32, at: Vec3, kind: LightKind, most_range: f32) -> Light {
    Light {
        cell,
        origin: at,
        kind,
        direction: Vec3::ZERO,
        color: Color::WHITE,
        specular: Color::WHITE,
        intensity: 1.0,
        range: most_range,
        flicker: None,
        most_intensity: None,
        most_range,
        lit: HashMap::new(),
    }
}

/// A light an object carries reaches a draw within its most range and the draw's reach: a point
/// light only on the draw's side of inside and out, a sky light only inside; across a block's edge
/// its place is taken from its own block.
///
/// Behaviour: world.inside.a-carried-light-reaches-draws-on-its-side-within-its-range
#[test]
fn a_carried_light_reaches_draws_on_its_side_within_its_range() {
    let (inside_cell, outside_cell) = (0x0298_0120, 0x0298_0021);
    let at = Vec3::new(100.0, 100.0, 0.0);
    let carried = [
        light(
            inside_cell,
            Vec3::new(104.0, 100.0, 0.0),
            LightKind::Point,
            5.0,
        ),
        light(
            inside_cell,
            Vec3::new(100.0, 100.0, 20.0),
            LightKind::Sky,
            30.0,
        ),
        light(
            outside_cell,
            Vec3::new(100.0, 103.0, 0.0),
            LightKind::Point,
            5.0,
        ),
        // the next block east, 10 m from a draw by the edge
        light(
            0x0398_0021,
            Vec3::new(5.0, 100.0, 0.0),
            LightKind::Point,
            8.0,
        ),
    ];
    assert_eq!(carried_in_reach(&carried, inside_cell, at, 0.5), [0, 1]);
    assert_eq!(carried_in_reach(&carried, outside_cell, at, 0.5), [2]);
    // A light 6 m off with a range of 5 reaches a draw reaching 1.5 m about it, not 0.5 m.
    let far = [light(
        inside_cell,
        Vec3::new(106.0, 100.0, 0.0),
        LightKind::Point,
        5.0,
    )];
    assert!(carried_in_reach(&far, inside_cell, at, 0.5).is_empty());
    assert_eq!(carried_in_reach(&far, inside_cell, at, 1.5), [0]);
    let edge = Vec3::new(155.0, 100.0, 0.0);
    assert_eq!(carried_in_reach(&carried, outside_cell, edge, 3.0), [3]);
    assert!(carried_in_reach(&carried, outside_cell, edge, 1.0).is_empty());
}

/// A lit particle inside takes the first five lights its system keeps: a point light gives its
/// color times its intensity over `1 + 10 d² / R²` for its distance `d` from the particle, a
/// directional light the same not fading, a sky light nothing though it takes one of the five; the
/// sixth light kept gives nothing. Their sum lights the particle.
///
/// Behaviour: world.inside.a-lit-particle-takes-the-first-five-lights-kept
#[test]
fn a_lit_particle_takes_the_first_five_lights_kept() {
    let at = Vec3::new(10.0, 10.0, 0.0);
    let red = |mut l: Light| {
        l.color = Color::new(1.0, 0.0, 0.0, 1.0);
        l.intensity = 0.5;
        l
    };
    let lights = BlockLights {
        lights: vec![
            light(
                0x0298_0120,
                Vec3::new(10.0, 10.0, 5.0),
                LightKind::Sky,
                50.0,
            ),
            red(light(
                0x0298_0120,
                Vec3::new(12.0, 10.0, 0.0),
                LightKind::Point,
                4.0,
            )),
            light(0x0298_0120, Vec3::ZERO, LightKind::Directional, 0.0),
            light(
                0x0298_0120,
                Vec3::new(10.0, 13.0, 0.0),
                LightKind::Point,
                3.0,
            ),
            light(
                0x0298_0120,
                Vec3::new(10.0, 10.0, 1.0),
                LightKind::Point,
                1.0,
            ),
            light(
                0x0298_0120,
                Vec3::new(10.0, 10.0, 0.0),
                LightKind::Point,
                9.0,
            ),
        ],
        ..BlockLights::default()
    };
    let kept = [0, 1, 2, 3, 4, 5];
    let taken = lights.particle_lights(&kept);
    assert_eq!(
        taken
            .iter()
            .map(|l| (l.origin, l.range))
            .collect::<Vec<_>>(),
        [
            (Vec3::new(12.0, 10.0, 0.0), Some(4.0)),
            (Vec3::ZERO, None),
            (Vec3::new(10.0, 13.0, 0.0), Some(3.0)),
            (Vec3::new(10.0, 10.0, 1.0), Some(1.0)),
        ],
        "the sky light and the sixth give nothing"
    );
    assert_eq!(taken[0].color, Color::new(0.5, 0.0, 0.0, 1.0));
    // The red light 2 m off with a range of 4: 0.5 / (1 + 10 * 4 / 16) = 0.5 / 3.5.
    let red_part = 0.5 / 3.5;
    // The white lights: 1 (directional), 1 / (1 + 10 * 9 / 9) = 1/11, 1 / (1 + 10) = 1/11.
    let white = 1.0 + 1.0 / 11.0 + 1.0 / 11.0;
    let c = particle_light(&taken, at, 0.0, &Random::new());
    let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
    assert!(
        near(c.r, red_part + white) && near(c.g, white) && near(c.b, white),
        "{c:?}"
    );
    assert!(near(c.a, 1.0));
    // Six lights handed over light with the first five only.
    let six: Vec<ParticleLight> = (0..6)
        .map(|_| ParticleLight {
            origin: Vec3::ZERO,
            color: Color::new(0.1, 0.1, 0.1, 1.0),
            range: None,
            flicker: None,
        })
        .collect();
    assert!(near(particle_light(&six, at, 0.0, &Random::new()).r, 0.5));
}

/// The lights a cell's record lists for its vertices are baked as they stand at the moment the
/// cell is first made ready: a light whose intensity bounces bakes its value then (at its lowest
/// at the start of its swing, its highest a quarter turn later), and the vertices lit on top of
/// the bake (the sun, the kept lights) keep it. Without a moment the bake takes the intensity's
/// start.
///
/// Behaviour: world.inside.a-cells-record-lights-are-baked-as-they-stand-at-that-moment
#[test]
fn a_cells_record_lights_are_baked_as_they_stand_at_that_moment() {
    use auberean_assets::common::Waveform;
    use auberean_world::inside::{CellVertex, Outside};
    use auberean_world::math::Mat4;
    let cell = 0x0100_0100;
    let mut l = light(cell, Vec3::new(0.0, 0.0, 1.0), LightKind::Point, 10.0);
    l.intensity = 0.2;
    l.flicker = Some(Waveform {
        kind: 6,
        base: 0.2,
        amplitude: 0.6,
        frequency: 1.0,
        ..Waveform::default()
    });
    l.lit.insert(cell, vec![0]);
    let lights = BlockLights {
        lights: vec![l],
        lookup: HashMap::from([(0x0100, vec![0])]),
        record: true,
        ..BlockLights::default()
    };
    let floor = [CellVertex {
        position: Vec3::ZERO,
        normal: Vec3::new(0.0, 0.0, 1.0),
        tangent_s: Vec3::new(1.0, 0.0, 0.0),
        tangent_t: Vec3::new(0.0, 1.0, 0.0),
    }];
    let rng = Random::new();
    let red = |at: Option<(f64, &Random)>| {
        lights.cell_bake(cell, &Mat4::IDENTITY, &floor, at).color[0][0]
    };
    let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
    // 1 m under the light of range 10: the intensity over 1.1
    assert!(near(red(None), 0.2 / 1.1), "{}", red(None));
    assert!(near(red(Some((0.0, &rng))), 0.2 / 1.1));
    let quarter = std::f64::consts::FRAC_PI_2;
    assert!(
        near(red(Some((quarter, &rng))), 0.8 / 1.1),
        "{}",
        red(Some((quarter, &rng)))
    );
    let bake = lights.cell_bake(cell, &Mat4::IDENTITY, &floor, Some((quarter, &rng)));
    let night = Outside {
        sun: Color::BLACK,
        brightness: 0.0,
        direction: Vec3::new(0.0, 0.0, -1.0),
        ambient: Color::BLACK,
    };
    let lit = lights.cell_light(&bake, cell, &Mat4::IDENTITY, &floor, &[], &night, 4);
    assert!(near(lit[0].color[0], 0.8 / 1.1));
    let plain = lights.cell_vertices(cell, &Mat4::IDENTITY, &floor, &[], &night, 4);
    assert!(near(plain[0].color[0], 0.2 / 1.1));
}
