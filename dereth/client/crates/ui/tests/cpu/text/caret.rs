//! The blinking caret in every edit box: a focused, editable text element draws a one-pixel bar in
//! the font colour at the insertion point, after its glyphs, and it blinks at the desktop's caret
//! rate. The draw fills it when the element has focus and flag bits `0x1` and `0x200` are both set
//! (editable, flash on); the element's tick on global message 3 toggles `0x200` every caret-blink
//! interval, or forces it on when the caret moved since the last flip, so typing holds it solid. An
//! empty box draws its caret at the justified origin, one line high; an unfocused or non-editable
//! box draws none; the bar follows the caret, not the end of the text.
//!
//! Fixture: a text box from a `LayoutDesc` written here, with the default fixed metrics (8 px
//! advance, 16 px line). No dats, no network.

use crate::common::NoAssets;
use dereth_primitives::{DataId, LocalTime};
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::{
    ElemHandle, ElementId, ElementType, InputPump, RecordingDrawBackend, UiFill, UiSystem,
};

#[derive(Debug, Default)]
struct Pump;
impl InputPump for Pump {
    fn use_time(&mut self, _now: LocalTime) {}
}

const CONTAINER: u32 = 0x1000_0010;
const BOX: u32 = 0x1000_0011;
/// `FixedMetrics { advance: 8, height: 16 }`, the element's default with no dat font.
const ADVANCE: i32 = 8;
const LINE: i32 = 16;
/// The current font colour's default — white, opaque.
const FONT_COLOR: u32 = 0xFFFF_FFFF;

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

/// A 200x40 text box at the origin of a 400x200 field, editable when asked (attribute `0x16`) —
/// the shape of every chat entry and name field.
fn text_box(ui: &mut UiSystem, editable: bool, text: &str) -> ElemHandle {
    let mut container = desc(CONTAINER, ty::FIELD, 400, 200);
    let mut t = desc(BOX, ty::TEXT, 200, 40);
    if editable {
        t.base.properties.set(
            dereth_ui::props::attr::TEXT_EDITABLE,
            dereth_ui::PropertyValue::Bool(true),
        );
    }
    container.children.insert(ElementId(BOX), t);
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
        .get_child(c, ElementId(BOX))
        .expect("the child was built");
    ui.text_element_mut(h).expect("a text half").set_text(text);
    h
}

/// One UI frame step, which broadcasts global message 3.
fn tick(ui: &mut UiSystem, t: f64) {
    ui.use_time(LocalTime(t), &mut Pump);
}

/// Everything the draw list says about `h`: the index of its glyph command and every visible
/// flat fill it emitted, with the index of the command that carried it.
struct Drawn {
    glyph_cmd: Option<usize>,
    fills: Vec<(usize, UiFill)>,
}

fn draw(ui: &mut UiSystem, h: ElemHandle) -> Drawn {
    let mut back = RecordingDrawBackend::default();
    ui.draw(&mut back);
    let glyph_cmd = back
        .calls
        .iter()
        .position(|c| c.who == h && !c.glyphs.is_empty());
    let fills = back
        .calls
        .iter()
        .enumerate()
        .filter(|(_, c)| c.who == h)
        .flat_map(|(i, c)| {
            c.fills
                .iter()
                .filter(|f| !f.is_invisible())
                .map(move |f| (i, *f))
        })
        .collect();
    Drawn { glyph_cmd, fills }
}

fn caret(x: i32, y: i32) -> UiFill {
    UiFill {
        x,
        y,
        w: 1,
        h: LINE,
        color: FONT_COLOR,
    }
}

/// Behaviour: ui.caret.a-focused-editable-box-draws-a-blinking-one-pixel-caret
///
/// **The caret is drawn where the retail client draws it, and it blinks at the caret rate.**
#[test]
fn a_focused_editable_box_draws_a_one_pixel_caret_that_blinks() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let h = text_box(&mut ui, true, "ab");

    // Calibration: unfocused, nothing — having focus is the first of the draw's three gates.
    assert!(draw(&mut ui, h).fills.is_empty(), "no focus, no caret");

    ui.take_focus(h);
    assert_eq!(ui.focus_element(), Some(h));
    // `set_text` went through the internal text append, which stamped the last caret-move time, so
    // the first tick after it finds the move newer than the last flip and forces the caret ON.
    tick(&mut ui, 1.0);
    let d = draw(&mut ui, h);
    // `set_text` leaves the caret at the end: after two 8 px glyphs, on the top line.
    assert_eq!(
        d.fills,
        vec![(
            d.glyph_cmd.expect("the glyphs were drawn") + 1,
            caret(2 * ADVANCE, 0)
        )]
    );

    // Inside the half-period nothing changes (`dt < blink -> return`).
    tick(&mut ui, 1.4);
    assert_eq!(draw(&mut ui, h).fills.len(), 1, "still on at +0.4 s");

    // Past it the bit toggles off, and the draw list carries no fill at all for the box.
    tick(&mut ui, 1.6);
    assert!(draw(&mut ui, h).fills.is_empty(), "off at +0.6 s");

    // And on again a half-period later.
    tick(&mut ui, 2.2);
    assert_eq!(
        draw(&mut ui, h).fills,
        vec![(d.glyph_cmd.unwrap() + 1, caret(2 * ADVANCE, 0))]
    );
}

/// **The caret paints over the glyphs, not under them.** The caret fill follows the complete
/// glyph loop, so the bar wins the column it shares
/// with the glyph it sits against. In the draw list that is "a later command than the glyphs",
/// because a command's own fills are painted before its glyphs (the element's erase step).
#[test]
fn the_caret_is_a_command_after_the_glyphs_and_before_the_next_element() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let h = text_box(&mut ui, true, "ab");
    ui.take_focus(h);
    tick(&mut ui, 1.0);
    let mut back = RecordingDrawBackend::default();
    ui.draw(&mut back);
    let glyphs = back
        .calls
        .iter()
        .position(|c| c.who == h && !c.glyphs.is_empty())
        .expect("glyphs");
    let bar = back
        .calls
        .iter()
        .position(|c| c.who == h && c.fills.iter().any(|f| !f.is_invisible()))
        .expect("the caret command");
    assert_eq!(
        bar,
        glyphs + 1,
        "the caret is the very next command after the element's glyphs"
    );
    assert!(back.calls[bar].glyphs.is_empty() && back.calls[bar].invert.is_empty());
}

/// **Typing holds the caret solid.** The internal text append stamps the last caret-move time; the
/// next global tick sees it newer than the flip and forces the bit on, restarting the half-period —
/// which is why a caret being typed with never blinks off under the keys. Driven through
/// `UiSystem::character`, the `WM_CHAR` seam, not by poking the element.
#[test]
fn typing_forces_the_caret_on_and_moves_it_to_the_new_insertion_point() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let h = text_box(&mut ui, true, "ab");
    ui.take_focus(h);
    tick(&mut ui, 1.0);
    tick(&mut ui, 1.6);
    assert!(
        draw(&mut ui, h).fills.is_empty(),
        "off before the keystroke"
    );

    ui.character(u16::from(b'c'));
    assert_eq!(
        ui.text_element_mut(h).unwrap().glyphs.inq_text(false),
        "abc"
    );
    // Only 0.1 s since the last flip — far inside the half-period — yet the caret is on again,
    // and one glyph further right.
    tick(&mut ui, 1.7);
    let d = draw(&mut ui, h);
    assert_eq!(
        d.fills.iter().map(|(_, f)| *f).collect::<Vec<_>>(),
        vec![caret(3 * ADVANCE, 0)]
    );
    // The restarted half-period runs from 1.7, so at 2.1 it is still on and at 2.3 it is off.
    tick(&mut ui, 2.1);
    assert_eq!(
        draw(&mut ui, h).fills.len(),
        1,
        "on at +0.4 s after the keystroke"
    );
    tick(&mut ui, 2.3);
    assert!(
        draw(&mut ui, h).fills.is_empty(),
        "off at +0.6 s after the keystroke"
    );
}

/// **An empty box has a caret too.** The glyph list's recalculation always pushes a final
/// line, so an empty box has one zero-height line. The draw passes
/// `max(font max character height, scrollable height)` to the vertical justification and
/// keeps the font's max character height as the caret height when the stored line height is zero.
/// The chat entry is empty most of the time, so this is the case a player sees first.
#[test]
fn an_empty_box_draws_its_caret_at_the_justified_origin_one_line_high() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let h = text_box(&mut ui, true, "");
    ui.take_focus(h);
    tick(&mut ui, 1.0);
    let d = draw(&mut ui, h);
    assert_eq!(d.glyph_cmd, None, "nothing to compose");
    assert_eq!(
        d.fills.iter().map(|(_, f)| *f).collect::<Vec<_>>(),
        vec![caret(0, 0)]
    );

    // Attribute 0x15 reaches. With no text the scrollable
    // height is zero, so retail centres the font-height extent rather than the stored zero-height
    // line: `(40 >> 1) - (16 >> 1) = 12`.
    ui.set_attribute_enum(h, dereth_ui::props::attr::TEXT_V_JUSTIFY, 1);
    assert_eq!(ui.text_element_mut(h).expect("the text box").v_justify, 1);
    let centred = draw(&mut ui, h);
    assert_eq!(
        centred.fills.iter().map(|(_, f)| *f).collect::<Vec<_>>(),
        vec![caret(0, 12)],
        "a zero-height line is justified with the font floor and the caret remains 16 px high"
    );
}

/// Behaviour: ui.caret.a-focused-editable-box-draws-a-blinking-one-pixel-caret
///
/// **The other two gates.** A focused box that is not editable (flag bit `0x1` clear — a chat
/// log, a label that took focus on a click) draws no caret; neither does an editable
/// box that has lost focus, however the bit was left.
#[test]
fn no_caret_without_editability_or_focus() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let h = text_box(&mut ui, false, "ab");
    ui.take_focus(h);
    tick(&mut ui, 1.0);
    assert!(
        draw(&mut ui, h).fills.is_empty(),
        "focused but not editable: no caret"
    );

    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let h = text_box(&mut ui, true, "ab");
    ui.take_focus(h);
    tick(&mut ui, 1.0);
    assert_eq!(
        draw(&mut ui, h).fills.len(),
        1,
        "focused and editable: caret"
    );
    ui.set_focus_element(None);
    assert!(
        draw(&mut ui, h).fills.is_empty(),
        "focus gone: no caret, whatever the bit says"
    );
}

/// **The geometry follows the caret, not the end of the text.** Home puts the caret at 0,
/// and the bar moves to the pen before the first glyph.
#[test]
fn the_caret_sits_at_the_pen_before_the_glyph_at_cursor_pos() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let h = text_box(&mut ui, true, "ab");
    ui.take_focus(h);
    // `CursorTravelMode::Home` — action `0x1C`, arm 6.
    ui.dispatch_action(
        h,
        &dereth_ui::focus::InputEvent {
            action: 0x1C,
            start: true,
            x: 0,
            y: 0,
        },
    );
    tick(&mut ui, 1.0);
    let d = draw(&mut ui, h);
    assert_eq!(
        d.fills.iter().map(|(_, f)| *f).collect::<Vec<_>>(),
        vec![caret(0, 0)]
    );
}
