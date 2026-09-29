//! A press on a list row selects it, and pressing the row that is already selected raises `0x43`.
//!
//! The gesture is a real press, `UiSystem::mouse_down(PRIMARY_CLICK, x, y)` at a row's screen
//! centre, and it hit-tests to the **list box**, not the row (a row is a field element that takes
//! no presses). A press selects **twice**, and both paths run the same selection routine:
//!
//! * the press descends through the scrollable base, which raises `0x1C` on the element; the list
//!   box's own message listener takes `0x1C` with `p1 == 7` when click-select (flag bit `0x2`, set
//!   only by attribute `0x59`) is on, finds the item under the mouse and selects it with notify;
//! * the press handler's own tail then repeats that for action `7` or `10`, unless the element is
//!   being moved or resized;
//! * selecting the item that is already selected broadcasts `0x43` and returns; selecting a new one
//!   walks the item list for its index (`-1` when absent), restyles the old and new rows when the
//!   styling bit (`0x20`) is on, and broadcasts `4` with that index and the row.
//!
//! So a press on a new row is heard as `4` then `0x43`, re-selecting is the only producer of `0x43`
//! (a double-click message is not), and a retail double-click is two presses. The restyle sets the
//! **state** (not the media state) of the old row to the list's `0x5D` and of the new one to
//! `0x5E`.
//!
//! Fixture: a list box with three field rows, from a `LayoutDesc` written here, inside `dereth-ui`
//! only (no screen, no panel); no assets, no dats.

use crate::common::NoAssets;
use dereth_primitives::DataId;
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::focus::action;
use dereth_ui::msg::element::id as msgid;
use dereth_ui::msg::Delivery;
use dereth_ui::widgets::listbox::{self, ListBox};
use dereth_ui::{ElemHandle, ElementId, ElementType, ListenerId, PropertyValue, UiSystem};

// ---------------------------------------------------------------------------------------------
// harness — a list box, three rows, and a recording listener. No assets, no dats, no screen.
// ---------------------------------------------------------------------------------------------

const CONTAINER: u32 = 0x1000_0010;
const LIST: u32 = 0x1000_0011;
const ROW: [u32; 3] = [0x1000_0020, 0x1000_0021, 0x1000_0022];

/// The click-select attribute: the `0x59` case in the list box's attribute setter. The literal is
/// the oracle and is written out here rather than imported, so that
/// renaming the widget's own constant cannot quietly change what this file asserts.
const ATTR_CLICK_SELECT: u32 = 0x59;

/// Who hears the list's broadcasts. An `External` listener, because that is what a screen is —
/// `GamePlayScreen` registers exactly this way — and because its deliveries queue in the outbox
/// where a test can read them back in order.
const WHO: ListenerId = ListenerId::External(0x1000_0015);

/// The list box's own rectangle inside its container. Off-origin on both axes on purpose: a hit
/// test that forgot to subtract the element's screen origin would
/// answer row 0 for every press and still look alive.
const LIST_BOX: (i32, i32, i32, i32) = (20, 40, 200, 90);
/// One row. Three of them are 90 px, which is exactly the list box, so nothing here depends on
/// scrolling.
const ROW_SIZE: (i32, i32) = (200, 30);

fn desc(id: u32, ty: ElementType, x: i32, y: i32, w: i32, h: i32) -> ElementDesc {
    ElementDesc {
        base: StateDesc {
            incorporation: incorporation::LEGACY_ALL_GEOMETRY,
            x,
            y,
            width: w,
            height: h,
            ..StateDesc::default()
        },
        element_id: ElementId(id),
        ty,
        ..ElementDesc::default()
    }
}

/// The list's item vector, written the way
/// `dereth_ui_screens::panels::listbox::ListBoxWidget::mirror_items` writes it, the only public
/// seam this crate offers for it.
///
/// Every screen-side list is populated exactly this way, so a harness that used some private
/// back door would be testing a shape no panel produces.
fn set_items(ui: &mut UiSystem, list: ElemHandle, rows: &[ElemHandle]) {
    let l = ui
        .node_mut(list)
        .and_then(|n| {
            n.behaviour
                .as_mut()?
                .as_any_mut()?
                .downcast_mut::<ListBox>()
        })
        .expect("the list box carries a ListBox behaviour");
    l.items = rows.to_vec();
}

/// `(list, rows)` — a list box holding three field rows.
///
/// The rows are authored as children of the list at their grid positions, which is the picture
/// the list box's own layout pass leaves and the one every screen-side
/// binder produces; `listbox::refresh_scroll_of` then captures their origins and the
/// grid exactly as it does on a live panel.
///
/// `click_select` chooses whether the layout declares attribute `0x59`. It is a parameter because
/// `dat::listbox_click_select` measures that **15 of the 35 shipped list boxes do not declare it**,
/// so "the layout did not ask for click-selection" is a real shipped state with its own behaviour
/// and not a case only a test can reach.
fn tree(ui: &mut UiSystem, click_select: bool) -> (ElemHandle, Vec<ElemHandle>) {
    let mut container = desc(CONTAINER, ty::FIELD, 0, 0, 400, 300);
    let (lx, ly, lw, lh) = LIST_BOX;
    let mut list = desc(LIST, ty::LISTBOX, lx, ly, lw, lh);
    if click_select {
        // attribute `0x59` is click-select: declaring it turns on the flag-bit `0x2` arm.
        list.base
            .properties
            .set(ATTR_CLICK_SELECT, PropertyValue::Bool(true));
    }
    for (i, id) in ROW.iter().enumerate() {
        let y = i32::try_from(i).unwrap_or(0) * ROW_SIZE.1;
        list.children.insert(
            ElementId(*id),
            desc(*id, ty::FIELD, 0, y, ROW_SIZE.0, ROW_SIZE.1),
        );
    }
    container.children.insert(ElementId(LIST), list);
    let l = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(CONTAINER), container)).collect(),
    };
    let d = l
        .access_element(ElementId(CONTAINER))
        .cloned()
        .expect("root");
    let c = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(c, Some(root));
    let list = ui.get_child(c, ElementId(LIST)).expect("the list box");
    let rows: Vec<ElemHandle> = ROW
        .iter()
        .map(|id| ui.get_child(list, ElementId(*id)).expect("a row"))
        .collect();
    ui.initialize_tree(c);
    set_items(ui, list, &rows);
    listbox::refresh_scroll_of(ui, list);
    ui.register_for_element_messages(list, WHO);
    (list, rows)
}

fn ui_with(click_select: bool) -> (UiSystem, ElemHandle, Vec<ElemHandle>) {
    let mut ui = UiSystem::new((800, 600));
    let (list, rows) = tree(&mut ui, click_select);
    ui.drain_outbox();
    (ui, list, rows)
}

/// Every **selection** message the list broadcast since the last drain, as `(id, p1, p2)`.
///
/// Narrowed to the selection routine's own two ids. A press also raises `0x1F`, `0x1B`,
/// `0x1C` and `0x2F` off the same element — mouse-over, mouse-over-top, the press itself and the
/// focus change — and those are the input layer's, not the list's; folding
/// them in would make every assertion here a transcript of the input layer instead of a statement
/// about the list.
fn heard(ui: &mut UiSystem, list: ElemHandle) -> Vec<(u32, u32, u32)> {
    const SELECTION: [u32; 2] = [
        msgid::LIST_SELECTION_CHANGED.0,
        msgid::LIST_ITEM_ACTIVATED.0,
    ];
    ui.drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            Delivery::Element { to, msg } if to == WHO && msg.source == list => SELECTION
                .contains(&msg.id.0)
                .then_some((msg.id.0, msg.p1, msg.p2)),
            _ => None,
        })
        .collect()
}

/// The selected row's index off the live element, or `-1` when nothing is selected.
fn selected(ui: &UiSystem, list: ElemHandle) -> i32 {
    ui.node(list)
        .and_then(|n| n.behaviour.as_ref()?.as_any()?.downcast_ref::<ListBox>())
        .and_then(|l| l.selected)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1)
}

/// A press at one row's own screen centre. Returns the element the hit test chose.
fn press_row(ui: &mut UiSystem, row: ElemHandle, act: u32) -> Option<ElementId> {
    let b = ui.screen_box(row);
    let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    let hit = ui.hit_test_screen(cx, cy);
    ui.mouse_down(act, cx, cy);
    hit.and_then(|h| ui.node(h))
        .map(dereth_ui::ElementNode::element_id)
}

// ---------------------------------------------------------------------------------------------
// 0 — the calibration: where does a press on a row actually land?
// ---------------------------------------------------------------------------------------------

/// **A press at a row's own centre hit-tests to the list box, not to the row.**
///
/// This is the reading that makes the rest of the file worth writing: on the live page list the
/// hit test answers the list box `0x10000583`, and this reproduces it on a synthetic list. It also
/// pins the *row's* answer, because "the row is not
/// mouse-visible" and "the list box happens to be in front" are different worlds and only the
/// first makes the `0x1C` arm necessary.
#[test]
fn the_press_lands_on_the_list_box_not_the_row() {
    let (mut ui, list, rows) = ui_with(true);
    let b = ui.screen_box(rows[1]);
    assert_eq!(
        (b.x0, b.y0, b.width(), b.height()),
        (LIST_BOX.0, LIST_BOX.1 + ROW_SIZE.1, ROW_SIZE.0, ROW_SIZE.1),
        "row 1 sits one row down, inside the list box"
    );
    let hit = press_row(&mut ui, rows[1], action::PRIMARY_CLICK);
    assert_eq!(
        hit,
        Some(ElementId(LIST)),
        "the hit test cannot return a row: a field element is not \
         mouse-visible and nothing registers 0x1C on it"
    );
    assert_ne!(
        hit,
        Some(ElementId(ROW[1])),
        "and it is emphatically not the row"
    );
    assert!(
        !ui.node(rows[1]).expect("live").is_mouse_visible,
        "mouse visibility answers false for a plain row"
    );
    assert!(
        ui.node(list).expect("live").is_mouse_visible,
        "the list box is always mouse-visible"
    );
}

// ---------------------------------------------------------------------------------------------
// 1 — the press selects
// ---------------------------------------------------------------------------------------------

/// Behaviour: ui.list.a-press-on-a-row-selects-it-and-pressing-the-selected-row-raises-activate
///
/// **One real press over row 1 selects row 1 and broadcasts `4` with `p1 = 1`.**
///
/// The `0x1C` arm of the list box's message listener plus its press handler's tail, and the
/// selection routine's new-item branch. An arm that needed a `MOUSE_CLICK` whose *source* was a
/// row could never fire: [`the_press_lands_on_the_list_box_not_the_row`] shows no hit test can
/// produce one.
///
/// `p2` is the selected item — the row **element**, not its id — because that is what
/// the client sends there, and the menu's `0x04` arm reads
/// `p2` back as an element pointer.
///
/// The trailing `0x43` on every press is the press handler's tail re-selecting the row the `0x1C`
/// arm has just selected; it has its own test below.
#[test]
fn a_press_over_a_row_selects_that_row_and_broadcasts_four() {
    let (mut ui, list, rows) = ui_with(true);
    assert_eq!(selected(&ui, list), -1, "nothing is selected to begin with");

    press_row(&mut ui, rows[1], action::PRIMARY_CLICK);
    assert_eq!(selected(&ui, list), 1, "the press selected row 1");
    assert_eq!(
        heard(&mut ui, list).first().copied(),
        Some((msgid::LIST_SELECTION_CHANGED.0, 1, rows[1].raw())),
        "set_selected_item broadcasts 4 with the index and the selected row"
    );

    // Row 2, to rule out "every press answers 1".
    press_row(&mut ui, rows[2], action::PRIMARY_CLICK);
    assert_eq!(selected(&ui, list), 2);
    assert_eq!(
        heard(&mut ui, list).first().copied(),
        Some((msgid::LIST_SELECTION_CHANGED.0, 2, rows[2].raw())),
        "and moving the selection raises a 4 for the new index"
    );

    // Row 0, upwards, because `inq_item_index_at_point`'s band walk is directional.
    press_row(&mut ui, rows[0], action::PRIMARY_CLICK);
    assert_eq!(selected(&ui, list), 0);
}

/// **A list whose layout does not declare `0x59` selects nothing — and that is retail.**
///
/// The list box's flag word is `0x290` out of construction, bit 1 clear,
/// and that bit is set from exactly one place: the attribute handler's `0x59` case. Both
/// press sites test flag bit `0x2` before they look at the pointer. Fifteen shipped list boxes
/// declare no `0x59` (see `dat::listbox_click_select`), so without this gate the widget would
/// select on presses the retail client ignores, in fifteen places.
///
/// This is the negative half of the calibration: the same harness, the same gesture, one bit of
/// shipped data different.
#[test]
fn without_click_select_a_press_selects_nothing() {
    let (mut ui, list, rows) = ui_with(false);
    let hit = press_row(&mut ui, rows[1], action::PRIMARY_CLICK);
    assert_eq!(
        hit,
        Some(ElementId(LIST)),
        "the press still lands on the list box"
    );
    assert_eq!(
        selected(&ui, list),
        -1,
        "flag bit 0x2 is clear, so neither site looks"
    );
    assert_eq!(heard(&mut ui, list), vec![], "and nothing is broadcast");
}

/// **Action 7 selects; action 8 does not.**
///
/// The press handler tests `action == 7 || action == 0xA` and the `0x1C` twin tests
/// `p1 == 7` alone. Note this is *narrower* than the stat-management panel's own arm,
/// which also takes `8` and `0xB` — the panel is more permissive than the
/// widget, which is one reason the two are not the same code and the panel's copy is not a
/// duplicate of this one.
#[test]
fn the_selecting_action_is_seven_and_not_eight() {
    for (act, want) in [
        (action::PRIMARY_CLICK, 1_i32),
        (action::SECONDARY_CLICK, -1),
    ] {
        let (mut ui, list, rows) = ui_with(true);
        press_row(&mut ui, rows[1], act);
        assert_eq!(selected(&ui, list), want, "action {act}");
    }
}

// ---------------------------------------------------------------------------------------------
// 2 — pressing the selected row is what raises 0x43
// ---------------------------------------------------------------------------------------------

/// Behaviour: ui.list.a-press-on-a-row-selects-it-and-pressing-the-selected-row-raises-activate
///
/// **Pressing the row that is already selected raises `0x43`, and `MOUSE_DOUBLE_CLICK` does not.**
///
/// Re-selecting the selected row is the only thing that raises `0x43` anywhere in the client, so
/// `0x43` *means* "you pressed the row that was already selected" and
/// the double-click check's one-second window is a detector built on top of it.
/// `MOUSE_DOUBLE_CLICK` has no producer on a row and must pass straight through, and the re-press
/// must reach the line the selection routine broadcasts `0x43` on rather than return early.
///
/// `p1` is the **row element**, `p2` is `0`: the broadcast is `0x43` with the selected row and `0`.
///
/// Both halves are asserted. Dropping the equal branch turns the first half red; raising `0x43`
/// from a double-click turns the second half red.
#[test]
fn pressing_the_already_selected_row_raises_0x43_and_a_double_click_does_not() {
    let (mut ui, list, rows) = ui_with(true);
    press_row(&mut ui, rows[1], action::PRIMARY_CLICK);
    assert_eq!(
        heard(&mut ui, list).first().copied(),
        Some((msgid::LIST_SELECTION_CHANGED.0, 1, rows[1].raw())),
        "the first press selects"
    );

    press_row(&mut ui, rows[1], action::PRIMARY_CLICK);
    assert_eq!(selected(&ui, list), 1, "the selection does not move");
    let again = heard(&mut ui, list);
    assert!(
        !again.is_empty()
            && again
                .iter()
                .all(|(id, p1, p2)| *id == msgid::LIST_ITEM_ACTIVATED.0
                    && *p1 == rows[1].raw()
                    && *p2 == 0),
        "every message the re-press raises is 0x43 with the selected row and 0: {again:?}"
    );

    // A double-click message. `0x1A` has no producer on a list box in retail and
    // `set_selected_item` is the only thing that makes a `0x43`, so a double-click message must
    // pass straight through.
    ui.broadcast_element_message(list, msgid::MOUSE_DOUBLE_CLICK, 0, 0);
    let after: Vec<(u32, u32, u32)> = heard(&mut ui, list)
        .into_iter()
        .filter(|(id, _, _)| *id == msgid::LIST_ITEM_ACTIVATED.0)
        .collect();
    assert_eq!(
        after,
        vec![],
        "MOUSE_DOUBLE_CLICK is not 0x43's producer anywhere in the client"
    );
}

/// **One press runs `set_selected_item(item, true)` twice, so a retail double-click is two
/// presses.**
///
/// The doubling is not an accident to be tidied away: the base element's `0x1C` broadcast
/// reaches the list box's message listener, and then the press handler's own tail
/// repeats it. The journal's page list relies on it: its double-click detector is armed by the
/// first `0x43` and fires on the second, so a list that selected once per press would need
/// **three** clicks to open a journal page.
///
/// Measured as what a listener hears per press, because that is the observable: press one is
/// `4` then `0x43` (the tail re-selecting the row the first call just selected), press two is
/// `0x43` twice.
#[test]
fn one_press_is_two_set_selected_item_calls() {
    let (mut ui, list, rows) = ui_with(true);

    press_row(&mut ui, rows[1], action::PRIMARY_CLICK);
    assert_eq!(
        heard(&mut ui, list),
        vec![
            (msgid::LIST_SELECTION_CHANGED.0, 1, rows[1].raw()),
            (msgid::LIST_ITEM_ACTIVATED.0, rows[1].raw(), 0),
        ],
        "press 1: the 0x1C arm selects (4), the press handler's tail re-selects the same row (0x43)"
    );

    press_row(&mut ui, rows[1], action::PRIMARY_CLICK);
    assert_eq!(
        heard(&mut ui, list),
        vec![
            (msgid::LIST_ITEM_ACTIVATED.0, rows[1].raw(), 0),
            (msgid::LIST_ITEM_ACTIVATED.0, rows[1].raw(), 0),
        ],
        "press 2: both calls find the row already selected, so two 0x43s"
    );
}
