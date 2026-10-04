//! The `dat` tier's shell scenarios: notices and refusals, the character-creation wizard, the HUD
//! and its lamps, tooltips, text drawing and outlines, focus and scrollbars, lists, item cells,
//! dates and shipped strings. Each claim needs the shipped element tree, so it needs the retail
//! dats that tree is built from, and is read through `HeadlessClient::ui_snapshot` and driven
//! with `Player` gestures; some replay recordings through `Inbound`.
//!
//! Every scenario builds a whole headless client, and **this binary must run serially**: two
//! headless clients in one process share the UI request globals. `ALL` is this file's list for the
//! census in `census.rs`, so a scenario that is written and not listed shows up as a shortfall.

use dereth_testkit::adapters_shell::Hands;
use dereth_testkit::{ClientSpec, HeadlessClient, Player};

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "the_notice_bubble_clears_itself",
        &["notice.bubble.clears-itself-after-five-seconds"],
        the_notice_bubble_clears_itself,
    ),
    (
        "a_refusal_is_a_bubble_and_not_a_chat_line",
        &["notice.refusal.is-a-bubble-and-not-a-chat-line"],
        a_refusal_is_a_bubble_and_not_a_chat_line,
    ),
    (
        "every_wizard_page_lays_out",
        &["chargen.every-wizard-page-lays-out"],
        every_wizard_page_lays_out,
    ),
    (
        "the_house_tab_tells_a_houseless_character_they_have_no_house",
        &["house.tab.a-character-with-no-house-is-told-so"],
        the_house_tab_tells_a_houseless_character_they_have_no_house,
    ),
    (
        "the_houseless_line_is_written_once_and_into_that_pane_alone",
        &["house.tab.the-line-is-written-once-and-into-that-pane-alone"],
        the_houseless_line_is_written_once_and_into_that_pane_alone,
    ),
    (
        "a_use_of_the_recorded_locked_chest_says_so_in_the_strip",
        &["notice.locked-container.a-use-of-a-locked-one-says-so-in-the-strip"],
        a_use_of_the_recorded_locked_chest_says_so_in_the_strip,
    ),
    (
        "a_toggle_is_not_unticked_by_its_own_state",
        &["ui.states.a-toggle-is-not-unticked-by-its-own-change-of-state"],
        a_toggle_is_not_unticked_by_its_own_state,
    ),
    (
        "a_state_with_nothing_to_draw_runs_nothing",
        &["ui.states.a-state-declared-with-no-media-runs-nothing-and-an-undeclared-one-runs-the-base"],
        a_state_with_nothing_to_draw_runs_nothing,
    ),
    (
        "the_shards_refusal_reaches_the_strip_and_not_the_scrollback",
        &["notice.locked-container.the-shards-own-refusal-reaches-the-strip-and-not-the-scrollback"],
        the_shards_refusal_reaches_the_strip_and_not_the_scrollback,
    ),
    (
        "putting_the_same_screen_up_again_builds_it_fresh",
        &["ui.screen-rebuild.putting-the-same-screen-up-again-builds-it-fresh"],
        putting_the_same_screen_up_again_builds_it_fresh,
    ),
    (
        "a_wrapped_right_justified_caption_is_flush_with_its_box",
        &["ui.text.a-right-justified-caption-that-wraps-sits-flush-against-its-box"],
        a_wrapped_right_justified_caption_is_flush_with_its_box,
    ),
    (
        "a_wrapped_line_drops_the_width_of_its_break",
        &["ui.text.a-wrapped-line-does-not-carry-the-width-of-the-space-it-broke-at"],
        a_wrapped_line_drops_the_width_of_its_break,
    ),
    (
        "a_scrolling_pane_measures_itself_from_the_draw",
        &["ui.text.a-scrolling-pane-measures-what-it-holds-from-the-draw-itself"],
        a_scrolling_pane_measures_itself_from_the_draw,
    ),
    (
        "the_run_lock_says_so_in_the_message_window",
        &["notice.autorun.turning-the-run-lock-on-or-off-says-so-in-the-message-window"],
        the_run_lock_says_so_in_the_message_window,
    ),
    (
        "a_key_that_changes_nothing_says_nothing",
        &["notice.autorun.a-key-that-does-not-change-the-run-lock-says-nothing"],
        a_key_that_changes_nothing_says_nothing,
    ),
    (
        "the_autorun_line_reaches_the_strip_and_the_chat_windows_drop_it",
        &["notice.autorun.the-line-reaches-the-strip-and-the-chat-windows-drop-it"],
        the_autorun_line_reaches_the_strip_and_the_chat_windows_drop_it,
    ),
    (
        "pressing_a_row_draws_the_band_across_that_row",
        &["ui.list.pressing-a-row-draws-the-band-across-that-row-and-no-other"],
        pressing_a_row_draws_the_band_across_that_row,
    ),
    (
        "a_caption_too_tall_for_its_box_shows_its_first_line_at_the_top",
        &["ui.text.a-caption-too-tall-for-its-box-shows-its-first-line-at-the-top"],
        a_caption_too_tall_for_its_box_shows_its_first_line_at_the_top,
    ),
    (
        "a_caption_that_fits_is_still_centred",
        &["ui.text.a-caption-that-fits-and-asks-to-be-centred-is-still-centred"],
        a_caption_that_fits_is_still_centred,
    ),
    (
        "a_paragraph_too_tall_for_its_box_keeps_every_line",
        &["ui.text.a-paragraph-too-tall-for-its-box-keeps-every-line-and-scrolls"],
        a_paragraph_too_tall_for_its_box_keeps_every_line,
    ),
    (
        "a_recall_broken_by_moving_says_so_on_the_strip",
        &["notice.failure.a-recall-broken-by-moving-says-so-on-the-strip"],
        a_recall_broken_by_moving_says_so_on_the_strip,
    ),
    (
        "every_refusal_draws_its_own_line_on_its_own_surface",
        &["notice.failure.every-refusal-the-shard-can-send-draws-its-own-line-on-its-own-surface"],
        every_refusal_draws_its_own_line_on_its_own_surface,
    ),
    (
        "a_refusal_carrying_the_shards_word_puts_it_in_the_line",
        &["notice.failure.a-refusal-carrying-the-shards-own-word-puts-it-in-the-line"],
        a_refusal_carrying_the_shards_word_puts_it_in_the_line,
    ),
    (
        "the_caret_flashes_at_the_players_own_desktop_interval",
        &["ui.caret.flashes-at-the-interval-the-player-set-for-their-desktop"],
        the_caret_flashes_at_the_players_own_desktop_interval,
    ),
    (
        "a_jump_shows_one_power_bar_and_never_the_other",
        &["hud.power-bar.a-jump-shows-the-one-bar-the-client-listens-with-and-never-the-other"],
        a_jump_shows_one_power_bar_and_never_the_other,
    ),
    (
        "both_raise_buttons_on_both_pages_put_the_request_on_the_wire",
        &["character-page.raise.both-buttons-on-both-pages-put-the-request-on-the-wire"],
        both_raise_buttons_on_both_pages_put_the_request_on_the_wire,
    ),
    (
        "the_first_press_on_the_vitals_bar_changes_nothing_on_screen",
        &["hud.vitals.the-first-press-changes-nothing-on-screen-and-the-second-hides-the-numbers"],
        the_first_press_on_the_vitals_bar_changes_nothing_on_screen,
    ),
    (
        "a_recorded_book_opens_the_reader_and_shows_its_first_page",
        &["reader.book.a-recorded-book-opens-the-reader-and-shows-its-first-page"],
        a_recorded_book_opens_the_reader_and_shows_its_first_page,
    ),
    (
        "paging_a_book_moves_the_page_and_greys_what_cannot_be_pressed",
        &["reader.book.paging-moves-the-page-and-greys-the-control-that-cannot-be-pressed"],
        paging_a_book_moves_the_page_and_greys_what_cannot_be_pressed,
    ),
    (
        "a_refused_portal_says_so_in_the_chat_log",
        &["notice.failure.a-refused-portal-says-so-in-the-chat-log-in-its-own-colour"],
        a_refused_portal_says_so_in_the_chat_log,
    ),
    (
        "a_press_on_the_vitals_bar_flips_it_between_its_two_presentations",
        &["hud.vitals.a-press-on-the-bar-flips-it-between-its-two-presentations"],
        a_press_on_the_vitals_bar_flips_it_between_its_two_presentations,
    ),
    (
        "resting_the_pointer_on_something_draws_a_box_with_words_in_it",
        &["tooltip.resting-the-pointer-on-something-draws-a-box-with-words-in-it"],
        resting_the_pointer_on_something_draws_a_box_with_words_in_it,
    ),
    (
        "the_words_in_a_tooltip_are_the_ones_that_element_was_given",
        &["tooltip.the-words-are-the-ones-that-element-was-given"],
        the_words_in_a_tooltip_are_the_ones_that_element_was_given,
    ),
    (
        "a_label_too_long_for_its_box_can_be_read_and_catches_the_pointer",
        &["tooltip.a-label-too-long-for-its-box-can-be-read-and-catches-the-pointer"],
        a_label_too_long_for_its_box_can_be_read_and_catches_the_pointer,
    ),
    (
        "setting_the_clock_does_not_change_the_lamp",
        &["hud.link-lamp.the-shard-setting-the-clock-does-not-change-it-and-nor-does-a-quiet-link"],
        setting_the_clock_does_not_change_the_lamp,
    ),
    (
        "a_real_failure_takes_the_player_off_the_world",
        &["hud.link-lamp.a-real-failure-puts-the-player-off-the-world-rather-than-reddening-a-lamp"],
        a_real_failure_takes_the_player_off_the_world,
    ),
    (
        "each_support_button_opens_its_in_game_form",
        &["options.support.each-support-button-opens-its-in-game-form"],
        each_support_button_opens_its_in_game_form,
    ),
    (
        "a_browser_that_will_not_open_says_so_in_a_box_with_the_address_in_it",
        &["options.support.a-browser-that-will-not-open-says-so-in-a-box-with-the-address-in-it"],
        a_browser_that_will_not_open_says_so_in_a_box_with_the_address_in_it,
    ),
    (
        "the_urgent_assistance_report_goes_out_on_the_help_channel",
        &["urgent-assistance.send.the-report-goes-out-on-the-help-channel"],
        the_urgent_assistance_report_goes_out_on_the_help_channel,
    ),
    (
        "the_refusal_for_a_typed_command_does_not_apply_to_the_window",
        &["urgent-assistance.send.the-refusal-that-applies-to-a-typed-command-does-not-apply-here"],
        the_refusal_for_a_typed_command_does_not_apply_to_the_window,
    ),
    (
        "the_refusal_strip_is_drawn_with_an_outline_and_the_chat_log_is_not",
        &["ui.text.outline.the-refusal-strip-is-drawn-with-one-and-the-chat-log-is-not"],
        the_refusal_strip_is_drawn_with_an_outline_and_the_chat_log_is_not,
    ),
    (
        "every_element_that_asks_for_an_outline_is_drawn_with_one",
        &["ui.text.outline.every-element-that-asks-for-one-is-drawn-with-one-and-no-other-is"],
        every_element_that_asks_for_an_outline_is_drawn_with_one,
    ),
    (
        "the_outline_comes_from_the_fonts_own_heavier_sheet",
        &["ui.text.outline.it-comes-from-the-fonts-own-heavier-sheet-and-fits-inside-it"],
        the_outline_comes_from_the_fonts_own_heavier_sheet,
    ),
    (
        "every_element_of_a_live_screen_is_sized_from_its_own_box",
        &["ui.surface.every-element-of-a-live-screen-is-sized-from-its-own-box-by-default"],
        every_element_of_a_live_screen_is_sized_from_its_own_box,
    ),
    (
        "an_element_owning_no_surface_is_stepped_over",
        &["ui.surface.an-element-owning-none-is-stepped-over-and-the-answer-is-its-owners"],
        an_element_owning_no_surface_is_stepped_over,
    ),
    (
        "a_colour_picker_is_sized_from_its_own_box_whatever_it_asks_for",
        &["ui.surface.a-colour-picker-is-sized-from-its-own-box-whatever-the-layout-asks-for"],
        a_colour_picker_is_sized_from_its_own_box_whatever_it_asks_for,
    ),
    (
        "a_shipped_root_that_owns_no_surface_is_left_without_one",
        &["ui.surface.a-shipped-root-that-owns-none-is-left-without-one-though-the-maker-asks"],
        a_shipped_root_that_owns_no_surface_is_left_without_one,
    ),
    (
        "a_press_in_a_box_takes_the_keyboard_and_keeps_it",
        &["ui.focus.a-press-in-a-box-takes-the-keyboard-and-keeps-it"],
        a_press_in_a_box_takes_the_keyboard_and_keeps_it,
    ),
    (
        "what_is_typed_reaches_the_box_holding_the_keyboard",
        &["ui.focus.what-is-typed-reaches-the-box-holding-the-keyboard-and-stops-when-it-lets-go"],
        what_is_typed_reaches_the_box_holding_the_keyboard,
    ),
    (
        "a_log_takes_the_keyboard_and_none_of_what_is_typed",
        &["ui.focus.a-log-takes-the-keyboard-and-none-of-what-is-typed"],
        a_log_takes_the_keyboard_and_none_of_what_is_typed,
    ),
    (
        "the_name_field_of_the_wizard_takes_a_typed_name",
        &["ui.focus.the-name-field-of-the-wizard-takes-a-typed-name"],
        the_name_field_of_the_wizard_takes_a_typed_name,
    ),
    (
        "dragging_the_stack_slider_moves_the_thumb_and_the_quantity_follows",
        &["ui.scrollbar.dragging-the-stack-slider-moves-the-thumb-and-the-quantity-follows"],
        dragging_the_stack_slider_moves_the_thumb_and_the_quantity_follows,
    ),
    (
        "the_chat_bars_thumb_is_sized_and_placed_inside_its_track",
        &["ui.scrollbar.the-chat-bars-thumb-is-sized-and-placed-inside-its-track"],
        the_chat_bars_thumb_is_sized_and_placed_inside_its_track,
    ),
    (
        "the_highlight_comes_down_by_itself_a_quarter_second_later",
        &["ui.selection-strip.the-highlight-comes-down-by-itself-a-quarter-second-later"],
        the_highlight_comes_down_by_itself_a_quarter_second_later,
    ),
    (
        "the_meters_start_down_and_a_creatures_answer_brings_its_bar_up",
        &["ui.selection-strip.the-meters-start-down-and-a-creatures-answer-brings-its-bar-up"],
        the_meters_start_down_and_a_creatures_answer_brings_its_bar_up,
    ),
    (
        "empty_a_single_item_and_a_stack_are_three_different_strips",
        &["ui.selection-strip.empty-a-single-item-and-a-stack-are-three-different-strips"],
        empty_a_single_item_and_a_stack_are_three_different_strips,
    ),
    (
        "the_players_own_cell_draws_a_backpack_and_not_a_second_backdrop",
        &["ui.item-cell.the-players-own-cell-draws-a-backpack-and-not-a-second-backdrop"],
        the_players_own_cell_draws_a_backpack_and_not_a_second_backdrop,
    ),
    (
        "what_follows_the_cursor_is_the_icon_alone",
        &["ui.item-cell.what-follows-the-cursor-is-the-icon-alone-and-not-the-lifted-cell"],
        what_follows_the_cursor_is_the_icon_alone,
    ),
    (
        "dropping_a_thing_back_on_its_own_slot_sends_nothing_and_ghosts_nothing",
        &["ui.item-cell.dropping-a-thing-back-on-its-own-slot-sends-nothing-and-ghosts-nothing"],
        dropping_a_thing_back_on_its_own_slot_sends_nothing_and_ghosts_nothing,
    ),
    (
        "the_way_out_of_the_strip_raises_the_question_about_ending_the_session",
        &["hud.lamp-row.the-way-out-of-the-strip-raises-the-question-about-ending-the-session"],
        the_way_out_of_the_strip_raises_the_question_about_ending_the_session,
    ),
    (
        "every_lamp_opens_its_own_panel",
        &["hud.lamp-row.every-lamp-opens-its-own-panel-and-leaves-the-other-lamps-panels-down"],
        every_lamp_opens_its_own_panel,
    ),
    (
        "a_button_of_the_strip_with_no_action_of_its_own_opens_nothing",
        &["hud.lamp-row.a-button-of-the-strip-with-no-action-of-its-own-opens-nothing"],
        a_button_of_the_strip_with_no_action_of_its_own_opens_nothing,
    ),
    (
        "the_burden_lamp_crosses_both_thresholds_on_the_characters_own_capacity",
        &["hud.lamp-row.the-burden-lamp-crosses-both-thresholds-on-the-characters-own-capacity"],
        the_burden_lamp_crosses_both_thresholds_on_the_characters_own_capacity,
    ),
    (
        "a_buff_lights_one_lamp_and_a_debuff_the_other",
        &["hud.lamp-row.a-buff-lights-one-lamp-a-debuff-the-other-and-a-purge-puts-both-out"],
        a_buff_lights_one_lamp_and_a_debuff_the_other,
    ),
    (
        "the_vitae_lamp_lights_for_a_penalty_and_not_for_a_multiplier_of_one",
        &["hud.lamp-row.the-vitae-lamp-lights-for-a-penalty-and-not-for-a-multiplier-of-one"],
        the_vitae_lamp_lights_for_a_penalty_and_not_for_a_multiplier_of_one,
    ),
    (
        "the_link_lamp_comes_up_good_and_falls_to_lost",
        &["hud.link-lamp.it-comes-up-good-and-falls-to-lost-when-nothing-is-heard-at-all"],
        the_link_lamp_comes_up_good_and_falls_to_lost,
    ),
    (
        "a_swing_with_nothing_selected_says_so_on_the_strip",
        &["notice.a-swing-with-nothing-selected-says-so-on-the-strip"],
        a_swing_with_nothing_selected_says_so_on_the_strip,
    ),
    (
        "the_character_sheets_born_line_is_in_the_machines_own_time",
        &["dates.the-character-sheets-born-line-is-in-the-machines-own-time-and-that-shape"],
        the_character_sheets_born_line_is_in_the_machines_own_time,
    ),
    (
        "every_date_on_the_house_pane_is_in_the_machines_own_time",
        &["dates.every-date-on-the-house-pane-is-in-the-machines-own-time-and-that-shape"],
        every_date_on_the_house_pane_is_in_the_machines_own_time,
    ),
    (
        "the_line_saying_when_another_house_may_be_bought_is_in_the_machines_own_time",
        &["dates.the-line-saying-when-another-house-may-be-bought-is-in-the-machines-own-time"],
        the_line_saying_when_another_house_may_be_bought_is_in_the_machines_own_time,
    ),
    (
        "the_chat_stamp_carries_the_machines_own_offset",
        &["dates.the-chat-stamp-carries-the-machines-own-offset-and-not-a-constant"],
        the_chat_stamp_carries_the_machines_own_offset,
    ),
    (
        "the_ban_expiry_keeps_its_shape_and_is_in_the_machines_own_time",
        &["dates.the-ban-expiry-keeps-the-other-shape-and-is-in-the-machines-own-time"],
        the_ban_expiry_keeps_its_shape_and_is_in_the_machines_own_time,
    ),
    (
        "a_run_of_spaces_in_a_shipped_line_is_drawn_as_one",
        &["strings.a-run-of-spaces-in-a-shipped-line-is-drawn-as-one-and-nothing-else-moves"],
        a_run_of_spaces_in_a_shipped_line_is_drawn_as_one,
    ),
    (
        "the_one_line_that_asks_to_keep_its_spaces_keeps_them",
        &["strings.the-one-line-that-asks-to-keep-its-spaces-keeps-them-and-loses-its-asking"],
        the_one_line_that_asks_to_keep_its_spaces_keeps_them,
    ),
    (
        "a_value_the_caller_did_not_give_is_nothing_rather_than_a_complaint",
        &["strings.a-value-the-caller-did-not-give-is-nothing-rather-than-a-complaint"],
        a_value_the_caller_did_not_give_is_nothing_rather_than_a_complaint,
    ),
    (
        "a_line_places_the_values_by_name_and_not_by_the_order_given",
        &["strings.a-row-places-the-values-by-name-so-the-order-they-are-given-in-is-invisible"],
        a_line_places_the_values_by_name_and_not_by_the_order_given,
    ),
    (
        "the_key_binding_prompt_loses_both_of_its_double_spaces",
        &["strings.the-key-binding-prompt-loses-both-of-its-double-spaces"],
        the_key_binding_prompt_loses_both_of_its_double_spaces,
    ),
    (
        "the_pane_draws_the_tidied_line_and_not_the_shipped_double_space",
        &["strings.the-pane-draws-the-tidied-line-and-not-the-shipped-double-space"],
        the_pane_draws_the_tidied_line_and_not_the_shipped_double_space,
    ),
    (
        "every_recorded_refusal_and_nothing_else_reaches_the_strip",
        &["notice.refusal.every-recorded-refusal-and-nothing-else-reaches-the-strip"],
        every_recorded_refusal_and_nothing_else_reaches_the_strip,
    ),
    (
        "a_refusal_about_something_the_player_asked_for_names_it_and_says_why",
        &["notice.refusal.a-refusal-about-something-the-player-asked-for-names-it-and-says-why"],
        a_refusal_about_something_the_player_asked_for_names_it_and_says_why,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

// -------------------------------------------------------------------------------------------
// 15. notice.bubble.clears-itself-after-five-seconds
// -------------------------------------------------------------------------------------------

/// A notice bubble goes away on its own, five seconds later, with nobody at the keyboard.
///
/// The other direction is checked too -- a second in, the bubble is still there -- without which
/// this would hold on a strip that deleted everything at once. The strip is read off the
/// **element tree** as well as off the model, which is what makes "it went away" a drawn result.
pub fn the_notice_bubble_clears_itself() {
    use dereth_client::app::HEADLESS_STEP;
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

#[test]
fn scenario_the_notice_bubble_clears_itself() {
    scenario("the_notice_bubble_clears_itself");
}

// -------------------------------------------------------------------------------------------
// 16. notice.refusal.is-a-bubble-and-not-a-chat-line
// -------------------------------------------------------------------------------------------

/// A refusal the client composes is a bubble, and the chat scrollback never sees it.
///
/// It reads the seam counters on both sides of the queue, and the words back out of the **element
/// tree** and out of the chat log's own element rather than off the model. A claim about what the
/// player sees that is only ever read off a counter is a claim about a counter.
pub fn a_refusal_is_a_bubble_and_not_a_chat_line() {
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

#[test]
fn scenario_a_refusal_is_a_bubble_and_not_a_chat_line() {
    scenario("a_refusal_is_a_bubble_and_not_a_chat_line");
}

// -------------------------------------------------------------------------------------------
// 17. chargen.every-wizard-page-lays-out
// -------------------------------------------------------------------------------------------

/// Every page of the character-creation wizard is reachable and lays out.
pub fn every_wizard_page_lays_out() {
    use dereth_ui_screens::screens::chargen::{CharGenScreen, EcgProgress};

    let mut c = HeadlessClient::new(ClientSpec::screen(dereth_ui::framework::mode::CHAR_GEN, 8));
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .and_then(|u| u.flow.current_mode()),
        Some(dereth_ui::framework::mode::CHAR_GEN),
        "the wizard is the screen this scenario is about"
    );
    {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("shell");
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        let wizard = any.downcast_mut::<CharGenScreen>().expect("the wizard");
        assert!(
            wizard.tables.is_some(),
            "the host handed the wizard its char-gen tables"
        );
    }

    // Each page in turn, through its own tab, and what the tab produced.
    let mut laid_out: Vec<(String, usize)> = Vec::new();
    for page in EcgProgress::PAGES {
        let tab = page.select_button().expect("a real page has a tab");
        {
            let app = c.app_mut();
            let shell = app.ui().expect("shell");
            let root = shell.flow.current().expect("a screen").roots()[0];
            let h = shell
                .ui
                .get_child_recursive(root, tab)
                .unwrap_or_else(|| panic!("{page:?}'s tab is in the shipped layout"));
            let shell = app.ui_mut().expect("shell");
            shell.ui.broadcast_element_message(
                h,
                dereth_ui::msg::element::id::BUTTON_CLICKED,
                7,
                0,
            );
        }
        c.tick(4);
        let elements = {
            let shell = c.view().expect_app().ui().expect("shell");
            let root = shell.flow.current().expect("a screen").roots()[0];
            let mut all = Vec::new();
            walk(&shell.ui, root, &mut all);
            all.len()
        };
        println!("chargen {page:?}: {elements} elements");
        laid_out.push((format!("{page:?}"), elements));
    }

    c.assert_behaviour("chargen.every-wizard-page-lays-out", move |_v| {
        laid_out.len() == EcgProgress::PAGES.len() && laid_out.iter().all(|(_, n)| *n > 1)
    });
    c.shutdown();
}

fn walk(ui: &dereth_ui::UiSystem, h: dereth_ui::ElemHandle, out: &mut Vec<dereth_ui::ElemHandle>) {
    out.push(h);
    for c in ui.children(h) {
        walk(ui, c, out);
    }
}

#[test]
fn scenario_every_wizard_page_lays_out() {
    scenario("every_wizard_page_lays_out");
}

// ---------------------------------------------------------------------------------------------
// house.tab.*
//
// Read through `HeadlessClient::ui_snapshot`, which walks to the gameplay screen, finds an element
// by id, reads its visibility and collects the text off a list box's children; what is left below
// is the gesture and the claim.
// ---------------------------------------------------------------------------------------------

/// Open one page of the toolbar's panel stack the way the toolbar button does, through the page's
/// own registered id read off the live stack.
fn open_page(c: &mut HeadlessClient, page: dereth_ui::ElementId) {
    {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the shell is up");
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a screen is current");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen is current");
        let panel_id = gameplay
            .panels
            .pages
            .iter()
            .find(|p| p.element == page)
            .map(|p| p.panel_id)
            .unwrap_or_else(|| panic!("{page:?} is one of the shipped registered pages"));
        gameplay.recv_set_panel_visibility(ui, panel_id, true);
    }
    c.tick(3);
}

/// **The gate.** Open the map page, click the House tab, read the pane.
pub fn the_house_tab_tells_a_houseless_character_they_have_no_house() {
    use dereth_ui_screens::panels::house;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));

    // The shipped layout opens that page on the *map* sub-panel, so the House pane starts down.
    assert!(
        !c.ui_snapshot().is_visible(house::PANEL),
        "the map sub-panel is the one the shipped page opens on"
    );

    open_page(&mut c, house::PAGE);
    c.when(Player::click(house::TAB));

    let after = c.ui_snapshot();
    after.assert_visible(house::PANEL);
    let lines: Vec<String> = after
        .rows_of(house::TEXT_BOX)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let pane = after.tree_text(Some(house::PANEL));
    let displays = c.view().expect_app().hud().panels.house.displays;
    let owns = c.view().expect_app().hud().panels.house.owns_house;
    let bound = c.view().expect_app().hud().panels.house.fully_bound();

    // The whole pane, and not four fields of it: a row that appeared, one that moved and one that
    // lost its text are three different changes and this is where all three show up.
    after.assert_tree("house_pane_with_no_house", Some(house::PANEL));

    c.assert_behaviour(
        "house.tab.a-character-with-no-house-is-told-so",
        move |_| {
            lines == vec![house::NO_HOUSE.to_owned()]
                && displays == 1
                && !owns
                && bound
                && pane.lines().count() > 1
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_house_tab_tells_a_houseless_character_they_have_no_house() {
    scenario("the_house_tab_tells_a_houseless_character_they_have_no_house");
}

/// The other half, so the gate above cannot be satisfied by a pane that writes that line into
/// everything: the map tab of the same page shows no such row, and the House pane holds one row
/// and not one per frame.
pub fn the_houseless_line_is_written_once_and_into_that_pane_alone() {
    use dereth_ui_screens::panels::house;

    /// The map sub-panel of the same page -- the one the shipped layout opens on.
    const MAP_PANEL: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_01F6);

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    open_page(&mut c, house::PAGE);
    // Several frames with the map tab up: the House pane must not add a row per frame.
    c.tick(8);
    let map_lines: Vec<String> = c
        .ui_snapshot()
        .rows_of(MAP_PANEL)
        .into_iter()
        .map(str::to_owned)
        .collect();

    c.when(Player::click(house::TAB));
    c.tick(8);

    let after = c.ui_snapshot();
    let lines: Vec<String> = after
        .rows_of(house::TEXT_BOX)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let displays = c.view().expect_app().hud().panels.house.displays;

    c.assert_behaviour(
        "house.tab.the-line-is-written-once-and-into-that-pane-alone",
        move |_| {
            lines.len() == 1
                && displays == 1
                && !map_lines.iter().any(|s| s.contains("own a house"))
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_houseless_line_is_written_once_and_into_that_pane_alone() {
    scenario("the_houseless_line_is_written_once_and_into_that_pane_alone");
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
pub fn a_use_of_the_recorded_locked_chest_says_so_in_the_strip() {
    use dereth_client_model::weenie::{bitfield, item_type};
    use dereth_protocol::types::PublicWeenieDesc;
    use dereth_ui_screens::hud::speech_bubbles::LIST_BOX;

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

#[test]
fn scenario_a_use_of_the_recorded_locked_chest_says_so_in_the_strip() {
    scenario("a_use_of_the_recorded_locked_chest_says_so_in_the_strip");
}

// ---------------------------------------------------------------------------------------------
// ui.states.*
//
// The element below is authored here on purpose: **no shipped element lines the three facts up at
// once** -- base artwork that ends by asking for a state, alternate states declared with nothing
// to draw, and a tick box. Each of the three is a shape the shipped layouts do
// use separately, so a scenario that waited for one that had all three would never run.
//
// It is a `dat`-tier scenario because it stands beside its sibling above; it opens no data file of
// its own, and the layout it drives is the twelve lines below.
// ---------------------------------------------------------------------------------------------

/// The authored tick box, and the attributes the widget reads.
const TOGGLE: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0900);
/// The button's "toggled" and "toggle button" attributes.
const ATTR_TOGGLED: u32 = 0x0E;
const ATTR_TOGGLE_BUTTON: u32 = 0x0B;

/// No data file at all: every element here is built from a description written above.
#[derive(Debug)]
struct NoAssets;

impl dereth_primitives::AssetSource for NoAssets {
    fn read(
        &self,
        id: dereth_primitives::DataId,
    ) -> Result<Vec<u8>, dereth_primitives::AssetError> {
        Err(dereth_primitives::AssetError::NotFound(id))
    }
    fn exists(&self, _: dereth_primitives::DataId) -> bool {
        false
    }
    fn iter_type(
        &self,
        _: dereth_primitives::DataType,
    ) -> Box<dyn Iterator<Item = dereth_primitives::DataId> + '_> {
        Box::new(std::iter::empty())
    }
}

/// One tick box carrying artwork of its own that ends by asking for a state, and declaring the
/// ticked state with nothing to draw.
fn a_toggle_whose_artwork_asks_for_a_state() -> dereth_ui::UiSystem {
    use dereth_ui::desc::{
        incorporation, ElementDesc, LayoutDesc, MediaDesc, MediaFields, StateDesc,
    };
    use dereth_ui::{PropertyValue, StateId, UiSystem};

    let asks_for_state_one = MediaDesc {
        media_type: 10,
        type_echo_ok: true,
        fields: MediaFields::State {
            state_id: 1,
            probability: 1.0,
        },
    };
    let mut base = StateDesc {
        incorporation: incorporation::LEGACY_ALL_GEOMETRY,
        x: 0,
        y: 0,
        width: 40,
        height: 20,
        media: vec![asks_for_state_one],
        ..StateDesc::default()
    };
    base.properties
        .set(ATTR_TOGGLE_BUTTON, PropertyValue::Bool(true));
    base.properties
        .set(ATTR_TOGGLED, PropertyValue::Bool(false));
    let declared = |id: u32| {
        (
            StateId(id),
            StateDesc {
                state_id: StateId(id),
                media: Vec::new(),
                ..StateDesc::default()
            },
        )
    };
    let desc = ElementDesc {
        base,
        element_id: TOGGLE,
        ty: dereth_ui::factory::ty::BUTTON,
        default_state: StateId(1),
        states: [declared(1), declared(3), declared(6)]
            .into_iter()
            .collect(),
        ..ElementDesc::default()
    };
    let layout = LayoutDesc {
        did: dereth_primitives::DataId(0x2100_0900),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((TOGGLE, desc)).collect(),
    };
    let mut ui = UiSystem::new((800, 600));
    let d = layout
        .access_element(TOGGLE)
        .cloned()
        .expect("the one element");
    let h = ui
        .create_element_recursive_from_full_desc(&NoAssets, &layout, &d)
        .expect("nothing is inherited")
        .expect("the button registers");
    let root = ui.root();
    ui.set_parent(h, Some(root));
    ui.initialize_tree(h);
    ui
}

fn the_toggle(ui: &dereth_ui::UiSystem) -> dereth_ui::ElemHandle {
    ui.get_child(ui.root(), TOGGLE)
        .expect("the button is under the root")
}

fn is_ticked(ui: &dereth_ui::UiSystem, h: dereth_ui::ElemHandle) -> Option<bool> {
    ui.node(h)
        .and_then(|n| n.merged_properties().get_bool(ATTR_TOGGLED))
}

/// A tick box put into the state that means ticked is not unticked by its own artwork.
pub fn a_toggle_is_not_unticked_by_its_own_state() {
    use dereth_ui::StateId;

    let mut ui = a_toggle_whose_artwork_asks_for_a_state();
    let h = the_toggle(&ui);
    let starts_unticked = is_ticked(&ui, h) == Some(false);

    // The state a tick box is put into when it is ticked, which is what letting go of the button
    // over it reaches.
    ui.set_state(h, StateId(6));
    let survived = is_ticked(&ui, h) == Some(true);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.states.a-toggle-is-not-unticked-by-its-own-change-of-state",
        move |_| starts_unticked && survived,
    );
}

#[test]
fn scenario_a_toggle_is_not_unticked_by_its_own_state() {
    scenario("a_toggle_is_not_unticked_by_its_own_state");
}

/// A state declared with nothing to draw runs nothing; an undeclared one runs the element's own.
pub fn a_state_with_nothing_to_draw_runs_nothing() {
    use dereth_ui::StateId;

    let mut ui = a_toggle_whose_artwork_asks_for_a_state();
    let h = the_toggle(&ui);

    // A state the layout declares and gives nothing to draw: nothing runs.
    ui.media_effects.clear();
    ui.set_state(h, StateId(3));
    let nothing_ran = ui.media_effects.is_empty();

    // A state the layout does not declare: the element's own artwork is what runs, and it lands --
    // the artwork asks for a state and the element goes there, which is the whole reason the
    // looser rule was dangerous.
    ui.media_effects.clear();
    ui.set_state(h, StateId(0x1234));
    let base_ran = ui.media_effects.iter().any(
        |(_, e)| matches!(e, dereth_ui::media::MediaEffect::SetState { id } if *id == StateId(1)),
    );
    let landed = ui.node(h).map(|n| n.state) == Some(StateId(1));

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.states.a-state-declared-with-no-media-runs-nothing-and-an-undeclared-one-runs-the-base",
        move |_| nothing_ran && base_ran && landed,
    );
}

#[test]
fn scenario_a_state_with_nothing_to_draw_runs_nothing() {
    scenario("a_state_with_nothing_to_draw_runs_nothing");
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
pub fn the_shards_refusal_reaches_the_strip_and_not_the_scrollback() {
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

#[test]
fn scenario_the_shards_refusal_reaches_the_strip_and_not_the_scrollback() {
    scenario("the_shards_refusal_reaches_the_strip_and_not_the_scrollback");
}

// ---------------------------------------------------------------------------------------------
// ui.screen-rebuild.*
// ---------------------------------------------------------------------------------------------

/// Putting the same screen up again builds it fresh, and what the old one was told is gone.
pub fn putting_the_same_screen_up_again_builds_it_fresh() {
    use dereth_client_model::combat::PowerBarMode;
    use dereth_ui_screens::bind::{attr, attr_float};

    /// The bars the shipped tree carries, and the meter inside each.
    fn bars(c: &HeadlessClient) -> Vec<(dereth_ui::ElemHandle, dereth_ui::ElemHandle)> {
        c.view()
            .expect_app()
            .hud()
            .panels
            .power_bar
            .bars
            .iter()
            .map(|b| {
                (
                    b.element.expect("the shipped bar"),
                    b.bound.get("bar").expect("the shipped meter"),
                )
            })
            .collect()
    }

    /// What each bar is showing.
    fn showing(c: &HeadlessClient) -> Vec<(bool, dereth_ui::StateId, Option<f32>)> {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        bars(c)
            .into_iter()
            .map(|(element, meter)| {
                let n = ui.node(element).expect("the bar is live");
                (
                    n.region.flags.visible,
                    n.state,
                    attr_float(ui, meter, attr::METER_LEVEL),
                )
            })
            .collect()
    }

    /// The one bar that is on the notice lists -- the only one a message can reach.
    fn subscriber(c: &HeadlessClient) -> Vec<(bool, dereth_ui::StateId, Option<f32>)> {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        app.hud()
            .panels
            .power_bar
            .bars
            .iter()
            .filter(|b| b.registered)
            .map(|b| {
                let element = b.element.expect("the shipped bar");
                let meter = b.bound.get("bar").expect("the shipped meter");
                let n = ui.node(element).expect("the bar is live");
                (
                    n.region.flags.visible,
                    n.state,
                    attr_float(ui, meter, attr::METER_LEVEL),
                )
            })
            .collect()
    }

    fn charge(c: &mut HeadlessClient, mode: PowerBarMode, level: f32) {
        let combat = &mut c.objects_mut().world.combat;
        combat.begin_power_bar(mode, false, 0);
        combat.set_power_bar_level(level);
    }

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // The state the shipped layout builds, read before anything is told to any of it.
    let pristine = showing(&c);
    let old_bars = bars(&c);
    let two_bars = old_bars.len() == 2;

    // The positive control: the old widgets really do take a charge.
    charge(&mut c, PowerBarMode::AdvancedCombat, 0.25);
    c.tick(1);
    let old_took_it = subscriber(&c)
        .iter()
        .all(|(v, _, l)| *v && *l == Some(0.25));

    // Now a charge sent while the old widgets are still there, and the same screen queued again.
    let switches = c.view().expect_app().ui().expect("shell").flow.switches;
    charge(&mut c, PowerBarMode::AdvancedCombat, 0.9);
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(1);

    let rebuilt = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("shell");
        shell.flow.current_mode() == Some(dereth_ui::framework::mode::GAME_PLAY)
            && shell.flow.switches == switches + 1
            // Every one of the old elements is gone rather than reused.
            && old_bars.iter().all(|(b, m)| shell.ui.node(*b).is_none() && shell.ui.node(*m).is_none())
    };
    let new_bars = bars(&c);
    let rebound = new_bars != old_bars;
    // The new widgets came up in the state the layout gives them, not in the state the old ones
    // were left in: the charge sent while the old ones existed does not reach them.
    let fresh = showing(&c) == pristine;

    // An unchanged frame neither rebuilds again nor replays what was dropped.
    c.tick(1);
    let settled = bars(&c) == new_bars
        && showing(&c) == pristine
        && c.view().expect_app().ui().expect("shell").flow.switches == switches + 1;

    // ...and the new widgets take a new charge.
    charge(&mut c, PowerBarMode::AdvancedCombat, 0.5);
    c.tick(1);
    let new_took_it = subscriber(&c).iter().all(|(v, _, l)| *v && *l == Some(0.5));

    c.assert_behaviour(
        "ui.screen-rebuild.putting-the-same-screen-up-again-builds-it-fresh",
        move |_| two_bars && old_took_it && rebuilt && rebound && fresh && settled && new_took_it,
    );
    c.shutdown();
}

#[test]
fn scenario_putting_the_same_screen_up_again_builds_it_fresh() {
    scenario("putting_the_same_screen_up_again_builds_it_fresh");
}

// ---------------------------------------------------------------------------------------------
// ui.text.*
//
// A caption with a trailing space ("Environment Texture " with a gap after it), and the four
// panes that measure themselves without being told to.
// ---------------------------------------------------------------------------------------------

/// A store over the retail data files, as an asset source.
#[derive(Debug)]
struct RetailAssets(std::sync::Arc<dereth_dat::RetailDatStore>);

impl dereth_primitives::AssetSource for RetailAssets {
    fn read(
        &self,
        id: dereth_primitives::DataId,
    ) -> Result<Vec<u8>, dereth_primitives::AssetError> {
        self.0.read(id)
    }
    fn exists(&self, id: dereth_primitives::DataId) -> bool {
        self.0.exists(id)
    }
    fn iter_type(
        &self,
        kind: dereth_primitives::DataType,
    ) -> Box<dyn Iterator<Item = dereth_primitives::DataId> + '_> {
        self.0.iter_type(kind)
    }
}

/// A UI over the shipped data files, with the two resolvers that make a shipped caption real text
/// measured in the shipped font. Without them every caption reads empty and a scenario about the
/// width of a line would be measuring its own gap.
fn a_shipped_ui() -> (
    dereth_ui::UiSystem,
    std::sync::Arc<dereth_dat::RetailDatStore>,
) {
    use std::rc::Rc;
    use std::sync::Arc;

    use dereth_primitives::AssetSource as _;

    let dir = dereth_dat::testing::dat_dir();
    let store = Arc::new(
        dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail data files are the oracle"),
    );
    let id = dereth_primitives::DataId(0x3900_0001);
    let master = <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(
        id,
        &store.read(id).expect("the master property table"),
    )
    .expect("it decodes");
    let mut ui = dereth_ui::UiSystem::new((800, 600));
    ui.install_master(&master);
    ui.strings = Some(Rc::new(dereth_client::ui_draw::DatStringResolver::new(
        Arc::clone(&store),
    )));
    ui.fonts = Some(Rc::new(dereth_client::ui_draw::DatFontProvider::new(
        Arc::clone(&store),
    )));
    let mut flow = dereth_ui::UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let s = Rc::new(RetailAssets(Arc::clone(&store)));
    let resolver = Rc::new(
        dereth_ui::framework::DidMapperResolver::load_via_master(s.as_ref()).expect("the mapper"),
    );
    dereth_ui_screens::env::install(&mut ui, s, resolver);
    (ui, store)
}

fn walk_tree(
    ui: &dereth_ui::UiSystem,
    h: dereth_ui::ElemHandle,
    out: &mut Vec<dereth_ui::ElemHandle>,
) {
    out.push(h);
    for c in ui.children(h) {
        walk_tree(ui, c, out);
    }
}

/// A right-justified caption that wraps sits flush against its box.
pub fn a_wrapped_right_justified_caption_is_flush_with_its_box() {
    use dereth_ui::framework::Screen as _;

    /// The caption inside the shipped option-row template.
    const MENU_LABEL: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0223);
    /// The caption with the trailing space, found by its own words rather than by a handle.
    const CAPTION: &str = "Environment Texture Detail";

    let (mut ui, _store) = a_shipped_ui();
    let mut screen = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    ui.requests.clear();

    let label = {
        let root = screen.root().expect("a root");
        let mut all = Vec::new();
        walk_tree(&ui, root, &mut all);
        let candidates: Vec<dereth_ui::ElemHandle> = all
            .into_iter()
            .filter(|h| ui.node(*h).is_some_and(|n| n.element_id() == MENU_LABEL))
            .collect();
        candidates
            .into_iter()
            .find(|h| {
                ui.text_element_mut(*h)
                    .is_some_and(|t| t.glyphs.inq_text(false) == CAPTION)
            })
            .expect("one option row's caption reads its own words")
    };

    let screen_box = ui.screen_box(label);
    let t = ui.text_element_mut(label).expect("a text element");
    // The calibration: this caption is right-justified, which is the only reason the space it
    // breaks at is visible at all. A left-justified one would hide the defect completely.
    let right_justified = t.h_justify == 3 && t.margins == (0, 0, 0, 0);
    let the_whole_caption = t.glyphs.inq_text(false) == CAPTION;

    let lines = dereth_ui::text::glyph::wrap(&t.glyphs.glyphs, screen_box.width(), false);
    let with_space: i32 = t.glyphs.glyphs[0..lines[0].end]
        .iter()
        .map(|g| g.width)
        .sum();
    let wrapped_once = lines.len() == 2;
    // The break character stays on the line it broke; only its width is dropped.
    let dropped_the_space = lines[0].width < with_space;

    let glyphs = t.compose(screen_box);
    let first_line: Vec<&dereth_ui::text::PlacedGlyph> =
        glyphs.iter().filter(|g| g.y == screen_box.y0).collect();
    let drawn: String = first_line
        .iter()
        .filter(|g| g.x <= screen_box.x1)
        .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        .collect();
    let last = first_line
        .iter()
        .filter(|g| g.ch != u16::from(b' '))
        .next_back()
        .expect("the line has ink");
    let advance = t
        .glyphs
        .glyphs
        .iter()
        .find(|g| g.data == last.ch)
        .map_or(0, |g| g.width);
    let flush = last.x + advance == screen_box.x1 + 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-right-justified-caption-that-wraps-sits-flush-against-its-box",
        move |_| {
            right_justified
                && the_whole_caption
                && wrapped_once
                && dropped_the_space
                && drawn == "Environment Texture"
                && flush
        },
    );
}

#[test]
fn scenario_a_wrapped_right_justified_caption_is_flush_with_its_box() {
    scenario("a_wrapped_right_justified_caption_is_flush_with_its_box");
}

/// A wrapped line does not carry the width of the space it broke at; the last line does.
pub fn a_wrapped_line_drops_the_width_of_its_break() {
    use dereth_ui::text::glyph::{wrap, Glyph};

    // Ten-pixel words and a four-pixel space, so the arithmetic is readable.
    let g = |ch: char| Glyph {
        data: ch as u16,
        width: if ch == ' ' { 4 } else { 10 },
        height: 14,
        color: 0xFFFF_FFFF,
        font: 0,
        tag: None,
    };
    let text: Vec<Glyph> = "aa bb cc".chars().map(g).collect();
    let lines = wrap(&text, 24, false);
    let three_lines = lines.len() == 3
        && (lines[0].start, lines[0].end) == (0, 3)
        && (lines[1].start, lines[1].end) == (3, 6)
        && (lines[2].start, lines[2].end) == (6, 8);
    // Each wrapped line is its words, not its words plus the space it broke at; the last line has
    // no break to drop and is its words too.
    let widths = lines.iter().all(|l| l.width == 20);

    // A space at the very end of the text is not a break, so it is still measured.
    let trailing = wrap(&"aa ".chars().map(g).collect::<Vec<_>>(), 100, false);
    let last_line_keeps_it = trailing.len() == 1 && trailing[0].width == 24;

    // ...and a space is not measured against the box, so a line whose words exactly fill it keeps
    // the space rather than wrapping it on to the next line on its own.
    let exact = wrap(&"aa bb".chars().map(g).collect::<Vec<_>>(), 20, false);
    let no_lonely_space =
        exact.len() == 2 && (exact[0].start, exact[0].end) == (0, 3) && exact[0].width == 20;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-wrapped-line-does-not-carry-the-width-of-the-space-it-broke-at",
        move |_| three_lines && widths && last_line_keeps_it && no_lonely_space,
    );
}

#[test]
fn scenario_a_wrapped_line_drops_the_width_of_its_break() {
    scenario("a_wrapped_line_drops_the_width_of_its_break");
}

/// A scrolling pane measures what it holds from the draw itself.
pub fn a_scrolling_pane_measures_itself_from_the_draw() {
    use std::rc::Rc;
    use std::sync::Arc;

    /// The four panes that must measure themselves without being told to.
    const PANES: [u32; 4] = [0x1000_03C4, 0x1000_013C, 0x1000_011D, 0x1000_0126];

    /// Build every shipped layout's roots and answer the first live element carrying `id` whose
    /// box is real -- so the scenario names a **shipped** element and cannot be satisfied by one
    /// it made up.
    fn a_shipped_pane(
        ui: &mut dereth_ui::UiSystem,
        store: &Arc<dereth_dat::RetailDatStore>,
        id: u32,
    ) -> dereth_ui::ElemHandle {
        use dereth_primitives::AssetSource as _;

        let types = ui.property_types.clone();
        for did in store.ids_of(dereth_dat::DbType::UiLayout) {
            let Ok(bytes) = store.read(did) else { continue };
            let Ok(desc) = dereth_ui::desc::LayoutDesc::read(did, &bytes, &types) else {
                continue;
            };
            let roots: Vec<u32> = desc.elements.values().map(|e| e.element_id.0).collect();
            for eid in roots {
                let s = Rc::new(RetailAssets(Arc::clone(store)));
                let Ok(h) = dereth_ui::framework::create_and_add_root_element_by_data_id(
                    ui,
                    s.as_ref(),
                    did,
                    dereth_ui::ElementId(eid),
                ) else {
                    continue;
                };
                let mut all = Vec::new();
                walk_tree(ui, h, &mut all);
                for candidate in all {
                    let Some(n) = ui.node(candidate) else {
                        continue;
                    };
                    if n.element_id().0 == id && n.region.box_.is_valid() {
                        return candidate;
                    }
                }
            }
        }
        panic!("{id:#010X} is in none of the shipped roots");
    }

    /// Twenty-four lines of prose -- more than any of these panes is tall.
    fn a_long_description() -> String {
        let mut s = String::new();
        for i in 0..24 {
            s.push_str(&format!(
                "Increases the target's Strength by a considerable amount, line {i} of the wordy \
                 description the shipped tables hand the pane. "
            ));
        }
        s
    }

    let mut overflowed = true;
    let mut measured_after_the_write = true;
    for id in PANES {
        let (mut ui, store) = a_shipped_ui();
        let h = a_shipped_pane(&mut ui, &store, id);
        ui.text_element_mut(h)
            .expect("a text element")
            .set_text(&a_long_description());
        let before = ui.text_element_mut(h).expect("alive").scroll.height;
        let view = ui.screen_box(h).height();
        // The precondition: writing the text alone does not measure it, so what the draw does is
        // separable from what the write does.
        measured_after_the_write &= before <= view;
        let mut back = dereth_ui::RecordingDrawBackend::default();
        ui.draw(&mut back);
        let content = ui.text_element_mut(h).expect("alive").scroll.height;
        overflowed &= content > view;
    }

    // The calibration: a pane whose text fits reports an extent that fits, so the readings above
    // are not of a rule that always says yes.
    let (mut ui, store) = a_shipped_ui();
    let h = a_shipped_pane(&mut ui, &store, PANES[3]);
    ui.text_element_mut(h)
        .expect("a text element")
        .set_text("Strength Self VI");
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let view = ui.screen_box(h).height();
    let content = ui.text_element_mut(h).expect("alive").scroll.height;
    let fits = content > 0 && content <= view;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-scrolling-pane-measures-what-it-holds-from-the-draw-itself",
        move |_| measured_after_the_write && overflowed && fits,
    );
}

#[test]
fn scenario_a_scrolling_pane_measures_itself_from_the_draw() {
    scenario("a_scrolling_pane_measures_itself_from_the_draw");
}

// ---------------------------------------------------------------------------------------------
// notice.autorun.*
//
// The words themselves are literals in the scenarios below rather than the symbols the client
// writes them through.
// ---------------------------------------------------------------------------------------------

/// The two lines the client writes, and the channel it writes them on. Literals, not the symbols
/// the client writes them through: a scenario that read a value back through the symbol the
/// production code writes it through could not detect a wrong value.
const AUTORUN_ON: &str = "AutoRun ON";
const AUTORUN_OFF: &str = "AutoRun OFF";
const CLIENT_FEEDBACK_CHANNEL: u32 = 0x1A;

/// What the client has queued to say and not yet said.
fn queued_to_say(c: &HeadlessClient) -> Vec<(u32, String)> {
    c.view()
        .expect_app()
        .objects()
        .world
        .scroll
        .pending()
        .iter()
        .map(|f| (f.chat_type, f.body.clone()))
        .collect()
}

/// How many lines the client has ever written -- a count and not a queue length, because the queue
/// is drained every frame and "nothing was said" and "something was said and consumed" are the
/// same reading without it.
fn lines_ever_written(c: &HeadlessClient) -> u64 {
    c.view().expect_app().objects().world.scroll.added
}

/// A client in the world with a keyboard on it.
fn a_client_in_the_world() -> (HeadlessClient, dereth_testkit::adapters_shell::Hands) {
    (
        HeadlessClient::new(ClientSpec::gameplay_in_world(4)),
        dereth_testkit::adapters_shell::Hands::new(),
    )
}

/// Press a key and run the frame that acts on it.
fn press_and_frame(
    c: &mut HeadlessClient,
    hands: &mut dereth_testkit::adapters_shell::Hands,
    code: winit::keyboard::KeyCode,
) {
    hands.press(c, the_key(code));
    c.tick(1);
}

fn release_and_frame(
    c: &mut HeadlessClient,
    hands: &mut dereth_testkit::adapters_shell::Hands,
    code: winit::keyboard::KeyCode,
) {
    hands.release(c, the_key(code));
    c.tick(1);
}

fn the_key(code: winit::keyboard::KeyCode) -> dereth_client::platform::keys::Key {
    dereth_client::platform::window::key_from_key_code(code).expect("the host names this key")
}

/// Turning the run lock on or off says so in the message window.
pub fn the_run_lock_says_so_in_the_message_window() {
    use winit::keyboard::KeyCode;

    let (mut c, mut hands) = a_client_in_the_world();
    let starts_off = !c.view().expect_app().char_input().auto_run && queued_to_say(&c).is_empty();
    let base = lines_ever_written(&c);

    press_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    let on = c.view().expect_app().char_input().auto_run
        && queued_to_say(&c) == vec![(CLIENT_FEEDBACK_CHANNEL, AUTORUN_ON.to_owned())]
        && lines_ever_written(&c) == base + 1
        && c.view()
            .expect_app()
            .movement_commands()
            .auto_run_notices_sent
            == 1;
    release_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    let drained = queued_to_say(&c).is_empty();

    press_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    let off = !c.view().expect_app().char_input().auto_run
        && queued_to_say(&c) == vec![(CLIENT_FEEDBACK_CHANNEL, AUTORUN_OFF.to_owned())]
        && lines_ever_written(&c) == base + 2
        && c.view()
            .expect_app()
            .movement_commands()
            .auto_run_notices_sent
            == 2;
    release_and_frame(&mut c, &mut hands, KeyCode::KeyQ);

    c.assert_behaviour(
        "notice.autorun.turning-the-run-lock-on-or-off-says-so-in-the-message-window",
        move |_| starts_off && on && drained && off,
    );
    c.shutdown();
}

#[test]
fn scenario_the_run_lock_says_so_in_the_message_window() {
    scenario("the_run_lock_says_so_in_the_message_window");
}

/// A key that does not change the run lock says nothing.
pub fn a_key_that_changes_nothing_says_nothing() {
    use winit::keyboard::KeyCode;

    let (mut c, mut hands) = a_client_in_the_world();
    let base = lines_ever_written(&c);

    // The lock is already off, so a movement key does not change it -- and the client says
    // nothing. Without this direction the claim above would hold on a client that wrote the line
    // on every key press.
    press_and_frame(&mut c, &mut hands, KeyCode::KeyW);
    let walked = c.view().expect_app().char_input().forward;
    let silent = lines_ever_written(&c) == base
        && c.view()
            .expect_app()
            .movement_commands()
            .auto_run_notices_sent
            == 0;
    release_and_frame(&mut c, &mut hands, KeyCode::KeyW);

    // ...and the same key when it *does* change it -- cancelling the lock -- tells the player.
    press_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    let locked = c.view().expect_app().char_input().auto_run;
    release_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    let after_on = lines_ever_written(&c);
    press_and_frame(&mut c, &mut hands, KeyCode::KeyW);
    let cancelled = !c.view().expect_app().char_input().auto_run
        && queued_to_say(&c) == vec![(CLIENT_FEEDBACK_CHANNEL, AUTORUN_OFF.to_owned())]
        && lines_ever_written(&c) == after_on + 1;
    release_and_frame(&mut c, &mut hands, KeyCode::KeyW);

    c.assert_behaviour(
        "notice.autorun.a-key-that-does-not-change-the-run-lock-says-nothing",
        move |_| walked && silent && locked && cancelled,
    );
    c.shutdown();
}

#[test]
fn scenario_a_key_that_changes_nothing_says_nothing() {
    scenario("a_key_that_changes_nothing_says_nothing");
}

/// The line reaches the strip and every chat window drops it.
pub fn the_autorun_line_reaches_the_strip_and_the_chat_windows_drop_it() {
    use dereth_ui_screens::chat::interface::{window, ChatInterface, ChatMessage, Routed};
    use winit::keyboard::KeyCode;

    let (mut c, mut hands) = a_client_in_the_world();
    let (spew, scroll, dropped, kept) = {
        let s = c.view().expect_app().hud().stats;
        (
            s.spew_lines,
            s.scroll_lines,
            s.chat_lines_dropped,
            s.chat_lines,
        )
    };

    press_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    // The queued line is drained by the next frame, which is what the release runs.
    release_and_frame(&mut c, &mut hands, KeyCode::KeyQ);

    let landed = {
        let app = c.view().expect_app();
        let s = app.hud().stats;
        s.scroll_lines == scroll + 1
            && s.spew_lines == spew + 1
            && app.hud().panels.spew.model.items == vec![AUTORUN_ON.to_owned()]
            && app.hud().panels.spew.model.pending.is_empty()
            && app.hud().panels.spew.drawn >= 1
            // Every chat window filtered it out, and none kept it.
            && s.chat_lines_dropped == dropped + 1
            && s.chat_lines == kept
    };

    // The filter itself, so the drop above is explained rather than only observed -- and a player
    // who asks for this kind of line does get it.
    let m = ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        ty: CLIENT_FEEDBACK_CHANNEL as u8,
        body: AUTORUN_ON.to_owned(),
        prefix: None,
        window: 0,
    };
    let dropped_everywhere = [
        window::MAIN,
        window::FLOATY_1,
        window::FLOATY_2,
        window::FLOATY_3,
        window::FLOATY_4,
    ]
    .into_iter()
    .all(|w| ChatInterface::new(w).route(&m) == Routed::FilteredOut);
    let asked_for_it = {
        let mut main = ChatInterface::new(window::MAIN);
        main.filter |= 0x0400_0000;
        main.route(&m) == Routed::Accepted
    };

    c.assert_behaviour(
        "notice.autorun.the-line-reaches-the-strip-and-the-chat-windows-drop-it",
        move |_| landed && dropped_everywhere && asked_for_it,
    );
    c.shutdown();
}

#[test]
fn scenario_the_autorun_line_reaches_the_strip_and_the_chat_windows_drop_it() {
    scenario("the_autorun_line_reaches_the_strip_and_the_chat_windows_drop_it");
}

// ---------------------------------------------------------------------------------------------
// ui.list.*
//
// Click a friend and the row highlights.
//
// What is read is the picture each row really put on the frame's blit list -- a drawn result and
// not a recorded intention -- rather than the rasterised pixels below it.
// ---------------------------------------------------------------------------------------------

/// Pressing a row draws the band across that row and no other.
pub fn pressing_a_row_draws_the_band_across_that_row() {
    use dereth_ui::{RecordingDrawBackend, StateId};

    /// The friends panel's row template, and the two pictures a row can blit.
    const ROW_TEMPLATE: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0519);
    const SELECTED_PLATE: dereth_primitives::DataId = dereth_primitives::DataId(0x0600_1AAF);
    const BASE_PLATE: dereth_primitives::DataId = dereth_primitives::DataId(0x0600_4CCA);
    /// The social panel, and the page the friends tab is one of.
    const SOCIAL_PANEL: u32 = 0x0C;
    const SOCIAL_PAGE: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_018F);

    fn a_friend(id: u32, name: &str) -> dereth_protocol::social::FriendData {
        dereth_protocol::social::FriendData {
            id: dereth_primitives::ObjectId(id),
            online: 1,
            appear_offline: 0,
            name: name.to_owned(),
            friends_list: Vec::new(),
            friend_of_list: Vec::new(),
        }
    }

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut hands = dereth_testkit::adapters_shell::Hands::new();

    // Open the social panel and its friends tab, by clicking what a player clicks.
    let button = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the UI shell is up");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen is current");
        any.downcast_ref::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen")
            .toolbar
            .buttons
            .iter()
            .find(|b| b.panel_id == SOCIAL_PANEL)
            .expect("the toolbar has a social button")
            .handle
    };
    hands.click_handle(&mut c, button);
    let tab = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the UI shell is up");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen is current");
        let root = any
            .downcast_ref::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen")
            .root()
            .expect("the gameplay root");
        let page = shell
            .ui
            .get_child_recursive(root, SOCIAL_PAGE)
            .expect("the social page");
        // The tab is discovered rather than named: it is the one whose page carries the
        // friends list, off the social panel's own table of pages and tabs.
        let pairs: Vec<(dereth_ui::ElementId, dereth_ui::ElementId)> = shell
            .ui
            .node(page)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .expect("the social page is a panel")
            .page_to_tab
            .iter()
            .map(|(p, t)| (*p, *t))
            .collect();
        pairs
            .into_iter()
            .find_map(|(page_id, tab_id)| {
                let pe = shell.ui.get_child_recursive(page, page_id)?;
                shell
                    .ui
                    .get_child_recursive(pe, dereth_ui_screens::panels::friends::FRIENDS_LIST)?;
                shell.ui.get_child_recursive(page, tab_id)
            })
            .expect("one page of the social panel carries the friends list")
    };
    hands.click_handle(&mut c, tab);
    c.tick(2);

    // Three friends, through the client's own receiver for the shard's message.
    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::social::SocialFriendsUpdate {
            friends: vec![
                a_friend(0x5000_001E, "Ash"),
                a_friend(0x5000_001F, "Bex"),
                a_friend(0x5000_0020, "Caius"),
            ],
            update_type: 0,
        },
    ));
    c.tick(3);

    /// The friends list, and the row elements it built from its own entry template.
    fn the_list(c: &mut HeadlessClient) -> dereth_ui::ElemHandle {
        with_gameplay_ui(c, |ui, s| {
            let root = s.root().expect("the gameplay root");
            ui.get_child_recursive(root, dereth_ui_screens::panels::friends::FRIENDS_LIST)
                .expect("the friends list")
        })
    }

    fn the_rows(c: &mut HeadlessClient) -> Vec<dereth_ui::ElemHandle> {
        let list = the_list(c);
        with_gameplay_ui(c, |ui, _| {
            ui.node(list)
                .and_then(|n| {
                    n.behaviour
                        .as_ref()?
                        .as_any()?
                        .downcast_ref::<dereth_ui::widgets::listbox::ListBox>()
                })
                .expect("the friends list is a list box")
                .items
                .clone()
        })
    }

    fn which_row_is_selected(c: &mut HeadlessClient) -> Option<usize> {
        let list = the_list(c);
        with_gameplay_ui(c, |ui, _| {
            ui.node(list)
                .and_then(|n| n.behaviour.as_ref())
                .and_then(|b| (**b).as_any())
                .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
                .and_then(|l| l.selected)
        })
    }

    let rows = the_rows(&mut c);
    let three_rows = rows.len() == 3;

    /// The pictures this element put on the frame's own blit list.
    fn pictures(
        c: &mut HeadlessClient,
        h: dereth_ui::ElemHandle,
    ) -> Vec<dereth_primitives::DataId> {
        let mut back = RecordingDrawBackend::default();
        c.app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .draw(&mut back);
        c.view()
            .expect_app()
            .ui_draw_list()
            .iter()
            .filter(|cmd| cmd.who == h)
            .filter_map(|cmd| cmd.image)
            .collect()
    }

    let right_template = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        rows.iter().all(|h| {
            let n = ui.node(*h).expect("the row is live");
            n.element_id() == ROW_TEMPLATE
                && n.desc.access_state(StateId(6)).is_some()
                && n.state == StateId(0)
        })
    };
    let nothing_selected = which_row_is_selected(&mut c).is_none();

    let list = the_list(&mut c);
    hands.click_row(&mut c, list, rows[1]);
    c.tick(1);

    let selected = which_row_is_selected(&mut c) == Some(1);
    let in_the_selected_state = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        ui.node(rows[1]).expect("live").state == StateId(6)
            && ui.node(rows[0]).expect("live").state == StateId(0)
            && ui.node(rows[2]).expect("live").state == StateId(0)
    };
    let bands = [
        pictures(&mut c, rows[0]),
        pictures(&mut c, rows[1]),
        pictures(&mut c, rows[2]),
    ];
    let only_that_row = bands[0] == vec![BASE_PLATE]
        && bands[1] == vec![SELECTED_PLATE]
        && bands[2] == vec![BASE_PLATE];

    // A second press moves the band and puts the first row back.
    hands.click_row(&mut c, list, rows[2]);
    c.tick(1);
    let moved = which_row_is_selected(&mut c) == Some(2)
        && pictures(&mut c, rows[1]) == vec![BASE_PLATE]
        && pictures(&mut c, rows[2]) == vec![SELECTED_PLATE]
        && pictures(&mut c, rows[0]) == vec![BASE_PLATE];

    c.assert_behaviour(
        "ui.list.pressing-a-row-draws-the-band-across-that-row-and-no-other",
        move |_| {
            three_rows
                && right_template
                && nothing_selected
                && selected
                && in_the_selected_state
                && only_that_row
                && moved
        },
    );
    c.shutdown();
}

#[test]
fn scenario_pressing_a_row_draws_the_band_across_that_row() {
    scenario("pressing_a_row_draws_the_band_across_that_row");
}

/// Do something with the live gameplay screen.
fn with_gameplay_ui<R>(
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

// ---------------------------------------------------------------------------------------------
// ui.text, the overflow rules
//
// Two captions in retail: the squelch panel's draws "Character" and the wrapped remainder is
// simply not shown, and the rendering-quality row's draws "Environment Texture" with "Detail" not
// shown. Centring the whole two-line block would land neither line where it should.
//
// **Nothing here drops a glyph.** What a player sees is what the element's own box lets through,
// which is why the paragraph scenario below asserts that every line is still there to scroll to.
// ---------------------------------------------------------------------------------------------

/// Which of a composed run's glyphs put ink inside `clip` -- a glyph straddling the edge is partly
/// drawn and counts, which is what the client's own per-pixel clipping does.
fn what_is_visible(
    glyphs: &[dereth_ui::text::PlacedGlyph],
    clip: dereth_ui::Box2D,
    line_height: i32,
) -> String {
    glyphs
        .iter()
        .filter(|g| {
            g.y <= clip.y1
                && g.y + line_height - 1 >= clip.y0
                && g.x <= clip.x1
                && g.x >= clip.x0 - 64
        })
        .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        .collect()
}

/// Deliver whatever the tree raised, until it settles.
fn settle(
    ui: &mut dereth_ui::UiSystem,
    s: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
) {
    use dereth_ui::framework::Screen as _;
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in batch {
            if let dereth_ui::Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
    panic!("the element outbox never settled");
}

/// A real click on an element of a shipped tree that has no client around it.
fn click_in_tree(
    ui: &mut dereth_ui::UiSystem,
    s: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
    h: dereth_ui::ElemHandle,
) {
    let (ox, oy) = ui.screen_origin(h);
    let r = ui.node(h).expect("alive").region.box_;
    let (x, y) = (ox + r.width() / 2, oy + r.height() / 2);
    ui.mouse_move(dereth_primitives::LocalTime(1.0), x, y);
    settle(ui, s);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    settle(ui, s);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    settle(ui, s);
}

/// The shipped gameplay screen over the shipped data files, with no client around it.
fn a_shipped_gameplay_tree() -> (
    dereth_ui::UiSystem,
    dereth_ui_screens::screens::gameplay::GamePlayScreen,
) {
    use dereth_ui::framework::Screen as _;
    let (mut ui, _store) = a_shipped_ui();
    let mut screen = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    ui.requests.clear();
    (ui, screen)
}

/// A caption too tall for its box shows its first line at the top of it.
pub fn a_caption_too_tall_for_its_box_shows_its_first_line_at_the_top() {
    use dereth_ui::framework::Screen as _;

    /// The squelch panel's caption, and the social panel it is on.
    const SQUELCH_CAPTION: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_053F);
    const SOCIAL_PANEL: u32 = 0x0C;
    const SOCIAL_PAGE: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_018F);
    /// The caption inside the shipped option-row template.
    const MENU_LABEL: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0223);

    // ---- the first caption, through the whole draw, after real clicks ----------------------
    let (mut ui, mut screen) = a_shipped_gameplay_tree();
    let social = screen
        .toolbar
        .buttons
        .iter()
        .find(|b| b.panel_id == SOCIAL_PANEL)
        .expect("the toolbar has a social button")
        .handle;
    click_in_tree(&mut ui, &mut screen, social);
    let root = screen.root().expect("the gameplay root");
    let page = ui
        .get_child_recursive(root, SOCIAL_PAGE)
        .expect("the social page");
    let pairs: Vec<(dereth_ui::ElementId, dereth_ui::ElementId)> = ui
        .node(page)
        .and_then(|n| {
            n.behaviour
                .as_ref()?
                .as_any()?
                .downcast_ref::<dereth_ui::widgets::panel::Panel>()
        })
        .expect("the social page is a panel")
        .page_to_tab
        .iter()
        .map(|(p, t)| (*p, *t))
        .collect();
    let tab = pairs
        .into_iter()
        .find_map(|(page_id, tab_id)| {
            let pe = ui.get_child_recursive(page, page_id)?;
            ui.get_child_recursive(pe, SQUELCH_CAPTION)?;
            ui.get_child_recursive(page, tab_id)
        })
        .expect("one page of the social panel carries that caption");
    click_in_tree(&mut ui, &mut screen, tab);

    let label = ui
        .get_child_recursive(root, SQUELCH_CAPTION)
        .expect("the caption");
    let open = ui.is_visible(label);
    let mut rec = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut rec);
    let cmd = rec
        .calls
        .iter()
        .find(|c| c.who == label)
        .expect("the caption put a draw of its own on the frame")
        .clone();
    let composed: String = cmd
        .glyphs
        .iter()
        .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        .collect();
    let seen = what_is_visible(&cmd.glyphs, cmd.screen.intersect(&cmd.clip), 18);
    let first_squelch = composed == "Character Name:"
        && seen == "Character "
        // ...and the first line sits at the top of the box, not centred as a two-line block.
        && cmd.glyphs[0].y == cmd.screen.y0;

    // ---- the second caption, found by its own words ----------------------------------------
    let (mut ui, screen) = a_shipped_gameplay_tree();
    let root = screen.root().expect("a root");
    let mut all = Vec::new();
    walk_tree(&ui, root, &mut all);
    let candidates: Vec<dereth_ui::ElemHandle> = all
        .into_iter()
        .filter(|h| ui.node(*h).is_some_and(|n| n.element_id() == MENU_LABEL))
        .collect();
    let label = candidates
        .into_iter()
        .find(|h| {
            ui.text_element_mut(*h)
                .is_some_and(|t| t.glyphs.inq_text(false) == "Environment Texture Detail")
        })
        .expect("one option row's caption reads its own words");
    let screen_box = ui.screen_box(label);
    let t = ui.text_element_mut(label).expect("a text element");
    let glyphs = t.compose(screen_box);
    let composed: String = glyphs
        .iter()
        .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        .collect();
    let second = composed == "Environment Texture Detail"
        && what_is_visible(&glyphs, screen_box, 14) == "Environment Texture"
        && glyphs[0].y == screen_box.y0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-caption-too-tall-for-its-box-shows-its-first-line-at-the-top",
        move |_| open && first_squelch && second,
    );
}

#[test]
fn scenario_a_caption_too_tall_for_its_box_shows_its_first_line_at_the_top() {
    scenario("a_caption_too_tall_for_its_box_shows_its_first_line_at_the_top");
}

/// A caption that fits and asks to be centred is still centred.
pub fn a_caption_that_fits_is_still_centred() {
    use dereth_ui::framework::Screen as _;

    let (mut ui, screen) = a_shipped_gameplay_tree();
    let root = screen.root().expect("a root");
    let mut all = Vec::new();
    walk_tree(&ui, root, &mut all);

    let mut looked = 0usize;
    let mut flattened = 0usize;
    for h in all {
        let Some(_) = ui.node(h) else { continue };
        let screen_box = ui.screen_box(h);
        if !screen_box.is_valid() {
            continue;
        }
        let Some(t) = ui.text_element_mut(h) else {
            continue;
        };
        if t.v_justify != 1 || t.glyphs.is_empty() {
            continue;
        }
        let content = t.content_box(screen_box);
        if !content.is_valid() {
            continue;
        }
        let lines =
            dereth_ui::text::glyph::wrap(&t.glyphs.glyphs, content.width(), t.glyphs.one_line);
        let text_h: i32 = lines.iter().map(|l| l.height).sum();
        // Only captions with real room to spare, so "centred" and "at the top" are two different
        // readings.
        if text_h == 0 || content.height() - text_h < 8 {
            continue;
        }
        looked += 1;
        let glyphs = t.compose(screen_box);
        let Some(first) = glyphs.first() else {
            continue;
        };
        if first.y <= content.y0 {
            flattened += 1;
        }
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-caption-that-fits-and-asks-to-be-centred-is-still-centred",
        move |_| {
            // The denominator first: "none failed" and "none was looked at" are the same reading
            // without it.
            looked >= 20 && flattened == 0
        },
    );
}

#[test]
fn scenario_a_caption_that_fits_is_still_centred() {
    scenario("a_caption_that_fits_is_still_centred");
}

/// A paragraph too tall for its box keeps every line and scrolls to them.
pub fn a_paragraph_too_tall_for_its_box_keeps_every_line() {
    use dereth_ui::framework::Screen as _;

    /// The wizard's appearance help -- the shipped paragraph that overflows its pane.
    const HELP: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_03AB);

    let (mut ui, _store) = a_shipped_ui();
    let mut screen = dereth_ui_screens::screens::chargen::CharGenScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the wizard builds from the shipped layout");
    let root = *screen.roots().first().expect("the wizard has a root");
    let help = ui
        .get_child_recursive(root, HELP)
        .expect("the appearance help");
    let screen_box = ui.screen_box(help);
    let t = ui.text_element_mut(help).expect("a text element");
    let content = t.content_box(screen_box);
    let lines = dereth_ui::text::glyph::wrap(&t.glyphs.glyphs, content.width(), t.glyphs.one_line);
    let text_h: i32 = lines.iter().map(|l| l.height).sum();
    let really_overflows = lines.len() > 25 && text_h > content.height();

    // How many wrapped lines carry a glyph that draws: a blank line occupies no row of its own.
    let n = t.glyphs.glyphs.len();
    let drawable = lines
        .iter()
        .filter(|l| {
            t.glyphs.glyphs[l.start.min(n)..l.end.min(n)]
                .iter()
                .any(|g| !g.is_new_line())
        })
        .count();
    let glyphs = t.compose(screen_box);
    let rows: std::collections::BTreeSet<i32> = glyphs.iter().map(|g| g.y).collect();
    let every_line_is_placed =
        rows.len() == drawable && *rows.iter().next().expect("a first row") == content.y0;

    // Scrolling is what shows the rest, and it can only do that because every line is there.
    let before = glyphs[0].y;
    t.scroll.y = 16;
    let scrolled = t.compose(screen_box);
    let follows_the_scroll = scrolled[0].y == before - 16;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-paragraph-too-tall-for-its-box-keeps-every-line-and-scrolls",
        move |_| really_overflows && every_line_is_placed && follows_the_scroll,
    );
}

#[test]
fn scenario_a_paragraph_too_tall_for_its_box_keeps_every_line() {
    scenario("a_paragraph_too_tall_for_its_box_keeps_every_line");
}

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
pub fn a_recall_broken_by_moving_says_so_on_the_strip() {
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

#[test]
fn scenario_a_recall_broken_by_moving_says_so_on_the_strip() {
    scenario("a_recall_broken_by_moving_says_so_on_the_strip");
}

/// Every refusal the shard can send draws its own line on the surface its own kind names.
pub fn every_refusal_draws_its_own_line_on_its_own_surface() {
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

#[test]
fn scenario_every_refusal_draws_its_own_line_on_its_own_surface() {
    scenario("every_refusal_draws_its_own_line_on_its_own_surface");
}

/// A refusal carrying a word of the shard's own puts it into the line.
pub fn a_refusal_carrying_the_shards_word_puts_it_in_the_line() {
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

#[test]
fn scenario_a_refusal_carrying_the_shards_word_puts_it_in_the_line() {
    scenario("a_refusal_carrying_the_shards_word_puts_it_in_the_line");
}

// -------------------------------------------------------------------------------------------
// ui.caret.flashes-at-the-interval-the-player-set-for-their-desktop
//
// **The client is asked, and the desktop is asked separately.** The interval the running client
// ends up with is compared against a fresh query of the desktop made here, so the reading is not
// derived from the value the client stored; and the client's own value is seeded with a
// deliberately wrong one first, so a client that never asked would fail rather than agree by
// accident on a machine whose setting happens to be this client's old default.
// -------------------------------------------------------------------------------------------

/// The chat entry of the shipped gameplay layout: a box that can be typed into, so it has a caret.
const CHAT_ENTRY_FOR_CARET: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0016);

/// An interval no desktop uses, seeded so that "the client replaced it" is a measurement.
const SENTINEL_INTERVAL: f64 = 17.0;

/// The caret flashes at the player's own desktop interval, and is drawn inside it and gone past it.
pub fn the_caret_flashes_at_the_players_own_desktop_interval() {
    use dereth_client::pump::window_proc::caret_blink_time_seconds;
    use dereth_primitives::LocalTime;
    use dereth_testkit::adapters_shell::element;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let entry = element(&c, CHAT_ENTRY_FOR_CARET);
    let editable = {
        let shell = c.app_mut().ui_mut().expect("the shell");
        let editable = shell
            .ui
            .text_element_mut(entry)
            .expect("the chat entry is a text element")
            .bits
            .editable();
        shell.ui.take_focus(entry);
        shell.ui.caret_blink_time = SENTINEL_INTERVAL;
        editable
    };

    // One ordinary frame of the running client -- the real producer -- and then the desktop asked
    // again, here, independently.
    c.tick(1);
    let queried = caret_blink_time_seconds();
    let asked_the_desktop = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .caret_blink_time
        == queried
        && c.view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .caret_blink_time
            != SENTINEL_INTERVAL;

    // The boundary itself, driven at a time the scenario names rather than by waiting: the shell's
    // own frame call is the same one the client makes, and it takes the clock as an argument, so
    // the exact edge is reachable without sleeping and without touching the player's setting.
    let host = c.view().expect_app().host_state().clone();
    let shell = c.app_mut().ui_mut().expect("the shell");
    let screen = shell.ui.screen_box(entry);
    let anchor = 1_000_000.0;
    let want = {
        let text = shell
            .ui
            .text_element_mut(entry)
            .expect("the chat entry is still live");
        text.last_flash_flip = anchor;
        text.caret_moved = false;
        text.caret_visible = true;
        let b = text
            .caret_box(screen)
            .expect("an editable, visible caret has a box");
        let colour = text.font_color;
        dereth_ui::UiFill {
            x: b.x0,
            y: b.y0,
            w: b.x1 - b.x0 + 1,
            h: b.y1 - b.y0 + 1,
            color: colour,
        }
    };
    let drawn = |shell: &mut dereth_client::ui::UiShell, want: dereth_ui::UiFill| {
        shell
            .draw_list()
            .iter()
            .filter(|cmd| cmd.who == entry)
            .flat_map(|cmd| cmd.fills.iter())
            .any(|fill| *fill == want)
    };

    let mut input = dereth_ui::NullInputPump;
    shell.frame(LocalTime(anchor), &host, &mut input);
    let flashes = if queried == 0.0 {
        // A desktop that answers with no interval at all: every tick is past the boundary, so the
        // caret flips on the first one and back on the next.
        let off = !drawn(shell, want);
        shell.frame(LocalTime(anchor), &host, &mut input);
        off && drawn(shell, want)
    } else {
        let on = drawn(shell, want);
        shell.frame(LocalTime(anchor + queried * 0.5), &host, &mut input);
        let still_on = drawn(shell, want);
        let epsilon = (queried * 1.0e-9).max(0.001);
        shell.frame(LocalTime(anchor + queried + epsilon), &host, &mut input);
        let off = !drawn(shell, want);
        on && still_on && off
    };

    c.assert_behaviour(
        "ui.caret.flashes-at-the-interval-the-player-set-for-their-desktop",
        move |_| editable && asked_the_desktop && flashes,
    );
    c.shutdown();
}

#[test]
fn scenario_the_caret_flashes_at_the_players_own_desktop_interval() {
    scenario("the_caret_flashes_at_the_players_own_desktop_interval");
}

// =============================================================================================
// hud.* and character-page.raise.* -- three things about the heads-up display
//
// Three rows: one is a defect this client had, and two are the original client's own behaviour,
// asserted so that a change which reverses either reddens here. The session is read from the
// decoded corpus through the harness's own reader; the claim is about the buttons, not about the
// bytes of the login.
// =============================================================================================

use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};
use dereth_ui_screens::panels::{attributes, skills, statmgmt};
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

/// The gameplay screen and the element tree together.
fn hud_gameplay(c: &mut HeadlessClient) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    (
        ui,
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen"),
    )
}

fn hud_root(c: &HeadlessClient) -> ElemHandle {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .flow
        .current()
        .expect("a screen")
        .roots()[0]
}

fn hud_find(c: &HeadlessClient, id: ElementId) -> ElemHandle {
    let shell = c.view().expect_app().ui().expect("the UI shell is up");
    shell
        .ui
        .get_child_recursive(hud_root(c), id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

fn hud_visible(c: &HeadlessClient, h: ElemHandle) -> bool {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .is_some_and(|n| n.region.flags.visible)
}

// ---------------------------------------------------------------------------------------------
// hud.power-bar.a-jump-shows-the-one-bar-the-client-listens-with-and-never-the-other
// ---------------------------------------------------------------------------------------------

/// Every power bar in the live tree, by its id and kind.
fn power_bars(c: &HeadlessClient) -> Vec<(u32, u32, ElemHandle)> {
    use dereth_ui_screens::hud::powerbar::{FLOATY_POWERBAR_TYPE, POWERBAR_TYPE};
    fn walk(ui: &UiSystem, h: ElemHandle, out: &mut Vec<(u32, u32, ElemHandle)>) {
        if let Some(n) = ui.node(h) {
            if n.ty() == POWERBAR_TYPE || n.ty() == FLOATY_POWERBAR_TYPE {
                out.push((n.element_id().0, n.ty().0, h));
            }
        }
        for child in ui.children(h) {
            walk(ui, child, out);
        }
    }
    let shell = c.view().expect_app().ui().expect("the UI shell is up");
    let mut out = Vec::new();
    walk(&shell.ui, hud_root(c), &mut out);
    out
}

/// How many of the frame's own drawings came from under this element.
fn draws_under(c: &HeadlessClient, h: ElemHandle) -> usize {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the UI shell is up").ui;
    app.ui_draw_list()
        .iter()
        .filter(|cmd| cmd.who == h || ui.is_ancestor_of(h, cmd.who))
        .count()
}

/// The layout carries two power bars, one at the bottom of the screen and one in the middle; a
/// jump shows only the one the client listens with.
pub fn a_jump_shows_one_power_bar_and_never_the_other() {
    use dereth_ui_screens::hud::powerbar::{FLOATY_POWERBAR_TYPE, POWERBAR_TYPE};

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));

    // The denominator first: both really are in the shipped tree, so "only one showed" cannot
    // pass because the other one is missing.
    let objects = power_bars(&c);
    let both_are_there = objects
        .iter()
        .map(|(id, ty, _)| (*id, *ty))
        .collect::<Vec<_>>()
        == vec![
            (0x1000_0044, POWERBAR_TYPE.0),
            (0x1000_0613, FLOATY_POWERBAR_TYPE.0),
        ];
    let classic = objects[0].2;
    let floaty = objects[1].2;
    // Only one of the two kinds asks to hear about a power bar at all.
    let one_listens = !dereth_ui_screens::hud::powerbar::registers_power_bar_notices(
        dereth_ui::ElementType(POWERBAR_TYPE.0),
    ) && dereth_ui_screens::hud::powerbar::registers_power_bar_notices(
        dereth_ui::ElementType(FLOATY_POWERBAR_TYPE.0),
    );
    let both_start_hidden = !hud_visible(&c, classic)
        && !hud_visible(&c, floaty)
        && c.view().expect_app().hud().panels.power_bar.bound() == 2;

    // The one line a jump runs.
    c.app_mut().objects_mut().world.combat.begin_power_bar(
        dereth_client_model::combat::PowerBarMode::Jump,
        false,
        0,
    );
    c.tick(1);

    let one_came_up = hud_visible(&c, floaty)
        && !hud_visible(&c, classic)
        && draws_under(&c, floaty) > 0
        && draws_under(&c, classic) == 0;
    // ...and it is the jump bar, on exactly one of them.
    let the_jump_bar = c
        .view()
        .expect_app()
        .hud()
        .panels
        .power_bar
        .bars
        .iter()
        .filter(|b| b.visible)
        .map(|b| b.mode)
        .collect::<Vec<_>>()
        == vec![dereth_ui_screens::hud::powerbar::PowerBarMode::Jump];

    c.app_mut().objects_mut().world.combat.hide_power_bar();
    c.tick(1);
    let and_went_away = !hud_visible(&c, floaty) && !hud_visible(&c, classic);

    c.assert_behaviour(
        "hud.power-bar.a-jump-shows-the-one-bar-the-client-listens-with-and-never-the-other",
        move |_| {
            both_are_there
                && one_listens
                && both_start_hidden
                && one_came_up
                && the_jump_bar
                && and_went_away
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_jump_shows_one_power_bar_and_never_the_other() {
    scenario("a_jump_shows_one_power_bar_and_never_the_other");
}

// ---------------------------------------------------------------------------------------------
// character-page.raise.both-buttons-on-both-pages-put-the-request-on-the-wire
// ---------------------------------------------------------------------------------------------

/// What this process really put on the wire since it was last asked.
fn wire_actions(c: &mut HeadlessClient) -> Vec<(u32, Vec<u8>)> {
    let out = c.replay_net_mut().expect("the endpoint").take_outgoing();
    let mut actions = Vec::new();
    for (raw, _) in &out {
        let p =
            dereth_transport::wire::ParsedPacket::parse(raw).expect("this process's own datagram");
        for f in &p.fragments {
            if f.payload.len() < dereth_protocol::OrderedActionHeader::PACK_SIZE + 4 {
                continue;
            }
            if u32::from_le_bytes(f.payload[0..4].try_into().expect("four bytes"))
                != dereth_protocol::OrderedActionHeader::MAGIC
            {
                continue;
            }
            actions.push((
                u32::from_le_bytes(f.payload[8..12].try_into().expect("four bytes")),
                f.payload[12..].to_vec(),
            ));
        }
    }
    actions
}

/// Open the character page and put the tab that owns `sub` up.
fn open_character_page(c: &mut HeadlessClient, sub: ElementId) {
    let page = dereth_ui_screens::panels::remaining::CHARACTER_PAGE;
    {
        let (ui, screen) = hud_gameplay(c);
        let panel_id = screen
            .panels
            .pages
            .iter()
            .find(|p| p.element == page)
            .map(|p| p.panel_id)
            .expect("the character page is one of the panel bar's pages");
        screen.recv_set_panel_visibility(ui, panel_id, true);
    }
    c.tick(1);
    let tab = {
        let (ui, screen) = hud_gameplay(c);
        let r = screen.root().expect("the gameplay root");
        let h = ui
            .get_child_recursive(r, page)
            .expect("the character page is in the layout");
        let t = {
            let n = ui.node(h).expect("the page has a node");
            let b = n.behaviour.as_ref().expect("the page is a panel");
            let p = (**b)
                .as_any()
                .and_then(|a| a.downcast_ref::<dereth_ui::widgets::panel::Panel>())
                .expect("its behaviour is a panel");
            p.tab_to_page
                .iter()
                .find(|(_, pg)| **pg == sub)
                .map(|(t, _)| *t)
        };
        t.and_then(|t| ui.get_child_recursive(r, t))
    };
    let th = tab.expect("the panel names a tab for this sub-page");
    {
        let (ui, _) = hud_gameplay(c);
        ui.broadcast_element_message(th, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    }
    c.tick(3);
}

/// One footer button of one sub-page, found the way the page finds it: the container the page's
/// own look picks, then the button inside it.
fn footer_button(c: &mut HeadlessClient, panel_id: ElementId, btn: u32) -> ElemHandle {
    let (ui, screen) = hud_gameplay(c);
    let r = screen.root().expect("the gameplay root");
    let page = ui
        .get_child_recursive(r, dereth_ui_screens::panels::remaining::CHARACTER_PAGE)
        .expect("the character page");
    let panel = ui
        .get_child_recursive(page, panel_id)
        .expect("the sub-page");
    let state = ui.node(panel).map_or(0, |n| n.state.0);
    let container = ui
        .get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))
        .expect("the footer this look selects");
    ui.get_child_recursive(container, ElementId(btn))
        .expect("the button inside it")
}

fn hud_centre(c: &mut HeadlessClient, h: ElemHandle) -> (i32, i32) {
    let (ui, _) = hud_gameplay(c);
    let b = ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// Press a button where a player presses it, having first proved the pointer lands on it.
fn press_footer(c: &mut HeadlessClient, hands: &mut Hands, h: ElemHandle, want: u32) {
    let at = hud_centre(c, h);
    let hit = {
        let (ui, _) = hud_gameplay(c);
        ui.hit_test_screen(at.0, at.1)
            .and_then(|e| ui.node(e))
            .map(dereth_ui::ElementNode::element_id)
    };
    assert_eq!(
        hit,
        Some(ElementId(want)),
        "the middle of the button must hit the button"
    );
    hands.click_at(c, at.0, at.1);
    // The request the press queues becomes a datagram on the frame after the one that queues it.
    c.tick(2);
}

/// A client in the world with a recorded character's own skills and attributes, and a socket-free
/// endpoint to read what it sends.
fn a_character_with_credits_to_spend() -> HeadlessClient {
    let n = dereth_client_net::client_session::testing::Corpus::load("first-login-walk-jump")
        .expect("the recordings are committed to the repository")
        .expect("first-login-walk-jump is one of them")
        .blobs
        .len();
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut net =
        dereth_client::net::ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", 0)
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
    c.when(dereth_testkit::Inbound::from_corpus(
        "first-login-walk-jump",
        0..n,
    ));
    c.tick(4);
    let _ = wire_actions(&mut c);
    c
}

/// The one-at-a-time and ten-at-a-time raise buttons both put a request on the wire.
pub fn both_raise_buttons_on_both_pages_put_the_request_on_the_wire() {
    let mut c = a_character_with_credits_to_spend();
    let mut hands = Hands::new();
    let mut every_button = true;

    for (panel_id, opcode) in [(skills::PANEL, 0x0046_u32), (attributes::PANEL, 0x0045)] {
        open_character_page(&mut c, panel_id);

        // Pick a row the way a player does.
        let row = if panel_id == skills::PANEL {
            c.view()
                .expect_app()
                .hud()
                .panels
                .skills
                .rows
                .iter()
                .find(|r| r.group == skills::SkillGroup::Trained)
                .map(|r| r.element)
        } else {
            c.view()
                .expect_app()
                .hud()
                .panels
                .attributes
                .rows
                .first()
                .map(|r| r.element)
        }
        .expect("the recorded character has a row that can be raised");
        let at = hud_centre(&mut c, row);
        hands.click_at(&mut c, at.0, at.1);
        c.tick(1);
        let _ = wire_actions(&mut c);

        let footer = if panel_id == skills::PANEL {
            c.view()
                .expect_app()
                .hud()
                .panels
                .skills
                .footer_content
                .clone()
        } else {
            c.view()
                .expect_app()
                .hud()
                .panels
                .attributes
                .footer_content
                .clone()
        };
        every_button &= footer.button == statmgmt::button_state::ENABLED
            && footer.button_10 == statmgmt::button_state::ENABLED
            && footer.button_10_visible;

        // One at a time.
        let one = footer_button(&mut c, panel_id, statmgmt::child::BUTTON);
        press_footer(&mut c, &mut hands, one, statmgmt::child::BUTTON);
        let sent = wire_actions(&mut c);
        every_button &= sent.len() == 1 && sent[0].0 == opcode && sent[0].1.len() == 8;
        let one_xp = u32::from_le_bytes(sent[0].1[4..8].try_into().expect("four bytes"));

        // The shard's answer is what lets the next press through, and there is no shard here, so
        // it is cleared the way that answer clears it.
        if panel_id == skills::PANEL {
            c.app_mut().hud_mut().panels.skills.clear_awaiting_raise();
        } else {
            c.app_mut()
                .hud_mut()
                .panels
                .attributes
                .clear_awaiting_raise();
        }

        // Ten at a time.
        let ten = footer_button(&mut c, panel_id, statmgmt::child::BUTTON_10);
        press_footer(&mut c, &mut hands, ten, statmgmt::child::BUTTON_10);
        let sent = wire_actions(&mut c);
        every_button &= sent.len() == 1 && sent[0].0 == opcode && sent[0].1.len() == 8;
        let ten_xp = u32::from_le_bytes(sent[0].1[4..8].try_into().expect("four bytes"));
        let wanted = if panel_id == skills::PANEL {
            c.view().expect_app().hud().panels.skills.selected_skill
        } else {
            c.view()
                .expect_app()
                .hud()
                .panels
                .attributes
                .selected()
                .map(dereth_ui_screens::panels::attributes::AttributeRow::wire_stat)
                .expect("a picked row")
        };
        every_button &= sent[0].1[0..4] == wanted.to_le_bytes();
        // ...and the two presses are asking for different amounts, which is the whole difference.
        every_button &= ten_xp > one_xp;

        if panel_id == skills::PANEL {
            c.app_mut().hud_mut().panels.skills.clear_awaiting_raise();
        } else {
            c.app_mut()
                .hud_mut()
                .panels
                .attributes
                .clear_awaiting_raise();
        }
    }

    c.assert_behaviour(
        "character-page.raise.both-buttons-on-both-pages-put-the-request-on-the-wire",
        move |_| every_button,
    );
    c.shutdown();
}

#[test]
fn scenario_both_raise_buttons_on_both_pages_put_the_request_on_the_wire() {
    scenario("both_raise_buttons_on_both_pages_put_the_request_on_the_wire");
}

// ---------------------------------------------------------------------------------------------
// hud.vitals.the-first-press-changes-nothing-on-screen-and-the-second-hides-the-numbers
// ---------------------------------------------------------------------------------------------

/// The vitals bar takes two presses to lose its numbers, which is the original's own arithmetic
/// over the original's own layout.
pub fn the_first_press_on_the_vitals_bar_changes_nothing_on_screen() {
    use dereth_ui_screens::hud::vitals::vitals_display as v;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let stacked = hud_find(&c, window::STACKED_VITALS);
    let health_label = hud_find(&c, ElementId(0x1000_00EB));

    // The layout names no look for this window to start in, anywhere in its subtree.
    let no_default_look = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        fn every_default(ui: &UiSystem, h: ElemHandle, out: &mut Vec<(u32, u32)>) {
            if let Some(n) = ui.node(h) {
                out.push((n.element_id().0, n.desc.default_state.0));
            }
            for child in ui.children(h) {
                every_default(ui, child, out);
            }
        }
        let mut all = Vec::new();
        every_default(ui, stacked, &mut all);
        all.iter().all(|(_, s)| *s == 0) && all.len() > 20
    } && c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(stacked)
        .map(|n| n.state)
        == Some(StateId(0));

    // What the two looks it can be put into do, and what the one it starts in does not do.
    let the_two_looks = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let d = &ui.node(health_label).expect("the health label").desc;
        let hide_in = |s: StateId| {
            d.access_state(s)
                .and_then(|st| st.properties.get_bool(dereth_ui::props::attr::HIDE))
        };
        hide_in(v::STATE_A) == Some(false)
            && hide_in(v::STATE_B) == Some(true)
            && d.base
                .properties
                .get_bool(dereth_ui::props::attr::HIDE)
                .is_none()
    };

    let starts_numeric = hud_visible(&c, health_label) && v::next_state(StateId(0)) == v::STATE_A;
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .set_state(stacked, v::STATE_A);
    c.tick(1);
    // The first press: the look changes and nothing on the screen does.
    let first_press_shows_nothing = hud_visible(&c, health_label);
    let next = v::next_state(v::STATE_A);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .set_state(stacked, next);
    c.tick(1);
    let second_press_hides_them = !hud_visible(&c, health_label);

    c.assert_behaviour(
        "hud.vitals.the-first-press-changes-nothing-on-screen-and-the-second-hides-the-numbers",
        move |_| {
            no_default_look
                && the_two_looks
                && starts_numeric
                && first_press_shows_nothing
                && second_press_hides_them
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_first_press_on_the_vitals_bar_changes_nothing_on_screen() {
    scenario("the_first_press_on_the_vitals_bar_changes_nothing_on_screen");
}

// =============================================================================================
// reader.book.*, notice.failure.a-refused-portal.*, hud.vitals.a-press.*
//
// Four rows, and every one of them is driven with the recorded message where a recording exists.
// =============================================================================================

/// The recorded opening of the one book the corpus carries: the twelve-page primer.
fn the_recorded_book() -> (
    dereth_client_net::client_session::SessionEvent,
    dereth_protocol::trade::WritingBookOpen,
) {
    use dereth_protocol::Message as _;
    let corpus = dereth_client_net::client_session::testing::Corpus::load("long-solo-play")
        .expect("the recordings are committed to the repository")
        .expect("long-solo-play is one of them");
    for row in &corpus.blobs {
        if row.dir != dereth_client_net::client_session::testing::Direction::ServerToClient
            || row.opcode != 0xF7B0
        {
            continue;
        }
        if row.payload.len() < 16 {
            continue;
        }
        let sub = u32::from_le_bytes(row.payload[12..16].try_into().expect("four bytes"));
        if sub == 0x00B4 {
            let e = dereth_testkit::inbound::event_of(row);
            let body = match &e {
                dereth_client_net::client_session::SessionEvent::UiEvent { blob, .. } => {
                    blob[4..].to_vec()
                }
                _ => panic!("the recorded opening is a queued event"),
            };
            let mut r = dereth_protocol::archive::Reader::new(&body);
            let decoded = dereth_protocol::trade::WritingBookOpen::read(&mut r)
                .expect("the recorded opening decodes");
            return (e, decoded);
        }
    }
    panic!("long-solo-play carries the corpus's one recorded book and it was not found");
}

/// Everything one text element holds, as a character and the colour it is drawn in.
fn drawn_glyphs(c: &mut HeadlessClient, h: ElemHandle) -> Vec<(char, u32)> {
    let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
    ui.text_element_mut(h).map_or_else(Vec::new, |t| {
        t.glyphs
            .glyphs
            .iter()
            .map(|g| (char::from_u32(u32::from(g.data)).unwrap_or('?'), g.color))
            .collect()
    })
}

fn drawn_text(c: &mut HeadlessClient, h: ElemHandle) -> String {
    drawn_glyphs(c, h).into_iter().map(|(ch, _)| ch).collect()
}

/// The colour every letter of one phrase is drawn in; the phrase being there at all is half the
/// reading.
fn drawn_colour(c: &mut HeadlessClient, h: ElemHandle, needle: &str) -> u32 {
    let gs = drawn_glyphs(c, h);
    let text: String = gs.iter().map(|(ch, _)| *ch).collect();
    let at = text
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not in the log; the log holds {text:?}"));
    let n = needle.chars().count();
    let colours: Vec<u32> = gs[at..at + n].iter().map(|(_, col)| *col).collect();
    assert!(
        colours.windows(2).all(|w| w[0] == w[1]),
        "the phrase is drawn in one colour"
    );
    colours[0]
}

fn hud_state(c: &HeadlessClient, h: ElemHandle) -> StateId {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .map_or(StateId(0), |n| n.state)
}

/// Whether an element is this one or lives under it.
fn under(c: &HeadlessClient, mut h: ElemHandle, id: ElementId) -> bool {
    let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
    loop {
        if ui.node(h).map(dereth_ui::ElementNode::element_id) == Some(id) {
            return true;
        }
        match ui.parent(h) {
            Some(p) => h = p,
            None => return false,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// reader.book.a-recorded-book-opens-the-reader-and-shows-its-first-page
// ---------------------------------------------------------------------------------------------

/// A scroll or a letter the shard sends really opens the reader and puts its page on the screen.
pub fn a_recorded_book_opens_the_reader_and_shows_its_first_page() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));

    // The layout half: the reader's window is fully bound in the shipped tree.
    let bound = {
        let (_, s) = hud_gameplay(&mut c);
        s.book.fully_bound()
    };
    let win = hud_find(&c, dereth_ui_screens::panels::book::WINDOW);
    let page = hud_find(&c, dereth_ui_screens::panels::book::PAGE_TEXT);
    let title = hud_find(&c, dereth_ui_screens::panels::book::TITLE_TEXT);
    let starts_hidden = !hud_visible(&c, win);

    let (event, recorded) = the_recorded_book();
    let twelve_pages = recorded.pages.pages.len() == 12;
    let first = recorded.pages.pages[0]
        .page_text
        .clone()
        .expect("page one carries its own text");
    let the_recorded_words = first.starts_with("To chat with those around you");

    let before = c.view().expect_app().interaction().stats.books_opened;
    c.when(dereth_testkit::Inbound::event(event));
    let the_arm_ran = c.view().expect_app().interaction().stats.books_opened == before + 1;
    c.tick(1);

    let the_model_took_it = {
        let app = c.view().expect_app();
        let b = app
            .objects()
            .world
            .book
            .open
            .as_ref()
            .expect("the model took the message");
        // The number of pages is the message's own second field and not the list's length.
        b.book_id == recorded.book_id && b.max_num_pages == 12
    };
    let the_panel_opened_it = {
        let (_, s) = hud_gameplay(&mut c);
        s.book.opens == 1 && s.book.cur_page == 0 && s.book.menu_labels.len() == 12
    } && hud_visible(&c, win);

    let on_screen = drawn_text(&mut c, page);
    let the_page_is_there = on_screen == first && on_screen.contains("press the ENTER key");
    // An unsigned book takes the thing's own name, and this client has never seen the thing, so
    // the title is empty -- which is the client's own answer rather than a missing write.
    let untitled = recorded.scribe_id == dereth_primitives::ObjectId(0)
        && drawn_text(&mut c, title).is_empty();

    c.assert_behaviour(
        "reader.book.a-recorded-book-opens-the-reader-and-shows-its-first-page",
        move |_| {
            bound
                && starts_hidden
                && twelve_pages
                && the_recorded_words
                && the_arm_ran
                && the_model_took_it
                && the_panel_opened_it
                && the_page_is_there
                && untitled
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_recorded_book_opens_the_reader_and_shows_its_first_page() {
    scenario("a_recorded_book_opens_the_reader_and_shows_its_first_page");
}

// ---------------------------------------------------------------------------------------------
// reader.book.paging-moves-the-page-and-greys-the-control-that-cannot-be-pressed
// ---------------------------------------------------------------------------------------------

/// Turning the pages, and the two controls that say which way one can be turned.
pub fn paging_a_book_moves_the_page_and_greys_what_cannot_be_pressed() {
    use dereth_ui_screens::panels::book::{STATE_NORMAL, STATE_UNAVAILABLE};

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let (event, recorded) = the_recorded_book();
    c.when(dereth_testkit::Inbound::event(event));
    c.tick(1);

    let prev = hud_find(&c, dereth_ui_screens::panels::book::PREV_BUTTON);
    let next = hud_find(&c, dereth_ui_screens::panels::book::NEXT_BUTTON);
    let page = hud_find(&c, dereth_ui_screens::panels::book::PAGE_TEXT);
    let on_the_first =
        hud_state(&c, prev) == STATE_UNAVAILABLE && hud_state(&c, next) == STATE_NORMAL;

    let mut hands = Hands::new();
    let at = hud_centre(&mut c, next);
    hands.click_at(&mut c, at.0, at.1);
    let want = recorded.pages.pages[1]
        .page_text
        .clone()
        .expect("page two's own text");
    let turned = {
        let (_, s) = hud_gameplay(&mut c);
        s.book.cur_page == 1
    } && drawn_text(&mut c, page) == want
        && hud_state(&c, prev) == STATE_NORMAL;

    // Off the end it stops, rather than wrapping or running past the last page.
    for _ in 0..20 {
        let at = hud_centre(&mut c, next);
        hands.click_at(&mut c, at.0, at.1);
    }
    let stops = {
        let (_, s) = hud_gameplay(&mut c);
        s.book.cur_page == 11
    } && hud_state(&c, next) == STATE_UNAVAILABLE;

    c.assert_behaviour(
        "reader.book.paging-moves-the-page-and-greys-the-control-that-cannot-be-pressed",
        move |_| on_the_first && turned && stops,
    );
    c.shutdown();
}

#[test]
fn scenario_paging_a_book_moves_the_page_and_greys_what_cannot_be_pressed() {
    scenario("paging_a_book_moves_the_page_and_greys_what_cannot_be_pressed");
}

// ---------------------------------------------------------------------------------------------
// notice.failure.a-refused-portal-says-so-in-the-chat-log-in-its-own-colour
// ---------------------------------------------------------------------------------------------

/// A portal that refuses the player puts its refusal in the chat log.
pub fn a_refused_portal_says_so_in_the_chat_log() {
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

#[test]
fn scenario_a_refused_portal_says_so_in_the_chat_log() {
    scenario("a_refused_portal_says_so_in_the_chat_log");
}

// ---------------------------------------------------------------------------------------------
// hud.vitals.a-press-on-the-bar-flips-it-between-its-two-presentations
// ---------------------------------------------------------------------------------------------

/// Pressing the vitals bar flips it between its two presentations.
pub fn a_press_on_the_vitals_bar_flips_it_between_its_two_presentations() {
    use dereth_ui_screens::hud::vitals::vitals_display as v;

    // The size matters: at the smaller one the notice strip lies over the lower part of the bar.
    let mut c = HeadlessClient::new(ClientSpec {
        width: 1024,
        height: 768,
        ..ClientSpec::gameplay(4)
    });

    // The two presentations are the **layout's**, so they are read before anything is asserted to
    // set them: a data change is then a different failure from a client one.
    let mut both_authored = true;
    for id in [window::STACKED_VITALS, window::SIDE_VITALS] {
        let h = hud_find(&c, id);
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let desc = &shell.ui.node(h).expect("the window").desc;
        both_authored &=
            desc.access_state(v::STATE_A).is_some() && desc.access_state(v::STATE_B).is_some();
    }

    let stacked = hud_find(&c, window::STACKED_VITALS);
    let untouched = hud_state(&c, stacked) == StateId(0);

    // **Where the press lands, asserted before it is made.** The bar's own middle is what a
    // player aims at; the hit test is the only thing that decides which element a press reaches,
    // and a miss and a missing answer are the same red without this line.
    let at = {
        let b = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .screen_box(stacked);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    let hit = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .hit_test_screen(at.0, at.1);
    let lands_on_the_bar = hit.is_some_and(|h| under(&c, h, window::STACKED_VITALS));

    let mut hands = Hands::new();
    hands.click_at(&mut c, at.0, at.1);
    let answered = {
        let (_, s) = hud_gameplay(&mut c);
        s.vitals_display_toggles == 1
    } && hud_state(&c, stacked) == v::STATE_A;

    hands.click_at(&mut c, at.0, at.1);
    let the_other = hud_state(&c, stacked) == v::STATE_B;
    hands.click_at(&mut c, at.0, at.1);
    let a_toggle = hud_state(&c, stacked) == v::STATE_A;

    // The whole element swaps and not only its frame: the meters inside it author both
    // presentations too, so a change that stopped at the window would leave the numbers behind.
    let meter_group = hud_find(&c, ElementId(0x1000_00E6));
    let the_whole_thing = hud_state(&c, meter_group) == v::STATE_A;

    c.assert_behaviour(
        "hud.vitals.a-press-on-the-bar-flips-it-between-its-two-presentations",
        move |_| {
            both_authored
                && untouched
                && lands_on_the_bar
                && answered
                && the_other
                && a_toggle
                && the_whole_thing
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_on_the_vitals_bar_flips_it_between_its_two_presentations() {
    scenario("a_press_on_the_vitals_bar_flips_it_between_its_two_presentations");
}

// =============================================================================================
// tooltip.* -- what resting the pointer on something draws
//
// The instrument's own calibration -- that the shipped panels really do carry tooltips and that
// none of them can be reached with the screen at rest -- is the premise of the first scenario
// rather than a row of its own.
// =============================================================================================

/// The attribute that turns a tooltip on at all.
const TOOLTIPS_ON: u32 = 0x4B;
/// The window a tooltip is drawn in.
const TOOLTIP_ELEMENT: u32 = 0x47;
/// The words it carries.
const TOOLTIP_ENTRY: u32 = 0x49;
/// The attribute on a label that says "show the whole of me when I do not fit".
const AUTO_TOOLTIP_TRUNCATED: u32 = 0xD0;

fn every_element(ui: &UiSystem) -> Vec<ElemHandle> {
    fn walk(ui: &UiSystem, h: ElemHandle, out: &mut Vec<ElemHandle>) {
        out.push(h);
        for child in ui.children(h) {
            walk(ui, child, out);
        }
    }
    let mut all = Vec::new();
    walk(ui, ui.root(), &mut all);
    all
}

/// Everything the tree would draw right now, recorded rather than drawn.
fn recorded_draw_list(ui: &mut UiSystem) -> Vec<dereth_ui::UiDrawCmd> {
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    back.calls
}

/// Every live element whose whole tooltip is words the layout already ships.
fn ships_its_own_tooltip(ui: &UiSystem) -> Vec<ElemHandle> {
    every_element(ui)
        .into_iter()
        .filter(|h| {
            let Some(n) = ui.node(*h) else { return false };
            let p = n.merged_properties();
            p.get_bool(TOOLTIPS_ON) == Some(true)
                && p.get(TOOLTIP_ELEMENT).is_some()
                && p.get(TOOLTIP_ENTRY).is_some()
        })
        .collect()
}

/// What the layout says this element's tooltip reads, resolved the way the client resolves it.
fn authored_tooltip(ui: &UiSystem, h: ElemHandle) -> Option<String> {
    let si = ui
        .node(h)?
        .merged_properties()
        .get_string_info(TOOLTIP_ENTRY)?
        .clone();
    si.literal
        .clone()
        .or_else(|| ui.resolve_string(si.table_id?, si.string_id?))
}

fn element_centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let b = ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// Open the panel this element lives in, from the top down, as the client does.
fn open_the_panel_that_owns(ui: &mut UiSystem, h: ElemHandle) {
    let mut chain = Vec::new();
    let mut q = Some(h);
    while let Some(a) = q {
        chain.push(a);
        q = ui.parent(a);
    }
    for a in chain.iter().rev() {
        ui.set_visible(*a, true);
    }
    ui.use_time(
        dereth_primitives::LocalTime(0.0),
        &mut dereth_ui::NullInputPump,
    );
}

/// The pointer comes to rest and stays there while frames go by -- which is the only thing a
/// tooltip answers.
fn rest_pointer_at(ui: &mut UiSystem, from: f64, x: i32, y: i32) {
    ui.mouse_move(dereth_primitives::LocalTime(from), x, y);
    let mut t = from;
    for _ in 0..8 {
        t += 0.25;
        ui.use_time(
            dereth_primitives::LocalTime(t),
            &mut dereth_ui::NullInputPump,
        );
    }
}

/// What appeared: the rectangle it covers, how many drawings it took and the letters in it.
struct Appeared {
    rect: dereth_ui::Box2D,
    commands: usize,
    glyphs: String,
}

fn what_appeared(before: &[dereth_ui::UiDrawCmd], after: &[dereth_ui::UiDrawCmd]) -> Appeared {
    use std::collections::BTreeSet;
    let known: BTreeSet<ElemHandle> = before.iter().map(|c| c.who).collect();
    let new: Vec<&dereth_ui::UiDrawCmd> =
        after.iter().filter(|c| !known.contains(&c.who)).collect();
    let mut rect = dereth_ui::Box2D::empty();
    for c in &new {
        rect = if rect.is_valid() {
            dereth_ui::Box2D {
                x0: rect.x0.min(c.screen.x0),
                y0: rect.y0.min(c.screen.y0),
                x1: rect.x1.max(c.screen.x1),
                y1: rect.y1.max(c.screen.y1),
            }
        } else {
            c.screen
        };
    }
    let glyphs: String = new
        .iter()
        .flat_map(|c| {
            c.glyphs
                .iter()
                .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        })
        .collect();
    Appeared {
        rect,
        commands: new.len(),
        glyphs,
    }
}

// ---------------------------------------------------------------------------------------------
// tooltip.resting-the-pointer-on-something-draws-a-box-with-words-in-it
// ---------------------------------------------------------------------------------------------

/// Resting the pointer on something draws a box with words in it, not an empty frame.
pub fn resting_the_pointer_on_something_draws_a_box_with_words_in_it() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(24));
    let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;

    // **The instrument's own calibration**: the shipped panels really do carry tooltips, and with
    // the screen at rest the pointer reaches none of them -- which is why every reading below opens
    // a panel first. An empty result is not a negative result until the instrument has been shown
    // to be pointing at the target.
    let candidates = ships_its_own_tooltip(ui);
    let the_instrument_can_look = !candidates.is_empty();
    let none_at_rest = candidates.iter().all(|h| {
        let (x, y) = element_centre(ui, *h);
        ui.hit_test_screen(x, y) != Some(*h)
    });

    let mut all_drawn = true;
    let mut proved = 0;
    let mut clock = 0.0;
    for h in candidates {
        open_the_panel_that_owns(ui, h);
        let (x, y) = element_centre(ui, h);
        if ui.hit_test_screen(x, y) != Some(h) {
            // Some of them sit under a neighbour even with the panel open. They are not this
            // claim's business, and they are not counted as passes either.
            continue;
        }
        let before = recorded_draw_list(ui);
        clock += 10.0;
        rest_pointer_at(ui, clock, x, y);
        let after = recorded_draw_list(ui);
        let got = what_appeared(&before, &after);
        // A box with no words and words with no box are two different faults; the first is a
        // frame drawn with nothing in it.
        all_drawn &= got.commands > 0
            && got.rect.width() > 0
            && got.rect.height() > 0
            && !got.glyphs.is_empty();
        proved += 1;
        clock += 10.0;
        ui.mouse_move(dereth_primitives::LocalTime(clock), 0, 0);
        ui.use_time(
            dereth_primitives::LocalTime(clock),
            &mut dereth_ui::NullInputPump,
        );
    }
    let measured_something = proved > 0;

    c.assert_behaviour(
        "tooltip.resting-the-pointer-on-something-draws-a-box-with-words-in-it",
        move |_| the_instrument_can_look && none_at_rest && all_drawn && measured_something,
    );
    c.shutdown();
}

#[test]
fn scenario_resting_the_pointer_on_something_draws_a_box_with_words_in_it() {
    scenario("resting_the_pointer_on_something_draws_a_box_with_words_in_it");
}

// ---------------------------------------------------------------------------------------------
// tooltip.the-words-are-the-ones-that-element-was-given
// ---------------------------------------------------------------------------------------------

/// The words are that element's own, not a neighbour's and not an empty frame.
pub fn the_words_in_a_tooltip_are_the_ones_that_element_was_given() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(24));
    let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
    let candidates = ships_its_own_tooltip(ui);

    let mut every_one = true;
    let mut checked = 0;
    let mut clock = 0.0;
    for h in candidates {
        let Some(expected) = authored_tooltip(ui, h) else {
            continue;
        };
        if expected.is_empty() {
            continue;
        }
        open_the_panel_that_owns(ui, h);
        let (x, y) = element_centre(ui, h);
        if ui.hit_test_screen(x, y) != Some(h) {
            continue;
        }
        let before = recorded_draw_list(ui);
        clock += 10.0;
        rest_pointer_at(ui, clock, x, y);
        let after = recorded_draw_list(ui);
        every_one &= what_appeared(&before, &after).glyphs == expected;
        checked += 1;
        clock += 10.0;
        ui.mouse_move(dereth_primitives::LocalTime(clock), 0, 0);
        ui.use_time(
            dereth_primitives::LocalTime(clock),
            &mut dereth_ui::NullInputPump,
        );
    }

    c.assert_behaviour(
        "tooltip.the-words-are-the-ones-that-element-was-given",
        move |_| every_one && checked > 0,
    );
    c.shutdown();
}

#[test]
fn scenario_the_words_in_a_tooltip_are_the_ones_that_element_was_given() {
    scenario("the_words_in_a_tooltip_are_the_ones_that_element_was_given");
}

// ---------------------------------------------------------------------------------------------
// tooltip.a-label-too-long-for-its-box-can-be-read-and-catches-the-pointer
// ---------------------------------------------------------------------------------------------

/// The one tooltip in the client that belongs to no particular panel: hover a label the client
/// had to cut short, and read the whole of it.
pub fn a_label_too_long_for_its_box_can_be_read_and_catches_the_pointer() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(24));
    let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;

    let armed: Vec<ElemHandle> = every_element(ui)
        .into_iter()
        .filter(|h| {
            ui.node(*h).is_some_and(|n| {
                n.merged_properties().get_bool(AUTO_TOOLTIP_TRUNCATED) == Some(true)
            }) && ui.screen_box(*h).is_valid()
        })
        .collect();
    let the_instrument_can_look = !armed.is_empty();

    let widest = armed
        .iter()
        .map(|h| ui.screen_box(*h).width())
        .max()
        .unwrap_or(0);
    let sentence = "The Lost City of Frore, and every step of the road that leads to it. ";
    let reps = 1 + usize::try_from(widest.max(0)).expect("a width") / sentence.len();
    let long = sentence.repeat(reps);

    let mut chosen = None;
    for h in &armed {
        if !ui
            .text_element_mut(*h)
            .is_some_and(|t| t.bits.truncate() && t.bits.one_line())
        {
            continue;
        }
        open_the_panel_that_owns(ui, *h);
        // A label is only reachable once it has been cut short, so the reach is tried with the
        // long text in place and then taken back out.
        ui.text_element_mut(*h)
            .expect("a text element")
            .set_text(&long);
        ui.use_time(
            dereth_primitives::LocalTime(1.0),
            &mut dereth_ui::NullInputPump,
        );
        let (x, y) = element_centre(ui, *h);
        let reachable = ui.hit_test_screen(x, y) == Some(*h);
        ui.text_element_mut(*h)
            .expect("a text element")
            .set_text("");
        ui.use_time(
            dereth_primitives::LocalTime(1.0),
            &mut dereth_ui::NullInputPump,
        );
        if reachable {
            chosen = Some(*h);
            break;
        }
    }
    let h = chosen.expect("one of the labels must be reachable, or this measures nothing");
    let width = ui.screen_box(h).width();

    // Text that fits: no tooltip, and the label is transparent to the pointer.
    ui.text_element_mut(h)
        .expect("a text element")
        .set_text("ok");
    ui.use_time(
        dereth_primitives::LocalTime(2.0),
        &mut dereth_ui::NullInputPump,
    );
    let fits = {
        let n = ui.node(h).expect("alive");
        n.tooltip_text.is_none() && !n.region.flags.tooltip && !n.is_mouse_visible
    };

    // Text that does not: the whole of it becomes the tooltip, and the label starts catching the
    // pointer -- which is what stops a press falling through it.
    let really_too_long = i32::try_from(long.len()).expect("a length") > width;
    ui.text_element_mut(h)
        .expect("a text element")
        .set_text(&long);
    ui.use_time(
        dereth_primitives::LocalTime(3.0),
        &mut dereth_ui::NullInputPump,
    );
    let clipped = {
        let n = ui.node(h).expect("alive");
        n.tooltip_text.as_deref() == Some(long.as_str())
            && n.region.flags.tooltip
            && n.is_mouse_visible
    };

    let (x, y) = element_centre(ui, h);
    let reaches = ui.hit_test_screen(x, y) == Some(h);
    let before = recorded_draw_list(ui);
    rest_pointer_at(ui, 4.0, x, y);
    let after = recorded_draw_list(ui);
    let drew_the_whole_thing = what_appeared(&before, &after).glyphs == long;

    c.assert_behaviour(
        "tooltip.a-label-too-long-for-its-box-can-be-read-and-catches-the-pointer",
        move |_| {
            the_instrument_can_look
                && fits
                && really_too_long
                && clipped
                && reaches
                && drew_the_whole_thing
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_label_too_long_for_its_box_can_be_read_and_catches_the_pointer() {
    scenario("a_label_too_long_for_its_box_can_be_read_and_catches_the_pointer");
}

// =============================================================================================
// hud.link-lamp.* -- the lamp that always showed red
//
// The state before the shard sets the clock, and the same claim over a longer silence, are legs
// of the first scenario here rather than rows of their own.
// =============================================================================================

/// The lamp, and the four pictures it can be in.
const LINK_LAMP: ElementId = ElementId(0x1000_00F8);

/// A shard on the far end of the client's own socket-free endpoint, sending real datagrams.
struct LampShard {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    from: std::net::SocketAddr,
}

impl LampShard {
    fn attach(c: &mut HeadlessClient) -> Self {
        let net = dereth_client::net::ClientNetwork::new(
            "127.0.0.1:19000",
            7304,
            "acct0001",
            "unused",
            0,
        )
        .expect("a socket-free endpoint");
        c.attach_replay(net);
        let mut shard = Self {
            crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
            sequence: 1,
            from: "127.0.0.1:19000".parse().expect("the peer address"),
        };
        shard.connect_request(c);
        // The first datagram that is not the opening answer finishes the handshake.
        shard.echo_request(c);
        assert_eq!(
            c.replay_net_mut().expect("the endpoint").status(),
            dereth_client::net::LinkStatus::Connected,
            "the scenario is worthless unless the link is genuinely up"
        );
        shard
    }

    fn send(&self, c: &mut HeadlessClient, raw: Vec<u8>) {
        let now = dereth_primitives::LocalTime(c.view().expect_app().clock().local_time);
        let from = self.from;
        c.replay_net_mut()
            .expect("the endpoint")
            .feed(&raw, from, now);
    }

    fn connect_request(&mut self, c: &mut HeadlessClient) {
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: 0,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_optional_header(
                dereth_transport::wire::PacketFlags::CONNECT_REQUEST,
                dereth_transport::conn::ConnectRequest {
                    server_time: 0.0,
                    cookie: 0,
                    net_id: 0,
                    outgoing_seed: 0xDEAD_BEEF,
                    incoming_seed: 0x1234_5678,
                }
                .to_bytes()
                .to_vec(),
            )
            .expect("one optional header");
        self.send(c, packet.serialize(None).expect("a serialisable packet"));
    }

    fn echo_request(&mut self, c: &mut HeadlessClient) {
        self.sequence += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_optional_header(
                dereth_transport::wire::PacketFlags::ECHO_REQUEST,
                0f32.to_le_bytes().to_vec(),
            )
            .expect("one optional header");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("a serialisable packet");
        self.send(c, raw);
    }

    /// The shard setting the game clock, which every shard does within seconds of a login.
    fn set_the_clock(&mut self, c: &mut HeadlessClient, t: f64) {
        self.sequence += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_optional_header(
                dereth_transport::wire::PacketFlags::TIME_SYNC,
                t.to_le_bytes().to_vec(),
            )
            .expect("one optional header");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("a serialisable packet");
        self.send(c, raw);
    }

    /// A real failure of the link, as the shard reports one.
    fn fail(&mut self, c: &mut HeadlessClient) {
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: 0,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_optional_header(
                dereth_transport::wire::PacketFlags::NET_ERROR,
                dereth_transport::conn::NetErrorCode::ServerFull
                    .pack()
                    .to_vec(),
            )
            .expect("one optional header");
        self.send(c, packet.serialize(None).expect("a serialisable packet"));
    }
}

/// The picture the lamp is really in.
fn lamp_state(c: &mut HeadlessClient) -> u32 {
    let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
    let root = ui
        .get_element(ElementId(0x1000_0495))
        .expect("the gameplay root");
    let h = ui
        .get_child_recursive(root, LINK_LAMP)
        .expect("the lamp is in the tree");
    ui.node(h).expect("live").state.0
}

/// Run frames until the lamp has settled, advancing the link's own clock as the socket loop does.
fn settle_the_lamp(c: &mut HeadlessClient, frames: u32, step: f64) -> u32 {
    let mut network_now = c.view().expect_app().clock().local_time;
    for _ in 0..frames {
        network_now += step;
        c.replay_net_mut()
            .expect("the endpoint")
            .receive_use_time(dereth_primitives::LocalTime(network_now));
        c.tick(1);
    }
    lamp_state(c)
}

/// The number of ticks a shard's clock carries, which is what made the lamp read a silence of
/// nine hundred million seconds.
const A_SHARDS_CLOCK: f64 = 1_073_741_828.0;

// ---------------------------------------------------------------------------------------------
// hud.link-lamp.the-shard-setting-the-clock-does-not-change-it-and-nor-does-a-quiet-link
// ---------------------------------------------------------------------------------------------

/// The lamp does not turn red when the shard sets the clock, nor when the link is merely quiet.
pub fn setting_the_clock_does_not_change_the_lamp() {
    use dereth_ui_screens::hud::indicators::link_status::media;

    // Before the shard sets the clock. This is the state the rest is compared against: without
    // it a good lamp afterwards could be one that was never moved at all.
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let _shard = LampShard::attach(&mut c);
    let no_correction_yet = c.view().expect_app().clock().external_offset() == 0.0;
    let good_before = settle_the_lamp(&mut c, 200, 0.1) == media::GOOD;
    c.shutdown();

    // And with it set, which is what once reddened the lamp.
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut shard = LampShard::attach(&mut c);
    shard.set_the_clock(&mut c, A_SHARDS_CLOCK);
    c.tick(1);
    // The scenario is worthless unless the shard really moved the clock.
    let the_clock_moved = c.view().expect_app().clock().external_offset() > 40.0;
    let still_good = settle_the_lamp(&mut c, 400, 0.1) == media::GOOD;

    // And a link that is up but quiet for the best part of a minute is still a link: silence
    // alone is not the same thing as a link that has gone.
    let quiet_is_still_good = settle_the_lamp(&mut c, 1500, 0.04) == media::GOOD;

    c.assert_behaviour(
        "hud.link-lamp.the-shard-setting-the-clock-does-not-change-it-and-nor-does-a-quiet-link",
        move |_| {
            no_correction_yet && good_before && the_clock_moved && still_good && quiet_is_still_good
        },
    );
    c.shutdown();
}

#[test]
fn scenario_setting_the_clock_does_not_change_the_lamp() {
    scenario("setting_the_clock_does_not_change_the_lamp");
}

// ---------------------------------------------------------------------------------------------
// hud.link-lamp.a-real-failure-puts-the-player-off-the-world-rather-than-reddening-a-lamp
// ---------------------------------------------------------------------------------------------

/// A link that really fails does not leave a red lamp on a world the player is still in.
pub fn a_real_failure_takes_the_player_off_the_world() {
    use dereth_ui_screens::hud::indicators::link_status::media;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut shard = LampShard::attach(&mut c);
    let good_first = settle_the_lamp(&mut c, 200, 0.1) == media::GOOD;
    // The change is measured, so "the link is down" cannot be read off a link that was never up.
    let up = c.replay_net_mut().expect("the endpoint").status()
        == dereth_client::net::LinkStatus::Connected;

    shard.fail(&mut c);
    c.tick(1);

    let down = {
        let net = c.replay_net_mut().expect("the endpoint");
        net.status() == dereth_client::net::LinkStatus::Disconnected && net.error().is_none()
    } && c.replay_net_mut().expect("the endpoint").session_state()
        == dereth_client_net::client_session::SessionState::Disconnected(
            dereth_client_net::client_session::DisconnectReason::ServerDied,
        );
    let holder_cleared = {
        let now = c.view().expect_app().clock().cur_time;
        dereth_client::net::link_status_holder::connection_status(now).is_none()
    };
    // And the world is gone with it: there is no lamp left to be red.
    let off_the_world = {
        let shell = c
            .app_mut()
            .ui_mut()
            .expect("the shell stays up to show the refusal");
        let screen = shell.flow.current_mut().expect("some screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .is_none()
    };

    c.assert_behaviour(
        "hud.link-lamp.a-real-failure-puts-the-player-off-the-world-rather-than-reddening-a-lamp",
        move |_| good_first && up && down && holder_cleared && off_the_world,
    );
    c.shutdown();
}

#[test]
fn scenario_a_real_failure_takes_the_player_off_the_world() {
    scenario("a_real_failure_takes_the_player_off_the_world");
}

// =============================================================================================
// options.support.* and urgent-assistance.* -- the two asks that reached nothing
//
// The plumbing's own good manners -- an arm that claims its own ask and hands every other one
// back -- is a leg of the first scenario rather than a row.
//
// The window is built out of the shipped data and driven with a real press on the control's own
// rectangle; what it asks for is then handed to the client's own consumer. Nothing calls a
// handler by hand. **No browser is opened, no box is put on a real screen and no datagram leaves
// the process**: the door to the desktop is stubbed before any press, and the one send goes to a
// transport that keeps what it is given.
// =============================================================================================

use dereth_ui_screens::options::gameplay::button as support_button;
use dereth_ui_screens::options::pages::{SHELL_EXECUTE_ERROR_TITLE, SUPPORT_URL};
use dereth_ui_screens::panels::urgent_assistance as ua;

/// The shipped element tree, with the real text tables behind it.
fn a_shipped_tree() -> (
    UiSystem,
    dereth_ui_screens::screens::gameplay::GamePlayScreen,
    dereth_ui_screens::panels::remaining::RemainingPanels,
) {
    use dereth_primitives::AssetSource as _;
    use dereth_ui::framework::{DidMapperResolver, Screen as _};

    #[derive(Debug)]
    struct Store(dereth_dat::RetailDatStore);
    impl dereth_primitives::AssetSource for Store {
        fn read(
            &self,
            id: dereth_primitives::DataId,
        ) -> Result<Vec<u8>, dereth_primitives::AssetError> {
            self.0.read(id)
        }
        fn exists(&self, id: dereth_primitives::DataId) -> bool {
            self.0.exists(id)
        }
        fn iter_type(
            &self,
            kind: dereth_primitives::DataType,
        ) -> Box<dyn Iterator<Item = dereth_primitives::DataId> + '_> {
            self.0.iter_type(kind)
        }
    }

    let dir = dereth_dat::testing::dat_dir();
    let store = dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail data files open");
    let master_id = dereth_primitives::DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("the master property record");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("it decodes");
    let mut ui = UiSystem::new((800, 600));
    ui.property_types = master.property_types();
    let mut flow = dereth_ui::UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let source = std::rc::Rc::new(Store(store));
    let resolver =
        std::rc::Rc::new(DidMapperResolver::load_via_master(source.as_ref()).expect("the mapper"));
    dereth_ui_screens::env::install(&mut ui, source, resolver);

    let mut s = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let root = s.root().expect("the gameplay root");
    let mut panels = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    panels.post_init(&mut ui, root);
    ui.drain_outbox();
    ui.requests.clear();
    (ui, s, panels)
}

/// Show this element and every ancestor: a page comes up hidden and a hidden ancestor cannot be
/// pressed.
fn reveal_for_press(ui: &mut UiSystem, mut h: ElemHandle) {
    loop {
        ui.set_visible(h, true);
        match ui.parent(h) {
            Some(p) => h = p,
            None => break,
        }
    }
    ui.drain_outbox();
}

/// One frame of the message pass, to both of the things that answer one.
fn pump_messages(
    ui: &mut UiSystem,
    s: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
    panels: &mut dereth_ui_screens::panels::remaining::RemainingPanels,
) {
    use dereth_ui::framework::Screen as _;
    let view = dereth_ui_screens::view::EmptyGameView;
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            break;
        }
        for d in batch {
            if let dereth_ui::Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
                panels.on_element_message(ui, &msg, &view);
            }
        }
    }
}

/// A real press in the middle of a control's own rectangle, and what it asked for.
fn press_and_take(
    ui: &mut UiSystem,
    s: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
    panels: &mut dereth_ui_screens::panels::remaining::RemainingPanels,
    parent: ElemHandle,
    id: ElementId,
) -> Vec<dereth_ui_screens::UiRequest> {
    let h = ui
        .get_child_recursive(parent, id)
        .unwrap_or_else(|| panic!("{:#010X} is in the shipped tree", id.0));
    reveal_for_press(ui, h);
    let b = ui.node(h).expect("the control").region.box_;
    let (ox, oy) = ui.screen_origin(h);
    let (x, y) = (ox + b.width() / 2, oy + b.height() / 2);
    ui.requests.clear();
    ui.mouse_down(7, x, y);
    ui.mouse_up(7, x, y, false);
    pump_messages(ui, s, panels);
    ui.requests.take()
}

/// The one options page that carries the support buttons.
fn the_support_page(ui: &UiSystem) -> ElemHandle {
    let all: Vec<ElemHandle> = ui
        .element_list()
        .iter()
        .copied()
        .filter(|h| {
            ui.node(*h)
                .is_some_and(|n| n.ty() == dereth_ui_screens::element_types::ty::GAMEPLAY_OPTIONS)
        })
        .collect();
    assert_eq!(
        all.len(),
        1,
        "the shipped tree carries exactly one of those pages"
    );
    all[0]
}

// ---------------------------------------------------------------------------------------------
// options.support.each-support-button-opens-its-in-game-form
// ---------------------------------------------------------------------------------------------

/// Both buttons, each to its form, and neither to the desktop's browser.
pub fn each_support_button_opens_its_in_game_form() {
    use dereth_client::app::{apply_open_url_requests, record_shell_calls};

    let (mut ui, mut s, mut panels) = a_shipped_tree();
    let page = the_support_page(&ui);
    let mut opened = true;
    for (id, form) in [
        (
            support_button::SUPPORT_TICKET_UPPER,
            dereth_ui_screens::panels::urgent_assistance::PANEL_TYPE,
        ),
        (
            support_button::SUPPORT_TICKET_LOWER,
            dereth_ui_screens::panels::abuse::PANEL_TYPE,
        ),
    ] {
        let asked = press_and_take(&mut ui, &mut s, &mut panels, page, id);
        opened &= !asked
            .iter()
            .any(|r| matches!(r, dereth_ui_screens::UiRequest::OpenUrl(_)));
        // The form is the one element of its type.
        opened &= ui
            .element_list()
            .iter()
            .any(|h| ui.node(*h).is_some_and(|n| n.ty() == form) && ui.is_visible(*h));
        // Nothing reaches the desktop.
        record_shell_calls();
        let (_, calls) = apply_open_url_requests(asked);
        opened &= calls.is_empty();
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.support.each-support-button-opens-its-in-game-form",
        move |_| opened,
    );
}

#[test]
fn scenario_each_support_button_opens_its_in_game_form() {
    scenario("each_support_button_opens_its_in_game_form");
}

// ---------------------------------------------------------------------------------------------
// options.support.a-browser-that-will-not-open-says-so-in-a-box-with-the-address-in-it
// ---------------------------------------------------------------------------------------------

/// The other half: a desktop that refuses.
pub fn a_browser_that_will_not_open_says_so_in_a_box_with_the_address_in_it() {
    use dereth_client::app::{
        apply_open_url_requests, record_shell_calls_answering, shell_error_text, ShellCall,
    };

    let request = || vec![dereth_ui_screens::UiRequest::OpenUrl(SUPPORT_URL)];

    record_shell_calls_answering(0);
    let (_, calls) = apply_open_url_requests(request());
    let the_box = calls
        == vec![
            ShellCall::Open {
                url: SUPPORT_URL.to_owned(),
                result: 0,
            },
            ShellCall::ErrorBox {
                title: "Asheron's Call Error".to_owned(),
                text: format!(
                    "An error occurred while trying to launch your web browser. \
                     (Error code 0)\nThe web site to submit an urgent assistance request is \
                     listed below. Please go there to complete your request.\n{SUPPORT_URL}\n"
                ),
            },
        ]
        && SHELL_EXECUTE_ERROR_TITLE == "Asheron's Call Error"
        && shell_error_text(0, SUPPORT_URL)
            == match &calls[1] {
                ShellCall::ErrorBox { text, .. } => text.clone(),
                other => panic!("{other:?}"),
            };

    // The line between a refusal and a success, read on both sides of it so an off-by-one cannot
    // hide: the value below it puts the box up and the value above it does not.
    record_shell_calls_answering(32);
    let refuses = apply_open_url_requests(request()).1.len() == 2;
    record_shell_calls_answering(33);
    let succeeds = apply_open_url_requests(request()).1.len() == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.support.a-browser-that-will-not-open-says-so-in-a-box-with-the-address-in-it",
        move |_| the_box && refuses && succeeds,
    );
}

#[test]
fn scenario_a_browser_that_will_not_open_says_so_in_a_box_with_the_address_in_it() {
    scenario("a_browser_that_will_not_open_says_so_in_a_box_with_the_address_in_it");
}

// ---------------------------------------------------------------------------------------------
// urgent-assistance.send.the-report-goes-out-on-the-help-channel
// ---------------------------------------------------------------------------------------------

/// The bytes a report really puts on the wire, written out here rather than read back through the
/// same writer that produced them.
const HELP_ME: [u8; 28] = [
    0xB1, 0xF7, 0x00, 0x00, // the ordered action envelope
    0x01, 0x00, 0x00, 0x00, // the first action of the session
    0x47, 0x01, 0x00, 0x00, // what the message is
    0x00, 0x04, 0x00, 0x00, // the help channel
    0x07, 0x00, // seven characters
    b'h', b'e', b'l', b'p', b' ', b'm', b'e', //
    0x00, 0x00, 0x00, // padding to the next four
];

/// The whole window, from the warning page to the datagram.
pub fn the_urgent_assistance_report_goes_out_on_the_help_channel() {
    use dereth_client::interaction::{send_request, Interaction};

    let (mut ui, mut s, mut panels) = a_shipped_tree();
    let panel = panels
        .urgent_assistance
        .panel
        .expect("the window is in the tree");
    ui.set_state(panel, ua::PAGE_WARNING);
    let _ = press_and_take(&mut ui, &mut s, &mut panels, panel, ua::NEXT_BUTTON);

    // Type the report into the window's own box and raise what a typed box raises.
    let box_h = ui
        .get_child_recursive(panel, ua::ENTRY_BOX)
        .expect("the box is a child of the window");
    if let Some(t) = ui.text_element_mut(box_h) {
        t.set_text("help me");
    }
    ui.broadcast_element_message(box_h, dereth_ui::msg::element::id::TEXT_CHANGED, 0, 0);
    pump_messages(&mut ui, &mut s, &mut panels);

    let asked = press_and_take(&mut ui, &mut s, &mut panels, panel, ua::CONTINUE_BUTTON);
    let queued = asked.iter().any(|r| {
        matches!(
            r,
            dereth_ui_screens::UiRequest::ChannelBroadcast { channel, text }
                if *channel == ua::HELP_CHANNEL && text == "help me"
        )
    }) && ua::HELP_CHANNEL == 0x0400;

    // ...and on to the client's own consumer, and then its own sender.
    let mut inter = Interaction::new();
    let mut game = dereth_client_model::World::new();
    inter.queue(Vec::new(), asked);
    let claimed = inter
        .run_ui_requests(&mut game, false, dereth_primitives::ServerTime(100.0))
        .is_empty();
    let requests = inter.take_pending_requests();
    let one_request = requests.len() == 1
        && matches!(&requests[0], dereth_client_model::Request::ChannelBroadcast(m)
            if m.channel == 0x0400 && m.message == "help me");

    let mut session = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );
    let sent = send_request(&mut session, &requests[0]);
    let on_the_wire = sent
        && session.transport.sent.len() == 1
        && (
            session.transport.sent[0].queue,
            session.transport.sent[0].ordered,
        ) == (dereth_primitives::NetQueue::Weenie, true)
        && session.transport.sent[0].payload == HELP_ME;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "urgent-assistance.send.the-report-goes-out-on-the-help-channel",
        move |_| queued && claimed && one_request && on_the_wire,
    );
}

#[test]
fn scenario_the_urgent_assistance_report_goes_out_on_the_help_channel() {
    scenario("the_urgent_assistance_report_goes_out_on_the_help_channel");
}

// ---------------------------------------------------------------------------------------------
// urgent-assistance.send.the-refusal-that-applies-to-a-typed-command-does-not-apply-here
// ---------------------------------------------------------------------------------------------

/// A trap the window must not fall into: the help channel is refused to a typed command, which is
/// what makes asking for help a command rather than a channel -- and the window is the one thing
/// in the client that is allowed to send on it.
pub fn the_refusal_for_a_typed_command_does_not_apply_to_the_window() {
    use dereth_client::interaction::Interaction;

    let still_refused = !dereth_client_model::chat::channel_command_broadcasts_on(0x0400);

    let mut inter = Interaction::new();
    let mut game = dereth_client_model::World::new();
    inter.queue(
        Vec::new(),
        vec![dereth_ui_screens::UiRequest::ChannelBroadcast {
            channel: 0x0400,
            text: "help me".into(),
        }],
    );
    let claimed = inter
        .run_ui_requests(&mut game, false, dereth_primitives::ServerTime(100.0))
        .is_empty();
    let not_refused = inter.take_pending_requests().len() == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "urgent-assistance.send.the-refusal-that-applies-to-a-typed-command-does-not-apply-here",
        move |_| still_refused && claimed && not_refused,
    );
}

#[test]
fn scenario_the_refusal_for_a_typed_command_does_not_apply_to_the_window() {
    scenario("the_refusal_for_a_typed_command_does_not_apply_to_the_window");
}

// =============================================================================================
// ui.text.outline.* -- the dark backing behind drawn text
//
// The refusal strip is drawn with an outline and the chat log is not, which is why the strip can
// look blurry beside the log. The raw count of elements that ask for an outline is the premise of
// the first scenario rather than a row, and the two sizings of the refusal string are legs of the
// third.
// =============================================================================================

/// The attribute a text element asks for an outline with.
const TEXT_OUTLINE: u32 = 0x21;
/// A refusal-strip string the outline scenarios measure.
const OWNER_STRING: &str = "You can't put that item there.";
/// The font the refusal strip is drawn in.
const STRIP_FONT: dereth_primitives::DataId = dereth_primitives::DataId(0x4000_0001);
/// The strip itself, and the layout it lives in.
const STRIP: (dereth_primitives::DataId, u32) =
    (dereth_primitives::DataId(0x2100_0011), 0x1000_004A);
/// The chat log, which asks for none -- which is why it looked fine.
const CHAT_LOG: u32 = 0x1000_0011;

fn the_data_files() -> dereth_dat::RetailDatStore {
    dereth_dat::testing::open_store().expect("the retail data files: set DERETH_TEST_DAT_DIR")
}

fn outline_property_types(s: &dereth_dat::RetailDatStore) -> dereth_assets::ui::PropertyTypes {
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;
    let id = dereth_primitives::DataId(0x3900_0001);
    let b = s.read(id).expect("the master property record");
    dereth_assets::MasterProperty::decode_payload(id, &b)
        .expect("it decodes")
        .property_types()
}

/// Every place in the shipped layouts where the attribute is declared, and what it says.
///
/// Every named look is searched as well as the element's own, because the client applies whichever
/// is current and missing one would undercount -- which reads exactly like the census being right.
fn outline_census(s: &dereth_dat::RetailDatStore) -> Vec<(dereth_primitives::DataId, u32, bool)> {
    use dereth_assets::ui::{ElementDesc, LayoutDesc, PropertyValue};
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;

    fn walk(e: &ElementDesc, out: &mut Vec<(u32, bool)>) {
        let mut v = None;
        for st in std::iter::once(&e.state).chain(e.states.iter().map(|(_, s)| s)) {
            for (_, p) in &st.properties {
                if p.id == TEXT_OUTLINE {
                    if let PropertyValue::Bool(b) = p.value {
                        v = Some(b);
                    }
                }
            }
        }
        if let Some(b) = v {
            out.push((e.element_id, b));
        }
        for (_, c) in &e.children {
            walk(c, out);
        }
    }

    let types = outline_property_types(s);
    let ids = s.ids_of(LayoutDesc::TYPE);
    assert!(
        ids.len() >= 100,
        "the shipped layout set: {} found",
        ids.len()
    );
    let mut out = Vec::new();
    let mut decoded = 0_usize;
    for id in &ids {
        let bytes = s.read(*id).expect("a layout the directory lists reads");
        let layout = LayoutDesc::decode_payload(*id, &bytes, &types).expect("it decodes");
        decoded += 1;
        for (_, e) in &layout.elements {
            let mut v = Vec::new();
            walk(e, &mut v);
            for (elem, b) in v {
                out.push((*id, elem, b));
            }
        }
    }
    assert_eq!(decoded, ids.len(), "every shipped layout was read");
    out
}

/// The attribute **resolved the way the client resolves it**: one look at a time, and along the
/// chain of things an element is built from. Written from the shipped descriptions alone, so it is
/// an oracle rather than a restatement of the thing under test.
fn resolved_outlines(s: &dereth_dat::RetailDatStore) -> std::collections::BTreeMap<u32, bool> {
    use dereth_assets::ui::{ElementDesc, LayoutDesc, PropertyValue};
    use dereth_assets::Decode as _;
    use dereth_primitives::{AssetSource as _, DataId};

    let types = outline_property_types(s);
    let mut layouts: std::collections::BTreeMap<DataId, LayoutDesc> =
        std::collections::BTreeMap::new();
    for id in s.ids_of(LayoutDesc::TYPE) {
        let bytes = s.read(id).expect("a layout reads");
        layouts.insert(
            id,
            LayoutDesc::decode_payload(id, &bytes, &types).expect("it decodes"),
        );
    }

    fn find(l: &LayoutDesc, id: u32) -> Option<&ElementDesc> {
        fn rec(e: &ElementDesc, id: u32) -> Option<&ElementDesc> {
            if e.element_id == id {
                return Some(e);
            }
            e.children.iter().find_map(|(_, c)| rec(c, id))
        }
        l.elements.iter().find_map(|(_, e)| rec(e, id))
    }

    fn declared(e: &ElementDesc, state: u32) -> Option<bool> {
        let read = |st: &dereth_assets::ui::StateDesc| -> Option<bool> {
            st.properties
                .iter()
                .find_map(|(_, p)| match (p.id, &p.value) {
                    (TEXT_OUTLINE, PropertyValue::Bool(b)) => Some(*b),
                    _ => None,
                })
        };
        let mut v = read(&e.state);
        for (sid, st) in &e.states {
            if *sid == state {
                if let Some(b) = read(st) {
                    v = Some(b);
                }
            }
        }
        v
    }

    fn resolve<'a>(
        layouts: &'a std::collections::BTreeMap<DataId, LayoutDesc>,
        mut layout: DataId,
        mut e: &'a ElementDesc,
        depth: usize,
    ) -> Option<bool> {
        let mut state = e.default_state;
        for _ in 0..depth {
            if let Some(b) = declared(e, state) {
                return Some(b);
            }
            if e.base_element == 0 {
                return None;
            }
            let bl = if e.base_layout.0 == 0 {
                layout
            } else {
                e.base_layout
            };
            let l = layouts.get(&bl)?;
            let next = find(l, e.base_element)?;
            if state == 0 {
                state = next.default_state;
            }
            layout = bl;
            e = next;
        }
        None
    }

    fn walk(
        layouts: &std::collections::BTreeMap<DataId, LayoutDesc>,
        id: DataId,
        e: &ElementDesc,
        out: &mut std::collections::BTreeMap<u32, bool>,
    ) {
        if let Some(b) = resolve(layouts, id, e, 16) {
            let slot = out.entry(e.element_id).or_insert(false);
            *slot = *slot || b;
        }
        for (_, c) in &e.children {
            walk(layouts, id, c, out);
        }
    }

    let mut out = std::collections::BTreeMap::new();
    for (id, l) in &layouts {
        for (_, e) in &l.elements {
            walk(&layouts, *id, e, &mut out);
        }
    }
    out
}

/// Build every shipped layout and record, per element, whether what it drew carried an outline.
fn drawn_outlines(s: &dereth_dat::RetailDatStore) -> std::collections::BTreeMap<u32, Option<u32>> {
    use dereth_assets::ui::LayoutDesc;
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;

    let types = outline_property_types(s);
    let mut out = std::collections::BTreeMap::new();
    for id in s.ids_of(LayoutDesc::TYPE) {
        let bytes = s.read(id).expect("a layout reads");
        let layout = LayoutDesc::decode_payload(id, &bytes, &types).expect("it decodes");
        let mut ui = UiSystem::new((800, 600));
        ui.property_types = types.clone();
        for (elem, _) in &layout.elements {
            // A layout whose root will not build is a standing condition of this client and not
            // this claim's subject; it is skipped, and the counts below say how many were seen.
            let _ = dereth_ui::framework::create_and_add_root_element_by_data_id(
                &mut ui,
                s,
                id,
                ElementId(*elem),
            );
        }
        let mut back = dereth_ui::RecordingDrawBackend::default();
        ui.draw(&mut back);
        for cmd in &back.calls {
            let Some(n) = ui.node(cmd.who) else { continue };
            let key = n.desc.element_id.0;
            // One element can draw twice; the drawing that carries the letters is the one with
            // the outline, so a colour wins over none.
            let e = out.entry(key).or_insert(None);
            if cmd.text_outline.is_some() {
                *e = cmd.text_outline;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// ui.text.outline.the-refusal-strip-is-drawn-with-one-and-the-chat-log-is-not
// ---------------------------------------------------------------------------------------------

/// The refusal strip and the chat log, side by side.
pub fn the_refusal_strip_is_drawn_with_an_outline_and_the_chat_log_is_not() {
    let s = the_data_files();
    let all = outline_census(&s);
    let asked: Vec<_> = all.iter().filter(|(_, _, b)| *b).collect();
    // The premise, and it is two numbers rather than one: declaring the attribute and asking for
    // an outline are different things, because two of them declare it false.
    let counted = all.len() == 71 && asked.len() == 69;
    let the_strip_asks = asked.iter().any(|(l, e, _)| (*l, *e) == STRIP);
    let the_log_does_not = !all.iter().any(|(_, e, _)| *e == CHAT_LOG);

    let drawn = drawn_outlines(&s);
    let the_strip_draws_one = drawn.get(&STRIP.1).copied().flatten().is_some();
    // The log draws nothing at all in this reading -- with no words in it there is nothing to
    // draw -- so the other half is read off the resolved data, which does not depend on it
    // drawing.
    let resolved = resolved_outlines(&s);
    let the_log_resolves_to_none = resolved.get(&CHAT_LOG) != Some(&true)
        && drawn
            .iter()
            .filter(|(e, _)| **e == CHAT_LOG)
            .all(|(_, v)| v.is_none());

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.outline.the-refusal-strip-is-drawn-with-one-and-the-chat-log-is-not",
        move |_| {
            counted
                && the_strip_asks
                && the_log_does_not
                && the_strip_draws_one
                && the_log_resolves_to_none
        },
    );
}

#[test]
fn scenario_the_refusal_strip_is_drawn_with_an_outline_and_the_chat_log_is_not() {
    scenario("the_refusal_strip_is_drawn_with_an_outline_and_the_chat_log_is_not");
}

// ---------------------------------------------------------------------------------------------
// ui.text.outline.every-element-that-asks-for-one-is-drawn-with-one-and-no-other-is
// ---------------------------------------------------------------------------------------------

/// The set, both ways, against an oracle written from the shipped data alone.
pub fn every_element_that_asks_for_an_outline_is_drawn_with_one() {
    let s = the_data_files();
    let resolved = resolved_outlines(&s);
    let want: std::collections::BTreeSet<u32> = resolved
        .iter()
        .filter(|(_, v)| **v)
        .map(|(k, _)| *k)
        .collect();
    let refused: std::collections::BTreeSet<u32> = resolved
        .iter()
        .filter(|(_, v)| !**v)
        .map(|(k, _)| *k)
        .collect();
    let both_arms = !want.is_empty() && !refused.is_empty();

    let drawn = drawn_outlines(&s);
    let with: std::collections::BTreeSet<u32> = drawn
        .iter()
        .filter(|(_, v)| v.is_some())
        .map(|(k, _)| *k)
        .collect();
    let observable: std::collections::BTreeSet<u32> = want
        .iter()
        .filter(|k| drawn.contains_key(k))
        .copied()
        .collect();

    let none_unasked = with.difference(&want).count() == 0;
    let none_silent = observable.difference(&with).count() == 0;
    // Equality, not containment: a claim about the whole set rather than a sample of it.
    let the_same_set = with == observable;
    // ...and the ones the data says must **not** be outlined are not, which is what stops a
    // reader that outlines every piece of text from passing.
    let none_refused_drew_one = refused.iter().all(|r| !with.contains(r));
    let the_strip_is_among_them = with.contains(&STRIP.1);
    // The measured shape of the shipped set, so none of it can drift in silence.
    let the_shape =
        want.len() == 106 && refused.len() == 14 && observable.len() == 30 && with.len() == 30;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.outline.every-element-that-asks-for-one-is-drawn-with-one-and-no-other-is",
        move |_| {
            both_arms
                && none_unasked
                && none_silent
                && the_same_set
                && none_refused_drew_one
                && the_strip_is_among_them
                && the_shape
        },
    );
}

#[test]
fn scenario_every_element_that_asks_for_an_outline_is_drawn_with_one() {
    scenario("every_element_that_asks_for_an_outline_is_drawn_with_one");
}

// ---------------------------------------------------------------------------------------------
// ui.text.outline.it-comes-from-the-fonts-own-heavier-sheet-and-fits-inside-it
// ---------------------------------------------------------------------------------------------

/// Where the dark backing comes from, and how much of it there is.
pub fn the_outline_comes_from_the_fonts_own_heavier_sheet() {
    use dereth_primitives::DataId;

    let s = the_data_files();
    let textures = dereth_client::textures::TextureStore::new(&s);
    let fonts = s.ids_of(dereth_dat::DbType::Font);
    let the_shipped_font_set = fonts.len() == 49;

    let (mut with_bg, mut without_bg, mut checked) = (0_usize, 0_usize, 0_u64);
    let mut all_fit = true;
    for f in &fonts {
        let font = dereth_client::ui_draw::load_font(&s, *f).expect("a shipped font loads");
        let atlas = dereth_client::ui_draw::build_font_atlas(&s, *f).expect("and rasterises");
        // The two spreads survive into what the client draws from.
        all_fit &= atlas.num_horizontal_border_pixels == font.num_horizontal_border_pixels
            && atlas.num_vertical_border_pixels == font.num_vertical_border_pixels;
        if font.background_surface_data_id == 0 {
            without_bg += 1;
            // A font with no heavier sheet declares no spread either, which is consistent: the
            // spread exists to widen a sheet that is not there.
            all_fit &= !atlas.has_outline_sheet()
                && (
                    font.num_horizontal_border_pixels,
                    font.num_vertical_border_pixels,
                ) == (0, 0);
            continue;
        }
        with_bg += 1;
        all_fit &= atlas.has_outline_sheet();
        let fg = textures
            .bgra8(DataId(font.foreground_surface_data_id))
            .expect("the letters' sheet");
        // The heavier sheet shares the letters' own size, because both are addressed by the same
        // place in it.
        all_fit &= atlas.texture_size == (fg.width, fg.height);
        let hb = i64::from(font.num_horizontal_border_pixels);
        let vb = i64::from(font.num_vertical_border_pixels);
        for d in &font.char_descs {
            let (l, t) = (i64::from(d.offset_x) - hb, i64::from(d.offset_y) - vb);
            let r = i64::from(d.offset_x) + i64::from(d.width) + hb;
            let b = i64::from(d.offset_y) + i64::from(d.height) + vb;
            // The client clamps a widened rectangle against the window it draws into and never
            // against the sheet, so a wrong spread would read outside the sheet on some letter of
            // some font. It reads outside on none of them.
            all_fit &= l >= 0 && t >= 0 && r <= i64::from(fg.width) && b <= i64::from(fg.height);
            checked += 1;
        }
    }
    let the_split = (with_bg, without_bg) == (37, 12) && checked > 0;

    // And how much backing there is, on a refusal the strip really draws.
    let font = dereth_client::ui_draw::load_font(&s, STRIP_FONT).expect("the strip's font");
    let fg = textures
        .bgra8(DataId(font.foreground_surface_data_id))
        .expect("the letters");
    let bg = textures
        .bgra8(DataId(font.background_surface_data_id))
        .expect("the backing");
    let hb = i32::try_from(font.num_horizontal_border_pixels).expect("a small number");
    let vb = i32::try_from(font.num_vertical_border_pixels).expect("a small number");
    let the_spread = (hb, vb) == (4, 4);

    // The sheet is taken apart into its three plain pieces, because the type it comes in belongs
    // to a crate this one does not name.
    let measure = |sw: u32, sh: u32, pixels: &[[u8; 4]], bx: i32, by: i32, bb: i32| {
        let mut map = std::collections::BTreeMap::<(i32, i32), u8>::new();
        let (mut sum_g, mut n_g) = (0_u64, 0_u64);
        let mut pen = 0_i32;
        for ch in OWNER_STRING.encode_utf16() {
            let Some(d) = font.get_char_desc(ch) else {
                continue;
            };
            let left = pen + i32::from(d.horizontal_offset_before);
            let top = i32::from(d.vertical_offset_before);
            let (w, h) = (i32::from(d.width), i32::from(d.height));
            for r in -by..(h + bb) {
                for col in -bx..(w + bx) {
                    let (sx, sy) = (i32::from(d.offset_x) + col, i32::from(d.offset_y) + r);
                    if sx < 0
                        || sy < 0
                        || sx >= i32::try_from(sw).expect("a small number")
                        || sy >= i32::try_from(sh).expect("a small number")
                    {
                        continue;
                    }
                    let a = pixels[(sy as usize) * (sw as usize) + (sx as usize)][3];
                    if a != 0 {
                        n_g += 1;
                        sum_g += u64::from(a);
                    }
                    let e = map.entry((left + col, top + r)).or_insert(0);
                    *e = (*e).max(a);
                }
            }
            pen += d.advance();
        }
        let inked: Vec<u8> = map.values().copied().filter(|a| *a != 0).collect();
        let n = inked.len() as u64;
        let opaque = inked.iter().filter(|a| **a == 255).count() as u64;
        let sum: u64 = inked.iter().map(|a| u64::from(*a)).sum();
        #[allow(clippy::cast_precision_loss)]
        let out = (
            n,
            opaque,
            sum as f64 / n.max(1) as f64,
            n_g,
            sum_g as f64 / n_g.max(1) as f64,
        );
        out
    };

    let f = measure(fg.width, fg.height, &fg.pixels, 0, 0, 0);
    let o = measure(bg.width, bg.height, &bg.pixels, hb, vb, hb);
    // The letters themselves. Counted two ways, which agree, because the letters' own rectangles
    // do not overlap.
    let the_letters = (f.0, f.1) == (543, 206) && f.0 == f.3 && (f.2 - 172.6).abs() < 0.1;
    // The backing, counted both ways: once letter by letter, which counts the overlaps the widened
    // rectangles make, and once as distinct places on the screen, which is the number comparable
    // with the letters' own.
    #[allow(clippy::cast_precision_loss)]
    let ratio = o.0 as f64 / f.0 as f64;
    let the_backing = o.3 == 2282
        && (o.4 - 128.4).abs() < 0.1
        && o.0 == 1881
        && (o.2 - 146.8).abs() < 0.1
        && (ratio - 3.46).abs() < 0.01;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.outline.it-comes-from-the-fonts-own-heavier-sheet-and-fits-inside-it",
        move |_| {
            the_shipped_font_set && all_fit && the_split && the_spread && the_letters && the_backing
        },
    );
}

#[test]
fn scenario_the_outline_comes_from_the_fonts_own_heavier_sheet() {
    scenario("the_outline_comes_from_the_fonts_own_heavier_sheet");
}

// =============================================================================================
// ui.surface.* -- where an element's drawing surface takes its size from
//
// The authored setting choosing where an element's surface takes its size from is one of four
// answers, not a yes/no.
// =============================================================================================

// The setting, and the four answers, named the way the scenarios below read them.
use dereth_ui::props::{attr, UiObjectMode};

/// A whole raised screen, settled.
fn a_raised_screen(m: dereth_ui::UiMode) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::screen(m, 8));
    c.tick(1);
    c
}

/// The elements of a live tree that own a surface of their own -- the ones the walk stops at.
fn surface_owners(c: &HeadlessClient) -> Vec<ElemHandle> {
    let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
    every_element(ui)
        .into_iter()
        .filter(|h| ui.node(*h).is_some_and(|n| n.flags.should_own_object()))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// ui.surface.every-element-of-a-live-screen-is-sized-from-its-own-box-by-default
// ---------------------------------------------------------------------------------------------

/// The sweep, with its denominator: "they all answered the same" is a result only if there were
/// elements to answer.
pub fn every_element_of_a_live_screen_is_sized_from_its_own_box() {
    let mut c = a_raised_screen(dereth_ui::framework::mode::CHAR_GEN);

    let (elements, answers) = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let all = every_element(ui);
        let mut answers: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
        for h in &all {
            *answers
                .entry(ui.current_ui_object_mode(*h).value())
                .or_default() += 1;
        }
        (all.len(), answers)
    };
    let a_whole_tree = elements > 100;
    // This screen authors the setting nowhere, so every element must land on the default the
    // readers carry -- and it is the element's own box, not "owns nothing".
    let every_one_takes_its_own_box =
        answers.keys().copied().collect::<Vec<u32>>() == vec![UiObjectMode::ElementSize.value()];
    let the_default_is_the_elements_own_box = UiObjectMode::default() == UiObjectMode::ElementSize;

    c.assert_behaviour(
        "ui.surface.every-element-of-a-live-screen-is-sized-from-its-own-box-by-default",
        move |_| a_whole_tree && every_one_takes_its_own_box && the_default_is_the_elements_own_box,
    );
    c.shutdown();
}

#[test]
fn scenario_every_element_of_a_live_screen_is_sized_from_its_own_box() {
    scenario("every_element_of_a_live_screen_is_sized_from_its_own_box");
}

// ---------------------------------------------------------------------------------------------
// ui.surface.an-element-owning-none-is-stepped-over-and-the-answer-is-its-owners
// ---------------------------------------------------------------------------------------------

/// Driven rather than hoped for: no raised screen need carry an element that owns no surface, so
/// the setting is written through the client's own attribute path and read back for that element
/// **and** for a child of it.
pub fn an_element_owning_no_surface_is_stepped_over() {
    let mut c = a_raised_screen(dereth_ui::framework::mode::CHAR_GEN);

    // Something that owns a surface and has a child, so the walk has both an owner to find and
    // somewhere to walk from.
    let (parent, child) = {
        let owners = surface_owners(&c);
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        owners
            .into_iter()
            .find_map(|h| ui.children(h).first().map(|ch| (h, *ch)))
            .expect("this screen has a surface-owning element with a child")
    };

    let (before, the_owning_stopped, up, down) = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        // The precondition: a write that changed nothing could not prove the walk.
        let before = shell.ui.current_ui_object_mode(parent) == UiObjectMode::ElementSize
            && shell.ui.current_ui_object_mode(child) == UiObjectMode::ElementSize
            && shell
                .ui
                .node(parent)
                .expect("live")
                .flags
                .should_own_object();

        shell
            .ui
            .set_attribute_enum(parent, attr::UI_OBJECT_MODE, UiObjectMode::NoObject.value());

        let the_owning_stopped = !shell
            .ui
            .node(parent)
            .expect("live")
            .flags
            .should_own_object()
            && shell.ui.node(parent).expect("live").region.object_mode == UiObjectMode::NoObject;
        (
            before,
            the_owning_stopped,
            shell.ui.current_ui_object_mode(parent),
            shell.ui.current_ui_object_mode(child),
        )
    };
    // The element that owns nothing is walked past: it is never the answer, and its child lands
    // on the very same ancestor it does.
    let stepped_over = up != UiObjectMode::NoObject && down != UiObjectMode::NoObject && up == down;

    c.assert_behaviour(
        "ui.surface.an-element-owning-none-is-stepped-over-and-the-answer-is-its-owners",
        move |_| before && the_owning_stopped && stepped_over,
    );
    c.shutdown();
}

#[test]
fn scenario_an_element_owning_no_surface_is_stepped_over() {
    scenario("an_element_owning_no_surface_is_stepped_over");
}

// ---------------------------------------------------------------------------------------------
// ui.surface.a-colour-picker-is-sized-from-its-own-box-whatever-the-layout-asks-for
// ---------------------------------------------------------------------------------------------

/// An A/B with one field changed. A test on a picker alone could not tell "forced" from "the
/// default", since the forced answer is also the default -- so a twin of the same tree takes the
/// same authored setting and must keep it.
pub fn a_colour_picker_is_sized_from_its_own_box_whatever_it_asks_for() {
    let mut c = a_raised_screen(dereth_ui::framework::mode::CHARACTER_MANAGEMENT);

    let owners = surface_owners(&c);
    let one_owner = owners.len() == 1;
    let control = owners[0];
    let picker = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        *ui.children(control)
            .first()
            .expect("the root has a child to promote")
    };

    let (promoted, both_took_the_write, the_control_kept_it, picker_answer, control_answer) = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        // The A/B needs two elements the walk stops at, so the second is promoted the same way
        // the maker of a layout root promotes one.
        shell.ui.set_should_own_object(picker, true);
        let promoted = shell
            .ui
            .node(picker)
            .expect("live")
            .flags
            .should_own_object();

        let mut both_took_the_write = true;
        for h in [control, picker] {
            shell
                .ui
                .set_attribute_enum(h, attr::UI_OBJECT_MODE, UiObjectMode::StateSize.value());
            both_took_the_write &=
                shell.ui.node(h).expect("live").region.object_mode == UiObjectMode::StateSize;
        }
        let the_control_kept_it =
            shell.ui.current_ui_object_mode(control) == UiObjectMode::StateSize;

        // The one field that differs between the two.
        shell.ui.node_mut(picker).expect("live").desc.ty = dereth_ui::factory::ty::COLOR_PICKER;
        (
            promoted,
            both_took_the_write,
            the_control_kept_it,
            shell.ui.current_ui_object_mode(picker),
            shell.ui.current_ui_object_mode(control),
        )
    };
    let the_picker_is_forced = picker_answer == UiObjectMode::ElementSize;
    let the_twin_is_untouched = control_answer == UiObjectMode::StateSize;

    c.assert_behaviour(
        "ui.surface.a-colour-picker-is-sized-from-its-own-box-whatever-the-layout-asks-for",
        move |_| {
            one_owner
                && promoted
                && both_took_the_write
                && the_control_kept_it
                && the_picker_is_forced
                && the_twin_is_untouched
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_colour_picker_is_sized_from_its_own_box_whatever_it_asks_for() {
    scenario("a_colour_picker_is_sized_from_its_own_box_whatever_it_asks_for");
}

// ---------------------------------------------------------------------------------------------
// ui.surface.a-shipped-root-that-owns-none-is-left-without-one-though-the-maker-asks
// ---------------------------------------------------------------------------------------------

/// The arm no raised screen reaches: a shipped layout root that authors "owns nothing". Driven
/// against a root of the same mechanism that authors nothing at all, because a test that only
/// ever raises the first could not tell "the authored setting took it away" from "roots never get
/// one".
pub fn a_shipped_root_that_owns_no_surface_is_left_without_one() {
    let mut c = a_raised_screen(dereth_ui::framework::mode::CHAR_GEN);
    let store = c.dat_store().expect("the retail dats are open").clone();

    let (authored, the_bit_was_taken_away, owns_nothing, the_walk_lands_on_the_default) = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let zero = shell
            .ui
            .create_root_by_data_id(
                &*store,
                dereth_primitives::DataId(0x2100_0001),
                dereth_ui::ElementId(0x1000_0419),
            )
            .expect("the shipped layout's root element is created");
        let authored = shell
            .ui
            .node(zero)
            .expect("live")
            .merged_properties()
            .get_enum(attr::UI_OBJECT_MODE)
            == Some(UiObjectMode::NoObject.value());
        (
            authored,
            !shell.ui.node(zero).expect("live").flags.should_own_object(),
            shell.ui.node(zero).expect("live").region.object_mode == UiObjectMode::NoObject,
            shell.ui.current_ui_object_mode(zero) == UiObjectMode::ElementSize,
        )
    };

    // The control: a root the screen raised for itself, through the very same maker.
    let (control_authors_nothing, the_control_kept_its_surface) = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let t = every_element(ui)
            .into_iter()
            .find(|h| {
                *h != ui.root()
                    && ui.node(*h).is_some_and(|n| {
                        n.flags.is_root_element()
                            && n.merged_properties()
                                .get_enum(attr::UI_OBJECT_MODE)
                                .is_none()
                    })
            })
            .expect("the screen raised a root of its own, or there is no control");
        (true, ui.node(t).expect("live").flags.should_own_object())
    };

    c.assert_behaviour(
        "ui.surface.a-shipped-root-that-owns-none-is-left-without-one-though-the-maker-asks",
        move |_| {
            authored
                && the_bit_was_taken_away
                && owns_nothing
                && the_walk_lands_on_the_default
                && control_authors_nothing
                && the_control_kept_its_surface
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_shipped_root_that_owns_no_surface_is_left_without_one() {
    scenario("a_shipped_root_that_owns_no_surface_is_left_without_one");
}

// =============================================================================================
// ui.focus.* and ui.scrollbar.* -- the keyboard, and the thing you drag
//
// A press in the chat box takes the keyboard so chat can be typed and sent, and the scrollbar
// beside it draws its thumb rather than an empty groove. Six scenarios, six rows.
//
// The keyboard is driven through the harness's own keyboard steps, which build the client's real
// `WM_CHAR` and hand it to the client's real message path; the pointer is `Hands`. Nothing here is
// a desktop event and nothing here is a windowed run.
// =============================================================================================

/// The chat **log**: the 368 x 73 scrollback, which is sweep-selectable and is not the entry box.
const THE_CHAT_LOG: ElementId = ElementId(0x1000_0011);
/// The chat window's own vertical bar.
const CHAT_BAR: ElementId = ElementId(0x1000_0012);
/// The toolbar strip the stack splitter lives in; the shipped layout starts it down.
const SEL_OBJECT_FIELD: ElementId = ElementId(0x1000_019E);

/// The shipped id an element was built from.
fn shipped_id(c: &HeadlessClient, h: ElemHandle) -> Option<ElementId> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .map(dereth_ui::ElementNode::element_id)
}

/// Who holds the keyboard, if anyone.
fn who_holds_the_keyboard(c: &HeadlessClient) -> Option<ElemHandle> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .focus_element()
}

/// Whether the client is taking typing at all -- the switch a box holding the keyboard turns on.
fn taking_typing(c: &mut HeadlessClient) -> bool {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell is up")
        .manager
        .text
        .text_mode
}

/// How many characters have reached an element, which is what tells "typed and refused" from
/// "never delivered".
fn characters_delivered(c: &HeadlessClient) -> u64 {
    u64::from(
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .stats
            .characters_delivered,
    )
}

/// What is actually in a text box.
fn typed_text(c: &mut HeadlessClient, h: ElemHandle) -> String {
    let (ui, _) = hud_gameplay(c);
    ui.text_element_mut(h).map_or(String::new(), |t| {
        String::from_utf16_lossy(&t.glyphs.glyphs.iter().map(|g| g.data).collect::<Vec<_>>())
    })
}

/// A gameplay client with the stack splitter up: the strip and its two widgets are shown the way
/// the toolbar shows them when a stack is selected, and the splitter is seeded from a stack of 20.
fn a_client_with_the_stack_splitter_up() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let field = hud_find(&c, SEL_OBJECT_FIELD);
    let entry = hud_find(&c, dereth_ui_screens::toolbar::splitter::ENTRY_BOX);
    let slider = hud_find(&c, dereth_ui_screens::toolbar::splitter::SLIDER);
    {
        let (ui, screen) = hud_gameplay(&mut c);
        // The toolbar puts all three down until a stack is selected, and a pointer cannot reach
        // something that is not visible -- so this stands in for the selection the scenario does
        // not make, and nothing else about the widgets is touched.
        ui.set_visible(field, true);
        ui.set_visible(entry, true);
        ui.set_visible(slider, true);
        screen.splitter = dereth_ui_screens::toolbar::splitter::Splitter::new(20);
    }
    c.tick(1);
    c
}

// ---------------------------------------------------------------------------------------------
// ui.focus.a-press-in-a-box-takes-the-keyboard-and-keeps-it
// ---------------------------------------------------------------------------------------------

/// A press in a box takes the keyboard, in one gesture and then ten frames of nothing.
pub fn a_press_in_a_box_takes_the_keyboard_and_keeps_it() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let nobody_held_it = who_holds_the_keyboard(&c).is_none();

    let presses_before = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .mouse_downs;
    // The shipped id names a log in each of the five chat windows, so the gesture says which one
    // it means and the box under test is whichever one the press actually reached.
    c.when(Player::Click(dereth_testkit::Target::Nth {
        id: THE_CHAT_LOG,
        index: 0,
    }));
    let holder = who_holds_the_keyboard(&c);
    // A real press and not a broadcast: the hit test is what found the box.
    let a_real_press = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .mouse_downs
        == presses_before + 1
        && c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .mouse_over()
            == holder;
    let it_took_the_keyboard = holder.is_some_and(|h| shipped_id(&c, h) == Some(THE_CHAT_LOG));
    let log = holder.expect("the press gave the keyboard to the box it landed on");

    // ...and it keeps it, rather than losing it again by the next frame.
    let mut kept_it = true;
    for _ in 0..10 {
        c.tick(1);
        kept_it &= who_holds_the_keyboard(&c) == Some(log)
            && c.view()
                .expect_app()
                .ui()
                .expect("the UI shell is up")
                .ui
                .is_alive(log);
    }

    c.assert_behaviour(
        "ui.focus.a-press-in-a-box-takes-the-keyboard-and-keeps-it",
        move |_| nobody_held_it && a_real_press && it_took_the_keyboard && kept_it,
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_in_a_box_takes_the_keyboard_and_keeps_it() {
    scenario("a_press_in_a_box_takes_the_keyboard_and_keeps_it");
}

// ---------------------------------------------------------------------------------------------
// ui.focus.what-is-typed-reaches-the-box-holding-the-keyboard-and-stops-when-it-lets-go
// ---------------------------------------------------------------------------------------------

/// The other half of the seam, on the one box in the toolbar a player really types a number into.
pub fn what_is_typed_reaches_the_box_holding_the_keyboard() {
    let mut c = a_client_with_the_stack_splitter_up();
    let entry = hud_find(&c, dereth_ui_screens::toolbar::splitter::ENTRY_BOX);
    let it_is_a_box_to_type_in = {
        let (ui, _) = hud_gameplay(&mut c);
        ui.text_element_mut(entry)
            .expect("a text element")
            .bits
            .editable()
    };
    let off_to_start_with = !taking_typing(&mut c);

    c.when(Player::Click(
        dereth_ui_screens::toolbar::splitter::ENTRY_BOX.into(),
    ));
    let it_holds_the_keyboard = who_holds_the_keyboard(&c) == Some(entry);
    let typing_came_on = taking_typing(&mut c);

    let before = characters_delivered(&c);
    c.when(Player::Type("42".into()));
    let both_arrived = characters_delivered(&c) == before + 2;
    let both_are_in_the_box = typed_text(&mut c, entry) == "42";

    // Let the keyboard go. The toolbar reads the box on that edge and settles what it holds -- 42
    // out of a stack of 20 comes back 20 -- and this scenario's claim is that nothing more can be
    // typed into it, not what the toolbar left there.
    {
        let (ui, _) = hud_gameplay(&mut c);
        ui.set_focus_element(None);
    }
    c.tick(1);
    let typing_went_off = !taking_typing(&mut c);
    let settled = typed_text(&mut c, entry);
    let the_owner_settled_it = settled == "20";

    let before = characters_delivered(&c);
    c.when(Player::Type("9".into()));
    let nothing_was_delivered = characters_delivered(&c) == before;
    let the_box_took_nothing = typed_text(&mut c, entry) == settled;

    c.assert_behaviour(
        "ui.focus.what-is-typed-reaches-the-box-holding-the-keyboard-and-stops-when-it-lets-go",
        move |_| {
            it_is_a_box_to_type_in
                && off_to_start_with
                && it_holds_the_keyboard
                && typing_came_on
                && both_arrived
                && both_are_in_the_box
                && typing_went_off
                && the_owner_settled_it
                && nothing_was_delivered
                && the_box_took_nothing
        },
    );
    c.shutdown();
}

#[test]
fn scenario_what_is_typed_reaches_the_box_holding_the_keyboard() {
    scenario("what_is_typed_reaches_the_box_holding_the_keyboard");
}

// ---------------------------------------------------------------------------------------------
// ui.focus.a-log-takes-the-keyboard-and-none-of-what-is-typed
// ---------------------------------------------------------------------------------------------

/// Both arms, so the boundary is a fact and not a hedge: as shipped the keystrokes reach the log
/// and are inserted nowhere, and with the one bit set the same keystrokes land.
pub fn a_log_takes_the_keyboard_and_none_of_what_is_typed() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    // One of the five chat windows' logs, named the way the press names it, and read afterwards
    // so that the box inspected is the very box the press reached.
    c.when(Player::Click(dereth_testkit::Target::Nth {
        id: THE_CHAT_LOG,
        index: 0,
    }));
    let holder = who_holds_the_keyboard(&c);
    let it_still_takes_the_keyboard =
        holder.is_some_and(|h| shipped_id(&c, h) == Some(THE_CHAT_LOG));
    let log = holder.expect("the press gave the keyboard to the box it landed on");
    let (sweepable, not_writable) = {
        let (ui, _) = hud_gameplay(&mut c);
        let bits = ui.text_element_mut(log).expect("a text element").bits;
        (bits.selectable(), !bits.editable())
    };
    c.when(Player::Type("hi".into()));
    let nothing_went_in = typed_text(&mut c, log).is_empty();

    // The one bit, set the way the client's own writer sets it.
    let the_bit_took = {
        let (ui, _) = hud_gameplay(&mut c);
        ui.set_attribute_bool(log, dereth_ui::props::attr::TEXT_EDITABLE, true);
        ui.text_element_mut(log)
            .expect("a text element")
            .bits
            .editable()
    };
    c.tick(1);
    c.when(Player::Type("hi".into()));
    let now_it_lands = typed_text(&mut c, log) == "hi";

    c.assert_behaviour(
        "ui.focus.a-log-takes-the-keyboard-and-none-of-what-is-typed",
        move |_| {
            sweepable
                && not_writable
                && it_still_takes_the_keyboard
                && nothing_went_in
                && the_bit_took
                && now_it_lands
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_log_takes_the_keyboard_and_none_of_what_is_typed() {
    scenario("a_log_takes_the_keyboard_and_none_of_what_is_typed");
}

// ---------------------------------------------------------------------------------------------
// ui.focus.the-name-field-of-the-wizard-takes-a-typed-name
// ---------------------------------------------------------------------------------------------

/// The wizard's name field takes a typed name. The two page presses are the route and not the
/// claim, so they go the short way; the press into the field and the typing are the claim and are
/// a real pointer and a real keyboard.
pub fn the_name_field_of_the_wizard_takes_a_typed_name() {
    let mut c = HeadlessClient::new(ClientSpec::screen(dereth_ui::framework::mode::CHAR_GEN, 8));
    for id in [0x1000_03BF_u32, 0x1000_03F4] {
        // A heritage, so the wizard has one, and then the summary page the field is on.
        let h = hud_find(&c, ElementId(id));
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        shell
            .ui
            .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
        c.tick(1);
    }

    let field = hud_find(&c, dereth_ui_screens::screens::chargen::NAME_FIELD);
    let before = wizard_name(&mut c);
    c.when(Player::Click(
        dereth_ui_screens::screens::chargen::NAME_FIELD.into(),
    ));
    let it_holds_the_keyboard = who_holds_the_keyboard(&c) == Some(field);

    c.when(Player::Type("Tarinell".into()));
    // The page puts its own prompt in the box and selects it, and a press into a box that already
    // holds the keyboard drops a caret rather than replacing the selection -- so what is claimed
    // here is that every typed character reached the wizard's record, not what it starts out
    // holding. Typing with no press at all replaces the whole selection and is a claim of the
    // wizard's own.
    let after = wizard_name(&mut c);
    let every_character_arrived = after.ends_with("Tarinell") && after != before;
    let the_wizard_has_a_name = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let s = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **s;
        any.downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
            .expect("the wizard")
            .name_entered
    };

    c.assert_behaviour(
        "ui.focus.the-name-field-of-the-wizard-takes-a-typed-name",
        move |_| it_holds_the_keyboard && every_character_arrived && the_wizard_has_a_name,
    );
    c.shutdown();
}

/// The name the wizard itself is holding -- the value the finish would carry.
fn wizard_name(c: &mut HeadlessClient) -> String {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let s = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
        .expect("the wizard")
        .state
        .name
        .clone()
}

#[test]
fn scenario_the_name_field_of_the_wizard_takes_a_typed_name() {
    scenario("the_name_field_of_the_wizard_takes_a_typed_name");
}

// ---------------------------------------------------------------------------------------------
// ui.scrollbar.dragging-the-stack-slider-moves-the-thumb-and-the-quantity-follows
// ---------------------------------------------------------------------------------------------

/// The acceptance gesture: press on the track, drag, and read the thumb's own pixels and the
/// quantity at three positions. A split of the wrong size looks exactly like a split of the right
/// one, so the quantity is read at each of them rather than once at the end.
pub fn dragging_the_stack_slider_moves_the_thumb_and_the_quantity_follows() {
    let mut c = a_client_with_the_stack_splitter_up();
    let slider = hud_find(&c, dereth_ui_screens::toolbar::splitter::SLIDER);

    let sb = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .screen_box(slider);
    let the_shipped_box = (sb.width(), sb.height()) == (90, 14);
    let y = (sb.y0 + sb.y1) / 2;

    let thumb_x = |c: &HeadlessClient| {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let t = ui
            .get_child(slider, ElementId(1))
            .expect("the thumb is the slider's own child");
        ui.node(t).expect("alive").region.box_.x0
    };
    let position = |c: &HeadlessClient| {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        ui.node(slider)
            .expect("alive")
            .merged_properties()
            .get_float(dereth_ui::widgets::scrollbar::attr::POSITION)
    };
    let split = |c: &mut HeadlessClient| hud_gameplay(c).1.splitter.split_size;

    let mut hands = Hands::new();
    // The near end of the track: nothing moved, one item.
    hands.move_to(&mut c, sb.x0, y);
    let m = hands.button_message(dereth_client::platform::keys::MouseButton::Left, true);
    hands.send(&mut c, m);
    c.tick(1);
    let at_the_near_end = position(&c) == Some(0.0) && thumb_x(&c) == 0 && split(&mut c) == 1;

    // Half way along. The pointer sits half a thumb ahead of the thumb's own left edge, and the
    // travel is the track less the thumb.
    hands.move_to(&mut c, sb.x0 + 44, y);
    c.tick(1);
    let mid = position(&c).expect("a dragged slider has a position");
    let half_way = (mid - 36.0 / 73.0).abs() < 1e-5 && thumb_x(&c) == 36 && split(&mut c) == 10;

    // Past the far end: both the thumb and the quantity stop hard.
    hands.move_to(&mut c, sb.x1 + 500, y);
    c.tick(1);
    let clamped = position(&c) == Some(1.0) && thumb_x(&c) == 73 && split(&mut c) == 20;

    // ...and the drag ends when the button does.
    let m = hands.button_message(dereth_client::platform::keys::MouseButton::Left, false);
    hands.send(&mut c, m);
    c.tick(1);
    hands.move_to(&mut c, sb.x0, y);
    c.tick(1);
    let let_go = position(&c) == Some(1.0) && split(&mut c) == 20;

    c.assert_behaviour(
        "ui.scrollbar.dragging-the-stack-slider-moves-the-thumb-and-the-quantity-follows",
        move |_| the_shipped_box && at_the_near_end && half_way && clamped && let_go,
    );
    c.shutdown();
}

#[test]
fn scenario_dragging_the_stack_slider_moves_the_thumb_and_the_quantity_follows() {
    scenario("dragging_the_stack_slider_moves_the_thumb_and_the_quantity_follows");
}

// ---------------------------------------------------------------------------------------------
// ui.scrollbar.the-chat-bars-thumb-is-sized-and-placed-inside-its-track
// ---------------------------------------------------------------------------------------------

/// The cheap half of the same row: a thumb, in the track, between the two arrows -- not an empty
/// groove with the thumb still at the box the layout drew it in.
pub fn the_chat_bars_thumb_is_sized_and_placed_inside_its_track() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let bar = hud_find(&c, CHAT_BAR);

    let (the_shipped_bar, below_the_top_arrow, above_the_bottom_one, as_wide_as_the_bar, fills_it) = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let bar_box = ui.node(bar).expect("alive").region.box_;
        // Which arrow ends up at which end is the bar's own doing and not the layout's: it puts
        // the one that scrolls forward at the top and the one that scrolls back at the bottom,
        // and the shipped chat bar authors them the other way round.
        let up = ui
            .get_child(bar, ElementId(0x1000_0072))
            .expect("the arrow the bar moves to the top");
        let down = ui
            .get_child(bar, ElementId(0x1000_0071))
            .expect("the arrow the bar moves to the bottom");
        let up_box = ui.node(up).expect("alive").region.box_;
        let down_box = ui.node(down).expect("alive").region.box_;
        let t = ui
            .node(
                ui.get_child(bar, ElementId(1))
                    .expect("the thumb is the bar's own child"),
            )
            .expect("alive")
            .region
            .box_;
        // With nothing to scroll the thumb covers the whole groove. That is the number the layout
        // box cannot give: the thumb ships square, and a bar that had only moved it would still
        // pass a "somewhere in the track" test while showing a stub in a long groove.
        let track = down_box.y0 - up_box.y1 - 1;
        (
            (bar_box.width(), bar_box.height()) == (16, 73),
            t.y0 > up_box.y1 && t.y0 == up_box.y1 + 1,
            t.y1 < down_box.y0,
            t.width() == bar_box.width(),
            track == 41 && t.height() == track,
        )
    };

    c.assert_behaviour(
        "ui.scrollbar.the-chat-bars-thumb-is-sized-and-placed-inside-its-track",
        move |_| {
            the_shipped_bar
                && below_the_top_arrow
                && above_the_bottom_one
                && as_wide_as_the_bar
                && fills_it
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_chat_bars_thumb_is_sized_and_placed_inside_its_track() {
    scenario("the_chat_bars_thumb_is_sized_and_placed_inside_its_track");
}

// =============================================================================================
// ui.selection-strip.* and ui.item-cell.* -- the toolbar's selection strip and the pack's cells
//
// The green highlight fades, the split slider shows only for a stack, the player's own cell draws
// a backpack, the drag image is the icon without the slot's backdrop, and a no-op drop leaves no
// slot greyed. Six scenarios, six rows.
// =============================================================================================

/// The strip's plate: the child of the selection field that carries the highlight picture.
const SEL_PLATE: ElementId = ElementId(0x1000_01A0);
/// The name beside it, which the fade must not touch.
const SEL_NAME: ElementId = ElementId(0x1000_019F);
/// The two meters that live in the rectangle the highlight vacates.
const SEL_HEALTH: ElementId = ElementId(0x1000_01A1);
const SEL_MANA: ElementId = ElementId(0x1000_01A2);

/// The picture an element is drawing, if any.
fn hud_image(c: &HeadlessClient, h: ElemHandle) -> Option<dereth_primitives::DataId> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("alive")
        .region
        .image
        .as_ref()
        .map(|g| g.did)
}

/// The clock the screen's own timed tracks are measured against. **Not the wall clock**: a
/// headless frame advances this by a fixed step and runs far faster than real time, so a
/// wall-clock reading of an authored quarter second says only how quick the machine is.
fn hud_clock(c: &HeadlessClient) -> f64 {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .now
        .0
}

/// Frames until `f`, or until `limit` seconds of that clock have gone by; answers how much of it
/// went by.
fn wait_on_the_clock(
    c: &mut HeadlessClient,
    limit: f64,
    mut f: impl FnMut(&HeadlessClient) -> bool,
) -> f64 {
    let t0 = hud_clock(c);
    loop {
        c.tick(1);
        if f(c) || hud_clock(c) - t0 > limit {
            return hud_clock(c) - t0;
        }
    }
}

/// Select something, or nothing, the way the world tells the toolbar about it.
fn select(c: &mut HeadlessClient, id: Option<dereth_primitives::ObjectId>) {
    c.app_mut().objects_mut().world.set_selected_object(
        id,
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    c.tick(1);
}

/// Put a thing in the world and hand it to the strip. The name is this scenario's own invention,
/// which is what "private" means here.
fn a_selected_thing(
    c: &mut HeadlessClient,
    id: u32,
    obj_type: u32,
    stack: Option<u16>,
) -> dereth_primitives::ObjectId {
    let id = dereth_primitives::ObjectId(id);
    let mut w = dereth_client_model::weenie::Weenie::new(id);
    w.pwd.name = "a private thing".into();
    w.pwd.obj_type = obj_type;
    w.pwd.stack_size = stack;
    c.app_mut().objects_mut().world.tables.weenies.insert(id, w);
    select(c, Some(id));
    id
}

// ---------------------------------------------------------------------------------------------
// ui.selection-strip.the-highlight-comes-down-by-itself-a-quarter-second-later
// ---------------------------------------------------------------------------------------------

/// The green plate comes down by itself rather than staying up for the rest of the session. The
/// authored number is a quarter of a second, and it is read off the clock the track is run against.
pub fn the_highlight_comes_down_by_itself_a_quarter_second_later() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let field = hud_find(&c, SEL_OBJECT_FIELD);
    let plate = hud_find(&c, SEL_PLATE);
    let name = hud_find(&c, SEL_NAME);

    let nothing_is_lit = hud_state(&c, field) == StateId(0) && hud_image(&c, plate).is_none();

    let thing = a_selected_thing(&mut c, 0x7100_0031, 1, None);
    // The selection edge lights the strip and the plate follows it there.
    let it_lit_up = hud_state(&c, field) == StateId(0x1000_000B);
    let green = hud_image(&c, plate).expect("the lit state authors a picture on the plate");
    let a_real_picture = green.0 != 0;

    // ...and it comes down by itself. Three seconds is generous head-room and is still far inside
    // "never".
    let took = wait_on_the_clock(&mut c, 3.0, |c| hud_image(c, plate).is_none());
    let it_came_down = hud_image(&c, plate).is_none() && (0.25..0.35).contains(&took);
    let the_child_was_told =
        hud_state(&c, field) == StateId(0) && hud_state(&c, plate) == StateId(0);

    // Only the plate goes: the name beside it and the strip itself are untouched.
    let the_rest_stayed = hud_visible(&c, name) && hud_visible(&c, field);

    // And it is a track and not a one-shot.
    select(&mut c, None);
    select(&mut c, Some(thing));
    let it_lights_again = hud_image(&c, plate) == Some(green);

    c.assert_behaviour(
        "ui.selection-strip.the-highlight-comes-down-by-itself-a-quarter-second-later",
        move |_| {
            nothing_is_lit
                && it_lit_up
                && a_real_picture
                && it_came_down
                && the_child_was_told
                && the_rest_stayed
                && it_lights_again
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_highlight_comes_down_by_itself_a_quarter_second_later() {
    scenario("the_highlight_comes_down_by_itself_a_quarter_second_later");
}

// ---------------------------------------------------------------------------------------------
// ui.selection-strip.the-meters-start-down-and-a-creatures-answer-brings-its-bar-up
// ---------------------------------------------------------------------------------------------

/// What replaces the highlight for a creature, driven through a whole live frame rather than the
/// model alone.
pub fn the_meters_start_down_and_a_creatures_answer_brings_its_bar_up() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let health = hud_find(&c, SEL_HEALTH);
    let mana = hud_find(&c, SEL_MANA);
    let both_start_down = !hud_visible(&c, health) && !hud_visible(&c, mana);

    let creature = a_selected_thing(&mut c, 0x7100_0032, 0x10, None);
    let nothing_yet = !hud_visible(&c, health);

    // The answer about its health, through the same door the wire arm goes through.
    let answered = c
        .app_mut()
        .objects_mut()
        .world
        .update_object_health(creature, 0.375);
    c.tick(1);
    let the_bar_came_up = answered && hud_visible(&c, health);
    let and_only_that_one = !hud_visible(&c, mana);

    c.assert_behaviour(
        "ui.selection-strip.the-meters-start-down-and-a-creatures-answer-brings-its-bar-up",
        move |_| both_start_down && nothing_yet && the_bar_came_up && and_only_that_one,
    );
    c.shutdown();
}

#[test]
fn scenario_the_meters_start_down_and_a_creatures_answer_brings_its_bar_up() {
    scenario("the_meters_start_down_and_a_creatures_answer_brings_its_bar_up");
}

// ---------------------------------------------------------------------------------------------
// ui.selection-strip.empty-a-single-item-and-a-stack-are-three-different-strips
// ---------------------------------------------------------------------------------------------

/// The three states are asserted against each other in one scenario, because a build that hides
/// the splitter always passes any one of them alone.
pub fn empty_a_single_item_and_a_stack_are_three_different_strips() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let field = hud_find(&c, SEL_OBJECT_FIELD);
    let entry = hud_find(&c, dereth_ui_screens::toolbar::splitter::ENTRY_BOX);
    let slider = hud_find(&c, dereth_ui_screens::toolbar::splitter::SLIDER);

    // 1. Nothing selected. The strip's own background is up; the splitter is not -- and the
    // shipped layout authors both of those two visible, so something took them down.
    let empty = hud_state(&c, field) == StateId(0)
        && hud_visible(&c, field)
        && !hud_visible(&c, entry)
        && !hud_visible(&c, slider);

    // 2. One thing. A different state, still no splitter.
    let thing = a_selected_thing(&mut c, 0x7100_0033, 1, None);
    let single = hud_state(&c, field) == StateId(0x1000_000B)
        && !hud_visible(&c, entry)
        && !hud_visible(&c, slider);

    // 3. A stack of them. A third state, and the splitter comes up.
    c.app_mut()
        .objects_mut()
        .world
        .tables
        .weenies
        .get_mut(thing)
        .expect("alive")
        .pwd
        .stack_size = Some(7);
    c.tick(1);
    let stacked = hud_state(&c, field) == StateId(0x1000_000C)
        && hud_visible(&c, entry)
        && hud_visible(&c, slider);

    // ...and dropping the selection takes the splitter down again.
    select(&mut c, None);
    let empty_again =
        hud_state(&c, field) == StateId(0) && !hud_visible(&c, entry) && !hud_visible(&c, slider);

    c.assert_behaviour(
        "ui.selection-strip.empty-a-single-item-and-a-stack-are-three-different-strips",
        move |_| empty && single && stacked && empty_again,
    );
    c.shutdown();
}

#[test]
fn scenario_empty_a_single_item_and_a_stack_are_three_different_strips() {
    scenario("empty_a_single_item_and_a_stack_are_three_different_strips");
}

// ---------------------------------------------------------------------------------------------
// ui.item-cell.the-players-own-cell-draws-a-backpack-and-not-a-second-backdrop
// ---------------------------------------------------------------------------------------------

/// The backing and the backpack on it both draw. The two ways of
/// reading the pair of numbers that names the picture both resolve, which is why reading them the
/// wrong way round was silent -- so the pixels are what settle it.
pub fn the_players_own_cell_draws_a_backpack_and_not_a_second_backdrop() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));

    // The two readings, through the client's own two-hop resolver, which needs a client up.
    let right = dereth_ui_screens::env::did_by_enum(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        7,
        0x1000_0004,
    )
    .expect("the pair the client forwards resolves");
    let wrong = dereth_ui_screens::env::did_by_enum(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        0x1000_0004,
        7,
    )
    .expect("and so does the reversed one, which is why the mistake was silent");
    let two_different_pictures = right != wrong;

    // The production recipe, taken off the real cell rather than from the tables.
    let d = dereth_ui_screens::view::SlotDecoration {
        is_player: true,
        obj_type: 0x10,
        icon_id: 0x0600_13A5,
        ..dereth_ui_screens::view::SlotDecoration::default()
    };
    let recipe = dereth_ui_screens::items::widget::object_recipe(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        &d,
    );
    let dereth_ui::region::IconRecipe::Object {
        background, icon, ..
    } = recipe
    else {
        panic!("the cell composites a thing")
    };
    let it_names_the_backpack = icon == Some(right);
    let the_tile_is_its_own_row = background
        == dereth_ui_screens::env::did_by_enum(
            &c.view().expect_app().ui().expect("the UI shell is up").ui,
            0x1000_0004,
            10,
        );
    let not_the_backdrop_again = icon != background;

    // The pixels. An opaque second backdrop laid over the tile changes *every* pixel of it; the
    // real picture has a clear field, so it changes some and leaves the rest of the tile showing.
    // That count is the discriminator, and nothing else here is.
    let store = c.dat_store().expect("the retail dats are open").clone();
    let textures = dereth_client::textures::TextureStore::new(&store);
    let f = |id: dereth_primitives::DataId| textures.texture_data(id).ok();
    let composed = dereth_client::ui_draw::composite(recipe, &f).expect("the cell composites");
    let tile_only = dereth_client::ui_draw::composite(
        dereth_ui::region::IconRecipe::Object {
            background,
            effects: None,
            icon: None,
            overlay: None,
            underlay: None,
        },
        &f,
    )
    .expect("the tile on its own composites");
    let a = composed.levels.first().expect("one level");
    let b = tile_only.levels.first().expect("one level");
    let pixels = a.len() / 4;
    let changed = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count();
    let something_was_drawn = changed > 0;
    let and_not_the_whole_tile = changed < pixels;

    c.assert_behaviour(
        "ui.item-cell.the-players-own-cell-draws-a-backpack-and-not-a-second-backdrop",
        move |_| {
            two_different_pictures
                && it_names_the_backpack
                && the_tile_is_its_own_row
                && not_the_backdrop_again
                && something_was_drawn
                && and_not_the_whole_tile
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_players_own_cell_draws_a_backpack_and_not_a_second_backdrop() {
    scenario("the_players_own_cell_draws_a_backpack_and_not_a_second_backdrop");
}

// ---------------------------------------------------------------------------------------------
// The pack, with one thing in it
// ---------------------------------------------------------------------------------------------

/// A player, one loose thing in the open pack, and the inventory page up -- the shipped list
/// filled the way the client fills it when the shard sends a container's contents.
fn a_pack_with_one_thing_in_it() -> (HeadlessClient, dereth_primitives::ObjectId) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let me = dereth_primitives::ObjectId(0x5000_0001);
    let thing = dereth_primitives::ObjectId(0x7100_0041);
    {
        let w = c.app_mut().objects_mut();
        let mut p = dereth_client_model::weenie::Weenie::new(me);
        p.pwd.name = "a private player".into();
        p.pwd.obj_type = 0x10;
        p.pwd.items_capacity = Some(10);
        p.pwd.containers_capacity = Some(2);
        w.world.tables.weenies.insert(me, p);
        let mut i = dereth_client_model::weenie::Weenie::new(thing);
        i.pwd.name = "a private thing".into();
        i.pwd.obj_type = 1;
        i.pwd.container_id = Some(me);
        i.pwd.icon_id = 0x0600_13A5;
        i.pwd.effects = Some(0);
        w.world.tables.weenies.insert(thing, i);
        w.world.player = Some(me);
        w.world.open_container = Some(me);
        w.world.view_object_contents(
            me,
            &[dereth_protocol::types::ContentProfile {
                iid: thing,
                container_properties: 0,
            }],
            &mut dereth_client_model::RecordingSink::default(),
        );
    }
    c.tick(1);
    let panel = {
        let (_, s) = hud_gameplay(&mut c);
        s.panels
            .pages
            .iter()
            .find(|q| q.element == ElementId(0x1000_018B))
            .map(|q| q.panel_id)
            .expect("the inventory page is in the panel stack")
    };
    {
        let (ui, s) = hud_gameplay(&mut c);
        s.recv_set_panel_visibility(ui, panel, true);
    }
    c.tick(1);
    (c, thing)
}

// ---------------------------------------------------------------------------------------------
// ui.item-cell.what-follows-the-cursor-is-the-icon-alone-and-not-the-lifted-cell
// ---------------------------------------------------------------------------------------------

/// A real drag off a real slot of the shipped list, and then the same rule stated over a whole
/// five-layer recipe and its pixels.
pub fn what_follows_the_cursor_is_the_icon_alone() {
    let (mut c, thing) = a_pack_with_one_thing_in_it();

    let (cell_recipe, proxy_recipe) = {
        let (ui, s) = hud_gameplay(&mut c);
        let w = s
            .inventory
            .item_list
            .as_mut()
            .expect("the shipped item list");
        let i = w
            .slots
            .iter()
            .position(|q| q.item == Some(thing))
            .expect("the thing has a slot");
        let cell_recipe = w.slots[i]
            .icon_recipe(ui)
            .expect("the slot draws a composite");
        let (ox, oy) = ui.screen_origin(w.handle);
        let b = ui.node(w.slots[i].handle).expect("alive").region.box_;
        let start = w
            .begin_drag(ui, ox + b.x0 + b.width() / 2, oy + b.y0 + b.height() / 2)
            .expect("a filled slot starts a drag");
        let proxy = ui
            .node(start.drag_icon)
            .expect("the thing following the cursor is alive")
            .region
            .image
            .clone()
            .expect("and it has been given a picture");
        let Some(dereth_ui::region::SurfaceOp::Icon(r)) = proxy.op else {
            panic!("the proxy carries no recipe at all")
        };
        (cell_recipe, r)
    };
    let not_the_cells_own = proxy_recipe != cell_recipe;
    let dereth_ui::region::IconRecipe::Object {
        background,
        underlay,
        icon,
        ..
    } = proxy_recipe
    else {
        panic!("the proxy composites a thing")
    };
    let no_tile_under_the_cursor = background.is_none() && underlay.is_none();
    let the_things_own_picture = icon == Some(dereth_primitives::DataId(0x0600_13A5));

    // The same rule over a whole five-layer recipe: two layers are dropped and three are kept.
    let cell = dereth_ui::region::IconRecipe::Object {
        background: Some(dereth_primitives::DataId(0x0600_11CE)),
        effects: Some(dereth_primitives::DataId(0x0600_11C5)),
        icon: Some(dereth_primitives::DataId(0x0600_13A5)),
        overlay: Some(dereth_primitives::DataId(0x0600_1234)),
        underlay: Some(dereth_primitives::DataId(0x0600_5678)),
    };
    let drag = cell.drag_surface();
    let a_different_surface = drag != cell;
    let dereth_ui::region::IconRecipe::Object {
        background,
        effects,
        icon,
        overlay,
        underlay,
    } = drag
    else {
        panic!("a thing")
    };
    let the_right_three = background.is_none()
        && underlay.is_none()
        && icon == Some(dereth_primitives::DataId(0x0600_13A5))
        && overlay == Some(dereth_primitives::DataId(0x0600_1234))
        && effects == Some(dereth_primitives::DataId(0x0600_11C5));

    // The pixels: what follows the cursor is not the cell's picture, and it is clear somewhere,
    // which is what makes it read as a floating icon rather than as a lifted cell.
    let store = c.dat_store().expect("the retail dats are open").clone();
    let textures = dereth_client::textures::TextureStore::new(&store);
    let f = |id: dereth_primitives::DataId| textures.texture_data(id).ok();
    let cell_px = dereth_client::ui_draw::composite(cell, &f).expect("the cell composites");
    let drag_px = dereth_client::ui_draw::composite(drag, &f).expect("the proxy composites");
    let different_pixels = cell_px.levels.first() != drag_px.levels.first();
    let clear_somewhere = drag_px
        .levels
        .first()
        .expect("one level")
        .chunks_exact(4)
        .any(|px| px[3] == 0);

    c.assert_behaviour(
        "ui.item-cell.what-follows-the-cursor-is-the-icon-alone-and-not-the-lifted-cell",
        move |_| {
            not_the_cells_own
                && no_tile_under_the_cursor
                && the_things_own_picture
                && a_different_surface
                && the_right_three
                && different_pixels
                && clear_somewhere
        },
    );
    c.shutdown();
}

#[test]
fn scenario_what_follows_the_cursor_is_the_icon_alone() {
    scenario("what_follows_the_cursor_is_the_icon_alone");
}

// ---------------------------------------------------------------------------------------------
// ui.item-cell.dropping-a-thing-back-on-its-own-slot-sends-nothing-and-ghosts-nothing
// ---------------------------------------------------------------------------------------------

/// Both halves of the decline: nothing is asked of the shard, and nothing is left greyed. The
/// first is what stops a fix that merely un-greys afterwards from passing.
pub fn dropping_a_thing_back_on_its_own_slot_sends_nothing_and_ghosts_nothing() {
    let (mut c, thing) = a_pack_with_one_thing_in_it();

    // Pick it up first. The lift greys the slot it came from, so the grey overlay is already
    // there before the drop -- a scenario that only dropped could not see it left behind.
    let (source, target, the_lift_greyed_it) = {
        let (ui, s) = hud_gameplay(&mut c);
        let (target, at) = {
            let w = s
                .inventory
                .item_list
                .as_mut()
                .expect("the shipped item list");
            let i = w
                .slots
                .iter()
                .position(|q| q.item == Some(thing))
                .expect("the thing has a slot");
            let target = w.slots[i].handle;
            let (ox, oy) = ui.screen_origin(w.handle);
            let b = ui.node(target).expect("alive").region.box_;
            (
                target,
                (ox + b.x0 + b.width() / 2, oy + b.y0 + b.height() / 2),
            )
        };
        // The screen's own arm, and not the widget's method: that is where the write that greys
        // the object crosses the seam.
        let start = s
            .begin_item_drag(ui, target, at.0, at.1)
            .expect("the slot starts a drag");
        let w = s
            .inventory
            .item_list
            .as_ref()
            .expect("the shipped item list");
        let i = w
            .slots
            .iter()
            .position(|q| q.item == Some(thing))
            .expect("the thing has a slot");
        (start.drag_icon, target, w.slots[i].waiting)
    };
    c.tick(1);

    // The greying is on the **object**, which is what survives the list being rebuilt.
    let greyed_on_the_object = c
        .view()
        .expect_app()
        .objects()
        .world
        .tables
        .weenies
        .get(thing)
        .map(|w| w.waiting)
        == Some(true);
    let sent_before = c.view().expect_app().interaction().stats.requests_sent;

    // The drop, on the very slot it came from.
    {
        let (ui, s) = hud_gameplay(&mut c);
        s.handle_drop_release(ui, target, source);
    }
    c.tick(1);

    let nothing_was_asked = c.view().expect_app().interaction().stats.requests_sent == sent_before;
    let the_object_came_back = c
        .view()
        .expect_app()
        .objects()
        .world
        .tables
        .weenies
        .get(thing)
        .map(|w| w.waiting)
        == Some(false);
    let the_slot_came_back = {
        let (_, s) = hud_gameplay(&mut c);
        let w = s
            .inventory
            .item_list
            .as_ref()
            .expect("the shipped item list");
        !w.slots
            .iter()
            .find(|q| q.item == Some(thing))
            .expect("the thing kept its slot")
            .waiting
    };

    c.assert_behaviour(
        "ui.item-cell.dropping-a-thing-back-on-its-own-slot-sends-nothing-and-ghosts-nothing",
        move |_| {
            the_lift_greyed_it
                && greyed_on_the_object
                && nothing_was_asked
                && the_object_came_back
                && the_slot_came_back
        },
    );
    c.shutdown();
}

#[test]
fn scenario_dropping_a_thing_back_on_its_own_slot_sends_nothing_and_ghosts_nothing() {
    scenario("dropping_a_thing_back_on_its_own_slot_sends_nothing_and_ghosts_nothing");
}

// =============================================================================================
// hud.lamp-row.* -- the six lamps and the way out, top left
//
// Every top-left lamp can be pressed, the way out raises its question, and buffs, a death
// penalty and an over-full pack each light their lamp. Seven scenarios, seven rows.
//
// **Every lamp claim is made unlit first and lit second on one client**, because a lamp lighting
// is an edge and a scenario that looks only once cannot see one.
//
// The character here is built by the shard stand-in the harness owns, and its strength is this
// scenario's own. Nothing the row claims depends on which strength it is: the lamp is read against
// whatever that character can carry, and the scenario works it out the way the client does.
// =============================================================================================

use dereth_ui_screens::hud::indicators;

/// The six lamps and the button at the end of the row.
const ROW_LINK_LAMP: ElementId = ElementId(0x1000_00F8);
const BUFF_LAMP: ElementId = ElementId(0x1000_00F5);
const DEBUFF_LAMP: ElementId = ElementId(0x1000_00F6);
const VITAE_LAMP: ElementId = ElementId(0x1000_00F4);
const BURDEN_LAMP: ElementId = ElementId(0x1000_00F7);
const EXIT_BUTTON: ElementId = ElementId(0x1000_00FA);

/// The panels the lamps' actions open, and the stack window that holds them.
const CHARACTER_INFO_PANEL: ElementId = ElementId(0x1000_0183);
const POSITIVE_MAGIC_PANEL: ElementId = ElementId(0x1000_0184);
const NEGATIVE_MAGIC_PANEL: ElementId = ElementId(0x1000_0185);
const LINK_STATUS_PANEL: ElementId = ElementId(0x1000_0187);
const VITAE_PANEL: ElementId = ElementId(0x1000_018A);
const PANEL_STACK: ElementId = ElementId(0x1000_05FF);

/// The character the lamp scenarios are about.
const LAMP_PLAYER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_0001);
/// What the character is carrying, and how many augmentations they have -- the two the client
/// works a carrying capacity out of.
const ENCUMB_VAL: u32 = 5;
const NUM_AUGMENTATIONS: u32 = 230;

/// The state an element of the strip is in.
fn strip_state(c: &HeadlessClient, id: ElementId) -> u32 {
    hud_state(c, hud_find(c, id)).0
}

/// Press something on the strip, through the channel a real press ends in.
///
/// **Two frames, and the second is not padding**: the press fires the action on the first, and the
/// showing it asks for reaches the panel stack on the second. That one-frame seam is the message
/// bus's, declared and pre-existing.
fn press_the_strip(c: &mut HeadlessClient, id: ElementId) {
    let h = hud_find(c, id);
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    c.tick(2);
}

/// Put a lamp in a lit state, the way the lamp's own update does the moment the player gains
/// something to show.
///
/// This is not decoration: a lamp with nothing to show *is* a disabled button, and a disabled
/// button swallows its own press. So a scenario about pressing a lamp has to light it first, and
/// that is the client's behaviour rather than this scenario's convenience.
fn light_the_lamp(c: &mut HeadlessClient, id: ElementId) {
    let h = hud_find(c, id);
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell.ui.set_state(h, StateId(1));
}

/// A gameplay client with a character the shard has described, so the lamps have qualities to
/// read.
fn a_described_character() -> (HeadlessClient, dereth_testkit::Peer) {
    use dereth_protocol::types::qualities::{
        attribute_cache_mask as m, quality_flags, AcQualities, Attribute, AttributeCache,
    };

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut peer = dereth_testkit::Peer::attach_creating(&mut c, LAMP_PLAYER);
    c.world_mut().player = Some(LAMP_PLAYER);
    let cache = AttributeCache {
        flags: m::STRENGTH,
        strength: Some(Attribute {
            level_from_cp: 0,
            init_level: 100,
            cp_spent: 0,
        }),
        ..AttributeCache::default()
    };
    let d = dereth_protocol::login::LoginPlayerDescription {
        qualities: AcQualities {
            flags: quality_flags::ATTRIBUTE_CACHE,
            attribute_cache: Some(cache),
            ..AcQualities::default()
        },
        player_module: dereth_protocol::login::PlayerModule {
            spell_bars: vec![Vec::new()],
            spell_filters: dereth_protocol::login::PlayerModule::DEFAULT_SPELL_FILTERS,
            options2: dereth_protocol::login::PlayerModule::DEFAULT_OPTIONS2,
            ..dereth_protocol::login::PlayerModule::default()
        },
        content_profiles: Vec::new(),
        inventory_placements: Vec::new(),
    };
    peer.event(&mut c, &d);
    c.tick(4);
    (c, peer)
}

/// One int quality, through the production writer. The sequence has to advance: a stale one is
/// dropped, which is the gate that would make a silent no-op look like a lamp that will not light.
fn set_int_quality(c: &mut HeadlessClient, sequence: u8, property: u32, value: i32) {
    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::qualities::QualitiesPrivateUpdateInt(
            dereth_protocol::qualities::PrivateUpdate {
                sequence,
                property_id: property,
                value,
            },
        ),
    ));
    c.tick(1);
}

/// One enchantment, the way the only producer of one writes it.
fn an_enchantment(
    spell: u16,
    category: u16,
    kind: u32,
    value: f32,
) -> dereth_protocol::types::qualities::Enchantment {
    dereth_protocol::types::qualities::Enchantment {
        id: u32::from(spell),
        category_word: u32::from(category),
        power_level: 1,
        start_time: 0.0,
        duration: 60.0,
        caster: dereth_primitives::ObjectId(0),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: dereth_protocol::types::qualities::StatMod {
            kind,
            key: 1,
            value,
        },
        spell_set_id: None,
    }
}

fn enchant(c: &mut HeadlessClient, e: dereth_protocol::types::qualities::Enchantment) {
    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::qualities::MagicUpdateEnchantment(e),
    ));
    c.tick(1);
}

/// One spell the shipped table really marks helpful and one it does not. A spell the table does
/// not carry moves neither counter, which is why an invented id would light no lamp and measure
/// nothing.
fn a_helpful_and_a_harmful_spell(c: &HeadlessClient) -> (u16, u16) {
    let app = c.view().expect_app();
    let t = app
        .hud()
        .spell_table
        .as_ref()
        .expect("the shipped spell table loads");
    let pick = |helpful: bool| {
        t.spells
            .iter()
            .find(|(_, s)| (s.bitfield & 4 != 0) == helpful)
            .map(|(id, _)| u16::try_from(*id).expect("a spell id fits"))
            .expect("the shipped table has one of each")
    };
    (pick(true), pick(false))
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.the-way-out-of-the-strip-raises-the-question-about-ending-the-session
// ---------------------------------------------------------------------------------------------

/// Two checkpoints: nothing up before the press, the question up after it -- a dialog that was
/// always up could not pass.
pub fn the_way_out_of_the_strip_raises_the_question_about_ending_the_session() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(6));

    // It is a plain button and carries no action of its own, which is why its press reaches the
    // strip's own listener at all rather than being consumed on the way.
    let a_plain_button = {
        let h = hud_find(&c, EXIT_BUTTON);
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let n = ui.node(h).expect("a live node");
        n.ty().0 == 1
            && n.merged_properties()
                .get_enum(dereth_ui_screens::screens::gameplay::BUTTON_INPUT_ACTION)
                .is_none()
    };
    let nothing_up_first = {
        let (_, s) = hud_gameplay(&mut c);
        s.logout_dialog().is_none()
    };

    press_the_strip(&mut c, EXIT_BUTTON);

    let (the_question_is_up, the_asking_form, and_it_does_not_close_the_client) = {
        let (_, s) = hud_gameplay(&mut c);
        (
            s.logout_dialog().is_some(),
            s.logout_prompt.as_deref()
                == Some(dereth_ui_screens::screens::gameplay::logout::END_SESSION_CONFIRM),
            !s.should_quit_on_logout,
        )
    };

    c.assert_behaviour(
        "hud.lamp-row.the-way-out-of-the-strip-raises-the-question-about-ending-the-session",
        move |_| {
            a_plain_button
                && nothing_up_first
                && the_question_is_up
                && the_asking_form
                && and_it_does_not_close_the_client
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_way_out_of_the_strip_raises_the_question_about_ending_the_session() {
    scenario("the_way_out_of_the_strip_raises_the_question_about_ending_the_session");
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.every-lamp-opens-its-own-panel-and-leaves-the-other-lamps-panels-down
// ---------------------------------------------------------------------------------------------

/// Two checkpoints per lamp -- down, then up -- and the four panels that were not asked for are
/// read at the same moment, which is what stops "everything is visible" passing.
pub fn every_lamp_opens_its_own_panel() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(6));
    let table = [
        (ROW_LINK_LAMP, 0x1000_0009_u32, LINK_STATUS_PANEL),
        (BUFF_LAMP, 0x1000_0006, POSITIVE_MAGIC_PANEL),
        (DEBUFF_LAMP, 0x1000_0007, NEGATIVE_MAGIC_PANEL),
        (VITAE_LAMP, 0x1000_000C, VITAE_PANEL),
        (BURDEN_LAMP, 0x1000_0005, CHARACTER_INFO_PANEL),
    ];

    // The layout half: each lamp carries the action the shipped layout gives it.
    let mut the_actions_are_authored = true;
    for (lamp, action, _) in table {
        let h = hud_find(&c, lamp);
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        the_actions_are_authored &= ui
            .node(h)
            .expect("a live node")
            .merged_properties()
            .get_enum(dereth_ui_screens::screens::gameplay::BUTTON_INPUT_ACTION)
            == Some(action);
    }

    // Checkpoint 1 -- every page is down, and the stack window with them.
    let mut all_down_first = !hud_visible(&c, hud_find(&c, PANEL_STACK));
    for (_, _, panel) in table {
        all_down_first &= !hud_visible(&c, hud_find(&c, panel));
    }

    // Checkpoint 2 -- one lamp at a time.
    let mut each_opens_its_own = true;
    for (lamp, _, panel) in table {
        light_the_lamp(&mut c, lamp);
        press_the_strip(&mut c, lamp);
        each_opens_its_own &=
            hud_visible(&c, hud_find(&c, panel)) && hud_visible(&c, hud_find(&c, PANEL_STACK));
        for (_, _, other) in table {
            if other != panel {
                each_opens_its_own &= !hud_visible(&c, hud_find(&c, other));
            }
        }
    }

    c.assert_behaviour(
        "hud.lamp-row.every-lamp-opens-its-own-panel-and-leaves-the-other-lamps-panels-down",
        move |_| the_actions_are_authored && all_down_first && each_opens_its_own,
    );
    c.shutdown();
}

#[test]
fn scenario_every_lamp_opens_its_own_panel() {
    scenario("every_lamp_opens_its_own_panel");
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.a-button-of-the-strip-with-no-action-of-its-own-opens-nothing
// ---------------------------------------------------------------------------------------------

/// The negative half of the same arm, which is what stops "every press opens every panel" passing.
pub fn a_button_of_the_strip_with_no_action_of_its_own_opens_nothing() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(6));
    let none_to_start = hud_gameplay(&mut c).1.button_actions_fired == 0;
    press_the_strip(&mut c, EXIT_BUTTON);
    let still_none = hud_gameplay(&mut c).1.button_actions_fired == 0;
    let and_no_panel = !hud_visible(&c, hud_find(&c, PANEL_STACK));

    c.assert_behaviour(
        "hud.lamp-row.a-button-of-the-strip-with-no-action-of-its-own-opens-nothing",
        move |_| none_to_start && still_none && and_no_panel,
    );
    c.shutdown();
}

#[test]
fn scenario_a_button_of_the_strip_with_no_action_of_its_own_opens_nothing() {
    scenario("a_button_of_the_strip_with_no_action_of_its_own_opens_nothing");
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.the-burden-lamp-crosses-both-thresholds-on-the-characters-own-capacity
// ---------------------------------------------------------------------------------------------

/// Five checkpoints on one client, driven by real quality updates against what this character can
/// actually carry, worked out the way the client works it out.
pub fn the_burden_lamp_crosses_both_thresholds_on_the_characters_own_capacity() {
    let (mut c, _peer) = a_described_character();

    let (strength, augs) = {
        let world = c.view().world();
        let q = world
            .player_qualities()
            .expect("the character has been described");
        let s = q
            .attributes
            .and_then(|a| a.strength)
            .map(|a| i32::try_from(a.init_level + a.level_from_cp).expect("fits"))
            .expect("the character has a strength");
        (s, q.inq_int(NUM_AUGMENTATIONS))
    };
    let capacity = dereth_client_model::inventory::burden::encumbrance_capacity(strength, augs);
    let a_real_capacity = capacity > 0;

    // Checkpoint 1 -- as described, carrying nothing.
    let under_to_start = strip_state(&c, BURDEN_LAMP) == indicators::burden::UNDER;

    // Checkpoint 2 -- under, but deliberately not nothing, so the reading is proven to be running
    // rather than merely agreeing with the layout.
    set_int_quality(&mut c, 1, NUM_AUGMENTATIONS, augs);
    set_int_quality(&mut c, 2, ENCUMB_VAL, capacity / 2);
    let half_is_under = strip_state(&c, BURDEN_LAMP) == indicators::burden::UNDER;

    // Checkpoint 3 -- exactly all of it, which is over and not under.
    set_int_quality(&mut c, 3, ENCUMB_VAL, capacity);
    let all_of_it_is_over = strip_state(&c, BURDEN_LAMP) == indicators::burden::OVER;

    // Checkpoint 4 -- twice it.
    set_int_quality(&mut c, 4, ENCUMB_VAL, capacity * 2);
    let twice_is_further = strip_state(&c, BURDEN_LAMP) == indicators::burden::WAY_OVER;

    // ...and it goes out again, so the lamp is seen going out as well as coming on.
    set_int_quality(&mut c, 5, ENCUMB_VAL, 0);
    let it_goes_out = strip_state(&c, BURDEN_LAMP) == indicators::burden::UNDER;

    c.assert_behaviour(
        "hud.lamp-row.the-burden-lamp-crosses-both-thresholds-on-the-characters-own-capacity",
        move |_| {
            a_real_capacity
                && under_to_start
                && half_is_under
                && all_of_it_is_over
                && twice_is_further
                && it_goes_out
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_burden_lamp_crosses_both_thresholds_on_the_characters_own_capacity() {
    scenario("the_burden_lamp_crosses_both_thresholds_on_the_characters_own_capacity");
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.a-buff-lights-one-lamp-a-debuff-the-other-and-a-purge-puts-both-out
// ---------------------------------------------------------------------------------------------

/// The asymmetry is the whole scenario: a pair of lamps driven by one shared count would pass a
/// "something is lit" assertion and be wrong.
pub fn a_buff_lights_one_lamp_and_a_debuff_the_other() {
    use dereth_client_model::enchant::ench_type;
    let (mut c, _peer) = a_described_character();

    let both_dark_first = strip_state(&c, BUFF_LAMP) == indicators::STATE_NOTHING
        && strip_state(&c, DEBUFF_LAMP) == indicators::STATE_NOTHING;
    let (good, bad) = a_helpful_and_a_harmful_spell(&c);

    enchant(
        &mut c,
        an_enchantment(good, 1, ench_type::ADDITIVE | ench_type::BENEFICIAL, 1.0),
    );
    let one_lamp_alone = strip_state(&c, BUFF_LAMP) == 1
        && strip_state(&c, DEBUFF_LAMP) == indicators::STATE_NOTHING;

    enchant(&mut c, an_enchantment(bad, 2, ench_type::ADDITIVE, 1.0));
    let and_then_the_other = strip_state(&c, BUFF_LAMP) == 1 && strip_state(&c, DEBUFF_LAMP) == 1;

    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::qualities::MagicPurgeEnchantments,
    ));
    c.tick(1);
    let a_purge_puts_both_out = strip_state(&c, BUFF_LAMP) == indicators::STATE_NOTHING
        && strip_state(&c, DEBUFF_LAMP) == indicators::STATE_NOTHING;

    c.assert_behaviour(
        "hud.lamp-row.a-buff-lights-one-lamp-a-debuff-the-other-and-a-purge-puts-both-out",
        move |_| both_dark_first && one_lamp_alone && and_then_the_other && a_purge_puts_both_out,
    );
    c.shutdown();
}

#[test]
fn scenario_a_buff_lights_one_lamp_and_a_debuff_the_other() {
    scenario("a_buff_lights_one_lamp_and_a_debuff_the_other");
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.the-vitae-lamp-lights-for-a-penalty-and-not-for-a-multiplier-of-one
// ---------------------------------------------------------------------------------------------

/// **No recording witnesses this**: nothing in the corpus installs a death penalty. The body is
/// this scenario's own, and the arm it goes in through is the client's only door for one.
pub fn the_vitae_lamp_lights_for_a_penalty_and_not_for_a_multiplier_of_one() {
    use dereth_client_model::enchant::ench_type;
    let (mut c, _peer) = a_described_character();

    let dark_with_no_penalty = strip_state(&c, VITAE_LAMP) == indicators::STATE_NOTHING;

    enchant(
        &mut c,
        an_enchantment(666, 0, ench_type::VITAE | ench_type::MULTIPLICATIVE, 0.95),
    );
    let it_reached_the_character = (c
        .view()
        .world()
        .player_qualities()
        .expect("the character has been described")
        .enchantments
        .vitae_value()
        - 0.95)
        .abs()
        < 1e-6;
    let a_penalty_lights_it = strip_state(&c, VITAE_LAMP) == 1;

    // Present and yet no penalty at all: the case a lit-only scenario cannot see.
    enchant(
        &mut c,
        an_enchantment(666, 0, ench_type::VITAE | ench_type::MULTIPLICATIVE, 1.0),
    );
    let none_at_all_puts_it_out = strip_state(&c, VITAE_LAMP) == indicators::STATE_NOTHING;

    c.assert_behaviour(
        "hud.lamp-row.the-vitae-lamp-lights-for-a-penalty-and-not-for-a-multiplier-of-one",
        move |_| {
            dark_with_no_penalty
                && it_reached_the_character
                && a_penalty_lights_it
                && none_at_all_puts_it_out
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_vitae_lamp_lights_for_a_penalty_and_not_for_a_multiplier_of_one() {
    scenario("the_vitae_lamp_lights_for_a_penalty_and_not_for_a_multiplier_of_one");
}

// ---------------------------------------------------------------------------------------------
// hud.link-lamp.it-comes-up-good-and-falls-to-lost-when-nothing-is-heard-at-all
// ---------------------------------------------------------------------------------------------

/// The one lamp no message drives. The strip's table would otherwise carry a citation for it
/// rather than a measurement.
pub fn the_link_lamp_comes_up_good_and_falls_to_lost() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(6));
    // The layout gives this lamp nothing to draw at rest, so it is put straight into "good".
    let good_to_start = strip_state(&c, ROW_LINK_LAMP) == indicators::link_status::media::GOOD;

    // There is no link at all here, so every reading falls through; run frames until the lamp's
    // own cadence has come round.
    for _ in 0..600 {
        c.tick(1);
        if strip_state(&c, ROW_LINK_LAMP) == indicators::link_status::media::LOST {
            break;
        }
    }
    let it_falls_to_lost = strip_state(&c, ROW_LINK_LAMP) == indicators::link_status::media::LOST;

    c.assert_behaviour(
        "hud.link-lamp.it-comes-up-good-and-falls-to-lost-when-nothing-is-heard-at-all",
        move |_| good_to_start && it_falls_to_lost,
    );
    c.shutdown();
}

#[test]
fn scenario_the_link_lamp_comes_up_good_and_falls_to_lost() {
    scenario("the_link_lamp_comes_up_good_and_falls_to_lost");
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
pub fn a_swing_with_nothing_selected_says_so_on_the_strip() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    c.world_mut().combat.combat_mode = dereth_client_model::combat::CombatMode::Melee;
    let nothing_said_yet = c.view().expect_app().interaction().last_refusal.is_none();

    c.when(Player::Press(dereth_input::ActionId(
        dereth_client::interaction::action::COMBAT_LOW_ATTACK,
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

#[test]
fn scenario_a_swing_with_nothing_selected_says_so_on_the_strip() {
    scenario("a_swing_with_nothing_selected_says_so_on_the_strip");
}

// =============================================================================================
// dates.* (dat) -- the five surfaces a date is drawn on
//
// The formatter itself and the zone seam are in the `cpu` tier's file beside this one, and the
// reasoning is there.
//
// **No assertion here reads a zone this scenario made up.** These are the wiring half: each
// surface is compared against what the machine's own offset would give for that very instant, so
// they say the same thing in every zone and go red only where a surface is left on a fixed one.
// =============================================================================================

use dereth_client::platform::local_utc_offset_secs;
use dereth_ui_screens::ctime::strftime_c;
use dereth_ui_screens::panels::{characterinfo, house};

/// The instant the date scenarios draw.
const DATE_INSTANT: i64 = 1_789_404_178;
/// A villa's own rent period; the arithmetic for it is another scenario's claim.
const DATE_RENT_PERIOD: i64 = 30 * 86_400;
/// When this house was bought, as the quality carries it.
const HOUSE_PURCHASE_TIMESTAMP: u32 = 0xC7;
/// The cell the recorded house sits in.
const HOUSE_CELL: u32 = 0xA9B4_0025;

/// The date the machine itself would draw for an instant -- the offset for **that** instant,
/// because which offset it is depends on when it is.
fn the_machines_own(t: i64) -> String {
    strftime_c(t, local_utc_offset_secs(t))
}

/// A surface must not be drawing in the shard's time, and must be drawing in the machine's.
///
/// On a machine that *is* at that offset the two are one string and there is nothing to see; that
/// is said out loud rather than left as a silently vacuous assertion, and the shape half still
/// bites everywhere.
fn drawn_in_the_machines_time(text: &str, t: i64) -> bool {
    let off = local_utc_offset_secs(t);
    if off == 0 {
        return text.contains(&strftime_c(t, 0));
    }
    !text.contains(&strftime_c(t, 0)) && text.contains(&strftime_c(t, off))
}

/// No date in the shipped runtime's shape carries a weekday, and every one of them ends in a
/// morning or afternoon mark.
fn not_the_other_shape(text: &str) -> bool {
    !["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        .iter()
        .any(|w| text.contains(w))
        && (text.contains(" AM") || text.contains(" PM"))
}

fn the_clock_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .expect("a clock after 1970")
}

// ---------------------------------------------------------------------------------------------
// dates.the-character-sheets-born-line-is-in-the-machines-own-time-and-that-shape
// ---------------------------------------------------------------------------------------------

/// The line is read off the element as well as off what the sheet remembers, because a claim about
/// what the player reads that is only ever taken from the model is a claim about the model.
pub fn the_character_sheets_born_line_is_in_the_machines_own_time() {
    let (mut c, _peer) = a_described_character();
    press_the_strip(&mut c, BURDEN_LAMP);

    let stamp = i32::try_from(DATE_INSTANT).expect("this year fits the clock the quality carries");
    set_int_quality(&mut c, 11, characterinfo::prop::CREATION_TIMESTAMP, stamp);
    // Anything that forces the sheet to compose again; the sheet watches exactly one quality and
    // this is it.
    set_int_quality(&mut c, 12, characterinfo::prop::AGE, 15);
    c.tick(6);

    let born = c.view().expect_app().hud().panels.character_info.sections[0].clone();
    let want = the_machines_own(DATE_INSTANT);
    let it_says_so = born.contains(&want);
    let it_is_on_the_element = {
        let h = hud_find(&c, characterinfo::INFO_TEXT);
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        shell
            .ui
            .text_element_mut(h)
            .expect("the sheet's text is a text element")
            .glyphs
            .inq_text(false)
            .contains(&want)
    };
    let the_right_shape = not_the_other_shape(&born);
    let the_right_time = drawn_in_the_machines_time(&born, DATE_INSTANT);

    c.assert_behaviour(
        "dates.the-character-sheets-born-line-is-in-the-machines-own-time-and-that-shape",
        move |_| it_says_so && it_is_on_the_element && the_right_shape && the_right_time,
    );
    c.shutdown();
}

#[test]
fn scenario_the_character_sheets_born_line_is_in_the_machines_own_time() {
    scenario("the_character_sheets_born_line_is_in_the_machines_own_time");
}

// ---------------------------------------------------------------------------------------------
// The house, as the shard describes one
// ---------------------------------------------------------------------------------------------

/// One line of what was paid: how much, how much of it is paid, and what it was paid in.
fn a_payment(num: i32, paid: i32, wcid: u32, name: &str, plural: &str) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&num.to_le_bytes());
    b.extend_from_slice(&paid.to_le_bytes());
    b.extend_from_slice(&wcid.to_le_bytes());
    for s in [name, plural] {
        b.extend_from_slice(&u16::try_from(s.len()).expect("a short name").to_le_bytes());
        b.extend_from_slice(s.as_bytes());
        while b.len() % 4 != 0 {
            b.push(0);
        }
    }
    b
}

/// A villa that still owes rent, written the way the shard writes one.
fn a_villa(buy_time: i32, rent_time: i32) -> Vec<u8> {
    let (buy, rent) = (
        a_payment(30_000, 30_000, 273, "Pyreal", "Pyreals"),
        a_payment(30_000, 20_000, 273, "Pyreal", "Pyreals"),
    );
    let mut b = Vec::new();
    b.extend_from_slice(&buy_time.to_le_bytes());
    b.extend_from_slice(&rent_time.to_le_bytes());
    b.extend_from_slice(&2u32.to_le_bytes());
    b.extend_from_slice(&0i32.to_le_bytes());
    for list in [&buy, &rent] {
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(list);
    }
    b.extend_from_slice(&HOUSE_CELL.to_le_bytes());
    for f in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b
}

/// The house pane, filled from one description of a villa.
fn a_pane_for_a_villa(bought_at: i64) -> HeadlessClient {
    let (mut c, _peer) = a_described_character();
    let stamp = i32::try_from(bought_at).expect("this year fits");
    set_int_quality(&mut c, 21, HOUSE_PURCHASE_TIMESTAMP, stamp);
    let mut blob = dereth_protocol::Opcode::HOUSE_HOUSE_DATA
        .0
        .to_le_bytes()
        .to_vec();
    blob.extend_from_slice(&a_villa(stamp, stamp));
    c.when(dereth_testkit::Inbound::event(
        dereth_client_net::client_session::SessionEvent::UiEvent {
            opcode: dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
            blob,
        },
    ));
    c.tick(6);
    c
}

fn pane_rows(c: &HeadlessClient) -> Vec<String> {
    c.view()
        .expect_app()
        .hud()
        .panels
        .house
        .lines
        .iter()
        .map(|(s, _)| s.clone())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// dates.every-date-on-the-house-pane-is-in-the-machines-own-time-and-that-shape
// ---------------------------------------------------------------------------------------------

/// Three rows of the one pane, each with its own date worked out on its own.
pub fn every_date_on_the_house_pane_is_in_the_machines_own_time() {
    let mut c = a_pane_for_a_villa(DATE_INSTANT);
    let rows = pane_rows(&c);
    let due = DATE_INSTANT + DATE_RENT_PERIOD;

    let bought = rows.get(2).cloned().expect("the pane has a bought row");
    let ends = rows.get(3).cloned().expect("the pane has a period-end row");
    let next = rows.get(4).cloned().expect("the pane has a next-due row");

    let the_bought_row = bought == format!("{}{}", house::BOUGHT, the_machines_own(DATE_INSTANT))
        && not_the_other_shape(&bought)
        && drawn_in_the_machines_time(&bought, DATE_INSTANT);
    let the_period_row = ends == format!("{}{}", house::PERIOD_ENDS, the_machines_own(due))
        && not_the_other_shape(&ends)
        && drawn_in_the_machines_time(&ends, due);
    let the_due_row = next.starts_with(house::NEXT_DUE)
        && next == format!("{}{}", house::NEXT_DUE, the_machines_own(due))
        && not_the_other_shape(&next)
        && drawn_in_the_machines_time(&next, due);

    c.assert_behaviour(
        "dates.every-date-on-the-house-pane-is-in-the-machines-own-time-and-that-shape",
        move |_| the_bought_row && the_period_row && the_due_row,
    );
    c.shutdown();
}

#[test]
fn scenario_every_date_on_the_house_pane_is_in_the_machines_own_time() {
    scenario("every_date_on_the_house_pane_is_in_the_machines_own_time");
}

// ---------------------------------------------------------------------------------------------
// dates.the-line-saying-when-another-house-may-be-bought-is-in-the-machines-own-time
// ---------------------------------------------------------------------------------------------

/// The one date on this pane that is not worked out the same way as the other three. A house
/// bought moments ago has not served its wait, so the line takes its dated arm rather than saying
/// nothing.
pub fn the_line_saying_when_another_house_may_be_bought_is_in_the_machines_own_time() {
    let bought_at = the_clock_now() - 10;
    let mut c = a_pane_for_a_villa(bought_at);
    let rows = pane_rows(&c);
    let line = rows
        .last()
        .cloned()
        .expect("the last row of the pane is drawn");
    let at = bought_at + house::PURCHASE_WAIT_SECONDS;

    let it_says_so = line
        == format!(
            "{}{}{}",
            house::BUY_LANDSCAPE_AT,
            the_machines_own(at),
            house::APARTMENT_EXEMPTION
        );
    let the_right_shape = not_the_other_shape(&line);
    let the_right_time = drawn_in_the_machines_time(&line, at);

    c.assert_behaviour(
        "dates.the-line-saying-when-another-house-may-be-bought-is-in-the-machines-own-time",
        move |_| it_says_so && the_right_shape && the_right_time,
    );
    c.shutdown();
}

#[test]
fn scenario_the_line_saying_when_another_house_may_be_bought_is_in_the_machines_own_time() {
    scenario("the_line_saying_when_another_house_may_be_bought_is_in_the_machines_own_time");
}

// ---------------------------------------------------------------------------------------------
// dates.the-chat-stamp-carries-the-machines-own-offset-and-not-a-constant
// ---------------------------------------------------------------------------------------------

/// The one surface whose shape was always right, because that shape says nothing about where in
/// the world it is. What is claimed here is the number behind it.
pub fn the_chat_stamp_carries_the_machines_own_offset() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.tick(6);

    let (stamped_at, offset) = {
        let world = c.view().world();
        (world.scroll.now_unix, world.scroll.utc_offset_secs)
    };
    let the_clock_was_written = stamped_at > 1_700_000_000;
    let it_is_the_machines = offset == local_utc_offset_secs(stamped_at);

    let prefix = dereth_client_model::scroll::timestamp_prefix(stamped_at, offset);
    let hour = prefix.split(':').next().unwrap_or("");
    let the_stamp_is_shaped_right =
        prefix.ends_with(' ') && !hour.is_empty() && (hour.len() == 1 || !hour.starts_with('0'));
    // ...and it is not the shard's time, unless this machine keeps that time.
    let not_a_constant =
        offset == 0 || prefix != dereth_client_model::scroll::timestamp_prefix(stamped_at, 0);

    c.assert_behaviour(
        "dates.the-chat-stamp-carries-the-machines-own-offset-and-not-a-constant",
        move |_| {
            the_clock_was_written
                && it_is_the_machines
                && the_stamp_is_shaped_right
                && not_a_constant
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_chat_stamp_carries_the_machines_own_offset() {
    scenario("the_chat_stamp_carries_the_machines_own_offset");
}

// ---------------------------------------------------------------------------------------------
// dates.the-ban-expiry-keeps-the-other-shape-and-is-in-the-machines-own-time
// ---------------------------------------------------------------------------------------------

/// The clock is read inside the arm that composes the sentence, so what is admissible is one
/// rendering per second the frames could have spanned -- built with the machine's own offset for
/// the instant the ban runs out.
pub fn the_ban_expiry_keeps_its_shape_and_is_in_the_machines_own_time() {
    use dereth_ui_screens::screens::disconnected::{self, DisconnectedScreen};

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let before = the_clock_now();
    c.app_mut().process_logon_event_queue(vec![
        dereth_client_net::client_session::SessionEvent::AccountBanned {
            expiry: 7_200,
            reason: " - testing".to_string(),
        },
    ]);
    c.tick(3);
    let after = the_clock_now();

    let shown = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let s = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **s;
        any.downcast_mut::<DisconnectedScreen>()
            .expect("the client is off the world")
            .shown_text
            .clone()
            .expect("and it says why")
    };

    let admissible: Vec<String> = (before..=after)
        .map(|n| {
            let at = disconnected::ban_expiry_epoch(7_200, n);
            disconnected::account_banned_message(7_200, " - testing", n, local_utc_offset_secs(at))
        })
        .collect();
    let it_is_one_of_them = admissible.contains(&shown);
    // ...and it is none of the shard-time renderings, unless this machine keeps that time.
    let not_the_shards = local_utc_offset_secs(before) == 0
        || !(before..=after)
            .map(|n| disconnected::account_banned_message(7_200, " - testing", n, 0))
            .any(|u| u == shown);
    // The one date in the client drawn in the weekday-and-month-name shape keeps it.
    let it_keeps_its_shape = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        .iter()
        .any(|w| shown.contains(w));

    c.assert_behaviour(
        "dates.the-ban-expiry-keeps-the-other-shape-and-is-in-the-machines-own-time",
        move |_| it_is_one_of_them && not_the_shards && it_keeps_its_shape,
    );
    c.shutdown();
}

#[test]
fn scenario_the_ban_expiry_keeps_its_shape_and_is_in_the_machines_own_time() {
    scenario("the_ban_expiry_keeps_its_shape_and_is_in_the_machines_own_time");
}

// =============================================================================================
// strings.* -- what the shipped text says, and what the player reads
//
// Every pane puts a shipped line through the tidying the client does, so a doubled space in the
// shipped text does not reach the screen. The instrument's own calibration and a guard that an
// element id still exists are folded in below as arms rather than rows.
//
// The reader is the client's own, taken off a whole running client rather than built here, so
// what is asserted is what that client would draw.
// =============================================================================================

/// The table the panes about a death's penalty, allegiance, fellowship and trade all read.
const TEXT_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0001);
/// The table the key-binding page reads.
const KEYS_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0004);
/// The table of titles, and the only home of the line that asks to keep its spaces.
const TITLES_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_000E);
/// The table the key names themselves live on.
const KEY_NAMES_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0007);

/// The id a shipped line is filed under, from the name the client files it under.
fn line_id(token: &str) -> u32 {
    dereth_ui::persist::preferences::token_of(token)
}

/// A whole client, whose own reader is what every scenario below asks.
fn a_client_that_can_read_the_shipped_text() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // The calibration, folded in here rather than kept as a scenario of its own: with no text
    // installed every reading below would be an empty answer dressed as a pass.
    {
        let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
        assert!(
            ui.resolve_string(TEXT_TABLE, line_id("ID_Vitae_Text_Skills"))
                .is_some(),
            "the shipped text has to be readable or nothing here is a measurement"
        );
        assert_eq!(
            ui.resolve_string(TEXT_TABLE, 0xDEAD_BEEF),
            None,
            "and a line that does not exist still answers with nothing"
        );
    }
    c
}

// ---------------------------------------------------------------------------------------------
// strings.a-run-of-spaces-in-a-shipped-line-is-drawn-as-one-and-nothing-else-moves
// ---------------------------------------------------------------------------------------------

/// The premise is asserted as well as the claim: the line really does ship the pair of spaces, so
/// "it was drawn with one" is a collapse and not a line that never had two.
pub fn a_run_of_spaces_in_a_shipped_line_is_drawn_as_one() {
    let mut c = a_client_that_can_read_the_shipped_text();
    let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
    let id = line_id("ID_Vitae_Text_Skills");

    let rendered = ui
        .resolve_string_rendered(TEXT_TABLE, id, &[String::from("5")])
        .expect("the shipped line");
    let it_is_collapsed = rendered.contains("by 5%. A reduction") && !rendered.contains("  ");

    // The premise: what ships.
    let pieces = ui
        .resolve_string_variants(TEXT_TABLE, id)
        .expect("the same line, untidied");
    let it_really_ships_two =
        format!("{}5{}", pieces[0], pieces[1]).contains("by 5%.  A reduction");

    // ...and nothing else moved: a line with no run and no markup is what it always was,
    // character for character. Nearly the whole of the shipped text is in this class.
    let the_rest_is_untouched = [
        "ID_Allegiance_MonarchLabel",
        "ID_SecureTrade_TotalItemsLabel",
    ]
    .into_iter()
    .all(|tok| {
        let id = line_id(tok);
        ui.resolve_string(TEXT_TABLE, id) == ui.resolve_string_unrendered(TEXT_TABLE, id)
    });

    c.assert_behaviour(
        "strings.a-run-of-spaces-in-a-shipped-line-is-drawn-as-one-and-nothing-else-moves",
        move |_| it_is_collapsed && it_really_ships_two && the_rest_is_untouched,
    );
    c.shutdown();
}

#[test]
fn scenario_a_run_of_spaces_in_a_shipped_line_is_drawn_as_one() {
    scenario("a_run_of_spaces_in_a_shipped_line_is_drawn_as_one");
}

// ---------------------------------------------------------------------------------------------
// strings.the-one-line-that-asks-to-keep-its-spaces-keeps-them-and-loses-its-asking
// ---------------------------------------------------------------------------------------------

/// The one line in the whole of the shipped text whose two readings differ by more than a run of
/// spaces.
pub fn the_one_line_that_asks_to_keep_its_spaces_keeps_them() {
    let mut c = a_client_that_can_read_the_shipped_text();
    let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
    let shown = ui
        .resolve_string(TITLES_TABLE, 0x00E3_C684)
        .expect("the shipped title line");
    let it_kept_them_and_lost_the_asking = shown == "Lore Master  Quiz Night";

    c.assert_behaviour(
        "strings.the-one-line-that-asks-to-keep-its-spaces-keeps-them-and-loses-its-asking",
        move |_| it_kept_them_and_lost_the_asking,
    );
    c.shutdown();
}

#[test]
fn scenario_the_one_line_that_asks_to_keep_its_spaces_keeps_them() {
    scenario("the_one_line_that_asks_to_keep_its_spaces_keeps_them");
}

// ---------------------------------------------------------------------------------------------
// strings.a-value-the-caller-did-not-give-is-nothing-rather-than-a-complaint
// ---------------------------------------------------------------------------------------------

/// Both ends of the same line: given both of its values it reads as it should, and given neither
/// it is left with only what stood between them, which the tidying then takes away too.
pub fn a_value_the_caller_did_not_give_is_nothing_rather_than_a_complaint() {
    let mut c = a_client_that_can_read_the_shipped_text();
    let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
    let id = line_id("ID_KeyNameWithSubControl");

    let both = ui
        .resolve_string_rendered(KEY_NAMES_TABLE, id, &["A".into(), "B".into()])
        .expect("the shipped three-piece line");
    let with_both = both == "A B";
    let with_neither = ui
        .resolve_string_rendered(KEY_NAMES_TABLE, id, &[])
        .as_deref()
        == Some("");

    c.assert_behaviour(
        "strings.a-value-the-caller-did-not-give-is-nothing-rather-than-a-complaint",
        move |_| with_both && with_neither,
    );
    c.shutdown();
}

#[test]
fn scenario_a_value_the_caller_did_not_give_is_nothing_rather_than_a_complaint() {
    scenario("a_value_the_caller_did_not_give_is_nothing_rather_than_a_complaint");
}

// ---------------------------------------------------------------------------------------------
// strings.a-row-places-the-values-by-name-so-the-order-they-are-given-in-is-invisible
// ---------------------------------------------------------------------------------------------

/// Four shipped lines, each of which a caller placing values by position would get wrong in a
/// different way, and one negative that a caller placing them by position could never fail.
pub fn a_line_places_the_values_by_name_and_not_by_the_order_given() {
    use dereth_ui_screens::panels::characterinfo::{
        compose, compose_in, query_duration, string, var, DURATION_TABLE,
    };

    let mut c = a_client_that_can_read_the_shipped_text();

    // 1. Two lines that name the same pair the opposite way round, from one named supply.
    let (one_conflict, one_line) = {
        let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
        let values = [("ACTION", "Turn Left"), ("KEY", "F7")];
        (
            ui.resolve_string_named(
                KEYS_TABLE,
                line_id("ID_ActionKeyMap_OverwriteExistingBinding"),
                &values,
            ),
            ui.resolve_string_named(KEYS_TABLE, line_id("ID_ActionKeyMap_Binding"), &values),
        )
    };
    let both_ways_round = one_conflict.as_deref()
        == Some("'F7' is currently bound to 'Turn Left'. Do you wish to erase that binding?")
        && one_line.as_deref() == Some("'Turn Left' ('F7')");

    // 2. A line that names the same value twice writes it twice, whichever way it is handed in.
    let (in_order, reversed) = {
        let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
        (
            compose(
                ui,
                string::RESISTS,
                &[(var::RESIST, "Hardy"), (var::REGEN, "Poor")],
            ),
            compose(
                ui,
                string::RESISTS,
                &[(var::REGEN, "Poor"), (var::RESIST, "Hardy")],
            ),
        )
    };
    let the_duplicate_is_written_twice = !in_order.starts_with(string::RESISTS)
        && in_order.matches("Hardy").count() == 2
        && in_order.matches("Poor").count() == 1
        && in_order == reversed;

    // 3. The widest line this client composes: seven terms, three of them blank, and the blanks
    // take their own words away with them.
    let (forward, backward) = {
        let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
        // A day, an hour, a minute and a second; years, months and weeks are nothing at all.
        let terms = query_duration(90_061);
        let named: Vec<(&str, &str)> = var::DURATION_TERMS
            .iter()
            .copied()
            .zip(terms.iter().map(String::as_str))
            .collect();
        let mut shuffled = named.clone();
        shuffled.reverse();
        (
            compose_in(ui, DURATION_TABLE, string::DURATION_FORMAT, &named),
            compose_in(ui, DURATION_TABLE, string::DURATION_FORMAT, &shuffled),
        )
    };
    let seven_terms_by_name = forward == "1 day 1 hour 1 minute 1 second"
        && forward == backward
        && !forward.contains('{')
        && !forward.contains("#1:");

    // 4. The two lines about the link, and the negative: a name the line does not carry puts
    // nothing on the screen, so the composer falls back to saying nothing but the line's own name.
    let (ping, loss, wrong) = {
        use dereth_ui_screens::panels::linkstatus::{string as link, var as link_var};
        let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;
        (
            compose(ui, link::PING, &[(link_var::PING, "42")]),
            compose(ui, link::PACKET_LOSS, &[(link_var::PACKET_LOSS, "1.50")]),
            compose(ui, link::PING, &[("PONG", "42")]),
        )
    };
    let the_link_lines = {
        use dereth_ui_screens::panels::linkstatus::string as link;
        !ping.starts_with(link::PING)
            && ping.contains("42")
            && !loss.starts_with(link::PACKET_LOSS)
            && loss.contains("1.50")
            && wrong == "ID_LinkStatus_Ping[42]"
    };

    c.assert_behaviour(
        "strings.a-row-places-the-values-by-name-so-the-order-they-are-given-in-is-invisible",
        move |_| {
            both_ways_round
                && the_duplicate_is_written_twice
                && seven_terms_by_name
                && the_link_lines
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_line_places_the_values_by_name_and_not_by_the_order_given() {
    scenario("a_line_places_the_values_by_name_and_not_by_the_order_given");
}

// ---------------------------------------------------------------------------------------------
// strings.the-key-binding-prompt-loses-both-of-its-double-spaces
// ---------------------------------------------------------------------------------------------

/// The second place on the screen where the tidying is the difference between what ships and what
/// is read -- and this line ships two runs rather than one.
pub fn the_key_binding_prompt_loses_both_of_its_double_spaces() {
    let mut c = a_client_that_can_read_the_shipped_text();
    let ui = &c.app_mut().ui_mut().expect("the UI shell is up").ui;

    let prompt = dereth_ui_screens::options::keybinding::map_instructions(ui, "Jump");
    let both_runs_gone = prompt.contains("the 'Jump' action. You may")
        && prompt.contains("new mappings. Remapping")
        && !prompt.contains("  ");
    // The premise: the line really does ship one of them.
    let it_really_ships_them = ui
        .resolve_string_variants(KEYS_TABLE, line_id("ID_ActionKeyMap_MapInstructions"))
        .expect("the shipped line")[1]
        .contains("action.  You");

    c.assert_behaviour(
        "strings.the-key-binding-prompt-loses-both-of-its-double-spaces",
        move |_| both_runs_gone && it_really_ships_them,
    );
    c.shutdown();
}

#[test]
fn scenario_the_key_binding_prompt_loses_both_of_its_double_spaces() {
    scenario("the_key_binding_prompt_loses_both_of_its_double_spaces");
}

// ---------------------------------------------------------------------------------------------
// strings.the-pane-draws-the-tidied-line-and-not-the-shipped-double-space
// ---------------------------------------------------------------------------------------------

/// Read off the glyphs the pane lays out, not off what the composer answered -- and the pane it
/// is read from is the one the shipped layout carries, which is the guard folded in here.
pub fn the_pane_draws_the_tidied_line_and_not_the_shipped_double_space() {
    use dereth_client_model::enchant::ench_type;

    let (mut c, _peer) = a_described_character();
    // A death's penalty, which is what the pane is about, through the one door one comes in by.
    enchant(
        &mut c,
        an_enchantment(666, 0, ench_type::VITAE | ench_type::MULTIPLICATIVE, 0.95),
    );
    light_the_lamp(&mut c, VITAE_LAMP);
    press_the_strip(&mut c, VITAE_LAMP);
    c.tick(2);

    let panel = hud_find(&c, dereth_ui_screens::panels::vitae::PANEL);
    let it_is_the_shipped_pane = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(panel)
        .expect("alive")
        .element_id()
        == ElementId(0x1000_018A);
    let main = hud_find(&c, dereth_ui_screens::panels::vitae::MAIN_TEXT);

    let drawn: String = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let box_ = shell.ui.screen_box(main);
        let t = shell
            .ui
            .text_element_mut(main)
            .expect("the pane's words are a text element");
        t.compose(box_)
            .into_iter()
            .filter_map(|g| char::from_u32(u32::from(g.ch)))
            .collect()
    };
    let one_space = drawn.contains("5%. A reduction") && !drawn.contains("  ");

    c.assert_behaviour(
        "strings.the-pane-draws-the-tidied-line-and-not-the-shipped-double-space",
        move |_| it_is_the_shipped_pane && one_space,
    );
    c.shutdown();
}

#[test]
fn scenario_the_pane_draws_the_tidied_line_and_not_the_shipped_double_space() {
    scenario("the_pane_draws_the_tidied_line_and_not_the_shipped_double_space");
}

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
pub fn every_recorded_refusal_and_nothing_else_reaches_the_strip() {
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

#[test]
fn scenario_every_recorded_refusal_and_nothing_else_reaches_the_strip() {
    scenario("every_recorded_refusal_and_nothing_else_reaches_the_strip");
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
pub fn a_refusal_about_something_the_player_asked_for_names_it_and_says_why() {
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

#[test]
fn scenario_a_refusal_about_something_the_player_asked_for_names_it_and_says_why() {
    scenario("a_refusal_about_something_the_player_asked_for_names_it_and_says_why");
}
