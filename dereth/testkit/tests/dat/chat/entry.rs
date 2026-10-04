use super::*;
fn characters_delivered(c: &HeadlessClient) -> u64 {
    c.view()
        .expect_app()
        .ui()
        .expect("the shell is up")
        .stats
        .characters_delivered
}

/// Type `line` one character at a time, checking every one arrived, so a send is never asserted
/// over a line that silently failed to be typed.
fn type_checked(c: &mut HeadlessClient, hand: &mut Hand, entry: ElemHandle, line: &str) {
    let before = characters_delivered(c);
    for ch in line.chars() {
        hand.character(c, ch);
        c.tick(1);
    }
    assert_eq!(
        characters_delivered(c),
        before + line.chars().count() as u64,
        "every character of {line:?} reached the box"
    );
    let text = with_screen(c, |ui, _| {
        ui.text_element_mut(entry)
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    });
    assert_eq!(text, line, "and the box holds the whole line");
}

fn entry_contents(c: &mut HeadlessClient, entry: ElemHandle) -> String {
    with_screen(c, |ui, _| {
        ui.text_element_mut(entry)
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    })
}

/// Pressing return sends the line, empties the box and gives the keyboard back to the world -- and
/// nothing puts the caret straight back a frame later.
pub fn the_return_key_sends_the_line_and_gives_the_keyboard_back() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    let enter = key_of(winit::keyboard::KeyCode::Enter);

    let nothing_yet = focus_of(&c).is_none();
    hand.tap(&mut c, enter);
    let opened = focus_of(&c) == Some(entry);
    // The character that key press also produces, which the box is told to ignore.
    hand.character(&mut c, char::from(0x0D_u8));
    c.tick(1);

    type_checked(&mut c, &mut hand, entry, "hail");
    let nothing_sent = chat_history(&mut c).is_empty();

    let had_it = focus_of(&c) == Some(entry);
    hand.tap(&mut c, enter);
    let gave_it_back = focus_of(&c).is_none();
    let emptied = entry_contents(&mut c, entry).is_empty();
    let sent = chat_history(&mut c) == vec!["hail".to_owned()];

    // With the box blurred the keyboard is the player's again: a movement key walks him, and a
    // character reaches nothing.
    let before = characters_delivered(&c);
    let w = key_of(winit::keyboard::KeyCode::KeyW);
    hand.key(&mut c, w, true);
    c.tick(1);
    let walks = c.view().expect_app().probe().char_input().forward;
    hand.key(&mut c, w, false);
    c.tick(1);
    hand.character(&mut c, 'q');
    c.tick(1);
    let nothing_typed =
        characters_delivered(&c) == before && entry_contents(&mut c, entry).is_empty();

    // And with the box open again the same character lands in it.
    hand.tap(&mut c, enter);
    let reopened = focus_of(&c) == Some(entry);
    hand.character(&mut c, char::from(0x0D_u8));
    c.tick(1);
    hand.character(&mut c, 'q');
    c.tick(1);
    let typed_now = characters_delivered(&c) == before + 1 && entry_contents(&mut c, entry) == "q";

    c.assert_behaviour(
        "chat.entry.the-return-key-sends-the-line-and-gives-the-keyboard-back",
        move |_| {
            nothing_yet
                && opened
                && nothing_sent
                && had_it
                && gave_it_back
                && emptied
                && sent
                && walks
                && nothing_typed
                && reopened
                && typed_now
        },
    );
    c.shutdown();
}

/// Pressing the send button sends the line and keeps the caret for itself, so the player has to
/// press the box again before he can type the next one.
///
/// It is asserted as "the button has it" rather than "nobody has it": those are two different
/// clients and only one of them is this one.
pub fn the_send_button_keeps_the_caret_for_itself() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    let send = element(&c, dereth_ui_screens::chat::window::SEND);

    hand.click_handle(&mut c, entry);
    let focused = focus_of(&c) == Some(entry);
    type_checked(&mut c, &mut hand, entry, "hail");

    let had_it = focus_of(&c) == Some(entry);
    hand.click_handle(&mut c, send);
    let button_has_it = focus_of(&c) == Some(send);
    let emptied = entry_contents(&mut c, entry).is_empty();
    let sent = chat_history(&mut c) == vec!["hail".to_owned()];

    // A button is not something that can be typed in, so nothing reaches anything.
    let before = characters_delivered(&c);
    hand.character(&mut c, 'q');
    c.tick(1);
    let nothing_typed =
        characters_delivered(&c) == before && entry_contents(&mut c, entry).is_empty();

    // The press the player has to make.
    hand.click_handle(&mut c, entry);
    let back = focus_of(&c) == Some(entry);
    hand.character(&mut c, 'q');
    c.tick(1);
    let typed_now = characters_delivered(&c) == before + 1 && entry_contents(&mut c, entry) == "q";

    c.assert_behaviour(
        "chat.entry.the-send-button-keeps-the-caret-for-itself",
        move |_| {
            focused
                && had_it
                && button_has_it
                && emptied
                && sent
                && nothing_typed
                && back
                && typed_now
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// Typing a run, and the two things that made one character overwrite the last
// ---------------------------------------------------------------------------------------------

/// Whether the client is about to eat the next character it is offered.
fn eats_the_next_character(c: &mut HeadlessClient) -> bool {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell is part of the shell")
        .manager
        .text
        .ignore_next_char
}

/// How many key presses the interface has been given -- one per dispatch, so an event that is put
/// back and offered again shows up here as growth on a frame with no key on it.
fn key_presses(c: &HeadlessClient) -> u64 {
    c.view()
        .expect_app()
        .ui()
        .expect("the shell is up")
        .stats
        .key_presses
}

/// Events offered to every part of a frame, claimed by none, and therefore dropped.
fn actions_expired(c: &mut HeadlessClient) -> u64 {
    c.app_mut().actions.stats().expired
}

/// Type `run` one character per frame into `target`, checking **after every character** that the
/// box holds everything typed so far and that exactly one more character arrived.
///
/// A run rather than one character, and a check after each rather than at the end, because the
/// first character always worked and it was the second that threw the first away: a scenario that
/// read only the final text would see one letter where the fault is "four were deleted".
fn type_a_run(c: &mut HeadlessClient, hand: &mut Hand, target: ElemHandle, run: &str) -> bool {
    let mut expected = entry_contents_of(c, target);
    let mut holds = true;
    for (i, ch) in run.chars().enumerate() {
        let before = characters_delivered(c);
        hand.character(c, ch);
        c.tick(1);
        expected.push(ch);
        let one = characters_delivered(c) == before + 1;
        let whole = entry_contents_of(c, target) == expected;
        assert!(
            one && whole,
            "character {i} ({ch:?}): arrived={one} whole={whole}"
        );
        holds &= one && whole;
    }
    holds
}

fn entry_contents_of(c: &mut HeadlessClient, h: ElemHandle) -> String {
    with_screen(c, |ui, _| {
        ui.text_element_mut(h)
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    })
}

/// A whole run of characters arrives in the chat entry, in order, whichever way the box was
/// opened -- none of them throws away what came before.
pub fn a_run_of_characters_all_arrives_in_the_entry() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    let enter = key_of(winit::keyboard::KeyCode::Enter);

    // The key route: the key opens the box and arms the client to eat the character that same key
    // press also produces.
    hand.key(&mut c, enter, true);
    c.tick(1);
    hand.key(&mut c, enter, false);
    c.tick(1);
    let opened = focus_of(&c) == Some(entry) && eats_the_next_character(&mut c);
    let offered_at = characters_delivered(&c);
    hand.character(&mut c, char::from(0x0D_u8));
    c.tick(1);
    // Eaten exactly once, and then the client is hungry no more.
    let eaten = characters_delivered(&c) == offered_at && !eats_the_next_character(&mut c);

    let by_key = type_a_run(&mut c, &mut hand, entry, "hello")
        && entry_contents_of(&mut c, entry) == "hello"
        && characters_delivered(&c) == offered_at + 5;

    // The pointer route, in the same client, so the two halves differ only in how the box was
    // given the caret.
    {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        ui.text_element_mut(entry)
            .expect("a text element")
            .set_text("");
        ui.relinquish_focus(entry);
    }
    c.tick(1);
    let blurred = focus_of(&c).is_none();
    hand.click_handle(&mut c, entry);
    let clicked = focus_of(&c) == Some(entry) && !eats_the_next_character(&mut c);
    let before = characters_delivered(&c);
    let by_pointer = type_a_run(&mut c, &mut hand, entry, "world")
        && entry_contents_of(&mut c, entry) == "world"
        && characters_delivered(&c) == before + 5;

    c.assert_behaviour(
        "chat.entry.a-run-of-characters-all-arrives-and-none-overwrites-the-last",
        move |_| opened && eaten && by_key && blurred && clicked && by_pointer,
    );
    c.shutdown();
}

/// A key press nobody claims is handed round once and then thrown away, rather than being offered
/// again on every frame for ever.
pub fn an_action_nobody_claims_is_dispatched_once() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);

    // A quiet stretch first, so that "it does not grow" is a claim about idleness rather than
    // about a counter nothing writes: it has to grow over the key press below.
    let quiet = key_presses(&c);
    c.tick(5);
    let still_quiet = key_presses(&c) == quiet;

    let expired_before = actions_expired(&mut c);
    hand.tap(&mut c, key_of(winit::keyboard::KeyCode::Enter));
    let arrived = focus_of(&c) == Some(entry) && key_presses(&c) == quiet + 1;
    // And it was claimed by nobody, so it was dropped -- which is what makes the silence below a
    // measurement rather than an empty queue that never had anything in it.
    let dropped = actions_expired(&mut c) > expired_before;

    let after = key_presses(&c);
    let mut silent = true;
    for i in 0..8 {
        c.tick(1);
        let same = key_presses(&c) == after;
        assert!(same, "idle frame {i}: the press was offered again");
        silent &= same;
    }

    c.assert_behaviour(
        "chat.input.a-press-nobody-claims-is-handed-round-once-and-then-dropped",
        move |_| still_quiet && arrived && dropped && silent,
    );
    c.shutdown();
}

/// The client is set to eat the next character on exactly one edge -- the key press that opens a
/// box -- and never otherwise, so no ordinary keystroke is swallowed.
pub fn the_eat_the_next_character_latch_is_armed_on_one_edge() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);

    // A press of the pointer gives the box the caret and arms nothing: a client that armed it
    // every time would eat the first character of every box a player clicks into.
    hand.click_handle(&mut c, entry);
    let clicked = focus_of(&c) == Some(entry) && !eats_the_next_character(&mut c);

    // And it stays unarmed right through a run.
    let mut through_a_run = true;
    for (i, ch) in "abcd".chars().enumerate() {
        let before = characters_delivered(&c);
        hand.character(&mut c, ch);
        c.tick(1);
        let holds = characters_delivered(&c) == before + 1 && !eats_the_next_character(&mut c);
        assert!(holds, "character {i}");
        through_a_run &= holds;
    }
    let typed = entry_contents_of(&mut c, entry) == "abcd";

    // The one edge that does arm it: a key press that turns typing on.
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .relinquish_focus(entry);
    c.tick(1);
    let losing_it_arms_nothing = !eats_the_next_character(&mut c);
    let enter = key_of(winit::keyboard::KeyCode::Enter);
    hand.key(&mut c, enter, true);
    c.tick(1);
    hand.key(&mut c, enter, false);
    c.tick(1);
    let armed = eats_the_next_character(&mut c);

    // And exactly one character pays for it.
    let before = characters_delivered(&c);
    hand.character(&mut c, char::from(0x0D_u8));
    c.tick(1);
    let ate_one = characters_delivered(&c) == before;
    hand.character(&mut c, 'z');
    c.tick(1);
    let only_one = characters_delivered(&c) == before + 1;

    c.assert_behaviour(
        "chat.input.the-client-is-set-to-eat-a-character-on-one-edge-and-never-otherwise",
        move |_| {
            clicked
                && through_a_run
                && typed
                && losing_it_arms_nothing
                && armed
                && ate_one
                && only_one
        },
    );
    c.shutdown();
}
