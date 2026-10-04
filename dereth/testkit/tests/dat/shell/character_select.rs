//! Shell fixtures and scenarios for character select.

use super::*;
// =============================================================================================
// character-select.* -- the five boxes the character list raises
//
// Guards against *"the delete button doesn't do anything"*. Nine scenarios, nine rows: every one of
// them is an independent claim about a box a player is shown and what answering it does, and none
// is a transcription.
//
// Every press is a real pointer gesture and every character a real typed one, so what is under
// test is the route rather than a fixture standing in for it.
// =============================================================================================

/// A character's own id, by the place the shard listed them in.
fn character_id(slot: usize) -> dereth_primitives::ObjectId {
    dereth_primitives::ObjectId(0x5000_0001 + u32::try_from(slot).unwrap_or(0))
}

/// The handle of a box the character screen has raised.
pub(super) fn charmgmt_dialog(
    c: &mut HeadlessClient,
    which: DialogContext,
) -> Option<dereth_ui::ElemHandle> {
    with_charmgmt_screen(c, |s| s.dialog_element(which))
}

/// A child **below** a box's root, the way the client searches: the root itself is never the
/// answer. That matters here and nowhere else, because the typing box and the root of the
/// question it sits in share an id.
pub(super) fn charmgmt_child(
    c: &HeadlessClient,
    dialog: dereth_ui::ElemHandle,
    id: ElementId,
) -> dereth_ui::ElemHandle {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(dialog, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped dialog layout"))
}

/// A word out of the shipped text table the character screen reads.
pub(super) fn charmgmt_word(c: &HeadlessClient, token: &str) -> String {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .resolve_string(
            charmgmt::STRING_TABLE,
            dereth_ui::persist::preferences::token_of(token),
        )
        .unwrap_or_else(|| panic!("{token} is in the shipped text table"))
}

/// The row element a character is on.
pub(super) fn charmgmt_row(
    c: &mut HeadlessClient,
    id: dereth_primitives::ObjectId,
) -> dereth_ui::ElemHandle {
    with_charmgmt_screen(c, |s| {
        s.rows
            .iter()
            .find(|r| r.id == id)
            .and_then(|r| r.element)
            .expect("the character has a row")
    })
}

/// A press and a release with **no frame after them**, so the caller can run the frame itself and
/// read what the screen asked for before the client drops it.
fn press_without_a_frame(c: &mut HeadlessClient, hands: &mut Hands, h: dereth_ui::ElemHandle) {
    use dereth_client::platform::keys::MouseButton;
    let (x, y) = middle_of(c, h);
    hands.move_to(c, x, y);
    for down in [true, false] {
        let m = hands.button_message(MouseButton::Left, down);
        hands.send(c, m);
    }
}

/// What the screen asked the client to do with a character, read between the frame that produces
/// it and the step that throws it away when there is no shard to send it to.
///
/// It is a pass-through and fabricates nothing: the events are the ones the **real** input shell
/// made out of the real messages, relayed so that the shell's own frame can be run from here.
#[derive(Debug, Default)]
struct ActionRelay {
    pos: (i32, i32),
    mouse_move: Option<(i32, i32)>,
    actions: Vec<dereth_input::InputEvent>,
    characters: Vec<char>,
}

impl dereth_ui::InputPump for ActionRelay {
    fn use_time(&mut self, _now: dereth_primitives::LocalTime) {}
}

impl dereth_client::ui::UiInput for ActionRelay {
    fn as_pump(&mut self) -> &mut dyn dereth_ui::InputPump {
        self
    }
    fn mouse_pos(&self) -> (i32, i32) {
        self.pos
    }
    fn take_mouse_move(&mut self) -> Option<(i32, i32)> {
        self.mouse_move.take()
    }
    fn take_mouse_left_window(&mut self) -> bool {
        false
    }
    fn take_actions(&mut self) -> Vec<dereth_input::InputEvent> {
        std::mem::take(&mut self.actions)
    }
    fn put_back_unconsumed(&mut self, _events: Vec<dereth_input::InputEvent>) {}
    fn take_characters(&mut self) -> Vec<char> {
        std::mem::take(&mut self.characters)
    }
}

fn frame_taking_character_actions(c: &mut HeadlessClient) -> Vec<CharacterAction> {
    use dereth_client::ui::UiInput;
    let now = dereth_primitives::LocalTime(1.0);
    let mut relay = ActionRelay::default();
    {
        let input = c.app_mut().input_manager_mut().expect("the input manager");
        // The one call that turns the pending messages into events, made on the real manager. The
        // relay's own is a no-op precisely so it happens exactly once, here.
        dereth_ui::InputPump::use_time(input, now);
        relay.pos = input.mouse_pos();
        relay.mouse_move = UiInput::take_mouse_move(input);
        relay.actions = UiInput::take_actions(input);
        relay.characters = UiInput::take_characters(input);
    }
    let host = c.view().expect_app().host_state().clone();
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell.frame(now, &host, &mut relay);
    shell.take_character_actions()
}

/// The opcode and body a message puts on the wire, which is what a datagram carries.
fn message_bytes<M: dereth_protocol::Message>(m: &M) -> Vec<u8> {
    let mut out = M::OPCODE.0.to_le_bytes().to_vec();
    out.extend(dereth_protocol::write_body(m).expect("the message encodes"));
    out
}

/// The two halves of the shipped sentence the character's name goes between.
fn delete_question_halves(c: &HeadlessClient) -> Vec<String> {
    let parts = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .resolve_string_variants(
            charmgmt::STRING_TABLE,
            dereth_ui::persist::preferences::token_of(charmgmt::DELETE_CONFIRMATION_STRING),
        )
        .expect("the delete question is in the shipped text table");
    assert_eq!(
        parts.len(),
        2,
        "the shipped sentence is two pieces around one name"
    );
    parts
}

const EXIT_BUTTON_ID: ElementId = ElementId(0x1000_03A4);
pub(super) const DELETE_BUTTON_ID: ElementId = ElementId(0x1000_039F);
pub(super) const RESTORE_BUTTON_ID: ElementId = ElementId(0x1000_039E);

/// The same three characters, and a check the list really rebuilt from them.
fn a_character_list() -> HeadlessClient {
    let mut c = a_client_on_character_select();
    assert_eq!(
        with_charmgmt_screen(&mut c, |s| s.rows.len()),
        3,
        "the list was rebuilt from the set the shard sent"
    );
    c
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.leaving-raises-a-modal-question-in-the-shipped-words
// ---------------------------------------------------------------------------------------------

/// The exit button really raises a box, and the box is modal and carries the shipped sentence.
pub(super) fn leaving_raises_a_modal_question_in_the_shipped_words() {
    let mut c = a_character_list();
    let mut hands = Hands::new();
    let nothing_first = charmgmt_dialog(&mut c, DialogContext::ConfirmExit).is_none();

    let exit = element(&c, EXIT_BUTTON_ID);
    hands.click_handle(&mut c, exit);

    let h = charmgmt_dialog(&mut c, DialogContext::ConfirmExit)
        .expect("the exit button raises a box, not merely a note to itself");
    let recorded =
        with_charmgmt_screen(&mut c, |s| s.open_dialog) == Some(DialogContext::ConfirmExit);
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
    let shaped = node_id == ElementId(0x15) && modal;
    let want = charmgmt_word(&c, charmgmt::CONFIRM_EXIT_STRING);
    let body = charmgmt_child(&c, h, dereth_ui::dialog::base::child::TEXT);
    let says_it = wizard_text(&mut c, body) == want && !want.is_empty();

    c.assert_behaviour(
        "character-select.dialogs.leaving-raises-a-modal-question-in-the-shipped-words",
        move |_| nothing_first && recorded && shaped && says_it,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.answering-yes-to-leaving-runs-the-closing-sequence-and-no-stays
// ---------------------------------------------------------------------------------------------

/// Both answers, because one of them alone cannot tell a client that always quits from one that
/// never does.
pub(super) fn answering_yes_to_leaving_runs_the_closing_sequence_and_no_stays() {
    // No, on its own client.
    let mut c = a_character_list();
    let mut hands = Hands::new();
    let exit = element(&c, EXIT_BUTTON_ID);
    hands.click_handle(&mut c, exit);
    let h = charmgmt_dialog(&mut c, DialogContext::ConfirmExit).expect("the question");
    let no = charmgmt_child(&c, h, dereth_ui::dialog::base::child::BUTTON2);
    hands.click_handle(&mut c, no);
    c.tick(1);
    let no_stays = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT)
        && charmgmt_dialog(&mut c, DialogContext::ConfirmExit).is_none()
        && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_none()
        && with_charmgmt_screen(&mut c, |s| s.rows.len()) == 3;
    c.shutdown();

    // Yes, on another.
    let mut c = a_character_list();
    let mut hands = Hands::new();
    let exit = element(&c, EXIT_BUTTON_ID);
    hands.click_handle(&mut c, exit);
    let h = charmgmt_dialog(&mut c, DialogContext::ConfirmExit).expect("the question");
    let yes = charmgmt_child(&c, h, dereth_ui::dialog::base::child::BUTTON1);
    hands.click_handle(&mut c, yes);
    // The closing sequence specifically, and not merely "somewhere that is not the list". No
    // further frame is run: the closing sequence is the client on its way out, and a step that
    // asserts the client is still running cannot be used past it.
    let yes_leaves = current_screen(&c) == Some(mode::EPILOGUE);

    c.assert_behaviour(
        "character-select.dialogs.answering-yes-to-leaving-runs-the-closing-sequence-and-no-stays",
        move |_| no_stays && yes_leaves,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.deleting-asks-about-the-character-that-was-picked-and-names-only-them
// ---------------------------------------------------------------------------------------------

/// The question names the character the player picked, and no other.
pub(super) fn deleting_asks_about_the_character_that_was_picked() {
    let mut c = a_character_list();
    let mut hands = Hands::new();

    // The third character the shard listed, which is neither the first row of the sorted list nor
    // the first the shard sent -- so mixing up a place in the list with a place in the set cannot
    // pass here.
    let row = charmgmt_row(&mut c, character_id(2));
    hands.click_handle(&mut c, row);
    let picked = with_charmgmt_screen(&mut c, |s| s.selected_id) == character_id(2);

    let delete = element(&c, DELETE_BUTTON_ID);
    hands.click_handle(&mut c, delete);
    let h = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).expect("it raises a box");
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
    let shaped = node_id == ElementId(0x2C) && modal;

    let body = charmgmt_child(&c, h, dereth_ui::dialog::base::child::TEXT);
    let shown = wizard_text(&mut c, body);
    let halves = delete_question_halves(&c);
    let names_them = shown == format!("{}Marbo{}", halves[0], halves[1])
        && !shown.contains("Zephyr")
        && !shown.contains("Aluvia");

    // The box grew to hold the sentence and is centred: an unresized one shows its first line and
    // an uncentred one sits in a corner, and a player sees both.
    let panel = charmgmt_child(&c, h, dereth_ui::dialog::base::child::PANEL);
    let (pb, rb) = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        (
            shell.ui.node(panel).expect("live").region.box_,
            shell.ui.node(h).expect("live").region.box_,
        )
    };
    let laid_out = pb.height() > 125
        && pb.width() == 400
        && (pb.x0, pb.y0)
            == (
                rb.width() / 2 - pb.width() / 2,
                rb.height() / 2 - pb.height() / 2,
            );

    // The three things the player uses are all there, and the box really can be typed into.
    let typing_box = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_FIELD);
    let usable = typing_box != h
        && c.app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .text_element_mut(typing_box)
            .expect("a text element")
            .bits
            .editable();
    let _ = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_ACCEPT);
    let _ = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_CANCEL);

    c.assert_behaviour("character-select.dialogs.deleting-asks-about-the-character-that-was-picked-and-names-only-them", move |_| {
        picked && shaped && names_them && laid_out && usable
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.the-question-is-modal-and-a-second-press-behind-it-reaches-nothing
// ---------------------------------------------------------------------------------------------

/// A second press on the button that raised the box does nothing, and neither does one on another.
pub(super) fn the_question_is_modal_and_a_press_behind_it_reaches_nothing() {
    let mut c = a_character_list();
    let mut hands = Hands::new();
    let delete = element(&c, DELETE_BUTTON_ID);
    let (dx, dy) = middle_of(&c, delete);
    hands.click_handle(&mut c, delete);
    let first = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).expect("one box");
    let one_root = with_charmgmt_screen(&mut c, |s| {
        use dereth_ui::framework::Screen as _;
        s.roots().len()
    }) == 2;

    // Aimed at where the button was. It is behind a modal now, so the press cannot reach it --
    // which is itself the claim, and why this press does not go through the hit test.
    hands.click_at(&mut c, dx, dy);
    let no_second = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter) == Some(first)
        && with_charmgmt_screen(&mut c, |s| {
            use dereth_ui::framework::Screen as _;
            s.roots().len()
        }) == 2;

    // And another button behind the same modal raises nothing either.
    let exit = element(&c, EXIT_BUTTON_ID);
    let (ex, ey) = middle_of(&c, exit);
    hands.click_at(&mut c, ex, ey);
    let nothing_behind = charmgmt_dialog(&mut c, DialogContext::ConfirmExit).is_none();

    c.assert_behaviour("character-select.dialogs.the-question-is-modal-and-a-second-press-behind-it-reaches-nothing", move |_| {
        one_root && no_second && nothing_behind
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.delete.only-the-typed-phrase-deletes-and-it-deletes-the-one-that-was-picked
// ---------------------------------------------------------------------------------------------

/// The one assertion that has to be exactly right: which character.
pub(super) fn only_the_typed_phrase_deletes_and_it_deletes_the_one_that_was_picked() {
    let mut c = a_character_list();
    let mut hands = Hands::new();

    // The phrase is the shipped one, not a word written here.
    let phrase = charmgmt_word(&c, charmgmt::DELETE_RESPONSE_STRING);
    let same_phrase = with_charmgmt_screen(&mut c, |s| s.delete_confirmation_phrase.clone())
        .as_deref()
        == Some(phrase.as_str());

    let row = charmgmt_row(&mut c, character_id(2));
    hands.click_handle(&mut c, row);
    let delete = element(&c, DELETE_BUTTON_ID);
    hands.click_handle(&mut c, delete);
    let h = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).expect("the question");

    let typing_box = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_FIELD);
    hands.click_handle(&mut c, typing_box);
    let caret_in_the_box = what_holds_the_keyboard(&c) == Some(typing_box);

    // A wrong phrase first, so the guard is driven both ways on the same box.
    hands.type_text(&mut c, "NOPE");
    let typed = wizard_text(&mut c, typing_box) == "NOPE";
    let accept = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_ACCEPT);
    press_without_a_frame(&mut c, &mut hands, accept);
    let wrong_deletes_nothing = frame_taking_character_actions(&mut c).is_empty()
        && charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).is_none()
        && charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();

    // Now the real one, in lower case, because the comparison does not care about case.
    let delete = element(&c, DELETE_BUTTON_ID);
    hands.click_handle(&mut c, delete);
    let h = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).expect("a fresh one");
    let typing_box = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_FIELD);
    hands.click_handle(&mut c, typing_box);
    hands.type_text(&mut c, &phrase.to_lowercase());
    let typed_right = wizard_text(&mut c, typing_box) == phrase.to_lowercase();
    let accept = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_ACCEPT);
    press_without_a_frame(&mut c, &mut hands, accept);

    let actions = frame_taking_character_actions(&mut c);
    let the_right_one = actions == vec![CharacterAction::Delete(character_id(2))];
    // The waiting box is up by the time the request exists, and not afterwards.
    let waiting = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_some();

    // And the bytes: the request names the character by their **place in the shard's own list**,
    // so the id above is only right if it maps to the place the player chose. Encoded here and
    // never sent; this client has no shard.
    let (account, slot) = with_charmgmt_screen(&mut c, |s| {
        (
            s.char_set.account.clone(),
            s.char_set
                .set
                .iter()
                .position(|ch| ch.id == character_id(2))
                .expect("in the set"),
        )
    });
    let body = |slot: i32| {
        message_bytes(&dereth_protocol::login::CharacterDeleteRequest {
            account: account.clone(),
            slot_index: slot,
        })
    };
    let by_place = slot == 2 && body(2) != body(0) && body(2) != body(1);

    c.assert_behaviour("character-select.delete.only-the-typed-phrase-deletes-and-it-deletes-the-one-that-was-picked", move |_| {
        same_phrase
            && caret_in_the_box
            && typed
            && wrong_deletes_nothing
            && typed_right
            && the_right_one
            && waiting
            && by_place
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.delete.cancelling-after-typing-the-phrase-deletes-nothing
// ---------------------------------------------------------------------------------------------

/// The player changed their mind, having typed the phrase correctly.
pub(super) fn cancelling_after_typing_the_phrase_deletes_nothing() {
    let mut c = a_character_list();
    let mut hands = Hands::new();
    let phrase = charmgmt_word(&c, charmgmt::DELETE_RESPONSE_STRING);

    let row = charmgmt_row(&mut c, character_id(2));
    hands.click_handle(&mut c, row);
    let delete = element(&c, DELETE_BUTTON_ID);
    hands.click_handle(&mut c, delete);
    let h = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).expect("the question");
    let typing_box = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_FIELD);
    hands.click_handle(&mut c, typing_box);
    hands.type_text(&mut c, &phrase);
    // The phrase that *would* have deleted them.
    let typed = wizard_text(&mut c, typing_box) == phrase;

    let cancel = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_CANCEL);
    press_without_a_frame(&mut c, &mut hands, cancel);

    let nothing_asked = frame_taking_character_actions(&mut c).is_empty();
    let closed = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).is_none()
        && charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none()
        && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_none();
    let names: Vec<String> =
        with_charmgmt_screen(&mut c, |s| s.rows.iter().map(|r| r.name.clone()).collect());
    let everyone_still_there = names == vec!["Aluvia", "Marbo", "Zephyr"]
        && with_charmgmt_screen(&mut c, |s| s.selected_id) == character_id(2);

    c.assert_behaviour(
        "character-select.delete.cancelling-after-typing-the-phrase-deletes-nothing",
        move |_| typed && nothing_asked && closed && everyone_still_there,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.restoring-raises-a-box-with-no-buttons-that-the-next-list-takes-down
// ---------------------------------------------------------------------------------------------

/// A box a player cannot dismiss, and the thing that dismisses it.
pub(super) fn restoring_raises_a_box_with_no_buttons_that_the_next_list_takes_down() {
    let mut c = a_character_list();
    let mut hands = Hands::new();

    // Restoring is only offered on a character already waiting to be deleted, so the fixture puts
    // one in that state the way the shard does.
    let mut set = three_characters();
    set.set[1].seconds_grace_period = 3600;
    c.app_mut().probe_mut().host_state_mut().character_set = Some(set.clone());
    c.tick(1);
    let row = charmgmt_row(&mut c, character_id(1));
    hands.click_handle(&mut c, row);
    let offered = with_charmgmt_screen(&mut c, |s| s.update_buttons().restore);

    let restore = element(&c, RESTORE_BUTTON_ID);
    press_without_a_frame(&mut c, &mut hands, restore);
    let asked =
        frame_taking_character_actions(&mut c) == vec![CharacterAction::Restore(character_id(1))];

    let h = charmgmt_dialog(&mut c, DialogContext::PleaseWait).expect("it raises the waiting box");
    let node_id = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .element_id();
    let want = charmgmt_word(&c, charmgmt::PLEASE_WAIT_STRING);
    let body = charmgmt_child(&c, h, dereth_ui::dialog::base::child::TEXT);
    let shaped = node_id == ElementId(0x31) && wizard_text(&mut c, body) == want;

    // No buttons at all: none of the things a player could press is anywhere under it.
    let no_buttons = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        [
            ElementId(0x17),
            ElementId(0x19),
            charmgmt::TEXT_INPUT_ACCEPT,
            charmgmt::MESSAGE_BUTTON,
        ]
        .iter()
        .all(|id| shell.ui.get_child_recursive(h, *id).is_none())
    };

    // The shard answering with a fresh list is the box's only way down -- a box with no button
    // and nothing to close it is a stuck client.
    set.set[1].seconds_grace_period = 0;
    c.app_mut().probe_mut().host_state_mut().character_set = Some(set);
    c.tick(1);
    let taken_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();

    c.assert_behaviour("character-select.dialogs.restoring-raises-a-box-with-no-buttons-that-the-next-list-takes-down", move |_| {
        offered && asked && shaped && no_buttons && taken_down
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.enter-world.a-double-press-raises-the-waiting-box-before-it-asks-to-log-on
// ---------------------------------------------------------------------------------------------

/// One press picks a character; two press in.
pub(super) fn a_double_press_raises_the_waiting_box_before_it_asks_to_log_on() {
    use dereth_client::platform::keys::MouseButton;

    let mut c = a_character_list();
    let mut hands = Hands::new();
    let row = charmgmt_row(&mut c, character_id(0));
    let (x, y) = middle_of(&c, row);

    // One press: it picks, and it enters nothing -- which is what makes the second meaningful.
    hands.click_at(&mut c, x, y);
    let one_press_picks = with_charmgmt_screen(&mut c, |s| s.selected_id) == character_id(0)
        && charmgmt_dialog(&mut c, DialogContext::EnteringWorld).is_none();

    // The second, at the same place and soon enough to be a double.
    for down in [true, false] {
        let m = hands.button_message(MouseButton::Left, down);
        hands.send(&mut c, m);
    }
    let actions = frame_taking_character_actions(&mut c);

    let h = charmgmt_dialog(&mut c, DialogContext::EnteringWorld)
        .expect("entering the world raises the waiting box");
    let node_id = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .element_id();
    let want = charmgmt_word(&c, charmgmt::ENTERING_WORLD_STRING);
    let body = charmgmt_child(&c, h, dereth_ui::dialog::base::child::TEXT);
    let shaped = node_id == ElementId(0x31) && wizard_text(&mut c, body) == want;
    // ...and it is up by the time the request exists, naming the character that was pressed.
    let asked = actions == vec![CharacterAction::LogOn(character_id(0))];

    c.assert_behaviour("character-select.enter-world.a-double-press-raises-the-waiting-box-before-it-asks-to-log-on", move |_| {
        one_press_picks && shaped && asked
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.an-error-brought-in-with-the-screen-becomes-a-one-button-message
// ---------------------------------------------------------------------------------------------

/// The one box raised by the screen coming up rather than by a press.
pub(super) fn an_error_brought_in_with_the_screen_becomes_a_one_button_message() {
    let mut c = a_character_list();
    let mut hands = Hands::new();
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .flow
        .queue_with_error(mode::CHARACTER_MANAGEMENT, "the world is full".into());
    c.tick(4);

    let h = charmgmt_dialog(&mut c, DialogContext::ErrorMessage).expect("it becomes a box");
    let node_id = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .element_id();
    let body = charmgmt_child(&c, h, dereth_ui::dialog::base::child::TEXT);
    let shaped = node_id == ElementId(0x24) && wizard_text(&mut c, body) == "the world is full";
    // One button, and it is a message box's and not a question's.
    let one_button = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(h, ElementId(0x17))
        .is_none();

    let ok = charmgmt_child(&c, h, charmgmt::MESSAGE_BUTTON);
    hands.click_handle(&mut c, ok);
    let dismissed = charmgmt_dialog(&mut c, DialogContext::ErrorMessage).is_none()
        && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_none();

    c.assert_behaviour(
        "character-select.dialogs.an-error-brought-in-with-the-screen-becomes-a-one-button-message",
        move |_| shaped && one_button && dismissed,
    );
    c.shutdown();
}

// =============================================================================================
// pointer.* -- a real press reaches the screen that owns what it landed on
//
// A test that broadcasts a message onto the tree proves the handler while saying nothing about
// whether a press can reach it; these do not. Eight scenarios, six rows: the three character-list
// buttons are one claim with four arms.
//
// Nothing here puts a message on the tree: every gesture below is the harness's own pointer,
// which builds the client's real window messages and delivers them where the window loop does.
// =============================================================================================

/// The buttons along the bottom of the character list, and the rows above them.
pub(super) const LIST_CREATE: ElementId = ElementId(0x1000_03A0);
pub(super) const LIST_CREDITS: ElementId = ElementId(0x1000_03A3);
pub(super) const LIST_EXIT: ElementId = ElementId(0x1000_03A4);
const LIST_DELETE: ElementId = ElementId(0x1000_039F);

// ---------------------------------------------------------------------------------------------
// pointer.click.the-character-lists-own-buttons-each-raise-what-they-name
// ---------------------------------------------------------------------------------------------

/// Four buttons, four different things, one client -- and the delete question is dismissed in
/// between, because it is a real box and the button behind it cannot be pressed through it.
pub(super) fn the_character_lists_own_buttons_each_raise_what_they_name() {
    use dereth_ui_screens::screens::charmgmt::DialogContext;

    let mut hands = Hands::new();

    // The delete question first: it refuses with nothing chosen, and the list chose a row for
    // itself when it was built.
    let mut c = a_client_on_character_select();
    let something_is_chosen = with_charmgmt_screen(&mut c, |s| s.selected.is_some());
    // ...and the button is something the pointer can land on at all, which is where this starts.
    let it_can_be_pressed = {
        let h = element(&c, LIST_DELETE);
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("alive")
            .is_mouse_visible
    };

    press_the_list_button(&mut c, &mut hands, LIST_DELETE);
    let the_delete_question_is_up =
        with_charmgmt_screen(&mut c, |s| s.open_dialog) == Some(DialogContext::DeleteCharacter);
    // A real box, not just a field: it has an element, and it is in the way.
    let box_handle =
        with_charmgmt_screen(&mut c, |s| s.dialog_element(DialogContext::DeleteCharacter));
    let it_is_a_real_box = box_handle.is_some();
    if let Some(d) = box_handle {
        let cancel = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .get_child_recursive(d, dereth_ui_screens::screens::charmgmt::TEXT_INPUT_CANCEL)
            .expect("the box has a way out");
        hands.click_handle(&mut c, cancel);
    }
    let it_closed =
        with_charmgmt_screen(&mut c, |s| s.dialog_element(DialogContext::DeleteCharacter))
            .is_none();

    // ...and then the question about leaving, on the same client, which is only reachable now the
    // first box is gone.
    press_the_list_button(&mut c, &mut hands, LIST_EXIT);
    let the_leaving_question_is_up = with_charmgmt_screen(&mut c, |s| s.open_dialog)
        == Some(DialogContext::ConfirmExit)
        && with_charmgmt_screen(&mut c, |s| s.dialog_element(DialogContext::ConfirmExit)).is_some();
    let three_real_presses = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .mouse_downs
        == 3;
    c.shutdown();

    // The two that change the screen get a client each, because each of them leaves the list.
    let mut c = a_client_on_character_select();
    press_the_list_button(&mut c, &mut hands, LIST_CREATE);
    let create_puts_the_wizard_up = current_screen(&c) == Some(mode::CHAR_GEN);
    c.shutdown();

    let mut c = a_client_on_character_select();
    press_the_list_button(&mut c, &mut hands, LIST_CREDITS);
    let credits_shows_the_credits = current_screen(&c) == Some(mode::CREDITS);

    c.assert_behaviour(
        "pointer.click.the-character-lists-own-buttons-each-raise-what-they-name",
        move |_| {
            something_is_chosen
                && it_can_be_pressed
                && the_delete_question_is_up
                && it_is_a_real_box
                && it_closed
                && the_leaving_question_is_up
                && three_real_presses
                && create_puts_the_wizard_up
                && credits_shows_the_credits
        },
    );
    c.shutdown();
}

/// Press one of the list's buttons where a person aiming at it would press.
fn press_the_list_button(c: &mut HeadlessClient, hands: &mut Hands, id: ElementId) {
    let h = element(c, id);
    let (x, y) = middle_of(c, h);
    hands.click_at(c, x, y);
}

// ---------------------------------------------------------------------------------------------
// pointer.click.one-press-on-a-character-picks-it-and-two-takes-them-into-the-world
// ---------------------------------------------------------------------------------------------

/// The row pressed is the **last** one, which the list did not choose for itself, so the choice
/// changing is evidence rather than a coincidence.
pub(super) fn one_press_on_a_character_picks_it_and_two_takes_them_into_the_world() {
    use dereth_client::platform::keys::MouseButton;

    let mut c = a_client_on_character_select();
    let rows = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let three_rows = rows.len() == 3;
    // A row the list did **not** choose for itself, so that the choice changing is evidence
    // rather than a coincidence -- which row that is is the list's business and not this
    // scenario's, so it is found rather than named.
    let already = with_charmgmt_screen(&mut c, |s| s.selected_id);
    let last = rows
        .iter()
        .find(|r| r.id != already)
        .expect("the list has a row it did not choose")
        .clone();
    let h = last.element.expect("the row is an element of its own");
    let it_takes_two_presses = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("alive")
        .flags
        .wants_dbl_clicks();
    let not_already_chosen = with_charmgmt_screen(&mut c, |s| s.selected_id) != last.id;

    let (x, y) = middle_of(&c, h);
    let mut hands = Hands::new();
    hands.click_at(&mut c, x, y);
    let one_press_picks_it = with_charmgmt_screen(&mut c, |s| s.selected_id) == last.id;
    let and_no_more = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .character_actions
        == 0;

    // The second press, straight after the first: that is what makes it a double one.
    for down in [true, false] {
        let m = hands.button_message(MouseButton::Left, down);
        hands.send(&mut c, m);
    }
    c.tick(1);
    let two_takes_them_in = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .character_actions
        == 1;

    c.assert_behaviour(
        "pointer.click.one-press-on-a-character-picks-it-and-two-takes-them-into-the-world",
        move |_| {
            three_rows
                && it_takes_two_presses
                && not_already_chosen
                && one_press_picks_it
                && and_no_more
                && two_takes_them_in
        },
    );
    c.shutdown();
}
