//! Character select, connecting and the disconnected screen.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::Action;
use auberean_horizon::ui::pregame::{
    Confirm, PanelLayout, Screen, Screens, SelectLayout, BRAND_MARK, CARD_NORMAL, CARD_SELECTED,
    FOOT_SHADE, ONLINE, TOP_BAND, TOP_SHADE, WELL_MARGIN,
};
use auberean_horizon::ui::vk;
use auberean_shell::interface::Phase;

use super::{click, frame, key, listed, ELM, ROWAN};
use crate::{centre, drew_piece, Harness};

fn layout(h: &Harness) -> SelectLayout {
    SelectLayout::new(h.screen, h.k())
}

/// The question's first button (its yes).
fn yes(h: &Harness) -> (f32, f32) {
    centre(*h.ui.pregame.confirm_buttons.first().expect("a question up"))
}

#[test]
fn the_screen_follows_the_session() {
    let h = Harness::new();
    let mut p = listed();
    p.characters = None;
    p.phase = Phase::Disconnected;
    assert_eq!(h.ui.pregame.screen(&p), Screen::Connecting);
    p.ended = Some("The server went down.".into());
    assert_eq!(h.ui.pregame.screen(&p), Screen::Disconnected);
    let mut p = listed();
    assert_eq!(h.ui.pregame.screen(&p), Screen::Select);
    p.phase = Phase::Entering;
    assert_eq!(h.ui.pregame.screen(&p), Screen::Entering);
}

#[test]
fn a_click_selects_a_character_and_a_second_asks_to_enter_with_it() {
    let mut h = Harness::new();
    let p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    let k = h.k();
    let elm = centre(l.card(1, 0.0, k));
    assert!(click(&mut h, &p, elm).acts.is_empty());
    assert_eq!(h.ui.pregame.selected, 1);
    // Elm is being deleted: a second click does not ask to enter.
    click(&mut h, &p, elm);
    assert_eq!(h.ui.pregame.confirm, None);
    let rowan = centre(l.card(0, 0.0, k));
    click(&mut h, &p, rowan);
    assert!(h.ui.pregame.confirm.is_none(), "selected first");
    click(&mut h, &p, rowan);
    assert_eq!(
        h.ui.pregame.confirm,
        Some(Confirm::Enter(ROWAN, "Rowan".into()))
    );
    let at = yes(&h);
    let out = click(&mut h, &p, at);
    assert_eq!(out.acts, vec![Action::PlayCharacter { character: ROWAN }]);
    assert!(h.ui.pregame.entering);
    assert_eq!(h.ui.pregame.screen(&p), Screen::Entering);
}

#[test]
fn enter_and_the_arrows_choose_and_ask_and_no_answers_nothing() {
    let mut h = Harness::new();
    let p = listed();
    frame(&mut h, &p);
    key(&mut h, &p, vk::DOWN);
    assert_eq!(h.ui.pregame.selected, 1);
    key(&mut h, &p, vk::DOWN);
    assert_eq!(h.ui.pregame.selected, 0, "round to the first");
    key(&mut h, &p, vk::ENTER);
    assert!(matches!(
        h.ui.pregame.confirm,
        Some(Confirm::Enter(ROWAN, _))
    ));
    let no = centre(h.ui.pregame.confirm_buttons[1]);
    assert!(click(&mut h, &p, no).acts.is_empty());
    assert_eq!(h.ui.pregame.confirm, None);
}

#[test]
fn delete_asks_first_and_a_character_being_deleted_is_restored_by_the_same_button() {
    let mut h = Harness::new();
    let p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    assert!(
        click(&mut h, &p, centre(l.delete)).acts.is_empty(),
        "asked first"
    );
    assert!(matches!(
        h.ui.pregame.confirm,
        Some(Confirm::Delete(ROWAN, _))
    ));
    let at = yes(&h);
    let out = click(&mut h, &p, at);
    assert_eq!(out.acts, vec![Action::DeleteCharacter { character: ROWAN }]);
    h.ui.pregame.selected = 1;
    key(&mut h, &p, vk::DELETE);
    assert!(matches!(
        h.ui.pregame.confirm,
        Some(Confirm::Restore(ELM, _))
    ));
    let at = yes(&h);
    let out = click(&mut h, &p, at);
    assert_eq!(
        out.acts,
        vec![Action::RestoreCharacter {
            character: ELM,
            name: "Elm".into()
        }]
    );
}

#[test]
fn create_opens_creation_only_while_the_account_has_room() {
    let mut h = Harness::new();
    let mut p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    assert!(click(&mut h, &p, centre(l.create)).create);
    p.slots = 2;
    assert!(!click(&mut h, &p, centre(l.create)).create, "full");
}

#[test]
fn the_selected_character_stands_in_the_middle_unless_it_is_being_deleted() {
    let mut h = Harness::new();
    let p = listed();
    frame(&mut h, &p);
    let (r, _) = h.ui.pregame.figure.expect("Rowan stands");
    assert_eq!(r, layout(&h).figure);
    h.ui.pregame.selected = 1;
    frame(&mut h, &p);
    assert_eq!(h.ui.pregame.figure, None);
}

#[test]
fn an_entry_refused_back_to_the_list_says_why_and_enter_may_be_asked_again() {
    let mut h = Harness::new();
    let mut p = listed();
    h.ui.pregame.entering = true;
    p.refused = Some(auberean_horizon::pregame::Refused {
        reason: 11,
        line: "Still in the world.".into(),
        to_list: true,
    });
    frame(&mut h, &p);
    assert!(!h.ui.pregame.entering);
    assert_eq!(h.ui.pregame.error.as_deref(), Some("Still in the world."));
    assert_eq!(h.ui.pregame.screen(&p), Screen::Select);
}

#[test]
fn quit_and_the_credits() {
    let mut h = Harness::new();
    let p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    assert!(click(&mut h, &p, centre(l.quit)).quit);
    click(&mut h, &p, centre(l.credits));
    assert_eq!(h.ui.pregame.screen(&p), Screen::Credits);
    key(&mut h, &p, vk::ESCAPE);
    assert_eq!(h.ui.pregame.screen(&p), Screen::Select);
}

#[test]
fn the_disconnected_screen_logs_in_again_or_quits() {
    let mut h = Harness::new();
    let mut p = listed();
    p.phase = Phase::Disconnected;
    p.characters = None;
    p.ended = Some("The server went down.".into());
    frame(&mut h, &p);
    let l = PanelLayout::new(h.screen, h.k());
    assert!(click(&mut h, &p, centre(l.again)).connect);
    assert!(click(&mut h, &p, centre(l.quit)).quit);
}

/// A flat quad of `colour` over exactly `r` in the last frame's list.
fn filled(h: &Harness, r: auberean_horizon::draw::Rect, colour: u32) -> bool {
    h.list
        .quads
        .iter()
        .any(|q| q.tex.is_none() && q.dst == r && q.colour == colour)
}

#[test]
fn character_select_stands_over_the_world_between_darker_bands_at_the_top_and_the_foot() {
    let mut h = Harness::with_built_in_art();
    let p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    assert!(Screens::over_the_world(Screen::Select));
    assert!(Screens::over_the_world(Screen::Creation));
    let (sw, sh) = h.screen;
    let whole = auberean_horizon::draw::Rect::new(0.0, 0.0, sw, sh);
    assert!(
        !h.list
            .quads
            .iter()
            .any(|q| q.dst == whole && q.colour >> 24 == 0xFF),
        "nothing opaque over the whole screen: the world shows"
    );
    assert!(filled(&h, l.top, TOP_SHADE), "the top band");
    assert!(filled(&h, l.foot, FOOT_SHADE), "the foot band");
    assert_eq!((l.top.y, l.top.w), (0.0, sw));
    assert_eq!((l.foot.bottom(), l.foot.w), (sh, sw));
    // The buttons stand on the foot's band.
    for b in [l.enter, l.create, l.delete, l.quit, l.credits] {
        assert!(b.y >= l.foot.y && b.bottom() <= l.foot.bottom(), "{b:?}");
    }
    // The connecting screen stands on its own.
    let mut c = listed();
    c.characters = None;
    frame(&mut h, &c);
    assert!(!Screens::over_the_world(Screen::Connecting));
    assert!(h
        .list
        .quads
        .iter()
        .any(|q| q.dst == whole && q.colour >> 24 == 0xFF));
}

#[test]
fn the_version_stands_at_the_bottom_left_of_the_screens_before_the_world() {
    let mut h = Harness::with_built_in_art();
    h.ui.pregame.version = "0.1.9".into();
    let p = listed();
    frame(&mut h, &p);
    let l = layout(&h);
    let (version, at) =
        h.ui.pregame
            .shown
            .version
            .clone()
            .expect("the version drawn");
    assert_eq!(version, "0.1.9");
    assert_eq!(at, l.version);
    let (sw, sh) = h.screen;
    assert!(at.0 < sw * 0.05 && at.1 > sh - 30.0 * h.k(), "{at:?}");
    assert!(at.1 >= l.quit.bottom(), "under the buttons");
    // Under connecting's too; never over the credits or the way into the world.
    let mut c = listed();
    c.characters = None;
    frame(&mut h, &c);
    assert!(h.ui.pregame.shown.version.is_some(), "connecting");
    h.ui.pregame.credits = Some(0.0);
    frame(&mut h, &p);
    assert_eq!(h.ui.pregame.screen(&p), Screen::Credits);
    assert!(h.ui.pregame.shown.version.is_none(), "the credits");
    h.ui.pregame.credits = None;
    h.ui.pregame.entering = true;
    frame(&mut h, &p);
    assert_eq!(h.ui.pregame.screen(&p), Screen::Entering);
    assert!(h.ui.pregame.shown.version.is_none(), "entering");
    // A host that names no version shows none.
    let mut h = Harness::with_built_in_art();
    frame(&mut h, &p);
    assert!(h.ui.pregame.shown.version.is_none());
}

#[test]
fn the_world_s_name_and_its_online_lamp_head_the_screen() {
    let mut h = Harness::with_built_in_art();
    let mut p = listed();
    p.world = "Eastwatch".into();
    frame(&mut h, &p);
    let l = layout(&h);
    let (name, word, lamp) = h.ui.pregame.shown.world.clone().expect("the world's line");
    assert_eq!((name.as_str(), word), ("Eastwatch", "Online"));
    // After the name, on the line's middle, in the top band.
    assert!(lamp.x > l.world.x + 40.0 * h.k(), "{lamp:?} after the name");
    let mid = |r: auberean_horizon::draw::Rect| r.y + r.h / 2.0;
    assert!((mid(lamp) - mid(l.world)).abs() < 0.5);
    assert!(lamp.bottom() < l.top.bottom());
    assert!(filled(&h, lamp.inset(2.0 * h.k()), ONLINE), "lit");
    // A world with no name yet: the lamp alone, at the line's start.
    p.world.clear();
    frame(&mut h, &p);
    let (_, _, lamp) = h.ui.pregame.shown.world.clone().expect("the lamp");
    assert!((lamp.x - l.world.x).abs() < 0.5);
}

#[test]
fn character_select_shows_the_client_s_mark_centered_on_the_top_band() {
    let mut h = Harness::with_built_in_art();
    let p = listed();
    frame(&mut h, &p);
    let k = h.k();
    let mark = h.ui.pregame.shown.brand.expect("the mark drawn");
    assert_eq!(drew_piece(&h, BRAND_MARK), vec![mark]);
    assert!((mark.h - dereth_ui_kit::widgets::BRAND_MARK_HEIGHT * k).abs() < 0.01);
    assert!(
        (mark.x + mark.w / 2.0 - h.screen.0 / 2.0).abs() < 0.01,
        "{mark:?}"
    );
    assert!(
        (mark.y + mark.h / 2.0 - TOP_BAND * k / 2.0).abs() < 0.01,
        "{mark:?}"
    );
    // The mark is its picture with its name beside it: wider than tall, as the art keeps it.
    let (w, ht) = crate::measured(&h, |p| p.art.piece_size(BRAND_MARK)).expect("the piece");
    assert!((mark.w / mark.h - w / ht).abs() < 0.01);
    // With no art, the name is written plainly and no mark is drawn.
    let mut plain = Harness::new();
    frame(&mut plain, &p);
    assert_eq!(plain.ui.pregame.shown.brand, None);
}

#[test]
fn each_card_s_well_holds_the_client_s_picture_a_little_clear_of_its_edges() {
    let mut h = Harness::with_built_in_art();
    let p = listed();
    frame(&mut h, &p);
    let k = h.k();
    let wells = h.ui.pregame.shown.wells.clone();
    assert_eq!(wells.len(), 2, "a well on each card");
    // The mark's picture alone, the square at its left end.
    let picture = crate::measured(&h, |p| p.piece(BRAND_MARK))
        .map(|m| dereth_ui_kit::widgets::brand_picture(&m))
        .expect("the mark");
    let drawn: Vec<_> = h
        .list
        .quads
        .iter()
        .filter(|q| q.tex == Some(picture.tex) && q.src == picture.src)
        .map(|q| q.dst)
        .collect();
    assert_eq!(drawn.len(), wells.len());
    let margin = WELL_MARGIN * k;
    for (at, well) in drawn.iter().zip(&wells) {
        assert!((at.w - at.h).abs() < 0.01, "square: {at:?}");
        // As large as fits with the margin, in the well's middle.
        assert!(
            (at.w - (well.w.min(well.h) - 2.0 * margin)).abs() < 0.01,
            "{at:?} {well:?}"
        );
        assert!((at.x + at.w / 2.0 - (well.x + well.w / 2.0)).abs() < 0.01);
        assert!((at.y + at.h / 2.0 - (well.y + well.h / 2.0)).abs() < 0.01);
    }
}

#[test]
fn the_character_cards_are_horizon_s_cards_lit_while_selected() {
    let mut h = Harness::with_built_in_art();
    let p = listed();
    h.input.mouse = (0.0, 0.0);
    frame(&mut h, &p);
    let l = layout(&h);
    let k = h.k();
    assert_eq!(
        h.ui.pregame.shown.cards,
        vec![
            (l.card(0, 0.0, k), Some(CARD_SELECTED)),
            (l.card(1, 0.0, k), Some(CARD_NORMAL)),
        ]
    );
    // Under the pointer, lit too.
    h.input.mouse = centre(l.card(1, 0.0, k));
    frame(&mut h, &p);
    assert_eq!(h.ui.pregame.shown.cards[1].1, Some(CARD_SELECTED));
    // Art with no cards: a plain panel each.
    let mut plain = Harness::new();
    frame(&mut plain, &p);
    assert!(plain
        .ui
        .pregame
        .shown
        .cards
        .iter()
        .all(|(_, piece)| piece.is_none()));
}
