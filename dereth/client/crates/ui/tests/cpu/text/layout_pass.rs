//! The text layout pass runs from the draw: writing a text element's text only raises the `0x100`
//! dirty bit, and the draw re-measures every dirty element before it is drawn, so a pane whose text
//! outgrows its box reports its extent after one draw, with or without a scrollbar bound. The pass
//! stores the line table, clears the dirty bit, writes the truncation state (`-1` without the
//! `0x800` truncate bit), and truncates a one-line element at its budget with the suffix. The two
//! glyph-list lookups the truncation needs, and the round argument that separates the caret from
//! the truncation, are pinned too.
//!
//! Fixture: one text element in a container, from a `LayoutDesc` written here, with fixed metrics
//! of 8 px per character and 16 px per line. The shipped panels that rely on this are covered in
//! `dereth-ui-screens`. No network.

use std::sync::Arc;

use crate::common::NoAssets;
use dereth_primitives::DataId;
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::text::{FixedMetrics, GlyphList};
use dereth_ui::{ElemHandle, ElementId, ElementType, RecordingDrawBackend, UiSystem};

/// 8 px per character, 16 px per line — the metrics every station measures against, so a width in
/// this file is a character count and a height is a line count.
fn metrics() -> Arc<dyn dereth_ui::text::FontMetrics> {
    Arc::new(FixedMetrics {
        advance: 8,
        height: 16,
    })
}

fn desc(id: u32, t: ElementType, x: i32, y: i32, w: i32, h: i32) -> ElementDesc {
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
        ty: t,
        ..ElementDesc::default()
    }
}

const CONTAINER: u32 = 0x1000_0010;
const PANE: u32 = 0x1000_0011;

/// A container with one `TextElement` of `w x h` inside it, named `0x10000011`, with **no**
/// scrollbar bound unless a station binds one.
fn tree(w: i32, h: i32) -> (UiSystem, ElemHandle) {
    let mut ui = UiSystem::new((800, 600));
    let mut container = desc(CONTAINER, ty::FIELD, 0, 0, w + 20, h + 20);
    container
        .children
        .insert(ElementId(PANE), desc(PANE, ty::TEXT, 0, 0, w, h));
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
    let e = ui.get_child(c, ElementId(PANE)).expect("the pane");
    if let Some(t) = ui.text_element_mut(e) {
        t.metrics = metrics();
    }
    (ui, e)
}

fn draw(ui: &mut UiSystem) {
    let mut back = RecordingDrawBackend::default();
    ui.draw(&mut back);
}

// =============================================================================================
// 1. The pass runs from the draw, and from nothing else
// =============================================================================================

/// Behaviour: ui.text.a-draw-measures-dirty-text-before-it-is-drawn
///
/// **A pane whose text grows past its box gets its scroll extent without a hand call.**
///
/// The only thing between `set_text` and the measured extent here is `UiSystem::draw`.
#[test]
fn a_pane_whose_text_outgrows_its_box_reports_its_extent_after_one_draw() {
    let (mut ui, e) = tree(80, 32);
    let long = "a b c d e f g h i j k l m n o p q r s t u v w x y z";
    ui.text_element_mut(e)
        .expect("a text element")
        .set_text(long);

    let before = ui.text_element_mut(e).expect("alive").scroll.height;
    assert_eq!(
        before, 0,
        "set_text raises the 0x100 dirty bit and measures nothing -- if this is already \
         non-zero the station cannot tell the draw from the write"
    );

    draw(&mut ui);

    let after = ui.text_element_mut(e).expect("alive").scroll.height;
    assert!(
        after > 32,
        "the region draw runs the glyph-list recalculation on every visible region; the pane \
         holds more than two 16 px lines in a 32 px box and reports {after}"
    );
}

/// Behaviour: ui.text.a-draw-measures-dirty-text-before-it-is-drawn
///
/// **An element that names no scrollbar is measured too.** `recalculate_glyph_list` does not
/// test for a scrollbar (the chat entry box `0x10000016` names none): it gates on the `0x100`
/// dirty bit alone.
#[test]
fn an_element_that_names_no_scrollbar_is_measured_too() {
    let (mut ui, e) = tree(80, 32);
    let t = ui.text_element_mut(e).expect("a text element");
    assert!(
        t.scroll.h_scrollbar.is_none() && t.scroll.v_scrollbar.is_none(),
        "the premise: this element names no bar at all"
    );
    t.set_text("a b c d e f g h i j k l m n o p");
    draw(&mut ui);
    assert!(
        ui.text_element_mut(e).expect("alive").scroll.height > 0,
        "the sweep used to require a bound scrollbar and skipped every entry box in the tree"
    );
}

/// The glyph list's recalculation **stores** the line table, and the pass runs it, so the scroll
/// extent and the caret read the lines the draw measured.
#[test]
fn the_draw_leaves_the_stored_line_table_agreeing_with_the_box() {
    let (mut ui, e) = tree(80, 64);
    ui.text_element_mut(e)
        .expect("alive")
        .set_text("aaaa bbbb cccc dddd eeee ffff");
    assert!(
        ui.text_element_mut(e)
            .expect("alive")
            .glyphs
            .lines
            .is_empty(),
        "nothing has wrapped this yet"
    );
    draw(&mut ui);
    let t = ui.text_element_mut(e).expect("alive");
    assert!(
        t.glyphs.lines.len() > 1,
        "a 29-character string does not fit 80 px of 8 px glyphs"
    );
    assert_eq!(
        t.glyphs.last_recalc_width, 80,
        "the recalculation is handed box.x1 - right margin - left margin - box.x0 + 1"
    );
}

/// The dirty gate itself: a second draw with nothing changed re-measures nothing.
/// The recalculation clears `0x100` on its way out and refuses to re-enter without it.
#[test]
fn a_second_draw_with_no_write_re_measures_nothing() {
    let (mut ui, e) = tree(80, 32);
    ui.text_element_mut(e)
        .expect("alive")
        .set_text("aaaa bbbb cccc");
    draw(&mut ui);
    assert!(
        !ui.text_element_mut(e).expect("alive").bits.dirty(),
        "the pass clears the dirty bit"
    );
    let root = ui.root();
    let n = dereth_ui::scrollable::recalculate_dirty_text(&mut ui, root);
    assert_eq!(
        n, 0,
        "nothing is dirty, so recalculate_glyph_list returns at its first test"
    );
}

// =============================================================================================
// 2. The truncation state
// =============================================================================================

/// The `else` arm: an element without the `0x800` truncate bit has its truncation state
/// **written**, to `-1`.
#[test]
fn an_element_without_the_truncate_bit_is_reset_rather_than_left_alone() {
    let (mut ui, e) = tree(80, 32);
    {
        let t = ui.text_element_mut(e).expect("alive");
        t.truncation_pos = 7;
        t.cx_adjusted_line_number = 3;
        t.set_text("aaaa");
    }
    draw(&mut ui);
    let t = ui.text_element_mut(e).expect("alive");
    assert_eq!(
        (t.truncation_pos, t.cx_adjusted_line_number),
        (-1, -1),
        "both are reset to -1"
    );
}

/// The one-line arm, which is the arm **105 of the 106 shipped elements that arm `0x800`
/// take**.
///
/// A 10-character box (80 px), an ellipsis of one character (8 px), and 20 characters of text:
/// the budget is `80 - 0 - 0 - 8 = 72`. The scan's test is `if (pixels <= glyph.width) break`
/// (strict), so **eight** glyphs are kept and not nine — 72 px buys eight 8 px glyphs
/// and stops with exactly one glyph's worth left. `cx_adjusted_line_size` is then `8*8 + 8 = 72`.
#[test]
fn a_one_line_element_that_overflows_truncates_at_the_budget_and_draws_the_suffix() {
    let (mut ui, e) = tree(80, 16);
    {
        let t = ui.text_element_mut(e).expect("alive");
        t.bits.set_one_line(true);
        t.glyphs.one_line = true;
        t.bits.set_truncate(true);
        t.truncation_suffix = ".".to_owned();
        t.cx_trailer = 8;
        t.set_text("abcdefghijklmnopqrst");
    }
    draw(&mut ui);
    let box_ = ui.screen_box(e);
    let t = ui.text_element_mut(e).expect("alive");
    assert_eq!(
        t.truncation_pos, 8,
        "the 72 px budget of line 0 runs out after eight glyphs"
    );
    assert_eq!(
        t.cx_adjusted_line_number, 0,
        "the truncation is on line 0, the only line there is"
    );
    assert_eq!(
        t.cx_adjusted_line_size, 72,
        "the kept glyphs plus the trailer"
    );
    let drawn: String = t
        .compose(box_)
        .into_iter()
        .filter_map(|g| char::from_u32(u32::from(g.ch)))
        .collect();
    assert_eq!(
        drawn, "abcdefgh.",
        "the draw swaps to the truncation suffix at truncation_pos and draws nothing after it"
    );
}

/// The negative control for the same arm: text that fits is not truncated, and its truncation
/// state is the `-1` the fits-in-the-box early-out writes.
#[test]
fn a_one_line_element_that_fits_is_not_truncated() {
    let (mut ui, e) = tree(80, 16);
    {
        let t = ui.text_element_mut(e).expect("alive");
        t.bits.set_one_line(true);
        t.glyphs.one_line = true;
        t.bits.set_truncate(true);
        t.truncation_suffix = ".".to_owned();
        t.cx_trailer = 8;
        t.set_text("abcd");
    }
    draw(&mut ui);
    let box_ = ui.screen_box(e);
    let t = ui.text_element_mut(e).expect("alive");
    assert_eq!(t.truncation_pos, -1);
    let drawn: String = t
        .compose(box_)
        .into_iter()
        .filter_map(|g| char::from_u32(u32::from(g.ch)))
        .collect();
    assert_eq!(
        drawn, "abcd",
        "and no ellipsis is added to a label that fits"
    );
}

// =============================================================================================
// 3. The two `GlyphList` functions the arm needed
// =============================================================================================

fn lines_of(text: &str, width: i32) -> GlyphList {
    let mut g = GlyphList::default();
    g.add_text(0, text, metrics().as_ref(), 0xFFFF_FFFF, 0, None);
    g.recalculate(width);
    g
}

/// The glyph list's complete-line-from-y, **including the client's fall-through off-by-one**.
///
/// 16 px lines. `y = 40` fits two whole lines, so the last complete one is index 1. `y = 999` fits
/// them all and the client answers the line count — **not** count `- 1` — which its own caller then
/// uses as an *inclusive* bound. It is reproduced, not fixed.
#[test]
fn find_complete_line_from_y_answers_the_last_whole_line_and_retails_off_by_one() {
    let g = lines_of("aaaa bbbb cccc dddd eeee ffff", 40);
    let n = g.lines.len();
    assert!(
        n > 3,
        "the premise: the text wraps to more than three 16 px lines, not {n}"
    );
    assert_eq!(
        g.find_complete_line_from_y(40),
        Some(1),
        "40 px fits two whole 16 px lines"
    );
    assert_eq!(
        g.find_complete_line_from_y(999),
        Some(n),
        "one past the last index: a y that fits them all answers the line count itself"
    );
}

/// The glyph list's position-from-line-and-pixels — the **whole-line-fits** arm, which the
/// caret's two-argument copy never had.
///
/// `pixels >` the line's width takes that arm. On the last line the answer is the glyph count;
/// on any other it is the next line's start position `- 1`, which drops the break
/// character the wrap left at the end of the kept line, so trailing spaces are trimmed.
#[test]
fn find_pos_from_line_and_pixels_drops_the_break_character_when_the_whole_line_fits() {
    let g = lines_of("aaaa bbbb", 40);
    assert_eq!(g.lines.len(), 2, "the premise: two lines");
    assert_eq!(
        g.find_pos_from_line_and_pixels_rounded(0, 999, false),
        Some(g.lines[1].start - 1),
        "nextStart - 1: the break character is not kept"
    );
    assert_eq!(
        g.find_pos_from_line_and_pixels_rounded(1, 999, false),
        Some(g.len()),
        "the last line answers the whole character count"
    );
    assert_eq!(
        g.find_pos_from_line_and_pixels_rounded(9, 999, false),
        None,
        "no line 9 to scan"
    );
}

/// The round argument is the whole difference between the caret and the truncation.
///
/// Ten 8 px glyphs and a 37 px budget. The scan divides the glyph width by
/// `1 + round`, and the break test is `pixels <= step`:
///
/// * `round = false` — after four glyphs 5 px remain, `5 <= 8`, stop at **4**. Four whole glyphs
///   is all that fits, which is what a truncation wants.
/// * `round = true` — the step is 4, `5 > 4`, so the caret steps onto the fifth glyph whose
///   centre the pointer passed: **5**.
#[test]
fn the_round_argument_separates_the_caret_from_the_truncation() {
    let g = lines_of("abcdefghij", 999);
    assert_eq!(g.lines.len(), 1);
    assert_eq!(
        g.find_pos_from_line_and_pixels_rounded(0, 37, false),
        Some(4)
    );
    assert_eq!(
        g.find_pos_from_line_and_pixels_rounded(0, 37, true),
        Some(5)
    );
    assert_eq!(
        g.find_pos_from_line_and_pixels(0, 37),
        5,
        "the caret helper is the bRound = true call and not a second copy of the scan"
    );
}
