//! The lights effects place on objects.

use auberean_assets::property::PropertyCollection;
use auberean_draw::effects::CarriedLight;
use auberean_primitives::Vec3;
use auberean_world::inside::{Light, LightKind};

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// A light an effect places shows whole while it lives as long as its object or fades in no
/// time; else it fades in over its fade time and out over its last, held to its object's
/// opacity. One of no light properties is a white point light of range 5 at intensity one half.
///
/// Behaviour: draw.effects.a-placed-light-fades-in-and-out
#[test]
fn a_placed_light_fades_in_and_out() {
    let (light, range, intensity) =
        Light::of_properties(0, Vec3::ZERO, &PropertyCollection::default());
    assert_eq!(light.kind, LightKind::Point);
    assert!(near(range.base, 5.0) && near(intensity.base, 0.5));
    let carried = |lifespan: f32, fade: f32| CarriedLight {
        light: light.clone(),
        range,
        intensity,
        offset: Vec3::ZERO,
        lifespan,
        fade,
        created: 100.0,
    };
    assert!(
        near(carried(0.0, 0.25).opacity(100.0, 1.0), 1.0),
        "as long as its object"
    );
    assert!(near(carried(2.0, 0.0).opacity(100.0, 0.8), 0.8), "no fade");
    let l = carried(2.0, 0.5);
    assert!(near(l.opacity(100.0, 1.0), 0.0));
    assert!(near(l.opacity(100.25, 1.0), 0.5), "half faded in");
    assert!(near(l.opacity(101.0, 1.0), 1.0));
    assert!(near(l.opacity(101.75, 1.0), 0.5), "half faded out");
    assert!(
        near(l.opacity(100.25, 0.4), 0.2),
        "times its object's opacity"
    );
    // a fade longer than half its life: no fading
    assert!(near(carried(0.4, 0.3).opacity(100.0, 1.0), 1.0));
}

/// A placed light as the frame's lights take it: a point light in its object's cell, at its place
/// in that cell's own block (from the block whose frame it was placed in), its range as it stands
/// and its intensity and specular color times how much of it shows; how far and how strongly it may
/// light anything from its waveforms' bounds, the strength times how much shows too.
///
/// Behaviour: draw.effects.a-placed-light-stands-in-its-own-cells-block
#[test]
fn a_placed_light_stands_in_its_own_cells_block() {
    use auberean_draw::effects::PlacedLight;
    use auberean_world::color::Color;
    let (mut light, mut range, mut intensity) =
        Light::of_properties(0, Vec3::ZERO, &PropertyCollection::default());
    light.color = Color::new(1.0, 0.5, 0.0, 1.0);
    light.specular = Color::new(1.0, 1.0, 1.0, 1.0);
    range.base = 8.0;
    intensity.base = 0.8;
    let placed = PlacedLight {
        origin: Vec3::new(-10.0, -5.0, 3.0),
        cell: 0x6EA8_0001,
        opacity: 0.5,
        light: CarriedLight {
            light,
            range,
            intensity,
            offset: Vec3::ZERO,
            lifespan: 0.0,
            fade: 0.0,
            created: 0.0,
        },
    };
    let dice = auberean_primitives::random::Random::new();
    let l = placed.standing((0x6F, 0xA9), 160.0, 5.0, &dice);
    assert_eq!(l.kind, LightKind::Point);
    assert_eq!(l.cell, 0x6EA8_0001);
    assert_eq!(
        l.origin,
        Vec3::new(150.0, 155.0, 3.0),
        "in the block south-west"
    );
    assert!(near(l.range, 8.0) && near(l.most_range, 8.0));
    assert!(near(l.intensity, 0.4), "{}", l.intensity);
    assert_eq!(l.most_intensity, Some(0.4));
    assert!(near(l.specular.r, 0.5) && near(l.color.g, 0.5));
    assert_eq!(l.flicker, None);
}
