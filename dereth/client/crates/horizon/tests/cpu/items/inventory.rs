//! The inventory's tiles do what the game's item lists do: a drag lands on a slot, a pack or the
//! character, and a double-click uses the item.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use dereth_client_contract::view::DropTarget;
use dereth_client_contract::UiRequest;
use dereth_horizon::draw::Rect;
use dereth_horizon::ui::game::{GameState, Item};
use dereth_horizon::ui::kit::Drop;
use dereth_horizon::ui::panels::WindowId;
use dereth_primitives::ObjectId;

use crate::Harness;

const ME: ObjectId = ObjectId(0x5000_0001);
const POTION: ObjectId = ObjectId(0x8000_0010);
const HELM: ObjectId = ObjectId(0x8000_0011);
const SACK: ObjectId = ObjectId(0x8000_0012);

fn item(id: ObjectId, name: &str) -> Item {
    Item {
        id,
        name: name.into(),
        ac_icon: None,
        underlay: None,
        overlay: None,
        stack: Some(1),
        worn: 0,
        equip_locations: 0,
        attuned: false,
        wcid: 0,
        container: false,
        decoration: None,
        cooldown: None,
    }
}

fn carrying() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        player_id: Some(ME),
        pack: vec![item(POTION, "Potion"), item(HELM, "Helm")],
        side_packs: vec![(item(SACK, "Sack"), Vec::new())],
        ..GameState::default()
    }
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// The rectangle of the last drop zone asking for `target`.
fn zone(h: &Harness, target: impl Fn(&DropTarget) -> bool) -> Rect {
    h.ui.drops
        .iter()
        .rev()
        .find(|(_, t)| matches!(t, Some(Drop::Target(d)) if target(d)))
        .map(|(r, _)| *r)
        .expect("the zone is on screen")
}

fn slot(index: u32) -> impl Fn(&DropTarget) -> bool {
    move |t| matches!(t, DropTarget::ItemListSlot { index: i, container, .. } if *i == index && *container == ME)
}

fn drag(h: &mut Harness, state: &GameState, from: (f32, f32), to: (f32, f32)) -> Vec<UiRequest> {
    h.move_to(from.0, from.1);
    h.press();
    h.frame(state);
    h.move_to(to.0, to.1);
    h.frame(state);
    h.release();
    h.frame(state).requests
}

fn open(windows: &[WindowId]) -> (Harness, GameState) {
    let state = carrying();
    let mut h = Harness::new(Default::default());
    for w in windows {
        h.ui.windows.open(*w, -1.0);
    }
    h.frame(&state);
    h.frame(&state);
    (h, state)
}

#[test]
fn a_drag_onto_another_slot_places_the_item_there() {
    let (mut h, state) = open(&[WindowId::Inventory]);
    let from = centre(zone(&h, slot(0)));
    let to = centre(zone(&h, slot(1)));
    let requests = drag(&mut h, &state, from, to);
    assert!(
        requests.iter().any(|r| matches!(
            r,
            UiRequest::DragDrop { item, target: DropTarget::ItemListSlot { index: 1, under: Some(u), .. } }
                if *item == POTION && *u == HELM
        )),
        "{requests:?}"
    );
}

#[test]
fn a_drag_onto_a_side_packs_tab_puts_the_item_in_that_pack() {
    let (mut h, state) = open(&[WindowId::Inventory]);
    let from = centre(zone(&h, slot(0)));
    let to = centre(zone(&h, |t| *t == DropTarget::Container(SACK)));
    let requests = drag(&mut h, &state, from, to);
    assert!(
        requests.contains(&UiRequest::DragDrop {
            item: POTION,
            target: DropTarget::Container(SACK)
        }),
        "{requests:?}"
    );
}

#[test]
fn what_is_dropped_is_selected() {
    let (mut h, state) = open(&[WindowId::Inventory]);
    let from = centre(zone(&h, slot(0)));
    let to = centre(zone(&h, |t| *t == DropTarget::Container(SACK)));
    let requests = drag(&mut h, &state, from, to);
    assert!(
        requests.contains(&UiRequest::Select(POTION)),
        "{requests:?}"
    );
    // Let go over the world too.
    let (mut h, state) = open(&[WindowId::Inventory]);
    let from = centre(zone(&h, slot(0)));
    let requests = drag(&mut h, &state, from, (300.0, 500.0));
    assert!(
        requests.contains(&UiRequest::DragDrop {
            item: POTION,
            target: DropTarget::World
        }) && requests.contains(&UiRequest::Select(POTION)),
        "{requests:?}"
    );
}

#[test]
fn a_drag_onto_the_character_wears_the_item() {
    let (mut h, state) = open(&[WindowId::Character, WindowId::Inventory]);
    let from = centre(zone(&h, slot(1)));
    let to = centre(zone(&h, |t| matches!(t, DropTarget::EquipCanvas)));
    let requests = drag(&mut h, &state, from, to);
    assert!(
        requests.iter().any(|r| matches!(r, UiRequest::DragDrop { item, target: DropTarget::EquipCanvas } if *item == HELM)),
        "{requests:?}"
    );
}

#[test]
fn a_double_click_on_a_tile_uses_the_item() {
    let (mut h, state) = open(&[WindowId::Inventory]);
    let (x, y) = centre(zone(&h, slot(0)));
    h.move_to(x, y);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    h.press();
    h.input.double = true;
    let out = h.frame(&state);
    assert!(
        out.requests.contains(&UiRequest::Use(POTION)),
        "{:?}",
        out.requests
    );
}

#[test]
fn an_item_dropped_on_the_salvage_window_is_offered_to_it() {
    let (mut h, mut state) = open(&[WindowId::Inventory]);
    state.salvage = Some((ObjectId(0x8000_0099), Vec::new()));
    h.frame(&state);
    h.frame(&state);
    let from = centre(zone(&h, slot(0)));
    let to =
        h.ui.drops
            .iter()
            .rev()
            .find(|(_, d)| *d == Some(Drop::Salvage))
            .map(|(r, _)| centre(*r))
            .expect("the salvage window takes drops");
    h.move_to(from.0, from.1);
    h.press();
    h.frame(&state);
    h.move_to(to.0, to.1);
    h.frame(&state);
    h.release();
    let out = h.frame(&state);
    assert!(
        out.requests.contains(&UiRequest::SalvageList(
            dereth_client_contract::panels::salvage::SalvageAction::Add(POTION)
        )),
        "{:?}",
        out.requests
    );
}

#[test]
fn a_drag_onto_the_shortcut_bar_makes_a_shortcut_in_that_slot() {
    let (mut h, state) = open(&[WindowId::Inventory]);
    let from = centre(zone(&h, slot(0)));
    let to =
        h.ui.hud
            .drop_slots
            .iter()
            .find(|(_, s)| *s == 3)
            .map(|(r, _)| centre(*r))
            .expect("shortcut slot 3 is on screen");
    let requests = drag(&mut h, &state, from, to);
    assert!(
        requests.contains(&UiRequest::DragDrop {
            item: POTION,
            target: DropTarget::ShortcutSlot(3)
        }),
        "{requests:?}"
    );
}

#[test]
fn a_click_on_a_tile_selects_its_item() {
    let (mut h, state) = open(&[WindowId::Inventory]);
    let (x, y) = centre(zone(&h, slot(1)));
    h.move_to(x, y);
    h.press();
    let pressed = h.frame(&state).requests;
    assert!(
        !pressed.contains(&UiRequest::Select(HELM)),
        "the press alone selects nothing: it may be the start of a drag"
    );
    h.release();
    let released = h.frame(&state).requests;
    assert!(released.contains(&UiRequest::Select(HELM)), "{released:?}");
}

#[test]
fn the_grid_shows_whole_rows_above_the_foot_and_scrolls_to_the_rest() {
    let mut state = carrying();
    state.pack_capacity = Some(102);
    state.pack = (0..102)
        .map(|n| item(ObjectId(0x8000_1000 + n), "Gem"))
        .collect();
    let mut h = Harness::new(Default::default());
    h.ui.windows.open(WindowId::Inventory, -1.0);
    h.frame(&state);
    h.frame(&state);
    let slots: Vec<Rect> =
        h.ui.drops
            .iter()
            .filter(|(_, t)| matches!(t, Some(Drop::Target(DropTarget::ItemListSlot { .. }))))
            .map(|(r, _)| *r)
            .collect();
    let lowest = slots.iter().map(|r| r.bottom()).fold(0.0, f32::max);
    let highest = slots.iter().map(|r| r.y).fold(f32::MAX, f32::min);
    let rows_shown = slots.len() / 8;
    assert!(
        rows_shown < 13,
        "a pack of 102 is more than one screen of rows"
    );
    assert_eq!(slots.len() % 8, 0, "only whole rows are shown");
    // The wheel over the grid brings the last row up, and it is drawn whole too.
    h.move_to(centre(slots[0]).0, centre(slots[0]).1);
    for _ in 0..20 {
        h.input.wheel = -1.0;
        h.frame(&state);
    }
    let scrolled: Vec<Rect> = h
        .ui
        .drops
        .iter()
        .filter(|(_, t)| matches!(t, Some(Drop::Target(DropTarget::ItemListSlot { index, .. })) if *index >= 96))
        .map(|(r, _)| *r)
        .collect();
    assert!(!scrolled.is_empty(), "the last row is reached");
    for r in &scrolled {
        assert!(r.y >= highest - 0.5 && r.bottom() <= lowest + 0.5, "{r:?}");
    }
}

#[test]
fn the_burden_is_worded_as_a_percentage() {
    assert_eq!(dereth_horizon::ui::panels::burden_percent(0.5), "50%");
    assert_eq!(dereth_horizon::ui::panels::burden_percent(1.234), "123%");
}

#[test]
fn a_weapon_dropped_on_the_shield_slot_is_asked_for_there_for_the_other_hand() {
    let shield = dereth_rules::slots::loc::SHIELD;
    let (mut h, state) = open(&[WindowId::Character, WindowId::Inventory]);
    let from = centre(zone(&h, slot(0)));
    let to = centre(zone(
        &h,
        |t| matches!(t, DropTarget::EquipLocation { mask, .. } if *mask == shield),
    ));
    let requests = drag(&mut h, &state, from, to);
    assert!(
        requests.iter().any(|r| matches!(
            r,
            UiRequest::DragDrop { item, target: DropTarget::EquipLocation { mask, .. } }
                if *item == POTION && *mask == shield
        )),
        "{requests:?}"
    );
}
