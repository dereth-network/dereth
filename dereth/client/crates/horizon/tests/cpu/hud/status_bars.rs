//! The status bars under the two options of the Client page's Display heading: Status Bar Colors
//! on the parameter bar, the party list and the fellowship window, and Large Status Bar Numbers
//! on the parameter bar.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_client_contract::options::status_bars::{self, Colors};
use dereth_client_contract::options::store;
use dereth_client_contract::view::{FellowEntry, FellowshipView, PrefValue};
use dereth_horizon::art::Family;
use dereth_horizon::draw::{Quad, Rect};
use dereth_horizon::ui::game::{GameState, Vital};
use dereth_horizon::ui::panels::WindowId;
use dereth_primitives::ObjectId;

use crate::Harness;

fn vital(current: u32) -> Vital {
    Vital { current, max: 100 }
}

fn in_world(health: u32) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        level: 20,
        health: vital(health),
        stamina: vital(60),
        mana: vital(40),
        ..GameState::default()
    }
}

/// In the world with a fellow: the party list has two rows, and so has the fellowship window.
fn in_a_fellowship() -> GameState {
    let fellow = |id: u32, name: &str| FellowEntry {
        id: ObjectId(id),
        name: name.into(),
        level: 20,
        current_health: 80,
        max_health: 100,
        current_stamina: 60,
        max_stamina: 100,
        current_mana: 40,
        max_mana: 100,
        ..FellowEntry::default()
    };
    GameState {
        player_id: Some(ObjectId(0x5000_0001)),
        fellowship: vec![
            ("Tester".into(), vital(80), vital(60), vital(40), true),
            ("Fellow".into(), vital(80), vital(60), vital(40), false),
        ],
        fellowship_view: Some(FellowshipView {
            name: "Band".into(),
            leader: ObjectId(0x5000_0001),
            members: vec![fellow(0x5000_0001, "Tester"), fellow(0x5000_0002, "Fellow")],
            ..FellowshipView::default()
        }),
        ..in_world(80)
    }
}

/// The frame drawn once the bars have settled.
fn settled(h: &mut Harness, state: &GameState) -> Vec<Quad> {
    for _ in 0..3 {
        h.frame(state);
    }
    h.list.quads.clone()
}

/// Horizon's own colours, place by place, beside retail's: the parameter bar's, the party list's
/// and the fellowship window's health, stamina and mana.
const HORIZON: [[u32; 3]; 3] = [
    [0xFF78_D85A, 0xFFF0_C050, 0xFFF0_80D8],
    [0xFF6E_CB5C, 0xFFE8_B84A, 0xFFDF_6FB5],
    [0xFF6E_CB5C, 0xFFE8_B84A, 0xFFD8_6CC8],
];
const CLASSIC: [u32; 3] = [0xFFC8_1E22, 0xFFE0_9E1C, 0xFF1A_70B4];

/// Horizon's colours draw the three places' bars as before; Classic draws the same frame with
/// each of those colours, and nothing else, turned to retail's red, amber and blue; and Horizon
/// again draws it as at first, at once.
///
/// Behaviour: hud.status-bars.the-colors-are-horizon-s-or-the-original-game-s
#[test]
fn status_bar_colors_turn_the_three_places_bars_to_retail_s_and_back_at_once() {
    store::init();
    let state = in_a_fellowship();
    let mut h = Harness::with_built_in_art();
    h.ui.windows.open(WindowId::Social, -1.0);
    let horizon = settled(&mut h, &state);
    let has = |quads: &[Quad], c: u32| quads.iter().any(|q| q.colour == c);
    for c in HORIZON.iter().flatten() {
        assert!(has(&horizon, *c), "{c:08X} drawn at first");
    }
    for c in CLASSIC {
        assert!(!has(&horizon, c), "{c:08X} not drawn at first");
    }

    assert!(store::set_value(
        status_bars::STATUS_BAR_COLORS,
        PrefValue::Int(Colors::Classic.value())
    ));
    let classic = settled(&mut h, &state);
    assert_eq!(classic.len(), horizon.len(), "the same frame");
    let mut turned = std::collections::BTreeSet::new();
    for (was, now) in horizon.iter().zip(&classic) {
        if was == now {
            continue;
        }
        assert_eq!(
            Quad {
                colour: was.colour,
                ..*now
            },
            *was,
            "only a colour changes"
        );
        let vital = HORIZON
            .iter()
            .find_map(|place| place.iter().position(|c| *c == was.colour))
            .unwrap_or_else(|| panic!("{:08X} is no bar's colour", was.colour));
        assert_eq!(now.colour, CLASSIC[vital], "{:08X}", was.colour);
        turned.insert(was.colour);
    }
    let every: std::collections::BTreeSet<u32> = HORIZON.iter().flatten().copied().collect();
    assert_eq!(turned, every, "every place's every bar");

    assert!(store::set_value(
        status_bars::STATUS_BAR_COLORS,
        PrefValue::Int(Colors::Horizon.value())
    ));
    assert_eq!(settled(&mut h, &state), horizon);
    store::clear();
}

/// The quads of `ch` in `family` at `points`, drawn in white (the glyphs, not their edge).
fn glyphs(h: &Harness, family: Family, points: f32, ch: char) -> Vec<Rect> {
    let face = h.ui.art.font(family, points).expect("the face");
    let g = *face.font.glyph(ch).expect("the face has the character");
    let page = h.ui.art.font_page(&face, g.page);
    let src = Rect::new(
        f32::from(g.x),
        f32::from(g.y),
        f32::from(g.w),
        f32::from(g.h),
    );
    h.list
        .quads
        .iter()
        .filter(|q| q.tex == page && q.src == src && q.colour == 0xFFFF_FFFF)
        .map(|q| q.dst)
        .collect()
}

/// The health gauge's number, as its glyphs in white (the bar-number face's at whatever size),
/// and the right edge of the gauge's title.
fn health_number_and_title(h: &Harness) -> (Vec<Rect>, f32) {
    let pages: Vec<_> = [8.0, 14.0, 20.0]
        .into_iter()
        .filter_map(|points| {
            let face = h.ui.art.font(Family::BarNumbers, points)?;
            let g = *face.font.glyph('0')?;
            h.ui.art.font_page(&face, g.page)
        })
        .collect();
    let [title] = glyphs(h, Family::Numerals, 12.0, 'H')[..] else {
        panic!("one H in the titles")
    };
    // The gauge is 160 wide, its title 6 in; a number drawn large may start left of it.
    let gauge = Rect::new(title.x - 206.0, title.y - 40.0, 360.0, 80.0);
    let number = h
        .list
        .quads
        .iter()
        .filter(|q| {
            q.colour == 0xFFFF_FFFF
                && q.tex.is_some_and(|t| pages.contains(&t))
                && gauge.contains(q.dst.x, q.dst.y)
        })
        .map(|q| q.dst)
        .collect();
    // The last letter of "Health", the only small h on the bar.
    let end = glyphs(h, Family::Numerals, 12.0, 'h')
        .into_iter()
        .filter(|r| gauge.contains(r.x, r.y))
        .map(|r| r.right())
        .fold(f32::MIN, f32::max);
    (number, end)
}

/// Ticked, which it is at first, the parameter bar's numbers are drawn large, at 20 points, as
/// they always were; unticked, at 14 points, each standing on its title's baseline; ticked
/// again, the frame is the first one, at once.
///
/// Behaviour: hud.status-bars.the-numbers-are-large-or-stand-on-the-titles-line
#[test]
fn large_status_bar_numbers_unticked_draws_them_small_on_the_title_s_baseline() {
    store::init();
    let state = in_world(80);
    let mut h = Harness::with_built_in_art();
    let large = settled(&mut h, &state);
    let eight = |h: &Harness, points| glyphs(h, Family::BarNumbers, points, '8');
    let [big] = eight(&h, 20.0)[..] else {
        panic!("one 8 at 20 points: {:?}", eight(&h, 20.0))
    };
    assert!(eight(&h, 14.0).is_empty());
    let [title] = glyphs(&h, Family::Numerals, 12.0, 'H')[..] else {
        panic!("one H in the titles")
    };

    assert!(store::set_value(
        status_bars::LARGE_STATUS_BAR_NUMBERS,
        PrefValue::Bool(false)
    ));
    settled(&mut h, &state);
    assert!(eight(&h, 20.0).is_empty());
    let [small] = eight(&h, 14.0)[..] else {
        panic!("one 8 at 14 points: {:?}", eight(&h, 14.0))
    };
    assert!(small.h < big.h, "{small:?} smaller than {big:?}");
    assert!(
        (small.bottom() - title.bottom()).abs() < 0.5,
        "{small:?} stands on the line of {title:?}"
    );
    assert!(
        (big.bottom() - title.bottom()).abs() >= 0.5,
        "the large {big:?} does not"
    );

    assert!(store::set_value(
        status_bars::LARGE_STATUS_BAR_NUMBERS,
        PrefValue::Bool(true)
    ));
    assert_eq!(settled(&mut h, &state), large);
    store::clear();
}

/// A number too long for the room beside its title shrinks, unticked, and stops short of the
/// title; drawn large, the same number runs over it.
///
/// Behaviour: hud.status-bars.the-numbers-are-large-or-stand-on-the-titles-line
#[test]
fn a_long_number_shrinks_short_of_its_title_unless_drawn_large() {
    store::init();
    let state = in_world(u32::MAX);
    let mut h = Harness::with_built_in_art();
    settled(&mut h, &state);
    let left = |number: &[Rect]| number.iter().map(|r| r.x).fold(f32::MAX, f32::min);
    let (number, title) = health_number_and_title(&h);
    assert_eq!(number.len(), 10, "{number:?}");
    assert!(
        left(&number) < title,
        "large, it runs over the title's end at {title}"
    );

    assert!(store::set_value(
        status_bars::LARGE_STATUS_BAR_NUMBERS,
        PrefValue::Bool(false)
    ));
    settled(&mut h, &state);
    let (number, title) = health_number_and_title(&h);
    assert_eq!(number.len(), 10, "{number:?}");
    assert!(
        left(&number) > title,
        "small, it stops short of the title's end at {title}"
    );
    // Mana's 40 is at 14 points.
    let [.., fourteen] = glyphs(&h, Family::BarNumbers, 14.0, '4')[..] else {
        panic!("mana's 4 at 14 points")
    };
    assert!(
        number.iter().all(|r| r.h < fourteen.h),
        "shrunk below 14 points: {number:?}"
    );
    store::clear();
}
