use super::*;
// ---------------------------------------------------------------------------------------------
// The emote, typed into the shipped entry
// ---------------------------------------------------------------------------------------------

/// The bytes one emote goes out in, composed here rather than by the writer being asserted over.
fn emote_bytes(stamp: u8) -> Vec<u8> {
    vec![
        0xb1, 0xf7, 0, 0, stamp, 0, 0, 0, 0xdf, 1, 0, 0, 5, 0, b'w', b'a', b'v', b'e', b's', 0,
    ]
}

/// Typing an emote into the shipped entry sends it, an empty one sends nothing, and neither is
/// repeated on the following frame.
pub fn typing_an_emote_sends_it_and_leaves_the_next_line_free() {
    let mut c = a_bare_client(true);
    to_gameplay(&mut c);
    let mut hand = Hand::new();
    let mut session = Session::new(MockTransport::new());
    let mut every = true;

    for (index, line) in ["@e waves", "@emote", ":waves"].into_iter().enumerate() {
        let (undeliverable, lines) = {
            let s = &c.view().expect_app().interaction().stats;
            (s.requests_undeliverable, s.chat_lines_sent)
        };
        hand.say(&mut c, line);
        // The keystrokes have to become a chat line before the command is judged at all.
        let reached = c.view().expect_app().interaction().stats.chat_lines_sent == lines + 1;
        let holds = if index == 1 {
            // The empty one: nothing to send, and nothing counted as undeliverable either.
            c.view().expect_app().interaction().last_sent.is_empty()
                && c.view()
                    .expect_app()
                    .interaction()
                    .stats
                    .requests_undeliverable
                    == undeliverable
        } else {
            let requests = c.view().expect_app().interaction().last_sent.clone();
            let one = requests.len() == 1
                && matches!(&requests[0], dereth_client_model::Request::Emote(m) if m.message == "waves");
            // A client with no link records it as undeliverable rather than as sent.
            let counted = c
                .view()
                .expect_app()
                .interaction()
                .stats
                .requests_undeliverable
                == undeliverable + 1;
            let bytes = one
                && dereth_client_runtime::requests::send_request(&mut session, &requests[0])
                && session.transport.sent.last().expect("one datagram").payload
                    == emote_bytes(if index == 0 { 1 } else { 2 });
            one && counted && bytes
        };
        let clean = c
            .view()
            .expect_app()
            .interaction()
            .stats
            .chat_commands_unimplemented
            == 0
            && c.view().expect_app().interaction().last_refusal.is_none();
        c.tick(1);
        let not_repeated = c.view().expect_app().interaction().last_sent.is_empty();
        assert!(reached && holds && clean && not_repeated, "{line}");
        every &= reached && holds && clean && not_repeated;
    }
    let two = session.transport.sent.len() == 2;

    c.assert_behaviour(
        "chat.emote.typing-one-into-the-shipped-entry-sends-it-and-leaves-the-next-line-free",
        move |_| every && two,
    );
    c.shutdown();
}

/// A typed emote reaches the link the client is attached to, on the frame after the one it was
/// typed in, and exactly once.
pub fn a_typed_emote_reaches_the_link_on_the_next_frame() {
    let mut c = a_bare_client(true);
    to_gameplay(&mut c);

    // A link with a connection on it and no socket behind it: nothing leaves this process.
    let mut net = dereth_client::net::ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "emote-scenario",
        "unused",
        0,
    )
    .expect("the socket-free link builds");
    net.session.transport.add_connection(
        0xB,
        0,
        1,
        0xDEAD_BEEF,
        0x1234_5678,
        Some("127.0.0.1:19000".parse().expect("a literal address")),
    );
    c.app_mut()
        .attach_replay_network(net)
        .expect("the link attaches");

    /// Every application action the link has been handed since the last look.
    fn actions(c: &mut HeadlessClient) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for (bytes, _) in c
            .app_mut()
            .replay_network_mut()
            .expect("the link is attached")
            .take_outgoing()
        {
            let packet =
                dereth_transport::wire::ParsedPacket::parse(&bytes).expect("a whole datagram");
            for fragment in packet.fragments {
                if fragment.header.queue_id == 3 {
                    assert_eq!(fragment.header.num_frags, 1, "this line is one fragment");
                    out.push(fragment.payload);
                }
            }
        }
        out
    }

    // **The baseline is not nought and says why.** The count of requests the client could not
    // deliver is cumulative, and the frame that brought the screen up ran before the link was
    // attached -- so whatever the interface asked for then is already in it. What this scenario is
    // about is the emote, which is a difference.
    let undeliverable = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .requests_undeliverable;
    let mut hand = Hand::new();
    hand.say(&mut c, "@e waves");

    let produced = {
        let s = &c.view().expect_app().interaction().stats;
        s.chat_lines_sent == 1 && s.requests_sent == 1 && s.requests_undeliverable == undeliverable
    } && matches!(
        c.view().expect_app().interaction().last_sent.as_slice(),
        [dereth_client_model::Request::Emote(m)] if m.message == "waves"
    );
    // The gesture arrives after this frame has already handed the link what it had.
    let not_yet = actions(&mut c).is_empty()
        && c.app_mut()
            .replay_network_mut()
            .expect("attached")
            .session
            .next_action_stamp()
            == 2;

    c.tick(1);
    let arrived = actions(&mut c) == vec![emote_bytes(1)];
    c.tick(1);
    let only_once = actions(&mut c).is_empty()
        && c.app_mut()
            .replay_network_mut()
            .expect("attached")
            .session
            .next_action_stamp()
            == 2;

    c.assert_behaviour(
        "chat.emote.a-typed-one-reaches-the-link-on-the-next-frame-and-only-once",
        move |_| produced && not_yet && arrived && only_once,
    );
    c.shutdown();
}
