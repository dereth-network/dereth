use super::*;

// =============================================================================================
// abuse.response.* / abuse.report.*
//
// Retail keeps `0x04B8`..`0x04BA` silent in the failure-event handler. The shared arm emits an
// abuse-report response notice, and the abuse-report panel changes the shipped result text when
// it receives that notice.
//
// It adds no chat line, and it neither opens nor closes the window.
//
// **The window is opened by its own bound action.** The shipped UI action is otherwise unbound,
// so the action itself goes in rather than a key, which is `crate::input_steps`' rule -- what key
// an action is bound to is the keymap's claim and has
// rows of its own, so a scenario that pressed a key would be asserting over the shipped binding as
// well as over what the action does.
// =============================================================================================

/// `UICommands`' `Show/Hide Abuse Panel`, the action the shipped keymap leaves user-bindable.
const TOGGLE_ABUSE: dereth_input::ActionId = dereth_input::ActionId(0x1000_0003);

/// Fire that action once, and run the frames the window it opens needs to reach the screen.
fn abuse_toggle_the_window(c: &mut HeadlessClient) {
    dereth_testkit::input_steps::press_on_map(
        c,
        TOGGLE_ABUSE,
        dereth_testkit::input_steps::UI_COMMANDS,
    );
    c.tick(2);
}

/// The shipped abuse-report panel, taken from its own binding rather than found by a tree walk.
fn abuse_window(c: &HeadlessClient) -> ElemHandle {
    c.view()
        .expect_app()
        .hud()
        .panels
        .abuse
        .panel
        .expect("the shipped classic_gameplay layout carries a reporting panel and it is bound")
}

/// One child of that window.
fn abuse_child(c: &HeadlessClient, id: ElementId) -> ElemHandle {
    let window = abuse_window(c);
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(window, id)
        .unwrap_or_else(|| panic!("the shipped abuse window has no child {id:?}"))
}

/// What one of its children says.
fn abuse_text(c: &mut HeadlessClient, id: ElementId) -> String {
    let h = abuse_child(c, id);
    glyph_runs(c.app_mut(), h).0
}

/// One shipped sentence, by the `ID_*` name the window looks it up under.
///
/// The table is the one the mapper names for the window's own enum, and the id is the same hash the
/// client computes from the name -- neither is written down here.
#[allow(deprecated)]
fn abuse_shipped_sentence(c: &HeadlessClient, token: &str) -> String {
    let table = dereth_ui_screens::env::did_by_enum(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        4,
        abuse::STRING_TABLE_ENUM,
    )
    .expect("the shipped string-table mapper row");
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .resolve_string(table, dereth_ui::persist::preferences::token_of(token))
        .unwrap_or_else(|| panic!("{token} is in the shipped string table"))
}

/// Whether the window is on screen.
fn abuse_window_is_up(c: &HeadlessClient) -> bool {
    let window = abuse_window(c);
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .is_visible(window)
}

/// The state an element of the window is in -- which page the window is on, or whether a button is
/// out of reach.
fn abuse_state(c: &HeadlessClient, h: ElemHandle) -> dereth_ui::StateId {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("the element is alive")
        .state
}

/// A real press on one of the window's own controls, with **whether the pointer could reach it**
/// handed back: a press that landed on something drawn over the control would be measuring nothing.
fn abuse_press(c: &mut HeadlessClient, id: ElementId) -> bool {
    let h = abuse_child(c, id);
    let (at, reached) = {
        let view = c.view();
        let ui = &view.expect_app().ui().expect("the UI shell is up").ui;
        let b = ui.screen_clip_box(h);
        assert!(
            b.is_valid(),
            "{id:?} is clipped to nothing, so no pointer can reach it"
        );
        let p = ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        let hit = ui.hit_test_screen(p.x, p.y);
        (p, hit.is_some_and(|x| x == h || ui.is_ancestor_of(h, x)))
    };
    c.when(Player::Click(Target::Point(at)));
    c.tick(1);
    reached
}

/// Every ordered game action this client really put on a datagram since the last look, envelope
/// stripped: `[sub-type][body]`.
///
/// It is the wire and not the frame's outbox on purpose -- the report's own bytes are what is
/// pinned, and a request has no bytes until it is framed.
fn abuse_ordered_actions(c: &mut HeadlessClient) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let taken = c
        .replay_net_mut()
        .expect("this scenario reads the wire, so its client has a shard attached")
        .take_outgoing();
    for (bytes, _) in taken {
        let Ok(packet) = dereth_transport::wire::ParsedPacket::parse(&bytes) else {
            continue;
        };
        for f in packet.fragments {
            let p = f.payload;
            if f.header.queue_id == 3
                && p.len() >= 12
                && u32::from_le_bytes(p[0..4].try_into().expect("four bytes"))
                    == dereth_testkit::outbound::ORDERED_ACTION
            {
                out.push(p[8..].to_vec());
            }
        }
    }
    out
}

/// One `CommunicationWeenieError`, delivered the way the session layer delivers one.
fn abuse_shard_answers(c: &mut HeadlessClient, code: u32) {
    deliver(
        c,
        &dereth_protocol::comms::CommunicationWeenieError { error_type: code },
    );
    c.tick(2);
}

/// **The three answers reach the window and stop there.** The result line becomes the shipped
/// sentence for the answer, nothing is said in chat, an answer with no sentence leaves the line
/// alone, and neither opening nor closing the window is part of it -- so an answer that arrives
/// while the window is shut is waiting there when the player opens it again.
pub(super) fn the_shards_abuse_answer_writes_the_result_line_and_nothing_in_chat() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));

    let it_starts_shut = !abuse_window_is_up(&c);
    abuse_toggle_the_window(&mut c);
    let the_action_opens_it = abuse_window_is_up(&c);

    let no_such_character = abuse_shipped_sentence(&c, "ID_Abuse_Response_NoSuchCharacter");
    let before = abuse_text(&mut c, abuse::RESULT_TEXT);
    abuse_shard_answers(&mut c, abuse::response::NO_SUCH_CHARACTER);
    let line = abuse_text(&mut c, abuse::RESULT_TEXT);
    let the_first_answer = line != before && line == no_such_character;
    let and_nothing_was_said_in_chat = c.chat_lines().is_empty();

    // There is no visibility gate on the receiver. Shut the window, prove the next answer still
    // writes the same child, and that it did not re-open the window to do it.
    abuse_toggle_the_window(&mut c);
    let the_action_shuts_it_too = !abuse_window_is_up(&c);
    let self_report = abuse_shipped_sentence(&c, "ID_Abuse_Response_Self");
    abuse_shard_answers(&mut c, abuse::response::SELF_REPORT);
    let a_shut_window_still_receives =
        !abuse_window_is_up(&c) && abuse_text(&mut c, abuse::RESULT_TEXT) == self_report;

    // The default arm is a true no-op, not a write of the empty string.
    abuse_shard_answers(&mut c, 0);
    let an_unknown_answer_changes_nothing = abuse_text(&mut c, abuse::RESULT_TEXT) == self_report;

    // The success arm reaches the same place through the failure that carries free-form text, and
    // that text is not what the window draws.
    let success = abuse_shipped_sentence(&c, "ID_Abuse_Response_Success");
    deliver(
        &mut c,
        &dereth_protocol::comms::CommunicationWeenieErrorWithString {
            error_type: abuse::response::SUCCESS,
            text: "not part of this response".to_owned(),
        },
    );
    c.tick(2);
    let the_success_sentence = abuse_text(&mut c, abuse::RESULT_TEXT) == success;

    abuse_toggle_the_window(&mut c);
    let the_last_answer_is_waiting =
        abuse_window_is_up(&c) && abuse_text(&mut c, abuse::RESULT_TEXT) == success;
    let and_still_nothing_in_chat = c.chat_lines().is_empty();

    c.assert_behaviour(
        "abuse.response.the-shards-answer-writes-the-result-line-and-nothing-in-chat",
        move |_| {
            it_starts_shut
                && the_action_opens_it
                && the_first_answer
                && and_nothing_was_said_in_chat
                && the_action_shuts_it_too
                && a_shut_window_still_receives
                && an_unknown_answer_changes_nothing
                && the_success_sentence
                && the_last_answer_is_waiting
                && and_still_nothing_in_chat
        },
    );
    c.shutdown();
}

/// **The window, driven the way a player drives it.** The name comes off whoever is selected, the
/// complaint is typed, Continue sends one report and only then, the window waits for the shard, and
/// Done empties it again.
pub(super) fn the_abuse_page_sends_one_report_and_then_empties_itself() {
    const REPORTER: ObjectId = ObjectId(0x5000_0041);
    const TARGET: ObjectId = ObjectId(0x5000_0042);

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // A socket-free endpoint with one established connection, so that what the window sends is
    // framed into a datagram this scenario can read. Nothing binds and nothing leaves.
    let _shard = Peer::attach(&mut c, REPORTER);
    {
        let mut reporter = dereth_client_model::Weenie::new(REPORTER);
        reporter.pwd.bitfield |= dereth_client_model::weenie::bitfield::PLAYER;
        reporter.pwd.name = "Reporter".into();
        let mut target = dereth_client_model::Weenie::new(TARGET);
        target.pwd.bitfield |= dereth_client_model::weenie::bitfield::PLAYER;
        target.pwd.name = "Target Player".into();
        let w = c.world_mut();
        w.tables.weenies.insert(REPORTER, reporter);
        w.tables.weenies.insert(TARGET, target);
        w.player = Some(REPORTER);
        w.selected = Some(TARGET);
    }
    c.tick(2);
    // Whatever the login and the first frames put on the wire is not this scenario's.
    let _ = abuse_ordered_actions(&mut c);

    abuse_toggle_the_window(&mut c);
    let the_window_is_up = abuse_window_is_up(&c);
    let window = abuse_window(&c);
    // The window comes up on neither of its two numbered pages -- the shipped layout gives it no
    // state at all until something moves it -- and what matters here is that it is not already on
    // the entry page, or the advance below would be no advance.
    let it_does_not_start_on_the_entry_page = abuse_state(&c, window) != abuse::PAGE_TWO;

    // Page one's own button: it advances to the entry page and copies the selected player's name.
    let pressed_the_name_button = abuse_press(&mut c, abuse::SELECTED_NAME_BUTTON);
    let it_advanced = abuse_state(&c, window) == abuse::PAGE_TWO;
    let it_took_the_selected_name = abuse_text(&mut c, abuse::NAME_ENTRY) == "Target Player";

    // The complaint, typed into the box the caret was put in.
    let pressed_the_complaint_box = abuse_press(&mut c, abuse::COMPLAINT_ENTRY);
    c.when(Player::Type("Repeated unwanted tells".to_owned()));
    c.tick(1);
    let the_complaint_is_there =
        abuse_text(&mut c, abuse::COMPLAINT_ENTRY) == "Repeated unwanted tells";
    let continue_button = abuse_child(&c, abuse::CONTINUE_BUTTON);
    // Both fields non-empty is what arms Continue.
    let continue_is_armed =
        abuse_state(&c, continue_button) == dereth_ui::widgets::button::state::NORMAL;
    let typing_alone_sends_nothing = abuse_ordered_actions(&mut c).is_empty();

    let pressed_continue = abuse_press(&mut c, abuse::CONTINUE_BUTTON);
    c.tick(3);
    let actions = abuse_ordered_actions(&mut c);

    // The literal oracle: the sub-type, the reported name, the status word, and the complaint.
    let mut expected = Vec::new();
    expected.extend_from_slice(&0x0140_u32.to_le_bytes());
    expected.extend_from_slice(&13_u16.to_le_bytes());
    expected.extend_from_slice(b"Target Player");
    expected.push(0);
    expected.extend_from_slice(&1_u32.to_le_bytes());
    expected.extend_from_slice(&23_u16.to_le_bytes());
    expected.extend_from_slice(b"Repeated unwanted tells");
    expected.extend_from_slice(&[0, 0, 0]);
    let one_exact_report = actions == vec![expected];

    let wait_text = abuse_shipped_sentence(&c, "ID_Abuse_PageThree_WaitText");
    let it_says_it_is_waiting = abuse_text(&mut c, abuse::RESULT_TEXT) == wait_text;

    let success = abuse_shipped_sentence(&c, "ID_Abuse_Response_Success");
    abuse_shard_answers(&mut c, abuse::response::SUCCESS);
    let the_answer_replaces_the_wait_text = abuse_text(&mut c, abuse::RESULT_TEXT) == success;

    let pressed_done = abuse_press(&mut c, abuse::DONE_BUTTON);
    let it_went_back_to_page_one = abuse_state(&c, window) == abuse::PAGE_ONE;
    let both_fields_are_empty = abuse_text(&mut c, abuse::NAME_ENTRY).is_empty()
        && abuse_text(&mut c, abuse::COMPLAINT_ENTRY).is_empty();
    let continue_button = abuse_child(&c, abuse::CONTINUE_BUTTON);
    let continue_is_out_of_reach_again =
        abuse_state(&c, continue_button) == dereth_ui::widgets::button::state::DISABLED;
    let done_sends_nothing_more = abuse_ordered_actions(&mut c).is_empty();

    c.assert_behaviour(
        "abuse.report.the-page-sends-one-report-naming-who-and-why-and-then-empties-itself",
        move |_| {
            the_window_is_up
                && it_does_not_start_on_the_entry_page
                && pressed_the_name_button
                && it_advanced
                && it_took_the_selected_name
                && pressed_the_complaint_box
                && the_complaint_is_there
                && continue_is_armed
                && typing_alone_sends_nothing
                && pressed_continue
                && one_exact_report
                && it_says_it_is_waiting
                && the_answer_replaces_the_wait_text
                && pressed_done
                && it_went_back_to_page_one
                && both_fields_are_empty
                && continue_is_out_of_reach_again
                && done_sends_nothing_more
        },
    );
    c.shutdown();
}
