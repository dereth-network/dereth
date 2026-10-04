//! The `dat` tier's shell scenarios: the keyboard and its bindings, the options pages, the windows,
//! the dialog boxes, the character-creation wizard, the login flow and what survives a relog.
//! Fixture: the retail dats and shipped layouts in a `HeadlessClient` (or the bare
//! `ClientSpec::shell` backend), driven by `adapters_shell`'s [`Hands`] -- keyboard, modifiers and
//! wheel -- and read against [`BareKeyboard`], the shipped key map with no client behind it, since
//! a screen that is up registers its own maps on top. **This binary must run serially**: two
//! headless clients in one process share the UI request globals. Each scenario is a `pub fn` with a
//! `#[test]` beside it; `ALL` lists them for the census in `tests/cpu/census.rs`, so a scenario
//! written and not listed shows up as a shortfall rather than as a silent gap.

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

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "a_key_in_use_given_to_this_clients_action_asks_first_and_is_taken",
        &["keys.own.a-key-in-use-given-to-this-clients-action-asks-first-and-is-taken"],
        a_key_in_use_given_to_this_clients_action_asks_first_and_is_taken,
    ),
    (
        "each_of_this_clients_actions_works_on_a_key_the_page_gives_it",
        &["keys.own.each-of-this-clients-actions-works-on-a-key-the-page-gives-it"],
        each_of_this_clients_actions_works_on_a_key_the_page_gives_it,
    ),
    (
        "every_function_key_does_what_the_shipped_map_says",
        &["keymap.function-keys.each-one-does-what-the-shipped-map-binds-it-to"],
        every_function_key_does_what_the_shipped_map_says,
    ),
    (
        "each_panel_function_key_opens_and_shuts_its_page",
        &["keymap.function-keys.each-panel-key-opens-its-own-page-and-shuts-it-again"],
        each_panel_function_key_opens_and_shuts_its_page,
    ),
    (
        "a_rebound_key_walks_the_body_and_the_old_one_stops",
        &["keymap.rebind.a-rebound-key-moves-the-body-and-the-old-one-stops"],
        a_rebound_key_walks_the_body_and_the_old_one_stops,
    ),
    (
        "a_rebind_is_written_beside_the_preferences",
        &["keymap.rebind.is-written-beside-the-preferences-on-a-clean-exit"],
        a_rebind_is_written_beside_the_preferences,
    ),
    (
        "the_desktops_four_are_taken_last_and_do_nothing",
        &["keymap.system-keys.the-four-the-desktop-owns-are-taken-last-and-do-nothing"],
        the_desktops_four_are_taken_last_and_do_nothing,
    ),
    (
        "the_window_keeps_the_switcher_and_passes_the_other_two",
        &["window.system-keys.the-desktop-keeps-the-two-it-must-and-the-client-eats-the-third"],
        the_window_keeps_the_switcher_and_passes_the_other_two,
    ),
    (
        "the_chat_map_is_walked_first_and_movement_before_the_camera",
        &["keymap.walk-order.the-chat-map-is-walked-first-and-movement-before-the-camera"],
        the_chat_map_is_walked_first_and_movement_before_the_camera,
    ),
    (
        "opening_the_chat_bar_with_a_key_swallows_its_own_character",
        &["text-entry.focus.opening-the-chat-bar-with-a-key-swallows-that-keys-own-character"],
        opening_the_chat_bar_with_a_key_swallows_its_own_character,
    ),
    (
        "the_armed_swallow_eats_exactly_one_character",
        &["text-entry.focus.the-armed-swallow-eats-exactly-one-character"],
        the_armed_swallow_eats_exactly_one_character,
    ),
    (
        "one_backspace_deletes_one_character_and_a_hold_repeats",
        &["text-entry.backspace.one-press-deletes-one-character-and-a-hold-repeats-at-the-systems-rate"],
        one_backspace_deletes_one_character_and_a_hold_repeats,
    ),
    (
        "a_backspace_tap_deletes_one_and_nothing_follows",
        &["text-entry.backspace.a-tap-deletes-one-character-and-nothing-follows-it"],
        a_backspace_tap_deletes_one_and_nothing_follows,
    ),
    (
        "a_modified_number_uses_a_quick_slot_and_not_the_chat_window",
        &["keymap.modified-digits.a-number-with-the-modifier-uses-a-quick-slot-and-not-the-chat-window"],
        a_modified_number_uses_a_quick_slot_and_not_the_chat_window,
    ),
    (
        "the_walk_mode_key_follows_the_run_by_default_option",
        &["movement.walk-mode-key.holding-it-follows-the-run-by-default-option-both-ways"],
        the_walk_mode_key_follows_the_run_by_default_option,
    ),
    (
        "the_walk_mode_option_is_read_on_every_press",
        &["movement.walk-mode-key.the-option-is-read-on-every-press-and-not-at-start-up"],
        the_walk_mode_option_is_read_on_every_press,
    ),
    (
        "the_key_binding_page_builds_one_row_per_bindable_action",
        &["options.key-bindings.the-page-builds-one-row-for-every-bindable-action-once"],
        the_key_binding_page_builds_one_row_per_bindable_action,
    ),
    (
        "a_key_pressed_over_a_row_rebinds_it",
        &["options.key-bindings.a-key-pressed-over-a-row-rebinds-it-and-frees-the-old-key"],
        a_key_pressed_over_a_row_rebinds_it,
    ),
    (
        "a_capture_in_flight_swallows_the_key",
        &["options.key-bindings.a-capture-in-flight-swallows-the-key-and-gives-it-back-afterwards"],
        a_capture_in_flight_swallows_the_key,
    ),
    (
        "the_two_keys_walk_the_fellowship_both_ways_and_wrap",
        &["selection.fellow.one-key-walks-the-fellowship-forward-and-the-other-back-both-wrapping"],
        the_two_keys_walk_the_fellowship_both_ways_and_wrap,
    ),
    (
        "an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing",
        &["selection.fellow.a-selection-outside-the-fellowship-starts-at-one-end-and-with-no-fellowship-nothing-moves"],
        an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing,
    ),
    (
        "the_fellow_cycle_follows_the_panels_order",
        &["selection.fellow.the-cycle-follows-the-order-the-panel-shows"],
        the_fellow_cycle_follows_the_panels_order,
    ),
    (
        "the_key_that_goes_back_is_a_toggle_and_not_a_stack",
        &["selection.previous.the-key-goes-back-one-and-is-a-toggle-rather-than-a-stack"],
        the_key_that_goes_back_is_a_toggle_and_not_a_stack,
    ),
    (
        "with_nothing_behind_it_the_key_selects_nothing",
        &["selection.previous.with-nothing-behind-it-the-key-selects-nothing"],
        with_nothing_behind_it_the_key_selects_nothing,
    ),
    (
        "a_cleared_target_comes_back",
        &["selection.previous.a-target-that-was-cleared-comes-back"],
        a_cleared_target_comes_back,
    ),
    (
        "every_row_shows_the_setting_the_shard_sent",
        &["options.character-page.each-row-shows-the-bit-the-shard-sent-for-it"],
        every_row_shows_the_setting_the_shard_sent,
    ),
    (
        "a_tick_changes_only_that_setting",
        &["options.character-page.a-tick-changes-only-that-bit-of-what-the-shard-sent"],
        a_tick_changes_only_that_setting,
    ),
    (
        "the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones",
        &["options.character-page.the-settings-it-sends-back-are-byte-for-byte-the-ones-a-real-client-sent"],
        the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones,
    ),
    (
        "one_tick_and_one_visit_sends_one_message",
        &["options.character-page.one-tick-and-one-visit-sends-one-message-with-one-bit-moved"],
        one_tick_and_one_visit_sends_one_message,
    ),
    (
        "a_deferred_change_is_flushed_by_the_frame",
        &["options.character-page.a-deferred-change-reaches-the-shard-eight-minutes-later-and-once"],
        a_deferred_change_is_flushed_by_the_frame,
    ),
    (
        "with_no_description_nothing_is_sent",
        &["options.character-page.with-no-description-nothing-is-sent-and-nothing-is-invented"],
        with_no_description_nothing_is_sent,
    ),
    (
        "a_ticked_row_stays_ticked_and_unticking_still_works",
        &["options.character-page.a-row-ticked-on-stays-ticked-and-unticking-still-works"],
        a_ticked_row_stays_ticked_and_unticking_still_works,
    ),
    (
        "a_detent_over_a_tick_box_scrolls_the_list",
        &["pointer.wheel.a-detent-over-a-check-box-scrolls-the-list-and-leaves-the-box-alone"],
        a_detent_over_a_tick_box_scrolls_the_list,
    ),
    (
        "the_same_box_still_answers_a_click_and_so_does_the_bar",
        &["pointer.wheel.the-same-box-still-answers-a-click-and-so-does-the-bar"],
        the_same_box_still_answers_a_click_and_so_does_the_bar,
    ),
    (
        "one_fellowship_setting_turns_the_other_off_at_the_shard",
        &["options.fellowship.turning-one-of-the-two-on-tells-the-shard-the-other-is-off-first"],
        one_fellowship_setting_turns_the_other_off_at_the_shard,
    ),
    (
        "auto_accept_from_the_default_clears_ignoring_at_the_shard",
        &["options.fellowship.the-same-holds-from-the-shipped-default-and-turning-it-off-again-is-one-message"],
        auto_accept_from_the_default_clears_ignoring_at_the_shard,
    ),
    (
        "the_excluded_row_goes_out_at_once_and_cancel_restores_both",
        &["options.character-page.the-excluded-row-goes-out-at-once-and-cancel-restores-both"],
        the_excluded_row_goes_out_at_once_and_cancel_restores_both,
    ),
    (
        "the_chat_options_tab_draws_its_controls",
        &[
            "options.chat-page.the-tab-comes-up-with-every-control-the-page-declares",
            "options.chat-page.the-chat-fonts-face-and-size-sit-under-the-windows-opacity",
        ],
        the_chat_options_tab_draws_its_controls,
    ),
    (
        "a_partly_chosen_group_is_drawn_differently",
        &["options.chat-page.a-partly-chosen-group-and-a-wholly-chosen-one-are-drawn-differently"],
        a_partly_chosen_group_is_drawn_differently,
    ),
    (
        "ticking_a_filter_reaches_the_window_and_sends_nothing",
        &["options.chat-page.ticking-a-filter-reaches-the-window-that-routes-by-it-and-sends-nothing"],
        ticking_a_filter_reaches_the_window_and_sends_nothing,
    ),
    (
        "the_opacity_slider_writes_what_the_fade_reads",
        &["options.chat-page.the-opacity-slider-writes-what-the-fade-reads-and-keeps-the-two-in-order"],
        the_opacity_slider_writes_what_the_fade_reads,
    ),
    (
        "apply_cancel_and_defaults_do_what_an_option_page_does",
        &["options.chat-page.apply-cancel-and-defaults-do-what-an-option-page-does"],
        apply_cancel_and_defaults_do_what_an_option_page_does,
    ),
    (
        "the_exit_button_raises_a_modal_warning",
        &["chargen.exit.the-exit-button-raises-a-modal-warning-in-the-shipped-words"],
        the_exit_button_raises_a_modal_warning,
    ),
    (
        "yes_leaves_the_wizard_and_no_stays_on_the_page",
        &["chargen.exit.saying-yes-goes-back-to-choosing-a-character-and-saying-no-stays-on-the-page"],
        yes_leaves_the_wizard_and_no_stays_on_the_page,
    ),
    (
        "the_back_arrow_is_exit_only_on_the_first_page",
        &["chargen.exit.the-back-arrow-is-exit-on-the-first-page-and-a-step-back-on-any-other"],
        the_back_arrow_is_exit_only_on_the_first_page,
    ),
    (
        "moving_a_window_is_remembered_across_a_rebuild",
        &["window.layout.moving-or-hiding-a-window-is-remembered-and-comes-back-when-the-screen-is-rebuilt"],
        moving_a_window_is_remembered_across_a_rebuild,
    ),
    (
        "a_change_before_the_description_survives_it",
        &["window.layout.a-change-made-before-the-character-is-described-survives-it"],
        a_change_before_the_description_survives_it,
    ),
    (
        "a_saved_layout_is_clamped_and_moves_only_what_it_names",
        &["window.layout.a-saved-layout-is-pulled-onto-the-screen-and-moves-only-the-window-it-names"],
        a_saved_layout_is_clamped_and_moves_only_what_it_names,
    ),
    (
        "a_click_advances_one_picture_and_the_release_is_not_another",
        &["intro.click.a-click-advances-one-picture-and-letting-go-is-not-another"],
        a_click_advances_one_picture_and_the_release_is_not_another,
    ),
    (
        "clicking_through_the_intro_ends_at_character_select",
        &["intro.click.clicking-through-the-sequence-ends-at-character-select-and-not-before"],
        clicking_through_the_intro_ends_at_character_select,
    ),
    (
        "the_quit_action_skips_the_rest_of_the_intro",
        &["intro.quit.the-quit-action-skips-the-rest-of-it"],
        the_quit_action_skips_the_rest_of_the_intro,
    ),
    (
        "any_character_advances_and_escape_skips",
        &["intro.keyboard.any-character-advances-one-picture-and-escape-skips"],
        any_character_advances_and_escape_skips,
    ),
    (
        "a_press_on_a_button_takes_the_keyboard",
        &["focus.press.a-press-on-a-button-takes-the-keyboard-and-the-button-keeps-its-own-look"],
        a_press_on_a_button_takes_the_keyboard,
    ),
    (
        "typing_stops_when_the_keyboard_leaves_the_entry",
        &["focus.press.typing-stops-when-the-keyboard-leaves-the-entry"],
        typing_stops_when_the_keyboard_leaves_the_entry,
    ),
    (
        "a_press_on_a_list_takes_the_keyboard_and_its_look_follows",
        &["focus.press.a-press-on-a-list-takes-the-keyboard-and-its-look-follows"],
        a_press_on_a_list_takes_the_keyboard_and_its_look_follows,
    ),
    (
        "a_press_on_a_scrollbar_takes_the_keyboard_without_moving_it",
        &["focus.press.a-press-on-a-scrollbar-takes-the-keyboard-too"],
        a_press_on_a_scrollbar_takes_the_keyboard_without_moving_it,
    ),
    (
        "a_press_on_something_that_cannot_scroll_moves_the_keyboard_nowhere",
        &["focus.press.a-press-on-something-that-cannot-scroll-moves-the-keyboard-nowhere"],
        a_press_on_something_that_cannot_scroll_moves_the_keyboard_nowhere,
    ),
    (
        "a_character_typed_the_moment_a_screen_takes_the_keyboard_is_not_lost",
        &["text-entry.focus.a-character-typed-the-moment-a-screen-takes-the-keyboard-is-not-lost"],
        a_character_typed_the_moment_a_screen_takes_the_keyboard_is_not_lost,
    ),
    (
        "a_screen_that_takes_the_keyboard_in_its_own_pass_keeps_the_next_character",
        &["text-entry.focus.a-screen-that-takes-the-keyboard-in-its-own-pass-keeps-the-next-character"],
        a_screen_that_takes_the_keyboard_in_its_own_pass_keeps_the_next_character,
    ),
    (
        "the_typing_switch_follows_an_editable_box_and_nothing_else",
        &["text-entry.focus.the-typing-switch-follows-a-box-that-can-be-typed-into-and-is-thrown-once-per-change"],
        the_typing_switch_follows_an_editable_box_and_nothing_else,
    ),
    (
        "the_returning_character_list_selects_the_character_that_was_just_played",
        &["shell-only.character-select.the-returning-list-selects-the-character-just-played"],
        the_returning_character_list_selects_the_character_that_was_just_played,
    ),
    (
        "the_full_screen_setting_is_kept_and_applied_on_entering_the_world",
        &["window.full-screen.the-setting-is-kept-from-the-start-and-applied-on-entering-the-world"],
        the_full_screen_setting_is_kept_and_applied_on_entering_the_world,
    ),
    (
        "the_full_screen_switch_key_is_refused_outside_the_world",
        &["window.full-screen.the-switch-key-is-refused-outside-the-world-and-works-inside-it"],
        the_full_screen_switch_key_is_refused_outside_the_world,
    ),
    (
        "the_options_page_turns_full_screen_on_and_off_mid_session",
        &["window.full-screen.the-options-page-turns-it-on-and-off-while-the-player-plays"],
        the_options_page_turns_full_screen_on_and_off_mid_session,
    ),
    (
        "the_summary_page_lists_every_choice_and_what_it_came_to",
        &["chargen.summary.lists-every-choice-the-wizard-has-made-and-what-they-came-to"],
        the_summary_page_lists_every_choice_and_what_it_came_to,
    ),
    (
        "finish_replaces_the_forward_arrow_on_the_last_page_alone",
        &["chargen.finish.replaces-the-forward-arrow-on-the-last-page-and-does-nothing-anywhere-else"],
        finish_replaces_the_forward_arrow_on_the_last_page_alone,
    ),
    (
        "finish_composes_the_character_the_player_built_and_only_once",
        &["chargen.finish.composes-the-character-the-player-built-and-only-once"],
        finish_composes_the_character_the_player_built_and_only_once,
    ),
    (
        "choosing_a_heritage_lights_its_bullet_and_describes_that_people",
        &["chargen.heritage.one-bullet-is-lit-and-the-page-behind-it-describes-that-people"],
        choosing_a_heritage_lights_its_bullet_and_describes_that_people,
    ),
    (
        "choosing_a_town_lights_its_pin_and_titles_the_page",
        &["chargen.town.one-pin-is-lit-and-the-page-is-titled-and-described-for-that-town"],
        choosing_a_town_lights_its_pin_and_titles_the_page,
    ),
    (
        "the_tab_strip_brightens_only_the_page_it_is_on",
        &["chargen.tabs.the-page-the-player-is-on-is-the-only-bright-one"],
        the_tab_strip_brightens_only_the_page_it_is_on,
    ),
    (
        "the_six_attribute_sliders_are_named_and_sit_at_their_values",
        &["chargen.profession.the-six-sliders-are-named-and-sit-where-their-numbers-say"],
        the_six_attribute_sliders_are_named_and_sit_at_their_values,
    ),
    (
        "the_name_box_prompts_takes_the_keyboard_and_keeps_what_replaced_the_prompt",
        &["chargen.summary.the-name-box-prompts-takes-the-keyboard-and-the-first-character-replaces-the-prompt"],
        the_name_box_prompts_takes_the_keyboard_and_keeps_what_replaced_the_prompt,
    ),
    (
        "the_finish_buttons_caption_is_drawn_in_the_font_the_layout_names",
        &["chargen.finish.its-caption-is-drawn-in-the-font-the-shipped-layout-names"],
        the_finish_buttons_caption_is_drawn_in_the_font_the_layout_names,
    ),
    (
        "the_wizard_opens_on_a_character_already_rolled",
        &["chargen.random.the-wizard-opens-on-a-character-already-rolled"],
        the_wizard_opens_on_a_character_already_rolled,
    ),
    (
        "the_opening_roll_is_the_seeds_own_and_two_clients_roll_the_same_character",
        &["chargen.random.the-opening-roll-is-the-seeds-own-and-two-clients-roll-the-same-character"],
        the_opening_roll_is_the_seeds_own_and_two_clients_roll_the_same_character,
    ),
    (
        "the_people_and_the_sex_come_out_of_their_own_stream",
        &["chargen.random.the-people-and-the-sex-come-out-of-a-different-draw-from-everything-else"],
        the_people_and_the_sex_come_out_of_their_own_stream,
    ),
    (
        "the_opening_people_is_one_a_plain_account_may_play",
        &["chargen.random.the-opening-people-is-one-a-plain-account-may-play-and-the-expansion-adds-one"],
        the_opening_people_is_one_a_plain_account_may_play,
    ),
    (
        "the_home_town_follows_the_people",
        &["chargen.heritage.the-home-town-follows-the-people-and-is-one-of-that-peoples-own"],
        the_home_town_follows_the_people,
    ),
    (
        "the_random_button_re_rolls_the_page_the_player_is_on",
        &["chargen.random.the-button-re-rolls-the-page-the-player-is-on"],
        the_random_button_re_rolls_the_page_the_player_is_on,
    ),
    (
        "the_last_page_warns_before_re_rolling_the_whole_character",
        &["chargen.random.on-the-last-page-it-warns-first-and-only-a-yes-re-rolls-the-whole-character"],
        the_last_page_warns_before_re_rolling_the_whole_character,
    ),
    (
        "the_town_pages_roll_can_land_on_any_town",
        &["chargen.random.on-the-town-page-it-can-land-on-any-town-and-not-only-the-peoples-own"],
        the_town_pages_roll_can_land_on_any_town,
    ),
    (
        "the_insect_peoples_lose_three_tabs_outright",
        &["chargen.tabs.the-two-insect-peoples-lose-three-pages-outright-rather-than-having-them-greyed"],
        the_insect_peoples_lose_three_tabs_outright,
    ),
    (
        "the_face_tab_takes_the_hat_off_and_the_clothes_tab_puts_it_back",
        &["chargen.appearance.the-face-tab-takes-the-hat-off-and-the-clothes-tab-puts-it-back"],
        the_face_tab_takes_the_hat_off_and_the_clothes_tab_puts_it_back,
    ),
    (
        "the_colour_spots_are_the_chosen_parts_own_colours",
        &["chargen.appearance.the-colour-spots-are-the-chosen-parts-own-colours-and-the-spare-ones-are-blank"],
        the_colour_spots_are_the_chosen_parts_own_colours,
    ),
    (
        "the_eyes_have_one_colour_each_and_no_shade_to_slide",
        &["chargen.appearance.the-eyes-have-one-colour-each-and-no-shade-to-slide"],
        the_eyes_have_one_colour_each_and_no_shade_to_slide,
    ),
    (
        "clicking_a_colour_moves_the_marker_and_tints_the_shade_wheel",
        &["chargen.appearance.clicking-a-colour-moves-the-marker-and-tints-the-shade-wheel-with-it"],
        clicking_a_colour_moves_the_marker_and_tints_the_shade_wheel,
    ),
    (
        "the_re_roll_warning_is_a_modal_question_in_the_shipped_words",
        &["chargen.dialogs.the-warning-before-a-re-roll-is-a-modal-question-in-the-shipped-words"],
        the_re_roll_warning_is_a_modal_question_in_the_shipped_words,
    ),
    (
        "the_warnings_two_buttons_answer_it_and_a_pointer_can_press_either",
        &["chargen.dialogs.the-two-answer-buttons-really-answer-and-a-pointer-can-press-either-of-them"],
        the_warnings_two_buttons_answer_it_and_a_pointer_can_press_either,
    ),
    (
        "only_the_last_page_asks_before_re_rolling",
        &["chargen.dialogs.the-warning-before-a-re-roll-is-the-last-pages-alone"],
        only_the_last_page_asks_before_re_rolling,
    ),
    (
        "a_people_the_account_cannot_play_is_refused_in_a_message_box",
        &["chargen.dialogs.a-people-the-account-cannot-play-is-refused-in-a-message-box"],
        a_people_the_account_cannot_play_is_refused_in_a_message_box,
    ),
    (
        "finishing_with_credits_unspent_asks_first",
        &["chargen.dialogs.finishing-with-credits-unspent-asks-first-and-a-yes-goes-on-to-create"],
        finishing_with_credits_unspent_asks_first,
    ),
    (
        "finishing_with_no_name_refuses_in_a_message_box",
        &["chargen.dialogs.finishing-with-no-name-refuses-in-a-message-box"],
        finishing_with_no_name_refuses_in_a_message_box,
    ),
    (
        "the_wizard_never_shows_a_please_wait_box",
        &["chargen.dialogs.the-wizard-never-shows-a-please-wait-box"],
        the_wizard_never_shows_a_please_wait_box,
    ),
    (
        "a_box_answered_from_outside_the_screen_comes_down",
        &["chargen.dialogs.one-answered-from-outside-the-screen-is-taken-down-on-the-next-frame"],
        a_box_answered_from_outside_the_screen_comes_down,
    ),
    (
        "a_second_box_waits_its_turn_rather_than_stacking",
        &["chargen.dialogs.a-second-one-waits-its-turn-rather-than-stacking-on-the-first"],
        a_second_box_waits_its_turn_rather_than_stacking,
    ),
    (
        "a_box_closed_from_underneath_the_screen_is_replaced",
        &["chargen.dialogs.one-closed-from-underneath-the-screen-is-replaced-rather-than-left-behind"],
        a_box_closed_from_underneath_the_screen_is_replaced,
    ),
    (
        "three_panes_and_two_lists_scroll_and_the_town_page_has_none",
        &["chargen.scroll.three-panes-and-two-lists-scroll-and-the-town-page-has-nothing-to-scroll"],
        three_panes_and_two_lists_scroll_and_the_town_page_has_none,
    ),
    (
        "each_pane_drives_the_bar_beside_it",
        &["chargen.scroll.each-pane-drives-the-bar-beside-it-and-not-another-pages"],
        each_pane_drives_the_bar_beside_it,
    ),
    (
        "a_filled_pane_has_a_live_bar_sized_to_what_is_shown",
        &["chargen.scroll.a-pane-with-text-in-it-has-a-live-bar-whose-thumb-is-the-size-of-what-is-shown"],
        a_filled_pane_has_a_live_bar_sized_to_what_is_shown,
    ),
    (
        "dragging_the_thumb_moves_the_rows_by_the_same_fraction",
        &["chargen.scroll.dragging-the-thumb-moves-the-rows-by-the-same-fraction"],
        dragging_the_thumb_moves_the_rows_by_the_same_fraction,
    ),
    (
        "an_arrow_moves_one_row_and_the_two_go_opposite_ways",
        &["chargen.scroll.an-arrow-moves-one-row-and-the-two-arrows-go-opposite-ways"],
        an_arrow_moves_one_row_and_the_two_go_opposite_ways,
    ),
    (
        "a_press_on_the_track_moves_a_whole_page_towards_the_press",
        &["chargen.scroll.a-press-on-the-track-moves-a-whole-page-towards-the-press"],
        a_press_on_the_track_moves_a_whole_page_towards_the_press,
    ),
    (
        "the_wheel_moves_a_list_one_row_and_stops_at_the_top",
        &["chargen.scroll.the-wheel-moves-a-list-one-row-and-stops-at-the-top"],
        the_wheel_moves_a_list_one_row_and_stops_at_the_top,
    ),
    (
        "a_pane_measures_its_whole_paragraph_and_the_blank_row_after_it",
        &["chargen.scroll.a-pane-measures-every-line-of-its-text-its-margins-and-the-blank-row-after-it"],
        a_pane_measures_its_whole_paragraph_and_the_blank_row_after_it,
    ),
    (
        "the_four_headings_sit_above_their_own_groups",
        &["chargen.skills.the-four-headings-sit-above-their-own-groups-and-each-group-is-in-name-order"],
        the_four_headings_sit_above_their_own_groups,
    ),
    (
        "every_row_shows_its_own_score_and_its_own_two_prices",
        &["chargen.skills.every-row-shows-its-own-score-and-its-own-two-prices"],
        every_row_shows_its_own_score_and_its_own_two_prices,
    ),
    (
        "training_a_skill_moves_its_row_and_re_prices_the_rest",
        &["chargen.skills.training-one-moves-its-row-under-the-trained-heading-and-re-prices-the-rest"],
        training_a_skill_moves_its_row_and_re_prices_the_rest,
    ),
    (
        "the_skill_rows_are_stacked_and_none_overlaps",
        &["chargen.skills.the-rows-are-stacked-one-below-another-and-none-overlaps"],
        the_skill_rows_are_stacked_and_none_overlaps,
    ),
    (
        "the_skills_page_reads_the_way_the_original_drew_it",
        &["chargen.skills.the-page-reads-the-way-the-original-drew-it-for-the-same-character"],
        the_skills_page_reads_the_way_the_original_drew_it,
    ),
    (
        "every_row_is_drawn_under_its_new_heading_and_back_again",
        &["chargen.skills.every-row-is-drawn-under-its-new-heading-and-back-again"],
        every_row_is_drawn_under_its_new_heading_and_back_again,
    ),
    (
        "the_reported_press_moves_the_row_it_names",
        &["chargen.skills.the-reported-press-moves-the-row-it-names-and-leaving-the-page-changes-nothing"],
        the_reported_press_moves_the_row_it_names,
    ),
    (
        "a_press_in_the_name_box_puts_the_caret_where_it_was_pressed",
        &["chargen.summary.a-press-in-the-name-box-puts-the-caret-where-it-was-pressed-and-typing-goes-there"],
        a_press_in_the_name_box_puts_the_caret_where_it_was_pressed,
    ),
    (
        "a_press_on_nothing_takes_the_keyboard_away_and_a_press_back_in_returns_it",
        &["chargen.summary.a-press-on-nothing-takes-the-keyboard-away-and-a-press-back-in-the-box-returns-it"],
        a_press_on_nothing_takes_the_keyboard_away_and_a_press_back_in_returns_it,
    ),
    (
        "a_press_that_types_nothing_leaves_a_plain_caret",
        &["chargen.summary.a-press-that-types-nothing-leaves-a-plain-caret-and-only-a-gesture-puts-the-highlight-back"],
        a_press_that_types_nothing_leaves_a_plain_caret,
    ),
    (
        "the_dialog_map_binds_only_escape_and_enter",
        &["keymap.dialog-keys.the-shipped-map-binds-only-escape-and-enter-and-each-once"],
        the_dialog_map_binds_only_escape_and_enter,
    ),
    (
        "enter_advances_the_opening_sequence_and_escape_leaves_it",
        &["dialog-keys.intro.enter-advances-the-opening-sequence-and-escape-leaves-it"],
        enter_advances_the_opening_sequence_and_escape_leaves_it,
    ),
    (
        "a_real_press_of_enter_advances_twice",
        &["dialog-keys.intro.a-real-press-of-enter-advances-twice-because-two-handlers-answer-it"],
        a_real_press_of_enter_advances_twice,
    ),
    (
        "the_opening_screen_answers_only_the_keys_its_own_maps_carry",
        &["dialog-keys.intro.the-screen-answers-the-keys-its-own-maps-carry-and-no-others"],
        the_opening_screen_answers_only_the_keys_its_own_maps_carry,
    ),
    (
        "either_key_ends_the_credit_roll_and_ends_it_once",
        &["dialog-keys.credits.either-key-ends-the-roll-and-ends-it-once"],
        either_key_ends_the_credit_roll_and_ends_it_once,
    ),
    (
        "enter_is_declined_on_the_character_list_and_escape_asks_once",
        &["dialog-keys.character-select.enter-is-declined-and-escape-asks-once-whether-to-quit"],
        enter_is_declined_on_the_character_list_and_escape_asks_once,
    ),
    (
        "a_box_that_takes_typing_adds_the_dialog_map_without_taking_the_keys",
        &["dialog-keys.a-box-that-takes-typing-adds-the-dialog-map-without-taking-the-keys-from-the-box"],
        a_box_that_takes_typing_adds_the_dialog_map_without_taking_the_keys,
    ),
    (
        "leaving_raises_a_modal_question_in_the_shipped_words",
        &["character-select.dialogs.leaving-raises-a-modal-question-in-the-shipped-words"],
        leaving_raises_a_modal_question_in_the_shipped_words,
    ),
    (
        "answering_yes_to_leaving_runs_the_closing_sequence_and_no_stays",
        &["character-select.dialogs.answering-yes-to-leaving-runs-the-closing-sequence-and-no-stays"],
        answering_yes_to_leaving_runs_the_closing_sequence_and_no_stays,
    ),
    (
        "deleting_asks_about_the_character_that_was_picked",
        &["character-select.dialogs.deleting-asks-about-the-character-that-was-picked-and-names-only-them"],
        deleting_asks_about_the_character_that_was_picked,
    ),
    (
        "the_question_is_modal_and_a_press_behind_it_reaches_nothing",
        &["character-select.dialogs.the-question-is-modal-and-a-second-press-behind-it-reaches-nothing"],
        the_question_is_modal_and_a_press_behind_it_reaches_nothing,
    ),
    (
        "only_the_typed_phrase_deletes_and_it_deletes_the_one_that_was_picked",
        &["character-select.delete.only-the-typed-phrase-deletes-and-it-deletes-the-one-that-was-picked"],
        only_the_typed_phrase_deletes_and_it_deletes_the_one_that_was_picked,
    ),
    (
        "cancelling_after_typing_the_phrase_deletes_nothing",
        &["character-select.delete.cancelling-after-typing-the-phrase-deletes-nothing"],
        cancelling_after_typing_the_phrase_deletes_nothing,
    ),
    (
        "restoring_raises_a_box_with_no_buttons_that_the_next_list_takes_down",
        &["character-select.dialogs.restoring-raises-a-box-with-no-buttons-that-the-next-list-takes-down"],
        restoring_raises_a_box_with_no_buttons_that_the_next_list_takes_down,
    ),
    (
        "a_double_press_raises_the_waiting_box_before_it_asks_to_log_on",
        &["character-select.enter-world.a-double-press-raises-the-waiting-box-before-it-asks-to-log-on"],
        a_double_press_raises_the_waiting_box_before_it_asks_to_log_on,
    ),
    (
        "an_error_brought_in_with_the_screen_becomes_a_one_button_message",
        &["character-select.dialogs.an-error-brought-in-with-the-screen-becomes-a-one-button-message"],
        an_error_brought_in_with_the_screen_becomes_a_one_button_message,
    ),
    (
        "one_detent_over_the_chat_log_moves_it_one_line",
        &["pointer.wheel.one-detent-over-the-chat-log-moves-it-one-line-and-the-other-way-puts-it-back"],
        one_detent_over_the_chat_log_moves_it_one_line,
    ),
    (
        "pressing_the_log_is_what_lets_the_wheel_move_it",
        &["pointer.wheel.pressing-the-log-is-what-lets-the-wheel-move-it-and-typing-does-not-take-it-away"],
        pressing_the_log_is_what_lets_the_wheel_move_it,
    ),
    (
        "the_arming_happens_when_the_keyboard_moves_and_once_per_move",
        &["pointer.wheel.the-arming-happens-when-the-keyboard-moves-and-once-per-move"],
        the_arming_happens_when_the_keyboard_moves_and_once_per_move,
    ),
    (
        "a_window_remembers_the_box_that_held_the_keyboard",
        &["window.focus.a-window-remembers-the-box-that-held-the-keyboard-and-gives-it-back"],
        a_window_remembers_the_box_that_held_the_keyboard,
    ),
    (
        "a_cell_shows_the_key_the_way_the_desktop_names_it",
        &["options.key-bindings.a-cell-shows-the-key-the-way-the-desktop-names-it"],
        a_cell_shows_the_key_the_way_the_desktop_names_it,
    ),
    (
        "resting_on_a_cell_says_what_a_press_there_would_do",
        &["options.key-bindings.resting-on-a-cell-says-what-a-press-there-would-do"],
        resting_on_a_cell_says_what_a_press_there_would_do,
    ),
    (
        "every_section_is_titled_in_words",
        &["options.key-bindings.every-section-is-titled-in-words"],
        every_section_is_titled_in_words,
    ),
    (
        "undo_opens_greyed_because_nothing_has_changed_yet",
        &["options.key-bindings.undo-opens-greyed-because-nothing-has-changed-yet"],
        undo_opens_greyed_because_nothing_has_changed_yet,
    ),
    (
        "a_press_on_a_cell_waits_for_a_key_and_undo_puts_the_old_one_back",
        &["options.key-bindings.a-press-on-a-cell-waits-for-a-key-and-undo-puts-the-old-one-back"],
        a_press_on_a_cell_waits_for_a_key_and_undo_puts_the_old_one_back,
    ),
    (
        "a_key_already_in_use_asks_first_and_one_that_cannot_be_taken_refuses",
        &["options.key-bindings.a-key-already-in-use-asks-first-and-one-that-cannot-be-taken-refuses"],
        a_key_already_in_use_asks_first_and_one_that_cannot_be_taken_refuses,
    ),
    (
        "taking_a_key_clears_it_from_the_rows_that_had_it",
        &["options.key-bindings.taking-a-key-clears-it-from-the-rows-that-had-it-on-the-screen"],
        taking_a_key_clears_it_from_the_rows_that_had_it,
    ),
    (
        "the_questions_about_a_key_in_use_are_the_shipped_sentences",
        &["options.key-bindings.the-questions-about-a-key-in-use-are-the-shipped-sentences-with-the-key-and-the-action-in-them"],
        the_questions_about_a_key_in_use_are_the_shipped_sentences,
    ),
    (
        "a_set_of_keys_can_be_saved_under_a_name_and_loaded_back",
        &["options.key-bindings.a-set-of-keys-can-be-saved-under-a-name-and-loaded-back"],
        a_set_of_keys_can_be_saved_under_a_name_and_loaded_back,
    ),
    (
        "saving_over_a_set_asks_first_and_one_that_cannot_be_written_refuses",
        &["options.key-bindings.saving-over-a-set-asks-first-and-one-that-cannot-be-written-refuses"],
        saving_over_a_set_asks_first_and_one_that_cannot_be_written_refuses,
    ),
    (
        "restoring_the_defaults_gives_back_the_shipped_keys",
        &["options.key-bindings.restoring-the-defaults-gives-back-the-shipped-keys-and-not-the-saved-ones"],
        restoring_the_defaults_gives_back_the_shipped_keys,
    ),
    (
        "three_of_the_clients_boxes_take_only_certain_characters",
        &["text-entry.filters.three-of-the-clients-boxes-take-only-certain-characters-and-the-rest-take-anything"],
        three_of_the_clients_boxes_take_only_certain_characters,
    ),
    (
        "the_name_box_takes_the_letters_a_name_may_have",
        &["text-entry.filters.the-name-box-takes-the-letters-a-name-may-have-and-nothing-else"],
        the_name_box_takes_the_letters_a_name_may_have,
    ),
    (
        "the_how_many_box_takes_digits_only",
        &["text-entry.filters.the-how-many-box-takes-digits-only"],
        the_how_many_box_takes_digits_only,
    ),
    (
        "the_how_many_of_a_component_box_takes_digits_only_on_every_row",
        &["text-entry.filters.the-how-many-of-a-component-box-takes-digits-only-on-every-row"],
        the_how_many_of_a_component_box_takes_digits_only_on_every_row,
    ),
    (
        "every_composition_message_is_handed_to_the_desktop",
        &["text-entry.ime.every-composition-message-is-handed-to-the-desktop-and-an-ordinary-character-is-not"],
        every_composition_message_is_handed_to_the_desktop,
    ),
    (
        "a_composed_character_reaches_the_box_unchanged",
        &["text-entry.ime.a-composed-character-reaches-the-box-unchanged"],
        a_composed_character_reaches_the_box_unchanged,
    ),
    (
        "a_paste_puts_the_clipboard_in_once_and_not_a_letter_with_it",
        &["text-entry.paste.a-paste-puts-the-clipboard-in-once-and-not-a-letter-with-it"],
        a_paste_puts_the_clipboard_in_once_and_not_a_letter_with_it,
    ),
    (
        "line_breaks_in_what_was_pasted_never_reach_the_shard",
        &["text-entry.paste.line-breaks-in-what-was-pasted-never-reach-the-shard"],
        line_breaks_in_what_was_pasted_never_reach_the_shard,
    ),
    (
        "the_same_press_works_on_the_character_list_and_in_the_wizard",
        &["pointer.click.the-same-press-works-on-the-character-list-and-in-the-wizard"],
        the_same_press_works_on_the_character_list_and_in_the_wizard,
    ),
    (
        "the_wizards_arrows_and_its_way_out_all_answer_a_press",
        &["pointer.click.the-wizards-arrows-and-its-way-out-all-answer-a-press"],
        the_wizards_arrows_and_its_way_out_all_answer_a_press,
    ),
    (
        "every_button_the_wizard_builds_can_be_pressed",
        &["pointer.click.every-button-the-wizard-builds-can-be-pressed-including-the-three-it-greys"],
        every_button_the_wizard_builds_can_be_pressed,
    ),
    (
        "the_left_button_and_the_wheel_each_belong_to_one_shipped_map",
        &["pointer.the-left-button-and-the-wheel-each-belong-to-one-shipped-map"],
        the_left_button_and_the_wheel_each_belong_to_one_shipped_map,
    ),
    (
        "a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye",
        &["viewport.a-press-in-the-middle-of-a-moved-view-finds-what-is-ahead-of-the-eye"],
        a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye,
    ),
    (
        "a_really_moved_view_measures_a_press_against_itself",
        &["viewport.a-really-moved-view-measures-a-press-against-itself-and-the-default-is-not"],
        a_really_moved_view_measures_a_press_against_itself,
    ),
    (
        "the_character_lists_own_buttons_each_raise_what_they_name",
        &["pointer.click.the-character-lists-own-buttons-each-raise-what-they-name"],
        the_character_lists_own_buttons_each_raise_what_they_name,
    ),
    (
        "one_press_on_a_character_picks_it_and_two_takes_them_into_the_world",
        &["pointer.click.one-press-on-a-character-picks-it-and-two-takes-them-into-the-world"],
        one_press_on_a_character_picks_it_and_two_takes_them_into_the_world,
    ),
    (
        "a_button_lights_under_the_pointer_and_sinks_under_the_press",
        &["pointer.a-button-lights-under-the-pointer-and-sinks-under-the-press"],
        a_button_lights_under_the_pointer_and_sinks_under_the_press,
    ),
    (
        "a_press_dragged_off_a_button_releases_it_without_firing_it",
        &["pointer.a-press-dragged-off-a-button-releases-it-without-firing-it"],
        a_press_dragged_off_a_button_releases_it_without_firing_it,
    ),
    (
        "a_move_makes_what_is_under_the_pointer_the_one_entered",
        &["pointer.a-move-makes-what-is-under-the-pointer-the-one-entered-until-it-leaves"],
        a_move_makes_what_is_under_the_pointer_the_one_entered,
    ),
    (
        "a_press_on_a_toolbar_button_opens_the_panel_it_owns",
        &["pointer.click.a-press-on-a-toolbar-button-opens-the-panel-it-owns"],
        a_press_on_a_toolbar_button_opens_the_panel_it_owns,
    ),
    (
        "a_key_the_roll_does_not_own_leaves_it_running",
        &["dialog-keys.credits.a-key-the-roll-does-not-own-leaves-it-running"],
        a_key_the_roll_does_not_own_leaves_it_running,
    ),
    (
        "only_the_opening_sequence_refuses_a_key_on_its_way_back_up",
        &["dialog-keys.only-the-opening-sequence-refuses-a-key-on-its-way-back-up"],
        only_the_opening_sequence_refuses_a_key_on_its_way_back_up,
    ),
    (
        "the_shards_log_off_answer_does_not_end_a_client_with_screens",
        &["shell-only.log-off.the-shards-answer-does-not-end-a-client-that-has-a-screen-to-go-back-to"],
        the_shards_log_off_answer_does_not_end_a_client_with_screens,
    ),
    (
        "every_character_set_the_session_decodes_is_an_arrival",
        &["shell-only.character-set.every-arrival-is-a-notice-even-when-the-list-has-not-changed"],
        every_character_set_the_session_decodes_is_an_arrival,
    ),
    (
        "a_list_arriving_in_the_world_sends_the_player_back_and_at_the_list_does_not",
        &["shell-only.character-set.a-list-arriving-in-the-world-sends-the-player-back-to-the-character-screen"],
        a_list_arriving_in_the_world_sends_the_player_back_and_at_the_list_does_not,
    ),
    (
        "the_way_into_the_world_opens_again_after_a_log_off",
        &["shell-only.enter-world.the-way-into-the-world-opens-again-after-a-log-off"],
        the_way_into_the_world_opens_again_after_a_log_off,
    ),
    (
        "answering_yes_asks_to_log_off_and_moves_no_screen_of_its_own",
        &["shell-only.log-off.answering-yes-asks-to-log-off-and-moves-no-screen-of-its-own"],
        answering_yes_asks_to_log_off_and_moves_no_screen_of_its_own,
    ),
    (
        "a_pick_the_returning_list_cannot_honour_falls_back_to_the_shards_own_order",
        &["shell-only.character-select.a-pick-the-returning-list-cannot-honour-falls-back-to-the-shards-own-order"],
        a_pick_the_returning_list_cannot_honour_falls_back_to_the_shards_own_order,
    ),
    (
        "the_delete_that_leaves_the_client_carries_the_account_and_the_shards_own_place",
        &["character-select.delete.the-request-that-leaves-the-client-carries-the-account-and-the-shards-own-place"],
        the_delete_that_leaves_the_client_carries_the_account_and_the_shards_own_place,
    ),
    (
        "a_pending_deletion_is_drawn_red_and_last_and_offers_restore",
        &["character-select.delete.a-pending-deletion-is-drawn-red-and-last-and-offers-restore"],
        a_pending_deletion_is_drawn_red_and_last_and_offers_restore,
    ),
    (
        "the_restore_that_leaves_the_client_is_the_characters_own_id_on_the_control_queue",
        &["character-select.restore.the-request-that-leaves-the-client-is-the-characters-own-id-on-the-control-queue"],
        the_restore_that_leaves_the_client_is_the_characters_own_id_on_the_control_queue,
    ),
    (
        "a_restored_character_comes_back_in_its_own_place_and_in_the_ordinary_colour",
        &["character-select.restore.a-restored-character-comes-back-in-its-own-place-and-in-the-ordinary-colour"],
        a_restored_character_comes_back_in_its_own_place_and_in_the_ordinary_colour,
    ),
    (
        "the_restore_answer_overwrites_the_place_it_was_asked_about_and_never_appends",
        &["character-select.restore.the-answer-overwrites-the-place-it-was-asked-about-and-never-appends"],
        the_restore_answer_overwrites_the_place_it_was_asked_about_and_never_appends,
    ),
    (
        "a_refused_restore_takes_the_waiting_box_down_and_says_why",
        &["character-select.restore.a-refusal-takes-the-waiting-box-down-and-says-why-in-the-shipped-words"],
        a_refused_restore_takes_the_waiting_box_down_and_says_why,
    ),
    (
        "deleting_and_restoring_over_the_wire_leaves_the_list_where_it_started",
        &["character-select.round-trip.deleting-and-restoring-over-the-wire-leaves-the-list-where-it-started"],
        deleting_and_restoring_over_the_wire_leaves_the_list_where_it_started,
    ),
    (
        "a_refused_creation_stops_the_wizard_waiting_and_says_why",
        &["chargen.finish.a-refused-creation-stops-the-wizard-waiting-and-says-why"],
        a_refused_creation_stops_the_wizard_waiting_and_says_why,
    ),
    (
        "every_refusal_the_shard_can_send_draws_its_own_sentence",
        &["chargen.finish.every-refusal-the-shard-can-send-draws-its-own-sentence"],
        every_refusal_the_shard_can_send_draws_its_own_sentence,
    ),
    (
        "only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture",
        &["relog.state.only-the-shortcut-and-the-auto-saved-option-reach-the-shard-at-the-gesture"],
        only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture,
    ),
    (
        "every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session",
        &["relog.state.every-deferred-change-is-saved-at-the-log-off-and-found-by-the-next-session"],
        every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session,
    ),
    (
        "a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off",
        &["relog.state.a-session-that-changed-nothing-deferred-saves-nothing-at-the-log-off"],
        a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

/// The host's own resolved key for a physical one.
///
/// # Panics
/// Panics on a key the host has no scan code for, which is not a key this client could ever see.
fn key(code: KeyCode) -> dereth_client::platform::keys::Key {
    key_from_key_code(code).expect("the host names this key")
}

// ---------------------------------------------------------------------------------------------
// keymap.function-keys.*
//
// Guards against the F8/F9 keys opening the wrong pages. The twelve expectations are the shipped
// map's own, read off the generated binding fixture: two of the twelve are deliberately not panels
// and one is deliberately bound to nothing at all, which is what makes "the panel keys open panels"
// a measurement rather than a tautology.
// ---------------------------------------------------------------------------------------------

/// The shipped default for each function key. `None` is a key the shipped map leaves free.
const FUNCTION_KEYS: [(KeyCode, Option<u32>); 12] = [
    (KeyCode::F1, Some(0x7B)),
    (KeyCode::F2, Some(0x3E)),
    (KeyCode::F3, Some(0x1000_000E)),
    (KeyCode::F4, Some(0x1000_000F)),
    (KeyCode::F5, Some(0x1000_0011)),
    (KeyCode::F6, Some(0x1000_0012)),
    (KeyCode::F7, None),
    (KeyCode::F8, Some(0x1000_0014)),
    (KeyCode::F9, Some(0x1000_0015)),
    (KeyCode::F10, Some(0x1000_0016)),
    (KeyCode::F11, Some(0x1000_001A)),
    (KeyCode::F12, Some(0x1000_0019)),
];

/// Every function key produces the action the shipped map binds it to, and the free one produces
/// none.
pub fn every_function_key_does_what_the_shipped_map_says() {
    let mut kb = BareKeyboard::new();
    let got: Vec<(KeyCode, Vec<u32>)> = FUNCTION_KEYS
        .iter()
        .map(|(k, _)| (*k, kb.tap(key(*k))))
        .collect();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "keymap.function-keys.each-one-does-what-the-shipped-map-binds-it-to",
        move |_| {
            got.iter()
                .zip(FUNCTION_KEYS)
                .all(|((_, fired), (_, want))| *fired == want.into_iter().collect::<Vec<u32>>())
        },
    );
}

#[test]
fn scenario_every_function_key_does_what_the_shipped_map_says() {
    scenario("every_function_key_does_what_the_shipped_map_says");
}

/// Each function key bound to a panel opens that panel, and closes it again on a second press.
pub fn each_panel_function_key_opens_and_shuts_its_page() {
    use dereth_ui::props::attr;

    /// Everything under the current screen whose shipped layout says it listens for `action`.
    fn listeners(c: &HeadlessClient, action: u32) -> Vec<dereth_ui::ElemHandle> {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the UI shell is up");
        let mut stack: Vec<dereth_ui::ElemHandle> = shell
            .flow
            .current()
            .expect("a screen is current")
            .roots()
            .to_vec();
        let mut out = Vec::new();
        while let Some(h) = stack.pop() {
            if shell
                .ui
                .node(h)
                .expect("live")
                .merged_properties()
                .get_enum(attr::INPUT_ACTION)
                == Some(action)
            {
                out.push(h);
            }
            stack.extend(shell.ui.children(h));
        }
        out
    }

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut hands = Hands::new();

    // Only the keys the shipped map binds to a panel action; the other three are the calibration
    // above, not pages.
    let panels: Vec<(KeyCode, u32)> = FUNCTION_KEYS
        .into_iter()
        .filter_map(|(k, a)| a.map(|a| (k, a)))
        .filter(|(_, a)| *a >= 0x1000_0000)
        .collect();

    let mut had_listeners = true;
    let mut opened = Vec::new();
    let mut shut = Vec::new();
    for (code, action) in panels.clone() {
        let targets = listeners(&c, action);
        had_listeners &= !targets.is_empty();
        hands.tap(&mut c, key(code));
        c.tick(1);
        {
            let app = c.view().expect_app();
            let ui = &app.ui().expect("the UI shell is up").ui;
            opened.push((code, targets.iter().all(|h| ui.is_visible(*h))));
        }
        hands.tap(&mut c, key(code));
        c.tick(1);
        {
            let app = c.view().expect_app();
            let ui = &app.ui().expect("the UI shell is up").ui;
            shut.push((code, targets.iter().all(|h| !ui.is_visible(*h))));
        }
    }

    c.assert_behaviour(
        "keymap.function-keys.each-panel-key-opens-its-own-page-and-shuts-it-again",
        move |_| {
            !panels.is_empty()
                && had_listeners
                && opened.iter().all(|(_, ok)| *ok)
                && shut.iter().all(|(_, ok)| *ok)
        },
    );
    c.shutdown();
}

#[test]
fn scenario_each_panel_function_key_opens_and_shuts_its_page() {
    scenario("each_panel_function_key_opens_and_shuts_its_page");
}

// ---------------------------------------------------------------------------------------------
// keymap.rebind.*
//
// The key the rebind uses is the one key in both shipped maps that fires nothing, which is what
// makes "it walks now" unambiguous; the key it is taken from is the shipped one for walking
// forward.
// ---------------------------------------------------------------------------------------------

/// The movement map and the walk-forward action, as the shipped binding fixture names them.
const MOVEMENT: dereth_input::InputMapId = dereth_input::InputMapId(4);
const MOVE_FORWARD: dereth_input::ActionId = dereth_input::ActionId(0x29);
/// The scan codes of the key that walks and the key that is free.
const SCAN_W: u16 = 0x11;
const SCAN_FREE: u16 = 0x41;

/// The control a binding is, and the same control as the key-hit handler receives it: on release.
fn control(offset: u16, activation: u32) -> dereth_input::ControlChord {
    use dereth_input::spec::{ControlCode, SubControlIndex};
    dereth_input::ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, offset),
        0,
        activation,
    )
}

/// Hold `code` for three frames and answer whether the body ever walked the way `read` looks.
fn held_moves(
    c: &mut HeadlessClient,
    hands: &mut Hands,
    code: KeyCode,
    read: fn(&dereth_client::app::App) -> bool,
) -> bool {
    // **Held and not tapped.** A down and an up inside one frame cancel before the frame's sweep
    // looks, so a tap reports "did not walk" for a reason that has nothing to do with the binding.
    hands.press(c, key(code));
    let mut ever = false;
    for _ in 0..3 {
        c.tick(1);
        ever |= read(c.view().expect_app());
    }
    hands.release(c, key(code));
    c.tick(1);
    ever
}

fn forward(app: &dereth_client::app::App) -> bool {
    app.char_input().forward
}

fn backward(app: &dereth_client::app::App) -> bool {
    app.char_input().back
}

/// A rebound key walks the body and the key it was taken from stops walking it.
pub fn a_rebound_key_walks_the_body_and_the_old_one_stops() {
    use dereth_input::binding::{Capture, DO_NOTHING};

    let mut c = HeadlessClient::new(ClientSpec {
        static_scene: true,
        ..ClientSpec::gameplay(6)
    });
    let mut hands = Hands::new();

    // 1. Before: the shipped key walks and the free one does not -- and produced no action at
    //    all, so it is genuinely free rather than bound to something that does nothing.
    let shipped_walks = held_moves(&mut c, &mut hands, KeyCode::KeyW, forward);
    let fired_before = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired;
    let free_walks = held_moves(&mut c, &mut hands, KeyCode::F7, forward);
    let fired_after = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired;

    // 2. The rebind, through the client's own capture policy rather than into the map behind it.
    let (captured_click, no_conflict) = {
        let shell = c.app_mut().input_manager_mut().expect("the input shell");
        let slot = shell
            .keys_for_action(MOVE_FORWARD, MOVEMENT)
            .iter()
            .position(|k| *k == control(SCAN_W, dereth_input::spec::activation::CLICK))
            .expect("the shipped key is one of the action's keys");
        let Capture::Ready {
            control: new,
            conflicts,
        } = shell.capture_key_hit(
            MOVEMENT,
            MOVE_FORWARD,
            control(SCAN_FREE, dereth_input::spec::activation::UP),
            false,
        )
        else {
            panic!("the free key can be captured for an action");
        };
        let forced_to_click = new.activation == dereth_input::spec::activation::CLICK;
        assert!(shell.set_binding(MOVEMENT, MOVE_FORWARD, Some(slot), new));
        (forced_to_click, conflicts.is_empty())
    };

    // 3. The new key walks.
    let rebound_walks = held_moves(&mut c, &mut hands, KeyCode::F7, forward);

    // 4. The old one does not -- with two denominators, because "the shipped key stopped walking"
    //    and "input stopped arriving" look identical from one measurement.
    let fired_before_old = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired;
    let old_walks = held_moves(&mut c, &mut hands, KeyCode::KeyW, forward);
    let fired_after_old = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired;
    let unrelated_still_works = held_moves(&mut c, &mut hands, KeyCode::KeyX, backward);
    let freed = {
        let shell = c.app_mut().input_manager_mut().expect("the input shell");
        shell.keys_for_action(DO_NOTHING, MOVEMENT)
            == vec![control(SCAN_W, dereth_input::spec::activation::CLICK)]
    };

    c.assert_behaviour(
        "keymap.rebind.a-rebound-key-moves-the-body-and-the-old-one-stops",
        move |_| {
            shipped_walks
            && !free_walks
            && fired_after == fired_before
            && captured_click
            && no_conflict
            && rebound_walks
            && !old_walks
            // The freed key produces nothing at all, which is why the live denominator has to be
            // a different key rather than an action count.
            && fired_after_old == fired_before_old
            && unrelated_still_works
            && freed
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_rebound_key_walks_the_body_and_the_old_one_stops() {
    scenario("a_rebound_key_walks_the_body_and_the_old_one_stops");
}

/// The rebind is written into a file beside the player's preferences when the client exits, and a
/// client with nowhere to keep them writes nothing.
///
/// **This one owns its `App` outright**: the claim is about what the *shutdown sequence* did, and
/// `HeadlessClient::shutdown` takes the client by value and drops the log it answers with. See
/// `dereth_testkit::adapters_shell::AppSpec`; that is a gap in the harness, not in the client.
pub fn a_rebind_is_written_beside_the_preferences() {
    use dereth_client::shutdown::{Outcome, Step};
    use dereth_input::binding::Capture;

    let prefs = scratch_preferences("shell-saved");
    let mut app = build_app(&AppSpec {
        preferences_file: Some(prefs.clone()),
        ..AppSpec::in_gameplay(4)
    });

    let shell = app.input_manager_mut().expect("the input shell");
    let path = shell
        .keymap_path()
        .expect("a preferences file gives a keymap path")
        .to_path_buf();
    let beside = path.parent() == prefs.parent()
        && path.extension().and_then(std::ffi::OsStr::to_str) == Some("keymap");
    let _ = std::fs::remove_file(&path);

    let slot = shell
        .keys_for_action(MOVE_FORWARD, MOVEMENT)
        .iter()
        .position(|k| *k == control(SCAN_W, dereth_input::spec::activation::CLICK))
        .expect("the shipped key");
    let Capture::Ready { control: new, .. } = shell.capture_key_hit(
        MOVEMENT,
        MOVE_FORWARD,
        control(SCAN_FREE, dereth_input::spec::activation::UP),
        false,
    ) else {
        panic!("the free key can be captured");
    };
    assert!(shell.set_binding(MOVEMENT, MOVE_FORWARD, Some(slot), new));

    let log = app.shutdown();
    let saved = log
        .0
        .iter()
        .find(|(s, _)| *s == Step::SaveKeyMap)
        .map(|(_, o)| *o);
    let text = std::fs::read_to_string(&path).expect("the key map file exists");
    // The whole merged map, not a diff: without the freeing of the old key the shipped binding
    // would come back on the next run.
    let carries_both = text.contains("DIK_F7") && text.contains("DoNothing");
    let whole_map = text.len() > 4_000;
    let _ = std::fs::remove_file(&path);

    // The other direction: nowhere to keep preferences, nothing written.
    let bare = build_app(&AppSpec::in_gameplay(2));
    let no_path = {
        let mut bare = bare;
        let none = bare
            .input_manager_mut()
            .expect("the input shell")
            .keymap_path()
            .is_none();
        let log = bare.shutdown();
        none && log
            .0
            .iter()
            .find(|(s, _)| *s == Step::SaveKeyMap)
            .map(|(_, o)| *o)
            == Some(Outcome::Nothing)
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "keymap.rebind.is-written-beside-the-preferences-on-a-clean-exit",
        move |_| beside && saved == Some(Outcome::Ran) && carries_both && whole_map && no_path,
    );
}

#[test]
fn scenario_a_rebind_is_written_beside_the_preferences() {
    scenario("a_rebind_is_written_beside_the_preferences");
}

// ---------------------------------------------------------------------------------------------
// keymap.system-keys.* and window.system-keys.*
//
// The desktop's system keys are bound, not left as an empty and unexplained map. The two rows are
// its two halves: the *action* half, where all four are consumed and do nothing, and the *window*
// half, where the three that arrive as system keys are not treated alike.
// ---------------------------------------------------------------------------------------------

/// The desktop's four, in the client's own map.
const SYSTEM_KEYS: dereth_input::InputMapId = dereth_input::InputMapId(0x10);

/// The four are bound last of all and swallowed without doing anything.
pub fn the_desktops_four_are_taken_last_and_do_nothing() {
    use dereth_client::interaction::{action as ia, Interaction};
    use dereth_client::objects::ObjectStream;
    use dereth_input::ActionId;
    use dereth_primitives::{LocalTime, ObjectId};

    const THE_FOUR: [u32; 4] = [
        ia::SYSTEM_ALT_TAB,
        ia::SYSTEM_ALT_ENTER,
        ia::SYSTEM_ALT_F4,
        ia::SYSTEM_CTRL_SHIFT_ESC,
    ];

    let store = dereth_dat::testing::open_store()
        .expect("the shipped key maps live in the retail data files");

    // 1. The shipped data binds all four, in that map, and the map is walked last.
    let (all_bound, last_in_the_walk) = {
        let shell = dereth_client::input::InputShell::new(&store, None).expect("the input tables");
        let section = shell
            .manager
            .keymap
            .section(SYSTEM_KEYS)
            .expect("the shipped section");
        let bound = THE_FOUR
            .iter()
            .all(|a| !section.keys_for_action(ActionId(*a)).is_empty());
        let entries = shell.manager.maps.entries();
        let at = entries
            .iter()
            .position(|e| e.map == SYSTEM_KEYS)
            .expect("the map is registered");
        (bound, at == entries.len() - 1)
    };

    // 2. Each of the four is consumed and changes nothing at all.
    let press = |action: u32| -> (
        usize,
        dereth_client::interaction::InteractionStats,
        dereth_client::interaction::InteractionStats,
        Option<ObjectId>,
    ) {
        let mut objects = ObjectStream::new();
        objects.world.player = Some(ObjectId(0x5000_0001));
        let mut inter = Interaction::new();
        let before = inter.stats;
        let e = dereth_client_runtime::actions::Action {
            id: ActionId(action),
            phase: dereth_client_runtime::actions::ActionPhase::Begin,
            extent: 1.0,
            repeats: 0,
        };
        let (unowned, left) = dereth_client::interaction::use_time(
            &mut inter,
            &store,
            None,
            &mut objects,
            None,
            vec![e],
            false,
            (800, 600),
            LocalTime(2.0),
        );
        assert!(unowned.is_empty(), "no unowned UI request was expected");
        (left.len(), before, inter.stats, objects.world.selected)
    };

    let mut swallowed = true;
    for action in THE_FOUR {
        let (left, before, after, selected) = press(action);
        // Consumed, counted once, and nothing else about the client moved: the arm has no body.
        let mut only_the_counter = after;
        only_the_counter.system_keys_swallowed = before.system_keys_swallowed;
        swallowed &= left == 0
            && after.system_keys_swallowed == before.system_keys_swallowed + 1
            && only_the_counter == before
            && selected.is_none();
    }

    // 3. The calibration, so "the arm ran" is a measurement: an action the client has no arm for
    //    comes back unconsumed and does not touch that counter.
    let (left, _, after, _) = press(0x0000_006F);
    let unrelated_is_handed_back = left == 1 && after.system_keys_swallowed == 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "keymap.system-keys.the-four-the-desktop-owns-are-taken-last-and-do-nothing",
        move |_| all_bound && last_in_the_walk && swallowed && unrelated_is_handed_back,
    );
}

#[test]
fn scenario_the_desktops_four_are_taken_last_and_do_nothing() {
    scenario("the_desktops_four_are_taken_last_and_do_nothing");
}

/// The switcher never reaches the desktop; the full-screen toggle and the close do.
pub fn the_window_keeps_the_switcher_and_passes_the_other_two() {
    use dereth_input::win32::{syskey_is_consumed, SYS_KEYS_ENABLED};

    const VK_TAB: usize = 0x09;
    const VK_RETURN: usize = 0x0D;
    const VK_F4: usize = 0x73;
    const VK_ESCAPE: usize = 0x1B;

    let switcher_is_eaten = syskey_is_consumed(VK_TAB);
    let full_screen_passes = !syskey_is_consumed(VK_RETURN);
    let close_passes = !syskey_is_consumed(VK_F4);
    // Shown rather than asserted away: the fourth of the desktop's four arrives with no modifier
    // at all, so this rule is not what decides it -- and under the rule it would have been eaten.
    let the_rule_is_not_the_decider_for_the_fourth = syskey_is_consumed(VK_ESCAPE);
    let premise = !SYS_KEYS_ENABLED;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.system-keys.the-desktop-keeps-the-two-it-must-and-the-client-eats-the-third",
        move |_| {
            switcher_is_eaten
                && full_screen_passes
                && close_passes
                && the_rule_is_not_the_decider_for_the_fourth
                && premise
        },
    );
}

#[test]
fn scenario_the_window_keeps_the_switcher_and_passes_the_other_two() {
    scenario("the_window_keeps_the_switcher_and_passes_the_other_two");
}

// ---------------------------------------------------------------------------------------------
// keymap.walk-order.* and text-entry.focus.*
//
// The four action numbers and the two registration triples are a transcription of the generated
// binding fixture and carry no row of their own; these scenarios assert what they do.
// ---------------------------------------------------------------------------------------------

/// The shipped chat entry.
const CHAT_ENTRY: ElementId = ElementId(0x1000_0016);

/// The maps are walked in the order the client registers them.
pub fn the_chat_map_is_walked_first_and_movement_before_the_camera() {
    /// The chat window's own map, the one that leaves the chat bar, and the camera's.
    const CHAT: u32 = 0x1000_000A;
    const LEAVE_THE_BAR: u32 = 0x1000_000D;
    const CAMERA: u32 = 5;
    const EMOTES: u32 = 0x1000_0006;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let (band, all): (Vec<u32>, Vec<(u32, i32)>) = {
        let shell = c.app_mut().input_manager_mut().expect("the input shell");
        let entries = shell.manager.maps.entries();
        (
            entries
                .iter()
                .filter(|e| e.priority == dereth_input::dispatch::priority::GAMEPLAY)
                .map(|e| e.map.0)
                .collect(),
            entries.iter().map(|e| (e.map.0, e.priority)).collect(),
        )
    };

    let at = move |b: &[u32], m: u32| b.iter().position(|x| *x == m);
    let in_all = move |a: &[(u32, i32)], m: u32| a.iter().position(|(x, _)| *x == m);

    c.assert_behaviour("keymap.walk-order.the-chat-map-is-walked-first-and-movement-before-the-camera", move |_| {
        // The chat window's own map is first of its band, movement is still ahead of the camera,
        // and the emote map is ahead of movement -- the client's own registration order, which is
        // the thing a reordering defect changes.
        at(&band, CHAT) == Some(0)
            && at(&band, MOVEMENT.0) < at(&band, CAMERA)
            && at(&band, EMOTES) < at(&band, MOVEMENT.0)
            // At peace no combat-mode map is in the band at all.
            && !band.iter().any(|m| dereth_input::combat::MODE_COMBAT_MAPS.iter().any(|x| x.0 == *m))
            // And the map that leaves the chat bar sits above the whole band, which is above the
            // barrier a focused text box puts in front of the keyboard.
            && in_all(&all, LEAVE_THE_BAR) < in_all(&all, MOVEMENT.0)
    });
    c.shutdown();
}

#[test]
fn scenario_the_chat_map_is_walked_first_and_movement_before_the_camera() {
    scenario("the_chat_map_is_walked_first_and_movement_before_the_camera");
}

/// Whether the client is armed to swallow the next character, and whether it is in text mode.
fn swallow_armed(c: &mut HeadlessClient) -> bool {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .text
        .ignore_next_char
}

fn text_mode(c: &mut HeadlessClient) -> bool {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .text
        .text_mode
}

/// How many characters the shipped text element has actually been handed -- the denominator
/// without which "the box is empty" and "nothing was ever offered" are the same observation.
fn characters_delivered(c: &HeadlessClient) -> u64 {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .characters_delivered
}

/// Opening the chat bar with a key arms the swallow; focusing it with the mouse does not.
pub fn opening_the_chat_bar_with_a_key_swallows_its_own_character() {
    use dereth_testkit::Player;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hands = Hands::new();

    let entry = element(&c, CHAT_ENTRY);
    let nothing_focused = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        .is_none();

    // ---- the mouse, which must NOT arm it ----------------------------------------------------
    // It runs first, so the key half below cannot pass on a latch this half left standing.
    // Five live elements carry the chat entry's template id once the gameplay screen is up; the
    // first is the one clicked here, named explicitly because `Target::Element` on a templated id
    // refuses rather than silently answering the first.
    c.when(Player::click_nth(CHAT_ENTRY, 0));
    let click_focused = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        == Some(entry);
    let click_typing = text_mode(&mut c);
    let click_armed = swallow_armed(&mut c);

    // Back to a box with no focus, which is where the key half starts.
    {
        let app = c.app_mut();
        app.ui_mut().expect("shell").ui.relinquish_focus(entry);
    }
    c.tick(1);
    let edges_before = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .stats
        .text_mode_edges;
    let late_before = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .stats
        .text_mode_edges_late;

    // ---- the key, which must arm it ----------------------------------------------------------
    hands.press(&mut c, key(KeyCode::Enter));
    // The character that key itself types, before any frame -- exactly as the pump produces it.
    let offered_at = characters_delivered(&c);
    hands.character(&mut c, char::from(0x0D_u8));
    let activation_never_arrives = characters_delivered(&c) == offered_at;
    c.tick(1);
    hands.release(&mut c, key(KeyCode::Enter));

    let key_focused = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        == Some(entry);
    let key_typing = text_mode(&mut c);
    let key_armed = swallow_armed(&mut c);
    let (edges, late) = {
        let s = c.view().expect_app().ui().expect("shell").stats;
        (s.text_mode_edges, s.text_mode_edges_late)
    };

    // ---- and leaving the bar arms nothing ----------------------------------------------------
    hands.tap(&mut c, key(KeyCode::Tab));
    let left_the_bar = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        .is_none();
    let left_typing = text_mode(&mut c);
    let left_armed = swallow_armed(&mut c);

    c.assert_behaviour(
        "text-entry.focus.opening-the-chat-bar-with-a-key-swallows-that-keys-own-character",
        move |_| {
            nothing_focused
            && click_focused && click_typing && !click_armed
            && activation_never_arrives
            && key_focused && key_typing && key_armed
            // Exactly one edge, written where the arming can happen and not by the end-of-frame
            // mirror -- an edge written there is outside the dispatch and can arm nothing.
            && edges == edges_before + 1
            && late == late_before
            && left_the_bar && !left_typing && !left_armed
        },
    );
    c.shutdown();
}

#[test]
fn scenario_opening_the_chat_bar_with_a_key_swallows_its_own_character() {
    scenario("opening_the_chat_bar_with_a_key_swallows_its_own_character");
}

/// The swallow takes one character and one only.
pub fn the_armed_swallow_eats_exactly_one_character() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hands = Hands::new();
    let entry = element(&c, CHAT_ENTRY);

    hands.press(&mut c, key(KeyCode::Enter));
    c.tick(1);
    hands.release(&mut c, key(KeyCode::Enter));
    let focused = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        == Some(entry);
    let armed = swallow_armed(&mut c) && text_mode(&mut c);

    // A character with the swallow up and no key press between: the only thing that can stop it
    // now is the swallow itself.
    let offered_at = characters_delivered(&c);
    hands.character(&mut c, 'q');
    c.tick(1);
    let eaten = characters_delivered(&c) == offered_at;
    let disarmed = !swallow_armed(&mut c);

    // The very next one is typed normally -- without this the claim would hold on a client that
    // never delivers a character at all.
    hands.character(&mut c, 'h');
    c.tick(1);
    let next_is_typed = characters_delivered(&c) == offered_at + 1;

    c.assert_behaviour(
        "text-entry.focus.the-armed-swallow-eats-exactly-one-character",
        move |_| focused && armed && eaten && disarmed && next_is_typed,
    );
    c.shutdown();
}

#[test]
fn scenario_the_armed_swallow_eats_exactly_one_character() {
    scenario("the_armed_swallow_eats_exactly_one_character");
}

// ---------------------------------------------------------------------------------------------
// text-entry.backspace.*
//
// One Backspace deletes one character; held, it repeats on the shell's own timing. The expected
// repeat count is computed from the timing the shell is actually running with rather than from the
// system defaults, so the claim is about the client's own sweep and not about the machine that ran
// it.
// ---------------------------------------------------------------------------------------------

/// The text the shipped entry holds.
fn entry_text(c: &mut HeadlessClient) -> String {
    let h = element(c, CHAT_ENTRY);
    c.app_mut()
        .ui_mut()
        .expect("shell")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// Focus the chat entry the way the player does and fill it with `line`.
fn a_line_being_typed(c: &mut HeadlessClient, hands: &mut Hands, line: &str) {
    hands.tap(c, key(KeyCode::Enter));
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .expect("shell")
            .ui
            .focus_element(),
        Some(element(c, CHAT_ENTRY)),
        "the chat bar opened"
    );
    // The character that key itself typed, swallowed by the arming above.
    hands.character(c, char::from(0x0D_u8));
    c.tick(1);
    hands.type_text(c, line);
    assert_eq!(entry_text(c), line, "the box holds the whole line");
}

/// One press deletes one character; a hold repeats at the system's own rate.
pub fn one_backspace_deletes_one_character_and_a_hold_repeats() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hands = Hands::new();
    let line = "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789";
    a_line_being_typed(&mut c, &mut hands, line);

    let (delay, speed) = {
        let t = c
            .app_mut()
            .input_manager_mut()
            .expect("the input shell")
            .manager
            .actions
            .repeat;
        (t.delay, t.speed)
    };

    // The press, with the character it translates to -- exactly what the keyboard delivers.
    let before = entry_text(&mut c).chars().count();
    // **The client's own clock, not a frame count.** What the repeat sweep measures against is the
    // time the action began, and the client stamps that with the clock it is holding when the key
    // goes down -- which is this reading, before the message is delivered and before the frame
    // that acts on it. Reading it here rather than multiplying frames by the step keeps the
    // arithmetic below the client's own instead of a second opinion about which frame the press
    // landed on.
    let began = c.view().expect_app().clock().cur_time;
    hands.press(&mut c, key(KeyCode::Backspace));
    hands.character(&mut c, char::from(0x08_u8));
    c.tick(1);
    let after_press = entry_text(&mut c).chars().count();

    // Held, with no release: what each frame takes out of the box over about two seconds, and how
    // long the client thought it had been held when it took it.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (2.0 / dereth_client::app::HEADLESS_STEP).round() as u64;
    let mut per_frame: Vec<(f64, usize)> = Vec::new();
    let mut prev = after_press;
    for _ in 0..frames {
        c.tick(1);
        let now = entry_text(&mut c).chars().count();
        let held = c.view().expect_app().clock().cur_time - began;
        per_frame.push((held, prev - now));
        prev = now;
    }
    let total = before - prev;
    let held_for = c.view().expect_app().clock().cur_time - began;

    // The client's own arithmetic: one on the press edge, then nothing until the delay and one
    // per interval after it.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let expected = if held_for < delay {
        1
    } else {
        1 + ((held_for - delay) / speed).trunc() as usize + 1
    };
    // **A frame short of the delay, not up to it.** The boundary frame is the one the delay names,
    // and whether the client's own stamp puts it a half-ulp either side of it is not what this
    // claim is about; what it is about is that nothing repeats in the four hundred and fifty
    // milliseconds before it. One repeat interval of tolerance is allowed, and it is stated rather
    // than folded into the comparison.
    let nothing_before_the_delay: usize = per_frame
        .iter()
        .filter(|(held, _)| *held < delay - speed)
        .map(|(_, n)| *n)
        .sum();

    // Let go, and it stops.
    hands.release(&mut c, key(KeyCode::Backspace));
    c.tick(1);
    let at_release = entry_text(&mut c).chars().count();
    c.tick(10);
    let after_release = entry_text(&mut c).chars().count();

    c.assert_behaviour("text-entry.backspace.one-press-deletes-one-character-and-a-hold-repeats-at-the-systems-rate", move |_| {
        // The line has to be longer than the count or the box could bottom out and the
        // measurement would be of its length rather than of the sweep.
        line.chars().count() > expected
            && before - after_press == 1
            && total == expected
            && nothing_before_the_delay == 0
            && after_release == at_release
    });
    c.shutdown();
}

#[test]
fn scenario_one_backspace_deletes_one_character_and_a_hold_repeats() {
    scenario("one_backspace_deletes_one_character_and_a_hold_repeats");
}

/// A tap deletes one character and nothing follows it, ever.
pub fn a_backspace_tap_deletes_one_and_nothing_follows() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut hands = Hands::new();
    a_line_being_typed(&mut c, &mut hands, "hello");

    hands.press(&mut c, key(KeyCode::Backspace));
    hands.character(&mut c, char::from(0x08_u8));
    hands.release(&mut c, key(KeyCode::Backspace));
    c.tick(1);
    let after_tap = entry_text(&mut c);
    c.tick(40);
    let much_later = entry_text(&mut c);

    c.assert_behaviour(
        "text-entry.backspace.a-tap-deletes-one-character-and-nothing-follows-it",
        move |_| after_tap == "hell" && much_later == "hell",
    );
    c.shutdown();
}

#[test]
fn scenario_a_backspace_tap_deletes_one_and_nothing_follows() {
    scenario("a_backspace_tap_deletes_one_and_nothing_follows");
}

// ---------------------------------------------------------------------------------------------
// The action bench
//
// Three scenarios here drive one input action straight through the frame's own tail --
// `dereth_client::interaction::use_time`, which is what the action handler's caller uses here --
// and read the model back. They need no screen and no element tree, only the shipped tables, so
// they are stood up directly rather than through a whole client; the claim is booked at the end
// through a model client, the shape the login scenarios use for a scenario whose subject is not the
// game model.
// ---------------------------------------------------------------------------------------------

/// One press of one action, through the frame's own tail.
struct ActionBench {
    store: dereth_dat::RetailDatStore,
    objects: dereth_client::objects::ObjectStream,
    inter: dereth_client::interaction::Interaction,
    now: f64,
}

impl ActionBench {
    fn new() -> Self {
        Self {
            store: dereth_dat::testing::open_store()
                .expect("the frame's tail takes the retail data files: set DERETH_TEST_DAT_DIR"),
            objects: dereth_client::objects::ObjectStream::new(),
            inter: dereth_client::interaction::Interaction::new(),
            now: 1.0,
        }
    }

    /// Press `action` and answer how many events came back **unconsumed** -- which is the half a
    /// fix that consumed the key and did nothing would also have to pass.
    fn press(&mut self, _map: dereth_input::InputMapId, action: u32) -> usize {
        self.now += 1.0;
        let e = dereth_client_runtime::actions::Action {
            id: dereth_input::ActionId(action),
            phase: dereth_client_runtime::actions::ActionPhase::Begin,
            extent: 1.0,
            repeats: 0,
        };
        let (unowned, left) = dereth_client::interaction::use_time(
            &mut self.inter,
            &self.store,
            None,
            &mut self.objects,
            None,
            vec![e],
            false,
            (800, 600),
            dereth_primitives::LocalTime(self.now),
        );
        assert!(unowned.is_empty(), "no unowned UI request was expected");
        left.len()
    }

    /// A press that must be consumed.
    fn hit(&mut self, map: dereth_input::InputMapId, action: u32) {
        assert_eq!(
            self.press(map, action),
            0,
            "{action:#010X} must be consumed"
        );
    }

    fn select(&mut self, id: Option<dereth_primitives::ObjectId>) {
        let mut out = dereth_client_model::RecordingSink::default();
        self.objects.world.set_selected_object(id, false, &mut out);
    }

    fn selected(&self) -> Option<dereth_primitives::ObjectId> {
        self.objects.world.selected
    }
}

/// Where the shipped map declares the selection actions.
const ITEM_SELECTION: dereth_input::InputMapId = dereth_input::InputMapId(0x1000_0007);

/// What a live press of `qc` resolves to over the shipped registration stack -- the client's own
/// map walk, presented the way a key going down is presented to it.
fn resolves_to(
    shell: &dereth_client::input::InputShell,
    qc: &dereth_input::ControlChord,
) -> Option<(dereth_input::InputMapId, dereth_input::ActionId)> {
    use dereth_input::spec::{activation, ControlChord, DeviceType};
    let km = &shell.manager.keymap;
    let live = ControlChord::new(
        qc.control,
        qc.meta_mode,
        (qc.activation & !activation::UP) | activation::LIVE,
    );
    let is_keyboard = km.device_type_of(qc.control) == Some(DeviceType::Keyboard);
    let stack = shell.manager.maps.entries().to_vec();
    dereth_input::fire::walk_input_maps(&stack, &live, is_keyboard, |m| km.section(m))
        .map(|r| (r.input_map, r.action))
}

/// The one control the shipped key map binds to `action` in `map`.
///
/// # Panics
/// Panics when the shipped map binds none or several, which would make every claim below about a
/// binding this scenario had chosen rather than one the client has.
fn the_only_binding(
    shell: &dereth_client::input::InputShell,
    map: dereth_input::InputMapId,
    action: u32,
) -> dereth_input::ControlChord {
    let section = shell
        .manager
        .keymap
        .section(map)
        .unwrap_or_else(|| panic!("the shipped map has a section {:#010X}", map.0));
    let mut keys = section.keys_for_action(dereth_input::ActionId(action));
    assert_eq!(
        keys.len(),
        1,
        "one shipped default binding for {action:#010X}"
    );
    keys.pop().expect("one")
}

// ---------------------------------------------------------------------------------------------
// keymap.modified-digits.*
//
// A modifier and a digit looks like "a key does the wrong thing" and is not a defect: the shipped
// map binds one key to two actions and the client settles it the way the original does. The
// scenario carries a counterfactual because without it "no chat window opened" would be evidence of
// an unbuilt panel rather than of the walk.
// ---------------------------------------------------------------------------------------------

/// The modifier and a number uses a quick slot, and not the floating chat window.
pub fn a_modified_number_uses_a_quick_slot_and_not_the_chat_window() {
    use dereth_input::{ActionId, InputMapId};

    const UI_COMMANDS: InputMapId = InputMapId(0x1000_0009);
    const QUICKSLOT_COMMANDS: InputMapId = InputMapId(0x1000_000C);
    /// The four floating chat windows, and the four quick slots the same four keys reach.
    const CHAT_WINDOWS: [u32; 4] = [0x1000_0114, 0x1000_0115, 0x1000_0116, 0x1000_0117];
    const QUICK_SLOTS: [u32; 4] = [0x1000_004B, 0x1000_004C, 0x1000_004D, 0x1000_0132];
    /// The unmodified number row, the negative control.
    const USE_QUICK_SLOT_1: u32 = 0x1000_0042;

    let store = dereth_dat::testing::open_store()
        .expect("the shipped key maps live in the retail data files");
    let shell = dereth_client::input::InputShell::new(&store, None).expect("the input tables");

    // 1. The shipped map really does bind one identical control to both actions, which is the
    //    input on which the tie rule applies at all.
    let mut identical = true;
    for (chat, slot) in CHAT_WINDOWS.into_iter().zip(QUICK_SLOTS) {
        let a = the_only_binding(&shell, UI_COMMANDS, chat);
        let b = the_only_binding(&shell, QUICKSLOT_COMMANDS, slot);
        identical &= a.is_exactly_equal(&b)
            && a.meta_mode != 0
            && a.activation == dereth_input::spec::activation::CLICK;
    }

    // 2. The two maps are registered at one priority, in the client's own order, and the later of
    //    the two is therefore walked first.
    let regs: Vec<(u32, i32)> = dereth_client::input::BASE_MAP_REGISTRATIONS
        .iter()
        .map(|(_, m, p)| (*m, *p))
        .collect();
    let ui_at = regs
        .iter()
        .position(|(m, _)| *m == UI_COMMANDS.0)
        .expect("registered");
    let qs_at = regs
        .iter()
        .position(|(m, _)| *m == QUICKSLOT_COMMANDS.0)
        .expect("registered");
    let entries = shell.manager.maps.entries();
    let walk_ui = entries
        .iter()
        .position(|e| e.map == UI_COMMANDS)
        .expect("in the stack");
    let walk_qs = entries
        .iter()
        .position(|e| e.map == QUICKSLOT_COMMANDS)
        .expect("in the stack");
    let registered_in_order = ui_at < qs_at && regs[ui_at].1 == regs[qs_at].1 && walk_qs < walk_ui;

    // 3. The observable, with the plain number as its negative control.
    let mut quick_slot_wins = true;
    for (chat, slot) in CHAT_WINDOWS.into_iter().zip(QUICK_SLOTS) {
        let qc = the_only_binding(&shell, UI_COMMANDS, chat);
        quick_slot_wins &= resolves_to(&shell, &qc) == Some((QUICKSLOT_COMMANDS, ActionId(slot)));
    }
    let plain = shell
        .manager
        .keymap
        .section(QUICKSLOT_COMMANDS)
        .expect("the section")
        .keys_for_action(ActionId(USE_QUICK_SLOT_1))
        .into_iter()
        .find(|q| q.meta_mode == 0)
        .expect("an unmodified binding");
    let plain_still_works =
        resolves_to(&shell, &plain) == Some((QUICKSLOT_COMMANDS, ActionId(USE_QUICK_SLOT_1)));

    // 4. The counterfactual: the same walk with the two swapped *does* reach the chat window, so
    //    the reading above is of the order and not of a stuck reader.
    let swapped_reaches_the_window = {
        let km = &shell.manager.keymap;
        let mut stack = shell.manager.maps.entries().to_vec();
        stack.swap(walk_qs, walk_ui);
        let qc = the_only_binding(&shell, UI_COMMANDS, CHAT_WINDOWS[0]);
        let live = dereth_input::ControlChord::new(
            qc.control,
            qc.meta_mode,
            (qc.activation & !dereth_input::spec::activation::UP)
                | dereth_input::spec::activation::LIVE,
        );
        dereth_input::fire::walk_input_maps(&stack, &live, true, |m| km.section(m))
            .map(|h| (h.input_map, h.action))
            == Some((UI_COMMANDS, ActionId(CHAT_WINDOWS[0])))
    };

    // 5. And the windows the modified numbers do not open are really there to be opened: every
    //    one of the four has a visibility-toggle listener in the shipped gameplay tree.
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let all_four_listen = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        CHAT_WINDOWS
            .into_iter()
            .all(|a| ui.dispatch_input_action(a))
            && !ui.dispatch_input_action(0xDEAD_BEEF)
    };

    c.assert_behaviour("keymap.modified-digits.a-number-with-the-modifier-uses-a-quick-slot-and-not-the-chat-window", move |_| {
        identical
            && registered_in_order
            && quick_slot_wins
            && plain_still_works
            && swapped_reaches_the_window
            && all_four_listen
    });
    c.shutdown();
}

#[test]
fn scenario_a_modified_number_uses_a_quick_slot_and_not_the_chat_window() {
    scenario("a_modified_number_uses_a_quick_slot_and_not_the_chat_window");
}

// ---------------------------------------------------------------------------------------------
// movement.walk-mode-key.*
//
// Two related checks are transcriptions and carry no row: one scans the exclusive-or and its callee
// out of the shipped executable, and the other pins the two statements of the shipped default
// against each other. The premise each of them protects -- that this character has the shipped
// default and it is on -- is asserted inside the first scenario below, where it is load-bearing.
// ---------------------------------------------------------------------------------------------

/// Whether the body is running this frame.
fn running(c: &HeadlessClient) -> bool {
    c.view().expect_app().char_input().run
}

/// Hold the walk-mode key down and answer what the body did, then let go and answer again.
fn walk_mode_edges(c: &mut HeadlessClient, hands: &mut Hands) -> (bool, bool) {
    // The walk-mode key is also a modifier key; a modifier still fires its own control, which is
    // why this reaches an action at all.
    hands.press(c, key(KeyCode::ShiftLeft));
    c.tick(1);
    let held = running(c);
    hands.release(c, key(KeyCode::ShiftLeft));
    c.tick(1);
    (held, running(c))
}

/// The walk-mode key follows the run-by-default option, both ways.
pub fn the_walk_mode_key_follows_the_run_by_default_option() {
    use dereth_client_model::player::options::option::TOGGLE_RUN;

    let mut c = HeadlessClient::new(ClientSpec {
        static_scene: true,
        ..ClientSpec::gameplay(8)
    });
    let mut hands = Hands::new();

    // The premise, asserted rather than assumed: this character has the shipped defaults and
    // run-by-default is on.
    let default_is_on = c
        .view()
        .objects()
        .world
        .player_system
        .options
        .get(TOGGLE_RUN);

    let routed_before = c.view().expect_app().actions_routed();
    let (held_on, released_on) = walk_mode_edges(&mut c, &mut hands);
    // The denominator: "the flag did not change" and "the key never arrived" are the same reading
    // without it, and a value that silently stayed at its default is this claim's whole subject.
    let key_arrived = c.view().expect_app().actions_routed() > routed_before;

    // The other arm, on the same client: with the option off the same key does the opposite.
    assert!(
        c.objects_mut()
            .world
            .player_system
            .options
            .set(TOGGLE_RUN, false),
        "the option was on and this turned it off"
    );
    let (held_off, released_off) = walk_mode_edges(&mut c, &mut hands);

    c.assert_behaviour(
        "movement.walk-mode-key.holding-it-follows-the-run-by-default-option-both-ways",
        move |_| {
            default_is_on && key_arrived && !held_on && released_on && held_off && !released_off
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_walk_mode_key_follows_the_run_by_default_option() {
    scenario("the_walk_mode_key_follows_the_run_by_default_option");
}

/// The option is read on every press rather than latched when the client started.
pub fn the_walk_mode_option_is_read_on_every_press() {
    use dereth_client_model::player::options::option::TOGGLE_RUN;

    let mut c = HeadlessClient::new(ClientSpec {
        static_scene: true,
        ..ClientSpec::gameplay(8)
    });
    let mut hands = Hands::new();

    let (held_before, _) = walk_mode_edges(&mut c, &mut hands);
    c.objects_mut()
        .world
        .player_system
        .options
        .set(TOGGLE_RUN, false);
    let (held_after, _) = walk_mode_edges(&mut c, &mut hands);

    c.assert_behaviour(
        "movement.walk-mode-key.the-option-is-read-on-every-press-and-not-at-start-up",
        move |_| {
            // The same key edge, before and after the option moved, with no relog in between.
            !held_before && held_after
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_walk_mode_option_is_read_on_every_press() {
    scenario("the_walk_mode_option_is_read_on_every_press");
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.*
//
// Each of these owns a scratch preferences directory of its own **and clears it first**: the client
// merges the player's saved key map over the shipped default on the way in and writes one out on
// the way out, so a second run of the rebind scenario would otherwise load the binding the first
// run made and fail on its own precondition. That state leak reads exactly like a flaky test.
// ---------------------------------------------------------------------------------------------

/// The first key button of the walk-forward row of the key-bindings page.
fn walk_forward_key_button(app: &mut dereth_client::app::App) -> dereth_ui::ElemHandle {
    let shell = app.ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    let gameplay = any
        .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("the gameplay screen is current");
    let i = gameplay
        .key_bindings
        .row_of(MOVEMENT, MOVE_FORWARD)
        .expect("walking forward has a row in a running client");
    gameplay.key_bindings.rows[i].key_buttons[0]
}

/// Click an element through the shell's own broadcast, which is the route a button takes.
fn click_element(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    c.tick(1);
}

/// What the running client has bound to walking forward.
fn walk_forward_keys(c: &mut HeadlessClient) -> Vec<dereth_input::ControlChord> {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .keys_for_action(MOVE_FORWARD, MOVEMENT)
}

/// A client for the key-bindings page.
///
/// **Nothing it does persists between runs**, which matters here: the client merges the player's
/// saved key map over the shipped default on the way in and writes one out on the way out, so a
/// rebind that survived would make the next run of the scenario below fail on its own precondition
/// -- and would read exactly like a flaky test. The harness points every client's preferences at a
/// directory under the temporary one that is deliberately never created, so the load finds nothing
/// and the save has nowhere to go. That closes the state leak by construction rather than by
/// clearing a folder.
fn a_client_for_the_key_bindings_page() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay_in_world(6))
}

/// The page builds one row for every bindable action, once.
pub fn the_key_binding_page_builds_one_row_per_bindable_action() {
    let mut c = a_client_for_the_key_bindings_page();

    let st = c.view().expect_app().key_binding_stats();
    let built_once = st.init_calls == 1
        && st.bindable_actions > 0
        && st.rows_built == st.bindable_actions
        && st.failures == 0
        && st.headers > 0;

    // It is a per-page call and not a per-frame one: rebuilding flushes every list, so a per-frame
    // call would throw a row away in the middle of a capture.
    c.tick(10);
    let not_again = c.view().expect_app().key_binding_stats().init_calls == 1;

    // And the rows are really in the tree rather than only counted: the walk-forward row is there
    // and shows the key the merged map reports.
    let labels = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let screen = shell.flow.current_mut().expect("a screen is current");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen");
        let i = gameplay
            .key_bindings
            .row_of(MOVEMENT, MOVE_FORWARD)
            .expect("the row");
        gameplay.key_bindings.rows[i].button_labels.clone()
    };

    c.assert_behaviour(
        "options.key-bindings.the-page-builds-one-row-for-every-bindable-action-once",
        move |_| built_once && not_again && labels.first().is_some_and(|l| !l.is_empty()),
    );
    c.shutdown();
}

#[test]
fn scenario_the_key_binding_page_builds_one_row_per_bindable_action() {
    scenario("the_key_binding_page_builds_one_row_per_bindable_action");
}

/// A key pressed over a row rebinds it, and the key it was taken from is left bound to nothing.
pub fn a_key_pressed_over_a_row_rebinds_it() {
    use dereth_input::binding::DO_NOTHING;

    let mut c = a_client_for_the_key_bindings_page();
    let mut hands = Hands::new();

    let before = walk_forward_keys(&mut c);
    let started_on_the_shipped_key = before
        .contains(&control(SCAN_W, dereth_input::spec::activation::CLICK))
        && !before.contains(&control(SCAN_FREE, dereth_input::spec::activation::CLICK));
    let nothing_capturing = !c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .key_hit_handler_registered();

    let button = walk_forward_key_button(c.app_mut());
    click_element(&mut c, button);
    let one_message = c.view().expect_app().key_binding_stats().row_events == 1;
    let capturing = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .key_hit_handler_registered();

    // The key, through the pump. The press is not an answer; the release is.
    hands.press(&mut c, key(KeyCode::F7));
    hands.release(&mut c, key(KeyCode::F7));
    c.tick(1);

    let st = c.view().expect_app().key_binding_stats();
    let both_diverted = st.key_hits_offered == 2 && st.key_hits_taken == 2 && st.bindings_made == 1;
    let handler_gone = !c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .key_hit_handler_registered();

    let after = walk_forward_keys(&mut c);
    let moved = after.contains(&control(SCAN_FREE, dereth_input::spec::activation::CLICK))
        && !after.contains(&control(SCAN_W, dereth_input::spec::activation::CLICK));
    // The freed key is bound to nothing at all rather than removed: a removed binding gets its
    // shipped default back on the next merge, which would leave both keys firing.
    let freed_not_deleted = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .keymap
        .section(MOVEMENT)
        .expect("the movement section")
        .bindings()
        .iter()
        .find(|(qc, _)| {
            qc.is_exactly_equal(&control(SCAN_W, dereth_input::spec::activation::CLICK))
        })
        .map(|(_, a)| *a)
        == Some(DO_NOTHING);

    c.assert_behaviour(
        "options.key-bindings.a-key-pressed-over-a-row-rebinds-it-and-frees-the-old-key",
        move |_| {
            started_on_the_shipped_key
                && nothing_capturing
                && one_message
                && capturing
                && both_diverted
                && handler_gone
                && moved
                && freed_not_deleted
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_key_pressed_over_a_row_rebinds_it() {
    scenario("a_key_pressed_over_a_row_rebinds_it");
}

/// A capture in flight swallows the key, and the key comes back once it is over.
pub fn a_capture_in_flight_swallows_the_key() {
    let mut c = a_client_for_the_key_bindings_page();
    let mut hands = Hands::new();

    // Direction one: with nothing capturing, the key walks.
    let walks_before = held_moves(&mut c, &mut hands, KeyCode::KeyW, forward);
    let nothing_diverted = c.view().expect_app().key_binding_stats().key_hits_offered == 0;

    // Direction two: with a capture in flight, the same key on the same client does not.
    let button = walk_forward_key_button(c.app_mut());
    click_element(&mut c, button);
    let walks_during = held_moves(&mut c, &mut hands, KeyCode::KeyW, forward);
    let diverted_instead = c.view().expect_app().key_binding_stats().key_hits_offered > 0;

    // ...and afterwards it is back. Binding the key to the action it already had changes nothing,
    // so the map is where it started.
    let capture_over = !c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .key_hit_handler_registered();
    let still_bound =
        walk_forward_keys(&mut c).contains(&control(SCAN_W, dereth_input::spec::activation::CLICK));
    let walks_after = held_moves(&mut c, &mut hands, KeyCode::KeyW, forward);

    c.assert_behaviour(
        "options.key-bindings.a-capture-in-flight-swallows-the-key-and-gives-it-back-afterwards",
        move |_| {
            walks_before
                && nothing_diverted
                && !walks_during
                && diverted_instead
                && capture_over
                && still_bound
                && walks_after
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_capture_in_flight_swallows_the_key() {
    scenario("a_capture_in_flight_swallows_the_key");
}

// ---------------------------------------------------------------------------------------------
// selection.fellow.*
//
// The fellowship is built by the client's own receiver for the shard's fellowship message rather
// than by hand, so a receiver that stopped filling the table would be visible here.
// ---------------------------------------------------------------------------------------------

/// The action ids of the two keys, and the four characters the scenarios use.
const ME: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_0001);
const BOB: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_0002);
const CAI: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_0003);
/// Deliberately not in the fellowship.
const STRANGER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x7000_0009);

/// An action bench with `members` in a fellowship and a stranger outside it.
fn a_fellowship(members: &[dereth_primitives::ObjectId]) -> ActionBench {
    let mut b = ActionBench::new();
    b.objects.world.player = Some(ME);
    for id in members.iter().copied().chain([STRANGER]) {
        let mut w = dereth_client_model::Weenie::new(id);
        w.valid = true;
        b.objects.world.tables.weenies.insert(id, w);
    }
    if !members.is_empty() {
        let wire = dereth_protocol::social::Fellowship {
            members: dereth_protocol::archive::PackedHash {
                table_size: 8,
                entries: members
                    .iter()
                    .map(|id| {
                        (
                            id.0,
                            dereth_protocol::social::Fellow {
                                name: format!("Fellow {:X}", id.0 & 0xFF),
                                level: 10,
                                ..dereth_protocol::social::Fellow::default()
                            },
                        )
                    })
                    .collect(),
            },
            name: "Fellows".to_owned(),
            leader: members[0],
            share_xp: 1,
            even_xp_split: 1,
            open_fellow: 0,
            locked: 0,
            fellows_departed: dereth_protocol::archive::PackedHash::default(),
            locks: dereth_protocol::social::FellowshipLocks::default(),
        };
        b.objects.world.recv_fellowship_full_update(&wire);
        assert_eq!(
            b.objects
                .world
                .fellowship
                .as_ref()
                .expect("a fellowship")
                .members
                .len(),
            members.len(),
            "the client's own receiver must have filled the table"
        );
    }
    b
}

/// One key walks the fellowship forward and the other back, both wrapping.
pub fn the_two_keys_walk_the_fellowship_both_ways_and_wrap() {
    use dereth_client::interaction::action as ia;

    // The shipped keys really reach the two actions, rather than the scenario naming them.
    let store = dereth_dat::testing::open_store().expect("the retail data files");
    let shell = dereth_client::input::InputShell::new(&store, None).expect("the input tables");
    let mut both_bound = true;
    for action in [ia::SELECTION_NEXT_FELLOW, ia::SELECTION_PREVIOUS_FELLOW] {
        let qc = the_only_binding(&shell, ITEM_SELECTION, action);
        both_bound &= qc.meta_mode == 0
            && resolves_to(&shell, &qc) == Some((ITEM_SELECTION, dereth_input::ActionId(action)));
    }

    // Forward, three times round a three-member fellowship, back where it started -- and the
    // player's own character is one of the three it lands on.
    let mut b = a_fellowship(&[ME, BOB, CAI]);
    b.select(Some(ME));
    let mut forward_walk = Vec::new();
    for _ in 0..3 {
        b.hit(ITEM_SELECTION, ia::SELECTION_NEXT_FELLOW);
        forward_walk.push(b.selected());
    }
    let forward_cycles = b.inter.stats.selection_fellow_cycles;

    // Backward, the other way round.
    let mut b = a_fellowship(&[ME, BOB, CAI]);
    b.select(Some(CAI));
    let mut backward_walk = Vec::new();
    for _ in 0..3 {
        b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_FELLOW);
        backward_walk.push(b.selected());
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.fellow.one-key-walks-the-fellowship-forward-and-the-other-back-both-wrapping",
        move |_| {
            both_bound
                && forward_walk == vec![Some(BOB), Some(CAI), Some(ME)]
                && backward_walk == vec![Some(BOB), Some(ME), Some(CAI)]
                && forward_cycles == 3
        },
    );
}

#[test]
fn scenario_the_two_keys_walk_the_fellowship_both_ways_and_wrap() {
    scenario("the_two_keys_walk_the_fellowship_both_ways_and_wrap");
}

/// An outsider starts the walk at one end, and with no fellowship nothing moves.
pub fn an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing() {
    use dereth_client::interaction::action as ia;

    let mut ends = Vec::new();
    for (action, want) in [
        (ia::SELECTION_NEXT_FELLOW, ME),
        (ia::SELECTION_PREVIOUS_FELLOW, CAI),
    ] {
        // A selection outside the fellowship...
        let mut b = a_fellowship(&[ME, BOB, CAI]);
        b.select(Some(STRANGER));
        b.hit(ITEM_SELECTION, action);
        let from_outsider = b.selected();
        // ...and no selection at all, which is the same leg.
        let mut b = a_fellowship(&[ME, BOB, CAI]);
        b.select(None);
        b.hit(ITEM_SELECTION, action);
        ends.push((from_outsider == Some(want), b.selected() == Some(want)));
    }

    // The negative control: with no fellowship at all neither key touches the selection. A wire
    // that selected "the first fellow" out of an absent table would pass everything above.
    let mut untouched = true;
    for action in [ia::SELECTION_NEXT_FELLOW, ia::SELECTION_PREVIOUS_FELLOW] {
        let mut b = a_fellowship(&[]);
        assert!(
            b.objects.world.fellowship.is_none(),
            "the premise: no fellowship"
        );
        b.select(Some(STRANGER));
        b.hit(ITEM_SELECTION, action);
        untouched &= b.selected() == Some(STRANGER) && b.inter.stats.selection_fellow_cycles == 0;
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour("selection.fellow.a-selection-outside-the-fellowship-starts-at-one-end-and-with-no-fellowship-nothing-moves", move |_| {
        ends.iter().all(|(a, b)| *a && *b) && untouched
    });
}

#[test]
fn scenario_an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing() {
    scenario("an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing");
}

/// The cycle follows the order the fellowship panel shows.
pub fn the_fellow_cycle_follows_the_panels_order() {
    use dereth_client::interaction::action as ia;

    // The members arrive in an order that is not the panel's, so a reader that walked the message
    // rather than the model would answer the other way round.
    let mut b = a_fellowship(&[CAI, BOB, ME]);
    let panel_order: Vec<dereth_primitives::ObjectId> = b
        .objects
        .world
        .fellowship
        .as_ref()
        .expect("a fellowship")
        .members
        .keys()
        .copied()
        .collect();
    b.select(Some(panel_order[0]));
    let mut walked = vec![panel_order[0]];
    for _ in 0..2 {
        b.hit(ITEM_SELECTION, ia::SELECTION_NEXT_FELLOW);
        walked.push(b.selected().expect("a selection"));
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.fellow.the-cycle-follows-the-order-the-panel-shows",
        move |_| walked == panel_order && panel_order.len() == 3,
    );
}

#[test]
fn scenario_the_fellow_cycle_follows_the_panels_order() {
    scenario("the_fellow_cycle_follows_the_panels_order");
}

// ---------------------------------------------------------------------------------------------
// selection.previous.*
// ---------------------------------------------------------------------------------------------

const TARGET_A: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x7000_0011);
const TARGET_B: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x7000_0012);
const TARGET_C: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x7000_0013);

/// An action bench with three things in the world to select.
fn three_targets() -> ActionBench {
    let mut b = ActionBench::new();
    b.objects.world.player = Some(ME);
    for id in [ME, TARGET_A, TARGET_B, TARGET_C] {
        let mut w = dereth_client_model::Weenie::new(id);
        w.valid = true;
        b.objects.world.tables.weenies.insert(id, w);
    }
    b
}

/// The key that goes back selects the one before, and is a toggle rather than a stack.
pub fn the_key_that_goes_back_is_a_toggle_and_not_a_stack() {
    use dereth_client::interaction::action as ia;

    // The shipped key really reaches the action.
    let store = dereth_dat::testing::open_store().expect("the retail data files");
    let shell = dereth_client::input::InputShell::new(&store, None).expect("the input tables");
    let qc = the_only_binding(&shell, ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION);
    let bound = qc.meta_mode == 0
        && resolves_to(&shell, &qc)
            == Some((
                ITEM_SELECTION,
                dereth_input::ActionId(ia::SELECTION_PREVIOUS_SELECTION),
            ));

    // Two selections and one press: the first comes back.
    let mut b = three_targets();
    b.select(Some(TARGET_A));
    b.select(Some(TARGET_B));
    let recorded = b.objects.world.prev_selected == Some(TARGET_A);
    b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION);
    let went_back =
        b.selected() == Some(TARGET_A) && b.inter.stats.selection_previous_restores == 1;

    // Three selections and three presses: the walk is a toggle between the last two, and the one
    // before them is never reachable.
    let mut b = three_targets();
    for id in [TARGET_A, TARGET_B, TARGET_C] {
        b.select(Some(id));
    }
    let mut walk = Vec::new();
    for _ in 0..3 {
        b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION);
        walk.push(b.selected());
    }
    let restores = b.inter.stats.selection_previous_restores;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.previous.the-key-goes-back-one-and-is-a-toggle-rather-than-a-stack",
        move |_| {
            bound
                && recorded
                && went_back
                && walk == vec![Some(TARGET_B), Some(TARGET_C), Some(TARGET_B)]
                && !walk.contains(&Some(TARGET_A))
                && restores == 3
        },
    );
}

#[test]
fn scenario_the_key_that_goes_back_is_a_toggle_and_not_a_stack() {
    scenario("the_key_that_goes_back_is_a_toggle_and_not_a_stack");
}

/// With nothing behind it, the key selects nothing.
pub fn with_nothing_behind_it_the_key_selects_nothing() {
    use dereth_client::interaction::action as ia;

    let mut b = three_targets();
    let nothing_yet = b.objects.world.prev_selected.is_none();
    b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION);
    let selected_nothing = b.selected().is_none() && b.inter.stats.selection_previous_restores == 0;

    // The same leg one step later: the very first selection of a session has nothing behind it
    // either, so the key is still inert. Without this a client that restored "the last thing that
    // was really there" would pass everything else.
    b.select(Some(TARGET_A));
    let still_nothing_behind = b.objects.world.prev_selected.is_none();
    b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION);
    let unchanged =
        b.selected() == Some(TARGET_A) && b.inter.stats.selection_previous_restores == 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.previous.with-nothing-behind-it-the-key-selects-nothing",
        move |_| nothing_yet && selected_nothing && still_nothing_behind && unchanged,
    );
}

#[test]
fn scenario_with_nothing_behind_it_the_key_selects_nothing() {
    scenario("with_nothing_behind_it_the_key_selects_nothing");
}

/// A target that was cleared comes back.
pub fn a_cleared_target_comes_back() {
    use dereth_client::interaction::action as ia;

    let mut b = three_targets();
    b.select(Some(TARGET_A));
    b.select(Some(TARGET_B));
    b.select(None);
    let cleared = b.selected().is_none() && b.objects.world.prev_selected == Some(TARGET_B);

    b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION);
    let came_back = b.selected() == Some(TARGET_B) && b.objects.world.prev_selected.is_none();

    // ...and going back again is the empty step, so it stays where it is.
    b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION);
    let stayed = b.selected() == Some(TARGET_B) && b.inter.stats.selection_previous_restores == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.previous.a-target-that-was-cleared-comes-back",
        move |_| cleared && came_back && stayed,
    );
}

#[test]
fn scenario_a_cleared_target_comes_back() {
    scenario("a_cleared_target_comes_back");
}

// ---------------------------------------------------------------------------------------------
// options.character-page.*
//
// The option ordinals and their bit masks, written out as literals beside the symbols that carry
// them, are a transcription and carry no row. That every row of the page edits the setting of the
// same name is the premise of `each-row-shows-the-bit-the-shard-sent-for-it` and is asserted inside
// it.
//
// **The counts here are read from the corpus index and never pinned.** Rather than a per-recording
// table of how many settings records each one sent and which header word each carried,
// `Outbound::count` answers the first from the index, and the second is a property of the bytes
// rather than of the corpus, so what is asserted is the fixed point over every recorded record
// rather than a table that a promoted recording moves.
// ---------------------------------------------------------------------------------------------

/// The ordered game action the settings record travels in, and the record's own message.
const CHARACTER_OPTIONS_EVENT: u32 = 0x01A1;

/// One recorded settings record: which recording sent it, the stamp of its envelope, and its body.
struct RecordedOptions {
    session: String,
    stamp: u32,
    body: Vec<u8>,
}

/// Every settings record the recorded clients sent, read out of the committed recordings.
///
/// **Why the payload is read here rather than through `Outbound::recorded`.** That reader answers
/// *which* message each client-to-server blob is and when it went, which is what a claim about
/// traffic needs; it does not carry the bytes, and the claim below is that the bytes come back
/// unchanged. The count it does answer is used as the cross-check, so the two readers have to agree
/// about how many there are. The missing bytes are a gap in the harness.
///
/// # Panics
/// Panics when a recording named by the index does not parse, which is a broken checkout.
fn recorded_option_records() -> Vec<RecordedOptions> {
    use dereth_client_net::client_session::testing::{Corpus, Direction};

    let mut out: Vec<RecordedOptions> = Vec::new();
    for name in dereth_client_net::client_session::testing::session_names() {
        let corpus = Corpus::load(name)
            .unwrap_or_else(|e| panic!("the recording {name} does not parse: {e}"))
            .unwrap_or_else(|| panic!("the decoded corpus has no recording {name}"));
        for b in &corpus.blobs {
            if b.dir != Direction::ClientToServer
                || dereth_testkit::outbound::message_of(&b.payload) != Some(CHARACTER_OPTIONS_EVENT)
            {
                continue;
            }
            out.push(RecordedOptions {
                session: (*name).to_owned(),
                stamp: u32::from_le_bytes(b.payload[4..8].try_into().expect("a stamp")),
                body: b.payload[12..].to_vec(),
            });
        }
    }
    assert!(
        !out.is_empty(),
        "no recording carries a settings record; a scan that read nothing would pass over nothing"
    );
    // The cross-check, against the index rather than a table written here: for every recording
    // that sent one, the crate's own client-to-server reader has to agree about how many.
    //
    // It is asked only about those recordings because `Outbound::all` refuses a recording with no
    // client-to-server blob at all, and the corpus has one. That is a gap in the harness.
    let mut per_session: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for r in &out {
        *per_session.entry(r.session.as_str()).or_default() += 1;
    }
    for (name, n) in per_session {
        assert_eq!(
            dereth_testkit::Outbound::count(name, CHARACTER_OPTIONS_EVENT),
            n,
            "{name}: the two readers disagree about how many settings records it carries"
        );
    }
    out
}

/// The one settings record a named recording sent.
fn the_record_of(session: &str) -> RecordedOptions {
    let mut it = recorded_option_records()
        .into_iter()
        .filter(|b| b.session == session);
    let first = it
        .next()
        .unwrap_or_else(|| panic!("{session} sent no settings record"));
    assert!(it.next().is_none(), "{session} sent more than one");
    first
}

/// Decode a recorded settings body into the record the client packed.
fn decode_module(body: &[u8]) -> dereth_protocol::login::PlayerModule {
    use dereth_protocol::Reader;
    let mut r = Reader::with_origin(body, 12);
    let m = dereth_protocol::login::PlayerModule::read(&mut r)
        .expect("a recorded settings record decodes");
    r.expect_exhausted()
        .expect("the cursor lands on the end of the record");
    m
}

/// Re-encode one the way the client's own sender does, so the alignment origin is the blob's.
fn encode_module(m: &dereth_protocol::login::PlayerModule) -> Vec<u8> {
    let mut w = dereth_protocol::actions::action_body_writer();
    m.write(&mut w).expect("it encodes");
    w.into_inner()
}

/// A world whose settings are a recorded record, the way the shard's description gives it one.
fn world_with(module: &dereth_protocol::login::PlayerModule) -> dereth_client_model::World {
    let mut w = dereth_client_model::World::new();
    w.player_system.apply_player_module(module);
    w
}

/// Every row of the page shows the setting the shard sent for it.
pub fn every_row_shows_the_setting_the_shard_sent() {
    use dereth_client::hud::{character_option, option_ordinal, Hud};
    use dereth_client_contract::options::sheet::{rows_for, Face, PageId, Value};
    use dereth_client_model::player::options::PLAYER_OPTIONS;
    use dereth_ui_screens::options::character::option_name;
    use dereth_ui_screens::view::{GameView, PlayerOption};

    let record = the_record_of("first-login-walk-jump");
    let module = decode_module(&record.body);

    // The premise: every row edits the setting of the same name, and no two rows edit the same one.
    // Two tables meet here and neither is derived from the other.
    let mut seen = std::collections::BTreeSet::new();
    let mut names_agree = true;
    for row in rows_for(PageId::Character, Face::Retail) {
        if let Value::Option(o) = row.value {
            let n = option_ordinal(o);
            names_agree &= PLAYER_OPTIONS[n].0 == option_name(o) && seen.insert(n);
        }
    }

    let mut objects = dereth_client::objects::ObjectStream::default();
    objects.world.player_system.apply_player_module(&module);
    let hud = Hud::default();
    let view = hud.view(&objects);

    let mut asked = 0usize;
    let mut ticked = 0usize;
    let mut each_row_agrees = true;
    for row in rows_for(PageId::Character, Face::Retail) {
        if let Value::Option(o) = row.value {
            asked += 1;
            let n = option_ordinal(o);
            let (_, word, mask) = PLAYER_OPTIONS[n];
            let expect = match word {
                dereth_client_model::player::OptionWord::One => module.options & mask != 0,
                dereth_client_model::player::OptionWord::Two => module.options2 & mask != 0,
            };
            each_row_agrees &= view.player_option(o) == expect;
            ticked += usize::from(expect);
        }
    }

    // The third answer, which a yes-or-no cannot carry: a client that has been told nothing says
    // it does not know. Without it "off" and "never asked" are the same reading, and the page
    // that opened blank read exactly like a page of things turned off.
    let empty = dereth_client::objects::ObjectStream::default();
    let unknown = character_option(&empty.world, PlayerOption::AutoTarget).is_none();
    let known = character_option(&objects.world, PlayerOption::AutoTarget).is_some();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.each-row-shows-the-bit-the-shard-sent-for-it",
        move |_| {
            names_agree
            && seen.len() == asked
            && asked > 40
            && each_row_agrees
            // Some of this character's rows are on and some are off, or the comparison above
            // would hold on a reader that answered one thing for everything.
            && ticked > 0
            && ticked < asked
            && unknown
            && known
        },
    );
}

#[test]
fn scenario_every_row_shows_the_setting_the_shard_sent() {
    scenario("every_row_shows_the_setting_the_shard_sent");
}

/// A tick changes one setting of what the shard sent and nothing else.
pub fn a_tick_changes_only_that_setting() {
    use dereth_client::interaction::Interaction;
    use dereth_primitives::ServerTime;
    use dereth_ui_screens::view::{PlayerOption, UiRequest};

    let record = the_record_of("first-login-walk-jump");
    let module = decode_module(&record.body);
    let before = (module.options, module.options2);
    // A setting this client does not model at all, carried in what the shard sent: it must come
    // through every write below untouched, and that is the regression this claim exists for.
    let unmodelled = module.options2 & 0x0200_0000;
    let mut world = world_with(&module);

    // A setting the client holds back until the record is saved.
    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(
            PlayerOption::DisplayTimeStamps,
            true,
        )],
    );
    let arm_exists = inter
        .run_ui_requests(&mut world, false, ServerTime(0.0))
        .is_empty();
    let held_back = inter.pending_requests().is_empty()
        && inter.stats.option_changes_deferred == 1
        && inter.stats.option_changes_unsendable == 0;
    let after = {
        let m = world
            .player_system
            .module
            .as_ref()
            .expect("the record is kept");
        (m.options, m.options2)
    };
    let dirty = world.player_system.is_dirty();

    // Setting it again is not a change at all.
    let mut again = Interaction::default();
    again.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(
            PlayerOption::DisplayTimeStamps,
            true,
        )],
    );
    let no_second_change = again
        .run_ui_requests(&mut world, false, ServerTime(1.0))
        .is_empty()
        && again.stats.option_changes_deferred == 0;

    // And a setting the client sends the moment it moves goes out at once, on its own.
    let mut at_once = Interaction::default();
    at_once.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(
            PlayerOption::HearGeneralChat,
            false,
        )],
    );
    let sent_at_once = at_once
        .run_ui_requests(&mut world, false, ServerTime(2.0))
        .is_empty()
        && at_once.stats.option_changes_unsendable == 0
        && at_once.stats.option_changes_sent == 1;
    let one_option_per_message = at_once
        .take_pending_requests()
        .iter()
        .filter(|r| matches!(r, dereth_client_model::Request::PlayerOptionChanged(_)))
        .count()
        == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.a-tick-changes-only-that-bit-of-what-the-shard-sent",
        move |_| {
            arm_exists
                && held_back
                && after.0 == before.0
                && after.1 == before.1 | 0x0000_0040
                && after.1 & 0x0200_0000 == unmodelled
                && dirty
                && no_second_change
                && sent_at_once
                && one_option_per_message
        },
    );
}

#[test]
fn scenario_a_tick_changes_only_that_setting() {
    scenario("a_tick_changes_only_that_setting");
}

/// Every recorded settings record, taken in and sent back out, is the same bytes.
pub fn the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones() {
    use dereth_client_model::Request;

    let records = recorded_option_records();
    let mut all_identical = true;
    let mut all_framed = true;
    let mut cleared = true;
    for record in &records {
        let module = decode_module(&record.body);
        let mut world = world_with(&module);
        let packed = world
            .player_system
            .client_packed_module()
            .expect("the record is kept");
        all_identical &= encode_module(&packed) == record.body;

        // ...and through the sender, framed: the whole blob including its envelope.
        let mut req = dereth_client_model::RecordingRequests::default();
        assert!(
            world.player_system.save_to_server(&mut req, true),
            "a forced save always sends"
        );
        let [Request::CharacterOptionsEvent(m)] = req.0.as_slice() else {
            panic!(
                "{}: expected one settings record, got {:?}",
                record.session, req.0
            )
        };
        let framed = dereth_protocol::actions::pack_action(record.stamp, m).expect("it frames");
        all_framed &= framed.len() > 12 && framed[12..] == record.body[..];
        cleared &= !world.player_system.is_dirty();
    }
    let how_many = records.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour("options.character-page.the-settings-it-sends-back-are-byte-for-byte-the-ones-a-real-client-sent", move |_| {
        how_many > 0 && all_identical && all_framed && cleared
    });
}

#[test]
fn scenario_the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones() {
    scenario("the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones");
}

/// One tick and one visit is one message with one setting moved.
pub fn one_tick_and_one_visit_sends_one_message() {
    use dereth_client::interaction::Interaction;
    use dereth_client_model::Request;
    use dereth_primitives::ServerTime;
    use dereth_ui_screens::view::{PlayerOption, UiRequest};

    let record = the_record_of("first-login-walk-jump");
    let original = decode_module(&record.body);
    let mut world = world_with(&original);

    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![
            UiRequest::SetPlayerOption(PlayerOption::DisplayTimeStamps, true),
            UiRequest::SavePlayerOptions,
        ],
    );
    let ran = inter
        .run_ui_requests(&mut world, false, ServerTime(0.0))
        .is_empty();
    let exactly_one = inter.stats.player_modules_sent == 1;

    let [Request::CharacterOptionsEvent(sent)] = inter.pending_requests() else {
        panic!(
            "expected one settings record, got {:?}",
            inter.pending_requests()
        )
    };
    let bytes = encode_module(&sent.module);
    // **Exactly one byte of the whole record differs**, and the narrowness is the assertion: the
    // spell bars, the window sizes and every other setting came through because the record was
    // changed in place rather than rebuilt.
    let differing: Vec<usize> = (0..bytes.len().min(record.body.len()))
        .filter(|i| bytes[*i] != record.body[*i])
        .collect();
    let same_length = bytes.len() == record.body.len();
    let carried_through = sent.module.options == original.options
        && sent.module.options2 == original.options2 | 0x0000_0040
        && sent.module.gameplay_options == original.gameplay_options
        && sent.module.spell_bars == original.spell_bars;

    // A second visit with nothing changed sends nothing.
    let mut nothing = Interaction::default();
    nothing.queue(Vec::new(), vec![UiRequest::SavePlayerOptions]);
    let quiet = nothing
        .run_ui_requests(&mut world, false, ServerTime(1.0))
        .is_empty()
        && nothing.stats.player_modules_sent == 0
        && nothing.pending_requests().is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.one-tick-and-one-visit-sends-one-message-with-one-bit-moved",
        move |_| {
            ran && exactly_one && same_length && differing.len() == 1 && carried_through && quiet
        },
    );
}

#[test]
fn scenario_one_tick_and_one_visit_sends_one_message() {
    scenario("one_tick_and_one_visit_sends_one_message");
}

/// A deferred change reaches the shard on the frame's own timer, once.
pub fn a_deferred_change_is_flushed_by_the_frame() {
    use dereth_client::interaction::Interaction;
    use dereth_client_model::Request;
    use dereth_primitives::{LocalTime, ServerTime};
    use dereth_ui_screens::view::{PlayerOption, UiRequest};

    let store = dereth_dat::testing::open_store().expect("the retail data files");
    let record = the_record_of("first-login-walk-jump");
    let mut objects = dereth_client::objects::ObjectStream::default();
    objects
        .world
        .player_system
        .apply_player_module(&decode_module(&record.body));
    let mut inter = Interaction::default();

    inter.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(
            PlayerOption::DisplayTimeStamps,
            true,
        )],
    );
    assert!(inter
        .run_ui_requests(&mut objects.world, false, ServerTime(0.0))
        .is_empty());
    let deferred = objects.world.player_system.is_dirty() && inter.stats.player_modules_sent == 0;
    let _ = inter.take_pending_requests();

    // **Driven from the frame's own entry point and not from the method.** A scenario that called
    // the timer directly would survive deleting its call site in the frame: it structurally could
    // not see the change.
    let mut frame =
        |inter: &mut Interaction, objects: &mut dereth_client::objects::ObjectStream, t: f64| {
            dereth_client::interaction::use_time(
                inter,
                &store,
                None,
                objects,
                None,
                Vec::new(),
                false,
                (800, 600),
                LocalTime(t),
            );
        };

    frame(&mut inter, &mut objects, 479.0);
    let not_yet = inter.stats.player_modules_sent == 0;
    frame(&mut inter, &mut objects, 481.0);
    let flushed = inter.stats.player_modules_sent == 1;
    let one_message = matches!(
        inter.last_sent.as_slice(),
        [Request::CharacterOptionsEvent(_)]
    );
    frame(&mut inter, &mut objects, 9_999.0);
    let only_once = inter.stats.player_modules_sent == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.a-deferred-change-reaches-the-shard-eight-minutes-later-and-once",
        move |_| deferred && not_yet && flushed && one_message && only_once,
    );
}

#[test]
fn scenario_a_deferred_change_is_flushed_by_the_frame() {
    scenario("a_deferred_change_is_flushed_by_the_frame");
}

/// With no description nothing is sent and nothing is invented.
pub fn with_no_description_nothing_is_sent() {
    use dereth_client::interaction::Interaction;
    use dereth_primitives::ServerTime;
    use dereth_ui_screens::view::{PlayerOption, UiRequest};

    // Nothing to send, and nothing composed out of the defaults.
    let mut world = dereth_client_model::World::new();
    let nothing_kept = world.player_system.module.is_none();
    let mut req = dereth_client_model::RecordingRequests::default();
    let sends_nothing = !world.player_system.save_to_server(&mut req, true) && req.0.is_empty();
    let flag_cleared = !world.player_system.is_dirty();

    // ...and the setting still moves locally, so the state is right the moment a description
    // arrives.
    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(PlayerOption::AutoTarget, false)],
    );
    let moved_locally = inter
        .run_ui_requests(&mut world, false, ServerTime(0.0))
        .is_empty()
        && !world.player_system.options.auto_target()
        && world.player_system.module.is_none();

    // And a record the shard shaped is narrowed to the shape this client sends: what a
    // description may carry and a settings record may not is dropped, with its gate.
    let record = the_record_of("first-login-walk-jump");
    let mut module = decode_module(&record.body);
    module.timestamp_format = Some("%H:%M".to_owned());
    module.spell_bars.truncate(5);
    module.option_flags = (module.option_flags & !0x0400) | 0x0080 | 0x0004;
    let mut shaped = world_with(&module);
    let packed = shaped
        .player_system
        .client_packed_module()
        .expect("the record is kept");
    let narrowed = packed.option_flags & 0x0080 == 0
        && packed.option_flags & 0x0004 == 0
        && packed.timestamp_format.is_none()
        && packed.spell_bars.len() == 8
        && packed.spell_bars[..5] == module.spell_bars[..]
        && !encode_module(&packed).is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.with-no-description-nothing-is-sent-and-nothing-is-invented",
        move |_| nothing_kept && sends_nothing && flag_cleared && moved_locally && narrowed,
    );
}

#[test]
fn scenario_with_no_description_nothing_is_sent() {
    scenario("with_no_description_nothing_is_sent");
}

// ---------------------------------------------------------------------------------------------
// The character options page, live
// ---------------------------------------------------------------------------------------------

/// `UICore_Button_toggled` -- the attribute a tick box's drawn value lives in.
const ATTR_CHECKED: u32 = 0x0E;

/// A client in the world with the shipped settings the shard would have sent.
fn a_client_with_settings() -> HeadlessClient {
    use dereth_protocol::login::{LoginPlayerDescription, PlayerModule};

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // The shard's own description, in memory: no socket and no link. Its settings are the shipped
    // defaults, which is what every character starts from.
    c.app_mut().apply_hud_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(
            LoginPlayerDescription {
                player_module: PlayerModule {
                    options: dereth_client_model::player::options::DEFAULT_OPTIONS,
                    options2: dereth_client_model::player::options::DEFAULT_OPTIONS2,
                    ..PlayerModule::default()
                },
                ..LoginPlayerDescription::default()
            },
        )),
    ]);
    c.tick(3);
    c
}

/// Do something with the live gameplay screen.
fn with_gameplay<R>(
    c: &mut HeadlessClient,
    f: impl FnOnce(
        &mut dereth_ui::UiSystem,
        &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
    ) -> R,
) -> R {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    let gameplay = any
        .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("the gameplay screen is current");
    f(&mut shell.ui, gameplay)
}

/// Open the character options page through the toolbar and the tab.
fn open_the_character_options_page(
    c: &mut HeadlessClient,
    hands: &mut Hands,
) -> dereth_ui::ElemHandle {
    let page = with_gameplay(c, |_, s| {
        s.character_options
            .page
            .expect("the character options page is bound")
    });
    open_options_page(c, hands, page)
}

/// What the player sees on a tick box.
fn drawn_tick(c: &HeadlessClient, h: dereth_ui::ElemHandle) -> bool {
    dereth_ui_screens::bind::attr_bool(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        h,
        ATTR_CHECKED,
    )
    .unwrap_or(false)
}

/// The first row whose value is `want`, skipping `skip`, whose box the pointer would really land
/// on -- the list clips everything past the visible dozen, and a scenario that aimed at a clipped
/// row would be measuring the viewport rather than the tick box.
fn a_clickable_row(
    c: &mut HeadlessClient,
    want: bool,
    skip: usize,
) -> (usize, dereth_ui::ElemHandle) {
    let rows: Vec<(usize, bool, dereth_ui::ElemHandle)> = with_gameplay(c, |_, s| {
        s.character_options
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| (i, r.current, r.element))
            .collect()
    });
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the UI shell is up").ui;
    rows.into_iter()
        .find(|(i, cur, h)| {
            *cur == want && *i != skip && {
                let b = ui.screen_box(*h);
                ui.hit_test_screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(*h)
            }
        })
        .map(|(i, _, h)| (i, h))
        .unwrap_or_else(|| panic!("no reachable row is currently {want}"))
}

/// Ticking a row sticks, and unticking still works.
pub fn a_ticked_row_stays_ticked_and_unticking_still_works() {
    let mut c = a_client_with_settings();
    let mut hands = Hands::new();
    open_the_character_options_page(&mut c, &mut hands);

    // Off to on, which is the direction a page that loses the tick fails in.
    let (i, h) = a_clickable_row(&mut c, false, usize::MAX);
    let started_off = !drawn_tick(&c, h);
    hands.click_handle(&mut c, h);
    let drawn = drawn_tick(&c, h);
    let remembered = with_gameplay(&mut c, |_, s| s.character_options.rows[i].current);
    let in_the_settings = {
        let o = with_gameplay(&mut c, |_, s| s.character_options.rows[i].option);
        c.view()
            .objects()
            .world
            .player_system
            .options
            .get(dereth_client::hud::option_ordinal(o))
    };
    // ...and still all three a good while later: the page re-reads every row from the settings on
    // every frame, so a row that lost the race would come back off on some later frame.
    c.tick(32);
    let still_drawn = drawn_tick(&c, h);
    let still_set = {
        let o = with_gameplay(&mut c, |_, s| s.character_options.rows[i].option);
        c.view()
            .objects()
            .world
            .player_system
            .options
            .get(dereth_client::hud::option_ordinal(o))
    };

    // On to off, on a different row so this is not the one just ticked.
    let (j, g) = a_clickable_row(&mut c, true, i);
    hands.click_handle(&mut c, g);
    let unticked =
        !drawn_tick(&c, g) && !with_gameplay(&mut c, |_, s| s.character_options.rows[j].current);
    c.tick(32);
    let stays_unticked = !drawn_tick(&c, g);

    c.assert_behaviour(
        "options.character-page.a-row-ticked-on-stays-ticked-and-unticking-still-works",
        move |_| {
            started_off
                && drawn
                && remembered
                && in_the_settings
                && still_drawn
                && still_set
                && unticked
                && stays_unticked
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_ticked_row_stays_ticked_and_unticking_still_works() {
    scenario("a_ticked_row_stays_ticked_and_unticking_still_works");
}

// ---------------------------------------------------------------------------------------------
// pointer.wheel.*
//
// The wheel scrolls scrollbars and must not also flip radios or tick boxes. The harness does not
// pack the wheel message itself -- `Hands::wheel` asks the client's own mapping for it -- so the
// harness and the client cannot disagree about it.
//
// **The row the gesture is aimed at is chosen for its value, and that is the instrument.** A tick
// box that is *off* can be flipped on and flipped straight back within the frame without either
// write being visible at the end of it; a box that starts *on* has no such eraser. So a scenario
// built on a ticked row can go red and one built on an unticked row cannot.
// ---------------------------------------------------------------------------------------------

/// The character options list, and a ticked row whose box the pointer really lands on.
fn the_list_and_a_ticked_row(
    c: &mut HeadlessClient,
) -> (dereth_ui::ElemHandle, dereth_ui::ElemHandle) {
    let (list, ticked): (dereth_ui::ElemHandle, Vec<usize>) = with_gameplay(c, |_, s| {
        let list = s
            .character_options
            .option_box
            .as_ref()
            .expect("the option list")
            .handle;
        let ticked = s
            .character_options
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.current)
            .map(|(i, _)| i)
            .collect();
        (list, ticked)
    });
    assert!(
        !ticked.is_empty(),
        "this character has some setting on; without one there is no instrument"
    );
    for i in ticked {
        // Bring the row into the pane first, as any player would have to before the pointer could
        // be over it.
        with_gameplay(c, |ui, s| {
            let row = s.character_options.rows[i].row;
            let b = s
                .character_options
                .option_box
                .as_mut()
                .expect("the option list");
            if let Some(idx) = b.items.iter().position(|h| *h == row) {
                b.scroll_to_view(ui, idx);
            }
        });
        c.tick(1);
        let h = with_gameplay(c, |_, s| s.character_options.rows[i].element);
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        let b = ui.screen_clip_box(h);
        if !b.is_valid() || !ui.is_visible(h) {
            continue;
        }
        if ui.hit_test_screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(h) {
            return (list, h);
        }
    }
    panic!("no ticked tick box hit-tests to itself");
}

/// How far the option list has been scrolled.
fn list_scroll(c: &mut HeadlessClient, list: dereth_ui::ElemHandle) -> i32 {
    with_gameplay(c, |ui, _| {
        ui.node(list)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::listbox::ListBox>()
            })
            .expect("the option list is a list box")
            .scroll
            .y
    })
}

/// A wheel detent over a tick box scrolls the list and leaves the box alone.
pub fn a_detent_over_a_tick_box_scrolls_the_list() {
    let mut c = a_client_with_settings();
    let mut hands = Hands::new();
    open_the_character_options_page(&mut c, &mut hands);
    let (list, row) = the_list_and_a_ticked_row(&mut c);

    let before_value = drawn_tick(&c, row);
    let before_y = list_scroll(&mut c, list);

    let (x, y) = {
        let app = c.view().expect_app();
        let b = app
            .ui()
            .expect("the UI shell is up")
            .ui
            .screen_clip_box(row);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    hands.move_to(&mut c, x, y);
    hands.wheel(&mut c, -1.0);
    c.tick(2);

    let after_y = list_scroll(&mut c, list);
    let after_value = drawn_tick(&c, row);

    c.assert_behaviour(
        "pointer.wheel.a-detent-over-a-check-box-scrolls-the-list-and-leaves-the-box-alone",
        move |_| before_value && after_y > before_y && after_value == before_value,
    );
    c.shutdown();
}

#[test]
fn scenario_a_detent_over_a_tick_box_scrolls_the_list() {
    scenario("a_detent_over_a_tick_box_scrolls_the_list");
}

/// The same box still answers a click, and so does the list's own bar.
pub fn the_same_box_still_answers_a_click_and_so_does_the_bar() {
    let mut c = a_client_with_settings();
    let mut hands = Hands::new();
    open_the_character_options_page(&mut c, &mut hands);
    let (list, row) = the_list_and_a_ticked_row(&mut c);

    let before = drawn_tick(&c, row);
    hands.click_handle(&mut c, row);
    let flipped = !drawn_tick(&c, row);

    // The other half of the blast radius: the list's own scrollbar is built out of the same kind
    // of widget, so a refusal written in the wrong place would make every bar in the client inert.
    let bar = with_gameplay(&mut c, |ui, _| {
        let s = ui
            .node(list)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::listbox::ListBox>()
            })
            .expect("the option list is a list box")
            .scroll
            .clone();
        s.scrollbar(ui, list, false)
            .expect("the option list names a vertical bar")
    });
    let before_y = list_scroll(&mut c, list);
    let (x, y) = {
        let app = c.view().expect_app();
        let b = app
            .ui()
            .expect("the UI shell is up")
            .ui
            .screen_clip_box(bar);
        assert!(b.is_valid(), "the bar is drawn");
        ((b.x0 + b.x1) / 2, b.y1 - 2)
    };
    hands.click_at(&mut c, x, y);
    let after_y = list_scroll(&mut c, list);

    c.assert_behaviour(
        "pointer.wheel.the-same-box-still-answers-a-click-and-so-does-the-bar",
        move |_| before && flipped && after_y > before_y,
    );
    c.shutdown();
}

#[test]
fn scenario_the_same_box_still_answers_a_click_and_so_does_the_bar() {
    scenario("the_same_box_still_answers_a_click_and_so_does_the_bar");
}

// ---------------------------------------------------------------------------------------------
// The two fellowship settings, and the shard that holds them
//
// The Auto-Accept box must not show off while auto-accept is on. Whether a fellowship request is
// ignored or accepted is **the shard's** decision, read off its own copy of the character's
// settings -- so a box can only be said to match the setting if the shard's copy is the client's.
// The shard is therefore modelled here, by folding what the client sends through the reference
// server's own rule: one message sets one setting, and it applies no exclusion of its own.
//
// Nothing leaves this process: the "shard" is that fold, and the description it sends back is
// handed to the client in memory.
// ---------------------------------------------------------------------------------------------

/// The two settings, as the client and the reference server both number them.
const IGNORE_REQUESTS: u32 = 0x02;
const AUTO_ACCEPT_REQUESTS: u32 = 0x12;
/// The two tick boxes of the fellowship tab, by the ids the panel builds them with.
const IGNORE_BOX: ElementId = ElementId(0x1000_0270);
const AUTO_ACCEPT_BOX: ElementId = ElementId(0x1000_0271);
/// The social panel, and the fellowship page inside it.
const SOCIAL_PANEL: u32 = 0x0C;
const SOCIAL_PAGE: ElementId = ElementId(0x1000_018F);
const FELLOWSHIP_PAGE: ElementId = ElementId(0x1000_0292);

/// The shard's own copy of the character's settings, written only by what the client sends.
///
/// One message names one setting and a value, and the reference server sets exactly that bit.
/// **It applies no exclusion of its own**, which is the whole reason the client has to send both
/// halves of one.
struct ShardSettings {
    word: u32,
}

impl ShardSettings {
    fn mask(option: u32) -> u32 {
        match option {
            IGNORE_REQUESTS => 0x0000_0008,
            AUTO_ACCEPT_REQUESTS => 0x2000_0000,
            other => {
                panic!("this scenario models only the two fellowship settings, not {other:#x}")
            }
        }
    }

    fn apply(&mut self, option: u32, value: bool) {
        let m = Self::mask(option);
        if value {
            self.word |= m;
        } else {
            self.word &= !m;
        }
    }

    fn holds(&self, option: u32) -> bool {
        self.word & Self::mask(option) != 0
    }
}

/// A client in the world, with the shard's settings word, and the shard beside it.
fn a_client_and_a_shard() -> (HeadlessClient, ShardSettings, usize) {
    let c = a_client_with_settings();
    let seen = c.outbound().len();
    (
        c,
        ShardSettings {
            word: dereth_client_model::player::options::DEFAULT_OPTIONS,
        },
        seen,
    )
}

/// Hand the client the shard's own description of the character -- the login, and the relog.
fn the_shard_describes_the_character(c: &mut HeadlessClient, shard: &ShardSettings) {
    use dereth_protocol::login::{LoginPlayerDescription, PlayerModule};

    c.app_mut().apply_hud_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(
            LoginPlayerDescription {
                player_module: PlayerModule {
                    options: shard.word,
                    options2: dereth_client_model::player::options::DEFAULT_OPTIONS2,
                    ..PlayerModule::default()
                },
                ..LoginPlayerDescription::default()
            },
        )),
    ]);
    c.tick(3);
}

/// Every setting change the client has put out since `seen`, in order, folded into the shard.
fn to_the_shard(
    c: &HeadlessClient,
    shard: &mut ShardSettings,
    seen: &mut usize,
) -> Vec<(u32, bool)> {
    let out: Vec<(u32, bool)> = c.outbound()[*seen..]
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::PlayerOptionChanged(m) => Some((m.option, m.value != 0)),
            _ => None,
        })
        .collect();
    *seen = c.outbound().len();
    for (o, v) in &out {
        shard.apply(*o, *v);
    }
    out
}

/// Open the social panel and then the fellowship tab inside it, by clicking what a player clicks.
fn open_the_fellowship_tab(c: &mut HeadlessClient, hands: &mut Hands) {
    let button = with_gameplay(c, |_, s| {
        s.toolbar
            .buttons
            .iter()
            .find(|b| b.panel_id == SOCIAL_PANEL)
            .expect("the toolbar has a social button")
            .handle
    });
    hands.click_handle(c, button);
    let tab = with_gameplay(c, |ui, s| {
        let root = s.root().expect("the gameplay root");
        let page = ui
            .get_child_recursive(root, SOCIAL_PAGE)
            .expect("the social page");
        let id = ui
            .node(page)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .and_then(|p| p.page_to_tab.get(&FELLOWSHIP_PAGE).copied())
            .expect("the social page's tab table names the fellowship page");
        ui.get_child_recursive(page, id)
            .expect("the tab caption element")
    });
    hands.click_handle(c, tab);
    c.tick(2);
    let page = element(c, FELLOWSHIP_PAGE);
    assert!(
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .is_visible(page),
        "the fellowship tab must be up"
    );
}

/// Whether the two boxes agree with the shard and with the client's own settings.
fn the_boxes_match(c: &mut HeadlessClient, shard: &ShardSettings) -> bool {
    let auto = drawn_tick(c, element(c, AUTO_ACCEPT_BOX));
    let ignore = drawn_tick(c, element(c, IGNORE_BOX));
    let model = |ordinal: usize| c.view().objects().world.player_system.options.get(ordinal);
    auto == shard.holds(AUTO_ACCEPT_REQUESTS)
        && auto == model(18)
        && ignore == shard.holds(IGNORE_REQUESTS)
        && ignore == model(2)
}

/// Turning one of the two on tells the shard the other is off, first.
pub fn one_fellowship_setting_turns_the_other_off_at_the_shard() {
    let (mut c, mut shard, mut seen) = a_client_and_a_shard();
    let mut hands = Hands::new();
    the_shard_describes_the_character(&mut c, &shard);
    open_the_fellowship_tab(&mut c, &mut hands);

    // The shipped default: requests are ignored and nothing is auto-accepted.
    let at_login = drawn_tick(&c, element(&c, IGNORE_BOX))
        && !drawn_tick(&c, element(&c, AUTO_ACCEPT_BOX))
        && the_boxes_match(&mut c, &shard)
        && to_the_shard(&c, &mut shard, &mut seen).is_empty();

    // 1. Stop ignoring: one message, nothing else moves.
    let h = element(&c, IGNORE_BOX);
    hands.click_handle(&mut c, h);
    let step1 = to_the_shard(&c, &mut shard, &mut seen) == vec![(IGNORE_REQUESTS, false)]
        && the_boxes_match(&mut c, &shard);

    // 2. Auto-accept on, with ignoring already off: no exclusion fires and one message goes.
    let h = element(&c, AUTO_ACCEPT_BOX);
    hands.click_handle(&mut c, h);
    let step2 = to_the_shard(&c, &mut shard, &mut seen) == vec![(AUTO_ACCEPT_REQUESTS, true)]
        && the_boxes_match(&mut c, &shard)
        && shard.holds(AUTO_ACCEPT_REQUESTS);

    // 3. Ignore on. This one *does* exclude, and the shard has to be told the cleared setting
    //    first -- a client that cleared it silently would leave the shard's copy stale.
    let h = element(&c, IGNORE_BOX);
    hands.click_handle(&mut c, h);
    let step3 = to_the_shard(&c, &mut shard, &mut seen)
        == vec![(AUTO_ACCEPT_REQUESTS, false), (IGNORE_REQUESTS, true)]
        && the_boxes_match(&mut c, &shard)
        && shard.holds(IGNORE_REQUESTS)
        && !shard.holds(AUTO_ACCEPT_REQUESTS);

    // 4. Ignore off again: one message, and the shard now auto-accepts nothing.
    let h = element(&c, IGNORE_BOX);
    hands.click_handle(&mut c, h);
    let step4 = to_the_shard(&c, &mut shard, &mut seen) == vec![(IGNORE_REQUESTS, false)]
        && the_boxes_match(&mut c, &shard)
        && !shard.holds(AUTO_ACCEPT_REQUESTS);

    // 5. Log in again: the shard sends its own word back and the boxes follow it.
    the_shard_describes_the_character(&mut c, &shard);
    let after_relog =
        the_boxes_match(&mut c, &shard) && !drawn_tick(&c, element(&c, AUTO_ACCEPT_BOX));

    c.assert_behaviour(
        "options.fellowship.turning-one-of-the-two-on-tells-the-shard-the-other-is-off-first",
        move |_| at_login && step1 && step2 && step3 && step4 && after_relog,
    );
    c.shutdown();
}

#[test]
fn scenario_one_fellowship_setting_turns_the_other_off_at_the_shard() {
    scenario("one_fellowship_setting_turns_the_other_off_at_the_shard");
}

/// The same from the shipped default, the other way round -- and turning it off is one message.
pub fn auto_accept_from_the_default_clears_ignoring_at_the_shard() {
    let (mut c, mut shard, mut seen) = a_client_and_a_shard();
    let mut hands = Hands::new();
    the_shard_describes_the_character(&mut c, &shard);
    open_the_fellowship_tab(&mut c, &mut hands);
    let starts_ignoring = the_boxes_match(&mut c, &shard) && shard.holds(IGNORE_REQUESTS);
    let _ = to_the_shard(&c, &mut shard, &mut seen);

    // From the shipped default, auto-accept on has to clear ignoring, and the shard is told.
    let h = element(&c, AUTO_ACCEPT_BOX);
    hands.click_handle(&mut c, h);
    let both_told = to_the_shard(&c, &mut shard, &mut seen)
        == vec![(IGNORE_REQUESTS, false), (AUTO_ACCEPT_REQUESTS, true)]
        && the_boxes_match(&mut c, &shard)
        && !shard.holds(IGNORE_REQUESTS)
        && shard.holds(AUTO_ACCEPT_REQUESTS);

    // Turning it off excludes nothing -- the rule is about the setting being turned *on* -- so
    // one message.
    let h = element(&c, AUTO_ACCEPT_BOX);
    hands.click_handle(&mut c, h);
    let one_message = to_the_shard(&c, &mut shard, &mut seen)
        == vec![(AUTO_ACCEPT_REQUESTS, false)]
        && the_boxes_match(&mut c, &shard)
        && !shard.holds(IGNORE_REQUESTS)
        && !shard.holds(AUTO_ACCEPT_REQUESTS);

    the_shard_describes_the_character(&mut c, &shard);
    let after_relog = the_boxes_match(&mut c, &shard);

    c.assert_behaviour("options.fellowship.the-same-holds-from-the-shipped-default-and-turning-it-off-again-is-one-message", move |_| {
        starts_ignoring && both_told && one_message && after_relog
    });
    c.shutdown();
}

#[test]
fn scenario_auto_accept_from_the_default_clears_ignoring_at_the_shard() {
    scenario("auto_accept_from_the_default_clears_ignoring_at_the_shard");
}

/// On the options page the excluded row goes out at once, and cancelling restores both.
pub fn the_excluded_row_goes_out_at_once_and_cancel_restores_both() {
    use dereth_ui_screens::view::PlayerOption as P;

    let (mut c, mut shard, mut seen) = a_client_and_a_shard();
    let mut hands = Hands::new();
    the_shard_describes_the_character(&mut c, &shard);
    open_the_character_options_page(&mut c, &mut hands);
    let opened_quietly = to_the_shard(&c, &mut shard, &mut seen).is_empty();

    let ignore_row = with_gameplay(&mut c, |_, s| {
        s.character_options
            .row_of(P::IgnoreFellowshipRequests)
            .expect("the row")
    });
    let auto_row = with_gameplay(&mut c, |_, s| {
        s.character_options
            .row_of(P::FellowshipAutoAcceptRequests)
            .expect("the row")
    });
    let row_drawn = |c: &mut HeadlessClient, i: usize| {
        let h = with_gameplay(c, |_, s| s.character_options.rows[i].element);
        drawn_tick(c, h)
    };
    let at_the_default = row_drawn(&mut c, ignore_row) && !row_drawn(&mut c, auto_row);

    // Tick auto-accept on the page. The row four above it has to go out on the notice, without
    // waiting for the page to be opened again.
    with_gameplay(&mut c, |ui, s| {
        let row = s.character_options.rows[auto_row].row;
        let list = s
            .character_options
            .option_box
            .as_mut()
            .expect("the option list");
        let idx = list
            .items
            .iter()
            .position(|h| *h == row)
            .expect("the row is a list item");
        list.scroll_to_view(ui, idx);
    });
    c.tick(1);
    let h = with_gameplay(&mut c, |_, s| s.character_options.rows[auto_row].element);
    hands.click_handle(&mut c, h);
    let both_told = to_the_shard(&c, &mut shard, &mut seen)
        == vec![(IGNORE_REQUESTS, false), (AUTO_ACCEPT_REQUESTS, true)];
    let rows_followed = row_drawn(&mut c, auto_row) && !row_drawn(&mut c, ignore_row);
    let shard_followed = !shard.holds(IGNORE_REQUESTS) && shard.holds(AUTO_ACCEPT_REQUESTS);
    let boxes_followed = the_boxes_match(&mut c, &shard);

    // Cancel restores what the page opened with -- **both** rows, because both moved -- and the
    // shard is told about both, or it would keep a setting the player has just cancelled.
    let cancel = with_gameplay(&mut c, |ui, s| {
        let page = s.character_options.page.expect("the page");
        ui.get_child_recursive(page, dereth_ui_screens::options::config::button::CANCEL)
            .expect("the page's cancel button")
    });
    hands.click_handle(&mut c, cancel);
    let cancelled = to_the_shard(&c, &mut shard, &mut seen)
        == vec![(AUTO_ACCEPT_REQUESTS, false), (IGNORE_REQUESTS, true)];
    let back_at_the_default = row_drawn(&mut c, ignore_row) && !row_drawn(&mut c, auto_row);
    let shard_back = shard.holds(IGNORE_REQUESTS) && !shard.holds(AUTO_ACCEPT_REQUESTS);
    let boxes_back = the_boxes_match(&mut c, &shard);

    c.assert_behaviour(
        "options.character-page.the-excluded-row-goes-out-at-once-and-cancel-restores-both",
        move |_| {
            opened_quietly
                && at_the_default
                && both_told
                && rows_followed
                && shard_followed
                && boxes_followed
                && cancelled
                && back_at_the_default
                && shard_back
                && boxes_back
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_excluded_row_goes_out_at_once_and_cancel_restores_both() {
    scenario("the_excluded_row_goes_out_at_once_and_cancel_restores_both");
}

// ---------------------------------------------------------------------------------------------
// options.chat-page.*
//
// The page is five per-window filter controls carrying sixty-four tick boxes between them and two
// opacity sliders, and **nothing a player does on it goes on the wire**: a change writes the
// retained settings record, raises a notice the chat windows answer, and waits for the eight-minute
// flush that `options.character-page.a-deferred-change-...` is about.
// ---------------------------------------------------------------------------------------------

use dereth_protocol::property::BasePropertyValue;
use dereth_ui_screens::options::chat::{ChatOption, ATTR_IMAGE_ALL, ATTR_IMAGE_SOME};

/// The five windows the page edits, in the order it builds them.
const CHAT_WINDOWS: [u32; 5] = [8, 2, 3, 4, 5];
/// The main window's first group -- the one the scenarios click.
const MASK_COMBAT: u64 = 0x0060_0040;
/// The filter property, the two opacity properties, and the per-window array they live in.
const PROP_FILTER: u32 = 0x1000_007F;
const PROP_IDLE_OPACITY: u32 = 0x1000_0080;
const PROP_ACTIVE_OPACITY: u32 = 0x1000_0081;
const PROP_WINDOW_ARRAY: u32 = 0x1000_008C;
/// The settings refresh writes this slider-position attribute.
const ATTR_POSITION: u32 = 0x86;

/// A client in the world whose settings record is `module`, as the shard's description gives it
/// one. Without a record there is nothing for a chat option to be written into, which is the
/// state a client is in before it logs in.
fn a_client_with_module(module: dereth_protocol::login::PlayerModule) -> HeadlessClient {
    use dereth_protocol::login::LoginPlayerDescription;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.app_mut().apply_hud_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(
            LoginPlayerDescription {
                player_module: module,
                ..LoginPlayerDescription::default()
            },
        )),
    ]);
    c.tick(3);
    c
}

/// A settings record whose main chat window's filter is exactly `mask`.
fn a_module_whose_main_filter_is(mask: u64) -> dereth_protocol::login::PlayerModule {
    use dereth_protocol::property::{BaseProperty, PackObjPropertyCollection, PropertyCollection};

    let property = |name: u32, value: BasePropertyValue| {
        (
            name,
            BaseProperty {
                name,
                value: Some(value),
            },
        )
    };
    let mut rows = vec![
        BaseProperty {
            name: 0x1000_008B,
            value: Some(BasePropertyValue::Struct(PropertyCollection::default())),
        };
        8
    ];
    rows[7].value = Some(BasePropertyValue::Struct(PropertyCollection {
        bucket_index: 2,
        entries: vec![property(PROP_FILTER, BasePropertyValue::Bitfield64(mask))],
    }));
    dereth_protocol::login::PlayerModule {
        gameplay_options: Some(PackObjPropertyCollection {
            version: 2,
            properties: PropertyCollection {
                bucket_index: 3,
                entries: vec![property(PROP_WINDOW_ARRAY, BasePropertyValue::Array(rows))],
            },
        }),
        ..dereth_protocol::login::PlayerModule::default()
    }
}

/// Open the chat options page through the toolbar and its tab.
fn open_the_chat_options_page(c: &mut HeadlessClient, hands: &mut Hands) -> dereth_ui::ElemHandle {
    let page = with_gameplay(c, |_, s| {
        s.chat_options.page.expect("the chat options page is bound")
    });
    open_options_page(c, hands, page)
}

/// The index of one window's filter control.
fn filter_control(c: &mut HeadlessClient, window: u32) -> usize {
    with_gameplay(c, |_, s| {
        s.chat_options
            .filter_of(window)
            .expect("the page has a control for that window")
    })
}

/// The page's own word for one window's filter.
fn filter_current(c: &mut HeadlessClient, window: u32) -> u64 {
    let i = filter_control(c, window);
    with_gameplay(c, |_, s| match &s.chat_options.options[i] {
        ChatOption::Filter(f) => f.current,
        ChatOption::Opacity(_) => unreachable!("that control is a filter"),
    })
}

/// Whether one group's tick box of one window's control is drawn ticked.
fn filter_box_drawn(c: &mut HeadlessClient, window: u32, group: usize) -> bool {
    let (h, _) = filter_box(c, window, group);
    drawn_tick(c, h)
}

/// The tick box of one group of one window's control, and the group's own mask.
fn filter_box(c: &mut HeadlessClient, window: u32, group: usize) -> (dereth_ui::ElemHandle, u64) {
    let i = filter_control(c, window);
    with_gameplay(c, |_, s| match &s.chat_options.options[i] {
        ChatOption::Filter(f) => (
            f.children[group]
                .element
                .expect("the control built its boxes"),
            f.children[group].mask,
        ),
        ChatOption::Opacity(_) => unreachable!("that control is a filter"),
    })
}

/// The chat window's own filter -- what routing really reads, as opposed to the page's word.
fn window_filter(c: &mut HeadlessClient, window: u32) -> u64 {
    with_gameplay(c, |_, s| {
        s.chat
            .iter()
            .find(|w| w.window_id == window)
            .expect("the chat window")
            .filter
    })
}

/// The idle and active opacity the chat window holds -- what the fade reads.
fn window_opacity(c: &mut HeadlessClient, window: u32) -> (f32, f32) {
    with_gameplay(c, |_, s| {
        let w = s
            .chat
            .iter()
            .find(|w| w.window_id == window)
            .expect("the chat window");
        (w.default_opacity, w.active_opacity)
    })
}

/// The slider control for one property, and what it holds.
fn slider_control(c: &mut HeadlessClient, property: u32) -> usize {
    with_gameplay(c, |_, s| {
        s.chat_options.slider_of(property).expect("the page has it")
    })
}

fn slider_element(c: &mut HeadlessClient, property: u32) -> dereth_ui::ElemHandle {
    let i = slider_control(c, property);
    with_gameplay(c, |_, s| match &s.chat_options.options[i] {
        ChatOption::Opacity(o) => o.element,
        ChatOption::Filter(_) => unreachable!("that control is a slider"),
    })
}

fn slider_current(c: &mut HeadlessClient, property: u32) -> f32 {
    let i = slider_control(c, property);
    with_gameplay(c, |_, s| match &s.chat_options.options[i] {
        ChatOption::Opacity(o) => o.current,
        ChatOption::Filter(_) => unreachable!("that control is a slider"),
    })
}

/// One window's filter as the retained settings record holds it.
fn module_filter(c: &HeadlessClient, window: u32) -> Option<u64> {
    let app = c.view().expect_app();
    let m = app.objects().world.player_system.module.as_ref()?;
    let BasePropertyValue::Array(rows) = m
        .gameplay_options
        .as_ref()?
        .properties
        .get(PROP_WINDOW_ARRAY)?
    else {
        return None;
    };
    let BasePropertyValue::Struct(fields) = rows
        .get((window as usize).checked_sub(1)?)?
        .value
        .as_ref()?
    else {
        return None;
    };
    match fields.get(PROP_FILTER)? {
        BasePropertyValue::Bitfield64(v) => Some(*v),
        _ => None,
    }
}

/// One of the two opacities as the retained record holds it.
fn module_opacity(c: &HeadlessClient, property: u32) -> Option<f32> {
    let app = c.view().expect_app();
    let m = app.objects().world.player_system.module.as_ref()?;
    match m.gameplay_options.as_ref()?.properties.get(property)? {
        BasePropertyValue::Float(v) => Some(*v),
        _ => None,
    }
}

/// Bring a control's row into the pane, as a player would have to before the pointer could reach
/// it.
fn scroll_chat_control_into_view(c: &mut HeadlessClient, i: usize) {
    with_gameplay(c, |ui, s| {
        let row = match &s.chat_options.options[i] {
            ChatOption::Filter(f) => f.element,
            ChatOption::Opacity(o) => o.row,
        };
        let list = s.chat_options.option_box.as_mut().expect("the option list");
        if let Some(idx) = list.items.iter().position(|h| *h == row) {
            list.scroll_to_view(ui, idx);
        }
    });
    c.tick(1);
}

/// One of the page's three buttons, scoped to this page -- all three option pages carry the same
/// three ids, so an unscoped lookup answers on the wrong one.
fn chat_page_button(c: &mut HeadlessClient, id: ElementId) -> dereth_ui::ElemHandle {
    with_gameplay(c, |ui, s| {
        let page = s.chat_options.page.expect("the page");
        ui.get_child_recursive(page, id).expect("the page's button")
    })
}

/// The tab comes up with every control the page declares.
pub fn the_chat_options_tab_draws_its_controls() {
    let mut c = a_client_with_module(dereth_protocol::login::PlayerModule::default());
    let mut hands = Hands::new();
    let seen = c.outbound().len();
    open_the_chat_options_page(&mut c, &mut hands);

    let ordered_and_linked = with_gameplay(&mut c, |_, s| {
        let keys = s
            .chat_options
            .options
            .iter()
            .map(|option| match option {
                ChatOption::Opacity(o) => (o.property, 0),
                ChatOption::Filter(f) => (f.property, f.window_id),
            })
            .collect::<Vec<_>>();
        keys == [
            (0x1000_0080, 0),
            (0x1000_0081, 0),
            (0x1000_007F, 8),
            (0x1000_007F, 2),
            (0x1000_007F, 3),
            (0x1000_007F, 4),
            (0x1000_007F, 5),
        ] && s.chat_options.slider_links == [(0, 1)]
    });
    let (controls, boxes, headers, separators, rows, failures) = with_gameplay(&mut c, |_, s| {
        (
            s.chat_options.options.len(),
            s.chat_options.filter_child_count(),
            s.chat_options.headers,
            s.chat_options.separators,
            s.chat_options.row_count(),
            s.chat_options.failures,
        )
    });
    let (header_captions, slider_captions, child_captions) = with_gameplay(&mut c, |_, s| {
        (
            s.chat_options.header_captions,
            s.chat_options.slider_end_captions,
            s.chat_options.child_captions,
        )
    });
    // The chat font's face and size: two drop-downs in the page's box, just under the two
    // opacity sliders.
    let fonts_under_the_opacity = with_gameplay(&mut c, |_, s| {
        let Some(list) = s.chat_options.option_box.as_ref() else {
            return false;
        };
        let at = s.chat_options.after_opacity();
        let names: Vec<&str> = s
            .chat_font_rows
            .iter()
            .map(|&i| s.config_page.options[i].preference)
            .collect();
        let places: Vec<Option<usize>> = s
            .chat_font_rows
            .iter()
            .map(|&i| list.index_of(s.config_page.options[i].row))
            .collect();
        names == ["UI.ChatFontFace", "UI.ChatFontSize"]
            && at.is_some()
            && places == [at, at.map(|a| a + 1)]
    });

    // Every control opens at its window's own default, and every box is drawn from that default.
    let mut opens_at_the_defaults = true;
    for w in CHAT_WINDOWS {
        let want = dereth_ui_screens::chat::interface::default_filter(w);
        opens_at_the_defaults &= filter_current(&mut c, w) == want;
        let groups = dereth_ui_screens::chat::interface::filter_groups_for(w).len();
        for k in 0..groups {
            let (h, mask) = filter_box(&mut c, w, k);
            opens_at_the_defaults &= drawn_tick(&c, h) == (want & mask != 0);
        }
    }
    let sliders_open_at_their_defaults = (slider_current(&mut c, PROP_IDLE_OPACITY) - 0.5).abs()
        < 1e-6
        && (slider_current(&mut c, PROP_ACTIVE_OPACITY) - 1.0).abs() < 1e-6;
    let quiet = c.outbound().len() == seen;

    c.assert_behaviour(
        "options.chat-page.the-tab-comes-up-with-every-control-the-page-declares",
        move |_| {
            // Seven controls, sixty-four boxes, six headings and six rules between them; every
            // caption resolved out of the shipped text, and nothing failed to build.
            controls == 7
                && ordered_and_linked
                && boxes == 64
                && headers == 6
                && separators == 6
                && rows == headers + separators + controls + 2
                && failures == 0
                && header_captions == 6
                && slider_captions == 2
                && child_captions == 64
                && opens_at_the_defaults
                && sliders_open_at_their_defaults
                && quiet
        },
    );
    c.assert_behaviour(
        "options.chat-page.the-chat-fonts-face-and-size-sit-under-the-windows-opacity",
        move |_| fonts_under_the_opacity,
    );
    c.shutdown();
}

#[test]
fn scenario_the_chat_options_tab_draws_its_controls() {
    scenario("the_chat_options_tab_draws_its_controls");
}

/// A partly chosen group is drawn differently from a wholly chosen one.
pub fn a_partly_chosen_group_is_drawn_differently() {
    // One bit of the combat group, which is what makes it partly chosen.
    let mut c = a_client_with_module(a_module_whose_main_filter_is(0x40));
    let mut hands = Hands::new();
    open_the_chat_options_page(&mut c, &mut hands);
    let i = filter_control(&mut c, 8);
    scroll_chat_control_into_view(&mut c, i);

    let (button, mask) = filter_box(&mut c, 8, 0);
    let control = with_gameplay(&mut c, |_, s| match &s.chat_options.options[i] {
        ChatOption::Filter(f) => f.element,
        ChatOption::Opacity(_) => unreachable!(),
    });
    let (some, all) = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        (
            dereth_ui_screens::bind::attr_data_id(ui, control, ATTR_IMAGE_SOME)
                .expect("the control carries its partly-chosen picture"),
            dereth_ui_screens::bind::attr_data_id(ui, control, ATTR_IMAGE_ALL)
                .expect("the control carries its wholly-chosen picture"),
        )
    };
    let two_pictures = some != all;

    // The picture has to reach the live region **and** the frame the client drew: recording which
    // one was wanted is not a drawn result.
    let box_media =
        |c: &mut HeadlessClient| -> (dereth_ui::ElemHandle, Option<dereth_primitives::DataId>) {
            let (b, _) = filter_box(c, 8, 0);
            let app = c.view().expect_app();
            let ui = &app.ui().expect("the UI shell is up").ui;
            let image = ui
                .children(b)
                .into_iter()
                .next()
                .expect("the box has a picture child");
            let did = ui
                .node(image)
                .and_then(|n| n.region.image.as_ref().map(|g| g.did));
            (image, did)
        };
    let submitted =
        |c: &HeadlessClient, image: dereth_ui::ElemHandle, did: dereth_primitives::DataId| {
            c.view()
                .expect_app()
                .ui_draw_list()
                .iter()
                .any(|cmd| cmd.who == image && cmd.image == Some(did))
        };

    let (image, drawn) = box_media(&mut c);
    let partly = drawn == Some(some) && submitted(&c, image, some);

    // Any bit makes the box ticked, so one press turns the partly chosen group off.
    hands.click_handle(&mut c, button);
    let (_, off) = box_media(&mut c);
    let turned_off = !filter_box_drawn(&mut c, 8, 0) && off != Some(some);

    // The next press chooses the whole group, and the other picture is what is drawn.
    let (button, _) = filter_box(&mut c, 8, 0);
    hands.click_handle(&mut c, button);
    let (image, drawn) = box_media(&mut c);
    let wholly =
        filter_current(&mut c, 8) & mask == mask && drawn == Some(all) && submitted(&c, image, all);

    c.assert_behaviour(
        "options.chat-page.a-partly-chosen-group-and-a-wholly-chosen-one-are-drawn-differently",
        move |_| mask == MASK_COMBAT && two_pictures && partly && turned_off && wholly,
    );
    c.shutdown();
}

#[test]
fn scenario_a_partly_chosen_group_is_drawn_differently() {
    scenario("a_partly_chosen_group_is_drawn_differently");
}

/// Ticking a filter reaches the window that routes by it, and sends nothing.
pub fn ticking_a_filter_reaches_the_window_and_sends_nothing() {
    use dereth_primitives::ServerTime;

    let mut c = a_client_with_module(dereth_protocol::login::PlayerModule::default());
    let mut hands = Hands::new();
    open_the_chat_options_page(&mut c, &mut hands);
    let seen = c.outbound().len();

    let (_, mask) = filter_box(&mut c, 8, 0);
    let before = window_filter(&mut c, 8);
    let starts_clean = mask == MASK_COMBAT
        && filter_box_drawn(&mut c, 8, 0)
        && !c
            .view()
            .expect_app()
            .objects()
            .world
            .player_system
            .is_dirty();

    let i = filter_control(&mut c, 8);
    scroll_chat_control_into_view(&mut c, i);
    let (h, _) = filter_box(&mut c, 8, 0);
    hands.click_handle(&mut c, h);

    let want = before & !MASK_COMBAT;
    let reached = !filter_box_drawn(&mut c, 8, 0)
        && filter_current(&mut c, 8) == want
        // The chat window's own filter is what routing reads; the page's word is not.
        && window_filter(&mut c, 8) == want
        && module_filter(&c, 8) == Some(want)
        && c.view().expect_app().objects().world.player_system.is_dirty();

    // The other four windows did not move: the notice names the window it is about.
    let others_untouched = [2, 3, 4, 5]
        .into_iter()
        .all(|w| window_filter(&mut c, w) == dereth_ui_screens::chat::interface::default_filter(w));

    // Nothing went out: a chat option raises a notice and a flag, and no message at all.
    let quiet = c.outbound().len() == seen;

    // ...and the record that does eventually go carries the new filter.
    let packed = {
        let ps = &mut c.objects_mut().world.player_system;
        assert!(
            ps.use_time(ServerTime(10_000.0)),
            "the deferred flush fires"
        );
        ps.client_packed_module().expect("the packed record")
    };
    let carried = matches!(
        packed
            .gameplay_options
            .as_ref()
            .and_then(|g| g.properties.get(PROP_WINDOW_ARRAY)),
        Some(BasePropertyValue::Array(rows))
            if matches!(
                rows[7].value.as_ref(),
                Some(BasePropertyValue::Struct(fields))
                    if fields.get(PROP_FILTER) == Some(&BasePropertyValue::Bitfield64(want))
            )
    );

    // Turning it back on restores the group's bits rather than flipping the whole word.
    let (h, _) = filter_box(&mut c, 8, 0);
    hands.click_handle(&mut c, h);
    let restored = filter_box_drawn(&mut c, 8, 0) && window_filter(&mut c, 8) == before;

    c.assert_behaviour(
        "options.chat-page.ticking-a-filter-reaches-the-window-that-routes-by-it-and-sends-nothing",
        move |_| starts_clean && reached && others_untouched && quiet && carried && restored,
    );
    c.shutdown();
}

#[test]
fn scenario_ticking_a_filter_reaches_the_window_and_sends_nothing() {
    scenario("ticking_a_filter_reaches_the_window_and_sends_nothing");
}

/// The opacity slider writes what the fade reads, and the two stay in order.
pub fn the_opacity_slider_writes_what_the_fade_reads() {
    let mut c = a_client_with_module(dereth_protocol::login::PlayerModule::default());
    let mut hands = Hands::new();
    open_the_chat_options_page(&mut c, &mut hands);
    let seen = c.outbound().len();

    // Both windows start at the shipped fallback, which is what makes the fade's travel nothing.
    let starts_flat =
        window_opacity(&mut c, 8) == (1.0, 1.0) && module_opacity(&c, PROP_IDLE_OPACITY).is_none();

    let i = slider_control(&mut c, PROP_IDLE_OPACITY);
    scroll_chat_control_into_view(&mut c, i);
    let bar = slider_element(&mut c, PROP_IDLE_OPACITY);
    // A press a quarter of the way along the bar: the widget puts the thumb where the pointer is.
    let (x, y) = {
        let app = c.view().expect_app();
        let b = app.ui().expect("the UI shell is up").ui.screen_box(bar);
        (b.x0 + (b.x1 - b.x0) / 4, (b.y0 + b.y1) / 2)
    };
    hands.click_at(&mut c, x, y);

    let pos = {
        let app = c.view().expect_app();
        dereth_ui_screens::bind::attr_float(&app.ui().expect("shell").ui, bar, ATTR_POSITION)
            .unwrap_or(-1.0)
    };
    let v = slider_current(&mut c, PROP_IDLE_OPACITY);
    let moved = (0.0..=1.0).contains(&pos) && v < 0.5 && (v - pos).abs() < 1e-5;

    let stored = module_opacity(&c, PROP_IDLE_OPACITY).expect("the record took the new value");
    let written = (stored - v).abs() < 1e-6
        && c.view()
            .expect_app()
            .objects()
            .world
            .player_system
            .is_dirty();

    // ...and the fade's own source, on every window, because this setting is not per-window.
    let every_window = CHAT_WINDOWS.into_iter().all(|w| {
        let (idle, active) = window_opacity(&mut c, w);
        (idle - v).abs() < 1e-6 && (active - 1.0).abs() < 1e-6
    });
    let (idle, active) = window_opacity(&mut c, 8);
    let has_travel = active - idle > 0.1;
    let quiet = c.outbound().len() == seen;

    // The page keeps the two in order: dragging the active one below the idle one takes the idle
    // one down with it, and writes its setting too.
    let j = slider_control(&mut c, PROP_ACTIVE_OPACITY);
    scroll_chat_control_into_view(&mut c, j);
    let bar = slider_element(&mut c, PROP_ACTIVE_OPACITY);
    let (x, y) = {
        let app = c.view().expect_app();
        let b = app.ui().expect("the UI shell is up").ui.screen_box(bar);
        (b.x0 + 1, (b.y0 + b.y1) / 2)
    };
    hands.click_at(&mut c, x, y);
    let a = slider_current(&mut c, PROP_ACTIVE_OPACITY);
    let d = slider_current(&mut c, PROP_IDLE_OPACITY);
    let pulled_down = a <= v
        && (d - a).abs() < 1e-6
        && module_opacity(&c, PROP_IDLE_OPACITY).is_some_and(|s| (s - a).abs() < 1e-6);

    c.assert_behaviour("options.chat-page.the-opacity-slider-writes-what-the-fade-reads-and-keeps-the-two-in-order", move |_| {
        starts_flat && moved && written && every_window && has_travel && quiet && pulled_down
    });
    c.shutdown();
}

#[test]
fn scenario_the_opacity_slider_writes_what_the_fade_reads() {
    scenario("the_opacity_slider_writes_what_the_fade_reads");
}

/// Apply, cancel and restore-defaults do what an option page does.
pub fn apply_cancel_and_defaults_do_what_an_option_page_does() {
    use dereth_ui_screens::options::config::button;

    let mut c = a_client_with_module(dereth_protocol::login::PlayerModule::default());
    let mut hands = Hands::new();
    open_the_chat_options_page(&mut c, &mut hands);
    let seen = c.outbound().len();

    let snapshot = window_filter(&mut c, 8);
    let i = filter_control(&mut c, 8);
    scroll_chat_control_into_view(&mut c, i);

    // Cancel reverts an uncommitted change...
    let (h, _) = filter_box(&mut c, 8, 0);
    hands.click_handle(&mut c, h);
    let moved = window_filter(&mut c, 8) != snapshot
        && with_gameplay(&mut c, |_, s| s.chat_options.changed());
    let cancel = chat_page_button(&mut c, button::CANCEL);
    hands.click_handle(&mut c, cancel);
    let reverted = window_filter(&mut c, 8) == snapshot
        && module_filter(&c, 8) == Some(snapshot)
        && filter_box_drawn(&mut c, 8, 0)
        && !with_gameplay(&mut c, |_, s| s.chat_options.changed());

    // ...and a second cancel writes nothing, which is why leaving the page does not rewrite
    // every control on it.
    let before = c
        .view()
        .expect_app()
        .hud()
        .stats
        .chat_option_controls_reread;
    let cancel = chat_page_button(&mut c, button::CANCEL);
    hands.click_handle(&mut c, cancel);
    let nothing_to_revert = c
        .view()
        .expect_app()
        .hud()
        .stats
        .chat_option_controls_reread
        == before;

    // Apply takes a new snapshot, so the next cancel has nothing older to go back to.
    let (h, _) = filter_box(&mut c, 8, 0);
    hands.click_handle(&mut c, h);
    let after_click = window_filter(&mut c, 8);
    let apply = chat_page_button(&mut c, button::APPLY);
    hands.click_handle(&mut c, apply);
    let applied = window_filter(&mut c, 8) == after_click
        && !with_gameplay(&mut c, |_, s| s.chat_options.changed());
    let cancel = chat_page_button(&mut c, button::CANCEL);
    hands.click_handle(&mut c, cancel);
    let cancel_after_apply = window_filter(&mut c, 8) == after_click;

    // Restore-defaults is unconditional: a control nobody touched goes back too.
    let j = filter_control(&mut c, 2);
    scroll_chat_control_into_view(&mut c, j);
    let (h, _) = filter_box(&mut c, 2, 0);
    hands.click_handle(&mut c, h);
    let defaults = chat_page_button(&mut c, button::DEFAULTS);
    hands.click_handle(&mut c, defaults);
    let all_back = CHAT_WINDOWS.into_iter().all(|w| {
        let want = dereth_ui_screens::chat::interface::default_filter(w);
        filter_current(&mut c, w) == want
            && window_filter(&mut c, w) == want
            && module_filter(&c, w) == Some(want)
    }) && (slider_current(&mut c, PROP_IDLE_OPACITY) - 0.5).abs() < 1e-6
        && (slider_current(&mut c, PROP_ACTIVE_OPACITY) - 1.0).abs() < 1e-6
        && module_opacity(&c, PROP_IDLE_OPACITY) == Some(0.5)
        && module_opacity(&c, PROP_ACTIVE_OPACITY) == Some(1.0);

    // Nothing in the whole scenario put a byte on the wire.
    let quiet = c.outbound().len() == seen;

    c.assert_behaviour(
        "options.chat-page.apply-cancel-and-defaults-do-what-an-option-page-does",
        move |_| {
            moved
                && reverted
                && nothing_to_revert
                && applied
                && cancel_after_apply
                && all_back
                && quiet
        },
    );
    c.shutdown();
}

#[test]
fn scenario_apply_cancel_and_defaults_do_what_an_option_page_does() {
    scenario("apply_cancel_and_defaults_do_what_an_option_page_does");
}

// ---------------------------------------------------------------------------------------------
// chargen.exit.*
//
// The wizard's Exit button does something: it asks, in the shipped words, and a yes goes back to
// choosing a character.
// ---------------------------------------------------------------------------------------------

/// A client sitting on the character-creation wizard, offline.
fn a_client_on_the_wizard() -> HeadlessClient {
    let c = HeadlessClient::new(ClientSpec::screen(dereth_ui::framework::mode::CHAR_GEN, 8));
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .and_then(|u| u.flow.current_mode()),
        Some(dereth_ui::framework::mode::CHAR_GEN),
        "the wizard is the screen these scenarios are about"
    );
    c
}

/// How many roots the wizard owns -- its own, plus one per dialog it has raised.
fn wizard_roots(c: &mut HeadlessClient) -> usize {
    use dereth_ui::framework::Screen as _;
    with_wizard(c, |_, w| w.roots().len())
}

/// The wizard itself.
fn with_wizard<R>(
    c: &mut HeadlessClient,
    f: impl FnOnce(
        &mut dereth_ui::UiSystem,
        &mut dereth_ui_screens::screens::chargen::CharGenScreen,
    ) -> R,
) -> R {
    use dereth_ui::framework::Screen as _;

    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    let wizard = any
        .downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
        .expect("the wizard is current");
    f(&mut shell.ui, wizard)
}

/// Press a button of the wizard, the way the element manager delivers a press.
fn press_wizard_button(c: &mut HeadlessClient, id: ElementId) {
    let h = element(c, id);
    press_handle(c, h);
}

fn press_handle(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    c.tick(1);
}

/// A child of a dialog the wizard raised.
fn dialog_child(
    c: &mut HeadlessClient,
    dialog: dereth_ui::ElemHandle,
    id: ElementId,
) -> dereth_ui::ElemHandle {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(dialog, id)
        .expect("the dialog carries that child")
}

fn dialog_text(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> String {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// The warning as the shipped text table itself gives it, never a string written here.
fn the_shipped_exit_warning(c: &HeadlessClient) -> String {
    use dereth_ui_screens::screens::chargen::{ERROR_STRING_TABLE, EXIT_WARNING_STRING};
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        // The same hash of the same name the client itself looks the row up by.
        .resolve_string(
            ERROR_STRING_TABLE,
            dereth_ui::persist::preferences::token_of(EXIT_WARNING_STRING),
        )
        .expect("the warning is in the shipped text table")
}

/// The exit button raises a modal warning in the shipped words, and only one of it.
pub fn the_exit_button_raises_a_modal_warning() {
    use dereth_ui_screens::screens::chargen::{CharGenDialog, EXIT_BUTTON};

    let mut c = a_client_on_the_wizard();
    let nothing_up = with_wizard(&mut c, |_, w| w.exit_dialog).is_none();

    press_wizard_button(&mut c, EXIT_BUTTON);
    let dialog = with_wizard(&mut c, |_, w| w.exit_dialog).expect("the exit button raises one");
    let context = with_wizard(&mut c, |_, w| w.open_dialog) == Some(CharGenDialog::Exit);
    let modal = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(dialog)
        .expect("live")
        .region
        .flags
        .block_clicks;

    let want = the_shipped_exit_warning(&c);
    let body = dialog_child(&mut c, dialog, dereth_ui::dialog::base::child::TEXT);
    let says_the_right_thing = !want.is_empty() && dialog_text(&mut c, body) == want;

    // A second press builds nothing: the same dialog, and the wizard still owns two roots.
    let roots = wizard_roots(&mut c);
    press_wizard_button(&mut c, EXIT_BUTTON);
    let only_one = with_wizard(&mut c, |_, w| w.exit_dialog) == Some(dialog)
        && wizard_roots(&mut c) == roots
        && roots == 2;

    c.assert_behaviour(
        "chargen.exit.the-exit-button-raises-a-modal-warning-in-the-shipped-words",
        move |_| nothing_up && context && modal && says_the_right_thing && only_one,
    );
    c.shutdown();
}

#[test]
fn scenario_the_exit_button_raises_a_modal_warning() {
    scenario("the_exit_button_raises_a_modal_warning");
}

/// Saying yes goes back to choosing a character; saying no stays on the page.
pub fn yes_leaves_the_wizard_and_no_stays_on_the_page() {
    use dereth_ui::framework::mode;
    use dereth_ui_screens::screens::chargen::{EcgProgress, EXIT_BUTTON};

    // Yes.
    let mut c = a_client_on_the_wizard();
    press_wizard_button(&mut c, EXIT_BUTTON);
    let dialog = with_wizard(&mut c, |_, w| w.exit_dialog).expect("the confirmation");
    let yes = dialog_child(&mut c, dialog, dereth_ui::dialog::base::child::BUTTON1);
    press_handle(&mut c, yes);
    // The screen changes at the end of the frame the request drained in.
    c.tick(1);
    let left_for_character_select = c
        .view()
        .expect_app()
        .ui()
        .and_then(|u| u.flow.current_mode())
        == Some(mode::CHARACTER_MANAGEMENT);
    c.shutdown();

    // No -- on a page that is not the first, so "where it was" is something to see.
    let mut c = a_client_on_the_wizard();
    press_wizard_button(
        &mut c,
        EcgProgress::Town.select_button().expect("the town tab"),
    );
    let on_that_page = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Town;
    press_wizard_button(&mut c, EXIT_BUTTON);
    let dialog = with_wizard(&mut c, |_, w| w.exit_dialog).expect("the confirmation");
    let no = dialog_child(&mut c, dialog, dereth_ui::dialog::base::child::BUTTON2);
    press_handle(&mut c, no);
    c.tick(1);
    let stayed = c.view().expect_app().ui().and_then(|u| u.flow.current_mode())
        == Some(mode::CHAR_GEN)
        && with_wizard(&mut c, |_, w| w.exit_dialog).is_none()
        && with_wizard(&mut c, |_, w| w.open_dialog).is_none()
        // The dialog's root is deleted rather than orphaned.
        && wizard_roots(&mut c) == 1
        && with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Town;

    c.assert_behaviour(
        "chargen.exit.saying-yes-goes-back-to-choosing-a-character-and-saying-no-stays-on-the-page",
        move |_| left_for_character_select && on_that_page && stayed,
    );
    c.shutdown();
}

#[test]
fn scenario_yes_leaves_the_wizard_and_no_stays_on_the_page() {
    scenario("yes_leaves_the_wizard_and_no_stays_on_the_page");
}

/// The back arrow is exit on the first page and a step back on any other.
pub fn the_back_arrow_is_exit_only_on_the_first_page() {
    use dereth_ui::framework::mode;
    use dereth_ui_screens::screens::chargen::{EcgProgress, LEFT_BUTTON};

    let mut c = a_client_on_the_wizard();
    let on_the_first_page = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Hertage;

    press_wizard_button(&mut c, LEFT_BUTTON);
    let dialog = with_wizard(&mut c, |_, w| w.exit_dialog).expect("the back arrow raises it too");
    let want = the_shipped_exit_warning(&c);
    let body = dialog_child(&mut c, dialog, dereth_ui::dialog::base::child::TEXT);
    let same_warning = dialog_text(&mut c, body) == want;

    let yes = dialog_child(&mut c, dialog, dereth_ui::dialog::base::child::BUTTON1);
    press_handle(&mut c, yes);
    c.tick(1);
    let lands_the_same_place = c
        .view()
        .expect_app()
        .ui()
        .and_then(|u| u.flow.current_mode())
        == Some(mode::CHARACTER_MANAGEMENT);
    c.shutdown();

    // ...and past the first page it steps back and raises nothing.
    let mut c = a_client_on_the_wizard();
    press_wizard_button(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    press_wizard_button(&mut c, LEFT_BUTTON);
    let stepped_back = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Profession
        && with_wizard(&mut c, |_, w| w.exit_dialog).is_none();
    press_wizard_button(&mut c, LEFT_BUTTON);
    let stepped_again = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Hertage
        && with_wizard(&mut c, |_, w| w.exit_dialog).is_none();

    c.assert_behaviour(
        "chargen.exit.the-back-arrow-is-exit-on-the-first-page-and-a-step-back-on-any-other",
        move |_| {
            on_the_first_page
                && same_warning
                && lands_the_same_place
                && stepped_back
                && stepped_again
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_back_arrow_is_exit_only_on_the_first_page() {
    scenario("the_back_arrow_is_exit_only_on_the_first_page");
}

// ---------------------------------------------------------------------------------------------
// window.layout.*
//
// Where each window sits, and whether it is shown, is kept in the settings record and comes back
// when the screen is rebuilt.
// ---------------------------------------------------------------------------------------------

/// The window bag's own field numbers, as the settings record carries them.
const WINDOW_X: u32 = 0x1000_0086;
const WINDOW_Y: u32 = 0x1000_0087;
const WINDOW_VISIBLE: u32 = 0x1000_008A;
const WINDOW_ROW: u32 = 0x1000_008B;
const WINDOW_TITLE: u32 = 0x1000_008D;

/// The toolbar of the live gameplay screen, and the window number the screen gives it.
fn the_toolbar(c: &mut HeadlessClient) -> (dereth_ui::ElemHandle, u32) {
    with_gameplay(c, |ui, s| {
        (
            ui.get_child_recursive(
                s.root().expect("the gameplay root"),
                dereth_ui_screens::screens::gameplay::window::TOOLBAR,
            )
            .expect("the toolbar"),
            s.window_id_of(dereth_ui_screens::screens::gameplay::window::TOOLBAR),
        )
    })
}

/// A settings record whose window bag puts `window` at `x` and says whether it is shown, with an
/// unrelated window and an entry this client does not model beside it -- both of which have to
/// survive every write.
fn a_module_with_a_window(
    window: u32,
    x: i32,
    visible: bool,
) -> dereth_protocol::login::PlayerModule {
    use dereth_protocol::property::{
        BaseProperty, BasePropertyValue as V, PackObjPropertyCollection, PropertyCollection,
    };

    let field = |name: u32, value: V| {
        (
            name,
            BaseProperty {
                name,
                value: Some(value),
            },
        )
    };
    let mut rows = vec![
        BaseProperty {
            name: WINDOW_ROW,
            value: Some(V::Struct(PropertyCollection::default()))
        };
        window as usize
    ];
    rows[window as usize - 1].value = Some(V::Struct(PropertyCollection {
        bucket_index: 3,
        entries: vec![
            field(WINDOW_X, V::Integer(x)),
            field(WINDOW_Y, V::Integer(125)),
            field(WINDOW_VISIBLE, V::Bool(visible)),
            field(
                WINDOW_TITLE,
                V::StringInfo(dereth_protocol::property::StringInfo {
                    string_id: 1234,
                    ..dereth_protocol::property::StringInfo::default()
                }),
            ),
        ],
    }));
    rows.push(BaseProperty {
        name: WINDOW_ROW,
        value: Some(V::Struct(PropertyCollection {
            bucket_index: 2,
            entries: vec![field(WINDOW_X, V::Integer(-77))],
        })),
    });
    dereth_protocol::login::PlayerModule {
        gameplay_options: Some(PackObjPropertyCollection {
            version: 2,
            properties: PropertyCollection {
                bucket_index: 4,
                entries: vec![
                    field(0x1000_008C, V::Array(rows)),
                    (
                        0x0BAD_F00D,
                        BaseProperty {
                            name: WINDOW_X,
                            value: Some(V::Integer(42)),
                        },
                    ),
                ],
            },
        }),
        ..dereth_protocol::login::PlayerModule::default()
    }
}

/// Hand the client the shard's description of the character.
fn describe(c: &mut HeadlessClient, module: dereth_protocol::login::PlayerModule) {
    use dereth_protocol::login::LoginPlayerDescription;
    c.app_mut().apply_hud_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(
            LoginPlayerDescription {
                player_module: module,
                ..LoginPlayerDescription::default()
            },
        )),
    ]);
}

/// What the retained settings record says about one window.
fn kept_placement(
    c: &HeadlessClient,
    window: u32,
) -> dereth_ui_screens::hud::floaty::WindowPlacement {
    dereth_client::hud::decode_placements(
        c.view()
            .expect_app()
            .objects()
            .world
            .player_system
            .module
            .as_ref()
            .expect("a record"),
    )
    .get(window)
    .expect("that window's row")
    .clone()
}

/// Moving or hiding a window is remembered and comes back when the screen is rebuilt.
pub fn moving_a_window_is_remembered_across_a_rebuild() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let (h, id) = the_toolbar(&mut c);
    describe(&mut c, a_module_with_a_window(id, 101, true));
    c.tick(1);
    let sent_before = c.view().expect_app().interaction().stats.requests_sent;

    {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.move_to(h, 37, 69);
        // Deliberately far bigger than the layout allows, so what is remembered is what the
        // window really became and not what was asked for.
        ui.resize_to(h, 9999, 9999);
        ui.set_visible(h, false);
    }
    c.tick(1);

    let row = kept_placement(&c, id);
    let drawn = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(h)
        .expect("live")
        .region
        .box_;
    let remembered = (row.x, row.y) == (Some(37), Some(69))
        && (row.w, row.h) == (Some(drawn.width()), Some(drawn.height()))
        && row.visible == Some(false)
        && row.title
            == Some(dereth_ui_screens::view::ChatWindowTitle::Table {
                string_id: 1234,
                table_id: 0,
            });
    // An unrelated window's row and the entry this client does not model both came through.
    let others_survived = kept_placement(&c, id + 1).x == Some(-77);
    // ...and nothing was sent: moving a window is a local write, kept for the next save.
    let quiet = c.view().expect_app().interaction().stats.requests_sent == sent_before
        && c.view()
            .expect_app()
            .objects()
            .world
            .player_system
            .is_dirty();

    // Through the bytes the client would send, and back: the same placement.
    let round_trips = {
        let packed = c
            .view()
            .expect_app()
            .objects()
            .world
            .player_system
            .client_packed_module()
            .expect("the record");
        let mut w = dereth_protocol::Writer::new();
        packed.write(&mut w).expect("it encodes");
        let bytes = w.into_inner();
        let back =
            dereth_protocol::login::PlayerModule::read(&mut dereth_protocol::Reader::new(&bytes))
                .expect("it decodes");
        dereth_client::hud::decode_placements(&back).get(id) == Some(&row)
    };

    // And the screen rebuilt: the new toolbar comes up where the old one was left, hidden.
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(1);
    let gone = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(h)
        .is_none();
    let (new, _) = the_toolbar(&mut c);
    let came_back = {
        let app = c.view().expect_app();
        let n = app.ui().expect("shell").ui.node(new).expect("live");
        (n.region.box_.x0, n.region.box_.y0) == (37, 69) && !n.region.flags.visible
    };

    c.assert_behaviour("window.layout.moving-or-hiding-a-window-is-remembered-and-comes-back-when-the-screen-is-rebuilt", move |_| {
        remembered && others_survived && quiet && round_trips && gone && came_back
    });
    c.shutdown();
}

#[test]
fn scenario_moving_a_window_is_remembered_across_a_rebuild() {
    scenario("moving_a_window_is_remembered_across_a_rebuild");
}

/// A change made before the character is described survives, and an older description cannot undo
/// a newer change.
pub fn a_change_before_the_description_survives_it() {
    use dereth_ui_screens::view::UiRequest;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let (h, id) = the_toolbar(&mut c);
    // Nothing kept yet: a client with no description of its own does not quietly take the
    // defaults as if the shard had sent them.
    {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.move_to(h, 73, 79);
        ui.set_visible(h, false);
    }
    c.tick(1);
    let nothing_kept = c
        .view()
        .expect_app()
        .objects()
        .world
        .player_system
        .module
        .is_none()
        && !c
            .view()
            .expect_app()
            .objects()
            .world
            .player_system
            .is_dirty();

    // A move, then the description arriving, then another move: the last one wins, and the
    // requests the description's arrival passes through are not lost.
    c.ui_outbox().clear();
    c.app_mut().ui_mut().expect("shell").ui.move_to(h, 83, 89);
    let unrelated = vec![
        UiRequest::Select(dereth_primitives::ObjectId(123)),
        UiRequest::SetChatWindowOption {
            window: id,
            property: WINDOW_TITLE,
            value: 4321,
        },
    ];
    for r in unrelated.clone() {
        c.ui_outbox().emit(r);
    }
    describe(&mut c, a_module_with_a_window(id, 131, true));
    let preserved = c.ui_outbox().take();
    let kept_the_others = preserved
        .iter()
        .filter(|r| !dereth_ui_screens::requests::is_numeric_placement_update(r))
        .cloned()
        .collect::<Vec<_>>()
        == unrelated;
    for r in preserved {
        c.ui_outbox().emit(r);
    }
    // This move happens after the description and belongs to it; the older position must not
    // overwrite it.
    c.app_mut().ui_mut().expect("shell").ui.move_to(h, 41, 43);
    c.tick(1);
    let newest_wins = (kept_placement(&c, id).x, kept_placement(&c, id).y) == (Some(41), Some(43))
        && c.view()
            .expect_app()
            .ui()
            .expect("shell")
            .ui
            .node(h)
            .expect("live")
            .region
            .flags
            .visible;

    // A move made just before the screen is rebuilt is still a change, even though the very next
    // frame destroys the element it was made on.
    c.app_mut().ui_mut().expect("shell").ui.move_to(h, 59, 61);
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(1);
    let (new, _) = the_toolbar(&mut c);
    let survived_the_rebuild = {
        let app = c.view().expect_app();
        let b = app
            .ui()
            .expect("shell")
            .ui
            .node(new)
            .expect("live")
            .region
            .box_;
        (b.x0, b.y0) == (59, 61)
    } && (kept_placement(&c, id).x, kept_placement(&c, id).y)
        == (Some(59), Some(61));

    // ...and an older description arriving after a newer move does overwrite it, because it is
    // the shard's own answer and the move is not.
    {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.move_to(new, 83, 89);
        ui.set_visible(new, true);
    }
    describe(&mut c, a_module_with_a_window(id, 203, false));
    c.tick(1);
    let description_wins = {
        let app = c.view().expect_app();
        let n = app.ui().expect("shell").ui.node(new).expect("live");
        (n.region.box_.x0, n.region.box_.y0) == (203, 125) && !n.region.flags.visible
    } && kept_placement(&c, id).visible == Some(false);

    c.assert_behaviour(
        "window.layout.a-change-made-before-the-character-is-described-survives-it",
        move |_| {
            nothing_kept
                && kept_the_others
                && newest_wins
                && survived_the_rebuild
                && description_wins
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_change_before_the_description_survives_it() {
    scenario("a_change_before_the_description_survives_it");
}

/// A saved layout is pulled onto the screen and moves only the window it names.
pub fn a_saved_layout_is_clamped_and_moves_only_what_it_names() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let (h, id) = the_toolbar(&mut c);
    let (stack, stack_id) = with_gameplay(&mut c, |ui, s| {
        (
            ui.get_child_recursive(
                s.root().expect("the gameplay root"),
                dereth_ui_screens::screens::gameplay::window::PANEL_STACK,
            )
            .expect("the panel stack"),
            s.window_id_of(dereth_ui_screens::screens::gameplay::window::PANEL_STACK),
        )
    });

    // A record that places the panel stack somewhere of its own, so "only the window it names"
    // has something to be true of.
    let mut module = a_module_with_a_window(id, 101, true);
    {
        use dereth_protocol::property::{BaseProperty, BasePropertyValue as V, PropertyCollection};
        let Some(V::Array(rows)) = &mut module
            .gameplay_options
            .as_mut()
            .expect("the bag")
            .properties
            .entries[0]
            .1
            .value
        else {
            panic!("the window array")
        };
        rows[stack_id as usize - 1].value = Some(V::Struct(PropertyCollection {
            bucket_index: 0,
            entries: vec![
                (
                    WINDOW_X,
                    BaseProperty {
                        name: WINDOW_X,
                        value: Some(V::Integer(10)),
                    },
                ),
                (
                    WINDOW_Y,
                    BaseProperty {
                        name: WINDOW_Y,
                        value: Some(V::Integer(11)),
                    },
                ),
            ],
        }));
    }
    describe(&mut c, module);
    c.tick(1);
    {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.move_to(stack, 77, 91);
        ui.move_to(h, 37, 43);
    }
    c.tick(1);
    let moved_each_on_its_own = {
        let app = c.view().expect_app();
        let b = app
            .ui()
            .expect("shell")
            .ui
            .node(stack)
            .expect("live")
            .region
            .box_;
        (b.x0, b.y0) == (77, 91)
    } && (
        kept_placement(&c, stack_id).x,
        kept_placement(&c, stack_id).y,
    ) == (Some(10), Some(11));

    // A saved layout of this scenario's own -- never the player's file -- placing the toolbar
    // far off the screen.
    let saved = dereth_ui::persist::ScreenLayout::parse("<TBAR> X:9999 Y: 9999 W: 9999 H: 9999 ")
        .expect("the layout parses");
    let applied = with_gameplay(&mut c, |ui, s| s.load_screen_layout(ui, &saved));
    let clamped = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("shell").ui;
        let b = ui.node(h).expect("live").region.box_;
        let parent = ui
            .node(ui.parent(h).expect("a parent"))
            .expect("live")
            .region
            .box_;
        applied == 1
            && (b.x0, b.y0)
                == (
                    (parent.width() - b.width()).max(0),
                    (parent.height() - b.height()).max(0),
                )
    };
    c.tick(1);
    let kept = {
        let app = c.view().expect_app();
        let b = app
            .ui()
            .expect("shell")
            .ui
            .node(h)
            .expect("live")
            .region
            .box_;
        let row = kept_placement(&c, id);
        (row.x, row.y, row.w, row.h) == (Some(b.x0), Some(b.y0), Some(b.width()), Some(b.height()))
    };
    let source_untouched = saved.windows[0].1.x == 9999;

    // A smaller display pulls it back again, and that placement is kept too.
    c.app_mut().ui_mut().expect("shell").set_display((640, 480));
    let after = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(h)
        .expect("live")
        .region
        .box_;
    c.tick(1);
    let followed_the_display =
        (kept_placement(&c, id).x, kept_placement(&c, id).y) == (Some(after.x0), Some(after.y0));

    // ...and a rebuild reads what the window really became.
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(1);
    let (new, _) = the_toolbar(&mut c);
    let rebuilt_there = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(new)
        .expect("live")
        .region
        .box_
        == after;

    c.assert_behaviour(
        "window.layout.a-saved-layout-is-pulled-onto-the-screen-and-moves-only-the-window-it-names",
        move |_| {
            moved_each_on_its_own
                && clamped
                && kept
                && source_untouched
                && followed_the_display
                && rebuilt_there
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_saved_layout_is_clamped_and_moves_only_what_it_names() {
    scenario("a_saved_layout_is_clamped_and_moves_only_what_it_names");
}

// ---------------------------------------------------------------------------------------------
// intro.*
//
// Clicking the intro movie or a splash screen does nothing.
//
// The gestures go into the client's own input queue rather than through the pointer, and that is
// deliberate rather than a shortcut: the shipped intro layout holds a root and two pictures and
// **no button at all**, so there is nothing for a hit test to land on. The first scenario asserts
// that from the data, because otherwise "no hotspot was pressed" and "the press did nothing" are
// the same reading.
// ---------------------------------------------------------------------------------------------

/// A client on the intro sequence.
fn a_client_on_the_intro() -> HeadlessClient {
    let c = HeadlessClient::new(ClientSpec::screen(dereth_ui::framework::mode::INTRO, 3));
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .and_then(|u| u.flow.current_mode()),
        Some(dereth_ui::framework::mode::INTRO),
        "the intro is the screen these scenarios are about"
    );
    c
}

fn with_intro<R>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut dereth_ui_screens::screens::intro::IntroScreen) -> R,
) -> R {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    f(any
        .downcast_mut::<dereth_ui_screens::screens::intro::IntroScreen>()
        .expect("the intro screen is current"))
}

/// The picture the intro is showing, and the ones still queued behind it.
fn intro_state(c: &mut HeadlessClient) -> (Option<u32>, Vec<u32>) {
    with_intro(c, |s| (s.current_state, s.states.iter().copied().collect()))
}

fn current_screen(c: &HeadlessClient) -> Option<dereth_ui::UiMode> {
    c.view()
        .expect_app()
        .ui()
        .and_then(|u| u.flow.current_mode())
}

/// Put one action on the client's own input queue, on the map the pointer is bound in.
fn inject(c: &mut HeadlessClient, action: u32, start: bool) {
    let e = dereth_input::InputEvent {
        action: dereth_input::ActionId(action),
        input_map: dereth_client::ui::UI_INPUT_MAP,
        toggle: dereth_input::ToggleType::OneShot,
        extent: 1.0,
        start,
        repeat_delta: 1,
        repeat_total: 0,
        from_key_down: false,
    };
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .inject_action(e);
}

/// One character, through the message the client's own character gate reads, so all three of its
/// gates apply.
fn inject_character(c: &mut HeadlessClient, ch: u8) {
    let m = dereth_client::pump::Win32Message::new(
        dereth_input::win32::msg::WM_CHAR,
        ch as usize,
        0,
        0,
    );
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .on_message(m);
}

/// The left button, as the client's own table numbers it.
const LEFT_BUTTON_ACTION: u32 = 7;

/// A click advances one picture, and letting go is not a second one.
pub fn a_click_advances_one_picture_and_the_release_is_not_another() {
    use dereth_ui_screens::screens::intro;

    let mut c = a_client_on_the_intro();

    // The premise: the shipped layout is a root and two pictures, with no button in it -- so
    // nothing here can be a press on a hotspot.
    let ids = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the UI shell is up");
        let root = shell.flow.current().expect("a screen").roots()[0];
        let mut ids = Vec::new();
        let mut stack = vec![root];
        while let Some(h) = stack.pop() {
            ids.push(shell.ui.node(h).expect("live").element_id().0);
            stack.extend(shell.ui.children(h));
        }
        ids.sort_unstable();
        ids
    };
    let no_hotspot = ids.len() == 3;

    let (before, queued) = intro_state(&mut c);
    let starts_on_the_first =
        before == Some(intro::SHIPPED_STATES[0]) && queued == intro::SHIPPED_STATES[1..].to_vec();

    inject(&mut c, LEFT_BUTTON_ACTION, true);
    c.tick(1);
    let (after, still_queued) = intro_state(&mut c);
    let advanced_one = after == Some(intro::SHIPPED_STATES[1])
        && still_queued == intro::SHIPPED_STATES[2..].to_vec()
        && current_screen(&c) == Some(dereth_ui::framework::mode::INTRO)
        && c.view()
            .expect_app()
            .ui()
            .expect("shell")
            .stats
            .intro_actions
            == 1;
    c.shutdown();

    // A real click is a press and a release. If the release advanced too, one click would eat two
    // pictures and the intro would be half as long as it should be.
    let mut c = a_client_on_the_intro();
    inject(&mut c, LEFT_BUTTON_ACTION, true);
    inject(&mut c, LEFT_BUTTON_ACTION, false);
    c.tick(1);
    let one_advance = intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[1])
        && c.view()
            .expect_app()
            .ui()
            .expect("shell")
            .stats
            .intro_actions
            == 1;

    c.assert_behaviour(
        "intro.click.a-click-advances-one-picture-and-letting-go-is-not-another",
        move |_| no_hotspot && starts_on_the_first && advanced_one && one_advance,
    );
    c.shutdown();
}

#[test]
fn scenario_a_click_advances_one_picture_and_the_release_is_not_another() {
    scenario("a_click_advances_one_picture_and_the_release_is_not_another");
}

/// Clicking through the whole sequence ends at character select and not before.
pub fn clicking_through_the_intro_ends_at_character_select() {
    use dereth_ui::framework::mode;
    use dereth_ui_screens::screens::intro;

    let mut c = a_client_on_the_intro();
    let mut one_per_click = true;
    for expected in &intro::SHIPPED_STATES[1..] {
        inject(&mut c, LEFT_BUTTON_ACTION, true);
        c.tick(1);
        one_per_click &=
            intro_state(&mut c).0 == Some(*expected) && current_screen(&c) == Some(mode::INTRO);
    }
    // ...and the click after the last picture finds nothing left.
    inject(&mut c, LEFT_BUTTON_ACTION, true);
    c.tick(1);
    let ends_there = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);

    c.assert_behaviour(
        "intro.click.clicking-through-the-sequence-ends-at-character-select-and-not-before",
        move |_| one_per_click && ends_there && intro::SHIPPED_STATES.len() > 1,
    );
    c.shutdown();
}

#[test]
fn scenario_clicking_through_the_intro_ends_at_character_select() {
    scenario("clicking_through_the_intro_ends_at_character_select");
}

/// The quit action skips the rest of the intro where a click advances it.
pub fn the_quit_action_skips_the_rest_of_the_intro() {
    use dereth_ui::framework::mode;

    let mut c = a_client_on_the_intro();
    inject(&mut c, dereth_ui_screens::screens::intro::ACTION_QUIT, true);
    c.tick(1);
    let skipped = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);

    c.assert_behaviour(
        "intro.quit.the-quit-action-skips-the-rest-of-it",
        move |_| skipped,
    );
    c.shutdown();
}

#[test]
fn scenario_the_quit_action_skips_the_rest_of_the_intro() {
    scenario("the_quit_action_skips_the_rest_of_the_intro");
}

/// Any character advances one picture, and escape skips.
pub fn any_character_advances_and_escape_skips() {
    use dereth_ui::framework::mode;
    use dereth_ui_screens::screens::intro;

    let mut c = a_client_on_the_intro();
    // One frame first, so the client has opened the gate a character has to pass -- which is
    // what it has done by the time a player could press anything.
    c.tick(1);
    inject_character(&mut c, b'a');
    c.tick(1);
    let advanced = intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[1])
        && current_screen(&c) == Some(mode::INTRO)
        && c.view()
            .expect_app()
            .ui()
            .expect("shell")
            .stats
            .intro_characters
            == 1;
    c.shutdown();

    let mut c = a_client_on_the_intro();
    c.tick(1);
    inject_character(&mut c, 0x1B);
    c.tick(1);
    let escaped = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);

    c.assert_behaviour(
        "intro.keyboard.any-character-advances-one-picture-and-escape-skips",
        move |_| advanced && escaped,
    );
    c.shutdown();
}

#[test]
fn scenario_any_character_advances_and_escape_skips() {
    scenario("any_character_advances_and_escape_skips");
}

// ---------------------------------------------------------------------------------------------
// focus.press.*
//
// A press focuses every scrollable element, as retail does, and not only a text box.
//
// Each scenario presses **without letting go**, so the state the element is in at the moment the
// keyboard moved can be read; letting go and comparing with where it started would be reading an
// operation against its own inverse. The state numbers are written out rather than read back
// through the symbols the client writes them through.
// ---------------------------------------------------------------------------------------------

/// Whatever currently holds the keyboard.
fn what_holds_the_keyboard(c: &HeadlessClient) -> Option<dereth_ui::ElemHandle> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .focus_element()
}

/// The state an element is in.
fn state_of(c: &HeadlessClient, h: dereth_ui::ElemHandle) -> dereth_ui::StateId {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .state
}

/// The middle of an element's box.
fn middle_of(c: &HeadlessClient, h: dereth_ui::ElemHandle) -> (i32, i32) {
    let b = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// Press the left button at a point and run the frame that acts on it, with no release.
fn press_at(c: &mut HeadlessClient, hands: &mut Hands, x: i32, y: i32) {
    use dereth_client::platform::keys::MouseButton;
    hands.move_to(c, x, y);
    let m = hands.button_message(MouseButton::Left, true);
    hands.send(c, m);
    c.tick(1);
}

/// Let it go.
fn release(c: &mut HeadlessClient, hands: &mut Hands) {
    use dereth_client::platform::keys::MouseButton;
    let m = hands.button_message(MouseButton::Left, false);
    hands.send(c, m);
    c.tick(1);
}

/// The wizard on its skills page, which is where the shipped tree actually *shows* a list and its
/// bar: every panel carrying one on the gameplay screen starts hidden, and a hidden element
/// cannot be pressed.
fn a_wizard_on_the_skills_page() -> HeadlessClient {
    use dereth_ui_screens::screens::chargen::{EcgProgress, HERITAGE_BUTTONS, TOWN_BUTTONS};

    let mut c = HeadlessClient::new(ClientSpec::screen(dereth_ui::framework::mode::CHAR_GEN, 8));
    press_wizard_button(&mut c, HERITAGE_BUTTONS[0].0);
    press_wizard_button(&mut c, TOWN_BUTTONS[0].0);
    press_wizard_button(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    c
}

/// The skills list and the bar bound to it.
const SKILLS_LIST: ElementId = ElementId(0x1000_03F7);
const SKILLS_BAR: ElementId = ElementId(0x1000_03F8);

/// A press on a button takes the keyboard and the button keeps its own look.
pub fn a_press_on_a_button_takes_the_keyboard() {
    use dereth_ui::StateId;
    use dereth_ui_screens::chat::window::{ENTRY, SEND};

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut hands = Hands::new();
    let entry = element(&c, ENTRY);
    let send = element(&c, SEND);

    let (x, y) = middle_of(&c, entry);
    press_at(&mut c, &mut hands, x, y);
    release(&mut c, &mut hands);
    let entry_had_it =
        what_holds_the_keyboard(&c) == Some(entry) && state_of(&c, send) != StateId(3);

    let (x, y) = middle_of(&c, send);
    press_at(&mut c, &mut hands, x, y);
    let moved = what_holds_the_keyboard(&c) == Some(send);
    // A pressed button is in its own pressed state, never in the generic focused one -- what a
    // player sees of a button being pressed is the button, not a focus ring.
    let pressed = state_of(&c, send);
    let own_look = pressed == StateId(3) && pressed != StateId(4);

    release(&mut c, &mut hands);
    let kept_it = what_holds_the_keyboard(&c) == Some(send)
        && state_of(&c, send) != StateId(4)
        // Only now, having read the state during the press, is the end worth comparing with the
        // start: the picture goes back where it was even though the keyboard does not.
        && state_of(&c, send) == StateId(1);

    c.assert_behaviour(
        "focus.press.a-press-on-a-button-takes-the-keyboard-and-the-button-keeps-its-own-look",
        move |_| entry_had_it && moved && own_look && kept_it,
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_on_a_button_takes_the_keyboard() {
    scenario("a_press_on_a_button_takes_the_keyboard");
}

/// Typing stops when the keyboard leaves the entry.
pub fn typing_stops_when_the_keyboard_leaves_the_entry() {
    use dereth_ui_screens::chat::window::{ENTRY, SEND};

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut hands = Hands::new();
    let entry = element(&c, ENTRY);
    let send = element(&c, SEND);

    let (x, y) = middle_of(&c, entry);
    press_at(&mut c, &mut hands, x, y);
    release(&mut c, &mut hands);
    let typing = what_holds_the_keyboard(&c) == Some(entry)
        && c.app_mut().ui_mut().expect("shell").wants_text_mode();

    hands.type_text(&mut c, "abc");
    let arrived = c
        .app_mut()
        .ui_mut()
        .expect("shell")
        .ui
        .text_element_mut(entry)
        .expect("a text element")
        .glyphs
        .inq_text(false)
        == "abc";

    let (x, y) = middle_of(&c, send);
    press_at(&mut c, &mut hands, x, y);
    release(&mut c, &mut hands);
    let stopped = what_holds_the_keyboard(&c) == Some(send)
        && !c.app_mut().ui_mut().expect("shell").wants_text_mode();

    c.assert_behaviour(
        "focus.press.typing-stops-when-the-keyboard-leaves-the-entry",
        move |_| typing && arrived && stopped,
    );
    c.shutdown();
}

#[test]
fn scenario_typing_stops_when_the_keyboard_leaves_the_entry() {
    scenario("typing_stops_when_the_keyboard_leaves_the_entry");
}

/// A press on a list takes the keyboard, and the list's own look follows it.
pub fn a_press_on_a_list_takes_the_keyboard_and_its_look_follows() {
    use dereth_ui::StateId;

    let mut c = a_wizard_on_the_skills_page();
    let mut hands = Hands::new();
    let list = element(&c, SKILLS_LIST);
    let (x, y) = middle_of(&c, list);
    let reachable = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .hit_test_screen(x, y)
        == Some(list);
    let nothing_holds_it = what_holds_the_keyboard(&c).is_none();

    let before = state_of(&c, list);
    // Which states this list declares is layout data, so what it lands in is derived from the
    // layout; what is written out here is the **rule** -- the change fires only from these three.
    let declares_focused = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(list)
        .expect("live")
        .desc
        .access_state(StateId(4))
        .is_some();

    press_at(&mut c, &mut hands, x, y);
    let took_it = what_holds_the_keyboard(&c) == Some(list);
    let after = state_of(&c, list);
    let look_followed = if matches!(before.0, 0 | 1 | 5) {
        after
            == if declares_focused {
                StateId(4)
            } else {
                StateId(0)
            }
    } else {
        after == before
    };

    c.assert_behaviour(
        "focus.press.a-press-on-a-list-takes-the-keyboard-and-its-look-follows",
        move |_| reachable && nothing_holds_it && took_it && look_followed,
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_on_a_list_takes_the_keyboard_and_its_look_follows() {
    scenario("a_press_on_a_list_takes_the_keyboard_and_its_look_follows");
}

/// A press on a scrollbar takes the keyboard too.
///
/// **What this does not say.** It says nothing about the bar's position, and the position does
/// move: a press in the middle of the track, above or below the thumb, pages the list, which is
/// what a track press is for. The claim here is only that the keyboard moves; the paging is
/// `chargen.scroll.a-press-on-the-track-moves-a-whole-page-towards-the-press`.
pub fn a_press_on_a_scrollbar_takes_the_keyboard_without_moving_it() {
    let mut c = a_wizard_on_the_skills_page();
    let mut hands = Hands::new();
    let bar = element(&c, SKILLS_BAR);
    // The middle of the track, not an arrow: an arrow would step the bar, which is a different
    // question from the one this asks.
    let (x, y) = middle_of(&c, bar);
    let hit = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .hit_test_screen(x, y);
    let reachable = hit.is_some();
    press_at(&mut c, &mut hands, x, y);
    let hit = hit.expect("the pointer lands on something");
    let took_it = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("shell").ui;
        (hit == bar || ui.is_ancestor_of(bar, hit))
            && ui.focus_element() == Some(hit)
            && ui.takes_focus_on_press(hit)
    };
    c.assert_behaviour(
        "focus.press.a-press-on-a-scrollbar-takes-the-keyboard-too",
        move |_| reachable && took_it,
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_on_a_scrollbar_takes_the_keyboard_without_moving_it() {
    scenario("a_press_on_a_scrollbar_takes_the_keyboard_without_moving_it");
}

/// A press on something that cannot scroll moves the keyboard nowhere.
pub fn a_press_on_something_that_cannot_scroll_moves_the_keyboard_nowhere() {
    use dereth_ui::ElementType;
    use dereth_ui_screens::chat::window::ENTRY;

    /// The first element of `ty` the hit test really answers when aimed at its own middle.
    ///
    /// Found by walking rather than by naming an id, because which panels a fresh gameplay screen
    /// shows is shipped data; `None` rather than a panic, so the caller can say what it could not
    /// find and a helper that failed is not mistaken for the claim failing.
    fn first_reachable(c: &HeadlessClient, ty: ElementType) -> Option<dereth_ui::ElemHandle> {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        ui.element_list().iter().copied().find(|h| {
            if ui.node(*h).map(dereth_ui::element::ElementNode::ty) != Some(ty) {
                return false;
            }
            let b = ui.screen_box(*h);
            if b.x1 < b.x0 || b.y1 < b.y0 {
                return false;
            }
            ui.hit_test_screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(*h)
        })
    }

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut hands = Hands::new();
    let entry = element(&c, ENTRY);
    let (x, y) = middle_of(&c, entry);
    press_at(&mut c, &mut hands, x, y);
    release(&mut c, &mut hands);
    let entry_has_it = what_holds_the_keyboard(&c) == Some(entry);

    // Every kind of element that is not in the scrolling family and that the shipped gameplay
    // tree actually shows, by its own type number.
    let outside: [u32; 8] = [0x02, 0x03, 0x07, 0x08, 0x09, 0x0D, 0x10, 0x11];
    let mut tried = 0usize;
    let mut none_took_it = true;
    for ty in outside {
        let Some(h) = first_reachable(&c, ElementType(ty)) else {
            continue;
        };
        tried += 1;
        let (x, y) = middle_of(&c, h);
        press_at(&mut c, &mut hands, x, y);
        none_took_it &=
            what_holds_the_keyboard(&c) != Some(h) && what_holds_the_keyboard(&c) == Some(entry);
        release(&mut c, &mut hands);
    }

    c.assert_behaviour(
        "focus.press.a-press-on-something-that-cannot-scroll-moves-the-keyboard-nowhere",
        move |_| {
            // The denominator: a loop that pressed nothing would pass in silence.
            entry_has_it && tried > 0 && none_took_it
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_on_something_that_cannot_scroll_moves_the_keyboard_nowhere() {
    scenario("a_press_on_something_that_cannot_scroll_moves_the_keyboard_nowhere");
}

// ---------------------------------------------------------------------------------------------
// text-entry.focus, the screen's own side of it
//
// The client's switch for "the player is typing" must not follow the keyboard one frame late: if it
// did, the first character after a *screen* took the keyboard would be destroyed rather than
// delayed -- the client decides whether to keep a character at the moment the message arrives.
//
// Each half is here with its opposite. Asserting only that a character arrives passes on a client
// that always accepts one; asserting only that one is lost passes on a client that never opens the
// gate at all, which is the defect itself.
//
// The shipped prompt, read out of the text table, is what the first typed character has to replace;
// that is asserted in the first scenario below as a fixture guard rather than as a row of its own.
// ---------------------------------------------------------------------------------------------

/// The wizard's name box, and the two buttons these scenarios press.
const ALUVIAN_BULLET: ElementId = ElementId(0x1000_03BF);
const SUMMARY_TAB: ElementId = ElementId(0x1000_03F4);

/// The wizard with a heritage settled and **nothing** holding the keyboard, which is where the
/// direction that must lose a character is asserted from.
fn a_wizard_with_a_heritage() -> HeadlessClient {
    let mut c = a_client_on_the_wizard();
    press_wizard_button(&mut c, ALUVIAN_BULLET);
    c.tick(1);
    c
}

/// How many characters the shipped text elements have actually been handed -- the denominator
/// without which "the box is empty" and "nothing was ever offered" are the same reading.
fn characters_handed_over(c: &HeadlessClient) -> u64 {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .characters_delivered
}

/// How many times the client has thrown the switch that says the player is typing.
fn typing_switch_edges(c: &HeadlessClient) -> u64 {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .text_mode_edges
}

fn box_text(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> String {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// A character typed the moment a screen takes the keyboard is not lost.
pub fn a_character_typed_the_moment_a_screen_takes_the_keyboard_is_not_lost() {
    use dereth_ui_screens::screens::chargen::{ERROR_STRING_TABLE, NAME_FIELD, NAME_PROMPT};

    let mut c = a_wizard_with_a_heritage();
    let mut hands = Hands::new();

    // ---- the direction that must LOSE the character ------------------------------------------
    // Nothing on this page can be typed into, so nothing holds the keyboard.
    let nothing_holds_it = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        .is_none();
    let edges_before = typing_switch_edges(&c);
    let offered_at = characters_handed_over(&c);
    // One character and **no frame between**: whatever the last frame left the switch at is what
    // decides, which is the whole of this claim.
    hands.character(&mut c, 'Q');
    c.tick(1);
    let lost = characters_handed_over(&c) == offered_at && typing_switch_edges(&c) == edges_before;

    // ---- the frame in which the screen takes the keyboard -------------------------------------
    press_wizard_button(&mut c, SUMMARY_TAB);
    let name = element(&c, NAME_FIELD);
    let took_it = c.view().expect_app().ui().expect("shell").ui.focus_element() == Some(name)
        // Exactly one throw of the switch: on the change, not on every frame.
        && typing_switch_edges(&c) == edges_before + 1;
    // The prompt the first typed character has to replace, read out of the shipped text table
    // rather than written here.
    let prompt = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .resolve_string(
            ERROR_STRING_TABLE,
            dereth_ui::persist::preferences::token_of(NAME_PROMPT),
        )
        .expect("the prompt is in the shipped text table");
    let prompt_is_there = !prompt.is_empty();

    // ---- the direction that must KEEP it, with no frame in between ----------------------------
    hands.character(&mut c, 'Z');
    c.tick(1);
    let kept = characters_handed_over(&c) == offered_at + 1
        // ...and it needed no further throw of the switch to get there: the switch was already
        // over when the message arrived, which is what deciding at message time requires. A
        // client that followed the keyboard a frame late would throw it here, and the character
        // would already be gone.
        && typing_switch_edges(&c) == edges_before + 1
        && box_text(&mut c, name) == "Z";
    let the_wizard_has_it = with_wizard(&mut c, |_, w| w.state.name.clone()) == "Z"
        && with_wizard(&mut c, |_, w| w.name_entered);

    c.assert_behaviour(
        "text-entry.focus.a-character-typed-the-moment-a-screen-takes-the-keyboard-is-not-lost",
        move |_| {
            nothing_holds_it && lost && took_it && prompt_is_there && kept && the_wizard_has_it
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_character_typed_the_moment_a_screen_takes_the_keyboard_is_not_lost() {
    scenario("a_character_typed_the_moment_a_screen_takes_the_keyboard_is_not_lost");
}

/// The same when the screen takes the keyboard in its own per-frame pass.
pub fn a_screen_that_takes_the_keyboard_in_its_own_pass_keeps_the_next_character() {
    use dereth_ui_screens::screens::chargen::NAME_FIELD;

    let mut c = a_wizard_with_a_heritage();
    let mut hands = Hands::new();
    press_wizard_button(&mut c, SUMMARY_TAB);
    let name = element(&c, NAME_FIELD);
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .expect("shell")
            .ui
            .focus_element(),
        Some(name),
        "the summary page took the keyboard"
    );

    // Take it away again, so the reading below cannot pass on the keyboard the last frame had.
    c.app_mut()
        .ui_mut()
        .expect("shell")
        .ui
        .relinquish_focus(name);
    c.tick(1);
    let given_up = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        .is_none();
    let offered_at = characters_handed_over(&c);

    // The control: a character offered now goes nowhere.
    hands.character(&mut c, 'X');
    c.tick(1);
    let lost = characters_handed_over(&c) == offered_at;

    // The screen's own per-frame pass takes it back -- a different step of the frame from the one
    // above, and the other half of "in its own update or in answering a message".
    with_wizard(&mut c, |_, w| w.pending_refresh = true);
    c.tick(1);
    let took_it_back = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        == Some(name);

    hands.character(&mut c, 'Y');
    c.tick(1);
    let kept = characters_handed_over(&c) == offered_at + 1 && box_text(&mut c, name) == "Y";

    c.assert_behaviour("text-entry.focus.a-screen-that-takes-the-keyboard-in-its-own-pass-keeps-the-next-character", move |_| {
        given_up && lost && took_it_back && kept
    });
    c.shutdown();
}

#[test]
fn scenario_a_screen_that_takes_the_keyboard_in_its_own_pass_keeps_the_next_character() {
    scenario("a_screen_that_takes_the_keyboard_in_its_own_pass_keeps_the_next_character");
}

/// The switch follows a box that can be typed into and nothing else, once per change.
pub fn the_typing_switch_follows_an_editable_box_and_nothing_else() {
    use dereth_ui_screens::screens::chargen::NAME_FIELD;

    let mut c = a_wizard_with_a_heritage();
    let nothing_focused = !c.app_mut().ui_mut().expect("shell").wants_text_mode();

    let edges_before = typing_switch_edges(&c);
    press_wizard_button(&mut c, SUMMARY_TAB);
    let name = element(&c, NAME_FIELD);
    let one_edge = typing_switch_edges(&c) == edges_before + 1;

    let editable_focus = {
        let shell = c.app_mut().ui_mut().expect("shell");
        shell.ui.focus_element() == Some(name)
            && shell
                .ui
                .text_element_mut(name)
                .expect("a text element")
                .bits
                .editable()
            && shell.wants_text_mode()
    };
    // The third answer, which a yes-or-no about "something is focused" gets wrong: a box that can
    // be picked at but not typed into is not a place characters go.
    let not_editable = {
        let shell = c.app_mut().ui_mut().expect("shell");
        let t = shell.ui.text_element_mut(name).expect("a text element");
        t.bits.set_editable(false);
        t.bits.set_selectable(true);
        let answer = !shell.wants_text_mode();
        shell
            .ui
            .text_element_mut(name)
            .expect("a text element")
            .bits
            .set_editable(true);
        answer && shell.wants_text_mode()
    };

    // Frames with nothing changing throw the switch no further times -- it is thrown on the
    // change, and a client that threw it every frame would be re-arming a latch the client arms
    // once.
    c.tick(8);
    let quiet = typing_switch_edges(&c) == edges_before + 1;

    // ...and once more on the way back.
    c.app_mut()
        .ui_mut()
        .expect("shell")
        .ui
        .relinquish_focus(name);
    c.tick(1);
    let back = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        .is_none()
        && typing_switch_edges(&c) == edges_before + 2;

    c.assert_behaviour("text-entry.focus.the-typing-switch-follows-a-box-that-can-be-typed-into-and-is-thrown-once-per-change", move |_| {
        nothing_focused && one_edge && editable_focus && not_editable && quiet && back
    });
    c.shutdown();
}

#[test]
fn scenario_the_typing_switch_follows_an_editable_box_and_nothing_else() {
    scenario("the_typing_switch_follows_an_editable_box_and_nothing_else");
}

// =============================================================================================
// The shell with no client under it
//
// `ClientSpec::shell(host)` is a backend and not a subject: the UI shell over the retail dats,
// driven against a `HostState` the scenario writes, with no `App` at all. Its one row keeps the
// `shell-only` id.
//
// The three tests at the end are **harness self-proofs and not census rows**: they make no claim
// about the client, they prove three pieces of the harness do what they say. `goldens.rs` is the
// precedent for a `dat` test that is not a behaviour.
// =============================================================================================

/// Three characters in an order that is neither alphabetical nor the one the scenario picks, so
/// that each of the list's three fallbacks would choose a **different** row: the char-gen slot is
/// unset, the remembered pick is the third name, and the first live row in the shard's own order
/// is the first. If the fallback and the answer were the same row this could not fail.
fn a_character_set() -> dereth_ui::persist::CharacterSet {
    let named = |gid: u32, name: &str| dereth_protocol::login::CharacterIdentity {
        gid: dereth_primitives::ObjectId(gid),
        name: name.into(),
        seconds_greyed_out: 0,
    };
    dereth_client::ui::character_set_from_login(&dereth_protocol::login::LoginCharacterSet {
        status: 0,
        characters: vec![
            named(0x5000_0001, "Zoranth"),
            named(0x5000_0002, "Ailinn"),
            named(0x5000_0003, "Borumar"),
        ],
        deleted: vec![],
        num_allowed_characters: 5,
        account: "acct0001".into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 0,
    })
}

/// What the application would be telling the shell: connected, patched, and here is the list.
fn a_host_at_character_select() -> HostState {
    HostState {
        has_packet_controller: true,
        connected: true,
        patch_finished: true,
        received_set: true,
        character_set: Some(a_character_set()),
        character_set_notices: 1,
        ..HostState::default()
    }
}

fn with_charmgmt<T>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut CharacterManagementScreen) -> T,
) -> T {
    let shell = c.expect_shell();
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    f(any
        .downcast_mut::<CharacterManagementScreen>()
        .expect("the character-management screen"))
}

/// The name of the row the list is currently highlighting.
fn selected_name(c: &mut HeadlessClient) -> Option<String> {
    with_charmgmt(c, |s| s.selected_row().map(|r| r.name.clone()))
}

/// Click the row carrying `name` -- through the row **element**, which is what a player's pointer
/// lands on, and not through the screen's own `select_character`.
fn click_the_row(c: &mut HeadlessClient, name: &str) {
    let row = with_charmgmt(c, |s| {
        s.rows
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("{name} is in the rebuilt list"))
            .element
            .unwrap_or_else(|| panic!("{name}'s row has an element"))
    });
    c.expect_shell().ui.broadcast_element_message(
        row,
        dereth_ui::msg::element::id::BUTTON_CLICKED,
        0,
        0,
    );
    c.tick(2);
}

// -------------------------------------------------------------------------------------------
// shell-only.character-select.the-returning-list-selects-the-character-just-played
// -------------------------------------------------------------------------------------------

/// The whole trip, as one scenario: the list comes up on its own fallback, the player picks
/// somebody else, the world comes up and throws the whole framework away, and the log-off brings
/// the list back.
pub fn the_returning_character_list_selects_the_character_that_was_just_played() {
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::CHARACTER_MANAGEMENT, 30),
    );
    c.tick(2);

    // The premise, asserted so the ending is a measurement and not a coincidence: with nothing
    // remembered the list falls back to the first live row in the shard's own order.
    assert_eq!(
        selected_name(&mut c).as_deref(),
        Some("Zoranth"),
        "the list's last fallback is the first live row in the order the shard sent"
    );

    click_the_row(&mut c, "Borumar");
    assert_eq!(
        selected_name(&mut c).as_deref(),
        Some("Borumar"),
        "the click selected a row"
    );

    // Into the world. The mode switch destroys the current screen, so anything the *screen*
    // remembered is gone from here on.
    c.host_mut().in_world = true;
    c.tick(2);
    assert_eq!(
        c.expect_shell().flow.current_mode(),
        Some(mode::GAME_PLAY),
        "in the world"
    );

    // The log-off, which the shard answers with a second, identical character set. **This is the
    // step that needs this backend**: the set has not changed, so only the notice count says
    // anything happened, and an `App` has no way to be told either.
    c.host_mut().in_world = false;
    c.host_mut().character_set_notices = 2;
    c.tick(4);
    assert_eq!(
        c.expect_shell().flow.current_mode(),
        Some(mode::CHARACTER_MANAGEMENT),
        "back at character select"
    );

    let selected = selected_name(&mut c);
    c.assert_behaviour(
        "shell-only.character-select.the-returning-list-selects-the-character-just-played",
        move |_| selected.as_deref() == Some("Borumar"),
    );
}

#[test]
fn scenario_the_returning_character_list_selects_the_character_that_was_just_played() {
    scenario("the_returning_character_list_selects_the_character_that_was_just_played");
}

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

// =============================================================================================
// window.full-screen.* -- the three that need a running client
//
// The other full-screen claims are pure arithmetic over a desktop the scenario makes up, and they
// are in the `cpu` tier's `shell.rs`; these three are here because each of them is about what a
// *client* does with the setting, and a client opens the dats.
//
// **Each owns its `App` outright**, through `adapters_shell::AppSpec` / `build_app`, for two
// reasons the harness cannot give: `ClientSpec` has no way to say what the player's saved
// full-screen setting is -- `AppSpec::full_screen` is -- and the device shadow these read is
// `App::device_state`, which is not on `HeadlessClient`. Both are gaps in the harness.
// =============================================================================================

/// The setting is kept from the start and applied on entering the world, both ways.
pub fn the_full_screen_setting_is_kept_and_applied_on_entering_the_world() {
    // It is read out of the player's own saved settings file in the first place.
    let read_from_the_file = {
        let prefs = dereth_client::config::Preferences::parse("[Display]\r\nFullScreen=True\r\n");
        let mut cfg = dereth_client::config::Config::default();
        cfg.display.full_screen = false;
        cfg.apply_preferences(&prefs);
        cfg.display.full_screen
    };

    // Asked for, and still a window: the patch screen and the character list are windowed.
    let app = build_app(&AppSpec {
        shell: true,
        full_screen: true,
        ..AppSpec::default()
    });
    // (a fresh App's UI starts with an empty request queue of its own)
    let kept = app.config().display.full_screen;
    let windowed_at_the_start = !app.device_state().full_screen;
    let _ = app.shutdown();

    // Entering the world applies it...
    let mut app = build_app(&AppSpec {
        full_screen: true,
        ..AppSpec::in_gameplay(4)
    });
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .clear();
    let in_the_world = app.device_state().full_screen;

    // ...and leaving takes it away again, which is the same edge in the other direction.
    app.queue_ui_mode(mode::CHARACTER_MANAGEMENT);
    for _ in 0..4 {
        assert!(
            app.frame(),
            "the client shut itself down on the way out of the world"
        );
    }
    let back_to_a_window = !app.device_state().full_screen;
    let setting_survived = app.config().display.full_screen;
    let _ = app.shutdown();

    // And a client that never asked for it is a window in the world too, which is what makes the
    // reading above a measurement.
    let off = build_app(&AppSpec::in_gameplay(4));
    // (a fresh App's UI starts with an empty request queue of its own)
    let never = !off.device_state().full_screen;
    let _ = off.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.full-screen.the-setting-is-kept-from-the-start-and-applied-on-entering-the-world",
        move |_| {
            read_from_the_file
                && kept
                && windowed_at_the_start
                && in_the_world
                && back_to_a_window
                && setting_survived
                && never
        },
    );
}

#[test]
fn scenario_the_full_screen_setting_is_kept_and_applied_on_entering_the_world() {
    scenario("the_full_screen_setting_is_kept_and_applied_on_entering_the_world");
}

/// The switch key does nothing at the character list and switches both ways in the world.
///
/// The gate it is refused by is the client's own, and it is read off a **real** client's device
/// shadow in each of the two places -- the copy is taken from the running client and then driven
/// through the same two calls the window loop makes, which is the only way to press a key that
/// the desktop, and not the client's input manager, owns.
pub fn the_full_screen_switch_key_is_refused_outside_the_world() {
    use dereth_client::pump::window_proc::{finish_event_loop, msg, wnd_proc};

    let app = build_app(&AppSpec {
        shell: true,
        full_screen: true,
        ..AppSpec::default()
    });
    // (a fresh App's UI starts with an empty request queue of its own)
    let mut outside = *app.device_state();
    let _ = app.shutdown();
    outside.is_active_app = true;
    let gate_shut = !outside.allow_full_screen_mode;
    let _ = wnd_proc(&mut outside, msg::WM_SYSKEYDOWN, msg::VK_RETURN, 0);
    // The latch still latches -- the refusal is at the gate and not at the keyboard, which is
    // what "refused rather than honoured and quietly undone" means here.
    let latched = outside.toggle_full_screen_mode;
    finish_event_loop(&mut outside, true);
    let refused = !outside.full_screen && !outside.toggle_full_screen_mode;

    let app = build_app(&AppSpec {
        full_screen: true,
        ..AppSpec::in_gameplay(4)
    });
    // (a fresh App's UI starts with an empty request queue of its own)
    let mut inside = *app.device_state();
    let _ = app.shutdown();
    inside.is_active_app = true;
    let gate_open = inside.allow_full_screen_mode && inside.full_screen;

    let _ = wnd_proc(&mut inside, msg::WM_SYSKEYDOWN, msg::VK_RETURN, 0);
    finish_event_loop(&mut inside, true);
    let turned_off = !inside.full_screen;
    let _ = wnd_proc(&mut inside, msg::WM_SYSKEYDOWN, msg::VK_RETURN, 0);
    finish_event_loop(&mut inside, true);
    let and_back_on = inside.full_screen;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.full-screen.the-switch-key-is-refused-outside-the-world-and-works-inside-it",
        move |_| gate_shut && latched && refused && gate_open && turned_off && and_back_on,
    );
}

#[test]
fn scenario_the_full_screen_switch_key_is_refused_outside_the_world() {
    scenario("the_full_screen_switch_key_is_refused_outside_the_world");
}

/// The options page's own tick reaches the window on the next frame, both ways.
pub fn the_options_page_turns_full_screen_on_and_off_mid_session() {
    use dereth_ui_screens::{PrefValue, UiRequest as ScreenRequest};

    let mut app = build_app(&AppSpec::in_gameplay(4));
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .clear();
    let starts_windowed = !app.device_state().full_screen;

    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(ScreenRequest::SetPreference(
            "Display.FullScreen",
            PrefValue::Bool(true),
        ));
    assert!(
        app.frame(),
        "the client shut itself down on the option's own frame"
    );
    let on = app.device_state().full_screen;
    // ...and it is kept for the next time the client starts, which is the half a device-only
    // reading would miss.
    let saved = app.config().display.full_screen;

    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(ScreenRequest::SetPreference(
            "Display.FullScreen",
            PrefValue::Bool(false),
        ));
    assert!(
        app.frame(),
        "the client shut itself down on the option's own frame"
    );
    let off = !app.device_state().full_screen;
    let _ = app.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.full-screen.the-options-page-turns-it-on-and-off-while-the-player-plays",
        move |_| starts_windowed && on && saved && off,
    );
}

#[test]
fn scenario_the_options_page_turns_full_screen_on_and_off_mid_session() {
    scenario("the_options_page_turns_full_screen_on_and_off_mid_session");
}

// =============================================================================================
// chargen.* -- the wizard working, page by page
//
// A character can be created end to end. The left arrow is **not** a row here: what it does is
// already `chargen.exit.the-back-arrow-is-exit-on-the-first-page-and-a-step-back-on-any-other`, and
// a second scenario asserting the same id would be a second scenario for one claim.
//
// The bring-up is the exit scenarios' `a_client_on_the_wizard` and `press_wizard_button`; what is
// added here is `walk_the_wizard`, a player's walk through the wizard -- a heritage, a profession,
// a town, the summary tab and a name typed into the box.
// =============================================================================================

use dereth_chargen::Attr;
use dereth_ui_screens::screens::chargen::{
    self as chargen, EcgProgress, ATTRIBUTE_SLIDERS, FINISH_BUTTON, LEFT_BUTTON, RIGHT_BUTTON,
    STATE_TAB_OFF, STATE_TAB_ON,
};

/// Press a wizard button and run the frame that drains the screen's action queue -- a click.
fn click_wizard(c: &mut HeadlessClient, id: ElementId) {
    press_wizard_button(c, id);
}

/// The same press with **no** frame after it, for the two claims whose subject is what the press
/// put on the screen's own queue before anything drained it.
fn press_only(c: &mut HeadlessClient, id: ElementId) {
    let h = element(c, id);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
}

/// The wizard's own text of an element, as the glyphs it composed.
fn wizard_text(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> String {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| {
            String::from_utf16_lossy(&t.glyphs.glyphs.iter().map(|g| g.data).collect::<Vec<_>>())
        })
}

fn wizard_text_by_id(c: &mut HeadlessClient, id: ElementId) -> String {
    let h = element(c, id);
    wizard_text(c, h)
}

fn wizard_state(c: &HeadlessClient, id: ElementId) -> dereth_ui::StateId {
    state_of(c, element(c, id))
}

fn wizard_visible(c: &HeadlessClient, id: ElementId) -> bool {
    let h = element(c, id);
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .region
        .flags
        .visible
}

/// A word out of the shipped text table, by the same name the client looks it up by.
fn shipped_word(c: &HeadlessClient, token: &str) -> String {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .resolve_string(
            chargen::ERROR_STRING_TABLE,
            dereth_ui::persist::preferences::token_of(token),
        )
        .unwrap_or_else(|| panic!("{token} is in the shipped text table"))
}

/// Every line the summary list shows, in list order, with a row's two columns joined by a tab.
fn summary_lines(c: &mut HeadlessClient) -> Vec<String> {
    let rows = with_wizard(c, |_, w| w.summary_rows.clone());
    let mut out = Vec::new();
    for r in rows {
        let cells: Vec<dereth_ui::ElemHandle> = {
            let shell = c.view().expect_app().ui().expect("the UI shell is up");
            [
                chargen::summary_page::LINE_TEXT,
                chargen::summary_page::HEADER_TEXT,
                chargen::summary_page::PAIR_NAME,
                chargen::summary_page::PAIR_VALUE,
            ]
            .iter()
            .filter_map(|id| shell.ui.get_child_recursive(r, *id))
            .collect()
        };
        let parts: Vec<String> = cells
            .into_iter()
            .map(|h| wizard_text(c, h))
            .filter(|s| !s.is_empty())
            .collect();
        out.push(parts.join("\t"));
    }
    out
}

/// The player's own walk: an Aluvian Soldier out of Holtburg, named, on the summary page.
fn walk_the_wizard(c: &mut HeadlessClient) {
    click_wizard(c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        c,
        EcgProgress::Profession
            .select_button()
            .expect("the profession tab"),
    );
    click_wizard(c, chargen::PROFESSION_BUTTONS[5].0);
    click_wizard(c, EcgProgress::Town.select_button().expect("the town tab"));
    click_wizard(c, chargen::TOWN_BUTTONS[0].0);
    click_wizard(
        c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    let name = element(c, chargen::NAME_FIELD);
    {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        shell
            .ui
            .text_element_mut(name)
            .expect("the name box is a text element")
            .set_text("Tarinell");
        shell
            .ui
            .broadcast_element_message(name, dereth_ui::MessageId(0x44), 0, 0);
    }
    c.tick(1);
}

/// What the wizard has asked the host to do, drained without an application frame -- which is how
/// a claim about *what the press composed* is made on a client with no shard to send it to.
fn chargen_actions(
    c: &mut HeadlessClient,
    now: f64,
) -> Vec<dereth_ui_screens::screens::chargen::CharGenAction> {
    let host = c.view().expect_app().host_state().clone();
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell.frame(
        dereth_primitives::LocalTime(now),
        &host,
        &mut dereth_ui::NullInputPump,
    );
    shell.take_chargen_actions()
}

// ---------------------------------------------------------------------------------------------
// chargen.summary.lists-every-choice-the-wizard-has-made-and-what-they-came-to
// ---------------------------------------------------------------------------------------------

/// The summary list, line by line, and the instruction pane beside it.
pub fn the_summary_page_lists_every_choice_and_what_it_came_to() {
    let mut c = a_client_on_the_wizard();
    walk_the_wizard(&mut c);
    assert_eq!(with_wizard(&mut c, |_, w| w.progress), EcgProgress::Summary);

    let lines = summary_lines(&mut c);
    assert!(!lines.is_empty(), "the summary list has rows at all");
    let choices = lines[0] == "Profession: Soldier"
        && lines[1] == "Gender: Male"
        && lines[2] == "Heritage: Aluvian"
        && lines[3] == "Starting Town: Holtburg"
        && lines[4] == "Attributes";

    // The ten numbered rows. The values are read back off the model, so what this states is the
    // *shape* the page must have; the numbers themselves are pinned by the create-request row.
    let (s, e, co, q, f, sf, credits) = with_wizard(&mut c, |_, w| {
        (
            w.state.get(Attr::Strength),
            w.state.get(Attr::Endurance),
            w.state.get(Attr::Coordination),
            w.state.get(Attr::Quickness),
            w.state.get(Attr::Focus),
            w.state.get(Attr::Self_),
            w.state.remaining_skill_credits,
        )
    });
    let want = [
        format!("Strength\t{s}"),
        format!("Endurance\t{e}"),
        format!("Coordination\t{co}"),
        format!("Quickness\t{q}"),
        format!("Focus\t{f}"),
        format!("Self\t{sf}"),
        format!("Health\t{}", e / 2),
        format!("Stamina\t{e}"),
        format!("Mana\t{sf}"),
        format!("Skill Credits\t{credits}"),
    ];
    let numbers = lines[5..15] == want[..];

    let sections = chargen::SUMMARY_SKILL_SECTIONS
        .iter()
        .all(|(header, _)| lines.iter().any(|l| l == header));
    let spec_at = lines
        .iter()
        .position(|l| l == "Specialized Skills")
        .expect("the header");
    let trained_at = lines
        .iter()
        .position(|l| l == "Trained Skills")
        .expect("the header");
    let specialised = &lines[spec_at + 1..trained_at];
    // Four specialised skills, each with a score -- so none of them is the nothing a missing
    // formula would have printed.
    let skills_scored = specialised.len() >= 4
        && specialised.iter().all(|row| {
            row.split_once('\t')
                .and_then(|(_, score)| score.parse::<i32>().ok())
                .is_some_and(|score| score > 0)
        });

    let how_to = wizard_text_by_id(&mut c, chargen::summary_page::HOW_TO);
    let instructions = how_to.contains("A summary of the choices made so far")
        && how_to.contains("Aluvian")
        && how_to.len() > 600;

    let named = with_wizard(&mut c, |_, w| w.state.name.clone()) == "Tarinell";

    c.assert_behaviour(
        "chargen.summary.lists-every-choice-the-wizard-has-made-and-what-they-came-to",
        move |_| choices && numbers && sections && skills_scored && instructions && named,
    );
    c.shutdown();
}

#[test]
fn scenario_the_summary_page_lists_every_choice_and_what_it_came_to() {
    scenario("the_summary_page_lists_every_choice_and_what_it_came_to");
}

// ---------------------------------------------------------------------------------------------
// chargen.finish.replaces-the-forward-arrow-on-the-last-page-and-does-nothing-anywhere-else
// ---------------------------------------------------------------------------------------------

/// FINISH and the forward arrow swap places on the last page, on every page, and the button is
/// dead everywhere else -- which a layout-only reading would miss.
pub fn finish_replaces_the_forward_arrow_on_the_last_page_alone() {
    let mut c = a_client_on_the_wizard();
    let mut swapped_everywhere = true;
    let mut dead_elsewhere = true;
    let mut refuses_for_its_own_reason = false;

    for page in EcgProgress::PAGES {
        click_wizard(&mut c, page.select_button().expect("a real page has a tab"));
        let last = page == EcgProgress::Summary;
        swapped_everywhere &=
            wizard_visible(&c, FINISH_BUTTON) == last && wizard_visible(&c, RIGHT_BUTTON) == !last;

        press_only(&mut c, FINISH_BUTTON);
        let actions = chargen_actions(&mut c, 1.0);
        let error = with_wizard(&mut c, |_, w| w.error_string_id);
        if last {
            // On the last page it refuses for a reason of its own -- no name has been typed --
            // which is the create path's first line and not the page gate.
            refuses_for_its_own_reason =
                actions.is_empty() && error == Some("ID_CharGen_NoNameWarning");
        } else {
            dead_elsewhere &= actions.is_empty() && error.is_none();
        }
        with_wizard(&mut c, |_, w| w.open_dialog = None);
    }

    c.assert_behaviour(
        "chargen.finish.replaces-the-forward-arrow-on-the-last-page-and-does-nothing-anywhere-else",
        move |_| swapped_everywhere && dead_elsewhere && refuses_for_its_own_reason,
    );
    c.shutdown();
}

#[test]
fn scenario_finish_replaces_the_forward_arrow_on_the_last_page_alone() {
    scenario("finish_replaces_the_forward_arrow_on_the_last_page_alone");
}

// ---------------------------------------------------------------------------------------------
// chargen.finish.composes-the-character-the-player-built-and-only-once
// ---------------------------------------------------------------------------------------------

/// What pressing FINISH would send, field by field, and the guard against sending it twice.
pub fn finish_composes_the_character_the_player_built_and_only_once() {
    use dereth_assets::Decode as _;
    use dereth_primitives::{AssetSource as _, DataId};

    let mut c = a_client_on_the_wizard();
    walk_the_wizard(&mut c);

    // The shipped tables the build is checked against, opened from the client's own store.
    let (cg, skills) = {
        let store = c.dat_store().expect("a retail client has a store").clone();
        let cg_bytes = store
            .read(DataId(0x0E00_0002))
            .expect("the character-creation table");
        let cg = dereth_assets::tables::CharGen::decode_payload(DataId(0x0E00_0002), &cg_bytes)
            .expect("it decodes");
        let sk_bytes = store.read(DataId(0x0E00_0004)).expect("the skill table");
        let sk = dereth_assets::tables::SkillTable::decode_payload(DataId(0x0E00_0004), &sk_bytes)
            .expect("it decodes");
        (cg, sk)
    };

    let (heritage, gender, template, start_area, attrs, levels) = with_wizard(&mut c, |_, w| {
        (
            w.state.heritage_group,
            w.state.gender,
            w.state.template,
            w.state.start_area,
            [
                w.state.get(Attr::Strength),
                w.state.get(Attr::Endurance),
                w.state.get(Attr::Coordination),
                w.state.get(Attr::Quickness),
                w.state.get(Attr::Focus),
                w.state.get(Attr::Self_),
            ],
            w.state.skill_levels.clone(),
        )
    });
    let walked_where_it_was_told = heritage == 1 && start_area == 0 && template == 6;

    // The press. Unspent credits raise a warning first, and answering yes re-enters the create
    // path -- which is the client's own route and not a shortcut around it.
    press_only(&mut c, FINISH_BUTTON);
    with_wizard(&mut c, |_, w| {
        if w.open_dialog.is_some() {
            w.close_dialog(true);
        }
    });
    let actions = chargen_actions(&mut c, 2.0);
    let [dereth_ui_screens::screens::chargen::CharGenAction::SendCharGenResult(result)] =
        actions.as_slice()
    else {
        panic!("FINISH did not ask to create anything: {actions:?}");
    };

    let identity = result.name == "Tarinell"
        && result.heritage_group == heritage
        && result.gender == gender
        && result.start_area == start_area
        && result.template_num == template;
    let six = [
        result.strength,
        result.endurance,
        result.coordination,
        result.quickness,
        result.focus,
        result.self_,
    ] == attrs;
    let whole_budget = attrs.iter().sum::<i32>()
        == i32::try_from(cg.heritage_groups[&heritage].attribute_credits).expect("it fits");

    let every_skill = result.skill_advancement_classes.len() == levels.len()
        && levels
            .iter()
            .enumerate()
            .all(|(id, want)| result.skill_advancement_classes[id] == *want as i32);

    let mut used = 0;
    let mut trained = 0;
    let mut every_one_is_real = true;
    for (id, sac) in result.skill_advancement_classes.iter().enumerate() {
        if *sac < 2 {
            continue;
        }
        trained += 1;
        let id = u32::try_from(id).expect("a skill id fits");
        every_one_is_real &= skills.skills.contains_key(&id);
        let (t, s) = dereth_chargen::CharGenState::skill_costs(&cg, &skills, heritage, id);
        used += if *sac == 3 { s } else { t };
    }
    let affordable = trained >= 8
        && every_one_is_real
        && used <= i32::try_from(cg.heritage_groups[&heritage].skill_credits).expect("it fits");

    let sex = &cg.heritage_groups[&heritage].sexes[&gender];
    #[allow(clippy::cast_sign_loss)]
    let looks = result.hair_style >= 0
        && (result.hair_style as usize) < sex.hair_styles.len()
        && result.hair_color >= 0
        && (result.hair_color as usize) < sex.hair_colors.len()
        && result.eye_color >= 0
        && (result.eye_color as usize) < sex.eye_colors.len()
        && result.eyes_strip >= 0
        && result.nose_strip >= 0
        && result.mouth_strip >= 0
        && result.is_admin == 0
        && result.is_envoy == 0;

    // And it survives the trip on to the wire and back, with the client's own checksum.
    let mut msg = dereth_client::app::chargen_result_to_wire(result);
    msg.checksum_value = msg.checksum();
    let m = dereth_protocol::login::CharacterSendCharGenResult {
        account: "ac01".into(),
        result: msg.clone(),
    };
    let body = dereth_protocol::write_body(&m).expect("the create message encodes");
    let back =
        dereth_protocol::read_body::<dereth_protocol::login::CharacterSendCharGenResult>(&body)
            .expect("and decodes again");
    let round_trips = back.result == msg && back.result.checksum_value == back.result.checksum();

    // A second press creates nothing, which is what keeps a double click from making two
    // characters.
    press_only(&mut c, FINISH_BUTTON);
    let only_once = chargen_actions(&mut c, 3.0).is_empty();

    c.assert_behaviour(
        "chargen.finish.composes-the-character-the-player-built-and-only-once",
        move |_| {
            walked_where_it_was_told
                && identity
                && six
                && whole_budget
                && every_skill
                && affordable
                && looks
                && round_trips
                && only_once
        },
    );
    c.shutdown();
}

#[test]
fn scenario_finish_composes_the_character_the_player_built_and_only_once() {
    scenario("finish_composes_the_character_the_player_built_and_only_once");
}

// ---------------------------------------------------------------------------------------------
// chargen.heritage.one-bullet-is-lit-and-the-page-behind-it-describes-that-people
// ---------------------------------------------------------------------------------------------

/// Which heritage bullets are lit, in shipped-layout order.
fn lit_heritages(c: &HeadlessClient) -> Vec<ElementId> {
    chargen::HERITAGE_BUTTONS
        .iter()
        .filter(|(id, _)| wizard_state(c, *id) == chargen::STATE_PROFESSION_ON)
        .map(|(id, _)| *id)
        .collect()
}

/// Exactly one bullet is lit, and the page behind it is that people's.
pub fn choosing_a_heritage_lights_its_bullet_and_describes_that_people() {
    let mut c = a_client_on_the_wizard();

    // The wizard opens on a rolled heritage with that bullet already lit, which is the state a
    // player finds the page in before touching anything.
    let opened_on = with_wizard(&mut c, |_, w| w.state.heritage_group);
    let rolled = opened_on != 0;
    let want = chargen::HERITAGE_BUTTONS
        .iter()
        .find(|(_, h)| *h == opened_on)
        .map(|(id, _)| *id)
        .expect("the rolled heritage has a bullet");
    let opens_lit = lit_heritages(&c) == vec![want];

    let mut one_lit = true;
    let mut picture_follows = true;
    let mut described = true;
    for (button, heritage) in [
        (chargen::HERITAGE_BUTTONS[0].0, 1_u32),
        (chargen::HERITAGE_BUTTONS[2].0, 3),
        (chargen::HERITAGE_BUTTONS[7].0, 8),
    ] {
        click_wizard(&mut c, button);
        one_lit &= with_wizard(&mut c, |_, w| w.state.heritage_group) == heritage
            && lit_heritages(&c) == vec![button];

        let (_, want_state, bonus, desc) = chargen::HERITAGE_PAGE
            .iter()
            .find(|(h, _, _, _)| *h == heritage)
            .copied()
            .expect("every heritage has a row");
        picture_follows &= wizard_state(&c, chargen::heritage_page::BACKGROUND) == want_state;

        let pane = wizard_text_by_id(&mut c, chargen::heritage_page::TEXT);
        described &= !pane.is_empty()
            && [
                chargen::HERITAGE_SKILLS_HEADER,
                chargen::HERITAGE_SKILLS_BODY,
                bonus,
                desc,
            ]
            .iter()
            .all(|token| pane.contains(shipped_word(&c, token).trim_end()));
    }

    c.assert_behaviour(
        "chargen.heritage.one-bullet-is-lit-and-the-page-behind-it-describes-that-people",
        move |_| rolled && opens_lit && one_lit && picture_follows && described,
    );
    c.shutdown();
}

#[test]
fn scenario_choosing_a_heritage_lights_its_bullet_and_describes_that_people() {
    scenario("choosing_a_heritage_lights_its_bullet_and_describes_that_people");
}

// ---------------------------------------------------------------------------------------------
// chargen.town.one-pin-is-lit-and-the-page-is-titled-and-described-for-that-town
// ---------------------------------------------------------------------------------------------

/// Exactly one map pin is lit, the page is titled with the town's name, and the pane describes it.
pub fn choosing_a_town_lights_its_pin_and_titles_the_page() {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        &mut c,
        EcgProgress::Town.select_button().expect("the town tab"),
    );

    let mut one_pin = true;
    let mut titled = true;
    let mut described = true;
    for (button, area) in [
        (chargen::TOWN_BUTTONS[0].0, 0_usize),
        (chargen::TOWN_BUTTONS[2].0, 2),
        (chargen::TOWN_BUTTONS[1].0, 1),
    ] {
        click_wizard(&mut c, button);
        let lit: Vec<ElementId> = chargen::TOWN_BUTTONS
            .iter()
            .filter(|(id, _)| wizard_state(&c, *id) == chargen::town_page::PIN_ON)
            .map(|(id, _)| *id)
            .collect();
        one_pin &=
            with_wizard(&mut c, |_, w| w.state.start_area) as usize == area && lit == vec![button];

        titled &= wizard_state(&c, chargen::town_page::TITLE)
            == chargen::town_page::TITLE_STATES[area]
            && wizard_text_by_id(&mut c, chargen::town_page::TITLE) == chargen::TOWN_NAMES[area];

        let pane = wizard_text_by_id(&mut c, chargen::town_page::TEXT);
        let body = shipped_word(&c, chargen::town_page::TEXT_TOKENS[area]);
        let frame = shipped_word(&c, chargen::town_page::HOW_TO);
        described &= pane == format!("{body}\n\n{frame}\n");
    }

    c.assert_behaviour(
        "chargen.town.one-pin-is-lit-and-the-page-is-titled-and-described-for-that-town",
        move |_| one_pin && titled && described,
    );
    c.shutdown();
}

#[test]
fn scenario_choosing_a_town_lights_its_pin_and_titles_the_page() {
    scenario("choosing_a_town_lights_its_pin_and_titles_the_page");
}

// ---------------------------------------------------------------------------------------------
// chargen.tabs.the-page-the-player-is-on-is-the-only-bright-one
// ---------------------------------------------------------------------------------------------

/// One tab is bright, and it is the page the wizard is on -- from every page.
pub fn the_tab_strip_brightens_only_the_page_it_is_on() {
    let mut c = a_client_on_the_wizard();
    let mut only_that_one = true;
    for page in EcgProgress::PAGES {
        click_wizard(&mut c, page.select_button().expect("a real page has a tab"));
        for other in EcgProgress::PAGES {
            let id = other.select_button().expect("a real page has a tab");
            let want = if other == page {
                STATE_TAB_ON
            } else {
                STATE_TAB_OFF
            };
            only_that_one &= wizard_state(&c, id) == want;
        }
    }

    c.assert_behaviour(
        "chargen.tabs.the-page-the-player-is-on-is-the-only-bright-one",
        move |_| only_that_one,
    );
    c.shutdown();
}

#[test]
fn scenario_the_tab_strip_brightens_only_the_page_it_is_on() {
    scenario("the_tab_strip_brightens_only_the_page_it_is_on");
}

// ---------------------------------------------------------------------------------------------
// chargen.profession.the-six-sliders-are-named-and-sit-where-their-numbers-say
// ---------------------------------------------------------------------------------------------

/// Each slider carries its attribute's name, its number, and a thumb at that number.
pub fn the_six_attribute_sliders_are_named_and_sit_at_their_values() {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        &mut c,
        EcgProgress::Profession
            .select_button()
            .expect("the profession tab"),
    );
    click_wizard(&mut c, chargen::PROFESSION_BUTTONS[5].0);

    let mut named = true;
    let mut numbered = true;
    let mut thumbs = true;
    for (field, attr) in ATTRIBUTE_SLIDERS {
        let f = element(&c, field);
        let (name, value, bar) = {
            let shell = c.view().expect_app().ui().expect("the UI shell is up");
            (
                shell
                    .ui
                    .get_child_recursive(f, chargen::slider::NAME)
                    .expect("the caption"),
                shell
                    .ui
                    .get_child_recursive(f, chargen::slider::VALUE)
                    .expect("the number"),
                shell
                    .ui
                    .get_child_recursive(f, chargen::slider::SCROLL)
                    .expect("the bar"),
            )
        };
        named &= wizard_text(&mut c, name) == chargen::attribute_name(attr);
        let want = with_wizard(&mut c, |_, w| w.state.get(attr));
        numbered &= wizard_text(&mut c, value) == want.to_string();

        let pos = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(bar)
            .expect("a live node")
            .merged_properties()
            .get(chargen::ATTR_SCROLL_POSITION)
            .and_then(|v| match v {
                dereth_assets::ui::PropertyValue::Float(f) => Some(*f),
                _ => None,
            })
            .expect("the bar carries a position");
        #[allow(clippy::cast_precision_loss)]
        let expected = want as f32 * 0.01;
        thumbs &= (pos - expected).abs() < 1e-5 && pos > 0.0;
    }

    // A Soldier is not six of the same number, so the sliders really do differ from one another.
    let values: Vec<i32> = ATTRIBUTE_SLIDERS
        .iter()
        .map(|(_, a)| with_wizard(&mut c, |_, w| w.state.get(*a)))
        .collect();
    let a_real_build = values.iter().any(|v| *v != values[0]);

    c.assert_behaviour(
        "chargen.profession.the-six-sliders-are-named-and-sit-where-their-numbers-say",
        move |_| named && numbered && thumbs && a_real_build,
    );
    c.shutdown();
}

#[test]
fn scenario_the_six_attribute_sliders_are_named_and_sit_at_their_values() {
    scenario("the_six_attribute_sliders_are_named_and_sit_at_their_values");
}

// ---------------------------------------------------------------------------------------------
// chargen.summary.the-name-box-prompts-takes-the-keyboard-and-the-first-character-replaces-the-prompt
// ---------------------------------------------------------------------------------------------

/// The name box on the last page prompts, takes the keyboard by itself, and what is typed
/// replaces the prompt rather than being typed on top of it.
pub fn the_name_box_prompts_takes_the_keyboard_and_keeps_what_replaced_the_prompt() {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );

    let name = element(&c, chargen::NAME_FIELD);
    let took_the_keyboard = what_holds_the_keyboard(&c) == Some(name);
    let prompt = shipped_word(&c, chargen::NAME_PROMPT);
    let prompts =
        wizard_text(&mut c, name) == prompt && !with_wizard(&mut c, |_, w| w.name_entered);
    // **The whole prompt is selected**, which is what makes the first character typed replace it
    // rather than join it -- and it is the selection the client's own reader answers with, not a
    // pair of endpoints left over from some earlier gesture.
    let selected = {
        let t = c
            .app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .text_element_mut(name)
            .expect("a text element");
        let whole = t.get_selection() == Some((0, prompt.chars().count()));
        // Selected, and not by a press that is still held: those are two different bits.
        whole && t.bits.selecting() && !t.bits.selection_from_press()
    };

    // No click first: the page has already given the box the keyboard, so a player can simply
    // type, and the whole prompt is selected so the first character replaces it.
    {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        for ch in "Tarinell".encode_utf16() {
            shell.ui.character(ch);
        }
        shell
            .ui
            .broadcast_element_message(name, dereth_ui::MessageId(0x44), 0, 0);
    }
    c.tick(1);
    let replaced = with_wizard(&mut c, |_, w| w.name_entered)
        && with_wizard(&mut c, |_, w| w.state.name.clone()) == "Tarinell"
        && wizard_text(&mut c, name) == "Tarinell"
        // ...and the selection went with the prompt it covered.
        && c.app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .text_element_mut(name)
            .and_then(|t| t.get_selection())
            .is_none();

    // And leaving the page and coming back does not put the prompt over the top of it.
    click_wizard(
        &mut c,
        EcgProgress::Hertage
            .select_button()
            .expect("the heritage tab"),
    );
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    let kept = wizard_text(&mut c, name) == "Tarinell";

    c.assert_behaviour("chargen.summary.the-name-box-prompts-takes-the-keyboard-and-the-first-character-replaces-the-prompt", move |_| {
        took_the_keyboard && prompts && selected && replaced && kept
    });
    c.shutdown();
}

#[test]
fn scenario_the_name_box_prompts_takes_the_keyboard_and_keeps_what_replaced_the_prompt() {
    scenario("the_name_box_prompts_takes_the_keyboard_and_keeps_what_replaced_the_prompt");
}

// ---------------------------------------------------------------------------------------------
// chargen.finish.its-caption-is-drawn-in-the-font-the-shipped-layout-names
// ---------------------------------------------------------------------------------------------

/// The FINISH button has a caption, in the font the layout names, and that font really draws it.
pub fn the_finish_buttons_caption_is_drawn_in_the_font_the_layout_names() {
    let mut c = a_client_on_the_wizard();
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    let shown = wizard_visible(&c, FINISH_BUTTON);
    let h = element(&c, FINISH_BUTTON);
    let font = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("a live node")
        .merged_properties()
        .get(dereth_ui::props::attr::TEXT_FONT_DID)
        .and_then(|v| match v {
            dereth_assets::ui::PropertyValue::Array(a) => match a.first().map(|e| &e.value) {
                Some(dereth_assets::ui::PropertyValue::DataFile(d)) => Some(*d),
                _ => None,
            },
            _ => None,
        });
    let names_a_font = font.is_some();
    let captioned = !wizard_text(&mut c, h).is_empty();

    // ...and the font draws the letters of the caption. It is the font's own sheet that a caption
    // is drawn from, and a sheet must not be refused for being too tall.
    let store = c.dat_store().expect("a retail client has a store").clone();
    let atlas = dereth_client::ui_draw::build_font_atlas(&store, font.expect("a font"))
        .expect("the caption's font rasterises");
    let legible = "FINISH".chars().all(|ch| atlas.glyph(ch).is_some());

    c.assert_behaviour(
        "chargen.finish.its-caption-is-drawn-in-the-font-the-shipped-layout-names",
        move |_| shown && names_a_font && captioned && legible,
    );
    c.shutdown();
}

#[test]
fn scenario_the_finish_buttons_caption_is_drawn_in_the_font_the_layout_names() {
    scenario("the_finish_buttons_caption_is_drawn_in_the_font_the_layout_names");
}

// ---------------------------------------------------------------------------------------------
// The left arrow: **not a row here.** Walking back a page, and raising the exit warning from the
// first, is exactly what
// `chargen.exit.the-back-arrow-is-exit-on-the-first-page-and-a-step-back-on-any-other` says and
// what the exit scenario asserts.
// ---------------------------------------------------------------------------------------------

// =============================================================================================
// chargen.random.* and chargen.appearance.* -- the rolled character and the colour wheel
//
// The roll written out as its own call list, and the two picture operations' arithmetic pinned
// against literals, would be transcriptions and are not rows. Two runs rolling the same character,
// the stream position after a roll, and the draw that does not draw are legs of the scenarios below
// rather than rows of their own.
//
// **No generator is named twice.** This crate does not depend on `dereth_primitives::num` and must
// not gain an edge for a test, so the oracle is a second `CharGenRng` seeded the same way -- which
// is the client's own pair of streams and the very thing the claim is about.
// =============================================================================================

use dereth_chargen::{CharGenRng, CharGenState, HERITAGE_OLTHOI, HERITAGE_OLTHOI_ACID};
use dereth_ui_screens::screens::chargen::CharGenDialog;

/// Everything the opening roll writes, as one comparable value.
#[derive(Debug, Clone, PartialEq)]
struct Roll {
    heritage: u32,
    gender: u32,
    start_area: u32,
    template: i32,
    face: [i32; 6],
    gear: [i32; 8],
    shades: [u64; 6],
}

fn roll_of(s: &CharGenState) -> Roll {
    Roll {
        heritage: s.heritage_group,
        gender: s.gender,
        start_area: s.start_area,
        template: s.template,
        face: [
            s.eyes_strip,
            s.nose_strip,
            s.mouth_strip,
            s.hair_color,
            s.eye_color,
            s.hair_style,
        ],
        gear: [
            s.headgear_style,
            s.headgear_color,
            s.shirt_style,
            s.shirt_color,
            s.trousers_style,
            s.trousers_color,
            s.footwear_style,
            s.footwear_color,
        ],
        shades: [
            s.skin_shade.to_bits(),
            s.hair_shade.to_bits(),
            s.headgear_shade.to_bits(),
            s.shirt_shade.to_bits(),
            s.trousers_shade.to_bits(),
            s.footwear_shade.to_bits(),
        ],
    }
}

/// The two shipped tables, out of the client's own store.
fn chargen_tables(
    c: &HeadlessClient,
) -> (
    dereth_assets::tables::CharGen,
    dereth_assets::tables::SkillTable,
) {
    use dereth_assets::Decode as _;
    use dereth_primitives::{AssetSource as _, DataId};
    let store = c.dat_store().expect("a retail client has a store").clone();
    let cg_bytes = store
        .read(DataId(0x0E00_0002))
        .expect("the character-creation table");
    let cg = dereth_assets::tables::CharGen::decode_payload(DataId(0x0E00_0002), &cg_bytes)
        .expect("it decodes");
    let sk_bytes = store.read(DataId(0x0E00_0004)).expect("the skill table");
    let sk = dereth_assets::tables::SkillTable::decode_payload(DataId(0x0E00_0004), &sk_bytes)
        .expect("it decodes");
    (cg, sk)
}

/// Every clothing table the character-creation tables name, which is the closed set the shell
/// hands the wizard. Without them a part reports no colours at all and four draws never happen,
/// so a roll built without them is a roll from a different place in the stream.
fn clothing_tables(
    c: &HeadlessClient,
    cg: &dereth_assets::tables::CharGen,
) -> std::rc::Rc<
    std::collections::BTreeMap<dereth_primitives::DataId, dereth_assets::motion::ClothingTable>,
> {
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;
    let store = c.dat_store().expect("a retail client has a store").clone();
    let mut out = std::collections::BTreeMap::new();
    for hg in cg.heritage_groups.values() {
        for sx in hg.sexes.values() {
            for item in sx
                .headgear
                .iter()
                .chain(&sx.shirts)
                .chain(&sx.pants)
                .chain(&sx.footwear)
            {
                let id = item.clothing_table;
                if id.0 == 0 || out.contains_key(&id) {
                    continue;
                }
                let bytes = store
                    .read(id)
                    .expect("a named clothing table is in the data files");
                out.insert(
                    id,
                    dereth_assets::motion::ClothingTable::decode_payload(id, &bytes)
                        .expect("it decodes"),
                );
            }
        }
    }
    std::rc::Rc::new(out)
}

/// A character rolled off the two tables with the seeds named, with no shell at all.
fn rolled_with(c: &HeadlessClient, ran2: i32, crt: u32, expansion: bool) -> CharGenState {
    let (cg, sk) = chargen_tables(c);
    let mut s = CharGenState::default();
    s.rng = CharGenRng::new(ran2, crt);
    s.clothing = clothing_tables(c, &cg);
    s.randomize_character(&cg, &sk, expansion);
    s
}

/// Walk to a page by its tab, and check the wizard really got there.
fn goto_page(c: &mut HeadlessClient, p: EcgProgress) {
    click_wizard(c, p.select_button().expect("every page has a tab"));
    assert_eq!(
        with_wizard(c, |_, w| w.progress),
        p,
        "the wizard reached {p:?}"
    );
}

/// The picture an element is showing, and the operation it is shown through.
fn picture_of(
    c: &HeadlessClient,
    id: ElementId,
) -> (
    Option<dereth_primitives::DataId>,
    Option<dereth_ui::region::SurfaceOp>,
) {
    let h = element(c, id);
    let n = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live");
    match &n.region.image {
        Some(g) => (Some(g.did), g.op),
        None => (None, None),
    }
}

fn tab_is_visible(c: &HeadlessClient, p: EcgProgress) -> bool {
    let h = element(c, p.select_button().expect("a tab"));
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .region
        .flags
        .visible
}

/// The rolled character's own half of the shipped table.
fn sex_of(c: &mut HeadlessClient) -> dereth_assets::tables::SexCg {
    let (t, heritage, gender) = with_wizard(c, |_, w| {
        (
            w.tables.clone().expect("the tables"),
            w.state.heritage_group,
            w.state.gender,
        )
    });
    t.chargen
        .heritage_groups
        .get(&heritage)
        .and_then(|h| h.sexes.get(&gender))
        .expect("the rolled people and sex are in the shipped table")
        .clone()
}

/// One entry of a palette, read out of the data file itself rather than off the host code.
fn palette_entry(c: &HeadlessClient, id: dereth_primitives::DataId, index: usize) -> u32 {
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;
    let store = c.dat_store().expect("a retail client has a store").clone();
    let bytes = store.read(id).expect("the palette is in the data files");
    let p = dereth_assets::material::Palette::decode_payload(id, &bytes).expect("it decodes");
    p.colors_argb[index]
}

/// The per-channel mean of one entry across a palette set, which is what a colour spot shows.
fn palette_set_average(c: &HeadlessClient, id: dereth_primitives::DataId, index: usize) -> u32 {
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;
    let store = c.dat_store().expect("a retail client has a store").clone();
    let bytes = store
        .read(id)
        .expect("the palette set is in the data files");
    let ps = dereth_assets::material::PaletteSet::decode_payload(id, &bytes).expect("it decodes");
    let n = u32::try_from(ps.palette_ids.len()).expect("a non-empty set");
    let (mut r, mut g, mut b) = (0_u32, 0_u32, 0_u32);
    for p in &ps.palette_ids {
        let col = palette_entry(c, *p, index);
        r += (col >> 16) & 0xFF;
        g += (col >> 8) & 0xFF;
        b += col & 0xFF;
    }
    0xFF00_0000 | ((r / n) << 16) | ((g / n) << 8) | (b / n)
}

// ---------------------------------------------------------------------------------------------
// chargen.random.the-wizard-opens-on-a-character-already-rolled
// ---------------------------------------------------------------------------------------------

/// Every field a player can see is a real choice the moment the wizard opens.
pub fn the_wizard_opens_on_a_character_already_rolled() {
    let mut c = a_client_on_the_wizard();
    let chosen = with_wizard(&mut c, |_, w| {
        let s = &w.state;
        let people =
            s.heritage_group != 0 && s.gender != 0 && s.start_area != u32::MAX && s.template >= 1;
        let face = [
            s.eyes_strip,
            s.nose_strip,
            s.mouth_strip,
            s.hair_color,
            s.eye_color,
            s.hair_style,
        ]
        .iter()
        .all(|v| *v >= 0);
        // A hat is the one part whose "none" is a real answer, so only its colour is read.
        let clothes = s.shirt_style >= 0 && s.trousers_style >= 0 && s.footwear_style >= 0;
        let shades = [
            s.skin_shade,
            s.hair_shade,
            s.shirt_shade,
            s.trousers_shade,
            s.footwear_shade,
        ]
        .iter()
        .all(|v| (0.0..=1.0).contains(v));
        // And the roll's own tail ran: the three choices it makes are settled rather than open.
        people && face && clothes && shades && s.frozen == [true; 3]
    });

    c.assert_behaviour(
        "chargen.random.the-wizard-opens-on-a-character-already-rolled",
        move |_| chosen,
    );
    c.shutdown();
}

#[test]
fn scenario_the_wizard_opens_on_a_character_already_rolled() {
    scenario("the_wizard_opens_on_a_character_already_rolled");
}

// ---------------------------------------------------------------------------------------------
// chargen.random.the-opening-roll-is-the-seeds-own-and-two-clients-roll-the-same-character
// ---------------------------------------------------------------------------------------------

/// The roll is a function of the seeds, and the shell adds one thing to it: the bared head.
pub fn the_opening_roll_is_the_seeds_own_and_two_clients_roll_the_same_character() {
    let mut c = a_client_on_the_wizard();
    let shown = roll_of(&with_wizard(&mut c, |_, w| w.state.clone()));

    // The oracle is a second pair of the client's own streams, seeded the way a client with no
    // sound device and no window seeds its own: the people is the first draw of one of them and
    // the sex is the very next, with no draw of the other in between.
    let mut oracle = CharGenRng::new(1, 1);
    let heritage = u32::try_from(oracle.roll_dice(1, 3)).expect("one of three");
    let gender = u32::try_from(oracle.roll_dice(1, 2)).expect("one of two");
    let first_two_draws = shown.heritage == heritage && shown.gender == gender;

    // The same character rolled with no shell at all agrees in every field but one: the page the
    // player arrives on takes the hat off so the face can be seen, and parks the style it took.
    let bare = roll_of(&rolled_with(&c, 1, 1, false));
    let bared_head =
        shown.gear[0] == -1 && with_wizard(&mut c, |_, w| w.hold_headgear) == bare.gear[0];
    let mut want = bare;
    want.gear[0] = -1;
    let nothing_else_added = want == shown;
    c.shutdown();

    // And a second client rolls the identical character, down to the shade of every part.
    let mut b = a_client_on_the_wizard();
    let again = roll_of(&with_wizard(&mut b, |_, w| w.state.clone()));
    let reproducible = again == shown;

    b.assert_behaviour(
        "chargen.random.the-opening-roll-is-the-seeds-own-and-two-clients-roll-the-same-character",
        move |_| first_two_draws && bared_head && nothing_else_added && reproducible,
    );
    b.shutdown();
}

#[test]
fn scenario_the_opening_roll_is_the_seeds_own_and_two_clients_roll_the_same_character() {
    scenario("the_opening_roll_is_the_seeds_own_and_two_clients_roll_the_same_character");
}

// ---------------------------------------------------------------------------------------------
// chargen.random.the-people-and-the-sex-come-out-of-a-different-draw-from-everything-else
// ---------------------------------------------------------------------------------------------

/// Two streams, proved from both sides, and the first left exactly two draws along.
pub fn the_people_and_the_sex_come_out_of_their_own_stream() {
    let mut c = a_client_on_the_wizard();

    let a = rolled_with(&c, 1, 1, false);
    let b = rolled_with(&c, 1, 12_345, false);
    // Move only the second stream's seed: the people and the sex must not budge...
    let unmoved =
        (a.heritage_group, a.gender) == (b.heritage_group, b.gender) && roll_of(&a) != roll_of(&b);

    // ...and move only the first stream's seed to one that lands elsewhere, and they must.
    let base = CharGenRng::new(1, 1).roll_dice(1, 3);
    let other = (2..2000)
        .find(|s| CharGenRng::new(*s, 1).roll_dice(1, 3) != base)
        .expect("some seed of the first stream lands on a different people");
    let moved = rolled_with(&c, other, 1, false).heritage_group != a.heritage_group;

    // And the sharper reading the values alone cannot give: after a roll the first stream is
    // exactly two draws along. A people taken from the *other* stream would leave it one short,
    // and under these seeds that would still have produced the same people.
    let mut oracle = CharGenRng::new(1, 1);
    oracle.roll_dice(1, 3);
    oracle.roll_dice(1, 2);
    let mut mine = a.rng.ran2.clone();
    let mut theirs = oracle.ran2;
    let in_step = (0..8).all(|_| (mine.next_f64() - theirs.next_f64()).abs() < f64::EPSILON);

    c.assert_behaviour(
        "chargen.random.the-people-and-the-sex-come-out-of-a-different-draw-from-everything-else",
        move |_| unmoved && moved && in_step,
    );
    c.shutdown();
}

#[test]
fn scenario_the_people_and_the_sex_come_out_of_their_own_stream() {
    scenario("the_people_and_the_sex_come_out_of_their_own_stream");
}

// ---------------------------------------------------------------------------------------------
// chargen.random.the-opening-people-is-one-a-new-account-may-play-and-the-expansion-adds-one
// ---------------------------------------------------------------------------------------------

/// Three peoples without the expansion and four with it, out of the thirteen the data file holds.
pub fn the_opening_people_is_one_a_plain_account_may_play() {
    let mut c = a_client_on_the_wizard();
    let (cg, sk) = chargen_tables(&c);

    let mut plain = std::collections::BTreeSet::new();
    let mut with_expansion = std::collections::BTreeSet::new();
    for seed in 1..512 {
        for (set, expansion) in [(&mut plain, false), (&mut with_expansion, true)] {
            let mut s = CharGenState::default();
            s.rng = CharGenRng::new(seed, 1);
            s.randomize_character(&cg, &sk, expansion);
            set.insert(s.heritage_group);
        }
    }
    let three = plain == [1, 2, 3].into_iter().collect();
    let four = with_expansion == [1, 2, 3, 4].into_iter().collect();
    // ...out of all thirteen the data file still carries, which is what makes three a choice.
    let thirteen = cg.heritage_groups.len() >= 13;

    c.assert_behaviour("chargen.random.the-opening-people-is-one-a-plain-account-may-play-and-the-expansion-adds-one", move |_| {
        three && four && thirteen
    });
    c.shutdown();
}

#[test]
fn scenario_the_opening_people_is_one_a_plain_account_may_play() {
    scenario("the_opening_people_is_one_a_plain_account_may_play");
}

// ---------------------------------------------------------------------------------------------
// chargen.heritage.the-home-town-follows-the-people-and-is-one-of-that-peoples-own
// ---------------------------------------------------------------------------------------------

/// Choosing a people settles a home town, from that people's own list -- and, as the data files
/// ship, that list has one entry, so the town is a consequence of the people and not a roll.
pub fn the_home_town_follows_the_people() {
    let mut c = a_client_on_the_wizard();
    let (cg, sk) = chargen_tables(&c);

    // Every people the data file has: the town settled on has to be one of that people's own.
    let mut from_their_own_list = true;
    for (&h, hg) in &cg.heritage_groups {
        if hg.primary_start_areas.is_empty() {
            continue;
        }
        let mut s = CharGenState::default();
        s.rng = CharGenRng::new(1, 1);
        s.reset(&cg, &sk);
        s.set_heritage_group(&cg, &sk, h);
        from_their_own_list &= hg.primary_start_areas.contains(&s.start_area)
            && (s.start_area as usize) < cg.starter_areas.len();
    }

    // And as the data files ship, every people lists exactly one, so the draw that chooses it has
    // one answer, takes no draw at all, and the town is a function of the people. A data file
    // that ever gave a people two would make that draw real, and this says so rather than
    // leaving a hole where the claim was.
    let one_each = cg.heritage_groups.len() == 13
        && cg
            .heritage_groups
            .values()
            .all(|hg| hg.primary_start_areas.len() == 1);
    let area = |h: u32| cg.heritage_groups[&h].primary_start_areas[0];
    let name = |i: u32| cg.starter_areas[i as usize].name.clone();
    let the_mapping = name(area(1)) == "Holtburg"
        && name(area(2)) == "Yaraq"
        && name(area(3)) == "Shoushi"
        && name(area(4)) == "Sanamar";

    // The draw itself, on a one-entry list: one answer, and the stream does not move -- which is
    // what makes the town a consequence rather than a coincidence.
    let mut r = CharGenRng::new(1, 1);
    let no_draw = r.rand_int_excluding(1, 7) == 0
        && r.rand_int_excluding(0, 7) == 0
        && r.rand_int_excluding(-3, 7) == 0
        && r.crt.next_u16() == CharGenRng::new(1, 1).crt.next_u16();
    // Two or more and it does draw, and never lands on the one it was told to avoid.
    let mut r = CharGenRng::new(1, 1);
    let a_real_draw = (0..200).all(|_| {
        let v = r.rand_int_excluding(4, 2);
        (0..4).contains(&v) && v != 2
    });

    c.assert_behaviour(
        "chargen.heritage.the-home-town-follows-the-people-and-is-one-of-that-peoples-own",
        move |_| from_their_own_list && one_each && the_mapping && no_draw && a_real_draw,
    );
    c.shutdown();
}

#[test]
fn scenario_the_home_town_follows_the_people() {
    scenario("the_home_town_follows_the_people");
}

// ---------------------------------------------------------------------------------------------
// chargen.random.the-button-re-rolls-the-page-the-player-is-on
// ---------------------------------------------------------------------------------------------

/// Each of the five ordinary pages re-rolls its own thing, asserted on that thing and not on
/// "something changed".
pub fn the_random_button_re_rolls_the_page_the_player_is_on() {
    let mut c = a_client_on_the_wizard();
    // An account with the expansion, so the town roll can reach the fourth town and the people
    // roll the fourth people.
    c.app_mut().host_state_mut().account_has_tod = true;
    c.tick(1);

    // The people: the roll may legitimately land where it already is, so it is pressed until it
    // moves. Forty presses of a four-way roll all repeating is not a thing that happens.
    goto_page(&mut c, EcgProgress::Hertage);
    let before = with_wizard(&mut c, |_, w| w.state.heritage_group);
    let mut people_moved = false;
    for _ in 0..40 {
        click_wizard(&mut c, chargen::RANDOM_BUTTON);
        if with_wizard(&mut c, |_, w| w.state.heritage_group) != before {
            people_moved = true;
            break;
        }
    }

    // The profession: the roll excludes the one already chosen, so **one** press must move it,
    // and it never lands on the hand-built one.
    goto_page(&mut c, EcgProgress::Profession);
    let before = with_wizard(&mut c, |_, w| w.state.template);
    click_wizard(&mut c, chargen::RANDOM_BUTTON);
    let after = with_wizard(&mut c, |_, w| w.state.template);
    let profession_moved = before != after && after >= 1;

    // The skills: re-rolled, and never overspent.
    goto_page(&mut c, EcgProgress::Skills);
    let before = with_wizard(&mut c, |_, w| w.state.skill_levels.clone());
    click_wizard(&mut c, chargen::RANDOM_BUTTON);
    let skills_moved = with_wizard(&mut c, |_, w| {
        w.state.skill_levels != before && w.state.remaining_skill_credits >= 0
    });

    // The face, with the current choice excluded from every draw, so one press moves all of it.
    goto_page(&mut c, EcgProgress::Appearance);
    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    let before = roll_of(&with_wizard(&mut c, |_, w| w.state.clone()));
    click_wizard(&mut c, chargen::RANDOM_BUTTON);
    let after = roll_of(&with_wizard(&mut c, |_, w| w.state.clone()));
    let face_moved = before.face != after.face && before.shades[0] != after.shades[0];

    // The clothes.
    click_wizard(&mut c, chargen::appearance::TAB_CLOTHES);
    let before = roll_of(&with_wizard(&mut c, |_, w| w.state.clone()));
    click_wizard(&mut c, chargen::RANDOM_BUTTON);
    let clothes_moved = roll_of(&with_wizard(&mut c, |_, w| w.state.clone())).gear != before.gear;

    // The town, which is a flat roll and so can repeat.
    goto_page(&mut c, EcgProgress::Town);
    let before = with_wizard(&mut c, |_, w| w.state.start_area);
    let mut town_moved = false;
    for _ in 0..40 {
        click_wizard(&mut c, chargen::RANDOM_BUTTON);
        if with_wizard(&mut c, |_, w| w.state.start_area) != before {
            town_moved = true;
            break;
        }
    }

    c.assert_behaviour(
        "chargen.random.the-button-re-rolls-the-page-the-player-is-on",
        move |_| {
            people_moved
                && profession_moved
                && skills_moved
                && face_moved
                && clothes_moved
                && town_moved
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_random_button_re_rolls_the_page_the_player_is_on() {
    scenario("the_random_button_re_rolls_the_page_the_player_is_on");
}

// ---------------------------------------------------------------------------------------------
// chargen.random.on-the-last-page-it-warns-first-and-only-a-yes-re-rolls-the-whole-character
// ---------------------------------------------------------------------------------------------

/// The last page warns before throwing the whole character away, and a refusal rolls nothing.
pub fn the_last_page_warns_before_re_rolling_the_whole_character() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    let before = roll_of(&with_wizard(&mut c, |_, w| w.state.clone()));
    click_wizard(&mut c, chargen::RANDOM_BUTTON);
    let warned = with_wizard(&mut c, |_, w| w.open_dialog) == Some(CharGenDialog::RandomizeWarning);
    let nothing_yet = roll_of(&with_wizard(&mut c, |_, w| w.state.clone())) == before;

    with_wizard(&mut c, |_, w| w.close_dialog(true));
    c.tick(1);
    let re_rolled = roll_of(&with_wizard(&mut c, |_, w| w.state.clone())) != before;
    // ...and the re-roll leaves a playable character rather than a blank one: a profession is
    // chosen, its own skills are specialised, and credits were spent doing it.
    let playable = with_wizard(&mut c, |_, w| {
        let specialised = w
            .state
            .skill_levels
            .iter()
            .filter(|s| **s == dereth_chargen::SkillAdvancementClass::Specialized)
            .count();
        w.state.template >= 1
            && specialised >= 1
            && w.state.remaining_skill_credits < w.state.total_skill_credits
    });
    c.shutdown();

    // The other half: a refusal is not a re-roll.
    let mut d = a_client_on_the_wizard();
    goto_page(&mut d, EcgProgress::Summary);
    let before = roll_of(&with_wizard(&mut d, |_, w| w.state.clone()));
    click_wizard(&mut d, chargen::RANDOM_BUTTON);
    with_wizard(&mut d, |_, w| w.close_dialog(false));
    d.tick(2);
    let declined = roll_of(&with_wizard(&mut d, |_, w| w.state.clone())) == before;

    d.assert_behaviour("chargen.random.on-the-last-page-it-warns-first-and-only-a-yes-re-rolls-the-whole-character", move |_| {
        warned && nothing_yet && re_rolled && playable && declined
    });
    d.shutdown();
}

#[test]
fn scenario_the_last_page_warns_before_re_rolling_the_whole_character() {
    scenario("the_last_page_warns_before_re_rolling_the_whole_character");
}

// ---------------------------------------------------------------------------------------------
// chargen.random.on-the-town-page-it-can-land-on-any-town-and-not-only-the-peoples-own
// ---------------------------------------------------------------------------------------------

/// The town page's own roll is flat, so it can give a player a town no people of theirs starts in.
pub fn the_town_pages_roll_can_land_on_any_town() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Town);

    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..200 {
        click_wizard(&mut c, chargen::RANDOM_BUTTON);
        seen.insert(with_wizard(&mut c, |_, w| w.state.start_area));
    }
    let three: bool = seen == [0, 1, 2].into_iter().collect();

    c.app_mut().host_state_mut().account_has_tod = true;
    c.tick(1);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..200 {
        click_wizard(&mut c, chargen::RANDOM_BUTTON);
        seen.insert(with_wizard(&mut c, |_, w| w.state.start_area));
    }
    let four: bool = seen == [0, 1, 2, 3].into_iter().collect();

    c.assert_behaviour(
        "chargen.random.on-the-town-page-it-can-land-on-any-town-and-not-only-the-peoples-own",
        move |_| three && four,
    );
    c.shutdown();
}

#[test]
fn scenario_the_town_pages_roll_can_land_on_any_town() {
    scenario("the_town_pages_roll_can_land_on_any_town");
}

// ---------------------------------------------------------------------------------------------
// chargen.tabs.the-two-insect-peoples-lose-three-pages-outright-rather-than-greyed
// ---------------------------------------------------------------------------------------------

/// Three tabs go away entirely for the two peoples that have no use for them.
pub fn the_insect_peoples_lose_three_tabs_outright() {
    let mut c = a_client_on_the_wizard();
    let gone = [
        EcgProgress::Profession,
        EcgProgress::Skills,
        EcgProgress::Town,
    ];

    // Every tab is there for an ordinary people, which is what makes the reading below a change.
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    let all_there = EcgProgress::PAGES.iter().all(|p| tab_is_visible(&c, *p));

    let mut hidden_not_greyed = true;
    for heritage in [HERITAGE_OLTHOI, HERITAGE_OLTHOI_ACID] {
        let (id, _) = *chargen::HERITAGE_BUTTONS
            .iter()
            .find(|(_, h)| *h == heritage)
            .expect("both bullets are in the strip");
        click_wizard(&mut c, id);
        hidden_not_greyed &= with_wizard(&mut c, |_, w| w.state.heritage_group) == heritage;
        for p in EcgProgress::PAGES {
            let h = element(&c, p.select_button().expect("a tab"));
            let node = c
                .view()
                .expect_app()
                .ui()
                .expect("the UI shell is up")
                .ui
                .node(h)
                .expect("live");
            hidden_not_greyed &= node.region.flags.visible == !gone.contains(&p);
            // ...and a tab that is gone is not *also* greyed: one thing happens to it, not two.
            hidden_not_greyed &= !node
                .instance_properties
                .get(dereth_ui::props::attr::DISABLED)
                .is_some_and(|v| matches!(v, dereth_assets::ui::PropertyValue::Bool(true)));
        }
        click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    }

    c.assert_behaviour("chargen.tabs.the-two-insect-peoples-lose-three-pages-outright-rather-than-having-them-greyed", move |_| {
        all_there && hidden_not_greyed
    });
    c.shutdown();
}

#[test]
fn scenario_the_insect_peoples_lose_three_tabs_outright() {
    scenario("the_insect_peoples_lose_three_tabs_outright");
}

// ---------------------------------------------------------------------------------------------
// chargen.appearance.the-face-tab-takes-the-hat-off-and-the-clothes-tab-puts-it-back
// ---------------------------------------------------------------------------------------------

/// The hat comes off to show the face and goes back on when the player leaves that tab -- and
/// "no hat" is an answer that survives the round trip, not an empty slot.
pub fn the_face_tab_takes_the_hat_off_and_the_clothes_tab_puts_it_back() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Appearance);

    click_wizard(&mut c, chargen::appearance::TAB_CLOTHES);
    let t = with_wizard(&mut c, |_, w| w.tables.clone().expect("the tables"));
    with_wizard(&mut c, |_, w| w.state.set_headgear_style(&t.chargen, 0));
    let wearing = with_wizard(&mut c, |_, w| w.state.headgear_style) == 0;

    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    let bared = with_wizard(&mut c, |_, w| (w.state.headgear_style, w.hold_headgear)) == (-1, 0);

    click_wizard(&mut c, chargen::appearance::TAB_CLOTHES);
    let back_on = with_wizard(&mut c, |_, w| w.state.headgear_style) == 0;

    // "No hat" is a real choice and comes back as itself.
    let t = with_wizard(&mut c, |_, w| w.tables.clone().expect("the tables"));
    with_wizard(&mut c, |_, w| w.state.set_headgear_style(&t.chargen, -1));
    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    click_wizard(&mut c, chargen::appearance::TAB_CLOTHES);
    let no_hat_survives = with_wizard(&mut c, |_, w| w.state.headgear_style) == -1;

    // And the park is not eaten by the page being drawn again: leaving the page and coming back
    // is two whole repaints, and the hat is still there to be put back on.
    with_wizard(&mut c, |_, w| w.state.set_headgear_style(&t.chargen, 0));
    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    goto_page(&mut c, EcgProgress::Town);
    goto_page(&mut c, EcgProgress::Appearance);
    let survived_the_repaints = with_wizard(&mut c, |_, w| w.hold_headgear) == 0;
    click_wizard(&mut c, chargen::appearance::TAB_CLOTHES);
    let and_came_back = with_wizard(&mut c, |_, w| w.state.headgear_style) == 0;

    c.assert_behaviour(
        "chargen.appearance.the-face-tab-takes-the-hat-off-and-the-clothes-tab-puts-it-back",
        move |_| {
            wearing && bared && back_on && no_hat_survives && survived_the_repaints && and_came_back
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_face_tab_takes_the_hat_off_and_the_clothes_tab_puts_it_back() {
    scenario("the_face_tab_takes_the_hat_off_and_the_clothes_tab_puts_it_back");
}

// ---------------------------------------------------------------------------------------------
// chargen.appearance.the-colour-spots-are-the-chosen-parts-own-colours
// ---------------------------------------------------------------------------------------------

/// Nine coloured spots, each the mean of that colour's own palette, and the spare ones blank.
pub fn the_colour_spots_are_the_chosen_parts_own_colours() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Appearance);
    click_wizard(&mut c, chargen::appearance::TAB_FACE);

    // The pictures the wheel is built out of resolve through the shipped lookup -- which is the
    // premise, and was the whole defect: nine black holes where the colours should have been.
    let art = with_wizard(&mut c, |_, w| w.tables.clone().expect("the tables")).color_wheel_art;
    let resolved = art.bullet.0 != 0 && art.empty.0 != 0;

    let sx = sex_of(&mut c);
    let n = sx.hair_colors.len();
    assert!(n > 1, "this people offers {n} hair colours");
    let lit = n.min(9);

    let mut spots = true;
    for (i, id) in chargen::appearance::COLOR_SPOTS.iter().enumerate() {
        let (did, op) = picture_of(&c, *id);
        if i < lit {
            let want = palette_set_average(&c, dereth_primitives::DataId(sx.hair_colors[i]), 0xD0);
            spots &= did == Some(art.bullet)
                && op
                    == Some(dereth_ui::region::SurfaceOp::ReplaceColor {
                        from: dereth_ui::region::SurfaceOp::OPAQUE_BLACK,
                        to: want,
                    });
        } else {
            // Past the colours this part has: the blank picture itself, not a recolouring of one.
            spots &= did == Some(art.empty) && op.is_none();
        }
    }

    c.assert_behaviour("chargen.appearance.the-colour-spots-are-the-chosen-parts-own-colours-and-the-spare-ones-are-blank", move |_| {
        resolved && spots
    });
    c.shutdown();
}

#[test]
fn scenario_the_colour_spots_are_the_chosen_parts_own_colours() {
    scenario("the_colour_spots_are_the_chosen_parts_own_colours");
}

// ---------------------------------------------------------------------------------------------
// chargen.appearance.the-eyes-have-one-colour-each-and-no-shade-to-slide
// ---------------------------------------------------------------------------------------------

/// The eye row is the one part that is a single colour rather than a range, and it has no shade.
pub fn the_eyes_have_one_colour_each_and_no_shade_to_slide() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Appearance);
    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    // The eye row of the face tab, which the page binds to its own spinner.
    click_wizard(&mut c, ElementId(0x1000_03B0));

    let art = with_wizard(&mut c, |_, w| w.tables.clone().expect("the tables")).color_wheel_art;
    let sx = sex_of(&mut c);
    assert!(!sx.eye_colors.is_empty(), "this people offers eye colours");

    let (did, op) = picture_of(&c, chargen::appearance::COLOR_SPOTS[0]);
    let want = palette_entry(&c, dereth_primitives::DataId(sx.eye_colors[0]), 0x103);
    let one_colour = did == Some(art.bullet)
        && op
            == Some(dereth_ui::region::SurfaceOp::ReplaceColor {
                from: dereth_ui::region::SurfaceOp::OPAQUE_BLACK,
                to: want,
            });

    let (did, op) = picture_of(&c, chargen::appearance::GRAD_CIRCLE);
    let flat_disk = did == Some(art.plug) && op.is_none();
    let h = element(&c, chargen::appearance::SHADE_SCROLL);
    let no_slider = !c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .region
        .flags
        .visible;

    c.assert_behaviour(
        "chargen.appearance.the-eyes-have-one-colour-each-and-no-shade-to-slide",
        move |_| one_colour && flat_disk && no_slider,
    );
    c.shutdown();
}

#[test]
fn scenario_the_eyes_have_one_colour_each_and_no_shade_to_slide() {
    scenario("the_eyes_have_one_colour_each_and_no_shade_to_slide");
}

// ---------------------------------------------------------------------------------------------
// chargen.appearance.clicking-a-colour-moves-the-marker-and-tints-the-shade-wheel-with-it
// ---------------------------------------------------------------------------------------------

/// Clicking a colour moves the marker to it and re-tints the shade wheel in that colour.
pub fn clicking_a_colour_moves_the_marker_and_tints_the_shade_wheel() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Appearance);
    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    let art = with_wizard(&mut c, |_, w| w.tables.clone().expect("the tables")).color_wheel_art;
    let ring_resolved = art.ring.0 != 0;

    let marker_visible = |c: &HeadlessClient, i: usize| {
        let h = element(c, chargen::appearance::COLOR_POINTERS[i]);
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("live")
            .region
            .flags
            .visible
    };

    // Two colours this part really has: a click past the end is refused, and a people's list is
    // not always nine long.
    let n = with_wizard(&mut c, |_, w| w.choices[0].num_colors);
    let last = usize::try_from(n).expect("a count").min(9) - 1;
    assert!(last >= 1, "this people offers {n} hair colours");

    click_wizard(&mut c, chargen::appearance::COLOR_SPOTS[0]);
    let first_picked = with_wizard(&mut c, |_, w| w.current_color) == 0 && marker_visible(&c, 0);
    let (did, op) = picture_of(&c, chargen::appearance::GRAD_CIRCLE);
    let two = with_wizard(&mut c, |_, w| {
        w.color_wheel[0].expect("the first spot has a colour")
    });
    let tinted = did == Some(art.ring) && op == Some(dereth_ui::region::SurfaceOp::Multiply(two));

    click_wizard(&mut c, chargen::appearance::COLOR_SPOTS[last]);
    let moved = with_wizard(&mut c, |_, w| w.current_color)
        == i32::try_from(last).expect("in range")
        && marker_visible(&c, last)
        && !marker_visible(&c, 0);
    let four = with_wizard(&mut c, |_, w| {
        w.color_wheel[last].expect("the last spot has a colour")
    });
    let followed = two != four
        && picture_of(&c, chargen::appearance::GRAD_CIRCLE).1
            == Some(dereth_ui::region::SurfaceOp::Multiply(four));

    c.assert_behaviour(
        "chargen.appearance.clicking-a-colour-moves-the-marker-and-tints-the-shade-wheel-with-it",
        move |_| ring_resolved && first_picked && tinted && moved && followed,
    );
    c.shutdown();
}

#[test]
fn scenario_clicking_a_colour_moves_the_marker_and_tints_the_shade_wheel() {
    scenario("clicking_a_colour_moves_the_marker_and_tints_the_shade_wheel");
}

// =============================================================================================
// chargen.dialogs.* -- the wizard's other four boxes
//
// The table of which box is which kind and which button answers it is **not** a row: it would be a
// table of numbers beside the symbols the production code carries them through, a transcription.
// What it would protect is asserted here from the outside, by pressing the buttons.
//
// The bring-up is the exit scenarios' `a_client_on_the_wizard`; the pointer is
// `adapters_shell::Hands`.
// =============================================================================================

use dereth_chargen::CgVerification;
use dereth_ui_screens::screens::chargen::{
    CREDIT_WARNING_STRING, MESSAGE_BUTTON, RANDOMIZE_WARNING_STRING, RANDOM_BUTTON,
    TOD_WARNING_STRING,
};

/// The handle of a dialog the wizard has raised, if it is on screen.
fn wizard_dialog(c: &mut HeadlessClient, which: CharGenDialog) -> Option<dereth_ui::ElemHandle> {
    with_wizard(c, |_, w| w.dialog_element(which))
}

/// Click a child **of a dialog**, which is not under the wizard's own root.
fn click_in_dialog(c: &mut HeadlessClient, dialog: dereth_ui::ElemHandle, id: ElementId) {
    let h = dialog_child(c, dialog, id);
    press_handle(c, h);
}

/// What a dialog is: the right subclass's root, modal, and carrying the shipped words.
///
/// The three together, because a box that is the wrong shape, one a player can click past, and a
/// blank one are three different things a player would see.
fn dialog_is(
    c: &mut HeadlessClient,
    h: dereth_ui::ElemHandle,
    kind: dereth_ui::dialog::DialogKind,
    token: &str,
) -> bool {
    let (node_id, modal) = {
        let n = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("live");
        (n.element_id(), n.region.flags.block_clicks)
    };
    let want = shipped_word(c, token);
    let body = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(h, dereth_ui::dialog::base::child::TEXT)
        .expect("a dialog carries its own text element");
    node_id == kind.root_element_id() && modal && !want.is_empty() && wizard_text(c, body) == want
}

/// Everything a re-roll can move, as one comparable value -- so "it was re-rolled" and "it was
/// left alone" are the same reading taken in two directions.
fn appearance_of(c: &mut HeadlessClient) -> Vec<i32> {
    with_wizard(c, |_, w| {
        let s = &w.state;
        vec![
            i32::try_from(s.heritage_group).unwrap_or(-1),
            i32::try_from(s.gender).unwrap_or(-1),
            s.eyes_strip,
            s.nose_strip,
            s.mouth_strip,
            s.hair_style,
            s.hair_color,
            s.eye_color,
            s.headgear_style,
            s.shirt_style,
            s.trousers_style,
            s.footwear_style,
            s.template,
        ]
    })
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.the-warning-before-a-re-roll-is-a-modal-question-in-the-shipped-words
// ---------------------------------------------------------------------------------------------

/// The warning is really built, is really modal, and really says what the shipped text says.
pub fn the_re_roll_warning_is_a_modal_question_in_the_shipped_words() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    let nothing_first = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none();

    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning)
        .expect("the random button on the last page raises the warning");
    let recorded =
        with_wizard(&mut c, |_, w| w.open_dialog) == Some(CharGenDialog::RandomizeWarning);
    let shaped = dialog_is(
        &mut c,
        h,
        dereth_ui::dialog::DialogKind::Confirmation,
        RANDOMIZE_WARNING_STRING,
    );

    c.assert_behaviour(
        "chargen.dialogs.the-warning-before-a-re-roll-is-a-modal-question-in-the-shipped-words",
        move |_| nothing_first && recorded && shaped,
    );
    c.shutdown();
}

#[test]
fn scenario_the_re_roll_warning_is_a_modal_question_in_the_shipped_words() {
    scenario("the_re_roll_warning_is_a_modal_question_in_the_shipped_words");
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.the-two-answer-buttons-really-answer-and-a-pointer-can-press-either-of-them
// ---------------------------------------------------------------------------------------------

/// Yes and no, pressed as buttons and then pressed with a pointer.
///
/// The two halves are one scenario because the interesting claim is the **difference**: a client
/// that re-rolled on both answers and one that re-rolled on neither each pass half of it. And the
/// pointer half is here because a driven windowed run once disagreed with the message-only one --
/// the no could be answered and the yes could not -- so the gesture that goes through the hit
/// test is asserted beside the one that does not.
pub fn the_warnings_two_buttons_answer_it_and_a_pointer_can_press_either() {
    // --- no, through the button's own message ---------------------------------------------
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    let before = appearance_of(&mut c);
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    click_in_dialog(&mut c, h, dereth_ui::dialog::base::child::BUTTON2);
    c.tick(1);
    let no_answered = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none()
        && appearance_of(&mut c) == before
        && with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Summary;
    c.shutdown();

    // --- yes, from the identical starting character ----------------------------------------
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    let same_start = appearance_of(&mut c) == before;
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    click_in_dialog(&mut c, h, dereth_ui::dialog::base::child::BUTTON1);
    c.tick(2);
    let yes_answered = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none()
        && appearance_of(&mut c) != before;
    c.shutdown();

    // --- the same two through a real pointer, hit test and all ------------------------------
    let mut c = a_client_on_the_wizard();
    let mut hands = Hands::new();
    goto_page(&mut c, EcgProgress::Summary);
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    let no = dialog_child(&mut c, h, dereth_ui::dialog::base::child::BUTTON2);
    hands.click_handle(&mut c, no);
    c.tick(1);
    // The calibration first: without it, "the yes does nothing" and "my pointer does nothing" are
    // the same reading, which is the mistake the driven runs could have led to.
    let pointer_can_answer = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none()
        && appearance_of(&mut c) == before;
    c.shutdown();

    let mut c = a_client_on_the_wizard();
    let mut hands = Hands::new();
    goto_page(&mut c, EcgProgress::Summary);
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    let yes = dialog_child(&mut c, h, dereth_ui::dialog::base::child::BUTTON1);
    hands.click_handle(&mut c, yes);
    c.tick(2);
    let pointer_yes = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none()
        && appearance_of(&mut c) != before;

    c.assert_behaviour("chargen.dialogs.the-two-answer-buttons-really-answer-and-a-pointer-can-press-either-of-them", move |_| {
        no_answered && same_start && yes_answered && pointer_can_answer && pointer_yes
    });
    c.shutdown();
}

#[test]
fn scenario_the_warnings_two_buttons_answer_it_and_a_pointer_can_press_either() {
    scenario("the_warnings_two_buttons_answer_it_and_a_pointer_can_press_either");
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.the-warning-before-a-re-roll-is-the-last-pages-alone
// ---------------------------------------------------------------------------------------------

/// On any other page the random button rolls at once, with no question and no box.
pub fn only_the_last_page_asks_before_re_rolling() {
    let mut c = a_client_on_the_wizard();
    // The face page, because its roll moves a dozen things at once, so "it rolled" is visible.
    // The people page's roll is a three-way one that may land where it started, which would make
    // this pass for the wrong reason.
    click_wizard(
        &mut c,
        EcgProgress::Appearance
            .select_button()
            .expect("the appearance tab"),
    );
    let on_the_page = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Appearance;
    let before = appearance_of(&mut c);

    click_wizard(&mut c, RANDOM_BUTTON);

    let no_question = with_wizard(&mut c, |_, w| w.open_dialog).is_none()
        && CharGenDialog::RAISED
            .iter()
            .all(|ctx| wizard_dialog(&mut c, *ctx).is_none());
    let rolled_at_once = appearance_of(&mut c) != before;

    c.assert_behaviour(
        "chargen.dialogs.the-warning-before-a-re-roll-is-the-last-pages-alone",
        move |_| on_the_page && no_question && rolled_at_once,
    );
    c.shutdown();
}

#[test]
fn scenario_only_the_last_page_asks_before_re_rolling() {
    scenario("only_the_last_page_asks_before_re_rolling");
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.a-people-the-account-cannot-play-is-refused-in-a-message-box
// ---------------------------------------------------------------------------------------------

/// A people that needs the expansion is refused, in a box with one button and nothing to answer.
pub fn a_people_the_account_cannot_play_is_refused_in_a_message_box() {
    let mut c = a_client_on_the_wizard();
    let plain_account = !with_wizard(&mut c, |_, w| w.account_has_tod);
    let before = with_wizard(&mut c, |_, w| w.state.heritage_group);

    let (bullet, which) = chargen::HERITAGE_BUTTONS[3];
    assert_eq!(which, 4, "the fourth bullet is the one the expansion adds");
    click_wizard(&mut c, bullet);

    let h = wizard_dialog(&mut c, CharGenDialog::ToDRequired).expect("the refusal is raised");
    let shaped = dialog_is(
        &mut c,
        h,
        dereth_ui::dialog::DialogKind::Message,
        TOD_WARNING_STRING,
    );
    let refused = with_wizard(&mut c, |_, w| w.state.heritage_group) == before;

    // Its one button, which is all a message box has.
    click_in_dialog(&mut c, h, MESSAGE_BUTTON);
    c.tick(1);
    let taken_down = wizard_dialog(&mut c, CharGenDialog::ToDRequired).is_none()
        && with_wizard(&mut c, |_, w| w.state.heritage_group) == before;

    c.assert_behaviour(
        "chargen.dialogs.a-people-the-account-cannot-play-is-refused-in-a-message-box",
        move |_| plain_account && shaped && refused && taken_down,
    );
    c.shutdown();
}

#[test]
fn scenario_a_people_the_account_cannot_play_is_refused_in_a_message_box() {
    scenario("a_people_the_account_cannot_play_is_refused_in_a_message_box");
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.finishing-with-credits-unspent-asks-first-and-a-yes-goes-on-to-create
// ---------------------------------------------------------------------------------------------

/// Unspent credits raise a question, the question is a refusal until it is answered, and yes
/// goes on to create the character.
pub fn finishing_with_credits_unspent_asks_first() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    // The first refusal is the empty name, which is a different box; give it a name.
    with_wizard(&mut c, |_, w| {
        w.state.name = "Alba".to_string();
        w.name_entered = true;
    });
    // The rolled character has spent its credits, which is the roll's own doing -- so the state
    // the warning is about is set up explicitly, which is what a player who moves a slider back
    // does, rather than relied upon.
    let spent = with_wizard(&mut c, |_, w| w.state.remaining_atrb_credits) == 0;
    with_wizard(&mut c, |_, w| {
        w.state.remaining_atrb_credits = 30;
        let _ = w.take_actions();
    });

    click_wizard(&mut c, chargen::FINISH_BUTTON);

    let h = wizard_dialog(&mut c, CharGenDialog::CreditWarning).expect("it asks");
    let shaped = dialog_is(
        &mut c,
        h,
        dereth_ui::dialog::DialogKind::Confirmation,
        CREDIT_WARNING_STRING,
    );
    // ...and it really is a refusal: nothing has been created while the question is up.
    let nothing_sent = with_wizard(&mut c, |_, w| w.state.verification) == CgVerification::Undef;

    click_in_dialog(&mut c, h, dereth_ui::dialog::base::child::BUTTON1);
    c.tick(1);
    let went_on = wizard_dialog(&mut c, CharGenDialog::CreditWarning).is_none()
        && with_wizard(&mut c, |_, w| w.state.verification) == CgVerification::Pending;

    c.assert_behaviour(
        "chargen.dialogs.finishing-with-credits-unspent-asks-first-and-a-yes-goes-on-to-create",
        move |_| spent && shaped && nothing_sent && went_on,
    );
    c.shutdown();
}

#[test]
fn scenario_finishing_with_credits_unspent_asks_first() {
    scenario("finishing_with_credits_unspent_asks_first");
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.finishing-with-no-name-refuses-in-a-message-box
// ---------------------------------------------------------------------------------------------

/// No name is a refusal in a box with one button, and dismissing it consumes the message.
pub fn finishing_with_no_name_refuses_in_a_message_box() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    with_wizard(&mut c, |_, w| {
        w.state.name = String::new();
        w.name_entered = false;
    });

    click_wizard(&mut c, chargen::FINISH_BUTTON);

    let h = wizard_dialog(&mut c, CharGenDialog::ErrorMessage).expect("it refuses in a box");
    let shaped = dialog_is(
        &mut c,
        h,
        dereth_ui::dialog::DialogKind::Message,
        "ID_CharGen_NoNameWarning",
    );
    click_in_dialog(&mut c, h, MESSAGE_BUTTON);
    c.tick(1);
    let taken_down = wizard_dialog(&mut c, CharGenDialog::ErrorMessage).is_none()
        && with_wizard(&mut c, |_, w| w.error_string_id).is_none();

    c.assert_behaviour(
        "chargen.dialogs.finishing-with-no-name-refuses-in-a-message-box",
        move |_| shaped && taken_down,
    );
    c.shutdown();
}

#[test]
fn scenario_finishing_with_no_name_refuses_in_a_message_box() {
    scenario("finishing_with_no_name_refuses_in_a_message_box");
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.the-wizard-never-shows-a-please-wait-box
// ---------------------------------------------------------------------------------------------

/// The one box the wizard keeps a place for and never raises draws nothing.
pub fn the_wizard_never_shows_a_please_wait_box() {
    let mut c = a_client_on_the_wizard();
    let roots_before = wizard_roots(&mut c);

    with_wizard(&mut c, |_, w| {
        w.open_dialog = Some(CharGenDialog::PleaseWait)
    });
    c.tick(3);

    let still_recorded =
        with_wizard(&mut c, |_, w| w.open_dialog) == Some(CharGenDialog::PleaseWait);
    let nothing_drawn = wizard_dialog(&mut c, CharGenDialog::PleaseWait).is_none();
    let no_extra_root = wizard_roots(&mut c) == roots_before;
    // ...and it is not one of the ones the wizard can raise, which is the same claim from the
    // other side: the five it can raise all have a shape, and this one has none.
    let not_raisable = !CharGenDialog::RAISED.contains(&CharGenDialog::PleaseWait)
        && CharGenDialog::RAISED.len() == 5
        && CharGenDialog::RAISED.iter().all(|ctx| ctx.kind().is_some())
        && CharGenDialog::PleaseWait.kind().is_none();

    c.assert_behaviour(
        "chargen.dialogs.the-wizard-never-shows-a-please-wait-box",
        move |_| still_recorded && nothing_drawn && no_extra_root && not_raisable,
    );
    c.shutdown();
}

#[test]
fn scenario_the_wizard_never_shows_a_please_wait_box() {
    scenario("the_wizard_never_shows_a_please_wait_box");
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.one-answered-from-outside-the-screen-is-taken-down-on-the-next-frame
// ---------------------------------------------------------------------------------------------

/// A box answered by something that is not the player's pointer still comes off the screen.
pub fn a_box_answered_from_outside_the_screen_comes_down() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    let roots_with_it = wizard_roots(&mut c);
    let live = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .is_some();

    // Exactly what the host does when the answer comes from somewhere with no screen in reach.
    with_wizard(&mut c, |_, w| w.close_dialog(false));
    c.tick(1);

    let forgotten = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).is_none();
    let root_deleted = wizard_roots(&mut c) == roots_with_it - 1;
    let gone_from_the_tree = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .is_none();

    c.assert_behaviour(
        "chargen.dialogs.one-answered-from-outside-the-screen-is-taken-down-on-the-next-frame",
        move |_| live && forgotten && root_deleted && gone_from_the_tree,
    );
    c.shutdown();
}

#[test]
fn scenario_a_box_answered_from_outside_the_screen_comes_down() {
    scenario("a_box_answered_from_outside_the_screen_comes_down");
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.a-second-one-waits-its-turn-rather-than-stacking-on-the-first
// ---------------------------------------------------------------------------------------------

/// Two boxes at once are one box and one waiting, and answering the first shows the second.
pub fn a_second_box_waits_its_turn_rather_than_stacking() {
    let mut c = a_client_on_the_wizard();
    let roots_before = wizard_roots(&mut c);

    let (raised_first, raised_second) = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let first = {
            let s = shell.flow.current_mut().expect("a screen");
            let any: &mut dyn std::any::Any = &mut **s;
            let w = any
                .downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
                .expect("the wizard");
            w.error_string_id = Some(chargen::TOD_WARNING_STRING);
            w.make_credit_warning_dialog(&mut shell.ui)
        };
        let second = {
            let s = shell.flow.current_mut().expect("a screen");
            let any: &mut dyn std::any::Any = &mut **s;
            let w = any
                .downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
                .expect("the wizard");
            w.make_error_message_dialog(&mut shell.ui)
        };
        (first, second)
    };
    c.tick(1);

    let one_on_screen = raised_first && !raised_second;
    let the_other_is_waiting = with_wizard(&mut c, |_, w| {
        w.dialog_context(CharGenDialog::ErrorMessage).is_some()
            && w.dialog_element(CharGenDialog::ErrorMessage).is_none()
            && w.dialog_element(CharGenDialog::CreditWarning).is_some()
    });
    let one_root = wizard_roots(&mut c) == roots_before + 1;

    // Answer the first, and the one that was waiting takes its place.
    let h = wizard_dialog(&mut c, CharGenDialog::CreditWarning).expect("the first is up");
    let no = dialog_child(&mut c, h, dereth_ui::dialog::base::child::BUTTON2);
    let mut hands = Hands::new();
    hands.click_handle(&mut c, no);
    c.tick(1);

    let promoted = with_wizard(&mut c, |_, w| {
        w.dialog_element(CharGenDialog::CreditWarning).is_none()
            && w.dialog_context(CharGenDialog::CreditWarning).is_none()
            && w.dialog_element(CharGenDialog::ErrorMessage).is_some()
    }) && wizard_roots(&mut c) == roots_before + 1;

    c.assert_behaviour(
        "chargen.dialogs.a-second-one-waits-its-turn-rather-than-stacking-on-the-first",
        move |_| one_on_screen && the_other_is_waiting && one_root && promoted,
    );
    c.shutdown();
}

#[test]
fn scenario_a_second_box_waits_its_turn_rather_than_stacking() {
    scenario("a_second_box_waits_its_turn_rather_than_stacking");
}

// ---------------------------------------------------------------------------------------------
// chargen.dialogs.one-closed-from-underneath-the-screen-is-replaced-rather-than-left-behind
// ---------------------------------------------------------------------------------------------

/// A box closed underneath the screen leaves no orphan on the screen.
pub fn a_box_closed_from_underneath_the_screen_is_replaced() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    click_wizard(&mut c, RANDOM_BUTTON);
    let h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning).expect("the warning");
    let ctx = with_wizard(&mut c, |_, w| {
        w.dialog_context(CharGenDialog::RandomizeWarning)
            .expect("and a place in the queue")
    });
    let roots_with_it = wizard_roots(&mut c);

    // Closed at the queue and not through the screen, which is what a reset does: the screen is
    // never told, and its own record still says the box is wanted.
    let now = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .now
        .0;
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .dialogs
        .close_dialog(ctx, now);
    let forgotten_below = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .dialogs
        .info(ctx)
        .is_none();
    let screen_still_has_it = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning) == Some(h);
    c.tick(1);

    let now_h = wizard_dialog(&mut c, CharGenDialog::RandomizeWarning)
        .expect("the screen still wants the box, so it is raised again");
    let replaced = now_h != h
        && c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .is_none()
        && with_wizard(&mut c, |_, w| {
            w.dialog_context(CharGenDialog::RandomizeWarning)
        }) != Some(ctx)
        && wizard_roots(&mut c) == roots_with_it;
    // And nothing was answered by it: a close from underneath is not a yes.
    let not_an_answer = !with_wizard(&mut c, |_, w| w.pending_random);

    c.assert_behaviour(
        "chargen.dialogs.one-closed-from-underneath-the-screen-is-replaced-rather-than-left-behind",
        move |_| forgotten_below && screen_still_has_it && replaced && not_an_answer,
    );
    c.shutdown();
}

#[test]
fn scenario_a_box_closed_from_underneath_the_screen_is_replaced() {
    scenario("a_box_closed_from_underneath_the_screen_is_replaced");
}

// =============================================================================================
// chargen.scroll.* -- the wizard's lists and panes really scroll
//
// The heritage screen's scrollbar, and every other scrollable on the wizard, really works. Three
// checks fold into the scenarios below rather than becoming rows of their own: the second list
// having its own bar, scrolling one pane not moving another, and the layout census are the same
// claim -- each scrollable drives the bar beside it -- read from three sides.
//
// **The wheel is a scroll path here**, not a press: it arrives at the scrollable rather than at the
// widget's own press handler. The two directions are asserted separately, because a wheel driven
// one way and then back cancels, and a working wheel and a dead one would read alike.
// =============================================================================================

use dereth_ui::widgets::scrollbar::attr as bar_attr;

const HERITAGE_PANE: ElementId = ElementId(0x1000_03C4);
const APPEARANCE_PANE: ElementId = ElementId(0x1000_03AB);
const SUMMARY_PANE: ElementId = ElementId(0x1000_0404);
const CHARGEN_SKILLS_LIST: ElementId = ElementId(0x1000_03F7);
const SUMMARY_LIST: ElementId = ElementId(0x1000_0400);

/// The wizard with a people and a home town chosen, so every description pane holds its text.
fn a_wizard_with_descriptions() -> HeadlessClient {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(&mut c, chargen::TOWN_BUTTONS[0].0);
    c
}

/// Every page shown once, which is what builds the lists on each of them.
fn show_every_page(c: &mut HeadlessClient) {
    for p in EcgProgress::PAGES {
        click_wizard(c, p.select_button().expect("a real page has a tab"));
        c.tick(4);
    }
}

fn box_of(c: &HeadlessClient, h: dereth_ui::ElemHandle) -> dereth_ui::Box2D {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .region
        .box_
}

/// A text pane's own scroll state.
fn text_scroll(
    c: &mut HeadlessClient,
    h: dereth_ui::ElemHandle,
) -> dereth_ui::scrollable::Scrollable {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .expect("a text pane")
        .scroll
}

/// A list box's own.
fn chargen_list_scroll(
    c: &HeadlessClient,
    h: dereth_ui::ElemHandle,
) -> dereth_ui::scrollable::Scrollable {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| b.as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .expect("a list box")
        .scroll
}

/// Which bar a scrollable really bound -- the whole of the pane-to-bar question.
fn bound_bar(
    c: &HeadlessClient,
    s: dereth_ui::scrollable::Scrollable,
    me: dereth_ui::ElemHandle,
) -> Option<dereth_ui::ElemHandle> {
    s.scrollbar(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        me,
        false,
    )
}

fn bar_float(c: &HeadlessClient, h: dereth_ui::ElemHandle, id: u32) -> Option<f32> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)?
        .merged_properties()
        .get_float(id)
}

fn bar_bool(c: &HeadlessClient, h: dereth_ui::ElemHandle, id: u32) -> Option<bool> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)?
        .merged_properties()
        .get_bool(id)
}

fn bar_enum(c: &HeadlessClient, h: dereth_ui::ElemHandle, id: u32) -> Option<ElementId> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)?
        .merged_properties()
        .get_enum(id)
        .map(ElementId)
}

/// Where the list's first row is, which is what a reader watches move.
fn first_row_y(c: &HeadlessClient, list: dereth_ui::ElemHandle) -> i32 {
    let shell = c.view().expect_app().ui().expect("the UI shell is up");
    let row = *shell.ui.children(list).first().expect("the list has rows");
    shell.ui.node(row).expect("a live row").region.box_.y0
}

/// One press of an arrow, through the arrow **button** the bar names, so that the client picks
/// which message that arrow raises rather than the scenario picking it.
fn press_arrow(c: &mut HeadlessClient, bar: dereth_ui::ElemHandle, increment: bool) {
    let which = if increment {
        bar_attr::INCREMENT_BUTTON
    } else {
        bar_attr::DECREMENT_BUTTON
    };
    let id = bar_enum(c, bar, which).expect("the bar names that arrow");
    let h = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(bar, id)
        .expect("the arrow is a child of the bar");
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_HOT_CLICK, 7, 0);
    c.tick(1);
}

/// The thumb put at a fraction of the track, telling its owner as the bar does.
fn drag_thumb_to(c: &mut HeadlessClient, bar: dereth_ui::ElemHandle, pos: f32) {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell.ui.set_attribute_float(bar, bar_attr::POSITION, pos);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let p1 = (pos * dereth_ui::widgets::scrollbar::POSITION_SCALE) as u32;
    shell
        .ui
        .broadcast_element_message(bar, dereth_ui::msg::element::id::SCROLL_POSITION, p1, 0);
    c.tick(1);
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.three-panes-and-two-lists-scroll-and-the-town-page-has-nothing-to-scroll
// ---------------------------------------------------------------------------------------------

/// What in the wizard scrolls, and what does not.
pub fn three_panes_and_two_lists_scroll_and_the_town_page_has_none() {
    let mut c = a_wizard_with_descriptions();
    show_every_page(&mut c);

    let (bars, mut scrollables) = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let root = shell.flow.current().expect("a screen").roots()[0];
        let mut stack = vec![root];
        let mut bars = 0;
        let mut scrollables = Vec::new();
        while let Some(h) = stack.pop() {
            stack.extend(shell.ui.children(h));
            let Some(n) = shell.ui.node(h) else { continue };
            if n.ty().0 == 0x0B {
                bars += 1;
            }
            let p = n.merged_properties();
            if p.get_enum(dereth_ui::scrollable::attr::H_SCROLLBAR)
                .is_some()
                || p.get_enum(dereth_ui::scrollable::attr::V_SCROLLBAR)
                    .is_some()
            {
                scrollables.push((n.element_id(), n.ty().0));
            }
        }
        (bars, scrollables)
    };
    scrollables.sort_by_key(|(id, _)| id.0);

    let twelve_bars = bars == 12;
    let five = scrollables
        == vec![
            (APPEARANCE_PANE, 0x0C),
            (HERITAGE_PANE, 0x0C),
            (CHARGEN_SKILLS_LIST, 0x05),
            (SUMMARY_LIST, 0x05),
            (SUMMARY_PANE, 0x0C),
        ];

    c.assert_behaviour(
        "chargen.scroll.three-panes-and-two-lists-scroll-and-the-town-page-has-nothing-to-scroll",
        move |_| twelve_bars && five,
    );
    c.shutdown();
}

#[test]
fn scenario_three_panes_and_two_lists_scroll_and_the_town_page_has_none() {
    scenario("three_panes_and_two_lists_scroll_and_the_town_page_has_none");
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.each-pane-drives-the-bar-beside-it-and-not-another-pages
// ---------------------------------------------------------------------------------------------

/// Three panes name the same bar and drive three different ones, and moving one moves no other.
pub fn each_pane_drives_the_bar_beside_it() {
    let mut c = a_wizard_with_descriptions();
    show_every_page(&mut c);

    let pages: [(ElementId, ElementId); 3] = [
        (HERITAGE_PANE, ElementId(0x1000_03D1)),
        (APPEARANCE_PANE, ElementId(0x1000_03D4)),
        (SUMMARY_PANE, ElementId(0x1000_03D6)),
    ];
    let mut names_the_shared_one = true;
    let mut inside_its_own_page = true;
    let mut bound = Vec::new();
    for (pane, page) in pages {
        let ph = element(&c, pane);
        let s = text_scroll(&mut c, ph);
        // They all name one id, which is why binding the right element matters at all.
        names_the_shared_one &= s.v_scrollbar == Some(ElementId(0x1000_02E7));
        let bar = bound_bar(&c, s, ph).expect("the pane binds a bar");
        let page_h = element(&c, page);
        inside_its_own_page &= c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .is_ancestor_of(page_h, bar);
        bound.push(bar);
    }
    bound.sort_unstable();
    bound.dedup();
    let three_different = bound.len() == 3;

    // The second list is not on the first list's bar either.
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    c.tick(4);
    let list = element(&c, SUMMARY_LIST);
    let s = chargen_list_scroll(&c, list);
    let overflows = s.height > box_of(&c, list).height();
    let bar = bound_bar(&c, s, list).expect("the summary list binds a bar");
    let its_own = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(bar)
        .expect("live")
        .element_id()
        == ElementId(0x1000_0401);
    let top = first_row_y(&c, list);
    press_arrow(&mut c, bar, false);
    let moved = chargen_list_scroll(&c, list).y;
    let it_scrolled = moved > 0 && first_row_y(&c, list) == top - moved;

    // And moving one pane leaves another alone, which is the cross-talk the shared id invites.
    show_every_page(&mut c);
    let summary = element(&c, SUMMARY_PANE);
    let appearance = element(&c, APPEARANCE_PANE);
    let s = text_scroll(&mut c, summary);
    let bar = bound_bar(&c, s, summary).expect("the summary pane's bar");
    let before = text_scroll(&mut c, appearance).y;
    press_arrow(&mut c, bar, false);
    let no_cross_talk =
        text_scroll(&mut c, summary).y > 0 && text_scroll(&mut c, appearance).y == before;

    c.assert_behaviour(
        "chargen.scroll.each-pane-drives-the-bar-beside-it-and-not-another-pages",
        move |_| {
            names_the_shared_one
                && inside_its_own_page
                && three_different
                && overflows
                && its_own
                && it_scrolled
                && no_cross_talk
        },
    );
    c.shutdown();
}

#[test]
fn scenario_each_pane_drives_the_bar_beside_it() {
    scenario("each_pane_drives_the_bar_beside_it");
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.a-pane-with-text-in-it-has-a-live-bar-whose-thumb-is-the-size-of-what-is-shown
// ---------------------------------------------------------------------------------------------

/// A filled pane knows how much it holds, and its bar says so.
pub fn a_filled_pane_has_a_live_bar_sized_to_what_is_shown() {
    let mut c = a_wizard_with_descriptions();
    show_every_page(&mut c);

    let mut measured = true;
    let mut bars_follow = true;
    for pane in [HERITAGE_PANE, APPEARANCE_PANE, SUMMARY_PANE] {
        let h = element(&c, pane);
        let s = text_scroll(&mut c, h);
        let b = box_of(&c, h);
        measured &= s.height > 0 && s.width > 0;
        let bar = bound_bar(&c, s, h).expect("its bar");
        let overflows = s.height > b.height();
        // A bar is dead exactly when there is nothing to scroll...
        bars_follow &= bar_bool(&c, bar, bar_attr::DISABLED) == Some(!overflows);
        // ...and the thumb covers as much of the track as the box covers of the text.
        #[allow(clippy::cast_precision_loss)]
        let want = (b.height() as f32 / s.height.max(b.height()) as f32).min(1.0);
        let got = bar_float(&c, bar, bar_attr::PROPORTION).expect("a proportion");
        bars_follow &= (got - want).abs() < 1e-4;
    }

    c.assert_behaviour("chargen.scroll.a-pane-with-text-in-it-has-a-live-bar-whose-thumb-is-the-size-of-what-is-shown", move |_| {
        measured && bars_follow
    });
    c.shutdown();
}

#[test]
fn scenario_a_filled_pane_has_a_live_bar_sized_to_what_is_shown() {
    scenario("a_filled_pane_has_a_live_bar_sized_to_what_is_shown");
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.dragging-the-thumb-moves-the-rows-by-the-same-fraction
// ---------------------------------------------------------------------------------------------

/// Half way down the track is half way down the list, and the rows really move.
pub fn dragging_the_thumb_moves_the_rows_by_the_same_fraction() {
    let mut c = a_wizard_with_descriptions();
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    let list = element(&c, CHARGEN_SKILLS_LIST);
    let s = chargen_list_scroll(&c, list);
    let view = box_of(&c, list).height();
    let travel = s.height - view;
    assert!(
        travel > 0,
        "the skill rows overflow a {view} pixel box; they are {} tall",
        s.height
    );
    let bar = bound_bar(&c, s, list).expect("the list binds its bar");
    let top = first_row_y(&c, list);

    drag_thumb_to(&mut c, bar, 0.5);
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let want = (0.5_f32 * travel as f32) as i32;
    // The offset is the fraction of the travel, and -- the half a screenshot would miss -- the
    // rows themselves moved up by exactly that many pixels.
    let half_way = chargen_list_scroll(&c, list).y == want && first_row_y(&c, list) == top - want;

    drag_thumb_to(&mut c, bar, 0.0);
    let back = chargen_list_scroll(&c, list).y == 0 && first_row_y(&c, list) == top;

    c.assert_behaviour(
        "chargen.scroll.dragging-the-thumb-moves-the-rows-by-the-same-fraction",
        move |_| half_way && back,
    );
    c.shutdown();
}

#[test]
fn scenario_dragging_the_thumb_moves_the_rows_by_the_same_fraction() {
    scenario("dragging_the_thumb_moves_the_rows_by_the_same_fraction");
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.an-arrow-moves-one-row-and-the-two-arrows-go-opposite-ways
// ---------------------------------------------------------------------------------------------

/// One press of an arrow is one row, the two arrows undo each other, and the top is the top.
pub fn an_arrow_moves_one_row_and_the_two_go_opposite_ways() {
    let mut c = a_wizard_with_descriptions();
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    let list = element(&c, CHARGEN_SKILLS_LIST);
    let s = chargen_list_scroll(&c, list);
    let bar = bound_bar(&c, s, list).expect("the bar");
    let rows = i32::try_from(
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .children(list)
            .len(),
    )
    .expect("a row count");
    let one_row = s.height / rows;
    assert!(one_row > 0, "{rows} rows over {} pixels", s.height);
    let top = first_row_y(&c, list);

    // Down: the rows move **up** past the box, which is the direction a reader means by down.
    press_arrow(&mut c, bar, false);
    let one = chargen_list_scroll(&c, list).y == one_row && first_row_y(&c, list) == top - one_row;
    press_arrow(&mut c, bar, false);
    let two = chargen_list_scroll(&c, list).y == 2 * one_row;

    // Up: the other arrow undoes it, exactly.
    press_arrow(&mut c, bar, true);
    let back_one = chargen_list_scroll(&c, list).y == one_row;
    press_arrow(&mut c, bar, true);
    let home = chargen_list_scroll(&c, list).y == 0 && first_row_y(&c, list) == top;

    // And the top is the top: another press there goes nowhere.
    press_arrow(&mut c, bar, true);
    let clamped = chargen_list_scroll(&c, list).y == 0;

    c.assert_behaviour(
        "chargen.scroll.an-arrow-moves-one-row-and-the-two-arrows-go-opposite-ways",
        move |_| one && two && back_one && home && clamped,
    );
    c.shutdown();
}

#[test]
fn scenario_an_arrow_moves_one_row_and_the_two_go_opposite_ways() {
    scenario("an_arrow_moves_one_row_and_the_two_go_opposite_ways");
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.a-press-on-the-track-moves-a-whole-page-towards-the-press
// ---------------------------------------------------------------------------------------------

/// A press on the track below the thumb pages down, and one above it pages back.
pub fn a_press_on_the_track_moves_a_whole_page_towards_the_press() {
    let mut c = a_wizard_with_descriptions();
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    let list = element(&c, CHARGEN_SKILLS_LIST);
    let s = chargen_list_scroll(&c, list);
    let bar = bound_bar(&c, s, list).expect("the bar");
    let view = box_of(&c, list).height();

    let press_track = |c: &mut HeadlessClient, below: bool| {
        let id = if below { 0x10 } else { 0x0F };
        c.app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .broadcast_element_message(bar, dereth_ui::MessageId(id), 0, 0);
        c.tick(1);
    };

    press_track(&mut c, true);
    // For a list a page is the whole box, not a box less one row.
    let a_page = chargen_list_scroll(&c, list).y == view;
    press_track(&mut c, false);
    let back = chargen_list_scroll(&c, list).y == 0;

    c.assert_behaviour(
        "chargen.scroll.a-press-on-the-track-moves-a-whole-page-towards-the-press",
        move |_| a_page && back,
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_on_the_track_moves_a_whole_page_towards_the_press() {
    scenario("a_press_on_the_track_moves_a_whole_page_towards_the_press");
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.the-wheel-moves-a-list-one-row-and-stops-at-the-top
// ---------------------------------------------------------------------------------------------

/// A wheel detent is one row, both ways, and the top is the top.
pub fn the_wheel_moves_a_list_one_row_and_stops_at_the_top() {
    let mut c = a_wizard_with_descriptions();
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    let list = element(&c, CHARGEN_SKILLS_LIST);

    let wheel = |c: &mut HeadlessClient, action: u32| {
        c.app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .broadcast_element_message(list, dereth_ui::msg::element::id::MOUSE_PRESS, action, 0);
        c.tick(1);
    };

    let at_the_top = chargen_list_scroll(&c, list).y == 0;
    // Backwards at the top: nowhere to go. **Asserted separately from the other direction**,
    // because a wheel driven both ways cancels and a dead wheel would read the same.
    wheel(&mut c, dereth_ui::focus::action::WHEEL_UP);
    let clamped = chargen_list_scroll(&c, list).y == 0;

    wheel(&mut c, dereth_ui::focus::action::WHEEL_DOWN);
    let moved = chargen_list_scroll(&c, list).y;
    let s = chargen_list_scroll(&c, list);
    let rows = i32::try_from(
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .children(list)
            .len(),
    )
    .expect("a row count");
    // ...and a detent is one row and not a whole page, which is what pressing the arrow does.
    let one_row = moved > 0 && moved == s.height / rows;

    wheel(&mut c, dereth_ui::focus::action::WHEEL_UP);
    let back = chargen_list_scroll(&c, list).y == 0;

    c.assert_behaviour(
        "chargen.scroll.the-wheel-moves-a-list-one-row-and-stops-at-the-top",
        move |_| at_the_top && clamped && one_row && back,
    );
    c.shutdown();
}

#[test]
fn scenario_the_wheel_moves_a_list_one_row_and_stops_at_the_top() {
    scenario("the_wheel_moves_a_list_one_row_and_stops_at_the_top");
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.a-pane-measures-every-line-of-its-text-its-margins-and-the-blank-row-after-it
// ---------------------------------------------------------------------------------------------

/// The people's own paragraph, measured: every wrapped line, the pane's four margins, and the
/// blank row the paragraph's closing newline leaves behind -- which is what gives the bar its
/// fourteen pixels of travel rather than none.
pub fn a_pane_measures_its_whole_paragraph_and_the_blank_row_after_it() {
    let mut c = a_wizard_with_descriptions();
    let pane = element(&c, HERITAGE_PANE);
    c.tick(4);
    let s = text_scroll(&mut c, pane);
    let b = box_of(&c, pane);
    let shipped_box = (b.width(), b.height()) == (265, 450);

    // The line table first, so the pair below is a measurement and not two magic numbers: a pane
    // that lost its margins, or the trailing row, or that broke the paragraph differently would
    // fail here with the reason named.
    let lines_are = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let t = shell
            .ui
            .text_element_mut(pane)
            .expect("the pane is a text element");
        let margins = t.margins == (15, 15, 9, 26);
        let wrapped = !t.glyphs.one_line;
        let lines = &t.glyphs.lines;
        let count = lines.len() == 31;
        let one_height = lines.iter().all(|l| l.height == 14);
        let last = *lines.last().expect("a last line");
        let trailing_is_empty =
            (last.end - last.start, last.width) == (0, 0) && last.start == t.glyphs.glyphs.len();
        let widest = lines.iter().map(|l| l.width).max() == Some(226);
        margins && wrapped && count && one_height && trailing_is_empty && widest
    };
    let measured = (s.width, s.height) == (226 + 9 + 26, 31 * 14 + 15 + 15)
        && (s.width, s.height) == (261, 464);

    let bar = bound_bar(&c, s, pane).expect("its own bar");
    let live = s.height > b.height()
        && bar_bool(&c, bar, bar_attr::DISABLED) == Some(s.height <= b.height());

    c.assert_behaviour("chargen.scroll.a-pane-measures-every-line-of-its-text-its-margins-and-the-blank-row-after-it", move |_| {
        shipped_box && lines_are && measured && live
    });
    c.shutdown();
}

#[test]
fn scenario_a_pane_measures_its_whole_paragraph_and_the_blank_row_after_it() {
    scenario("a_pane_measures_its_whole_paragraph_and_the_blank_row_after_it");
}

// =============================================================================================
// chargen.skills.* -- the wizard's skills page
//
// Five scenarios, five rows, and none of them is a transcription -- every expected name, score and
// price below is read out of the shipped skill table rather than written down, and the last one is
// the page compared against the original client's own drawing of the same character.
// =============================================================================================

use dereth_chargen::SkillAdvancementClass;
use dereth_ui_screens::screens::chargen::{ATTR_ROW_SKILL_ID, STATE_ARROW_OFF, STATE_ARROW_ON};

/// What one entry of the skills list is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillEntry {
    /// One of the four category rows, by its place in the page's own order.
    Heading(usize),
    /// A skill row, by the skill it carries.
    Row(u32),
}

/// The list in the order it is drawn, top to bottom.
fn skill_items(c: &mut HeadlessClient) -> Vec<dereth_ui::ElemHandle> {
    with_wizard(c, |_, w| {
        w.skill_list
            .as_ref()
            .expect("the skills list is bound")
            .items
            .clone()
    })
}

fn skill_entries(c: &mut HeadlessClient) -> Vec<(dereth_ui::ElemHandle, SkillEntry)> {
    let headings = with_wizard(c, |_, w| w.skill_headers.clone());
    skill_items(c)
        .into_iter()
        .map(|h| {
            if let Some(i) = headings.iter().position(|x| *x == h) {
                return (h, SkillEntry::Heading(i));
            }
            let node = c
                .view()
                .expect_app()
                .ui()
                .expect("the UI shell is up")
                .ui
                .node(h)
                .expect("a live list item");
            match node.instance_properties.get(ATTR_ROW_SKILL_ID) {
                Some(dereth_assets::ui::PropertyValue::InstanceId(v)) => (h, SkillEntry::Row(*v)),
                other => panic!("a list item that is neither a heading nor a skill row: {other:?}"),
            }
        })
        .collect()
}

/// One cell of a row, by the child the page binds.
fn skill_cell(c: &mut HeadlessClient, row: dereth_ui::ElemHandle, id: ElementId) -> String {
    let h = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(row, id)
        .unwrap_or_else(|| panic!("the skill row template has no {id:?}"));
    wizard_text(c, h)
}

/// The state of one of a row's arrows.
fn arrow_state(
    c: &HeadlessClient,
    row: dereth_ui::ElemHandle,
    id: ElementId,
) -> dereth_ui::StateId {
    let shell = c.view().expect_app().ui().expect("the UI shell is up");
    let h = shell
        .ui
        .get_child_recursive(row, id)
        .expect("the row's arrow");
    shell.ui.node(h).expect("a live node").state
}

/// A row's two prices and whether either move is possible at all, after the people's own
/// overrides -- written out here so that deleting the page's own copy cannot make a scenario
/// pass by agreeing with itself.
fn skill_prices(
    cg: &dereth_assets::tables::CharGen,
    sk: &dereth_assets::tables::SkillTable,
    heritage: u32,
    id: u32,
) -> (i32, i32, bool, bool) {
    let base = sk.skills.get(&id).expect("the skill is in the table");
    let mut train = base.trained_cost;
    let mut spec = base.specialized_cost;
    let mut untrainable = train != 0;
    let mut unspecializable = spec != 0;
    if let Some(h) = cg.heritage_groups.get(&heritage) {
        if let Some((_, normal, primary)) = h.skills.iter().find(|(s, _, _)| *s == id) {
            if *normal == 0 {
                spec -= train;
                train = 0;
                untrainable = false;
            }
            if *primary == 0 {
                spec = 0;
                unspecializable = false;
            }
        }
    }
    (train, spec, untrainable, unspecializable)
}

/// Which heading a skill belongs under, from its level and whether it can be used untrained.
fn heading_for(level: SkillAdvancementClass, min_level: u32) -> usize {
    match level {
        SkillAdvancementClass::Specialized => 0,
        SkillAdvancementClass::Trained => 1,
        SkillAdvancementClass::Untrained if min_level < 2 => 2,
        SkillAdvancementClass::Untrained => 3,
        SkillAdvancementClass::Inactive => panic!("a skill with no row has no heading"),
    }
}

/// Aluvian, the profession at `profession`, then the skills page.
fn on_the_skills_page(profession: usize) -> HeadlessClient {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        &mut c,
        EcgProgress::Profession
            .select_button()
            .expect("the profession tab"),
    );
    click_wizard(&mut c, chargen::PROFESSION_BUTTONS[profession].0);
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    assert_eq!(with_wizard(&mut c, |_, w| w.progress), EcgProgress::Skills);
    c
}

/// The four headings, in the page's own order.
const SKILL_HEADINGS: [&str; 4] = [
    "Specialized Skills",
    "Trained Skills",
    "Useable Untrained Skills",
    "Unuseable Untrained Skills",
];

// ---------------------------------------------------------------------------------------------
// chargen.skills.the-four-headings-sit-above-their-own-groups
// ---------------------------------------------------------------------------------------------

/// Heading, group, heading, group -- and each group in name order.
pub fn the_four_headings_sit_above_their_own_groups() {
    let mut c = on_the_skills_page(5);
    let (_, sk) = chargen_tables(&c);

    let entries = skill_entries(&mut c);
    let whole_table = entries.len() > 40;

    let heading_at: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, (_, e))| matches!(e, SkillEntry::Heading(_)).then_some(i))
        .collect();
    let four_in_order = heading_at.len() == 4
        && heading_at.iter().enumerate().all(|(n, i)| entries[*i].1 == SkillEntry::Heading(n))
        && heading_at[0] == 0
        // The whole finding in one line: they are **not** the first four items.
        && heading_at != vec![0, 1, 2, 3];

    let headings = with_wizard(&mut c, |_, w| w.skill_headers.clone());
    let named = headings
        .iter()
        .zip(SKILL_HEADINGS)
        .all(|(h, want)| skill_cell(&mut c, *h, chargen::skills_page::HEADER_TEXT) == want);

    // Every row sits under the heading its level and its own usability name.
    let mut group = usize::MAX;
    let mut counts = [0_usize; 4];
    let mut under_the_right_one = true;
    for (_, e) in &entries {
        match e {
            SkillEntry::Heading(i) => group = *i,
            SkillEntry::Row(id) => {
                let base = sk
                    .skills
                    .get(id)
                    .expect("the row names a skill in the table");
                let level = with_wizard(&mut c, |_, w| w.state.skill_level(*id));
                under_the_right_one &= group == heading_for(level, base.min_level);
                counts[group] += 1;
            }
        }
    }
    let populated = counts[0] >= 4 && counts[1] > 0 && counts[2] > 0;

    // And inside a group the rows are in name order.
    let mut prev: Option<&str> = None;
    let mut sorted = true;
    for (_, e) in &entries {
        match e {
            SkillEntry::Heading(_) => prev = None,
            SkillEntry::Row(id) => {
                let name = sk.skills[id].name.as_str();
                if let Some(p) = prev {
                    sorted &= p <= name;
                }
                prev = Some(name);
            }
        }
    }

    c.assert_behaviour("chargen.skills.the-four-headings-sit-above-their-own-groups-and-each-group-is-in-name-order", move |_| {
        whole_table && four_in_order && named && under_the_right_one && populated && sorted
    });
    c.shutdown();
}

#[test]
fn scenario_the_four_headings_sit_above_their_own_groups() {
    scenario("the_four_headings_sit_above_their_own_groups");
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.every-row-shows-its-own-score-and-its-own-two-prices
// ---------------------------------------------------------------------------------------------

/// The cell-by-cell reading a picture of the page cannot make: which string is in which box.
pub fn every_row_shows_its_own_score_and_its_own_two_prices() {
    let mut c = on_the_skills_page(5);
    let (cg, sk) = chargen_tables(&c);

    let entries = skill_entries(&mut c);
    let heritage = with_wizard(&mut c, |_, w| w.state.heritage_group);
    let credits = with_wizard(&mut c, |_, w| w.state.remaining_skill_credits);

    let mut every_cell = true;
    let mut no_heading_text = true;
    let mut checked: Vec<u32> = Vec::new();
    for (h, e) in &entries {
        let SkillEntry::Row(id) = e else { continue };
        let base = sk
            .skills
            .get(id)
            .expect("the row names a skill in the table");
        let (train, spec, untrainable, unspecializable) = skill_prices(&cg, &sk, heritage, *id);
        let level = with_wizard(&mut c, |_, w| w.state.skill_level(*id));
        let score = with_wizard(&mut c, |_, w| w.state.skill_score(&sk, *id));

        every_cell &= skill_cell(&mut c, *h, chargen::skills_page::ROW_NAME) == base.name;
        // A number -- the score this skill would start at -- and not a level's name and certainly
        // not a heading's.
        every_cell &= skill_cell(&mut c, *h, chargen::skills_page::ROW_LEVEL) == score.to_string();

        let printed = |v: i32| {
            if v < 999 {
                v.to_string()
            } else {
                String::new()
            }
        };
        let (up, down, up_on, down_on) = match level {
            SkillAdvancementClass::Untrained => {
                (printed(train), "0".to_string(), credits >= train, false)
            }
            SkillAdvancementClass::Trained => (
                printed(spec - train),
                train.to_string(),
                credits >= spec - train,
                untrainable,
            ),
            SkillAdvancementClass::Specialized => (
                "0".to_string(),
                (spec - train).to_string(),
                false,
                unspecializable,
            ),
            SkillAdvancementClass::Inactive => panic!("a skill with no row"),
        };
        every_cell &= skill_cell(&mut c, *h, chargen::skills_page::ROW_UP_COST) == up
            && skill_cell(&mut c, *h, chargen::skills_page::ROW_DOWN_COST) == down;
        // An arrow is lit exactly when the move it offers can be made.
        every_cell &= arrow_state(&c, *h, chargen::skills_page::ROW_INCREASE)
            == if up_on {
                STATE_ARROW_ON
            } else {
                STATE_ARROW_OFF
            }
            && arrow_state(&c, *h, chargen::skills_page::ROW_DECREASE)
                == if down_on {
                    STATE_ARROW_ON
                } else {
                    STATE_ARROW_OFF
                };

        // The defect this was reported as: a heading's words inside a row.
        for cell_id in [
            chargen::skills_page::ROW_NAME,
            chargen::skills_page::ROW_LEVEL,
            chargen::skills_page::ROW_UP_COST,
            chargen::skills_page::ROW_DOWN_COST,
        ] {
            let text = skill_cell(&mut c, *h, cell_id);
            no_heading_text &= !SKILL_HEADINGS.contains(&text.as_str());
        }
        checked.push(*id);
    }
    checked.sort_unstable();

    // Every skill the character really has a level for has exactly one row, and nothing else does.
    let mut want: Vec<u32> = with_wizard(&mut c, |_, w| {
        sk.skills
            .keys()
            .copied()
            .filter(|id| w.state.skill_level(*id) != SkillAdvancementClass::Inactive)
            .collect()
    });
    want.sort_unstable();
    let one_row_each = checked == want;

    c.assert_behaviour(
        "chargen.skills.every-row-shows-its-own-score-and-its-own-two-prices",
        move |_| every_cell && no_heading_text && one_row_each,
    );
    c.shutdown();
}

#[test]
fn scenario_every_row_shows_its_own_score_and_its_own_two_prices() {
    scenario("every_row_shows_its_own_score_and_its_own_two_prices");
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.training-one-moves-its-row-under-the-trained-heading-and-re-prices-the-rest
// ---------------------------------------------------------------------------------------------

/// Which group a row sits in now.
fn group_of_row(c: &mut HeadlessClient, row: dereth_ui::ElemHandle) -> Option<usize> {
    let mut group = None;
    for (h, e) in skill_entries(c) {
        match e {
            SkillEntry::Heading(i) => group = Some(i),
            SkillEntry::Row(_) if h == row => return group,
            SkillEntry::Row(_) => {}
        }
    }
    None
}

/// Training a skill moves its row, and re-prices every other row.
pub fn training_a_skill_moves_its_row_and_re_prices_the_rest() {
    // The hand-built profession leaves credits to spend; the one the last scenario used has none.
    let mut c = on_the_skills_page(0);
    let (_, sk) = chargen_tables(&c);

    // An affordable untrained skill, chosen off the character rather than by name.
    let (skill, row, cost) = with_wizard(&mut c, |_, w| {
        let credits = w.state.remaining_skill_credits;
        let r = w
            .skill_rows
            .iter()
            .find(|r| {
                r.level == SkillAdvancementClass::Untrained
                    && sk.skills[&r.skill].min_level < 2
                    && r.train_cost > 0
                    && r.train_cost <= credits
            })
            .expect("an affordable, useable untrained skill");
        (r.skill, r.element.expect("its row"), r.train_cost)
    });
    let starts_untrained = group_of_row(&mut c, row) == Some(2);

    let plus = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(row, chargen::skills_page::ROW_INCREASE)
        .expect("the row's + arrow");
    let credits_before = with_wizard(&mut c, |_, w| w.state.remaining_skill_credits);
    let headings = with_wizard(&mut c, |_, w| w.skill_headers.clone());
    press_handle(&mut c, plus);

    let trained = with_wizard(&mut c, |_, w| w.state.skill_level(skill))
        == SkillAdvancementClass::Trained
        && with_wizard(&mut c, |_, w| w.state.remaining_skill_credits) == credits_before - cost;
    let moved_in_the_list = group_of_row(&mut c, row) == Some(1);

    // **And in the frame, not only in the list.** A client that re-ordered what it holds and drew
    // the old picture would pass everything above.
    c.tick(4);
    let moved_on_the_screen = {
        let hs = skill_items(&mut c);
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let boxes: Vec<dereth_ui::region::Box2D> = hs
            .iter()
            .map(|h| shell.ui.node(*h).expect("a live item").region.box_)
            .collect();
        let mut y = boxes[0].y0;
        let mut stacked = true;
        for b in &boxes {
            stacked &= b.y0 == y;
            y = b.y1 + 1;
        }
        let drawn = shell.ui.node(row).expect("the trained row").region.box_.y0;
        let trained_heading = shell
            .ui
            .node(headings[1])
            .expect("the trained heading")
            .region
            .box_
            .y0;
        let untrained_heading = shell
            .ui
            .node(headings[2])
            .expect("the untrained heading")
            .region
            .box_
            .y0;
        stacked && drawn > trained_heading && drawn < untrained_heading
    };

    // Still in name order in its new group, and its own cells followed it.
    let mut group = usize::MAX;
    let mut prev: Option<&str> = None;
    let mut sorted = true;
    for (_, e) in &skill_entries(&mut c) {
        match e {
            SkillEntry::Heading(i) => {
                group = *i;
                prev = None;
            }
            SkillEntry::Row(id) => {
                let name = sk.skills[id].name.as_str();
                if let Some(p) = prev {
                    sorted &= p <= name;
                }
                prev = Some(name);
            }
        }
    }
    let _ = group;
    let score = with_wizard(&mut c, |_, w| w.state.skill_score(&sk, skill));
    let cells_followed = skill_cell(&mut c, row, chargen::skills_page::ROW_LEVEL)
        == score.to_string()
        && skill_cell(&mut c, row, chargen::skills_page::ROW_DOWN_COST) == cost.to_string();

    // The re-pricing. **One purchase is not enough to see it**: with plenty of credits left
    // nothing else becomes unaffordable, so the sweep could be missing and every reading above
    // would still hold. Spend down until at least one row really is out of reach.
    let mut bought = 1_usize;
    loop {
        let (credits, next) = with_wizard(&mut c, |_, w| {
            let credits = w.state.remaining_skill_credits;
            let next = w.skill_rows.iter().position(|r| {
                r.level == SkillAdvancementClass::Untrained
                    && r.train_cost > 0
                    && r.train_cost <= credits
            });
            (credits, next)
        });
        let priced_out = with_wizard(&mut c, |_, w| {
            w.skill_rows
                .iter()
                .any(|r| r.level == SkillAdvancementClass::Untrained && r.train_cost > credits)
        });
        if priced_out {
            break;
        }
        let Some(n) = next else { break };
        let h = with_wizard(&mut c, |_, w| w.skill_rows[n].element.expect("a row"));
        let plus = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .get_child_recursive(h, chargen::skills_page::ROW_INCREASE)
            .expect("the + arrow");
        press_handle(&mut c, plus);
        bought += 1;
        assert!(
            bought < 60,
            "the credits never ran low enough to price a skill out"
        );
    }
    let credits = with_wizard(&mut c, |_, w| w.state.remaining_skill_credits);
    let something_is_out_of_reach = with_wizard(&mut c, |_, w| {
        w.skill_rows
            .iter()
            .any(|r| r.level == SkillAdvancementClass::Untrained && r.train_cost > credits)
    });

    let mut re_priced = true;
    for (h, e) in &skill_entries(&mut c) {
        let SkillEntry::Row(id) = e else { continue };
        let (train, spec) = with_wizard(&mut c, |_, w| {
            let r = w
                .skill_rows
                .iter()
                .find(|r| r.skill == *id)
                .expect("a record");
            (r.train_cost, r.spec_cost)
        });
        let level = with_wizard(&mut c, |_, w| w.state.skill_level(*id));
        let want = match level {
            SkillAdvancementClass::Untrained => credits >= train,
            SkillAdvancementClass::Trained => credits >= spec - train,
            _ => false,
        };
        re_priced &= arrow_state(&c, *h, chargen::skills_page::ROW_INCREASE)
            == if want {
                STATE_ARROW_ON
            } else {
                STATE_ARROW_OFF
            };
    }

    c.assert_behaviour("chargen.skills.training-one-moves-its-row-under-the-trained-heading-and-re-prices-the-rest", move |_| {
        starts_untrained
            && trained
            && moved_in_the_list
            && moved_on_the_screen
            && sorted
            && cells_followed
            && something_is_out_of_reach
            && re_priced
    });
    c.shutdown();
}

#[test]
fn scenario_training_a_skill_moves_its_row_and_re_prices_the_rest() {
    scenario("training_a_skill_moves_its_row_and_re_prices_the_rest");
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.the-rows-are-stacked-one-below-another-and-none-overlaps
// ---------------------------------------------------------------------------------------------

/// One column, each item starting where the one above it ends.
pub fn the_skill_rows_are_stacked_and_none_overlaps() {
    let mut c = on_the_skills_page(5);
    let entries = skill_entries(&mut c);
    let boxes: Vec<dereth_ui::region::Box2D> = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        entries
            .iter()
            .map(|(h, _)| shell.ui.node(*h).expect("a live item").region.box_)
            .collect()
    };
    let whole_table = boxes.len() > 40;

    let mut y = boxes[0].y0;
    let mut stacked = true;
    for b in &boxes {
        stacked &= b.x0 == boxes[0].x0 && b.y0 == y && b.y1 > b.y0;
        y = b.y1 + 1;
    }
    // The row template's own height, which is the pitch the original draws at -- not the flat
    // number the page used to place its rows by hand at.
    let pitch = boxes[0].y1 - boxes[0].y0 + 1;
    let the_templates_pitch = pitch == 26;

    c.assert_behaviour(
        "chargen.skills.the-rows-are-stacked-one-below-another-and-none-overlaps",
        move |_| whole_table && stacked && the_templates_pitch,
    );
    c.shutdown();
}

#[test]
fn scenario_the_skill_rows_are_stacked_and_none_overlaps() {
    scenario("the_skill_rows_are_stacked_and_none_overlaps");
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.the-page-reads-the-way-the-original-drew-it-for-the-same-character
// ---------------------------------------------------------------------------------------------

/// The twelve items the original client draws for this character, before the list scrolls.
///
/// Each is (name, score, the price to go up, the price to go down, the up arrow lit, the down
/// arrow lit). Nothing here is derived from this client: they were read off the original's own
/// picture of the same character, and this is what catches a row showing another skill's price,
/// which no whole-picture comparison would notice.
const AS_THE_ORIGINAL_DREW_IT: [(&str, &str, &str, &str, bool, bool); 12] = [
    ("Specialized Skills", "", "", "", false, false),
    ("Dirty Fighting", "77", "0", "2", false, true),
    ("Heavy Weapons", "77", "0", "6", false, true),
    ("Melee Defense", "60", "0", "10", false, true),
    ("Shield", "110", "0", "2", false, true),
    ("Trained Skills", "", "", "", false, false),
    ("Arcane Lore", "8", "2", "0", false, false),
    ("Healing", "42", "4", "6", false, true),
    ("Jump", "105", "4", "0", false, false),
    ("Loyalty", "5", "2", "0", false, false),
    ("Magic Defense", "8", "12", "0", false, false),
    ("Missile Weapons", "55", "6", "6", false, true),
];

/// Item for item against the original's own drawing.
pub fn the_skills_page_reads_the_way_the_original_drew_it() {
    let mut c = on_the_skills_page(5);

    // The original's own credit meter reads nothing left for this character.
    let meter = {
        let f = element(&c, chargen::skills_page::CREDITS_FIELD);
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .get_child_recursive(f, chargen::skills_page::READOUT_TEXT)
            .expect("the number")
    };
    let no_credits_left = wizard_text(&mut c, meter) == "0";

    let entries = skill_entries(&mut c);
    let mut item_for_item = true;
    for (i, want) in AS_THE_ORIGINAL_DREW_IT.iter().enumerate() {
        let (h, e) = entries[i];
        match e {
            SkillEntry::Heading(_) => {
                item_for_item &= skill_cell(&mut c, h, chargen::skills_page::HEADER_TEXT) == want.0;
            }
            SkillEntry::Row(_) => {
                let got = (
                    skill_cell(&mut c, h, chargen::skills_page::ROW_NAME),
                    skill_cell(&mut c, h, chargen::skills_page::ROW_LEVEL),
                    skill_cell(&mut c, h, chargen::skills_page::ROW_UP_COST),
                    skill_cell(&mut c, h, chargen::skills_page::ROW_DOWN_COST),
                    arrow_state(&c, h, chargen::skills_page::ROW_INCREASE) == STATE_ARROW_ON,
                    arrow_state(&c, h, chargen::skills_page::ROW_DECREASE) == STATE_ARROW_ON,
                );
                item_for_item &= (
                    got.0.as_str(),
                    got.1.as_str(),
                    got.2.as_str(),
                    got.3.as_str(),
                    got.4,
                    got.5,
                ) == *want;
            }
        }
    }

    c.assert_behaviour(
        "chargen.skills.the-page-reads-the-way-the-original-drew-it-for-the-same-character",
        move |_| no_credits_left && item_for_item,
    );
    c.shutdown();
}

#[test]
fn scenario_the_skills_page_reads_the_way_the_original_drew_it() {
    scenario("the_skills_page_reads_the_way_the_original_drew_it");
}

// =============================================================================================
// chargen.skills.* -- the row moves in the picture, not only in the list
//
// This sits beside the skills scenarios because they read the client's own list and say nothing
// about the screen: a live page can put the new price and the new arrows on a row and **leave the
// row where it was** while every list reading is right.
//
// The instrument's own calibration -- a reader that says the picture follows the list must also say
// so when it does not -- is not a row; it is folded in here as the premise of the scenario it
// calibrates.
//
// Every reading below is of a rectangle on the screen, and every one is taken after the frames
// the snap-back needed: the revert happened on the frame *after* the press, so a scenario that
// pressed and looked at once could not have seen it.
// =============================================================================================

/// A press with the frames the list's own per-frame pass needs after it.
fn press_and_settle(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    c.tick(4);
}

/// Every item's own rectangle, in the order the client holds them.
fn skill_boxes(c: &mut HeadlessClient) -> Vec<dereth_ui::region::Box2D> {
    let hs = skill_items(c);
    let shell = c.view().expect_app().ui().expect("the UI shell is up");
    hs.iter()
        .map(|h| shell.ui.node(*h).expect("a live list item").region.box_)
        .collect()
}

fn top_of(c: &HeadlessClient, h: dereth_ui::ElemHandle) -> i32 {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("a live item")
        .region
        .box_
        .y0
}

/// Where the four headings are **drawn**.
fn heading_tops(c: &mut HeadlessClient) -> Vec<i32> {
    let hs = with_wizard(c, |_, w| w.skill_headers.clone());
    hs.iter().map(|h| top_of(c, *h)).collect()
}

/// Which group a row is **drawn** in: the last heading drawn above it.
///
/// Deliberately not the same question as which group the client's own list puts it in. This is
/// the one a player answers by looking at the screen.
fn drawn_group(c: &mut HeadlessClient, row: dereth_ui::ElemHandle) -> Option<usize> {
    let y = top_of(c, row);
    let mut found = None;
    for (i, t) in heading_tops(c).iter().enumerate() {
        if *t < y {
            found = Some(i);
        }
    }
    found
}

/// The picture follows the list: one column, no gaps, no overlaps, in the list's own order.
fn geometry_follows_the_list(c: &mut HeadlessClient) -> bool {
    let bs = skill_boxes(c);
    if bs.len() <= 40 {
        return false;
    }
    let mut y = bs[0].y0;
    let mut ok = true;
    for b in &bs {
        ok &= b.x0 == bs[0].x0 && b.y1 > b.y0 && b.y0 == y;
        y = b.y1 + 1;
    }
    ok
}

/// The list in the order it is drawn, top to bottom.
fn drawn_order(c: &mut HeadlessClient) -> Vec<(dereth_ui::ElemHandle, i32)> {
    let hs = skill_items(c);
    let mut v: Vec<(dereth_ui::ElemHandle, i32)> = hs.iter().map(|h| (*h, top_of(c, *h))).collect();
    v.sort_by_key(|(_, y)| *y);
    v
}

fn row_arrow(c: &HeadlessClient, row: dereth_ui::ElemHandle, up: bool) -> dereth_ui::ElemHandle {
    let id = if up {
        chargen::skills_page::ROW_INCREASE
    } else {
        chargen::skills_page::ROW_DECREASE
    };
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(row, id)
        .expect("the row's arrow")
}

/// The rows **drawn** under heading `g`, in the order they are drawn, ascending by name.
fn drawn_group_is_in_name_order(
    c: &mut HeadlessClient,
    sk: &dereth_assets::tables::SkillTable,
    g: usize,
) -> bool {
    let tops = heading_tops(c);
    let lo = tops[g];
    let hi = tops.get(g + 1).copied().unwrap_or(i32::MAX);
    let headings = with_wizard(c, |_, w| w.skill_headers.clone());
    let mut prev: Option<String> = None;
    let mut seen = 0_usize;
    let mut ok = true;
    for (h, y) in drawn_order(c) {
        if y <= lo || y >= hi || headings.contains(&h) {
            continue;
        }
        let id = {
            let shell = c.view().expect_app().ui().expect("the UI shell is up");
            match shell
                .ui
                .node(h)
                .expect("live")
                .instance_properties
                .get(ATTR_ROW_SKILL_ID)
            {
                Some(dereth_assets::ui::PropertyValue::InstanceId(v)) => *v,
                other => panic!("a drawn item that is neither a heading nor a row: {other:?}"),
            }
        };
        let name = sk.skills[&id].name.clone();
        if let Some(p) = &prev {
            ok &= p.as_str() <= name.as_str();
        }
        prev = Some(name);
        seen += 1;
    }
    ok && seen > 0
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.every-row-is-drawn-under-its-new-heading-and-back-again
// ---------------------------------------------------------------------------------------------

/// Every row of the page, driven both ways, read off the screen each time.
pub fn every_row_is_drawn_under_its_new_heading_and_back_again() {
    let mut c = on_the_skills_page(0);
    let (_, sk) = chargen_tables(&c);
    let on_entry = geometry_follows_the_list(&mut c);

    // **The calibration.** A reader that says the picture follows the list has to say so when it
    // does not, or "it agrees" and "my reader is blind" are the same reading. Two items of the
    // client's own list are swapped by hand, with nothing re-laid-out, and the reader must
    // disagree; then it is swapped back.
    {
        with_wizard(&mut c, |_, w| {
            w.skill_list.as_mut().expect("the list").items.swap(1, 2)
        });
    }
    let reader_can_disagree = !geometry_follows_the_list(&mut c);
    {
        with_wizard(&mut c, |_, w| {
            w.skill_list.as_mut().expect("the list").items.swap(1, 2)
        });
    }
    let and_agrees_again = geometry_follows_the_list(&mut c);

    let total = with_wizard(&mut c, |_, w| w.skill_rows.len());
    let mut driven = 0_usize;
    let mut out_ok = 0_usize;
    let mut back_ok = 0_usize;
    let mut trained_ok = 0_usize;
    let mut untrained_ok = 0_usize;
    let mut accounted = 0_usize;

    for i in 0..total {
        let (skill, row, cost, spec, level, untrainable, unspecializable, credits) =
            with_wizard(&mut c, |_, w| {
                let credits = w.state.remaining_skill_credits;
                let r = &w.skill_rows[i];
                (
                    r.skill,
                    r.element,
                    r.train_cost,
                    r.spec_cost,
                    r.level,
                    r.untrainable,
                    r.unspecializable,
                    credits,
                )
            });
        let Some(row) = row else { continue };
        let min_level = sk.skills[&skill].min_level;

        // Which way this row can be driven first, so that it ends where it began, and which class
        // each press lands it in. A skill a people gets free cannot be given up, so its return
        // leg cannot be driven at all and it is counted rather than pretended about.
        let (out_is_up, want_class) = match level {
            SkillAdvancementClass::Untrained if untrainable && cost > 0 && cost <= credits => {
                (true, SkillAdvancementClass::Trained)
            }
            SkillAdvancementClass::Untrained => {
                accounted += 1;
                continue;
            }
            SkillAdvancementClass::Trained if untrainable => {
                (false, SkillAdvancementClass::Untrained)
            }
            SkillAdvancementClass::Trained
                if unspecializable && spec - cost >= 0 && spec - cost <= credits =>
            {
                (true, SkillAdvancementClass::Specialized)
            }
            SkillAdvancementClass::Trained => {
                accounted += 1;
                continue;
            }
            SkillAdvancementClass::Specialized if unspecializable => {
                (false, SkillAdvancementClass::Trained)
            }
            _ => {
                accounted += 1;
                continue;
            }
        };

        let group_before = drawn_group(&mut c, row).expect("the row is drawn under a heading");
        let y_before = top_of(&c, row);
        let class_before = level;
        let want_group = heading_for(want_class, min_level);
        assert_ne!(
            want_group, group_before,
            "the row is driven across a group boundary"
        );

        // Out.
        let arrow = row_arrow(&c, row, out_is_up);
        press_and_settle(&mut c, arrow);
        let landed = with_wizard(&mut c, |_, w| w.state.skill_level(skill)) == want_class
            && geometry_follows_the_list(&mut c)
            && drawn_group(&mut c, row) == Some(want_group)
            && top_of(&c, row) != y_before
            && drawn_group_is_in_name_order(&mut c, &sk, want_group);
        if landed {
            out_ok += 1;
        }
        driven += 1;
        accounted += 1;
        if want_class == SkillAdvancementClass::Trained && out_is_up {
            trained_ok += 1;
        }
        if want_class == SkillAdvancementClass::Untrained {
            untrained_ok += 1;
        }

        // Back.
        let arrow = row_arrow(&c, row, !out_is_up);
        press_and_settle(&mut c, arrow);
        let returned = with_wizard(&mut c, |_, w| w.state.skill_level(skill)) == class_before
            && geometry_follows_the_list(&mut c)
            && drawn_group(&mut c, row) == Some(group_before)
            && top_of(&c, row) == y_before
            && with_wizard(&mut c, |_, w| w.state.remaining_skill_credits) == credits;
        if returned {
            back_ok += 1;
        }
        if class_before == SkillAdvancementClass::Trained && !out_is_up {
            trained_ok += 1;
        }
        if class_before == SkillAdvancementClass::Untrained {
            untrained_ok += 1;
        }
    }

    // Essentially the whole table was really driven, both ways, and the trained boundary is the
    // bulk of it -- without which "every driven row landed" could be a claim about two of them.
    let every_one = out_ok == driven && back_ok == driven && accounted == total;
    let enough = driven >= 37 && trained_ok >= 30 && untrained_ok >= 30;

    c.assert_behaviour(
        "chargen.skills.every-row-is-drawn-under-its-new-heading-and-back-again",
        move |_| on_entry && reader_can_disagree && and_agrees_again && every_one && enough,
    );
    c.shutdown();
}

#[test]
fn scenario_every_row_is_drawn_under_its_new_heading_and_back_again() {
    scenario("every_row_is_drawn_under_its_new_heading_and_back_again");
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.the-reported-press-moves-the-row-it-names-and-leaving-the-page-changes-nothing
// ---------------------------------------------------------------------------------------------

/// The observation as it was reported, as one gesture.
pub fn the_reported_press_moves_the_row_it_names() {
    let mut c = on_the_skills_page(0);
    let (_, sk) = chargen_tables(&c);

    // The skill is named because the report named it; every number is still read off the shipped
    // table rather than written down here.
    let skill = sk
        .skills
        .iter()
        .find(|(_, b)| b.name == "Armor Tinkering")
        .map(|(id, _)| *id)
        .expect("the shipped skill table has it");
    let i = with_wizard(&mut c, |_, w| {
        w.skill_rows
            .iter()
            .position(|r| r.skill == skill)
            .expect("it has a row on this page")
    });
    let (row, cost) = with_wizard(&mut c, |_, w| {
        (
            w.skill_rows[i].element.expect("its row"),
            w.skill_rows[i].train_cost,
        )
    });
    let credits_before = with_wizard(&mut c, |_, w| w.state.remaining_skill_credits);
    let the_reported_start =
        credits_before == 52 && cost == 4 && drawn_group(&mut c, row) == Some(2);
    let y_before = top_of(&c, row);

    let arrow = row_arrow(&c, row, true);
    press_and_settle(&mut c, arrow);

    let paid = with_wizard(&mut c, |_, w| w.state.remaining_skill_credits) == 48;
    let moved = geometry_follows_the_list(&mut c)
        && drawn_group(&mut c, row) == Some(1)
        && top_of(&c, row) != y_before;

    // And leaving the page and coming back draws the same list it was already drawing -- which is
    // what used to be the only way to see the move at all.
    let before_reentry: Vec<i32> = drawn_order(&mut c).iter().map(|(_, y)| *y).collect();
    click_wizard(
        &mut c,
        EcgProgress::Profession
            .select_button()
            .expect("the profession tab"),
    );
    c.tick(4);
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    let after_reentry: Vec<i32> = drawn_order(&mut c).iter().map(|(_, y)| *y).collect();
    let nothing_changed = before_reentry == after_reentry
        && with_wizard(&mut c, |_, w| w.state.skill_level(skill)) == SkillAdvancementClass::Trained;

    c.assert_behaviour("chargen.skills.the-reported-press-moves-the-row-it-names-and-leaving-the-page-changes-nothing", move |_| {
        the_reported_start && paid && moved && nothing_changed
    });
    c.shutdown();
}

#[test]
fn scenario_the_reported_press_moves_the_row_it_names() {
    scenario("the_reported_press_moves_the_row_it_names");
}

// =============================================================================================
// chargen.summary.* -- the two gestures into the name box
//
// Guards against *"after a click into the box, typing enters nothing"*. Three related checks are
// **not** rows here: the box coming up with the keyboard and the prompt selected, typing into it
// with no press at all, and the name surviving a trip off the page and back are
// `chargen.summary.the-name-box-prompts-takes-the-keyboard-and-the-first-
// character-replaces-the-prompt` -- and the selection read here is folded into that scenario rather
// than asserted twice.
//
// What is a row here is the two gestures into the box, and they are two rows because they use
// different machinery and a client with one of them broken passes the other.
// =============================================================================================

/// The name box's selection, as the client's own reader gives it.
fn name_selection(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> Option<(usize, usize)> {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .and_then(|t| t.get_selection())
}

/// The two bits a press and a release write: whether there is a selection at all, and whether it
/// belongs to a press that has not been let go yet.
fn selection_bits(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> (bool, bool) {
    let t = c
        .app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .expect("a text element");
    (t.bits.selecting(), t.bits.selection_from_press())
}

/// The wizard on its last page with a people settled, which is where the name box lives.
fn a_wizard_on_the_summary_page() -> HeadlessClient {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    c.tick(2);
    c
}

// ---------------------------------------------------------------------------------------------
// chargen.summary.a-press-in-the-name-box-puts-the-caret-where-it-was-pressed
// ---------------------------------------------------------------------------------------------

/// A press in a box that already holds the keyboard places the caret rather than selecting
/// everything again, so what is typed goes in at the caret.
pub fn a_press_in_the_name_box_puts_the_caret_where_it_was_pressed() {
    let mut c = a_wizard_on_the_summary_page();
    let mut hands = Hands::new();
    let name = element(&c, chargen::NAME_FIELD);
    let p = shipped_word(&c, chargen::NAME_PROMPT);
    let (x, y) = middle_of(&c, name);

    press_at(&mut c, &mut hands, x, y);
    // While the button is down there is a selection, and it is the press's own -- the two bits
    // are different things and a gesture that only ever reported its end could not tell them
    // apart.
    let while_down = selection_bits(&mut c, name) == (true, true);

    release(&mut c, &mut hands);
    let kept_the_keyboard = what_holds_the_keyboard(&c) == Some(name);
    // Letting go collapses the selection onto the caret the press placed...
    let caret_placed = name_selection(&mut c, name) == Some((p.chars().count(), p.chars().count()));
    // ...and the press's own bit is gone with the button, while the selection bit is not touched
    // -- which is why a selection made this way survives the release and can still be copied.
    let after_up = selection_bits(&mut c, name) == (true, false);

    dereth_testkit::input_steps::type_text(&mut c, "Zz");
    let typed_at_the_caret = wizard_text(&mut c, name) == format!("{p}Zz")
        && with_wizard(&mut c, |_, w| w.state.name.clone()) == format!("{p}Zz")
        && with_wizard(&mut c, |_, w| w.name_entered);

    c.assert_behaviour("chargen.summary.a-press-in-the-name-box-puts-the-caret-where-it-was-pressed-and-typing-goes-there", move |_| {
        while_down && kept_the_keyboard && caret_placed && after_up && typed_at_the_caret
    });
    c.shutdown();
}

#[test]
fn scenario_a_press_in_the_name_box_puts_the_caret_where_it_was_pressed() {
    scenario("a_press_in_the_name_box_puts_the_caret_where_it_was_pressed");
}

// ---------------------------------------------------------------------------------------------
// chargen.summary.a-press-on-nothing-takes-the-keyboard-away-and-a-press-back-in-returns-it
// ---------------------------------------------------------------------------------------------

/// The reported symptom, and the gesture that undoes it.
pub fn a_press_on_nothing_takes_the_keyboard_away_and_a_press_back_in_returns_it() {
    let mut c = a_wizard_on_the_summary_page();
    let mut hands = Hands::new();
    let name = element(&c, chargen::NAME_FIELD);
    let p = shipped_word(&c, chargen::NAME_PROMPT);
    let (x, y) = middle_of(&c, name);

    // A point on the page nothing answers for. Asserted rather than assumed: a layout change that
    // put something there would leave this scenario proving nothing.
    let empty = {
        let b = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .screen_box(name);
        ((b.x0 + b.x1) / 2, b.y1 + 16)
    };
    let really_empty = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .hit_test_screen(empty.0, empty.1)
        .is_none();

    hands.click_at(&mut c, empty.0, empty.1);
    let keyboard_let_go = what_holds_the_keyboard(&c).is_none();

    // With nothing holding the keyboard a keystroke goes nowhere, which is the symptom as it was
    // reported.
    dereth_testkit::input_steps::type_text(&mut c, "X");
    let nothing_entered = wizard_text(&mut c, name) == p;

    press_at(&mut c, &mut hands, x, y);
    release(&mut c, &mut hands);
    let keyboard_back = what_holds_the_keyboard(&c) == Some(name);

    dereth_testkit::input_steps::type_text(&mut c, "Zz");
    let now_it_enters = wizard_text(&mut c, name) == format!("{p}Zz")
        && with_wizard(&mut c, |_, w| w.state.name.clone()) == format!("{p}Zz");

    c.assert_behaviour("chargen.summary.a-press-on-nothing-takes-the-keyboard-away-and-a-press-back-in-the-box-returns-it", move |_| {
        really_empty && keyboard_let_go && nothing_entered && keyboard_back && now_it_enters
    });
    c.shutdown();
}

#[test]
fn scenario_a_press_on_nothing_takes_the_keyboard_away_and_a_press_back_in_returns_it() {
    scenario("a_press_on_nothing_takes_the_keyboard_away_and_a_press_back_in_returns_it");
}

// =============================================================================================
// chargen.summary.a-press-that-types-nothing-leaves-a-plain-caret
//
// A press into the name box that types nothing leaves a plain caret, and it stays one; the name is
// selected again only when the page is refreshed -- coming back to it, or a re-roll. How often the
// page refreshes, and which message sets the "a name has been typed" flag, are not rows of their
// own; they are asserted here from the outside, by making the gesture and watching thirty frames go
// by.
//
// The two halves are one scenario on purpose: the plain caret alone would pass on a client whose
// name box could not be re-selected at all, and the re-selection alone would pass on one that
// re-selected constantly.
// =============================================================================================

/// The press that types nothing leaves a caret, and the two gestures that do put the highlight
/// back really do.
pub fn a_press_that_types_nothing_leaves_a_plain_caret() {
    let mut c = a_wizard_on_the_summary_page();
    let name = element(&c, chargen::NAME_FIELD);
    let p = shipped_word(&c, chargen::NAME_PROMPT);
    let n = p.chars().count();
    let (x, y) = middle_of(&c, name);

    // The premise: the page came up with the whole prompt highlighted. Without it the press below
    // has nothing to collapse and this scenario measures nothing.
    let came_up_selected = name_selection(&mut c, name) == Some((0, n));
    // **Read before the press**, because a press takes the keyboard by itself: an assertion made
    // afterwards could not tell whether the page had ever taken it.
    let page_took_the_keyboard = what_holds_the_keyboard(&c) == Some(name);

    c.when(dereth_testkit::Player::Click(
        dereth_testkit::Target::Point(dereth_testkit::ScreenPoint::new(x, y)),
    ));
    let collapsed = name_selection(&mut c, name) == Some((n, n));

    let before = c.view().expect_app().frames_drawn();
    c.tick(30);
    // Thirty frames really ran: a client that had stopped ticking would satisfy everything below
    // at once.
    let still_running = c.view().expect_app().frames_drawn() - before == 30;
    let still_a_plain_caret = name_selection(&mut c, name) == Some((n, n))
        && wizard_text(&mut c, name) == p
        && what_holds_the_keyboard(&c) == Some(name)
        && !with_wizard(&mut c, |_, w| w.name_entered);

    // The other direction. Leaving the page and coming back refreshes it, and the highlight is
    // back -- so the client can put it back, and the thirty quiet frames above are a measurement.
    click_wizard(
        &mut c,
        EcgProgress::Hertage
            .select_button()
            .expect("the heritage tab"),
    );
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    let name = element(&c, chargen::NAME_FIELD);
    let coming_back_re_selects = name_selection(&mut c, name) == Some((0, n));

    // Collapse it again so the second gesture is measured from the state the first one was.
    // The press goes in through the step that spaces its gestures two seconds apart, because two
    // presses at one point inside the double-click window are a double click, which this box
    // ignores entirely -- and the reading would then be of a press that never happened.
    let (x, y) = middle_of(&c, name);
    c.when(dereth_testkit::Player::Click(
        dereth_testkit::Target::Point(dereth_testkit::ScreenPoint::new(x, y)),
    ));
    let collapsed_again = name_selection(&mut c, name) == Some((n, n));

    // And a re-roll, which is the other gesture that refreshes the page.
    with_wizard(&mut c, |_, w| w.pending_random = true);
    c.tick(1);
    let name = element(&c, chargen::NAME_FIELD);
    let a_re_roll_re_selects = name_selection(&mut c, name) == Some((0, n))
        && wizard_text(&mut c, name) == p
        && !with_wizard(&mut c, |_, w| w.name_entered);

    c.assert_behaviour("chargen.summary.a-press-that-types-nothing-leaves-a-plain-caret-and-only-a-gesture-puts-the-highlight-back", move |_| {
        came_up_selected
            && page_took_the_keyboard
            && collapsed
            && still_running
            && still_a_plain_caret
            && coming_back_re_selects
            && collapsed_again
            && a_re_roll_re_selects
    });
    c.shutdown();
}

#[test]
fn scenario_a_press_that_types_nothing_leaves_a_plain_caret() {
    scenario("a_press_that_types_nothing_leaves_a_plain_caret");
}

// =============================================================================================
// dialog-keys.* -- the two keys the pre-game screens answer
//
// Where the dialog key map is registered is not a row of its own; the census of which shipped
// elements carry an input map of their own is folded into the last scenario here, as the
// denominator that census exists to be.
//
// Every key below is a real key-down / character / key-up trio built by the client's own pump and
// delivered where the window loop delivers one, so which map wins the key is the thing under test
// rather than a fixture standing in for it. Nothing here injects a bound action, except once,
// deliberately, as a control.
// =============================================================================================

use dereth_input::fire::{walk_input_maps, InputMapEntry};
use dereth_input::spec::{activation, ControlChord, DeviceType};
use dereth_input::{ActionId, InputMapId};

/// The two actions the dialog map declares: answer, and back out.
const ACCEPT_INPUT: u32 = 0x25;
const ESCAPE_KEY: u32 = 0x27;

/// The live map stack, in the order the client walks it.
fn map_stack(c: &mut HeadlessClient) -> Vec<InputMapEntry> {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell is part of the UI shell")
        .manager
        .maps
        .entries()
        .to_vec()
}

/// Which map and action the key **the shipped dialog map binds to `want`** resolves to, against
/// the stack as it stands.
///
/// The control comes out of the shipped keymap rather than out of a number here, so this is
/// exactly the question "does the dialog map still win the key it declares". It drives no frame,
/// so it can be asked before and after a gesture.
fn dialog_key_resolves_to(c: &mut HeadlessClient, want: u32) -> Option<(InputMapId, ActionId)> {
    let entries = map_stack(c);
    let shell = c.app_mut().input_manager_mut().expect("an input shell");
    let km = &shell.manager.keymap;
    let section = km
        .section(dereth_input::MAP_DIALOG_BOXES)
        .expect("the dialog section");
    let (qc, _) = section
        .bindings()
        .iter()
        .find(|(_, a)| a.0 == want)
        .copied()
        .unwrap_or_else(|| panic!("the dialog map binds action {want:#X}"));
    // Presented the way a live press is: the release bit cleared and the live bit set.
    let live = ControlChord::new(
        qc.control,
        qc.meta_mode,
        (qc.activation & !activation::UP) | activation::LIVE,
    );
    let is_keyboard = km.device_type_of(qc.control) == Some(DeviceType::Keyboard);
    assert!(
        is_keyboard,
        "both of the dialog map's controls are keyboard controls"
    );
    walk_input_maps(&entries, &live, is_keyboard, |m| km.section(m))
        .map(|r| (r.input_map, r.action))
}

/// How many entries of the stack are the dialog map at the screens' own priority.
///
/// There can be two -- a screen's own and a focused box's -- and they are different callbacks at
/// the same priority, so only a count tells them apart from outside.
fn dialog_map_entries(c: &mut HeadlessClient) -> usize {
    map_stack(c)
        .iter()
        .filter(|e| e.map == InputMapId(9) && e.priority == 3000)
        .count()
}

/// A key pressed and released with the character the desktop makes of it in between -- which is
/// what a player's finger really sends.
fn key_trio(c: &mut HeadlessClient, hands: &mut Hands, code: KeyCode, ch: char) {
    hands.key(c, key(code), true);
    hands.character(c, ch);
    hands.key(c, key(code), false);
    c.tick(1);
}

/// The key without the character: the road a bound action takes, on its own.
fn key_only(c: &mut HeadlessClient, hands: &mut Hands, code: KeyCode) {
    hands.key(c, key(code), true);
    hands.key(c, key(code), false);
    c.tick(1);
}

fn screen_stats(c: &HeadlessClient) -> (u64, u64, u64) {
    let st = &c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats;
    (st.intro_actions, st.intro_characters, st.mode_switches)
}

fn registered_mode_maps(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .registered_mode_maps()
        .to_vec()
}

/// The maps the character list registers for itself: its own dialog keys, as the retail screen
/// does, and the scrollable controls beside them, so the mouse wheel scrolls what is under the
/// pointer with nothing focused (CD-017).
fn character_screen_maps() -> Vec<u32> {
    vec![
        dereth_input::MAP_DIALOG_BOXES.0,
        dereth_client::ui::CHARACTER_SCREEN_SCROLL_MAP,
    ]
}

/// `maps` as a set, sorted, for a comparison that does not turn on registration order.
fn sorted(mut maps: Vec<u32>) -> Vec<u32> {
    maps.sort_unstable();
    maps
}

/// Three characters, so that deleting one has something to ask about.
fn three_characters() -> dereth_ui::persist::CharacterSet {
    dereth_ui::persist::CharacterSet {
        set: ["Zephyr", "Aluvia", "Marbo"]
            .iter()
            .enumerate()
            .map(|(i, n)| dereth_ui::persist::CharacterIdentity {
                id: dereth_primitives::ObjectId(0x5000_0001 + u32::try_from(i).unwrap_or(0)),
                name: (*n).to_string(),
                seconds_grace_period: 0,
            })
            .collect(),
        num_allowed_characters: 11,
        account: "offline".into(),
        ..dereth_ui::persist::CharacterSet::default()
    }
}

fn a_client_on_character_select() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::screen(mode::CHARACTER_MANAGEMENT, 4));
    {
        let host = c.app_mut().host_state_mut();
        host.character_set = Some(three_characters());
        host.received_set = true;
        host.world_name = Some("ACEmulator".into());
    }
    c.tick(4);
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .and_then(|u| u.flow.current_mode()),
        Some(mode::CHARACTER_MANAGEMENT),
        "the character list is the screen this scenario is about"
    );
    c
}

/// The credit roll, reached with **no** character set: a set arriving re-queues the character
/// list, and the roll would then be cut short by the fixture rather than by the key under test.
fn a_client_on_the_credits() -> HeadlessClient {
    let c = HeadlessClient::new(ClientSpec::screen(mode::CREDITS, 4));
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .and_then(|u| u.flow.current_mode()),
        Some(mode::CREDITS),
        "the credit roll is the screen this scenario is about"
    );
    c
}

fn with_credits<R>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut dereth_ui_screens::screens::credits::CreditsScreen) -> R,
) -> R {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    f(any
        .downcast_mut::<dereth_ui_screens::screens::credits::CreditsScreen>()
        .expect("the credits screen is current"))
}

fn with_charmgmt_screen<R>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut CharacterManagementScreen) -> R,
) -> R {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    f(any
        .downcast_mut::<CharacterManagementScreen>()
        .expect("the character screen is current"))
}

// ---------------------------------------------------------------------------------------------
// keymap.dialog-keys.the-shipped-map-binds-only-escape-and-enter-and-each-once
// ---------------------------------------------------------------------------------------------

/// The map the pre-game screens listen on carries two keys, and no more.
///
/// It is asserted rather than assumed because it is the denominator of every scenario below: a
/// third key appearing there would make each of them stop being exhaustive.
pub fn the_dialog_map_binds_only_escape_and_enter() {
    use std::collections::BTreeMap;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    // The shell, and with it the shipped keymap, without any screen on top of it.
    c.app_mut().start_shell().expect("the UI shell comes up");
    let shell = c.app_mut().input_manager_mut().expect("an input shell");
    let section = shell
        .manager
        .keymap
        .section(dereth_input::MAP_DIALOG_BOXES)
        .expect("the dialog section");
    let mut by_action: BTreeMap<u32, usize> = BTreeMap::new();
    for (_, action) in section.bindings() {
        *by_action.entry(action.0).or_default() += 1;
    }
    let two = by_action.keys().copied().collect::<Vec<u32>>() == vec![ACCEPT_INPUT, ESCAPE_KEY];
    let each_once = by_action.values().all(|n| *n == 1);
    c.shutdown();

    // **The negative control**: in the world, where no screen before it has claimed this map,
    // neither of its two keys resolves in it -- they go to the in-game maps that used to swallow
    // them. Without this, "the pre-game screens win these keys" and "this map always wins them"
    // read alike.
    let mut w = HeadlessClient::new(ClientSpec::gameplay(4));
    let not_in_the_world = dialog_key_resolves_to(&mut w, ACCEPT_INPUT)
        .is_none_or(|(m, _)| m != InputMapId(9))
        && dialog_key_resolves_to(&mut w, ESCAPE_KEY).is_none_or(|(m, _)| m != InputMapId(9));
    // ...and they still reach something, rather than nowhere: a walk that answered nothing at all
    // would satisfy the reading above just as well.
    let they_still_arrive = dialog_key_resolves_to(&mut w, ACCEPT_INPUT).is_some()
        && dialog_key_resolves_to(&mut w, ESCAPE_KEY).is_some();

    w.assert_behaviour(
        "keymap.dialog-keys.the-shipped-map-binds-only-escape-and-enter-and-each-once",
        move |_| two && each_once && not_in_the_world && they_still_arrive,
    );
    w.shutdown();
}

#[test]
fn scenario_the_dialog_map_binds_only_escape_and_enter() {
    scenario("the_dialog_map_binds_only_escape_and_enter");
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.intro.enter-advances-the-opening-sequence-and-escape-leaves-it
// ---------------------------------------------------------------------------------------------

/// On the opening sequence, enter moves it on and escape leaves it for the character list.
pub fn enter_advances_the_opening_sequence_and_escape_leaves_it() {
    use dereth_ui_screens::screens::intro;

    let mut c = a_client_on_the_intro();
    let maps_registered = registered_mode_maps(&c) == dereth_client::ui::INTRO_INPUT_MAPS;
    // The dialog map outranks the in-game chat map that used to win this key, which is the whole
    // of why the screen never heard it.
    let enter_arrives = dialog_key_resolves_to(&mut c, ACCEPT_INPUT)
        == Some((InputMapId(9), ActionId(ACCEPT_INPUT)));

    let opened_on_the_first = intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[0]);
    let mut hands = Hands::new();

    // **The two roads are driven apart**, which is the whole design of this scenario: the opening
    // sequence is the one screen that answers a key twice, once as a bound action and once as a
    // typed character, and a scenario that only ever sent the real trio could not say which of
    // them did the work or notice one of them stopping.
    let (a0, ch0, _) = screen_stats(&c);
    let queued_before = intro_state(&mut c).1.len();
    key_only(&mut c, &mut hands, KeyCode::Enter);
    let (a1, ch1, _) = screen_stats(&c);
    let the_action_road = a1 == a0 + 1
        && ch1 == ch0
        && intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[1])
        && intro_state(&mut c).1.len() == queued_before - 1;

    hands.character(&mut c, '\r');
    c.tick(1);
    let (a2, ch2, _) = screen_stats(&c);
    let the_character_road = a2 == a1
        && ch2 == ch1 + 1
        && intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[2])
        && intro_state(&mut c).1.len() == queued_before - 2;

    let still_on_the_sequence = current_screen(&c) == Some(mode::INTRO);

    // Escape, on the other hand, leaves it -- and the screen going away takes both of its maps
    // with it, leaving only the character list's own.
    let escape_arrives =
        dialog_key_resolves_to(&mut c, ESCAPE_KEY) == Some((InputMapId(9), ActionId(ESCAPE_KEY)));
    key_trio(&mut c, &mut hands, KeyCode::Escape, '\u{1b}');
    let left_for_the_list = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT)
        && registered_mode_maps(&c) == character_screen_maps();
    // The second reading, and the only one that can see the screen's own tidying up: the other
    // map the sequence took has no second owner, so a screen that registered without
    // unregistering would leave that one behind even though its dialog map looked clean.
    let at_the_screens_priority: Vec<u32> = map_stack(&mut c)
        .iter()
        .filter(|e| e.priority == 3000)
        .map(|e| e.map.0)
        .collect();
    let nothing_left_behind = sorted(at_the_screens_priority) == sorted(character_screen_maps());

    c.assert_behaviour(
        "dialog-keys.intro.enter-advances-the-opening-sequence-and-escape-leaves-it",
        move |_| {
            maps_registered
                && enter_arrives
                && opened_on_the_first
                && the_action_road
                && the_character_road
                && still_on_the_sequence
                && escape_arrives
                && left_for_the_list
                && nothing_left_behind
        },
    );
    c.shutdown();
}

#[test]
fn scenario_enter_advances_the_opening_sequence_and_escape_leaves_it() {
    scenario("enter_advances_the_opening_sequence_and_escape_leaves_it");
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.intro.a-real-press-of-enter-advances-twice-because-two-handlers-answer-it
// ---------------------------------------------------------------------------------------------

/// One press of enter moves the sequence on by two pictures, and that is the client's own doing.
///
/// The opening screen subscribes to both roads a key takes, and both move the sequence on for
/// anything that is not the key that leaves. So a real press advances twice. It is said plainly
/// rather than smoothed over: a printable key, which no map here binds, takes the character road
/// alone and advances once -- without which "two roads" and "one road firing twice" read alike.
pub fn a_real_press_of_enter_advances_twice() {
    use dereth_ui_screens::screens::intro;

    let mut c = a_client_on_the_intro();
    let mut hands = Hands::new();
    let opened_on_the_first = intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[0]);

    key_trio(&mut c, &mut hands, KeyCode::Enter, '\r');
    let advanced_twice = intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[2]);

    let before = intro_state(&mut c).0;
    key_trio(&mut c, &mut hands, KeyCode::KeyA, 'a');
    let a_printable_advances_once =
        intro_state(&mut c).0 != before && intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[3]);

    c.assert_behaviour(
        "dialog-keys.intro.a-real-press-of-enter-advances-twice-because-two-handlers-answer-it",
        move |_| opened_on_the_first && advanced_twice && a_printable_advances_once,
    );
    c.shutdown();
}

#[test]
fn scenario_a_real_press_of_enter_advances_twice() {
    scenario("a_real_press_of_enter_advances_twice");
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.intro.the-screen-answers-the-keys-its-own-maps-carry-and-no-others
// ---------------------------------------------------------------------------------------------

/// The opening screen takes what its own maps carry, and refuses an action arriving on another.
pub fn the_opening_screen_answers_only_the_keys_its_own_maps_carry() {
    let mut c = a_client_on_the_intro();
    let (before, _, _) = screen_stats(&c);

    let mut hands = Hands::new();
    key_trio(&mut c, &mut hands, KeyCode::Enter, '\r');
    let one_press_one_action = screen_stats(&c).0 == before + 1;

    // The control: the same action delivered on a map the screen does not own. It is the map the
    // client keeps registered for the whole session, so it is an honest control rather than an
    // invented one -- and the screen must refuse it.
    let mid = screen_stats(&c).0;
    c.app_mut()
        .input_manager_mut()
        .expect("an input manager")
        .inject_action(dereth_input::InputEvent {
            action: ActionId(ESCAPE_KEY),
            input_map: InputMapId(0x1000_0009),
            toggle: dereth_input::ToggleType::OneShot,
            extent: 1.0,
            start: true,
            repeat_delta: 1,
            repeat_total: 0,
            from_key_down: false,
        });
    c.tick(1);
    let another_map_is_refused = screen_stats(&c).0 == mid;

    c.assert_behaviour(
        "dialog-keys.intro.the-screen-answers-the-keys-its-own-maps-carry-and-no-others",
        move |_| one_press_one_action && another_map_is_refused,
    );
    c.shutdown();
}

#[test]
fn scenario_the_opening_screen_answers_only_the_keys_its_own_maps_carry() {
    scenario("the_opening_screen_answers_only_the_keys_its_own_maps_carry");
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.credits.either-key-ends-the-roll-and-ends-it-once
// ---------------------------------------------------------------------------------------------

/// Enter ends the credit roll, escape ends it, and each does it exactly once.
pub fn either_key_ends_the_credit_roll_and_ends_it_once() {
    // --- enter ---------------------------------------------------------------------------
    let mut c = a_client_on_the_credits();
    let its_own_map = registered_mode_maps(&c) == vec![dereth_input::MAP_DIALOG_BOXES.0];
    let enter_arrives = dialog_key_resolves_to(&mut c, ACCEPT_INPUT)
        == Some((InputMapId(9), ActionId(ACCEPT_INPUT)));
    // The subject is alive enough to have failed: the roll is really running, on the whole
    // shipped text, and has not ended by itself.
    let running = with_credits(&mut c, |s| {
        !s.finished && s.wait_element.is_none() && s.line_count > 2000
    });
    let (intro_actions, _, switches) = screen_stats(&c);

    let mut hands = Hands::new();
    key_trio(&mut c, &mut hands, KeyCode::Enter, '\r');

    // The whole chain runs inside one frame, so by the time this reads, the screen that answered
    // has already been thrown away: the end of the roll is the observable, and it is the one the
    // player sees.
    let enter_ended_it = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT)
        && screen_stats(&c).2 == switches + 1
        && screen_stats(&c).0 == intro_actions
        && registered_mode_maps(&c) == character_screen_maps();
    c.shutdown();

    // --- escape --------------------------------------------------------------------------
    let mut c = a_client_on_the_credits();
    let escape_arrives =
        dialog_key_resolves_to(&mut c, ESCAPE_KEY) == Some((InputMapId(9), ActionId(ESCAPE_KEY)));
    let still_running = with_credits(&mut c, |s| !s.finished);
    let (intro_actions, _, switches) = screen_stats(&c);

    let mut hands = Hands::new();
    key_trio(&mut c, &mut hands, KeyCode::Escape, '\u{1b}');

    // One switch. Two deliveries of the same key would be two, which is the only observable here
    // that can count them.
    let escape_ended_it_once = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT)
        && screen_stats(&c).2 == switches + 1
        && screen_stats(&c).0 == intro_actions;

    c.assert_behaviour(
        "dialog-keys.credits.either-key-ends-the-roll-and-ends-it-once",
        move |_| {
            its_own_map
                && enter_arrives
                && running
                && enter_ended_it
                && escape_arrives
                && still_running
                && escape_ended_it_once
        },
    );
    c.shutdown();
}

#[test]
fn scenario_either_key_ends_the_credit_roll_and_ends_it_once() {
    scenario("either_key_ends_the_credit_roll_and_ends_it_once");
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.character-select.enter-is-declined-and-escape-asks-once-whether-to-quit
// ---------------------------------------------------------------------------------------------

/// On the character list, enter arrives and is declined; escape asks, and only once.
pub fn enter_is_declined_on_the_character_list_and_escape_asks_once() {
    use dereth_ui::framework::Screen as _;

    let mut c = a_client_on_character_select();
    let its_own_map = registered_mode_maps(&c) == character_screen_maps();
    let enter_arrives = dialog_key_resolves_to(&mut c, ACCEPT_INPUT)
        == Some((InputMapId(9), ActionId(ACCEPT_INPUT)));

    let mut hands = Hands::new();
    key_trio(&mut c, &mut hands, KeyCode::Enter, '\r');
    // The key really arrives -- which it could not before -- and the screen is unmoved by it.
    // Asserting that something opened would be asserting a behaviour the client does not have.
    let declined = with_charmgmt_screen(&mut c, |s| s.open_dialog.is_none())
        && current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);

    let escape_arrives =
        dialog_key_resolves_to(&mut c, ESCAPE_KEY) == Some((InputMapId(9), ActionId(ESCAPE_KEY)));
    let roots_before = with_charmgmt_screen(&mut c, |s| s.roots().len());
    key_trio(&mut c, &mut hands, KeyCode::Escape, '\u{1b}');
    // The count of boxes is what can see a double delivery: the screen's one slot for which box
    // is open cannot.
    let asked_once = with_charmgmt_screen(&mut c, |s| s.open_dialog.is_some())
        && with_charmgmt_screen(&mut c, |s| s.roots().len()) == roots_before + 1;

    key_trio(&mut c, &mut hands, KeyCode::Escape, '\u{1b}');
    let and_no_second = with_charmgmt_screen(&mut c, |s| s.roots().len()) == roots_before + 1;

    c.assert_behaviour(
        "dialog-keys.character-select.enter-is-declined-and-escape-asks-once-whether-to-quit",
        move |_| {
            its_own_map
                && enter_arrives
                && declined
                && escape_arrives
                && asked_once
                && and_no_second
        },
    );
    c.shutdown();
}

#[test]
fn scenario_enter_is_declined_on_the_character_list_and_escape_asks_once() {
    scenario("enter_is_declined_on_the_character_list_and_escape_asks_once");
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.a-box-that-takes-typing-adds-the-dialog-map-without-taking-the-keys-from-the-box
// ---------------------------------------------------------------------------------------------

/// The two shipped elements that name an input map of their own, and what focusing one does.
pub fn a_box_that_takes_typing_adds_the_dialog_map_without_taking_the_keys() {
    use dereth_primitives::AssetSource as _;
    use dereth_ui::props::attr;

    let mut c = a_client_on_character_select();

    // **The denominator first.** Whether focusing a box can register a map at all depends on
    // there being a box that names one, and the whole shipped corpus carries exactly two -- both
    // naming this same map, both leaves, neither the top of its own layout. The census is read
    // out of the data files' own directory rather than off a range written here, so a layout
    // outside any range would be counted rather than missed.
    let (layouts, elements, carriers, control) = {
        let store = c.dat_store().expect("a retail client has a store").clone();
        let master_id = dereth_primitives::DataId(0x3900_0001);
        let bytes = store.read(master_id).expect("the master property record");
        let master = <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(
            master_id, &bytes,
        )
        .expect("it decodes");
        let types = master.property_types();

        fn walk(
            e: &dereth_ui::desc::ElementDesc,
            path: String,
            top_level: bool,
            want: u32,
            hits: &mut Vec<(String, bool, bool, String)>,
        ) {
            let leaf = e.children.is_empty();
            let mut collections = vec![&e.base.properties];
            collections.extend(e.states.values().map(|s| &s.properties));
            for coll in collections {
                for (id, v) in &coll.0 {
                    if *id == want {
                        hits.push((path.clone(), top_level, leaf, format!("{v:?}")));
                    }
                }
            }
            for child in e.children.values() {
                walk(
                    child,
                    format!("{path}/{:#010X}", child.element_id.0),
                    false,
                    want,
                    hits,
                );
            }
        }

        let ids = store.ids_of(dereth_dat::DbType::UiLayout);
        let mut elements = 0_usize;
        let mut carriers = Vec::new();
        let mut control = Vec::new();
        for did in &ids {
            let raw = store
                .read(*did)
                .expect("a layout the directory lists reads");
            let l = dereth_ui::desc::LayoutDesc::read(*did, &raw, &types)
                .expect("every shipped layout decodes");
            elements += l.element_count();
            for e in l.elements.values() {
                let path = format!("{:#010X}/{:#010X}", did.0, e.element_id.0);
                walk(e, path.clone(), true, attr::INPUT_MAP, &mut carriers);
                walk(e, path, true, attr::INPUT_ACTION, &mut control);
            }
        }
        (ids.len(), elements, carriers, control)
    };
    // Without the denominators, "no element names a map" and "the walk visited nothing" are one
    // reading; and the control is the same walk pointed at a neighbouring attribute that is known
    // to be there in quantity, so a zero above would mean the walk is blind.
    let counted_the_corpus = layouts == 101 && elements == 2162 && control.len() == 41;
    let exactly_two = carriers.len() == 2
        && carriers
            .iter()
            .all(|(_, top, leaf, value)| value == "Enum(9)" && !*top && *leaf);

    // The box is reached the way a player reaches it: pick a character, press delete, and the
    // question that comes up is built out of the shared layout one of those two lives in.
    let mut hands = Hands::new();
    let row = with_charmgmt_screen(&mut c, |s| {
        s.rows
            .iter()
            .find(|r| r.name == "Marbo")
            .and_then(|r| r.element)
            .expect("Marbo's row")
    });
    hands.click_handle(&mut c, row);
    let delete = element(&c, ElementId(0x1000_039F));
    hands.click_handle(&mut c, delete);

    let dialog = with_charmgmt_screen(&mut c, |s| {
        s.dialog_element(dereth_ui_screens::screens::charmgmt::DialogContext::DeleteCharacter)
            .expect("deleting raises the question with a box to type in")
    });
    // The box and the question's own root share an id, and the recursive walk never answers the
    // element it started from, so this is the box.
    let typing_box = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(dialog, ElementId(0x2C))
        .expect("the box below the question's root");
    // Read off the live tree, so this and the census are readings of two different things.
    let it_names_the_map = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(typing_box)
        .expect("live")
        .input_map
        == Some(9);

    let before = dialog_map_entries(&mut c);
    let nothing_focused_yet = before == 1
        && !c
            .app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .focused_input_maps()
            .into_iter()
            .any(|(m, _)| m == 9);

    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .take_focus(typing_box);
    c.tick(1);

    let after = c
        .app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .focused_input_maps();
    let maps: Vec<u32> = after.iter().map(|(m, _)| *m).collect();
    let the_boxs_own_first = maps == vec![9, 0x0A, 1, 7, 8]
        && after.iter().find(|(m, _)| *m == 9).map(|(_, p)| *p) == Some(3000);
    // **Counted, not looked for**: the screen already has this map at this priority under a
    // different owner, so asking whether one is present could never have failed.
    let a_second_entry = dialog_map_entries(&mut c) == before + 1;

    // ...and it did not take the keys from the box. The box's own map carries both of these keys
    // too, and it goes in afterwards, so it walks first: enter in the box is still the box's.
    let the_box_keeps_its_keys = dialog_key_resolves_to(&mut c, ACCEPT_INPUT)
        == Some((InputMapId(7), ActionId(ACCEPT_INPUT)))
        && dialog_key_resolves_to(&mut c, ESCAPE_KEY)
            == Some((InputMapId(7), ActionId(ESCAPE_KEY)));

    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .relinquish_focus(typing_box);
    c.tick(1);
    let taken_back = c
        .app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .focused_input_maps()
        .is_empty()
        && dialog_map_entries(&mut c) == before;

    c.assert_behaviour("dialog-keys.a-box-that-takes-typing-adds-the-dialog-map-without-taking-the-keys-from-the-box", move |_| {
        counted_the_corpus
            && exactly_two
            && it_names_the_map
            && nothing_focused_yet
            && the_boxs_own_first
            && a_second_entry
            && the_box_keeps_its_keys
            && taken_back
    });
    c.shutdown();
}

#[test]
fn scenario_a_box_that_takes_typing_adds_the_dialog_map_without_taking_the_keys() {
    scenario("a_box_that_takes_typing_adds_the_dialog_map_without_taking_the_keys");
}

// =============================================================================================
// character-select.* -- the five boxes the character list raises
//
// Guards against *"the delete button doesn't do anything"*. Nine scenarios, nine rows: every one of
// them is an independent claim about a box a player is shown and what answering it does, and none
// is a transcription.
//
// Every press is a real pointer gesture and every character a real typed one, so what is under
// test is the route rather than a fixture standing in for it.
// =============================================================================================

use dereth_ui_screens::screens::charmgmt::{self, CharacterAction, DialogContext};

/// A character's own id, by the place the shard listed them in.
fn character_id(slot: usize) -> dereth_primitives::ObjectId {
    dereth_primitives::ObjectId(0x5000_0001 + u32::try_from(slot).unwrap_or(0))
}

/// The handle of a box the character screen has raised.
fn charmgmt_dialog(c: &mut HeadlessClient, which: DialogContext) -> Option<dereth_ui::ElemHandle> {
    with_charmgmt_screen(c, |s| s.dialog_element(which))
}

/// A child **below** a box's root, the way the client searches: the root itself is never the
/// answer. That matters here and nowhere else, because the typing box and the root of the
/// question it sits in share an id.
fn charmgmt_child(
    c: &HeadlessClient,
    dialog: dereth_ui::ElemHandle,
    id: ElementId,
) -> dereth_ui::ElemHandle {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(dialog, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped dialog layout"))
}

/// A word out of the shipped text table the character screen reads.
fn charmgmt_word(c: &HeadlessClient, token: &str) -> String {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .resolve_string(
            charmgmt::STRING_TABLE,
            dereth_ui::persist::preferences::token_of(token),
        )
        .unwrap_or_else(|| panic!("{token} is in the shipped text table"))
}

/// The row element a character is on.
fn charmgmt_row(c: &mut HeadlessClient, id: dereth_primitives::ObjectId) -> dereth_ui::ElemHandle {
    with_charmgmt_screen(c, |s| {
        s.rows
            .iter()
            .find(|r| r.id == id)
            .and_then(|r| r.element)
            .expect("the character has a row")
    })
}

/// A press and a release with **no frame after them**, so the caller can run the frame itself and
/// read what the screen asked for before the client drops it.
fn press_without_a_frame(c: &mut HeadlessClient, hands: &mut Hands, h: dereth_ui::ElemHandle) {
    use dereth_client::platform::keys::MouseButton;
    let (x, y) = middle_of(c, h);
    hands.move_to(c, x, y);
    for down in [true, false] {
        let m = hands.button_message(MouseButton::Left, down);
        hands.send(c, m);
    }
}

/// What the screen asked the client to do with a character, read between the frame that produces
/// it and the step that throws it away when there is no shard to send it to.
///
/// It is a pass-through and fabricates nothing: the events are the ones the **real** input shell
/// made out of the real messages, relayed so that the shell's own frame can be run from here.
#[derive(Debug, Default)]
struct ActionRelay {
    pos: (i32, i32),
    mouse_move: Option<(i32, i32)>,
    actions: Vec<dereth_input::InputEvent>,
    characters: Vec<char>,
}

impl dereth_ui::InputPump for ActionRelay {
    fn use_time(&mut self, _now: dereth_primitives::LocalTime) {}
}

impl dereth_client::ui::UiInput for ActionRelay {
    fn as_pump(&mut self) -> &mut dyn dereth_ui::InputPump {
        self
    }
    fn mouse_pos(&self) -> (i32, i32) {
        self.pos
    }
    fn take_mouse_move(&mut self) -> Option<(i32, i32)> {
        self.mouse_move.take()
    }
    fn take_mouse_left_window(&mut self) -> bool {
        false
    }
    fn take_actions(&mut self) -> Vec<dereth_input::InputEvent> {
        std::mem::take(&mut self.actions)
    }
    fn put_back_unconsumed(&mut self, _events: Vec<dereth_input::InputEvent>) {}
    fn take_characters(&mut self) -> Vec<char> {
        std::mem::take(&mut self.characters)
    }
}

fn frame_taking_character_actions(c: &mut HeadlessClient) -> Vec<CharacterAction> {
    use dereth_client::ui::UiInput;
    let now = dereth_primitives::LocalTime(1.0);
    let mut relay = ActionRelay::default();
    {
        let input = c.app_mut().input_manager_mut().expect("the input manager");
        // The one call that turns the pending messages into events, made on the real manager. The
        // relay's own is a no-op precisely so it happens exactly once, here.
        dereth_ui::InputPump::use_time(input, now);
        relay.pos = input.mouse_pos();
        relay.mouse_move = UiInput::take_mouse_move(input);
        relay.actions = UiInput::take_actions(input);
        relay.characters = UiInput::take_characters(input);
    }
    let host = c.view().expect_app().host_state().clone();
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell.frame(now, &host, &mut relay);
    shell.take_character_actions()
}

/// The opcode and body a message puts on the wire, which is what a datagram carries.
fn message_bytes<M: dereth_protocol::Message>(m: &M) -> Vec<u8> {
    let mut out = M::OPCODE.0.to_le_bytes().to_vec();
    out.extend(dereth_protocol::write_body(m).expect("the message encodes"));
    out
}

/// The two halves of the shipped sentence the character's name goes between.
fn delete_question_halves(c: &HeadlessClient) -> Vec<String> {
    let parts = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .resolve_string_variants(
            charmgmt::STRING_TABLE,
            dereth_ui::persist::preferences::token_of(charmgmt::DELETE_CONFIRMATION_STRING),
        )
        .expect("the delete question is in the shipped text table");
    assert_eq!(
        parts.len(),
        2,
        "the shipped sentence is two pieces around one name"
    );
    parts
}

const EXIT_BUTTON_ID: ElementId = ElementId(0x1000_03A4);
const DELETE_BUTTON_ID: ElementId = ElementId(0x1000_039F);
const RESTORE_BUTTON_ID: ElementId = ElementId(0x1000_039E);

/// The same three characters, and a check the list really rebuilt from them.
fn a_character_list() -> HeadlessClient {
    let mut c = a_client_on_character_select();
    assert_eq!(
        with_charmgmt_screen(&mut c, |s| s.rows.len()),
        3,
        "the list was rebuilt from the set the shard sent"
    );
    c
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.leaving-raises-a-modal-question-in-the-shipped-words
// ---------------------------------------------------------------------------------------------

/// The exit button really raises a box, and the box is modal and carries the shipped sentence.
pub fn leaving_raises_a_modal_question_in_the_shipped_words() {
    let mut c = a_character_list();
    let mut hands = Hands::new();
    let nothing_first = charmgmt_dialog(&mut c, DialogContext::ConfirmExit).is_none();

    let exit = element(&c, EXIT_BUTTON_ID);
    hands.click_handle(&mut c, exit);

    let h = charmgmt_dialog(&mut c, DialogContext::ConfirmExit)
        .expect("the exit button raises a box, not merely a note to itself");
    let recorded =
        with_charmgmt_screen(&mut c, |s| s.open_dialog) == Some(DialogContext::ConfirmExit);
    let (node_id, modal) = {
        let n = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("live");
        (n.element_id(), n.region.flags.block_clicks)
    };
    let shaped = node_id == ElementId(0x15) && modal;
    let want = charmgmt_word(&c, charmgmt::CONFIRM_EXIT_STRING);
    let body = charmgmt_child(&c, h, dereth_ui::dialog::base::child::TEXT);
    let says_it = wizard_text(&mut c, body) == want && !want.is_empty();

    c.assert_behaviour(
        "character-select.dialogs.leaving-raises-a-modal-question-in-the-shipped-words",
        move |_| nothing_first && recorded && shaped && says_it,
    );
    c.shutdown();
}

#[test]
fn scenario_leaving_raises_a_modal_question_in_the_shipped_words() {
    scenario("leaving_raises_a_modal_question_in_the_shipped_words");
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.answering-yes-to-leaving-runs-the-closing-sequence-and-no-stays
// ---------------------------------------------------------------------------------------------

/// Both answers, because one of them alone cannot tell a client that always quits from one that
/// never does.
pub fn answering_yes_to_leaving_runs_the_closing_sequence_and_no_stays() {
    // No, on its own client.
    let mut c = a_character_list();
    let mut hands = Hands::new();
    let exit = element(&c, EXIT_BUTTON_ID);
    hands.click_handle(&mut c, exit);
    let h = charmgmt_dialog(&mut c, DialogContext::ConfirmExit).expect("the question");
    let no = charmgmt_child(&c, h, dereth_ui::dialog::base::child::BUTTON2);
    hands.click_handle(&mut c, no);
    c.tick(1);
    let no_stays = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT)
        && charmgmt_dialog(&mut c, DialogContext::ConfirmExit).is_none()
        && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_none()
        && with_charmgmt_screen(&mut c, |s| s.rows.len()) == 3;
    c.shutdown();

    // Yes, on another.
    let mut c = a_character_list();
    let mut hands = Hands::new();
    let exit = element(&c, EXIT_BUTTON_ID);
    hands.click_handle(&mut c, exit);
    let h = charmgmt_dialog(&mut c, DialogContext::ConfirmExit).expect("the question");
    let yes = charmgmt_child(&c, h, dereth_ui::dialog::base::child::BUTTON1);
    hands.click_handle(&mut c, yes);
    // The closing sequence specifically, and not merely "somewhere that is not the list". No
    // further frame is run: the closing sequence is the client on its way out, and a step that
    // asserts the client is still running cannot be used past it.
    let yes_leaves = current_screen(&c) == Some(mode::EPILOGUE);

    c.assert_behaviour(
        "character-select.dialogs.answering-yes-to-leaving-runs-the-closing-sequence-and-no-stays",
        move |_| no_stays && yes_leaves,
    );
    c.shutdown();
}

#[test]
fn scenario_answering_yes_to_leaving_runs_the_closing_sequence_and_no_stays() {
    scenario("answering_yes_to_leaving_runs_the_closing_sequence_and_no_stays");
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.deleting-asks-about-the-character-that-was-picked-and-names-only-them
// ---------------------------------------------------------------------------------------------

/// The question names the character the player picked, and no other.
pub fn deleting_asks_about_the_character_that_was_picked() {
    let mut c = a_character_list();
    let mut hands = Hands::new();

    // The third character the shard listed, which is neither the first row of the sorted list nor
    // the first the shard sent -- so mixing up a place in the list with a place in the set cannot
    // pass here.
    let row = charmgmt_row(&mut c, character_id(2));
    hands.click_handle(&mut c, row);
    let picked = with_charmgmt_screen(&mut c, |s| s.selected_id) == character_id(2);

    let delete = element(&c, DELETE_BUTTON_ID);
    hands.click_handle(&mut c, delete);
    let h = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).expect("it raises a box");
    let (node_id, modal) = {
        let n = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("live");
        (n.element_id(), n.region.flags.block_clicks)
    };
    let shaped = node_id == ElementId(0x2C) && modal;

    let body = charmgmt_child(&c, h, dereth_ui::dialog::base::child::TEXT);
    let shown = wizard_text(&mut c, body);
    let halves = delete_question_halves(&c);
    let names_them = shown == format!("{}Marbo{}", halves[0], halves[1])
        && !shown.contains("Zephyr")
        && !shown.contains("Aluvia");

    // The box grew to hold the sentence and is centred: an unresized one shows its first line and
    // an uncentred one sits in a corner, and a player sees both.
    let panel = charmgmt_child(&c, h, dereth_ui::dialog::base::child::PANEL);
    let (pb, rb) = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        (
            shell.ui.node(panel).expect("live").region.box_,
            shell.ui.node(h).expect("live").region.box_,
        )
    };
    let laid_out = pb.height() > 125
        && pb.width() == 400
        && (pb.x0, pb.y0)
            == (
                rb.width() / 2 - pb.width() / 2,
                rb.height() / 2 - pb.height() / 2,
            );

    // The three things the player uses are all there, and the box really can be typed into.
    let typing_box = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_FIELD);
    let usable = typing_box != h
        && c.app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .text_element_mut(typing_box)
            .expect("a text element")
            .bits
            .editable();
    let _ = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_ACCEPT);
    let _ = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_CANCEL);

    c.assert_behaviour("character-select.dialogs.deleting-asks-about-the-character-that-was-picked-and-names-only-them", move |_| {
        picked && shaped && names_them && laid_out && usable
    });
    c.shutdown();
}

#[test]
fn scenario_deleting_asks_about_the_character_that_was_picked() {
    scenario("deleting_asks_about_the_character_that_was_picked");
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.the-question-is-modal-and-a-second-press-behind-it-reaches-nothing
// ---------------------------------------------------------------------------------------------

/// A second press on the button that raised the box does nothing, and neither does one on another.
pub fn the_question_is_modal_and_a_press_behind_it_reaches_nothing() {
    let mut c = a_character_list();
    let mut hands = Hands::new();
    let delete = element(&c, DELETE_BUTTON_ID);
    let (dx, dy) = middle_of(&c, delete);
    hands.click_handle(&mut c, delete);
    let first = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).expect("one box");
    let one_root = with_charmgmt_screen(&mut c, |s| {
        use dereth_ui::framework::Screen as _;
        s.roots().len()
    }) == 2;

    // Aimed at where the button was. It is behind a modal now, so the press cannot reach it --
    // which is itself the claim, and why this press does not go through the hit test.
    hands.click_at(&mut c, dx, dy);
    let no_second = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter) == Some(first)
        && with_charmgmt_screen(&mut c, |s| {
            use dereth_ui::framework::Screen as _;
            s.roots().len()
        }) == 2;

    // And another button behind the same modal raises nothing either.
    let exit = element(&c, EXIT_BUTTON_ID);
    let (ex, ey) = middle_of(&c, exit);
    hands.click_at(&mut c, ex, ey);
    let nothing_behind = charmgmt_dialog(&mut c, DialogContext::ConfirmExit).is_none();

    c.assert_behaviour("character-select.dialogs.the-question-is-modal-and-a-second-press-behind-it-reaches-nothing", move |_| {
        one_root && no_second && nothing_behind
    });
    c.shutdown();
}

#[test]
fn scenario_the_question_is_modal_and_a_press_behind_it_reaches_nothing() {
    scenario("the_question_is_modal_and_a_press_behind_it_reaches_nothing");
}

// ---------------------------------------------------------------------------------------------
// character-select.delete.only-the-typed-phrase-deletes-and-it-deletes-the-one-that-was-picked
// ---------------------------------------------------------------------------------------------

/// The one assertion that has to be exactly right: which character.
pub fn only_the_typed_phrase_deletes_and_it_deletes_the_one_that_was_picked() {
    let mut c = a_character_list();
    let mut hands = Hands::new();

    // The phrase is the shipped one, not a word written here.
    let phrase = charmgmt_word(&c, charmgmt::DELETE_RESPONSE_STRING);
    let same_phrase = with_charmgmt_screen(&mut c, |s| s.delete_confirmation_phrase.clone())
        .as_deref()
        == Some(phrase.as_str());

    let row = charmgmt_row(&mut c, character_id(2));
    hands.click_handle(&mut c, row);
    let delete = element(&c, DELETE_BUTTON_ID);
    hands.click_handle(&mut c, delete);
    let h = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).expect("the question");

    let typing_box = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_FIELD);
    hands.click_handle(&mut c, typing_box);
    let caret_in_the_box = what_holds_the_keyboard(&c) == Some(typing_box);

    // A wrong phrase first, so the guard is driven both ways on the same box.
    hands.type_text(&mut c, "NOPE");
    let typed = wizard_text(&mut c, typing_box) == "NOPE";
    let accept = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_ACCEPT);
    press_without_a_frame(&mut c, &mut hands, accept);
    let wrong_deletes_nothing = frame_taking_character_actions(&mut c).is_empty()
        && charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).is_none()
        && charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();

    // Now the real one, in lower case, because the comparison does not care about case.
    let delete = element(&c, DELETE_BUTTON_ID);
    hands.click_handle(&mut c, delete);
    let h = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).expect("a fresh one");
    let typing_box = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_FIELD);
    hands.click_handle(&mut c, typing_box);
    hands.type_text(&mut c, &phrase.to_lowercase());
    let typed_right = wizard_text(&mut c, typing_box) == phrase.to_lowercase();
    let accept = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_ACCEPT);
    press_without_a_frame(&mut c, &mut hands, accept);

    let actions = frame_taking_character_actions(&mut c);
    let the_right_one = actions == vec![CharacterAction::Delete(character_id(2))];
    // The waiting box is up by the time the request exists, and not afterwards.
    let waiting = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_some();

    // And the bytes: the request names the character by their **place in the shard's own list**,
    // so the id above is only right if it maps to the place the player chose. Encoded here and
    // never sent; this client has no shard.
    let (account, slot) = with_charmgmt_screen(&mut c, |s| {
        (
            s.char_set.account.clone(),
            s.char_set
                .set
                .iter()
                .position(|ch| ch.id == character_id(2))
                .expect("in the set"),
        )
    });
    let body = |slot: i32| {
        message_bytes(&dereth_protocol::login::CharacterDeleteRequest {
            account: account.clone(),
            slot_index: slot,
        })
    };
    let by_place = slot == 2 && body(2) != body(0) && body(2) != body(1);

    c.assert_behaviour("character-select.delete.only-the-typed-phrase-deletes-and-it-deletes-the-one-that-was-picked", move |_| {
        same_phrase
            && caret_in_the_box
            && typed
            && wrong_deletes_nothing
            && typed_right
            && the_right_one
            && waiting
            && by_place
    });
    c.shutdown();
}

#[test]
fn scenario_only_the_typed_phrase_deletes_and_it_deletes_the_one_that_was_picked() {
    scenario("only_the_typed_phrase_deletes_and_it_deletes_the_one_that_was_picked");
}

// ---------------------------------------------------------------------------------------------
// character-select.delete.cancelling-after-typing-the-phrase-deletes-nothing
// ---------------------------------------------------------------------------------------------

/// The player changed their mind, having typed the phrase correctly.
pub fn cancelling_after_typing_the_phrase_deletes_nothing() {
    let mut c = a_character_list();
    let mut hands = Hands::new();
    let phrase = charmgmt_word(&c, charmgmt::DELETE_RESPONSE_STRING);

    let row = charmgmt_row(&mut c, character_id(2));
    hands.click_handle(&mut c, row);
    let delete = element(&c, DELETE_BUTTON_ID);
    hands.click_handle(&mut c, delete);
    let h = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).expect("the question");
    let typing_box = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_FIELD);
    hands.click_handle(&mut c, typing_box);
    hands.type_text(&mut c, &phrase);
    // The phrase that *would* have deleted them.
    let typed = wizard_text(&mut c, typing_box) == phrase;

    let cancel = charmgmt_child(&c, h, charmgmt::TEXT_INPUT_CANCEL);
    press_without_a_frame(&mut c, &mut hands, cancel);

    let nothing_asked = frame_taking_character_actions(&mut c).is_empty();
    let closed = charmgmt_dialog(&mut c, DialogContext::DeleteCharacter).is_none()
        && charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none()
        && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_none();
    let names: Vec<String> =
        with_charmgmt_screen(&mut c, |s| s.rows.iter().map(|r| r.name.clone()).collect());
    let everyone_still_there = names == vec!["Aluvia", "Marbo", "Zephyr"]
        && with_charmgmt_screen(&mut c, |s| s.selected_id) == character_id(2);

    c.assert_behaviour(
        "character-select.delete.cancelling-after-typing-the-phrase-deletes-nothing",
        move |_| typed && nothing_asked && closed && everyone_still_there,
    );
    c.shutdown();
}

#[test]
fn scenario_cancelling_after_typing_the_phrase_deletes_nothing() {
    scenario("cancelling_after_typing_the_phrase_deletes_nothing");
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.restoring-raises-a-box-with-no-buttons-that-the-next-list-takes-down
// ---------------------------------------------------------------------------------------------

/// A box a player cannot dismiss, and the thing that dismisses it.
pub fn restoring_raises_a_box_with_no_buttons_that_the_next_list_takes_down() {
    let mut c = a_character_list();
    let mut hands = Hands::new();

    // Restoring is only offered on a character already waiting to be deleted, so the fixture puts
    // one in that state the way the shard does.
    let mut set = three_characters();
    set.set[1].seconds_grace_period = 3600;
    c.app_mut().host_state_mut().character_set = Some(set.clone());
    c.tick(1);
    let row = charmgmt_row(&mut c, character_id(1));
    hands.click_handle(&mut c, row);
    let offered = with_charmgmt_screen(&mut c, |s| s.update_buttons().restore);

    let restore = element(&c, RESTORE_BUTTON_ID);
    press_without_a_frame(&mut c, &mut hands, restore);
    let asked =
        frame_taking_character_actions(&mut c) == vec![CharacterAction::Restore(character_id(1))];

    let h = charmgmt_dialog(&mut c, DialogContext::PleaseWait).expect("it raises the waiting box");
    let node_id = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .element_id();
    let want = charmgmt_word(&c, charmgmt::PLEASE_WAIT_STRING);
    let body = charmgmt_child(&c, h, dereth_ui::dialog::base::child::TEXT);
    let shaped = node_id == ElementId(0x31) && wizard_text(&mut c, body) == want;

    // No buttons at all: none of the things a player could press is anywhere under it.
    let no_buttons = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        [
            ElementId(0x17),
            ElementId(0x19),
            charmgmt::TEXT_INPUT_ACCEPT,
            charmgmt::MESSAGE_BUTTON,
        ]
        .iter()
        .all(|id| shell.ui.get_child_recursive(h, *id).is_none())
    };

    // The shard answering with a fresh list is the box's only way down -- a box with no button
    // and nothing to close it is a stuck client.
    set.set[1].seconds_grace_period = 0;
    c.app_mut().host_state_mut().character_set = Some(set);
    c.tick(1);
    let taken_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();

    c.assert_behaviour("character-select.dialogs.restoring-raises-a-box-with-no-buttons-that-the-next-list-takes-down", move |_| {
        offered && asked && shaped && no_buttons && taken_down
    });
    c.shutdown();
}

#[test]
fn scenario_restoring_raises_a_box_with_no_buttons_that_the_next_list_takes_down() {
    scenario("restoring_raises_a_box_with_no_buttons_that_the_next_list_takes_down");
}

// ---------------------------------------------------------------------------------------------
// character-select.enter-world.a-double-press-raises-the-waiting-box-before-it-asks-to-log-on
// ---------------------------------------------------------------------------------------------

/// One press picks a character; two press in.
pub fn a_double_press_raises_the_waiting_box_before_it_asks_to_log_on() {
    use dereth_client::platform::keys::MouseButton;

    let mut c = a_character_list();
    let mut hands = Hands::new();
    let row = charmgmt_row(&mut c, character_id(0));
    let (x, y) = middle_of(&c, row);

    // One press: it picks, and it enters nothing -- which is what makes the second meaningful.
    hands.click_at(&mut c, x, y);
    let one_press_picks = with_charmgmt_screen(&mut c, |s| s.selected_id) == character_id(0)
        && charmgmt_dialog(&mut c, DialogContext::EnteringWorld).is_none();

    // The second, at the same place and soon enough to be a double.
    for down in [true, false] {
        let m = hands.button_message(MouseButton::Left, down);
        hands.send(&mut c, m);
    }
    let actions = frame_taking_character_actions(&mut c);

    let h = charmgmt_dialog(&mut c, DialogContext::EnteringWorld)
        .expect("entering the world raises the waiting box");
    let node_id = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .element_id();
    let want = charmgmt_word(&c, charmgmt::ENTERING_WORLD_STRING);
    let body = charmgmt_child(&c, h, dereth_ui::dialog::base::child::TEXT);
    let shaped = node_id == ElementId(0x31) && wizard_text(&mut c, body) == want;
    // ...and it is up by the time the request exists, naming the character that was pressed.
    let asked = actions == vec![CharacterAction::LogOn(character_id(0))];

    c.assert_behaviour("character-select.enter-world.a-double-press-raises-the-waiting-box-before-it-asks-to-log-on", move |_| {
        one_press_picks && shaped && asked
    });
    c.shutdown();
}

#[test]
fn scenario_a_double_press_raises_the_waiting_box_before_it_asks_to_log_on() {
    scenario("a_double_press_raises_the_waiting_box_before_it_asks_to_log_on");
}

// ---------------------------------------------------------------------------------------------
// character-select.dialogs.an-error-brought-in-with-the-screen-becomes-a-one-button-message
// ---------------------------------------------------------------------------------------------

/// The one box raised by the screen coming up rather than by a press.
pub fn an_error_brought_in_with_the_screen_becomes_a_one_button_message() {
    let mut c = a_character_list();
    let mut hands = Hands::new();
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .flow
        .queue_with_error(mode::CHARACTER_MANAGEMENT, "the world is full".into());
    c.tick(4);

    let h = charmgmt_dialog(&mut c, DialogContext::ErrorMessage).expect("it becomes a box");
    let node_id = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .element_id();
    let body = charmgmt_child(&c, h, dereth_ui::dialog::base::child::TEXT);
    let shaped = node_id == ElementId(0x24) && wizard_text(&mut c, body) == "the world is full";
    // One button, and it is a message box's and not a question's.
    let one_button = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(h, ElementId(0x17))
        .is_none();

    let ok = charmgmt_child(&c, h, charmgmt::MESSAGE_BUTTON);
    hands.click_handle(&mut c, ok);
    let dismissed = charmgmt_dialog(&mut c, DialogContext::ErrorMessage).is_none()
        && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_none();

    c.assert_behaviour(
        "character-select.dialogs.an-error-brought-in-with-the-screen-becomes-a-one-button-message",
        move |_| shaped && one_button && dismissed,
    );
    c.shutdown();
}

#[test]
fn scenario_an_error_brought_in_with_the_screen_becomes_a_one_button_message() {
    scenario("an_error_brought_in_with_the_screen_becomes_a_one_button_message");
}

// =============================================================================================
// pointer.wheel.* and window.focus.* -- the wheel over the chat log
//
// Guards against *"the mouse wheel produces no action at all in the running client"*. Several
// related checks are not rows: state map and priority numbers written as literals beside the
// symbols the client carries them through are transcriptions; the wheel message's own packing is
// not built by the harness, because `Hands::wheel` goes through the client's own mapping; a map id
// **no shipped element carries** would be a claim about a fixture rather than about a client; and
// the layout census is folded into the pre-game-keys scenario as the denominator it is.
//
// **The log is filled from the shipped welcome text** the chat scenarios already use, not from a
// raw recording: this crate reads only the published recordings, and the claim is about the wheel
// and not about the words.
// =============================================================================================

use dereth_ui_screens::chat::window::{ENTRY, LOG};

/// The reference server's own welcome burst, which is what a player's log holds a moment after
/// they arrive, and which the chat scenarios already fill a window from.
const WELCOME_BURST: &str = "Welcome to Asheron's Call\n  powered by ACEmulator\n\nFor more information on commands supported by this server, type @acehelp\n";

/// The arrow that walks the log **back** -- the one at the top.
const LOG_ARROW_UP: ElementId = ElementId(0x1000_0072);
/// The one at the bottom.
const LOG_ARROW_DOWN: ElementId = ElementId(0x1000_0071);

/// The line the client composes from one of the shard's system messages, made on a client of its
/// own so that nothing else is in the window that takes it.
fn a_system_line(text: &str) -> dereth_ui_screens::chat::interface::ChatMessage {
    let mut c = HeadlessClient::model();
    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::comms::CommunicationTextboxString {
            text: text.to_owned(),
            text_type: 0,
        },
    ));
    let lines = c.view().chat_lines();
    assert_eq!(lines.len(), 1, "one line per system message");
    lines[0].clone()
}

/// Fill the log past its pane and leave it at its end, the way the client does.
fn fill_the_log(c: &mut HeadlessClient) {
    let line = a_system_line(WELCOME_BURST);
    for _ in 0..3 {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the UI shell is up");
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen");
        assert!(
            !gameplay
                .recv_display_final_string_info(ui, &line)
                .is_empty(),
            "the line reached a shipped chat window"
        );
    }
    c.tick(1);
    let log = element(c, LOG);
    let (content, view) = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        (
            ui.text_element_mut(log).map_or(0, |t| t.scroll.height),
            ui.screen_box(log).height(),
        )
    };
    assert!(
        content > view,
        "the burst overflows the pane ({content} > {view})"
    );
}

/// How far down the chat log is scrolled.
fn log_scroll(c: &mut HeadlessClient) -> i32 {
    let log = element(c, LOG);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(log)
        .map_or(0, |t| t.scroll.y)
}

/// The maps a focused element has put in front of the client, ids only.
fn focused_maps(c: &mut HeadlessClient) -> Vec<u32> {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .focused_input_maps()
        .into_iter()
        .map(|(m, _)| m)
        .collect()
}

/// The order the client walks its maps in, ids only.
fn walk_order(c: &mut HeadlessClient) -> Vec<u32> {
    map_stack(c).iter().map(|e| e.map.0).collect()
}

/// The map a scrollable element puts in front of the client when it takes the keyboard.
const SCROLL_MAP: u32 = 0x0A;

/// One press, through the pointer, with the frames the gesture needs.
fn press_point(c: &mut HeadlessClient, hands: &mut Hands, at: (i32, i32)) {
    hands.click_at(c, at.0, at.1);
}

// ---------------------------------------------------------------------------------------------
// pointer.wheel.one-detent-over-the-chat-log-moves-it-one-line-and-the-other-way-puts-it-back
// ---------------------------------------------------------------------------------------------

/// One turn of the wheel is one line, the same line an arrow takes, and the two ways are exact
/// inverses -- with the end of the log a clamp rather than a dead wheel.
pub fn one_detent_over_the_chat_log_moves_it_one_line() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    fill_the_log(&mut c);
    let mut hands = Hands::new();

    // The arrow's own step first, from the state the wheel will be driven from, so the two are
    // comparable -- a client that wheeled by a whole page would look right in a picture.
    let at_end = log_scroll(&mut c);
    assert!(
        at_end > 0,
        "the log is at its end, so there is somewhere to go back to"
    );
    let up = element(&c, LOG_ARROW_UP);
    let up_at = middle_of(&c, up);
    press_point(&mut c, &mut hands, up_at);
    let one_line = at_end - log_scroll(&mut c);
    let an_arrow_moves_it = one_line > 0;

    // Back to the end.
    let down = element(&c, LOG_ARROW_DOWN);
    let down_at = middle_of(&c, down);
    for _ in 0..64 {
        if log_scroll(&mut c) >= at_end {
            break;
        }
        press_point(&mut c, &mut hands, down_at);
    }
    let back_at_the_end = log_scroll(&mut c) == at_end;

    // The player is typing, which is what puts the scrolling map in front of the client.
    let entry = element(&c, ENTRY);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .take_focus(entry);
    c.tick(1);
    let armed =
        what_holds_the_keyboard(&c) == Some(entry) && walk_order(&mut c).contains(&SCROLL_MAP);

    // The pointer over the log itself, and not over its bar: a bar is a button and takes its own
    // presses.
    let log = element(&c, LOG);
    let over_log = middle_of(&c, log);
    hands.move_to(&mut c, over_log.0, over_log.1);
    c.tick(1);
    let over_the_log = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .mouse_over()
        == Some(log);

    // Backwards from the end: clamped, **not** lost. The count of presses that reached the tree
    // is the denominator without which a clamped wheel and a dead one are one reading.
    let downs_before = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .mouse_downs;
    hands.wheel(&mut c, -1.0);
    c.tick(1);
    let clamped = log_scroll(&mut c) == at_end
        && c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .stats
            .mouse_downs
            == downs_before + 1;

    hands.wheel(&mut c, 1.0);
    c.tick(1);
    let one_detent_is_one_line = at_end - log_scroll(&mut c) == one_line;

    hands.wheel(&mut c, -1.0);
    c.tick(1);
    let the_exact_inverse = log_scroll(&mut c) == at_end;

    for _ in 0..5 {
        hands.wheel(&mut c, 1.0);
        c.tick(1);
    }
    let five_is_five = at_end - log_scroll(&mut c) == one_line * 5;

    c.assert_behaviour("pointer.wheel.one-detent-over-the-chat-log-moves-it-one-line-and-the-other-way-puts-it-back", move |_| {
        an_arrow_moves_it
            && back_at_the_end
            && armed
            && over_the_log
            && clamped
            && one_detent_is_one_line
            && the_exact_inverse
            && five_is_five
    });
    c.shutdown();
}

#[test]
fn scenario_one_detent_over_the_chat_log_moves_it_one_line() {
    scenario("one_detent_over_the_chat_log_moves_it_one_line");
}

// ---------------------------------------------------------------------------------------------
// pointer.wheel.pressing-the-log-is-what-lets-the-wheel-move-it-and-typing-does-not-take-it-away
// ---------------------------------------------------------------------------------------------

/// Four states of the same client, and both halves read at each: which maps are in front of it,
/// and what a real detent does to the log.
pub fn pressing_the_log_is_what_lets_the_wheel_move_it() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    fill_the_log(&mut c);
    let log = element(&c, LOG);
    let entry = element(&c, ENTRY);
    let over_log = middle_of(&c, log);
    let mut hands = Hands::new();

    // The log really is the case the whole thing turns on: it can be picked at but not typed
    // into, read off the shipped layout rather than assumed.
    let the_log_is_what_it_is = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        let t = ui.text_element_mut(log).expect("the log is a text element");
        t.bits.selectable() && !t.bits.editable()
    };

    // Nothing holding the keyboard: no map, and the detent does nothing.
    hands.move_to(&mut c, over_log.0, over_log.1);
    c.tick(1);
    let at_start = log_scroll(&mut c);
    let nothing_yet = what_holds_the_keyboard(&c).is_none() && focused_maps(&mut c).is_empty();
    hands.wheel(&mut c, 1.0);
    c.tick(1);
    let inert = log_scroll(&mut c) == at_start;

    // Pressing the log: the scrolling map goes in and the one that swallows typing does not,
    // because the log is not a box that can be typed into.
    press_point(&mut c, &mut hands, over_log);
    let pressing_arms_it =
        what_holds_the_keyboard(&c) == Some(log) && focused_maps(&mut c) == vec![SCROLL_MAP, 8];

    // ...and the picture. One detent is one arrow's step.
    let up = element(&c, LOG_ARROW_UP);
    let up_at = middle_of(&c, up);
    let before_arrow = log_scroll(&mut c);
    press_point(&mut c, &mut hands, up_at);
    let one_line = before_arrow - log_scroll(&mut c);
    let the_arrow_moved_it = one_line > 0;
    // Pressing the arrow moved the keyboard to the arrow, which is itself a thing that scrolls --
    // so the map survives.
    let the_arrow_keeps_it = focused_maps(&mut c).contains(&SCROLL_MAP);
    hands.move_to(&mut c, over_log.0, over_log.1);
    c.tick(1);
    let before_wheel = log_scroll(&mut c);
    hands.wheel(&mut c, 1.0);
    c.tick(1);
    let a_detent_is_a_line = before_wheel - log_scroll(&mut c) == one_line;

    // Typing into the entry instead: all four maps go in, and the wheel over the log still works
    // -- which is the case that used to be the *only* one that did.
    let at_entry = middle_of(&c, entry);
    press_point(&mut c, &mut hands, at_entry);
    let typing_arms_it = what_holds_the_keyboard(&c) == Some(entry)
        && focused_maps(&mut c) == vec![SCROLL_MAP, 1, 7, 8];
    hands.move_to(&mut c, over_log.0, over_log.1);
    c.tick(1);
    let before_wheel = log_scroll(&mut c);
    hands.wheel(&mut c, 1.0);
    c.tick(1);
    let still_works_while_typing = before_wheel - log_scroll(&mut c) == one_line;

    // And letting the keyboard go takes the whole set back with it.
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .relinquish_focus(entry);
    c.tick(1);
    let disarmed = focused_maps(&mut c).is_empty();
    let before_wheel = log_scroll(&mut c);
    hands.wheel(&mut c, 1.0);
    c.tick(1);
    let inert_again = log_scroll(&mut c) == before_wheel;

    c.assert_behaviour("pointer.wheel.pressing-the-log-is-what-lets-the-wheel-move-it-and-typing-does-not-take-it-away", move |_| {
        the_log_is_what_it_is
            && nothing_yet
            && inert
            && pressing_arms_it
            && the_arrow_moved_it
            && the_arrow_keeps_it
            && a_detent_is_a_line
            && typing_arms_it
            && still_works_while_typing
            && disarmed
            && inert_again
    });
    c.shutdown();
}

#[test]
fn scenario_pressing_the_log_is_what_lets_the_wheel_move_it() {
    scenario("pressing_the_log_is_what_lets_the_wheel_move_it");
}

// ---------------------------------------------------------------------------------------------
// pointer.wheel.the-arming-happens-when-the-keyboard-moves-and-once-per-move
// ---------------------------------------------------------------------------------------------

/// The maps go in when the keyboard moves, once, and not again every frame.
pub fn the_arming_happens_when_the_keyboard_moves_and_once_per_move() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let log = element(&c, LOG);
    let mut hands = Hands::new();
    let before = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .focused_map_edges;

    let at = middle_of(&c, log);
    press_point(&mut c, &mut hands, at);
    let after_press = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .focused_map_edges;
    let one_gain_one_arming = after_press == before + 1;

    c.tick(8);
    // Eight more frames with the keyboard where it is arm nothing further: re-arming every frame
    // would throw away what the maps are holding on to.
    let quiet = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .focused_map_edges
        == after_press;

    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .relinquish_focus(log);
    c.tick(1);
    let the_other_edge = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .focused_map_edges
        == after_press + 1;

    c.assert_behaviour(
        "pointer.wheel.the-arming-happens-when-the-keyboard-moves-and-once-per-move",
        move |_| one_gain_one_arming && quiet && the_other_edge,
    );
    c.shutdown();
}

#[test]
fn scenario_the_arming_happens_when_the_keyboard_moves_and_once_per_move() {
    scenario("the_arming_happens_when_the_keyboard_moves_and_once_per_move");
}

// ---------------------------------------------------------------------------------------------
// window.focus.a-window-remembers-the-box-that-held-the-keyboard-and-gives-it-back
// ---------------------------------------------------------------------------------------------

/// A window keeps the caret's place while it is put aside, and hands it back when it is worked in
/// again.
pub fn a_window_remembers_the_box_that_held_the_keyboard() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let root = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .flow
        .current()
        .expect("a screen")
        .roots()[0];
    let entry = element(&c, ENTRY);

    let remembered_on_the_window = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.activate(root);
        ui.take_focus(entry);
        // The window remembers the box itself, not the child on the way down to it -- and the box
        // is not the window's own child, so storing the wrong one would be invisible in a flat
        // tree.
        ui.node(root).expect("the window").focus_descendant == Some(entry)
            && ui.parent(entry) != Some(root)
            && ui.focus_element() == Some(entry)
    };

    // Put aside: the caret goes, and the memory of it does not.
    let put_aside = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.deactivate(root);
        ui.focus_element().is_none()
            && ui.node(root).expect("the window").focus_descendant == Some(entry)
    };

    // Worked in again: the caret comes back to where it was.
    let handed_back = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.activate(root);
        ui.focus_element() == Some(entry)
    };

    // ...and a window already being worked in does not have the caret put back into it: moving
    // the caret away and working in the same window again leaves it where the player put it.
    let not_re_done = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.set_focus_element(None);
        ui.activate(root);
        ui.focus_element().is_none()
    };
    let listed = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .activatable_elements()
        .contains(&root);

    // **The case the two paths do not collapse into one**: a window that loses to *another*
    // window. Its own tail is the only thing that takes its caret away then.
    let lost_to_another = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        let other = ui
            .get_child_recursive(root, LOG_ARROW_UP)
            .expect("the log's arrow is in the shipped layout");
        let off_the_chain = !ui.is_ancestor_of(other, entry);
        ui.node_mut(other)
            .expect("the arrow")
            .flags
            .set_is_root_element(true);

        ui.deactivate(root);
        ui.activate(root);
        ui.take_focus(entry);
        let precondition = ui.focus_element() == Some(entry) && ui.active_element() == Some(root);

        ui.activate(other);
        let took_over = ui.active_element() == Some(other) && ui.focus_element().is_none();
        let still_remembered = ui.node(root).expect("the window").focus_descendant == Some(entry);
        ui.activate(root);
        let came_back = ui.focus_element() == Some(entry);

        ui.node_mut(other)
            .expect("the arrow")
            .flags
            .set_is_root_element(false);
        ui.set_focus_element(None);
        off_the_chain && precondition && took_over && still_remembered && came_back
    };

    // And working in something inside a window works in the **window**, never the thing itself.
    let the_window_not_the_thing = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.deactivate(root);
        let cleared = ui.active_element().is_none();
        let not_a_window = !ui.node(entry).expect("the box").flags.is_root_element();
        let forwarded = ui.activate(entry);
        cleared
            && not_a_window
            && forwarded
            && ui.active_element() == Some(root)
            && !ui.node(entry).expect("the box").flags.is_active()
    };

    c.assert_behaviour(
        "window.focus.a-window-remembers-the-box-that-held-the-keyboard-and-gives-it-back",
        move |_| {
            remembered_on_the_window
                && put_aside
                && handed_back
                && not_re_done
                && listed
                && lost_to_another
                && the_window_not_the_thing
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_window_remembers_the_box_that_held_the_keyboard() {
    scenario("a_window_remembers_the_box_that_held_the_keyboard");
}

// =============================================================================================
// options.key-bindings.* -- the rest of the key-bindings page
//
// The page is whole: the cells read a key name rather than a symbol, the sections read a word
// rather than a token, undo is dark until something has changed, and pressing a cell does
// something. Eleven scenarios, eleven rows; none of them is a transcription and every gesture is a
// real press through the hit test or a real key through the client's own pump.
//
// The page's first three claims are in the `options.key-bindings.*` section above; these are the
// rest.
// =============================================================================================

use dereth_input::spec::{ControlCode, SubControlIndex};
use dereth_ui_screens::options::keybinding::{control_name, RowDialog, DIALOG_QUEUE};

/// The movement map and the walk-forward action, which every scenario below rebinds.
const KB_MOVEMENT: InputMapId = InputMapId(4);
const KB_FORWARD: ActionId = ActionId(0x29);
const KB_TURN_LEFT: ActionId = ActionId(0x2F);
const KB_UI_COMMANDS: InputMapId = InputMapId(0x1000_0009);
const KB_ESCAPE_ACTION: ActionId = ActionId(0x27);
const KB_USE_SELECTED: ActionId = ActionId(0x1000_0025);
const KB_W: u16 = 0x11;
const KB_A: u16 = 0x1E;
/// The one function key no shipped binding uses.
const KB_F7: u16 = 0x41;
const KB_DIGIT_1: u16 = 0x02;
const KB_SPELL_BAR: InputMapId = InputMapId(0x1000_0005);
const KB_SPELL_SLOT_1: ActionId = ActionId(0x1000_0065);
const KB_QUICK_SLOTS: InputMapId = InputMapId(0x1000_000C);
const KB_QUICKSLOT_1: ActionId = ActionId(0x1000_0042);
/// The one key the shipped text table gives a name of its own.
const KB_LEFT_CONTROL: u16 = 0x1D;

/// The two states the undo button is written into: greyed, and live.
const KB_GREYED: dereth_ui::StateId = dereth_ui::StateId(0x0D);
const KB_LIVE: dereth_ui::StateId = dereth_ui::StateId(1);

/// A client in the world with a settings directory of its own, which is where the page's own
/// files are written and read.
fn a_client_on_the_key_bindings(tag: &str) -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay_in_world(6).with_scratch_settings(tag))
}

fn kb_screen(
    c: &mut dereth_client::app::App,
) -> &mut dereth_ui_screens::screens::gameplay::GamePlayScreen {
    let shell = c.ui_mut().expect("the UI shell is up");
    let s = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("the gameplay screen")
}

fn kb_ui(c: &mut dereth_client::app::App) -> &mut dereth_ui::UiSystem {
    &mut c.ui_mut().expect("the UI shell is up").ui
}

fn kb_text(c: &mut dereth_client::app::App, h: dereth_ui::ElemHandle) -> String {
    kb_ui(c)
        .text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

fn kb_state(c: &mut dereth_client::app::App, h: dereth_ui::ElemHandle) -> dereth_ui::StateId {
    kb_ui(c)
        .node(h)
        .map(|n| n.state)
        .expect("the element is in the tree")
}

/// The walk-forward row's first key cell.
fn forward_cell(c: &mut dereth_client::app::App) -> dereth_ui::ElemHandle {
    let s = kb_screen(c);
    let i = s
        .key_bindings
        .row_of(KB_MOVEMENT, KB_FORWARD)
        .expect("walking forward has a row");
    s.key_bindings.rows[i].key_buttons[0]
}

/// One row's key cells as **the letters actually on those cells** -- the grid a player looks at,
/// and not what the client holds underneath it.
fn drawn_cells(c: &mut dereth_client::app::App, map: InputMapId, action: ActionId) -> Vec<String> {
    let cells = {
        let s = kb_screen(c);
        let i = s
            .key_bindings
            .row_of(map, action)
            .expect("the action has a row on the page");
        s.key_bindings.rows[i].key_buttons.clone()
    };
    cells.into_iter().map(|h| kb_text(c, h)).collect()
}

/// The caption the page writes for one plain key, through the same resolver a row uses -- so an
/// expectation below is never a second guess at what the client would say.
fn kb_caption(c: &mut dereth_client::app::App, offset: u16) -> String {
    control_name(
        kb_ui(c),
        DeviceType::Keyboard,
        ControlCode::new(0, SubControlIndex::None, offset),
        false,
    )
}

/// Show an element and every ancestor of it, so a press can reach it: the key-bindings page is a
/// tab of the options window and is not on the screen by default.
fn kb_reveal(c: &mut dereth_client::app::App, mut h: dereth_ui::ElemHandle) {
    {
        let u = kb_ui(c);
        loop {
            u.set_visible(h, true);
            match u.parent(h) {
                Some(p) => h = p,
                None => break,
            }
        }
    }
    c.frame();
}

/// A real press: the element's own centre, hit-tested, then the button down and up.
fn kb_press(c: &mut dereth_client::app::App, h: dereth_ui::ElemHandle) {
    kb_reveal(c, h);
    let (cx, cy) = {
        let u = kb_ui(c);
        let b = u.screen_box(h);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    assert_eq!(
        kb_ui(c).hit_test_screen(cx, cy),
        Some(h),
        "the press must land on the element itself, not on something drawn over it"
    );
    {
        let u = kb_ui(c);
        u.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
        u.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
    }
    c.frame();
    c.frame();
}

/// The right-hand press, which is what erases a binding.
fn kb_press_right(c: &mut dereth_client::app::App, h: dereth_ui::ElemHandle) {
    kb_reveal(c, h);
    let (cx, cy) = {
        let u = kb_ui(c);
        let b = u.screen_box(h);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    assert_eq!(kb_ui(c).hit_test_screen(cx, cy), Some(h));
    {
        let u = kb_ui(c);
        u.mouse_down(dereth_ui::focus::action::SECONDARY_CLICK, cx, cy);
        u.mouse_up(dereth_ui::focus::action::SECONDARY_CLICK, cx, cy, false);
    }
    c.frame();
    c.frame();
}

/// One key, pressed and let go, through the client's own pump.
fn kb_tap(c: &mut dereth_client::app::App, hand: &mut KeyHand, code: KeyCode) {
    hand.key(c, code, true);
    hand.key(c, code, false);
}

/// The keyboard, driven the way the window loop drives it: the client's own pump builds each
/// message and the real input shell receives it.
struct KeyHand {
    pump: dereth_client::pump::Pump,
    time_ms: u32,
}

impl KeyHand {
    fn new() -> Self {
        let mut pump = dereth_client::pump::Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 500_000,
        }
    }

    fn key(&mut self, app: &mut dereth_client::app::App, code: KeyCode, down: bool) {
        self.time_ms += 10;
        let m = self
            .pump
            .key_message_for(code, down, self.time_ms)
            .expect("the host names this key");
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
        app.frame();
    }
}

fn kb_control(offset: u16) -> dereth_input::ControlChord {
    dereth_input::ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, offset),
        0,
        dereth_input::spec::activation::CLICK,
    )
}

fn keys_for_forward(c: &mut dereth_client::app::App) -> Vec<dereth_input::ControlChord> {
    c.input_manager_mut()
        .expect("the input manager")
        .keys_for_action(KB_FORWARD, KB_MOVEMENT)
}

/// The box a row has raised, and the place it holds in the queue.
fn row_dialog(c: &mut dereth_client::app::App, which: RowDialog) -> (u64, dereth_ui::ElemHandle) {
    let context = {
        let s = kb_screen(c);
        let i = s
            .key_bindings
            .row_of(KB_MOVEMENT, KB_FORWARD)
            .expect("walking forward has a row");
        s.key_bindings.rows[i]
            .dialog_context(which)
            .expect("the row owns the place")
    };
    let root = kb_ui(c)
        .dialogs
        .info(context)
        .and_then(|info| info.element)
        .expect("the row's place has a box drawn for it");
    (context, root)
}

/// The one text element a box draws its question in.
fn kb_prompt(c: &mut dereth_client::app::App, dialog: dereth_ui::ElemHandle) -> String {
    let h = kb_ui(c)
        .get_child_recursive(dialog, dereth_ui::dialog::base::child::TEXT)
        .expect("the box's own text element");
    kb_text(c, h)
}

fn kb_child(
    c: &mut dereth_client::app::App,
    dialog: dereth_ui::ElemHandle,
    id: ElementId,
) -> dereth_ui::ElemHandle {
    kb_ui(c)
        .get_child_recursive(dialog, id)
        .expect("the shipped dialog layout carries it")
}

/// The page's own file box, which is not a row's.
fn page_dialog(
    c: &mut dereth_client::app::App,
    kind: dereth_ui::dialog::DialogKind,
) -> (u64, dereth_ui::ElemHandle) {
    let info = kb_ui(c)
        .dialogs
        .open_on(DIALOG_QUEUE)
        .expect("the file box is open")
        .clone();
    assert_eq!(info.kind, kind, "the shipped kind of that file box");
    (
        info.context,
        info.element.expect("it has a box drawn for it"),
    )
}

fn submit_save_name(c: &mut dereth_client::app::App, name: &str) {
    let save = kb_screen(c)
        .key_bindings
        .save_button
        .expect("the save button is bound");
    kb_press(c, save);
    let (context, dialog) = page_dialog(c, dereth_ui::dialog::DialogKind::ConfirmationTextInput);
    let box_ = kb_ui(c)
        .get_child_recursive(
            dialog,
            dereth_ui::dialog::base::child::CONFIRM_TEXT_INPUT_BOX,
        )
        .filter(|h| *h != dialog)
        .expect("the file-name box");
    kb_ui(c)
        .text_element_mut(box_)
        .expect("the name is editable text")
        .set_text(name);
    let accept = kb_child(
        c,
        dialog,
        dereth_ui::dialog::base::child::CONFIRM_TEXT_INPUT_ACCEPT,
    );
    kb_press(c, accept);
    assert!(
        kb_ui(c).dialogs.info(context).is_none(),
        "the name box closes"
    );
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.a-cell-shows-the-key-the-way-the-desktop-names-it
// ---------------------------------------------------------------------------------------------

/// A cell shows what a player would call the key, not the name the client looks it up by.
pub fn a_cell_shows_the_key_the_way_the_desktop_names_it() {
    let mut c = a_client_on_the_key_bindings("keybinding-name");
    let starts_bound = keys_for_forward(c.app_mut()).contains(&kb_control(KB_W));

    let cell = forward_cell(c.app_mut());
    let caption = kb_text(c.app_mut(), cell);
    let plain = caption == "W" && !caption.starts_with("DIK_");
    // The other half of the same lookup: the one key the shipped text table really does name, so
    // a flat table of symbols cannot pass for the lookup.
    let named = kb_caption(c.app_mut(), KB_LEFT_CONTROL) == "Left Ctrl";

    c.assert_behaviour(
        "options.key-bindings.a-cell-shows-the-key-the-way-the-desktop-names-it",
        move |_| starts_bound && plain && named,
    );
    c.shutdown();
}

#[test]
fn scenario_a_cell_shows_the_key_the_way_the_desktop_names_it() {
    scenario("a_cell_shows_the_key_the_way_the_desktop_names_it");
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.resting-on-a-cell-says-what-a-press-there-would-do
// ---------------------------------------------------------------------------------------------

/// The letters a tooltip really draws when the pointer rests on a cell.
fn kb_tooltip(c: &mut dereth_client::app::App, cell: dereth_ui::ElemHandle, time: f64) -> String {
    kb_reveal(c, cell);
    let u = kb_ui(c);
    let b = u.screen_box(cell);
    let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    assert_eq!(u.hit_test_screen(x, y), Some(cell));
    u.mouse_move(dereth_primitives::LocalTime(time), x, y);
    for n in 1..=8 {
        u.use_time(
            dereth_primitives::LocalTime(time + f64::from(n) * 0.25),
            &mut dereth_ui::NullInputPump,
        );
    }
    let tooltip = u
        .tooltip_element()
        .expect("resting on a key cell shows a tooltip");
    let mut draw = dereth_ui::RecordingDrawBackend::default();
    u.draw(&mut draw);
    draw.calls
        .iter()
        .filter(|call| {
            let mut h = Some(call.who);
            while let Some(p) = h {
                if p == tooltip {
                    return true;
                }
                h = u.parent(p);
            }
            false
        })
        .flat_map(|call| {
            call.glyphs
                .iter()
                .map(|g| char::from_u32(u32::from(g.ch)).expect("a character"))
        })
        .collect()
}

/// A cell with a key in it says how to take it away; an empty one says how to fill it.
pub fn resting_on_a_cell_says_what_a_press_there_would_do() {
    let mut c = a_client_on_the_key_bindings("keybinding-tooltips");
    let cells = {
        let s = kb_screen(c.app_mut());
        let i = s
            .key_bindings
            .row_of(KB_MOVEMENT, KB_FORWARD)
            .expect("the row");
        s.key_bindings.rows[i].key_buttons.clone()
    };
    let three_cells = cells.len() == 3;
    let empty = *cells.last().expect("a third cell");
    let third_is_empty = kb_text(c.app_mut(), empty).is_empty();

    let bound = kb_tooltip(c.app_mut(), cells[0], 20.0);
    let says_the_key = bound.contains('W') && bound.contains("Right-click");
    let vacant = kb_tooltip(c.app_mut(), empty, 25.0);
    let says_how_to_fill = (vacant.contains("Left-Click") || vacant.contains("Left-click"))
        && !vacant.contains("Right-click");

    // Take both of the shipped bindings away through the press that takes them away, and the
    // first cell now says what an empty one says.
    for _ in 0..2 {
        kb_press_right(c.app_mut(), cells[0]);
    }
    let erased = keys_for_forward(c.app_mut()).is_empty();
    let now_vacant = kb_tooltip(c.app_mut(), cells[0], 30.0) == vacant;

    c.assert_behaviour(
        "options.key-bindings.resting-on-a-cell-says-what-a-press-there-would-do",
        move |_| {
            three_cells
                && third_is_empty
                && says_the_key
                && says_how_to_fill
                && erased
                && now_vacant
        },
    );
    c.shutdown();
}

#[test]
fn scenario_resting_on_a_cell_says_what_a_press_there_would_do() {
    scenario("resting_on_a_cell_says_what_a_press_there_would_do");
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.every-section-is-titled-in-words
// ---------------------------------------------------------------------------------------------

/// The headings are words, not the tokens that look the words up.
pub fn every_section_is_titled_in_words() {
    let mut c = a_client_on_the_key_bindings("keybinding-titles");
    let headers = kb_screen(c.app_mut()).key_bindings.header_elements.clone();
    let enough = headers.len() >= 6;
    let titles: Vec<String> = headers
        .into_iter()
        .map(|h| kb_text(c.app_mut(), h))
        .collect();
    let all_words = titles
        .iter()
        .all(|t| !t.is_empty() && !t.starts_with("ID_"));
    let one_of_them = titles.iter().any(|t| t == "Movement");

    c.assert_behaviour(
        "options.key-bindings.every-section-is-titled-in-words",
        move |_| enough && all_words && one_of_them,
    );
    c.shutdown();
}

#[test]
fn scenario_every_section_is_titled_in_words() {
    scenario("every_section_is_titled_in_words");
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.undo-opens-greyed-because-nothing-has-changed-yet
// ---------------------------------------------------------------------------------------------

/// The undo button is dead when the page comes up.
pub fn undo_opens_greyed_because_nothing_has_changed_yet() {
    let mut c = a_client_on_the_key_bindings("keybinding-revert");
    let nothing_changed = !kb_screen(c.app_mut()).key_bindings.changed();
    let undo = kb_screen(c.app_mut())
        .key_bindings
        .revert_to_saved_button
        .expect("undo is bound");
    let greyed = kb_state(c.app_mut(), undo) == KB_GREYED;

    c.assert_behaviour(
        "options.key-bindings.undo-opens-greyed-because-nothing-has-changed-yet",
        move |_| nothing_changed && greyed,
    );
    c.shutdown();
}

#[test]
fn scenario_undo_opens_greyed_because_nothing_has_changed_yet() {
    scenario("undo_opens_greyed_because_nothing_has_changed_yet");
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.a-press-on-a-cell-waits-for-a-key-and-undo-puts-the-old-one-back
// ---------------------------------------------------------------------------------------------

/// The whole gesture: press a cell, be told to press a key, cancel, do it again, and undo.
pub fn a_press_on_a_cell_waits_for_a_key_and_undo_puts_the_old_one_back() {
    let mut c = a_client_on_the_key_bindings("keybinding-gesture");
    let mut hand = KeyHand::new();

    let before = keys_for_forward(c.app_mut());
    let starts_right = before.contains(&kb_control(KB_W)) && !before.contains(&kb_control(KB_F7));

    let cell = forward_cell(c.app_mut());
    kb_press(c.app_mut(), cell);
    let waiting = c
        .app_mut()
        .input_manager_mut()
        .expect("the input manager")
        .manager
        .key_hit_handler_registered();

    // The visible half: a box that says what to do, with the action's own name in it.
    let (context, dialog) = row_dialog(c.app_mut(), RowDialog::MapWarn);
    let shown = kb_ui(c.app_mut())
        .node(dialog)
        .is_some_and(|n| n.region.flags.visible);
    let prompt = kb_prompt(c.app_mut(), dialog);
    let says_what_to_do = prompt.starts_with("The next key you press")
        && prompt.contains("Move Forward")
        && prompt.contains("Press the ESC key to cancel");

    kb_tap(c.app_mut(), &mut hand, KeyCode::Escape);
    let cancelled = keys_for_forward(c.app_mut()) == before
        && !c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager
            .key_hit_handler_registered()
        && kb_ui(c.app_mut()).dialogs.info(context).is_none()
        && kb_ui(c.app_mut()).node(dialog).is_none();

    kb_press(c.app_mut(), cell);
    let waiting_again = c
        .app_mut()
        .input_manager_mut()
        .expect("the input manager")
        .manager
        .key_hit_handler_registered();
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    c.tick(4);
    let bound = keys_for_forward(c.app_mut()).contains(&kb_control(KB_F7))
        && !c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager
            .key_hit_handler_registered();

    let undo = kb_screen(c.app_mut())
        .key_bindings
        .revert_to_saved_button
        .expect("undo is bound");
    let lit = kb_screen(c.app_mut()).key_bindings.changed()
        && kb_state(c.app_mut(), undo) == KB_LIVE
        && kb_text(c.app_mut(), cell) == "F7";

    kb_press(c.app_mut(), undo);
    let restored = keys_for_forward(c.app_mut());
    let undone = restored.contains(&kb_control(KB_W))
        && !restored.contains(&kb_control(KB_F7))
        && !kb_screen(c.app_mut()).key_bindings.changed()
        && kb_state(c.app_mut(), undo) == KB_GREYED;

    c.assert_behaviour(
        "options.key-bindings.a-press-on-a-cell-waits-for-a-key-and-undo-puts-the-old-one-back",
        move |_| {
            starts_right
                && waiting
                && shown
                && says_what_to_do
                && cancelled
                && waiting_again
                && bound
                && lit
                && undone
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_on_a_cell_waits_for_a_key_and_undo_puts_the_old_one_back() {
    scenario("a_press_on_a_cell_waits_for_a_key_and_undo_puts_the_old_one_back");
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.a-key-already-in-use-asks-first-and-one-that-cannot-be-taken-refuses
// ---------------------------------------------------------------------------------------------

/// The two ways a captured key is not simply taken.
pub fn a_key_already_in_use_asks_first_and_one_that_cannot_be_taken_refuses() {
    let mut c = a_client_on_the_key_bindings("keybinding-conflicts");
    let mut hand = KeyHand::new();
    let cell = forward_cell(c.app_mut());

    let the_conflict_is_shipped = c
        .app_mut()
        .input_manager_mut()
        .expect("the input manager")
        .manager
        .find_keys_for_action(KB_TURN_LEFT, KB_MOVEMENT)
        .contains(&kb_control(KB_A));

    // No: the question is really there, and answering no leaves both actions alone.
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyA);
    let (no_context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let it_is_a_question = dereth_ui::dialog::types::dialog_element(kb_ui(c.app_mut()), question)
        .map(|d| d.kind)
        == Some(dereth_ui::dialog::DialogKind::Confirmation);
    let no = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON2,
    );
    kb_press(c.app_mut(), no);
    let no_changed_nothing = kb_ui(c.app_mut()).dialogs.info(no_context).is_none()
        && kb_ui(c.app_mut()).node(question).is_none()
        && !keys_for_forward(c.app_mut()).contains(&kb_control(KB_A))
        && c.app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager
            .find_keys_for_action(KB_TURN_LEFT, KB_MOVEMENT)
            .contains(&kb_control(KB_A));

    // Yes: the key moves, and the action that had it loses it.
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyA);
    let (yes_context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let yes = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    kb_press(c.app_mut(), yes);
    let yes_moved_it = kb_ui(c.app_mut()).dialogs.info(yes_context).is_none()
        && kb_ui(c.app_mut()).node(question).is_none()
        && keys_for_forward(c.app_mut()).contains(&kb_control(KB_A))
        && !c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager
            .find_keys_for_action(KB_TURN_LEFT, KB_MOVEMENT)
            .contains(&kb_control(KB_A));

    // The refusal. The shipped maps' one key a player may not take is the one that cancels the
    // capture itself, so a key that is free is put on that action here to make the refusal
    // reachable at all, and everything after it is the client's own.
    {
        let m = &mut c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager;
        assert!(!m
            .action_map
            .is_user_bindable(KB_UI_COMMANDS, KB_ESCAPE_ACTION));
        assert!(m
            .find_conflicting_input_maps(KB_MOVEMENT)
            .contains(&KB_UI_COMMANDS));
        m.bind_action(kb_control(KB_F7), KB_ESCAPE_ACTION, KB_UI_COMMANDS);
    }
    let before_refusal = keys_for_forward(c.app_mut());
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    let (notice_context, notice) = row_dialog(c.app_mut(), RowDialog::CantOverwrite);
    let it_is_a_notice = dereth_ui::dialog::types::dialog_element(kb_ui(c.app_mut()), notice)
        .map(|d| d.kind)
        == Some(dereth_ui::dialog::DialogKind::Message);
    let dismiss = kb_child(
        c.app_mut(),
        notice,
        dereth_ui::dialog::base::child::MESSAGE_BUTTON,
    );
    kb_press(c.app_mut(), dismiss);
    let refused = kb_ui(c.app_mut()).dialogs.info(notice_context).is_none()
        && kb_ui(c.app_mut()).node(notice).is_none()
        && keys_for_forward(c.app_mut()) == before_refusal;

    c.assert_behaviour(
        "options.key-bindings.a-key-already-in-use-asks-first-and-one-that-cannot-be-taken-refuses",
        move |_| {
            the_conflict_is_shipped
                && it_is_a_question
                && no_changed_nothing
                && yes_moved_it
                && it_is_a_notice
                && refused
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_key_already_in_use_asks_first_and_one_that_cannot_be_taken_refuses() {
    scenario("a_key_already_in_use_asks_first_and_one_that_cannot_be_taken_refuses");
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.taking-a-key-clears-it-from-the-rows-that-had-it-on-the-screen
// ---------------------------------------------------------------------------------------------

/// Saying yes really does rebind, and the row that lost the key stops drawing it.
pub fn taking_a_key_clears_it_from_the_rows_that_had_it() {
    let mut c = a_client_on_the_key_bindings("keybinding-refresh");
    let mut hand = KeyHand::new();
    let cell = forward_cell(c.app_mut());
    let a = kb_caption(c.app_mut(), KB_A);
    let one = kb_caption(c.app_mut(), KB_DIGIT_1);

    // One row loses it. Both sides of this are the shipped data.
    let before = drawn_cells(c.app_mut(), KB_MOVEMENT, KB_TURN_LEFT);
    let starts_there =
        before.contains(&a) && !drawn_cells(c.app_mut(), KB_MOVEMENT, KB_FORWARD).contains(&a);

    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyA);
    let (context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let yes = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    kb_press(c.app_mut(), yes);
    let took_it = kb_ui(c.app_mut()).dialogs.info(context).is_none()
        && keys_for_forward(c.app_mut()).contains(&kb_control(KB_A));
    let one_row_cleared = !drawn_cells(c.app_mut(), KB_MOVEMENT, KB_TURN_LEFT).contains(&a)
        && drawn_cells(c.app_mut(), KB_MOVEMENT, KB_FORWARD).contains(&a);

    // Two rows lose it at once, in two different sections -- also the shipped data: the key is on
    // a spell slot and on a quick slot, in two sets that each clash with movement but not with
    // one another.
    let both_are_shipped = {
        let m = &mut c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager;
        let maps = m.find_conflicting_input_maps(KB_MOVEMENT);
        m.action_map.is_user_bindable(KB_SPELL_BAR, KB_SPELL_SLOT_1)
            && m.action_map
                .is_user_bindable(KB_QUICK_SLOTS, KB_QUICKSLOT_1)
            && maps.contains(&KB_SPELL_BAR)
            && maps.contains(&KB_QUICK_SLOTS)
    };
    let both_draw_it = drawn_cells(c.app_mut(), KB_SPELL_BAR, KB_SPELL_SLOT_1).contains(&one)
        && drawn_cells(c.app_mut(), KB_QUICK_SLOTS, KB_QUICKSLOT_1).contains(&one);

    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::Digit1);
    let (context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let yes = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    kb_press(c.app_mut(), yes);
    let took_that_one = kb_ui(c.app_mut()).dialogs.info(context).is_none()
        && keys_for_forward(c.app_mut()).contains(&kb_control(KB_DIGIT_1));

    let quick = drawn_cells(c.app_mut(), KB_QUICK_SLOTS, KB_QUICKSLOT_1);
    let both_rows_cleared = !drawn_cells(c.app_mut(), KB_SPELL_BAR, KB_SPELL_SLOT_1).contains(&one)
        && !quick.contains(&one)
        && drawn_cells(c.app_mut(), KB_MOVEMENT, KB_FORWARD).contains(&one);
    // ...and the quick slot's *modified* key on the same letter is untouched, because a key with
    // a modifier held is a different key and was never in the way.
    let the_modified_one_survives = quick
        .iter()
        .any(|cell| cell.ends_with(&one) && cell != &one);

    c.assert_behaviour(
        "options.key-bindings.taking-a-key-clears-it-from-the-rows-that-had-it-on-the-screen",
        move |_| {
            starts_there
                && took_it
                && one_row_cleared
                && both_are_shipped
                && both_draw_it
                && took_that_one
                && both_rows_cleared
                && the_modified_one_survives
        },
    );
    c.shutdown();
}

#[test]
fn scenario_taking_a_key_clears_it_from_the_rows_that_had_it() {
    scenario("taking_a_key_clears_it_from_the_rows_that_had_it");
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.the-questions-about-a-key-in-use-are-the-shipped-sentences
// ---------------------------------------------------------------------------------------------

/// The three prompts, word for word, with the key and the action in them.
pub fn the_questions_about_a_key_in_use_are_the_shipped_sentences() {
    let mut c = a_client_on_the_key_bindings("keybinding-conflict-text");
    let mut hand = KeyHand::new();
    let cell = forward_cell(c.app_mut());

    // One clash, on the shipped data.
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyA);
    let (one_context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let one_clash = kb_prompt(c.app_mut(), question)
        == "'A' is currently bound to 'Turn Left'. Do you wish to erase that binding?";
    let no = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON2,
    );
    kb_press(c.app_mut(), no);
    let no_answered = kb_ui(c.app_mut()).dialogs.info(one_context).is_none()
        && !keys_for_forward(c.app_mut()).contains(&kb_control(KB_A));

    // Several clashes. A key no shipped map uses is put on two actions that really are bindable
    // and really do clash with each other, so the several-clash sentence is reachable.
    let order: Vec<InputMapId> = {
        let m = &mut c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager;
        assert!(m.action_map.is_user_bindable(KB_MOVEMENT, KB_TURN_LEFT));
        assert!(m
            .action_map
            .is_user_bindable(KB_UI_COMMANDS, KB_USE_SELECTED));
        m.bind_action(kb_control(KB_F7), KB_TURN_LEFT, KB_MOVEMENT);
        m.bind_action(kb_control(KB_F7), KB_USE_SELECTED, KB_UI_COMMANDS);
        // The order the client walks them in is the order the lines come out in, read here rather
        // than assumed.
        m.find_conflicting_input_maps(KB_MOVEMENT).to_vec()
    };
    let movement_first = order.iter().position(|m| *m == KB_MOVEMENT)
        < order.iter().position(|m| *m == KB_UI_COMMANDS);
    let turn_left = "'Turn Left' ('F7')\n";
    let use_selected = "'Use Selected Object' ('F7')\n";
    let list = if movement_first {
        format!("{turn_left}{use_selected}")
    } else {
        format!("{use_selected}{turn_left}")
    };
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    let (many_context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let several_clashes = kb_prompt(c.app_mut(), question)
        == format!(
            "'F7' conflicts with the following bindings:\n{list}\nDo you wish to erase those \
             bindings?"
        );
    let no = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON2,
    );
    kb_press(c.app_mut(), no);
    let still_free = kb_ui(c.app_mut()).dialogs.info(many_context).is_none()
        && !keys_for_forward(c.app_mut()).contains(&kb_control(KB_F7));

    // And the refusal, which names the key and not the row it was pressed on.
    {
        let m = &mut c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager;
        assert!(!m
            .action_map
            .is_user_bindable(KB_UI_COMMANDS, KB_ESCAPE_ACTION));
        m.bind_action(kb_control(KB_F7), KB_ESCAPE_ACTION, KB_UI_COMMANDS);
    }
    let before_refusal = keys_for_forward(c.app_mut());
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    let (notice_context, notice) = row_dialog(c.app_mut(), RowDialog::CantOverwrite);
    let refusal = kb_prompt(c.app_mut(), notice)
        == "'F7' is currently bound to a non user-bindable action. Please select a different \
            binding.";
    let dismiss = kb_child(
        c.app_mut(),
        notice,
        dereth_ui::dialog::base::child::MESSAGE_BUTTON,
    );
    kb_press(c.app_mut(), dismiss);
    let dismissed = kb_ui(c.app_mut()).dialogs.info(notice_context).is_none()
        && keys_for_forward(c.app_mut()) == before_refusal;

    c.assert_behaviour("options.key-bindings.the-questions-about-a-key-in-use-are-the-shipped-sentences-with-the-key-and-the-action-in-them", move |_| {
        one_clash && no_answered && several_clashes && still_free && refusal && dismissed
    });
    c.shutdown();
}

#[test]
fn scenario_the_questions_about_a_key_in_use_are_the_shipped_sentences() {
    scenario("the_questions_about_a_key_in_use_are_the_shipped_sentences");
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.a-set-of-keys-can-be-saved-under-a-name-and-loaded-back
// ---------------------------------------------------------------------------------------------

/// Save, change, load: the loaded keys come back and the page redraws them.
pub fn a_set_of_keys_can_be_saved_under_a_name_and_loaded_back() {
    let mut c = a_client_on_the_key_bindings("keybinding-files");
    let dir = c
        .scratch_settings()
        .expect("a settings directory")
        .dir()
        .to_path_buf();
    let saved = dir.join("keys-roundtrip-modern.keymap");
    let mut hand = KeyHand::new();
    let cell = forward_cell(c.app_mut());

    // Something recognisable to save.
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    let changed_first = keys_for_forward(c.app_mut()).contains(&kb_control(KB_F7));

    submit_save_name(c.app_mut(), "keys-roundtrip");
    let written = saved.exists();

    // Change it again, so loading the file has something to undo.
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyA);
    let (_, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let yes = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    kb_press(c.app_mut(), yes);
    let now_different = {
        let keys = keys_for_forward(c.app_mut());
        keys.contains(&kb_control(KB_A)) && !keys.contains(&kb_control(KB_F7))
    };

    let load = kb_screen(c.app_mut())
        .key_bindings
        .load_button
        .expect("the load button is bound");
    kb_press(c.app_mut(), load);
    let (load_context, load_dialog) =
        page_dialog(c.app_mut(), dereth_ui::dialog::DialogKind::ConfirmationMenu);
    let menu = kb_child(
        c.app_mut(),
        load_dialog,
        dereth_ui::dialog::base::child::CONFIRM_MENU_MENU,
    );
    // "Default" first, then this interface's saved key maps by the names they were saved
    // under.
    let default_first = dereth_ui::widgets::menu::get_item(kb_ui(c.app_mut()), menu, 0)
        .is_some_and(|row| kb_text(c.app_mut(), row) == "Default");
    let row = dereth_ui::widgets::menu::get_item(kb_ui(c.app_mut()), menu, 1)
        .expect("the saved file is a real row of the list");
    let listed = default_first && kb_text(c.app_mut(), row) == "keys-roundtrip";
    kb_press(c.app_mut(), menu);
    kb_press(c.app_mut(), row);
    let accept = kb_child(
        c.app_mut(),
        load_dialog,
        dereth_ui::dialog::base::child::CONFIRM_MENU_ACCEPT,
    );
    kb_press(c.app_mut(), accept);
    let closed = kb_ui(c.app_mut()).dialogs.info(load_context).is_none();

    let loaded = keys_for_forward(c.app_mut());
    let came_back = loaded.contains(&kb_control(KB_F7)) && !loaded.contains(&kb_control(KB_A));
    let cell = forward_cell(c.app_mut());
    let redrawn = kb_text(c.app_mut(), cell) == "F7";

    c.assert_behaviour(
        "options.key-bindings.a-set-of-keys-can-be-saved-under-a-name-and-loaded-back",
        move |_| {
            changed_first && written && now_different && listed && closed && came_back && redrawn
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_set_of_keys_can_be_saved_under_a_name_and_loaded_back() {
    scenario("a_set_of_keys_can_be_saved_under_a_name_and_loaded_back");
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.saving-over-a-set-asks-first-and-one-that-cannot-be-written-refuses
// ---------------------------------------------------------------------------------------------

/// Saving over a file that exists asks, and a file that cannot be written says so.
pub fn saving_over_a_set_asks_first_and_one_that_cannot_be_written_refuses() {
    let mut c = a_client_on_the_key_bindings("keybinding-overwrite");
    let dir = c
        .scratch_settings()
        .expect("a settings directory")
        .dir()
        .to_path_buf();
    let writable = dir.join("existing-modern.keymap");
    let read_only = dir.join("read-only-modern.keymap");
    let sentinel = b"existing bytes must survive no";
    let read_only_sentinel = b"read-only bytes must not change";
    std::fs::write(&writable, sentinel).expect("a disposable target");
    std::fs::write(&read_only, read_only_sentinel).expect("a disposable target");
    let permissions = std::fs::metadata(&read_only)
        .expect("its metadata")
        .permissions();
    {
        let mut ro = permissions.clone();
        ro.set_readonly(true);
        std::fs::set_permissions(&read_only, ro).expect("mark the disposable target read-only");
    }

    let mut hand = KeyHand::new();
    let cell = forward_cell(c.app_mut());
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    let something_to_save = keys_for_forward(c.app_mut()).contains(&kb_control(KB_F7));

    // A file that exists: no leaves its bytes exactly alone.
    submit_save_name(c.app_mut(), "existing");
    let (no_context, question) =
        page_dialog(c.app_mut(), dereth_ui::dialog::DialogKind::Confirmation);
    let no = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON2,
    );
    kb_press(c.app_mut(), no);
    let untouched = kb_ui(c.app_mut()).dialogs.info(no_context).is_none()
        && std::fs::read(&writable).expect("the target remains") == sentinel;

    // The same name and yes replace it with the keys that are live now.
    submit_save_name(c.app_mut(), "existing");
    let (yes_context, question) =
        page_dialog(c.app_mut(), dereth_ui::dialog::DialogKind::Confirmation);
    let yes = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    kb_press(c.app_mut(), yes);
    let written = std::fs::read_to_string(&writable).expect("yes wrote a file");
    let replaced = kb_ui(c.app_mut()).dialogs.info(yes_context).is_none()
        && written.as_bytes() != sentinel
        && dereth_input::MasterInputMap::from_keymap_text(&written)
            .expect("the file is a set of keys")
            .section(KB_MOVEMENT)
            .expect("the movement section")
            .bindings()
            .iter()
            .any(|(k, a)| *k == kb_control(KB_F7) && *a == KB_FORWARD);

    // One that cannot be written never asks: it says so, in a box with one button, and the bytes
    // do not change.
    submit_save_name(c.app_mut(), "read-only");
    let (notice_context, notice) = page_dialog(c.app_mut(), dereth_ui::dialog::DialogKind::Message);
    let dismiss = kb_child(
        c.app_mut(),
        notice,
        dereth_ui::dialog::base::child::MESSAGE_BUTTON,
    );
    kb_press(c.app_mut(), dismiss);
    let refused = kb_ui(c.app_mut()).dialogs.info(notice_context).is_none()
        && std::fs::read(&read_only).expect("the target remains") == read_only_sentinel;

    // Put the attribute back, so the directory can be removed with the client.
    let _ = std::fs::set_permissions(&read_only, permissions);

    c.assert_behaviour(
        "options.key-bindings.saving-over-a-set-asks-first-and-one-that-cannot-be-written-refuses",
        move |_| something_to_save && untouched && replaced && refused,
    );
    c.shutdown();
}

#[test]
fn scenario_saving_over_a_set_asks_first_and_one_that_cannot_be_written_refuses() {
    scenario("saving_over_a_set_asks_first_and_one_that_cannot_be_written_refuses");
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.restoring-the-defaults-gives-back-the-shipped-keys-and-not-the-saved-ones
// ---------------------------------------------------------------------------------------------

/// Two whole client lifetimes against one settings directory: the first rebinds and exits, which
/// writes the file, and the second starts with that file already merged in -- which is the state
/// in which "restore the defaults" could restore the rebind instead of the shipped key.
///
/// **It owns its clients outright**, through `adapters_shell::AppSpec` / `build_app`: the harness
/// removes a scenario's settings directory when the client is dropped, which is right for every
/// other scenario and is exactly what this one cannot have.
pub fn restoring_the_defaults_gives_back_the_shipped_keys() {
    use dereth_testkit::adapters_shell::{build_app, scratch_preferences, AppSpec};

    let prefs = scratch_preferences("keybinding-defaults");
    let dir = prefs.parent().expect("a directory").to_path_buf();
    // A fresh start: a previous run's file would be this run's precondition.
    for name in ["UserPreferences.keymap", "UserPreferences.ini"] {
        let _ = std::fs::remove_file(dir.join(name));
    }
    let spec = AppSpec {
        preferences_file: Some(prefs.clone()),
        static_scene: true,
        ..AppSpec::in_gameplay(6)
    };

    // Life one: rebind, and let the exit write the file.
    let wrote_the_file = {
        let mut app = build_app(&spec);
        let mut hand = KeyHand::new();
        let cell = forward_cell(&mut app);
        kb_press(&mut app, cell);
        kb_tap(&mut app, &mut hand, KeyCode::F7);
        for _ in 0..4 {
            app.frame();
        }
        let took = keys_for_forward(&mut app).contains(&kb_control(KB_F7));
        let path = app
            .input_manager_mut()
            .expect("the input manager")
            .keymap_path()
            .map(std::path::Path::to_path_buf);
        let _ = app.shutdown();
        let path = path.expect("the client keeps its keys beside its preferences");
        took && path.exists()
    };

    // Life two: the page is built with the rebind already merged in.
    let mut app = build_app(&spec);
    let loaded = keys_for_forward(&mut app).contains(&kb_control(KB_F7));

    let i = {
        let s = kb_screen(&mut app);
        s.key_bindings
            .row_of(KB_MOVEMENT, KB_FORWARD)
            .expect("walking forward has a row")
    };
    let (defaults, current) = {
        let r = &kb_screen(&mut app).key_bindings.rows[i];
        (r.defaults.clone(), r.current.clone())
    };
    // What the row is showing is the merged set, which has the rebind; what "the defaults" means
    // is the shipped set, which does not.
    let the_two_differ = current.contains(&kb_control(KB_F7))
        && defaults.contains(&kb_control(KB_W))
        && !defaults.contains(&kb_control(KB_F7));

    let reset = kb_screen(&mut app)
        .key_bindings
        .reset_defaults_button
        .expect("the reset button is bound");
    kb_press(&mut app, reset);
    let restored = keys_for_forward(&mut app);
    let shipped_came_back =
        restored.contains(&kb_control(KB_W)) && !restored.contains(&kb_control(KB_F7));
    let cell = forward_cell(&mut app);
    let redrawn = kb_text(&mut app, cell) == "W";
    let _ = app.shutdown();
    let _ = std::fs::remove_dir_all(&dir);

    let mut c = HeadlessClient::model();
    c.assert_behaviour("options.key-bindings.restoring-the-defaults-gives-back-the-shipped-keys-and-not-the-saved-ones", move |_| {
        wrote_the_file && loaded && the_two_differ && shipped_came_back && redrawn
    });
}

#[test]
fn scenario_restoring_the_defaults_gives_back_the_shipped_keys() {
    scenario("restoring_the_defaults_gives_back_the_shipped_keys");
}

// =============================================================================================
// text-entry.filters.* and text-entry.ime.* -- what a box will take, and what an IME sends
//
// Eight scenarios, eight rows: the census of which boxes take only certain characters is itself a
// claim a player meets -- most boxes take anything and three of them do not -- so it keeps a row of
// its own rather than retiring as a denominator.
//
// Every character below is a real message built by the client's own pump and delivered where the
// window loop delivers one. No datagram leaves the process except the one the last scenario reads
// out of a socket-free endpoint.
// =============================================================================================

use dereth_client::pump::key_text_messages;
use dereth_client::pump::window_proc::{msg as win_msg, Effect};

/// The chat entry: editable, one line, and one of the boxes that takes anything -- which is what
/// makes it the right box to prove a composed character on.
const TEXT_CHAT_ENTRY: ElementId = ElementId(0x1000_0016);
/// The caption beside a profession slider, which the client filters although nobody can type in it.
const ATTRIB_CAPTION: u32 = 0x1000_02ED;
/// The strip the stack splitter lives in, hidden until something is picked.
const SEL_OBJECT_FIELD: ElementId = ElementId(0x1000_019E);
/// The four boxes the client puts a filter on, and which filter.
const FILTERED_IDS: [u32; 4] = [0x1000_0402, ATTRIB_CAPTION, 0x1000_046B, 0x1000_01A3];

/// Digits only.
const NUMBER_PROBE: (bool, bool, bool) = (true, false, false);
/// Letters, an apostrophe, a space and a hyphen; no digits.
const NAME_PROBE: (bool, bool, bool) = (false, true, true);

impl KeyHand {
    /// One press of the button at a point, with the frames the gesture needs.
    fn click_at(&mut self, app: &mut dereth_client::app::App, at: (i32, i32)) {
        use dereth_client::platform::keys::MouseButton;
        self.time_ms += 10;
        let m = self
            .pump
            .mouse_move_message(f64::from(at.0), f64::from(at.1), self.time_ms);
        self.send(app, m);
        for pressed in [true, false] {
            self.time_ms += 10;
            let m = self
                .pump
                .button_message(MouseButton::Left, pressed, self.time_ms)
                .expect("the left button is one of the client's own messages");
            self.send(app, m);
        }
        app.frame();
        app.frame();
    }

    fn send(&mut self, app: &mut dereth_client::app::App, m: dereth_client::pump::Win32Message) {
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
    }

    /// One typed code unit per call, unsigned, exactly as the message carries it -- which is what
    /// an IME's committed string arrives as too.
    fn type_units(&mut self, app: &mut dereth_client::app::App, units: &[u16]) {
        for u in units {
            self.time_ms += 10;
            let m = dereth_client::pump::Win32Message::new(
                win_msg::WM_CHAR,
                *u as usize,
                0,
                self.time_ms,
            );
            self.send(app, m);
        }
        app.frame();
    }

    fn type_text(&mut self, app: &mut dereth_client::app::App, s: &str) {
        let units: Vec<u16> = s.encode_utf16().collect();
        self.type_units(app, &units);
    }

    /// One key edge with **no frame after it**, for a scenario that wants the harness to run the
    /// frame -- which is the only way what the client asked for on that frame is recorded.
    fn key_quiet(&mut self, app: &mut dereth_client::app::App, code: KeyCode, down: bool) {
        self.time_ms += 10;
        let m = self
            .pump
            .key_message_for(code, down, self.time_ms)
            .expect("the host names this key");
        self.send(app, m);
    }

    /// A key down together with the text the desktop says that key produced with the modifiers
    /// that are held -- which is not the same as the letter on the key.
    fn key_with_text(
        &mut self,
        app: &mut dereth_client::app::App,
        code: KeyCode,
        text: Option<&str>,
        alt_down: bool,
    ) {
        self.key(app, code, true);
        self.time_ms += 10;
        for m in key_text_messages(true, alt_down, text, self.time_ms) {
            self.send(app, m);
        }
    }
}

/// Whether a box will take a character at all, probed by asking it -- three characters chosen to
/// tell the two filters apart: a digit, a letter, and the hyphen a name may have.
fn filter_probe(
    app: &mut dereth_client::app::App,
    h: dereth_ui::ElemHandle,
) -> Option<(bool, bool, bool)> {
    let f = kb_ui(app).text_element_mut(h)?.filter?;
    Some((f(0x37), f(0x61), f(0x2D)))
}

/// Press an element, having first proved the pointer really lands on it or inside it.
fn press_text_element(
    app: &mut dereth_client::app::App,
    hand: &mut KeyHand,
    h: dereth_ui::ElemHandle,
) {
    let at = {
        let u = kb_ui(app);
        let mut a = Some(h);
        while let Some(x) = a {
            assert!(
                u.node(x).expect("live").region.flags.visible,
                "every element between the screen and {h:?} must be shown"
            );
            a = u.parent(x);
        }
        let b = u.screen_clip_box(h);
        assert!(b.is_valid(), "the element has a real rectangle");
        let at = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        let hit = u.hit_test_screen(at.0, at.1);
        assert!(
            hit.is_some_and(|x| x == h || u.is_ancestor_of(h, x)),
            "the pointer at {at:?} must land on {h:?} or something inside it; it landed on {hit:?}"
        );
        at
    };
    hand.click_at(app, at);
}

fn text_element(app: &dereth_client::app::App, id: ElementId) -> dereth_ui::ElemHandle {
    let shell = app.ui().expect("the UI shell is up");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

// ---------------------------------------------------------------------------------------------
// text-entry.filters.three-of-the-clients-boxes-take-only-certain-characters
// ---------------------------------------------------------------------------------------------

/// Which boxes are fussy, out of all the boxes there are.
pub fn three_of_the_clients_boxes_take_only_certain_characters() {
    use dereth_primitives::AssetSource as _;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    let store = c.dat_store().expect("a retail client has a store").clone();
    let master_id = dereth_primitives::DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("the master property record");
    let types =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("it decodes")
            .property_types();

    /// The last value an element's own look, or any of its named looks, gives this attribute.
    /// Every named look is searched, because an undercount here reads exactly like the census
    /// being right.
    fn declared(e: &dereth_ui::desc::ElementDesc, want: u32) -> Option<bool> {
        let mut v = None;
        for st in std::iter::once(&e.base).chain(e.states.values()) {
            for (id, value) in &st.properties.0 {
                if *id == want {
                    if let dereth_assets::ui::PropertyValue::Bool(b) = value {
                        v = Some(*b);
                    }
                }
            }
        }
        v
    }

    fn walk(
        e: &dereth_ui::desc::ElementDesc,
        lid: dereth_primitives::DataId,
        out: &mut Vec<(dereth_primitives::DataId, u32, Option<bool>)>,
    ) {
        // The two attributes together: a box that can be typed into, or only picked at.
        let editable = declared(e, 0x16);
        if editable.is_some() || declared(e, 0x27).is_some() {
            out.push((lid, e.element_id.0, editable));
        }
        for child in e.children.values() {
            walk(child, lid, out);
        }
    }

    let ids = store.ids_of(dereth_dat::DbType::UiLayout);
    let mut rows = Vec::new();
    let mut decoded = 0_usize;
    for id in &ids {
        let raw = store.read(*id).expect("a layout the directory lists reads");
        let layout = dereth_ui::desc::LayoutDesc::read(*id, &raw, &types).expect("it decodes");
        decoded += 1;
        for e in layout.elements.values() {
            walk(e, *id, &mut rows);
        }
    }
    // The space, so a census that silently read half the layouts cannot pass.
    let whole_corpus = ids.len() == 101 && decoded == ids.len();

    let editable: Vec<_> = rows.iter().filter(|r| r.2 == Some(true)).collect();
    let pairs: std::collections::BTreeSet<(dereth_primitives::DataId, u32)> =
        editable.iter().map(|r| (r.0, r.1)).collect();
    let element_ids: std::collections::BTreeSet<u32> = editable.iter().map(|r| r.1).collect();
    let counted = editable.len() == 41 && pairs.len() == 39 && element_ids.len() == 36;

    let filtered: std::collections::BTreeSet<u32> = FILTERED_IDS.iter().copied().collect();
    let four_of_them = filtered.len() == 4;
    let not_a_box: Vec<u32> = filtered
        .iter()
        .copied()
        .filter(|id| !element_ids.contains(id))
        .collect();
    let real_boxes = filtered
        .iter()
        .copied()
        .filter(|id| element_ids.contains(id))
        .count();
    // The odd one out, and the whole answer turns on it: one of the four is a caption the client
    // filters although nobody can type in it, so three of the client's boxes are fussy and the
    // other thirty-three take whatever the player types.
    let three_boxes_and_a_caption = not_a_box == vec![ATTRIB_CAPTION] && real_boxes == 3;

    c.assert_behaviour("text-entry.filters.three-of-the-clients-boxes-take-only-certain-characters-and-the-rest-take-anything", move |_| {
        whole_corpus && counted && four_of_them && three_boxes_and_a_caption
    });
    c.shutdown();
}

#[test]
fn scenario_three_of_the_clients_boxes_take_only_certain_characters() {
    scenario("three_of_the_clients_boxes_take_only_certain_characters");
}

// ---------------------------------------------------------------------------------------------
// text-entry.filters.the-name-box-takes-the-letters-a-name-may-have-and-nothing-else
// ---------------------------------------------------------------------------------------------

/// The character's name box, typed into by hand.
pub fn the_name_box_takes_the_letters_a_name_may_have() {
    let mut c = a_wizard_on_the_summary_page();
    let app = c.app_mut();
    let name = text_element(app, chargen::NAME_FIELD);
    let mut hand = KeyHand::new();
    press_text_element(app, &mut hand, name);
    let focused = kb_ui(app).focus_element() == Some(name);

    // A press is not a select-all, so the prompt stays and the caret sits past it; the prompt is
    // read off the live box rather than written down here.
    let before = kb_text(app, name);
    let prompted = !before.is_empty();

    // Everything a name may have, interleaved with everything a player is most likely to try that
    // it may not: digits and the punctuation that is not an apostrophe, a space or a hyphen.
    hand.type_text(app, "Bo0!b_-.Sm+ith's");
    let refused_the_rest = kb_text(app, name) == format!("{before}Bob-Smith's");

    // The space on its own, because it is the one accepted character easiest to lose by keeping
    // only the letters.
    hand.type_text(app, " Jr");
    let space_allowed = kb_text(app, name) == format!("{before}Bob-Smith's Jr");

    // ...and the same from the other side: the box really is asking the name question and not
    // some other one that happens to agree on these sixteen characters.
    let the_right_filter = filter_probe(app, name) == Some(NAME_PROBE);

    c.assert_behaviour(
        "text-entry.filters.the-name-box-takes-the-letters-a-name-may-have-and-nothing-else",
        move |_| focused && prompted && refused_the_rest && space_allowed && the_right_filter,
    );
    c.shutdown();
}

#[test]
fn scenario_the_name_box_takes_the_letters_a_name_may_have() {
    scenario("the_name_box_takes_the_letters_a_name_may_have");
}

// ---------------------------------------------------------------------------------------------
// text-entry.filters.the-how-many-box-takes-digits-only
// ---------------------------------------------------------------------------------------------

/// The box a player says how many of a stack to move in.
pub fn the_how_many_box_takes_digits_only() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(8));
    {
        let app = c.app_mut();
        let field = text_element(app, SEL_OBJECT_FIELD);
        let shell = app.ui_mut().expect("the UI shell is up");
        let u = &mut shell.ui;
        u.set_visible(field, true);
        let box_h = u
            .get_child_recursive(field, dereth_ui_screens::toolbar::splitter::ENTRY_BOX)
            .expect("the box is under the strip");
        u.set_visible(box_h, true);
        if let Some(s) = u.get_child_recursive(field, dereth_ui_screens::toolbar::splitter::SLIDER)
        {
            u.set_visible(s, true);
        }
        if let Some(t) = u.text_element_mut(box_h) {
            t.set_text("20");
        }
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen")
            .splitter = dereth_ui_screens::toolbar::splitter::Splitter::new(20);
    }
    c.tick(1);

    let app = c.app_mut();
    let box_h = text_element(app, dereth_ui_screens::toolbar::splitter::ENTRY_BOX);
    let mut hand = KeyHand::new();
    press_text_element(app, &mut hand, box_h);
    let focused = kb_ui(app).focus_element() == Some(box_h);

    // This shape is the one that matters: the number is read the way a program reads one, so an
    // unfiltered box would read a leading zero and an x as sixteen-and-something and a player
    // asking for twelve would move a different number of things. The filter is what makes that
    // reading safe.
    hand.type_text(app, "0x1x2");
    let digits_only = kb_text(app, box_h) == "012";
    let the_right_filter = filter_probe(app, box_h) == Some(NUMBER_PROBE);

    c.assert_behaviour(
        "text-entry.filters.the-how-many-box-takes-digits-only",
        move |_| focused && digits_only && the_right_filter,
    );
    c.shutdown();
}

#[test]
fn scenario_the_how_many_box_takes_digits_only() {
    scenario("the_how_many_box_takes_digits_only");
}

// ---------------------------------------------------------------------------------------------
// text-entry.ime.every-composition-message-is-handed-to-the-desktop
// ---------------------------------------------------------------------------------------------

/// The client writes no input method of its own: it offers every composition message on and lets
/// the desktop drive it.
pub fn every_composition_message_is_handed_to_the_desktop() {
    let mut pump = dereth_client::pump::Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;

    let eight = [
        win_msg::WM_INPUTLANGCHANGE,
        win_msg::WM_IME_STARTCOMPOSITION,
        win_msg::WM_IME_ENDCOMPOSITION,
        win_msg::WM_IME_COMPOSITION,
        win_msg::WM_IME_SETCONTEXT,
        win_msg::WM_IME_NOTIFY,
        win_msg::WM_IME_CONTROL,
        win_msg::WM_IME_COMPOSITIONFULL,
    ];
    let mut all_offered = true;
    for m in eight {
        let r = pump.dispatch(dereth_client::pump::Win32Message::new(m, 0, 0, 1_000));
        all_offered &= r.effects == vec![Effect::ForwardToBrowser] && !r.handled && r.result == 0;
    }

    // The control: an ordinary typed character takes the other road -- offered on, and then given
    // to the client's own input. Without it the eight above would prove nothing about the arm.
    let r = pump.dispatch(dereth_client::pump::Win32Message::new(
        win_msg::WM_CHAR,
        u32::from('a') as usize,
        0,
        1_100,
    ));
    let a_character_is_different =
        r.effects == vec![Effect::ForwardToBrowser, Effect::ForwardToInputManager];

    let mut c = HeadlessClient::model();
    c.assert_behaviour("text-entry.ime.every-composition-message-is-handed-to-the-desktop-and-an-ordinary-character-is-not", move |_| {
        all_offered && a_character_is_different
    });
}

#[test]
fn scenario_every_composition_message_is_handed_to_the_desktop() {
    scenario("every_composition_message_is_handed_to_the_desktop");
}

// ---------------------------------------------------------------------------------------------
// text-entry.ime.a-composed-character-reaches-the-box-unchanged
// ---------------------------------------------------------------------------------------------

/// A character an input method commits arrives like any other and must not be narrowed.
pub fn a_composed_character_reaches_the_box_unchanged() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(8));
    let app = c.app_mut();
    let entry = text_element(app, TEXT_CHAT_ENTRY);
    let takes_anything = kb_ui(app)
        .text_element_mut(entry)
        .expect("the entry")
        .filter
        .is_none();

    let mut hand = KeyHand::new();
    press_text_element(app, &mut hand, entry);
    let focused = kb_ui(app).focus_element() == Some(entry);

    // A plain letter, an accented one an input method or a dead key commits, a Chinese character,
    // and another plain letter.
    hand.type_units(app, &[0x0065, 0x00E9, 0x4E2D, 0x007A]);
    let unchanged = kb_text(app, entry) == "e\u{00E9}\u{4E2D}z";

    c.assert_behaviour(
        "text-entry.ime.a-composed-character-reaches-the-box-unchanged",
        move |_| takes_anything && focused && unchanged,
    );
    c.shutdown();
}

#[test]
fn scenario_a_composed_character_reaches_the_box_unchanged() {
    scenario("a_composed_character_reaches_the_box_unchanged");
}

// ---------------------------------------------------------------------------------------------
// text-entry.paste.a-paste-puts-the-clipboard-in-once-and-not-a-letter-with-it
// ---------------------------------------------------------------------------------------------

/// Pasting with the keyboard pastes, and does not also type the letter that was held.
pub fn a_paste_puts_the_clipboard_in_once_and_not_a_letter_with_it() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(8));
    let app = c.app_mut();
    let entry = text_element(app, TEXT_CHAT_ENTRY);
    let mut hand = KeyHand::new();
    press_text_element(app, &mut hand, entry);
    let focused = kb_ui(app).focus_element() == Some(entry);
    kb_ui(app).clipboard = "paste".to_string();

    // The desktop reports the character the key really produced with the modifier held, which is
    // not the letter on the key -- and the client must not put that letter in as well.
    hand.key(app, KeyCode::ControlLeft, true);
    hand.key_with_text(app, KeyCode::KeyV, Some("\u{16}"), false);
    app.frame();
    let pasted_once = kb_text(app, entry) == "paste";

    hand.key(app, KeyCode::KeyV, false);
    hand.key(app, KeyCode::ControlLeft, false);
    hand.key_with_text(app, KeyCode::KeyV, Some("v"), false);
    hand.key(app, KeyCode::KeyV, false);
    hand.key(app, KeyCode::ShiftLeft, true);
    hand.key_with_text(app, KeyCode::KeyV, Some("V"), false);
    hand.key(app, KeyCode::KeyV, false);
    hand.key(app, KeyCode::ShiftLeft, false);
    app.frame();
    let plain_and_shifted = kb_text(app, entry) == "pastevV";

    // And a character the desktop produces with a keyboard of its own, and one it produces with
    // the right-hand alt key, still reach the box.
    let t = hand.time_ms + 10;
    for m in key_text_messages(true, false, Some("\u{4E2D}"), t) {
        hand.send(app, m);
    }
    let t = hand.time_ms + 20;
    for m in key_text_messages(true, true, Some("\u{20AC}"), t) {
        hand.send(app, m);
    }
    app.frame();
    let other_layouts = kb_text(app, entry) == "pastevV\u{4E2D}\u{20AC}";

    c.assert_behaviour(
        "text-entry.paste.a-paste-puts-the-clipboard-in-once-and-not-a-letter-with-it",
        move |_| focused && pasted_once && plain_and_shifted && other_layouts,
    );
    c.shutdown();
}

#[test]
fn scenario_a_paste_puts_the_clipboard_in_once_and_not_a_letter_with_it() {
    scenario("a_paste_puts_the_clipboard_in_once_and_not_a_letter_with_it");
}

// ---------------------------------------------------------------------------------------------
// text-entry.paste.line-breaks-in-what-was-pasted-never-reach-the-shard
// ---------------------------------------------------------------------------------------------

/// Pasting several lines into a one-line box leaves one line, and that is what is said.
pub fn line_breaks_in_what_was_pasted_never_reach_the_shard() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(8));
    let mut net =
        dereth_client::net::ClientNetwork::new("127.0.0.1:19000", 7304, "paste", "unused", 0)
            .expect("a socket-free endpoint");
    net.session.transport.add_connection(
        0xB,
        0,
        1,
        0xDEAD_BEEF,
        0x1234_5678,
        Some("127.0.0.1:19000".parse().expect("the peer address")),
    );
    c.attach_replay(net);

    let app = c.app_mut();
    let entry = text_element(app, TEXT_CHAT_ENTRY);
    let mut hand = KeyHand::new();
    press_text_element(app, &mut hand, entry);
    kb_ui(app).clipboard = "one\r\ntwo\nthree\rfour\tfive".into();
    hand.key(app, KeyCode::ControlLeft, true);
    hand.key_with_text(app, KeyCode::KeyV, Some("\u{16}"), false);
    hand.key(app, KeyCode::KeyV, false);
    hand.key(app, KeyCode::ControlLeft, false);
    app.frame();

    let one_line = kb_text(app, entry) == "onetwothreefourfive";
    // A pasted line break is not a press of the return key: the line is not sent by pasting it.
    let not_sent =
        kb_ui(app).focus_element() == Some(entry) && app.interaction().last_sent.is_empty();

    // The two edges of the return key with no frame between them, so that the frame which turns
    // them into a line is the harness's own -- which is what records the line for the reading
    // below. What the client last asked for is a one-frame window that the next frame writes
    // over, so a scenario that ran its own frames here would find it already gone.
    hand.key_quiet(app, KeyCode::Enter, true);
    hand.key_quiet(app, KeyCode::Enter, false);
    c.tick(1);
    let said = c
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::Talk(m) => Some(m.message.clone()),
            _ => None,
        })
        .collect::<Vec<String>>()
        == vec!["onetwothreefourfive".to_string()];
    c.tick(1);

    let mut actions = Vec::new();
    for (bytes, _) in c.replay_net_mut().expect("the endpoint").take_outgoing() {
        let packet =
            dereth_transport::wire::ParsedPacket::parse(&bytes).expect("the endpoint's output");
        for fragment in packet.fragments {
            if fragment.header.queue_id == 3 {
                assert_eq!(fragment.header.num_frags, 1);
                actions.push(fragment.payload);
            }
        }
    }
    let mut expected = vec![0xb1, 0xf7, 0, 0, 1, 0, 0, 0, 0x15, 0, 0, 0, 19, 0];
    expected.extend_from_slice(b"onetwothreefourfive");
    expected.extend_from_slice(&[0, 0, 0]);
    let one_request = actions == vec![expected];
    let cleared = kb_text(c.app_mut(), entry).is_empty();

    c.assert_behaviour(
        "text-entry.paste.line-breaks-in-what-was-pasted-never-reach-the-shard",
        move |_| one_line && not_sent && said && one_request && cleared,
    );
    c.shutdown();
}

#[test]
fn scenario_line_breaks_in_what_was_pasted_never_reach_the_shard() {
    scenario("line_breaks_in_what_was_pasted_never_reach_the_shard");
}

// ---------------------------------------------------------------------------------------------
// text-entry.filters.the-how-many-of-a-component-box-takes-digits-only-on-every-row
// ---------------------------------------------------------------------------------------------

/// long-solo-play's own character.
const COMPONENT_PLAYER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_000A);
/// The panel page the spell pages live in.
const SPELL_PAGE_ID: ElementId = dereth_ui_screens::panels::remaining::SPELL_PAGE;
/// The list of components, which is how the right sub-page is found without naming it.
const COMPONENT_LIST_ID: ElementId = dereth_ui_screens::panels::spellcomponent::COMPONENT_LIST;

/// The box beside each spell component, where a player says how many to keep.
pub fn the_how_many_of_a_component_box_takes_digits_only_on_every_row() {
    let n = dereth_client_net::client_session::testing::Corpus::load("long-solo-play")
        .expect("the recordings are committed to the repository")
        .expect("long-solo-play is one of them")
        .blobs
        .len();

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.app_mut().objects_mut().world.player = Some(COMPONENT_PLAYER);
    c.when(dereth_testkit::Inbound::from_corpus("long-solo-play", 0..n));
    c.tick(3);

    // Raise the page the components live on, the way the screen does, and then press the tab a
    // player presses -- found by which sub-page carries the list rather than by naming it.
    {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the UI shell is up");
        let any: &mut dyn std::any::Any = &mut **shell.flow.current_mut().expect("a screen");
        let s = any
            .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen");
        let panel = s
            .panels
            .pages
            .iter()
            .find(|p| p.element == SPELL_PAGE_ID)
            .expect("the spell page is a registered page")
            .panel_id;
        s.recv_set_panel_visibility(&mut shell.ui, panel, true);
    }
    c.tick(3);

    let app = c.app_mut();
    let tab = {
        let u = kb_ui(app);
        let page = text_element_in(u, SPELL_PAGE_ID);
        let pairs: Vec<(ElementId, ElementId)> = u
            .node(page)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .expect("the spell page is a panel")
            .page_to_tab
            .iter()
            .map(|(p, t)| (*p, *t))
            .collect();
        let mut found = None;
        for (page_id, tab_id) in pairs {
            let Some(pe) = u.get_child_recursive(page, page_id) else {
                continue;
            };
            if u.get_child_recursive(pe, COMPONENT_LIST_ID).is_some() {
                found = u.get_child_recursive(page, tab_id);
                break;
            }
        }
        found.expect("one sub-page of the spell page carries the component list")
    };
    let mut hand = KeyHand::new();
    let at = {
        let u = kb_ui(app);
        let b = u.screen_clip_box(tab);
        assert!(b.is_valid(), "the tab has a real rectangle");
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    hand.click_at(app, at);
    app.frame();
    app.frame();

    let rows: Vec<dereth_ui_screens::panels::spellcomponent::DrawnRow> =
        app.hud().panels.spell_components.rows.clone();
    let there_are_rows = !rows.is_empty();
    let field = rows[0]
        .desired_field
        .expect("the first row carries the box");

    press_text_element(app, &mut hand, field);
    let focused = kb_ui(app).focus_element() == Some(field);
    // This box is one of the two in the whole shipped layout that select everything on the first
    // press, so the first accepted key replaces what was there.
    hand.type_text(app, "9zz9");
    let digits_only = kb_text(app, field) == "99";

    // Every row, not only the one typed into: the box is set up inside the per-component loop, so
    // a client that did it once when the panel was built would pass the reading above and fail
    // here on the second row.
    let mut every_row = true;
    for r in &rows {
        let h = r.desired_field.expect("every row carries the box");
        every_row &= filter_probe(app, h) == Some(NUMBER_PROBE);
    }

    c.assert_behaviour(
        "text-entry.filters.the-how-many-of-a-component-box-takes-digits-only-on-every-row",
        move |_| there_are_rows && focused && digits_only && every_row,
    );
    c.shutdown();
}

/// An element by id under the element manager's own root, off a system the caller already holds.
fn text_element_in(u: &dereth_ui::UiSystem, id: ElementId) -> dereth_ui::ElemHandle {
    u.get_element(id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

#[test]
fn scenario_the_how_many_of_a_component_box_takes_digits_only_on_every_row() {
    scenario("the_how_many_of_a_component_box_takes_digits_only_on_every_row");
}

// =============================================================================================
// pointer.click.* -- a real press works on every screen
//
// A real press reaches the character-creation wizard -- a tab, an arrow, the way out -- just as it
// reaches the character list. The wheel's own map is not a row of its own: its data half is folded
// into the map row here, and its other half would be a constant beside the symbol the client
// carries it through.
//
// The rows are in this subject rather than the one the census names, because everything they are
// about -- the wizard, its tabs, its arrows -- is here.
// =============================================================================================

/// A client on the character list, then in the wizard, reached the way a player reaches it.
fn a_client_in_the_wizard_from_the_list(hands: &mut Hands) -> HeadlessClient {
    let mut c = a_client_on_character_select();
    let create = element(&c, ElementId(0x1000_03A0));
    hands.click_handle(&mut c, create);
    assert_eq!(
        current_screen(&c),
        Some(mode::CHAR_GEN),
        "the press reached the character list and it put the wizard up"
    );
    c.tick(1);
    c
}

/// Press an element at its own middle, without asking the hit test to resolve to it exactly --
/// a tab's middle can land on a caption inside it, which is still the tab being pressed.
fn press_at_middle(c: &mut HeadlessClient, hands: &mut Hands, id: ElementId) {
    let h = element(c, id);
    let (x, y) = middle_of(c, h);
    hands.click_at(c, x, y);
}

// ---------------------------------------------------------------------------------------------
// pointer.click.the-same-press-works-on-the-character-list-and-in-the-wizard
// ---------------------------------------------------------------------------------------------

/// The same press, on the screen that answered it and the screen that did not.
pub fn the_same_press_works_on_the_character_list_and_in_the_wizard() {
    let mut hands = Hands::new();
    // The character-list half is the control: if it stops working the press is gone and the
    // wizard half proves nothing.
    let mut c = a_client_in_the_wizard_from_the_list(&mut hands);

    let opens_on_the_first = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Hertage;
    // The profession tab -- one of the three the wizard greys on every page change, and the press
    // that was reported reaching nothing.
    press_at_middle(&mut c, &mut hands, ElementId(0x1000_03F0));
    let the_tab_answered = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Profession;

    // Both presses went through the client's own pointer, which is what makes them real.
    let stats = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats;
    let two_real_presses = stats.mouse_downs == 2 && stats.mouse_ups == 2;

    c.assert_behaviour(
        "pointer.click.the-same-press-works-on-the-character-list-and-in-the-wizard",
        move |_| opens_on_the_first && the_tab_answered && two_real_presses,
    );
    c.shutdown();
}

#[test]
fn scenario_the_same_press_works_on_the_character_list_and_in_the_wizard() {
    scenario("the_same_press_works_on_the_character_list_and_in_the_wizard");
}

// ---------------------------------------------------------------------------------------------
// pointer.click.the-wizards-arrows-and-its-way-out-all-answer-a-press
// ---------------------------------------------------------------------------------------------

/// The other two things reported: an arrow, and the way out.
pub fn the_wizards_arrows_and_its_way_out_all_answer_a_press() {
    let mut hands = Hands::new();
    let mut c = a_client_in_the_wizard_from_the_list(&mut hands);

    press_at_middle(&mut c, &mut hands, ElementId(0x1000_03C7));
    let forward = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Profession;
    press_at_middle(&mut c, &mut hands, ElementId(0x1000_03C6));
    let back = with_wizard(&mut c, |_, w| w.progress) == EcgProgress::Hertage;

    press_at_middle(&mut c, &mut hands, ElementId(0x1000_03CA));
    let the_way_out = with_wizard(&mut c, |_, w| w.open_dialog) == Some(CharGenDialog::Exit);

    c.assert_behaviour(
        "pointer.click.the-wizards-arrows-and-its-way-out-all-answer-a-press",
        move |_| forward && back && the_way_out,
    );
    c.shutdown();
}

#[test]
fn scenario_the_wizards_arrows_and_its_way_out_all_answer_a_press() {
    scenario("the_wizards_arrows_and_its_way_out_all_answer_a_press");
}

// ---------------------------------------------------------------------------------------------
// pointer.click.every-button-the-wizard-builds-can-be-pressed-including-the-three-it-greys
// ---------------------------------------------------------------------------------------------

/// The defect itself, read off the tree rather than through a press: a button that is greyed must
/// not stop being something a pointer can land on.
pub fn every_button_the_wizard_builds_can_be_pressed() {
    let mut hands = Hands::new();
    let mut c = a_client_in_the_wizard_from_the_list(&mut hands);
    // Change the page, so the greying has been written at least twice.
    press_at_middle(&mut c, &mut hands, ElementId(0x1000_03F2));

    const BUTTON: dereth_ui::ElementType = dereth_ui::ElementType(1);
    let (buttons, dead) = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let root = shell.flow.current().expect("the wizard").roots()[0];
        fn walk(
            ui: &dereth_ui::UiSystem,
            h: dereth_ui::ElemHandle,
            buttons: &mut u32,
            dead: &mut Vec<u32>,
        ) {
            if let Some(n) = ui.node(h) {
                if n.desc.ty == BUTTON {
                    *buttons += 1;
                    if !n.is_mouse_visible {
                        dead.push(n.element_id().0);
                    }
                }
            }
            for child in ui.children(h) {
                walk(ui, child, buttons, dead);
            }
        }
        let mut buttons = 0_u32;
        let mut dead = Vec::new();
        walk(&shell.ui, root, &mut buttons, &mut dead);
        (buttons, dead)
    };
    // The denominator, so a sweep that found nothing cannot pass for a sweep that found nothing
    // wrong.
    let the_whole_tree = buttons > 100;
    let none_is_dead = dead.is_empty();

    // ...and the three the wizard greys on every page change, by name -- a sweep that happens to
    // pass says less than one that names its suspects.
    let mut the_three = true;
    for id in [0x1000_03F0_u32, 0x1000_03F1, 0x1000_03F3] {
        let h = element(&c, ElementId(id));
        the_three &= c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("alive")
            .is_mouse_visible;
    }

    c.assert_behaviour(
        "pointer.click.every-button-the-wizard-builds-can-be-pressed-including-the-three-it-greys",
        move |_| the_whole_tree && none_is_dead && the_three,
    );
    c.shutdown();
}

#[test]
fn scenario_every_button_the_wizard_builds_can_be_pressed() {
    scenario("every_button_the_wizard_builds_can_be_pressed");
}

// ---------------------------------------------------------------------------------------------
// pointer.the-left-button-and-the-wheel-each-belong-to-one-shipped-map
// ---------------------------------------------------------------------------------------------

/// Which maps the shipped keys bind a press and a wheel turn in, read out of the shipped data at
/// run time rather than taken from a document.
pub fn the_left_button_and_the_wheel_each_belong_to_one_shipped_map() {
    let mut c = a_client_on_character_select();

    let binders = |c: &mut HeadlessClient, offset: u16| -> Vec<u32> {
        let input = c
            .app_mut()
            .input_manager_mut()
            .expect("the input shell is up");
        input
            .manager
            .keymap
            .sections
            .iter()
            .filter(|s| {
                s.bindings().iter().any(|(qc, _)| {
                    input.manager.keymap.device_type_of(qc.control)
                        == Some(dereth_input::spec::DeviceType::Mouse)
                        && qc.control.offset() == offset
                })
            })
            .map(|s| s.input_map_id.0)
            .collect()
    };

    // The left button: two shipped maps bind it, and only one of them is a map this client ever
    // puts in front of itself -- the other belongs to something that does not exist yet, and a
    // map nothing registers can bind whatever it likes and never win. So the interface's own
    // filter cannot be what kept a real press out.
    let left = binders(&mut c, 0x0C);
    let two_binders = left == vec![0x1000_000B, dereth_client::ui::UI_INPUT_MAP.0];
    let only_one_registered: Vec<u32> = dereth_client::input::BASE_MAP_REGISTRATIONS
        .iter()
        .map(|(_, m, _)| *m)
        .filter(|m| left.contains(m))
        .collect();
    let the_interfaces_own = only_one_registered == vec![dereth_client::ui::UI_INPUT_MAP.0];

    // The wheel: exactly one shipped map binds it, and it is not one the client registers at
    // start-up -- it goes in when something takes the keyboard, which is what the wheel scenarios
    // are about.
    let wheel = binders(&mut c, 0x08);
    let one_binder = wheel == vec![0x0A]
        && !dereth_client::input::BASE_MAP_REGISTRATIONS
            .iter()
            .any(|(_, m, _)| *m == 0x0A);

    // And what a real press really produces carries that map and that action.
    let mut hands = Hands::new();
    hands.move_to(&mut c, 100, 100);
    let m = hands.button_message(dereth_client::platform::keys::MouseButton::Left, true);
    hands.send(&mut c, m);
    let (map, action) = {
        let input = c
            .app_mut()
            .input_manager_mut()
            .expect("the input shell is up");
        input.use_time(dereth_primitives::LocalTime(1.0));
        let events = input.take_events();
        let click = events
            .iter()
            .find(|e| e.start)
            .expect("a press produces one action");
        (click.input_map, click.action.0)
    };
    let the_press_carries_it = map == dereth_client::ui::UI_INPUT_MAP && action == 7;

    c.assert_behaviour(
        "pointer.the-left-button-and-the-wheel-each-belong-to-one-shipped-map",
        move |_| two_binders && the_interfaces_own && one_binder && the_press_carries_it,
    );
    c.shutdown();
}

#[test]
fn scenario_the_left_button_and_the_wheel_each_belong_to_one_shipped_map() {
    scenario("the_left_button_and_the_wheel_each_belong_to_one_shipped_map");
}

// =============================================================================================
// viewport.* (dat) -- the two halves that need the shipped geometry and a whole client
//
// The three arithmetic rows are in the `cpu` tier's `shell.rs`, and the reasoning is there.
// =============================================================================================

/// The window, throughout. Every press is given in **window** coordinates.
const VIEW_WINDOW: (u32, u32) = (800, 600);
/// The whole window as a view.
const VIEW_WHOLE: dereth_primitives::viewport::Viewport = dereth_primitives::viewport::Viewport {
    x: 0,
    y: 0,
    width: VIEW_WINDOW.0,
    height: VIEW_WINDOW.1,
};
/// A moved and resized view, deliberately off-centre.
const VIEW_INSET: dereth_primitives::viewport::Viewport = dereth_primitives::viewport::Viewport {
    x: 240,
    y: 90,
    width: 400,
    height: 300,
};
/// That view's own middle, and the window's, which are not the same point.
const VIEW_INSET_MIDDLE: (i32, i32) = (240 + 199, 90 + 149);
const VIEW_WINDOW_MIDDLE: (i32, i32) = (399, 299);

/// The thing to be found, and the shape it is drawn with -- a real body from the shipped data, so
/// the sweep has geometry to sweep.
const VIEW_TARGET: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x8300_0F01);
const VIEW_BODY: dereth_primitives::DataId = dereth_primitives::DataId(0x0200_0001);

/// The viewer and the one thing in front of it -- all the sweep reads, and the reason this needs
/// no device.
struct AView {
    viewer: dereth_primitives::Frame,
    frames: std::collections::BTreeMap<dereth_primitives::ObjectId, dereth_primitives::Frame>,
}

impl dereth_client::pick::PickScene for AView {
    fn viewer(&self) -> dereth_primitives::Frame {
        self.viewer
    }
    fn object_frame(&self, id: dereth_primitives::ObjectId) -> Option<dereth_primitives::Frame> {
        self.frames.get(&id).copied()
    }
    fn fov_y_rad(&self, _window: (u32, u32)) -> f32 {
        dereth_client_contract::camera::DEFAULT_FOV_DEGREES
            * dereth_client_contract::camera::DEG_TO_RAD
    }
}

/// One thing three metres due north of the eye and level with it.
fn one_thing_ahead(
    store: &std::sync::Arc<dereth_dat::RetailDatStore>,
) -> (dereth_client::objects::ObjectStream, AView) {
    let mut objects = dereth_client::objects::ObjectStream::with_store(store.clone());
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id: VIEW_TARGET,
        objdesc: dereth_protocol::types::ObjDesc::default(),
        physicsdesc: dereth_protocol::types::PhysicsDesc {
            bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                | dereth_protocol::types::physicsdesc::flags::SETUP,
            setup_id: Some(VIEW_BODY.0),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: 0x0001_0001,
                frame: dereth_protocol::types::Frame {
                    origin: dereth_primitives::Vec3::ZERO.into(),
                    orientation: dereth_primitives::Quat::new(1.0, 0.0, 0.0, 0.0).into(),
                },
            }),
            ..dereth_protocol::types::PhysicsDesc::default()
        },
        wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
    };
    let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
        .expect("an object create encodes");
    objects.apply_event(
        &dereth_client_net::client_session::SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        dereth_primitives::LocalTime(1.0),
    );
    let identity = dereth_primitives::Quat::new(1.0, 0.0, 0.0, 0.0);
    let view = AView {
        viewer: dereth_primitives::Frame::new(
            dereth_primitives::Vec3::new(0.0, -3.0, 0.7),
            identity,
        ),
        frames: [(
            VIEW_TARGET,
            dereth_primitives::Frame::new(dereth_primitives::Vec3::new(0.0, 0.0, 0.0), identity),
        )]
        .into_iter()
        .collect(),
    };
    (objects, view)
}

/// One whole look: aim at a window point, sweep, and answer what was found.
fn what_is_found_at(
    store: &dereth_dat::RetailDatStore,
    objects: &dereth_client::objects::ObjectStream,
    view: &AView,
    at: (i32, i32),
    viewport: dereth_primitives::viewport::Viewport,
) -> Option<dereth_primitives::ObjectId> {
    let mut pick = dereth_client::pick::WorldPicker::new();
    if !pick.find_object(at.0, at.1, viewport) {
        return None;
    }
    pick.draw_no_blit(store, view, objects, VIEW_WINDOW, viewport)
}

// =============================================================================================
// viewport.a-press-in-the-middle-of-a-moved-view-finds-what-is-ahead-of-the-eye
// =============================================================================================

/// The rejecting half is the window's middle: a client that ignored the view's corner would find
/// the thing from there, which is exactly what the reported one did.
pub fn a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye() {
    let mut c = HeadlessClient::new(ClientSpec::retail());
    let store = c
        .dat_store()
        .expect("the retail data files are open")
        .clone();
    let (objects, view) = one_thing_ahead(&store);

    // The control first, so that a client in which nothing is findable at all is caught here
    // rather than read as a pass below.
    let found_through_the_whole_window =
        what_is_found_at(&store, &objects, &view, VIEW_WINDOW_MIDDLE, VIEW_WHOLE)
            == Some(VIEW_TARGET);

    let found_through_the_moved_view =
        what_is_found_at(&store, &objects, &view, VIEW_INSET_MIDDLE, VIEW_INSET)
            == Some(VIEW_TARGET);

    // ...and the window's middle is not the view's, so aiming there must miss.
    let a_real_difference = VIEW_INSET_MIDDLE != VIEW_WINDOW_MIDDLE;
    let the_windows_middle_misses =
        what_is_found_at(&store, &objects, &view, VIEW_WINDOW_MIDDLE, VIEW_INSET)
            != Some(VIEW_TARGET);

    c.assert_behaviour(
        "viewport.a-press-in-the-middle-of-a-moved-view-finds-what-is-ahead-of-the-eye",
        move |_| {
            found_through_the_whole_window
                && found_through_the_moved_view
                && a_real_difference
                && the_windows_middle_misses
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye() {
    scenario("a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye");
}

// =============================================================================================
// viewport.a-really-moved-view-measures-a-press-against-itself-and-the-default-is-not
// =============================================================================================

/// One press on the world, over no part of the interface, through the client's own step.
fn a_press_on_the_world(c: &mut HeadlessClient, x: i32, y: i32) -> (bool, Option<(f32, f32)>) {
    let e = dereth_client::ui::UiMouseEvent {
        action: dereth_ui::focus::action::PRIMARY_CLICK,
        start: true,
        x,
        y,
        over: None,
    };
    let app = c.app_mut();
    let armed = app.interaction_mut().wrapper_mouse(
        e,
        VIEW_WINDOW,
        dereth_client::interaction::is_world_click(e.over),
    );
    (armed, app.interaction().pick.selection_cursor())
}

/// The acceptance: the view is really moved and resized in a running client, a frame runs, and a
/// press is measured against the box the player is now looking through.
pub fn a_really_moved_view_measures_a_press_against_itself() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(2));
    let sbox = element(&c, dereth_ui_screens::hud::world_view::SMART_BOX);

    // The view as it ships: it fills the window from its corner, so the window's number is the
    // view's number and every press in the window is taken.
    let shipped = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .screen_box(sbox);
    let docked_at_the_corner = (shipped.x0, shipped.y0) == (0, 0);
    let mut the_default_takes_everything = true;
    for (x, y) in [(0, 0), (399, 299), (799, 599), (1, 598)] {
        let (armed, at) = a_press_on_the_world(&mut c, x, y);
        the_default_takes_everything &= armed && at == Some((x as f32, y as f32));
    }
    let nothing_refused_yet = c
        .view()
        .expect_app()
        .interaction()
        .pick
        .stats
        .outside_viewport
        == 0;

    // Now really move and resize it.
    {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        shell.ui.resize_to(sbox, 300, 200);
        shell.ui.move_to(sbox, 120, 80);
    }
    c.tick(1);
    let moved = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .screen_box(sbox);
    let it_really_moved = moved != shipped && moved.x0 > 0 && moved.y0 > 0;

    // Its own corner is the view's origin.
    let (armed, at) = a_press_on_the_world(&mut c, moved.x0, moved.y0);
    let its_corner_is_the_origin = armed && at == Some((0.0, 0.0));

    // Its own middle, in window coordinates, is the view's middle and not the window's.
    let (cx, cy) = (moved.x0 + moved.width() / 2, moved.y0 + moved.height() / 2);
    let (armed, at) = a_press_on_the_world(&mut c, cx, cy);
    let its_middle_is_the_views =
        armed && at == Some(((moved.width() / 2) as f32, (moved.height() / 2) as f32));

    // ...and the window's own corner is now outside it, and is refused rather than dropped.
    let refusals = c
        .view()
        .expect_app()
        .interaction()
        .pick
        .stats
        .outside_viewport;
    let (armed, _) = a_press_on_the_world(&mut c, 4, 4);
    let outside_is_refused = !armed
        && c.view()
            .expect_app()
            .interaction()
            .pick
            .stats
            .outside_viewport
            == refusals + 1;

    c.assert_behaviour(
        "viewport.a-really-moved-view-measures-a-press-against-itself-and-the-default-is-not",
        move |_| {
            docked_at_the_corner
                && the_default_takes_everything
                && nothing_refused_yet
                && it_really_moved
                && its_corner_is_the_origin
                && its_middle_is_the_views
                && outside_is_refused
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_really_moved_view_measures_a_press_against_itself() {
    scenario("a_really_moved_view_measures_a_press_against_itself");
}

// =============================================================================================
// pointer.* -- a real press reaches the screen that owns what it landed on
//
// A test that broadcasts a message onto the tree proves the handler while saying nothing about
// whether a press can reach it; these do not. Eight scenarios, six rows: the three character-list
// buttons are one claim with four arms.
//
// Nothing here puts a message on the tree: every gesture below is the harness's own pointer,
// which builds the client's real window messages and delivers them where the window loop does.
// =============================================================================================

/// The buttons along the bottom of the character list, and the rows above them.
const LIST_CREATE: ElementId = ElementId(0x1000_03A0);
const LIST_CREDITS: ElementId = ElementId(0x1000_03A3);
const LIST_EXIT: ElementId = ElementId(0x1000_03A4);
const LIST_DELETE: ElementId = ElementId(0x1000_039F);

// ---------------------------------------------------------------------------------------------
// pointer.click.the-character-lists-own-buttons-each-raise-what-they-name
// ---------------------------------------------------------------------------------------------

/// Four buttons, four different things, one client -- and the delete question is dismissed in
/// between, because it is a real box and the button behind it cannot be pressed through it.
pub fn the_character_lists_own_buttons_each_raise_what_they_name() {
    use dereth_ui_screens::screens::charmgmt::DialogContext;

    let mut hands = Hands::new();

    // The delete question first: it refuses with nothing chosen, and the list chose a row for
    // itself when it was built.
    let mut c = a_client_on_character_select();
    let something_is_chosen = with_charmgmt_screen(&mut c, |s| s.selected.is_some());
    // ...and the button is something the pointer can land on at all, which is where this starts.
    let it_can_be_pressed = {
        let h = element(&c, LIST_DELETE);
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("alive")
            .is_mouse_visible
    };

    press_the_list_button(&mut c, &mut hands, LIST_DELETE);
    let the_delete_question_is_up =
        with_charmgmt_screen(&mut c, |s| s.open_dialog) == Some(DialogContext::DeleteCharacter);
    // A real box, not just a field: it has an element, and it is in the way.
    let box_handle =
        with_charmgmt_screen(&mut c, |s| s.dialog_element(DialogContext::DeleteCharacter));
    let it_is_a_real_box = box_handle.is_some();
    if let Some(d) = box_handle {
        let cancel = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .get_child_recursive(d, dereth_ui_screens::screens::charmgmt::TEXT_INPUT_CANCEL)
            .expect("the box has a way out");
        hands.click_handle(&mut c, cancel);
    }
    let it_closed =
        with_charmgmt_screen(&mut c, |s| s.dialog_element(DialogContext::DeleteCharacter))
            .is_none();

    // ...and then the question about leaving, on the same client, which is only reachable now the
    // first box is gone.
    press_the_list_button(&mut c, &mut hands, LIST_EXIT);
    let the_leaving_question_is_up = with_charmgmt_screen(&mut c, |s| s.open_dialog)
        == Some(DialogContext::ConfirmExit)
        && with_charmgmt_screen(&mut c, |s| s.dialog_element(DialogContext::ConfirmExit)).is_some();
    let three_real_presses = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .mouse_downs
        == 3;
    c.shutdown();

    // The two that change the screen get a client each, because each of them leaves the list.
    let mut c = a_client_on_character_select();
    press_the_list_button(&mut c, &mut hands, LIST_CREATE);
    let create_puts_the_wizard_up = current_screen(&c) == Some(mode::CHAR_GEN);
    c.shutdown();

    let mut c = a_client_on_character_select();
    press_the_list_button(&mut c, &mut hands, LIST_CREDITS);
    let credits_shows_the_credits = current_screen(&c) == Some(mode::CREDITS);

    c.assert_behaviour(
        "pointer.click.the-character-lists-own-buttons-each-raise-what-they-name",
        move |_| {
            something_is_chosen
                && it_can_be_pressed
                && the_delete_question_is_up
                && it_is_a_real_box
                && it_closed
                && the_leaving_question_is_up
                && three_real_presses
                && create_puts_the_wizard_up
                && credits_shows_the_credits
        },
    );
    c.shutdown();
}

/// Press one of the list's buttons where a person aiming at it would press.
fn press_the_list_button(c: &mut HeadlessClient, hands: &mut Hands, id: ElementId) {
    let h = element(c, id);
    let (x, y) = middle_of(c, h);
    hands.click_at(c, x, y);
}

#[test]
fn scenario_the_character_lists_own_buttons_each_raise_what_they_name() {
    scenario("the_character_lists_own_buttons_each_raise_what_they_name");
}

// ---------------------------------------------------------------------------------------------
// pointer.click.one-press-on-a-character-picks-it-and-two-takes-them-into-the-world
// ---------------------------------------------------------------------------------------------

/// The row pressed is the **last** one, which the list did not choose for itself, so the choice
/// changing is evidence rather than a coincidence.
pub fn one_press_on_a_character_picks_it_and_two_takes_them_into_the_world() {
    use dereth_client::platform::keys::MouseButton;

    let mut c = a_client_on_character_select();
    let rows = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let three_rows = rows.len() == 3;
    // A row the list did **not** choose for itself, so that the choice changing is evidence
    // rather than a coincidence -- which row that is is the list's business and not this
    // scenario's, so it is found rather than named.
    let already = with_charmgmt_screen(&mut c, |s| s.selected_id);
    let last = rows
        .iter()
        .find(|r| r.id != already)
        .expect("the list has a row it did not choose")
        .clone();
    let h = last.element.expect("the row is an element of its own");
    let it_takes_two_presses = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("alive")
        .flags
        .wants_dbl_clicks();
    let not_already_chosen = with_charmgmt_screen(&mut c, |s| s.selected_id) != last.id;

    let (x, y) = middle_of(&c, h);
    let mut hands = Hands::new();
    hands.click_at(&mut c, x, y);
    let one_press_picks_it = with_charmgmt_screen(&mut c, |s| s.selected_id) == last.id;
    let and_no_more = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .character_actions
        == 0;

    // The second press, straight after the first: that is what makes it a double one.
    for down in [true, false] {
        let m = hands.button_message(MouseButton::Left, down);
        hands.send(&mut c, m);
    }
    c.tick(1);
    let two_takes_them_in = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .character_actions
        == 1;

    c.assert_behaviour(
        "pointer.click.one-press-on-a-character-picks-it-and-two-takes-them-into-the-world",
        move |_| {
            three_rows
                && it_takes_two_presses
                && not_already_chosen
                && one_press_picks_it
                && and_no_more
                && two_takes_them_in
        },
    );
    c.shutdown();
}

#[test]
fn scenario_one_press_on_a_character_picks_it_and_two_takes_them_into_the_world() {
    scenario("one_press_on_a_character_picks_it_and_two_takes_them_into_the_world");
}

// ---------------------------------------------------------------------------------------------
// pointer.a-button-lights-under-the-pointer-and-sinks-under-the-press
// ---------------------------------------------------------------------------------------------

/// Four looks in order, and the third is the one a single-look scenario cannot see: still held,
/// pointer gone, and the button is lit rather than sunk.
pub fn a_button_lights_under_the_pointer_and_sinks_under_the_press() {
    use dereth_client::platform::keys::MouseButton;

    let mut c = a_client_on_character_select();
    let exit = element(&c, LIST_EXIT);
    let look = |c: &HeadlessClient| {
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(exit)
            .expect("alive")
            .state
    };
    // The shipped button asks to light up at all, which is what makes the lit look reachable.
    let it_asks_to_light = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(exit)
        .expect("alive")
        .merged_properties()
        .get_bool(dereth_ui::props::attr::ROLLOVER_HIGHLIGHT)
        == Some(true);
    let resting = look(&c) == dereth_ui::widgets::button::state::NORMAL;

    let (x, y) = middle_of(&c, exit);
    let mut hands = Hands::new();
    // Somewhere else first, so moving onto it is a real change.
    hands.move_to(&mut c, 4, 4);
    c.tick(1);
    hands.move_to(&mut c, x, y);
    c.tick(1);
    let lit = look(&c) == dereth_ui::widgets::button::state::ROLLOVER;

    let m = hands.button_message(MouseButton::Left, true);
    hands.send(&mut c, m);
    c.tick(1);
    let sunk = look(&c) == dereth_ui::widgets::button::state::PRESSED;

    // Still held, pointer off it.
    hands.move_to(&mut c, 4, 4);
    c.tick(1);
    let lit_again = look(&c) == dereth_ui::widgets::button::state::ROLLOVER;

    let m = hands.button_message(MouseButton::Left, false);
    hands.send(&mut c, m);
    c.tick(1);
    let resting_again = look(&c) == dereth_ui::widgets::button::state::NORMAL;

    c.assert_behaviour(
        "pointer.a-button-lights-under-the-pointer-and-sinks-under-the-press",
        move |_| it_asks_to_light && resting && lit && sunk && lit_again && resting_again,
    );
    c.shutdown();
}

#[test]
fn scenario_a_button_lights_under_the_pointer_and_sinks_under_the_press() {
    scenario("a_button_lights_under_the_pointer_and_sinks_under_the_press");
}

// ---------------------------------------------------------------------------------------------
// pointer.a-press-dragged-off-a-button-releases-it-without-firing-it
// ---------------------------------------------------------------------------------------------

/// The press and the release both happen -- which is what stops a client that simply lost the
/// gesture from passing -- and what the button does is not done.
pub fn a_press_dragged_off_a_button_releases_it_without_firing_it() {
    use dereth_client::platform::keys::MouseButton;

    let mut c = a_client_on_character_select();
    let credits = element(&c, LIST_CREDITS);
    let (x, y) = middle_of(&c, credits);
    let mut hands = Hands::new();

    hands.move_to(&mut c, x, y);
    let m = hands.button_message(MouseButton::Left, true);
    hands.send(&mut c, m);
    c.tick(1);

    // Off the button, and then let go. The button still holds the pointer, so the release does
    // reach it -- and goes no further.
    hands.move_to(&mut c, 4, 4);
    let m = hands.button_message(MouseButton::Left, false);
    hands.send(&mut c, m);
    c.tick(2);

    let it_did_not_fire = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);
    let stats = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats;
    let both_halves_happened = stats.mouse_downs == 1 && stats.mouse_ups == 1;

    c.assert_behaviour(
        "pointer.a-press-dragged-off-a-button-releases-it-without-firing-it",
        move |_| it_did_not_fire && both_halves_happened,
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_dragged_off_a_button_releases_it_without_firing_it() {
    scenario("a_press_dragged_off_a_button_releases_it_without_firing_it");
}

// ---------------------------------------------------------------------------------------------
// pointer.a-move-makes-what-is-under-the-pointer-the-one-entered-until-it-leaves
// ---------------------------------------------------------------------------------------------

/// This is the claim that fails first if the route from the window to the tree is cut, and it
/// leans on no screen's handler at all.
pub fn a_move_makes_what_is_under_the_pointer_the_one_entered() {
    let mut c = a_client_on_character_select();
    let create = element(&c, LIST_CREATE);
    let under = |c: &HeadlessClient| {
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .mouse_over()
    };
    let nothing_at_first = under(&c).is_none();

    let (x, y) = middle_of(&c, create);
    let mut hands = Hands::new();
    hands.move_to(&mut c, x, y);
    c.tick(1);
    let now_the_button = under(&c) == Some(create);

    // ...and the pointer leaving the window clears it, which is the one thing that does.
    hands.leave(&mut c);
    c.tick(1);
    let cleared = under(&c).is_none();

    c.assert_behaviour(
        "pointer.a-move-makes-what-is-under-the-pointer-the-one-entered-until-it-leaves",
        move |_| nothing_at_first && now_the_button && cleared,
    );
    c.shutdown();
}

#[test]
fn scenario_a_move_makes_what_is_under_the_pointer_the_one_entered() {
    scenario("a_move_makes_what_is_under_the_pointer_the_one_entered");
}

// ---------------------------------------------------------------------------------------------
// pointer.click.a-press-on-a-toolbar-button-opens-the-panel-it-owns
// ---------------------------------------------------------------------------------------------

/// **No panel is named.** Nothing pairs a button with a page but the layout's own attribute, so
/// this takes the first button the toolbar was given and asks what the stack did.
pub fn a_press_on_a_toolbar_button_opens_the_panel_it_owns() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let button = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let screen = shell.flow.current().expect("a screen is up");
        let any: &dyn std::any::Any = screen;
        any.downcast_ref::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen")
            .toolbar
            .buttons
            .first()
            .copied()
            .expect("the toolbar was given its panel buttons")
    };
    let stack = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_element(dereth_ui_screens::screens::gameplay::window::PANEL_STACK)
        .expect("the stack of panels is in the shipped layout");
    let visible = |c: &HeadlessClient, h: dereth_ui::ElemHandle| {
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("alive")
            .region
            .flags
            .visible
    };
    let it_starts_away = !visible(&c, stack);

    let (x, y) = middle_of(&c, button.handle);
    let mut hands = Hands::new();
    hands.click_at(&mut c, x, y);
    // One more frame so the stack's own showing settles.
    c.tick(1);
    let it_came_up = visible(&c, stack);

    c.assert_behaviour(
        "pointer.click.a-press-on-a-toolbar-button-opens-the-panel-it-owns",
        move |_| it_starts_away && it_came_up,
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_on_a_toolbar_button_opens_the_panel_it_owns() {
    scenario("a_press_on_a_toolbar_button_opens_the_panel_it_owns");
}

// =============================================================================================
// dialog-keys.* -- the three screens before the world, and which keys each of them answers
//
// Two rows. What else a pre-game screen does with a key is already booked on the
// `dialog-keys.credits.*`, `dialog-keys.character-select.*` and
// `dialog-keys.a-box-that-takes-typing-*` rows above; the table of which screen registers which
// map, as literals beside the client's own constants, would be a transcription and is not a row;
// and the premise of the last -- that both of these keys are one-shot in the shipped bindings -- is
// folded in below as the arm that says why the edge has to be made up.
// =============================================================================================

/// The two actions the pre-game screens listen for, and the one the camera map gives to a key
/// none of them own.
const PREGAME_ACCEPT: u32 = 0x25;
const PREGAME_ESCAPE: u32 = 0x27;
const CAMERA_TO_DEFAULT: u32 = 0x39;
const CAMERA_MAP: dereth_input::InputMapId = dereth_input::InputMapId(5);

/// How many times each pre-game screen has been asked about an action.
fn times_asked(c: &HeadlessClient) -> (u64, u64, u64) {
    let st = &c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats;
    (st.intro_actions, st.credits_actions, st.charmgmt_actions)
}

/// One action arriving at the client on its way **back up** -- the edge a real finger cannot
/// produce for these keys, declared as made up because there is no producer of one here.
fn a_key_coming_back_up(c: &mut HeadlessClient, action: u32) {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell is up")
        .inject_action(dereth_input::InputEvent {
            action: dereth_input::ActionId(action),
            input_map: dereth_input::MAP_DIALOG_BOXES,
            toggle: dereth_input::ToggleType::OneShot,
            extent: 0.0,
            start: false,
            repeat_delta: 0,
            repeat_total: 0,
            from_key_down: false,
        });
    c.tick(1);
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.credits.a-key-the-roll-does-not-own-leaves-it-running
// ---------------------------------------------------------------------------------------------

/// Both directions in one scenario: the key is shown to have been delivered, so "the roll survived"
/// cannot be read off a keyboard that had stopped working, and the key the roll does own is
/// pressed on the same screen afterwards as the calibration.
pub fn a_key_the_roll_does_not_own_leaves_it_running() {
    let mut c = a_client_on_the_credits();

    // The key is a real one and it is bound: the camera map gives it an action of its own, and
    // that map is registered for the whole run, the credits included.
    let to_default = dereth_testkit::input_steps::bound_scan_code(
        &mut c,
        dereth_input::ActionId(CAMERA_TO_DEFAULT),
        CAMERA_MAP,
    );
    let it_is_bound = to_default != 0;

    let broadcasts = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .key_presses_broadcast;
    let (_, asked_before, _) = times_asked(&c);

    dereth_testkit::input_steps::tap(&mut c, key(KeyCode::Numpad0));
    c.tick(1);

    let it_was_delivered = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .key_presses_broadcast
        > broadcasts;
    let the_roll_was_not_asked = times_asked(&c).1 == asked_before;
    let it_is_still_running =
        current_screen(&c) == Some(mode::CREDITS) && !with_credits(&mut c, |s| s.finished);

    // The calibration, on the same screen with the same hands: a key the roll does own ends it.
    dereth_testkit::input_steps::tap(&mut c, key(KeyCode::Escape));
    c.tick(2);
    let the_owned_key_still_ends_it = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);

    c.assert_behaviour(
        "dialog-keys.credits.a-key-the-roll-does-not-own-leaves-it-running",
        move |_| {
            it_is_bound
                && it_was_delivered
                && the_roll_was_not_asked
                && it_is_still_running
                && the_owned_key_still_ends_it
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_key_the_roll_does_not_own_leaves_it_running() {
    scenario("a_key_the_roll_does_not_own_leaves_it_running");
}

// ---------------------------------------------------------------------------------------------
// dialog-keys.only-the-opening-sequence-refuses-a-key-on-its-way-back-up
// ---------------------------------------------------------------------------------------------

/// Three screens through one seam, which is what makes this worth asserting: a client that hoisted
/// the opening sequence's own refusal out of its arm and applied it to all three would read
/// nothing-happened three times and pass every other scenario about these keys.
pub fn only_the_opening_sequence_refuses_a_key_on_its_way_back_up() {
    // The premise, read out of the **shipped** bindings rather than written down: both keys these
    // screens listen for are one-shot, so the coming-up edge is never made for them and the made
    // up one below is the only way to see the difference at all.
    let mut c = a_client_on_the_intro();
    let both_are_one_shot = {
        let shell = c
            .app_mut()
            .input_manager_mut()
            .expect("the input shell is up");
        [PREGAME_ACCEPT, PREGAME_ESCAPE].into_iter().all(|a| {
            shell
                .manager
                .action_map
                .toggle_type(dereth_input::MAP_DIALOG_BOXES, dereth_input::ActionId(a))
                == dereth_input::ToggleType::OneShot
        })
    } && !dereth_input::ToggleType::OneShot.is_hold();

    // The opening sequence refuses it...
    a_key_coming_back_up(&mut c, PREGAME_ESCAPE);
    let the_sequence_refused_it = times_asked(&c).0 == 0;
    // ...and the same screen answers the press, which is what makes that nothing a reading.
    dereth_testkit::input_steps::tap(&mut c, key(KeyCode::Enter));
    c.tick(1);
    let but_it_answers_a_press = times_asked(&c).0 >= 1;
    c.shutdown();

    // The credit roll answers it, and a coming-up edge alone ends the roll.
    let mut c = a_client_on_the_credits();
    a_key_coming_back_up(&mut c, PREGAME_ESCAPE);
    c.tick(1);
    let the_roll_answered_it =
        times_asked(&c).1 == 1 && current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);
    c.shutdown();

    // So does the character list, and a coming-up edge alone raises the question about leaving.
    let mut c = a_client_on_character_select();
    a_key_coming_back_up(&mut c, PREGAME_ESCAPE);
    c.tick(1);
    let the_list_answered_it =
        times_asked(&c).2 == 1 && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_some();

    c.assert_behaviour(
        "dialog-keys.only-the-opening-sequence-refuses-a-key-on-its-way-back-up",
        move |_| {
            both_are_one_shot
                && the_sequence_refused_it
                && but_it_answers_a_press
                && the_roll_answered_it
                && the_list_answered_it
        },
    );
    c.shutdown();
}

#[test]
fn scenario_only_the_opening_sequence_refuses_a_key_on_its_way_back_up() {
    scenario("only_the_opening_sequence_refuses_a_key_on_its_way_back_up");
}

// =============================================================================================
// The login flow, end to end
//
// Seven scenarios. Five of them drive a whole `App` with a shard on the other end of a socket-free
// endpoint: the character list arrives as a real message through the transport, and what the client
// sends back is read off its own outgoing datagrams rather than out of its memory. Two of them need
// the bare shell -- `ClientSpec::shell(HostState)` -- for the one thing an `App` cannot be told:
// that a second, *identical*, character list has arrived.
//
// Nothing here binds a socket. [`Peer`] is the harness's own shard and every datagram it writes
// is handed straight to the client's transport.
// =============================================================================================

use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::ObjectId;
use dereth_testkit::replay::Peer;

/// The queue the shard's login messages ride on, which is what the client listens for a character
/// list on.
const LOGIN_QUEUE: u16 = 9;

/// The character this family deletes. It is the **first** the shard lists and sorts **last** by
/// name, so a client that confused a place in the sorted list with a place in the shard's own
/// could never pass here.
const WIRE_DOOMED: ObjectId = ObjectId(0x5000_0003);

/// The character this family restores. It is the **last** the shard lists and sorts into the
/// **middle**, so "back in its own place" is a claim that can fail: while it is waiting to be
/// deleted it is drawn at the bottom instead.
const WIRE_LAPSED: ObjectId = ObjectId(0x5000_0002);

/// The account the shard's list carries, which is the string a delete must carry back.
const WIRE_ACCOUNT: &str = "acct0001";

/// The shard's own list of three, with at most one of them waiting to be deleted.
fn wire_character_set(
    pending: Option<(ObjectId, u32)>,
) -> dereth_protocol::login::LoginCharacterSet {
    let named = |gid: ObjectId, name: &str| dereth_protocol::login::CharacterIdentity {
        gid,
        name: name.into(),
        seconds_greyed_out: match pending {
            Some((p, s)) if p == gid => s,
            _ => 0,
        },
    };
    dereth_protocol::login::LoginCharacterSet {
        status: 0,
        characters: vec![
            named(WIRE_DOOMED, "Zeddish"),
            named(ObjectId(0x5000_0001), "Larktest"),
            named(WIRE_LAPSED, "Tarinell"),
        ],
        deleted: vec![],
        num_allowed_characters: 5,
        account: WIRE_ACCOUNT.into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 0,
    }
}

/// The shard sends its list, and the screen rebuilds out of it.
fn the_shard_lists_the_characters(
    c: &mut HeadlessClient,
    peer: &mut Peer,
    pending: Option<(ObjectId, u32)>,
) {
    let blob = dereth_protocol::write_blob(&wire_character_set(pending)).expect("the list encodes");
    peer.send(c, LOGIN_QUEUE, blob);
    c.tick(3);
    assert_eq!(
        with_charmgmt_screen(c, |s| s.rows.len()),
        3,
        "the list on screen was rebuilt out of the one the shard really sent"
    );
}

/// A character screen whose list came **off the wire**, and the shard that sent it.
///
/// The endpoint goes on after the flow has reached the screen: the data-patch screen waits on a
/// connect meter only the real login exchange moves, and this shard deliberately does not run that
/// exchange.
fn a_character_list_from_the_wire(pending: Option<(ObjectId, u32)>) -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::new(ClientSpec::screen(mode::CHARACTER_MANAGEMENT, 4));
    c.app_mut().host_state_mut().world_name = Some("ACEmulator".into());
    let mut peer = Peer::attach(&mut c, WIRE_DOOMED);
    the_shard_lists_the_characters(&mut c, &mut peer, pending);
    (c, peer)
}

/// Every fragment the client has really put on a datagram since the last look, as
/// `(queue, payload)`.
///
/// `HeadlessClient::outbound_wire` is the wrong reader for this family: it reports the sub-types
/// of the **ordered game actions** a client framed, and every message here -- the delete, the
/// restore, the creation -- is a bare control message with no ordered envelope at all.
fn fragments_sent(c: &mut HeadlessClient) -> Vec<(u16, Vec<u8>)> {
    let mut out = Vec::new();
    let taken = c
        .replay_net_mut()
        .expect("this scenario reads the wire, so its client has a shard attached")
        .take_outgoing();
    for (bytes, _) in taken {
        let Ok(packet) = dereth_transport::wire::ParsedPacket::parse(&bytes) else {
            continue;
        };
        for f in packet.fragments {
            out.push((f.header.queue_id, f.payload.clone()));
        }
    }
    out
}

/// The one fragment whose leading word is `op`, if the client sent one.
fn one_message(sent: &[(u16, Vec<u8>)], op: dereth_protocol::Opcode) -> Option<(u16, Vec<u8>)> {
    sent.iter()
        .find(|(_, p)| {
            p.get(..4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                == Some(op.0)
        })
        .cloned()
}

/// The colour a row is really drawn in -- the text element's own current font colour.
fn row_colour(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> u32 {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .map_or(0, |t| t.font_color)
}

/// Pick `who`, press delete, type `typed` into the box and accept it.
fn delete_the_character(c: &mut HeadlessClient, hands: &mut Hands, who: ObjectId, typed: &str) {
    let row = charmgmt_row(c, who);
    hands.click_handle(c, row);
    let delete = element(c, DELETE_BUTTON_ID);
    hands.click_handle(c, delete);
    let h = charmgmt_dialog(c, DialogContext::DeleteCharacter).expect("the question is raised");
    let field = charmgmt_child(c, h, charmgmt::TEXT_INPUT_FIELD);
    hands.click_handle(c, field);
    hands.type_text(c, typed);
    let accept = charmgmt_child(c, h, charmgmt::TEXT_INPUT_ACCEPT);
    hands.click_handle(c, accept);
    // The action leaves the screen in the press's own frame; the pass that puts it on a datagram
    // is the next one.
    c.tick(1);
}

/// Pick `who` and press restore, leaving the waiting box up. Answers whether the screen really
/// offered the button and really raised the box.
fn restore_the_character(c: &mut HeadlessClient, hands: &mut Hands, who: ObjectId) -> bool {
    let row = charmgmt_row(c, who);
    hands.click_handle(c, row);
    let offered = with_charmgmt_screen(c, |s| s.update_buttons().restore);
    let restore = element(c, RESTORE_BUTTON_ID);
    hands.click_handle(c, restore);
    let waiting = charmgmt_dialog(c, DialogContext::PleaseWait).is_some();
    c.tick(1);
    offered && waiting
}

/// The shard's answer to a delete: the bare acknowledgement, which carries no list at all.
fn the_shard_acknowledges_the_delete(c: &mut HeadlessClient, peer: &mut Peer) {
    let blob = dereth_protocol::write_blob(&dereth_protocol::login::CharacterDeleteAck::default())
        .expect("the acknowledgement encodes");
    peer.send(c, LOGIN_QUEUE, blob);
    c.tick(2);
}

/// The shard's answer to a restore: the identity that place in the list now holds.
fn the_shard_restores(c: &mut HeadlessClient, peer: &mut Peer, gid: ObjectId, name: &str) {
    let r = dereth_protocol::login::CharGenVerificationResponse {
        response_type: 1,
        identity: dereth_protocol::login::CharacterIdentity {
            gid,
            name: name.into(),
            seconds_greyed_out: 0,
        },
    };
    let blob = dereth_protocol::write_blob(&r).expect("the answer encodes");
    peer.send(c, LOGIN_QUEUE, blob);
    c.tick(2);
}

/// The shard's refusal of a restore or a creation: the reason and nothing after it, built by hand
/// so that this crate's own writer cannot define the oracle.
fn the_shard_refuses(c: &mut HeadlessClient, peer: &mut Peer, code: u32) {
    let mut blob = Vec::new();
    blob.extend_from_slice(
        &dereth_protocol::Opcode::CHARACTER_CHAR_GEN_VERIFICATION_RESPONSE
            .0
            .to_le_bytes(),
    );
    blob.extend_from_slice(&code.to_le_bytes());
    assert_eq!(
        blob.len(),
        8,
        "a refusal is the opcode and the reason, and nothing else"
    );
    peer.send(c, LOGIN_QUEUE, blob);
    c.tick(2);
}

/// The reason the shard gives when somebody else has taken the name.
const NAME_IN_USE: u32 = 3;

// ---------------------------------------------------------------------------------------------
// shell-only.log-off.the-shards-answer-does-not-end-a-client-that-has-a-screen-to-go-back-to
// ---------------------------------------------------------------------------------------------

/// The defect that made re-entry impossible in the most literal way there is: the process was
/// gone. Both directions, because a client that always keeps running and one that never does read
/// alike from one of them.
pub fn the_shards_log_off_answer_does_not_end_a_client_with_screens() {
    // With a screen to go back to: thirty frames of still running.
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let _shard = Peer::attach(&mut c, ObjectId(0x5000_0001));
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::LoggedOff]);
    let mut still_running = true;
    for _ in 0..30 {
        still_running &= c.app_mut().frame();
    }
    let not_shutting_down =
        c.view().expect_app().state() != dereth_client::app::AppState::ShuttingDown;
    c.shutdown();

    // With none: the same answer ends the run, because there is nothing to return to and nothing
    // to take the player's next press.
    let mut c = HeadlessClient::new(ClientSpec::retail());
    let no_screens = c.view().expect_app().ui().is_none();
    let _shard = Peer::attach(&mut c, ObjectId(0x5000_0001));
    let running_before = c.app_mut().frame();
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::LoggedOff]);
    let ended = !c.app_mut().frame();

    c.assert_behaviour(
        "shell-only.log-off.the-shards-answer-does-not-end-a-client-that-has-a-screen-to-go-back-to",
        move |_| still_running && not_shutting_down && no_screens && running_before && ended,
    );
    c.shutdown();
}

#[test]
fn scenario_the_shards_log_off_answer_does_not_end_a_client_with_screens() {
    scenario("the_shards_log_off_answer_does_not_end_a_client_with_screens");
}

// ---------------------------------------------------------------------------------------------
// shell-only.character-set.every-arrival-is-a-notice-even-when-the-list-has-not-changed
// ---------------------------------------------------------------------------------------------

/// A notice is a count, not a value: the shard re-sends the *same* list after a log-off, and a
/// client comparing the two would see nothing happen at the one moment something did.
pub fn every_character_set_the_session_decodes_is_an_arrival() {
    let mut c = HeadlessClient::new(ClientSpec::screen(mode::CHARACTER_MANAGEMENT, 4));
    let _shard = Peer::attach(&mut c, ObjectId(0x5000_0001));
    let nothing_yet = c.view().expect_app().host_state().character_set_notices == 0;

    let set = wire_character_set(None);
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::CharacterSet(Box::new(set.clone()))]);
    let first = c.view().expect_app().host_state().character_set_notices == 1;

    // The second carries the **same** characters, which is what the shard sends after a log-off.
    let before = c.view().expect_app().host_state().character_set.clone();
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::CharacterSet(Box::new(set))]);
    let unchanged = c.view().expect_app().host_state().character_set == before;
    let second = c.view().expect_app().host_state().character_set_notices == 2;

    c.assert_behaviour(
        "shell-only.character-set.every-arrival-is-a-notice-even-when-the-list-has-not-changed",
        move |_| nothing_yet && first && unchanged && second,
    );
    c.shutdown();
}

#[test]
fn scenario_every_character_set_the_session_decodes_is_an_arrival() {
    scenario("every_character_set_the_session_decodes_is_an_arrival");
}

// ---------------------------------------------------------------------------------------------
// shell-only.character-set.a-list-arriving-in-the-world-sends-the-player-back-to-the-character-screen
// ---------------------------------------------------------------------------------------------

/// Both halves, because either alone is a different defect -- and the negative beside them,
/// because an arm that fired every frame would pass the first two.
pub fn a_list_arriving_in_the_world_sends_the_player_back_and_at_the_list_does_not() {
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::CHARACTER_MANAGEMENT, 30),
    );
    c.tick(2);
    let received = c.expect_shell().flow.data.received_set;

    // At the character screen, a second list rebuilds the rows and moves the flow nowhere.
    c.host_mut().character_set_notices = 2;
    c.tick(4);
    let stays_at_the_list =
        c.expect_shell().flow.current_mode() == Some(mode::CHARACTER_MANAGEMENT);

    // Into the world, and then the same list again -- which no value comparison could see.
    c.expect_shell().queue(mode::GAME_PLAY);
    c.tick(1);
    let in_the_world = c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);
    c.host_mut().character_set_notices = 3;
    c.tick(1);
    let carried_back = c.expect_shell().flow.current_mode() == Some(mode::CHARACTER_MANAGEMENT);
    c.shutdown();

    // And with no arrival at all the player stays in the world, however long the client runs.
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::GAME_PLAY, 30),
    );
    let mut stays_in_the_world = c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);
    for _ in 0..30 {
        c.tick(1);
        stays_in_the_world &= c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);
    }

    c.assert_behaviour(
        "shell-only.character-set.a-list-arriving-in-the-world-sends-the-player-back-to-the-character-screen",
        move |_| {
            received && stays_at_the_list && in_the_world && carried_back && stays_in_the_world
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_list_arriving_in_the_world_sends_the_player_back_and_at_the_list_does_not() {
    scenario("a_list_arriving_in_the_world_sends_the_player_back_and_at_the_list_does_not");
}

// ---------------------------------------------------------------------------------------------
// shell-only.enter-world.the-way-into-the-world-opens-again-after-a-log-off
// ---------------------------------------------------------------------------------------------

/// In, out, in. The middle leg is the whole scenario: the step into the world is taken on the
/// **rising** edge of being in it, so a log-off that never lowered that edge would leave the player
/// on the character screen with a world running behind it for ever.
pub fn the_way_into_the_world_opens_again_after_a_log_off() {
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::CHARACTER_MANAGEMENT, 30),
    );
    c.tick(2);

    c.host_mut().in_world = true;
    c.tick(1);
    let first_entry = c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);

    c.host_mut().in_world = false;
    c.host_mut().character_set_notices = 2;
    c.tick(1);
    let back_at_the_list = c.expect_shell().flow.current_mode() == Some(mode::CHARACTER_MANAGEMENT);

    c.host_mut().in_world = true;
    c.tick(1);
    let second_entry = c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);

    c.assert_behaviour(
        "shell-only.enter-world.the-way-into-the-world-opens-again-after-a-log-off",
        move |_| first_entry && back_at_the_list && second_entry,
    );
    c.shutdown();
}

#[test]
fn scenario_the_way_into_the_world_opens_again_after_a_log_off() {
    scenario("the_way_into_the_world_opens_again_after_a_log_off");
}

// ---------------------------------------------------------------------------------------------
// shell-only.log-off.answering-yes-asks-to-log-off-and-moves-no-screen-of-its-own
// ---------------------------------------------------------------------------------------------

/// Press an element of the bare shell by id, through the arena rather than by walking a root: a
/// dialog's buttons are children of the dialog the screen raised and not of the screen itself.
fn shell_click(c: &mut HeadlessClient, id: ElementId) {
    let shell = c.expect_shell();
    let h = shell
        .ui
        .get_element(id)
        .unwrap_or_else(|| panic!("{:#X} is in the shipped layout", id.0));
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
}

/// *Exit to Character Selection*, then *Yes*: the host is asked to log off and nothing on screen
/// moves in that frame, which is the window the leaving is played out in.
pub fn answering_yes_asks_to_log_off_and_moves_no_screen_of_its_own() {
    use dereth_ui_screens::screens::gameplay::{logout, GamePlayScreen};

    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::GAME_PLAY, 30),
    );
    // Anything the shell raised on the way here is taken now, so every later read is about this
    // scenario's own gesture.
    let _ = c.expect_shell().take_log_off();

    shell_click(&mut c, logout::EXIT_TO_CHARACTER_SELECTION);
    c.tick(1);
    let asked_first = {
        let shell = c.expect_shell();
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen is up")
            .logout_dialog()
            .is_some()
    };

    shell_click(&mut c, logout::BUTTON_YES);
    c.tick(1);
    let no_screen_moved = c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);
    let the_host_was_asked = c.expect_shell().take_log_off();

    c.assert_behaviour(
        "shell-only.log-off.answering-yes-asks-to-log-off-and-moves-no-screen-of-its-own",
        move |_| asked_first && no_screen_moved && the_host_was_asked,
    );
    c.shutdown();
}

#[test]
fn scenario_answering_yes_asks_to_log_off_and_moves_no_screen_of_its_own() {
    scenario("answering_yes_asks_to_log_off_and_moves_no_screen_of_its_own");
}

// ---------------------------------------------------------------------------------------------
// shell-only.character-select.a-pick-the-returning-list-cannot-honour-falls-back-to-the-shards-own-order
// ---------------------------------------------------------------------------------------------

/// The other direction of the remembered pick, without which a client that simply repeats the
/// *last* thing it was told reads the same as one that remembers a choice.
pub fn a_pick_the_returning_list_cannot_honour_falls_back_to_the_shards_own_order() {
    // Nothing was ever picked: out and back, and the fallback still decides.
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::CHARACTER_MANAGEMENT, 30),
    );
    c.tick(2);
    let opens_on_the_fallback = selected_name(&mut c).as_deref() == Some("Zoranth");
    c.host_mut().in_world = true;
    c.tick(2);
    c.host_mut().in_world = false;
    c.host_mut().character_set_notices = 2;
    c.tick(4);
    let back_at_the_list = c.expect_shell().flow.current_mode() == Some(mode::CHARACTER_MANAGEMENT);
    let nothing_remembered = selected_name(&mut c).as_deref() == Some("Zoranth");
    c.shutdown();

    // And a pick the next list has lost: driven as the deletion it would be.
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::CHARACTER_MANAGEMENT, 30),
    );
    c.tick(2);
    click_the_row(&mut c, "Borumar");
    let picked = selected_name(&mut c).as_deref() == Some("Borumar");
    let mut shorter = a_character_set();
    shorter.set.retain(|ch| ch.name != "Borumar");
    c.host_mut().character_set = Some(shorter);
    c.host_mut().character_set_notices = 2;
    c.tick(3);
    let fell_back = selected_name(&mut c).as_deref() == Some("Zoranth");

    c.assert_behaviour(
        "shell-only.character-select.a-pick-the-returning-list-cannot-honour-falls-back-to-the-shards-own-order",
        move |_| {
            opens_on_the_fallback && back_at_the_list && nothing_remembered && picked && fell_back
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_pick_the_returning_list_cannot_honour_falls_back_to_the_shards_own_order() {
    scenario("a_pick_the_returning_list_cannot_honour_falls_back_to_the_shards_own_order");
}

// ---------------------------------------------------------------------------------------------
// character-select.delete.the-request-that-leaves-the-client-carries-the-account-and-the-shards-own-place
// ---------------------------------------------------------------------------------------------

/// The whole message, byte for byte, and the one thing a sorted list box can get wrong: **which
/// place**. `WIRE_DOOMED` is the shard's first character and the screen's third row, so a client
/// that sent the row would be asking the shard to destroy somebody else.
pub fn the_delete_that_leaves_the_client_carries_the_account_and_the_shards_own_place() {
    let (mut c, _peer) = a_character_list_from_the_wire(None);
    let mut hands = Hands::new();
    let phrase = charmgmt_word(&c, charmgmt::DELETE_RESPONSE_STRING);

    // The premise, asserted rather than assumed: the two numbers really are different here.
    let row_index =
        with_charmgmt_screen(&mut c, |s| s.rows.iter().position(|r| r.id == WIRE_DOOMED));
    let set_index = with_charmgmt_screen(&mut c, |s| {
        s.char_set.set.iter().position(|ch| ch.id == WIRE_DOOMED)
    });
    let they_differ = row_index == Some(2) && set_index == Some(0);

    let _ = fragments_sent(&mut c);
    delete_the_character(&mut c, &mut hands, WIRE_DOOMED, &phrase);

    let out = fragments_sent(&mut c);
    let sent = one_message(&out, dereth_protocol::Opcode::CHARACTER_CHARACTER_DELETE);
    let mut want = dereth_protocol::Opcode::CHARACTER_CHARACTER_DELETE
        .0
        .to_le_bytes()
        .to_vec();
    want.extend(
        dereth_protocol::write_body(&dereth_protocol::login::CharacterDeleteRequest {
            account: WIRE_ACCOUNT.to_owned(),
            slot_index: 0,
        })
        .expect("the request encodes"),
    );
    let on_the_log_on_queue = sent.as_ref().map(|(q, _)| *q) == Some(4);
    let byte_for_byte = sent.as_ref().map(|(_, p)| p.clone()) == Some(want);

    // Read back the way the shard reads it, which is the only reading that matters.
    let read_back = sent.as_ref().and_then(|(_, p)| {
        dereth_protocol::read_body::<dereth_protocol::login::CharacterDeleteRequest>(&p[4..]).ok()
    });
    let names_the_account_and_the_place = read_back
        .as_ref()
        .is_some_and(|r| r.account == WIRE_ACCOUNT && r.slot_index == 0);

    // ...and the waiting box is up by the time the request exists.
    let waiting = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_some();

    c.assert_behaviour(
        "character-select.delete.the-request-that-leaves-the-client-carries-the-account-and-the-shards-own-place",
        move |_| {
            they_differ
                && on_the_log_on_queue
                && byte_for_byte
                && names_the_account_and_the_place
                && waiting
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_delete_that_leaves_the_client_carries_the_account_and_the_shards_own_place() {
    scenario("the_delete_that_leaves_the_client_carries_the_account_and_the_shards_own_place");
}

// ---------------------------------------------------------------------------------------------
// character-select.delete.a-pending-deletion-is-drawn-red-and-last-and-offers-restore
// ---------------------------------------------------------------------------------------------

/// What the player sees once the shard has answered: the acknowledgement alone changes nothing,
/// the fresh list is what takes the waiting box down, and the row is then red, last, and offers
/// to be restored rather than played.
///
/// It is also where the premise *"a pending row says how long it has left"* is falsified: the row
/// says the name and nothing else.
pub fn a_pending_deletion_is_drawn_red_and_last_and_offers_restore() {
    let (mut c, mut peer) = a_character_list_from_the_wire(None);
    let mut hands = Hands::new();
    let phrase = charmgmt_word(&c, charmgmt::DELETE_RESPONSE_STRING);
    delete_the_character(&mut c, &mut hands, WIRE_DOOMED, &phrase);

    // The acknowledgement carries no list, so the box the player cannot dismiss is still up.
    the_shard_acknowledges_the_delete(&mut c, &mut peer);
    let still_waiting = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_some();

    // The fresh list is what takes it down.
    the_shard_lists_the_characters(&mut c, &mut peer, Some((WIRE_DOOMED, 3_600)));
    let taken_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();

    let rows = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let kept_its_place_in_the_list = rows.len() == 3;
    let last_and_alone = rows[2].id == WIRE_DOOMED
        && rows[2].greyed_out
        && !rows[0].greyed_out
        && !rows[1].greyed_out;

    let doomed_h = rows[2].element.expect("the row has an element");
    let live_h = rows[0].element.expect("the row has an element");
    let drawn_red = row_colour(&mut c, doomed_h) == charmgmt::GREYED_OUT_COLOR
        && row_colour(&mut c, live_h) != charmgmt::GREYED_OUT_COLOR;
    let says_only_the_name = wizard_text(&mut c, doomed_h) == "Zeddish";

    let row = charmgmt_row(&mut c, WIRE_DOOMED);
    hands.click_handle(&mut c, row);
    let buttons = with_charmgmt_screen(&mut c, |s| s.update_buttons());
    let offers_restore =
        buttons.restore && !buttons.delete && !buttons.enter_game && buttons.create;

    c.assert_behaviour(
        "character-select.delete.a-pending-deletion-is-drawn-red-and-last-and-offers-restore",
        move |_| {
            still_waiting
                && taken_down
                && kept_its_place_in_the_list
                && last_and_alone
                && drawn_red
                && says_only_the_name
                && offers_restore
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_pending_deletion_is_drawn_red_and_last_and_offers_restore() {
    scenario("a_pending_deletion_is_drawn_red_and_last_and_offers_restore");
}

// ---------------------------------------------------------------------------------------------
// character-select.restore.the-request-that-leaves-the-client-is-the-characters-own-id-on-the-control-queue
// ---------------------------------------------------------------------------------------------

/// The restore's whole message: the id and two empty names, on the control queue. A place in the
/// list on this wire would find the shard nothing at all, and silently.
pub fn the_restore_that_leaves_the_client_is_the_characters_own_id_on_the_control_queue() {
    let (mut c, _peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
    let mut hands = Hands::new();

    let _ = fragments_sent(&mut c);
    let asked = restore_the_character(&mut c, &mut hands, WIRE_LAPSED);

    let out = fragments_sent(&mut c);
    let sent = one_message(
        &out,
        dereth_protocol::Opcode::ADMIN_SEND_ADMIN_RESTORE_CHARACTER,
    );
    let mut want = dereth_protocol::Opcode::ADMIN_SEND_ADMIN_RESTORE_CHARACTER
        .0
        .to_le_bytes()
        .to_vec();
    want.extend(
        dereth_protocol::write_body(&dereth_protocol::admin::AdminSendAdminRestoreCharacter {
            iid: WIRE_LAPSED,
            restored_char_name: String::new(),
            account_to_restore_to: String::new(),
        })
        .expect("the request encodes"),
    );
    let on_the_control_queue = sent.as_ref().map(|(q, _)| *q) == Some(2);
    let byte_for_byte = sent.as_ref().map(|(_, p)| p.clone()) == Some(want);

    let read_back = sent.as_ref().and_then(|(_, p)| {
        dereth_protocol::read_body::<dereth_protocol::admin::AdminSendAdminRestoreCharacter>(
            &p[4..],
        )
        .ok()
    });
    let names_the_character = read_back.as_ref().is_some_and(|r| {
        r.iid == WIRE_LAPSED
            && r.restored_char_name.is_empty()
            && r.account_to_restore_to.is_empty()
    });
    // ...and it is the id and not the place: the place is 2 here, and the id is not.
    let not_a_place = read_back.as_ref().is_some_and(|r| r.iid.0 != 2);

    c.assert_behaviour(
        "character-select.restore.the-request-that-leaves-the-client-is-the-characters-own-id-on-the-control-queue",
        move |_| asked && on_the_control_queue && byte_for_byte && names_the_character && not_a_place,
    );
    c.shutdown();
}

#[test]
fn scenario_the_restore_that_leaves_the_client_is_the_characters_own_id_on_the_control_queue() {
    scenario("the_restore_that_leaves_the_client_is_the_characters_own_id_on_the_control_queue");
}

// ---------------------------------------------------------------------------------------------
// character-select.restore.a-restored-character-comes-back-in-its-own-place-and-in-the-ordinary-colour
// ---------------------------------------------------------------------------------------------

/// The check a player makes, drawn: the waiting box goes, the list is no longer, the row is back in
/// the middle where its name sorts, in the colour every other row is drawn in, and offers to be
/// deleted or played.
pub fn a_restored_character_comes_back_in_its_own_place_and_in_the_ordinary_colour() {
    let (mut c, mut peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
    let mut hands = Hands::new();

    // Pending: last, and red. Asserted, so that the ending is a measurement.
    let before = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let pending_h = before[2].element.expect("the row has an element");
    let started_last_and_red =
        before[2].id == WIRE_LAPSED && row_colour(&mut c, pending_h) == charmgmt::GREYED_OUT_COLOR;

    restore_the_character(&mut c, &mut hands, WIRE_LAPSED);
    the_shard_restores(&mut c, &mut peer, WIRE_LAPSED, "Tarinell");

    let taken_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none()
        && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_none();
    let after = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let replaced_rather_than_added = after.len() == 3;
    let back_in_its_own_place = after.iter().map(|r| r.name.clone()).collect::<Vec<_>>()
        == ["Larktest", "Tarinell", "Zeddish"]
        && after.iter().all(|r| !r.greyed_out);

    let restored_h = after[1].element.expect("the row has an element");
    let live_h = after[0].element.expect("the row has an element");
    let drawn_like_the_others = row_colour(&mut c, restored_h) != charmgmt::GREYED_OUT_COLOR
        && row_colour(&mut c, restored_h) == row_colour(&mut c, live_h);

    let row = charmgmt_row(&mut c, WIRE_LAPSED);
    hands.click_handle(&mut c, row);
    let buttons = with_charmgmt_screen(&mut c, |s| s.update_buttons());
    let playable_again = buttons.delete && !buttons.restore && buttons.enter_game;

    c.assert_behaviour(
        "character-select.restore.a-restored-character-comes-back-in-its-own-place-and-in-the-ordinary-colour",
        move |_| {
            started_last_and_red
                && taken_down
                && replaced_rather_than_added
                && back_in_its_own_place
                && drawn_like_the_others
                && playable_again
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_restored_character_comes_back_in_its_own_place_and_in_the_ordinary_colour() {
    scenario("a_restored_character_comes_back_in_its_own_place_and_in_the_ordinary_colour");
}

// ---------------------------------------------------------------------------------------------
// character-select.restore.the-answer-overwrites-the-place-it-was-asked-about-and-never-appends
// ---------------------------------------------------------------------------------------------

/// The answer is addressed by the **place** the client asked about, and whatever identity it
/// carries is written over that place -- name included. That is not a quirk to design around: it
/// is how a character brought back under another name reaches the list at all.
pub fn the_restore_answer_overwrites_the_place_it_was_asked_about_and_never_appends() {
    let (mut c, mut peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
    let mut hands = Hands::new();

    restore_the_character(&mut c, &mut hands, WIRE_LAPSED);
    // A different name *and* a different id, which is the sharpest form of the same rule.
    the_shard_restores(&mut c, &mut peer, ObjectId(0x5000_00FF), "Somebodyelse");

    let rows = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let nothing_appended = rows.len() == 3;
    let the_place_was_overwritten = rows.iter().all(|r| r.id != WIRE_LAPSED);
    let carries_what_the_answer_said = rows
        .iter()
        .find(|r| r.name == "Somebodyelse")
        .is_some_and(|r| !r.greyed_out);
    let sorted_like_any_other_row = rows.iter().map(|r| r.name.clone()).collect::<Vec<_>>()
        == ["Larktest", "Somebodyelse", "Zeddish"];

    c.assert_behaviour(
        "character-select.restore.the-answer-overwrites-the-place-it-was-asked-about-and-never-appends",
        move |_| {
            nothing_appended
                && the_place_was_overwritten
                && carries_what_the_answer_said
                && sorted_like_any_other_row
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_restore_answer_overwrites_the_place_it_was_asked_about_and_never_appends() {
    scenario("the_restore_answer_overwrites_the_place_it_was_asked_about_and_never_appends");
}

// ---------------------------------------------------------------------------------------------
// character-select.restore.a-refusal-takes-the-waiting-box-down-and-says-why-in-the-shipped-words
// ---------------------------------------------------------------------------------------------

/// The half that leaves a player stuck when it is missing: the shard answers a restore with a
/// refusal and **no list at all**, so the box the player cannot dismiss has only this one way
/// down. A refusal shows why; a success shows nothing; and a second refusal carrying the same
/// reason as the first still takes the second box down.
pub fn a_refused_restore_takes_the_waiting_box_down_and_says_why() {
    let want = {
        let (c, _peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
        let s = charmgmt_word(&c, charmgmt::CHARGEN_VERIFICATION_STRINGS[0]);
        c.shutdown();
        s
    };

    let (mut c, mut peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
    let mut hands = Hands::new();

    restore_the_character(&mut c, &mut hands, WIRE_LAPSED);
    the_shard_refuses(&mut c, &mut peer, NAME_IN_USE);

    let taken_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();
    let told_why = charmgmt_dialog(&mut c, DialogContext::ErrorMessage).is_some()
        && with_charmgmt_screen(&mut c, |s| s.error_text.clone()).as_deref() == Some(want.as_str());
    // A refusal writes nothing: the character is still waiting to be deleted.
    let still_pending = with_charmgmt_screen(&mut c, |s| {
        s.rows
            .iter()
            .find(|r| r.id == WIRE_LAPSED)
            .is_some_and(|r| r.greyed_out)
    });

    // The same reason a second time still takes the second box down -- a notice is a count.
    let h = charmgmt_dialog(&mut c, DialogContext::ErrorMessage).expect("the message box");
    let button = charmgmt_child(&c, h, charmgmt::MESSAGE_BUTTON);
    hands.click_handle(&mut c, button);
    c.tick(1);
    restore_the_character(&mut c, &mut hands, WIRE_LAPSED);
    the_shard_refuses(&mut c, &mut peer, NAME_IN_USE);
    let second_taken_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();
    c.shutdown();

    // And a restore that works shows no message at all, on a client of its own.
    let (mut c, mut peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
    let mut hands = Hands::new();
    restore_the_character(&mut c, &mut hands, WIRE_LAPSED);
    the_shard_restores(&mut c, &mut peer, WIRE_LAPSED, "Tarinell");
    let success_says_nothing = charmgmt_dialog(&mut c, DialogContext::ErrorMessage).is_none()
        && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_none();

    c.assert_behaviour(
        "character-select.restore.a-refusal-takes-the-waiting-box-down-and-says-why-in-the-shipped-words",
        move |_| {
            taken_down && told_why && still_pending && second_taken_down && success_says_nothing
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_refused_restore_takes_the_waiting_box_down_and_says_why() {
    scenario("a_refused_restore_takes_the_waiting_box_down_and_says_why");
}

// ---------------------------------------------------------------------------------------------
// character-select.round-trip.deleting-and-restoring-over-the-wire-leaves-the-list-where-it-started
// ---------------------------------------------------------------------------------------------

/// Delete, then restore, both over real datagrams. The asymmetry between the two answers is the
/// whole story: a delete is followed by a fresh list and a restore is followed by nothing else,
/// so one of the two waiting boxes has to be taken down by the answer itself.
pub fn deleting_and_restoring_over_the_wire_leaves_the_list_where_it_started() {
    let (mut c, mut peer) = a_character_list_from_the_wire(None);
    let mut hands = Hands::new();
    let phrase = charmgmt_word(&c, charmgmt::DELETE_RESPONSE_STRING);

    let started_live = with_charmgmt_screen(&mut c, |s| s.rows.iter().all(|r| !r.greyed_out));

    let _ = fragments_sent(&mut c);
    delete_the_character(&mut c, &mut hands, WIRE_DOOMED, &phrase);
    let asked_to_delete = one_message(
        &fragments_sent(&mut c),
        dereth_protocol::Opcode::CHARACTER_CHARACTER_DELETE,
    )
    .is_some();
    let waiting_on_the_delete = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_some();

    the_shard_acknowledges_the_delete(&mut c, &mut peer);
    the_shard_lists_the_characters(&mut c, &mut peer, Some((WIRE_DOOMED, 3_600)));
    let the_list_took_that_box_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none()
        && with_charmgmt_screen(&mut c, |s| {
            s.rows
                .iter()
                .find(|r| r.id == WIRE_DOOMED)
                .is_some_and(|r| r.greyed_out)
        });

    let _ = fragments_sent(&mut c);
    restore_the_character(&mut c, &mut hands, WIRE_DOOMED);
    let asked_to_restore = one_message(
        &fragments_sent(&mut c),
        dereth_protocol::Opcode::ADMIN_SEND_ADMIN_RESTORE_CHARACTER,
    )
    .is_some();

    // No list follows a restore, so this answer is the only thing that can take its box down.
    the_shard_restores(&mut c, &mut peer, WIRE_DOOMED, "Zeddish");
    let the_answer_took_this_one_down =
        charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();

    let rows = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let row = charmgmt_row(&mut c, WIRE_DOOMED);
    hands.click_handle(&mut c, row);
    let where_it_started = rows.len() == 3
        && rows.iter().all(|r| !r.greyed_out)
        && with_charmgmt_screen(&mut c, |s| s.update_buttons().delete);

    c.assert_behaviour(
        "character-select.round-trip.deleting-and-restoring-over-the-wire-leaves-the-list-where-it-started",
        move |_| {
            started_live
                && asked_to_delete
                && waiting_on_the_delete
                && the_list_took_that_box_down
                && asked_to_restore
                && the_answer_took_this_one_down
                && where_it_started
        },
    );
    c.shutdown();
}

#[test]
fn scenario_deleting_and_restoring_over_the_wire_leaves_the_list_where_it_started() {
    scenario("deleting_and_restoring_over_the_wire_leaves_the_list_where_it_started");
}

// ---------------------------------------------------------------------------------------------
// chargen.finish.a-refused-creation-stops-the-wizard-waiting-and-says-why
// ---------------------------------------------------------------------------------------------

/// The check a player makes: Finish with a name somebody already has. The request goes out, the
/// wizard waits, the shard's refusal comes back and the wizard says which refusal it was, with a
/// button that closes it -- and the same is true of the reason that shares the client's general
/// arm, which must not leave nothing on the screen at all.
pub fn a_refused_creation_stops_the_wizard_waiting_and_says_why() {
    use dereth_chargen::CgVerification;
    use dereth_ui_screens::screens::chargen::{CharGenDialog, MESSAGE_BUTTON};

    // The name already taken.
    let mut c = a_client_on_the_wizard();
    let mut peer = Peer::attach(&mut c, ObjectId(0x5000_0001));
    walk_the_wizard(&mut c);
    let _ = fragments_sent(&mut c);
    click_wizard(&mut c, chargen::FINISH_BUTTON);
    c.tick(2);

    let asked = one_message(
        &fragments_sent(&mut c),
        dereth_protocol::Opcode::CHARACTER_SEND_CHAR_GEN_RESULT,
    )
    .is_some_and(|(q, _)| q == 4);
    let waiting = with_wizard(&mut c, |_, w| w.state.verification) == CgVerification::Pending;

    the_shard_refuses(&mut c, &mut peer, NAME_IN_USE);

    let h = wizard_dialog(&mut c, CharGenDialog::ErrorMessage).expect("the player is told why");
    let want = shipped_word(&c, "ID_Character_Err_NameReserved");
    let other = shipped_word(
        &c,
        "ID_CharacterManagement_CG_VERIFICATION_RESPONSE_NAME_IN_USE",
    );
    let body = dialog_child(&mut c, h, dereth_ui::dialog::base::child::TEXT);
    let drawn = wizard_text(&mut c, body);
    let says_the_wizards_own_sentence = drawn == want && drawn != other;
    // ...and the latch clears, so the player can fix the name and finish again.
    let ready_again = with_wizard(&mut c, |_, w| w.state.verification) == CgVerification::Undef;
    c.shutdown();

    // The reason that takes the client's own general arm, which must not answer with nothing.
    let mut c = a_client_on_the_wizard();
    let mut peer = Peer::attach(&mut c, ObjectId(0x5000_0001));
    walk_the_wizard(&mut c);
    click_wizard(&mut c, chargen::FINISH_BUTTON);
    c.tick(2);
    the_shard_refuses(&mut c, &mut peer, 2);

    let h = wizard_dialog(&mut c, CharGenDialog::ErrorMessage)
        .expect("the general arm builds a box like every other failure");
    let want = shipped_word(&c, "ID_Character_Err_NameDBDown");
    let body = dialog_child(&mut c, h, dereth_ui::dialog::base::child::TEXT);
    let the_general_arm_says_so = wizard_text(&mut c, body) == want;

    // And the one button closes it, leaving the wizard usable.
    click_in_dialog(&mut c, h, MESSAGE_BUTTON);
    c.tick(2);
    let closed = wizard_dialog(&mut c, CharGenDialog::ErrorMessage).is_none()
        && with_wizard(&mut c, |_, w| w.open_dialog).is_none();

    c.assert_behaviour(
        "chargen.finish.a-refused-creation-stops-the-wizard-waiting-and-says-why",
        move |_| {
            asked
                && waiting
                && says_the_wizards_own_sentence
                && ready_again
                && the_general_arm_says_so
                && closed
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_refused_creation_stops_the_wizard_waiting_and_says_why() {
    scenario("a_refused_creation_stops_the_wizard_waiting_and_says_why");
}

// ---------------------------------------------------------------------------------------------
// chargen.finish.every-refusal-the-shard-can-send-draws-its-own-sentence
// ---------------------------------------------------------------------------------------------

/// Every reason the shard can answer a creation with, and the sentence the wizard's own arm for it
/// names -- including two that could be answered with a token nobody had written and two that could
/// be answered with silence.
pub fn every_refusal_the_shard_can_send_draws_its_own_sentence() {
    use dereth_chargen::CgVerification;

    let table: [(u32, &str); 7] = [
        (0, "ID_Character_Err_NameDBDown"),
        (2, "ID_Character_Err_NameDBDown"),
        (3, "ID_Character_Err_NameReserved"),
        (4, "ID_Character_Err_NameBanned"),
        (5, "ID_Character_Err_NameDBDown"),
        (6, "ID_Character_Err_NameDBDown"),
        (7, "ID_Character_Err_NameAdminDenied"),
    ];
    let every_arm_names_its_own = table
        .iter()
        .all(|(code, token)| CgVerification::from_code(*code).error_string_id() == Some(*token));
    // The one answer that is not a refusal names no sentence at all.
    let a_yes_says_nothing = CgVerification::Ok.error_string_id().is_none();

    // And the four are real: four different sentences the shipped table can draw, none of them a
    // bare token.
    let mut c = a_client_on_the_wizard();
    let mut seen: Vec<String> = Vec::new();
    let mut four_different_sentences = true;
    for token in [
        "ID_Character_Err_NameDBDown",
        "ID_Character_Err_NameAdminDenied",
        "ID_Character_Err_NameBanned",
        "ID_Character_Err_NameReserved",
    ] {
        let s = shipped_word(&c, token);
        four_different_sentences &= !s.is_empty() && s != token && !seen.contains(&s);
        seen.push(s);
    }

    c.assert_behaviour(
        "chargen.finish.every-refusal-the-shard-can-send-draws-its-own-sentence",
        move |_| every_arm_names_its_own && a_yes_says_nothing && four_different_sentences,
    );
    c.shutdown();
}

#[test]
fn scenario_every_refusal_the_shard_can_send_draws_its_own_sentence() {
    scenario("every_refusal_the_shard_can_send_draws_its_own_sentence");
}

// =============================================================================================
// What survives a relog
//
// This is not a shell-only scenario at all: it is a whole client with a shard on the other end of a
// socket-free endpoint, and the shard is the point. [`RelogShard`] is a reference server's
// persistence and nothing more -- it holds the settings it would store for this character and
// applies the four handlers a reference server has for the messages this client sends -- and it
// learns **only what crossed the wire**, because [`relog_drain_wire`] reads the client's own
// outgoing datagrams and parses them.
//
// That is the whole argument: a scenario that re-read the client's own memory after a log-off
// would be green on a client that never told anybody anything.
// =============================================================================================

use dereth_client_model::player::options::option as relog_option;
use dereth_ui_screens::view::{DropTarget, PlayerOption, UiRequest};

/// The character this scenario logs on as.
const RELOG_LARK: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_0001);

/// The item the shortcut gesture drops. The toolbar refuses a zero id and nothing else, so an id
/// with no object behind it still makes the shortcut -- which is the point: this is about what the
/// shortcut list does across a relog and not about what is in the pack.
const RELOG_ITEM: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x8000_0111);

/// The setting whose ordinal is not one of the named ones, taken from the table by index with the
/// table's own name asserted beside it -- a bare number with nothing checking it is exactly the
/// kind that goes stale.
const HEAR_GENERAL_CHAT: usize = 35;

/// The main chat window, which is the one the filter and placement gestures address.
const RELOG_WINDOW: u32 = 1;

/// A setting the client keeps to itself until the way out...
const RELOG_DEFERRED: PlayerOption = PlayerOption::ShowTooltips;
/// ...and one it saves the moment it is ticked, so both directions are asserted.
const RELOG_AUTO_SAVED: PlayerOption = PlayerOption::HearGeneralChat;

/// A reference server's persistence for this character, and the four handlers it has for the
/// messages this client sends. Nothing here reads the client's memory.
struct RelogShard {
    module: dereth_protocol::login::PlayerModule,
    /// Every message the client put on the wire, for the record.
    heard: Vec<(dereth_protocol::Opcode, Vec<u8>)>,
}

impl RelogShard {
    /// A fresh character: the settings a reference server builds for somebody who has never
    /// changed anything.
    fn new() -> Self {
        Self {
            module: dereth_protocol::login::PlayerModule {
                spell_bars: vec![Vec::new()],
                spell_filters: dereth_protocol::login::PlayerModule::DEFAULT_SPELL_FILTERS,
                options: dereth_client_model::player::options::DEFAULT_OPTIONS,
                options2: dereth_client_model::player::options::DEFAULT_OPTIONS2,
                ..dereth_protocol::login::PlayerModule::default()
            },
            heard: Vec::new(),
        }
    }

    fn opcodes(&self) -> Vec<u32> {
        self.heard.iter().map(|(o, _)| o.0).collect()
    }

    /// One inbound message. Anything the shard has no handler for is recorded and ignored, which
    /// is what a shard with no handler for it does.
    fn apply(&mut self, opcode: dereth_protocol::Opcode, body: &[u8]) {
        use dereth_protocol::Message as _;
        self.heard.push((opcode, body.to_vec()));
        let mut r = dereth_protocol::archive::Reader::new(body);
        match opcode.0 {
            // One setting, persisted on its own.
            0x0005 => {
                if let Ok(m) =
                    dereth_protocol::login::CharacterPlayerOptionChangedEvent::read(&mut r)
                {
                    let ordinal = usize::try_from(m.option).unwrap_or(usize::MAX);
                    let mut o = dereth_client_model::player::options::Options {
                        options: self.module.options,
                        options2: self.module.options2,
                    };
                    o.set(ordinal, m.value != 0);
                    self.module.options = o.options;
                    self.module.options2 = o.options2;
                }
            }
            // A shortcut added, and one removed.
            0x019C => {
                if let Ok(m) = dereth_protocol::login::CharacterAddShortCut::read(&mut r) {
                    let list = self.module.shortcuts.get_or_insert_with(Vec::new);
                    list.retain(|s| s.index != m.shortcut.index);
                    list.push(m.shortcut);
                    list.sort_by_key(|s| s.index);
                    self.module.option_flags |=
                        dereth_protocol::login::player_module_flags::SHORTCUT;
                }
            }
            0x019D => {
                if let Ok(m) = dereth_protocol::login::CharacterRemoveShortCut::read(&mut r) {
                    if let Some(list) = self.module.shortcuts.as_mut() {
                        list.retain(|s| s.index != i32::try_from(m.index).unwrap_or(-1));
                    }
                }
            }
            // The whole settings block, replacing the stored one. This is the only route the
            // deferred half of the state has.
            0x01A1 => {
                if let Ok(m) = dereth_protocol::login::CharacterCharacterOptionsEvent::read(&mut r)
                {
                    self.module = m.module;
                }
            }
            _ => {}
        }
    }

    /// The description a second login receives, built out of nothing but what the shard was told.
    fn player_description(&self) -> dereth_protocol::login::LoginPlayerDescription {
        use dereth_protocol::login::player_module_flags as f;
        let mut m = self.module.clone();
        // The header and the fields have to agree or the block does not encode; the shard rebuilds
        // the header from what it holds, as its own writer does.
        let mut flags = f::SPELLBOOK_FILTERS | f::CHARACTER_OPTIONS_2;
        if m.shortcuts.as_ref().is_some_and(|s| !s.is_empty()) {
            flags |= f::SHORTCUT;
        } else {
            m.shortcuts = None;
        }
        if m.desired_comps.is_some() {
            flags |= f::DESIRED_COMPS;
        }
        if m.generic_qualities.is_some() {
            flags |= f::GENERIC_QUALITIES_DATA;
        }
        if m.gameplay_options
            .as_ref()
            .is_some_and(|o| !o.properties.entries.is_empty())
        {
            flags |= f::GAMEPLAY_OPTIONS;
        } else {
            m.gameplay_options = None;
        }
        if m.spell_bars.len() >= 8 {
            m.spell_bars.resize(8, Vec::new());
            flags |= f::SPELL_LISTS_8;
        } else {
            m.spell_bars.resize(1, Vec::new());
        }
        m.timestamp_format = None;
        m.option_flags = flags;
        dereth_protocol::login::LoginPlayerDescription {
            player_module: m,
            ..dereth_protocol::login::LoginPlayerDescription::default()
        }
    }
}

/// Every message the client has put on the wire since the last drain, read off its **own outgoing
/// datagrams** and handed to the shard.
///
/// A payload that begins with the ordered-action envelope is unpacked into its sub-type and body;
/// anything else is a control message whose first word is its own.
fn relog_drain_wire(c: &mut HeadlessClient, shard: &mut RelogShard) {
    let out = c
        .replay_net_mut()
        .expect("the endpoint is attached")
        .take_outgoing();
    for (bytes, _) in out {
        let Ok(packet) = dereth_transport::wire::ParsedPacket::parse(&bytes) else {
            continue;
        };
        for frag in &packet.fragments {
            let payload = &frag.payload;
            let Some(first) = payload
                .get(..4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            else {
                continue;
            };
            if first == 0xF7B1 {
                if let Ok(a) = dereth_protocol::actions::unpack_action(payload) {
                    let consumed = payload.len() - a.body.remaining();
                    shard.apply(a.sub_type, &payload[consumed..]);
                }
            } else {
                shard.apply(dereth_protocol::Opcode(first), &payload[4..]);
            }
        }
    }
}

/// Bring a client up on a socket-free link, land it in the world, and give it the settings block
/// the shard holds. The login is driven **through the transport** rather than by handing the
/// client decoded events, because the log-off below is state-gated: a session that never reached
/// the world sends no departure at all and the scenario would be measuring a log-off that did not
/// happen.
fn a_relog_client(shard: &RelogShard) -> (HeadlessClient, dereth_testkit::replay::Peer) {
    let mut c =
        HeadlessClient::new(ClientSpec::gameplay_in_world(4).with_scratch_settings("relog"));
    let mut peer = dereth_testkit::replay::Peer::attach(&mut c, RELOG_LARK);

    // The shard names itself, then offers the character list. The world name is load-bearing for
    // the notebook: the client composes its per-character file path out of the settings directory,
    // the world name and the character.
    peer.send(
        &mut c,
        9,
        dereth_protocol::write_blob(&dereth_protocol::login::LoginWorldInfo {
            connections: 1,
            max_connections: 100,
            world_name: "relog-world".into(),
        })
        .expect("the world name encodes"),
    );
    peer.send(
        &mut c,
        9,
        dereth_protocol::write_blob(&relog_character_set()).expect("the list encodes"),
    );
    c.tick(1);

    // The player picks the character, which is the two-step exchange the session layer owns.
    c.replay_net_mut()
        .expect("the endpoint")
        .session
        .enter_world(RELOG_LARK, "relog");
    c.tick(1);
    peer.send(
        &mut c,
        9,
        dereth_protocol::write_blob(&dereth_protocol::login::LoginEnterGameServerReady)
            .expect("the ready message encodes"),
    );
    c.tick(1);

    // The description, which is the last step and what puts the client in the world.
    peer.send(
        &mut c,
        9,
        dereth_protocol::write_blob(&shard.player_description()).expect("the description encodes"),
    );
    c.tick(1);

    // Two passes: the description is reassembled on the first and applied on the second.
    c.tick(2);
    assert_eq!(
        c.replay_net_mut().expect("the endpoint").session_state(),
        dereth_client_net::client_session::SessionState::Playable,
        "the description is what puts the client in the world, and the log-off depends on it"
    );
    assert!(
        c.view().world().player_system.module.is_some(),
        "the retained settings are the shard's, applied through the description"
    );
    (c, peer)
}

fn relog_character_set() -> dereth_protocol::login::LoginCharacterSet {
    dereth_protocol::login::LoginCharacterSet {
        status: 0,
        characters: vec![dereth_protocol::login::CharacterIdentity {
            gid: RELOG_LARK,
            name: "Lark".into(),
            seconds_greyed_out: 0,
        }],
        deleted: vec![],
        num_allowed_characters: 5,
        account: "relog".into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 0,
    }
}

/// Run on until the login's own traffic has stopped, so that what a gesture puts on the wire is
/// that gesture's and not the tail of the two questions every entry asks. The shard is told about
/// them and then its record is cleared: the denominator below is *what this gesture sent*, and a
/// login's own messages in it would read as one.
fn relog_settle(c: &mut HeadlessClient, shard: &mut RelogShard) {
    c.tick(6);
    relog_drain_wire(c, shard);
    shard.heard.clear();
}

/// Raise one request the way a panel does -- the queue every production callback pushes into --
/// and let the frame drain it through the real interaction arm.
///
/// Three passes and not one: the first drains the request and hands it to the session, and the
/// **next** one's send phase is what builds the datagram. A scenario that read the wire after one
/// pass would read a message that had been queued and not yet sent.
fn relog_gesture(c: &mut HeadlessClient, r: UiRequest) {
    c.ui_outbox().emit(r);
    c.tick(3);
}

/// What the client is holding for each of the states this is about, read off the one place each of
/// them lives, and comparable across two sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RelogHeld {
    deferred_option: bool,
    auto_saved_option: bool,
    shortcut_slot_3: Option<dereth_primitives::ObjectId>,
    chat_filter: Option<u64>,
    window_x: Option<i32>,
}

fn relog_held(c: &HeadlessClient) -> RelogHeld {
    let ps = &c.view().world().player_system;
    RelogHeld {
        deferred_option: ps.options.get(relog_option::SHOW_TOOLTIPS),
        auto_saved_option: ps.options.get(HEAR_GENERAL_CHAT),
        shortcut_slot_3: ps.shortcut_at(3),
        chat_filter: relog_chat_filter(c, RELOG_WINDOW),
        window_x: relog_window_x(c, RELOG_WINDOW),
    }
}

/// The chat window's own filter, out of the retained settings.
fn relog_chat_filter(c: &HeadlessClient, window: u32) -> Option<u64> {
    let module = c.view().world().player_system.module.as_ref()?;
    dereth_client::hud::decode_chat_filters(module)
        .into_iter()
        .find_map(|(w, m)| (w == window).then_some(m))
}

/// Where the chat window sits, out of the retained settings.
fn relog_window_x(c: &HeadlessClient, window: u32) -> Option<i32> {
    let module = c.view().world().player_system.module.as_ref()?;
    dereth_client::hud::decode_placements(module)
        .get(window)
        .and_then(|p| p.x)
}

// ---------------------------------------------------------------------------------------------
// relog.state.only-the-shortcut-and-the-auto-saved-option-reach-the-shard-at-the-gesture
// ---------------------------------------------------------------------------------------------

/// The ownership map, asserted rather than described. It is the denominator for everything below:
/// three of the four kinds of change put nothing on the wire at the moment they are made, so what
/// happens at the log-off is the whole question.
pub fn only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture() {
    use dereth_client_model::player::options::{is_auto_save_option, PLAYER_OPTIONS};

    // The two ordinals, named out of the table rather than trusted as numbers, with the membership
    // that is the whole distinction between them.
    let named = PLAYER_OPTIONS[HEAR_GENERAL_CHAT].0 == "HearGeneralChat"
        && PLAYER_OPTIONS[relog_option::SHOW_TOOLTIPS].0 == "ShowTooltips"
        && is_auto_save_option(HEAR_GENERAL_CHAT)
        && !is_auto_save_option(relog_option::SHOW_TOOLTIPS);

    let mut shard = RelogShard::new();
    let (mut c, _peer) = a_relog_client(&shard);
    relog_settle(&mut c, &mut shard);

    relog_gesture(
        &mut c,
        UiRequest::DragDrop {
            item: RELOG_ITEM,
            target: DropTarget::ShortcutAlias { slot: 3, from: -1 },
        },
    );
    relog_drain_wire(&mut c, &mut shard);
    let the_drop_is_told_at_once = shard.opcodes() == vec![0x019C];
    shard.heard.clear();

    // Both settings are toggled **off their current value**, never written to a fixed one: writing
    // the value a setting already holds returns before anything is raised, and both of these ship
    // on, so writing `true` would be a no-op that reads exactly like a send that did not happen.
    let want_auto = !c
        .view()
        .world()
        .player_system
        .options
        .get(HEAR_GENERAL_CHAT);
    relog_gesture(
        &mut c,
        UiRequest::SetPlayerOption(RELOG_AUTO_SAVED, want_auto),
    );
    relog_drain_wire(&mut c, &mut shard);
    let the_auto_saved_one_goes_out = shard.opcodes() == vec![0x0005];
    shard.heard.clear();

    let want_deferred = !c
        .view()
        .world()
        .player_system
        .options
        .get(relog_option::SHOW_TOOLTIPS);
    relog_gesture(
        &mut c,
        UiRequest::SetPlayerOption(RELOG_DEFERRED, want_deferred),
    );
    relog_drain_wire(&mut c, &mut shard);
    let the_deferred_one_does_not = shard.opcodes().is_empty();

    relog_gesture(
        &mut c,
        UiRequest::SetChatWindowFilter {
            window: RELOG_WINDOW,
            mask: 0x0000_00FF,
        },
    );
    relog_gesture(
        &mut c,
        UiRequest::SetChatWindowOption {
            window: RELOG_WINDOW,
            property: 0x1000_0086,
            value: 137,
        },
    );
    relog_drain_wire(&mut c, &mut shard);
    let the_window_sends_nothing_either = shard.opcodes().is_empty();

    // ...and the client is holding all four, so the gestures did land somewhere.
    let unsaved = c.view().world().player_system.is_dirty();
    let all_four_landed = relog_held(&c)
        == RelogHeld {
            deferred_option: want_deferred,
            auto_saved_option: want_auto,
            shortcut_slot_3: Some(RELOG_ITEM),
            chat_filter: Some(0x0000_00FF),
            window_x: Some(137),
        };

    c.assert_behaviour(
        "relog.state.only-the-shortcut-and-the-auto-saved-option-reach-the-shard-at-the-gesture",
        move |_| {
            named
                && the_drop_is_told_at_once
                && the_auto_saved_one_goes_out
                && the_deferred_one_does_not
                && the_window_sends_nothing_either
                && unsaved
                && all_four_landed
        },
    );
    c.shutdown();
}

#[test]
fn scenario_only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture() {
    scenario("only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture");
}

// ---------------------------------------------------------------------------------------------
// relog.state.every-deferred-change-is-saved-at-the-log-off-and-found-by-the-next-session
// ---------------------------------------------------------------------------------------------

/// A player's own trip: one client, four changes, a clean log-off, and a second login on the same
/// character whose description is built out of nothing but what the shard was told.
///
/// Every state is read as a **value** at both ends rather than as a difference, because a scenario
/// that compared the second session with the first through the same field could not see a value
/// that was never stored at all.
pub fn every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session() {
    let mut shard = RelogShard::new();

    let (mut c, _peer) = a_relog_client(&shard);
    relog_settle(&mut c, &mut shard);

    let default_tooltips = c
        .view()
        .world()
        .player_system
        .options
        .get(relog_option::SHOW_TOOLTIPS);
    let default_hear = c
        .view()
        .world()
        .player_system
        .options
        .get(HEAR_GENERAL_CHAT);
    let default_filter = relog_chat_filter(&c, RELOG_WINDOW);

    relog_gesture(
        &mut c,
        UiRequest::DragDrop {
            item: RELOG_ITEM,
            target: DropTarget::ShortcutAlias { slot: 3, from: -1 },
        },
    );
    relog_gesture(
        &mut c,
        UiRequest::SetPlayerOption(RELOG_AUTO_SAVED, !default_hear),
    );
    relog_gesture(
        &mut c,
        UiRequest::SetPlayerOption(RELOG_DEFERRED, !default_tooltips),
    );
    relog_gesture(
        &mut c,
        UiRequest::SetChatWindowFilter {
            window: RELOG_WINDOW,
            mask: 0x0000_00FF,
        },
    );
    relog_gesture(
        &mut c,
        UiRequest::SetChatWindowOption {
            window: RELOG_WINDOW,
            property: 0x1000_0086,
            value: 137,
        },
    );

    let left = relog_held(&c);
    let first_session_left =
        left == RelogHeld {
            deferred_option: !default_tooltips,
            auto_saved_option: !default_hear,
            shortcut_slot_3: Some(RELOG_ITEM),
            chat_filter: Some(0x0000_00FF),
            window_x: Some(137),
        } && left.chat_filter != default_filter;

    // The clean log-off, through the screen's own answer.
    relog_gesture(&mut c, UiRequest::EndCharacterSession { ask: false });
    relog_drain_wire(&mut c, &mut shard);
    let heard = shard.opcodes();
    let saved_once =
        heard.contains(&0x01A1) && c.view().expect_app().player_modules_saved_at_logout() == 1;
    // ...and in the client's own order: the settings go out ahead of the departure.
    let in_order = match (
        heard.iter().position(|o| *o == 0x01A1),
        heard.iter().position(|o| *o == 0xF653),
    ) {
        (Some(a), Some(b)) => a < b,
        _ => false,
    };

    // The notebook is the one thing on this list that is not the shard's, and it is not on the
    // wire: the settings the shard now stores carry no text of any kind.
    let nothing_of_the_notebook = shard
        .player_description()
        .player_module
        .timestamp_format
        .is_none()
        && shard.heard.iter().all(|(o, _)| o.0 != 0x0295);

    // The shard answers as it does six seconds later, and the client keeps running.
    c.app_mut().process_logon_event_queue(vec![
        dereth_client_net::client_session::SessionEvent::LoggedOff,
        dereth_client_net::client_session::SessionEvent::CharacterSet(Box::new(
            relog_character_set(),
        )),
    ]);
    let mut still_running = true;
    for _ in 0..4 {
        still_running &= c.app_mut().frame();
    }
    relog_drain_wire(&mut c, &mut shard);
    c.shutdown();

    // The second session, on a description the shard built out of the wire alone.
    let (mut c, _peer) = a_relog_client(&shard);
    let found = relog_held(&c);

    c.assert_behaviour(
        "relog.state.every-deferred-change-is-saved-at-the-log-off-and-found-by-the-next-session",
        move |_| {
            first_session_left
                && saved_once
                && in_order
                && nothing_of_the_notebook
                && still_running
                && found == left
        },
    );
    c.shutdown();
}

#[test]
fn scenario_every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session() {
    scenario("every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session");
}

// ---------------------------------------------------------------------------------------------
// relog.state.a-session-that-changed-nothing-deferred-saves-nothing-at-the-log-off
// ---------------------------------------------------------------------------------------------

/// The other direction, and the one that keeps the saving from being a blanket send: a session
/// whose only change was a setting that had already gone out saves nothing at all on the way out,
/// and the departure itself still goes.
pub fn a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off() {
    let mut shard = RelogShard::new();
    let (mut c, _peer) = a_relog_client(&shard);
    relog_settle(&mut c, &mut shard);

    let want = !c
        .view()
        .world()
        .player_system
        .options
        .get(HEAR_GENERAL_CHAT);
    relog_gesture(&mut c, UiRequest::SetPlayerOption(RELOG_AUTO_SAVED, want));
    relog_drain_wire(&mut c, &mut shard);
    let it_went_out_on_its_own = shard.heard.iter().any(|(o, _)| o.0 == 0x0005)
        && !c.view().world().player_system.is_dirty();
    shard.heard.clear();

    relog_gesture(&mut c, UiRequest::EndCharacterSession { ask: false });
    relog_drain_wire(&mut c, &mut shard);
    let heard = shard.opcodes();
    let nothing_was_saved = !heard.contains(&0x01A1);
    let and_it_still_left = heard.contains(&0xF653);

    c.assert_behaviour(
        "relog.state.a-session-that-changed-nothing-deferred-saves-nothing-at-the-log-off",
        move |_| it_went_out_on_its_own && nothing_was_saved && and_it_still_left,
    );
    c.shutdown();
}

#[test]
fn scenario_a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off() {
    scenario("a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off");
}

// ---------------------------------------------------------------------------------------------
// keys.own.each-of-this-clients-actions-works-on-a-key-the-page-gives-it
// ---------------------------------------------------------------------------------------------

/// The row of one of this client's own actions, its first key cell.
fn own_cell(app: &mut dereth_client::app::App, action: u32) -> dereth_ui::ElemHandle {
    let s = kb_screen(app);
    let i = s
        .key_bindings
        .row_of(dereth_input::dereth::INPUT_MAP, ActionId(action))
        .expect("this client's action has a row");
    s.key_bindings.rows[i].key_buttons[0]
}

fn pref_on(name: &str) -> bool {
    matches!(
        dereth_client_contract::options::store::inq_value(name),
        Some(dereth_client_contract::PrefValue::Bool(true))
    )
}

/// Each of this client's own actions the retail interface answers, given a free key on the key
/// page as a player gives it one, answers that key: the performance panel, the inverted mouse
/// look and mute-when-inactive flip their settings, the trade key shows the trade window, and
/// hold sidestep is held for as long as its key is.
pub fn each_of_this_clients_actions_works_on_a_key_the_page_gives_it() {
    use dereth_client_contract::actions::dereth as own;
    let mut c = a_client_for_the_key_bindings_page();
    let mut hands = Hands::new();
    // Keys no shipped map binds, one for each action.
    let free = [
        (own::TOGGLE_PERFORMANCE_PANEL, KeyCode::F7),
        (own::TOGGLE_INVERT_MOUSE_LOOK, KeyCode::KeyV),
        (own::TOGGLE_MUTE_ON_LOSING_FOCUS, KeyCode::ScrollLock),
        (own::TOGGLE_TRADE_PANEL, KeyCode::Numpad7),
        (own::MOVEMENT_HOLD_SIDESTEP, KeyCode::Numpad9),
    ];
    let mut bound = Vec::new();
    for (action, code) in free {
        let cell = own_cell(c.app_mut(), action);
        click_element(&mut c, cell);
        hands.tap(&mut c, key(code));
        c.tick(2);
        let keys = c
            .app_mut()
            .input_manager_mut()
            .expect("the input shell")
            .keys_for_action(ActionId(action), dereth_input::dereth::INPUT_MAP);
        bound.push((action, keys.len()));
    }
    let all_bound = bound.iter().all(|(_, n)| *n == 1);

    let flips = |c: &mut HeadlessClient, hands: &mut Hands, code, name: &str| {
        let was = pref_on(name);
        hands.tap(c, key(code));
        c.tick(3);
        pref_on(name) != was
    };
    let perf = flips(
        &mut c,
        &mut hands,
        KeyCode::F7,
        dereth_client_contract::options::performance::PERFORMANCE_PANEL,
    );
    let invert = flips(
        &mut c,
        &mut hands,
        KeyCode::KeyV,
        "Input.InvertMouseLookYAxis",
    );
    let mute = flips(
        &mut c,
        &mut hands,
        KeyCode::ScrollLock,
        "Sound.PlaySoundOnlyWhenActive",
    );
    let trade_window = {
        let shell = c.app_mut().ui_mut().expect("the shell");
        let root = shell
            .flow
            .current()
            .and_then(|s| s.roots().first().copied())
            .expect("the gameplay root");
        shell
            .ui
            .get_child_recursive(root, dereth_ui_screens::panels::trade::WINDOW)
            .expect("the trade window")
    };
    let trade_shown = |c: &mut HeadlessClient| {
        c.app_mut()
            .ui_mut()
            .and_then(|s| s.ui.node(trade_window))
            .is_some_and(|n| n.region.flags.visible)
    };
    let trade_before = trade_shown(&mut c);
    hands.tap(&mut c, key(KeyCode::Numpad7));
    c.tick(3);
    let trade = trade_shown(&mut c) != trade_before;
    hands.key(&mut c, key(KeyCode::Numpad9), true);
    c.tick(3);
    let held = c.app_mut().movement.lists.hold_sidestep;
    hands.key(&mut c, key(KeyCode::Numpad9), false);
    c.tick(3);
    let let_go = !c.app_mut().movement.lists.hold_sidestep;

    c.assert_behaviour(
        "keys.own.each-of-this-clients-actions-works-on-a-key-the-page-gives-it",
        move |_| {
            eprintln!(
                "bound {bound:?} perf {perf} invert {invert} mute {mute} trade {trade} held {held} let go {let_go}"
            );
            all_bound && perf && invert && mute && trade && held && let_go
        },
    );
    c.shutdown();
}

#[test]
fn scenario_each_of_this_clients_actions_works_on_a_key_the_page_gives_it() {
    scenario("each_of_this_clients_actions_works_on_a_key_the_page_gives_it");
}

// ---------------------------------------------------------------------------------------------
// keys.own.a-key-in-use-given-to-this-clients-action-asks-first-and-is-taken
// ---------------------------------------------------------------------------------------------

/// A key another action has, given to one of this client's own actions, raises the same question
/// a shipped action's row raises; yes takes the key from the other action, and the key then
/// answers this client's action.
pub fn a_key_in_use_given_to_this_clients_action_asks_first_and_is_taken() {
    use dereth_client_contract::actions::dereth as own;
    const PICK_UP: ActionId = ActionId(0x1000_002C);
    const ITEMS: InputMapId = InputMapId(0x1000_0007);
    const KB_F: u16 = 0x21;
    let mut c = a_client_on_the_key_bindings("own-conflict");
    let mut hand = KeyHand::new();
    let perf = ActionId(own::TOGGLE_PERFORMANCE_PANEL);
    let picks_up_on_f = |c: &mut dereth_client::app::App| {
        c.input_manager_mut()
            .expect("the input manager")
            .keys_for_action(PICK_UP, ITEMS)
            .iter()
            .any(|k| k.control.offset() == KB_F && k.meta_mode == 0)
    };
    let f_picked_up_first = picks_up_on_f(c.app_mut());
    let cell = {
        let s = kb_screen(c.app_mut());
        let i = s
            .key_bindings
            .row_of(dereth_input::dereth::INPUT_MAP, perf)
            .expect("the performance panel has a row");
        s.key_bindings.rows[i].key_buttons[0]
    };
    // The row is down the interface tab's list, out of the list's view: its press is the
    // element message a click raises.
    click_element(&mut c, cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyF);
    let context = {
        let s = kb_screen(c.app_mut());
        let i = s
            .key_bindings
            .row_of(dereth_input::dereth::INPUT_MAP, perf)
            .expect("the row");
        s.key_bindings.rows[i].dialog_context(RowDialog::Overwrite)
    };
    let asked = context.is_some();
    let not_yet = !c
        .app_mut()
        .input_manager_mut()
        .expect("the input manager")
        .keys_for_action(perf, dereth_input::dereth::INPUT_MAP)
        .iter()
        .any(|k| k.control.offset() == KB_F);
    if let Some(context) = context {
        let question = kb_ui(c.app_mut())
            .dialogs
            .info(context)
            .and_then(|info| info.element)
            .expect("the question is drawn");
        let yes = kb_child(
            c.app_mut(),
            question,
            dereth_ui::dialog::base::child::BUTTON1,
        );
        kb_press(c.app_mut(), yes);
    }
    let taken = c
        .app_mut()
        .input_manager_mut()
        .expect("the input manager")
        .keys_for_action(perf, dereth_input::dereth::INPUT_MAP)
        .iter()
        .any(|k| k.control.offset() == KB_F)
        && !picks_up_on_f(c.app_mut());
    let name = dereth_client_contract::options::performance::PERFORMANCE_PANEL;
    let on = |n: &str| {
        matches!(
            dereth_client_contract::options::store::inq_value(n),
            Some(dereth_client_contract::PrefValue::Bool(true))
        )
    };
    let was = on(name);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyF);
    c.app_mut().frame();
    let answers = on(name) != was;

    c.assert_behaviour(
        "keys.own.a-key-in-use-given-to-this-clients-action-asks-first-and-is-taken",
        move |_| {
            eprintln!(
                "first {f_picked_up_first} asked {asked} not yet {not_yet} taken {taken} answers {answers}"
            );
            f_picked_up_first && asked && not_yet && taken && answers
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_key_in_use_given_to_this_clients_action_asks_first_and_is_taken() {
    scenario("a_key_in_use_given_to_this_clients_action_asks_first_and_is_taken");
}
