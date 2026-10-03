//! The character, examination and dialog panels, read off the shipped element tree: the skills,
//! attributes and titles pages, the identify pane, the House pane, the notebook, inscriptions,
//! confirmation dialogs and the chess board. Fixture: the retail dats plus recorded characters and
//! answers replayed through `Inbound`, driven by a headless client.
//!
//! Each scenario is a `pub fn` with a `#[test]` beside it, listed in `ALL` for the census
//! (`census.rs`), so a scenario written and not listed shows up as a shortfall.
//! **This binary must run serially:** two headless clients in one process share the UI request
//! globals. [`glyph_runs`] and [`state_colours`] read the colour a composed glyph is drawn in,
//! which `UiSnapshot` cannot see.

use std::sync::Arc;

use dereth_client::app::App;
use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::types::AppraisalProfile;
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound, Peer, Player, ScreenPoint, Target};
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::panels::{attributes, examination, remaining, skills, statmgmt};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "the_skills_page_builds_a_header_per_group_and_a_row_per_recorded_skill",
        &["skills.list.a-header-per-group-and-a-row-per-skill-the-shard-sent"],
        the_skills_page_builds_a_header_per_group_and_a_row_per_recorded_skill,
    ),
    (
        "an_empty_skills_page_is_distinguishable_from_a_full_one",
        &["skills.list.a-page-that-built-nothing-is-told-apart-from-one-that-built-everything"],
        an_empty_skills_page_is_distinguishable_from_a_full_one,
    ),
    (
        "the_footer_title_is_the_name_the_number_and_the_signed_change",
        &["skills.footer.the-title-is-the-name-and-the-number-with-the-change-spelled-out-after-it"],
        the_footer_title_is_the_name_the_number_and_the_signed_change,
    ),
    (
        "the_experience_to_raise_line_is_legible_and_uncovered",
        &["skills.footer.the-experience-line-is-legible-and-nothing-is-painted-over-it"],
        the_experience_to_raise_line_is_legible_and_uncovered,
    ),
    (
        "a_buffed_attribute_row_is_green_a_debuffed_one_red_and_the_rest_white",
        &["attributes.rows.a-buffed-value-is-green-a-debuffed-one-red-and-an-unmodified-one-white"],
        a_buffed_attribute_row_is_green_a_debuffed_one_red_and_the_rest_white,
    ),
    (
        "a_vital_row_is_current_over_maximum_and_colours_by_the_maximum",
        &["attributes.rows.a-vital-row-is-current-over-maximum-and-takes-its-colour-from-the-maximum"],
        a_vital_row_is_current_over_maximum_and_colours_by_the_maximum,
    ),
    (
        "a_fractional_enchantment_rounds_in_the_rows_the_player_reads",
        &["attributes.rows.a-fractional-enchantment-rounds-the-way-the-panel-draws-it"],
        a_fractional_enchantment_rounds_in_the_rows_the_player_reads,
    ),
    (
        "ten_points_cost_the_distance_between_two_entries_of_the_shipped_table",
        &["advancement.cost.the-ten-point-cost-comes-off-the-shipped-experience-table"],
        ten_points_cost_the_distance_between_two_entries_of_the_shipped_table,
    ),
    (
        "the_plus_ten_button_lights_when_the_unassigned_experience_covers_it",
        &["advancement.cost.the-plus-ten-button-is-affordable-when-the-unassigned-experience-covers-it"],
        the_plus_ten_button_lights_when_the_unassigned_experience_covers_it,
    ),
    (
        "an_assessed_item_names_its_spells_and_describes_them",
        &["examine.spells.an-item-with-spells-on-it-names-them-and-describes-them"],
        an_assessed_item_names_its_spells_and_describes_them,
    ),
    (
        "an_enchantment_is_listed_apart_from_the_items_own_spells",
        &["examine.spells.an-enchantment-is-listed-apart-from-the-spells-the-item-was-made-with"],
        an_enchantment_is_listed_apart_from_the_items_own_spells,
    ),
    (
        "the_magic_lines_come_in_order_and_a_rate_beats_a_flat_cost",
        &["examine.spells.the-magic-lines-come-in-the-clients-own-order-and-a-rate-beats-a-flat-cost"],
        the_magic_lines_come_in_order_and_a_rate_beats_a_flat_cost,
    ),
    (
        "an_unsuccessful_assessment_says_unknown_and_a_plain_item_says_nothing",
        &["examine.spells.an-unsuccessful-assessment-says-so-and-a-plain-item-says-nothing"],
        an_unsuccessful_assessment_says_unknown_and_a_plain_item_says_nothing,
    ),
    (
        "an_item_that_expires_says_when_and_needs_all_three_numbers",
        &["examine.lifespan.an-item-that-expires-says-when-and-needs-all-three-of-its-numbers"],
        an_item_that_expires_says_when_and_needs_all_three_numbers,
    ),
    (
        "the_creature_pane_names_the_kind_the_level_and_the_nine_rows",
        &["examine.creature.the-pane-names-the-kind-the-level-six-attributes-and-three-vitals"],
        the_creature_pane_names_the_kind_the_level_and_the_nine_rows,
    ),
    (
        "the_armour_pane_gives_its_level_and_eight_resistances",
        &["examine.armour.the-pane-gives-its-level-and-eight-resistances-in-the-order-it-draws-them"],
        the_armour_pane_gives_its_level_and_eight_resistances,
    ),
    (
        "the_weapon_pane_gives_the_skill_the_speed_the_range_and_the_ammunition",
        &["examine.weapon.the-pane-gives-the-skill-the-damage-the-speed-the-range-and-the-ammunition"],
        the_weapon_pane_gives_the_skill_the_speed_the_range_and_the_ammunition,
    ),
    (
        "an_item_on_someone_elses_hook_takes_its_slot_from_the_reply",
        &["examine.weapon.an-item-on-someone-elses-hook-takes-its-slot-from-the-reply"],
        an_item_on_someone_elses_hook_takes_its_slot_from_the_reply,
    ),
    (
        "the_inscribe_box_appears_only_on_something_inscribable",
        &["examine.inscription.the-box-is-there-only-on-something-that-can-be-inscribed"],
        the_inscribe_box_appears_only_on_something_inscribable,
    ),
    (
        "the_assess_key_shuts_an_open_pane_and_opens_a_shut_one",
        &["examine.key.the-assess-key-shuts-an-open-pane-and-opens-a-shut-one"],
        the_assess_key_shuts_an_open_pane_and_opens_a_shut_one,
    ),
    (
        "the_assess_key_shuts_the_pane_with_nothing_under_the_pointer",
        &["examine.key.the-pane-shuts-with-nothing-under-the-pointer-and-a-shut-one-does-nothing"],
        the_assess_key_shuts_the_pane_with_nothing_under_the_pointer,
    ),
    (
        "a_potion_says_what_it_restores_and_a_kit_what_it_adds",
        &["examine.consumables.a-potion-says-what-it-restores-and-a-kit-what-it-adds"],
        a_potion_says_what_it_restores_and_a_kit_what_it_adds,
    ),
    (
        "a_container_says_how_much_it_holds_and_a_book_how_many_pages_are_used",
        &["examine.capacity.a-container-says-how-much-it-holds-and-a-book-how-many-pages-are-used"],
        a_container_says_how_much_it_holds_and_a_book_how_many_pages_are_used,
    ),
    (
        "every_line_the_item_pane_can_draw_reaches_it_through_the_shard",
        &["examine.item-blocks.every-line-the-item-pane-can-draw-reaches-it-through-the-shard"],
        every_line_the_item_pane_can_draw_reaches_it_through_the_shard,
    ),
    (
        "the_character_pane_draws_its_rows_and_has_nothing_left_undrawn",
        &["examine.character.the-pane-draws-its-rows-and-has-nothing-left-undrawn"],
        the_character_pane_draws_its_rows_and_has_nothing_left_undrawn,
    ),
    (
        "a_spell_cast_while_playing_moves_the_skill_row_it_is_about",
        &["skills.rows.a-spell-cast-while-playing-moves-the-skill-row-it-is-about"],
        a_spell_cast_while_playing_moves_the_skill_row_it_is_about,
    ),
    (
        "a_character_who_logs_in_already_enchanted_reads_the_same_total",
        &["skills.rows.a-character-who-logs-in-already-enchanted-reads-the-same-total"],
        a_character_who_logs_in_already_enchanted_reads_the_same_total,
    ),
    (
        "the_shards_house_message_fills_every_row_of_the_tab",
        &["house.data.the-shards-house-message-fills-every-row-of-the-tab"],
        the_shards_house_message_fills_every_row_of_the_tab,
    ),
    (
        "the_purchase_time_line_reads_what_retail_prints_in_each_arm",
        &["house.purchase-time.the-line-reads-what-retail-prints-in-each-of-its-three-arms"],
        the_purchase_time_line_reads_what_retail_prints_in_each_arm,
    ),
    (
        "a_paid_up_apartment_drops_the_location_row_and_doubles_the_period",
        &["house.data.a-paid-up-apartment-drops-the-location-row-and-doubles-the-period"],
        a_paid_up_apartment_drops_the_location_row_and_doubles_the_period,
    ),
    (
        "a_zero_instant_prints_the_sentinel_and_not_the_epoch",
        &["house.data.a-zero-instant-prints-the-sentinel-and-not-the-start-of-the-epoch"],
        a_zero_instant_prints_the_sentinel_and_not_the_epoch,
    ),
    (
        "a_house_row_is_as_tall_as_the_text_it_holds",
        &["house.rows.a-row-is-as-tall-as-the-text-it-holds-and-fits-inside-the-pane"],
        a_house_row_is_as_tall_as_the_text_it_holds,
    ),
    (
        "the_paid_maintenance_line_is_drawn_in_the_templates_green",
        &["house.rows.the-paid-maintenance-line-is-drawn-in-the-colour-the-template-ships"],
        the_paid_maintenance_line_is_drawn_in_the_templates_green,
    ),
    (
        "the_house_price_rows_are_comma_joined_and_grouped",
        &["house.prices.the-price-rows-are-comma-joined-and-their-counts-are-grouped"],
        the_house_price_rows_are_comma_joined_and_grouped,
    ),
    (
        "the_client_asks_about_the_house_once_when_it_learns_who_it_is",
        &["house.query.the-client-asks-about-the-house-once-when-it-learns-who-it-is"],
        the_client_asks_about_the_house_once_when_it_learns_who_it_is,
    ),
    (
        "a_new_maintenance_period_clears_every_payment",
        &["house.rent.a-new-period-moves-both-dates-and-marks-every-payment-unpaid"],
        a_new_maintenance_period_clears_every_payment,
    ),
    (
        "a_payment_update_replaces_the_list_rather_than_merging",
        &["house.rent.a-payment-update-replaces-the-list-rather-than-merging-into-it"],
        a_payment_update_replaces_the_list_rather_than_merging,
    ),
    (
        "neither_maintenance_update_touches_a_houseless_character",
        &["house.rent.neither-update-touches-a-character-with-no-house"],
        neither_maintenance_update_touches_a_houseless_character,
    ),
    (
        "a_restriction_update_reaches_the_object_it_names",
        &["house.restrictions.an-update-reaches-the-object-it-names-and-never-the-player"],
        a_restriction_update_reaches_the_object_it_names,
    ),
    (
        "a_transaction_answer_clears_the_pane_as_a_status_does",
        &["house.data.a-transaction-answer-clears-the-pane-exactly-as-a-status-answer-does"],
        a_transaction_answer_clears_the_pane_as_a_status_does,
    ),
    (
        "two_confirmations_send_the_abandon_and_only_the_shard_empties_the_pane",
        &["house.abandon.two-confirmations-send-it-and-only-the-shards-answer-empties-the-pane"],
        two_confirmations_send_the_abandon_and_only_the_shard_empties_the_pane,
    ),
    (
        "the_same_command_twice_asks_twice_one_question_after_the_other",
        &["dialog.confirmation.the-same-command-twice-asks-twice-one-question-after-the-other"],
        the_same_command_twice_asks_twice_one_question_after_the_other,
    ),
    (
        "the_augmentation_cost_line_comes_off_the_shipped_sentence",
        &["examine.augmentation.the-cost-line-is-the-shipped-sentence-with-the-number-in-it"],
        the_augmentation_cost_line_comes_off_the_shipped_sentence,
    ),
    (
        "the_cooldown_lines_reach_the_item_pane",
        &["examine.cooldown.the-pane-says-how-long-it-is-and-how-long-is-left-of-the-players-own"],
        the_cooldown_lines_reach_the_item_pane,
    ),
    (
        "the_long_description_is_decorated_where_the_player_reads_it",
        &["examine.description.the-long-one-is-decorated-and-a-plating-name-replaces-it"],
        the_long_description_is_decorated_where_the_player_reads_it,
    ),
    (
        "the_short_description_is_used_only_when_there_is_no_long_one",
        &["examine.description.the-short-one-is-drawn-only-when-there-is-no-long-one-at-all"],
        the_short_description_is_used_only_when_there_is_no_long_one,
    ),
    (
        "each_portal_restriction_draws_its_own_sentence_and_only_its_own",
        &["examine.portal.each-restriction-draws-its-own-sentence-and-only-its-own"],
        each_portal_restriction_draws_its_own_sentence_and_only_its_own,
    ),
    (
        "a_portal_with_no_restrictions_still_gets_the_blocks_separators",
        &["examine.portal.a-portal-with-no-restriction-still-gains-the-blocks-two-separators"],
        a_portal_with_no_restrictions_still_gets_the_blocks_separators,
    ),
    (
        "a_failed_assessment_draws_every_value_cell_in_the_unknown_colour",
        &["examine.failed-assess.every-value-cell-takes-the-unknown-colour-and-no-label-does"],
        a_failed_assessment_draws_every_value_cell_in_the_unknown_colour,
    ),
    (
        "a_failed_assessment_keeps_the_numbers_and_reduces_health_to_a_percentage",
        &["examine.failed-assess.the-numbers-the-shard-described-are-kept-and-health-is-a-percentage"],
        a_failed_assessment_keeps_the_numbers_and_reduces_health_to_a_percentage,
    ),
    (
        "the_failure_colour_outranks_the_enchantment_colour",
        &["examine.failed-assess.an-unassessed-stat-reads-unknown-even-when-it-is-enchanted"],
        the_failure_colour_outranks_the_enchantment_colour,
    ),
    (
        "a_failed_assessment_leaves_the_item_panes_value_line_plain",
        &["examine.failed-assess.the-item-panes-own-unknown-value-line-is-drawn-plain"],
        a_failed_assessment_leaves_the_item_panes_value_line_plain,
    ),
    (
        "the_description_pane_shows_a_bar_only_when_its_text_does_not_fit",
        &["examine.scroll.the-description-pane-shows-a-bar-only-when-its-text-does-not-fit"],
        the_description_pane_shows_a_bar_only_when_its_text_does_not_fit,
    ),
    (
        "the_bar_really_scrolls_the_description",
        &["examine.scroll.the-bar-brings-the-end-of-a-long-description-into-view"],
        the_bar_really_scrolls_the_description,
    ),
    (
        "a_raised_line_is_green_a_lowered_one_red_and_the_rest_plain",
        &["examine.enchanted.a-raised-line-is-green-a-lowered-one-red-and-everything-else-plain"],
        a_raised_line_is_green_a_lowered_one_red_and_the_rest_plain,
    ),
    (
        "an_answer_the_pane_did_not_ask_for_opens_nothing",
        &["examine.window.an-answer-the-pane-did-not-ask-for-opens-nothing-and-a-repeat-does-not-reopen-it"],
        an_answer_the_pane_did_not_ask_for_opens_nothing,
    ),
    (
        "the_identify_button_is_what_starts_the_chain",
        &["examine.window.the-identify-button-is-what-makes-the-pane-wait-for-an-answer"],
        the_identify_button_is_what_starts_the_chain,
    ),
    (
        "the_character_pane_rows_are_the_shards_own_numbers",
        &["examine.character.the-armour-rows-are-the-shards-own-numbers-and-the-three-lines-beside-them"],
        the_character_pane_rows_are_the_shards_own_numbers,
    ),
    (
        "the_society_row_names_the_society_and_its_rank_band",
        &["examine.character.the-society-row-names-the-society-and-the-band-its-rank-falls-in"],
        the_society_row_names_the_society_and_its_rank_band,
    ),
    (
        "the_society_row_is_coloured_by_the_viewers_own_society",
        &["examine.character.the-society-row-is-coloured-by-the-viewers-own-society"],
        the_society_row_is_coloured_by_the_viewers_own_society,
    ),
    (
        "the_allegiance_rows_fork_on_the_two_titles",
        &["examine.character.the-allegiance-rows-fork-on-whether-the-monarch-is-also-the-patron"],
        the_allegiance_rows_fork_on_the_two_titles,
    ),
    (
        "an_allegiance_rank_puts_its_title_in_front_of_the_name",
        &["examine.character.an-allegiance-rank-puts-its-title-in-front-of-the-name"],
        an_allegiance_rank_puts_its_title_in_front_of_the_name,
    ),
    (
        "an_unenchantable_body_part_is_marked_with_a_star",
        &["examine.character.a-body-part-nothing-can-be-cast-on-is-marked-and-reads-what-is-left"],
        an_unenchantable_body_part_is_marked_with_a_star,
    ),
    (
        "the_rating_groups_draw_the_pairs_the_pane_prints",
        &["examine.character.the-rating-groups-draw-the-pairs-the-pane-prints-and-no-others"],
        the_rating_groups_draw_the_pairs_the_pane_prints,
    ),
    (
        "the_creature_pane_draws_the_same_rating_rows_without_the_footnote",
        &["examine.creature.the-creature-pane-draws-the-same-rating-rows-without-the-footnote"],
        the_creature_pane_draws_the_same_rating_rows_without_the_footnote,
    ),
    (
        "the_tail_rows_are_the_seven_the_pane_lists",
        &["examine.character.the-seven-tail-rows-each-draw-the-thing-behind-them"],
        the_tail_rows_are_the_seven_the_pane_lists,
    ),
    (
        "every_character_row_draws_in_the_panes_own_order",
        &["examine.character.every-row-draws-in-the-panes-own-order-and-reaches-the-list"],
        every_character_row_draws_in_the_panes_own_order,
    ),
    (
        "an_npc_question_appears_and_both_answers_reach_the_shard",
        &["dialog.confirmation.a-question-appears-and-both-answers-reach-the-shard"],
        an_npc_question_appears_and_both_answers_reach_the_shard,
    ),
    (
        "a_making_question_gets_the_clients_own_continue",
        &["dialog.confirmation.a-question-about-making-something-adds-the-clients-own-continue"],
        a_making_question_gets_the_clients_own_continue,
    ),
    (
        "the_shards_withdrawal_closes_the_question_and_refuses_on_the_way_out",
        &["dialog.confirmation.the-shards-withdrawal-closes-it-and-refuses-on-the-way-out"],
        the_shards_withdrawal_closes_the_question_and_refuses_on_the_way_out,
    ),
    (
        "a_second_question_while_one_is_open_takes_over_its_handle",
        &["dialog.confirmation.a-second-question-while-one-is-open-takes-over-its-handle-and-is-not-queued"],
        a_second_question_while_one_is_open_takes_over_its_handle,
    ),
    (
        "an_invitation_and_a_swearing_raise_their_own_panels_questions",
        &["dialog.confirmation.an-invitation-and-a-swearing-ask-their-own-panels-question"],
        an_invitation_and_a_swearing_raise_their_own_panels_questions,
    ),
    (
        "the_invitation_is_centred_and_a_real_inscription_is_not",
        &["examine.inscription.the-invitation-is-centred-and-a-real-inscription-is-not"],
        the_invitation_is_centred_and_a_real_inscription_is_not,
    ),
    (
        "a_press_empties_the_box_and_shows_whose_signature_it_will_carry",
        &["examine.inscription.a-press-empties-the-box-and-shows-whose-signature-it-will-carry"],
        a_press_empties_the_box_and_shows_whose_signature_it_will_carry,
    ),
    (
        "letting_the_caret_go_sends_what_was_written",
        &["examine.inscription.letting-the-caret-go-sends-what-was-written-and-it-comes-back"],
        letting_the_caret_go_sends_what_was_written,
    ),
    (
        "leaving_an_unchanged_box_sends_nothing",
        &["examine.inscription.leaving-an-unchanged-box-sends-nothing-and-the-invitation-comes-back"],
        leaving_an_unchanged_box_sends_nothing,
    ),
    (
        "escape_is_the_confirm_edge_and_return_is_not",
        &["examine.inscription.escape-is-the-edge-that-confirms-it-and-return-is-not"],
        escape_is_the_confirm_edge_and_return_is_not,
    ),
    (
        "a_box_somebody_else_signed_is_not_yours_to_change",
        &["examine.inscription.a-box-somebody-else-signed-is-not-yours-to-change"],
        a_box_somebody_else_signed_is_not_yours_to_change,
    ),
    (
        "a_signed_thing_with_no_words_shows_a_blank_box",
        &["examine.inscription.a-signed-thing-with-no-words-shows-a-blank-box-that-is-the-scribes"],
        a_signed_thing_with_no_words_shows_a_blank_box,
    ),
    (
        "the_scribe_can_rub_out_his_own_words",
        &["examine.inscription.the-writer-can-rub-out-his-own-words-and-the-invitation-comes-back"],
        the_scribe_can_rub_out_his_own_words,
    ),
    (
        "the_close_button_commits_on_its_way_out",
        &["examine.inscription.the-windows-close-button-commits-what-was-typed-on-its-way-out"],
        the_close_button_commits_on_its_way_out,
    ),
    (
        "changing_the_target_to_one_you_may_not_write_on_drops_the_caret",
        &["examine.inscription.looking-at-one-you-may-not-write-on-takes-the-caret-away"],
        changing_the_target_to_one_you_may_not_write_on_drops_the_caret,
    ),
    (
        "changing_to_another_one_you_may_write_on_keeps_the_caret",
        &["examine.inscription.looking-at-another-you-may-write-on-keeps-the-caret-where-it-was"],
        changing_to_another_one_you_may_write_on_keeps_the_caret,
    ),
    (
        "the_journal_opens_on_page_one_with_the_timer_editable",
        &["journal.page.the-tab-opens-on-page-one-with-the-timer-ready-to-set"],
        the_journal_opens_on_page_one_with_the_timer_editable,
    ),
    (
        "a_typed_title_survives_a_page_turn",
        &["journal.page.a-title-typed-on-one-page-is-there-on-the-way-back"],
        a_typed_title_survives_a_page_turn,
    ),
    (
        "the_journal_timer_counts_down_and_the_button_resets_it",
        &["journal.timer.it-counts-down-on-its-own-and-the-button-puts-it-back"],
        the_journal_timer_counts_down_and_the_button_resets_it,
    ),
    (
        "the_page_list_lists_the_journals_pages",
        &["journal.list.the-list-is-the-journals-own-pages-in-page-order"],
        the_page_list_lists_the_journals_pages,
    ),
    (
        "only_the_search_control_filters_the_page_list",
        &["journal.list.typing-alone-does-not-filter-and-the-search-control-does"],
        only_the_search_control_filters_the_page_list,
    ),
    (
        "reset_clears_the_search_and_deletes_nothing",
        &["journal.list.the-reset-control-clears-the-search-and-deletes-no-page"],
        reset_clears_the_search_and_deletes_nothing,
    ),
    (
        "pressing_a_row_then_delete_removes_that_rows_page",
        &["journal.list.a-pressed-row-is-the-one-the-delete-control-removes"],
        pressing_a_row_then_delete_removes_that_rows_page,
    ),
    (
        "a_double_press_on_a_row_opens_that_page",
        &["journal.list.a-double-press-on-a-row-opens-that-page-in-the-journal"],
        a_double_press_on_a_row_opens_that_page,
    ),
    (
        "a_journal_page_survives_a_restart",
        &["journal.file.a-page-written-here-is-on-disk-in-the-shipped-format-and-comes-back"],
        a_journal_page_survives_a_restart,
    ),
    (
        "a_client_that_does_not_know_its_character_writes_nothing",
        &["journal.file.a-client-that-does-not-know-its-character-writes-no-notebook"],
        a_client_that_does_not_know_its_character_writes_nothing,
    ),
    (
        "a_notebook_in_the_shipped_format_is_read_and_written_back",
        &["journal.file.a-notebook-in-the-shipped-format-is-read-and-written-back-in-place"],
        a_notebook_in_the_shipped_format_is_read_and_written_back,
    ),
    (
        "a_second_character_gets_its_own_notebook",
        &["journal.file.a-second-character-gets-a-notebook-of-their-own"],
        a_second_character_gets_its_own_notebook,
    ),
    (
        "a_relog_keeps_each_characters_notebook",
        &["journal.file.a-relog-keeps-each-characters-notebook-and-saves-the-page-still-open"],
        a_relog_keeps_each_characters_notebook,
    ),
    (
        "the_titles_tab_draws_every_earned_title_sorted_by_name",
        &["titles.tab.the-tab-draws-every-earned-title-sorted-by-name"],
        the_titles_tab_draws_every_earned_title_sorted_by_name,
    ),
    (
        "picking_a_title_arms_the_button_and_sends_nothing",
        &["titles.tab.picking-a-row-arms-the-button-only-for-a-title-not-already-worn"],
        picking_a_title_arms_the_button_and_sends_nothing,
    ),
    (
        "the_display_button_puts_the_set_title_request_on_the_wire",
        &["titles.tab.the-button-sends-one-request-for-the-picked-title-and-nothing-local-moves"],
        the_display_button_puts_the_set_title_request_on_the_wire,
    ),
    (
        "the_ten_point_button_lights_up_and_a_real_press_sends_the_raise",
        &["advancement.raise.the-ten-point-button-lights-up-with-the-experience-and-a-press-sends-the-raise"],
        the_ten_point_button_lights_up_and_a_real_press_sends_the_raise,
    ),
    (
        "every_house_sub_command_that_sends_puts_its_own_message_on_the_wire",
        &["house.commands.every-sub-command-that-sends-puts-its-own-message-on-the-wire"],
        every_house_sub_command_that_sends_puts_its_own_message_on_the_wire,
    ),
    (
        "a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing",
        &["house.commands.a-line-the-ladder-refuses-prints-the-shipped-sentence-and-sends-nothing"],
        a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing,
    ),
    (
        "the_help_listing_and_the_house_help_are_what_the_client_ships",
        &["house.commands.the-help-listing-and-the-house-help-are-what-the-client-ships"],
        the_help_listing_and_the_house_help_are_what_the_client_ships,
    ),
    (
        "the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log",
        &["house.guests.the-guest-list-the-shard-sends-back-is-written-out-on-the-chat-log"],
        the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log,
    ),
    (
        "the_free_houses_the_shard_lists_are_written_out_with_their_locations",
        &["house.available.the-free-houses-the-shard-lists-are-written-out-with-their-locations"],
        the_free_houses_the_shard_lists_are_written_out_with_their_locations,
    ),
    (
        "a_press_picks_the_attribute_row_under_it_and_lands_on_the_list",
        &["attributes.selection.a-press-picks-the-row-under-the-pointer-through-the-list-itself"],
        a_press_picks_the_attribute_row_under_it_and_lands_on_the_list,
    ),
    (
        "a_press_picks_the_skill_row_under_it_and_the_attribute_page_stays_put",
        &["skills.selection.a-press-picks-the-skill-row-under-it-and-the-other-page-stays-put"],
        a_press_picks_the_skill_row_under_it_and_the_attribute_page_stays_put,
    ),
    (
        "a_raise_sends_nothing_until_a_skill_row_is_picked",
        &["advancement.raise.a-raise-sends-nothing-until-a-row-is-picked-and-then-names-it-and-the-cost"],
        a_raise_sends_nothing_until_a_skill_row_is_picked,
    ),
    (
        "every_skill_row_draws_its_value_in_a_font_that_exists",
        &["skills.rows.every-row-draws-its-value-in-a-font-that-exists-and-the-colour-it-declares"],
        every_skill_row_draws_its_value_in_a_font_that_exists,
    ),
    (
        "every_attribute_row_draws_the_icon_its_own_group_names",
        &["attributes.rows.every-row-draws-the-icon-its-own-group-names"],
        every_attribute_row_draws_the_icon_its_own_group_names,
    ),
    (
        "the_character_pages_numbers_carry_the_shipped_separator",
        &["skills.numbers.the-experience-numbers-are-grouped-with-the-shipped-separator"],
        the_character_pages_numbers_carry_the_shipped_separator,
    ),
    (
        "the_shards_abuse_answer_writes_the_result_line_and_nothing_in_chat",
        &["abuse.response.the-shards-answer-writes-the-result-line-and-nothing-in-chat"],
        the_shards_abuse_answer_writes_the_result_line_and_nothing_in_chat,
    ),
    (
        "the_abuse_page_sends_one_report_and_then_empties_itself",
        &["abuse.report.the-page-sends-one-report-naming-who-and-why-and-then-empties-itself"],
        the_abuse_page_sends_one_report_and_then_empties_itself,
    ),
    (
        "the_skills_list_is_one_column_of_equal_rows_mostly_out_of_sight",
        &["skills.scroll.the-list-is-one-column-of-equal-rows-and-most-of-it-is-out-of-sight"],
        the_skills_list_is_one_column_of_equal_rows_mostly_out_of_sight,
    ),
    (
        "the_bar_beside_the_skills_list_reports_the_view_and_where_it_is",
        &["skills.scroll.the-bar-beside-the-list-says-how-much-is-in-view-and-where-it-is"],
        the_bar_beside_the_skills_list_reports_the_view_and_where_it_is,
    ),
    (
        "each_arrow_moves_the_skills_list_one_row_its_own_way",
        &["skills.scroll.each-arrow-moves-the-list-one-row-its-own-way-and-the-top-is-the-top"],
        each_arrow_moves_the_skills_list_one_row_its_own_way,
    ),
    (
        "a_press_on_a_scrolled_skills_list_picks_the_row_it_is_drawn_over",
        &["skills.scroll.a-press-on-a-scrolled-list-picks-the-row-it-is-drawn-over"],
        a_press_on_a_scrolled_skills_list_picks_the_row_it_is_drawn_over,
    ),
    (
        "rebuilding_the_skills_list_keeps_where_it_was_scrolled_to",
        &["skills.scroll.rebuilding-the-list-keeps-where-it-was-scrolled-to"],
        rebuilding_the_skills_list_keeps_where_it_was_scrolled_to,
    ),
    (
        "a_skill_row_shows_the_total_after_everything_in_the_colour_of_the_change",
        &["skills.rows.the-number-shown-is-the-total-after-everything-and-the-colour-says-which-way"],
        a_skill_row_shows_the_total_after_everything_in_the_colour_of_the_change,
    ),
    (
        "a_weakened_character_shows_the_lower_number_still_drawn_plain",
        &["skills.rows.a-weakened-character-shows-the-lower-number-without-calling-it-lowered"],
        a_weakened_character_shows_the_lower_number_still_drawn_plain,
    ),
    (
        "the_contracts_tab_draws_a_row_per_contract_in_name_order",
        &["contracts.tab.the-tab-draws-a-row-per-contract-the-shard-sent-in-name-order"],
        the_contracts_tab_draws_a_row_per_contract_in_name_order,
    ),
    (
        "picking_a_contract_fills_the_pane_beside_the_list",
        &["contracts.detail.picking-a-contract-fills-the-pane-beside-the-list"],
        picking_a_contract_fills_the_pane_beside_the_list,
    ),
    (
        "the_abandon_button_gives_up_the_picked_contract_and_nothing_local_moves",
        &["contracts.abandon.the-button-gives-up-the-picked-contract-and-nothing-local-moves"],
        the_abandon_button_gives_up_the_picked_contract_and_nothing_local_moves,
    ),
    (
        "one_contract_at_a_time_is_added_changed_or_taken_away",
        &["contracts.list.one-contract-at-a-time-is-added-changed-or-taken-away"],
        one_contract_at_a_time_is_added_changed_or_taken_away,
    ),
    (
        "the_sort_buttons_choose_the_order_and_a_second_press_turns_it_round",
        &["contracts.sort.the-two-buttons-choose-the-order-and-pressing-one-twice-turns-it-round"],
        the_sort_buttons_choose_the_order_and_a_second_press_turns_it_round,
    ),
    (
        "the_born_line_is_the_day_and_time_the_character_was_made",
        &["character-sheet.born.the-line-is-the-day-and-time-the-character-was-made"],
        the_born_line_is_the_day_and_time_the_character_was_made,
    ),
    (
        "the_playtime_line_spells_out_only_the_terms_that_are_not_zero",
        &["character-sheet.played.the-line-spells-out-only-the-terms-that-are-not-zero"],
        the_playtime_line_spells_out_only_the_terms_that_are_not_zero,
    ),
    (
        "the_mastery_lines_name_the_weapon_group_rather_than_the_number",
        &["character-sheet.mastery.the-lines-name-the-weapon-group-rather-than-the-number-that-picks-it"],
        the_mastery_lines_name_the_weapon_group_rather_than_the_number,
    ),
    (
        "the_luminance_section_is_a_header_a_split_pair_and_an_unclamped_single",
        &["character-sheet.luminance.the-heading-is-always-there-and-a-rating-over-five-splits-in-two"],
        the_luminance_section_is_a_header_a_split_pair_and_an_unclamped_single,
    ),
    (
        "an_augmentation_row_says_time_once_and_times_more_than_once",
        &["character-sheet.augmentations.a-row-says-time-once-and-times-more-than-once"],
        an_augmentation_row_says_time_once_and_times_more_than_once,
    ),
    (
        "the_six_sections_of_the_sheet_are_in_the_order_the_composer_appends_them",
        &["character-sheet.sections.the-six-parts-are-drawn-in-the-order-the-sheet-builds-them"],
        the_six_sections_of_the_sheet_are_in_the_order_the_composer_appends_them,
    ),
    (
        "a_removed_death_count_disappears_from_the_character_sheet",
        &["character-sheet.deaths.a-count-the-shard-clears-leaves-the-sheet"],
        a_removed_death_count_disappears_from_the_character_sheet,
    ),
    (
        "the_skills_page_opens_asking_for_a_pick_and_a_press_picks_one",
        &["skills.selection.a-press-picks-a-skill-and-a-second-press-puts-the-footer-back"],
        the_skills_page_opens_asking_for_a_pick_and_a_press_picks_one,
    ),
    (
        "the_footer_under_a_picked_skill_is_that_characters_own_arithmetic",
        &["skills.footer.the-numbers-under-a-picked-skill-are-the-characters-own"],
        the_footer_under_a_picked_skill_is_that_characters_own_arithmetic,
    ),
    (
        "the_raise_button_spends_experience_or_credits_by_what_the_skill_is",
        &["skills.raise.the-button-spends-experience-on-a-trained-skill-and-credits-on-an-untrained-one"],
        the_raise_button_spends_experience_or_credits_by_what_the_skill_is,
    ),
    (
        "a_skill_values_colour_is_one_of_the_rows_own_three",
        &["skills.rows.the-colour-a-value-is-drawn-in-is-one-of-the-rows-own-three"],
        a_skill_values_colour_is_one_of_the_rows_own_three,
    ),
    (
        "both_character_pages_head_with_the_name_the_level_and_the_experience",
        &["character.header.the-character-page-heads-with-the-name-the-level-and-the-experience"],
        both_character_pages_head_with_the_name_the_level_and_the_experience,
    ),
    (
        "turning_a_school_off_hides_its_spells_and_sends_the_whole_list",
        &["spellbook.filter.turning-a-school-off-hides-its-spells-and-sends-the-whole-list"],
        turning_a_school_off_hides_its_spells_and_sends_the_whole_list,
    ),
    (
        "every_chess_message_reaches_a_receiver_or_a_sender",
        &["minigame.messages.every-message-the-game-speaks-reaches-a-receiver-or-a-sender"],
        every_chess_message_reaches_a_receiver_or_a_sender,
    ),
    (
        "using_a_board_raises_the_window_and_asks_for_a_seat",
        &["minigame.board.using-a-board-raises-the-window-and-asks-for-a-seat"],
        using_a_board_raises_the_window_and_asks_for_a_seat,
    ),
    (
        "the_game_lamp_is_lit_from_the_first_use_until_the_game_is_over",
        &["minigame.indicator.the-lamp-is-lit-from-the-first-use-until-the-game-is-over"],
        the_game_lamp_is_lit_from_the_first_use_until_the_game_is_over,
    ),
    (
        "the_seat_deals_the_board_and_the_start_names_whose_turn_it_is",
        &["minigame.board.the-shards-answer-deals-the-board-or-takes-the-window-away"],
        the_seat_deals_the_board_and_the_start_names_whose_turn_it_is,
    ),
    (
        "two_presses_move_a_piece_and_a_refused_move_never_leaves",
        &["minigame.board.two-presses-move-a-piece-and-a-move-the-rules-refuse-never-leaves"],
        two_presses_move_a_piece_and_a_refused_move_never_leaves,
    ),
    (
        "the_opponents_move_is_replayed_on_the_players_own_board",
        &["minigame.board.the-opponents-move-is-replayed-on-the-players-own-board"],
        the_opponents_move_is_replayed_on_the_players_own_board,
    ),
    (
        "the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window",
        &["minigame.window.the-offer-of-a-stalemate-and-the-end-of-a-game-are-said-in-the-window"],
        the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window,
    ),
    (
        "the_window_buttons_offer_a_draw_and_ask_before_resigning",
        &["minigame.window.the-buttons-offer-a-stalemate-and-ask-before-resigning"],
        the_window_buttons_offer_a_draw_and_ask_before_resigning,
    ),
    (
        "a_message_about_another_board_changes_nothing",
        &["minigame.board.a-message-about-another-board-changes-nothing"],
        a_message_about_another_board_changes_nothing,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

// =============================================================================================
// Support
//
// The client, the login and the pointer are `dereth_testkit`'s; what stays here is the walk down
// the shipped character page and the two glyph readers the snapshot has no answer for.
// =============================================================================================

/// The shell's element system and the gameplay screen behind it.
fn gameplay_screen(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the shell is up");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen is current");
    (ui, screen)
}

/// The decoded corpus of `session`.
fn corpus(session: &str) -> Corpus {
    Corpus::load(session)
        .unwrap_or_else(|e| panic!("the recording {session} does not parse: {e}"))
        .unwrap_or_else(|| {
            panic!(
                "the decoded corpus has no scenario {session}; it is generated from the \
                 committed recordings and a missing one is a broken checkout"
            )
        })
}

/// The ordered-event envelope, and the sub-type it carries.
const ORDERED_EVENT: u32 = 0xF7B0;
const PLAYER_DESCRIPTION: u32 = 0x0013;

/// Which blob of `session` is the shard's `0x0013` -- **found rather than pinned**, so a
/// re-promoted corpus moves the index instead of reddening an arithmetic nobody reads.
fn description_blob(session: &str) -> CorpusBlob {
    corpus(session)
        .blobs
        .into_iter()
        .find(|b| {
            b.dir == Direction::ServerToClient
                && b.opcode == ORDERED_EVENT
                && b.payload
                    .get(12..16)
                    .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
                    == Some(PLAYER_DESCRIPTION)
        })
        .unwrap_or_else(|| panic!("{session} never reached the shard's character description"))
}

/// A whole client in the world, with `session`'s own character behind the character page.
///
/// The recording is replayed **up to and including** its `0x0013` and stopped there: past it lies
/// the recorded log-off, which empties every panel below, and a count taken after that would be a
/// correct and meaningless zero.
fn a_recorded_character(session: &'static str) -> HeadlessClient {
    let end = description_blob(session).idx;
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    // **The identity has to exist before the description arrives.** The shard's `0x0013` unpacks
    // into the player weenie's own record, and the slice replayed below carries no moment at
    // which this client adopts one -- a replay that ran on would reach the recorded log-off first.
    // Without it every quality lands nowhere: the panels draw the shard's numbers (they come
    // through the HUD's own copy) and nothing addressed to the *player* -- a spell landing on an
    // attribute, say -- reaches anything at all.
    {
        let player = ObjectId(0x5000_0001);
        let w = c.world_mut();
        w.player = None;
        w.tables
            .weenies
            .insert(player, dereth_client_model::weenie::Weenie::new(player));
        assert!(w.set_player(player), "the identity is adopted once");
    }
    c.when(Inbound::from_corpus(session, 0..end + 1));
    c.tick(4);
    assert!(
        !c.view().expect_app().hud().skills.is_empty(),
        "{session}'s character description is this scenario's oracle and it carried no skill"
    );
    c
}

/// The character's qualities as the shard sent them, rebuilt from the recorded blob itself rather
/// than read back out of the client -- so the scenario's arithmetic and the panel's cannot share a
/// mistake.
fn recorded_qualities(session: &str) -> dereth_client_model::Qualities {
    let d = recorded_description(session);
    let mut q = dereth_client_model::Qualities::default();
    q.apply_ac_qualities(&d.qualities, LocalTime(0.0));
    q
}

/// The shard's own description of the character, as it recorded it.
fn recorded_description(session: &str) -> dereth_protocol::login::LoginPlayerDescription {
    let b = description_blob(session);
    dereth_protocol::read_body(&b.payload[16..])
        .expect("the recorded character description decodes")
}

/// One message, delivered the way the session layer delivers it.
fn deliver<M: dereth_protocol::Message>(c: &mut HeadlessClient, m: &M) {
    c.when(Inbound::message(m));
    c.tick(1);
}

/// The glyphs `h` composed, as text and as one `(font index, colour)` pair per UTF-16 unit.
///
/// **The gap `UiSnapshot` has.** A snapshot answers what an element says; what colour it says it
/// in is a property of the composed glyph, and font-aware text composition is how a
/// panel writes two colours into one element. Several claims here are exactly about that
/// second colour.
fn glyph_runs(app: &mut App, h: ElemHandle) -> (String, Vec<(u32, u32)>) {
    let (ui, _) = gameplay_screen(app);
    ui.text_element_mut(h).map_or_else(
        || (String::new(), Vec::new()),
        |t| {
            (
                t.glyphs.inq_text(false),
                t.glyphs.glyphs.iter().map(|g| (g.font, g.color)).collect(),
            )
        },
    )
}

/// The three colours an element's own state array declares, read back off the live element and
/// checked against the words a reader would use for them -- a symmetric read cannot see a wrong
/// constant.
fn state_colours(app: &mut App, h: ElemHandle) -> (u32, u32, u32) {
    let (ui, _) = gameplay_screen(app);
    let white = statmgmt::font_color_at(ui, h, 0).expect("the plain colour");
    let green = statmgmt::font_color_at(ui, h, 1).expect("the raised colour");
    let red = statmgmt::font_color_at(ui, h, 2).expect("the lowered colour");
    assert_eq!(white, 0xFFFF_FFFF, "unmodified: white");
    assert_eq!(green, 0xFF00_FF00, "raised: green");
    assert_eq!(red, 0xFFFF_0000, "lowered: red");
    (white, green, red)
}

/// Every glyph of `h` is one run: font index 0 throughout, and exactly `colour`.
fn one_run(g: &[(u32, u32)], text: &str, colour: u32) -> bool {
    g.len() == text.encode_utf16().count()
        && g.iter().all(|(f, _)| *f == 0)
        && g.iter().all(|(_, c)| *c == colour)
}

/// The character page of the toolbar's panel stack, raised the way the toolbar button raises it.
fn open_the_character_page(c: &mut HeadlessClient) {
    let app = c.app_mut();
    {
        let (ui, screen) = gameplay_screen(app);
        let panel_id = screen
            .panels
            .pages
            .iter()
            .find(|p| p.element == remaining::CHARACTER_PAGE)
            .map(|p| p.panel_id)
            .expect("the character page is one of the shipped registered pages");
        screen.recv_set_panel_visibility(ui, panel_id, true);
    }
    c.tick(3);
}

/// Raise the character page and select its skills tab.
///
/// **Selecting the tab rebuilds the list**, so a row handle taken before this call is a handle to
/// an element that no longer exists: every caller resolves its row afterwards.
fn show_the_skills_page(c: &mut HeadlessClient) {
    open_the_character_page(c);
    // **The tab is a toggle, not a destination.** Pressing it while its own sub-panel is already
    // up moves the page to the other one, which is how a second selection in one scenario ends up
    // clicking an attribute row and reading "select a skill" back.
    let already_up = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        ui.get_child_recursive(root, skills::PANEL)
            .is_some_and(|h| ui.is_visible(h))
    };
    if already_up {
        return;
    }
    let app = c.app_mut();
    let tab = {
        let (ui, screen) = gameplay_screen(app);
        let root = screen.root().expect("the gameplay screen has a root");
        ui.get_child_recursive(root, remaining::CHARACTER_PAGE)
            .and_then(|ph| {
                let n = ui.node(ph)?;
                let b = n.behaviour.as_ref()?;
                let p = (**b)
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()?;
                p.tab_to_page
                    .iter()
                    .find(|(_, pg)| **pg == skills::PANEL)
                    .map(|(t, _)| *t)
            })
            .and_then(|t| ui.get_child_recursive(root, t))
    };
    if let Some(th) = tab {
        let (ui, _) = gameplay_screen(app);
        ui.broadcast_element_message(th, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    }
    c.tick(3);
}

/// The live row `skill` is drawn in, on the page as it stands now.
fn skill_row(c: &HeadlessClient, skill: u32) -> ElemHandle {
    c.view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| r.skill == skill)
        .map(|r| r.element)
        .unwrap_or_else(|| panic!("skill {skill} has no row on the page"))
}

/// Press `skill`'s row **through the real pointer**: the page is raised, the row is scrolled into
/// view, and `Player::Click` goes to its centre through the client's own pump and input manager.
///
/// The row is addressed by **screen position** and not by element id on purpose: a list box builds
/// its rows from one template, so every row carries the same shipped id and `Target::Element`
/// would always answer the first one.
fn press_skill_row(c: &mut HeadlessClient, skill: u32) {
    show_the_skills_page(c);
    let h = skill_row(c, skill);
    {
        let app = c.app_mut();
        let mut panels = std::mem::take(&mut app.hud_mut().panels);
        {
            let (ui, _) = gameplay_screen(app);
            if let Some(w) = panels.skills.list.as_mut() {
                if let Some(i) = w.index_of(h) {
                    w.scroll_to_view(ui, i);
                }
            }
        }
        app.hud_mut().panels = panels;
    }
    c.tick(1);
    let (x, y) = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    c.when(Player::Click(Target::Point(ScreenPoint::new(x, y))));
}

/// One footer child, resolved the way the panel resolves it: the sub-panel's own state picks the
/// footer container, then the child id.
fn footer_child(app: &mut App, child: u32) -> ElemHandle {
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root().expect("the gameplay screen has a root");
    let page = ui
        .get_child_recursive(root, remaining::CHARACTER_PAGE)
        .expect("the character page is in the shipped layout");
    let panel = ui
        .get_child_recursive(page, skills::PANEL)
        .expect("the skills panel is in the shipped layout");
    let state = ui.node(panel).map_or(0, |n| n.state.0);
    let container = ui
        .get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))
        .expect("the footer container is in the shipped layout");
    ui.get_child_recursive(container, ElementId(child))
        .expect("the footer child is in the shipped layout")
}

/// The skills sub-panel's own state, which is what selects the footer.
fn skills_panel_state(app: &mut App) -> Option<u32> {
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root()?;
    let page = ui.get_child_recursive(root, remaining::CHARACTER_PAGE)?;
    let panel = ui.get_child_recursive(page, skills::PANEL)?;
    ui.node(panel).map(|n| n.state.0)
}

/// One additive enchantment of `family` keyed on `key`, as the shard sends one.
fn enchantment(
    spell_id: u16,
    family: u32,
    key: u32,
    delta: f32,
) -> dereth_protocol::types::qualities::Enchantment {
    use dereth_client_model::enchant::ench_type;
    dereth_protocol::types::qualities::Enchantment {
        id: u32::from(spell_id),
        category_word: 0,
        power_level: 1,
        start_time: 0.0,
        duration: 600.0,
        caster: ObjectId(0),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: dereth_protocol::types::qualities::StatMod {
            kind: family | ench_type::SINGLE_STAT | ench_type::ADDITIVE,
            key,
            value: delta,
        },
        spell_set_id: None,
    }
}

/// Land one enchantment on the client, through the shard's own message.
fn land(c: &mut HeadlessClient, e: dereth_protocol::types::qualities::Enchantment) {
    deliver(c, &dereth_protocol::qualities::MagicUpdateEnchantment(e));
}

/// The oracle's own copy of the registry takes the same enchantment.
fn oracle_takes(
    q: &mut dereth_client_model::Qualities,
    e: &dereth_protocol::types::qualities::Enchantment,
) {
    let ge = dereth_client_model::enchant::Enchantment::from_wire(e, LocalTime(0.0));
    assert!(
        q.enchantments.update_enchantment(ge),
        "the oracle's registry took the enchantment"
    );
}

/// The ladder both row updaters share: raised is 1, lowered is 2, unmodified is 0.
const fn ladder(raw: i64, effective: i64) -> u32 {
    if raw < effective {
        1
    } else if effective < raw {
        2
    } else {
        0
    }
}

/// One shipped table, decoded straight out of the dats the client was built over.
fn table<T: dereth_assets::Decode>(c: &HeadlessClient, id: u32) -> T {
    use dereth_primitives::AssetSource as _;
    let id = dereth_primitives::DataId(id);
    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let bytes = store.read(id).expect("the shipped table is in the dats");
    T::decode_payload(id, &bytes).expect("the shipped table decodes")
}

/// The recording every scenario in this file drives from: the one whose character description
/// carries a full skill list.
const SESSION: &str = "first-login-walk-jump";

// =============================================================================================
// skills.list.*
// =============================================================================================

/// **The gate.** After the shard's description has landed, the skills page holds one heading per
/// group and one row per skill, and every one of them was really created.
pub fn the_skills_page_builds_a_header_per_group_and_a_row_per_recorded_skill() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);

    let skills: Vec<u32> = c
        .view()
        .expect_app()
        .hud()
        .skills
        .iter()
        .map(|s| s.id)
        .collect();
    let panel = &c.view().expect_app().hud().panels.skills;
    assert!(panel.list.is_some(), "the shipped list box bound");

    // The four headings are pinned as a number as well as read through the table: a check that
    // reads a constant through the symbol it writes it through cannot see a wrong one.
    assert_eq!(
        skills::HEADER_TEMPLATES.len(),
        4,
        "the page makes four group headings"
    );
    let created = panel.rows_created() as usize;
    let failures = panel.create_failures();
    let rows = panel.rows.len();
    let shown: std::collections::BTreeSet<u32> = panel.shown().into_iter().collect();

    // And the rows are the recording's own skills, so the count above counts something real.
    let every_skill_has_a_row = skills.iter().all(|id| shown.contains(id));

    c.assert_behaviour(
        "skills.list.a-header-per-group-and-a-row-per-skill-the-shard-sent",
        move |_| {
            created > 0
                && created == 4 + rows
                && failures == 0
                && rows == skills.len()
                && every_skill_has_a_row
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_skills_page_builds_a_header_per_group_and_a_row_per_recorded_skill() {
    scenario("the_skills_page_builds_a_header_per_group_and_a_row_per_recorded_skill");
}

/// The other half, and the reason the page keeps a count of what it built at all: **a page that
/// built nothing reports no failure either**, so the failure count cannot be the guard.
///
/// Three states of one instrument, on one live list box: never rebuilt, rebuilt with no character
/// behind it, and rebuilt with the recording's own skills. The first two are indistinguishable by
/// failures alone, and the third is the known positive that says the instrument can move.
pub fn an_empty_skills_page_is_distinguishable_from_a_full_one() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let skills: Vec<dereth_ui_screens::view::SkillEntry> =
        c.view().expect_app().hud().skills.clone();

    let app = c.app_mut();
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root().expect("the gameplay screen has a root");
    let page = ui
        .get_child_recursive(root, remaining::CHARACTER_PAGE)
        .expect("the character page is in the shipped layout");

    // A second panel bound to the same live list box, so its counters start at zero without
    // disturbing the one the client drives.
    let mut fresh = skills::SkillsPanel::default();
    fresh.post_init(ui, page);
    let bound = fresh.list.is_some();
    let untouched = (fresh.rows_created(), fresh.create_failures());

    fresh.rebuild(ui, &[]);
    let with_nobody = (fresh.rows_created(), fresh.create_failures());

    fresh.rebuild(ui, &skills);
    let with_the_character = (
        fresh.rows_created() as usize,
        fresh.create_failures(),
        fresh.rows.len(),
    );

    c.assert_behaviour(
        "skills.list.a-page-that-built-nothing-is-told-apart-from-one-that-built-everything",
        move |_| {
            bound
                && untouched == (0, 0)
                // The ambiguity, measured: the unbuilt page and the built one agree on failures.
                && with_nobody == (0, 0)
                && with_the_character == (4 + skills.len(), 0, skills.len())
        },
    );
    c.shutdown();
}

#[test]
fn scenario_an_empty_skills_page_is_distinguishable_from_a_full_one() {
    scenario("an_empty_skills_page_is_distinguishable_from_a_full_one");
}

// =============================================================================================
// skills.footer.*
//
// The footer title's two claims -- the gain and the loss -- are one row: the footer carries the
// change, signed and coloured, and an implementation that got one arm right and the other wrong
// fails here.
// =============================================================================================

/// **The gate.** Select a skill: the footer is titled `Name: value`, in the plain colour, and the
/// change a spell made follows it in brackets -- green and signed `+` when it is a gain, red when
/// it is a loss.
pub fn the_footer_title_is_the_name_the_number_and_the_signed_change() {
    let q = recorded_qualities(SESSION);

    // ---- the gain the recorded character already carries -------------------------------------
    //
    // **Two clients, and not one with two selections in it.** Pressing the row a second time is
    // pressing the row that is *already selected*, which the list box reads as the player
    // un-selecting it -- the footer goes back to saying "select a skill" and the second arm would
    // be measuring that instead of the debuff. So each arm gets a fresh client.
    let (gain_reads, gain_font, gain_copy, want_gain, selected, metered, skill, name, white, red) = {
        let mut c = a_recorded_character(SESSION);
        let t: dereth_assets::tables::SkillTable = table(&c, 0x0E00_0004);
        let (skill, name) = c
            .view()
            .expect_app()
            .hud()
            .panels
            .skills
            .rows
            .iter()
            .find(|r| q.skill(r.skill).is_some_and(|s| s.sac >= 2))
            .map(|r| (r.skill, r.name.clone()))
            .expect("the recorded character has a trained skill with a row");

        let raw =
            i64::from(dereth_client_model::skills::inq_skill(&q, &t, skill, true).expect("raw"));
        let gained = i64::from(
            dereth_client_model::skills::inq_skill(&q, &t, skill, false).expect("enchanted"),
        );
        assert!(
            q.enchantments.vitae.is_none(),
            "the recorded character carries no vitae"
        );
        let up = gained - raw;
        assert!(
            up > 0,
            "the premise: this character's skill {skill} is already raised"
        );

        press_skill_row(&mut c, skill);
        let selected = c.view().expect_app().hud().panels.skills.selected_skill == skill;
        let metered = skills_panel_state(c.app_mut()) == Some(statmgmt::state::SELECTION_METER);

        let title = footer_child(c.app_mut(), statmgmt::child::TITLE);
        let (white, green, red) = state_colours(c.app_mut(), title);
        let (text, glyphs) = glyph_runs(c.app_mut(), title);
        let base = format!("{name}: {gained}");
        let suffix = format!(" (+{up})");
        let n = base.encode_utf16().count();
        let reads = text == format!("{base}{suffix}")
            && glyphs.len() == n + suffix.encode_utf16().count()
            && one_run(&glyphs[..n], &base, white)
            && one_run(&glyphs[n..], &suffix, green);
        let font = c
            .view()
            .expect_app()
            .hud()
            .panels
            .skills
            .footer_content
            .title_font;
        let copy = c
            .view()
            .expect_app()
            .hud()
            .panels
            .skills
            .footer_content
            .title
            .clone();
        let want = format!("{base}{suffix}");
        c.shutdown();
        (
            reads, font, copy, want, selected, metered, skill, name, white, red,
        )
    };

    // ---- the loss, which no recording carries, through the shard's own message ----------------
    let mut c = a_recorded_character(SESSION);
    let t: dereth_assets::tables::SkillTable = table(&c, 0x0E00_0004);
    let mut q2 = q.clone();
    let e = enchantment(
        0x1234,
        dereth_client_model::enchant::ench_type::SKILL,
        skill,
        -20.0,
    );
    oracle_takes(&mut q2, &e);
    let raw2 =
        i64::from(dereth_client_model::skills::inq_skill(&q2, &t, skill, true).expect("raw"));
    let lost = i64::from(
        dereth_client_model::skills::inq_skill(&q2, &t, skill, false).expect("enchanted"),
    );
    let down = lost - raw2;
    assert!(
        down < 0,
        "the premise: the loss outweighs what the character already had"
    );

    // **One spell landing is enough.** The panel does not wait for a skill update:
    // `Magic_UpdateEnchantment` raises `EnchantmentsChanged`, the frame's panels callback answers
    // it, and the answer is the rebuild. No second message is sent here, so the row can only move
    // because the enchantment itself rebuilt it.
    land(&mut c, e);
    c.tick(3);

    press_skill_row(&mut c, skill);
    let still_selected = c.view().expect_app().hud().panels.skills.selected_skill == skill;

    let title = footer_child(c.app_mut(), statmgmt::child::TITLE);
    let (text, glyphs) = glyph_runs(c.app_mut(), title);
    let base2 = format!("{name}: {lost}");
    let suffix2 = format!(" ({down})");
    let n2 = base2.encode_utf16().count();
    let loss_reads = text == format!("{base2}{suffix2}")
        && glyphs.len() == n2 + suffix2.encode_utf16().count()
        && one_run(&glyphs[..n2], &base2, white)
        && one_run(&glyphs[n2..], &suffix2, red);
    let loss_font = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .footer_content
        .title_font;

    c.assert_behaviour(
        "skills.footer.the-title-is-the-name-and-the-number-with-the-change-spelled-out-after-it",
        move |_| {
            selected
                && metered
                && gain_reads
                && gain_font == 1
                && gain_copy == want_gain
                && still_selected
                && loss_reads
                && loss_font == 2
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_footer_title_is_the_name_the_number_and_the_signed_change() {
    scenario("the_footer_title_is_the_name_the_number_and_the_signed_change");
}

/// **The words under the title, and the fact that they can be read.**
///
/// A line that holds the right text can still draw nothing, so text alone is not the observable:
/// the line has to compose glyphs in a real font at an opaque colour, and nothing the panel paints
/// after it may cover its box. The meter behind it is opaque and is painted **first**.
pub fn the_experience_to_raise_line_is_legible_and_uncovered() {
    let mut c = a_recorded_character(SESSION);
    let q = recorded_qualities(SESSION);
    let skill = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| q.skill(r.skill).is_some_and(|s| s.sac >= 2))
        .map(|r| r.skill)
        .expect("the recorded character has a trained skill with a row");

    press_skill_row(&mut c, skill);
    let selected = c.view().expect_app().hud().panels.skills.selected_skill == skill;
    let metered = skills_panel_state(c.app_mut()) == Some(statmgmt::state::SELECTION_METER);

    let label = footer_child(c.app_mut(), statmgmt::child::LINE_ONE_LABEL);
    let meter = footer_child(c.app_mut(), statmgmt::child::METER);
    let (text, _) = glyph_runs(c.app_mut(), label);

    let (ui, _) = gameplay_screen(c.app_mut());
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let has_trough = ui
        .node(meter)
        .and_then(|n| n.region.image.as_ref())
        .is_some();
    let at = |who: ElemHandle| back.calls.iter().position(|call| call.who == who);
    let meter_at = at(meter).expect("the meter draws");
    let label_at = at(label).expect("the label draws");
    let box_of = ui.screen_box(label);
    let covering = back.calls[label_at + 1..]
        .iter()
        .filter(|call| {
            (call.image.is_some() || call.fills.iter().any(|f| !f.is_invisible()))
                && call.clip.intersect(&box_of).is_valid()
                && call.screen.intersect(&box_of).is_valid()
        })
        .count();
    let visible = ui.is_visible(label);
    let boxed = box_of.x1 > box_of.x0 && box_of.y1 > box_of.y0;
    let placed = ui
        .text_element_mut(label)
        .map_or_else(Vec::new, |t| t.compose(box_of));
    let composed = !placed.is_empty()
        && placed.iter().all(|p| p.font.0 != 0)
        && placed.iter().all(|p| p.color >> 24 != 0);

    c.assert_behaviour(
        "skills.footer.the-experience-line-is-legible-and-nothing-is-painted-over-it",
        move |_| {
            selected
                && metered
                && text == "Experience To Raise:"
                && has_trough
                && meter_at < label_at
                && covering == 0
                && visible
                && boxed
                && composed
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_experience_to_raise_line_is_legible_and_uncovered() {
    scenario("the_experience_to_raise_line_is_legible_and_uncovered");
}

// =============================================================================================
// attributes.rows.*
// =============================================================================================

/// The row element carrying a stat's value: child `VALUE` of the row.
fn value_of(app: &mut App, row: ElemHandle) -> ElemHandle {
    let (ui, _) = gameplay_screen(app);
    ui.get_child_recursive(row, ElementId(skills::row::VALUE))
        .expect("the row's value element")
}

/// One attribute or vital row of the live panel.
fn attribute_row(c: &HeadlessClient, stat: u32, secondary: bool) -> ElemHandle {
    c.view()
        .expect_app()
        .hud()
        .panels
        .attributes
        .rows
        .iter()
        .find(|r| r.stat == stat && r.secondary == secondary)
        .map(|r| r.element)
        .unwrap_or_else(|| panic!("no row for stat {stat} secondary={secondary}"))
}

/// The three colours the row's value element declares. See [`state_colours`].
fn value_colours(c: &mut HeadlessClient, row: ElemHandle) -> (u32, u32, u32) {
    let value = value_of(c.app_mut(), row);
    state_colours(c.app_mut(), value)
}

/// What the row's value element says, and the runs it says it in.
fn value_runs(c: &mut HeadlessClient, row: ElemHandle) -> (String, Vec<(u32, u32)>) {
    let value = value_of(c.app_mut(), row);
    glyph_runs(c.app_mut(), value)
}

/// What one row draws and in what colour, plus the colour index the panel kept for itself.
fn row_reads(c: &mut HeadlessClient, row: ElemHandle, want: &str, colour: u32) -> bool {
    let (text, glyphs) = value_runs(c, row);
    let font = c
        .view()
        .expect_app()
        .hud()
        .panels
        .attributes
        .rows
        .iter()
        .find(|r| r.element == row)
        .map(|r| r.font);
    text == want && one_run(&glyphs, want, colour) && font.is_some()
}

/// **The gate.** Six attribute rows, one raised, one lowered, four untouched: each draws the
/// number the character actually has, in the colour that says which of the three it is.
pub fn a_buffed_attribute_row_is_green_a_debuffed_one_red_and_the_rest_white() {
    use dereth_client_model::enchant::ench_type;

    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let q = recorded_qualities(SESSION);

    // The premise: nothing on this character touches an attribute, so every row starts white.
    let plain = attributes::ATTRIBUTE_ROWS.iter().all(|(stat, _)| {
        dereth_client_model::attributes::inq_attribute(&q, *stat, true)
            == dereth_client_model::attributes::inq_attribute(&q, *stat, false)
    });
    let strength = attribute_row(&c, 1, false);
    let (white, green, red) = value_colours(&mut c, strength);
    let mut started_plain = true;
    for (stat, _) in attributes::ATTRIBUTE_ROWS {
        let eff =
            dereth_client_model::attributes::inq_attribute(&q, stat, false).expect("enchanted");
        let row = attribute_row(&c, stat, false);
        started_plain &= row_reads(&mut c, row, &eff.to_string(), white);
    }

    // One spell raises Strength, another lowers Endurance, through the shard's own message.
    let up = enchantment(0x1234, ench_type::ATTRIBUTE, 1, 20.0);
    let down = enchantment(0x1235, ench_type::ATTRIBUTE, 2, -20.0);
    let mut q2 = q.clone();
    oracle_takes(&mut q2, &up);
    oracle_takes(&mut q2, &down);
    land(&mut c, up);
    land(&mut c, down);
    c.tick(3);
    let mut all_six = true;
    let mut seen = [false; 3];
    for (stat, _) in attributes::ATTRIBUTE_ROWS {
        let raw = i64::from(
            dereth_client_model::attributes::inq_attribute(&q2, stat, true).expect("raw"),
        );
        let eff = i64::from(
            dereth_client_model::attributes::inq_attribute(&q2, stat, false).expect("enchanted"),
        );
        let font = ladder(raw, eff);
        seen[font as usize] = true;
        let colour = [white, green, red][font as usize];
        let row = attribute_row(&c, stat, false);
        all_six &= row_reads(&mut c, row, &eff.to_string(), colour);
    }

    c.assert_behaviour(
        "attributes.rows.a-buffed-value-is-green-a-debuffed-one-red-and-an-unmodified-one-white",
        move |_| plain && started_plain && all_six && seen == [true, true, true],
    );
    c.shutdown();
}

#[test]
fn scenario_a_buffed_attribute_row_is_green_a_debuffed_one_red_and_the_rest_white() {
    scenario("a_buffed_attribute_row_is_green_a_debuffed_one_red_and_the_rest_white");
}

/// A vital row is `current/maximum`, and the colour follows the **maximum**: a spell that raises
/// the ceiling colours the row even though the current value did not move.
pub fn a_vital_row_is_current_over_maximum_and_colours_by_the_maximum() {
    use dereth_client_model::attributes::{inq_attribute_2nd, vital};
    use dereth_client_model::enchant::ench_type;

    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let q = recorded_qualities(SESSION);
    let t: dereth_assets::tables::Attribute2ndTable = table(&c, 0x0E00_0003);
    let f: dereth_assets::tables::QualityFilter = table(&c, 0x0E01_0001);
    let f = Some(&f);
    assert!(
        q.enchantments.vitae.is_none(),
        "the recorded character carries no vitae"
    );

    let plain = attributes::SECONDARY_ROWS.iter().all(|(stat, _, _)| {
        let max = stat - 1;
        inq_attribute_2nd(&q, &t, max, true, f) == inq_attribute_2nd(&q, &t, max, false, f)
    });

    let up = enchantment(0x1236, ench_type::SECOND_ATT, vital::MAX_HEALTH, 50.0);
    let down = enchantment(0x1237, ench_type::SECOND_ATT, vital::MAX_STAMINA, -50.0);
    let mut q2 = q.clone();
    oracle_takes(&mut q2, &up);
    oracle_takes(&mut q2, &down);
    land(&mut c, up);
    land(&mut c, down);
    c.tick(3);

    let health = attribute_row(&c, 2, true);
    let (white, green, red) = value_colours(&mut c, health);
    let mut all_three = true;
    let mut seen = [false; 3];
    for (stat, _, _) in attributes::SECONDARY_ROWS {
        let max = stat - 1;
        let rawmax = i64::from(inq_attribute_2nd(&q2, &t, max, true, f).expect("raw maximum"));
        let effmax =
            i64::from(inq_attribute_2nd(&q2, &t, max, false, f).expect("enchanted maximum"));
        let cur = inq_attribute_2nd(&q2, &t, stat, false, f).expect("current");
        let font = ladder(rawmax, effmax);
        seen[font as usize] = true;
        let colour = [white, green, red][font as usize];
        let row = attribute_row(&c, stat, true);
        all_three &= row_reads(&mut c, row, &format!("{cur}/{effmax}"), colour);
    }

    c.assert_behaviour(
        "attributes.rows.a-vital-row-is-current-over-maximum-and-takes-its-colour-from-the-maximum",
        move |_| plain && all_three && seen == [true, true, true],
    );
    c.shutdown();
}

#[test]
fn scenario_a_vital_row_is_current_over_maximum_and_colours_by_the_maximum() {
    scenario("a_vital_row_is_current_over_maximum_and_colours_by_the_maximum");
}

/// A spell whose effect is a fraction is rounded to the nearest whole number **in the rows the
/// player reads**, and casting the same spell again over itself refreshes the row.
pub fn a_fractional_enchantment_rounds_in_the_rows_the_player_reads() {
    use dereth_client_model::enchant::ench_type;

    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let strength = attribute_row(&c, 1, false);
    let health = attribute_row(&c, 2, true);

    let (base_strength, base_health) = {
        let s = value_runs(&mut c, strength).0;
        let h = value_runs(&mut c, health).0;
        (s.parse::<u32>().expect("the strength row is a number"), h)
    };
    let (current, maximum) = base_health
        .split_once('/')
        .expect("the health row is current over maximum");
    let base_max: u32 = maximum.parse().expect("the maximum is a number");
    let current = current.to_owned();
    let (white, green, _red) = value_colours(&mut c, strength);

    // Either side of a half, and the half itself. The expected steps are literals: an oracle that
    // shared the client's own rounding could share its mistake too.
    let mut rounded = true;
    for (delta, step) in [(0.49_f32, 0_u32), (0.5, 1), (0.51, 1)] {
        land(&mut c, enchantment(0x1234, ench_type::ATTRIBUTE, 1, delta));
        land(&mut c, enchantment(0x1235, ench_type::SECOND_ATT, 1, delta));
        c.tick(3);
        let colour = if step == 0 { white } else { green };
        rounded &= row_reads(
            &mut c,
            strength,
            &(base_strength + step).to_string(),
            colour,
        );
        rounded &= row_reads(
            &mut c,
            health,
            &format!("{current}/{}", base_max + step),
            colour,
        );
    }

    c.assert_behaviour(
        "attributes.rows.a-fractional-enchantment-rounds-the-way-the-panel-draws-it",
        move |_| rounded,
    );
    c.shutdown();
}

#[test]
fn scenario_a_fractional_enchantment_rounds_in_the_rows_the_player_reads() {
    scenario("a_fractional_enchantment_rounds_in_the_rows_the_player_reads");
}

// =============================================================================================
// advancement.cost.*
//
// A `+10` button must light the same under an optimised and an unoptimised build: every step of
// the arithmetic between the shipped table and the button is a plain integer comparison, so the
// two profiles cannot differ.
// =============================================================================================

/// The character these scenarios cost against: an attribute at rank 30, with what has been spent
/// on it and the experience still unassigned.
const RANK: u32 = 30;
const SPENT: u32 = 32_676;
const AVAILABLE: u64 = 20_887_465;

/// **The table, and the two ways ten points are costed.** An attribute's ten points cost the
/// distance from where the character is to ten ranks on; a skill's cost the distance between the
/// same two entries of its own table, and **not** the distance to the table's last entry.
pub fn ten_points_cost_the_distance_between_two_entries_of_the_shipped_table() {
    use dereth_client_model::advancement as adv;
    use dereth_client_model::qualities::Qualities;
    use dereth_client_model::skills::Sac;
    use dereth_protocol::types::qualities::Skill;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    let t: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);

    let attribute = adv::max_attribute_level(&t) == 190
        && adv::attribute_cost_to_raise(&t, RANK, SPENT, false) == 2_456
        && adv::attribute_cost_to_raise_10(&t, RANK, SPENT, false) == 31_202;

    // The sibling arithmetic: any skill id, because what is read of the record is its class, its
    // level and what has been sunk into it.
    let id = 1;
    let mut skill = true;
    for sac in [Sac::Trained, Sac::Specialized] {
        let column = if sac == Sac::Trained {
            &t.trained_xp
        } else {
            &t.specialized_xp
        };
        let mut q = Qualities::new();
        q.set_skill(
            id,
            Skill {
                level_from_pp: 30,
                format_version: 1,
                sac: sac as u32,
                pp: column[30],
                init_level: 0,
                resistance_of_last_check: 0,
                last_used_time: 0.0,
            },
        );
        let want = column[40] - column[30];
        // The premise: ten ranks on and the table's end are different numbers, so a cost that
        // reached for the cap would be visible here.
        skill &= want != column[column.len() - 1] - column[30]
            && adv::skill_cost_to_raise_10(&q, &t, id) == want;
    }

    c.assert_behaviour(
        "advancement.cost.the-ten-point-cost-comes-off-the-shipped-experience-table",
        move |_| attribute && skill,
    );
    c.shutdown();
}

#[test]
fn scenario_ten_points_cost_the_distance_between_two_entries_of_the_shipped_table() {
    scenario("ten_points_cost_the_distance_between_two_entries_of_the_shipped_table");
}

/// **At rank 30.** The `+10` and the `+1` are both affordable against the experience that
/// character had not spent, both from the literal cost and from the cost the table really answers.
pub fn the_plus_ten_button_lights_when_the_unassigned_experience_covers_it() {
    use dereth_client_model::advancement as adv;
    use dereth_ui_screens::panels::statmgmt::{button_state, Footer};

    let mut c = HeadlessClient::new(ClientSpec::retail());
    let t: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);
    let cost_10 = u64::from(adv::attribute_cost_to_raise_10(&t, RANK, SPENT, false));
    let cost_1 = u64::from(adv::attribute_cost_to_raise(&t, RANK, SPENT, false));

    let lit = Footer::enable_for(31_202, AVAILABLE) == button_state::ENABLED
        && Footer::enable_for(2_456, AVAILABLE) == button_state::ENABLED
        && Footer::enable_for(cost_10, AVAILABLE) == button_state::ENABLED
        && Footer::enable_for(cost_1, AVAILABLE) == button_state::ENABLED;
    // The premise: the same test can say no, so a button that always lit would not pass.
    let dark = Footer::enable_for(cost_10, cost_10 - 1) != button_state::ENABLED;

    c.assert_behaviour(
        "advancement.cost.the-plus-ten-button-is-affordable-when-the-unassigned-experience-covers-it",
        move |_| lit && dark,
    );
    c.shutdown();
}

#[test]
fn scenario_the_plus_ten_button_lights_when_the_unassigned_experience_covers_it() {
    scenario("the_plus_ten_button_lights_when_the_unassigned_experience_covers_it");
}

// =============================================================================================
// examine.*
//
// The identify window's panes.
//
// The shard here is `replay::Peer` and not a replayed login: what these claims need from a
// recording is two blobs, the shard's own create for the object and the shard's own assessment of
// it, and replaying the several thousand datagrams between them would be replaying the log-off at
// the far end as well. The bytes are the recording's either way; `Peer` is what re-stamps them for
// a session whose ordered counter starts at zero.
// =============================================================================================

/// The recordings the assessment scenarios take their objects and answers out of.
const EXAMINE_SESSION: &str = "long-solo-play";
const ARMOUR_SESSION: &str = "early-inventory-and-casting";

/// The character these scenarios look out of. Nothing recorded carries it: the thing the pane is
/// about is the recording's, and whoever is looking at it only has to exist.
const LOOKER: ObjectId = ObjectId(0x5000_0001);

const ITEM_CREATE_OBJECT: u32 = 0xF745;
const ITEM_SET_APPRAISE_INFO: u32 = 0x00C9;

fn dword(s: &[u8]) -> u32 {
    u32::from_le_bytes([s[0], s[1], s[2], s[3]])
}

/// The shard's own assessment, decoded out of a recorded blob.
fn appraisal_in(b: &CorpusBlob) -> Option<dereth_protocol::objects::ItemSetAppraiseInfo> {
    use dereth_protocol::Message as _;
    let mut r = dereth_protocol::archive::Reader::new(b.payload.get(16..)?);
    dereth_protocol::objects::ItemSetAppraiseInfo::read(&mut r).ok()
}

/// The two blobs an assessment scenario needs: the create that made `object`, and the assessment
/// the shard answered about it. **Found rather than pinned**, so a re-promoted corpus moves them
/// instead of reddening an index nobody reads.
fn recorded_object(session: &str, object: ObjectId) -> (Vec<u8>, Vec<u8>, AppraisalProfile) {
    let blobs = corpus(session).blobs;
    let create = blobs
        .iter()
        .find(|b| {
            b.dir == Direction::ServerToClient
                && b.opcode == ITEM_CREATE_OBJECT
                && b.payload.get(4..8).map(dword) == Some(object.0)
        })
        .map(|b| b.payload.clone())
        .unwrap_or_else(|| panic!("{session} never creates {object:?}"));
    let (blob, profile) = blobs
        .iter()
        .filter(|b| {
            b.dir == Direction::ServerToClient
                && b.opcode == ORDERED_EVENT
                && b.payload.get(12..16).map(dword) == Some(ITEM_SET_APPRAISE_INFO)
        })
        .find_map(|b| {
            appraisal_in(b)
                .filter(|m| m.object == object)
                .map(|m| (b.payload.clone(), m.profile))
        })
        .unwrap_or_else(|| panic!("{session} carries no assessment of {object:?}"));
    (create, blob, profile)
}

/// A client with a character to look out of, and a shard to answer it.
fn a_client_and_a_shard() -> (HeadlessClient, dereth_testkit::Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut peer = dereth_testkit::Peer::attach(&mut c, LOOKER);
    // **The character is created by the shard, not written into the tables.** An ordered game
    // event is addressed to an object, and the client's ordered queue only has somewhere to put
    // one for an object its own object stream has been told about.
    let mut p = dereth_protocol::objects::ObjectCreatePayload {
        id: LOOKER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut c,
        dereth_testkit::replay::OBJECT_QUEUE,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("the create encodes"),
    );
    c.tick(1);
    c.world_mut().player = Some(LOOKER);
    c.world_mut()
        .weenie_mut(LOOKER)
        .expect("the shard created the character")
        .pwd
        .name = "the looker".to_owned();
    (c, peer)
}

/// Assess `object` **the way the player does**: the thing is under the pointer, and the shipped
/// identify button on the toolbar is really pressed.
///
/// It has to be the button. The panel's own examine is called by the *screen*, from the arm that
/// handles that button -- not by the request the button emits -- so a scenario that queued
/// `UiRequest::Examine` straight into the interaction layer would set the world asking and leave
/// the pane refusing every answer -- the middle link these scenarios exist to cover.
fn examine(c: &mut HeadlessClient, object: ObjectId) {
    {
        // What a click on the thing in the viewport calls.
        let mut sink = dereth_client_model::RecordingSink::default();
        c.world_mut()
            .set_selected_object(Some(object), false, &mut sink);
    }
    c.tick(1);
    c.when(Player::click(
        dereth_ui_screens::toolbar::target_mode::EXAMINE_BUTTON,
    ));
}

/// Every glyph the item pane composed, as the player reads it.
fn description(c: &mut HeadlessClient) -> String {
    c.ui_snapshot()
        .text_of(examination::ITEM_DISPLAY_TEXT)
        .to_owned()
}

/// What the panel is holding for the item pane, which is the same words in the client's own model.
fn item_text(c: &mut HeadlessClient) -> String {
    let (_, screen) = gameplay_screen(c.app_mut());
    screen
        .examination
        .item_text
        .clone()
        .expect("an item pane is up")
}

/// The recorded object, created and assessed, with the shard's own bytes for both.
fn assessing_the_recorded(
    session: &str,
    object: ObjectId,
) -> (HeadlessClient, dereth_testkit::Peer, AppraisalProfile) {
    let (create, blob, profile) = recorded_object(session, object);
    let (mut c, mut peer) = a_client_and_a_shard();
    peer.send(&mut c, dereth_testkit::replay::OBJECT_QUEUE, create);
    c.tick(1);
    assert!(
        c.view().world().weenie(object).is_some(),
        "the recorded create is what puts {object:?} in this client's world"
    );
    examine(&mut c, object);
    peer.replay_blob(&mut c, blob);
    c.tick(2);
    (c, peer, profile)
}

/// One assessment the scenario built, for a shape no recording carries.
fn assess(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    object: ObjectId,
    p: &AppraisalProfile,
) {
    examine(c, object);
    peer.event(
        c,
        &dereth_protocol::objects::ItemSetAppraiseInfo {
            object,
            profile: p.clone(),
        },
    );
    c.tick(2);
}

// ---------------------------------------------------------------------------------------------
// examine.spells.*
// ---------------------------------------------------------------------------------------------

/// The one recorded assessment in the whole corpus whose answer carries a spell book, and the two
/// strings the shipped spell table answers for its single spell.
const FOUNTAIN: ObjectId = ObjectId(0x7DA5_5046);
const REVITALIZE: u32 = 1183;
const REVITALIZE_NAME: &str = "Revitalize Other I";
const REVITALIZE_DESC: &str = "Restores 15-35 points of the target's Stamina.";

/// An assessment shaped the way the decoder produces one, with whatever spells, whole numbers and
/// fractions the scenario wants.
fn spell_profile(
    success: u32,
    ids: Option<Vec<u32>>,
    ints: &[(u32, i32)],
    floats: &[(u32, f64)],
) -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = AppraisalProfile {
        success_flag: success,
        ..AppraisalProfile::default()
    };
    if let Some(ids) = ids {
        p.flags |= flags::SPELL_BOOK;
        p.spell_book = Some(ids);
    }
    if !ints.is_empty() {
        p.flags |= flags::INT;
        p.tables.ints = Some(dereth_protocol::archive::PackedHash {
            table_size: 16,
            entries: ints.to_vec(),
        });
    }
    if !floats.is_empty() {
        p.flags |= flags::FLOAT;
        p.tables.floats = Some(dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries: floats.to_vec(),
        });
    }
    p
}

/// **The gate.** The recorded fountain's own assessment: the summary line names its spell, and the
/// paragraph under it describes that spell out of the shipped table.
pub fn an_assessed_item_names_its_spells_and_describes_them() {
    let (mut c, _peer, profile) = assessing_the_recorded(EXAMINE_SESSION, FOUNTAIN);
    // The premise, off the recorded answer itself: this is the corpus's one spell book.
    assert_eq!(
        profile.spell_book.as_deref(),
        Some(&[REVITALIZE][..]),
        "the recorded assessment of the fountain carries one spell"
    );

    let text = description(&mut c);
    let short = text.find("Spells: ");
    let long = text.find("Spell Descriptions:");
    let names = text.contains(&format!("Spells: {REVITALIZE_NAME}"));
    let describes = text.contains(&format!(
        "Spell Descriptions:\n~ {REVITALIZE_NAME}: {REVITALIZE_DESC}"
    ));
    // Each of the two starts a paragraph of its own.
    let paragraphs = text.contains(&format!("\n\nSpells: {REVITALIZE_NAME}"))
        && text.contains("\n\nSpell Descriptions:");
    // The answer carries none of the mana keys and no fractions at all, so those lines are absent
    // -- which is as much the client's doing as the two above.
    let quiet = ["Spellcraft:", "Mana:", "Mana Cost:", "Enchantments:"]
        .iter()
        .all(|w| !text.contains(w));

    c.assert_behaviour(
        "examine.spells.an-item-with-spells-on-it-names-them-and-describes-them",
        move |_| names && describes && paragraphs && quiet && short.is_some() && short < long,
    );
    c.shutdown();
}

#[test]
fn scenario_an_assessed_item_names_its_spells_and_describes_them() {
    scenario("an_assessed_item_names_its_spells_and_describes_them");
}

/// A spell somebody cast on the thing is listed apart from the spells it was made with: under its
/// own heading, and out of the summary line altogether.
///
/// **No recording carries one.** Not one of the corpus's assessments names a spell on that side of
/// the split, so the answer below is built; the spell, its name and its description are still the
/// shipped table's, and everything from the datagram on is the client's own.
pub fn an_enchantment_is_listed_apart_from_the_items_own_spells() {
    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, FOUNTAIN);
    assess(
        &mut c,
        &mut peer,
        FOUNTAIN,
        &spell_profile(1, Some(vec![0x8000_0000 | REVITALIZE]), &[(0x13, 200)], &[]),
    );

    let text = description(&mut c);
    // The blank line under this heading is the client's and the other heading has none: the two
    // really do disagree, and it is the first thing a tidy-up would lose.
    let under_its_own_heading = text.contains(&format!(
        "Enchantments:\n\n~ {REVITALIZE_NAME}: {REVITALIZE_DESC}"
    ));
    let not_in_the_summary = !text.contains("Spells: ");
    let not_in_the_other_paragraph = !text.contains("Spell Descriptions:");

    c.assert_behaviour(
        "examine.spells.an-enchantment-is-listed-apart-from-the-spells-the-item-was-made-with",
        move |_| under_its_own_heading && not_in_the_summary && not_in_the_other_paragraph,
    );
    c.shutdown();
}

#[test]
fn scenario_an_enchantment_is_listed_apart_from_the_items_own_spells() {
    scenario("an_enchantment_is_listed_apart_from_the_items_own_spells");
}

/// The three magic lines, in the client's own order, and which of the two prices wins.
///
/// A thing that spends mana over time says so per so many seconds and the flat price is never
/// reached; one with no rate gives the flat price and the note that a skill reduces it.
pub fn the_magic_lines_come_in_order_and_a_rate_beats_a_flat_cost() {
    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, FOUNTAIN);

    assess(
        &mut c,
        &mut peer,
        FOUNTAIN,
        &spell_profile(
            1,
            Some(vec![REVITALIZE]),
            &[
                (0x13, 200),
                (0x6A, 250),
                (0x6B, 700),
                (0x6C, 800),
                (0x75, 5),
            ],
            &[(5, -0.05)],
        ),
    );
    let text = description(&mut c);
    let at = |s: &str| text.find(s);
    let by_the_rate = [
        "Spellcraft: 250.",
        "Mana: 700 / 800.",
        "Mana Cost: 1 point per 20 seconds.",
    ]
    .iter()
    .all(|l| text.contains(l))
        && !text.contains("Mana Cost: 5.")
        && !text.contains("Mana Conversion")
        && at("Spellcraft: 250.").is_some()
        && at("Spellcraft: 250.") < at("Mana: 700 / 800.")
        && at("Mana: 700 / 800.") < at("Mana Cost: 1 point per 20 seconds.")
        && at("Mana Cost: 1 point per 20 seconds.") < at("Spell Descriptions:");

    assess(
        &mut c,
        &mut peer,
        FOUNTAIN,
        &spell_profile(1, Some(vec![REVITALIZE]), &[(0x13, 200), (0x75, 5)], &[]),
    );
    let text = description(&mut c);
    let flat = text.contains("Mana Cost: 5.\n(Can be reduced by the Mana Conversion skill)");

    c.assert_behaviour(
        "examine.spells.the-magic-lines-come-in-the-clients-own-order-and-a-rate-beats-a-flat-cost",
        move |_| by_the_rate && flat,
    );
    c.shutdown();
}

#[test]
fn scenario_the_magic_lines_come_in_order_and_a_rate_beats_a_flat_cost() {
    scenario("the_magic_lines_come_in_order_and_a_rate_beats_a_flat_cost");
}

/// An assessment the character was not up to says the spells are unknown -- **twice**, because
/// both blocks say it, which is the client's own and not a transcription to be tidied -- and a
/// thing with no spells at all draws neither heading, whether the assessment succeeded or not.
pub fn an_unsuccessful_assessment_says_unknown_and_a_plain_item_says_nothing() {
    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, FOUNTAIN);

    assess(
        &mut c,
        &mut peer,
        FOUNTAIN,
        &spell_profile(0, Some(vec![REVITALIZE]), &[], &[]),
    );
    let text = description(&mut c);
    let unknown = text.matches("Spells: unknown.").count() == 2 && !text.contains(REVITALIZE_NAME);

    let mut silent = true;
    for success in [0_u32, 1] {
        assess(
            &mut c,
            &mut peer,
            FOUNTAIN,
            &spell_profile(success, None, &[(0x13, 200)], &[]),
        );
        let text = description(&mut c);
        silent &= ["Spells:", "Spell Descriptions:", "Enchantments:"]
            .iter()
            .all(|w| !text.contains(w));
    }

    c.assert_behaviour(
        "examine.spells.an-unsuccessful-assessment-says-so-and-a-plain-item-says-nothing",
        move |_| unknown && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_an_unsuccessful_assessment_says_unknown_and_a_plain_item_says_nothing() {
    scenario("an_unsuccessful_assessment_says_unknown_and_a_plain_item_says_nothing");
}

// ---------------------------------------------------------------------------------------------
// examine.lifespan.*
// ---------------------------------------------------------------------------------------------

/// The recorded potion the lifespan scenario hangs its built answers on.
const POTION: ObjectId = ObjectId(0x8000_09B5);
const LIFESPAN: u32 = 0x10B;
const CREATION_TIMESTAMP: u32 = 0x62;
const REMAINING_LIFESPAN: u32 = 0x10C;

/// An assessment carrying only whole numbers.
fn int_profile(entries: &[(u32, i32)]) -> AppraisalProfile {
    AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::INT,
        success_flag: 1,
        tables: dereth_protocol::types::PropertyTables {
            ints: Some(dereth_protocol::archive::PackedHash {
                table_size: 8,
                entries: entries.to_vec(),
            }),
            ..dereth_protocol::types::PropertyTables::default()
        },
        ..AppraisalProfile::default()
    }
}

/// The line that says when the thing runs out, if there is one.
fn expiry(text: &str) -> Option<&str> {
    text.lines().find(|line| {
        line.starts_with("This item expires in ")
            || *line == "This item is in the act of disintegrating."
    })
}

/// **The gate.** How long is left is spelled out in years, days, hours, minutes and seconds; a
/// thing already past its time says it is disintegrating; and an answer missing any one of the
/// three numbers the line is built from draws no line at all.
pub fn an_item_that_expires_says_when_and_needs_all_three_numbers() {
    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, POTION);

    let mut spelled_out = true;
    for (lifespan, creation, remaining, want) in [
        (0, 0, 0, "This item expires in 0 seconds."),
        (-1, -1, 60, "This item expires in 60 seconds."),
        (1, 2, 61, "This item expires in 1 minutes, 1 seconds."),
        (1, 2, 3_600, "This item expires in 60 minutes, 0 seconds."),
        (1, 2, 86_400, "This item expires in 24 hours, 0 seconds."),
        (
            1,
            2,
            31_536_000,
            "This item expires in 365 days, 0 seconds.",
        ),
        (
            1,
            2,
            31_626_061,
            "This item expires in 1 years, 1 days, 1 hours, 1 minutes, 1 seconds.",
        ),
        (1, 2, -1, "This item is in the act of disintegrating."),
    ] {
        assess(
            &mut c,
            &mut peer,
            POTION,
            &int_profile(&[
                (LIFESPAN, lifespan),
                (CREATION_TIMESTAMP, creation),
                (REMAINING_LIFESPAN, remaining),
            ]),
        );
        let text = description(&mut c);
        spelled_out &= expiry(&text) == Some(want);
    }

    // The first two numbers are gates and not sources: a zero in either still draws the line, and
    // a missing one of the three draws nothing at all.
    let all = [
        (LIFESPAN, 99),
        (CREATION_TIMESTAMP, 100),
        (REMAINING_LIFESPAN, 101),
    ];
    let mut silent = true;
    for missing in [LIFESPAN, CREATION_TIMESTAMP, REMAINING_LIFESPAN] {
        let entries: Vec<(u32, i32)> = all
            .iter()
            .copied()
            .filter(|(key, _)| *key != missing)
            .collect();
        assess(&mut c, &mut peer, POTION, &int_profile(&entries));
        let text = description(&mut c);
        silent &= expiry(&text).is_none();
    }

    c.assert_behaviour(
        "examine.lifespan.an-item-that-expires-says-when-and-needs-all-three-of-its-numbers",
        move |_| spelled_out && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_an_item_that_expires_says_when_and_needs_all_three_numbers() {
    scenario("an_item_that_expires_says_when_and_needs_all_three_numbers");
}

// ---------------------------------------------------------------------------------------------
// examine.creature.* / examine.armour.* / examine.weapon.* / examine.inscription.*
// ---------------------------------------------------------------------------------------------

/// The recorded golem, the recorded gauntlets, and the recorded thing the built answers hang on.
const GOLEM: ObjectId = ObjectId(0x8000_09D9);
const GAUNTLETS: ObjectId = ObjectId(0x8000_0674);
const SUBJECT: ObjectId = ObjectId(0x8000_0997);

/// The nine rows of the creature pane, as the panel holds them.
fn creature_rows(c: &mut HeadlessClient) -> Vec<(String, String)> {
    let (_, screen) = gameplay_screen(c.app_mut());
    screen.examination.creature_row_text.clone()
}

/// **The gate.** The recorded golem: the pane names what kind of creature it is, its level, its six
/// attributes and its three vitals -- with the percentage on health alone.
pub fn the_creature_pane_names_the_kind_the_level_and_the_nine_rows() {
    let (mut c, mut peer, profile) = assessing_the_recorded(EXAMINE_SESSION, GOLEM);

    // The premise, off the recorded answer: a creature, at part health.
    let body = profile
        .creature_profile
        .expect("the recorded answer carries a creature");
    assert_eq!(
        (body.health, body.max_health),
        (12, 31),
        "the recorded golem's health"
    );

    let (title, kind, level, drawn, active) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        let p = &screen.examination;
        (
            p.title_text.clone(),
            p.creature_name_text.clone(),
            p.level_text.clone(),
            p.creature_rows_drawn,
            p.active,
        )
    };
    let want: Vec<(String, String)> = [
        ("Strength", "1"),
        ("Endurance", "1"),
        ("Coordination", "1"),
        ("Quickness", "1"),
        ("Focus", "1"),
        ("Self", "1"),
        ("Health", "12/31 (39 %)"),
        ("Stamina", "51/51"),
        ("Mana", "1/1"),
    ]
    .iter()
    .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
    .collect();
    let recorded_reads = title.as_deref() == Some("Sparring Golem")
        && kind.as_deref() == Some("Golem")
        && level.as_deref() == Some("1")
        && drawn == 9
        && active == Some(examination::ExamineSubUi::Creature)
        && creature_rows(&mut c) == want;

    // The same body at full health -- the percentage is on health and on neither of the others.
    let mut full = profile.clone();
    if let Some(b) = full.creature_profile.as_mut() {
        b.health = b.max_health;
    }
    assess(&mut c, &mut peer, GOLEM, &full);
    let rows = creature_rows(&mut c);
    let at_full = rows[6] == ("Health".to_owned(), "31/31 (100 %)".to_owned())
        && rows[7] == ("Stamina".to_owned(), "51/51".to_owned())
        && rows[8] == ("Mana".to_owned(), "1/1".to_owned());

    c.assert_behaviour(
        "examine.creature.the-pane-names-the-kind-the-level-six-attributes-and-three-vitals",
        move |_| recorded_reads && at_full,
    );
    c.shutdown();
}

#[test]
fn scenario_the_creature_pane_names_the_kind_the_level_and_the_nine_rows() {
    scenario("the_creature_pane_names_the_kind_the_level_and_the_nine_rows");
}

/// The recorded gauntlets: the armour level, then the eight kinds of harm as a word and a number
/// each, in the order the pane draws them rather than the order the answer packs them in.
pub fn the_armour_pane_gives_its_level_and_eight_resistances() {
    let (mut c, _peer, profile) = assessing_the_recorded(ARMOUR_SESSION, GAUNTLETS);
    let a = profile
        .armor_profile
        .expect("the recorded answer carries armour");
    assert!(
        (a.mod_vs_pierce - 0.8).abs() < 1e-6,
        "the recorded piercing modifier"
    );
    assert!(
        (a.mod_vs_acid - 0.3).abs() < 1e-6,
        "the recorded acid modifier"
    );

    let text = item_text(&mut c);
    let want = "Value: 0\n\
                Burden: 270\n\
                \n\
                \n\
                Armor Level: 20\n\
                Slashing: Average  (20)\n\
                Piercing: Below Average  (16)\n\
                Bludgeoning: Average  (20)\n\
                Fire: Below Average  (10)\n\
                Cold: Below Average  (10)\n\
                Acid: Poor  (6)\n\
                Electric: Below Average  (12)\n\
                Nether: Average  (20)\n";
    let reads = text == want;

    c.assert_behaviour(
        "examine.armour.the-pane-gives-its-level-and-eight-resistances-in-the-order-it-draws-them",
        move |_| reads,
    );
    c.shutdown();
}

#[test]
fn scenario_the_armour_pane_gives_its_level_and_eight_resistances() {
    scenario("the_armour_pane_gives_its_level_and_eight_resistances");
}

/// A bow as a retail screenshot shows it. **No recorded assessment carries a weapon at all**, so
/// this is built from that screenshot; the thing it is about is the recording's, and everything
/// from the datagram onwards is the client's.
fn a_bow() -> dereth_protocol::types::appraisal::WeaponProfile {
    dereth_protocol::types::appraisal::WeaponProfile {
        damage_type: 2,
        weapon_time: 40,
        weapon_skill: 47,
        weapon_damage: 0,
        damage_variance: 0.0,
        damage_mod: 1.0,
        weapon_length: 0.0,
        max_velocity: 22.5,
        weapon_offense: 1.0,
        max_velocity_estimated: 0,
    }
}

/// The weapon pane: the skill, the damage, the speed as a word and a number, the reach and what it
/// shoots -- and a modifier of exactly none prints as plus nothing rather than as a blank.
pub fn the_weapon_pane_gives_the_skill_the_speed_the_range_and_the_ammunition() {
    let (mut c, mut peer, recorded) = assessing_the_recorded(EXAMINE_SESSION, SUBJECT);
    assert!(
        recorded.weapon_profile.is_none(),
        "the premise: no recorded assessment carries a weapon, which is why this one is built"
    );
    {
        // The two fields of the live object the block branches on: a launcher, and arrows.
        let w = c
            .world_mut()
            .weenie_mut(SUBJECT)
            .expect("the recorded create made it");
        w.pwd.valid_locations = Some(0x0040_0000);
        w.pwd.ammo_type = Some(1);
    }

    let mut profile = AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::INT
            | dereth_protocol::types::appraisal::flags::STRING
            | dereth_protocol::types::appraisal::flags::WEAPON_PROFILE,
        success_flag: 1,
        weapon_profile: Some(a_bow()),
        ..AppraisalProfile::default()
    };
    profile.tables.ints = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(0x13, 25), (5, 400), (0x161, 8)],
    });
    profile.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![
            (
                0x0E,
                "Use Oil of Rendering on this weapon to create an Academy Shortbow.".to_owned(),
            ),
            (
                0x10,
                "A shortbow used by the students of the Academy.".to_owned(),
            ),
        ],
    });
    assess(&mut c, &mut peer, SUBJECT, &profile);
    let text = item_text(&mut c);
    // The whole pane, and not six lines of it. Two of the blank lines are the client's own -- two
    // blocks open a paragraph before they have looked at anything -- and the line about what it
    // holds is read off the **object** and not the assessment, which is the recording's doing:
    // the thing this answer is carried on is a container the recording created.
    let reads = text
        == [
            "Value: 25",
            "Burden: 400",
            "",
            "Skill: Missile Weapons (Bow)",
            "Damage Bonus: 0",
            "Damage Modifier: +0%.",
            "Speed: Average (40)",
            "Range: 55 yds.",
            "Uses arrows as ammunition.",
            "",
            "",
            "Use Oil of Rendering on this weapon to create an Academy Shortbow.",
            "",
            "Can hold up to 24 items.",
            "",
            "A shortbow used by the students of the Academy.",
        ]
        .join("\n");

    c.assert_behaviour(
        "examine.weapon.the-pane-gives-the-skill-the-damage-the-speed-the-range-and-the-ammunition",
        move |_| reads,
    );
    c.shutdown();
}

#[test]
fn scenario_the_weapon_pane_gives_the_skill_the_speed_the_range_and_the_ammunition() {
    scenario("the_weapon_pane_gives_the_skill_the_speed_the_range_and_the_ammunition");
}

/// A thing on somebody else's hook: the client has never been sent the object itself, so where it
/// is worn comes out of the assessment. Without that the weapon lines are absent although the
/// weapon numbers are there; with it, and nothing about the live object having changed, they
/// appear.
pub fn an_item_on_someone_elses_hook_takes_its_slot_from_the_reply() {
    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, SUBJECT);
    {
        // The live object knows nothing: this is a thing in somebody else's house.
        let w = c
            .world_mut()
            .weenie_mut(SUBJECT)
            .expect("the recorded create made it");
        w.pwd.valid_locations = Some(0);
        w.pwd.ammo_type = Some(0);
    }

    let base = AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::WEAPON_PROFILE,
        success_flag: 1,
        weapon_profile: Some(a_bow()),
        ..AppraisalProfile::default()
    };
    assess(&mut c, &mut peer, SUBJECT, &base);
    let plain = item_text(&mut c);
    let absent = !plain.contains("Skill:") && !plain.contains("Range:");

    let hooked = AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::WEAPON_PROFILE
            | dereth_protocol::types::appraisal::flags::HOOK_PROFILE,
        hook_profile: Some(dereth_protocol::types::appraisal::HookAppraisalProfile {
            bitfield: 0,
            valid_locations: 0x0040_0000,
            ammo_type: 1,
        }),
        ..base
    };
    assess(&mut c, &mut peer, SUBJECT, &hooked);
    let on_a_hook = item_text(&mut c);
    let present = [
        "Skill: Missile Weapons",
        "Damage Modifier: +0%.",
        "Speed: Average (40)",
        "Range: 55 yds.",
        "Uses arrows as ammunition.",
    ]
    .iter()
    .all(|l| on_a_hook.contains(l));

    c.assert_behaviour(
        "examine.weapon.an-item-on-someone-elses-hook-takes-its-slot-from-the-reply",
        move |_| absent && present,
    );
    c.shutdown();
}

#[test]
fn scenario_an_item_on_someone_elses_hook_takes_its_slot_from_the_reply() {
    scenario("an_item_on_someone_elses_hook_takes_its_slot_from_the_reply");
}

/// The place to write on a thing is there only on a thing that can be written on, and on one
/// nobody has signed it invites the player to. On anything else there is no box, not a blank one.
pub fn the_inscribe_box_appears_only_on_something_inscribable() {
    /// The one bit of the object's own description that decides it.
    const INSCRIBABLE: u32 = 0x0000_0002;

    let (mut c, mut peer, profile) = assessing_the_recorded(EXAMINE_SESSION, SUBJECT);
    let recorded_bit = c
        .view()
        .world()
        .weenie(SUBJECT)
        .is_some_and(|w| w.pwd.bitfield & INSCRIBABLE != 0);
    assert!(
        recorded_bit,
        "the premise: the recorded object can be written on"
    );
    let invited = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.inscription.clone()
    };

    // Cleared: the box is taken away, not blanked.
    c.world_mut()
        .weenie_mut(SUBJECT)
        .expect("the recorded create made it")
        .pwd
        .bitfield &= !INSCRIBABLE;
    assess(&mut c, &mut peer, SUBJECT, &profile);
    let gone = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.inscription.clone()
    };

    // And back.
    c.world_mut()
        .weenie_mut(SUBJECT)
        .expect("the recorded create made it")
        .pwd
        .bitfield |= INSCRIBABLE;
    assess(&mut c, &mut peer, SUBJECT, &profile);
    let (again, signature) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        (
            screen.examination.inscription.clone(),
            screen.examination.inscription_signature_text.clone(),
        )
    };

    // Written on and signed: the box holds what was written, and the line under it names who
    // wrote it.
    let mut written = profile.clone();
    written.flags |= dereth_protocol::types::appraisal::flags::STRING;
    written.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(7, "For Aldwyne.".to_owned()), (8, "Lark".to_owned())],
    });
    assess(&mut c, &mut peer, SUBJECT, &written);
    let (words, by) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        (
            screen.examination.inscription.clone(),
            screen.examination.inscription_signature_text.clone(),
        )
    };

    c.assert_behaviour(
        "examine.inscription.the-box-is-there-only-on-something-that-can-be-inscribed",
        move |_| {
            invited.as_deref() == Some("<Inscribe here>")
                && gone.is_none()
                && again.as_deref() == Some("<Inscribe here>")
                && signature.is_empty()
                && words.as_deref() == Some("For Aldwyne.")
                && by == "--Lark"
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_inscribe_box_appears_only_on_something_inscribable() {
    scenario("the_inscribe_box_appears_only_on_something_inscribable");
}

// =============================================================================================
// examine.key.*
//
// **Every scenario here presses the key at least twice.** The two halves of the arm differ only
// in what happens when the pane is already open, so a scenario that pressed once would agree with
// a build that could only ever open it.
//
// `Player` has no step for a bound action, so the press below goes into the client's own input
// manager.
// =============================================================================================

/// The thing the key assesses. Seeded rather than replayed: nothing here is about *which* thing,
/// and a corpus replay for it would cost a minute a scenario.
const KEYED: ObjectId = ObjectId(0xC000_0011);

/// Is `<EXAM>` up?
fn pane_is_up(c: &mut HeadlessClient) -> bool {
    let (ui, screen) = gameplay_screen(c.app_mut());
    let root = screen.root().expect("the gameplay screen has a root");
    let h = ui
        .get_child_recursive(root, examination::WINDOW)
        .expect("the assessment window is in the shipped layout");
    ui.node(h).expect("a live node").region.flags.visible
}

/// How many times the pane has been hidden, by any of its close paths, and how many times the
/// arriving answer has shown it.
fn shown_and_hidden(c: &mut HeadlessClient) -> (u32, u32) {
    let (_, screen) = gameplay_screen(c.app_mut());
    (screen.examination.opened, screen.examination.closed)
}

/// How many times the key's **close-first** leg has run, and how many times the client has asked
/// the shard about something. The second moves only on the other leg, which is what says the
/// first one sent nothing.
fn closes_and_asks(c: &mut HeadlessClient) -> (u64, u64) {
    let closes = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .examine_panel_closes;
    let asks = c.world_mut().appraisal.examine_serial;
    (closes, asks)
}

/// A thing in the world for the key to be about.
fn a_thing_to_look_at(c: &mut HeadlessClient) {
    let w = c.world_mut();
    if w.weenie(KEYED).is_none() {
        w.tables
            .weenies
            .insert(KEYED, dereth_client_model::Weenie::new(KEYED));
    }
    if let Some(m) = w.tables.weenies.get_mut(KEYED) {
        m.pwd.name = "Sparring Golem".into();
    }
}

/// One press of the assess key, through the client's own input manager and one whole frame.
fn press_the_key(c: &mut HeadlessClient) {
    let e = dereth_input::InputEvent {
        action: dereth_input::ActionId(dereth_client::interaction::action::SELECTION_EXAMINE),
        input_map: dereth_client::ui::UI_INPUT_MAP,
        toggle: dereth_input::ToggleType::OneShot,
        extent: 1.0,
        start: true,
        repeat_delta: 1,
        repeat_total: 0,
        from_key_down: false,
    };
    c.app_mut()
        .input_manager_mut()
        .expect("the input manager is part of the shell this scenario asked for")
        .inject_action(e);
    c.tick(1);
}

/// The shard's answer for `KEYED`, which is what actually shows the pane.
fn answer_about_it(c: &mut HeadlessClient) {
    let mut sink = dereth_client_model::RecordingSink::default();
    c.world_mut()
        .set_appraise_info(KEYED, AppraisalProfile::default(), &mut sink);
    c.tick(1);
}

/// Put the thing under the pointer.
fn look_at_it(c: &mut HeadlessClient) {
    let mut sink = dereth_client_model::RecordingSink::default();
    c.world_mut()
        .set_selected_object(Some(KEYED), false, &mut sink);
    c.tick(1);
}

/// Open the pane the way a player does: look at something, press the key, let the answer land.
///
/// Returns with the pane **up**, asserted -- the premise every closing claim below rests on. A
/// comparison that never established the pane was open would be satisfied by one that never
/// opened.
fn a_pane_that_is_open() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(3));
    a_thing_to_look_at(&mut c);
    look_at_it(&mut c);
    assert!(!pane_is_up(&mut c), "the premise: the pane starts shut");

    let (_, asked) = closes_and_asks(&mut c);
    press_the_key(&mut c);
    let (closed, asked_now) = closes_and_asks(&mut c);
    assert_eq!(
        asked_now,
        asked + 1,
        "a press on a shut pane asks the shard"
    );
    assert_eq!(closed, 0, "and does not take the closing leg");
    assert!(
        !pane_is_up(&mut c),
        "nor does asking open the window by itself"
    );

    c.tick(1);
    answer_about_it(&mut c);
    c.tick(1);
    assert!(
        pane_is_up(&mut c),
        "the answer for the thing the pane is waiting on shows it"
    );
    assert_eq!(shown_and_hidden(&mut c).0, 1, "and exactly once");
    c
}

/// **The gate.** The key is a toggle: a press on a shut pane asks the shard and the answer opens
/// it, a press on the open pane shuts it and asks nothing, and a third press asks again -- so
/// "it closed" and "it stopped working" are different answers.
pub fn the_assess_key_shuts_an_open_pane_and_opens_a_shut_one() {
    let mut c = a_pane_that_is_open();
    let (shown_before, hidden_before) = shown_and_hidden(&mut c);
    let (_, asked_before) = closes_and_asks(&mut c);

    press_the_key(&mut c);
    let shut = !pane_is_up(&mut c);
    let (closes, asked) = closes_and_asks(&mut c);
    let (shown, hidden) = shown_and_hidden(&mut c);
    // Two claims and not one: a build that shut the pane *and* asked again would look the same
    // for one frame and would pop the window straight back open on the next answer.
    let shut_without_asking = shut
        && closes == 1
        && hidden == hidden_before + 1
        && asked == asked_before
        && shown == shown_before;
    // And the pane is still waiting on the same thing, so a re-poll refills rather than shows.
    let still_waiting = c.world_mut().appraisal.examining == Some(KEYED);

    press_the_key(&mut c);
    let (closes_again, asked_again) = closes_and_asks(&mut c);
    let asked_a_second_time = asked_again == asked + 1 && closes_again == closes;

    c.tick(1);
    answer_about_it(&mut c);
    c.tick(1);
    let open_again = pane_is_up(&mut c) && shown_and_hidden(&mut c).0 == 2;

    c.assert_behaviour(
        "examine.key.the-assess-key-shuts-an-open-pane-and-opens-a-shut-one",
        move |_| shut_without_asking && still_waiting && asked_a_second_time && open_again,
    );
    c.shutdown();
}

#[test]
fn scenario_the_assess_key_shuts_an_open_pane_and_opens_a_shut_one() {
    scenario("the_assess_key_shuts_an_open_pane_and_opens_a_shut_one");
}

/// The shutting happens before the key looks at what is under the pointer, so it works with
/// nothing under it; and with nothing under the pointer and the pane already shut the press does
/// nothing at all rather than opening an empty one.
pub fn the_assess_key_shuts_the_pane_with_nothing_under_the_pointer() {
    let mut c = a_pane_that_is_open();

    // Nothing under the pointer, and the pane put back up by hand: dropping the selection is the
    // pane's *own* second closing path, which would otherwise be the thing being measured.
    c.world_mut().selected = None;
    c.tick(1);
    {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        let h = ui
            .get_child_recursive(root, examination::WINDOW)
            .expect("the assessment window is in the shipped layout");
        ui.set_visible(h, true);
    }
    assert!(
        pane_is_up(&mut c),
        "the premise: the pane is open and nothing is under the pointer"
    );
    let (closes_before, asked_before) = closes_and_asks(&mut c);

    press_the_key(&mut c);
    let (closes, asked) = closes_and_asks(&mut c);
    let shut_anyway = !pane_is_up(&mut c) && closes == closes_before + 1 && asked == asked_before;

    // The mirror: shut, and nothing under the pointer. The press is inert -- it neither asks nor
    // closes, which is a different reading from "the arm never ran".
    let mut c2 = HeadlessClient::new(ClientSpec::gameplay_in_world(3));
    assert!(!pane_is_up(&mut c2), "the premise: the pane starts shut");
    assert_eq!(
        c2.world_mut().selected,
        None,
        "and nothing is under the pointer"
    );
    press_the_key(&mut c2);
    let (closes2, asked2) = closes_and_asks(&mut c2);
    let inert = !pane_is_up(&mut c2) && closes2 == 0 && asked2 == 0;
    c2.shutdown();

    c.assert_behaviour(
        "examine.key.the-pane-shuts-with-nothing-under-the-pointer-and-a-shut-one-does-nothing",
        move |_| shut_anyway && inert,
    );
    c.shutdown();
}

#[test]
fn scenario_the_assess_key_shuts_the_pane_with_nothing_under_the_pointer() {
    scenario("the_assess_key_shuts_the_pane_with_nothing_under_the_pointer");
}

// =============================================================================================
// examine.consumables.* / examine.capacity.* / examine.item-blocks.* / examine.character.*
//
// The four the corpus answers for are driven off recorded answers; the rest are built, because
// no recording carries them -- and each of them goes in as a datagram and comes out as the words
// in the live pane, so a block that is written and never wired fails here even though its own
// unit test passes.
// =============================================================================================

/// The recorded container every built answer in this family is carried on. It has to be something
/// the recording also created: the pane refuses an answer about a thing it has never heard of.
const CARRIER: ObjectId = SUBJECT;

/// A table of whole numbers, which is also the shape a table of flags takes.
fn ints(pairs: &[(u32, i32)]) -> dereth_protocol::archive::PackedHash<u32, i32> {
    dereth_protocol::archive::PackedHash {
        table_size: 32,
        entries: pairs.to_vec(),
    }
}

/// An answer carrying whole numbers and nothing else.
fn plain(pairs: &[(u32, i32)]) -> AppraisalProfile {
    AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::INT,
        success_flag: 1,
        tables: dereth_protocol::types::PropertyTables {
            ints: Some(ints(pairs)),
            ..dereth_protocol::types::PropertyTables::default()
        },
        ..AppraisalProfile::default()
    }
}

/// Assess `CARRIER` with `p` and read the pane back.
fn shows(c: &mut HeadlessClient, peer: &mut dereth_testkit::Peer, p: &AppraisalProfile) -> String {
    assess(c, peer, CARRIER, p);
    item_text(c)
}

/// One whole number out of a recorded answer.
fn recorded_int(p: &AppraisalProfile, key: u32) -> Option<i32> {
    p.tables
        .ints
        .as_ref()
        .and_then(|t| t.entries.iter().find(|(i, _)| *i == key).map(|(_, v)| *v))
}

/// **The gate.** The two recorded potions say what they restore and that they cannot be sold; a
/// healing kit made out of the same answer says what it adds to the skill instead, and both say
/// how many uses are left.
pub fn a_potion_says_what_it_restores_and_a_kit_what_it_adds() {
    // The recorded potion, whose two lines are the recording's own.
    let stamina = {
        let (mut c, _peer, profile) = assessing_the_recorded(EXAMINE_SESSION, POTION);
        // The recorded body, read first, so a corpus change is a failure here and not a silence.
        assert_eq!(
            recorded_int(&profile, 0x59),
            Some(4),
            "the recorded potion's kind"
        );
        assert_eq!(recorded_int(&profile, 0x5A), Some(5), "and how much of it");
        assert_eq!(
            profile.tables.bools.as_ref().and_then(|t| t
                .entries
                .iter()
                .find(|(i, _)| *i == 0x45)
                .map(|(_, v)| *v)),
            Some(0),
            "and that it cannot be sold"
        );
        let text = item_text(&mut c);
        let ok = text.contains("Restores 5 Stamina when consumed.")
            && text.contains("This item cannot be sold.");
        c.shutdown();
        ok
    };

    // The other potion of the same recording, whose kind is a different one.
    let mana = {
        let (mut c, _peer, _) = assessing_the_recorded(EXAMINE_SESSION, ObjectId(0x8000_09B4));
        let ok = item_text(&mut c).contains("Restores 5 Mana when used.");
        c.shutdown();
        ok
    };

    // One built answer, and the one bit of the object that decides which block owns the number:
    // clear, it is a potion and the number is what it restores; set, it is a kit and the number is
    // what it adds to the skill.
    const A_HEALER: u32 = 0x0001_0000;
    let (mut c2, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, CARRIER);
    let built = AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::INT
            | dereth_protocol::types::appraisal::flags::FLOAT,
        success_flag: 1,
        tables: dereth_protocol::types::PropertyTables {
            ints: Some(ints(&[
                (0x13, 20),
                (5, 50),
                (0x5A, 5),
                (0x59, 2),
                (0x5C, 4),
            ])),
            floats: Some(dereth_protocol::archive::PackedHash {
                table_size: 8,
                entries: vec![(0x64, 1.15)],
            }),
            ..dereth_protocol::types::PropertyTables::default()
        },
        ..AppraisalProfile::default()
    };
    let text = shows(&mut c2, &mut peer, &built);
    let as_a_potion = text.contains("Restores 5 Health when used.")
        && !text.contains("Bonus to Healing Skill")
        && text.contains("Number of uses remaining: 4");

    c2.world_mut()
        .weenie_mut(CARRIER)
        .expect("the recorded create made it")
        .pwd
        .bitfield |= A_HEALER;
    let text = shows(&mut c2, &mut peer, &built);
    // **114, not 115.** The fraction is not representable and the client truncates rather than
    // rounds, so asserting 115 would be asserting an arithmetic the client does not do.
    let as_a_kit = text.contains("Bonus to Healing Skill: 5")
        && text.contains("Restoration Bonus: 114%")
        && !text.contains("Restores 5 Health when used.");

    let mut unknown = built.clone();
    unknown.success_flag = 0;
    unknown.tables.ints = Some(ints(&[(0x13, 20), (5, 50), (0x5A, 5), (0x59, 2)]));
    let text = shows(&mut c2, &mut peer, &unknown);
    // Two spaces, which is the client's own and not slack.
    let uses_unknown = text.contains("Number of uses remaining:  Unknown");

    c2.assert_behaviour(
        "examine.consumables.a-potion-says-what-it-restores-and-a-kit-what-it-adds",
        move |_| stamina && mana && as_a_potion && as_a_kit && uses_unknown,
    );
    c2.shutdown();
}

#[test]
fn scenario_a_potion_says_what_it_restores_and_a_kit_what_it_adds() {
    scenario("a_potion_says_what_it_restores_and_a_kit_what_it_adds");
}

/// A container says how much it holds and a book how many of its pages are used -- and the pages
/// are given used-first, which a symmetric subject could not tell from the other way round.
pub fn a_container_says_how_much_it_holds_and_a_book_how_many_pages_are_used() {
    // The recorded book, and then the same book with the two numbers made different.
    let book = ObjectId(0x8000_0A76);
    let (mut c, mut peer, mut profile) = assessing_the_recorded(EXAMINE_SESSION, book);
    assert_eq!(
        recorded_int(&profile, 0xAE),
        Some(12),
        "the recorded book's used pages"
    );
    assert_eq!(
        recorded_int(&profile, 0xAF),
        Some(12),
        "and how many it has"
    );
    let full = item_text(&mut c).contains("12 of 12 pages full.");
    if let Some(t) = profile.tables.ints.as_mut() {
        for e in &mut t.entries {
            if e.0 == 0xAE {
                e.1 = 3;
            }
        }
    }
    assess(&mut c, &mut peer, book, &profile);
    let asymmetric = item_text(&mut c).contains("3 of 12 pages full.");
    c.shutdown();

    // The container: what it holds is read off the **object** and never off the answer, so no
    // recording carries it and every arm below is the client reading its own world.
    let (mut c, mut peer, profile) = assessing_the_recorded(EXAMINE_SESSION, CARRIER);
    let (items, containers) = {
        let w = c
            .view()
            .world()
            .weenie(CARRIER)
            .expect("the recorded create made it");
        (
            i32::from(w.pwd.items_capacity.unwrap_or(0)),
            i32::from(w.pwd.containers_capacity.unwrap_or(0)),
        )
    };
    assert!(
        items > 0 || containers > 0,
        "the premise: the recorded thing really is a container -- got {items} and {containers}"
    );
    let want = if items > 0 && containers > 0 {
        format!("Can hold up to {items} items and {containers} containers.")
    } else if items > 0 {
        format!("Can hold up to {items} items.")
    } else {
        format!("Can hold up to {containers} containers.")
    };
    let recorded_capacity = item_text(&mut c).contains(&want);

    {
        let w = c
            .world_mut()
            .weenie_mut(CARRIER)
            .expect("the recorded create made it");
        w.pwd.items_capacity = Some(0);
        w.pwd.containers_capacity = Some(0);
    }
    let holds_nothing = !shows(&mut c, &mut peer, &profile).contains("Can hold up to");

    let mut three_arms = true;
    for (i, n, want) in [
        (24_u8, 1_u8, "Can hold up to 24 items and 1 containers."),
        (24, 0, "Can hold up to 24 items."),
        (0, 7, "Can hold up to 7 containers."),
    ] {
        {
            let w = c
                .world_mut()
                .weenie_mut(CARRIER)
                .expect("the recorded create made it");
            w.pwd.items_capacity = Some(i);
            w.pwd.containers_capacity = Some(n);
        }
        three_arms &= shows(&mut c, &mut peer, &profile).contains(want);
    }

    c.assert_behaviour(
        "examine.capacity.a-container-says-how-much-it-holds-and-a-book-how-many-pages-are-used",
        move |_| full && asymmetric && recorded_capacity && holds_nothing && three_arms,
    );
    c.shutdown();
}

#[test]
fn scenario_a_container_says_how_much_it_holds_and_a_book_how_many_pages_are_used() {
    scenario("a_container_says_how_much_it_holds_and_a_book_how_many_pages_are_used");
}

/// **Every line the item pane can draw, through the shard.** No recording carries any of these, so
/// each answer is built -- but each goes in as a datagram and comes out as the words the player
/// reads, which is what tells a block that is written and wired from one that is only written.
///
/// The list is deliberately long and flat: one entry is one sentence the pane must be able to
/// produce, and the list itself is the claim.
pub fn every_line_the_item_pane_can_draw_reaches_it_through_the_shard() {
    use dereth_protocol::archive::PackedHash;
    use dereth_protocol::types::appraisal::flags;

    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, CARRIER);
    let mut ok = true;
    let mut says = |c: &mut HeadlessClient,
                    peer: &mut dereth_testkit::Peer,
                    p: &AppraisalProfile,
                    want: &[&str],
                    absent: &[&str]| {
        let text = shows(c, peer, p);
        for w in want {
            if !text.contains(w) {
                eprintln!("the pane never said {w:?}; it said:\n{text}");
                ok = false;
            }
        }
        for w in absent {
            if text.contains(w) {
                eprintln!("the pane said {w:?} and should not have:\n{text}");
                ok = false;
            }
        }
    };

    // How often it has been worked on, and how well it was made.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0xAB, 1)]),
        &["This item has been tinkered 1 time."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0xAB, 4)]),
        &["This item has been tinkered 4 times."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x69, 3)]),
        &["Workmanship: Finely crafted (3)"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x69, 10)]),
        &["Workmanship: Priceless (10)"],
        &[],
    );
    // The salvaged arm: the adjective comes off the average and not off the raw number.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x69, 7), (0xAA, 2)]),
        &["Workmanship: Exquisitely crafted (3.50)\n\nSalvaged from 2 items."],
        &[],
    );
    let mut worked = plain(&[(0xAB, 2)]);
    worked.flags |= flags::STRING;
    worked.tables.strings = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x27, "Bael".to_owned()), (0x28, "Asheron".to_owned())],
    });
    says(
        &mut c,
        &mut peer,
        &worked,
        &["Last tinkered by Bael.", "Imbued by Asheron."],
        &[],
    );

    // The set it belongs to, and an id that names no set at all.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x109, 0x0D)]),
        &["Set: Soldier's"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x109, 0x82)]),
        &["Set: Shimmering Shadows"],
        &[],
    );
    says(&mut c, &mut peer, &plain(&[(0x109, 0x22)]), &[], &["Set: "]);

    // The ratings, whose drawn order is not the order they are read in, and a rating of none
    // which is not a term at all.
    says(
        &mut c,
        &mut peer,
        &plain(&[
            (0x172, 1),
            (0x173, 2),
            (0x174, 3),
            (0x175, 4),
            (0x176, 5),
            (0x177, 6),
            (0x178, 7),
            (0x179, 8),
            (0x17A, 9),
            (0x17B, 10),
        ]),
        &[
            "Ratings: Dam 1, Dam Resist 2, Crit 3, Crit Dam 5, Crit Resist 4, Crit Dam Resist 6, \
             Heal Boost 7, Nether Resist 8, Life Resist 9.",
            "This item adds 10 Vitality.",
        ],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x172, 0), (0x174, 3)]),
        &["Ratings: Crit 3."],
        &[],
    );

    // What it does for defence -- and exactly none is not a line.
    let mut defence = plain(&[]);
    defence.flags |= flags::FLOAT;
    defence.tables.floats = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x1D, 1.08), (0x95, 1.1), (0x96, 1.0)],
    });
    says(
        &mut c,
        &mut peer,
        &defence,
        &[
            "Bonus to Melee Defense: +8.0%.",
            "Bonus to Missile Defense: +10.0%.",
        ],
        &["Bonus to Magic Defense"],
    );

    // What it does for a caster, including the quarter-weight the same number gets against
    // another player.
    let mut caster = plain(&[(0x2D, 0x40)]);
    caster.flags |= flags::FLOAT;
    caster.tables.floats = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x90, 0.05), (0x98, 1.2)],
    });
    says(
        &mut c,
        &mut peer,
        &caster,
        &[
            "Bonus to Mana Conversion: +5%.",
            " vs. Monsters: +20.0%.",
            " vs. Players: +5.0%.",
        ],
        &[],
    );

    // Who may use it, by level, and where it goes.
    for (min, max, want) in [
        (20, 40, "Restricted to characters of Levels 20 to 40."),
        (20, 20, "Restricted to characters of Level 20."),
        (20, 0, "Restricted to characters of Level 20 or greater."),
        (0, 40, "Restricted to characters of Level 40 or below."),
    ] {
        let mut pairs = Vec::new();
        if min != 0 {
            pairs.push((0x56, min));
        }
        if max != 0 {
            pairs.push((0x57, max));
        }
        says(&mut c, &mut peer, &plain(&pairs), &[want], &[]);
    }
    let mut portal = plain(&[]);
    portal.flags |= flags::STRING;
    portal.tables.strings = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x26, "Holtburg".to_owned())],
    });
    says(&mut c, &mut peer, &portal, &["Destination: Holtburg"], &[]);

    // What it takes to hold it.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x9E, 7), (0x9F, 0), (0xA0, 150)]),
        &["Wield requires level 150"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x9E, 2), (0x9F, 45), (0xA0, 250)]),
        &["Wield requires base "],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x9E, 8), (0x9F, 45), (0xA0, 3)]),
        &["Wield requires specialized "],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x9E, 8), (0x9F, 45), (0xA0, 2)]),
        &["Wield requires trained "],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x9E, 12), (0x9F, 0), (0xA0, 2)]),
        &["Wield requires Gharu'ndim race"],
        &[],
    );
    let mut owner = plain(&[]);
    owner.flags |= flags::BOOL;
    owner.tables.bools = Some(ints(&[(0x55, 1)]));
    says(
        &mut c,
        &mut peer,
        &owner,
        &["Wield requires the original owner"],
        &["Created by"],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x1A, 1)]),
        &["Use requires Throne of Destiny."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x144, 5)]),
        &["Wield requires Umbraen"],
        &[],
    );

    // What it takes to use it. A skill the shipped table has no row for still draws its line.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x171, 30)]),
        &["Use requires level 30."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x16E, 9999), (0x16F, 250)]),
        &["Use requires Unknown Skill of at least 250."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x170, 9999)]),
        &["Use requires specialized Unknown Skill."],
        &[],
    );

    // How far it has come along, under each of the two ladders.
    let mut level = plain(&[(0x13F, 5), (0x140, 1)]);
    level.flags |= flags::INT64;
    level.tables.int64s = Some(PackedHash {
        table_size: 8,
        entries: vec![(5, 1_000_000), (4, 3_500_000)],
    });
    says(
        &mut c,
        &mut peer,
        &level,
        &["Item Level: 3 / 5", "Item XP: 3,500,000 / 4,000,000"],
        &[],
    );
    let mut doubling = plain(&[(0x13F, 5), (0x140, 2)]);
    doubling.flags |= flags::INT64;
    doubling.tables.int64s = Some(PackedHash {
        table_size: 8,
        entries: vec![(5, 1_000), (4, 7_000)],
    });
    says(
        &mut c,
        &mut peer,
        &doubling,
        &["Item Level: 3 / 5", "Item XP: 7,000 / 15,000"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x160, 2)]),
        &["This cloak has a chance to reduce an incoming attack by 200 damage."],
        &[],
    );

    // What it takes to switch it on, and who alone may.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x6D, 250), (0x6E, 3), (0xBC, 13)]),
        &["Activation requires Arcane Lore: 250, Allegiance Rank: 3, Olthoi"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x102, 200), (0x101, 1)]),
        &["Activation requires Strength: 200"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x104, 150), (0x103, 1)]),
        &["Activation requires Maximum Health: 150"],
        &[],
    );
    let mut failed = plain(&[(0x6D, 250)]);
    failed.success_flag = 0;
    says(&mut c, &mut peer, &failed, &[], &["Arcane Lore"]);
    let mut only_one = plain(&[]);
    only_one.flags |= flags::BOOL | flags::STRING;
    only_one.tables.bools = Some(ints(&[(0x5E, 1)]));
    only_one.tables.strings = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x19, "Asheron".to_owned())],
    });
    says(
        &mut c,
        &mut peer,
        &only_one,
        &["This item can only be activated by Asheron."],
        &[],
    );

    // A mana stone, both of whose percentages are percentages.
    let mut stone = plain(&[(0x6B, 600)]);
    stone.flags |= flags::FLOAT;
    stone.tables.floats = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x57, 0.8), (0x89, 0.05)],
    });
    says(
        &mut c,
        &mut peer,
        &stone,
        &[
            "Stored Mana: 600",
            "Efficiency: 80%",
            "Chance of Destruction: 5%",
        ],
        &[],
    );
    let mut absorbed = stone.clone();
    absorbed.flags |= flags::SPELL_BOOK;
    absorbed.spell_book = Some(vec![157]);
    says(&mut c, &mut peer, &absorbed, &[], &["Stored Mana"]);

    // What is left in it.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0xC1, 1)]),
        &["Contains 1 key."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0xC1, 3)]),
        &["Contains 3 keys."],
        &[],
    );
    let mut unlimited = plain(&[(0x5C, 4)]);
    unlimited.flags |= flags::BOOL;
    unlimited.tables.bools = Some(ints(&[(0x3F, 1)]));
    says(
        &mut c,
        &mut peer,
        &unlimited,
        &["Number of uses remaining:  Unlimited"],
        &["Number of uses remaining: 4"],
    );

    // Who made it, and how rare it is.
    let mut made_by = plain(&[]);
    made_by.flags |= flags::STRING;
    made_by.tables.strings = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x19, "Ulgrim".to_owned())],
    });
    says(&mut c, &mut peer, &made_by, &["Created by Ulgrim."], &[]);
    let mut rare = plain(&[(0x11, 42)]);
    rare.flags |= flags::BOOL;
    rare.tables.bools = Some(ints(&[(0x6C, 1)]));
    says(
        &mut c,
        &mut peer,
        &rare,
        &[
            "Rare #42",
            "This rare item has a timer restriction of 3 minutes. You will not be able to use \
             another rare item with a timer within 3 minutes of using this one.",
        ],
        &[],
    );

    // And the denominator: nothing is left on the pane's own list of lines it cannot draw.
    let nothing_left = examination::ITEM_BLOCKS_NOT_IMPLEMENTED.is_empty();

    c.assert_behaviour(
        "examine.item-blocks.every-line-the-item-pane-can-draw-reaches-it-through-the-shard",
        move |_| ok && nothing_left,
    );
    c.shutdown();
}

#[test]
fn scenario_every_line_the_item_pane_can_draw_reaches_it_through_the_shard() {
    scenario("every_line_the_item_pane_can_draw_reaches_it_through_the_shard");
}

/// Assessing another player goes to the character pane, which draws the same nine rows plus the
/// player's own level -- and the list of rows that pane cannot draw is empty **because they were
/// drawn**, which is why the list beside it is not.
pub fn the_character_pane_draws_its_rows_and_has_nothing_left_undrawn() {
    let (mut c, _peer, profile) = assessing_the_recorded(ARMOUR_SESSION, ObjectId(0x5000_0003));
    assert!(
        profile.creature_profile.is_some(),
        "the recorded answer carries a creature"
    );

    let (active, level, rows, misc) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        let p = &screen.examination;
        (
            p.active,
            p.level_text.clone(),
            p.creature_rows_drawn,
            p.misc_rows_drawn,
        )
    };

    c.assert_behaviour(
        "examine.character.the-pane-draws-its-rows-and-has-nothing-left-undrawn",
        move |_| {
            active == Some(examination::ExamineSubUi::Char)
                && level.as_deref() == Some("6")
                && rows == 9
                && misc == 5
                && examination::CREATURE_MISC_NOT_IMPLEMENTED.is_empty()
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_character_pane_draws_its_rows_and_has_nothing_left_undrawn() {
    scenario("the_character_pane_draws_its_rows_and_has_nothing_left_undrawn");
}

// =============================================================================================
// skills.rows.*
//
// A spell cast *while playing* moves a skill row at once, not only after a relog: what these
// claims measure is the live arrival path, not the arithmetic.
//
// The character is the recording's, not one the scenario builds: what these claims need is a
// page with a row per skill and a real starting number, and the recorded description has both.
// =============================================================================================

/// The recorded character, **with a shard attached**, for the claims that are about a panel
/// reacting to a message.
///
/// The spells have to arrive as real datagrams. The decoded-event step hands a message straight
/// to the two halves of the client that read it, and the callback the *frame* passes -- the one
/// that tells a panel a notice happened -- is a no-op on that path, so a page that is a cached
/// join rather than a live read never rebuilds. That is exactly the shape of defect this family
/// is about, so a scenario driven that way would be measuring the harness.
///
/// The **description** still comes through the corpus step, because it is nearly two kilobytes
/// and the harness's shard frames one fragment per datagram: a blob that long does not fit in
/// one and the client's parser refuses it. It is a description and not a panel notice, so nothing
/// here turns on which way it arrived.
fn a_recorded_character_and_a_shard() -> (HeadlessClient, dereth_testkit::Peer) {
    let end = description_blob(SESSION).idx;
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut peer = dereth_testkit::Peer::attach(&mut c, LOOKER);

    // **The character is made by the shard**, not written into the tables: an ordered game event
    // is addressed to an object, and the ordered queue only has somewhere to put one for an
    // object the client's own object stream has been told about. A character written straight
    // into the tables passes every visible check and then every spell is dropped in silence.
    let mut p = dereth_protocol::objects::ObjectCreatePayload {
        id: LOOKER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut c,
        dereth_testkit::replay::OBJECT_QUEUE,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("the create encodes"),
    );
    c.tick(1);
    assert!(
        c.world_mut().set_player(LOOKER),
        "the identity is adopted once"
    );

    c.when(Inbound::from_corpus(SESSION, 0..end + 1));
    c.tick(4);
    assert!(
        !c.view().expect_app().hud().skills.is_empty(),
        "the recorded description is this scenario's oracle and it carried no skill"
    );
    (c, peer)
}

/// Two skills far enough apart that culling by key is visible: one gets the spells, the other is
/// the control.
const HEAVY_WEAPONS: u32 = 0x2C;
const ARCANE_LORE: u32 = 0x0E;
const STRENGTH: u32 = 1;

/// What one skill row **draws** -- its number and its colour.
fn skill_row_value(c: &HeadlessClient, skill: u32) -> (i32, u32) {
    let r = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| r.skill == skill)
        .unwrap_or_else(|| panic!("skill {skill:#X} has no row on the page"));
    (r.value, r.font)
}

/// What the attribute page reads for one attribute -- the control beside the skill rows, so that
/// "both broke" and "one broke" are different answers.
fn attribute_shown(c: &HeadlessClient, id: u32) -> i32 {
    let app = c.view().expect_app();
    dereth_ui_screens::view::GameView::attribute(&app.hud().view(app.objects()), id)
        .expect("the attribute page has this row")
}

/// One spell landing on the character, as a real datagram from the shard.
fn cast(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    e: dereth_protocol::types::qualities::Enchantment,
) {
    peer.event(c, &dereth_protocol::qualities::MagicUpdateEnchantment(e));
    c.tick(4);
}

/// One spell on a skill, in its own duelling category.
fn spell_on(
    spell: u16,
    category: u16,
    key: u32,
    delta: f32,
) -> dereth_protocol::types::qualities::Enchantment {
    let mut e = enchantment(
        spell,
        dereth_client_model::enchant::ench_type::SKILL,
        key,
        delta,
    );
    e.category_word = u32::from(category);
    e.power_level = 8;
    e.duration = 1800.0;
    e
}

/// Two spells on one skill in different categories, so neither replaces the other and both have
/// to be counted.
fn two_spells_on_the_same_skill() -> Vec<dereth_protocol::types::qualities::Enchantment> {
    vec![
        spell_on(4624, 100, HEAVY_WEAPONS, 45.0),
        spell_on(2000, 101, HEAVY_WEAPONS, 10.0),
    ]
}

/// **The gate.** A spell that lands while the player is playing moves the skill row it is about,
/// two of them add up, a spell that lowers a skill lowers it and colours it, and a spell on an
/// attribute lifts the skills that are worked out from that attribute -- while leaving every
/// other row alone.
pub fn a_spell_cast_while_playing_moves_the_skill_row_it_is_about() {
    let (mut c, mut peer) = a_recorded_character_and_a_shard();
    // The instrument has to be able to look: an empty page reports "no spell" exactly the way a
    // stale one does, so the denominator comes first.
    assert!(
        c.view().expect_app().hud().skills.len() > 30,
        "the premise: the page holds a row per skill, and it holds {}",
        c.view().expect_app().hud().skills.len()
    );
    // **The recorded character is not a blank one**: it carries something that raises every
    // skill a little, so the starting colour is the raised one and what is measured below is the
    // movement rather than the absolute.
    let (base, base_font) = skill_row_value(&c, HEAVY_WEAPONS);
    assert_ne!(base_font, 2, "the premise: nothing has lowered this skill");

    let spells = two_spells_on_the_same_skill();
    cast(&mut c, &mut peer, spells[0]);
    // The arrival, proved before anything is concluded about the sum.
    let arrived = {
        let w = c.view().world();
        w.player
            .and_then(|p| w.weenie(p))
            .and_then(|we| we.qualities.as_ref())
            .map(|q| {
                q.enchantments.add_list.len() == 1
                    && q.enchantments.add_list[0].smod.key == HEAVY_WEAPONS
                    && (q.enchantments.add_list[0].smod.value - 45.0).abs() < 1e-6
            })
            .unwrap_or(false)
    };
    let (one, one_font) = skill_row_value(&c, HEAVY_WEAPONS);

    cast(&mut c, &mut peer, spells[1]);
    let (two, two_font) = skill_row_value(&c, HEAVY_WEAPONS);

    // The control: an attribute still reads its own total, and the skills worked out from that
    // attribute move with it.
    let strength = attribute_shown(&c, STRENGTH);
    let mut on_strength = enchantment(
        4325,
        dereth_client_model::enchant::ench_type::ATTRIBUTE,
        STRENGTH,
        45.0,
    );
    on_strength.category_word = 200;
    cast(&mut c, &mut peer, on_strength);
    let attribute_moved = attribute_shown(&c, STRENGTH) == strength + 45;
    let through_the_formula = skill_row_value(&c, HEAVY_WEAPONS).0 > two;
    let after_the_attribute = skill_row_value(&c, HEAVY_WEAPONS).0;

    // A spell that lowers one skill lowers that one, colours it, and leaves the other alone.
    let (lore_base, lore_font) = skill_row_value(&c, ARCANE_LORE);
    assert_ne!(
        lore_font, 2,
        "the premise: nothing has lowered this one either"
    );
    cast(&mut c, &mut peer, spell_on(3000, 102, ARCANE_LORE, -20.0));
    let (lore, lore_after_font) = skill_row_value(&c, ARCANE_LORE);
    // And the other skill is untouched by it: the spells are sorted by what they are about before
    // any of them is applied.
    let the_other_is_untouched = skill_row_value(&c, HEAVY_WEAPONS).0 == after_the_attribute;

    c.assert_behaviour(
        "skills.rows.a-spell-cast-while-playing-moves-the-skill-row-it-is-about",
        move |_| {
            arrived
                && one == base + 45
                && one_font == 1
                && two == base + 55
                && two_font == 1
                && attribute_moved
                && through_the_formula
                && lore == lore_base - 20
                && lore_after_font == 2
                && the_other_is_untouched
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_spell_cast_while_playing_moves_the_skill_row_it_is_about() {
    scenario("a_spell_cast_while_playing_moves_the_skill_row_it_is_about");
}

/// A character who logs in **already** carrying both spells gets the same total the live path
/// produces: the login path and the live path share one arithmetic.
pub fn a_character_who_logs_in_already_enchanted_reads_the_same_total() {
    use dereth_protocol::types::qualities::{quality_flags, EnchantmentRegistry};

    let mut c = a_recorded_character(SESSION);
    let (base, _) = skill_row_value(&c, HEAVY_WEAPONS);

    // The recording's own description, delivered a second time with the two spells already in the
    // registry it carries -- which is what a character who logs in enchanted receives, because
    // the live message is only sent when something *changes*.
    let mut d = recorded_description(SESSION);
    d.qualities.flags |= quality_flags::ENCHANTMENT_REGISTRY;
    d.qualities.enchantments = Some(EnchantmentRegistry {
        flags: EnchantmentRegistry::ADDITIVE,
        additive: Some(two_spells_on_the_same_skill()),
        ..EnchantmentRegistry::default()
    });
    // The description goes in as a decoded event: it is the same two kilobytes the harness's
    // shard cannot frame in one datagram, and the arm that unpacks it rebuilds the page itself
    // rather than through the notice a panel would hear.
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(d)),
    ));
    c.tick(8);

    let took_both = {
        let w = c.view().world();
        w.player
            .and_then(|p| w.weenie(p))
            .and_then(|we| we.qualities.as_ref())
            .is_some_and(|q| q.enchantments.add_list.len() == 2)
    };
    let (after, font) = skill_row_value(&c, HEAVY_WEAPONS);

    c.assert_behaviour(
        "skills.rows.a-character-who-logs-in-already-enchanted-reads-the-same-total",
        move |_| took_both && after == base + 55 && font == 1,
    );
    c.shutdown();
}

#[test]
fn scenario_a_character_who_logs_in_already_enchanted_reads_the_same_total() {
    scenario("a_character_who_logs_in_already_enchanted_reads_the_same_total");
}

// =============================================================================================
// house.*
//
// The House pane: what it draws from the shard's house message, the purchase text, number
// grouping, and the abandon gesture.
//
// # The payload is laid out by hand, and that is deliberate
//
// No recorded session carries the shard's house message: a shard only sends one to an account
// that owns a house and the recorded character owns none. So the bytes are written out by hand in
// the shard's own writer order -- as raw dwords rather than through this tree's own encoder, so
// that a decoder regression cannot hide by agreeing with the encoder that produced its input.
// That property is worth more than a shared builder would be, so [`house_data_body`] keeps the
// layout whole.
// =============================================================================================

/// The object this family's shard talks about, which is the player's own body.
const HOUSE_PLAYER: ObjectId = ObjectId(0x5000_0001);

/// The quality that says when the player last bought a house.
const HOUSE_PURCHASE_TIMESTAMP: u32 = 0xC7;

/// Holtburg, the cell the client's own coordinate read-out is pinned over.
const HOUSE_CELL: u32 = 0xA9B4_0025;

/// The two house types the pane branches on, and their maintenance periods in seconds.
const VILLA: u32 = 2;
const APARTMENT: u32 = 4;
const THIRTY_DAYS: i64 = 2_592_000;
const NINETY_DAYS: i64 = 90 * 86_400;

/// One payment of a price list: how many, how many are paid, which thing, and its two names.
///
/// The three numbers are in the shard's **member** order and not its layout order -- writing the
/// thing's id first decodes a price list with the quantity in the wrong field.
fn payment(num: i32, paid: i32, wcid: u32, name: &str, plural: &str) -> Vec<u8> {
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

/// A bare price list, which is the whole body of the maintenance-payment update.
fn payment_list(items: &[Vec<u8>]) -> Vec<u8> {
    let mut b = u32::try_from(items.len())
        .expect("a short list")
        .to_le_bytes()
        .to_vec();
    for p in items {
        b.extend_from_slice(p);
    }
    b
}

/// The whole house body: the two instants, the type, whether maintenance is waived, the two price
/// lists and the position -- in the shard's own order.
fn house_data_body(
    buy_time: i32,
    rent_time: i32,
    house_type: u32,
    maintenance_free: i32,
    buy: &[Vec<u8>],
    rent: &[Vec<u8>],
    cell: u32,
) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&buy_time.to_le_bytes());
    b.extend_from_slice(&rent_time.to_le_bytes());
    b.extend_from_slice(&house_type.to_le_bytes());
    b.extend_from_slice(&maintenance_free.to_le_bytes());
    for list in [buy, rent] {
        b.extend_from_slice(
            &u32::try_from(list.len())
                .expect("a short list")
                .to_le_bytes(),
        );
        for p in list {
            b.extend_from_slice(p);
        }
    }
    b.extend_from_slice(&cell.to_le_bytes());
    for f in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b
}

/// A villa bought outright whose maintenance is two thirds paid -- so the pane takes its
/// *warning* arm, which is the one a happy path never reaches.
fn the_villa(buy_time: i32, rent_time: i32) -> Vec<u8> {
    house_data_body(
        buy_time,
        rent_time,
        VILLA,
        0,
        &[payment(30_000, 30_000, 273, "Pyreal", "Pyreals")],
        &[payment(30_000, 20_000, 273, "Pyreal", "Pyreals")],
        HOUSE_CELL,
    )
}

/// A villa as retail screenshots show it: bought for two million with three things in the price,
/// and a maintenance period paid in full.
fn the_owners_villa() -> Vec<u8> {
    house_data_body(
        1_600_000_000,
        1_700_000_000,
        VILLA,
        0,
        &[
            payment(2_000_000, 2_000_000, 273, "Pyreal", "Pyreals"),
            payment(5, 5, 11_710, "Writ of Refuge", "Writs of Refuge"),
            payment(1, 1, 511, "Crude Lockpick", "Crude Lockpicks"),
        ],
        &[
            payment(100_000, 100_000, 273, "Pyreal", "Pyreals"),
            payment(2, 2, 21_073, "Writ of Refuge", "Writs of Refuge"),
        ],
        HOUSE_CELL,
    )
}

/// The wall clock, which is what the purchase-wait arithmetic reads.
fn house_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .expect("a clock after 1970")
}

/// An instant as the pane prints it, in whatever zone the host is in -- which is what the pane
/// asks for too, so this file stays green wherever it runs.
fn c_time(t: i64) -> String {
    dereth_ui_screens::ctime::strftime_c(t, dereth_client::platform::local_utc_offset_secs(t))
}

/// The frames the pane needs to redraw after something arrives.
fn house_settle(c: &mut HeadlessClient) {
    c.tick(6);
}

/// One ordered event with a **hand-laid body**, through the harness's own shard.
///
/// [`Peer::event`] takes a typed message and encodes it, which is exactly what these claims may
/// not do; [`Peer::replay_blob`] re-addresses and re-stamps a blob the caller framed, so the
/// envelope is the shard's and the body is the scenario's.
fn house_event(
    c: &mut HeadlessClient,
    peer: &mut Peer,
    opcode: dereth_protocol::Opcode,
    body: &[u8],
) {
    let mut blob = 0xF7B0_u32.to_le_bytes().to_vec();
    // The recipient and the stamp `replay_blob` overwrites.
    blob.extend_from_slice(&[0_u8; 8]);
    blob.extend_from_slice(&opcode.0.to_le_bytes());
    blob.extend_from_slice(body);
    peer.replay_blob(c, blob);
    house_settle(c);
}

/// Tell the client how long ago the player bought a house.
fn purchased_at(c: &mut HeadlessClient, peer: &mut Peer, sequence: u8, when: i64) {
    let m = dereth_protocol::qualities::QualitiesPrivateUpdateInt(
        dereth_protocol::qualities::PrivateUpdate {
            sequence,
            property_id: HOUSE_PURCHASE_TIMESTAMP,
            value: i32::try_from(when).expect("an instant that fits"),
        },
    );
    peer.event(c, &m);
    house_settle(c);
}

/// A client in the world with a shard attached, a player it has adopted, and the description the
/// last row of the pane is gated on.
///
/// **The description matters and its absence is silent**: the row about buying another house is
/// drawn only once the client has been told who the player is, so a scenario without it asserts
/// over a row that was never drawn.
fn a_house_client() -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut peer = Peer::attach_creating(&mut c, HOUSE_PLAYER);
    {
        let w = c.world_mut();
        w.player = Some(HOUSE_PLAYER);
        w.weenie_mut(HOUSE_PLAYER)
            .expect("this shard created the player")
            .pwd
            .name = "Larktest".to_owned();
    }
    peer.event(
        &mut c,
        &dereth_protocol::login::LoginPlayerDescription::default(),
    );
    house_settle(&mut c);
    assert!(
        c.view().world().player_qualities().is_some(),
        "the last row of the pane is gated on the client knowing who the player is"
    );
    assert!(
        c.view().expect_app().hud().panels.house.fully_bound(),
        "the House pane is bound off the gameplay root"
    );
    (c, peer)
}

/// The pane's own record of what it wrote, with the colour index of each row.
fn house_lines(
    c: &HeadlessClient,
) -> Vec<(
    String,
    dereth_ui_screens::panels::house::HousePanelTextColor,
)> {
    c.view().expect_app().hud().panels.house.lines.clone()
}

/// Every row as its **element really holds it** -- the far side of the write, which is what a
/// draw would rasterise and what the player sees.
fn house_rows_drawn(c: &mut HeadlessClient) -> Vec<String> {
    let handles = c
        .view()
        .expect_app()
        .hud()
        .panels
        .house
        .row_elements()
        .to_vec();
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the shell is up");
    handles
        .into_iter()
        .map(|h| {
            shell
                .ui
                .text_element_mut(h)
                .map_or_else(String::new, |t| t.glyphs.inq_text(false))
        })
        .collect()
}

/// Every row as `(text, the colour its glyphs carry, the height of its box)`.
fn house_rows_drawn_full(c: &mut HeadlessClient) -> Vec<(String, u32, i32)> {
    let handles = c
        .view()
        .expect_app()
        .hud()
        .panels
        .house
        .row_elements()
        .to_vec();
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the shell is up");
    handles
        .into_iter()
        .map(|h| {
            let height = shell.ui.screen_box(h).height();
            let t = shell
                .ui
                .text_element_mut(h)
                .expect("a row is a text element");
            let text = t.glyphs.inq_text(false);
            let colour = t.glyphs.glyphs.first().map_or(0, |g| g.color);
            (text, colour, height)
        })
        .collect()
}

/// The one drawn row that begins with `prefix`.
fn house_row_starting(c: &mut HeadlessClient, prefix: &str) -> String {
    let rows = house_rows_drawn(c);
    rows.iter()
        .find(|r| r.starts_with(prefix))
        .unwrap_or_else(|| panic!("no row begins {prefix:?}; the pane holds {rows:#?}"))
        .clone()
}

// ---------------------------------------------------------------------------------------------

/// **The gate.** A character with no house sees two rows; the shard's description of one fills
/// all eight, in the pane's own order, each with its own colour.
pub fn the_shards_house_message_fills_every_row_of_the_tab() {
    use dereth_ui_screens::panels::house::{self, HousePanelTextColor as Colour};

    let (mut c, mut peer) = a_house_client();
    let bought = house_now();
    purchased_at(&mut c, &mut peer, 1, bought);

    assert_eq!(c.view().expect_app().hud().stats.house_data_notices, 0);
    assert!(!c.view().expect_app().hud().panels.house.owns_house);
    let before = house_rows_drawn(&mut c);
    let wait_line = format!(
        "{}{}{}",
        house::BUY_LANDSCAPE_AT,
        c_time(bought + THIRTY_DAYS),
        house::APARTMENT_EXEMPTION
    );
    assert_eq!(
        before,
        vec![house::NO_HOUSE.to_owned(), wait_line.clone()],
        "with no house the pane is the line saying so and the line about buying one"
    );

    let buy_time = 1_500_000_000_i32;
    let rent_time = 1_600_000_000_i32;
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(buy_time, rent_time),
    );

    let notices = c.view().expect_app().hud().stats.house_data_notices;
    let owns = c.view().expect_app().hud().panels.house.owns_house;
    let period_end = i64::from(rent_time) + THIRTY_DAYS;
    let want: Vec<(String, Colour)> =
        vec![
        (format!("{}30,000 Pyreals", house::PURCHASE_PRICE), Colour::Normal),
        (format!("{}20,000/30,000 Pyreals", house::RENT), Colour::Normal),
        (format!("{}{}", house::BOUGHT, c_time(i64::from(buy_time))), Colour::Normal),
        (format!("{}{}", house::PERIOD_ENDS, c_time(period_end)), Colour::Normal),
        // Maintenance is still owed, so the next one is due at the end of *this* period and not
        // a period past it.
        (format!("{}{}", house::NEXT_DUE, c_time(period_end)), Colour::Normal),
        // The latitude is drawn before the longitude, and the cell is normalised against the
        // origin -- so this is the corner of Holtburg's own block.
        ("Location: 42.1N, 33.3E".to_owned(), Colour::Normal),
        (
            "Warning!  You have not paid your maintenance costs for the last 30 day maintenance \
             period.  Please pay these costs by this deadline or you will lose your house, and \
             all your items within it."
                .to_owned(),
            Colour::RentNotPaid,
        ),
        (wait_line, Colour::Normal),
    ];
    let got = house_lines(&c);
    // And the same eight really reached their elements, which is the side the player reads.
    let drawn = house_rows_drawn(&mut c);
    let drawn_matches: Vec<String> = want.iter().map(|(s, _)| s.clone()).collect();

    c.assert_behaviour(
        "house.data.the-shards-house-message-fills-every-row-of-the-tab",
        move |_| notices == 1 && owns && got == want && drawn == drawn_matches,
    );
    c.shutdown();
}

#[test]
fn scenario_the_shards_house_message_fills_every_row_of_the_tab() {
    scenario("the_shards_house_message_fills_every_row_of_the_tab");
}

/// The line about buying another house, in all three of its arms and read off the **tree**.
///
/// A first arm that stops at the date is invisible to the pane's own record of what it meant to
/// write, because the near side of the write holds the whole sentence all along. Every reading
/// here is the glyph list a draw would rasterise.
pub fn the_purchase_time_line_reads_what_retail_prints_in_each_arm() {
    use dereth_ui_screens::panels::house;

    let (mut c, mut peer) = a_house_client();

    // Arm one: bought just now, so the wait has not run out. Three terms, and the third is the
    // one a truncated line drops.
    let bought = house_now();
    purchased_at(&mut c, &mut peer, 1, bought);
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let first = house_rows_drawn(&mut c)
        .last()
        .expect("the pane has rows")
        .clone();
    let want_first = format!(
        "{}{}{}",
        house::BUY_LANDSCAPE_AT,
        c_time(bought + THIRTY_DAYS),
        house::APARTMENT_EXEMPTION
    );

    // Arm two: bought forty days ago, and the house is still theirs.
    purchased_at(&mut c, &mut peer, 2, bought - 40 * 86_400);
    let second = house_rows_drawn(&mut c)
        .last()
        .expect("the pane has rows")
        .clone();

    // Arm three: the same wait, and the house is gone.
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_STATUS,
        &2_u32.to_le_bytes(),
    );
    let still_owns = c.view().expect_app().hud().panels.house.owns_house;
    let after = house_rows_drawn(&mut c);
    let third = after.last().expect("the pane has rows").clone();

    c.assert_behaviour(
        "house.purchase-time.the-line-reads-what-retail-prints-in-each-of-its-three-arms",
        move |_| {
            first == want_first
                && first.ends_with(house::APARTMENT_EXEMPTION)
                && second == house::BUY_AFTER_ABANDON
                // The third term belongs to the first arm alone and neither of the others may
                // grow one.
                && !second.contains(house::APARTMENT_EXEMPTION)
                && !still_owns
                && after.len() == 2
                && after[0] == house::NO_HOUSE
                && third == house::BUY_IMMEDIATELY
                && !third.contains(house::APARTMENT_EXEMPTION)
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_purchase_time_line_reads_what_retail_prints_in_each_arm() {
    scenario("the_purchase_time_line_reads_what_retail_prints_in_each_arm");
}

/// An apartment takes the other side of every guard in the pane at once: no location row, a
/// ninety-day period, the next payment two whole periods out, and the paid sentence in its own
/// colour.
pub fn a_paid_up_apartment_drops_the_location_row_and_doubles_the_period() {
    use dereth_ui_screens::panels::house::{self, HousePanelTextColor as Colour};

    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, house_now());

    let rent_time = 1_600_000_000_i32;
    let body = house_data_body(
        1_500_000_000,
        rent_time,
        APARTMENT,
        0,
        // One of a thing takes its singular name and not its plural.
        &[payment(1, 1, 273, "Pyreal", "Pyreals")],
        &[payment(10_000, 10_000, 273, "Pyreal", "Pyreals")],
        HOUSE_CELL,
    );
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &body,
    );

    let text = house_rows_drawn(&mut c);
    let lines = house_lines(&c);
    let ends = format!(
        "{}{}",
        house::PERIOD_ENDS,
        c_time(i64::from(rent_time) + NINETY_DAYS)
    );
    let due = format!(
        "{}{}",
        house::NEXT_DUE,
        c_time(i64::from(rent_time) + 2 * NINETY_DAYS)
    );

    c.assert_behaviour(
        "house.data.a-paid-up-apartment-drops-the-location-row-and-doubles-the-period",
        move |_| {
            text.len() == 7
                && !text.iter().any(|l| l.starts_with("Location:"))
                && text[0] == format!("{}1 Pyreal", house::PURCHASE_PRICE)
                && text[1] == format!("{}10,000/10,000 Pyreals", house::RENT)
                && text[3] == ends
                && text[4] == due
                && text[5] == house::MAINTENANCE_ALREADY_PAID
                && lines[5].1 == Colour::RentPaid
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_paid_up_apartment_drops_the_location_row_and_doubles_the_period() {
    scenario("a_paid_up_apartment_drops_the_location_row_and_doubles_the_period");
}

/// A zero instant is *not available* rather than the first moment of 1970 -- and the zero that
/// still has a period added to it is a real date, which is what makes the first a sentinel and
/// not a rule about zero.
pub fn a_zero_instant_prints_the_sentinel_and_not_the_epoch() {
    use dereth_ui_screens::panels::house;

    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, 0);
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(0, 0),
    );

    let text = house_rows_drawn(&mut c);
    let ends = format!("{}{}", house::PERIOD_ENDS, c_time(THIRTY_DAYS));

    c.assert_behaviour(
        "house.data.a-zero-instant-prints-the-sentinel-and-not-the-start-of-the-epoch",
        move |_| {
            text[2] == format!("{}{}", house::BOUGHT, house::TIME_NA)
                && text[3] == ends
                // A purchase instant of zero is a wait that ran out however the clock is set.
                && text.last().is_some_and(|l| l == house::BUY_AFTER_ABANDON)
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_zero_instant_prints_the_sentinel_and_not_the_epoch() {
    scenario("a_zero_instant_prints_the_sentinel_and_not_the_epoch");
}

/// Every row is made as tall as its own text, so the sentence that wraps onto three lines is
/// drawn on three lines inside its row and inside the pane.
///
/// The assertion is the same arithmetic the scrollbar sizer uses, per row -- so a row that keeps
/// the template's single-line height while holding three lines fails here whichever row it is.
pub fn a_house_row_is_as_tall_as_the_text_it_holds() {
    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, house_now());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_owners_villa(),
    );

    let handles = c
        .view()
        .expect_app()
        .hud()
        .panels
        .house
        .row_elements()
        .to_vec();
    let panel = c
        .view()
        .expect_app()
        .hud()
        .panels
        .house
        .panel
        .expect("the pane is in the tree");
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the shell is up");
    let pane_box = shell.ui.screen_box(panel);
    let mut tall_enough = true;
    let mut wrapped_at_all = false;
    let mut margin_holds = true;
    let mut wrap_width_is_the_templates = true;
    let mut fits_to_text = true;
    let mut last_bottom = 0;
    for h in &handles {
        let b = shell.ui.screen_box(*h);
        let max_width = shell.ui.node(*h).and_then(|n| {
            n.merged_properties()
                .get_int(dereth_ui::props::attr::MAX_WIDTH)
        });
        wrap_width_is_the_templates &= max_width == Some(280);
        let t = shell
            .ui
            .text_element_mut(*h)
            .expect("a row is a text element");
        fits_to_text &= t.bits.fit_to_text();
        let (_, needed) = t.scrollable_extent(b);
        tall_enough &= b.height() >= needed;
        wrapped_at_all |= needed > 27;
        // The height of a row is a whole number of lines plus the template's own bottom margin.
        let drawn_lines = b.height().saturating_sub(8) / 16;
        margin_holds &= b.height() == drawn_lines * 16 + 8 && drawn_lines >= 1;
        last_bottom = b.y1;
    }
    let rows = handles.len();
    let inside = last_bottom <= pane_box.y1;
    let ends_whole = house_rows_drawn(&mut c)
        .last()
        .is_some_and(|r| r.ends_with(dereth_ui_screens::panels::house::APARTMENT_EXEMPTION));

    c.assert_behaviour(
        "house.rows.a-row-is-as-tall-as-the-text-it-holds-and-fits-inside-the-pane",
        move |_| {
            rows == 8
            && wrap_width_is_the_templates
            && fits_to_text
            && tall_enough
            // A fixture with nothing that wraps would prove nothing at all.
            && wrapped_at_all
            && margin_holds
            && inside
            && ends_whole
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_house_row_is_as_tall_as_the_text_it_holds() {
    scenario("a_house_row_is_as_tall_as_the_text_it_holds");
}

/// The paid-maintenance sentence is drawn in the template's second colour and every other row in
/// its first -- read off the composed glyphs, because the colour is a property of the glyph.
pub fn the_paid_maintenance_line_is_drawn_in_the_templates_green() {
    use dereth_ui_screens::panels::house::{self, HousePanelTextColor as Colour};

    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, house_now());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_owners_villa(),
    );

    // The array the colour indices resolve through, read off the shipped row template rather
    // than written down here: an index out of its range leaves the element's own colour, so a
    // template with fewer entries would show as an un-coloured row and not as a failure.
    let palette = {
        let h = *c
            .view()
            .expect_app()
            .hud()
            .panels
            .house
            .row_elements()
            .first()
            .expect("rows");
        let app = c.app_mut();
        app.ui_mut()
            .expect("the shell")
            .ui
            .text_element_mut(h)
            .expect("text")
            .font_colors
            .clone()
    };
    let lines = house_lines(&c);
    let rows = house_rows_drawn_full(&mut c);

    c.assert_behaviour(
        "house.rows.the-paid-maintenance-line-is-drawn-in-the-colour-the-template-ships",
        move |_| {
            palette == vec![0xFFFF_FFFF_u32, 0xFF00_FF00, 0xFFFF_FF00]
                && rows.len() == 8
                && lines[6].1 == Colour::RentPaid
                && rows[6].0 == house::MAINTENANCE_ALREADY_PAID
                && rows[6].1 == palette[1]
                && rows
                    .iter()
                    .enumerate()
                    .all(|(i, r)| i == 6 || r.1 == palette[0])
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_paid_maintenance_line_is_drawn_in_the_templates_green() {
    scenario("the_paid_maintenance_line_is_drawn_in_the_templates_green");
}

/// The two price rows: several payments joined with a comma and a space, every count grouped the
/// way the shipped language data says to, and the row wrapping onto as many lines as it needs.
///
/// The second half is the part that is easy to lose: the composer the other windows share is
/// deliberately **left ungrouped**, because the grouping is applied at this pane's own projection
/// and no screenshot of those other windows was ever taken.
pub fn the_house_price_rows_are_comma_joined_and_grouped() {
    use dereth_ui_screens::panels::house;
    use dereth_ui_screens::panels::numfmt;

    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, house_now());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_owners_villa(),
    );

    let owns = c.view().expect_app().hud().panels.house.owns_house;
    let rows = house_rows_drawn(&mut c);

    // The rule itself, off the shipped language data.
    numfmt::clear_cache();
    let g = numfmt::shipped();
    let rule = (g.size, g.separator.clone());
    let grouped = (
        numfmt::group(2_000_000_i64, &g),
        numfmt::group(100_000_i64, &g),
    );

    // And the shared composer, which is not this pane's projection and stays plain.
    let shared = dereth_client_model::housing::HousePayment {
        num: 2_000_000,
        paid: 2_000_000,
        wcid: 273,
        name: "Pyreal".to_owned(),
        pname: "Pyreals".to_owned(),
    };
    let shared_text = (shared.compose_text(), shared.compose_text2());

    // The wrap the shipped font and the template's own content width really produce.
    let wrapped: Vec<String> = {
        let h = c.view().expect_app().hud().panels.house.row_elements()[0];
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the shell");
        let screen = shell.ui.screen_box(h);
        let t = shell
            .ui
            .text_element_mut(h)
            .expect("a row is a text element");
        let content = t.content_box(screen);
        dereth_ui::text::glyph::wrap(&t.glyphs.glyphs, content.width(), t.glyphs.one_line)
            .into_iter()
            .map(|line| {
                let utf16: Vec<u16> = t.glyphs.glyphs[line.start..line.end]
                    .iter()
                    .map(|g| g.data)
                    .collect();
                String::from_utf16_lossy(&utf16).trim().to_owned()
            })
            .collect()
    };

    c.assert_behaviour(
        "house.prices.the-price-rows-are-comma-joined-and-their-counts-are-grouped",
        move |_| {
            owns
            && rows.len() == 8
            && rows[0]
                == format!(
                    "{}2,000,000 Pyreals, 5 Writs of Refuge, 1 Crude Lockpick",
                    house::PURCHASE_PRICE
                )
            && rows[1]
                == format!("{}100,000/100,000 Pyreals, 2/2 Writs of Refuge", house::RENT)
            // There is no full stop between payments in either composer.
            && !rows[0].contains(". ")
            && rule == (3, ",".to_owned())
            && grouped == ("2,000,000".to_owned(), "100,000".to_owned())
            && shared_text == ("2000000 Pyreals".to_owned(), "2000000/2000000 Pyreals".to_owned())
            && wrapped.len() == 3
            && wrapped[2] == "Lockpick"
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_house_price_rows_are_comma_joined_and_grouped() {
    scenario("the_house_price_rows_are_comma_joined_and_grouped");
}

/// Learning who the player is asks the shard about their house, once -- and a second telling asks
/// nothing, while re-arming the flag the way a log-off does asks again.
///
/// The last step is what makes the zero in the middle a guard rather than a send path that never
/// worked.
pub fn the_client_asks_about_the_house_once_when_it_learns_who_it_is() {
    let (mut c, mut peer) = a_house_client();

    let initialized = c.view().world().player_initialized;
    // The requests are drained every frame, so the counters are the durable record and the
    // outbound list is what names the one that went.
    let sent = |c: &HeadlessClient| {
        let s = &c.view().interaction().stats;
        s.requests_sent + s.requests_undeliverable
    };
    let after_first = sent(&c);
    let named = c
        .view()
        .outbound()
        .iter()
        .any(|r| matches!(r, dereth_client_model::Request::QueryHouse(_)));

    let desc = dereth_protocol::login::LoginPlayerDescription::default();
    peer.event(&mut c, &desc);
    house_settle(&mut c);
    let after_second = sent(&c);

    c.world_mut().player_initialized = false;
    peer.event(&mut c, &desc);
    house_settle(&mut c);
    let after_third = sent(&c);

    c.assert_behaviour(
        "house.query.the-client-asks-about-the-house-once-when-it-learns-who-it-is",
        move |_| {
            initialized && named && after_second == after_first && after_third == after_first + 1
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_client_asks_about_the_house_once_when_it_learns_who_it_is() {
    scenario("the_client_asks_about_the_house_once_when_it_learns_who_it_is");
}

/// A new maintenance period moves both dates and puts every payment back to nothing paid, so the
/// pane keeps warning rather than showing the last period's payment against the new one.
pub fn a_new_maintenance_period_clears_every_payment() {
    use dereth_ui_screens::panels::house::HousePanelTextColor as Colour;

    let (mut c, mut peer) = a_house_client();
    let rent_time = 1_600_000_000_i32;
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, rent_time),
    );
    let before_rows = house_rows_drawn(&mut c).len();
    let before_ends = house_row_starting(&mut c, "This maintenance period ends: ");
    let before_rent = house_row_starting(&mut c, "Rent:");

    let next = rent_time + i32::try_from(THIRTY_DAYS).expect("a period that fits");
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RENT_TIME,
        &next.to_le_bytes(),
    );

    let stats = c.view().expect_app().hud().stats;
    let after_ends = house_row_starting(&mut c, "This maintenance period ends: ");
    let after_rent = house_row_starting(&mut c, "Rent:");
    let after_rows = house_rows_drawn(&mut c).len();
    let warnings = house_lines(&c)
        .iter()
        .filter(|(_, col)| *col == Colour::RentNotPaid)
        .count();

    c.assert_behaviour(
        "house.rent.a-new-period-moves-both-dates-and-marks-every-payment-unpaid",
        move |_| {
            before_rows == 8
                && before_rent == "Rent:\n20,000/30,000 Pyreals"
                && stats.house_rent_time_updates == 1
                && stats.house_rent_time_applied == 1
                && after_rent == "Rent:\n0/30,000 Pyreals"
                && after_ends != before_ends
                && after_rows == 8
                // A fresh period is an unpaid one, which is the whole point of clearing them.
                && warnings == 1
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_new_maintenance_period_clears_every_payment() {
    scenario("a_new_maintenance_period_clears_every_payment");
}

/// A maintenance-payment update replaces the list rather than merging into it, leaves the
/// purchase row alone, and -- when it pays in full -- changes the numbers, the warning sentence,
/// its colour and how far away the next payment is, all at once.
pub fn a_payment_update_replaces_the_list_rather_than_merging() {
    use dereth_ui_screens::panels::house::HousePanelTextColor as Colour;

    let (mut c, mut peer) = a_house_client();
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let buy_before = house_row_starting(&mut c, "The purchase price");
    let ends = house_row_starting(&mut c, "This maintenance period ends: ");
    let due_before = house_row_starting(&mut c, "Maintenance is next due: ");
    let same_while_owed = ends.trim_start_matches("This maintenance period ends: ")
        == due_before.trim_start_matches("Maintenance is next due: ");

    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RENT_PAYMENT,
        &payment_list(&[payment(30_000, 30_000, 273, "Pyreal", "Pyreals")]),
    );

    let stats = c.view().expect_app().hud().stats;
    let rent = house_row_starting(&mut c, "Rent:");
    let buy_after = house_row_starting(&mut c, "The purchase price");
    let ends_after = house_row_starting(&mut c, "This maintenance period ends: ");
    let due_after = house_row_starting(&mut c, "Maintenance is next due: ");
    let coloured: Vec<Colour> = house_lines(&c)
        .into_iter()
        .map(|(_, col)| col)
        .filter(|col| *col != Colour::Normal)
        .collect();
    let paid_sentence = house_rows_drawn(&mut c)
        .iter()
        .any(|t| t.starts_with("The maintenance has already been paid"));

    // ...and a shorter list really is shorter: a second update with one different payment in it
    // leaves nothing of the first behind.
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RENT_PAYMENT,
        &payment_list(&[payment(2, 2, 21_073, "Pyreal Scarab", "Pyreal Scarabs")]),
    );
    let replaced = house_row_starting(&mut c, "Rent:");

    c.assert_behaviour(
        "house.rent.a-payment-update-replaces-the-list-rather-than-merging-into-it",
        move |_| {
            same_while_owed
                && stats.house_rent_payment_updates == 1
                && stats.house_rent_payment_applied == 1
                && rent == "Rent:\n30,000/30,000 Pyreals"
                && buy_after == buy_before
                && ends_after == ends
                // Paid in full, so the next payment is a whole extra period away.
                && due_after != due_before
                && coloured == vec![Colour::RentPaid]
                && paid_sentence
                && replaced == "Rent:\n2/2 Pyreal Scarabs"
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_payment_update_replaces_the_list_rather_than_merging() {
    scenario("a_payment_update_replaces_the_list_rather_than_merging");
}

/// Both maintenance updates arrive at a houseless character, and both are deliberately ignored:
/// no house is invented and nothing is redrawn.
///
/// The pair of counters is what makes *arrived* and *applied* separable; without the second there
/// would be no telling a working guard from a message that never reached a receiver at all.
pub fn neither_maintenance_update_touches_a_houseless_character() {
    let (mut c, mut peer) = a_house_client();
    house_settle(&mut c);
    let before = house_rows_drawn(&mut c);

    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RENT_TIME,
        &1_600_000_000_i32.to_le_bytes(),
    );
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RENT_PAYMENT,
        &payment_list(&[payment(30_000, 30_000, 273, "Pyreal", "Pyreals")]),
    );

    let stats = c.view().expect_app().hud().stats;
    let invented = c.view().world().house.is_some();
    let after = house_rows_drawn(&mut c);

    c.assert_behaviour(
        "house.rent.neither-update-touches-a-character-with-no-house",
        move |_| {
            before.len() == 2
                && stats.house_rent_time_updates == 1
                && stats.house_rent_payment_updates == 1
                && stats.house_rent_time_applied == 0
                && stats.house_rent_payment_applied == 0
                && !invented
                && after == before
        },
    );
    c.shutdown();
}

#[test]
fn scenario_neither_maintenance_update_touches_a_houseless_character() {
    scenario("neither_maintenance_update_touches_a_houseless_character");
}

/// A list of who may enter is stored against the object the shard names it for, and one about the
/// player themselves is dropped rather than stored.
pub fn a_restriction_update_reaches_the_object_it_names() {
    const OTHER: ObjectId = ObjectId(0x5000_0002);

    let (mut c, mut peer) = a_house_client();
    {
        let mut p = dereth_protocol::objects::ObjectCreatePayload {
            id: OTHER,
            ..Default::default()
        };
        p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
        p.physicsdesc.setup_id = Some(0x0200_0001);
        p.physicsdesc.timestamps.instance = 1;
        let blob = dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("an object create encodes");
        peer.send(&mut c, 10, blob);
        house_settle(&mut c);
    }

    // The body is unaligned from its second byte, so it is laid out here: a sequence byte, the
    // object the list is about, the open-house flag, and an empty guest table -- whose header is
    // a single zero word, which is the third of the protocol's three hash-table headers and not
    // the one a first draft reaches for.
    let body = |seq: u8, who: ObjectId| -> Vec<u8> {
        let mut b = vec![seq];
        b.extend_from_slice(&who.0.to_le_bytes());
        b.extend_from_slice(&1_u32.to_le_bytes());
        b.extend_from_slice(&0_u32.to_le_bytes());
        b
    };

    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RESTRICTIONS,
        &body(1, OTHER),
    );
    let first = c.view().expect_app().hud().stats;
    let other_has = c
        .view()
        .world()
        .weenie(OTHER)
        .expect("the other object")
        .pwd
        .restrictions
        .is_some();

    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_RESTRICTIONS,
        &body(1, HOUSE_PLAYER),
    );
    let second = c.view().expect_app().hud().stats;
    let player_has = c
        .view()
        .world()
        .weenie(HOUSE_PLAYER)
        .expect("the player")
        .pwd
        .restrictions
        .is_some();

    c.assert_behaviour(
        "house.restrictions.an-update-reaches-the-object-it-names-and-never-the-player",
        move |_| {
            first.house_restriction_updates == 1
                && first.house_restrictions_applied == 1
                && other_has
                && second.house_restriction_updates == 2
                && second.house_restrictions_applied == 1
                && !player_has
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_restriction_update_reaches_the_object_it_names() {
    scenario("a_restriction_update_reaches_the_object_it_names");
}

/// The other answer a shard can give about a house that is gone empties the pane exactly as the
/// first one does.
pub fn a_transaction_answer_clears_the_pane_as_a_status_does() {
    use dereth_ui_screens::panels::house;

    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, house_now() - 40 * 86_400);
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let before = house_rows_drawn(&mut c).len();
    let notices = c.view().expect_app().hud().stats.house_status_notices;

    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_TRANSACTION,
        &2_u32.to_le_bytes(),
    );

    let after_notices = c.view().expect_app().hud().stats.house_status_notices;
    let owns = c.view().world().house.is_some();
    let after = house_rows_drawn(&mut c);

    c.assert_behaviour(
        "house.data.a-transaction-answer-clears-the-pane-exactly-as-a-status-answer-does",
        move |_| {
            before == 8
                && after_notices == notices + 1
                && !owns
                && after.len() == 2
                && after[0] == house::NO_HOUSE
                && after[1] == house::BUY_IMMEDIATELY
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_transaction_answer_clears_the_pane_as_a_status_does() {
    scenario("a_transaction_answer_clears_the_pane_as_a_status_does");
}

/// The player's gesture, end to end: asking to abandon a house asks twice, the first answer
/// sends nothing, the second sends the one request -- and the pane does not move until the shard
/// answers.
///
/// The middle claim is the one that needs saying out loud. A client that emptied the pane on the
/// send would look right in a happy-path screenshot and be wrong for every refusal a shard can
/// answer with, where the house is still the player's and the tab must still say so.
pub fn two_confirmations_send_the_abandon_and_only_the_shard_empties_the_pane() {
    use dereth_ui_screens::panels::house;

    /// The shipped confirmation dialog's Yes.
    const DIALOG_YES: ElementId = ElementId(0x17);

    let (mut c, mut peer) = a_house_client();
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let started_with = house_rows_drawn(&mut c).len();
    let had_house = c.view().world().house.is_some();

    // The command, typed into the chat entry a character at a time and committed with return --
    // which is the only way a player reaches this ladder.
    let mut hand = dereth_testkit::adapters_chat::Hand::new();
    hand.say(&mut c, "@house abandon");
    c.tick(1);

    let first_prompt = dialog_prompt(&mut c);
    answer_the_dialog(&mut c, &mut hand, DIALOG_YES);
    let nothing_yet = abandons(&c).is_empty();

    let second_prompt = dialog_prompt(&mut c);
    answer_the_dialog(&mut c, &mut hand, DIALOG_YES);
    let wire = abandons(&c);
    // ...and it was really framed into a datagram, which the outbox alone does not say.
    let framed = c.outbound_wire().contains(&0x0000_021F);

    // What that request encodes to, on the queue it goes out on: an empty body inside the game
    // action envelope, ordered, on the weenie queue.
    let bytes_and_queue = {
        let mut session = dereth_client_net::client_session::Session::new(
            dereth_client_net::client_session::testing::MockTransport::new(),
        );
        let framed = dereth_client::interaction::send_request(&mut session, &wire[0]);
        let packet = session.transport.sent.last().expect("one datagram").clone();
        let mut want = 0xF7B1_u32.to_le_bytes().to_vec();
        want.extend_from_slice(&1_u32.to_le_bytes());
        want.extend_from_slice(&0x0000_021F_u32.to_le_bytes());
        (framed, packet.payload == want, packet.queue, packet.ordered)
    };

    let still_has_house = c.view().world().house.is_some();
    let still_eight = house_rows_drawn(&mut c).len();

    // The shard's answer, which is the only thing that empties it.
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_STATUS,
        &2_u32.to_le_bytes(),
    );
    let gone = c.view().world().house.is_none();
    let after = house_rows_drawn(&mut c);

    c.assert_behaviour(
        "house.abandon.two-confirmations-send-it-and-only-the-shards-answer-empties-the-pane",
        move |_| {
            started_with == 8
                && had_house
                && first_prompt.as_deref()
                    == Some(dereth_client_model::chat_cmd::HOUSE_ABANDON_FIRST)
                && nothing_yet
                && second_prompt.as_deref()
                    == Some(dereth_client_model::chat_cmd::HOUSE_ABANDON_SECOND)
                && wire.len() == 1
                && matches!(wire[0], dereth_client_model::Request::AbandonHouse(_))
                && framed
                && bytes_and_queue == (true, true, dereth_primitives::NetQueue::Weenie, true)
                && still_has_house
                && still_eight == 8
                && gone
                && after.len() == 2
                && after[0] == house::NO_HOUSE
                && after[1] == house::BUY_IMMEDIATELY
        },
    );
    c.shutdown();
}

#[test]
fn scenario_two_confirmations_send_the_abandon_and_only_the_shard_empties_the_pane() {
    scenario("two_confirmations_send_the_abandon_and_only_the_shard_empties_the_pane");
}

/// Issuing the same confirmed command twice asks twice: the first question is on screen with the
/// second waiting behind it, and answering the first brings up the second in the same words.
///
/// Both commands the client asks about on its own are typed twice before either is answered,
/// `@die` and then `@house abandon`, and each question is answered No, so the claim is the queue
/// alone: nothing reaches the wire, and after the second No nothing is left on screen or waiting.
pub fn the_same_command_twice_asks_twice_one_question_after_the_other() {
    use dereth_client_model::chat_cmd::{DIE_CONFIRMATION, HOUSE_ABANDON_FIRST};
    use dereth_ui::dialog::factory::DEFAULT_QUEUE;

    /// The first prompt and the waiting count beside it, the second prompt and the waiting count
    /// beside that, whether a question was still open after the second answer, and how many were
    /// still waiting.
    type Asked = (
        (Option<String>, usize),
        (Option<String>, usize),
        bool,
        usize,
    );

    let (mut c, mut peer) = a_house_client();
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let mut hand = dereth_testkit::adapters_chat::Hand::new();

    let waiting = |c: &mut HeadlessClient| {
        c.app_mut()
            .ui()
            .map_or(usize::MAX, |s| s.ui.dialogs.waiting_on(DEFAULT_QUEUE))
    };
    let open = |c: &mut HeadlessClient| {
        c.app_mut()
            .ui()
            .is_some_and(|s| s.ui.dialogs.is_dialog_open(DEFAULT_QUEUE))
    };

    // Asks the same question twice through `command`, answering No each time, and returns what
    // the player saw.
    let mut ask_twice = |c: &mut HeadlessClient, command: &str| -> Asked {
        hand.say(c, command);
        c.tick(1);
        hand.say(c, command);
        c.tick(1);
        let first = (dialog_prompt(c), waiting(c));
        answer_the_dialog(c, &mut hand, DIALOG_NO_BUTTON);
        let second = (dialog_prompt(c), waiting(c));
        answer_the_dialog(c, &mut hand, DIALOG_NO_BUTTON);
        (first, second, open(c), waiting(c))
    };

    let die = ask_twice(&mut c, "@die");
    let abandon = ask_twice(&mut c, "@house abandon");

    let sent_nothing = !c.view().outbound().iter().any(|r| {
        matches!(
            r,
            dereth_client_model::Request::Suicide(_)
                | dereth_client_model::Request::AbandonHouse(_)
        )
    });

    c.assert_behaviour(
        "dialog.confirmation.the-same-command-twice-asks-twice-one-question-after-the-other",
        move |_| {
            let asked_twice = |got: &Asked, words: &str| {
                let ((first, waiting_first), (second, waiting_second), still_open, left) = got;
                first.as_deref() == Some(words)
                    && *waiting_first == 1
                    && second.as_deref() == Some(words)
                    && *waiting_second == 0
                    && !still_open
                    && *left == 0
            };
            asked_twice(&die, DIE_CONFIRMATION)
                && asked_twice(&abandon, HOUSE_ABANDON_FIRST)
                && sent_nothing
        },
    );
    c.shutdown();
}

/// Behaviour: dialog.confirmation.the-same-command-twice-asks-twice-one-question-after-the-other
#[test]
fn scenario_the_same_command_twice_asks_twice_one_question_after_the_other() {
    scenario("the_same_command_twice_asks_twice_one_question_after_the_other");
}

/// What the open confirmation dialog's prompt really says, trimmed.
///
/// A dialog's subtree is not under the current screen's roots, so
/// `Target::Element` cannot reach its buttons and `UiSnapshot` cannot see its text; the dialog
/// queue is where an open one is, and that is what this and [`answer_the_dialog`] walk.
fn dialog_prompt(c: &mut HeadlessClient) -> Option<String> {
    let app = c.app_mut();
    let root = app
        .ui()?
        .ui
        .dialogs
        .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)?
        .element?;
    let ui = &mut app.ui_mut()?.ui;
    let h = ui.get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)?;
    ui.text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false).trim().to_owned())
}

/// Every request to abandon a house this scenario has put on the wire so far.
///
/// **It reads the accumulating outbox and not the layer's own `last_sent`**, which is a one-frame
/// window the send pass replaces.
fn abandons(c: &HeadlessClient) -> Vec<dereth_client_model::Request> {
    c.view()
        .outbound()
        .iter()
        .filter(|r| matches!(r, dereth_client_model::Request::AbandonHouse(_)))
        .cloned()
        .collect()
}

/// Press the open dialog's `button` with a real hit-tested click, and run the frames that carry
/// what it raised out to the wire.
fn answer_the_dialog(
    c: &mut HeadlessClient,
    hand: &mut dereth_testkit::adapters_chat::Hand,
    button: ElementId,
) {
    let at = {
        let app = c.app_mut();
        let root = app
            .ui()
            .expect("the shell is up")
            .ui
            .dialogs
            .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)
            .expect("a dialog is open to answer")
            .element
            .expect("an open dialog has a root");
        let ui = &app.ui().expect("the shell is up").ui;
        let h = ui
            .get_child_recursive(root, button)
            .expect("the shipped confirmation root carries both buttons");
        let r = ui.screen_clip_box(h);
        assert!(r.is_valid(), "the button has a real visible clip");
        let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        assert!(
            ui.hit_test_screen(at.0, at.1)
                .is_some_and(|hit| hit == h || ui.is_ancestor_of(h, hit)),
            "the button is the thing under the pointer"
        );
        at
    };
    hand.press_at(c, at.0, at.1);
    c.tick(6);
}

// =============================================================================================
// examine.description.*, examine.portal.*, examine.failed-assess.*
//
// The same harness as the rest of this window: the recorded object, the recorded answer, and the
// one property the scenario adds to it. Everything downstream of the answer is the client's own
// production path.
// =============================================================================================

/// The recorded player, which takes the **character** pane and carries the creature block.
const APPRAISED_PLAYER: ObjectId = ObjectId(0x5000_0003);

/// The two description keys the item pane forks on, the bits it decorates with, and the
/// restrictions of a destroyed portal -- which is what every one of that kind carries in the
/// reference server's own world data.
const SHORT_DESC: u32 = 0x0F;
const LONG_DESC: u32 = 0x10;
const DESTROYED_PORTAL: i32 = 49;

const NO_RECALL_LINE: &str = "This portal cannot be recalled nor linked to.";
const NO_SUMMON_LINE: &str = "This portal cannot be summoned.";
const NO_PK_LINE: &str = "Player Killers may not use this portal.";
const NO_PK_LITE_LINE: &str = "Lite Player Killers may not use this portal.";
const NO_NPK_LINE: &str = "Non-Player Killers may not use this portal.";

/// The same profile with these whole numbers set, replacing any it already carries.
fn with_ints(base: &AppraisalProfile, pairs: &[(u32, i32)]) -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = base.clone();
    let mut entries = p.tables.ints.clone().map(|h| h.entries).unwrap_or_default();
    for (k, v) in pairs {
        entries.retain(|(ek, _)| ek != k);
        entries.push((*k, *v));
    }
    p.flags |= flags::INT;
    p.tables.ints = Some(dereth_protocol::archive::PackedHash {
        table_size: 32,
        entries,
    });
    p
}

/// The same profile with exactly these strings, and no others.
fn with_strings(base: &AppraisalProfile, entries: &[(u32, &str)]) -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = base.clone();
    p.flags |= flags::STRING;
    p.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: entries.iter().map(|(k, v)| (*k, (*v).to_owned())).collect(),
    });
    p
}

/// The assessment window in the live tree.
fn exam_window(c: &mut HeadlessClient) -> ElemHandle {
    let app = c.app_mut();
    let shell = app.ui().expect("the shell is up");
    let root = shell.flow.current().expect("a screen is current").roots()[0];
    shell
        .ui
        .get_child_recursive(root, examination::WINDOW)
        .expect("the window is in the layout")
}

/// One element under `root`.
fn exam_find(c: &mut HeadlessClient, root: ElemHandle, id: ElementId) -> Option<ElemHandle> {
    c.app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .get_child_recursive(root, id)
}

/// Every composed glyph of an element as `(character, colour)` -- the **draw data**, and not a
/// field the panel keeps beside it.
fn painted(c: &mut HeadlessClient, h: ElemHandle) -> Vec<(char, u32)> {
    let app = c.app_mut();
    let box_ = app.ui().expect("the shell is up").ui.screen_box(h);
    let ui = &mut app.ui_mut().expect("the shell is up").ui;
    let Some(t) = ui.text_element_mut(h) else {
        return Vec::new();
    };
    t.compose(box_)
        .into_iter()
        .map(|g| (char::from_u32(u32::from(g.ch)).unwrap_or('?'), g.color))
        .collect()
}

/// The distinct colours the first occurrence of `needle` is painted in, in order.
fn colors_of(p: &[(char, u32)], needle: &str) -> Vec<u32> {
    let text: String = p.iter().map(|(ch, _)| *ch).collect();
    let at = text
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not drawn; the element reads:\n{text}"));
    let mut out = Vec::new();
    for (_, colour) in &p[at..at + needle.chars().count()] {
        if !out.contains(colour) {
            out.push(*colour);
        }
    }
    out
}

/// The plain colour the item description pane's own array declares.
fn description_plain(c: &mut HeadlessClient) -> u32 {
    let w = exam_window(c);
    let h = exam_find(c, w, examination::ITEM_DISPLAY_TEXT).expect("the description pane");
    let app = c.app_mut();
    let ui = &mut app.ui_mut().expect("the shell is up").ui;
    ui.text_element_mut(h)
        .expect("a text element")
        .font_color_at(0)
}

/// The colour index each of the nine stat rows was drawn with, as the panel recorded it.
fn creature_row_colors(c: &mut HeadlessClient) -> Vec<u8> {
    let (_, screen) = gameplay_screen(c.app_mut());
    screen.examination.creature_row_colors.clone()
}

/// The drawn rows of the stat list as `(label, label colour, value, value colour)`, read off the
/// **glyphs each cell is carrying** -- so a colour that never reached a cell fails here and not in
/// the panel's own bookkeeping.
fn drawn_stat_rows(c: &mut HeadlessClient) -> Vec<(String, u32, String, u32)> {
    let w = exam_window(c);
    let base = exam_find(c, w, examination::CREATURE_BASE).expect("the creature pane");
    let list = exam_find(c, base, examination::CREATURE_STAT_LIST).expect("the stat list");
    let rows = c.app_mut().ui().expect("the shell is up").ui.children(list);
    let mut out = Vec::new();
    for row in rows {
        let (Some(label), Some(value)) = (
            exam_find(c, row, examination::ROW_LABEL),
            exam_find(c, row, examination::ROW_VALUE),
        ) else {
            continue;
        };
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let cell = |ui: &mut dereth_ui::UiSystem, h: ElemHandle| -> (String, u32) {
            let Some(t) = ui.text_element_mut(h) else {
                return (String::new(), 0);
            };
            let text: String = t
                .glyphs
                .glyphs
                .iter()
                .map(|g| char::from_u32(u32::from(g.data)).unwrap_or('?'))
                .collect();
            let colour = t.glyphs.glyphs.first().map_or(0, |g| g.color);
            (text, colour)
        };
        let (label_text, label_colour) = cell(ui, label);
        let (value_text, value_colour) = cell(ui, value);
        out.push((label_text, label_colour, value_text, value_colour));
    }
    out
}

/// The colour array the stat list's value cell authors -- **four** where the item description
/// pane authors three, and the fourth is the one the failure arm indexes.
fn value_cell_colors(c: &mut HeadlessClient) -> Vec<u32> {
    let w = exam_window(c);
    let base = exam_find(c, w, examination::CREATURE_BASE).expect("the creature pane");
    let list = exam_find(c, base, examination::CREATURE_STAT_LIST).expect("the stat list");
    let row = c
        .app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .children(list)
        .first()
        .copied()
        .expect("the stat list has a row");
    let value = exam_find(c, row, examination::ROW_VALUE).expect("the row's value cell");
    let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
    ui.text_element_mut(value)
        .expect("a text element")
        .font_colors
        .clone()
}

/// **The last line of the item pane.** A gem that drains experience says how much, in the
/// shipped sentence with the shipped separators, and says nothing at all when the shard did not
/// name a cost -- which is presence and not a positive number.
pub fn the_augmentation_cost_line_comes_off_the_shipped_sentence() {
    use dereth_protocol::types::appraisal::flags;

    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);

    // The sentence itself, off the shipped string table -- a premise, so a data change fails here
    // rather than quietly rewriting the claim.
    //
    // **The row is found by what it says and not by the hash of its name.** The hashing crate the
    // client keys these rows with is not a dependency of this one, and taking one would move the
    // lock file. What the premise needs is that the shipped
    // data carries this sentence -- in two pieces with their own newlines, with exactly one thing
    // interpolated between them -- and that only one row carries it.
    let table: dereth_assets::ui::StringTable = table(&c, 0x2300_0001);
    let want = vec![
        "\\nUsing this gem will drain ".to_owned(),
        " points of your available experience.\\n".to_owned(),
    ];
    let matching: Vec<_> = table
        .strings
        .iter()
        .filter(|(_, r)| r.strings == want)
        .map(|(_, r)| r.clone())
        .collect();
    let shipped = (
        matching.len() == 1,
        matching.first().is_some_and(|r| r.variables.len() == 1),
    );

    let base = with_strings(&base, &[(LONG_DESC, "Augmentation description.")]);
    assess(&mut c, &mut peer, POTION, &base);
    let without = item_text(&mut c);
    let ends_with_description = without.ends_with("Augmentation description.");

    let mut every_cost_reads = true;
    for (cost, formatted) in [
        (5_000_000_000_i64, "5,000,000,000"),
        // The client converts the shard's whole number to a double before it takes its digits
        // out, and this is the first positive one that cannot survive that exactly.
        (9_007_199_254_740_993, "9,007,199,254,740,992"),
        (0, "0"),
        (-1234, "-1,234"),
    ] {
        let mut p = base.clone();
        p.flags |= flags::INT64;
        p.tables.int64s = Some(dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries: vec![(3, cost)],
        });
        assess(&mut c, &mut peer, POTION, &p);
        every_cost_reads &= item_text(&mut c)
            == format!(
                "{without}\nUsing this gem will drain {formatted} points of your available \
                 experience.\n"
            );
    }

    assess(&mut c, &mut peer, POTION, &base);
    let removed = item_text(&mut c) == without;

    c.assert_behaviour(
        "examine.augmentation.the-cost-line-is-the-shipped-sentence-with-the-number-in-it",
        move |_| shipped == (true, true) && ends_with_description && every_cost_reads && removed,
    );
    c.shutdown();
}

#[test]
fn scenario_the_augmentation_cost_line_comes_off_the_shipped_sentence() {
    scenario("the_augmentation_cost_line_comes_off_the_shipped_sentence");
}

/// A thing with a cooldown says how long it is, and how long is left of the player's own -- the
/// second only when the player really is waiting on *that* group, and neither unless the shard
/// named a duration.
pub fn the_cooldown_lines_reach_the_item_pane() {
    use dereth_protocol::types::appraisal::flags;

    /// The two keys: which cooldown group the thing is in, and how long its own is.
    const GROUP: u32 = 0x118;
    const DURATION: u32 = 0xA7;

    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);
    let mut p = with_ints(&base, &[(GROUP, 3)]);
    p.flags |= flags::FLOAT;
    p.tables.floats = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(DURATION, 90.9)],
    });

    // **The looker has to have been described.** The pane asks the *player's own* quality bag
    // what is still running, and this harness's looker is a body the shard created and nothing
    // more -- so the shard describes it first, which is what a real session does at login.
    peer.event(
        &mut c,
        &dereth_protocol::login::LoginPlayerDescription::default(),
    );
    c.tick(2);

    // The recording carries no live cooldown, so the player is given one: the claim is about
    // what the pane reads out of the player's own list, and the list has to have something in it.
    let start = c.view().expect_app().hud().now.0;
    c.world_mut()
        .player_qualities_mut()
        .expect("the recorded character description")
        .enchantments
        .cooldown_list
        .push(dereth_client_model::enchant::Enchantment {
            id: 0x8003,
            spell_category: 0,
            power_level: 0,
            start_time: start,
            duration: 45.75,
            caster: ObjectId(0),
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: start,
            smod: dereth_protocol::types::qualities::StatMod {
                kind: dereth_client_model::enchant::ench_type::COOLDOWN,
                key: 0,
                value: 0.0,
            },
            spell_set_id: None,
        });

    assess(&mut c, &mut peer, POTION, &p);
    let text = item_text(&mut c);
    let now = c.view().expect_app().hud().now.0;
    #[allow(clippy::cast_possible_truncation)]
    let seconds = (45.75 - (now - start)).trunc() as i32;
    assert!(
        (1..60).contains(&seconds),
        "the running scenario must not outlive its own fixture"
    );
    let remaining = format!("Cooldown Remaining: {seconds}s");
    let duration_line = "Cooldown When Used: 1m 30s";
    let both_in_the_model = text.contains(duration_line) && text.contains(&remaining);

    // ...and both really reached the glyphs the player reads.
    let drawn = description(&mut c);
    let both_drawn = drawn.contains(duration_line) && drawn.contains(&remaining);

    // A different group is not the player's, so only the duration is drawn.
    assess(&mut c, &mut peer, POTION, &with_ints(&p, &[(GROUP, 4)]));
    let other = item_text(&mut c);
    let other_group = other.contains(duration_line) && !other.contains("Cooldown Remaining:");

    // An expired one draws no remaining either.
    c.world_mut()
        .player_qualities_mut()
        .expect("the description")
        .enchantments
        .cooldown_list[0]
        .duration = 0.0;
    assess(&mut c, &mut peer, POTION, &p);
    let expired = !item_text(&mut c).contains("Cooldown Remaining:");

    // The group on its own draws neither line: the duration is what opens the block.
    let mut no_duration = p.clone();
    no_duration.tables.floats = None;
    no_duration.flags &= !flags::FLOAT;
    assess(&mut c, &mut peer, POTION, &no_duration);
    let group_alone = !item_text(&mut c).contains("Cooldown");

    // ...and a duration of none is still a duration.
    let mut zero = p.clone();
    zero.tables.floats = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(DURATION, 0.0)],
    });
    zero.flags |= flags::FLOAT;
    assess(&mut c, &mut peer, POTION, &zero);
    let zero_duration = item_text(&mut c).contains("Cooldown When Used: 0s");

    c.assert_behaviour(
        "examine.cooldown.the-pane-says-how-long-it-is-and-how-long-is-left-of-the-players-own",
        move |_| {
            both_in_the_model
                && both_drawn
                && other_group
                && expired
                && group_alone
                && zero_duration
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_cooldown_lines_reach_the_item_pane() {
    scenario("the_cooldown_lines_reach_the_item_pane");
}

/// The long description is decorated in place -- how well it is made in front of it, what it is
/// made of woven into it, what it is set with after it -- and a plating name replaces it outright.
pub fn the_long_description_is_decorated_where_the_player_reads_it() {
    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);

    /// The keys the decoration reads: which parts are known, the workmanship, the material, how
    /// many gems and which kind, and the plating name.
    const KNOWN: u32 = 0xAC;
    const WORKMANSHIP: u32 = 0x69;
    const MATERIAL: u32 = 0x83;
    const GEM_COUNT: u32 = 0xB1;
    const GEM_KIND: u32 = 0xB2;
    const PLATING_NAME: u32 = 0x34;

    let named = with_strings(&base, &[(LONG_DESC, "Steel sword")]);
    let decorated = with_ints(
        &named,
        &[
            (KNOWN, 7),
            (WORKMANSHIP, 3),
            (MATERIAL, 0x40),
            (GEM_COUNT, 2),
            (GEM_KIND, 0x26),
        ],
    );
    assess(&mut c, &mut peer, POTION, &decorated);
    let whole = item_text(&mut c).ends_with("Finely crafted Steel sword, set with 2 Rubies");

    let mut every_gem_reads = true;
    for (count, gem, expected) in [
        (1, 0x26, "Steel sword, set with 1 Ruby"),
        (1000, 0x26, "Steel sword, set with 1,000 Rubies"),
        (2, 0x20, "Steel sword, set with 2 pieces of Onyx"),
        (2, 0x1C, "Steel sword, set with 2 Lapis Lazuli"),
    ] {
        let p = with_ints(
            &decorated,
            &[(KNOWN, 4), (GEM_COUNT, count), (GEM_KIND, gem)],
        );
        assess(&mut c, &mut peer, POTION, &p);
        every_gem_reads &= item_text(&mut c).ends_with(expected);
    }

    // The material survives when the bit that nominally guards it is clear...
    let no_bits = with_ints(&decorated, &[(KNOWN, 0)]);
    assess(&mut c, &mut peer, POTION, &no_bits);
    let material_survives = item_text(&mut c).ends_with("Steel sword");

    // ...and a description the material's name is not already in is not trimmed by the miss.
    assess(
        &mut c,
        &mut peer,
        POTION,
        &with_strings(&no_bits, &[(LONG_DESC, "  sword  ")]),
    );
    let miss_does_not_trim = item_text(&mut c).ends_with("Steel   sword  ");

    let plated = with_strings(
        &base,
        &[
            (LONG_DESC, "old description"),
            (PLATING_NAME, "Plated sword"),
        ],
    );
    assess(&mut c, &mut peer, POTION, &plated);
    let plating_replaces = item_text(&mut c).ends_with("Plated sword");

    // The short one, when it is what is used, is used undecorated and unplated.
    let short = with_strings(
        &decorated,
        &[
            (SHORT_DESC, "short fallback"),
            (PLATING_NAME, "Plated sword"),
        ],
    );
    assess(&mut c, &mut peer, POTION, &short);
    let short_is_bare = item_text(&mut c).ends_with("short fallback");

    c.assert_behaviour(
        "examine.description.the-long-one-is-decorated-and-a-plating-name-replaces-it",
        move |_| {
            whole
                && every_gem_reads
                && material_survives
                && miss_does_not_trim
                && plating_replaces
                && short_is_bare
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_long_description_is_decorated_where_the_player_reads_it() {
    scenario("the_long_description_is_decorated_where_the_player_reads_it");
}

/// The short description is drawn only when there is no long one at all -- an **empty** long one
/// is still one, and suppresses it.
pub fn the_short_description_is_used_only_when_there_is_no_long_one() {
    const SHORT: &str = "The short fallback reached the shipped description pane.";
    const LONG: &str = "The long description keeps priority.";

    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);

    assess(
        &mut c,
        &mut peer,
        POTION,
        &with_strings(&base, &[(SHORT_DESC, SHORT)]),
    );
    let fallback_used = item_text(&mut c).contains(SHORT);

    assess(
        &mut c,
        &mut peer,
        POTION,
        &with_strings(&base, &[(LONG_DESC, LONG), (SHORT_DESC, SHORT)]),
    );
    let with_both = item_text(&mut c);
    let long_wins = with_both.contains(LONG) && !with_both.contains(SHORT);

    assess(
        &mut c,
        &mut peer,
        POTION,
        &with_strings(&base, &[(LONG_DESC, ""), (SHORT_DESC, SHORT)]),
    );
    let empty_still_suppresses = !item_text(&mut c).contains(SHORT);

    assess(&mut c, &mut peer, POTION, &with_strings(&base, &[]));
    let neither = {
        let t = item_text(&mut c);
        !t.contains(SHORT) && !t.contains(LONG)
    };

    c.assert_behaviour(
        "examine.description.the-short-one-is-drawn-only-when-there-is-no-long-one-at-all",
        move |_| fallback_used && long_wins && empty_still_suppresses && neither,
    );
    c.shutdown();
}

#[test]
fn scenario_the_short_description_is_used_only_when_there_is_no_long_one() {
    scenario("the_short_description_is_used_only_when_there_is_no_long_one");
}

/// **A destroyed portal.** Assessing a destroyed portal draws both of its restriction sentences
/// and none of the other three, each restriction draws its own sentence and only its own, they
/// come in the pane's own order, and each is drawn plain.
///
/// Three of the five sentences are tails of one another, so *which* one was drawn is a question
/// about whole lines: asking it with a substring search is a scenario that cannot fail for the
/// first of them.
pub fn each_portal_restriction_draws_its_own_sentence_and_only_its_own() {
    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);
    let carries_none = base.tables.ints.as_ref().is_none_or(|t| {
        !t.entries
            .iter()
            .any(|(k, _)| *k == examination::PORTAL_BITMASK)
    });
    assert!(
        carries_none,
        "the corpus carries no portal restrictions, which is why this adds one"
    );

    // A plain thing first, so "the lines were already there" cannot pass for "the lines landed".
    let plain_before = item_text(&mut c);
    let not_a_portal =
        !plain_before.contains(NO_RECALL_LINE) && !plain_before.contains(NO_SUMMON_LINE);

    let portal = |mask: i32| with_ints(&base, &[(examination::PORTAL_BITMASK, mask)]);

    assess(&mut c, &mut peer, POTION, &portal(DESTROYED_PORTAL));
    let text = item_text(&mut c);
    let destroyed = text.contains(NO_RECALL_LINE)
        && text.contains(NO_SUMMON_LINE)
        && !text.contains(NO_PK_LINE)
        && !text.contains(NO_PK_LITE_LINE)
        && !text.contains(NO_NPK_LINE)
        && text.find(NO_RECALL_LINE) < text.find(NO_SUMMON_LINE);

    // Each bit alone.
    let mut each_alone = true;
    for (bit, literal) in examination::PORTAL_RESTRICTION_LINES {
        // The unrestricted bit is on in every one of the world's own values and has no sentence.
        assess(&mut c, &mut peer, POTION, &portal(0x01 | bit));
        let drawn: Vec<String> = item_text(&mut c).lines().map(str::to_owned).collect();
        let want = literal.trim_end_matches('\n');
        each_alone &= drawn.iter().any(|l| l == want);
        for (other, other_literal) in examination::PORTAL_RESTRICTION_LINES {
            if other == bit {
                continue;
            }
            let unwanted = other_literal.trim_end_matches('\n');
            each_alone &= !drawn.iter().any(|l| l == unwanted);
        }
        each_alone &= drawn
            .iter()
            .filter(|l| l.ends_with("this portal.") || l.starts_with("This portal"))
            .count()
            == 1;
    }

    // All five at once: the pane's own order, and every one of them plain.
    assess(
        &mut c,
        &mut peer,
        POTION,
        &portal(0x01 | 0x02 | 0x04 | 0x08 | 0x10 | 0x20),
    );
    let all = item_text(&mut c);
    let mut at = 0_usize;
    let mut in_order = true;
    for (_, literal) in examination::PORTAL_RESTRICTION_LINES {
        let want = literal.trim_end_matches('\n');
        match all[at..].find(want) {
            Some(found) => at += found + want.len(),
            None => in_order = false,
        }
    }
    let plain = description_plain(&mut c);
    let w = exam_window(&mut c);
    let pane = exam_find(&mut c, w, examination::ITEM_DISPLAY_TEXT).expect("the description pane");
    let glyphs = painted(&mut c, pane);
    let all_plain = [
        NO_PK_LINE,
        NO_PK_LITE_LINE,
        NO_NPK_LINE,
        NO_RECALL_LINE,
        NO_SUMMON_LINE,
    ]
    .into_iter()
    .all(|line| colors_of(&glyphs, line) == vec![plain]);

    // And the denominator: nothing is left on the pane's own list of description blocks it
    // cannot draw.
    let nothing_left = examination::DESCRIPTION_BLOCKS_NOT_IMPLEMENTED.is_empty();

    c.assert_behaviour(
        "examine.portal.each-restriction-draws-its-own-sentence-and-only-its-own",
        move |_| not_a_portal && destroyed && each_alone && in_order && all_plain && nothing_left,
    );
    c.shutdown();
}

#[test]
fn scenario_each_portal_restriction_draws_its_own_sentence_and_only_its_own() {
    scenario("each_portal_restriction_draws_its_own_sentence_and_only_its_own");
}

/// The block's own denominator: a portal with no restriction at all still gains the two
/// separators the block always writes, and a thing that is not a portal gains neither.
///
/// Exact text, because that is what makes the separators assertable at all.
pub fn a_portal_with_no_restrictions_still_gets_the_blocks_separators() {
    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);
    let without = item_text(&mut c);
    let portal = |mask: i32| with_ints(&base, &[(examination::PORTAL_BITMASK, mask)]);

    assess(&mut c, &mut peer, POTION, &portal(0x01));
    let separators_only = item_text(&mut c) == format!("{without}\n\n");

    assess(&mut c, &mut peer, POTION, &portal(DESTROYED_PORTAL));
    let with_two =
        item_text(&mut c) == format!("{without}\n\n{NO_RECALL_LINE}\n{NO_SUMMON_LINE}\n");

    assess(&mut c, &mut peer, POTION, &base.clone());
    let block_skipped = item_text(&mut c) == without;

    c.assert_behaviour(
        "examine.portal.a-portal-with-no-restriction-still-gains-the-blocks-two-separators",
        move |_| separators_only && with_two && block_skipped,
    );
    c.shutdown();
}

#[test]
fn scenario_a_portal_with_no_restrictions_still_gets_the_blocks_separators() {
    scenario("a_portal_with_no_restrictions_still_gets_the_blocks_separators");
}

/// **A failed assessment, and a successful one.** An assessment that failed draws all
/// nine value cells in the unknown colour and leaves every label alone -- and the *successful*
/// assessment of the recorded player was wrong the same way, because the nine cells the shard
/// marked raised were drawn white too.
///
/// The colour array is read off the shipped row first: the value cell authors **four** colours
/// where the item description pane authors three, and the fourth is the one the failure arm
/// indexes. Without it the failure would have nothing to index and would leave the cell as it was.
pub fn a_failed_assessment_draws_every_value_cell_in_the_unknown_colour() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    let palette = value_cell_colors(&mut c);
    let four_colours = palette == vec![0xFFFF_FFFF_u32, 0xFF00_FF00, 0xFFFF_0000, 0xFFFF_FF00]
        && palette[usize::from(examination::UNKNOWN_FONT)] == 0xFFFF_FF00;

    let succeeded = base.success_flag != 0;
    // The recording's own answer: all nine stats enchanted and all nine raised.
    let recorded_bits = base
        .creature_profile
        .clone()
        .and_then(|p| p.enchantment_bitfield)
        == Some(0x01FF_01FF);
    let all_raised = creature_row_colors(&mut c) == vec![examination::MOD_HIGH_FONT; 9];
    let raised = palette[usize::from(examination::MOD_HIGH_FONT)];
    let unknown = palette[usize::from(examination::UNKNOWN_FONT)];
    let raised_drawn = drawn_stat_rows(&mut c)
        .into_iter()
        .all(|(_, _, _, colour)| colour == raised);

    let mut failed = base.clone();
    failed.success_flag = 0;
    assess(&mut c, &mut peer, APPRAISED_PLAYER, &failed);

    let all_unknown = creature_row_colors(&mut c) == vec![examination::UNKNOWN_FONT; 9];
    let rows = drawn_stat_rows(&mut c);
    let nine_rows = rows.len() == 9;
    let values_unknown_labels_not = rows.iter().all(|(_, label_colour, _, value_colour)| {
        *value_colour == unknown && *label_colour != unknown
    });

    c.assert_behaviour(
        "examine.failed-assess.every-value-cell-takes-the-unknown-colour-and-no-label-does",
        move |_| {
            four_colours
                && succeeded
                && recorded_bits
                && all_raised
                && raised_drawn
                && all_unknown
                && nine_rows
                && values_unknown_labels_not
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_failed_assessment_draws_every_value_cell_in_the_unknown_colour() {
    scenario("a_failed_assessment_draws_every_value_cell_in_the_unknown_colour");
}

/// The **words** on a failed assessment are chosen separately from the colour, and the two do not
/// line up the way *"the ??? values"* suggests: the six attributes keep whatever numbers the
/// shard already described, health falls back to a bare percentage, and only stamina and mana
/// read as unknown -- while a body the shard described nothing of is unknown nine times.
pub fn a_failed_assessment_keeps_the_numbers_and_reduces_health_to_a_percentage() {
    use dereth_protocol::types::appraisal::CreatureAppraisalProfile;

    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    let creature = base
        .creature_profile
        .clone()
        .expect("the recorded answer carries a creature");
    let described = creature.attributes.is_some();

    let mut failed = base.clone();
    failed.success_flag = 0;
    assess(&mut c, &mut peer, APPRAISED_PLAYER, &failed);

    let rows = creature_rows(&mut c);
    let nine = rows.len() == 9;
    let six_keep_their_numbers = rows.iter().take(6).all(|(_, value)| value != "???");
    let health_is_a_percentage = rows[6].1.ends_with(" %") && !rows[6].1.contains('/');
    let the_other_two = rows[7].1 == "???" && rows[8].1 == "???";

    // The same answer with the creature block emptied. **The block itself has to stay**: the
    // pane forks on whether the thing is a creature at all, so an answer with no block is an
    // item pane and would leave these nine cells showing the last creature.
    let mut bare = failed.clone();
    if let Some(cp) = bare.creature_profile.as_mut() {
        cp.flags &= !CreatureAppraisalProfile::HAS_ATTRIBUTES;
        cp.attributes = None;
        cp.health = 0;
        cp.max_health = 0;
    }
    assess(&mut c, &mut peer, APPRAISED_PLAYER, &bare);
    let bare_rows = creature_rows(&mut c);
    let nine_unknowns = bare_rows.iter().filter(|(_, v)| v == "???").count() == 9;
    let still_the_failure_colour =
        creature_row_colors(&mut c) == vec![examination::UNKNOWN_FONT; 9];

    c.assert_behaviour(
        "examine.failed-assess.the-numbers-the-shard-described-are-kept-and-health-is-a-percentage",
        move |_| {
            described
                && nine
                && six_keep_their_numbers
                && health_is_a_percentage
                && the_other_two
                && nine_unknowns
                && still_the_failure_colour
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_failed_assessment_keeps_the_numbers_and_reduces_health_to_a_percentage() {
    scenario("a_failed_assessment_keeps_the_numbers_and_reduces_health_to_a_percentage");
}

/// A stat that is both enchanted and unassessed reads unknown and not enchanted: the failure is
/// decided before the question about enchantment is asked at all.
///
/// The same stats on a successful assessment take the enchantment colours, which is what makes
/// the first half a precedence and not an accident.
pub fn the_failure_colour_outranks_the_enchantment_colour() {
    use dereth_protocol::types::appraisal::CreatureAppraisalProfile;

    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    // Strength enchanted and raised, endurance enchanted and lowered, the maximum health raised.
    let bits = 0x1 | 0x1_0000 | 0x2 | 0x40 | 0x40_0000;
    let enchanted = |success: u32| {
        let mut p = base.clone();
        p.success_flag = success;
        if let Some(cp) = p.creature_profile.as_mut() {
            cp.flags |= CreatureAppraisalProfile::HAS_ENCHANTMENTS;
            cp.enchantment_bitfield = Some(bits);
        }
        p
    };

    assess(&mut c, &mut peer, APPRAISED_PLAYER, &enchanted(1));
    let ok = creature_row_colors(&mut c);
    let succeeded = ok[0] == examination::MOD_HIGH_FONT
        && ok[1] == examination::MOD_LOW_FONT
        // Not enchanted at all, so no colour of any kind.
        && ok[2] == 0
        // Health asks about its *maximum*, which is the one that was raised.
        && ok[6] == examination::MOD_HIGH_FONT;

    assess(&mut c, &mut peer, APPRAISED_PLAYER, &enchanted(0));
    let failed = creature_row_colors(&mut c) == vec![examination::UNKNOWN_FONT; 9];

    c.assert_behaviour(
        "examine.failed-assess.an-unassessed-stat-reads-unknown-even-when-it-is-enchanted",
        move |_| succeeded && failed,
    );
    c.shutdown();
}

#[test]
fn scenario_the_failure_colour_outranks_the_enchantment_colour() {
    scenario("the_failure_colour_outranks_the_enchantment_colour");
}

/// The two places a failure does **not** colour, asserted so that a later change cannot quietly
/// widen the first: the item pane's own unknown value line is drawn plain.
pub fn a_failed_assessment_leaves_the_item_panes_value_line_plain() {
    use dereth_protocol::types::appraisal::flags;

    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);
    let mut failed = base.clone();
    failed.success_flag = 0;
    failed.tables.ints = None;
    failed.flags &= !flags::INT;
    assess(&mut c, &mut peer, POTION, &failed);

    let starts_unknown = item_text(&mut c).starts_with("Value: ???");
    let plain = description_plain(&mut c);
    let w = exam_window(&mut c);
    let pane = exam_find(&mut c, w, examination::ITEM_DISPLAY_TEXT).expect("the description pane");
    let glyphs = painted(&mut c, pane);
    let drawn_plain = colors_of(&glyphs, "Value: ???") == vec![plain];

    c.assert_behaviour(
        "examine.failed-assess.the-item-panes-own-unknown-value-line-is-drawn-plain",
        move |_| starts_unknown && drawn_plain,
    );
    c.shutdown();
}

#[test]
fn scenario_a_failed_assessment_leaves_the_item_panes_value_line_plain() {
    scenario("a_failed_assessment_leaves_the_item_panes_value_line_plain");
}

// =============================================================================================
// examine.scroll.*, examine.enchanted.*
//
// Two things about the assessment window: the description pane's scrollbar, and the enchantment
// highlighting.
//
// **No recorded assessment in the corpus carries an enchantment bitfield at all**, so the
// highlighting scenario uses a built answer. Everything from the datagram onward is the client's
// own path.
// =============================================================================================

/// The scrollbar the description pane names, the plate behind it, the arrow at the bottom of the
/// bar and the thumb inside it.
const DESC_SCROLLBAR: ElementId = ElementId(0x1000_013D);
const DESC_SCROLLBAR_PLATE: ElementId = ElementId(0x1000_05F8);
const SCROLL_ARROW_DOWN: ElementId = ElementId(0x1000_0071);
const SCROLL_THUMB: ElementId = ElementId(1);

/// The thing these scenarios assess, and the last line of the description they give it: it sits
/// below the bottom edge of the pane and only a scroll brings it inside.
const GAUNTLETS_SUBJECT: ObjectId = ObjectId(0x7DA5_5047);
const MARKER: &str = "THE LAST LINE";

/// The three colours the shipped description pane declares.
const PLAIN: u32 = 0xFFFF_FFFF;
const RAISED: u32 = 0xFF00_FF00;
const LOWERED: u32 = 0xFFFF_0000;

/// A client, a shard, and a thing of the scenario's own to assess.
fn a_client_and_a_thing_to_assess() -> (HeadlessClient, dereth_testkit::Peer) {
    let (mut c, mut peer) = a_client_and_a_shard();
    let mut p = dereth_protocol::objects::ObjectCreatePayload {
        id: GAUNTLETS_SUBJECT,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut c,
        dereth_testkit::replay::OBJECT_QUEUE,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("the create encodes"),
    );
    c.tick(1);
    c.world_mut()
        .weenie_mut(GAUNTLETS_SUBJECT)
        .expect("the shard created the subject")
        .pwd
        .name = "Bronze Gauntlets".to_owned();
    (c, peer)
}

/// A whole numbers table, as the decoder produces one.
fn int_table(entries: Vec<(u32, i32)>) -> dereth_protocol::archive::PackedHash<u32, i32> {
    dereth_protocol::archive::PackedHash {
        table_size: 16,
        entries,
    }
}

/// An assessment whose description is longer than the pane, ending in [`MARKER`].
///
/// The lines are short on purpose: an answer whose string table does not fit one fragment is
/// refused by the wire, which is a thing this scenario would rather not be about.
fn a_long_description() -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut body = String::new();
    for i in 1..=24 {
        body.push_str(&format!("Line {i:02}.\n"));
    }
    body.push_str(MARKER);
    let mut p = AppraisalProfile {
        success_flag: 1,
        ..AppraisalProfile::default()
    };
    p.flags |= flags::INT;
    p.tables.ints = Some(int_table(vec![(0x13, 200), (5, 50)]));
    p.flags |= flags::STRING;
    p.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(0x10, body)],
    });
    p
}

/// The same thing with one line of description: the negative control for every claim about the
/// bar.
fn a_short_description() -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = AppraisalProfile {
        success_flag: 1,
        ..AppraisalProfile::default()
    };
    p.flags |= flags::INT;
    p.tables.ints = Some(int_table(vec![(0x13, 200), (5, 50)]));
    p.flags |= flags::STRING;
    p.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(0x10, "A pair of gauntlets.".to_owned())],
    });
    p
}

/// A piece of armour whose armour level was **raised** and whose slashing resistance was
/// **lowered**, with every one of its eight modifiers the same -- so the only thing separating the
/// three rows these claims read is which of them is enchanted.
fn enchanted_armour() -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    /// Which bit says a thing is enchanted, and which says it was raised rather than lowered.
    const ARMOUR_LEVEL_ENCHANTED: u32 = 0x1;
    const ARMOUR_LEVEL_RAISED: u32 = 0x1_0000;
    const SLASH_ENCHANTED: u32 = 0x2;

    let mut p = AppraisalProfile {
        success_flag: 1,
        ..AppraisalProfile::default()
    };
    p.flags |= flags::INT;
    // The armour level is the block's own gate: nothing of it is drawn without one.
    p.tables.ints = Some(int_table(vec![(0x13, 200), (5, 50), (0x1C, 20)]));
    p.flags |= flags::ARMOR_PROFILE;
    p.armor_profile = Some(dereth_protocol::types::appraisal::ArmorProfile {
        mod_vs_slash: 1.0,
        mod_vs_pierce: 1.0,
        mod_vs_bludgeon: 1.0,
        mod_vs_cold: 1.0,
        mod_vs_fire: 1.0,
        mod_vs_acid: 1.0,
        mod_vs_electric: 1.0,
        mod_vs_nether: 1.0,
    });
    p.flags |= flags::ARMOR_ENCHANTMENT;
    p.armor_enchantment = Some(ARMOUR_LEVEL_ENCHANTED | ARMOUR_LEVEL_RAISED | SLASH_ENCHANTED);
    p
}

/// The description pane itself, and the bar beside it.
fn description_pane(c: &mut HeadlessClient) -> ElemHandle {
    let w = exam_window(c);
    exam_find(c, w, examination::ITEM_DISPLAY_TEXT).expect("the description pane is in the layout")
}

fn description_bar(c: &mut HeadlessClient) -> ElemHandle {
    let w = exam_window(c);
    exam_find(c, w, DESC_SCROLLBAR).expect("the bar is in the layout")
}

/// Whether an element is drawn at all.
fn element_visible(c: &mut HeadlessClient, h: ElemHandle) -> bool {
    c.app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .node(h)
        .is_some_and(|n| n.region.flags.visible)
}

/// How tall the pane says its own content is, against how tall the pane is -- the two numbers the
/// bar's sizer compares to decide whether it has anything to do.
fn description_extent(c: &mut HeadlessClient) -> (i32, i32) {
    let h = description_pane(c);
    let app = c.app_mut();
    let view = app.ui().expect("the shell is up").ui.screen_box(h).height();
    let ui = &mut app.ui_mut().expect("the shell is up").ui;
    (
        ui.text_element_mut(h)
            .expect("a text element")
            .scroll
            .height,
        view,
    )
}

/// How far down the pane has been scrolled.
fn description_offset(c: &mut HeadlessClient) -> i32 {
    let h = description_pane(c);
    let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
    ui.text_element_mut(h).expect("a text element").scroll.y
}

/// Every character of the description whose composed cell lies **inside the pane's box** -- what a
/// player can actually read, with the scroll already applied.
fn readable_description(c: &mut HeadlessClient) -> String {
    let h = description_pane(c);
    let app = c.app_mut();
    let box_ = app.ui().expect("the shell is up").ui.screen_box(h);
    let ui = &mut app.ui_mut().expect("the shell is up").ui;
    let t = ui.text_element_mut(h).expect("a text element");
    t.compose(box_)
        .into_iter()
        .filter(|g| g.y >= box_.y0 && g.y < box_.y1)
        .filter_map(|g| char::from_u32(u32::from(g.ch)))
        .collect()
}

/// The centre of an element's box, which is where a hand puts the pointer.
fn centre_of(c: &mut HeadlessClient, h: ElemHandle) -> ScreenPoint {
    let b = c.app_mut().ui().expect("the shell is up").ui.screen_box(h);
    ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// **The scrollbar, from both sides.** A description that does not fit the pane shows the
/// bar; one that fits keeps it off the screen -- which is what makes the first half a measurement
/// and not a bar that is simply always there.
///
/// The bar is a real one and the strip beside it is only its backing plate: both are
/// read off the shipped layout first, so a failure in the layout reads as a failure in the layout.
pub fn the_description_pane_shows_a_bar_only_when_its_text_does_not_fit() {
    let (mut c, mut peer) = a_client_and_a_thing_to_assess();
    assess(&mut c, &mut peer, GAUNTLETS_SUBJECT, &a_short_description());

    let w = exam_window(&mut c);
    let plate = exam_find(&mut c, w, DESC_SCROLLBAR_PLATE).expect("the plate is in the layout");
    let b = description_bar(&mut c);
    let pane = description_pane(&mut c);
    let (plate_is_a_static, bar_is_a_bar) = {
        let ui = &c.app_mut().ui().expect("the shell is up").ui;
        (
            ui.node(plate).expect("a node").ty().0 == 0x03,
            ui.node(b).expect("a node").ty().0 == 0x0B,
        )
    };
    let bound = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        ui.text_element_mut(pane)
            .expect("a text element")
            .scroll
            .v_scrollbar
            == Some(DESC_SCROLLBAR)
    };

    let (short_content, short_view) = description_extent(&mut c);
    let short_fits = short_content <= short_view;
    let hidden = !element_visible(&mut c, b);

    assess(&mut c, &mut peer, GAUNTLETS_SUBJECT, &a_long_description());
    let (long_content, long_view) = description_extent(&mut c);
    let overflows = long_content > long_view;
    let long_bar = description_bar(&mut c);
    let shown = element_visible(&mut c, long_bar);

    c.assert_behaviour(
        "examine.scroll.the-description-pane-shows-a-bar-only-when-its-text-does-not-fit",
        move |_| {
            plate_is_a_static && bar_is_a_bar && bound && short_fits && hidden && overflows && shown
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_description_pane_shows_a_bar_only_when_its_text_does_not_fit() {
    scenario("the_description_pane_shows_a_bar_only_when_its_text_does_not_fit");
}

/// **The gesture, both of the ways a player makes it.** A press on the arrow at the bottom of the
/// bar, and a drag of the thumb to the bottom of its track, each bring the end of a description
/// that overflows into view.
///
/// Each half begins by asserting that the last line is *not* readable, so an instrument pointed at
/// a pane that never overflowed reports that rather than passing.
pub fn the_bar_really_scrolls_the_description() {
    let (mut c, mut peer) = a_client_and_a_thing_to_assess();
    assess(&mut c, &mut peer, GAUNTLETS_SUBJECT, &a_long_description());

    let before_arrow = readable_description(&mut c);
    let arrow = {
        let b = description_bar(&mut c);
        exam_find(&mut c, b, SCROLL_ARROW_DOWN).expect("the bar's own arrow")
    };
    let at = centre_of(&mut c, arrow);
    for _ in 0..40 {
        c.when(Player::Click(Target::Point(at)));
    }
    let arrow_offset = description_offset(&mut c);
    let after_arrow = readable_description(&mut c);

    // A fresh client for the drag: the pane this one is looking at is already at the bottom.
    let (mut c2, mut peer2) = a_client_and_a_thing_to_assess();
    assess(
        &mut c2,
        &mut peer2,
        GAUNTLETS_SUBJECT,
        &a_long_description(),
    );
    let before_drag = readable_description(&mut c2);
    let bar = description_bar(&mut c2);
    let thumb = exam_find(&mut c2, bar, SCROLL_THUMB).expect("the bar's thumb");
    let from = centre_of(&mut c2, thumb);
    let track = c2
        .app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .screen_box(bar);
    c2.when(Player::Drag {
        from: Target::Point(from),
        to: Target::Point(ScreenPoint::new(from.x, track.y1 - 2)),
        hold_frames: 1,
    });
    let drag_offset = description_offset(&mut c2);
    let after_drag = readable_description(&mut c2);
    c2.shutdown();

    c.assert_behaviour(
        "examine.scroll.the-bar-brings-the-end-of-a-long-description-into-view",
        move |_| {
            !before_arrow.contains(MARKER)
                && arrow_offset > 0
                && after_arrow.contains(MARKER)
                && !before_drag.contains(MARKER)
                && drag_offset > 0
                && after_drag.contains(MARKER)
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_bar_really_scrolls_the_description() {
    scenario("the_bar_really_scrolls_the_description");
}

/// **Highlighting.** A line about something the shard says was raised is drawn green,
/// one about something lowered red, and everything else plain -- three colours on one pane at
/// once, out of the pane's own array of three, and one colour when nothing is enchanted.
///
/// The array is read off the live element first, so the colours below are the shipped ones and not
/// numbers written down here.
pub fn a_raised_line_is_green_a_lowered_one_red_and_the_rest_plain() {
    let (mut c, mut peer) = a_client_and_a_thing_to_assess();
    assess(&mut c, &mut peer, GAUNTLETS_SUBJECT, &enchanted_armour());

    let pane = description_pane(&mut c);
    let (declared, fonts) = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let t = ui.text_element_mut(pane).expect("a text element");
        (t.font_colors.clone(), t.fonts.clone())
    };
    let shipped = declared == vec![PLAIN, RAISED, LOWERED]
        && fonts == vec![dereth_primitives::DataId(0x4000_0001)];

    let glyphs = painted(&mut c, pane);
    let coloured = colors_of(&glyphs, "Armor Level: 20") == vec![RAISED]
        && colors_of(&glyphs, "Slashing: Average  (20)") == vec![LOWERED]
        // Neither bit is set for this one, so it is plain.
        && colors_of(&glyphs, "Piercing: Average  (20)") == vec![PLAIN]
        && colors_of(&glyphs, "Value: 200") == vec![PLAIN]
        && colors_of(&glyphs, "Burden: 50") == vec![PLAIN];
    let mut distinct: Vec<u32> = glyphs.iter().map(|(_, col)| *col).collect();
    distinct.sort_unstable();
    distinct.dedup();
    let three_at_once = distinct.len() == 3;

    // The same armour with nothing enchanted: one colour, and the armour level plain.
    let mut plain = enchanted_armour();
    plain.flags &= !dereth_protocol::types::appraisal::flags::ARMOR_ENCHANTMENT;
    plain.armor_enchantment = None;
    assess(&mut c, &mut peer, GAUNTLETS_SUBJECT, &plain);
    let pane = description_pane(&mut c);
    let glyphs = painted(&mut c, pane);
    let mut distinct: Vec<u32> = glyphs.iter().map(|(_, col)| *col).collect();
    distinct.sort_unstable();
    distinct.dedup();
    let one_when_nothing_is =
        distinct == vec![PLAIN] && colors_of(&glyphs, "Armor Level: 20") == vec![PLAIN];

    c.assert_behaviour(
        "examine.enchanted.a-raised-line-is-green-a-lowered-one-red-and-everything-else-plain",
        move |_| shipped && coloured && three_at_once && one_when_nothing_is,
    );
    c.shutdown();
}

#[test]
fn scenario_a_raised_line_is_green_a_lowered_one_red_and_the_rest_plain() {
    scenario("a_raised_line_is_green_a_lowered_one_red_and_the_rest_plain");
}

// =============================================================================================
// examine.window.*
//
// The identify button raises the identify window: a chain of four links -- click, question,
// answer, pane -- and a window that never opens can be missing any one of them.
//
// Every recorded assessment filling the pane from its own body is what the twenty-odd `examine.*`
// scenarios above this one cover, each with one recorded answer and one claim.
// =============================================================================================

/// The second recorded thing in the assessment recording, so that a claim about *which* object the
/// pane was waiting for has two to choose between.
const OTHER_SUBJECT: ObjectId = ObjectId(0x8000_09B4);

/// Whether the assessment window is on the screen.
fn exam_window_visible(c: &mut HeadlessClient) -> bool {
    let h = exam_window(c);
    c.app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .node(h)
        .expect("a live node")
        .region
        .flags
        .visible
}

/// Put the window away, as the player closing it does.
fn close_exam_window(c: &mut HeadlessClient) {
    let h = exam_window(c);
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .set_visible(h, false);
}

/// Create one recorded thing in this client's world and hand back the answer the shard recorded
/// about it -- without asking about it, which is what these two claims are about.
fn a_recorded_thing(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    session: &str,
    object: ObjectId,
) -> AppraisalProfile {
    let (create, _blob, profile) = recorded_object(session, object);
    peer.send(c, dereth_testkit::replay::OBJECT_QUEUE, create);
    c.tick(1);
    assert!(
        c.view().world().weenie(object).is_some(),
        "the recorded create is what puts {object:?} in this client's world"
    );
    profile
}

/// Tell the pane it is waiting for `object`, the way the request does.
fn await_the_answer_for(c: &mut HeadlessClient, object: ObjectId) {
    let (_, screen) = gameplay_screen(c.app_mut());
    screen.examination.examine_object(object);
}

/// Deliver one assessment through the shard, without asking for it first.
fn deliver_the_answer(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    object: ObjectId,
    profile: &AppraisalProfile,
) {
    peer.event(
        c,
        &dereth_protocol::objects::ItemSetAppraiseInfo {
            object,
            profile: profile.clone(),
        },
    );
    c.tick(2);
}

/// **The guard, both ways.** An answer the pane never asked for opens nothing, an answer about a
/// different thing from the one it asked about opens nothing -- and an answer about the *same*
/// thing, arriving again after the player has put the window away, refills it without putting it
/// back on the screen.
///
/// The last half is the one that needs saying: while something is being fought the client asks
/// again about four times a second, and a pane that reopened on each answer would fight the player
/// for the screen.
pub fn an_answer_the_pane_did_not_ask_for_opens_nothing() {
    let (mut c, mut peer) = a_client_and_a_shard();
    let first = a_recorded_thing(&mut c, &mut peer, EXAMINE_SESSION, POTION);
    let second = a_recorded_thing(&mut c, &mut peer, EXAMINE_SESSION, OTHER_SUBJECT);

    // Nothing was asked for at all.
    deliver_the_answer(&mut c, &mut peer, POTION, &first);
    let applied_with_nothing_asked = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.replies_applied
    };
    let down_with_nothing_asked = !exam_window_visible(&mut c);

    // A different thing was asked about.
    await_the_answer_for(&mut c, OTHER_SUBJECT);
    deliver_the_answer(&mut c, &mut peer, POTION, &first);
    let applied_with_the_wrong_one = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.replies_applied
    };
    let down_with_the_wrong_one = !exam_window_visible(&mut c);

    // The one it did ask about.
    deliver_the_answer(&mut c, &mut peer, OTHER_SUBJECT, &second);
    let up = exam_window_visible(&mut c);
    let (opened, current, awaiting) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        (
            screen.examination.opened,
            screen.examination.current,
            screen.examination.awaiting,
        )
    };

    // The player puts it away, and the same answer comes round again.
    close_exam_window(&mut c);
    c.tick(1);
    deliver_the_answer(&mut c, &mut peer, OTHER_SUBJECT, &second);
    let (refilled, still_opened_once) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        (
            screen.examination.replies_applied,
            screen.examination.opened,
        )
    };
    let stayed_down = !exam_window_visible(&mut c);

    c.assert_behaviour(
        "examine.window.an-answer-the-pane-did-not-ask-for-opens-nothing-and-a-repeat-does-not-reopen-it",
        move |_| {
            applied_with_nothing_asked == 0
                && down_with_nothing_asked
                && applied_with_the_wrong_one == 0
                && down_with_the_wrong_one
                && up
                && opened == 1
                && current == Some(OTHER_SUBJECT)
                // The question is consumed by the answer to it.
                && awaiting.is_none()
                && refilled == 2
                && still_opened_once == 1
                && stayed_down
        },
    );
    c.shutdown();
}

#[test]
fn scenario_an_answer_the_pane_did_not_ask_for_opens_nothing() {
    scenario("an_answer_the_pane_did_not_ask_for_opens_nothing");
}

/// **The click that starts the chain.** Pressing the identify button with something selected is
/// what makes the pane wait for that thing's answer -- the middle link a window that never opens
/// is most likely missing -- and the answer then opens the window: click, question, answer, pane.
pub fn the_identify_button_is_what_starts_the_chain() {
    let (mut c, mut peer) = a_client_and_a_shard();
    let profile = a_recorded_thing(&mut c, &mut peer, EXAMINE_SESSION, POTION);

    let nothing_awaited_before = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.awaiting.is_none()
    };

    // The real gesture: the thing is under the pointer and the shipped button is pressed.
    examine(&mut c, POTION);

    let (awaited, asked) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        (
            screen.examination.awaiting,
            screen.examination.replies_applied,
        )
    };
    let still_down = !exam_window_visible(&mut c);

    deliver_the_answer(&mut c, &mut peer, POTION, &profile);
    let up = exam_window_visible(&mut c);

    c.assert_behaviour(
        "examine.window.the-identify-button-is-what-makes-the-pane-wait-for-an-answer",
        move |_| {
            nothing_awaited_before && awaited == Some(POTION) && asked == 0 && still_down && up
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_identify_button_is_what_starts_the_chain() {
    scenario("the_identify_button_is_what_starts_the_chain");
}

// =============================================================================================
// examine.character.*, examine.creature.* -- the rest of the pane
//
// The recorded player's own body is the oracle for the armour rows and the three lines beside
// them: the recording carries them and nothing here builds them. The rows the corpus has **no**
// body for -- society, allegiance, the ratings and the seven tail rows -- are the recording's own
// answer with one property added to it, which is the same shape every other scenario in this file
// uses: the shard's answer goes in at the wire and everything after it is the client's.
// =============================================================================================

/// The recorded player's own armour, as the pane writes it out.
const RECORDED_ARMOUR: [&str; 3] = ["AL: 170/150/150", "AL: 150/150/170", "AL: 150/150/170"];

/// The three colours a row of the character pane's extra list declares.
const MISC_TABLE: [u32; 3] = [0xFFFF_FFFF, 0xFF00_FF00, 0xFFFF_0000];

/// The same profile with these strings **added to** whatever it already carries.
fn with_more_strings(base: &AppraisalProfile, pairs: &[(u32, &str)]) -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = base.clone();
    let mut entries = p
        .tables
        .strings
        .clone()
        .map(|h| h.entries)
        .unwrap_or_default();
    for (k, v) in pairs {
        entries.retain(|(ek, _)| ek != k);
        entries.push((*k, (*v).to_owned()));
    }
    p.flags |= flags::STRING;
    p.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 32,
        entries,
    });
    p
}

/// The same profile with no armour block at all.
fn without_base_armour(base: &AppraisalProfile) -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = base.clone();
    p.flags &= !flags::BASE_ARMOR;
    p.base_armor = None;
    p
}

/// What the character pane is showing, read back off the panel after an answer.
#[derive(Debug, Clone)]
struct CharPane {
    sub: Option<examination::ExamineSubUi>,
    rows: Vec<examination::MiscRow>,
    drawn: u32,
    heritage: Option<String>,
    profession: Option<String>,
    pk: Option<String>,
    allegiance: Option<String>,
    title: Option<String>,
}

fn char_pane(c: &mut HeadlessClient) -> CharPane {
    let (_, screen) = gameplay_screen(c.app_mut());
    let p = &screen.examination;
    CharPane {
        sub: p.active,
        rows: p.misc_row_text.clone(),
        drawn: p.misc_rows_drawn,
        heritage: p.heritage_text.clone(),
        profession: p.profession_text.clone(),
        pk: p.pk_status_text.clone(),
        allegiance: p.allegiance_name_text.clone(),
        title: p.title_text.clone(),
    }
}

/// The pane's rows as `(label, value)`, which is how every claim below reads them.
fn row_pairs(rows: &[examination::MiscRow]) -> Vec<(String, String)> {
    rows.iter()
        .map(|r| (r.label.clone(), r.value.clone()))
        .collect()
}

/// Assess `object` again with `profile`, and hand back what the pane then shows.
fn reassess(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    object: ObjectId,
    profile: &AppraisalProfile,
) -> CharPane {
    assess(c, peer, object, profile);
    char_pane(c)
}

/// One live row of the pane's extra list, with the glyph colours of both its cells and the
/// colours each cell's own template declares.
#[derive(Debug, Clone)]
struct DrawnMiscRow {
    label: String,
    value: String,
    label_colours: Vec<u32>,
    value_colours: Vec<u32>,
    label_declared: Vec<u32>,
    value_declared: Vec<u32>,
}

/// Every row of the extra list after layout and composition -- the **draw data**, not the panel's
/// own copy of the selector under test.
fn drawn_misc_rows(c: &mut HeadlessClient) -> Vec<DrawnMiscRow> {
    let window = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.window.expect("the window is bound")
    };
    let list = c
        .app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .get_child_recursive(window, examination::MISC_LIST)
        .expect("the character pane's extra list is in the shipped layout");
    let rows = c.app_mut().ui().expect("the shell is up").ui.children(list);
    let mut out = Vec::new();
    for row in rows {
        let pair = {
            let ui = &c.app_mut().ui().expect("the shell is up").ui;
            (
                ui.get_child_recursive(row, examination::ROW_LABEL),
                ui.get_child_recursive(row, examination::ROW_VALUE),
            )
        };
        let (Some(label), Some(value)) = pair else {
            continue;
        };
        let cell = |c: &mut HeadlessClient, h: ElemHandle| {
            let app = c.app_mut();
            let box_ = app.ui().expect("the shell is up").ui.screen_box(h);
            let t = app
                .ui_mut()
                .expect("the shell is up")
                .ui
                .text_element_mut(h)
                .expect("a row cell is text");
            let text = t.glyphs.inq_text(false);
            let declared = t.font_colors.clone();
            let colours: Vec<u32> = t.compose(box_).into_iter().map(|g| g.color).collect();
            (text, colours, declared)
        };
        let (label, label_colours, label_declared) = cell(c, label);
        let (value, value_colours, value_declared) = cell(c, value);
        out.push(DrawnMiscRow {
            label,
            value,
            label_colours,
            value_colours,
            label_declared,
            value_declared,
        });
    }
    out
}

/// Tell the client which society the **player** belongs to, as a real update through the shard.
fn set_viewer_society(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    sequence: u8,
    value: i32,
) {
    let m = dereth_protocol::qualities::QualitiesPrivateUpdateInt(
        dereth_protocol::qualities::PrivateUpdate {
            sequence,
            property_id: 0x119,
            value,
        },
    );
    peer.event(c, &m);
    c.tick(2);
}

/// **The recorded player's own body.** The armour rows are the recording's nine numbers, the three
/// lines beside them are its gender, its heritage and its character title, and the pane's list of
/// rows it cannot draw is empty because they were drawn.
pub fn the_character_pane_rows_are_the_shards_own_numbers() {
    let (mut c, _peer, profile) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    let carries = profile.creature_profile.is_some()
        && profile.base_armor == Some([170, 150, 150, 150, 150, 170, 150, 150, 170]);

    let p = char_pane(&mut c);
    let got = row_pairs(&p.rows);
    let rows_read = got.len() == 5
        && p.rows[0] == examination::MiscRow::default()
        && got[1] == ("Head/Chest/Groin".to_owned(), RECORDED_ARMOUR[0].to_owned())
        && got[2] == ("Bicep/Wrist/Hand".to_owned(), RECORDED_ARMOUR[1].to_owned())
        && got[3] == ("Thigh/Shin/Foot".to_owned(), RECORDED_ARMOUR[2].to_owned())
        && got[4]
            == (
                examination::UNENCHANTABLE_FOOTNOTE.to_owned(),
                String::new(),
            );

    // And every one of them reached an element of the list, not only the panel's own vector.
    let all_drawn = p.drawn == 5;

    let beside = p.sub == Some(examination::ExamineSubUi::Char)
        && p.heritage.as_deref() == Some("Female Sho")
        && p.profession.as_deref() == Some("Life Caster")
        && p.pk.as_deref() == Some("Non-Player Killer")
        // Nothing said the character has an allegiance, so the line is cleared and not left
        // holding the last one.
        && p.allegiance.is_none();

    let nothing_left = examination::CREATURE_MISC_NOT_IMPLEMENTED.is_empty();

    c.assert_behaviour(
        "examine.character.the-armour-rows-are-the-shards-own-numbers-and-the-three-lines-beside-them",
        move |_| carries && rows_read && all_drawn && beside && nothing_left,
    );
    c.shutdown();
}

#[test]
fn scenario_the_character_pane_rows_are_the_shards_own_numbers() {
    scenario("the_character_pane_rows_are_the_shards_own_numbers");
}

/// The society row names the society and the band the rank falls in, drops the band when there is
/// no rank at all, and says as much when the society is one it has no name for -- and it is drawn
/// at the top of the list.
pub fn the_society_row_names_the_society_and_its_rank_band() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    // The recording's own character belongs to no society, so the row must be absent first.
    let absent_first = !char_pane(&mut c).rows.iter().any(|r| r.label == "Society:");

    let p = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_ints(&base, &[(0x119, 2), (0x120, 350)]),
    );
    let banded = p
        .rows
        .iter()
        .find(|r| r.label == "Society:")
        .is_some_and(|r| r.value == "Eldrytch Web ~ Knight" && r.color == 0);
    let drawn_first = p.rows.first().is_some_and(|r| r.label == "Society:");

    // A society with no rank at all: the band is dropped rather than printed as nothing.
    let p = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_ints(&base, &[(0x119, 1)]),
    );
    let unranked = p
        .rows
        .iter()
        .find(|r| r.label == "Society:")
        .is_some_and(|r| r.value == "Celestial Hand");

    // A society the pane has no name for says so, rather than drawing an empty row.
    let p = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_ints(&base, &[(0x119, 0x10)]),
    );
    let unknown = p
        .rows
        .iter()
        .find(|r| r.label == "Society:")
        .is_some_and(|r| r.value == "???");

    c.assert_behaviour(
        "examine.character.the-society-row-names-the-society-and-the-band-its-rank-falls-in",
        move |_| absent_first && banded && drawn_first && unranked && unknown,
    );
    c.shutdown();
}

#[test]
fn scenario_the_society_row_names_the_society_and_its_rank_band() {
    scenario("the_society_row_names_the_society_and_its_rank_band");
}

/// The society row is coloured by the **viewer's own** society: plain when they have none, the
/// row template's green for a fellow member and its red for a rival -- read off the glyphs each
/// cell composed, and off the colours each cell's own template declares.
///
/// Both cells take the same choice, so both have to declare and resolve the whole table: a label
/// that stayed white while its value went red would be a half-coloured row.
pub fn the_society_row_is_coloured_by_the_viewers_own_society() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    // The viewer has to have been described before anything can be said about their own society.
    peer.event(
        &mut c,
        &dereth_protocol::login::LoginPlayerDescription::default(),
    );
    c.tick(2);

    let target = with_ints(&base, &[(0x119, 1), (0x11F, 175)]);
    let look = |c: &mut HeadlessClient, peer: &mut dereth_testkit::Peer| -> DrawnMiscRow {
        let p = reassess(c, peer, APPRAISED_PLAYER, &target);
        assert_eq!(
            p.drawn as usize,
            p.rows.len(),
            "every row the pane worked out reached the real list"
        );
        drawn_misc_rows(c)
            .into_iter()
            .find(|r| r.label == "Society:")
            .expect("the society row is a live child of the list")
    };

    let neutral = look(&mut c, &mut peer);
    let plain = neutral.value == "Celestial Hand ~ Adept"
        && neutral.label_declared == MISC_TABLE
        && neutral.value_declared == MISC_TABLE
        && !neutral.label_colours.is_empty()
        && neutral
            .label_colours
            .iter()
            .all(|col| *col == MISC_TABLE[0])
        && !neutral.value_colours.is_empty()
        && neutral
            .value_colours
            .iter()
            .all(|col| *col == MISC_TABLE[0]);

    set_viewer_society(&mut c, &mut peer, 1, 1);
    let same = look(&mut c, &mut peer);
    let fellow = !same.label_colours.is_empty()
        && !same.value_colours.is_empty()
        && same.label_colours.iter().all(|col| *col == MISC_TABLE[1])
        && same.value_colours.iter().all(|col| *col == MISC_TABLE[1]);

    set_viewer_society(&mut c, &mut peer, 2, 2);
    let rival_row = look(&mut c, &mut peer);
    let rival = !rival_row.label_colours.is_empty()
        && !rival_row.value_colours.is_empty()
        && rival_row
            .label_colours
            .iter()
            .all(|col| *col == MISC_TABLE[2])
        && rival_row
            .value_colours
            .iter()
            .all(|col| *col == MISC_TABLE[2]);

    c.assert_behaviour(
        "examine.character.the-society-row-is-coloured-by-the-viewers-own-society",
        move |_| plain && fellow && rival,
    );
    c.shutdown();
}

#[test]
fn scenario_the_society_row_is_coloured_by_the_viewers_own_society() {
    scenario("the_society_row_is_coloured_by_the_viewers_own_society");
}

/// The allegiance rows fork on the two titles: a vassal with a monarch and a different patron gets
/// two rows, one whose monarch *is* their patron gets one row naming both, and a monarch gets a
/// count of followers with the singular and the plural told apart. A character of no rank gets
/// none of it, and the allegiance name beside the rows is cleared rather than left holding the
/// last character's.
pub fn the_allegiance_rows_fork_on_the_two_titles() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    let vassal = with_more_strings(
        &with_ints(&base, &[(0x1E, 3)]),
        &[
            (0x2F, "The Lost Light"),
            (0x15, "High King Al"),
            (0x23, "Baron Bob"),
        ],
    );
    let p = reassess(&mut c, &mut peer, APPRAISED_PLAYER, &vassal);
    let got = row_pairs(&p.rows);
    let two_rows = got.contains(&("Monarch:".to_owned(), "High King Al".to_owned()))
        && got.contains(&("Patron:".to_owned(), "Baron Bob".to_owned()))
        && !got.iter().any(|(l, _)| l == "Monarch/Patron:")
        && p.allegiance.as_deref() == Some("The Lost Light");

    let direct = with_more_strings(
        &with_ints(&base, &[(0x1E, 2)]),
        &[(0x15, "High King Al"), (0x23, "High King Al")],
    );
    let got = row_pairs(&reassess(&mut c, &mut peer, APPRAISED_PLAYER, &direct).rows);
    let one_row = got.contains(&("Monarch/Patron:".to_owned(), "High King Al".to_owned()));

    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &with_ints(&base, &[(0x1E, 9), (0x23, 1)]),
        )
        .rows,
    );
    let one_follower = got.contains(&("Alleg. Monarch:".to_owned(), "1 Follower".to_owned()));
    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &with_ints(&base, &[(0x1E, 9), (0x23, 12)]),
        )
        .rows,
    );
    let many_followers = got.contains(&("Alleg. Monarch:".to_owned(), "12 Followers".to_owned()));

    // No rank at all, which is the same thing as rank nothing: the whole block is skipped and the
    // name element is cleared.
    let p = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_more_strings(&base, &[(0x2F, "The Lost Light")]),
    );
    let got = row_pairs(&p.rows);
    let none = !got
        .iter()
        .any(|(l, _)| l.starts_with("Monarch") || l.starts_with("Alleg"))
        && p.allegiance.is_none();

    c.assert_behaviour(
        "examine.character.the-allegiance-rows-fork-on-whether-the-monarch-is-also-the-patron",
        move |_| two_rows && one_row && one_follower && many_followers && none,
    );
    c.shutdown();
}

#[test]
fn scenario_the_allegiance_rows_fork_on_the_two_titles() {
    scenario("the_allegiance_rows_fork_on_the_two_titles");
}

/// An allegiance rank puts its own title in front of the character's name in the window's title
/// bar -- and a rank there is no title for leaves the bare name.
pub fn an_allegiance_rank_puts_its_title_in_front_of_the_name() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    let plain = char_pane(&mut c).title.expect("the window has a title");
    let filled_first = !plain.is_empty();

    let titled = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_ints(&base, &[(0x1E, 3)]),
    )
    .title
    .expect("the window has a title");
    let prefixed = titled != plain && titled.ends_with(&plain) && titled.len() > plain.len();

    // A rank outside the range the titles cover: the bare name stands.
    let untitled = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_ints(&base, &[(0x1E, 44)]),
    )
    .title;
    let bare = untitled.as_deref() == Some(plain.as_str());

    c.assert_behaviour(
        "examine.character.an-allegiance-rank-puts-its-title-in-front-of-the-name",
        move |_| filled_first && prefixed && bare,
    );
    c.shutdown();
}

#[test]
fn scenario_an_allegiance_rank_puts_its_title_in_front_of_the_name() {
    scenario("an_allegiance_rank_puts_its_title_in_front_of_the_name");
}

/// A body part nothing can be cast on is marked with a star and its armour reads as what is left
/// -- and a character the shard sent no armour block for loses the three rows and the blank line
/// above them, keeping only the footnote, which sits outside every guard.
pub fn an_unenchantable_body_part_is_marked_with_a_star() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    let mut starred = base.clone();
    starred.base_armor = Some([170 + 9999, 150, 150, 150, 150, 170, 150, 150, 9999]);
    let got = row_pairs(&reassess(&mut c, &mut peer, APPRAISED_PLAYER, &starred).rows);
    let marked = got.contains(&("Head/Chest/Groin".to_owned(), "AL: *170/150/150".to_owned()))
        // Exactly the marker and nothing else is still marked, and reads as nothing left.
        && got.contains(&("Thigh/Shin/Foot".to_owned(), "AL: 150/150/*0".to_owned()));

    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &without_base_armour(&base),
        )
        .rows,
    );
    let only_the_footnote = got
        == vec![(
            examination::UNENCHANTABLE_FOOTNOTE.to_owned(),
            String::new(),
        )];

    c.assert_behaviour(
        "examine.character.a-body-part-nothing-can-be-cast-on-is-marked-and-reads-what-is-left",
        move |_| marked && only_the_footnote,
    );
    c.shutdown();
}

#[test]
fn scenario_an_unenchantable_body_part_is_marked_with_a_star() {
    scenario("an_unenchantable_body_part_is_marked_with_a_star");
}

/// The three rating groups draw the pairs the pane prints and no others: the number that opens a
/// group is a guard and is never itself drawn, and one of the numbers the pane reads is read and
/// then never printed at all.
pub fn the_rating_groups_draw_the_pairs_the_pane_prints() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    // The first group alone, opened by a number that is a guard and never drawn itself.
    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &with_ints(&base, &[(0x139, 7)]),
        )
        .rows,
    );
    // The group is opened and its pair reads nothing, which is the guard not being drawn: the
    // number that opened it is seven and the row says nothing of the sort.
    let guard_only = got.contains(&("Dmg/CritDmg".to_owned(), "Rating: 0/0".to_owned()))
        && !got
            .iter()
            .any(|(l, v)| l.starts_with("Dmg") && v != "Rating: 0/0");

    let all = with_ints(
        &base,
        &[
            (0x133, 11),
            (0x13A, 22),
            (0x134, 33),
            (0x13C, 44),
            (0x15E, 55),
            (0x15F, 66),
            (0x143, 77),
        ],
    );
    let got = row_pairs(&reassess(&mut c, &mut peer, APPRAISED_PLAYER, &all).rows);
    let three_groups = got.contains(&("Dmg/CritDmg".to_owned(), "Rating: 11/22".to_owned()))
        && got.contains(&("Dmg/CritDmg".to_owned(), "Resist: 33/44".to_owned()))
        && got.contains(&("DoT/Life:".to_owned(), "Resist: 55/66".to_owned()))
        // ...and the one the pane reads and never draws.
        && !got.iter().any(|(_, v)| v.contains("77"));

    c.assert_behaviour(
        "examine.character.the-rating-groups-draw-the-pairs-the-pane-prints-and-no-others",
        move |_| guard_only && three_groups,
    );
    c.shutdown();
}

#[test]
fn scenario_the_rating_groups_draw_the_pairs_the_pane_prints() {
    scenario("the_rating_groups_draw_the_pairs_the_pane_prints");
}

/// The creature pane shares the same list and draws the same rating rows, with no footnote of its
/// own -- and a creature the shard sent no rating above nothing for draws no rows at all.
pub fn the_creature_pane_draws_the_same_rating_rows_without_the_footnote() {
    let (mut c, mut peer, golem) = assessing_the_recorded(EXAMINE_SESSION, GOLEM);
    let p = char_pane(&mut c);
    let creature_pane = p.sub == Some(examination::ExamineSubUi::Creature);
    // The recorded creature carries no rating above nothing, and the creature pane has no
    // footnote, so it draws nothing at all here.
    let empty = p.rows.is_empty();

    let p = reassess(
        &mut c,
        &mut peer,
        GOLEM,
        &with_ints(&golem, &[(0x133, 5), (0x13A, 6)]),
    );
    let got = row_pairs(&p.rows);
    let drawn =
        got == vec![
            (String::new(), String::new()),
            ("Dmg/CritDmg".to_owned(), "Rating: 5/6".to_owned()),
            (String::new(), String::new()),
        ] && p.drawn == 3;

    c.assert_behaviour(
        "examine.creature.the-creature-pane-draws-the-same-rating-rows-without-the-footnote",
        move |_| creature_pane && empty && drawn,
    );
    c.shutdown();
}

#[test]
fn scenario_the_creature_pane_draws_the_same_rating_rows_without_the_footnote() {
    scenario("the_creature_pane_draws_the_same_rating_rows_without_the_footnote");
}

/// The seven rows at the foot of the character pane, each with the thing behind it -- and the two
/// of them whose keys are the same number in two different tables really are two rows, which is
/// the only way to believe either.
pub fn the_tail_rows_are_the_seven_the_pane_lists() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    let bare = without_base_armour(&base);

    let full = with_more_strings(
        &with_ints(
            &bare,
            &[
                (0x7D, 3_723),
                (0xB5, 1_600),
                (0xC0, 42),
                (0x2B, 7),
                (0x106, 17),
            ],
        ),
        &[(0x0A, "The Lost Light"), (0x2B, "4/13/2026 7:14:52 PM")],
    );
    let got = row_pairs(&reassess(&mut c, &mut peer, APPRAISED_PLAYER, &full).rows);
    let want: Vec<(String, String)> = [
        ("Fellowship:", "The Lost Light"),
        ("Arrived in Dereth:", "4/13/2026 7:14:52 PM"),
        ("Time in Dereth:", "1h 2m 3s"),
        ("Chess Rank:", "1600"),
        ("Fishing Skill:", "42"),
        ("Deaths:", "7"),
        ("Titles Earned:", "17"),
        (examination::UNENCHANTABLE_FOOTNOTE, ""),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_owned(), b.to_owned()))
    .collect();
    let in_order = got == want;

    // Never having died is a sentence and not a number...
    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &with_ints(&bare, &[(0x2B, 0)]),
        )
        .rows,
    );
    let never_died = got.contains(&("Deaths:".to_owned(), "Has never died".to_owned()))
        // ...and the same key number in the other table is a different row entirely.
        && !got.iter().any(|(l, _)| l == "Arrived in Dereth:");

    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &with_more_strings(&bare, &[(0x2B, "4/13/2026 7:14:52 PM")]),
        )
        .rows,
    );
    let the_other_way = got.contains(&(
        "Arrived in Dereth:".to_owned(),
        "4/13/2026 7:14:52 PM".to_owned(),
    )) && !got.iter().any(|(l, _)| l == "Deaths:");

    c.assert_behaviour(
        "examine.character.the-seven-tail-rows-each-draw-the-thing-behind-them",
        move |_| in_order && never_died && the_other_way,
    );
    c.shutdown();
}

#[test]
fn scenario_the_tail_rows_are_the_seven_the_pane_lists() {
    scenario("the_tail_rows_are_the_seven_the_pane_lists");
}

/// **The whole pane at once, in the order it writes it** -- so a row with the right words in the
/// wrong place is still a failure -- and every one of them reaching a real element of the list.
pub fn every_character_row_draws_in_the_panes_own_order() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    let everything = with_more_strings(
        &with_ints(
            &base,
            &[
                (0x119, 4),
                (0x121, 1_200),
                (0x1E, 5),
                (0x133, 11),
                (0x13A, 22),
                (0x7D, 90),
                (0xB5, 1_600),
                (0xC0, 42),
                (0x2B, 3),
                (0x106, 17),
            ],
        ),
        &[
            (0x2F, "The Lost Light"),
            (0x15, "High King Al"),
            (0x23, "Baron Bob"),
            (0x0A, "The Second Light"),
        ],
    );
    let p = reassess(&mut c, &mut peer, APPRAISED_PLAYER, &everything);
    let got = row_pairs(&p.rows);
    let want: Vec<(String, String)> = [
        ("Society:", "Radiant Blood ~ Master"),
        ("Monarch:", "High King Al"),
        ("Patron:", "Baron Bob"),
        ("", ""),
        ("Head/Chest/Groin", RECORDED_ARMOUR[0]),
        ("Bicep/Wrist/Hand", RECORDED_ARMOUR[1]),
        ("Thigh/Shin/Foot", RECORDED_ARMOUR[2]),
        ("", ""),
        ("Dmg/CritDmg", "Rating: 11/22"),
        ("", ""),
        ("Fellowship:", "The Second Light"),
        ("Time in Dereth:", "1m 30s"),
        ("Chess Rank:", "1600"),
        ("Fishing Skill:", "42"),
        ("Deaths:", "3"),
        ("Titles Earned:", "17"),
        (examination::UNENCHANTABLE_FOOTNOTE, ""),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_owned(), b.to_owned()))
    .collect();
    let in_order = got == want;
    let all_drawn = p.drawn as usize == p.rows.len();
    let nothing_left = examination::CREATURE_MISC_NOT_IMPLEMENTED.is_empty();

    c.assert_behaviour(
        "examine.character.every-row-draws-in-the-panes-own-order-and-reaches-the-list",
        move |_| in_order && all_drawn && nothing_left,
    );
    c.shutdown();
}

#[test]
fn scenario_every_character_row_draws_in_the_panes_own_order() {
    scenario("every_character_row_draws_in_the_panes_own_order");
}

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
const DIALOG_NO_BUTTON: ElementId = ElementId(0x19);

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
pub fn an_npc_question_appears_and_both_answers_reach_the_shard() {
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

#[test]
fn scenario_an_npc_question_appears_and_both_answers_reach_the_shard() {
    scenario("an_npc_question_appears_and_both_answers_reach_the_shard");
}

/// A question about something being made adds the client's own *Continue?* to what the shard
/// wrote, where the plainest kind adds nothing -- so the two arms are told apart by what is on the
/// screen and not by which arm was taken.
pub fn a_making_question_gets_the_clients_own_continue() {
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

#[test]
fn scenario_a_making_question_gets_the_clients_own_continue() {
    scenario("a_making_question_gets_the_clients_own_continue");
}

/// **The client has no timer of its own.** The only thing that takes an unanswered question down
/// is the shard withdrawing it -- and when it does, the client refuses on the way out rather than
/// leaving the shard to time it out. A withdrawal naming a different question is ignored outright.
pub fn the_shards_withdrawal_closes_the_question_and_refuses_on_the_way_out() {
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

#[test]
fn scenario_the_shards_withdrawal_closes_the_question_and_refuses_on_the_way_out() {
    scenario("the_shards_withdrawal_closes_the_question_and_refuses_on_the_way_out");
}

/// A second question while one is already up puts nothing new on screen and waits behind nothing,
/// and the first question's words stay up -- but the client now holds the **second** question's
/// handle. So answering answers the second question, a withdrawal of the first no longer takes
/// the dialog down, and a withdrawal of the second does, refusing on the way out.
pub fn a_second_question_while_one_is_open_takes_over_its_handle() {
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

#[test]
fn scenario_a_second_question_while_one_is_open_takes_over_its_handle() {
    scenario("a_second_question_while_one_is_open_takes_over_its_handle");
}

/// Two of the seven kinds belong to panels of their own, and each asks its **own** question rather
/// than falling through to the plain one: an invitation to a fellowship names whoever sent it, and
/// somebody swearing to the player is asked about in the allegiance panel's own words -- which is
/// the discriminator, because the plain dialog would have put a question on screen too.
///
/// A kind the client knows nothing about raises nothing and answers nothing.
pub fn an_invitation_and_a_swearing_raise_their_own_panels_questions() {
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

#[test]
fn scenario_an_invitation_and_a_swearing_raise_their_own_panels_questions() {
    scenario("an_invitation_and_a_swearing_raise_their_own_panels_questions");
}

// =============================================================================================
// examine.inscription.* -- the box, its focus edges, and what leaves on the commit
//
// The gestures below are the harness's own -- the pointer goes through the client's pump and
// input manager, and a character is the message the window loop produces after translation.
//
// **The focus edges are the whole subject**: a press that drops a caret into the box, the press
// elsewhere that commits what was typed, Escape, and a target change that takes the caret away.
// =============================================================================================

/// The box, the line under it that carries the signature, and the state the invitation is drawn
/// in.
const INSCRIPTION_TEXT: ElementId = ElementId(0x1000_013E);
const INSCRIPTION_SIGNATURE: ElementId = ElementId(0x1000_013F);
const INSCRIPTION_CENTRED: u32 = 0x1000_0050;
/// The assessment window's own close button.
const EXAM_CLOSE_BUTTON: ElementId = ElementId(0x1000_05F3);

/// The player these scenarios look out of, and the three things in their pack: two that can be
/// written on and one that cannot.
const SCRIBE: ObjectId = ObjectId(0x5000_0001);
const PARCHMENT: ObjectId = ObjectId(0x8000_0001);
const LETTER: ObjectId = ObjectId(0x8000_0002);
const PEBBLE: ObjectId = ObjectId(0x8000_0003);
/// The bit on a thing that says it can be written on.
const INSCRIBABLE: u32 = 0x0000_0002;

/// A client with a scribe and three things in their pack.
fn a_scribe_and_three_things() -> (HeadlessClient, dereth_testkit::Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut peer = dereth_testkit::Peer::attach(&mut c, SCRIBE);
    for (id, parent) in [
        (SCRIBE, ObjectId(0)),
        (PARCHMENT, SCRIBE),
        (LETTER, SCRIBE),
        (PEBBLE, SCRIBE),
    ] {
        let mut p = dereth_protocol::objects::ObjectCreatePayload {
            id,
            ..Default::default()
        };
        p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
        p.physicsdesc.setup_id = Some(0x0200_0001);
        p.physicsdesc.timestamps.instance = 1;
        p.wdesc.header |= dereth_protocol::types::weeniedesc::header::CONTAINER_ID;
        p.wdesc.container_id = Some(parent);
        peer.send(
            &mut c,
            dereth_testkit::replay::OBJECT_QUEUE,
            dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
                .expect("the create encodes"),
        );
    }
    c.tick(1);
    c.world_mut().player = Some(SCRIBE);
    c.world_mut()
        .weenie_mut(SCRIBE)
        .expect("the scribe was created")
        .pwd
        .name = "Larktest".to_owned();
    for (id, name, inscribable) in [
        (PARCHMENT, "Parchment", true),
        (LETTER, "Letter", true),
        (PEBBLE, "Pebble", false),
    ] {
        let w = c.world_mut().weenie_mut(id).expect("created");
        w.pwd.name = name.to_owned();
        if inscribable {
            w.pwd.bitfield |= INSCRIBABLE;
        }
    }
    (c, peer)
}

/// An assessment carrying whatever signature and inscription the scenario wants.
fn inscription_profile(scribe: Option<&str>, inscription: Option<&str>) -> AppraisalProfile {
    let mut p = AppraisalProfile {
        success_flag: 1,
        ..AppraisalProfile::default()
    };
    let mut entries = Vec::new();
    if let Some(t) = inscription {
        entries.push((7_u32, t.to_owned()));
    }
    if let Some(n) = scribe {
        entries.push((8_u32, n.to_owned()));
    }
    if !entries.is_empty() {
        p.flags |= dereth_protocol::types::appraisal::flags::STRING;
        p.tables.strings = Some(dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries,
        });
    }
    p
}

/// What the box is showing, and who may change it.
#[derive(Debug, Clone)]
struct BoxState {
    text: String,
    signature: String,
    state: u32,
    visible: bool,
    editable: bool,
    selectable: bool,
}

fn box_state(c: &mut HeadlessClient) -> BoxState {
    let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
    let h = ui
        .get_element(INSCRIPTION_TEXT)
        .expect("the box is in the shipped layout");
    let hs = ui
        .get_element(INSCRIPTION_SIGNATURE)
        .expect("the signature line is in the layout");
    let state = ui.node(h).expect("a node").state.0;
    let visible = ui.node(h).expect("a node").region.flags.visible;
    let t = ui.text_element_mut(h).expect("a text element");
    let text = t.glyphs.inq_text(false);
    let editable = t.bits.editable();
    let selectable = t.bits.selectable();
    let signature = ui
        .text_element_mut(hs)
        .expect("a text element")
        .glyphs
        .inq_text(false);
    BoxState {
        text,
        signature,
        state,
        visible,
        editable,
        selectable,
    }
}

/// Whether the box is what the keyboard is going to -- what the player notices first.
fn box_has_the_caret(c: &mut HeadlessClient) -> bool {
    let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
    let h = ui
        .get_element(INSCRIPTION_TEXT)
        .expect("the box is in the shipped layout");
    ui.focus_element() == Some(h)
}

/// Where a run of text is drawn inside its own box, on both axes.
///
/// `(left gap, right gap)` and `(top gap, bottom gap)`: a centred run has the two of a pair equal
/// to within the pixel integer division drops, and a run drawn hard against the corner has a gap
/// of nothing on that side.
fn text_gaps(c: &mut HeadlessClient, id: ElementId) -> ((i32, i32), (i32, i32)) {
    let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
    let h = ui
        .get_element(id)
        .expect("the element is in the shipped layout");
    let screen = ui.screen_box(h);
    let t = ui.text_element_mut(h).expect("a text element");
    let content = t.content_box(screen);
    let placed = t.compose(screen);
    if placed.is_empty() {
        return ((0, 0), (0, 0));
    }
    let run_w: i32 = t.glyphs.glyphs.iter().map(|g| g.width).sum();
    let run_h = t.text_height(screen);
    let left = placed[0].x - content.x0;
    let top = placed[0].y - content.y0;
    (
        (left, content.width() - run_w - left),
        (top, content.height() - run_h - top),
    )
}

/// Put the caret in the box, as a press inside it does.
fn press_the_box(c: &mut HeadlessClient) {
    dereth_testkit::input_steps::focus(c, INSCRIPTION_TEXT);
    c.tick(1);
}

/// One of the two actions the box answers -- Escape, which lets the caret go, and Return, which
/// this box deliberately does not treat as a commit.
fn send_box_action(c: &mut HeadlessClient, action: u32) {
    {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let Some(h) = ui.focus_element() else { return };
        ui.dispatch_action(
            h,
            &dereth_ui::focus::InputEvent {
                action,
                start: true,
                x: 0,
                y: 0,
            },
        );
    }
    c.tick(2);
}

fn press_escape_in_the_box(c: &mut HeadlessClient) {
    send_box_action(c, dereth_ui::focus::action::ESCAPE);
}

/// Rub out `n` characters, one key press each.
fn backspace(c: &mut HeadlessClient, n: usize) {
    {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let h = ui.focus_element().expect("the box has the caret");
        for _ in 0..n {
            ui.dispatch_action(
                h,
                &dereth_ui::focus::InputEvent {
                    action: dereth_ui::focus::action::BACKSPACE,
                    start: true,
                    x: 0,
                    y: 0,
                },
            );
        }
    }
    c.tick(1);
}

/// Let the caret go without pressing anything, which is what any other window taking it does.
fn drop_the_caret(c: &mut HeadlessClient) {
    {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let h = ui
            .get_element(INSCRIPTION_TEXT)
            .expect("the box is in the shipped layout");
        ui.relinquish_focus(h);
    }
    c.tick(2);
}

/// Look at another thing **without a pointer gesture**: the pane is told what it is waiting for
/// and the shard answers it.
///
/// It is not [`assess`] because that one presses the identify button, and a press anywhere else on
/// the screen is itself a focus edge -- which is the very thing these two claims are about.
fn look_at(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    object: ObjectId,
    profile: &AppraisalProfile,
) {
    await_the_answer_for(c, object);
    deliver_the_answer(c, peer, object, profile);
}

/// Every inscription this client has really put on the wire since the last look.
fn inscriptions_sent(c: &HeadlessClient) -> Vec<dereth_protocol::trade::WritingSetInscription> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::SetInscription(m) => Some(m.clone()),
            _ => None,
        })
        .collect()
}

/// **The invitation is centred and a real inscription is not.** The empty box says where to write
/// and says it in the middle of itself; a box that carries somebody's words draws them from the
/// corner, with the signature under it.
pub fn the_invitation_is_centred_and_a_real_inscription_is_not() {
    let (mut c, mut peer) = a_scribe_and_three_things();

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    let empty = box_state(&mut c);
    let (h_gaps, v_gaps) = text_gaps(&mut c, INSCRIPTION_TEXT);
    let centred = empty.text == "<Inscribe here>"
        && empty.state == INSCRIPTION_CENTRED
        && (h_gaps.0 - h_gaps.1).abs() <= 1
        && h_gaps.0 > 0
        && (v_gaps.0 - v_gaps.1).abs() <= 1
        && v_gaps.0 > 0;

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Aldwyne"), Some("For a friend.")),
    );
    let written = box_state(&mut c);
    let (h_gaps, v_gaps) = text_gaps(&mut c, INSCRIPTION_TEXT);
    let not_centred = written.text == "For a friend."
        && written.state != INSCRIPTION_CENTRED
        && h_gaps.0 == 0
        && v_gaps.0 == 0
        && written.signature == "--Aldwyne";

    c.assert_behaviour(
        "examine.inscription.the-invitation-is-centred-and-a-real-inscription-is-not",
        move |_| centred && not_centred,
    );
    c.shutdown();
}

#[test]
fn scenario_the_invitation_is_centred_and_a_real_inscription_is_not() {
    scenario("the_invitation_is_centred_and_a_real_inscription_is_not");
}

/// **A press inside the box.** A press inside the box empties the invitation out of it rather
/// than dropping a caret into the literal words, and puts the player's own signature under it so
/// they can see whose name they are about to leave -- and nothing but a press inside it gives the
/// box the keyboard, so assessing a thing does not quietly take it.
pub fn a_press_empties_the_box_and_shows_whose_signature_it_will_carry() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );

    let before = box_state(&mut c);
    let untouched = before.text == "<Inscribe here>" && before.signature.is_empty();
    let no_caret_from_assessing = !box_has_the_caret(&mut c);

    press_the_box(&mut c);
    let after = box_state(&mut c);
    let emptied = after.text.is_empty()
        && after.state != INSCRIPTION_CENTRED
        && after.signature == "--Larktest";
    let has_caret = box_has_the_caret(&mut c);

    c.assert_behaviour(
        "examine.inscription.a-press-empties-the-box-and-shows-whose-signature-it-will-carry",
        move |_| untouched && no_caret_from_assessing && emptied && has_caret,
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_empties_the_box_and_shows_whose_signature_it_will_carry() {
    scenario("a_press_empties_the_box_and_shows_whose_signature_it_will_carry");
}

/// **The inscription persists.** Letting the caret go is what sends
/// what was written; the box keeps showing it and the signature keeps showing who wrote it, and
/// the same thing assessed again comes back with both, still the writer's to change -- with a
/// second look saying nothing more.
pub fn letting_the_caret_go_sends_what_was_written() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );

    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Property of Larktest");
    drop_the_caret(&mut c);

    let sent = inscriptions_sent(&c);
    let on_the_wire = sent
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: PARCHMENT,
            text: "Property of Larktest".to_owned(),
        }];
    let kept = {
        let b = box_state(&mut c);
        b.text == "Property of Larktest" && b.signature == "--Larktest"
    };

    // The shard took it, and answers the next look with both strings.
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Larktest"), Some("Property of Larktest")),
    );
    let b = box_state(&mut c);
    let came_back = b.text == "Property of Larktest"
        && b.signature == "--Larktest"
        && b.state != INSCRIPTION_CENTRED
        && b.editable;

    // ...and looking in and out again without changing a letter is not a second inscription.
    press_the_box(&mut c);
    drop_the_caret(&mut c);
    let no_second = inscriptions_sent(&c).len() == 1;

    c.assert_behaviour(
        "examine.inscription.letting-the-caret-go-sends-what-was-written-and-it-comes-back",
        move |_| on_the_wire && kept && came_back && no_second,
    );
    c.shutdown();
}

#[test]
fn scenario_letting_the_caret_go_sends_what_was_written() {
    scenario("letting_the_caret_go_sends_what_was_written");
}

/// An empty box on a thing nobody has signed is nothing to say: the caret going in and out again
/// sends nothing and the invitation comes back, centred.
pub fn leaving_an_unchanged_box_sends_nothing() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );

    press_the_box(&mut c);
    drop_the_caret(&mut c);
    let nothing_sent = inscriptions_sent(&c).is_empty();
    let b = box_state(&mut c);
    let invitation_back = b.text == "<Inscribe here>" && b.state == INSCRIPTION_CENTRED;

    // Write something, leave, come back, leave again without touching it: one inscription.
    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Mine");
    drop_the_caret(&mut c);
    let one = inscriptions_sent(&c).len() == 1;
    press_the_box(&mut c);
    drop_the_caret(&mut c);
    let still_one = inscriptions_sent(&c).len() == 1;

    c.assert_behaviour(
        "examine.inscription.leaving-an-unchanged-box-sends-nothing-and-the-invitation-comes-back",
        move |_| nothing_sent && invitation_back && one && still_one,
    );
    c.shutdown();
}

#[test]
fn scenario_leaving_an_unchanged_box_sends_nothing() {
    scenario("leaving_an_unchanged_box_sends_nothing");
}

/// **The premise the report got the wrong way round.** The shipped data says Escape is the edge
/// that confirms an inscription and that Return is not: the box carries the attribute for one and
/// not the other, Return is swallowed and the caret stays, and Escape lets the caret go -- which
/// is what sends.
pub fn escape_is_the_confirm_edge_and_return_is_not() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );

    // The shipped attributes, so this stands on the data and not on a belief about it.
    let (escape_attr, accept_attr) = {
        let ui = &c.app_mut().ui().expect("the shell is up").ui;
        let h = ui
            .get_element(INSCRIPTION_TEXT)
            .expect("the box is in the shipped layout");
        let p = ui.node(h).expect("a node").merged_properties();
        (
            p.get_bool(dereth_ui::props::attr::TEXT_LOSE_FOCUS_ON_ESCAPE),
            p.get_bool(dereth_ui::props::attr::TEXT_LOSE_FOCUS_ON_ACCEPT),
        )
    };
    let shipped = escape_attr == Some(true) && accept_attr.is_none();

    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Ink");

    send_box_action(&mut c, dereth_ui::focus::action::ACCEPT);
    let return_does_nothing = inscriptions_sent(&c).is_empty() && box_has_the_caret(&mut c);

    press_escape_in_the_box(&mut c);
    let escape_commits = inscriptions_sent(&c)
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: PARCHMENT,
            text: "Ink".to_owned(),
        }];

    c.assert_behaviour(
        "examine.inscription.escape-is-the-edge-that-confirms-it-and-return-is-not",
        move |_| shipped && return_does_nothing && escape_commits,
    );
    c.shutdown();
}

#[test]
fn scenario_escape_is_the_confirm_edge_and_return_is_not() {
    scenario("escape_is_the_confirm_edge_and_return_is_not");
}

/// A box somebody else signed is not the player's to change, and whatever is typed at it goes
/// nowhere -- while their own signature does not lock them out, and a thing that is not in their
/// possession is not theirs to write on either.
pub fn a_box_somebody_else_signed_is_not_yours_to_change() {
    let (mut c, mut peer) = a_scribe_and_three_things();

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Aldwyne"), Some("For a friend.")),
    );
    let b = box_state(&mut c);
    let locked = !b.editable && !b.selectable;

    // ...and nothing typed at it reaches the shard: the backing plate has the pointer, so a press
    // never reaches the text at all.
    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "mine now");
    press_escape_in_the_box(&mut c);
    let silent = inscriptions_sent(&c).is_empty();

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Larktest"), Some("Mine.")),
    );
    let own = box_state(&mut c).editable;

    // A thing that is not in the player's possession is not theirs either.
    c.world_mut()
        .weenie_mut(PARCHMENT)
        .expect("the parchment")
        .pwd
        .container_id = Some(ObjectId(0x7000_0009));
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    let not_yours = !box_state(&mut c).editable;

    c.assert_behaviour(
        "examine.inscription.a-box-somebody-else-signed-is-not-yours-to-change",
        move |_| locked && silent && own && not_yours,
    );
    c.shutdown();
}

#[test]
fn scenario_a_box_somebody_else_signed_is_not_yours_to_change() {
    scenario("a_box_somebody_else_signed_is_not_yours_to_change");
}

/// **Signed and blank.** A thing somebody has signed but written nothing on shows a box
/// that is there and blank -- still centred, with no signature under it -- and it is not the
/// player's to change unless the signature is their own, in which case the box is theirs to write
/// on and the press does not wipe what is not there.
pub fn a_signed_thing_with_no_words_shows_a_blank_box() {
    let (mut c, mut peer) = a_scribe_and_three_things();

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Aldwyne"), None),
    );
    let b = box_state(&mut c);
    let blank_and_locked = b.text.is_empty()
        && b.visible
        && b.state == INSCRIPTION_CENTRED
        && b.signature.is_empty()
        && !b.editable
        && !b.selectable;

    assess(
        &mut c,
        &mut peer,
        LETTER,
        &inscription_profile(Some("Larktest"), None),
    );
    let b = box_state(&mut c);
    let blank_and_yours = b.text.is_empty() && b.editable;

    press_the_box(&mut c);
    let after = box_state(&mut c);
    // Nothing to wipe, so nothing is wiped -- and the signature preview runs on every press and
    // not only on the empty-scribe one.
    let preview = after.text.is_empty() && after.signature == "--Larktest";

    dereth_testkit::input_steps::type_text(&mut c, "Ink");
    press_escape_in_the_box(&mut c);
    let sent = inscriptions_sent(&c)
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: LETTER,
            text: "Ink".to_owned(),
        }];

    c.assert_behaviour(
        "examine.inscription.a-signed-thing-with-no-words-shows-a-blank-box-that-is-the-scribes",
        move |_| blank_and_locked && blank_and_yours && preview && sent,
    );
    c.shutdown();
}

#[test]
fn scenario_a_signed_thing_with_no_words_shows_a_blank_box() {
    scenario("a_signed_thing_with_no_words_shows_a_blank_box");
}

/// **The erase half.** The writer rubbing their own words out sends an
/// empty inscription, the centred invitation comes back with the signature line cleared -- and the
/// thing is anybody's to sign again.
pub fn the_scribe_can_rub_out_his_own_words() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Larktest"), Some("Ink")),
    );
    let starts_with_words = box_state(&mut c).text == "Ink";

    press_the_box(&mut c);
    backspace(&mut c, 8);
    press_escape_in_the_box(&mut c);

    let erased = inscriptions_sent(&c)
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: PARCHMENT,
            text: String::new(),
        }];
    let b = box_state(&mut c);
    let invitation_back =
        b.text == "<Inscribe here>" && b.state == INSCRIPTION_CENTRED && b.signature.is_empty();

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    let b = box_state(&mut c);
    let anybodys = b.text == "<Inscribe here>" && b.editable;

    c.assert_behaviour(
        "examine.inscription.the-writer-can-rub-out-his-own-words-and-the-invitation-comes-back",
        move |_| starts_with_words && erased && invitation_back && anybodys,
    );
    c.shutdown();
}

#[test]
fn scenario_the_scribe_can_rub_out_his_own_words() {
    scenario("the_scribe_can_rub_out_his_own_words");
}

/// **The commit a player really makes.** Pressing the window's own close button takes the caret
/// away before the window goes, so what was typed is sent on the way out rather than lost.
pub fn the_close_button_commits_on_its_way_out() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );

    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Ink");
    c.when(Player::click(EXAM_CLOSE_BUTTON));
    c.tick(2);

    let sent = inscriptions_sent(&c)
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: PARCHMENT,
            text: "Ink".to_owned(),
        }];

    c.assert_behaviour(
        "examine.inscription.the-windows-close-button-commits-what-was-typed-on-its-way-out",
        move |_| sent,
    );
    c.shutdown();
}

#[test]
fn scenario_the_close_button_commits_on_its_way_out() {
    scenario("the_close_button_commits_on_its_way_out");
}

/// **Changing target.** Looking at something the player may not write on
/// -- or at something that cannot be written on at all -- takes the caret away with it, and what
/// was typed is discarded rather than sent to either thing; the keyboard stops going to the box,
/// which is the symptom.
pub fn changing_the_target_to_one_you_may_not_write_on_drops_the_caret() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Ink");
    let writing = box_has_the_caret(&mut c) && box_state(&mut c).text == "Ink";

    // Somebody else's, and signed.
    look_at(
        &mut c,
        &mut peer,
        LETTER,
        &inscription_profile(Some("Aldwyne"), Some("Bob's")),
    );
    let dropped = !box_has_the_caret(&mut c);
    let b = box_state(&mut c);
    let shows_the_new_one =
        !b.editable && !b.selectable && b.text == "Bob's" && b.signature == "--Aldwyne";
    let discarded = inscriptions_sent(&c).is_empty();

    // ...and the keyboard really has stopped going there.
    dereth_testkit::input_steps::type_text(&mut c, "more");
    let no_insertion = box_state(&mut c).text == "Bob's";

    // The other way the caret goes: a thing that cannot be written on at all.
    let (mut c2, mut peer2) = a_scribe_and_three_things();
    assess(
        &mut c2,
        &mut peer2,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    press_the_box(&mut c2);
    dereth_testkit::input_steps::type_text(&mut c2, "Ink");
    look_at(
        &mut c2,
        &mut peer2,
        PEBBLE,
        &inscription_profile(None, None),
    );
    let pebble_dropped = !box_has_the_caret(&mut c2);
    let b = box_state(&mut c2);
    let pebble_silent = !b.editable
        && b.text == "<Inscribe here>"
        && b.signature.is_empty()
        && inscriptions_sent(&c2).is_empty();
    c2.shutdown();

    c.assert_behaviour(
        "examine.inscription.looking-at-one-you-may-not-write-on-takes-the-caret-away",
        move |_| {
            writing
                && dropped
                && shows_the_new_one
                && discarded
                && no_insertion
                && pebble_dropped
                && pebble_silent
        },
    );
    c.shutdown();
}

#[test]
fn scenario_changing_the_target_to_one_you_may_not_write_on_drops_the_caret() {
    scenario("changing_the_target_to_one_you_may_not_write_on_drops_the_caret");
}

/// **The half the fix must not overreach.** Looking at another thing the player *may* write on
/// keeps the caret exactly where it was and sends nothing -- and after a drop, the next
/// inscription still commits, once, carrying the second set of words and naming the thing the box
/// was showing.
pub fn changing_to_another_one_you_may_write_on_keeps_the_caret() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Ink");

    look_at(&mut c, &mut peer, LETTER, &inscription_profile(None, None));
    let kept = box_has_the_caret(&mut c) && box_state(&mut c).editable;
    let silent = inscriptions_sent(&c).is_empty();

    // A drop, and then a second inscription that still works.
    look_at(&mut c, &mut peer, PEBBLE, &inscription_profile(None, None));
    let dropped = !box_has_the_caret(&mut c);
    look_at(&mut c, &mut peer, LETTER, &inscription_profile(None, None));
    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Second");
    press_escape_in_the_box(&mut c);
    let committed = inscriptions_sent(&c)
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: LETTER,
            text: "Second".to_owned(),
        }];

    c.assert_behaviour(
        "examine.inscription.looking-at-another-you-may-write-on-keeps-the-caret-where-it-was",
        move |_| kept && silent && dropped && committed,
    );
    c.shutdown();
}

#[test]
fn scenario_changing_to_another_one_you_may_write_on_keeps_the_caret() {
    scenario("changing_to_another_one_you_may_write_on_keeps_the_caret");
}

// =============================================================================================
// journal.*
//
// The notebook: its pages, the page list, search, the timer, and persistence. The notebook is a
// file beside the preferences file, kept in a disposable directory `ClientSpec` names, and every
// scenario types into an edit box.
//
// **Nothing here sends a datagram.** The journal is client-local: no message carries a page, and
// every scenario asserts that nothing left.
//
// The gestures are the player's. A press is the pointer at the control's own centre through the
// client's pump and input manager; a keystroke is the message the window loop produces after
// translation, sent to whatever holds the caret; and a box is given the caret by **pressing it**,
// with the caret's arrival asserted before a single character goes in -- so a scenario cannot
// pass by typing into a box a player could never have reached.
// =============================================================================================

use dereth_ui_screens::panels::{journal, pagelist};

/// The world and the character the notebook's own name is made of.
const JOURNAL_WORLD: &str = "Frostfell";
const JOURNAL_CHARACTER: &str = "Kupotest";

/// A client in the world whose settings live in a disposable directory of their own.
fn a_client_with_a_notebook(tag: &str) -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay_in_world(4).with_scratch_settings(tag))
}

/// Tell the client who it is playing, the way logging in does.
///
/// **It happens after the opening frames on purpose.** A real client has no character name until
/// it logs on, so the notebook's own latch happens first and the file is read on a later frame --
/// which is the half of the load a scenario that set this up front would never reach.
fn playing_as(c: &mut HeadlessClient, character: &str) {
    let host = c.app_mut().host_state_mut();
    host.entered_character = Some(character.to_owned());
    host.world_name = Some(JOURNAL_WORLD.to_owned());
    c.tick(2);
}

/// Where this client's notebook is written.
fn notebook_path(c: &HeadlessClient, character: &str) -> std::path::PathBuf {
    let dir = c
        .scratch_settings()
        .expect("this scenario asked for a settings directory")
        .dir();
    journal::JournalIdentity {
        directory: dir.to_path_buf(),
        world: JOURNAL_WORLD.to_owned(),
        character: character.to_owned(),
    }
    .client_path()
}

/// One element of the shipped layout under the gameplay root.
fn el(c: &mut HeadlessClient, id: ElementId) -> ElemHandle {
    let (ui, screen) = gameplay_screen(c.app_mut());
    let root = screen.root().expect("the gameplay screen has a root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

fn el_text(c: &mut HeadlessClient, id: ElementId) -> String {
    let h = el(c, id);
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

fn el_visible(c: &mut HeadlessClient, id: ElementId) -> bool {
    let h = el(c, id);
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(h).expect("a live node").region.flags.visible
}

/// Open one page of the toolbar's stack, the way the toolbar button does.
fn open_the_page(c: &mut HeadlessClient, page: ElementId) {
    {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let panel_id = screen
            .panels
            .pages
            .iter()
            .find(|p| p.element == page)
            .map(|p| p.panel_id)
            .unwrap_or_else(|| panic!("{page:?} is one of the shipped registered pages"));
        screen.recv_set_panel_visibility(ui, panel_id, true);
    }
    c.tick(3);
}

/// Press the tab that owns `sub`, found through the page's own tab table rather than named.
fn click_the_tab(c: &mut HeadlessClient, page: ElementId, sub: ElementId) {
    let handle = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        let h = ui
            .get_child_recursive(root, page)
            .expect("the page is in the shipped layout");
        let tab = {
            let n = ui.node(h).expect("the page has a node");
            let b = n.behaviour.as_ref().expect("the page has a behaviour");
            (**b)
                .as_any()
                .and_then(|a| a.downcast_ref::<dereth_ui::widgets::panel::Panel>())
                .expect("the page is a panel")
                .tab_to_page
                .iter()
                .find(|(_, pg)| **pg == sub)
                .map(|(t, _)| *t)
                .expect("the page names a tab for this sub-panel")
        };
        ui.get_child_recursive(root, tab)
            .expect("the tab element is in the shipped layout")
    };
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(handle, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    }
    c.tick(3);
}

/// Open the quest page with `sub` up. Every journal scenario starts here.
fn open_the_journal(c: &mut HeadlessClient, sub: ElementId) {
    open_the_page(c, journal::PAGE);
    click_the_tab(c, journal::PAGE, sub);
}

/// **A real pointer press**, with the element the hit test chose handed back -- so a scenario can
/// say that the press landed on the control it meant and not on whatever is drawn over it.
fn press_control(c: &mut HeadlessClient, id: ElementId) -> Option<ElementId> {
    let h = el(c, id);
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    let hit = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.hit_test_screen(at.x, at.y)
    };
    c.when(Player::Click(Target::Point(at)));
    c.tick(2);
    let (ui, _) = gameplay_screen(c.app_mut());
    hit.and_then(|e| ui.node(e))
        .map(dereth_ui::ElementNode::element_id)
}

/// Put the caret in a box **by pressing it**, and prove the caret arrived before anything is
/// typed.
fn press_into(c: &mut HeadlessClient, id: ElementId) {
    let h = el(c, id);
    let hit = press_control(c, id);
    assert_eq!(
        hit,
        Some(id),
        "the pointer landed on {id:?} and not on something drawn over it"
    );
    let (ui, _) = gameplay_screen(c.app_mut());
    assert_eq!(
        ui.focus_element(),
        Some(h),
        "a press on {id:?} must take the caret; nothing else in this client moves it on a click"
    );
}

/// Empty a box. The client's own clear is not reachable by any gesture, so a scenario that needs
/// an empty box says so here rather than pretending to rub it out.
fn empty_the_box(c: &mut HeadlessClient, id: ElementId) {
    let h = el(c, id);
    let (ui, _) = gameplay_screen(c.app_mut());
    if let Some(t) = ui.text_element_mut(h) {
        t.set_text("");
    }
}

/// Press a box and type into it.
fn write_in(c: &mut HeadlessClient, id: ElementId, text: &str) {
    press_into(c, id);
    dereth_testkit::input_steps::type_text(c, text);
    c.tick(1);
}

/// Every row of the page list, as `(number, title, timer, label)`.
fn page_list_rows(c: &mut HeadlessClient) -> Vec<(String, String, String, String)> {
    let list = el(c, pagelist::LIST);
    let (ui, _) = gameplay_screen(c.app_mut());
    let mut out = Vec::new();
    for row in ui.children(list) {
        let mut cell = |id: ElementId| -> String {
            ui.get_child_recursive(row, id)
                .and_then(|h| ui.text_element_mut(h))
                .map_or_else(String::new, |t| t.glyphs.inq_text(false))
        };
        let n = cell(pagelist::ROW_PAGE_NUMBER);
        if n.is_empty() {
            continue;
        }
        out.push((
            n,
            cell(pagelist::ROW_TITLE),
            cell(pagelist::ROW_TIMER),
            cell(pagelist::ROW_LABEL),
        ));
    }
    out
}

fn page_titles(c: &mut HeadlessClient) -> Vec<String> {
    page_list_rows(c).into_iter().map(|r| r.1).collect()
}

/// Write three titled pages through the journal tab, every keystroke through the pointer.
fn three_journal_pages(c: &mut HeadlessClient) {
    open_the_journal(c, journal::PANEL);
    for (i, title) in ["Aluvian", "Banderling", "Colosseum"]
        .into_iter()
        .enumerate()
    {
        empty_the_box(c, journal::TITLE_EDIT);
        write_in(c, journal::TITLE_EDIT, title);
        if i < 2 {
            press_control(c, journal::NEW_PAGE_BUTTON);
        }
    }
    press_control(c, journal::FIRST_PAGE_BUTTON);
}

/// Nothing about the journal ever leaves the machine.
fn nothing_was_sent(c: &HeadlessClient) -> bool {
    c.view().outbound().is_empty()
}

/// **The gate.** The quest page opens on another tab; pressing the journal's own tab puts a
/// notebook up, open at page one, with the timer's three boxes showing and its countdown down.
pub fn the_journal_opens_on_page_one_with_the_timer_editable() {
    let mut c = a_client_with_a_notebook("journal-open");

    let closed_first = !el_visible(&mut c, journal::PANEL);
    open_the_journal(&mut c, journal::PANEL);
    let up = el_visible(&mut c, journal::PANEL);

    let page_one = el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 1 ~";
    let button = el_text(&mut c, journal::START_TIMER_BUTTON) == journal::START;
    let nowhere = el_text(&mut c, journal::LOCATION_TEXT) == journal::NONE;
    let editable = [
        journal::DAYS_EDIT,
        journal::DAYS_TEXT,
        journal::HOURS_EDIT,
        journal::HOURS_TEXT,
        journal::MINUTES_EDIT,
        journal::MINUTES_TEXT,
    ]
    .into_iter()
    .all(|id| el_visible(&mut c, id));
    let countdown_down = !el_visible(&mut c, journal::TIMER_TEXT);
    let made = c.view().expect_app().hud().panels.journal.pages.len() == 1
        && c.view().expect_app().hud().panels.journal.fully_bound();
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.page.the-tab-opens-on-page-one-with-the-timer-ready-to-set",
        move |_| {
            closed_first
                && up
                && page_one
                && button
                && nowhere
                && editable
                && countdown_down
                && made
                && silent
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_journal_opens_on_page_one_with_the_timer_editable() {
    scenario("the_journal_opens_on_page_one_with_the_timer_editable");
}

/// **The typing reaches the notebook.** A title typed on page one is gone from the box on page
/// two and back again on the way back -- which is the page being written down rather than the box
/// being left alone.
pub fn a_typed_title_survives_a_page_turn() {
    let mut c = a_client_with_a_notebook("journal-turn");
    open_the_journal(&mut c, journal::PANEL);

    let title = "Hollow Minion";
    write_in(&mut c, journal::TITLE_EDIT, title);
    let typed = el_text(&mut c, journal::TITLE_EDIT) == title;

    let hit = press_control(&mut c, journal::NEW_PAGE_BUTTON);
    let turned = hit == Some(journal::NEW_PAGE_BUTTON)
        && el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 2 ~"
        && el_text(&mut c, journal::TITLE_EDIT).is_empty()
        && c.view().expect_app().hud().panels.journal.pages.len() == 2;

    let hit = press_control(&mut c, journal::PREV_PAGE_BUTTON);
    let back = hit == Some(journal::PREV_PAGE_BUTTON)
        && el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 1 ~"
        && el_text(&mut c, journal::TITLE_EDIT) == title
        && c.view().expect_app().hud().panels.journal.pages[0].title == title;
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.page.a-title-typed-on-one-page-is-there-on-the-way-back",
        move |_| typed && turned && back && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_a_typed_title_survives_a_page_turn() {
    scenario("a_typed_title_survives_a_page_turn");
}

/// **The countdown.** Two minutes typed in and started: the button says reset, the three boxes go
/// away, the countdown comes up reading what was set, and it runs down on its own with no further
/// input. Pressing again puts the boxes back.
pub fn the_journal_timer_counts_down_and_the_button_resets_it() {
    let mut c = a_client_with_a_notebook("journal-timer");
    open_the_journal(&mut c, journal::PANEL);

    empty_the_box(&mut c, journal::MINUTES_EDIT);
    write_in(&mut c, journal::MINUTES_EDIT, "2");
    let hit = press_control(&mut c, journal::START_TIMER_BUTTON);
    let started = hit == Some(journal::START_TIMER_BUTTON)
        && el_text(&mut c, journal::START_TIMER_BUTTON) == journal::RESET
        && el_visible(&mut c, journal::TIMER_TEXT)
        && !el_visible(&mut c, journal::MINUTES_EDIT);
    let first = el_text(&mut c, journal::TIMER_TEXT);
    // **Two minutes, less the frames the press itself costs.** The countdown starts the moment
    // the button is pressed and the client's clock moves a fixed amount per frame, so the first
    // reading a scenario can take is already a second or so down. The shape is exact and the
    // number is a window.
    let read_seconds = |t: &str| -> Option<i64> {
        t.split_once("m ").and_then(|(m, s)| {
            Some(m.parse::<i64>().ok()? * 60 + s.strip_suffix('s')?.parse::<i64>().ok()?)
        })
    };
    let reads_two_minutes = read_seconds(&first).is_some_and(|r| (117..=120).contains(&r));

    // Nothing but time: the headless client steps its own clock a fixed amount per frame.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (5.0 / dereth_client::app::HEADLESS_STEP).ceil() as u64 + 2;
    c.tick(frames);
    let later = el_text(&mut c, journal::TIMER_TEXT);
    let moved = later != first;
    let remaining = read_seconds(&later);
    // Two minutes less about five seconds of frames; the frame step and the redraw period both
    // round, so the window is the claim and the shape is exact.
    let counted_down = remaining.is_some_and(|r| (112..120).contains(&r));

    let hit = press_control(&mut c, journal::START_TIMER_BUTTON);
    let reset = hit == Some(journal::START_TIMER_BUTTON)
        && el_text(&mut c, journal::START_TIMER_BUTTON) == journal::START
        && el_visible(&mut c, journal::MINUTES_EDIT)
        && !el_visible(&mut c, journal::TIMER_TEXT);
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.timer.it-counts-down-on-its-own-and-the-button-puts-it-back",
        move |_| started && reads_two_minutes && moved && counted_down && reset && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_the_journal_timer_counts_down_and_the_button_resets_it() {
    scenario("the_journal_timer_counts_down_and_the_button_resets_it");
}

/// **The page list is the journal's own pages.** Three written pages are three rows, numbered and
/// titled in page order, with no timer running on any of them.
pub fn the_page_list_lists_the_journals_pages() {
    let mut c = a_client_with_a_notebook("journal-list");
    three_journal_pages(&mut c);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let up = el_visible(&mut c, pagelist::PANEL);

    let rows = page_list_rows(&mut c);
    let numbered = rows.iter().map(|r| r.0.clone()).collect::<Vec<_>>() == vec!["1", "2", "3"];
    let titled = rows.iter().map(|r| r.1.clone()).collect::<Vec<_>>()
        == vec!["Aluvian", "Banderling", "Colosseum"];
    let no_timers = rows.iter().all(|r| r.2 == journal::NONE);
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.list.the-list-is-the-journals-own-pages-in-page-order",
        move |_| up && numbered && titled && no_timers && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_the_page_list_lists_the_journals_pages() {
    scenario("the_page_list_lists_the_journals_pages");
}

/// **Search, from both sides.** Typing into the search box narrows
/// nothing by itself -- the search control is the only thing that filters -- and pressing it
/// narrows the list to the pages whose words contain what was typed, whatever the case.
pub fn only_the_search_control_filters_the_page_list() {
    let mut c = a_client_with_a_notebook("journal-search");
    three_journal_pages(&mut c);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let all_three = page_titles(&mut c) == vec!["Aluvian", "Banderling", "Colosseum"];

    write_in(&mut c, pagelist::SEARCH_EDIT, "band");
    let typed_but_unfiltered = el_text(&mut c, pagelist::SEARCH_EDIT) == "band"
        && page_titles(&mut c) == vec!["Aluvian", "Banderling", "Colosseum"];

    let hit = press_control(&mut c, pagelist::SEARCH_BUTTON);
    let filtered =
        hit == Some(pagelist::SEARCH_BUTTON) && page_titles(&mut c) == vec!["Banderling"];
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.list.typing-alone-does-not-filter-and-the-search-control-does",
        move |_| all_three && typed_but_unfiltered && filtered && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_only_the_search_control_filters_the_page_list() {
    scenario("only_the_search_control_filters_the_page_list");
}

/// **The reset control clears the search and deletes nothing.** It puts the whole list back with
/// every page still in the notebook -- and on an empty box it is a gesture that changes nothing
/// and still ran, which is the half that would hide a reset wired to the delete arm.
pub fn reset_clears_the_search_and_deletes_nothing() {
    let mut c = a_client_with_a_notebook("journal-reset");
    three_journal_pages(&mut c);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);

    write_in(&mut c, pagelist::SEARCH_EDIT, "colo");
    press_control(&mut c, pagelist::SEARCH_BUTTON);
    let narrowed = page_titles(&mut c) == vec!["Colosseum"];

    let hit = press_control(&mut c, pagelist::CLEAR_SEARCH_BUTTON);
    let cleared = hit == Some(pagelist::CLEAR_SEARCH_BUTTON)
        && el_text(&mut c, pagelist::SEARCH_EDIT).is_empty()
        && page_titles(&mut c) == vec!["Aluvian", "Banderling", "Colosseum"]
        && c.view().expect_app().hud().panels.journal.pages.len() == 3;

    // On an empty box: nothing changes, and nothing is deleted either.
    let before = page_list_rows(&mut c);
    let hit = press_control(&mut c, pagelist::CLEAR_SEARCH_BUTTON);
    let harmless = hit == Some(pagelist::CLEAR_SEARCH_BUTTON)
        && page_list_rows(&mut c) == before
        && c.view().expect_app().hud().panels.journal.pages.len() == 3;
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.list.the-reset-control-clears-the-search-and-deletes-no-page",
        move |_| narrowed && cleared && harmless && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_reset_clears_the_search_and_deletes_nothing() {
    scenario("reset_clears_the_search_and_deletes_nothing");
}

/// **Selection, asserted by its consequence.** Pressing a row and
/// then the delete control removes *that* row's page and no other -- which is the only way to see
/// that the press selected anything at all.
pub fn pressing_a_row_then_delete_removes_that_rows_page() {
    let mut c = a_client_with_a_notebook("journal-delete");
    three_journal_pages(&mut c);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let all_three = page_titles(&mut c) == vec!["Aluvian", "Banderling", "Colosseum"];

    let row = {
        let list = el(&mut c, pagelist::LIST);
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.children(list).get(1).copied().expect("three rows")
    };
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(row);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    // A press over a row lands on the **list**, because no row of the shipped list is a target of
    // its own: the list works out which row was pressed from where the pointer was.
    let hit = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.hit_test_screen(at.x, at.y)
            .and_then(|h| ui.node(h))
            .map(dereth_ui::ElementNode::element_id)
    };
    let on_the_list = hit == Some(pagelist::LIST);
    c.when(Player::Click(Target::Point(at)));
    c.tick(2);

    let hit = press_control(&mut c, pagelist::DELETE_BUTTON);
    let deleted = hit == Some(pagelist::DELETE_BUTTON)
        && page_titles(&mut c) == vec!["Aluvian", "Colosseum"]
        && c.view().expect_app().hud().panels.journal.pages.len() == 2;
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.list.a-pressed-row-is-the-one-the-delete-control-removes",
        move |_| all_three && on_the_list && deleted && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_pressing_a_row_then_delete_removes_that_rows_page() {
    scenario("pressing_a_row_then_delete_removes_that_rows_page");
}

/// **A double press on a row opens that page in the journal.** Two presses inside the double-press
/// window on the third row, and the journal comes forward showing page three.
pub fn a_double_press_on_a_row_opens_that_page() {
    let mut c = a_client_with_a_notebook("journal-open-row");
    three_journal_pages(&mut c);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let three_rows = page_list_rows(&mut c).len() == 3;
    let on_page_one = el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 1 ~";

    let row = {
        let list = el(&mut c, pagelist::LIST);
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.children(list).get(2).copied().expect("three rows")
    };
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(row);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    for _ in 0..2 {
        c.when(Player::Click(Target::Point(at)));
        c.tick(1);
    }

    let opened = el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 3 ~"
        && el_text(&mut c, journal::TITLE_EDIT) == "Colosseum";
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.list.a-double-press-on-a-row-opens-that-page-in-the-journal",
        move |_| three_rows && on_page_one && opened && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_a_double_press_on_a_row_opens_that_page() {
    scenario("a_double_press_on_a_row_opens_that_page");
}

/// **The notebook survives a restart, and the file is the shipped format.** A page written and
/// committed is on disk in the twelve records the client writes; a client started afresh with that
/// file beside it comes up with the page in it.
///
/// The file's name is the world's and the character's, which is what the next scenario is about.
pub fn a_journal_page_survives_a_restart() {
    let (written, path_name) = {
        let mut c = a_client_with_a_notebook("journal-save");
        playing_as(&mut c, JOURNAL_CHARACTER);
        let path = notebook_path(&c, JOURNAL_CHARACTER);
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .expect("the notebook has a name")
            .to_owned();

        open_the_page(&mut c, journal::PAGE);
        click_the_tab(&mut c, journal::PAGE, journal::PANEL);
        // Opening the tab has already written the blank page: both edges save.
        assert_eq!(c.view().expect_app().hud().panels.journal.page_saves, 1);
        assert!(
            path.exists(),
            "the file is on disk before a single character is typed"
        );

        write_in(&mut c, journal::LABEL_EDIT, "Hunt");
        write_in(&mut c, journal::TITLE_EDIT, "The Bandit Camp");
        write_in(&mut c, journal::NOTES_EDIT, "Two lives left");
        empty_the_box(&mut c, journal::DAYS_EDIT);
        write_in(&mut c, journal::DAYS_EDIT, "3");

        // Leaving the tab is what commits the page.
        click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
        assert_eq!(c.view().expect_app().hud().panels.journal.page_saves, 2);
        let text = std::fs::read_to_string(&path).expect("the client wrote the notebook");
        assert!(nothing_was_sent(&c), "the journal sends nothing");
        c.shutdown();
        (text, name)
    };

    let named = path_name == "Journal-Frostfell-Kupotest.txt";
    let in_format = written
        == "<NEWP>\n\
            <LABE> Hunt\n\
            <TITL> The Bandit Camp\n\
            <NOTE> Two lives left\n\
            <DAYS> 3\n\
            <HOUR> 0\n\
            <MINU> 0\n\
            <LOC?> FALSE\n\
            <LOCX> 0.000000\n\
            <LOCY> 0.000000\n\
            <TIM?> FALSE\n\
            <TIME> 0.000000\n\
            \n";

    // A client started afresh with that file beside it. It is a second client and not the same
    // one: a new element tree, a new panel, a new notebook.
    let mut c = a_client_with_a_notebook("journal-load");
    std::fs::write(notebook_path(&c, JOURNAL_CHARACTER), &written)
        .expect("the disposable settings directory is writable");
    playing_as(&mut c, JOURNAL_CHARACTER);
    let read_it = c.view().expect_app().hud().panels.journal.page_loads == 1;

    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    let came_back = el_text(&mut c, journal::TITLE_EDIT) == "The Bandit Camp"
        && el_text(&mut c, journal::LABEL_EDIT) == "Hunt"
        && el_text(&mut c, journal::NOTES_EDIT) == "Two lives left"
        && el_text(&mut c, journal::DAYS_EDIT) == "3"
        && el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 1 ~"
        && c.view().expect_app().hud().panels.journal.pages.len() == 1;
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.file.a-page-written-here-is-on-disk-in-the-shipped-format-and-comes-back",
        move |_| named && in_format && read_it && came_back && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_a_journal_page_survives_a_restart() {
    scenario("a_journal_page_survives_a_restart");
}

/// **The negative that makes the one above a measurement.** A client that does not know which
/// character it is playing writes **nothing at all** -- where one shared notebook for every
/// character on every world would pass the restart claim and be wrong in the way that matters
/// most.
pub fn a_client_that_does_not_know_its_character_writes_nothing() {
    let mut c = a_client_with_a_notebook("journal-nameless");
    let dir = c
        .scratch_settings()
        .expect("a settings directory")
        .dir()
        .to_path_buf();

    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    write_in(&mut c, journal::TITLE_EDIT, "Nobody");
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);

    let no_file = c.view().expect_app().hud().panels.journal.file.is_none();
    let nothing_written = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .all(|f| !f.file_name().to_string_lossy().starts_with("Journal-"));
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.file.a-client-that-does-not-know-its-character-writes-no-notebook",
        move |_| no_file && nothing_written && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_a_client_that_does_not_know_its_character_writes_nothing() {
    scenario("a_client_that_does_not_know_its_character_writes_nothing");
}

/// **A notebook in the shipped format is read and then written back in place.** One written by
/// another client -- with a record this client does not write and a note whose line break is
/// stored as a tab -- is parsed whole, and an edit lands in that same file.
pub fn a_notebook_in_the_shipped_format_is_read_and_written_back() {
    let mut c = a_client_with_a_notebook("journal-retail");
    let path = notebook_path(&c, JOURNAL_CHARACTER);
    let original = "<NEWP>\n\
                    <PNUM> 1\n\
                    <LABE> Old\n\
                    <TITL> Written by another client\n\
                    <NOTE> first\tsecond\n\
                    <DAYS> 0\n\
                    <HOUR> 2\n\
                    <MINU> 0\n\
                    <LOC?> TRUE\n\
                    <LOCX> 33.800000\n\
                    <LOCY> -42.200000\n\
                    <TIM?> FALSE\n\
                    <TIME> 0.000000\n\
                    \n";
    std::fs::write(&path, original).expect("the disposable settings directory is writable");
    playing_as(&mut c, JOURNAL_CHARACTER);

    let read_it = c.view().expect_app().hud().panels.journal.page_loads == 1;
    let parsed = {
        let p = &c.view().expect_app().hud().panels.journal.pages[0];
        p.title == "Written by another client"
            // The tab came back as a line break.
            && p.notes == "first\nsecond"
            && p.hours == 2
            && p.location_set
            && (p.ew - 33.8).abs() < 1e-6
            && (p.ns + 42.2).abs() < 1e-6
    };

    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    empty_the_box(&mut c, journal::TITLE_EDIT);
    write_in(&mut c, journal::TITLE_EDIT, "Written by this one");
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);

    let written = std::fs::read_to_string(&path).expect("the client wrote the file back");
    let in_place = written != original
        && written.contains("<TITL> Written by this one")
        // The record only the reader knows about is not one the writer writes.
        && !written.contains("<PNUM>");
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.file.a-notebook-in-the-shipped-format-is-read-and-written-back-in-place",
        move |_| read_it && parsed && in_place && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_a_notebook_in_the_shipped_format_is_read_and_written_back() {
    scenario("a_notebook_in_the_shipped_format_is_read_and_written_back");
}

/// **A second character gets a notebook of their own**, and the first character's is not written
/// over -- which is the worst failure this feature can have.
pub fn a_second_character_gets_its_own_notebook() {
    let mut c = a_client_with_a_notebook("journal-twochars");
    playing_as(&mut c, JOURNAL_CHARACTER);
    let first = notebook_path(&c, JOURNAL_CHARACTER);

    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    write_in(&mut c, journal::TITLE_EDIT, "Kupotest was here");
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let on_disk = std::fs::read_to_string(&first)
        .expect("the first character's page is on disk")
        .contains("Kupotest was here");

    // The next log-on, as somebody else.
    playing_as(&mut c, "Otherguy");
    let repointed = c
        .view()
        .expect_app()
        .hud()
        .panels
        .journal
        .file
        .as_deref()
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        == Some("Journal-Frostfell-Otherguy.txt");
    let fresh = c.view().expect_app().hud().panels.journal.pages.len() == 1
        && c.view().expect_app().hud().panels.journal.pages[0]
            .title
            .is_empty();

    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let untouched = std::fs::read_to_string(&first)
        .expect("the first character's notebook is still there")
        .contains("Kupotest was here");
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.file.a-second-character-gets-a-notebook-of-their-own",
        move |_| on_disk && repointed && fresh && untouched && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_a_second_character_gets_its_own_notebook() {
    scenario("a_second_character_gets_its_own_notebook");
}

/// A log-off and a log-on again, with the screen genuinely rebound in between.
fn relog_as(c: &mut HeadlessClient, character: &str) {
    c.app_mut().host_state_mut().entered_character = None;
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::CHARACTER_MANAGEMENT);
    c.tick(3);
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(4);
    playing_as(c, character);
    c.tick(3);
}

/// **A relog keeps each character's notebook, and saves the page that was still open.**
///
/// The page one character typed is on disk and off it again when they come back; the other
/// character's notebook is a different file and a blank one, and neither writes over the other.
/// And the page the player was still looking at when they logged off is there afterwards --
/// which is the edit a relog would otherwise throw away.
pub fn a_relog_keeps_each_characters_notebook() {
    let mut c = a_client_with_a_notebook("journal-relog");
    playing_as(&mut c, JOURNAL_CHARACTER);
    let first = notebook_path(&c, JOURNAL_CHARACTER);
    let second = notebook_path(&c, "Otherguy");
    let per_character = first != second;

    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    write_in(&mut c, journal::TITLE_EDIT, "Gharundim Dig");
    write_in(&mut c, journal::LABEL_EDIT, "Dig");
    write_in(&mut c, journal::NOTES_EDIT, "Second cellar, north wall");
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let on_disk = std::fs::read_to_string(&first).expect("the first notebook");
    let written = on_disk.contains("<TITL> Gharundim Dig") && !second.exists();

    // ...and as somebody else, a blank notebook that is not the first one.
    let loads = c.view().expect_app().hud().panels.journal.page_loads;
    relog_as(&mut c, "Otherguy");
    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    let blank = el_text(&mut c, journal::TITLE_EDIT).is_empty()
        && el_text(&mut c, journal::NOTES_EDIT).is_empty()
        && std::fs::read_to_string(&first).expect("still there") == on_disk
        // Nothing was read, so the blank page is a new one and not a read of somebody's file.
        && c.view().expect_app().hud().panels.journal.page_loads == loads;

    // ...and back again: the page comes off disk.
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    relog_as(&mut c, JOURNAL_CHARACTER);
    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    let came_back = c.view().expect_app().hud().panels.journal.page_loads == loads + 1
        && el_text(&mut c, journal::TITLE_EDIT) == "Gharundim Dig"
        && el_text(&mut c, journal::LABEL_EDIT) == "Dig"
        && el_text(&mut c, journal::NOTES_EDIT) == "Second cellar, north wall";

    // The page that was **still open** when the player logged off. The tab is left up: nothing
    // has committed the typing, and the relog is the only thing that can.
    empty_the_box(&mut c, journal::TITLE_EDIT);
    write_in(&mut c, journal::TITLE_EDIT, "Unsaved");
    let uncommitted = !std::fs::read_to_string(&first)
        .expect("the notebook")
        .contains("Unsaved");
    relog_as(&mut c, JOURNAL_CHARACTER);
    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    let saved_on_the_way_out = el_text(&mut c, journal::TITLE_EDIT) == "Unsaved"
        && std::fs::read_to_string(&first)
            .expect("the notebook")
            .contains("<TITL> Unsaved");

    c.assert_behaviour(
        "journal.file.a-relog-keeps-each-characters-notebook-and-saves-the-page-still-open",
        move |_| {
            per_character && written && blank && came_back && uncommitted && saved_on_the_way_out
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_relog_keeps_each_characters_notebook() {
    scenario("a_relog_keeps_each_characters_notebook");
}

// =============================================================================================
// titles.*
//
// The Titles tab draws the earned titles the shard sends: the message carrying the list reaches
// the panel, and the panel draws it.
// =============================================================================================

/// Three titles the shard says this character has earned, and the order their names sort in.
const WARRIOR: u32 = 8;
const ADVENTURER: u32 = 1;
const BLADEMASTER: u32 = 3;
const TITLES_EARNED: [u32; 3] = [WARRIOR, ADVENTURER, BLADEMASTER];
const TITLES_SORTED: [u32; 3] = [ADVENTURER, BLADEMASTER, WARRIOR];

/// A client on the character page's Titles tab, with a list of earned titles delivered through
/// the real arm.
fn a_client_on_the_titles_tab() -> (HeadlessClient, dereth_testkit::Peer) {
    use dereth_ui_screens::panels::titles;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let peer = dereth_testkit::Peer::attach_creating(&mut c, HOUSE_PLAYER);
    c.world_mut().player = Some(HOUSE_PLAYER);
    c.when(Inbound::message(
        &dereth_protocol::social::CharacterTitlesMessage {
            version: 1,
            display_title: WARRIOR,
            titles: TITLES_EARNED.to_vec(),
        },
    ));
    c.tick(2);
    open_the_page(&mut c, dereth_ui_screens::panels::remaining::CHARACTER_PAGE);
    click_the_tab(
        &mut c,
        dereth_ui_screens::panels::remaining::CHARACTER_PAGE,
        titles::PANEL,
    );
    (c, peer)
}

/// The titles panel, as the client holds it.
fn titles_panel(c: &HeadlessClient) -> &dereth_ui_screens::panels::titles::TitlesPanel {
    &c.view().expect_app().hud().panels.titles
}

/// A **real press** at the centre of a row, with the element the hit test chose handed back --
/// which is the list and never the row, because no row of a shipped list is a target of its own.
fn press_title_row(c: &mut HeadlessClient, h: ElemHandle) -> Option<ElementId> {
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    let hit = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.hit_test_screen(at.x, at.y)
    };
    c.when(Player::Click(Target::Point(at)));
    c.tick(2);
    let (ui, _) = gameplay_screen(c.app_mut());
    hit.and_then(|e| ui.node(e))
        .map(dereth_ui::ElementNode::element_id)
}

/// Whether the button that wears a title is armed.
fn title_button_armed(c: &mut HeadlessClient) -> bool {
    let b = titles_panel(c).button.expect("the button is bound");
    let (ui, _) = gameplay_screen(c.app_mut());
    dereth_ui_screens::panels::titles::button_enabled(ui, b)
}

/// Every request this client has made to wear a title.
fn title_requests(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::SetDisplayCharacterTitle(m) => Some(m.title_id),
            _ => None,
        })
        .collect()
}

/// **The gate.** The Titles tab draws one row per title the shard said the character has earned,
/// sorted by the name each resolves to rather than by its number, with the worn one named above
/// them -- and every row keeps its own title's number, which is what the send arm reads back.
pub fn the_titles_tab_draws_every_earned_title_sorted_by_name() {
    use dereth_ui_screens::panels::titles;

    let (mut c, _peer) = a_client_on_the_titles_tab();

    let (bound, templates, rebuilt, unresolved, shown, selected) = {
        let p = titles_panel(&c);
        (
            p.bound(),
            p.templates(),
            p.rebuilds,
            p.unresolved,
            p.shown(),
            p.selected,
        )
    };
    let list_is_right = bound
        && templates == 1
        // Rebuilt at least once, which is what tells an empty list from an unwired one.
        && rebuilt > 0
        && unresolved == 0
        && shown == TITLES_SORTED
        && selected.is_none();

    let rows: Vec<(u32, ElemHandle)> = titles_panel(&c)
        .rows
        .iter()
        .map(|r| (r.id, r.element))
        .collect();
    // An instrument that cannot look reports absence: without this the loop below is vacuous.
    let there_are_rows = rows.iter().map(|(id, _)| *id).collect::<Vec<_>>() == TITLES_SORTED;
    let display = titles_panel(&c)
        .display_text
        .expect("the worn title's line is bound");
    let (drawn, ids_on_the_rows, worn) = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let drawn: Vec<String> = rows
            .iter()
            .map(|(_, h)| {
                ui.get_child_recursive(*h, ElementId(titles::ROW_TEXT))
                    .and_then(|ch| ui.text_element_mut(ch))
                    .map_or_else(String::new, |t| t.glyphs.inq_text(false))
            })
            .collect();
        let ids = rows.iter().all(|(id, h)| {
            dereth_ui_screens::bind::attr_enum(ui, *h, titles::ATTR_TITLE_ID) == Some(*id)
        });
        let worn = ui
            .text_element_mut(display)
            .map(|t| t.glyphs.inq_text(false));
        (drawn, ids, worn)
    };
    let named =
        drawn == vec!["Adventurer", "Blademaster", "Warrior"] && worn.as_deref() == Some("Warrior");

    c.assert_behaviour(
        "titles.tab.the-tab-draws-every-earned-title-sorted-by-name",
        move |_| list_is_right && there_are_rows && named && ids_on_the_rows,
    );
    c.shutdown();
}

#[test]
fn scenario_the_titles_tab_draws_every_earned_title_sorted_by_name() {
    scenario("the_titles_tab_draws_every_earned_title_sorted_by_name");
}

/// Picking a row arms the button only when the title picked is not the one already worn -- and
/// picking sends nothing, which matters because a client that sent on a pick would rewrite the
/// worn title on every stray press.
pub fn picking_a_title_arms_the_button_and_sends_nothing() {
    use dereth_ui_screens::panels::titles;

    let (mut c, _peer) = a_client_on_the_titles_tab();
    let nothing_armed = !title_button_armed(&mut c);

    let worn = titles_panel(&c)
        .rows
        .iter()
        .position(|r| r.id == WARRIOR)
        .expect("it is drawn");
    let h = titles_panel(&c).rows[worn].element;
    let hit = press_title_row(&mut c, h);
    let on_the_list = hit == Some(titles::TITLE_LIST);
    let worn_picked = titles_panel(&c).selected == Some(worn)
        && !title_button_armed(&mut c)
        && title_requests(&c).is_empty();

    let other = titles_panel(&c)
        .rows
        .iter()
        .position(|r| r.id == ADVENTURER)
        .expect("it is drawn");
    let h = titles_panel(&c).rows[other].element;
    press_title_row(&mut c, h);
    let other_picked = titles_panel(&c).selected == Some(other)
        && title_button_armed(&mut c)
        && title_requests(&c).is_empty();

    c.assert_behaviour(
        "titles.tab.picking-a-row-arms-the-button-only-for-a-title-not-already-worn",
        move |_| nothing_armed && on_the_list && worn_picked && other_picked,
    );
    c.shutdown();
}

#[test]
fn scenario_picking_a_title_arms_the_button_and_sends_nothing() {
    scenario("picking_a_title_arms_the_button_and_sends_nothing");
}

/// **The gesture, end to end.** The button with nothing picked sends nothing; a row pressed and
/// then the button sends exactly one request carrying that row's own title -- and nothing local
/// moves, because which title is worn is the shard's to say.
pub fn the_display_button_puts_the_set_title_request_on_the_wire() {
    let (mut c, _peer) = a_client_on_the_titles_tab();
    let button = titles_panel(&c).button.expect("the button is bound");

    {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(button, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
    }
    c.tick(3);
    let idle = title_requests(&c).is_empty();

    let want = titles_panel(&c)
        .rows
        .iter()
        .position(|r| r.id == BLADEMASTER)
        .expect("it is drawn");
    let h = titles_panel(&c).rows[want].element;
    press_title_row(&mut c, h);
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(button, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
    }
    c.tick(3);

    let sent = title_requests(&c) == vec![BLADEMASTER];
    let framed = c.outbound_wire().contains(&0x0000_002C);
    // What that request encodes to, and the queue it goes out on.
    let bytes_and_queue = {
        let m = dereth_protocol::social::SocialSetDisplayCharacterTitle {
            title_id: BLADEMASTER,
        };
        let mut session = dereth_client_net::client_session::Session::new(
            dereth_client_net::client_session::testing::MockTransport::new(),
        );
        let ok = dereth_client::interaction::send_request(
            &mut session,
            &dereth_client_model::Request::SetDisplayCharacterTitle(m),
        );
        let packet = session.transport.sent.last().expect("one datagram").clone();
        let mut want = 0xF7B1_u32.to_le_bytes().to_vec();
        want.extend_from_slice(&1_u32.to_le_bytes());
        want.extend_from_slice(&0x0000_002C_u32.to_le_bytes());
        want.extend_from_slice(&BLADEMASTER.to_le_bytes());
        (ok, packet.payload == want, packet.queue)
    };

    // Nothing local moved: the header still says the title the shard last said was worn.
    let unchanged =
        c.view().expect_app().hud().display_title == WARRIOR && title_button_armed(&mut c);

    c.assert_behaviour(
        "titles.tab.the-button-sends-one-request-for-the-picked-title-and-nothing-local-moves",
        move |_| {
            idle && sent
                && framed
                && bytes_and_queue == (true, true, dereth_primitives::NetQueue::Weenie)
                && unchanged
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_display_button_puts_the_set_title_request_on_the_wire() {
    scenario("the_display_button_puts_the_set_title_request_on_the_wire");
}

// =============================================================================================
// advancement.raise.*
//
// The `+10` button on the attributes, vitals and skills pages is clickable whenever the unspent
// experience covers ten raises.
//
// **The button is reached by the pointer and by nothing else.** The shipped id for a ten-point
// button is carried twice in the live tree -- once under the skills page and once under the
// attributes page -- and each page has two footer containers of which its own state picks one, so
// a scenario that broadcast a click at an id would be pressing whichever one the walk found
// first. Every press here is a hit test at the button's own centre, and the element the hit test
// chose is asserted to be the one the page's own state names.
// =============================================================================================

/// How much unspent experience the character has.
fn set_available_experience(c: &mut HeadlessClient, n: i64) {
    let q = c
        .world_mut()
        .player_qualities_mut()
        .expect("the recorded description is on the player's own weenie");
    assert!(
        q.set(
            dereth_client_model::StatKey::new(
                dereth_client_model::StatType::Int64,
                dereth_client::hud::AVAILABLE_EXPERIENCE,
            ),
            dereth_client_model::StatValue::Int64(n),
        ),
        "a whole number is always storable"
    );
}

/// The footer child of `panel`'s **own** footer container -- the one the sub-panel's state names,
/// and not the first element in the tree carrying that id.
fn panel_footer_child(c: &mut HeadlessClient, panel: ElementId, child: u32) -> ElemHandle {
    let p = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        let page = ui
            .get_child_recursive(root, remaining::CHARACTER_PAGE)
            .expect("the character page is in the shipped layout");
        ui.get_child_recursive(page, panel)
            .expect("the sub-panel is in the shipped layout")
    };
    let (ui, _) = gameplay_screen(c.app_mut());
    let state = ui.node(p).map_or(0, |n| n.state.0);
    let container = ui
        .get_child_recursive(p, ElementId(statmgmt::Footer::container_for(state)))
        .expect("the footer container the panel's own state names");
    ui.get_child_recursive(container, ElementId(child))
        .expect("the footer child")
}

/// Whether the button says it is out of reach -- the attribute the button's own state writes and
/// the press gate reads.
fn button_is_disabled(c: &mut HeadlessClient, h: ElemHandle) -> Option<bool> {
    let (ui, _) = gameplay_screen(c.app_mut());
    dereth_ui_screens::bind::attr_bool(ui, h, statmgmt::ATTR_DISABLED)
}

fn element_state(c: &mut HeadlessClient, h: ElemHandle) -> u32 {
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(h).map(|n| n.state.0).expect("the element is alive")
}

/// Raise the character page and select whichever sub-panel `h` belongs to.
fn show_the_page_holding(c: &mut HeadlessClient, h: ElemHandle) {
    open_the_page(c, remaining::CHARACTER_PAGE);
    let sub = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let mut cur = Some(h);
        let mut found = None;
        while let Some(e) = cur {
            let id = ui.node(e).map(dereth_ui::ElementNode::element_id);
            if id == Some(skills::PANEL) || id == Some(attributes::PANEL) {
                found = id;
                break;
            }
            cur = ui.parent(e);
        }
        found
    };
    if let Some(sub) = sub {
        click_the_tab(c, remaining::CHARACTER_PAGE, sub);
    }
    c.tick(3);
}

/// Press one row of a list with the pointer, having scrolled it into view first.
fn press_the_row(c: &mut HeadlessClient, h: ElemHandle) {
    show_the_page_holding(c, h);
    {
        let app = c.app_mut();
        let mut panels = std::mem::take(&mut app.hud_mut().panels);
        {
            let (ui, _) = gameplay_screen(app);
            for w in [panels.skills.list.as_mut(), panels.attributes.list.as_mut()]
                .into_iter()
                .flatten()
            {
                if let Some(i) = w.index_of(h) {
                    w.scroll_to_view(ui, i);
                }
            }
        }
        app.hud_mut().panels = panels;
    }
    c.tick(1);
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    c.when(Player::Click(Target::Point(at)));
    c.tick(1);
}

/// A real press on an element, with the element the hit test chose handed back.
fn press_and_say_what_was_under_it(
    c: &mut HeadlessClient,
    h: ElemHandle,
) -> (Option<ElemHandle>, ScreenPoint) {
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    let hit = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.hit_test_screen(at.x, at.y)
    };
    c.when(Player::Click(Target::Point(at)));
    // Two frames: the press's own action becomes a request on the next frame's pass.
    c.tick(2);
    (hit, at)
}

/// Every raise this client has asked for, as `(what it names, how much)`.
fn raises_asked_for(c: &HeadlessClient) -> Vec<(u32, u32, u32)> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::TrainAttribute(m) => {
                Some((0x0045, m.attribute_id, m.xp_spent))
            }
            dereth_client_model::Request::TrainAttribute2nd(m) => {
                Some((0x0044, m.vital_id, m.xp_spent))
            }
            dereth_client_model::Request::TrainSkill(m) => Some((0x0046, m.skill_id, m.xp_spent)),
            _ => None,
        })
        .collect()
}

/// **All three pages at once.** One point short of what ten raises cost,
/// the ten-point button says it is out of reach and a real press on it sends nothing; with exactly
/// that much unspent experience it lights up, the pointer really lands on the page's own button,
/// and one press sends one raise naming that stat and that amount -- after which the button puts
/// itself back out of reach while it waits for the answer.
pub fn the_ten_point_button_lights_up_and_a_real_press_sends_the_raise() {
    let mut c = a_recorded_character(SESSION);
    let xp: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);

    // What the three cases are, worked out from the character's own record rather than pinned.
    let q = c
        .view()
        .world()
        .player_qualities()
        .expect("the recorded description")
        .clone();
    let strength = q.attribute(1).expect("the recorded character has strength");
    let strength_ten = dereth_client_model::advancement::attribute_cost_to_raise_10(
        &xp,
        strength.level_from_cp,
        strength.cp_spent,
        false,
    );
    let health = q
        .attribute_2nd(1)
        .expect("the recorded character has health");
    let health_ten = dereth_client_model::advancement::attribute_cost_to_raise_10(
        &xp,
        health.attribute.level_from_cp,
        health.attribute.cp_spent,
        true,
    );
    let skill = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| q.skill(r.skill).is_some_and(|s| s.sac >= 2))
        .map(|r| r.skill)
        .expect("the recorded character has a trained skill with a row");
    let skill_ten = dereth_client_model::advancement::skill_cost_to_raise_10(&q, &xp, skill);
    let costs_something = strength_ten > 0 && health_ten > 0 && skill_ten > 0;

    let mut every_case_holds = true;
    let cases: [(ElementId, u32, u32, u32); 3] = [
        (attributes::PANEL, 0x0045, 1, strength_ten),
        (attributes::PANEL, 0x0044, 1, health_ten),
        (skills::PANEL, 0x0046, skill, skill_ten),
    ];
    for (i, (panel, opcode, names, ten)) in cases.into_iter().enumerate() {
        // **A client of its own per half.** The footer is worked out when the row is selected, so
        // how much experience the character has must be set *before* the press -- and pressing a
        // row that is already selected is the player un-selecting it, which empties the footer.
        for enough in [false, true] {
            let mut c = a_recorded_character(SESSION);
            let row = match i {
                0 => attribute_row(&c, 1, false),
                1 => attribute_row(&c, 2, true),
                _ => skill_row(&c, skill),
            };
            set_available_experience(&mut c, i64::from(ten) - i64::from(!enough));
            press_the_row(&mut c, row);

            let button = panel_footer_child(&mut c, panel, statmgmt::child::BUTTON_10);
            let disabled = button_is_disabled(&mut c, button);
            let state = element_state(&mut c, button);
            let (hit, _) = press_and_say_what_was_under_it(&mut c, button);
            let asked = raises_asked_for(&c);
            // Whichever half this is, the pointer landed on *this* page's own button and not on
            // the other page's, nor on a container drawn over it.
            every_case_holds &= hit == Some(button);
            if enough {
                every_case_holds &= disabled == Some(false)
                    && state == statmgmt::button_state::ENABLED
                    && asked == vec![(opcode, names, ten)];
                // ...and the button puts itself back out of reach while it waits for an answer.
                let button = panel_footer_child(&mut c, panel, statmgmt::child::BUTTON_10);
                every_case_holds &= button_is_disabled(&mut c, button) == Some(true);
            } else {
                every_case_holds &= disabled == Some(true)
                    && state == statmgmt::button_state::DISABLED
                    && asked.is_empty();
            }
            c.shutdown();
        }
    }

    c.assert_behaviour(
        "advancement.raise.the-ten-point-button-lights-up-with-the-experience-and-a-press-sends-the-raise",
        move |_| costs_something && every_case_holds,
    );
    c.shutdown();
}

#[test]
fn scenario_the_ten_point_button_lights_up_and_a_real_press_sends_the_raise() {
    scenario("the_ten_point_button_lights_up_and_a_real_press_sends_the_raise");
}

// =============================================================================================
// The house commands
//
// This is the command table, not the House *pane*. It is here rather than with the chat subject
// because every claim it makes is about a house, which is what the `house.*` rows already are,
// and because the one claim it shares with the pane -- the two-stage abandon -- is already
// asserted by
// `house.abandon.two-confirmations-send-it-and-only-the-shards-answer-empties-the-pane`.
//
// The line is typed: `adapters_chat::Hand::say` clicks the chat entry, types one character at a
// time and presses return, which is the only route a player has to this ladder.
// =============================================================================================

use dereth_client_model::chat_cmd as house_cmd;

/// A client in the world with the shell up, for a scenario whose subject is a typed line.
fn a_client_for_house_commands() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay_in_world(4))
}

/// The text of every live bubble in the strip, in the order it holds them.
///
/// The strip is the observable and not a counter, because the refusals are drawn on the one
/// channel the chat windows' shipped filters drop and the strip's own accepts.
fn house_bubbles(c: &mut HeadlessClient) -> Vec<String> {
    let list = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let root = shell.flow.current().expect("a screen is current").roots()[0];
        shell
            .ui
            .get_child_recursive(root, dereth_ui_screens::hud::speech_bubbles::LIST_BOX)
            .expect("the bubble strip is in the shipped layout")
    };
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell
        .ui
        .children(list)
        .into_iter()
        .filter_map(|h| {
            shell
                .ui
                .text_element_mut(h)
                .map(|t| t.glyphs.inq_text(false))
        })
        .collect()
}

/// The chat log as its element really holds it -- what a player reads.
fn house_log_text(c: &mut HeadlessClient) -> String {
    let log = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let root = shell.flow.current().expect("a screen is current").roots()[0];
        shell
            .ui
            .get_child_recursive(root, dereth_ui_screens::chat::window::LOG)
            .expect("the chat log is in the shipped layout")
    };
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(log)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// A name packed the way the client packs one: a count, the characters, and padding to four.
fn house_pstr(s: &str) -> Vec<u8> {
    let mut v = u16::try_from(s.len())
        .expect("a short name")
        .to_le_bytes()
        .to_vec();
    v.extend_from_slice(s.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

fn house_dw(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}

/// The opcode each request of this family carries, written out here rather than read off the
/// codec, so that the table and the codec cannot drift together.
fn house_opcode_of(r: &dereth_client_model::Request) -> u32 {
    use dereth_client_model::Request as R;
    match r {
        R::AbandonHouse(_) => 0x021F,
        R::AddPermanentGuest(_) => 0x0245,
        R::RemovePermanentGuest(_) => 0x0246,
        R::SetOpenHouseStatus(_) => 0x0247,
        R::ChangeStoragePermission(_) => 0x0249,
        R::BootSpecificHouseGuest(_) => 0x024A,
        R::RemoveAllStoragePermission(_) => 0x024C,
        R::RequestFullGuestList(_) => 0x024D,
        R::AddAllStoragePermission(_) => 0x025C,
        R::RemoveAllPermanentGuests(_) => 0x025E,
        R::BootEveryone(_) => 0x025F,
        R::TeleToHouse(_) => 0x0262,
        R::SetHooksVisibility(_) => 0x0266,
        R::ModifyAllegianceGuestPermission(_) => 0x0267,
        R::ModifyAllegianceStoragePermission(_) => 0x0268,
        R::ListAvailableHouses(_) => 0x0270,
        R::TeleToMansion(_) => 0x0278,
        other => panic!("this family never builds {other:?}"),
    }
}

/// The datagram a request becomes: the ordered envelope, the stamp, the opcode, the body.
fn house_action_bytes(stamp: u32, opcode: u32, body: &[u8]) -> Vec<u8> {
    let mut v = 0xF7B1_u32.to_le_bytes().to_vec();
    v.extend_from_slice(&stamp.to_le_bytes());
    v.extend_from_slice(&opcode.to_le_bytes());
    v.extend_from_slice(body);
    v
}

// ---------------------------------------------------------------------------------------------
// house.commands.every-sub-command-that-sends-puts-its-own-message-on-the-wire
// ---------------------------------------------------------------------------------------------

/// Every typed line of the ladder that sends, with the bytes it sends. The counter the client
/// keeps of commands it does not know is asserted **not** to move, because that counter is where
/// a line the ladder misses lands.
pub fn every_house_sub_command_that_sends_puts_its_own_message_on_the_wire() {
    use dereth_client_model::Request as R;
    use dereth_protocol::trade as t;

    let cases: Vec<(&str, R, Vec<u8>)> = vec![
        (
            "@house open",
            R::SetOpenHouseStatus(t::HouseSetOpenHouseStatus { open: 1 }),
            house_dw(1),
        ),
        (
            "@house close",
            R::SetOpenHouseStatus(t::HouseSetOpenHouseStatus { open: 0 }),
            house_dw(0),
        ),
        (
            "@house recall",
            R::TeleToHouse(t::HouseTeleToHouse),
            Vec::new(),
        ),
        ("@house re", R::TeleToHouse(t::HouseTeleToHouse), Vec::new()),
        (
            "@house mansion_recall",
            R::TeleToMansion(t::HouseTeleToMansion),
            Vec::new(),
        ),
        (
            "@house alleg_recall",
            R::TeleToMansion(t::HouseTeleToMansion),
            Vec::new(),
        ),
        (
            "@house ma",
            R::TeleToMansion(t::HouseTeleToMansion),
            Vec::new(),
        ),
        (
            "@house hooks on",
            R::SetHooksVisibility(t::HouseSetHooksVisibility { visible: 1 }),
            house_dw(1),
        ),
        (
            "@house hooks off",
            R::SetHooksVisibility(t::HouseSetHooksVisibility { visible: 0 }),
            house_dw(0),
        ),
        (
            "@house boot Brenwick",
            R::BootSpecificHouseGuest(t::HouseBootSpecificHouseGuest {
                name: "Brenwick".into(),
            }),
            house_pstr("Brenwick"),
        ),
        (
            "@house remove Brenwick",
            R::BootSpecificHouseGuest(t::HouseBootSpecificHouseGuest {
                name: "Brenwick".into(),
            }),
            house_pstr("Brenwick"),
        ),
        (
            "@house boot_all",
            R::BootEveryone(t::HouseBootEveryone),
            Vec::new(),
        ),
        (
            "@house remove_all",
            R::BootEveryone(t::HouseBootEveryone),
            Vec::new(),
        ),
        (
            "@house boot -all",
            R::BootEveryone(t::HouseBootEveryone),
            Vec::new(),
        ),
        (
            "@house guest add Ash",
            R::AddPermanentGuest(t::HouseAddPermanentGuest { name: "Ash".into() }),
            house_pstr("Ash"),
        ),
        (
            "@house guest remove Ash",
            R::RemovePermanentGuest(t::HouseRemovePermanentGuest { name: "Ash".into() }),
            house_pstr("Ash"),
        ),
        (
            "@house guest remove_all",
            R::RemoveAllPermanentGuests(t::HouseRemoveAllPermanentGuests),
            Vec::new(),
        ),
        (
            "@house guest list",
            R::RequestFullGuestList(t::HouseRequestFullGuestList),
            Vec::new(),
        ),
        (
            "@house guest show",
            R::RequestFullGuestList(t::HouseRequestFullGuestList),
            Vec::new(),
        ),
        (
            "@house guest add_allegiance",
            R::ModifyAllegianceGuestPermission(t::HouseModifyAllegianceGuestPermission {
                allow: 1,
            }),
            house_dw(1),
        ),
        (
            "@house guest remove_allegiance",
            R::ModifyAllegianceGuestPermission(t::HouseModifyAllegianceGuestPermission {
                allow: 0,
            }),
            house_dw(0),
        ),
        (
            "@house storage add Ash",
            R::ChangeStoragePermission(t::HouseChangeStoragePermission {
                name: "Ash".into(),
                has_permission: 1,
            }),
            [house_pstr("Ash"), house_dw(1)].concat(),
        ),
        (
            "@house storage remove Ash",
            R::ChangeStoragePermission(t::HouseChangeStoragePermission {
                name: "Ash".into(),
                has_permission: 0,
            }),
            [house_pstr("Ash"), house_dw(0)].concat(),
        ),
        (
            "@house storage add -all",
            R::AddAllStoragePermission(t::HouseAddAllStoragePermission),
            Vec::new(),
        ),
        (
            "@house storage remove -all",
            R::RemoveAllStoragePermission(t::HouseRemoveAllStoragePermission),
            Vec::new(),
        ),
        (
            "@house storage remove_all",
            R::RemoveAllStoragePermission(t::HouseRemoveAllStoragePermission),
            Vec::new(),
        ),
        (
            "@house storage list",
            R::RequestFullGuestList(t::HouseRequestFullGuestList),
            Vec::new(),
        ),
        (
            "@house storage add_allegiance",
            R::ModifyAllegianceStoragePermission(t::HouseModifyAllegianceStoragePermission {
                allow: 1,
            }),
            house_dw(1),
        ),
        (
            "@house storage remove_allegiance",
            R::ModifyAllegianceStoragePermission(t::HouseModifyAllegianceStoragePermission {
                allow: 0,
            }),
            house_dw(0),
        ),
        (
            "@house available cottage",
            R::ListAvailableHouses(t::HouseListAvailableHouses { house_type: 1 }),
            house_dw(1),
        ),
        (
            "@hslist villa",
            R::ListAvailableHouses(t::HouseListAvailableHouses { house_type: 2 }),
            house_dw(2),
        ),
        (
            "@hslist MANSION",
            R::ListAvailableHouses(t::HouseListAvailableHouses { house_type: 3 }),
            house_dw(3),
        ),
        // The comparison is case-blind in every arm of the family, so case never matters.
        (
            "@hslist Apartment",
            R::ListAvailableHouses(t::HouseListAvailableHouses { house_type: 4 }),
            house_dw(4),
        ),
    ];

    let mut c = a_client_for_house_commands();
    let mut hand = dereth_testkit::adapters_chat::Hand::new();
    let mut session = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );
    let mut stamp = 1_u32;
    let mut every_line_holds = true;

    for (line, want, body) in &cases {
        let unimplemented = c.view().interaction().stats.chat_commands_unimplemented;
        let refused = c.view().interaction().stats.chat_commands_refused;
        let before = c.view().outbound().len();
        hand.say(&mut c, line);
        c.tick(1);

        let asked: Vec<dereth_client_model::Request> = c.view().outbound()[before..].to_vec();
        every_line_holds &=
            c.view().interaction().stats.chat_commands_unimplemented == unimplemented;
        every_line_holds &= c.view().interaction().stats.chat_commands_refused == refused;
        every_line_holds &= asked.len() == 1 && asked[0] == *want;

        // And what that request becomes on a datagram, through the client's own sender.
        every_line_holds &= dereth_client::interaction::send_request(&mut session, want);
        let packet = session
            .transport
            .sent
            .last()
            .expect("one datagram per request")
            .clone();
        every_line_holds &=
            (packet.queue, packet.ordered) == (dereth_primitives::NetQueue::Weenie, true);
        every_line_holds &=
            packet.payload == house_action_bytes(stamp, house_opcode_of(want), body);
        stamp += 1;
    }

    // The census: every opcode this ladder can reach without a question first. The one that is
    // behind the two-stage question has its own scenario.
    let mut opcodes: Vec<u32> = session
        .transport
        .sent
        .iter()
        .map(|p| u32::from_le_bytes(p.payload[8..12].try_into().expect("an opcode")))
        .collect();
    opcodes.sort_unstable();
    opcodes.dedup();
    let the_whole_family = opcodes
        == vec![
            0x0245, 0x0246, 0x0247, 0x0249, 0x024A, 0x024C, 0x024D, 0x025C, 0x025E, 0x025F, 0x0262,
            0x0266, 0x0267, 0x0268, 0x0270, 0x0278,
        ];

    c.assert_behaviour(
        "house.commands.every-sub-command-that-sends-puts-its-own-message-on-the-wire",
        move |_| every_line_holds && the_whole_family,
    );
    c.shutdown();
}

#[test]
fn scenario_every_house_sub_command_that_sends_puts_its_own_message_on_the_wire() {
    scenario("every_house_sub_command_that_sends_puts_its_own_message_on_the_wire");
}

// ---------------------------------------------------------------------------------------------
// house.commands.a-line-the-ladder-refuses-prints-the-shipped-sentence-and-sends-nothing
// ---------------------------------------------------------------------------------------------

/// Every refusal the ladder has, read off the strip rather than off a counter -- and none of them
/// is the client's own not-a-command sentence, which a client without the ladder prints instead
/// of any of these.
pub fn a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing() {
    let cases: Vec<(&str, &str)> = vec![
        ("@house", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@hou", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@house wibble", house_cmd::PLEASE_SEE_HELP_HOUSE),
        // Neither word after `hooks`, so it falls through rather than defaulting either way.
        ("@house hooks", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@house hooks maybe", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@house boot", house_cmd::PLEASE_SEE_HELP_HOUSE),
        // The guests' own sentence...
        ("@house guest add", house_cmd::SPECIFY_THE_GUESTS_NAME),
        ("@house guest remove", house_cmd::SPECIFY_THE_GUESTS_NAME),
        // ...and the storage's, which is a *different* one, and the reason `@house storage add`
        // with no name is not an accidental grant to everybody.
        ("@house storage add", house_cmd::SPECIFY_AN_ACTUAL_NAME),
        ("@house storage remove", house_cmd::SPECIFY_AN_ACTUAL_NAME),
        ("@house guest wibble", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@house storage wibble", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@hslist", house_cmd::PLEASE_SEE_HELP_HSLIST),
        ("@hslist castle", house_cmd::PLEASE_SEE_HELP_HSLIST),
        ("@house available", house_cmd::PLEASE_SEE_HELP_HSLIST),
    ];

    let mut c = a_client_for_house_commands();
    let mut hand = dereth_testkit::adapters_chat::Hand::new();
    let mut every_refusal_holds = true;

    for (line, want) in &cases {
        let before = c.view().outbound().len();
        hand.say(&mut c, line);
        c.tick(3);
        every_refusal_holds &= c.view().outbound().len() == before;
        let drawn = house_bubbles(&mut c);
        every_refusal_holds &= drawn.iter().any(|t| t.trim() == want.trim());
        every_refusal_holds &= !drawn
            .iter()
            .any(|t| t.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim());
    }

    // The reader is proved able to see the sentence it has just reported absent fifteen times:
    // there is one shipped command that really is refused that way, and it says so here.
    hand.say(&mut c, "@afk wibble");
    c.tick(3);
    let the_reader_can_see_it = house_bubbles(&mut c)
        .iter()
        .any(|t| t.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim());

    c.assert_behaviour(
        "house.commands.a-line-the-ladder-refuses-prints-the-shipped-sentence-and-sends-nothing",
        move |_| every_refusal_holds && the_reader_can_see_it,
    );
    c.shutdown();
}

#[test]
fn scenario_a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing() {
    scenario("a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing");
}

// ---------------------------------------------------------------------------------------------
// house.commands.the-help-listing-and-the-house-help-are-what-the-client-ships
// ---------------------------------------------------------------------------------------------

/// `/help` and `/help house`, typed with the slash a player types them with.
pub fn the_help_listing_and_the_house_help_are_what_the_client_ships() {
    let mut c = a_client_for_house_commands();
    let mut hand = dereth_testkit::adapters_chat::Hand::new();

    let unimplemented = c.view().interaction().stats.chat_commands_unimplemented;
    let before = c.view().outbound().len();
    hand.say(&mut c, "/help");
    c.tick(3);
    let a_slash_works = c.view().interaction().stats.chat_commands_unimplemented == unimplemented
        && c.view().outbound().len() == before;

    let log = house_log_text(&mut c);
    let listed = log.contains("Available help:")
        && [
            "@help allegiances - Commands to help you deal with your Allegiance.",
            "@help house - Commands that help you manage your house, including guest and storage management.",
            "@help commands - Lists all commands.",
            "Note: You may substitute a forward slash (/) for the at symbol (@).",
        ]
        .iter()
        .all(|line| log.contains(line));
    let not_refused = !house_bubbles(&mut c)
        .iter()
        .any(|t| t.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim());

    hand.say(&mut c, "/help house");
    c.tick(3);
    let log = house_log_text(&mut c);
    let house_help = log.contains("For more information, type @help")
        && [
            "@house abandon - Abandons your house.",
            "- Adds players to your house guest list.",
            "@house storage remove_all - Removes all storage permissions from guests.",
            "@house hooks on|off - Makes the hooks in your house visible or invisible.",
            "@house available - See @hslist",
        ]
        .iter()
        .all(|line| log.contains(line));

    hand.say(&mut c, "@help hslist");
    c.tick(3);
    let the_list_help =
        house_log_text(&mut c).contains("Types include: Apartment, Cottage, Villa, Mansion");

    hand.say(&mut c, "@help wibble");
    c.tick(3);
    let drawn = house_bubbles(&mut c);
    let an_unknown_word = drawn
        .iter()
        .any(|t| t.trim() == dereth_client_model::cmd::help::UNKNOWN_COMMAND)
        && !drawn
            .iter()
            .any(|t| t.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim());

    c.assert_behaviour(
        "house.commands.the-help-listing-and-the-house-help-are-what-the-client-ships",
        move |_| {
            a_slash_works && listed && not_refused && house_help && the_list_help && an_unknown_word
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_help_listing_and_the_house_help_are_what_the_client_ships() {
    scenario("the_help_listing_and_the_house_help_are_what_the_client_ships");
}

// ---------------------------------------------------------------------------------------------
// house.guests.the-guest-list-the-shard-sends-back-is-written-out-on-the-chat-log
// ---------------------------------------------------------------------------------------------

/// The guest table the shard sends back, laid out by hand so that this crate's own writer cannot
/// define the oracle, and read off the chat log's own element.
fn guest_table_body(guests: &[(u32, i32, &str)]) -> Vec<u8> {
    let mut b = 0x1000_0002_u32.to_le_bytes().to_vec(); // the version the recording carries
    b.extend_from_slice(&0_u32.to_le_bytes()); // bitmask
    b.extend_from_slice(&0_u32.to_le_bytes()); // the monarch
                                               // The table header: the count in the low half, the bucket count in the high half.
    b.extend_from_slice(
        &u32::try_from(guests.len())
            .expect("a short table")
            .to_le_bytes()[..2],
    );
    b.extend_from_slice(&64_u16.to_le_bytes());
    for (id, storage, name) in guests {
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(&storage.to_le_bytes());
        b.extend_from_slice(&house_pstr(name));
    }
    b.extend_from_slice(&0_u32.to_le_bytes()); // an empty roommate list
    while b.len() % 4 != 0 {
        b.push(0);
    }
    b
}

/// What the player asked for with `@house guest list`, arriving.
pub fn the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log() {
    let (mut c, mut peer) = a_house_client();

    // The empty table first, which is the shape a recorded session carries.
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_HAR,
        &guest_table_body(&[]),
    );
    let log = house_log_text(&mut c);
    let empty_table = c.view().expect_app().hud().stats.house_har_updates == 1
        && c.view().expect_app().hud().stats.house_har_guests == 0
        && log.contains("Guests:")
        && log.contains("None")
        // The client sends no roommate flag, so that heading is never drawn.
        && !log.contains("Roommates:");

    // And a populated one, with the mark on the guest that may use the storage and not the other.
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_HAR,
        &guest_table_body(&[(0x5000_0014, 0, "Plain"), (0x5000_0015, 1, "Trusted")]),
    );
    let log = house_log_text(&mut c);
    let two_guests = c.view().expect_app().hud().stats.house_har_updates == 2
        && c.view().expect_app().hud().stats.house_har_guests == 2
        && log.contains("Plain")
        && log.contains("Trusted *")
        && !log.contains("Plain *");

    // The lines themselves, pinned on the model as well, so the text is not read only through the
    // glyph reader.
    let one_guest = dereth_protocol::trade::Har {
        version: dereth_protocol::trade::Har::CURRENT_VERSION,
        bitmask: 0,
        monarch_iid: dereth_primitives::ObjectId(0),
        guest_table: dereth_protocol::archive::PackedHash {
            table_size: 64,
            entries: vec![(
                1,
                dereth_protocol::trade::GuestInfo {
                    item_storage_permission: 1,
                    char_name: "Trusted".into(),
                },
            )],
        },
        roommate_list: None,
    };
    let composed = dereth_client_model::housing::har_dump(&one_guest, false)
        == "Guests:\n  Trusted *\n"
        && dereth_client_model::housing::har_dump(&dereth_protocol::trade::Har::default(), false)
            == "Guests:\n  None\n";

    c.assert_behaviour(
        "house.guests.the-guest-list-the-shard-sends-back-is-written-out-on-the-chat-log",
        move |_| empty_table && two_guests && composed,
    );
    c.shutdown();
}

#[test]
fn scenario_the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log() {
    scenario("the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log");
}

// ---------------------------------------------------------------------------------------------
// house.available.the-free-houses-the-shard-lists-are-written-out-with-their-locations
// ---------------------------------------------------------------------------------------------

/// What the player asked for with `@hslist`, arriving -- with the three branches the answer has.
pub fn the_free_houses_the_shard_lists_are_written_out_with_their_locations() {
    /// Holtburg's landcell, which is west and south of the origin.
    const HOLTBURG: u32 = 0xA9B4_0025;

    let (mut c, mut peer) = a_house_client();

    let mut body = house_dw(2); // a villa
    body.extend_from_slice(&house_dw(1)); // one landcell
    body.extend_from_slice(&house_dw(HOLTBURG));
    body.extend_from_slice(&3_i32.to_le_bytes());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_AVAILABLE_HOUSES,
        &body,
    );
    let (ew, ns) = dereth_physics::landdefs::gid_to_lcoord(dereth_primitives::CellId(HOLTBURG))
        .expect("Holtburg is a real landcell");
    let want = dereth_client_model::housing::coord_line(ew, ns);
    let log = house_log_text(&mut c);
    let listed = c.view().expect_app().hud().stats.house_available_houses == 1
        && c.view()
            .expect_app()
            .hud()
            .stats
            .house_available_coord_lines
            == 1
        && log.contains("There are 3 villas available.")
        && log.contains(want.trim())
        && !log.contains("too many houses");

    // An apartment listing gives the count and stops, however large the count is.
    let coords = c
        .view()
        .expect_app()
        .hud()
        .stats
        .house_available_coord_lines;
    let mut body = house_dw(4);
    body.extend_from_slice(&house_dw(1));
    body.extend_from_slice(&house_dw(HOLTBURG));
    body.extend_from_slice(&500_i32.to_le_bytes());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_AVAILABLE_HOUSES,
        &body,
    );
    let log = house_log_text(&mut c);
    let apartments = c.view().expect_app().hud().stats.house_available_houses == 2
        && c.view()
            .expect_app()
            .hud()
            .stats
            .house_available_coord_lines
            == coords
        && log.contains("There are 500 apartments available.")
        && !log.contains("too many houses");

    // ...and the cut-off, on a kind that does list its places.
    let mut body = house_dw(1); // a cottage
    body.extend_from_slice(&house_dw(1));
    body.extend_from_slice(&house_dw(HOLTBURG));
    body.extend_from_slice(&401_i32.to_le_bytes());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_AVAILABLE_HOUSES,
        &body,
    );
    let cut_off =
        house_log_text(&mut c).contains("Only the first 400 locations are displayed here.");

    // The boundary and the kind the client has no word for, pinned on the composer so that the
    // exact numbers are readable.
    let boundary = !dereth_client_model::housing::available_houses_truncated(400)
        && dereth_client_model::housing::available_houses_truncated(401)
        && dereth_client_model::housing::available_houses_header(0, 7)
            == "There are 7  available.\n";

    c.assert_behaviour(
        "house.available.the-free-houses-the-shard-lists-are-written-out-with-their-locations",
        move |_| listed && apartments && cut_off && boundary,
    );
    c.shutdown();
}

#[test]
fn scenario_the_free_houses_the_shard_lists_are_written_out_with_their_locations() {
    scenario("the_free_houses_the_shard_lists_are_written_out_with_their_locations");
}
// =============================================================================================
// attributes.selection.* / skills.selection.* / skills.rows.* / skills.numbers.* /
// attributes.rows.* / advancement.raise.*
//
// The character pages: a row on either page can be selected, the attribute rows carry icons, the
// skills page draws its values, and numbers carry a thousands separator.
//
// **Every gesture on a row here is the pointer.** A press synthesized on the row element would
// bypass the real producer.
//
// A row created from the list template does not accept mouse hits, so it cannot source the
// message. The type-5 list box is the hit target. Its handler finds the row under the mouse
// instead of reading a row from the message source.
//
// The message source therefore cannot substitute for the item-at-point result.
// Both selection scenarios assert the element that hit testing actually chose.
//
// **Selection is asserted apart from the spend**: the two
// selection scenarios emit nothing and say so, and the raise has a scenario of its own that first
// presses the button with nothing picked and requires silence.
// =============================================================================================

use dereth_ui_screens::panels::{abuse, numfmt};

/// Raise the character page with `sub` up -- **unless it is already up**, because the tab is a
/// toggle and pressing it again would move the page to the other sub-panel. This is
/// [`show_the_skills_page`]'s guard with the sub-panel as a parameter.
fn o422_show_the_page(c: &mut HeadlessClient, sub: ElementId) {
    open_the_page(c, remaining::CHARACTER_PAGE);
    let already_up = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        ui.get_child_recursive(root, sub)
            .is_some_and(|h| ui.is_visible(h))
    };
    if !already_up {
        click_the_tab(c, remaining::CHARACTER_PAGE, sub);
    }
    c.tick(2);
}

/// A real press at the middle of `h`, with **the shipped id of the element the hit test chose**
/// handed back -- which on a list row is never the row.
fn o422_press(c: &mut HeadlessClient, h: ElemHandle) -> Option<ElementId> {
    let (hit, _) = press_and_say_what_was_under_it(c, h);
    let (ui, _) = gameplay_screen(c.app_mut());
    hit.and_then(|e| ui.node(e))
        .map(dereth_ui::ElementNode::element_id)
}

/// The shipped id of the element `h` is.
fn o422_id_of(c: &mut HeadlessClient, h: ElemHandle) -> Option<ElementId> {
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(h).map(dereth_ui::ElementNode::element_id)
}

/// One child of a list row, as text.
fn o422_row_text(c: &mut HeadlessClient, row: ElemHandle, child: u32) -> String {
    let h = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.get_child_recursive(row, ElementId(child))
    };
    h.map_or_else(String::new, |h| glyph_runs(c.app_mut(), h).0)
}

/// The skills footer's own child, as text. [`footer_child`] resolves it the way the panel does.
fn o422_footer_text(c: &mut HeadlessClient, child: u32) -> String {
    let h = footer_child(c.app_mut(), child);
    glyph_runs(c.app_mut(), h).0
}

/// Press the skills footer's raise button, **when the footer the panel's own state chose carries
/// one at all** -- with nothing picked the default container has no raise button, and the answer
/// is then `false` rather than a panic.
///
/// The raise action is reached from the button's message. The
/// button under test in the *other* scenario -- the ten-point one -- is
/// the one whose claim is about what the pointer lands on. Two frames afterwards, because the
/// action the press queues is turned into a request by the next frame's pass.
fn o422_press_the_raise_button(c: &mut HeadlessClient) -> bool {
    let button = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        ui.get_child_recursive(root, remaining::CHARACTER_PAGE)
            .and_then(|page| ui.get_child_recursive(page, skills::PANEL))
            .and_then(|panel| {
                let state = ui.node(panel).map_or(0, |n| n.state.0);
                ui.get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))
                    .and_then(|k| ui.get_child_recursive(k, ElementId(statmgmt::child::BUTTON)))
            })
    };
    if let Some(h) = button {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    }
    c.tick(2);
    button.is_some()
}

/// **Selection on the attributes page.** A real press picks the row under the pointer,
/// the press lands on the list rather than on the row, the footer names what was picked, and the
/// toggle is an edge in both directions. Nothing is sent.
pub fn a_press_picks_the_attribute_row_under_it_and_lands_on_the_list() {
    let mut c = a_recorded_character(SESSION);
    o422_show_the_page(&mut c, attributes::PANEL);

    let rows: Vec<(u32, ElemHandle)> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .attributes
        .rows
        .iter()
        .map(|r| (r.stat, r.element))
        .collect();
    // Six primaries and three vitals, and the first of them is Strength.
    let the_nine_rows = rows.len() == 9 && rows.first().map(|(s, _)| *s) == Some(1);
    // The selected-row index starts at -1, which is not the same number as row zero.
    let nothing_is_picked_to_start =
        c.view().expect_app().hud().panels.attributes.selected_index == -1;
    let sent_before = c.outbound_opcodes();

    let row = rows[0].1;
    let row_id = o422_id_of(&mut c, row);
    let hit = o422_press(&mut c, row);
    // Asserted rather than assumed: the press never lands on the row.
    let it_landed_on_the_list = hit != row_id && hit == Some(attributes::LIST_BOX);

    let panel = &c.view().expect_app().hud().panels.attributes;
    let it_picked_strength =
        panel.selected_index == 0 && panel.selected().map(|r| r.stat) == Some(1);
    let the_footer_names_it = panel.footer_content.title == "Strength: 30";

    // The selection-update loop puts the picked row in state 6 and
    // every other row to state 1.
    let the_rows_are_drawn_picked_and_not = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let states: Vec<u32> = rows
            .iter()
            .map(|(_, e)| ui.node(*e).map_or(0, |n| n.state.0))
            .collect();
        states[0] == statmgmt::row_state::SELECTED
            && states[1..]
                .iter()
                .all(|s| *s == statmgmt::row_state::UNSELECTED)
    };

    // The toggle is an **edge**, so it is driven three times: a harness that stops at the first
    // press cannot tell a toggle from a one-way latch.
    let _ = o422_press(&mut c, row);
    let pressing_it_again_un_picks_it = {
        let panel = &c.view().expect_app().hud().panels.attributes;
        panel.selected_index == -1 && panel.footer_content.title == "Select an Attribute to Improve"
    };
    let _ = o422_press(&mut c, row);
    let and_a_third_press_picks_it_again =
        c.view().expect_app().hud().panels.attributes.selected_index == 0;

    // Nothing was spent. This scenario's subject is the picking alone, and "nothing" is every
    // message the client framed and not only a raise.
    let nothing_was_sent = c.outbound_opcodes() == sent_before;

    c.assert_behaviour(
        "attributes.selection.a-press-picks-the-row-under-the-pointer-through-the-list-itself",
        move |_| {
            the_nine_rows
                && nothing_is_picked_to_start
                && it_landed_on_the_list
                && it_picked_strength
                && the_footer_names_it
                && the_rows_are_drawn_picked_and_not
                && pressing_it_again_un_picks_it
                && and_a_third_press_picks_it_again
                && nothing_was_sent
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_picks_the_attribute_row_under_it_and_lands_on_the_list() {
    scenario("a_press_picks_the_attribute_row_under_it_and_lands_on_the_list");
}

/// **The same press on the other page, and only on that one.** The attribute and skill panels
/// both carry a `0x1000023D` list box at the **same screen rectangle**; the
/// attributes panel is offered the message first, so without `ListBoxWidget::owns` it claims every
/// press meant for a skill row. Both halves are asserted, because "a skill got picked" alone would
/// pass with the two panels' answers swapped in exactly one direction.
pub fn a_press_picks_the_skill_row_under_it_and_the_attribute_page_stays_put() {
    let mut c = a_recorded_character(SESSION);
    o422_show_the_page(&mut c, skills::PANEL);

    let rows: Vec<(u32, ElemHandle)> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| (r.skill, r.element))
        .collect();
    let the_recorded_skills = rows.len() == 38;
    let nothing_is_picked_to_start = c.view().expect_app().hud().panels.skills.selected_skill == 0;
    let the_other_page_before = c.view().expect_app().hud().panels.attributes.selected_index;
    let sent_before = c.outbound_opcodes();

    let (skill, row) = rows[0];
    let hit = o422_press(&mut c, row);
    let it_landed_on_the_list = hit == Some(skills::LIST_BOX);
    let it_picked_that_skill = c.view().expect_app().hud().panels.skills.selected_skill == skill;
    let the_other_page_stayed_put =
        c.view().expect_app().hud().panels.attributes.selected_index == the_other_page_before;

    let _ = o422_press(&mut c, row);
    let pressing_it_again_un_picks_it =
        c.view().expect_app().hud().panels.skills.selected_skill == 0;

    let nothing_was_sent = c.outbound_opcodes() == sent_before;

    c.assert_behaviour(
        "skills.selection.a-press-picks-the-skill-row-under-it-and-the-other-page-stays-put",
        move |_| {
            the_recorded_skills
                && nothing_is_picked_to_start
                && it_landed_on_the_list
                && it_picked_that_skill
                && the_other_page_stayed_put
                && pressing_it_again_un_picks_it
                && nothing_was_sent
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_picks_the_skill_row_under_it_and_the_attribute_page_stays_put() {
    scenario("a_press_picks_the_skill_row_under_it_and_the_attribute_page_stays_put");
}

/// **The other half of the pair, and the one that spends.** The raise button is pressed
/// *before* anything is picked and must be silent; then a trained skill is picked, which must also
/// be silent by itself; then one press must ask for exactly one raise, naming that skill and the
/// cost the footer showed.
///
/// Without the first half a green result would say nothing about whether the gate exists at all.
pub fn a_raise_sends_nothing_until_a_skill_row_is_picked() {
    let mut c = a_recorded_character(SESSION);
    o422_show_the_page(&mut c, skills::PANEL);
    let sent_before = c.outbound_opcodes();

    // With nothing picked the default container carries no raise button at all, and the panel's
    // raise action returns false when the selected skill is zero. Either way: silence.
    let nothing_is_picked_to_start = c.view().expect_app().hud().panels.skills.selected_skill == 0;
    let _ = o422_press_the_raise_button(&mut c);
    let silent_with_nothing_picked = c.outbound_opcodes() == sent_before;

    // A **trained** skill, whose arm is the one that sends the raise.
    let (skill, row) = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| r.group == skills::SkillGroup::Trained)
        .map(|r| (r.skill, r.element))
        .expect("the recorded character has a trained skill with a row");
    let _ = o422_press(&mut c, row);
    let the_pick_is_the_gate = c.view().expect_app().hud().panels.skills.selected_skill == skill;
    let picking_is_silent_too = c.outbound_opcodes() == sent_before;

    // What the footer said the raise would cost, read before the press that spends it.
    let shown_cost = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .footer_content
        .line_one_value
        .clone();
    let the_button_was_there = o422_press_the_raise_button(&mut c);

    // One press, one message -- and the opcode is the one the **production sender** wrote onto the
    // blob, not a number typed here twice.
    let after = c.outbound_opcodes();
    let one_message = after.len() == sent_before.len() + 1 && after.last().copied() == Some(0x0046);
    // And that one message names the skill that was picked and carries the cost that was shown.
    let asked = raises_asked_for(&c);
    let it_names_the_skill_and_the_cost = asked.len() == 1
        && asked[0].0 == 0x0046
        && asked[0].1 == skill
        && statmgmt::num(asked[0].2) == shown_cost;

    c.assert_behaviour(
        "advancement.raise.a-raise-sends-nothing-until-a-row-is-picked-and-then-names-it-and-the-cost",
        move |_| {
            nothing_is_picked_to_start
                && silent_with_nothing_picked
                && the_pick_is_the_gate
                && picking_is_silent_too
                && the_button_was_there
                && one_message
                && it_names_the_skill_and_the_cost
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_raise_sends_nothing_until_a_skill_row_is_picked() {
    scenario("a_raise_sends_nothing_until_a_skill_row_is_picked");
}

/// The 38 skill rows of the recording's character as they are **drawn**, in list order:
/// `(SkillTable id, label, value cell)`.
///
/// **The independent oracle for the value column.** These are literals taken out of
/// the running panel against that recording, not restatements of anything the panel computes: no
/// symbol here is one the production code reads, so a change to the skill query, to which field
/// `write_row` writes, to the group ordering or to the row-to-skill mapping reddens it. The
/// assertion it replaces read the drawn cell back against `SkillRow::value`, and both sides are
/// written from `SkillEntry::effective` on one line of `write_row` -- one number compared with
/// itself, which would pass unchanged if all 38 rows were five too low.
///
/// **Where the numbers come from, independently of this build.** The character carries no
/// enchantments and `vitae_value() == 1.0`, but does carry `JACK_OF_ALL_TRADES` (property 326)
/// = 1, which adds **+5** to the non-raw value for each of these 38 rows. So every value
/// here is the character's base skill plus five, and *Melee Defense* is pinned separately at
/// 72 raw, 77 drawn.
const O422_FIRST_LOGIN_WALK_JUMP_SKILL_CELLS: [(u32, &str, &str); 38] = [
    // Specialized
    (52, "Dirty Fighting", "58"),
    (49, "Dual Wield", "82"),
    (46, "Finesse Weapons", "82"),
    (14, "Arcane Lore", "30"),
    (21, "Healing", "63"),
    (32, "Item Enchantment", "28"),
    (22, "Jump", "75"),
    (23, "Lockpick", "63"),
    (36, "Loyalty", "10"),
    (15, "Magic Defense", "20"),
    (6, "Melee Defense", "77"),
    (47, "Missile Weapons", "60"),
    (24, "Run", "110"),
    (40, "Salvaging", "10"),
    // Trained
    (29, "Armor Tinkering", "50"),
    (27, "Assess Creature", "5"),
    (19, "Assess Person", "5"),
    (20, "Deception", "5"),
    (44, "Heavy Weapons", "48"),
    (18, "Item Tinkering", "85"),
    (35, "Leadership", "5"),
    (45, "Light Weapons", "48"),
    (30, "Magic Item Tinkering", "65"),
    (7, "Missile Defense", "45"),
    (48, "Shield", "70"),
    (41, "Two Handed Combat", "48"),
    (28, "Weapon Tinkering", "50"),
    // Untrained and unusable
    (38, "Alchemy", "5"),
    (39, "Cooking", "5"),
    (31, "Creature Enchantment", "5"),
    (37, "Fletching", "5"),
    (33, "Life Magic", "5"),
    (16, "Mana Conversion", "5"),
    (50, "Recklessness", "5"),
    (51, "Sneak Attack", "5"),
    (54, "Summoning", "5"),
    (43, "Void Magic", "5"),
    (34, "War Magic", "5"),
];

/// **Every skill row draws its value, and the glyphs name a real font.**
///
/// The blank column was one wrong argument. Text composition selects a font from property
/// `0x1A` with a font index, then a colour from property `0x1B` with a colour index. The row
/// update supplies font index **0** and the computed colour index. This build had passed the
/// computed colour index as the glyph's font index instead.
///
/// That index could exceed the font array and resolve to `DataId(0)`, which rasterises nothing.
/// All 38 values were invisible.
///
/// The shipped layout is the second, independent reading: the value element `0x1000012B` declares
/// **one** font and **three** colours, and the label `0x1000012A`, which never takes the index,
/// declares one of each.
pub fn every_skill_row_draws_its_value_in_a_font_that_exists() {
    let mut c = a_recorded_character(SESSION);
    o422_show_the_page(&mut c, skills::PANEL);

    let rows: Vec<(u32, i32, u32, ElemHandle)> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| (r.skill, r.value, r.font, r.element))
        .collect();
    let the_denominator = rows.len() == 38;

    let mut every_row_draws_a_value = true;
    let mut cells: Vec<(u32, String, String)> = Vec::new();
    for (skill, value, colour_index, row) in &rows {
        let label = o422_row_text(&mut c, *row, skills::row::LABEL);
        let drawn = o422_row_text(&mut c, *row, skills::row::VALUE);
        every_row_draws_a_value &= !label.is_empty();
        // **Not the value check** -- the value is pinned against the table below. `SkillRow::value`
        // and the drawn cell are written from `SkillEntry::effective` on the same line, so this can
        // only catch the two *drifting apart*, as would happen if a cell were rewritten without
        // touching the cache.
        every_row_draws_a_value &= drawn == value.to_string();
        cells.push((*skill, label, drawn));

        let (ui, _) = gameplay_screen(c.app_mut());
        let v = ui
            .get_child_recursive(*row, ElementId(skills::row::VALUE))
            .expect("the row has a value element");
        let want = statmgmt::font_color_at(ui, v, *colour_index)
            .unwrap_or_else(|| panic!("skill {skill}: the row declares no colour {colour_index}"));
        let sb = ui.screen_box(v);
        let placed = ui
            .text_element_mut(v)
            .map_or_else(Vec::new, |t| t.compose(sb));
        every_row_draws_a_value &= !placed.is_empty()
            // The half that was broken: the glyphs must resolve to a font, not to `DataId(0)`.
            && placed.iter().all(|p| p.font.0 != 0)
            && placed.iter().all(|p| p.color == want);
    }

    // The independent oracle. Compared as one vector so the **order** is pinned too:
    // The rebuild makes four groups back to front and re-sorts them, and a row that
    // moved would still match a per-skill lookup.
    let want: Vec<(u32, String, String)> = O422_FIRST_LOGIN_WALK_JUMP_SKILL_CELLS
        .iter()
        .map(|(id, name, value)| (*id, (*name).to_string(), (*value).to_string()))
        .collect();
    let the_recorded_cells = cells == want;
    let and_it_agrees_with_the_other_pin = cells
        .iter()
        .find(|(id, _, _)| *id == 6)
        .map(|(_, _, v)| v.as_str())
        == Some("77");

    // The three colours, as literals out of the shipped layout rather than read back through the
    // same accessor that wrote them -- and the label, which never takes the index, as the
    // discriminating pair.
    let the_shipped_colours = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let v = ui
            .get_child_recursive(rows[0].3, ElementId(skills::row::VALUE))
            .expect("the row has a value element");
        let l = ui
            .get_child_recursive(rows[0].3, ElementId(skills::row::LABEL))
            .expect("the row has a label element");
        statmgmt::font_count(ui, v) == 1
            && statmgmt::font_color_at(ui, v, 0) == Some(0xFFFF_FFFF)
            && statmgmt::font_color_at(ui, v, 1) == Some(0xFF00_FF00)
            && statmgmt::font_color_at(ui, v, 2) == Some(0xFFFF_0000)
            && statmgmt::font_color_at(ui, v, 3).is_none()
            && statmgmt::font_count(ui, l) == 1
            && statmgmt::font_color_at(ui, l, 1).is_none()
    };

    // The font helper's out-of-range rule is exercised directly because **no production
    // call site supplies an out-of-range font index any more** -- `write_row` passes 0. Without
    // this the clamp is unfalsifiable, which is what a surviving mutation said.
    let an_index_past_the_end_falls_back = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let v = ui
            .get_child_recursive(rows[0].3, ElementId(skills::row::VALUE))
            .expect("the row has a value element");
        let wrote = statmgmt::set_text_with_font(ui, v, "45", 9, 9);
        let glyphs: Vec<(u32, u32)> = ui.text_element_mut(v).map_or_else(Vec::new, |t| {
            t.glyphs.glyphs.iter().map(|g| (g.font, g.color)).collect()
        });
        let sb = ui.screen_box(v);
        let placed = ui
            .text_element_mut(v)
            .map_or_else(Vec::new, |t| t.compose(sb));
        wrote
            && glyphs == vec![(0, 0xFFFF_FFFF), (0, 0xFFFF_FFFF)]
            && !placed.is_empty()
            && placed.iter().all(|p| p.font.0 != 0)
    };

    c.assert_behaviour(
        "skills.rows.every-row-draws-its-value-in-a-font-that-exists-and-the-colour-it-declares",
        move |_| {
            the_denominator
                && every_row_draws_a_value
                && the_recorded_cells
                && and_it_agrees_with_the_other_pin
                && the_shipped_colours
                && an_index_past_the_end_falls_back
        },
    );
    c.shutdown();
}

#[test]
fn scenario_every_skill_row_draws_its_value_in_a_font_that_exists() {
    scenario("every_skill_row_draws_its_value_in_a_font_that_exists");
}

/// **Every attribute row draws an icon, and it is that row's own icon.**
///
/// Initialization performs **nine** icon lookups, one immediately before each row is constructed:
/// group `0x10000002` for the six primaries
/// and `0x10000003` for the three vitals, with the stat id as the enum value. One fixed lookup,
/// *"one icon for every row"*, is the wrong reading; nine **distinct** ids is what makes it
/// false, and it is asserted rather than described.
///
/// The oracle is the shipped master mapper, read here rather than through the panel.
pub fn every_attribute_row_draws_the_icon_its_own_group_names() {
    let mut c = a_recorded_character(SESSION);
    o422_show_the_page(&mut c, attributes::PANEL);

    let rows: Vec<(u32, bool, ElemHandle)> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .attributes
        .rows
        .iter()
        .map(|r| (r.stat, r.secondary, r.element))
        .collect();
    let the_nine_rows = rows.len() == 9;

    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let mut seen: std::collections::BTreeSet<dereth_primitives::DataId> =
        std::collections::BTreeSet::new();
    let mut every_row_draws_its_own = true;
    for (stat, secondary, row) in &rows {
        let group = if *secondary {
            attributes::ICON_GROUP_2ND
        } else {
            attributes::ICON_GROUP
        };
        let want = dereth_assets::did_by_enum(&*store, group, *stat)
            .unwrap_or_else(|| panic!("group {group:#010X} value {stat} resolves to nothing"));
        let label = o422_row_text(&mut c, *row, skills::row::LABEL);
        let icon = {
            let (ui, _) = gameplay_screen(c.app_mut());
            ui.get_child_recursive(*row, ElementId(skills::row::ICON))
                .and_then(|h| ui.node(h))
                .and_then(|n| n.region.image.as_ref().map(|g| g.did))
        };
        every_row_draws_its_own &= !label.is_empty() && icon == Some(want);
        seen.insert(want);
    }
    let nine_distinct_icons = seen.len() == 9;
    // The two groups, written out rather than read back through the same constant the panel uses.
    let the_two_enum_groups =
        attributes::ICON_GROUP == 0x1000_0002 && attributes::ICON_GROUP_2ND == 0x1000_0003;

    c.assert_behaviour(
        "attributes.rows.every-row-draws-the-icon-its-own-group-names",
        move |_| {
            the_nine_rows && every_row_draws_its_own && nine_distinct_icons && the_two_enum_groups
        },
    );
    c.shutdown();
}

#[test]
fn scenario_every_attribute_row_draws_the_icon_its_own_group_names() {
    scenario("every_attribute_row_draws_the_icon_its_own_group_names");
}

/// **Every large number in the character panels carries the client's own separator.**
///
/// The separator is not written here twice: [`dereth_ui_screens::panels::numfmt`] takes it from the
/// shipped language record, and this scenario pins it as a literal against the number formatter's
/// `NUMBERFMTA` configuration -- two readings that could have
/// disagreed and do not. The *values* come out of the recording's own character description.
///
/// The three grouped sites are all here: unassigned experience (footer line two), total
/// experience and experience to the next level (the header). A four-digit and a seven-digit case
/// are exercised, and a skill row's own value is the asymmetric case that separates "we grouped
/// the right things" from "we grouped everything" -- it is a `%d` in the client and must not be
/// grouped.
pub fn the_character_pages_numbers_carry_the_shipped_separator() {
    let mut c = a_recorded_character(SESSION);
    o422_show_the_page(&mut c, skills::PANEL);

    // ---- the separator itself, pinned two ways -----------------------------------------------
    let g = numfmt::shipped();
    let the_separator = g.separator == ","
        && g.size == 3
        && g.separator == numfmt::XPTOSTRING_THOUSAND_SEP
        && g.size == numfmt::XPTOSTRING_GROUPING
        // The boundaries, written out rather than derived, so a wrong `size` cannot hide.
        && numfmt::group(999, &g) == "999"
        && numfmt::group(1_000, &g) == "1,000"
        && numfmt::group(1_234_567, &g) == "1,234,567"
        // The shipped language record supplies the negative-number format. These results prove
        // that its selected separator is applied to negative values rather than merely assumed.
        && g.negative_format == "-%s"
        && numfmt::group(-1_234, &g) == "-1,234"
        && numfmt::group(-999, &g) == "-999";

    // ---- the recording's own numbers, on screen ----------------------------------------------
    let q = recorded_qualities(SESSION);
    let int64 = |k: u32| -> i64 {
        match q.get(dereth_client_model::StatKey::new(
            dereth_client_model::StatType::Int64,
            k,
        )) {
            Some(dereth_client_model::StatValue::Int64(v)) => v,
            other => panic!("the recorded description carries no Int64 {k}: {other:?}"),
        }
    };
    let available = int64(dereth_client::hud::AVAILABLE_EXPERIENCE);
    let total = int64(dereth_client::hud::TOTAL_EXPERIENCE);
    let four_digit_numbers_to_read = available >= 1_000 && total >= 1_000;

    let header = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .header_content
        .clone();
    let the_header = header.total_xp == numfmt::group(total, &g)
        && header.total_xp.contains(',')
        && (header.xp_to_level.contains(',') || header.xp_to_level == "Infinity!");

    // Footer line two is `Unassigned Experience`, the third grouped site. It is read out of
    // the live element rather than off the panel's own record.
    let shown = o422_footer_text(&mut c, statmgmt::child::LINE_TWO_VALUE);
    let the_footer = shown == numfmt::group(available, &g) && shown.contains(',');

    // The same after a pick, because the selection footer is a different function.
    let row = c.view().expect_app().hud().panels.skills.rows[0].element;
    let _ = o422_press(&mut c, row);
    let shown = o422_footer_text(&mut c, statmgmt::child::LINE_TWO_VALUE);
    let the_selection_footer_too = shown == numfmt::group(available, &g);

    // A row **value** is `%d` in the client and must *not* be grouped.
    let a_row_value_is_left_plain = !o422_row_text(&mut c, row, skills::row::VALUE).contains(',');

    c.assert_behaviour(
        "skills.numbers.the-experience-numbers-are-grouped-with-the-shipped-separator",
        move |_| {
            the_separator
                && four_digit_numbers_to_read
                && the_header
                && the_footer
                && the_selection_footer_too
                && a_row_value_is_left_plain
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_character_pages_numbers_carry_the_shipped_separator() {
    scenario("the_character_pages_numbers_carry_the_shipped_separator");
}

// =============================================================================================
// abuse.response.* / abuse.report.*
//
// Retail keeps `0x04B8`..`0x04BA` silent in the failure-event handler. The shared arm emits an
// abuse-report response notice, and the abuse-report panel changes the shipped result text when
// it receives that notice.
//
// It adds no chat line, and it neither opens nor closes the window.
//
// **The window is opened by its own bound action.** The shipped UI action is otherwise unbound,
// so the action itself goes in rather than a key, which is `crate::input_steps`' rule -- what key
// an action is bound to is the keymap's claim and has
// rows of its own, so a scenario that pressed a key would be asserting over the shipped binding as
// well as over what the action does.
// =============================================================================================

/// `UICommands`' `Show/Hide Abuse Panel`, the action the shipped keymap leaves user-bindable.
const P1_41_TOGGLE_ABUSE: dereth_input::ActionId = dereth_input::ActionId(0x1000_0003);

/// Fire that action once, and run the frames the window it opens needs to reach the screen.
fn p1_41_toggle_the_window(c: &mut HeadlessClient) {
    dereth_testkit::input_steps::press_on_map(
        c,
        P1_41_TOGGLE_ABUSE,
        dereth_testkit::input_steps::UI_COMMANDS,
    );
    c.tick(2);
}

/// The shipped abuse-report panel, taken from its own binding rather than found by a tree walk.
fn p1_41_window(c: &HeadlessClient) -> ElemHandle {
    c.view()
        .expect_app()
        .hud()
        .panels
        .abuse
        .panel
        .expect("the shipped classic_gameplay layout carries a reporting panel and it is bound")
}

/// One child of that window.
fn p1_41_child(c: &HeadlessClient, id: ElementId) -> ElemHandle {
    let window = p1_41_window(c);
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(window, id)
        .unwrap_or_else(|| panic!("the shipped abuse window has no child {id:?}"))
}

/// What one of its children says.
fn p1_41_text(c: &mut HeadlessClient, id: ElementId) -> String {
    let h = p1_41_child(c, id);
    glyph_runs(c.app_mut(), h).0
}

/// One shipped sentence, by the `ID_*` name the window looks it up under.
///
/// The table is the one the mapper names for the window's own enum, and the id is the same hash the
/// client computes from the name -- neither is written down here.
#[allow(deprecated)]
fn p1_41_shipped_sentence(c: &HeadlessClient, token: &str) -> String {
    let table = dereth_ui_screens::env::did_by_enum(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        4,
        abuse::STRING_TABLE_ENUM,
    )
    .expect("the shipped string-table mapper row");
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .resolve_string(table, dereth_ui::persist::preferences::token_of(token))
        .unwrap_or_else(|| panic!("{token} is in the shipped string table"))
}

/// Whether the window is on screen.
fn p1_41_window_is_up(c: &HeadlessClient) -> bool {
    let window = p1_41_window(c);
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .is_visible(window)
}

/// The state an element of the window is in -- which page the window is on, or whether a button is
/// out of reach.
fn p1_41_state(c: &HeadlessClient, h: ElemHandle) -> dereth_ui::StateId {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("the element is alive")
        .state
}

/// A real press on one of the window's own controls, with **whether the pointer could reach it**
/// handed back: a press that landed on something drawn over the control would be measuring nothing.
fn p1_41_press(c: &mut HeadlessClient, id: ElementId) -> bool {
    let h = p1_41_child(c, id);
    let (at, reached) = {
        let view = c.view();
        let ui = &view.expect_app().ui().expect("the UI shell is up").ui;
        let b = ui.screen_clip_box(h);
        assert!(
            b.is_valid(),
            "{id:?} is clipped to nothing, so no pointer can reach it"
        );
        let p = ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        let hit = ui.hit_test_screen(p.x, p.y);
        (p, hit.is_some_and(|x| x == h || ui.is_ancestor_of(h, x)))
    };
    c.when(Player::Click(Target::Point(at)));
    c.tick(1);
    reached
}

/// Every ordered game action this client really put on a datagram since the last look, envelope
/// stripped: `[sub-type][body]`.
///
/// It is the wire and not the frame's outbox on purpose -- the report's own bytes are what is
/// pinned, and a request has no bytes until it is framed.
fn p1_41_ordered_actions(c: &mut HeadlessClient) -> Vec<Vec<u8>> {
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
            let p = f.payload;
            if f.header.queue_id == 3
                && p.len() >= 12
                && u32::from_le_bytes(p[0..4].try_into().expect("four bytes"))
                    == dereth_testkit::outbound::ORDERED_ACTION
            {
                out.push(p[8..].to_vec());
            }
        }
    }
    out
}

/// One `CommunicationWeenieError`, delivered the way the session layer delivers one.
fn p1_41_shard_answers(c: &mut HeadlessClient, code: u32) {
    deliver(
        c,
        &dereth_protocol::comms::CommunicationWeenieError { error_type: code },
    );
    c.tick(2);
}

/// **The three answers reach the window and stop there.** The result line becomes the shipped
/// sentence for the answer, nothing is said in chat, an answer with no sentence leaves the line
/// alone, and neither opening nor closing the window is part of it -- so an answer that arrives
/// while the window is shut is waiting there when the player opens it again.
pub fn the_shards_abuse_answer_writes_the_result_line_and_nothing_in_chat() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));

    let it_starts_shut = !p1_41_window_is_up(&c);
    p1_41_toggle_the_window(&mut c);
    let the_action_opens_it = p1_41_window_is_up(&c);

    let no_such_character = p1_41_shipped_sentence(&c, "ID_Abuse_Response_NoSuchCharacter");
    let before = p1_41_text(&mut c, abuse::RESULT_TEXT);
    p1_41_shard_answers(&mut c, abuse::response::NO_SUCH_CHARACTER);
    let line = p1_41_text(&mut c, abuse::RESULT_TEXT);
    let the_first_answer = line != before && line == no_such_character;
    let and_nothing_was_said_in_chat = c.chat_lines().is_empty();

    // There is no visibility gate on the receiver. Shut the window, prove the next answer still
    // writes the same child, and that it did not re-open the window to do it.
    p1_41_toggle_the_window(&mut c);
    let the_action_shuts_it_too = !p1_41_window_is_up(&c);
    let self_report = p1_41_shipped_sentence(&c, "ID_Abuse_Response_Self");
    p1_41_shard_answers(&mut c, abuse::response::SELF_REPORT);
    let a_shut_window_still_receives =
        !p1_41_window_is_up(&c) && p1_41_text(&mut c, abuse::RESULT_TEXT) == self_report;

    // The default arm is a true no-op, not a write of the empty string.
    p1_41_shard_answers(&mut c, 0);
    let an_unknown_answer_changes_nothing = p1_41_text(&mut c, abuse::RESULT_TEXT) == self_report;

    // The success arm reaches the same place through the failure that carries free-form text, and
    // that text is not what the window draws.
    let success = p1_41_shipped_sentence(&c, "ID_Abuse_Response_Success");
    deliver(
        &mut c,
        &dereth_protocol::comms::CommunicationWeenieErrorWithString {
            error_type: abuse::response::SUCCESS,
            text: "not part of this response".to_owned(),
        },
    );
    c.tick(2);
    let the_success_sentence = p1_41_text(&mut c, abuse::RESULT_TEXT) == success;

    p1_41_toggle_the_window(&mut c);
    let the_last_answer_is_waiting =
        p1_41_window_is_up(&c) && p1_41_text(&mut c, abuse::RESULT_TEXT) == success;
    let and_still_nothing_in_chat = c.chat_lines().is_empty();

    c.assert_behaviour(
        "abuse.response.the-shards-answer-writes-the-result-line-and-nothing-in-chat",
        move |_| {
            it_starts_shut
                && the_action_opens_it
                && the_first_answer
                && and_nothing_was_said_in_chat
                && the_action_shuts_it_too
                && a_shut_window_still_receives
                && an_unknown_answer_changes_nothing
                && the_success_sentence
                && the_last_answer_is_waiting
                && and_still_nothing_in_chat
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_shards_abuse_answer_writes_the_result_line_and_nothing_in_chat() {
    scenario("the_shards_abuse_answer_writes_the_result_line_and_nothing_in_chat");
}

/// **The window, driven the way a player drives it.** The name comes off whoever is selected, the
/// complaint is typed, Continue sends one report and only then, the window waits for the shard, and
/// Done empties it again.
pub fn the_abuse_page_sends_one_report_and_then_empties_itself() {
    const REPORTER: ObjectId = ObjectId(0x5000_0041);
    const TARGET: ObjectId = ObjectId(0x5000_0042);

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // A socket-free endpoint with one established connection, so that what the window sends is
    // framed into a datagram this scenario can read. Nothing binds and nothing leaves.
    let _shard = Peer::attach(&mut c, REPORTER);
    {
        let mut reporter = dereth_client_model::Weenie::new(REPORTER);
        reporter.pwd.bitfield |= dereth_client_model::weenie::bitfield::PLAYER;
        reporter.pwd.name = "Reporter".into();
        let mut target = dereth_client_model::Weenie::new(TARGET);
        target.pwd.bitfield |= dereth_client_model::weenie::bitfield::PLAYER;
        target.pwd.name = "Target Player".into();
        let w = c.world_mut();
        w.tables.weenies.insert(REPORTER, reporter);
        w.tables.weenies.insert(TARGET, target);
        w.player = Some(REPORTER);
        w.selected = Some(TARGET);
    }
    c.tick(2);
    // Whatever the login and the first frames put on the wire is not this scenario's.
    let _ = p1_41_ordered_actions(&mut c);

    p1_41_toggle_the_window(&mut c);
    let the_window_is_up = p1_41_window_is_up(&c);
    let window = p1_41_window(&c);
    // The window comes up on neither of its two numbered pages -- the shipped layout gives it no
    // state at all until something moves it -- and what matters here is that it is not already on
    // the entry page, or the advance below would be no advance.
    let it_does_not_start_on_the_entry_page = p1_41_state(&c, window) != abuse::PAGE_TWO;

    // Page one's own button: it advances to the entry page and copies the selected player's name.
    let pressed_the_name_button = p1_41_press(&mut c, abuse::SELECTED_NAME_BUTTON);
    let it_advanced = p1_41_state(&c, window) == abuse::PAGE_TWO;
    let it_took_the_selected_name = p1_41_text(&mut c, abuse::NAME_ENTRY) == "Target Player";

    // The complaint, typed into the box the caret was put in.
    let pressed_the_complaint_box = p1_41_press(&mut c, abuse::COMPLAINT_ENTRY);
    c.when(Player::Type("Repeated unwanted tells".to_owned()));
    c.tick(1);
    let the_complaint_is_there =
        p1_41_text(&mut c, abuse::COMPLAINT_ENTRY) == "Repeated unwanted tells";
    let continue_button = p1_41_child(&c, abuse::CONTINUE_BUTTON);
    // Both fields non-empty is what arms Continue.
    let continue_is_armed =
        p1_41_state(&c, continue_button) == dereth_ui::widgets::button::state::NORMAL;
    let typing_alone_sends_nothing = p1_41_ordered_actions(&mut c).is_empty();

    let pressed_continue = p1_41_press(&mut c, abuse::CONTINUE_BUTTON);
    c.tick(3);
    let actions = p1_41_ordered_actions(&mut c);

    // The literal oracle: the sub-type, the reported name, the status word, and the complaint.
    let mut expected = Vec::new();
    expected.extend_from_slice(&0x0140_u32.to_le_bytes());
    expected.extend_from_slice(&13_u16.to_le_bytes());
    expected.extend_from_slice(b"Target Player");
    expected.push(0);
    expected.extend_from_slice(&1_u32.to_le_bytes());
    expected.extend_from_slice(&23_u16.to_le_bytes());
    expected.extend_from_slice(b"Repeated unwanted tells");
    expected.extend_from_slice(&[0, 0, 0]);
    let one_exact_report = actions == vec![expected];

    let wait_text = p1_41_shipped_sentence(&c, "ID_Abuse_PageThree_WaitText");
    let it_says_it_is_waiting = p1_41_text(&mut c, abuse::RESULT_TEXT) == wait_text;

    let success = p1_41_shipped_sentence(&c, "ID_Abuse_Response_Success");
    p1_41_shard_answers(&mut c, abuse::response::SUCCESS);
    let the_answer_replaces_the_wait_text = p1_41_text(&mut c, abuse::RESULT_TEXT) == success;

    let pressed_done = p1_41_press(&mut c, abuse::DONE_BUTTON);
    let it_went_back_to_page_one = p1_41_state(&c, window) == abuse::PAGE_ONE;
    let both_fields_are_empty = p1_41_text(&mut c, abuse::NAME_ENTRY).is_empty()
        && p1_41_text(&mut c, abuse::COMPLAINT_ENTRY).is_empty();
    let continue_button = p1_41_child(&c, abuse::CONTINUE_BUTTON);
    let continue_is_out_of_reach_again =
        p1_41_state(&c, continue_button) == dereth_ui::widgets::button::state::DISABLED;
    let done_sends_nothing_more = p1_41_ordered_actions(&mut c).is_empty();

    c.assert_behaviour(
        "abuse.report.the-page-sends-one-report-naming-who-and-why-and-then-empties-itself",
        move |_| {
            the_window_is_up
                && it_does_not_start_on_the_entry_page
                && pressed_the_name_button
                && it_advanced
                && it_took_the_selected_name
                && pressed_the_complaint_box
                && the_complaint_is_there
                && continue_is_armed
                && typing_alone_sends_nothing
                && pressed_continue
                && one_exact_report
                && it_says_it_is_waiting
                && the_answer_replaces_the_wait_text
                && pressed_done
                && it_went_back_to_page_one
                && both_fields_are_empty
                && continue_is_out_of_reach_again
                && done_sends_nothing_more
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_abuse_page_sends_one_report_and_then_empties_itself() {
    scenario("the_abuse_page_sends_one_report_and_then_empties_itself");
}
// =============================================================================================
// skills.scroll.* and skills.rows.* (the number and the colour a row draws)
//
// Every row of the skills list is reachable to a real click once the list is scrolled: the list's
// item-at-point lookup reads the same scroll offset the element's scrollable behavior moves the
// rows by, rather than a copy of its own that nothing writes. A row draws the non-raw skill
// query, the **enchanted** total, and its colour compares the raw value against the effective
// value minus the vitae modifier.
//
// The shipped `0x1000023D` names the scrollbar `0x1000023E` in attribute `0x72`, and the thumb is
// sized and positioned from the same offset. A second, silent copy of that one number is worse
// than an absent one: the two can only ever disagree, and the hit test could read the dead one.
// Hence the thumb geometry and the hit test are asserted as two claims and not one.
// =============================================================================================

/// The stat-management list's vertical scrollbar, attribute `0x72` on `0x1000023D` in the shipped
/// `classic_gameplay` layout. Read back off the live tree below rather than assumed.
const O458_SCROLLBAR: ElementId = ElementId(0x1000_023E);

/// The skill id for *Melee Defense*, the row the value claims are pinned against: the
/// recorded character's first specialised skill, and a number well clear of zero.
const O458_MELEE_DEFENSE: u32 = 6;
/// *Healing*, and *Jump* -- the second is chosen because `70 * 0.95` is exactly `66.5`, so
/// rounding half up and cutting short disagree on it.
const O458_HEALING: u32 = 21;
const O458_JUMP: u32 = 22;

/// The live `0x1000023D` under the skill panel.
fn o458_list_box(c: &mut HeadlessClient) -> ElemHandle {
    let (ui, screen) = gameplay_screen(c.app_mut());
    let root = screen.root().expect("the gameplay screen has a root");
    let page = ui
        .get_child_recursive(root, remaining::CHARACTER_PAGE)
        .expect("the character page is in the shipped layout");
    let panel = ui
        .get_child_recursive(page, skills::PANEL)
        .expect("the skills panel is in the layout");
    ui.get_child_recursive(panel, skills::LIST_BOX)
        .expect("the skills list box is in the layout")
}

/// The element's own scrollable behavior: the offset that really moves the rows and the
/// content extent written when the scrollable area is resized.
fn o458_scrollable(c: &mut HeadlessClient, lb: ElemHandle) -> dereth_ui::scrollable::Scrollable {
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(lb)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| (**b).as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .expect("0x1000023D carries a list-box element behaviour")
        .scroll
}

/// The live vertical scroll offset, read off the element and never off a second copy.
fn o458_scroll_y(c: &mut HeadlessClient, lb: ElemHandle) -> i32 {
    o458_scrollable(c, lb).y
}

/// The bar the list really bound through its relative-element lookup. A bar
/// that is the list's *sibling* would not be found by a walk down from the panel.
fn o458_bar(c: &mut HeadlessClient, lb: ElemHandle) -> ElemHandle {
    let s = o458_scrollable(c, lb);
    let (ui, _) = gameplay_screen(c.app_mut());
    s.scrollbar(ui, lb, false)
        .expect("the skills list names a vertical bar of its own")
}

fn o458_bar_float(c: &mut HeadlessClient, bar: ElemHandle, attr: u32) -> f32 {
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(bar)
        .and_then(|n| n.merged_properties().get_float(attr))
        .unwrap_or_else(|| panic!("the bar carries no float attribute {attr:#04x}"))
}

fn o458_bar_bool(c: &mut HeadlessClient, bar: ElemHandle, attr: u32) -> Option<bool> {
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(bar)
        .and_then(|n| n.merged_properties().get_bool(attr))
}

/// One of the bar's two arrows, named by the bar's increment or decrement button id rather than
/// by a transcribed id.
fn o458_arrow(c: &mut HeadlessClient, bar: ElemHandle, down: bool) -> ElemHandle {
    // **The shipped bar's two attributes are named the other way round from what they do here.**
    // The one it calls the increment names the arrow drawn at the *head* of the bar, and that is
    // the one that moves the list up. Which is which is therefore read off the bar and then
    // asserted by where each is drawn and by which way the list really goes -- never assumed from
    // the name, which is exactly the mistake this comment exists to stop.
    let which = if down {
        dereth_ui::widgets::scrollbar::attr::DECREMENT_BUTTON
    } else {
        dereth_ui::widgets::scrollbar::attr::INCREMENT_BUTTON
    };
    let (ui, _) = gameplay_screen(c.app_mut());
    let id = ui
        .node(bar)
        .and_then(|n| n.merged_properties().get_enum(which))
        .map(ElementId)
        .expect("the bar names that arrow");
    ui.get_child_recursive(bar, id)
        .expect("the arrow is a child of the bar")
}

/// One press of one arrow, through the arrow **button** the bar names, so the client picks which
/// message that arrow raises rather than the scenario picking it.
///
/// **One press per fire.** The bar auto-repeats while an arrow is held, so this measures the
/// per-press scroll delta and not the repeat rate.
fn o458_press_arrow(c: &mut HeadlessClient, bar: ElemHandle, down: bool) {
    let h = o458_arrow(c, bar, down);
    let (cx, cy) = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    // **A press and no frame after it.** The arrows are buttons and the bar repeats while one is
    // held, so a press, a frame and a release move the list *twice*; one press per call is what
    // makes the step below a measurement of the scroll delta rather than of the repeat rate.
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
}

/// Scroll one item into view; report whether the viewport moved.
fn o458_bring_into_view(c: &mut HeadlessClient, index: usize) -> bool {
    let app = c.app_mut();
    let mut panels = std::mem::take(&mut app.hud_mut().panels);
    let moved = {
        let (ui, _) = gameplay_screen(app);
        panels
            .skills
            .list
            .as_mut()
            .expect("the skills list is bound")
            .scroll_to_view(ui, index)
    };
    app.hud_mut().panels = panels;
    c.tick(1);
    moved
}

/// The centre of one list **item** -- headers included, so the index is the list's own.
fn o458_item_centre(c: &mut HeadlessClient, index: usize) -> ScreenPoint {
    let item = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .list
        .as_ref()
        .expect("the skills list is bound")
        .items
        .get(index)
        .copied()
        .unwrap_or_else(|| panic!("the skills list has no item {index}"));
    let (ui, _) = gameplay_screen(c.app_mut());
    let b = ui.screen_box(item);
    ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// **The discriminator.** What the band walk would name for this point with the scroll offset
/// dropped -- the answer the defect gave. If it is the item really under the pointer then a
/// scrolled station proves nothing, so it is taken before the press and asserted to differ.
fn o458_band_walk_without_the_offset(c: &mut HeadlessClient, lb: ElemHandle, y: i32) -> usize {
    let heights = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .list
        .as_ref()
        .expect("the skills list is bound")
        .item_heights
        .clone();
    let (ui, _) = gameplay_screen(c.app_mut());
    let local = y - ui.screen_box(lb).y0;
    let mut acc = 0i32;
    let mut idx = 0usize;
    for (i, h) in heights.iter().enumerate() {
        acc += *h;
        if local <= acc {
            idx = i;
            break;
        }
    }
    idx
}

/// Which skill rows are **drawn** under a point, from geometry alone -- an oracle that never
/// calls the list's item-at-point lookup.
fn o458_rows_drawn_under(c: &mut HeadlessClient, at: ScreenPoint) -> Vec<u32> {
    let rows: Vec<(u32, ElemHandle)> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| (r.skill, r.element))
        .collect();
    let (ui, _) = gameplay_screen(c.app_mut());
    rows.iter()
        .filter(|(_, h)| {
            let b = ui.screen_box(*h);
            at.y >= b.y0 && at.y <= b.y1 && at.x >= b.x0 && at.x <= b.x1
        })
        .map(|(s, _)| *s)
        .collect()
}

/// The skill the page has selected. `0` is "none"; selecting the current row again toggles it off.
fn o458_selected(c: &HeadlessClient) -> u32 {
    c.view().expect_app().hud().panels.skills.selected_skill
}

/// The three numbers used to build one row: the raw value, the enchanted/effective total, and the
/// vitae modifier, in that order.
fn o458_skill_numbers(c: &HeadlessClient, skill: u32) -> (i32, i32, i32) {
    let e = c
        .view()
        .expect_app()
        .hud()
        .skills
        .iter()
        .find(|e| e.id == skill)
        .unwrap_or_else(|| panic!("skill {skill} is in the retail SkillTable"));
    (e.level, e.effective, e.vitae)
}

/// What one row's value cell **draws**: the text, and the index into the element's own `0x1B`
/// colour array -- so the claim reads as the composition style's colour index, not as an ARGB
/// word. `3` is returned when the colour is none of the three the element declares.
fn o458_value_cell(c: &mut HeadlessClient, skill: u32) -> (String, u32) {
    let row = skill_row(c, skill);
    let cell = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.get_child_recursive(row, ElementId(skills::row::VALUE))
            .expect("the row draws its number in the shipped 0x1000012B")
    };
    let (text, runs) = glyph_runs(c.app_mut(), cell);
    let colour = runs.first().map(|(_, col)| *col);
    let (ui, _) = gameplay_screen(c.app_mut());
    let index = colour
        .and_then(|col| (0..3u32).find(|i| statmgmt::font_color_at(ui, cell, *i) == Some(col)))
        .unwrap_or(3);
    (text, index)
}

/// The colour each row was drawn in, over the whole page, so that "plain" is a property of the
/// panel and not of one row.
fn o458_fonts(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.font)
        .collect()
}

fn o458_by_font(c: &HeadlessClient, f: u32) -> usize {
    o458_fonts(c).into_iter().filter(|x| *x == f).count()
}

/// Vitae is a **single** enchantment on the registry rather than a list entry, and its `StatMod`
/// is keyed on `0`. Skill enchantment applies it before culling either spell list.
fn o458_vitae(multiplier: f32) -> dereth_protocol::types::qualities::Enchantment {
    dereth_protocol::types::qualities::Enchantment {
        id: 0,
        category_word: 0,
        power_level: 0,
        start_time: 0.0,
        duration: -1.0,
        caster: ObjectId(0),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: dereth_protocol::types::qualities::StatMod {
            kind: dereth_client_model::enchant::ench_type::VITAE,
            key: 0,
            value: multiplier,
        },
        spell_set_id: None,
    }
}

/// **A constructed station, and what it is built from.** The recording's own character
/// description, with its augmentation removed and an enchantment registry put in its place, then
/// **re-encoded and decoded again** through the production codec before the client ever sees it
/// -- so even the constructed half reaches the client as bytes its own decoder produced. Nothing
/// is synthesised from scratch and no skill entry is hand-written.
///
/// Removing property **326** is how a *plain* baseline is reached at all: the recorded
/// character's `JACK_OF_ALL_TRADES` buffs every skill by five, so with it present a vitae penalty
/// can never bring `eff - vitae` below `raw` and the vitae arm would be unobservable.
fn o458_a_character_respun(
    clear_augmentation: bool,
    registry: Option<dereth_protocol::types::qualities::EnchantmentRegistry>,
) -> HeadlessClient {
    use dereth_protocol::types::qualities::quality_flags;

    let mut d = recorded_description(SESSION);
    if clear_augmentation {
        let ints = d
            .qualities
            .base
            .tables
            .ints
            .as_mut()
            .expect("the int table is present");
        let before = ints.entries.len();
        ints.entries.retain(|(k, _)| *k != 326);
        assert_eq!(
            before - 1,
            ints.entries.len(),
            "property 326 was there to remove"
        );
    }
    if let Some(r) = registry {
        d.qualities.flags |= quality_flags::ENCHANTMENT_REGISTRY;
        d.qualities.enchantments = Some(r);
    }
    let bytes = dereth_protocol::write_body(&d).expect("the spliced description re-encodes");
    let back: dereth_protocol::login::LoginPlayerDescription =
        dereth_protocol::read_body(&bytes).expect("and decodes again, byte for byte");
    assert_eq!(
        back.qualities.enchantments, d.qualities.enchantments,
        "the round trip holds"
    );

    let mut c = a_recorded_character(SESSION);
    // A second description is what a client receives on a relog; the arm that unpacks it rebuilds
    // the page itself, and `apply_property_tables` replaces the int table wholesale, which is how
    // the augmentation really goes away rather than merging back in.
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(back)),
    ));
    c.tick(8);
    show_the_skills_page(&mut c);
    c
}

// ---------------------------------------------------------------------------------------------
// skills.scroll.the-list-is-one-column-of-equal-rows-and-most-of-it-is-out-of-sight
// ---------------------------------------------------------------------------------------------

/// **Every number the row quoted, reproduced, and one of them corrected.** The row said "42 rows
/// of 20 px in a 159 px box"; **42 is the count of list *items*, not of skills** -- 38 skill rows
/// plus 4 group headers -- and the two are asserted apart so the figure cannot be quoted forward
/// as a skill count again.
///
/// **159 and 160 are both right and they are different numbers.** The box height is inclusive of
/// both edges; hit testing and the thumb proportion use the resulting 160-pixel height. A check
/// written against 159 would be off by one row-tenth and
/// would still look plausible.
///
/// 840 of content over a 160 view is 5.25 viewports: 8 items fit, so 34 of 42 are outside the
/// rectangle at rest, and item-at-point lookup refuses every one of them.
pub fn the_skills_list_is_one_column_of_equal_rows_mostly_out_of_sight() {
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);

    let (items, heights, columns, rows, headers) = {
        let p = &c.view().expect_app().hud().panels.skills;
        let l = p.list.as_ref().expect("the skills list is bound");
        (
            l.items.len(),
            l.item_heights.clone(),
            l.item_widths.len(),
            p.rows.len(),
            p.headers.len(),
        )
    };
    let counted = rows == 38 && headers == 4 && items == 42 && rows + headers == items;
    // One column: `0x1000023D` declares no `0x5F`, so its column count is 1. Every recorded item
    // height is 20, written by the layout update's first pass.
    let one_column = columns == 1 && heights.len() == 42 && heights.iter().all(|h| *h == 20);
    let content: i32 = heights.iter().sum();
    let sums = content == 840;

    let lb = o458_list_box(&mut c);
    let b = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.node(lb).expect("the list box has a node").region.box_
    };
    let boxed = (b.y0, b.y1) == (112, 271) && b.y1 - b.y0 == 159 && b.height() == 160;

    // A second, independent reading of the content extent: the live height on the scrollable element
    // rather than off the widget's own array. Two readings that could have disagreed.
    let declared = o458_scrollable(&mut c, lb).height == content;

    let fits = b.height() / 20;
    let out_of_sight =
        fits == 8 && items - usize::try_from(fits).expect("a row count fits a usize") == 34;

    c.assert_behaviour(
        "skills.scroll.the-list-is-one-column-of-equal-rows-and-most-of-it-is-out-of-sight",
        move |_| counted && one_column && sums && boxed && declared && out_of_sight,
    );
    c.shutdown();
}

#[test]
fn scenario_the_skills_list_is_one_column_of_equal_rows_mostly_out_of_sight() {
    scenario("the_skills_list_is_one_column_of_equal_rows_mostly_out_of_sight");
}

// ---------------------------------------------------------------------------------------------
// skills.scroll.the-bar-beside-the-list-says-how-much-is-in-view-and-where-it-is
// ---------------------------------------------------------------------------------------------

/// **The thumb, on its own.** The row asked for the thumb geometry to be asserted *separately*
/// from the hit test, because the two are independent mechanisms that happen to move together.
/// The scrollbar-size and scrollbar-position updates derive the thumb geometry, while
/// item-at-point lookup derives the hit-tested row.
/// The first can work while the second does not, so a check that only said "the list scrolls"
/// would pass on a panel where two thirds of the rows could not be clicked.
///
/// Both numbers are pinned as literals derived from the measured 160 and 840, and each is checked
/// against the formulas the client uses: `0x88` is `min(view_height / content_height, 1.0)`
/// and `0x86` is `scroll_y / (content_height - view_height)`.
///
/// **Two stations, so a stuck attribute cannot pass**: the bar is read at rest and again once the
/// viewport has moved.
pub fn the_bar_beside_the_skills_list_reports_the_view_and_where_it_is() {
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);
    let lb = o458_list_box(&mut c);

    // Attribute `0x72` names the bar and there is no `0x71`, which is why every claim here is on
    // the y axis.
    let s = o458_scrollable(&mut c, lb);
    let names_a_bar = s.v_scrollbar == Some(O458_SCROLLBAR) && s.h_scrollbar.is_none();

    let bar = o458_bar(&mut c, lb);
    let real_bar = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let kind = ui.node(bar).expect("the bar has a node").ty().0;
        let lbb = ui.node(lb).expect("the list box has a node").region.box_;
        let barb = ui.node(bar).expect("the bar has a node").region.box_;
        // `0x0B` is the scrollbar element type, and the bar spans the list's own rows.
        kind == 0x0B && (barb.y0, barb.y1) == (lbb.y0, lbb.y1)
    };

    // Station 1 -- at rest.
    let at_rest = o458_scroll_y(&mut c, lb) == 0;
    let want = 160.0f32 / 840.0f32;
    let sized =
        (o458_bar_float(&mut c, bar, dereth_ui::widgets::scrollbar::attr::PROPORTION) - want).abs()
            < 1e-6;
    let placed = o458_bar_float(&mut c, bar, dereth_ui::widgets::scrollbar::attr::POSITION) == 0.0;
    // 840 of content in a 160 view: the bar is live, not disabled.
    let live =
        o458_bar_bool(&mut c, bar, dereth_ui::widgets::scrollbar::attr::DISABLED) == Some(false);

    // Station 2 -- after four arrow presses, the bar must have moved with the viewport.
    for _ in 0..4 {
        o458_press_arrow(&mut c, bar, true);
    }
    let moved = o458_scroll_y(&mut c, lb) == 80;
    let want_pos = 80.0f32 / (840.0f32 - 160.0f32);
    let followed = (o458_bar_float(&mut c, bar, dereth_ui::widgets::scrollbar::attr::POSITION)
        - want_pos)
        .abs()
        < 1e-6;
    // The proportion is a property of the content, so it must **not** have moved.
    let still_sized =
        (o458_bar_float(&mut c, bar, dereth_ui::widgets::scrollbar::attr::PROPORTION) - want).abs()
            < 1e-6;

    c.assert_behaviour(
        "skills.scroll.the-bar-beside-the-list-says-how-much-is-in-view-and-where-it-is",
        move |_| {
            names_a_bar
                && real_bar
                && at_rest
                && sized
                && placed
                && live
                && moved
                && followed
                && still_sized
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_bar_beside_the_skills_list_reports_the_view_and_where_it_is() {
    scenario("the_bar_beside_the_skills_list_reports_the_view_and_where_it_is");
}

// ---------------------------------------------------------------------------------------------
// skills.scroll.each-arrow-moves-the-list-one-row-its-own-way-and-the-top-is-the-top
// ---------------------------------------------------------------------------------------------

/// **The cancelling pair, taken apart.** Never drive an operation and its inverse and compare
/// with the start: the down step, the up step and the floor are each asserted on their own, and
/// the arrow drawn at the **top** of the bar is required to be the one that scrolls **up**. Two
/// errors -- the arrows swapped and the sign inverted -- cancel exactly on a vertical bar, which
/// is why this is the shape of check that can see it.
pub fn each_arrow_moves_the_skills_list_one_row_its_own_way() {
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);
    let lb = o458_list_box(&mut c);
    let bar = o458_bar(&mut c, lb);

    // Which arrow is which is read off the bar, and where each is drawn is asserted rather than
    // assumed: the one that moves the list **up** is the one at the head of the bar.
    let up = o458_arrow(&mut c, bar, false);
    let down = o458_arrow(&mut c, bar, true);
    let laid_out = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.screen_box(up).y0 < ui.screen_box(down).y0
    };

    let start = o458_scroll_y(&mut c, lb) == 0;
    o458_press_arrow(&mut c, bar, true);
    // One press moves one row: 840 / 42 = 20 pixels.
    let one = o458_scroll_y(&mut c, lb) == 20;
    o458_press_arrow(&mut c, bar, true);
    let two = o458_scroll_y(&mut c, lb) == 40;
    o458_press_arrow(&mut c, bar, false);
    let back = o458_scroll_y(&mut c, lb) == 20;
    o458_press_arrow(&mut c, bar, false);
    let home = o458_scroll_y(&mut c, lb) == 0;
    // The scroll offset clamps into `0 ..= content - view` unless attribute `0x73`
    // says otherwise, and `0x1000023D` declares no `0x73`, so the local reads 1 and it clamps.
    o458_press_arrow(&mut c, bar, false);
    let clamped = o458_scroll_y(&mut c, lb) == 0;

    c.assert_behaviour(
        "skills.scroll.each-arrow-moves-the-list-one-row-its-own-way-and-the-top-is-the-top",
        move |_| laid_out && start && one && two && back && home && clamped,
    );
    c.shutdown();
}

#[test]
fn scenario_each_arrow_moves_the_skills_list_one_row_its_own_way() {
    scenario("each_arrow_moves_the_skills_list_one_row_its_own_way");
}

// ---------------------------------------------------------------------------------------------
// skills.scroll.a-press-on-a-scrolled-list-picks-the-row-it-is-drawn-over
// ---------------------------------------------------------------------------------------------

/// **The binding criterion, and "at a scrolled viewport" is the whole of it**: the same press at
/// the top of the list passes with the defect present, because at scroll offset zero the dead
/// field and the live one agree. Measured before the fix, with the viewport at 240, a press ten
/// pixels below the top of the box -- over the row of skill 6, *Melee Defense* -- resolved to
/// item **0**, the "Specialized" header, and selected nothing.
///
/// Two things are asserted and separately: that the drawn row under the pointer and the row the
/// panel selects are the same one, and that the row the **unscrolled** band walk would have named
/// is a different one -- so a build that ignores the offset cannot pass, and the station cannot
/// be satisfied by the accident that the item happens to be somewhere plausible.
pub fn a_press_on_a_scrolled_skills_list_picks_the_row_it_is_drawn_over() {
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);
    let lb = o458_list_box(&mut c);

    // Station 1 -- unscrolled. The control, and the arm the old code got right.
    let first = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .first()
        .expect("the page has 38 rows")
        .skill;
    let at_rest = o458_scroll_y(&mut c, lb) == 0;
    press_skill_row(&mut c, first);
    c.tick(1);
    let picked_first = o458_selected(&c) == first;
    // Clear the selection again so the scrolled station starts from nothing. Selecting the
    // current row again toggles it off.
    press_skill_row(&mut c, first);
    c.tick(1);
    let cleared = o458_selected(&c) == 0;

    // Station 2 -- scrolled, which is the one the claim is about.
    const TARGET: usize = 39;
    let brought = o458_bring_into_view(&mut c, TARGET);
    let y = o458_scroll_y(&mut c, lb);
    let scrolled = brought && y > 0;
    let at = o458_item_centre(&mut c, TARGET);
    // It must really be inside the list box now, or the press proves nothing.
    let inside = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let lbb = ui.screen_box(lb);
        at.y >= lbb.y0 && at.y <= lbb.y1
    };
    let discriminates = o458_band_walk_without_the_offset(&mut c, lb, at.y) != TARGET;
    let drawn = o458_rows_drawn_under(&mut c, at);
    let exactly_one = drawn.len() == 1;

    c.when(Player::Click(Target::Point(at)));
    c.tick(2);
    let picked = drawn.first().copied() == Some(o458_selected(&c));
    // A selection emits no request; the spend is the advancement family's claim.
    let sent_nothing = c
        .view()
        .outbound()
        .iter()
        .all(|r| !matches!(r, dereth_client_model::Request::TrainSkill(_)));

    c.assert_behaviour(
        "skills.scroll.a-press-on-a-scrolled-list-picks-the-row-it-is-drawn-over",
        move |_| {
            at_rest
                && picked_first
                && cleared
                && scrolled
                && inside
                && discriminates
                && exactly_one
                && picked
                && sent_nothing
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_press_on_a_scrolled_skills_list_picks_the_row_it_is_drawn_over() {
    scenario("a_press_on_a_scrolled_skills_list_picks_the_row_it_is_drawn_over");
}

// ---------------------------------------------------------------------------------------------
// skills.scroll.rebuilding-the-list-keeps-where-it-was-scrolled-to
// ---------------------------------------------------------------------------------------------

/// **One frame, not two.** The layout update builds the per-column and per-row maxima, resizes the
/// scrollable area, and *then*
/// places every item at `running_sum - scroll_offset`. Without the second half in the same
/// pass the rows sit unscrolled until the next global tick while the vertical offset is still set,
/// and anything that reads geometry in between -- a click, a screenshot -- sees a list that has
/// forgotten where it was scrolled to. The skill-list rebuild runs on
/// `PlayerDescReceived` and on `SkillAdvancementClassChanged`, so a player scrolled two thirds
/// down who trains a skill hits exactly this.
///
/// The rebuild is run with **no frame after it**, which is what makes this a claim about one
/// frame rather than about the tick that follows.
pub fn rebuilding_the_skills_list_keeps_where_it_was_scrolled_to() {
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);
    let lb = o458_list_box(&mut c);
    let bar = o458_bar(&mut c, lb);

    for _ in 0..6 {
        o458_press_arrow(&mut c, bar, true);
    }
    let y = o458_scroll_y(&mut c, lb);
    let six = y == 120;

    let skills_now = c.view().expect_app().hud().skills.clone();
    {
        let app = c.app_mut();
        let mut panels = std::mem::take(&mut app.hud_mut().panels);
        {
            let (ui, _) = gameplay_screen(app);
            panels.skills.rebuild(ui, &skills_now);
        }
        app.hud_mut().panels = panels;
    }

    let kept = o458_scroll_y(&mut c, lb) == y;
    // The first list item's own box: at offset 120 it must sit 120 px above the list box's top,
    // in the rebuild's own frame.
    let placed = {
        let item0 = c
            .view()
            .expect_app()
            .hud()
            .panels
            .skills
            .list
            .as_ref()
            .expect("the skills list is bound")
            .items[0];
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.screen_box(item0).y0 - ui.screen_box(lb).y0 == -y
    };

    c.assert_behaviour(
        "skills.scroll.rebuilding-the-list-keeps-where-it-was-scrolled-to",
        move |_| six && kept && placed,
    );
    c.shutdown();
}

#[test]
fn scenario_rebuilding_the_skills_list_keeps_where_it_was_scrolled_to() {
    scenario("rebuilding_the_skills_list_keeps_where_it_was_scrolled_to");
}

// ---------------------------------------------------------------------------------------------
// skills.rows.the-number-shown-is-the-total-after-everything-and-the-colour-says-which-way
// ---------------------------------------------------------------------------------------------

/// **Three stations on one claim.** The row update formats the out slot of the
/// **third** query as a decimal string. That query is
/// the non-raw skill query -- the enchanted total -- and not the *second*, raw query.
///
/// **The corpus reaches the buffed arm without any construction at all, and it is worth saying
/// how.** The recorded character carries no enchantments (both lists empty, vitae 1.0) but does
/// carry `JACK_OF_ALL_TRADES` = 1, which adds **+5** only to the non-raw result. So the enchanted
/// total is the
/// base plus five on every one of the 38 rows, and a panel that drew the raw query would draw a
/// number five too low.
///
/// The numbers are pinned as **literals from the recording** rather than read back through the
/// symbol the production code writes them through: a check that reads a constant through the same
/// symbol it writes it through cannot detect a wrong constant.
///
/// The corpus reaches neither a debuffed row nor a plain one, so those two are constructed -- and
/// the plain one is the control that shows the augmentation was doing the work above: the same
/// character, one property lighter, draws **72** in white where it drew **77** in green.
pub fn a_skill_row_shows_the_total_after_everything_in_the_colour_of_the_change() {
    // ---- station 1: the recorded character, buffed on every row by its own augmentation. ------
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);

    let augmented = {
        let q = c
            .view()
            .world()
            .player_qualities()
            .expect("the recorded description")
            .clone();
        q.inq_int(326) == 1 && q.enchantments.vitae_value() == 1.0
    };
    let numbers = o458_skill_numbers(&c, O458_MELEE_DEFENSE) == (72, 77, 0);
    // The cell carries the ENCHANTED total, not the base 72; 72 < 77 - 0, so the buffed colour.
    let cell = o458_value_cell(&mut c, O458_MELEE_DEFENSE) == ("77".to_owned(), 1);
    // And it is every row, not one: all 38 are +5 and all 38 are green.
    let every_row = c
        .view()
        .expect_app()
        .hud()
        .skills
        .iter()
        .all(|r| r.effective == r.level + 5)
        && o458_fonts(&c).iter().all(|f| *f == 1);

    // ---- station 2: the same character with the augmentation removed and nothing added. -------
    let plain = {
        let mut p = o458_a_character_respun(true, None);
        let gone = p
            .view()
            .world()
            .player_qualities()
            .expect("the description")
            .inq_int(326)
            == 0;
        let n = o458_skill_numbers(&p, O458_MELEE_DEFENSE) == (72, 72, 0);
        let drawn = o458_value_cell(&mut p, O458_MELEE_DEFENSE) == ("72".to_owned(), 0);
        let whole_page = o458_fonts(&p).iter().all(|f| *f == 0);
        p.shutdown();
        gone && n && drawn && whole_page
    };

    // ---- station 3: buffed and debuffed, on one character, at once. ---------------------------
    //
    // The corpus reaches the buffed arm only through an augmentation that applies to every skill
    // uniformly, so it can never show a *debuffed* row and it can never show the two side by
    // side. One `+20` and one `-20` on the plain character put all three colours on one panel, so
    // a mistake that colours the whole list one way cannot pass.
    let both = {
        let reg = dereth_protocol::types::qualities::EnchantmentRegistry {
            flags: dereth_protocol::types::qualities::EnchantmentRegistry::ADDITIVE,
            additive: Some(vec![
                spell_on(4700, 110, O458_MELEE_DEFENSE, 20.0),
                spell_on(4701, 111, O458_HEALING, -20.0),
            ]),
            ..dereth_protocol::types::qualities::EnchantmentRegistry::default()
        };
        let mut p = o458_a_character_respun(true, Some(reg));
        let n = o458_skill_numbers(&p, O458_MELEE_DEFENSE) == (72, 92, 0)
            && o458_skill_numbers(&p, O458_HEALING) == (58, 38, 0);
        let up = o458_value_cell(&mut p, O458_MELEE_DEFENSE) == ("92".to_owned(), 1);
        let down = o458_value_cell(&mut p, O458_HEALING) == ("38".to_owned(), 2);
        // The rest of the panel is untouched -- the enchantments are keyed, so exactly two rows
        // move. A colour bug that painted the list uniformly would fail here.
        let rest = (
            o458_by_font(&p, 0),
            o458_by_font(&p, 1),
            o458_by_font(&p, 2),
        ) == (36, 1, 1);
        p.shutdown();
        n && up && down && rest
    };

    c.assert_behaviour(
        "skills.rows.the-number-shown-is-the-total-after-everything-and-the-colour-says-which-way",
        move |_| augmented && numbers && cell && every_row && plain && both,
    );
    c.shutdown();
}

#[test]
fn scenario_a_skill_row_shows_the_total_after_everything_in_the_colour_of_the_change() {
    scenario("a_skill_row_shows_the_total_after_everything_in_the_colour_of_the_change");
}

// ---------------------------------------------------------------------------------------------
// skills.rows.a-weakened-character-shows-the-lower-number-without-calling-it-lowered
// ---------------------------------------------------------------------------------------------

/// **The case the vitae term is invisible without.** The non-raw skill query already carries
/// vitae -- the skill-enchantment calculation applies it before culling either spell list -- so
/// the naive comparison of `raw` against `eff` reads `72 < 68` for every skill and **paints the
/// whole list red**. The row color calculation subtracts the vitae modifier, which is
/// `Enchant(raw) - raw` with only vitae applied and is therefore `-4` here, so `eff - v` is `72`
/// and the row draws plain.
///
/// The second half separates the two readings in the *other* direction: a `+8` buff under vitae
/// gives `eff = 63 < 58` raw on a naive reading, which paints **red**, where the client paints
/// **green**. Both directions, so the term cannot be right for the wrong reason.
///
/// The arithmetic is `raw * 0.95 + 0.5`, truncated. Melee Defense gives `68.4` either way, so a
/// second skill is pinned whose
/// product has a fractional part of exactly a half: *Jump*'s `70 * 0.95` is `66.5`, which the
/// client shows as **67** and a truncating build shows as 66. A case whose every product falls
/// below a half cannot see that `+ 0.5` at all.
pub fn a_weakened_character_shows_the_lower_number_still_drawn_plain() {
    let reg = dereth_protocol::types::qualities::EnchantmentRegistry {
        flags: dereth_protocol::types::qualities::EnchantmentRegistry::VITAE
            | dereth_protocol::types::qualities::EnchantmentRegistry::ADDITIVE,
        additive: Some(vec![spell_on(4702, 112, O458_HEALING, 8.0)]),
        vitae: Some(o458_vitae(0.95)),
        ..dereth_protocol::types::qualities::EnchantmentRegistry::default()
    };
    let mut c = o458_a_character_respun(true, Some(reg));

    let reached = c
        .view()
        .world()
        .player_qualities()
        .expect("the spliced description")
        .enchantments
        .vitae_value()
        == 0.95;

    // Vitae alone: 72 * 0.95 = 68.4, + 0.5, truncated: 68 -- and the row is PLAIN, because
    // 72 == 68 - (-4).
    let alone = o458_skill_numbers(&c, O458_MELEE_DEFENSE) == (72, 68, -4)
        && o458_value_cell(&mut c, O458_MELEE_DEFENSE) == ("68".to_owned(), 0);

    // The rounding, on the panel: 70 * 0.95 = 66.5, + 0.5 = 67.0, truncated: 67, not 66.
    let rounding = o458_skill_numbers(&c, O458_JUMP) == (70, 67, -3)
        && o458_value_cell(&mut c, O458_JUMP) == ("67".to_owned(), 0);

    // Vitae plus a small buff -- green, though the number drawn is below the base level:
    // 58 * 0.95 = 55.1 -> 55, + 8 = 63; 55 - 58 = -3; and 58 < 63 - (-3) = 66.
    let buffed_under_it = o458_skill_numbers(&c, O458_HEALING) == (58, 63, -3)
        && o458_value_cell(&mut c, O458_HEALING) == ("63".to_owned(), 1);

    // The whole list, so "plain" is a property of the panel and not of one row: vitae alone
    // colours nothing, exactly the buffed row is green, and nothing is red.
    let whole_page = (
        o458_by_font(&c, 0),
        o458_by_font(&c, 1),
        o458_by_font(&c, 2),
    ) == (37, 1, 0);

    c.assert_behaviour(
        "skills.rows.a-weakened-character-shows-the-lower-number-without-calling-it-lowered",
        move |_| reached && alone && rounding && buffed_under_it && whole_page,
    );
    c.shutdown();
}

#[test]
fn scenario_a_weakened_character_shows_the_lower_number_still_drawn_plain() {
    scenario("a_weakened_character_shows_the_lower_number_still_drawn_plain");
}

// =============================================================================================
// contracts.*
//
// The contracts panel, end to end: `0x0314`/`0x0315` reach the tracker table, the shipped
// contract table is loaded and read, and the panel draws what they hold.
//
// **Every gesture goes through the client's own machinery.** The tab is raised through the panel
// stack and the shipped tab table; a row is picked with a real press at a real screen position,
// which lands on the **list box** and never on the row; the button release raises the message;
// and both shard messages arrive as the session
// layer shapes them.
// =============================================================================================

use dereth_ui_screens::panels::contracts::{self, ContractSort, ContractsPanel};

/// The four contracts these scenarios drive, with shipped contract table `0x0E00001D`'s own
/// names as literals, so a wrong id cannot agree with a wrong name.
///
/// `"The Shadows of Bitter Winter"` -- a repeat flag, no end NPC, quest area `0xC8DC0033`.
const CONTRACTS_SHADOWS: u32 = 1;
/// `"Test Quest Stamping"` -- the only one of the four with **both** NPCs and a timer flag.
const CONTRACTS_STAMPING: u32 = 2;
/// `"Reign of Terror"`.
const CONTRACTS_TERROR: u32 = 5;
/// `"Glenden Wood Invasion (Low)"`.
const CONTRACTS_GLENDEN: u32 = 6;

/// The order the shard sends them in -- deliberately **not** the display order.
const CONTRACTS_ARRIVAL: [u32; 4] = [
    CONTRACTS_SHADOWS,
    CONTRACTS_STAMPING,
    CONTRACTS_TERROR,
    CONTRACTS_GLENDEN,
];
/// Name sorting is case-insensitive by `_contract_name`, so
/// `"Test Quest Stamping"` sorts before `"The Shadows of Bitter Winter"` -- `'e' < 'h'`.
const CONTRACTS_BY_NAME: [u32; 4] = [
    CONTRACTS_GLENDEN,
    CONTRACTS_TERROR,
    CONTRACTS_STAMPING,
    CONTRACTS_SHADOWS,
];
/// The names, in that order, as literals out of the shipped dat.
const CONTRACTS_BY_NAME_TEXT: [&str; 4] = [
    "Glenden Wood Invasion (Low)",
    "Reign of Terror",
    "Test Quest Stamping",
    "The Shadows of Bitter Winter",
];

/// One wire contract tracker. Its receipt timestamp is **not** on the wire -- the client
/// stamps it on receipt.
fn contracts_tracker(
    id: u32,
    stage: u32,
    when_done: f64,
    when_repeats: f64,
) -> dereth_protocol::social::ContractTracker {
    dereth_protocol::social::ContractTracker {
        version: 0,
        contract_id: id,
        contract_stage: stage,
        time_when_done: when_done,
        time_when_repeats: when_repeats,
    }
}

/// `0x0314`, carrying the four contracts in [`CONTRACTS_ARRIVAL`] order.
fn contracts_table() -> dereth_protocol::social::SocialSendClientContractTrackerTable {
    dereth_protocol::social::SocialSendClientContractTrackerTable(
        dereth_protocol::archive::PackedHash {
            table_size: 32,
            entries: vec![
                (
                    CONTRACTS_SHADOWS,
                    contracts_tracker(CONTRACTS_SHADOWS, 1, 0.0, 0.0),
                ),
                (
                    CONTRACTS_STAMPING,
                    contracts_tracker(CONTRACTS_STAMPING, 2, 90.0, 0.0),
                ),
                (
                    CONTRACTS_TERROR,
                    contracts_tracker(CONTRACTS_TERROR, 3, 0.0, 0.0),
                ),
                (
                    CONTRACTS_GLENDEN,
                    contracts_tracker(CONTRACTS_GLENDEN, 4, 0.0, 0.0),
                ),
            ],
        },
    )
}

/// A client with the quest page open on the Contracts tab and the four contracts delivered
/// through the **real** `0x0314` arm.
fn a_client_on_the_contracts_tab() -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let peer = Peer::attach_creating(&mut c, HOUSE_PLAYER);
    c.world_mut().player = Some(HOUSE_PLAYER);
    c.when(Inbound::message(&contracts_table()));
    c.tick(2);
    open_the_page(&mut c, contracts::PAGE);
    click_the_tab(&mut c, contracts::PAGE, contracts::PANEL);
    (c, peer)
}

/// The contracts panel, as the client holds it.
fn contracts_panel(c: &HeadlessClient) -> &ContractsPanel {
    &c.view().expect_app().hud().panels.contracts
}

/// The list box's live rows, in list order.
fn contracts_rows(c: &HeadlessClient) -> Vec<ElemHandle> {
    contracts_panel(c)
        .list
        .as_ref()
        .map(|l| l.items.clone())
        .unwrap_or_default()
}

/// The text one child of one row is really drawing, read back off the tree.
fn contracts_row_text(c: &mut HeadlessClient, id: u32) -> Vec<String> {
    let rows = contracts_rows(c);
    let (ui, _) = gameplay_screen(c.app_mut());
    rows.iter()
        .map(|r| {
            ui.get_child_recursive(*r, ElementId(id))
                .and_then(|ch| ui.text_element_mut(ch))
                .map_or_else(String::new, |t| t.glyphs.inq_text(false))
        })
        .collect()
}

/// One of the six detail lines, read back off the tree.
fn contracts_field(c: &mut HeadlessClient, id: ElementId) -> String {
    let root = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.root().expect("the gameplay screen has a root")
    };
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.get_child_recursive(root, id)
        .and_then(|h| ui.text_element_mut(h))
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// A **real press** at the centre of a row, with the element the hit test chose handed back.
///
/// **The same rule as the Titles tab, one level further down.** A press never
/// lands on the row element: the Titles tab's row is a bare container so the hit test stops at
/// the list box, while the contracts row carries two text children that fill it,
/// so the hit test stops at `0x100005D1`. Either way the press is the **list box's** --
/// list-box press handling is what finds the item under the mouse -- and the claim
/// that matters is the selection, not the element under the cursor.
fn contracts_press_row(c: &mut HeadlessClient, index: usize) -> Option<ElementId> {
    let h = *contracts_rows(c)
        .get(index)
        .expect("the contracts list has that row");
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    let hit = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.hit_test_screen(at.x, at.y)
    };
    c.when(Player::Click(Target::Point(at)));
    c.tick(2);
    let (ui, _) = gameplay_screen(c.app_mut());
    hit.and_then(|e| ui.node(e))
        .map(dereth_ui::ElementNode::element_id)
}

/// Send a button-release message to one of the panel's three buttons.
fn contracts_click_button(c: &mut HeadlessClient, id: ElementId) {
    let h = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        ui.get_child_recursive(root, id)
            .unwrap_or_else(|| panic!("{id:?} is in the shipped tree"))
    };
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
    }
    c.tick(3);
}

/// Every contract this client has asked to give up.
fn contracts_abandoned(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::SocialAbandonContract(m) => Some(m.contract_id),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// contracts.tab.the-tab-draws-a-row-per-contract-the-shard-sent-in-name-order
// ---------------------------------------------------------------------------------------------

/// **This cannot pass without all four halves.** Without the `0x0314` arm the tracker table is
/// empty; without the shipped contract-table loader every tracker is unresolved and drops; without a
/// reader for the world's contract trackers there is nothing to join; and without the panel there is no
/// module to draw it.
///
/// The panel joins the trackers to shipped contract table `0x0E00001D`, defaults to name sorting,
/// and fills the status column. Three status arms that were previously transcribed incorrectly
/// are asserted here on screen.
pub fn the_contracts_tab_draws_a_row_per_contract_in_name_order() {
    let (mut c, _peer) = a_client_on_the_contracts_tab();

    let list_is_right = {
        let p = contracts_panel(&c);
        // Initialization found the contracts list box, the list box carries one row template,
        // the rebuild really ran -- which is what separates empty from unwired -- name order is the
        // constructor's criterion, and the rebuild ends by clearing the selected item.
        p.bound()
            && p.templates() == 1
            && p.rebuilds > 0
            && p.sort == ContractSort::Name
            && p.shown() == CONTRACTS_BY_NAME
            // ...and the sort is doing work rather than echoing the wire.
            && p.shown().as_slice() != CONTRACTS_ARRIVAL
            && p.drawn() == 4
            && p.selected.is_none()
    };

    let named = contracts_row_text(&mut c, contracts::ROW_NAME) == CONTRACTS_BY_NAME_TEXT.to_vec();
    let statuses = contracts_row_text(&mut c, contracts::ROW_STATUS)
        == vec![
            // GLENDEN is stage 4 with an empty `_description_progress` -> `"In Progress"`,
            // and not an empty line.
            "In Progress",
            // TERROR is stage 3 with no repeat timer and a **non-empty**
            // `_questflag_repeat_time` -> `"Available"`, which that transcription
            // rendered as the empty string.
            "Available",
            "In Progress",
            "Available",
        ];

    // The whole chain's counters, so an empty tab could say which link broke.
    let counters = {
        let s = &c.view().expect_app().hud().stats;
        s.contract_tables == 1 && s.contracts == 4 && s.contracts_unresolved == 0
    };

    c.assert_behaviour(
        "contracts.tab.the-tab-draws-a-row-per-contract-the-shard-sent-in-name-order",
        move |_| list_is_right && named && statuses && counters,
    );
    c.shutdown();
}

#[test]
fn scenario_the_contracts_tab_draws_a_row_per_contract_in_name_order() {
    scenario("the_contracts_tab_draws_a_row_per_contract_in_name_order");
}

// ---------------------------------------------------------------------------------------------
// contracts.detail.picking-a-contract-fills-the-pane-beside-the-list
// ---------------------------------------------------------------------------------------------

/// The detail update supplies six fields, including three that are easy to transcribe wrong.
/// Its contact choice is checked on the one contract of the four that carries **both** NPCs.
pub fn picking_a_contract_fills_the_pane_beside_the_list() {
    let (mut c, _peer) = a_client_on_the_contracts_tab();

    // Nothing picked: the detail update's second early return leaves every field alone.
    let empty = contracts_panel(&c).selected.is_none()
        && contracts_field(&mut c, contracts::NOTES_TEXT).is_empty();

    // The fourth row is SHADOWS -- stage 1, no end NPC, quest area `0xC8DC0033`.
    let hit = contracts_press_row(&mut c, 3);
    let landed =
        hit == Some(contracts::CONTRACTS_BOX) || hit == Some(ElementId(contracts::ROW_NAME));
    let resolved = contracts_panel(&c).selected == Some(3);

    let notes = contracts_field(&mut c, contracts::NOTES_TEXT)
        == "Farelaith's younger brothers have been corrupted into Shadows and must be destroyed.";
    // `_name_npc_end` is empty, so `contact_is_the_end_npc` takes the start NPC.
    let contact = contracts_field(&mut c, contracts::CONTACT_TEXT) == "Shade of Farelaith";
    let progress = contracts_field(&mut c, contracts::PROGRESS_TEXT) == "Available";
    // No `_questflag_timer` selects the literal `None`.
    let timed = contracts_field(&mut c, contracts::TIMED_TEXT) == "None";
    // `0xC8DC0033` -> lcoord (1606, 1762) -> ((1762-0x400)*0.1+0.5, (1606-0x400)*0.1+0.5).
    // **Both axes subtract 0x400 and add 0.5**; this is the pane's own arithmetic rather than a
    // shared cell-coordinate conversion.
    let area = contracts_field(&mut c, contracts::AREA_TEXT) == "74.3N, 58.7E";
    // `_location_npc_start` is the same cell for this contract.
    let contact_at = contracts_field(&mut c, contracts::CONTACT_LOC_TEXT) == "74.3N, 58.7E";

    // The third row is STAMPING -- stage 2, both NPCs, a timer flag.
    let _ = contracts_press_row(&mut c, 2);
    let the_named_one = contracts_panel(&c).shown()[2] == CONTRACTS_STAMPING;
    // Stage 2 with a non-empty `_name_npc_end` takes the end NPC.
    let end_npc = contracts_field(&mut c, contracts::CONTACT_TEXT) == "Bob the Righteous and Bold";
    // `_questflag_timer` is set, so the timed line is the delta-time text of what is left of
    // `_time_when_done` -- 90 s, truncated. A running countdown, and
    // not either fallback.
    let counting = {
        let t = contracts_field(&mut c, contracts::TIMED_TEXT);
        t.ends_with('s') && t != "None" && t != "Finished"
    };

    // Picking sends nothing -- asserted rather than assumed.
    let silent = contracts_abandoned(&c).is_empty()
        && c.view().expect_app().interaction().stats.contract_requests == 0;

    c.assert_behaviour(
        "contracts.detail.picking-a-contract-fills-the-pane-beside-the-list",
        move |_| {
            empty
                && landed
                && resolved
                && notes
                && contact
                && progress
                && timed
                && area
                && contact_at
                && the_named_one
                && end_npc
                && counting
                && silent
        },
    );
    c.shutdown();
}

#[test]
fn scenario_picking_a_contract_fills_the_pane_beside_the_list() {
    scenario("picking_a_contract_fills_the_pane_beside_the_list");
}

// ---------------------------------------------------------------------------------------------
// contracts.abandon.the-button-gives-up-the-picked-contract-and-nothing-local-moves
// ---------------------------------------------------------------------------------------------

/// Element arm `0x100005DC` sends an abandon-contract request on the object queue:
/// opcode `0x0316`, with a four-byte body.
pub fn the_abandon_button_gives_up_the_picked_contract_and_nothing_local_moves() {
    let (mut c, _peer) = a_client_on_the_contracts_tab();
    // Whatever the fixture framed is not this claim's business.
    let _ = c.take_wire_count(0x0316);

    // With nothing picked the button is silent because there is no selected index.
    contracts_click_button(&mut c, contracts::ABANDON_BUTTON);
    let idle = c.take_wire_count(0x0316) == 0 && contracts_abandoned(&c).is_empty();
    // ...and the refusal is counted, not silent.
    let counted = contracts_panel(&c).abandon_refusals == 1;

    // Pick `Reign of Terror` -- the second row -- and press Abandon.
    let _ = contracts_press_row(&mut c, 1);
    let the_named_one = contracts_panel(&c).shown()[1] == CONTRACTS_TERROR;
    contracts_click_button(&mut c, contracts::ABANDON_BUTTON);
    c.tick(3);

    let sent = contracts_abandoned(&c) == vec![CONTRACTS_TERROR]
        && c.view().expect_app().interaction().stats.contract_requests == 1;
    let framed = c.take_wire_count(0x0316) == 1;

    // What that request encodes to, and the queue it goes out on: four body bytes carrying the
    // picked row's own `_contract_id` and nothing else.
    let bytes_and_queue = {
        let m = dereth_protocol::social::SocialAbandonContract {
            contract_id: CONTRACTS_TERROR,
        };
        let mut session = dereth_client_net::client_session::Session::new(
            dereth_client_net::client_session::testing::MockTransport::new(),
        );
        let ok = dereth_client::interaction::send_request(
            &mut session,
            &dereth_client_model::Request::SocialAbandonContract(m),
        );
        let packet = session.transport.sent.last().expect("one datagram").clone();
        let mut want = 0xF7B1_u32.to_le_bytes().to_vec();
        want.extend_from_slice(&1_u32.to_le_bytes());
        want.extend_from_slice(&0x0000_0316_u32.to_le_bytes());
        want.extend_from_slice(&CONTRACTS_TERROR.to_le_bytes());
        (ok, packet.payload == want, packet.queue)
    };

    // **The tracker is NOT removed locally.** The table changes when the shard answers with a
    // `0x0315` carrying `deleteContract`; a client that removed it here would show the row gone
    // and then get it back on the next `0x0314`.
    let nothing_local_moved = contracts_panel(&c).shown() == CONTRACTS_BY_NAME
        && c.view().expect_app().hud().stats.contracts == 4;

    c.assert_behaviour(
        "contracts.abandon.the-button-gives-up-the-picked-contract-and-nothing-local-moves",
        move |_| {
            idle && counted
                && the_named_one
                && sent
                && framed
                && bytes_and_queue == (true, true, dereth_primitives::NetQueue::Weenie)
                && nothing_local_moved
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_abandon_button_gives_up_the_picked_contract_and_nothing_local_moves() {
    scenario("the_abandon_button_gives_up_the_picked_contract_and_nothing_local_moves");
}

// ---------------------------------------------------------------------------------------------
// contracts.list.one-contract-at-a-time-is-added-changed-or-taken-away
// ---------------------------------------------------------------------------------------------

/// The tracker update's add, update, and delete arms are driven through the
/// production router and read back off the **tree**.
pub fn one_contract_at_a_time_is_added_changed_or_taken_away() {
    let (mut c, _peer) = a_client_on_the_contracts_tab();

    // An update in place: SHADOWS moves from stage 1 to stage 2.
    deliver(
        &mut c,
        &dereth_protocol::social::SocialSendClientContractTracker {
            tracker: contracts_tracker(CONTRACTS_SHADOWS, 2, 0.0, 0.0),
            delete_contract: 0,
            set_as_display_contract: 0,
        },
    );
    c.tick(1);
    let counted_once = c.view().expect_app().hud().stats.contract_trackers_updated == 1;
    let in_place = contracts_panel(&c).shown() == CONTRACTS_BY_NAME;
    // ...and the row on screen redrew from stage 2.
    let redrawn = contracts_row_text(&mut c, contracts::ROW_STATUS)[3] == "In Progress";
    let updated = counted_once && in_place && redrawn;

    // A delete removes the matching contract entry.
    deliver(
        &mut c,
        &dereth_protocol::social::SocialSendClientContractTracker {
            tracker: contracts_tracker(CONTRACTS_SHADOWS, 2, 0.0, 0.0),
            delete_contract: 1,
            set_as_display_contract: 0,
        },
    );
    c.tick(1);
    let removed_once = c.view().expect_app().hud().stats.contract_trackers_removed == 1;
    let three_left =
        contracts_panel(&c).shown() == [CONTRACTS_GLENDEN, CONTRACTS_TERROR, CONTRACTS_STAMPING];
    // ...and the list box lost a row too.
    let removed = removed_once && three_left && contracts_panel(&c).drawn() == 3;

    // And an add, for a contract the character did not hold.
    deliver(
        &mut c,
        &dereth_protocol::social::SocialSendClientContractTracker {
            tracker: contracts_tracker(CONTRACTS_SHADOWS, 1, 0.0, 0.0),
            delete_contract: 0,
            set_as_display_contract: 1,
        },
    );
    c.tick(1);
    let added_once = {
        let s = &c.view().expect_app().hud().stats;
        // `setAsDisplayContract` is counted, because this build pins nothing to the HUD.
        s.contract_trackers_added == 1 && s.contract_display_requests == 1
    };
    let added = added_once && contracts_panel(&c).shown() == CONTRACTS_BY_NAME;

    c.assert_behaviour(
        "contracts.list.one-contract-at-a-time-is-added-changed-or-taken-away",
        move |_| updated && removed && added,
    );
    c.shutdown();
}

#[test]
fn scenario_one_contract_at_a_time_is_added_changed_or_taken_away() {
    scenario("one_contract_at_a_time_is_added_changed_or_taken_away");
}

// ---------------------------------------------------------------------------------------------
// contracts.sort.the-two-buttons-choose-the-order-and-pressing-one-twice-turns-it-round
// ---------------------------------------------------------------------------------------------

/// The sort buttons use element arms `0x100005CE` and `0x100005D6`: a press on
/// the criterion already in force **flips** the direction; a press on the other one selects it
/// and resets the direction to forward.
pub fn the_sort_buttons_choose_the_order_and_a_second_press_turns_it_round() {
    let (mut c, _peer) = a_client_on_the_contracts_tab();
    let start = contracts_panel(&c).shown() == CONTRACTS_BY_NAME;

    // The name button again: same criterion -> reverse.
    contracts_click_button(&mut c, contracts::SORT_NAME_BUTTON);
    let reversed = {
        let p = contracts_panel(&c);
        p.sort == ContractSort::Name && p.reverse
    };
    let mut backwards = CONTRACTS_BY_NAME;
    backwards.reverse();
    let list_reversed = contracts_panel(&c).shown() == backwards;
    // ...and the rows on screen followed, which a model-only check would not catch.
    let mut names = CONTRACTS_BY_NAME_TEXT;
    names.reverse();
    let drawn_reversed = contracts_row_text(&mut c, contracts::ROW_NAME) == names.to_vec();

    // The status button: a different criterion -> forward, not "reverse by status".
    contracts_click_button(&mut c, contracts::SORT_STATUS_BUTTON);
    let switched = {
        let p = contracts_panel(&c);
        p.sort == ContractSort::Status && !p.reverse
    };
    // "available" before "in progress", ties by name.
    let by_status = contracts_panel(&c).shown()
        == [
            CONTRACTS_TERROR,
            CONTRACTS_SHADOWS,
            CONTRACTS_GLENDEN,
            CONTRACTS_STAMPING,
        ];

    // Sorting sends nothing.
    let silent = contracts_abandoned(&c).is_empty()
        && c.view().expect_app().interaction().stats.contract_requests == 0;

    c.assert_behaviour(
        "contracts.sort.the-two-buttons-choose-the-order-and-pressing-one-twice-turns-it-round",
        move |_| {
            start && reversed && list_reversed && drawn_reversed && switched && by_status && silent
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_sort_buttons_choose_the_order_and_a_second_press_turns_it_round() {
    scenario("the_sort_buttons_choose_the_order_and_a_second_press_turns_it_round");
}
// =============================================================================================
// character-sheet.*
//
// The subject is the character-information panel -- the sheet the **burden** lamp opens, not a burden
// panel: the load section is one of its six and the other five have nothing to do with
// encumbrance. Every claim here is about what the shipped text element ends up saying, so every
// one of them reads the panel's own composed sections *and* the live element, because a test that
// read only the panel's cache would be green over a sheet that never reached the tree.
//
// The recording is `long-solo-play` and not this file's usual `first-login-walk-jump`, and that is
// load-bearing: its character was made moments before the capture, so its playtime is fifteen
// seconds and six of the seven terms of that sentence are blank -- which is the half of the
// duration row worth testing -- and its melee mastery is the first entry of the melee list where
// `first-login-walk-jump`'s is the sixth.
// =============================================================================================

/// The recording these claims are read off. See the section header.
const P115_SESSION: &str = "long-solo-play";

/// The player this file's shard creates and adopts.
const P115_PLAYER: ObjectId = ObjectId(0x5000_0001);

/// Shipped element `0x100000F7` -- the burden lamp that opens the character-information panel.
const P115_BURDEN_LAMP: ElementId = ElementId(0x1000_00F7);

/// What `long-solo-play`'s `0x0013` carries for `98 CreationTimestamp`, and the sentence it makes.
const P115_CORPUS_CREATED: i32 = 1_788_565_581;

/// What it carries for `125 Age`: fifteen seconds. A character made moments before the capture.
const P115_CORPUS_AGE: i32 = 15;

/// `0x14D LumAugDamageRating` -- one of the four ratings whose line splits at five. The **next**
/// pair, `0x14E`, is deliberately left ungranted, so that a specialised value leaking out of this
/// one would show up as a line that must not be there.
const P115_LUM_DAMAGE_RATING: u32 = 0x14D;
/// `0x152 LumAugSurgeChanceRating` -- a single rating, which has no cap of five.
const P115_LUM_SURGE_CHANCE_RATING: u32 = 0x152;
/// `0xDA AugmentationInnateStrength` -- one augmentation row, for the plural block every row has.
const P115_AUG_INNATE_STRENGTH: u32 = 0xDA;
/// `PropertyInt` **43** `NumDeaths`, which is *text* on this sheet and so the visible witness
/// for a property the shard clears.
const P115_NUM_DEATHS: u32 = 43;

/// The recorded character, **with a shard attached**, for a chosen recording.
///
/// This is `a_recorded_character_and_a_shard` with the recording as a parameter: that one is
/// fixed to this file's `SESSION`, and every claim below is read off `long-solo-play` instead.
///
/// The character is made by the shard rather than written into the tables, for the reason that
/// helper gives: an ordered game event is addressed to an object, and the ordered queue only has
/// somewhere to put one for an object the client's own object stream has been told about. The
/// **description** still comes through the corpus step, because it is nearly two kilobytes and is
/// a description rather than a panel notice, so nothing here turns on which way it arrived.
fn p115_a_recorded_character_and_a_shard(session: &'static str) -> (HeadlessClient, Peer) {
    let end = description_blob(session).idx;
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut shard = Peer::attach(&mut c, P115_PLAYER);

    let mut p = dereth_protocol::objects::ObjectCreatePayload {
        id: P115_PLAYER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    shard.send(
        &mut c,
        dereth_testkit::replay::OBJECT_QUEUE,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("the create encodes"),
    );
    c.tick(1);
    assert!(
        c.world_mut().set_player(P115_PLAYER),
        "the identity is adopted once"
    );

    c.when(Inbound::from_corpus(session, 0..end + 1));
    c.tick(4);
    assert!(
        c.view().world().player_qualities().is_some(),
        "the recorded description has to install a quality store or nothing here reads a quality"
    );
    (c, shard)
}

/// Open the burden lamp's panel with element message 1 and `p1 = 7`, which is the message a
/// released button raises and the one the six lamp classes answer. The lamp's own gesture is
/// claimed elsewhere; what this file's claims are about is the text behind it.
fn p115_open_the_burden_sheet(c: &mut HeadlessClient) {
    let h = el(c, P115_BURDEN_LAMP);
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    }
    c.tick(6);
    assert!(
        el_visible(c, dereth_ui_screens::panels::characterinfo::PANEL),
        "the burden lamp has to open the character sheet before its text is an observable"
    );
}

/// The sheet's six sections, in the order the composer appends them.
fn p115_sections(c: &HeadlessClient) -> Vec<String> {
    c.view()
        .expect_app()
        .hud()
        .panels
        .character_info
        .sections
        .clone()
}

/// One section of the sheet, with the count asserted rather than indexed into blindly.
fn p115_section(c: &HeadlessClient, n: usize) -> String {
    let s = p115_sections(c);
    assert!(
        s.len() > n,
        "the sheet has been written and carries a section {n}: {s:?}"
    );
    s[n].clone()
}

/// How many times the composer has written the sheet.
fn p115_updates(c: &HeadlessClient) -> u32 {
    c.view().expect_app().hud().panels.character_info.updates
}

/// **What the live information-text element is actually showing**, read back rather than off the
/// panel's own cache.
fn p115_sheet_on_the_element(c: &mut HeadlessClient) -> String {
    el_text(c, dereth_ui_screens::panels::characterinfo::INFO_TEXT)
}

/// A `0x02CD Qualities_PrivateUpdateInt` for the player, **through the transport** -- bytes from
/// the shard rather than a model handed to the panel -- and the frames that consume it.
fn p115_set_int(c: &mut HeadlessClient, shard: &mut Peer, sequence: u8, property: u32, value: i32) {
    shard.event(
        c,
        &dereth_protocol::qualities::QualitiesPrivateUpdateInt(
            dereth_protocol::qualities::PrivateUpdate {
                sequence,
                property_id: property,
                value,
            },
        ),
    );
    c.tick(6);
}

/// A `0x01D1 Qualities_PrivateRemoveIntEvent` for the player, the same way.
fn p115_remove_int(c: &mut HeadlessClient, shard: &mut Peer, sequence: u8, property: u32) {
    shard.event(
        c,
        &dereth_protocol::qualities::QualitiesPrivateRemoveInt(
            dereth_protocol::qualities::PrivateRemove {
                sequence,
                property_id: property,
            },
        ),
    );
    c.tick(6);
}

/// The integer table of the **recording's own** character description, decoded out of the recorded
/// blob rather than read back out of the client.
///
/// This is the premise every claim below rests on, and it is asserted rather than assumed: a green
/// run over absent qualities would prove nothing, so each scenario carries the reading it needs.
/// It is the whole table in one call because the corpus is loaded to answer it.
fn p115_recorded_ints(session: &str) -> Vec<(u32, i32)> {
    let d = recorded_description(session);
    d.qualities
        .base
        .tables
        .ints
        .as_ref()
        .expect("the recorded description carries an integer table")
        .entries
        .clone()
}

/// One integer property of it.
fn p115_recorded_int(ints: &[(u32, i32)], property: u32) -> Option<i32> {
    ints.iter().find(|(k, _)| *k == property).map(|(_, v)| *v)
}

/// **The born line.** Creation-timestamp property `0x62` is gated on lookup success rather than
/// its numeric value. It is then converted through local time and formatted as `%c` into a
/// `0x400`-byte buffer.
///
/// **Not UTC, and not `asctime`'s shape.** The retail client's initialization calls
/// `setlocale(LC_ALL, "English")`, so `%c` is `English_United States.1252`'s
/// `M/d/yyyy h:mm:ss tt`, and the zone is the machine's. The expected string is therefore built
/// with the host's own offset, the same way the client builds it, so that this is green in every
/// zone -- **the zone is not what this pins**; the dates scenarios do that. The format is, and the
/// discrimination that matters is that the sentence is in the locale short-date shape and not in
/// `asctime`'s: no weekday, no month name, and an AM/PM that `asctime` has not got.
pub fn the_born_line_is_the_day_and_time_the_character_was_made() {
    use dereth_ui_screens::panels::characterinfo::prop;

    let ints = p115_recorded_ints(P115_SESSION);
    let premise = p115_recorded_int(&ints, prop::CREATION_TIMESTAMP) == Some(P115_CORPUS_CREATED);

    let (mut c, _shard) = p115_a_recorded_character_and_a_shard(P115_SESSION);
    p115_open_the_burden_sheet(&mut c);

    let composed = p115_updates(&c) > 0;
    let born = p115_section(&c, 0);
    let expected = format!(
        "You were born on {}.",
        dereth_ui_screens::ctime::strftime_c(
            i64::from(P115_CORPUS_CREATED),
            dereth_client::platform::local_utc_offset_secs(i64::from(P115_CORPUS_CREATED)),
        )
    );
    let says_it = born.contains(&expected);
    let on_the_element = p115_sheet_on_the_element(&mut c).contains(&expected);
    // `%c` under English_United States has no month name and no weekday...
    let not_asctime = !born.contains("Sep") && !born.contains("Fri");
    // ...and it does have AM/PM.
    let has_the_meridiem = born.contains(" AM.") || born.contains(" PM.");

    c.assert_behaviour(
        "character-sheet.born.the-line-is-the-day-and-time-the-character-was-made",
        move |_| {
            premise && composed && says_it && on_the_element && not_asctime && has_the_meridiem
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_born_line_is_the_day_and_time_the_character_was_made() {
    scenario("the_born_line_is_the_day_and_time_the_character_was_made");
}

/// **The playtime line**, and the first row in this workspace that needs the metalanguage.
///
/// Integer property `0x7D Age` is rendered through `ID_DurationFormat` on table
/// enum 2. The recorded character is **fifteen seconds** old, which exercises the interesting half
/// of the row: six of the seven terms are blank and `{ …[!b]}` has to delete each of them, along
/// with its separator, instead of rendering a brace.
///
/// The refresh is the second half of the claim and it is not decoration. Initialization
/// registers exactly one player-quality handler, for integer property `0x7D`, so `125` is *the*
/// id whose arrival has to recompose the
/// sheet. Both new figures go through the wire as a `0x02CD`, bytes and all.
///
/// Falsified by: dropping the arm; interleaving the row's pieces instead of rendering it (the
/// sheet reads `#1:{ year[1!b]| years[!b]}…`); rendering a blank term (`0 years 0 months …`);
/// making the plural a suffix rule rather than the block's own alternative.
pub fn the_playtime_line_spells_out_only_the_terms_that_are_not_zero() {
    use dereth_ui_screens::panels::characterinfo::prop;

    let premise =
        p115_recorded_int(&p115_recorded_ints(P115_SESSION), prop::AGE) == Some(P115_CORPUS_AGE);

    let (mut c, mut shard) = p115_a_recorded_character_and_a_shard(P115_SESSION);
    p115_open_the_burden_sheet(&mut c);

    let born_and_played = p115_section(&c, 0);
    let fifteen_seconds = born_and_played.contains("You have played for 15 seconds.");
    let no_markup = !born_and_played.contains('{') && !born_and_played.contains("#1:");

    let before = p115_updates(&c);
    p115_set_int(&mut c, &mut shard, 1, prop::AGE, 90_061);
    let recomposed = p115_updates(&c) > before;
    let after = p115_section(&c, 0);
    // 90 061 s is 1 day 1 hour 1 minute 1 second, each term singular.
    let singular = after.contains("You have played for 1 day 1 hour 1 minute 1 second.");
    let reached_the_element =
        p115_sheet_on_the_element(&mut c).contains("1 day 1 hour 1 minute 1 second");

    // The singular/plural choice is the block's, not a suffix rule: nudge it by one second.
    p115_set_int(&mut c, &mut shard, 2, prop::AGE, 90_062);
    let plural = p115_section(&c, 0).contains("1 day 1 hour 1 minute 2 seconds.");

    c.assert_behaviour(
        "character-sheet.played.the-line-spells-out-only-the-terms-that-are-not-zero",
        move |_| {
            premise
                && fifteen_seconds
                && no_markup
                && recomposed
                && singular
                && reached_the_element
                && plural
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_playtime_line_spells_out_only_the_terms_that_are_not_zero() {
    scenario("the_playtime_line_spells_out_only_the_terms_that_are_not_zero");
}

/// **The mastery lines.**
///
/// They are not with the fake skills: they are the **first three lines of the augmentations
/// section**. The three reads use
/// properties `0x162`, `0x163`, and `0x16A`.
///
/// The fake-skills section reads `ChessRank` and `FakeFishingSkill` and nothing else.
///
/// The recorded character has `354 = 1` and `355 = 8` and **no** `362`, so two lines appear and
/// one does not -- which is what makes this able to fail in both directions. It also carries no
/// `43 NumDeaths`, so the deaths line takes its zero arm; that reading is asserted here because it
/// is the premise the sections claim and the deaths claim both rest on.
///
/// Falsified by: dropping the section; naming the value instead of indexing the list (the sheet
/// says `1` and `8`); using the melee list for the ranged line (`8` would be *"Unknown"*);
/// emitting the summoning line for an absent quality.
pub fn the_mastery_lines_name_the_weapon_group_rather_than_the_number() {
    use dereth_ui_screens::panels::characterinfo::prop;

    // The whole recorded premise, in one place: the four integers the four lines are made of, and
    // the two absences that make two of the assertions below able to fail.
    let ints = p115_recorded_ints(P115_SESSION);
    let premise = p115_recorded_int(&ints, prop::CREATION_TIMESTAMP) == Some(P115_CORPUS_CREATED)
        && p115_recorded_int(&ints, prop::AGE) == Some(P115_CORPUS_AGE)
        // 354 = 1 is "Unarmed Weapons" on the melee list.
        && p115_recorded_int(&ints, prop::WEAPON_MASTERY) == Some(1)
        // 355 = 8 is "Bows" on the ranged one, which is indexed at [v - 8].
        && p115_recorded_int(&ints, prop::MISSILE_MASTERY) == Some(8)
        && p115_recorded_int(&ints, prop::SUMMONING_MASTERY).is_none()
        // ...and no deaths either, so that line takes its zero arm.
        && p115_recorded_int(&ints, P115_NUM_DEATHS).is_none();

    let (mut c, mut shard) = p115_a_recorded_character_and_a_shard(P115_SESSION);
    p115_open_the_burden_sheet(&mut c);

    let augs = p115_section(&c, 4);
    // 354 = 1 is the first line of the augmentations section.
    let melee = augs.starts_with("Your melee mastery is Unarmed Weapons.");
    // 355 = 8 indexes the *ranged* list at [v-8].
    let ranged = augs.contains("Your ranged mastery is Bows.");
    // The recording carries no 362, so the line must not be there.
    let no_summoning = !augs.contains("summoning mastery");
    let on_the_element = p115_sheet_on_the_element(&mut c).contains("Your ranged mastery is Bows.");

    // A shard that grants one moves the line, and the three summoning arms are named by value.
    p115_set_int(&mut c, &mut shard, 1, prop::SUMMONING_MASTERY, 2);
    // 362 = 2 maps to the displayed mastery group `Necromancer`.
    let summoning = p115_section(&c, 4).contains("Your summoning mastery is Necromancer.");

    c.assert_behaviour(
        "character-sheet.mastery.the-lines-name-the-weapon-group-rather-than-the-number-that-picks-it",
        move |_| premise && melee && ranged && no_summoning && on_the_element && summoning,
    );
    c.shutdown();
}

#[test]
fn scenario_the_mastery_lines_name_the_weapon_group_rather_than_the_number() {
    scenario("the_mastery_lines_name_the_weapon_group_rather_than_the_number");
}

/// **The luminance augmentations**, header and ratings.
///
/// `ID_CharacterInfo_Luminance_Header` is emitted with **no gate at all**, so it is
/// on every character's sheet; the eleven ratings behind it are each `if (v > 0)`, and four of
/// them are pairs split at 5 that can show two lines from one quality.
///
/// Each pair can therefore contribute both its capped base line and its specialized remainder.
///
/// Falsified by: dropping the section (no header); gating the header on having a rating; clamping
/// a single rating at five the way the pairs are clamped; forgetting the specialised half of a
/// pair; leaking the previous pair's specialised value into the next (the accumulator is cleared
/// between pairs, and omitting that reset would show a phantom line).
pub fn the_luminance_section_is_a_header_a_split_pair_and_an_unclamped_single() {
    let (mut c, mut shard) = p115_a_recorded_character_and_a_shard(P115_SESSION);
    p115_open_the_burden_sheet(&mut c);

    let augs = p115_section(&c, 4);
    let header_is_unconditional = augs.contains("Luminance Augmentations:");
    // The recording carries no rating, so no aura line may appear yet.
    let nothing_behind_it = !augs.contains("Aura of");

    // `0x14D LumAugDamageRating = 8` -> base 5 and specialised 3, i.e. **both** lines.
    p115_set_int(&mut c, &mut shard, 1, P115_LUM_DAMAGE_RATING, 8);
    // `0x152 LumAugSurgeChanceRating = 7` -> one line that says **7**, not 5: the singles have no
    // split at 5; this single has only a `> 0` gate.
    p115_set_int(&mut c, &mut shard, 2, P115_LUM_SURGE_CHANCE_RATING, 7);

    let augs = p115_section(&c, 4);
    let base_caps_at_five = augs.contains("Aura of Valor. Your Damage Rating is increased by 5.");
    let the_remainder_is_specialised =
        augs.contains("Your Seer grants you an increase to Damage Rating of 3.");
    let a_single_is_not_clamped =
        augs.contains("Your Aetheria's chance to surge is increased by a rating of 7.");
    // 0x14E is absent, so the next pair's specialised line must not appear; clearing the
    // specialised accumulator between pairs prevents the prior remainder from leaking into it.
    let no_leak_into_the_next_pair = !augs.contains("Aura of Invulnerability");
    let on_the_element = p115_sheet_on_the_element(&mut c).contains("Aura of Valor");

    c.assert_behaviour(
        "character-sheet.luminance.the-heading-is-always-there-and-a-rating-over-five-splits-in-two",
        move |_| {
            header_is_unconditional
                && nothing_behind_it
                && base_caps_at_five
                && the_remainder_is_specialised
                && a_single_is_not_clamped
                && no_leak_into_the_next_pair
                && on_the_element
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_luminance_section_is_a_header_a_split_pair_and_an_unclamped_single() {
    scenario("the_luminance_section_is_a_header_a_split_pair_and_an_unclamped_single");
}

/// **An augmentation row, and the plural block every one of them carries.**
///
/// `{time[1]|times}` is an **unnumbered** choice block matched against the row's unnumbered
/// variable, so it is the auto-derived `1` flag on the value that picks the singular. Without
/// `dereth_ui::text::metalanguage` the sheet would read *"3 {time[1]|times}"*.
pub fn an_augmentation_row_says_time_once_and_times_more_than_once() {
    let (mut c, mut shard) = p115_a_recorded_character_and_a_shard(P115_SESSION);
    p115_open_the_burden_sheet(&mut c);

    p115_set_int(&mut c, &mut shard, 1, P115_AUG_INNATE_STRENGTH, 1);
    let one = p115_section(&c, 4).contains("You have augmented your Innate Strength 1 time.");

    p115_set_int(&mut c, &mut shard, 2, P115_AUG_INNATE_STRENGTH, 3);
    let augs = p115_section(&c, 4);
    let three = augs.contains("You have augmented your Innate Strength 3 times.");
    let no_markup = !augs.contains('{');
    let on_the_element = p115_sheet_on_the_element(&mut c)
        .contains("You have augmented your Innate Strength 3 times.");

    c.assert_behaviour(
        "character-sheet.augmentations.a-row-says-time-once-and-times-more-than-once",
        move |_| one && three && no_markup && on_the_element,
    );
    c.shutdown();
}

#[test]
fn scenario_an_augmentation_row_says_time_once_and_times_more_than_once() {
    scenario("an_augmentation_row_says_time_once_and_times_more_than_once");
}

/// **The whole sheet, in the composer's order.** The composer appends six sections
/// separated by a literal `"\n"`, and augmentations is the **fifth** -- between the fake-skills
/// section and the load section.
///
/// A section list in the wrong order would pass every claim above and still draw the wrong sheet,
/// so the order is read twice: once off the panel's own sections and once off the live element,
/// where the three landmarks have to come in the same sequence.
pub fn the_six_sections_of_the_sheet_are_in_the_order_the_composer_appends_them() {
    let (mut c, _shard) = p115_a_recorded_character_and_a_shard(P115_SESSION);
    p115_open_the_burden_sheet(&mut c);

    let s = p115_sections(&c);
    // Six string appends, six sections.
    let six = s.len() == 6;
    let in_order = six
        && s[0].starts_with("You were born on")
        // ...and the third arm of the first section is still last in it.
        && s[0].contains("You have never died!")
        && s[1].starts_with("Natural Resistances:")
        && s[2].starts_with("Innate Strength:")
        && s[3].starts_with("Chess Rank:")
        && s[4].starts_with("Your melee mastery is")
        && s[5].starts_with("You are not overburdened");

    // And the whole thing reaches the element in that order.
    let text = p115_sheet_on_the_element(&mut c);
    let born = text.find("You were born on");
    let mastery = text.find("Your melee mastery");
    let load = text.find("You are not overburdened");
    let on_the_element = match (born, mastery, load) {
        (Some(b), Some(m), Some(l)) => b < m && m < l,
        _ => false,
    };

    c.assert_behaviour(
        "character-sheet.sections.the-six-parts-are-drawn-in-the-order-the-sheet-builds-them",
        move |_| six && in_order && on_the_element,
    );
    c.shutdown();
}

#[test]
fn scenario_the_six_sections_of_the_sheet_are_in_the_order_the_composer_appends_them() {
    scenario("the_six_sections_of_the_sheet_are_in_the_order_the_composer_appends_them");
}

/// **A property the shard clears disappears from the panel instead of sticking.**
///
/// This is the remove family's one claim that reads the shipped element tree; the others are
/// `cpu` scenarios.
///
/// The birth/age/deaths update's third arm reads integer property `43 NumDeaths` and picks one
/// of four plural forms, so this one integer is *text* on the character sheet. Seven deaths, then
/// a `0x01D1` clearing the property, and the line has to change back -- because lookup on a
/// deleted key answers false and the panel's pre-seeded default takes over, which is what
/// "disappears" means at the byte level.
///
/// What the sheet says for a character the shard has never told about deaths is captured *before*
/// the property exists, so the after-state is compared with a real reading and not with a string
/// this scenario invented.
///
/// Falsified by: deleting the remove arm from the HUD (the sheet keeps saying seven); making the
/// remove a no-op on the player's store (the same); leaving the panel's own cache unchanged so the
/// composer does not re-run.
pub fn a_removed_death_count_disappears_from_the_character_sheet() {
    use dereth_client_model::{StatKey, StatType};

    let (mut c, mut shard) = p115_a_recorded_character_and_a_shard(P115_SESSION);
    p115_open_the_burden_sheet(&mut c);

    let with_none = p115_section(&c, 0);
    let updates_at_start = p115_updates(&c);

    p115_set_int(&mut c, &mut shard, 1, P115_NUM_DEATHS, 7);
    let with_seven = p115_section(&c, 0);
    let seven_reached_the_sheet = with_seven.contains('7');
    let the_line_moved = with_seven != with_none;
    let recomposed_for_the_update = p115_updates(&c) > updates_at_start;
    let updates_before_remove = p115_updates(&c);

    // `0x01D1 Qualities_PrivateRemoveIntEvent` -- ACE clears `NumDeaths` nowhere, but the retail
    // client has the handler and this is the shape of every property a shard ever clears.
    p115_remove_int(&mut c, &mut shard, 2, P115_NUM_DEATHS);

    let key_is_gone = c
        .view()
        .world()
        .player_qualities()
        .expect("the player's store")
        .get(StatKey::new(StatType::Int, P115_NUM_DEATHS))
        .is_none();
    let after = p115_section(&c, 0);
    let stopped_saying_seven = !after.contains('7');
    let reads_as_it_did = after == with_none;
    // A stale cache would show the same text for the wrong reason.
    let recomposed_for_the_remove = p115_updates(&c) > updates_before_remove;

    c.assert_behaviour(
        "character-sheet.deaths.a-count-the-shard-clears-leaves-the-sheet",
        move |_| {
            seven_reached_the_sheet
                && the_line_moved
                && recomposed_for_the_update
                && key_is_gone
                && stopped_saying_seven
                && reads_as_it_did
                && recomposed_for_the_remove
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_removed_death_count_disappears_from_the_character_sheet() {
    scenario("a_removed_death_count_disappears_from_the_character_sheet");
}

// =============================================================================================
// skills.selection.*, skills.footer.*, skills.raise.*, skills.rows.*, character.header.*,
// spellbook.filter.*
//
// The character and spell panels *answering input*. Some drives below are the pointer and some
// the element message; the two are the same channel one layer apart, and each claim keeps the
// one it is measured through.
//
// The recorded character is `SESSION`'s, as it is for the rest of this file: the numbers every
// assertion here is against are recomputed in the scenario out of the recording's own character
// description and the shipped tables, read through `table` rather than back out of the panel, so
// the scenario's arithmetic and the panel's cannot share a mistake.
// =============================================================================================

/// One footer child of the **skills** sub-panel, resolved the way the panel resolves it -- the
/// sub-panel's own state picks the container, then the child id -- and `None` when that container
/// has no such child.
///
/// [`footer_child`] and [`panel_footer_child`] both panic on a missing child, and this file's one
/// claim about the *default* footer is that it carries no raise button at all, which is a
/// `None` and not a panic.
fn pinp_footer_child(c: &mut HeadlessClient, child: u32) -> Option<ElemHandle> {
    let (ui, screen) = gameplay_screen(c.app_mut());
    let root = screen.root()?;
    let page = ui.get_child_recursive(root, remaining::CHARACTER_PAGE)?;
    let panel = ui.get_child_recursive(page, skills::PANEL)?;
    let state = ui.node(panel).map_or(0, |n| n.state.0);
    let container =
        ui.get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))?;
    ui.get_child_recursive(container, ElementId(child))
}

/// What one footer child says, off the live element.
fn pinp_footer_text(c: &mut HeadlessClient, child: u32) -> String {
    match pinp_footer_child(c, child) {
        Some(h) => glyph_runs(c.app_mut(), h).0,
        None => String::new(),
    }
}

/// One footer child's own state -- which is what says whether a button is lit or out of reach.
fn pinp_footer_state(c: &mut HeadlessClient, child: u32) -> Option<u32> {
    let h = pinp_footer_child(c, child)?;
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(h).map(|n| n.state.0)
}

/// Every skill row's own state, in list order.
fn pinp_row_states(c: &mut HeadlessClient) -> Vec<u32> {
    let rows: Vec<ElemHandle> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.element)
        .collect();
    let (ui, _) = gameplay_screen(c.app_mut());
    rows.iter()
        .map(|h| ui.node(*h).map_or(0, |n| n.state.0))
        .collect()
}

/// Press the raise button under the picked skill **with the pointer**, and say what the hit test
/// found there -- so a scenario can say the press landed on that page's own button.
fn pinp_press_the_raise_button(
    c: &mut HeadlessClient,
    child: u32,
) -> (Option<ElemHandle>, ElemHandle) {
    let button = panel_footer_child(c, skills::PANEL, child);
    let (hit, _) = press_and_say_what_was_under_it(c, button);
    (hit, button)
}

/// Every raise-the-skill request this client has put in its outbox, as `(opcode, what, how much)`.
///
/// [`raises_asked_for`] answers for the three the ten-point family is about; this one adds the
/// training-in-credits request, which is the other arm of the same button.
fn pinp_skill_requests(c: &HeadlessClient) -> Vec<(u32, u32, u32)> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::TrainSkill(m) => Some((0x0046, m.skill_id, m.xp_spent)),
            dereth_client_model::Request::TrainSkillAdvancementClass(m) => {
                Some((0x0047, m.skill_id, m.credits_spent))
            }
            _ => None,
        })
        .collect()
}

/// The bytes one request really leaves as, on the queue it leaves on.
///
/// **The bytes, not the request.** The accumulating
/// outbox this harness keeps holds the request and not the frame it was written in, so the
/// encoding is taken the way the house-abandon scenario takes it: through the client's own
/// sender, into a transport that keeps what it was handed.
fn pinp_encoded(r: &dereth_client_model::Request) -> (bool, Vec<u8>) {
    let mut session = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );
    let framed = dereth_client::interaction::send_request(&mut session, r);
    let payload = session
        .transport
        .sent
        .last()
        .map(|p| p.payload.clone())
        .unwrap_or_default();
    (framed, payload)
}

/// The game action envelope the four requests below go out in: the ordered-action wrapper, the
/// first sequence number of a fresh session, the opcode, and then the message's own body.
fn pinp_action_bytes(opcode: u32, body: &[u8]) -> Vec<u8> {
    let mut want = 0xF7B1_u32.to_le_bytes().to_vec();
    want.extend_from_slice(&1_u32.to_le_bytes());
    want.extend_from_slice(&opcode.to_le_bytes());
    want.extend_from_slice(body);
    want
}

/// Flip a toggle button the way the button's own release flips it -- the toggled attribute goes
/// down **before** the click is raised, which is what lets the filter read the post-click answer.
///
/// **This channel is kept deliberately.** The claim is measured through
/// `broadcast_element_message`, which is what a completed pointer click ends in one layer further
/// down; the spellbook page is not raised anywhere in this family, and raising it to drive a
/// pointer would change what the claim is measured over.
fn pinp_toggle(c: &mut HeadlessClient, id: ElementId, on: bool) {
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        let h = ui
            .get_element(id)
            .expect("the filter button is in the shipped tree");
        statmgmt::set_toggle_button_state(ui, h, on);
        let want = if on {
            dereth_ui_screens::panels::spellbook::BUTTON_ON
        } else {
            dereth_ui_screens::panels::spellbook::BUTTON_OFF
        };
        assert_eq!(
            ui.node(h).map(|n| n.state.0),
            Some(want),
            "the toggle must land before the click reads it"
        );
    }
    c.tick(1);
}

/// The click a completed button press raises, on a button named by its id.
fn pinp_click(c: &mut HeadlessClient, id: ElementId) -> bool {
    let found = {
        let (ui, _) = gameplay_screen(c.app_mut());
        match ui.get_element(id) {
            Some(h) => {
                ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
                true
            }
            None => false,
        }
    };
    // Two frames, and the second is not slack: the packet controller runs near the top of a frame
    // and the command slot near the bottom, so this click's action becomes a datagram on the next
    // frame's controller pass.
    c.tick(2);
    found
}

/// Every spellbook-filter message this client has sent, in order.
fn pinp_filters_sent(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::SpellbookFilterEvent(m) => Some(m.filter_mask),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------

/// **The page comes up asking for a pick, and a press picks.** The default panel under the list
/// is the no-selection state and is what a second press on the same row brings back; a press on a
/// different row moves the pick rather than adding one.
pub fn the_skills_page_opens_asking_for_a_pick_and_a_press_picks_one() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);

    // Nothing picked: the default container, its title and two labels -- which are not written by
    // the client but resolved out of the shipped string table -- and no raise button at all.
    let opened_default = skills_panel_state(c.app_mut()) == Some(statmgmt::state::DEFAULT);
    let title = pinp_footer_text(&mut c, statmgmt::child::TITLE);
    let credits_label = pinp_footer_text(&mut c, statmgmt::child::LINE_ONE_LABEL);
    let experience_label = pinp_footer_text(&mut c, statmgmt::child::LINE_TWO_LABEL);
    let no_raise_button = pinp_footer_state(&mut c, statmgmt::child::BUTTON).is_none();

    let rows: Vec<u32> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.skill)
        .collect();
    assert!(
        rows.len() > 2,
        "the recorded character has rows to pick from ({})",
        rows.len()
    );
    let (first, second) = (rows[0], rows[1]);

    press_skill_row(&mut c, first);
    let picked = {
        let p = &c.view().expect_app().hud().panels.skills;
        (p.selected_skill, p.selected_index)
    } == (first, 0);
    let footer_swapped = skills_panel_state(c.app_mut()) != Some(statmgmt::state::DEFAULT);
    let states = pinp_row_states(&mut c);
    let picked_count = states
        .iter()
        .filter(|s| **s == statmgmt::row_state::SELECTED)
        .count();
    let exactly_one_is_picked = picked_count == 1
        && states[0] == statmgmt::row_state::SELECTED
        && states[1..]
            .iter()
            .all(|s| *s == statmgmt::row_state::UNSELECTED);

    // The same row again is the player un-picking it.
    press_skill_row(&mut c, first);
    let unpicked = {
        let p = &c.view().expect_app().hud().panels.skills;
        (p.selected_skill, p.selected_index)
    } == (0, -1);
    let back_to_default = skills_panel_state(c.app_mut()) == Some(statmgmt::state::DEFAULT);
    let title_again = pinp_footer_text(&mut c, statmgmt::child::TITLE);

    // And a different row moves the pick rather than adding a second one.
    press_skill_row(&mut c, second);
    let moved = c.view().expect_app().hud().panels.skills.selected_skill == second;
    let states = pinp_row_states(&mut c);
    let still_exactly_one = states
        .iter()
        .filter(|s| **s == statmgmt::row_state::SELECTED)
        .count()
        == 1
        && states[1] == statmgmt::row_state::SELECTED;

    c.assert_behaviour(
        "skills.selection.a-press-picks-a-skill-and-a-second-press-puts-the-footer-back",
        move |_| {
            opened_default
                && title == "Select a Skill to Improve"
                && credits_label == "Skill Credits Available:"
                && experience_label == "Unassigned Experience:"
                && no_raise_button
                && picked
                && footer_swapped
                && exactly_one_is_picked
                && unpicked
                && back_to_default
                && title_again == "Select a Skill to Improve"
                && moved
                && still_exactly_one
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_skills_page_opens_asking_for_a_pick_and_a_press_picks_one() {
    scenario("the_skills_page_opens_asking_for_a_pick_and_a_press_picks_one");
}

/// **The invisible tier.** A panel showing the wrong price to raise a skill looks perfectly
/// plausible, so every number below is recomputed here out of the recording's own description and
/// the two shipped tables, read through the dats rather than back out of the panel.
pub fn the_footer_under_a_picked_skill_is_that_characters_own_arithmetic() {
    let mut c = a_recorded_character(SESSION);
    let skill_table: dereth_assets::tables::SkillTable = table(&c, 0x0E00_0004);
    let xp: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);
    let q = recorded_qualities(SESSION);
    let desc = recorded_description(SESSION);

    // Every skill the recording says the character has already trained, with the two prices the
    // panel is meant to show for it.
    let table_of_skills = desc
        .qualities
        .skills
        .as_ref()
        .expect("the recording carries skills");
    let trained: std::collections::BTreeMap<u32, (u32, u32)> = table_of_skills
        .entries
        .iter()
        .filter(|(_, s)| s.sac >= 2)
        .map(|(id, _)| {
            (
                *id,
                (
                    dereth_client_model::advancement::skill_cost_to_raise(
                        &q,
                        &skill_table,
                        &xp,
                        *id,
                    ),
                    dereth_client_model::advancement::skill_cost_to_raise_10(&q, &xp, *id),
                ),
            )
        })
        .collect();
    assert!(
        !trained.is_empty(),
        "the recorded character has trained no skill, so this claim has no subject"
    );
    let available = match q.get(dereth_client_model::StatKey::new(
        dereth_client_model::StatType::Int64,
        2,
    )) {
        Some(dereth_client_model::StatValue::Int64(n)) => n,
        _ => 0,
    };

    let skill = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.skill)
        .find(|id| trained.contains_key(id))
        .expect("a trained skill has a row on the page");
    let (want_cost, want_cost_10) = trained[&skill];

    press_skill_row(&mut c, skill);
    let picked = c.view().expect_app().hud().panels.skills.selected_skill == skill;

    // A trained skill takes the container with the progress meter in it.
    let state = skills_panel_state(c.app_mut());
    let line_one_label = pinp_footer_text(&mut c, statmgmt::child::LINE_ONE_LABEL);
    let shown_cost = pinp_footer_text(&mut c, statmgmt::child::LINE_ONE_VALUE);
    let line_two_label = pinp_footer_text(&mut c, statmgmt::child::LINE_TWO_LABEL);
    let shown_available = pinp_footer_text(&mut c, statmgmt::child::LINE_TWO_VALUE);
    let cost_reads_right = if want_cost == 0 {
        // A skill that can go no further says so rather than showing a price.
        shown_cost == "Infinity!"
    } else {
        shown_cost == statmgmt::num(want_cost)
    };

    // The two buttons' states are the same two comparisons, made here from the same three numbers.
    let want_button = if want_cost == 0 || available < i64::from(want_cost) {
        statmgmt::button_state::DISABLED
    } else {
        statmgmt::button_state::ENABLED
    };
    let want_button_10 = if want_cost_10 == 0 || available < i64::from(want_cost_10) {
        statmgmt::button_state::DISABLED
    } else {
        statmgmt::button_state::ENABLED
    };
    let button = pinp_footer_state(&mut c, statmgmt::child::BUTTON);
    let button_10 = pinp_footer_state(&mut c, statmgmt::child::BUTTON_10);

    // How far through the current point the skill is, computed from the shipped curve.
    let s = q
        .skill(skill)
        .copied()
        .expect("the picked skill is in the recording");
    let sac = dereth_client_model::skills::Sac::from_raw(s.sac);
    let rank = usize::from(s.level_from_pp);
    let lo = dereth_client_model::advancement::experience_to_skill_level(&xp, sac, rank);
    let hi = dereth_client_model::advancement::experience_to_skill_level(&xp, sac, rank + 1);
    #[allow(clippy::cast_precision_loss)]
    let want_meter = if hi == lo {
        0.0f32
    } else {
        (s.pp - lo) as f32 / (hi.wrapping_sub(lo)) as f32
    };
    let meter = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .footer_content
        .meter;
    let meter_reads_right = meter.is_some_and(|m| (m - want_meter).abs() < 1e-6);

    c.assert_behaviour(
        "skills.footer.the-numbers-under-a-picked-skill-are-the-characters-own",
        move |_| {
            picked
                && state == Some(statmgmt::state::SELECTION_METER)
                && line_one_label == "Experience To Raise:"
                && cost_reads_right
                && line_two_label == "Unassigned Experience:"
                && shown_available == statmgmt::num(available)
                && button == Some(want_button)
                && button_10 == Some(want_button_10)
                && meter_reads_right
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_footer_under_a_picked_skill_is_that_characters_own_arithmetic() {
    scenario("the_footer_under_a_picked_skill_is_that_characters_own_arithmetic");
}

/// **What a press on the raise button really asks for**, on both kinds of skill, in bytes.
///
/// Nothing is spent on any shard: no server is contacted and every assertion is over the request
/// the client would send.
pub fn the_raise_button_spends_experience_or_credits_by_what_the_skill_is() {
    // ---- the trained arm ---------------------------------------------------------------------
    let mut c = a_recorded_character(SESSION);
    let skill_table: dereth_assets::tables::SkillTable = table(&c, 0x0E00_0004);
    let xp: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);
    let q = recorded_qualities(SESSION);
    let desc = recorded_description(SESSION);
    let entries = desc
        .qualities
        .skills
        .as_ref()
        .expect("the recording carries skills");
    let trained: Vec<u32> = entries
        .entries
        .iter()
        .filter(|(_, s)| s.sac >= 2)
        .map(|(id, _)| *id)
        .collect();

    let skill = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.skill)
        .find(|id| trained.contains(id))
        .expect("a trained skill has a row on the page");
    let want_xp =
        dereth_client_model::advancement::skill_cost_to_raise(&q, &skill_table, &xp, skill);

    press_skill_row(&mut c, skill);
    let (hit, button) = pinp_press_the_raise_button(&mut c, statmgmt::child::BUTTON);
    let landed_on_the_button = hit == Some(button);
    let asked = pinp_skill_requests(&c);
    let one_raise = asked == vec![(0x0046, skill, want_xp)];

    // And the bytes that raise carries, through the client's own sender.
    let mut body = skill.to_le_bytes().to_vec();
    body.extend_from_slice(&want_xp.to_le_bytes());
    let request = dereth_client_model::Request::TrainSkill(dereth_protocol::admin::TrainSkill {
        skill_id: skill,
        xp_spent: want_xp,
    });
    let (framed, payload) = pinp_encoded(&request);
    let bytes_are_right = framed && payload == pinp_action_bytes(0x0046, &body);

    // The double-spend latch: the button puts itself out of reach and a second press asks nothing.
    let latched = c.view().expect_app().hud().panels.skills.awaiting_raise;
    let greyed = pinp_footer_state(&mut c, statmgmt::child::BUTTON)
        == Some(statmgmt::button_state::DISABLED);
    let _ = pinp_press_the_raise_button(&mut c, statmgmt::child::BUTTON);
    let still_one = pinp_skill_requests(&c) == vec![(0x0046, skill, want_xp)];
    c.shutdown();

    // ---- the untrained arm -------------------------------------------------------------------
    let mut c = a_recorded_character(SESSION);
    let untrained = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.skill)
        .find(|id| q.skill(*id).is_none_or(|s| s.sac <= 1) && skill_table.skills.contains_key(id))
        .expect("the recorded character has an untrained skill with a row");
    let want_credits =
        u32::try_from(skill_table.skills[&untrained].trained_cost).expect("a non-negative cost");

    press_skill_row(&mut c, untrained);
    // An untrained skill takes the container *without* a meter, and its line talks about credits.
    let untrained_state = skills_panel_state(c.app_mut());
    let credits_label = pinp_footer_text(&mut c, statmgmt::child::LINE_ONE_LABEL);
    let credits_value = pinp_footer_text(&mut c, statmgmt::child::LINE_ONE_VALUE);

    let (hit, button) = pinp_press_the_raise_button(&mut c, statmgmt::child::BUTTON);
    let landed_on_the_untrained_button = hit == Some(button);
    let asked_in_credits = pinp_skill_requests(&c) == vec![(0x0047, untrained, want_credits)];
    let mut credit_body = untrained.to_le_bytes().to_vec();
    credit_body.extend_from_slice(&want_credits.to_le_bytes());
    let credit_request = dereth_client_model::Request::TrainSkillAdvancementClass(
        dereth_protocol::admin::TrainSkillAdvancementClass {
            skill_id: untrained,
            credits_spent: want_credits,
        },
    );
    let (credit_framed, credit_payload) = pinp_encoded(&credit_request);
    let credit_bytes_are_right =
        credit_framed && credit_payload == pinp_action_bytes(0x0047, &credit_body);

    // The gate on the credits arm is not decorative: the same sender refuses a skill the character
    // already has, so a press that reached the wrong arm would send nothing rather than the wrong
    // thing.
    let gate_holds = {
        let mut req = dereth_client_model::RecordingRequests::default();
        let takes_the_untrained =
            dereth_client_model::advancement::send_train_skill_advancement_class(
                &q,
                &mut req,
                untrained,
                want_credits,
            );
        let mut refused = dereth_client_model::RecordingRequests::default();
        let refuses_the_trained =
            !dereth_client_model::advancement::send_train_skill_advancement_class(
                &q,
                &mut refused,
                skill,
                1,
            );
        takes_the_untrained && refuses_the_trained && refused.0.is_empty()
    };

    c.assert_behaviour(
        "skills.raise.the-button-spends-experience-on-a-trained-skill-and-credits-on-an-untrained-one",
        move |_| {
            landed_on_the_button
                && one_raise
                && bytes_are_right
                && latched
                && greyed
                && still_one
                && untrained_state == Some(statmgmt::state::SELECTION)
                && credits_label == "Skill Credits To Train"
                && credits_value == want_credits.to_string()
                && landed_on_the_untrained_button
                && asked_in_credits
                && credit_bytes_are_right
                && gate_holds
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_raise_button_spends_experience_or_credits_by_what_the_skill_is() {
    scenario("the_raise_button_spends_experience_or_credits_by_what_the_skill_is");
}

/// **The colour, not the lettering.** The three colours a skill row's value cell ships are the
/// whole of its palette, and a number asked for a colour the cell does not have is drawn plain
/// rather than not at all -- which is the half that was really broken, because an index the cell
/// does not declare resolves to nothing and rasterises nothing.
///
/// The premise is asserted rather than assumed: this recording's character carries the
/// jack-of-all-trades augmentation and no spell at all, so every skill is raised by the same flat
/// amount and every row legitimately draws in the raised colour. A recording without it would
/// change the expected set, and this says so instead of quietly agreeing.
pub fn a_skill_values_colour_is_one_of_the_rows_own_three() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let skill_table: dereth_assets::tables::SkillTable = table(&c, 0x0E00_0004);
    let q = recorded_qualities(SESSION);

    // ---- the premise -------------------------------------------------------------------------
    let desc = recorded_description(SESSION);
    let enchantments = desc.qualities.enchantments.as_ref().map_or(0, |e| {
        e.multiplicative.as_ref().map_or(0, Vec::len)
            + e.additive.as_ref().map_or(0, Vec::len)
            + usize::from(e.vitae.is_some())
    });
    assert_eq!(
        enchantments, 0,
        "this recording must carry no spell; it carries {enchantments}"
    );
    assert_eq!(
        q.inq_int(dereth_client_model::skills::aug::JACK_OF_ALL_TRADES),
        1,
        "this character must carry jack-of-all-trades for the flat difference below to be it"
    );

    // ---- the oracle, recomputed here ---------------------------------------------------------
    let want: std::collections::BTreeMap<u32, u32> = skill_table
        .skills
        .keys()
        .map(|id| {
            let raw =
                dereth_client_model::skills::inq_skill(&q, &skill_table, *id, true).unwrap_or(0);
            let eff =
                dereth_client_model::skills::inq_skill(&q, &skill_table, *id, false).unwrap_or(0);
            assert_eq!(
                i64::from(eff) - i64::from(raw),
                5,
                "skill {id}: the only source of a difference here is the augmentation"
            );
            (*id, ladder(i64::from(raw), i64::from(eff)))
        })
        .collect();
    let thirty_eight = want.len() == 38 && want.values().all(|f| *f == 1);

    let rows = c.view().expect_app().hud().panels.skills.rows.clone();
    let got: std::collections::BTreeMap<u32, u32> =
        rows.iter().map(|r| (r.skill, r.font)).collect();
    let every_row_agrees = rows.len() == 38 && got == want;

    // ---- the palette the cell itself ships ---------------------------------------------------
    let value_child = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.get_child_recursive(rows[0].element, ElementId(skills::row::VALUE))
            .expect("the row has a value cell")
    };
    // Three colours, and the words a reader would use for them: white plain, green raised, red
    // lowered. A read that went back through the same constant could not see a wrong one.
    let (white, green, red) = state_colours(c.app_mut(), value_child);
    let one_lettering_three_colours = {
        let (ui, _) = gameplay_screen(c.app_mut());
        statmgmt::font_count(ui, value_child) == 1
            && statmgmt::font_color_at(ui, value_child, 3).is_none()
    };

    // ---- and the colour really reaches the glyphs --------------------------------------------
    let raised = glyph_runs(c.app_mut(), value_child).1;
    let raised_is_green =
        !raised.is_empty() && raised.iter().all(|(f, col)| *f == 0 && *col == green);
    // It must also resolve to a real lettering, which is the half that was broken: a cell asked
    // for a lettering it does not declare composes nothing at all.
    let really_composed = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let sb = ui.screen_box(value_child);
        let placed = ui
            .text_element_mut(value_child)
            .map_or_else(Vec::new, |t| t.compose(sb));
        !placed.is_empty() && placed.iter().all(|p| p.font.0 != 0)
    };

    {
        let (ui, _) = gameplay_screen(c.app_mut());
        statmgmt::set_text_with_font(ui, value_child, "45", 0, 2);
    }
    let lowered_is_red = glyph_runs(c.app_mut(), value_child).1 == vec![(0, red), (0, red)];

    // A colour past the end of the cell's own three leaves the colour alone; it must never blank
    // the cell.
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        statmgmt::set_text_with_font(ui, value_child, "45", 9, 9);
    }
    let out_of_range_falls_back =
        glyph_runs(c.app_mut(), value_child).1 == vec![(0, white), (0, white)];

    {
        let (ui, _) = gameplay_screen(c.app_mut());
        if let Some(t) = ui.text_element_mut(value_child) {
            t.set_text("678");
        }
    }
    let plain_text_is_plain = glyph_runs(c.app_mut(), value_child)
        .1
        .iter()
        .all(|(f, _)| *f == 0);

    c.assert_behaviour(
        "skills.rows.the-colour-a-value-is-drawn-in-is-one-of-the-rows-own-three",
        move |_| {
            thirty_eight
                && every_row_agrees
                && one_lettering_three_colours
                && raised_is_green
                && really_composed
                && lowered_is_red
                && out_of_range_falls_back
                && plain_text_is_plain
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_skill_values_colour_is_one_of_the_rows_own_three() {
    scenario("a_skill_values_colour_is_one_of_the_rows_own_three");
}

/// **The heading a paired sweep found empty.** The three labels come from the shipped layout; the
/// values beside them were written by nothing, and a page built from the rows up cannot see a
/// blank heading. Both sub-panels are read, because each carries its own copy of the eight
/// elements.
pub fn both_character_pages_head_with_the_name_the_level_and_the_experience() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let xp: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);
    let q = recorded_qualities(SESSION);

    // ---- the oracle, off the recording's own description -------------------------------------
    let total = match q.get(dereth_client_model::StatKey::new(
        dereth_client_model::StatType::Int64,
        1,
    )) {
        Some(dereth_client_model::StatValue::Int64(n)) => u64::try_from(n).unwrap_or(0),
        _ => 0,
    };
    let level = q.inq_int(0x19);
    assert!(level > 0, "the recorded character has a level ({level})");
    let this_level = dereth_client_model::advancement::experience_to_level(
        &xp,
        usize::try_from(level).unwrap_or(0),
    )
    .expect("the level is inside the shipped curve");
    let next_level = dereth_client_model::advancement::experience_to_level(
        &xp,
        usize::try_from(level).unwrap_or(0) + 1,
    )
    .expect("and so is the next one");
    let to_level = next_level.saturating_sub(total);
    // The name's oracle is the recording's own, read off the description: the object table is not
    // it, because a client is in exactly this window between the description and being put in the
    // world, and the heading has to be right there too.
    let name = match q.get(dereth_client_model::StatKey::new(
        dereth_client_model::StatType::String,
        1,
    )) {
        Some(dereth_client_model::StatValue::Str(n)) => n,
        _ => String::new(),
    };
    assert!(!name.is_empty(), "the recording names the character");
    #[allow(clippy::cast_precision_loss)]
    let want_meter = (total - this_level) as f32 / (next_level - this_level) as f32;

    // ---- what each sub-panel worked out ------------------------------------------------------
    let mut both_halves_agree = true;
    for got in [
        c.view()
            .expect_app()
            .hud()
            .panels
            .skills
            .header_content
            .clone(),
        c.view()
            .expect_app()
            .hud()
            .panels
            .attributes
            .header_content
            .clone(),
    ] {
        both_halves_agree &= got.name == name
            && got.level == statmgmt::num(level)
            && got.total_xp == statmgmt::num(total)
            && got.xp_to_level == statmgmt::num(to_level)
            && (got.meter - want_meter).abs() < 1e-6;
    }

    // ---- and it is on the live elements, not only in the panel's own copy ---------------------
    let mut both_halves_drew_it = true;
    for panel in [skills::PANEL, attributes::PANEL] {
        for (id, want) in [
            (statmgmt::header::NAME, name.clone()),
            (statmgmt::header::LEVEL, statmgmt::num(level)),
            (statmgmt::header::TOTAL_XP, statmgmt::num(total)),
            (statmgmt::header::XP_TO_LEVEL, statmgmt::num(to_level)),
        ] {
            let h = {
                let (ui, screen) = gameplay_screen(c.app_mut());
                let root = screen.root().expect("the gameplay screen has a root");
                let page = ui
                    .get_child_recursive(root, remaining::CHARACTER_PAGE)
                    .expect("the character page is in the shipped layout");
                let p = ui
                    .get_child_recursive(page, panel)
                    .expect("the sub-panel is in the layout");
                ui.get_child_recursive(p, ElementId(id))
                    .unwrap_or_else(|| panic!("{panel:?} has no {id:#010X}"))
            };
            both_halves_drew_it &= glyph_runs(c.app_mut(), h).0 == want;
        }
    }

    c.assert_behaviour(
        "character.header.the-character-page-heads-with-the-name-the-level-and-the-experience",
        move |_| both_halves_agree && both_halves_drew_it,
    );
    c.shutdown();
}

#[test]
fn scenario_both_character_pages_head_with_the_name_the_level_and_the_experience() {
    scenario("both_character_pages_head_with_the_name_the_level_and_the_experience");
}

/// **The mask is the invisible half.** The book filters on one set of bits and the wire carries
/// another, and the only symptom would be a spell quietly missing. The oracle for what is sent is
/// the recording's own filter word with exactly the pressed button's bit cleared.
pub fn turning_a_school_off_hides_its_spells_and_sends_the_whole_list() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);

    let mask0 = recorded_description(SESSION).player_module.spell_filters;
    let before = c.view().expect_app().hud().panels.spellbook.shown.clone();
    assert!(
        !before.is_empty(),
        "the recorded spellbook has spells to filter"
    );

    // The buttons already say which schools the shard said are showing -- all thirteen of them,
    // read off the live tree.
    let buttons_match_the_shard = {
        let (ui, _) = gameplay_screen(c.app_mut());
        dereth_ui_screens::panels::spellbook::FILTER_BUTTONS
            .iter()
            .all(|(id, bit)| {
                let h = ui
                    .get_element(ElementId(*id))
                    .expect("the filter button is in the tree");
                let want = if mask0 & bit != 0 {
                    dereth_ui_screens::panels::spellbook::BUTTON_ON
                } else {
                    dereth_ui_screens::panels::spellbook::BUTTON_OFF
                };
                ui.node(h).map(|n| n.state.0) == Some(want)
            })
    };

    // Which school every spell on show belongs to -- the one turned off has to be showing.
    let school_of: std::collections::BTreeMap<u32, u32> = {
        use dereth_ui_screens::view::GameView as _;
        let app = c.view().expect_app();
        let view = app.hud().view(app.objects());
        view.spellbook().iter().map(|s| (s.id, s.school)).collect()
    };
    let (button, bit, school) = dereth_ui_screens::panels::spellbook::School::ALL
        .iter()
        .filter_map(|s| {
            let bit = dereth_ui_screens::panels::spellbook::filter_bit(s.button())?;
            let m = s.magic_school();
            before
                .iter()
                .any(|id| school_of.get(id) == Some(&m))
                .then_some((s.button(), bit, m))
        })
        .next()
        .expect("some spell on show belongs to a school with a button of its own");

    pinp_toggle(&mut c, button, false);
    let found = pinp_click(&mut c, button);

    let after = c.view().expect_app().hud().panels.spellbook.shown.clone();
    let the_list_changed = after != before;
    let that_school_is_gone = after.iter().all(|id| school_of.get(id) != Some(&school));
    // ...and one of them was there before, or this proves nothing.
    let it_was_there_before = before.iter().any(|id| school_of.get(id) == Some(&school));

    let sent_off = pinp_filters_sent(&c);
    let client_side = {
        use dereth_ui_screens::view::GameView as _;
        let app = c.view().expect_app();
        app.hud().view(app.objects()).spell_filters()
    };
    let (framed_off, payload_off) =
        pinp_encoded(&dereth_client_model::Request::SpellbookFilterEvent(
            dereth_protocol::combat::CharacterSpellbookFilterEvent {
                filter_mask: mask0 & !bit,
            },
        ));
    let off_bytes_are_right =
        framed_off && payload_off == pinp_action_bytes(0x0286, &(mask0 & !bit).to_le_bytes());

    // Turning it back on restores the book exactly, which is what says the buttons filter rather
    // than edit -- and the message is the whole list again, not the one bit that moved.
    pinp_toggle(&mut c, button, true);
    pinp_click(&mut c, button);
    let the_book_came_back = c.view().expect_app().hud().panels.spellbook.shown == before;
    let sent_both = pinp_filters_sent(&c);

    c.assert_behaviour(
        "spellbook.filter.turning-a-school-off-hides-its-spells-and-sends-the-whole-list",
        move |_| {
            found
                && buttons_match_the_shard
                && the_list_changed
                && that_school_is_gone
                && it_was_there_before
                && sent_off == vec![mask0 & !bit]
                && client_side == mask0 & !bit
                && off_bytes_are_right
                && the_book_came_back
                && sent_both == vec![mask0 & !bit, mask0]
        },
    );
    c.shutdown();
}

#[test]
fn scenario_turning_a_school_off_hides_its_spells_and_sends_the_whole_list() {
    scenario("turning_a_school_off_hides_its_spells_and_sends_the_whole_list");
}

// =============================================================================================
// minigame.*
//
// The chess board: the window, the seat, the moves, and every chess message.
//
// **No recording is the oracle here and that is measured**, by the first scenario below: all
// eleven of the messages a board game is played in are corpus zeros, so every one the shard says
// is built by this file in the writer order the reference server uses and fed as a real datagram
// through `Peer`. Nothing binds a socket; every byte the client writes stays in its own endpoint.
// =============================================================================================

use dereth_client_model::chess::{
    move_result as chess_mr, Coord as ChessCoord, GameState, PieceType,
};
use dereth_client_model::minigame::GameBoard as ChessBoard;
use dereth_ui_screens::panels::minigame;

/// The player, and the two boards: a landblock-static guid is the shape a fixed piece of furniture
/// in the world has.
const CHESS_PLAYER: ObjectId = ObjectId(0x5000_0001);
const CHESS_BOARD: ObjectId = ObjectId(0x79DA_F100);
const CHESS_OTHER_BOARD: ObjectId = ObjectId(0x79DA_F200);

/// The shipped mini-game lamp in the indicator strip.
const CHESS_LAMP: ElementId = ElementId(0x1000_00F3);

/// A client in the world with a shard behind it and two boards standing in front of it.
fn chess_a_client_and_two_boards() -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    // The shard creates the player, because an ordered event is addressed to an object and the
    // client's ordered queue only has somewhere to put one for an object it has been told about.
    let peer = Peer::attach_creating(&mut c, CHESS_PLAYER);
    assert!(
        c.world_mut().set_player(CHESS_PLAYER),
        "the identity is adopted once"
    );
    c.world_mut()
        .weenie_mut(CHESS_PLAYER)
        .expect("the shard's create reached the world")
        .pwd
        .name = "Larktest".to_string();
    for id in [CHESS_BOARD, CHESS_OTHER_BOARD] {
        chess_put_a_board_in_the_world(&mut c, id);
    }
    c.tick(6);
    (c, peer)
}

/// A board as the shard describes one, and **every one of these is load-bearing** for a
/// double-click on it to come out as *begin a game* rather than *put it in your backpack*: it is a
/// game piece, it is stuck to the world, it is in no container and no hand, and it is not the
/// player's own.
fn chess_put_a_board_in_the_world(c: &mut HeadlessClient, id: ObjectId) {
    let mut w = dereth_client_model::Weenie::default();
    w.pwd.obj_type = dereth_client_model::weenie::item_type::GAMEBOARD;
    w.pwd.bitfield |= dereth_client_model::weenie::bitfield::STUCK;
    w.pwd.name = format!("Chess Board {:X}", id.0);
    c.world_mut().tables.weenies.insert(id, w);
}

/// Use a board, which is the gesture every double-click in this client funnels into.
fn chess_use_the_board(c: &mut HeadlessClient, id: ObjectId) {
    c.when(Player::DoubleClick(id));
    c.tick(6);
}

/// One thing the shard says about a game, as a real ordered datagram, and the frames that read it.
fn chess_the_shard_says<M: dereth_protocol::Message>(
    c: &mut HeadlessClient,
    peer: &mut Peer,
    m: &M,
) {
    peer.event(c, m);
    c.tick(6);
}

fn chess_panel(c: &HeadlessClient) -> &dereth_ui_screens::panels::minigame::MiniGamePanel {
    &c.view().expect_app().hud().panels.minigame
}

fn chess_model(c: &HeadlessClient) -> &dereth_client_model::minigame::MiniGame {
    &c.view().world().minigame
}

/// Every request about a game this client has put in its outbox, in order.
fn chess_wire(c: &HeadlessClient) -> Vec<dereth_client_model::Request> {
    use dereth_client_model::Request as R;
    c.view()
        .outbound()
        .iter()
        .filter(|r| {
            matches!(
                r,
                R::GameJoin(_)
                    | R::GameQuit(_)
                    | R::GameMove(_)
                    | R::GameMovePass(_)
                    | R::GameStalemate(_)
            )
        })
        .cloned()
        .collect()
}

/// The ones sent since `mark`, which is what a scenario takes before the gesture it is about.
fn chess_wire_since(c: &HeadlessClient, mark: usize) -> Vec<dereth_client_model::Request> {
    chess_wire(c).into_iter().skip(mark).collect()
}

/// The **main chat window**'s log, which is where every line this window composes ends up. Not the
/// world's scroll: the frame drains that into the chat windows, so a scenario that settles and
/// then reads the scroll reads an empty one.
fn chess_chat_log(c: &mut HeadlessClient) -> String {
    let (_, screen) = gameplay_screen(c.app_mut());
    screen
        .chat
        .iter()
        .find(|w| w.window_id == 8)
        .expect("the main chat window is in the shipped frame")
        .log_text()
}

/// The last thing the window said. The lines it composes carry their own newline and the scroll
/// trims both ends, so every expectation below is trimmed too.
fn chess_last_said(c: &mut HeadlessClient) -> String {
    chess_chat_log(c)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .next_back()
        .unwrap_or_default()
        .to_owned()
}

/// The newest line in the strip across the top of the viewport. The two refusals a board's window
/// makes go there and **not** to a chat window -- a distinction the client makes, and one a
/// scenario reading one surface for both would have papered over.
fn chess_last_spewed(c: &HeadlessClient) -> String {
    c.view()
        .expect_app()
        .hud()
        .panels
        .spew
        .model
        .items
        .first()
        .cloned()
        .unwrap_or_default()
}

fn chess_lamp(c: &HeadlessClient) -> ElemHandle {
    let ui = &c.view().expect_app().ui().expect("the ui shell is up").ui;
    ui.get_child_recursive(ui.root(), CHESS_LAMP)
        .expect("the shipped mini-game lamp")
}

fn chess_lamp_state(c: &HeadlessClient) -> u32 {
    let h = chess_lamp(c);
    let ui = &c.view().expect_app().ui().expect("the ui shell is up").ui;
    ui.node(h).expect("the lamp is alive").state.0
}

/// The lit lamp must resolve through the shipped state description all the way to a visible image
/// draw, or the scenario would be accepting a state nothing painted.
fn chess_lamp_is_lit(c: &HeadlessClient) -> dereth_primitives::DataId {
    let h = chess_lamp(c);
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the ui shell is up").ui;
    let node = ui.node(h).expect("the lamp is alive");
    assert_eq!(
        node.state.0, 1,
        "the notice puts the lamp in its own lit state"
    );
    assert!(ui.is_visible(h), "the lit lamp is visible");
    assert_eq!(
        node.merged_properties()
            .get_bool(dereth_ui::props::attr::DISABLED),
        Some(false),
        "the lit state re-enables the lamp"
    );
    let did = node
        .region
        .image
        .as_ref()
        .expect("the lit state supplies its shipped image")
        .did;
    let draw = app
        .ui_draw_list()
        .iter()
        .find(|cmd| cmd.who == h)
        .expect("the lit lamp reaches the frame's draw list");
    assert_eq!(draw.image, Some(did), "the drawn image is the shipped one");
    did
}

/// The twelve piece pictures, read off the board's own shipped property rather than out of a table
/// of this file's own.
fn chess_piece_pictures(c: &HeadlessClient) -> [dereth_primitives::DataId; 12] {
    let list = chess_panel(c)
        .board
        .as_ref()
        .expect("the board is bound")
        .handle;
    let app = c.view().expect_app();
    let props = app
        .ui()
        .expect("the ui shell is up")
        .ui
        .node(list)
        .expect("the board's list node")
        .merged_properties();
    let dereth_ui::PropertyValue::Array(members) = props
        .get(minigame::PIECE_ICON_ARRAY)
        .expect("the shipped piece picture array")
    else {
        panic!("the piece picture property is an array")
    };
    members
        .iter()
        .map(|member| match member.value {
            dereth_ui::PropertyValue::DataFile(did) => did,
            ref value => panic!("a piece picture is a data file, got {value:?}"),
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("the shipped array has exactly twelve piece pictures")
}

/// Every one of those pictures really decodes to pixels out of the shipped data.
fn chess_pieces_have_pixels(c: &HeadlessClient, dids: &[dereth_primitives::DataId; 12]) -> bool {
    use dereth_primitives::AssetSource as _;
    let store = std::sync::Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    dids.iter().all(|did| {
        let Ok(bytes) = store.read(*did) else {
            return false;
        };
        match <dereth_assets::RenderSurface as dereth_assets::Decode>::decode_payload(*did, &bytes)
        {
            Ok(s) => s.width > 0 && s.height > 0 && s.image_size > 0,
            Err(_) => false,
        }
    })
}

/// A square has gone through the panel and the renderer: its own region carries the shipped
/// picture, the draw mode the board asks for, and this frame asks the renderer to blit it.
fn chess_square_draws(c: &HeadlessClient, cell: usize, did: dereth_primitives::DataId) -> bool {
    let h = chess_panel(c)
        .board
        .as_ref()
        .expect("the board is bound")
        .items[cell];
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the ui shell is up").ui;
    let Some(node) = ui.node(h) else { return false };
    let cmd = app.ui_draw_list().iter().find(|cmd| cmd.who == h);
    ui.is_visible(h)
        && node.region.image.as_ref().map(|i| i.did) == Some(did)
        && node.region.blit_mode == dereth_ui::BlitMode::Alpha4
        && cmd.is_some_and(|cmd| {
            cmd.image == Some(did) && cmd.blit_mode == dereth_ui::BlitMode::Alpha4
        })
}

/// An empty square draws nothing, and leaves no stale command behind either.
fn chess_square_is_empty(c: &HeadlessClient, cell: usize) -> bool {
    let h = chess_panel(c)
        .board
        .as_ref()
        .expect("the board is bound")
        .items[cell];
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the ui shell is up").ui;
    !ui.is_visible(h) && !app.ui_draw_list().iter().any(|cmd| cmd.who == h)
}

/// Press one square **through the real tree**: the pointer goes to the middle of that square's own
/// drawn box, so the hit test and the list's own working-out of which square was pressed are both
/// production. A scenario that handed the panel a square number would measure nothing.
fn chess_press_square(c: &mut HeadlessClient, cell: usize) {
    let h = chess_panel(c)
        .board
        .as_ref()
        .expect("the board is bound")
        .items[cell];
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    c.when(Player::Click(Target::Point(at)));
    c.tick(6);
}

/// Press one of the window's three buttons.
///
/// **This channel is kept deliberately**: the click a completed button press raises,
/// broadcast on the shipped element. One of the three -- the pass button -- is never shown, so a
/// pointer gesture cannot reach it at all, and driving the other two differently from the third
/// would make one scenario measure two things.
fn chess_press_button(c: &mut HeadlessClient, id: ElementId) {
    {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        let h = ui
            .get_child_recursive(root, id)
            .unwrap_or_else(|| panic!("{id:?} is in the shipped tree"));
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
    }
    c.tick(6);
}

/// Whether the window's question has gone away.
fn chess_no_question_is_up(c: &HeadlessClient) -> bool {
    c.view()
        .expect_app()
        .ui()
        .expect("the ui shell is up")
        .ui
        .dialogs
        .non_queued()
        .is_empty()
}

/// The one question the window puts up, on the all-at-once dialog list: its context and its root.
fn chess_resign_question(c: &HeadlessClient) -> (u64, ElemHandle) {
    let dialogs = c
        .view()
        .expect_app()
        .ui()
        .expect("the ui shell is up")
        .ui
        .dialogs
        .non_queued();
    let [info] = dialogs else {
        panic!(
            "resigning raises one visible question, got {}",
            dialogs.len()
        )
    };
    assert_eq!(info.kind, dereth_ui::dialog::base::DialogKind::Confirmation);
    (
        info.context,
        info.element.expect("the shipped question has a tree"),
    )
}

/// What one element of the question says.
fn chess_dialog_text(c: &mut HeadlessClient, root: ElemHandle, child: ElementId) -> String {
    let h = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.get_child_recursive(root, child)
            .expect("the question carries this element")
    };
    glyph_runs(c.app_mut(), h).0
}

/// Answer the question with a **real hit-tested press**, the way
/// [`answer_the_dialog`] answers the one on the default queue.
fn chess_answer(
    c: &mut HeadlessClient,
    hand: &mut dereth_testkit::adapters_chat::Hand,
    root: ElemHandle,
    child: ElementId,
) {
    let at = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the ui shell is up").ui;
        let h = ui
            .get_child_recursive(root, child)
            .expect("the question carries this answer");
        let r = ui.screen_clip_box(h);
        assert!(r.is_valid(), "the answer has a real visible clip");
        let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        assert!(
            ui.hit_test_screen(at.0, at.1)
                .is_some_and(|hit| hit == h || ui.is_ancestor_of(h, hit)),
            "the answer is the thing under the pointer"
        );
        at
    };
    hand.press_at(c, at.0, at.1);
    c.tick(6);
}

/// Take a seat as `my_team` and let `first_team` have the first move.
///
/// The two are separate because the local engine always gives the first move to team 0, so a start
/// naming team 1 leaves a team-0 player waiting and a team-1 player to move. That is the shard's
/// arrangement, measured, and it is why no scenario here takes the second seat and then moves.
///
/// **This is a fixture and not a claim**: the seat request it sends is what
/// [`using_a_board_raises_the_window_and_asks_for_a_seat`] is about, so every caller takes its own
/// mark afterwards and never counts it.
fn chess_sit_down(c: &mut HeadlessClient, peer: &mut Peer, my_team: i32, first_team: i32) {
    chess_use_the_board(c, CHESS_BOARD);
    chess_the_shard_says(
        c,
        peer,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: my_team,
        },
    );
    chess_the_shard_says(
        c,
        peer,
        &dereth_protocol::trade::GameStartGame {
            game_id: CHESS_BOARD.0,
            team: first_team,
        },
    );
}

/// The shard's *"the opponent moved"*, in the from-and-to form.
fn chess_opponent_turn(
    game: ObjectId,
    team: i32,
    from: (u32, u32),
    to: (u32, u32),
) -> dereth_protocol::trade::GameOpponentTurn {
    dereth_protocol::trade::GameOpponentTurn {
        game_id: game.0,
        team,
        move_data: dereth_protocol::trade::GameMoveData {
            move_type: dereth_protocol::trade::GameMoveData::FROM_TO,
            player: ObjectId(0),
            from: Some(from),
            to: Some(to),
            piece_index: None,
        },
    }
}

// ---------------------------------------------------------------------------------------------

/// **The denominator, measured rather than looked up.** Each of the six the shard can send is
/// delivered to both consumers as a real event and the client is asked whether either took an arm,
/// because a decoder that works proves nothing about whether anything calls it; each of the five
/// the client can send is checked to have a sender and to be addressed at the shard.
///
/// **Do not shorten either list**: they are the denominator, and a census that drops rows as they
/// are fixed cannot tell *closed* from *forgotten*.
pub fn every_chess_message_reaches_a_receiver_or_a_sender() {
    use dereth_protocol::Opcode;

    const INBOUND: [Opcode; 6] = [
        Opcode::GAME_JOIN_GAME_RESPONSE,
        Opcode::GAME_START_GAME,
        Opcode::GAME_MOVE_RESPONSE,
        Opcode::GAME_OPPONENT_TURN,
        Opcode::GAME_OPPONENT_STALEMATE_STATE,
        Opcode::GAME_GAME_OVER,
    ];
    const OUTBOUND: [Opcode; 5] = [
        Opcode::GAME_JOIN,
        Opcode::GAME_QUIT,
        Opcode::GAME_MOVE,
        Opcode::GAME_MOVE_PASS,
        Opcode::GAME_STALEMATE,
    ];

    let mut received = Vec::new();
    let mut dropped = Vec::new();
    for op in INBOUND {
        dereth_client::dropped::clear();
        let mut blob = op.0.to_le_bytes().to_vec();
        blob.extend(std::iter::repeat_n(0_u8, 64));
        let events =
            [dereth_client_net::client_session::SessionEvent::UiEvent { opcode: op, blob }];

        let mut world = dereth_client_model::World::new();
        let mut hud = dereth_client::hud::Hud::new();
        hud.apply_events(&events, &mut world);

        let mut world = dereth_client_model::World::new();
        let mut inter = dereth_client::interaction::Interaction::new();
        dereth_client::interaction::apply_events(&mut inter, &events, &mut world);

        if dereth_client::dropped::unreceived(op) {
            dropped.push(op);
        } else {
            received.push(op);
        }
    }

    // The outbound five, discriminated by construction: each has a request of its own **and** a
    // sender arm, which is what "a sender exists" means here.
    let senders: Vec<(Opcode, dereth_client_model::Request)> = vec![
        (
            Opcode::GAME_JOIN,
            dereth_client_model::Request::GameJoin(Default::default()),
        ),
        (
            Opcode::GAME_QUIT,
            dereth_client_model::Request::GameQuit(dereth_protocol::trade::GameQuit),
        ),
        (
            Opcode::GAME_MOVE,
            dereth_client_model::Request::GameMove(Default::default()),
        ),
        (
            Opcode::GAME_MOVE_PASS,
            dereth_client_model::Request::GameMovePass(dereth_protocol::trade::GameMovePass),
        ),
        (
            Opcode::GAME_STALEMATE,
            dereth_client_model::Request::GameStalemate(Default::default()),
        ),
    ];
    let all_are_client_to_server = senders.iter().all(|(op, _)| {
        op.info().map(|i| i.direction) == Some(dereth_protocol::opcodes::Direction::C2S)
    });
    let sender_count = senders.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "minigame.messages.every-message-the-game-speaks-reaches-a-receiver-or-a-sender",
        move |_| {
            INBOUND.len() == 6
                && dropped.is_empty()
                && received.len() == 6
                && sender_count == 5
                && all_are_client_to_server
                && INBOUND.len() + OUTBOUND.len() == 11
        },
    );
    c.shutdown();
}

#[test]
fn scenario_every_chess_message_reaches_a_receiver_or_a_sender() {
    scenario("every_chess_message_reaches_a_receiver_or_a_sender");
}

/// **Using the board.** No round trip raises this window: the board is raised by
/// the use itself, and the first thing it does is ask for a seat.
pub fn using_a_board_raises_the_window_and_asks_for_a_seat() {
    let (mut c, _peer) = chess_a_client_and_two_boards();

    // Before: built, and down.
    let built = chess_panel(&c).bound()
        && chess_panel(&c).cells_built == minigame::CELLS
        && chess_panel(&c).resign.is_some()
        && chess_panel(&c).pass.is_some()
        && chess_panel(&c).stalemate.is_some()
        && !chess_panel(&c).visible;
    let nothing_yet = chess_model(&c).begin_games == 0
        && chess_model(&c).current_game == ObjectId(0)
        && chess_wire(&c).is_empty();

    chess_use_the_board(&mut c, CHESS_BOARD);

    // The use reached the game-board arm **and** the notice reached the window -- both, because
    // either alone is a state this client has been in.
    let reached_the_window = c.view().expect_app().interaction().stats.panels_requested == 1
        && c.view()
            .expect_app()
            .interaction()
            .stats
            .minigame_boards_used
            == 1
        && chess_model(&c).begin_games == 1;
    let the_window_is_up = chess_panel(&c).visible
        && chess_model(&c).current_game == CHESS_BOARD
        && chess_model(&c).state == GameState::AttemptingToJoinGame;
    let asked_for_a_seat = chess_wire(&c)
        == vec![dereth_client_model::Request::GameJoin(
            dereth_protocol::trade::GameJoin {
                game_id: CHESS_BOARD.0,
                which_team: dereth_client_model::minigame::JOIN_ANY_TEAM,
            },
        )];
    let said_so =
        chess_last_said(&mut c) == dereth_client_model::minigame::ATTEMPTING_TO_JOIN.trim();

    // The two refusals, and that neither of them asks the shard for anything.
    let mark = chess_wire(&c).len();
    chess_use_the_board(&mut c, CHESS_BOARD);
    let same_board = chess_last_spewed(&c) == dereth_client_model::minigame::ALREADY_THIS_GAME;
    // The client's own short throttle on using a thing is measured in its own clock, and six
    // headless frames do not always clear it: settling again is obeying the client's rule, not
    // making the scenario pass.
    c.tick(12);
    chess_use_the_board(&mut c, CHESS_OTHER_BOARD);
    let other_board = chess_last_spewed(&c) == dereth_client_model::minigame::ALREADY_ANOTHER_GAME;
    let both_refused = chess_model(&c).joins_refused == 2;
    let nothing_more_was_asked = chess_wire_since(&c, mark).is_empty();
    let the_first_game_is_untouched = chess_model(&c).current_game == CHESS_BOARD;

    c.assert_behaviour(
        "minigame.board.using-a-board-raises-the-window-and-asks-for-a-seat",
        move |_| {
            built
                && nothing_yet
                && reached_the_window
                && the_window_is_up
                && asked_for_a_seat
                && said_so
                && same_board
                && other_board
                && both_refused
                && nothing_more_was_asked
                && the_first_game_is_untouched
        },
    );
    c.shutdown();
}

#[test]
fn scenario_using_a_board_raises_the_window_and_asks_for_a_seat() {
    scenario("using_a_board_raises_the_window_and_asks_for_a_seat");
}

/// **The lamp has two notices and only two**: the one that begins a game and the one that ends it.
/// Everything in between changes the board window and must not select other lamp art.
pub fn the_game_lamp_is_lit_from_the_first_use_until_the_game_is_over() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    let dark_to_begin = chess_lamp_state(&c) == dereth_ui_screens::hud::indicators::STATE_NOTHING;

    chess_use_the_board(&mut c, CHESS_BOARD);
    let lit = chess_lamp_is_lit(&c);
    let mut stays_the_same = chess_model(&c).state == GameState::AttemptingToJoinGame;

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    stays_the_same &=
        chess_model(&c).state == GameState::WaitingForGameStart && chess_lamp_is_lit(&c) == lit;

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameStartGame {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    stays_the_same &=
        chess_model(&c).state == GameState::PlayingMyTurn && chess_lamp_is_lit(&c) == lit;

    let from = ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 1 }, 0).expect("a square");
    let to = ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 3 }, 0).expect("a square");
    chess_press_square(&mut c, from);
    chess_press_square(&mut c, to);
    stays_the_same &=
        chess_model(&c).state == GameState::PlayingTryingToMove && chess_lamp_is_lit(&c) == lit;

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameMoveResponse {
            game_id: CHESS_BOARD.0,
            result: 1,
        },
    );
    stays_the_same &=
        chess_model(&c).state == GameState::PlayingNotMyTurn && chess_lamp_is_lit(&c) == lit;

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &chess_opponent_turn(CHESS_BOARD, 1, (4, 6), (4, 4)),
    );
    stays_the_same &=
        chess_model(&c).state == GameState::PlayingMyTurn && chess_lamp_is_lit(&c) == lit;

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameGameOver {
            game_id: CHESS_BOARD.0,
            team_winner: dereth_client_model::minigame::GAME_OVER_ABORTED,
        },
    );
    let dark_again = chess_lamp_state(&c) == dereth_ui_screens::hud::indicators::STATE_NOTHING;
    let the_window_went_down = !chess_model(&c).visible;

    c.assert_behaviour(
        "minigame.indicator.the-lamp-is-lit-from-the-first-use-until-the-game-is-over",
        move |_| dark_to_begin && stays_the_same && dark_again && the_window_went_down,
    );
    c.shutdown();
}

#[test]
fn scenario_the_game_lamp_is_lit_from_the_first_use_until_the_game_is_over() {
    scenario("the_game_lamp_is_lit_from_the_first_use_until_the_game_is_over");
}

/// **The round trip.** The seat deals the board, the start names whose turn it is -- both arms --
/// and a refused seat puts the window back to no game at all.
pub fn the_seat_deals_the_board_and_the_start_names_whose_turn_it_is() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_use_the_board(&mut c, CHESS_BOARD);

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    let reached_an_arm = c.view().expect_app().hud().stats.minigame_events == 1
        && c.view().expect_app().hud().stats.minigame_guarded == 0;
    let dealt = chess_model(&c).team == 0
        && chess_model(&c).state == GameState::WaitingForGameStart
        && chess_model(&c).board.logic.pieces.len() == 32
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 4, y: 0 })
            .map(|p| p.piece_type)
            == Some(PieceType::King);
    let said_joined = chess_last_said(&mut c) == dereth_client_model::minigame::JOINED.trim();

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameStartGame {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    let my_turn = chess_model(&c).state == GameState::PlayingMyTurn
        && chess_last_said(&mut c) == dereth_client_model::minigame::BEGUN_YOUR_TURN.trim();
    c.shutdown();

    // The other arm of the same message: the shard says the *other* side moves first.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_use_the_board(&mut c, CHESS_BOARD);
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameStartGame {
            game_id: CHESS_BOARD.0,
            team: 1,
        },
    );
    let their_turn = chess_model(&c).state == GameState::PlayingNotMyTurn
        && chess_last_said(&mut c) == dereth_client_model::minigame::BEGUN_THEIR_TURN.trim();
    c.shutdown();

    // And a seat the shard refuses.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_use_the_board(&mut c, CHESS_BOARD);
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: -1,
        },
    );
    let refused = chess_last_said(&mut c) == dereth_client_model::minigame::COULD_NOT_JOIN.trim()
        && chess_model(&c).state == GameState::NotPlaying
        && chess_model(&c).current_game == ObjectId(0)
        && chess_model(&c).board.logic.pieces.is_empty();

    c.assert_behaviour(
        "minigame.board.the-shards-answer-deals-the-board-or-takes-the-window-away",
        move |_| reached_an_arm && dealt && said_joined && my_turn && their_turn && refused,
    );
    c.shutdown();
}

#[test]
fn scenario_the_seat_deals_the_board_and_the_start_names_whose_turn_it_is() {
    scenario("the_seat_deals_the_board_and_the_start_names_whose_turn_it_is");
}

/// **The move.** Two presses through the real tree, the request they make, the move the client's
/// own rules refuse where the player made it, and the move the shard refuses afterwards.
pub fn two_presses_move_a_piece_and_a_refused_move_never_leaves() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    let mark = chess_wire(&c).len();

    let from = ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 1 }, 0).expect("a square");
    let to = ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 3 }, 0).expect("a square");

    chess_press_square(&mut c, from);
    let picked_up = chess_panel(&c).presses_routed == 1
        && chess_model(&c).board.selected == Some(ChessCoord { x: 4, y: 1 })
        && chess_wire_since(&c, mark).is_empty();

    chess_press_square(&mut c, to);
    let asked = chess_wire_since(&c, mark)
        == vec![dereth_client_model::Request::GameMove(
            dereth_protocol::trade::GameMove {
                x_from: 4,
                y_from: 1,
                x_to: 4,
                y_to: 3,
            },
        )];
    let on_its_way = chess_model(&c).state == GameState::PlayingTryingToMove
        && chess_model(&c).moves_sent == 1
        && chess_last_said(&mut c) == dereth_client_model::minigame::MOVE_IN_PROGRESS;

    // Let the move stand and give the turn back, so the next press is the player's to make.
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameMoveResponse {
            game_id: CHESS_BOARD.0,
            result: 1,
        },
    );
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &chess_opponent_turn(CHESS_BOARD, 1, (4, 6), (4, 4)),
    );
    let our_turn_again = chess_model(&c).state == GameState::PlayingMyTurn;

    // A move the client's own rules refuse never reaches the shard -- the whole reason the rules
    // are in the client at all. The king cannot step on to its own pawn.
    let before = chess_model(&c).moves_sent;
    let mark = chess_wire(&c).len();
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 0 }, 0).expect("a square"),
    );
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 3, y: 1 }, 0).expect("a square"),
    );
    let refused_here = chess_model(&c).moves_sent == before
        && chess_model(&c).moves_refused_locally == 1
        && chess_wire_since(&c, mark).is_empty()
        && chess_last_said(&mut c)
            == format!(
                "You cannot attack your own pieces{}{}",
                dereth_client_model::minigame::TRY_AGAIN,
                dereth_client_model::minigame::YOUR_TURN
            )
            .trim();
    c.shutdown();

    // And the shard's own refusal, which puts the piece back and gives the turn back.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 1 }, 0).expect("a square"),
    );
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 3 }, 0).expect("a square"),
    );
    let waiting = chess_model(&c).state == GameState::PlayingTryingToMove;
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameMoveResponse {
            game_id: CHESS_BOARD.0,
            result: chess_mr::BAD_MOVE_NOT_YOUR_TURN,
        },
    );
    let undone = chess_model(&c).moves_undone == 1
        && chess_model(&c).state == GameState::PlayingMyTurn
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 4, y: 1 })
            .map(|p| p.piece_type)
            == Some(PieceType::Pawn)
        && chess_last_said(&mut c) == dereth_client_model::minigame::NOT_YOUR_TURN.trim();

    c.assert_behaviour(
        "minigame.board.two-presses-move-a-piece-and-a-move-the-rules-refuse-never-leaves",
        move |_| {
            picked_up && asked && on_its_way && our_turn_again && refused_here && waiting && undone
        },
    );
    c.shutdown();
}

#[test]
fn scenario_two_presses_move_a_piece_and_a_refused_move_never_leaves() {
    scenario("two_presses_move_a_piece_and_a_refused_move_never_leaves");
}

/// **The opponent's move, on the player's own board, drawn from the player's own side.**
pub fn the_opponents_move_is_replayed_on_the_players_own_board() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    // The second seat, with the first side moving first, so the start leaves the player waiting.
    chess_sit_down(&mut c, &mut peer, 1, 0);
    let mark = chess_wire(&c).len();
    let seated = chess_model(&c).team == 1 && chess_model(&c).state == GameState::PlayingNotMyTurn;

    // From the second side the board is drawn the other way round: the far corner piece is the
    // last square, and its picture is the one the board ships for it.
    let pictures = chess_piece_pictures(&c);
    let real_pixels = chess_pieces_have_pixels(&c, &pictures);
    let corner = ChessBoard::cell_of_coord(ChessCoord { x: 0, y: 7 }, 1).expect("a square");
    let drawn_from_our_side = corner == 63 && chess_square_draws(&c, corner, pictures[9]);

    let from = ChessBoard::cell_of_coord(ChessCoord { x: 3, y: 1 }, 1).expect("a square");
    let to = ChessBoard::cell_of_coord(ChessCoord { x: 3, y: 3 }, 1).expect("a square");
    let before = chess_square_draws(&c, from, pictures[0]) && chess_square_is_empty(&c, to);

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &chess_opponent_turn(CHESS_BOARD, 0, (3, 1), (3, 3)),
    );

    let moved = chess_model(&c).opponent_moves == 1
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 3, y: 3 })
            .map(|p| p.piece_type)
            == Some(PieceType::Pawn)
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 3, y: 1 })
            .is_none();
    let redrawn = chess_square_is_empty(&c, from) && chess_square_draws(&c, to, pictures[0]);
    let turn_came_back = chess_model(&c).state == GameState::PlayingMyTurn
        && chess_last_said(&mut c) == dereth_client_model::minigame::YOUR_TURN.trim();
    let sent_nothing = chess_wire_since(&c, mark).is_empty();

    c.assert_behaviour(
        "minigame.board.the-opponents-move-is-replayed-on-the-players-own-board",
        move |_| {
            seated
                && real_pixels
                && drawn_from_our_side
                && before
                && moved
                && redrawn
                && turn_came_back
                && sent_nothing
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_opponents_move_is_replayed_on_the_players_own_board() {
    scenario("the_opponents_move_is_replayed_on_the_players_own_board");
}

/// **The offer, and the end.** An offer of a draw and its withdrawal are both said in the window
/// and neither turns on the player's own offer -- agreeing is pressing your own button.
pub fn the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameOpponentStalemateState {
            game_id: CHESS_BOARD.0,
            team: 1,
            on: 1,
        },
    );
    let offered =
        chess_last_said(&mut c) == dereth_client_model::minigame::STALEMATE_OFFERED.trim();

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameOpponentStalemateState {
            game_id: CHESS_BOARD.0,
            team: 1,
            on: 0,
        },
    );
    let retracted =
        chess_last_said(&mut c) == dereth_client_model::minigame::STALEMATE_RETRACTED.trim();
    let both_counted = chess_model(&c).stalemate_offers == 2;
    let ours_is_untouched = !chess_model(&c).stalemate;

    // The player's side is the first, so the first side winning is the player winning.
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameGameOver {
            game_id: CHESS_BOARD.0,
            team_winner: 0,
        },
    );
    let won = chess_last_said(&mut c)
        == format!(
            "{}{}",
            dereth_client_model::minigame::VICTORIOUS,
            dereth_client_model::minigame::DEFAULT_STATE
        )
        .trim();
    let put_away = chess_model(&c).state == GameState::NotPlaying
        && chess_model(&c).team == -1
        && chess_model(&c).current_game == ObjectId(0);

    c.assert_behaviour(
        "minigame.window.the-offer-of-a-stalemate-and-the-end-of-a-game-are-said-in-the-window",
        move |_| offered && retracted && both_counted && ours_is_untouched && won && put_away,
    );
    c.shutdown();
}

#[test]
fn scenario_the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window() {
    scenario("the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window");
}

/// **The three buttons.** The draw button toggles and tells the shard the value it toggled *to*;
/// resigning asks first, through the client's own question, and only Yes quits; and the button
/// that passes a turn is never shown to the player at all.
pub fn the_window_buttons_offer_a_draw_and_ask_before_resigning() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    let mut mark = chess_wire(&c).len();

    chess_press_button(&mut c, minigame::STALEMATE_BUTTON);
    let offered = chess_panel(&c).button_clicks == 1
        && chess_model(&c).stalemate
        && chess_wire_since(&c, mark)
            == vec![dereth_client_model::Request::GameStalemate(
                dereth_protocol::trade::GameStalemate { on: 1 },
            )];
    mark = chess_wire(&c).len();
    chess_press_button(&mut c, minigame::STALEMATE_BUTTON);
    let withdrawn = !chess_model(&c).stalemate
        && chess_wire_since(&c, mark)
            == vec![dereth_client_model::Request::GameStalemate(
                dereth_protocol::trade::GameStalemate { on: 0 },
            )];

    // Resigning raises the question and sends nothing until it is answered.
    mark = chess_wire(&c).len();
    chess_press_button(&mut c, minigame::RESIGN_BUTTON);
    let asked_first = chess_wire_since(&c, mark).is_empty();
    let (first_context, first_root) = chess_resign_question(&c);
    let question_is_the_windows_own = u64::from(chess_model(&c).resign_dialog) == first_context
        && !chess_model(&c).resign_prompt_pending;
    let prompt = chess_dialog_text(&mut c, first_root, dereth_ui::dialog::base::child::TEXT);
    let yes = chess_dialog_text(&mut c, first_root, dereth_ui::dialog::base::child::BUTTON1);
    let no = chess_dialog_text(&mut c, first_root, dereth_ui::dialog::base::child::BUTTON2);
    let asked_in_its_own_words =
        prompt == dereth_client_model::minigame::RESIGN_PROMPT && yes == "Yes" && no == "No";

    // A second press while the question is up asks nothing more.
    chess_press_button(&mut c, minigame::RESIGN_BUTTON);
    let one_question_only =
        chess_resign_question(&c).0 == first_context && chess_wire_since(&c, mark).is_empty();

    // No closes it, sends nothing, and lets it be asked again.
    let mut hand = dereth_testkit::adapters_chat::Hand::new();
    chess_answer(
        &mut c,
        &mut hand,
        first_root,
        dereth_ui::dialog::base::child::BUTTON2,
    );
    let no_sends_nothing = chess_wire_since(&c, mark).is_empty()
        && chess_model(&c).current_game == CHESS_BOARD
        && chess_model(&c).resign_dialog == 0
        && !chess_model(&c).resign_prompt_pending
        && chess_no_question_is_up(&c);

    chess_press_button(&mut c, minigame::RESIGN_BUTTON);
    let (second_context, second_root) = chess_resign_question(&c);
    let a_fresh_question = second_context != first_context;
    chess_answer(
        &mut c,
        &mut hand,
        second_root,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    let yes_quits = chess_wire_since(&c, mark)
        == vec![dereth_client_model::Request::GameQuit(
            dereth_protocol::trade::GameQuit,
        )]
        && chess_model(&c).current_game == ObjectId(0)
        && !chess_panel(&c).visible;
    c.shutdown();

    // The pass button: never shown, and still a sender. With no game it says so and sends nothing;
    // with a game it sends the pass.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    let never_shown = {
        let h = chess_panel(&c)
            .pass
            .expect("the pass button is in the shipped tree");
        minigame::MiniGamePanel::PASS_BUTTON_IS_NEVER_SHOWN
            && !c
                .view()
                .expect_app()
                .ui()
                .expect("the ui shell is up")
                .ui
                .is_visible(h)
    };
    let mark = chess_wire(&c).len();
    c.when(Player::ui(
        dereth_ui_screens::view::UiRequest::MiniGameButton(minigame::PASS_BUTTON.0),
    ));
    c.tick(6);
    let no_game = chess_last_spewed(&c) == dereth_client_model::minigame::NOT_PLAYING
        && chess_wire_since(&c, mark).is_empty();

    chess_sit_down(&mut c, &mut peer, 0, 0);
    let mark = chess_wire(&c).len();
    c.when(Player::ui(
        dereth_ui_screens::view::UiRequest::MiniGameButton(minigame::PASS_BUTTON.0),
    ));
    c.tick(6);
    let passes = chess_wire_since(&c, mark)
        == vec![dereth_client_model::Request::GameMovePass(
            dereth_protocol::trade::GameMovePass,
        )];

    c.assert_behaviour(
        "minigame.window.the-buttons-offer-a-stalemate-and-ask-before-resigning",
        move |_| {
            offered
                && withdrawn
                && asked_first
                && question_is_the_windows_own
                && asked_in_its_own_words
                && one_question_only
                && no_sends_nothing
                && a_fresh_question
                && yes_quits
                && never_shown
                && no_game
                && passes
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_window_buttons_offer_a_draw_and_ask_before_resigning() {
    scenario("the_window_buttons_offer_a_draw_and_ask_before_resigning");
}

/// **The guard.** Every one of the six is guarded on which board it is about. A message about a
/// board this window did not sit down at must change nothing and say nothing -- and must still be
/// counted, so that "no traffic" and "all of it refused" are different answers.
pub fn a_message_about_another_board_changes_nothing() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    let before = chess_model(&c).state;
    let events_before = c.view().expect_app().hud().stats.minigame_events;
    let said_before = chess_chat_log(&mut c);
    let mark = chess_wire(&c).len();

    let other = CHESS_OTHER_BOARD.0;
    peer.event(
        &mut c,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: other,
            team: 1,
        },
    );
    peer.event(
        &mut c,
        &dereth_protocol::trade::GameStartGame {
            game_id: other,
            team: 1,
        },
    );
    peer.event(
        &mut c,
        &dereth_protocol::trade::GameMoveResponse {
            game_id: other,
            result: 1,
        },
    );
    peer.event(
        &mut c,
        &chess_opponent_turn(CHESS_OTHER_BOARD, 1, (4, 6), (4, 4)),
    );
    peer.event(
        &mut c,
        &dereth_protocol::trade::GameOpponentStalemateState {
            game_id: other,
            team: 1,
            on: 1,
        },
    );
    peer.event(
        &mut c,
        &dereth_protocol::trade::GameGameOver {
            game_id: other,
            team_winner: 1,
        },
    );
    c.tick(6);

    let all_six_arrived = c.view().expect_app().hud().stats.minigame_events - events_before == 6;
    let all_six_were_refused = c.view().expect_app().hud().stats.minigame_guarded == 6;
    let nothing_moved = chess_model(&c).state == before
        && chess_model(&c).current_game == CHESS_BOARD
        && chess_model(&c).team == 0;
    let nothing_was_said = chess_chat_log(&mut c) == said_before;
    let nothing_was_sent = chess_wire_since(&c, mark).is_empty();

    c.assert_behaviour(
        "minigame.board.a-message-about-another-board-changes-nothing",
        move |_| {
            all_six_arrived
                && all_six_were_refused
                && nothing_moved
                && nothing_was_said
                && nothing_was_sent
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_message_about_another_board_changes_nothing() {
    scenario("a_message_about_another_board_changes_nothing");
}
