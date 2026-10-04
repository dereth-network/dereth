//! Chat: what reaches the chat window, and what leaves it -- rooms and channels, the talk-to
//! menu, the squelch tab, emotes and death notices, drawn in the shipped gameplay layout.
//!
//! Every scenario here opens the retail dats under `$DERETH_TEST_DAT_DIR`, and **this binary must run
//! serially**: two headless clients in one process share the UI request globals. Incoming lines
//! are packets built here and fed through a real session, or recordings replayed through `Inbound`.
//!
//! `ALL` is this file's own list, concatenated with the other subjects' in `census.rs`, so a
//! scenario that is written and not listed shows up as a shortfall rather than as a silent gap.

use dereth_client_model::chat::TalkFocus;
use dereth_testkit::{ClientSpec, HeadlessClient};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

dereth_testkit::scenarios! {
    scenario_the_talk_to_menu_is_redrawn_from_the_settings => the_talk_to_menu_is_redrawn_from_the_settings ["chat.talk-focus.picking-a-target-redraws-every-row-from-the-settings", "chat.talk-focus.channel-fallback-does-not-require-a-window"],
    scenario_the_general_channel_carries_a_typed_line_to_its_room => the_general_channel_carries_a_typed_line_to_its_room ["chat.turbine.the-general-channel-carries-a-typed-line-to-its-room"],
    scenario_a_line_typed_before_the_shard_names_a_room_is_refused => a_line_typed_before_the_shard_names_a_room_is_refused ["chat.turbine.a-line-typed-before-the-shard-names-a-room-is-refused"],
    scenario_the_room_service_runs_with_no_interface_at_all => the_room_service_runs_with_no_interface_at_all ["chat.turbine.the-service-runs-with-no-interface-at-all"],
    scenario_the_hosts_own_spelling_is_what_goes_on_the_wire => the_hosts_own_spelling_is_what_goes_on_the_wire ["chat.turbine.the-hosts-own-spelling-is-what-goes-on-the-wire"],
    scenario_a_room_line_reaches_the_log_with_its_rooms_name_and_colour => a_room_line_reaches_the_log_with_its_rooms_name_and_colour ["chat.turbine.a-room-line-reaches-the-log-with-its-rooms-own-name-and-colour"],
    scenario_an_answer_that_arrives_after_the_screen_has_gone => an_answer_that_arrives_after_the_screen_has_gone ["chat.turbine.an-answer-that-arrives-after-the-screen-has-gone-completes-quietly"],
    scenario_a_line_in_flight_does_not_follow_the_character_out => a_line_in_flight_does_not_follow_the_character_out ["chat.turbine.a-line-in-flight-does-not-follow-the-character-out"],
    scenario_room_lines_and_ordinary_notices_share_one_ordered_queue => room_lines_and_ordinary_notices_share_one_ordered_queue ["chat.turbine.the-rooms-lines-and-the-ordinary-notices-share-one-queue"],
    scenario_typing_an_emote_sends_it_and_leaves_the_next_line_free => typing_an_emote_sends_it_and_leaves_the_next_line_free ["chat.emote.typing-one-into-the-shipped-entry-sends-it-and-leaves-the-next-line-free"],
    scenario_a_typed_emote_reaches_the_link_on_the_next_frame => a_typed_emote_reaches_the_link_on_the_next_frame ["chat.emote.a-typed-one-reaches-the-link-on-the-next-frame-and-only-once"],
    scenario_a_channel_switched_off_and_on_drops_the_player_back_to_saying_it => a_channel_switched_off_and_on_drops_the_player_back_to_saying_it ["chat.talk-focus.a-channel-switched-off-and-on-again-drops-the-player-back-to-saying-it"],
    scenario_a_new_chat_window_reads_the_settings_now => a_new_chat_window_reads_the_settings_now ["chat.talk-focus.a-new-window-reads-the-settings-now-rather-than-replaying-what-it-missed", "chat.talk-focus.channel-fallback-does-not-require-a-window"],
    scenario_channel_fallback_does_not_require_a_window => channel_fallback_does_not_require_a_window ["chat.talk-focus.channel-fallback-does-not-require-a-window"],
    scenario_the_talk_to_popup_draws_over_the_window_and_gives_the_keyboard_back => the_talk_to_popup_draws_over_the_window_and_gives_the_keyboard_back ["chat.talk-to-menu.the-popup-draws-over-the-window-and-gives-the-keyboard-back"],
    scenario_the_selection_field_follows_what_is_picked_and_clears => the_selection_field_follows_what_is_picked_and_clears ["chat.selection.the-field-beside-the-window-follows-what-is-picked-and-clears"],
    scenario_every_channel_word_sends_its_own_channel => every_channel_word_sends_its_own_channel ["chat.channel-commands.every-word-sends-its-own-channel-and-says-nothing-locally"],
    scenario_an_empty_channel_command_says_what_to_do_and_sends_nothing => an_empty_channel_command_says_what_to_do_and_sends_nothing ["chat.channel-commands.one-with-no-text-says-what-to-do-and-sends-nothing"],
    scenario_the_bottom_row_of_the_log_is_the_newest_line => the_bottom_row_of_the_log_is_the_newest_line ["chat.log.the-bottom-row-is-the-newest-line-and-never-an-empty-one"],
    scenario_the_newline_between_lines_is_a_separator => the_newline_between_lines_is_a_separator ["chat.log.the-newline-between-lines-is-a-separator-and-not-a-terminator"],
    scenario_the_log_draws_the_name_and_not_the_markup => the_log_draws_the_name_and_not_the_markup ["chat.tell-markup.the-log-draws-the-name-and-not-the-markup"],
    scenario_the_clickable_name_is_drawn_in_its_own_colour => the_clickable_name_is_drawn_in_its_own_colour ["chat.tell-markup.the-clickable-name-is-drawn-in-its-own-colour"],
    scenario_markup_the_client_does_not_know_is_drawn_as_it_stands => markup_the_client_does_not_know_is_drawn_as_it_stands ["chat.tell-markup.markup-the-client-does-not-know-is-drawn-as-it-stands"],
    scenario_the_squelch_tab_opens_with_both_buttons_lit => the_squelch_tab_opens_with_both_buttons_lit ["chat.squelch-panel.the-tab-opens-with-both-buttons-lit-over-an-empty-box"],
    scenario_emptying_the_squelch_name_box_dims_both_buttons => emptying_the_squelch_name_box_dims_both_buttons ["chat.squelch-panel.emptying-the-name-box-darkens-both-buttons-and-typing-arms-them"],
    scenario_squelching_clears_the_box_and_dims_that_button_alone => squelching_clears_the_box_and_dims_that_button_alone ["chat.squelch-panel.squelching-clears-the-box-and-darkens-that-button-alone"],
    scenario_the_shards_answer_lights_both_buttons_and_adds_the_row => the_shards_answer_lights_both_buttons_and_adds_the_row ["chat.squelch-panel.the-shards-answer-lights-both-buttons-again-and-adds-the-row"],
    scenario_picking_a_squelch_row_lights_both_buttons_again => picking_a_squelch_row_lights_both_buttons_again ["chat.squelch-panel.picking-a-row-lights-both-buttons-again-over-an-empty-box"],
    scenario_the_remove_button_follows_the_selection => the_remove_button_follows_the_selection ["chat.squelch-panel.remove-follows-what-is-picked"],
    scenario_the_squelch_tab_lists_who_the_shard_says_is_squelched => the_squelch_tab_lists_who_the_shard_says_is_squelched ["chat.squelch-panel.the-tab-lists-who-the-shard-says-is-squelched-in-one-sorted-block"],
    scenario_the_two_squelch_buttons_send_different_messages => the_two_squelch_buttons_send_different_messages ["chat.squelch-panel.the-two-buttons-send-different-messages-about-the-same-name"],
    scenario_removing_sends_the_kind_the_row_itself_names => removing_sends_the_kind_the_row_itself_names ["chat.squelch-panel.removing-sends-the-kind-the-row-itself-names"],
    scenario_the_squelch_name_box_takes_the_caret_from_a_press => the_squelch_name_box_takes_the_caret_from_a_press ["chat.squelch-panel.the-name-box-takes-the-caret-from-a-press"],
    scenario_the_name_label_is_wider_than_its_box_and_wraps => the_name_label_is_wider_than_its_box_and_wraps ["chat.squelch-panel.the-name-label-is-wider-than-its-box-and-wraps-in-retail-too"],
    scenario_every_window_offers_the_same_filter_rows => every_window_offers_the_same_filter_rows ["chat.filters.every-window-offers-the-same-rows-and-each-row-lies-inside-its-control"],
    scenario_the_global_channels_are_rows_of_that_list_and_are_drawn => the_global_channels_are_rows_of_that_list_and_are_drawn ["chat.filters.the-global-channels-are-rows-of-that-list-and-are-drawn"],
    scenario_ticking_a_filter_row_writes_that_windows_own_filter => ticking_a_filter_row_writes_that_windows_own_filter ["chat.filters.ticking-a-row-writes-that-windows-own-filter-and-sends-nothing"],
    scenario_typing_past_the_entrys_edge_keeps_the_caret_in_view => typing_past_the_entrys_edge_keeps_the_caret_in_view ["chat.entry.typing-past-the-edge-slides-the-line-so-the-caret-stays-in-view"],
    scenario_changing_the_chat_font_size_remeasures_the_backlog => changing_the_chat_font_size_remeasures_the_backlog ["chat.log.changing-the-font-size-remeasures-the-backlog-and-the-next-line"],
    scenario_the_idle_opacity_fades_the_chat_window_at_once => the_idle_opacity_fades_the_chat_window_at_once ["chat.window.how-solid-it-is-follows-the-slider-at-once-and-is-worn-from-login"],
    scenario_the_return_key_sends_the_line_and_gives_the_keyboard_back => the_return_key_sends_the_line_and_gives_the_keyboard_back ["chat.entry.the-return-key-sends-the-line-and-gives-the-keyboard-back"],
    scenario_the_send_button_keeps_the_caret_for_itself => the_send_button_keeps_the_caret_for_itself ["chat.entry.the-send-button-keeps-the-caret-for-itself"],
    scenario_a_run_of_characters_all_arrives_in_the_entry => a_run_of_characters_all_arrives_in_the_entry ["chat.entry.a-run-of-characters-all-arrives-and-none-overwrites-the-last"],
    scenario_an_action_nobody_claims_is_dispatched_once => an_action_nobody_claims_is_dispatched_once ["chat.input.a-press-nobody-claims-is-handed-round-once-and-then-dropped"],
    scenario_the_eat_the_next_character_latch_is_armed_on_one_edge => the_eat_the_next_character_latch_is_armed_on_one_edge ["chat.input.the-client-is-set-to-eat-a-character-on-one-edge-and-never-otherwise"],
    scenario_a_recorded_broadcast_that_names_its_speaker_is_drawn => a_recorded_broadcast_that_names_its_speaker_is_drawn ["chat.channel.a-recorded-broadcast-that-names-its-speaker-is-drawn-in-its-own-colour"],
    scenario_every_channel_draws_its_own_line_in_its_own_colour => every_channel_draws_its_own_line_in_its_own_colour ["chat.channel.every-channel-draws-its-own-line-in-its-own-colour"],
    scenario_a_squelched_speaker_is_still_heard_on_a_channel => a_squelched_speaker_is_still_heard_on_a_channel ["chat.channel.silencing-the-speaker-does-not-silence-a-channel-line"],
    scenario_every_recorded_kill_notification_is_drawn_verbatim => every_recorded_kill_notification_is_drawn_verbatim ["chat.death.every-recorded-kill-notification-is-drawn-word-for-word-on-the-log"],
    scenario_your_own_death_goes_through_the_same_hand => your_own_death_goes_through_the_same_hand ["chat.death.your-own-death-goes-through-the-same-hand-and-an-empty-one-says-nothing"],
    scenario_a_death_you_were_part_of_reaches_the_log_only_when_you_were_not => a_death_you_were_part_of_reaches_the_log_only_when_you_were_not ["chat.death.a-death-the-player-was-part-of-reaches-the-log-only-when-he-was-not"],
    scenario_no_death_line_is_silenced_where_a_combat_line_is => no_death_line_is_silenced_where_a_combat_line_is ["chat.death.no-death-line-can-be-silenced-where-a-combat-line-can"],
    scenario_a_key_bound_to_a_pose_moves_the_body => a_key_bound_to_a_pose_moves_the_body ["chat.pose.a-key-bound-to-a-pose-moves-the-body-and-one-that-is-not-is-bound-to-nothing"],
    scenario_a_run_between_stars_is_performed_and_the_rest_spoken => a_run_between_stars_is_performed_and_the_rest_spoken ["chat.pose.a-run-between-stars-is-performed-and-the-rest-of-the-line-is-spoken"],
    scenario_a_run_alone_says_nothing_and_an_unknown_one_is_spoken => a_run_alone_says_nothing_and_an_unknown_one_is_spoken ["chat.pose.a-run-alone-says-nothing-and-one-the-client-cannot-place-is-spoken-whole"],
    scenario_the_say_command_goes_through_the_same_extraction => the_say_command_goes_through_the_same_extraction ["chat.pose.the-say-command-goes-through-the-same-extraction"],
    scenario_the_emotes_command_prints_the_shipped_list => the_emotes_command_prints_the_shipped_list ["chat.pose.the-list-command-prints-the-shipped-list-and-sends-nothing"],
    scenario_a_pose_sends_a_different_message_from_the_emote_command => a_pose_sends_a_different_message_from_the_emote_command ["chat.pose.what-a-pose-sends-is-a-different-message-from-what-the-command-sends"],
    scenario_the_possessive_word_is_chosen_by_sex => the_possessive_word_is_chosen_by_sex ["chat.pose.the-possessive-word-is-chosen-by-sex-before-the-message-leaves"],
    scenario_the_players_own_echo_is_never_turned_into_a_noise => the_players_own_echo_is_never_turned_into_a_noise ["chat.pose.the-players-own-echo-is-never-turned-into-a-noise"],
    scenario_every_pose_the_list_advertises_can_be_performed => every_pose_the_list_advertises_can_be_performed ["chat.pose.every-pose-the-list-advertises-can-really-be-performed"],
    scenario_clicking_a_name_in_the_log_starts_a_tell_to_him => clicking_a_name_in_the_log_starts_a_tell_to_him ["chat.tell.clicking-a-name-in-the-log-starts-a-private-message-to-him"],
    scenario_the_tell_key_names_the_selected_player => the_tell_key_names_the_selected_player ["chat.tell.the-key-for-a-private-message-to-the-selected-player-names-him"],
    scenario_the_selected_players_name_fills_both_changing_rows => the_selected_players_name_fills_both_changing_rows ["chat.talk-to-menu.the-selected-players-name-fills-both-of-the-changing-rows"],
    scenario_the_tell_key_does_nothing_without_a_player_selected => the_tell_key_does_nothing_without_a_player_selected ["chat.tell.the-key-does-nothing-when-nobody-a-player-could-talk-to-is-selected"],
    scenario_picking_a_friend_and_pressing_tell_fills_the_entry => picking_a_friend_and_pressing_tell_fills_the_entry ["chat.tell.picking-a-friend-and-pressing-tell-fills-the-entry-and-replaces-what-was-there"],
    scenario_only_the_main_window_takes_the_tell => only_the_main_window_takes_the_tell ["chat.tell.only-the-main-window-takes-the-tell"],
    scenario_two_recorded_channels_draw_in_two_different_colours => two_recorded_channels_draw_in_two_different_colours ["chat.log.two-recorded-kinds-of-line-draw-in-two-different-colours"],
    scenario_your_own_echo_and_a_remote_speaker_are_drawn_apart => your_own_echo_and_a_remote_speaker_are_drawn_apart ["chat.speech.your-own-echo-and-a-remote-speaker-are-drawn-from-two-different-forms"],
    scenario_the_line_the_return_key_sends_is_remembered => the_line_the_return_key_sends_is_remembered ["chat.entry.the-line-the-return-key-sends-is-remembered-and-raised-for-its-own-window"],
    scenario_the_scroll_keys_move_the_log_and_the_history_keys_fill_the_entry => the_scroll_keys_move_the_log_and_the_history_keys_fill_the_entry ["chat.window.the-keys-that-move-the-log-and-the-keys-that-walk-the-history-are-different"],
    scenario_what_a_typed_line_becomes_follows_the_menu => what_a_typed_line_becomes_follows_the_menu ["chat.talk-focus.what-a-typed-line-becomes-follows-the-menu"],
    scenario_a_command_the_client_handles_is_never_spoken => a_command_the_client_handles_is_never_spoken ["chat.commands.a-command-the-client-handles-is-never-spoken-to-the-shard"],
    scenario_the_button_caption_follows_the_menu => the_button_caption_follows_the_menu ["chat.talk-to-menu.the-button-caption-follows-the-menu-and-a-row-that-is-shut-refuses"],
    scenario_a_rows_place_in_the_list_is_not_the_order_of_the_channels => a_rows_place_in_the_list_is_not_the_order_of_the_channels ["chat.talk-to-menu.which-channel-a-row-is-comes-from-the-row-and-not-its-place"],
    scenario_the_reply_keys_address_three_different_people => the_reply_keys_address_three_different_people ["chat.tell.the-three-reply-keys-address-three-different-people"],
    scenario_a_floaty_window_writes_its_place_and_title_back => a_floaty_window_writes_its_place_and_title_back ["chat.window.a-small-window-writes-its-place-size-openness-and-title-back"],
    scenario_the_close_button_hides_its_own_window => the_close_button_hides_its_own_window ["chat.window.the-close-button-shuts-its-own-window-and-writes-that-down"],
}

// =============================================================================================
// The chat-room service: starting it, typing into it, and what it draws
// =============================================================================================
//
// **No socket is opened anywhere here.** An outgoing line is read out of the client's own outbox
// and, where the bytes are the claim, handed to the production sender over a mock transport; an
// incoming one is a packet this file builds and feeds through a real session, which is what decides
// that a packet on that queue is a chat-room line at all.

use dereth_client_net::client_session::testing::{Corpus, Direction, MockTransport};
use dereth_client_net::client_session::{Session, SessionEvent};
use dereth_primitives::{IncomingMessage, LocalTime, NetBlobId, NetQueue, ObjectId, RecipientId};
use dereth_protocol::login::LoginCharacterSet;
use dereth_testkit::adapters_chat::Hand;
use dereth_testkit::Inbound;
use dereth_ui::{ElemHandle, ElementId, StateId};

/// Behaviour: chat.entry-adapters
#[test]
fn modern_entry_batches_keep_shared_replies_history_aliases_and_widget_edits_current() {
    fn action(c: &mut HeadlessClient, action: u32) {
        c.app_mut()
            .input_manager_mut()
            .unwrap()
            .inject_action(dereth_input::InputEvent {
                action: dereth_input::ActionId(action),
                input_map: dereth_input::InputMapId(0x10000009),
                toggle: dereth_input::ToggleType::OneShot,
                extent: 1.0,
                start: true,
                repeat_delta: 1,
                repeat_total: 0,
                from_key_down: false,
            });
    }
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut hand = Hand::new();
    let entry = main_chat_entry(&c);
    hand.click_handle(&mut c, entry);
    c.world_mut().chat.last_monarch_sender = "Monarch Peer".into();
    c.world_mut().chat.last_teller_name = "Last Peer".into();
    action(&mut c, 0x10000020);
    hand.character(&mut c, 'H');
    c.tick(1);
    assert_eq!(element_text_of(&mut c, entry), "@tell Monarch Peer, H");
    hand.key(&mut c, key_of(winit::keyboard::KeyCode::Enter), true);
    c.tick(1);
    assert_eq!(chat_history(&mut c), ["@tell Monarch Peer, H"]);
    hand.key(&mut c, key_of(winit::keyboard::KeyCode::Enter), false);
    hand.click_handle(&mut c, entry);
    action(&mut c, ACTION_HISTORY_BACK);
    hand.character(&mut c, 'X');
    c.tick(1);
    hand.key(&mut c, key_of(winit::keyboard::KeyCode::Enter), true);
    c.tick(1);
    assert_eq!(
        chat_history(&mut c),
        ["@tell Monarch Peer, H", "@tell Monarch Peer, HX"]
    );
    hand.key(&mut c, key_of(winit::keyboard::KeyCode::Enter), false);
    hand.click_handle(&mut c, entry);
    for ch in "@r A".chars() {
        hand.character(&mut c, ch);
    }
    c.tick(1);
    hand.key(&mut c, key_of(winit::keyboard::KeyCode::Enter), true);
    c.tick(1);
    assert_eq!(chat_history(&mut c).last().unwrap(), "@tell Last Peer, A");
    assert_eq!(c.view().expect_app().interaction().stats.chat_lines_sent, 3);
    hand.key(&mut c, key_of(winit::keyboard::KeyCode::Enter), false);
    with_screen(&mut c, |ui, s| {
        s.chat_on_action(ui, ACTION_OPEN_ENTRY);
        ui.text_element_mut(entry).unwrap().paste("pasted");
    });
    c.tick(1);
    assert_eq!(c.view().world().chat.entries[&8].text, "pasted");
    hand.key(&mut c, key_of(winit::keyboard::KeyCode::Backspace), true);
    c.tick(1);
    assert_eq!(c.view().world().chat.entries[&8].text, "paste");
    let update = dereth_client_contract::chat::entry::EntryUpdate {
        window: 8,
        text: c.view().world().chat.entries[&8].text.clone(),
        cursor: 5,
        focus: false,
    };
    with_screen(&mut c, |ui, s| {
        ui.text_element_mut(entry).unwrap().set_text("stale");
        s.chat_entry_update(ui, &update);
    });
    assert_eq!(element_text_of(&mut c, entry), "paste");
    assert_eq!(
        chat_history(&mut c).len(),
        3,
        "restoring a draft does not submit or replay it"
    );
    c.shutdown();
}

mod menu;
use menu::{
    a_channel_switched_off_and_on_drops_the_player_back_to_saying_it,
    a_new_chat_window_reads_the_settings_now, channel_fallback_does_not_require_a_window,
    the_selection_field_follows_what_is_picked_and_clears,
    the_talk_to_menu_is_redrawn_from_the_settings,
    the_talk_to_popup_draws_over_the_window_and_gives_the_keyboard_back, GENERAL_ROW,
};

mod rooms;
use rooms::{
    a_line_in_flight_does_not_follow_the_character_out,
    a_line_typed_before_the_shard_names_a_room_is_refused,
    a_room_line_reaches_the_log_with_its_rooms_name_and_colour,
    an_answer_that_arrives_after_the_screen_has_gone,
    room_lines_and_ordinary_notices_share_one_ordered_queue,
    the_general_channel_carries_a_typed_line_to_its_room,
    the_hosts_own_spelling_is_what_goes_on_the_wire,
    the_room_service_runs_with_no_interface_at_all, ROOMS_SESSION, TALK_TO_BUTTON,
};

mod support;
use support::{
    a_bare_client, a_client_listening, chat_history, colour_runs, describe, drawn_line, element,
    element_text_of, focus_of, into_the_window, key_of, log_text, main_chat_entry, mark,
    on_the_wire, open_the_options_page, pick_general, recorded, to_gameplay, with_screen, word,
};

mod emote_entry;
use emote_entry::{
    a_typed_emote_reaches_the_link_on_the_next_frame,
    typing_an_emote_sends_it_and_leaves_the_next_line_free,
};

mod channel_commands;
use channel_commands::{
    an_empty_channel_command_says_what_to_do_and_sends_nothing,
    every_channel_word_sends_its_own_channel,
};

mod log;
use log::{the_bottom_row_of_the_log_is_the_newest_line, the_newline_between_lines_is_a_separator};

mod links;
use links::{
    markup_the_client_does_not_know_is_drawn_as_it_stands,
    the_clickable_name_is_drawn_in_its_own_colour, the_log_draws_the_name_and_not_the_markup,
};

mod squelch;
use squelch::{
    emptying_the_squelch_name_box_dims_both_buttons,
    picking_a_squelch_row_lights_both_buttons_again, removing_sends_the_kind_the_row_itself_names,
    squelching_clears_the_box_and_dims_that_button_alone,
    the_name_label_is_wider_than_its_box_and_wraps, the_remove_button_follows_the_selection,
    the_shards_answer_lights_both_buttons_and_adds_the_row,
    the_squelch_name_box_takes_the_caret_from_a_press,
    the_squelch_tab_lists_who_the_shard_says_is_squelched,
    the_squelch_tab_opens_with_both_buttons_lit, the_two_squelch_buttons_send_different_messages,
};

mod options;
use options::{
    every_window_offers_the_same_filter_rows,
    the_global_channels_are_rows_of_that_list_and_are_drawn,
    ticking_a_filter_row_writes_that_windows_own_filter,
};

mod appearance;
use appearance::{
    changing_the_chat_font_size_remeasures_the_backlog,
    the_idle_opacity_fades_the_chat_window_at_once,
    typing_past_the_entrys_edge_keeps_the_caret_in_view,
};

mod entry;
use entry::{
    a_run_of_characters_all_arrives_in_the_entry, an_action_nobody_claims_is_dispatched_once,
    the_eat_the_next_character_latch_is_armed_on_one_edge,
    the_return_key_sends_the_line_and_gives_the_keyboard_back,
    the_send_button_keeps_the_caret_for_itself,
};

mod channel_display;
use channel_display::{
    a_recorded_broadcast_that_names_its_speaker_is_drawn,
    a_squelched_speaker_is_still_heard_on_a_channel,
    every_channel_draws_its_own_line_in_its_own_colour,
    two_recorded_channels_draw_in_two_different_colours,
    your_own_echo_and_a_remote_speaker_are_drawn_apart, OPAQUE,
};

mod deaths;
use deaths::{
    a_death_you_were_part_of_reaches_the_log_only_when_you_were_not,
    every_recorded_kill_notification_is_drawn_verbatim,
    no_death_line_is_silenced_where_a_combat_line_is, your_own_death_goes_through_the_same_hand,
};

mod poses;
use poses::{
    a_key_bound_to_a_pose_moves_the_body, a_pose_sends_a_different_message_from_the_emote_command,
    a_run_alone_says_nothing_and_an_unknown_one_is_spoken,
    a_run_between_stars_is_performed_and_the_rest_spoken,
    every_pose_the_list_advertises_can_be_performed, the_emotes_command_prints_the_shipped_list,
    the_players_own_echo_is_never_turned_into_a_noise, the_possessive_word_is_chosen_by_sex,
    the_say_command_goes_through_the_same_extraction,
};

mod tell;
use tell::{
    clicking_a_name_in_the_log_starts_a_tell_to_him, only_the_main_window_takes_the_tell,
    picking_a_friend_and_pressing_tell_fills_the_entry,
    the_selected_players_name_fills_both_changing_rows,
    the_tell_key_does_nothing_without_a_player_selected, the_tell_key_names_the_selected_player,
};

mod surface;
use surface::{
    a_command_the_client_handles_is_never_spoken, a_floaty_window_writes_its_place_and_title_back,
    a_rows_place_in_the_list_is_not_the_order_of_the_channels, the_button_caption_follows_the_menu,
    the_close_button_hides_its_own_window, the_line_the_return_key_sends_is_remembered,
    the_reply_keys_address_three_different_people,
    the_scroll_keys_move_the_log_and_the_history_keys_fill_the_entry,
    what_a_typed_line_becomes_follows_the_menu, ACTION_HISTORY_BACK, ACTION_OPEN_ENTRY,
};
