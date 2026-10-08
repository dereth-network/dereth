//! The Spellbook window and the spell bar: what a click, a drag or a switch there asks the game
//! for, and when the spell bar is up.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_client_contract::combat_mode::{MAGIC, MELEE};
use dereth_client_contract::view::SpellExamineView;
use dereth_client_contract::UiRequest;
use dereth_horizon::draw::Rect;
use dereth_horizon::ui::game::{GameState, Spell};
use dereth_horizon::ui::kit::Drop;
use dereth_horizon::ui::panels::WindowId;

use crate::Harness;

const FLAME_BOLT: u32 = 7;

fn caster(combat_mode: u32) -> GameState {
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        combat_mode,
        spells: vec![Spell {
            id: FLAME_BOLT,
            name: "Flame Bolt I".into(),
            school: 1,
            level: 1,
            ac_icon: 0,
            icon_power: 0,
            bitfield: 0,
        }],
        spell_tabs: vec![vec![8, 9]],
        spell_detail: Some((
            FLAME_BOLT,
            SpellExamineView {
                name: "Flame Bolt I".into(),
                school: 1,
                ..SpellExamineView::default()
            },
        )),
        ..GameState::default()
    };
    state.world_services.spell_filters = dereth_client_contract::spellbook::DEFAULT_SPELL_FILTERS;
    state
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// The Spellbook window's body, as it was last laid out (the interface at scale 1).
fn body(h: &Harness) -> Rect {
    let (x, y) =
        h.ui.windows
            .state(WindowId::Actions)
            .pos
            .expect("the window was drawn");
    let ((w, wh), _) = WindowId::Actions.geometry();
    Rect::new(x + 12.0, y + 46.0, w - 24.0, wh - 56.0)
}

/// The spell list's area, under the tabs and the search box.
fn list(h: &Harness) -> Rect {
    let b = body(h);
    Rect::new(b.x, b.y + 74.0, b.w * 0.47, b.h - 120.0)
}

/// The spellbook open on the War tab with its spell chosen.
fn open_on_flame_bolt(state: &GameState) -> Harness {
    let mut h = Harness::new(Default::default());
    h.frame(state);
    h.ui.windows.open(WindowId::Actions, 0.0);
    for _ in 0..12 {
        h.frame(state);
    }
    let l = list(&h);
    h.move_to(l.x + 60.0, l.y + 20.0);
    h.press();
    h.frame(state);
    h.release();
    h.frame(state);
    h
}

fn click(h: &mut Harness, state: &GameState, at: (f32, f32)) -> Vec<UiRequest> {
    h.move_to(at.0, at.1);
    h.press();
    let mut requests = h.frame(state).requests;
    h.release();
    requests.extend(h.frame(state).requests);
    requests
}

#[test]
fn the_spell_bar_is_up_only_while_the_character_fights_with_magic() {
    let spell_slots = |state: &GameState| {
        let mut h = Harness::new(Default::default());
        h.frame(state);
        h.ui.drops
            .iter()
            .filter(|(_, d)| matches!(d, Some(Drop::SpellSlot { .. })))
            .count()
    };
    assert_eq!(spell_slots(&caster(MELEE)), 0);
    assert_eq!(spell_slots(&caster(MAGIC)), 12);
}

#[test]
fn a_level_switch_hides_that_level_through_the_game_s_filter_and_shows_it_again() {
    use dereth_client_contract::spellbook::{level_mask, DEFAULT_SPELL_FILTERS};
    let mut state = caster(MAGIC);
    let mut h = open_on_flame_bolt(&state);
    let l = list(&h);
    // Level I's label, the first switch under the list.
    let level_one = (l.x + 50.0, l.bottom() + 16.0);
    let off = DEFAULT_SPELL_FILTERS & !level_mask(1);
    assert!(click(&mut h, &state, level_one).contains(&UiRequest::SetSpellbookFilter { mask: off }));
    state.world_services.spell_filters = off;
    assert!(
        click(&mut h, &state, level_one).contains(&UiRequest::SetSpellbookFilter {
            mask: DEFAULT_SPELL_FILTERS
        })
    );
}

#[test]
fn add_to_bar_puts_the_chosen_spell_at_the_end_of_the_current_spell_bar() {
    let state = caster(MAGIC);
    let mut h = open_on_flame_bolt(&state);
    let l = list(&h);
    let b = body(&h);
    let pane = Rect::new(l.right() + 12.0, l.y, b.right() - l.right() - 12.0, l.h);
    // The details keep ten from the pane's sides and eight from its foot, with no art.
    let x = pane.x + 10.0;
    let bw = (pane.w - 20.0 - 8.0) / 2.0;
    let add = Rect::new(x, pane.bottom() - 36.0, bw, 28.0);
    assert!(
        click(&mut h, &state, centre(add)).contains(&UiRequest::AddSpellFavorite {
            spell_id: FLAME_BOLT,
            index: 2,
            tab: 0
        })
    );
}

/// The spell-bar requests among `requests`.
fn favourites(requests: Vec<UiRequest>) -> Vec<UiRequest> {
    requests
        .into_iter()
        .filter(|r| {
            matches!(
                r,
                UiRequest::AddSpellFavorite { .. } | UiRequest::RemoveSpellFavorite { .. }
            )
        })
        .collect()
}

#[test]
fn add_to_bar_moves_a_spell_the_tab_already_holds_to_its_end_instead_of_adding_it_twice() {
    let mut state = caster(MAGIC);
    state.spell_tabs = vec![vec![FLAME_BOLT, 8, 9]];
    let mut h = open_on_flame_bolt(&state);
    let l = list(&h);
    let b = body(&h);
    let pane = Rect::new(l.right() + 12.0, l.y, b.right() - l.right() - 12.0, l.h);
    let x = pane.x + 10.0;
    let bw = (pane.w - 20.0 - 8.0) / 2.0;
    let add = Rect::new(x, pane.bottom() - 36.0, bw, 28.0);
    assert_eq!(
        favourites(click(&mut h, &state, centre(add))),
        vec![
            UiRequest::RemoveSpellFavorite {
                spell_id: FLAME_BOLT,
                tab: 0
            },
            UiRequest::AddSpellFavorite {
                spell_id: FLAME_BOLT,
                index: 2,
                tab: 0
            },
        ]
    );
}

#[test]
fn a_spell_dragged_from_the_spellbook_onto_a_tab_that_holds_it_moves_it_to_that_slot() {
    let mut state = caster(MAGIC);
    state.spell_tabs = vec![vec![8, 9, FLAME_BOLT]];
    let mut h = open_on_flame_bolt(&state);
    let target =
        h.ui.drops
            .iter()
            .find(|(_, d)| *d == Some(Drop::SpellSlot { tab: 0, index: 0 }))
            .map(|(r, _)| *r)
            .expect("the spell bar's first slot");
    let l = list(&h);
    h.move_to(l.x + 60.0, l.y + 20.0);
    h.press();
    h.frame(&state);
    let (tx, ty) = centre(target);
    h.move_to(tx, ty);
    h.frame(&state);
    h.release();
    let dropped = h.frame(&state);
    assert_eq!(
        favourites(dropped.requests),
        vec![
            UiRequest::RemoveSpellFavorite {
                spell_id: FLAME_BOLT,
                tab: 0
            },
            UiRequest::AddSpellFavorite {
                spell_id: FLAME_BOLT,
                index: 0,
                tab: 0
            },
        ]
    );
}

#[test]
fn forget_asks_first_and_a_yes_removes_the_spell() {
    let state = caster(MAGIC);
    let mut h = open_on_flame_bolt(&state);
    let l = list(&h);
    let b = body(&h);
    let pane = Rect::new(l.right() + 12.0, l.y, b.right() - l.right() - 12.0, l.h);
    // The details keep ten from the pane's sides and eight from its foot, with no art.
    let x = pane.x + 10.0;
    let bw = (pane.w - 20.0 - 8.0) / 2.0;
    let forget = Rect::new(x + bw + 8.0, pane.bottom() - 36.0, bw, 28.0);
    let asked = click(&mut h, &state, centre(forget));
    assert!(
        !asked
            .iter()
            .any(|r| matches!(r, UiRequest::RemoveSpell { .. })),
        "nothing is forgotten before the answer: {asked:?}"
    );
    h.input.keys = vec![dereth_horizon::ui::input::vk::ENTER];
    let answered = h.frame(&state);
    assert!(answered.requests.contains(&UiRequest::RemoveSpell {
        spell_id: FLAME_BOLT
    }));
}

#[test]
fn a_spell_dragged_from_the_spellbook_onto_the_spell_bar_lands_in_that_slot() {
    let state = caster(MAGIC);
    let mut h = open_on_flame_bolt(&state);
    let target =
        h.ui.drops
            .iter()
            .find(|(_, d)| *d == Some(Drop::SpellSlot { tab: 0, index: 1 }))
            .map(|(r, _)| *r)
            .expect("the spell bar's second slot");
    let l = list(&h);
    h.move_to(l.x + 60.0, l.y + 20.0);
    h.press();
    h.frame(&state);
    let (tx, ty) = centre(target);
    h.move_to(tx, ty);
    h.frame(&state);
    h.release();
    let dropped = h.frame(&state);
    assert!(dropped.requests.contains(&UiRequest::AddSpellFavorite {
        spell_id: FLAME_BOLT,
        index: 1,
        tab: 0
    }));
}

#[test]
fn a_click_on_a_spell_in_the_spell_bar_casts_it_when_the_button_comes_up() {
    let state = caster(MAGIC);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let slot =
        h.ui.drops
            .iter()
            .find(|(_, d)| *d == Some(Drop::SpellSlot { tab: 0, index: 0 }))
            .map(|(r, _)| *r)
            .expect("the spell bar's first slot");
    assert!(click(&mut h, &state, centre(slot)).contains(&UiRequest::CastSpell { spell_id: 8 }));
}

const LEAD_SCARAB: u32 = 691;

/// The spellbook open on its Components tab, one component listed, ten of it wanted.
fn open_on_components() -> (Harness, GameState) {
    use dereth_client_contract::view::{ComponentCategory, ComponentRow};
    let mut state = caster(MAGIC);
    state.world_services.era_ui.void_magic = true;
    state.world_services.features.spell_research = false;
    state.components = vec![ComponentCategory {
        category: 0,
        rows: vec![ComponentRow {
            wcid: LEAD_SCARAB,
            name: "Lead Scarab".into(),
            icon: None,
            owned: 3,
            desired: 10,
            object: None,
        }],
    }];
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.open(WindowId::Actions, 0.0);
    for _ in 0..12 {
        h.frame(&state);
    }
    // The sixth tab (after the five schools), at the tabs' least width with no fonts.
    let b = body(&h);
    click(&mut h, &state, (b.x + 66.0 * 5.0 + 35.0, b.y + 13.0));
    (h, state)
}

/// The first component's wanted box and its up arrow, with no art.
fn wanted(h: &Harness) -> (Rect, Rect) {
    let b = body(h);
    let right = b.right() - 10.0;
    let y = b.y + 34.0;
    let up = Rect::new(right - 24.0 - 26.0, y + 8.0, 24.0, 24.0);
    (Rect::new(up.x - 68.0, y + 6.0, 64.0, 28.0), up)
}

fn sets(requests: &[UiRequest], level: i32) -> bool {
    requests.contains(&UiRequest::SetDesiredComponentLevel {
        wcid: LEAD_SCARAB,
        level,
    })
}

fn sets_any(requests: &[UiRequest]) -> bool {
    requests
        .iter()
        .any(|r| matches!(r, UiRequest::SetDesiredComponentLevel { .. }))
}

#[test]
fn a_wanted_count_typed_into_a_component_s_box_is_set_on_enter() {
    use dereth_horizon::ui::input::vk;
    let (mut h, state) = open_on_components();
    let (field, _) = wanted(&h);
    assert!(!sets_any(&click(&mut h, &state, centre(field))));
    h.input.chars = "37".chars().collect();
    assert!(
        !sets_any(&h.frame(&state).requests),
        "nothing is set while typing"
    );
    h.input.keys = vec![vk::ENTER];
    assert!(sets(&h.frame(&state).requests, 37));
}

#[test]
fn a_typed_count_is_set_when_its_box_lets_go_and_dropped_on_escape() {
    use dereth_horizon::ui::input::vk;
    let (mut h, state) = open_on_components();
    let (field, _) = wanted(&h);
    click(&mut h, &state, centre(field));
    h.input.chars = "5".chars().collect();
    h.frame(&state);
    h.input.keys = vec![vk::ESCAPE];
    assert!(!sets_any(&h.frame(&state).requests), "Escape drops it");
    click(&mut h, &state, centre(field));
    h.input.chars = "9999".chars().collect();
    h.frame(&state);
    let b = body(&h);
    let away = click(&mut h, &state, (b.x + 100.0, b.bottom() - 20.0));
    assert!(
        sets(&away, 5000),
        "held to the most the game allows: {away:?}"
    );
}

#[test]
fn a_component_s_up_arrow_wants_one_more() {
    let (mut h, state) = open_on_components();
    let (_, up) = wanted(&h);
    assert!(sets(&click(&mut h, &state, centre(up)), 11));
}

#[test]
fn a_count_being_typed_is_set_when_the_spellbook_turns_to_another_tab() {
    let (mut h, state) = open_on_components();
    let (field, _) = wanted(&h);
    click(&mut h, &state, centre(field));
    h.input.chars = "42".chars().collect();
    h.frame(&state);
    let b = body(&h);
    // The War tab.
    let turned = click(&mut h, &state, (b.x + 35.0, b.y + 13.0));
    assert!(sets(&turned, 42), "{turned:?}");
}
