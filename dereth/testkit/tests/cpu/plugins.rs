//! Plugins: the spec's Core and UI conformance cases, each run by this client's driver over a
//! replayed recording, a scripted world or frames alone, with no data file, and the plugin audit's
//! answer with plugins loaded.
//!
//! A case is a folder under `docs/plugins/conformance/<profile>/`: a test plugin and the
//! `expect.toml` it is checked against (see `dereth_testkit::conformance`). A scenario runs its
//! cases and asserts its row once every case held.

use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::frame_hook::{audit_reply, AuditReply, FrameHook};
use dereth_plugin_host::{Package, PluginHook};
use dereth_protocol::admin::{AdminQueryPlugin, AdminQueryPluginList};
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::conformance::{assert_case, CASES_DIR};
use dereth_testkit::plugins::{read_package, PluginRun};

/// The Trackers sample: Mag-Tools' trackers.
pub(super) mod trackers;

/// The Buffbot sample: buffs for other players on request.
pub(crate) mod buffbot;

/// The Inventory sample: Mag-Tools' inventory tools, and the suits it builds from them.
pub(crate) mod inventory;

pub(crate) mod navigator;

/// The samples' windows: what each command changes, in the window too.
pub(crate) mod windows;

/// The Tank sample against VTank's documented behavior.
pub(crate) mod tank;

/// The Tank sample's looting, items and trading against VTank's documented behavior.
pub(crate) mod tank_items;

/// The Tank sample's following, fellows, summoning and locks against VTank's documented behavior.
pub(crate) mod tank_fellows;

/// The Tank sample's quest steps, its journey with the macro stopped, and its Academy profile.
pub(crate) mod tank_quests;

/// The Tank sample's buffbot trips and its corpse recovery after a death.
pub(crate) mod tank_buffbot;

/// The Tank sample's windows and profiles: its meta editor, hotkeys, remote control, Items and
/// Buffs pages, and VTank settings profiles' items, consumables and buffs.
pub(crate) mod tank_windows;

/// Scripts written for UtilityBelt's scripting, through the host's library of its script API.
pub(crate) mod ubscript;

/// The author's tool's plugin template: its plugin runs.
pub(crate) mod template;

dereth_testkit::scenarios! {
    scenario_a_click_loots_a_corpse_one_awaited_request_at_a_time => a_click_loots_a_corpse_one_awaited_request_at_a_time ["plugins.actions.a-click-loots-a-corpse-one-awaited-request-at-a-time"],
    scenario_a_trade_and_a_salvage_end_at_the_servers_word => a_trade_and_a_salvage_end_at_the_servers_word ["plugins.actions.a-trade-and-a-salvage-end-at-the-servers-word"],
    scenario_a_refusal_of_something_the_character_said_settles_no_action => a_refusal_of_something_the_character_said_settles_no_action ["plugins.actions.a-refusal-of-something-the-character-said-settles-no-action"],
    scenario_a_refusal_of_the_action_itself_fails_it_with_its_reason => a_refusal_of_the_action_itself_fails_it_with_its_reason ["plugins.actions.a-refusal-of-the-action-itself-fails-it-with-its-reason"],
    scenario_an_item_used_on_a_target_sends_one_use_with_target_and_a_wielded_caster_stays_in_hand => an_item_used_on_a_target_sends_one_use_with_target_and_a_wielded_caster_stays_in_hand ["plugins.actions.an-item-used-on-a-target-sends-one-use-with-target-and-a-wielded-caster-stays-in-hand"],
    scenario_an_appraisal_settles_its_promise_with_what_it_told => an_appraisal_settles_its_promise_with_what_it_told ["plugins.actions.an-appraisal-settles-its-promise-with-what-it-told"],
    scenario_without_a_gesture_an_action_needs_its_domain_and_the_worlds_leave => without_a_gesture_an_action_needs_its_domain_and_the_worlds_leave ["plugins.actions.without-a-gesture-an-action-needs-its-domain-and-the-worlds-leave"],
    scenario_the_audit_names_each_loaded_plugin_and_the_permissions_its_manifest_names => the_audit_names_each_loaded_plugin_and_the_permissions_its_manifest_names ["plugins.audit.the-audit-names-each-loaded-plugin-and-the-permissions-its-manifest-names"],
    scenario_a_plugin_hears_each_line_and_prints_its_own_locally => a_plugin_hears_each_line_and_prints_its_own_locally ["plugins.chat.a-plugin-hears-each-line-and-prints-its-own-locally"],
    scenario_a_plugins_own_line_is_never_taken_for_a_command => a_plugins_own_line_is_never_taken_for_a_command ["plugins.chat.a-plugins-own-line-is-never-taken-for-a-command"],
    scenario_a_plugin_hides_incoming_lines => a_plugin_hides_incoming_lines ["plugins.chat.a-plugin-hides-incoming-lines"],
    scenario_a_line_reaches_a_plugin_as_the_player_reads_it => a_line_reaches_a_plugin_as_the_player_reads_it ["plugins.chat.a-line-reaches-a-plugin-as-the-player-reads-it"],
    scenario_a_claimed_command_reaches_its_plugin_and_the_clients_own_win => a_claimed_command_reaches_its_plugin_and_the_clients_own_win ["plugins.commands.a-claimed-command-reaches-its-plugin-and-the-clients-own-win"],
    scenario_state_changes_arrive_in_the_order_the_client_applied_them => state_changes_arrive_in_the_order_the_client_applied_them ["plugins.events.state-changes-arrive-in-the-order-the-client-applied-them"],
    scenario_a_failing_or_runaway_plugin_is_suspended => a_failing_or_runaway_plugin_is_suspended ["plugins.faults.a-failing-or-runaway-plugin-is-suspended-and-the-client-carries-on"],
    scenario_a_found_plugin_is_listed_off_until_the_player_ticks_it => a_found_plugin_is_listed_off_until_the_player_ticks_it ["plugins.load.a-found-plugin-is-listed-off-until-the-player-ticks-it"],
    scenario_a_plugin_that_needs_what_the_host_lacks_is_refused => a_plugin_that_needs_what_the_host_lacks_is_refused ["plugins.load.a-plugin-that-needs-what-the-host-lacks-is-refused-with-the-reason"],
    scenario_a_plugin_reaches_nothing_but_its_package_and_the_host => a_plugin_reaches_nothing_but_its_package_and_the_host ["plugins.sandbox.a-plugin-reaches-nothing-but-its-package-and-the-host"],
    scenario_the_character_store_opens_at_login_and_keeps_plain_values => the_character_store_opens_at_login_and_keeps_plain_values ["plugins.store.the-character-store-opens-at-login-and-keeps-plain-values"],
    scenario_the_global_world_and_character_stores_keep_values_within_their_quota => the_global_world_and_character_stores_keep_values_within_their_quota ["plugins.store.the-global-world-and-character-stores-keep-values-within-their-quota"],
    scenario_timers_and_sleeping_tasks_run_on_the_frame_clock => timers_and_sleeping_tasks_run_on_the_frame_clock ["plugins.tasks.timers-and-sleeping-tasks-run-on-the-frame-clock"],
    scenario_the_looter_keeps_what_its_profile_keeps_and_id_notices_reports_it => the_looter_keeps_what_its_profile_keeps_and_id_notices_reports_it ["plugins.samples.the-looter-keeps-what-its-profile-keeps-and-id-notices-reports-it"],
    scenario_a_declared_window_takes_clicks_and_changes => a_declared_window_takes_clicks_and_changes ["plugins.ui.a-declared-window-takes-clicks-and-changes"],
    scenario_tabs_a_number_and_a_choice_take_the_players_changes => tabs_a_number_and_a_choice_take_the_players_changes ["plugins.ui.tabs-a-number-and-a-choice-take-the-players-changes"],
    scenario_the_tank_confirms_an_item_buff_by_the_items_appraisal => the_tank_confirms_an_item_buff_by_the_items_appraisal ["plugins.samples.the-tank-confirms-an-item-buff-by-the-items-appraisal"],
    scenario_the_tank_casts_a_buff_again_until_its_effect_shows => the_tank_casts_a_buff_again_until_its_effect_shows ["plugins.samples.the-tank-casts-a-buff-again-until-its-effect-shows"],
    scenario_the_tank_buffs_at_the_level_its_skill_reaches_and_climbs_as_it_rises => the_tank_buffs_at_the_level_its_skill_reaches_and_climbs_as_it_rises ["plugins.samples.the-tank-buffs-at-the-level-its-skill-reaches-and-climbs-as-it-rises"],
    scenario_the_plugin_bar_shows_each_plugin_and_toggles_its_main_window => the_plugin_bar_shows_each_plugin_and_toggles_its_main_window ["plugins.ui.the-plugin-bar-shows-each-plugin-and-toggles-its-main-window"],
    scenario_a_resizable_window_takes_the_players_size_within_its_bounds => a_resizable_window_takes_the_players_size_within_its_bounds ["plugins.ui.a-resizable-window-takes-the-players-size-within-its-bounds"],
    scenario_a_node_stretches_across_its_container_unless_it_keeps_its_own_size => a_node_stretches_across_its_container_unless_it_keeps_its_own_size ["plugins.ui.a-node-stretches-across-its-container-unless-it-keeps-its-own-size"],
    scenario_growing_nodes_share_the_room_left_in_proportion_within_their_bounds => growing_nodes_share_the_room_left_in_proportion_within_their_bounds ["plugins.ui.growing-nodes-share-the-room-left-in-proportion-within-their-bounds"],
    scenario_where_nothing_says_the_last_list_or_text_box_grows => where_nothing_says_the_last_list_or_text_box_grows ["plugins.ui.where-nothing-says-the-last-list-or-text-box-grows"],
    scenario_a_lists_columns_are_fixed_or_share_its_width => a_lists_columns_are_fixed_or_share_its_width ["plugins.ui.a-lists-columns-are-fixed-or-share-its-width"],
    scenario_a_window_shorter_than_its_nodes_scrolls_through_them => a_window_shorter_than_its_nodes_scrolls_through_them ["plugins.ui.a-window-shorter-than-its-nodes-scrolls-through-them"],
    scenario_with_tabs_only_the_pages_content_scrolls => with_tabs_only_the_pages_content_scrolls ["plugins.ui.with-tabs-only-the-pages-content-scrolls"],
    scenario_while_the_huds_are_unlocked_a_huds_controls_take_no_input => while_the_huds_are_unlocked_a_huds_controls_take_no_input ["plugins.ui.while-the-huds-are-unlocked-a-huds-controls-take-no-input"],
    scenario_the_spell_book_is_read_whole_or_a_page_at_a_time => the_spell_book_is_read_whole_or_a_page_at_a_time ["plugins.game.the-spell-book-is-read-whole-or-a-page-at-a-time"],
    scenario_a_hud_is_kept_with_its_anchor_and_changes_like_a_window => a_hud_is_kept_with_its_anchor_and_changes_like_a_window ["plugins.ui.a-hud-is-kept-with-its-anchor-and-changes-like-a-window"],
    scenario_markers_are_kept_with_what_they_mark_and_go_when_removed_or_due => markers_are_kept_with_what_they_mark_and_go_when_removed_or_due ["plugins.overlay.markers-are-kept-with-what-they-mark-and-go-when-removed-or-due"],
    scenario_an_image_shows_a_png_from_the_package_or_one_fetched => an_image_shows_a_png_from_the_package_or_one_fetched ["plugins.ui.an-image-shows-a-png-from-the-package-or-one-fetched"],
    scenario_requests_reach_only_the_listed_domains_over_https => requests_reach_only_the_listed_domains_over_https ["plugins.net.requests-reach-only-the-listed-domains-over-https"],
    scenario_a_published_policy_sets_each_domains_rule_and_denies_the_rest => a_published_policy_sets_each_domains_rule_and_denies_the_rest ["plugins.policy.a-published-policy-sets-each-domains-rule-and-denies-the-rest"],
    scenario_a_plugin_that_names_no_permission_may_do_what_each_covers => a_plugin_that_names_no_permission_may_do_what_each_covers ["plugins.permissions.a-plugin-that-names-no-permission-may-do-what-each-covers"],
    scenario_a_world_with_no_policy_allows_every_kind_of_automation => a_world_with_no_policy_allows_every_kind_of_automation ["plugins.policy.a-world-with-no-policy-allows-every-kind-of-automation"],
    scenario_a_move_turns_toward_the_place_runs_and_stops_within_range => a_move_turns_toward_the_place_runs_and_stops_within_range ["plugins.nav.a-move-turns-toward-the-place-runs-and-stops-within-range"],
    scenario_movement_without_a_gesture_needs_its_domain_and_the_worlds_leave => movement_without_a_gesture_needs_its_domain_and_the_worlds_leave ["plugins.nav.movement-without-a-gesture-needs-its-domain-and-the-worlds-leave"],
    scenario_a_follow_holds_within_range_until_stopped => a_follow_holds_within_range_until_stopped ["plugins.nav.a-follow-holds-within-range-until-stopped"],
    scenario_a_held_motion_presses_a_movement_control_for_the_time_asked => a_held_motion_presses_a_movement_control_for_the_time_asked ["plugins.nav.a-held-motion-presses-a-movement-control-for-the-time-asked"],
    scenario_a_jump_faces_its_heading_charges_for_the_time_asked_and_leaps_the_way_held => a_jump_faces_its_heading_charges_for_the_time_asked_and_leaps_the_way_held ["plugins.nav.a-jump-faces-its-heading-charges-for-the-time-asked-and-leaps-the-way-held"],
    scenario_a_vtank_route_is_walked_point_by_point => a_vtank_route_is_walked_point_by_point ["plugins.nav.a-vtank-route-is-walked-point-by-point"],
    scenario_a_journey_reaches_a_dungeon_through_the_portal_that_leads_there => a_journey_reaches_a_dungeon_through_the_portal_that_leads_there ["plugins.nav.a-journey-reaches-a-dungeon-through-the-portal-that-leads-there"],
    scenario_a_refused_portal_is_learned_and_the_journey_planned_again => a_refused_portal_is_learned_and_the_journey_planned_again ["plugins.nav.a-refused-portal-is-learned-and-the-journey-planned-again"],
    scenario_a_plan_answers_the_journey_without_moving => a_plan_answers_the_journey_without_moving ["plugins.nav.a-plan-answers-the-journey-without-moving"],
    scenario_a_walking_route_goes_round_walls_and_through_doors_and_a_line_of_sight_stops_at_them => a_walking_route_goes_round_walls_and_through_doors_and_a_line_of_sight_stops_at_them ["plugins.nav.a-walking-route-goes-round-walls-and-through-doors-and-a-line-of-sight-stops-at-them"],
    scenario_a_journey_needs_its_domain_and_ends_when_stopped_or_interrupted => a_journey_needs_its_domain_and_ends_when_stopped_or_interrupted ["plugins.nav.a-journey-needs-its-domain-and-ends-when-stopped-or-interrupted"],
    scenario_a_journey_is_drawn_until_the_plugin_hides_it => a_journey_is_drawn_until_the_plugin_hides_it ["plugins.overlay.a-journey-is-drawn-until-the-plugin-hides-it"],
    scenario_the_tank_fights_by_its_monster_list_with_debuffs_and_spells => the_tank_fights_by_its_monster_list_with_debuffs_and_spells ["plugins.samples.the-tank-fights-by-its-monster-list-with-debuffs-and-spells"],
    scenario_the_tank_travels_to_its_hunting_place_and_hunts_there => the_tank_travels_to_its_hunting_place_and_hunts_there ["plugins.samples.the-tank-travels-to-its-hunting-place-and-hunts-there"],
    scenario_the_tank_is_driven_by_a_vtank_meta => the_tank_is_driven_by_a_vtank_meta ["plugins.samples.the-tank-is-driven-by-a-vtank-meta"],
    scenario_the_tank_reads_and_writes_vtank_settings_profiles => the_tank_reads_and_writes_vtank_settings_profiles ["plugins.samples.the-tank-reads-and-writes-vtank-settings-profiles"],
    scenario_the_tank_keeps_its_buffs_up => the_tank_keeps_its_buffs_up ["plugins.samples.the-tank-keeps-its-buffs-up"],
    scenario_the_tank_takes_the_selected_item_for_a_slot => the_tank_takes_the_selected_item_for_a_slot ["plugins.samples.the-tank-takes-the-selected-item-for-a-slot"],
    scenario_the_tank_pauses_a_domain_it_may_not_automate_and_says_why_once => the_tank_pauses_a_domain_it_may_not_automate_and_says_why_once ["plugins.samples.the-tank-pauses-a-domain-it-may-not-automate-and-says-why-once"],
    scenario_the_tank_recharges_by_vtanks_lists_and_refills_worn_mana => the_tank_recharges_by_vtanks_lists_and_refills_worn_mana ["plugins.samples.the-tank-recharges-by-vtanks-lists-and-refills-worn-mana"],
    scenario_the_tank_fights_loots_its_kill_and_walks_its_route => the_tank_fights_loots_its_kill_and_walks_its_route ["plugins.samples.the-tank-fights-loots-its-kill-and-walks-its-route"],
    scenario_the_tank_makes_a_route_in_the_game_and_walks_it_jumping_and_through_its_portal => the_tank_makes_a_route_in_the_game_and_walks_it_jumping_and_through_its_portal ["plugins.samples.the-tank-makes-a-route-in-the-game-and-walks-it-jumping-and-through-its-portal"],
    scenario_the_tank_answers_vtanks_route_and_jump_commands => the_tank_answers_vtanks_route_and_jump_commands ["plugins.samples.the-tank-answers-vtanks-route-and-jump-commands"],
    scenario_a_plugin_is_active_only_in_the_parts_of_the_session_it_names => a_plugin_is_active_only_in_the_parts_of_the_session_it_names ["plugins.pregame.a-plugin-is-active-only-in-the-parts-of-the-session-it-names"],
    scenario_a_plugin_picks_a_character_and_retries_a_refused_entry => a_plugin_picks_a_character_and_retries_a_refused_entry ["plugins.pregame.a-plugin-picks-a-character-and-retries-a-refused-entry"],
    scenario_a_world_that_denies_session_control_refuses_a_plugins_entry => a_world_that_denies_session_control_refuses_a_plugins_entry ["plugins.pregame.a-world-that-denies-session-control-refuses-a-plugins-entry"],
    scenario_a_plugin_hears_its_copies_in_the_other_clients => a_plugin_hears_its_copies_in_the_other_clients ["plugins.peers.a-plugin-hears-its-copies-in-the-other-clients"],
    scenario_a_plugin_fights_through_the_combat_controls => a_plugin_fights_through_the_combat_controls ["plugins.automation.a-plugin-fights-through-the-combat-controls"],
    scenario_a_player_killers_automation_stops_where_the_world_says => a_player_killers_automation_stops_where_the_world_says ["plugins.automation.a-player-killers-automation-stops-where-the-world-says"],
    scenario_read_parity_limits_a_search_to_the_radar_and_the_pack => read_parity_limits_a_search_to_the_radar_and_the_pack ["plugins.automation.read-parity-limits-a-search-to-the-radar-and-the-pack"],
    scenario_a_meta_runs_its_states_rules_on_a_timer => a_meta_runs_its_states_rules_on_a_timer ["plugins.automation.a-meta-runs-its-states-rules-on-a-timer"],
    scenario_portal_space_and_death_are_heard_in_order => portal_space_and_death_are_heard_in_order ["plugins.automation.portal-space-and-death-are-heard-in-order"],
    scenario_a_watchdog_moves_a_still_meta_on => a_watchdog_moves_a_still_meta_on ["plugins.automation.a-watchdog-moves-a-still-meta-on"],
    scenario_expressions_evaluate_in_vtanks_language => expressions_evaluate_in_vtanks_language ["plugins.automation.expressions-evaluate-in-vtanks-language"],
    scenario_a_vtank_meta_file_imports_and_runs_in_vtanks_order => a_vtank_meta_file_imports_and_runs_in_vtanks_order ["plugins.automation.a-vtank-meta-file-imports-and-runs-in-vtanks-order"],
    scenario_a_vtank_meta_file_read_as_data_writes_back_as_vtank_writes_it => a_vtank_meta_file_read_as_data_writes_back_as_vtank_writes_it ["plugins.automation.a-vtank-meta-file-read-as-data-writes-back-as-vtank-writes-it"],
    scenario_stopping_and_letting_go_are_always_allowed => stopping_and_letting_go_are_always_allowed ["plugins.automation.stopping-and-letting-go-are-always-allowed"],
    scenario_stopping_an_attack_cancels_it_and_fails_its_promise => stopping_an_attack_cancels_it_and_fails_its_promise ["plugins.automation.stopping-an-attack-cancels-it-and-fails-its-promise"],
    scenario_a_heal_is_its_own_domain_and_a_debuff_is_cast_once_per_target => a_heal_is_its_own_domain_and_a_debuff_is_cast_once_per_target ["plugins.automation.a-heal-is-its-own-domain-and-a-debuff-is-cast-once-per-target"],
    scenario_beneficial_spells_on_another_character_are_their_own_domain => beneficial_spells_on_another_character_are_their_own_domain ["plugins.automation.beneficial-spells-on-another-character-are-their-own-domain"],
    scenario_a_compiled_plugin_counts_what_the_same_lua_plugin_counts => a_compiled_plugin_counts_what_the_same_lua_plugin_counts ["plugins.packages.a-compiled-plugin-counts-what-the-same-lua-plugin-counts"],
    scenario_the_players_own_movement_ends_a_plugins_movement_and_comes_first => the_players_own_movement_ends_a_plugins_movement_and_comes_first ["plugins.nav.the-players-own-movement-ends-a-plugins-movement-and-comes-first"],
    scenario_a_plugins_movement_gives_way_when_the_server_takes_control => a_plugins_movement_gives_way_when_the_server_takes_control ["plugins.nav.a-plugins-movement-gives-way-when-the-server-takes-control"],
    scenario_an_unloaded_or_suspended_plugin_lets_go_of_the_character => an_unloaded_or_suspended_plugin_lets_go_of_the_character ["plugins.nav.an-unloaded-or-suspended-plugin-lets-go-of-the-character"],
    scenario_an_attack_settles_on_what_the_server_does_with_it => an_attack_settles_on_what_the_server_does_with_it ["plugins.automation.an-attack-settles-on-what-the-server-does-with-it"],
    scenario_an_attacks_swing_goes_only_at_its_target => an_attacks_swing_goes_only_at_its_target ["plugins.automation.an-attacks-swing-goes-only-at-its-target"],
    scenario_a_package_loads_whatever_signature_file_it_carries => a_package_loads_whatever_signature_file_it_carries ["plugins.packages.a-package-loads-whatever-signature-file-it-carries"],
    scenario_the_navigator_points_at_a_place_and_walks_there => the_navigator_points_at_a_place_and_walks_there ["plugins.samples.the-navigator-points-at-a-place-and-walks-there"],
    scenario_auto_login_enters_the_chosen_character_and_retries_while_it_is_still_in_the_world => auto_login_enters_the_chosen_character_and_retries_while_it_is_still_in_the_world ["plugins.samples.auto-login-enters-the-chosen-character-and-retries"],
    scenario_follow_brings_another_client_to_the_leader => follow_brings_another_client_to_the_leader ["plugins.samples.follow-brings-another-client-to-the-leader"],
    scenario_a_plugin_reads_each_message_decoded_and_can_send_none => a_plugin_reads_each_message_decoded_and_can_send_none ["plugins.messages.a-plugin-reads-each-message-decoded-and-can-send-none"],
    scenario_a_plugin_hears_objects_change_and_move_and_finds_them_by_their_properties => a_plugin_hears_objects_change_and_move_and_finds_them_by_their_properties ["plugins.objects.a-plugin-hears-objects-change-and-move-and-finds-them-by-their-properties"],
    scenario_a_plugin_reads_the_characters_fellowship_titles_augmentations_bars_and_options => a_plugin_reads_the_characters_fellowship_titles_augmentations_bars_and_options ["plugins.character.a-plugin-reads-the-characters-fellowship-titles-augmentations-bars-and-options"],
    scenario_utilitybelts_additions_to_the_expression_language_evaluate_as_documented => utilitybelts_additions_to_the_expression_language_evaluate_as_documented ["plugins.automation.utilitybelts-additions-to-the-expression-language-evaluate-as-documented"],
    scenario_a_plugin_draws_lines_rings_squares_text_and_nametags_in_the_world => a_plugin_draws_lines_rings_squares_text_and_nametags_in_the_world ["plugins.overlay.a-plugin-draws-lines-rings-squares-text-and-nametags-in-the-world"],
    scenario_a_canvas_and_a_chart_draw_what_a_plugin_gives_them => a_canvas_and_a_chart_draw_what_a_plugin_gives_them ["plugins.ui.a-canvas-and-a-chart-draw-what-a-plugin-gives-them"],
    scenario_text_inputs_sliders_rich_list_cells_tooltips_and_toasts_work_as_declared => text_inputs_sliders_rich_list_cells_tooltips_and_toasts_work_as_declared ["plugins.ui.text-inputs-sliders-rich-list-cells-tooltips-and-toasts-work-as-declared"],
    scenario_a_hotkey_runs_its_plugins_command_as_a_gesture => a_hotkey_runs_its_plugins_command_as_a_gesture ["plugins.ui.a-hotkey-runs-its-plugins-command-as-a-gesture"],
    scenario_a_plugin_calls_a_service_another_plugin_offers => a_plugin_calls_a_service_another_plugin_offers ["plugins.services.a-plugin-calls-a-service-another-plugin-offers"],
    scenario_a_file_the_player_picks_reaches_the_plugin_and_nothing_else_does => a_file_the_player_picks_reaches_the_plugin_and_nothing_else_does ["plugins.files.a-file-the-player-picks-reaches-the-plugin-and-nothing-else-does"],
    scenario_a_plugin_hides_and_moves_the_games_panels_and_lets_go_when_it_unloads => a_plugin_hides_and_moves_the_games_panels_and_lets_go_when_it_unloads ["plugins.ui.a-plugin-hides-and-moves-the-games-panels-and-lets-go-when-it-unloads"],
    scenario_a_mark_may_stay_on_top_of_what_stands_in_front_and_nametags_keep_to_the_nearest => a_mark_may_stay_on_top_of_what_stands_in_front_and_nametags_keep_to_the_nearest ["plugins.overlay.a-mark-may-stay-on-top-of-what-stands-in-front-and-nametags-keep-to-the-nearest"],
    scenario_a_press_on_the_map_is_heard_where_it_falls => a_press_on_the_map_is_heard_where_it_falls ["plugins.overlay.a-press-on-the-map-is-heard-where-it-falls"],
}

fn core(case: &str, id: &'static str) {
    assert_case("core", case);
    note_asserted(id);
}

/// A plugin audit message as the session hands it over: its type dword, then its payload.
fn ui_event<M: dereth_protocol::Message>(m: &M) -> SessionEvent {
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(M::OPCODE.0);
    m.write(&mut w).expect("the audit query writes");
    SessionEvent::UiEvent {
        opcode: M::OPCODE,
        blob: w.into_inner(),
    }
}

fn the_audit_names_each_loaded_plugin_and_the_permissions_its_manifest_names() {
    let dir = dereth_testkit::conformance::workspace_root()
        .join(CASES_DIR)
        .join("core/chat-filter");
    let mut run = PluginRun::new();
    run.load(read_package(&dir));
    let hook = PluginHook {
        host: run.into_host(),
    };
    let list = audit_reply(
        &ui_event(&AdminQueryPluginList { context: 7 }),
        Some(&hook as &dyn FrameHook),
    );
    let one = audit_reply(
        &ui_event(&AdminQueryPlugin {
            context: 8,
            plugin_name: "conformance.chatfilter".into(),
        }),
        Some(&hook as &dyn FrameHook),
    );
    let listed = matches!(&list, Some(AuditReply::List(r)) if r.context == 7 && r.plugin_list == "conformance.chatfilter 0.1.0");
    let described = matches!(&one, Some(AuditReply::One(r))
        if r.context == 8 && r.success == 1
            && r.name == "Conformance: chat-filter 0.1.0 (conformance.chatfilter)"
            && r.email == "permissions: chat.filter");
    assert!(listed && described, "list {list:?}, one {one:?}");
    note_asserted(
        "plugins.audit.the-audit-names-each-loaded-plugin-and-the-permissions-its-manifest-names",
    );
}

fn a_plugin_hears_each_line_and_prints_its_own_locally() {
    core(
        "chat-lines",
        "plugins.chat.a-plugin-hears-each-line-and-prints-its-own-locally",
    );
}

fn a_plugin_hides_incoming_lines() {
    core("chat-filter", "plugins.chat.a-plugin-hides-incoming-lines");
}

fn a_plugins_own_line_is_never_taken_for_a_command() {
    core(
        "chat-send",
        "plugins.chat.a-plugins-own-line-is-never-taken-for-a-command",
    );
}

/// A system line and a fellowship line holding web addresses, a Dereth Network channel's line,
/// the world's echo of the player's own line to the channel, and a tell from a character, heard
/// by a plugin naming no permission: each line as its kind and its text.
fn a_line_reaches_a_plugin_as_the_player_reads_it() {
    use dereth_client_model::chat::text_type;
    use dereth_primitives::ObjectId;
    use dereth_protocol::comms::{
        CommunicationChannelBroadcastRecv, CommunicationHearDirectSpeech,
        CommunicationTextboxString,
    };
    let me = ObjectId(dereth_testkit::plugins::SCRIPTED_PLAYER);
    let tell = |sender: u32, name: &str, message: &str| {
        ui_event(&CommunicationHearDirectSpeech {
            message: message.into(),
            sender_name: name.into(),
            sender_id: ObjectId(sender),
            target_id: me,
            text_type: text_type::SPEECH_DIRECT,
            secret_flags: 0,
        })
    };
    let mut run = PluginRun::new();
    run.load(Package {
        origin: "listener".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.listener\"\nname = \"Listener\"\nversion = \"1.0.0\"\napi = \"0.7\"\n"
                    .to_vec(),
            ),
            (
                "main.lua".to_owned(),
                b"on('chat.line', function(e) print(e.kind .. '|' .. e.text) end)\n".to_vec(),
            ),
        ]
        .into(),
    });
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    run.server_says(&[
        ui_event(&CommunicationTextboxString {
            text: "Notes at https://example.com/notes.".into(),
            text_type: 0,
        }),
        ui_event(&CommunicationChannelBroadcastRecv {
            channel: 0x800,
            sender_name: "Alba".into(),
            message: "see (https://example.com/a?b=1)".into(),
        }),
        // A network channel's line, from its stand-in id, and the world's echo of the player's.
        tell(
            0x6000_0007,
            "group@channel",
            "Person@world: meet at https://example.com/b",
        ),
        ui_event(&CommunicationTextboxString {
            text: "You tell group@channel, \"on my way\"".into(),
            text_type: text_type::SPEECH_DIRECT_SEND,
        }),
        tell(0x5000_0002, "Alba", "a private word"),
    ]);
    run.advance(0.2);
    assert_eq!(
        run.console("test.listener"),
        [
            "default|Notes at https://example.com/notes.",
            "fellowship|[Fellowship] <Tell:IIDString:0:Alba>Alba<\\Tell> says, \"see (https://example.com/a?b=1)\"",
            "channel|[group] <Tell:IIDString:0:Person@world>Person@world<\\Tell> says, \"meet at https://example.com/b\"",
            "channel_sent|[group] You say, \"on my way\"",
            "tell|<Tell:IIDString:1342177282:Alba>Alba<\\Tell> tells you, \"a private word\"",
        ],
        "the addresses as written, the channel's lines as channel lines, and the tell as a tell"
    );
    note_asserted("plugins.chat.a-line-reaches-a-plugin-as-the-player-reads-it");
}

fn a_claimed_command_reaches_its_plugin_and_the_clients_own_win() {
    core(
        "commands",
        "plugins.commands.a-claimed-command-reaches-its-plugin-and-the-clients-own-win",
    );
}

fn state_changes_arrive_in_the_order_the_client_applied_them() {
    core(
        "events-order",
        "plugins.events.state-changes-arrive-in-the-order-the-client-applied-them",
    );
}

fn a_failing_or_runaway_plugin_is_suspended() {
    assert_case("core", "faults");
    core(
        "budget",
        "plugins.faults.a-failing-or-runaway-plugin-is-suspended-and-the-client-carries-on",
    );
}

fn a_plugin_that_needs_what_the_host_lacks_is_refused() {
    core(
        "refused",
        "plugins.load.a-plugin-that-needs-what-the-host-lacks-is-refused-with-the-reason",
    );
}

fn a_plugin_reaches_nothing_but_its_package_and_the_host() {
    core(
        "sandbox",
        "plugins.sandbox.a-plugin-reaches-nothing-but-its-package-and-the-host",
    );
}

fn the_character_store_opens_at_login_and_keeps_plain_values() {
    core(
        "store",
        "plugins.store.the-character-store-opens-at-login-and-keeps-plain-values",
    );
}

fn timers_and_sleeping_tasks_run_on_the_frame_clock() {
    core(
        "timers",
        "plugins.tasks.timers-and-sleeping-tasks-run-on-the-frame-clock",
    );
}

fn a_declared_window_takes_clicks_and_changes() {
    core(
        "ui-window",
        "plugins.ui.a-declared-window-takes-clicks-and-changes",
    );
}

fn a_click_loots_a_corpse_one_awaited_request_at_a_time() {
    core(
        "loot-by-hand",
        "plugins.actions.a-click-loots-a-corpse-one-awaited-request-at-a-time",
    );
}

fn a_trade_and_a_salvage_end_at_the_servers_word() {
    core(
        "trade-and-salvage",
        "plugins.actions.a-trade-and-a-salvage-end-at-the-servers-word",
    );
}

/// An action's promise settles on the answer to that action. After a vendor's window closes, the
/// server refuses a tell the character sent (the one told is not there: a vendor answered by
/// tell) while a use, a buy and a give are each waiting for their own answer: the refusal is the
/// tell's, and each still ends as the server answers it.
fn a_refusal_of_something_the_character_said_settles_no_action() {
    use dereth_protocol::comms::CommunicationWeenieError;
    use dereth_rules::weenie::item_type;
    use dereth_testkit::plugins::{ScriptedObject, ScriptedVendor, SCRIPTED_PLAYER};
    const VENDOR: u32 = 0x8000_0D00;
    const TAPER: u32 = 0x8000_0D01;
    const BREAD: u32 = 0x8000_0D02;
    const GIFT: u32 = 0x8000_0D03;
    const FRIEND: u32 = 0x8000_0D04;
    /// `CharacterNotAvailable`: the one a tell was sent to is not there.
    const NOT_THERE: u32 = 0x052B;
    let main = format!(
        r#"
local function act_on(name, p)
	task.spawn(function()
		print(name, pawait(p))
	end)
end
commands.add("open", "", function() act_on("open", act.use({VENDOR})) end)
commands.add("close", "", function() act_on("close", vendor.close()) end)
commands.add("buy", "", function() act_on("buy", vendor.buy({{ {{ id = {TAPER}, count = 5 }} }})) end)
commands.add("eat", "", function() act_on("eat", act.use({BREAD})) end)
commands.add("give", "", function() act_on("give", items.give({GIFT}, {FRIEND})) end)
"#
    );
    let package = Package {
        origin: "answers".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.answers\"\nname = \"Answers\"\nversion = \"1.0.0\"\napi = \"0.7\"\n\n[permissions]\n\"act.interactive\" = \"Acts when typed to.\"\n"
                    .to_vec(),
            ),
            ("main.lua".to_owned(), main.into_bytes()),
        ]
        .into(),
    };
    let mut run = PluginRun::new();
    run.load(package);
    run.scripted_world(
        "Tester",
        vec![
            ScriptedObject {
                id: VENDOR,
                name: "Tester the Shopkeeper".into(),
                obj_type: item_type::CREATURE,
                place: Some((42.2, 33.80001)),
                vendor: Some(ScriptedVendor { buys: 0 }),
                ..ScriptedObject::default()
            },
            ScriptedObject {
                id: TAPER,
                name: "Prismatic Taper".into(),
                obj_type: item_type::SPELL_COMPONENTS,
                container: Some(VENDOR),
                value: 5,
                max_stack: Some(1000),
                ..ScriptedObject::default()
            },
            ScriptedObject {
                id: BREAD,
                name: "Bread".into(),
                obj_type: item_type::FOOD,
                container: Some(SCRIPTED_PLAYER),
                ..ScriptedObject::default()
            },
            ScriptedObject {
                id: GIFT,
                name: "Token".into(),
                obj_type: item_type::MISC,
                container: Some(SCRIPTED_PLAYER),
                ..ScriptedObject::default()
            },
            ScriptedObject {
                id: FRIEND,
                name: "Training Master".into(),
                obj_type: item_type::CREATURE,
                place: Some((42.2, 33.80002)),
                ..ScriptedObject::default()
            },
        ],
    );
    run.set_coins(1000);
    run.advance(0.5);
    let not_there = || {
        dereth_testkit::plugins::ui_event(&CommunicationWeenieError {
            error_type: NOT_THERE,
        })
    };
    run.type_line("/open");
    run.advance(1.0);
    run.type_line("/close");
    run.advance(1.0);
    // The vendor used again, a buy, a use and a give: each sent, and a frame later the tell's
    // refusal, before its own answer.
    for line in ["/open", "/buy", "/eat", "/give"] {
        run.type_line(line);
        run.advance(1.0 / 30.0);
        run.server_says(&[not_there()]);
        run.advance(2.0);
    }
    assert_eq!(
        run.console("test.answers"),
        ["open", "close", "open", "buy", "eat", "give"]
            .map(|name| format!("{name}\ttrue\tnil"))
            .to_vec()
    );
    note_asserted("plugins.actions.a-refusal-of-something-the-character-said-settles-no-action");
}

/// A refusal of the action itself still fails it, with the reason the server gave: a buy refused
/// naming the item bought, or refused whole (the server's failed save of the character, its reason
/// with the acknowledgement); a sale refused naming the item sold; a use on a target refused
/// naming the target; a trade add refused naming the item; a cast refused outright.
fn a_refusal_of_the_action_itself_fails_it_with_its_reason() {
    use dereth_protocol::comms::CommunicationWeenieError;
    use dereth_protocol::objects::{CharacterServerSaysAttemptFailed, ItemUseDone};
    use dereth_protocol::trade::{TradeRegisterTrade, TradeTradeFailure};
    use dereth_rules::weenie::item_type;
    use dereth_testkit::plugins::{ui_event, ScriptedObject, ScriptedVendor, SCRIPTED_PLAYER};
    const VENDOR: u32 = 0x8000_0D10;
    const TAPER: u32 = 0x8000_0D11;
    const DAGGER: u32 = 0x8000_0D12;
    const KIT: u32 = 0x8000_0D13;
    const GIFT: u32 = 0x8000_0D14;
    const FRIEND: u32 = 0x8000_0D15;
    const WAND: u32 = 0x8000_0D16;
    let main = format!(
        r#"
local fight = require("dereth.combat")
local function act_on(name, p)
	task.spawn(function()
		print(name, pawait(p))
	end)
end
commands.add("open", "", function() act_on("open", act.use({VENDOR})) end)
commands.add("buy", "", function() act_on("buy", vendor.buy({{ {{ id = {TAPER}, count = 5 }} }})) end)
commands.add("sell", "", function() act_on("sell", vendor.sell({{ {DAGGER} }})) end)
commands.add("close", "", function() act_on("close", vendor.close()) end)
commands.add("useon", "", function() act_on("useon", act.use_on({KIT}, {FRIEND})) end)
commands.add("trade", "", function() act_on("trade", trade.add({GIFT})) end)
commands.add("magic", "", function() task.spawn(function() print("magic", fight.ready_magic()) end) end)
commands.add("cast", "", function() act_on("cast", magic.cast(1)) end)
"#
    );
    let package = Package {
        origin: "own".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.own\"\nname = \"Own\"\nversion = \"1.0.0\"\napi = \"0.7\"\n\n[permissions]\n\"act.interactive\" = \"Acts when typed to.\"\n"
                    .to_vec(),
            ),
            ("main.lua".to_owned(), main.into_bytes()),
        ]
        .into(),
    };
    let held = |id: u32, name: &str, obj_type: u32| ScriptedObject {
        id,
        name: name.into(),
        obj_type,
        container: Some(SCRIPTED_PLAYER),
        value: 50,
        ..ScriptedObject::default()
    };
    let mut run = PluginRun::new();
    run.load(package);
    run.scripted_world(
        "Tester",
        vec![
            ScriptedObject {
                id: VENDOR,
                name: "Tester the Shopkeeper".into(),
                obj_type: item_type::CREATURE,
                place: Some((42.2, 33.80001)),
                vendor: Some(ScriptedVendor {
                    buys: item_type::MELEE_WEAPON,
                }),
                ..ScriptedObject::default()
            },
            ScriptedObject {
                id: TAPER,
                name: "Prismatic Taper".into(),
                obj_type: item_type::SPELL_COMPONENTS,
                container: Some(VENDOR),
                value: 5,
                max_stack: Some(1000),
                ..ScriptedObject::default()
            },
            held(DAGGER, "Rusty Dagger", item_type::MELEE_WEAPON),
            held(KIT, "Healing Kit", item_type::MISC),
            held(GIFT, "Token", item_type::MISC),
            ScriptedObject {
                locations: Some(0x0100_0000),
                ..held(WAND, "Wand", item_type::CASTER)
            },
            ScriptedObject {
                id: FRIEND,
                name: "Friend".into(),
                obj_type: item_type::CREATURE,
                place: Some((42.2, 33.80002)),
                ..ScriptedObject::default()
            },
        ],
    );
    run.set_coins(1000);
    run.advance(0.5);
    let attempt = |object: u32, reason: u32| {
        ui_event(&CharacterServerSaysAttemptFailed {
            object: dereth_primitives::ObjectId(object),
            reason,
        })
    };
    // Each action sent, and a frame later its refusal, before anything else answers it.
    let act = |run: &mut PluginRun, line: &str, refusal: Vec<_>| {
        run.type_line(line);
        run.advance(1.0 / 30.0);
        if !refusal.is_empty() {
            run.server_says(&refusal);
        }
        run.advance(2.0);
    };
    act(&mut run, "/open", Vec::new());
    act(&mut run, "/buy", vec![attempt(TAPER, 0x002A)]);
    act(
        &mut run,
        "/buy",
        vec![
            attempt(SCRIPTED_PLAYER, 0),
            ui_event(&ItemUseDone {
                failure_type: 0x001D,
            }),
        ],
    );
    act(&mut run, "/sell", vec![attempt(DAGGER, 0x002B)]);
    act(&mut run, "/useon", vec![attempt(FRIEND, 0x002C)]);
    run.server_says(&[ui_event(&TradeRegisterTrade {
        initiator: dereth_primitives::ObjectId(SCRIPTED_PLAYER),
        partner: dereth_primitives::ObjectId(FRIEND),
        stamp: 1.0,
    })]);
    act(
        &mut run,
        "/trade",
        vec![ui_event(&TradeTradeFailure {
            item: dereth_primitives::ObjectId(GIFT),
            reason: 0x002D,
        })],
    );
    act(&mut run, "/magic", Vec::new());
    act(
        &mut run,
        "/cast",
        vec![ui_event(&CommunicationWeenieError { error_type: 0x0401 })],
    );
    assert_eq!(
        run.console("test.own"),
        [
            "open\ttrue\tnil",
            "buy\tfalse\trefused (0x002A)",
            "buy\tfalse\trefused (0x001D)",
            "sell\tfalse\trefused (0x002B)",
            "useon\tfalse\trefused (0x002C)",
            "trade\tfalse\trefused (0x002D)",
            "magic\ttrue\tnil",
            "cast\tfalse\trefused (0x0401)",
        ]
        .map(str::to_owned)
        .to_vec()
    );
    note_asserted("plugins.actions.a-refusal-of-the-action-itself-fails-it-with-its-reason");
}

fn an_item_used_on_a_target_sends_one_use_with_target_and_a_wielded_caster_stays_in_hand() {
    core(
        "use-on",
        "plugins.actions.an-item-used-on-a-target-sends-one-use-with-target-and-a-wielded-caster-stays-in-hand",
    );
}

fn an_appraisal_settles_its_promise_with_what_it_told() {
    core(
        "appraise",
        "plugins.actions.an-appraisal-settles-its-promise-with-what-it-told",
    );
}

fn without_a_gesture_an_action_needs_its_domain_and_the_worlds_leave() {
    core(
        "gestures",
        "plugins.actions.without-a-gesture-an-action-needs-its-domain-and-the-worlds-leave",
    );
}

fn the_global_world_and_character_stores_keep_values_within_their_quota() {
    core(
        "store-scopes",
        "plugins.store.the-global-world-and-character-stores-keep-values-within-their-quota",
    );
}

fn tabs_a_number_and_a_choice_take_the_players_changes() {
    assert_case("ui", "controls");
    note_asserted("plugins.ui.tabs-a-number-and-a-choice-take-the-players-changes");
}

fn the_plugin_bar_shows_each_plugin_and_toggles_its_main_window() {
    assert_case("ui", "bar");
    note_asserted("plugins.ui.the-plugin-bar-shows-each-plugin-and-toggles-its-main-window");
}

fn a_resizable_window_takes_the_players_size_within_its_bounds() {
    assert_case("ui", "resize");
    note_asserted("plugins.ui.a-resizable-window-takes-the-players-size-within-its-bounds");
}

fn a_node_stretches_across_its_container_unless_it_keeps_its_own_size() {
    assert_case("ui", "layout-stretch");
    note_asserted("plugins.ui.a-node-stretches-across-its-container-unless-it-keeps-its-own-size");
}

fn growing_nodes_share_the_room_left_in_proportion_within_their_bounds() {
    assert_case("ui", "layout-grow");
    assert_case("ui", "layout-bounds");
    note_asserted("plugins.ui.growing-nodes-share-the-room-left-in-proportion-within-their-bounds");
}

fn where_nothing_says_the_last_list_or_text_box_grows() {
    assert_case("ui", "layout-auto");
    note_asserted("plugins.ui.where-nothing-says-the-last-list-or-text-box-grows");
}

fn a_lists_columns_are_fixed_or_share_its_width() {
    assert_case("ui", "layout-columns");
    note_asserted("plugins.ui.a-lists-columns-are-fixed-or-share-its-width");
}

fn a_window_shorter_than_its_nodes_scrolls_through_them() {
    assert_case("ui", "layout-scroll");
    note_asserted("plugins.ui.a-window-shorter-than-its-nodes-scrolls-through-them");
}

fn with_tabs_only_the_pages_content_scrolls() {
    assert_case("ui", "layout-tabs-scroll");
    note_asserted("plugins.ui.with-tabs-only-the-pages-content-scrolls");
}

fn while_the_huds_are_unlocked_a_huds_controls_take_no_input() {
    assert_case("ui", "hud-unlock");
    note_asserted("plugins.ui.while-the-huds-are-unlocked-a-huds-controls-take-no-input");
}

fn the_spell_book_is_read_whole_or_a_page_at_a_time() {
    core(
        "spell-book",
        "plugins.game.the-spell-book-is-read-whole-or-a-page-at-a-time",
    );
}

fn a_hud_is_kept_with_its_anchor_and_changes_like_a_window() {
    assert_case("ui", "hud");
    note_asserted("plugins.ui.a-hud-is-kept-with-its-anchor-and-changes-like-a-window");
}

/// The sample plugins that ship with the client, together in a scripted world: each loads, and the
/// looter, clicked with a corpse selected, opens it and keeps the pyreals and the gem its profile
/// keeps and leaves the dagger, while ID Notices appraises what the corpse held and reports the
/// valuable gem.
fn the_looter_keeps_what_its_profile_keeps_and_id_notices_reports_it() {
    use dereth_testkit::plugins::{ScriptedObject, SCRIPTED_PLAYER};
    const CORPSE: u32 = 0x8000_0100;
    let samples = dereth_testkit::conformance::workspace_root().join("dereth/plugins");
    let mut run = PluginRun::new();
    run.load(read_package(&samples.join("looter")));
    run.load(read_package(&samples.join("id-notices")));
    run.load(read_package(&samples.join("trackers")));
    let item = |id: u32, name: &str, obj_type: u32, value: u32| ScriptedObject {
        id,
        name: name.into(),
        obj_type,
        container: Some(CORPSE),
        value,
        ..ScriptedObject::default()
    };
    use dereth_rules::weenie::{bitfield, item_type};
    run.scripted_world(
        "Tester",
        vec![
            ScriptedObject {
                id: CORPSE,
                name: "Corpse of a Drudge".into(),
                obj_type: item_type::CONTAINER,
                bitfield: bitfield::CORPSE | bitfield::OPENABLE | bitfield::STUCK,
                ..ScriptedObject::default()
            },
            item(0x8000_0101, "Pyreal", item_type::MONEY, 100),
            item(0x8000_0102, "Opal", item_type::GEM, 6000),
            item(0x8000_0103, "Rusty Dagger", item_type::MELEE_WEAPON, 5),
        ],
    );
    run.select(CORPSE);
    for id in [
        "network.dereth.looter",
        "network.dereth.idnotices",
        "network.dereth.trackers",
    ] {
        assert_eq!(run.status(id), "running", "{id}: {:?}", run.console(id));
    }
    run.click("network.dereth.looter", "main.loot");
    run.advance(4.0);
    let kept = [0x8000_0101, 0x8000_0102].map(|i| run.container_of(i));
    let left = run.container_of(0x8000_0103);
    let printed = run.printed();
    let ui = run.ui_dump();
    assert_eq!(
        kept,
        [Some(SCRIPTED_PLAYER); 2],
        "the pyreals and the gem are in the pack (console {:?})",
        run.console("network.dereth.looter")
    );
    assert_eq!(left, Some(CORPSE), "the dagger stays in the corpse");
    assert!(
        printed.iter().any(|l| l == "[ID] Opal: worth 6000"),
        "ID Notices reported the gem: {printed:?}"
    );
    assert!(
        ui.contains("status text=\"Kept 2 of 3.\""),
        "the looter's window says what it kept:\n{ui}"
    );
    note_asserted(
        "plugins.samples.the-looter-keeps-what-its-profile-keeps-and-id-notices-reports-it",
    );
}

fn markers_are_kept_with_what_they_mark_and_go_when_removed_or_due() {
    assert_case("overlay", "markers");
    note_asserted(
        "plugins.overlay.markers-are-kept-with-what-they-mark-and-go-when-removed-or-due",
    );
}

fn an_image_shows_a_png_from_the_package_or_one_fetched() {
    assert_case("ui", "image");
    note_asserted("plugins.ui.an-image-shows-a-png-from-the-package-or-one-fetched");
}

fn requests_reach_only_the_listed_domains_over_https() {
    assert_case("core", "net");
    assert_case("core", "net-unnamed");
    note_asserted("plugins.net.requests-reach-only-the-listed-domains-over-https");
}

fn a_published_policy_sets_each_domains_rule_and_denies_the_rest() {
    assert_case("core", "policy");
    note_asserted("plugins.policy.a-published-policy-sets-each-domains-rule-and-denies-the-rest");
}

fn a_move_turns_toward_the_place_runs_and_stops_within_range() {
    assert_case("nav", "move-to");
    note_asserted("plugins.nav.a-move-turns-toward-the-place-runs-and-stops-within-range");
}

fn movement_without_a_gesture_needs_its_domain_and_the_worlds_leave() {
    assert_case("nav", "automated");
    note_asserted("plugins.nav.movement-without-a-gesture-needs-its-domain-and-the-worlds-leave");
}

fn a_follow_holds_within_range_until_stopped() {
    assert_case("nav", "follow");
    note_asserted("plugins.nav.a-follow-holds-within-range-until-stopped");
}

fn a_held_motion_presses_a_movement_control_for_the_time_asked() {
    assert_case("nav", "motion");
    note_asserted("plugins.nav.a-held-motion-presses-a-movement-control-for-the-time-asked");
}

fn a_jump_faces_its_heading_charges_for_the_time_asked_and_leaps_the_way_held() {
    assert_case("nav", "jump");
    note_asserted(
        "plugins.nav.a-jump-faces-its-heading-charges-for-the-time-asked-and-leaps-the-way-held",
    );
}

fn a_vtank_route_is_walked_point_by_point() {
    assert_case("nav", "route");
    note_asserted("plugins.nav.a-vtank-route-is-walked-point-by-point");
}

fn a_journey_reaches_a_dungeon_through_the_portal_that_leads_there() {
    assert_case("nav", "journey");
    note_asserted("plugins.nav.a-journey-reaches-a-dungeon-through-the-portal-that-leads-there");
}

fn a_refused_portal_is_learned_and_the_journey_planned_again() {
    assert_case("nav", "journey-refused");
    note_asserted("plugins.nav.a-refused-portal-is-learned-and-the-journey-planned-again");
}

fn a_plan_answers_the_journey_without_moving() {
    assert_case("nav", "journey-plan");
    note_asserted("plugins.nav.a-plan-answers-the-journey-without-moving");
}

fn a_walking_route_goes_round_walls_and_through_doors_and_a_line_of_sight_stops_at_them() {
    assert_case("nav", "path");
    note_asserted(
        "plugins.nav.a-walking-route-goes-round-walls-and-through-doors-and-a-line-of-sight-stops-at-them",
    );
}

fn a_journey_needs_its_domain_and_ends_when_stopped_or_interrupted() {
    assert_case("nav", "journey-automated");
    assert_case("nav", "journey-stop");
    note_asserted("plugins.nav.a-journey-needs-its-domain-and-ends-when-stopped-or-interrupted");
}

fn a_journey_is_drawn_until_the_plugin_hides_it() {
    assert_case("overlay", "journey");
    assert_case("overlay", "journey-hidden");
    note_asserted("plugins.overlay.a-journey-is-drawn-until-the-plugin-hides-it");
}

fn a_plugin_is_active_only_in_the_parts_of_the_session_it_names() {
    assert_case("pregame", "contexts");
    note_asserted("plugins.pregame.a-plugin-is-active-only-in-the-parts-of-the-session-it-names");
}

fn a_plugin_picks_a_character_and_retries_a_refused_entry() {
    assert_case("pregame", "auto-login");
    note_asserted("plugins.pregame.a-plugin-picks-a-character-and-retries-a-refused-entry");
}

fn a_world_that_denies_session_control_refuses_a_plugins_entry() {
    assert_case("pregame", "denied");
    note_asserted("plugins.pregame.a-world-that-denies-session-control-refuses-a-plugins-entry");
}

/// A world that publishes no policy allows every kind of automation, with a gesture or without,
/// and session control too: a plugin hears so, and a published policy that names none denies them.
fn a_plugin_that_names_no_permission_may_do_what_each_covers() {
    core(
        "unasked",
        "plugins.permissions.a-plugin-that-names-no-permission-may-do-what-each-covers",
    );
}

fn a_world_with_no_policy_allows_every_kind_of_automation() {
    let mut run = PluginRun::new();
    let open = run.host().policy().clone();
    assert!(!open.published);
    for d in dereth_plugin_api::AutoDomain::ALL {
        assert!(open.allows(d, false), "{d:?} without a gesture");
    }
    assert!(open.allows_session(false));
    let closed = dereth_plugin_api::WorldPolicy::published(std::collections::BTreeMap::new());
    for d in dereth_plugin_api::AutoDomain::ALL {
        assert!(!closed.allows(d, true), "{d:?} denied when not named");
    }
    note_asserted("plugins.policy.a-world-with-no-policy-allows-every-kind-of-automation");
}

/// Two clients on one bus, each with the peers case loaded: a message the leader sends reaches
/// the follower's copy with the leader's character, and not the leader's own; a third client
/// beyond a world's cap of one is not heard.
fn a_plugin_hears_its_copies_in_the_other_clients() {
    use dereth_plugin_host::LocalBus;
    let dir = dereth_testkit::conformance::workspace_root()
        .join(CASES_DIR)
        .join("peers")
        .join("echo");
    let bus = LocalBus::new();
    let client = |name: &str, cap: Option<u32>| {
        let mut run = PluginRun::new().with_peers(Box::new(bus.endpoint()));
        if let Some(c) = cap {
            let mut p = dereth_plugin_api::WorldPolicy::unpublished();
            p.peers_local = Some(c);
            run.host().set_world_policy(p);
        }
        run.load(read_package(&dir));
        run.scripted_world(name, Vec::new());
        run
    };
    let mut leader = client("Leader", None);
    let mut follower = client("Follower", Some(1));
    let mut third = client("Third", None);
    leader.type_line("/hello");
    third.type_line("/hello");
    for run in [&mut leader, &mut follower, &mut third] {
        run.advance(0.2);
    }
    let heard = follower.console("conformance.peers");
    assert_eq!(
        heard,
        ["heard\tLeader\tgreet\t1\tLeader"],
        "the follower hears the leader, and not the third client beyond its world's cap of one"
    );
    assert_eq!(
        leader.console("conformance.peers"),
        ["heard\tThird\tgreet\t1\tThird"],
        "the leader hears the third client, never itself"
    );
    // The case's name, for the census of cases: "echo".
    note_asserted("plugins.peers.a-plugin-hears-its-copies-in-the-other-clients");
}

fn samples() -> std::path::PathBuf {
    dereth_testkit::conformance::workspace_root().join("dereth/plugins")
}

/// The sample plugins, found as a fresh install finds them: each listed off and none loaded; one
/// ticked by command loads and answers its own command; the panel lists the rest off, each with
/// its button to turn it on, which loads the one chosen.
fn a_found_plugin_is_listed_off_until_the_player_ticks_it() {
    let found: Vec<Package> = ["xp-meter", "tank", "trackers", "navigator"]
        .iter()
        .map(|f| read_package(&samples().join(f)))
        .collect();
    let mut run = PluginRun::new().with_found(found);
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    assert!(
        run.host().plugins().is_empty(),
        "none loaded: {:?}",
        run.host().plugins()
    );
    run.type_line("/plugin list");
    run.advance(0.2);
    let printed = run.printed();
    for id in [
        "network.dereth.xpmeter",
        "network.dereth.tank",
        "network.dereth.trackers",
        "network.dereth.navigator",
    ] {
        assert!(
            printed
                .iter()
                .any(|l| l.contains(id) && l.contains(&format!("off (/plugin on {id})"))),
            "{id} listed off: {printed:#?}"
        );
    }
    run.type_line("/plugin on network.dereth.xpmeter");
    run.advance(0.5);
    run.type_line("/xp");
    run.advance(0.2);
    let loaded: Vec<String> = run.host().plugins().into_iter().map(|p| p.id).collect();
    assert_eq!(loaded, ["network.dereth.xpmeter"]);
    assert!(
        run.printed()
            .iter()
            .any(|l| l.starts_with("XP this session:")),
        "{:#?}",
        run.printed()
    );
    // The panel: the loaded plugin first, then the three that are off; the trackers turned on by
    // its button.
    run.type_line("/plugin");
    run.advance(0.2);
    let panel = run
        .host()
        .window("dereth.plugins", "panel")
        .map(|w| w.tree.dump())
        .unwrap_or_default();
    assert!(
        panel.contains(r#""text":"\"Trackers\""},"0.3.0","off"]"#) && panel.contains("Turn on"),
        "{panel}"
    );
    let row = 1
        + 1
        + run
            .host()
            .found()
            .iter()
            .filter(|b| !b.on)
            .position(|b| b.id == "network.dereth.trackers")
            .expect("listed");
    run.host()
        .ui_input(
            "dereth.plugins",
            "panel",
            dereth_plugin_host::UiInput::Select {
                node: "plugins".into(),
                rows: vec![i64::try_from(row).expect("small")],
            },
        )
        .expect("chosen");
    run.host()
        .ui_input(
            "dereth.plugins",
            "panel",
            dereth_plugin_host::UiInput::Click {
                node: "turn_on".into(),
            },
        )
        .expect("clicked");
    run.advance(0.5);
    let loaded: Vec<String> = run.host().plugins().into_iter().map(|p| p.id).collect();
    assert_eq!(
        loaded,
        ["network.dereth.xpmeter", "network.dereth.trackers"]
    );
    assert!(run
        .host()
        .found()
        .iter()
        .all(|b| b.on == (b.id == "network.dereth.xpmeter" || b.id == "network.dereth.trackers")));
    note_asserted("plugins.load.a-found-plugin-is-listed-off-until-the-player-ticks-it");
}

/// The Navigator sample: `/nav holtburg`'s neighbour by coordinates puts up the arrow, a world
/// marker and a map marker on the place; `/nav go` walks there; the HUD names the distance.
fn the_navigator_points_at_a_place_and_walks_there() {
    let mut run = PluginRun::new();
    run.load(read_package(&samples().join("navigator")));
    run.scripted_world("Tester", Vec::new());
    run.type_line("/nav 42.2N, 33.9E");
    run.advance(0.5);
    let overlays = run.overlays();
    assert!(
        overlays.contains("arrow network.dereth.navigator")
            && overlays.contains("turn=90 distance=24.0")
            && overlays.contains("world network.dereth.navigator")
            && overlays.contains("map network.dereth.navigator"),
        "the arrow and the markers are up:\n{overlays}"
    );
    assert!(
        run.ui_dump().contains("24 m E"),
        "the HUD names the distance and direction:\n{}",
        run.ui_dump()
    );
    run.type_line("/nav go");
    run.advance(10.0);
    let (ns, ew, _) = run.player_place().expect("placed");
    assert!(
        (ns - 42.2).abs() < 0.02 && (ew - 33.9).abs() < 0.02,
        "walked to the place: {ns} {ew}"
    );
    assert!(
        run.printed()
            .iter()
            .any(|l| l == "[Nav] Arrived at 42.2N 33.9E."),
        "{:?}",
        run.printed()
    );
    note_asserted("plugins.samples.the-navigator-points-at-a-place-and-walks-there");
}

/// The Auto-login sample: set in the world, it logs the chosen character in at the next character
/// list, tries again while the server says it is still in the world, and types the listed command
/// after login.
fn auto_login_enters_the_chosen_character_and_retries_while_it_is_still_in_the_world() {
    use dereth_testkit::plugins::ScriptedPregame;
    let mut run = PluginRun::new();
    run.load(read_package(&samples().join("autologin")));
    run.scripted_pregame(ScriptedPregame {
        characters: vec![(0x5000_0001, "Alpha".into()), (0x5000_0002, "Bravo".into())],
        still_in_world: vec!["Bravo".into()],
        objects: Vec::new(),
    });
    run.log_on("Alpha");
    run.advance(0.5);
    run.type_line("/autologin set Bravo");
    run.type_line("/autologin cmd add /em waves");
    run.type_line("/autologin next");
    run.advance(12.0);
    assert_eq!(
        run.moves(),
        ["log_on Alpha", "log_off", "log_on Bravo", "log_on Bravo"],
        "console {:?}",
        run.console("network.dereth.autologin")
    );
    let console = run.console("network.dereth.autologin");
    assert!(
        console.contains(&"login refused\tBravo\tcharacter_in_world".to_owned())
            && console.contains(&"logged in as\tBravo".to_owned()),
        "{console:?}"
    );
    assert!(
        run.sent()
            .iter()
            .any(|s| s.contains("Emote") || s.contains("emote")),
        "the command was typed after login: {:?}",
        run.sent()
    );
    note_asserted("plugins.samples.auto-login-enters-the-chosen-character-and-retries");
}

/// The Follow sample on two clients: the follower, told to follow the leading character, closes on
/// it as it moves.
fn follow_brings_another_client_to_the_leader() {
    use dereth_plugin_host::LocalBus;
    use dereth_testkit::plugins::ScriptedObject;
    const LEADER: u32 = 0x5000_0002;
    let bus = LocalBus::new();
    let mut leader = PluginRun::new()
        .with_peers(Box::new(bus.endpoint()))
        .with_player_id(LEADER);
    let mut follower = PluginRun::new().with_peers(Box::new(bus.endpoint()));
    leader.load(read_package(&samples().join("follow")));
    follower.load(read_package(&samples().join("follow")));
    leader.scripted_world("Leader", Vec::new());
    follower.scripted_world(
        "Follower",
        vec![ScriptedObject {
            id: LEADER,
            name: "Leader".into(),
            obj_type: dereth_rules::weenie::item_type::CREATURE,
            place: Some((42.25, 33.8)),
            ..ScriptedObject::default()
        }],
    );
    leader.type_line("/follow lead");
    follower.type_line("/follow Leader");
    for step in 0..40 {
        leader.advance(0.25);
        follower.advance(0.25);
        if step == 20 {
            follower.move_object(LEADER, 42.25, 33.85);
        }
    }
    let (ns, ew, _) = follower.player_place().expect("placed");
    let d = (((ns - 42.25) * 240.0).powi(2) + ((ew - 33.85) * 240.0).powi(2)).sqrt();
    assert!(
        d <= 4.5,
        "the follower is within range of the leader: {d:.1} m ({:?})",
        follower.moves()
    );
    note_asserted("plugins.samples.follow-brings-another-client-to-the-leader");
}

fn a_plugin_fights_through_the_combat_controls() {
    assert_case("automation", "combat");
    note_asserted("plugins.automation.a-plugin-fights-through-the-combat-controls");
}

fn a_player_killers_automation_stops_where_the_world_says() {
    assert_case("automation", "pk");
    note_asserted("plugins.automation.a-player-killers-automation-stops-where-the-world-says");
}

fn read_parity_limits_a_search_to_the_radar_and_the_pack() {
    assert_case("automation", "parity");
    note_asserted("plugins.automation.read-parity-limits-a-search-to-the-radar-and-the-pack");
}

fn a_meta_runs_its_states_rules_on_a_timer() {
    assert_case("automation", "meta");
    note_asserted("plugins.automation.a-meta-runs-its-states-rules-on-a-timer");
}

fn portal_space_and_death_are_heard_in_order() {
    assert_case("automation", "portal-death");
    note_asserted("plugins.automation.portal-space-and-death-are-heard-in-order");
}

fn a_watchdog_moves_a_still_meta_on() {
    assert_case("automation", "watchdog");
    note_asserted("plugins.automation.a-watchdog-moves-a-still-meta-on");
}

fn expressions_evaluate_in_vtanks_language() {
    assert_case("automation", "expressions");
    note_asserted("plugins.automation.expressions-evaluate-in-vtanks-language");
}

fn a_vtank_meta_file_imports_and_runs_in_vtanks_order() {
    assert_case("automation", "met-import");
    assert_case("automation", "meta-once");
    note_asserted("plugins.automation.a-vtank-meta-file-imports-and-runs-in-vtanks-order");
}

fn a_vtank_meta_file_read_as_data_writes_back_as_vtank_writes_it() {
    assert_case("automation", "met-write");
    note_asserted(
        "plugins.automation.a-vtank-meta-file-read-as-data-writes-back-as-vtank-writes-it",
    );
}

fn stopping_and_letting_go_are_always_allowed() {
    assert_case("automation", "let-go");
    note_asserted("plugins.automation.stopping-and-letting-go-are-always-allowed");
}

fn stopping_an_attack_cancels_it_and_fails_its_promise() {
    assert_case("automation", "combat-stop");
    note_asserted("plugins.automation.stopping-an-attack-cancels-it-and-fails-its-promise");
}

/// The test suite's compiled plugin (the XP Meter compiled from Rust to WebAssembly) and the Lua
/// XP Meter, loaded together over a recorded session, answer their commands (`/xpc` and `/xp`)
/// with the same line and show the same total in their windows.
fn a_compiled_plugin_counts_what_the_same_lua_plugin_counts() {
    let mut run = PluginRun::new();
    run.load(read_package(&samples().join("xp-meter")));
    run.load(read_package(&compiled_fixture().join("package")));
    run.login("long-solo-play", Some("+Aldric"));
    run.advance(1050.0);
    // Both asked in the same frame, so both measure the rate at the same moment.
    run.type_line("/xp");
    run.type_line("/xpc");
    run.advance(0.5);
    let answers: Vec<String> = run
        .printed()
        .into_iter()
        .filter(|l| l.starts_with("XP this session:"))
        .collect();
    assert_eq!(answers.len(), 2, "{:?}", run.printed());
    assert!(
        answers[0].starts_with("XP this session: 23,651 in "),
        "{}",
        answers[0]
    );
    assert_eq!(answers[0], answers[1]);
    let ui = run.ui_dump();
    assert_eq!(
        ui.matches("text=\"23,651\"").count(),
        2,
        "both windows show the total:\n{ui}"
    );
    note_asserted("plugins.packages.a-compiled-plugin-counts-what-the-same-lua-plugin-counts");
}

/// The compiled plugin the tests run: its package and the Rust crate it is built from.
fn compiled_fixture() -> std::path::PathBuf {
    dereth_testkit::conformance::workspace_root()
        .join("dereth/testkit/tests/fixtures/compiled-plugin")
}

fn the_players_own_movement_ends_a_plugins_movement_and_comes_first() {
    assert_case("nav", "player-first");
    assert_case("nav", "player-yield");
    note_asserted("plugins.nav.the-players-own-movement-ends-a-plugins-movement-and-comes-first");
}

fn a_plugins_movement_gives_way_when_the_server_takes_control() {
    assert_case("nav", "server-control");
    note_asserted("plugins.nav.a-plugins-movement-gives-way-when-the-server-takes-control");
}

fn an_unloaded_or_suspended_plugin_lets_go_of_the_character() {
    assert_case("nav", "released");
    assert_case("nav", "suspended");
    assert_case("automation", "attack-released");
    note_asserted("plugins.nav.an-unloaded-or-suspended-plugin-lets-go-of-the-character");
}

fn an_attack_settles_on_what_the_server_does_with_it() {
    assert_case("automation", "attack-outcomes");
    note_asserted("plugins.automation.an-attack-settles-on-what-the-server-does-with-it");
}

fn an_attacks_swing_goes_only_at_its_target() {
    assert_case("automation", "attack-target");
    note_asserted("plugins.automation.an-attacks-swing-goes-only-at-its-target");
}

fn a_heal_is_its_own_domain_and_a_debuff_is_cast_once_per_target() {
    assert_case("automation", "heal-debuff");
    note_asserted(
        "plugins.automation.a-heal-is-its-own-domain-and-a-debuff-is-cast-once-per-target",
    );
}

fn beneficial_spells_on_another_character_are_their_own_domain() {
    assert_case("automation", "support");
    note_asserted("plugins.automation.beneficial-spells-on-another-character-are-their-own-domain");
}

/// A package carrying a `plugin.sig` file loads as any other: the file is just a file in it, read
/// by no one, whatever it holds.
fn a_package_loads_whatever_signature_file_it_carries() {
    let with = |text: &str| {
        let mut p = read_package(&samples().join("xp-meter"));
        p.files
            .insert("plugin.sig".into(), text.as_bytes().to_vec());
        p
    };
    let mut run = PluginRun::new();
    run.load(with("key = \"00\"\nsignature = \"00\"\n"));
    let listed = run.host().plugins();
    assert_eq!(listed.len(), 1, "{:?}", run.printed());
    assert_eq!(listed[0].status, "running");
    run.host().request_unload("network.dereth.xpmeter");
    run.frame();
    run.load(with("not even TOML"));
    assert_eq!(run.host().plugins().len(), 1, "{:?}", run.printed());
    note_asserted("plugins.packages.a-package-loads-whatever-signature-file-it-carries");
}

/// The Tank sample's loop in a scripted world: it closes on the monster in range and attacks it;
/// when the monster falls and its corpse appears it counts the kill, walks to the corpse, opens it
/// and keeps what its loot profile keeps; with nothing left it walks its route.
fn the_tank_fights_loots_its_kill_and_walks_its_route() {
    use dereth_rules::weenie::{bitfield, item_type};
    use dereth_testkit::plugins::ScriptedObject;
    const MONSTER: u32 = 0x8000_0080;
    const CORPSE: u32 = 0x8000_0081;
    const GEM: u32 = 0x8000_0082;
    let mut run = PluginRun::new();
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("tank")));
    run.scripted_world(
        "Tester",
        vec![ScriptedObject {
            id: MONSTER,
            name: "Drudge Skulker".into(),
            obj_type: item_type::CREATURE,
            bitfield: bitfield::ATTACKABLE,
            place: Some((42.2, 33.85)),
            ..ScriptedObject::default()
        }],
    );
    run.type_line("/tank route holtburg-loop.nav");
    run.type_line("/tank profile basic.utl");
    run.type_line("/tank start");
    run.advance(15.0);
    let sent = run.sent();
    assert!(
        sent.iter()
            .any(|s| s == &format!("melee_attack {MONSTER:#x}")),
        "a swing at the monster: {sent:?} (console {:?})",
        run.console("network.dereth.tank")
    );
    // The monster falls where it stood, and its corpse holds a gem.
    run.remove(MONSTER);
    run.spawn(vec![
        ScriptedObject {
            id: CORPSE,
            name: "Corpse of Drudge Skulker".into(),
            obj_type: item_type::CONTAINER,
            bitfield: bitfield::CORPSE | bitfield::OPENABLE | bitfield::STUCK,
            place: Some((42.2, 33.85)),
            ..ScriptedObject::default()
        },
        ScriptedObject {
            id: GEM,
            name: "Black Garnet".into(),
            obj_type: item_type::GEM,
            container: Some(CORPSE),
            value: 500,
            ..ScriptedObject::default()
        },
    ]);
    run.advance(20.0);
    let console = run.console("network.dereth.tank");
    let said = |s: &str| console.iter().any(|l| l.contains(s));
    assert!(said("killed Drudge Skulker (1 so far)"), "{console:#?}");
    assert!(said("kept Black Garnet"), "{console:#?}");
    assert_eq!(
        run.container_of(GEM),
        Some(0x5000_0001),
        "the gem is in the pack"
    );
    assert!(said("route: starting at waypoint"), "{console:#?}");
    note_asserted("plugins.samples.the-tank-fights-loots-its-kill-and-walks-its-route");
}

/// The Tank by its monster list: of three monsters in range it leaves the one set to be left alone
/// (though it is nearest) and fights the one of highest priority first; holding a caster, it casts
/// the debuff the list asks for on it, then its attack spell.
fn the_tank_fights_by_its_monster_list_with_debuffs_and_spells() {
    use dereth_rules::weenie::{bitfield, item_type};
    use dereth_testkit::plugins::{ScriptedObject, SCRIPTED_PLAYER};
    const RAT: u32 = 0x8000_0090;
    const SKULKER: u32 = 0x8000_0091;
    const PROWLER: u32 = 0x8000_0092;
    const WAND: u32 = 0x8000_0093;
    let mut run = PluginRun::new();
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("tank")));
    let monster = |id: u32, name: &str, ew: f64| ScriptedObject {
        id,
        name: name.into(),
        obj_type: item_type::CREATURE,
        bitfield: bitfield::ATTACKABLE,
        place: Some((42.2, ew)),
        ..ScriptedObject::default()
    };
    run.scripted_world(
        "Tester",
        vec![
            monster(RAT, "Rat", 33.8001),
            monster(SKULKER, "Drudge Skulker", 33.8002),
            monster(PROWLER, "Drudge Prowler", 33.8003),
            ScriptedObject {
                id: WAND,
                name: "Wand".into(),
                obj_type: item_type::CASTER,
                container: Some(SCRIPTED_PLAYER),
                locations: Some(0x0100_0000),
                ..ScriptedObject::default()
            },
        ],
    );
    for line in [
        "/tank set EnableNav false",
        "/tank set Weapon Wand",
        "/tank monster priority -1 Rat",
        "/tank monster priority 0 Drudge Skulker",
        "/tank monster priority 3 Drudge Prowler",
        "/tank monster imperil on Drudge Prowler",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(15.0);
    let sent = run.sent();
    let console = run.console("network.dereth.tank");
    // The scripted spell table carries no component formulas, so a cast goes out with the target
    // selected rather than named (`cast <spell>`); the console says on whom.
    let at = |s: &str| sent.iter().position(|x| x == s);
    let imperil = at("cast 23");
    let bolt = at("cast 27");
    assert!(
        imperil.is_some() && bolt.is_some() && imperil < bolt,
        "the debuff, then the bolt: {sent:?} (console {console:#?})"
    );
    assert!(
        console
            .iter()
            .any(|l| l.ends_with("Imperil Other I on Drudge Prowler")),
        "{console:#?}"
    );
    assert!(
        !sent.iter().any(|s| s.contains(&format!("{RAT:#x}"))),
        "the rat is left alone: {sent:?}"
    );
    let first_target = console
        .iter()
        .find(|l| l.contains("target "))
        .cloned()
        .unwrap_or_default();
    assert!(first_target.contains("Drudge Prowler"), "{console:#?}");
    note_asserted("plugins.samples.the-tank-fights-by-its-monster-list-with-debuffs-and-spells");
}

/// The Tank sent to hunt somewhere far travels there by a journey (here through a portal), says
/// so in its log, and runs its loop where it arrived; a route's far waypoint is reached the same way.
/// Told to hunt somewhere else while it runs, it travels there at its next step.
fn the_tank_travels_to_its_hunting_place_and_hunts_there() {
    use dereth_rules::weenie::{bitfield, item_type};
    use dereth_testkit::plugins::{ScriptedObject, SCRIPTED_PLAYER};
    use dereth_testkit::scripted_travel::ScriptedPortal;
    const SKULKER: u32 = 0x8000_0091;
    const WAND: u32 = 0x8000_0093;
    let mut run = PluginRun::new();
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("tank")));
    let place = |ns: f64, ew: f64| {
        dereth_plugin_host::places::from_coords(ns, ew, 0.0).expect("on the map")
    };
    run.scripted_portals(
        &[ScriptedPortal {
            name: "Portal to the Hills".into(),
            class: 100,
            at: place(42.2, 33.85),
            to: place(40.0, 30.0),
            source: dereth_navigation::EdgeSource::Hint,
            refuses: false,
        }],
        Vec::new(),
    );
    run.scripted_world(
        "Tester",
        vec![
            ScriptedObject {
                id: SKULKER,
                name: "Drudge Skulker".into(),
                obj_type: item_type::CREATURE,
                bitfield: bitfield::ATTACKABLE,
                place: Some((40.0, 30.0002)),
                ..ScriptedObject::default()
            },
            ScriptedObject {
                id: WAND,
                name: "Wand".into(),
                obj_type: item_type::CASTER,
                container: Some(SCRIPTED_PLAYER),
                locations: Some(0x0100_0000),
                ..ScriptedObject::default()
            },
        ],
    );
    for line in [
        "/tank set EnableNav false",
        "/tank set Weapon Wand",
        "/tank hunt 40.0N 30.0E",
    ] {
        run.type_line(line);
    }
    run.advance(20.0);
    let console = run.console("network.dereth.tank");
    let at = |needle: &str| console.iter().position(|l| l.contains(needle));
    let planned = at("journey: walk, portal Portal to the Hills");
    let arrived = at("arrived ");
    let hunting = at("hunting at 40.0N 30.0E");
    let target = at("target Drudge Skulker");
    assert!(
        planned.is_some() && planned < arrived && arrived < hunting && hunting < target,
        "planned, arrived, hunting, then a target: {console:#?}"
    );
    assert!(
        run.moves()
            .iter()
            .any(|m| m == "travel portal Portal to the Hills"),
        "{:?}",
        run.moves()
    );
    let (ns, ew, _) = run.player_place().expect("a place");
    assert!(
        (ns - 40.0).abs() < 0.01 && (ew - 30.0).abs() < 0.01,
        "at {ns}, {ew}"
    );
    // Told to hunt somewhere else while it runs: it travels there at its next step.
    run.type_line("/tank hunt 40.1N 30.0E");
    run.advance(10.0);
    assert!(
        run.console("network.dereth.tank")
            .iter()
            .any(|l| l.contains("travelling to 40.1N 30.0E")),
        "{:#?}",
        run.console("network.dereth.tank")
    );
    note_asserted("plugins.samples.the-tank-travels-to-its-hunting-place-and-hunts-there");
}

/// The Tank keeps its buffs up: with nothing to fight it takes up its caster and enters magic
/// stance, and casts on itself the attribute and protection buffs it knows, each once when its
/// enchantment shows, and none it does not know.
fn the_tank_keeps_its_buffs_up() {
    use dereth_testkit::plugins::{ScriptedObject, SCRIPTED_PLAYER};
    let mut run = PluginRun::new();
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("tank")));
    run.scripted_world(
        "Tester",
        vec![ScriptedObject {
            id: 0x8000_00B0,
            name: "Wand".into(),
            obj_type: dereth_rules::weenie::item_type::CASTER,
            container: Some(SCRIPTED_PLAYER),
            locations: Some(0x0100_0000),
            ..ScriptedObject::default()
        }],
    );
    run.casts_land(true);
    run.type_line("/tank set EnableNav false");
    run.type_line("/tank start");
    run.advance(3.0);
    let sent = run.sent();
    let casts: Vec<&String> = sent.iter().filter(|s| s.starts_with("cast ")).collect();
    // The scripted character knows Strength Self I, Armor Self I and Blade Protection Self I among
    // the defaults (each cast on itself, 0x50000001);
    // Heal Self is not a buff, and the "Other" spells are not cast on itself.
    assert_eq!(
        casts,
        [
            "cast 2087 0x50000001",
            "cast 1133 0x50000001",
            "cast 2215 0x50000001"
        ],
        "{sent:?} (console {:#?})",
        run.console("network.dereth.tank")
    );
    // Each showed on the character: none is cast again.
    run.advance(60.0);
    let strength = run
        .sent()
        .iter()
        .filter(|s| s.as_str() == "cast 2087 0x50000001")
        .count();
    assert_eq!(strength, 1, "{:?}", run.sent());
    note_asserted("plugins.samples.the-tank-keeps-its-buffs-up");
}

/// The Tank's item buffs are confirmed by appraisal: a buff on the caster or the armour worn is done
/// only once the item's appraisal lists its enchantment. A fizzle casts it again at once; a cast that
/// ends well with nothing in the appraisal casts it again; once it shows, the next item's buff.
/// Where the weapon buffs are auras, the caster's is cast on the character and confirmed there.
fn the_tank_confirms_an_item_buff_by_the_items_appraisal() {
    use dereth_rules::weenie::item_type;
    use dereth_testkit::plugins::{ScriptedObject, SCRIPTED_PLAYER};
    const WAND: u32 = 0x8000_00B0;
    const ARMOR: u32 = 0x8000_00C0;
    let start = |spells: &[(u32, &str)], failures: Vec<u32>| {
        let mut run = PluginRun::new()
            .with_item_spells(spells)
            .with_cast_failures(failures);
        run.host().set_world_policy(
            dereth_plugin_host::policy_doc::parse(
                "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
            )
            .expect("a policy"),
        );
        run.load(read_package(&samples().join("tank")));
        let worn = |id: u32, name: &str, obj_type: u32, locations: u32| ScriptedObject {
            id,
            name: name.into(),
            obj_type,
            // Gear a spell may be cast on, as the server describes it.
            bitfield: dereth_rules::weenie::bitfield::ATTACKABLE,
            container: Some(SCRIPTED_PLAYER),
            locations: Some(locations),
            worn: true,
            ..ScriptedObject::default()
        };
        run.scripted_world(
            "Tester",
            vec![
                // The wand in the pack: the Tank takes it up for magic stance, then holds it.
                ScriptedObject {
                    worn: false,
                    ..worn(WAND, "Wand", item_type::CASTER, 0x0100_0000)
                },
                worn(ARMOR, "Breastplate", item_type::ARMOR, 0x0000_0200),
            ],
        );
        // Only the item buffs: no self buff family the character knows.
        run.type_line("/tank set EnableNav false");
        run.type_line("/tank set BuffSpells Nothing");
        run.type_line("/tank set BuffItems true");
        run.type_line("/tank start");
        run
    };
    let mut run = start(
        &[(49, "Defender I"), (51, "Impenetrability I")],
        vec![0x0402],
    );
    let casts = |run: &PluginRun| -> Vec<String> {
        run.sent()
            .iter()
            .filter(|s| s.starts_with("cast "))
            .cloned()
            .collect()
    };
    let first_buff = format!("cast 51 {ARMOR:#x}");
    let next_buff = format!("cast 49 {WAND:#x}");
    // A fizzle, then casts that end well with nothing on the breastplate's appraisal:
    // Impenetrability each time.
    run.advance(6.0);
    let first = casts(&run);
    assert!(
        first.len() >= 3 && first.iter().all(|c| *c == first_buff),
        "{first:?} (console {:#?})",
        run.console("network.dereth.tank")
    );
    assert!(
        run.sent()
            .iter()
            .any(|s| *s == format!("appraise {ARMOR:#x}")),
        "the breastplate is appraised after a cast: {:?}",
        run.sent()
    );
    let console = run.console("network.dereth.tank");
    assert!(
        console
            .iter()
            .any(|l| l.contains("Impenetrability I on Breastplate: fizzled; casting it again")),
        "{console:#?}"
    );
    assert!(
        console
            .iter()
            .any(|l| l
                .contains("Impenetrability I on Breastplate: no effect showed; casting it again")),
        "{console:#?}"
    );
    // Now casts land: Impenetrability shows in the breastplate's appraisal, then the wand's
    // Defender is cast, once.
    run.casts_land(true);
    run.advance(12.0);
    let all = casts(&run);
    let tries = all.iter().filter(|c| **c == first_buff).count();
    let after: Vec<&String> = all.iter().skip_while(|c| **c == first_buff).collect();
    assert_eq!(
        after,
        [&next_buff],
        "{all:?} (console {:#?})",
        run.console("network.dereth.tank")
    );
    let console = run.console("network.dereth.tank");
    assert!(
        console.iter().any(|l| l.contains(&format!(
            "Impenetrability I on Breastplate confirmed in its appraisal after {tries} casts"
        ))),
        "{console:#?}"
    );
    assert!(
        console
            .iter()
            .any(|l| l.ends_with("Defender I on Wand confirmed in its appraisal")),
        "{console:#?}"
    );
    // Both confirmed: neither is cast again.
    run.advance(60.0);
    assert_eq!(casts(&run).len(), all.len(), "{:?}", casts(&run));

    // Where the caster's buff is an aura, it is cast on the character, for a character carrying a
    // caster, and is done once it shows among the character's enchantments.
    let mut run = start(&[(1599, "Aura of Defender Self I")], vec![0x0402]);
    run.casts_land(true);
    run.advance(8.0);
    assert_eq!(
        casts(&run),
        ["cast 1599 0x50000001", "cast 1599 0x50000001"],
        "(console {:#?})",
        run.console("network.dereth.tank")
    );
    assert!(
        run.console("network.dereth.tank")
            .iter()
            .any(|l| l.ends_with("Aura of Defender Self I confirmed after 2 casts")),
        "{:#?}",
        run.console("network.dereth.tank")
    );
    note_asserted("plugins.samples.the-tank-confirms-an-item-buff-by-the-items-appraisal");
}

/// The Tank's buffing is strict: a buff is done only once its effect shows on the character. Fizzles
/// and a cast that ends well with nothing to show cast the same spell again, never the next buff;
/// a refusal casting again cannot mend (a component missing) skips it, said once; and a spell that
/// fails again and again pauses buffing, said, rather than moving on.
fn the_tank_casts_a_buff_again_until_its_effect_shows() {
    use dereth_testkit::plugins::{ScriptedObject, SCRIPTED_PLAYER};
    let start = |failures: Vec<u32>, land: bool| {
        let mut run = PluginRun::new().with_cast_failures(failures);
        run.host().set_world_policy(
            dereth_plugin_host::policy_doc::parse(
                "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
            )
            .expect("a policy"),
        );
        run.load(read_package(&samples().join("tank")));
        run.scripted_world(
            "Tester",
            vec![ScriptedObject {
                id: 0x8000_00B0,
                name: "Wand".into(),
                obj_type: dereth_rules::weenie::item_type::CASTER,
                container: Some(SCRIPTED_PLAYER),
                locations: Some(0x0100_0000),
                ..ScriptedObject::default()
            }],
        );
        run.casts_land(land);
        run.type_line("/tank set EnableNav false");
        run.type_line("/tank start");
        run
    };
    let casts = |run: &PluginRun| -> Vec<String> {
        run.sent()
            .iter()
            .filter(|s| s.starts_with("cast "))
            .cloned()
            .collect()
    };
    const STRENGTH: &str = "cast 2087 0x50000001";

    // Three fizzles, then casts that end well but show nothing: the strength buff every time.
    let mut run = start(vec![0x0402, 0x0402, 0x0402], false);
    run.advance(14.0);
    let first = casts(&run);
    assert!(
        first.len() >= 5 && first.iter().all(|c| c == STRENGTH),
        "the same spell until its effect shows: {first:?} (console {:#?})",
        run.console("network.dereth.tank")
    );
    let console = run.console("network.dereth.tank");
    assert_eq!(
        console
            .iter()
            .filter(|l| l.contains("fizzled; casting it again"))
            .count(),
        3,
        "{console:#?}"
    );
    assert!(
        console
            .iter()
            .any(|l| l.contains("no effect showed; casting it again")),
        "{console:#?}"
    );
    // Now it shows: the step is done, said with how many casts it took, and the next buffs follow.
    run.casts_land(true);
    run.advance(8.0);
    let all = casts(&run);
    let strength = all.iter().filter(|c| c.as_str() == STRENGTH).count();
    let after: Vec<&String> = all.iter().skip_while(|c| c.as_str() == STRENGTH).collect();
    assert_eq!(
        after,
        ["cast 1133 0x50000001", "cast 2215 0x50000001"],
        "once strength shows, the next buffs, each once: {all:?} (console {:#?})",
        run.console("network.dereth.tank")
    );
    let console = run.console("network.dereth.tank");
    assert!(
        console.iter().any(|l| l.contains(&format!(
            "Might of the Lugians confirmed after {strength} casts"
        ))),
        "{console:#?}"
    );

    // A missing component is not mended by casting again: skipped, said once, the next cast.
    let mut run = start(vec![0x0400], true);
    run.advance(4.0);
    assert_eq!(
        casts(&run),
        [STRENGTH, "cast 1133 0x50000001", "cast 2215 0x50000001"]
    );
    let console = run.console("network.dereth.tank");
    assert_eq!(
        console
            .iter()
            .filter(|l| l.contains("Might of the Lugians skipped: a component missing"))
            .count(),
        1,
        "{console:#?}"
    );

    // Fifteen fizzles in a row: buffing pauses, said, and no other buff is cast meanwhile.
    let mut run = start(vec![0x0402; 15], true);
    run.advance(45.0);
    assert!(
        casts(&run).iter().all(|c| c == STRENGTH),
        "{:?}",
        casts(&run)
    );
    let console = run.console("network.dereth.tank");
    assert!(
        console
            .iter()
            .any(|l| l.contains("failed 15 times in a row") && l.contains("buffing paused")),
        "{console:#?}"
    );
    note_asserted("plugins.samples.the-tank-casts-a-buff-again-until-its-effect-shows");
}

/// The Tank buffs as VTank does: each buff at the highest level its buffed skill casts reliably
/// (the spell's difficulty and SpellDiffExcessThreshold-Buff more), climbing to the higher level once
/// the skill reaches it; with too little mana for the buff it casts Stamina to Mana first.
fn the_tank_buffs_at_the_level_its_skill_reaches_and_climbs_as_it_rises() {
    use dereth_testkit::plugins::{ScriptedObject, SCRIPTED_PLAYER};
    let mut run = PluginRun::new();
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1
[automation]
default = \"allow\"
",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("tank")));
    run.scripted_world(
        "Tester",
        vec![ScriptedObject {
            id: 0x8000_00B0,
            name: "Wand".into(),
            obj_type: dereth_rules::weenie::item_type::CASTER,
            container: Some(SCRIPTED_PLAYER),
            locations: Some(0x0100_0000),
            ..ScriptedObject::default()
        }],
    );
    // Might of the Lugians (2087, level 7) takes 300 and 5 more; Strength Self I (1) takes 1.
    run.set_skill("Creature Enchantment", 300);
    run.set_skill("Life Magic", 300);
    run.casts_land(true);
    run.set_vitals((100, 100), (100, 100), (8, 100));
    run.type_line("/tank set EnableNav false");
    run.type_line("/tank start");
    run.advance(1.0);
    let casts = |run: &PluginRun| -> Vec<String> {
        run.sent()
            .iter()
            .filter(|s| s.starts_with("cast "))
            .cloned()
            .collect()
    };
    let first = casts(&run);
    assert_eq!(
        first.first().map(String::as_str),
        Some("cast 1676 0x50000001"),
        "too little mana: Stamina to Mana first ({first:?}, console {:#?})",
        run.console("network.dereth.tank")
    );
    run.set_vitals((100, 100), (100, 100), (100, 100));
    run.advance(3.0);
    let all = casts(&run);
    assert!(
        all.contains(&"cast 1 0x50000001".to_owned())
            && !all.contains(&"cast 2087 0x50000001".to_owned()),
        "at 300 the strength buff is level 1, not 7: {all:?}"
    );
    // A mastery lands: the skill reaches the level 7 spell, and the Tank climbs to it.
    run.set_skill("Creature Enchantment", 305);
    run.advance(5.0);
    let all = casts(&run);
    assert!(
        all.contains(&"cast 2087 0x50000001".to_owned()),
        "at 305 it climbs to level 7: {all:?} (console {:#?})",
        run.console("network.dereth.tank")
    );
    note_asserted(
        "plugins.samples.the-tank-buffs-at-the-level-its-skill-reaches-and-climbs-as-it-rises",
    );
}

/// The Tank gets its vitals back by VTank's lists, in peace stance: low health takes the healing
/// kit; low mana with nothing about takes the mana potion (its vital read from the appraisal, here
/// from its name), not the stamina one; and a worn item under the refill mark is refilled from the
/// charged mana stone, used on the character.
fn the_tank_recharges_by_vtanks_lists_and_refills_worn_mana() {
    use dereth_rules::weenie::{bitfield, item_type};
    use dereth_testkit::plugins::{ScriptedObject, SCRIPTED_PLAYER};
    const KIT: u32 = 0x8000_00A0;
    const MANA_POTION: u32 = 0x8000_00A1;
    const STAMINA_POTION: u32 = 0x8000_00A2;
    const STONE: u32 = 0x8000_00A3;
    const ROBE: u32 = 0x8000_00A4;
    let mut run = PluginRun::new();
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("tank")));
    let held = |id: u32, name: &str, obj_type: u32| ScriptedObject {
        id,
        name: name.into(),
        obj_type,
        container: Some(SCRIPTED_PLAYER),
        ..ScriptedObject::default()
    };
    run.scripted_world(
        "Tester",
        vec![
            ScriptedObject {
                bitfield: bitfield::HEALER,
                ..held(KIT, "Healing Kit", item_type::MISC)
            },
            held(MANA_POTION, "Mana Potion", item_type::FOOD),
            held(STAMINA_POTION, "Stamina Elixir", item_type::FOOD),
            ScriptedObject {
                ints: vec![(107, 4000), (108, 4000)],
                ..held(STONE, "Mana Stone", item_type::MANASTONE)
            },
            ScriptedObject {
                container: None,
                worn: true,
                ints: vec![(107, 100), (108, 1000)],
                ..held(ROBE, "Robe", item_type::CLOTHING)
            },
        ],
    );
    // Health 40%: under Recharge-Norm-HitP (50), the kit first in peace stance.
    run.set_vitals((40, 100), (100, 100), (100, 100));
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableBuffing false",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(3.0);
    // Health back; mana 10% with nothing about: under Recharge-NoTarg-Mana (60), the mana potion.
    run.set_vitals((100, 100), (100, 100), (10, 100));
    run.advance(3.0);
    // Vitals full: the robe at 10% of its mana is under the refill mark (25%), so the charged
    // stone is used on the character.
    run.set_vitals((100, 100), (100, 100), (100, 100));
    run.advance(3.0);
    let sent = run.sent();
    // An item used on the character itself goes out as a plain use.
    let first = |id: u32| sent.iter().position(|s| *s == format!("use {id:#x}"));
    let (kit, potion, stone) = (first(KIT), first(MANA_POTION), first(STONE));
    assert!(
        kit.is_some() && potion.is_some() && stone.is_some() && kit < potion && potion < stone,
        "the kit, then the mana potion, then the stone: {sent:?} (console {:#?})",
        run.console("network.dereth.tank")
    );
    assert_eq!(
        first(STAMINA_POTION),
        None,
        "not the stamina potion: {sent:?}"
    );
    note_asserted("plugins.samples.the-tank-recharges-by-vtanks-lists-and-refills-worn-mana");
}

/// The Tank in a world that allows combat only with a click, without auto.combat: its own
/// /tank start covers the first 30 s; after that the fight is paused, said once with what would
/// allow it, and no error repeats.
fn the_tank_pauses_a_domain_it_may_not_automate_and_says_why_once() {
    use dereth_rules::weenie::{bitfield, item_type};
    use dereth_testkit::plugins::ScriptedObject;
    let mut run = PluginRun::with_config(|c| {
        c.policy.automation.insert(
            dereth_plugin_api::AutoDomain::Combat,
            dereth_plugin_api::DomainRule::InteractiveOnly,
        );
    });
    run.load(read_package(&samples().join("tank")));
    run.scripted_world(
        "Tester",
        vec![ScriptedObject {
            id: 0x8000_00C0,
            name: "Drudge Skulker".into(),
            obj_type: item_type::CREATURE,
            bitfield: bitfield::ATTACKABLE,
            place: Some((42.2, 33.8001)),
            ..ScriptedObject::default()
        }],
    );
    run.type_line("/tank set EnableNav false");
    run.type_line("/tank set EnableBuffing false");
    run.type_line("/tank start");
    run.advance(45.0);
    let console = run.console("network.dereth.tank");
    let paused: Vec<&String> = console
        .iter()
        .filter(|l| l.contains("combat paused"))
        .collect();
    assert_eq!(paused.len(), 1, "{console:#?}");
    assert!(
        paused[0].contains("this world allows combat only with a click of yours"),
        "says why: {}",
        paused[0]
    );
    assert!(
        !console.iter().any(|l| l.contains("error")),
        "no error: {console:#?}"
    );
    assert!(
        console.iter().any(|l| l.contains("unattended, these wait")),
        "said at the start: {console:#?}"
    );
    let bar = run.bar();
    assert!(
        bar.iter()
            .any(|l| l.starts_with("network.dereth.tank	paused	")),
        "the plugin bar shows it paused: {bar:?}"
    );
    run.type_line("/tank stop");
    run.advance(1.0);
    let bar = run.bar();
    assert!(
        bar.iter()
            .any(|l| l.starts_with("network.dereth.tank	idle	")),
        "and idle once stopped: {bar:?}"
    );
    note_asserted("plugins.samples.the-tank-pauses-a-domain-it-may-not-automate-and-says-why-once");
}

/// The Tank's window takes the item selected in the game for a slot at a click: a wand for the
/// Wand, arrows for the Ammo; an item that does not fit is refused with why.
fn the_tank_takes_the_selected_item_for_a_slot() {
    use dereth_rules::weenie::item_type;
    use dereth_testkit::plugins::{ScriptedObject, SCRIPTED_PLAYER};
    let mut run = PluginRun::new();
    run.load(read_package(&samples().join("tank")));
    let held = |id: u32, name: &str, obj_type: u32| ScriptedObject {
        id,
        name: name.into(),
        obj_type,
        container: Some(SCRIPTED_PLAYER),
        ..ScriptedObject::default()
    };
    run.scripted_world(
        "Tester",
        vec![
            held(0x8000_00D0, "Fine Wand", item_type::CASTER),
            held(0x8000_00D1, "Deadly Arrow", item_type::MISSILE_WEAPON),
        ],
    );
    run.select(0x8000_00D0);
    run.click("network.dereth.tank", "main.use:Wand");
    run.click("network.dereth.tank", "main.use:Ammo");
    run.select(0x8000_00D1);
    run.click("network.dereth.tank", "main.use:Ammo");
    run.type_line("/tank settings");
    let console = run.console("network.dereth.tank");
    let has = |s: &str| console.iter().any(|l| l.contains(s));
    assert!(has("Wand: Fine Wand"), "{console:#?}");
    assert!(
        has("Fine Wand is not arrows, bolts or darts"),
        "{console:#?}"
    );
    assert!(has("Ammo: Deadly Arrow"), "{console:#?}");
    let printed = run.printed().join("\n");
    assert!(
        printed.contains("CasterWeapon")
            && printed.contains("Fine Wand")
            && printed.contains("Deadly Arrow"),
        "{printed}"
    );
    note_asserted("plugins.samples.the-tank-takes-the-selected-item-for-a-slot");
}

/// Data folders shared with the test, so it can put files in and read what the plugin saved.
#[derive(Debug, Clone, Default)]
pub(crate) struct SharedFiles(std::rc::Rc<std::cell::RefCell<dereth_plugin_host::MemoryFiles>>);

impl dereth_plugin_host::PluginFiles for SharedFiles {
    fn list(&mut self, plugin: &str) -> Vec<String> {
        self.0.borrow_mut().list(plugin)
    }
    fn read(&mut self, plugin: &str, path: &str) -> Option<Vec<u8>> {
        self.0.borrow_mut().read(plugin, path)
    }
    fn write(&mut self, plugin: &str, path: &str, bytes: &[u8]) -> Result<(), String> {
        self.0.borrow_mut().write(plugin, path, bytes)
    }
}

/// The Tank makes a route in the game as VTank's Route tab makes one (a point where the character
/// stands, a jump, the selected portal), saves it as a VTank route that reads back, and walks it:
/// it jumps at the jump waypoint and uses the portal, going on once through.
fn the_tank_makes_a_route_in_the_game_and_walks_it_jumping_and_through_its_portal() {
    use dereth_plugin_host::PluginFiles;
    use dereth_rules::weenie::item_type;
    use dereth_testkit::plugins::ScriptedObject;
    const PORTAL: u32 = 0x8000_0090;
    let files = SharedFiles::default();
    let mut run = PluginRun::new().with_files(Box::new(files.clone()));
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("tank")));
    run.scripted_world(
        "Tester",
        vec![ScriptedObject {
            id: PORTAL,
            name: "Portal to Rithwic".into(),
            obj_type: item_type::PORTAL,
            place: Some((42.2, 33.84)),
            ..ScriptedObject::default()
        }],
    );
    run.type_line("/tank route new once");
    run.type_line("/tank route add");
    run.place_player(42.2, 33.82);
    run.type_line("/tank route jump 90 false 500");
    run.select(PORTAL);
    run.type_line("/tank route add");
    run.type_line("/tank route save hop");
    run.advance(0.5);
    let printed = run.printed();
    let said = |s: &str| printed.iter().any(|l| l.contains(s));
    assert!(
        said("Route: waypoint 3: portal Portal to Rithwic")
            && said("Route: saved routes/hop.nav (3 waypoints)"),
        "{printed:#?}"
    );
    let text = files
        .clone()
        .read("network.dereth.tank", "routes/hop.nav")
        .expect("the route was saved");
    let route = dereth_plugin_host::navfile::parse(&String::from_utf8_lossy(&text))
        .expect("the saved route reads as a VTank route");
    let points = route.get("points").as_list().expect("points").to_vec();
    let kinds: Vec<_> = points
        .iter()
        .map(|p| p.get("kind").as_str().unwrap_or("?").to_owned())
        .collect();
    assert_eq!(route.get("kind").as_str(), Some("once"));
    assert_eq!(kinds, ["point", "jump", "portal"]);
    assert_eq!(points[1].get("heading").as_f64(), Some(90.0));
    assert_eq!(points[1].get("ms").as_f64(), Some(500.0));
    assert_eq!(points[2].get("name").as_str(), Some("Portal to Rithwic"));

    run.place_player(42.2, 33.8);
    run.type_line("/tank route hop.nav");
    run.type_line("/tank start");
    let used = format!("use {PORTAL:#x}");
    for _ in 0..60 {
        if run.sent().contains(&used) {
            break;
        }
        run.advance(0.5);
    }
    assert!(
        run.sent().contains(&used),
        "the portal was used: {:?} (console {:#?})",
        run.sent(),
        run.console("network.dereth.tank")
    );
    let moves = run.moves();
    assert!(
        moves.contains(&"hold jump".to_owned()) && moves.contains(&"leap 0.5 s up".to_owned()),
        "the jump waypoint jumped: {moves:?}"
    );
    run.teleport(42.25, 33.9);
    run.advance(3.0);
    let console = run.console("network.dereth.tank");
    let logged = |s: &str| console.iter().any(|l| l.contains(s));
    assert!(
        logged("route: jumped at waypoint 2 (heading 90, 500 ms)")
            && logged("route: through Portal to Rithwic")
            && logged("route: finished"),
        "{console:#?}"
    );
    note_asserted(
        "plugins.samples.the-tank-makes-a-route-in-the-game-and-walks-it-jumping-and-through-its-portal",
    );
}

/// The Tank answers VTank's route and jump commands as a meta or the player types them (a jump
/// waypoint added, a jump with a sidestep, the route saved and reversed).
fn the_tank_answers_vtanks_route_and_jump_commands() {
    use dereth_plugin_host::PluginFiles;
    let files = SharedFiles::default();
    let mut run = PluginRun::new().with_files(Box::new(files.clone()));
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("tank")));
    run.scripted_world("Tester", Vec::new());
    run.type_line("/tank route new linear");
    run.type_line("/tank vt addnavpt 42.2N, 33.81E");
    run.type_line("/tank vt addnavjump 90 true 300");
    run.type_line("/tank vt nav save vt");
    run.advance(0.5);
    let text = files
        .clone()
        .read("network.dereth.tank", "routes/vt.nav")
        .expect("the route was saved");
    let route = dereth_plugin_host::navfile::parse(&String::from_utf8_lossy(&text))
        .expect("it reads as a VTank route");
    let points = route.get("points").as_list().expect("points").to_vec();
    assert_eq!(route.get("kind").as_str(), Some("linear"));
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].get("position").get("ew").as_f64(), Some(33.81));
    assert_eq!(points[1].get("kind").as_str(), Some("jump"));
    assert_eq!(points[1].get("shift").as_bool(), Some(true));
    assert_eq!(points[1].get("ms").as_f64(), Some(300.0));
    run.type_line("/tank vt reverseroute true");
    run.type_line("/tank route points");
    run.type_line("/tank vt jump 180 false 200 strafeleft");
    run.advance(2.0);
    let printed = run.printed();
    assert!(
        printed
            .iter()
            .any(|l| l == "[Tank] Route: linear (run reversed), 2 waypoints: point, jump"),
        "{printed:#?}"
    );
    let moves = run.moves();
    let has = |m: &str| moves.iter().any(|x| x == m);
    assert!(
        has("face 180") && has("hold strafe_left") && has("leap 0.2 s strafe_left"),
        "{moves:?}"
    );
    note_asserted("plugins.samples.the-tank-answers-vtanks-route-and-jump-commands");
}

/// The Tank reads a VTank settings profile from its data folder into its settings and monster
/// list (a row's damage type at VTank's default, 8, read as auto, and one whose number VTank does
/// not publish as auto too), says which of the profile's settings it does not have, takes its route
/// from the data folder, and saves its own settings as a profile that reads back the same, a row's
/// damage type its own.
fn the_tank_reads_and_writes_vtank_settings_profiles() {
    use dereth_plugin_host::PluginFiles;
    let usd = [
        "2",
        "Settings",
        "2",
        "Setting",
        "Value",
        "y",
        "n",
        "3", //
        "s",
        "AttackDistance",
        "d",
        "0.05", //
        "s",
        "EnableNav",
        "b",
        "False", //
        "s",
        "WhoYouGonnaCall",
        "i",
        "3", //
        "MyMonsters",
        "5",
        "MonsterName",
        "AttackPriority",
        "DamageType",
        "Attack",
        "Imperil", //
        "y",
        "n",
        "n",
        "n",
        "n",
        "2", //
        "s",
        "<DEFAULT>",
        "i",
        "2",
        "i",
        "16",
        "b",
        "True",
        "b",
        "False", //
        "s",
        "Drudge Skulker",
        "i",
        "4",
        "i",
        "8",
        "b",
        "True",
        "b",
        "True",
        "",
    ]
    .join("\r\n");
    let files = SharedFiles::default();
    let mut put = files.clone();
    put.write("network.dereth.tank", "my.usd", usd.as_bytes())
        .expect("in memory");
    let route = std::fs::read(samples().join("tank/routes/holtburg-loop.nav")).expect("a route");
    put.write("network.dereth.tank", "routes/mine.nav", &route)
        .expect("in memory");
    let mut run = PluginRun::new().with_files(Box::new(files.clone()));
    run.load(read_package(&samples().join("tank")));
    run.scripted_world("Tester", Vec::new());
    run.type_line("/tank import my.usd");
    run.type_line("/tank route mine.nav");
    run.type_line("/tank monsters");
    run.type_line("/tank monster damage fire Drudge Skulker");
    run.type_line("/tank export saved.usd");
    run.advance(0.5);
    let printed = run.printed();
    let said = |s: &str| printed.iter().any(|l| l.contains(s));
    assert!(
        said("2 settings and 2 monster rows read; 0 combat items, 0 consumables, 0 buffs added, 0 left out; 1 settings this macro does not have (WhoYouGonnaCall)"),
        "{printed:#?}"
    );
    assert!(
        said("Drudge Skulker: priority 4, attack, damage auto, debuffs imperil"),
        "{printed:#?}"
    );
    assert!(
        said("<DEFAULT>: priority 2, attack, damage auto"),
        "{printed:#?}"
    );
    assert!(
        run.console("network.dereth.tank")
            .iter()
            .any(|l| l.contains("route mine.nav: circular, 4 waypoints")),
        "{:#?}",
        run.console("network.dereth.tank")
    );
    let saved = put
        .read("network.dereth.tank", "saved.usd")
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .expect("the profile was saved");
    for want in ["AttackDistance", "0.05", "EnableNav", "Drudge Skulker"] {
        assert!(saved.contains(want), "{want} in {saved}");
    }
    // What was saved reads back the same.
    let mut again = PluginRun::new().with_files(Box::new(files));
    again.load(read_package(&samples().join("tank")));
    again.scripted_world("Tester", Vec::new());
    again.type_line("/tank import saved.usd");
    again.type_line("/tank monsters");
    again.advance(0.5);
    let printed = again.printed();
    assert!(
        printed
            .iter()
            .any(|l| l.contains("Drudge Skulker: priority 4, attack, damage fire, debuffs imperil")),
        "{printed:#?}"
    );
    note_asserted("plugins.samples.the-tank-reads-and-writes-vtank-settings-profiles");
}

/// The Tank runs a VTank meta on top of itself: the meta's "set option" turns combat off (so the
/// monster beside the character is not fought), the VTank command it types loads a route the
/// macro then walks, and its next state sets another option.
fn the_tank_is_driven_by_a_vtank_meta() {
    use dereth_plugin_host::PluginFiles;
    use dereth_rules::weenie::{bitfield, item_type};
    use dereth_testkit::plugins::ScriptedObject;
    const MONSTER: u32 = 0x8000_0080;
    // A meta in the file's own layout: rules of (condition, action, state), each cell a type tag
    // and its value; a k/v table is named fields, a K/V table a list of typed entries.
    let kv = |pairs: &[(&str, &str)]| {
        let mut l = vec!["TABLE", "2", "k", "v", "n", "n"];
        let n = pairs.len().to_string();
        let mut out: Vec<String> = l.drain(..).map(str::to_owned).collect();
        out.push(n);
        for (k, v) in pairs {
            out.extend(["s", k, "s", v].map(str::to_owned));
        }
        out
    };
    let mut met: Vec<String> = [
        "1", "CondAct", "5", "CType", "AType", "CData", "AData", "State", "n", "n", "n", "n", "n",
        "2",
    ]
    .map(str::to_owned)
    .to_vec();
    // Default: always -> set EnableCombat false; type /vt nav load; go to Walk.
    met.extend(["i", "1", "i", "3", "i", "0"].map(str::to_owned));
    met.extend(["TABLE", "2", "K", "V", "n", "n", "3"].map(str::to_owned));
    met.extend(["i", "12"].map(str::to_owned));
    met.extend(kv(&[("o", "EnableCombat"), ("v", "false")]));
    met.extend(["i", "2", "s", "/vt nav load holtburg-loop.nav"].map(str::to_owned));
    met.extend(["i", "1", "s", "Walk"].map(str::to_owned));
    met.extend(["s", "Default"].map(str::to_owned));
    // Walk: always -> set EnableLooting false.
    met.extend(["i", "1", "i", "12", "i", "0"].map(str::to_owned));
    met.extend(kv(&[("o", "EnableLooting"), ("v", "false")]));
    met.extend(["s", "Walk"].map(str::to_owned));
    let text = met.join("\r\n") + "\r\n";
    let files = SharedFiles::default();
    files
        .clone()
        .write("network.dereth.tank", "metas/test.met", text.as_bytes())
        .expect("in memory");
    let mut run = PluginRun::new().with_files(Box::new(files));
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("tank")));
    run.scripted_world(
        "Tester",
        vec![ScriptedObject {
            id: MONSTER,
            name: "Drudge Skulker".into(),
            obj_type: item_type::CREATURE,
            bitfield: bitfield::ATTACKABLE,
            place: Some((42.2, 33.8001)),
            ..ScriptedObject::default()
        }],
    );
    run.type_line("/tank meta test.met");
    run.type_line("/tank start");
    run.advance(5.0);
    let console = run.console("network.dereth.tank");
    let said = |s: &str| console.iter().any(|l| l.contains(s));
    assert!(said("meta test.met: 2 states"), "{console:#?}");
    assert!(said("meta: enablecombat = false"), "{console:#?}");
    assert!(
        said("route holtburg-loop.nav: circular, 4 waypoints"),
        "{console:#?}"
    );
    assert!(said("meta: Default -> Walk"), "{console:#?}");
    assert!(said("meta: enablelooting = false"), "{console:#?}");
    assert!(said("route: starting at waypoint"), "{console:#?}");
    assert!(
        !run.sent().iter().any(|s| s.contains("attack")),
        "combat is off: {:?}",
        run.sent()
    );
    note_asserted("plugins.samples.the-tank-is-driven-by-a-vtank-meta");
}

// The `#[test]` beside each, which runs it and checks its declaration.

fn a_plugin_reads_each_message_decoded_and_can_send_none() {
    core(
        "messages",
        "plugins.messages.a-plugin-reads-each-message-decoded-and-can-send-none",
    );
}

fn a_plugin_hears_objects_change_and_move_and_finds_them_by_their_properties() {
    core(
        "objects",
        "plugins.objects.a-plugin-hears-objects-change-and-move-and-finds-them-by-their-properties",
    );
}

fn a_plugin_reads_the_characters_fellowship_titles_augmentations_bars_and_options() {
    use dereth_protocol::social::{
        CharacterTitlesMessage, Fellow, Fellowship, FellowshipFullUpdate, FellowshipLocks,
    };
    let main = r#"
on("character.fellowship", function(e)
	print("fellowship", e.change, e.name, #e.members)
end)
commands.add("show", "the character", function()
	local f = game.player.fellowship
	local fellow = f.members[2]
	print("members", #f.members, f.name, f.leader == game.player.id, f.share_xp)
	print("fellow", fellow.name, fellow.level, fellow.health.current, fellow.health.max, fellow.mana.max)
	print("fellow health", game.world:get(fellow.id) == nil)
	local t = game.player.titles
	print("titles", t.current, #t.list)
	local a = game.player.augmentations
	print("augmentations", a.increased_carrying_capacity, a.bonus_xp, a.stat)
	local bars = game.player.spell_bars
	print("bars", #bars, #bars[1], bars[1][1], bars[2][1])
	local o = game.player.options
	print("options", type(o.auto_repeat_attack), o.view_combat_target ~= nil)
	print("attributes", #game.player.attributes, game.player.attributes[1].name)
	print("allegiance", game.player.allegiance, #game.player.quests)
end)
"#;
    let package = Package {
        origin: "character".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.character\"\nname = \"Character\"\nversion = \"1.0.0\"\napi = \"0.6\"\n"
                    .to_vec(),
            ),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into(),
    };
    let mut run = PluginRun::new();
    run.load(package);
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    let me = dereth_testkit::plugins::SCRIPTED_PLAYER;
    let fellow = |name: &str, level: u32, health: u32| Fellow {
        name: name.into(),
        level,
        max_health: 100,
        max_stamina: 120,
        max_mana: 80,
        current_health: health,
        current_stamina: 120,
        current_mana: 80,
        ..Fellow::default()
    };
    run.server_says(&[
        dereth_testkit::plugins::ui_event(&FellowshipFullUpdate(Fellowship {
            members: dereth_protocol::archive::PackedHash {
                table_size: 16,
                entries: vec![
                    (me, fellow("Tester", 10, 100)),
                    (0x5000_0099, fellow("Friend", 12, 40)),
                ],
            },
            name: "Hunters".into(),
            leader: dereth_primitives::ObjectId(me),
            share_xp: 1,
            even_xp_split: 0,
            open_fellow: 1,
            locked: 0,
            fellows_departed: dereth_protocol::archive::PackedHash::default(),
            locks: FellowshipLocks::default(),
        })),
        dereth_testkit::plugins::ui_event(&CharacterTitlesMessage {
            version: 1,
            display_title: 7,
            titles: vec![7, 12, 30],
        }),
    ]);
    {
        use dereth_client_model::{StatKey, StatType, StatValue};
        let w = run.world_mut();
        if let Some(q) = w.player_qualities_mut() {
            q.set(StatKey::new(StatType::Int, 230), StatValue::Int(1));
            q.set(StatKey::new(StatType::Int, 234), StatValue::Int(3));
        }
        w.player_system.spell_tabs = vec![vec![1, 2], vec![1133], Vec::new()];
    }
    run.type_line("/show");
    run.advance(0.5);
    assert_eq!(
        run.console("test.character"),
        [
            "fellowship\tjoined\tHunters\t2",
            "members\t2\tHunters\ttrue\ttrue",
            "fellow\tFriend\t12\t40\t100\t80",
            "fellow health\ttrue",
            "titles\t7\t3",
            "augmentations\t1\t3\tnil",
            "bars\t8\t2\t1\t1133",
            "options\tboolean\ttrue",
            "attributes\t6\tstrength",
            "allegiance\tnil\t0",
        ]
    );
    note_asserted(
        "plugins.character.a-plugin-reads-the-characters-fellowship-titles-augmentations-bars-and-options",
    );
}

fn utilitybelts_additions_to_the_expression_language_evaluate_as_documented() {
    assert_case("automation", "expressions-ub");
    note_asserted(
        "plugins.automation.utilitybelts-additions-to-the-expression-language-evaluate-as-documented",
    );
}

fn a_plugin_draws_lines_rings_squares_text_and_nametags_in_the_world() {
    assert_case("overlay", "shapes");
    note_asserted(
        "plugins.overlay.a-plugin-draws-lines-rings-squares-text-and-nametags-in-the-world",
    );
}

fn a_canvas_and_a_chart_draw_what_a_plugin_gives_them() {
    assert_case("ui", "canvas");
    note_asserted("plugins.ui.a-canvas-and-a-chart-draw-what-a-plugin-gives-them");
}

fn text_inputs_sliders_rich_list_cells_tooltips_and_toasts_work_as_declared() {
    assert_case("ui", "inputs");
    note_asserted(
        "plugins.ui.text-inputs-sliders-rich-list-cells-tooltips-and-toasts-work-as-declared",
    );
}

fn a_hotkey_runs_its_plugins_command_as_a_gesture() {
    assert_case("ui", "hotkeys");
    note_asserted("plugins.ui.a-hotkey-runs-its-plugins-command-as-a-gesture");
}

fn a_plugin_calls_a_service_another_plugin_offers() {
    let package = |id: &str, main: &str| Package {
        origin: id.into(),
        files: [
            (
                "plugin.toml".to_owned(),
                format!("id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\napi = \"0.6\"\n")
                    .into_bytes(),
            ),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into(),
    };
    let provider = r#"
services.offer("double", function(args, call)
	return { twice = args.n * 2, from = call.from }
end)
services.offer("fail", function() error("no can do") end)
services.offer("private", function() return 1 end, { allow = { "test.nobody" } })
"#;
    let caller = r#"
commands.add("go", "call the services", function()
	task.spawn(function()
		local list = services.list()
		print("listed", #list)
		local r = await(services.call("test.provider", "double", { n = 21 }))
		print("double", r.twice, r.from)
		print("fail", pawait(services.call("test.provider", "fail")))
		print("private", pawait(services.call("test.provider", "private")))
		print("missing", pawait(services.call("test.provider", "nothing")))
		print("nobody", pawait(services.call("test.gone", "double")))
		print("tank state", pawait(services.call("network.dereth.tank", "set_meta_state", { state = "Hunt" })))
		print("tank setting", await(services.call("network.dereth.tank", "get_setting", "EnableMeta")))
		print("tank running", await(services.call("network.dereth.tank", "running")))
	end)
end)
"#;
    let mut run = PluginRun::new();
    run.load(package("test.provider", provider));
    run.load(package("test.caller", caller));
    run.load(read_package(&samples().join("tank")));
    run.scripted_world("Tester", Vec::new());
    run.advance(1.0);
    run.type_line("/go");
    run.advance(2.0);
    let console = run.console("test.caller");
    // The Tank's refusal names the line of its main.lua that raised it, wherever that line is.
    let tank_main =
        std::fs::read_to_string(samples().join("tank/main.lua")).expect("the Tank's main.lua");
    let refusal_line = tank_main
        .lines()
        .position(|l| l.contains("\"no meta is loaded\""))
        .expect("the Tank refuses a state with no meta loaded")
        + 1;
    assert_eq!(
        console,
        [
            "listed\t8".to_owned(),
            "double\t42\ttest.caller".to_owned(),
            "fail\tfalse\ttest.provider: main.lua:5: no can do".to_owned(),
            "private\tfalse\tnot_allowed".to_owned(),
            "missing\tfalse\tno_service".to_owned(),
            "nobody\tfalse\tno_plugin".to_owned(),
            format!(
                "tank state\tfalse\tnetwork.dereth.tank: main.lua:{refusal_line}: no meta is loaded"
            ),
            "tank setting\ttrue".to_owned(),
            "tank running\tfalse".to_owned(),
        ],
        "{console:?}"
    );
    note_asserted("plugins.services.a-plugin-calls-a-service-another-plugin-offers");
}

fn a_file_the_player_picks_reaches_the_plugin_and_nothing_else_does() {
    let package = |id: &str, main: &str| Package {
        origin: id.into(),
        files: [
            (
                "plugin.toml".to_owned(),
                format!("id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\napi = \"0.6\"\n")
                    .into_bytes(),
            ),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into(),
    };
    use dereth_plugin_host::MemoryFiles;
    let main = r#"
commands.add("pick", "ask for a meta", function()
	task.spawn(function()
		local ok, f = pawait(files.import({ prompt = "a meta to run", extensions = { "met" } }))
		print("first", ok, ok and f.name or f, ok and #f.text or "")
		print("second", pawait(files.import({ prompt = "again" })))
	end)
end)
"#;
    let mut files = MemoryFiles::default();
    for (path, text) in [
        ("import/a.met", "abc"),
        ("import/b.nav", "xyz"),
        ("import/c.met", "12345"),
    ] {
        files.files.insert(
            ("dereth.plugins".into(), path.into()),
            text.as_bytes().to_vec(),
        );
    }
    let mut run = PluginRun::new().with_files(Box::new(files));
    run.load(package("test.picker", main));
    run.scripted_world("Tester", Vec::new());
    run.type_line("/pick");
    run.advance(0.5);
    let asked = run.host().question().expect("the client asks the player");
    assert_eq!(
        asked.picks,
        ["a.met", "c.met"],
        "the files the request takes"
    );
    run.answer(true, Some("c.met"));
    run.advance(0.5);
    run.answer(false, None);
    run.advance(0.5);
    assert_eq!(
        run.console("test.picker"),
        ["first\ttrue\tc.met\t5", "second\tfalse\trefused"]
    );
    assert_eq!(run.host().question(), None, "the question goes");
    note_asserted("plugins.files.a-file-the-player-picks-reaches-the-plugin-and-nothing-else-does");
}

fn a_plugin_hides_and_moves_the_games_panels_and_lets_go_when_it_unloads() {
    let package = |id: &str, main: &str| Package {
        origin: id.into(),
        files: [
            (
                "plugin.toml".to_owned(),
                format!("id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\napi = \"0.6\"\n")
                    .into_bytes(),
            ),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into(),
    };
    let main = r#"
commands.add("arrange", "move the panels", function()
	retail_ui.hide("radar")
	retail_ui.move("chat", 10, 400)
	retail_ui.set("toolbar", { hidden = false, x = 200, y = 0 })
	retail_ui.reset("toolbar")
	print("bad", pcall(retail_ui.hide, "minimap"))
end)
"#;
    // The conformance host draws no game panels; this run offers them.
    let mut run = PluginRun::with_config(|c| {
        c.capabilities
            .push(dereth_plugin_api::HostCapability::RetailUi);
    });
    run.load(package("test.arranger", main));
    run.scripted_world("Tester", Vec::new());
    run.type_line("/arrange");
    run.advance(0.2);
    assert_eq!(
        run.retail_ui(),
        [
            "chat test.arranger hidden=- at=10,400",
            "radar test.arranger hidden=true at=-",
        ]
    );
    let bad = run.console("test.arranger");
    assert!(
        bad.len() == 1 && bad[0].starts_with("bad\tfalse"),
        "{bad:?}"
    );
    run.unload("test.arranger");
    assert!(run.retail_ui().is_empty(), "{:?}", run.retail_ui());
    note_asserted(
        "plugins.ui.a-plugin-hides-and-moves-the-games-panels-and-lets-go-when-it-unloads",
    );
}

fn a_mark_may_stay_on_top_of_what_stands_in_front_and_nametags_keep_to_the_nearest() {
    assert_case("overlay", "depth");
    note_asserted(
        "plugins.overlay.a-mark-may-stay-on-top-of-what-stands-in-front-and-nametags-keep-to-the-nearest",
    );
}

/// The spec's case, and on a host that tells no presses on its map: there a plugin's listening
/// for them is refused, and a press reaches no one.
fn a_press_on_the_map_is_heard_where_it_falls() {
    use dereth_client_contract::panels::map::{MapButton, MapClick};
    assert_case("overlay", "map-click");
    let mut run = PluginRun::with_config(|c| {
        c.capabilities
            .retain(|c| *c != dereth_plugin_api::HostCapability::MapClicks);
    });
    run.load(Package {
        origin: "deaf".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.deaf\"\nname = \"Deaf\"\nversion = \"1.0.0\"\napi = \"0.7\"\n"
                    .to_vec(),
            ),
            (
                "main.lua".to_owned(),
                b"print(pcall(on, 'map.clicked', function() print('heard') end))\n".to_vec(),
            ),
        ]
        .into(),
    });
    run.scripted_world("Tester", Vec::new());
    run.map_click(&MapClick {
        ns: 42.3,
        ew: 33.8,
        button: MapButton::Left,
        ctrl: false,
        shift: false,
        alt: false,
    });
    let said = run.console("test.deaf");
    assert!(
        said.len() == 1 && said[0].starts_with("false") && said[0].contains("map.clicks"),
        "the listening is refused, and the press heard by no one: {said:?}"
    );
    note_asserted("plugins.overlay.a-press-on-the-map-is-heard-where-it-falls");
}

/// The cases about an entry's time budget run at the host's own budget, timed as each entry runs:
/// the runaway plugin is stopped by it until it is suspended, in a small part of the time one
/// unhurried entry may take (an unhurried run of the case takes three of those).
#[test]
fn a_budget_case_stops_its_runaway_plugin_at_the_hosts_own_budget() {
    use dereth_testkit::conformance::BUDGET_CASES;
    use dereth_testkit::plugins::UNHURRIED_BUDGET_MS;
    assert!(!BUDGET_CASES.is_empty());
    for c in BUDGET_CASES {
        let (profile, name) = c.split_once('/').expect("a budget case is profile/case");
        let started = std::time::Instant::now();
        let out = assert_case(profile, name);
        let took = started.elapsed();
        assert_eq!(out.status, "suspended", "{c}");
        assert!(
            took.as_secs_f64() * 1000.0 < UNHURRIED_BUDGET_MS,
            "{c} took {took:?}: its entries ran unhurried"
        );
    }
}

/// Every other run gives an entry far longer than the host's own budget: a handler that works for
/// many of the host's budgets runs to its end, and the plugin is not faulted for it.
#[test]
fn a_run_lets_an_entry_run_far_past_the_hosts_own_budget() {
    let mut run = PluginRun::new();
    run.load(Package {
        origin: "long".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.long\"\nname = \"Long\"\nversion = \"1.0.0\"\napi = \"0.4\"\ncontexts = [\"pregame\", \"world\"]\n"
                    .to_vec(),
            ),
            (
                "main.lua".to_owned(),
                b"task.after(100, function()\n\tlocal n = 0\n\tfor j = 1, 2000000 do n = n + j end\n\tprint(\"done\", n)\nend)\n"
                    .to_vec(),
            ),
        ]
        .into(),
    });
    run.advance(0.5);
    assert_eq!(run.console("test.long"), ["done\t2000001000000"]);
    assert_eq!(run.status("test.long"), "running");
}

/// The spec's event table and the events the host delivers are the same list, in the same order.
#[test]
fn the_specs_event_table_is_the_list_the_host_delivers() {
    let path = dereth_testkit::conformance::workspace_root().join("docs/plugins/events.md");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let documented: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("| `"))
        .filter_map(|l| l.split_once('`').map(|(name, _)| name))
        .collect();
    let implemented: Vec<&str> = dereth_plugin_api::event::EVENTS
        .iter()
        .map(|(n, _)| *n)
        .collect();
    assert_eq!(
        documented, implemented,
        "docs/plugins/events.md's table against the host's events"
    );
}

/// The compiled-plugin interface (WIT) and the Luau definitions in the spec are what the API's
/// schema generates, and each Lua function the schema names is a function of the Lua API.
/// `DERETH_TEST_GOLDEN=1` rewrites the two files.
#[test]
fn the_wit_interface_and_the_luau_definitions_are_what_the_schema_generates() {
    use dereth_plugin_api::schema;
    let root = dereth_testkit::conformance::workspace_root();
    for (file, text) in [
        ("docs/plugins/wit/dereth-plugin.wit", schema::wit()),
        ("docs/plugins/dereth.d.luau", schema::luau_definitions()),
    ] {
        let path = root.join(file);
        if dereth_testkit::golden::rewriting() {
            std::fs::create_dir_all(path.parent().expect("a folder")).expect("the folder");
            std::fs::write(&path, &text).expect("written");
            continue;
        }
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            on_disk == text,
            "{file} is not what the schema generates; DERETH_TEST_GOLDEN=1 rewrites it"
        );
    }
    let mut main: String = schema::OPS
        .iter()
        .filter_map(|o| o.lua)
        .map(|l| format!("assert(type({l}) == \"function\", \"{l}\")\n"))
        .collect();
    main.push_str("print(\"all there\")\n");
    let package = Package {
        origin: "schema".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.schema\"\nname = \"Schema\"\nversion = \"1.0.0\"\napi = \"0.4\"\n"
                    .to_vec(),
            ),
            ("main.lua".to_owned(), main.into_bytes()),
        ]
        .into(),
    };
    let mut run = PluginRun::new();
    run.load(package);
    run.frame();
    assert_eq!(run.console("test.schema"), ["all there"]);
}

/// The spec's plugin reference (`docs/plugins/reference/`) is what the plugin interface's code
/// generates: the host operations, the permissions, the host capabilities, the UI node kinds, the
/// host's limits and the conformance cases. `DERETH_TEST_GOLDEN=1` rewrites the pages.
#[test]
fn the_plugin_reference_is_what_the_code_generates() {
    use dereth_testkit::plugin_reference::{pages, REFERENCE_DIR};
    let dir = dereth_testkit::conformance::workspace_root().join(REFERENCE_DIR);
    for (file, text) in pages() {
        let path = dir.join(file);
        if dereth_testkit::golden::rewriting() {
            std::fs::create_dir_all(&dir).expect("the folder");
            std::fs::write(&path, &text).expect("written");
            continue;
        }
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            on_disk == text,
            "{REFERENCE_DIR}/{file} is not what the code generates; DERETH_TEST_GOLDEN=1 rewrites it"
        );
    }
}

/// The compiled test plugin's `plugin.wasm` was built from the sources beside it: the source hash
/// the author's tool wrote into it is the hash of the crate's files as they are. Rebuild it with
/// `cargo run -p dereth-plugin -- build dereth/testkit/tests/fixtures/compiled-plugin/source
/// dereth/testkit/tests/fixtures/compiled-plugin/package`.
#[test]
fn the_compiled_test_plugins_module_is_built_from_its_sources() {
    use dereth_plugin_host::source;
    let root = compiled_fixture();
    let wasm = std::fs::read(root.join("package/plugin.wasm")).expect("the module");
    // The component's own sections: an id, a LEB128 size and the payload; a custom one (id 0)
    // starts with its LEB128-sized name.
    let leb = |b: &[u8], at: &mut usize| -> usize {
        let (mut n, mut shift) = (0usize, 0);
        loop {
            let byte = b[*at];
            *at += 1;
            n |= usize::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return n;
            }
            shift += 7;
        }
    };
    let mut at = 8;
    let mut claimed = None;
    while at < wasm.len() {
        let id = wasm[at];
        at += 1;
        let size = leb(&wasm, &mut at);
        let end = at + size;
        if id == 0 {
            let mut p = at;
            let n = leb(&wasm, &mut p);
            if &wasm[p..p + n] == source::SOURCE_SECTION.as_bytes() {
                claimed = Some(String::from_utf8_lossy(&wasm[p + n..end]).into_owned());
            }
        }
        at = end;
    }
    let krate = root.join("source");
    let mut files = std::collections::BTreeMap::new();
    let mut stack = vec![std::path::PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for e in std::fs::read_dir(krate.join(&rel)).expect("the crate") {
            let e = e.expect("an entry");
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || (rel.as_os_str().is_empty() && name == "target") {
                continue;
            }
            let path = rel.join(&name);
            if e.file_type().expect("a type").is_dir() {
                stack.push(path);
            } else {
                let key = path
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                files.insert(key, std::fs::read(krate.join(&path)).expect("a file"));
            }
        }
    }
    assert_eq!(
        claimed.as_deref(),
        Some(source::hex(&source::source_hash(&files)).as_str()),
        "the compiled test plugin's plugin.wasm is not built from its source as it is"
    );
}

/// The conformance folders hold exactly the cases these scenarios run: a case nobody runs is a
/// claim nobody checks.
#[test]
fn every_core_case_is_run_by_a_scenario() {
    let source = include_str!("plugins.rs");
    for profile in [
        "core",
        "ui",
        "overlay",
        "nav",
        "pregame",
        "peers",
        "automation",
    ] {
        for c in dereth_testkit::conformance::cases(profile) {
            assert!(
                source.contains(&format!("\"{}\"", c.name)),
                "no scenario runs the {profile} case {}",
                c.name
            );
        }
    }
}
