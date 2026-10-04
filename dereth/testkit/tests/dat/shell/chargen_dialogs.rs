//! Shell fixtures and scenarios for chargen dialogs.

use super::*;
use dereth_ui_screens::screens::chargen;
// =============================================================================================
// chargen.dialogs.* -- the wizard's other four boxes
//
// The table of which box is which kind and which button answers it is **not** a row: it would be a
// table of numbers beside the symbols the production code carries them through, a transcription.
// What it would protect is asserted here from the outside, by pressing the buttons.
//
// The bring-up is the exit scenarios' `a_client_on_the_wizard`; the pointer is
// `adapters_shell::Hands`.
// =============================================================================================

/// The handle of a dialog the wizard has raised, if it is on screen.
pub(super) fn wizard_dialog(
    c: &mut HeadlessClient,
    which: CharGenDialog,
) -> Option<dereth_ui::ElemHandle> {
    with_wizard(c, |_, w| w.dialog_element(which))
}

/// Click a child **of a dialog**, which is not under the wizard's own root.
pub(super) fn click_in_dialog(
    c: &mut HeadlessClient,
    dialog: dereth_ui::ElemHandle,
    id: ElementId,
) {
    let h = dialog_child(c, dialog, id);
    press_handle(c, h);
}

/// What a dialog is: the right subclass's root, modal, and carrying the shipped words.
///
/// The three together, because a box that is the wrong shape, one a player can click past, and a
/// blank one are three different things a player would see.
fn dialog_is(
    c: &mut HeadlessClient,
    h: dereth_ui::ElemHandle,
    kind: dereth_ui::dialog::DialogKind,
    token: &str,
) -> bool {
    let (node_id, modal) = {
        let n = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("live");
        (n.element_id(), n.region.flags.block_clicks)
    };
    let want = shipped_word(c, token);
    let body = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(h, dereth_ui::dialog::base::child::TEXT)
        .expect("a dialog carries its own text element");
    node_id == kind.root_element_id() && modal && !want.is_empty() && wizard_text(c, body) == want
}

/// Everything a re-roll can move, as one comparable value -- so "it was re-rolled" and "it was
/// left alone" are the same reading taken in two directions.
fn appearance_of(c: &mut HeadlessClient) -> Vec<i32> {
    with_wizard(c, |_, w| {
        let s = &w.state;
        vec![
            i32::try_from(s.heritage_group).unwrap_or(-1),
            i32::try_from(s.gender).unwrap_or(-1),
            s.eyes_strip,
            s.nose_strip,
            s.mouth_strip,
            s.hair_style,
            s.hair_color,
            s.eye_color,
            s.headgear_style,
            s.shirt_style,
            s.trousers_style,
            s.footwear_style,
            s.template,
        ]
    })
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.the-warning-before-a-re-roll-is-a-modal-question-in-the-shipped-words
// ---------------------------------------------------------------------------------------------

/// The warning is really built, is really modal, and really says what the shipped text says.
pub(super) fn the_re_roll_warning_is_a_modal_question_in_the_shipped_words() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    let nothing_first = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none();

    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning)
        .expect("the random button on the last page raises the warning");
    let recorded =
        with_wizard(&mut c, |_, w| w.open_dialog) == Some(CharGenDialog::RandomizeWarning);
    let shaped = dialog_is(
        &mut c,
        h,
        dereth_ui::dialog::DialogKind::Confirmation,
        RANDOMIZE_WARNING_STRING,
    );

    c.assert_behaviour(
        "chargen.dialogs.the-warning-before-a-re-roll-is-a-modal-question-in-the-shipped-words",
        move |_| nothing_first && recorded && shaped,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.the-two-answer-buttons-really-answer-and-a-pointer-can-press-either-of-them
// ---------------------------------------------------------------------------------------------

/// Yes and no, pressed as buttons and then pressed with a pointer.
///
/// The two halves are one scenario because the interesting claim is the **difference**: a client
/// that re-rolled on both answers and one that re-rolled on neither each pass half of it. And the
/// pointer half is here because a driven windowed run once disagreed with the message-only one --
/// the no could be answered and the yes could not -- so the gesture that goes through the hit
/// test is asserted beside the one that does not.
pub(super) fn the_warnings_two_buttons_answer_it_and_a_pointer_can_press_either() {
    // --- no, through the button's own message ---------------------------------------------
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    let before = appearance_of(&mut c);
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    click_in_dialog(&mut c, h, dereth_ui::dialog::base::child::BUTTON2);
    c.tick(1);
    let no_answered = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none()
        && appearance_of(&mut c) == before
        && with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Summary;
    c.shutdown();

    // --- yes, from the identical starting character ----------------------------------------
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    let same_start = appearance_of(&mut c) == before;
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    click_in_dialog(&mut c, h, dereth_ui::dialog::base::child::BUTTON1);
    c.tick(2);
    let yes_answered = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none()
        && appearance_of(&mut c) != before;
    c.shutdown();

    // --- the same two through a real pointer, hit test and all ------------------------------
    let mut c = a_client_on_the_wizard();
    let mut hands = Hands::new();
    goto_page(&mut c, EcgProgress::Summary);
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    let no = dialog_child(&mut c, h, dereth_ui::dialog::base::child::BUTTON2);
    hands.click_handle(&mut c, no);
    c.tick(1);
    // The calibration first: without it, "the yes does nothing" and "my pointer does nothing" are
    // the same reading, which is the mistake the driven runs could have led to.
    let pointer_can_answer = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none()
        && appearance_of(&mut c) == before;
    c.shutdown();

    let mut c = a_client_on_the_wizard();
    let mut hands = Hands::new();
    goto_page(&mut c, EcgProgress::Summary);
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    let yes = dialog_child(&mut c, h, dereth_ui::dialog::base::child::BUTTON1);
    hands.click_handle(&mut c, yes);
    c.tick(2);
    let pointer_yes = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none()
        && appearance_of(&mut c) != before;

    c.assert_behaviour("chargen.dialogs.the-two-answer-buttons-really-answer-and-a-pointer-can-press-either-of-them", move |_| {
        no_answered && same_start && yes_answered && pointer_can_answer && pointer_yes
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.the-warning-before-a-re-roll-is-the-last-pages-alone
// ---------------------------------------------------------------------------------------------

/// On any other page the random button rolls at once, with no question and no box.
pub(super) fn only_the_last_page_asks_before_re_rolling() {
    let mut c = a_client_on_the_wizard();
    // The face page, because its roll moves a dozen things at once, so "it rolled" is visible.
    // The people page's roll is a three-way one that may land where it started, which would make
    // this pass for the wrong reason.
    click_wizard(
        &mut c,
        EcgProgress::Appearance
            .select_button()
            .expect("the appearance tab"),
    );
    let on_the_page = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Appearance;
    let before = appearance_of(&mut c);

    click_wizard(&mut c, RANDOM_BUTTON);

    let no_question = with_wizard(&mut c, |_, w| w.open_dialog).is_none()
        && CharGenDialog::RAISED
            .iter()
            .all(|ctx| wizard_dialog(&mut c, *ctx).is_none());
    let rolled_at_once = appearance_of(&mut c) != before;

    c.assert_behaviour(
        "chargen.dialogs.the-warning-before-a-re-roll-is-the-last-pages-alone",
        move |_| on_the_page && no_question && rolled_at_once,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.a-people-the-account-cannot-play-is-refused-in-a-message-box
// ---------------------------------------------------------------------------------------------

/// A people that needs the expansion is refused, in a box with one button and nothing to answer.
pub(super) fn a_people_the_account_cannot_play_is_refused_in_a_message_box() {
    let mut c = a_client_on_the_wizard();
    let plain_account = !with_wizard(&mut c, |_, w| w.account_has_tod);
    let before = with_wizard(&mut c, |_, w| w.state.heritage_group);

    let (bullet, which) = chargen::HERITAGE_BUTTONS[3];
    assert_eq!(which, 4, "the fourth bullet is the one the expansion adds");
    click_wizard(&mut c, bullet);

    let h = wizard_dialog(&mut c, CharGenDialog::ToDRequired).expect("the refusal is raised");
    let shaped = dialog_is(
        &mut c,
        h,
        dereth_ui::dialog::DialogKind::Message,
        TOD_WARNING_STRING,
    );
    let refused = with_wizard(&mut c, |_, w| w.state.heritage_group) == before;

    // Its one button, which is all a message box has.
    click_in_dialog(&mut c, h, MESSAGE_BUTTON);
    c.tick(1);
    let taken_down = wizard_dialog(&mut c, CharGenDialog::ToDRequired).is_none()
        && with_wizard(&mut c, |_, w| w.state.heritage_group) == before;

    c.assert_behaviour(
        "chargen.dialogs.a-people-the-account-cannot-play-is-refused-in-a-message-box",
        move |_| plain_account && shaped && refused && taken_down,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.finishing-with-credits-unspent-asks-first-and-a-yes-goes-on-to-create
// ---------------------------------------------------------------------------------------------

/// Unspent credits raise a question, the question is a refusal until it is answered, and yes
/// goes on to create the character.
pub(super) fn finishing_with_credits_unspent_asks_first() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    // The first refusal is the empty name, which is a different box; give it a name.
    with_wizard(&mut c, |_, w| {
        w.state.name = "Alba".to_string();
        w.name_entered = true;
    });
    // The rolled character has spent its credits, which is the roll's own doing -- so the state
    // the warning is about is set up explicitly, which is what a player who moves a slider back
    // does, rather than relied upon.
    let spent = with_wizard(&mut c, |_, w| w.state.remaining_atrb_credits) == 0;
    with_wizard(&mut c, |_, w| {
        w.state.remaining_atrb_credits = 30;
        let _ = w.take_actions();
    });

    click_wizard(&mut c, chargen::FINISH_BUTTON);

    let h = wizard_dialog(&mut c, CharGenDialog::CreditWarning).expect("it asks");
    let shaped = dialog_is(
        &mut c,
        h,
        dereth_ui::dialog::DialogKind::Confirmation,
        CREDIT_WARNING_STRING,
    );
    // ...and it really is a refusal: nothing has been created while the question is up.
    let nothing_sent = with_wizard(&mut c, |_, w| w.state.verification) == CgVerification::Undef;

    click_in_dialog(&mut c, h, dereth_ui::dialog::base::child::BUTTON1);
    c.tick(1);
    let went_on = wizard_dialog(&mut c, CharGenDialog::CreditWarning).is_none()
        && with_wizard(&mut c, |_, w| w.state.verification) == CgVerification::Pending;

    c.assert_behaviour(
        "chargen.dialogs.finishing-with-credits-unspent-asks-first-and-a-yes-goes-on-to-create",
        move |_| spent && shaped && nothing_sent && went_on,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.finishing-with-no-name-refuses-in-a-message-box
// ---------------------------------------------------------------------------------------------

/// No name is a refusal in a box with one button, and dismissing it consumes the message.
pub(super) fn finishing_with_no_name_refuses_in_a_message_box() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    with_wizard(&mut c, |_, w| {
        w.state.name = String::new();
        w.name_entered = false;
    });

    click_wizard(&mut c, chargen::FINISH_BUTTON);

    let h = wizard_dialog(&mut c, CharGenDialog::ErrorMessage).expect("it refuses in a box");
    let shaped = dialog_is(
        &mut c,
        h,
        dereth_ui::dialog::DialogKind::Message,
        "ID_CharGen_NoNameWarning",
    );
    click_in_dialog(&mut c, h, MESSAGE_BUTTON);
    c.tick(1);
    let taken_down = wizard_dialog(&mut c, CharGenDialog::ErrorMessage).is_none()
        && with_wizard(&mut c, |_, w| w.error_string_id).is_none();

    c.assert_behaviour(
        "chargen.dialogs.finishing-with-no-name-refuses-in-a-message-box",
        move |_| shaped && taken_down,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.the-wizard-never-shows-a-please-wait-box
// ---------------------------------------------------------------------------------------------

/// The one box the wizard keeps a place for and never raises draws nothing.
pub(super) fn the_wizard_never_shows_a_please_wait_box() {
    let mut c = a_client_on_the_wizard();
    let roots_before = wizard_roots(&mut c);

    with_wizard(&mut c, |_, w| {
        w.open_dialog = Some(CharGenDialog::PleaseWait)
    });
    c.tick(3);

    let still_recorded =
        with_wizard(&mut c, |_, w| w.open_dialog) == Some(CharGenDialog::PleaseWait);
    let nothing_drawn = wizard_dialog(&mut c, CharGenDialog::PleaseWait).is_none();
    let no_extra_root = wizard_roots(&mut c) == roots_before;
    // ...and it is not one of the ones the wizard can raise, which is the same claim from the
    // other side: the five it can raise all have a shape, and this one has none.
    let not_raisable = !CharGenDialog::RAISED.contains(&CharGenDialog::PleaseWait)
        && CharGenDialog::RAISED.len() == 5
        && CharGenDialog::RAISED.iter().all(|ctx| ctx.kind().is_some())
        && CharGenDialog::PleaseWait.kind().is_none();

    c.assert_behaviour(
        "chargen.dialogs.the-wizard-never-shows-a-please-wait-box",
        move |_| still_recorded && nothing_drawn && no_extra_root && not_raisable,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.one-answered-from-outside-the-screen-is-taken-down-on-the-next-frame
// ---------------------------------------------------------------------------------------------

/// A box answered by something that is not the player's pointer still comes off the screen.
pub(super) fn a_box_answered_from_outside_the_screen_comes_down() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    let roots_with_it = wizard_roots(&mut c);
    let live = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .is_some();

    // Exactly what the host does when the answer comes from somewhere with no screen in reach.
    with_wizard(&mut c, |_, w| w.close_dialog(false));
    c.tick(1);

    let forgotten = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none();
    let root_deleted = wizard_roots(&mut c) == roots_with_it - 1;
    let gone_from_the_tree = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .is_none();

    c.assert_behaviour(
        "chargen.dialogs.one-answered-from-outside-the-screen-is-taken-down-on-the-next-frame",
        move |_| live && forgotten && root_deleted && gone_from_the_tree,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.a-second-one-waits-its-turn-rather-than-stacking-on-the-first
// ---------------------------------------------------------------------------------------------

/// Two boxes at once are one box and one waiting, and answering the first shows the second.
pub(super) fn a_second_box_waits_its_turn_rather_than_stacking() {
    let mut c = a_client_on_the_wizard();
    let roots_before = wizard_roots(&mut c);

    let (raised_first, raised_second) = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let first = {
            let s = shell.flow.current_mut().expect("a screen");
            let any: &mut dyn std::any::Any = &mut **s;
            let w = any
                .downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
                .expect("the wizard");
            w.error_string_id = Some(chargen::TOD_WARNING_STRING);
            w.make_credit_warning_dialog(&mut shell.ui)
        };
        let second = {
            let s = shell.flow.current_mut().expect("a screen");
            let any: &mut dyn std::any::Any = &mut **s;
            let w = any
                .downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
                .expect("the wizard");
            w.make_error_message_dialog(&mut shell.ui)
        };
        (first, second)
    };
    c.tick(1);

    let one_on_screen = raised_first && !raised_second;
    let the_other_is_waiting = with_wizard(&mut c, |_, w| {
        w.dialog_context(CharGenDialog::ErrorMessage).is_some()
            && w.dialog_element(CharGenDialog::ErrorMessage).is_none()
            && w.dialog_element(CharGenDialog::CreditWarning).is_some()
    });
    let one_root = wizard_roots(&mut c) == roots_before + 1;

    // Answer the first, and the one that was waiting takes its place.
    let h = wizard_dialog(&mut c, CharGenDialog::CreditWarning).expect("the first is up");
    let no = dialog_child(&mut c, h, dereth_ui::dialog::base::child::BUTTON2);
    let mut hands = Hands::new();
    hands.click_handle(&mut c, no);
    c.tick(1);

    let promoted = with_wizard(&mut c, |_, w| {
        w.dialog_element(CharGenDialog::CreditWarning).is_none()
            && w.dialog_context(CharGenDialog::CreditWarning).is_none()
            && w.dialog_element(CharGenDialog::ErrorMessage).is_some()
    }) && wizard_roots(&mut c) == roots_before + 1;

    c.assert_behaviour(
        "chargen.dialogs.a-second-one-waits-its-turn-rather-than-stacking-on-the-first",
        move |_| one_on_screen && the_other_is_waiting && one_root && promoted,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.one-closed-from-underneath-the-screen-is-replaced-rather-than-left-behind
// ---------------------------------------------------------------------------------------------

/// A box closed underneath the screen leaves no orphan on the screen.
pub(super) fn a_box_closed_from_underneath_the_screen_is_replaced() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    let ctx = with_wizard(&mut c, |_, w| {
        w.dialog_context(CharGenDialog::RandomizeWarning)
            .expect("and a place in the queue")
    });
    let roots_with_it = wizard_roots(&mut c);

    // Closed at the queue and not through the screen, which is what a reset does: the screen is
    // never told, and its own record still says the box is wanted.
    let now = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .now
        .0;
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .dialogs
        .close_dialog(ctx, now);
    let forgotten_below = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .dialogs
        .info(ctx)
        .is_none();
    let screen_still_has_it = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning) == Some(h);
    c.tick(1);

    let now_h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning)
        .expect("the screen still wants the box, so it is raised again");
    let replaced = now_h != h
        && c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .is_none()
        && with_wizard(&mut c, |_, w| {
            w.dialog_context(CharGenDialog::RandomizeWarning)
        }) != Some(ctx)
        && wizard_roots(&mut c) == roots_with_it;
    // And nothing was answered by it: a close from underneath is not a yes.
    let not_an_answer = !with_wizard(&mut c, |_, w| w.pending_random);

    c.assert_behaviour(
        "chargen.dialogs.one-closed-from-underneath-the-screen-is-replaced-rather-than-left-behind",
        move |_| forgotten_below && screen_still_has_it && replaced && not_an_answer,
    );
    c.shutdown();
}
