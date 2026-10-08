//! The information windows: a right-click on a spell (in the spellbook, on the spell bar, or among
//! the effects in force) identifies it, and the vitae and burden lamps open their windows.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_client_contract::combat_mode::MAGIC;
use dereth_horizon::ui::game::{Effect, GameState, Spell};
use dereth_horizon::ui::kit::Drop;
use dereth_horizon::ui::panels::WindowId;

use crate::Harness;

const FLAME_BOLT: u32 = 7;
const STRENGTH_SELF: u32 = 2;

/// The status row's first icon at 1920x1080, with no art: the rightmost, 32 square.
const FIRST_ICON: (f32, f32) = (1628.0 + 16.0, 44.0 + 16.0);

fn caster() -> GameState {
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        combat_mode: MAGIC,
        spells: vec![Spell {
            id: FLAME_BOLT,
            name: "Flame Bolt I".into(),
            school: 1,
            level: 1,
            ac_icon: 0,
            icon_power: 0,
            bitfield: 0,
        }],
        spell_tabs: vec![vec![FLAME_BOLT]],
        ..GameState::default()
    };
    state.world_services.spell_filters = dereth_client_contract::spellbook::DEFAULT_SPELL_FILTERS;
    state
}

fn right_click(h: &mut Harness, state: &GameState, at: (f32, f32)) {
    h.move_to(at.0, at.1);
    h.input.down[1] = true;
    h.input.pressed[1] = true;
    h.frame(state);
    h.input.down[1] = false;
    h.input.released[1] = true;
    h.frame(state);
}

fn click(h: &mut Harness, state: &GameState, at: (f32, f32)) {
    h.move_to(at.0, at.1);
    h.press();
    h.frame(state);
    h.release();
    h.frame(state);
}

#[test]
fn a_right_click_on_a_spell_bar_slot_identifies_its_spell_in_the_spell_window() {
    let state = caster();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let slot =
        h.ui.drops
            .iter()
            .find(|(_, d)| *d == Some(Drop::SpellSlot { tab: 0, index: 0 }))
            .map(|(r, _)| *r)
            .expect("the spell bar's first slot");
    right_click(
        &mut h,
        &state,
        (slot.x + slot.w / 2.0, slot.y + slot.h / 2.0),
    );
    assert!(h.ui.windows.is_open(WindowId::SpellInfo));
    assert_eq!(h.frame(&state).identify_spell, Some(FLAME_BOLT));
}

#[test]
fn a_right_click_on_a_spellbook_row_identifies_it_without_casting_or_choosing_it() {
    let state = caster();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.open(WindowId::Actions, 0.0);
    for _ in 0..12 {
        h.frame(&state);
    }
    let (x, y) =
        h.ui.windows
            .state(WindowId::Actions)
            .pos
            .expect("the window was drawn");
    // The first row of the list, under the tabs and the search box.
    right_click(&mut h, &state, (x + 12.0 + 60.0, y + 46.0 + 74.0 + 20.0));
    assert!(h.ui.windows.is_open(WindowId::SpellInfo));
    let out = h.frame(&state);
    assert_eq!(out.identify_spell, Some(FLAME_BOLT));
    assert_eq!(
        out.examine_spell, None,
        "the spellbook's own choice is left alone"
    );
    assert!(out.requests.is_empty(), "{:?}", out.requests);
}

#[test]
fn a_right_click_on_an_effect_in_its_list_examines_that_spell() {
    let mut state = caster();
    state.effects = vec![Effect {
        spell: STRENGTH_SELF,
        name: "Strength Self I".into(),
        ac_icon: 0,
        harmful: false,
        remaining: Some(600.0),
    }];
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    click(&mut h, &state, FIRST_ICON);
    // The list hangs under its icon, its first row inside the plain frame's margin.
    right_click(&mut h, &state, (1660.0 - 200.0, 76.0 + 20.0 + 8.0 + 14.0));
    assert!(h.ui.windows.is_open(WindowId::SpellInfo));
    assert_eq!(h.frame(&state).identify_spell, Some(STRENGTH_SELF));
}

#[test]
fn the_burden_lamp_shows_only_while_overloaded_and_opens_the_burden_window() {
    let mut state = caster();
    state.burden = Some(0.5);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    click(&mut h, &state, FIRST_ICON);
    assert!(
        !h.ui.windows.is_open(WindowId::Burden),
        "no lamp under a full load"
    );
    state.burden = Some(1.4);
    h.frame(&state);
    click(&mut h, &state, FIRST_ICON);
    assert!(h.ui.windows.is_open(WindowId::Burden));
}

#[test]
fn the_vitae_lamp_shows_while_there_is_a_penalty_and_opens_the_vitae_window() {
    let mut state = caster();
    state.vitae = Some(1.0);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    click(&mut h, &state, FIRST_ICON);
    assert!(!h.ui.windows.is_open(WindowId::Vitae));
    state.vitae = Some(0.9);
    h.frame(&state);
    click(&mut h, &state, FIRST_ICON);
    assert!(h.ui.windows.is_open(WindowId::Vitae));
}
