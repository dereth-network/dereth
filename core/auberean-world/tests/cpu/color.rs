//! Colors: packed words, hue, saturation and brightness.

use auberean_world::color::{lerp_angle_closest, Color};

/// A packed color word is alpha in the top byte, then red, green and blue.
///
/// Behaviour: world.color.a-packed-color-is-alpha-red-green-blue
#[test]
fn a_packed_color_is_alpha_red_green_blue() {
    let c = Color::from_argb(0x80FF_4000);
    assert!((c.a - 128.0 / 255.0).abs() < 1e-6);
    assert!((c.r - 1.0).abs() < 1e-6);
    assert!((c.g - 64.0 / 255.0).abs() < 1e-6);
    assert_eq!(c.b, 0.0);
    assert_eq!(Color::from_argb(0x80FF_4000).to_argb(), 0x80FF_4000);
}

/// Hue runs in sextants from red (0) through yellow (1), green (2), cyan (3), blue (4) and magenta
/// (5); saturation is one less the smallest channel over the largest, brightness the largest
/// channel; a gray has no hue or saturation. Converting back gives the color again.
///
/// Behaviour: world.color.hue-saturation-and-brightness-round-trip
#[test]
fn hue_saturation_and_brightness_round_trip() {
    let cases = [
        (Color::new(1.0, 0.0, 0.0, 1.0), (0.0, 1.0, 1.0)),
        (Color::new(1.0, 1.0, 0.0, 1.0), (1.0, 1.0, 1.0)),
        (Color::new(0.0, 0.5, 0.0, 1.0), (2.0, 1.0, 0.5)),
        (Color::new(0.0, 1.0, 1.0, 1.0), (3.0, 1.0, 1.0)),
        (Color::new(0.0, 0.0, 0.8, 1.0), (4.0, 1.0, 0.8)),
        (Color::new(1.0, 0.0, 1.0, 1.0), (5.0, 1.0, 1.0)),
        (Color::new(0.4, 0.4, 0.4, 1.0), (0.0, 0.0, 0.4)),
        (Color::new(0.6, 0.3, 0.15, 1.0), (1.0 / 3.0, 0.75, 0.6)),
    ];
    for (c, (h, s, v)) in cases {
        let (h2, s2, v2) = c.to_hsb();
        assert!(
            (h - h2).abs() < 1e-5 && (s - s2).abs() < 1e-5 && (v - v2).abs() < 1e-5,
            "{c:?}: {h2} {s2} {v2}"
        );
        let back = Color::from_hsb(h2, s2, v2);
        assert!(
            (back.r - c.r).abs() < 1e-5
                && (back.g - c.g).abs() < 1e-5
                && (back.b - c.b).abs() < 1e-5,
            "{c:?} -> {back:?}"
        );
    }
    // No saturation or no brightness is a gray of the brightness, clamped.
    assert_eq!(
        Color::from_hsb(2.5, 0.0, 0.3),
        Color::new(0.3, 0.3, 0.3, 1.0)
    );
    assert_eq!(
        Color::from_hsb(2.5, 0.5, 0.0),
        Color::new(0.0, 0.0, 0.0, 1.0)
    );
}

/// An angle interpolates the short way round: from 350 to 10 degrees passes through 0.
///
/// Behaviour: world.color.angles-interpolate-the-short-way-round
#[test]
fn angles_interpolate_the_short_way_round() {
    assert!((lerp_angle_closest(350.0, 10.0, 0.5, 360.0) - 0.0).abs() < 1e-4);
    assert!((lerp_angle_closest(10.0, 350.0, 0.25, 360.0) - 5.0).abs() < 1e-4);
    assert!((lerp_angle_closest(90.0, 180.0, 0.5, 360.0) - 135.0).abs() < 1e-4);
    assert!((lerp_angle_closest(5.5, 0.5, 0.5, 6.0) - 0.0).abs() < 1e-4);
}
