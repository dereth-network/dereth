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

/// The lights objects carry join the lights a draw keeps when they reach it: the block's lights it
/// keeps, then the carried lights in reach, all sorted by their weight where the draw stands
/// (strongest first); a draw no carried light reaches keeps its own in their order. The device takes
/// a carried light at its place in the draw's block (across a block's edge too) with its color
/// times its intensity as it stands, and lit particles take it the same way.
///
/// Behaviour: world.inside.carried-lights-join-the-lights-a-draw-keeps-by-their-weight
#[test]
fn carried_lights_join_the_lights_a_draw_keeps_by_their_weight() {
    use auberean_world::inside::{
        device_lights_kept, keep_carried, particle_lights_kept, Kept, Outside,
    };
    let cell = 0x0298_0120;
    let at = Vec3::new(100.0, 100.0, 0.0);
    let night = Outside {
        sun: Color::new(0.0, 0.0, 0.0, 1.0),
        brightness: 0.0,
        direction: Vec3::new(0.0, 0.0, -1.0),
        ambient: Color::new(0.0, 0.0, 0.0, 1.0),
    };
    // The block's: a dim light 4 m off and one 2 m off, both of range 10.
    let dim = |mut l: Light| {
        l.intensity = 0.2;
        l.most_intensity = Some(0.2);
        l
    };
    let lights = BlockLights {
        lights: vec![
            dim(light(
                cell,
                Vec3::new(104.0, 100.0, 0.0),
                LightKind::Point,
                10.0,
            )),
            light(cell, Vec3::new(102.0, 100.0, 0.0), LightKind::Point, 10.0),
        ],
        ..BlockLights::default()
    };
    let own = [1, 0];
    // A torch carried 1 m off, orange; another in the next room's block 30 m off; one outdoors.
    let torch = |cell: u32, at: Vec3| {
        let mut l = light(cell, at, LightKind::Point, 8.0);
        l.color = Color::new(1.0, 0.5, 0.0, 1.0);
        l.intensity = 0.5;
        l.most_intensity = Some(0.5);
        l
    };
    let carried = [
        torch(cell, Vec3::new(100.0, 101.0, 0.0)),
        torch(cell, Vec3::new(130.0, 100.0, 0.0)),
        torch(0x0298_0021, Vec3::new(100.0, 99.0, 0.0)),
    ];
    let kept = keep_carried(Some(&lights), &own, &carried, cell, at, 0.5, &night);
    // Weights: the near block light (10 - 2) / 10 = 0.8; the torch (8 - 1) / 8 * 0.5 * 0.5 =
    // 0.219; the dim one (10 - 4) / 10 * 0.2 = 0.12.
    assert_eq!(kept, [Kept::Block(1), Kept::Carried(0), Kept::Block(0)]);
    assert_eq!(
        keep_carried(Some(&lights), &own, &carried[1..], cell, at, 0.5, &night),
        [Kept::Block(1), Kept::Block(0)],
        "none in reach: its own, in their order"
    );
    let device = device_lights_kept(Some(&lights), &kept, &carried, cell, &night, 4);
    assert_eq!(device.len(), 3);
    assert_eq!(device[1].origin, Vec3::new(100.0, 101.0, 0.0));
    assert_eq!(device[1].diffuse, Color::new(0.5, 0.25, 0.0, 1.0));
    assert_eq!(device[1].range, 8.0);
    assert_eq!(
        device_lights_kept(Some(&lights), &kept, &carried, cell, &night, 2).len(),
        2,
        "as many as the program takes"
    );
    // A torch carried in the block west, by the edge, lights a draw by this block's west edge.
    let west = [torch(0x0198_0120, Vec3::new(158.0, 50.0, 0.0))];
    let edge = Vec3::new(1.0, 50.0, 0.0);
    let k = keep_carried(None, &[], &west, cell, edge, 0.5, &night);
    assert_eq!(k, [Kept::Carried(0)]);
    let d = device_lights_kept(None, &k, &west, cell, &night, 4);
    assert_eq!(
        d[0].origin,
        Vec3::new(-2.0, 50.0, 0.0),
        "in the draw's block"
    );
    let p = particle_lights_kept(None, &k, &west, cell);
    assert_eq!(p[0].origin, Vec3::new(-2.0, 50.0, 0.0));
    assert_eq!(p[0].color, Color::new(0.5, 0.25, 0.0, 1.0));
    assert_eq!(p[0].range, Some(8.0));
}

/// A light's packed list of lit vertices reads as the run-length form: single indices and runs in
/// order, and a run whose length has its top bit set (not above 0) gives nothing.
///
/// Behaviour: world.inside.a-lights-packed-list-reads-as-the-run-length-form
#[test]
fn a_lights_packed_list_reads_as_the_run_length_form() {
    let packed =
        |words: &[u16]| -> Vec<u8> { words.iter().flat_map(|w| w.to_le_bytes()).collect() };
    assert_eq!(
        auberean_world::inside::unpack_indices(&packed(&[7, 0x8002, 3, 40])),
        [7, 2, 3, 4, 40]
    );
    assert_eq!(
        auberean_world::inside::unpack_indices(&packed(&[0x8005, 0x8000, 9])),
        [9]
    );
}
