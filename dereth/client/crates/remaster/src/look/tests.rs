//! Behaviour: none (experimental)

use glam::Vec3;

use super::*;
use crate::render::light::{frame_rows, LightMode};
use crate::scene::{Fog, FrameDelta, Light, LightKind};

/// Retail's light at dusk: a low orange sun, the purple ambient and fog of a Sunny evening, and a
/// lamp.
fn dusk() -> RetailLight {
    RetailLight {
        sun: Vec3::new(0.31, -0.42, 0.08),
        sun_color: [255, 170, 120],
        ambient_level: 0.38,
        ambient_color: [150, 110, 190],
        object_ambient: [0.27, 0.2, 0.34],
        fog: Some(Fog {
            start: 120.0,
            end: 480.0,
            color: [110, 80, 140],
        }),
        outdoor: true,
        sees_outside: true,
        time_of_day: 0.72,
        lights: vec![Light {
            kind: LightKind::Point,
            cell: 0xA9B4_0001,
            position: Vec3::new(10.0, 20.0, 3.0),
            direction: Vec3::ZERO,
            color: [1.0, 0.8, 0.6],
            intensity: 100.0,
            falloff: 8.0,
            cone_angle: 0.0,
        }],
        statics: 1,
    }
}

fn saturation(c: [f32; 3]) -> f32 {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    if max <= 0.0 {
        0.0
    } else {
        (max - min) / max
    }
}

fn bytes(c: [u8; 3]) -> [f32; 3] {
    c.map(f32::from)
}

/// **The identity look gives retail's light back bit for bit**, so the frame's constants the
/// shaders read are the same bytes: the empty stack, a layer that sets nothing, and a file that
/// sets every value to what retail has.
#[test]
fn the_identity_look_gives_retails_light_back_bit_for_bit() {
    let l = dusk();
    let mut stack = LookStack::new();
    assert_eq!(stack.look(None, 0.72), Look::IDENTITY);
    assert!(stack.is_empty());
    let neutral = LookToml::parse(
        "sun = 1\nsky = 1.0\ntint = 1\nfog_density = 1\nnight_floor = 0\nexposure = 0\n",
    )
    .unwrap();
    stack.set(Layer::Player, Some(neutral));
    assert!(!stack.is_empty(), "it says something, the identity");
    let look = stack.look(Some(DayKind::Sunny), 0.72);
    assert_eq!(look, Look::IDENTITY);
    let applied = look.apply(&l);
    assert_eq!(applied, l);
    let mut frame = FrameDelta {
        light: l.clone(),
        ..FrameDelta::default()
    };
    let before = frame_rows(&frame, 0.0, LightMode::Pixel, (960, 540)).bytes();
    frame.light = applied;
    let after = frame_rows(&frame, 0.0, LightMode::Pixel, (960, 540)).bytes();
    assert_eq!(before, after);
    // Exposure is the high dynamic range pass's alone: it leaves retail's light as it is.
    let exposed = Look {
        exposure: 1.5,
        ..Look::IDENTITY
    };
    assert!(exposed.leaves_light_alone());
    assert_eq!(exposed.apply(&l), l);
}

/// **A tint of 0 takes the evening's purple to gray of the same brightness, a tint of 1 keeps
/// it, and halfway is halfway**: the ambient's color, the objects' ambient and the fog's color,
/// and nothing else.
#[test]
fn a_tint_of_zero_grays_the_evening_and_one_keeps_retails_purple() {
    let l = dusk();
    let gray = Look {
        tint: 0.0,
        ..Look::IDENTITY
    }
    .apply(&l);
    let luma = |c: [f32; 3]| c[0] * 0.299 + c[1] * 0.587 + c[2] * 0.114;
    for c in [gray.ambient_color, gray.fog.unwrap().color] {
        assert!(saturation(bytes(c)) < 0.01, "{c:?} is gray");
    }
    assert!((luma(bytes(gray.ambient_color)) - luma(bytes(l.ambient_color))).abs() <= 0.5);
    assert!(saturation(gray.object_ambient) < 1e-5);
    assert!((luma(gray.object_ambient) - luma(l.object_ambient)).abs() < 1e-5);
    assert_eq!(
        (gray.sun, gray.sun_color, gray.ambient_level),
        (l.sun, l.sun_color, l.ambient_level),
        "the sun and the ambient's level are the tint's to leave"
    );
    assert_eq!(gray.lights, l.lights);
    let half = Look {
        tint: 0.5,
        ..Look::IDENTITY
    }
    .apply(&l);
    let s = |c: [u8; 3]| saturation(bytes(c));
    assert!(s(half.ambient_color) > s(gray.ambient_color));
    assert!(s(half.ambient_color) < s(l.ambient_color));
    assert!((s(half.ambient_color) - s(l.ambient_color) / 2.0).abs() < 0.05);
}

/// **The sun's and the sky's strengths, the fog's density and color and the night floor each do
/// what they say**: the sun's brightness is its vector's length, the sky's is both ambients, a
/// thicker fog starts and ends nearer, and the floor holds the ambient up.
#[test]
fn the_strengths_the_fog_and_the_floor_each_change_their_own_input() {
    let l = dusk();
    let look = Look {
        sun: 2.0,
        sky: 1.5,
        fog_density: 2.0,
        fog_color: Some([10, 20, 30]),
        ..Look::IDENTITY
    };
    let a = look.apply(&l);
    assert!((a.sun.length() - 2.0 * l.sun.length()).abs() < 1e-6);
    assert!((a.sun.normalize() - l.sun.normalize()).length() < 1e-6);
    assert!((a.ambient_level - 1.5 * l.ambient_level).abs() < 1e-6);
    for k in 0..3 {
        assert!((a.object_ambient[k] - 1.5 * l.object_ambient[k]).abs() < 1e-6);
    }
    let (f, g) = (a.fog.unwrap(), l.fog.unwrap());
    assert_eq!(
        (f.start, f.end, f.color),
        (g.start / 2.0, g.end / 2.0, [10, 20, 30])
    );
    let night = RetailLight {
        ambient_level: 0.2,
        ambient_color: [255, 255, 255],
        object_ambient: [0.1, 0.1, 0.1],
        ..dusk()
    };
    let lit = Look {
        night_floor: 0.35,
        ..Look::IDENTITY
    }
    .apply(&night);
    assert!((lit.ambient_level - 0.35).abs() < 1e-6);
    assert!(lit.object_ambient.iter().all(|o| (o - 0.35).abs() < 1e-6));
    let day = Look {
        night_floor: 0.35,
        ..Look::IDENTITY
    }
    .apply(&RetailLight {
        ambient_level: 0.6,
        ..night.clone()
    });
    assert!(
        (day.ambient_level - 0.6).abs() < 1e-6,
        "a floor never lowers"
    );
}

/// **The layers lay in order, the later winning value by value**: the built-in identity, then an
/// art pack's, a world pack's and the player's; a layer that says nothing of a value leaves it
/// below; and a layer's table for a kind of day wins over its own values on those days.
#[test]
fn the_later_layer_wins_value_by_value_and_a_kind_of_day_over_its_layer() {
    let mut stack = LookStack::new();
    stack.set(
        Layer::ArtPack,
        Some(LookToml::parse("sun = 1.2\ntint = 0.8\n[rainy]\nsky = 0.7\n").unwrap()),
    );
    stack.set(
        Layer::WorldPack,
        Some(LookToml::parse("tint = 0.6\nfog_density = 1.5\n").unwrap()),
    );
    stack.set(
        Layer::Player,
        Some(LookToml::parse("sky = 1.1\n[rainy]\nfog_density = 3\n").unwrap()),
    );
    let sunny = stack.look(Some(DayKind::Sunny), 0.5);
    assert_eq!(
        (sunny.sun, sunny.tint, sunny.fog_density, sunny.sky),
        (1.2, 0.6, 1.5, 1.1)
    );
    let rainy = stack.look(Some(DayKind::Rainy), 0.5);
    assert_eq!(
        (rainy.sky, rainy.fog_density),
        (1.1, 3.0),
        "the player's sky over the pack's rainy one; the player's rainy fog over its own"
    );
    assert!(
        stack.set(Layer::Player, None),
        "taking a layer away changes the look"
    );
    assert!(!stack.set(Layer::Player, None));
    assert_eq!(stack.look(Some(DayKind::Rainy), 0.5).sky, 0.7);
}

/// **A curve joins its points with straight lines and runs round midnight**; one number is the
/// same all day.
#[test]
fn a_curve_joins_its_points_round_the_day() {
    let c = Curve::Points(vec![(0.25, 1.0), (0.75, 0.0)]);
    assert!((c.at(0.5) - 0.5).abs() < 1e-6);
    assert!((c.at(0.25) - 1.0).abs() < 1e-6);
    assert!((c.at(0.75) - 0.0).abs() < 1e-6);
    // From 0.75 (0) round midnight to 0.25 (1): halfway is 0.0, a day apart.
    assert!((c.at(0.0) - 0.5).abs() < 1e-6);
    assert!((c.at(1.0) - 0.5).abs() < 1e-6);
    assert!((c.at(0.875) - 0.25).abs() < 1e-6);
    assert!((c.at(0.125) - 0.75).abs() < 1e-6);
    assert_eq!(Curve::Points(vec![(0.3, 0.4)]).at(0.9), 0.4);
    assert_eq!(Curve::Constant(0.7).at(0.1), 0.7);
    let parsed = LookToml::parse("tint = [[0.75, 0.0], [0.25, 1.0]]\n").unwrap();
    assert_eq!(parsed.values.tint, Some(c), "points are read in time order");
}

/// **A look file says what it does not understand**: an unknown value or day, a value out of its
/// range, a malformed point or color, and text that is not TOML are each refused with a line
/// naming them.
#[test]
fn a_look_file_refuses_what_it_does_not_understand() {
    assert!(LookToml::parse("").unwrap() == LookToml::default());
    for (text, says) in [
        ("brightness = 2\n", "brightness"),
        ("[stormy]\nsun = 2\n", "stormy"),
        ("[rainy]\n[rainy.sunny]\nsun = 1\n", "rainy.sunny"),
        ("tint = 2\n", "tint = 2"),
        ("sun = -1\n", "sun = -1"),
        ("tint = [[1.5, 0.5]]\n", "time 1.5"),
        ("tint = [[0.5]]\n", "two numbers"),
        ("tint = []\n", "no points"),
        ("fog_color = [1, 2]\n", "fog_color"),
        ("fog_color = [1, 2, 300]\n", "fog_color"),
        ("[rainy]\nfog_density = 0\n", "rainy.fog_density"),
        ("sun = \n", "not TOML"),
    ] {
        let e = LookToml::parse(text).unwrap_err();
        assert!(e.0.contains(says), "{text:?}: {e}");
    }
}

/// **A day group's name says its kind**, whatever its case and whatever follows.
#[test]
fn a_day_groups_name_says_its_kind() {
    assert_eq!(DayKind::of_group("Sunny"), Some(DayKind::Sunny));
    assert_eq!(DayKind::of_group("rainy 3"), Some(DayKind::Rainy));
    assert_eq!(DayKind::of_group(" Cloudy Day"), Some(DayKind::Cloudy));
    assert_eq!(DayKind::of_group("Clear"), Some(DayKind::Clear));
    assert_eq!(DayKind::of_group("Stormy"), None);
}
