//! Character creation.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use std::collections::BTreeMap;

use auberean_client_sdk::Action;
use auberean_horizon::creation::Creation;
use auberean_horizon::pregame::Made;
use auberean_horizon::ui::creation::{CreationLayout, LABEL_PAD, TURN_PER_PIXEL};
use auberean_horizon::ui::pregame::Screen;
use auberean_primitives::random::Random;
use auberean_world::chargen::{self, Choice};

use super::{click, frame, listed};
use crate::{centre, Harness};

/// Choices for every race and sex: height and skin tone, five stops each.
fn choices() -> BTreeMap<(u32, u32), Vec<Choice>> {
    let mut m = BTreeMap::new();
    for (race, _) in auberean_horizon::creation::RACES {
        for (sex, _) in auberean_horizon::creation::SEXES {
            let c = |physique_type| Choice {
                physique_type,
                label: "",
                key: 0,
                table: 0,
                values: vec![0.1, 0.2, 0.3, 0.4, 0.5],
            };
            m.insert((race, sex), vec![c(chargen::HEIGHT), c(chargen::SKIN_TONE)]);
        }
    }
    m
}

/// Creation open over [`choices`], the account may make `may_make`.
fn opened(may_make: u32) -> Harness {
    let mut h = Harness::new();
    h.ui.pregame.creation = Some(Creation::with_choices(choices(), may_make, Random::new()));
    h
}

fn layout(h: &Harness) -> CreationLayout {
    let races = h.ui.pregame.creation.as_ref().expect("open").races();
    let labels: Vec<f32> = crate::measured(h, |p| {
        races
            .iter()
            .map(|(_, name)| dereth_ui_kit::widgets::button_label_width(p, name))
            .collect()
    });
    CreationLayout::new(h.screen, h.k(), &labels)
}

fn creation(h: &Harness) -> &Creation {
    h.ui.pregame.creation.as_ref().expect("open")
}

#[test]
fn creation_offers_the_restricted_races_only_to_an_account_that_may_make_them() {
    let h = opened(0);
    let races: Vec<_> = creation(&h).races().iter().map(|r| r.1).collect();
    assert_eq!(races, ["Human", "Lugian", "Tumerok"]);
    let h = opened(chargen::EMPYREAN);
    assert_eq!(creation(&h).races().len(), 4);
}

#[test]
fn the_race_and_sex_buttons_choose_and_each_keeps_its_own_avatar() {
    let mut h = opened(0);
    let p = listed();
    assert_eq!(h.ui.pregame.screen(&p), Screen::Creation);
    frame(&mut h, &p);
    let l = layout(&h);
    click(&mut h, &p, centre(l.races[1]));
    click(&mut h, &p, centre(l.sexes[1]));
    assert_eq!(creation(&h).chosen, (chargen::LUGIAN, chargen::FEMALE));
    let rows = creation(&h).rows();
    assert_eq!(
        rows.iter().map(|r| r.name).collect::<Vec<_>>(),
        ["Height", "Skin tone"]
    );
    assert_eq!(rows[0].stops, 5);
}

#[test]
fn a_slider_sets_its_choice_to_the_stop_under_the_pointer() {
    let mut h = opened(0);
    let p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    let k = h.k();
    let s = l.slider(1, k);
    // The skin tone's slider pressed at its right end: its last stop.
    click(&mut h, &p, (s.right() - 1.0, s.y + s.h / 2.0));
    let row = creation(&h).rows()[1].clone();
    assert_eq!(row.at, 4);
    let a = creation(&h).avatar().expect("chosen");
    assert_eq!(a.value(chargen::SKIN_TONE), Some(0.5));
}

#[test]
fn the_height_chosen_is_the_height_the_character_is_made_with() {
    let mut h = opened(0);
    let p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    let k = h.k();
    // The height's slider pressed at its right end: the tallest.
    let s = l.slider(0, k);
    click(&mut h, &p, (s.right() - 1.0, s.y + s.h / 2.0));
    assert_eq!(creation(&h).tallest(), Some(0.5));
    h.ui.pregame.creation.as_mut().expect("open").name = "Ash".into();
    let out = click(&mut h, &p, centre(l.create));
    let [Action::CreateCharacter(c)] = out.acts.as_slice() else {
        panic!("one character asked: {:?}", out.acts);
    };
    let height = c.physique.iter().find(|(t, _)| *t == chargen::HEIGHT);
    assert_eq!(height.map(|(_, v)| *v), Some(0.5), "{:?}", c.physique);
}

#[test]
fn randomize_picks_the_choices_again() {
    let mut h = opened(0);
    let p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    let before = creation(&h).avatar().cloned();
    let mut changed = false;
    for _ in 0..20 {
        click(&mut h, &p, centre(l.randomize));
        changed |= creation(&h).avatar().cloned() != before;
    }
    assert!(changed, "the choices picked again");
}

#[test]
fn create_waits_for_a_name_and_asks_for_the_chosen_avatar_under_it() {
    let mut h = opened(0);
    let p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    assert!(
        click(&mut h, &p, centre(l.create)).acts.is_empty(),
        "no name"
    );
    h.ui.pregame.creation.as_mut().expect("open").name = "  Ash ".into();
    let out = click(&mut h, &p, centre(l.create));
    let [Action::CreateCharacter(c)] = out.acts.as_slice() else {
        panic!("one character asked: {:?}", out.acts);
    };
    assert_eq!(String::from_utf16_lossy(&c.name), "Ash");
    assert_eq!((c.race, c.sex), (chargen::HUMAN, chargen::MALE));
    assert!(h.ui.pregame.creation_window.waiting);
    assert!(
        click(&mut h, &p, centre(l.create)).acts.is_empty(),
        "asked once"
    );
}

#[test]
fn a_character_made_is_entered_and_one_refused_says_why() {
    let mut h = opened(0);
    let mut p = listed();
    h.ui.pregame.creation.as_mut().expect("open").name = "Ash".into();
    frame(&mut h, &p);
    let l = layout(&h);
    click(&mut h, &p, centre(l.create));
    p.made = Some(Made::Refused("That name is taken.".into()));
    frame(&mut h, &p);
    assert!(!h.ui.pregame.creation_window.waiting);
    assert_eq!(
        h.ui.pregame.creation_window.error.as_deref(),
        Some("That name is taken.")
    );
    p.made = None;
    click(&mut h, &p, centre(l.create));
    p.made = Some(Made::Made(7));
    let out = frame(&mut h, &p);
    assert_eq!(out.acts, vec![Action::PlayCharacter { character: 7 }]);
    assert!(h.ui.pregame.creation.is_none());
    assert_eq!(h.ui.pregame.screen(&p), Screen::Entering);
}

#[test]
fn back_returns_to_character_select() {
    let mut h = opened(0);
    let p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    click(&mut h, &p, centre(l.back));
    assert!(h.ui.pregame.creation.is_none());
    assert_eq!(h.ui.pregame.screen(&p), Screen::Select);
}

#[test]
fn a_drag_across_the_figure_turns_it() {
    let mut h = opened(0);
    let p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    let (x, y) = centre(l.figure);
    h.input.mouse = (x, y);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    frame(&mut h, &p);
    h.input.mouse = (x + 40.0, y);
    frame(&mut h, &p);
    h.input.down[0] = false;
    h.input.released[0] = true;
    frame(&mut h, &p);
    let turn = creation(&h).turn;
    assert!((turn - 40.0 * TURN_PER_PIXEL).abs() < 1e-3, "{turn}");
    assert_eq!(
        h.ui.pregame.creation_window.figure.map(|f| f.0),
        Some(l.figure)
    );
}

#[test]
fn every_race_name_stands_clear_of_its_button_s_ends_at_each_scale() {
    // 100% at two window sizes, then the automatic 150% and 200%.
    for screen in [
        (1920.0, 1080.0),
        (1280.0, 960.0),
        (2560.0, 1440.0),
        (3840.0, 2160.0),
    ] {
        let mut h = Harness::with_built_in_art();
        h.screen = screen;
        h.ui.pregame.creation = Some(Creation::with_choices(
            choices(),
            chargen::EMPYREAN | chargen::DRUDGE,
            Random::new(),
        ));
        let p = listed();
        frame(&mut h, &p);
        let k = h.ui.layout_scale(screen);
        let buttons = h.ui.pregame.creation_window.races.clone();
        let races = creation(&h).races();
        assert_eq!(buttons.len(), races.len());
        for ((_, name), r) in races.iter().zip(&buttons) {
            let wide = crate::measured(&h, |p| dereth_ui_kit::widgets::button_label_width(p, name));
            // The name centered in its button, at its own size.
            let name_rect = (r.x + (r.w - wide) / 2.0, r.x + (r.w + wide) / 2.0);
            assert!(
                name_rect.0 >= r.x + LABEL_PAD * k - 0.5
                    && name_rect.1 <= r.right() - LABEL_PAD * k + 0.5,
                "{screen:?} {name}: {wide} in {r:?}"
            );
        }
    }
}

#[test]
fn every_race_button_is_wide_enough_for_its_whole_label() {
    for screen in [(1920.0, 1080.0), (1280.0, 960.0)] {
        let mut h = Harness::with_built_in_art();
        h.screen = screen;
        h.ui.pregame.creation = Some(Creation::with_choices(
            choices(),
            chargen::EMPYREAN | chargen::DRUDGE,
            Random::new(),
        ));
        let races = creation(&h).races();
        assert_eq!(races.len(), 5);
        let l = layout(&h);
        let k = h.k();
        for ((_, name), r) in races.iter().zip(&l.races) {
            let wide = crate::measured(&h, |p| dereth_ui_kit::widgets::button_label_width(p, name));
            // The room a button keeps for its label before the kit draws it smaller.
            assert!(
                wide > 0.0 && wide <= r.w - 12.0 * k,
                "{screen:?} {name}: {wide} in {}",
                r.w
            );
        }
        for pair in l.races.windows(2) {
            assert!(pair[0].right() <= pair[1].x, "{screen:?}: apart");
        }
        let last = l.races.last().expect("races");
        assert!(
            last.right() <= l.panel.right() - 20.0 * k + 1.0,
            "{screen:?}: in the panel"
        );
    }
}
