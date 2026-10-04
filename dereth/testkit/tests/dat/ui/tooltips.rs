//! UI fixtures and scenarios for tooltips.

use super::*;
// =============================================================================================
// tooltip.* -- what resting the pointer on something draws
//
// The instrument's own calibration -- that the shipped panels really do carry tooltips and that
// none of them can be reached with the screen at rest -- is the premise of the first scenario
// rather than a row of its own.
// =============================================================================================

/// The attribute that turns a tooltip on at all.
const TOOLTIPS_ON: u32 = 0x4B;
/// The window a tooltip is drawn in.
const TOOLTIP_ELEMENT: u32 = 0x47;
/// The words it carries.
const TOOLTIP_ENTRY: u32 = 0x49;
/// The attribute on a label that says "show the whole of me when I do not fit".
const AUTO_TOOLTIP_TRUNCATED: u32 = 0xD0;

pub(super) fn every_element(ui: &UiSystem) -> Vec<ElemHandle> {
    fn walk(ui: &UiSystem, h: ElemHandle, out: &mut Vec<ElemHandle>) {
        out.push(h);
        for child in ui.children(h) {
            walk(ui, child, out);
        }
    }
    let mut all = Vec::new();
    walk(ui, ui.root(), &mut all);
    all
}

/// Everything the tree would draw right now, recorded rather than drawn.
fn recorded_draw_list(ui: &mut UiSystem) -> Vec<dereth_ui::UiDrawCmd> {
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    back.calls
}

/// Every live element whose whole tooltip is words the layout already ships.
fn ships_its_own_tooltip(ui: &UiSystem) -> Vec<ElemHandle> {
    every_element(ui)
        .into_iter()
        .filter(|h| {
            let Some(n) = ui.node(*h) else { return false };
            let p = n.merged_properties();
            p.get_bool(TOOLTIPS_ON) == Some(true)
                && p.get(TOOLTIP_ELEMENT).is_some()
                && p.get(TOOLTIP_ENTRY).is_some()
        })
        .collect()
}

/// What the layout says this element's tooltip reads, resolved the way the client resolves it.
fn authored_tooltip(ui: &UiSystem, h: ElemHandle) -> Option<String> {
    let si = ui
        .node(h)?
        .merged_properties()
        .get_string_info(TOOLTIP_ENTRY)?
        .clone();
    si.literal
        .clone()
        .or_else(|| ui.resolve_string(si.table_id?, si.string_id?))
}

fn element_centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let b = ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// Open the panel this element lives in, from the top down, as the client does.
fn open_the_panel_that_owns(ui: &mut UiSystem, h: ElemHandle) {
    let mut chain = Vec::new();
    let mut q = Some(h);
    while let Some(a) = q {
        chain.push(a);
        q = ui.parent(a);
    }
    for a in chain.iter().rev() {
        ui.set_visible(*a, true);
    }
    ui.use_time(
        dereth_primitives::LocalTime(0.0),
        &mut dereth_ui::NullInputPump,
    );
}

/// The pointer comes to rest and stays there while frames go by -- which is the only thing a
/// tooltip answers.
fn rest_pointer_at(ui: &mut UiSystem, from: f64, x: i32, y: i32) {
    ui.mouse_move(dereth_primitives::LocalTime(from), x, y);
    let mut t = from;
    for _ in 0..8 {
        t += 0.25;
        ui.use_time(
            dereth_primitives::LocalTime(t),
            &mut dereth_ui::NullInputPump,
        );
    }
}

/// What appeared: the rectangle it covers, how many drawings it took and the letters in it.
struct Appeared {
    rect: dereth_ui::Box2D,
    commands: usize,
    glyphs: String,
}

fn what_appeared(before: &[dereth_ui::UiDrawCmd], after: &[dereth_ui::UiDrawCmd]) -> Appeared {
    use std::collections::BTreeSet;
    let known: BTreeSet<ElemHandle> = before.iter().map(|c| c.who).collect();
    let new: Vec<&dereth_ui::UiDrawCmd> =
        after.iter().filter(|c| !known.contains(&c.who)).collect();
    let mut rect = dereth_ui::Box2D::empty();
    for c in &new {
        rect = if rect.is_valid() {
            dereth_ui::Box2D {
                x0: rect.x0.min(c.screen.x0),
                y0: rect.y0.min(c.screen.y0),
                x1: rect.x1.max(c.screen.x1),
                y1: rect.y1.max(c.screen.y1),
            }
        } else {
            c.screen
        };
    }
    let glyphs: String = new
        .iter()
        .flat_map(|c| {
            c.glyphs
                .iter()
                .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        })
        .collect();
    Appeared {
        rect,
        commands: new.len(),
        glyphs,
    }
}

// ---------------------------------------------------------------------------------------------
// tooltip.resting-the-pointer-on-something-draws-a-box-with-words-in-it
// ---------------------------------------------------------------------------------------------

/// Resting the pointer on something draws a box with words in it, not an empty frame.
pub(super) fn resting_the_pointer_on_something_draws_a_box_with_words_in_it() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(24));
    let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;

    // **The instrument's own calibration**: the shipped panels really do carry tooltips, and with
    // the screen at rest the pointer reaches none of them -- which is why every reading below opens
    // a panel first. An empty result is not a negative result until the instrument has been shown
    // to be pointing at the target.
    let candidates = ships_its_own_tooltip(ui);
    let the_instrument_can_look = !candidates.is_empty();
    let none_at_rest = candidates.iter().all(|h| {
        let (x, y) = element_centre(ui, *h);
        ui.hit_test_screen(x, y) != Some(*h)
    });

    let mut all_drawn = true;
    let mut proved = 0;
    let mut clock = 0.0;
    for h in candidates {
        open_the_panel_that_owns(ui, h);
        let (x, y) = element_centre(ui, h);
        if ui.hit_test_screen(x, y) != Some(h) {
            // Some of them sit under a neighbour even with the panel open. They are not this
            // claim's business, and they are not counted as passes either.
            continue;
        }
        let before = recorded_draw_list(ui);
        clock += 10.0;
        rest_pointer_at(ui, clock, x, y);
        let after = recorded_draw_list(ui);
        let got = what_appeared(&before, &after);
        // A box with no words and words with no box are two different faults; the first is a
        // frame drawn with nothing in it.
        all_drawn &= got.commands > 0
            && got.rect.width() > 0
            && got.rect.height() > 0
            && !got.glyphs.is_empty();
        proved += 1;
        clock += 10.0;
        ui.mouse_move(dereth_primitives::LocalTime(clock), 0, 0);
        ui.use_time(
            dereth_primitives::LocalTime(clock),
            &mut dereth_ui::NullInputPump,
        );
    }
    let measured_something = proved > 0;

    c.assert_behaviour(
        "tooltip.resting-the-pointer-on-something-draws-a-box-with-words-in-it",
        move |_| the_instrument_can_look && none_at_rest && all_drawn && measured_something,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// tooltip.the-words-are-the-ones-that-element-was-given
// ---------------------------------------------------------------------------------------------

/// The words are that element's own, not a neighbour's and not an empty frame.
pub(super) fn the_words_in_a_tooltip_are_the_ones_that_element_was_given() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(24));
    let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
    let candidates = ships_its_own_tooltip(ui);

    let mut every_one = true;
    let mut checked = 0;
    let mut clock = 0.0;
    for h in candidates {
        let Some(expected) = authored_tooltip(ui, h) else {
            continue;
        };
        if expected.is_empty() {
            continue;
        }
        open_the_panel_that_owns(ui, h);
        let (x, y) = element_centre(ui, h);
        if ui.hit_test_screen(x, y) != Some(h) {
            continue;
        }
        let before = recorded_draw_list(ui);
        clock += 10.0;
        rest_pointer_at(ui, clock, x, y);
        let after = recorded_draw_list(ui);
        every_one &= what_appeared(&before, &after).glyphs == expected;
        checked += 1;
        clock += 10.0;
        ui.mouse_move(dereth_primitives::LocalTime(clock), 0, 0);
        ui.use_time(
            dereth_primitives::LocalTime(clock),
            &mut dereth_ui::NullInputPump,
        );
    }

    c.assert_behaviour(
        "tooltip.the-words-are-the-ones-that-element-was-given",
        move |_| every_one && checked > 0,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// tooltip.a-label-too-long-for-its-box-can-be-read-and-catches-the-pointer
// ---------------------------------------------------------------------------------------------

/// The one tooltip in the client that belongs to no particular panel: hover a label the client
/// had to cut short, and read the whole of it.
pub(super) fn a_label_too_long_for_its_box_can_be_read_and_catches_the_pointer() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(24));
    let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;

    let armed: Vec<ElemHandle> = every_element(ui)
        .into_iter()
        .filter(|h| {
            ui.node(*h).is_some_and(|n| {
                n.merged_properties().get_bool(AUTO_TOOLTIP_TRUNCATED) == Some(true)
            }) && ui.screen_box(*h).is_valid()
        })
        .collect();
    let the_instrument_can_look = !armed.is_empty();

    let widest = armed
        .iter()
        .map(|h| ui.screen_box(*h).width())
        .max()
        .unwrap_or(0);
    let sentence = "The Lost City of Frore, and every step of the road that leads to it. ";
    let reps = 1 + usize::try_from(widest.max(0)).expect("a width") / sentence.len();
    let long = sentence.repeat(reps);

    let mut chosen = None;
    for h in &armed {
        if !ui
            .text_element_mut(*h)
            .is_some_and(|t| t.bits.truncate() && t.bits.one_line())
        {
            continue;
        }
        open_the_panel_that_owns(ui, *h);
        // A label is only reachable once it has been cut short, so the reach is tried with the
        // long text in place and then taken back out.
        ui.text_element_mut(*h)
            .expect("a text element")
            .set_text(&long);
        ui.use_time(
            dereth_primitives::LocalTime(1.0),
            &mut dereth_ui::NullInputPump,
        );
        let (x, y) = element_centre(ui, *h);
        let reachable = ui.hit_test_screen(x, y) == Some(*h);
        ui.text_element_mut(*h)
            .expect("a text element")
            .set_text("");
        ui.use_time(
            dereth_primitives::LocalTime(1.0),
            &mut dereth_ui::NullInputPump,
        );
        if reachable {
            chosen = Some(*h);
            break;
        }
    }
    let h = chosen.expect("one of the labels must be reachable, or this measures nothing");
    let width = ui.screen_box(h).width();

    // Text that fits: no tooltip, and the label is transparent to the pointer.
    ui.text_element_mut(h)
        .expect("a text element")
        .set_text("ok");
    ui.use_time(
        dereth_primitives::LocalTime(2.0),
        &mut dereth_ui::NullInputPump,
    );
    let fits = {
        let n = ui.node(h).expect("alive");
        n.tooltip_text.is_none() && !n.region.flags.tooltip && !n.is_mouse_visible
    };

    // Text that does not: the whole of it becomes the tooltip, and the label starts catching the
    // pointer -- which is what stops a press falling through it.
    let really_too_long = i32::try_from(long.len()).expect("a length") > width;
    ui.text_element_mut(h)
        .expect("a text element")
        .set_text(&long);
    ui.use_time(
        dereth_primitives::LocalTime(3.0),
        &mut dereth_ui::NullInputPump,
    );
    let clipped = {
        let n = ui.node(h).expect("alive");
        n.tooltip_text.as_deref() == Some(long.as_str())
            && n.region.flags.tooltip
            && n.is_mouse_visible
    };

    let (x, y) = element_centre(ui, h);
    let reaches = ui.hit_test_screen(x, y) == Some(h);
    let before = recorded_draw_list(ui);
    rest_pointer_at(ui, 4.0, x, y);
    let after = recorded_draw_list(ui);
    let drew_the_whole_thing = what_appeared(&before, &after).glyphs == long;

    c.assert_behaviour(
        "tooltip.a-label-too-long-for-its-box-can-be-read-and-catches-the-pointer",
        move |_| {
            the_instrument_can_look
                && fits
                && really_too_long
                && clipped
                && reaches
                && drew_the_whole_thing
        },
    );
    c.shutdown();
}
