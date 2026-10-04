//! UI fixtures and scenarios for recorded notices.

use super::*;
// =============================================================================================
// The notice strip, over the whole corpus
//
// Both scenarios replay **every** recording and derive what to expect from the recordings
// themselves. `dereth_testkit::corpus_steps::sweep` delivers a recording one datagram at a time
// and lets a scenario look between them, which is what a claim about a surface the ending clears
// needs.
//
// Everything counted here is counted off the **decoded corpus index**, never off a raw payload's
// words: the index says how many of each message a recording carries, and the client's own
// composers say what each of them draws.
// =============================================================================================

/// Every message the shard sent in `session`, as `(what it is, its body)`.
///
/// A message inside the ordered-event envelope is four bytes further in than one that is not, and
/// reading it at the wrong offset is a recorded mistake that reads the stamp.
fn strip_messages(session: &str) -> Vec<(u32, Vec<u8>)> {
    use dereth_client_net::client_session::testing::Direction;
    dereth_testkit::corpus_steps::corpus(session)
        .blobs
        .into_iter()
        .filter(|b| b.dir == Direction::ServerToClient)
        .filter_map(|b| {
            if b.opcode == 0xF7B0 {
                let sub = dereth_testkit::corpus_steps::event_sub_type(&b)?;
                Some((sub, b.payload.get(16..)?.to_vec()))
            } else {
                Some((b.opcode, b.payload.get(4..)?.to_vec()))
            }
        })
        .collect()
}

/// The lines the shard's own transient strings put on the strip, derived from the recording.
fn strip_transient_lines(msgs: &[(u32, Vec<u8>)]) -> Vec<String> {
    use dereth_protocol::Message as _;
    msgs.iter()
        .filter(|(op, _)| *op == dereth_protocol::Opcode::COMMUNICATION_TRANSIENT_STRING.0)
        .filter_map(|(_, body)| {
            let mut r = dereth_protocol::archive::Reader::new(body);
            dereth_protocol::comms::CommunicationTransientString::read(&mut r)
                .ok()
                .map(|m| m.text)
        })
        .collect()
}

/// The lines the shard's **failure** events put on the strip, derived the same way: the reason
/// each of them carries picks an arm, and only the arms whose channel is the strip's own land
/// here. The channel is read from the client's own table rather than assumed, which is what keeps
/// the recording's six chat-channel failures out of this count.
fn strip_failure_lines(msgs: &[(u32, Vec<u8>)]) -> Vec<String> {
    use dereth_protocol::Message as _;
    msgs.iter()
        .filter_map(|(op, body)| {
            let mut r = dereth_protocol::archive::Reader::new(body);
            let (code, text) = if *op == dereth_protocol::Opcode::ITEM_USE_DONE.0 {
                (
                    dereth_protocol::objects::ItemUseDone::read(&mut r)
                        .ok()?
                        .failure_type,
                    String::new(),
                )
            } else if *op == dereth_protocol::Opcode::COMMUNICATION_WEENIE_ERROR.0 {
                (
                    dereth_protocol::comms::CommunicationWeenieError::read(&mut r)
                        .ok()?
                        .error_type,
                    String::new(),
                )
            } else if *op == dereth_protocol::Opcode::COMMUNICATION_WEENIE_ERROR_WITH_STRING.0 {
                let m =
                    dereth_protocol::comms::CommunicationWeenieErrorWithString::read(&mut r).ok()?;
                (m.error_type, m.text)
            } else if *op == dereth_protocol::Opcode::CHARACTER_SERVER_SAYS_ATTEMPT_FAILED.0 {
                // The attempt-failed arm's second half: every reason but the seven whose object
                // line says it all is a failure event too, named object or not.
                let m =
                    dereth_protocol::objects::CharacterServerSaysAttemptFailed::read(&mut r).ok()?;
                if dereth_protocol::objects::CharacterServerSaysAttemptFailed::suppresses_generic_text(
                    m.reason,
                ) {
                    return None;
                }
                (m.reason, String::new())
            } else {
                return None;
            };
            let (ty, arm) = dereth_ui_screens::chat::failure::arm_for(code)?;
            (ty == dereth_ui_screens::hud::speech_bubbles::BUBBLE_CHAT_TYPE).then_some(())?;
            arm.render(&text)
                .map(|s| dereth_client::chat::add_text_to_scroll_trim(&s).to_owned())
        })
        .collect()
}

/// A model client with the recording's own endpoint under it, ready to be swept.
fn a_client_for(session: &str) -> HeadlessClient {
    let mut c = HeadlessClient::model();
    c.attach_replay(dereth_testkit::replay::recorded_endpoint(
        &dereth_testkit::replay::records(session),
    ));
    c
}

// ---------------------------------------------------------------------------------------------
// notice.refusal.every-recorded-refusal-and-nothing-else-reaches-the-strip
// ---------------------------------------------------------------------------------------------

/// Every recording, swept: what reaches the strip is exactly the refusals the client composed for
/// itself plus the two kinds of line the shard sends on the same channel, and nothing else.
///
/// **The strip has to be read while the recording is still running.** Every recording ends in a
/// log-off, and the ending empties the strip, so a sweep that replayed to the last datagram and
/// then looked would find nothing -- which is exactly how this assertion was once vacuously true.
/// The look between the datagrams stands in for the panel's own sweep, which a headless client
/// does not run.
pub(super) fn every_recorded_refusal_and_nothing_else_reaches_the_strip() {
    const ATTEMPT_FAILED: u32 = 0x00A0;

    let mut recordings = 0usize;
    let mut total = 0usize;
    let mut nameless_total = 0usize;
    let mut transient_total = 0usize;
    let mut failure_total = 0usize;
    let mut drawn_total = 0usize;
    let mut every_session_holds = true;
    let mut lines: Vec<String> = Vec::new();
    let mut expected_named: Vec<String> = Vec::new();

    for session in dereth_testkit::corpus_steps::sessions() {
        let msgs = strip_messages(session);
        let refusals: Vec<&(u32, Vec<u8>)> = msgs
            .iter()
            .filter(|(op, _)| *op == ATTEMPT_FAILED)
            .collect();
        if refusals.is_empty() {
            continue;
        }
        recordings += 1;
        let n = refusals.len();
        // How many of them name no object at all. A refusal about nothing draws nothing, which is
        // the client's own guard and not an accident of the fixture.
        let nameless = refusals
            .iter()
            .filter(|(_, body)| body.get(..4).is_some_and(|b| b == [0, 0, 0, 0]))
            .count();
        let transient = strip_transient_lines(&msgs);
        let failures = strip_failure_lines(&msgs);

        let mut c = a_client_for(session);
        let before = c.view().hud().stats.spew_lines;
        let mut seen: Vec<String> = Vec::new();
        dereth_testkit::corpus_steps::sweep(&mut c, session, |c| {
            seen.append(&mut c.hud_mut().panels.spew.model.pending);
        });
        // One more pass: a notice raised by the second half of a pass is drawn by the first half
        // of the next, which is the one pass of latency the scroll documents rather than hides.
        c.tick(1);
        let drawn = usize::try_from(c.view().hud().stats.spew_lines - before).unwrap_or(usize::MAX);
        every_session_holds &= drawn == n - nameless + transient.len() + failures.len();

        seen.extend(c.view().hud().panels.spew.model.pending.iter().cloned());
        seen.extend(c.view().hud().panels.spew.model.items.iter().cloned());
        every_session_holds &= seen.len() == drawn;

        lines.extend(seen);
        expected_named.extend(transient.iter().cloned());
        expected_named.extend(failures.iter().cloned());
        total += n;
        nameless_total += nameless;
        transient_total += transient.len();
        failure_total += failures.len();
        drawn_total += drawn;
        c.shutdown();
    }

    let carried = recordings > 0 && total > 0;

    // The census, against the corpus rather than against a number typed here: every refusal the
    // recordings carry, one of which names nothing and draws nothing.
    // Thirteen failure lines: the shard's twelve failure events, and the one recorded refusal
    // whose reason (`0x36`, action cancelled) is also a failure event of the attempt-failed arm.
    let census = total == 6 && nameless_total == 1 && transient_total == 4 && failure_total == 13;
    let all_of_them_drew =
        drawn_total == (total - nameless_total) + transient_total + failure_total;

    // **The refusals that ARE drawn are empty, and that is the client's answer rather than a lost
    // string.** The line is a name and a reason: a replay of the shard's own traffic never made a
    // request, so there is no name; and of the reasons the recordings carry only one is a reason
    // the client has a word for, so one of the five carries that word and the rest carry nothing.
    let bubbles = drawn_total - transient_total - failure_total;
    let suffixed = dereth_client_model::inventory::requests::attempt_failed_suffix(0x36)
        .trim()
        .to_owned();
    let (empty, named): (Vec<String>, Vec<String>) =
        lines.into_iter().partition(std::string::String::is_empty);
    let four_empty_and_one_suffixed =
        bubbles == 5 && !suffixed.is_empty() && empty.len() == bubbles - 1;

    let mut named = named;
    named.sort();
    expected_named.push(suffixed);
    expected_named.sort();
    let the_rest_is_the_shards_own = named == expected_named;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "notice.refusal.every-recorded-refusal-and-nothing-else-reaches-the-strip",
        move |_| {
            carried
                && every_session_holds
                && census
                && all_of_them_drew
                && four_empty_and_one_suffixed
                && the_rest_is_the_shards_own
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// notice.refusal.a-refusal-about-something-the-player-asked-for-names-it-and-says-why
// ---------------------------------------------------------------------------------------------

/// ...and the named form, which is what a player actually reads. The same message and the same
/// handler, with the object in the tables and a real request of the player's outstanding -- both
/// of which a replay of the shard's own traffic cannot have, and which is why every refusal in the
/// scenario above is empty.
///
/// The object is a **recorded** one, taken out of the table the recording built, and the refusal
/// is a **recorded** message with its two numbers rewritten, so the envelope and the field
/// positions are the shard's own rather than this file's idea of them.
pub(super) fn a_refusal_about_something_the_player_asked_for_names_it_and_says_why() {
    const ATTEMPT_FAILED: u32 = 0x00A0;
    const SESSION: &str = "long-solo-play";
    /// The one reason in the ladder this scenario asks for: too much to carry.
    const TOO_ENCUMBERED: u32 = 0x2A;

    // The shard's own message, from the recording, with its shape asserted before it is edited.
    let recorded = strip_messages(SESSION)
        .into_iter()
        .find(|(op, _)| *op == ATTEMPT_FAILED)
        .map(|(_, body)| body)
        .expect("this recording carries a refusal");
    let shaped = recorded.len() == 8;

    let mut c = a_client_for(SESSION);
    let found = dereth_testkit::corpus_steps::sweep_until(&mut c, SESSION, |c| {
        c.view()
            .world()
            .tables
            .weenies
            .iter()
            .map(|(id, o)| {
                (
                    id,
                    o.object_name(dereth_client_model::weenie::NameType::Appropriate),
                )
            })
            .find(|(_, n)| !n.is_empty())
    });
    let (id, want_name) = found.expect("the recording builds a named object table while in world");

    // The player asks for something, which is what gives the refusal a shape to be drawn in.
    {
        let mut req = dereth_client_model::RecordingRequests::default();
        let mut sink = dereth_client_model::NullSink;
        let w = c.world_mut();
        let _ = w.attempt_wield(
            &mut req,
            &mut sink,
            id,
            dereth_client_model::inventory::slots::loc::MELEE_WEAPON,
            dereth_client_model::inventory::SplitState::default(),
            dereth_primitives::ServerTime(1.0),
            true,
        );
    }

    let mut body = recorded;
    body[0..4].copy_from_slice(&id.0.to_le_bytes());
    body[4..8].copy_from_slice(&TOO_ENCUMBERED.to_le_bytes());
    let mut blob = ATTEMPT_FAILED.to_le_bytes().to_vec();
    blob.extend_from_slice(&body);

    let before = c.view().hud().stats.spew_lines;
    c.when(dereth_testkit::Inbound::event(
        dereth_client_net::client_session::SessionEvent::UiEvent {
            opcode: dereth_protocol::Opcode(ATTEMPT_FAILED),
            blob,
        },
    ));
    c.tick(1);

    // Two lines: the reason as a failure event, which is not suppressed for this code, and the
    // object's own line naming it.
    let drew = c.view().hud().stats.spew_lines == before + 2;
    let want = format!("The {want_name} can't be wielded - you are too encumbered");
    let names_it = c.view().hud().panels.spew.model.pending
        == vec!["You are too encumbered to carry that!".to_owned(), want];

    c.assert_behaviour(
        "notice.refusal.a-refusal-about-something-the-player-asked-for-names-it-and-says-why",
        move |_| shaped && drew && names_it,
    );
    c.shutdown();
}
