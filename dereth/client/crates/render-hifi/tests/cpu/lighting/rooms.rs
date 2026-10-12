//! Behaviour: none (experimental Horizon rendering effects: which of the rooms' lights a pixel
//! walks, how each lights a room's surface per pixel, and the burned light a room's surface was
//! drawn with carried to the relight in its class's code)

use dereth_render_hifi::derive::reshade::{baked_code, baked_light, MaterialClass, BAKED_CODES};
use dereth_render_hifi::passes::lighting::cells::{
    burned_lights, reach, room_window, shader, MAX_CELL_LIGHTS, RANGE_OVER_FALLOFF,
};
use dereth_render_hifi::snapshot::HifiLight;
use dereth_render_hifi::HifiFrame;
use glam::Vec3;

use crate::common::wgsl_eval::{Evaluator, Value};

fn light(x: f32, burned: bool) -> HifiLight {
    HifiLight {
        position: Vec3::new(x, 0.0, 0.0),
        color: [1.0, 0.6, 0.3],
        intensity: 100.0,
        falloff: 5.0,
        interior: true,
        burned,
    }
}

fn frame(lights: Vec<HifiLight>) -> HifiFrame {
    let mut f = HifiFrame::default();
    f.camera.eye = Vec3::ZERO;
    f.camera.far = 500.0;
    f.lights = lights;
    f
}

#[test]
fn only_the_lights_burned_into_the_rooms_are_walked_nearest_first() {
    let mut dark = light(2.0, true);
    dark.falloff = 0.0;
    let f = frame(vec![
        light(9.0, true),
        light(1.0, false),
        light(4.0, true),
        dark,
        light(700.0, true),
    ]);
    let walked: Vec<f32> = burned_lights(&f).iter().map(|l| l.position.x).collect();
    // The light that follows the player is not burned into the rooms, a light with no reach
    // lights nothing, and a light past the far plane and its reach is not in view.
    assert_eq!(walked, [4.0, 9.0]);

    let many: Vec<HifiLight> = (0..MAX_CELL_LIGHTS + 20)
        .map(|i| light(300.0 - i as f32, true))
        .collect();
    let f = frame(many);
    let walked = burned_lights(&f);
    assert_eq!(walked.len(), MAX_CELL_LIGHTS);
    // The nearest are kept.
    assert!(walked.iter().all(|l| l.position.x <= 300.0 - 20.0));
}

#[test]
fn a_lights_reach_is_its_falloff_and_nearly_a_third_again_as_the_rooms_are_burned() {
    assert_eq!(RANGE_OVER_FALLOFF, 1.3);
    assert_eq!(reach(&light(0.0, true)), 6.5);
}

fn direct(eval: &Evaluator, p: Vec3, n: Vec3, at: Vec3, range: f32) -> f32 {
    let v = |v: Vec3| Value::vector(&[v.x, v.y, v.z]);
    let c = eval.call(
        "room_direct",
        &[
            v(p),
            v(n),
            v(at),
            Value::F32(range),
            Value::vector(&[1.0, 1.0, 1.0]),
        ],
    );
    c.floats()[1]
}

fn scattered(eval: &Evaluator, p: Vec3, at: Vec3, range: f32) -> f32 {
    let v = |v: Vec3| Value::vector(&[v.x, v.y, v.z]);
    eval.call(
        "room_scattered",
        &[
            v(p),
            v(at),
            Value::F32(range),
            Value::vector(&[1.0, 1.0, 1.0]),
        ],
    )
    .floats()[1]
}

#[test]
fn a_rooms_light_is_brightest_near_it_and_falls_off_to_nothing_at_its_reach() {
    let eval = Evaluator::new(&shader(false));
    let up = Vec3::Z;
    let at = Vec3::new(0.0, 0.0, 2.0);
    let range = 6.5;
    // Straight below the light, farther and farther off along the floor.
    let along: Vec<f32> = [0.0f32, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
        .iter()
        .map(|x| direct(&eval, Vec3::new(*x, 0.0, 0.0), up, at, range))
        .collect();
    for w in along.windows(2) {
        assert!(w[1] < w[0], "{along:?}");
    }
    assert!(along[0] > 1.0, "right under the light: {along:?}");
    assert!(along[6] < 0.01, "at the edge of its reach: {along:?}");
    assert_eq!(direct(&eval, Vec3::new(7.0, 0.0, 0.0), up, at, range), 0.0);
    // A face turned from the light, past the soft wrap, takes none of its direct light, but some
    // of what the room scatters.
    let away = Vec3::new(0.0, 0.0, -1.0);
    assert_eq!(direct(&eval, Vec3::ZERO, away, at, range), 0.0);
    let s = scattered(&eval, Vec3::ZERO, at, range);
    assert!(s > 0.0 && s < along[0] * 0.25, "{s} against {}", along[0]);
    assert_eq!(scattered(&eval, Vec3::new(7.0, 0.0, 0.0), at, range), 0.0);
    // The falloff's window is the one the code names.
    for x in [0.0f32, 0.3, 0.6, 0.9, 1.0, 1.2] {
        let w = eval.call("room_window", &[Value::F32(x)]).f32();
        assert!((w - room_window(x)).abs() < 1e-6, "{x}: {w}");
    }
}

#[test]
fn the_burned_light_a_room_surface_was_drawn_with_comes_back_from_its_code() {
    let relight = dereth_render_hifi::passes::lighting::shaders()
        .into_iter()
        .find(|(n, _)| *n == "relight")
        .expect("the relight shader")
        .1;
    let eval = Evaluator::new(&relight);
    let (lo, hi) = BAKED_CODES;
    for l in [0.004f32, 0.01, 0.05, 0.1, 0.3, 0.5, 0.8, 1.0] {
        let code = baked_code(l);
        // A room's surface still reads as one, and never as one whose light is not known.
        assert!(code >= lo && code <= hi, "{l}: {code}");
        assert!(code > 1.5 && code < 2.5 && code != MaterialClass::Baked.encoded());
        let back = baked_light(code).expect("a room's code");
        assert!((back - l).abs() / l < 0.01, "{l} came back as {back}");
        let drawn = eval.call("burned_drawn", &[Value::F32(code)]).f32();
        assert!(
            (drawn - l).abs() / l < 0.01,
            "{l} came back in the shader as {drawn}"
        );
    }
    // No light at all is the darkest code; a code that is not a room's says nothing.
    assert_eq!(baked_code(0.0), lo);
    assert_eq!(baked_light(MaterialClass::Baked.encoded()), None);
    for other in [0.0f32, 1.0, 1.25, 2.0, 3.0, 4.2] {
        assert!(
            eval.call("burned_drawn", &[Value::F32(other)]).f32() < 0.0,
            "{other}"
        );
    }
}
