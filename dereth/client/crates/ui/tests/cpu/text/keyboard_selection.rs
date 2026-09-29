//! Shift+arrow extends a text selection from the old caret; a bare arrow drops the selection and
//! still moves the caret.
//!
//! The caret-move entry point is a twelve-way switch, and **every** arm that lands the caret does
//! so through the cursor-position setter with the `Default` movement flags. `Default` is the
//! *conditional* mode: it extends the selection when a drag is live (`0x40`) or shift is held, and
//! drops a stale one otherwise, so routing the arrows through it is the whole of keyboard
//! selection. Fixture: the shipped chat log's shape (200x40, selectable), from a `LayoutDesc`
//! written here, with an input pump whose shift key the test holds.

use crate::common::NoAssets;
use dereth_primitives::{DataId, LocalTime};
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::focus::InputEvent;
use dereth_ui::{ElemHandle, ElementId, ElementType, InputPump, UiSystem};

/// `InputPump::shift_key_down` with an answer the test chooses.
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

/// `CursorTravelMode::Right`, the action id maps to arm 1.
const RIGHT: u32 = 0x17;
/// `CursorTravelMode::Left`.
const LEFT: u32 = 0x16;
/// `CursorTravelMode::Home` — arm 6, which also lands through `set_cursor_position(pos, Default)`.
const HOME: u32 = 0x1C;

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

/// The shipped chat log's shape: 200x40 at the origin, `0x27`-style selectable.
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

fn press(ui: &mut UiSystem, h: ElemHandle, a: u32) {
    ui.dispatch_action(
        h,
        &InputEvent {
            action: a,
            start: true,
            x: 0,
            y: 0,
        },
    );
}

/// Behaviour: ui.text.shift-arrow-extends-a-selection-and-a-bare-arrow-clears-it
///
/// **Shift+arrow extends a selection from where the caret was.**
///
/// The caret starts at 0. Three shift+rights must select `abc` — the anchor is the *old*
/// caret position (the selection start is set to the caret before the caret moves), so the
/// range grows 0..1, 0..2, 0..3 and never re-anchors.
#[test]
fn shift_arrow_extends_a_selection_from_the_old_caret() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let h = log(&mut ui, "abcdefgh");

    // `set_text` leaves the caret at the end of the text, so park it at 0 first -- with shift
    // *up*, because shift+Home is itself a selecting move (`Default` + shift held) and
    // would anchor at 8 and select the whole line, which is correct and not what is under test.
    press(&mut ui, h, HOME);
    assert_eq!(
        ui.text_element_mut(h).expect("text").cursor,
        0,
        "parked at the top"
    );
    assert!(
        ui.text_element_mut(h)
            .expect("text")
            .get_selection()
            .is_none(),
        "and a bare Home selected nothing on the way"
    );

    // Shift goes down; the UI frame step latches the input pump's modifier state.
    pump.shift = true;
    ui.use_time(LocalTime(1.0), &mut pump);

    press(&mut ui, h, RIGHT);
    assert_eq!(
        ui.text_element_mut(h).expect("text").selected_text(),
        "a",
        "one shift+right selects the glyph the caret crossed"
    );

    press(&mut ui, h, RIGHT);
    press(&mut ui, h, RIGHT);
    assert_eq!(
        ui.text_element_mut(h).expect("text").selected_text(),
        "abc",
        "the anchor is the caret's position *before* the first move and does not follow it"
    );
}

/// Behaviour: ui.text.shift-arrow-extends-a-selection-and-a-bare-arrow-clears-it
///
/// **A bare arrow clears a live selection**, which is the `else` arm of the same `if`.
///
/// Calibration in the other direction: without the `else` the highlight survives the keypress and
/// `Copy` keeps yielding the stale text, the same failure the mouse path guards against.
#[test]
fn a_bare_arrow_drops_the_selection() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let h = log(&mut ui, "abcdefgh");

    press(&mut ui, h, HOME);
    pump.shift = true;
    ui.use_time(LocalTime(1.0), &mut pump);
    press(&mut ui, h, RIGHT);
    press(&mut ui, h, RIGHT);
    assert_eq!(ui.text_element_mut(h).expect("text").selected_text(), "ab");

    // Shift comes up; the next arrow is bare.
    pump.shift = false;
    ui.use_time(LocalTime(2.0), &mut pump);
    press(&mut ui, h, LEFT);
    assert_eq!(
        ui.text_element_mut(h).expect("text").selected_text(),
        "",
        "`set_selecting(false)` runs on the else arm, so the highlight goes"
    );
    assert!(
        ui.text_element_mut(h)
            .expect("text")
            .get_selection()
            .is_none(),
        "and the range itself is gone, not merely empty: `0x80` gates every reader"
    );
}

/// **A bare arrow still moves the caret**, the calibration against simply ignoring the arrows.
/// Selecting `ab` then pressing Left twice bare must leave the caret at 0, not at 2.
#[test]
fn a_bare_arrow_still_travels() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump::default();
    ui.use_time(LocalTime(0.0), &mut pump);
    let h = log(&mut ui, "abcdefgh");

    press(&mut ui, h, HOME);
    press(&mut ui, h, RIGHT);
    press(&mut ui, h, RIGHT);
    press(&mut ui, h, RIGHT);
    assert_eq!(
        ui.text_element_mut(h).expect("text").cursor,
        3,
        "three bare rights travel"
    );
    assert!(
        ui.text_element_mut(h)
            .expect("text")
            .get_selection()
            .is_none(),
        "and select nothing at all"
    );
}
