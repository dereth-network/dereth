use super::*;
/// The chat window's talk-to button. The row of its menu that is the general channel is
/// `GENERAL_ROW` above.
pub(super) const TALK_TO_BUTTON: ElementId = ElementId(0x1000_0014);

/// The recording whose login this section takes its character set and room list from.
pub(super) const ROOMS_SESSION: &str = "short-second-connection";

/// The player this section's client is.
const ME: ObjectId = ObjectId(0x5000_0017);

/// Hand the client the character set the recording's login carried, with the chat-room permission
/// as given -- the negative is a labelled mutation of the shard's own answer.
fn character_set(c: &mut HeadlessClient, permitted: bool) {
    let mut set: LoginCharacterSet = dereth_protocol::read_body(&recorded(0xF658)[4..])
        .expect("the recorded character set decodes");
    assert_ne!(
        set.use_turbine_chat, 0,
        "the recording's own answer permits it"
    );
    if !permitted {
        set.use_turbine_chat = 0;
    }
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::CharacterSet(Box::new(set))]);
}

/// Hand the client the room list the recording's login carried.
fn room_list(c: &mut HeadlessClient) -> dereth_protocol::comms::ChatRoomMembership {
    use dereth_protocol::Message as _;
    let blob = recorded(0x0295);
    let tracker = dereth_protocol::comms::ChatRoomMembership::read(
        &mut dereth_protocol::archive::Reader::body(&blob[4..]),
    )
    .expect("the recorded room list decodes");
    c.when(Inbound::event(SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode(0x0295),
        blob,
    }));
    tracker
}

/// Every chat-room message the client has produced, in order.
fn room_messages(c: &HeadlessClient) -> Vec<dereth_protocol::turbine::SendToRoomById> {
    c.view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::TurbineChat(m) => Some(m.clone()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// chat.turbine.the-general-channel-carries-a-typed-line-to-its-room
// ---------------------------------------------------------------------------------------------

/// The shard's permission starts the service, the player picks the general channel, types a line,
/// and one chat-room message goes out -- with the room, the sender and the kind of line on it, and
/// on the queue that carries no ordering stamp.
pub fn the_general_channel_carries_a_typed_line_to_its_room() {
    let mut c = a_bare_client(true);
    // The shard's answer is the gate: with the permission withheld the service does not start.
    character_set(&mut c, false);
    let refused = !c.view().world().chat.using_turbine_chat;
    character_set(&mut c, true);
    let started = c.view().world().chat.using_turbine_chat;

    c.world_mut().player = Some(ME);
    describe(&mut c, true, true);
    let tracker = room_list(&mut c);
    assert_ne!(
        tracker.general_room, 0,
        "the recording names a general room"
    );
    to_gameplay(&mut c);

    let mut hand = Hand::new();
    pick_general(&mut c, &mut hand);
    let undeliverable = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .requests_undeliverable;
    hand.say(&mut c, "General oracle");

    let sent = room_messages(&c);
    let one_message = sent.len() == 1
        && (
            sent[0].context,
            sent[0].room,
            sent[0].sender,
            sent[0].chat_type,
        ) == (1, tracker.general_room, ME.0, 1)
        && sent[0].text == "General oracle";
    // And it is a room line, not an ordinary say.
    let not_a_say = !c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .any(|r| matches!(r, dereth_client_model::Request::Talk(_)));
    // A client with no link records the line as undeliverable rather than as sent.
    let recorded_as_undeliverable = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .requests_undeliverable
        == undeliverable + 1;

    // The bytes, through the production sender over a mock transport. **Nothing is sent.**
    let mut session = Session::new(MockTransport::new());
    let put_on_the_wire = dereth_client_runtime::requests::send_request(
        &mut session,
        &dereth_client_model::Request::TurbineChat(sent[0].clone()),
    );
    let wire = &session.transport.sent[0];
    let framed = (wire.queue, wire.ordered) == (NetQueue::Logon, false)
        && wire.payload[..4] == 0xF7DE_u32.to_le_bytes()
        && wire.payload[8..] == *sent[0].network_packet().expect("the packet composes");

    // The service belongs to the client and not to the screen: rebuild the screen, restart the
    // service, and the next line carries on from where the last one left off.
    let old_root = element(&c, dereth_ui_screens::chat::window::LOG);
    to_gameplay(&mut c);
    let screen_is_new = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell is up")
        .ui
        .node(old_root)
        .is_none();
    character_set(&mut c, false);
    character_set(&mut c, true);
    // How fast the player may talk is measured against the wall clock, so a real gap is the only
    // way to drain it -- an injected one would be asserting over a bucket the client does not use.
    std::thread::sleep(std::time::Duration::from_secs(2));
    hand.say(&mut c, "@general explicit line");
    let sent = room_messages(&c);
    let explicit = sent.len() == 1
        && (sent[0].context, sent[0].room, sent[0].chat_type) == (2, tracker.general_room, 2)
        && sent[0].text == "explicit line";

    c.assert_behaviour(
        "chat.turbine.the-general-channel-carries-a-typed-line-to-its-room",
        move |_| {
            refused
                && started
                && one_message
                && not_a_say
                && recorded_as_undeliverable
                && put_on_the_wire
                && framed
                && screen_is_new
                && explicit
        },
    );
    c.shutdown();
}

/// A line typed before the shard has named a room is refused, and the next one is not.
pub fn a_line_typed_before_the_shard_names_a_room_is_refused() {
    let mut c = a_bare_client(true);
    character_set(&mut c, true);
    describe(&mut c, true, true);
    to_gameplay(&mut c);

    let mut hand = Hand::new();
    pick_general(&mut c, &mut hand);
    hand.say(&mut c, "no room yet");
    let refused = !c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .any(|r| {
            matches!(
                r,
                dereth_client_model::Request::TurbineChat(_)
                    | dereth_client_model::Request::Talk(_)
            )
        });
    // And the player is left on the channel he picked rather than being moved off it.
    let still_general =
        c.view().world().chat.talk_focus == dereth_client_model::chat::TalkFocus::General;

    room_list(&mut c);
    hand.say(&mut c, "room received");
    let sent = room_messages(&c);
    let now_sent = sent.len() == 1 && sent[0].context == 1 && sent[0].chat_type == 1;

    c.assert_behaviour(
        "chat.turbine.a-line-typed-before-the-shard-names-a-room-is-refused",
        move |_| refused && still_general && now_sent,
    );
    c.shutdown();
}

/// The service is the client's and not the interface's: it runs with no interface at all, and a
/// restart does not put it back to the beginning.
pub fn the_room_service_runs_with_no_interface_at_all() {
    let mut c = a_bare_client(false);
    character_set(&mut c, false);
    let refused = !c.view().world().chat.using_turbine_chat;

    let set: LoginCharacterSet = dereth_protocol::read_body(&recorded(0xF658)[4..])
        .expect("the recorded character set decodes");
    let mut desc = dereth_protocol::login::LoginPlayerDescription::default();
    desc.player_module.options2 |= 0x100;
    c.when(Inbound::event(SessionEvent::CharacterSet(Box::new(set))));
    c.when(Inbound::event(SessionEvent::PlayerDescription(Box::new(
        desc,
    ))));
    c.when(Inbound::event(SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode(0x0295),
        blob: recorded(0x0295),
    }));
    let offered = c
        .view()
        .world()
        .chat
        .is_talk_focus_enabled(dereth_client_model::chat::TalkFocus::General);

    let mut req = dereth_client_model::RecordingRequests::default();
    let first = c.world_mut().send_turbine_chat(
        &mut req,
        dereth_client_model::chat::TalkFocus::General,
        true,
        "no interface first",
        4,
        100,
    );
    c.tick(1);
    character_set(&mut c, false);
    character_set(&mut c, true);
    c.tick(1);
    let no_shell = c.view().expect_app().ui().is_none();
    let second = c.world_mut().send_turbine_chat(
        &mut req,
        dereth_client_model::chat::TalkFocus::General,
        true,
        "no interface next",
        4,
        102,
    );
    let dereth_client_model::Request::TurbineChat(m) = &req.0[1] else {
        panic!("the second line is a room message")
    };
    // The count carries on rather than starting again, and the room is still the one the shard
    // named before the restart.
    let carried_on = (m.context, m.room) == (2, 2);

    c.assert_behaviour(
        "chat.turbine.the-service-runs-with-no-interface-at-all",
        move |_| refused && offered && first && no_shell && second && carried_on,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chat.turbine.the-hosts-own-spelling-is-what-goes-on-the-wire
// ---------------------------------------------------------------------------------------------

/// Type `text` into the chat entry through the wide-character boundary and press return.
///
/// The character-message gesture [`Hand::type_text`] makes is a byte one and is deliberately
/// ASCII, so a line that is about the host's own spelling of a word enters at the wide boundary
/// instead. It still goes through the real editing, the real history and the real Enter, and it
/// claims nothing about a keyboard or an input method.
fn wide_entry(c: &mut HeadlessClient, hand: &mut Hand, text: &str) {
    hand.click_element(c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        for unit in text.encode_utf16() {
            ui.character(unit);
        }
    }
    c.tick(1);
    hand.press_return(c);
}

/// One journey of `input` through a host that spells its text in `code_page`.
fn conversion_journey(code_page: u32, input: &str, native: &[u8], room_text: &str) -> bool {
    let mut c = a_bare_client(true);
    character_set(&mut c, true);
    describe(&mut c, true, true);
    // **The host is constructed, not the machine's.** Reading the machine's own code page and
    // measuring against it would make the claim depend on the locale of whatever ran it. Naming
    // the code page here is the same claim without that.
    c.world_mut().chat.text_conversion = dereth_client_model::HostText::new(std::sync::Arc::new(
        dereth_client_runtime::platform::text::HostAcp::for_ansi_code_page(code_page),
    ));
    room_list(&mut c);
    to_gameplay(&mut c);

    let mut hand = Hand::new();
    wide_entry(&mut c, &mut hand, input);
    let ordinary = c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .find(|r| matches!(r, dereth_client_model::Request::Talk(_)))
        .expect("an ordinary line has a producer")
        .clone();
    let dereth_client_model::Request::Talk(talk) = &ordinary else {
        unreachable!("matched above")
    };
    let spelled =
        dereth_primitives::text::cp1252::encode(&talk.message).expect("the line spells") == native;

    let mut session = Session::new(MockTransport::new());
    assert!(dereth_client_runtime::requests::send_request(
        &mut session,
        &ordinary
    ));
    let wire = &session.transport.sent[0];
    // The framing, assembled here rather than by the writer being asserted over.
    let mut want: Vec<u8> = [0xF7B1_u32, 1, 0x15]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    want.extend(u16::try_from(native.len()).expect("short").to_le_bytes());
    want.extend(native);
    while want.len() % 4 != 0 {
        want.push(0);
    }
    let framed = wire.payload == want && (wire.queue, wire.ordered) == (NetQueue::Weenie, true);

    // What the player typed is what his own history holds, whatever the host could spell.
    let kept = c
        .view()
        .world()
        .chat
        .entries
        .get(&8)
        .and_then(|entry| entry.history().last())
        .map(String::as_str)
        == Some(input);

    // The same line on the general channel: the same host spells it, and the room message carries
    // that spelling rather than the ordinary one's bytes.
    pick_general(&mut c, &mut hand);
    wide_entry(&mut c, &mut hand, input);
    let room = c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .find(|r| matches!(r, dereth_client_model::Request::TurbineChat(_)))
        .expect("the general channel has a producer")
        .clone();
    let dereth_client_model::Request::TurbineChat(message) = &room else {
        unreachable!("matched above")
    };
    let same_spelling = message.text == room_text;
    assert!(dereth_client_runtime::requests::send_request(
        &mut session,
        &room
    ));
    let wire = &session.transport.sent[1];
    let decoded: dereth_protocol::turbine::SendToRoomById =
        dereth_protocol::read_body(&wire.payload[4..]).expect("the room message decodes");
    let on_the_other_queue =
        (wire.queue, wire.ordered) == (NetQueue::Logon, false) && decoded.text == room_text;
    // And it is a room line and not also an ordinary one.
    let not_also_a_say = !c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .any(|r| matches!(r, dereth_client_model::Request::Talk(_)));

    c.shutdown();
    spelled && framed && kept && same_spelling && on_the_other_queue && not_also_a_say
}

/// What the host can spell is what goes on the wire, for an ordinary line and for a room line
/// alike -- and a word it cannot spell is written out in the client's own escape rather than lost.
pub fn the_hosts_own_spelling_is_what_goes_on_the_wire() {
    // A host that spells the western alphabet: the accented letter it has and the ideograph it has
    // not are both written out in the escape, because one unspellable unit escapes the whole word.
    let western = conversion_journey(
        1252,
        "cafe\u{e9}\u{4e00}",
        b"cafe<00e9><4e00>",
        "cafe<00e9><4e00>",
    );
    // A host that spells the ideographs: the same three characters keep their own bytes going out
    // and come back as themselves on the room message.
    let eastern = conversion_journey(
        932,
        "\u{65e5}\u{672c}\u{8a9e}",
        &[0x93, 0xfa, 0x96, 0x7b, 0x8c, 0xea],
        "\u{65e5}\u{672c}\u{8a9e}",
    );

    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.assert_behaviour(
        "chat.turbine.the-hosts-own-spelling-is-what-goes-on-the-wire",
        move |_| western && eastern,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The incoming half: a packet on the chat-room queue, and what the log does with it
// ---------------------------------------------------------------------------------------------

/// The room every packet below is addressed to.
const GENERAL_ROOM: u32 = 0x1234_5678;

fn wide(out: &mut Vec<u8>, value: &str) {
    let units: Vec<u16> = value.encode_utf16().collect();
    assert!(units.len() < 128, "the length prefix is one byte");
    out.push(u8::try_from(units.len()).expect("checked above"));
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
}

/// The envelope a chat-room packet arrives in.
///
/// `over_long` adds the one fixed overshoot the reference server sends, which the client accepts
/// and counts rather than refusing.
fn envelope(kind: u32, body: Vec<u8>, over_long: bool) -> Vec<u8> {
    let extra = u32::from(over_long) * 8;
    let mut raw = Vec::new();
    word(
        &mut raw,
        u32::try_from(body.len()).expect("small") + 32 + extra,
    );
    for v in [kind, 1, 1, 0xB00B5, 1, 0xB00B5, 0] {
        word(&mut raw, v);
    }
    word(&mut raw, u32::try_from(body.len()).expect("small") + extra);
    raw.extend(body);
    raw
}

/// One line spoken in a room.
fn room_event(room: u32, name: &str, text: &str, over_long: bool) -> Vec<u8> {
    let mut body = Vec::new();
    word(&mut body, room);
    wide(&mut body, name);
    wide(&mut body, text);
    // The metadata is deliberately not the general room's: what kind of line it is drawn as comes
    // from the **room**, not from this.
    for v in [12, 0x5000_0017, 0x8007_0005, 10] {
        word(&mut body, v);
    }
    envelope(1, body, over_long)
}

/// The service's answer about a line the player sent.
fn room_answer(context: u32, result: u32) -> Vec<u8> {
    envelope(
        5,
        [context, 2, 2, result]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
        true,
    )
}

/// A client with the chat-room service up, a general room, and the gameplay screen settled.
fn a_client_in_a_room(shell: bool) -> HeadlessClient {
    let mut c = a_bare_client(shell);
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::CharacterSet(Box::new(
            LoginCharacterSet {
                use_turbine_chat: 1,
                ..LoginCharacterSet::default()
            },
        ))]);
    c.when(Inbound::event(SessionEvent::PlayerDescription(
        Box::default(),
    )));
    let rooms = dereth_protocol::comms::ChatRoomMembership {
        general_room: GENERAL_ROOM,
        ..dereth_protocol::comms::ChatRoomMembership::default()
    };
    c.when(Inbound::event(SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode(0x0295),
        blob: dereth_protocol::write_blob(&rooms).expect("the room list encodes"),
    }));
    to_gameplay(&mut c);
    c
}

/// Put `raw` on the chat-room queue through a real session, and deliver whatever it decides that
/// is. **No socket**: the transport is a mock that is handed the bytes directly.
fn deliver_room_packet(c: &mut HeadlessClient, session: &mut Session<MockTransport>, raw: Vec<u8>) {
    session.transport.deliver(IncomingMessage {
        opcode: 0xF7DE,
        body: raw.clone(),
        queue: NetQueue::Logon,
        sender: RecipientId(0),
        blob_id: NetBlobId(1),
    });
    session.tick(LocalTime(0.0));
    let events: Vec<SessionEvent> = session.drain_events().collect();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SessionEvent::TurbineChat(b) if b == &raw)),
        "a packet on that queue is a chat-room line"
    );
    for e in events {
        c.when(Inbound::event(e));
    }
}

/// Send a line to the general room through the client's own guarded producer and return the number
/// the answer will name it by.
fn a_line_in_flight(c: &mut HeadlessClient, text: &str, now: i32) -> u32 {
    c.world_mut().player_system.options.set(27, true);
    let mut out = dereth_client_model::RecordingRequests::default();
    assert!(c.world_mut().send_turbine_chat(
        &mut out,
        dereth_client_model::chat::TalkFocus::General,
        false,
        text,
        4,
        now,
    ));
    let Some(dereth_client_model::Request::TurbineChat(request)) = out.0.pop() else {
        panic!("the producer makes a room message")
    };
    request.context
}

/// A line spoken in a room is drawn under that room's own name, in that room's own colour, with
/// the speaker as something the player can click -- once, and not again on the next frame.
pub fn a_room_line_reaches_the_log_with_its_rooms_name_and_colour() {
    let mut c = a_client_in_a_room(true);
    let mut session = Session::new(MockTransport::new());

    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "Sender", "a line", false),
    );
    c.tick(1);
    let shown = log_text(&mut c);
    let drawn = shown.contains("[General] Sender says, \"a line\"");

    let (coloured, tagged) = {
        let h = element(&c, dereth_ui_screens::chat::window::LOG);
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let glyphs = &ui
            .text_element_mut(h)
            .expect("the log is a text element")
            .glyphs
            .glyphs;
        (
            glyphs.iter().any(|g| g.color == 0xFFB4_DCF0),
            glyphs.iter().any(|g| g.tag.is_some()),
        )
    };

    c.tick(1);
    let not_replayed = log_text(&mut c) == shown;

    // The same line in the one over-long shape the reference server sends: accepted, drawn, and
    // counted as the overshoot it is rather than refused.
    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "Next", "over long", true),
    );
    c.tick(1);
    let over_long = log_text(&mut c).contains("[General] Next says, \"over long\"")
        && c.view().world().chat.turbine_extent_discrepancies() == 1;

    c.assert_behaviour(
        "chat.turbine.a-room-line-reaches-the-log-with-its-rooms-own-name-and-colour",
        move |_| drawn && coloured && tagged && not_replayed && over_long,
    );
    c.shutdown();
}

/// An answer about a line sent from a screen that has since been rebuilt completes the line
/// without reviving the log it was typed into, and the next one is drawn in the new log.
pub fn an_answer_that_arrives_after_the_screen_has_gone() {
    let mut c = a_client_in_a_room(true);
    let mut session = Session::new(MockTransport::new());

    let first = a_line_in_flight(&mut c, "old generation", 100);
    let old_log = element(&c, dereth_ui_screens::chat::window::LOG);
    deliver_room_packet(&mut c, &mut session, room_answer(first, 0x8000_4005));
    // The line is finished with before anything is drawn.
    let completed = c.view().world().chat.pending_turbine_contexts().is_empty();
    // The screen that was waiting for it is rebuilt before the answer would have been drawn.
    to_gameplay(&mut c);
    let old_is_gone = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell is up")
        .ui
        .node(old_log)
        .is_none()
        && !log_text(&mut c).contains("old generation");

    let second = a_line_in_flight(&mut c, "fresh generation", 102);
    let numbered = second == first + 1;
    deliver_room_packet(&mut c, &mut session, room_answer(second, 1));
    c.tick(1);
    let shown = log_text(&mut c);
    let told = shown.contains("Failed to send text: [fresh generation] to room 12345678.");
    // A second answer about the same line says nothing more.
    deliver_room_packet(&mut c, &mut session, room_answer(second, 1));
    c.tick(1);
    let quiet_the_second_time = log_text(&mut c) == shown;

    // The same boundary for a line somebody else spoke, not only for an answer.
    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "Old", "queued", false),
    );
    to_gameplay(&mut c);
    let not_revived = !log_text(&mut c).contains("queued");
    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "Fresh", "new text", false),
    );
    c.tick(1);
    let new_is_drawn = log_text(&mut c).contains("[General] Fresh says, \"new text\"");

    c.assert_behaviour(
        "chat.turbine.an-answer-that-arrives-after-the-screen-has-gone-completes-quietly",
        move |_| {
            completed
                && old_is_gone
                && numbered
                && told
                && quiet_the_second_time
                && not_revived
                && new_is_drawn
        },
    );
    c.shutdown();
}

/// A line still in flight when the character leaves is finished with, and does not follow him back
/// into the next character's window.
pub fn a_line_in_flight_does_not_follow_the_character_out() {
    // First, with no interface at all: the line is still finished with.
    let mut headless = a_client_in_a_room(false);
    let mut session = Session::new(MockTransport::new());
    let context = a_line_in_flight(&mut headless, "no interface", 100);
    deliver_room_packet(&mut headless, &mut session, room_answer(context, 1));
    let finished_without_a_screen = headless
        .view()
        .world()
        .chat
        .pending_turbine_contexts()
        .is_empty();
    headless.tick(1);
    let had_none = headless.view().expect_app().ui().is_none();
    headless.shutdown();

    let mut c = a_client_in_a_room(true);
    c.world_mut().player = Some(ME);
    let old = a_line_in_flight(&mut c, "old character", 100);
    c.when(Inbound::event(SessionEvent::LoggedOff));
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::CHARACTER_MANAGEMENT);
    c.tick(1);
    // The character has gone and the line has not: it is still waiting to be answered.
    let still_waiting = c.view().world().player.is_none()
        && c.view().world().chat.pending_turbine_contexts() == [old];
    deliver_room_packet(&mut c, &mut session, room_answer(old, 1));
    let answered = c.view().world().chat.pending_turbine_contexts().is_empty();
    c.tick(1);

    // A new character, and the old line does not appear in his window.
    c.world_mut().player = Some(ObjectId(0x5000_0099));
    c.when(Inbound::event(SessionEvent::PlayerDescription(
        Box::default(),
    )));
    to_gameplay(&mut c);
    let not_carried_over = !log_text(&mut c).contains("old character");

    let new = a_line_in_flight(&mut c, "new character", 102);
    let numbered = new == old + 1;
    deliver_room_packet(&mut c, &mut session, room_answer(new, 1));
    c.tick(1);
    let his_own_is_drawn =
        log_text(&mut c).contains("Failed to send text: [new character] to room 12345678.");

    c.assert_behaviour(
        "chat.turbine.a-line-in-flight-does-not-follow-the-character-out",
        move |_| {
            finished_without_a_screen
                && had_none
                && still_waiting
                && answered
                && not_carried_over
                && numbered
                && his_own_is_drawn
        },
    );
    c.shutdown();
}

/// The room's lines and the client's own notices are one queue, in arrival order -- and a packet
/// that came in on the wrong queue is not a chat-room line at all.
pub fn room_lines_and_ordinary_notices_share_one_ordered_queue() {
    let mut c = a_client_in_a_room(true);
    let mut session = Session::new(MockTransport::new());

    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "First", "one", false),
    );
    c.world_mut()
        .scroll
        .add_text_to_scroll("middle ordinary notice", 0, true, 0);
    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "Last", "three", false),
    );
    c.tick(1);
    let shown = log_text(&mut c);
    let ordered = {
        let first = shown.find("[General] First").expect("the first line");
        let middle = shown.find("middle ordinary notice").expect("the notice");
        let last = shown.find("[General] Last").expect("the last line");
        first < middle && middle < last
    };

    // The same bytes on the ordinary message queue: the session does not call it a chat-room line,
    // and nothing is drawn.
    session.transport.deliver(IncomingMessage {
        opcode: 0xF7DE,
        body: room_event(GENERAL_ROOM, "Wrong", "must not display", false),
        queue: NetQueue::UiQueue,
        sender: RecipientId(0),
        blob_id: NetBlobId(2),
    });
    session.tick(LocalTime(0.0));
    let events: Vec<SessionEvent> = session.drain_events().collect();
    let not_chat = !events
        .iter()
        .any(|e| matches!(e, SessionEvent::TurbineChat(_)));
    for e in events {
        c.when(Inbound::event(e));
    }
    c.tick(1);
    let nothing_drawn = log_text(&mut c) == shown;

    c.assert_behaviour(
        "chat.turbine.the-rooms-lines-and-the-ordinary-notices-share-one-queue",
        move |_| ordered && not_chat && nothing_drawn,
    );
    c.shutdown();
}
