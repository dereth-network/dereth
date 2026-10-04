//! UI fixtures and scenarios for failures.

use super::*;
// ---------------------------------------------------------------------------------------------
// notice.failure.*
//
// Recalling and then moving aborts the teleport, and the client says so.
//
// The shard's refusals are one table of three hundred and forty-four arms, and an arm that is
// missing falls through a silence that reads exactly like the table's own miss. The scenarios
// below drive the whole table. Its three counts -- how many arms there are, how many say nothing,
// how many go to the strip -- are kept **as literals in the scenario**, so the generated rows have
// an oracle outside themselves.
// ---------------------------------------------------------------------------------------------

/// One refusal from the shard, in the envelope the client receives it in.
fn a_refusal(code: u32) -> dereth_testkit::Inbound {
    dereth_testkit::Inbound::message(&dereth_protocol::comms::CommunicationWeenieError {
        error_type: code,
    })
}

/// The same refusal carrying a word of the shard's own.
fn a_refusal_naming(code: u32, text: &str) -> dereth_testkit::Inbound {
    dereth_testkit::Inbound::message(
        &dereth_protocol::comms::CommunicationWeenieErrorWithString {
            error_type: code,
            text: text.to_owned(),
        },
    )
}

/// Hand one refusal to the client and answer the lines it composed -- before any surface sees
/// them, which is what makes the surface half below a separate question.
fn lines_for(
    c: &mut HeadlessClient,
    e: dereth_testkit::Inbound,
) -> Vec<dereth_ui_screens::chat::interface::ChatMessage> {
    let event = match e {
        dereth_testkit::Inbound::Event(b) => *b,
        _ => unreachable!("built by the two helpers above"),
    };
    c.app_mut().apply_hud_events(&[event])
}

/// The line the client would end up with for an arm: what the arm composes, then the trim the
/// client performs as its first act on any line.
fn composed(arm: dereth_ui_screens::chat::failure::Arm, text: &str) -> Option<String> {
    arm.render(text)
        .map(|s| dereth_client::chat::add_text_to_scroll_trim(&s).to_owned())
}

/// A recall broken by moving says so, on the strip and not in the scrollback.
pub(super) fn a_recall_broken_by_moving_says_so_on_the_strip() {
    use dereth_ui_screens::chat::failure::{LOCAL_ERROR_CHAT_TYPE, YOU_HAVE_MOVED_TOO_FAR};
    use dereth_ui_screens::hud::speech_bubbles::LIST_BOX;

    /// The words the client draws, as a literal.
    const LINE: &str = "You have moved too far!";

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let before: Vec<String> = c
        .ui_snapshot()
        .rows_of(LIST_BOX)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let log_before = c
        .ui_snapshot()
        .text_of(dereth_ui_screens::chat::window::LOG)
        .to_owned();
    let nothing_said_it = !before.iter().any(|t| t == LINE);

    let produced = lines_for(&mut c, a_refusal(YOU_HAVE_MOVED_TOO_FAR));
    c.tick(4);

    // The line the receiving path made, before any surface saw it.
    let one_line = produced.len() == 1
        && produced[0].body == LINE
        && produced[0].ty == LOCAL_ERROR_CHAT_TYPE
        && produced[0].window == 0;

    // ...and the surface. The strip takes it; the scrollback's own filter does not.
    let after = c.ui_snapshot();
    let bubbles: Vec<String> = after
        .rows_of(LIST_BOX)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let log = after
        .text_of(dereth_ui_screens::chat::window::LOG)
        .to_owned();

    c.assert_behaviour(
        "notice.failure.a-recall-broken-by-moving-says-so-on-the-strip",
        move |_| {
            nothing_said_it
                && one_line
                && bubbles.iter().any(|t| t == LINE)
                && bubbles.len() == before.len() + 1
                && !log.contains(LINE)
                && log == log_before
        },
    );
    c.shutdown();
}

/// Every refusal the shard can send draws its own line on the surface its own kind names.
pub(super) fn every_refusal_draws_its_own_line_on_its_own_surface() {
    use dereth_ui_screens::chat::failure::{arm_for, ARMS, LOCAL_ERROR_CHAT_TYPE};

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));

    let mut drawn = 0usize;
    let mut silent = 0usize;
    let mut to_the_strip = 0usize;
    let mut every_arm_holds = true;
    for &(code, ty, arm) in &ARMS {
        let before = c.view().expect_app().hud().stats.spew_lines;
        let produced = lines_for(&mut c, a_refusal(code));
        let moved = c.view().expect_app().hud().stats.spew_lines - before;
        match composed(arm, "") {
            None => {
                every_arm_holds &= produced.is_empty() && moved == 0;
                silent += 1;
            }
            Some(line) => {
                every_arm_holds &= produced.len() == 1
                    && produced[0].body == line
                    && produced[0].ty == ty
                    // The strip takes a line if and only if it is of the client's own kind, and
                    // it tests nothing else.
                    && moved == u64::from(ty == LOCAL_ERROR_CHAT_TYPE);
                drawn += 1;
                if ty == LOCAL_ERROR_CHAT_TYPE {
                    to_the_strip += 1;
                }
            }
        }
    }

    // ...and a code the shard could send that the table has no arm for draws nothing.
    let mut no_arm_no_line = true;
    for code in [
        0x0000_u32,
        0x0016,
        0x0417,
        0x04DB,
        0x051D,
        0x0594,
        0xFFFF_FFFF,
    ] {
        let before = c.view().expect_app().hud().stats.spew_lines;
        let produced = lines_for(&mut c, a_refusal(code));
        no_arm_no_line &= arm_for(code).is_none()
            && produced.is_empty()
            && c.view().expect_app().hud().stats.spew_lines == before;
    }

    c.assert_behaviour(
        "notice.failure.every-refusal-the-shard-can-send-draws-its-own-line-on-its-own-surface",
        move |_| {
            every_arm_holds
            && no_arm_no_line
            // The three shapes of the shipped table, as literals: a scenario that read them back
            // out of the table would be comparing it with itself.
            && drawn + silent == 344
            && silent == 3
            && to_the_strip == 120
        },
    );
    c.shutdown();
}

/// A refusal carrying a word of the shard's own puts it into the line.
pub(super) fn a_refusal_carrying_the_shards_word_puts_it_in_the_line() {
    use dereth_ui_screens::chat::failure::{Arm, ARMS};

    const WHO: &str = "Frundi";

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut substituting = 0usize;
    let mut every_one_holds = true;
    for &(code, ty, arm) in &ARMS {
        let uses_text = match arm {
            Arm::Lit(_) | Arm::Silent => false,
            Arm::Fmt(f) | Arm::FmtOr(f, _) => f.contains("%s"),
            Arm::Suffix(_) | Arm::Wrap(_, _) | Arm::SuffixMid(_, _) | Arm::Text => true,
        };
        if !uses_text {
            continue;
        }
        substituting += 1;
        let produced = lines_for(&mut c, a_refusal_naming(code, WHO));
        let line = composed(arm, WHO).expect("an arm that draws a line");
        every_one_holds &= produced.len() == 1
            && produced[0].body == line
            && line.contains(WHO)
            && produced[0].ty == ty;
    }

    c.assert_behaviour(
        "notice.failure.a-refusal-carrying-the-shards-own-word-puts-it-in-the-line",
        move |_| {
            // The denominator: over a hundred of the arms read the word, so a loop that found none
            // would pass in silence.
            every_one_holds && substituting > 100
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// notice.failure.a-refused-portal-says-so-in-the-chat-log-in-its-own-colour
// ---------------------------------------------------------------------------------------------

/// A portal that refuses the player puts its refusal in the chat log.
pub(super) fn a_refused_portal_says_so_in_the_chat_log() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let log = hud_find(&c, dereth_ui_screens::chat::window::LOG);
    let nothing_yet = !drawn_text(&mut c, log).contains("complete a quest");

    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::comms::CommunicationWeenieError {
            error_type: dereth_ui_screens::chat::failure::YOU_MUST_COMPLETE_QUEST_TO_USE_PORTAL,
        },
    ));
    c.tick(1);

    let needle = "You must complete a quest to interact with that portal.";
    let said = drawn_text(&mut c, log).contains(needle);
    // The colour is part of the claim and not decoration: the same sentence in the wrong colour
    // is a different line to a player, and one shared colour for every refusal would be green
    // where this one is light blue.
    let in_its_colour = drawn_colour(&mut c, log, needle) == 0xFF3F_BFFF;

    // The other door into the same answer -- the refusal a use gets back -- really produces a
    // line, and every window's filter then refuses it. That is a seam rather than a fix, and it
    // is asserted as it is so the day it is settled this says the arm was already right.
    let dropped = c.view().expect_app().hud().stats.chat_lines_dropped;
    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::objects::ItemUseDone {
            failure_type: dereth_ui_screens::chat::failure::ACTION_CANCELLED,
        },
    ));
    c.tick(1);
    let produced_and_filtered = c.view().expect_app().hud().stats.chat_lines_dropped == dropped + 1
        && !dereth_ui_screens::chat::interface::ChatInterface::new(
            dereth_ui_screens::chat::interface::window::MAIN,
        )
        .type_is_active(dereth_ui_screens::chat::failure::LOCAL_ERROR_CHAT_TYPE);

    c.assert_behaviour(
        "notice.failure.a-refused-portal-says-so-in-the-chat-log-in-its-own-colour",
        move |_| nothing_yet && said && in_its_colour && produced_and_filtered,
    );
    c.shutdown();
}
