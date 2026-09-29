//! Double-click appends the spell to the open tab without moving selection; drag from the spellbook
//! lands on row/tab/background; dragging off a tab removes it; Delete asks first and only Yes
//! queues a wire request.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

#![allow(clippy::pedantic)]

use crate::common::layout::RegistrationOrder;
use dereth_primitives::{DataId, LocalTime};
use dereth_ui::{ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::remaining::RemainingPanels;
use dereth_ui_screens::view::{GameView, SpellEntry, UiRequest};

/// The read-only half of the seam. `tabs` contains the favorite-spell lists, which the host
/// owns: this view is edited by hand where a test needs to model the host having applied a
/// [`UiRequest::AddSpellFavorite`], and never by the panel.
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

const LISTENER: u32 = 0xF900;

fn env() -> (UiSystem, RemainingPanels, View, ElemHandle) {
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
                display_order: i32::try_from(id).expect("a small id"),
                bitfield: 0,
            })
            .collect(),
        tabs: std::array::from_fn(|_| Vec::new()),
    };
    panels.update(&mut ui, &view);
    ui.requests.clear();
    ui.notice_inbox.clear();
    ui.register_for_element_messages(root, dereth_ui::ListenerId::External(LISTENER));
    ui.drain_outbox();
    (ui, panels, view, root)
}

/// The shipped tree hides the pages this screen is not showing; a pointer test needs the subtree
/// visible for `hit_test_screen` to reach it, which is what the panel button would have done.
fn reveal(ui: &mut UiSystem, mut h: ElemHandle) {
    loop {
        ui.set_visible(h, true);
        let Some(parent) = ui.parent(h) else { break };
        h = parent;
    }
}

/// Deliver.
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

fn centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let b = ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// A whole drag: press on `from`, cross the `dx² + dy² > 15` threshold, move onto `to`, release.
/// Every message the manager raises on the way — `0x21` for the refused generic drag, `0x14` for
/// the proxy, `0x15` on the target and `0x16` on the owner — goes through the real dispatch.
fn drag(
    ui: &mut UiSystem,
    panels: &mut RemainingPanels,
    view: &View,
    from: (i32, i32),
    to: (i32, i32),
) {
    ui.mouse_down(7, from.0, from.1);
    deliver(ui, panels, view);
    ui.mouse_move(LocalTime(0.0), from.0 + 20, from.1 + 20);
    deliver(ui, panels, view);
    assert!(ui.is_dragging(), "the pointer really picked the row up");
    ui.mouse_move(LocalTime(0.0), to.0, to.1);
    deliver(ui, panels, view);
    ui.mouse_up(7, to.0, to.1, false);
    deliver(ui, panels, view);
}

/// The spellbook row showing `spell`, revealed and hit-tested so the pointer really lands on it.
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
    let (h, row) = (w.handle, w.slots[i].handle);
    reveal(ui, h);
    // Bring the source panel to the front: the panel a player drags from is the one they last
    // clicked, so the pointer can reach it. Without it the spell bar, revealed by the arm under
    // test, sits over the book and `hit_test_screen` answers with the bar.
    let mut chain = vec![row];
    while let Some(parent) = ui.parent(*chain.last().unwrap()) {
        chain.push(parent);
    }
    for h in chain.into_iter().rev() {
        ui.bring_to_front(h);
    }
    let p = centre(ui, row);
    let hit = ui
        .hit_test_screen(p.0, p.1)
        .expect("the real spellbook row is hit-testable");
    assert!(
        hit == row || ui.is_ancestor_of(row, hit),
        "spell {spell} row {i} box {:?} list {:?} point {p:?} hit {:?} id {:?}",
        ui.screen_box(row),
        ui.screen_box(h),
        hit,
        ui.node(hit).unwrap().desc.element_id
    );
    p
}

/// Behaviour: spellbar.transfer.a-double-click-or-a-drag-from-the-book-adds-the-spell-to-the-open-tab
/// The spellbook's element-message listener, its `dwParam1 == 10` arm and everything it
/// reaches: the add-spell-shortcut notice, received by the spellcasting panel, which appends the
/// spell as a favourite at the end of the open tab.
///
/// **Falsified by** deleting the `if m.p1 == 10` block in `SpellbookPanel::on_element_message`,
/// which is what this build shipped: the double-click fell into the select arm, moved the ring and
/// raised nothing. It is also falsified by removing the `spellbook_message` hop in
/// `RemainingPanels`, which is what would leave the notice with no receiver.
#[test]
fn a_double_click_appends_the_spell_to_the_open_tab_and_never_moves_the_selection() {
    let (mut ui, mut panels, mut view, _) = env();
    view.tabs[0] = vec![5, 6];
    panels.update(&mut ui, &view);
    let tab = panels.spellcasting.open_sub_menu_index(&ui);
    assert_eq!(
        tab, 0,
        "0x100000AA falls through open_sub_menu_index's switch to 0"
    );
    let p = spellbook_point(&mut ui, &panels, 3);

    // The first parameter is the input action, and 10 is the double-press — the same value the
    // bar's own cast arm keys on, which is why the two panels must not both claim it.
    ui.mouse_down(10, p.0, p.1);
    deliver(&mut ui, &mut panels, &view);

    // The add-favourite append: the tab's spell count is read *after* its own increment, so a tab
    // holding two rows sends **3**, one past the last index. The packable list's insert walks that
    // many nodes, runs off the end and pushes at the tail, so the list is right and the number is
    // retail's.
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::AddSpellFavorite {
            spell_id: 3,
            index: 3,
            tab: 0
        }],
        "the double-click reaches the wire, and appends"
    );
    // The client's arm `break`s before the select arm: putting a spell on the bar does not change
    // what the spellbook has selected, which is what DELETE would then ask about.
    assert_eq!(
        panels.spellbook.selected_spell, 0,
        "the add-favourite arm never calls SetSelected"
    );
    // Adding a favorite selects the spell on the bar, independently of the book's selection.
    assert_eq!(panels.spellcasting.sub_menus[0].selected_spell, 3);
    assert_eq!(panels.spellcasting.favorites_added, 1);

    // The host applied it. A second double-click of the same spell hits `add_favorite`'s
    // already-listed check (when moves are not allowed, a spell already on the list returns) and is refused,
    // silently and without a message.
    view.tabs[0] = vec![5, 6, 3];
    panels.update(&mut ui, &view);
    ui.mouse_up(10, p.0, p.1, false);
    deliver(&mut ui, &mut panels, &view);
    let _ = ui.requests.take();
    ui.mouse_down(10, p.0, p.1);
    deliver(&mut ui, &mut panels, &view);
    assert!(
        ui.requests.take().is_empty(),
        "a duplicate is refused: the check reads the favourite-spells list, not the rows"
    );
    assert_eq!(panels.spellcasting.favorites_added, 1);
}

/// A drag from the spellbook lands on the row the tab or the background.
#[test]
fn a_drag_from_the_spellbook_lands_on_the_row_the_tab_or_the_background() {
    let (mut ui, mut panels, mut view, _) = env();
    view.tabs[0] = vec![5, 6];
    panels.update(&mut ui, &view);

    // ---- arm one: onto a row of the open tab's own item list ---------------------------------
    // If the target is inside the open tab's spell item list, the item under the mouse gives its
    // row number, so the index is the **row** and not the end of the list.
    let bar = panels.spellcasting.lists[0].as_ref().unwrap();
    let (bar_h, row1) = (bar.handle, bar.slots[1].handle);
    reveal(&mut ui, bar_h);
    let from = spellbook_point(&mut ui, &panels, 3);
    let to = centre(&ui, row1);
    drag(&mut ui, &mut panels, &view, from, to);
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::AddSpellFavorite {
            spell_id: 3,
            index: 1,
            tab: 0
        }],
        "a drop on row 1 inserts at 1, not at the end"
    );

    view.tabs[0] = vec![5, 6, 3];
    panels.update(&mut ui, &view);
    let tab5 = panels.spellcasting.sub_menus[5]
        .tab_element
        .expect("the tab element binds");
    assert_eq!(
        ui.node(tab5).unwrap().desc.element_id,
        ElementId(0x1000_00A8),
        "the sixth tab element is Init's third argument for sub-menu 5"
    );
    reveal(&mut ui, tab5);
    // Rows 0..7 are inside the shipped list's viewport; a clipped row would need
    // `scroll_to_view` first, which is `set_selected`'s job and not this arm's.
    let from = spellbook_point(&mut ui, &panels, 4);
    let to = centre(&ui, tab5);
    drag(&mut ui, &mut panels, &view, from, to);
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::AddSpellFavorite {
            spell_id: 4,
            index: 1,
            tab: 5
        }],
        "a drop on tab 5's element appends to tab 5, whose empty list makes the spell count 1"
    );
    assert_eq!(
        panels.spellcasting.open_sub_menu_index(&ui),
        0,
        "and the open tab did not change"
    );

    // ---- arm three: onto the bar's background --------------------------------------------------
    // Otherwise, if the target is the bar's background, the spell is added as a shortcut at the
    // first free position (-1).
    let bg = panels
        .spellcasting
        .background
        .expect("the spellcast background (0x100000A0) binds");
    let from = spellbook_point(&mut ui, &panels, 2);
    // The press happens first, because `spellbook_point` had to bring the book to the front to be
    // clickable and the bar has to be in front to be *droppable*. A real player has the same
    // problem and solves it the same way round: the drag is already in flight, so the release
    // lands on whatever is under the pointer then.
    ui.mouse_down(7, from.0, from.1);
    deliver(&mut ui, &mut panels, &view);
    ui.mouse_move(LocalTime(0.0), from.0 + 20, from.1 + 20);
    deliver(&mut ui, &mut panels, &view);
    assert!(ui.is_dragging());
    reveal(&mut ui, bg);
    let mut chain = vec![bg];
    while let Some(parent) = ui.parent(*chain.last().unwrap()) {
        chain.push(parent);
    }
    for h in chain.into_iter().rev() {
        ui.bring_to_front(h);
    }
    // The background is the whole bar, with the tab strip, the eight lists, the cast button and
    // the endowment icon over most of it; the client's arm is reached wherever none of those is,
    // so the point is *found* rather than assumed to be the centre.
    let b = ui.screen_box(bg);
    let p = (b.x0..b.x1)
        .step_by(4)
        .flat_map(|x| (b.y0..b.y1).step_by(4).map(move |y| (x, y)))
        .find(|(x, y)| ui.hit_test_screen(*x, *y) == Some(bg))
        .expect("some part of the shipped 0x100000A0 is exposed");
    ui.mouse_move(LocalTime(0.0), p.0, p.1);
    deliver(&mut ui, &mut panels, &view);
    ui.mouse_up(7, p.0, p.1, false);
    deliver(&mut ui, &mut panels, &view);
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::AddSpellFavorite {
            spell_id: 2,
            index: 4,
            tab: 0
        }],
        "the background appends to the OPEN tab, which now holds three"
    );
}

/// Behaviour: spellbar.transfer.a-spell-dragged-off-a-tab-is-removed
/// The item list's begin-drag notice, as the spellcasting UI receives it.
///
/// The counterpart of the drop, and the reason it is here rather than left for later: wiring the
/// `0x21` pickup for the bar's own lists without this would have made a spell dragged off a tab
/// disappear from the screen with nothing sent, which is a *new* silent divergence.
///
/// **Falsified by** deleting the `start.spell` block in `SpellcastingPanel::on_element_message`'s
/// `0x21` arm.
#[test]
fn dragging_a_spell_off_a_tab_removes_it_when_it_is_picked_up() {
    let (mut ui, mut panels, mut view, _) = env();
    view.tabs[0] = vec![5, 6, 7];
    panels.update(&mut ui, &view);
    let bar = panels.spellcasting.lists[0].as_ref().unwrap();
    let (bar_h, row1) = (bar.handle, bar.slots[1].handle);
    reveal(&mut ui, bar_h);
    let p = centre(&ui, row1);
    ui.mouse_down(7, p.0, p.1);
    deliver(&mut ui, &mut panels, &view);
    ui.mouse_move(LocalTime(0.0), p.0 + 20, p.1 + 20);
    deliver(&mut ui, &mut panels, &view);
    // `remove_spell_from_menu` runs on **pick-up**, not on release: the removal is already on the
    // wire before the pointer has gone anywhere.
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::RemoveSpellFavorite {
            spell_id: 6,
            tab: 0
        }],
        "0x01E4 for the tab the list belongs to"
    );
    ui.mouse_up(7, p.0 + 400, p.1 + 200, false);
    deliver(&mut ui, &mut panels, &view);
}

/// Behaviour: spellbook.delete.asks-the-shard-and-predicts-nothing
/// Delete asks first and only a yes puts anything on the wire.
#[test]
fn delete_asks_first_and_only_a_yes_puts_anything_on_the_wire() {
    let (mut ui, mut panels, view, root) = env();
    panels.update(&mut ui, &view);
    let btn = ui
        .get_child_recursive(root, ElementId(0x1000_02A5))
        .expect("the DELETE button");
    reveal(&mut ui, btn);
    let p = centre(&ui, btn);

    // With no spell selected the handler returns false — the button with nothing selected does
    // nothing, and in particular raises no dialog.
    ui.mouse_down(7, p.0, p.1);
    deliver(&mut ui, &mut panels, &view);
    ui.mouse_up(7, p.0, p.1, false);
    deliver(&mut ui, &mut panels, &view);
    assert!(
        panels.spellbook.delete_dialogs().is_empty(),
        "nothing selected, nothing asked"
    );
    assert!(ui.requests.take().is_empty());

    // Select through the production path: a primary press on a spellbook row.
    let row = spellbook_point(&mut ui, &panels, 7);
    ui.mouse_down(7, row.0, row.1);
    deliver(&mut ui, &mut panels, &view);
    ui.mouse_up(7, row.0, row.1, false);
    deliver(&mut ui, &mut panels, &view);
    assert_eq!(panels.spellbook.selected_spell, 7);

    for answer_yes in [false, true] {
        ui.mouse_down(7, p.0, p.1);
        deliver(&mut ui, &mut panels, &view);
        ui.mouse_up(7, p.0, p.1, false);
        deliver(&mut ui, &mut panels, &view);
        let &[(context, spell)] = panels.spellbook.delete_dialogs() else {
            panic!("delete_spell raises exactly one dialog per press")
        };
        assert_eq!(
            spell, 7,
            "the id travels in the collection under 0x1000003F"
        );
        let droot = ui
            .dialogs
            .info(context)
            .unwrap()
            .element
            .expect("the element half ran");
        // The dialog's text update puts property 0xC5 on child 0x3E. The prompt is the retail
        // string with the spell's name in its one `%s`: a confirmation that did not
        // name the spell would be a confirmation of nothing.
        let text = ui
            .get_child_recursive(droot, dereth_ui::dialog::base::child::TEXT)
            .unwrap();
        let shown: String = ui
            .text_element_mut(text)
            .unwrap()
            .glyphs
            .glyphs
            .iter()
            .filter_map(|g| char::from_u32(u32::from(g.data)))
            .collect();
        assert_eq!(
            shown,
            "Are you sure you want to remove Spell 7 from your spellbook? \
             You will no longer be able to cast this spell unless you learn it again!"
                .replace("             ", "")
        );

        let (accept, cancel) = dereth_ui::dialog::DialogKind::Confirmation.answer_children();
        let button = if answer_yes { accept } else { cancel }.unwrap();
        let h = ui.get_child_recursive(droot, button).unwrap();
        reveal(&mut ui, h);
        let q = centre(&ui, h);
        ui.mouse_down(7, q.0, q.1);
        deliver(&mut ui, &mut panels, &view);
        ui.mouse_up(7, q.0, q.1, false);
        deliver(&mut ui, &mut panels, &view);
        panels.update(&mut ui, &view);

        assert!(
            panels.spellbook.delete_dialogs().is_empty(),
            "the context is closed either way"
        );
        assert_eq!(
            ui.requests.take(),
            if answer_yes {
                vec![UiRequest::RemoveSpell { spell_id: 7 }]
            } else {
                Vec::new()
            },
            "only a true 0x92 answer sends the remove-spell request"
        );
        // Nothing local: the row is still there, because the shard has not said otherwise.
        assert!(
            panels.spellbook.shown.contains(&7),
            "delete_spell removes nothing itself"
        );
    }
}
