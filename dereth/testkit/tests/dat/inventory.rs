//! Inventory, trade and use: the pack and everything an item can be dragged into -- pickup, the
//! held item, the secure-trade and vendor windows, drag and drop, the paper doll, the shortcut bar,
//! use and target mode, splitting stacks, and slot decoration.
//!
//! Fixture: the retail dats under `$DERETH_TEST_DAT_DIR` read by headless clients, with recordings
//! replayed through `Inbound` where a claim needs the shard's own answers. **This binary must run
//! serially**: two headless clients in one process share the UI request globals. `ALL` is this
//! file's list, concatenated with the other subjects' in `census.rs`, so a scenario that is written
//! and not listed shows up as a shortfall. The dats are needed even where nothing reads a table:
//! an attachment is validated against the holder's part array, which comes from the shipped data.

use dereth_primitives::ObjectId;
use dereth_protocol::types::PublicWeenieDesc;
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound, Player};

dereth_testkit::scenarios! {
    scenario_a_stowed_item_leaves_the_hand_and_the_world => a_stowed_item_leaves_the_hand_and_the_world ["inventory.pickup.an-item-the-shard-says-was-stowed-leaves-the-hand-and-the-world"],
    scenario_a_stale_or_unknown_stow_changes_nothing => a_stale_or_unknown_stow_changes_nothing ["inventory.pickup.a-stale-or-unknown-stow-changes-nothing"],
    scenario_a_holders_own_list_decides_what_it_holds => a_holders_own_list_decides_what_it_holds ["inventory.held-item.a-holders-own-list-decides-what-hangs-off-it"],
    scenario_every_trade_element_is_in_the_live_tree => every_trade_element_is_in_the_live_tree ["trade.window.every-element-the-panel-binds-to-is-in-the-shipped-tree"],
    scenario_a_drop_is_drawn_before_the_shard_answers => a_drop_is_drawn_before_the_shard_answers ["trade.table.the-optimistic-row-is-drawn-before-the-shard-answers-and-is-not-doubled-by-it"],
    scenario_the_partner_name_follows_the_partner_alone => the_partner_name_follows_the_partner_alone ["trade.window.the-partners-name-follows-the-partner-and-a-close-blanks-it"],
    scenario_the_light_and_the_button_are_two_different_things => the_light_and_the_button_are_two_different_things ["trade.window.the-partners-light-and-your-own-button-are-two-different-things"],
    scenario_each_sides_total_counts_only_its_own_side => each_sides_total_counts_only_its_own_side ["trade.window.each-sides-total-counts-only-its-own-side"],
    scenario_your_own_name_is_bound_and_never_written => your_own_name_is_bound_and_never_written ["trade.window.your-own-name-is-bound-and-never-written"],
    scenario_a_shard_failure_takes_the_optimistic_row_off => a_shard_failure_takes_the_optimistic_row_off ["trade.removal.the-shards-failure-takes-the-optimistic-row-off-the-table"],
    scenario_a_withdrawal_names_a_side => a_withdrawal_names_a_side ["trade.removal.the-shard-withdrawing-your-row-takes-it-off-and-the-partners-does-not"],
    scenario_a_plain_inventory_refusal_leaves_the_row => a_plain_inventory_refusal_leaves_the_row ["trade.removal.a-plain-inventory-refusal-leaves-the-row-where-it-is"],
    scenario_an_item_on_the_table_wears_the_traded_mark => an_item_on_the_table_wears_the_traded_mark ["trade.table.an-item-put-on-the-table-wears-the-traded-mark-on-its-slot"],
    scenario_a_stack_on_the_trade_table_is_counted_and_pluralised => a_stack_on_the_trade_table_is_counted_and_pluralised ["trade.table.a-stack-tells-you-how-many-there-are-and-what-they-are-called"],
    scenario_the_tooltip_reaches_the_slot_itself => the_tooltip_reaches_the_slot_itself ["trade.table.the-tooltip-reaches-the-slot-itself-and-not-only-the-panels-record"],
    scenario_a_stack_in_vendor_stock_is_counted_and_pluralised => a_stack_in_vendor_stock_is_counted_and_pluralised ["vendor.stock.a-stack-tells-you-how-many-there-are-and-what-they-are-called"],
    scenario_the_vendor_numeral_is_the_advertised_amount => the_vendor_numeral_is_the_advertised_amount ["vendor.stock.the-numeral-is-the-amount-the-shop-advertises-on-the-row-the-player-picked"],
    scenario_the_trade_table_follows_every_frozen_fact_and_not_the_clock => the_trade_table_follows_every_frozen_fact_and_not_the_clock ["trade.table.what-is-drawn-follows-every-frozen-fact-about-an-item-and-not-the-clock"],
    scenario_the_vendor_stock_follows_every_frozen_fact_and_not_the_clock => the_vendor_stock_follows_every_frozen_fact_and_not_the_clock ["vendor.stock.what-is-drawn-follows-every-frozen-fact-about-an-item-and-not-the-clock"],
    scenario_a_stock_row_follows_a_late_picture_and_a_new_price => a_stock_row_follows_a_late_picture_and_a_new_price ["panels.redraw.a-shops-stock-follows-a-picture-or-a-price-arriving-under-an-unchanged-row"],
    scenario_the_partners_side_follows_a_row_filled_in_later => the_partners_side_follows_a_row_filled_in_later ["panels.redraw.the-partners-side-of-the-trade-follows-a-row-being-filled-in-later"],
    scenario_a_vassals_own_fields_redraw_the_row => a_vassals_own_fields_redraw_the_row ["panels.redraw.the-vassal-list-follows-a-members-own-fields-changing-under-an-unchanged-id"],
    scenario_an_owned_component_count_redraws_its_row => an_owned_component_count_redraws_its_row ["panels.redraw.the-component-list-follows-a-count-changing-under-an-unchanged-component"],
    scenario_a_skill_row_follows_the_enchanted_number_and_the_penalty => a_skill_row_follows_the_enchanted_number_and_the_penalty ["panels.redraw.a-skill-row-follows-the-enchanted-number-and-the-death-penalty-each-on-their-own"],
    scenario_the_attribute_rows_compare_the_text_they_write => the_attribute_rows_compare_the_text_they_write ["panels.redraw.the-attribute-rows-compare-the-very-text-they-write"],
    scenario_a_move_request_ghosts_its_own_item_at_once => a_move_request_ghosts_its_own_item_at_once ["inventory.pack.a-request-the-player-made-ghosts-its-own-item-at-once"],
    scenario_the_grid_follows_the_containers_own_capacity => the_grid_follows_the_containers_own_capacity ["inventory.pack.the-grid-draws-as-many-slots-as-the-container-can-hold"],
    scenario_the_side_strip_follows_the_players_own_capacity => the_side_strip_follows_the_players_own_capacity ["inventory.pack.the-side-strip-draws-as-many-slots-as-the-player-can-carry-packs"],
    scenario_the_pack_ring_counts_down_while_the_gate_is_shut => the_pack_ring_counts_down_while_the_gate_is_shut ["inventory.cooldown.a-running-ring-counts-down-while-the-pack-is-otherwise-still"],
    scenario_a_pack_slot_follows_every_word_it_draws => a_pack_slot_follows_every_word_it_draws ["inventory.pack.a-slot-is-redrawn-when-anything-it-draws-about-its-item-changes"],
    scenario_the_eighteen_tiles_fill_and_hold_still => the_eighteen_tiles_fill_and_hold_still ["shortcut.bar.the-eighteen-tiles-fill-and-an-unchanged-frame-redraws-nothing"],
    scenario_a_tile_follows_every_thing_it_draws => a_tile_follows_every_thing_it_draws ["shortcut.bar.a-tile-is-redrawn-when-anything-it-draws-about-its-item-changes"],
    scenario_the_bar_ring_counts_down_while_the_gate_is_shut => the_bar_ring_counts_down_while_the_gate_is_shut ["shortcut.cooldown.a-running-ring-counts-down-while-the-bar-is-otherwise-still"],
    scenario_a_world_drop_is_refused_in_mid_air => a_world_drop_is_refused_in_mid_air ["inventory.world-drop.a-player-in-mid-air-is-refused-and-one-on-the-ground-is-not"],
    scenario_a_give_in_mid_air_is_not_refused => a_give_in_mid_air_is_not_refused ["inventory.give.handing-something-over-in-mid-air-is-not-refused"],
    scenario_the_ground_answer_is_read_again_every_frame => the_ground_answer_is_read_again_every_frame ["inventory.world-drop.whether-the-player-is-on-the-ground-is-read-again-every-frame"],
    scenario_a_purchase_leaves_the_player_able_to_act => a_purchase_leaves_the_player_able_to_act ["vendor.busy.a-purchase-leaves-the-player-able-to-act"],
    scenario_a_sale_leaves_the_player_able_to_act => a_sale_leaves_the_player_able_to_act ["vendor.busy.a-sale-leaves-the-player-able-to-act"],
    scenario_while_the_shop_has_not_answered_the_refusal_still_fires => while_the_shop_has_not_answered_the_refusal_still_fires ["vendor.busy.while-the-shop-has-not-answered-the-refusal-still-fires"],
    scenario_a_salvage_row_dragged_out_leaves_the_list => a_salvage_row_dragged_out_leaves_the_list ["salvage.row.dragging-one-out-takes-it-off-the-list-and-sends-nothing"],
    scenario_a_refusal_naming_the_list_starts_nothing => a_refusal_naming_the_list_starts_nothing ["salvage.row.a-message-naming-the-whole-list-starts-no-drag"],
    scenario_a_refused_drag_still_takes_the_salvage_row_off => a_refused_drag_still_takes_the_salvage_row_off ["salvage.row.a-drag-the-shell-refuses-still-takes-the-row-off-and-letting-go-does-not-put-it-back"],
    scenario_the_money_lines_are_five_different_places => the_money_lines_are_five_different_places ["vendor.money.the-four-lines-and-the-filter-strip-are-five-different-things-in-the-shipped-tree"],
    scenario_each_money_line_follows_its_own_source => each_money_line_follows_its_own_source ["vendor.money.each-of-the-four-lines-is-written-by-its-own-source"],
    scenario_a_basket_is_counted_in_things_and_not_in_rows => a_basket_is_counted_in_things_and_not_in_rows ["vendor.money.a-basket-is-counted-in-things-and-not-in-rows"],
    scenario_the_filter_strip_follows_the_stock => the_filter_strip_follows_the_stock ["vendor.stock.the-filter-strip-is-built-from-the-stock-that-is-actually-there"],
    scenario_the_purse_is_filled_from_the_players_own_coin => the_purse_is_filled_from_the_players_own_coin ["vendor.purse.is-filled-from-the-players-own-coin-when-the-shop-opens"],
    scenario_the_shop_shows_the_players_own_coin_and_follows_it => the_shop_shows_the_players_own_coin_and_follows_it ["vendor.purse.the-open-shop-shows-the-players-own-coin-and-follows-it"],
    scenario_the_picked_stock_row_writes_its_name_and_its_cost => the_picked_stock_row_writes_its_name_and_its_cost ["vendor.stock.the-picked-row-writes-its-name-and-its-cost-and-wakes-both-buttons"],
    scenario_closing_with_a_basket_asks_first => closing_with_a_basket_asks_first ["vendor.close.leaving-with-something-in-a-basket-asks-first"],
    scenario_a_pack_slot_answers_the_pointer_at_its_own_centre => a_pack_slot_answers_the_pointer_at_its_own_centre ["inventory.drag.a-pack-slot-answers-the-pointer-at-its-own-centre"],
    scenario_a_press_and_a_move_lift_the_items_own_picture => a_press_and_a_move_lift_the_items_own_picture ["inventory.drag.a-press-and-a-move-lift-the-items-own-picture-off-its-slot"],
    scenario_a_drop_on_a_side_pack_becomes_one_move => a_drop_on_a_side_pack_becomes_one_move ["inventory.drag.a-drop-on-a-side-pack-becomes-one-move-naming-the-pack-under-the-pointer"],
    scenario_an_empty_slot_and_a_forbidden_list_start_nothing => an_empty_slot_and_a_forbidden_list_start_nothing ["inventory.drag.an-empty-slot-starts-nothing-and-a-list-that-forbids-it-starts-nothing"],
    scenario_the_backpack_button_lights_while_something_is_over_it => the_backpack_button_lights_while_something_is_over_it ["inventory.drag-hint.the-backpack-button-lights-while-something-is-carried-over-it"],
    scenario_a_body_slot_shows_whether_it_could_take_what_is_carried => a_body_slot_shows_whether_it_could_take_what_is_carried ["inventory.drag-hint.a-body-slot-shows-whether-it-could-take-what-is-carried"],
    scenario_a_shortcut_tile_offers_itself_for_anything_carried => a_shortcut_tile_offers_itself_for_anything_carried ["inventory.drag-hint.a-shortcut-tile-offers-itself-for-anything-carried"],
    scenario_letting_go_on_a_tile_takes_its_own_hint_down => letting_go_on_a_tile_takes_its_own_hint_down ["inventory.drag-hint.letting-go-on-a-tile-takes-its-own-hint-down"],
    scenario_dropping_a_worn_thing_back_on_its_own_slot_says_so => dropping_a_worn_thing_back_on_its_own_slot_says_so ["inventory.wear.dropping-a-worn-thing-back-on-its-own-slot-says-it-is-already-worn"],
    scenario_the_figure_says_whether_it_could_take_what_is_carried => the_figure_says_whether_it_could_take_what_is_carried ["inventory.body.the-figure-says-whether-it-could-take-what-is-carried"],
    scenario_something_already_worn_leaves_the_figure_saying_nothing => something_already_worn_leaves_the_figure_saying_nothing ["inventory.body.something-already-worn-leaves-the-figure-saying-nothing"],
    scenario_the_figures_answer_comes_down_on_leave_and_on_the_drop => the_figures_answer_comes_down_on_leave_and_on_the_drop ["inventory.body.the-figures-answer-comes-down-on-leave-and-on-the-drop"],
    scenario_a_wearable_dropped_on_the_figure_is_put_on_with_its_own_places => a_wearable_dropped_on_the_figure_is_put_on_with_its_own_places ["inventory.body.a-wearable-dropped-on-the-figure-is-put-on-with-its-own-list-of-places"],
    scenario_what_cannot_be_worn_at_all_is_refused_in_words => what_cannot_be_worn_at_all_is_refused_in_words ["inventory.body.what-cannot-be-worn-at-all-is-refused-in-the-clients-own-words"],
    scenario_an_occupied_place_names_what_is_in_the_way => an_occupied_place_names_what_is_in_the_way ["inventory.body.an-occupied-place-names-what-is-in-the-way"],
    scenario_something_already_worn_is_told_so_when_it_is_dropped => something_already_worn_is_told_so_when_it_is_dropped ["inventory.body.something-already-worn-is-told-so-when-it-is-dropped"],
    scenario_a_thing_pushed_off_its_tile_keeps_its_shortcut_in_the_next_free_slot => a_thing_pushed_off_its_tile_keeps_its_shortcut_in_the_next_free_slot ["shortcut.number.a-thing-pushed-off-its-tile-keeps-its-shortcut-in-the-next-free-slot"],
    scenario_an_assigned_thing_draws_its_slot_number_wherever_its_picture_appears => an_assigned_thing_draws_its_slot_number_wherever_its_picture_appears ["shortcut.number.an-assigned-thing-draws-its-slot-number-wherever-its-picture-appears"],
    scenario_the_number_follows_the_assignment_off_and_on_again => the_number_follows_the_assignment_off_and_on_again ["shortcut.number.it-follows-the-assignment-off-and-on-to-a-different-slot"],
    scenario_the_magic_stance_dims_the_number_everywhere => the_magic_stance_dims_the_number_everywhere ["shortcut.number.the-magic-stance-dims-it-everywhere-it-is-drawn"],
    scenario_the_number_is_drawn_over_the_busy_mark_and_the_ring => the_number_is_drawn_over_the_busy_mark_and_the_ring ["shortcut.number.it-is-drawn-over-the-busy-mark-and-the-selection-ring"],
    scenario_a_drag_that_ends_over_nothing_takes_the_mark_off => a_drag_that_ends_over_nothing_takes_the_mark_off ["inventory.busy-mark.a-drag-that-ends-over-nothing-takes-it-off"],
    scenario_a_drop_that_moves_nothing_takes_the_mark_off => a_drop_that_moves_nothing_takes_the_mark_off ["inventory.busy-mark.a-drop-that-moves-nothing-takes-it-off"],
    scenario_a_drop_on_the_shortcut_bar_takes_the_mark_off => a_drop_on_the_shortcut_bar_takes_the_mark_off ["inventory.busy-mark.a-drop-on-the-shortcut-bar-takes-it-off"],
    scenario_the_shards_two_answers_both_take_the_mark_off => the_shards_two_answers_both_take_the_mark_off ["inventory.busy-mark.the-shards-refusal-and-the-shards-move-both-take-it-off"],
    scenario_every_tile_mirrors_the_things_own_state => every_tile_mirrors_the_things_own_state ["inventory.busy-mark.every-tile-mirrors-the-things-own-state"],
    scenario_a_destroyed_thing_leaves_no_tile_and_no_mark => a_destroyed_thing_leaves_no_tile_and_no_mark ["inventory.busy-mark.a-thing-the-shard-destroys-leaves-no-tile-and-no-mark"],
    scenario_a_tile_clears_its_own_before_catching_another_thing => a_tile_clears_its_own_before_catching_another_thing ["inventory.busy-mark.a-tile-clears-its-own-before-catching-another-thing"],
    scenario_the_backpack_button_sends_one_move_and_keeps_the_mark => the_backpack_button_sends_one_move_and_keeps_the_mark ["inventory.backpack-button.a-drop-sends-one-move-and-keeps-the-mark-until-the-answer"],
    scenario_a_refused_backpack_drop_sends_nothing_and_takes_the_mark_off => a_refused_backpack_drop_sends_nothing_and_takes_the_mark_off ["inventory.backpack-button.a-refused-drop-sends-nothing-and-takes-the-mark-off"],
    scenario_a_full_pack_is_named_the_way_the_player_would_name_it => a_full_pack_is_named_the_way_the_player_would_name_it ["inventory.refusal.a-full-pack-is-named-the-way-the-player-would-name-it"],
    scenario_a_drop_on_the_open_chest_puts_the_thing_in_it_once => a_drop_on_the_open_chest_puts_the_thing_in_it_once ["container.ground.a-drop-on-the-open-ones-own-list-puts-the-thing-in-it-exactly-once"],
    scenario_a_drop_on_the_chests_own_row_goes_into_it_just_the_same => a_drop_on_the_chests_own_row_goes_into_it_just_the_same ["container.ground.a-drop-on-the-open-ones-own-row-goes-into-it-just-the-same"],
    scenario_a_hook_the_player_does_not_own_refuses_the_drop_in_words => a_hook_the_player_does_not_own_refuses_the_drop_in_words ["container.ground.a-hook-in-a-house-the-player-does-not-own-refuses-the-drop-in-words"],
    scenario_identifying_a_locked_container_says_so_and_how_hard_the_lock_is => identifying_a_locked_container_says_so_and_how_hard_the_lock_is ["container.lock.identifying-a-locked-one-says-so-and-how-hard-the-lock-is"],
    scenario_a_cancelled_drag_out_of_the_chest_leaves_no_grey_row => a_cancelled_drag_out_of_the_chest_leaves_no_grey_row ["inventory.busy-mark.a-drag-out-of-the-open-one-that-ends-over-nothing-leaves-no-grey-row"],
    scenario_a_refused_move_out_of_the_chest_clears_it_and_lets_the_next_one_go => a_refused_move_out_of_the_chest_clears_it_and_lets_the_next_one_go ["inventory.busy-mark.a-refused-move-out-of-the-open-one-clears-it-and-lets-the-next-one-go"],
    scenario_the_tile_a_drop_lands_on_clears_its_own_grey_mark => the_tile_a_drop_lands_on_clears_its_own_grey_mark ["inventory.busy-mark.the-tile-a-drop-lands-on-clears-its-own-and-not-the-source-s"],
    scenario_a_full_destination_draws_no_row_and_the_move_spills_to_a_side_pack => a_full_destination_draws_no_row_and_the_move_spills_to_a_side_pack ["inventory.pending-row.a-destination-that-is-full-draws-none-and-the-move-spills-to-a-side-pack"],
    scenario_a_destination_with_room_draws_one_grey_row_in_the_cell_aimed_at => a_destination_with_room_draws_one_grey_row_in_the_cell_aimed_at ["inventory.pending-row.a-destination-with-room-draws-one-grey-in-the-very-cell-aimed-at"],
    scenario_a_full_destination_with_nowhere_to_spill_says_so_and_sends_nothing => a_full_destination_with_nowhere_to_spill_says_so_and_sends_nothing ["inventory.refusal.a-full-destination-with-nowhere-to-spill-says-so-and-sends-nothing"],
    scenario_the_waiting_row_is_drawn_at_once_and_the_answer_replaces_it => the_waiting_row_is_drawn_at_once_and_the_answer_replaces_it ["inventory.pending-row.the-row-is-drawn-before-the-shard-answers-and-the-answer-replaces-it"],
    scenario_a_refusal_takes_the_waiting_row_away_and_un_greys_the_source => a_refusal_takes_the_waiting_row_away_and_un_greys_the_source ["inventory.pending-row.a-refusal-takes-the-row-away-and-un-greys-what-never-moved"],
    scenario_a_drop_aimed_at_a_side_pack_draws_no_waiting_row => a_drop_aimed_at_a_side_pack_draws_no_waiting_row ["inventory.pending-row.a-drop-aimed-at-a-side-pack-draws-none-and-still-sends-the-move"],
    scenario_a_drop_the_client_refuses_for_itself_draws_no_waiting_row => a_drop_the_client_refuses_for_itself_draws_no_waiting_row ["inventory.pending-row.a-drop-the-client-refuses-for-itself-draws-none-either"],
    scenario_a_drop_on_the_backpack_button_draws_the_waiting_row_at_the_head => a_drop_on_the_backpack_button_draws_the_waiting_row_at_the_head ["inventory.backpack-button.a-drop-on-it-draws-the-waiting-row-at-the-head-of-the-pack"],
    scenario_a_second_drop_on_the_same_list_names_what_is_already_being_placed => a_second_drop_on_the_same_list_names_what_is_already_being_placed ["inventory.pending-row.a-second-drop-on-the-same-list-names-what-is-already-being-placed"],
    scenario_a_second_drop_on_a_different_list_meets_the_one_request_hold => a_second_drop_on_a_different_list_meets_the_one_request_hold ["inventory.pending-row.a-second-drop-on-a-different-list-meets-the-one-request-hold-instead"],
    scenario_once_the_waiting_row_is_gone_the_same_drop_is_taken_again => once_the_waiting_row_is_gone_the_same_drop_is_taken_again ["inventory.pending-row.once-the-row-is-gone-the-very-same-drop-is-taken-again"],
    scenario_dragging_yourself_into_your_own_pack_is_refused_by_the_list_itself => dragging_yourself_into_your_own_pack_is_refused_by_the_list_itself ["inventory.refusal.dragging-yourself-into-your-own-pack-is-refused-by-the-list-itself"],
    scenario_a_plain_thing_on_an_empty_pack_strip_slot_is_refused_for_its_kind => a_plain_thing_on_an_empty_pack_strip_slot_is_refused_for_its_kind ["inventory.refusal.a-plain-thing-on-an-empty-pack-strip-slot-is-refused-for-being-the-wrong-kind"],
    scenario_a_pack_on_the_item_grid_is_refused_for_its_kind => a_pack_on_the_item_grid_is_refused_for_its_kind ["inventory.refusal.a-pack-on-the-item-grid-is-refused-for-being-the-wrong-kind"],
    scenario_a_refused_drag_takes_the_mark_off_the_tile_as_well => a_refused_drag_takes_the_mark_off_the_tile_as_well ["inventory.busy-mark.a-refused-drag-takes-the-mark-off-the-tile-as-well-as-off-the-thing"],
    scenario_a_row_with_nothing_behind_it_does_not_keep_a_mark_it_was_given => a_row_with_nothing_behind_it_does_not_keep_a_mark_it_was_given ["inventory.busy-mark.a-row-with-nothing-behind-it-does-not-keep-a-mark-it-was-given"],
    scenario_dragging_a_shortcut_off_the_bar_empties_its_tile_and_tells_the_shard => dragging_a_shortcut_off_the_bar_empties_its_tile_and_tells_the_shard ["shortcut.bar.dragging-one-off-the-bar-empties-its-tile-and-tells-the-shard"],
    scenario_dragging_a_shortcut_tile_to_tile_moves_it_and_tells_the_shard_both_halves => dragging_a_shortcut_tile_to_tile_moves_it_and_tells_the_shard_both_halves ["shortcut.bar.dragging-one-tile-to-tile-moves-it-and-tells-the-shard-both-halves"],
    scenario_a_drag_out_of_the_open_chest_moves_it_to_the_pack_and_keeps_the_mark => a_drag_out_of_the_open_chest_moves_it_to_the_pack_and_keeps_the_mark ["container.ground.a-drag-out-of-the-open-one-onto-the-pack-moves-it-and-keeps-the-mark"],
    scenario_the_open_chest_lights_the_row_the_carried_thing_is_over => the_open_chest_lights_the_row_the_carried_thing_is_over ["container.ground.the-open-one-lights-the-row-the-carried-thing-is-over-and-no-other"],
    scenario_a_hook_shows_whether_it_could_take_what_is_carried => a_hook_shows_whether_it_could_take_what_is_carried ["container.ground.a-hook-shows-whether-it-could-take-what-is-carried"],
    scenario_a_hook_in_nobodys_house_refuses_everything_carried_over_it => a_hook_in_nobodys_house_refuses_everything_carried_over_it ["container.ground.a-hook-in-nobodys-house-refuses-everything-carried-over-it"],
    scenario_the_windows_own_row_answers_as_a_list_of_packs_would => the_windows_own_row_answers_as_a_list_of_packs_would ["container.ground.the-windows-own-row-answers-a-carried-thing-as-a-list-of-packs-would"],
    scenario_a_pack_over_the_contents_lights_green_and_is_still_refused_on_the_drop => a_pack_over_the_contents_lights_green_and_is_still_refused_on_the_drop ["container.ground.a-pack-over-the-contents-lights-green-and-is-still-refused-on-the-drop"],
    scenario_a_pack_goes_into_the_chest_through_its_own_strip => a_pack_goes_into_the_chest_through_its_own_strip ["container.ground.a-pack-goes-in-through-the-windows-own-strip-of-packs"],
    scenario_every_recorded_give_is_answered_by_the_move_and_nothing_else => every_recorded_give_is_answered_by_the_move_and_nothing_else ["inventory.give.every-recorded-one-is-answered-by-the-move-and-by-nothing-else"],
    scenario_a_give_takes_the_hold_and_the_next_gesture_is_refused_in_words => a_give_takes_the_hold_and_the_next_gesture_is_refused_in_words ["inventory.give.the-hold-it-takes-refuses-the-next-gesture-in-the-clients-own-words"],
    scenario_the_shards_move_releases_the_give_and_the_next_one_goes_out => the_shards_move_releases_the_give_and_the_next_one_goes_out ["inventory.give.the-shards-move-releases-the-hold-and-the-next-one-goes-out"],
    scenario_the_shards_refusal_releases_the_give_and_says_which_thing => the_shards_refusal_releases_the_give_and_says_which_thing ["inventory.give.the-shards-refusal-releases-the-hold-and-says-which-thing"],
    scenario_part_of_a_stack_is_released_by_the_stacks_new_count => part_of_a_stack_is_released_by_the_stacks_new_count ["inventory.give.part-of-a-stack-is-released-by-the-stacks-new-count"],
    scenario_a_container_being_viewed_does_not_release_a_give => a_container_being_viewed_does_not_release_a_give ["inventory.give.the-contents-of-a-container-being-viewed-do-not-release-one"],
    scenario_something_carried_over_the_window_turns_it_to_the_selling_tab => something_carried_over_the_window_turns_it_to_the_selling_tab ["vendor.tabs.something-carried-over-the-window-turns-it-to-the-selling-tab"],
    scenario_the_shop_says_whether_it_would_buy_what_is_carried_over_it => the_shop_says_whether_it_would_buy_what_is_carried_over_it ["vendor.sell.the-window-says-whether-it-would-buy-what-is-carried-over-it"],
    scenario_something_offered_to_the_shop_wears_the_mark_in_both_windows => something_offered_to_the_shop_wears_the_mark_in_both_windows ["vendor.sell.something-offered-wears-the-mark-in-the-pack-as-well-as-in-the-window"],
    scenario_a_refused_world_drop_names_the_thing_as_the_player_sees_it => a_refused_world_drop_names_the_thing_as_the_player_sees_it ["inventory.refusal.a-world-drop-the-shard-refuses-names-the-thing-as-the-player-sees-it"],
    scenario_a_refused_container_drop_says_so_in_its_own_words => a_refused_container_drop_says_so_in_its_own_words ["inventory.refusal.a-container-drop-the-shard-refuses-says-so-in-its-own-words"],
    scenario_an_attuned_thing_says_so_in_its_description => an_attuned_thing_says_so_in_its_description ["inventory.attunement.an-attuned-thing-says-so-in-its-description"],
    scenario_a_real_drag_across_the_grid_asks_for_the_place_the_player_aimed_at => a_real_drag_across_the_grid_asks_for_the_place_the_player_aimed_at ["inventory.place.a-real-drag-across-the-grid-asks-for-the-place-the-player-aimed-at"],
    scenario_no_cursor_the_client_ships_is_chosen_by_a_drag => no_cursor_the_client_ships_is_chosen_by_a_drag ["inventory.drag-hint.no-cursor-the-client-ships-is-chosen-by-a-drag"],
    scenario_every_live_slot_carries_the_overlay_the_hint_is_drawn_on => every_live_slot_carries_the_overlay_the_hint_is_drawn_on ["inventory.drag-hint.every-live-slot-carries-the-overlay-the-hint-is-drawn-on"],
    scenario_a_real_drag_puts_the_hint_up_on_what_it_crosses_and_takes_it_down_again => a_real_drag_puts_the_hint_up_on_what_it_crosses_and_takes_it_down_again ["inventory.drag-hint.a-real-drag-puts-the-hint-up-on-what-it-crosses-and-takes-it-down-again"],
    scenario_the_crossing_is_reported_as_a_leave_and_an_enter_in_that_order => the_crossing_is_reported_as_a_leave_and_an_enter_in_that_order ["inventory.drag-hint.the-crossing-is-reported-as-a-leave-and-an-enter-in-that-order"],
    scenario_the_crossing_message_is_not_the_focus_message => the_crossing_message_is_not_the_focus_message ["inventory.drag-hint.the-crossing-message-is-not-the-focus-message"],
    scenario_the_drop_is_aimed_at_the_element_the_cursor_was_over => the_drop_is_aimed_at_the_element_the_cursor_was_over ["inventory.drag-hint.the-drop-is-aimed-at-the-element-the-cursor-was-over"],
    scenario_a_kit_key_then_the_main_pack_key_uses_the_kit_on_the_player => a_kit_key_then_the_main_pack_key_uses_the_kit_on_the_player ["inventory.shortcut-bar.a-kit-key-then-the-main-pack-key-uses-the-kit-on-the-player"],
    scenario_a_thing_dropped_on_a_tile_fills_it_and_tells_the_shard_once => a_thing_dropped_on_a_tile_fills_it_and_tells_the_shard_once ["inventory.shortcut-bar.a-thing-dropped-on-a-tile-fills-it-and-tells-the-shard-once"],
    scenario_a_pack_can_be_put_on_a_tile_like_anything_else => a_pack_can_be_put_on_a_tile_like_anything_else ["inventory.shortcut-bar.a-pack-can-be-put-on-a-tile-like-anything-else"],
    scenario_the_number_key_of_a_filled_tile_uses_what_is_in_it => the_number_key_of_a_filled_tile_uses_what_is_in_it ["inventory.shortcut-bar.the-number-key-of-a-filled-tile-uses-what-is-in-it"],
    scenario_a_number_key_pressed_with_a_cursor_armed_finishes_that_gesture => a_number_key_pressed_with_a_cursor_armed_finishes_that_gesture ["inventory.shortcut-bar.a-number-key-pressed-with-a-cursor-armed-finishes-that-gesture"],
    scenario_letting_go_takes_the_shop_hint_down_whichever_answer_it_was_showing => letting_go_takes_the_shop_hint_down_whichever_answer_it_was_showing ["vendor.sell.letting-go-takes-the-hint-down-whichever-answer-it-was-showing"],
    scenario_coming_back_to_the_stock_tab_draws_every_row_decorated_again => coming_back_to_the_stock_tab_draws_every_row_decorated_again ["vendor.stock.coming-back-to-the-stock-tab-draws-every-row-decorated-again"],
    scenario_a_pack_let_go_on_the_window_offers_what_is_inside_it_and_not_itself => a_pack_let_go_on_the_window_offers_what_is_inside_it_and_not_itself ["vendor.sell.a-pack-let-go-on-the-window-offers-what-is-inside-it-and-not-itself"],
    scenario_an_empty_pack_is_offered_as_itself => an_empty_pack_is_offered_as_itself ["vendor.sell.an-empty-pack-is-offered-as-itself"],
    scenario_every_place_the_figure_is_filled_from_answers_a_drop_with_its_own_place => every_place_the_figure_is_filled_from_answers_a_drop_with_its_own_place ["inventory.body.every-place-the-figure-is-filled-from-answers-a-drop-with-its-own-place"],
    scenario_every_place_on_the_figure_is_a_place_to_wear_and_no_cell_of_a_pack_is => every_place_on_the_figure_is_a_place_to_wear_and_no_cell_of_a_pack_is ["inventory.body.every-place-on-the-figure-is-a-place-to-wear-and-no-cell-of-a-pack-is"],
    scenario_the_same_drag_is_a_wear_on_the_figure_and_a_move_into_a_pack => the_same_drag_is_a_wear_on_the_figure_and_a_move_into_a_pack ["inventory.body.the-same-drag-is-a-wear-on-the-figure-and-a-move-into-a-pack"],
    scenario_the_picture_of_the_character_is_a_twenty_fifth_place_to_let_something_go => the_picture_of_the_character_is_a_twenty_fifth_place_to_let_something_go ["inventory.body.the-picture-of-the-character-is-a-twenty-fifth-place-to-let-something-go"],
    scenario_asking_for_the_grid_of_places_takes_the_picture_out_of_the_hit_test => asking_for_the_grid_of_places_takes_the_picture_out_of_the_hit_test ["inventory.body.asking-for-the-grid-of-places-takes-the-picture-out-of-the-hit-test"],
    scenario_the_shipped_picture_of_the_character_is_painted_for_every_region => the_shipped_picture_of_the_character_is_painted_for_every_region ["inventory.body.the-shipped-picture-of-the-character-is-painted-for-every-region"],
    scenario_a_click_on_the_picture_selects_what_is_worn_in_that_region => a_click_on_the_picture_selects_what_is_worn_in_that_region ["inventory.body.a-click-on-the-picture-selects-what-is-worn-in-that-region"],
    scenario_a_click_on_a_bare_region_of_the_picture_selects_the_player => a_click_on_a_bare_region_of_the_picture_selects_the_player ["inventory.body.a-click-on-a-bare-region-of-the-picture-selects-the-player"],
    scenario_the_shipped_use_key_raises_the_question_and_the_two_answers_differ => the_shipped_use_key_raises_the_question_and_the_two_answers_differ ["use.confirmation.the-shipped-use-key-raises-the-question-and-the-two-answers-differ"],
    scenario_the_shards_own_message_puts_the_shop_on_screen_and_not_only_its_tabs => the_shards_own_message_puts_the_shop_on_screen_and_not_only_its_tabs ["vendor.window.the-shards-own-message-puts-the-shop-on-screen-and-not-only-its-tabs"],
    scenario_the_shards_unlock_reaches_the_chest_and_the_relock_follows_it => the_shards_unlock_reaches_the_chest_and_the_relock_follows_it ["container.ground.the-shards-unlock-reaches-the-chest-and-the-relock-follows-it"],
    scenario_the_shipped_key_puts_what_is_picked_into_the_players_own_pack => the_shipped_key_puts_what_is_picked_into_the_players_own_pack ["inventory.pickup.the-shipped-key-puts-what-is-picked-into-the-players-own-pack"],
    scenario_the_shipped_key_carries_the_picked_stacks_own_count_into_the_merge => the_shipped_key_carries_the_picked_stacks_own_count_into_the_merge ["inventory.pickup.the-shipped-key-carries-the-picked-stacks-own-count-into-the-merge"],
    scenario_the_shipped_key_with_nothing_picked_sends_nothing => the_shipped_key_with_nothing_picked_sends_nothing ["inventory.pickup.the-shipped-key-with-nothing-picked-sends-nothing"],
    scenario_the_first_recorded_shop_opens_on_the_purse_the_login_description_carried => the_first_recorded_shop_opens_on_the_purse_the_login_description_carried ["vendor.purse.the-first-recorded-shop-opens-on-the-purse-the-login-description-carried"],
    scenario_a_purse_that_came_only_from_the_login_can_buy_what_it_can_afford => a_purse_that_came_only_from_the_login_can_buy_what_it_can_afford ["vendor.purse.a-purse-that-came-only-from-the-login-can-buy-what-it-can-afford"],
    scenario_a_stale_coin_update_for_the_player_is_dropped_by_the_same_counter => a_stale_coin_update_for_the_player_is_dropped_by_the_same_counter ["vendor.purse.a-stale-coin-update-for-the-player-is-dropped-by-the-same-counter"],
}

// =============================================================================================
// The secure-trade window and the vendor's stock, against the live shipped element tree.
//
// Every claim below is about what a player sees in a window the client built out of
// `client_portal.dat`, so every one of them is read back off a live element rather than off the
// panel's own record of what it wrote -- which is the question a model test structurally cannot
// ask.
// =============================================================================================

use dereth_client::app::App;
use dereth_primitives::DataId;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::items::widget::ItemListWidget;
use dereth_ui_screens::panels::trade::{self, status_state, ButtonState, TradePanel};
use dereth_ui_screens::panels::vendor::{Tab, VendorPanel};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{GameView, ShopRow, ShopView, SlotDecoration, TradeRow, TradeView};

// =============================================================================================
// The redraw gates: what makes a panel draw again, and the one input no gate can hold.
//
// One rule runs through all of it: **a panel may skip a redraw exactly when the value it compared
// contains every input the redraw reads.** Everything below is a station on that rule -- one
// input moved with the whole rest of the frame held identical, both directions asserted, and the
// answer read off the live element rather than off the panel's record of what it wrote.
//
// The exception is the last input of each pair: what is left of a cooldown takes a new value on
// every frame with nothing behind it, so a gate that held it would fire never, the window would
// be flushed every frame, and the screen would look perfect. It is drawn by a separate pass
// outside the gate, and both halves of that -- that the ring moves, and that without the pass it
// would not -- are asserted.
//
// **Four of the scenarios are about panels that are not inventory's** -- the vassal list, the
// spell components, the skills and the attributes. Their rows are `panels.redraw.*`; they sit
// here because they are stations on the same rule, and belong with `social`, `magic` and `ui`.
// =============================================================================================

use dereth_primitives::LocalTime;
use dereth_ui_screens::items::widget::child;
use dereth_ui_screens::panels::allegiance::{AllegiancePanel, ROW_EXPERIENCE, VASSAL_LIST};
use dereth_ui_screens::panels::attributes::AttributesPanel;
use dereth_ui_screens::panels::skills::{self, SkillsPanel};
use dereth_ui_screens::panels::spellcomponent::{self, SpellComponentPanel};
use dereth_ui_screens::toolbar::shortcuts::SLOT_COUNT;
use dereth_ui_screens::view::{
    AllegianceEntry, AllegianceRoster, ComponentCategory, ComponentRow, SkillEntry,
};

// =============================================================================================
// The ground under a drop, the shop's own latch, and the salvage window's drag-out.
// =============================================================================================

use dereth_client::character::{Character, CharacterInput};
use dereth_client::interaction::{self, Interaction};
use dereth_client::objects::ObjectStream;
use dereth_client::world::{load_region, DEFAULT_LANDBLOCK};
use dereth_client_model::Request;
use dereth_client_net::client_session::testing::{Corpus, MockTransport};
use dereth_client_net::client_session::Session;
use dereth_primitives::ServerTime;
use dereth_ui_screens::hud::speech_bubbles::LIST_BOX;
use dereth_ui_screens::items::widget::{drag_flags, inq_drop_icon_info};
use dereth_ui_screens::panels::salvage::{self, ButtonState as SalvageButton};
use dereth_ui_screens::view::{DropTarget, UiRequest};
use std::sync::Arc;

// =============================================================================================
// The shop's money, its category tabs, and the two lines about the row the player picked.
// =============================================================================================

use dereth_client_model::{RecordingRequests, RecordingSink, World};
use dereth_ui_screens::panels::vendor::{
    self, ATTR_SHOP_FILTER, BUY_LIST_TEXT, BUY_PURSE_TEXT, SELL_LIST_TEXT, SELL_PURSE_TEXT,
    TYPE_FILTER_MENU,
};

// =============================================================================================
// The drag: picking an icon up out of the pack and letting it go somewhere.
//
// These are written on `Player::Drag`, so the first thing they rest on is that the step's motion
// satisfies the shell's own drag threshold.
//
// **It does, and here is what the threshold is.** `UiSystem::mouse_move` arms a pending drag on
// the press and starts it only once `dx^2 + dy^2 > 15` -- about four pixels from the press
// point. `Player::Drag` delivers its motion in two moves rather than one, and the first of them
// is the midpoint between the two targets, so a drag between two live elements passes the
// threshold on the first move and arrives on the second. A single-move implementation would
// have passed here too, since the arrival point is also far from the origin; what actually
// makes the step work is that each move is delivered **while the button is down** and is
// followed by a frame, because the shell reads the pointer out of the input manager once per
// frame and a second move delivered before that frame would replace the first.
//
// What `Player::Drag` cannot do is say anything about the screen *between* the press and the
// release, which is what most of the scenarios below are about. Those are driven through
// `dereth_testkit::adapters_inventory`, the same gesture in three steps.
// =============================================================================================

use dereth_testkit::adapters_inventory::{Grab, Over, Release};
use dereth_testkit::{ScreenPoint, Target};

// =============================================================================================
// The hints a drag lights on what it is carried over.
//
// Three targets that are not ordinary item lists -- the toolbar's backpack button, the paper
// doll's body slots and the shortcut bar -- plus the hint coming down on a drop, and the sentence
// the client says when a worn thing is dropped back where it already is.
//
// Every hint is read off the live element the pointer hit-tested to, and every gesture is
// `Grab` / `Over` / `Drop`, so a hint that lit because a scenario raised a message by hand could
// not pass. Each scenario asserts its premise before it measures: the element under the pointer
// must be the one that catches this drag.
// =============================================================================================

use dereth_testkit::adapters_inventory::Drop as LetGo;
use dereth_ui::StateId;
use dereth_ui_screens::items::widget::drag_accept_state;
use dereth_ui_screens::toolbar::{INVENTORY_BUTTON, INVENTORY_DRAG_OVERLAY};

// =============================================================================================
// The figure itself: dropping something on the picture of your own character.
//
// Two halves of one thing, on one fixture: what the figure **says** while something is held
// over it, and what **happens** when it is let go.
//
// The asymmetry between them is the point of both, and it is one row each: a piece the player
// is already wearing gets no answer at all on the way over -- the figure neither offers nor
// refuses -- and is told, in so many words, when it is dropped.
// =============================================================================================

use dereth_ui_screens::panels::inventory::{
    paper_doll_drag_overlay, CANNOT_PUT_THAT_ITEM_THERE, PAPER_DOLL_DRAG_MASK,
};

// =============================================================================================
// The little number a shortcut puts on an item's picture.
//
// The nine shipped pictures the numbers are drawn from are not pinned by id: the behaviour
// underneath them -- that each slot draws its **own** number, so slot 5 does not look like
// slot 0 -- is asserted by comparing the two against each other instead. That catches
// everything an id table would, and nothing about it goes stale when the data does.
// =============================================================================================

use dereth_protocol::login::ShortCutData;

// =============================================================================================
// The chest on the ground: what a drop on it sends, and what identifying it says.
//
// Written on `dereth_testkit::adapters_inventory::given_recorded_chest`, which gets one recorded
// Holtburg chest open on screen in the harness, so the scenarios below say only what they are
// about.
// =============================================================================================

use dereth_testkit::adapters_inventory::{
    first_blob_where, given_recorded_chest, is_locked_appraisal_of, CHEST_SESSION, LOCKED_BOOL,
    RECORDED_CHEST, RECORDED_LOCKED_CHEST,
};

// =============================================================================================
// Handing something over, and the one request the client holds while it waits.
//
// The client has exactly one inventory request in flight at a time -- not one per object -- and
// nothing ever times it out. That is the specification and not a defect, so what the scenarios
// below have to prove is both halves: that the hold is really taken, and that every answer that can
// release it does.
//
// **A release-only scenario is worth nothing**, which is why each of these asserts the *held*
// state first, through the thing a player would actually notice -- a second gesture refused in
// the client's own words with nothing reaching the shard.
//
// The census scenario below reads the recording's own gives through `Outbound`: `Sent::field`
// reads a field out of a recorded client-to-server message, so a give's recipient and item are
// read off the wire rather than only counted.
// =============================================================================================

use dereth_client_model::inventory::requests::InventoryRequest;
use dereth_testkit::Outbound;

use dereth_ui_screens::panels::inventory::{InventoryPanels, PAPER_DOLL_SLOTS};

// =============================================================================================
// Clicking the picture of the character.
//
// A click on the paper doll selects the item worn there, without first switching the window to
// its grid of item slots.
//
// The grid of places is a list, so a click on one of its cells selects the ordinary way. The
// **picture** is not a list at all: which part of the body the pointer is over is read out of a
// painted picture shipped with the client, and a scenario whose coordinates were written down
// rather than found in that picture would be measuring nothing. So the first scenario below
// measures the shipped picture, and the two after it take their points from it.
// =============================================================================================

use dereth_client_model::objects::InventoryPlacement;
use dereth_ui_screens::panels::inventory::{ClickMap, HIT_TEST_COLOURS};

/// The shop, the rest, in three parts about one window. The window the shard's own message put
/// on screen -- whether its rows are drawn finished, whether they can be pressed, and what a drop
/// on its counter does. The strip of categories across the top of it, and where a row's *kind*
/// is read from. And the three things that happen between the category test and the row
/// appearing: the supply the basket has already taken, the stack a row is offered in, and a
/// container with something still in it.
pub(super) mod shop;

// =============================================================================================
// The secure-trade window, the rest of it: opening a negotiation, putting things on the table by
// hand, what the table says while an icon is in the air, and every way a trade ends.
//
// The trade scenarios above are about what the window *draws*; these are about what a player
// *does* to it -- the pointer, the two buttons, the shard's own messages -- so every scenario
// below drives the client's own drag, its own button press and its own request pipeline, and
// reads the answer off the live window.
// =============================================================================================

/// The rest of the secure-trade window: opening a negotiation, offering by hand, the acceptance
/// lights and the refusals in the client's own words; what each half of the table answers a
/// carried icon; and every way a trade ends and what each one tells the shard.
pub(super) mod trade_window;

/// **The box that asks first.**
///
/// Some uses destroy what they are used on, so the client asks before it sends anything: a box
/// with the thing's own name in it and two buttons. These scenarios are that box -- what it says,
/// what a No does, what a second one does while the first is up, what happens to one nobody
/// answers, and the two ways a use skips it altogether.
pub(super) mod confirm;
/// **Using one thing on another: which thing is armed, which is picked, and what the cursor says
/// while the second click is waiting.**
///
/// Every scenario here is a whole client over the retail data with one recording replayed into it
/// -- the player is standing in the Academy with his pack open and a shop window up -- and every
/// gesture goes in through the pointer at a tile's own centre.
pub(super) mod delivery;
/// **Letting something go over the world: the gift, the sale, the trade, the chest and the
/// ground.** An item can be given to an NPC by dropping it on them.
///
/// One release over the viewport arms the client's own pick and sends nothing; what the pick then
/// finds decides everything. These scenarios are that decision, arm by arm, on the host built in
/// this file for the world drop and extended for the give.
pub(super) mod give;
/// **Picking something up off the ground, giving it away, and what a use is refused with.**
///
/// An item on the ground can be picked up. There is no pickup message: the client picks a thing up
/// with the same `PutItemInContainer` a drag into the pack sends, and what makes it a pickup is the
/// use handler choosing that over a plain use for something lying loose in the world. Every
/// scenario below is about one link of that chain.
pub(super) mod pickup;
/// **The seven lines a use prints when it is refused.**
///
/// A click that cannot do anything says why: the reason is worked out and printed, rather than the
/// player pressing the button and the game saying nothing. Each scenario below is one arm of that
/// decision and the sentence it prints -- verbatim, because a scenario that read the sentence back
/// through the symbol that writes it could not catch a wrong one.
pub(super) mod use_refusal;

/// Equip and the paper doll: what happens when something is let go on a place on the figure, on
/// the picture of a weapon slot, or double-clicked in the pack.
///
/// All of it is the same gesture:
///
/// * the gate a place makes against the thing let go on it, the fork between asking with the
///   whole list of places and asking with one, the side that tells the left wrist from the right,
///   the off-hand, and the stack slider;
/// * what happens when the place is already full: the thing in the way goes to the pack and the
///   new one goes on when the shard says it landed;
/// * the same two things asked for by a double-click instead of a drag, and the one place the two
///   gestures are meant to disagree;
/// * the weapons that cannot share the hands with what is already held, and the stacks that merge
///   instead of unwielding;
/// * eight things the client does that each need a gesture to reach them: the hot keys, the
///   presses on a pack slot, the two buttons that arm the pointer, the drop into the world, the
///   shard's own answer to a move, the right button that turns the camera, and the box the player
///   types a quantity into.
pub(super) mod equip;

/// The stack split, the salvage window and the row a drop left behind.
///
/// The quantity a player chooses before dragging part of a stack away, the window a tinkering
/// tool opens, and the claim that a thing dropped on the ground really leaves the pack.
pub(super) mod split;

// =============================================================================================
// Decoration and the panels: the burden bar, the two little bars on a slot, the hover text and
// the number that is never painted; a thing that reaches the pack before it is described; the
// recharge the selection ring and the open-pack frame; what a click on a slot does; and a
// wielded thing lost to death.
//
// **Which reader each one uses, and why.** None of the slot claims is about reassembly, ordering
// or the connection sweep, so nothing here is written in `Inbound::from_raw_capture`. What a
// slot draws is read off the shipped panel driven by the production `update_inventory` /
// `update_shortcuts` / `do_item_heartbeat` passes, which is the same seam `Hud::drive` reaches
// and is an order of magnitude cheaper. The burden claim keeps the recording -- the character's
// own encumbrance is the oracle -- and reads it with `Inbound::from_corpus`. The death claim
// keeps the **real transport**, because it only holds when the delete travels the client's own
// dispatching route.
// =============================================================================================

/// The burden: the bar and the percentage on the pack page, what they are made of, and how the
/// bar is filled.
pub(super) mod burden;
/// What each button does to a slot, what opening a side pack does, what happens to the grid when
/// that pack leaves, and the numeral a shortcut tile withholds until its thing arrives.
pub(super) mod clicks;
/// A wielded thing that is also on the shortcut bar, lost to the corpse when the character dies.
///
/// **This one keeps the real transport.** The claim only holds when the shard's "that thing is
/// gone" travels the client's own dispatching route, and a message handed straight to the object
/// table raises no notice for the bar to hear -- so every message below is framed into real
/// datagrams by the harness's own shard and fed through the client's endpoint.
pub(super) mod death;
/// A thing that reaches the pack before the shard has described it, and what a second
/// description of a thing the client already has does to the pack's order.
pub(super) mod icons;
/// The recharge the selection ring and the frame on the pack that is open.
pub(super) mod overlays;
/// The one reader the two selection groups share: which slots anywhere on the pack page and the
/// shortcut bar are drawing `item`, and whether each of them has its ring up.
mod rings;
/// The slot: what a slot says on hover, the two little bars on it, the number that is never painted
/// on it, and the fact that decorating a slot leaves it exactly as clickable as it was.
pub(super) mod slots;

/// **A refused wield**: a Gear Knight drags an Academy Coat onto the paper doll, the figure
/// accepts it (neither retail client checks heritage armour), the shard refuses the wield with
/// `0x585`, and afterwards the shortcut keys and the use key must still use things.
///
/// Retail against the same refusal shows *"You are restricted to clothes and armor created for
/// your race."* and *"The Academy Coat can't be wielded"*, keeps the coat in the pack, and goes on
/// normally. Each refusal reason the attempt-failed message can carry is driven: the heritage
/// one, the reasonless one and a busy one, because all of them must let go of the
/// one-at-a-time hold the wield took.
#[test]
fn a_wield_the_shard_refuses_says_why_and_leaves_the_next_uses_working() {
    for (reason, why) in [
        (
            0x585,
            Some("You are restricted to clothes and armor created for your race."),
        ),
        (0x000, None),
        (0x01D, Some("You're too busy!")),
    ] {
        let mut c = a_dressed_client_with_food();
        // The boots' free place stands in for the coat's; only the name is the coat's.
        c.world_mut()
            .tables
            .weenies
            .get_mut(BOOTS)
            .expect("seeded")
            .pwd
            .name = "Academy Coat".to_owned();
        let tile = pack_slot(&mut c, BOOTS);
        let at = point_of(&mut c, tile);
        c.when(Player::Click(at));
        c.tick(3);
        assert_eq!(
            c.world_mut().selected,
            Some(BOOTS),
            "the coat is selected first"
        );

        let tile = pack_slot(&mut c, BOOTS);
        let before = strip_lines(&mut c).len();
        let (_, sent) = drop_on_the_figure(&mut c, tile);
        assert_eq!(
            sent.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![BOOTS],
            "{reason:#x}"
        );
        assert!(
            !c.world_mut().request_lock.is_idle(),
            "the wield takes the hold"
        );

        c.when(Inbound::message(
            &dereth_protocol::objects::CharacterServerSaysAttemptFailed {
                object: BOOTS,
                reason,
            },
        ));
        c.tick(3);
        let lines: Vec<String> = strip_lines(&mut c).into_iter().skip(before).collect();
        let mut want: Vec<String> = why.map(str::to_owned).into_iter().collect();
        want.push(match reason {
            0x01D => "The Academy Coat can't be wielded - you're too busy".to_owned(),
            _ => "The Academy Coat can't be wielded".to_owned(),
        });
        assert_eq!(lines, want, "what the player is told for {reason:#x}");
        {
            let w = c.world_mut();
            assert!(
                w.request_lock.is_idle(),
                "the refusal lets go of the hold ({reason:#x})"
            );
            let coat = w.weenie(BOOTS).expect("seeded");
            assert!(!coat.waiting, "the coat un-greys");
            assert_eq!(
                coat.pwd.container_id,
                Some(DOLL_PLAYER),
                "and stays in the pack"
            );
            assert_eq!(coat.pwd.wielder_id, None, "unworn");
        }

        // The shortcut key for the apple's tile.
        c.tick(20);
        let from = c.outbound().len();
        press_tile_key(&mut c, 2, winit::keyboard::KeyCode::Digit3);
        let used: Vec<ObjectId> = c.outbound()[from..]
            .iter()
            .filter_map(|r| match r {
                Request::UseEvent(m) => Some(m.object),
                _ => None,
            })
            .collect();
        assert_eq!(
            used,
            vec![APPLE],
            "the shortcut key still uses its tile ({reason:#x})"
        );

        // The use key on the selected apple.
        c.tick(20);
        c.world_mut().selected = Some(APPLE);
        c.tick(1);
        let from = c.outbound().len();
        press_the_use_key(&mut c);
        let used: Vec<ObjectId> = c.outbound()[from..]
            .iter()
            .filter_map(|r| match r {
                Request::UseEvent(m) => Some(m.object),
                _ => None,
            })
            .collect();
        assert_eq!(
            used,
            vec![APPLE],
            "the use key still uses the selection ({reason:#x})"
        );
        clear_requests(c.ui_outbox());
        c.shutdown();
    }
}

/// **The make-shortcut key.** With the pear selected it puts the pear in the first empty slot of
/// the bar and tells the shard (`0x019C`); pressed again, the pear already has a shortcut, and the
/// player is told so and nothing is sent.
#[test]
fn the_make_shortcut_key_puts_the_selection_on_the_bar() {
    const MAKE_SHORTCUT: dereth_input::ActionId = dereth_input::ActionId(0x1000_010D);
    let mut c = a_dressed_client_with_food();
    c.world_mut().selected = Some(PEAR);
    c.tick(1);
    let from = c.outbound().len();
    c.when(Player::Press(MAKE_SHORTCUT));
    c.tick(3);
    let added: Vec<(i32, ObjectId)> = c.outbound()[from..]
        .iter()
        .filter_map(|r| match r {
            Request::AddShortCut(m) => Some((m.shortcut.index, m.shortcut.object_id)),
            _ => None,
        })
        .collect();
    assert_eq!(added, vec![(0, PEAR)], "slot 0 is the first empty one");
    assert_eq!(c.world_mut().player_system.shortcut_at(0), Some(PEAR));
    {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        assert_eq!(
            screen.shortcuts.item_at(0),
            Some(PEAR),
            "and the tile shows it"
        );
    }

    let before = strip_lines(&mut c).len();
    let from = c.outbound().len();
    c.when(Player::Press(MAKE_SHORTCUT));
    c.tick(3);
    assert!(
        !c.outbound()[from..]
            .iter()
            .any(|r| matches!(r, Request::AddShortCut(_))),
        "nothing more is sent"
    );
    let lines: Vec<String> = strip_lines(&mut c).into_iter().skip(before).collect();
    assert_eq!(
        lines,
        vec!["There is already a shortcut to the Pear".to_owned()]
    );
    clear_requests(c.ui_outbox());
    c.shutdown();
}

mod client_support;
use client_support::{a_client, a_gameplay_client, a_retail_client};

mod container_moves;
use container_moves::{
    a_cancelled_drag_out_of_the_chest_leaves_no_grey_row, a_chest_row_with_no_object_behind_it,
    a_chest_with_something_in_it, a_client_at_the_open_chest,
    a_destination_with_room_draws_one_grey_row_in_the_cell_aimed_at,
    a_drag_out_of_the_open_chest_moves_it_to_the_pack_and_keeps_the_mark,
    a_drop_aimed_at_a_side_pack_draws_no_waiting_row,
    a_drop_on_the_backpack_button_draws_the_waiting_row_at_the_head,
    a_drop_on_the_chests_own_row_goes_into_it_just_the_same,
    a_drop_on_the_open_chest_puts_the_thing_in_it_once,
    a_drop_the_client_refuses_for_itself_draws_no_waiting_row,
    a_full_destination_draws_no_row_and_the_move_spills_to_a_side_pack,
    a_full_destination_with_nowhere_to_spill_says_so_and_sends_nothing,
    a_hook_the_player_does_not_own_refuses_the_drop_in_words,
    a_refusal_takes_the_waiting_row_away_and_un_greys_the_source,
    a_refused_move_out_of_the_chest_clears_it_and_lets_the_next_one_go,
    a_second_drop_on_a_different_list_meets_the_one_request_hold,
    a_second_drop_on_the_same_list_names_what_is_already_being_placed,
    a_second_thing_into_the_chest, arm_the_grid, chest_tile, chest_tile_ghosted,
    identifying_a_locked_container_says_so_and_how_hard_the_lock_is, object_waiting,
    once_the_waiting_row_is_gone_the_same_drop_is_taken_again, own_cell_in_the_clear,
    pack_cell_in_the_clear, pending_row, recorded_player, sent_since, something_to_drag,
    the_shards_own_message_puts_the_shop_on_screen_and_not_only_its_tabs,
    the_shards_unlock_reaches_the_chest_and_the_relock_follows_it,
    the_tile_a_drop_lands_on_clears_its_own_grey_mark,
    the_waiting_row_is_drawn_at_once_and_the_answer_replaces_it, Pick,
};

mod containment;
use containment::{
    a_holders_own_list_decides_what_it_holds, a_stale_or_unknown_stow_changes_nothing,
    a_stowed_item_leaves_the_hand_and_the_world,
};

mod drag_gestures;
use drag_gestures::{
    a_client_with_a_loose_item_and_a_side_pack, a_drop_on_a_side_pack_becomes_one_move,
    a_pack_on_the_item_grid_is_refused_for_its_kind,
    a_pack_slot_answers_the_pointer_at_its_own_centre,
    a_plain_thing_on_an_empty_pack_strip_slot_is_refused_for_its_kind,
    a_press_and_a_move_lift_the_items_own_picture,
    a_real_drag_across_the_grid_asks_for_the_place_the_player_aimed_at,
    a_refused_container_drop_says_so_in_its_own_words,
    a_refused_drag_takes_the_mark_off_the_tile_as_well,
    a_refused_world_drop_names_the_thing_as_the_player_sees_it,
    a_row_with_nothing_behind_it_does_not_keep_a_mark_it_was_given,
    an_attuned_thing_says_so_in_its_description, an_empty_slot_and_a_forbidden_list_start_nothing,
    dragging_yourself_into_your_own_pack_is_refused_by_the_list_itself, strip_slot, DRAG_ITEM,
    DRAG_PACK, DRAG_PLAYER,
};

mod drag_marks;
use drag_marks::{
    a_destroyed_thing_leaves_no_tile_and_no_mark, a_drag_that_ends_over_nothing_takes_the_mark_off,
    a_drop_on_the_shortcut_bar_takes_the_mark_off, a_drop_that_moves_nothing_takes_the_mark_off,
    a_full_pack_is_named_the_way_the_player_would_name_it,
    a_refused_backpack_drop_sends_nothing_and_takes_the_mark_off,
    a_tile_clears_its_own_before_catching_another_thing, every_tile_mirrors_the_things_own_state,
    the_backpack_button_sends_one_move_and_keeps_the_mark,
    the_shards_two_answers_both_take_the_mark_off,
};

mod drop_hints;
use drop_hints::{
    a_body_slot_shows_whether_it_could_take_what_is_carried,
    a_hook_in_nobodys_house_refuses_everything_carried_over_it,
    a_hook_shows_whether_it_could_take_what_is_carried,
    a_pack_goes_into_the_chest_through_its_own_strip,
    a_pack_over_the_contents_lights_green_and_is_still_refused_on_the_drop,
    a_shortcut_tile_offers_itself_for_anything_carried, carry_over, doll_item, doll_tile,
    dropping_a_worn_thing_back_on_its_own_slot_says_so,
    letting_go_on_a_tile_takes_its_own_hint_down, shipped, shortcut_tile, strip_lines,
    the_backpack_button_lights_while_something_is_over_it,
    the_open_chest_lights_the_row_the_carried_thing_is_over,
    the_windows_own_row_answers_as_a_list_of_packs_would, DOLL_SHIELD_SLOT, DOLL_WEAPON_SLOT,
};

mod drop_readiness;
use drop_readiness::{
    a_give_in_mid_air_is_not_refused, a_world_drop_is_refused_in_mid_air,
    the_ground_answer_is_read_again_every_frame, BUSY, FEEDBACK_CHANNEL,
};

mod give_receipts;
use give_receipts::{
    a_container_being_viewed_does_not_release_a_give,
    a_give_takes_the_hold_and_the_next_gesture_is_refused_in_words, every_recorded_give,
    every_recorded_give_is_answered_by_the_move_and_nothing_else,
    part_of_a_stack_is_released_by_the_stacks_new_count,
    the_shards_move_releases_the_give_and_the_next_one_goes_out,
    the_shards_refusal_releases_the_give_and_says_which_thing, EVERY_SESSION,
};

mod input_hits;
use input_hits::{
    a_client_with_three_things_and_a_pack,
    a_real_drag_puts_the_hint_up_on_what_it_crosses_and_takes_it_down_again,
    every_live_slot_carries_the_overlay_the_hint_is_drawn_on, grid_cell,
    no_cursor_the_client_ships_is_chosen_by_a_drag, seed_three_things_and_a_pack, strip_cell,
    the_crossing_is_reported_as_a_leave_and_an_enter_in_that_order,
    the_crossing_message_is_not_the_focus_message,
    the_drop_is_aimed_at_the_element_the_cursor_was_over, HINT_DRAG_ITEMS, HINT_DRAG_PACK,
    HINT_DRAG_PLAYER,
};

mod paper_doll;
use paper_doll::{
    a_click_on_a_bare_region_of_the_picture_selects_the_player,
    a_click_on_the_picture_selects_what_is_worn_in_that_region, a_dressed_client_with_food,
    a_wearable_dropped_on_the_figure_is_put_on_with_its_own_places,
    an_occupied_place_names_what_is_in_the_way,
    asking_for_the_grid_of_places_takes_the_picture_out_of_the_hit_test, drop_on_the_figure,
    equipment_destination, every_place_on_the_figure_is_a_place_to_wear_and_no_cell_of_a_pack_is,
    every_place_the_figure_is_filled_from_answers_a_drop_with_its_own_place,
    something_already_worn_is_told_so_when_it_is_dropped,
    something_already_worn_leaves_the_figure_saying_nothing,
    the_figure_says_whether_it_could_take_what_is_carried,
    the_figures_answer_comes_down_on_leave_and_on_the_drop,
    the_picture_of_the_character_is_a_twenty_fifth_place_to_let_something_go,
    the_same_drag_is_a_wear_on_the_figure_and_a_move_into_a_pack,
    the_shipped_picture_of_the_character_is_painted_for_every_region,
    what_cannot_be_worn_at_all_is_refused_in_words, APPLE, BOOTS, DOLL_PLAYER, PEAR,
};

mod request_support;
use request_support::{a_host_with_a_recipient, a_settled_body, is_drop, is_give, jump, DropHost};

mod row_updates;
use row_updates::{
    a_drop_is_drawn_before_the_shard_answers, a_plain_inventory_refusal_leaves_the_row,
    a_shard_failure_takes_the_optimistic_row_off,
    a_skill_row_follows_the_enchanted_number_and_the_penalty,
    a_stack_in_vendor_stock_is_counted_and_pluralised,
    a_stack_on_the_trade_table_is_counted_and_pluralised,
    a_stock_row_follows_a_late_picture_and_a_new_price, a_vassals_own_fields_redraw_the_row,
    a_withdrawal_names_a_side, an_item_on_the_table_wears_the_traded_mark,
    an_owned_component_count_redraws_its_row, bound_vendor,
    each_sides_total_counts_only_its_own_side, every_trade_element_is_in_the_live_tree,
    shop_with_stack, the_attribute_rows_compare_the_text_they_write,
    the_light_and_the_button_are_two_different_things, the_partner_name_follows_the_partner_alone,
    the_partners_side_follows_a_row_filled_in_later, the_tooltip_reaches_the_slot_itself,
    the_trade_table_follows_every_frozen_fact_and_not_the_clock,
    the_vendor_numeral_is_the_advertised_amount,
    the_vendor_stock_follows_every_frozen_fact_and_not_the_clock,
    your_own_name_is_bound_and_never_written,
};

mod salvage_drag;
use salvage_drag::{
    a_refusal_naming_the_list_starts_nothing, a_refused_drag_still_takes_the_salvage_row_off,
    a_salvage_row_dragged_out_leaves_the_list, open_pack, pack_slot, seed_salvage, use_the_tool,
    SALVAGE_PLAYER, SALVAGE_RING, SALVAGE_TOOL,
};

mod shortcut_actions;
use shortcut_actions::{
    a_client_with_shortcuts, a_kit_key_then_the_main_pack_key_uses_the_kit_on_the_player,
    a_number_key_pressed_with_a_cursor_armed_finishes_that_gesture,
    a_pack_can_be_put_on_a_tile_like_anything_else,
    a_thing_dropped_on_a_tile_fills_it_and_tells_the_shard_once,
    a_thing_pushed_off_its_tile_keeps_its_shortcut_in_the_next_free_slot,
    an_assigned_thing_draws_its_slot_number_wherever_its_picture_appears,
    dragging_a_shortcut_off_the_bar_empties_its_tile_and_tells_the_shard,
    dragging_a_shortcut_tile_to_tile_moves_it_and_tells_the_shard_both_halves, press_tile_key,
    the_magic_stance_dims_the_number_everywhere,
    the_number_follows_the_assignment_off_and_on_again,
    the_number_is_drawn_over_the_busy_mark_and_the_ring,
    the_number_key_of_a_filled_tile_uses_what_is_in_it, NUM_PACK, NUM_PLAYER, NUM_ROCK, WORN_SHIRT,
};

mod slot_redraw;
use slot_redraw::{
    a_move_request_ghosts_its_own_item_at_once, a_pack_slot_follows_every_word_it_draws,
    a_tile_follows_every_thing_it_draws, the_bar_ring_counts_down_while_the_gate_is_shut,
    the_eighteen_tiles_fill_and_hold_still, the_grid_follows_the_containers_own_capacity,
    the_pack_ring_counts_down_while_the_gate_is_shut,
    the_side_strip_follows_the_players_own_capacity, Bar, GEM, OIL,
};

mod target_use;
use target_use::{
    press_the_use_key, the_shipped_key_carries_the_picked_stacks_own_count_into_the_merge,
    the_shipped_key_puts_what_is_picked_into_the_players_own_pack,
    the_shipped_key_with_nothing_picked_sends_nothing,
    the_shipped_use_key_raises_the_question_and_the_two_answers_differ,
};

mod ui_support;
use ui_support::{
    centre, centre_of, clear_requests, click, element_of, gameplay_root, gameplay_screen, parts,
    point_of, press, pump, take_requests, text_of,
};

mod vendor_activity;
use vendor_activity::{
    a_basket_is_counted_in_things_and_not_in_rows, a_client_at_the_shop,
    a_pack_let_go_on_the_window_offers_what_is_inside_it_and_not_itself,
    a_purchase_leaves_the_player_able_to_act,
    a_purse_that_came_only_from_the_login_can_buy_what_it_can_afford,
    a_sale_leaves_the_player_able_to_act,
    a_stale_coin_update_for_the_player_is_dropped_by_the_same_counter,
    an_empty_pack_is_offered_as_itself, bubbles, click_shop_tab, closing_with_a_basket_asks_first,
    coming_back_to_the_stock_tab_draws_every_row_decorated_again, dword,
    each_money_line_follows_its_own_source, feedback,
    letting_go_takes_the_shop_hint_down_whichever_answer_it_was_showing, sellable,
    something_carried_over_the_window_turns_it_to_the_selling_tab,
    something_offered_to_the_shop_wears_the_mark_in_both_windows, stock_slots,
    the_filter_strip_follows_the_stock,
    the_first_recorded_shop_opens_on_the_purse_the_login_description_carried,
    the_money_lines_are_five_different_places, the_picked_stock_row_writes_its_name_and_its_cost,
    the_purse_is_filled_from_the_players_own_coin, the_recorded_grocer,
    the_shop_says_whether_it_would_buy_what_is_carried_over_it,
    the_shop_shows_the_players_own_coin_and_follows_it,
    while_the_shop_has_not_answered_the_refusal_still_fires, Shop, BTN_SELL_ALL, INVENTORY_PAGE,
    RECORDED_PLAYER, SELL_LIST, SELL_TAB, SHOPKEEPER,
};
