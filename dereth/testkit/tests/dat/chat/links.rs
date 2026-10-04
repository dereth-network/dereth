use super::*;
// ---------------------------------------------------------------------------------------------
// The clickable name in a chat line
// ---------------------------------------------------------------------------------------------

/// The room this section's general channel is, and who speaks in it.
const TAG_ROOM: u32 = 123;
const TAG_SENDER: &str = "Sender";
const TAG_SAID: &str = "hello";
/// What a general-channel line is drawn as, and the two colours the window picks between: the one
/// a clickable name is drawn in, and the one this channel's lines are drawn in.
const GENERAL_TYPE: u8 = 0x1B;
const TAG_GREEN: u32 = 0xFF00_B200;
const CHANNEL_BLUE: u32 = 0xFFB4_DCF0;

fn composed_room_line(sender: &str, said: &str) -> String {
    format!("[General] <Tell:IIDString:0:{sender}>{sender}<\\Tell> says, \"{said}\"")
}

fn drawn_room_line(sender: &str, said: &str) -> String {
    format!("[General] {sender} says, \"{said}\"")
}

/// One room event, in the shape the chat-room transport delivers.
fn tagged_room_event(text: &str) -> Vec<u8> {
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

/// The line the client composes from one room event, made on a client of its own.
fn turbine_line(text: &str) -> dereth_ui_screens::chat::interface::ChatMessage {
    let mut c = HeadlessClient::model();
    {
        let w = c.world_mut();
        w.chat.startup_turbine_chat();
        w.chat
            .recv_chat_room_tracker(dereth_protocol::comms::ChatRoomMembership {
                general_room: TAG_ROOM,
                ..dereth_protocol::comms::ChatRoomMembership::default()
            });
    }
    c.when(Inbound::event(SessionEvent::TurbineChat(
        tagged_room_event(text),
    )));
    let lines = c.view().chat_lines();
    assert_eq!(lines.len(), 1, "one line per accepted room event");
    lines[0].clone()
}

/// What each drawn letter of a tagged glyph carries.
type TagSummary = Option<(String, String, Option<u32>, Option<String>)>;

/// Put one line through the window's own delivery and read the shipped log's letters back, with
/// what each one carries.
fn drawn_with_tags(
    c: &mut HeadlessClient,
    m: &dereth_ui_screens::chat::interface::ChatMessage,
) -> (String, Vec<TagSummary>) {
    let log = element(c, dereth_ui_screens::chat::window::LOG);
    into_the_window(c, std::slice::from_ref(m));
    let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
    let t = ui.text_element_mut(log).expect("the log is a text element");
    let tags = t
        .glyphs
        .glyphs
        .iter()
        .map(|g| {
            g.tag.as_ref().map(|tag| {
                (
                    tag.type_keyword.clone(),
                    tag.format.clone(),
                    tag.id(),
                    tag.payload().map(str::to_owned),
                )
            })
        })
        .collect();
    (t.glyphs.inq_text(false), tags)
}

/// The window draws the name and not the markup around it, and every letter of the name is
/// something the player can click.
pub fn the_log_draws_the_name_and_not_the_markup() {
    let m = turbine_line(TAG_SAID);
    let carried = m.body == composed_room_line(TAG_SENDER, TAG_SAID);
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let (text, tags) = drawn_with_tags(&mut c, &m);

    let want = drawn_room_line(TAG_SENDER, TAG_SAID);
    let drawn = text == want && !text.contains('<') && tags.len() == want.chars().count();
    let start = want
        .chars()
        .take(want.find(TAG_SENDER).expect("the name is drawn"))
        .count();
    let end = start + TAG_SENDER.chars().count();
    let mut tagged = true;
    for (i, tag) in tags.iter().enumerate() {
        if (start..end).contains(&i) {
            let Some((ty, format, id, payload)) = tag else {
                panic!("letter {i} of the name carries nothing")
            };
            tagged &= ty == "Tell"
                && format == "IIDString"
                && *id == Some(0)
                && payload.as_deref() == Some(TAG_SENDER);
        } else {
            tagged &= tag.is_none();
        }
    }

    c.assert_behaviour(
        "chat.tell-markup.the-log-draws-the-name-and-not-the-markup",
        move |_| carried && drawn && tagged,
    );
    c.shutdown();
}

/// The clickable name is drawn in its own colour and the rest of the line in the channel's, so
/// that a name the player can click looks different from the words around it.
pub fn the_clickable_name_is_drawn_in_its_own_colour() {
    let m = turbine_line(TAG_SAID);
    let is_general = m.ty == GENERAL_TYPE;
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let log = element(&c, dereth_ui_screens::chat::window::LOG);
    let (text, tags) = drawn_with_tags(&mut c, &m);
    let (tag_colour, line_colour, colours) = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let t = ui.text_element_mut(log).expect("the log is a text element");
        (
            t.tag_font_color,
            t.font_color_at(GENERAL_TYPE),
            t.glyphs
                .glyphs
                .iter()
                .map(|g| g.color)
                .collect::<Vec<u32>>(),
        )
    };

    let same_line = text == drawn_room_line(TAG_SENDER, TAG_SAID);
    // The two colours are read off the live element and they differ, so this measures something.
    let two_colours =
        tag_colour == TAG_GREEN && line_colour == CHANNEL_BLUE && tag_colour != line_colour;
    let one_each = colours.len() == tags.len()
        && tags.iter().filter(|t| t.is_some()).count() == TAG_SENDER.chars().count();
    let mut each_letter = true;
    for (colour, tag) in colours.iter().zip(&tags) {
        let want = if tag.is_some() {
            tag_colour
        } else {
            line_colour
        };
        each_letter &= *colour == want;
    }

    c.assert_behaviour(
        "chat.tell-markup.the-clickable-name-is-drawn-in-its-own-colour",
        move |_| is_general && same_line && two_colours && one_each && each_letter,
    );
    c.shutdown();
}

/// Markup the client has no reader for is not markup: every character of it is drawn, and nothing
/// in the line is clickable.
pub fn markup_the_client_does_not_know_is_drawn_as_it_stands() {
    let raw = "give <IIDString:Name:0x50001234:x>Shard<\\IIDString> to";
    let m = dereth_ui_screens::chat::interface::ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        ty: GENERAL_TYPE,
        body: raw.to_owned(),
        prefix: None,
        window: 0,
    };
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let (text, tags) = drawn_with_tags(&mut c, &m);
    let verbatim = text == raw && tags.iter().all(Option::is_none);

    c.assert_behaviour(
        "chat.tell-markup.markup-the-client-does-not-know-is-drawn-as-it-stands",
        move |_| verbatim,
    );
    c.shutdown();
}
