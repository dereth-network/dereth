//! The view behind character select and creation: each view's camera path on a fixed clock,
//! Random's pick on a fixed seed, and the settings' choice of view.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_horizon::ui::settings::Settings;
use auberean_horizon::vista::{
    camera, pick, Choice, Visit, Vista, AT, BLOCK, MIDDLE, PERIOD, PITCH, SWING,
};
use auberean_primitives::random::Random;

use crate::{in_world, Harness};

/// Every frame of two turns at 60 frames a second, from a clock that started long ago.
fn frames() -> impl Iterator<Item = f64> {
    let start = 1_000.0 * PERIOD;
    (0..)
        .map(move |n: u32| start + f64::from(n) / 60.0)
        .take_while(move |t| *t <= start + 2.0 * PERIOD)
}

#[test]
fn the_camera_stands_still_over_cragstone_and_turns_slowly_right_and_back() {
    let mut last: Option<f32> = None;
    let (mut least, mut most) = (f32::MAX, f32::MIN);
    for t in frames() {
        let c = camera(t);
        assert_eq!((c.block, c.position, c.pitch), (BLOCK, AT, PITCH), "{t}");
        assert!(!c.find_ground);
        least = least.min(c.heading);
        most = most.max(c.heading);
        if let Some(h) = last {
            // A fortieth of a degree a frame at the most: slow, and never a jump.
            assert!((c.heading - h).abs() < 0.03, "{t}: {h} to {}", c.heading);
        }
        last = Some(c.heading);
    }
    assert!((least - (MIDDLE - SWING)).abs() < 0.01, "{least}");
    assert!((most - (MIDDLE + SWING)).abs() < 0.01, "{most}");
}

#[test]
fn a_turn_takes_its_period_right_first_then_back_through_the_middle() {
    let at = |share: f64| camera(1_000.0 * PERIOD + share * PERIOD).heading;
    assert!((at(0.0) - MIDDLE).abs() < 1e-3);
    assert!((at(0.25) - (MIDDLE + SWING)).abs() < 1e-3);
    assert!((at(0.5) - MIDDLE).abs() < 1e-3);
    assert!((at(0.75) - (MIDDLE - SWING)).abs() < 1e-3);
    assert!((at(1.0) - MIDDLE).abs() < 1e-3);
    assert!(
        at(0.1) > MIDDLE && at(0.6) < MIDDLE,
        "right first, then left"
    );
}

#[test]
fn every_view_s_camera_stands_still_and_turns_slowly_right_first_then_back() {
    for v in Vista::ALL {
        let spot = v.spot();
        let at = |share: f64| v.camera(1_000.0 * PERIOD + share * PERIOD);
        let mut last: Option<f32> = None;
        for t in frames() {
            let c = v.camera(t);
            assert_eq!(
                (c.block, c.position, c.pitch),
                (spot.block, spot.at, spot.pitch),
                "{v:?} {t}"
            );
            assert!(!c.find_ground);
            if let Some(h) = last {
                assert!(
                    (c.heading - h).abs() < 0.03,
                    "{v:?} {t}: {h} to {}",
                    c.heading
                );
            }
            last = Some(c.heading);
        }
        assert!((at(0.0).heading - spot.middle).abs() < 1e-3, "{v:?}");
        assert!(
            (at(0.25).heading - (spot.middle + spot.swing)).abs() < 1e-3,
            "{v:?}"
        );
        assert!(
            (at(0.75).heading - (spot.middle - spot.swing)).abs() < 1e-3,
            "{v:?}"
        );
        assert!(spot.swing > 10.0, "{v:?} turns");
    }
    let blocks: std::collections::HashSet<(i32, i32)> =
        Vista::ALL.iter().map(|v| v.spot().block).collect();
    assert_eq!(blocks.len(), Vista::ALL.len(), "each view its own place");
}

#[test]
fn the_settings_offer_random_then_every_view_and_random_is_the_default() {
    assert_eq!(Choice::default(), Choice::Random);
    let names: Vec<&str> = Choice::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(
        names,
        [
            "Random",
            "Cragstone",
            "Drudge Citadel",
            "Arramora",
            "Linvak Tukal",
            "Aun Tanua",
            "Ikeras"
        ]
    );
    for c in Choice::ALL {
        assert_eq!(Choice::of_word(c.word()), Some(c), "{c:?}");
    }
    for v in Vista::ALL {
        assert_eq!(Vista::of_word(v.word()), Some(v), "{v:?}");
    }
    assert_eq!(Choice::of_word("nowhere"), None);
}

/// Character select shows a different view each time it comes up, picked with the client's random
/// numbers and never the one shown the time before; the same seed picks the same views.
///
/// Behaviour: desktop.horizon.character-select-shows-a-different-view-each-time-unless-one-is-chosen
#[test]
fn random_picks_another_view_each_time_character_select_comes_up_from_a_fixed_seed() {
    let visits = |random: &Random| -> Vec<Vista> {
        let (mut visit, mut last) = (Visit::default(), None);
        (0..60)
            .map(|_| {
                let (view, picked) = visit.view(Choice::Random, &mut last, random);
                assert!(picked, "picked as character select comes up");
                assert_eq!(last, Some(view), "kept as the last shown");
                // The rest of the visit, creation too, stays on it.
                for _ in 0..3 {
                    assert_eq!(visit.view(Choice::Random, &mut last, random), (view, false));
                }
                visit.leave();
                view
            })
            .collect()
    };
    let shown = visits(&Random::new());
    assert_eq!(
        shown,
        visits(&Random::new()),
        "the same seed, the same views"
    );
    // The client's numbers start the same every run, so its first picks are these.
    assert_eq!(
        shown[..12],
        [
            Vista::LinvakTukal,
            Vista::DrudgeCitadel,
            Vista::Ikeras,
            Vista::DrudgeCitadel,
            Vista::LinvakTukal,
            Vista::DrudgeCitadel,
            Vista::LinvakTukal,
            Vista::DrudgeCitadel,
            Vista::LinvakTukal,
            Vista::Arramora,
            Vista::Ikeras,
            Vista::LinvakTukal
        ],
        "{shown:?}"
    );
    for pair in shown.windows(2) {
        assert_ne!(pair[0], pair[1], "never the same twice running");
    }
    for v in Vista::ALL {
        assert!(shown.contains(&v), "{v:?} comes up in 60 visits");
    }
    // The last view kept from before is not the first one picked.
    let random = Random::new();
    for last in Vista::ALL {
        assert_ne!(pick(Some(last), &random), last);
    }
}

/// A view named in the settings is the one shown every time, and nothing is picked.
///
/// Behaviour: desktop.horizon.character-select-shows-a-different-view-each-time-unless-one-is-chosen
#[test]
fn a_view_chosen_is_shown_every_time_and_a_change_of_choice_shows_at_once() {
    let random = Random::new();
    let (mut visit, mut last) = (Visit::default(), Some(Vista::Ikeras));
    for v in Vista::ALL {
        for _ in 0..3 {
            assert_eq!(visit.view(Choice::One(v), &mut last, &random), (v, false));
            visit.leave();
        }
    }
    assert_eq!(last, Some(Vista::Ikeras), "nothing picked, nothing kept");
    // Random chosen while character select is up keeps the view it shows until the next visit.
    assert_eq!(
        visit.view(Choice::One(Vista::Arramora), &mut last, &random),
        (Vista::Arramora, false)
    );
    assert_eq!(
        visit.view(Choice::Random, &mut last, &random),
        (Vista::Arramora, false)
    );
    visit.leave();
    let (next, picked) = visit.view(Choice::Random, &mut last, &random);
    assert!(picked && next != Vista::Ikeras, "{next:?}");
}

#[test]
fn the_settings_choose_the_view_behind_character_select_from_a_list_and_ask_to_keep_it() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.settings.toggle();
    h.frame(&s);
    let k = h.ui.layout_scale(h.screen);
    let body = h.ui.settings.body().expect("open");
    let c = Settings::controls(body, k);
    let (label, choices) = Settings::vista_rows(&c, body, k);
    // One row, under the window's buttons and inside its body.
    assert!(label.y >= c.layout.bottom() && label.y >= c.log_out.bottom());
    assert!(choices.bottom() <= body.bottom(), "{choices:?} in {body:?}");
    assert!(choices.right() <= body.right(), "{choices:?} in {body:?}");
    assert!((label.y - choices.y).abs() < 0.5 && label.right() <= choices.x);
    assert_eq!(h.ui.settings.own.vista, Choice::Random);
    let centre = |r: auberean_horizon::draw::Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
    // The box opens its list under it, a row a choice; the fourth row is Arramora.
    let row = |n: f32| {
        (
            choices.x + choices.w / 2.0,
            choices.bottom() + 2.0 * k + 3.0 * k + (n + 0.5) * choices.h.max(24.0 * k),
        )
    };
    h.click(&s, centre(choices));
    h.click(&s, row(3.0));
    assert_eq!(h.ui.settings.own.vista, Choice::One(Vista::Arramora));
    assert!(h.ui.settings.take_own_changed());
    // Closed once chosen: a press where its rows were is not a choice.
    h.click(&s, row(5.0));
    assert_eq!(h.ui.settings.own.vista, Choice::One(Vista::Arramora));
    h.click(&s, centre(choices));
    h.click(&s, row(0.0));
    assert_eq!(h.ui.settings.own.vista, Choice::Random);
    assert!(h.ui.settings.take_own_changed());
}
