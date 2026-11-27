//! The damage text over struck objects: the scripts' damage texts followed into Horizon's state,
//! each written over its object's box in its color, rising and fading over its life on the
//! interface's clock (the harness's frames, a sixtieth of a second each), moved off the texts
//! placed before it, over the selection's marks for the selection, kept on the screen across and
//! out of the bands along its top and foot that the HUD's bars fill where its layout shows them,
//! gone when its object leaves the world, and moved with its object once the world is drawn. Drawn
//! with the art the client carries.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use std::collections::BTreeMap;

use auberean_client_sdk::contract::notice::{HitText, Notice, Said, WorldNotice};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{self as v, View};
use auberean_horizon::draw::Rect;
use auberean_horizon::state::{Frame, GameState, Target, Vital};
use auberean_horizon::ui::damage::Drawn;
use auberean_horizon::ui::layout::{element, Edges, Placement};
use auberean_horizon::ui::marker::{self, Where};
use dereth_ui_kit::selection::{ARROW_BOB, ARROW_GAP, ARROW_HEIGHT};

use crate::{in_world, Harness};

const DRUDGE: u64 = 0x8130_0000_0000_0042;
const MITE: u64 = 0x8130_0000_0000_0043;

/// The drudge's box on the screen: two hundred and forty pixels tall, round the screen's middle.
const BOX: Rect = Rect {
    x: 900.0,
    y: 420.0,
    w: 120.0,
    h: 240.0,
};

/// A hit's text as the scripts fill it: red, two seconds, scale 1.
fn hit(text: &str) -> HitText {
    HitText {
        text: text.into(),
        color: [1.0, 0.0, 0.0, 0.25],
        seconds: 2.0,
        scale: 1.0,
    }
}

/// A player in the world with the drudge's box on the screen and `texts` sent this frame.
fn struck(texts: Vec<(u64, HitText)>) -> GameState {
    let mut s = in_world();
    s.damage.texts = texts;
    s.damage.at = BTreeMap::from([(DRUDGE, BOX)]);
    s
}

/// The last frame's texts.
fn drawn(h: &Harness) -> Vec<Drawn> {
    h.ui.damage.drawn().to_vec()
}

/// `n` frames of `s`, a sixtieth of a second each.
fn frames(h: &mut Harness, s: &GameState, n: usize) {
    for _ in 0..n {
        h.frame(s);
    }
}

/// How far two boxes overlap across and down, in pixels (0 or less where they do not).
fn overlap(a: Rect, b: Rect) -> (f32, f32) {
    (
        a.right().min(b.right()) - a.x.max(b.x),
        a.bottom().min(b.bottom()) - a.y.max(b.y),
    )
}

#[test]
fn a_hit_s_text_stands_over_its_object_in_its_color_then_rises_three_heights_and_fades_out_over_its_life(
) {
    let mut h = Harness::with_built_in_art();
    let mut s = struck(vec![(DRUDGE, hit("12"))]);
    h.frame(&s);
    let first = drawn(&h);
    assert_eq!(first.len(), 1, "{first:?}");
    let t = &first[0];
    assert_eq!((t.object, t.text.as_str()), (DRUDGE, "12"));
    assert_eq!(t.colour, 0xFFFF_0000, "its color, opaque");
    let middle = t.rect.x + t.rect.w / 2.0;
    assert!(
        (middle - (BOX.x + BOX.w / 2.0)).abs() <= 1.0,
        "centered: {t:?}"
    );
    assert!(
        (t.rect.bottom() - BOX.y).abs() <= 1.0,
        "standing on the box: {t:?}"
    );
    assert!(t.alpha > 0.99, "whole as it begins: {t:?}");
    // Sent once: the next frames bring no more.
    s.damage.texts.clear();
    frames(&mut h, &s, 60);
    let half = drawn(&h);
    assert_eq!(half.len(), 1);
    assert!(
        (half[0].alpha - 0.5).abs() < 0.02,
        "half gone half way: {half:?}"
    );
    let risen = t.rect.y - half[0].rect.y;
    let height = t.rect.h;
    assert!(
        (risen - 1.5 * height).abs() <= 1.0,
        "a second in, one and a half heights up: {risen} of {height}"
    );
    frames(&mut h, &s, 61);
    assert!(drawn(&h).is_empty(), "gone after two seconds");
    assert!(h.ui.damage.is_empty());
}

#[test]
fn a_second_hit_s_text_at_once_is_moved_off_the_first() {
    let mut h = Harness::with_built_in_art();
    let s = struck(vec![(DRUDGE, hit("12")), (DRUDGE, hit("345"))]);
    h.frame(&s);
    let texts = drawn(&h);
    assert_eq!(texts.len(), 2, "{texts:?}");
    // As the game moves a text left or up, it stops two pixels short.
    let (across, down) = overlap(texts[0].rect, texts[1].rect);
    assert!(
        across <= 2.0 || down <= 2.0,
        "apart: {:?} {:?}",
        texts[0].rect,
        texts[1].rect
    );
}

#[test]
fn a_hit_s_text_over_the_selection_stands_over_its_name_and_arrow() {
    let mut h = Harness::with_built_in_art();
    let mut s = struck(vec![(DRUDGE, hit("12"))]);
    s.target = Some(Target {
        id: DRUDGE,
        name: "Drudge".into(),
        health: None,
    });
    s.selection_at = Some(Where::Box(BOX));
    h.frame(&s);
    let texts = drawn(&h);
    assert_eq!(texts.len(), 1, "{texts:?}");
    let k = h.k();
    let line = crate::measured(&h, |p| {
        p.line_height(
            &dereth_ui_kit::paint::TextStyle::new(dereth_ui_kit::art::Family::Body, 14.0, 0)
                .edge(0),
        )
    });
    let name_top = marker::name_rect(BOX, k, line).y;
    let arrow_top = name_top - (ARROW_HEIGHT + ARROW_GAP + ARROW_BOB) * k;
    assert!(
        texts[0].rect.bottom() <= arrow_top + 1.0,
        "over the arrow over the name: {:?}, the arrow's top {arrow_top}",
        texts[0].rect
    );
}

/// The text over the drudge's box at `b`, as the first frame draws it.
fn drawn_over(b: Rect) -> (Drawn, f32) {
    let mut h = Harness::with_built_in_art();
    let mut s = struck(vec![(DRUDGE, hit("12"))]);
    s.damage.at.insert(DRUDGE, b);
    h.frame(&s);
    let texts = drawn(&h);
    assert_eq!(texts.len(), 1, "{texts:?}");
    (texts[0].clone(), h.k())
}

/// A player in the world with the experience band shown and the drudge's box on the screen.
fn with_experience() -> GameState {
    let mut s = struck(Vec::new());
    s.experience = Vital {
        current: 400,
        max: 1000,
    };
    s
}

/// The text over the drudge's box at `b` in `s`, as a frame draws it after one with the HUD the
/// same and no text, and where element `name` was outlined on it.
fn drawn_after_hud(h: &mut Harness, mut s: GameState, b: Rect, name: &str) -> (Drawn, Rect) {
    s.damage.at.insert(DRUDGE, b);
    h.frame(&s);
    s.damage.texts = vec![(DRUDGE, hit("12"))];
    h.frame(&s);
    let texts = drawn(h);
    assert_eq!(texts.len(), 1, "{texts:?}");
    let outline =
        h.ui.layout
            .outlines
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{name} shown"));
    (texts[0].clone(), outline)
}

/// A box reaching past the screen's top, and one at its foot.
const HIGH: Rect = Rect {
    x: 900.0,
    y: 10.0,
    w: 120.0,
    h: 240.0,
};
const LOW: Rect = Rect {
    x: 900.0,
    y: 1078.0,
    w: 120.0,
    h: 240.0,
};

/// Element `name` moved `(dx, dy)` layout units from its default place.
fn moved(h: &mut Harness, name: &str, dx: f32, dy: f32) {
    h.ui.layout.placements.insert(
        name.to_owned(),
        Placement {
            dx,
            dy,
            ..Placement::default()
        },
    );
}

/// Element `name` kept to the screen's edges `edges`, `(dx, dy)` layout units from them.
fn kept_to(h: &mut Harness, name: &str, edges: &str, dx: f32, dy: f32) {
    h.ui.layout.placements.insert(
        name.to_owned(),
        Placement {
            dx,
            dy,
            scale: 1.0,
            edges: Edges::read(edges),
        },
    );
}

/// Whether the text `t` stands with its top at `edge`, to the pixel.
fn top_at(t: &Drawn, edge: f32) {
    assert!(
        (t.rect.y - edge).abs() <= 1.0,
        "its top at the band's edge, {edge}: {t:?}"
    );
}

#[test]
fn a_hit_s_text_that_would_stand_in_the_band_along_the_screen_s_top_stands_at_the_experience_band_s_foot(
) {
    let mut h = Harness::with_built_in_art();
    let (t, band) = drawn_after_hud(&mut h, with_experience(), HIGH, element::EXPERIENCE);
    top_at(&t, band.bottom());
}

#[test]
fn a_hit_s_text_that_would_stand_in_the_band_along_the_screen_s_foot_stands_at_the_hotbar_s_top() {
    let mut h = Harness::with_built_in_art();
    let (t, bar) = drawn_after_hud(&mut h, struck(Vec::new()), LOW, element::HOTBAR);
    top_at(&t, bar.y);
}

#[test]
fn with_something_selected_a_hit_s_text_that_would_stand_over_the_target_bar_stands_at_its_foot() {
    let mut h = Harness::with_built_in_art();
    let mut s = with_experience();
    s.target = Some(Target {
        id: MITE,
        name: "Mite".into(),
        health: None,
    });
    let (t, bar) = drawn_after_hud(&mut h, s.clone(), HIGH, element::TARGET);
    top_at(&t, bar.bottom());
    // Selected no more: at once a text is no longer kept under the target bar.
    s.target = None;
    s.damage.at.insert(DRUDGE, HIGH);
    s.damage.texts = vec![(DRUDGE, hit("3"))];
    h.frame(&s);
    let free = drawn(&h);
    let three = free.iter().find(|t| t.text == "3").expect("drawn");
    assert!(three.rect.y < bar.y, "not kept under the bar: {free:?}");
    let (_, band) = drawn_after_hud(
        &mut Harness::with_built_in_art(),
        with_experience(),
        HIGH,
        element::EXPERIENCE,
    );
    assert!(
        bar.bottom() > band.bottom() + 20.0,
        "{bar:?} under {band:?}"
    );
}

#[test]
fn the_band_along_the_screen_s_top_follows_the_experience_band_where_the_hud_layout_moves_it() {
    let mut h = Harness::with_built_in_art();
    moved(&mut h, element::EXPERIENCE, 0.0, 60.0);
    let (t, band) = drawn_after_hud(&mut h, with_experience(), HIGH, element::EXPERIENCE);
    let k = h.k();
    assert!(band.bottom() > 70.0 * k, "moved down: {band:?}");
    top_at(&t, band.bottom());
}

#[test]
fn the_band_along_the_screen_s_foot_follows_the_hotbar_and_the_gauges_where_the_hud_layout_moves_them(
) {
    let mut h = Harness::with_built_in_art();
    moved(&mut h, element::HOTBAR, 0.0, -60.0);
    let (t, bar) = drawn_after_hud(&mut h, struck(Vec::new()), LOW, element::HOTBAR);
    let k = h.k();
    assert!(bar.y < 1080.0 - 150.0 * k, "moved up: {bar:?}");
    top_at(&t, bar.y);
    let mut h = Harness::with_built_in_art();
    moved(&mut h, element::PARAMETER, 0.0, -200.0);
    let (t, gauges) = drawn_after_hud(&mut h, struck(Vec::new()), LOW, element::PARAMETER);
    assert!(
        gauges.y < 1080.0 - 200.0 * k,
        "moved over the hotbar: {gauges:?}"
    );
    top_at(&t, gauges.y);
}

#[test]
fn a_bar_the_hud_layout_moves_across_the_screen_s_middle_keeps_no_band_and_one_moved_to_the_other_edge_keeps_that_edge_s(
) {
    // The hotbar in the screen's middle: the foot keeps only the gauges' band.
    let mut h = Harness::with_built_in_art();
    kept_to(&mut h, element::HOTBAR, "cm", 0.0, 0.0);
    let (t, gauges) = drawn_after_hud(&mut h, struck(Vec::new()), LOW, element::PARAMETER);
    let bar =
        h.ui.layout
            .outlines
            .iter()
            .find(|(n, _)| *n == element::HOTBAR);
    assert!(
        bar.is_some_and(|(_, r)| r.y < 540.0 && r.bottom() > 540.0),
        "across the middle: {bar:?}"
    );
    top_at(&t, gauges.y);
    // Nor the top a band down to its foot: with nothing else along the top, none at all.
    let mut h = Harness::with_built_in_art();
    kept_to(&mut h, element::HOTBAR, "cm", 0.0, 0.0);
    let (t, _) = drawn_after_hud(&mut h, struck(Vec::new()), HIGH, element::HOTBAR);
    top_at(&t, 0.0);
    // The hotbar along the top: the top's band reaches its foot.
    let mut h = Harness::with_built_in_art();
    kept_to(&mut h, element::HOTBAR, "ct", 0.0, 0.0);
    let (t, bar) = drawn_after_hud(&mut h, struck(Vec::new()), HIGH, element::HOTBAR);
    assert!(bar.y < 1.0, "along the top: {bar:?}");
    top_at(&t, bar.bottom());
}

#[test]
fn a_hit_s_text_over_a_box_past_the_screen_s_side_stands_centered_on_the_side() {
    let (t, _) = drawn_over(Rect::new(-200.0, 420.0, 120.0, 240.0));
    let middle = t.rect.x + t.rect.w / 2.0;
    assert!(middle.abs() <= 1.0, "centered on the left side: {t:?}");
}

#[test]
fn a_hit_s_text_over_an_object_not_on_the_screen_is_not_drawn_and_ages_all_the_same() {
    let mut h = Harness::with_built_in_art();
    let mut s = struck(vec![(MITE, hit("7"))]);
    h.frame(&s);
    assert!(drawn(&h).is_empty(), "the mite is not on the screen");
    s.damage.texts.clear();
    frames(&mut h, &s, 59);
    s.damage.at.insert(MITE, BOX);
    h.frame(&s);
    let texts = drawn(&h);
    assert_eq!(texts.len(), 1, "on the screen now: {texts:?}");
    assert!(
        (texts[0].alpha - 0.5).abs() < 0.02,
        "a second old: {texts:?}"
    );
}

#[test]
fn a_hit_s_text_goes_when_its_object_leaves_the_world() {
    let mut h = Harness::with_built_in_art();
    let mut s = struck(vec![(DRUDGE, hit("12"))]);
    h.frame(&s);
    assert_eq!(drawn(&h).len(), 1);
    s.damage.texts.clear();
    s.damage.gone = vec![DRUDGE];
    h.frame(&s);
    s.damage.gone.clear();
    h.frame(&s);
    assert!(drawn(&h).is_empty(), "gone with its object");
    assert!(h.ui.damage.is_empty());
}

#[test]
fn a_hit_s_text_is_moved_with_its_object_once_the_world_is_drawn_and_hidden_once_it_is_off_the_screen(
) {
    let mut h = Harness::with_built_in_art();
    let s = struck(vec![(DRUDGE, hit("12"))]);
    h.frame(&s);
    assert_eq!(h.ui.damage.anchored(), &[DRUDGE]);
    assert_eq!(h.list.anchored.len(), 1, "the text's quads kept as a group");
    let range = h.list.anchored[0].quads.clone();
    let text =
        |h: &Harness| -> Vec<Rect> { h.list.quads[range.clone()].iter().map(|q| q.dst).collect() };
    let before = text(&h);
    assert!(!before.is_empty(), "written");
    marker::follow(&mut h.list, None, &[Some(BOX.offset(30.0, -20.0))]);
    let moved = text(&h);
    for (a, b) in before.iter().zip(&moved) {
        assert!(
            (b.x - a.x - 30.0).abs() < 0.01 && (b.y - a.y + 20.0).abs() < 0.01,
            "moved with the drudge: {a:?} to {b:?}"
        );
    }
    marker::follow(&mut h.list, None, &[None]);
    assert!(
        text(&h).iter().all(|r| r.w == 0.0 && r.h == 0.0),
        "hidden with the drudge off the screen"
    );
}

fn no_name(_: &TextRef) -> Option<String> {
    None
}

/// A view of the world with the objects `ids` about the player.
fn view_of(ids: &[u64]) -> View {
    View {
        player_id: Some(crate::PLAYER),
        player: Some(v::Player {
            id: crate::PLAYER,
            ..v::Player::default()
        }),
        world: Some(v::Surroundings {
            player: crate::PLAYER,
            things: ids
                .iter()
                .map(|id| v::Thing {
                    id: *id,
                    ..v::Thing::default()
                })
                .collect(),
            ..v::Surroundings::default()
        }),
        ..View::default()
    }
}

fn follow(s: &mut GameState, view: &View, notices: &[Notice]) {
    s.follow(&Frame {
        view: Some(view),
        notices,
        events: &[],
        told: &[],
        lines: &[],
        write: &no_name,
        unknown_command: "",
    });
}

#[test]
fn the_scripts_damage_texts_are_kept_for_their_frame_with_the_objects_that_left_the_world() {
    let mut s = GameState::default();
    let both = view_of(&[DRUDGE, MITE]);
    let damage = |about: Option<u64>, h: Option<HitText>| Notice {
        about,
        said: Said::World(WorldNotice::Damage(h)),
    };
    follow(
        &mut s,
        &both,
        &[
            damage(Some(DRUDGE), Some(hit("12"))),
            damage(Some(MITE), None),
            damage(None, Some(hit("3"))),
            damage(Some(MITE), Some(hit("4"))),
        ],
    );
    assert_eq!(
        s.damage.texts,
        vec![(DRUDGE, hit("12")), (MITE, hit("4"))],
        "each text with its object, in order"
    );
    assert!(s.damage.gone.is_empty());
    follow(&mut s, &view_of(&[DRUDGE]), &[]);
    assert!(s.damage.texts.is_empty(), "for their frame only");
    assert_eq!(s.damage.gone, vec![MITE], "the mite left");
    follow(&mut s, &view_of(&[DRUDGE]), &[]);
    assert!(s.damage.gone.is_empty(), "once");
}
