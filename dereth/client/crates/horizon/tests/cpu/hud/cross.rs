//! The pad's cross hotbars: both halves drawn in gamepad mode where the shortcut bar would be, a
//! trigger's half used by its buttons, the bind-to-hotbar notice binding the next slot used, and
//! RB with a button picking the set.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use dereth_client_contract::UiRequest;
use dereth_horizon::pad::CrossSet;
use dereth_horizon::ui::game::{GameState, Item, Prompt};
use dereth_horizon::ui::hud::cross::{CrossBind, PowerAct};
use dereth_horizon::ui::input::PadHints;
use dereth_primitives::ObjectId;

use crate::Harness;

const SWORD: ObjectId = ObjectId(0x8000_0042);
const FLAME_BOLT: u32 = 0x3E;

fn player() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        pack: vec![Item {
            id: SWORD,
            name: "Sword".into(),
            ..Item::default()
        }],
        ..GameState::default()
    }
}

/// The interface in gamepad mode, one frame drawn.
fn pad_mode(state: &GameState) -> Harness {
    let mut h = Harness::new(Default::default());
    h.input.pad.mode = Some(PadHints::World);
    h.frame(state);
    h
}

#[test]
fn in_gamepad_mode_both_halves_stand_where_the_shortcut_bar_would() {
    let state = player();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    assert!(h.ui.hud.cross.drawn.is_empty(), "keyboard and mouse: none");
    assert!(!h.ui.hud.drop_slots.is_empty(), "the shortcut bar is up");
    let h = pad_mode(&state);
    let slots: Vec<usize> = h.ui.hud.cross.drawn.iter().map(|(_, s)| *s).collect();
    assert_eq!(slots, (0..16).collect::<Vec<_>>(), "two halves of eight");
    assert!(h.ui.hud.drop_slots.is_empty(), "the shortcut bar is hidden");
    let left = h.ui.hud.cross.drawn[..8]
        .iter()
        .map(|(r, _)| r.right())
        .fold(0.0, f32::max);
    let right = h.ui.hud.cross.drawn[8..]
        .iter()
        .map(|(r, _)| r.x)
        .fold(f32::MAX, f32::min);
    assert!(
        left < 972.0 && right > 972.0,
        "the left half left of the middle, the right right"
    );
}

#[test]
fn a_trigger_s_half_uses_its_slots_and_stands_larger() {
    let state = player();
    let mut h = pad_mode(&state);
    h.ui.hud.cross.assign(6, Some(CrossBind::Item(SWORD)));
    h.ui.hud
        .cross
        .assign(8 + 4, Some(CrossBind::Spell(FLAME_BOLT)));
    let before = h.ui.hud.cross.drawn[6].0;
    h.input.pad.set = Some(CrossSet::Left);
    // A, the bottom face button, is the half's seventh place.
    h.input.pad.fired = vec![6];
    assert_eq!(h.frame(&state).requests, vec![UiRequest::Use(SWORD)]);
    let lit = h.ui.hud.cross.drawn[6].0;
    assert!(lit.w > before.w, "the half held up stands larger");
    h.input.pad.set = Some(CrossSet::Right);
    h.input.pad.fired = vec![4];
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::CastSpell {
            spell_id: FLAME_BOLT
        }]
    );
    h.input.pad.fired = vec![0];
    assert!(
        h.frame(&state).requests.is_empty(),
        "an empty slot asks nothing"
    );
}

#[test]
fn a_stance_control_on_a_slot_taps_its_action() {
    let state = player();
    let mut h = pad_mode(&state);
    h.ui.hud
        .cross
        .assign(1, Some(CrossBind::Power(PowerAct::Raise)));
    h.input.pad.set = Some(CrossSet::Left);
    h.input.pad.fired = vec![1];
    let out = h.frame(&state);
    assert_eq!(out.action_taps, vec![PowerAct::Raise.action(false)]);
}

#[test]
fn the_bind_to_hotbar_notice_binds_the_next_slot_used_and_goes() {
    let state = player();
    let mut h = pad_mode(&state);
    h.ui.hud.cross.binding = Some(CrossBind::Item(SWORD));
    h.frame(&state);
    h.input.pad.set = Some(CrossSet::Right);
    h.input.pad.fired = vec![2];
    let out = h.frame(&state);
    assert!(out.requests.is_empty(), "bound, not used");
    assert_eq!(h.ui.hud.cross.slot(8 + 2), Some(CrossBind::Item(SWORD)));
    assert_eq!(h.ui.hud.cross.binding, None);
}

#[test]
fn in_magic_rb_with_a_button_picks_the_bar_and_each_keeps_its_own_slots() {
    let mut state = player();
    state.combat_mode = dereth_client_contract::combat_mode::MAGIC;
    let mut h = pad_mode(&state);
    h.ui.hud.cross.assign(0, Some(CrossBind::Item(SWORD)));
    h.input.pad.pick = Some(2);
    h.frame(&state);
    assert_eq!(h.ui.hud.cross.set, 2, "RB with A: the third");
    assert_eq!(h.ui.hud.cross.slot(0), None, "a bar of its own");
    h.input.pad.pick = Some(0);
    h.frame(&state);
    assert_eq!(h.ui.hud.cross.slot(0), Some(CrossBind::Item(SWORD)));
    // In peace there is one bar, and RB with a button does nothing.
    state.combat_mode = dereth_client_contract::combat_mode::NONCOMBAT;
    h.frame(&state);
    h.input.pad.pick = Some(5);
    h.frame(&state);
    assert_eq!(h.ui.hud.cross.set, 0);
    assert_eq!(h.ui.hud.cross.slot(0), None, "peace's own bar");
}

#[test]
fn a_pressed_with_a_question_on_screen_says_yes_to_it() {
    let mut state = player();
    state.prompts = vec![Prompt {
        id: 7,
        question: true,
        modal: true,
        waiting: 0,
        text: "Really?".into(),
    }];
    let mut h = Harness::new(Default::default());
    h.input.pad.confirm = true;
    assert_eq!(h.frame(&state).answers, vec![(7, true)]);
}

#[test]
fn a_melee_stance_brings_up_its_bar_whose_right_half_fights_and_peace_puts_it_away() {
    let mut state = player();
    let mut h = pad_mode(&state);
    h.ui.hud.cross.assign(0, Some(CrossBind::Item(SWORD)));
    state.combat_mode = 2;
    h.frame(&state);
    assert_eq!(h.ui.hud.cross.slot(0), None, "the melee bar");
    // RT with A: the right half's bottom face button, attack medium.
    h.input.pad.set = Some(CrossSet::Right);
    h.input.pad.fired = vec![6];
    let out = h.frame(&state);
    assert_eq!(out.action_taps, vec![PowerAct::Medium.action(false)]);
    // RT with d-pad right: raise the power.
    h.input.pad.fired = vec![1];
    assert_eq!(
        h.frame(&state).action_taps,
        vec![PowerAct::Raise.action(false)]
    );
    state.combat_mode = 1;
    h.input.pad.set = None;
    h.frame(&state);
    assert_eq!(
        h.ui.hud.cross.slot(0),
        Some(CrossBind::Item(SWORD)),
        "peace's bar again"
    );
}

#[test]
fn a_use_armed_from_a_slot_goes_on_the_next_slot_used_the_character_s_own_included() {
    let mut state = player();
    state.player_id = Some(ObjectId(0x5000_0001));
    let mut h = pad_mode(&state);
    h.ui.hud.cross.assign(0, Some(CrossBind::Item(SWORD)));
    h.ui.hud.cross.assign(1, Some(CrossBind::Myself));
    // Not armed: the kit's slot uses it, which arms it in the game.
    h.input.pad.set = Some(CrossSet::Left);
    h.input.pad.fired = vec![0];
    assert_eq!(h.frame(&state).requests, vec![UiRequest::Use(SWORD)]);
    // Armed: the character's slot takes it.
    state.targeting = true;
    state.armed = Some(SWORD);
    h.input.pad.fired = vec![1];
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::ExecuteTargetItem(ObjectId(0x5000_0001))]
    );
    // Its own slot again puts it down.
    h.input.pad.fired = vec![0];
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::SetTargetMode(
            dereth_client_contract::view::TargetMode::None
        )]
    );
}
