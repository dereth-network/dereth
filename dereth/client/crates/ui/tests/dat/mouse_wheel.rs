//! The mouse wheel, from `WM_MOUSEWHEEL` to a scroll offset: one detent over a chat log moves it
//! one line, forward toward the oldest line, and the other way puts it back. The wheel is a click
//! reflected off the element's own scrollbar:
//!
//! ```text
//! WM_MOUSEWHEEL
//!   1  the wheel event                          one click of DIMOFS_Z[+] / [-]
//!   2  the DefaultMap binding, input map 0x0A    -> action 5 / 6
//!   3  the manager's action handler             -> the manager's mouse-down (action, extent)
//!   4  the element's press                      -> element message 0x1C, p1 = action
//!   5  the scrollable's message listener        -> the scrollbar's wheel handler
//!   6  the bar broadcasts 0x0D / 0x0E
//!   7  the first arm                             -> inq_scroll_delta -> set_scrollable_xy
//! ```
//!
//! Hop 2 is asserted against the shipped keymaps read from the retail dats: the wheel is bound only
//! in input map `0x0A`, which the client registers for a focused text element. Hop 3's range
//! (`focus::action::MOUSE_ACTIONS`) is pinned as a literal; hops 4 to 7 are driven directly. A
//! scrollable with only a horizontal bar does not wheel, a detent over the bar itself is refused,
//! and the text element's own press does nothing with the wheel. Fixture: the shipped chat window's
//! shape (a log and its vertical bar as siblings), from a `LayoutDesc` written here.

use crate::common::NoAssets;
use dereth_primitives::{AssetSource, DataId, LocalTime};
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::focus::action;
use dereth_ui::scrollable::attr as scrollable_attr;
use dereth_ui::widgets::scrollbar::attr as bar_attr;
use dereth_ui::{ElemHandle, ElementId, ElementType, NullInputPump, UiSystem};

// ---------------------------------------------------------------------------------------------
// harness
// ---------------------------------------------------------------------------------------------

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

const LOG: u32 = 0x1000_0011;
const BAR: u32 = 0x1000_0012;

/// The shipped chat-window shape: a `TextElement` log and its vertical bar as **siblings** under
/// one container, which is why `find_relative` rather than `get_child_recursive` is what binds
/// them.
///
/// Returns `(container, log, bar)`. The log is 200x100; [`fill`] gives it twenty `"line N\n"`
/// rows, which measures as **twenty-one** lines of 16 px
/// (see [`LINES`]), i.e. 336 px of content and [`TRAVEL`] px of travel.
/// The text element's scroll delta makes one line step 16 px here.
fn tree(ui: &mut UiSystem) -> (ElemHandle, ElemHandle, ElemHandle) {
    tree_with(ui, true)
}

/// [`tree`], with a choice about whether the bar carries attribute `0x86` at all.
///
/// The wheel handler's **first** act is to read its own float attribute `0x86`, and it
/// returns when that fails, so "the bar has no position" is a real state with its own behaviour —
/// and one every other test here hides, because they all set it.
fn tree_with(ui: &mut UiSystem, with_position: bool) -> (ElemHandle, ElemHandle, ElemHandle) {
    let mut container = desc(0x1000_0010, ty::FIELD, 0, 0, 220, 100);
    let mut log = desc(LOG, ty::TEXT, 0, 0, 200, 100);
    // **`0x27` is what makes a chat log hittable at all**, and it is the shipped value:
    // mouse visibility is `editable || selectable`, so a
    // non-selectable label is transparent to the mouse-down hit test and no wheel click could
    // ever land on one. The retail log is selectable because the player can drag-select a line
    // out of it.
    log.base.properties.set(
        dereth_ui::props::attr::TEXT_SELECTABLE,
        dereth_ui::PropertyValue::Bool(true),
    );
    container.children.insert(ElementId(LOG), log);
    container
        .children
        .insert(ElementId(BAR), desc(BAR, ty::SCROLLBAR, 200, 0, 16, 100));
    let l = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(0x1000_0010), container)).collect(),
    };
    let d = l
        .access_element(ElementId(0x1000_0010))
        .cloned()
        .expect("root");
    let c = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(c, Some(root));
    let log = ui.get_child(c, ElementId(LOG)).expect("the log");
    let bar = ui.get_child(c, ElementId(BAR)).expect("the bar");
    // `0x72` is what binds on, and the binding is what
    // makes the bar's messages arrive at a sibling at all.
    ui.on_set_attribute(
        log,
        scrollable_attr::V_SCROLLBAR,
        Some(&dereth_ui::PropertyValue::Enum(BAR)),
    );
    if with_position {
        ui.set_attribute_float(bar, bar_attr::POSITION, 0.0);
    }
    ui.initialize_tree(c);
    // 20 rows, measured as 21 lines of 16 px, against a 100 px view: TRAVEL px of travel.
    fill(ui, c, log, ROWS);
    (c, log, bar)
}

/// Fill the log the way the client does: setting the text builds the glyph
/// list, and `dereth_ui::scrollable::recalculate_dirty_text` (the layout pass) runs the
/// glyph-list recalculation's tail, which is the scrollable-area resize.
///
/// **Nothing here hand-writes a scroll extent.** The `LINES * 16` px of content is measured
/// through `TextElement`'s own default `FixedMetrics { advance: 8, height: 16 }`, so the step the
/// wheel produces is whatever computes rather than a number this test
/// chose.
fn fill(ui: &mut UiSystem, container: ElemHandle, log: ElemHandle, lines: usize) {
    let text: String = (0..lines)
        .map(|i| {
            format!(
                "line {i}
"
            )
        })
        .collect::<Vec<_>>()
        .concat();
    let t = ui.text_element_mut(log).expect("a text element");
    t.set_text(&text);
    // Setting the text raises the `0x100` dirty bit; `TextElement::set_text` does not, and the
    // attribute-`0x17` path is what does it in a running screen. Raised here so the layout pass
    // below has something to consume, rather than reaching past it and writing an extent by hand.
    t.bits.set_dirty(true);
    let n = dereth_ui::scrollable::recalculate_dirty_text(ui, container);
    assert_eq!(n, 1, "the log re-measured itself");
}

/// One line's worth of scroll, as the text element's scroll delta computes it: the
/// height of the line at the top of the view, clamped to the view. With the default metrics that
/// is 16 px, and it is asserted rather than assumed so that a wheel click moving a *page* would be
/// caught.
const LINE: i32 = 16;

/// How many `"line N\n"` rows [`fill`] writes.
const ROWS: usize = 20;

/// How many **lines** the glyph list's recalculation then measures — twenty-one, not twenty.
///
/// The recalculation's tail asks whether the **last processed glyph** is a
/// newline and, when it is, appends an empty line at the glyph count whose height is that glyph's
/// own. [`fill`]'s text ends in `\n`, so retail measures the empty row after it and the pane is
/// `21 * 16 = 336` px tall, not `20 * 16 = 320`. `dereth_ui::text::glyph::wrap` has that arm, and
/// text layout sums every stored line's height, so the extra row is in the extent the layout
/// publishes.
const LINES: i32 = ROWS as i32 + 1;

/// The scroll clamp: content less view, and no further.
const TRAVEL: i32 = LINES * LINE - 100;

fn offset(ui: &mut UiSystem, log: ElemHandle) -> (i32, i32) {
    let s = ui
        .text_element_mut(log)
        .expect("the log is a text element")
        .scroll;
    (s.x, s.y)
}

// ---------------------------------------------------------------------------------------------
// hop 2 — the retail binding lives in input map 0x0A
// ---------------------------------------------------------------------------------------------

/// **The wheel is bound, and only in input map `0x0A`, calibrated in both directions.** A single
/// negative reading cannot distinguish "not bound in any other map" from a broken harness.
///
/// So this drives one `WM_MOUSEWHEEL` through the **real merged keymap** -- the shipped
/// default key map then `DefaultMap`, read from the retail dats, merged in the client's load order
/// and passed through our own keymap writer and reader, as a saved keymap is -- twice: once with
/// every map the retail client registers except `0x0A`, where it must produce **nothing**, and once
/// with input map `0x0A` additionally registered, where it must produce **action 5**. A harness
/// that could not see an action would fail the second half.
///
/// The maps are `dereth_input::RETAIL_MAP_REGISTRATIONS`, because `dereth-ui` cannot depend on
/// `dereth-client`. That the client registers `0x0A` for a focused text element (its
/// `FOCUSED_TEXT_MAPS`, `0x0A` first and unconditional) is asserted in `dereth-client`.
#[test]
fn the_shipped_keymap_binds_the_wheel_only_in_map_0x0a() {
    use dereth_input::{ActionId, InputManager, InputMapId, MasterInputMap};

    let store = dereth_dat::testing::open_store_or_fail();
    let read = |id: u32| {
        store
            .read(DataId(id))
            .unwrap_or_else(|e| panic!("{id:#010X} from the retail dats: {e}"))
    };
    let action_map = read(0x2600_0000);
    let gm_map = read(0x1400_0000);
    let default_map = read(0x1400_0002);

    // The merged keymap a client with no user file saves: the default key map, then `DefaultMap`,
    // written by our keymap writer.
    let mut merged = MasterInputMap::default();
    merged.merge(
        &MasterInputMap::read(&gm_map).expect("the default key map decodes"),
        true,
    );
    merged.merge(
        &MasterInputMap::read(&default_map).expect("DefaultMap decodes"),
        true,
    );
    let merged_text = merged.to_keymap_text();

    // The binding itself, straight out of the retail keymap: `DIMOFS_Z` positive -> action 5 in
    // input map 0x0A, negative -> action 6. `0x00080101` is device 1 (mouse), sub-control
    // `AxisPositive`, offset 8 (`DIMOFS_Z`).
    let merged = MasterInputMap::from_keymap_text(&merged_text).expect("parses");
    let map_a = merged
        .sections
        .iter()
        .find(|s| s.input_map_id == InputMapId(0x0A))
        .expect("the shipped DefaultMap has an input map 0x0A");
    let actions: Vec<u32> = map_a.bindings().iter().map(|(_, a)| a.0).collect();
    assert!(
        actions.contains(&5),
        "wheel up is action 5 in map 0x0A: {actions:?}"
    );
    assert!(
        actions.contains(&6),
        "wheel down is action 6 in map 0x0A: {actions:?}"
    );

    let wheel_up = dereth_input::win32::Win32Message::new(
        dereth_input::win32::msg::WM_MOUSEWHEEL,
        120_usize << 16,
        0,
        100,
    );
    let no_lead = |_: u8| false;

    // 1. Everything the client could plausibly have registered, **except** `0x0A`.
    //
    // `dereth-ui` cannot depend on `dereth-client`, so this walks `RETAIL_MAP_REGISTRATIONS` rather
    // than `BASE_MAP_REGISTRATIONS`, a strict **superset** of the client's base maps, which
    // makes the negative below stronger rather than weaker: not one of the maps retail ever
    // registers binds the wheel except the one that is skipped here.
    let mut m = InputManager::on_startup(&action_map, &default_map).expect("the retail tables");
    m.keymap = MasterInputMap::from_keymap_text(&merged_text).expect("parses");
    let cb = m.new_callback();
    for (_, map, prio) in dereth_input::RETAIL_MAP_REGISTRATIONS {
        if *map == 0x0A {
            continue;
        }
        m.register_input_map(InputMapId(*map), *prio, cb);
    }
    m.on_window_event(&wheel_up, &no_lead);
    let without: Vec<ActionId> = m.take_events().into_iter().map(|e| e.action).collect();
    assert!(
        without.is_empty(),
        "the wheel is bound only in map 0x0A, so with every other retail map registered a \
         wheel produces no action: {without:?}"
    );

    // 2. The calibration positive: push the map the scrollables are supposed to push.
    m.register_input_map(
        InputMapId(0x0A),
        dereth_input::dispatch::priority::LOWEST,
        cb,
    );
    m.on_window_event(&wheel_up, &no_lead);
    let with: Vec<(ActionId, bool)> = m
        .take_events()
        .into_iter()
        .map(|e| (e.action, e.start))
        .collect();
    assert_eq!(
        with,
        vec![(ActionId(5), true), (ActionId(5), false)],
        "one WM_MOUSEWHEEL is exactly one click: a start and a stop"
    );

    // Hop 3 is `UiClient::on_action`'s `if !action::MOUSE_ACTIONS.contains(&a) { return false }`.
    // Narrowing that range would silently break the wheel, so it is pinned here as a literal.
    assert!(action::MOUSE_ACTIONS.contains(&5));
    assert!(action::MOUSE_ACTIONS.contains(&6));
    assert_eq!(action::WHEEL_UP, 5);
    assert_eq!(action::WHEEL_DOWN, 6);
}

// ---------------------------------------------------------------------------------------------
// hops 4-7 — the press reflected off the scrollbar
// ---------------------------------------------------------------------------------------------

/// Behaviour: pointer.wheel.one-detent-over-the-chat-log-moves-it-one-line-and-the-other-way-puts-it-back
///
/// Oracle: the scrollable's message listener, second arm ->
/// the scrollbar's wheel handler -> `0x0D`/`0x0E` -> the first arm ->
/// the text element's scroll delta -> the scrollable's own XY setter.
///
/// The press is delivered through [`UiSystem::mouse_down`], which is exactly what
/// `UiClient::on_action` calls with the action `dereth-input` produced.
///
/// # The direction is the modern convention
///
/// | where | what it says |
/// |---|---|
/// | the wheel event | `if (delta < 1) sub = 0x80200 else sub = 0x80100`, so a forward wheel is the **positive** axis, which the shipped `DefaultMap` binds to action **5** |
/// | the scrollbar's wheel handler | broadcasts element message `0xE - (action == 5)` with `(0, 0)`, so action 5 raises **`0x0D`** |
/// | the scrollable's first arm | passes `id == 0x0D` or `id == 0x0F` as `inq_scroll_delta`'s **negate** flag, which negates the step when set |
///
/// So `0x0D` **decreases** the offset: one forward wheel click moves the log toward its *oldest*
/// line, which is what every other program on the machine does. The move-steps handler's
/// `+1`/`-1` says which *arrow* raises which id, not the sign the scrollable then applies; the
/// scrolling-area update puts the increment arrow at the top (see `cpu::controls::scrollbar`).
#[test]
fn a_wheel_click_over_a_chat_log_scrolls_it_by_one_line() {
    let mut ui = UiSystem::new((800, 600));
    let (_c, log, _bar) = tree(&mut ui);
    assert_eq!(offset(&mut ui, log), (0, 0), "before");

    // Action 5 at the top of the log: `0x0D`, which `handle_scrollbar_message` **negates**, so a
    // forward wheel from the oldest line is clamped by `set_scrollable_xy`.
    ui.mouse_down(action::WHEEL_UP, 100, 50);
    assert_eq!(
        offset(&mut ui, log),
        (0, 0),
        "a forward wheel has nowhere to go from the top"
    );

    // Action 6: `0x0E`, not negated, one `inq_scroll_delta` — a line, not a page.
    ui.mouse_down(action::WHEEL_DOWN, 100, 50);
    assert_eq!(offset(&mut ui, log), (0, LINE), "one line");
    ui.mouse_down(action::WHEEL_DOWN, 100, 50);
    assert_eq!(
        offset(&mut ui, log),
        (0, LINE * 2),
        "each click is one step, and it does not grow"
    );
    assert!(
        LINE < 100,
        "a line, not the 100 px view a track click pages by"
    );

    // And back, exactly.
    ui.mouse_down(action::WHEEL_UP, 100, 50);
    assert_eq!(
        offset(&mut ui, log),
        (0, LINE),
        "action 5 is the exact inverse of action 6"
    );
    ui.mouse_down(action::WHEEL_UP, 100, 50);
    assert_eq!(offset(&mut ui, log), (0, 0), "back where it started");

    // The far end clamps too. **The measurement, before the clamp that depends on it** — a
    // wrong line count and a wrong clamp would otherwise agree with each other and this file
    // would pass on both being wrong together.
    {
        let t = ui.text_element_mut(log).expect("the log");
        assert_eq!(
            t.glyphs.lines.len(),
            LINES as usize,
            "twenty `line N\\n` rows measure as twenty-one lines: the trailing-newline \
             arm appends the empty row after the final `\\n`"
        );
        let last = *t.glyphs.lines.last().expect("a last line");
        assert_eq!(
            (last.width, last.height),
            (0, LINE),
            "and it is empty, at the glyph height"
        );
        assert_eq!(
            t.glyphs.lines.iter().map(|l| l.height).sum::<i32>(),
            LINES * LINE,
            "which is what `recalculate_layout` sums into the extent",
        );
    }
    for _ in 0..40 {
        ui.mouse_down(action::WHEEL_DOWN, 100, 50);
    }
    assert_eq!(
        offset(&mut ui, log),
        (0, TRAVEL),
        "content {} less view 100, and no further",
        LINES * LINE,
    );
}

/// Oracle: the same arm's "has a vertical scrollbar id" test. A scrollable with only a horizontal
/// bar does not wheel at all — the client never consults `0x71` here.
#[test]
fn a_scrollable_with_only_a_horizontal_bar_does_not_wheel() {
    let mut ui = UiSystem::new((800, 600));
    let (_c, log, _bar) = tree(&mut ui);
    // Move the binding from 0x72 to 0x71 and re-bind.
    ui.on_set_attribute(log, scrollable_attr::V_SCROLLBAR, None);
    ui.on_set_attribute(
        log,
        scrollable_attr::H_SCROLLBAR,
        Some(&dereth_ui::PropertyValue::Enum(BAR)),
    );
    // Action **6**, the direction that moves a log sitting at 0. Action 5 would be clamped to 0
    // anyway, so this assertion would hold on a perfectly working wheel.
    ui.mouse_down(action::WHEEL_DOWN, 100, 50);
    assert_eq!(offset(&mut ui, log), (0, 0));
}

/// Oracle: the same arm's "the bar is not under the mouse" test. A wheel click while the pointer is
/// over the
/// **bar** is refused, because the bar is a `Button` and the press is already going to be
/// its own; without the guard the same gesture would scroll twice.
///
/// Calibrated: the second half is the same gesture with the pointer moved off the bar, which must
/// scroll. Without it "refused" and "the harness cannot scroll at all" read the same.
#[test]
fn a_wheel_click_over_the_scrollbar_itself_is_refused() {
    let mut ui = UiSystem::new((800, 600));
    let (_c, log, bar) = tree(&mut ui);
    // The press lands on the **bar**, whose screen box is x 200..215. The log hears it anyway:
    // the scrollable's post-init registers the log as a listener on the bar's
    // messages, because the two are siblings and nothing the bar raises would otherwise reach it.
    assert_eq!(
        ui.hit_test_screen(207, 50),
        Some(bar),
        "the press is on the bar"
    );
    ui.mouse_down(action::WHEEL_DOWN, 207, 50);
    assert_eq!(
        offset(&mut ui, log),
        (0, 0),
        "refused while the pointer is on the bar"
    );
    // The same gesture one pixel-space to the left, over the log itself.
    ui.mouse_down(action::WHEEL_DOWN, 100, 50);
    assert_eq!(
        offset(&mut ui, log),
        (0, LINE),
        "and allowed the moment it is not"
    );
}

/// Behaviour: chargen.controls.wheel-over-scrollbars
///
/// Direct content-wheel input updates the linked text and the live thumb together, even while
/// the scrollbar's message handler owns its behavior during the nested content notification.
#[test]
fn an_opted_in_content_bar_moves_text_and_thumb_once_and_back() {
    use dereth_ui::widgets::scrollbar::{self, DirectWheel, Scrollbar};

    let mut ui = UiSystem::new((800, 600));
    let (_container, log, bar) = tree(&mut ui);
    let layout = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        ..LayoutDesc::default()
    };
    let thumb = ui
        .create_element(&NoAssets, &layout, &desc(1, ty::FIELD, 0, 0, 16, 20))
        .unwrap()
        .unwrap();
    ui.set_parent(thumb, Some(bar));
    ui.initialize_tree(thumb);
    scrollbar::update_layout_of(&mut ui, bar);
    ui.node_mut(bar)
        .unwrap()
        .behaviour
        .as_mut()
        .unwrap()
        .as_any_mut()
        .unwrap()
        .downcast_mut::<Scrollbar>()
        .unwrap()
        .direct_wheel = Some(DirectWheel::Content);

    let before = ui.screen_box(thumb);
    assert_eq!(offset(&mut ui, log), (0, 0));
    assert_eq!(Scrollbar::position(&ui, bar), 0.0);
    ui.mouse_move(LocalTime(1.0), 207, 50);
    assert_eq!(ui.hit_test_screen(207, 50), Some(bar));
    ui.mouse_down(action::WHEEL_DOWN, 207, 50);
    assert_eq!(offset(&mut ui, log), (0, LINE), "one line, not two");
    assert!((Scrollbar::position(&ui, bar) - 16.0 / 236.0).abs() < 0.00001);
    let after = ui.screen_box(thumb);
    assert_eq!(
        after.y0,
        before.y0 + 5,
        "the thumb follows the new position"
    );
    assert_eq!((after.width(), after.height()), (16, 20));

    ui.mouse_down(action::WHEEL_UP, 207, 50);
    assert_eq!(offset(&mut ui, log), (0, 0));
    assert_eq!(Scrollbar::position(&ui, bar), 0.0);
    assert_eq!(
        ui.screen_box(thumb),
        before,
        "the inverse restores the thumb"
    );
}

/// The same guard, asserted directly on [`Scrollable::wheel_target`]'s contract.
///
/// **The end-to-end test above passes for two reasons and this one isolates the guard.** A press
/// that lands on the bar is consumed by the bar's own `Button` base before `dispatch_and_forward`
/// reaches the bar's registered listeners, so end to end the log never hears it and the guard is
/// never evaluated. The guard is still the client's, so it is pinned here, where it is
/// falsifiable.
#[test]
fn the_mouse_over_guard_is_wheel_targets_own_contract() {
    use dereth_ui::msg::element::id::MOUSE_PRESS;
    let mut ui = UiSystem::new((800, 600));
    let (_c, log, bar) = tree(&mut ui);
    let s = ui.text_element_mut(log).expect("a text element").scroll;

    assert_eq!(
        s.wheel_target(&ui, log, MOUSE_PRESS, action::WHEEL_UP),
        Some(bar)
    );
    ui.switch_mouse_over(Some(bar));
    assert_eq!(
        s.wheel_target(&ui, log, MOUSE_PRESS, action::WHEEL_UP),
        None,
        "the bar is under the mouse"
    );
    ui.switch_mouse_over(None);
    assert_eq!(
        s.wheel_target(&ui, log, MOUSE_PRESS, action::WHEEL_UP),
        Some(bar),
        "and back"
    );

    // The other two conditions of the same guard, for completeness: it is a *press*, and it is a
    // wheel action.
    assert_eq!(
        s.wheel_target(
            &ui,
            log,
            dereth_ui::msg::element::id::MOUSE_RELEASE,
            action::WHEEL_UP
        ),
        None
    );
    assert_eq!(
        s.wheel_target(&ui, log, MOUSE_PRESS, action::PRIMARY_CLICK),
        None
    );
}

/// Oracle: — actions `5` and `6` pass its outer guard
/// (`action == 7 || action == 5 || action == 6`) and **every statement inside is gated on
/// `action == 7`**. It is a dead branch in the shipped client, and pinned here so that "the text
/// element does nothing with the wheel" is a recorded fact rather than an absence.
///
/// The calibration is the primary click: the same point, the same element, and it *does* move the
/// caret. Without that half, a wheel that left the caret alone would be indistinguishable from a
/// press that never arrived.
#[test]
fn the_text_elements_own_mouse_down_still_does_nothing_with_the_wheel() {
    let mut ui = UiSystem::new((800, 600));
    let (_c, log, _bar) = tree(&mut ui);

    // Calibration: a primary click at the same point moves the caret off zero and arms a
    // selection drag, because `0x27` (selectable) is set on this log.
    ui.mouse_down(action::PRIMARY_CLICK, 60, 40);
    let t = ui.text_element_mut(log).expect("alive");
    let clicked_cursor = t.cursor;
    assert!(
        clicked_cursor > 0,
        "the primary click put the caret at glyph {clicked_cursor}"
    );
    assert!(t.bits.selecting(), "and armed a selection drag");
    t.cursor = 0;
    t.bits.set_selecting(false);
    t.selection = Default::default();

    // The wheel, at the same point. It must scroll and must not touch the caret.
    ui.mouse_down(action::WHEEL_DOWN, 60, 40);
    assert_eq!(offset(&mut ui, log), (0, LINE), "the press was delivered");
    let t = ui.text_element_mut(log).expect("alive");
    assert_eq!(
        t.cursor, 0,
        "action 5/6 falls through the press handler's `action == 7` gate"
    );
    assert!(!t.bits.selecting(), "and starts no selection");
}

/// Oracle: the scrollbar's wheel handler, whose first act is to read float attribute `0x86` and
/// return if it is absent. **The fetched position is never used**;
/// the read is purely a presence test, so a bar carrying no `0x86` does nothing at all on the
/// wheel while a bar carrying `0x86 = 0.0` scrolls.
///
/// **The bar acquires `0x86` whether or not a caller sets one.** Building the tree without the
/// harness's `set_attribute_float(bar, 0x86, 0.0)` still gives the bar a position:
/// `resize_scrollable_area`, which is what gives a text element a scroll extent in the first place,
/// ends by writing the bar's position ( -> `write_position`), so *every* route that makes a
/// scrollable scrollable also gives its bar a position.
///
/// So the guard cannot be reached through this crate's own paths. It is kept because it is the
/// client's first act, and measured here: with and without the explicit set, the attribute is
/// present and the wheel moves exactly one line either way.
#[test]
fn the_bar_acquires_its_position_attribute_whether_or_not_a_caller_sets_one() {
    for explicit in [false, true] {
        let mut ui = UiSystem::new((800, 600));
        let (_c, log, bar) = tree_with(&mut ui, explicit);
        assert!(
            ui.node(bar)
                .expect("the bar")
                .merged_properties()
                .get_float(bar_attr::POSITION)
                .is_some(),
            "explicit={explicit}: 0x86 is present because resize_scrollable_area wrote it"
        );
        ui.mouse_down(action::WHEEL_DOWN, 100, 50);
        assert_eq!(
            offset(&mut ui, log),
            (0, LINE),
            "explicit={explicit}: one line"
        );
    }
}

/// A frame's worth of the real loop, so that nothing here depends on a hand-driven order.
#[test]
fn the_wheel_survives_a_frame_tick() {
    let mut ui = UiSystem::new((800, 600));
    let (_c, log, _bar) = tree(&mut ui);
    let mut pump = NullInputPump;
    ui.use_time(LocalTime(1.0), &mut pump);
    ui.mouse_down(action::WHEEL_DOWN, 100, 50);
    ui.use_time(LocalTime(2.0), &mut pump);
    assert_eq!(offset(&mut ui, log), (0, LINE));
}
