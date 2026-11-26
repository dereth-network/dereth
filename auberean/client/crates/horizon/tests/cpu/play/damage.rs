//! The damage text over struck objects: the scripts' damage texts followed into Horizon's state,
//! each written over its object's box in its color, rising and fading over its life on the
//! interface's clock (the harness's frames, a sixtieth of a second each), moved off the texts
//! placed before it, over the selection's marks for the selection, gone when its object leaves the
//! world, and moved with its object once the world is drawn. Drawn with the art the client
//! carries.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use std::collections::BTreeMap;

use auberean_client_sdk::contract::notice::{HitText, Notice, Said, WorldNotice};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{self as v, View};
use auberean_horizon::draw::Rect;
use auberean_horizon::state::{Frame, GameState, Target};
use auberean_horizon::ui::damage::Drawn;
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
