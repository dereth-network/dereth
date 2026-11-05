//! The graphics preferences and the overall graphics quality.

use auberean_draw::quality::{
    brightness_ramp, key, Graphics, Kind, GRAPHICS, HIGHEST, PREFERENCES,
};

/// Each of the five overall qualities sets every graphics preference but the screen's brightness,
/// values and defaults, and the preferences then match it; the first of the five that every
/// preference but the adaptive degrade's bias matches is the quality (0 when none): the defaults
/// match none, the bias alone changes nothing, any other preference makes it the player's own.
///
/// Behaviour: draw.quality.an-overall-quality-sets-the-graphics-preferences
#[test]
fn an_overall_quality_sets_the_graphics_preferences() {
    assert_eq!(GRAPHICS, 0x4000_0004);
    assert_eq!(PREFERENCES.len(), 21);
    assert_eq!(
        Graphics::default().overall(),
        0,
        "the defaults are no quality"
    );
    for q in 1..=HIGHEST {
        let mut g = Graphics::default();
        g.set(key::SCREEN_BRIGHTNESS, 0.45);
        g.set_overall(q);
        assert_eq!(g.overall(), q);
        assert_eq!(
            g.get(key::SCREEN_BRIGHTNESS),
            Some(0.45),
            "brightness untouched"
        );
        for p in PREFERENCES
            .iter()
            .filter(|p| p.key != key::SCREEN_BRIGHTNESS)
        {
            assert_eq!(
                g.get(p.key),
                g.default_of(p.key),
                "{:08X} default with it",
                p.key
            );
        }
        g.set(key::ADAPTIVE_DEGRADE_BIAS, 0.25);
        assert_eq!(g.overall(), q, "the bias is not compared");
        g.set(key::LIGHT_CORONAS, if q >= 4 { 0.0 } else { 1.0 });
        assert_eq!(g.overall(), 0, "a quality of the player's own");
    }
    // Spot values: the lowest texture detail any quality takes is medium; the cloud shadows come
    // only with the highest; the adaptive degrade goes off at the two highest.
    let at = |q: u32, k: u32| {
        let mut g = Graphics::default();
        g.set_overall(q);
        g.get(k).unwrap_or(-1.0)
    };
    let row = |k: u32| (1..=HIGHEST).map(|q| at(q, k)).collect::<Vec<_>>();
    assert_eq!(row(key::TEXTURE_DETAIL), [2.0, 2.0, 3.0, 3.0, 4.0]);
    assert_eq!(row(key::LANDSCAPE_DRAW_DISTANCE), [0.0, 1.0, 2.0, 3.0, 4.0]);
    assert_eq!(row(key::LANDSCAPE_CLOUD_SHADOWS), [0.0, 0.0, 0.0, 0.0, 1.0]);
    assert_eq!(row(key::DYNAMIC_OBJECT_SHADOWS), [0.0, 0.0, 0.0, 2.0, 2.0]);
    assert_eq!(row(key::ADAPTIVE_DEGRADE), [1.0, 1.0, 1.0, 0.0, 0.0]);
    assert_eq!(row(key::ADAPTIVE_DEGRADE_BIAS), [0.5, 0.0, -0.5, 0.0, 0.0]);
    assert_eq!(row(key::ANISOTROPIC_FILTERING), [0.0; 5]);
    // A choice that is not one of its choices is refused; other qualities change nothing.
    let mut g = Graphics::default();
    assert!(!g.set(key::TEXTURE_DETAIL, 7.0));
    assert!(g.set(key::TEXTURE_DETAIL, 4.0));
    let before = g.clone();
    g.set_overall(0);
    g.set_overall(6);
    assert_eq!(g, before);
    // The defaults as the client sets its preferences up.
    let d = Graphics::default();
    assert_eq!(d.get(key::TEXTURE_DETAIL), Some(2.0));
    assert_eq!(d.get(key::MATERIAL_DETAIL), Some(1.0));
    assert_eq!(d.get(key::TRILINEAR_FILTERING), Some(1.0));
    assert!(matches!(
        PREFERENCES.iter().find(|p| p.key == key::SCREEN_BRIGHTNESS).map(|p| p.kind),
        Some(Kind::Number { least, greatest, .. }) if (least, greatest) == (-0.2, 1.0)
    ));
}

/// The water's reflection ripples only with the water reflections at their highest choice (High);
/// at Low it is laid as it stands, and with none there is no reflection to ripple.
///
/// Behaviour: draw.quality.the-reflection-ripples-only-at-the-highest-water-reflections
#[test]
fn the_reflection_ripples_only_at_the_highest_water_reflections() {
    let mut g = Graphics::default();
    let mut at = |choice: f32| {
        assert!(g.set(key::WATER_REFLECTIONS, choice));
        g.water_ripples()
    };
    assert_eq!([at(0.0), at(1.0), at(2.0)], [false, false, true]);
}

/// The screen's brightness makes each color's level 255 times itself plus twice the brightness of
/// that, held to 16 bits, the brightness itself held to -0.2 to 1.
///
/// Behaviour: draw.quality.the-screen-brightness-sets-each-levels-ramp
#[test]
fn the_screen_brightness_sets_each_levels_ramp() {
    let none = brightness_ramp(0.0);
    assert_eq!(
        (none[0], none[1], none[128], none[255]),
        (0, 255, 32640, 65025)
    );
    let most = brightness_ramp(1.0);
    assert_eq!(
        (most[1], most[85], most[86], most[255]),
        (765, 65025, 65535, 65535)
    );
    let least = brightness_ramp(-0.2);
    assert_eq!((least[1], least[255]), (153, 39015));
    assert_eq!(brightness_ramp(5.0), most, "held to 1");
    assert_eq!(brightness_ramp(-5.0), least, "held to -0.2");
}
