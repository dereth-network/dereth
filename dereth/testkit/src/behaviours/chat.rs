//! Chat -- what reaches the chat window, and what leaves it.
//!
//! Speech and its earshot, emotes, the channels, the squelch table, the talk focus, and the
//! Turbine chat service's own channel.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "chat.aliases.typing-a-space-after-a-reply-alias-expands-it-into-a-tell",
        says: "Typing @r into the chat box does nothing until the space after it: then it becomes \
               @tell and the name of whoever last wrote followed by a comma and the space, with \
               the caret after it and the rest of the line typing on from there; a later space, or \
               an edited line, never expands again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O129-ALIASES"),
        station: "dereth-client::dat::chat::reply_targets::typing_the_space_after_an_alias_expands_it_into_a_tell",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.allegiance.refusal-is-its-own-and-the-option-rejoins-the-channel",
        says: "Typing the allegiance chat command with no allegiance channel says \"You are not in an \
               allegiance!\" and nothing is sent; the other six channel commands keep their own refusal \
               instead. Turning the listen-to-allegiance-chat option off and on again asks the shard, \
               and once the shard answers with the channel list the allegiance row can be selected and \
               the command reaches that room.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G29"),
        station: "dereth-testkit::cpu::chat::scenario_allegiance_chat_refusal_and_rejoin",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.channel-commands.every-word-sends-its-own-channel-and-says-nothing-locally",
        says: "There are nineteen words a player can type to talk on one of six channels -- the \
               allegiance, his co-vassals, his fellowship, his monarch, his patron and his vassals -- \
               and which channel a line goes to comes from the word he typed rather than from one \
               command deciding for all of them. The word itself is not part of what he said, and a \
               line that went out puts nothing in his own window.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O924"),
        station: "dereth-testkit::dat::chat::scenario_every_channel_word_sends_its_own_channel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.channel-commands.one-with-no-text-says-what-to-do-and-sends-nothing",
        says: "A channel command with nothing after it sends nothing and tells the player he must \
               say what he wants to broadcast. It does not tell him that what he typed was not a \
               command, which is a different answer and would be the wrong one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O924-EMPTY"),
        station: "dereth-testkit::dat::chat::scenario_an_empty_channel_command_says_what_to_do_and_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.channel.a-recorded-broadcast-that-names-its-speaker-is-drawn-in-its-own-colour",
        says: "A line a shard really broadcast on a channel, naming who said it, reaches the chat \
               log: the speaker's name drawn as something the player can click and in its own \
               colour, the rest of the line in the colour that kind of line is drawn in, and none \
               of the mark around the name drawn as letters.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-129"),
        station: "dereth-testkit::dat::chat::scenario_a_recorded_broadcast_that_names_its_speaker_is_drawn",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.channel.each-channel-has-its-own-wording-and-type",
        says: "A line broadcast on a channel is drawn with that channel's own wording and its own \
               colour: your own vassal line echoes back as you saying it to your vassals, a vassal, \
               patron or follower line names them and says it is to you, the fellowship and \
               allegiance-wide channels are bracketed, an unknown channel is named as unknown, and a \
               fellowship broadcast is the bare text with no quotes at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-2"),
        station: "dereth-testkit::cpu::chat::scenario_channel_broadcast_wording_per_channel",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.channel.every-channel-draws-its-own-line-in-its-own-colour",
        says: "Each channel a player can speak on -- his fellowship, his vassals, his patron, his \
               monarch, his co-vassals, the allegiance, help, abuse, and one the client does not \
               know -- draws its own line on the log in the colour its kind of line is drawn in, \
               with the speaker's name clickable. Four distinct colours across the nine, because \
               two of the kinds really do share one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-129-COLOURS"),
        station: "dereth-testkit::dat::chat::scenario_every_channel_draws_its_own_line_in_its_own_colour",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.channel.silencing-the-speaker-does-not-silence-a-channel-line",
        says: "Silencing somebody does not silence what he says on a channel: a channel line \
               carries no way of saying who said it, so the question the client asks cannot be \
               about him. Silencing the kind of line does silence it, and it is dropped before it \
               is even composed -- but only for a kind the client will answer that question about \
               at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-129-SQUELCH"),
        station: "dereth-testkit::dat::chat::scenario_a_squelched_speaker_is_still_heard_on_a_channel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.channels.the-list-replies-print-into-the-log",
        says: "The shard's two channel replies print into the chat log: one heads its list The \
               following characters are currently listening on the channel:, the other The \
               following channels are available to you:, with one name to a line under the \
               heading.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P24-CHANNELS"),
        station: "dereth-client::gpu::chat::channel_and_age_replies::the_two_channel_replies_print_their_lists_into_the_chat_log",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "chat.classic.global-room-callbacks-reach-the-active-chat-window",
        says: "General and Trade room callbacks reach the active Classic chat window once; callbacks outside gameplay and pending lines at logoff do not enter the next character's chat.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-CLASSIC-GLOBAL-CHAT"),
        station: "dereth-client-shell::lib::front_end::message_tests::classic_global_room_callbacks_reach_chat_once_and_stop_outside_gameplay",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.commands.a-command-the-client-handles-is-never-spoken-to-the-shard",
        says: "A line that is a command the client handles itself is never said out loud, whichever \
               channel the talk-to menu is on -- saying a command on a channel everybody reads would \
               be both wrong and rude.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O109-LOCAL"),
        station: "dereth-testkit::dat::chat::scenario_a_command_the_client_handles_is_never_spoken",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.commands.a-verb-the-client-knows-answers-it-and-a-word-it-does-not-know-is-passed-on",
        says: "A verb the client knows is answered by the client and never counted as one it has \
               not written: the one that explains itself writes two lines, the one that asks before \
               it acts raises its question, the one that lists friends lists them and the one that \
               reads a position says its own sentence when it is given something it cannot read. A \
               word that is not in the table at all is passed to the shard exactly as typed, and \
               the one arm that really does refuse still says so, which is what makes the rest a \
               reading rather than a reader that cannot see.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O167-VERB"),
        station: "dereth-testkit::cpu::chat::scenario_a_verb_the_client_knows_is_not_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.commands.every-wired-command-reaches-the-wire-from-a-typed-line",
        says: "Every chat command that asks the shard for something -- recall to the lifestone, \
               the marketplace or the house, the arenas, age and birth, turning chat on and off \
               and the rest -- sends exactly its own request with its own body when typed, and \
               none falls through to the unknown-command answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-COMMANDS"),
        station: "dereth-client::dat::chat::chat_commands::every_wired_chat_command_reaches_the_wire_from_a_typed_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.death.a-death-the-player-was-part-of-reaches-the-log-only-when-he-was-not",
        says: "Somebody else's death is drawn in the chat log in the colour a plain line is drawn \
               in. A death the player was part of is not, whether he was the one killed, the one \
               who killed, or both -- because the shard tells everybody nearby including him, and \
               he is already being told about it another way. A client that does not know yet who \
               its player is draws it, because the test is about him not being involved rather than \
               about there being a player at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-128-GUARD"),
        station: "dereth-testkit::dat::chat::scenario_a_death_you_were_part_of_reaches_the_log_only_when_you_were_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.death.a-third-partys-death-is-announced-and-your-own-is-not",
        says: "When somebody else nearby is killed, the announcement the shard sends appears in the \
               chat window. Your own death and your own kill produce no line from it, because other \
               messages carry those; an empty announcement prints nothing, and a truncated one is \
               refused rather than drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O937-DEATH"),
        station: "dereth-testkit::cpu::chat::scenario_a_third_partys_death_is_announced",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.death.every-recorded-kill-notification-is-drawn-word-for-word-on-the-log",
        says: "Every notification a shard really sent about something the player killed reaches the \
               chat log word for word, with the newline the shard put on the end taken off, drawn as \
               a plain line and in the colour a plain line is drawn in -- and it travels the same \
               road every other line in the client travels.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-128"),
        station: "dereth-testkit::dat::chat::scenario_every_recorded_kill_notification_is_drawn_verbatim",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.death.no-death-line-can-be-silenced-where-a-combat-line-can",
        says: "None of the three death lines can be silenced: the client never asks whether they \
               should be, whatever the player has silenced. An ordinary combat line beside them is \
               asked about and is silenced, which is what makes that a measurement and not a \
               silence that never took.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-128-SQUELCH"),
        station: "dereth-testkit::dat::chat::scenario_no_death_line_is_silenced_where_a_combat_line_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.death.your-own-death-goes-through-the-same-hand-and-an-empty-one-says-nothing",
        says: "Being told that you died is drawn by the same hand that draws being told you killed \
               something, as a plain line in the same colour. A notification with nothing in it is \
               read all the same and then deliberately says nothing -- which is a different thing \
               from a message the client could not read.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-128-VICTIM"),
        station: "dereth-testkit::dat::chat::scenario_your_own_death_goes_through_the_same_hand",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.emote.a-typed-one-reaches-the-link-on-the-next-frame-and-only-once",
        says: "An emote typed into the chat entry reaches the connection the client is on -- not on \
               the frame it was typed in, because by then that frame has already handed the link \
               what it had, but on the next one, and exactly once. It takes one place in the order \
               of things the player has sent and no more.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-EMOTE-LINK"),
        station: "dereth-testkit::dat::chat::scenario_a_typed_emote_reaches_the_link_on_the_next_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.emote.an-empty-one-is-silent-and-costs-no-place-in-the-order",
        says: "An emote with nothing after it is accepted and does nothing: no message goes out, \
               nothing is written in the player's own window, and he is not told he did anything \
               wrong. It costs him nothing either -- the next line he sends takes the place in the \
               order the empty one would have had, and so does the next after a line the host could \
               not spell at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-EMOTE-EMPTY"),
        station: "dereth-testkit::cpu::chat::scenario_an_empty_emote_is_silent_and_costs_no_place_in_the_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.emote.every-spelling-of-the-command-sends-the-same-thing",
        says: "There are nine ways to write the emote command -- the long word, its abbreviations, \
               the two punctuation marks, either kind of slash, in capitals, and with spaces around \
               it -- and every one of them sends the same thing about the same words. None of them \
               writes anything in the player's own window first: he reads his own emote when the \
               shard sends it back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-EMOTE-ALIASES"),
        station: "dereth-testkit::cpu::chat::scenario_every_spelling_of_the_emote_command_sends_the_same_thing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.emote.is-drawn-as-name-then-text",
        says: "An emote another player performs appears in the chat window as their name followed \
               by the emote text, on the emote channel. The space between the two is omitted when \
               the text begins with an apostrophe. An emote is drawn even by a client that has no \
               body in the world yet, where a spoken line from the same speaker is discarded.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O494"),
        station: "dereth-testkit::cpu::chat::scenario_emote_is_drawn_as_name_then_text",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.emote.typing-one-into-the-shipped-entry-sends-it-and-leaves-the-next-line-free",
        says: "An emote typed into the shipped chat entry and sent with the return key produces one \
               message and no refusal; an empty one produces none and is not counted as a line the \
               client could not deliver; and neither is sent a second time on the following frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-EMOTE-TYPED"),
        station: "dereth-testkit::dat::chat::scenario_typing_an_emote_sends_it_and_leaves_the_next_line_free",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.entry-adapters",
        says: "Entry actions and widget edits reach the shared draft before the following input, and each submitted line enters its source window history once.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-CHAT-SHARED-ADAPTERS"),
        station: "dereth-testkit::dat::chat::modern_entry_batches_keep_shared_replies_history_aliases_and_widget_edits_current",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.entry-aliases",
        says: "Reply aliases use each interface's matching and missing-target rules; Classic expands the whole entry before the translated space, while modern preserves a trailing message and its insertion point.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHAT-SHARED-ALIASES"),
        station: "dereth-client-model::lib::chat_entry::tests::classic_aliases_match_the_whole_entry_before_space_and_clear_missing_targets",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.entry-history",
        says: "Nonempty submissions enter history once; backward and forward navigation stop at the ends and forward navigation clears an unsubmitted draft.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHAT-SHARED-HISTORY"),
        station: "dereth-client-model::lib::chat_entry::tests::history_has_dead_ends_clears_forward_drafts_and_ignores_empty_submissions",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.entry-interface-history",
        says: "Changing interface retains window drafts and history; submission applies the active interface's ten or one hundred entry limit.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-CHAT-SHARED-SWITCH"),
        station: "dereth-client-model::lib::chat_entry::tests::each_interface_bounds_history_on_submission_and_switching_keeps_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.entry-replies",
        says: "The monarch, patron and ordinary reply actions address their distinct remembered senders; Classic uses its short tell prefix and missing-target warning, while modern uses its assisted-tell prefix and silently preserves an unavailable draft.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHAT-SHARED-REPLIES"),
        station: "dereth-client-model::lib::chat_entry::tests::canonical_reply_actions_address_distinct_sender_slots_and_keep_interface_prefixes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.entry-start-tell",
        says: "Starting a tell fills and focuses the entry without submitting it or appending to history.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHAT-SHARED-START"),
        station: "dereth-client-model::lib::chat_entry::tests::start_tell_replaces_the_draft_without_submitting_or_recording_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.entry.a-run-of-characters-all-arrives-and-none-overwrites-the-last",
        says: "A whole run typed into the chat entry arrives in it, in order, with every character \
               added to what is already there rather than replacing it -- whether the box was opened \
               with the key or pressed with the pointer. The one character the key that opened it \
               also produces is eaten, once and only once.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O275"),
        station: "dereth-testkit::dat::chat::scenario_a_run_of_characters_all_arrives_in_the_entry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.entry.the-line-the-return-key-sends-is-remembered-and-raised-for-its-own-window",
        says: "The return key takes the line out of the box, remembers it where the player can walk \
               back to it with the up key, puts the browsing back to the start, and raises it for \
               the window it was typed in. The key that opens the entry gives it the caret again, \
               which is the other way in besides pressing it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O109"),
        station: "dereth-testkit::dat::chat::scenario_the_line_the_return_key_sends_is_remembered",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.entry.the-return-key-sends-the-line-and-gives-the-keyboard-back",
        says: "The return key opens the chat box, and pressing it again sends what was typed, empties \
               the box and gives the keyboard back to the world -- and nothing puts the caret back \
               in the box a moment later. With the box closed a movement key walks the character \
               again and a letter reaches nothing; with it open the same letter lands in the box.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O276-ENTER"),
        station: "dereth-testkit::dat::chat::scenario_the_return_key_sends_the_line_and_gives_the_keyboard_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.entry.the-send-button-keeps-the-caret-for-itself",
        says: "Pressing the send button sends the line and empties the box, but the button keeps the \
               caret rather than handing it back: nothing can be typed until the player presses the \
               box again. It is the button that has it and not nobody, which is a different thing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O276-SEND"),
        station: "dereth-testkit::dat::chat::scenario_the_send_button_keeps_the_caret_for_itself",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.entry.typing-past-the-edge-slides-the-line-so-the-caret-stays-in-view",
        says: "Typing more into the chat entry than fits across it slides the line along underneath \
               so that the caret stays where the player can see it, hard against the right-hand edge \
               of the box, with the start of what he typed off the left. The sliding is remembered \
               rather than worked out afresh, so redrawing does not throw it away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-46"),
        station: "dereth-testkit::dat::chat::scenario_typing_past_the_entrys_edge_keeps_the_caret_in_view",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.filter.a-censored-word-keeps-the-quotes-around-it",
        says: "With Filter Language on, a censored word is replaced with four asterisks and the \
               punctuation at either end of it stays: the first or last word of a message keeps \
               the quote the chat line puts beside it.",
        since: THIS_CLIENT,
        divergence: "CD-024",
        evidence: Evidence::Private("AC-EVID-CHAT-FILTER-QUOTES"),
        station: "dereth-rules::lib::taboo::tests::a_censored_first_or_last_word_keeps_the_quote_beside_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.filters.every-window-offers-the-same-rows-and-each-row-lies-inside-its-control",
        says: "Each of the five chat windows is given the same list of kinds of line it may show, \
               each row under the caption the shipped text gives it, and the main window is given \
               one row fewer because the everything-else row is not offered for it. Every row is \
               drawn inside the control that holds it, so none of them hangs out beyond it where \
               the next section would be drawn over the top.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-63"),
        station: "dereth-testkit::dat::chat::scenario_every_window_offers_the_same_filter_rows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.filters.the-global-channels-are-rows-of-that-list-and-are-drawn",
        says: "The channels everybody can talk on -- general, trade, looking for a group, \
               roleplaying and the society one -- are rows of that same list rather than a separate \
               page or a set the shard sends, and they are drawn on the screen: the first of them \
               as soon as the tab comes up and the others by scrolling to them, along with the \
               rest of the list below.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-63-CHANNELS"),
        station: "dereth-testkit::dat::chat::scenario_the_global_channels_are_rows_of_that_list_and_are_drawn",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.filters.ticking-a-row-writes-that-windows-own-filter-and-sends-nothing",
        says: "Pressing one of those boxes writes what that chat window will show, and pressing it \
               again puts it back. It is the player's own setting written locally: nothing goes to \
               the shard when he presses it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-63-TICK"),
        station: "dereth-testkit::dat::chat::scenario_ticking_a_filter_row_writes_that_windows_own_filter",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.focus-state",
        says: "Allegiance listening desire survives changing speaking focus; enabled rows and fallback follow ordered availability notices, including the restricted character menu.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHAT-SHARED-FOCUS"),
        station: "dereth-client-model::lib::chat::tests::allegiance_desire_survives_another_focus_and_olthoi_rows_keep_notice_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.garble.a-speaker-you-cannot-understand-is-drawn-as-a-name-and-a-noise",
        says: "A player whose tongue this character does not share is still shown speaking, but what \
               he said is replaced by one of ten noises drawn at random, written as his name and then \
               the noise with no quotes and no \"says\" -- and the mark that says which tongue he used \
               is taken off the name before it is drawn. The mark is on the speaker and not on the \
               listener, so an insect and a human each hear the other as noise and their own kind \
               plainly, and all ten noises really do come up.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O541"),
        station: "dereth-testkit::cpu::chat::scenario_a_speaker_you_cannot_understand_is_a_name_and_a_noise",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.garble.a-tell-you-cannot-understand-is-drawn-even-when-it-was-not-for-you",
        says: "A private message this character cannot understand is drawn as a noise even when it was \
               addressed to somebody else, where one he could have understood and that was not for him \
               is drawn nowhere. It never arms the reply command either, so a speaker whose words were \
               never shown cannot be answered; and a message a character sends to himself is his own \
               thought and is never turned into a noise.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O541-TELL"),
        station: "dereth-testkit::cpu::chat::scenario_a_garbled_tell_is_drawn_even_when_it_was_not_for_you",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.garble.an-acted-emote-is-garbled-where-a-pose-and-a-shout-are-not",
        says: "An emote from somebody whose tongue this character does not share is replaced by a \
               noise like his speech is, and the line is counted as one the player could not read. A \
               pose is never turned into a noise, whoever strikes it, and neither is a line shouted \
               with a range on it -- that one keeps its ordinary wording and is not even stripped of \
               the tongue mark.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O541-EMOTE"),
        station: "dereth-testkit::cpu::chat::scenario_an_acted_emote_garbles_where_a_pose_and_a_shout_do_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.input.a-press-nobody-claims-is-handed-round-once-and-then-dropped",
        says: "A key press is offered to each part of the client in turn, and one that nobody claims \
               is dropped rather than put back: it is not offered again on the next frame, or on \
               every frame after that for ever. An idle client hands nothing round at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O275-EXPIRY"),
        station: "dereth-testkit::dat::chat::scenario_an_action_nobody_claims_is_dispatched_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.input.the-client-is-set-to-eat-a-character-on-one-edge-and-never-otherwise",
        says: "The client is set to swallow one character only on the edge where a key press turns \
               typing on -- the key that opens a box also produces a character, and that is the one \
               it eats. A box given the caret with the pointer swallows nothing, no ordinary \
               keystroke re-arms it, and losing the caret does not arm it either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O275-LATCH"),
        station: "dereth-testkit::dat::chat::scenario_the_eat_the_next_character_latch_is_armed_on_one_edge",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.interface-colors",
        says: "Each interface keeps its chat palette, including the Classic overrides and green fallback outside the table.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHAT-SHARED-COLORS"),
        station: "dereth-client-contract::lib::chat::colors::tests::classic_overrides_are_narrow_and_modern_keeps_the_full_table",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.loc.prints-the-bodys-location-line",
        says: "Typing /loc and Enter prints Your location is: followed by the body's cell, its \
               position to six decimals and its facing on the main chat log, and counts as neither \
               a refusal nor an unknown command.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-LOC"),
        station: "dereth-client::dat::chat::loc_command::typing_slash_loc_and_enter_prints_your_location_is_with_the_bodys_position",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.log.a-drag-selects-the-swept-text",
        says: "Pressing on a line of text, dragging and releasing selects exactly the characters \
               the pointer swept over; the press alone selects nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F28-LOG"),
        station: "dereth-ui::cpu::text::drag_selection::a_press_drag_and_release_select_the_glyphs_the_pointer_swept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.log.a-logged-in-client-draws-lines-on-the-main-window-and-follows",
        says: "Once the player is in the world, a line the client writes to chat is drawn on the \
               main chat window's log where he reads it, and is counted as taken rather than \
               dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P155-LOG"),
        station: "dereth-client::dat::chat::chat_entry_after_login::a_line_pushed_in_world_is_drawn_on_the_main_chat_windows_log",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.log.a-shard-line-still-carries-its-newline-at-the-model-boundary",
        says: "A line the shard broadcasts -- the welcome on entering the world, the answer to a \
               request for help -- ends in a newline on the wire, and it still does when the client \
               has finished reading it: the newline is taken off later, between reading the line and \
               putting it in the window, and not before.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-104-WIRE"),
        station: "dereth-testkit::cpu::chat::scenario_a_shard_line_still_carries_its_newline_at_the_model",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.log.changing-the-font-size-remeasures-the-backlog-and-the-next-line",
        says: "Picking a different size for the chat text changes the size of the lines already in \
               the window as well as the next one to arrive: the whole backlog is measured again, \
               not only what comes afterwards. The list starts on the size the window was drawn \
               with, which is why only a change to it can be seen at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-47"),
        station: "dereth-testkit::dat::chat::scenario_changing_the_chat_font_size_remeasures_the_backlog",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.log.every-family-is-stamped-filtered-and-logged-per-the-options",
        says: "Every kind of chat line -- combat, speech, tells, channels, death notices and the \
               rest -- carries a timestamp in front of it when the timestamp option is on and none \
               when it is off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1185-LOG"),
        station: "dereth-client::dat::chat::chat_family_post_processing::every_family_carries_the_timestamp_prefix_and_only_when_the_option_is_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.log.the-bottom-row-is-the-newest-line-and-never-an-empty-one",
        says: "The bottom row of the chat log carries the newest message's own letters and sits \
               flush on the bottom edge of the pane. It is never a blank row underneath the last \
               message, however many newlines the shard put on the end of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-104"),
        station: "dereth-testkit::dat::chat::scenario_the_bottom_row_of_the_log_is_the_newest_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.log.the-log-follows-new-lines-while-at-its-end",
        says: "While the chat log is at its end it follows what arrives: through the shard's whole \
               welcome burst it stays on its last line, with the newest words in the pane and the \
               oldest scrolled off the top, and the player makes no gesture.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O148-LOG"),
        station: "dereth-client::gpu::chat::chat_log_scrolling::the_log_follows_the_shards_own_welcome_burst",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "chat.log.the-newline-between-lines-is-a-separator-and-not-a-terminator",
        says: "The newline the window puts between one line and the next is a separator: three \
               messages leave two of them in the log, and nothing at all after the third.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-104-SEPARATOR"),
        station: "dereth-testkit::dat::chat::scenario_the_newline_between_lines_is_a_separator",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.log.the-newlines-are-trimmed-from-both-ends-and-nothing-else-is",
        says: "The newline the client's own wording leaves on a line is taken off both ends, however \
               many there are, and only the newlines are: the spaces a timestamp or a shard put \
               there survive it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O277-TRIM"),
        station: "dereth-testkit::cpu::chat::scenario_the_newlines_are_trimmed_from_both_ends",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.log.two-recorded-kinds-of-line-draw-in-two-different-colours",
        says: "Two recorded lines of different kinds reach the chat log in two different colours, so \
               which colour a line is drawn in is chosen by what kind of line it is rather than \
               being one colour for everything. The speaker's name is part of the coloured line and \
               not a grey run of its own -- the grey belongs to the timestamp -- unless his name is \
               clickable, in which case it is drawn in the colour a clickable name is drawn in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O278"),
        station: "dereth-testkit::dat::chat::scenario_two_recorded_channels_draw_in_two_different_colours",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.pose.a-key-bound-to-a-pose-moves-the-body-and-one-that-is-not-is-bound-to-nothing",
        says: "A key the shipped keymap binds to lying down really lies the body down when it is \
               pressed, issuing the motion for it. A key the shipped keymap binds to no pose at all \
               is bound to none, in either of the two places a pose can be bound.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-6-KEYS"),
        station: "dereth-testkit::dat::chat::scenario_a_key_bound_to_a_pose_moves_the_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.pose.a-run-alone-says-nothing-and-one-the-client-cannot-place-is-spoken-whole",
        says: "A pose typed on its own is performed and nothing is said out loud, because there is \
               nothing left of the line. A run between stars that the client cannot place is not a \
               pose at all: the whole line is spoken as it was typed, stars and spelling and all, \
               rather than the run being eaten.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-6-EDGES"),
        station: "dereth-testkit::dat::chat::scenario_a_run_alone_says_nothing_and_an_unknown_one_is_spoken",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.pose.a-run-between-stars-is-performed-and-the-rest-of-the-line-is-spoken",
        says: "Typing a pose between stars in the middle of a line does three things: the body \
               performs it with the same motion the key bound to it would, the people around are \
               told one form of the words, and what the player himself reads is the other form. The \
               rest of the line is spoken out loud, with the run taken out of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-6"),
        station: "dereth-testkit::dat::chat::scenario_a_run_between_stars_is_performed_and_the_rest_spoken",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.pose.every-pose-the-list-advertises-can-really-be-performed",
        says: "Every pose the client's own list advertises is one the shipped table can place and \
               one the body has a motion for. The list and the table are two separate things and can \
               disagree, and a name advertised that cannot be placed would be a command that \
               silently does nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O540-LIST"),
        station: "dereth-testkit::dat::chat::scenario_every_pose_the_list_advertises_can_be_performed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.pose.the-list-command-prints-the-shipped-list-and-sends-nothing",
        says: "Asking for the list of poses prints the client's own list, whole and in one line, in \
               the chat window -- and sends nothing to the shard, because it is the client's list \
               and not the shard's.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-6-LIST"),
        station: "dereth-testkit::dat::chat::scenario_the_emotes_command_prints_the_shipped_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.pose.the-players-own-echo-is-never-turned-into-a-noise",
        says: "What the player reads about his own pose is written for him rather than repeated \
               back from the shard, so it is never turned into a noise however alien the people he \
               belongs to -- and it is not swallowed as his own words coming back to him either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O540-ECHO"),
        station: "dereth-testkit::dat::chat::scenario_the_players_own_echo_is_never_turned_into_a_noise",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.pose.the-possessive-word-is-chosen-by-sex-before-the-message-leaves",
        says: "A pose whose words mention something of the player's own picks the right possessive \
               word from his sex before the message leaves him, so no placeholder ever reaches \
               anybody's window; the form he reads about himself has no possessive in it at all. \
               The name of a pose is found without minding how it was capitalised.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O540-PRONOUN"),
        station: "dereth-testkit::dat::chat::scenario_the_possessive_word_is_chosen_by_sex",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.pose.the-say-command-goes-through-the-same-extraction",
        says: "Saying something with the command that says it goes through exactly the same \
               extraction as saying it with no command in front: a pose in the line is performed and \
               the rest is spoken, the same way round.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-6-SAY"),
        station: "dereth-testkit::dat::chat::scenario_the_say_command_goes_through_the_same_extraction",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.pose.what-a-pose-sends-is-a-different-message-from-what-the-command-sends",
        says: "Performing a pose and using the emote command send two different messages: the same \
               framing and the same place in the order, but a different message and different \
               words -- the pose sends the form the people around read, and the command sends what \
               the player typed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O540"),
        station: "dereth-testkit::dat::chat::scenario_a_pose_sends_a_different_message_from_the_emote_command",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.ranged-speech.is-gated-by-the-range-the-message-carries",
        says: "A line spoken with a range on it is heard inside exactly that range and not outside it, \
               whether the player is indoors or outdoors, so it is not the hearing radius that decides. \
               A speaker the client holds no body for is refused on this path, which is the opposite of \
               the ordinary speech path, and a squelched speaker one step away is refused too.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O496-RANGED"),
        station: "dereth-testkit::cpu::chat::scenario_a_ranged_line_is_gated_by_its_own_range",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.routing",
        says: "Addressed chat reaches only its destination regardless of filters; broadcast chat requires a valid enabled type and trims edge newlines.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHAT-SHARED-ROUTE"),
        station: "dereth-client-contract::lib::chat::interface::shared_tests::addressed_messages_bypass_filters_and_large_broadcast_types_never_shift_the_mask",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.selection.the-field-beside-the-window-follows-what-is-picked-and-clears",
        says: "The field beside the chat window shows what the player has picked: its background \
               changes when he picks something, changes again for a stack of more than one, goes \
               back for a single one, and returns to the background it was drawn with when he picks \
               nothing or when the thing itself has gone -- with the name beside it emptied. It is \
               always there; what changes is what it shows, and an empty one blocks nothing else.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-EMPTY-SELECTION"),
        station: "dereth-testkit::dat::chat::scenario_the_selection_field_follows_what_is_picked_and_clears",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.soul-emote.your-own-is-not-echoed-back",
        says: "A pose you perform yourself is not drawn a second time when the shard echoes it, while \
               an acted emote from you is drawn; and a sender name the shard has marked keeps that mark \
               on the pose path, where the acted path strips it and garbles the line.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O494-SOUL"),
        station: "dereth-testkit::cpu::chat::scenario_your_own_soul_emote_is_not_echoed_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.speech.a-line-with-a-range-on-it-has-no-echo-of-your-own",
        says: "A line spoken with a range on it has no special form for the player's own words: his \
               own line comes back to him the way anybody else's would, with his name clickable, \
               where an ordinary spoken line of his own comes back as his own.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O277-RANGED"),
        station: "dereth-testkit::cpu::chat::scenario_a_line_with_a_range_on_it_has_no_echo_of_your_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.speech.a-speaker-the-client-cannot-place-is-always-heard",
        says: "Hearing is refused only when the client can actually measure the distance: a speaker \
               with no body in the world, and a line with no speaker at all, are heard at any range \
               whatever, and that escape is taken ahead of the squelch as well as ahead of the \
               distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O464-ESCAPE"),
        station: "dereth-testkit::cpu::chat::scenario_an_unplaceable_speaker_is_always_heard",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.speech.a-spoken-line-is-the-name-the-verb-and-the-words-in-quotes",
        says: "A spoken line is the speaker's name, then the client's own verb and comma, then what \
               he said in quotes -- never the name and the words run together. The player's own \
               words come back to him in a form of their own, another player's name is drawn as \
               something he can click, and before the shard has said who the player is nothing can \
               be his own line.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O277"),
        station: "dereth-testkit::cpu::chat::scenario_a_spoken_line_is_the_name_the_verb_and_the_words",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.speech.earshot-and-squelch-both-gate",
        says: "A spoken line is shown only when the speaker is close enough to be heard -- 75 \
               metres outdoors, 25 indoors, measured horizontally and exclusive at the edge -- \
               and only when that speaker is not squelched for that kind of text. Either \
               condition alone silences the line.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O464"),
        station: "dereth-testkit::cpu::chat::scenario_speech_earshot_and_squelch_both_gate",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.speech.every-recorded-spoken-line-is-drawn-with-the-clients-own-verb-and-quotes",
        says: "Every line the recordings carry of somebody speaking is drawn with the client's own \
               verb and quotes around it, with the speaker's name clickable when he is a player and \
               plain when he is not -- never the name and the words run together. The recordings \
               carry no private messages at all, because what one player says to another is private \
               and the public recordings are scrubbed of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O277-CORPUS"),
        station: "dereth-testkit::cpu::chat::scenario_every_recorded_spoken_line_is_drawn_with_the_verb",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.speech.every-spoken-line-the-recorded-client-sent-is-rebuilt-byte-for-byte",
        says: "Every spoken line the recorded client sent is rebuilt by this client's own writer, \
               byte for byte, with nothing but the counter that numbers the actions of a connection \
               put back. The recordings carry lines of several different lengths, and the spread is \
               asserted rather than described, because a writer that measured a string wrongly \
               would agree on one length and disagree on another.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O167-TALK"),
        station: "dereth-testkit::cpu::chat::scenario_every_recorded_spoken_line_is_rebuilt_byte_for_byte",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.speech.neither-safety-gate-would-have-stopped-a-line-the-recordings-carry",
        says: "Neither of the two gates a line passes before it is sent -- the one that reads the \
               text for what it must not contain, and the one that asks how fast the player is \
               talking -- would have stopped any line a player sent in the recordings. The only \
               recorded lines of 256 characters or more, which the first refuses outright, are \
               long tells from non-player characters, and those never pass through either gate. \
               Both are calibrated in \
               the same run on lines that are known bad, so the zero is a reading and not an \
               instrument that has never answered anything else.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O167-GATE"),
        station: "dereth-testkit::cpu::chat::scenario_neither_safety_gate_would_have_stopped_a_recorded_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.speech.only-a-player-has-a-clickable-name-and-the-range-is-exclusive-at-both-ends",
        says: "Only somebody who is a player has a name the player can click, and which ids count \
               as a player is exclusive at both ends -- one either way is a name that silently \
               cannot be clicked, or a creature whose name can be.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O277-RANGE"),
        station: "dereth-testkit::cpu::chat::scenario_only_a_player_has_a_clickable_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.speech.your-own-echo-and-a-remote-speaker-are-drawn-from-two-different-forms",
        says: "The player's own spoken line comes back to him in one form and somebody else's in \
               another, and both reach the log in the colour a spoken line is drawn in -- never the \
               name and the words run together with no verb between them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O277-LOG"),
        station: "dereth-testkit::dat::chat::scenario_your_own_echo_and_a_remote_speaker_are_drawn_apart",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch-panel.emptying-the-name-box-darkens-both-buttons-and-typing-arms-them",
        says: "Typing a name into the squelch box arms both squelch buttons and emptying it again \
               darkens both. It is the only rule in the tab that reads the box at all, and it is run \
               by the keystroke rather than by the box being empty -- so a box emptied with the \
               backspace key darkens them and a box emptied any other way does not.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-51-KEYSTROKE"),
        station: "dereth-testkit::dat::chat::scenario_emptying_the_squelch_name_box_dims_both_buttons",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch-panel.picking-a-row-lights-both-buttons-again-over-an-empty-box",
        says: "Picking a row of the squelch list lights both squelch buttons again even with the \
               name box empty, because picking a row does not look at the box. A darkness the box's \
               own rule caused is undone by it, which is the client's own behaviour and what a \
               player sees.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-62-SELECTION"),
        station: "dereth-testkit::dat::chat::scenario_picking_a_squelch_row_lights_both_buttons_again",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch-panel.remove-follows-what-is-picked",
        says: "The button that takes somebody off the squelch list is dark until a row is picked and \
               lit once one is, and redrawing the list picks nothing by itself. The two squelch \
               buttons are on another rule entirely and do not follow the selection.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-62-REMOVE"),
        station: "dereth-testkit::dat::chat::scenario_the_remove_button_follows_the_selection",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch-panel.removing-sends-the-kind-the-row-itself-names",
        says: "Taking somebody off the squelch list sends the undoing of the kind of silence that \
               row names -- a whole account is undone as an account and one character as a \
               character. The picked row is drawn as picked so the player can see which it is, and \
               the one he did not pick is not.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-8-REMOVE"),
        station: "dereth-testkit::dat::chat::scenario_removing_sends_the_kind_the_row_itself_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch-panel.squelching-clears-the-box-and-darkens-that-button-alone",
        says: "Pressing one of the two squelch buttons empties the name box, sends that button's own \
               message about the name that was in it, and darkens that button and no other -- so the \
               one that was not pressed is still lit over an empty box.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-51-CLICK"),
        station: "dereth-testkit::dat::chat::scenario_squelching_clears_the_box_and_dims_that_button_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch-panel.the-name-box-takes-the-caret-from-a-press",
        says: "The name box on the squelch tab takes the caret when the player presses it, and once \
               it has the caret there is a caret to draw inside the box.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-8-CARET"),
        station: "dereth-testkit::dat::chat::scenario_the_squelch_name_box_takes_the_caret_from_a_press",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch-panel.the-name-label-is-wider-than-its-box-and-wraps-in-retail-too",
        says: "The label beside the squelch name box is wider than the box it was drawn in, so it \
               breaks at the space and takes two lines. That is what the shipped layout does -- \
               nothing in it says to keep the label on one line, and it does not fit on one however \
               the margins are read -- so the wrapping is the client's own and not something the \
               drawing got wrong.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-20"),
        station: "dereth-testkit::dat::chat::scenario_the_name_label_is_wider_than_its_box_and_wraps",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch-panel.the-shards-answer-lights-both-buttons-again-and-adds-the-row",
        says: "The shard answers a squelch it accepted with the whole list, and that draws the new \
               row and lights both squelch buttons again -- so the darkness a player sees after \
               pressing one lasts exactly as long as the round trip, and a squelch the shard refuses \
               leaves that button dark until he types again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-51-REPLY"),
        station: "dereth-testkit::dat::chat::scenario_the_shards_answer_lights_both_buttons_and_adds_the_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch-panel.the-tab-lists-who-the-shard-says-is-squelched-in-one-sorted-block",
        says: "The squelch tab is built and driven on an ordinary frame, and the list the shard \
               sends fills it: one block sorted by name with whole accounts mixed in among the \
               characters rather than kept apart, each row saying which kind it is. An empty list \
               really arrives and correctly draws nothing, and a second copy of the same list \
               redraws it rather than adding to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-8"),
        station: "dereth-testkit::dat::chat::scenario_the_squelch_tab_lists_who_the_shard_says_is_squelched",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch-panel.the-tab-opens-with-both-buttons-lit-over-an-empty-box",
        says: "The squelch tab opens with both squelch buttons lit although the name box is empty, \
               and with the remove button dark because nothing is picked yet. Being empty is not by \
               itself what darkens them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-51"),
        station: "dereth-testkit::dat::chat::scenario_the_squelch_tab_opens_with_both_buttons_lit",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch-panel.the-two-buttons-send-different-messages-about-the-same-name",
        says: "The two squelch buttons send two different things about the same typed name: \
               silencing one character carries a place for who he is and the whole set of kinds of \
               line to silence, while silencing a whole account carries only the name and is shorter \
               by exactly those two.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-8-SENDS"),
        station: "dereth-testkit::dat::chat::scenario_the_two_squelch_buttons_send_different_messages",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.squelch.a-populated-list-is-read-whole-and-keeps-the-kind-of-each-entry",
        says: "A squelch list with names in it is read to its last byte and each name keeps the kind \
               of silence it was given -- one character, or a whole account -- and written back out \
               it is the same bytes again. Every list the recordings carry is empty, so a reader \
               that had the fields in the wrong order would read all of them and leave the tab blank \
               for ever with nothing to say so.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-8-BYTES"),
        station: "dereth-testkit::cpu::chat::scenario_a_populated_squelch_list_is_read_whole",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.squelch.every-incoming-path-asks-on-the-type-it-carries",
        says: "Every kind of incoming text asks the squelch tables about the type that line actually \
               carries: an ordinary say, a ranged say, an emote, a channel line, combat text, a system \
               line and a chat-room line each have their own answer, your own say is composed above the \
               gate and cannot be hidden, a tell is never refused by the client, and a type that is not \
               a squelchable channel is never refused however the tables are set.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-4B"),
        station: "dereth-testkit::cpu::chat::scenario_squelch_is_asked_on_every_incoming_path",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.squelch.the-shards-table-replaces-the-clients-own",
        says: "The squelch list the shard sends is the whole list: a speaker named in it stops being \
               heard, the list-wide entry silences a whole kind of text on its own, and a later, \
               smaller list makes a speaker audible again rather than being merged into the old one. A \
               repeated name keeps the first entry, and a truncated list leaves the table exactly as it \
               was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O934"),
        station: "dereth-testkit::cpu::chat::scenario_squelch_table_off_the_wire_replaces_the_clients_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.taboo.the-shipped-table-censors-a-taboo-word-and-passes-an-ordinary-one",
        says: "Chat is filtered with the word list the game ships: a listed word in a line is \
               replaced by asterisks of the same length while the ordinary words around it pass \
               unchanged.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P165-TABOO"),
        station: "dereth-client-model::dat::chat::taboo_filter::shipped_audience_one_drives_the_chat_replacement",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.talk-focus.a-channel-switched-off-and-on-again-drops-the-player-back-to-saying-it",
        says: "A channel the player was talking on that is switched off and straight back on again \
               leaves him saying things aloud rather than on the channel: it is the switching off \
               that moves him, and switching it back on offers the channel again rather than putting \
               him back on it. The line he was part-way through typing goes wherever the window says \
               he is talking when he presses return -- whether it was the shard's description of \
               him, his own option, or the service itself going away that moved him.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-EMPTY-SELECTION-FALLBACK"),
        station: "dereth-testkit::dat::chat::scenario_a_channel_switched_off_and_on_drops_the_player_back_to_saying_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.talk-focus.a-new-window-reads-the-settings-now-rather-than-replaying-what-it-missed",
        says: "A new chat window projects current shared channel state without replaying old notices; a previously queued line keeps its intended destination.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-EMPTY-SELECTION-GENERATION"),
        station: "dereth-testkit::dat::chat::scenario_a_new_chat_window_reads_the_settings_now",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.talk-focus.channel-fallback-does-not-require-a-window",
        says: "Channel fallback uses shared state even with no interface or a missing projected row; switching a disabled channel back on does not undo the fallback or replay an old notice.",
        since: THIS_CLIENT,
        divergence: "CD-027",
        evidence: Evidence::Private("AC-EVID-CHAT-SHARED-FALLBACK"),
        station: "dereth-testkit::dat::chat::scenario_channel_fallback_does_not_require_a_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.talk-focus.every-row-is-told-every-time-and-the-menu-guard-does-not-unset-it",
        says: "Every row of the talk-to menu is told whenever the player's channels are set, in the \
               menu's own order and even when the answer has not changed, so a menu that missed one \
               cannot stay stale. The menu greys a row the character may not use -- an Olthoi \
               cannot talk to a fellowship -- and greying it leaves the setting behind it switched \
               on, so the row comes back the moment the character may use it again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CHAT-FOCUS-MENU"),
        station: "dereth-testkit::cpu::chat::scenario_every_talk_focus_row_is_told_every_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.talk-focus.one-authoritative-value",
        says: "Who the player is talking to is one piece of client state: choosing a target from \
               the chat menu and the game changing it both write the same value, and the next \
               line typed goes to whatever that value currently is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CHAT-FOCUS"),
        station: "dereth-testkit::cpu::chat::scenario_talk_focus_has_one_authoritative_value",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.talk-focus.picking-a-target-redraws-every-row-from-the-settings",
        says: "The rows of the shipped talk-to menu carry what the chat system last said about each \
               one. Picking a target from that menu redraws every row from the player's stored \
               settings rather than from the last batch of answers, so a row those answers had \
               opened closes again while the setting behind it is left exactly as it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CHAT-FOCUS-SHIPPED"),
        station: "dereth-testkit::dat::chat::scenario_the_talk_to_menu_is_redrawn_from_the_settings",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.talk-focus.the-channel-rows-follow-the-service-and-the-players-own-options",
        says: "The channel rows of the talk-to menu are shut altogether while the chat-room service \
               is not running, whatever the player's own options say. With it running each row \
               follows its own option, and the row for the alien people follows who the player is \
               rather than an option at all. The two rows that are not channels are never touched by \
               either, which is why saying it aloud is always offered.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O235-ENABLES"),
        station: "dereth-testkit::cpu::chat::scenario_the_channel_rows_follow_the_service_and_the_options",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.talk-focus.what-a-typed-line-becomes-follows-the-menu",
        says: "What a typed line becomes follows the talk-to menu rather than only the label on it: \
               an ordinary spoken line, or a line on the fellowship's, the patron's, the monarch's \
               or the vassals' channel, each with its own number. The row that talks to whoever is \
               selected sends nothing at all when nobody is, rather than saying a private line out \
               loud.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O109-FOCUS"),
        station: "dereth-testkit::dat::chat::scenario_what_a_typed_line_becomes_follows_the_menu",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.talk-to-menu.a-tell-goes-to-the-chat-target-and-not-to-the-selection",
        says: "A line typed with the talk-to menu on Tell to <name> goes to the one the menu names \
               -- the chat target, which is taken up from what the player picked out and then kept \
               while it stays near -- and not to whatever the player has picked out since. With no \
               chat target the line goes nowhere. The menu's squelch row asks about the same one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHAT-TARGET-TELL"),
        station: "dereth-testkit::cpu::chat::scenario_a_tell_to_the_chat_target_goes_to_it_and_not_to_the_selection",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.talk-to-menu.the-button-caption-follows-the-menu-and-a-row-that-is-shut-refuses",
        says: "The caption on the talk-to button follows what the player picked from the menu. A row \
               for a channel he is not in refuses to be picked and leaves the caption alone, and a \
               row the menu does not have at all does nothing. One row asks to be put in the \
               allegiance's chat as well as changing where a line goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O109-CAPTION"),
        station: "dereth-testkit::dat::chat::scenario_the_button_caption_follows_the_menu",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.talk-to-menu.the-chat-target-follows-what-is-selected-while-it-is-near",
        says: "Who the player is talking to follows what he has picked out in the world: something \
               he can talk to and that is near him is taken up when he has nobody, kept while it \
               stays near, and let go of when it goes. Something picked out but far away is not \
               taken up at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O235-AUTOTARGET"),
        station: "dereth-testkit::cpu::chat::scenario_the_chat_target_follows_what_is_selected_while_it_is_near",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.talk-to-menu.the-menu-is-real-rows-in-two-columns-that-opens-above-the-button",
        says: "The chat window's talk-to menu opens upwards, directly above its button and lined \
               up with its left edge, as fourteen rows, the squelch toggle and thirteen channels, \
               laid out two to a line in seven lines with none overlapping.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O139-TALK-TO-MENU"),
        station: "dereth-ui-screens::dat::chat::talk_focus_menu::the_popup_is_two_columns_of_seven_and_opens_above_the_button",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.talk-to-menu.the-popup-draws-over-the-window-and-gives-the-keyboard-back",
        says: "The talk-to menu opens as a popup that draws over the whole chat window, not inside \
               it, with every row drawn whole rather than cut off at the window's edge. A row this \
               character may not use is dark and clicking it changes nothing; a row he may use takes \
               the channel and closes the popup, which then draws nothing and takes no input. \
               Clicking away closes it too, and neither closing eats the next thing he types.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-EMPTY-SELECTION-POPUP"),
        station: "dereth-testkit::dat::chat::scenario_the_talk_to_popup_draws_over_the_window_and_gives_the_keyboard_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.talk-to-menu.the-selected-players-name-fills-both-of-the-changing-rows",
        says: "The two rows of the talk-to menu that follow what the player has picked out carry \
               that person's own name drawn into them -- the row that talks to him and the row that \
               silences him -- rather than a placeholder where his name should be.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-101"),
        station: "dereth-testkit::dat::chat::scenario_the_selected_players_name_fills_both_changing_rows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.talk-to-menu.the-squelch-row-is-a-toggle-and-its-message-names-the-speaker",
        says: "The row that silences somebody is a toggle on whoever the player was last talking \
               to: it asks for him to be silenced if he is not and un-silenced if he is, on every \
               kind of line and as one character rather than a whole account. The current server list \
               is checked by character id, with an empty wire name; a namesake or partial squelch does \
               not count as silencing every kind of line. With no existing target it asks nothing, \
               and only the add flag differs between silencing and un-silencing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O235-SQUELCH"),
        station: "dereth-testkit::cpu::chat::scenario_the_squelch_row_is_a_toggle_and_names_the_speaker",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.talk-to-menu.which-channel-a-row-is-comes-from-the-row-and-not-its-place",
        says: "Which channel a row of the talk-to menu is, is carried by the row itself rather than \
               by where it sits in the list: the list is not in the channels' own order and the first \
               row is not a channel at all. Picking one asks for the talk focus to be changed to \
               that channel.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O109-ROWS"),
        station: "dereth-testkit::dat::chat::scenario_a_rows_place_in_the_list_is_not_the_order_of_the_channels",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.target-sweep",
        says: "Chat retains an owned or nearby selected speaker, forgets an unavailable one and adopts a named nearby selection on the throttled sweep without a rendered chat panel.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHAT-SHARED-TARGET"),
        station: "dereth-client-runtime::lib::interaction::tests::chat_target_sweep_runs_without_a_panel_obeys_the_boundary_and_replaces_nameless_targets",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.tell-markup.a-composed-room-line-still-carries-its-markup-at-the-model-boundary",
        says: "The line the client composes around something said in a chat room carries the mark \
               that makes the speaker's name clickable as ordinary characters of the line, and still \
               does when the client has finished composing it. Turning that mark into something the \
               player can click is the window's job, further down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-108-WIRE"),
        station: "dereth-testkit::cpu::chat::scenario_a_composed_room_line_still_carries_its_markup",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.tell-markup.markup-the-client-does-not-know-is-drawn-as-it-stands",
        says: "A mark the client has no reader for is not a mark at all: every character of it is \
               drawn as it stands, and nothing in that line is clickable.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-108-REJECT"),
        station: "dereth-testkit::dat::chat::scenario_markup_the_client_does_not_know_is_drawn_as_it_stands",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.tell-markup.the-clickable-name-is-drawn-in-its-own-colour",
        says: "A name the player can click is drawn in its own colour and the rest of the line in \
               the colour that channel's lines are drawn in, so a clickable name does not look like \
               the words around it. The two colours come from different places in the shipped \
               layout, which is why they can differ at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-108B"),
        station: "dereth-testkit::dat::chat::scenario_the_clickable_name_is_drawn_in_its_own_colour",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.tell-markup.the-log-draws-the-name-and-not-the-markup",
        says: "The chat log draws the speaker's name and not the mark around it: no angle bracket \
               of it survives into the window, there is one letter drawn per letter of the line the \
               player reads, and every letter of the name -- and no letter outside it -- carries \
               what the player needs to click it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-108"),
        station: "dereth-testkit::dat::chat::scenario_the_log_draws_the_name_and_not_the_markup",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.tell.a-private-message-is-drawn-only-when-it-was-addressed-to-you",
        says: "A private message is drawn only when it was addressed to the player: one he sent to \
               himself is drawn as a thought instead, and one the shard copied to him but addressed \
               to somebody else draws nothing at all. Which of the three it is, is decided in that \
               order.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O277-TELL"),
        station: "dereth-testkit::cpu::chat::scenario_a_private_message_is_drawn_only_when_it_was_for_you",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.tell.a-reply-goes-to-whoever-last-wrote-to-you-and-nowhere-when-nobody-has",
        says: "A reply goes to whoever last wrote to the player and to nobody else: every line the \
               recordings carry is driven twice, once as recorded -- where the speaker is not \
               somebody a player may reply to, so nothing is remembered and the reply is refused in \
               the client's own words -- and once re-addressed to the player standing here, where \
               the reply goes back to that one speaker, never to the player himself and never to \
               nobody. A line addressed to somebody else is remembered by nobody.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O167-REPLY"),
        station: "dereth-testkit::cpu::chat::scenario_a_reply_goes_to_whoever_last_wrote_to_you",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.tell.a-tell-addressed-to-someone-else-is-counted-and-not-drawn",
        says: "A tell the shard passes on that was addressed to somebody else draws nothing in the \
               chat window, although it is read and counted; the same tell addressed to the player \
               is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O320-TELL"),
        station: "dereth-client::dat::chat::heard_speech_formatting::a_tell_addressed_to_somebody_else_is_counted_and_not_drawn",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.tell.a-typed-private-message-goes-out-with-the-words-first-and-the-name-after",
        says: "A private message the player types goes out with the words first and the name after, \
               and the words are written in the same bytes the recorded client used for the same \
               text, in the same place in the message. The line the comparison is made with is the \
               longest one the recordings carry, found by walking them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O167-WIRE"),
        station: "dereth-testkit::cpu::chat::scenario_a_typed_tell_goes_out_with_the_words_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.tell.clicking-a-name-in-the-log-starts-a-private-message-to-him",
        says: "A name in the chat log that the player can click starts a private message to him: the \
               main entry is filled with the beginning of one and given the caret, and nothing is \
               sent. It does that only while the entry does not already have the caret, so a line \
               the player was half-way through typing is not thrown away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-5-CLICK"),
        station: "dereth-testkit::dat::chat::scenario_clicking_a_name_in_the_log_starts_a_tell_to_him",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.tell.every-recorded-private-message-is-rebuilt-from-the-same-typed-line",
        says: "Every private message the recorded client sent is rebuilt by typing the same line \
               into this one: the same name, the same words, and the same bytes, with nothing but \
               the counter that numbers the actions of a connection put back. That is the whole \
               path measured at once, from the characters a player presses to the blob that leaves.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O167-BYTES"),
        station: "dereth-testkit::cpu::chat::scenario_every_recorded_tell_is_rebuilt_from_a_typed_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.tell.every-refusal-is-the-clients-own-words",
        says: "A private message the client will not send says so in the client's own words on the \
               channel it answers on, and sends nothing: a name with no comma after it, a comma \
               with no name before it, a reply with no words, a reply with nobody to reply to and a \
               repeat with nobody to repeat to are five different sentences. The one line that \
               really does answer with nothing at all is counted on its own, so that silence and a \
               lost line are two different readings.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O167-REFUSE"),
        station: "dereth-testkit::cpu::chat::scenario_every_refusal_is_the_clients_own_words",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.tell.every-way-of-writing-a-private-message-reaches-the-wire",
        says: "All ten ways of writing a private message reach the wire, and the three things they \
               mean are three different messages: the five spellings of telling somebody and the \
               two of telling him again go out addressed by name, and the three spellings of \
               replying go out addressed by the id the client remembered. Being in the table of \
               verbs and actually sending are asserted apart, because the client once did the first \
               without the second.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O167-ALIAS"),
        station: "dereth-testkit::cpu::chat::scenario_every_way_of_writing_a_private_message_reaches_the_wire",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.tell.only-the-main-window-takes-the-tell",
        says: "Only the main chat window takes a private message that is being started: the smaller \
               windows beside it have entries of their own and are left alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-5-MAIN"),
        station: "dereth-testkit::dat::chat::scenario_only_the_main_window_takes_the_tell",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.tell.picking-a-friend-and-pressing-tell-fills-the-entry-and-replaces-what-was-there",
        says: "Picking a friend and pressing the button that writes to him fills the main chat entry \
               with the beginning of a private message to him, gives it the caret with the caret at \
               the end and nothing picked out, and tells the window that the player is typing. \
               Whatever was half-typed there is replaced rather than added to.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-5"),
        station: "dereth-testkit::dat::chat::scenario_picking_a_friend_and_pressing_tell_fills_the_entry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.tell.the-key-does-nothing-when-nobody-a-player-could-talk-to-is-selected",
        says: "The key that starts a private message to whoever is selected does nothing when \
               nothing is selected, and nothing when what is selected is not somebody a player could \
               talk to. It reads the selection as it is at that moment rather than as the last frame \
               left it, so it cannot address the person who was selected before.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-5-NOBODY"),
        station: "dereth-testkit::dat::chat::scenario_the_tell_key_does_nothing_without_a_player_selected",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.tell.the-key-for-a-private-message-to-the-selected-player-names-him",
        says: "The key that starts a private message to whoever is selected fills the main entry \
               with the beginning of one addressed to him by name, gives it the caret with the caret \
               at the end and nothing picked out, and sends nothing: starting one is the client \
               putting words in its own box.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-5-KEY"),
        station: "dereth-testkit::dat::chat::scenario_the_tell_key_names_the_selected_player",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.tell.the-mark-a-shard-puts-in-front-of-a-name-is-taken-off-before-it-goes-out",
        says: "The mark a shard puts in front of the names on the player's own account is taken off \
               before the name goes out, and nothing else is taken off with it. Every name the \
               recordings' character listings carry is driven: the marked ones lose the mark, the \
               unmarked ones are untouched, and the line that repeats a private message re-uses the \
               name as it went out rather than as it was typed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O167-MARK"),
        station: "dereth-testkit::cpu::chat::scenario_the_mark_in_front_of_a_name_is_taken_off_before_it_goes_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.tell.the-name-on-the-wire-is-the-one-the-line-asked-for-and-no-other",
        says: "The name a private message goes out to is the name the line asked for, written in \
               the same bytes the shard itself used for that name, and no other name the recordings \
               carry appears anywhere in the message. Every speaker the recordings carry is taken \
               off the wire and typed back in, and the count of messages composed is asserted \
               alongside, because a build that sent nothing would satisfy the other half on its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O167-NAME"),
        station: "dereth-testkit::cpu::chat::scenario_the_name_on_the_wire_is_the_one_the_line_asked_for",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.tell.the-three-reply-keys-address-three-different-people",
        says: "There are three keys that begin a reply -- to whoever last wrote to the player, to \
               his monarch and to his patron -- and each addresses his own person by name. With \
               nobody to reply to the key is still taken and the box is left exactly as it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O109-REPLY"),
        station: "dereth-testkit::dat::chat::scenario_the_reply_keys_address_three_different_people",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.turbine.a-failure-answer-names-the-line-and-a-relog-keeps-the-service",
        says: "When the chat-room service refuses a line the player sent, the client tells him which \
               line failed and which room it was for, once; the answer to a line it is not waiting \
               on, and a truncated answer, change nothing at all. Logging one character out and \
               another in leaves the service itself running -- the rooms, the lines still in flight \
               and the how-fast-may-I-talk bucket all survive -- while the new character sends under \
               his own name.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-IN-RESPONSE"),
        station: "dereth-testkit::cpu::chat::scenario_a_failed_room_line_is_named_back_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.turbine.a-line-in-flight-does-not-follow-the-character-out",
        says: "A line the player sent and has not been answered about is still his when he logs out: \
               the answer is taken and the line finished with, whether or not there is a window left \
               to draw it in. The next character sees none of it, and his own lines are numbered on \
               from where the last one left off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-IN-TEARDOWN"),
        station: "dereth-testkit::dat::chat::scenario_a_line_in_flight_does_not_follow_the_character_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.turbine.a-line-is-refused-by-the-option-the-bucket-and-the-safety-check",
        says: "A line typed at a chat room is refused before it is sent when the channel has not \
               started, when the room is unknown, when the player is not listening to that channel, \
               when it is empty, is 256 characters or longer, or carries a forged tell marker or a \
               second line, and when it arrives \
               too soon after the last one. A refusal the player can fix says so in the chat window, \
               and a refused line never costs the player their next one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-SEND"),
        station: "dereth-testkit::cpu::chat::scenario_turbine_line_is_refused_by_three_separate_gates",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.turbine.a-line-typed-before-the-shard-names-a-room-is-refused",
        says: "A line typed at the general channel before the shard has said which room that is goes \
               nowhere at all -- not to the room and not as an ordinary spoken line -- and the player \
               is left on the channel he picked rather than being moved off it. The moment the shard \
               names the room, the next line goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-ROOMLESS"),
        station: "dereth-testkit::dat::chat::scenario_a_line_typed_before_the_shard_names_a_room_is_refused",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.turbine.a-room-line-reaches-the-log-with-its-rooms-own-name-and-colour",
        says: "A line spoken in a chat room reaches the log the player is reading under that room's \
               own bracketed name and in that room's own colour, with the speaker's name left as \
               something he can click rather than as plain letters -- drawn once, and not again on \
               the following frame. The one over-long shape the reference server sends is drawn too, \
               and counted as the overshoot it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-IN-LOG"),
        station: "dereth-testkit::dat::chat::scenario_a_room_line_reaches_the_log_with_its_rooms_name_and_colour",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.turbine.an-answer-that-arrives-after-the-screen-has-gone-completes-quietly",
        says: "An answer about a line the player sent from a window that has since been rebuilt \
               finishes that line without putting it into the new window: the old window is gone and \
               its text stays gone. A line sent afterwards is answered into the window the player is \
               actually looking at, and a second answer about the same line says nothing more.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-IN-GENERATION"),
        station: "dereth-testkit::dat::chat::scenario_an_answer_that_arrives_after_the_screen_has_gone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.turbine.an-incoming-line-is-refused-unless-the-packet-is-whole",
        says: "A chat-room packet is read only when every field it promises is really there: a packet \
               cut short anywhere is refused rather than half-read, and one whose body is longer than \
               the header admits is refused too, except for the one fixed overshoot the reference \
               server sends, which is accepted and counted. A kind of packet the client has no use \
               for is kept exactly as it arrived and draws nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-IN-DECODE"),
        station: "dereth-testkit::cpu::chat::scenario_an_incoming_room_packet_must_be_whole",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.turbine.each-room-is-drawn-with-its-own-name-and-type",
        says: "A line from a chat room is drawn under that room's own bracketed name and on that \
               room's own channel, for every room the shard named, and the room is matched by the \
               number the shard gave it rather than by the order the rooms were listed in. A line \
               from a room the player is in no channel for, a line carrying a forged clickable-name \
               marker, a line carrying a second line inside it, a line from somebody the player has \
               squelched, and a line with no sender at all are each drawn nowhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-IN-ROOMS"),
        station: "dereth-testkit::cpu::chat::scenario_each_chat_room_is_drawn_with_its_own_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.turbine.every-recorded-login-asks-the-client-to-use-the-service",
        says: "Every recorded login the shard has sent asks the client to use the chat-room service, \
               and to offer the expansion's content as well -- so a client whose channel rows are \
               shut is not one the shard said no to.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O235-LOGINS"),
        station: "dereth-testkit::cpu::chat::scenario_every_recorded_login_asks_for_the_room_service",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.turbine.host-text-conversion-happens-before-the-markup-is-read",
        says: "A line arriving from a chat room is first spelled in the host's own narrow text and only \
               then searched for the clickable-name markup, so a line written in wide letters cannot \
               forge one. A character the host cannot spell ends the converted text there, the line is \
               drawn up to that point, and the wire text itself is never rewritten.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-LEGACY-CHAT"),
        station: "dereth-testkit::cpu::chat::scenario_turbine_text_conversion_runs_before_the_markup",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.turbine.the-channel-comes-up-with-the-shards-permission",
        says: "The chat-room system starts only when the shard's character list says it may, and a \
               later list that says it may not does not shut it down again. The rooms the shard names \
               replace whatever the client held, including with zeros, and each channel's row in the \
               talk-to menu is selectable exactly when that channel has a room.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-START"),
        station: "dereth-testkit::cpu::chat::scenario_turbine_channel_comes_up_with_the_shards_permission",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chat.turbine.the-general-channel-carries-a-typed-line-to-its-room",
        says: "The shard's own answer at login is what starts the chat-room service; withheld, \
               nothing starts. With it started, picking the general channel out of the talk-to menu \
               and typing a line sends one chat-room message and not an ordinary spoken one -- to \
               the room the shard named, from the player, on the connection that carries no ordering \
               stamp. Naming the channel in the line itself sends it as a different kind of line, \
               and the service survives the window being rebuilt.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-CHAT"),
        station: "dereth-testkit::dat::chat::scenario_the_general_channel_carries_a_typed_line_to_its_room",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.turbine.the-hosts-own-spelling-is-what-goes-on-the-wire",
        says: "What the player typed is spelled by the host before it leaves, and that spelling is \
               what goes out -- for an ordinary spoken line and for a line to a chat room alike. A \
               character the host cannot spell puts the whole word out in the client's own written \
               form rather than losing it, a host that can spell it sends its own bytes, and the \
               player's history keeps what he actually typed either way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-ACP"),
        station: "dereth-testkit::dat::chat::scenario_the_hosts_own_spelling_is_what_goes_on_the_wire",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.turbine.the-rooms-lines-and-the-ordinary-notices-share-one-queue",
        says: "A line from a chat room and a notice the client wrote itself are drawn in the order \
               they arrived, in one window and one queue, rather than in two streams that overtake \
               each other. The same packet arriving on the ordinary message connection is not a \
               chat-room line at all and draws nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-IN-ORDER"),
        station: "dereth-testkit::dat::chat::scenario_room_lines_and_ordinary_notices_share_one_ordered_queue",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.turbine.the-service-runs-with-no-interface-at-all",
        says: "The chat-room service belongs to the client and not to its interface: with no \
               interface at all the shard's permission still starts it, the general channel is still \
               offered, and lines still go out. Stopping and starting it again does not put the count \
               of lines back to the beginning or forget which room the shard named.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TURBINE-NOUI"),
        station: "dereth-testkit::dat::chat::scenario_the_room_service_runs_with_no_interface_at_all",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.window.a-small-window-writes-its-place-size-openness-and-title-back",
        says: "Each of the four small chat windows writes back where it was put, how big it is, \
               whether it is open and what it is called, so a player's arrangement survives him \
               logging out. A new title is broadcast and taken by the one window it names and by no \
               other.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O109-PLACEMENT"),
        station: "dereth-testkit::dat::chat::scenario_a_floaty_window_writes_its_place_and_title_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.window.how-solid-it-is-follows-the-slider-at-once-and-is-worn-from-login",
        says: "Moving the slider for how solid an idle chat window is drawn fades it at once rather \
               than creeping towards the new value, and the window's background and frame fade with \
               it because they are drawn into its own surface. The backlog inside it has a surface \
               of its own and is not faded. A player whose stored setting arrives with his \
               description wears it from the first frame after logging in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-48"),
        station: "dereth-testkit::dat::chat::scenario_the_idle_opacity_fades_the_chat_window_at_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.window.scrolls-colours-each-line-and-sends-what-is-typed",
        says: "Clicking the chat entry box, typing and pressing Send sends the typed line for the \
               main window, empties the box and keeps the line in its history; the Send button \
               takes the keyboard, so the box has to be clicked again before the next line.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O69-WINDOW"),
        station: "dereth-client::dat::chat::chat_window_interaction::a_click_and_a_typed_line_and_the_send_button_emit_the_chat_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.window.the-close-button-shuts-its-own-window-and-writes-that-down",
        says: "The close button on a small chat window shuts exactly that window and no other, and \
               writes down that it is shut -- so a window the player closed is still closed when he \
               comes back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O109-CLOSE"),
        station: "dereth-testkit::dat::chat::scenario_the_close_button_hides_its_own_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.window.the-keys-that-move-the-log-and-the-keys-that-walk-the-history-are-different",
        says: "The keys that go to the top of the chat log, to the bottom, and a page at a time move \
               the log; the keys that walk back through what the player typed fill the entry \
               instead. They are two different places and the keys do not confuse them. Giving up \
               takes the caret out of the box and leaves what was typed in it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O109-KEYS"),
        station: "dereth-testkit::dat::chat::scenario_the_scroll_keys_move_the_log_and_the_history_keys_fill_the_entry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chat.window.the-mouse-arriving-fades-it-up-to-the-active-opacity",
        says: "A chat window that has faded to its idle opacity while the pointer was away fades \
               back up to its full active opacity once the pointer arrives over it, and stops \
               fading when it gets there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P37-WINDOW"),
        station: "dereth-ui-screens::dat::chat::chat_window_opacity_fade::the_mouse_arriving_fades_the_window_back_up_to_the_active_opacity",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "feedback.classic.explicit-emphasis-survives-identical-panel-text",
        says: "Identical abuse feedback retains the producing operation emphasis; ordinary textbox traffic cannot steal its color or warning sound.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FEEDBACK-CLASSIC-CONSUMER"),
        station: "dereth-classic-ui::lib::runtime::chat_tests::actual_abuse_feedback_survives_an_identical_ordinary_line_and_controls_color_and_audio",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "feedback.delivery.active-face-switches-do-not-replay-pending-transients",
        says: "Switching interfaces discards pending viewport notices belonging to the outgoing interface, while retaining chat history without replaying transient callbacks.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-FEEDBACK-ACTIVE-FACE"),
        station: "dereth-client-shell::lib::front_end::message_tests::active_face_switches_drop_old_pending_transients_without_replaying_chat_history",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "feedback.delivery.producer-metadata-survives-local-and-network-delivery",
        says: "Local operation emphasis and decoded server transient provenance survive final-string delivery; ordinary textbox lines remain ordinary and each receiver and log sees a line once.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FEEDBACK-DELIVERY"),
        station: "dereth-client-runtime::lib::interaction::feedback_tests::real_use_and_decoded_messages_keep_meaning_through_notice_absorb_and_both_hud_routes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "feedback.modern.typed-lines-keep-yellow-replacement-and-expiry",
        says: "The modern viewport draws accepted lines yellow, replaces duplicate text and expires the bound bubble independently of operation emphasis.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FEEDBACK-MODERN-CONSUMER"),
        station: "dereth-client-shell::lib::front_end::message_tests::typed_lines_reach_the_bound_modern_bubble_and_keep_replacement_and_expiry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "feedback.numeric.silent-and-unknown-arms-do-not-create-a-displayed-line",
        says: "Silent and unknown numeric responses do not create a displayed line from their supplied text.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FEEDBACK-NUMERIC-SILENCE"),
        station: "dereth-client-contract::lib::feedback::tests::silent_and_unknown_numeric_arms_do_not_create_a_displayed_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "feedback.numeric.the-producing-arm-keeps-its-code-route-and-established-emphasis",
        says: "Each numeric response keeps its code and channel; only producing arms with established warning emphasis acquire that flag.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FEEDBACK-NUMERIC-EMPHASIS"),
        station: "dereth-client-contract::lib::feedback::tests::numeric_failure_lines_keep_their_code_route_and_established_emphasis",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "feedback.producers.composed-unblock-emphasis-follows-the-appended-refusal",
        says: "Moving equipment to the backpack is informational unless the cannot-unwield suffix was actually appended, even when placement failed after its blocker disappeared.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FEEDBACK-PRODUCER-MEANING"),
        station: "dereth-client-model::lib::inventory::equip::tests::unblock_emphasis_follows_the_actual_suffix_even_when_a_blocker_disappears",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "feedback.producers.successful-casting-keeps-warning-emphasis",
        says: "A successful targeted cast announces its spell name with warning emphasis, while an unknown spell stays silent.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FEEDBACK-CAST-PRODUCER"),
        station: "dereth-client-model::lib::magic::tests::real_targeted_cast_announces_dynamic_spell_name_as_warning_and_unknown_stays_silent",
        tier: Tier::Cpu,
    },
];
