use super::*;
// ---------------------------------------------------------------------------------------------
// A line broadcast on a channel, end to end and in its own colour
// ---------------------------------------------------------------------------------------------

/// An opaque colour, and the colour a clickable name is drawn in.
pub(super) const OPAQUE: u32 = 0xFF00_0000;
const TAG_COLOUR: u32 = 0xFF00_B200;

/// The clickable-name markup is taken off the letters and hung on them, so what is drawn is the
/// line without it.
fn without_tell_markup(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        if let Some(close) = after.find('>') {
            let tag = &after[1..close];
            if tag.starts_with("Tell:") || tag == "\\Tell" {
                rest = &after[close + 1..];
                continue;
            }
        }
        out.push('<');
        rest = &after[1..];
    }
    out.push_str(rest);
    out
}

/// Hand one broadcast to the client and let it reach the drawn log.
///
/// The line sits in the client's own pending list until the **next** frame, because what moves it
/// into the window is part of the frame and not part of taking the message, so the frame below is
/// load-bearing rather than a settling loop. What the client composed is answered, so the model
/// and the letters are asserted apart.
fn hear_on_a_channel(
    c: &mut HeadlessClient,
    channel: u32,
    sender: &str,
    message: &str,
) -> Vec<dereth_ui_screens::chat::interface::ChatMessage> {
    use dereth_protocol::Message as _;
    let m = dereth_protocol::comms::CommunicationChannelBroadcastRecv {
        channel,
        sender_name: sender.to_owned(),
        message: message.to_owned(),
    };
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(dereth_protocol::Opcode::COMMUNICATION_CHANNEL_BROADCAST.0);
    m.write(&mut w).expect("the message encodes");
    let composed = c.app_mut().apply_hud_events(&[SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode::COMMUNICATION_CHANNEL_BROADCAST,
        blob: w.into_inner(),
    }]);
    c.tick(1);
    composed
}

/// How many channel lines the client has composed and how many it has refused. The pair tells a
/// line dropped at the gate apart from a line the client never read at all, which a missing line
/// on the log cannot.
fn channel_counts(c: &HeadlessClient) -> (u64, u64) {
    let s = &c.view().expect_app().hud().stats;
    (
        s.channel_broadcast_lines_composed,
        s.channel_broadcast_lines_squelched,
    )
}

/// The first recorded broadcast that names its speaker, found by walking the recordings.
fn a_recorded_named_broadcast() -> (u32, String, String) {
    use dereth_client_net::client_session::testing::Direction;
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient || b.opcode != 0xF7B0 || b.payload.len() < 16 {
                continue;
            }
            if u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes"))
                != dereth_protocol::Opcode::COMMUNICATION_CHANNEL_BROADCAST.0
            {
                continue;
            }
            let Ok(m) = dereth_protocol::read_body::<
                dereth_protocol::comms::CommunicationChannelBroadcastRecv,
            >(&b.payload[16..]) else {
                continue;
            };
            if !m.sender_name.is_empty() {
                return (m.channel, m.sender_name, m.message);
            }
        }
    }
    panic!("the corpus carries a recorded broadcast that names its speaker");
}

/// A speaker's name, lifted off a recorded spoken line so that the name is a shard's and not this
/// file's.
fn a_recorded_speaker_name() -> String {
    use dereth_client_net::client_session::testing::Direction;
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient
                || b.opcode != dereth_protocol::Opcode::COMMUNICATION_HEAR_SPEECH.0
            {
                continue;
            }
            let Ok(m) = dereth_protocol::read_body::<dereth_protocol::comms::CommunicationHearSpeech>(
                &b.payload[4..],
            ) else {
                continue;
            };
            if !m.sender_name.is_empty() && m.sender_name.is_ascii() {
                return m.sender_name;
            }
        }
    }
    panic!("the corpus carries a recorded speaker's name");
}

/// A broadcast a shard really sent, naming its speaker, reaches the log -- the name drawn as
/// something to click and the rest of the line in the colour that kind of line is drawn in.
pub fn a_recorded_broadcast_that_names_its_speaker_is_drawn() {
    let (channel, sender, message) = a_recorded_named_broadcast();
    let (ty, line) =
        dereth_client_model::chat::composition::channel_broadcast_line(channel, &sender, &message);
    let trimmed = dereth_client_model::chat::composition::add_text_to_scroll_trim(&line);
    let visible = without_tell_markup(&trimmed);

    let mut c = a_client_listening();
    let before = colour_runs(&mut c).len();
    let composed = hear_on_a_channel(&mut c, channel, &sender, &message);
    let one_line =
        composed.len() == 1 && composed[0].body == trimmed && u32::from(composed[0].ty) == ty;

    let runs = colour_runs(&mut c);
    let reached = runs.len() > before;
    let (joined, colour) = drawn_line(&mut c, &message);
    let drawn = joined.contains(&visible) && !joined.contains("<Tell:IIDString:");
    // The colour, apart from the letters: the body is drawn in this kind of line's own colour and
    // the speaker's name in the one a clickable name is drawn in.
    let want = OPAQUE
        | dereth_ui_screens::chat::colors::color_for_type(
            u8::try_from(ty).expect("a kind of line"),
        )
        .hex;
    let body_colour = colour == Some(want);
    let name_run = colour_runs(&mut c)
        .into_iter()
        .find(|(s, _)| s.contains(&sender) && !s.contains("says"))
        .map(|(_, col)| col);
    let name_colour = name_run == Some(TAG_COLOUR);

    c.assert_behaviour(
        "chat.channel.a-recorded-broadcast-that-names-its-speaker-is-drawn-in-its-own-colour",
        move |_| one_line && reached && drawn && body_colour && name_colour,
    );
    c.shutdown();
}

/// Every channel a player speaks on draws its own line on the log in its own kind's colour -- and
/// the colours are not all one colour, or a table that answered the same thing for everything
/// would satisfy every row of it.
pub fn every_channel_draws_its_own_line_in_its_own_colour() {
    use dereth_client_model::chat::text_type;

    let who = a_recorded_speaker_name();
    // (the channel, the word this line is known by here, the kind of line it is)
    let cases: [(u32, &str, u32); 9] = [
        (0x0000_0800, "fellowship", text_type::FELLOWSHIP),
        (0x0000_1000, "vassals", text_type::SOCIAL),
        (0x0000_2000, "patron", text_type::SOCIAL),
        (0x0000_4000, "monarch", text_type::SOCIAL),
        (0x0100_0000, "covassals", text_type::SOCIAL),
        (0x0200_0000, "allegiance", text_type::SOCIAL),
        (0x0000_0400, "help", text_type::HELP),
        (0x0000_0001, "abuse", text_type::ABUSE),
        (0x0000_0080, "unknown", text_type::CHANNEL),
    ];

    let mut c = a_client_listening();
    let mut every = true;
    for (channel, word, want_ty) in cases {
        // A body unique to this channel, so a lookup cannot read another line's run.
        let message = format!("channel {word} body");
        let (ty, line) =
            dereth_client_model::chat::composition::channel_broadcast_line(channel, &who, &message);
        let kind = ty == want_ty;
        let before = colour_runs(&mut c).len();
        hear_on_a_channel(&mut c, channel, &who, &message);
        let visible = without_tell_markup(
            &dereth_client_model::chat::composition::add_text_to_scroll_trim(&line),
        );
        let reached = colour_runs(&mut c).len() > before;
        let (joined, colour) = drawn_line(&mut c, &message);
        let want = OPAQUE
            | dereth_ui_screens::chat::colors::color_for_type(
                u8::try_from(ty).expect("a kind of line"),
            )
            .hex;
        let holds = kind
            && reached
            && joined.contains(&visible)
            && colour == Some(want)
            && !joined.contains("<Tell:IIDString:")
            && colour_runs(&mut c)
                .iter()
                .any(|(s, col)| s.contains(&who) && *col == TAG_COLOUR);
        assert!(holds, "channel {channel:#x} ({word})");
        every &= holds;
    }

    // Four distinct colours across the nine. Two of the kinds share one, which is the client's own
    // answer and is asserted as such rather than worked around.
    let fellow = drawn_line(&mut c, "channel fellowship body")
        .1
        .expect("the fellowship line");
    let social = drawn_line(&mut c, "channel vassals body")
        .1
        .expect("the vassals line");
    let help = drawn_line(&mut c, "channel help body")
        .1
        .expect("the help line");
    let abuse = drawn_line(&mut c, "channel abuse body")
        .1
        .expect("the abuse line");
    let other = drawn_line(&mut c, "channel unknown body")
        .1
        .expect("the unknown-channel line");
    let distinct: std::collections::BTreeSet<u32> =
        [fellow, social, help, abuse, other].into_iter().collect();
    let four = fellow == social && distinct.len() == 4;

    c.assert_behaviour(
        "chat.channel.every-channel-draws-its-own-line-in-its-own-colour",
        move |_| every && four,
    );
    c.shutdown();
}

/// Silencing the speaker does not silence a channel line, and silencing the kind of line does --
/// but only for a kind the client will answer about at all.
pub fn a_squelched_speaker_is_still_heard_on_a_channel() {
    use dereth_client_model::chat::text_type;

    let who = a_recorded_speaker_name();
    let mut c = a_client_listening();

    // The speaker, silenced on everything there is and on his account too. A channel line carries
    // no speaker's id, so the strongest form of "this one is silenced" cannot reach the question
    // the client asks -- and that is the claim.
    {
        let mut entry = dereth_client_model::chat::SquelchEntry {
            name: who.clone(),
            ..Default::default()
        };
        entry.squelch_everything();
        let squelch = &mut c.world_mut().chat.squelch;
        squelch.characters.insert(ObjectId(0x5000_0099), entry);
        squelch.accounts.insert(who.clone(), 0);
    }
    hear_on_a_channel(&mut c, 0x0000_0800, &who, "channel speaker silenced");
    let still_heard = drawn_line(&mut c, "channel speaker silenced")
        .0
        .contains("channel speaker silenced");

    // The kind of line, silenced: a fellowship line is one the client will answer about, and it is
    // dropped before it is composed.
    c.world_mut()
        .chat
        .squelch
        .global
        .types
        .insert(text_type::FELLOWSHIP);
    let before = channel_counts(&c);
    hear_on_a_channel(&mut c, 0x0000_0800, &who, "channel kind silenced");
    let after = channel_counts(&c);
    let dropped = (after.0 - before.0, after.1 - before.1) == (0, 1)
        && !drawn_line(&mut c, "channel kind silenced")
            .0
            .contains("channel kind silenced");

    // The same table, a kind the client will not answer about: the question is refused before it
    // reaches the table, so the line is drawn.
    c.world_mut()
        .chat
        .squelch
        .global
        .types
        .insert(text_type::SOCIAL);
    let before = channel_counts(&c);
    hear_on_a_channel(&mut c, 0x0200_0000, &who, "channel kind not asked");
    let after = channel_counts(&c);
    let still_drawn = (after.0 - before.0, after.1 - before.1) == (1, 0)
        && drawn_line(&mut c, "channel kind not asked")
            .0
            .contains("channel kind not asked");

    c.assert_behaviour(
        "chat.channel.silencing-the-speaker-does-not-silence-a-channel-line",
        move |_| still_heard && dropped && still_drawn,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// What the log draws, and in what colour
// ---------------------------------------------------------------------------------------------

/// Two recorded spoken lines of **different kinds**, one from somebody who is a player and one
/// from somebody who is not -- found by walking the recordings rather than named.
///
/// The recordings carry no private messages: what one player says to another is private and the
/// public recordings are scrubbed of it. So the two kinds this compares are two kinds of spoken
/// line, which is what the claim needs -- the colour has to be chosen by the kind.
fn two_recorded_lines_of_different_kinds() -> [dereth_protocol::comms::CommunicationHearSpeech; 2] {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    let mut plain: Option<dereth_protocol::comms::CommunicationHearSpeech> = None;
    let mut clickable: Option<dereth_protocol::comms::CommunicationHearSpeech> = None;
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient
                || b.opcode != dereth_protocol::Opcode::COMMUNICATION_HEAR_SPEECH.0
                || b.payload.len() < 4
            {
                continue;
            }
            let Ok(m) = dereth_protocol::read_body::<dereth_protocol::comms::CommunicationHearSpeech>(
                &b.payload[4..],
            ) else {
                continue;
            };
            if m.message.is_empty() || m.sender_name.is_empty() {
                continue;
            }
            let is_player = dereth_client_model::chat::composition::CLICKABLE_PLAYER_IDS
                .contains(&m.sender_id.0);
            if is_player {
                if clickable.is_none() {
                    clickable = Some(m);
                }
            } else if plain.is_none() {
                plain = Some(m);
            }
            if let (Some(p), Some(c)) = (plain.as_ref(), clickable.as_ref()) {
                if p.text_type != c.text_type {
                    break;
                }
                // The two so far are the same kind of line, which would prove nothing about the
                // colour being chosen; keep looking for one of another kind.
                if is_player {
                    clickable = None;
                } else {
                    plain = None;
                }
            }
        }
    }
    let plain = plain.expect("a recorded line from somebody who is not a player");
    let clickable = clickable.expect("a recorded line from somebody who is");
    assert_ne!(
        plain.text_type, clickable.text_type,
        "the two recorded lines must be of different kinds, or the colour proves nothing"
    );
    [plain, clickable]
}

/// Two lines of different kinds, both recorded, drawn on the shipped log in two different colours
/// -- which is what shows the colour is chosen by the kind of line rather than being one colour
/// for everything.
pub fn two_recorded_channels_draw_in_two_different_colours() {
    let [plain, clickable] = two_recorded_lines_of_different_kinds();
    let plain_body = dereth_client_model::chat::composition::hear_speech_line(
        plain.sender_id.0,
        None,
        &plain.sender_name,
        &plain.message,
    );
    let clickable_body = dereth_client_model::chat::composition::hear_speech_line(
        clickable.sender_id.0,
        None,
        &clickable.sender_name,
        &clickable.message,
    );
    // One is drawn plainly and the other with a name the player can click, so between them both
    // forms of a line reach the log.
    let two_forms = !plain_body.starts_with('<') && clickable_body.starts_with("<Tell:IIDString:");
    let plain_visible = format!("{} says, \"{}\"", plain.sender_name, plain.message);
    let clickable_visible = format!("{} says, \"{}\"", clickable.sender_name, clickable.message);

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let cases: [(u8, &str, &str, &str, &str); 2] = [
        (
            u8::try_from(plain.text_type).expect("a kind of line"),
            &plain_body,
            &plain_visible,
            &plain.sender_name,
            &plain.message,
        ),
        (
            u8::try_from(clickable.text_type).expect("a kind of line"),
            &clickable_body,
            &clickable_visible,
            &clickable.sender_name,
            &clickable.message,
        ),
    ];
    let mut every = true;
    let mut colours: Vec<u32> = Vec::new();
    for (ty, body, visible, name, words) in cases {
        let want = OPAQUE | dereth_ui_screens::chat::colors::color_for_type(ty).hex;
        let before = colour_runs(&mut c).len();
        let line = dereth_ui_screens::chat::interface::ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
            ty,
            body: (*body).to_owned(),
            prefix: None,
            window: 0,
        };
        let took = with_screen(&mut c, |ui, s| s.recv_display_final_string_info(ui, &line));
        let reached = !took.is_empty() && colour_runs(&mut c).len() > before;

        let all = colour_runs(&mut c);
        let joined: String = all.iter().map(|(s, _)| s.as_str()).collect();
        let drawn = joined.contains(visible)
            && !joined.contains("<Tell:IIDString:")
            // The name is followed by the client's own verb on the element itself.
            && joined.contains(&format!("{name} says, \""));

        // The colour, apart from the letters: the run carrying the verb is this kind of line's
        // colour, and not the grey the timestamp is drawn in.
        let is_clickable = body.starts_with("<Tell:IIDString:");
        // Found by this line's own words, so it cannot read the other line's run.
        let run = all
            .iter()
            .find(|(s, _)| s.contains(words))
            .unwrap_or_else(|| panic!("this line's words are one run; got {all:?}"));
        // The one kind of line that really is drawn grey is an emote, so the check that the name
        // is not in the timestamp's grey is made for every other kind.
        let coloured = run.1 == want
            && (ty == 12 || run.1 != OPAQUE | dereth_ui_screens::chat::colors::GREY.hex);
        // And a clickable name is drawn in the colour a clickable name is drawn in, which is not
        // the line's own.
        let name_coloured = !is_clickable
            || all
                .iter()
                .rev()
                .find(|(s, _)| s.contains(name))
                .is_some_and(|(_, col)| *col == TAG_COLOUR && *col != want);
        assert!(
            reached && drawn && coloured && name_coloured,
            "the {ty} line: reached={reached} drawn={drawn} coloured={coloured} name={name_coloured}; runs {all:?}"
        );
        every &= reached && drawn && coloured && name_coloured;
        colours.push(want);
    }

    // The two kinds really differed **on the element**, which one colour for everything could not
    // survive.
    let all = colour_runs(&mut c);
    let on_the_element: std::collections::BTreeSet<u32> = all
        .iter()
        .filter(|(s, _)| s.contains(&plain.message) || s.contains(&clickable.message))
        .map(|(_, col)| *col)
        .collect();
    let differ = colours[0] != colours[1]
        && on_the_element
            == colours
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<u32>>();

    c.assert_behaviour(
        "chat.log.two-recorded-kinds-of-line-draw-in-two-different-colours",
        move |_| two_forms && every && differ,
    );
    c.shutdown();
}

/// The player's own line comes back to him in one form and somebody else's in another, and both
/// reach the log in the colour a spoken line is drawn in -- never the name and the words run
/// together with no verb between them.
pub fn your_own_echo_and_a_remote_speaker_are_drawn_apart() {
    const ME_HERE: u32 = 0x5000_1234;
    const NOT_A_PLAYER: u32 = 0x8000_0DE9;

    let kind = u8::try_from(dereth_client_model::chat::text_type::SPEECH).expect("a kind of line");
    let want = OPAQUE | dereth_ui_screens::chat::colors::color_for_type(kind).hex;

    let echo = dereth_client_model::chat::composition::hear_speech_line(
        ME_HERE,
        Some(ME_HERE),
        "Lark",
        "W",
    );
    let remote = dereth_client_model::chat::composition::hear_speech_line(
        NOT_A_PLAYER,
        Some(ME_HERE),
        "Sparring Golem",
        "Have at you!",
    );
    let two_forms = echo == "You say, \"W\"" && remote == "Sparring Golem says, \"Have at you!\"";

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    for body in [&echo, &remote] {
        let line = dereth_ui_screens::chat::interface::ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
            ty: kind,
            body: body.clone(),
            prefix: None,
            window: 0,
        };
        let took = with_screen(&mut c, |ui, s| s.recv_display_final_string_info(ui, &line));
        assert!(!took.is_empty(), "{body:?} reached a window");
    }

    let all = colour_runs(&mut c);
    let joined: String = all.iter().map(|(s, _)| s.as_str()).collect();
    let both_drawn =
        joined.contains(&echo) && joined.contains(&remote) && !joined.contains("LarkW");
    let mut coloured = true;
    for body in [&echo, &remote] {
        coloured &= all
            .iter()
            .find(|(s, _)| s.contains(body.as_str()))
            .is_some_and(|(_, col)| *col == want);
    }

    c.assert_behaviour(
        "chat.speech.your-own-echo-and-a-remote-speaker-are-drawn-from-two-different-forms",
        move |_| two_forms && both_drawn && coloured,
    );
    c.shutdown();
}
