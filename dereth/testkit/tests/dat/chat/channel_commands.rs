use super::*;
// ---------------------------------------------------------------------------------------------
// The channel commands
// ---------------------------------------------------------------------------------------------

/// Every word that names a channel, and the channel it names.
///
/// One command, nineteen words, six channels: which channel a line goes to comes from **the word
/// the player typed** and not from the command behind it.
const THE_NINETEEN: [(&str, u32); 19] = [
    ("a", 0x0200_0000),
    ("co-vassals", 0x0100_0000),
    ("covassals", 0x0100_0000),
    ("covassal", 0x0100_0000),
    ("c", 0x0100_0000),
    ("fellowship", 0x800),
    ("fellows", 0x800),
    ("fellow", 0x800),
    ("f", 0x800),
    ("group", 0x800),
    ("g", 0x800),
    ("party", 0x800),
    ("monarch", 0x4000),
    ("m", 0x4000),
    ("patron", 0x2000),
    ("p", 0x2000),
    ("vassals", 0x1000),
    ("vassal", 0x1000),
    ("v", 0x1000),
];

/// Whatever the strip across the top of the screen is holding.
fn spew(c: &HeadlessClient) -> Vec<String> {
    let app = c.view().expect_app();
    let mut out = app.hud().panels.spew.model.pending.clone();
    out.extend(app.hud().panels.spew.model.items.iter().cloned());
    out
}

/// Type `line` and answer with the broadcasts it produced.
///
/// **The frame matters.** What the client sent is read on the frame it sent it, because the list
/// is cleared at the top of every frame; the strip is read a frame later, because a notice raised
/// in one frame is fanned out to it in the next.
fn broadcast_line(
    c: &mut HeadlessClient,
    hand: &mut Hand,
    line: &str,
) -> Vec<dereth_protocol::comms::CommunicationChannelBroadcast> {
    hand.say(c, line);
    let sent: Vec<dereth_protocol::comms::CommunicationChannelBroadcast> = c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::ChannelBroadcast(m) => Some(m.clone()),
            _ => None,
        })
        .collect();
    c.tick(1);
    sent
}

/// Each of the nineteen words sends a line on its own channel, with the word itself left out of
/// what is said, and says nothing in the player's own window.
pub fn every_channel_word_sends_its_own_channel() {
    let mut c = a_bare_client(true);
    to_gameplay(&mut c);
    let mut hand = Hand::new();
    let quiet_to_start = spew(&c).len();

    // The first one, all the way to the bytes: a game action carrying the channel and then the
    // line, and nothing else. The bytes are read here because a command whose counter climbs while
    // nothing reaches the wire would satisfy everything else.
    let sent = broadcast_line(&mut c, &mut hand, "@f hello fellows");
    let first = sent.len() == 1 && sent[0].channel == 0x800 && sent[0].message == "hello fellows";
    let mut session = Session::new(MockTransport::new());
    let on_the_wire = dereth_client_runtime::requests::send_request(
        &mut session,
        &dereth_client_model::Request::ChannelBroadcast(sent[0].clone()),
    );
    let wire = &session.transport.sent[0];
    let bytes = wire.ordered
        && wire.payload[..4] == 0xF7B1_u32.to_le_bytes()
        && wire.payload[8..12] == 0x0147_u32.to_le_bytes()
        && wire.payload[12..16] == 0x800_u32.to_le_bytes()
        && wire.payload[12..]
            == *dereth_protocol::write_body(&sent[0])
                .expect("the body encodes")
                .as_slice();
    // A line that went out says nothing at all in the player's own window.
    let printed = spew(&c);
    let said_nothing = printed.len() == quiet_to_start
        && !printed.iter().any(|l| l.contains("not a valid command"));

    // And every one of the nineteen, because a command that hard-coded one channel would satisfy
    // everything above.
    let mut seen: Vec<(&str, u32)> = Vec::new();
    for (word, _) in THE_NINETEEN {
        let sent = broadcast_line(&mut c, &mut hand, &format!("@{word} o924"));
        assert_eq!(
            sent.len(),
            1,
            "@{word} sent nothing; the strip holds {:?}",
            spew(&c)
        );
        assert_eq!(
            sent[0].message, "o924",
            "@{word}: the word itself is not part of the line"
        );
        seen.push((word, sent[0].channel));
    }
    let all_nineteen = seen == THE_NINETEEN.to_vec();

    c.assert_behaviour(
        "chat.channel-commands.every-word-sends-its-own-channel-and-says-nothing-locally",
        move |_| first && on_the_wire && bytes && said_nothing && all_nineteen,
    );
    c.shutdown();
}

/// A channel command with nothing after it is told what to do -- and is not told it is not a
/// command, which is a different answer and would be the wrong one.
pub fn an_empty_channel_command_says_what_to_do_and_sends_nothing() {
    let mut c = a_bare_client(true);
    to_gameplay(&mut c);
    let mut hand = Hand::new();

    let sent = broadcast_line(&mut c, &mut hand, "@f");
    let silent = sent.is_empty();
    let printed = spew(&c);
    let told = printed
        .iter()
        .any(|l| l == "You must specify the text you wish to broadcast!");
    let not_refused = !printed.iter().any(|l| l.contains("not a valid command"));

    c.assert_behaviour(
        "chat.channel-commands.one-with-no-text-says-what-to-do-and-sends-nothing",
        move |_| silent && told && not_refused,
    );
    c.shutdown();
}
