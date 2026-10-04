use super::*;
// ---------------------------------------------------------------------------------------------
// The bottom of the chat log
// ---------------------------------------------------------------------------------------------

/// The two lines a shard sends on entering the world and on being asked for help. **Both end in a
/// newline**, which is what the window's own trimming exists for; that they still do when the
/// client has finished reading them is asserted in `tests/cpu/chat.rs`.
const WELCOME: &str = "Welcome to Asheron's Call\n  powered by ACEmulator\n\nFor more information on commands supported by this server, type @acehelp\n";
const ACEHELP: &str = "Note: You may substitute a forward slash (/) for the at symbol (@).\nUse @help to get more information about commands supported by the client.\nAvailable help:\n@acehelp commands - Lists all commands.\nYou can also use @acecommands to get a complete list of the supported ACEmulator commands available to you.\nTo get more information about a specific command, use @acehelp command\n";

/// The line the client composes from one of the shard's system messages, made on a client of its
/// own so that nothing else is in the window that takes it.
fn system_line(text: &str) -> dereth_ui_screens::chat::interface::ChatMessage {
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

/// The bottom row of the shipped log carries the newest line's own letters and sits on the pane's
/// bottom edge -- it is never an empty row under the last message.
pub fn the_bottom_row_of_the_log_is_the_newest_line() {
    let lines = [
        system_line(WELCOME),
        system_line(ACEHELP),
        system_line(WELCOME),
    ];
    let last = lines.last().expect("three lines").body.clone();

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let log = element(&c, dereth_ui_screens::chat::window::LOG);
    into_the_window(&mut c, &lines);

    let (
        not_a_newline,
        has_glyphs,
        has_width,
        ends_with_the_last,
        overflows,
        on_the_last_row,
        agrees,
    ) = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let screen_box = ui.screen_box(log);
        let view = screen_box.height();
        let t = ui.text_element_mut(log).expect("the log is a text element");
        let content = t.content_box(screen_box);
        let wrapped =
            dereth_ui::text::glyph::wrap(&t.glyphs.glyphs, content.width(), t.glyphs.one_line);
        let bottom = *wrapped.last().expect("the log has wrapped rows");
        let text = t.glyphs.inq_text(false);
        (
            !t.glyphs
                .glyphs
                .last()
                .expect("the log has letters")
                .is_new_line(),
            bottom.end > bottom.start,
            bottom.width > 0,
            text.ends_with(last.trim_matches('\n')),
            t.scroll.height > view,
            t.scroll.y == t.scroll.height - view,
            t.is_at_vertical_end(screen_box),
        )
    };

    c.assert_behaviour(
        "chat.log.the-bottom-row-is-the-newest-line-and-never-an-empty-one",
        move |_| {
            not_a_newline
                && has_glyphs
                && has_width
                && ends_with_the_last
                && overflows
                && on_the_last_row
                && agrees
        },
    );
    c.shutdown();
}

/// The newline the window puts in front of each line is a separator between two lines and not a
/// terminator after the last one: three lines leave two of them and nothing after.
pub fn the_newline_between_lines_is_a_separator() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let log = element(&c, dereth_ui_screens::chat::window::LOG);
    into_the_window(
        &mut c,
        &[
            system_line(WELCOME),
            system_line(ACEHELP),
            system_line(WELCOME),
        ],
    );

    let text = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        ui.text_element_mut(log)
            .expect("a text element")
            .glyphs
            .inq_text(false)
    };
    let want = format!(
        "{}\n{}\n{}",
        WELCOME.trim_matches('\n'),
        ACEHELP.trim_matches('\n'),
        WELCOME.trim_matches('\n'),
    );
    let joined = text == want;

    c.assert_behaviour(
        "chat.log.the-newline-between-lines-is-a-separator-and-not-a-terminator",
        move |_| joined,
    );
    c.shutdown();
}
