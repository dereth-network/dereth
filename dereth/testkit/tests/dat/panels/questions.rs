use super::*;

// =============================================================================================
// dialog.confirmation.*
//
// Every yes-or-no question a shard asks is shown to the player, and the answer goes back on the
// wire: the message's decoder, the answer's encoder and the dialog are all on the live path.
//
// These are dialogs rather than panels, which is a different shape from the rest of this file --
// an open dialog's subtree is not under the current screen's roots, so [`UiSnapshot`] cannot see
// it and `Target::Element` cannot reach its buttons. [`dialog_prompt`] and [`answer_the_dialog`]
// above walk the dialog queue instead.
// =============================================================================================

/// The kinds of question a shard can ask, by the number it names them with.
const ASK_SWEAR: i32 = 1;
const ASK_FELLOWSHIP: i32 = 4;
const ASK_CRAFT: i32 = 5;
const ASK_YES_NO: i32 = 7;

/// The shipped confirmation root's two buttons.
const DIALOG_YES_BUTTON: ElementId = ElementId(0x17);
pub(super) const DIALOG_NO_BUTTON: ElementId = ElementId(0x19);

/// The shard asks a question.
fn ask(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    kind: i32,
    context: u32,
    text: &str,
) {
    peer.event(
        c,
        &dereth_protocol::comms::CharacterConfirmationRequest {
            confirmation_type: kind,
            context_id: context,
            text: text.to_owned(),
        },
    );
    c.tick(6);
}

/// The shard takes its question back.
fn stop_asking(c: &mut HeadlessClient, peer: &mut dereth_testkit::Peer, kind: i32, context: u32) {
    peer.event(
        c,
        &dereth_protocol::comms::CharacterConfirmationDone {
            confirmation_type: kind,
            context_id: context,
        },
    );
    c.tick(6);
}

/// Whether a question is on the screen at all.
fn a_question_is_open(c: &mut HeadlessClient) -> bool {
    c.app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .dialogs
        .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)
        .is_some_and(|d| d.element.is_some())
}

/// Every answer this client has put on the wire, as `(kind, context, accepted)`.
fn answers(c: &HeadlessClient) -> Vec<(i32, u32, i32)> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::ConfirmationResponse(m) => {
                Some((m.confirmation_type, m.context_id, m.accepted))
            }
            _ => None,
        })
        .collect()
}

/// **The headline.** A question an NPC asks appears on screen word for word, nothing is sent
/// until the player answers, and both answers are messages -- a refusal is a refusal and not a
/// silence, which is what stops the shard sitting out its own timeout.
pub(super) fn an_npc_question_appears_and_both_answers_reach_the_shard() {
    let (mut c, mut peer) = a_client_and_a_shard();
    let mut hand = dereth_testkit::adapters_chat::Hand::new();

    ask(
        &mut c,
        &mut peer,
        ASK_YES_NO,
        0x0000_002A,
        "Do you wish to join the Hand of Destiny?",
    );
    let up = a_question_is_open(&mut c);
    // The plainest kind of question is passed through untouched, which is why it reads as the
    // one who asked it wrote it.
    let verbatim =
        dialog_prompt(&mut c).as_deref() == Some("Do you wish to join the Hand of Destiny?");
    let nothing_yet = answers(&c).is_empty();

    answer_the_dialog(&mut c, &mut hand, DIALOG_YES_BUTTON);
    let said_yes = answers(&c) == vec![(ASK_YES_NO, 0x0000_002A, 1)];
    let gone = !a_question_is_open(&mut c);

    // The other half of the same gesture, on a fresh question.
    ask(
        &mut c,
        &mut peer,
        ASK_YES_NO,
        0x0000_0100,
        "Shall I tinker with your armour?",
    );
    answer_the_dialog(&mut c, &mut hand, DIALOG_NO_BUTTON);
    let said_no = answers(&c) == vec![(ASK_YES_NO, 0x0000_002A, 1), (ASK_YES_NO, 0x0000_0100, 0)];
    let gone_again = !a_question_is_open(&mut c);
    // ...and both really left as datagrams rather than only reaching the outbox.
    let framed = c
        .outbound_wire()
        .iter()
        .filter(|s| **s == dereth_protocol::Opcode::CHARACTER_CONFIRMATION_RESPONSE.0)
        .count()
        == 2;

    c.assert_behaviour(
        "dialog.confirmation.a-question-appears-and-both-answers-reach-the-shard",
        move |_| {
            up && verbatim && nothing_yet && said_yes && gone && said_no && gone_again && framed
        },
    );
    c.shutdown();
}

/// A question about something being made adds the client's own *Continue?* to what the shard
/// wrote, where the plainest kind adds nothing -- so the two arms are told apart by what is on the
/// screen and not by which arm was taken.
pub(super) fn a_making_question_gets_the_clients_own_continue() {
    let (mut c, mut peer) = a_client_and_a_shard();
    let mut hand = dereth_testkit::adapters_chat::Hand::new();

    ask(
        &mut c,
        &mut peer,
        ASK_CRAFT,
        7,
        "This may destroy the item.",
    );
    let suffixed = dialog_prompt(&mut c).as_deref() == Some("This may destroy the item. Continue?");

    // Answering no is the safe half of this gesture: a yes here is the irreversible one.
    answer_the_dialog(&mut c, &mut hand, DIALOG_NO_BUTTON);
    let refused = answers(&c) == vec![(ASK_CRAFT, 7, 0)];

    ask(
        &mut c,
        &mut peer,
        ASK_YES_NO,
        8,
        "This may destroy the item.",
    );
    let plain = dialog_prompt(&mut c).as_deref() == Some("This may destroy the item.");

    c.assert_behaviour(
        "dialog.confirmation.a-question-about-making-something-adds-the-clients-own-continue",
        move |_| suffixed && refused && plain,
    );
    c.shutdown();
}

/// **The client has no timer of its own.** The only thing that takes an unanswered question down
/// is the shard withdrawing it -- and when it does, the client refuses on the way out rather than
/// leaving the shard to time it out. A withdrawal naming a different question is ignored outright.
pub(super) fn the_shards_withdrawal_closes_the_question_and_refuses_on_the_way_out() {
    let (mut c, mut peer) = a_client_and_a_shard();

    ask(&mut c, &mut peer, ASK_YES_NO, 0x0000_0300, "Well?");
    stop_asking(&mut c, &mut peer, ASK_YES_NO, 0x0000_0301);
    let still_open = a_question_is_open(&mut c);
    let nothing_answered = answers(&c).is_empty();

    stop_asking(&mut c, &mut peer, ASK_YES_NO, 0x0000_0300);
    let closed = !a_question_is_open(&mut c);
    let refused = answers(&c) == vec![(ASK_YES_NO, 0x0000_0300, 0)];

    c.assert_behaviour(
        "dialog.confirmation.the-shards-withdrawal-closes-it-and-refuses-on-the-way-out",
        move |_| still_open && nothing_answered && closed && refused,
    );
    c.shutdown();
}

/// A second question while one is already up puts nothing new on screen and waits behind nothing,
/// and the first question's words stay up -- but the client now holds the **second** question's
/// handle. So answering answers the second question, a withdrawal of the first no longer takes
/// the dialog down, and a withdrawal of the second does, refusing on the way out.
pub(super) fn a_second_question_while_one_is_open_takes_over_its_handle() {
    let (mut c, mut peer) = a_client_and_a_shard();
    let mut hand = dereth_testkit::adapters_chat::Hand::new();

    ask(&mut c, &mut peer, ASK_YES_NO, 0x0000_0400, "First?");
    let first = c
        .app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .dialogs
        .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)
        .expect("the first question is up")
        .context;

    ask(&mut c, &mut peer, ASK_YES_NO, 0x0000_0401, "Second?");
    let (still_first, waiting) = {
        let ui = &c.app_mut().ui().expect("the shell is up").ui;
        (
            ui.dialogs
                .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)
                .expect("still the first")
                .context
                == first,
            ui.dialogs
                .waiting_on(dereth_ui::dialog::factory::DEFAULT_QUEUE),
        )
    };
    let reads_first = dialog_prompt(&mut c).as_deref() == Some("First?");

    answer_the_dialog(&mut c, &mut hand, DIALOG_YES_BUTTON);
    // One answer, carrying the second question's handle.
    let answered_second = answers(&c) == vec![(ASK_YES_NO, 0x0000_0401, 1)];

    // The two withdrawals, on a fresh pair: the first question's no longer matches the handle
    // the client holds and leaves the dialog up; the second question's takes it down, refused.
    ask(&mut c, &mut peer, ASK_YES_NO, 0x0000_0500, "Third?");
    ask(&mut c, &mut peer, ASK_YES_NO, 0x0000_0501, "Fourth?");
    stop_asking(&mut c, &mut peer, ASK_YES_NO, 0x0000_0500);
    let first_withdrawal_ignored =
        a_question_is_open(&mut c) && answers(&c) == vec![(ASK_YES_NO, 0x0000_0401, 1)];
    stop_asking(&mut c, &mut peer, ASK_YES_NO, 0x0000_0501);
    let second_withdrawal_closes = !a_question_is_open(&mut c)
        && answers(&c) == vec![(ASK_YES_NO, 0x0000_0401, 1), (ASK_YES_NO, 0x0000_0501, 0)];

    c.assert_behaviour(
        "dialog.confirmation.a-second-question-while-one-is-open-takes-over-its-handle-and-is-not-queued",
        move |_| {
            still_first
                && waiting == 0
                && reads_first
                && answered_second
                && first_withdrawal_ignored
                && second_withdrawal_closes
        },
    );
    c.shutdown();
}

/// Two of the seven kinds belong to panels of their own, and each asks its **own** question rather
/// than falling through to the plain one: an invitation to a fellowship names whoever sent it, and
/// somebody swearing to the player is asked about in the allegiance panel's own words -- which is
/// the discriminator, because the plain dialog would have put a question on screen too.
///
/// A kind the client knows nothing about raises nothing and answers nothing.
pub(super) fn an_invitation_and_a_swearing_raise_their_own_panels_questions() {
    let (mut c, mut peer) = a_client_and_a_shard();
    let mut hand = dereth_testkit::adapters_chat::Hand::new();

    ask(&mut c, &mut peer, ASK_FELLOWSHIP, 9, "Larktest");
    let invite_up =
        a_question_is_open(&mut c) && dialog_prompt(&mut c).is_some_and(|p| p.contains("Larktest"));
    let nothing_yet = answers(&c).is_empty();
    answer_the_dialog(&mut c, &mut hand, DIALOG_YES_BUTTON);
    let invite_answered = answers(&c) == vec![(ASK_FELLOWSHIP, 9, 1)];

    ask(&mut c, &mut peer, ASK_SWEAR, 0x0000_0500, "Talins Three");
    let swear_up = a_question_is_open(&mut c);
    // **The words are the discriminator.** Sending this kind to the plain dialog would also put a
    // question on screen and would also answer it; only the allegiance panel composes this
    // sentence, out of the shipped table, with the shard's text as its one name.
    let swear_words = dialog_prompt(&mut c).as_deref()
        == Some("Talins Three would like to swear allegiance to you. Do you accept?");
    answer_the_dialog(&mut c, &mut hand, DIALOG_YES_BUTTON);
    let swear_answered = answers(&c) == vec![(ASK_FELLOWSHIP, 9, 1), (ASK_SWEAR, 0x0000_0500, 1)];
    let swear_gone = !a_question_is_open(&mut c);

    // A kind out of range: nothing at all.
    let unknown_is_unknown = !dereth_protocol::comms::CharacterConfirmationRequest::is_handled(8);
    ask(&mut c, &mut peer, 8, 11, "?");
    let nothing_raised = !a_question_is_open(&mut c)
        && answers(&c) == vec![(ASK_FELLOWSHIP, 9, 1), (ASK_SWEAR, 0x0000_0500, 1)];

    c.assert_behaviour(
        "dialog.confirmation.an-invitation-and-a-swearing-ask-their-own-panels-question",
        move |_| {
            invite_up
                && nothing_yet
                && invite_answered
                && swear_up
                && swear_words
                && swear_answered
                && swear_gone
                && unknown_is_unknown
                && nothing_raised
        },
    );
    c.shutdown();
}
