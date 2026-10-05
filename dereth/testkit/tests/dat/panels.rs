//! The character, examination and dialog panels, read off the shipped element tree: the skills,
//! attributes and titles pages, the identify pane, the House pane, the notebook, inscriptions,
//! confirmation dialogs and the chess board. Fixture: the retail dats plus recorded characters and
//! answers replayed through `Inbound`, driven by a headless client.
//!
//! The declarations below pair each test wrapper with its behaviour claims and collect the
//! census entries in `ALL`. Implementations and fixtures live in subject modules.
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

dereth_testkit::scenarios! {
    scenario_the_skills_page_builds_a_header_per_group_and_a_row_per_recorded_skill => the_skills_page_builds_a_header_per_group_and_a_row_per_recorded_skill ["skills.list.a-header-per-group-and-a-row-per-skill-the-shard-sent"],
    scenario_an_empty_skills_page_is_distinguishable_from_a_full_one => an_empty_skills_page_is_distinguishable_from_a_full_one ["skills.list.a-page-that-built-nothing-is-told-apart-from-one-that-built-everything"],
    scenario_the_footer_title_is_the_name_the_number_and_the_signed_change => the_footer_title_is_the_name_the_number_and_the_signed_change ["skills.footer.the-title-is-the-name-and-the-number-with-the-change-spelled-out-after-it"],
    scenario_the_experience_to_raise_line_is_legible_and_uncovered => the_experience_to_raise_line_is_legible_and_uncovered ["skills.footer.the-experience-line-is-legible-and-nothing-is-painted-over-it"],
    scenario_a_buffed_attribute_row_is_green_a_debuffed_one_red_and_the_rest_white => a_buffed_attribute_row_is_green_a_debuffed_one_red_and_the_rest_white ["attributes.rows.a-buffed-value-is-green-a-debuffed-one-red-and-an-unmodified-one-white"],
    scenario_a_vital_row_is_current_over_maximum_and_colours_by_the_maximum => a_vital_row_is_current_over_maximum_and_colours_by_the_maximum ["attributes.rows.a-vital-row-is-current-over-maximum-and-takes-its-colour-from-the-maximum"],
    scenario_a_fractional_enchantment_rounds_in_the_rows_the_player_reads => a_fractional_enchantment_rounds_in_the_rows_the_player_reads ["attributes.rows.a-fractional-enchantment-rounds-the-way-the-panel-draws-it"],
    scenario_ten_points_cost_the_distance_between_two_entries_of_the_shipped_table => ten_points_cost_the_distance_between_two_entries_of_the_shipped_table ["advancement.cost.the-ten-point-cost-comes-off-the-shipped-experience-table"],
    scenario_the_plus_ten_button_lights_when_the_unassigned_experience_covers_it => the_plus_ten_button_lights_when_the_unassigned_experience_covers_it ["advancement.cost.the-plus-ten-button-is-affordable-when-the-unassigned-experience-covers-it"],
    scenario_an_assessed_item_names_its_spells_and_describes_them => an_assessed_item_names_its_spells_and_describes_them ["examine.spells.an-item-with-spells-on-it-names-them-and-describes-them"],
    scenario_an_enchantment_is_listed_apart_from_the_items_own_spells => an_enchantment_is_listed_apart_from_the_items_own_spells ["examine.spells.an-enchantment-is-listed-apart-from-the-spells-the-item-was-made-with"],
    scenario_the_magic_lines_come_in_order_and_a_rate_beats_a_flat_cost => the_magic_lines_come_in_order_and_a_rate_beats_a_flat_cost ["examine.spells.the-magic-lines-come-in-the-clients-own-order-and-a-rate-beats-a-flat-cost"],
    scenario_an_unsuccessful_assessment_says_unknown_and_a_plain_item_says_nothing => an_unsuccessful_assessment_says_unknown_and_a_plain_item_says_nothing ["examine.spells.an-unsuccessful-assessment-says-so-and-a-plain-item-says-nothing"],
    scenario_an_item_that_expires_says_when_and_needs_all_three_numbers => an_item_that_expires_says_when_and_needs_all_three_numbers ["examine.lifespan.an-item-that-expires-says-when-and-needs-all-three-of-its-numbers"],
    scenario_the_creature_pane_names_the_kind_the_level_and_the_nine_rows => the_creature_pane_names_the_kind_the_level_and_the_nine_rows ["examine.creature.the-pane-names-the-kind-the-level-six-attributes-and-three-vitals"],
    scenario_the_armour_pane_gives_its_level_and_eight_resistances => the_armour_pane_gives_its_level_and_eight_resistances ["examine.armour.the-pane-gives-its-level-and-eight-resistances-in-the-order-it-draws-them"],
    scenario_the_weapon_pane_gives_the_skill_the_speed_the_range_and_the_ammunition => the_weapon_pane_gives_the_skill_the_speed_the_range_and_the_ammunition ["examine.weapon.the-pane-gives-the-skill-the-damage-the-speed-the-range-and-the-ammunition"],
    scenario_an_item_on_someone_elses_hook_takes_its_slot_from_the_reply => an_item_on_someone_elses_hook_takes_its_slot_from_the_reply ["examine.weapon.an-item-on-someone-elses-hook-takes-its-slot-from-the-reply"],
    scenario_the_inscribe_box_appears_only_on_something_inscribable => the_inscribe_box_appears_only_on_something_inscribable ["examine.inscription.the-box-is-there-only-on-something-that-can-be-inscribed"],
    scenario_the_assess_key_shuts_an_open_pane_and_opens_a_shut_one => the_assess_key_shuts_an_open_pane_and_opens_a_shut_one ["examine.key.the-assess-key-shuts-an-open-pane-and-opens-a-shut-one"],
    scenario_the_assess_key_shuts_the_pane_with_nothing_under_the_pointer => the_assess_key_shuts_the_pane_with_nothing_under_the_pointer ["examine.key.the-pane-shuts-with-nothing-under-the-pointer-and-a-shut-one-does-nothing"],
    scenario_a_potion_says_what_it_restores_and_a_kit_what_it_adds => a_potion_says_what_it_restores_and_a_kit_what_it_adds ["examine.consumables.a-potion-says-what-it-restores-and-a-kit-what-it-adds"],
    scenario_a_container_says_how_much_it_holds_and_a_book_how_many_pages_are_used => a_container_says_how_much_it_holds_and_a_book_how_many_pages_are_used ["examine.capacity.a-container-says-how-much-it-holds-and-a-book-how-many-pages-are-used"],
    scenario_every_line_the_item_pane_can_draw_reaches_it_through_the_shard => every_line_the_item_pane_can_draw_reaches_it_through_the_shard ["examine.item-blocks.every-line-the-item-pane-can-draw-reaches-it-through-the-shard"],
    scenario_the_character_pane_draws_its_rows_and_has_nothing_left_undrawn => the_character_pane_draws_its_rows_and_has_nothing_left_undrawn ["examine.character.the-pane-draws-its-rows-and-has-nothing-left-undrawn"],
    scenario_a_spell_cast_while_playing_moves_the_skill_row_it_is_about => a_spell_cast_while_playing_moves_the_skill_row_it_is_about ["skills.rows.a-spell-cast-while-playing-moves-the-skill-row-it-is-about"],
    scenario_a_character_who_logs_in_already_enchanted_reads_the_same_total => a_character_who_logs_in_already_enchanted_reads_the_same_total ["skills.rows.a-character-who-logs-in-already-enchanted-reads-the-same-total"],
    scenario_the_shards_house_message_fills_every_row_of_the_tab => the_shards_house_message_fills_every_row_of_the_tab ["house.data.the-shards-house-message-fills-every-row-of-the-tab"],
    scenario_the_purchase_time_line_reads_what_retail_prints_in_each_arm => the_purchase_time_line_reads_what_retail_prints_in_each_arm ["house.purchase-time.the-line-reads-what-retail-prints-in-each-of-its-three-arms"],
    scenario_a_paid_up_apartment_drops_the_location_row_and_doubles_the_period => a_paid_up_apartment_drops_the_location_row_and_doubles_the_period ["house.data.a-paid-up-apartment-drops-the-location-row-and-doubles-the-period"],
    scenario_a_zero_instant_prints_the_sentinel_and_not_the_epoch => a_zero_instant_prints_the_sentinel_and_not_the_epoch ["house.data.a-zero-instant-prints-the-sentinel-and-not-the-start-of-the-epoch"],
    scenario_a_house_row_is_as_tall_as_the_text_it_holds => a_house_row_is_as_tall_as_the_text_it_holds ["house.rows.a-row-is-as-tall-as-the-text-it-holds-and-fits-inside-the-pane"],
    scenario_the_paid_maintenance_line_is_drawn_in_the_templates_green => the_paid_maintenance_line_is_drawn_in_the_templates_green ["house.rows.the-paid-maintenance-line-is-drawn-in-the-colour-the-template-ships"],
    scenario_the_house_price_rows_are_comma_joined_and_grouped => the_house_price_rows_are_comma_joined_and_grouped ["house.prices.the-price-rows-are-comma-joined-and-their-counts-are-grouped"],
    scenario_the_client_asks_about_the_house_once_when_it_learns_who_it_is => the_client_asks_about_the_house_once_when_it_learns_who_it_is ["house.query.the-client-asks-about-the-house-once-when-it-learns-who-it-is"],
    scenario_a_new_maintenance_period_clears_every_payment => a_new_maintenance_period_clears_every_payment ["house.rent.a-new-period-moves-both-dates-and-marks-every-payment-unpaid"],
    scenario_a_payment_update_replaces_the_list_rather_than_merging => a_payment_update_replaces_the_list_rather_than_merging ["house.rent.a-payment-update-replaces-the-list-rather-than-merging-into-it"],
    scenario_neither_maintenance_update_touches_a_houseless_character => neither_maintenance_update_touches_a_houseless_character ["house.rent.neither-update-touches-a-character-with-no-house"],
    scenario_a_restriction_update_reaches_the_object_it_names => a_restriction_update_reaches_the_object_it_names ["house.restrictions.an-update-reaches-the-object-it-names-and-never-the-player"],
    scenario_a_transaction_answer_clears_the_pane_as_a_status_does => a_transaction_answer_clears_the_pane_as_a_status_does ["house.data.a-transaction-answer-clears-the-pane-exactly-as-a-status-answer-does"],
    scenario_two_confirmations_send_the_abandon_and_only_the_shard_empties_the_pane => two_confirmations_send_the_abandon_and_only_the_shard_empties_the_pane ["house.abandon.two-confirmations-send-it-and-only-the-shards-answer-empties-the-pane"],
    /// Behaviour: dialog.confirmation.the-same-command-twice-asks-twice-one-question-after-the-other
    scenario_the_same_command_twice_asks_twice_one_question_after_the_other => the_same_command_twice_asks_twice_one_question_after_the_other ["dialog.confirmation.the-same-command-twice-asks-twice-one-question-after-the-other"],
    scenario_the_augmentation_cost_line_comes_off_the_shipped_sentence => the_augmentation_cost_line_comes_off_the_shipped_sentence ["examine.augmentation.the-cost-line-is-the-shipped-sentence-with-the-number-in-it"],
    scenario_the_cooldown_lines_reach_the_item_pane => the_cooldown_lines_reach_the_item_pane ["examine.cooldown.the-pane-says-how-long-it-is-and-how-long-is-left-of-the-players-own"],
    scenario_the_long_description_is_decorated_where_the_player_reads_it => the_long_description_is_decorated_where_the_player_reads_it ["examine.description.the-long-one-is-decorated-and-a-plating-name-replaces-it"],
    scenario_the_short_description_is_used_only_when_there_is_no_long_one => the_short_description_is_used_only_when_there_is_no_long_one ["examine.description.the-short-one-is-drawn-only-when-there-is-no-long-one-at-all"],
    scenario_each_portal_restriction_draws_its_own_sentence_and_only_its_own => each_portal_restriction_draws_its_own_sentence_and_only_its_own ["examine.portal.each-restriction-draws-its-own-sentence-and-only-its-own"],
    scenario_a_portal_with_no_restrictions_still_gets_the_blocks_separators => a_portal_with_no_restrictions_still_gets_the_blocks_separators ["examine.portal.a-portal-with-no-restriction-still-gains-the-blocks-two-separators"],
    scenario_a_failed_assessment_draws_every_value_cell_in_the_unknown_colour => a_failed_assessment_draws_every_value_cell_in_the_unknown_colour ["examine.failed-assess.every-value-cell-takes-the-unknown-colour-and-no-label-does"],
    scenario_a_failed_assessment_keeps_the_numbers_and_reduces_health_to_a_percentage => a_failed_assessment_keeps_the_numbers_and_reduces_health_to_a_percentage ["examine.failed-assess.the-numbers-the-shard-described-are-kept-and-health-is-a-percentage"],
    scenario_the_failure_colour_outranks_the_enchantment_colour => the_failure_colour_outranks_the_enchantment_colour ["examine.failed-assess.an-unassessed-stat-reads-unknown-even-when-it-is-enchanted"],
    scenario_a_failed_assessment_leaves_the_item_panes_value_line_plain => a_failed_assessment_leaves_the_item_panes_value_line_plain ["examine.failed-assess.the-item-panes-own-unknown-value-line-is-drawn-plain"],
    scenario_the_description_pane_shows_a_bar_only_when_its_text_does_not_fit => the_description_pane_shows_a_bar_only_when_its_text_does_not_fit ["examine.scroll.the-description-pane-shows-a-bar-only-when-its-text-does-not-fit"],
    scenario_the_bar_really_scrolls_the_description => the_bar_really_scrolls_the_description ["examine.scroll.the-bar-brings-the-end-of-a-long-description-into-view"],
    scenario_a_raised_line_is_green_a_lowered_one_red_and_the_rest_plain => a_raised_line_is_green_a_lowered_one_red_and_the_rest_plain ["examine.enchanted.a-raised-line-is-green-a-lowered-one-red-and-everything-else-plain"],
    scenario_an_answer_the_pane_did_not_ask_for_opens_nothing => an_answer_the_pane_did_not_ask_for_opens_nothing ["examine.window.an-answer-the-pane-did-not-ask-for-opens-nothing-and-a-repeat-does-not-reopen-it"],
    scenario_the_identify_button_is_what_starts_the_chain => the_identify_button_is_what_starts_the_chain ["examine.window.the-identify-button-is-what-makes-the-pane-wait-for-an-answer"],
    scenario_the_character_pane_rows_are_the_shards_own_numbers => the_character_pane_rows_are_the_shards_own_numbers ["examine.character.the-armour-rows-are-the-shards-own-numbers-and-the-three-lines-beside-them"],
    scenario_the_society_row_names_the_society_and_its_rank_band => the_society_row_names_the_society_and_its_rank_band ["examine.character.the-society-row-names-the-society-and-the-band-its-rank-falls-in"],
    scenario_the_society_row_is_coloured_by_the_viewers_own_society => the_society_row_is_coloured_by_the_viewers_own_society ["examine.character.the-society-row-is-coloured-by-the-viewers-own-society"],
    scenario_the_allegiance_rows_fork_on_the_two_titles => the_allegiance_rows_fork_on_the_two_titles ["examine.character.the-allegiance-rows-fork-on-whether-the-monarch-is-also-the-patron"],
    scenario_an_allegiance_rank_puts_its_title_in_front_of_the_name => an_allegiance_rank_puts_its_title_in_front_of_the_name ["examine.character.an-allegiance-rank-puts-its-title-in-front-of-the-name"],
    scenario_an_unenchantable_body_part_is_marked_with_a_star => an_unenchantable_body_part_is_marked_with_a_star ["examine.character.a-body-part-nothing-can-be-cast-on-is-marked-and-reads-what-is-left"],
    scenario_the_rating_groups_draw_the_pairs_the_pane_prints => the_rating_groups_draw_the_pairs_the_pane_prints ["examine.character.the-rating-groups-draw-the-pairs-the-pane-prints-and-no-others"],
    scenario_the_creature_pane_draws_the_same_rating_rows_without_the_footnote => the_creature_pane_draws_the_same_rating_rows_without_the_footnote ["examine.creature.the-creature-pane-draws-the-same-rating-rows-without-the-footnote"],
    scenario_the_tail_rows_are_the_seven_the_pane_lists => the_tail_rows_are_the_seven_the_pane_lists ["examine.character.the-seven-tail-rows-each-draw-the-thing-behind-them"],
    scenario_every_character_row_draws_in_the_panes_own_order => every_character_row_draws_in_the_panes_own_order ["examine.character.every-row-draws-in-the-panes-own-order-and-reaches-the-list"],
    scenario_an_npc_question_appears_and_both_answers_reach_the_shard => an_npc_question_appears_and_both_answers_reach_the_shard ["dialog.confirmation.a-question-appears-and-both-answers-reach-the-shard"],
    scenario_a_making_question_gets_the_clients_own_continue => a_making_question_gets_the_clients_own_continue ["dialog.confirmation.a-question-about-making-something-adds-the-clients-own-continue"],
    scenario_the_shards_withdrawal_closes_the_question_and_refuses_on_the_way_out => the_shards_withdrawal_closes_the_question_and_refuses_on_the_way_out ["dialog.confirmation.the-shards-withdrawal-closes-it-and-refuses-on-the-way-out"],
    scenario_a_second_question_while_one_is_open_takes_over_its_handle => a_second_question_while_one_is_open_takes_over_its_handle ["dialog.confirmation.a-second-question-while-one-is-open-takes-over-its-handle-and-is-not-queued"],
    scenario_an_invitation_and_a_swearing_raise_their_own_panels_questions => an_invitation_and_a_swearing_raise_their_own_panels_questions ["dialog.confirmation.an-invitation-and-a-swearing-ask-their-own-panels-question"],
    scenario_the_invitation_is_centred_and_a_real_inscription_is_not => the_invitation_is_centred_and_a_real_inscription_is_not ["examine.inscription.the-invitation-is-centred-and-a-real-inscription-is-not"],
    scenario_a_press_empties_the_box_and_shows_whose_signature_it_will_carry => a_press_empties_the_box_and_shows_whose_signature_it_will_carry ["examine.inscription.a-press-empties-the-box-and-shows-whose-signature-it-will-carry"],
    scenario_letting_the_caret_go_sends_what_was_written => letting_the_caret_go_sends_what_was_written ["examine.inscription.letting-the-caret-go-sends-what-was-written-and-it-comes-back"],
    scenario_leaving_an_unchanged_box_sends_nothing => leaving_an_unchanged_box_sends_nothing ["examine.inscription.leaving-an-unchanged-box-sends-nothing-and-the-invitation-comes-back"],
    scenario_escape_is_the_confirm_edge_and_return_is_not => escape_is_the_confirm_edge_and_return_is_not ["examine.inscription.escape-is-the-edge-that-confirms-it-and-return-is-not"],
    scenario_a_box_somebody_else_signed_is_not_yours_to_change => a_box_somebody_else_signed_is_not_yours_to_change ["examine.inscription.a-box-somebody-else-signed-is-not-yours-to-change"],
    scenario_a_signed_thing_with_no_words_shows_a_blank_box => a_signed_thing_with_no_words_shows_a_blank_box ["examine.inscription.a-signed-thing-with-no-words-shows-a-blank-box-that-is-the-scribes"],
    scenario_the_scribe_can_rub_out_his_own_words => the_scribe_can_rub_out_his_own_words ["examine.inscription.the-writer-can-rub-out-his-own-words-and-the-invitation-comes-back"],
    scenario_the_close_button_commits_on_its_way_out => the_close_button_commits_on_its_way_out ["examine.inscription.the-windows-close-button-commits-what-was-typed-on-its-way-out"],
    scenario_changing_the_target_to_one_you_may_not_write_on_drops_the_caret => changing_the_target_to_one_you_may_not_write_on_drops_the_caret ["examine.inscription.looking-at-one-you-may-not-write-on-takes-the-caret-away"],
    scenario_changing_to_another_one_you_may_write_on_keeps_the_caret => changing_to_another_one_you_may_write_on_keeps_the_caret ["examine.inscription.looking-at-another-you-may-write-on-keeps-the-caret-where-it-was"],
    scenario_the_journal_opens_on_page_one_with_the_timer_editable => the_journal_opens_on_page_one_with_the_timer_editable ["journal.page.the-tab-opens-on-page-one-with-the-timer-ready-to-set"],
    scenario_a_typed_title_survives_a_page_turn => a_typed_title_survives_a_page_turn ["journal.page.a-title-typed-on-one-page-is-there-on-the-way-back"],
    scenario_the_journal_timer_counts_down_and_the_button_resets_it => the_journal_timer_counts_down_and_the_button_resets_it ["journal.timer.it-counts-down-on-its-own-and-the-button-puts-it-back"],
    scenario_the_page_list_lists_the_journals_pages => the_page_list_lists_the_journals_pages ["journal.list.the-list-is-the-journals-own-pages-in-page-order"],
    scenario_only_the_search_control_filters_the_page_list => only_the_search_control_filters_the_page_list ["journal.list.typing-alone-does-not-filter-and-the-search-control-does"],
    scenario_reset_clears_the_search_and_deletes_nothing => reset_clears_the_search_and_deletes_nothing ["journal.list.the-reset-control-clears-the-search-and-deletes-no-page"],
    scenario_pressing_a_row_then_delete_removes_that_rows_page => pressing_a_row_then_delete_removes_that_rows_page ["journal.list.a-pressed-row-is-the-one-the-delete-control-removes"],
    scenario_a_double_press_on_a_row_opens_that_page => a_double_press_on_a_row_opens_that_page ["journal.list.a-double-press-on-a-row-opens-that-page-in-the-journal"],
    scenario_a_journal_page_survives_a_restart => a_journal_page_survives_a_restart ["journal.file.a-page-written-here-is-on-disk-in-the-shipped-format-and-comes-back"],
    scenario_a_client_that_does_not_know_its_character_writes_nothing => a_client_that_does_not_know_its_character_writes_nothing ["journal.file.a-client-that-does-not-know-its-character-writes-no-notebook"],
    scenario_a_notebook_in_the_shipped_format_is_read_and_written_back => a_notebook_in_the_shipped_format_is_read_and_written_back ["journal.file.a-notebook-in-the-shipped-format-is-read-and-written-back-in-place"],
    scenario_a_second_character_gets_its_own_notebook => a_second_character_gets_its_own_notebook ["journal.file.a-second-character-gets-a-notebook-of-their-own"],
    scenario_a_relog_keeps_each_characters_notebook => a_relog_keeps_each_characters_notebook ["journal.file.a-relog-keeps-each-characters-notebook-and-saves-the-page-still-open"],
    scenario_the_titles_tab_draws_every_earned_title_sorted_by_name => the_titles_tab_draws_every_earned_title_sorted_by_name ["titles.tab.the-tab-draws-every-earned-title-sorted-by-name"],
    scenario_picking_a_title_arms_the_button_and_sends_nothing => picking_a_title_arms_the_button_and_sends_nothing ["titles.tab.picking-a-row-arms-the-button-only-for-a-title-not-already-worn"],
    scenario_the_display_button_puts_the_set_title_request_on_the_wire => the_display_button_puts_the_set_title_request_on_the_wire ["titles.tab.the-button-sends-one-request-for-the-picked-title-and-nothing-local-moves"],
    scenario_the_ten_point_button_lights_up_and_a_real_press_sends_the_raise => the_ten_point_button_lights_up_and_a_real_press_sends_the_raise ["advancement.raise.the-ten-point-button-lights-up-with-the-experience-and-a-press-sends-the-raise"],
    scenario_every_house_sub_command_that_sends_puts_its_own_message_on_the_wire => every_house_sub_command_that_sends_puts_its_own_message_on_the_wire ["house.commands.every-sub-command-that-sends-puts-its-own-message-on-the-wire"],
    scenario_a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing => a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing ["house.commands.a-line-the-ladder-refuses-prints-the-shipped-sentence-and-sends-nothing"],
    scenario_the_help_listing_and_the_house_help_are_what_the_client_ships => the_help_listing_and_the_house_help_are_what_the_client_ships ["house.commands.the-help-listing-and-the-house-help-are-what-the-client-ships"],
    scenario_the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log => the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log ["house.guests.the-guest-list-the-shard-sends-back-is-written-out-on-the-chat-log"],
    scenario_the_free_houses_the_shard_lists_are_written_out_with_their_locations => the_free_houses_the_shard_lists_are_written_out_with_their_locations ["house.available.the-free-houses-the-shard-lists-are-written-out-with-their-locations"],
    scenario_a_press_picks_the_attribute_row_under_it_and_lands_on_the_list => a_press_picks_the_attribute_row_under_it_and_lands_on_the_list ["attributes.selection.a-press-picks-the-row-under-the-pointer-through-the-list-itself"],
    scenario_a_press_picks_the_skill_row_under_it_and_the_attribute_page_stays_put => a_press_picks_the_skill_row_under_it_and_the_attribute_page_stays_put ["skills.selection.a-press-picks-the-skill-row-under-it-and-the-other-page-stays-put"],
    scenario_a_raise_sends_nothing_until_a_skill_row_is_picked => a_raise_sends_nothing_until_a_skill_row_is_picked ["advancement.raise.a-raise-sends-nothing-until-a-row-is-picked-and-then-names-it-and-the-cost"],
    scenario_every_skill_row_draws_its_value_in_a_font_that_exists => every_skill_row_draws_its_value_in_a_font_that_exists ["skills.rows.every-row-draws-its-value-in-a-font-that-exists-and-the-colour-it-declares"],
    scenario_every_attribute_row_draws_the_icon_its_own_group_names => every_attribute_row_draws_the_icon_its_own_group_names ["attributes.rows.every-row-draws-the-icon-its-own-group-names"],
    scenario_the_character_pages_numbers_carry_the_shipped_separator => the_character_pages_numbers_carry_the_shipped_separator ["skills.numbers.the-experience-numbers-are-grouped-with-the-shipped-separator"],
    scenario_the_shards_abuse_answer_writes_the_result_line_and_nothing_in_chat => the_shards_abuse_answer_writes_the_result_line_and_nothing_in_chat ["abuse.response.the-shards-answer-writes-the-result-line-and-nothing-in-chat"],
    scenario_the_abuse_page_sends_one_report_and_then_empties_itself => the_abuse_page_sends_one_report_and_then_empties_itself ["abuse.report.the-page-sends-one-report-naming-who-and-why-and-then-empties-itself"],
    scenario_the_skills_list_is_one_column_of_equal_rows_mostly_out_of_sight => the_skills_list_is_one_column_of_equal_rows_mostly_out_of_sight ["skills.scroll.the-list-is-one-column-of-equal-rows-and-most-of-it-is-out-of-sight"],
    scenario_the_bar_beside_the_skills_list_reports_the_view_and_where_it_is => the_bar_beside_the_skills_list_reports_the_view_and_where_it_is ["skills.scroll.the-bar-beside-the-list-says-how-much-is-in-view-and-where-it-is"],
    scenario_each_arrow_moves_the_skills_list_one_row_its_own_way => each_arrow_moves_the_skills_list_one_row_its_own_way ["skills.scroll.each-arrow-moves-the-list-one-row-its-own-way-and-the-top-is-the-top"],
    scenario_a_press_on_a_scrolled_skills_list_picks_the_row_it_is_drawn_over => a_press_on_a_scrolled_skills_list_picks_the_row_it_is_drawn_over ["skills.scroll.a-press-on-a-scrolled-list-picks-the-row-it-is-drawn-over"],
    scenario_rebuilding_the_skills_list_keeps_where_it_was_scrolled_to => rebuilding_the_skills_list_keeps_where_it_was_scrolled_to ["skills.scroll.rebuilding-the-list-keeps-where-it-was-scrolled-to"],
    scenario_a_skill_row_shows_the_total_after_everything_in_the_colour_of_the_change => a_skill_row_shows_the_total_after_everything_in_the_colour_of_the_change ["skills.rows.the-number-shown-is-the-total-after-everything-and-the-colour-says-which-way"],
    scenario_a_weakened_character_shows_the_lower_number_still_drawn_plain => a_weakened_character_shows_the_lower_number_still_drawn_plain ["skills.rows.a-weakened-character-shows-the-lower-number-without-calling-it-lowered"],
    scenario_the_contracts_tab_draws_a_row_per_contract_in_name_order => the_contracts_tab_draws_a_row_per_contract_in_name_order ["contracts.tab.the-tab-draws-a-row-per-contract-the-shard-sent-in-name-order"],
    scenario_picking_a_contract_fills_the_pane_beside_the_list => picking_a_contract_fills_the_pane_beside_the_list ["contracts.detail.picking-a-contract-fills-the-pane-beside-the-list"],
    scenario_the_abandon_button_gives_up_the_picked_contract_and_nothing_local_moves => the_abandon_button_gives_up_the_picked_contract_and_nothing_local_moves ["contracts.abandon.the-button-gives-up-the-picked-contract-and-nothing-local-moves"],
    scenario_one_contract_at_a_time_is_added_changed_or_taken_away => one_contract_at_a_time_is_added_changed_or_taken_away ["contracts.list.one-contract-at-a-time-is-added-changed-or-taken-away"],
    scenario_the_sort_buttons_choose_the_order_and_a_second_press_turns_it_round => the_sort_buttons_choose_the_order_and_a_second_press_turns_it_round ["contracts.sort.the-two-buttons-choose-the-order-and-pressing-one-twice-turns-it-round"],
    scenario_the_born_line_is_the_day_and_time_the_character_was_made => the_born_line_is_the_day_and_time_the_character_was_made ["character-sheet.born.the-line-is-the-day-and-time-the-character-was-made"],
    scenario_the_playtime_line_spells_out_only_the_terms_that_are_not_zero => the_playtime_line_spells_out_only_the_terms_that_are_not_zero ["character-sheet.played.the-line-spells-out-only-the-terms-that-are-not-zero"],
    scenario_the_mastery_lines_name_the_weapon_group_rather_than_the_number => the_mastery_lines_name_the_weapon_group_rather_than_the_number ["character-sheet.mastery.the-lines-name-the-weapon-group-rather-than-the-number-that-picks-it"],
    scenario_the_luminance_section_is_a_header_a_split_pair_and_an_unclamped_single => the_luminance_section_is_a_header_a_split_pair_and_an_unclamped_single ["character-sheet.luminance.the-heading-is-always-there-and-a-rating-over-five-splits-in-two"],
    scenario_an_augmentation_row_says_time_once_and_times_more_than_once => an_augmentation_row_says_time_once_and_times_more_than_once ["character-sheet.augmentations.a-row-says-time-once-and-times-more-than-once"],
    scenario_the_six_sections_of_the_sheet_are_in_the_order_the_composer_appends_them => the_six_sections_of_the_sheet_are_in_the_order_the_composer_appends_them ["character-sheet.sections.the-six-parts-are-drawn-in-the-order-the-sheet-builds-them"],
    scenario_a_removed_death_count_disappears_from_the_character_sheet => a_removed_death_count_disappears_from_the_character_sheet ["character-sheet.deaths.a-count-the-shard-clears-leaves-the-sheet"],
    scenario_the_skills_page_opens_asking_for_a_pick_and_a_press_picks_one => the_skills_page_opens_asking_for_a_pick_and_a_press_picks_one ["skills.selection.a-press-picks-a-skill-and-a-second-press-puts-the-footer-back"],
    scenario_the_footer_under_a_picked_skill_is_that_characters_own_arithmetic => the_footer_under_a_picked_skill_is_that_characters_own_arithmetic ["skills.footer.the-numbers-under-a-picked-skill-are-the-characters-own"],
    scenario_the_raise_button_spends_experience_or_credits_by_what_the_skill_is => the_raise_button_spends_experience_or_credits_by_what_the_skill_is ["skills.raise.the-button-spends-experience-on-a-trained-skill-and-credits-on-an-untrained-one"],
    scenario_a_skill_values_colour_is_one_of_the_rows_own_three => a_skill_values_colour_is_one_of_the_rows_own_three ["skills.rows.the-colour-a-value-is-drawn-in-is-one-of-the-rows-own-three"],
    scenario_both_character_pages_head_with_the_name_the_level_and_the_experience => both_character_pages_head_with_the_name_the_level_and_the_experience ["character.header.the-character-page-heads-with-the-name-the-level-and-the-experience"],
    scenario_turning_a_school_off_hides_its_spells_and_sends_the_whole_list => turning_a_school_off_hides_its_spells_and_sends_the_whole_list ["spellbook.filter.turning-a-school-off-hides-its-spells-and-sends-the-whole-list"],
    scenario_every_chess_message_reaches_a_receiver_or_a_sender => every_chess_message_reaches_a_receiver_or_a_sender ["minigame.messages.every-message-the-game-speaks-reaches-a-receiver-or-a-sender"],
    scenario_using_a_board_raises_the_window_and_asks_for_a_seat => using_a_board_raises_the_window_and_asks_for_a_seat ["minigame.board.using-a-board-raises-the-window-and-asks-for-a-seat"],
    scenario_the_game_lamp_is_lit_from_the_first_use_until_the_game_is_over => the_game_lamp_is_lit_from_the_first_use_until_the_game_is_over ["minigame.indicator.the-lamp-is-lit-from-the-first-use-until-the-game-is-over"],
    scenario_the_seat_deals_the_board_and_the_start_names_whose_turn_it_is => the_seat_deals_the_board_and_the_start_names_whose_turn_it_is ["minigame.board.the-shards-answer-deals-the-board-or-takes-the-window-away"],
    scenario_two_presses_move_a_piece_and_a_refused_move_never_leaves => two_presses_move_a_piece_and_a_refused_move_never_leaves ["minigame.board.two-presses-move-a-piece-and-a-move-the-rules-refuse-never-leaves"],
    scenario_the_opponents_move_is_replayed_on_the_players_own_board => the_opponents_move_is_replayed_on_the_players_own_board ["minigame.board.the-opponents-move-is-replayed-on-the-players-own-board"],
    scenario_the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window => the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window ["minigame.window.the-offer-of-a-stalemate-and-the-end-of-a-game-are-said-in-the-window"],
    scenario_the_window_buttons_offer_a_draw_and_ask_before_resigning => the_window_buttons_offer_a_draw_and_ask_before_resigning ["minigame.window.the-buttons-offer-a-stalemate-and-ask-before-resigning"],
    scenario_a_message_about_another_board_changes_nothing => a_message_about_another_board_changes_nothing ["minigame.board.a-message-about-another-board-changes-nothing"],
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

use dereth_client_model::minigame::GameBoard as ChessBoard;
use dereth_ui_screens::panels::minigame;
use {
    dereth_rules::chess::move_result as chess_mr, dereth_rules::chess::Coord as ChessCoord,
    dereth_rules::chess::GameState, dereth_rules::chess::PieceType,
};

mod abuse_reports;
use abuse_reports::{
    the_abuse_page_sends_one_report_and_then_empties_itself,
    the_shards_abuse_answer_writes_the_result_line_and_nothing_in_chat,
};

mod appraisal_character;
use appraisal_character::{
    an_allegiance_rank_puts_its_title_in_front_of_the_name,
    an_unenchantable_body_part_is_marked_with_a_star,
    every_character_row_draws_in_the_panes_own_order, the_allegiance_rows_fork_on_the_two_titles,
    the_character_pane_rows_are_the_shards_own_numbers,
    the_creature_pane_draws_the_same_rating_rows_without_the_footnote,
    the_rating_groups_draw_the_pairs_the_pane_prints,
    the_society_row_is_coloured_by_the_viewers_own_society,
    the_society_row_names_the_society_and_its_rank_band,
    the_tail_rows_are_the_seven_the_pane_lists,
};

mod appraisal_text;
use appraisal_text::{
    a_failed_assessment_draws_every_value_cell_in_the_unknown_colour,
    a_failed_assessment_keeps_the_numbers_and_reduces_health_to_a_percentage,
    a_failed_assessment_leaves_the_item_panes_value_line_plain,
    a_portal_with_no_restrictions_still_gets_the_blocks_separators,
    a_raised_line_is_green_a_lowered_one_red_and_the_rest_plain,
    an_answer_the_pane_did_not_ask_for_opens_nothing, await_the_answer_for, deliver_the_answer,
    each_portal_restriction_draws_its_own_sentence_and_only_its_own,
    the_augmentation_cost_line_comes_off_the_shipped_sentence,
    the_bar_really_scrolls_the_description, the_cooldown_lines_reach_the_item_pane,
    the_description_pane_shows_a_bar_only_when_its_text_does_not_fit,
    the_failure_colour_outranks_the_enchantment_colour,
    the_identify_button_is_what_starts_the_chain,
    the_long_description_is_decorated_where_the_player_reads_it,
    the_short_description_is_used_only_when_there_is_no_long_one, with_ints, APPRAISED_PLAYER,
};

mod appraisal_values;
use appraisal_values::{
    a_client_and_a_shard, a_container_says_how_much_it_holds_and_a_book_how_many_pages_are_used,
    a_potion_says_what_it_restores_and_a_kit_what_it_adds,
    an_assessed_item_names_its_spells_and_describes_them,
    an_enchantment_is_listed_apart_from_the_items_own_spells,
    an_item_on_someone_elses_hook_takes_its_slot_from_the_reply,
    an_item_that_expires_says_when_and_needs_all_three_numbers,
    an_unsuccessful_assessment_says_unknown_and_a_plain_item_says_nothing, assess,
    assessing_the_recorded, creature_rows, description,
    every_line_the_item_pane_can_draw_reaches_it_through_the_shard, examine, item_text,
    recorded_object, the_armour_pane_gives_its_level_and_eight_resistances,
    the_assess_key_shuts_an_open_pane_and_opens_a_shut_one,
    the_assess_key_shuts_the_pane_with_nothing_under_the_pointer,
    the_character_pane_draws_its_rows_and_has_nothing_left_undrawn,
    the_creature_pane_names_the_kind_the_level_and_the_nine_rows,
    the_inscribe_box_appears_only_on_something_inscribable,
    the_magic_lines_come_in_order_and_a_rate_beats_a_flat_cost,
    the_weapon_pane_gives_the_skill_the_speed_the_range_and_the_ammunition, ARMOUR_SESSION,
    EXAMINE_SESSION, GOLEM, LOOKER, POTION,
};

mod character_input;
use character_input::{
    a_press_picks_the_attribute_row_under_it_and_lands_on_the_list,
    a_press_picks_the_skill_row_under_it_and_the_attribute_page_stays_put,
    a_raise_sends_nothing_until_a_skill_row_is_picked,
    a_skill_values_colour_is_one_of_the_rows_own_three,
    both_character_pages_head_with_the_name_the_level_and_the_experience,
    every_attribute_row_draws_the_icon_its_own_group_names,
    every_skill_row_draws_its_value_in_a_font_that_exists,
    the_character_pages_numbers_carry_the_shipped_separator,
    the_footer_under_a_picked_skill_is_that_characters_own_arithmetic,
    the_raise_button_spends_experience_or_credits_by_what_the_skill_is,
    the_skills_page_opens_asking_for_a_pick_and_a_press_picks_one,
    the_ten_point_button_lights_up_and_a_real_press_sends_the_raise,
    turning_a_school_off_hides_its_spells_and_sends_the_whole_list,
};

mod character_sheet;
use character_sheet::{
    a_removed_death_count_disappears_from_the_character_sheet,
    an_augmentation_row_says_time_once_and_times_more_than_once,
    the_born_line_is_the_day_and_time_the_character_was_made,
    the_luminance_section_is_a_header_a_split_pair_and_an_unclamped_single,
    the_mastery_lines_name_the_weapon_group_rather_than_the_number,
    the_playtime_line_spells_out_only_the_terms_that_are_not_zero,
    the_six_sections_of_the_sheet_are_in_the_order_the_composer_appends_them,
};

mod character_stats;
use character_stats::{
    a_buffed_attribute_row_is_green_a_debuffed_one_red_and_the_rest_white,
    a_fractional_enchantment_rounds_in_the_rows_the_player_reads,
    a_vital_row_is_current_over_maximum_and_colours_by_the_maximum,
    an_empty_skills_page_is_distinguishable_from_a_full_one, attribute_row, enchantment,
    footer_child, ladder, open_the_character_page, press_skill_row, show_the_skills_page,
    skill_row, skills_panel_state,
    ten_points_cost_the_distance_between_two_entries_of_the_shipped_table,
    the_experience_to_raise_line_is_legible_and_uncovered,
    the_footer_title_is_the_name_the_number_and_the_signed_change,
    the_plus_ten_button_lights_when_the_unassigned_experience_covers_it,
    the_skills_page_builds_a_header_per_group_and_a_row_per_recorded_skill,
};

mod chess_board;
use chess_board::{
    a_message_about_another_board_changes_nothing,
    every_chess_message_reaches_a_receiver_or_a_sender,
    the_game_lamp_is_lit_from_the_first_use_until_the_game_is_over,
    the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window,
    the_opponents_move_is_replayed_on_the_players_own_board,
    the_seat_deals_the_board_and_the_start_names_whose_turn_it_is,
    the_window_buttons_offer_a_draw_and_ask_before_resigning,
    two_presses_move_a_piece_and_a_refused_move_never_leaves,
    using_a_board_raises_the_window_and_asks_for_a_seat,
};

mod contracts_list;
use contracts_list::{
    one_contract_at_a_time_is_added_changed_or_taken_away,
    picking_a_contract_fills_the_pane_beside_the_list,
    the_abandon_button_gives_up_the_picked_contract_and_nothing_local_moves,
    the_contracts_tab_draws_a_row_per_contract_in_name_order,
    the_sort_buttons_choose_the_order_and_a_second_press_turns_it_round,
};

mod earned_titles;
use earned_titles::{
    picking_a_title_arms_the_button_and_sends_nothing,
    the_display_button_puts_the_set_title_request_on_the_wire,
    the_titles_tab_draws_every_earned_title_sorted_by_name,
};

mod enchantments;
use enchantments::{
    a_character_who_logs_in_already_enchanted_reads_the_same_total,
    a_spell_cast_while_playing_moves_the_skill_row_it_is_about, spell_on,
};

mod housing;
use housing::{
    a_house_client, a_house_row_is_as_tall_as_the_text_it_holds,
    a_new_maintenance_period_clears_every_payment,
    a_paid_up_apartment_drops_the_location_row_and_doubles_the_period,
    a_payment_update_replaces_the_list_rather_than_merging,
    a_restriction_update_reaches_the_object_it_names,
    a_transaction_answer_clears_the_pane_as_a_status_does,
    a_zero_instant_prints_the_sentinel_and_not_the_epoch, answer_the_dialog, dialog_prompt,
    house_event, neither_maintenance_update_touches_a_houseless_character,
    the_client_asks_about_the_house_once_when_it_learns_who_it_is,
    the_house_price_rows_are_comma_joined_and_grouped,
    the_paid_maintenance_line_is_drawn_in_the_templates_green,
    the_purchase_time_line_reads_what_retail_prints_in_each_arm,
    the_same_command_twice_asks_twice_one_question_after_the_other,
    the_shards_house_message_fills_every_row_of_the_tab,
    two_confirmations_send_the_abandon_and_only_the_shard_empties_the_pane, HOUSE_PLAYER,
};

mod housing_commands;
use housing_commands::{
    a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing,
    every_house_sub_command_that_sends_puts_its_own_message_on_the_wire,
    the_free_houses_the_shard_lists_are_written_out_with_their_locations,
    the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log,
    the_help_listing_and_the_house_help_are_what_the_client_ships,
};

mod inscriptions;
use inscriptions::{
    a_box_somebody_else_signed_is_not_yours_to_change,
    a_press_empties_the_box_and_shows_whose_signature_it_will_carry,
    a_signed_thing_with_no_words_shows_a_blank_box,
    changing_the_target_to_one_you_may_not_write_on_drops_the_caret,
    changing_to_another_one_you_may_write_on_keeps_the_caret,
    escape_is_the_confirm_edge_and_return_is_not, leaving_an_unchanged_box_sends_nothing,
    letting_the_caret_go_sends_what_was_written, the_close_button_commits_on_its_way_out,
    the_invitation_is_centred_and_a_real_inscription_is_not, the_scribe_can_rub_out_his_own_words,
};

mod notebook;
use notebook::{
    a_client_that_does_not_know_its_character_writes_nothing,
    a_double_press_on_a_row_opens_that_page, a_journal_page_survives_a_restart,
    a_notebook_in_the_shipped_format_is_read_and_written_back,
    a_relog_keeps_each_characters_notebook, a_second_character_gets_its_own_notebook,
    a_typed_title_survives_a_page_turn, click_the_tab, el, el_text, el_visible,
    only_the_search_control_filters_the_page_list, open_the_page,
    pressing_a_row_then_delete_removes_that_rows_page, reset_clears_the_search_and_deletes_nothing,
    the_journal_opens_on_page_one_with_the_timer_editable,
    the_journal_timer_counts_down_and_the_button_resets_it, the_page_list_lists_the_journals_pages,
};

mod questions;
use questions::{
    a_making_question_gets_the_clients_own_continue,
    a_second_question_while_one_is_open_takes_over_its_handle,
    an_invitation_and_a_swearing_raise_their_own_panels_questions,
    an_npc_question_appears_and_both_answers_reach_the_shard,
    the_shards_withdrawal_closes_the_question_and_refuses_on_the_way_out, DIALOG_NO_BUTTON,
};

mod recording_support;
use recording_support::{
    a_recorded_character, corpus, description_blob, recorded_description, recorded_qualities,
    table, ORDERED_EVENT, SESSION,
};

mod skill_scrolling;
use skill_scrolling::{
    a_press_on_a_scrolled_skills_list_picks_the_row_it_is_drawn_over,
    a_skill_row_shows_the_total_after_everything_in_the_colour_of_the_change,
    a_weakened_character_shows_the_lower_number_still_drawn_plain,
    each_arrow_moves_the_skills_list_one_row_its_own_way,
    rebuilding_the_skills_list_keeps_where_it_was_scrolled_to,
    the_bar_beside_the_skills_list_reports_the_view_and_where_it_is,
    the_skills_list_is_one_column_of_equal_rows_mostly_out_of_sight,
};

mod ui_support;
use ui_support::{
    centre_of, deliver, element_visible, gameplay_screen, glyph_runs, one_run, state_colours,
};
