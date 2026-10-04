//! The world in motion: what a moving body reports, what the shard's corrections do to it, what
//! the physics refuses, what the camera slides through, what a click answers, and the map, radar,
//! door and stance behaviour built on top of them. Fixture: the retail dats (the real land, cells
//! and body setups) and, for the claims about retail's own output, the recorded sessions read
//! through the corpus or replayed. Most scenarios open the dats themselves, drive a body a
//! physics sub-step at a time, and book the claim with `assert_behaviour` on a model client; the
//! rest build a whole client through [`dereth_testkit::ClientSpec::retail`]. This binary runs
//! serially: two headless clients in one process share the UI request globals. `ALL` feeds
//! `census.rs`. Nothing here binds a socket or sends a datagram.

use dereth_testkit::{ClientSpec, HeadlessClient};
// The map and radar scenarios reach the shipped gameplay screen through its
// own trait, which several of them and the fixtures below all need in scope.
use dereth_ui::framework::Screen as _;

use support::{
    body_origin, body_position, create_event, fresh_local_body, give_part_array, position_event,
    settled_at, store, xy_gap, CREATURE, DT, IMPENETRABLE, PK, PK_LITE, PLAYER, STEP,
};

dereth_testkit::scenarios! {
    scenario_running_reports_walk_with_the_run_hold_key => running_reports_walk_with_the_run_hold_key ["movement.run.reports-walk-with-the-run-hold-key"],
    #[cfg(windows)]
    scenario_enter_world_refuses_a_body_inside_a_building => enter_world_refuses_a_body_inside_a_building ["physics.enter-world.refuses-a-body-inside-a-building"],
    scenario_a_teleport_onto_another_player_is_deflected_only_by_a_creature => a_teleport_onto_another_player_is_deflected_only_by_a_creature ["movement.teleport.arrives-exactly-on-another-player-and-is-pushed-aside-only-by-a-creature"],
    scenario_an_arrival_through_the_login_path_lands_on_the_other_players_spot => an_arrival_through_the_login_path_lands_on_the_other_players_spot ["movement.teleport.a-player-arriving-on-another-player-through-the-shards-own-login-is-not-deflected"],
    scenario_two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt => two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt ["movement.teleport.two-players-pass-through-each-other-only-while-neither-is-marked-for-combat"],
    scenario_an_update_with_no_ground_contact_moves_the_body_nowhere => an_update_with_no_ground_contact_moves_the_body_nowhere ["movement.correction.an-update-that-does-not-say-the-body-is-on-the-ground-moves-it-nowhere"],
    scenario_a_create_onto_an_occupied_spot_is_put_down_beside_it => a_create_onto_an_occupied_spot_is_put_down_beside_it ["movement.remote-body.a-create-onto-an-occupied-spot-is-put-down-beside-it"],
    scenario_a_nearby_correction_glides_the_body_over_the_following_frames => a_nearby_correction_glides_the_body_over_the_following_frames ["movement.correction.a-nearby-correction-glides-the-body-over-the-following-frames"],
    scenario_a_correction_out_of_reach_is_taken_in_one_step => a_correction_out_of_reach_is_taken_in_one_step ["movement.correction.a-correction-out-of-reach-is-taken-in-one-step-instead-of-a-glide"],
    scenario_a_body_not_yet_simulated_is_put_straight_onto_the_shards_position => a_body_not_yet_simulated_is_put_straight_onto_the_shards_position ["movement.correction.a-body-the-client-has-not-simulated-yet-is-put-straight-onto-the-shards-position"],
    scenario_a_teleport_out_of_the_world_drops_a_glide_in_flight => a_teleport_out_of_the_world_drops_a_glide_in_flight ["movement.correction.a-teleport-out-of-the-world-drops-a-glide-that-was-already-in-flight"],
    scenario_a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world => a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world ["movement.correction.a-destination-in-a-block-the-client-has-not-loaded-takes-the-body-out-of-the-world"],
    scenario_walking_forward_reports_a_walk_with_no_hold_key => walking_forward_reports_a_walk_with_no_hold_key ["movement.walk.reports-walking-forward-with-no-hold-key"],
    scenario_a_running_bodys_report_is_the_retail_clients_own => a_running_bodys_report_is_the_retail_clients_own ["movement.run.the-report-a-running-body-sends-is-the-one-the-retail-client-sent"],
    scenario_turning_run_on_mid_walk_speeds_the_body_and_changes_the_report => turning_run_on_mid_walk_speeds_the_body_and_changes_the_report ["movement.run.turning-run-on-while-already-walking-speeds-the-body-up-and-changes-the-report"],
    scenario_the_camera_slides_through_a_creature_but_not_through_a_solid_twin => the_camera_slides_through_a_creature_but_not_through_a_solid_twin ["camera.view.slides-through-a-creature-standing-between-you-and-the-camera"],
    scenario_a_scene_less_click_reports_nothing_picked => a_scene_less_click_reports_nothing_picked ["world.click.a-click-answered-with-nothing-in-view-reports-nothing-picked"],
    scenario_the_reported_state_is_the_bodys_own => the_reported_state_is_the_bodys_own ["movement.position-report.the-state-the-client-reports-is-the-bodys-own"],
    scenario_walking_forward_reports_the_state_the_recordings_carry => walking_forward_reports_the_state_the_recordings_carry ["movement.position-report.walking-forward-reports-the-state-the-recordings-carry"],
    scenario_a_walking_body_reports_where_it_is_and_a_still_one_falls_silent => a_walking_body_reports_where_it_is_and_a_still_one_falls_silent ["movement.position-report.a-walking-body-reports-where-it-is-and-a-still-one-falls-silent"],
    scenario_every_drawn_frame_reaches_the_position_reporter => every_drawn_frame_reaches_the_position_reporter ["movement.position-report.every-drawn-frame-reaches-the-reporter-and-invents-nothing"],
    scenario_a_jump_reports_the_bodys_own_position_and_speed => a_jump_reports_the_bodys_own_position_and_speed ["movement.jump.a-jump-puts-the-bodys-own-position-and-speed-on-the-wire"],
    scenario_a_second_jump_in_mid_air_sends_nothing => a_second_jump_in_mid_air_sends_nothing ["movement.jump.a-second-jump-in-mid-air-is-refused-and-nothing-goes-out"],
    scenario_a_running_jump_carries_its_speed_in_the_bodys_own_frame => a_running_jump_carries_its_speed_in_the_bodys_own_frame ["movement.jump.a-running-jump-carries-its-speed-in-the-bodys-own-frame"],
    scenario_the_clients_jump_height_explains_the_recorded_jumps => the_clients_jump_height_explains_the_recorded_jumps ["movement.jump.the-height-the-client-works-out-explains-the-recorded-jumps"],
    scenario_every_drawn_frame_visits_the_jump_dispatch => every_drawn_frame_visits_the_jump_dispatch ["movement.jump.every-drawn-frame-visits-the-jump-dispatch-and-invents-none"],
    scenario_a_drop_armed_with_nothing_drawn_is_answered => a_drop_armed_with_nothing_drawn_is_answered ["world.scene-less-frame.a-drop-armed-with-nothing-drawn-is-answered-and-the-next-click-still-works"],
    scenario_a_click_with_nothing_drawn_is_answered_too => a_click_with_nothing_drawn_is_answered_too ["world.scene-less-frame.a-click-and-a-double-click-are-answered-so-the-gestures-after-them-still-work"],
    scenario_a_frame_with_nothing_armed_answers_nothing => a_frame_with_nothing_armed_answers_nothing ["world.scene-less-frame.a-frame-with-nothing-armed-answers-nothing-and-clears-nothing"],
    scenario_the_scene_less_answer_names_nothing => the_scene_less_answer_names_nothing ["world.scene-less-frame.the-answer-names-nothing-so-a-dropped-item-takes-the-ground-leg"],
    scenario_a_delayed_script_call_plays_after_its_delay => a_delayed_script_call_plays_after_its_delay ["world.physics-script.a-delayed-call-plays-the-script-it-names-after-the-delay-it-rolled"],
    scenario_an_object_that_leaves_before_the_delay_plays_nothing => an_object_that_leaves_before_the_delay_plays_nothing ["world.physics-script.an-object-that-leaves-its-room-before-the-delay-is-up-plays-nothing"],
    scenario_a_script_call_with_no_delay_plays_on_the_spot => a_script_call_with_no_delay_plays_on_the_spot ["world.physics-script.a-call-with-no-delay-still-plays-on-the-spot"],
    scenario_a_scale_hook_moves_the_collision_radius => a_scale_hook_moves_the_collision_radius ["world.physics-script.a-scale-hook-moves-the-objects-own-collision-radius"],
    scenario_a_body_walking_at_a_scaled_object_stops_further_away => a_body_walking_at_a_scaled_object_stops_further_away ["world.physics-script.a-body-walking-at-a-scaled-object-stops-at-the-scaled-distance"],
    scenario_the_camera_against_a_wall_settles_the_same_at_any_tick_rate => the_camera_against_a_wall_settles_the_same_at_any_tick_rate ["camera.wall.settles-in-the-same-place-however-often-the-client-ticks"],
    scenario_a_key_release_stops_the_body_after_the_shard_has_taken_control => a_key_release_stops_the_body_after_the_shard_has_taken_control ["movement.server-control.a-key-release-stops-the-body-after-the-shard-has-taken-control"],
    scenario_a_turn_release_stops_the_turn_the_same_way => a_turn_release_stops_the_turn_the_same_way ["movement.server-control.a-turn-release-stops-the-turn-the-same-way"],
    scenario_the_player_can_move_again_after_the_shard_teleports_him => the_player_can_move_again_after_the_shard_teleports_him ["movement.teleport.the-player-can-move-again-after-the-shard-teleports-him"],
    scenario_a_press_that_takes_control_back_re_issues_the_key_already_held => a_press_that_takes_control_back_re_issues_the_key_already_held ["movement.server-control.a-press-that-takes-control-back-re-issues-the-key-already-held"],
    scenario_neither_an_approach_nor_a_target_update_leaves_a_motion_behind => neither_an_approach_nor_a_target_update_leaves_a_motion_behind ["movement.approach.neither-an-approach-nor-a-target-update-leaves-a-motion-behind"],
    scenario_a_teleport_with_no_shard_involvement_leaves_a_movable_body => a_teleport_with_no_shard_involvement_leaves_a_movable_body ["movement.teleport.a-teleport-with-no-shard-involvement-leaves-a-body-that-still-moves"],
    scenario_a_shard_authored_swing_holds_the_body_for_its_own_animation => a_shard_authored_swing_holds_the_body_for_its_own_animation ["movement.action.a-shard-authored-swing-holds-every-motion-behind-it-for-its-own-animation"],
    scenario_leaving_the_ground_empties_the_motion_ledger_and_routes_it => leaving_the_ground_empties_the_motion_ledger_and_routes_it ["movement.action.leaving-the-ground-empties-the-motion-ledger-and-routes-what-it-drained"],
    scenario_a_teleport_that_changes_the_ground_frees_the_body => a_teleport_that_changes_the_ground_frees_the_body ["movement.teleport.a-teleport-that-changes-the-ground-empties-the-ledger-and-the-body-moves-again"],
    scenario_a_teleport_onto_the_same_ground_raises_no_edge => a_teleport_onto_the_same_ground_raises_no_edge ["movement.teleport.a-teleport-onto-the-same-kind-of-ground-raises-no-edge-and-empties-nothing"],
    scenario_a_replacement_ends_the_old_approach => a_replacement_ends_the_old_approach ["movement.approach.a-replacement-ends-the-old-approach-before-the-new-one-is-installed"],
    scenario_a_key_press_ends_an_approach_on_the_spot => a_key_press_ends_an_approach_on_the_spot ["movement.approach.a-key-press-ends-an-approach-on-the-spot"],
    scenario_a_jump_ends_an_approach_whether_or_not_it_is_allowed => a_jump_ends_an_approach_whether_or_not_it_is_allowed ["movement.approach.a-jump-ends-an-approach-whether-or-not-the-jump-is-allowed"],
    scenario_the_old_approachs_clean_up_never_outlives_the_next => the_old_approachs_clean_up_never_outlives_the_next ["movement.approach.the-old-approachs-clean-up-never-outlives-the-next-approachs-subject"],
    scenario_every_way_a_follow_ends_reports_it_once => every_way_a_follow_ends_reports_it_once ["movement.follow.every-way-a-follow-ends-reports-it-once-and-leaves-the-body-free"],
    scenario_every_recorded_teleport_is_applied_and_no_other_position_is => every_recorded_teleport_is_applied_and_no_other_position_is ["movement.teleport.every-teleport-the-recordings-carry-is-applied-and-no-other-position-is"],
    scenario_a_recorded_teleport_moves_the_body_and_the_reports_follow => a_recorded_teleport_moves_the_body_and_the_reports_follow ["movement.teleport.a-recorded-teleport-moves-the-body-and-the-reports-name-the-destination"],
    scenario_several_teleports_in_a_row_each_move_the_body => several_teleports_in_a_row_each_move_the_body ["movement.teleport.several-teleports-in-a-row-each-move-the-body"],
    scenario_a_teleport_into_a_room_lands_the_body_on_its_floor => a_teleport_into_a_room_lands_the_body_on_its_floor ["movement.teleport.a-teleport-into-a-room-lands-the-body-on-its-floor"],
    scenario_every_drawn_frame_applies_a_teleport_before_reporting => every_drawn_frame_applies_a_teleport_before_reporting ["movement.teleport.every-drawn-frame-applies-a-teleport-before-it-reports-a-position"],
    scenario_a_full_ledger_stops_turning_and_walking_alike => a_full_ledger_stops_turning_and_walking_alike ["movement.motion-ledger.gates-turning-and-walking-alike-and-a-fresh-key-press-escapes-it"],
    scenario_a_charged_jump_turns_but_does_not_walk => a_charged_jump_turns_but_does_not_walk ["movement.jump-charge.a-charged-jump-turns-but-does-not-walk-until-the-body-leaves-the-ground"],
    scenario_a_placement_inside_a_building_takes_and_raises_its_footing_change => a_placement_inside_a_building_takes_and_raises_its_footing_change ["movement.placement.a-placement-inside-a-building-takes-and-raises-the-change-of-footing-it-implies"],
    scenario_a_body_that_loses_its_footing_with_no_edge_turns_for_ever => a_body_that_loses_its_footing_with_no_edge_turns_for_ever ["movement.contact.a-body-that-loses-its-footing-with-nothing-to-announce-it-turns-for-ever"],
    scenario_no_door_journey_leaves_a_body_that_turns_but_cannot_walk => no_door_journey_leaves_a_body_that_turns_but_cannot_walk ["movement.door.no-way-of-opening-a-door-leaves-a-body-that-turns-but-cannot-walk"],
    scenario_outdoors_the_map_shows_the_date_the_coordinates_and_the_mark => outdoors_the_map_shows_the_date_the_coordinates_and_the_mark ["map.window.outdoors-it-shows-the-date-the-coordinates-and-the-players-own-mark"],
    scenario_a_second_look_at_an_unchanged_world_writes_nothing => a_second_look_at_an_unchanged_world_writes_nothing ["map.window.a-second-look-at-an-unchanged-world-writes-nothing"],
    scenario_the_players_mark_follows_him_east_right_and_north_up => the_players_mark_follows_him_east_right_and_north_up ["map.window.the-players-own-mark-follows-him-with-east-to-the-right-and-north-up"],
    scenario_indoors_the_date_keeps_going_and_only_the_rest_stops => indoors_the_date_keeps_going_and_only_the_rest_stops ["map.window.indoors-the-date-keeps-going-and-only-the-coordinates-and-the-mark-stop"],
    scenario_with_no_clock_yet_the_date_line_keeps_its_labels => with_no_clock_yet_the_date_line_keeps_its_labels ["map.window.with-no-clock-yet-the-date-line-keeps-its-labels"],
    scenario_the_map_is_redrawn_once_every_five_seconds => the_map_is_redrawn_once_every_five_seconds ["map.window.is-redrawn-once-every-five-seconds-and-the-first-look-is-due-at-once"],
    scenario_every_shipped_town_is_on_the_map_where_the_table_says => every_shipped_town_is_on_the_map_where_the_table_says ["map.page.every-town-the-shipped-table-names-is-on-the-map-where-it-says"],
    scenario_the_map_is_one_picture_over_the_whole_element => the_map_is_one_picture_over_the_whole_element ["map.page.the-map-is-one-picture-drawn-once-over-the-whole-map-element"],
    scenario_a_freshly_opened_map_highlights_no_town => a_freshly_opened_map_highlights_no_town ["map.page.a-freshly-opened-map-highlights-no-town-at-all"],
    scenario_hovering_a_town_frames_that_town_alone => hovering_a_town_frames_that_town_alone ["map.page.hovering-a-town-frames-that-town-alone-and-leaving-un-frames-it"],
    scenario_hovering_a_town_shows_its_name => hovering_a_town_shows_its_name ["map.page.hovering-a-town-shows-its-name"],
    scenario_pressing_the_map_does_nothing_for_an_ordinary_player => pressing_the_map_does_nothing_for_an_ordinary_player ["map.page.pressing-the-map-does-nothing-at-all-for-an-ordinary-player"],
    scenario_the_players_dot_is_where_the_arithmetic_puts_it => the_players_dot_is_where_the_arithmetic_puts_it ["map.page.the-players-own-dot-is-where-the-shipped-arithmetic-puts-it"],
    scenario_the_date_and_time_are_drawn_on_the_page => the_date_and_time_are_drawn_on_the_page ["map.page.the-date-and-the-time-are-drawn-on-the-page"],
    scenario_opening_the_map_refreshes_it_at_once => opening_the_map_refreshes_it_at_once ["map.page.opening-the-map-refreshes-it-at-once-and-closing-it-asks-for-nothing"],
    scenario_only_what_the_shard_marks_reaches_the_radar => only_what_the_shard_marks_reaches_the_radar ["radar.filter.only-what-the-shard-marks-for-the-radar-reaches-it"],
    scenario_the_blips_are_the_showable_objects_inside_the_range => the_blips_are_the_showable_objects_inside_the_range ["radar.blips.are-exactly-the-things-the-radar-shows-that-are-inside-its-range"],
    scenario_the_filter_is_the_difference_between_a_crowd_and_a_handful => the_filter_is_the_difference_between_a_crowd_and_a_handful ["radar.filter.is-the-difference-between-a-crowded-radar-and-a-readable-one"],
    scenario_every_blip_takes_the_colour_its_description_asks_for => every_blip_takes_the_colour_its_description_asks_for ["radar.blips.every-one-takes-the-colour-its-own-description-asks-for"],
    scenario_no_blip_falls_through_to_the_colour_that_means_nothing => no_blip_falls_through_to_the_colour_that_means_nothing ["radar.blips.the-only-things-that-take-no-colour-of-their-own-are-ordinary-players"],
    scenario_the_players_mark_is_nine_bright_green_points_on_the_centre => the_players_mark_is_nine_bright_green_points_on_the_centre ["radar.player.his-own-place-is-nine-bright-green-points-on-the-centre"],
    scenario_the_radars_padlock_is_up_and_open_from_the_first_frame => the_radars_padlock_is_up_and_open_from_the_first_frame ["radar.padlock.is-up-and-drawing-its-open-picture-from-the-first-frame"],
    scenario_the_players_mark_is_laid_on_the_radar_element => the_players_mark_is_laid_on_the_radar_element ["radar.player.his-mark-is-laid-on-the-radars-own-element-after-the-blips"],
    scenario_there_are_two_radar_ranges_and_one_question => there_are_two_radar_ranges_and_one_question ["radar.range.there-are-two-and-one-question-about-the-players-own-room-decides"],
    scenario_every_recorded_place_takes_the_range_its_room_asks_for => every_recorded_place_takes_the_range_its_room_asks_for ["radar.range.every-place-a-recording-puts-the-player-takes-the-range-its-room-asks-for"],
    scenario_a_dungeon_and_a_building_inside_are_the_same_case => a_dungeon_and_a_building_inside_are_the_same_case ["radar.range.a-dungeon-and-the-inside-of-a-building-are-the-same-case"],
    scenario_the_range_follows_the_player_in_and_out => the_range_follows_the_player_in_and_out ["radar.range.follows-the-player-as-he-steps-inside-and-out-again"],
    scenario_what_the_radar_shows_stops_one_short_of_its_range => what_the_radar_shows_stops_one_short_of_its_range ["radar.range.what-the-radar-shows-stops-one-short-of-its-own-range-at-either-range"],
    scenario_the_chat_sweep_uses_the_same_radius => the_chat_sweep_uses_the_same_radius ["radar.range.the-chat-windows-own-sweep-uses-the-same-radius-as-the-radar"],
    scenario_the_radar_zooms_when_the_player_goes_inside => the_radar_zooms_when_the_player_goes_inside ["radar.range.the-things-on-the-radar-really-zoom-when-the-player-goes-inside"],
    scenario_the_padlock_works_the_same_at_either_range => the_padlock_works_the_same_at_either_range ["radar.padlock.works-the-same-at-either-range-and-neither-moves-the-other"],
    scenario_a_click_on_a_thing_on_the_radar_selects_it => a_click_on_a_thing_on_the_radar_selects_it ["radar.click.a-click-on-a-thing-on-the-radar-selects-the-thing-it-stands-for"],
    scenario_empty_radar_is_transparent_and_selects_nothing => empty_radar_is_transparent_and_selects_nothing ["radar.click.an-empty-part-of-the-radar-is-transparent-and-selects-nothing"],
    scenario_the_name_over_a_thing_is_the_shards_own => the_name_over_a_thing_is_the_shards_own ["radar.hover.the-name-shown-over-a-thing-is-the-shards-own-name-for-it"],
    scenario_the_lock_is_one_bit_of_the_players_options => the_lock_is_one_bit_of_the_players_options ["radar.lock.whether-the-interface-is-locked-is-one-bit-of-the-players-own-options"],
    scenario_clicking_the_padlock_asks_to_flip_the_lock => clicking_the_padlock_asks_to_flip_the_lock ["radar.padlock.a-click-on-it-asks-to-flip-the-lock-and-does-nothing-else"],
    scenario_the_lock_reaches_the_players_options_and_survives_a_rebuild => the_lock_reaches_the_players_options_and_survives_a_rebuild ["radar.lock.the-lock-the-player-asks-for-reaches-his-options-and-survives-a-rebuilt-screen"],
    scenario_the_padlock_swaps_its_pictures_and_the_handle_follows => the_padlock_swaps_its_pictures_and_the_handle_follows ["radar.padlock.swaps-its-two-pictures-with-the-lock-and-the-drag-handle-follows"],
    scenario_the_handle_moves_the_radar_and_keeps_it_on_screen => the_handle_moves_the_radar_and_keeps_it_on_screen ["radar.drag.the-handle-moves-the-window-by-how-far-the-pointer-moved-and-keeps-it-on-screen"],
    scenario_a_moved_radar_writes_its_new_place_into_the_options => a_moved_radar_writes_its_new_place_into_the_options ["radar.drag.a-radar-the-player-has-moved-writes-its-new-place-into-his-own-options"],
}

// -------------------------------------------------------------------------------------------
// movement.teleport.a-player-arriving-on-another-player-through-the-shards-own-login-is-not-deflected
// movement.teleport.two-players-pass-through-each-other-only-while-neither-is-marked-for-combat
// -------------------------------------------------------------------------------------------

/// The measurement and the pairings of player descriptions it is taken over.
///
/// Both halves of the pair arrive the way a connected client gets them -- the login names the
/// player, the player's own create carries his description, the client adopts the shard's id and
/// the object stream pushes that description on to the collision body -- so this is a claim about
/// the whole client and not about the collision walk alone.
mod teleloc;

// -------------------------------------------------------------------------------------------
// movement.walk.reports-walking-forward-with-no-hold-key
// movement.run.the-report-a-running-body-sends-is-the-one-the-retail-client-sent
// movement.run.turning-run-on-while-already-walking-speeds-the-body-up-and-changes-the-report
// -------------------------------------------------------------------------------------------

/// The walk and run reports a live body sends, driven over real terrain; no corpus count is pinned.
mod run_forward;

// -------------------------------------------------------------------------------------------
// The fixtures the body scenarios share
// -------------------------------------------------------------------------------------------

/// What a scenario in this subject needs before it can measure anything: the retail dats, a settled
/// local body over real terrain, and a create or a position update that has really been through the
/// wire.
///
/// It is shared so that the scenarios that need the same four helpers do not each build their own.
pub(super) mod support;

pub(super) use support as world_support;

// ===========================================================================================
// Movement driven through the animation data: sub-steps, cached speeds, approaches, the walk
// bit, doors, stances and the host's effect queue.
//
// These need `dereth-animation` and `dereth-assets` as well as the dats. Like the scenarios above
// them, they open the dats themselves, drive the structure directly, and book the claim through
// `assert_behaviour` on a model client.
//
// The `movement.stance.*` claims sit here with the rest of the motion vocabulary although the
// census files their subject under `combat`.
// ===========================================================================================

pub(super) mod motion;

mod bodies;
use bodies::{
    a_body_not_yet_simulated_is_put_straight_onto_the_shards_position,
    a_correction_out_of_reach_is_taken_in_one_step,
    a_create_onto_an_occupied_spot_is_put_down_beside_it,
    a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world,
    a_nearby_correction_glides_the_body_over_the_following_frames,
    a_running_bodys_report_is_the_retail_clients_own,
    a_teleport_onto_another_player_is_deflected_only_by_a_creature,
    a_teleport_out_of_the_world_drops_a_glide_in_flight,
    an_arrival_through_the_login_path_lands_on_the_other_players_spot,
    an_update_with_no_ground_contact_moves_the_body_nowhere,
    enter_world_refuses_a_body_inside_a_building, running_reports_walk_with_the_run_hold_key,
    turning_run_on_mid_walk_speeds_the_body_and_changes_the_report,
    two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt,
    walking_forward_reports_a_walk_with_no_hold_key,
};

mod camera;
use camera::{
    the_camera_against_a_wall_settles_the_same_at_any_tick_rate,
    the_camera_slides_through_a_creature_but_not_through_a_solid_twin,
};

mod picking;
use picking::{
    a_click_with_nothing_drawn_is_answered_too, a_drop_armed_with_nothing_drawn_is_answered,
    a_frame_with_nothing_armed_answers_nothing, a_scene_less_click_reports_nothing_picked,
    the_scene_less_answer_names_nothing,
};

mod reports;
use reports::{
    a_jump_reports_the_bodys_own_position_and_speed,
    a_running_jump_carries_its_speed_in_the_bodys_own_frame,
    a_second_jump_in_mid_air_sends_nothing,
    a_walking_body_reports_where_it_is_and_a_still_one_falls_silent,
    every_drawn_frame_reaches_the_position_reporter, every_drawn_frame_visits_the_jump_dispatch,
    the_clients_jump_height_explains_the_recorded_jumps, the_reported_state_is_the_bodys_own,
    walking_forward_reports_the_state_the_recordings_carry,
};

mod scripts;
use scripts::{
    a_body_walking_at_a_scaled_object_stops_further_away,
    a_delayed_script_call_plays_after_its_delay, a_scale_hook_moves_the_collision_radius,
    a_script_call_with_no_delay_plays_on_the_spot,
    an_object_that_leaves_before_the_delay_plays_nothing,
};

mod control;
use control::{
    a_jump_ends_an_approach_whether_or_not_it_is_allowed, a_key_press_ends_an_approach_on_the_spot,
    a_key_release_stops_the_body_after_the_shard_has_taken_control,
    a_press_that_takes_control_back_re_issues_the_key_already_held,
    a_replacement_ends_the_old_approach,
    a_shard_authored_swing_holds_the_body_for_its_own_animation,
    a_teleport_onto_the_same_ground_raises_no_edge,
    a_teleport_that_changes_the_ground_frees_the_body,
    a_teleport_with_no_shard_involvement_leaves_a_movable_body,
    a_turn_release_stops_the_turn_the_same_way, every_way_a_follow_ends_reports_it_once,
    leaving_the_ground_empties_the_motion_ledger_and_routes_it,
    neither_an_approach_nor_a_target_update_leaves_a_motion_behind,
    the_old_approachs_clean_up_never_outlives_the_next,
    the_player_can_move_again_after_the_shard_teleports_him,
};

mod teleports;
use teleports::{
    a_body_that_loses_its_footing_with_no_edge_turns_for_ever,
    a_charged_jump_turns_but_does_not_walk, a_full_ledger_stops_turning_and_walking_alike,
    a_placement_inside_a_building_takes_and_raises_its_footing_change,
    a_recorded_teleport_moves_the_body_and_the_reports_follow,
    a_teleport_into_a_room_lands_the_body_on_its_floor,
    every_drawn_frame_applies_a_teleport_before_reporting,
    every_recorded_teleport_is_applied_and_no_other_position_is,
    no_door_journey_leaves_a_body_that_turns_but_cannot_walk,
    several_teleports_in_a_row_each_move_the_body,
};

mod map;
use map::{
    a_freshly_opened_map_highlights_no_town, a_second_look_at_an_unchanged_world_writes_nothing,
    every_shipped_town_is_on_the_map_where_the_table_says, hovering_a_town_frames_that_town_alone,
    hovering_a_town_shows_its_name, indoors_the_date_keeps_going_and_only_the_rest_stops,
    opening_the_map_refreshes_it_at_once,
    outdoors_the_map_shows_the_date_the_coordinates_and_the_mark,
    pressing_the_map_does_nothing_for_an_ordinary_player, the_date_and_time_are_drawn_on_the_page,
    the_map_is_one_picture_over_the_whole_element, the_map_is_redrawn_once_every_five_seconds,
    the_players_dot_is_where_the_arithmetic_puts_it,
    the_players_mark_follows_him_east_right_and_north_up,
    with_no_clock_yet_the_date_line_keeps_its_labels,
};

mod radar;
use radar::{
    a_click_on_a_thing_on_the_radar_selects_it, a_dungeon_and_a_building_inside_are_the_same_case,
    a_moved_radar_writes_its_new_place_into_the_options,
    clicking_the_padlock_asks_to_flip_the_lock, empty_radar_is_transparent_and_selects_nothing,
    every_blip_takes_the_colour_its_description_asks_for,
    every_recorded_place_takes_the_range_its_room_asks_for,
    no_blip_falls_through_to_the_colour_that_means_nothing,
    only_what_the_shard_marks_reaches_the_radar,
    the_blips_are_the_showable_objects_inside_the_range, the_chat_sweep_uses_the_same_radius,
    the_filter_is_the_difference_between_a_crowd_and_a_handful,
    the_handle_moves_the_radar_and_keeps_it_on_screen, the_lock_is_one_bit_of_the_players_options,
    the_lock_reaches_the_players_options_and_survives_a_rebuild,
    the_name_over_a_thing_is_the_shards_own, the_padlock_swaps_its_pictures_and_the_handle_follows,
    the_padlock_works_the_same_at_either_range, the_players_mark_is_laid_on_the_radar_element,
    the_players_mark_is_nine_bright_green_points_on_the_centre,
    the_radar_zooms_when_the_player_goes_inside,
    the_radars_padlock_is_up_and_open_from_the_first_frame,
    the_range_follows_the_player_in_and_out, there_are_two_radar_ranges_and_one_question,
    what_the_radar_shows_stops_one_short_of_its_range,
};
