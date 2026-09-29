//! `Copy` on a text element hands the host the selected text to put on the operating system's
//! clipboard, once per copy; with nothing selected it pushes nothing and leaves the clipboard
//! alone; `cut` pushes the same text and deletes it; the read-only chat log refuses a paste
//! whatever the clipboard mirror holds.
//!
//! `dereth-ui` has no window and cannot call the operating system, so the hop is deferred the way
//! the cursor is (`UiSystem::take_pending_cursor`): the element records what the client must send
//! and the client sends it. This file asserts that record, the layer immediately above the system
//! call; that the text reaches the real clipboard is `dereth-clipboard`'s `cpu::roundtrip`, ignored
//! by default because it would replace the machine's clipboard.
//!
//! Copy is one gate and two calls: with the selecting bit (`0x80`) clear it does nothing at all;
//! with it set it reads the selected text and hands it to the device call that writes the host's
//! clipboard. Fixture: the shipped chat log's shape (selectable, not editable), from a `LayoutDesc`
//! written here.

use crate::common::NoAssets;
use dereth_primitives::{DataId, LocalTime};
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::focus::{action, InputEvent};
use dereth_ui::{ElemHandle, ElementId, ElementType, InputPump, UiSystem};

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

/// The shipped chat log's shape and attributes: selectable, not editable (`0x27`).
fn log(ui: &mut UiSystem, text: &str) -> ElemHandle {
    let mut container = desc(CONTAINER, ty::FIELD, 400, 200);
    let mut t = desc(LOG, ty::TEXT, 200, 40);
    t.base.properties.set(
        dereth_ui::props::attr::TEXT_SELECTABLE,
        dereth_ui::PropertyValue::Bool(true),
    );
    container.children.insert(ElementId(LOG), t);
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
    ui.initialize_tree(c);
    let h = ui
        .get_child(c, ElementId(LOG))
        .expect("the child was built");
    ui.text_element_mut(h).expect("a text half").set_text(text);
    h
}

/// The x of the boundary before glyph `i` — the default advance is 8px.
const fn at(i: i32) -> i32 {
    i * 8
}

/// Sweep glyphs `[a, b)` out of the log with the mouse.
fn sweep(ui: &mut UiSystem, a: i32, b: i32) {
    ui.mouse_down(action::PRIMARY_CLICK, at(a), 8);
    ui.mouse_move(LocalTime(1.0), at(b), 8);
    ui.mouse_up(action::PRIMARY_CLICK, at(b), 8, false);
}

/// Behaviour: clipboard.copy.hands-the-selection-to-the-host-once
///
/// **Selecting chat text and pressing Copy leaves the host something to send to the operating
/// system's clipboard.**
///
/// Filling only [`UiSystem::clipboard`] would be *invisible from outside the process*: the
/// selection highlights, the copy appears to work, and nothing can be pasted into another
/// application.
#[test]
fn copy_hands_the_host_a_pending_clipboard_write() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let l = log(&mut ui, "abcdefgh");

    sweep(&mut ui, 1, 5);
    assert_eq!(
        ui.text_element_mut(l).expect("text").selected_text(),
        "bcde",
        "the sweep"
    );

    assert_eq!(
        ui.take_pending_clipboard(),
        None,
        "nothing is pending before the Copy"
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
        ui.take_pending_clipboard(),
        Some("bcde".to_string()),
        " must reach the host, not only the in-process mirror"
    );
    assert_eq!(
        ui.clipboard, "bcde",
        "and the in-process mirror stays, because `Paste` reads it without a round trip"
    );
}

/// Behaviour: clipboard.copy.hands-the-selection-to-the-host-once
///
/// **Draining is a take, not a peek** — the client must not re-send the same text every frame.
///
/// `SetClipboardData` calls `EmptyClipboard` first and bumps the clipboard sequence number, so a
/// re-send once a frame would make this client fight every other application for the clipboard.
#[test]
fn the_pending_write_is_drained_once() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let l = log(&mut ui, "abcdefgh");

    sweep(&mut ui, 1, 5);
    ui.dispatch_action(
        l,
        &InputEvent {
            action: action::COPY,
            start: true,
            x: 0,
            y: 0,
        },
    );
    assert_eq!(ui.take_pending_clipboard(), Some("bcde".to_string()));
    assert_eq!(
        ui.take_pending_clipboard(),
        None,
        "the second frame sends nothing"
    );
}

/// **The `0x80` gate.** The test skips the entire body, so an element with no live selection
/// never reaches the device's clipboard call at all — it does not clear the clipboard either.
///
/// This is the calibration against pushing on every Copy keystroke: pressing Ctrl+C with nothing
/// selected must leave whatever the user copied out of another application exactly where it was.
#[test]
fn copy_with_nothing_selected_pushes_nothing() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let l = log(&mut ui, "abcdefgh");

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
        ui.take_pending_clipboard(),
        None,
        "the gate skipped the body; the user's clipboard is not emptied by a stray Ctrl+C"
    );
}

/// **`cut` is `copy` then `delete_selection`**, so it pushes too.
///
/// Driven on an editable box, because cutting out of the read-only chat log deletes nothing.
#[test]
fn cut_pushes_the_same_text_copy_would() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);

    let mut container = desc(CONTAINER, ty::FIELD, 400, 200);
    let mut t = desc(LOG, ty::TEXT, 200, 40);
    t.base.properties.set(
        dereth_ui::props::attr::TEXT_SELECTABLE,
        dereth_ui::PropertyValue::Bool(true),
    );
    t.base.properties.set(
        dereth_ui::props::attr::TEXT_EDITABLE,
        dereth_ui::PropertyValue::Bool(true),
    );
    container.children.insert(ElementId(LOG), t);
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
    ui.initialize_tree(c);
    let h = ui
        .get_child(c, ElementId(LOG))
        .expect("the child was built");
    ui.text_element_mut(h).expect("text").set_text("abcdefgh");

    sweep(&mut ui, 1, 5);
    ui.dispatch_action(
        h,
        &InputEvent {
            action: action::CUT,
            start: true,
            x: 0,
            y: 0,
        },
    );
    assert_eq!(ui.take_pending_clipboard(), Some("bcde".to_string()));
    assert_eq!(
        ui.text_element_mut(h).expect("text").glyphs.inq_text(false),
        "afgh",
        "and the selection went"
    );
}

/// **The selectable-only chat log refuses a paste, although the mirror holds the whole machine's
/// clipboard.**
///
/// The paste's entire body is inside a test of the editable bit. The mirror holds whatever the
/// player last copied *anywhere*, so a missing gate would splice arbitrary outside text into the
/// conversation history. Input-map registration gives a selectable-only element input map
/// **8**, which is where the paste action `0x24` lives, so the keystroke really does arrive.
#[test]
fn the_chat_log_refuses_a_paste_even_with_the_mirror_full() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let l = log(&mut ui, "abcdefgh");

    // Whatever the player copied out of a browser, mirrored in by the client's per-frame refresh.
    ui.clipboard = "text from somewhere else".to_string();

    ui.dispatch_action(
        l,
        &InputEvent {
            action: action::PASTE,
            start: true,
            x: 0,
            y: 0,
        },
    );

    assert_eq!(
        ui.text_element_mut(l).expect("text").glyphs.inq_text(false),
        "abcdefgh",
        "a non-editable 0x27 chat log ignores paste and preserves its text"
    );
    assert_eq!(
        ui.clipboard, "text from somewhere else",
        "and the clipboard is untouched"
    );
}
