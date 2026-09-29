//! Pinned literals match symbols; the two update-layout transcriptions agree over a sweep; the
//! screen-side binder writes the behaviour's list items; a real press on a row selects it and
//! raises message 4.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

#![allow(clippy::pedantic)]

use crate::common::layout::RegistrationOrder;

use crate::common::*;
use dereth_ui::framework::LayoutEnum;
use dereth_ui::widgets::listbox::ListBox;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::env::create_and_add_root_element;
use dereth_ui_screens::panels::listbox::{ListBoxWidget, ATTR_HORIZONTAL, ATTR_MAX_COLUMNS};

// ------------------------------------------------------------------------------------------
// The literals, pinned rather than reached through the symbols under test.
//
// The stated testability rule: *"a test that reads a constant through the same symbol it writes it through
// cannot detect a wrong constant"*. Every number below is written here as the client's own and
// asserted equal to the symbol, so a transcription error in either has somewhere to fail.
// ------------------------------------------------------------------------------------------

/// The list box's layout pass makes exactly one attribute read,
/// integer attribute `0x5F` (columns) — `maximum columns`.
const MAX_COLUMNS_ATTRIBUTE: u32 = 0x5F;
/// Bit 1 of the list box's flags — `horizontal layout`.
const HORIZONTAL_ATTRIBUTE: u32 = 0x5C;
const COMPONENT_LIST: ElementId = ElementId(0x1000_0464);
/// A new list-box selection broadcasts element message **4**.
const LIST_SELECTION_CHANGED: u32 = 0x04;
/// A row's mouse-click message.
const MOUSE_CLICK: u32 = 0x19;
/// A list box is element **type 5**; the component list's rows come from
/// its template list and not from an item-slot cache.
const LISTBOX_ELEMENT_TYPE: u32 = 5;

#[test]
fn the_pinned_literals_are_the_symbols_they_stand_for() {
    assert_eq!(ATTR_MAX_COLUMNS, MAX_COLUMNS_ATTRIBUTE);
    assert_eq!(ATTR_HORIZONTAL, HORIZONTAL_ATTRIBUTE);
    assert_eq!(
        dereth_ui::msg::element::id::LIST_SELECTION_CHANGED.0,
        LIST_SELECTION_CHANGED
    );
    assert_eq!(dereth_ui::msg::element::id::MOUSE_CLICK.0, MOUSE_CLICK);
    assert_eq!(
        dereth_ui_screens::panels::spellcomponent::COMPONENT_LIST,
        COMPONENT_LIST
    );
}

// ------------------------------------------------------------------------------------------
// The differential — no dats, no screen, pure arithmetic.
// ------------------------------------------------------------------------------------------

/// Read the item list back off a live element's behaviour.
fn behaviour_items(ui: &UiSystem, h: ElemHandle) -> Option<Vec<ElemHandle>> {
    Some(
        ui.node(h)?
            .behaviour
            .as_ref()?
            .as_any()?
            .downcast_ref::<ListBox>()?
            .items
            .clone(),
    )
}

fn behaviour_selected(ui: &UiSystem, h: ElemHandle) -> Option<usize> {
    ui.node(h)?
        .behaviour
        .as_ref()?
        .as_any()?
        .downcast_ref::<ListBox>()?
        .selected
}

/// The two transcriptions of update layout agree on every swept case.
#[test]
fn the_two_transcriptions_of_update_layout_agree_on_every_swept_case() {
    let mut cases = 0usize;
    for n in 0..=8usize {
        for cols in [-1i32, 0, 1, 2, 3, 7] {
            for horizontal in [false, true] {
                for ragged in [false, true] {
                    let mut ui = UiSystem::new((800, 600));
                    let holder = ui.create_hollow(None);
                    ui.set_attribute_int(holder, MAX_COLUMNS_ATTRIBUTE, cols);
                    ui.set_attribute_bool(holder, HORIZONTAL_ATTRIBUTE, horizontal);
                    let mut rows = Vec::new();
                    for i in 0..n {
                        let e = ui.create_hollow(Some(holder));
                        // Ragged rows: a header 3x as tall and 2x as wide as its neighbours, which
                        // is the skills list's own shape.
                        let (w, h) = if ragged && i % 3 == 0 {
                            (60, 30)
                        } else {
                            (30, 10)
                        };
                        ui.resize_to(e, w, h);
                        rows.push(e);
                    }

                    // (a) the behaviour's copy, driven off a standalone `ListBox` so the arena's
                    //     own behaviour on `holder` is untouched.
                    let mut lb = ListBox::default();
                    lb.items = rows.clone();
                    let a_grid = lb.update_layout(&mut ui, holder);
                    let a_boxes: Vec<(i32, i32)> = rows
                        .iter()
                        .map(|h| {
                            let b = ui.node(*h).map(|n| n.region.box_).unwrap_or_default();
                            (b.x0, b.y0)
                        })
                        .collect();

                    // Put every row back where it started, so (b) cannot inherit (a)'s placement.
                    for r in &rows {
                        ui.move_to(*r, 0, 0);
                    }

                    // (b) the screen-side binder's copy.
                    let mut w = ListBoxWidget::bind(&ui, holder);
                    w.items = rows.clone();
                    let b_grid = w.update_layout(&mut ui);
                    let b_boxes: Vec<(i32, i32)> = rows
                        .iter()
                        .map(|h| {
                            let b = ui.node(*h).map(|n| n.region.box_).unwrap_or_default();
                            (b.x0, b.y0)
                        })
                        .collect();

                    let case = format!("n={n} 0x5F={cols} horizontal={horizontal} ragged={ragged}");
                    assert_eq!(a_grid, b_grid, "(columns, rows) disagree: {case}");
                    assert_eq!(a_boxes, b_boxes, "row placement disagrees: {case}");
                    // `bind` must have read the two attributes back off the element, or the
                    // agreement above is an agreement between two defaults.
                    assert_eq!(w.max_columns, cols, "bind read 0x5F: {case}");
                    assert_eq!(w.horizontal, horizontal, "bind read 0x5C: {case}");
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 9 * 6 * 2 * 2, "the sweep's own denominator");
    assert_eq!(cases, 216);
}

// ------------------------------------------------------------------------------------------
// The real list, off the shipped layout.
// ------------------------------------------------------------------------------------------

/// Every fixture path here is an `expect`, never a skip — the stated testability rule says *"a test that skips is
/// a test that passes"*.
fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

fn component_list(ui: &mut UiSystem) -> ElemHandle {
    let root = create_and_add_root_element(ui, LayoutEnum(0x1000_0006), ElementId(0x1000_0495))
        .expect("classic_gameplay builds");
    let mut all = Vec::new();
    walk(ui, root, &mut all);
    let h = all
        .iter()
        .copied()
        .find(|h| {
            ui.node(*h)
                .is_some_and(|n| n.element_id() == COMPONENT_LIST)
        })
        .expect(
            "the spell components panel's component list box is in the shipped gameplay layout",
        );
    assert_eq!(
        ui.node(h).expect("the list box is in the tree").ty().0,
        LISTBOX_ELEMENT_TYPE,
        "0x10000464 is a plain list-box element and therefore carries a ListBox behaviour"
    );
    h
}

/// The screen side binder writes the behaviours m list items.
#[test]
fn the_screen_side_binder_writes_the_behaviours_m_list_items() {
    let mut ui = env();
    let h = component_list(&mut ui);
    let mut w = ListBoxWidget::bind(&ui, h);
    assert_eq!(
        w.templates.len(),
        2,
        "a category header and a component row"
    );
    assert_eq!(
        behaviour_items(&ui, h),
        Some(Vec::new()),
        "the element starts with no rows"
    );

    // Six rows, alternating the two templates, exactly as the spell-component notice rebuilds
    // the list.
    for i in 0..6 {
        w.add_from_template(&mut ui, i % 2, None)
            .expect("the shipped templates build");
    }
    assert_eq!(w.created, 6);
    assert_eq!(w.create_failures, 0);
    assert_eq!(
        w.behaviour_unreachable, 0,
        "every mirror reached a ListBox behaviour"
    );
    assert_eq!(w.items.len(), 6);
    assert_eq!(
        behaviour_items(&ui, h).as_ref(),
        Some(&w.items),
        "the item list is one array"
    );

    // An insert before the end, which is the other row insertion path.
    let inserted = w
        .add_from_template(&mut ui, 1, Some(2))
        .expect("insert builds");
    assert_eq!(w.items[2], inserted);
    assert_eq!(
        behaviour_items(&ui, h).as_ref(),
        Some(&w.items),
        "after an insert"
    );

    // Delete by index.
    assert!(w.delete_item(&mut ui, 0));
    assert_eq!(w.items.len(), 6);
    assert_eq!(
        behaviour_items(&ui, h).as_ref(),
        Some(&w.items),
        "after a delete"
    );

    // Flush the whole list.
    w.flush(&mut ui);
    assert!(w.items.is_empty());
    assert_eq!(behaviour_items(&ui, h), Some(Vec::new()), "after a flush");
    assert_eq!(
        w.behaviour_unreachable, 0,
        "and not one mirror was silently skipped"
    );
}

/// Behaviour: ui.list.a-row-built-from-a-template-is-a-list-item-and-a-press-selects-it
/// A real press on a row selects it and raises message 4.
#[test]
fn a_real_press_on_a_row_selects_it_and_raises_message_4() {
    let mut ui = env();
    let h = component_list(&mut ui);
    let mut w = ListBoxWidget::bind(&ui, h);
    for i in 0..4 {
        w.add_from_template(&mut ui, i % 2, None)
            .expect("the shipped templates build");
    }
    // The layout pass — without it the four rows sit on top of each other at the list
    // box's origin and the hit test has only one band to walk.
    w.update_layout(&mut ui);
    assert_eq!(
        behaviour_selected(&ui, h),
        None,
        "the selected index is -1 before any press"
    );

    ui.register_for_element_message(
        COMPONENT_LIST,
        dereth_ui::msg::element::id::LIST_SELECTION_CHANGED,
        dereth_ui::msg::ListenerId::External(235),
    );
    let _ = ui.drain_outbox();

    let row = w.items[2];
    let b = ui.screen_box(row);
    let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    let lb = ui.screen_box(h);

    assert_eq!(
        ui.hit_test(h, cx - lb.x0, cy - lb.y0),
        Some(h),
        "a press at a row's centre resolves to the list box: a row is a field element and \
         the client's should-be-mouse-visible test answers false for it"
    );

    // The `0x1C` a real press would raise, with the window point the element manager's
    // mouse-down event stamps on it — which is what
    // the under-the-mouse lookup reads.
    ui.broadcast_element_message_at(
        h,
        dereth_ui::msg::element::id::MOUSE_PRESS,
        dereth_ui::focus::action::PRIMARY_CLICK,
        0,
        dereth_ui::msg::MessagePoint {
            window: (cx, cy),
            element: (cx - lb.x0, cy - lb.y0),
        },
    );

    let fours: Vec<_> = ui
        .drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            dereth_ui::Delivery::Element { to, msg }
                if to == dereth_ui::msg::ListenerId::External(235)
                    && msg.id.0 == LIST_SELECTION_CHANGED =>
            {
                Some(msg)
            }
            _ => None,
        })
        .collect();
    assert_eq!(fours.len(), 1, "one selection change for one press");
    assert_eq!(fours[0].source, h, "raised by the list box, not by the row");
    assert_eq!(fours[0].p1, 2, "p1 is the index in the item list");
    assert_eq!(ElemHandle::from_raw(fours[0].p2), row, "p2 is the item");
    assert_eq!(
        behaviour_selected(&ui, h),
        Some(2),
        "the selected item is the row that was pressed"
    );

    // The selected item is held by **reference**: inserting above the selection moves its index and
    // does not change what is selected. That is what the mirror carries across, and asserting it is
    // what stops the mirror being written index-wise.
    w.add_from_template(&mut ui, 0, Some(0))
        .expect("insert at the head");
    assert_eq!(
        behaviour_selected(&ui, h),
        Some(3),
        "the same row, one index further down"
    );
    assert_eq!(behaviour_items(&ui, h).map(|v| v[3]), Some(row));
}
