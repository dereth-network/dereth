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

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "the_talk_to_menu_is_redrawn_from_the_settings",
        &["chat.talk-focus.picking-a-target-redraws-every-row-from-the-settings", "chat.talk-focus.channel-fallback-does-not-require-a-window"],
        the_talk_to_menu_is_redrawn_from_the_settings,
    ),
    (
        "the_general_channel_carries_a_typed_line_to_its_room",
        &["chat.turbine.the-general-channel-carries-a-typed-line-to-its-room"],
        the_general_channel_carries_a_typed_line_to_its_room,
    ),
    (
        "a_line_typed_before_the_shard_names_a_room_is_refused",
        &["chat.turbine.a-line-typed-before-the-shard-names-a-room-is-refused"],
        a_line_typed_before_the_shard_names_a_room_is_refused,
    ),
    (
        "the_room_service_runs_with_no_interface_at_all",
        &["chat.turbine.the-service-runs-with-no-interface-at-all"],
        the_room_service_runs_with_no_interface_at_all,
    ),
    (
        "the_hosts_own_spelling_is_what_goes_on_the_wire",
        &["chat.turbine.the-hosts-own-spelling-is-what-goes-on-the-wire"],
        the_hosts_own_spelling_is_what_goes_on_the_wire,
    ),
    (
        "a_room_line_reaches_the_log_with_its_rooms_name_and_colour",
        &["chat.turbine.a-room-line-reaches-the-log-with-its-rooms-own-name-and-colour"],
        a_room_line_reaches_the_log_with_its_rooms_name_and_colour,
    ),
    (
        "an_answer_that_arrives_after_the_screen_has_gone",
        &["chat.turbine.an-answer-that-arrives-after-the-screen-has-gone-completes-quietly"],
        an_answer_that_arrives_after_the_screen_has_gone,
    ),
    (
        "a_line_in_flight_does_not_follow_the_character_out",
        &["chat.turbine.a-line-in-flight-does-not-follow-the-character-out"],
        a_line_in_flight_does_not_follow_the_character_out,
    ),
    (
        "room_lines_and_ordinary_notices_share_one_ordered_queue",
        &["chat.turbine.the-rooms-lines-and-the-ordinary-notices-share-one-queue"],
        room_lines_and_ordinary_notices_share_one_ordered_queue,
    ),
    (
        "typing_an_emote_sends_it_and_leaves_the_next_line_free",
        &["chat.emote.typing-one-into-the-shipped-entry-sends-it-and-leaves-the-next-line-free"],
        typing_an_emote_sends_it_and_leaves_the_next_line_free,
    ),
    (
        "a_typed_emote_reaches_the_link_on_the_next_frame",
        &["chat.emote.a-typed-one-reaches-the-link-on-the-next-frame-and-only-once"],
        a_typed_emote_reaches_the_link_on_the_next_frame,
    ),
    (
        "a_channel_switched_off_and_on_drops_the_player_back_to_saying_it",
        &["chat.talk-focus.a-channel-switched-off-and-on-again-drops-the-player-back-to-saying-it"],
        a_channel_switched_off_and_on_drops_the_player_back_to_saying_it,
    ),
    (
        "a_new_chat_window_reads_the_settings_now",
        &["chat.talk-focus.a-new-window-reads-the-settings-now-rather-than-replaying-what-it-missed", "chat.talk-focus.channel-fallback-does-not-require-a-window"],
        a_new_chat_window_reads_the_settings_now,
    ),
    (
        "channel_fallback_does_not_require_a_window",
        &["chat.talk-focus.channel-fallback-does-not-require-a-window"],
        channel_fallback_does_not_require_a_window,
    ),
    (
        "the_talk_to_popup_draws_over_the_window_and_gives_the_keyboard_back",
        &["chat.talk-to-menu.the-popup-draws-over-the-window-and-gives-the-keyboard-back"],
        the_talk_to_popup_draws_over_the_window_and_gives_the_keyboard_back,
    ),
    (
        "the_selection_field_follows_what_is_picked_and_clears",
        &["chat.selection.the-field-beside-the-window-follows-what-is-picked-and-clears"],
        the_selection_field_follows_what_is_picked_and_clears,
    ),
    (
        "every_channel_word_sends_its_own_channel",
        &["chat.channel-commands.every-word-sends-its-own-channel-and-says-nothing-locally"],
        every_channel_word_sends_its_own_channel,
    ),
    (
        "an_empty_channel_command_says_what_to_do_and_sends_nothing",
        &["chat.channel-commands.one-with-no-text-says-what-to-do-and-sends-nothing"],
        an_empty_channel_command_says_what_to_do_and_sends_nothing,
    ),
    (
        "the_bottom_row_of_the_log_is_the_newest_line",
        &["chat.log.the-bottom-row-is-the-newest-line-and-never-an-empty-one"],
        the_bottom_row_of_the_log_is_the_newest_line,
    ),
    (
        "the_newline_between_lines_is_a_separator",
        &["chat.log.the-newline-between-lines-is-a-separator-and-not-a-terminator"],
        the_newline_between_lines_is_a_separator,
    ),
    (
        "the_log_draws_the_name_and_not_the_markup",
        &["chat.tell-markup.the-log-draws-the-name-and-not-the-markup"],
        the_log_draws_the_name_and_not_the_markup,
    ),
    (
        "the_clickable_name_is_drawn_in_its_own_colour",
        &["chat.tell-markup.the-clickable-name-is-drawn-in-its-own-colour"],
        the_clickable_name_is_drawn_in_its_own_colour,
    ),
    (
        "markup_the_client_does_not_know_is_drawn_as_it_stands",
        &["chat.tell-markup.markup-the-client-does-not-know-is-drawn-as-it-stands"],
        markup_the_client_does_not_know_is_drawn_as_it_stands,
    ),
    (
        "the_squelch_tab_opens_with_both_buttons_lit",
        &["chat.squelch-panel.the-tab-opens-with-both-buttons-lit-over-an-empty-box"],
        the_squelch_tab_opens_with_both_buttons_lit,
    ),
    (
        "emptying_the_squelch_name_box_dims_both_buttons",
        &["chat.squelch-panel.emptying-the-name-box-darkens-both-buttons-and-typing-arms-them"],
        emptying_the_squelch_name_box_dims_both_buttons,
    ),
    (
        "squelching_clears_the_box_and_dims_that_button_alone",
        &["chat.squelch-panel.squelching-clears-the-box-and-darkens-that-button-alone"],
        squelching_clears_the_box_and_dims_that_button_alone,
    ),
    (
        "the_shards_answer_lights_both_buttons_and_adds_the_row",
        &["chat.squelch-panel.the-shards-answer-lights-both-buttons-again-and-adds-the-row"],
        the_shards_answer_lights_both_buttons_and_adds_the_row,
    ),
    (
        "picking_a_squelch_row_lights_both_buttons_again",
        &["chat.squelch-panel.picking-a-row-lights-both-buttons-again-over-an-empty-box"],
        picking_a_squelch_row_lights_both_buttons_again,
    ),
    (
        "the_remove_button_follows_the_selection",
        &["chat.squelch-panel.remove-follows-what-is-picked"],
        the_remove_button_follows_the_selection,
    ),
    (
        "the_squelch_tab_lists_who_the_shard_says_is_squelched",
        &["chat.squelch-panel.the-tab-lists-who-the-shard-says-is-squelched-in-one-sorted-block"],
        the_squelch_tab_lists_who_the_shard_says_is_squelched,
    ),
    (
        "the_two_squelch_buttons_send_different_messages",
        &["chat.squelch-panel.the-two-buttons-send-different-messages-about-the-same-name"],
        the_two_squelch_buttons_send_different_messages,
    ),
    (
        "removing_sends_the_kind_the_row_itself_names",
        &["chat.squelch-panel.removing-sends-the-kind-the-row-itself-names"],
        removing_sends_the_kind_the_row_itself_names,
    ),
    (
        "the_squelch_name_box_takes_the_caret_from_a_press",
        &["chat.squelch-panel.the-name-box-takes-the-caret-from-a-press"],
        the_squelch_name_box_takes_the_caret_from_a_press,
    ),
    (
        "the_name_label_is_wider_than_its_box_and_wraps",
        &["chat.squelch-panel.the-name-label-is-wider-than-its-box-and-wraps-in-retail-too"],
        the_name_label_is_wider_than_its_box_and_wraps,
    ),
    (
        "every_window_offers_the_same_filter_rows",
        &["chat.filters.every-window-offers-the-same-rows-and-each-row-lies-inside-its-control"],
        every_window_offers_the_same_filter_rows,
    ),
    (
        "the_global_channels_are_rows_of_that_list_and_are_drawn",
        &["chat.filters.the-global-channels-are-rows-of-that-list-and-are-drawn"],
        the_global_channels_are_rows_of_that_list_and_are_drawn,
    ),
    (
        "ticking_a_filter_row_writes_that_windows_own_filter",
        &["chat.filters.ticking-a-row-writes-that-windows-own-filter-and-sends-nothing"],
        ticking_a_filter_row_writes_that_windows_own_filter,
    ),
    (
        "typing_past_the_entrys_edge_keeps_the_caret_in_view",
        &["chat.entry.typing-past-the-edge-slides-the-line-so-the-caret-stays-in-view"],
        typing_past_the_entrys_edge_keeps_the_caret_in_view,
    ),
    (
        "changing_the_chat_font_size_remeasures_the_backlog",
        &["chat.log.changing-the-font-size-remeasures-the-backlog-and-the-next-line"],
        changing_the_chat_font_size_remeasures_the_backlog,
    ),
    (
        "the_idle_opacity_fades_the_chat_window_at_once",
        &["chat.window.how-solid-it-is-follows-the-slider-at-once-and-is-worn-from-login"],
        the_idle_opacity_fades_the_chat_window_at_once,
    ),
    (
        "the_return_key_sends_the_line_and_gives_the_keyboard_back",
        &["chat.entry.the-return-key-sends-the-line-and-gives-the-keyboard-back"],
        the_return_key_sends_the_line_and_gives_the_keyboard_back,
    ),
    (
        "the_send_button_keeps_the_caret_for_itself",
        &["chat.entry.the-send-button-keeps-the-caret-for-itself"],
        the_send_button_keeps_the_caret_for_itself,
    ),
    (
        "a_run_of_characters_all_arrives_in_the_entry",
        &["chat.entry.a-run-of-characters-all-arrives-and-none-overwrites-the-last"],
        a_run_of_characters_all_arrives_in_the_entry,
    ),
    (
        "an_action_nobody_claims_is_dispatched_once",
        &["chat.input.a-press-nobody-claims-is-handed-round-once-and-then-dropped"],
        an_action_nobody_claims_is_dispatched_once,
    ),
    (
        "the_eat_the_next_character_latch_is_armed_on_one_edge",
        &["chat.input.the-client-is-set-to-eat-a-character-on-one-edge-and-never-otherwise"],
        the_eat_the_next_character_latch_is_armed_on_one_edge,
    ),
    (
        "a_recorded_broadcast_that_names_its_speaker_is_drawn",
        &["chat.channel.a-recorded-broadcast-that-names-its-speaker-is-drawn-in-its-own-colour"],
        a_recorded_broadcast_that_names_its_speaker_is_drawn,
    ),
    (
        "every_channel_draws_its_own_line_in_its_own_colour",
        &["chat.channel.every-channel-draws-its-own-line-in-its-own-colour"],
        every_channel_draws_its_own_line_in_its_own_colour,
    ),
    (
        "a_squelched_speaker_is_still_heard_on_a_channel",
        &["chat.channel.silencing-the-speaker-does-not-silence-a-channel-line"],
        a_squelched_speaker_is_still_heard_on_a_channel,
    ),
    (
        "every_recorded_kill_notification_is_drawn_verbatim",
        &["chat.death.every-recorded-kill-notification-is-drawn-word-for-word-on-the-log"],
        every_recorded_kill_notification_is_drawn_verbatim,
    ),
    (
        "your_own_death_goes_through_the_same_hand",
        &["chat.death.your-own-death-goes-through-the-same-hand-and-an-empty-one-says-nothing"],
        your_own_death_goes_through_the_same_hand,
    ),
    (
        "a_death_you_were_part_of_reaches_the_log_only_when_you_were_not",
        &["chat.death.a-death-the-player-was-part-of-reaches-the-log-only-when-he-was-not"],
        a_death_you_were_part_of_reaches_the_log_only_when_you_were_not,
    ),
    (
        "no_death_line_is_silenced_where_a_combat_line_is",
        &["chat.death.no-death-line-can-be-silenced-where-a-combat-line-can"],
        no_death_line_is_silenced_where_a_combat_line_is,
    ),
    (
        "a_key_bound_to_a_pose_moves_the_body",
        &["chat.pose.a-key-bound-to-a-pose-moves-the-body-and-one-that-is-not-is-bound-to-nothing"],
        a_key_bound_to_a_pose_moves_the_body,
    ),
    (
        "a_run_between_stars_is_performed_and_the_rest_spoken",
        &["chat.pose.a-run-between-stars-is-performed-and-the-rest-of-the-line-is-spoken"],
        a_run_between_stars_is_performed_and_the_rest_spoken,
    ),
    (
        "a_run_alone_says_nothing_and_an_unknown_one_is_spoken",
        &["chat.pose.a-run-alone-says-nothing-and-one-the-client-cannot-place-is-spoken-whole"],
        a_run_alone_says_nothing_and_an_unknown_one_is_spoken,
    ),
    (
        "the_say_command_goes_through_the_same_extraction",
        &["chat.pose.the-say-command-goes-through-the-same-extraction"],
        the_say_command_goes_through_the_same_extraction,
    ),
    (
        "the_emotes_command_prints_the_shipped_list",
        &["chat.pose.the-list-command-prints-the-shipped-list-and-sends-nothing"],
        the_emotes_command_prints_the_shipped_list,
    ),
    (
        "a_pose_sends_a_different_message_from_the_emote_command",
        &["chat.pose.what-a-pose-sends-is-a-different-message-from-what-the-command-sends"],
        a_pose_sends_a_different_message_from_the_emote_command,
    ),
    (
        "the_possessive_word_is_chosen_by_sex",
        &["chat.pose.the-possessive-word-is-chosen-by-sex-before-the-message-leaves"],
        the_possessive_word_is_chosen_by_sex,
    ),
    (
        "the_players_own_echo_is_never_turned_into_a_noise",
        &["chat.pose.the-players-own-echo-is-never-turned-into-a-noise"],
        the_players_own_echo_is_never_turned_into_a_noise,
    ),
    (
        "every_pose_the_list_advertises_can_be_performed",
        &["chat.pose.every-pose-the-list-advertises-can-really-be-performed"],
        every_pose_the_list_advertises_can_be_performed,
    ),
    (
        "clicking_a_name_in_the_log_starts_a_tell_to_him",
        &["chat.tell.clicking-a-name-in-the-log-starts-a-private-message-to-him"],
        clicking_a_name_in_the_log_starts_a_tell_to_him,
    ),
    (
        "the_tell_key_names_the_selected_player",
        &["chat.tell.the-key-for-a-private-message-to-the-selected-player-names-him"],
        the_tell_key_names_the_selected_player,
    ),
    (
        "the_selected_players_name_fills_both_changing_rows",
        &["chat.talk-to-menu.the-selected-players-name-fills-both-of-the-changing-rows"],
        the_selected_players_name_fills_both_changing_rows,
    ),
    (
        "the_tell_key_does_nothing_without_a_player_selected",
        &["chat.tell.the-key-does-nothing-when-nobody-a-player-could-talk-to-is-selected"],
        the_tell_key_does_nothing_without_a_player_selected,
    ),
    (
        "picking_a_friend_and_pressing_tell_fills_the_entry",
        &["chat.tell.picking-a-friend-and-pressing-tell-fills-the-entry-and-replaces-what-was-there"],
        picking_a_friend_and_pressing_tell_fills_the_entry,
    ),
    (
        "only_the_main_window_takes_the_tell",
        &["chat.tell.only-the-main-window-takes-the-tell"],
        only_the_main_window_takes_the_tell,
    ),
    (
        "two_recorded_channels_draw_in_two_different_colours",
        &["chat.log.two-recorded-kinds-of-line-draw-in-two-different-colours"],
        two_recorded_channels_draw_in_two_different_colours,
    ),
    (
        "your_own_echo_and_a_remote_speaker_are_drawn_apart",
        &["chat.speech.your-own-echo-and-a-remote-speaker-are-drawn-from-two-different-forms"],
        your_own_echo_and_a_remote_speaker_are_drawn_apart,
    ),
    (
        "the_line_the_return_key_sends_is_remembered",
        &["chat.entry.the-line-the-return-key-sends-is-remembered-and-raised-for-its-own-window"],
        the_line_the_return_key_sends_is_remembered,
    ),
    (
        "the_scroll_keys_move_the_log_and_the_history_keys_fill_the_entry",
        &["chat.window.the-keys-that-move-the-log-and-the-keys-that-walk-the-history-are-different"],
        the_scroll_keys_move_the_log_and_the_history_keys_fill_the_entry,
    ),
    (
        "what_a_typed_line_becomes_follows_the_menu",
        &["chat.talk-focus.what-a-typed-line-becomes-follows-the-menu"],
        what_a_typed_line_becomes_follows_the_menu,
    ),
    (
        "a_command_the_client_handles_is_never_spoken",
        &["chat.commands.a-command-the-client-handles-is-never-spoken-to-the-shard"],
        a_command_the_client_handles_is_never_spoken,
    ),
    (
        "the_button_caption_follows_the_menu",
        &["chat.talk-to-menu.the-button-caption-follows-the-menu-and-a-row-that-is-shut-refuses"],
        the_button_caption_follows_the_menu,
    ),
    (
        "a_rows_place_in_the_list_is_not_the_order_of_the_channels",
        &["chat.talk-to-menu.which-channel-a-row-is-comes-from-the-row-and-not-its-place"],
        a_rows_place_in_the_list_is_not_the_order_of_the_channels,
    ),
    (
        "the_reply_keys_address_three_different_people",
        &["chat.tell.the-three-reply-keys-address-three-different-people"],
        the_reply_keys_address_three_different_people,
    ),
    (
        "a_floaty_window_writes_its_place_and_title_back",
        &["chat.window.a-small-window-writes-its-place-size-openness-and-title-back"],
        a_floaty_window_writes_its_place_and_title_back,
    ),
    (
        "the_close_button_hides_its_own_window",
        &["chat.window.the-close-button-shuts-its-own-window-and-writes-that-down"],
        the_close_button_hides_its_own_window,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

// -------------------------------------------------------------------------------------------
// chat.talk-focus.picking-a-target-redraws-every-row-from-the-settings
// -------------------------------------------------------------------------------------------

/// The rows of the shipped talk-to menu, by the number the chat system knows each one as.
const SAY_ROW: u32 = 1;
const SELECTED_ROW: u32 = 2;
const FELLOWSHIP_ROW: u32 = 3;
const GENERAL_ROW: u32 = 8;
const OLTHOI_ROW: u32 = 13;
/// What a row's element carries when it may be picked, and when it may not.
const PICKABLE: dereth_ui::StateId = dereth_ui::StateId(1);
const GREYED: dereth_ui::StateId = dereth_ui::StateId(0xD);

/// Run `f` against the shipped gameplay screen and the shell it was laid out in.
///
/// The chat state is lifted out of the world for the call and put straight back, because the
/// client's own delivery takes the screen, the shell and the chat state at once and a scenario
/// reaching the live client can only borrow one of them at a time. Nothing else sees it move.
fn on_the_shipped_menu<T>(
    c: &mut HeadlessClient,
    f: impl FnOnce(
        &mut dereth_ui::UiSystem,
        &mut GamePlayScreen,
        &mut dereth_client_model::chat::ChatState,
    ) -> T,
) -> T {
    let mut chat = std::mem::take(&mut c.world_mut().chat);
    let out = {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the UI shell is up");
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");
        f(&mut shell.ui, gameplay, &mut chat)
    };
    c.world_mut().chat = chat;
    out
}

/// The menu's rows follow what the chat system said, and picking a target redraws them all from
/// the player's own settings.
pub fn the_talk_to_menu_is_redrawn_from_the_settings() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // Whatever bringing the screen up said about the rows is spent; what is measured below is
    // what this scenario says about them.
    let _ = c.world_mut().chat.take_talk_focus_notices();

    // An Olthoi character, with the two rows that are not channels switched on.
    {
        let w = c.world_mut();
        w.chat.using_turbine_chat = true;
        w.enable_chat_talk_focuses(true);
        w.chat.set_talk_focus_enabled(TalkFocus::Selected, true);
        w.chat.set_talk_focus_enabled(TalkFocus::Fellowship, true);
    }
    let notices = c.world_mut().chat.take_talk_focus_notices();
    let drawn = on_the_shipped_menu(&mut c, |ui, screen, chat| {
        dereth_client_runtime::ui_context::offer_talk_focus_notices(
            chat,
            notices,
            &mut |focus, notice| dereth_client::hud::talk_focus_notice(ui, screen, focus, notice),
        );
        let state = |row: u32| {
            screen
                .main_chat
                .menu_item(ui, row)
                .map(|h| ui.node(h).expect("the row").state)
        };
        (
            state(SELECTED_ROW),
            state(FELLOWSHIP_ROW),
            state(GENERAL_ROW),
            state(OLTHOI_ROW),
            // The menu's own readback of the setting behind the row it just greyed.
            screen.main_chat.is_talk_focus_enabled(FELLOWSHIP_ROW),
        )
    });
    let rows_follow_the_answers = drawn
        == (
            Some(PICKABLE),
            Some(GREYED),
            Some(GREYED),
            Some(PICKABLE),
            true,
        );

    // **Picking a target.** Every row is redrawn from the player's stored settings, so the row the
    // answers above had opened closes again -- while the setting behind it is untouched.
    let after_picking = on_the_shipped_menu(&mut c, |ui, screen, _| {
        let say = screen
            .main_chat
            .menu_item(ui, SAY_ROW)
            .expect("the say row");
        screen.chat_target_menu_item(ui, say);
        screen
            .main_chat
            .menu_item(ui, SELECTED_ROW)
            .map(|h| ui.node(h).expect("the row").state)
    });
    let setting_untouched = c.view().world().chat.enabled_focuses()[SELECTED_ROW as usize];

    // Removing a projected row does not remove the shared channel fallback.
    let removed = on_the_shipped_menu(&mut c, |ui, screen, _| {
        let general = screen
            .main_chat
            .menu_item(ui, GENERAL_ROW)
            .expect("the general row");
        ui.remove_and_delete_root(general);
        screen.main_chat.menu_item(ui, GENERAL_ROW).is_none()
    });
    {
        let w = c.world_mut();
        w.chat.set_talk_focus(TalkFocus::General);
        w.enable_chat_talk_focuses(false);
        w.chat.set_talk_focus_enabled(TalkFocus::General, false);
    }
    let notices = c.world_mut().chat.take_talk_focus_notices();
    let rebuilt = on_the_shipped_menu(&mut c, |ui, screen, chat| {
        dereth_client_runtime::ui_context::offer_talk_focus_notices(
            chat,
            notices,
            &mut |focus, notice| dereth_client::hud::talk_focus_notice(ui, screen, focus, notice),
        );
        let kept = chat.talk_focus == TalkFocus::All;
        // The menu builds itself again and lands on the row it does have.
        let landed = screen.main_chat.init_talk_focus_menu(ui);
        let say = screen
            .main_chat
            .menu_item(ui, SAY_ROW)
            .expect("the say row");
        screen.chat_target_menu_item(ui, say);
        let general = screen
            .main_chat
            .menu_item(ui, GENERAL_ROW)
            .map(|h| ui.node(h).expect("the row").state);
        (kept, landed, general)
    });

    c.assert_behaviour(
        "chat.talk-focus.picking-a-target-redraws-every-row-from-the-settings",
        move |_| {
            rows_follow_the_answers
            && after_picking == Some(GREYED)
            && setting_untouched
            && removed
            && rebuilt.1 == OLTHOI_ROW as usize
            // Rebuilt from the setting the chat system holds, which that last batch set false.
            && rebuilt.2 == Some(GREYED)
        },
    );
    c.assert_behaviour(
        "chat.talk-focus.channel-fallback-does-not-require-a-window",
        move |_| rebuilt.0,
    );
    c.shutdown();
}

#[test]
fn scenario_the_talk_to_menu_is_redrawn_from_the_settings() {
    scenario("the_talk_to_menu_is_redrawn_from_the_settings");
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

/// The chat window's talk-to button. The row of its menu that is the general channel is
/// `GENERAL_ROW` above.
const TALK_TO_BUTTON: ElementId = ElementId(0x1000_0014);

/// The recording whose login this section takes its character set and room list from.
const ROOMS_SESSION: &str = "short-second-connection";

/// The player this section's client is.
const ME: ObjectId = ObjectId(0x5000_0017);

/// The body of the first server-to-client blob of `ROOMS_SESSION` carrying `opcode`, starting at
/// the opcode dword.
///
/// The blobs are found by walking the recording rather than by index, so a re-locked corpus moves
/// nothing here.
fn recorded(opcode: u32) -> Vec<u8> {
    let corpus = Corpus::load(ROOMS_SESSION)
        .expect("the corpus decodes")
        .expect("the corpus holds the recording");
    for b in &corpus.blobs {
        if b.dir != Direction::ServerToClient {
            continue;
        }
        if b.opcode == opcode {
            return b.payload.clone();
        }
        // A game event carries its own header before the message it wraps.
        if b.opcode == 0xF7B0 && b.payload.len() >= 16 {
            let sub = u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes"));
            if sub == opcode {
                return b.payload[12..].to_vec();
            }
        }
    }
    panic!("{ROOMS_SESSION} carries no server-to-client {opcode:#x}");
}

/// A client with the shell up and no screen settled yet.
fn a_bare_client(shell: bool) -> HeadlessClient {
    HeadlessClient::new(ClientSpec {
        shell,
        ui_mode: None,
        settle_frames: 0,
        ..ClientSpec::retail()
    })
}

/// Hand the client the character set the recording's login carried, with the chat-room permission
/// as given -- the negative is a labelled mutation of the shard's own answer.
fn character_set(c: &mut HeadlessClient, permitted: bool) {
    let mut set: LoginCharacterSet = dereth_protocol::read_body(&recorded(0xF658)[4..])
        .expect("the recorded character set decodes");
    assert_ne!(
        set.use_turbine_chat, 0,
        "the recording's own answer permits it"
    );
    if !permitted {
        set.use_turbine_chat = 0;
    }
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::CharacterSet(Box::new(set))]);
}

/// Hand the client a description of its own player with the two listening options as given.
fn describe(c: &mut HeadlessClient, general: bool, allegiance: bool) {
    let mut module = dereth_protocol::login::PlayerModule::default();
    if general {
        module.options2 |= 0x100;
    } else {
        module.options2 &= !0x100;
    }
    if allegiance {
        module.options |= 0x4000_0000;
    } else {
        module.options &= !0x4000_0000;
    }
    c.when(Inbound::event(SessionEvent::PlayerDescription(Box::new(
        dereth_protocol::login::LoginPlayerDescription {
            player_module: module,
            ..Default::default()
        },
    ))));
}

/// Hand the client the room list the recording's login carried.
fn room_list(c: &mut HeadlessClient) -> dereth_protocol::comms::ChatRoomMembership {
    use dereth_protocol::Message as _;
    let blob = recorded(0x0295);
    let tracker = dereth_protocol::comms::ChatRoomMembership::read(
        &mut dereth_protocol::archive::Reader::body(&blob[4..]),
    )
    .expect("the recorded room list decodes");
    c.when(Inbound::event(SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode(0x0295),
        blob,
    }));
    tracker
}

/// Put the gameplay screen up and settle it.
fn to_gameplay(c: &mut HeadlessClient) {
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(1);
}

/// One element of the shipped gameplay tree, by id.
fn element(c: &HeadlessClient, id: ElementId) -> ElemHandle {
    let app = c.view().expect_app();
    let shell = app.ui().expect("the UI shell is up");
    let any: &dyn std::any::Any = shell.flow.current().expect("a screen is up");
    let root = any
        .downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen")
        .root()
        .expect("the gameplay screen's root");
    shell
        .ui
        .get_child_recursive(root, id)
        .expect("the element is in the shipped layout")
}

/// Everything the live chat log holds, as text.
fn log_text(c: &mut HeadlessClient) -> String {
    let h = element(c, dereth_ui_screens::chat::window::LOG);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// Pick the general channel out of the talk-to menu, as a player does.
fn pick_general(c: &mut HeadlessClient, hand: &mut Hand) {
    hand.click_element(c, TALK_TO_BUTTON);
    let row = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the shell is up");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen is up");
        let screen = any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen");
        let row = screen
            .main_chat
            .menu_item(&shell.ui, GENERAL_ROW)
            .expect("the general row");
        assert_ne!(
            shell.ui.node(row).expect("the row is alive").state,
            StateId(0xD),
            "the general row must be pickable, or this scenario measures nothing"
        );
        row
    };
    hand.click_handle(c, row);
    assert_eq!(
        c.view().world().chat.talk_focus,
        dereth_client_model::chat::TalkFocus::General,
        "picking the row must move the talk focus"
    );
}

/// Every chat-room message the client has produced, in order.
fn room_messages(c: &HeadlessClient) -> Vec<dereth_protocol::turbine::SendToRoomById> {
    c.view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::TurbineChat(m) => Some(m.clone()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// chat.turbine.the-general-channel-carries-a-typed-line-to-its-room
// ---------------------------------------------------------------------------------------------

/// The shard's permission starts the service, the player picks the general channel, types a line,
/// and one chat-room message goes out -- with the room, the sender and the kind of line on it, and
/// on the queue that carries no ordering stamp.
pub fn the_general_channel_carries_a_typed_line_to_its_room() {
    let mut c = a_bare_client(true);
    // The shard's answer is the gate: with the permission withheld the service does not start.
    character_set(&mut c, false);
    let refused = !c.view().world().chat.using_turbine_chat;
    character_set(&mut c, true);
    let started = c.view().world().chat.using_turbine_chat;

    c.world_mut().player = Some(ME);
    describe(&mut c, true, true);
    let tracker = room_list(&mut c);
    assert_ne!(
        tracker.general_room, 0,
        "the recording names a general room"
    );
    to_gameplay(&mut c);

    let mut hand = Hand::new();
    pick_general(&mut c, &mut hand);
    let undeliverable = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .requests_undeliverable;
    hand.say(&mut c, "General oracle");

    let sent = room_messages(&c);
    let one_message = sent.len() == 1
        && (
            sent[0].context,
            sent[0].room,
            sent[0].sender,
            sent[0].chat_type,
        ) == (1, tracker.general_room, ME.0, 1)
        && sent[0].text == "General oracle";
    // And it is a room line, not an ordinary say.
    let not_a_say = !c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .any(|r| matches!(r, dereth_client_model::Request::Talk(_)));
    // A client with no link records the line as undeliverable rather than as sent.
    let recorded_as_undeliverable = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .requests_undeliverable
        == undeliverable + 1;

    // The bytes, through the production sender over a mock transport. **Nothing is sent.**
    let mut session = Session::new(MockTransport::new());
    let put_on_the_wire = dereth_client_runtime::requests::send_request(
        &mut session,
        &dereth_client_model::Request::TurbineChat(sent[0].clone()),
    );
    let wire = &session.transport.sent[0];
    let framed = (wire.queue, wire.ordered) == (NetQueue::Logon, false)
        && wire.payload[..4] == 0xF7DE_u32.to_le_bytes()
        && wire.payload[8..] == *sent[0].network_packet().expect("the packet composes");

    // The service belongs to the client and not to the screen: rebuild the screen, restart the
    // service, and the next line carries on from where the last one left off.
    let old_root = element(&c, dereth_ui_screens::chat::window::LOG);
    to_gameplay(&mut c);
    let screen_is_new = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell is up")
        .ui
        .node(old_root)
        .is_none();
    character_set(&mut c, false);
    character_set(&mut c, true);
    // How fast the player may talk is measured against the wall clock, so a real gap is the only
    // way to drain it -- an injected one would be asserting over a bucket the client does not use.
    std::thread::sleep(std::time::Duration::from_secs(2));
    hand.say(&mut c, "@general explicit line");
    let sent = room_messages(&c);
    let explicit = sent.len() == 1
        && (sent[0].context, sent[0].room, sent[0].chat_type) == (2, tracker.general_room, 2)
        && sent[0].text == "explicit line";

    c.assert_behaviour(
        "chat.turbine.the-general-channel-carries-a-typed-line-to-its-room",
        move |_| {
            refused
                && started
                && one_message
                && not_a_say
                && recorded_as_undeliverable
                && put_on_the_wire
                && framed
                && screen_is_new
                && explicit
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_general_channel_carries_a_typed_line_to_its_room() {
    scenario("the_general_channel_carries_a_typed_line_to_its_room");
}

/// A line typed before the shard has named a room is refused, and the next one is not.
pub fn a_line_typed_before_the_shard_names_a_room_is_refused() {
    let mut c = a_bare_client(true);
    character_set(&mut c, true);
    describe(&mut c, true, true);
    to_gameplay(&mut c);

    let mut hand = Hand::new();
    pick_general(&mut c, &mut hand);
    hand.say(&mut c, "no room yet");
    let refused = !c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .any(|r| {
            matches!(
                r,
                dereth_client_model::Request::TurbineChat(_)
                    | dereth_client_model::Request::Talk(_)
            )
        });
    // And the player is left on the channel he picked rather than being moved off it.
    let still_general =
        c.view().world().chat.talk_focus == dereth_client_model::chat::TalkFocus::General;

    room_list(&mut c);
    hand.say(&mut c, "room received");
    let sent = room_messages(&c);
    let now_sent = sent.len() == 1 && sent[0].context == 1 && sent[0].chat_type == 1;

    c.assert_behaviour(
        "chat.turbine.a-line-typed-before-the-shard-names-a-room-is-refused",
        move |_| refused && still_general && now_sent,
    );
    c.shutdown();
}

#[test]
fn scenario_a_line_typed_before_the_shard_names_a_room_is_refused() {
    scenario("a_line_typed_before_the_shard_names_a_room_is_refused");
}

/// The service is the client's and not the interface's: it runs with no interface at all, and a
/// restart does not put it back to the beginning.
pub fn the_room_service_runs_with_no_interface_at_all() {
    let mut c = a_bare_client(false);
    character_set(&mut c, false);
    let refused = !c.view().world().chat.using_turbine_chat;

    let set: LoginCharacterSet = dereth_protocol::read_body(&recorded(0xF658)[4..])
        .expect("the recorded character set decodes");
    let mut desc = dereth_protocol::login::LoginPlayerDescription::default();
    desc.player_module.options2 |= 0x100;
    c.when(Inbound::event(SessionEvent::CharacterSet(Box::new(set))));
    c.when(Inbound::event(SessionEvent::PlayerDescription(Box::new(
        desc,
    ))));
    c.when(Inbound::event(SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode(0x0295),
        blob: recorded(0x0295),
    }));
    let offered = c
        .view()
        .world()
        .chat
        .is_talk_focus_enabled(dereth_client_model::chat::TalkFocus::General);

    let mut req = dereth_client_model::RecordingRequests::default();
    let first = c.world_mut().send_turbine_chat(
        &mut req,
        dereth_client_model::chat::TalkFocus::General,
        true,
        "no interface first",
        4,
        100,
    );
    c.tick(1);
    character_set(&mut c, false);
    character_set(&mut c, true);
    c.tick(1);
    let no_shell = c.view().expect_app().ui().is_none();
    let second = c.world_mut().send_turbine_chat(
        &mut req,
        dereth_client_model::chat::TalkFocus::General,
        true,
        "no interface next",
        4,
        102,
    );
    let dereth_client_model::Request::TurbineChat(m) = &req.0[1] else {
        panic!("the second line is a room message")
    };
    // The count carries on rather than starting again, and the room is still the one the shard
    // named before the restart.
    let carried_on = (m.context, m.room) == (2, 2);

    c.assert_behaviour(
        "chat.turbine.the-service-runs-with-no-interface-at-all",
        move |_| refused && offered && first && no_shell && second && carried_on,
    );
    c.shutdown();
}

#[test]
fn scenario_the_room_service_runs_with_no_interface_at_all() {
    scenario("the_room_service_runs_with_no_interface_at_all");
}

// ---------------------------------------------------------------------------------------------
// chat.turbine.the-hosts-own-spelling-is-what-goes-on-the-wire
// ---------------------------------------------------------------------------------------------

/// Type `text` into the chat entry through the wide-character boundary and press return.
///
/// The character-message gesture [`Hand::type_text`] makes is a byte one and is deliberately
/// ASCII, so a line that is about the host's own spelling of a word enters at the wide boundary
/// instead. It still goes through the real editing, the real history and the real Enter, and it
/// claims nothing about a keyboard or an input method.
fn wide_entry(c: &mut HeadlessClient, hand: &mut Hand, text: &str) {
    hand.click_element(c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        for unit in text.encode_utf16() {
            ui.character(unit);
        }
    }
    c.tick(1);
    hand.press_return(c);
}

/// One journey of `input` through a host that spells its text in `code_page`.
fn conversion_journey(code_page: u32, input: &str, native: &[u8], room_text: &str) -> bool {
    let mut c = a_bare_client(true);
    character_set(&mut c, true);
    describe(&mut c, true, true);
    // **The host is constructed, not the machine's.** Reading the machine's own code page and
    // measuring against it would make the claim depend on the locale of whatever ran it. Naming
    // the code page here is the same claim without that.
    c.world_mut().chat.text_conversion = dereth_client_model::HostText::new(std::sync::Arc::new(
        dereth_client::platform::text::HostAcp::for_ansi_code_page(code_page),
    ));
    room_list(&mut c);
    to_gameplay(&mut c);

    let mut hand = Hand::new();
    wide_entry(&mut c, &mut hand, input);
    let ordinary = c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .find(|r| matches!(r, dereth_client_model::Request::Talk(_)))
        .expect("an ordinary line has a producer")
        .clone();
    let dereth_client_model::Request::Talk(talk) = &ordinary else {
        unreachable!("matched above")
    };
    let spelled =
        dereth_protocol::cp1252::encode(&talk.message).expect("the line spells") == native;

    let mut session = Session::new(MockTransport::new());
    assert!(dereth_client_runtime::requests::send_request(
        &mut session,
        &ordinary
    ));
    let wire = &session.transport.sent[0];
    // The framing, assembled here rather than by the writer being asserted over.
    let mut want: Vec<u8> = [0xF7B1_u32, 1, 0x15]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    want.extend(u16::try_from(native.len()).expect("short").to_le_bytes());
    want.extend(native);
    while want.len() % 4 != 0 {
        want.push(0);
    }
    let framed = wire.payload == want && (wire.queue, wire.ordered) == (NetQueue::Weenie, true);

    // What the player typed is what his own history holds, whatever the host could spell.
    let kept = c
        .view()
        .world()
        .chat
        .entries
        .get(&8)
        .and_then(|entry| entry.history().last())
        .map(String::as_str)
        == Some(input);

    // The same line on the general channel: the same host spells it, and the room message carries
    // that spelling rather than the ordinary one's bytes.
    pick_general(&mut c, &mut hand);
    wide_entry(&mut c, &mut hand, input);
    let room = c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .find(|r| matches!(r, dereth_client_model::Request::TurbineChat(_)))
        .expect("the general channel has a producer")
        .clone();
    let dereth_client_model::Request::TurbineChat(message) = &room else {
        unreachable!("matched above")
    };
    let same_spelling = message.text == room_text;
    assert!(dereth_client_runtime::requests::send_request(
        &mut session,
        &room
    ));
    let wire = &session.transport.sent[1];
    let decoded: dereth_protocol::turbine::SendToRoomById =
        dereth_protocol::read_body(&wire.payload[4..]).expect("the room message decodes");
    let on_the_other_queue =
        (wire.queue, wire.ordered) == (NetQueue::Logon, false) && decoded.text == room_text;
    // And it is a room line and not also an ordinary one.
    let not_also_a_say = !c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .any(|r| matches!(r, dereth_client_model::Request::Talk(_)));

    c.shutdown();
    spelled && framed && kept && same_spelling && on_the_other_queue && not_also_a_say
}

/// What the host can spell is what goes on the wire, for an ordinary line and for a room line
/// alike -- and a word it cannot spell is written out in the client's own escape rather than lost.
pub fn the_hosts_own_spelling_is_what_goes_on_the_wire() {
    // A host that spells the western alphabet: the accented letter it has and the ideograph it has
    // not are both written out in the escape, because one unspellable unit escapes the whole word.
    let western = conversion_journey(
        1252,
        "cafe\u{e9}\u{4e00}",
        b"cafe<00e9><4e00>",
        "cafe<00e9><4e00>",
    );
    // A host that spells the ideographs: the same three characters keep their own bytes going out
    // and come back as themselves on the room message.
    let eastern = conversion_journey(
        932,
        "\u{65e5}\u{672c}\u{8a9e}",
        &[0x93, 0xfa, 0x96, 0x7b, 0x8c, 0xea],
        "\u{65e5}\u{672c}\u{8a9e}",
    );

    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.assert_behaviour(
        "chat.turbine.the-hosts-own-spelling-is-what-goes-on-the-wire",
        move |_| western && eastern,
    );
    c.shutdown();
}

#[test]
fn scenario_the_hosts_own_spelling_is_what_goes_on_the_wire() {
    scenario("the_hosts_own_spelling_is_what_goes_on_the_wire");
}

// ---------------------------------------------------------------------------------------------
// The incoming half: a packet on the chat-room queue, and what the log does with it
// ---------------------------------------------------------------------------------------------

/// The room every packet below is addressed to.
const GENERAL_ROOM: u32 = 0x1234_5678;

fn word(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}

fn wide(out: &mut Vec<u8>, value: &str) {
    let units: Vec<u16> = value.encode_utf16().collect();
    assert!(units.len() < 128, "the length prefix is one byte");
    out.push(u8::try_from(units.len()).expect("checked above"));
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
}

/// The envelope a chat-room packet arrives in.
///
/// `over_long` adds the one fixed overshoot the reference server sends, which the client accepts
/// and counts rather than refusing.
fn envelope(kind: u32, body: Vec<u8>, over_long: bool) -> Vec<u8> {
    let extra = u32::from(over_long) * 8;
    let mut raw = Vec::new();
    word(
        &mut raw,
        u32::try_from(body.len()).expect("small") + 32 + extra,
    );
    for v in [kind, 1, 1, 0xB00B5, 1, 0xB00B5, 0] {
        word(&mut raw, v);
    }
    word(&mut raw, u32::try_from(body.len()).expect("small") + extra);
    raw.extend(body);
    raw
}

/// One line spoken in a room.
fn room_event(room: u32, name: &str, text: &str, over_long: bool) -> Vec<u8> {
    let mut body = Vec::new();
    word(&mut body, room);
    wide(&mut body, name);
    wide(&mut body, text);
    // The metadata is deliberately not the general room's: what kind of line it is drawn as comes
    // from the **room**, not from this.
    for v in [12, 0x5000_0017, 0x8007_0005, 10] {
        word(&mut body, v);
    }
    envelope(1, body, over_long)
}

/// The service's answer about a line the player sent.
fn room_answer(context: u32, result: u32) -> Vec<u8> {
    envelope(
        5,
        [context, 2, 2, result]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
        true,
    )
}

/// A client with the chat-room service up, a general room, and the gameplay screen settled.
fn a_client_in_a_room(shell: bool) -> HeadlessClient {
    let mut c = a_bare_client(shell);
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::CharacterSet(Box::new(
            LoginCharacterSet {
                use_turbine_chat: 1,
                ..LoginCharacterSet::default()
            },
        ))]);
    c.when(Inbound::event(SessionEvent::PlayerDescription(
        Box::default(),
    )));
    let rooms = dereth_protocol::comms::ChatRoomMembership {
        general_room: GENERAL_ROOM,
        ..dereth_protocol::comms::ChatRoomMembership::default()
    };
    c.when(Inbound::event(SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode(0x0295),
        blob: dereth_protocol::write_blob(&rooms).expect("the room list encodes"),
    }));
    to_gameplay(&mut c);
    c
}

/// Put `raw` on the chat-room queue through a real session, and deliver whatever it decides that
/// is. **No socket**: the transport is a mock that is handed the bytes directly.
fn deliver_room_packet(c: &mut HeadlessClient, session: &mut Session<MockTransport>, raw: Vec<u8>) {
    session.transport.deliver(IncomingMessage {
        opcode: 0xF7DE,
        body: raw.clone(),
        queue: NetQueue::Logon,
        sender: RecipientId(0),
        blob_id: NetBlobId(1),
    });
    session.tick(LocalTime(0.0));
    let events: Vec<SessionEvent> = session.drain_events().collect();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SessionEvent::TurbineChat(b) if b == &raw)),
        "a packet on that queue is a chat-room line"
    );
    for e in events {
        c.when(Inbound::event(e));
    }
}

/// Send a line to the general room through the client's own guarded producer and return the number
/// the answer will name it by.
fn a_line_in_flight(c: &mut HeadlessClient, text: &str, now: i32) -> u32 {
    c.world_mut().player_system.options.set(27, true);
    let mut out = dereth_client_model::RecordingRequests::default();
    assert!(c.world_mut().send_turbine_chat(
        &mut out,
        dereth_client_model::chat::TalkFocus::General,
        false,
        text,
        4,
        now,
    ));
    let Some(dereth_client_model::Request::TurbineChat(request)) = out.0.pop() else {
        panic!("the producer makes a room message")
    };
    request.context
}

/// A line spoken in a room is drawn under that room's own name, in that room's own colour, with
/// the speaker as something the player can click -- once, and not again on the next frame.
pub fn a_room_line_reaches_the_log_with_its_rooms_name_and_colour() {
    let mut c = a_client_in_a_room(true);
    let mut session = Session::new(MockTransport::new());

    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "Sender", "a line", false),
    );
    c.tick(1);
    let shown = log_text(&mut c);
    let drawn = shown.contains("[General] Sender says, \"a line\"");

    let (coloured, tagged) = {
        let h = element(&c, dereth_ui_screens::chat::window::LOG);
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let glyphs = &ui
            .text_element_mut(h)
            .expect("the log is a text element")
            .glyphs
            .glyphs;
        (
            glyphs.iter().any(|g| g.color == 0xFFB4_DCF0),
            glyphs.iter().any(|g| g.tag.is_some()),
        )
    };

    c.tick(1);
    let not_replayed = log_text(&mut c) == shown;

    // The same line in the one over-long shape the reference server sends: accepted, drawn, and
    // counted as the overshoot it is rather than refused.
    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "Next", "over long", true),
    );
    c.tick(1);
    let over_long = log_text(&mut c).contains("[General] Next says, \"over long\"")
        && c.view().world().chat.turbine_extent_discrepancies() == 1;

    c.assert_behaviour(
        "chat.turbine.a-room-line-reaches-the-log-with-its-rooms-own-name-and-colour",
        move |_| drawn && coloured && tagged && not_replayed && over_long,
    );
    c.shutdown();
}

#[test]
fn scenario_a_room_line_reaches_the_log_with_its_rooms_name_and_colour() {
    scenario("a_room_line_reaches_the_log_with_its_rooms_name_and_colour");
}

/// An answer about a line sent from a screen that has since been rebuilt completes the line
/// without reviving the log it was typed into, and the next one is drawn in the new log.
pub fn an_answer_that_arrives_after_the_screen_has_gone() {
    let mut c = a_client_in_a_room(true);
    let mut session = Session::new(MockTransport::new());

    let first = a_line_in_flight(&mut c, "old generation", 100);
    let old_log = element(&c, dereth_ui_screens::chat::window::LOG);
    deliver_room_packet(&mut c, &mut session, room_answer(first, 0x8000_4005));
    // The line is finished with before anything is drawn.
    let completed = c.view().world().chat.pending_turbine_contexts().is_empty();
    // The screen that was waiting for it is rebuilt before the answer would have been drawn.
    to_gameplay(&mut c);
    let old_is_gone = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell is up")
        .ui
        .node(old_log)
        .is_none()
        && !log_text(&mut c).contains("old generation");

    let second = a_line_in_flight(&mut c, "fresh generation", 102);
    let numbered = second == first + 1;
    deliver_room_packet(&mut c, &mut session, room_answer(second, 1));
    c.tick(1);
    let shown = log_text(&mut c);
    let told = shown.contains("Failed to send text: [fresh generation] to room 12345678.");
    // A second answer about the same line says nothing more.
    deliver_room_packet(&mut c, &mut session, room_answer(second, 1));
    c.tick(1);
    let quiet_the_second_time = log_text(&mut c) == shown;

    // The same boundary for a line somebody else spoke, not only for an answer.
    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "Old", "queued", false),
    );
    to_gameplay(&mut c);
    let not_revived = !log_text(&mut c).contains("queued");
    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "Fresh", "new text", false),
    );
    c.tick(1);
    let new_is_drawn = log_text(&mut c).contains("[General] Fresh says, \"new text\"");

    c.assert_behaviour(
        "chat.turbine.an-answer-that-arrives-after-the-screen-has-gone-completes-quietly",
        move |_| {
            completed
                && old_is_gone
                && numbered
                && told
                && quiet_the_second_time
                && not_revived
                && new_is_drawn
        },
    );
    c.shutdown();
}

#[test]
fn scenario_an_answer_that_arrives_after_the_screen_has_gone() {
    scenario("an_answer_that_arrives_after_the_screen_has_gone");
}

/// A line still in flight when the character leaves is finished with, and does not follow him back
/// into the next character's window.
pub fn a_line_in_flight_does_not_follow_the_character_out() {
    // First, with no interface at all: the line is still finished with.
    let mut headless = a_client_in_a_room(false);
    let mut session = Session::new(MockTransport::new());
    let context = a_line_in_flight(&mut headless, "no interface", 100);
    deliver_room_packet(&mut headless, &mut session, room_answer(context, 1));
    let finished_without_a_screen = headless
        .view()
        .world()
        .chat
        .pending_turbine_contexts()
        .is_empty();
    headless.tick(1);
    let had_none = headless.view().expect_app().ui().is_none();
    headless.shutdown();

    let mut c = a_client_in_a_room(true);
    c.world_mut().player = Some(ME);
    let old = a_line_in_flight(&mut c, "old character", 100);
    c.when(Inbound::event(SessionEvent::LoggedOff));
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::CHARACTER_MANAGEMENT);
    c.tick(1);
    // The character has gone and the line has not: it is still waiting to be answered.
    let still_waiting = c.view().world().player.is_none()
        && c.view().world().chat.pending_turbine_contexts() == [old];
    deliver_room_packet(&mut c, &mut session, room_answer(old, 1));
    let answered = c.view().world().chat.pending_turbine_contexts().is_empty();
    c.tick(1);

    // A new character, and the old line does not appear in his window.
    c.world_mut().player = Some(ObjectId(0x5000_0099));
    c.when(Inbound::event(SessionEvent::PlayerDescription(
        Box::default(),
    )));
    to_gameplay(&mut c);
    let not_carried_over = !log_text(&mut c).contains("old character");

    let new = a_line_in_flight(&mut c, "new character", 102);
    let numbered = new == old + 1;
    deliver_room_packet(&mut c, &mut session, room_answer(new, 1));
    c.tick(1);
    let his_own_is_drawn =
        log_text(&mut c).contains("Failed to send text: [new character] to room 12345678.");

    c.assert_behaviour(
        "chat.turbine.a-line-in-flight-does-not-follow-the-character-out",
        move |_| {
            finished_without_a_screen
                && had_none
                && still_waiting
                && answered
                && not_carried_over
                && numbered
                && his_own_is_drawn
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_line_in_flight_does_not_follow_the_character_out() {
    scenario("a_line_in_flight_does_not_follow_the_character_out");
}

/// The room's lines and the client's own notices are one queue, in arrival order -- and a packet
/// that came in on the wrong queue is not a chat-room line at all.
pub fn room_lines_and_ordinary_notices_share_one_ordered_queue() {
    let mut c = a_client_in_a_room(true);
    let mut session = Session::new(MockTransport::new());

    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "First", "one", false),
    );
    c.world_mut()
        .scroll
        .add_text_to_scroll("middle ordinary notice", 0, true, 0);
    deliver_room_packet(
        &mut c,
        &mut session,
        room_event(GENERAL_ROOM, "Last", "three", false),
    );
    c.tick(1);
    let shown = log_text(&mut c);
    let ordered = {
        let first = shown.find("[General] First").expect("the first line");
        let middle = shown.find("middle ordinary notice").expect("the notice");
        let last = shown.find("[General] Last").expect("the last line");
        first < middle && middle < last
    };

    // The same bytes on the ordinary message queue: the session does not call it a chat-room line,
    // and nothing is drawn.
    session.transport.deliver(IncomingMessage {
        opcode: 0xF7DE,
        body: room_event(GENERAL_ROOM, "Wrong", "must not display", false),
        queue: NetQueue::UiQueue,
        sender: RecipientId(0),
        blob_id: NetBlobId(2),
    });
    session.tick(LocalTime(0.0));
    let events: Vec<SessionEvent> = session.drain_events().collect();
    let not_chat = !events
        .iter()
        .any(|e| matches!(e, SessionEvent::TurbineChat(_)));
    for e in events {
        c.when(Inbound::event(e));
    }
    c.tick(1);
    let nothing_drawn = log_text(&mut c) == shown;

    c.assert_behaviour(
        "chat.turbine.the-rooms-lines-and-the-ordinary-notices-share-one-queue",
        move |_| ordered && not_chat && nothing_drawn,
    );
    c.shutdown();
}

#[test]
fn scenario_room_lines_and_ordinary_notices_share_one_ordered_queue() {
    scenario("room_lines_and_ordinary_notices_share_one_ordered_queue");
}

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

#[test]
fn scenario_typing_an_emote_sends_it_and_leaves_the_next_line_free() {
    scenario("typing_an_emote_sends_it_and_leaves_the_next_line_free");
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

#[test]
fn scenario_a_typed_emote_reaches_the_link_on_the_next_frame() {
    scenario("a_typed_emote_reaches_the_link_on_the_next_frame");
}

// ---------------------------------------------------------------------------------------------
// The talk-to menu: picking a channel, losing it, and the popup itself
// ---------------------------------------------------------------------------------------------

/// The say row of the talk-to menu, and the row for a channel this character may never use.
const SAY_MENU_ROW: u32 = 1;
const OLTHOI_MENU_ROW: u32 = 13;
/// The chat window itself, its entry, and the selection area beside it.
const CHAT_WINDOW: ElementId = ElementId(0x1000_0601);
const SELECTION_FIELD: ElementId = ElementId(0x1000_019E);
const SELECTION_NAME: ElementId = ElementId(0x1000_019F);

/// What the talk-to row of the chat window says the player is talking to.
fn menu_talk_focus(c: &HeadlessClient) -> u32 {
    let app = c.view().expect_app();
    let any: &dyn std::any::Any = app
        .ui()
        .expect("the shell")
        .flow
        .current()
        .expect("a screen");
    any.downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen")
        .main_chat
        .talk_focus
}

/// The shipped popup the talk-to button opens.
fn talk_to_popup(c: &HeadlessClient) -> ElemHandle {
    let menu = element(c, TALK_TO_BUTTON);
    dereth_ui::widgets::menu::popup_handle(
        &c.view().expect_app().ui().expect("the shell is up").ui,
        menu,
    )
    .expect("the talk-to button carries a popup")
}

/// The drawn text of one element.
fn element_text(c: &mut HeadlessClient, h: ElemHandle) -> String {
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(true))
}

/// A client with the chat-room service on and the player described, in gameplay.
fn a_client_on_the_general_channel() -> HeadlessClient {
    let mut c = a_bare_client(true);
    to_gameplay(&mut c);
    c.world_mut().chat.using_turbine_chat = true;
    describe(&mut c, true, false);
    c.tick(1);
    c
}

/// A channel the player was talking on, switched off and straight back on again, drops him back to
/// saying it aloud -- and the line he sends next goes where the window now says it does.
pub fn a_channel_switched_off_and_on_drops_the_player_back_to_saying_it() {
    use dereth_client_model::chat::TalkFocus;

    let mut c = a_client_on_the_general_channel();
    let mut hand = Hand::new();
    pick_general(&mut c, &mut hand);

    // A line typed on the channel is not misrouted into an ordinary spoken one just because the
    // room is not there to take it.
    let lines = c.view().expect_app().interaction().stats.chat_lines_sent;
    hand.say(&mut c, "general intent");
    let routed = c.view().expect_app().interaction().stats.chat_lines_sent == lines + 1
        && c.view().world().chat.talk_focus == TalkFocus::General
        && !c.view().expect_app().interaction().last_sent.iter().any(
            |r| matches!(r, dereth_client_model::Request::Talk(m) if m.message == "general intent"),
        );

    // Switching the channel off and on again is two changes, not none: the player is put back on
    // saying it aloud by the first and left there by the second, and the line he was typing goes
    // out that way.
    hand.click_element(&mut c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    hand.type_text(&mut c, "after the fallback");
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, false);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    hand.press_return(&mut c);
    let fell_back = menu_talk_focus(&c) == SAY_MENU_ROW
        && c.view().world().chat.talk_focus == TalkFocus::All
        && c.view().expect_app().interaction().last_sent.iter().any(
            |r| matches!(r, dereth_client_model::Request::Talk(m) if m.message == "after the fallback"),
        );
    // The row is offered again, not picked again.
    let offered = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        let screen = any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen");
        let row = screen
            .main_chat
            .menu_item(&shell.ui, GENERAL_ROW)
            .expect("the general row");
        shell.ui.node(row).expect("the row is alive").state == StateId(1)
    };

    // The same edge from the option the player himself sets, with a line between the two halves:
    // the line has to see the fallback that has already happened rather than the final answer.
    pick_general(&mut c, &mut hand);
    for request in [
        dereth_client_contract::UiRequest::SetPlayerOption(
            dereth_client_contract::PlayerOption::HearGeneralChat,
            false,
        ),
        dereth_client_contract::UiRequest::ChatLine {
            text: "between the options".to_owned(),
            window: 1,
        },
        dereth_client_contract::UiRequest::SetPlayerOption(
            dereth_client_contract::PlayerOption::HearGeneralChat,
            true,
        ),
    ] {
        c.ui_outbox().emit(request);
    }
    c.tick(1);
    let in_between = c.view().world().chat.talk_focus == TalkFocus::All
        && menu_talk_focus(&c) == SAY_MENU_ROW
        && c.view().expect_app().interaction().last_sent.iter().any(
            |r| matches!(r, dereth_client_model::Request::Talk(m) if m.message == "between the options"),
        )
        && c.view().world().chat.enabled_focuses()[GENERAL_ROW as usize];

    // And the same edge when it comes from the shard's own description of the player.
    pick_general(&mut c, &mut hand);
    describe(&mut c, false, false);
    describe(&mut c, true, false);
    c.tick(1);
    let from_the_shard =
        c.view().world().chat.talk_focus == TalkFocus::All && menu_talk_focus(&c) == SAY_MENU_ROW;

    // And when it comes from the service itself going away.
    pick_general(&mut c, &mut hand);
    c.world_mut().chat.using_turbine_chat = false;
    c.world_mut().enable_chat_talk_focuses(false);
    c.world_mut().chat.using_turbine_chat = true;
    c.world_mut().enable_chat_talk_focuses(false);
    c.tick(1);
    let from_the_service = menu_talk_focus(&c) == SAY_MENU_ROW
        && c.view().world().chat.talk_focus == TalkFocus::All
        && c.world_mut().chat.take_talk_focus_notices().is_empty();

    c.assert_behaviour(
        "chat.talk-focus.a-channel-switched-off-and-on-again-drops-the-player-back-to-saying-it",
        move |_| routed && fell_back && offered && in_between && from_the_shard && from_the_service,
    );
    c.shutdown();
}

#[test]
fn scenario_a_channel_switched_off_and_on_drops_the_player_back_to_saying_it() {
    scenario("a_channel_switched_off_and_on_drops_the_player_back_to_saying_it");
}

/// A chat window built after the settings changed reads the settings as they are now, and never
/// replays what it was not there to hear.
pub fn a_new_chat_window_reads_the_settings_now() {
    use dereth_client_model::chat::TalkFocus;

    let mut c = a_bare_client(true);
    let mut hand = Hand::new();
    // The player is described before the window is built, which is the ordinary order.
    c.world_mut().chat.using_turbine_chat = true;
    describe(&mut c, false, false);
    describe(&mut c, true, false);
    to_gameplay(&mut c);

    // The offline server stub answers the opening allegiance request after the UI boundary;
    // its newly shared availability notices belong to the next frame.
    c.tick(1);
    // **Everything a frame says about the channels is delivered in that frame.** A notice left
    // over would be heard by the next window, which is the replay this claim is about; the message
    // names whatever is left so that the next producer to forget is identified rather than guessed.
    let left_over = c.world_mut().chat.take_talk_focus_notices();
    assert!(
        left_over.is_empty(),
        "a change to a channel is delivered in the frame that made it; left over: {left_over:?}"
    );

    pick_general(&mut c, &mut hand);
    let old_menu = element(&c, TALK_TO_BUTTON);
    let old_popup = talk_to_popup(&c);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, false);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    to_gameplay(&mut c);
    let rebuilt = {
        let ui = &c.view().expect_app().ui().expect("the shell").ui;
        !ui.is_alive(old_menu) && !ui.is_alive(old_popup)
    } && c.view().world().chat.talk_focus == TalkFocus::All;
    pick_general(&mut c, &mut hand);
    c.tick(1);
    let not_replayed = c.view().world().chat.talk_focus == TalkFocus::General;

    // A closed window does not suspend shared fallback; notices still are not replayed.
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::CHARACTER_MANAGEMENT);
    c.tick(1);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, false);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    c.tick(1);
    let discarded = c.world_mut().chat.take_talk_focus_notices().is_empty()
        && c.view().world().chat.talk_focus == TalkFocus::All;
    // A returning window projects that current shared choice.
    to_gameplay(&mut c);
    let chose_say = c.view().world().chat.talk_focus == TalkFocus::All;
    pick_general(&mut c, &mut hand);
    c.tick(1);
    let picked_again = c.view().world().chat.talk_focus == TalkFocus::General;

    // A line the player typed to somebody in particular, queued before the window was replaced: the
    // new window may choose a channel for the next line, but it must not turn this one into a
    // shout.
    let selected = ObjectId(0x5000_0017);
    c.world_mut().selected = Some(selected);
    // The chat window has taken him up as its chat target, which is who a tell goes to.
    c.world_mut().chat.last_speakable_target = Some(selected);
    c.world_mut().chat.set_talk_focus(TalkFocus::Selected);
    c.ui_outbox()
        .emit(dereth_client_contract::UiRequest::ChatLine {
            text: "before the window was replaced".to_owned(),
            window: 1,
        });
    to_gameplay(&mut c);
    let stayed_a_tell = c.view().expect_app().interaction().last_sent.iter().any(|r| {
        matches!(r, dereth_client_model::Request::TalkDirect(m)
            if m.target == selected && m.message == "before the window was replaced")
    }) && !c.view().expect_app().interaction().last_sent.iter().any(|r| {
        matches!(r, dereth_client_model::Request::Talk(m) if m.message == "before the window was replaced")
    });

    c.assert_behaviour(
        "chat.talk-focus.a-new-window-reads-the-settings-now-rather-than-replaying-what-it-missed",
        move |_| rebuilt && not_replayed && picked_again && stayed_a_tell,
    );
    c.assert_behaviour(
        "chat.talk-focus.channel-fallback-does-not-require-a-window",
        move |_| discarded && chose_say,
    );
    c.shutdown();
}

#[test]
fn scenario_a_new_chat_window_reads_the_settings_now() {
    scenario("a_new_chat_window_reads_the_settings_now");
}

/// Shared channel fallback runs without a window, and notices are not kept for a future one.
pub fn channel_fallback_does_not_require_a_window() {
    use dereth_client_model::chat::TalkFocus;

    let mut c = a_bare_client(false);
    let had_none = c.view().expect_app().ui().is_none();
    c.world_mut().chat.using_turbine_chat = true;
    describe(&mut c, true, false);
    c.world_mut().chat.set_talk_focus(TalkFocus::General);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, false);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    c.tick(1);
    let kept = c.view().world().chat.talk_focus == TalkFocus::All
        && c.world_mut().chat.take_talk_focus_notices().is_empty();
    c.tick(1);
    let still = c.view().world().chat.talk_focus == TalkFocus::All;

    c.assert_behaviour(
        "chat.talk-focus.channel-fallback-does-not-require-a-window",
        move |_| had_none && kept && still,
    );
    c.shutdown();
}

#[test]
fn scenario_channel_fallback_does_not_require_a_window() {
    scenario("channel_fallback_does_not_require_a_window");
}

/// Whether `popup` draws above every part of the chat window, and every one of its rows is drawn
/// whole rather than clipped into the window's log.
fn popup_is_in_front(c: &HeadlessClient, popup: ElemHandle, chat: ElemHandle) -> bool {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the shell is up").ui;
    let cmds = app.ui_draw_list();
    let last_chat = cmds
        .iter()
        .rposition(|cmd| cmd.who == chat || ui.is_ancestor_of(chat, cmd.who))
        .expect("the chat window is drawn");
    let first_popup = cmds
        .iter()
        .position(|cmd| cmd.who == popup || ui.is_ancestor_of(popup, cmd.who))
        .expect("the popup is drawn");
    if first_popup <= last_chat {
        return false;
    }
    let any: &dyn std::any::Any = app
        .ui()
        .expect("the shell")
        .flow
        .current()
        .expect("a screen");
    let screen = any
        .downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen");
    for row in std::iter::once(screen.main_chat.squelch_toggle.expect("the squelch row"))
        .chain(screen.main_chat.talk_focus_buttons.iter().copied())
    {
        let Some(cmd) = cmds.iter().find(|cmd| cmd.who == row) else {
            return false;
        };
        if cmd.glyphs.is_empty() || cmd.screen.intersect(&cmd.clip) != cmd.screen {
            return false;
        }
    }
    true
}

/// The talk-to popup draws over the chat window, a row the player may not use refuses him, a row
/// he may use closes the popup and takes the channel, and the keyboard goes back to the entry.
pub fn the_talk_to_popup_draws_over_the_window_and_gives_the_keyboard_back() {
    let mut c = a_bare_client(true);
    to_gameplay(&mut c);
    c.world_mut().chat.using_turbine_chat = true;
    c.when(Inbound::event(SessionEvent::PlayerDescription(
        Box::default(),
    )));
    c.tick(1);

    let chat = element(&c, CHAT_WINDOW);
    let popup = talk_to_popup(&c);
    let parented = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .parent(popup)
        == Some(c.view().expect_app().ui().expect("the shell").ui.root());

    let mut hand = Hand::new();
    hand.click_element(&mut c, TALK_TO_BUTTON);
    let in_front = popup_is_in_front(&c, popup, chat)
        && c.view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .active_element()
            == Some(popup);

    // The general channel is offered because the player's own description said so; the one this
    // character may never use is not.
    let (general_offered, dark_row) = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        let screen = any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen");
        let general = screen
            .main_chat
            .menu_item(&shell.ui, GENERAL_ROW)
            .expect("the general row");
        let dark = screen
            .main_chat
            .menu_item(&shell.ui, OLTHOI_MENU_ROW)
            .expect("the dark row");
        (
            c.view().world().chat.enabled_focuses()[GENERAL_ROW as usize]
                && shell.ui.node(general).expect("alive").state != StateId(0xD),
            dark,
        )
    };
    let dark_state = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .node(dark_row)
        .expect("alive")
        .state
        == StateId(0xD);
    hand.click_handle(&mut c, dark_row);
    let refused = menu_talk_focus(&c) == SAY_MENU_ROW;

    // Re-open it if that click closed it, then pick a row the player may use.
    if !c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .node(popup)
        .expect("alive")
        .region
        .flags
        .visible
    {
        hand.click_element(&mut c, TALK_TO_BUTTON);
    }
    let still_in_front = popup_is_in_front(&c, popup, chat);
    let row = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        let screen = any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen");
        screen
            .main_chat
            .menu_item(&shell.ui, SAY_MENU_ROW)
            .expect("the say row")
    };
    let hit = {
        let ui = &c.view().expect_app().ui().expect("the shell").ui;
        let b = ui.screen_box(row);
        ui.hit_test(ui.root(), (b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(row)
    };
    hand.click_handle(&mut c, row);
    let closed = menu_talk_focus(&c) == SAY_MENU_ROW
        && !c
            .view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .node(popup)
            .expect("alive")
            .region
            .flags
            .visible
        && !c
            .view()
            .expect_app()
            .ui_draw_list()
            .iter()
            .any(|cmd| cmd.who == row)
        && !c
            .view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .activatable_elements()
            .contains(&popup);

    // And the keyboard is the entry's again.
    let entry = element(&c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    hand.click_element(&mut c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    hand.type_text(&mut c, "next");
    let typed = element_text(&mut c, entry) == "next";
    // Opening the popup and then clicking away closes it without eating the next keystroke.
    hand.click_element(&mut c, TALK_TO_BUTTON);
    let reopened = popup_is_in_front(&c, popup, chat);
    hand.click_element(&mut c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    let clicked_away = !c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .node(popup)
        .expect("alive")
        .region
        .flags
        .visible;
    hand.type_text(&mut c, " operation");
    let carried_on = element_text(&mut c, entry) == "next operation";

    c.assert_behaviour(
        "chat.talk-to-menu.the-popup-draws-over-the-window-and-gives-the-keyboard-back",
        move |_| {
            parented
                && in_front
                && general_offered
                && dark_state
                && refused
                && still_in_front
                && hit
                && closed
                && typed
                && reopened
                && clicked_away
                && carried_on
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_talk_to_popup_draws_over_the_window_and_gives_the_keyboard_back() {
    scenario("the_talk_to_popup_draws_over_the_window_and_gives_the_keyboard_back");
}

// ---------------------------------------------------------------------------------------------
// The selection area beside the chat window
// ---------------------------------------------------------------------------------------------
//
// **This row belongs in another subject.** What the selection field draws is the shell's claim and
// not chat's; it is kept here, beside the chat window it borders.

/// The image the field draws, and the state it is in.
fn selection_field(c: &HeadlessClient, want_state: u32) -> bool {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the shell is up").ui;
    let field = {
        let any: &dyn std::any::Any = app
            .ui()
            .expect("the shell")
            .flow
            .current()
            .expect("a screen");
        let root = any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen")
            .root()
            .expect("a root");
        ui.get_child_recursive(root, SELECTION_FIELD)
            .expect("the selection field")
    };
    let Some(n) = ui.node(field) else {
        return false;
    };
    // The field is authored visible even when nothing is picked: what changes is its state, not
    // whether it is there.
    if n.state != StateId(want_state) || !n.region.flags.visible {
        return false;
    }
    let last_image = |media: &[dereth_ui::desc::MediaDesc]| {
        media.iter().rev().find_map(|m| match m.fields {
            dereth_ui::desc::MediaFields::Image { file, .. } => Some(file),
            _ => None,
        })
    };
    let image = n
        .desc
        .access_state(StateId(want_state))
        .and_then(|d| last_image(&d.media))
        .or_else(|| last_image(&n.desc.base.media))
        .expect("the shipped field has an authored image");
    let Some(cmd) = app.ui_draw_list().iter().find(|cmd| cmd.who == field) else {
        return false;
    };
    cmd.image == Some(image) && image.0 != 0
}

/// Picking an object fills the field beside the chat window, a stack of more than one changes it
/// again, and picking nothing puts it back to the background it was authored with.
pub fn the_selection_field_follows_what_is_picked_and_clears() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let name = element(&c, SELECTION_NAME);
    let empty_to_start = selection_field(&c, 0);

    let id = ObjectId(0x7100_0021);
    {
        let mut w = dereth_client_model::Weenie::new(id);
        w.pwd.name = "a picked thing".to_owned();
        w.pwd.obj_type = 1;
        c.world_mut().tables.weenies.insert(id, w);
    }
    let mut choose = |c: &mut HeadlessClient, id: Option<ObjectId>| {
        c.world_mut().set_selected_object(
            id,
            false,
            &mut dereth_client_model::RecordingSink::default(),
        );
        c.tick(1);
    };
    choose(&mut c, Some(id));
    let picked = selection_field(&c, 0x1000_000B) && element_text(&mut c, name) == "a picked thing";

    // A stack of more than one is a different state again, and dropping back to one puts it back.
    c.world_mut()
        .tables
        .weenies
        .get_mut(id)
        .expect("the thing")
        .pwd
        .stack_size = Some(3);
    c.tick(1);
    let stacked = selection_field(&c, 0x1000_000C);
    c.world_mut()
        .tables
        .weenies
        .get_mut(id)
        .expect("the thing")
        .pwd
        .stack_size = Some(1);
    c.tick(1);
    let unstacked = selection_field(&c, 0x1000_000B);

    choose(&mut c, None);
    let cleared = selection_field(&c, 0) && element_text(&mut c, name).is_empty();

    // And the same when the thing itself has gone rather than merely been unpicked.
    choose(&mut c, Some(id));
    let picked_again = selection_field(&c, 0x1000_000B);
    c.world_mut().tables.weenies.remove(id);
    choose(&mut c, None);
    let gone = selection_field(&c, 0) && element_text(&mut c, name).is_empty();

    // An empty field does not block anything else: the talk-to popup still opens.
    let mut hand = Hand::new();
    hand.click_element(&mut c, TALK_TO_BUTTON);
    let popup = talk_to_popup(&c);
    let unblocked = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .node(popup)
        .expect("alive")
        .region
        .flags
        .visible;

    c.assert_behaviour(
        "chat.selection.the-field-beside-the-window-follows-what-is-picked-and-clears",
        move |_| {
            empty_to_start
                && picked
                && stacked
                && unstacked
                && cleared
                && picked_again
                && gone
                && unblocked
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_selection_field_follows_what_is_picked_and_clears() {
    scenario("the_selection_field_follows_what_is_picked_and_clears");
}

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

#[test]
fn scenario_every_channel_word_sends_its_own_channel() {
    scenario("every_channel_word_sends_its_own_channel");
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

#[test]
fn scenario_an_empty_channel_command_says_what_to_do_and_sends_nothing() {
    scenario("an_empty_channel_command_says_what_to_do_and_sends_nothing");
}

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

/// Put `lines` into the shipped chat window, through the window's own delivery.
fn into_the_window(
    c: &mut HeadlessClient,
    lines: &[dereth_ui_screens::chat::interface::ChatMessage],
) {
    for (n, m) in lines.iter().enumerate() {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the UI shell is up");
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");
        assert!(
            !gameplay.recv_display_final_string_info(ui, m).is_empty(),
            "line {n} reached a shipped chat window"
        );
    }
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

#[test]
fn scenario_the_bottom_row_of_the_log_is_the_newest_line() {
    scenario("the_bottom_row_of_the_log_is_the_newest_line");
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

#[test]
fn scenario_the_newline_between_lines_is_a_separator() {
    scenario("the_newline_between_lines_is_a_separator");
}

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

#[test]
fn scenario_the_log_draws_the_name_and_not_the_markup() {
    scenario("the_log_draws_the_name_and_not_the_markup");
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

#[test]
fn scenario_the_clickable_name_is_drawn_in_its_own_colour() {
    scenario("the_clickable_name_is_drawn_in_its_own_colour");
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

#[test]
fn scenario_markup_the_client_does_not_know_is_drawn_as_it_stands() {
    scenario("markup_the_client_does_not_know_is_drawn_as_it_stands");
}

// ---------------------------------------------------------------------------------------------
// The squelch tab
// ---------------------------------------------------------------------------------------------
//
// **No datagram leaves this process**: the shard's answer is built with the production writer and
// handed to the reader, and the two sends are read out of the client's own outbox as the bytes
// that *would* go out. Squelching somebody is real state on a live shard and nothing here does it.

/// The social page, and the recording whose login puts a real character behind the tab.
const SOCIAL_PAGE_ID: ElementId = ElementId(0x1000_018F);
const SOCIAL_BUTTON_ID: u32 = 0x0C;
const LOGIN_SESSION: &str = "long-solo-play";

/// Replay the recorded login up to the moment the shard creates the player, so that the tab is
/// driven with a live character the way it is in the running client.
///
/// The blob to stop at is **found** rather than pinned: it is the first one that creates a player.
fn corpus_login(c: &mut HeadlessClient) {
    let corpus = Corpus::load(LOGIN_SESSION)
        .expect("the corpus decodes")
        .expect("the corpus holds the recording");
    let end = corpus
        .blobs
        .iter()
        .find(|b| b.dir == Direction::ServerToClient && b.opcode == 0xF746)
        .map(|b| b.idx)
        .expect("the recorded login creates a player");
    c.when(Inbound::from_corpus(LOGIN_SESSION, 0..end + 1));
    assert!(
        c.view().world().player.is_some(),
        "the recorded login reaches the player's creation"
    );
    c.tick(2);
}

/// How many requests the client has produced so far, to count the next window from.
fn mark(c: &HeadlessClient) -> usize {
    c.view().outbound().len()
}

/// Run `f` against the shipped gameplay screen's root and the shell it was laid out in.
fn on_the_shipped_tree<T>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut dereth_ui::UiSystem, ElemHandle) -> T,
) -> T {
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the UI shell is up");
    let root = {
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen")
            .root()
            .expect("the gameplay screen's root")
    };
    f(&mut shell.ui, root)
}

/// The squelch tab as the client holds it.
fn squelch_panel(c: &HeadlessClient) -> &dereth_ui_screens::panels::squelch::SquelchPanel {
    &c.view().expect_app().hud().panels.squelch
}

/// `(squelch this character, squelch this account, remove)` as a player sees them -- read off the
/// live elements and not off the tab's own mirror, because a tab that worked out the right answer
/// and wrote it nowhere is exactly what this is about.
fn squelch_buttons(c: &HeadlessClient) -> (bool, bool, bool) {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the shell is up").ui;
    let p = &app.hud().panels.squelch;
    let read = |h: Option<ElemHandle>| {
        h.is_some_and(|h| dereth_ui_screens::panels::squelch::button_enabled(ui, h))
    };
    (
        read(p.squelch_character_button),
        read(p.squelch_account_button),
        read(p.remove_button),
    )
}

fn squelch_name_box(c: &mut HeadlessClient) -> String {
    let entry = squelch_panel(c).name_entry.expect("the name box is bound");
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .text_element_mut(entry)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// Open the social page and then the squelch tab, by real presses. The tab is **discovered** off
/// the live tab table -- the page whose subtree carries the squelch list -- rather than named, so a
/// layout change fails loudly instead of quietly binding the wrong caption.
fn open_the_squelch_tab(c: &mut HeadlessClient, hand: &mut Hand) {
    let button = {
        let app = c.view().expect_app();
        let any: &dyn std::any::Any = app
            .ui()
            .expect("the shell")
            .flow
            .current()
            .expect("a screen");
        any.downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen")
            .toolbar
            .buttons
            .iter()
            .find(|b| b.panel_id == SOCIAL_BUTTON_ID)
            .expect("one of the shipped toolbar's buttons opens the social page")
            .handle
    };
    hand.click_handle(c, button);
    let tab = on_the_shipped_tree(c, |ui, root| {
        let page = ui
            .get_child_recursive(root, SOCIAL_PAGE_ID)
            .expect("the social page");
        let pairs: Vec<(ElementId, ElementId)> = ui
            .node(page)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .expect("the social page is a tabbed container")
            .page_to_tab
            .iter()
            .map(|(p, t)| (*p, *t))
            .collect();
        for (page_id, tab_id) in pairs {
            let Some(pe) = ui.get_child_recursive(page, page_id) else {
                continue;
            };
            if ui
                .get_child_recursive(pe, dereth_ui_screens::panels::squelch::SQUELCH_LIST)
                .is_some()
            {
                return ui
                    .get_child_recursive(page, tab_id)
                    .expect("the tab caption");
            }
        }
        panic!("no page of the social panel carries the squelch list");
    });
    hand.click_handle(c, tab);
    let h = squelch_panel(c)
        .panel
        .expect("the squelch tab is in the shipped tree");
    assert!(
        c.view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .is_visible(h),
        "clicking the squelch caption must bring the tab up"
    );
}

/// Put the caret in the name box with a real press, then type. The caret first: a keystroke only
/// reaches an element that has it, and a press is the only thing in the client that moves it.
fn type_into_the_squelch_box(c: &mut HeadlessClient, hand: &mut Hand, text: &str) {
    let entry = squelch_panel(c).name_entry.expect("the name box is bound");
    if c.view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .focus_element()
        != Some(entry)
    {
        hand.click_handle(c, entry);
        assert_eq!(
            c.view()
                .expect_app()
                .ui()
                .expect("the shell")
                .ui
                .focus_element(),
            Some(entry),
            "a press on the name box must take the caret"
        );
    }
    {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        for ch in text.encode_utf16() {
            ui.character(ch);
        }
    }
    c.tick(2);
}

/// One backspace, through the key the player presses. It is **not** the programmatic clear, and
/// the two paths differ.
fn backspace(c: &mut HeadlessClient, times: usize) {
    for _ in 0..times {
        {
            let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
            ui.key_press(&dereth_ui::focus::InputEvent {
                action: dereth_ui::focus::action::BACKSPACE,
                start: true,
                x: 0,
                y: 0,
            });
        }
        c.tick(1);
    }
}

/// One squelched character, as the shard names him.
fn squelched(name: &str, account: bool) -> dereth_protocol::comms::SquelchInfo {
    dereth_protocol::comms::SquelchInfo {
        squelch_msgs: dereth_protocol::comms::VLong(vec![
            0xFFFF_FFFF,
            0xFFFF_FFFF,
            0xFFFF_FFFF,
            0xFFFF_FFFF,
        ]),
        name: name.to_owned(),
        is_zone_squelch: i32::from(account),
    }
}

/// The whole squelch list the shard sends after every change it accepts.
fn squelch_list(
    characters: Vec<(u32, dereth_protocol::comms::SquelchInfo)>,
) -> dereth_protocol::comms::CommunicationSetSquelchDb {
    dereth_protocol::comms::CommunicationSetSquelchDb(dereth_protocol::comms::SquelchDb {
        // The shard sends this half always empty and folds an account squelch into the other one.
        account_hash: dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries: Vec::new(),
        },
        character_hash: dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries: characters,
        },
        global_squelch_info: dereth_protocol::comms::SquelchInfo {
            squelch_msgs: dereth_protocol::comms::VLong(vec![0, 0, 0, 0]),
            name: String::new(),
            is_zone_squelch: 0,
        },
    })
}

/// The squelch messages the client put in its outbox after `from`, as the bytes that would go out.
fn squelch_sent_since(c: &HeadlessClient, from: usize) -> Vec<Vec<u8>> {
    use dereth_protocol::Opcode;
    c.view().outbound()[from..]
        .iter()
        .filter_map(|r| {
            let (op, body) = match r {
                dereth_client_model::Request::ModifyCharacterSquelch(m) => (
                    Opcode::COMMUNICATION_MODIFY_CHARACTER_SQUELCH,
                    dereth_protocol::write_body(m),
                ),
                dereth_client_model::Request::ModifyAccountSquelch(m) => (
                    Opcode::COMMUNICATION_MODIFY_ACCOUNT_SQUELCH,
                    dereth_protocol::write_body(m),
                ),
                _ => return None,
            };
            let mut blob = op.0.to_le_bytes().to_vec();
            blob.extend(body.expect("the body encodes"));
            Some(blob)
        })
        .collect()
}

/// A client logged in from the recording, with the squelch tab open.
fn a_client_on_the_squelch_tab(hand: &mut Hand) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(6));
    corpus_login(&mut c);
    open_the_squelch_tab(&mut c, hand);
    c
}

/// Deliver the shard's whole squelch list and let the tab redraw.
fn deliver_squelch_list(
    c: &mut HeadlessClient,
    characters: Vec<(u32, dereth_protocol::comms::SquelchInfo)>,
) {
    c.when(Inbound::message(&squelch_list(characters)));
    c.tick(2);
}

/// The tab opens with both squelch buttons lit over an empty box, and remove dark.
pub fn the_squelch_tab_opens_with_both_buttons_lit() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    let empty = squelch_name_box(&mut c).is_empty();
    // Both squelch buttons are lit whatever the box says; remove is dark because nothing is
    // picked. This is what says the darkness below is not simply always right.
    let lit = squelch_buttons(&c) == (true, true, false);

    c.assert_behaviour(
        "chat.squelch-panel.the-tab-opens-with-both-buttons-lit-over-an-empty-box",
        move |_| empty && lit,
    );
    c.shutdown();
}

#[test]
fn scenario_the_squelch_tab_opens_with_both_buttons_lit() {
    scenario("the_squelch_tab_opens_with_both_buttons_lit");
}

/// Typing in the name box arms both buttons and emptying it again darkens both -- which is the one
/// rule in the whole tab that reads the box, and it reads it only when a key was pressed.
pub fn emptying_the_squelch_name_box_dims_both_buttons() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);

    type_into_the_squelch_box(&mut c, &mut hand, "A");
    let typed = squelch_name_box(&mut c) == "A"
        && (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, true);

    backspace(&mut c, 1);
    let emptied = squelch_name_box(&mut c).is_empty()
        && (squelch_buttons(&c).0, squelch_buttons(&c).1) == (false, false);

    type_into_the_squelch_box(&mut c, &mut hand, "B");
    let re_armed = (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, true);

    c.assert_behaviour(
        "chat.squelch-panel.emptying-the-name-box-darkens-both-buttons-and-typing-arms-them",
        move |_| typed && emptied && re_armed,
    );
    c.shutdown();
}

#[test]
fn scenario_emptying_the_squelch_name_box_dims_both_buttons() {
    scenario("emptying_the_squelch_name_box_dims_both_buttons");
}

/// Pressing one of the two squelch buttons empties the box, sends that button's own message, and
/// darkens that button and no other.
pub fn squelching_clears_the_box_and_dims_that_button_alone() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    type_into_the_squelch_box(&mut c, &mut hand, "Bob");
    let armed = squelch_buttons(&c).0;

    let b = squelch_panel(&c).squelch_character_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, b);
    let sent = squelch_sent_since(&c, before);
    let character = squelch_name_box(&mut c).is_empty()
        && sent.len() == 1
        && sent[0][0..4] == [0x58, 0, 0, 0]
        // The pressed button darkens; the other is still lit over an empty box, which is the
        // client's own asymmetry and not the same thing the box's own rule does.
        && (squelch_buttons(&c).0, squelch_buttons(&c).1) == (false, true);

    // The other button, the same way round.
    type_into_the_squelch_box(&mut c, &mut hand, "Dave");
    let b = squelch_panel(&c).squelch_account_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, b);
    let sent = squelch_sent_since(&c, before);
    let account = sent.len() == 1
        && sent[0][0..4] == [0x59, 0, 0, 0]
        && (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, false);

    c.assert_behaviour(
        "chat.squelch-panel.squelching-clears-the-box-and-darkens-that-button-alone",
        move |_| armed && character && account,
    );
    c.shutdown();
}

#[test]
fn scenario_squelching_clears_the_box_and_dims_that_button_alone() {
    scenario("squelching_clears_the_box_and_dims_that_button_alone");
}

/// The shard's answer draws the new row and lights both buttons again -- so the darkness a player
/// sees after pressing one lasts exactly as long as the round trip.
pub fn the_shards_answer_lights_both_buttons_and_adds_the_row() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    type_into_the_squelch_box(&mut c, &mut hand, "Bob");
    let b = squelch_panel(&c).squelch_character_button.expect("bound");
    hand.click_handle(&mut c, b);
    let dark_in_flight = !squelch_buttons(&c).0;

    deliver_squelch_list(&mut c, vec![(0x5000_001E, squelched("Bob", false))]);
    let drawn = squelch_panel(&c).shown() == vec!["Bob".to_owned()];
    let lit_again = (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, true);

    c.assert_behaviour(
        "chat.squelch-panel.the-shards-answer-lights-both-buttons-again-and-adds-the-row",
        move |_| dark_in_flight && drawn && lit_again,
    );
    c.shutdown();
}

#[test]
fn scenario_the_shards_answer_lights_both_buttons_and_adds_the_row() {
    scenario("the_shards_answer_lights_both_buttons_and_adds_the_row");
}

/// Picking a row in the list lights both squelch buttons again, over an empty box.
///
/// **This is the client's own behaviour and the scenario says so out loud.** Picking a row does
/// not look at the name box at all, so a darkness the box's own rule caused is undone by it. If
/// the buttons are ever made to follow the box whatever happened, this is what
/// will go red -- which is what makes that a decision somebody takes rather than a drift somebody
/// notices.
pub fn picking_a_squelch_row_lights_both_buttons_again() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    deliver_squelch_list(
        &mut c,
        vec![
            (0x5000_001E, squelched("Bob", false)),
            (0x5000_001F, squelched("Carol", false)),
        ],
    );
    let two_rows = squelch_panel(&c).shown() == vec!["Bob".to_owned(), "Carol".to_owned()];

    // Darken one button the only way a player can: type a name and press it.
    type_into_the_squelch_box(&mut c, &mut hand, "Dave");
    let b = squelch_panel(&c).squelch_character_button.expect("bound");
    hand.click_handle(&mut c, b);
    let darkened = squelch_name_box(&mut c).is_empty()
        && (squelch_buttons(&c).0, squelch_buttons(&c).1) == (false, true);

    // Now the gesture.
    let (list, row) = squelch_panel(&c)
        .list
        .as_ref()
        .and_then(|l| Some((l.handle, l.items.first().copied()?)))
        .expect("the list drew its rows");
    hand.click_row(&mut c, list, row);
    let picked = squelch_panel(&c).selected == Some(0);
    let box_still_empty = squelch_name_box(&mut c).is_empty();
    let lit_again = (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, true);

    c.assert_behaviour(
        "chat.squelch-panel.picking-a-row-lights-both-buttons-again-over-an-empty-box",
        move |_| two_rows && darkened && picked && box_still_empty && lit_again,
    );
    c.shutdown();
}

#[test]
fn scenario_picking_a_squelch_row_lights_both_buttons_again() {
    scenario("picking_a_squelch_row_lights_both_buttons_again");
}

/// Remove follows what is picked, and the two squelch buttons are on another rule entirely.
///
/// It is worth asserting on its own: if a press on a row reached nothing at all, both squelch
/// buttons would still read lit -- they already were -- and the scenario above would pass while
/// measuring silence.
pub fn the_remove_button_follows_the_selection() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    let dark_to_start = !squelch_buttons(&c).2;

    deliver_squelch_list(&mut c, vec![(0x5000_001E, squelched("Bob", false))]);
    // A redraw picks nothing, so remove is still dark.
    let still_dark = !squelch_buttons(&c).2;

    let (list, row) = squelch_panel(&c)
        .list
        .as_ref()
        .and_then(|l| Some((l.handle, l.items.first().copied()?)))
        .expect("the list drew its row");
    hand.click_row(&mut c, list, row);
    let picked = squelch_panel(&c).selected == Some(0);
    let lit = squelch_buttons(&c).2;
    let others = (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, true);

    c.assert_behaviour(
        "chat.squelch-panel.remove-follows-what-is-picked",
        move |_| dark_to_start && still_dark && picked && lit && others,
    );
    c.shutdown();
}

#[test]
fn scenario_the_remove_button_follows_the_selection() {
    scenario("the_remove_button_follows_the_selection");
}

// ---------------------------------------------------------------------------------------------
// What the squelch tab lists, and what its buttons send
// ---------------------------------------------------------------------------------------------

/// The names the shipped squelch list holds, in the order a player sees them.
fn squelch_names(c: &mut HeadlessClient) -> Vec<String> {
    squelch_panel(c).shown()
}

/// What each row says about the kind of squelch it is, in list order.
fn squelch_row_states(c: &HeadlessClient) -> Vec<dereth_ui::StateId> {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the shell is up").ui;
    app.hud()
        .panels
        .squelch
        .rows()
        .iter()
        .filter_map(|r| {
            ui.get_child_recursive(r.element, dereth_ui_screens::panels::squelch::ROW_NAME)
        })
        .filter_map(|t| ui.node(t).map(|n| n.state))
        .collect()
}

/// The bytes one squelch-a-character message goes out in: whether it is being added or taken off,
/// a place for an id that is never used, the name, and the whole set of kinds of line. Composed
/// here rather than by the writer being asserted over.
fn character_squelch_bytes(add: u32, name: &str) -> Vec<u8> {
    let mut v = vec![0x58, 0x00, 0x00, 0x00];
    v.extend(add.to_le_bytes());
    v.extend(0_u32.to_le_bytes());
    v.extend(u16::try_from(name.len()).expect("short").to_le_bytes());
    v.extend(name.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v.extend(1_u32.to_le_bytes());
    v
}

/// The bytes one squelch-a-whole-account message goes out in: no id and no kind, only the name.
fn account_squelch_bytes(add: u32, name: &str) -> Vec<u8> {
    let mut v = vec![0x59, 0x00, 0x00, 0x00];
    v.extend(add.to_le_bytes());
    v.extend(u16::try_from(name.len()).expect("short").to_le_bytes());
    v.extend(name.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

/// The tab lists who the shard says is squelched, in one block sorted by name, with each row
/// saying whether it is a character or a whole account.
pub fn the_squelch_tab_lists_who_the_shard_says_is_squelched() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    // Before: bound, driven, and nothing on it -- which is a different reading from never run.
    let before = squelch_panel(&c).bound()
        && squelch_panel(&c).rebuilds >= 1
        && c.view().expect_app().hud().stats.squelch_db_applied == 0
        && squelch_names(&mut c).is_empty();

    // The empty list every recorded session's login carries: a real arrival that draws nothing.
    deliver_squelch_list(&mut c, Vec::new());
    let empty_arrived = c.view().expect_app().hud().stats.squelch_db_applied == 1
        && squelch_names(&mut c).is_empty();

    // A populated one, out of order, and with one whole account among the characters.
    deliver_squelch_list(
        &mut c,
        vec![
            (0x5000_0031, squelched("Zed", false)),
            (0x5000_001E, squelched("Ash", false)),
            (0x5000_001F, squelched("Bex", true)),
        ],
    );
    let held = c.view().world().chat.squelch.characters.len() == 3;
    // **One** block, sorted by name: unlike the friends list there is no second block for the
    // accounts, and they are mixed in with the characters.
    let sorted =
        squelch_names(&mut c) == vec!["Ash".to_owned(), "Bex".to_owned(), "Zed".to_owned()];
    let kinds = squelch_row_states(&c)
        == vec![
            dereth_ui_screens::panels::squelch::ROW_STATE_CHARACTER,
            dereth_ui_screens::panels::squelch::ROW_STATE_ACCOUNT,
            dereth_ui_screens::panels::squelch::ROW_STATE_CHARACTER,
        ];

    // Idempotent, and it flushes rather than appending.
    let rebuilds = squelch_panel(&c).rebuilds;
    c.tick(1);
    let unchanged = squelch_panel(&c).rebuilds == rebuilds && squelch_names(&mut c).len() == 3;

    c.assert_behaviour(
        "chat.squelch-panel.the-tab-lists-who-the-shard-says-is-squelched-in-one-sorted-block",
        move |_| before && empty_arrived && held && sorted && kinds && unchanged,
    );
    c.shutdown();
}

#[test]
fn scenario_the_squelch_tab_lists_who_the_shard_says_is_squelched() {
    scenario("the_squelch_tab_lists_who_the_shard_says_is_squelched");
}

/// The two squelch buttons send two different messages about the same typed name, and the one for
/// a whole account is shorter by exactly the two things it has no room for.
pub fn the_two_squelch_buttons_send_different_messages() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);

    type_into_the_squelch_box(&mut c, &mut hand, "Bob");
    let b = squelch_panel(&c).squelch_character_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, b);
    let character = squelch_sent_since(&c, before) == vec![character_squelch_bytes(1, "Bob")]
        && squelch_panel(&c).character_squelch_requests == 1;

    type_into_the_squelch_box(&mut c, &mut hand, "Bob");
    let b = squelch_panel(&c).squelch_account_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, b);
    let sent = squelch_sent_since(&c, before);
    let account = sent == vec![account_squelch_bytes(1, "Bob")]
        && squelch_panel(&c).account_squelch_requests == 1;
    // With the same name, the account one is eight bytes shorter -- exactly the unused place for
    // an id and the set of kinds it does not carry.
    let shorter = sent[0].len() + 8 == character_squelch_bytes(1, "Bob").len();

    c.assert_behaviour(
        "chat.squelch-panel.the-two-buttons-send-different-messages-about-the-same-name",
        move |_| character && account && shorter,
    );
    c.shutdown();
}

#[test]
fn scenario_the_two_squelch_buttons_send_different_messages() {
    scenario("the_two_squelch_buttons_send_different_messages");
}

/// Taking somebody off the list sends the kind of un-squelch the row itself names, so a whole
/// account and a single character are undone the way each was done.
pub fn removing_sends_the_kind_the_row_itself_names() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    deliver_squelch_list(
        &mut c,
        vec![
            (0x5000_001E, squelched("Ash", false)),
            (0x5000_001F, squelched("Bex", true)),
        ],
    );
    let two = squelch_names(&mut c) == vec!["Ash".to_owned(), "Bex".to_owned()];

    let list = squelch_panel(&c)
        .list
        .as_ref()
        .expect("the list is bound")
        .handle;
    let row = squelch_panel(&c).rows()[0].element;
    hand.click_row(&mut c, list, row);
    let picked = squelch_panel(&c).selected == Some(0);
    // The picked row is drawn in the state the list gives a picked row, so a player can see which
    // one it is; the other is not.
    let visibly_picked = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the shell").ui;
        let other = app.hud().panels.squelch.rows()[1].element;
        ui.node(row).map(|n| n.state.0) == Some(ui.get_attribute_enum(list, 0x5E).1)
            && ui.node(other).map(|n| n.state.0) != ui.node(row).map(|n| n.state.0)
    };

    let h = squelch_panel(&c).remove_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, h);
    let a_character = squelch_sent_since(&c, before) == vec![character_squelch_bytes(0, "Ash")]
        && squelch_panel(&c).remove_requests == 1;

    // The other row is the other arm of the same rule.
    let row = squelch_panel(&c).rows()[1].element;
    hand.click_row(&mut c, list, row);
    let picked_second = squelch_panel(&c).selected == Some(1);
    let h = squelch_panel(&c).remove_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, h);
    let an_account = squelch_sent_since(&c, before) == vec![account_squelch_bytes(0, "Bex")]
        && squelch_panel(&c).remove_requests == 2;

    c.assert_behaviour(
        "chat.squelch-panel.removing-sends-the-kind-the-row-itself-names",
        move |_| two && picked && visibly_picked && a_character && picked_second && an_account,
    );
    c.shutdown();
}

#[test]
fn scenario_removing_sends_the_kind_the_row_itself_names() {
    scenario("removing_sends_the_kind_the_row_itself_names");
}

/// The name box takes the caret from a press, and once it has it there is a caret to draw.
pub fn the_squelch_name_box_takes_the_caret_from_a_press() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    let entry = squelch_panel(&c).name_entry.expect("the name box is bound");
    let not_yet = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .focus_element()
        != Some(entry);

    hand.click_handle(&mut c, entry);
    let took_it = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .focus_element()
        == Some(entry);

    let inside = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let (ox, oy) = ui.screen_origin(entry);
        let r = ui.node(entry).expect("alive").region.box_;
        let screen = dereth_ui::Box2D {
            x0: ox,
            y0: oy,
            x1: ox + r.width(),
            y1: oy + r.height(),
        };
        let caret = ui
            .text_element_mut(entry)
            .expect("the name box is a text element")
            .caret_box(screen)
            .expect("a box that can be typed in and has the caret has a caret to draw");
        caret.x0 >= screen.x0 && caret.x1 <= screen.x1 && caret.height() > 0
    };

    c.assert_behaviour(
        "chat.squelch-panel.the-name-box-takes-the-caret-from-a-press",
        move |_| not_yet && took_it && inside,
    );
    c.shutdown();
}

#[test]
fn scenario_the_squelch_name_box_takes_the_caret_from_a_press() {
    scenario("the_squelch_name_box_takes_the_caret_from_a_press");
}

/// The label beside the name box is wider than the box it was drawn in, so it wraps on to two
/// lines -- **and that is what the shipped layout does**, not a defect in the drawing.
pub fn the_name_label_is_wider_than_its_box_and_wraps() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    let label = on_the_shipped_tree(&mut c, |ui, root| {
        ui.get_child_recursive(root, ElementId(0x1000_053F))
            .expect("the label is in the tree")
    });

    let (box_, one_line_attr) = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the shell").ui;
        let n = ui.node(label).expect("alive");
        (
            n.region.box_,
            n.merged_properties()
                .0
                .get(&dereth_ui::props::attr::TEXT_ONE_LINE)
                .is_some(),
        )
    };
    // The shipped layout carries no instruction to keep it on one line; its neighbours do.
    let measured = (box_.x0, box_.x1) == (0, 99) && box_.height() == 18 && !one_line_attr;

    let (text, one_line, margins, glyphs) = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let t = ui.text_element_mut(label).expect("a text element");
        (
            t.glyphs.inq_text(false),
            t.glyphs.one_line,
            t.margins,
            t.glyphs.glyphs.clone(),
        )
    };
    let label_reads = text == "Character Name:" && !one_line && margins == (0, 0, 0, 6);
    let wider = glyphs.iter().map(|g| g.width).sum::<i32>() == 103;

    // The width it is laid out in, and what it does with it: it breaks at the space, and the space
    // stays on the first line while not counting towards its width.
    let width = box_.x1 - box_.x0 - margins.2 - margins.3 + 1;
    let lines = dereth_ui::text::glyph::wrap(&glyphs, width, false);
    let wraps = width == 94
        && lines.len() == 2
        // Nor does it fit in the whole box, so the margins are not what decides it.
        && dereth_ui::text::glyph::wrap(&glyphs, box_.width(), false).len() == 2
        && lines.iter().map(|l| l.width).collect::<Vec<_>>() == vec![60, 39];
    let first = lines[0];
    let space_stays = glyphs[first.start..first.end]
        .iter()
        .map(|g| g.width)
        .sum::<i32>()
        == 64
        && glyphs[first.end - 1].is_white_space()
        && 64 - 60 == glyphs[first.end - 1].width
        && lines[1].end == glyphs.len();

    c.assert_behaviour(
        "chat.squelch-panel.the-name-label-is-wider-than-its-box-and-wraps-in-retail-too",
        move |_| measured && label_reads && wider && wraps && space_stays,
    );
    c.shutdown();
}

#[test]
fn scenario_the_name_label_is_wider_than_its_box_and_wraps() {
    scenario("the_name_label_is_wider_than_its_box_and_wraps");
}

// ---------------------------------------------------------------------------------------------
// The chat tab of the options: which kinds of line each window shows
// ---------------------------------------------------------------------------------------------

/// The five chat windows the page edits, in the order it builds them.
const FILTER_WINDOWS: [u32; 5] = [8, 2, 3, 4, 5];
/// What a check box carries when it is ticked.
const ATTR_CHECKED: u32 = 0x0E;

/// The thirteen rows the page offers, in the order it adds them, as `(token, mask, caption)`.
///
/// **Five of them are the global channels**: they are rows of this same list and not a second
/// group, a second page or a set the shard sends.
const FILTER_ROWS: [(&str, u64, &str); 13] = [
    ("ID_ChatOption_TextFilter_Gameplay", 0x8391_2021, "Gameplay"),
    ("ID_ChatOption_TextFilter_Combat", 0x0060_0040, "Combat"),
    ("ID_ChatOption_TextFilter_Magic", 0x0002_0080, "Magic"),
    (
        "ID_ChatOption_TextFilter_AreaSpeech",
        0x0000_1004,
        "Area Chat",
    ),
    ("ID_ChatOption_TextFilter_Tells", 0x0000_0018, "Tells"),
    (
        "ID_ChatOption_TextFilter_Allegience",
        0x0004_0C00,
        "Allegiance",
    ),
    (
        "ID_ChatOption_TextFilter_Fellowship",
        0x0008_0000,
        "Fellowship",
    ),
    (
        "ID_ChatOption_TextFilter_General",
        0x0800_0000,
        "General Channel",
    ),
    (
        "ID_ChatOption_TextFilter_Trade",
        0x1000_0000,
        "Trade Channel",
    ),
    ("ID_ChatOption_TextFilter_LFG", 0x2000_0000, "LFG Channel"),
    (
        "ID_ChatOption_TextFilter_Roleplay",
        0x4000_0000,
        "Roleplay Channel",
    ),
    (
        "ID_ChatOption_TextFilter_Society",
        0x1_0000_0000,
        "Society Channel",
    ),
    ("ID_ChatOption_TextFilter_Error", 0x0400_0000, "Errors"),
];

/// Run `f` against the shipped gameplay screen and the shell it was laid out in.
fn with_screen<T>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut dereth_ui::UiSystem, &mut GamePlayScreen) -> T,
) -> T {
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    let gameplay = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen");
    f(&mut shell.ui, gameplay)
}

/// Open the toolbar panel that holds an option page, then that page's own tab, by pressing what a
/// player presses.
fn open_the_options_page(c: &mut HeadlessClient, hand: &mut Hand, page_element: ElementId) {
    let (panel_id, container, page) = with_screen(c, |ui, s| {
        let root = s.root().expect("the screen root");
        let page = ui
            .get_child_recursive(root, page_element)
            .expect("the page is in the shipped tree");
        let info = s
            .panels
            .pages
            .iter()
            .copied()
            .find(|p| {
                let mut h = Some(page);
                while let Some(cur) = h {
                    if cur == p.handle {
                        return true;
                    }
                    h = ui.parent(cur);
                }
                false
            })
            .expect("the page sits inside a toolbar panel");
        (info.panel_id, info.handle, page)
    });
    let button = with_screen(c, |_, s| {
        s.toolbar
            .buttons
            .iter()
            .find(|b| b.panel_id == panel_id)
            .unwrap_or_else(|| panic!("the shipped toolbar has no button for panel {panel_id}"))
            .handle
    });
    hand.click_handle(c, button);
    let tab = with_screen(c, |ui, _| {
        let tab = ui
            .node(container)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .and_then(|p| p.page_to_tab.get(&page_element).copied())
            .expect("the options panel's tab table names the page");
        ui.get_child_recursive(container, tab)
            .expect("the tab caption element")
    });
    hand.click_handle(c, tab);
    assert!(
        c.view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .is_visible(page),
        "the tab must be up"
    );
}

/// A client with the chat tab of the options open, reached the way a player reaches it.
fn a_client_on_the_chat_options_tab(hand: &mut Hand) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // Without a description of the player there is nothing for an option to be written into.
    c.when(Inbound::event(SessionEvent::PlayerDescription(
        Box::default(),
    )));
    c.tick(3);
    open_the_options_page(
        &mut c,
        hand,
        dereth_ui_screens::options::chat::CHAT_PAGE_ELEMENT,
    );
    c
}

/// Every row of one window's filter, as `(token, mask, the caption drawn, the box's element)`.
type FilterRow = (&'static str, u64, Option<String>, Option<ElemHandle>);
fn filter_rows(c: &mut HeadlessClient, window: u32) -> Vec<FilterRow> {
    with_screen(c, |_, s| {
        let i = s
            .chat_options
            .filter_of(window)
            .expect("a filter for that window");
        match &s.chat_options.options[i] {
            dereth_ui_screens::options::chat::ChatOption::Filter(f) => f
                .children
                .iter()
                .map(|ch| (ch.label_token, ch.mask, ch.label.clone(), ch.element))
                .collect(),
            dereth_ui_screens::options::chat::ChatOption::Opacity(_) => {
                unreachable!("a filter is not an opacity")
            }
        }
    })
}

/// The control one window's rows live inside.
fn filter_control(c: &mut HeadlessClient, window: u32) -> ElemHandle {
    with_screen(c, |_, s| {
        let i = s
            .chat_options
            .filter_of(window)
            .expect("a filter for that window");
        match &s.chat_options.options[i] {
            dereth_ui_screens::options::chat::ChatOption::Filter(f) => f.element,
            dereth_ui_screens::options::chat::ChatOption::Opacity(_) => {
                unreachable!("a filter is not an opacity")
            }
        }
    })
}

/// What one chat window itself is showing, which is what a line is tested against.
fn window_filter(c: &mut HeadlessClient, window: u32) -> u64 {
    with_screen(c, |_, s| {
        s.chat
            .iter()
            .find(|w| w.window_id == window)
            .expect("the chat window")
            .filter
    })
}

/// Every run of letters the interface draws this frame.
fn drawn_runs(c: &mut HeadlessClient) -> Vec<String> {
    let mut back = dereth_ui::RecordingDrawBackend::default();
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .draw(&mut back);
    back.calls
        .into_iter()
        .map(|call| {
            call.glyphs
                .iter()
                .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Every chat window offers the same rows, and every row is drawn inside the control that owns it.
pub fn every_window_offers_the_same_filter_rows() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_chat_options_tab(&mut hand);

    let mut total = 0_usize;
    let mut holds = true;
    for w in FILTER_WINDOWS {
        // The main window is given one row fewer: the everything-else row is not offered for it.
        let expected: Vec<&(&str, u64, &str)> = if w == 8 {
            FILTER_ROWS[1..].iter().collect()
        } else {
            FILTER_ROWS.iter().collect()
        };
        let rows = filter_rows(&mut c, w);
        assert_eq!(
            rows.len(),
            expected.len(),
            "window {w} offers a different number of rows"
        );
        let control = filter_control(&mut c, w);
        let control_box = c
            .view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .screen_box(control);
        for (k, (token, mask, caption)) in expected.iter().enumerate() {
            let (got_token, got_mask, got_caption, element) = &rows[k];
            let h = element.unwrap_or_else(|| panic!("window {w} row {k} has no box"));
            let row_box = c
                .view()
                .expect_app()
                .ui()
                .expect("the shell")
                .ui
                .screen_box(h);
            // **The containment is the half that can break**: a control that was not made as tall
            // as its own rows leaves everything from the sixth down hanging outside it, where the
            // next section is drawn over them.
            let row_holds = got_token == token
                && got_mask == mask
                && got_caption.as_deref() == Some(*caption)
                && control_box.y0 <= row_box.y0
                && row_box.y1 <= control_box.y1;
            assert!(
                row_holds,
                "window {w} row {k} ({caption}) at {row_box:?} in {control_box:?}"
            );
            holds &= row_holds;
            total += 1;
        }
    }
    let all_of_them = total == 64;

    c.assert_behaviour(
        "chat.filters.every-window-offers-the-same-rows-and-each-row-lies-inside-its-control",
        move |_| holds && all_of_them,
    );
    c.shutdown();
}

#[test]
fn scenario_every_window_offers_the_same_filter_rows() {
    scenario("every_window_offers_the_same_filter_rows");
}

/// The global channels are rows of that same list, and they are on the screen -- which is a
/// different fact from being in the list.
pub fn the_global_channels_are_rows_of_that_list_and_are_drawn() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_chat_options_tab(&mut hand);

    let at_rest = drawn_runs(&mut c);
    // The first two are on screen the moment the tab comes up (the chat font's two rows above
    // the list push the others down); the rest are the last boxes of the main window's list and
    // sit just below what the page shows at rest.
    let mut drawn = true;
    for caption in ["General Channel", "Trade Channel"] {
        assert!(
            at_rest.iter().any(|s| s == caption),
            "{caption:?} is not drawn: {at_rest:?}"
        );
        drawn &= at_rest.iter().any(|s| s == caption);
    }

    // And the scrollbar reaches the rest of the list and the sections below it.
    let list = with_screen(&mut c, |_, s| {
        s.chat_options
            .option_box
            .as_ref()
            .expect("the page's list")
            .handle
    });
    dereth_ui::widgets::listbox::set_scroll_offset(
        &mut c.app_mut().ui_mut().expect("the shell is up").ui,
        list,
        0,
        200,
    );
    c.tick(1);
    let scrolled = drawn_runs(&mut c);
    let mut reachable = true;
    for caption in [
        "LFG Channel",
        "Roleplay Channel",
        "Society Channel",
        "Errors",
        "Gameplay",
    ] {
        assert!(
            scrolled.iter().any(|s| s == caption),
            "{caption:?} is not reachable by scrolling: {scrolled:?}"
        );
        reachable &= scrolled.iter().any(|s| s == caption);
    }

    c.assert_behaviour(
        "chat.filters.the-global-channels-are-rows-of-that-list-and-are-drawn",
        move |_| drawn && reachable,
    );
    c.shutdown();
}

#[test]
fn scenario_the_global_channels_are_rows_of_that_list_and_are_drawn() {
    scenario("the_global_channels_are_rows_of_that_list_and_are_drawn");
}

/// Ticking a row writes that window's own filter, and sends nothing: what a window shows is the
/// player's own setting and not something the shard is told about at once.
pub fn ticking_a_filter_row_writes_that_windows_own_filter() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_chat_options_tab(&mut hand);

    // The main window's rows are the thirteen less the first, so the general channel is the
    // seventh of them.
    let rows = filter_rows(&mut c, 8);
    let (token, mask, _, element) = rows[6].clone();
    let is_general = token == FILTER_ROWS[7].0 && mask == FILTER_ROWS[7].1;
    let boxh = element.expect("the general channel has a box");

    let before = window_filter(&mut c, 8);
    let on_to_start = before & mask == mask
        && dereth_ui_screens::bind::attr_bool(
            &c.view().expect_app().ui().expect("the shell").ui,
            boxh,
            ATTR_CHECKED,
        ) == Some(true);

    let mark_out = mark(&c);
    hand.click_handle(&mut c, boxh);
    let cleared = window_filter(&mut c, 8) & mask == 0
        && dereth_ui_screens::bind::attr_bool(
            &c.view().expect_app().ui().expect("the shell").ui,
            boxh,
            ATTR_CHECKED,
        ) == Some(false);

    hand.click_handle(&mut c, boxh);
    let restored = window_filter(&mut c, 8) == before;
    // Nothing went out: what a window shows is written locally and told to the shard later.
    let silent = c.view().outbound()[mark_out..].is_empty();

    c.assert_behaviour(
        "chat.filters.ticking-a-row-writes-that-windows-own-filter-and-sends-nothing",
        move |_| is_general && on_to_start && cleared && restored && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_ticking_a_filter_row_writes_that_windows_own_filter() {
    scenario("ticking_a_filter_row_writes_that_windows_own_filter");
}

// ---------------------------------------------------------------------------------------------
// The chat window itself: its entry, its font, and how solid it looks
// ---------------------------------------------------------------------------------------------

/// The main chat window's background picture -- a child of the window that owns no drawing surface
/// of its own, and therefore the first thing a change to the window's own has to reach.
const CHAT_BACKGROUND: ElementId = ElementId(0x1000_0010);
/// The stored setting for how solid an idle chat window is drawn.
const IDLE_OPACITY: u32 = 0x1000_0080;
/// A scrollbar's position along its length, which a slider reads.
const ATTR_POSITION: u32 = 0x86;
/// The fifth of the five chat font sizes, and the two fonts the first and the last resolve to.
const SIZE_LARGEST: i32 = 4;
const FONT_SMALL: dereth_primitives::DataId = dereth_primitives::DataId(0x4000_0000);
const FONT_LARGEST: dereth_primitives::DataId = dereth_primitives::DataId(0x4000_0005);

/// A client in gameplay with the player described.
fn a_client_with_a_described_player(
    module: dereth_protocol::login::PlayerModule,
) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.when(Inbound::event(SessionEvent::PlayerDescription(Box::new(
        dereth_protocol::login::LoginPlayerDescription {
            player_module: module,
            ..dereth_protocol::login::LoginPlayerDescription::default()
        },
    ))));
    c.tick(3);
    c
}

/// The caret the entry draws, the entry's own box, the box its letters go in, and how far the
/// letters have been slid along under it.
fn entry_caret(
    c: &mut HeadlessClient,
) -> (dereth_ui::Box2D, dereth_ui::Box2D, dereth_ui::Box2D, i32) {
    let entry = element(c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    let screen = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .screen_box(entry);
    with_screen(c, |ui, _| {
        let t = ui
            .text_element_mut(entry)
            .expect("the entry is a text element");
        (
            t.caret_box(screen)
                .expect("a box that can be typed in and has the caret draws one"),
            screen,
            t.content_box(screen),
            t.scroll.x,
        )
    })
}

/// Where the entry draws its `i`th letter, with the sliding already applied.
fn entry_glyph_x(c: &mut HeadlessClient, i: usize) -> i32 {
    let entry = element(c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    let screen = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .screen_box(entry);
    with_screen(c, |ui, _| {
        let t = ui.text_element_mut(entry).expect("the entry");
        t.compose(screen).get(i).expect("that letter is composed").x
    })
}

fn entry_text(c: &mut HeadlessClient) -> String {
    let entry = element(c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    with_screen(c, |ui, _| {
        ui.text_element_mut(entry)
            .map(|t| t.glyphs.inq_text(false))
            .unwrap_or_default()
    })
}

/// Typing more than fits slides the line along so the caret stays where the player can see it,
/// and the sliding stays put rather than being worked out again every frame.
pub fn typing_past_the_entrys_edge_keeps_the_caret_in_view() {
    let mut c = a_client_with_a_described_player(dereth_protocol::login::PlayerModule::default());
    let mut hand = Hand::new();
    let entry = element(&c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    hand.click_element(&mut c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    let focused = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .focus_element()
        == Some(entry);

    // An empty box is not slid along at all, and the caret is inside it.
    let (caret, _, content, slid) = entry_caret(&mut c);
    let at_rest = slid == 0 && caret.x0 >= content.x0 && caret.x1 <= content.x1;

    let typed = "the quick brown fox jumps over the lazy dog and keeps on running past the edge";
    hand.type_text(&mut c, typed);
    let every_key = entry_text(&mut c) == typed;

    let (caret, element_box, content, slid) = entry_caret(&mut c);
    // **Where the caret lands is arithmetic and not taste.** It is asked for two pixels past the
    // end of the line and the sliding is allowed two pixels less than that, so it comes to rest
    // one pixel into the right-hand margin -- inside the element, and drawn rather than trimmed.
    let in_view = slid > 0
        && caret.x0 >= element_box.x0
        && caret.x1 <= element_box.x1
        && caret.x1 - content.x1 == 1;
    // And the line really moved: its first letter is off the left edge now.
    let moved = entry_glyph_x(&mut c, 0) < content.x0;

    c.tick(2);
    let (caret, element_box, _, still) = entry_caret(&mut c);
    let stays = still == slid && caret.x0 >= element_box.x0 && caret.x1 <= element_box.x1;

    c.assert_behaviour(
        "chat.entry.typing-past-the-edge-slides-the-line-so-the-caret-stays-in-view",
        move |_| focused && at_rest && every_key && in_view && moved && stays,
    );
    c.shutdown();
}

#[test]
fn scenario_typing_past_the_entrys_edge_keeps_the_caret_in_view() {
    scenario("typing_past_the_entrys_edge_keeps_the_caret_in_view");
}

/// The font the log is drawn in, and the heights of the letters already in it.
fn log_font_and_heights(
    c: &mut HeadlessClient,
) -> (Option<dereth_primitives::DataId>, i32, Vec<i32>) {
    let log = element(c, dereth_ui_screens::chat::window::LOG);
    with_screen(c, |ui, _| {
        let t = ui.text_element_mut(log).expect("the log");
        let mut hs: Vec<i32> = t.glyphs.glyphs.iter().map(|g| g.height).collect();
        hs.sort_unstable();
        hs.dedup();
        (t.fonts.first().copied(), t.metrics.height(), hs)
    })
}

/// Put a line in the log the way the shard's own broadcast puts one there.
fn say_in_the_log(c: &mut HeadlessClient, text: &str) {
    let m = dereth_ui_screens::chat::interface::ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        window: 0,
        ty: 0,
        prefix: None,
        body: text.to_owned(),
    };
    let took = with_screen(c, |ui, s| s.recv_display_final_string_info(ui, &m));
    assert!(!took.is_empty(), "the line reached at least one window");
    c.tick(1);
}

/// Picking a different chat font size re-measures what is already in the log and the next line
/// that arrives.
pub fn changing_the_chat_font_size_remeasures_the_backlog() {
    let mut c = a_client_with_a_described_player(dereth_protocol::login::PlayerModule::default());
    let mut hand = Hand::new();

    say_in_the_log(&mut c, "a line of chat to measure");
    let (font, before_box, before_heights) = log_font_and_heights(&mut c);
    // The shipped setting and the layout's own font agree, which is why only a **change** to it is
    // observable at all.
    let starts_small = !before_heights.is_empty()
        && font == Some(FONT_SMALL)
        && dereth_ui_screens::options::store::inq_value("UI.ChatFontSize")
            == Some(dereth_ui_screens::view::PrefValue::Int(1));

    // The chat font's size is chosen on the chat options page.
    open_the_options_page(
        &mut c,
        &mut hand,
        dereth_ui_screens::options::chat::CHAT_PAGE_ELEMENT,
    );
    // Bring the row on to the screen the way the scrollbar does, then press it.
    let menu = {
        let i = with_screen(&mut c, |_, s| {
            s.config_page
                .options
                .iter()
                .position(|o| o.preference == "UI.ChatFontSize")
                .expect("the options window has a chat font size row")
        });
        with_screen(&mut c, |ui, s| {
            let row = s.config_page.options[i].row;
            if let Some(list) = s.chat_options.option_box.as_mut() {
                if let Some(k) = list.items.iter().position(|&h| h == row) {
                    list.scroll_to_view(ui, k);
                }
            }
        });
        c.tick(1);
        with_screen(&mut c, |_, s| s.config_page.options[i].element)
    };
    let popup = dereth_ui::widgets::menu::popup_handle(
        &c.view().expect_app().ui().expect("the shell").ui,
        menu,
    )
    .expect("the row carries a popup");
    let closed_to_start = !c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .is_visible(popup);
    hand.click_handle(&mut c, menu);
    let opened = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .is_visible(popup);
    #[allow(clippy::cast_sign_loss)]
    let row = dereth_ui::widgets::menu::get_item(
        &c.view().expect_app().ui().expect("the shell").ui,
        menu,
        SIZE_LARGEST as usize,
    )
    .expect("the largest size is a row of the list");
    hand.click_handle(&mut c, row);
    let picked = dereth_ui_screens::options::store::inq_value("UI.ChatFontSize")
        == Some(dereth_ui_screens::view::PrefValue::Int(SIZE_LARGEST));

    c.tick(1);
    let (font, after_box, after_heights) = log_font_and_heights(&mut c);
    let refonted = font == Some(FONT_LARGEST)
        && after_box > before_box
        && after_heights.iter().max() > before_heights.iter().max();

    // And the next line that arrives is measured with the new font as well.
    say_in_the_log(&mut c, "a second line");
    let (_, _, later) = log_font_and_heights(&mut c);
    let next_line_too = later == after_heights;

    c.assert_behaviour(
        "chat.log.changing-the-font-size-remeasures-the-backlog-and-the-next-line",
        move |_| starts_small && closed_to_start && opened && picked && refonted && next_line_too,
    );
    c.shutdown();
}

#[test]
fn scenario_changing_the_chat_font_size_remeasures_the_backlog() {
    scenario("changing_the_chat_font_size_remeasures_the_backlog");
}

/// How solid the surface the chat window composes into is drawn.
fn chat_window_opacity(c: &mut HeadlessClient) -> f32 {
    let root = with_screen(c, |_, s| s.chat_windows.first().and_then(|w| w.root))
        .expect("the main chat window is bound");
    c.view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .material_opacity(root)
}

/// How solid one element's own drawing is, off the draw list.
fn drawn_alpha(c: &mut HeadlessClient, id: ElementId) -> f32 {
    let h = element(c, id);
    let mut back = dereth_ui::RecordingDrawBackend::default();
    with_screen(c, |ui, _| ui.draw(&mut back));
    back.calls
        .iter()
        .find(|call| call.who == h)
        .unwrap_or_else(|| panic!("{id:?} drew nothing at all"))
        .alpha_blend_mod
}

/// Dragging the idle-opacity slider fades the chat window at once, and a stored value is worn from
/// the first frame after login rather than crept towards.
pub fn the_idle_opacity_fades_the_chat_window_at_once() {
    let mut c = a_client_with_a_described_player(dereth_protocol::login::PlayerModule::default());
    let mut hand = Hand::new();
    // No shipped chat window says how solid it should be, so they all come up solid.
    let solid_to_start = (chat_window_opacity(&mut c) - 1.0).abs() < 1e-6
        && (drawn_alpha(&mut c, CHAT_BACKGROUND) - 1.0).abs() < 1e-6;

    open_the_options_page(
        &mut c,
        &mut hand,
        dereth_ui_screens::options::chat::CHAT_PAGE_ELEMENT,
    );
    // Bring the slider on to the screen and press it near its left-hand end.
    let bar = {
        let i = with_screen(&mut c, |_, s| {
            s.chat_options
                .slider_of(IDLE_OPACITY)
                .expect("the page has the idle slider")
        });
        with_screen(&mut c, |ui, s| {
            let row = match &s.chat_options.options[i] {
                dereth_ui_screens::options::chat::ChatOption::Opacity(o) => o.row,
                dereth_ui_screens::options::chat::ChatOption::Filter(f) => f.element,
            };
            let list = s.chat_options.option_box.as_mut().expect("the page's list");
            if let Some(k) = list.items.iter().position(|&h| h == row) {
                list.scroll_to_view(ui, k);
            }
        });
        c.tick(1);
        with_screen(&mut c, |_, s| match &s.chat_options.options[i] {
            dereth_ui_screens::options::chat::ChatOption::Opacity(o) => o.element,
            dereth_ui_screens::options::chat::ChatOption::Filter(_) => {
                unreachable!("a slider is not a filter")
            }
        })
    };
    let (x, y) = {
        let ui = &c.view().expect_app().ui().expect("the shell").ui;
        let b = ui.screen_box(bar);
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let x = b.x0 + ((b.x1 - b.x0) as f32 * 0.1) as i32;
        let y = (b.y0 + b.y1) / 2;
        let hit = ui.hit_test_screen(x, y);
        assert!(
            hit.is_some_and(|h| h == bar || ui.parent(h) == Some(bar)),
            "the pointer must land on the bar or its thumb, got {hit:?}"
        );
        (x, y)
    };
    hand.press_at(&mut c, x, y);
    let pos = dereth_ui_screens::bind::attr_float(
        &c.view().expect_app().ui().expect("the shell").ui,
        bar,
        ATTR_POSITION,
    )
    .unwrap_or(-1.0);
    let lowered = (0.0..0.3).contains(&pos);

    // **At once, not twenty frames later**: the window is idle, so the new value is pushed
    // straight through rather than crept towards.
    let (stored, current) = with_screen(&mut c, |_, s| {
        let w = s.chat.first().expect("the main window");
        (w.default_opacity, w.current_opacity)
    });
    let straight_through = (stored - pos).abs() < 1e-5
        && (current - pos).abs() < 1e-5
        && (chat_window_opacity(&mut c) - pos).abs() < 1e-5;
    // The half a player sees: the window's background, which owns no surface of its own and so
    // carries the window's.
    let background = (drawn_alpha(&mut c, CHAT_BACKGROUND) - pos).abs() < 1e-5;
    // **And it stops where the client stops it.** The log owns a surface of its own, so it is not
    // faded with the window around it -- a fading that ran to every descendant regardless would
    // look prettier and would not be what the client does.
    let stops_at_the_log =
        {
            let log = element(&c, dereth_ui_screens::chat::window::LOG);
            let ui = &c.view().expect_app().ui().expect("the shell").ui;
            ui.node(log).is_some_and(|n| n.flags.should_own_object())
                && ui
                    .node(log)
                    .and_then(|n| n.merged_properties().get_enum(0xCD))
                    == Some(3)
        } && (drawn_alpha(&mut c, dereth_ui_screens::chat::window::LOG) - 1.0).abs() < 1e-6;
    c.shutdown();

    // A client whose player arrives already carrying a stored value wears it on the first frame,
    // rather than fading towards it.
    let mut c = a_client_with_a_described_player(dereth_protocol::login::PlayerModule {
        gameplay_options: Some(dereth_protocol::property::PackObjPropertyCollection {
            version: 2,
            properties: dereth_protocol::property::PropertyCollection {
                bucket_index: 4,
                entries: vec![(
                    IDLE_OPACITY,
                    dereth_protocol::property::BaseProperty {
                        name: IDLE_OPACITY,
                        value: Some(dereth_protocol::property::BasePropertyValue::Float(0.25)),
                    },
                )],
            },
        }),
        ..dereth_protocol::login::PlayerModule::default()
    });
    let at_login = (chat_window_opacity(&mut c) - 0.25).abs() < 1e-5
        && (drawn_alpha(&mut c, CHAT_BACKGROUND) - 0.25).abs() < 1e-5;

    c.assert_behaviour(
        "chat.window.how-solid-it-is-follows-the-slider-at-once-and-is-worn-from-login",
        move |_| {
            solid_to_start
                && lowered
                && straight_through
                && background
                && stops_at_the_log
                && at_login
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_idle_opacity_fades_the_chat_window_at_once() {
    scenario("the_idle_opacity_fades_the_chat_window_at_once");
}

// ---------------------------------------------------------------------------------------------
// Where the caret goes after a line is sent
// ---------------------------------------------------------------------------------------------
//
// **Both directions, every time.** Asserting only "the entry has lost the caret" passes on a build
// where it can never have it at all, so each arm below asks where a keystroke goes on **both**
// sides of the edge: with the box blurred a movement key walks the character, and with it focused
// the same character lands in the box.

/// The key the host resolves a named key to.
fn key_of(code: winit::keyboard::KeyCode) -> dereth_client::platform::keys::Key {
    dereth_client::platform::window::key_from_key_code(code)
        .expect("the host has a scan code for this key")
}

fn focus_of(c: &HeadlessClient) -> Option<ElemHandle> {
    c.view()
        .expect_app()
        .ui()
        .expect("the shell is up")
        .ui
        .focus_element()
}

fn characters_delivered(c: &HeadlessClient) -> u64 {
    c.view()
        .expect_app()
        .ui()
        .expect("the shell is up")
        .stats
        .characters_delivered
}

/// The main chat window's own history, which is the client's evidence that a line really left the
/// box: an empty entry is not remembered, so a line in the history is a line that was sent.
fn chat_history(c: &mut HeadlessClient) -> Vec<String> {
    c.view()
        .world()
        .chat
        .entries
        .get(&8)
        .map_or_else(Vec::new, |entry| entry.history().to_vec())
}

/// Type `line` one character at a time, checking every one arrived, so a send is never asserted
/// over a line that silently failed to be typed.
fn type_checked(c: &mut HeadlessClient, hand: &mut Hand, entry: ElemHandle, line: &str) {
    let before = characters_delivered(c);
    for ch in line.chars() {
        hand.character(c, ch);
        c.tick(1);
    }
    assert_eq!(
        characters_delivered(c),
        before + line.chars().count() as u64,
        "every character of {line:?} reached the box"
    );
    let text = with_screen(c, |ui, _| {
        ui.text_element_mut(entry)
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    });
    assert_eq!(text, line, "and the box holds the whole line");
}

fn entry_contents(c: &mut HeadlessClient, entry: ElemHandle) -> String {
    with_screen(c, |ui, _| {
        ui.text_element_mut(entry)
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    })
}

/// Pressing return sends the line, empties the box and gives the keyboard back to the world -- and
/// nothing puts the caret straight back a frame later.
pub fn the_return_key_sends_the_line_and_gives_the_keyboard_back() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    let enter = key_of(winit::keyboard::KeyCode::Enter);

    let nothing_yet = focus_of(&c).is_none();
    hand.tap(&mut c, enter);
    let opened = focus_of(&c) == Some(entry);
    // The character that key press also produces, which the box is told to ignore.
    hand.character(&mut c, char::from(0x0D_u8));
    c.tick(1);

    type_checked(&mut c, &mut hand, entry, "hail");
    let nothing_sent = chat_history(&mut c).is_empty();

    let had_it = focus_of(&c) == Some(entry);
    hand.tap(&mut c, enter);
    let gave_it_back = focus_of(&c).is_none();
    let emptied = entry_contents(&mut c, entry).is_empty();
    let sent = chat_history(&mut c) == vec!["hail".to_owned()];

    // With the box blurred the keyboard is the player's again: a movement key walks him, and a
    // character reaches nothing.
    let before = characters_delivered(&c);
    let w = key_of(winit::keyboard::KeyCode::KeyW);
    hand.key(&mut c, w, true);
    c.tick(1);
    let walks = c.view().expect_app().probe().char_input().forward;
    hand.key(&mut c, w, false);
    c.tick(1);
    hand.character(&mut c, 'q');
    c.tick(1);
    let nothing_typed =
        characters_delivered(&c) == before && entry_contents(&mut c, entry).is_empty();

    // And with the box open again the same character lands in it.
    hand.tap(&mut c, enter);
    let reopened = focus_of(&c) == Some(entry);
    hand.character(&mut c, char::from(0x0D_u8));
    c.tick(1);
    hand.character(&mut c, 'q');
    c.tick(1);
    let typed_now = characters_delivered(&c) == before + 1 && entry_contents(&mut c, entry) == "q";

    c.assert_behaviour(
        "chat.entry.the-return-key-sends-the-line-and-gives-the-keyboard-back",
        move |_| {
            nothing_yet
                && opened
                && nothing_sent
                && had_it
                && gave_it_back
                && emptied
                && sent
                && walks
                && nothing_typed
                && reopened
                && typed_now
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_return_key_sends_the_line_and_gives_the_keyboard_back() {
    scenario("the_return_key_sends_the_line_and_gives_the_keyboard_back");
}

/// Pressing the send button sends the line and keeps the caret for itself, so the player has to
/// press the box again before he can type the next one.
///
/// It is asserted as "the button has it" rather than "nobody has it": those are two different
/// clients and only one of them is this one.
pub fn the_send_button_keeps_the_caret_for_itself() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    let send = element(&c, dereth_ui_screens::chat::window::SEND);

    hand.click_handle(&mut c, entry);
    let focused = focus_of(&c) == Some(entry);
    type_checked(&mut c, &mut hand, entry, "hail");

    let had_it = focus_of(&c) == Some(entry);
    hand.click_handle(&mut c, send);
    let button_has_it = focus_of(&c) == Some(send);
    let emptied = entry_contents(&mut c, entry).is_empty();
    let sent = chat_history(&mut c) == vec!["hail".to_owned()];

    // A button is not something that can be typed in, so nothing reaches anything.
    let before = characters_delivered(&c);
    hand.character(&mut c, 'q');
    c.tick(1);
    let nothing_typed =
        characters_delivered(&c) == before && entry_contents(&mut c, entry).is_empty();

    // The press the player has to make.
    hand.click_handle(&mut c, entry);
    let back = focus_of(&c) == Some(entry);
    hand.character(&mut c, 'q');
    c.tick(1);
    let typed_now = characters_delivered(&c) == before + 1 && entry_contents(&mut c, entry) == "q";

    c.assert_behaviour(
        "chat.entry.the-send-button-keeps-the-caret-for-itself",
        move |_| {
            focused
                && had_it
                && button_has_it
                && emptied
                && sent
                && nothing_typed
                && back
                && typed_now
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_send_button_keeps_the_caret_for_itself() {
    scenario("the_send_button_keeps_the_caret_for_itself");
}

// ---------------------------------------------------------------------------------------------
// Typing a run, and the two things that made one character overwrite the last
// ---------------------------------------------------------------------------------------------

/// Whether the client is about to eat the next character it is offered.
fn eats_the_next_character(c: &mut HeadlessClient) -> bool {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell is part of the shell")
        .manager
        .text
        .ignore_next_char
}

/// How many key presses the interface has been given -- one per dispatch, so an event that is put
/// back and offered again shows up here as growth on a frame with no key on it.
fn key_presses(c: &HeadlessClient) -> u64 {
    c.view()
        .expect_app()
        .ui()
        .expect("the shell is up")
        .stats
        .key_presses
}

/// Events offered to every part of a frame, claimed by none, and therefore dropped.
fn actions_expired(c: &mut HeadlessClient) -> u64 {
    c.app_mut().actions.stats().expired
}

/// Type `run` one character per frame into `target`, checking **after every character** that the
/// box holds everything typed so far and that exactly one more character arrived.
///
/// A run rather than one character, and a check after each rather than at the end, because the
/// first character always worked and it was the second that threw the first away: a scenario that
/// read only the final text would see one letter where the fault is "four were deleted".
fn type_a_run(c: &mut HeadlessClient, hand: &mut Hand, target: ElemHandle, run: &str) -> bool {
    let mut expected = entry_contents_of(c, target);
    let mut holds = true;
    for (i, ch) in run.chars().enumerate() {
        let before = characters_delivered(c);
        hand.character(c, ch);
        c.tick(1);
        expected.push(ch);
        let one = characters_delivered(c) == before + 1;
        let whole = entry_contents_of(c, target) == expected;
        assert!(
            one && whole,
            "character {i} ({ch:?}): arrived={one} whole={whole}"
        );
        holds &= one && whole;
    }
    holds
}

fn entry_contents_of(c: &mut HeadlessClient, h: ElemHandle) -> String {
    with_screen(c, |ui, _| {
        ui.text_element_mut(h)
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    })
}

/// A whole run of characters arrives in the chat entry, in order, whichever way the box was
/// opened -- none of them throws away what came before.
pub fn a_run_of_characters_all_arrives_in_the_entry() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    let enter = key_of(winit::keyboard::KeyCode::Enter);

    // The key route: the key opens the box and arms the client to eat the character that same key
    // press also produces.
    hand.key(&mut c, enter, true);
    c.tick(1);
    hand.key(&mut c, enter, false);
    c.tick(1);
    let opened = focus_of(&c) == Some(entry) && eats_the_next_character(&mut c);
    let offered_at = characters_delivered(&c);
    hand.character(&mut c, char::from(0x0D_u8));
    c.tick(1);
    // Eaten exactly once, and then the client is hungry no more.
    let eaten = characters_delivered(&c) == offered_at && !eats_the_next_character(&mut c);

    let by_key = type_a_run(&mut c, &mut hand, entry, "hello")
        && entry_contents_of(&mut c, entry) == "hello"
        && characters_delivered(&c) == offered_at + 5;

    // The pointer route, in the same client, so the two halves differ only in how the box was
    // given the caret.
    {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        ui.text_element_mut(entry)
            .expect("a text element")
            .set_text("");
        ui.relinquish_focus(entry);
    }
    c.tick(1);
    let blurred = focus_of(&c).is_none();
    hand.click_handle(&mut c, entry);
    let clicked = focus_of(&c) == Some(entry) && !eats_the_next_character(&mut c);
    let before = characters_delivered(&c);
    let by_pointer = type_a_run(&mut c, &mut hand, entry, "world")
        && entry_contents_of(&mut c, entry) == "world"
        && characters_delivered(&c) == before + 5;

    c.assert_behaviour(
        "chat.entry.a-run-of-characters-all-arrives-and-none-overwrites-the-last",
        move |_| opened && eaten && by_key && blurred && clicked && by_pointer,
    );
    c.shutdown();
}

#[test]
fn scenario_a_run_of_characters_all_arrives_in_the_entry() {
    scenario("a_run_of_characters_all_arrives_in_the_entry");
}

/// A key press nobody claims is handed round once and then thrown away, rather than being offered
/// again on every frame for ever.
pub fn an_action_nobody_claims_is_dispatched_once() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);

    // A quiet stretch first, so that "it does not grow" is a claim about idleness rather than
    // about a counter nothing writes: it has to grow over the key press below.
    let quiet = key_presses(&c);
    c.tick(5);
    let still_quiet = key_presses(&c) == quiet;

    let expired_before = actions_expired(&mut c);
    hand.tap(&mut c, key_of(winit::keyboard::KeyCode::Enter));
    let arrived = focus_of(&c) == Some(entry) && key_presses(&c) == quiet + 1;
    // And it was claimed by nobody, so it was dropped -- which is what makes the silence below a
    // measurement rather than an empty queue that never had anything in it.
    let dropped = actions_expired(&mut c) > expired_before;

    let after = key_presses(&c);
    let mut silent = true;
    for i in 0..8 {
        c.tick(1);
        let same = key_presses(&c) == after;
        assert!(same, "idle frame {i}: the press was offered again");
        silent &= same;
    }

    c.assert_behaviour(
        "chat.input.a-press-nobody-claims-is-handed-round-once-and-then-dropped",
        move |_| still_quiet && arrived && dropped && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_an_action_nobody_claims_is_dispatched_once() {
    scenario("an_action_nobody_claims_is_dispatched_once");
}

/// The client is set to eat the next character on exactly one edge -- the key press that opens a
/// box -- and never otherwise, so no ordinary keystroke is swallowed.
pub fn the_eat_the_next_character_latch_is_armed_on_one_edge() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);

    // A press of the pointer gives the box the caret and arms nothing: a client that armed it
    // every time would eat the first character of every box a player clicks into.
    hand.click_handle(&mut c, entry);
    let clicked = focus_of(&c) == Some(entry) && !eats_the_next_character(&mut c);

    // And it stays unarmed right through a run.
    let mut through_a_run = true;
    for (i, ch) in "abcd".chars().enumerate() {
        let before = characters_delivered(&c);
        hand.character(&mut c, ch);
        c.tick(1);
        let holds = characters_delivered(&c) == before + 1 && !eats_the_next_character(&mut c);
        assert!(holds, "character {i}");
        through_a_run &= holds;
    }
    let typed = entry_contents_of(&mut c, entry) == "abcd";

    // The one edge that does arm it: a key press that turns typing on.
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .relinquish_focus(entry);
    c.tick(1);
    let losing_it_arms_nothing = !eats_the_next_character(&mut c);
    let enter = key_of(winit::keyboard::KeyCode::Enter);
    hand.key(&mut c, enter, true);
    c.tick(1);
    hand.key(&mut c, enter, false);
    c.tick(1);
    let armed = eats_the_next_character(&mut c);

    // And exactly one character pays for it.
    let before = characters_delivered(&c);
    hand.character(&mut c, char::from(0x0D_u8));
    c.tick(1);
    let ate_one = characters_delivered(&c) == before;
    hand.character(&mut c, 'z');
    c.tick(1);
    let only_one = characters_delivered(&c) == before + 1;

    c.assert_behaviour(
        "chat.input.the-client-is-set-to-eat-a-character-on-one-edge-and-never-otherwise",
        move |_| {
            clicked
                && through_a_run
                && typed
                && losing_it_arms_nothing
                && armed
                && ate_one
                && only_one
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_eat_the_next_character_latch_is_armed_on_one_edge() {
    scenario("the_eat_the_next_character_latch_is_armed_on_one_edge");
}

// ---------------------------------------------------------------------------------------------
// A line broadcast on a channel, end to end and in its own colour
// ---------------------------------------------------------------------------------------------

/// An opaque colour, and the colour a clickable name is drawn in.
const OPAQUE: u32 = 0xFF00_0000;
const TAG_COLOUR: u32 = 0xFF00_B200;

/// The log's letters as runs of one colour -- what a player actually sees, read off the element
/// rather than off the colour table.
fn colour_runs(c: &mut HeadlessClient) -> Vec<(String, u32)> {
    let log = element(c, dereth_ui_screens::chat::window::LOG);
    with_screen(c, |ui, _| {
        let t = ui.text_element_mut(log).expect("the log is a text element");
        let mut out: Vec<(String, u32)> = Vec::new();
        for g in &t.glyphs.glyphs {
            let ch = char::from_u32(u32::from(g.data)).unwrap_or('\u{FFFD}');
            match out.last_mut() {
                Some((s, col)) if *col == g.color => s.push(ch),
                _ => out.push((ch.to_string(), g.color)),
            }
        }
        out
    })
}

/// The whole drawn log, and the colour of the run carrying `needle`.
fn drawn_line(c: &mut HeadlessClient, needle: &str) -> (String, Option<u32>) {
    let all = colour_runs(c);
    let joined: String = all.iter().map(|(s, _)| s.as_str()).collect();
    let colour = all
        .iter()
        .find(|(s, _)| s.contains(needle))
        .map(|(_, col)| *col);
    (joined, colour)
}

/// The clickable-name markup is taken off the letters and hung on them, so what is drawn is the
/// line without it.
fn without_tell_markup(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        if let Some(close) = after.find('>') {
            let tag = &after[1..close];
            if tag.starts_with("Tell:") || tag == "\\Tell" {
                rest = &after[close + 1..];
                continue;
            }
        }
        out.push('<');
        rest = &after[1..];
    }
    out.push_str(rest);
    out
}

/// Hand one broadcast to the client and let it reach the drawn log.
///
/// The line sits in the client's own pending list until the **next** frame, because what moves it
/// into the window is part of the frame and not part of taking the message, so the frame below is
/// load-bearing rather than a settling loop. What the client composed is answered, so the model
/// and the letters are asserted apart.
fn hear_on_a_channel(
    c: &mut HeadlessClient,
    channel: u32,
    sender: &str,
    message: &str,
) -> Vec<dereth_ui_screens::chat::interface::ChatMessage> {
    use dereth_protocol::Message as _;
    let m = dereth_protocol::comms::CommunicationChannelBroadcastRecv {
        channel,
        sender_name: sender.to_owned(),
        message: message.to_owned(),
    };
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(dereth_protocol::Opcode::COMMUNICATION_CHANNEL_BROADCAST.0);
    m.write(&mut w).expect("the message encodes");
    let composed = c.app_mut().apply_hud_events(&[SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode::COMMUNICATION_CHANNEL_BROADCAST,
        blob: w.into_inner(),
    }]);
    c.tick(1);
    composed
}

/// How many channel lines the client has composed and how many it has refused. The pair tells a
/// line dropped at the gate apart from a line the client never read at all, which a missing line
/// on the log cannot.
fn channel_counts(c: &HeadlessClient) -> (u64, u64) {
    let s = &c.view().expect_app().hud().stats;
    (
        s.channel_broadcast_lines_composed,
        s.channel_broadcast_lines_squelched,
    )
}

/// The first recorded broadcast that names its speaker, found by walking the recordings.
fn a_recorded_named_broadcast() -> (u32, String, String) {
    use dereth_client_net::client_session::testing::Direction;
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient || b.opcode != 0xF7B0 || b.payload.len() < 16 {
                continue;
            }
            if u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes"))
                != dereth_protocol::Opcode::COMMUNICATION_CHANNEL_BROADCAST.0
            {
                continue;
            }
            let Ok(m) = dereth_protocol::read_body::<
                dereth_protocol::comms::CommunicationChannelBroadcastRecv,
            >(&b.payload[16..]) else {
                continue;
            };
            if !m.sender_name.is_empty() {
                return (m.channel, m.sender_name, m.message);
            }
        }
    }
    panic!("the corpus carries a recorded broadcast that names its speaker");
}

/// A speaker's name, lifted off a recorded spoken line so that the name is a shard's and not this
/// file's.
fn a_recorded_speaker_name() -> String {
    use dereth_client_net::client_session::testing::Direction;
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient
                || b.opcode != dereth_protocol::Opcode::COMMUNICATION_HEAR_SPEECH.0
            {
                continue;
            }
            let Ok(m) = dereth_protocol::read_body::<dereth_protocol::comms::CommunicationHearSpeech>(
                &b.payload[4..],
            ) else {
                continue;
            };
            if !m.sender_name.is_empty() && m.sender_name.is_ascii() {
                return m.sender_name;
            }
        }
    }
    panic!("the corpus carries a recorded speaker's name");
}

/// A client in the world with its player described, ready to be spoken to.
fn a_client_listening() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    // A client that is in the world has had a description of its player, and the chat windows take
    // their stored settings off it.
    c.when(Inbound::event(SessionEvent::PlayerDescription(
        Box::default(),
    )));
    c.tick(1);
    c
}

/// A broadcast a shard really sent, naming its speaker, reaches the log -- the name drawn as
/// something to click and the rest of the line in the colour that kind of line is drawn in.
pub fn a_recorded_broadcast_that_names_its_speaker_is_drawn() {
    let (channel, sender, message) = a_recorded_named_broadcast();
    let (ty, line) = dereth_client::chat::channel_broadcast_line(channel, &sender, &message);
    let trimmed = dereth_client::chat::add_text_to_scroll_trim(&line);
    let visible = without_tell_markup(&trimmed);

    let mut c = a_client_listening();
    let before = colour_runs(&mut c).len();
    let composed = hear_on_a_channel(&mut c, channel, &sender, &message);
    let one_line =
        composed.len() == 1 && composed[0].body == trimmed && u32::from(composed[0].ty) == ty;

    let runs = colour_runs(&mut c);
    let reached = runs.len() > before;
    let (joined, colour) = drawn_line(&mut c, &message);
    let drawn = joined.contains(&visible) && !joined.contains("<Tell:IIDString:");
    // The colour, apart from the letters: the body is drawn in this kind of line's own colour and
    // the speaker's name in the one a clickable name is drawn in.
    let want = OPAQUE
        | dereth_ui_screens::chat::colors::color_for_type(
            u8::try_from(ty).expect("a kind of line"),
        )
        .hex;
    let body_colour = colour == Some(want);
    let name_run = colour_runs(&mut c)
        .into_iter()
        .find(|(s, _)| s.contains(&sender) && !s.contains("says"))
        .map(|(_, col)| col);
    let name_colour = name_run == Some(TAG_COLOUR);

    c.assert_behaviour(
        "chat.channel.a-recorded-broadcast-that-names-its-speaker-is-drawn-in-its-own-colour",
        move |_| one_line && reached && drawn && body_colour && name_colour,
    );
    c.shutdown();
}

#[test]
fn scenario_a_recorded_broadcast_that_names_its_speaker_is_drawn() {
    scenario("a_recorded_broadcast_that_names_its_speaker_is_drawn");
}

/// Every channel a player speaks on draws its own line on the log in its own kind's colour -- and
/// the colours are not all one colour, or a table that answered the same thing for everything
/// would satisfy every row of it.
pub fn every_channel_draws_its_own_line_in_its_own_colour() {
    use dereth_client_model::chat::text_type;

    let who = a_recorded_speaker_name();
    // (the channel, the word this line is known by here, the kind of line it is)
    let cases: [(u32, &str, u32); 9] = [
        (0x0000_0800, "fellowship", text_type::FELLOWSHIP),
        (0x0000_1000, "vassals", text_type::SOCIAL),
        (0x0000_2000, "patron", text_type::SOCIAL),
        (0x0000_4000, "monarch", text_type::SOCIAL),
        (0x0100_0000, "covassals", text_type::SOCIAL),
        (0x0200_0000, "allegiance", text_type::SOCIAL),
        (0x0000_0400, "help", text_type::HELP),
        (0x0000_0001, "abuse", text_type::ABUSE),
        (0x0000_0080, "unknown", text_type::CHANNEL),
    ];

    let mut c = a_client_listening();
    let mut every = true;
    for (channel, word, want_ty) in cases {
        // A body unique to this channel, so a lookup cannot read another line's run.
        let message = format!("channel {word} body");
        let (ty, line) = dereth_client::chat::channel_broadcast_line(channel, &who, &message);
        let kind = ty == want_ty;
        let before = colour_runs(&mut c).len();
        hear_on_a_channel(&mut c, channel, &who, &message);
        let visible = without_tell_markup(&dereth_client::chat::add_text_to_scroll_trim(&line));
        let reached = colour_runs(&mut c).len() > before;
        let (joined, colour) = drawn_line(&mut c, &message);
        let want = OPAQUE
            | dereth_ui_screens::chat::colors::color_for_type(
                u8::try_from(ty).expect("a kind of line"),
            )
            .hex;
        let holds = kind
            && reached
            && joined.contains(&visible)
            && colour == Some(want)
            && !joined.contains("<Tell:IIDString:")
            && colour_runs(&mut c)
                .iter()
                .any(|(s, col)| s.contains(&who) && *col == TAG_COLOUR);
        assert!(holds, "channel {channel:#x} ({word})");
        every &= holds;
    }

    // Four distinct colours across the nine. Two of the kinds share one, which is the client's own
    // answer and is asserted as such rather than worked around.
    let fellow = drawn_line(&mut c, "channel fellowship body")
        .1
        .expect("the fellowship line");
    let social = drawn_line(&mut c, "channel vassals body")
        .1
        .expect("the vassals line");
    let help = drawn_line(&mut c, "channel help body")
        .1
        .expect("the help line");
    let abuse = drawn_line(&mut c, "channel abuse body")
        .1
        .expect("the abuse line");
    let other = drawn_line(&mut c, "channel unknown body")
        .1
        .expect("the unknown-channel line");
    let distinct: std::collections::BTreeSet<u32> =
        [fellow, social, help, abuse, other].into_iter().collect();
    let four = fellow == social && distinct.len() == 4;

    c.assert_behaviour(
        "chat.channel.every-channel-draws-its-own-line-in-its-own-colour",
        move |_| every && four,
    );
    c.shutdown();
}

#[test]
fn scenario_every_channel_draws_its_own_line_in_its_own_colour() {
    scenario("every_channel_draws_its_own_line_in_its_own_colour");
}

/// Silencing the speaker does not silence a channel line, and silencing the kind of line does --
/// but only for a kind the client will answer about at all.
pub fn a_squelched_speaker_is_still_heard_on_a_channel() {
    use dereth_client_model::chat::text_type;

    let who = a_recorded_speaker_name();
    let mut c = a_client_listening();

    // The speaker, silenced on everything there is and on his account too. A channel line carries
    // no speaker's id, so the strongest form of "this one is silenced" cannot reach the question
    // the client asks -- and that is the claim.
    {
        let mut entry = dereth_client_model::chat::SquelchEntry {
            name: who.clone(),
            ..Default::default()
        };
        entry.squelch_everything();
        let squelch = &mut c.world_mut().chat.squelch;
        squelch.characters.insert(ObjectId(0x5000_0099), entry);
        squelch.accounts.insert(who.clone(), 0);
    }
    hear_on_a_channel(&mut c, 0x0000_0800, &who, "channel speaker silenced");
    let still_heard = drawn_line(&mut c, "channel speaker silenced")
        .0
        .contains("channel speaker silenced");

    // The kind of line, silenced: a fellowship line is one the client will answer about, and it is
    // dropped before it is composed.
    c.world_mut()
        .chat
        .squelch
        .global
        .types
        .insert(text_type::FELLOWSHIP);
    let before = channel_counts(&c);
    hear_on_a_channel(&mut c, 0x0000_0800, &who, "channel kind silenced");
    let after = channel_counts(&c);
    let dropped = (after.0 - before.0, after.1 - before.1) == (0, 1)
        && !drawn_line(&mut c, "channel kind silenced")
            .0
            .contains("channel kind silenced");

    // The same table, a kind the client will not answer about: the question is refused before it
    // reaches the table, so the line is drawn.
    c.world_mut()
        .chat
        .squelch
        .global
        .types
        .insert(text_type::SOCIAL);
    let before = channel_counts(&c);
    hear_on_a_channel(&mut c, 0x0200_0000, &who, "channel kind not asked");
    let after = channel_counts(&c);
    let still_drawn = (after.0 - before.0, after.1 - before.1) == (1, 0)
        && drawn_line(&mut c, "channel kind not asked")
            .0
            .contains("channel kind not asked");

    c.assert_behaviour(
        "chat.channel.silencing-the-speaker-does-not-silence-a-channel-line",
        move |_| still_heard && dropped && still_drawn,
    );
    c.shutdown();
}

#[test]
fn scenario_a_squelched_speaker_is_still_heard_on_a_channel() {
    scenario("a_squelched_speaker_is_still_heard_on_a_channel");
}

// ---------------------------------------------------------------------------------------------
// The three death lines, on the log
// ---------------------------------------------------------------------------------------------
//
// The model half of two of these -- that somebody else's death is announced and your own is not --
// is `chat.death.a-third-partys-death-is-announced-and-your-own-is-not` in the cpu tier. What is
// here is the half that needs the shipped window: the lines really reach the log a player reads,
// in the colour a plain line is drawn in.

/// The player this section is, and the two other people in it.
const DEAD_LOCAL: ObjectId = ObjectId(0x5000_0001);
const DEAD_VICTIM: ObjectId = ObjectId(0x5000_0011);
const DEAD_KILLER: ObjectId = ObjectId(0x5000_0012);

/// The colour a plain line is drawn in.
fn plain_line_colour() -> u32 {
    OPAQUE | dereth_ui_screens::chat::colors::color_for_type(0).hex
}

/// A client in the world, with a player of its own, ready to be told about a death.
fn a_client_that_can_die() -> HeadlessClient {
    let mut c = a_client_listening();
    c.world_mut().player = Some(DEAD_LOCAL);
    c
}

/// The five counts the two arms keep, so a line refused at a guard is told apart from a message
/// that never read at all -- which a missing line on the log cannot do.
fn death_counts(c: &HeadlessClient) -> (u64, u64, u64, u64, u64) {
    let s = &c.view().expect_app().hud().stats;
    (
        s.player_deaths,
        s.player_deaths_announced,
        s.victim_notifications,
        s.victim_notifications_announced,
        s.scroll_lines,
    )
}

/// Hand one message to the client and let it reach the drawn log.
///
/// Both of the arms below end by putting a line on the client's own scroll, which is drained at the
/// **head** of the next batch -- so the empty second call is the client's own ordering rather than
/// a settling loop, and the frame then carries the line into the window.
fn hear_a_death(
    c: &mut HeadlessClient,
    e: SessionEvent,
) -> Vec<dereth_ui_screens::chat::interface::ChatMessage> {
    c.app_mut().apply_hud_events(&[e]);
    let composed = c.app_mut().apply_hud_events(&[]);
    c.tick(1);
    composed
}

/// One announcement that somebody died.
fn death_event(message: &str, killed: ObjectId, killer: ObjectId) -> SessionEvent {
    use dereth_protocol::Message as _;
    let m = dereth_protocol::combat::CombatHandlePlayerDeathEvent {
        message: message.to_owned(),
        killed,
        killer,
    };
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(dereth_protocol::Opcode::COMBAT_HANDLE_PLAYER_DEATH_EVENT.0);
    m.write(&mut w).expect("the message encodes");
    SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode::COMBAT_HANDLE_PLAYER_DEATH_EVENT,
        blob: w.into_inner(),
    }
}

/// One notification that you died, or that you killed something.
fn notification_event(opcode: dereth_protocol::Opcode, message: &str) -> SessionEvent {
    use dereth_protocol::Message as _;
    let m = dereth_protocol::combat::VictimNotificationOther {
        message: message.to_owned(),
    };
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(opcode.0);
    m.write(&mut w).expect("the message encodes");
    SessionEvent::UiEvent {
        opcode,
        blob: w.into_inner(),
    }
}

/// Every recorded notification that the player killed something, found by walking the recordings.
fn recorded_kill_notifications() -> Vec<(dereth_protocol::Opcode, Vec<u8>, String)> {
    use dereth_client_net::client_session::testing::Direction;
    let mut out = Vec::new();
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient || b.payload.len() < 16 {
                continue;
            }
            let (op, body) = if b.opcode == 0xF7B0 {
                (
                    u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes")),
                    &b.payload[16..],
                )
            } else {
                (b.opcode, &b.payload[4..])
            };
            if op != dereth_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_OTHER.0 {
                continue;
            }
            let Ok(m) = dereth_protocol::read_body::<
                dereth_protocol::combat::VictimNotificationOther,
            >(body) else {
                continue;
            };
            let blob = if b.opcode == 0xF7B0 {
                b.payload[12..].to_vec()
            } else {
                b.payload.clone()
            };
            out.push((dereth_protocol::Opcode(op), blob, m.message));
        }
    }
    assert!(
        !out.is_empty(),
        "the corpus carries recorded kill notifications"
    );
    out
}

/// Every notification a shard really sent about something the player killed is drawn on the log,
/// word for word, in the colour a plain line is drawn in.
pub fn every_recorded_kill_notification_is_drawn_verbatim() {
    let recorded = recorded_kill_notifications();
    let mut c = a_client_that_can_die();
    let green = plain_line_colour();
    let mut every = true;

    for (opcode, blob, message) in &recorded {
        let before = death_counts(&c);
        let composed = hear_a_death(
            &mut c,
            SessionEvent::UiEvent {
                opcode: *opcode,
                blob: blob.clone(),
            },
        );
        let after = death_counts(&c);
        // Read, announced, and it travelled the client's own scroll -- the road every line takes,
        // rather than a second one built for this message.
        let counted = (after.2 - before.2, after.3 - before.3, after.4 - before.4) == (1, 1, 1);
        let line = composed
            .iter()
            .find(|m| m.body.contains(message.trim()))
            .unwrap_or_else(|| panic!("no line carried {message:?}"));
        let composed_right =
            line.ty == 0 && line.window == 0 && line.body == *message && !line.body.ends_with('\n');
        let (joined, colour) = drawn_line(&mut c, message);
        let drawn = joined.contains(message) && colour == Some(green);
        assert!(counted && composed_right && drawn, "{message:?}");
        every &= counted && composed_right && drawn;
    }

    c.assert_behaviour(
        "chat.death.every-recorded-kill-notification-is-drawn-word-for-word-on-the-log",
        move |_| every,
    );
    c.shutdown();
}

#[test]
fn scenario_every_recorded_kill_notification_is_drawn_verbatim() {
    scenario("every_recorded_kill_notification_is_drawn_verbatim");
}

/// Being told that you died goes through the same hand as being told you killed something -- and a
/// notification with nothing in it is read and then deliberately says nothing.
pub fn your_own_death_goes_through_the_same_hand() {
    let mut c = a_client_that_can_die();
    let green = plain_line_colour();

    let mut every = true;
    for message in [
        "You died!",
        "Gaerlan cleaves you in twain!",
        "You are torn to ribbons by Gaerlan's assault!",
    ] {
        let before = death_counts(&c);
        let composed = hear_a_death(
            &mut c,
            notification_event(
                dereth_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_SELF,
                message,
            ),
        );
        let after = death_counts(&c);
        let counted = (after.2 - before.2, after.3 - before.3, after.4 - before.4) == (1, 1, 1);
        let line = composed
            .iter()
            .find(|m| m.body.contains(message))
            .unwrap_or_else(|| panic!("no line carried {message:?}"));
        let composed_right = (line.ty, line.window) == (0, 0) && line.body == message;
        let (joined, colour) = drawn_line(&mut c, message);
        let drawn = joined.contains(message) && colour == Some(green);
        assert!(counted && composed_right && drawn, "{message:?}");
        every &= counted && composed_right && drawn;
    }

    // A notification with nothing in it: read either way, and deliberately not announced. A
    // scenario that watched the log alone could not tell that from a message never read.
    let before_text: String = colour_runs(&mut c)
        .iter()
        .map(|(s, _)| s.as_str())
        .collect();
    let mut empty_holds = true;
    for opcode in [
        dereth_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_SELF,
        dereth_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_OTHER,
    ] {
        let before = death_counts(&c);
        hear_a_death(&mut c, notification_event(opcode, ""));
        let after = death_counts(&c);
        let holds = (after.2 - before.2, after.3 - before.3, after.4 - before.4) == (1, 0, 0);
        assert!(holds, "{opcode:?}");
        empty_holds &= holds;
    }
    let after_text: String = colour_runs(&mut c)
        .iter()
        .map(|(s, _)| s.as_str())
        .collect();
    let nothing_drawn = after_text == before_text;

    c.assert_behaviour(
        "chat.death.your-own-death-goes-through-the-same-hand-and-an-empty-one-says-nothing",
        move |_| every && empty_holds && nothing_drawn,
    );
    c.shutdown();
}

#[test]
fn scenario_your_own_death_goes_through_the_same_hand() {
    scenario("your_own_death_goes_through_the_same_hand");
}

/// Somebody else's death reaches the log; a death the player was part of does not, from either
/// side -- and a client that does not know who its player is yet shows it, because the guard is
/// about the player not being involved and not about there being one.
pub fn a_death_you_were_part_of_reaches_the_log_only_when_you_were_not() {
    let mut c = a_client_that_can_die();
    let green = plain_line_colour();
    let knows_itself = c.view().world().is_the_player(DEAD_LOCAL);

    let message = "Gaerlan splits Larktest apart!";
    let before = death_counts(&c);
    let composed = hear_a_death(&mut c, death_event(message, DEAD_VICTIM, DEAD_KILLER));
    let after = death_counts(&c);
    let counted = (after.0 - before.0, after.1 - before.1, after.4 - before.4) == (1, 1, 1);
    let line = composed
        .iter()
        .find(|m| m.body.contains(message))
        .expect("a line carried the announcement");
    let composed_right = (line.ty, line.window) == (0, 0) && line.body == message;
    let (joined, colour) = drawn_line(&mut c, message);
    let drawn = joined.contains(message) && colour == Some(green);

    // The three ways the player can be part of it. The killer side is the one an implementation is
    // most likely to miss, because the victim one alone passes everything else here.
    let mut refused = true;
    for (tag, killed, killer) in [
        ("you are the victim", DEAD_LOCAL, DEAD_KILLER),
        ("you are the killer", DEAD_VICTIM, DEAD_LOCAL),
        ("both, a suicide", DEAD_LOCAL, DEAD_LOCAL),
    ] {
        let message = format!("death {tag}");
        let before = death_counts(&c);
        hear_a_death(&mut c, death_event(&message, killed, killer));
        let after = death_counts(&c);
        let holds = (after.0 - before.0, after.1 - before.1, after.4 - before.4) == (1, 0, 0)
            && !drawn_line(&mut c, &message).0.contains(&message);
        assert!(holds, "{tag}");
        refused &= holds;
    }

    // And the same thing again on a client that does not know who its player is: it matches
    // nobody, so the line is shown.
    c.world_mut().player = None;
    let before = death_counts(&c);
    hear_a_death(
        &mut c,
        death_event("death with no player yet", DEAD_LOCAL, DEAD_KILLER),
    );
    let after = death_counts(&c);
    let (joined, colour) = drawn_line(&mut c, "death with no player yet");
    let unknown = (after.0 - before.0, after.1 - before.1) == (1, 1)
        && joined.contains("death with no player yet")
        && colour == Some(green);

    c.assert_behaviour(
        "chat.death.a-death-the-player-was-part-of-reaches-the-log-only-when-he-was-not",
        move |_| knows_itself && counted && composed_right && drawn && refused && unknown,
    );
    c.shutdown();
}

#[test]
fn scenario_a_death_you_were_part_of_reaches_the_log_only_when_you_were_not() {
    scenario("a_death_you_were_part_of_reaches_the_log_only_when_you_were_not");
}

/// None of the three death lines can be silenced, and a combat line beside them can -- which is
/// what makes that a measurement rather than a squelch that never armed.
pub fn no_death_line_is_silenced_where_a_combat_line_is() {
    use dereth_protocol::Message as _;

    let mut c = a_client_that_can_die();
    let green = plain_line_colour();

    // Everything the table can be asked about, silenced: every kind, and the killer by name and by
    // account. The two death arms ask nothing, so none of it can bite.
    {
        let mut entry = dereth_client_model::chat::SquelchEntry {
            name: "Gaerlan".to_owned(),
            ..Default::default()
        };
        entry.squelch_everything();
        let squelch = &mut c.world_mut().chat.squelch;
        squelch.characters.insert(DEAD_KILLER, entry.clone());
        squelch.characters.insert(ObjectId(0), entry);
        squelch.accounts.insert("Gaerlan".to_owned(), 0);
        squelch
            .global
            .types
            .insert(dereth_client_model::chat::text_type::COMBAT);
        squelch.global.types.insert(0);
    }

    // The control: a combat line, which does ask, and is dropped.
    let before = c.view().expect_app().hud().stats.combat_lines_squelched;
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(dereth_protocol::Opcode::COMBAT_HANDLE_ATTACKER_NOTIFICATION_EVENT.0);
    dereth_protocol::combat::AttackerNotification {
        defender_name: "death control".to_owned(),
        damage_type: 1,
        percent: 0.1,
        damage: 3,
        critical: 0,
        attack_conditions: 0,
        attack_conditions_high: 0,
    }
    .write(&mut w)
    .expect("the control message encodes");
    hear_a_death(
        &mut c,
        SessionEvent::UiEvent {
            opcode: dereth_protocol::Opcode::COMBAT_HANDLE_ATTACKER_NOTIFICATION_EVENT,
            blob: w.into_inner(),
        },
    );
    let armed = c.view().expect_app().hud().stats.combat_lines_squelched == before + 1;

    // And the three subjects, with that same silence in force.
    let mut every = true;
    let cases: Vec<(String, SessionEvent)> = vec![
        (
            "death silenced third party".to_owned(),
            death_event("death silenced third party", DEAD_VICTIM, DEAD_KILLER),
        ),
        (
            "death silenced your own".to_owned(),
            notification_event(
                dereth_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_SELF,
                "death silenced your own",
            ),
        ),
        (
            "death silenced your kill".to_owned(),
            notification_event(
                dereth_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_OTHER,
                "death silenced your kill",
            ),
        ),
    ];
    for (needle, event) in cases {
        let before = death_counts(&c);
        hear_a_death(&mut c, event);
        let after = death_counts(&c);
        let (joined, colour) = drawn_line(&mut c, &needle);
        let holds = after.4 - before.4 == 1 && joined.contains(&needle) && colour == Some(green);
        assert!(holds, "{needle}");
        every &= holds;
    }

    c.assert_behaviour(
        "chat.death.no-death-line-can-be-silenced-where-a-combat-line-can",
        move |_| armed && every,
    );
    c.shutdown();
}

#[test]
fn scenario_no_death_line_is_silenced_where_a_combat_line_is() {
    scenario("no_death_line_is_silenced_where_a_combat_line_is");
}

// ---------------------------------------------------------------------------------------------
// Poses: the run between stars, and the keys bound to one
// ---------------------------------------------------------------------------------------------
//
// **No datagram leaves this process.** The client has no link; where the bytes are the claim the
// request is handed to the production sender over a mock transport.

/// The two sections of the shipped keymap a pose can be bound in.
const MOVEMENT_COMMANDS: u32 = 4;
const EMOTES_MAP: u32 = 0x1000_0006;
/// Waving, and the two motions this section is about.
const WAVE_ACTION: u32 = 0x1000_00E5;
const MOTION_WAVE: u32 = 0x1300_0087;
const MOTION_SLEEPING: u32 = 0x4100_0014;

/// The key the shipped keymap binds an action to, answered as one of the keys a hand could press.
fn key_bound_to(c: &mut HeadlessClient, action: u32, map: u32) -> winit::keyboard::KeyCode {
    use winit::keyboard::KeyCode;
    const CANDIDATES: [KeyCode; 8] = [
        KeyCode::KeyJ,
        KeyCode::KeyB,
        KeyCode::KeyO,
        KeyCode::KeyU,
        KeyCode::KeyI,
        KeyCode::KeyK,
        KeyCode::KeyG,
        KeyCode::KeyV,
    ];
    let binding = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell is part of the shell")
        .keys_for_action(
            dereth_input::ActionId(action),
            dereth_input::InputMapId(map),
        )
        .into_iter()
        .find(|b| b.meta_mode == 0)
        .unwrap_or_else(|| panic!("nothing unmodified is bound to {action:#010X} in {map:#010X}"));
    CANDIDATES
        .into_iter()
        .find(|k| {
            dereth_client::pump::scan_code_from_key_code(*k)
                .is_some_and(|s| s & 0x7F == binding.control.offset() & 0x7F)
        })
        .unwrap_or_else(|| panic!("no candidate key carries that scan code"))
}

/// A client in the world, with the gameplay screen up, to press keys at.
fn a_client_to_pose_with() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay(4))
}

/// A key bound to lying down really lies the body down, and one the shipped keymap binds to no
/// pose at all is bound to none.
pub fn a_key_bound_to_a_pose_moves_the_body() {
    use winit::keyboard::KeyCode;

    let mut c = a_client_to_pose_with();
    let mut hand = Hand::new();

    let lie_down = key_bound_to(
        &mut c,
        dereth_client_runtime::actions::movement::action::LAY_DOWN.0,
        MOVEMENT_COMMANDS,
    );
    let bound_to_b = lie_down == KeyCode::KeyB;
    let before = c
        .view()
        .expect_app()
        .probe()
        .movement()
        .transient_motions_issued;
    hand.tap(&mut c, key_of(lie_down));
    c.tick(2);
    let moved = c
        .view()
        .expect_app()
        .probe()
        .movement()
        .transient_motions_issued
        == before + 1
        && c.view()
            .expect_app()
            .probe()
            .movement()
            .last_transient_motion
            == Some((MOTION_SLEEPING, true));

    // The other half. The lookup must be able to find a binding at all, so it is pointed at a key
    // that has one first: an empty answer from a lookup that never resolves anything is not
    // evidence of anything.
    let resolves = key_bound_to(&mut c, WAVE_ACTION, EMOTES_MAP) == KeyCode::KeyJ;
    let v_scan = dereth_client::pump::scan_code_from_key_code(KeyCode::KeyV)
        .expect("that key has a scan code");
    let mut bound_to_v: Vec<u32> = Vec::new();
    for map in [MOVEMENT_COMMANDS, EMOTES_MAP] {
        let shell = c.app_mut().input_manager_mut().expect("the input shell");
        if let Some(section) = shell.manager.keymap.section(dereth_input::InputMapId(map)) {
            for (control, act) in section.bindings() {
                if control.control.offset() & 0x7F == v_scan & 0x7F {
                    bound_to_v.push(act.0);
                }
            }
        }
    }
    let unbound = bound_to_v.is_empty();

    c.assert_behaviour(
        "chat.pose.a-key-bound-to-a-pose-moves-the-body-and-one-that-is-not-is-bound-to-nothing",
        move |_| bound_to_b && moved && resolves && unbound,
    );
    c.shutdown();
}

#[test]
fn scenario_a_key_bound_to_a_pose_moves_the_body() {
    scenario("a_key_bound_to_a_pose_moves_the_body");
}

/// The bytes one request goes out in, composed here rather than by the writer being asserted over.
fn game_action(stamp: u32, opcode: u32, body: &[u8]) -> Vec<u8> {
    let mut v = vec![0xb1, 0xf7, 0x00, 0x00];
    v.extend_from_slice(&stamp.to_le_bytes());
    v.extend_from_slice(&opcode.to_le_bytes());
    v.extend_from_slice(body);
    v
}

/// A length-prefixed string padded out to four.
fn pstr(s: &str) -> Vec<u8> {
    let mut v = u16::try_from(s.len())
        .expect("short")
        .to_le_bytes()
        .to_vec();
    v.extend_from_slice(s.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

/// A run between stars is performed, the others are told one word and the player reads another,
/// and the rest of the line is spoken.
pub fn a_run_between_stars_is_performed_and_the_rest_spoken() {
    let mut c = a_client_to_pose_with();
    let mut hand = Hand::new();

    let motions = c
        .view()
        .expect_app()
        .probe()
        .movement()
        .transient_motions_issued;
    let poses = c.view().expect_app().interaction().stats.poses_resolved;
    hand.say(&mut c, "hello *wave* there");

    let found = c.view().expect_app().interaction().stats.poses_resolved == poses + 1;
    // Two messages, in the client's own order: what the others are told, then what he said.
    let sent = c.view().expect_app().interaction().last_sent.to_vec();
    let both = sent
        == vec![
            dereth_client_model::Request::SoulEmote(
                dereth_protocol::comms::CommunicationSoulEmote {
                    message: "waves.".to_owned(),
                },
            ),
            dereth_client_model::Request::Talk(dereth_protocol::comms::CommunicationTalk {
                message: "hello  there".to_owned(),
            }),
        ];

    // The bytes.
    let mut session = Session::new(MockTransport::new());
    let mut bytes = true;
    for (i, r) in sent.iter().enumerate() {
        bytes &= dereth_client_runtime::requests::send_request(&mut session, r);
        let packet = session
            .transport
            .sent
            .last()
            .expect("one datagram per message");
        let stamp = u32::try_from(i + 1).expect("small");
        let want = match r {
            dereth_client_model::Request::SoulEmote(_) => {
                game_action(stamp, 0x01E1, &pstr("waves."))
            }
            dereth_client_model::Request::Talk(_) => {
                game_action(stamp, 0x0015, &pstr("hello  there"))
            }
            other => panic!("unexpected {other:?}"),
        };
        bytes &=
            (packet.queue, packet.ordered) == (NetQueue::Weenie, true) && packet.payload == want;
    }

    // The body moved, with the same command the key bound to waving produces.
    c.tick(2);
    let performed = c
        .view()
        .expect_app()
        .probe()
        .movement()
        .transient_motions_issued
        == motions + 1
        && c.view()
            .expect_app()
            .probe()
            .movement()
            .last_transient_motion
            == Some((MOTION_WAVE, true));

    // And what the player himself reads is the **other** word of the pair.
    c.tick(3);
    let log = log_text(&mut c);
    let echoed = log.contains("You wave.")
        && !log.contains("waves.")
        && c.view()
            .expect_app()
            .interaction()
            .stats
            .pose_echoes_printed
            == 1;

    c.assert_behaviour(
        "chat.pose.a-run-between-stars-is-performed-and-the-rest-of-the-line-is-spoken",
        move |_| found && both && bytes && performed && echoed,
    );
    c.shutdown();
}

#[test]
fn scenario_a_run_between_stars_is_performed_and_the_rest_spoken() {
    scenario("a_run_between_stars_is_performed_and_the_rest_spoken");
}

/// A run on its own says nothing at all, and a run the client cannot place is spoken whole --
/// stars and typing and all -- rather than being eaten.
pub fn a_run_alone_says_nothing_and_an_unknown_one_is_spoken() {
    let mut c = a_client_to_pose_with();
    let mut hand = Hand::new();

    hand.say(&mut c, "*wave*");
    let alone = c.view().expect_app().interaction().last_sent.as_slice()
        == [dereth_client_model::Request::SoulEmote(
            dereth_protocol::comms::CommunicationSoulEmote {
                message: "waves.".to_owned(),
            },
        )];

    let poses = c.view().expect_app().interaction().stats.poses_resolved;
    hand.say(&mut c, "hello *xyzzy* there");
    let unknown = c.view().expect_app().interaction().stats.poses_resolved == poses
        && c.view().expect_app().interaction().last_sent.as_slice()
            == [dereth_client_model::Request::Talk(
                dereth_protocol::comms::CommunicationTalk {
                    message: "hello *xyzzy* there".to_owned(),
                },
            )];

    c.assert_behaviour(
        "chat.pose.a-run-alone-says-nothing-and-one-the-client-cannot-place-is-spoken-whole",
        move |_| alone && unknown,
    );
    c.shutdown();
}

#[test]
fn scenario_a_run_alone_says_nothing_and_an_unknown_one_is_spoken() {
    scenario("a_run_alone_says_nothing_and_an_unknown_one_is_spoken");
}

/// Saying something with the command that says it goes through the same extraction as saying it
/// with no command in front at all.
pub fn the_say_command_goes_through_the_same_extraction() {
    let mut c = a_client_to_pose_with();
    let mut hand = Hand::new();
    hand.say(&mut c, "@say *wave* hi");
    let same = c.view().expect_app().interaction().last_sent.as_slice()
        == [
            dereth_client_model::Request::SoulEmote(
                dereth_protocol::comms::CommunicationSoulEmote {
                    message: "waves.".to_owned(),
                },
            ),
            dereth_client_model::Request::Talk(dereth_protocol::comms::CommunicationTalk {
                message: "hi".to_owned(),
            }),
        ];

    c.assert_behaviour(
        "chat.pose.the-say-command-goes-through-the-same-extraction",
        move |_| same,
    );
    c.shutdown();
}

#[test]
fn scenario_the_say_command_goes_through_the_same_extraction() {
    scenario("the_say_command_goes_through_the_same_extraction");
}

/// Asking for the list of poses prints the shipped list, whole and in one line, and sends nothing.
pub fn the_emotes_command_prints_the_shipped_list() {
    let mut c = a_client_to_pose_with();
    let mut hand = Hand::new();

    let unimplemented = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .chat_commands_unimplemented;
    hand.say(&mut c, "@emotes");
    let wired = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .chat_commands_unimplemented
        == unimplemented
        && c.view().expect_app().interaction().last_sent.is_empty();
    c.tick(3);
    let log = log_text(&mut c);
    let want = dereth_client_model::emotes::emote_list_text();
    // Two of the names, so a list that was cut short cannot pass.
    let printed =
        log.contains(want.trim_end()) && want.contains("ShakeFist") && want.contains("Shake Head");

    c.assert_behaviour(
        "chat.pose.the-list-command-prints-the-shipped-list-and-sends-nothing",
        move |_| wired && printed,
    );
    c.shutdown();
}

#[test]
fn scenario_the_emotes_command_prints_the_shipped_list() {
    scenario("the_emotes_command_prints_the_shipped_list");
}

// ---------------------------------------------------------------------------------------------
// The shipped table of poses, and what it decides
// ---------------------------------------------------------------------------------------------
//
// The two words a pose has are the **shipped** table's and not this file's: a pose has one form
// the player reads about himself and another the people around him read, and which is which is
// what these are about.

/// The shipped table of poses, out of the retail data.
fn shipped_pose_table() -> dereth_assets::tables::ChatPoseTable {
    use dereth_assets::Decode as _;
    let store =
        dereth_dat::testing::open_store().expect("the retail data is this scenario's oracle");
    let id = store
        .ids_of(dereth_dat::divine::DbType::ChatPoseTable)
        .into_iter()
        .next()
        .expect("the retail data carries a table of poses");
    let bytes = store.read_portal(id).expect("the table reads");
    dereth_assets::tables::ChatPoseTable::decode(&mut dereth_dat::Cursor::new(&bytes))
        .expect("the table decodes")
}

/// The motion a pose's name resolves to, which the client asks the animation side for.
fn motion_of(name: &str) -> Option<u32> {
    dereth_animation::command::MotionCommand::from_name(name).map(|m| m.0)
}

/// One message, through the production sender over a mock transport. **Nothing is sent.**
fn on_the_wire<M: dereth_protocol::Message>(m: &M) -> (NetQueue, Vec<u8>) {
    let mut s = Session::new(MockTransport::new());
    s.send_action(m).expect("the message encodes");
    assert_eq!(s.transport.sent.len(), 1, "one action, one datagram");
    let b = &s.transport.sent[0];
    (b.queue, b.payload.clone())
}

/// What a pose sends is a different message from what the emote command sends: the same framing
/// and the same place in the order, a different message and different words.
pub fn a_pose_sends_a_different_message_from_the_emote_command() {
    let table = shipped_pose_table();
    let out = dereth_client_model::emotes::public_chat("hello *wave* there", |name| {
        dereth_client_model::emotes::pose(&table, name, 1, motion_of)
    });
    let extracted = out.poses.len() == 1;
    let p = &out.poses[0];
    let resolved = p.motion_name == "Wave"
        && p.motion_command.is_some()
        // The two words: one for the people around, one for the player himself.
        && p.soul_emote.as_deref() == Some("waves.")
        && p.my_emote.as_deref() == Some("wave.")
        && out.speech.as_deref() == Some("hello  there");

    let (queue, pose_bytes) = on_the_wire(&dereth_protocol::comms::CommunicationSoulEmote {
        message: p.soul_emote.clone().expect("the other word is not empty"),
    });
    let framed = queue == NetQueue::Weenie
        && pose_bytes[0..4] == 0xF7B1_u32.to_le_bytes()
        && pose_bytes[8..12] == [0xE1, 0x01, 0x00, 0x00]
        && pose_bytes[12..14] == 6_u16.to_le_bytes()
        && &pose_bytes[14..20] == b"waves.";

    // And beside it the message the emote command sends, with the same header and the same first
    // place in the order -- but a different message and a different body.
    let (_, command_bytes) = on_the_wire(&dereth_protocol::comms::CommunicationEmote {
        message: "waves".to_owned(),
    });
    let differ = pose_bytes[8..12] != command_bytes[8..12]
        && pose_bytes[0..8] == command_bytes[0..8]
        && pose_bytes[12..] != command_bytes[12..];
    // The emote command's own bytes, whole: a length-prefixed word padded out to four.
    let commands_own = command_bytes[8..12] == [0xDF, 0x01, 0x00, 0x00]
        && command_bytes[4..8] == 1_u32.to_le_bytes()
        && command_bytes[12..14] == 5_u16.to_le_bytes()
        && &command_bytes[14..19] == b"waves"
        && command_bytes.len() == 20;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.pose.what-a-pose-sends-is-a-different-message-from-what-the-command-sends",
        move |_| extracted && resolved && framed && differ && commands_own,
    );
}

#[test]
fn scenario_a_pose_sends_a_different_message_from_the_emote_command() {
    scenario("a_pose_sends_a_different_message_from_the_emote_command");
}

/// The possessive word in a pose is chosen by the player's sex before the message leaves, so no
/// placeholder ever reaches anybody's window.
pub fn the_possessive_word_is_chosen_by_sex() {
    let table = shipped_pose_table();
    let male =
        dereth_client_model::emotes::pose(&table, "Scratch Head", 1, motion_of).expect("a pose");
    let female =
        dereth_client_model::emotes::pose(&table, "Scratch Head", 2, motion_of).expect("a pose");
    let chosen = male.soul_emote.as_deref() == Some("scratches his head.")
        && female.soul_emote.as_deref() == Some("scratches her head.")
        // The form the player reads about himself has no possessive in it at all.
        && male.my_emote.as_deref() == Some("scratch your head.")
        && !male.soul_emote.as_deref().unwrap_or_default().contains("%p");
    // And the name is looked up without minding how it was capitalised.
    let insensitive = dereth_client_model::emotes::pose(&table, "scratch head", 1, motion_of)
        .expect("found either way")
        .motion_name
        == "ScratchHead";

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.pose.the-possessive-word-is-chosen-by-sex-before-the-message-leaves",
        move |_| chosen && insensitive,
    );
}

#[test]
fn scenario_the_possessive_word_is_chosen_by_sex() {
    scenario("the_possessive_word_is_chosen_by_sex");
}

/// What the player reads about his own pose is never turned into a noise, whatever people he is.
pub fn the_players_own_echo_is_never_turned_into_a_noise() {
    let table = shipped_pose_table();
    let p = dereth_client_model::emotes::pose(&table, "Wave", 1, motion_of).expect("a pose");
    let my_emote = p.my_emote.clone().expect("the player's own form");

    let mut every = true;
    for heritage in [0_i32, 12] {
        let mut c = HeadlessClient::model();
        {
            let w = c.world_mut();
            let mut q = dereth_client_model::qualities::Qualities::new();
            q.ints = Some(
                [(
                    dereth_ui_screens::panels::inventory::HERITAGE_GROUP_PROPERTY,
                    heritage,
                )]
                .into_iter()
                .collect(),
            );
            w.seed_player_desc(ObjectId(0x5000_0001), q);
        }
        c.hud_mut().player_desc_received = true;
        let is_olthoi = c.view().hud().is_olthoi(c.view().world());
        assert_eq!(
            is_olthoi,
            heritage == 12,
            "the premise: {heritage} decides it"
        );

        c.when(Inbound::message(
            &dereth_protocol::comms::CommunicationHearSoulEmote {
                sender: ObjectId(0),
                sender_name: dereth_client_model::emotes::LOCAL_ECHO_NAME.to_owned(),
                text: my_emote.clone(),
            },
        ));
        let lines: Vec<String> = c
            .view()
            .chat_lines()
            .iter()
            .map(|m| m.body.clone())
            .collect();
        let stats = &c.view().hud().stats;
        let holds = lines == vec!["You wave.".to_owned()]
            && c.view().chat_lines()[0].ty == 0x0C
            && stats.emote_lines_composed == 1
            // The echo cannot be turned into a noise, whatever people the player is.
            && stats.emote_lines_untranslated == 0
            // And it is not swallowed as the player's own words coming back to him.
            && stats.soul_emote_self_echoes_discarded == 0;
        assert!(holds, "heritage {heritage}: {lines:?}");
        every &= holds;
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.pose.the-players-own-echo-is-never-turned-into-a-noise",
        move |_| every,
    );
}

#[test]
fn scenario_the_players_own_echo_is_never_turned_into_a_noise() {
    scenario("the_players_own_echo_is_never_turned_into_a_noise");
}

/// Every pose the list advertises is one the shipped table can really perform.
///
/// The list and the table are two different things and can disagree; a name the list advertises
/// that the table cannot place is a command that silently does nothing at all.
pub fn every_pose_the_list_advertises_can_be_performed() {
    let table = shipped_pose_table();
    let text = dereth_client_model::emotes::emote_list_text();
    let shape = text.starts_with("Standard Emotes:\n") && text.ends_with("Shake Head\n\n");

    let mut missing: Vec<&str> = Vec::new();
    let mut motionless: Vec<&str> = Vec::new();
    for name in dereth_client_model::emotes::STANDARD_EMOTES {
        match dereth_client_model::emotes::pose(&table, name, 1, motion_of) {
            None => missing.push(name),
            Some(p) if p.motion_command.is_none() => motionless.push(name),
            Some(_) => {}
        }
    }
    assert!(
        missing.is_empty(),
        "advertised and not in the table: {missing:?}"
    );
    assert!(
        motionless.is_empty(),
        "advertised, in the table, and with no motion: {motionless:?}"
    );
    let all_perform = missing.is_empty() && motionless.is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.pose.every-pose-the-list-advertises-can-really-be-performed",
        move |_| shape && all_perform,
    );
}

#[test]
fn scenario_every_pose_the_list_advertises_can_be_performed() {
    scenario("every_pose_the_list_advertises_can_be_performed");
}

// ---------------------------------------------------------------------------------------------
// Starting a private message to somebody
// ---------------------------------------------------------------------------------------------
//
// **No datagram leaves this process**: starting a tell is the client putting words in its own box,
// and every scenario here asserts that nothing is sent.

/// The action that starts a tell to whoever is selected, the map it is in, and a key that the
/// shipped keymaps deliberately leave free to bind it to.
const TELL_SELECTED: dereth_input::ActionId = dereth_input::ActionId(0x1000_0119);
const CHAT_COMMANDS: dereth_input::InputMapId = dereth_input::InputMapId(0x1000_000A);
const FREE_KEY_SCAN: u16 = 0x41;

/// The main chat window's entry -- the one the tell fills.
fn main_chat_entry(c: &HeadlessClient) -> ElemHandle {
    let app = c.view().expect_app();
    let any: &dyn std::any::Any = app
        .ui()
        .expect("the shell")
        .flow
        .current()
        .expect("a screen");
    any.downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen")
        .chat_windows
        .first()
        .and_then(|w| w.entry)
        .expect("the main window's entry")
}

/// Bind the tell-to-selected action to a key the shipped keymaps leave free, so that it can be
/// pressed at all: it is an action the client has and gives no key to.
fn bind_the_tell_key(c: &mut HeadlessClient) {
    use dereth_input::spec::{activation, ControlCode, SubControlIndex};
    let input = c.app_mut().input_manager_mut().expect("the input shell");
    assert!(
        input
            .keys_for_action(TELL_SELECTED, CHAT_COMMANDS)
            .is_empty(),
        "the client has this action and gives it no key, which is why one is bound here"
    );
    let key = dereth_input::ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, FREE_KEY_SCAN),
        0,
        activation::CLICK,
    );
    assert!(input.set_binding(CHAT_COMMANDS, TELL_SELECTED, None, key));
}

/// Put a named body in the world and select it, and let the screen see the selection.
fn select_a_named_player(c: &mut HeadlessClient, id: ObjectId, name: &str) {
    {
        let w = c.world_mut();
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.pwd.name = name.to_owned();
        wn.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
        w.tables.weenies.insert(id, wn);
        w.selected = Some(id);
    }
    c.tick(1);
}

/// The text of one element.
fn element_text_of(c: &mut HeadlessClient, h: ElemHandle) -> String {
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// Put one line with a clickable name in it on the main log, and answer with the log and a point
/// over the first letter of that name.
fn a_clickable_name_on_the_log(c: &mut HeadlessClient, name: &str) -> (ElemHandle, (i32, i32)) {
    let line = dereth_ui_screens::chat::interface::ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        ty: 17,
        body: format!("<Tell:IIDString:0:{name}>{name}<\\Tell> says, \"hello\""),
        prefix: None,
        window: 0,
    };
    let reached = with_screen(c, |ui, s| s.recv_display_final_string_info(ui, &line));
    assert!(
        reached.contains(&dereth_ui_screens::chat::interface::window::MAIN),
        "the line reaches the main window"
    );
    let log = element(c, dereth_ui_screens::chat::window::LOG);
    let screen_box = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .screen_box(log);
    let (placed, glyph) = with_screen(c, |ui, _| {
        let t = ui.text_element_mut(log).expect("the log");
        let at = t
            .glyphs
            .glyphs
            .iter()
            .position(|g| {
                g.tag
                    .as_ref()
                    .is_some_and(|tag| tag.payload() == Some(name))
                    && g.data == name.encode_utf16().next().expect("a name has a letter")
            })
            .expect("the drawn name keeps what makes it clickable");
        (t.compose(screen_box)[at], t.glyphs.glyphs[at].clone())
    });
    (
        log,
        (
            placed.x + (glyph.width / 4).max(1),
            placed.y + (glyph.height / 2).max(1),
        ),
    )
}

/// Clicking a name in the chat log starts a private message to him -- and it does that only while
/// the entry does not already have the caret, so a half-typed line is not thrown away.
pub fn clicking_a_name_in_the_log_starts_a_tell_to_him() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let mut hand = Hand::new();
    let entry = main_chat_entry(&c);
    let (log, point) = a_clickable_name_on_the_log(&mut c, "Alba");
    c.app_mut().probe_mut().interaction_mut().last_sent.clear();

    let ready = focus_of(&c) != Some(entry)
        && element_text_of(&mut c, entry).is_empty()
        && c.view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .hit_test_screen(point.0, point.1)
            == Some(log);

    hand.press_at(&mut c, point.0, point.1);
    let started = element_text_of(&mut c, entry) == "@tell Alba, "
        && focus_of(&c) == Some(entry)
        && c.view().expect_app().interaction().last_sent.is_empty();

    // The other side of the guard. A press on the log takes the caret by itself, so merely giving
    // the entry the caret beforehand cannot reach it: the caret has to come back between the press
    // and the release.
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .text_element_mut(entry)
        .expect("the entry")
        .set_text("draft");
    let (_, other) = a_clickable_name_on_the_log(&mut c, "Focused");
    hand.mouse_down_at(&mut c, other.0, other.1);
    let took_it = focus_of(&c) != Some(entry);
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .take_focus(entry);
    hand.mouse_up(&mut c);
    let left_alone = element_text_of(&mut c, entry) == "draft"
        && c.view().expect_app().interaction().last_sent.is_empty();

    c.assert_behaviour(
        "chat.tell.clicking-a-name-in-the-log-starts-a-private-message-to-him",
        move |_| ready && started && took_it && left_alone,
    );
    c.shutdown();
}

#[test]
fn scenario_clicking_a_name_in_the_log_starts_a_tell_to_him() {
    scenario("clicking_a_name_in_the_log_starts_a_tell_to_him");
}

/// The key that starts a private message to whoever is selected names him, gives the entry the
/// caret with the caret at the end and nothing picked out, and sends nothing.
pub fn the_tell_key_names_the_selected_player() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let mut hand = Hand::new();
    bind_the_tell_key(&mut c);
    select_a_named_player(&mut c, ObjectId(0x5000_1234), "Target Player");
    let entry = main_chat_entry(&c);
    let before = c.view().expect_app().interaction().last_sent.len();

    let key = key_of(winit::keyboard::KeyCode::F7);
    hand.key(&mut c, key, true);
    c.tick(1);

    let filled =
        element_text_of(&mut c, entry) == "@tell Target Player, " && focus_of(&c) == Some(entry);
    let caret = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let t = ui.text_element_mut(entry).expect("the entry");
        t.cursor == t.glyphs.len() && !t.bits.selecting()
    };
    let sent_nothing = c.view().expect_app().interaction().last_sent.len() == before;
    hand.key(&mut c, key, false);
    c.tick(1);

    c.assert_behaviour(
        "chat.tell.the-key-for-a-private-message-to-the-selected-player-names-him",
        move |_| filled && caret && sent_nothing,
    );
    c.shutdown();
}

#[test]
fn scenario_the_tell_key_names_the_selected_player() {
    scenario("the_tell_key_names_the_selected_player");
}

/// The two rows of the talk-to menu that change with the selection carry the selected player's own
/// name, drawn into the rows rather than left as a placeholder.
pub fn the_selected_players_name_fills_both_changing_rows() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let mut hand = Hand::new();
    let selected = ObjectId(0x5000_1234);
    select_a_named_player(&mut c, selected, "Target Player");

    // The rows are filled by the client's own once-a-second sweep rather than by the selection
    // being written, so the sweep is run -- with the one thing a nearby body would have given it.
    let mut facts = with_screen(&mut c, |_ui, s| s.chat_auto_target_world.clone());
    facts.in_range_of_player.push(selected.0);
    let adopted = {
        let (interaction, world) = c.app_mut().probe_mut().interaction_and_world_mut();
        world
            .chat
            .set_talk_focus_enabled(TalkFocus::Selected, false);
        interaction.update_chat_target(world, 100.0, &facts);
        world.chat.last_speakable_target == Some(selected)
    };
    c.tick(1);

    let (menu, row, squelch) = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the shell").ui;
        let any: &dyn std::any::Any = app
            .ui()
            .expect("the shell")
            .flow
            .current()
            .expect("a screen");
        let main = &any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen")
            .main_chat;
        (
            main.menu.expect("the talk-to menu"),
            main.menu_item(ui, 2).expect("the tell-to-selected row"),
            main.squelch_toggle.expect("the squelch row"),
        )
    };
    let popup = dereth_ui::widgets::menu::popup_handle(
        &c.view().expect_app().ui().expect("the shell").ui,
        menu,
    )
    .expect("the menu has a popup");

    hand.click_handle(&mut c, menu);
    let opened = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .node(popup)
        .expect("alive")
        .region
        .flags
        .visible;
    let reachable = {
        let ui = &c.view().expect_app().ui().expect("the shell").ui;
        let b = ui.screen_box(row);
        ui.hit_test_screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(row)
    };
    let named = element_text_of(&mut c, row) == "Tell to Target Player"
        && element_text_of(&mut c, squelch) == "Squelch (ignore) Target Player";

    c.assert_behaviour(
        "chat.talk-to-menu.the-selected-players-name-fills-both-of-the-changing-rows",
        move |_| adopted && opened && reachable && named,
    );
    c.shutdown();
}

#[test]
fn scenario_the_selected_players_name_fills_both_changing_rows() {
    scenario("the_selected_players_name_fills_both_changing_rows");
}

/// The key does nothing when there is nobody selected, or when what is selected is not a player --
/// and it reads the selection as it is now rather than as the last frame left it.
pub fn the_tell_key_does_nothing_without_a_player_selected() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let mut hand = Hand::new();
    bind_the_tell_key(&mut c);
    select_a_named_player(&mut c, ObjectId(0x5000_2345), "Old Selection");
    let entry = main_chat_entry(&c);
    let key = key_of(winit::keyboard::KeyCode::F7);

    // Cleared after the screen had already been told about it, so a client reading last frame's
    // answer would address the wrong person.
    c.world_mut().selected = None;
    let stale = with_screen(&mut c, |_, s| {
        s.chat_auto_target_world.selected_name.clone()
    }) == "Old Selection";
    let fired_before = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired;
    hand.key(&mut c, key, true);
    c.tick(1);
    let pressed = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired
        > fired_before;
    let nothing = element_text_of(&mut c, entry).is_empty() && focus_of(&c) != Some(entry);
    hand.key(&mut c, key, false);
    c.tick(1);

    // And the two ends of the range of ids a player can have, neither of which is one.
    let mut boundaries = true;
    for boundary in [0x5000_0000_u32, 0x7000_0000] {
        select_a_named_player(&mut c, ObjectId(boundary), "Not A Player");
        hand.key(&mut c, key, true);
        c.tick(1);
        let holds = element_text_of(&mut c, entry).is_empty() && focus_of(&c) != Some(entry);
        assert!(holds, "{boundary:#010X}");
        boundaries &= holds;
        hand.key(&mut c, key, false);
        c.tick(1);
    }

    c.assert_behaviour(
        "chat.tell.the-key-does-nothing-when-nobody-a-player-could-talk-to-is-selected",
        move |_| stale && pressed && nothing && boundaries,
    );
    c.shutdown();
}

#[test]
fn scenario_the_tell_key_does_nothing_without_a_player_selected() {
    scenario("the_tell_key_does_nothing_without_a_player_selected");
}

/// A client with the friends tab open and two friends on it.
fn a_client_with_two_friends(hand: &mut Hand) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    crate::social::open_the_friends_tab(&mut c);
    let _ = &hand;
    c.when(Inbound::message(&crate::social::friends_update(
        vec![
            crate::social::a_friend(crate::social::FRIEND_A, "Ash", true),
            crate::social::a_friend(crate::social::FRIEND_B, "Bex", true),
        ],
        0,
    )));
    c.tick(1);
    assert_eq!(
        c.view().expect_app().hud().panels.friends.rows().len(),
        2,
        "the list reached the tab"
    );
    c
}

/// Picking a friend and pressing the tell button fills the main entry with the beginning of a
/// private message to him, gives it the caret, and replaces whatever was half-typed there.
pub fn picking_a_friend_and_pressing_tell_fills_the_entry() {
    let mut hand = Hand::new();
    let mut c = a_client_with_two_friends(&mut hand);
    let entry = main_chat_entry(&c);

    // Before: nothing picked, the button dark, the entry empty and without the caret.
    let before = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .selected
        .is_none()
        && !crate::social::friends_button_lit(
            &c,
            c.view().expect_app().hud().panels.friends.tell_button,
        )
        && element_text_of(&mut c, entry).is_empty()
        && focus_of(&c) != Some(entry);

    let (list, row) = {
        let p = &c.view().expect_app().hud().panels.friends;
        (
            p.list.as_ref().expect("the list is bound").handle,
            p.rows()[0].element,
        )
    };
    let first_is_one = c.view().expect_app().hud().panels.friends.rows()[0].name == "Ash";
    hand.click_row(&mut c, list, row);
    let picked = c.view().expect_app().hud().panels.friends.selected == Some(0)
        && crate::social::friends_button_lit(
            &c,
            c.view().expect_app().hud().panels.friends.tell_button,
        );

    let tell = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .tell_button
        .expect("bound");
    hand.click_handle(&mut c, tell);
    let filled = element_text_of(&mut c, entry) == "@tell Ash, " && focus_of(&c) == Some(entry);
    let caret = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let t = ui.text_element_mut(entry).expect("the entry");
        t.cursor == t.glyphs.len() && !t.bits.selecting()
    };
    // And the window's own idea of what is being typed agrees with the element.
    let agrees = with_screen(&mut c, |_, s| {
        (s.chat[0].entry.clone(), s.chat[0].chat_entry_active)
    }) == ("@tell Ash, ".to_owned(), true);

    // Half-typed, then the other friend: replaced, not added to.
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .text_element_mut(entry)
        .expect("the entry")
        .set_text("hello there");
    let half_typed = element_text_of(&mut c, entry) == "hello there";
    let row = c.view().expect_app().hud().panels.friends.rows()[1].element;
    hand.click_row(&mut c, list, row);
    let tell = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .tell_button
        .expect("bound");
    hand.click_handle(&mut c, tell);
    let replaced = element_text_of(&mut c, entry) == "@tell Bex, " && focus_of(&c) == Some(entry);

    c.assert_behaviour(
        "chat.tell.picking-a-friend-and-pressing-tell-fills-the-entry-and-replaces-what-was-there",
        move |_| {
            before && first_is_one && picked && filled && caret && agrees && half_typed && replaced
        },
    );
    c.shutdown();
}

#[test]
fn scenario_picking_a_friend_and_pressing_tell_fills_the_entry() {
    scenario("picking_a_friend_and_pressing_tell_fills_the_entry");
}

/// Only the main chat window takes the tell: the smaller windows beside it are left alone.
pub fn only_the_main_window_takes_the_tell() {
    let mut hand = Hand::new();
    let mut c = a_client_with_two_friends(&mut hand);
    let (list, row) = {
        let p = &c.view().expect_app().hud().panels.friends;
        (p.list.as_ref().expect("bound").handle, p.rows()[0].element)
    };
    hand.click_row(&mut c, list, row);
    let tell = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .tell_button
        .expect("bound");
    hand.click_handle(&mut c, tell);

    let main = main_chat_entry(&c);
    let filled = element_text_of(&mut c, main) == "@tell Ash, ";
    let others: Vec<ElemHandle> = with_screen(&mut c, |_, s| {
        s.chat_windows
            .iter()
            .skip(1)
            .filter_map(|w| w.entry)
            .collect()
    });
    let there_are_others = !others.is_empty();
    let mut untouched = true;
    for h in others {
        untouched &= element_text_of(&mut c, h).is_empty();
    }

    c.assert_behaviour("chat.tell.only-the-main-window-takes-the-tell", move |_| {
        filled && there_are_others && untouched
    });
    c.shutdown();
}

#[test]
fn scenario_only_the_main_window_takes_the_tell() {
    scenario("only_the_main_window_takes_the_tell");
}

// ---------------------------------------------------------------------------------------------
// What the log draws, and in what colour
// ---------------------------------------------------------------------------------------------

/// Two recorded spoken lines of **different kinds**, one from somebody who is a player and one
/// from somebody who is not -- found by walking the recordings rather than named.
///
/// The recordings carry no private messages: what one player says to another is private and the
/// public recordings are scrubbed of it. So the two kinds this compares are two kinds of spoken
/// line, which is what the claim needs -- the colour has to be chosen by the kind.
fn two_recorded_lines_of_different_kinds() -> [dereth_protocol::comms::CommunicationHearSpeech; 2] {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    let mut plain: Option<dereth_protocol::comms::CommunicationHearSpeech> = None;
    let mut clickable: Option<dereth_protocol::comms::CommunicationHearSpeech> = None;
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient
                || b.opcode != dereth_protocol::Opcode::COMMUNICATION_HEAR_SPEECH.0
                || b.payload.len() < 4
            {
                continue;
            }
            let Ok(m) = dereth_protocol::read_body::<dereth_protocol::comms::CommunicationHearSpeech>(
                &b.payload[4..],
            ) else {
                continue;
            };
            if m.message.is_empty() || m.sender_name.is_empty() {
                continue;
            }
            let is_player = dereth_client::chat::CLICKABLE_PLAYER_IDS.contains(&m.sender_id.0);
            if is_player {
                if clickable.is_none() {
                    clickable = Some(m);
                }
            } else if plain.is_none() {
                plain = Some(m);
            }
            if let (Some(p), Some(c)) = (plain.as_ref(), clickable.as_ref()) {
                if p.text_type != c.text_type {
                    break;
                }
                // The two so far are the same kind of line, which would prove nothing about the
                // colour being chosen; keep looking for one of another kind.
                if is_player {
                    clickable = None;
                } else {
                    plain = None;
                }
            }
        }
    }
    let plain = plain.expect("a recorded line from somebody who is not a player");
    let clickable = clickable.expect("a recorded line from somebody who is");
    assert_ne!(
        plain.text_type, clickable.text_type,
        "the two recorded lines must be of different kinds, or the colour proves nothing"
    );
    [plain, clickable]
}

/// Two lines of different kinds, both recorded, drawn on the shipped log in two different colours
/// -- which is what shows the colour is chosen by the kind of line rather than being one colour
/// for everything.
pub fn two_recorded_channels_draw_in_two_different_colours() {
    let [plain, clickable] = two_recorded_lines_of_different_kinds();
    let plain_body = dereth_client::chat::hear_speech_line(
        plain.sender_id.0,
        None,
        &plain.sender_name,
        &plain.message,
    );
    let clickable_body = dereth_client::chat::hear_speech_line(
        clickable.sender_id.0,
        None,
        &clickable.sender_name,
        &clickable.message,
    );
    // One is drawn plainly and the other with a name the player can click, so between them both
    // forms of a line reach the log.
    let two_forms = !plain_body.starts_with('<') && clickable_body.starts_with("<Tell:IIDString:");
    let plain_visible = format!("{} says, \"{}\"", plain.sender_name, plain.message);
    let clickable_visible = format!("{} says, \"{}\"", clickable.sender_name, clickable.message);

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let cases: [(u8, &str, &str, &str, &str); 2] = [
        (
            u8::try_from(plain.text_type).expect("a kind of line"),
            &plain_body,
            &plain_visible,
            &plain.sender_name,
            &plain.message,
        ),
        (
            u8::try_from(clickable.text_type).expect("a kind of line"),
            &clickable_body,
            &clickable_visible,
            &clickable.sender_name,
            &clickable.message,
        ),
    ];
    let mut every = true;
    let mut colours: Vec<u32> = Vec::new();
    for (ty, body, visible, name, words) in cases {
        let want = OPAQUE | dereth_ui_screens::chat::colors::color_for_type(ty).hex;
        let before = colour_runs(&mut c).len();
        let line = dereth_ui_screens::chat::interface::ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
            ty,
            body: (*body).to_owned(),
            prefix: None,
            window: 0,
        };
        let took = with_screen(&mut c, |ui, s| s.recv_display_final_string_info(ui, &line));
        let reached = !took.is_empty() && colour_runs(&mut c).len() > before;

        let all = colour_runs(&mut c);
        let joined: String = all.iter().map(|(s, _)| s.as_str()).collect();
        let drawn = joined.contains(visible)
            && !joined.contains("<Tell:IIDString:")
            // The name is followed by the client's own verb on the element itself.
            && joined.contains(&format!("{name} says, \""));

        // The colour, apart from the letters: the run carrying the verb is this kind of line's
        // colour, and not the grey the timestamp is drawn in.
        let is_clickable = body.starts_with("<Tell:IIDString:");
        // Found by this line's own words, so it cannot read the other line's run.
        let run = all
            .iter()
            .find(|(s, _)| s.contains(words))
            .unwrap_or_else(|| panic!("this line's words are one run; got {all:?}"));
        // The one kind of line that really is drawn grey is an emote, so the check that the name
        // is not in the timestamp's grey is made for every other kind.
        let coloured = run.1 == want
            && (ty == 12 || run.1 != OPAQUE | dereth_ui_screens::chat::colors::GREY.hex);
        // And a clickable name is drawn in the colour a clickable name is drawn in, which is not
        // the line's own.
        let name_coloured = !is_clickable
            || all
                .iter()
                .rev()
                .find(|(s, _)| s.contains(name))
                .is_some_and(|(_, col)| *col == TAG_COLOUR && *col != want);
        assert!(
            reached && drawn && coloured && name_coloured,
            "the {ty} line: reached={reached} drawn={drawn} coloured={coloured} name={name_coloured}; runs {all:?}"
        );
        every &= reached && drawn && coloured && name_coloured;
        colours.push(want);
    }

    // The two kinds really differed **on the element**, which one colour for everything could not
    // survive.
    let all = colour_runs(&mut c);
    let on_the_element: std::collections::BTreeSet<u32> = all
        .iter()
        .filter(|(s, _)| s.contains(&plain.message) || s.contains(&clickable.message))
        .map(|(_, col)| *col)
        .collect();
    let differ = colours[0] != colours[1]
        && on_the_element
            == colours
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<u32>>();

    c.assert_behaviour(
        "chat.log.two-recorded-kinds-of-line-draw-in-two-different-colours",
        move |_| two_forms && every && differ,
    );
    c.shutdown();
}

#[test]
fn scenario_two_recorded_channels_draw_in_two_different_colours() {
    scenario("two_recorded_channels_draw_in_two_different_colours");
}

/// The player's own line comes back to him in one form and somebody else's in another, and both
/// reach the log in the colour a spoken line is drawn in -- never the name and the words run
/// together with no verb between them.
pub fn your_own_echo_and_a_remote_speaker_are_drawn_apart() {
    const ME_HERE: u32 = 0x5000_1234;
    const NOT_A_PLAYER: u32 = 0x8000_0DE9;

    let kind = u8::try_from(dereth_client_model::chat::text_type::SPEECH).expect("a kind of line");
    let want = OPAQUE | dereth_ui_screens::chat::colors::color_for_type(kind).hex;

    let echo = dereth_client::chat::hear_speech_line(ME_HERE, Some(ME_HERE), "Lark", "W");
    let remote = dereth_client::chat::hear_speech_line(
        NOT_A_PLAYER,
        Some(ME_HERE),
        "Sparring Golem",
        "Have at you!",
    );
    let two_forms = echo == "You say, \"W\"" && remote == "Sparring Golem says, \"Have at you!\"";

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    for body in [&echo, &remote] {
        let line = dereth_ui_screens::chat::interface::ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
            ty: kind,
            body: body.clone(),
            prefix: None,
            window: 0,
        };
        let took = with_screen(&mut c, |ui, s| s.recv_display_final_string_info(ui, &line));
        assert!(!took.is_empty(), "{body:?} reached a window");
    }

    let all = colour_runs(&mut c);
    let joined: String = all.iter().map(|(s, _)| s.as_str()).collect();
    let both_drawn =
        joined.contains(&echo) && joined.contains(&remote) && !joined.contains("LarkW");
    let mut coloured = true;
    for body in [&echo, &remote] {
        coloured &= all
            .iter()
            .find(|(s, _)| s.contains(body.as_str()))
            .is_some_and(|(_, col)| *col == want);
    }

    c.assert_behaviour(
        "chat.speech.your-own-echo-and-a-remote-speaker-are-drawn-from-two-different-forms",
        move |_| two_forms && both_drawn && coloured,
    );
    c.shutdown();
}

#[test]
fn scenario_your_own_echo_and_a_remote_speaker_are_drawn_apart() {
    scenario("your_own_echo_and_a_remote_speaker_are_drawn_apart");
}

// ---------------------------------------------------------------------------------------------
// The chat surface: the return key, the keys that move the log, the menu, and the small windows
// ---------------------------------------------------------------------------------------------

/// The keys the chat window answers, by the number it knows each by.
const ACTION_ENTER: u32 = 0x25;
const ACTION_HOME: u32 = 0x1C;
const ACTION_END: u32 = 0x1D;
const ACTION_HISTORY_BACK: u32 = 0x1E;
const ACTION_PAGE_UP: u32 = 0x20;
const ACTION_ESCAPE: u32 = 0x27;
/// The key that opens the entry, and the three that reply to somebody.
const ACTION_OPEN_ENTRY: u32 = 0x1000_0023;
const ACTION_REPLY_MONARCH: u32 = 0x1000_0020;
const ACTION_REPLY_PATRON: u32 = 0x1000_0021;
const ACTION_REPLY_LAST: u32 = 0x1000_0022;

/// One key the chat window is asked about.
fn an_action(action: u32) -> dereth_ui::focus::InputEvent {
    dereth_ui::focus::InputEvent {
        action,
        start: true,
        x: 0,
        y: 0,
    }
}

/// The line the return key sends is taken out of the box, remembered, and raised for the window it
/// was typed in -- and the key that opens the entry gives it the caret back.
pub fn the_line_the_return_key_sends_is_remembered() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    hand.click_handle(&mut c, entry);
    let focused = focus_of(&c) == Some(entry);
    hand.type_text(&mut c, "hail well met");
    let typed = element_text_of(&mut c, entry) == "hail well met";

    c.ui_outbox().clear();
    hand.press_return(&mut c);
    let emptied = element_text_of(&mut c, entry).is_empty();
    let remembered = chat_history(&mut c) == ["hail well met"]
        && with_screen(&mut c, |_, s| {
            !s.chat
                .iter()
                .find(|w| w.window_id == 8)
                .unwrap()
                .chat_entry_active
        });

    // The key that opens the entry gives it the caret back, which is the other way in.
    let opened = with_screen(&mut c, |ui, s| s.chat_on_action(ui, ACTION_OPEN_ENTRY));
    // One frame, because whether the keyboard goes to the box is read when the character arrives
    // and the shell mirrors the new caret into it on the next.
    c.tick(1);
    hand.type_text(&mut c, "second line");
    let back = focus_of(&c) == Some(entry);
    // The frame drains what the window asked for before a scenario could look at it, so the ask
    // is read by running the same arm the key runs, with the queue in hand.
    c.ui_outbox().clear();
    let consumed = with_screen(&mut c, |ui, s| {
        s.chat_on_child_action(ui, entry, &an_action(ACTION_ENTER))
    });
    let raised = c.ui_outbox().take()
        == vec![dereth_client_contract::UiRequest::ChatLine {
            text: "second line".to_owned(),
            window: 8,
        }];

    c.assert_behaviour(
        "chat.entry.the-line-the-return-key-sends-is-remembered-and-raised-for-its-own-window",
        move |_| focused && typed && emptied && remembered && opened && back && consumed && raised,
    );
    c.shutdown();
}

#[test]
fn scenario_the_line_the_return_key_sends_is_remembered() {
    scenario("the_line_the_return_key_sends_is_remembered");
}

/// The keys that move the log move the log, and the keys that walk the history fill the entry --
/// two different places, and telling them apart is the point.
pub fn the_scroll_keys_move_the_log_and_the_history_keys_fill_the_entry() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    let log = element(&c, dereth_ui_screens::chat::window::LOG);
    hand.click_handle(&mut c, entry);

    // Enough in the window that the log has somewhere to go.
    for i in 0..12 {
        let m = dereth_ui_screens::chat::interface::ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
            ty: 0,
            body: format!("line number {i} of the backlog"),
            prefix: None,
            window: 0,
        };
        with_screen(&mut c, |ui, s| s.recv_display_final_string_info(ui, &m));
    }
    c.tick(1);

    let scroll_y = |c: &mut HeadlessClient| {
        with_screen(c, |ui, _| {
            ui.text_element_mut(log).map_or(0, |t| t.scroll.y)
        })
    };
    let at_end = scroll_y(&mut c);
    let has_travel = at_end > 0;

    let home = with_screen(&mut c, |ui, s| {
        s.chat_on_child_action(ui, entry, &an_action(ACTION_HOME))
    });
    let to_the_top = home && scroll_y(&mut c) == 0;
    let end = with_screen(&mut c, |ui, s| {
        s.chat_on_child_action(ui, entry, &an_action(ACTION_END))
    });
    let to_the_bottom = end && scroll_y(&mut c) == at_end;
    let page = with_screen(&mut c, |ui, s| {
        s.chat_on_child_action(ui, entry, &an_action(ACTION_PAGE_UP))
    });
    let paged = page && scroll_y(&mut c) < at_end;

    // The history keys write into the **entry**, not the log.
    for text in ["first", "second"] {
        c.world_mut().chat.entries.entry(8).or_default().submit(
            text,
            dereth_client_contract::options::interface::Interface::Retail,
        );
    }
    let back = with_screen(&mut c, |ui, s| {
        s.chat_on_child_action(ui, entry, &an_action(ACTION_HISTORY_BACK))
    });
    c.tick(1);
    let filled = back && element_text_of(&mut c, entry) == "second";

    // And the key that gives up: the caret goes, and the text stays.
    let escaped = with_screen(&mut c, |ui, s| {
        let took = s.chat_on_child_action(ui, entry, &an_action(ACTION_ESCAPE));
        (took, ui.focus_element())
    }) == (true, None);

    c.assert_behaviour(
        "chat.window.the-keys-that-move-the-log-and-the-keys-that-walk-the-history-are-different",
        move |_| has_travel && to_the_top && to_the_bottom && paged && filled && escaped,
    );
    c.shutdown();
}

#[test]
fn scenario_the_scroll_keys_move_the_log_and_the_history_keys_fill_the_entry() {
    scenario("the_scroll_keys_move_the_log_and_the_history_keys_fill_the_entry");
}

/// Run one typed line through the client's own frame with the talk focus set, and answer with what
/// it produced.
fn line_with_focus(focus: u32, text: &str) -> Vec<dereth_client_model::Request> {
    let store = dereth_dat::testing::open_store().expect("the retail data is the client's own");
    let mut inter = dereth_client::interaction::Interaction::new();
    let mut objects = dereth_client::objects::ObjectStream::new();
    inter.queue(
        Vec::new(),
        vec![
            dereth_client_contract::UiRequest::SetTalkFocus { focus },
            dereth_client_contract::UiRequest::ChatLine {
                text: text.to_owned(),
                window: 8,
            },
        ],
    );
    let (unowned, _) = dereth_client::interaction::use_time(
        &mut inter,
        &store,
        None,
        &mut objects,
        None,
        Vec::new(),
        false,
        (800, 600),
        dereth_primitives::LocalTime(1.0),
    );
    assert!(
        unowned.is_empty(),
        "focus {focus}: nobody owned {unowned:?}"
    );
    inter.last_sent.clone()
}

/// What a typed line becomes follows the talk-to menu: an ordinary spoken line, or a line on one
/// of four channels -- and the row that talks to whoever is selected sends nothing at all when
/// nobody is.
pub fn what_a_typed_line_becomes_follows_the_menu() {
    use dereth_client_model::Request;
    let talk = |m: &str| {
        Request::Talk(dereth_protocol::comms::CommunicationTalk {
            message: m.to_owned(),
        })
    };
    let channel = |ch: u32, m: &str| {
        Request::ChannelBroadcast(dereth_protocol::comms::CommunicationChannelBroadcast {
            channel: ch,
            message: m.to_owned(),
        })
    };
    // **The channels do not follow the menu's own naming**, which is why each is named here: the
    // fellowship row carries one number and the vassals row another, and getting them the wrong
    // way round sends a line where everybody but the player can read it.
    let cases: [(u32, Option<Request>); 6] = [
        (1, Some(talk("well met"))),
        // The row that talks to whoever is selected, with nobody selected: dropped rather than
        // said out loud, which is what stops a private line becoming a public one.
        (2, None),
        (3, Some(channel(0x800, "well met"))),
        (4, Some(channel(0x2000, "well met"))),
        (5, Some(channel(0x4000, "well met"))),
        (6, Some(channel(0x1000, "well met"))),
    ];
    let mut every = true;
    for (focus, want) in cases {
        let got = line_with_focus(focus, "well met");
        let holds = match &want {
            Some(w) => got == vec![w.clone()],
            None => got.is_empty(),
        };
        assert!(holds, "focus {focus}: got {got:?}");
        every &= holds;
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.talk-focus.what-a-typed-line-becomes-follows-the-menu",
        move |_| every,
    );
}

#[test]
fn scenario_what_a_typed_line_becomes_follows_the_menu() {
    scenario("what_a_typed_line_becomes_follows_the_menu");
}

/// A line the client handles itself is never spoken to the shard, whichever channel the menu is
/// on -- saying a command out loud on a channel everybody reads would be both wrong and rude.
pub fn a_command_the_client_handles_is_never_spoken() {
    let mut every = true;
    for focus in [1_u32, 3, 4, 5, 6] {
        let got = line_with_focus(focus, "@help");
        assert!(got.is_empty(), "focus {focus}: got {got:?}");
        every &= got.is_empty();
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.commands.a-command-the-client-handles-is-never-spoken-to-the-shard",
        move |_| every,
    );
}

#[test]
fn scenario_a_command_the_client_handles_is_never_spoken() {
    scenario("a_command_the_client_handles_is_never_spoken");
}

/// The caption on the talk-to button follows the menu, a row that is shut refuses to be picked,
/// and a row that is not there at all does nothing.
pub fn the_button_caption_follows_the_menu() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let caption_el = element(
        &c,
        dereth_ui_screens::chat::mainchat::CHAT_TARGET_BUTTON_TEXT,
    );
    let caption = |c: &mut HeadlessClient| element_text_of(c, caption_el);
    let default = caption(&mut c);
    let started_on_one = with_screen(&mut c, |_, s| (s.main_chat.talk_focus, s.main_chat.caption))
        == (1, "ID_Chat_ChatTargetMenu")
        && !default.is_empty();

    // A channel row starts shut, and picking it is refused.
    let refused = with_screen(&mut c, |ui, s| s.main_chat.handle_selection(ui, 8)).is_none()
        && caption(&mut c) == default;

    // Opened, the same pick takes.
    let taken = with_screen(&mut c, |ui, s| {
        s.main_chat.set_talk_focus_enabled(8, true);
        // Turning the channel on is the chat system's own flag and touches no row; putting the
        // row's own state back is a separate act, and it is what the pick is refused on.
        s.main_chat.reset_all_talk_focus_menu_buttons(ui);
        s.main_chat.handle_selection(ui, 8)
    })
    .is_some_and(|change| {
        change.focus == 8
            && change.caption == "ID_Chat_ChatTargetMenuGeneral"
            && !change.wants_alleg_chat
    });
    let general = caption(&mut c);
    let followed = general != default;

    // One row does something else as well: it asks to be in the allegiance's chat.
    let allegiance = with_screen(&mut c, |ui, s| {
        s.main_chat.set_talk_focus_enabled(7, true);
        s.main_chat.reset_all_talk_focus_menu_buttons(ui);
        let change = s
            .main_chat
            .handle_selection(ui, 7)
            .expect("the allegiance row");
        change.wants_alleg_chat
    }) && caption(&mut c) != general;

    // A row the menu does not have at all: nothing happens.
    let absent = with_screen(&mut c, |ui, s| s.main_chat.handle_selection(ui, 99)).is_none();

    c.assert_behaviour(
        "chat.talk-to-menu.the-button-caption-follows-the-menu-and-a-row-that-is-shut-refuses",
        move |_| started_on_one && refused && taken && followed && allegiance && absent,
    );
    c.shutdown();
}

#[test]
fn scenario_the_button_caption_follows_the_menu() {
    scenario("the_button_caption_follows_the_menu");
}

/// Which channel a row of the talk-to menu is, is carried by the row itself rather than by where
/// it sits in the list -- the list is not in the channels' own order, and the first row is not a
/// channel at all.
pub fn a_rows_place_in_the_list_is_not_the_order_of_the_channels() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.ui_outbox().clear();

    // The first row is the one that silences somebody and is not a channel.
    let first_is_not_a_channel =
        with_screen(&mut c, |ui, s| s.chat_target_menu_selection(ui, 0)).is_none();

    let picked = with_screen(&mut c, |ui, s| {
        s.main_chat.set_talk_focus_enabled(5, true);
        s.main_chat.reset_all_talk_focus_menu_buttons(ui);
        s.chat_target_menu_selection(ui, 1)
    })
    .is_some_and(|change| change.focus == 5 && change.caption == "ID_Chat_ChatTargetMenuMonarch");
    let asked =
        c.ui_outbox().take() == vec![dereth_client_contract::UiRequest::SetTalkFocus { focus: 5 }];

    c.assert_behaviour(
        "chat.talk-to-menu.which-channel-a-row-is-comes-from-the-row-and-not-its-place",
        move |_| first_is_not_a_channel && picked && asked,
    );
    c.shutdown();
}

#[test]
fn scenario_a_rows_place_in_the_list_is_not_the_order_of_the_channels() {
    scenario("a_rows_place_in_the_list_is_not_the_order_of_the_channels");
}

/// The three reply keys address three different people, and each addresses his own -- getting them
/// the wrong way round would send a private reply to the wrong person.
pub fn the_reply_keys_address_three_different_people() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    c.world_mut().chat.last_teller_name = "Ipsum".into();
    c.world_mut().chat.last_monarch_sender = "Amaranthea".into();
    c.world_mut().chat.last_patron_sender = "Borelean".into();
    let template = dereth_ui_screens::chat::window::ASSISTED_TELL_FALLBACK;

    let mut every = true;
    for (action, who) in [
        (ACTION_REPLY_LAST, "Ipsum"),
        (ACTION_REPLY_MONARCH, "Amaranthea"),
        (ACTION_REPLY_PATRON, "Borelean"),
    ] {
        let took = with_screen(&mut c, |ui, s| s.chat_on_action(ui, action));
        c.tick(1);
        let took = took && focus_of(&c) == Some(entry);
        let addressed = element_text_of(&mut c, entry) == format!("{template}{who}, ");
        assert!(took && addressed, "{action:#X} should address {who}");
        every &= took && addressed;
    }

    // With nobody to reply to, the key is still taken and the box is left alone.
    c.world_mut().chat.last_teller_name.clear();
    c.world_mut().chat.last_monarch_sender.clear();
    c.world_mut().chat.last_patron_sender.clear();
    let before = element_text_of(&mut c, entry);
    let still_taken = with_screen(&mut c, |ui, s| s.chat_on_action(ui, ACTION_REPLY_LAST));
    c.tick(1);
    let left_alone = still_taken && element_text_of(&mut c, entry) == before;

    c.assert_behaviour(
        "chat.tell.the-three-reply-keys-address-three-different-people",
        move |_| every && left_alone,
    );
    c.shutdown();
}

#[test]
fn scenario_the_reply_keys_address_three_different_people() {
    scenario("the_reply_keys_address_three_different_people");
}

/// A small chat window writes back where it was put, how big it is, whether it is open and what it
/// is called -- so a player's arrangement survives him logging out.
pub fn a_floaty_window_writes_its_place_and_title_back() {
    use dereth_ui_screens::chat::floaty::placement as p;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.ui_outbox().clear();
    let ids: Vec<u32> = with_screen(&mut c, |_, s| {
        assert_eq!(s.floaty_chat.len(), 4, "the four small chat windows");
        s.floaty_chat.iter().map(|w| w.window_id).collect()
    });
    let four = ids == vec![2, 3, 4, 5];

    with_screen(&mut c, |ui, s| {
        let w = s.floaty_chat[0].clone();
        w.move_to(ui, 120, 340);
        w.resize_to(ui, 300, 150);
        w.set_visible(ui, false);
    });
    let written = c.ui_outbox().take()
        == vec![
            dereth_client_contract::UiRequest::SetChatWindowOption {
                window: 2,
                property: p::X,
                value: 120,
            },
            dereth_client_contract::UiRequest::SetChatWindowOption {
                window: 2,
                property: p::Y,
                value: 340,
            },
            dereth_client_contract::UiRequest::SetChatWindowOption {
                window: 2,
                property: p::WIDTH,
                value: 300,
            },
            dereth_client_contract::UiRequest::SetChatWindowOption {
                window: 2,
                property: p::HEIGHT,
                value: 150,
            },
            dereth_client_contract::UiRequest::SetChatWindowOption {
                window: 2,
                property: p::VISIBILITY,
                value: 0,
            },
        ];

    // A new title is broadcast and taken by the one window it names, and that window alone.
    let title = dereth_ui_screens::view::ChatWindowTitle::Table {
        string_id: dereth_ui::persist::preferences::token_of("ID_Chat_Chat3_DefaultTitle"),
        table_id: dereth_ui_screens::chat::mainchat::CAPTION_STRING_TABLE.0,
    };
    let titled = with_screen(&mut c, |ui, s| {
        let took = s.on_set_chat_window_title(ui, 4, title.clone());
        (
            took,
            s.floaty_chat[0].title.clone(),
            s.floaty_chat[2].title.clone(),
        )
    }) == (true, None, Some(title));
    let sent = c.ui_outbox().take();
    let title_written = sent.len() == 1
        && matches!(
            sent[0],
            dereth_client_contract::UiRequest::SetChatWindowTitle { window: 4, .. }
        );

    c.assert_behaviour(
        "chat.window.a-small-window-writes-its-place-size-openness-and-title-back",
        move |_| four && written && titled && title_written,
    );
    c.shutdown();
}

#[test]
fn scenario_a_floaty_window_writes_its_place_and_title_back() {
    scenario("a_floaty_window_writes_its_place_and_title_back");
}

/// The close button shuts exactly its own window and writes that down, so a window a player closed
/// stays closed when he comes back.
pub fn the_close_button_hides_its_own_window() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.ui_outbox().clear();
    let (root, close) = with_screen(&mut c, |_, s| {
        let w = &s.floaty_chat[1];
        (
            w.root.expect("the window's root"),
            w.close_button.expect("the close button"),
        )
    });
    let closed = with_screen(&mut c, |ui, s| {
        let which = s.chat_on_close_button(ui, close);
        (which, ui.node(root).is_some_and(|n| n.region.flags.visible))
    }) == (Some(3), false);
    let written = c.ui_outbox().take()
        == vec![dereth_client_contract::UiRequest::SetChatWindowOption {
            window: 3,
            property: dereth_ui_screens::chat::floaty::placement::VISIBILITY,
            value: 0,
        }];

    c.assert_behaviour(
        "chat.window.the-close-button-shuts-its-own-window-and-writes-that-down",
        move |_| closed && written,
    );
    c.shutdown();
}

#[test]
fn scenario_the_close_button_hides_its_own_window() {
    scenario("the_close_button_hides_its_own_window");
}

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
