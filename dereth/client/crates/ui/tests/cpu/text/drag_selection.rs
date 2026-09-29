//! Click-drag text selection on a text element, the control behind the chat log: a press anchors an
//! empty range at the caret, the drag moves its end to the glyph under the pointer, and the release
//! keeps it. A backwards drag selects the same range, a move after the release changes nothing, a
//! sweep over a scrolled log selects what is on screen, and a copied line pastes into another box.
//! An editable box that is not selectable takes the press and selects nothing. A shift-click
//! extends the selection from where it began, or from the caret when no range is live.
//!
//! Selection is a three-bit state machine on the text element: `0x4` says the element is selectable
//! at all, `0x40` says a drag is in progress, and `0x80` says a range is live.
//!
//! * A mouse-move with `0x40` clear goes straight to the base element and touches nothing; with
//!   it set the move hit-tests the pointer against the glyphs and moves the caret there.
//! * A mouse-up with `0x40` set moves the caret one last time and then clears `0x40`, so the
//!   release ends the drag without collapsing the range.
//! * Moving the caret is what anchors. Asked to select (explicitly, or because `0x40` or the
//!   shift key is down on an ordinary move), a selectable element with no live range sets
//!   `0x80` and anchors at the **old** caret; any other move with `0x80` set drops the range.
//!   The caret is then clamped to the glyph count and, while `0x80` is set, becomes the
//!   selection's end.
//!
//! A selection that anchors and never extends draws nothing, and so does one never anchored; the
//! two are identical on screen, so every assertion is on the range, the text it yields and the
//! rectangles it inverts. Fixture: one text element in a field, from a `LayoutDesc` written here.

use crate::common::NoAssets;
use dereth_primitives::{DataId, LocalTime};
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::focus::{action, InputEvent};
use dereth_ui::{ElemHandle, ElementId, ElementType, InputPump, UiSystem};

/// The input device's shift-key query, with an answer the test chooses.
#[derive(Debug, Default)]
struct Pump {
    shift: bool,
}
impl InputPump for Pump {
    fn use_time(&mut self, _now: LocalTime) {}
    fn shift_key_down(&self) -> bool {
        self.shift
    }
}

const CONTAINER: u32 = 0x1000_0010;
const LOG: u32 = 0x1000_0011;

fn desc(id: u32, ty: ElementType, w: i32, h: i32) -> ElementDesc {
    ElementDesc {
        base: StateDesc {
            incorporation: incorporation::LEGACY_ALL_GEOMETRY,
            x: 0,
            y: 0,
            width: w,
            height: h,
            ..StateDesc::default()
        },
        element_id: ElementId(id),
        ty,
        ..ElementDesc::default()
    }
}

/// One `TextElement` at the screen origin, 200 x 40, holding `text`.
///
/// The default `FixedMetrics` advance is **8** and the line height **16**, and the element carries
/// no margins and the default (near) justification, so glyph `i` occupies screen x `[8i, 8i+8)`.
/// The point-to-glyph lookup's half-width rule therefore answers `i` for any x in `[8i-4, 8i+4)`.
///
/// `attr` is the one bit under test, written into the **desc** rather than poked afterwards:
/// mouse visibility is `editable || selectable`, so a text
/// element carrying neither is transparent to the hit test and no press could reach it at all.
/// `0x27` is what the shipped chat log `0x10000011` carries; `0x16` is what the entry box carries.
fn text_element(ui: &mut UiSystem, text: &str, attr: u32) -> ElemHandle {
    text_element_with_ids(ui, text, attr, CONTAINER, LOG)
}

fn text_element_with_ids(
    ui: &mut UiSystem,
    text: &str,
    attr: u32,
    container_id: u32,
    text_id: u32,
) -> ElemHandle {
    let mut container = desc(container_id, ty::FIELD, 400, 200);
    let mut log = desc(text_id, ty::TEXT, 200, 40);
    log.base
        .properties
        .set(attr, dereth_ui::PropertyValue::Bool(true));
    container.children.insert(ElementId(text_id), log);
    let l = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(container_id), container)).collect(),
    };
    let d = l
        .access_element(ElementId(container_id))
        .cloned()
        .expect("root");
    let c = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(c, Some(root));
    ui.initialize_tree(c);
    let h = ui
        .get_child(c, ElementId(text_id))
        .expect("the child was built");
    ui.text_element_mut(h).expect("a text half").set_text(text);
    h
}

/// The shipped chat log: `0x27`, selectable and not editable.
fn log(ui: &mut UiSystem, text: &str) -> ElemHandle {
    text_element(ui, text, dereth_ui::props::attr::TEXT_SELECTABLE)
}

/// The x of the boundary **before** glyph `i` — the pen position, which is the midpoint of the
/// window the hit test answers `i` for.
const fn at(i: i32) -> i32 {
    i * 8
}

/// Every invert rectangle the element emits.
fn inverts(ui: &mut UiSystem, h: ElemHandle) -> usize {
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    back.calls
        .iter()
        .filter(|c| c.who == h)
        .map(|c| c.invert.len())
        .sum()
}

/// Behaviour: chat.log.a-drag-selects-the-swept-text
///
/// **A press, a drag and a release select the text the pointer swept.**
///
/// The calibration is the press on its own: after the press the range is the empty one at the
/// anchor, which is *not* a selection anybody can see. The drag is what has to change it.
#[test]
fn a_press_drag_and_release_select_the_glyphs_the_pointer_swept() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let h = log(&mut ui, "abcdefgh");

    // 1. the press anchors, and anchors *empty*.
    ui.mouse_down(action::PRIMARY_CLICK, at(1), 8);
    assert_eq!(
        ui.text_element_mut(h).expect("text").get_selection(),
        Some((1, 1)),
        "the press turns selecting on and then sets the selection start to the caret, which \
         collapses the end onto the start"
    );
    assert_eq!(inverts(&mut ui, h), 0, "an empty range inverts nothing");

    // 2. the drag extends it — the move calls `set_cursor_position` with the default move type
    //    and the 0x40 selecting flag set, and its tail sets the selection end to the caret.
    ui.mouse_move(LocalTime(1.0), at(5), 8);
    assert_eq!(
        ui.text_element_mut(h).expect("text").get_selection(),
        Some((1, 5)),
        "the drag must move the selection *end* to the glyph under the pointer"
    );

    // 3. the release keeps it — clears 0x40 and nothing else; 0x80 and both
    //    endpoints survive, which is why a retail selection stays highlighted after the button
    //    comes up.
    ui.mouse_up(action::PRIMARY_CLICK, at(5), 8, false);
    let t = ui.text_element_mut(h).expect("text");
    assert_eq!(
        t.get_selection(),
        Some((1, 5)),
        "the release does not collapse the range"
    );
    assert_eq!(
        t.selected_text(),
        "bcde",
        "the selected text is the glyph list's text over [1, 5)"
    );
    assert_eq!(
        inverts(&mut ui, h),
        4,
        "one invert rectangle per selected glyph"
    );
}

/// **A backwards drag selects the same four glyphs**, because `get_selection` orders the
/// endpoints and `set_selection_end` does not.
///
/// This is the assertion a "clamp the end to be at least the start" shortcut fails.
#[test]
fn a_backwards_drag_selects_the_same_range() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let h = log(&mut ui, "abcdefgh");

    ui.mouse_down(action::PRIMARY_CLICK, at(5), 8);
    ui.mouse_move(LocalTime(1.0), at(1), 8);
    ui.mouse_up(action::PRIMARY_CLICK, at(1), 8, false);

    let t = ui.text_element_mut(h).expect("text");
    assert_eq!(t.get_selection(), Some((1, 5)), "min/max, not start/end");
    assert_eq!(
        t.selection.start, 5,
        "and the raw anchor is still where the press put it"
    );
    assert_eq!(t.selected_text(), "bcde");
}

/// **The release ends the gesture**: the element's `0x40` flag is cleared, so a later move with no
/// button down leaves the selection alone.
///
/// Without the clear, the selection would follow the pointer around the screen for ever after one
/// click — a defect that is invisible in a single-gesture test and obvious in the running client.
#[test]
fn a_move_after_the_release_does_not_move_the_selection() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let h = log(&mut ui, "abcdefgh");

    ui.mouse_down(action::PRIMARY_CLICK, at(1), 8);
    ui.mouse_move(LocalTime(1.0), at(5), 8);
    ui.mouse_up(action::PRIMARY_CLICK, at(5), 8, false);
    assert_eq!(
        ui.text_element_mut(h).expect("text").get_selection(),
        Some((1, 5))
    );

    ui.mouse_move(LocalTime(2.0), at(8), 8);
    assert_eq!(
        ui.text_element_mut(h).expect("text").get_selection(),
        Some((1, 5)),
        "0x40 (selecting) is clear: the move forwards to the base element and touches nothing"
    );
}

/// Behaviour: chat.log.a-drag-selects-the-swept-text
///
/// **A sweep over a *scrolled* log selects the glyphs that are on the screen.**
///
/// The hit test searches with the vertical scroll offset plus `y` and the horizontal scroll offset
/// plus `x`; the draw's placement moves the run by **minus** the same pair. Leave the
/// addition out and the two disagree by exactly the scroll offset — the pointer picks the glyph
/// that *would* be there if the log had never scrolled.
///
/// This is not a corner: a chat log follows the conversation, so it is scrolled from the second
/// screenful onwards and a sweep in the running client is always this case. One line of text at the
/// top of the box is enough to show it, because a one-line offset already swaps which sentence is
/// selected.
#[test]
fn a_sweep_over_a_scrolled_log_selects_what_is_under_the_pointer() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    // Two lines of eight, so the second is a different word from the first at every column.
    let h = log(&mut ui, "abcdefgh\nijklmnop");
    // A vertical scroll offset of 16 — one line height, exactly as one arrow click leaves it.
    ui.text_element_mut(h).expect("text").scroll.y = 16;

    // The top row of the pane now shows the *second* line.
    ui.mouse_down(action::PRIMARY_CLICK, at(1), 8);
    ui.mouse_move(LocalTime(1.0), at(5), 8);
    ui.mouse_up(action::PRIMARY_CLICK, at(5), 8, false);

    assert_eq!(
        ui.text_element_mut(h).expect("text").selected_text(),
        "jklm",
        "the sweep must read the scrolled layout; without adding the scroll offset to y it selects \
         `bcde`, the glyphs that scrolled off the top"
    );
}

/// **What one text element copies is what the next one pastes**, because
/// the copy goes to the device's *system* clipboard and not to a buffer on the
/// element.
///
/// Getting a line of chat back out of the log matters most for the entry box directly below it.
/// With a buffer per element that hop could not be made at all: `Copy` would fill the log's own
/// buffer and `Paste` read the entry's, which is empty.
///
/// Both halves run through [`UiSystem::dispatch_action`] with the real action ids
/// (`0x22` Copy, `0x24` Paste), so this also asserts the
/// route and not only the buffer.
#[test]
fn a_line_copied_from_the_log_pastes_into_the_entry_box() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let l = log(&mut ui, "abcdefgh");

    // Sweep four glyphs out of the log and copy them.
    ui.mouse_down(action::PRIMARY_CLICK, at(1), 8);
    ui.mouse_move(LocalTime(1.0), at(5), 8);
    ui.mouse_up(action::PRIMARY_CLICK, at(5), 8, false);
    assert_eq!(
        ui.text_element_mut(l).expect("text").selected_text(),
        "bcde"
    );
    ui.dispatch_action(
        l,
        &InputEvent {
            action: action::COPY,
            start: true,
            x: 0,
            y: 0,
        },
    );
    assert_eq!(
        ui.clipboard, "bcde",
        "the copy reached the device's clipboard"
    );

    // Paste into the *other* element: a second, editable box built after the sweep so that it
    // cannot have taken the press.
    let entry = text_element_with_ids(
        &mut ui,
        "",
        dereth_ui::props::attr::TEXT_EDITABLE,
        0x1000_0020,
        0x1000_0021,
    );
    ui.dispatch_action(
        entry,
        &InputEvent {
            action: action::PASTE,
            start: true,
            x: 0,
            y: 0,
        },
    );
    assert_eq!(
        ui.text_element_mut(entry)
            .expect("text")
            .glyphs
            .inq_text(false),
        "bcde",
        "and the entry box read the same buffer"
    );
    assert_eq!(
        ui.clipboard, "bcde",
        "a paste does not consume the clipboard"
    );
}

/// **A text element that is editable but not selectable never starts a drag.**
///
/// The press puts the whole selecting/`0x40` block behind the element's `0x4` (selectable) flag,
/// and `0x40` is what both the drag and the release test first. So an
/// element carrying `0x16` and not `0x27` takes the press, moves its caret and selects **nothing**.
///
/// This is the negative calibration for the three tests above: without it they would also pass
/// against a build that selected on any press at all. `0x16` rather than nothing at all because
/// mouse visibility is `editable || selectable` — a text element carrying
/// neither is not hit-testable and the press would never arrive, which would prove nothing.
#[test]
fn an_editable_box_that_is_not_selectable_cannot_be_swept() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let h = text_element(&mut ui, "abcdefgh", dereth_ui::props::attr::TEXT_EDITABLE);

    ui.mouse_down(action::PRIMARY_CLICK, at(1), 8);
    ui.mouse_move(LocalTime(1.0), at(5), 8);
    ui.mouse_up(action::PRIMARY_CLICK, at(5), 8, false);

    let t = ui.text_element_mut(h).expect("text");
    assert_eq!(t.cursor, 1, "the press did arrive and did move the caret");
    assert_eq!(
        t.get_selection(),
        None,
        "`get_selection` is false with 0x80 clear"
    );
    assert_eq!(t.selected_text(), "", "and `selected_text` copies nothing");
}

/// The attribute setter sends both `0x16` (editable) and `0x27`
/// (selectable) through one shared mouse-visibility refresh on its tail.
///
/// These are runtime property writes, not constructor bits: a text control becoming locked must
/// immediately stop winning the real hit test, and either capability becoming live must offer it
/// to the pointer again. The setters' unchanged-value early return does not skip the containing
/// attribute handler's tail.
#[test]
fn runtime_editable_and_selectable_writes_refresh_text_mouse_visibility() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let h = text_element(&mut ui, "abcdefgh", dereth_ui::props::attr::TEXT_EDITABLE);

    assert_eq!(
        ui.hit_test_screen(8, 8),
        Some(h),
        "the authored editable bit offers the box"
    );
    ui.mouse_down(action::PRIMARY_CLICK, 8, 8);
    assert_eq!(
        ui.focus_element(),
        Some(h),
        "the real press takes text focus"
    );
    ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_EDITABLE, true);
    assert_eq!(
        ui.focus_element(),
        Some(h),
        "an unchanged true write keeps the caret"
    );
    assert_eq!(
        ui.hit_test_screen(8, 8),
        Some(h),
        "the shared tail keeps unchanged true hittable"
    );

    ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_EDITABLE, false);
    assert_eq!(
        ui.focus_element(),
        None,
        "the existing set_editable transition relinquishes focus"
    );
    assert_eq!(
        ui.hit_test_screen(8, 8),
        None,
        "a locked plain text element stops taking presses"
    );
    ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_EDITABLE, false);
    assert_eq!(
        ui.hit_test_screen(8, 8),
        None,
        "an unchanged false write remains mouse-invisible"
    );

    ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_SELECTABLE, true);
    assert_eq!(
        ui.hit_test_screen(8, 8),
        Some(h),
        "selectable alone offers the box again"
    );
    ui.mouse_down(action::PRIMARY_CLICK, 8, 8);
    assert_eq!(
        ui.focus_element(),
        Some(h),
        "the selectable text receives the real press"
    );
    ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_SELECTABLE, true);
    assert_eq!(
        ui.focus_element(),
        Some(h),
        "unchanged selectable retains focus"
    );

    ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_SELECTABLE, false);
    assert_eq!(
        ui.focus_element(),
        None,
        "the existing set_selectable transition drops focus"
    );
    assert_eq!(
        ui.hit_test_screen(8, 8),
        None,
        "neither text capability leaves a hit target"
    );
    ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_SELECTABLE, false);
    assert_eq!(
        ui.hit_test_screen(8, 8),
        None,
        "the shared tail also preserves unchanged false"
    );

    ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_EDITABLE, true);
    assert_eq!(
        ui.hit_test_screen(8, 8),
        Some(h),
        "editable restores pointer eligibility"
    );
}

// ---------------------------------------------------------------------------------------------
// Shift-click extends the selection
// ---------------------------------------------------------------------------------------------

/// A selectable log holding twenty `line N` rows, the text the shift-click tests press on.
fn twenty_line_log(ui: &mut UiSystem) -> ElemHandle {
    let text: String = (0..20).map(|i| format!("line {i}\n")).collect();
    log(ui, &text)
}

/// Behaviour: ui.text.shift-click-extends-the-selection
///
/// **The text element's shift arm on a press**, and the seam that feeds it.
///
/// The press anchors on the current selection start when a range is live and on the old caret
/// otherwise, then moves the caret without selecting. With shift down it sets the selection to
/// run from that anchor to the new caret; without shift it starts a fresh, empty selection at the
/// new caret.
///
/// Both legs are driven from the same two clicks, which is what makes the shift leg a measurement
/// rather than a reading: without shift the second click *replaces* the selection and the range is
/// empty; with shift it extends, and the range is the two clicks apart.
///
/// The anchor is read **before** the caret moves. Reading it after would anchor every shift-click
/// on itself and select nothing, which still "works" and always produces an empty range.
#[test]
fn a_shift_click_extends_the_selection_from_where_it_began() {
    for shift in [false, true] {
        let mut ui = UiSystem::new((800, 600));
        let log = twenty_line_log(&mut ui);
        let mut pump = Pump { shift: false };
        // `UiSystem::use_time` latches `InputPump::shift_key_down` once a frame, which is where the
        // client polls it, so the press arm and the drag both
        // read the same latch.
        ui.use_time(LocalTime(1.0), &mut pump);

        // The player's gesture, in order: a plain click puts the caret and anchors the selection
        // there, then the second click either replaces it or extends it. Both presses are at
        // glyphs on the first line; 8 px per glyph in the default metrics.
        ui.mouse_down(action::PRIMARY_CLICK, 20, 8);
        let first = ui.text_element_mut(log).expect("alive").cursor;
        ui.mouse_up(action::PRIMARY_CLICK, 20, 8, false);

        pump.shift = shift;
        ui.use_time(LocalTime(2.0), &mut pump);
        ui.mouse_down(action::PRIMARY_CLICK, 52, 8);
        let t = ui.text_element_mut(log).expect("alive");
        let second = t.cursor;
        assert!(
            second > first,
            "the second click is further along the line: {first} -> {second}"
        );
        let (a, b) = t
            .get_selection()
            .expect("selecting is on after a press on a selectable");

        if shift {
            assert_eq!(
                (a, b),
                (first, second),
                "shift extends from the first click to the second"
            );
        } else {
            assert_eq!(
                a, b,
                "without shift the press starts a fresh, empty selection at the caret"
            );
            assert_eq!(a, second, "at the caret it just set");
        }
    }
}

/// The anchor's *other* source: with no selection live, flag bit `0x80` is clear and the
/// anchor is the **caret**, not the stored selection start — which is zero-initialised and would
/// silently anchor every first shift-click at the start of the text.
#[test]
fn with_no_live_selection_the_shift_anchor_is_the_caret_and_not_a_stale_selection_start() {
    let mut ui = UiSystem::new((800, 600));
    let log = twenty_line_log(&mut ui);
    let mut pump = Pump { shift: true };
    ui.use_time(LocalTime(1.0), &mut pump);

    // Put the caret somewhere without arming a selection, the way an arrow key would.
    {
        let t = ui.text_element_mut(log).expect("alive");
        t.cursor = 4;
        t.set_selecting(false);
        assert!(!t.bits.selecting(), "nothing is selected");
        assert_eq!(
            t.selection.start, 0,
            "and the selection start is its zero-initialised value"
        );
    }
    ui.mouse_down(action::PRIMARY_CLICK, 60, 8);
    let t = ui.text_element_mut(log).expect("alive");
    let (a, b) = t.get_selection().expect("the press armed a selection");
    assert_eq!(
        a, 4,
        "the anchor is the caret the press found, not the stale selection start of 0"
    );
    assert!(b > a, "and it extends to the click: {a}..{b}");
}
