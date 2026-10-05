//! The DAT UI scenarios: notices, HUD controls, text, focus and shipped presentation.
//! Subject modules own related fixtures and bodies; declarations remain here to preserve their
//! behavior stations. Character-creation coverage lives with the shell's wizard scenarios.
//! Run serially because whole headless clients share UI request globals.

mod autorun;
mod caret;
mod connection;
mod dates;
mod failures;
mod focus;
mod housing;
mod hud;
mod items;
mod lamps;
mod lists;
mod notices;
mod outlines;
mod reader;
mod recorded_notices;
mod screens;
mod states;
mod strings;
mod support;
mod surfaces;
mod text;
mod tooltips;

use autorun::{
    a_key_that_changes_nothing_says_nothing,
    the_autorun_line_reaches_the_strip_and_the_chat_windows_drop_it,
    the_run_lock_says_so_in_the_message_window,
};
use caret::the_caret_flashes_at_the_players_own_desktop_interval;
use connection::{
    a_real_failure_takes_the_player_off_the_world, setting_the_clock_does_not_change_the_lamp,
};
use dates::{
    every_date_on_the_house_pane_is_in_the_machines_own_time,
    the_ban_expiry_keeps_its_shape_and_is_in_the_machines_own_time,
    the_character_sheets_born_line_is_in_the_machines_own_time,
    the_chat_stamp_carries_the_machines_own_offset,
    the_line_saying_when_another_house_may_be_bought_is_in_the_machines_own_time,
};
use failures::{
    a_recall_broken_by_moving_says_so_on_the_strip,
    a_refusal_carrying_the_shards_word_puts_it_in_the_line,
    a_refused_portal_says_so_in_the_chat_log, every_refusal_draws_its_own_line_on_its_own_surface,
};
use focus::{
    a_log_takes_the_keyboard_and_none_of_what_is_typed,
    a_press_in_a_box_takes_the_keyboard_and_keeps_it,
    dragging_the_stack_slider_moves_the_thumb_and_the_quantity_follows,
    the_chat_bars_thumb_is_sized_and_placed_inside_its_track,
    what_is_typed_reaches_the_box_holding_the_keyboard, SEL_OBJECT_FIELD,
};
use housing::{
    the_house_tab_tells_a_houseless_character_they_have_no_house,
    the_houseless_line_is_written_once_and_into_that_pane_alone,
};
use hud::{
    a_jump_shows_one_power_bar_and_never_the_other,
    a_press_on_the_vitals_bar_flips_it_between_its_two_presentations,
    both_raise_buttons_on_both_pages_put_the_request_on_the_wire, hud_centre, hud_gameplay,
    hud_visible, the_first_press_on_the_vitals_bar_changes_nothing_on_screen,
};
use items::{
    dropping_a_thing_back_on_its_own_slot_sends_nothing_and_ghosts_nothing,
    empty_a_single_item_and_a_stack_are_three_different_strips,
    the_highlight_comes_down_by_itself_a_quarter_second_later,
    the_meters_start_down_and_a_creatures_answer_brings_its_bar_up,
    the_players_own_cell_draws_a_backpack_and_not_a_second_backdrop,
    what_follows_the_cursor_is_the_icon_alone,
};
use lamps::{
    a_buff_lights_one_lamp_and_a_debuff_the_other,
    a_button_of_the_strip_with_no_action_of_its_own_opens_nothing, a_described_character,
    an_enchantment, enchant, every_lamp_opens_its_own_panel, light_the_lamp, press_the_strip,
    set_int_quality, the_burden_lamp_crosses_both_thresholds_on_the_characters_own_capacity,
    the_link_lamp_comes_up_good_and_falls_to_lost,
    the_vitae_lamp_lights_for_a_penalty_and_not_for_a_multiplier_of_one,
    the_way_out_of_the_strip_raises_the_question_about_ending_the_session, BURDEN_LAMP, VITAE_LAMP,
};
use lists::pressing_a_row_draws_the_band_across_that_row;
use notices::{
    a_refusal_is_a_bubble_and_not_a_chat_line, a_swing_with_nothing_selected_says_so_on_the_strip,
    a_use_of_the_recorded_locked_chest_says_so_in_the_strip, the_notice_bubble_clears_itself,
    the_shards_refusal_reaches_the_strip_and_not_the_scrollback,
};
use outlines::{
    every_element_that_asks_for_an_outline_is_drawn_with_one,
    the_outline_comes_from_the_fonts_own_heavier_sheet,
    the_refusal_strip_is_drawn_with_an_outline_and_the_chat_log_is_not,
};
use reader::{
    a_recorded_book_opens_the_reader_and_shows_its_first_page, drawn_colour, drawn_text, hud_state,
    paging_a_book_moves_the_page_and_greys_what_cannot_be_pressed, under,
};
use recorded_notices::{
    a_refusal_about_something_the_player_asked_for_names_it_and_says_why,
    every_recorded_refusal_and_nothing_else_reaches_the_strip,
};
use screens::putting_the_same_screen_up_again_builds_it_fresh;
use states::{
    a_state_with_nothing_to_draw_runs_nothing, a_toggle_is_not_unticked_by_its_own_state,
};
use strings::{
    a_line_places_the_values_by_name_and_not_by_the_order_given,
    a_run_of_spaces_in_a_shipped_line_is_drawn_as_one,
    a_value_the_caller_did_not_give_is_nothing_rather_than_a_complaint,
    the_key_binding_prompt_loses_both_of_its_double_spaces,
    the_one_line_that_asks_to_keep_its_spaces_keeps_them,
    the_pane_draws_the_tidied_line_and_not_the_shipped_double_space,
};
use support::{
    a_browser_that_will_not_open_says_so_in_a_box_with_the_address_in_it,
    each_support_button_opens_its_in_game_form,
    the_refusal_for_a_typed_command_does_not_apply_to_the_window,
    the_urgent_assistance_report_goes_out_on_the_help_channel,
};
use surfaces::{
    a_colour_picker_is_sized_from_its_own_box_whatever_it_asks_for,
    a_shipped_root_that_owns_no_surface_is_left_without_one,
    an_element_owning_no_surface_is_stepped_over,
    every_element_of_a_live_screen_is_sized_from_its_own_box,
};
use text::{
    a_caption_that_fits_is_still_centred,
    a_caption_too_tall_for_its_box_shows_its_first_line_at_the_top,
    a_paragraph_too_tall_for_its_box_keeps_every_line,
    a_scrolling_pane_measures_itself_from_the_draw, a_wrapped_line_drops_the_width_of_its_break,
    a_wrapped_right_justified_caption_is_flush_with_its_box,
};
use tooltips::{
    a_label_too_long_for_its_box_can_be_read_and_catches_the_pointer, every_element,
    resting_the_pointer_on_something_draws_a_box_with_words_in_it,
    the_words_in_a_tooltip_are_the_ones_that_element_was_given,
};

pub(super) use focus::who_holds_the_keyboard;
pub(super) use hud::hud_find;

use dereth_testkit::adapters_shell::Hands;
use dereth_testkit::{ClientSpec, HeadlessClient, Player};

dereth_testkit::scenarios! {
    scenario_the_notice_bubble_clears_itself => the_notice_bubble_clears_itself ["notice.bubble.clears-itself-after-five-seconds"],
    scenario_a_refusal_is_a_bubble_and_not_a_chat_line => a_refusal_is_a_bubble_and_not_a_chat_line ["notice.refusal.is-a-bubble-and-not-a-chat-line"],
    scenario_the_house_tab_tells_a_houseless_character_they_have_no_house => the_house_tab_tells_a_houseless_character_they_have_no_house ["house.tab.a-character-with-no-house-is-told-so"],
    scenario_the_houseless_line_is_written_once_and_into_that_pane_alone => the_houseless_line_is_written_once_and_into_that_pane_alone ["house.tab.the-line-is-written-once-and-into-that-pane-alone"],
    scenario_a_use_of_the_recorded_locked_chest_says_so_in_the_strip => a_use_of_the_recorded_locked_chest_says_so_in_the_strip ["notice.locked-container.a-use-of-a-locked-one-says-so-in-the-strip"],
    scenario_a_toggle_is_not_unticked_by_its_own_state => a_toggle_is_not_unticked_by_its_own_state ["ui.states.a-toggle-is-not-unticked-by-its-own-change-of-state"],
    scenario_a_state_with_nothing_to_draw_runs_nothing => a_state_with_nothing_to_draw_runs_nothing ["ui.states.a-state-declared-with-no-media-runs-nothing-and-an-undeclared-one-runs-the-base"],
    scenario_the_shards_refusal_reaches_the_strip_and_not_the_scrollback => the_shards_refusal_reaches_the_strip_and_not_the_scrollback ["notice.locked-container.the-shards-own-refusal-reaches-the-strip-and-not-the-scrollback"],
    scenario_putting_the_same_screen_up_again_builds_it_fresh => putting_the_same_screen_up_again_builds_it_fresh ["ui.screen-rebuild.putting-the-same-screen-up-again-builds-it-fresh"],
    scenario_a_wrapped_right_justified_caption_is_flush_with_its_box => a_wrapped_right_justified_caption_is_flush_with_its_box ["ui.text.a-right-justified-caption-that-wraps-sits-flush-against-its-box"],
    scenario_a_wrapped_line_drops_the_width_of_its_break => a_wrapped_line_drops_the_width_of_its_break ["ui.text.a-wrapped-line-does-not-carry-the-width-of-the-space-it-broke-at"],
    scenario_a_scrolling_pane_measures_itself_from_the_draw => a_scrolling_pane_measures_itself_from_the_draw ["ui.text.a-scrolling-pane-measures-what-it-holds-from-the-draw-itself"],
    scenario_the_run_lock_says_so_in_the_message_window => the_run_lock_says_so_in_the_message_window ["notice.autorun.turning-the-run-lock-on-or-off-says-so-in-the-message-window"],
    scenario_a_key_that_changes_nothing_says_nothing => a_key_that_changes_nothing_says_nothing ["notice.autorun.a-key-that-does-not-change-the-run-lock-says-nothing"],
    scenario_the_autorun_line_reaches_the_strip_and_the_chat_windows_drop_it => the_autorun_line_reaches_the_strip_and_the_chat_windows_drop_it ["notice.autorun.the-line-reaches-the-strip-and-the-chat-windows-drop-it"],
    scenario_pressing_a_row_draws_the_band_across_that_row => pressing_a_row_draws_the_band_across_that_row ["ui.list.pressing-a-row-draws-the-band-across-that-row-and-no-other"],
    scenario_a_caption_too_tall_for_its_box_shows_its_first_line_at_the_top => a_caption_too_tall_for_its_box_shows_its_first_line_at_the_top ["ui.text.a-caption-too-tall-for-its-box-shows-its-first-line-at-the-top"],
    scenario_a_caption_that_fits_is_still_centred => a_caption_that_fits_is_still_centred ["ui.text.a-caption-that-fits-and-asks-to-be-centred-is-still-centred"],
    scenario_a_paragraph_too_tall_for_its_box_keeps_every_line => a_paragraph_too_tall_for_its_box_keeps_every_line ["ui.text.a-paragraph-too-tall-for-its-box-keeps-every-line-and-scrolls"],
    scenario_a_recall_broken_by_moving_says_so_on_the_strip => a_recall_broken_by_moving_says_so_on_the_strip ["notice.failure.a-recall-broken-by-moving-says-so-on-the-strip"],
    scenario_every_refusal_draws_its_own_line_on_its_own_surface => every_refusal_draws_its_own_line_on_its_own_surface ["notice.failure.every-refusal-the-shard-can-send-draws-its-own-line-on-its-own-surface"],
    scenario_a_refusal_carrying_the_shards_word_puts_it_in_the_line => a_refusal_carrying_the_shards_word_puts_it_in_the_line ["notice.failure.a-refusal-carrying-the-shards-own-word-puts-it-in-the-line"],
    scenario_the_caret_flashes_at_the_players_own_desktop_interval => the_caret_flashes_at_the_players_own_desktop_interval ["ui.caret.flashes-at-the-interval-the-player-set-for-their-desktop"],
    scenario_a_jump_shows_one_power_bar_and_never_the_other => a_jump_shows_one_power_bar_and_never_the_other ["hud.power-bar.a-jump-shows-the-one-bar-the-client-listens-with-and-never-the-other"],
    scenario_both_raise_buttons_on_both_pages_put_the_request_on_the_wire => both_raise_buttons_on_both_pages_put_the_request_on_the_wire ["character-page.raise.both-buttons-on-both-pages-put-the-request-on-the-wire"],
    scenario_the_first_press_on_the_vitals_bar_changes_nothing_on_screen => the_first_press_on_the_vitals_bar_changes_nothing_on_screen ["hud.vitals.the-first-press-changes-nothing-on-screen-and-the-second-hides-the-numbers"],
    scenario_a_recorded_book_opens_the_reader_and_shows_its_first_page => a_recorded_book_opens_the_reader_and_shows_its_first_page ["reader.book.a-recorded-book-opens-the-reader-and-shows-its-first-page"],
    scenario_paging_a_book_moves_the_page_and_greys_what_cannot_be_pressed => paging_a_book_moves_the_page_and_greys_what_cannot_be_pressed ["reader.book.paging-moves-the-page-and-greys-the-control-that-cannot-be-pressed"],
    scenario_a_refused_portal_says_so_in_the_chat_log => a_refused_portal_says_so_in_the_chat_log ["notice.failure.a-refused-portal-says-so-in-the-chat-log-in-its-own-colour"],
    scenario_a_press_on_the_vitals_bar_flips_it_between_its_two_presentations => a_press_on_the_vitals_bar_flips_it_between_its_two_presentations ["hud.vitals.a-press-on-the-bar-flips-it-between-its-two-presentations"],
    scenario_resting_the_pointer_on_something_draws_a_box_with_words_in_it => resting_the_pointer_on_something_draws_a_box_with_words_in_it ["tooltip.resting-the-pointer-on-something-draws-a-box-with-words-in-it"],
    scenario_the_words_in_a_tooltip_are_the_ones_that_element_was_given => the_words_in_a_tooltip_are_the_ones_that_element_was_given ["tooltip.the-words-are-the-ones-that-element-was-given"],
    scenario_a_label_too_long_for_its_box_can_be_read_and_catches_the_pointer => a_label_too_long_for_its_box_can_be_read_and_catches_the_pointer ["tooltip.a-label-too-long-for-its-box-can-be-read-and-catches-the-pointer"],
    scenario_setting_the_clock_does_not_change_the_lamp => setting_the_clock_does_not_change_the_lamp ["hud.link-lamp.the-shard-setting-the-clock-does-not-change-it-and-nor-does-a-quiet-link"],
    scenario_a_real_failure_takes_the_player_off_the_world => a_real_failure_takes_the_player_off_the_world ["hud.link-lamp.a-real-failure-puts-the-player-off-the-world-rather-than-reddening-a-lamp"],
    scenario_each_support_button_opens_its_in_game_form => each_support_button_opens_its_in_game_form ["options.support.each-support-button-opens-its-in-game-form"],
    scenario_a_browser_that_will_not_open_says_so_in_a_box_with_the_address_in_it => a_browser_that_will_not_open_says_so_in_a_box_with_the_address_in_it ["options.support.a-browser-that-will-not-open-says-so-in-a-box-with-the-address-in-it"],
    scenario_the_urgent_assistance_report_goes_out_on_the_help_channel => the_urgent_assistance_report_goes_out_on_the_help_channel ["urgent-assistance.send.the-report-goes-out-on-the-help-channel"],
    scenario_the_refusal_for_a_typed_command_does_not_apply_to_the_window => the_refusal_for_a_typed_command_does_not_apply_to_the_window ["urgent-assistance.send.the-refusal-that-applies-to-a-typed-command-does-not-apply-here"],
    scenario_the_refusal_strip_is_drawn_with_an_outline_and_the_chat_log_is_not => the_refusal_strip_is_drawn_with_an_outline_and_the_chat_log_is_not ["ui.text.outline.the-refusal-strip-is-drawn-with-one-and-the-chat-log-is-not"],
    scenario_every_element_that_asks_for_an_outline_is_drawn_with_one => every_element_that_asks_for_an_outline_is_drawn_with_one ["ui.text.outline.every-element-that-asks-for-one-is-drawn-with-one-and-no-other-is"],
    scenario_the_outline_comes_from_the_fonts_own_heavier_sheet => the_outline_comes_from_the_fonts_own_heavier_sheet ["ui.text.outline.it-comes-from-the-fonts-own-heavier-sheet-and-fits-inside-it"],
    scenario_every_element_of_a_live_screen_is_sized_from_its_own_box => every_element_of_a_live_screen_is_sized_from_its_own_box ["ui.surface.every-element-of-a-live-screen-is-sized-from-its-own-box-by-default"],
    scenario_an_element_owning_no_surface_is_stepped_over => an_element_owning_no_surface_is_stepped_over ["ui.surface.an-element-owning-none-is-stepped-over-and-the-answer-is-its-owners"],
    scenario_a_colour_picker_is_sized_from_its_own_box_whatever_it_asks_for => a_colour_picker_is_sized_from_its_own_box_whatever_it_asks_for ["ui.surface.a-colour-picker-is-sized-from-its-own-box-whatever-the-layout-asks-for"],
    scenario_a_shipped_root_that_owns_no_surface_is_left_without_one => a_shipped_root_that_owns_no_surface_is_left_without_one ["ui.surface.a-shipped-root-that-owns-none-is-left-without-one-though-the-maker-asks"],
    scenario_a_press_in_a_box_takes_the_keyboard_and_keeps_it => a_press_in_a_box_takes_the_keyboard_and_keeps_it ["ui.focus.a-press-in-a-box-takes-the-keyboard-and-keeps-it"],
    scenario_what_is_typed_reaches_the_box_holding_the_keyboard => what_is_typed_reaches_the_box_holding_the_keyboard ["ui.focus.what-is-typed-reaches-the-box-holding-the-keyboard-and-stops-when-it-lets-go"],
    scenario_a_log_takes_the_keyboard_and_none_of_what_is_typed => a_log_takes_the_keyboard_and_none_of_what_is_typed ["ui.focus.a-log-takes-the-keyboard-and-none-of-what-is-typed"],
    scenario_dragging_the_stack_slider_moves_the_thumb_and_the_quantity_follows => dragging_the_stack_slider_moves_the_thumb_and_the_quantity_follows ["ui.scrollbar.dragging-the-stack-slider-moves-the-thumb-and-the-quantity-follows"],
    scenario_the_chat_bars_thumb_is_sized_and_placed_inside_its_track => the_chat_bars_thumb_is_sized_and_placed_inside_its_track ["ui.scrollbar.the-chat-bars-thumb-is-sized-and-placed-inside-its-track"],
    scenario_the_highlight_comes_down_by_itself_a_quarter_second_later => the_highlight_comes_down_by_itself_a_quarter_second_later ["ui.selection-strip.the-highlight-comes-down-by-itself-a-quarter-second-later"],
    scenario_the_meters_start_down_and_a_creatures_answer_brings_its_bar_up => the_meters_start_down_and_a_creatures_answer_brings_its_bar_up ["ui.selection-strip.the-meters-start-down-and-a-creatures-answer-brings-its-bar-up"],
    scenario_empty_a_single_item_and_a_stack_are_three_different_strips => empty_a_single_item_and_a_stack_are_three_different_strips ["ui.selection-strip.empty-a-single-item-and-a-stack-are-three-different-strips"],
    scenario_the_players_own_cell_draws_a_backpack_and_not_a_second_backdrop => the_players_own_cell_draws_a_backpack_and_not_a_second_backdrop ["ui.item-cell.the-players-own-cell-draws-a-backpack-and-not-a-second-backdrop"],
    scenario_what_follows_the_cursor_is_the_icon_alone => what_follows_the_cursor_is_the_icon_alone ["ui.item-cell.what-follows-the-cursor-is-the-icon-alone-and-not-the-lifted-cell"],
    scenario_dropping_a_thing_back_on_its_own_slot_sends_nothing_and_ghosts_nothing => dropping_a_thing_back_on_its_own_slot_sends_nothing_and_ghosts_nothing ["ui.item-cell.dropping-a-thing-back-on-its-own-slot-sends-nothing-and-ghosts-nothing"],
    scenario_the_way_out_of_the_strip_raises_the_question_about_ending_the_session => the_way_out_of_the_strip_raises_the_question_about_ending_the_session ["hud.lamp-row.the-way-out-of-the-strip-raises-the-question-about-ending-the-session"],
    scenario_every_lamp_opens_its_own_panel => every_lamp_opens_its_own_panel ["hud.lamp-row.every-lamp-opens-its-own-panel-and-leaves-the-other-lamps-panels-down"],
    scenario_a_button_of_the_strip_with_no_action_of_its_own_opens_nothing => a_button_of_the_strip_with_no_action_of_its_own_opens_nothing ["hud.lamp-row.a-button-of-the-strip-with-no-action-of-its-own-opens-nothing"],
    scenario_the_burden_lamp_crosses_both_thresholds_on_the_characters_own_capacity => the_burden_lamp_crosses_both_thresholds_on_the_characters_own_capacity ["hud.lamp-row.the-burden-lamp-crosses-both-thresholds-on-the-characters-own-capacity"],
    scenario_a_buff_lights_one_lamp_and_a_debuff_the_other => a_buff_lights_one_lamp_and_a_debuff_the_other ["hud.lamp-row.a-buff-lights-one-lamp-a-debuff-the-other-and-a-purge-puts-both-out"],
    scenario_the_vitae_lamp_lights_for_a_penalty_and_not_for_a_multiplier_of_one => the_vitae_lamp_lights_for_a_penalty_and_not_for_a_multiplier_of_one ["hud.lamp-row.the-vitae-lamp-lights-for-a-penalty-and-not-for-a-multiplier-of-one"],
    scenario_the_link_lamp_comes_up_good_and_falls_to_lost => the_link_lamp_comes_up_good_and_falls_to_lost ["hud.link-lamp.it-comes-up-good-and-falls-to-lost-when-nothing-is-heard-at-all"],
    scenario_a_swing_with_nothing_selected_says_so_on_the_strip => a_swing_with_nothing_selected_says_so_on_the_strip ["notice.a-swing-with-nothing-selected-says-so-on-the-strip"],
    scenario_the_character_sheets_born_line_is_in_the_machines_own_time => the_character_sheets_born_line_is_in_the_machines_own_time ["dates.the-character-sheets-born-line-is-in-the-machines-own-time-and-that-shape"],
    scenario_every_date_on_the_house_pane_is_in_the_machines_own_time => every_date_on_the_house_pane_is_in_the_machines_own_time ["dates.every-date-on-the-house-pane-is-in-the-machines-own-time-and-that-shape"],
    scenario_the_line_saying_when_another_house_may_be_bought_is_in_the_machines_own_time => the_line_saying_when_another_house_may_be_bought_is_in_the_machines_own_time ["dates.the-line-saying-when-another-house-may-be-bought-is-in-the-machines-own-time"],
    scenario_the_chat_stamp_carries_the_machines_own_offset => the_chat_stamp_carries_the_machines_own_offset ["dates.the-chat-stamp-carries-the-machines-own-offset-and-not-a-constant"],
    scenario_the_ban_expiry_keeps_its_shape_and_is_in_the_machines_own_time => the_ban_expiry_keeps_its_shape_and_is_in_the_machines_own_time ["dates.the-ban-expiry-keeps-the-other-shape-and-is-in-the-machines-own-time"],
    scenario_a_run_of_spaces_in_a_shipped_line_is_drawn_as_one => a_run_of_spaces_in_a_shipped_line_is_drawn_as_one ["strings.a-run-of-spaces-in-a-shipped-line-is-drawn-as-one-and-nothing-else-moves"],
    scenario_the_one_line_that_asks_to_keep_its_spaces_keeps_them => the_one_line_that_asks_to_keep_its_spaces_keeps_them ["strings.the-one-line-that-asks-to-keep-its-spaces-keeps-them-and-loses-its-asking"],
    scenario_a_value_the_caller_did_not_give_is_nothing_rather_than_a_complaint => a_value_the_caller_did_not_give_is_nothing_rather_than_a_complaint ["strings.a-value-the-caller-did-not-give-is-nothing-rather-than-a-complaint"],
    scenario_a_line_places_the_values_by_name_and_not_by_the_order_given => a_line_places_the_values_by_name_and_not_by_the_order_given ["strings.a-row-places-the-values-by-name-so-the-order-they-are-given-in-is-invisible"],
    scenario_the_key_binding_prompt_loses_both_of_its_double_spaces => the_key_binding_prompt_loses_both_of_its_double_spaces ["strings.the-key-binding-prompt-loses-both-of-its-double-spaces"],
    scenario_the_pane_draws_the_tidied_line_and_not_the_shipped_double_space => the_pane_draws_the_tidied_line_and_not_the_shipped_double_space ["strings.the-pane-draws-the-tidied-line-and-not-the-shipped-double-space"],
    scenario_every_recorded_refusal_and_nothing_else_reaches_the_strip => every_recorded_refusal_and_nothing_else_reaches_the_strip ["notice.refusal.every-recorded-refusal-and-nothing-else-reaches-the-strip"],
    scenario_a_refusal_about_something_the_player_asked_for_names_it_and_says_why => a_refusal_about_something_the_player_asked_for_names_it_and_says_why ["notice.refusal.a-refusal-about-something-the-player-asked-for-names-it-and-says-why"],
}

use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};
use dereth_ui_screens::panels::{attributes, skills, statmgmt};
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

use dereth_ui_screens::options::gameplay::button as support_button;
use dereth_ui_screens::options::pages::SHELL_EXECUTE_ERROR_TITLE;
use dereth_ui_screens::panels::urgent_assistance as ua;

use dereth_ui::props::{attr, UiObjectMode};

use dereth_ui_screens::hud::indicators;

use dereth_desktop::platform::local_utc_offset_secs;
use dereth_ui_screens::ctime::strftime_c;
use dereth_ui_screens::panels::{characterinfo, house};
