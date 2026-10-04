//! A spell dragged from the book or along the bar lights the bar row under the pointer (green even
//! when already held), the hint follows and drops behind the pointer, the landing row loses it;
//! each of eight tabs has its own hinted list.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

#![allow(clippy::pedantic)]

use crate::common::layout::RegistrationOrder;
use dereth_primitives::{DataId, LocalTime};
use dereth_ui::{ElemHandle, ElementId, Screen, StateId, UiSystem};
use dereth_ui_screens::items::widget::drag_accept_state;
use dereth_ui_screens::panels::remaining::RemainingPanels;
use dereth_ui_screens::view::{GameView, SpellEntry};

#[derive(Debug)]
struct View {
    spells: Vec<SpellEntry>,
    tabs: [Vec<u32>; 8],
}
impl GameView for View {
    fn spellbook(&self) -> &[SpellEntry] {
        &self.spells
    }
    fn spell_tab(&self, tab: usize) -> &[u32] {
        &self.tabs[tab]
    }
    fn spell_filters(&self) -> u32 {
        0x3FFF
    }
}

const LISTENER: u32 = 0xF115;

fn env() -> (UiSystem, RemainingPanels, View) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut screen = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the shipped gameplay screen");
    let root = screen.root().unwrap();
    let mut panels = RemainingPanels::default();
    panels.post_init(&mut ui, root);
    let view = View {
        spells: (1..=40)
            .map(|id| SpellEntry {
                id,
                name: format!("Spell {id}"),
                icon: Some(DataId(0x0600_13A5)),
                school: 4,
                level: 1,
                icon_power: 1,
                display_order: i32::try_from(id).expect("a small id"),
                bitfield: 0,
            })
            .collect(),
        tabs: std::array::from_fn(|_| Vec::new()),
    };
    // The settling frame before the clear -- see `spell_bar_transfer::env` for why
    // `AllegiancePanel`'s one-time `SetTalkFocusEnabled` triple has to be run off first.
    panels.update(&mut ui, &view);
    ui.requests.clear();
    ui.notice_inbox.clear();
    ui.register_for_element_messages(root, dereth_ui::ListenerId::External(LISTENER));
    ui.drain_outbox();
    (ui, panels, view)
}

fn deliver(ui: &mut UiSystem, panels: &mut RemainingPanels, view: &View) {
    for d in ui.drain_outbox() {
        if let dereth_ui::Delivery::Element {
            to: dereth_ui::ListenerId::External(LISTENER),
            msg,
        } = d
        {
            panels.on_element_message(ui, &msg, view);
        }
    }
}

/// The shipped tree hides the pages this screen is not showing, and the panel a player is
/// dragging out of is the one they last clicked. Both are what a button press would have done.
fn front(ui: &mut UiSystem, h: ElemHandle) {
    let mut cur = h;
    loop {
        ui.set_visible(cur, true);
        let Some(parent) = ui.parent(cur) else { break };
        cur = parent;
    }
    let mut chain = vec![h];
    while let Some(parent) = ui.parent(*chain.last().unwrap()) {
        chain.push(parent);
    }
    for h in chain.into_iter().rev() {
        ui.bring_to_front(h);
    }
}

fn centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let b = ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// The drag-accept icon as the panel last wrote it **and** as the live element carries it.
fn hint(
    ui: &UiSystem,
    panels: &RemainingPanels,
    tab: usize,
    row: usize,
) -> (Option<StateId>, Option<StateId>) {
    let s = &panels.spellcasting.lists[tab]
        .as_ref()
        .expect("the spell item list binds")
        .slots[row];
    (
        s.drag_accept_state,
        s.drag_accept.and_then(|h| ui.node(h)).map(|n| n.state),
    )
}

/// The spellbook row showing `spell`, brought to the front and hit-tested.
fn spellbook_point(ui: &mut UiSystem, panels: &RemainingPanels, spell: u32) -> (i32, i32) {
    let w = panels
        .spellbook
        .list
        .as_ref()
        .expect("the spell list binds");
    let i = w
        .slots
        .iter()
        .position(|s| s.spell == Some(spell))
        .expect("spell is on show");
    let row = w.slots[i].handle;
    front(ui, row);
    let p = centre(ui, row);
    let hit = ui
        .hit_test_screen(p.0, p.1)
        .expect("the real spellbook row is hit-testable");
    assert!(
        hit == row || ui.is_ancestor_of(row, hit),
        "the pointer must land on the book's row"
    );
    p
}

/// Press and cross the 16-px threshold, so shared element handling refuses the generic drag (`0x21`)
/// and starts the list's own with a real proxy.
fn pick_up(ui: &mut UiSystem, panels: &mut RemainingPanels, view: &View, p: (i32, i32)) {
    ui.mouse_down(7, p.0, p.1);
    deliver(ui, panels, view);
    ui.mouse_move(LocalTime(0.0), p.0 + 20, p.1 + 20);
    deliver(ui, panels, view);
    assert!(
        ui.drag_state().element.is_some(),
        "the press really picked a row up"
    );
}

/// Carry the proxy onto `to` and prove the pointer is over it before anything is read back:
/// the drag-and-drop catcher under the pointer is the *only* element the mouse-over switch raises
/// `0x3E` on, so a station that skipped this would be measuring a hint raised somewhere else.
fn carry_to(ui: &mut UiSystem, panels: &mut RemainingPanels, view: &View, to: ElemHandle, t: f64) {
    let (x, y) = centre(ui, to);
    ui.mouse_move(LocalTime(t), x, y);
    let hit = ui
        .hit_test_screen(x, y)
        .expect("the pointer is over something");
    let catcher = ui
        .drag_and_drop_catcher(hit)
        .expect("and over a drop catcher");
    assert!(
        catcher == to || ui.is_ancestor_of(to, catcher),
        "the pointer must reach {:?}; it reached {:?}",
        ui.node(to).unwrap().desc.element_id,
        ui.node(catcher).unwrap().desc.element_id
    );
    deliver(ui, panels, view);
}

/// Open tab 0 on `rows`, put the bar in front, and answer with its live list.
fn bar_with(
    ui: &mut UiSystem,
    panels: &mut RemainingPanels,
    view: &mut View,
    rows: Vec<u32>,
) -> Vec<ElemHandle> {
    view.tabs[0] = rows;
    panels.update(ui, view);
    let w = panels.spellcasting.lists[0]
        .as_ref()
        .expect("the spell item list binds");
    let handles: Vec<ElemHandle> = w.slots.iter().map(|s| s.handle).collect();
    front(ui, w.handle);
    handles
}

// =============================================================================================
// 1. A spell from the book, carried over a row of the bar.
// =============================================================================================

/// A spell from the spellbook lights the bar row it is over.
#[test]
fn a_spell_from_the_spellbook_lights_the_bar_row_it_is_over() {
    let (mut ui, mut panels, mut view) = env();
    let rows = bar_with(&mut ui, &mut panels, &mut view, vec![5, 6]);
    for i in 0..2 {
        assert_eq!(
            hint(&ui, &panels, 0, i).0,
            None,
            "nothing has painted row {i} yet"
        );
    }

    let from = spellbook_point(&mut ui, &panels, 3);
    pick_up(&mut ui, &mut panels, &view, from);
    front(&mut ui, rows[1]);
    carry_to(&mut ui, &mut panels, &view, rows[1], 0.1);

    assert_eq!(
        hint(&ui, &panels, 0, 1),
        (
            Some(drag_accept_state::ACCEPT),
            Some(drag_accept_state::ACCEPT)
        ),
        "the handler sets drag-accept 0x10000040 on the hovered tile"
    );
    assert_eq!(
        hint(&ui, &panels, 0, 0).0,
        None,
        "and on that row alone -- the tile's own arm acts on the slot the message names"
    );
    let _ = ui.requests.take();
}

/// **The hint follows the pointer, and comes down when it leaves.**
///
/// The element manager raises `0x3E` with `dwParam1 = 0` on the element being *left* before it
/// raises `1` on the one being entered, and the tile's leave arm is drag-accept `0x1000003F`.
/// Carrying the spell off the bar entirely — onto the spellbook, which no longer catches it —
/// has to take the last row's green down too.
#[test]
fn the_bar_hint_moves_with_the_pointer_and_comes_down_behind_it() {
    let (mut ui, mut panels, mut view) = env();
    let rows = bar_with(&mut ui, &mut panels, &mut view, vec![5, 6, 7]);
    let from = spellbook_point(&mut ui, &panels, 3);
    pick_up(&mut ui, &mut panels, &view, from);
    front(&mut ui, rows[2]);

    carry_to(&mut ui, &mut panels, &view, rows[2], 0.1);
    assert_eq!(hint(&ui, &panels, 0, 2).1, Some(drag_accept_state::ACCEPT));

    carry_to(&mut ui, &mut panels, &view, rows[0], 0.2);
    assert_eq!(
        (hint(&ui, &panels, 0, 2).1, hint(&ui, &panels, 0, 0).1),
        (
            Some(drag_accept_state::NONE),
            Some(drag_accept_state::ACCEPT)
        ),
        "the row that was left is cleared and the row that was entered is lit, in that order"
    );

    // Off the bar altogether: `0x100000A0` is not a `UiItemWidget`, so whatever catches the
    // drag there paints nothing, and the leave message is the only thing that runs.
    let book = panels
        .spellbook
        .list
        .as_ref()
        .expect("the spell list binds")
        .handle;
    let (x, y) = centre(&ui, book);
    ui.mouse_move(LocalTime(0.3), x, y);
    deliver(&mut ui, &mut panels, &view);
    for i in [0, 2] {
        assert_eq!(
            hint(&ui, &panels, 0, i).1,
            Some(drag_accept_state::NONE),
            "row {i} keeps no hint once the pointer has left the bar"
        );
    }
    assert_eq!(
        hint(&ui, &panels, 0, 1),
        (None, Some(StateId(0))),
        "and the row the pointer never visited was never written at all -- the leave arm acts on \
         the one slot its message names, which is why `clear_drag_accept`'s list-wide sweep is \
         gone"
    );
    let _ = ui.requests.take();
}

/// A spell dragged along the bar lights the row it is reordered onto.
#[test]
fn a_spell_dragged_along_the_bar_lights_the_row_it_is_reordered_onto() {
    let (mut ui, mut panels, mut view) = env();
    let rows = bar_with(&mut ui, &mut panels, &mut view, vec![5, 6, 7]);
    let p = centre(&ui, rows[2]);
    pick_up(&mut ui, &mut panels, &view, p);
    assert_eq!(
        ui.requests.take(),
        vec![dereth_ui_screens::view::UiRequest::RemoveSpellFavorite {
            spell_id: 7,
            tab: 0
        }],
        "the pick-up is the removal, not the release"
    );

    carry_to(&mut ui, &mut panels, &view, rows[0], 0.2);
    assert_eq!(
        hint(&ui, &panels, 0, 0),
        (Some(drag_accept_state::ACCEPT), Some(drag_accept_state::ACCEPT)),
        "a bar-to-bar drag carries a spellID, so the handler's spell-id test passes as it does for \
         the spellbook"
    );
    let _ = ui.requests.take();
}

/// **The bar has no refusal, and a duplicate is not one.**
///
/// The add-favorite path's duplicate check reads the player's favorite-spells list and is
/// inverted by `move_allowed`, which every *drag* sets; the hover handler does not contain the
/// check at all, and no `0x10000041` refusal exists in it. A station that let the bar
/// invent a red here would be a plausible-looking divergence nobody would think to look for.
#[test]
fn the_bar_shows_green_even_for_a_spell_the_tab_already_holds() {
    let (mut ui, mut panels, mut view) = env();
    let rows = bar_with(&mut ui, &mut panels, &mut view, vec![5, 6]);
    let from = spellbook_point(&mut ui, &panels, 5);
    pick_up(&mut ui, &mut panels, &view, from);
    front(&mut ui, rows[1]);
    carry_to(&mut ui, &mut panels, &view, rows[1], 0.1);
    assert_eq!(
        hint(&ui, &panels, 0, 1).1,
        Some(drag_accept_state::ACCEPT),
        "spell 5 is already row 0 of this tab and the hover still says yes"
    );
    let _ = ui.requests.take();
}

/// Behaviour: spellbar.drag-hint.a-spell-over-a-bar-row-lights-that-row-and-the-landing-row-does-not-keep-it
/// The row the spell lands on does not keep its green.
#[test]
fn the_row_the_spell_lands_on_does_not_keep_its_green() {
    let (mut ui, mut panels, mut view) = env();
    let rows = bar_with(&mut ui, &mut panels, &mut view, vec![5, 6]);
    let from = spellbook_point(&mut ui, &panels, 3);
    pick_up(&mut ui, &mut panels, &view, from);
    front(&mut ui, rows[1]);
    carry_to(&mut ui, &mut panels, &view, rows[1], 0.1);
    assert_eq!(
        hint(&ui, &panels, 0, 1).1,
        Some(drag_accept_state::ACCEPT),
        "lit first"
    );

    let (x, y) = centre(&ui, rows[1]);
    ui.mouse_up(7, x, y, false);
    deliver(&mut ui, &mut panels, &view);
    assert_eq!(
        ui.requests.take(),
        vec![dereth_ui_screens::view::UiRequest::AddSpellFavorite {
            spell_id: 3,
            index: 1,
            tab: 0
        }],
        "and the drop itself still works -- the clear must not eat the 0x15"
    );
    assert_eq!(
        hint(&ui, &panels, 0, 1),
        (Some(drag_accept_state::NONE), Some(drag_accept_state::NONE)),
        "the drag-ended arm sets drag-accept 0x1000003F on the tile the drop landed on"
    );
}

/// The denominator, so that the four stations above are known to be about **all** eight tabs and
/// not about the one the fixture happens to open.
///
/// The spellcasting screen's post-init runs the per-tab init eight times, once per page, and
/// each call registers the drag handler on that page's own `0x100000B6` list.
#[test]
fn every_one_of_the_eight_tabs_has_its_own_hinted_list() {
    let (ui, panels, _view) = env();
    let ids: Vec<Option<ElementId>> = panels
        .spellcasting
        .lists
        .iter()
        .map(|l| {
            l.as_ref()
                .and_then(|w| ui.node(w.handle))
                .map(|n| n.desc.element_id)
        })
        .collect();
    assert_eq!(
        ids,
        vec![Some(ElementId(0x1000_00B6)); 8],
        "init finds the spell item list at the SAME child id inside all eight pages"
    );
}
