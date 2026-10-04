//! A real spellbook press selects the row, sets the ring and scrolls only a clipped row into view;
//! secondary press preserves scroll; zero/absent spell clears rings; invalid index is a no-op;
//! filter button refill resets scroll.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

#![allow(clippy::pedantic)]

use crate::common::layout::RegistrationOrder;
use dereth_primitives::DataId;
use dereth_ui::framework::LayoutEnum;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::panels::{panel_stack::PanelStack, spellbook::SpellbookPanel};
use dereth_ui_screens::view::{GameView, SpellEntry};

#[derive(Debug)]
struct Book(Vec<SpellEntry>);
impl GameView for Book {
    fn spellbook(&self) -> &[SpellEntry] {
        &self.0
    }
}

const LISTENER: u32 = 0x743;

fn env() -> (UiSystem, SpellbookPanel, Book) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let root = dereth_ui_screens::env::create_and_add_root_element(
        &mut ui,
        LayoutEnum(0x1000_0006),
        ElementId(0x1000_0495),
    )
    .expect("shipped gameplay root");
    let page = ui
        .get_child_recursive(root, ElementId(0x1000_0190))
        .expect("spell page");
    let mut stack = PanelStack::default();
    stack.setup_children(&mut ui, root);
    let page_id = stack
        .pages
        .iter()
        .find(|p| p.handle == page)
        .expect("spell tab")
        .panel_id;
    stack.recv_set_panel_visibility(&mut ui, page_id, true);
    let mut panel = SpellbookPanel::default();
    panel.post_init(&mut ui, page);
    let book = Book(
        (1..=40)
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
    );
    assert!(panel.update(&mut ui, &book));
    let mut at = Some(panel.list.as_ref().expect("spell list").handle);
    while let Some(h) = at {
        ui.set_visible(h, true);
        at = ui.parent(h);
    }
    ui.register_for_element_messages(root, dereth_ui::ListenerId::External(LISTENER));
    ui.drain_outbox();
    (ui, panel, book)
}

fn deliver(ui: &mut UiSystem, panel: &mut SpellbookPanel, book: &Book) {
    for delivery in ui.drain_outbox() {
        if let dereth_ui::Delivery::Element {
            to: dereth_ui::ListenerId::External(LISTENER),
            msg,
        } = delivery
        {
            panel.on_element_message(ui, &msg, book);
        }
    }
}

fn is_descendant(ui: &UiSystem, mut h: ElemHandle, ancestor: ElemHandle) -> bool {
    loop {
        if h == ancestor {
            return true;
        }
        let Some(p) = ui.parent(h) else { return false };
        h = p;
    }
}

/// Behaviour: spellbook.click.reveals-and-selects-the-row
#[test]
fn actual_spellbook_press_selects_and_reveals_only_the_clipped_row_tail() {
    let (mut ui, mut panel, book) = env();
    let w = panel.list.as_ref().unwrap();
    let h = w.handle;
    let ch = w.cell.1;
    assert_eq!(w.max_columns, 1, "shipped spellbook is a vertical list");
    let view = ui.screen_box(h);
    let sy = if view.height() % ch == 0 { ch / 2 } else { 0 };
    dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, h, 0, sy);
    let index = usize::try_from((view.height() + sy - 2) / ch).unwrap();
    let slot = panel.list.as_ref().unwrap().slots[index].handle;
    let before = ui.node(slot).unwrap().region.box_;
    assert!(
        before.y1 >= view.height(),
        "the selected row is partially clipped"
    );
    let (x, y) = (view.x0 + 10, view.y1 - 1);
    let hit = ui.hit_test_screen(x, y).expect("a real visible row hit");
    assert!(
        is_descendant(&ui, hit, slot),
        "the test clicks the actual UIItem subtree"
    );
    ui.mouse_down(7, x, y); // the spellbook's element-message listener: the select action.
    deliver(&mut ui, &mut panel, &book);
    let expected_y = i32::try_from(index).unwrap() * ch - view.height() + ch;
    assert_eq!(
        panel.list.as_ref().unwrap().scroll(&ui),
        (0, expected_y),
        "set_selected must call scroll_to_view, not origin-aligning scroll_to_show"
    );
    assert_eq!(ui.node(slot).unwrap().region.box_.y1, view.height() - 1);
    let w = panel.list.as_ref().unwrap();
    assert!(w.slots[index].selected);
    assert!(
        ui.node(w.slots[index].selected_ring.expect("retail selected ring"))
            .unwrap()
            .region
            .flags
            .visible
    );
    assert!(w
        .slots
        .iter()
        .enumerate()
        .all(|(i, s)| s.selected == (i == index)));
    assert_eq!(panel.selected_spell, book.0[index].id);
}

#[test]
fn visible_selection_and_secondary_press_preserve_scroll_and_scope_the_source() {
    let (mut ui, mut panel, book) = env();
    let h = panel.list.as_ref().unwrap().handle;
    let ch = panel.list.as_ref().unwrap().cell.1;
    let view = ui.screen_box(h);
    dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, h, 0, ch / 2);
    let (x, y) = (view.x0 + 10, view.y0 + ch);
    for action in [7, 8] {
        ui.mouse_down(action, x, y);
        deliver(&mut ui, &mut panel, &book);
        ui.mouse_up(action, x, y, false);
        deliver(&mut ui, &mut panel, &book);
        assert_eq!(panel.selected_spell, book.0[1].id);
        assert_eq!(
            panel.list.as_ref().unwrap().scroll(&ui),
            (0, ch / 2),
            "an already fully visible selection must not origin-align"
        );
    }
    ui.mouse_down(8, x, y + ch);
    deliver(&mut ui, &mut panel, &book);
    assert_eq!(
        panel.selected_spell, book.0[2].id,
        "retail secondary press selects too"
    );
    assert!(!panel.list.as_ref().unwrap().slots[1].selected);
    assert!(panel.list.as_ref().unwrap().slots[2].selected);

    // Source-backed negative: a 0x1C from outside the spell-list subtree is not this panel's,
    // even with a valid pointer and misleading p2. Explicitly carry the point, as mouse input
    // does: the point-less broadcast API intentionally supplies (0, 0), not the current pointer.
    let unrelated = ui
        .get_element(ElementId(0x1000_0298))
        .expect("school filter button");
    ui.broadcast_element_message_at(
        unrelated,
        dereth_ui::msg::element::id::MOUSE_PRESS,
        7,
        1,
        dereth_ui::msg::MessagePoint {
            window: (x, y),
            element: (0, 0),
        },
    );
    deliver(&mut ui, &mut panel, &book);
    assert_eq!(panel.selected_spell, book.0[2].id);
    assert_eq!(panel.list.as_ref().unwrap().scroll(&ui), (0, ch / 2));

    // The remaining press actions do not select a spellbook row (10 is the separate
    // `add_spell_shortcut` arm, not `set_selected`). Wheel scrolling itself is covered in the
    // prior slice.
    for action in [9, 10] {
        ui.mouse_down(action, x, y);
        deliver(&mut ui, &mut panel, &book);
        assert_eq!(panel.selected_spell, book.0[2].id);
    }

    // Item-list actions follow the element parent chain; they have no generic text-arrow handler.
    let slot = panel.list.as_ref().unwrap().slots[2].handle;
    for target in [h, slot] {
        for action in [0x16, 0x17, 0x1A, 0x1B] {
            assert!(!ui.dispatch_action(
                target,
                &dereth_ui::focus::InputEvent {
                    action,
                    start: true,
                    x: 0,
                    y: 0
                }
            ));
            deliver(&mut ui, &mut panel, &book);
        }
    }
    assert_eq!(panel.selected_spell, book.0[2].id);
    assert_eq!(panel.list.as_ref().unwrap().scroll(&ui), (0, ch / 2));
}

#[test]
fn absent_or_zero_spell_clears_rings_without_scrolling_and_invalid_index_is_a_noop() {
    let (mut ui, mut panel, book) = env();
    let h = panel.list.as_ref().unwrap().handle;
    let view = ui.screen_box(h);
    ui.mouse_down(7, view.x0 + 10, view.y0 + 10);
    deliver(&mut ui, &mut panel, &book);
    assert_eq!(panel.selected_spell, book.0[0].id);
    assert!(panel.list.as_ref().unwrap().slots[0].selected);
    for id in [999_999, 0] {
        dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, h, 0, 100);
        panel.set_selected(&mut ui, id);
        assert_eq!(
            panel.selected_spell, id,
            "retail stores the id even if no row matches"
        );
        assert!(panel
            .list
            .as_ref()
            .unwrap()
            .slots
            .iter()
            .all(|s| !s.selected));
        assert_eq!(panel.list.as_ref().unwrap().scroll(&ui), (0, 100));
    }
    let w = panel.list.as_mut().unwrap();
    let geometry: Vec<_> = w
        .slots
        .iter()
        .map(|s| ui.node(s.handle).unwrap().region.box_)
        .collect();
    assert!(!w.scroll_to_view(&mut ui, w.slots.len()));
    assert_eq!(w.scroll(&ui), (0, 100));
    assert_eq!(
        geometry,
        w.slots
            .iter()
            .map(|s| ui.node(s.handle).unwrap().region.box_)
            .collect::<Vec<_>>()
    );
}

/// Behaviour: spellbook.filter.a-refill-after-a-filter-press-resets-the-scroll
#[test]
fn actual_filter_button_resets_scroll_after_refill() {
    let (mut ui, mut panel, mut book) = env();
    for (i, s) in book.0.iter_mut().enumerate() {
        if i % 2 == 0 {
            s.school = 1;
        }
    }
    assert!(panel.update(&mut ui, &book));
    let h = panel.list.as_ref().unwrap().handle;
    dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, h, 0, 200);
    assert_eq!(panel.list.as_ref().unwrap().scroll(&ui), (0, 200));
    let filter = ui
        .get_element(ElementId(0x1000_0298))
        .expect("Creature filter");
    let b = ui.screen_box(filter);
    let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    assert!(is_descendant(
        &ui,
        ui.hit_test_screen(x, y).expect("filter hit"),
        filter
    ));
    ui.mouse_down(7, x, y);
    ui.mouse_up(7, x, y, false);
    deliver(&mut ui, &mut panel, &book);
    assert_eq!(
        panel.shown.len(),
        20,
        "only War rows survive turning Creature off"
    );
    assert_eq!(panel.list.as_ref().unwrap().scroll(&ui), (0, 0));
    assert_eq!(
        ui.node(panel.list.as_ref().unwrap().slots[0].handle)
            .unwrap()
            .region
            .box_
            .y0,
        0
    );
}
