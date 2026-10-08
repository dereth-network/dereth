//! The first hotbar holds the shortcut bar: a click uses a shortcut, a drag moves it, and a drag
//! that lands nowhere leaves it removed.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use dereth_client_contract::view::DropTarget;
use dereth_client_contract::UiRequest;
use dereth_horizon::draw::Rect;
use dereth_horizon::ui::game::{GameState, Shortcut};
use dereth_primitives::ObjectId;

use crate::Harness;

const SWORD: ObjectId = ObjectId(0x8000_0042);

fn player_with_a_shortcut_in_slot(slot: u32) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        shortcuts: vec![Shortcut {
            index: slot,
            name: "Sword".into(),
            object: Some(SWORD),
            spell: None,
            ac_icon: None,
            cooldown: None,
            look: None,
        }],
        ..GameState::default()
    }
}

/// The screen rectangle of shortcut slot `slot`, as the HUD last laid it out.
fn slot_rect(h: &Harness, slot: u32) -> Rect {
    h.ui.hud
        .drop_slots
        .iter()
        .find(|(_, s)| *s == slot)
        .map(|(r, _)| *r)
        .expect("the slot is on screen")
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

#[test]
fn a_click_on_a_shortcut_selects_the_item_and_a_double_click_uses_it() {
    let state = player_with_a_shortcut_in_slot(2);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let (x, y) = centre(slot_rect(&h, 2));
    h.move_to(x, y);
    h.press();
    let down = h.frame(&state);
    assert!(
        down.requests.is_empty(),
        "nothing is asked on the press: {:?}",
        down.requests
    );
    h.release();
    let up = h.frame(&state);
    assert_eq!(up.requests, vec![UiRequest::Select(SWORD)]);
    // The second press of a double-click uses it.
    h.press();
    h.input.double = true;
    let used = h.frame(&state);
    assert_eq!(used.requests, vec![UiRequest::Use(SWORD)]);
    h.release();
    assert!(
        h.frame(&state).requests.is_empty(),
        "the release asks nothing more"
    );
}

#[test]
fn dragging_a_shortcut_to_another_slot_removes_it_then_places_it_there() {
    let state = player_with_a_shortcut_in_slot(2);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let (x, y) = centre(slot_rect(&h, 2));
    let (tx, ty) = centre(slot_rect(&h, 7));
    h.move_to(x, y);
    h.press();
    h.frame(&state);
    h.move_to(x + 20.0, y);
    let picked = h.frame(&state);
    assert_eq!(picked.requests, vec![UiRequest::RemoveShortcut(SWORD)]);
    h.move_to(tx, ty);
    h.frame(&state);
    h.release();
    let dropped = h.frame(&state);
    assert_eq!(
        dropped.requests,
        vec![UiRequest::DragDrop {
            item: SWORD,
            target: DropTarget::ShortcutAlias { slot: 7, from: 2 },
        }]
    );
}

#[test]
fn a_shortcut_dragged_off_the_bar_stays_removed() {
    let state = player_with_a_shortcut_in_slot(0);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let (x, y) = centre(slot_rect(&h, 0));
    h.move_to(x, y);
    h.press();
    h.frame(&state);
    h.move_to(900.0, 400.0);
    let picked = h.frame(&state);
    assert_eq!(picked.requests, vec![UiRequest::RemoveShortcut(SWORD)]);
    h.release();
    let dropped = h.frame(&state);
    assert!(dropped.requests.is_empty(), "{:?}", dropped.requests);
}

#[test]
fn a_shortcut_key_uses_the_item_in_its_slot_and_its_control_form_selects_it() {
    let state = player_with_a_shortcut_in_slot(2);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    // The third slot's key, then the same slot's secondary key.
    h.input.actions.push(0x1000_0044);
    let used = h.frame(&state);
    assert_eq!(used.requests, vec![UiRequest::Use(SWORD)]);
    h.input.actions.push(0x1000_0050);
    let selected = h.frame(&state);
    assert_eq!(selected.requests, vec![UiRequest::Select(SWORD)]);
    // An empty slot's key asks for nothing.
    h.input.actions.push(0x1000_0042);
    assert!(h.frame(&state).requests.is_empty());
}

#[test]
fn the_log_out_key_asks_first_and_logs_out_only_on_yes() {
    let state = player_with_a_shortcut_in_slot(0);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.input.actions.push(0x1000_0026);
    let asked = h.frame(&state);
    assert!(!asked.log_off, "the key only asks");
    // Enter answers the question's first button, Yes.
    h.input.keys.push(0x0D);
    let answered = h.frame(&state);
    assert!(answered.log_off);
}

#[test]
fn a_spell_dragged_along_the_spell_bar_leaves_its_slot_and_lands_one_earlier() {
    use dereth_horizon::ui::game::Spell;
    use dereth_horizon::ui::kit::Drop;
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        spells: vec![Spell {
            id: 7,
            name: "Flame Bolt I".into(),
            school: 1,
            level: 1,
            ac_icon: 0,
            icon_power: 0,
            bitfield: 0,
        }],
        spell_tabs: vec![vec![7, 8, 9]],
        combat_mode: dereth_client_contract::combat_mode::MAGIC,
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let slot = |h: &Harness, i: usize| {
        h.ui.drops
            .iter()
            .find(|(_, d)| *d == Some(Drop::SpellSlot { tab: 0, index: i }))
            .map(|(r, _)| *r)
            .expect("the spell slot is on screen")
    };
    let (fx, fy) = centre(slot(&h, 0));
    let (tx, ty) = centre(slot(&h, 3));
    h.move_to(fx, fy);
    h.press();
    h.frame(&state);
    h.move_to(tx, ty);
    let moved = h.frame(&state);
    assert_eq!(
        moved.requests,
        vec![UiRequest::RemoveSpellFavorite {
            spell_id: 7,
            tab: 0
        }]
    );
    h.release();
    let dropped = h.frame(&state);
    assert_eq!(
        dropped.requests,
        vec![UiRequest::AddSpellFavorite {
            spell_id: 7,
            index: 2,
            tab: 0
        }]
    );
}

#[test]
fn a_spell_dragged_along_the_spell_bar_lands_in_the_slot_it_is_dropped_on_once_the_bar_has_closed_up(
) {
    use dereth_horizon::ui::kit::Drop;
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        spell_tabs: vec![vec![7, 8, 9]],
        combat_mode: dereth_client_contract::combat_mode::MAGIC,
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let slot = |h: &Harness, i: usize| {
        h.ui.drops
            .iter()
            .find(|(_, d)| *d == Some(Drop::SpellSlot { tab: 0, index: i }))
            .map(|(r, _)| *r)
            .expect("the spell slot is on screen")
    };
    let (fx, fy) = centre(slot(&h, 0));
    h.move_to(fx, fy);
    h.press();
    h.frame(&state);
    let (tx, ty) = centre(slot(&h, 2));
    h.move_to(tx, ty);
    h.frame(&state);
    // The game has taken the spell off: the bar now reads 8, 9, and the pointer is over the
    // second slot, after 8.
    state.spell_tabs = vec![vec![8, 9]];
    h.frame(&state);
    let (tx, ty) = centre(slot(&h, 1));
    h.move_to(tx, ty);
    h.frame(&state);
    h.release();
    let dropped = h.frame(&state);
    assert_eq!(
        dropped.requests,
        vec![UiRequest::AddSpellFavorite {
            spell_id: 7,
            index: 1,
            tab: 0
        }]
    );
}

#[test]
fn a_click_on_a_radar_blip_selects_it() {
    use dereth_horizon::ui::game::{Blip, BlipKind};
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        blips: vec![Blip {
            id: ObjectId(0x8000_0077),
            dx: 0.0,
            dy: 20.0,
            kind: BlipKind::Npc,
            colour: 0,
            shape: 4,
            name: "Alcott".into(),
        }],
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    // The radar sits at the top right: a 196-pixel disc 26 from the right edge and 44 from the
    // top, its blips out to 82 pixels for 75 metres.
    let centre = (1920.0 - 26.0 - 98.0, 44.0 + 98.0);
    h.move_to(centre.0, centre.1 - 20.0 / 75.0 * 82.0);
    h.press();
    let out = h.frame(&state);
    assert!(
        out.requests
            .contains(&UiRequest::Select(ObjectId(0x8000_0077))),
        "{:?}",
        out.requests
    );
}

#[test]
fn a_spell_tab_longer_than_the_bar_is_paged_and_the_slot_keys_cast_from_the_page_shown() {
    use dereth_horizon::ui::kit::Drop;
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        spell_tabs: vec![(100..120).collect()],
        combat_mode: dereth_client_contract::combat_mode::MAGIC,
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let slot = |h: &Harness, i: usize| {
        h.ui.drops
            .iter()
            .find(|(_, d)| *d == Some(Drop::SpellSlot { tab: 0, index: i }))
            .map(|(r, _)| *r)
    };
    assert!(
        slot(&h, 11).is_some() && slot(&h, 12).is_none(),
        "the first page"
    );
    // The next-page arrow stands just past the bar's last slot, at its foot.
    let last = slot(&h, 11).unwrap();
    h.move_to(last.right() + 12.0, last.bottom() - 9.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    assert!(
        slot(&h, 12).is_some() && slot(&h, 11).is_none(),
        "the second page"
    );
    // The first slot's key casts the page's first spell, the tab's thirteenth.
    h.input
        .magic
        .push(dereth_client_contract::view::MagicNotice::CastQuickslotSpell { slot: 0 });
    let cast = h.frame(&state);
    assert_eq!(cast.requests, vec![UiRequest::CastSpell { spell_id: 112 }]);
}

#[test]
fn a_full_page_of_spells_offers_an_empty_next_page_to_drop_a_new_spell_onto() {
    use dereth_horizon::ui::kit::Drop;
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        spell_tabs: vec![(100..112).collect()],
        combat_mode: dereth_client_contract::combat_mode::MAGIC,
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let slot = |h: &Harness, i: usize| {
        h.ui.drops
            .iter()
            .find(|(_, d)| *d == Some(Drop::SpellSlot { tab: 0, index: i }))
            .map(|(r, _)| *r)
    };
    let last = slot(&h, 11).expect("the first page");
    h.move_to(last.right() + 12.0, last.bottom() - 9.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    assert!(
        slot(&h, 12).is_some(),
        "the empty second page takes a drop in its first slot"
    );
}
