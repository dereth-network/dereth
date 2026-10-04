//! Shell fixtures and scenarios for chargen name.

use super::*;
use dereth_ui_screens::screens::chargen;
// =============================================================================================
// chargen.summary.* -- the two gestures into the name box
//
// Guards against *"after a click into the box, typing enters nothing"*. Three related checks are
// **not** rows here: the box coming up with the keyboard and the prompt selected, typing into it
// with no press at all, and the name surviving a trip off the page and back are
// `chargen.summary.the-name-box-prompts-takes-the-keyboard-and-the-first-
// character-replaces-the-prompt` -- and the selection read here is folded into that scenario rather
// than asserted twice.
//
// What is a row here is the two gestures into the box, and they are two rows because they use
// different machinery and a client with one of them broken passes the other.
// =============================================================================================

/// The name box's selection, as the client's own reader gives it.
fn name_selection(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> Option<(usize, usize)> {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .and_then(|t| t.get_selection())
}

/// The two bits a press and a release write: whether there is a selection at all, and whether it
/// belongs to a press that has not been let go yet.
fn selection_bits(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> (bool, bool) {
    let t = c
        .app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .expect("a text element");
    (t.bits.selecting(), t.bits.selection_from_press())
}

/// The wizard on its last page with a people settled, which is where the name box lives.
pub(super) fn a_wizard_on_the_summary_page() -> HeadlessClient {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    c.tick(2);
    c
}

// ---------------------------------------------------------------------------------------------
// chargen.summary.a-press-in-the-name-box-puts-the-caret-where-it-was-pressed
// ---------------------------------------------------------------------------------------------

/// A press in a box that already holds the keyboard places the caret rather than selecting
/// everything again, so what is typed goes in at the caret.
pub(super) fn a_press_in_the_name_box_puts_the_caret_where_it_was_pressed() {
    let mut c = a_wizard_on_the_summary_page();
    let mut hands = Hands::new();
    let name = element(&c, chargen::NAME_FIELD);
    let p = "Tarinell";
    dereth_testkit::input_steps::type_text(&mut c, p);
    assert_eq!(wizard_text(&mut c, name), p);
    let (x, y) = middle_of(&c, name);

    press_at(&mut c, &mut hands, x, y);
    // While the button is down there is a selection, and it is the press's own -- the two bits
    // are different things and a gesture that only ever reported its end could not tell them
    // apart.
    let while_down = selection_bits(&mut c, name) == (true, true);

    release(&mut c, &mut hands);
    let kept_the_keyboard = what_holds_the_keyboard(&c) == Some(name);
    // Letting go collapses the selection onto the caret the press placed...
    let caret_placed = name_selection(&mut c, name) == Some((p.chars().count(), p.chars().count()));
    // ...and the press's own bit is gone with the button, while the selection bit is not touched
    // -- which is why a selection made this way survives the release and can still be copied.
    let after_up = selection_bits(&mut c, name) == (true, false);

    dereth_testkit::input_steps::type_text(&mut c, "Zz");
    let typed_at_the_caret = wizard_text(&mut c, name) == "TarinellZz"
        && with_wizard(&mut c, |_, w| w.state.name.clone()) == "Tarinellzz"
        && with_wizard(&mut c, |_, w| w.name_entered);

    c.assert_behaviour("chargen.summary.a-press-in-the-name-box-puts-the-caret-where-it-was-pressed-and-typing-goes-there", move |_| {
        while_down && kept_the_keyboard && caret_placed && after_up && typed_at_the_caret
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.summary.a-press-on-nothing-takes-the-keyboard-away-and-a-press-back-in-returns-it
// ---------------------------------------------------------------------------------------------

/// The reported symptom, and the gesture that undoes it.
pub(super) fn a_press_on_nothing_takes_the_keyboard_away_and_a_press_back_in_returns_it() {
    let mut c = a_wizard_on_the_summary_page();
    let mut hands = Hands::new();
    let name = element(&c, chargen::NAME_FIELD);
    let p = shipped_word(&c, chargen::NAME_PROMPT);
    let (x, y) = middle_of(&c, name);

    // A point on the page nothing answers for. Asserted rather than assumed: a layout change that
    // put something there would leave this scenario proving nothing.
    let empty = {
        let b = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .screen_box(name);
        ((b.x0 + b.x1) / 2, b.y1 + 16)
    };
    let really_empty = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .hit_test_screen(empty.0, empty.1)
        .is_none();

    hands.click_at(&mut c, empty.0, empty.1);
    let keyboard_let_go = what_holds_the_keyboard(&c).is_none();

    // With nothing holding the keyboard a keystroke goes nowhere, which is the symptom as it was
    // reported.
    dereth_testkit::input_steps::type_text(&mut c, "X");
    let nothing_entered = wizard_text(&mut c, name) == p;

    press_at(&mut c, &mut hands, x, y);
    release(&mut c, &mut hands);
    let keyboard_back = what_holds_the_keyboard(&c) == Some(name);

    dereth_testkit::input_steps::type_text(&mut c, "Zz");
    let now_it_enters = wizard_text(&mut c, name) == format!("{p}Zz")
        && with_wizard(&mut c, |_, w| w.state.name.clone()) == format!("{p}Zz");

    c.assert_behaviour("chargen.summary.a-press-on-nothing-takes-the-keyboard-away-and-a-press-back-in-the-box-returns-it", move |_| {
        really_empty && keyboard_let_go && nothing_entered && keyboard_back && now_it_enters
    });
    c.shutdown();
}

// =============================================================================================
// chargen.summary.a-press-that-types-nothing-leaves-a-plain-caret
//
// A press into the name box that types nothing leaves a plain caret, and it stays one; the name is
// selected again only when the page is refreshed -- coming back to it, or a re-roll. How often the
// page refreshes, and which message sets the "a name has been typed" flag, are not rows of their
// own; they are asserted here from the outside, by making the gesture and watching thirty frames go
// by.
//
// The two halves are one scenario on purpose: the plain caret alone would pass on a client whose
// name box could not be re-selected at all, and the re-selection alone would pass on one that
// re-selected constantly.
// =============================================================================================

/// The press that types nothing leaves a caret, and the two gestures that do put the highlight
/// back really do.
pub(super) fn a_press_that_types_nothing_leaves_a_plain_caret() {
    let mut c = a_wizard_on_the_summary_page();
    let name = element(&c, chargen::NAME_FIELD);
    let p = shipped_word(&c, chargen::NAME_PROMPT);
    let n = p.chars().count();
    let (x, y) = middle_of(&c, name);

    // The premise: the page came up with the whole prompt highlighted. Without it the press below
    // has nothing to collapse and this scenario measures nothing.
    let came_up_selected = name_selection(&mut c, name) == Some((0, n));
    // **Read before the press**, because a press takes the keyboard by itself: an assertion made
    // afterwards could not tell whether the page had ever taken it.
    let page_took_the_keyboard = what_holds_the_keyboard(&c) == Some(name);

    c.when(dereth_testkit::Player::Click(
        dereth_testkit::Target::Point(dereth_testkit::ScreenPoint::new(x, y)),
    ));
    let collapsed = name_selection(&mut c, name) == Some((n, n));

    let before = c.view().expect_app().frames_drawn();
    c.tick(30);
    // Thirty frames really ran: a client that had stopped ticking would satisfy everything below
    // at once.
    let still_running = c.view().expect_app().frames_drawn() - before == 30;
    let still_a_plain_caret = name_selection(&mut c, name) == Some((n, n))
        && wizard_text(&mut c, name) == p
        && what_holds_the_keyboard(&c) == Some(name)
        && !with_wizard(&mut c, |_, w| w.name_entered);

    // The other direction. Leaving the page and coming back refreshes it, and the highlight is
    // back -- so the client can put it back, and the thirty quiet frames above are a measurement.
    click_wizard(
        &mut c,
        EcgProgress::Hertage
            .select_button()
            .expect("the heritage tab"),
    );
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    let name = element(&c, chargen::NAME_FIELD);
    let coming_back_re_selects = name_selection(&mut c, name) == Some((0, n));

    // Collapse it again so the second gesture is measured from the state the first one was.
    // The press goes in through the step that spaces its gestures two seconds apart, because two
    // presses at one point inside the double-click window are a double click, which this box
    // ignores entirely -- and the reading would then be of a press that never happened.
    let (x, y) = middle_of(&c, name);
    c.when(dereth_testkit::Player::Click(
        dereth_testkit::Target::Point(dereth_testkit::ScreenPoint::new(x, y)),
    ));
    let collapsed_again = name_selection(&mut c, name) == Some((n, n));

    // And a re-roll, which is the other gesture that refreshes the page.
    with_wizard(&mut c, |_, w| w.pending_random = true);
    c.tick(1);
    let name = element(&c, chargen::NAME_FIELD);
    let a_re_roll_re_selects = name_selection(&mut c, name) == Some((0, n))
        && wizard_text(&mut c, name) == p
        && !with_wizard(&mut c, |_, w| w.name_entered);

    c.assert_behaviour("chargen.summary.a-press-that-types-nothing-leaves-a-plain-caret-and-only-a-gesture-puts-the-highlight-back", move |_| {
        came_up_selected
            && page_took_the_keyboard
            && collapsed
            && still_running
            && still_a_plain_caret
            && coming_back_re_selects
            && collapsed_again
            && a_re_roll_re_selects
    });
    c.shutdown();
}
