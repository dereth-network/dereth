//! Chat: what the player reads in the windows, and what decides whether a line is drawn at all.
//!
//! Scenarios drive a model-only client with messages delivered through `Inbound` and lines typed
//! through `Player`; the tell and spoken-line scenarios also read the recorded corpus as their
//! oracle. No data file is opened.
//!
//! Several scenarios build more than one client. A client is a *run*, and a claim of the form "this
//! happens and that does not" needs two of them; the values each run produced are captured and the
//! single [`HeadlessClient::assert_behaviour`] at the end reads them, as the earshot scenario does.
//!
//! [`HeadlessClient::assert_behaviour`]: dereth_testkit::HeadlessClient::assert_behaviour

use dereth_client::hud::ViewerFrame;
use dereth_client_contract::{PlayerOption, UiRequest};
use dereth_client_model::chat::{text_type, SquelchEntry, TalkFocus};
use dereth_client_model::Request;
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, ObjectId, Position, Quat, Vec3};
use dereth_protocol::archive::PackedHash;
use dereth_protocol::comms::{
    self, ChatRoomMembership, CommunicationChannelBroadcastRecv, CommunicationHearEmote,
    CommunicationHearSpeech, CommunicationSetSquelchDb, SquelchDb as WireDb, SquelchInfo, VLong,
};
use dereth_protocol::login::{LoginCharacterSet, LoginPlayerDescription};
use dereth_protocol::types::PublicWeenieDesc;
use dereth_testkit::{HeadlessClient, Inbound, Player};
use dereth_ui_screens::chat::mainchat::MainChatPanel;
use dereth_ui_screens::panels::inventory::HERITAGE_GROUP_PROPERTY;

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "turbine_text_conversion_runs_before_the_markup",
        &["chat.turbine.host-text-conversion-happens-before-the-markup-is-read"],
        turbine_text_conversion_runs_before_the_markup,
    ),
    (
        "turbine_channel_comes_up_with_the_shards_permission",
        &["chat.turbine.the-channel-comes-up-with-the-shards-permission"],
        turbine_channel_comes_up_with_the_shards_permission,
    ),
    (
        "turbine_line_is_refused_by_three_separate_gates",
        &["chat.turbine.a-line-is-refused-by-the-option-the-bucket-and-the-safety-check"],
        turbine_line_is_refused_by_three_separate_gates,
    ),
    (
        "allegiance_chat_refusal_and_rejoin",
        &["chat.allegiance.refusal-is-its-own-and-the-option-rejoins-the-channel"],
        allegiance_chat_refusal_and_rejoin,
    ),
    (
        "channel_broadcast_wording_per_channel",
        &["chat.channel.each-channel-has-its-own-wording-and-type"],
        channel_broadcast_wording_per_channel,
    ),
    (
        "squelch_table_off_the_wire_replaces_the_clients_own",
        &["chat.squelch.the-shards-table-replaces-the-clients-own"],
        squelch_table_off_the_wire_replaces_the_clients_own,
    ),
    (
        "squelch_is_asked_on_every_incoming_path",
        &["chat.squelch.every-incoming-path-asks-on-the-type-it-carries"],
        squelch_is_asked_on_every_incoming_path,
    ),
    (
        "a_third_partys_death_is_announced",
        &["chat.death.a-third-partys-death-is-announced-and-your-own-is-not"],
        a_third_partys_death_is_announced,
    ),
    (
        "an_unplaceable_speaker_is_always_heard",
        &["chat.speech.a-speaker-the-client-cannot-place-is-always-heard"],
        an_unplaceable_speaker_is_always_heard,
    ),
    (
        "your_own_soul_emote_is_not_echoed_back",
        &["chat.soul-emote.your-own-is-not-echoed-back"],
        your_own_soul_emote_is_not_echoed_back,
    ),
    (
        "a_ranged_line_is_gated_by_its_own_range",
        &["chat.ranged-speech.is-gated-by-the-range-the-message-carries"],
        a_ranged_line_is_gated_by_its_own_range,
    ),
    (
        "every_talk_focus_row_is_told_every_time",
        &["chat.talk-focus.every-row-is-told-every-time-and-the-menu-guard-does-not-unset-it"],
        every_talk_focus_row_is_told_every_time,
    ),
    (
        "an_incoming_room_packet_must_be_whole",
        &["chat.turbine.an-incoming-line-is-refused-unless-the-packet-is-whole"],
        an_incoming_room_packet_must_be_whole,
    ),
    (
        "each_chat_room_is_drawn_with_its_own_name",
        &["chat.turbine.each-room-is-drawn-with-its-own-name-and-type"],
        each_chat_room_is_drawn_with_its_own_name,
    ),
    (
        "a_failed_room_line_is_named_back_once",
        &["chat.turbine.a-failure-answer-names-the-line-and-a-relog-keeps-the-service"],
        a_failed_room_line_is_named_back_once,
    ),
    (
        "a_speaker_you_cannot_understand_is_a_name_and_a_noise",
        &["chat.garble.a-speaker-you-cannot-understand-is-drawn-as-a-name-and-a-noise"],
        a_speaker_you_cannot_understand_is_a_name_and_a_noise,
    ),
    (
        "a_garbled_tell_is_drawn_even_when_it_was_not_for_you",
        &["chat.garble.a-tell-you-cannot-understand-is-drawn-even-when-it-was-not-for-you"],
        a_garbled_tell_is_drawn_even_when_it_was_not_for_you,
    ),
    (
        "an_acted_emote_garbles_where_a_pose_and_a_shout_do_not",
        &["chat.garble.an-acted-emote-is-garbled-where-a-pose-and-a-shout-are-not"],
        an_acted_emote_garbles_where_a_pose_and_a_shout_do_not,
    ),
    (
        "every_spelling_of_the_emote_command_sends_the_same_thing",
        &["chat.emote.every-spelling-of-the-command-sends-the-same-thing"],
        every_spelling_of_the_emote_command_sends_the_same_thing,
    ),
    (
        "an_empty_emote_is_silent_and_costs_no_place_in_the_order",
        &["chat.emote.an-empty-one-is-silent-and-costs-no-place-in-the-order"],
        an_empty_emote_is_silent_and_costs_no_place_in_the_order,
    ),
    (
        "a_shard_line_still_carries_its_newline_at_the_model",
        &["chat.log.a-shard-line-still-carries-its-newline-at-the-model-boundary"],
        a_shard_line_still_carries_its_newline_at_the_model,
    ),
    (
        "a_composed_room_line_still_carries_its_markup",
        &["chat.tell-markup.a-composed-room-line-still-carries-its-markup-at-the-model-boundary"],
        a_composed_room_line_still_carries_its_markup,
    ),
    (
        "a_populated_squelch_list_is_read_whole",
        &["chat.squelch.a-populated-list-is-read-whole-and-keeps-the-kind-of-each-entry"],
        a_populated_squelch_list_is_read_whole,
    ),
    (
        "the_squelch_row_is_a_toggle_and_names_the_speaker",
        &["chat.talk-to-menu.the-squelch-row-is-a-toggle-and-its-message-names-the-speaker"],
        the_squelch_row_is_a_toggle_and_names_the_speaker,
    ),
    (
        "the_chat_target_follows_what_is_selected_while_it_is_near",
        &["chat.talk-to-menu.the-chat-target-follows-what-is-selected-while-it-is-near"],
        the_chat_target_follows_what_is_selected_while_it_is_near,
    ),
    (
        "a_tell_to_the_chat_target_goes_to_it_and_not_to_the_selection",
        &["chat.talk-to-menu.a-tell-goes-to-the-chat-target-and-not-to-the-selection"],
        a_tell_to_the_chat_target_goes_to_it_and_not_to_the_selection,
    ),
    (
        "the_channel_rows_follow_the_service_and_the_options",
        &["chat.talk-focus.the-channel-rows-follow-the-service-and-the-players-own-options"],
        the_channel_rows_follow_the_service_and_the_options,
    ),
    (
        "every_recorded_login_asks_for_the_room_service",
        &["chat.turbine.every-recorded-login-asks-the-client-to-use-the-service"],
        every_recorded_login_asks_for_the_room_service,
    ),
    (
        "the_newlines_are_trimmed_from_both_ends",
        &["chat.log.the-newlines-are-trimmed-from-both-ends-and-nothing-else-is"],
        the_newlines_are_trimmed_from_both_ends,
    ),
    (
        "a_spoken_line_is_the_name_the_verb_and_the_words",
        &["chat.speech.a-spoken-line-is-the-name-the-verb-and-the-words-in-quotes"],
        a_spoken_line_is_the_name_the_verb_and_the_words,
    ),
    (
        "a_private_message_is_drawn_only_when_it_was_for_you",
        &["chat.tell.a-private-message-is-drawn-only-when-it-was-addressed-to-you"],
        a_private_message_is_drawn_only_when_it_was_for_you,
    ),
    (
        "a_line_with_a_range_on_it_has_no_echo_of_your_own",
        &["chat.speech.a-line-with-a-range-on-it-has-no-echo-of-your-own"],
        a_line_with_a_range_on_it_has_no_echo_of_your_own,
    ),
    (
        "only_a_player_has_a_clickable_name",
        &["chat.speech.only-a-player-has-a-clickable-name-and-the-range-is-exclusive-at-both-ends"],
        only_a_player_has_a_clickable_name,
    ),
    (
        "every_recorded_spoken_line_is_drawn_with_the_verb",
        &["chat.speech.every-recorded-spoken-line-is-drawn-with-the-clients-own-verb-and-quotes"],
        every_recorded_spoken_line_is_drawn_with_the_verb,
    ),
    (
        "every_recorded_spoken_line_is_rebuilt_byte_for_byte",
        &["chat.speech.every-spoken-line-the-recorded-client-sent-is-rebuilt-byte-for-byte"],
        every_recorded_spoken_line_is_rebuilt_byte_for_byte,
    ),
    (
        "every_recorded_tell_is_rebuilt_from_a_typed_line",
        &["chat.tell.every-recorded-private-message-is-rebuilt-from-the-same-typed-line"],
        every_recorded_tell_is_rebuilt_from_a_typed_line,
    ),
    (
        "a_typed_tell_goes_out_with_the_words_first",
        &["chat.tell.a-typed-private-message-goes-out-with-the-words-first-and-the-name-after"],
        a_typed_tell_goes_out_with_the_words_first,
    ),
    (
        "every_way_of_writing_a_private_message_reaches_the_wire",
        &["chat.tell.every-way-of-writing-a-private-message-reaches-the-wire"],
        every_way_of_writing_a_private_message_reaches_the_wire,
    ),
    (
        "the_name_on_the_wire_is_the_one_the_line_asked_for",
        &["chat.tell.the-name-on-the-wire-is-the-one-the-line-asked-for-and-no-other"],
        the_name_on_the_wire_is_the_one_the_line_asked_for,
    ),
    (
        "a_reply_goes_to_whoever_last_wrote_to_you",
        &["chat.tell.a-reply-goes-to-whoever-last-wrote-to-you-and-nowhere-when-nobody-has"],
        a_reply_goes_to_whoever_last_wrote_to_you,
    ),
    (
        "the_mark_in_front_of_a_name_is_taken_off_before_it_goes_out",
        &["chat.tell.the-mark-a-shard-puts-in-front-of-a-name-is-taken-off-before-it-goes-out"],
        the_mark_in_front_of_a_name_is_taken_off_before_it_goes_out,
    ),
    (
        "every_refusal_is_the_clients_own_words",
        &["chat.tell.every-refusal-is-the-clients-own-words"],
        every_refusal_is_the_clients_own_words,
    ),
    (
        "a_verb_the_client_knows_is_not_refused",
        &["chat.commands.a-verb-the-client-knows-answers-it-and-a-word-it-does-not-know-is-passed-on"],
        a_verb_the_client_knows_is_not_refused,
    ),
    (
        "neither_safety_gate_would_have_stopped_a_recorded_line",
        &["chat.speech.neither-safety-gate-would-have-stopped-a-line-the-recordings-carry"],
        neither_safety_gate_would_have_stopped_a_recorded_line,
    ),
    (
        "emote_is_drawn_as_name_then_text",
        &["chat.emote.is-drawn-as-name-then-text"],
        emote_is_drawn_as_name_then_text,
    ),
    (
        "speech_earshot_and_squelch_both_gate",
        &["chat.speech.earshot-and-squelch-both-gate"],
        speech_earshot_and_squelch_both_gate,
    ),
    (
        "talk_focus_has_one_authoritative_value",
        &["chat.talk-focus.one-authoritative-value"],
        talk_focus_has_one_authoritative_value,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

const ME: ObjectId = ObjectId(0x5000_0001);
const SPEAKER: ObjectId = ObjectId(0x5000_0AAA);

/// A heading no axis is aligned with, so a dropped term in a distance cannot cancel.
const HEADING_DEGREES: f32 = 41.7;
/// An outdoor landcell, where the hearing radius is the larger of the two.
const OUTDOOR_CELL: u32 = 0xA9B4_0001;

// =============================================================================================
// 1. chat.turbine.host-text-conversion-happens-before-the-markup-is-read
// =============================================================================================

/// One `RoomEvent` datagram carrying `text`, in the shape the chat-room transport delivers.
fn room_event(text: &str) -> Vec<u8> {
    let mut body = 123_u32.to_le_bytes().to_vec();
    for s in ["Sender", text] {
        let units: Vec<u16> = s.encode_utf16().collect();
        assert!(units.len() < 128, "the length prefix is one byte");
        body.push(u8::try_from(units.len()).expect("checked above"));
        body.extend(units.into_iter().flat_map(u16::to_le_bytes));
    }
    body.extend(
        [12_u32, 0x5000_0017, 0, 2]
            .into_iter()
            .flat_map(u32::to_le_bytes),
    );
    let mut packet = (u32::try_from(body.len()).expect("small") + 32)
        .to_le_bytes()
        .to_vec();
    packet.extend(
        [
            1_u32,
            1,
            1,
            0,
            0,
            0,
            0,
            u32::try_from(body.len()).expect("small"),
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes),
    );
    packet.extend(body);
    packet
}

/// A client whose chat-room system is up with one general room.
fn in_a_chat_room() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    let w = c.world_mut();
    // The text arm is the **client's**, installed by `ObjectStream::new`, so the conversion below
    // is the host's own and not the workspace's fallback table.
    w.chat.startup_turbine_chat();
    w.chat.recv_chat_room_tracker(ChatRoomMembership {
        general_room: 123,
        ..ChatRoomMembership::default()
    });
    c
}

/// What one client draws for the forged, the plain and the partly spellable line, in that order.
struct DrawnRoomLines {
    forged_drawn: bool,
    plain_drawn: bool,
    partial_line: Option<String>,
}

/// Deliver the three room lines to `c` and read back what it drew.
fn draw_room_lines(c: &mut HeadlessClient) -> DrawnRoomLines {
    let w = c.world_mut();
    assert!(
        w.recv_turbine_chat(&room_event("＜ＴＥＬＬ：Name>")),
        "the datagram is accepted"
    );
    let forged_drawn = !w.scroll.drain().is_empty();
    assert!(w.recv_turbine_chat(&room_event("ordinary")));
    let plain_drawn = w.scroll.drain().iter().any(|l| l.body.contains("ordinary"));
    assert!(w.recv_turbine_chat(&room_event("before\u{1F642}after")));
    let partial_line = w.scroll.drain().pop().map(|l| l.body);
    DrawnRoomLines {
        forged_drawn,
        plain_drawn,
        partial_line,
    }
}

/// A chat-room line is narrowed to the host's own spelling before its markup is read.
///
/// The spelling is a western Windows install's, code page 1252 with its best-fit mappings, on every
/// platform. The scenario runs twice: once with the conversion the client installs (the host's NLS
/// on a Windows build, the table elsewhere) and once with the portable 1252 table installed in its
/// place, so a Windows run also checks the arm every other platform draws with.
pub fn turbine_text_conversion_runs_before_the_markup() {
    let partial = room_event("before\u{1F642}after");

    let mut installed = in_a_chat_room();
    let installed_lines = draw_room_lines(&mut installed);

    let mut c = in_a_chat_room();
    {
        let table = dereth_client_model::HostText::new(std::sync::Arc::new(
            dereth_primitives::text::cp1252::Cp1252,
        ));
        let w = c.world_mut();
        w.chat.text_conversion = table.clone();
        w.scroll.encoding = table;
    }
    let table_lines = draw_room_lines(&mut c);

    // The wire text itself is untouched: the conversion is for drawing, not for storage.
    let decoded =
        dereth_protocol::turbine::decode_incoming(&partial).expect("the datagram decodes");
    let wire_kept = match decoded.payload {
        dereth_protocol::turbine::IncomingPayload::RoomEvent(e) => e.text == "before\u{1F642}after",
        _ => false,
    };
    // And the narrow spelling is reversible for every byte, which is what stops the conversion
    // losing a character it could have kept.
    let all: Vec<u8> = (0_u8..=255).collect();
    let bijective = dereth_protocol::cp1252::encode(&dereth_protocol::cp1252::decode(&all))
        .is_some_and(|back| back == all)
        && (0_u8..=255).all(|b| {
            dereth_protocol::cp1252::encode(&dereth_protocol::cp1252::decode(&[b]))
                .is_some_and(|back| back == [b])
        });

    c.assert_behaviour(
        "chat.turbine.host-text-conversion-happens-before-the-markup-is-read",
        move |_| {
            [installed_lines, table_lines].iter().all(|lines| {
                !lines.forged_drawn
                    && lines.plain_drawn
                    && lines.partial_line.as_deref()
                        == Some(
                            "[General] <Tell:IIDString:0:Sender>Sender<\\Tell> says, \"before\"",
                        )
            }) && wire_kept
                && bijective
        },
    );
}

// =============================================================================================
// 2. chat.turbine.the-channel-comes-up-with-the-shards-permission
// =============================================================================================

/// The character list is what starts the chat-room system, and a later "no" does not stop it.
pub fn turbine_channel_comes_up_with_the_shards_permission() {
    let mut description = LoginPlayerDescription::default();
    description.player_module.options2 |= 0x100;
    let no = SessionEvent::CharacterSet(Box::default());
    let yes = SessionEvent::CharacterSet(Box::new(LoginCharacterSet {
        use_turbine_chat: 1,
        ..LoginCharacterSet::default()
    }));
    let desc = SessionEvent::PlayerDescription(Box::new(description));

    let mut c = HeadlessClient::model();
    c.when(Inbound::event(no.clone()))
        .when(Inbound::event(desc.clone()));
    let refused = !c.view().world().chat.using_turbine_chat
        && !c
            .view()
            .world()
            .chat
            .is_talk_focus_enabled(TalkFocus::General);

    c.when(Inbound::event(yes))
        .when(Inbound::event(desc.clone()));
    let started = c.view().world().chat.using_turbine_chat
        && c.view()
            .world()
            .chat
            .is_talk_focus_enabled(TalkFocus::General);

    c.when(Inbound::event(no)).when(Inbound::event(desc));
    let not_shut_down = c.view().world().chat.using_turbine_chat
        && c.view()
            .world()
            .chat
            .is_talk_focus_enabled(TalkFocus::General);

    // The rooms the shard names replace whatever was held, zeros and all, and a channel's row is
    // selectable exactly when it has a room.
    let mut rooms = HeadlessClient::model();
    {
        let w = rooms.world_mut();
        w.chat.startup_turbine_chat();
        w.chat.recv_chat_room_tracker(ChatRoomMembership {
            allegiance_room: 101,
            general_room: 102,
            ..ChatRoomMembership::default()
        });
    }
    let allegiance_up = rooms
        .view()
        .world()
        .chat
        .is_talk_focus_enabled(TalkFocus::Allegiance);
    rooms
        .world_mut()
        .chat
        .recv_chat_room_tracker(ChatRoomMembership::default());
    let zeroed = rooms
        .view()
        .world()
        .chat
        .chat_rooms
        .values()
        .all(|id| *id == 0)
        && !rooms
            .view()
            .world()
            .chat
            .is_talk_focus_enabled(TalkFocus::Allegiance);

    c.assert_behaviour(
        "chat.turbine.the-channel-comes-up-with-the-shards-permission",
        move |_| refused && started && not_shut_down && allegiance_up && zeroed,
    );
}

// =============================================================================================
// 3. chat.turbine.a-line-is-refused-by-the-option-the-bucket-and-the-safety-check
// =============================================================================================

/// Send one line at the general room at simulated second `at`, and report whether it travelled.
fn say_to_the_room(
    c: &mut HeadlessClient,
    out: &mut dereth_client_model::RecordingRequests,
    text: &str,
    at: i32,
) -> (bool, Vec<String>) {
    let w = c.world_mut();
    w.scroll.clear();
    let ok = w.send_turbine_chat(out, TalkFocus::General, false, text, 4, at);
    let lines = w.scroll.pending().iter().map(|e| e.body.clone()).collect();
    (ok, lines)
}

/// A line is refused by the listening option, by the spam bucket and by the safety check, and a
/// refused line does not cost the player the next one.
pub fn turbine_line_is_refused_by_three_separate_gates() {
    let mut c = HeadlessClient::model();
    {
        let w = c.world_mut();
        w.player = Some(ObjectId(0x5000_0017));
        for option in [27, 35, 36, 37, 38, 46] {
            w.player_system.options.set(option, true);
        }
    }

    let mut out = dereth_client_model::RecordingRequests::default();

    // Before the system is up at all.
    let (unavailable, unavailable_lines) = say_to_the_room(&mut c, &mut out, "unavailable", 100);

    {
        let w = c.world_mut();
        w.chat.startup_turbine_chat();
        w.chat.recv_chat_room_tracker(ChatRoomMembership {
            general_room: 222,
            ..ChatRoomMembership::default()
        });
        // Not listening to the channel.
        w.player_system.options.set(27, false);
    }
    let (not_listening, not_listening_lines) =
        say_to_the_room(&mut c, &mut out, "not listening", 100);
    c.world_mut().player_system.options.set(27, true);

    // The safety check: nothing at all, a forged tell marker, a second line.
    let mut safety_refused = true;
    let mut safety_said_anything = false;
    for bad in ["", "safe <TeLl:bad> payload", "two\nlines"] {
        let (ok, lines) = say_to_the_room(&mut c, &mut out, bad, 100);
        safety_refused &= !ok;
        safety_said_anything |= !lines.is_empty();
    }

    // …and the line after them still travels, so a refused line did not spend the bucket.
    let (first, _) = say_to_the_room(&mut c, &mut out, "first", 100);
    let (same_second, same_second_lines) = say_to_the_room(&mut c, &mut out, "same second", 100);
    let (one_later, _) = say_to_the_room(&mut c, &mut out, "one second later", 101);
    let (recovered, _) = say_to_the_room(&mut c, &mut out, "recovered", 103);

    let sent: Vec<u32> = out
        .0
        .iter()
        .filter_map(|r| match r {
            Request::TurbineChat(m) => Some(m.room),
            _ => None,
        })
        .collect();

    c.assert_behaviour(
        "chat.turbine.a-line-is-refused-by-the-option-the-bucket-and-the-safety-check",
        move |_| {
            let before_startup = !unavailable
                && unavailable_lines
                    .iter()
                    .any(|l| l == "Turbine chat is not available.");
            // The refusal a player can fix is reported as delivered, because retail's handler
            // returns true after printing it -- the line the player reads is the answer.
            let option = not_listening
                && not_listening_lines
                    .iter()
                    .any(|l| l == "You are not listening to the Allegiance channel!");
            let safety = safety_refused && !safety_said_anything;
            let bucket = first
                && !same_second
                && same_second_lines
                    .iter()
                    .any(|l| l.starts_with("You must wait"))
                && !one_later
                && recovered;
            before_startup && option && safety && bucket && sent == [222, 222]
        },
    );
}

// =============================================================================================
// 4. chat.allegiance.refusal-is-its-own-and-the-option-rejoins-the-channel
// =============================================================================================

/// The forty bytes a recorded shard sent the retail client when it joined the allegiance
/// channel: ten room ids, the allegiance one first.
const SHARD_CHAT_ROOMS: [u8; 40] = [
    0xA2, 0x07, 0x00, 0x80, // allegiance
    0x02, 0x00, 0x00, 0x00, // general
    0x03, 0x00, 0x00, 0x00, // trade
    0x04, 0x00, 0x00, 0x00, // lfg
    0x05, 0x00, 0x00, 0x00, // roleplay
    0x0A, 0x00, 0x00, 0x00, // olthoi -- sixth
    0x00, 0x00, 0x00, 0x00, // society -- seventh, zero for a player with no faction
    0x07, 0x00, 0x00, 0x00, //
    0x08, 0x00, 0x00, 0x00, //
    0x09, 0x00, 0x00, 0x00, //
];
const SHARD_ALLEGIANCE_ROOM: u32 = 0x8000_07A2;
const NOT_IN_AN_ALLEGIANCE: &str = "You are not in an allegiance!";
const NOT_AVAILABLE: &str = "Turbine chat is not available.";

/// One `@a hello` through the production command interpreter; the scroll lines it produced.
fn allegiance_command(c: &mut HeadlessClient) -> Vec<String> {
    c.world_mut().scroll.clear();
    c.when(Player::ui(UiRequest::ChatLine {
        text: "@a hello".into(),
        window: 4,
    }));
    c.view()
        .world()
        .scroll
        .pending()
        .iter()
        .map(|e| e.body.clone())
        .collect()
}

/// The allegiance command has its own refusal, and the listening option is the one lever that
/// rejoins the channel.
pub fn allegiance_chat_refusal_and_rejoin() {
    let mut c = HeadlessClient::model();
    {
        // The command table the client registers at login; without it `@a` is not a command.
        c.interaction_mut().startup_turbine_chat_commands();
        // The chat commands are a logged-in client's, which is what the HUD's flag says.
        c.hud_mut().player_desc_received = true;
        let w = c.world_mut();
        w.player = Some(ObjectId(0x5000_001F));
        w.chat.startup_turbine_chat();
        w.player_system.options.set(27, true);
        let mut without = SHARD_CHAT_ROOMS;
        without[0..4].copy_from_slice(&0_u32.to_le_bytes());
        w.chat.recv_chat_room_tracker(
            dereth_protocol::read_body::<ChatRoomMembership>(&without).expect("ten dwords decode"),
        );
    }

    let before = allegiance_command(&mut c);
    let refused_in_its_own_words = before.iter().any(|l| l == NOT_IN_AN_ALLEGIANCE)
        && !before.iter().any(|l| l.trim() == NOT_AVAILABLE);
    let nothing_sent = c.outbound().is_empty();

    // The ask: the option off and on again.
    c.when(Player::Ui(vec![
        UiRequest::SetPlayerOption(PlayerOption::HearAllegianceChat, false),
        UiRequest::SetPlayerOption(PlayerOption::HearAllegianceChat, true),
    ]));
    let asked: Vec<Vec<u8>> = c
        .outbound()
        .iter()
        .filter_map(|r| match r {
            Request::PlayerOptionChanged(m) => {
                Some(dereth_protocol::write_body(m).expect("the option change encodes"))
            }
            _ => None,
        })
        .collect();

    // The shard's answer, verbatim.
    c.world_mut().chat.recv_chat_room_tracker(
        dereth_protocol::read_body::<ChatRoomMembership>(&SHARD_CHAT_ROOMS)
            .expect("the forty bytes decode"),
    );
    let row_selectable = c
        .view()
        .world()
        .chat
        .is_talk_focus_enabled(TalkFocus::Allegiance);
    let after = allegiance_command(&mut c);
    let rooms: Vec<u32> = c
        .outbound()
        .iter()
        .filter_map(|r| match r {
            Request::TurbineChat(m) => Some(m.room),
            _ => None,
        })
        .collect();

    c.assert_behaviour(
        "chat.allegiance.refusal-is-its-own-and-the-option-rejoins-the-channel",
        move |_| {
            refused_in_its_own_words
                && nothing_sent
                && asked
                    == vec![
                        vec![0x1B, 0, 0, 0, 0, 0, 0, 0],
                        vec![0x1B, 0, 0, 0, 1, 0, 0, 0],
                    ]
                && row_selectable
                && after.is_empty()
                && rooms == [SHARD_ALLEGIANCE_ROOM]
        },
    );
}

// =============================================================================================
// 5. chat.channel.each-channel-has-its-own-wording-and-type
// =============================================================================================

/// One broadcast through a fresh client; the line it drew and that line's type.
fn broadcast(channel: u32, sender: &str, message: &str) -> Option<(String, u32)> {
    let mut c = HeadlessClient::model();
    c.when(Inbound::message(&CommunicationChannelBroadcastRecv {
        channel,
        sender_name: sender.to_owned(),
        message: message.to_owned(),
    }));
    let lines = c.chat_lines();
    (lines.len() == 1).then(|| (lines[0].body.clone(), u32::from(lines[0].ty)))
}

/// Each channel has its own wording and its own colour, and a squelched channel drops the line.
pub fn channel_broadcast_wording_per_channel() {
    #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
    let cases: Vec<(Option<(String, u32)>, &str, u32)> = vec![
        (
            broadcast(0x1000, "", "hello"),
            "You say to your Vassals, \"hello\"",
            text_type::SOCIAL_SEND,
        ),
        (
            broadcast(0x2000, "", "hi"),
            "You say to your Patron, \"hi\"",
            text_type::SOCIAL_SEND,
        ),
        (
            broadcast(0x4000, "", "hi"),
            "You say to your Monarch, \"hi\"",
            text_type::SOCIAL_SEND,
        ),
        (
            broadcast(0x2000, "Alba", "well met"),
            "Your vassal <Tell:IIDString:0:Alba>Alba<\\Tell> says to you, \"well met\"",
            text_type::SOCIAL,
        ),
        (
            broadcast(0x1000, "Aldis", "orders"),
            "Your patron <Tell:IIDString:0:Aldis>Aldis<\\Tell> says to you, \"orders\"",
            text_type::SOCIAL,
        ),
        (
            broadcast(0x4000, "Plonk", "sire"),
            "Your follower <Tell:IIDString:0:Plonk>Plonk<\\Tell> says to you, \"sire\"",
            text_type::SOCIAL,
        ),
        (
            broadcast(0x800, "", "grp"),
            "[Fellowship] You say, \"grp\"",
            text_type::FELLOWSHIP,
        ),
        (
            broadcast(0x0200_0000, "Dee", "all"),
            "[Allegiance Broadcast] <Tell:IIDString:0:Dee>Dee<\\Tell> says, \"all\"",
            text_type::SOCIAL,
        ),
        (
            broadcast(0x0100_0000, "", "co"),
            "[Co-Vassals] You say, \"co\"",
            text_type::SOCIAL,
        ),
        (
            broadcast(0x200, "", "s"),
            "You say on the Sentinel channel, \"s\"",
            text_type::CHANNEL_SEND,
        ),
        (
            broadcast(0x200, "Fay", "s"),
            "<Tell:IIDString:0:Fay>Fay<\\Tell> says on the Sentinel channel, \"s\"",
            text_type::CHANNEL,
        ),
        (
            broadcast(0x400, "", "h"),
            "You say on the Help channel, \"h\"",
            text_type::HELP,
        ),
        (
            broadcast(1, "", "a"),
            "You say on the Abuse channel, \"a\"",
            text_type::ABUSE,
        ),
        (
            broadcast(0x80, "", "x"),
            "You say on the <unknown> channel, \"x\"",
            text_type::CHANNEL_SEND,
        ),
        // The odd one: no prefix, no quotes, the fellowship colour.
        (
            broadcast(0x0400_0000, "", "Cid has joined the fellowship."),
            "Cid has joined the fellowship.",
            text_type::FELLOWSHIP,
        ),
    ];
    let all_match = cases
        .iter()
        .all(|(got, body, ty)| got.as_ref().is_some_and(|(b, t)| b == body && t == ty));

    // A squelchable type that is off drops the line; a type that is not squelchable is drawn
    // whatever the table says.
    let mut squelched = HeadlessClient::model();
    {
        let chat = &mut squelched.world_mut().chat;
        chat.squelch.global.types.insert(text_type::FELLOWSHIP);
        chat.squelch.global.types.insert(text_type::SOCIAL);
    }
    squelched.when(Inbound::message(&CommunicationChannelBroadcastRecv {
        channel: 0x800,
        sender_name: "Cid".to_owned(),
        message: "grp".to_owned(),
    }));
    let dropped = squelched.chat_lines().is_empty();
    squelched.when(Inbound::message(&CommunicationChannelBroadcastRecv {
        channel: 0x0200_0000,
        sender_name: "Dee".to_owned(),
        message: "all".to_owned(),
    }));
    let kept = squelched.chat_lines().len() == 1;

    squelched.assert_behaviour(
        "chat.channel.each-channel-has-its-own-wording-and-type",
        move |_| all_match && dropped && kept,
    );
}

// =============================================================================================
// 6. chat.squelch.the-shards-table-replaces-the-clients-own
// =============================================================================================

fn squelch_info(types: &[u32], name: &str) -> SquelchInfo {
    let mut limbs = vec![0_u32; 4];
    for t in types {
        limbs[(*t / 32) as usize] |= 1 << (*t % 32);
    }
    SquelchInfo {
        squelch_msgs: VLong(limbs),
        name: name.to_owned(),
        is_zone_squelch: 0,
    }
}

fn packed<K, V>(entries: Vec<(K, V)>) -> PackedHash<K, V> {
    PackedHash {
        table_size: 8,
        entries,
    }
}

fn squelch_db(db: WireDb) -> CommunicationSetSquelchDb {
    CommunicationSetSquelchDb(db)
}

fn a_say(sender: ObjectId, text: &str) -> CommunicationHearSpeech {
    CommunicationHearSpeech {
        message: text.to_owned(),
        sender_name: "Speaker".to_owned(),
        sender_id: sender,
        text_type: text_type::SPEECH,
    }
}

/// A client with a body but no radar entry for the speaker, so every refusal below is the
/// squelch half and cannot be the distance.
fn a_listener() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    c.sync_viewer(Some(ViewerFrame {
        position: Position::new(
            CellId(0x00C3_0001),
            Frame::new(Vec3::new(80.0, 80.0, 0.0), Quat::IDENTITY),
        ),
        heading_degrees: HEADING_DEGREES,
    }));
    c
}

/// The shard's squelch list is the whole list, and a smaller one makes a speaker audible again.
pub fn squelch_table_off_the_wire_replaces_the_clients_own() {
    let mut c = a_listener();
    c.when(Inbound::message(&a_say(SPEAKER, "before")));
    let heard_before = c.chat_lines().len() == 1;

    c.when(Inbound::message(&squelch_db(WireDb {
        account_hash: packed(vec![("someaccount".to_owned(), 1)]),
        character_hash: packed(vec![(
            SPEAKER.0,
            squelch_info(&[text_type::SPEECH], "Speaker"),
        )]),
        global_squelch_info: squelch_info(&[], ""),
    })));
    c.when(Inbound::message(&a_say(SPEAKER, "after")));
    let silenced = c.chat_lines().len() == 1
        && c.view().hud().stats.speech_lines_squelched == 1
        && c.view().hud().stats.speech_lines_out_of_earshot == 0
        && c.view().hud().stats.squelch_rows_applied == 2;

    // The player un-squelches them: the shard re-sends the whole, now empty, list.
    c.when(Inbound::message(&squelch_db(WireDb {
        account_hash: packed(vec![]),
        character_hash: packed(vec![]),
        global_squelch_info: squelch_info(&[], ""),
    })));
    c.when(Inbound::message(&a_say(SPEAKER, "again")));
    let audible_again = c.chat_lines().len() == 2
        && c.view().world().chat.squelch.characters.is_empty()
        && c.view().world().chat.squelch.accounts.is_empty();

    // The list-wide entry is a different member and is read first: it silences a speaker with no
    // row of their own at all.
    let mut global = a_listener();
    global.when(Inbound::message(&squelch_db(WireDb {
        account_hash: packed(vec![]),
        character_hash: packed(vec![]),
        global_squelch_info: squelch_info(&[text_type::SPEECH], ""),
    })));
    global.when(Inbound::message(&a_say(SPEAKER, "filtered")));
    let global_silences =
        global.chat_lines().is_empty() && global.view().world().chat.squelch.characters.is_empty();

    // The bit the wire sets is the kind of text the router asks about, and only that one.
    let mut per_type = a_listener();
    per_type.when(Inbound::message(&squelch_db(WireDb {
        account_hash: packed(vec![]),
        character_hash: packed(vec![(
            SPEAKER.0,
            squelch_info(&[text_type::EMOTE], "Speaker"),
        )]),
        global_squelch_info: squelch_info(&[], ""),
    })));
    per_type.when(Inbound::message(&a_say(SPEAKER, "speech survives")));
    let only_that_bit = per_type.chat_lines().len() == 1
        && per_type
            .view()
            .world()
            .chat
            .squelch
            .characters
            .get(&SPEAKER)
            .is_some_and(|e| {
                e.is_squelched(text_type::EMOTE) && !e.is_squelched(text_type::SPEECH)
            });

    // A repeated name keeps the first entry, the way the shard's own table does.
    let first_wins = {
        let db = dereth_client_model::chat::SquelchDb::from_wire(&WireDb {
            account_hash: packed(vec![("dup".to_owned(), 1), ("dup".to_owned(), 2)]),
            character_hash: packed(vec![
                (SPEAKER.0, squelch_info(&[text_type::SPEECH], "first")),
                (SPEAKER.0, squelch_info(&[text_type::EMOTE], "second")),
            ]),
            global_squelch_info: squelch_info(&[], ""),
        });
        db.characters
            .get(&SPEAKER)
            .is_some_and(|e| e.name == "first")
    };

    // A truncated list leaves the table exactly as it was.
    let mut truncated = a_listener();
    truncated.when(Inbound::message(&squelch_db(WireDb {
        account_hash: packed(vec![]),
        character_hash: packed(vec![(
            SPEAKER.0,
            squelch_info(&[text_type::SPEECH], "Speaker"),
        )]),
        global_squelch_info: squelch_info(&[], ""),
    })));
    let short = {
        let mut blob = dereth_protocol::write_blob(&squelch_db(WireDb {
            account_hash: packed(vec![]),
            character_hash: packed(vec![]),
            global_squelch_info: squelch_info(&[text_type::SPEECH], ""),
        }))
        .expect("the message encodes");
        blob.truncate(6);
        SessionEvent::UiEvent {
            opcode: <CommunicationSetSquelchDb as dereth_protocol::Message>::OPCODE,
            blob,
        }
    };
    truncated.when(Inbound::event(short));
    let survived_the_truncation = truncated
        .view()
        .world()
        .chat
        .squelch
        .characters
        .get(&SPEAKER)
        .is_some_and(|e| e.is_squelched(text_type::SPEECH))
        && truncated.view().hud().stats.undecodable == 1;

    c.assert_behaviour(
        "chat.squelch.the-shards-table-replaces-the-clients-own",
        move |_| {
            heard_before
                && silenced
                && audible_again
                && global_silences
                && only_that_bit
                && first_wins
                && survived_the_truncation
        },
    );
}

// =============================================================================================
// 7. chat.squelch.every-incoming-path-asks-on-the-type-it-carries
// =============================================================================================

/// Which table a run installs.
#[derive(Clone, Copy)]
enum Table {
    Empty,
    /// One character, squelched on one kind of text.
    Character(ObjectId, u32),
    /// The list-wide entry, one kind of text.
    Global(u32),
}

/// Drive one message through a client holding `table`, and report the lines it drew.
fn heard_with(table: Table, event: SessionEvent) -> (usize, dereth_client::hud::HudStats) {
    let mut c = HeadlessClient::model();
    c.hud_mut().player = Some(ME);
    // `player_body` is the gate the three speech handlers open with; set here rather than by a
    // viewer sync so that no speaker has a radar entry and the distance can never be the reason.
    c.hud_mut().player_body = true;
    match table {
        Table::Empty => {}
        Table::Character(id, ty) => {
            let mut e = SquelchEntry {
                name: "Them".to_owned(),
                ..SquelchEntry::default()
            };
            e.types.insert(ty);
            c.world_mut().chat.squelch.characters.insert(id, e);
        }
        Table::Global(ty) => {
            c.world_mut().chat.squelch.global.types.insert(ty);
        }
    }
    c.when(Inbound::event(event));
    (c.chat_lines().len(), c.view().hud().stats)
}

fn say_event(sender: ObjectId, ty: u32) -> SessionEvent {
    ui_event(&comms::CommunicationHearSpeech {
        message: "p2".to_owned(),
        sender_name: "Them".to_owned(),
        sender_id: sender,
        text_type: ty,
    })
}

fn ui_event<M: dereth_protocol::Message>(m: &M) -> SessionEvent {
    SessionEvent::UiEvent {
        opcode: M::OPCODE,
        blob: dereth_protocol::write_blob(m).expect("the message encodes"),
    }
}

/// Every incoming path asks about the kind of text it carries, and only that kind.
pub fn squelch_is_asked_on_every_incoming_path() {
    const THEM: ObjectId = ObjectId(0x5000_0002);

    // A say, gated on the type on the wire and on no other.
    let (say_open, _) = heard_with(Table::Empty, say_event(THEM, text_type::SPEECH));
    let (say_shut, _) = heard_with(
        Table::Character(THEM, text_type::SPEECH),
        say_event(THEM, text_type::SPEECH),
    );
    let (say_other_type, _) = heard_with(
        Table::Character(THEM, text_type::EMOTE),
        say_event(THEM, text_type::SPEECH),
    );

    // Your own say is composed above the gate and no table can hide it.
    let (own_say, _) = heard_with(
        Table::Character(ME, text_type::SPEECH),
        say_event(ME, text_type::SPEECH),
    );

    // A ranged say is refused **before** its own range test. The speaker here has no body at
    // all, so the range refuses it either way and the silence is identical; what tells the two
    // apart is which counter moved. (The range itself is the subject of its own scenario.)
    let ranged = |sender| {
        ui_event(&comms::CommunicationHearRangedSpeech {
            message: "p2".to_owned(),
            sender_name: "Them".to_owned(),
            sender_id: sender,
            text_type: text_type::SPEECH,
            range: 25.0,
        })
    };
    let (ranged_open, ranged_open_stats) = heard_with(Table::Empty, ranged(THEM));
    let (ranged_shut, ranged_shut_stats) =
        heard_with(Table::Character(THEM, text_type::SPEECH), ranged(THEM));

    // A tell is never refused by the client: the shard filters those.
    let tell = ui_event(&comms::CommunicationHearDirectSpeech {
        message: "p2".to_owned(),
        sender_name: "Them".to_owned(),
        sender_id: THEM,
        target_id: ME,
        text_type: text_type::SPEECH_DIRECT,
        secret_flags: 0,
    });
    let (tell_open, _) = heard_with(Table::Empty, tell.clone());
    let (tell_shut, _) = heard_with(Table::Character(THEM, text_type::SPEECH_DIRECT), tell);

    // An emote is gated on the emote type, and your own is not exempt.
    let emote = |sender| {
        ui_event(&CommunicationHearEmote {
            sender,
            sender_name: "Them".to_owned(),
            text: "waves.".to_owned(),
        })
    };
    let (emote_open, _) = heard_with(Table::Empty, emote(THEM));
    let (emote_shut, _) = heard_with(Table::Character(THEM, text_type::EMOTE), emote(THEM));
    let (own_emote_shut, _) = heard_with(Table::Character(ME, text_type::EMOTE), emote(ME));

    // A channel line consults only the list-wide table, and only for a squelchable channel.
    let channel = |c: u32| {
        ui_event(&CommunicationChannelBroadcastRecv {
            channel: c,
            sender_name: "Them".to_owned(),
            message: "p2".to_owned(),
        })
    };
    let (channel_open, _) = heard_with(Table::Empty, channel(0x800));
    let (channel_shut, _) = heard_with(Table::Global(text_type::FELLOWSHIP), channel(0x800));
    let (channel_not_squelchable, _) =
        heard_with(Table::Global(text_type::SOCIAL), channel(0x0200_0000));
    let (channel_per_player, _) = heard_with(
        Table::Character(THEM, text_type::FELLOWSHIP),
        channel(0x800),
    );

    // A system line is refused by a list-wide squelch of its own type and by no per-player one.
    let system = |ty: u32| {
        ui_event(&comms::CommunicationTextboxString {
            text: "p2".to_owned(),
            text_type: ty,
        })
    };
    let (system_open, _) = heard_with(Table::Empty, system(text_type::CRAFT));
    let (system_shut, _) = heard_with(Table::Global(text_type::CRAFT), system(text_type::CRAFT));
    let (system_per_player, _) = heard_with(
        Table::Character(THEM, text_type::CRAFT),
        system(text_type::CRAFT),
    );
    // …and a type that is not a squelchable channel is drawn however the table is set.
    let (system_unsquelchable, _) =
        heard_with(Table::Global(text_type::SYSTEM), system(text_type::SYSTEM));

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.squelch.every-incoming-path-asks-on-the-type-it-carries",
        move |_| {
            let says = say_open == 1 && say_shut == 0 && say_other_type == 1 && own_say == 1;
            let ranged_ok = ranged_open == 0
                && ranged_open_stats.speech_lines_squelched == 0
                && ranged_open_stats.ranged_lines_out_of_range == 1
                && ranged_shut == 0
                && ranged_shut_stats.speech_lines_squelched == 1
                && ranged_shut_stats.ranged_lines_out_of_range == 0;
            let tells = tell_open == 1 && tell_shut == 1;
            let emotes = emote_open == 1 && emote_shut == 0 && own_emote_shut == 0;
            let channels = channel_open == 1
                && channel_shut == 0
                && channel_not_squelchable == 1
                && channel_per_player == 1;
            let system_lines = system_open == 1
                && system_shut == 0
                && system_per_player == 1
                && system_unsquelchable == 1;
            says && ranged_ok && tells && emotes && channels && system_lines
        },
    );
}

// =============================================================================================
// 8. chat.death.a-third-partys-death-is-announced-and-your-own-is-not
// =============================================================================================

fn death(
    message: &str,
    killed: ObjectId,
    killer: ObjectId,
) -> dereth_protocol::combat::CombatHandlePlayerDeathEvent {
    dereth_protocol::combat::CombatHandlePlayerDeathEvent {
        message: message.to_owned(),
        killed,
        killer,
    }
}

/// Deliver one death announcement to a client that knows who the player is, and report the lines.
fn death_lines(
    player: Option<ObjectId>,
    m: &dereth_protocol::combat::CombatHandlePlayerDeathEvent,
) -> (Vec<String>, dereth_client::hud::HudStats) {
    let mut c = HeadlessClient::model();
    c.world_mut().player = player;
    // The scroll is drained at the head of the next batch, so the frame after the message is
    // what carries the line -- the client's own ordering.
    c.when(Inbound::message(m)).tick(1);
    (
        c.chat_lines().iter().map(|l| l.body.clone()).collect(),
        c.view().hud().stats,
    )
}

/// Somebody else's death is announced; your own death and your own kill are not.
pub fn a_third_partys_death_is_announced() {
    const VICTIM: ObjectId = ObjectId(0x5000_0002);
    const KILLER: ObjectId = ObjectId(0x5000_0003);

    let (third_party, third_party_stats) = death_lines(
        Some(ME),
        &death("Gaerlan has been slain by Larktest!", VICTIM, KILLER),
    );
    let (own_death, own_death_stats) = death_lines(
        Some(ME),
        &death("You have been slain by Gaerlan!", ME, KILLER),
    );
    let (own_kill, own_kill_stats) = death_lines(
        Some(ME),
        &death("Gaerlan has been slain by you!", VICTIM, ME),
    );
    let (empty, _) = death_lines(Some(ME), &death("", VICTIM, KILLER));
    // With no player at all the client is not involved, so the line is shown.
    let (no_player_yet, _) = death_lines(None, &death("Gaerlan dies.", VICTIM, KILLER));

    // A truncated body is refused rather than announced, and is not counted as a death either.
    let truncated = {
        let mut c = HeadlessClient::model();
        c.world_mut().player = Some(ME);
        let mut blob = dereth_protocol::write_blob(&death("Gaerlan dies.", VICTIM, KILLER))
            .expect("the message encodes");
        blob.truncate(6);
        c.when(Inbound::event(SessionEvent::UiEvent {
            opcode: dereth_protocol::Opcode::COMBAT_HANDLE_PLAYER_DEATH_EVENT,
            blob,
        }))
        .tick(1);
        c.chat_lines().is_empty()
            && c.view().hud().stats.player_deaths == 0
            && c.view().hud().stats.undecodable == 1
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.death.a-third-partys-death-is-announced-and-your-own-is-not",
        move |_| {
            third_party == ["Gaerlan has been slain by Larktest!"]
                && third_party_stats.player_deaths_announced == 1
                && third_party_stats.scroll_lines == 1
                && own_death.is_empty()
                && own_death_stats.player_deaths == 1
                && own_death_stats.player_deaths_announced == 0
                && own_kill.is_empty()
                && own_kill_stats.player_deaths == 1
                && own_kill_stats.player_deaths_announced == 0
                && empty.is_empty()
                && no_player_yet == ["Gaerlan dies."]
                && truncated
        },
    );
}

// =============================================================================================
// 9. chat.speech.a-speaker-the-client-cannot-place-is-always-heard
// =============================================================================================

fn offset(d: f32) -> (f32, f32) {
    let h = HEADING_DEGREES.to_radians();
    (d * math::sinf(h), d * math::cosf(h))
}

/// The two escapes: a speaker with no body, and a line with no speaker at all, are audible at a
/// distance that would otherwise refuse -- and the escape is ahead of the squelch too.
pub fn an_unplaceable_speaker_is_always_heard() {
    const OUTDOOR_RANGE: f32 = 75.0;
    let far = Some(offset(500.0));

    let mut c = HeadlessClient::model();
    let (control, no_speaker, no_body) = {
        let chat = &c.view().world().chat;
        let control = chat.can_hear(SPEAKER, "", text_type::SPEECH, far, OUTDOOR_RANGE);
        let no_speaker = chat.can_hear(ObjectId(0), "", text_type::SPEECH, far, OUTDOOR_RANGE);
        let no_body = chat.can_hear(SPEAKER, "", text_type::SPEECH, None, OUTDOOR_RANGE);
        (control, no_speaker, no_body)
    };

    // …and the no-speaker escape really is ahead of the squelch, not merely of the distance.
    let squelched_zero = {
        let chat = &mut c.world_mut().chat;
        let mut e = SquelchEntry::default();
        e.types.insert(text_type::SPEECH);
        chat.squelch.characters.insert(ObjectId(0), e);
        chat.squelch.global.types.insert(text_type::SPEECH);
        chat.is_squelched(ObjectId(0), "", text_type::SPEECH)
            && chat.can_hear(ObjectId(0), "", text_type::SPEECH, far, OUTDOOR_RANGE)
    };

    // Through the real handler: a speaker the client holds no body for is drawn anyway.
    let mut driven = a_listener();
    driven.when(Inbound::message(&a_say(SPEAKER, "heard anyway")));
    let drawn = driven.chat_lines().len() == 1
        && driven.view().hud().stats.speech_lines_out_of_earshot == 0;

    c.assert_behaviour(
        "chat.speech.a-speaker-the-client-cannot-place-is-always-heard",
        move |_| !control && no_speaker && no_body && squelched_zero && drawn,
    );
}

// =============================================================================================
// 10. chat.soul-emote.your-own-is-not-echoed-back
// =============================================================================================

/// A client that is `who`, standing where nothing can be out of earshot.
fn emote_listener(who: ObjectId) -> HeadlessClient {
    let mut c = HeadlessClient::model();
    c.hud_mut().player = Some(who);
    c.hud_mut().player_body = true;
    c.world_mut().player = Some(who);
    c
}

/// Your own pose is not echoed back to you; an acted emote from you is. A marked name keeps its
/// mark on the pose path and loses it on the acted one.
pub fn your_own_soul_emote_is_not_echoed_back() {
    let acted = dereth_protocol::Opcode::COMMUNICATION_HEAR_EMOTE;
    let soul = dereth_protocol::Opcode::COMMUNICATION_HEAR_SOUL_EMOTE;
    let body = |sender: ObjectId, name: &str, text: &str| CommunicationHearEmote {
        sender,
        sender_name: name.to_owned(),
        text: text.to_owned(),
    };
    let deliver = |who: ObjectId, opcode: dereth_protocol::Opcode, m: &CommunicationHearEmote| {
        let mut c = emote_listener(who);
        c.when(Inbound::event(SessionEvent::UiEvent {
            opcode,
            blob: {
                let mut w = dereth_protocol::archive::Writer::new();
                w.u32(opcode.0);
                <CommunicationHearEmote as dereth_protocol::Message>::write(m, &mut w)
                    .expect("the message encodes");
                w.into_inner()
            },
        }));
        (
            c.chat_lines()
                .iter()
                .map(|l| l.body.clone())
                .collect::<Vec<_>>(),
            c.view().hud().stats,
        )
    };

    // Four cells: each opcode, from yourself and from somebody else.
    let (own_acted, _) = deliver(SPEAKER, acted, &body(SPEAKER, "Alba", "bows deeply."));
    let (own_soul, own_soul_stats) = deliver(SPEAKER, soul, &body(SPEAKER, "Alba", "bows deeply."));
    let (other_acted, _) = deliver(ME, acted, &body(SPEAKER, "Alba", "bows."));
    let (other_soul, _) = deliver(ME, soul, &body(SPEAKER, "Alba", "bows."));

    // A name the shard has marked: the pose path keeps the mark and is understood, the acted path
    // strips it and garbles the line.
    let (marked_acted, marked_acted_stats) = deliver(ME, acted, &body(SPEAKER, "Alba&", "waves."));
    let (marked_soul, marked_soul_stats) = deliver(ME, soul, &body(SPEAKER, "Alba&", "waves."));

    let mut c = HeadlessClient::model();
    c.assert_behaviour("chat.soul-emote.your-own-is-not-echoed-back", move |_| {
        let self_echo = own_acted.len() == 1
            && own_soul.is_empty()
            && own_soul_stats.soul_emote_self_echoes_discarded == 1;
        let others = other_acted.len() == 1 && other_soul.len() == 1;
        let garbled = marked_acted.len() == 1
            && marked_acted[0].starts_with("Alba ")
            && marked_acted_stats.emote_lines_untranslated == 1;
        let understood = marked_soul == ["Alba& waves.".to_owned()]
            && marked_soul_stats.emote_lines_untranslated == 0;
        self_echo && others && garbled && understood && marked_acted != marked_soul
    });
}

// =============================================================================================
// 11. chat.ranged-speech.is-gated-by-the-range-the-message-carries
// =============================================================================================

/// A speaker placed at a known point, and a listener `d` metres away, through the real handler.
fn ranged_scene(
    d: f32,
    outside: bool,
    range: f32,
    placed: bool,
) -> (usize, dereth_client::hud::HudStats) {
    let origin = Vec3::new(100.0, 100.0, 0.0);
    let cell = if outside { OUTDOOR_CELL } else { 0xA9B4_0100 };
    let mut c = HeadlessClient::model();
    if placed {
        let create = dereth_protocol::objects::ItemCreateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id: SPEAKER,
                objdesc: dereth_protocol::types::ObjDesc::default(),
                physicsdesc: dereth_protocol::types::PhysicsDesc {
                    bitfield: dereth_protocol::types::physicsdesc::flags::POSITION,
                    position: Some(dereth_protocol::types::PositionWire {
                        objcell_id: cell,
                        frame: dereth_protocol::types::Frame {
                            origin: dereth_protocol::types::Vec3 {
                                x: origin.x,
                                y: origin.y,
                                z: origin.z,
                            },
                            orientation: dereth_protocol::types::Quat::default(),
                        },
                    }),
                    ..dereth_protocol::types::PhysicsDesc::default()
                },
                wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
            },
        );
        c.when(Inbound::world_view(&create));
    } else {
        let mut w = dereth_client_model::Weenie::new(SPEAKER);
        w.valid = true;
        c.world_mut().tables.weenies.insert(SPEAKER, w);
    }

    let (dx, dy) = offset(d);
    c.sync_viewer(Some(ViewerFrame {
        position: Position::new(
            CellId(cell),
            Frame::new(
                Vec3::new(origin.x - dx, origin.y - dy, origin.z),
                Quat::IDENTITY,
            ),
        ),
        heading_degrees: HEADING_DEGREES,
    }));
    c.when(Inbound::message(&comms::CommunicationHearRangedSpeech {
        message: "o496".to_owned(),
        sender_name: "Speaker".to_owned(),
        sender_id: SPEAKER,
        text_type: text_type::SPEECH,
        range,
    }));
    (c.chat_lines().len(), c.view().hud().stats)
}

/// A ranged line is gated by the range on the message and by nothing else.
pub fn a_ranged_line_is_gated_by_its_own_range() {
    /// Neither of the two hearing radii, so a build reaching for one of those answers differently.
    const WIRE_RANGE: f32 = 30.0;

    let near = ranged_scene(1.0, true, WIRE_RANGE, true).0;
    let inside = ranged_scene(WIRE_RANGE - 0.1, true, WIRE_RANGE, true);
    let outside = ranged_scene(WIRE_RANGE + 0.1, true, WIRE_RANGE, true);
    let far = ranged_scene(60.0, true, WIRE_RANGE, true).0;
    // The cell makes no difference at all, which is what fails if the wire range is quietly
    // replaced by the hearing radius.
    let indoors_in = ranged_scene(28.0, false, WIRE_RANGE, true).0;
    let indoors_out = ranged_scene(40.0, false, WIRE_RANGE, true).0;
    let outdoors_in = ranged_scene(28.0, true, WIRE_RANGE, true).0;
    let outdoors_out = ranged_scene(40.0, true, WIRE_RANGE, true).0;
    // A speaker the client holds no body for is refused here, which is the opposite of the
    // ordinary speech path.
    let unplaced = ranged_scene(1.0, true, 1.0e9, false);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.ranged-speech.is-gated-by-the-range-the-message-carries",
        move |_| {
            near == 1
                && inside.0 == 1
                && outside.0 == 0
                && outside.1.ranged_lines_out_of_range == 1
                && far == 0
                && indoors_in == 1
                && indoors_out == 0
                && outdoors_in == 1
                && outdoors_out == 0
                && unplaced.0 == 0
                && unplaced.1.ranged_lines_out_of_range == 1
        },
    );
}

// =============================================================================================
// 12. chat.talk-focus.every-row-is-told-every-time-and-the-menu-guard-does-not-unset-it
// =============================================================================================

/// The six talk-to rows the chat-room channels own, in the order the menu lists them.
const CHANNEL_ROWS: [u32; 6] = [8, 9, 10, 11, 12, 13];
/// The fellowship row, which an Olthoi character may not pick.
const FELLOWSHIP_ROW: u32 = 3;

/// Every row is told every time, and the menu's own guard does not clear the setting.
pub fn every_talk_focus_row_is_told_every_time() {
    let mut c = HeadlessClient::model();

    // **No equality gate.** The same answer twice is two answers, because a menu that missed the
    // first one would otherwise stay stale for ever.
    {
        let w = c.world_mut();
        w.chat.set_talk_focus_enabled(TalkFocus::General, true);
        w.chat.set_talk_focus_enabled(TalkFocus::General, true);
    }
    let repeated = c.world_mut().chat.take_talk_focus_notices();
    let told_twice = repeated.len() == 2
        && repeated[0] == repeated[1]
        && (
            repeated[0].focus as u32,
            repeated[0].enabled,
            repeated[0].is_olthoi,
        ) == (CHANNEL_ROWS[0], true, false);

    // Turning the whole set on and then off again tells all six rows each time, in the menu's
    // order, and says which kind of character it is answering for.
    {
        let w = c.world_mut();
        w.chat.using_turbine_chat = true;
        w.enable_chat_talk_focuses(true);
        w.enable_chat_talk_focuses(false);
    }
    let batch = c.world_mut().chat.take_talk_focus_notices();
    let in_order = batch.iter().map(|n| n.focus as u32).collect::<Vec<_>>()
        == [CHANNEL_ROWS, CHANNEL_ROWS].concat();
    let olthoi_then_not =
        batch[..6].iter().all(|n| n.is_olthoi) && batch[6..].iter().all(|n| !n.is_olthoi);
    let drained = c.world_mut().chat.take_talk_focus_notices().is_empty();

    // **The menu's guard.** An Olthoi cannot pick a fellowship, so the row is greyed -- and the
    // setting behind it is left switched on, so the row comes back when the guard does not apply.
    let mut ui = dereth_ui::UiSystem::new((800, 600));
    let mut menu = MainChatPanel::default();
    menu.is_olthoi = true;
    menu.enable_selection(&mut ui, FELLOWSHIP_ROW, true);
    let greyed = menu.talk_focus_row_state(FELLOWSHIP_ROW) == Some(dereth_ui::StateId(0xD));
    let setting_kept = menu.is_talk_focus_enabled(FELLOWSHIP_ROW);

    c.assert_behaviour(
        "chat.talk-focus.every-row-is-told-every-time-and-the-menu-guard-does-not-unset-it",
        move |_| {
            told_twice
                && batch.len() == 12
                && in_order
                && olthoi_then_not
                && drained
                && greyed
                && setting_kept
        },
    );
}

// =============================================================================================
// 13. chat.turbine.an-incoming-line-is-refused-unless-the-packet-is-whole
// 14. chat.turbine.each-room-is-drawn-with-its-own-name-and-type
// 15. chat.turbine.a-failure-answer-names-the-line-and-a-relog-keeps-the-service
// =============================================================================================

/// The player, who is also the sender the room lines below carry.
const ROOM_SENDER: ObjectId = ObjectId(0x5000_0017);
/// The one overshoot the reference server states in a room packet's extent, in bytes.
const ACE_OVERSTATEMENT: u32 = 8;

fn wide_into(out: &mut Vec<u8>, text: &str) {
    let units: Vec<u16> = text.encode_utf16().collect();
    assert!(units.len() < 128, "the length prefix is one byte");
    out.push(u8::try_from(units.len()).expect("checked above"));
    out.extend(units.into_iter().flat_map(u16::to_le_bytes));
}

/// One chat-room datagram: a thirty-two byte header, then `body`, with `excess` bytes of extent
/// the body does not actually carry.
fn room_packet(kind: u32, dispatch: u32, body: Vec<u8>, excess: u32) -> Vec<u8> {
    let len = u32::try_from(body.len()).expect("small");
    let mut raw = (len + 32 + excess).to_le_bytes().to_vec();
    for v in [
        kind,
        dispatch,
        1,
        0x000B_00B5,
        1,
        0x000B_00B5,
        0,
        len + excess,
    ] {
        raw.extend(v.to_le_bytes());
    }
    raw.extend(body);
    raw
}

/// A line somebody said in `room`. `extra` is the trailing block that carries the speaker.
fn room_line(room: u32, text: &str, extra: &[u8], excess: u32) -> Vec<u8> {
    let mut body = room.to_le_bytes().to_vec();
    wide_into(&mut body, "Speaker");
    wide_into(&mut body, text);
    body.extend(u32::try_from(extra.len()).expect("small").to_le_bytes());
    body.extend_from_slice(extra);
    room_packet(1, 1, body, excess)
}

/// The speaker block a real room line carries.
fn speaker_block() -> Vec<u8> {
    [ROOM_SENDER.0, 0x8007_0005, 10]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect()
}

/// The service's answer to a line the player sent.
fn room_answer(context: u32, result: u32) -> Vec<u8> {
    room_packet(
        5,
        1,
        [context, 99, 77, result]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
        0,
    )
}

/// A client whose chat-room service is up, with every room the shard names and the player
/// listening to the general channel.
fn in_every_chat_room() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    let w = c.world_mut();
    w.chat.startup_turbine_chat();
    w.chat.recv_chat_room_tracker(ChatRoomMembership {
        allegiance_room: 101,
        general_room: 102,
        trade_room: 103,
        lfg_room: 104,
        roleplay_room: 105,
        society_room: 106,
        society_celhan_room: 107,
        society_eldweb_room: 108,
        society_radblo_room: 109,
        olthoi_room: 110,
    });
    w.player = Some(ROOM_SENDER);
    w.player_system.options.set(27, true);
    c
}

/// A room packet is read only when every field it promises is there.
pub fn an_incoming_room_packet_must_be_whole() {
    let exact = room_line(102, "hello", &speaker_block(), 0);
    let overstated = room_line(102, "hello", &speaker_block(), ACE_OVERSTATEMENT);

    let a = dereth_protocol::turbine::decode_incoming(&exact).expect("the exact packet decodes");
    let b = dereth_protocol::turbine::decode_incoming(&overstated)
        .expect("and so does the reference server's own overshoot");
    let same_line = a.payload == b.payload && !a.ace_overstated_extent && b.ace_overstated_extent;
    // The bytes are never rewritten: what arrived is what is kept.
    let kept = b.raw == overstated;

    // Cut anywhere, and it is refused rather than half-read.
    let truncation_refused = (0..exact.len()).all(|end| {
        dereth_protocol::turbine::decode_incoming(&exact[..end]).is_err()
            && dereth_protocol::turbine::decode_incoming(&overstated[..end]).is_err()
    });
    // And no other overshoot is tolerated -- the one the reference server sends, and nothing else.
    let other_excess_refused = [1_u32, 4, 7, 9, 16, u32::MAX - 200]
        .into_iter()
        .all(|excess| {
            dereth_protocol::turbine::decode_incoming(&room_line(
                102,
                "hello",
                &speaker_block(),
                excess,
            ))
            .is_err()
        });

    // A kind of packet the client has no use for is kept as it arrived and draws nothing.
    let unknown = room_packet(1, 0xABC, vec![0xFF, 0, 0xAA, 0x55], 0);
    let decoded =
        dereth_protocol::turbine::decode_incoming(&unknown).expect("an unknown kind decodes");
    let unknown_kept = decoded.payload == dereth_protocol::turbine::IncomingPayload::Unknown
        && decoded.raw == unknown;
    let unknown_never_relaxed =
        dereth_protocol::turbine::decode_incoming(&room_packet(1, 0xABC, vec![0; 4], 8)).is_err();

    let mut c = in_every_chat_room();
    c.when(Inbound::event(SessionEvent::TurbineChat(unknown)));

    c.assert_behaviour(
        "chat.turbine.an-incoming-line-is-refused-unless-the-packet-is-whole",
        move |v| {
            same_line
                && kept
                && truncation_refused
                && other_excess_refused
                && unknown_kept
                && unknown_never_relaxed
                && v.chat_lines().is_empty()
        },
    );
}

/// Every room has its own bracketed name and its own channel, and a line that is not one is drawn
/// nowhere.
///
/// The text is plain ASCII here on purpose: what the host's own spelling does to a line is
/// `chat.turbine.host-text-conversion-happens-before-the-markup-is-read`'s claim, and this one
/// would otherwise depend on the code page the machine running it happens to use.
pub fn each_chat_room_is_drawn_with_its_own_name() {
    let mut c = in_every_chat_room();
    // Reception does not repeat the gate the sending half applies.
    c.world_mut().player_system.options.set(35, false);

    c.when(Inbound::event(SessionEvent::TurbineChat(room_line(
        102,
        "hello all",
        &speaker_block(),
        ACE_OVERSTATEMENT,
    ))));
    let general = c
        .chat_lines()
        .last()
        .map(|l| (l.ty, l.window, l.body.clone()));
    let counted = c.view().world().chat.turbine_extent_discrepancies() == 1;

    let mut per_room = Vec::new();
    for (room, ty, name) in [
        (101_u32, 18_u8, "Allegiance"),
        (103, 28, "Trade"),
        (104, 29, "LFG"),
        (105, 30, "Roleplay"),
        (106, 32, "Society"),
        (107, 32, "Celestial Hand"),
        (108, 32, "Eldrytch Web"),
        (109, 32, "Radiant Blood"),
        (110, 18, "Olthoi"),
    ] {
        let before = c.chat_lines().len();
        c.when(Inbound::event(SessionEvent::TurbineChat(room_line(
            room,
            "next",
            &speaker_block(),
            0,
        ))));
        let drawn = &c.chat_lines()[before..];
        per_room.push(
            drawn.len() == 1
                && drawn[0].ty == ty
                && drawn[0].body.starts_with(&format!("[{name}] ")),
        );
    }

    // A room the player is in no channel for, a forged clickable-name marker, and a second line
    // smuggled inside the first are each drawn nowhere.
    let mut refused = Vec::new();
    for raw in [
        room_line(999, "unknown room", &speaker_block(), 0),
        room_line(102, "<TeLl:bad>", &speaker_block(), 0),
        room_line(102, "bad\nline", &speaker_block(), 0),
    ] {
        let before = c.chat_lines().len();
        c.when(Inbound::event(SessionEvent::TurbineChat(raw)));
        refused.push(c.chat_lines().len() == before);
    }

    // A speaker the player has squelched is drawn nowhere either.
    let mut squelch = SquelchEntry::default();
    squelch.squelch_everything();
    c.world_mut()
        .chat
        .squelch
        .characters
        .insert(ROOM_SENDER, squelch);
    let before = c.chat_lines().len();
    c.when(Inbound::event(SessionEvent::TurbineChat(room_line(
        102,
        "squelched",
        &speaker_block(),
        0,
    ))));
    let squelched = c.chat_lines().len() == before;
    c.world_mut().chat.squelch.characters.clear();

    // A line with no speaker at all is counted as one the client could not read.
    let undecodable_before = c.view().hud().stats.undecodable;
    let before = c.chat_lines().len();
    c.when(Inbound::event(SessionEvent::TurbineChat(room_line(
        102,
        "no speaker",
        &[],
        0,
    ))));
    let no_speaker = c.chat_lines().len() == before
        && c.view().hud().stats.undecodable == undecodable_before + 1;

    // The room is matched by its number and not by the order the rooms were listed in: the same
    // number registered under a second channel is drawn as the channel that number belongs to.
    c.world_mut().chat.chat_rooms.insert(1, 102);
    let before = c.chat_lines().len();
    c.when(Inbound::event(SessionEvent::TurbineChat(room_line(
        102,
        "duplicate room",
        &speaker_block(),
        0,
    ))));
    let by_number = c.chat_lines()[before..]
        .first()
        .is_some_and(|l| l.ty == 18 && l.body.starts_with("[Allegiance] "));

    c.assert_behaviour(
        "chat.turbine.each-room-is-drawn-with-its-own-name-and-type",
        move |_| {
            general.as_ref().is_some_and(|(ty, window, body)| {
                (*ty, *window) == (27, 0)
                    && body.starts_with("[General] ")
                    && body.contains("hello all")
            }) && counted
                && per_room.iter().all(|ok| *ok)
                && refused.iter().all(|ok| *ok)
                && squelched
                && no_speaker
                && by_number
        },
    );
}

/// A refused line is named back to the player once, and a relog leaves the service running.
pub fn a_failed_room_line_is_named_back_once() {
    let mut c = in_every_chat_room();
    let mut sent = dereth_client_model::RecordingRequests::default();
    {
        let w = c.world_mut();
        assert!(w.send_turbine_chat(&mut sent, TalkFocus::General, false, "first", 4, 100));
        assert!(w.send_turbine_chat(&mut sent, TalkFocus::General, false, "second", 4, 102));
    }
    let both_in_flight = c.view().world().chat.pending_turbine_contexts() == [1, 2];

    // The service refuses the second one. The player is told which line it was.
    c.when(Inbound::event(SessionEvent::TurbineChat(room_answer(2, 1))));
    let told = c
        .chat_lines()
        .last()
        .map(|l| (l.ty, l.window, l.body.clone()));
    let only_that_one = c.view().world().chat.pending_turbine_contexts() == [1];

    // An answer to a line it is not waiting on changes nothing, twice over.
    let before = c.chat_lines().len();
    for context in [2_u32, 99] {
        c.when(Inbound::event(SessionEvent::TurbineChat(room_answer(
            context, 1,
        ))));
    }
    // ...and neither does an answer that is cut short.
    let mut truncated = room_answer(1, 0);
    truncated.pop();
    c.when(Inbound::event(SessionEvent::TurbineChat(truncated)));
    let nothing_else =
        c.chat_lines().len() == before && c.view().world().chat.pending_turbine_contexts() == [1];

    // A success is silent, and takes the line out of flight.
    c.when(Inbound::event(SessionEvent::TurbineChat(room_answer(1, 0))));
    let success_silent = c.chat_lines().len() == before
        && c.view().world().chat.pending_turbine_contexts().is_empty();

    // **The relog.** One character out, another in: the rooms, the line still in flight and the
    // how-fast-may-I-talk bucket are the process's and survive; the name and the talk target are
    // the character's and do not.
    let mut c2 = in_every_chat_room();
    let mut out = dereth_client_model::RecordingRequests::default();
    {
        let w = c2.world_mut();
        assert!(w.send_turbine_chat(&mut out, TalkFocus::General, false, "old character", 0, 100));
        w.chat.last_teller_name = "old name".to_owned();
        w.chat.set_talk_focus(TalkFocus::General);
    }
    c2.when(Inbound::event(SessionEvent::LoggedOff));
    let service_kept = {
        let w = c2.view().world();
        w.player.is_none()
            && w.chat.last_teller_name.is_empty()
            && w.chat.talk_focus == TalkFocus::All
            && w.chat.using_turbine_chat
            && w.chat.chat_rooms.get(&2) == Some(&102)
            && w.chat.pending_turbine_contexts() == [1]
    };
    // The bucket did not start again with the new character: the same second is still too soon.
    let still_too_soon = {
        let w = c2.world_mut();
        w.player_system.options.set(27, true);
        !w.send_turbine_chat(&mut out, TalkFocus::General, false, "spam", 0, 100)
    };
    // The old character's line can still be answered, and the new character sends as himself.
    let answered = c2.world_mut().recv_turbine_chat(&room_answer(1, 0))
        && c2.view().world().chat.pending_turbine_contexts().is_empty();
    {
        let w = c2.world_mut();
        w.scroll.clear();
        w.player = Some(ObjectId(0x5000_0099));
        w.chat.startup_turbine_chat();
        assert!(w.send_turbine_chat(&mut out, TalkFocus::General, false, "new character", 0, 102));
    }
    let under_the_new_name = matches!(
        out.0.last(),
        Some(Request::TurbineChat(m)) if m.sender == 0x5000_0099
    ) && c2.view().world().chat.pending_turbine_contexts() == [2];

    c.assert_behaviour(
        "chat.turbine.a-failure-answer-names-the-line-and-a-relog-keeps-the-service",
        move |_| {
            both_in_flight
                && told == Some((0, 0, "Failed to send text: [second] to room 66.".to_owned()))
                && only_that_one
                && nothing_else
                && success_silent
                && service_kept
                && still_too_soon
                && answered
                && under_the_new_name
        },
    );
}

// =============================================================================================
// 16. chat.garble.a-speaker-you-cannot-understand-is-drawn-as-a-name-and-a-noise
// 17. chat.garble.a-tell-you-cannot-understand-is-drawn-even-when-it-was-not-for-you
// 18. chat.garble.an-acted-emote-is-garbled-where-a-pose-and-a-shout-are-not
// =============================================================================================

/// A listener with a body, outdoors, whose character is of `heritage`, and whose random draws are
/// seeded so that the noises it hears are reproducible.
fn a_listener_of(heritage: i32, seed: i32) -> HeadlessClient {
    let mut c = HeadlessClient::model();
    c.sync_viewer(Some(ViewerFrame {
        position: Position::new(
            CellId(OUTDOOR_CELL),
            Frame::new(Vec3::new(10.0, 10.0, 0.0), Quat::IDENTITY),
        ),
        heading_degrees: HEADING_DEGREES,
    }));
    let mut q = dereth_client_model::qualities::Qualities::new();
    q.ints = Some([(HERITAGE_GROUP_PROPERTY, heritage)].into_iter().collect());
    c.world_mut().seed_player_desc(ME, q);
    {
        let hud = c.hud_mut();
        hud.seed_random(seed);
        hud.player = Some(ME);
        hud.player_desc_received = true;
    }
    c
}

fn a_spoken_line(name: &str, message: &str) -> CommunicationHearSpeech {
    CommunicationHearSpeech {
        message: message.to_owned(),
        sender_name: name.to_owned(),
        sender_id: SPEAKER,
        text_type: text_type::SPEECH,
    }
}

fn a_tell(target: ObjectId, name: &str, message: &str) -> comms::CommunicationHearDirectSpeech {
    comms::CommunicationHearDirectSpeech {
        message: message.to_owned(),
        sender_name: name.to_owned(),
        sender_id: SPEAKER,
        target_id: target,
        text_type: text_type::SPEECH,
        secret_flags: 0,
    }
}

fn an_emote(opcode: dereth_protocol::Opcode, name: &str, text: &str) -> SessionEvent {
    let m = CommunicationHearEmote {
        sender: SPEAKER,
        sender_name: name.to_owned(),
        text: text.to_owned(),
    };
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(opcode.0);
    <CommunicationHearEmote as dereth_protocol::Message>::write(&m, &mut w).expect("it encodes");
    SessionEvent::UiEvent {
        opcode,
        blob: w.into_inner(),
    }
}

/// The speaker, with a body a metre from where [`a_listener_of`] puts the player -- which is what
/// a shout's own range gate needs before it will look at the line at all.
fn a_placed_speaker() -> dereth_protocol::objects::ItemCreateObject {
    dereth_protocol::objects::ItemCreateObject(dereth_protocol::objects::ObjectCreatePayload {
        id: SPEAKER,
        objdesc: dereth_protocol::types::ObjDesc::default(),
        physicsdesc: dereth_protocol::types::PhysicsDesc {
            bitfield: dereth_protocol::types::physicsdesc::flags::POSITION,
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: OUTDOOR_CELL,
                frame: dereth_protocol::types::Frame {
                    origin: dereth_protocol::types::Vec3 {
                        x: 11.0,
                        y: 10.0,
                        z: 0.0,
                    },
                    orientation: dereth_protocol::types::Quat::default(),
                },
            }),
            ..dereth_protocol::types::PhysicsDesc::default()
        },
        wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
    })
}

/// A speaker whose tongue the character does not share is drawn as his name and a noise.
pub fn a_speaker_you_cannot_understand_is_a_name_and_a_noise() {
    use dereth_client::chat::{garbled_line, HUMAN_TEXT, OLTHOI_TEXT};

    // A human hearing a speaker the shard has marked as speaking the other tongue.
    let mut c = a_listener_of(0, 7);
    c.when(Inbound::message(&a_spoken_line("Alba&", "hello there")));
    let drawn = c
        .view()
        .chat_text()
        .first()
        .map(|l| (*l).to_owned())
        .expect("the line is drawn, not dropped");
    let noise = drawn.trim_start_matches("Alba ").to_owned();
    let garbled = drawn.starts_with("Alba ")
        && !drawn.contains("says,")
        && !drawn.contains("hello there")
        && OLTHOI_TEXT.contains(&noise.as_str())
        && drawn == garbled_line("Alba", &noise)
        && c.view().hud().stats.speech_lines_untranslated == 1;

    // The same speaker with no mark is understood, which is what makes the above a measurement
    // rather than "this client garbles everything".
    let mut plain = a_listener_of(0, 7);
    plain.when(Inbound::message(&a_spoken_line("Alba", "hello there")));
    let understood = plain
        .view()
        .chat_text()
        .first()
        .is_some_and(|l| l.contains("hello there"))
        && plain.view().hud().stats.speech_lines_untranslated == 0;

    // **The mark is on the speaker, not on the listener.** An Olthoi hearing an unmarked speaker
    // is the mirror case, and it draws from the other table.
    let mut olthoi = a_listener_of(12, 5);
    olthoi.when(Inbound::message(&a_spoken_line("Alba", "hello there")));
    let mirrored = olthoi
        .view()
        .chat_text()
        .first()
        .is_some_and(|l| HUMAN_TEXT.contains(&l.trim_start_matches("Alba ")))
        && olthoi.view().hud().stats.speech_lines_untranslated == 1;
    // ...and an Olthoi hearing another Olthoi understands him.
    let mut kin = a_listener_of(12, 5);
    kin.when(Inbound::message(&a_spoken_line("Alba&", "hello there")));
    let kin_understood = kin.view().hud().stats.speech_lines_untranslated == 0;

    // **All ten noises really come up**, driven through the handler rather than read off the
    // table: a draw that was one out would show nine of them, and one that clamped would repeat
    // the first.
    let mut many = a_listener_of(0, 20_260_907);
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for _ in 0..400 {
        let before = many.chat_lines().len();
        many.when(Inbound::message(&a_spoken_line("Alba&", "hello there")));
        seen.insert(
            many.view().chat_text()[before]
                .trim_start_matches("Alba ")
                .to_owned(),
        );
    }
    let all_ten: std::collections::BTreeSet<String> =
        OLTHOI_TEXT.iter().map(|s| (*s).to_owned()).collect();
    let every_noise = seen == all_ten && many.view().hud().stats.speech_lines_untranslated == 400;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.garble.a-speaker-you-cannot-understand-is-drawn-as-a-name-and-a-noise",
        move |_| garbled && understood && mirrored && kin_understood && every_noise,
    );
}

/// A private message the character cannot understand is drawn even when it was for somebody else.
pub fn a_garbled_tell_is_drawn_even_when_it_was_not_for_you() {
    use dereth_client::chat::OLTHOI_TEXT;
    let somebody_else = ObjectId(0x5000_0003);

    let mut mine = a_listener_of(0, 3);
    mine.when(Inbound::message(&a_tell(ME, "Alba&", "psst")));
    let to_me = mine.view().chat_text().len() == 1
        && OLTHOI_TEXT.contains(&mine.view().chat_text()[0].trim_start_matches("Alba "))
        && !mine.view().chat_text()[0].contains("tells you")
        && mine.view().hud().stats.direct_speech_lines_untranslated == 1;

    // Addressed to somebody else: a garbled one is still drawn, where an understood one is not.
    let mut theirs = a_listener_of(0, 3);
    theirs.when(Inbound::message(&a_tell(somebody_else, "Alba&", "psst")));
    let theirs_garbled = theirs.view().chat_text().len() == 1
        && theirs.view().hud().stats.direct_speech_lines_untranslated == 1
        && theirs.view().hud().stats.speech_lines_not_addressed_to_us == 0;

    let mut theirs_plain = a_listener_of(0, 3);
    theirs_plain.when(Inbound::message(&a_tell(somebody_else, "Alba", "psst")));
    let theirs_understood = theirs_plain.view().chat_text().is_empty()
        && theirs_plain
            .view()
            .hud()
            .stats
            .speech_lines_not_addressed_to_us
            == 1;

    // **The reply command.** An understood message arms it; one that was never shown cannot.
    let mut armed = Vec::new();
    for name in ["Alba", "Alba&"] {
        let mut c = a_listener_of(0, 3);
        c.when(Inbound::message(&a_tell(ME, name, "psst")));
        armed.push(c.view().world().chat.last_teller.is_some());
    }

    // A message a character sends to himself is his own thought and is never a noise.
    let mut self_tell = a_listener_of(0, 3);
    self_tell.when(Inbound::message(&a_tell(SPEAKER, "Alba&", "psst")));
    let thought = self_tell.view().chat_text() == ["You think, \"psst\""]
        && self_tell
            .view()
            .hud()
            .stats
            .direct_speech_lines_untranslated
            == 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.garble.a-tell-you-cannot-understand-is-drawn-even-when-it-was-not-for-you",
        move |_| to_me && theirs_garbled && theirs_understood && armed == [true, false] && thought,
    );
}

/// An acted emote garbles; a pose and a line shouted with a range never do.
pub fn an_acted_emote_garbles_where_a_pose_and_a_shout_do_not() {
    use dereth_client::chat::OLTHOI_TEXT;
    let acted = dereth_protocol::Opcode::COMMUNICATION_HEAR_EMOTE;
    let pose = dereth_protocol::Opcode::COMMUNICATION_HEAR_SOUL_EMOTE;

    let mut c = a_listener_of(0, 11);
    c.when(Inbound::event(an_emote(acted, "Alba&", "waves.")));
    let stats = c.view().hud().stats;
    let emote_garbled = c.view().chat_text().len() == 1
        && OLTHOI_TEXT.contains(&c.view().chat_text()[0].trim_start_matches("Alba "))
        && !c.view().chat_text()[0].contains("waves.")
        // A garbled line is one the player could not read **and** one that was drawn.
        && stats.emote_lines_untranslated == 1
        && stats.emote_lines_composed == 1
        && stats.speech_lines_untranslated == 0;

    // The same name on a pose: understood, mark and all.
    let mut posed = a_listener_of(0, 11);
    posed.when(Inbound::event(an_emote(pose, "Alba&", "waves.")));
    let pose_understood = posed.view().chat_text() == ["Alba& waves."]
        && posed.view().hud().stats.emote_lines_untranslated == 0;

    // **A shout is the fourth arm, and it has no garble at all**: it keeps its ordinary wording
    // and does not even take the mark off the name. The speaker needs a body, because a shout is
    // refused outright for a speaker the client cannot place.
    let mut shouted = Vec::new();
    for (heritage, name) in [(0, "Alba&"), (12, "Alba")] {
        let mut c = a_listener_of(heritage, 9);
        // The body first, then the viewer: the earshot answer is measured against the objects the
        // HUD had when it was last told where the player is looking from.
        c.when(Inbound::world_view(&a_placed_speaker()));
        c.sync_viewer(Some(ViewerFrame {
            position: Position::new(
                CellId(OUTDOOR_CELL),
                Frame::new(Vec3::new(10.0, 10.0, 0.0), Quat::IDENTITY),
            ),
            heading_degrees: HEADING_DEGREES,
        }));
        c.when(Inbound::message(&comms::CommunicationHearRangedSpeech {
            message: "shout".to_owned(),
            sender_name: name.to_owned(),
            sender_id: SPEAKER,
            text_type: text_type::SPEECH,
            range: 100.0,
        }));
        let s = c.view().hud().stats;
        shouted.push(
            c.view().chat_text().len() == 1
                && c.view().chat_text()[0].contains("says, \"shout\"")
                && c.view().chat_text()[0].contains(name)
                && s.speech_lines_untranslated == 0
                && s.direct_speech_lines_untranslated == 0
                && s.emote_lines_untranslated == 0
                && s.ranged_lines_out_of_range == 0,
        );
    }

    let mut c2 = HeadlessClient::model();
    c2.assert_behaviour(
        "chat.garble.an-acted-emote-is-garbled-where-a-pose-and-a-shout-are-not",
        move |_| emote_garbled && pose_understood && shouted == [true, true],
    );
}

// -------------------------------------------------------------------------------------------
// The `#[test]` beside each, which runs it and checks its declaration.
// -------------------------------------------------------------------------------------------

#[test]
fn scenario_turbine_text_conversion_runs_before_the_markup() {
    scenario("turbine_text_conversion_runs_before_the_markup");
}

#[test]
fn scenario_turbine_channel_comes_up_with_the_shards_permission() {
    scenario("turbine_channel_comes_up_with_the_shards_permission");
}

#[test]
fn scenario_turbine_line_is_refused_by_three_separate_gates() {
    scenario("turbine_line_is_refused_by_three_separate_gates");
}

#[test]
fn scenario_allegiance_chat_refusal_and_rejoin() {
    scenario("allegiance_chat_refusal_and_rejoin");
}

#[test]
fn scenario_channel_broadcast_wording_per_channel() {
    scenario("channel_broadcast_wording_per_channel");
}

#[test]
fn scenario_squelch_table_off_the_wire_replaces_the_clients_own() {
    scenario("squelch_table_off_the_wire_replaces_the_clients_own");
}

#[test]
fn scenario_squelch_is_asked_on_every_incoming_path() {
    scenario("squelch_is_asked_on_every_incoming_path");
}

#[test]
fn scenario_a_third_partys_death_is_announced() {
    scenario("a_third_partys_death_is_announced");
}

#[test]
fn scenario_an_unplaceable_speaker_is_always_heard() {
    scenario("an_unplaceable_speaker_is_always_heard");
}

#[test]
fn scenario_your_own_soul_emote_is_not_echoed_back() {
    scenario("your_own_soul_emote_is_not_echoed_back");
}

#[test]
fn scenario_a_ranged_line_is_gated_by_its_own_range() {
    scenario("a_ranged_line_is_gated_by_its_own_range");
}

#[test]
fn scenario_every_talk_focus_row_is_told_every_time() {
    scenario("every_talk_focus_row_is_told_every_time");
}

#[test]
fn scenario_an_incoming_room_packet_must_be_whole() {
    scenario("an_incoming_room_packet_must_be_whole");
}

#[test]
fn scenario_each_chat_room_is_drawn_with_its_own_name() {
    scenario("each_chat_room_is_drawn_with_its_own_name");
}

#[test]
fn scenario_a_failed_room_line_is_named_back_once() {
    scenario("a_failed_room_line_is_named_back_once");
}

#[test]
fn scenario_a_speaker_you_cannot_understand_is_a_name_and_a_noise() {
    scenario("a_speaker_you_cannot_understand_is_a_name_and_a_noise");
}

#[test]
fn scenario_a_garbled_tell_is_drawn_even_when_it_was_not_for_you() {
    scenario("a_garbled_tell_is_drawn_even_when_it_was_not_for_you");
}

#[test]
fn scenario_an_acted_emote_garbles_where_a_pose_and_a_shout_do_not() {
    scenario("an_acted_emote_garbles_where_a_pose_and_a_shout_do_not");
}

// =============================================================================================
// The emote command: every spelling of it, and what an empty one does
// =============================================================================================
//
// **Nothing is sent.** Where the bytes are the claim, the request is handed to the production
// sender over a mock transport; there is no socket here.

/// The bytes one emote goes out in: the ordered-action header, the stamp the client allocated, the
/// message, and the text padded out. Composed here rather than by the writer being asserted over.
fn emote_bytes(stamp: u8) -> Vec<u8> {
    vec![
        0xb1, 0xf7, 0, 0, stamp, 0, 0, 0, 0xdf, 1, 0, 0, 5, 0, b'w', b'a', b'v', b'e', b's', 0,
    ]
}

/// Type `line` into a chat window, and answer with everything it put in the outbox.
fn command(c: &mut HeadlessClient, line: &str) -> Vec<dereth_client_model::Request> {
    let from = c.view().outbound().len();
    c.when(Player::ui(UiRequest::ChatLine {
        text: line.to_owned(),
        window: 1,
    }));
    c.view().outbound()[from..].to_vec()
}

/// Every spelling of the emote command reaches the same message, and none of them says anything
/// in the player's own window first.
pub fn every_spelling_of_the_emote_command_sends_the_same_thing() {
    let mut every = true;
    for line in [
        "@e waves",
        "@em waves",
        "@emote waves",
        "@me waves",
        "/e waves",
        "/emote waves",
        ":waves",
        ";waves",
        // Extra spaces around it, and shouted in capitals.
        "@EMOTE  waves  ",
    ] {
        let mut c = HeadlessClient::model();
        let requests = command(&mut c, line);
        let one = requests.len() == 1
            && matches!(&requests[0], dereth_client_model::Request::Emote(m) if m.message == "waves");
        let clean = c.view().interaction().stats.chat_commands_unimplemented == 0
            && c.view().interaction().stats.chat_commands_refused == 0;
        // A text emote is not echoed locally: the player reads it when the shard sends it back.
        let no_echo = c.view().world().scroll.pending().is_empty();

        let mut session = dereth_client_net::client_session::Session::new(
            dereth_client_net::client_session::testing::MockTransport::new(),
        );
        let sent = dereth_client::interaction::send_request(&mut session, &requests[0]);
        let packet = &session.transport.sent[0];
        let bytes = session.transport.sent.len() == 1
            && (packet.queue, packet.ordered) == (dereth_primitives::NetQueue::Weenie, true)
            && packet.payload == emote_bytes(1);
        assert!(one && clean && no_echo && sent && bytes, "{line}");
        every &= one && clean && no_echo && sent && bytes;
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.emote.every-spelling-of-the-command-sends-the-same-thing",
        move |_| every,
    );
}

#[test]
fn scenario_every_spelling_of_the_emote_command_sends_the_same_thing() {
    scenario("every_spelling_of_the_emote_command_sends_the_same_thing");
}

/// An emote with nothing in it is silently accepted -- not refused, not sent -- and costs the
/// player nothing: the next line he sends takes the place in the order that one would have had.
pub fn an_empty_emote_is_silent_and_costs_no_place_in_the_order() {
    let mut c = HeadlessClient::model();
    let mut session = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );

    let mut silent = true;
    for line in ["@e", "@emote  ", "@me\t", ":", ";"] {
        let requests = command(&mut c, line);
        let quiet = requests.is_empty()
            && c.view().world().scroll.pending().is_empty()
            && c.view().interaction().last_refusal.is_none();
        assert!(quiet, "{line}: an empty emote is accepted and says nothing");
        silent &= quiet;
    }
    let untouched = session.next_action_stamp() == 1;

    // An ordinary line, then an emote: they take the first and second places in the order.
    let mut sent = true;
    for line in ["ordinary", "@e waves"] {
        let requests = command(&mut c, line);
        sent &= requests.len() == 1
            && dereth_client::interaction::send_request(&mut session, &requests[0]);
    }
    let in_order = session.transport.sent.len() == 2
        && session.transport.sent[0].payload[4..12] == [1, 0, 0, 0, 0x15, 0, 0, 0]
        && session.transport.sent[1].payload == emote_bytes(2)
        && session.next_action_stamp() == 3;

    // And a line the host cannot spell at all is refused by the sender rather than half-sent, and
    // takes no place in the order either.
    let mut fresh = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );
    let unspellable =
        dereth_client_model::Request::Emote(dereth_protocol::comms::CommunicationEmote {
            message: "\u{1f642}".to_owned(),
        });
    let refused = !dereth_client::interaction::send_request(&mut fresh, &unspellable)
        && fresh.transport.sent.is_empty()
        && fresh.next_action_stamp() == 1;
    // The next one, which it can spell, takes that place.
    let after = dereth_client::interaction::send_request(
        &mut fresh,
        &dereth_client_model::Request::Emote(dereth_protocol::comms::CommunicationEmote {
            message: "waves".to_owned(),
        }),
    ) && fresh.transport.sent[0].payload == emote_bytes(1);

    c.assert_behaviour(
        "chat.emote.an-empty-one-is-silent-and-costs-no-place-in-the-order",
        move |_| silent && untouched && sent && in_order && refused && after,
    );
}

#[test]
fn scenario_an_empty_emote_is_silent_and_costs_no_place_in_the_order() {
    scenario("an_empty_emote_is_silent_and_costs_no_place_in_the_order");
}

// =============================================================================================
// chat.log.a-shard-line-still-carries-its-newline-at-the-model-boundary
// =============================================================================================

/// The two lines a shard sends on entering the world and on being asked for help. Both are
/// recorded and **both end in a newline**, which is the fact the window's own trimming exists for.
pub const WELCOME: &str = "Welcome to Asheron's Call\n  powered by ACEmulator\n\nFor more information on commands supported by this server, type @acehelp\n";
pub const ACEHELP: &str = "Note: You may substitute a forward slash (/) for the at symbol (@).\nUse @help to get more information about commands supported by the client.\nAvailable help:\n@acehelp commands - Lists all commands.\nYou can also use @acecommands to get a complete list of the supported ACEmulator commands available to you.\nTo get more information about a specific command, use @acehelp command\n";

/// The line the client composes from one of the shard's system messages.
pub fn system_line(text: &str) -> dereth_ui_screens::chat::interface::ChatMessage {
    let mut c = HeadlessClient::model();
    c.when(Inbound::message(
        &dereth_protocol::comms::CommunicationTextboxString {
            text: text.to_owned(),
            text_type: 0,
        },
    ));
    let lines = c.view().chat_lines();
    assert_eq!(lines.len(), 1, "one line per system message");
    lines[0].clone()
}

/// **The denominator for the window's trimming**: the shard's lines are still newline-ended when
/// the client has finished reading them, because the trimming happens between there and the
/// window. A scenario that went green because something upstream started trimming would leave the
/// window's own claim measuring nothing, so it is asserted on its own.
pub fn a_shard_line_still_carries_its_newline_at_the_model() {
    let mut holds = true;
    for text in [WELCOME, ACEHELP] {
        let m = system_line(text);
        holds &= m.body == text && m.body.ends_with('\n') && m.prefix.is_none() && m.window == 0;
    }
    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.log.a-shard-line-still-carries-its-newline-at-the-model-boundary",
        move |_| holds,
    );
}

#[test]
fn scenario_a_shard_line_still_carries_its_newline_at_the_model() {
    scenario("a_shard_line_still_carries_its_newline_at_the_model");
}

// =============================================================================================
// chat.tell-markup.a-composed-room-line-still-carries-its-markup-at-the-model-boundary
// =============================================================================================

/// The room this scenario's general channel is, and who speaks in it.
pub const TAG_ROOM: u32 = 123;
pub const TAG_SENDER: &str = "Sender";

/// The line the client composes around a room event, markup and all.
pub fn composed_room_line(sender: &str, said: &str) -> String {
    format!("[General] <Tell:IIDString:0:{sender}>{sender}<\\Tell> says, \"{said}\"")
}

/// One room event carrying `text`, in the shape the chat-room transport delivers.
pub fn tagged_room_event(text: &str) -> Vec<u8> {
    let mut body = TAG_ROOM.to_le_bytes().to_vec();
    for s in [TAG_SENDER, text] {
        let units: Vec<u16> = s.encode_utf16().collect();
        assert!(units.len() < 128, "the length prefix is one byte");
        body.push(u8::try_from(units.len()).expect("checked above"));
        body.extend(units.into_iter().flat_map(u16::to_le_bytes));
    }
    body.extend(
        [12_u32, 0x5000_0017, 0, 2]
            .into_iter()
            .flat_map(u32::to_le_bytes),
    );
    let mut packet = (u32::try_from(body.len()).expect("small") + 32)
        .to_le_bytes()
        .to_vec();
    packet.extend(
        [
            1_u32,
            1,
            1,
            0,
            0,
            0,
            0,
            u32::try_from(body.len()).expect("small"),
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes),
    );
    packet.extend(body);
    packet
}

/// The line the client composes from one room event.
pub fn turbine_line(text: &str) -> dereth_ui_screens::chat::interface::ChatMessage {
    let mut c = HeadlessClient::model();
    {
        let w = c.world_mut();
        w.chat.startup_turbine_chat();
        w.chat.recv_chat_room_tracker(ChatRoomMembership {
            general_room: TAG_ROOM,
            ..ChatRoomMembership::default()
        });
    }
    c.when(Inbound::event(SessionEvent::TurbineChat(
        tagged_room_event(text),
    )));
    let lines = c.view().chat_lines();
    assert_eq!(lines.len(), 1, "one line per accepted room event");
    lines[0].clone()
}

/// **The denominator for the window's markup handling**: the line the client composes around a
/// room event carries the clickable-name markup as ordinary characters of the line, and still does
/// when the client has finished composing it. Taking it off is the window's job, downstream.
pub fn a_composed_room_line_still_carries_its_markup() {
    let m = turbine_line("hello");
    let holds = m.body == composed_room_line(TAG_SENDER, "hello")
        && m.ty == 0x1B
        && m.window == 0
        && m.body.contains('<');

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.tell-markup.a-composed-room-line-still-carries-its-markup-at-the-model-boundary",
        move |_| holds,
    );
}

#[test]
fn scenario_a_composed_room_line_still_carries_its_markup() {
    scenario("a_composed_room_line_still_carries_its_markup");
}

// =============================================================================================
// chat.squelch.a-populated-list-is-read-whole-and-keeps-the-kind-of-each-entry
// =============================================================================================

/// One squelched name, as the shard sends it.
fn squelched(name: &str, account: bool) -> dereth_protocol::comms::SquelchInfo {
    dereth_protocol::comms::SquelchInfo {
        squelch_msgs: dereth_protocol::comms::VLong(vec![
            0xFFFF_FFFF,
            0xFFFF_FFFF,
            0xFFFF_FFFF,
            0xFFFF_FFFF,
        ]),
        name: name.to_owned(),
        is_zone_squelch: i32::from(account),
    }
}

/// **The check the empty recordings cannot make.** Every squelch list in the recordings is
/// empty, so a reader that had the fields in the wrong order would read all of them and every
/// scenario would stay green while the tab was blank for ever. This one has two names in it, and
/// the field that says whether a name is a whole account -- the one nothing was thought to read --
/// is asserted to survive the trip.
pub fn a_populated_squelch_list_is_read_whole() {
    let db = dereth_protocol::comms::SquelchDb {
        // The shard sends this half always empty and folds an account squelch into the other one.
        account_hash: dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries: Vec::new(),
        },
        character_hash: dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries: vec![
                (0x5000_001E, squelched("Ash", false)),
                (0x5000_001F, squelched("Bex", true)),
            ],
        },
        global_squelch_info: dereth_protocol::comms::SquelchInfo {
            squelch_msgs: dereth_protocol::comms::VLong(vec![0, 0, 0, 0]),
            name: String::new(),
            is_zone_squelch: 0,
        },
    };
    let body = dereth_protocol::write_body(&dereth_protocol::comms::CommunicationSetSquelchDb(db))
        .expect("the list encodes");
    let back: dereth_protocol::comms::CommunicationSetSquelchDb =
        dereth_protocol::read_body(&body).expect("the cursor lands exactly on the end");

    let entries = &back.0.character_hash.entries;
    let read_whole = back.0.account_hash.entries.is_empty()
        && entries.len() == 2
        && entries[0].0 == 0x5000_001E
        && entries[0].1.name == "Ash"
        && entries[0].1.is_zone_squelch == 0
        && entries[1].0 == 0x5000_001F
        && entries[1].1.name == "Bex"
        && entries[1].1.is_zone_squelch == 1;
    // And it writes back out to the same bytes, which a reader that skipped a field would not.
    let unchanged = dereth_protocol::write_body(&back).expect("encodes") == body;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.squelch.a-populated-list-is-read-whole-and-keeps-the-kind-of-each-entry",
        move |_| read_whole && unchanged,
    );
}

#[test]
fn scenario_a_populated_squelch_list_is_read_whole() {
    scenario("a_populated_squelch_list_is_read_whole");
}

// =============================================================================================
// The talk-to menu's own seams: the squelch row, the chat target, and the channel rows
// =============================================================================================

/// The rows of the talk-to menu this section is about, by the number the client knows each by.
const ROW_ALL: usize = 1;
const ROW_SELECTED: usize = 2;
const ROW_GENERAL: usize = 8;
const ROW_TRADE: usize = 9;
const ROW_LFG: usize = 10;
const ROW_ROLEPLAY: usize = 11;
const ROW_SOCIETY: usize = 12;
const ROW_OLTHOI: usize = 13;
/// The player's own options the channel rows follow, in the order they are read.
const HEAR_GENERAL: usize = 35;
const HEAR_TRADE: usize = 36;
const HEAR_LFG: usize = 37;
const HEAR_ROLEPLAY: usize = 38;
const HEAR_SOCIETY: usize = 46;

/// A client with a player who has a name and a body.
fn a_client_with_a_named_player() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    let player = ObjectId(0x5000_000A);
    {
        let w = c.world_mut();
        w.player = Some(player);
        let mut wn = dereth_client_model::Weenie::new(player);
        wn.pwd.name = "Lark".to_owned();
        wn.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
        w.tables.weenies.insert(player, wn);
    }
    c
}

/// The squelch row of the talk-to menu is a toggle: it silences whoever the player was last
/// talking to, and un-silences him if he already was. With nobody to talk to it asks nothing.
pub fn the_squelch_row_is_a_toggle_and_names_the_speaker() {
    let target = ObjectId(0x5000_1234);
    let mut c = a_client_with_a_named_player();
    c.when(Player::ui(UiRequest::ToggleCharacterSquelch(ObjectId(0))));
    c.when(Player::ui(UiRequest::ToggleCharacterSquelch(target)));
    let missing_is_inert = c.outbound().is_empty();
    let mut object = dereth_client_model::Weenie::new(target);
    object.pwd.name = "Alba".into();
    c.world_mut().tables.weenies.insert(target, object);

    // A namesake and an account matching the displayed name do not identify this character.
    // A partial squelch on the right id also does not mean all message types are squelched.
    c.when(Inbound::message(&squelch_db(WireDb {
        account_hash: packed(vec![("Alba".into(), 1)]),
        character_hash: packed(vec![
            (target.0 + 1, squelched("Alba", false)),
            (target.0, squelch_info(&[text_type::SPEECH], "Alba")),
        ]),
        global_squelch_info: squelch_info(&[], ""),
    })));
    c.when(Player::ui(UiRequest::ToggleCharacterSquelch(target)));
    let sent = c.outbound().to_vec();
    let [Request::ModifyCharacterSquelch(add)] = sent.as_slice() else {
        panic!("one character-squelch message, got {sent:?}");
    };
    let body = dereth_protocol::write_body(add).expect("the body encodes");
    let adds_by_id = body == vec![1, 0, 0, 0, 0x34, 0x12, 0, 0x50, 0, 0, 0, 0, 1, 0, 0, 0];
    let waits_for_server = !c
        .view()
        .world()
        .chat
        .is_squelched(target, "", text_type::ALL_CHANNELS);

    // The server's new list, not a stale displayed name or cached UI flag, decides the next click.
    c.when(Inbound::message(&squelch_db(WireDb {
        account_hash: packed(vec![]),
        character_hash: packed(vec![(target.0, squelched("A different name", false))]),
        global_squelch_info: squelch_info(&[], ""),
    })));
    c.when(Player::ui(UiRequest::ToggleCharacterSquelch(target)));
    let sent = c.outbound().to_vec();
    let [_, Request::ModifyCharacterSquelch(remove)] = sent.as_slice() else {
        panic!("add then remove, got {sent:?}");
    };
    let undo = dereth_protocol::write_body(remove).expect("the body encodes");
    let removes_by_id = undo[..4] == [0, 0, 0, 0] && undo[4..] == body[4..];
    let counted = c.view().interaction().stats.squelch_requests == 2;
    let removal_waits_for_server =
        c.view()
            .world()
            .chat
            .is_squelched(target, "", text_type::ALL_CHANNELS);
    c.world_mut().tables.weenies.remove(target);
    c.when(Player::ui(UiRequest::ToggleCharacterSquelch(target)));
    let vanished_is_inert = c.outbound().len() == 2;

    c.assert_behaviour(
        "chat.talk-to-menu.the-squelch-row-is-a-toggle-and-its-message-names-the-speaker",
        move |_| {
            missing_is_inert
                && adds_by_id
                && waits_for_server
                && removes_by_id
                && removal_waits_for_server
                && counted
                && vanished_is_inert
        },
    );
}

#[test]
fn scenario_the_squelch_row_is_a_toggle_and_names_the_speaker() {
    scenario("the_squelch_row_is_a_toggle_and_names_the_speaker");
}

/// Who the player is talking to follows what he has selected, while that is near him -- and is
/// let go of when it is not.
pub fn the_chat_target_follows_what_is_selected_while_it_is_near() {
    use dereth_client_contract::chat::mainchat::AutoTargetWorld;

    let player = ObjectId(0x5000_000A);
    let npc = ObjectId(0x5000_1111);
    let mut c = HeadlessClient::model();
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::PlayerCreated(player),
    ));
    {
        let w = c.world_mut();
        w.player = Some(player);
        for (id, name) in [(player, "Lark"), (npc, "Ulgrim")] {
            let mut wn = dereth_client_model::Weenie::new(id);
            wn.pwd.name = name.to_owned();
            wn.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
            w.tables.weenies.insert(id, wn);
        }
    }

    // Nothing selected: nothing to adopt.
    let empty = c.view().hud().auto_target_world(c.view().world());
    fn sweep(c: &mut HeadlessClient, now: f64, facts: &AutoTargetWorld) -> Option<ObjectId> {
        let (interaction, world) = c.interaction_and_world_mut();
        interaction.update_chat_target(world, now, facts);
        world.chat.last_speakable_target
    }
    let knows_the_player = empty.player_id == player.0 && empty.selected_id == 0;
    let nothing_yet = sweep(&mut c, 0.0, &empty).is_none();

    // Selected, but not near: not adopted.
    c.world_mut().selected = Some(npc);
    let selected = c.view().hud().auto_target_world(c.view().world());
    let named = selected.selected_id == npc.0
        && selected.selected_name == "Ulgrim"
        && selected.in_range_of_player.is_empty()
        && selected.selected_talkable;
    let not_near = sweep(&mut c, 1.0, &selected).is_none();

    // Near, and the player has nobody yet: adopted.
    let near = AutoTargetWorld {
        in_range_of_player: vec![npc.0],
        ..selected.clone()
    };
    let adopted = sweep(&mut c, 2.0, &near) == Some(npc);

    // Kept while he stays near, and let go of when he goes.
    let kept = sweep(&mut c, 3.0, &near) == Some(npc);
    let gone = AutoTargetWorld {
        in_range_of_player: Vec::new(),
        ..near.clone()
    };
    let let_go = sweep(&mut c, 4.0, &gone).is_none();

    c.assert_behaviour(
        "chat.talk-to-menu.the-chat-target-follows-what-is-selected-while-it-is-near",
        move |_| knows_the_player && nothing_yet && named && not_near && adopted && kept && let_go,
    );
}

#[test]
fn scenario_the_chat_target_follows_what_is_selected_while_it_is_near() {
    scenario("the_chat_target_follows_what_is_selected_while_it_is_near");
}

/// A line typed to "Tell to <name>" goes to the chat target the talk-to menu names, not to what
/// happens to be selected; with no chat target it goes nowhere. The squelch row asks about the
/// same target.
pub fn a_tell_to_the_chat_target_goes_to_it_and_not_to_the_selection() {
    use dereth_ui_screens::chat::mainchat::MainChatPanel;

    let npc = ObjectId(0x5000_1111);
    let door = ObjectId(0x7000_2222);

    let mut c = a_client_with_a_named_player();
    let mut object = dereth_client_model::Weenie::new(npc);
    object.pwd.name = "Ulgrim".into();
    object.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
    c.world_mut().tables.weenies.insert(npc, object);
    let facts = dereth_client_contract::chat::mainchat::AutoTargetWorld {
        selected_id: npc.0,
        selected_name: "Ulgrim".into(),
        selected_talkable: true,
        in_range_of_player: vec![npc.0],
        ..Default::default()
    };
    let (interaction, world) = c.interaction_and_world_mut();
    interaction.update_chat_target(world, 0.0, &facts);
    let recorded = world.chat.last_speakable_target == Some(npc);
    let mut ui = dereth_ui::UiSystem::new((800, 600));
    let mut menu = MainChatPanel::default();
    menu.project_communication(&mut ui, &c.view().hud().chat_focus_view(c.view().world()));
    let told = menu.last_speakable_target == npc.0 && ui.requests.take().is_empty();
    // The player has since selected a door; the menu still says Ulgrim, so the line is his.
    c.world_mut().selected = Some(door);
    c.when(Player::ui(UiRequest::SetTalkFocus { focus: 2 }));
    let sent = type_line(&mut c, "well met");
    let to_the_target = matches!(
        sent.as_slice(),
        [Request::TalkDirect(m)] if m.target == npc && m.message == "well met"
    );
    // The squelch row reads the same target: Ulgrim silenced, the host says so.
    let mut everything = SquelchEntry::default();
    everything.squelch_everything();
    c.world_mut()
        .chat
        .squelch
        .characters
        .insert(npc, everything);
    let squelched_seen = {
        let w = c.view().world();
        w.chat
            .last_speakable_target
            .is_some_and(|t| w.chat.is_squelched(t, "", 1))
    };

    // A selected-target command without a remembered recipient never becomes public speech.
    c.world_mut().chat.set_speakable_target(None, false);
    menu.project_communication(&mut ui, &c.view().hud().chat_focus_view(c.view().world()));
    let cleared =
        c.view().world().chat.last_speakable_target.is_none() && menu.last_speakable_target == 0;
    c.when(Player::ui(UiRequest::SetTalkFocus { focus: 2 }));
    let nowhere = type_line(&mut c, "anyone?").is_empty();

    c.assert_behaviour(
        "chat.talk-to-menu.a-tell-goes-to-the-chat-target-and-not-to-the-selection",
        move |_| told && recorded && to_the_target && squelched_seen && cleared && nowhere,
    );
}

#[test]
fn scenario_a_tell_to_the_chat_target_goes_to_it_and_not_to_the_selection() {
    scenario("a_tell_to_the_chat_target_goes_to_it_and_not_to_the_selection");
}

/// The channel rows of the talk-to menu follow the chat-room service and the player's own options,
/// and the two rows that are not channels are never touched by either.
pub fn the_channel_rows_follow_the_service_and_the_options() {
    let mut c = a_client_with_a_named_player();
    for opt in [
        HEAR_GENERAL,
        HEAR_TRADE,
        HEAR_LFG,
        HEAR_ROLEPLAY,
        HEAR_SOCIETY,
    ] {
        c.world_mut().player_system.options.set(opt, true);
    }

    // With the service not running, every channel row is shut whatever the options say.
    let not_running = !c.view().world().chat.using_turbine_chat;
    let off = c.world_mut().enable_chat_talk_focuses(true);
    let all_shut = [
        ROW_GENERAL,
        ROW_TRADE,
        ROW_LFG,
        ROW_ROLEPLAY,
        ROW_SOCIETY,
        ROW_OLTHOI,
    ]
    .into_iter()
    .all(|f| !off[f]);
    // And the two rows that are not channels are untouched, which is why the menu always offers
    // saying it aloud.
    let others_untouched = off[ROW_ALL] && off[ROW_SELECTED];

    // With it running, each row follows its own option -- and the one for the alien people follows
    // who the player is rather than an option at all.
    c.world_mut().chat.startup_turbine_chat();
    c.world_mut().player_system.options.set(HEAR_TRADE, false);
    let on = c.world_mut().enable_chat_talk_focuses(false);
    let each_follows = on[ROW_GENERAL]
        && !on[ROW_TRADE]
        && on[ROW_LFG]
        && on[ROW_ROLEPLAY]
        && on[ROW_SOCIETY]
        && !on[ROW_OLTHOI];
    c.world_mut().chat.using_turbine_chat = true;
    let olthoi_follows_who_he_is = c.world_mut().enable_chat_talk_focuses(true)[ROW_OLTHOI];

    c.assert_behaviour(
        "chat.talk-focus.the-channel-rows-follow-the-service-and-the-players-own-options",
        move |_| {
            not_running && all_shut && others_untouched && each_follows && olthoi_follows_who_he_is
        },
    );
}

#[test]
fn scenario_the_channel_rows_follow_the_service_and_the_options() {
    scenario("the_channel_rows_follow_the_service_and_the_options");
}

/// Every recorded login asks the client to use the chat-room service -- so a client whose channel
/// rows are shut is not one the shard said no to.
///
/// The recordings are walked rather than counted: how many there are is the corpus's business and
/// this claim is about all of them, whatever the number.
pub fn every_recorded_login_asks_for_the_room_service() {
    use dereth_client_net::client_session::testing::{Corpus, Direction};

    let mut seen = 0_usize;
    let mut holds = true;
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient
                || b.opcode != dereth_protocol::Opcode::LOGIN_LOGIN_CHARACTER_SET.0
            {
                continue;
            }
            let Ok(m) = dereth_protocol::read_body::<dereth_protocol::login::LoginCharacterSet>(
                &b.payload[4..],
            ) else {
                continue;
            };
            seen += 1;
            holds &= m.use_turbine_chat != 0 && m.has_throne_of_destiny == 1;
        }
    }
    assert!(seen > 0, "the recordings carry character sets to walk");
    let every = holds;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.turbine.every-recorded-login-asks-the-client-to-use-the-service",
        move |_| every,
    );
}

#[test]
fn scenario_every_recorded_login_asks_for_the_room_service() {
    scenario("every_recorded_login_asks_for_the_room_service");
}

// =============================================================================================
// How a spoken line and a private message are put together
// =============================================================================================
//
// The words the client puts around what somebody said -- the name, the verb, the comma and the
// quotes -- and which of them a clickable name goes in. Each of these asserts the whole line
// spelled out rather than built from the same pieces the client builds it from, so a line put
// together in the wrong order is a failure here rather than a match against itself.

/// A player, somebody who is not a player, and another player.
const A_PLAYER: u32 = 0x5000_1234;
const NOT_A_PLAYER: u32 = 0x8000_0DE9;
const ANOTHER_PLAYER: u32 = 0x5000_ABCD;

/// The newlines the client's own wording leaves on a line are taken off both ends -- and only the
/// newlines are.
pub fn the_newlines_are_trimmed_from_both_ends() {
    use dereth_client::chat::add_text_to_scroll_trim as trim;
    let holds = trim("Bob says, \"hi\"\n") == "Bob says, \"hi\""
        && trim("\nwelcome\n") == "welcome"
        && trim("\n\nwelcome\n\n") == "welcome"
        && trim("no newline") == "no newline"
        // It takes newlines and not whitespace: the spaces the shard and the timestamp put there
        // survive it.
        && trim(" spaced \n") == " spaced ";

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.log.the-newlines-are-trimmed-from-both-ends-and-nothing-else-is",
        move |_| holds,
    );
}

#[test]
fn scenario_the_newlines_are_trimmed_from_both_ends() {
    scenario("the_newlines_are_trimmed_from_both_ends");
}

/// A spoken line is the speaker's name, then the client's own verb, then what he said in quotes --
/// and the player's own line comes back to him in a different form again.
pub fn a_spoken_line_is_the_name_the_verb_and_the_words() {
    use dereth_client::chat::hear_speech_line as line;

    let remote = line(
        NOT_A_PLAYER,
        Some(A_PLAYER),
        "Sparring Golem",
        "Have at you!",
    ) == "Sparring Golem says, \"Have at you!\"";
    // The player's own words come back to him in a form of their own.
    let echo = line(A_PLAYER, Some(A_PLAYER), "Lark", "W") == "You say, \"W\"";
    // Another player's name is something he can click, and the form that makes it clickable
    // carries who that player is.
    let clickable = line(ANOTHER_PLAYER, Some(A_PLAYER), "Alba", "hello")
        == format!("<Tell:IIDString:{ANOTHER_PLAYER}:Alba>Alba<\\Tell> says, \"hello\"");
    // Before the shard has said who the player is, nothing can be his own line -- and that is a
    // different thing from his being somebody whose id happens to be nought.
    let no_player_yet =
        line(NOT_A_PLAYER, None, "Sparring Golem", "hi") == "Sparring Golem says, \"hi\"";

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.speech.a-spoken-line-is-the-name-the-verb-and-the-words-in-quotes",
        move |_| remote && echo && clickable && no_player_yet,
    );
}

#[test]
fn scenario_a_spoken_line_is_the_name_the_verb_and_the_words() {
    scenario("a_spoken_line_is_the_name_the_verb_and_the_words");
}

/// A private message is drawn only when it was addressed to the player, one he sent to himself is
/// drawn as a thought, and one he merely overheard draws nothing at all.
pub fn a_private_message_is_drawn_only_when_it_was_for_you() {
    use dereth_client::chat::hear_direct_speech_line as line;
    const SOMEBODY_ELSE: u32 = 0x5000_9999;
    const AN_NPC: u32 = 0x77F0_0042;

    let to_you = line(
        AN_NPC,
        A_PLAYER,
        Some(A_PLAYER),
        "Academy Researcher",
        "Welcome.",
    ) == Some("Academy Researcher tells you, \"Welcome.\"".to_owned());
    // To himself: a thought, and the order of the two tests is what decides it.
    let to_yourself = line(A_PLAYER, A_PLAYER, Some(A_PLAYER), "Lark", "hmm")
        == Some("You think, \"hmm\"".to_owned());
    // Copied to him but addressed elsewhere: nothing at all.
    let overheard = line(
        AN_NPC,
        SOMEBODY_ELSE,
        Some(A_PLAYER),
        "Academy Researcher",
        "Hi.",
    )
    .is_none();
    let from_a_player = line(ANOTHER_PLAYER, A_PLAYER, Some(A_PLAYER), "Alba", "yo")
        == Some(format!(
            "<Tell:IIDString:{ANOTHER_PLAYER}:Alba>Alba<\\Tell> tells you, \"yo\""
        ));

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.tell.a-private-message-is-drawn-only-when-it-was-addressed-to-you",
        move |_| to_you && to_yourself && overheard && from_a_player,
    );
}

#[test]
fn scenario_a_private_message_is_drawn_only_when_it_was_for_you() {
    scenario("a_private_message_is_drawn_only_when_it_was_for_you");
}

/// A line spoken with a range on it has no form of its own for the player's own words: his own
/// line comes back to him as any other speaker's would, with his name clickable.
pub fn a_line_with_a_range_on_it_has_no_echo_of_your_own() {
    use dereth_client::chat::{hear_ranged_speech_line as ranged, hear_speech_line as ordinary};

    let ordinary_echoes = ordinary(A_PLAYER, Some(A_PLAYER), "Lark", "W") == "You say, \"W\"";
    let ranged_does_not = ranged(A_PLAYER, "Lark", "W")
        == format!("<Tell:IIDString:{A_PLAYER}:Lark>Lark<\\Tell> says, \"W\"");
    let and_others_are_the_same =
        ranged(NOT_A_PLAYER, "Sparring Golem", "hi") == "Sparring Golem says, \"hi\"";

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.speech.a-line-with-a-range-on-it-has-no-echo-of-your-own",
        move |_| ordinary_echoes && ranged_does_not && and_others_are_the_same,
    );
}

#[test]
fn scenario_a_line_with_a_range_on_it_has_no_echo_of_your_own() {
    scenario("a_line_with_a_range_on_it_has_no_echo_of_your_own");
}

/// Only somebody who is a player gets a clickable name, and the range of who counts as one is
/// exclusive at both ends -- an error of one either way is a name that silently cannot be clicked,
/// or a creature that can.
pub fn only_a_player_has_a_clickable_name() {
    let clickable =
        |id: u32| dereth_client::chat::hear_speech_line(id, None, "X", "y").starts_with("<Tell:");
    let holds = !clickable(0x5000_0000)
        && clickable(0x5000_0001)
        && clickable(0x6FFF_FFFF)
        && !clickable(0x7000_0000);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.speech.only-a-player-has-a-clickable-name-and-the-range-is-exclusive-at-both-ends",
        move |_| holds,
    );
}

#[test]
fn scenario_only_a_player_has_a_clickable_name() {
    scenario("only_a_player_has_a_clickable_name");
}

/// Every overheard line the recordings carry, drawn with the client's own verb and quotes --
/// never the name and the words run together, which is the fault this comes from.
///
/// The recordings are walked rather than counted: how many there are is the corpus's business and
/// the claim is about all of them. **They carry no private messages at all**, and that is asserted
/// rather than passed over: what one player says to another is private and the public recordings
/// are scrubbed of it, so the loop for them is a standing check that stays right if that changes.
pub fn every_recorded_spoken_line_is_drawn_with_the_verb() {
    use dereth_client_net::client_session::testing::{Corpus, Direction};

    let mut tells = 0_usize;
    let mut says = (0_usize, 0_usize);
    let mut every = true;
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient || b.payload.len() < 4 {
                continue;
            }
            if b.opcode == dereth_protocol::Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH.0 {
                let Ok(m) = dereth_protocol::read_body::<
                    dereth_protocol::comms::CommunicationHearDirectSpeech,
                >(&b.payload[4..]) else {
                    continue;
                };
                let line = dereth_client::chat::hear_direct_speech_line(
                    m.sender_id.0,
                    m.target_id.0,
                    Some(m.target_id.0),
                    &m.sender_name,
                    &m.message,
                )
                .expect("one addressed to the player is drawn");
                let want = if dereth_client::chat::CLICKABLE_PLAYER_IDS.contains(&m.sender_id.0) {
                    format!(
                        "<Tell:IIDString:{}:{n}>{n}<\\Tell> tells you, \"{}\"",
                        m.sender_id.0,
                        m.message,
                        n = m.sender_name
                    )
                } else {
                    format!("{} tells you, \"{}\"", m.sender_name, m.message)
                };
                let holds = line == want
                    && line.ends_with('"')
                    && line != format!("{}{}", m.sender_name, m.message);
                assert!(holds, "a recorded message from {:?}", m.sender_name);
                every &= holds;
                tells += 1;
            } else if b.opcode == dereth_protocol::Opcode::COMMUNICATION_HEAR_SPEECH.0 {
                let Ok(m) = dereth_protocol::read_body::<
                    dereth_protocol::comms::CommunicationHearSpeech,
                >(&b.payload[4..]) else {
                    continue;
                };
                let line = dereth_client::chat::hear_speech_line(
                    m.sender_id.0,
                    None,
                    &m.sender_name,
                    &m.message,
                );
                // Both forms are counted apart, because one check that only asked whether the
                // words were in there would have passed on either.
                let holds = if dereth_client::chat::CLICKABLE_PLAYER_IDS.contains(&m.sender_id.0) {
                    says.1 += 1;
                    line == format!(
                        "<Tell:IIDString:{}:{n}>{n}<\\Tell> says, \"{}\"",
                        m.sender_id.0,
                        m.message,
                        n = m.sender_name
                    )
                } else {
                    says.0 += 1;
                    line == format!("{} says, \"{}\"", m.sender_name, m.message)
                };
                assert!(holds, "a recorded line from {:?}", m.sender_name);
                every &= holds;
            }
        }
    }
    // **The recordings carry no private messages**, and that is not an oversight: what one player
    // says to another is private and the public recordings are scrubbed of it. So the loop above
    // walks whatever is there -- and what is there is the spoken lines, in both of their forms.
    let both = tells == 0 && says.0 > 0 && says.1 > 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.speech.every-recorded-spoken-line-is-drawn-with-the-clients-own-verb-and-quotes",
        move |_| every && both,
    );
}

#[test]
fn scenario_every_recorded_spoken_line_is_drawn_with_the_verb() {
    scenario("every_recorded_spoken_line_is_drawn_with_the_verb");
}

// =============================================================================================
// 19. The tell on the wire -- who a typed line is addressed to
// =============================================================================================
//
// The subject is the one thing on the chat path that can go badly in a way nobody sees: *a private
// message delivered to the wrong player is invisible on the sender's own screen and irreversible
// for the recipient*.
// So every claim here about **who** a line is addressed to has a recording for an oracle rather
// than a written one, and both directions of every count are measured -- a build in which no tell
// was composed at all would satisfy "none went to the wrong person" on its own.
//
// `interaction::use_time` takes a `&RetailDatStore` for the pick, but nothing on the chat path
// reads it, and the model backend runs a typed line through `Interaction::run_ui_requests` -- the
// same arm -- so these are cpu scenarios and open no dat at all.
//
// **Every count is the corpus's own.** No integer is pinned; each is read off the recordings, so a
// promoted recording re-measures them.

/// Every client-to-server game action of sub-type `sub` the recordings carry, as
/// `(recording, blob)` with the blob starting at its `0xF7B1` opcode dword.
///
/// The `0xF7B1` envelope is `[opcode][stamp][sub-type]`, so the body begins at byte 12 and the
/// stamp -- the one field this client cannot reproduce, because it counts the actions *this*
/// connection has sent -- is the dword at byte 4.
fn recorded_actions(sub: u32) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for corpus in dereth_client_net::client_session::testing::Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != dereth_client_net::client_session::testing::Direction::ClientToServer
                || b.opcode != 0xF7B1
            {
                continue;
            }
            if b.payload.len() >= 12 && action_sub(&b.payload) == sub {
                out.push((corpus.name.clone(), b.payload.clone()));
            }
        }
    }
    out
}

/// Every private message the recordings carry. The decoded corpus carries none -- the derivation
/// that builds it leaves them out -- so these come off the datagrams, through the chat adapter.
fn recorded_tells() -> &'static [(String, Vec<u8>)] {
    dereth_testkit::adapters_chat::recorded_private_messages()
}

fn dword_at(blob: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([blob[at], blob[at + 1], blob[at + 2], blob[at + 3]])
}

/// The sub-type of a `0xF7B1` game action.
fn action_sub(blob: &[u8]) -> u32 {
    dword_at(blob, 8)
}

fn decode_recorded_tell(blob: &[u8]) -> comms::CommunicationHearDirectSpeech {
    let mut r = dereth_protocol::archive::Reader::new(blob.get(4..).unwrap_or_default());
    <comms::CommunicationHearDirectSpeech as dereth_protocol::Message>::read(&mut r)
        .expect("a recorded 0x02BD decodes")
}

/// `senderID` sits 16 bytes from the end of a `0x02BD` and `targetID` 12: the message's last four
/// fields are `senderID`, `targetID`, `type`, `secretFlags`, all `u32`.
const SENDER_ID_FROM_END: usize = 16;

/// The same recorded line with its two ids replaced -- which is the only way to get a recording of
/// a line *the player standing here* was sent, out of recordings of lines sent to somebody else.
fn with_ids(blob: &[u8], sender: ObjectId, target: ObjectId) -> Vec<u8> {
    let mut out = blob.to_vec();
    let at = out.len() - SENDER_ID_FROM_END;
    out[at..at + 4].copy_from_slice(&sender.0.to_le_bytes());
    out[at + 4..at + 8].copy_from_slice(&target.0.to_le_bytes());
    out
}

/// A narrow pstring as it appears on the wire, without the trailing alignment padding: a
/// `u16` character count and then the characters. This is the part of a pstring whose bytes do not
/// depend on where in the blob it sits, which is why the comparisons below use it.
fn pstring_head(s: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len() + 2);
    let n = u16::try_from(s.len()).expect("a name shorter than 64K");
    v.extend_from_slice(&n.to_le_bytes());
    v.extend_from_slice(s.as_bytes());
    v
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// One request through the one place a request becomes bytes, answering the action stamp the
/// session gave it and the blob it produced.
fn on_the_wire(r: &Request) -> (u32, Vec<u8>) {
    let mut s = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );
    let stamp = s.next_action_stamp();
    assert!(
        dereth_client::interaction::send_request(&mut s, r),
        "{r:?} would not send"
    );
    let sent = s.transport.sent.last().expect("one blob").clone();
    assert_eq!(
        sent.queue,
        dereth_primitives::NetQueue::Weenie,
        "a game action rides the Weenie queue"
    );
    assert!(sent.ordered, "and is marked ordered");
    (stamp, sent.payload)
}

/// A client that is logged in and can type: the command interpreter reads the described flag, and
/// a client that had never received its own description would refuse every line for that reason
/// rather than for the one under test.
fn a_talker() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    c.hud_mut().player_desc_received = true;
    c
}

/// A talker who is `who`, and whose body the speech handlers can see -- which is the gate
/// the direct-speech handler opens with, above the producer that stores a reply
/// target.
fn a_talker_who_is(who: ObjectId) -> HeadlessClient {
    let mut c = a_talker();
    c.hud_mut().player = Some(who);
    c.hud_mut().player_body = true;
    c
}

/// One line typed into a chat window and sent, through the command interpreter. Answers the
/// requests it put in the outbox.
fn type_line(c: &mut HeadlessClient, text: &str) -> Vec<Request> {
    c.interaction_mut().last_refusal = None;
    let before = c.outbound().len();
    c.when(Player::ui(UiRequest::ChatLine {
        text: text.to_owned(),
        window: 8,
    }));
    c.outbound()[before..].to_vec()
}

/// Every spoken line the recorded client sent is rebuilt by this client's own writer, byte for
/// byte. This is the calibration for every byte claim below: before believing a tell this build
/// composes, the same writer has to reproduce a blob somebody recorded.
pub fn every_recorded_spoken_line_is_rebuilt_byte_for_byte() {
    let talk = recorded_actions(0x0015);
    assert!(!talk.is_empty(), "the recordings carry spoken lines");
    let mut rebuilt = 0usize;
    let mut lengths: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    for (name, blob) in &talk {
        let mut r = dereth_protocol::archive::Reader::new(&blob[12..]);
        let m = <comms::CommunicationTalk as dereth_protocol::Message>::read(&mut r)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        lengths.insert(m.message.len());
        let (stamp, ours) = on_the_wire(&Request::Talk(m.clone()));
        let mut expected = blob.clone();
        expected[4..8].copy_from_slice(&stamp.to_le_bytes());
        assert_eq!(ours, expected, "{name}: {:?}", m.message);
        rebuilt += 1;
    }
    // The oracle is worth what its spread is worth: a writer that padded a pstring wrongly would
    // pass on one length and fail on another, so the count of distinct body lengths is asserted
    // rather than described.
    let spread = lengths.len() > 3;
    let all = rebuilt == talk.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.speech.every-spoken-line-the-recorded-client-sent-is-rebuilt-byte-for-byte",
        move |_| all && spread,
    );
}

#[test]
fn scenario_every_recorded_spoken_line_is_rebuilt_byte_for_byte() {
    scenario("every_recorded_spoken_line_is_rebuilt_byte_for_byte");
}

/// Every private message the recorded client sent is rebuilt from the same line typed into this
/// client, byte for byte -- the whole path, from the characters a player pressed to the blob.
pub fn every_recorded_tell_is_rebuilt_from_a_typed_line() {
    let tells = recorded_actions(0x005D);
    assert!(
        !tells.is_empty(),
        "the recordings carry a private message the client sent"
    );
    let mut rebuilt = 0usize;
    for (name, blob) in &tells {
        let mut r = dereth_protocol::archive::Reader::new(&blob[12..]);
        let m = <comms::CommunicationTalkDirectByName as dereth_protocol::Message>::read(&mut r)
            .unwrap_or_else(|e| panic!("{name}: {e}"));

        let mut c = a_talker();
        let sent = type_line(&mut c, &format!("@tell {}, {}", m.target_name, m.message));
        assert_eq!(sent.len(), 1, "{name}: one line, one game action: {sent:?}");
        let Request::TalkDirectByName(out) = &sent[0] else {
            panic!(
                "{name}: a tell is the by-name tell action, got {:?}",
                sent[0]
            )
        };
        assert_eq!(out.target_name, m.target_name, "{name}");
        assert_eq!(out.message, m.message, "{name}");
        assert_eq!(c.view().interaction().stats.tells_sent, 1, "{name}");

        let (stamp, ours) = on_the_wire(&sent[0]);
        let mut expected = blob.clone();
        expected[4..8].copy_from_slice(&stamp.to_le_bytes());
        assert_eq!(
            ours, expected,
            "{name}: ours {ours:02X?} vs the recording {blob:02X?}"
        );
        rebuilt += 1;
    }
    let all = rebuilt == tells.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.tell.every-recorded-private-message-is-rebuilt-from-the-same-typed-line",
        move |_| all,
    );
}

#[test]
fn scenario_every_recorded_tell_is_rebuilt_from_a_typed_line() {
    scenario("every_recorded_tell_is_rebuilt_from_a_typed_line");
}

/// A typed private message reaches the wire with the **words first** and the name after, and the
/// words are encoded in the bytes the recorded client used for the same text.
pub fn a_typed_tell_goes_out_with_the_words_first() {
    // The longest line the recorded client ever spoke, found by walking rather than named here, so
    // that the pstring being compared is the widest one the recordings can offer.
    let (session, recorded, text) = recorded_actions(0x0015)
        .into_iter()
        .map(|(name, blob)| {
            let mut r = dereth_protocol::archive::Reader::new(&blob[12..]);
            let m = <comms::CommunicationTalk as dereth_protocol::Message>::read(&mut r)
                .expect("a recorded 0x0015 decodes");
            (name, blob, m.message)
        })
        .max_by_key(|(_, _, message)| message.len())
        .expect("the recordings carry spoken lines");

    let mut c = a_talker();
    let sent = type_line(&mut c, &format!("@tell Alba, {text}"));
    assert_eq!(sent.len(), 1, "one line, one game action: {sent:?}");
    let Request::TalkDirectByName(m) = &sent[0] else {
        panic!("a tell is the by-name tell action, got {:?}", sent[0])
    };
    let composed = m.message == text
        && m.target_name == "Alba"
        && c.view().interaction().stats.tells_sent == 1;

    let (_, blob) = on_the_wire(&sent[0]);
    let head = pstring_head(&text);
    // The message is the first field: its pstring head starts at byte 12, exactly where the
    // recorded spoken line's does, and the bytes are identical.
    let words_first = dword_at(&blob, 8) == 0x005D
        && blob[12..12 + head.len()] == head[..]
        && recorded[12..12 + head.len()] == head[..];
    // The name is the second field, and it is somewhere after the message.
    let name_after = contains(&blob[12 + head.len()..], &pstring_head("Alba"));
    assert!(words_first && name_after, "{session}: {blob:02X?}");

    c.assert_behaviour(
        "chat.tell.a-typed-private-message-goes-out-with-the-words-first-and-the-name-after",
        move |_| composed && words_first && name_after,
    );
}

#[test]
fn scenario_a_typed_tell_goes_out_with_the_words_first() {
    scenario("a_typed_tell_goes_out_with_the_words_first");
}

/// All ten ways of writing a private message reach the wire, and the three verbs behind them are
/// three different messages: five spellings of "tell" and the two that repeat it go out addressed
/// by name, and the three spellings of "reply" go out addressed by id.
pub fn every_way_of_writing_a_private_message_reaches_the_wire() {
    let teller = ObjectId(0x5000_0AB1);
    let mut by_name = 0usize;
    let mut by_id = 0usize;

    for verb in ["tell", "t", "send", "whisper", "w"] {
        let mut c = a_talker();
        let sent = type_line(&mut c, &format!("@{verb} Alba, hi"));
        assert_eq!(sent.len(), 1, "@{verb}");
        let (_, blob) = on_the_wire(&sent[0]);
        assert_eq!(
            dword_at(&blob, 8),
            0x005D,
            "@{verb} is the by-name tell action"
        );
        assert!(
            matches!(&sent[0], Request::TalkDirectByName(m) if m.target_name == "Alba"),
            "@{verb}: {:?}",
            sent[0]
        );
        by_name += 1;
    }

    for verb in ["reply", "r", "rp"] {
        let mut c = a_talker();
        c.world_mut().chat.last_teller = Some(teller);
        let sent = type_line(&mut c, &format!("@{verb} hi"));
        assert_eq!(sent.len(), 1, "@{verb}");
        let (_, blob) = on_the_wire(&sent[0]);
        assert_eq!(
            dword_at(&blob, 8),
            0x0032,
            "@{verb} is the by-id tell action"
        );
        assert!(
            matches!(&sent[0], Request::TalkDirect(m) if m.target == teller),
            "@{verb}: {:?}",
            sent[0]
        );
        by_id += 1;
    }

    for verb in ["retell", "rt"] {
        let mut c = a_talker();
        c.world_mut().chat.last_tellee_name = "Alba".to_owned();
        let sent = type_line(&mut c, &format!("@{verb} hi"));
        assert_eq!(sent.len(), 1, "@{verb}");
        let (_, blob) = on_the_wire(&sent[0]);
        assert_eq!(
            dword_at(&blob, 8),
            0x005D,
            "@{verb} is the by-name tell action"
        );
        assert!(
            matches!(&sent[0], Request::TalkDirectByName(m) if m.target_name == "Alba"),
            "@{verb}: {:?}",
            sent[0]
        );
        by_name += 1;
    }

    let all_ten = by_name == 7 && by_id == 3;
    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.tell.every-way-of-writing-a-private-message-reaches-the-wire",
        move |_| all_ten,
    );
}

#[test]
fn scenario_every_way_of_writing_a_private_message_reaches_the_wire() {
    scenario("every_way_of_writing_a_private_message_reaches_the_wire");
}

/// The name a private message goes out to is the name the line asked for, in the shard's own
/// bytes, and nobody else's name is anywhere in the blob. The oracle is every speaker the
/// recordings carry: each name is taken off the wire, typed back into a line, and required to come
/// out the same.
pub fn the_name_on_the_wire_is_the_one_the_line_asked_for() {
    let tells = recorded_tells();
    assert!(
        !tells.is_empty(),
        "the recordings carry lines spoken to somebody"
    );
    let mut composed = 0usize;
    let mut distinct: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut c = a_talker();

    for (session, blob) in tells {
        let m = decode_recorded_tell(blob);
        assert!(
            !m.sender_name.is_empty(),
            "{session}: a recorded speaker has a name"
        );
        *distinct.entry(m.sender_name.clone()).or_default() += 1;

        let sent = type_line(&mut c, &format!("@tell {}, hi", m.sender_name));
        assert_eq!(sent.len(), 1, "{session}: one tell for one line: {sent:?}");
        let Request::TalkDirectByName(out) = &sent[0] else {
            panic!("{session}: {:?}", sent[0])
        };
        assert_eq!(
            out.target_name, m.sender_name,
            "{session}: the recipient is the typed name"
        );
        assert_eq!(out.message, "hi");

        let (_, wire) = on_the_wire(&sent[0]);
        let head = pstring_head(&m.sender_name);
        assert!(
            contains(&wire, &head),
            "{session}: our blob carries {:?}",
            m.sender_name
        );
        assert!(
            contains(blob, &head),
            "{session}: and those are the bytes the shard used for the same name"
        );
        // Nobody else the recordings name is anywhere on this line.
        for other in distinct.keys().filter(|k| *k != &m.sender_name) {
            assert!(
                !contains(&wire, &pstring_head(other)),
                "{session}: {other:?} must not appear in a line addressed to {:?}",
                m.sender_name
            );
        }
        composed += 1;
    }

    // Both directions. `composed == tells.len()` is what says the arm ran at all; without it "none
    // went to the wrong name" is what a build that sends nothing would answer.
    let every = composed == tells.len();
    let several = distinct.len() > 1;
    let counted = c.view().interaction().stats.tells_sent == composed as u64
        && c.view().interaction().stats.chat_commands_refused == 0;

    c.assert_behaviour(
        "chat.tell.the-name-on-the-wire-is-the-one-the-line-asked-for-and-no-other",
        move |_| every && several && counted,
    );
}

#[test]
fn scenario_the_name_on_the_wire_is_the_one_the_line_asked_for() {
    scenario("the_name_on_the_wire_is_the_one_the_line_asked_for");
}

/// A reply goes to the id the client stored when somebody wrote to it, and to nothing at all when
/// nobody has. Every recorded line spoken to somebody is driven both ways: as recorded, where the
/// speaker is not somebody a player may reply to and the client says so, and with the two ids
/// moved so that the line was addressed to the player standing here, where the reply goes back to
/// that speaker and to no other.
pub fn a_reply_goes_to_whoever_last_wrote_to_you() {
    let tells = recorded_tells();
    assert!(
        !tells.is_empty(),
        "the recordings carry lines spoken to somebody"
    );
    let me = ObjectId(0x5000_0001);
    let a_player = |id: ObjectId| (0x5000_0001..0x7000_0000).contains(&id.0);
    let hear = |blob: Vec<u8>| {
        Inbound::event(SessionEvent::UiEvent {
            opcode: dereth_protocol::Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
            blob,
        })
    };

    // --- as recorded: a line from somebody who is not a player stores nothing, and the reply that
    // follows it is refused in the client's own words rather than sent to nobody.
    let mut declined = 0usize;
    let mut from_players = 0usize;
    for (session, blob) in tells {
        let m = decode_recorded_tell(blob);
        if a_player(m.sender_id) {
            // The recorded player-to-player line: stored for the character it was addressed to,
            // and stored for nobody else -- which is the `targetID` gate, on a recording.
            let mut mine = a_talker_who_is(m.target_id);
            mine.when(hear(blob.to_vec()));
            assert_eq!(
                mine.view().world().chat.last_teller,
                Some(m.sender_id),
                "{session}: the line was addressed to this character"
            );
            let sent = type_line(&mut mine, "@r hi");
            assert_eq!(sent.len(), 1, "{session}: {sent:?}");
            let Request::TalkDirect(out) = &sent[0] else {
                panic!("{session}: {:?}", sent[0])
            };
            assert_eq!(
                out.target, m.sender_id,
                "{session}: the reply goes back to the speaker"
            );
            let (_, wire) = on_the_wire(&sent[0]);
            assert_eq!(dword_at(&wire, 8), 0x0032);
            assert!(
                contains(&wire, &m.sender_id.0.to_le_bytes()),
                "{session}: {wire:02X?}"
            );

            let mut other = a_talker_who_is(me);
            other.when(hear(blob.to_vec()));
            assert_eq!(
                other.view().world().chat.last_teller,
                None,
                "{session}: a line addressed to somebody else stores nothing here"
            );
            assert!(type_line(&mut other, "@r hi").is_empty(), "{session}");
            from_players += 1;
            continue;
        }
        let mut c = a_talker_who_is(me);
        c.when(hear(blob.to_vec()));
        assert_eq!(
            c.view().world().chat.last_teller,
            None,
            "{session}: nothing stored"
        );
        let sent = type_line(&mut c, "@r hi");
        assert!(
            sent.is_empty(),
            "{session}: no reply target, no packet: {sent:?}"
        );
        assert_eq!(
            c.view().interaction().last_refusal.as_deref(),
            Some(dereth_client_model::chat::SOMEONE_MUST_TELL_YOU_FIRST),
            "{session}: and the client says so"
        );
        declined += 1;
    }

    // --- every recorded line, with the speaker moved into the range a player's id lives in and the
    // line re-addressed to the player standing here. A distinct id per line, so a target that
    // leaked from the line before shows up as the wrong id rather than as a passing run.
    let mut stored = 0usize;
    let mut addressed: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for (i, (session, blob)) in tells.iter().enumerate() {
        let sender = ObjectId(0x5000_1000 + u32::try_from(i).expect("the corpus is small"));
        let edited = with_ids(blob, sender, me);
        let differing = blob.iter().zip(&edited).filter(|(a, b)| a != b).count();
        assert!(
            differing <= 8,
            "{session}: the rewrite touches at most the two ids"
        );
        assert_eq!(
            edited.len(),
            blob.len(),
            "{session}: and changes no lengths"
        );

        let mut c = a_talker_who_is(me);
        c.when(hear(edited));
        assert_eq!(
            c.view().world().chat.last_teller,
            Some(sender),
            "{session}: the client stored the speaker"
        );
        let sent = type_line(&mut c, "@r hi");
        assert_eq!(sent.len(), 1, "{session}: {sent:?}");
        let Request::TalkDirect(out) = &sent[0] else {
            panic!("{session}: {:?}", sent[0])
        };
        assert_eq!(
            out.target, sender,
            "{session}: the reply goes to the speaker"
        );
        assert_ne!(out.target, me, "{session}: and never to ourselves");
        assert_ne!(out.target.0, 0, "{session}: and never to nobody");
        assert_eq!(out.message, "hi");
        let (_, wire) = on_the_wire(&sent[0]);
        assert_eq!(dword_at(&wire, 8), 0x0032);
        assert!(
            contains(&wire, &sender.0.to_le_bytes()),
            "{session}: {wire:02X?}"
        );
        addressed.insert(out.target.0);
        stored += 1;
    }

    let every_line = declined + from_players == tells.len();
    let some_of_each = declined > 0 && from_players > 0;
    let all_stored = stored == tells.len() && addressed.len() == tells.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.tell.a-reply-goes-to-whoever-last-wrote-to-you-and-nowhere-when-nobody-has",
        move |_| every_line && some_of_each && all_stored,
    );
}

#[test]
fn scenario_a_reply_goes_to_whoever_last_wrote_to_you() {
    scenario("a_reply_goes_to_whoever_last_wrote_to_you");
}

/// The mark a shard puts in front of the names on an account is taken off before the name goes out,
/// and nothing else is. The oracle is every name the recordings' own character listings carry:
/// the marked ones lose the mark, the unmarked ones are untouched, and the line that repeats a
/// private message re-uses the name as it went out rather than as it was typed.
pub fn the_mark_in_front_of_a_name_is_taken_off_before_it_goes_out() {
    let mut listed: Vec<String> = Vec::new();
    for corpus in dereth_client_net::client_session::testing::Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != dereth_client_net::client_session::testing::Direction::ServerToClient
                || b.opcode != dereth_protocol::Opcode::LOGIN_LOGIN_CHARACTER_SET.0
            {
                continue;
            }
            let mut r = dereth_protocol::archive::Reader::new(&b.payload[4..]);
            if let Ok(set) = <LoginCharacterSet as dereth_protocol::Message>::read(&mut r) {
                listed.extend(set.characters.iter().map(|ch| ch.name.clone()));
            }
            break;
        }
    }
    assert!(!listed.is_empty(), "the recordings reach character select");
    let (marked, bare): (Vec<String>, Vec<String>) =
        listed.iter().cloned().partition(|n| n.starts_with('+'));
    // Both halves have to exist for this to be a measurement: the marked ones are what the trim is
    // for, and the unmarked ones are the control that says it trims nothing else.
    let both_halves = !marked.is_empty() && !bare.is_empty();

    let mut c = a_talker();
    let mut checked = 0usize;
    for name in bare.iter().chain(&marked) {
        let sent = type_line(&mut c, &format!("@tell {name}, hi"));
        assert_eq!(sent.len(), 1, "{name}");
        let Request::TalkDirectByName(m) = &sent[0] else {
            panic!("{name}: {:?}", sent[0])
        };
        let want = name.trim_start_matches('+');
        assert_eq!(
            m.target_name, want,
            "{name}: the client sends the name without its mark"
        );
        assert!(!m.target_name.starts_with('+'), "{name}");
        // And the line that repeats it re-uses the name as it went out.
        assert_eq!(c.view().world().chat.last_tellee_name, want, "{name}");
        checked += 1;
    }
    let all = checked == listed.len();

    c.assert_behaviour(
        "chat.tell.the-mark-a-shard-puts-in-front-of-a-name-is-taken-off-before-it-goes-out",
        move |_| both_halves && all,
    );
}

#[test]
fn scenario_the_mark_in_front_of_a_name_is_taken_off_before_it_goes_out() {
    scenario("the_mark_in_front_of_a_name_is_taken_off_before_it_goes_out");
}

/// A private message the client will not send says so in the client's own words, on the feedback
/// channel, and sends nothing. Five lines, five refusals, each on its own path -- and the one line
/// that really does answer with nothing at all is counted separately, so that silence and a lost
/// line are two different readings.
pub fn every_refusal_is_the_clients_own_words() {
    use dereth_client_model::chat as strings;
    let cases: [(&str, &str, &str); 5] = [
        (
            "@tell Alba hi",
            strings::USE_COMMA_AFTER_THE_NAME,
            "no comma",
        ),
        (
            "@tell ,hi",
            strings::USE_COMMA_AFTER_THE_NAME,
            "a comma at index 0",
        ),
        (
            "@r",
            strings::YOU_MUST_SPECIFY_THE_TEXT,
            "a reply with no text",
        ),
        (
            "@r hi",
            strings::SOMEONE_MUST_TELL_YOU_FIRST,
            "a reply with nobody to reply to",
        ),
        (
            "@rt hi",
            strings::YOU_MUST_FIRST_PROVIDE_A_NAME,
            "a repeat with nobody to repeat to",
        ),
    ];
    let mut c = a_talker();
    let mut refused = 0usize;
    for (line, want, why) in cases {
        let sent = type_line(&mut c, line);
        assert!(
            sent.is_empty(),
            "{why}: nothing goes on the wire, got {sent:?}"
        );
        assert_eq!(
            c.view().interaction().last_refusal.as_deref(),
            Some(want),
            "{why}"
        );
        // The same string one link further along the client's own chain: the queue the chat
        // surface is handed.
        let pending = c.view().world().scroll.pending().to_vec();
        assert_eq!(
            pending.len(),
            1,
            "{why}: exactly one line reached the scroll"
        );
        assert_eq!(pending[0].body, want, "{why}");
        assert_eq!(pending[0].chat_type, 0x1A, "{why}: the feedback channel");
        c.world_mut().scroll.clear();
        refused += 1;
    }
    let said = refused == 5
        && c.view().interaction().stats.chat_commands_refused == 5
        && c.view().interaction().stats.notice_strings_scrolled == 5
        && c.view().interaction().stats.tells_sent == 0;

    // The one place the client answers with nothing at all, and it has a counter of its own.
    let sent = type_line(&mut c, "@rt");
    let silent = sent.is_empty()
        && c.view().interaction().last_refusal.is_none()
        && c.view().world().scroll.pending().is_empty()
        && c.view().interaction().stats.tells_dropped_silently == 1;

    c.assert_behaviour(
        "chat.tell.every-refusal-is-the-clients-own-words",
        move |_| said && silent,
    );
}

#[test]
fn scenario_every_refusal_is_the_clients_own_words() {
    scenario("every_refusal_is_the_clients_own_words");
}

/// A verb the client knows is answered by the client, and a word that is not a verb at all is
/// passed to the shard untouched. Four verbs that once fell through to the refusal are checked
/// from the other side, so that "nothing falls through" is a measurement and not a deletion, and
/// the one arm that really does refuse is the control that says the reader can still see it.
pub fn a_verb_the_client_knows_is_not_refused() {
    let mut c = a_talker();

    // The help verb is local: two lines on the scroll, nothing on the wire, no refusal.
    let sent = type_line(&mut c, "@help");
    let helped = sent.is_empty()
        && c.view().interaction().last_refusal.is_none()
        && c.view().interaction().stats.chat_commands_unimplemented == 0
        && {
            let pending = c.view().world().scroll.pending().to_vec();
            pending.len() == 2
                && pending[0].body.trim() == dereth_client_model::cmd::help::HELP_NOTE.trim()
                && pending[0].chat_type == 0
                && pending[1].body.starts_with("Available help:")
        };
    c.world_mut().scroll.clear();

    // The verb that asks before it acts: it raises its question and sends nothing until the answer.
    let died = type_line(&mut c, "@die").is_empty()
        && c.view().interaction().last_refusal.is_none()
        && c.view().interaction().stats.chat_commands_unimplemented == 0;

    // A listing is local, and is counted as the thing it is rather than as a fallthrough.
    let listed = type_line(&mut c, "@friends").is_empty()
        && c.view().interaction().stats.chat_commands_unimplemented == 0
        && c.view().interaction().stats.friends_listings == 1;

    // The verb that reads a position: with nothing to read it says nothing and refuses nothing,
    // and with an argument it says its own sentence -- either way it reached a handler.
    let located = type_line(&mut c, "@loc").is_empty()
        && c.view().interaction().last_refusal.is_none()
        && c.view().interaction().stats.chat_commands_unimplemented == 0;
    c.world_mut().scroll.clear();
    let argued = type_line(&mut c, "@loc here").is_empty() && {
        let pending = c.view().world().scroll.pending().to_vec();
        pending.len() == 1
            && pending[0].body == "Unexpected arguments to @loc"
            && pending[0].chat_type == 0x1A
            && c.view().interaction().stats.chat_commands_unimplemented == 0
    };
    c.world_mut().scroll.clear();

    // A word the client's table does not carry at all is forwarded to the shard verbatim, and is
    // not counted as a verb this build has not written.
    let sent = type_line(&mut c, "@acehelp");
    let forwarded = sent.len() == 1
        && matches!(&sent[0], Request::Talk(m) if m.message == "@acehelp")
        && c.view().interaction().stats.chat_commands_unimplemented == 0;

    // And the negative control, so that the zeroes above are a reading and not a reader that
    // cannot see: the one shipped arm that really does refuse still says the sentence.
    let refused = c.view().interaction().stats.chat_commands_refused;
    let control = type_line(&mut c, "@afk wibble").is_empty()
        && c.view().interaction().stats.chat_commands_refused == refused + 1
        && c.view()
            .world()
            .scroll
            .pending()
            .iter()
            .any(|l| l.body.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim());

    c.assert_behaviour(
        "chat.commands.a-verb-the-client-knows-answers-it-and-a-word-it-does-not-know-is-passed-on",
        move |_| helped && died && listed && located && argued && forwarded && control,
    );
}

#[test]
fn scenario_a_verb_the_client_knows_is_not_refused() {
    scenario("a_verb_the_client_knows_is_not_refused");
}

/// Neither safety gate would have stopped a line the recordings carry -- and the same two gates do
/// stop lines that are known bad, which is what makes the zero a measurement rather than an
/// instrument that has never answered anything else.
pub fn neither_safety_gate_would_have_stopped_a_recorded_line() {
    let mut lines: Vec<String> = Vec::new();
    for (name, blob) in recorded_actions(0x0015) {
        let mut r = dereth_protocol::archive::Reader::new(&blob[12..]);
        let m = <comms::CommunicationTalk as dereth_protocol::Message>::read(&mut r)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        lines.push(m.message);
    }
    // The recorded tells include long ones from non-player characters, which no gate ever sees;
    // only the ones short enough to have been typed are read, and the long ones are counted apart.
    let mut long_tells = 0;
    for (_, blob) in recorded_tells() {
        let message = decode_recorded_tell(blob).message;
        if message.chars().count() >= dereth_client_model::chat::MAX_SAFE_MESSAGE_CHARS {
            long_tells += 1;
        } else {
            lines.push(message);
        }
    }
    assert!(!lines.is_empty(), "the recordings carry lines");
    assert!(
        lines
            .iter()
            .all(|l| l.chars().count() < dereth_client_model::chat::MAX_SAFE_MESSAGE_CHARS),
        "no recorded line a player said reaches the length the check refuses"
    );
    assert!(long_tells > 0, "the long tells are there to be set apart");
    let stopped = lines
        .iter()
        .filter(|l| !dereth_client_model::chat::is_message_safe(l))
        .count();

    // The calibration: the same reader does answer on something known bad.
    let mut planted = lines.clone();
    planted.push("click <tell:Alba> here".to_owned());
    planted.push("two\nlines".to_owned());
    let planted_stopped = planted
        .iter()
        .filter(|l| !dereth_client_model::chat::is_message_safe(l))
        .count();

    // The other gate, over the cadence a player really types at, and then over one he cannot.
    let mut gate = dereth_client_model::chat::SpamGate::default();
    let paced = [108, 112, 155, 166, 168, 193]
        .iter()
        .filter(|t| gate.is_message_spam(**t))
        .count();
    let mut gate = dereth_client_model::chat::SpamGate::default();
    let burst = !gate.is_message_spam(108) && gate.is_message_spam(108);

    let safe = stopped == 0 && planted_stopped == 2 && paced == 0 && burst;
    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.speech.neither-safety-gate-would-have-stopped-a-line-the-recordings-carry",
        move |_| safe,
    );
}

#[test]
fn scenario_neither_safety_gate_would_have_stopped_a_recorded_line() {
    scenario("neither_safety_gate_would_have_stopped_a_recorded_line");
}

// -------------------------------------------------------------------------------------------
// 2. chat.emote.is-drawn-as-name-then-text
// -------------------------------------------------------------------------------------------

/// An emote is drawn as the name and the text; a spoken line from the same speaker, to a client
/// with no body in the world, is not drawn at all.
pub fn emote_is_drawn_as_name_then_text() {
    const SPEAKER: ObjectId = ObjectId(0x5000_0AAA);

    let mut c = HeadlessClient::model();
    let speech = dereth_protocol::comms::CommunicationHearSpeech {
        message: "hello".into(),
        sender_name: "Alba".into(),
        sender_id: SPEAKER,
        text_type: text_type::SPEECH,
    };
    let waves = dereth_protocol::comms::CommunicationHearEmote {
        sender: SPEAKER,
        sender_name: "Alba".to_owned(),
        text: "waves.".to_owned(),
    };
    let cloak = dereth_protocol::comms::CommunicationHearEmote {
        sender: SPEAKER,
        sender_name: "Alba".to_owned(),
        text: "'s cloak is torn.".to_owned(),
    };

    c.when(Inbound::message(&speech))
        .when(Inbound::message(&waves))
        .when(Inbound::message(&cloak))
        .assert_behaviour("chat.emote.is-drawn-as-name-then-text", |v| {
            let drawn = v.chat_text() == ["Alba waves.", "Alba's cloak is torn."];
            let typed = v
                .chat_lines()
                .iter()
                .all(|m| u32::from(m.ty) == text_type::EMOTE);
            let speech_dropped = v.hud().stats.speech_lines_with_no_player_body == 1
                && v.hud().stats.speech_lines_composed == 0;
            drawn && typed && speech_dropped && v.hud().stats.emote_lines_composed == 2
        });
}

#[test]
fn scenario_emote_is_drawn_as_name_then_text() {
    scenario("emote_is_drawn_as_name_then_text");
}

// -------------------------------------------------------------------------------------------
// 3. chat.speech.earshot-and-squelch-both-gate
// -------------------------------------------------------------------------------------------

const OUTDOOR_RANGE: f32 = 75.0;
const INDOOR_RANGE: f32 = 25.0;

/// A speaker standing at a known point, and a listener `d` metres away, through the real handler.
fn lines_heard(d: f32, squelched: bool) -> usize {
    let origin = Vec3::new(100.0, 100.0, 0.0);
    let mut c = HeadlessClient::model();

    // The speaker is created the way the shard creates one, so its position is the client's own
    // reading of a create and not a field this scenario wrote.
    let create =
        dereth_protocol::objects::ItemCreateObject(dereth_protocol::objects::ObjectCreatePayload {
            id: SPEAKER,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: dereth_protocol::types::PhysicsDesc {
                bitfield: dereth_protocol::types::physicsdesc::flags::POSITION,
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: OUTDOOR_CELL,
                    frame: dereth_protocol::types::Frame {
                        origin: dereth_protocol::types::Vec3 {
                            x: origin.x,
                            y: origin.y,
                            z: origin.z,
                        },
                        orientation: dereth_protocol::types::Quat::default(),
                    },
                }),
                ..dereth_protocol::types::PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        });
    c.when(Inbound::world_view(&create));
    assert_eq!(
        c.view()
            .objects()
            .presence(SPEAKER)
            .and_then(|p| p.position)
            .map(|p| p.cell),
        Some(CellId(OUTDOOR_CELL)),
        "the create must have placed the speaker, or the distance below measures nothing"
    );

    if squelched {
        let mut e = SquelchEntry::default();
        e.types.insert(text_type::SPEECH);
        c.world_mut().chat.squelch.characters.insert(SPEAKER, e);
    }

    let (dx, dy) = offset(d);
    c.sync_viewer(Some(ViewerFrame {
        position: Position::new(
            CellId(OUTDOOR_CELL),
            Frame::new(
                Vec3::new(origin.x - dx, origin.y - dy, origin.z),
                Quat::IDENTITY,
            ),
        ),
        heading_degrees: HEADING_DEGREES,
    }));
    assert!(
        c.view().hud().player_body,
        "the listener must have a body before anything is said"
    );

    let say = dereth_protocol::comms::CommunicationHearSpeech {
        message: "o464".into(),
        sender_name: "Speaker".into(),
        sender_id: SPEAKER,
        text_type: text_type::SPEECH,
    };
    c.when(Inbound::message(&say));
    c.chat_lines().len()
}

/// Either the distance or the squelch silences a spoken line, and the two are independent.
pub fn speech_earshot_and_squelch_both_gate() {
    let near = lines_heard(10.0, false);
    let far = lines_heard(90.0, false);
    let muted = lines_heard(10.0, true);

    let mut c = HeadlessClient::model();
    c.assert_behaviour("chat.speech.earshot-and-squelch-both-gate", move |v| {
        // Through the two real handlers.
        let driven = near == 1 && far == 0 && muted == 0;
        // And the cell, which is the only thing that chooses between the two radii.
        let chat = &v.world().chat;
        let mid = Some(offset(40.0));
        let indoors_is_closer = chat.can_hear(SPEAKER, "", text_type::SPEECH, mid, OUTDOOR_RANGE)
            && !chat.can_hear(SPEAKER, "", text_type::SPEECH, mid, INDOOR_RANGE);
        driven && indoors_is_closer
    });
}

#[test]
fn scenario_speech_earshot_and_squelch_both_gate() {
    scenario("speech_earshot_and_squelch_both_gate");
}

// -------------------------------------------------------------------------------------------
// 18. chat.talk-focus.one-authoritative-value
// -------------------------------------------------------------------------------------------

/// The menu and the model write the same focus, and the next line typed reads it.
pub fn talk_focus_has_one_authoritative_value() {
    let mut c = HeadlessClient::model();
    c.when(Player::ui(UiRequest::SetTalkFocus { focus: 5 }));
    let menu_wrote_the_model = c.view().world().chat.talk_focus == TalkFocus::Monarch;

    // A model-owned change is the next command's source, not the last menu request.
    c.world_mut().chat.set_talk_focus(TalkFocus::All);
    c.when(Player::Say("after fallback".to_owned()))
        .assert_behaviour("chat.talk-focus.one-authoritative-value", move |v| {
            menu_wrote_the_model
                && v.world().chat.talk_focus == TalkFocus::All
                && matches!(
                    v.outbound().last(),
                    Some(Request::Talk(m)) if m.message == "after fallback"
                )
        });
}

#[test]
fn scenario_talk_focus_has_one_authoritative_value() {
    scenario("talk_focus_has_one_authoritative_value");
}
