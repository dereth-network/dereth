//! The game keys the interface answers itself: Tab through the monsters by distance, the use key,
//! F1 to F9 for the fellowship, T for the stack splitter and Escape to close, let go of the
//! selection and log out.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_client_contract::UiRequest;
use dereth_horizon::ui::game::{Blip, BlipKind, GameState, Item, Relation, Target};
use dereth_horizon::ui::hud::{
    escape_deselects, escape_logs_out, fellow_in_reach, tab_target, ACTION_CLOSEST_THING,
    ACTION_NEXT_MONSTER, ACTION_PICK_UP, ACTION_PREVIOUS_MONSTER, ACTION_USE,
};
use dereth_primitives::ObjectId;

use crate::Harness;

const ME: ObjectId = ObjectId(0x5000_0001);
const NEAR: ObjectId = ObjectId(0x8000_0001);
const MIDDLE: ObjectId = ObjectId(0x8000_0002);
const FAR: ObjectId = ObjectId(0x8000_0003);
const GROCER: ObjectId = ObjectId(0x7A9B_4024);

fn blip(id: ObjectId, dx: f32, kind: BlipKind) -> Blip {
    Blip {
        id,
        dx,
        dy: 0.0,
        kind,
        colour: 0,
        shape: 0,
        name: String::new(),
    }
}

fn world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        player_id: Some(ME),
        blips: vec![
            blip(FAR, 30.0, BlipKind::Creature),
            blip(NEAR, 3.0, BlipKind::Creature),
            blip(GROCER, 2.0, BlipKind::Npc),
            blip(MIDDLE, 12.0, BlipKind::Creature),
        ],
        radar_range: 75.0,
        ..GameState::default()
    }
}

fn selecting(state: &mut GameState, id: ObjectId, relation: Relation) {
    state.target = Some(Target {
        id,
        name: "It".into(),
        relation,
        ..Target::default()
    });
    for b in &mut state.blips {
        if b.id == id {
            b.kind = BlipKind::Selected;
        }
    }
}

#[test]
fn tab_selects_monsters_nearest_first_and_shift_tab_furthest_first() {
    let mut state = world();
    assert_eq!(
        tab_target(&state, true),
        Some(NEAR),
        "nothing selected: the nearest"
    );
    assert_eq!(tab_target(&state, false), Some(FAR), "Shift: the furthest");
    let mut order = Vec::new();
    for _ in 0..4 {
        let next = tab_target(&state, true).unwrap();
        order.push(next);
        state = world();
        selecting(&mut state, next, Relation::Hostile);
    }
    assert_eq!(
        order,
        [NEAR, MIDDLE, FAR, NEAR],
        "out and round again; never the grocer"
    );
    let mut state = world();
    selecting(&mut state, FAR, Relation::Hostile);
    assert_eq!(
        tab_target(&state, false),
        Some(MIDDLE),
        "Shift: each nearer one"
    );
}

#[test]
fn tab_reaches_no_further_than_the_radar() {
    let mut state = world();
    state.radar_range = 25.0;
    assert_eq!(
        tab_target(&state, false),
        Some(MIDDLE),
        "the furthest within reach"
    );
    state.radar_range = 2.5;
    assert_eq!(tab_target(&state, true), None, "nothing within reach");
}

#[test]
fn tab_and_shift_tab_reach_the_interface_and_select() {
    let state = world();
    let mut h = Harness::new(Default::default());
    h.input.actions.push(ACTION_NEXT_MONSTER);
    assert!(h.frame(&state).requests.contains(&UiRequest::Select(NEAR)));
    h.input.actions.push(ACTION_PREVIOUS_MONSTER);
    assert!(h.frame(&state).requests.contains(&UiRequest::Select(FAR)));
}

#[test]
fn the_use_key_uses_first_picks_up_a_loose_item_and_with_nothing_selected_finds_the_nearest() {
    let use_key = |state: &GameState| {
        let mut h = Harness::new(Default::default());
        h.input.actions.push(ACTION_USE);
        h.frame(state)
    };
    // Nothing selected: the nearest thing.
    assert!(use_key(&world()).actions.contains(&ACTION_CLOSEST_THING));
    // A monster selected: it stays selected, and nothing else is picked.
    let mut state = world();
    selecting(&mut state, NEAR, Relation::Hostile);
    let out = use_key(&state);
    assert!(
        out.actions.is_empty()
            && !out
                .requests
                .iter()
                .any(|r| matches!(r, UiRequest::Use(_) | UiRequest::Select(_))),
        "{out:?}"
    );
    // A door, used where it stands, is used before anything is picked up.
    let mut state = world();
    selecting(&mut state, ObjectId(0x7A9B_0010), Relation::Object);
    state.target_usable_here = true;
    state.target_pickable = true;
    let out = use_key(&state);
    assert!(out
        .requests
        .contains(&UiRequest::Use(ObjectId(0x7A9B_0010))));
    assert!(!out.actions.contains(&ACTION_PICK_UP));
    // A person or a thing: used.
    let mut state = world();
    selecting(&mut state, GROCER, Relation::Npc);
    assert!(use_key(&state).requests.contains(&UiRequest::Use(GROCER)));
    // An item on the ground: picked up.
    let mut state = world();
    selecting(&mut state, ObjectId(0x8000_0050), Relation::Object);
    state.target_pickable = true;
    assert!(use_key(&state).actions.contains(&ACTION_PICK_UP));
    // An item in the corpse open: picked up.
    let mut state = world();
    selecting(&mut state, ObjectId(0x8000_0051), Relation::Object);
    state.loot = Some((
        ObjectId(0x8000_0060),
        "Corpse".into(),
        vec![Item {
            id: ObjectId(0x8000_0051),
            ..Item::default()
        }],
    ));
    assert!(use_key(&state).actions.contains(&ACTION_PICK_UP));
}

#[test]
fn the_function_keys_select_the_fellowship_s_members_within_reach() {
    use dereth_client_contract::view::{FellowEntry, FellowshipView};
    let member = |id| FellowEntry {
        id,
        ..FellowEntry::default()
    };
    let mut state = world();
    state.fellowship_view = Some(FellowshipView {
        members: vec![member(ME), member(MIDDLE), member(ObjectId(0x5000_0099))],
        ..FellowshipView::default()
    });
    assert_eq!(fellow_in_reach(&state, 0), Some(ME));
    assert_eq!(fellow_in_reach(&state, 1), Some(MIDDLE));
    assert_eq!(fellow_in_reach(&state, 2), None, "out of reach");
    assert_eq!(fellow_in_reach(&state, 5), None, "no such member");
    let mut h = Harness::new(Default::default());
    h.input.keys.push(0x71);
    assert!(
        h.frame(&state)
            .requests
            .contains(&UiRequest::Select(MIDDLE)),
        "F2"
    );
}

#[test]
fn escape_asks_to_log_out_only_with_nothing_selected_open_waiting_or_building() {
    let state = world();
    assert!(escape_logs_out(&state, false));
    assert!(!escape_logs_out(&state, true), "a window open");
    let mut selected = world();
    selecting(&mut selected, NEAR, Relation::Hostile);
    assert!(!escape_logs_out(&selected, false), "something selected");
    let targeting = GameState {
        targeting: true,
        ..world()
    };
    assert!(
        !escape_logs_out(&targeting, false),
        "a use waiting for its target"
    );
    let building = GameState {
        power: Some(0.3),
        ..world()
    };
    assert!(!escape_logs_out(&building, false), "a jump building");
    // The key: the log-out question is asked.
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    assert!(h.ui.wants_escape, "the interface keeps Escape");
    h.input.keys.push(dereth_horizon::ui::input::vk::ESCAPE);
    h.frame(&state);
    assert!(h.ui.hud.log_out_asked(), "asked");
}

#[test]
fn escape_is_the_games_while_an_attack_charges_or_repeats_and_lets_go_of_nothing_itself() {
    use dereth_horizon::ui::input::vk::ESCAPE;
    let mut fighting = world();
    selecting(&mut fighting, NEAR, Relation::Hostile);
    fighting.combat_mode = dereth_client_contract::combat_mode::MISSILE;
    fighting.power = Some(0.5);
    assert!(!escape_deselects(&fighting, false), "the attack goes first");
    assert!(!escape_logs_out(&fighting, false));
    let mut h = Harness::new(Default::default());
    h.frame(&fighting);
    assert!(
        !h.ui.wants_escape,
        "the game's Escape, which stops the attack"
    );
    h.input.keys.push(ESCAPE);
    let out = h.frame(&fighting);
    assert!(
        !out.requests.contains(&UiRequest::Select(ObjectId(0))),
        "{:?}",
        out.requests
    );
    assert!(!h.ui.hud.log_out_asked());
}

#[test]
fn escape_closes_the_top_window_then_lets_go_of_the_selection_then_asks_to_log_out() {
    use dereth_horizon::ui::input::vk::ESCAPE;
    use dereth_horizon::ui::panels::WindowId;
    let mut selected = world();
    selecting(&mut selected, NEAR, Relation::Hostile);
    assert!(escape_deselects(&selected, false));
    assert!(!escape_deselects(&selected, true), "a window open");
    assert!(!escape_deselects(&world(), false), "nothing selected");
    let mut h = Harness::new(Default::default());
    h.frame(&selected);
    h.ui.windows.toggle(WindowId::Character, 0.0);
    h.frame(&selected);
    assert!(h.ui.wants_escape, "the interface keeps Escape");
    // A window open: Escape closes it, and the selection stays.
    h.input.keys.push(ESCAPE);
    let closed = h.frame(&selected);
    assert!(
        !h.ui.windows.is_open(WindowId::Character),
        "the window closed"
    );
    assert!(
        !closed.requests.contains(&UiRequest::Select(ObjectId(0))),
        "{:?}",
        closed.requests
    );
    assert!(!h.ui.hud.log_out_asked());
    // Nothing open, something selected: Escape lets go of it.
    assert!(h.ui.wants_escape, "the interface still keeps Escape");
    h.input.keys.push(ESCAPE);
    let let_go = h.frame(&selected);
    assert!(
        let_go.requests.contains(&UiRequest::Select(ObjectId(0))),
        "{:?}",
        let_go.requests
    );
    assert!(!h.ui.hud.log_out_asked(), "not asked yet");
    // Nothing open, nothing selected: Escape asks to log out.
    let nothing = world();
    h.frame(&nothing);
    h.input.keys.push(ESCAPE);
    h.frame(&nothing);
    assert!(h.ui.hud.log_out_asked(), "asked");
}

#[test]
fn t_puts_the_keyboard_in_the_stack_splitter_s_number() {
    let mut state = world();
    selecting(&mut state, ObjectId(0x8000_0070), Relation::Object);
    state.target.as_mut().unwrap().look = Some(Item {
        id: ObjectId(0x8000_0070),
        stack: Some(50),
        ..Item::default()
    });
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    h.input.keys.push(0x54);
    h.frame(&state);
    h.input.chars = vec!['7'];
    let typed = h.frame(&state).requests;
    assert!(
        typed.contains(&UiRequest::StackSliderChanged { split: 7, max: 50 }),
        "{typed:?}"
    );
}

#[test]
fn a_click_on_a_party_list_row_selects_that_member() {
    use dereth_client_contract::view::{FellowEntry, FellowshipView};
    use dereth_horizon::ui::game::Vital;
    const BORIN: ObjectId = ObjectId(0x5000_0002);
    let mut state = world();
    let vital = Vital::default();
    state.fellowship = vec![
        ("Tester".into(), vital, vital, vital, true),
        ("Borin".into(), vital, vital, vital, false),
    ];
    let member = |id, name: &str| FellowEntry {
        id,
        name: name.into(),
        ..FellowEntry::default()
    };
    state.fellowship_view = Some(FellowshipView {
        members: vec![member(ME, "Tester"), member(BORIN, "Borin")],
        ..FellowshipView::default()
    });
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    // The second row, under the player's own.
    h.move_to(100.0, 160.0);
    h.press();
    let mut picked = h.frame(&state).requests;
    h.release();
    picked.extend(h.frame(&state).requests);
    assert!(picked.contains(&UiRequest::Select(BORIN)), "{picked:?}");
}

#[test]
fn yes_on_a_question_over_a_window_answers_it_whatever_lies_under_it() {
    use dereth_horizon::ui::game::Prompt;
    use dereth_horizon::ui::panels::WindowId;
    let mut state = world();
    state.prompts = vec![Prompt {
        id: 7,
        text: "Borin Stonefist wants you to join the fellowship. Will you?".into(),
        question: true,
        modal: false,
        waiting: 0,
    }];
    let yes_at = |windows: &[WindowId], y: f32| {
        let mut h = Harness::new(Default::default());
        for w in windows {
            h.ui.windows.open(*w, -1.0);
        }
        h.frame(&state);
        h.move_to(895.0, y);
        h.press();
        h.frame(&state).answers.contains(&(7, true))
    };
    // Where Yes is, with nothing under the box.
    #[allow(clippy::cast_precision_loss)]
    let y = (300..800)
        .step_by(4)
        .map(|y| y as f32)
        .find(|y| yes_at(&[], *y))
        .expect("the box's Yes");
    // The same press with the windows that sit under the middle of the screen open.
    for w in [
        WindowId::Social,
        WindowId::Character,
        WindowId::Actions,
        WindowId::Map,
    ] {
        assert!(yes_at(&[w], y), "Yes over {w:?}");
    }
}
