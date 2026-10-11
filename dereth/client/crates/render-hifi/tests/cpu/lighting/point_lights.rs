//! Behaviour: none (experimental Horizon rendering effects: which point lights the relight walks,
//! which changes the work done and no pixel)

use dereth_render_hifi::passes::lighting::{point_lights, MAX_LIGHTS};
use dereth_render_hifi::snapshot::HifiLight;
use dereth_render_hifi::HifiFrame;
use glam::Vec3;

fn light(x: f32, interior: bool) -> HifiLight {
    HifiLight {
        position: Vec3::new(x, 0.0, 0.0),
        color: [1.0, 0.8, 0.6],
        intensity: 1.0,
        falloff: 5.0,
        interior,
    }
}

/// A frame whose eye is at the origin, with `lights`.
fn frame(lights: Vec<HifiLight>) -> HifiFrame {
    let mut f = HifiFrame::default();
    f.camera.eye = Vec3::ZERO;
    f.lights = lights;
    f
}

#[test]
fn with_the_sun_high_the_point_lights_have_no_gain_and_none_is_walked() {
    let f = frame(vec![light(3.0, true), light(4.0, false)]);
    assert!(point_lights(&f, 0.0).is_empty());
    assert_eq!(point_lights(&f, 0.35).len(), 2);
}

#[test]
fn the_lights_inside_come_first_then_the_rest_each_kind_nearest_first() {
    let f = frame(vec![
        light(9.0, false),
        light(7.0, true),
        light(2.0, false),
        light(5.0, true),
        light(400.0, true),
    ]);
    let order: Vec<(f32, bool)> = point_lights(&f, 1.0)
        .iter()
        .map(|l| (l.position.x, l.interior))
        .collect();
    // The light past 250 metres is not taken.
    assert_eq!(
        order,
        vec![(5.0, true), (7.0, true), (2.0, false), (9.0, false)]
    );
}

#[test]
fn only_the_nearest_lights_are_taken_before_they_are_sorted_by_kind() {
    // The nearest are outdoors; a full list of them leaves out every farther light inside.
    let mut lights: Vec<HifiLight> = (0..MAX_LIGHTS)
        .map(|i| light(1.0 + i as f32 * 0.5, false))
        .collect();
    lights.push(light(200.0, true));
    let f = frame(lights);
    let taken = point_lights(&f, 1.0);
    assert_eq!(taken.len(), MAX_LIGHTS);
    assert!(taken.iter().all(|l| !l.interior));
}
