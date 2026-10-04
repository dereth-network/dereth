//! The DAT shell scenarios, driven through physical input and shipped layouts.
//! Subject modules own their scenarios and fixture helpers. The declarations remain here so
//! wrapper names and behavior stations stay stable. Run this binary serially: headless clients
//! share UI request globals.

mod bindings;
mod character_select;
pub(super) mod chargen;
mod chargen_appearance;
mod chargen_dialogs;
mod chargen_name;
mod chargen_scroll;
mod chargen_skills;
mod dialog_keys;
mod input;
mod intro;
mod layout;
mod login;
mod options;
mod pointer;
mod relog;
mod text;
mod viewport;
mod wizard;

use bindings::{
    a_capture_in_flight_swallows_the_key, a_cell_shows_the_key_the_way_the_desktop_names_it,
    a_key_already_in_use_asks_first_and_one_that_cannot_be_taken_refuses,
    a_key_in_use_given_to_this_clients_action_asks_first_and_is_taken,
    a_key_pressed_over_a_row_rebinds_it,
    a_press_on_a_cell_waits_for_a_key_and_undo_puts_the_old_one_back,
    a_set_of_keys_can_be_saved_under_a_name_and_loaded_back,
    each_of_this_clients_actions_works_on_a_key_the_page_gives_it,
    every_section_is_titled_in_words, kb_text, kb_ui,
    resting_on_a_cell_says_what_a_press_there_would_do,
    restoring_the_defaults_gives_back_the_shipped_keys,
    saving_over_a_set_asks_first_and_one_that_cannot_be_written_refuses,
    taking_a_key_clears_it_from_the_rows_that_had_it,
    the_key_binding_page_builds_one_row_per_bindable_action,
    the_questions_about_a_key_in_use_are_the_shipped_sentences,
    undo_opens_greyed_because_nothing_has_changed_yet, KeyHand,
};
use character_select::{
    a_double_press_raises_the_waiting_box_before_it_asks_to_log_on,
    an_error_brought_in_with_the_screen_becomes_a_one_button_message,
    answering_yes_to_leaving_runs_the_closing_sequence_and_no_stays,
    cancelling_after_typing_the_phrase_deletes_nothing, charmgmt_child, charmgmt_dialog,
    charmgmt_row, charmgmt_word, deleting_asks_about_the_character_that_was_picked,
    leaving_raises_a_modal_question_in_the_shipped_words,
    one_press_on_a_character_picks_it_and_two_takes_them_into_the_world,
    only_the_typed_phrase_deletes_and_it_deletes_the_one_that_was_picked,
    restoring_raises_a_box_with_no_buttons_that_the_next_list_takes_down,
    the_character_lists_own_buttons_each_raise_what_they_name,
    the_question_is_modal_and_a_press_behind_it_reaches_nothing, DELETE_BUTTON_ID, LIST_CREATE,
    LIST_CREDITS, LIST_EXIT, RESTORE_BUTTON_ID,
};
use chargen_appearance::{
    chargen_tables, clicking_a_colour_moves_the_marker_and_tints_the_shade_wheel, goto_page,
    the_colour_spots_are_the_chosen_parts_own_colours,
    the_eyes_have_one_colour_each_and_no_shade_to_slide,
    the_face_tab_takes_the_hat_off_and_the_clothes_tab_puts_it_back,
    the_home_town_follows_the_people, the_insect_peoples_lose_three_tabs_outright,
    the_last_page_warns_before_re_rolling_the_whole_character,
    the_opening_people_is_one_a_plain_account_may_play,
    the_opening_roll_is_the_seeds_own_and_two_clients_roll_the_same_character,
    the_people_and_the_sex_come_out_of_their_own_stream,
    the_random_button_re_rolls_the_page_the_player_is_on, the_town_pages_roll_can_land_on_any_town,
    the_wizard_opens_on_a_character_already_rolled,
};
use chargen_dialogs::{
    a_box_answered_from_outside_the_screen_comes_down,
    a_box_closed_from_underneath_the_screen_is_replaced,
    a_people_the_account_cannot_play_is_refused_in_a_message_box,
    a_second_box_waits_its_turn_rather_than_stacking, click_in_dialog,
    finishing_with_credits_unspent_asks_first, finishing_with_no_name_refuses_in_a_message_box,
    only_the_last_page_asks_before_re_rolling,
    the_re_roll_warning_is_a_modal_question_in_the_shipped_words,
    the_warnings_two_buttons_answer_it_and_a_pointer_can_press_either,
    the_wizard_never_shows_a_please_wait_box, wizard_dialog,
};
use chargen_name::{
    a_press_in_the_name_box_puts_the_caret_where_it_was_pressed,
    a_press_on_nothing_takes_the_keyboard_away_and_a_press_back_in_returns_it,
    a_press_that_types_nothing_leaves_a_plain_caret, a_wizard_on_the_summary_page,
};
use chargen_scroll::{
    a_filled_pane_has_a_live_bar_sized_to_what_is_shown,
    a_pane_measures_its_whole_paragraph_and_the_blank_row_after_it,
    a_press_on_the_track_moves_a_whole_page_towards_the_press,
    an_arrow_moves_one_row_and_the_two_go_opposite_ways,
    dragging_the_thumb_moves_the_rows_by_the_same_fraction, each_pane_drives_the_bar_beside_it,
    the_wheel_moves_a_list_one_row_and_stops_at_the_top,
    three_panes_and_two_lists_scroll_and_the_town_page_has_none,
};
use chargen_skills::{
    every_row_is_drawn_under_its_new_heading_and_back_again,
    every_row_shows_its_own_score_and_its_own_two_prices,
    the_four_headings_sit_above_their_own_groups, the_reported_press_moves_the_row_it_names,
    the_skill_rows_are_stacked_and_none_overlaps,
    the_skills_page_reads_the_way_the_original_drew_it,
    training_a_skill_moves_its_row_and_re_prices_the_rest,
};
use dialog_keys::{
    a_box_that_takes_typing_adds_the_dialog_map_without_taking_the_keys,
    a_client_on_character_select, a_key_the_roll_does_not_own_leaves_it_running,
    a_real_press_of_enter_advances_twice, either_key_ends_the_credit_roll_and_ends_it_once,
    enter_advances_the_opening_sequence_and_escape_leaves_it,
    enter_is_declined_on_the_character_list_and_escape_asks_once, map_stack,
    only_the_opening_sequence_refuses_a_key_on_its_way_back_up,
    the_dialog_map_binds_only_escape_and_enter,
    the_opening_screen_answers_only_the_keys_its_own_maps_carry, three_characters,
    with_charmgmt_screen,
};
use input::{
    a_backspace_tap_deletes_one_and_nothing_follows, a_cleared_target_comes_back,
    a_modified_number_uses_a_quick_slot_and_not_the_chat_window,
    a_rebind_is_written_beside_the_preferences, a_rebound_key_walks_the_body_and_the_old_one_stops,
    an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing, control,
    each_panel_function_key_opens_and_shuts_its_page,
    every_function_key_does_what_the_shipped_map_says, forward, held_moves, key,
    one_backspace_deletes_one_character_and_a_hold_repeats,
    opening_the_chat_bar_with_a_key_swallows_its_own_character,
    the_armed_swallow_eats_exactly_one_character,
    the_chat_map_is_walked_first_and_movement_before_the_camera,
    the_desktops_four_are_taken_last_and_do_nothing, the_fellow_cycle_follows_the_panels_order,
    the_key_that_goes_back_is_a_toggle_and_not_a_stack,
    the_left_button_and_the_wheel_each_belong_to_one_shipped_map,
    the_two_keys_walk_the_fellowship_both_ways_and_wrap,
    the_walk_mode_key_follows_the_run_by_default_option,
    the_walk_mode_option_is_read_on_every_press,
    the_window_keeps_the_switcher_and_passes_the_other_two,
    with_nothing_behind_it_the_key_selects_nothing, MOVEMENT, MOVE_FORWARD, SCAN_FREE, SCAN_W,
};
use intro::{
    a_click_advances_one_picture_and_the_release_is_not_another, a_client_on_the_intro,
    any_character_advances_and_escape_skips, clicking_through_the_intro_ends_at_character_select,
    current_screen, intro_state, the_quit_action_skips_the_rest_of_the_intro,
};
use layout::{
    a_change_before_the_description_survives_it,
    a_saved_layout_is_clamped_and_moves_only_what_it_names,
    moving_a_window_is_remembered_across_a_rebuild,
    the_full_screen_setting_is_kept_and_applied_on_entering_the_world,
    the_full_screen_switch_key_is_refused_outside_the_world,
    the_options_page_turns_full_screen_on_and_off_mid_session,
};
use login::{
    a_list_arriving_in_the_world_sends_the_player_back_and_at_the_list_does_not,
    a_pending_deletion_is_drawn_red_and_last_and_offers_restore,
    a_pick_the_returning_list_cannot_honour_falls_back_to_the_shards_own_order,
    a_refused_creation_stops_the_wizard_waiting_and_says_why,
    a_refused_restore_takes_the_waiting_box_down_and_says_why,
    a_restored_character_comes_back_in_its_own_place_and_in_the_ordinary_colour,
    answering_yes_asks_to_log_off_and_moves_no_screen_of_its_own,
    deleting_and_restoring_over_the_wire_leaves_the_list_where_it_started,
    every_character_set_the_session_decodes_is_an_arrival,
    every_refusal_the_shard_can_send_draws_its_own_sentence,
    the_delete_that_leaves_the_client_carries_the_account_and_the_shards_own_place,
    the_restore_answer_overwrites_the_place_it_was_asked_about_and_never_appends,
    the_restore_that_leaves_the_client_is_the_characters_own_id_on_the_control_queue,
    the_returning_character_list_selects_the_character_that_was_just_played,
    the_shards_log_off_answer_does_not_end_a_client_with_screens,
    the_way_into_the_world_opens_again_after_a_log_off,
};
use options::{
    a_client_with_settings, a_deferred_change_is_flushed_by_the_frame,
    a_partly_chosen_group_is_drawn_differently, a_tick_changes_only_that_setting,
    a_ticked_row_stays_ticked_and_unticking_still_works,
    apply_cancel_and_defaults_do_what_an_option_page_does,
    auto_accept_from_the_default_clears_ignoring_at_the_shard, drawn_tick,
    every_row_shows_the_setting_the_shard_sent,
    one_fellowship_setting_turns_the_other_off_at_the_shard,
    one_tick_and_one_visit_sends_one_message, open_the_character_options_page,
    the_chat_options_tab_draws_its_controls,
    the_excluded_row_goes_out_at_once_and_cancel_restores_both,
    the_opacity_slider_writes_what_the_fade_reads,
    the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones,
    ticking_a_filter_reaches_the_window_and_sends_nothing, with_gameplay,
    with_no_description_nothing_is_sent,
};
use pointer::{
    a_button_lights_under_the_pointer_and_sinks_under_the_press,
    a_detent_over_a_tick_box_scrolls_the_list,
    a_move_makes_what_is_under_the_pointer_the_one_entered,
    a_press_dragged_off_a_button_releases_it_without_firing_it,
    a_press_on_a_toolbar_button_opens_the_panel_it_owns,
    the_same_box_still_answers_a_click_and_so_does_the_bar,
};
use relog::{
    a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off,
    every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session,
    only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture,
};
use text::{
    a_character_typed_the_moment_a_screen_takes_the_keyboard_is_not_lost,
    a_composed_character_reaches_the_box_unchanged,
    a_paste_puts_the_clipboard_in_once_and_not_a_letter_with_it,
    a_press_on_a_button_takes_the_keyboard,
    a_press_on_a_list_takes_the_keyboard_and_its_look_follows,
    a_press_on_a_scrollbar_takes_the_keyboard_without_moving_it,
    a_press_on_something_that_cannot_scroll_moves_the_keyboard_nowhere,
    a_screen_that_takes_the_keyboard_in_its_own_pass_keeps_the_next_character,
    a_window_remembers_the_box_that_held_the_keyboard,
    every_composition_message_is_handed_to_the_desktop,
    line_breaks_in_what_was_pasted_never_reach_the_shard, middle_of,
    one_detent_over_the_chat_log_moves_it_one_line, press_at,
    pressing_the_log_is_what_lets_the_wheel_move_it, release, state_of,
    the_arming_happens_when_the_keyboard_moves_and_once_per_move,
    the_how_many_box_takes_digits_only,
    the_how_many_of_a_component_box_takes_digits_only_on_every_row,
    the_name_box_takes_the_letters_a_name_may_have,
    the_typing_switch_follows_an_editable_box_and_nothing_else,
    three_of_the_clients_boxes_take_only_certain_characters,
    typing_stops_when_the_keyboard_leaves_the_entry, what_holds_the_keyboard,
};
use viewport::{
    a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye,
    a_really_moved_view_measures_a_press_against_itself,
};
use wizard::{
    a_client_on_the_wizard, choosing_a_heritage_lights_its_bullet_and_describes_that_people,
    choosing_a_town_lights_its_pin_and_titles_the_page, click_wizard, dialog_child,
    every_button_the_wizard_builds_can_be_pressed,
    finish_composes_the_character_the_player_built_and_only_once,
    finish_replaces_the_forward_arrow_on_the_last_page_alone, press_handle, press_wizard_button,
    shipped_word, the_back_arrow_is_exit_only_on_the_first_page,
    the_exit_button_raises_a_modal_warning,
    the_finish_buttons_caption_is_drawn_in_the_font_the_layout_names,
    the_name_box_prompts_takes_the_keyboard_and_keeps_what_replaced_the_prompt,
    the_same_press_works_on_the_character_list_and_in_the_wizard,
    the_six_attribute_sliders_are_named_and_sit_at_their_values,
    the_summary_page_lists_every_choice_and_what_it_came_to,
    the_tab_strip_brightens_only_the_page_it_is_on,
    the_wizards_arrows_and_its_way_out_all_answer_a_press, walk_the_wizard, with_wizard,
    wizard_roots, wizard_text, yes_leaves_the_wizard_and_no_stays_on_the_page,
};

use dereth_client::platform::window::key_from_key_code;
use dereth_client::ui::HostState;
use dereth_testkit::adapters_shell::{
    build_app, element, open_options_page, scratch_preferences, AppSpec, BareKeyboard, Hands,
};
use dereth_testkit::{ClientSpec, HeadlessClient};
use dereth_ui::framework::mode;
use dereth_ui::ElementId;
use dereth_ui_screens::screens::charmgmt::CharacterManagementScreen;
use winit::keyboard::KeyCode;

dereth_testkit::scenarios! {
    scenario_every_function_key_does_what_the_shipped_map_says => every_function_key_does_what_the_shipped_map_says ["keymap.function-keys.each-one-does-what-the-shipped-map-binds-it-to"],
    scenario_each_panel_function_key_opens_and_shuts_its_page => each_panel_function_key_opens_and_shuts_its_page ["keymap.function-keys.each-panel-key-opens-its-own-page-and-shuts-it-again"],
    scenario_a_rebound_key_walks_the_body_and_the_old_one_stops => a_rebound_key_walks_the_body_and_the_old_one_stops ["keymap.rebind.a-rebound-key-moves-the-body-and-the-old-one-stops"],
    scenario_a_rebind_is_written_beside_the_preferences => a_rebind_is_written_beside_the_preferences ["keymap.rebind.is-written-beside-the-preferences-on-a-clean-exit"],
    scenario_the_desktops_four_are_taken_last_and_do_nothing => the_desktops_four_are_taken_last_and_do_nothing ["keymap.system-keys.the-four-the-desktop-owns-are-taken-last-and-do-nothing"],
    scenario_the_window_keeps_the_switcher_and_passes_the_other_two => the_window_keeps_the_switcher_and_passes_the_other_two ["window.system-keys.the-desktop-keeps-the-two-it-must-and-the-client-eats-the-third"],
    scenario_the_chat_map_is_walked_first_and_movement_before_the_camera => the_chat_map_is_walked_first_and_movement_before_the_camera ["keymap.walk-order.the-chat-map-is-walked-first-and-movement-before-the-camera"],
    scenario_opening_the_chat_bar_with_a_key_swallows_its_own_character => opening_the_chat_bar_with_a_key_swallows_its_own_character ["text-entry.focus.opening-the-chat-bar-with-a-key-swallows-that-keys-own-character"],
    scenario_the_armed_swallow_eats_exactly_one_character => the_armed_swallow_eats_exactly_one_character ["text-entry.focus.the-armed-swallow-eats-exactly-one-character"],
    scenario_one_backspace_deletes_one_character_and_a_hold_repeats => one_backspace_deletes_one_character_and_a_hold_repeats ["text-entry.backspace.one-press-deletes-one-character-and-a-hold-repeats-at-the-systems-rate"],
    scenario_a_backspace_tap_deletes_one_and_nothing_follows => a_backspace_tap_deletes_one_and_nothing_follows ["text-entry.backspace.a-tap-deletes-one-character-and-nothing-follows-it"],
    scenario_a_modified_number_uses_a_quick_slot_and_not_the_chat_window => a_modified_number_uses_a_quick_slot_and_not_the_chat_window ["keymap.modified-digits.a-number-with-the-modifier-uses-a-quick-slot-and-not-the-chat-window"],
    scenario_the_walk_mode_key_follows_the_run_by_default_option => the_walk_mode_key_follows_the_run_by_default_option ["movement.walk-mode-key.holding-it-follows-the-run-by-default-option-both-ways"],
    scenario_the_walk_mode_option_is_read_on_every_press => the_walk_mode_option_is_read_on_every_press ["movement.walk-mode-key.the-option-is-read-on-every-press-and-not-at-start-up"],
    scenario_the_key_binding_page_builds_one_row_per_bindable_action => the_key_binding_page_builds_one_row_per_bindable_action ["options.key-bindings.the-page-builds-one-row-for-every-bindable-action-once"],
    scenario_a_key_pressed_over_a_row_rebinds_it => a_key_pressed_over_a_row_rebinds_it ["options.key-bindings.a-key-pressed-over-a-row-rebinds-it-and-frees-the-old-key"],
    scenario_a_capture_in_flight_swallows_the_key => a_capture_in_flight_swallows_the_key ["options.key-bindings.a-capture-in-flight-swallows-the-key-and-gives-it-back-afterwards"],
    scenario_the_two_keys_walk_the_fellowship_both_ways_and_wrap => the_two_keys_walk_the_fellowship_both_ways_and_wrap ["selection.fellow.one-key-walks-the-fellowship-forward-and-the-other-back-both-wrapping"],
    scenario_an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing => an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing ["selection.fellow.a-selection-outside-the-fellowship-starts-at-one-end-and-with-no-fellowship-nothing-moves"],
    scenario_the_fellow_cycle_follows_the_panels_order => the_fellow_cycle_follows_the_panels_order ["selection.fellow.the-cycle-follows-the-order-the-panel-shows"],
    scenario_the_key_that_goes_back_is_a_toggle_and_not_a_stack => the_key_that_goes_back_is_a_toggle_and_not_a_stack ["selection.previous.the-key-goes-back-one-and-is-a-toggle-rather-than-a-stack"],
    scenario_with_nothing_behind_it_the_key_selects_nothing => with_nothing_behind_it_the_key_selects_nothing ["selection.previous.with-nothing-behind-it-the-key-selects-nothing"],
    scenario_a_cleared_target_comes_back => a_cleared_target_comes_back ["selection.previous.a-target-that-was-cleared-comes-back"],
    scenario_every_row_shows_the_setting_the_shard_sent => every_row_shows_the_setting_the_shard_sent ["options.character-page.each-row-shows-the-bit-the-shard-sent-for-it"],
    scenario_a_tick_changes_only_that_setting => a_tick_changes_only_that_setting ["options.character-page.a-tick-changes-only-that-bit-of-what-the-shard-sent"],
    scenario_the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones => the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones ["options.character-page.the-settings-it-sends-back-are-byte-for-byte-the-ones-a-real-client-sent"],
    scenario_one_tick_and_one_visit_sends_one_message => one_tick_and_one_visit_sends_one_message ["options.character-page.one-tick-and-one-visit-sends-one-message-with-one-bit-moved"],
    scenario_a_deferred_change_is_flushed_by_the_frame => a_deferred_change_is_flushed_by_the_frame ["options.character-page.a-deferred-change-reaches-the-shard-eight-minutes-later-and-once"],
    scenario_with_no_description_nothing_is_sent => with_no_description_nothing_is_sent ["options.character-page.with-no-description-nothing-is-sent-and-nothing-is-invented"],
    scenario_a_ticked_row_stays_ticked_and_unticking_still_works => a_ticked_row_stays_ticked_and_unticking_still_works ["options.character-page.a-row-ticked-on-stays-ticked-and-unticking-still-works"],
    scenario_a_detent_over_a_tick_box_scrolls_the_list => a_detent_over_a_tick_box_scrolls_the_list ["pointer.wheel.a-detent-over-a-check-box-scrolls-the-list-and-leaves-the-box-alone"],
    scenario_the_same_box_still_answers_a_click_and_so_does_the_bar => the_same_box_still_answers_a_click_and_so_does_the_bar ["pointer.wheel.the-same-box-still-answers-a-click-and-so-does-the-bar"],
    scenario_one_fellowship_setting_turns_the_other_off_at_the_shard => one_fellowship_setting_turns_the_other_off_at_the_shard ["options.fellowship.turning-one-of-the-two-on-tells-the-shard-the-other-is-off-first"],
    scenario_auto_accept_from_the_default_clears_ignoring_at_the_shard => auto_accept_from_the_default_clears_ignoring_at_the_shard ["options.fellowship.the-same-holds-from-the-shipped-default-and-turning-it-off-again-is-one-message"],
    scenario_the_excluded_row_goes_out_at_once_and_cancel_restores_both => the_excluded_row_goes_out_at_once_and_cancel_restores_both ["options.character-page.the-excluded-row-goes-out-at-once-and-cancel-restores-both"],
    scenario_the_chat_options_tab_draws_its_controls => the_chat_options_tab_draws_its_controls ["options.chat-page.the-tab-comes-up-with-every-control-the-page-declares",
            "options.chat-page.the-chat-fonts-face-and-size-sit-under-the-windows-opacity",],
    scenario_a_partly_chosen_group_is_drawn_differently => a_partly_chosen_group_is_drawn_differently ["options.chat-page.a-partly-chosen-group-and-a-wholly-chosen-one-are-drawn-differently"],
    scenario_ticking_a_filter_reaches_the_window_and_sends_nothing => ticking_a_filter_reaches_the_window_and_sends_nothing ["options.chat-page.ticking-a-filter-reaches-the-window-that-routes-by-it-and-sends-nothing"],
    scenario_the_opacity_slider_writes_what_the_fade_reads => the_opacity_slider_writes_what_the_fade_reads ["options.chat-page.the-opacity-slider-writes-what-the-fade-reads-and-keeps-the-two-in-order"],
    scenario_apply_cancel_and_defaults_do_what_an_option_page_does => apply_cancel_and_defaults_do_what_an_option_page_does ["options.chat-page.apply-cancel-and-defaults-do-what-an-option-page-does"],
    scenario_the_exit_button_raises_a_modal_warning => the_exit_button_raises_a_modal_warning ["chargen.exit.the-exit-button-raises-a-modal-warning-in-the-shipped-words"],
    scenario_yes_leaves_the_wizard_and_no_stays_on_the_page => yes_leaves_the_wizard_and_no_stays_on_the_page ["chargen.exit.saying-yes-goes-back-to-choosing-a-character-and-saying-no-stays-on-the-page"],
    scenario_the_back_arrow_is_exit_only_on_the_first_page => the_back_arrow_is_exit_only_on_the_first_page ["chargen.exit.the-back-arrow-is-exit-on-the-first-page-and-a-step-back-on-any-other"],
    scenario_moving_a_window_is_remembered_across_a_rebuild => moving_a_window_is_remembered_across_a_rebuild ["window.layout.moving-or-hiding-a-window-is-remembered-and-comes-back-when-the-screen-is-rebuilt"],
    scenario_a_change_before_the_description_survives_it => a_change_before_the_description_survives_it ["window.layout.a-change-made-before-the-character-is-described-survives-it"],
    scenario_a_saved_layout_is_clamped_and_moves_only_what_it_names => a_saved_layout_is_clamped_and_moves_only_what_it_names ["window.layout.a-saved-layout-is-pulled-onto-the-screen-and-moves-only-the-window-it-names"],
    scenario_a_click_advances_one_picture_and_the_release_is_not_another => a_click_advances_one_picture_and_the_release_is_not_another ["intro.click.a-click-advances-one-picture-and-letting-go-is-not-another"],
    scenario_clicking_through_the_intro_ends_at_character_select => clicking_through_the_intro_ends_at_character_select ["intro.click.clicking-through-the-sequence-ends-at-character-select-and-not-before"],
    scenario_the_quit_action_skips_the_rest_of_the_intro => the_quit_action_skips_the_rest_of_the_intro ["intro.quit.the-quit-action-skips-the-rest-of-it"],
    scenario_any_character_advances_and_escape_skips => any_character_advances_and_escape_skips ["intro.keyboard.any-character-advances-one-picture-and-escape-skips"],
    scenario_a_press_on_a_button_takes_the_keyboard => a_press_on_a_button_takes_the_keyboard ["focus.press.a-press-on-a-button-takes-the-keyboard-and-the-button-keeps-its-own-look"],
    scenario_typing_stops_when_the_keyboard_leaves_the_entry => typing_stops_when_the_keyboard_leaves_the_entry ["focus.press.typing-stops-when-the-keyboard-leaves-the-entry"],
    scenario_a_press_on_a_list_takes_the_keyboard_and_its_look_follows => a_press_on_a_list_takes_the_keyboard_and_its_look_follows ["focus.press.a-press-on-a-list-takes-the-keyboard-and-its-look-follows"],
    scenario_a_press_on_a_scrollbar_takes_the_keyboard_without_moving_it => a_press_on_a_scrollbar_takes_the_keyboard_without_moving_it ["focus.press.a-press-on-a-scrollbar-takes-the-keyboard-too"],
    scenario_a_press_on_something_that_cannot_scroll_moves_the_keyboard_nowhere => a_press_on_something_that_cannot_scroll_moves_the_keyboard_nowhere ["focus.press.a-press-on-something-that-cannot-scroll-moves-the-keyboard-nowhere"],
    scenario_a_character_typed_the_moment_a_screen_takes_the_keyboard_is_not_lost => a_character_typed_the_moment_a_screen_takes_the_keyboard_is_not_lost ["text-entry.focus.a-character-typed-the-moment-a-screen-takes-the-keyboard-is-not-lost"],
    scenario_a_screen_that_takes_the_keyboard_in_its_own_pass_keeps_the_next_character => a_screen_that_takes_the_keyboard_in_its_own_pass_keeps_the_next_character ["text-entry.focus.a-screen-that-takes-the-keyboard-in-its-own-pass-keeps-the-next-character"],
    scenario_the_typing_switch_follows_an_editable_box_and_nothing_else => the_typing_switch_follows_an_editable_box_and_nothing_else ["text-entry.focus.the-typing-switch-follows-a-box-that-can-be-typed-into-and-is-thrown-once-per-change"],
    scenario_the_returning_character_list_selects_the_character_that_was_just_played => the_returning_character_list_selects_the_character_that_was_just_played ["shell-only.character-select.the-returning-list-selects-the-character-just-played"],
    scenario_the_full_screen_setting_is_kept_and_applied_on_entering_the_world => the_full_screen_setting_is_kept_and_applied_on_entering_the_world ["window.full-screen.the-setting-is-kept-from-the-start-and-applied-on-entering-the-world"],
    scenario_the_full_screen_switch_key_is_refused_outside_the_world => the_full_screen_switch_key_is_refused_outside_the_world ["window.full-screen.the-switch-key-is-refused-outside-the-world-and-works-inside-it"],
    scenario_the_options_page_turns_full_screen_on_and_off_mid_session => the_options_page_turns_full_screen_on_and_off_mid_session ["window.full-screen.the-options-page-turns-it-on-and-off-while-the-player-plays"],
    scenario_the_summary_page_lists_every_choice_and_what_it_came_to => the_summary_page_lists_every_choice_and_what_it_came_to ["chargen.summary.lists-every-choice-the-wizard-has-made-and-what-they-came-to"],
    scenario_finish_replaces_the_forward_arrow_on_the_last_page_alone => finish_replaces_the_forward_arrow_on_the_last_page_alone ["chargen.finish.replaces-the-forward-arrow-on-the-last-page-and-does-nothing-anywhere-else"],
    scenario_finish_composes_the_character_the_player_built_and_only_once => finish_composes_the_character_the_player_built_and_only_once ["chargen.finish.composes-the-character-the-player-built-and-only-once"],
    scenario_choosing_a_heritage_lights_its_bullet_and_describes_that_people => choosing_a_heritage_lights_its_bullet_and_describes_that_people ["chargen.heritage.one-bullet-is-lit-and-the-page-behind-it-describes-that-people"],
    scenario_choosing_a_town_lights_its_pin_and_titles_the_page => choosing_a_town_lights_its_pin_and_titles_the_page ["chargen.town.one-pin-is-lit-and-the-page-is-titled-and-described-for-that-town"],
    scenario_the_tab_strip_brightens_only_the_page_it_is_on => the_tab_strip_brightens_only_the_page_it_is_on ["chargen.tabs.the-page-the-player-is-on-is-the-only-bright-one"],
    scenario_the_six_attribute_sliders_are_named_and_sit_at_their_values => the_six_attribute_sliders_are_named_and_sit_at_their_values ["chargen.profession.the-six-sliders-are-named-and-sit-where-their-numbers-say"],
    scenario_the_name_box_prompts_takes_the_keyboard_and_keeps_what_replaced_the_prompt => the_name_box_prompts_takes_the_keyboard_and_keeps_what_replaced_the_prompt ["chargen.summary.the-name-box-prompts-takes-the-keyboard-and-the-first-character-replaces-the-prompt"],
    scenario_the_finish_buttons_caption_is_drawn_in_the_font_the_layout_names => the_finish_buttons_caption_is_drawn_in_the_font_the_layout_names ["chargen.finish.its-caption-is-drawn-in-the-font-the-shipped-layout-names"],
    scenario_the_wizard_opens_on_a_character_already_rolled => the_wizard_opens_on_a_character_already_rolled ["chargen.random.the-wizard-opens-on-a-character-already-rolled"],
    scenario_the_opening_roll_is_the_seeds_own_and_two_clients_roll_the_same_character => the_opening_roll_is_the_seeds_own_and_two_clients_roll_the_same_character ["chargen.random.the-opening-roll-is-the-seeds-own-and-two-clients-roll-the-same-character"],
    scenario_the_people_and_the_sex_come_out_of_their_own_stream => the_people_and_the_sex_come_out_of_their_own_stream ["chargen.random.the-people-and-the-sex-come-out-of-a-different-draw-from-everything-else"],
    scenario_the_opening_people_is_one_a_plain_account_may_play => the_opening_people_is_one_a_plain_account_may_play ["chargen.random.the-opening-people-is-one-a-plain-account-may-play-and-the-expansion-adds-one"],
    scenario_the_home_town_follows_the_people => the_home_town_follows_the_people ["chargen.heritage.the-home-town-follows-the-people-and-is-one-of-that-peoples-own"],
    scenario_the_random_button_re_rolls_the_page_the_player_is_on => the_random_button_re_rolls_the_page_the_player_is_on ["chargen.random.the-button-re-rolls-the-page-the-player-is-on"],
    scenario_the_last_page_warns_before_re_rolling_the_whole_character => the_last_page_warns_before_re_rolling_the_whole_character ["chargen.random.on-the-last-page-it-warns-first-and-only-a-yes-re-rolls-the-whole-character"],
    scenario_the_town_pages_roll_can_land_on_any_town => the_town_pages_roll_can_land_on_any_town ["chargen.random.on-the-town-page-it-can-land-on-any-town-and-not-only-the-peoples-own"],
    scenario_the_insect_peoples_lose_three_tabs_outright => the_insect_peoples_lose_three_tabs_outright ["chargen.tabs.the-two-insect-peoples-lose-three-pages-outright-rather-than-having-them-greyed"],
    scenario_the_face_tab_takes_the_hat_off_and_the_clothes_tab_puts_it_back => the_face_tab_takes_the_hat_off_and_the_clothes_tab_puts_it_back ["chargen.appearance.the-face-tab-takes-the-hat-off-and-the-clothes-tab-puts-it-back"],
    scenario_the_colour_spots_are_the_chosen_parts_own_colours => the_colour_spots_are_the_chosen_parts_own_colours ["chargen.appearance.the-colour-spots-are-the-chosen-parts-own-colours-and-the-spare-ones-are-blank"],
    scenario_the_eyes_have_one_colour_each_and_no_shade_to_slide => the_eyes_have_one_colour_each_and_no_shade_to_slide ["chargen.appearance.the-eyes-have-one-colour-each-and-no-shade-to-slide"],
    scenario_clicking_a_colour_moves_the_marker_and_tints_the_shade_wheel => clicking_a_colour_moves_the_marker_and_tints_the_shade_wheel ["chargen.appearance.clicking-a-colour-moves-the-marker-and-tints-the-shade-wheel-with-it"],
    scenario_the_re_roll_warning_is_a_modal_question_in_the_shipped_words => the_re_roll_warning_is_a_modal_question_in_the_shipped_words ["chargen.dialogs.the-warning-before-a-re-roll-is-a-modal-question-in-the-shipped-words"],
    scenario_the_warnings_two_buttons_answer_it_and_a_pointer_can_press_either => the_warnings_two_buttons_answer_it_and_a_pointer_can_press_either ["chargen.dialogs.the-two-answer-buttons-really-answer-and-a-pointer-can-press-either-of-them"],
    scenario_only_the_last_page_asks_before_re_rolling => only_the_last_page_asks_before_re_rolling ["chargen.dialogs.the-warning-before-a-re-roll-is-the-last-pages-alone"],
    scenario_a_people_the_account_cannot_play_is_refused_in_a_message_box => a_people_the_account_cannot_play_is_refused_in_a_message_box ["chargen.dialogs.a-people-the-account-cannot-play-is-refused-in-a-message-box"],
    scenario_finishing_with_credits_unspent_asks_first => finishing_with_credits_unspent_asks_first ["chargen.dialogs.finishing-with-credits-unspent-asks-first-and-a-yes-goes-on-to-create"],
    scenario_finishing_with_no_name_refuses_in_a_message_box => finishing_with_no_name_refuses_in_a_message_box ["chargen.dialogs.finishing-with-no-name-refuses-in-a-message-box"],
    scenario_the_wizard_never_shows_a_please_wait_box => the_wizard_never_shows_a_please_wait_box ["chargen.dialogs.the-wizard-never-shows-a-please-wait-box"],
    scenario_a_box_answered_from_outside_the_screen_comes_down => a_box_answered_from_outside_the_screen_comes_down ["chargen.dialogs.one-answered-from-outside-the-screen-is-taken-down-on-the-next-frame"],
    scenario_a_second_box_waits_its_turn_rather_than_stacking => a_second_box_waits_its_turn_rather_than_stacking ["chargen.dialogs.a-second-one-waits-its-turn-rather-than-stacking-on-the-first"],
    scenario_a_box_closed_from_underneath_the_screen_is_replaced => a_box_closed_from_underneath_the_screen_is_replaced ["chargen.dialogs.one-closed-from-underneath-the-screen-is-replaced-rather-than-left-behind"],
    scenario_three_panes_and_two_lists_scroll_and_the_town_page_has_none => three_panes_and_two_lists_scroll_and_the_town_page_has_none ["chargen.scroll.three-panes-and-two-lists-scroll-and-the-town-page-has-nothing-to-scroll"],
    scenario_each_pane_drives_the_bar_beside_it => each_pane_drives_the_bar_beside_it ["chargen.scroll.each-pane-drives-the-bar-beside-it-and-not-another-pages"],
    scenario_a_filled_pane_has_a_live_bar_sized_to_what_is_shown => a_filled_pane_has_a_live_bar_sized_to_what_is_shown ["chargen.scroll.a-pane-with-text-in-it-has-a-live-bar-whose-thumb-is-the-size-of-what-is-shown"],
    scenario_dragging_the_thumb_moves_the_rows_by_the_same_fraction => dragging_the_thumb_moves_the_rows_by_the_same_fraction ["chargen.scroll.dragging-the-thumb-moves-the-rows-by-the-same-fraction"],
    scenario_an_arrow_moves_one_row_and_the_two_go_opposite_ways => an_arrow_moves_one_row_and_the_two_go_opposite_ways ["chargen.scroll.an-arrow-moves-one-row-and-the-two-arrows-go-opposite-ways"],
    scenario_a_press_on_the_track_moves_a_whole_page_towards_the_press => a_press_on_the_track_moves_a_whole_page_towards_the_press ["chargen.scroll.a-press-on-the-track-moves-a-whole-page-towards-the-press"],
    scenario_the_wheel_moves_a_list_one_row_and_stops_at_the_top => the_wheel_moves_a_list_one_row_and_stops_at_the_top ["chargen.scroll.the-wheel-moves-a-list-one-row-and-stops-at-the-top"],
    scenario_a_pane_measures_its_whole_paragraph_and_the_blank_row_after_it => a_pane_measures_its_whole_paragraph_and_the_blank_row_after_it ["chargen.scroll.a-pane-measures-every-line-of-its-text-its-margins-and-the-blank-row-after-it"],
    scenario_the_four_headings_sit_above_their_own_groups => the_four_headings_sit_above_their_own_groups ["chargen.skills.the-four-headings-sit-above-their-own-groups-and-each-group-is-in-name-order"],
    scenario_every_row_shows_its_own_score_and_its_own_two_prices => every_row_shows_its_own_score_and_its_own_two_prices ["chargen.skills.every-row-shows-its-own-score-and-its-own-two-prices"],
    scenario_training_a_skill_moves_its_row_and_re_prices_the_rest => training_a_skill_moves_its_row_and_re_prices_the_rest ["chargen.skills.training-one-moves-its-row-under-the-trained-heading-and-re-prices-the-rest"],
    scenario_the_skill_rows_are_stacked_and_none_overlaps => the_skill_rows_are_stacked_and_none_overlaps ["chargen.skills.the-rows-are-stacked-one-below-another-and-none-overlaps"],
    scenario_the_skills_page_reads_the_way_the_original_drew_it => the_skills_page_reads_the_way_the_original_drew_it ["chargen.skills.the-page-reads-the-way-the-original-drew-it-for-the-same-character"],
    scenario_every_row_is_drawn_under_its_new_heading_and_back_again => every_row_is_drawn_under_its_new_heading_and_back_again ["chargen.skills.every-row-is-drawn-under-its-new-heading-and-back-again"],
    scenario_the_reported_press_moves_the_row_it_names => the_reported_press_moves_the_row_it_names ["chargen.skills.the-reported-press-moves-the-row-it-names-and-leaving-the-page-changes-nothing"],
    scenario_a_press_in_the_name_box_puts_the_caret_where_it_was_pressed => a_press_in_the_name_box_puts_the_caret_where_it_was_pressed ["chargen.summary.a-press-in-the-name-box-puts-the-caret-where-it-was-pressed-and-typing-goes-there"],
    scenario_a_press_on_nothing_takes_the_keyboard_away_and_a_press_back_in_returns_it => a_press_on_nothing_takes_the_keyboard_away_and_a_press_back_in_returns_it ["chargen.summary.a-press-on-nothing-takes-the-keyboard-away-and-a-press-back-in-the-box-returns-it"],
    scenario_a_press_that_types_nothing_leaves_a_plain_caret => a_press_that_types_nothing_leaves_a_plain_caret ["chargen.summary.a-press-that-types-nothing-leaves-a-plain-caret-and-only-a-gesture-puts-the-highlight-back"],
    scenario_the_dialog_map_binds_only_escape_and_enter => the_dialog_map_binds_only_escape_and_enter ["keymap.dialog-keys.the-shipped-map-binds-only-escape-and-enter-and-each-once"],
    scenario_enter_advances_the_opening_sequence_and_escape_leaves_it => enter_advances_the_opening_sequence_and_escape_leaves_it ["dialog-keys.intro.enter-advances-the-opening-sequence-and-escape-leaves-it"],
    scenario_a_real_press_of_enter_advances_twice => a_real_press_of_enter_advances_twice ["dialog-keys.intro.a-real-press-of-enter-advances-twice-because-two-handlers-answer-it"],
    scenario_the_opening_screen_answers_only_the_keys_its_own_maps_carry => the_opening_screen_answers_only_the_keys_its_own_maps_carry ["dialog-keys.intro.the-screen-answers-the-keys-its-own-maps-carry-and-no-others"],
    scenario_either_key_ends_the_credit_roll_and_ends_it_once => either_key_ends_the_credit_roll_and_ends_it_once ["dialog-keys.credits.either-key-ends-the-roll-and-ends-it-once"],
    scenario_enter_is_declined_on_the_character_list_and_escape_asks_once => enter_is_declined_on_the_character_list_and_escape_asks_once ["dialog-keys.character-select.enter-is-declined-and-escape-asks-once-whether-to-quit"],
    scenario_a_box_that_takes_typing_adds_the_dialog_map_without_taking_the_keys => a_box_that_takes_typing_adds_the_dialog_map_without_taking_the_keys ["dialog-keys.a-box-that-takes-typing-adds-the-dialog-map-without-taking-the-keys-from-the-box"],
    scenario_leaving_raises_a_modal_question_in_the_shipped_words => leaving_raises_a_modal_question_in_the_shipped_words ["character-select.dialogs.leaving-raises-a-modal-question-in-the-shipped-words"],
    scenario_answering_yes_to_leaving_runs_the_closing_sequence_and_no_stays => answering_yes_to_leaving_runs_the_closing_sequence_and_no_stays ["character-select.dialogs.answering-yes-to-leaving-runs-the-closing-sequence-and-no-stays"],
    scenario_deleting_asks_about_the_character_that_was_picked => deleting_asks_about_the_character_that_was_picked ["character-select.dialogs.deleting-asks-about-the-character-that-was-picked-and-names-only-them"],
    scenario_the_question_is_modal_and_a_press_behind_it_reaches_nothing => the_question_is_modal_and_a_press_behind_it_reaches_nothing ["character-select.dialogs.the-question-is-modal-and-a-second-press-behind-it-reaches-nothing"],
    scenario_only_the_typed_phrase_deletes_and_it_deletes_the_one_that_was_picked => only_the_typed_phrase_deletes_and_it_deletes_the_one_that_was_picked ["character-select.delete.only-the-typed-phrase-deletes-and-it-deletes-the-one-that-was-picked"],
    scenario_cancelling_after_typing_the_phrase_deletes_nothing => cancelling_after_typing_the_phrase_deletes_nothing ["character-select.delete.cancelling-after-typing-the-phrase-deletes-nothing"],
    scenario_restoring_raises_a_box_with_no_buttons_that_the_next_list_takes_down => restoring_raises_a_box_with_no_buttons_that_the_next_list_takes_down ["character-select.dialogs.restoring-raises-a-box-with-no-buttons-that-the-next-list-takes-down"],
    scenario_a_double_press_raises_the_waiting_box_before_it_asks_to_log_on => a_double_press_raises_the_waiting_box_before_it_asks_to_log_on ["character-select.enter-world.a-double-press-raises-the-waiting-box-before-it-asks-to-log-on"],
    scenario_an_error_brought_in_with_the_screen_becomes_a_one_button_message => an_error_brought_in_with_the_screen_becomes_a_one_button_message ["character-select.dialogs.an-error-brought-in-with-the-screen-becomes-a-one-button-message"],
    scenario_one_detent_over_the_chat_log_moves_it_one_line => one_detent_over_the_chat_log_moves_it_one_line ["pointer.wheel.one-detent-over-the-chat-log-moves-it-one-line-and-the-other-way-puts-it-back"],
    scenario_pressing_the_log_is_what_lets_the_wheel_move_it => pressing_the_log_is_what_lets_the_wheel_move_it ["pointer.wheel.pressing-the-log-is-what-lets-the-wheel-move-it-and-typing-does-not-take-it-away"],
    scenario_the_arming_happens_when_the_keyboard_moves_and_once_per_move => the_arming_happens_when_the_keyboard_moves_and_once_per_move ["pointer.wheel.the-arming-happens-when-the-keyboard-moves-and-once-per-move"],
    scenario_a_window_remembers_the_box_that_held_the_keyboard => a_window_remembers_the_box_that_held_the_keyboard ["window.focus.a-window-remembers-the-box-that-held-the-keyboard-and-gives-it-back"],
    scenario_a_cell_shows_the_key_the_way_the_desktop_names_it => a_cell_shows_the_key_the_way_the_desktop_names_it ["options.key-bindings.a-cell-shows-the-key-the-way-the-desktop-names-it"],
    scenario_resting_on_a_cell_says_what_a_press_there_would_do => resting_on_a_cell_says_what_a_press_there_would_do ["options.key-bindings.resting-on-a-cell-says-what-a-press-there-would-do"],
    scenario_every_section_is_titled_in_words => every_section_is_titled_in_words ["options.key-bindings.every-section-is-titled-in-words"],
    scenario_undo_opens_greyed_because_nothing_has_changed_yet => undo_opens_greyed_because_nothing_has_changed_yet ["options.key-bindings.undo-opens-greyed-because-nothing-has-changed-yet"],
    scenario_a_press_on_a_cell_waits_for_a_key_and_undo_puts_the_old_one_back => a_press_on_a_cell_waits_for_a_key_and_undo_puts_the_old_one_back ["options.key-bindings.a-press-on-a-cell-waits-for-a-key-and-undo-puts-the-old-one-back"],
    scenario_a_key_already_in_use_asks_first_and_one_that_cannot_be_taken_refuses => a_key_already_in_use_asks_first_and_one_that_cannot_be_taken_refuses ["options.key-bindings.a-key-already-in-use-asks-first-and-one-that-cannot-be-taken-refuses"],
    scenario_taking_a_key_clears_it_from_the_rows_that_had_it => taking_a_key_clears_it_from_the_rows_that_had_it ["options.key-bindings.taking-a-key-clears-it-from-the-rows-that-had-it-on-the-screen"],
    scenario_the_questions_about_a_key_in_use_are_the_shipped_sentences => the_questions_about_a_key_in_use_are_the_shipped_sentences ["options.key-bindings.the-questions-about-a-key-in-use-are-the-shipped-sentences-with-the-key-and-the-action-in-them"],
    scenario_a_set_of_keys_can_be_saved_under_a_name_and_loaded_back => a_set_of_keys_can_be_saved_under_a_name_and_loaded_back ["options.key-bindings.a-set-of-keys-can-be-saved-under-a-name-and-loaded-back"],
    scenario_saving_over_a_set_asks_first_and_one_that_cannot_be_written_refuses => saving_over_a_set_asks_first_and_one_that_cannot_be_written_refuses ["options.key-bindings.saving-over-a-set-asks-first-and-one-that-cannot-be-written-refuses"],
    scenario_restoring_the_defaults_gives_back_the_shipped_keys => restoring_the_defaults_gives_back_the_shipped_keys ["options.key-bindings.restoring-the-defaults-gives-back-the-shipped-keys-and-not-the-saved-ones"],
    scenario_three_of_the_clients_boxes_take_only_certain_characters => three_of_the_clients_boxes_take_only_certain_characters ["text-entry.filters.three-of-the-clients-boxes-take-only-certain-characters-and-the-rest-take-anything"],
    scenario_the_name_box_takes_the_letters_a_name_may_have => the_name_box_takes_the_letters_a_name_may_have ["text-entry.filters.the-name-box-takes-the-letters-a-name-may-have-and-nothing-else"],
    scenario_the_how_many_box_takes_digits_only => the_how_many_box_takes_digits_only ["text-entry.filters.the-how-many-box-takes-digits-only"],
    scenario_every_composition_message_is_handed_to_the_desktop => every_composition_message_is_handed_to_the_desktop ["text-entry.ime.every-composition-message-is-handed-to-the-desktop-and-an-ordinary-character-is-not"],
    scenario_a_composed_character_reaches_the_box_unchanged => a_composed_character_reaches_the_box_unchanged ["text-entry.ime.a-composed-character-reaches-the-box-unchanged"],
    scenario_a_paste_puts_the_clipboard_in_once_and_not_a_letter_with_it => a_paste_puts_the_clipboard_in_once_and_not_a_letter_with_it ["text-entry.paste.a-paste-puts-the-clipboard-in-once-and-not-a-letter-with-it"],
    scenario_line_breaks_in_what_was_pasted_never_reach_the_shard => line_breaks_in_what_was_pasted_never_reach_the_shard ["text-entry.paste.line-breaks-in-what-was-pasted-never-reach-the-shard"],
    scenario_the_how_many_of_a_component_box_takes_digits_only_on_every_row => the_how_many_of_a_component_box_takes_digits_only_on_every_row ["text-entry.filters.the-how-many-of-a-component-box-takes-digits-only-on-every-row"],
    scenario_the_same_press_works_on_the_character_list_and_in_the_wizard => the_same_press_works_on_the_character_list_and_in_the_wizard ["pointer.click.the-same-press-works-on-the-character-list-and-in-the-wizard"],
    scenario_the_wizards_arrows_and_its_way_out_all_answer_a_press => the_wizards_arrows_and_its_way_out_all_answer_a_press ["pointer.click.the-wizards-arrows-and-its-way-out-all-answer-a-press"],
    scenario_every_button_the_wizard_builds_can_be_pressed => every_button_the_wizard_builds_can_be_pressed ["pointer.click.every-button-the-wizard-builds-can-be-pressed-including-the-three-it-greys"],
    scenario_the_left_button_and_the_wheel_each_belong_to_one_shipped_map => the_left_button_and_the_wheel_each_belong_to_one_shipped_map ["pointer.the-left-button-and-the-wheel-each-belong-to-one-shipped-map"],
    scenario_a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye => a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye ["viewport.a-press-in-the-middle-of-a-moved-view-finds-what-is-ahead-of-the-eye"],
    scenario_a_really_moved_view_measures_a_press_against_itself => a_really_moved_view_measures_a_press_against_itself ["viewport.a-really-moved-view-measures-a-press-against-itself-and-the-default-is-not"],
    scenario_the_character_lists_own_buttons_each_raise_what_they_name => the_character_lists_own_buttons_each_raise_what_they_name ["pointer.click.the-character-lists-own-buttons-each-raise-what-they-name"],
    scenario_one_press_on_a_character_picks_it_and_two_takes_them_into_the_world => one_press_on_a_character_picks_it_and_two_takes_them_into_the_world ["pointer.click.one-press-on-a-character-picks-it-and-two-takes-them-into-the-world"],
    scenario_a_button_lights_under_the_pointer_and_sinks_under_the_press => a_button_lights_under_the_pointer_and_sinks_under_the_press ["pointer.a-button-lights-under-the-pointer-and-sinks-under-the-press"],
    scenario_a_press_dragged_off_a_button_releases_it_without_firing_it => a_press_dragged_off_a_button_releases_it_without_firing_it ["pointer.a-press-dragged-off-a-button-releases-it-without-firing-it"],
    scenario_a_move_makes_what_is_under_the_pointer_the_one_entered => a_move_makes_what_is_under_the_pointer_the_one_entered ["pointer.a-move-makes-what-is-under-the-pointer-the-one-entered-until-it-leaves"],
    scenario_a_press_on_a_toolbar_button_opens_the_panel_it_owns => a_press_on_a_toolbar_button_opens_the_panel_it_owns ["pointer.click.a-press-on-a-toolbar-button-opens-the-panel-it-owns"],
    scenario_a_key_the_roll_does_not_own_leaves_it_running => a_key_the_roll_does_not_own_leaves_it_running ["dialog-keys.credits.a-key-the-roll-does-not-own-leaves-it-running"],
    scenario_only_the_opening_sequence_refuses_a_key_on_its_way_back_up => only_the_opening_sequence_refuses_a_key_on_its_way_back_up ["dialog-keys.only-the-opening-sequence-refuses-a-key-on-its-way-back-up"],
    scenario_the_shards_log_off_answer_does_not_end_a_client_with_screens => the_shards_log_off_answer_does_not_end_a_client_with_screens ["shell-only.log-off.the-shards-answer-does-not-end-a-client-that-has-a-screen-to-go-back-to"],
    scenario_every_character_set_the_session_decodes_is_an_arrival => every_character_set_the_session_decodes_is_an_arrival ["shell-only.character-set.every-arrival-is-a-notice-even-when-the-list-has-not-changed"],
    scenario_a_list_arriving_in_the_world_sends_the_player_back_and_at_the_list_does_not => a_list_arriving_in_the_world_sends_the_player_back_and_at_the_list_does_not ["shell-only.character-set.a-list-arriving-in-the-world-sends-the-player-back-to-the-character-screen"],
    scenario_the_way_into_the_world_opens_again_after_a_log_off => the_way_into_the_world_opens_again_after_a_log_off ["shell-only.enter-world.the-way-into-the-world-opens-again-after-a-log-off"],
    scenario_answering_yes_asks_to_log_off_and_moves_no_screen_of_its_own => answering_yes_asks_to_log_off_and_moves_no_screen_of_its_own ["shell-only.log-off.answering-yes-asks-to-log-off-and-moves-no-screen-of-its-own"],
    scenario_a_pick_the_returning_list_cannot_honour_falls_back_to_the_shards_own_order => a_pick_the_returning_list_cannot_honour_falls_back_to_the_shards_own_order ["shell-only.character-select.a-pick-the-returning-list-cannot-honour-falls-back-to-the-shards-own-order"],
    scenario_the_delete_that_leaves_the_client_carries_the_account_and_the_shards_own_place => the_delete_that_leaves_the_client_carries_the_account_and_the_shards_own_place ["character-select.delete.the-request-that-leaves-the-client-carries-the-account-and-the-shards-own-place"],
    scenario_a_pending_deletion_is_drawn_red_and_last_and_offers_restore => a_pending_deletion_is_drawn_red_and_last_and_offers_restore ["character-select.delete.a-pending-deletion-is-drawn-red-and-last-and-offers-restore"],
    scenario_the_restore_that_leaves_the_client_is_the_characters_own_id_on_the_control_queue => the_restore_that_leaves_the_client_is_the_characters_own_id_on_the_control_queue ["character-select.restore.the-request-that-leaves-the-client-is-the-characters-own-id-on-the-control-queue"],
    scenario_a_restored_character_comes_back_in_its_own_place_and_in_the_ordinary_colour => a_restored_character_comes_back_in_its_own_place_and_in_the_ordinary_colour ["character-select.restore.a-restored-character-comes-back-in-its-own-place-and-in-the-ordinary-colour"],
    scenario_the_restore_answer_overwrites_the_place_it_was_asked_about_and_never_appends => the_restore_answer_overwrites_the_place_it_was_asked_about_and_never_appends ["character-select.restore.the-answer-overwrites-the-place-it-was-asked-about-and-never-appends"],
    scenario_a_refused_restore_takes_the_waiting_box_down_and_says_why => a_refused_restore_takes_the_waiting_box_down_and_says_why ["character-select.restore.a-refusal-takes-the-waiting-box-down-and-says-why-in-the-shipped-words"],
    scenario_deleting_and_restoring_over_the_wire_leaves_the_list_where_it_started => deleting_and_restoring_over_the_wire_leaves_the_list_where_it_started ["character-select.round-trip.deleting-and-restoring-over-the-wire-leaves-the-list-where-it-started"],
    scenario_a_refused_creation_stops_the_wizard_waiting_and_says_why => a_refused_creation_stops_the_wizard_waiting_and_says_why ["chargen.finish.a-refused-creation-stops-the-wizard-waiting-and-says-why"],
    scenario_every_refusal_the_shard_can_send_draws_its_own_sentence => every_refusal_the_shard_can_send_draws_its_own_sentence ["chargen.finish.every-refusal-the-shard-can-send-draws-its-own-sentence"],
    scenario_only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture => only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture ["relog.state.only-the-shortcut-and-the-auto-saved-option-reach-the-shard-at-the-gesture"],
    scenario_every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session => every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session ["relog.state.every-deferred-change-is-saved-at-the-log-off-and-found-by-the-next-session"],
    scenario_a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off => a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off ["relog.state.a-session-that-changed-nothing-deferred-saves-nothing-at-the-log-off"],
    scenario_each_of_this_clients_actions_works_on_a_key_the_page_gives_it => each_of_this_clients_actions_works_on_a_key_the_page_gives_it ["keys.own.each-of-this-clients-actions-works-on-a-key-the-page-gives-it"],
    scenario_a_key_in_use_given_to_this_clients_action_asks_first_and_is_taken => a_key_in_use_given_to_this_clients_action_asks_first_and_is_taken ["keys.own.a-key-in-use-given-to-this-clients-action-asks-first-and-is-taken"],
}

use dereth_protocol::property::BasePropertyValue;
use dereth_ui_screens::options::chat::{ChatOption, ATTR_IMAGE_ALL, ATTR_IMAGE_SOME};

// -------------------------------------------------------------------------------------------
// Harness self-proofs. **Not census rows**: they make no claim about the client, they prove that
// two of the harness's `ClientSpec` options and one input step do what they say. They live beside
// the shell-only scenario; `goldens.rs` is the precedent for a `dat` test that is not a behaviour.
// -------------------------------------------------------------------------------------------

/// The inventory page of the gameplay panel bar, in the shipped layout's own id.
const INVENTORY_PAGE: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_018B);

/// `ClientSpec::with_open_page` raises the page, and a spec without it leaves the page down --
/// which is the half that makes the first half a measurement.
#[test]
fn with_open_page_raises_the_page_and_nothing_else_does() {
    let mut shut = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    assert!(
        !shut.ui_snapshot().is_visible(INVENTORY_PAGE),
        "the premise: the shipped layout comes up with the pack down"
    );
    shut.shutdown();

    let mut open =
        HeadlessClient::new(ClientSpec::gameplay_in_world(4).with_open_page(INVENTORY_PAGE));
    assert!(
        open.ui_snapshot().is_visible(INVENTORY_PAGE),
        "ClientSpec::with_open_page did not raise the page"
    );
    open.shutdown();
}

/// `ClientSpec::with_scratch_settings` makes its directory, the client is pointed at it, and
/// dropping the client removes it. A scenario about what the client *wrote* gets a fresh one every
/// time and leaves nothing behind -- and nothing of it is the player's real settings folder.
#[test]
fn a_scratch_settings_directory_is_made_and_removed_with_the_client() {
    let dir;
    {
        let c = HeadlessClient::new(ClientSpec::retail().with_scratch_settings("selftest"));
        dir = c
            .scratch_settings()
            .expect("the spec named one")
            .dir()
            .to_path_buf();
        assert!(
            dir.is_dir(),
            "the settings directory was not made at {}",
            dir.display()
        );
        assert!(
            c.view()
                .expect_app()
                .config()
                .preferences_file
                .starts_with(&dir),
            "the client's preferences file is not under the directory the scenario named"
        );
        c.shutdown();
    }
    assert!(
        !dir.exists(),
        "the settings directory outlived the client at {}",
        dir.display()
    );
}

/// `input_steps::press_use` presses the key the **shipped keymap** binds, and the shipped keymap
/// binds it to `R`.
///
/// It is the whole of what the step claims: the binding is read out of the client's own keymap,
/// the press is asserted against it, and a wrong key would fail here rather than press something
/// else.
#[test]
fn the_shipped_keymap_binds_use_to_r_and_the_step_presses_it() {
    use dereth_testkit::input_steps;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let bound = input_steps::bound_scan_code(&mut c, input_steps::USE, input_steps::UI_COMMANDS);
    let r = dereth_client::platform::window::key_from_key_code(winit::keyboard::KeyCode::KeyR)
        .expect("the host names R");
    assert_eq!(
        bound & 0x7F,
        r.scan_code & 0x7F,
        "the shipped keymap binds USE to DIK_R; it binds {bound:#06X}"
    );
    input_steps::press_use(&mut c, r);
    c.shutdown();
}

use dereth_chargen::Attr;
use dereth_ui_screens::screens::chargen::{
    EcgProgress, ATTRIBUTE_SLIDERS, FINISH_BUTTON, LEFT_BUTTON, RIGHT_BUTTON, STATE_TAB_OFF,
    STATE_TAB_ON,
};

use dereth_chargen::{CharGenRng, CharGenState, HERITAGE_OLTHOI, HERITAGE_OLTHOI_ACID};
use dereth_ui_screens::screens::chargen::CharGenDialog;

use dereth_chargen::CgVerification;
use dereth_ui_screens::screens::chargen::{
    CREDIT_WARNING_STRING, MESSAGE_BUTTON, RANDOMIZE_WARNING_STRING, RANDOM_BUTTON,
    TOD_WARNING_STRING,
};

use dereth_ui::widgets::scrollbar::attr as bar_attr;

use dereth_chargen::SkillAdvancementClass;
use dereth_ui_screens::screens::chargen::{ATTR_ROW_SKILL_ID, STATE_ARROW_OFF, STATE_ARROW_ON};

use dereth_input::fire::{walk_input_maps, InputMapEntry};
use dereth_input::spec::{activation, ControlChord, DeviceType};
use dereth_input::{ActionId, InputMapId};

use dereth_ui_screens::screens::charmgmt::{self, CharacterAction, DialogContext};

use dereth_ui_screens::chat::window::{ENTRY, LOG};

use dereth_input::spec::{ControlCode, SubControlIndex};
use dereth_ui_screens::options::keybinding::{control_name, RowDialog, DIALOG_QUEUE};

use dereth_client::pump::key_text_messages;
use dereth_client::pump::window_proc::{msg as win_msg, Effect};

use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::ObjectId;
use dereth_testkit::replay::Peer;

use dereth_client_model::player::options::option as relog_option;
use dereth_ui_screens::view::{DropTarget, PlayerOption, UiRequest};
