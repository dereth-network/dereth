//! How far the landscape is drawn, from the graphics preferences.

use auberean_draw::quality::{key, Graphics, HIGHEST};
use auberean_scene::Landscape;

/// The landscape's draw distance sets the far landscape's reach in blocks (very low to very high:
/// none, 2, 4, 6, 8); the frill distance where frills are drawn: none when disabled, the cells next
/// to the viewer's at low and medium (low fading them out at 0.65 of that distance), two cells about
/// it at high. The highest overall quality (frill distance medium) draws 8 blocks out and frills
/// in the cells next to the viewer's.
///
/// Behaviour: desktop.world.the-landscape-preferences-set-the-far-landscape-and-the-frills
#[test]
fn the_landscape_preferences_set_the_far_landscape_and_the_frills() {
    let with = |draw: f32, frill: f32| {
        let mut g = Graphics::default();
        assert!(g.set(key::LANDSCAPE_DRAW_DISTANCE, draw));
        assert!(g.set(key::LANDSCAPE_FRILL_DISTANCE, frill));
        Landscape::of(&g)
    };
    let far: Vec<i32> = (0..5).map(|d| with(d as f32, 2.0).far_radius).collect();
    assert_eq!(far, [0, 2, 4, 6, 8]);
    let frills: Vec<(i32, f32)> = (0..4)
        .map(|f| {
            let l = with(4.0, f as f32);
            (l.frill_radius, l.frill_reach)
        })
        .collect();
    assert_eq!(frills, [(0, 1.0), (1, 0.65), (1, 1.0), (2, 1.0)]);
    let mut highest = Graphics::default();
    highest.set_overall(HIGHEST);
    assert_eq!(
        Landscape::of(&highest),
        Landscape {
            far_radius: 8,
            frill_radius: 1,
            frill_reach: 1.0,
            shore_effects: 2,
        }
    );
    assert_eq!(
        Landscape::of(&Graphics::default()).far_radius,
        4,
        "the draw distance's own default, medium"
    );
}

/// The landscape's shore effects preference says what the shores draw: nothing when disabled,
/// their strips in the frame's alpha at medium (its own default), those and their foam at high
/// (the highest overall quality's).
///
/// Behaviour: desktop.world.the-shore-effects-preference-sets-what-the-shores-draw
#[test]
fn the_shore_effects_preference_sets_what_the_shores_draw() {
    let with = |v: f32| {
        let mut g = Graphics::default();
        assert!(g.set(key::LANDSCAPE_SHORE_EFFECTS, v));
        Landscape::of(&g).shore_effects
    };
    assert_eq!([0.0, 1.0, 2.0].map(with), [0, 1, 2]);
    assert_eq!(Landscape::of(&Graphics::default()).shore_effects, 1);
    let mut highest = Graphics::default();
    highest.set_overall(HIGHEST);
    assert_eq!(Landscape::of(&highest).shore_effects, 2);
}
