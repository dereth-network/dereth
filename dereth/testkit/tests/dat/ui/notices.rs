//! UI fixtures and scenarios for notices.

use super::*;
// -------------------------------------------------------------------------------------------
// 15. notice.bubble.clears-itself-after-five-seconds
// -------------------------------------------------------------------------------------------

/// A notice bubble goes away on its own, five seconds later, with nobody at the keyboard.
///
/// The other direction is checked too -- a second in, the bubble is still there -- without which
/// this would hold on a strip that deleted everything at once. The strip is read off the
/// **element tree** as well as off the model, which is what makes "it went away" a drawn result.
pub(super) fn the_notice_bubble_clears_itself() {
    use dereth_client_runtime::platform::clock::HEADLESS_STEP;
    use dereth_ui_screens::hud::speech_bubbles::{BUBBLE_CHAT_TYPE, LIST_BOX};

    /// The bubble's own lifetime, off the shipped layout.
    const LIFETIME_SECS: f64 = 5.0;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    assert!(
        c.view()
            .expect_app()
            .hud()
            .panels
            .spew
            .model
            .items
            .is_empty(),
        "the strip starts empty"
    );
    assert!(
        c.ui_snapshot().rows_of(LIST_BOX).is_empty(),
        "and nothing is drawn in it"
    );

    let want = dereth_client_model::magic::messages::casting("Flame Bolt VI");
    c.world_mut()
        .scroll
        .add_text_to_scroll(&want, u32::from(BUBBLE_CHAT_TYPE), false, 0);
    // One frame for the HUD to drain the scroll, one for the panel to build the element.
    c.tick(3);
    let up = c.view().expect_app().hud().panels.spew.model.items.clone();
    let drawn: Vec<String> = c
        .ui_snapshot()
        .rows_of(LIST_BOX)
        .into_iter()
        .map(str::to_owned)
        .collect();

    // A second in, with four to go: still there. Without this the claim below would hold on a
    // strip that deleted every bubble the moment it was built.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let a_second = (1.0 / HEADLESS_STEP).ceil() as u64;
    c.tick(a_second);
    let still_there: Vec<String> = c
        .ui_snapshot()
        .rows_of(LIST_BOX)
        .into_iter()
        .map(str::to_owned)
        .collect();

    // Now nothing but time, and no further input at all.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (LIFETIME_SECS / HEADLESS_STEP).ceil() as u64 + 2;
    c.tick(frames);
    let gone = c.ui_snapshot().rows_of(LIST_BOX).is_empty();

    c.assert_behaviour("notice.bubble.clears-itself-after-five-seconds", move |v| {
        up == vec![want.clone()]
            && drawn.iter().any(|t| *t == want)
            && still_there.iter().any(|t| *t == want)
            && gone
            && v.expect_app().hud().panels.spew.model.items.is_empty()
    });
    c.shutdown();
}

// -------------------------------------------------------------------------------------------
// 16. notice.refusal.is-a-bubble-and-not-a-chat-line
// -------------------------------------------------------------------------------------------

/// A refusal the client composes is a bubble, and the chat scrollback never sees it.
///
/// It reads the seam counters on both sides of the queue, and the words back out of the **element
/// tree** and out of the chat log's own element rather than off the model. A claim about what the
/// player sees that is only ever read off a counter is a claim about a counter.
pub(super) fn a_refusal_is_a_bubble_and_not_a_chat_line() {
    use dereth_ui_screens::hud::speech_bubbles::LIST_BOX;

    let want = dereth_client_model::chat::SOMEONE_MUST_TELL_YOU_FIRST;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let chat_before = c.view().expect_app().hud().stats.chat_lines;
    let bubbles_before = c.ui_snapshot().rows_of(LIST_BOX).len();
    let log_before = chat_log_words(&mut c);

    // A reply with nobody to reply to: the client understands the command and refuses it, which
    // is a line it composes for itself rather than one the shard sent.
    c.when(Player::Say("@reply hello".to_owned()));
    // One frame for the refusal to reach the scroll, one for the HUD to drain it, one for the
    // panel to build the element.
    c.tick(3);

    // The queue it passed through is empty again: the line was handed on and not left in it.
    let the_queue_drained = c.view().world().scroll.pending().is_empty();
    // ...and it is not written to the client's own log file, which this channel never is.
    let nothing_logged = c.view().world().scroll.logged == 0;
    let drawn: Vec<String> = c
        .ui_snapshot()
        .rows_of(LIST_BOX)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let one_more_bubble = drawn.len() == bubbles_before + 1 && drawn.iter().any(|t| t == want);
    let log_after = chat_log_words(&mut c);
    let the_log_is_untouched = log_after == log_before && !log_after.contains(want);

    c.assert_behaviour("notice.refusal.is-a-bubble-and-not-a-chat-line", move |v| {
        let app = v.expect_app();
        let refused = app.interaction().stats.chat_commands_refused == 1;
        let in_the_strip = app.hud().stats.spew_lines == 1
            && app.hud().panels.spew.model.items.iter().any(|t| t == want);
        // The scrollback's own filter drops this channel, so the line that reached the strip must
        // not also have been routed to a chat window.
        let not_in_the_log = app.hud().stats.chat_lines == chat_before;
        refused
            && in_the_strip
            && not_in_the_log
            && the_queue_drained
            && nothing_logged
            && one_more_bubble
            && the_log_is_untouched
    });
    c.shutdown();
}

/// Everything the chat scrollback is drawing, as one string.
fn chat_log_words(c: &mut HeadlessClient) -> String {
    let h = hud_find(c, dereth_ui_screens::chat::window::LOG);
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell
        .ui
        .text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// notice.locked-container.*
//
// The three blobs below are the shard's own, recorded during a live session and carried over
// unchanged; nothing here is synthesised and nothing leaves this process.
// ---------------------------------------------------------------------------------------------

/// The live player of that session, and the chest they used.
const CHEST_PLAYER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_000D);
const CHEST: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x7A9B_4072);

/// The recorded object-create for the chest, with its openable bit clear.
const CREATE_OBJECT: &str = "45f7000072409b7a1100000003980100180400000c00000000003d00030000003d000c00000000002200b4a90000c6420000ec410000bc420000803f00000000000000000000000004000009210000202b0000347c00000200000000000000000000000000000000000000003e0020001800416c757669616e205061746877617264656e204368657374000000804983201000020000140000000000780ac4090000300000000000803f9e39";
/// The recorded sound the shard answered the use with.
const CHEST_SOUND: &str = "50f7000072409b7a940000000000803f";
/// The recorded acknowledgement that followed it.
const CHEST_USE_DONE: &str = "b0f700000d00005010000000c701000000000000";

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

/// One recorded blob, in the envelope it arrived in.
fn recorded(blob: &[u8]) -> dereth_testkit::Inbound {
    let op = dereth_protocol::Opcode(u32::from_le_bytes(
        blob[0..4].try_into().expect("an opcode"),
    ));
    if op.0 == 0xF7B0 {
        dereth_testkit::Inbound::event(dereth_client_net::client_session::SessionEvent::UiEvent {
            opcode: dereth_protocol::Opcode(u32::from_le_bytes(
                blob[12..16].try_into().expect("a sub-type"),
            )),
            blob: blob[12..].to_vec(),
        })
    } else {
        dereth_testkit::Inbound::event(
            dereth_client_net::client_session::SessionEvent::WorldObject {
                opcode: op,
                body: blob[4..].to_vec(),
            },
        )
    }
}

/// **The gate.** The recorded chest, the shipped Use button really pressed, the recorded answers:
/// the strip carries the client's own refusal, in its own order, and the scrollback does not.
pub(super) fn a_use_of_the_recorded_locked_chest_says_so_in_the_strip() {
    use dereth_protocol::types::PublicWeenieDesc;
    use dereth_ui_screens::hud::speech_bubbles::LIST_BOX;
    use {dereth_rules::weenie::bitfield, dereth_rules::weenie::item_type};

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    {
        let w = c.world_mut();
        let mut me = dereth_client_model::Weenie::new(CHEST_PLAYER);
        me.pwd = PublicWeenieDesc {
            bitfield: bitfield::PLAYER,
            obj_type: item_type::CREATURE,
            items_capacity: Some(102),
            containers_capacity: Some(7),
            ..PublicWeenieDesc::default()
        };
        me.pwd.name = "+Lark".to_owned();
        me.valid = true;
        w.tables.weenies.insert(CHEST_PLAYER, me);
        w.player = Some(CHEST_PLAYER);
        w.tables.inventories.insert(
            CHEST_PLAYER,
            dereth_client_model::objects::ObjectInventory::new(CHEST_PLAYER),
        );
    }
    c.when(recorded(&hex(CREATE_OBJECT)));
    c.tick(1);
    {
        let w = c.view().world();
        let chest = w.weenie(CHEST).expect("the recorded create made the chest");
        assert_eq!(
            chest.pwd.bitfield & bitfield::OPENABLE,
            0,
            "the recorded chest is locked, which is the premise of the whole scenario"
        );
    }
    {
        // What a viewport click on the chest calls.
        let mut sink = dereth_client_model::RecordingSink::default();
        c.world_mut()
            .set_selected_object(Some(CHEST), false, &mut sink);
    }
    c.tick(1);

    let max = c.view().expect_app().hud().panels.spew.model.max_concurrent;
    assert!(
        max >= 2,
        "the shipped strip must hold both lines for this to be readable: {max}"
    );
    let chat_before = c.view().expect_app().hud().stats.chat_lines;
    let refused_before = c.view().expect_app().interaction().stats.uses_refused;

    c.when(Player::click(
        dereth_ui_screens::toolbar::target_mode::USE_BUTTON,
    ));

    let after = c.ui_snapshot();
    after.assert_visible(LIST_BOX);
    after.assert_tree("locked_chest_bubble_strip", Some(LIST_BOX));
    // Unfiltered: an empty bubble is the defect this guards against, so a reader that dropped the
    // empty rows would be a reader that could not see it.
    let bubbles: Vec<String> = after
        .lines_of(LIST_BOX)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let strip = c.view().expect_app().hud().panels.spew.model.items.clone();
    let refused = c.view().expect_app().interaction().stats.uses_refused;
    let chat_after = c.view().expect_app().hud().stats.chat_lines;

    // The control: the shard's whole answer adds no line anywhere.
    c.when(recorded(&hex(CHEST_SOUND)));
    c.when(recorded(&hex(CHEST_USE_DONE)));
    c.tick(2);
    let settled: Vec<String> = c
        .ui_snapshot()
        .lines_of(LIST_BOX)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let chat_settled = c.view().expect_app().hud().stats.chat_lines;

    c.assert_behaviour(
        "notice.locked-container.a-use-of-a-locked-one-says-so-in-the-strip",
        move |_| {
            // Two lines, the refusal above the use in the model's own newest-first order, neither of
            // them empty -- the defect this guards against is an empty bubble.
            strip.len() == 2
            && strip[1].ends_with("is locked")
            && strip[0].starts_with("Using the")
            && !bubbles.is_empty()
            && !bubbles.iter().any(String::is_empty)
            // Not one of the use path's own refusals: the use was sent and the open was refused.
            && refused == refused_before
            // The channel the refusal is on does not reach the scrollback.
            && chat_after == chat_before
            && chat_settled == chat_before
            && settled == bubbles
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// notice.locked-container, the other half
//
// The refusal the *shard* sends, for a line no recording carries. The message is built here as
// the reference server frames it and handed to the client through the seam a decoded message
// reaches, so nothing is sent and nothing invented about the wire.
// ---------------------------------------------------------------------------------------------

/// The line the shard sends for a locked container.
const SHARD_REFUSAL: &str = "The Aluvian Pathwarden Chest is locked";

/// The shard's own transient line, in the envelope the client receives it in.
fn a_transient_line(text: &str) -> dereth_testkit::Inbound {
    dereth_testkit::Inbound::message(&dereth_protocol::comms::CommunicationTransientString {
        text: text.to_owned(),
    })
}

/// The shard's own refusal reaches the strip and stays out of the scrollback.
pub(super) fn the_shards_refusal_reaches_the_strip_and_not_the_scrollback() {
    use dereth_ui_screens::hud::speech_bubbles::LIST_BOX;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let before = c.ui_snapshot().rows_of(LIST_BOX).len();
    let log_before = c
        .ui_snapshot()
        .text_of(dereth_ui_screens::chat::window::LOG)
        .to_owned();

    c.when(a_transient_line(SHARD_REFUSAL));
    c.tick(4);

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
        "notice.locked-container.the-shards-own-refusal-reaches-the-strip-and-not-the-scrollback",
        move |_| {
            // Exactly one bubble joined the strip, and it is the shard's line...
            bubbles.iter().any(|t| t == SHARD_REFUSAL)
            && bubbles.len() == before + 1
            // ...and the scrollback did not change at all: this kind of line is bubble-only.
            && !log.contains(SHARD_REFUSAL)
            && log == log_before
        },
    );
    c.shutdown();
}

// =============================================================================================
// notice.a-swing-with-nothing-selected-says-so-on-the-strip
//
// The combat refusals do not go through the notice machinery at all -- they put their words
// straight into the same queue -- so they are a second entry into it. The other notice rows are
// in the `cpu` tier's file and on `notice.refusal.is-a-bubble-and-not-a-chat-line` above.
// =============================================================================================

/// A stance is the precondition, not decoration: nothing below the stance toggle runs at all while
/// the character is at peace, so a swing thrown there is consumed by nothing and says nothing.
pub(super) fn a_swing_with_nothing_selected_says_so_on_the_strip() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    c.world_mut().combat.combat_mode = dereth_client_model::combat::CombatMode::Melee;
    let nothing_said_yet = c.view().expect_app().interaction().last_refusal.is_none();

    c.when(Player::Press(dereth_input::ActionId(
        dereth_client_runtime::interaction::action::COMBAT_LOW_ATTACK,
    )));
    c.tick(2);

    let it_was_refused_in_words = c.view().expect_app().interaction().last_refusal.as_deref()
        == Some("You must select a valid combat target before attacking");
    let it_reached_the_queue = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .notice_strings_scrolled
        == 1;
    let it_reached_the_strip = c.view().expect_app().hud().stats.spew_lines == 1;

    c.assert_behaviour(
        "notice.a-swing-with-nothing-selected-says-so-on-the-strip",
        move |_| {
            nothing_said_yet
                && it_was_refused_in_words
                && it_reached_the_queue
                && it_reached_the_strip
        },
    );
    c.shutdown();
}
