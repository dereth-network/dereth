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
