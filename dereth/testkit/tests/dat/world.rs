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

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "running_reports_walk_with_the_run_hold_key",
        &["movement.run.reports-walk-with-the-run-hold-key"],
        running_reports_walk_with_the_run_hold_key,
    ),
    (
        "enter_world_refuses_a_body_inside_a_building",
        &["physics.enter-world.refuses-a-body-inside-a-building"],
        enter_world_refuses_a_body_inside_a_building,
    ),
    (
        "a_teleport_onto_another_player_is_deflected_only_by_a_creature",
        &["movement.teleport.arrives-exactly-on-another-player-and-is-pushed-aside-only-by-a-creature"],
        a_teleport_onto_another_player_is_deflected_only_by_a_creature,
    ),
    (
        "an_arrival_through_the_login_path_lands_on_the_other_players_spot",
        &["movement.teleport.a-player-arriving-on-another-player-through-the-shards-own-login-is-not-deflected"],
        an_arrival_through_the_login_path_lands_on_the_other_players_spot,
    ),
    (
        "two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt",
        &["movement.teleport.two-players-pass-through-each-other-only-while-neither-is-marked-for-combat"],
        two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt,
    ),
    (
        "an_update_with_no_ground_contact_moves_the_body_nowhere",
        &["movement.correction.an-update-that-does-not-say-the-body-is-on-the-ground-moves-it-nowhere"],
        an_update_with_no_ground_contact_moves_the_body_nowhere,
    ),
    (
        "a_create_onto_an_occupied_spot_is_put_down_beside_it",
        &["movement.remote-body.a-create-onto-an-occupied-spot-is-put-down-beside-it"],
        a_create_onto_an_occupied_spot_is_put_down_beside_it,
    ),
    (
        "a_nearby_correction_glides_the_body_over_the_following_frames",
        &["movement.correction.a-nearby-correction-glides-the-body-over-the-following-frames"],
        a_nearby_correction_glides_the_body_over_the_following_frames,
    ),
    (
        "a_correction_out_of_reach_is_taken_in_one_step",
        &["movement.correction.a-correction-out-of-reach-is-taken-in-one-step-instead-of-a-glide"],
        a_correction_out_of_reach_is_taken_in_one_step,
    ),
    (
        "a_body_not_yet_simulated_is_put_straight_onto_the_shards_position",
        &["movement.correction.a-body-the-client-has-not-simulated-yet-is-put-straight-onto-the-shards-position"],
        a_body_not_yet_simulated_is_put_straight_onto_the_shards_position,
    ),
    (
        "a_teleport_out_of_the_world_drops_a_glide_in_flight",
        &["movement.correction.a-teleport-out-of-the-world-drops-a-glide-that-was-already-in-flight"],
        a_teleport_out_of_the_world_drops_a_glide_in_flight,
    ),
    (
        "a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world",
        &["movement.correction.a-destination-in-a-block-the-client-has-not-loaded-takes-the-body-out-of-the-world"],
        a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world,
    ),
    (
        "walking_forward_reports_a_walk_with_no_hold_key",
        &["movement.walk.reports-walking-forward-with-no-hold-key"],
        walking_forward_reports_a_walk_with_no_hold_key,
    ),
    (
        "a_running_bodys_report_is_the_retail_clients_own",
        &["movement.run.the-report-a-running-body-sends-is-the-one-the-retail-client-sent"],
        a_running_bodys_report_is_the_retail_clients_own,
    ),
    (
        "turning_run_on_mid_walk_speeds_the_body_and_changes_the_report",
        &["movement.run.turning-run-on-while-already-walking-speeds-the-body-up-and-changes-the-report"],
        turning_run_on_mid_walk_speeds_the_body_and_changes_the_report,
    ),
    (
        "the_camera_slides_through_a_creature_but_not_through_a_solid_twin",
        &["camera.view.slides-through-a-creature-standing-between-you-and-the-camera"],
        the_camera_slides_through_a_creature_but_not_through_a_solid_twin,
    ),
    (
        "a_scene_less_click_reports_nothing_picked",
        &["world.click.a-click-answered-with-nothing-in-view-reports-nothing-picked"],
        a_scene_less_click_reports_nothing_picked,
    ),
    (
        "the_reported_state_is_the_bodys_own",
        &["movement.position-report.the-state-the-client-reports-is-the-bodys-own"],
        the_reported_state_is_the_bodys_own,
    ),
    (
        "walking_forward_reports_the_state_the_recordings_carry",
        &["movement.position-report.walking-forward-reports-the-state-the-recordings-carry"],
        walking_forward_reports_the_state_the_recordings_carry,
    ),
    (
        "a_walking_body_reports_where_it_is_and_a_still_one_falls_silent",
        &["movement.position-report.a-walking-body-reports-where-it-is-and-a-still-one-falls-silent"],
        a_walking_body_reports_where_it_is_and_a_still_one_falls_silent,
    ),
    (
        "every_drawn_frame_reaches_the_position_reporter",
        &["movement.position-report.every-drawn-frame-reaches-the-reporter-and-invents-nothing"],
        every_drawn_frame_reaches_the_position_reporter,
    ),
    (
        "a_jump_reports_the_bodys_own_position_and_speed",
        &["movement.jump.a-jump-puts-the-bodys-own-position-and-speed-on-the-wire"],
        a_jump_reports_the_bodys_own_position_and_speed,
    ),
    (
        "a_second_jump_in_mid_air_sends_nothing",
        &["movement.jump.a-second-jump-in-mid-air-is-refused-and-nothing-goes-out"],
        a_second_jump_in_mid_air_sends_nothing,
    ),
    (
        "a_running_jump_carries_its_speed_in_the_bodys_own_frame",
        &["movement.jump.a-running-jump-carries-its-speed-in-the-bodys-own-frame"],
        a_running_jump_carries_its_speed_in_the_bodys_own_frame,
    ),
    (
        "the_clients_jump_height_explains_the_recorded_jumps",
        &["movement.jump.the-height-the-client-works-out-explains-the-recorded-jumps"],
        the_clients_jump_height_explains_the_recorded_jumps,
    ),
    (
        "every_drawn_frame_visits_the_jump_dispatch",
        &["movement.jump.every-drawn-frame-visits-the-jump-dispatch-and-invents-none"],
        every_drawn_frame_visits_the_jump_dispatch,
    ),
    (
        "a_drop_armed_with_nothing_drawn_is_answered",
        &["world.scene-less-frame.a-drop-armed-with-nothing-drawn-is-answered-and-the-next-click-still-works"],
        a_drop_armed_with_nothing_drawn_is_answered,
    ),
    (
        "a_click_with_nothing_drawn_is_answered_too",
        &["world.scene-less-frame.a-click-and-a-double-click-are-answered-so-the-gestures-after-them-still-work"],
        a_click_with_nothing_drawn_is_answered_too,
    ),
    (
        "a_frame_with_nothing_armed_answers_nothing",
        &["world.scene-less-frame.a-frame-with-nothing-armed-answers-nothing-and-clears-nothing"],
        a_frame_with_nothing_armed_answers_nothing,
    ),
    (
        "the_scene_less_answer_names_nothing",
        &["world.scene-less-frame.the-answer-names-nothing-so-a-dropped-item-takes-the-ground-leg"],
        the_scene_less_answer_names_nothing,
    ),
    (
        "a_delayed_script_call_plays_after_its_delay",
        &["world.physics-script.a-delayed-call-plays-the-script-it-names-after-the-delay-it-rolled"],
        a_delayed_script_call_plays_after_its_delay,
    ),
    (
        "an_object_that_leaves_before_the_delay_plays_nothing",
        &["world.physics-script.an-object-that-leaves-its-room-before-the-delay-is-up-plays-nothing"],
        an_object_that_leaves_before_the_delay_plays_nothing,
    ),
    (
        "a_script_call_with_no_delay_plays_on_the_spot",
        &["world.physics-script.a-call-with-no-delay-still-plays-on-the-spot"],
        a_script_call_with_no_delay_plays_on_the_spot,
    ),
    (
        "a_scale_hook_moves_the_collision_radius",
        &["world.physics-script.a-scale-hook-moves-the-objects-own-collision-radius"],
        a_scale_hook_moves_the_collision_radius,
    ),
    (
        "a_body_walking_at_a_scaled_object_stops_further_away",
        &["world.physics-script.a-body-walking-at-a-scaled-object-stops-at-the-scaled-distance"],
        a_body_walking_at_a_scaled_object_stops_further_away,
    ),
    (
        "the_camera_against_a_wall_settles_the_same_at_any_tick_rate",
        &["camera.wall.settles-in-the-same-place-however-often-the-client-ticks"],
        the_camera_against_a_wall_settles_the_same_at_any_tick_rate,
    ),
    (
        "a_key_release_stops_the_body_after_the_shard_has_taken_control",
        &["movement.server-control.a-key-release-stops-the-body-after-the-shard-has-taken-control"],
        a_key_release_stops_the_body_after_the_shard_has_taken_control,
    ),
    (
        "a_turn_release_stops_the_turn_the_same_way",
        &["movement.server-control.a-turn-release-stops-the-turn-the-same-way"],
        a_turn_release_stops_the_turn_the_same_way,
    ),
    (
        "the_player_can_move_again_after_the_shard_teleports_him",
        &["movement.teleport.the-player-can-move-again-after-the-shard-teleports-him"],
        the_player_can_move_again_after_the_shard_teleports_him,
    ),
    (
        "a_press_that_takes_control_back_re_issues_the_key_already_held",
        &["movement.server-control.a-press-that-takes-control-back-re-issues-the-key-already-held"],
        a_press_that_takes_control_back_re_issues_the_key_already_held,
    ),
    (
        "neither_an_approach_nor_a_target_update_leaves_a_motion_behind",
        &["movement.approach.neither-an-approach-nor-a-target-update-leaves-a-motion-behind"],
        neither_an_approach_nor_a_target_update_leaves_a_motion_behind,
    ),
    (
        "a_teleport_with_no_shard_involvement_leaves_a_movable_body",
        &["movement.teleport.a-teleport-with-no-shard-involvement-leaves-a-body-that-still-moves"],
        a_teleport_with_no_shard_involvement_leaves_a_movable_body,
    ),
    (
        "a_shard_authored_swing_holds_the_body_for_its_own_animation",
        &["movement.action.a-shard-authored-swing-holds-every-motion-behind-it-for-its-own-animation"],
        a_shard_authored_swing_holds_the_body_for_its_own_animation,
    ),
    (
        "leaving_the_ground_empties_the_motion_ledger_and_routes_it",
        &["movement.action.leaving-the-ground-empties-the-motion-ledger-and-routes-what-it-drained"],
        leaving_the_ground_empties_the_motion_ledger_and_routes_it,
    ),
    (
        "a_teleport_that_changes_the_ground_frees_the_body",
        &["movement.teleport.a-teleport-that-changes-the-ground-empties-the-ledger-and-the-body-moves-again"],
        a_teleport_that_changes_the_ground_frees_the_body,
    ),
    (
        "a_teleport_onto_the_same_ground_raises_no_edge",
        &["movement.teleport.a-teleport-onto-the-same-kind-of-ground-raises-no-edge-and-empties-nothing"],
        a_teleport_onto_the_same_ground_raises_no_edge,
    ),
    (
        "a_replacement_ends_the_old_approach",
        &["movement.approach.a-replacement-ends-the-old-approach-before-the-new-one-is-installed"],
        a_replacement_ends_the_old_approach,
    ),
    (
        "a_key_press_ends_an_approach_on_the_spot",
        &["movement.approach.a-key-press-ends-an-approach-on-the-spot"],
        a_key_press_ends_an_approach_on_the_spot,
    ),
    (
        "a_jump_ends_an_approach_whether_or_not_it_is_allowed",
        &["movement.approach.a-jump-ends-an-approach-whether-or-not-the-jump-is-allowed"],
        a_jump_ends_an_approach_whether_or_not_it_is_allowed,
    ),
    (
        "the_old_approachs_clean_up_never_outlives_the_next",
        &["movement.approach.the-old-approachs-clean-up-never-outlives-the-next-approachs-subject"],
        the_old_approachs_clean_up_never_outlives_the_next,
    ),
    (
        "every_way_a_follow_ends_reports_it_once",
        &["movement.follow.every-way-a-follow-ends-reports-it-once-and-leaves-the-body-free"],
        every_way_a_follow_ends_reports_it_once,
    ),
    (
        "every_recorded_teleport_is_applied_and_no_other_position_is",
        &["movement.teleport.every-teleport-the-recordings-carry-is-applied-and-no-other-position-is"],
        every_recorded_teleport_is_applied_and_no_other_position_is,
    ),
    (
        "a_recorded_teleport_moves_the_body_and_the_reports_follow",
        &["movement.teleport.a-recorded-teleport-moves-the-body-and-the-reports-name-the-destination"],
        a_recorded_teleport_moves_the_body_and_the_reports_follow,
    ),
    (
        "several_teleports_in_a_row_each_move_the_body",
        &["movement.teleport.several-teleports-in-a-row-each-move-the-body"],
        several_teleports_in_a_row_each_move_the_body,
    ),
    (
        "a_teleport_into_a_room_lands_the_body_on_its_floor",
        &["movement.teleport.a-teleport-into-a-room-lands-the-body-on-its-floor"],
        a_teleport_into_a_room_lands_the_body_on_its_floor,
    ),
    (
        "every_drawn_frame_applies_a_teleport_before_reporting",
        &["movement.teleport.every-drawn-frame-applies-a-teleport-before-it-reports-a-position"],
        every_drawn_frame_applies_a_teleport_before_reporting,
    ),
    (
        "a_full_ledger_stops_turning_and_walking_alike",
        &["movement.motion-ledger.gates-turning-and-walking-alike-and-a-fresh-key-press-escapes-it"],
        a_full_ledger_stops_turning_and_walking_alike,
    ),
    (
        "a_charged_jump_turns_but_does_not_walk",
        &["movement.jump-charge.a-charged-jump-turns-but-does-not-walk-until-the-body-leaves-the-ground"],
        a_charged_jump_turns_but_does_not_walk,
    ),
    (
        "a_placement_inside_a_building_takes_and_raises_its_footing_change",
        &["movement.placement.a-placement-inside-a-building-takes-and-raises-the-change-of-footing-it-implies"],
        a_placement_inside_a_building_takes_and_raises_its_footing_change,
    ),
    (
        "a_body_that_loses_its_footing_with_no_edge_turns_for_ever",
        &["movement.contact.a-body-that-loses-its-footing-with-nothing-to-announce-it-turns-for-ever"],
        a_body_that_loses_its_footing_with_no_edge_turns_for_ever,
    ),
    (
        "no_door_journey_leaves_a_body_that_turns_but_cannot_walk",
        &["movement.door.no-way-of-opening-a-door-leaves-a-body-that-turns-but-cannot-walk"],
        no_door_journey_leaves_a_body_that_turns_but_cannot_walk,
    ),
    (
        "outdoors_the_map_shows_the_date_the_coordinates_and_the_mark",
        &["map.window.outdoors-it-shows-the-date-the-coordinates-and-the-players-own-mark"],
        outdoors_the_map_shows_the_date_the_coordinates_and_the_mark,
    ),
    (
        "a_second_look_at_an_unchanged_world_writes_nothing",
        &["map.window.a-second-look-at-an-unchanged-world-writes-nothing"],
        a_second_look_at_an_unchanged_world_writes_nothing,
    ),
    (
        "the_players_mark_follows_him_east_right_and_north_up",
        &["map.window.the-players-own-mark-follows-him-with-east-to-the-right-and-north-up"],
        the_players_mark_follows_him_east_right_and_north_up,
    ),
    (
        "indoors_the_date_keeps_going_and_only_the_rest_stops",
        &["map.window.indoors-the-date-keeps-going-and-only-the-coordinates-and-the-mark-stop"],
        indoors_the_date_keeps_going_and_only_the_rest_stops,
    ),
    (
        "with_no_clock_yet_the_date_line_keeps_its_labels",
        &["map.window.with-no-clock-yet-the-date-line-keeps-its-labels"],
        with_no_clock_yet_the_date_line_keeps_its_labels,
    ),
    (
        "the_map_is_redrawn_once_every_five_seconds",
        &["map.window.is-redrawn-once-every-five-seconds-and-the-first-look-is-due-at-once"],
        the_map_is_redrawn_once_every_five_seconds,
    ),
    (
        "every_shipped_town_is_on_the_map_where_the_table_says",
        &["map.page.every-town-the-shipped-table-names-is-on-the-map-where-it-says"],
        every_shipped_town_is_on_the_map_where_the_table_says,
    ),
    (
        "the_map_is_one_picture_over_the_whole_element",
        &["map.page.the-map-is-one-picture-drawn-once-over-the-whole-map-element"],
        the_map_is_one_picture_over_the_whole_element,
    ),
    (
        "a_freshly_opened_map_highlights_no_town",
        &["map.page.a-freshly-opened-map-highlights-no-town-at-all"],
        a_freshly_opened_map_highlights_no_town,
    ),
    (
        "hovering_a_town_frames_that_town_alone",
        &["map.page.hovering-a-town-frames-that-town-alone-and-leaving-un-frames-it"],
        hovering_a_town_frames_that_town_alone,
    ),
    (
        "hovering_a_town_shows_its_name",
        &["map.page.hovering-a-town-shows-its-name"],
        hovering_a_town_shows_its_name,
    ),
    (
        "pressing_the_map_does_nothing_for_an_ordinary_player",
        &["map.page.pressing-the-map-does-nothing-at-all-for-an-ordinary-player"],
        pressing_the_map_does_nothing_for_an_ordinary_player,
    ),
    (
        "the_players_dot_is_where_the_arithmetic_puts_it",
        &["map.page.the-players-own-dot-is-where-the-shipped-arithmetic-puts-it"],
        the_players_dot_is_where_the_arithmetic_puts_it,
    ),
    (
        "the_date_and_time_are_drawn_on_the_page",
        &["map.page.the-date-and-the-time-are-drawn-on-the-page"],
        the_date_and_time_are_drawn_on_the_page,
    ),
    (
        "opening_the_map_refreshes_it_at_once",
        &["map.page.opening-the-map-refreshes-it-at-once-and-closing-it-asks-for-nothing"],
        opening_the_map_refreshes_it_at_once,
    ),
    (
        "only_what_the_shard_marks_reaches_the_radar",
        &["radar.filter.only-what-the-shard-marks-for-the-radar-reaches-it"],
        only_what_the_shard_marks_reaches_the_radar,
    ),
    (
        "the_blips_are_the_showable_objects_inside_the_range",
        &["radar.blips.are-exactly-the-things-the-radar-shows-that-are-inside-its-range"],
        the_blips_are_the_showable_objects_inside_the_range,
    ),
    (
        "the_filter_is_the_difference_between_a_crowd_and_a_handful",
        &["radar.filter.is-the-difference-between-a-crowded-radar-and-a-readable-one"],
        the_filter_is_the_difference_between_a_crowd_and_a_handful,
    ),
    (
        "every_blip_takes_the_colour_its_description_asks_for",
        &["radar.blips.every-one-takes-the-colour-its-own-description-asks-for"],
        every_blip_takes_the_colour_its_description_asks_for,
    ),
    (
        "no_blip_falls_through_to_the_colour_that_means_nothing",
        &["radar.blips.the-only-things-that-take-no-colour-of-their-own-are-ordinary-players"],
        no_blip_falls_through_to_the_colour_that_means_nothing,
    ),
    (
        "the_players_mark_is_nine_bright_green_points_on_the_centre",
        &["radar.player.his-own-place-is-nine-bright-green-points-on-the-centre"],
        the_players_mark_is_nine_bright_green_points_on_the_centre,
    ),
    (
        "the_radars_padlock_is_up_and_open_from_the_first_frame",
        &["radar.padlock.is-up-and-drawing-its-open-picture-from-the-first-frame"],
        the_radars_padlock_is_up_and_open_from_the_first_frame,
    ),
    (
        "the_players_mark_is_laid_on_the_radar_element",
        &["radar.player.his-mark-is-laid-on-the-radars-own-element-after-the-blips"],
        the_players_mark_is_laid_on_the_radar_element,
    ),
    (
        "there_are_two_radar_ranges_and_one_question",
        &["radar.range.there-are-two-and-one-question-about-the-players-own-room-decides"],
        there_are_two_radar_ranges_and_one_question,
    ),
    (
        "every_recorded_place_takes_the_range_its_room_asks_for",
        &["radar.range.every-place-a-recording-puts-the-player-takes-the-range-its-room-asks-for"],
        every_recorded_place_takes_the_range_its_room_asks_for,
    ),
    (
        "a_dungeon_and_a_building_inside_are_the_same_case",
        &["radar.range.a-dungeon-and-the-inside-of-a-building-are-the-same-case"],
        a_dungeon_and_a_building_inside_are_the_same_case,
    ),
    (
        "the_range_follows_the_player_in_and_out",
        &["radar.range.follows-the-player-as-he-steps-inside-and-out-again"],
        the_range_follows_the_player_in_and_out,
    ),
    (
        "what_the_radar_shows_stops_one_short_of_its_range",
        &["radar.range.what-the-radar-shows-stops-one-short-of-its-own-range-at-either-range"],
        what_the_radar_shows_stops_one_short_of_its_range,
    ),
    (
        "the_chat_sweep_uses_the_same_radius",
        &["radar.range.the-chat-windows-own-sweep-uses-the-same-radius-as-the-radar"],
        the_chat_sweep_uses_the_same_radius,
    ),
    (
        "the_radar_zooms_when_the_player_goes_inside",
        &["radar.range.the-things-on-the-radar-really-zoom-when-the-player-goes-inside"],
        the_radar_zooms_when_the_player_goes_inside,
    ),
    (
        "the_padlock_works_the_same_at_either_range",
        &["radar.padlock.works-the-same-at-either-range-and-neither-moves-the-other"],
        the_padlock_works_the_same_at_either_range,
    ),
    (
        "a_click_on_a_thing_on_the_radar_selects_it",
        &["radar.click.a-click-on-a-thing-on-the-radar-selects-the-thing-it-stands-for"],
        a_click_on_a_thing_on_the_radar_selects_it,
    ),
    (
        "empty_radar_is_transparent_and_selects_nothing",
        &["radar.click.an-empty-part-of-the-radar-is-transparent-and-selects-nothing"],
        empty_radar_is_transparent_and_selects_nothing,
    ),
    (
        "the_name_over_a_thing_is_the_shards_own",
        &["radar.hover.the-name-shown-over-a-thing-is-the-shards-own-name-for-it"],
        the_name_over_a_thing_is_the_shards_own,
    ),
    (
        "the_lock_is_one_bit_of_the_players_options",
        &["radar.lock.whether-the-interface-is-locked-is-one-bit-of-the-players-own-options"],
        the_lock_is_one_bit_of_the_players_options,
    ),
    (
        "clicking_the_padlock_asks_to_flip_the_lock",
        &["radar.padlock.a-click-on-it-asks-to-flip-the-lock-and-does-nothing-else"],
        clicking_the_padlock_asks_to_flip_the_lock,
    ),
    (
        "the_lock_reaches_the_players_options_and_survives_a_rebuild",
        &["radar.lock.the-lock-the-player-asks-for-reaches-his-options-and-survives-a-rebuilt-screen"],
        the_lock_reaches_the_players_options_and_survives_a_rebuild,
    ),
    (
        "the_padlock_swaps_its_pictures_and_the_handle_follows",
        &["radar.padlock.swaps-its-two-pictures-with-the-lock-and-the-drag-handle-follows"],
        the_padlock_swaps_its_pictures_and_the_handle_follows,
    ),
    (
        "the_handle_moves_the_radar_and_keeps_it_on_screen",
        &["radar.drag.the-handle-moves-the-window-by-how-far-the-pointer-moved-and-keeps-it-on-screen"],
        the_handle_moves_the_radar_and_keeps_it_on_screen,
    ),
    (
        "a_moved_radar_writes_its_new_place_into_the_options",
        &["radar.drag.a-radar-the-player-has-moved-writes-its-new-place-into-his-own-options"],
        a_moved_radar_writes_its_new_place_into_the_options,
    ),
    (
        "a_correction_runs_at_the_bodys_own_speed",
        &["movement.correction.a-body-walks-a-correction-at-its-own-speed-and-not-a-default"],
        motion::a_correction_runs_at_the_bodys_own_speed,
    ),
    (
        "a_body_on_its_way_keeps_its_own_facing",
        &["movement.correction.a-body-on-its-way-somewhere-takes-the-place-and-keeps-its-own-facing"],
        motion::a_body_on_its_way_keeps_its_own_facing,
    ),
    (
        "a_running_creature_walks_at_the_rate_the_shard_sent",
        &["movement.correction.a-creature-the-shard-says-is-running-walks-at-the-rate-the-shard-gave-it"],
        motion::a_running_creature_walks_at_the_rate_the_shard_sent,
    ),
    (
        "a_creature_the_shard_said_nothing_about_walks_at_the_default",
        &["movement.correction.a-creature-the-shard-has-said-nothing-about-walks-at-the-clients-own-rate"],
        motion::a_creature_the_shard_said_nothing_about_walks_at_the_default,
    ),
    (
        "the_approach_is_the_shards_own",
        &["movement.approach.is-the-shards-and-the-client-never-starts-one-itself"],
        motion::the_approach_is_the_shards_own,
    ),
    (
        "an_approach_walks_only_to_what_is_out_of_reach",
        &["movement.approach.walks-to-what-is-out-of-reach-and-never-to-what-is-already-in-reach"],
        motion::an_approach_walks_only_to_what_is_out_of_reach,
    ),
    (
        "an_approach_ends_by_stopping_within_reach",
        &["movement.approach.ends-by-stopping-within-reach-and-nothing-else-happens"],
        motion::an_approach_ends_by_stopping_within_reach,
    ),
    (
        "after_an_approach_the_player_has_his_body_back",
        &["movement.approach.once-it-is-over-the-player-can-walk-the-body-himself-again"],
        motion::after_an_approach_the_player_has_his_body_back,
    ),
    (
        "a_target_that_leaves_the_world_ends_the_approach",
        &["movement.approach.a-target-that-leaves-the-world-ends-it-and-says-which-way-it-went"],
        motion::a_target_that_leaves_the_world_ends_the_approach,
    ),
    (
        "an_approach_gives_up_only_on_straying_too_far",
        &["movement.approach.gives-up-only-when-the-body-has-strayed-further-than-the-shard-allowed"],
        motion::an_approach_gives_up_only_on_straying_too_far,
    ),
    (
        "a_second_approach_replaces_the_first",
        &["movement.approach.a-second-one-replaces-the-first-rather-than-queueing"],
        motion::a_second_approach_replaces_the_first,
    ),
    (
        "the_target_is_re_read_on_a_gate",
        &["movement.approach.the-target-is-re-read-on-a-gate-and-only-when-it-has-moved"],
        motion::the_target_is_re_read_on_a_gate,
    ),
    (
        "an_exact_approach_does_not_go_through_what_is_in_the_way",
        &["movement.move-to.an-exact-approach-does-not-carry-the-body-through-what-is-in-the-way"],
        motion::an_exact_approach_does_not_go_through_what_is_in_the_way,
    ),
    (
        "a_replaced_animation_table_does_not_strand_the_body_in_walk",
        &["movement.run.a-body-whose-animation-table-is-replaced-still-runs-when-the-player-said-run"],
        motion::a_replaced_animation_table_does_not_strand_the_body_in_walk,
    ),
    (
        "a_walked_approach_leaves_the_players_own_movement_alone",
        &["movement.approach.a-walked-one-leaves-the-players-own-way-of-moving-alone"],
        motion::a_walked_approach_leaves_the_players_own_movement_alone,
    ),
    (
        "a_body_in_the_air_turns_but_does_not_walk",
        &["movement.jump.a-body-in-the-air-turns-but-does-not-walk-and-takes-up-the-held-key-when-it-lands"],
        motion::a_body_in_the_air_turns_but_does_not_walk,
    ),
    (
        "the_thrown_weapon_stance_reaches_the_body",
        &["movement.stance.the-one-the-shard-sends-for-a-thrown-weapon-reaches-the-body"],
        motion::the_thrown_weapon_stance_reaches_the_body,
    ),
    (
        "a_body_in_its_stance_can_attack_and_leave_combat",
        &["movement.stance.a-body-that-is-in-its-stance-can-attack-and-can-leave-combat-mode"],
        motion::a_body_in_its_stance_can_attack_and_leave_combat,
    ),
    (
        "a_delayed_scenery_effect_survives_the_frames_drain",
        &["scenery.animation.a-delayed-effect-still-happens-when-the-frame-empties-the-queue"],
        motion::a_delayed_scenery_effect_survives_the_frames_drain,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

// -------------------------------------------------------------------------------------------
// 19. movement.run.reports-walk-with-the-run-hold-key
// -------------------------------------------------------------------------------------------

/// Running moves the body at run speed and is reported as walking with the run key held.
pub fn running_reports_walk_with_the_run_hold_key() {
    use dereth_client::character::{Character, CharacterInput};
    use dereth_client::world::DEFAULT_LANDBLOCK;
    use dereth_primitives::LocalTime;

    /// The middle of the default landblock, where every movement scenario spawns.
    const SPAWN: (f32, f32) = (96.0, 96.0);
    /// The command a walk forward reports, as a literal: reading it back through the constant
    /// would not notice a wrong constant.
    const WALK_FORWARD: u32 = 0x4500_0005;
    /// The command a run forward would have reported, and never does.
    const RUN_FORWARD: u32 = 0x4400_0007;
    /// The run hold key. Invalid is 0, None is 1.
    const HOLD_KEY_RUN: u32 = 2;

    let mut c = HeadlessClient::new(ClientSpec::retail());

    c.assert_behaviour("movement.run.reports-walk-with-the-run-hold-key", |v| {
        let store = v.dat_store();
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let settled = |store: &_, region: &_| {
            let mut ch = Character::new(store, region, DEFAULT_LANDBLOCK, SPAWN)
                .expect("the body is created");
            for i in 1..=60 {
                ch.update(LocalTime(f64::from(i) / 30.0));
            }
            assert!(
                ch.on_ground(),
                "the body must settle before anything is measured"
            );
            ch
        };
        let hold = |ch: &mut Character, input: CharacterInput| -> f32 {
            ch.input = input;
            for i in 60..90 {
                ch.update(LocalTime(f64::from(i + 1) / 30.0));
            }
            let before = ch.position().frame.origin;
            for i in 90..120 {
                ch.update(LocalTime(f64::from(i + 1) / 30.0));
            }
            let after = ch.position().frame.origin;
            ((after.x - before.x).powi(2) + (after.y - before.y).powi(2)).sqrt()
        };

        let mut walker = settled(store, &region);
        let walked = hold(
            &mut walker,
            CharacterInput {
                forward: true,
                ..CharacterInput::default()
            },
        );
        let mut runner = settled(store, &region);
        let ran = hold(
            &mut runner,
            CharacterInput {
                forward: true,
                run: true,
                ..CharacterInput::default()
            },
        );
        println!("run vs walk: run {ran:.3} m/s vs walk {walked:.3} m/s");

        // The calibration: the body really is running.
        let faster = ran > walked * 1.5;
        // The measurement: what it reports is a walk with the run key held.
        let s = dereth_client::app::raw_motion_state_to_wire(
            &runner.driver().movement.interp.raw_state,
        );
        faster
            && s.forward_command == Some(WALK_FORWARD)
            && s.forward_command != Some(RUN_FORWARD)
            && s.current_holdkey == Some(HOLD_KEY_RUN)
            && s.forward_holdkey.is_none()
    });
    c.shutdown();
}

#[test]
fn scenario_running_reports_walk_with_the_run_hold_key() {
    scenario("running_reports_walk_with_the_run_hold_key");
}

// -------------------------------------------------------------------------------------------
// 20. physics.enter-world.refuses-a-body-inside-a-building
// -------------------------------------------------------------------------------------------

/// A body may only enter the world where it fits.
#[cfg(windows)]
pub fn enter_world_refuses_a_body_inside_a_building() {
    use std::sync::Arc;

    use dereth_client::land_source::DatLandSource;
    use dereth_physics::source::SetupGeometry;
    use dereth_physics::{LandSource, PhysicsWorld};
    use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};

    /// The landblock the house stands in.
    const BLOCK: u16 = 0xA9B4;
    /// The outdoor landcell the house's front doorway falls in.
    const OUTDOOR: u32 = 0xA9B4_0029;
    /// A room of the house.
    const ROOM: u32 = 0xA9B4_0143;
    const ROT: Quat = Quat::new(0.707_107, 0.0, 0.0, -0.707_107);

    let mut c = HeadlessClient::new(ClientSpec::retail());

    c.assert_behaviour(
        "physics.enter-world.refuses-a-body-inside-a-building",
        |v| {
            let store = Arc::clone(v.dat_store());
            let region = dereth_client::world::load_region(&store).expect("the region decodes");
            let land =
                Arc::new(DatLandSource::new(store, &region).expect("the height table validates"));
            land.load_block_cells(LandblockId(BLOCK));
            let _ = land.landblock(LandblockId(BLOCK));
            let mut w = PhysicsWorld::new(Arc::clone(&land) as Arc<dyn LandSource>);

            let mut enter = |n: u32, cell: u32, at: Vec3| -> Option<CellId> {
                let h = w.create(
                    ObjectId(0x5000_0000 + n),
                    Arc::new(SetupGeometry::default()),
                    true,
                );
                let ok = w.enter_world(h, &Position::new(CellId(cell), Frame::new(at, ROT)));
                let got = w.get(h).and_then(|o| o.cell);
                println!("enter_world({cell:#010X}) -> {ok}, cell {got:?}");
                got
            };

            // Bare terrain, well clear of the house: nothing may refuse this.
            let open = enter(1, OUTDOOR, Vec3::new(133.00, 5.15, 94.20));
            // The doorway's outer face: still outside the shell.
            let doorway = enter(2, OUTDOOR, Vec3::new(136.289_993, 5.155, 94.082_001));
            // Half a metre further in: inside the shell, carrying the outdoor cell.
            let inner = enter(3, OUTDOOR, Vec3::new(136.90, 5.155, 94.082_001));
            // Two metres into the room, still carrying the outdoor cell.
            let room_as_outdoor = enter(4, OUTDOOR, Vec3::new(138.60, 7.00, 94.082_001));
            // The same point, carrying the room that contains it.
            let interior = enter(5, ROOM, Vec3::new(138.60, 7.00, 94.082_001));

            // The premises first: an instrument that refuses everything would report the same
            // "nothing" as one that is measuring a building.
            let premises = open == Some(CellId(OUTDOOR))
                && doorway == Some(CellId(OUTDOOR))
                && interior == Some(CellId(ROOM));
            // The finding.
            premises && inner.is_none() && room_as_outdoor.is_none()
        },
    );
    c.shutdown();
}

#[cfg(not(windows))]
pub fn enter_world_refuses_a_body_inside_a_building() {
    panic!("the collision walk this scenario drives is built on Windows only");
}

#[test]
#[cfg(windows)]
fn scenario_enter_world_refuses_a_body_inside_a_building() {
    scenario("enter_world_refuses_a_body_inside_a_building");
}

// -------------------------------------------------------------------------------------------
// movement.teleport.arrives-exactly-on-another-player-and-is-pushed-aside-only-by-a-creature
// -------------------------------------------------------------------------------------------

/// Teleporting onto another player's exact spot lands there; a creature overlapping it does not.
///
/// The whole measurement is one arrival: settle a body, note the ground-true destination, stand the
/// scene's bodies on it, move the local body away, then run the production teleport back onto it.
pub fn a_teleport_onto_another_player_is_deflected_only_by_a_creature() {
    use dereth_client::character::{CharacterInput, MovementCommands};
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{LocalTime, ObjectId, Position};

    const VICTIM: ObjectId = ObjectId(0x8000_1114);
    const MONSTER: ObjectId = ObjectId(0x8000_1115);

    /// What stands at the destination before the local body teleports onto it.
    #[derive(Debug, Clone, Copy, Default)]
    struct Scene {
        /// A remote **player** exactly on the destination.
        victim: bool,
        /// A remote **creature** this far along the x axis from the destination, if any.
        monster_at: Option<f32>,
        /// Make the arriving body pass through everything first -- the control for "is the
        /// deflection really the creature's collision", not a claim that the client sets it.
        ethereal_mover: bool,
    }

    /// `(offset, the gap between the creature's body and the destination)`.
    fn teleport_onto(scene: Scene) -> (f32, f32) {
        let store = store();
        let mut c = fresh_local_body(&store);

        // The destination is a real settled ground position, so the control has no drop to absorb.
        let destination = c.position();

        // Stand the local body somewhere else **before** the remote bodies are placed: the local
        // body is a solid player, and a creature placed on top of it would be moved aside by the
        // client's own placement, silently emptying the destination.
        let mut away = destination;
        away.frame.origin.x -= 12.0;
        c.teleport(away);
        assert!(
            c.position()
                .frame
                .origin
                .sub(destination.frame.origin)
                .mag2()
                .sqrt()
                > 8.0,
            "the body did not actually leave the destination"
        );

        let mut stream = dereth_client::objects::ObjectStream::new();
        if scene.victim {
            stream.apply_event(
                &create_event(VICTIM, Some(destination), PLAYER, "Victim"),
                LocalTime(9.0),
            );
        }
        // Each create is synced on its own, because this client places a body when its position
        // arrives and a second body therefore meets the first one already standing there.
        stream.sync_physics(&store, &mut c.world);
        let mut monster_gap = f32::INFINITY;
        if let Some(d) = scene.monster_at {
            let mut at = destination;
            at.frame.origin.x += d;
            stream.apply_event(
                &create_event(MONSTER, Some(at), 0, "Monster"),
                LocalTime(9.0),
            );
            stream.sync_physics(&store, &mut c.world);
            let h = c
                .world
                .by_object_id(MONSTER)
                .expect("the creature has a body");
            let b = c.world.get(h).expect("the creature's body");
            assert!(b.cell.is_some(), "the creature is in no cell");
            let w = b.weenie.as_ref().expect("the creature's weenie half");
            assert!(
                w.is_creature && !w.is_player,
                "the creature must not be a player"
            );
            monster_gap = b
                .position
                .frame
                .origin
                .sub(destination.frame.origin)
                .mag2()
                .sqrt();
        }
        if scene.victim {
            let h = c.world.by_object_id(VICTIM).expect("the victim has a body");
            let b = c.world.get(h).expect("the victim's body");
            assert!(b.cell.is_some(), "the victim is in no cell");
            assert!(
                b.weenie
                    .as_ref()
                    .expect("the victim's weenie half")
                    .is_player,
                "the victim's own description did not reach the collision body"
            );
        }

        if scene.ethereal_mover {
            let h = c.handle;
            let o = c.world.get_mut(h).expect("the local body exists");
            let mut s = o.state();
            s.set_ethereal_bit(true);
            let _ = o.set_state(s);
        }

        // The production arrival, which is what an accepted teleport reaches.
        let mut movement = MovementCommands::default();
        let mut input = CharacterInput::default();
        dereth_client::app::complete_player_teleport_at(
            destination,
            &mut c,
            &mut movement,
            &mut input,
            |_| {},
        );
        let landed: Position = c.position();
        let offset = landed
            .frame
            .origin
            .sub(destination.frame.origin)
            .mag2()
            .sqrt();
        (offset, monster_gap)
    }

    // Nothing at the destination: the control the whole table is read against.
    let (empty, _) = teleport_onto(Scene::default());
    // Another ordinary player standing exactly on it.
    let (onto_player, _) = teleport_onto(Scene {
        victim: true,
        ..Scene::default()
    });
    println!("arrive empty destination offset {empty:.6}; onto another player {onto_player:.6}");

    // A creature, across a sweep of separations, with no player present so that the creature's
    // own placement is not itself deflected and the separation is the real one.
    let mut table = Vec::new();
    for d in [
        0.1_f32, 0.25, 0.5, 0.75, 0.9, 0.95, 0.96, 1.0, 1.25, 1.5, 2.0,
    ] {
        let (offset, gap) = teleport_onto(Scene {
            monster_at: Some(d),
            ..Scene::default()
        });
        println!("arrive creature asked at {d:.2} m -> gap {gap:.4} m, arrival offset {offset:.6}");
        table.push((d, gap, offset));
    }
    let worst = table.iter().copied().fold(0.0_f32, |m, (_, _, o)| m.max(o));

    // The control that says the deflection is the creature's collision and nothing else: the same
    // creature, a mover that passes through everything.
    let (solid, solid_gap) = teleport_onto(Scene {
        monster_at: Some(0.25),
        ..Scene::default()
    });
    let (ethereal, _) = teleport_onto(Scene {
        monster_at: Some(0.25),
        ethereal_mover: true,
        ..Scene::default()
    });
    println!(
        "arrive solid mover offset {solid:.6} (creature gap {solid_gap:.4}); ethereal mover \
         {ethereal:.6}"
    );

    let lands_exactly = empty < 1e-3 && onto_player < 1e-3;
    // A creature really does push the arrival, and never further than the client's own search.
    let creature_pushes = worst > 1e-3 && worst <= 4.0;
    let passes_through = solid > 1e-3 && ethereal < 1e-3;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.teleport.arrives-exactly-on-another-player-and-is-pushed-aside-only-by-a-creature",
        move |_| lands_exactly && creature_pushes && passes_through,
    );
}

#[test]
fn scenario_a_teleport_onto_another_player_is_deflected_only_by_a_creature() {
    scenario("a_teleport_onto_another_player_is_deflected_only_by_a_creature");
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
mod teleloc {
    use dereth_client::character::{Character, CharacterInput, MovementCommands};
    use dereth_client_net::client_session::testing::MockTransport;
    use dereth_client_net::client_session::{PositionReporter, Session, SessionEvent};
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{CellId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
    use dereth_protocol::actions::unpack_action;
    use dereth_protocol::movement::{MoveTimestamps, MovementAutonomousPosition};
    use dereth_protocol::types::PositionWire;
    use dereth_protocol::Message;

    use super::support::{create_event, store, PLAYER};

    pub const LOCAL: ObjectId = ObjectId(0x8000_114F);
    pub const REMOTE: ObjectId = ObjectId(0x8000_1150);

    /// The reported destination, from the evidence behind this claim's handle.
    const OWNER_CELL: u32 = 0x01F9_01E7;
    const OWNER_ORIGIN: [f32; 3] = [55.310642, -59.855103, 12.004999];
    const OWNER_ROTATION: [f32; 4] = [-0.961667, 0.0, 0.0, 0.274220];

    /// What stands on (or beside) the destination, and what the local body's description says.
    #[derive(Debug, Clone, Copy)]
    pub struct Scene {
        /// The local player's own description bits.
        pub local: u32,
        /// A body standing **exactly** on the destination, with these description bits.
        pub on_the_spot: Option<u32>,
        /// A body this far along the x axis from the destination instead.
        pub beside: Option<(f32, u32)>,
    }

    impl Scene {
        pub const fn npk_onto(remote: u32) -> Self {
            Self {
                local: PLAYER,
                on_the_spot: Some(remote),
                beside: None,
            }
        }
    }

    /// What one measured arrival produced.
    #[derive(Debug, Clone, Copy)]
    pub struct Arrival {
        pub landed: Position,
        pub offset: f32,
        /// What the next outbound position report actually carried.
        pub reported: PositionWire,
    }

    fn owner_position() -> Position {
        Position::new(
            CellId(OWNER_CELL),
            Frame::new(
                Vec3::new(OWNER_ORIGIN[0], OWNER_ORIGIN[1], OWNER_ORIGIN[2]),
                Quat {
                    w: OWNER_ROTATION[0],
                    x: OWNER_ROTATION[1],
                    y: OWNER_ROTATION[2],
                    z: OWNER_ROTATION[3],
                },
            ),
        )
    }

    /// A settled local body standing on the reported spot, so the destination below is a real
    /// ground position and no measurement is absorbing a drop.
    fn settled_on_the_owner_spot(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
    ) -> (Character, Position) {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let mut c = Character::new(
            store,
            &region,
            dereth_client::world::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("the ordinary local body is created");
        let there = owner_position();
        c.land().load_block_cells(there.cell.landblock());
        c.teleport(there);
        for i in 0..=60 {
            c.update(LocalTime(f64::from(i) / 30.0));
        }
        assert!(
            c.on_ground(),
            "the reported destination cell must support a body: {:?}",
            c.position()
        );
        let settled = c.position();
        assert_eq!(
            settled.cell, there.cell,
            "the body left the reported destination cell while settling"
        );
        assert!(
            settled.frame.origin.sub(there.frame.origin).mag2().sqrt() < 1e-3,
            "those coordinates are not a settled standing position: {:?}",
            settled.frame.origin
        );
        (c, settled)
    }

    /// The whole measurement, through the production path end to end.
    pub fn teleport_onto(scene: Scene) -> (Arrival, Position) {
        let store = store();
        let (mut c, destination) = settled_on_the_owner_spot(&store);

        // Stand the local body somewhere else first, so the remote body is placed on an empty
        // destination and the arrival is the only thing being measured.
        let mut away = destination;
        away.frame.origin.x -= 12.0;
        c.teleport(away);
        assert!(
            c.position()
                .frame
                .origin
                .sub(destination.frame.origin)
                .mag2()
                .sqrt()
                > 8.0,
            "the body did not actually leave the destination"
        );

        // The local half, as the connected client gets it.
        let mut stream = dereth_client::objects::ObjectStream::new();
        stream.apply_event(&SessionEvent::PlayerCreated(LOCAL), LocalTime(1.0));
        stream.apply_event(
            &create_event(LOCAL, None, scene.local, "Me"),
            LocalTime(1.0),
        );
        c.adopt_server_id(stream.player());
        stream.sync_physics(&store, &mut c.world);
        let local_weenie = c
            .world
            .get(c.handle)
            .and_then(|o| o.weenie.clone())
            .expect("the local body's weenie half is pushed by the object stream");
        assert_eq!(
            local_weenie.is_player,
            scene.local & PLAYER != 0,
            "the local body's own description is not the one its create carried"
        );

        // The remote half, an ordinary create for a body standing where the other player stood.
        let remote = scene
            .on_the_spot
            .map(|bits| (destination, bits))
            .or_else(|| {
                scene.beside.map(|(d, bits)| {
                    let mut at = destination;
                    at.frame.origin.x += d;
                    (at, bits)
                })
            });
        if let Some((at, bits)) = remote {
            stream.apply_event(
                &create_event(REMOTE, Some(at), bits, "Them"),
                LocalTime(9.0),
            );
            stream.sync_physics(&store, &mut c.world);
            let h = c
                .world
                .by_object_id(REMOTE)
                .expect("the remote body exists");
            let b = c.world.get(h).expect("the remote body");
            assert!(b.cell.is_some(), "the remote body is in no cell");
            let w = b.weenie.as_ref().expect("the remote body's weenie half");
            assert_eq!(
                w.is_player,
                bits & PLAYER != 0,
                "the remote description did not arrive"
            );
            assert!(
                b.position.frame.origin.sub(at.frame.origin).mag2().sqrt() < 1e-3,
                "the remote body was not placed where the shard put it: {:?}",
                b.position.frame.origin
            );
        }

        // The arrival: the production call an accepted teleport reaches.
        let mut movement = MovementCommands::default();
        let mut input = CharacterInput::default();
        dereth_client::app::complete_player_teleport_at(
            destination,
            &mut c,
            &mut movement,
            &mut input,
            |_| {},
        );
        let landed = c.position();
        let offset = landed
            .frame
            .origin
            .sub(destination.frame.origin)
            .mag2()
            .sqrt();

        // And what the client then tells the shard, through the real producer.
        let mut reporter = PositionReporter::new(0.0);
        reporter.active = true;
        let mut session = Session::new(MockTransport::new());
        let motion = dereth_client::app::body_motion(&c, MoveTimestamps::default());
        assert!(
            motion.contact,
            "a settled arrival must be in contact or nothing is reported"
        );
        assert!(
            reporter.should_send_position_event(2.0, &motion),
            "the arrival changed the position, so the schedule must want a report"
        );
        reporter.send_position_event(2.0, &motion, &mut session);
        let blob = session
            .transport
            .sent
            .first()
            .expect("a report was emitted")
            .payload
            .clone();
        let mut action = unpack_action(&blob).expect("the producer emits a game action");
        let sent = MovementAutonomousPosition::read(&mut action.body).expect("the body decodes");
        action
            .body
            .expect_exhausted()
            .expect("the body is fully consumed");

        (
            Arrival {
                landed,
                offset,
                reported: sent.0.position,
            },
            destination,
        )
    }

    /// Bit for bit on the requested position, and the report says so too.
    pub fn landed_exactly(a: &Arrival, destination: Position) -> bool {
        a.landed.cell == destination.cell
            && a.landed.frame.origin.x.to_bits() == destination.frame.origin.x.to_bits()
            && a.landed.frame.origin.y.to_bits() == destination.frame.origin.y.to_bits()
            && a.landed.frame.origin.z.to_bits() == destination.frame.origin.z.to_bits()
            && a.reported.objcell_id == destination.cell.0
            && a.reported.frame.origin.x.to_bits() == destination.frame.origin.x.to_bits()
            && a.reported.frame.origin.y.to_bits() == destination.frame.origin.y.to_bits()
            && a.reported.frame.origin.z.to_bits() == destination.frame.origin.z.to_bits()
    }

    /// Pushed aside, and the client reports where it really is rather than where it was sent.
    pub fn pushed_aside(a: &Arrival, destination: Position) -> bool {
        a.offset > 0.4
            && (a.landed.frame.origin.x - destination.frame.origin.x).abs() < 1e-4
            && (a.reported.frame.origin.y - a.landed.frame.origin.y).abs() < 1e-6
    }
}

/// Arriving on another player's exact spot, with both bodies delivered the way a logged-in client
/// receives them, lands there to the float -- and the report the client then sends says so.
pub fn an_arrival_through_the_login_path_lands_on_the_other_players_spot() {
    use teleloc::{landed_exactly, teleport_onto, Scene};

    let mut exact = true;
    for (what, remote) in [
        ("onto an ordinary player", PLAYER),
        ("onto a player-killer -- the reported pair", PLAYER | PK),
        ("onto a player-killer-lite", PLAYER | PK_LITE),
    ] {
        let (a, destination) = teleport_onto(Scene::npk_onto(remote));
        println!("onto: {what}: offset {:.6} m", a.offset);
        exact &= landed_exactly(&a, destination);
    }

    // The other direction of the same pairing, so the rule is shown to be symmetric.
    let (a, destination) = teleport_onto(Scene {
        local: PLAYER | PK,
        on_the_spot: Some(PLAYER),
        beside: None,
    });
    println!(
        "onto: a player-killer onto an ordinary player: offset {:.6} m",
        a.offset
    );
    exact &= landed_exactly(&a, destination);

    // The control with nothing there at all, so the cell itself is not what is being measured.
    let (a, destination) = teleport_onto(Scene {
        local: PLAYER,
        on_the_spot: None,
        beside: None,
    });
    println!("onto: an empty destination: offset {:.6} m", a.offset);
    exact &= landed_exactly(&a, destination);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.teleport.a-player-arriving-on-another-player-through-the-shards-own-login-is-not-deflected",
        move |_| exact,
    );
}

#[test]
fn scenario_an_arrival_through_the_login_path_lands_on_the_other_players_spot() {
    scenario("an_arrival_through_the_login_path_lands_on_the_other_players_spot");
}

/// The pairings that are **not** exempt, and the creature control that bounds the rule.
pub fn two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt() {
    use teleloc::{landed_exactly, pushed_aside, teleport_onto, Scene};

    let mut collides = true;
    for (what, local, remote) in [
        ("two player-killers", PLAYER | PK, PLAYER | PK),
        (
            "two player-killer-lites",
            PLAYER | PK_LITE,
            PLAYER | PK_LITE,
        ),
        ("onto an impenetrable body", PLAYER, PLAYER | IMPENETRABLE),
        ("an impenetrable mover", PLAYER | IMPENETRABLE, PLAYER),
        // Either half's "this is a player" answer missing is the same thing to the collision
        // walk as not being a player at all.
        ("a mover that is not a player", 0, PLAYER | PK),
        ("a candidate that is not a player", PLAYER, PK),
        // And a creature is never in the exemption.
        ("a creature on the spot", PLAYER, 0),
    ] {
        let (a, destination) = teleport_onto(Scene {
            local,
            on_the_spot: Some(remote),
            beside: None,
        });
        println!(
            "onto: {what}: offset {:.6} m, y {}",
            a.offset, a.landed.frame.origin.y
        );
        collides &= pushed_aside(&a, destination);
    }

    // The known-negative in the other direction: the same creature a full body diameter away
    // overlaps nothing, and the arrival is exact again.
    let (clear, destination) = teleport_onto(Scene {
        local: PLAYER,
        on_the_spot: None,
        beside: Some((1.0, 0)),
    });
    println!(
        "onto: a creature a body diameter away: offset {:.6} m",
        clear.offset
    );
    let exempt_again = landed_exactly(&clear, destination);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.teleport.two-players-pass-through-each-other-only-while-neither-is-marked-for-combat",
        move |_| collides && exempt_again,
    );
}

#[test]
fn scenario_two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt() {
    scenario("two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt");
}

// -------------------------------------------------------------------------------------------
// movement.correction.an-update-that-does-not-say-the-body-is-on-the-ground-moves-it-nowhere
// movement.remote-body.a-create-onto-an-occupied-spot-is-put-down-beside-it
// -------------------------------------------------------------------------------------------

/// The scene an occupied-spot create starts from: the local body parked well away, a creature a
/// quarter of a metre along the x axis from the destination, and a remote player created in the
/// clear three metres further on, ticked until the client has really simulated it.
fn occupied_scene(
    store: &std::sync::Arc<dereth_dat::RetailDatStore>,
    victim: dereth_primitives::ObjectId,
    monster: dereth_primitives::ObjectId,
) -> (
    dereth_client::character::Character,
    dereth_client::objects::ObjectStream,
    dereth_primitives::Position,
) {
    use dereth_primitives::LocalTime;

    let mut c = fresh_local_body(store);
    let destination = c.position();

    // Out of the way first: a creature placed on top of the local player's solid body would be
    // moved aside by its own create and the separation under test would not be the one asked for.
    let mut away = destination;
    away.frame.origin.x -= 12.0;
    c.teleport(away);

    let mut stream = dereth_client::objects::ObjectStream::new();
    let mut monster_at = destination;
    monster_at.frame.origin.x += 0.25;
    stream.apply_event(
        &create_event(monster, Some(monster_at), 0, "Monster"),
        LocalTime(9.0),
    );
    let mut clear = destination;
    clear.frame.origin.x += 3.0;
    stream.apply_event(
        &create_event(victim, Some(clear), PLAYER, "Victim"),
        LocalTime(9.0),
    );
    stream.sync_physics(store, &mut c.world);

    assert_eq!(
        body_origin(&c, monster).expect("the creature is in a cell"),
        monster_at.frame.origin,
        "the creature's own create was moved, so the separation under test is not the one asked for"
    );
    assert_eq!(
        body_origin(&c, victim).expect("the player is in a cell"),
        clear.frame.origin,
        "the player's create landed off its own clear position"
    );

    // A body the physics tick has never reached carries no distance to the player, and the arm
    // under test reads it. Two ticks of the real gate, as the connected client runs.
    for i in 61..=70 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    (c, stream, destination)
}

/// A position update that does not say the body is on the ground repositions nothing.
pub fn an_update_with_no_ground_contact_moves_the_body_nowhere() {
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{LocalTime, ObjectId};

    const VICTIM: ObjectId = ObjectId(0x8000_114B);
    const MONSTER: ObjectId = ObjectId(0x8000_114C);

    let store = store();
    let (mut c, mut stream, destination) = occupied_scene(&store, VICTIM, MONSTER);
    let before = body_origin(&c, VICTIM).expect("the player is in a cell");

    stream.apply_event(
        &position_event(
            VICTIM,
            destination.cell.raw(),
            destination.frame.origin,
            1,
            0,
            false,
        ),
        LocalTime(10.0),
    );
    stream.sync_physics(&store, &mut c.world);

    let after = body_origin(&c, VICTIM).expect("the player is still in a cell");
    println!("correction: an update with no contact: the body stayed at {after:?}");
    let unmoved = after == before;
    // And the position it carried really was somewhere else, or this proves nothing.
    let elsewhere = after.sub(destination.frame.origin).mag2().sqrt() > 2.0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.correction.an-update-that-does-not-say-the-body-is-on-the-ground-moves-it-nowhere",
        move |_| unmoved && elsewhere,
    );
}

#[test]
fn scenario_an_update_with_no_ground_contact_moves_the_body_nowhere() {
    scenario("an_update_with_no_ground_contact_moves_the_body_nowhere");
}

/// A body created onto a spot something else already fills is put down beside it.
///
/// The same holds for a remote body the shard parks against a creature.
pub fn a_create_onto_an_occupied_spot_is_put_down_beside_it() {
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{LocalTime, ObjectId};

    const VICTIM: ObjectId = ObjectId(0x8000_114B);
    const MONSTER: ObjectId = ObjectId(0x8000_114C);

    let store = store();
    let mut c = fresh_local_body(&store);
    let destination = c.position();
    let mut away = destination;
    away.frame.origin.x -= 12.0;
    c.teleport(away);

    let mut stream = dereth_client::objects::ObjectStream::new();
    let mut monster_at = destination;
    monster_at.frame.origin.x += 0.25;
    stream.apply_event(
        &create_event(MONSTER, Some(monster_at), 0, "Monster"),
        LocalTime(9.0),
    );
    stream.sync_physics(&store, &mut c.world);
    // The player is *created* straight onto the creature.
    stream.apply_event(
        &create_event(VICTIM, Some(destination), PLAYER, "Victim"),
        LocalTime(9.0),
    );
    stream.sync_physics(&store, &mut c.world);

    let landed = body_origin(&c, VICTIM).expect("the created player is in a cell");
    let offset = landed.sub(destination.frame.origin).mag2().sqrt();
    println!("correction: a create onto an occupied spot: offset {offset:.6} m");
    let beside_it = offset > 0.1 && offset <= 4.0;

    // The known-negative: the same create onto an empty spot lands exactly on it.
    let mut c2 = fresh_local_body(&store);
    let empty = c2.position();
    let mut away2 = empty;
    away2.frame.origin.x -= 12.0;
    c2.teleport(away2);
    let mut stream2 = dereth_client::objects::ObjectStream::new();
    stream2.apply_event(
        &create_event(VICTIM, Some(empty), PLAYER, "Victim"),
        LocalTime(9.0),
    );
    stream2.sync_physics(&store, &mut c2.world);
    let exact = body_origin(&c2, VICTIM)
        .expect("the created player is in a cell")
        .sub(empty.frame.origin)
        .mag2()
        .sqrt()
        < 1e-4;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.remote-body.a-create-onto-an-occupied-spot-is-put-down-beside-it",
        move |_| beside_it && exact,
    );
}

#[test]
fn scenario_a_create_onto_an_occupied_spot_is_put_down_beside_it() {
    scenario("a_create_onto_an_occupied_spot_is_put_down_beside_it");
}

// -------------------------------------------------------------------------------------------
// movement.correction.a-nearby-correction-glides-the-body-over-the-following-frames
// movement.correction.a-correction-out-of-reach-is-taken-in-one-step-instead-of-a-glide
// movement.correction.a-body-the-client-has-not-simulated-yet-is-put-straight-onto-the-shards-position
// movement.correction.a-teleport-out-of-the-world-drops-a-glide-that-was-already-in-flight
// -------------------------------------------------------------------------------------------

/// The scene the four glide scenarios share: two real ground positions three metres apart, the
/// local body parked twelve metres from both, and the remote player standing on the first with a
/// part array and a real distance to the player.
///
/// Returns the body, the stream, the clock and the destination the shard will name.
fn glide_scene(
    store: &std::sync::Arc<dereth_dat::RetailDatStore>,
    id: dereth_primitives::ObjectId,
    tick_it: bool,
) -> (
    dereth_client::character::Character,
    dereth_client::objects::ObjectStream,
    f64,
    dereth_primitives::Position,
) {
    use dereth_physics::math::V3 as _;
    use dereth_primitives::LocalTime;

    let mut c = fresh_local_body(store);
    let mut t = 3.0;
    let start = settled_at(&mut c, 96.0, 96.0, &mut t);
    let destination = settled_at(&mut c, 99.0, 96.0, &mut t);
    // Out of the way: a local body standing on either spot would be collided with.
    let _ = settled_at(&mut c, 96.0, 84.0, &mut t);

    let mut stream = dereth_client::objects::ObjectStream::new();
    stream.apply_event(
        &create_event(id, Some(start), PLAYER, "Victim"),
        LocalTime(t),
    );
    stream.sync_physics(store, &mut c.world);
    assert!(
        body_origin(&c, id)
            .expect("the body is in a cell")
            .sub(start.frame.origin)
            .mag2()
            .sqrt()
            < 1e-4,
        "the create was moved aside, so the glide under test does not start where it says"
    );
    if tick_it {
        give_part_array(&mut c, id);
        for _ in 0..4 {
            t += DT;
            c.update(LocalTime(t));
        }
    }
    (c, stream, t, destination)
}

/// A nearby correction queues a glide and moves nothing, and finishes it over the next frames.
pub fn a_nearby_correction_glides_the_body_over_the_following_frames() {
    use dereth_physics::math::V3 as _;
    use dereth_physics::pmanager::CLOSE_ENOUGH;
    use dereth_primitives::{LocalTime, ObjectId};

    const VICTIM: ObjectId = ObjectId(0x8000_114C);
    const MONSTER: ObjectId = ObjectId(0x8000_114D);

    let store = store();
    let (mut c, mut stream, mut t, destination) = glide_scene(&store, VICTIM, true);
    let before = body_origin(&c, VICTIM).expect("the body is in a cell");
    let distance = before.sub(destination.frame.origin).mag2().sqrt();
    assert!(
        distance > 2.0,
        "the glide under test is only {distance:.3} m long"
    );
    let arms = stream.physics.stats.interpolate_arm;

    stream.apply_event(
        &position_event(
            VICTIM,
            destination.cell.raw(),
            destination.frame.origin,
            1,
            0,
            true,
        ),
        LocalTime(t),
    );
    stream.sync_physics(&store, &mut c.world);

    let h = c.world.by_object_id(VICTIM).expect("the body exists");
    let queued_not_moved = stream.physics.stats.interpolate_arm == arms + 1
        && body_origin(&c, VICTIM) == Some(before)
        && c.world.is_interpolating(h);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (distance / STEP).ceil() as usize + 1;
    for _ in 0..steps {
        t += DT;
        c.update(LocalTime(t));
    }
    let offset = body_origin(&c, VICTIM)
        .expect("the body is in a cell")
        .sub(destination.frame.origin)
        .mag2()
        .sqrt();
    println!("glide: after {steps} sub-steps the glide is {offset:.6} m out");
    let finished = offset < CLOSE_ENOUGH && !c.world.is_interpolating(h);

    // And a creature standing in the way stops the glide short rather than being pushed out of
    // it.
    let (mut c2, mut stream2, destination2) = occupied_scene(&store, VICTIM, MONSTER);
    let standing = body_origin(&c2, VICTIM).expect("the player is in a cell");
    stream2.apply_event(
        &position_event(
            VICTIM,
            destination2.cell.raw(),
            destination2.frame.origin,
            1,
            0,
            true,
        ),
        LocalTime(10.0),
    );
    stream2.sync_physics(&store, &mut c2.world);
    let no_placement = body_origin(&c2, VICTIM) == Some(standing);
    let mut t2 = 10.0;
    for _ in 0..10 {
        t2 += DT;
        c2.update(LocalTime(t2));
    }
    let walked = body_origin(&c2, VICTIM).expect("the player is still in a cell");
    let covered = walked.sub(standing).mag2().sqrt();
    let short = walked.sub(destination2.frame.origin).mag2().sqrt();
    println!("correction: beside a creature: covered {covered:.4} m, {short:.4} m short");
    let creature_stops_it = covered > 1.0
        && short > CLOSE_ENOUGH
        && body_origin(&c2, MONSTER).map(|o| o.sub(destination2.frame.origin).x) == Some(0.25);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.correction.a-nearby-correction-glides-the-body-over-the-following-frames",
        move |_| queued_not_moved && finished && no_placement && creature_stops_it,
    );
}

#[test]
fn scenario_a_nearby_correction_glides_the_body_over_the_following_frames() {
    scenario("a_nearby_correction_glides_the_body_over_the_following_frames");
}

/// A correction further than the body could have walked is taken in one step.
pub fn a_correction_out_of_reach_is_taken_in_one_step() {
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{LocalTime, ObjectId};

    const VICTIM: ObjectId = ObjectId(0x8000_114C);

    let store = store();
    let (mut c, mut stream, mut t, _destination) = glide_scene(&store, VICTIM, true);
    let before = body_origin(&c, VICTIM).expect("the body is in a cell");
    let h = c.world.by_object_id(VICTIM).expect("the body exists");
    let reach = c.world.autonomy_blip_distance(h);
    assert_eq!(
        reach,
        dereth_physics::globals::AUTONOMY_BLIP_OUTDOORS,
        "an outdoor cell carries the outdoor reach"
    );

    // Far enough to be out of reach and still inside the one landblock this client has loaded.
    let far = settled_at(&mut c, 176.0, 16.0, &mut t);
    let _ = settled_at(&mut c, 96.0, 84.0, &mut t);
    let asked = before.sub(far.frame.origin).mag2().sqrt();
    assert!(
        asked > reach,
        "only {asked:.3} m away, which is still within reach"
    );

    stream.apply_event(
        &position_event(VICTIM, far.cell.raw(), far.frame.origin, 1, 0, true),
        LocalTime(t),
    );
    stream.sync_physics(&store, &mut c.world);

    let moved = body_origin(&c, VICTIM)
        .expect("the body is in a cell")
        .sub(before)
        .mag2()
        .sqrt();
    let landed = body_origin(&c, VICTIM)
        .expect("the body is in a cell")
        .sub(far.frame.origin)
        .mag2()
        .sqrt();
    println!("glide: out of reach: {moved:.3} m at once; one glide step would be {STEP:.3} m");
    let in_one_step = moved > reach && landed < 0.5 && !c.world.is_interpolating(h);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.correction.a-correction-out-of-reach-is-taken-in-one-step-instead-of-a-glide",
        move |_| in_one_step,
    );
}

#[test]
fn scenario_a_correction_out_of_reach_is_taken_in_one_step() {
    scenario("a_correction_out_of_reach_is_taken_in_one_step");
}

/// A body the client has not simulated yet is put straight onto the position the shard named.
pub fn a_body_not_yet_simulated_is_put_straight_onto_the_shards_position() {
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{LocalTime, ObjectId};

    const VICTIM: ObjectId = ObjectId(0x8000_114C);

    let store = store();
    // Deliberately no tick: the body has never been reached by the physics pass.
    let (mut c, mut stream, t, destination) = glide_scene(&store, VICTIM, false);
    let h = c.world.by_object_id(VICTIM).expect("the body exists");
    assert!(
        c.world.get(h).expect("the body exists").player_distance >= 96.0,
        "the body has been reached by the physics pass, so this is a different arm"
    );
    let snaps = stream.physics.stats.snap_arm;

    stream.apply_event(
        &position_event(
            VICTIM,
            destination.cell.raw(),
            destination.frame.origin,
            1,
            0,
            true,
        ),
        LocalTime(t),
    );
    stream.sync_physics(&store, &mut c.world);

    let offset = body_origin(&c, VICTIM)
        .expect("the body is in a cell")
        .sub(destination.frame.origin)
        .mag2()
        .sqrt();
    println!("glide: a body never simulated: offset {offset:.6} m with no tick at all");
    let straight_there =
        stream.physics.stats.snap_arm == snaps + 1 && offset < 0.5 && !c.world.is_interpolating(h);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.correction.a-body-the-client-has-not-simulated-yet-is-put-straight-onto-the-shards-position",
        move |_| straight_there,
    );
}

#[test]
fn scenario_a_body_not_yet_simulated_is_put_straight_onto_the_shards_position() {
    scenario("a_body_not_yet_simulated_is_put_straight_onto_the_shards_position");
}

/// A teleport into land the client has not loaded drops a glide that was already in flight.
pub fn a_teleport_out_of_the_world_drops_a_glide_in_flight() {
    use dereth_primitives::{LocalTime, ObjectId, Vec3};

    const VICTIM: ObjectId = ObjectId(0x8000_114C);
    /// A landblock this client has never loaded.
    const UNLOADED_CELL: u32 = 0x0102_0101;
    let lost_at = Vec3::new(30.0, 30.0, 40.0);

    let store = store();
    let (mut c, mut stream, mut t, destination) = glide_scene(&store, VICTIM, true);

    // Arm a glide first, so the teleport has something to out-rank.
    stream.apply_event(
        &position_event(
            VICTIM,
            destination.cell.raw(),
            destination.frame.origin,
            1,
            0,
            true,
        ),
        LocalTime(t),
    );
    stream.sync_physics(&store, &mut c.world);
    let h = c.world.by_object_id(VICTIM).expect("the body exists");
    assert!(
        c.world.is_interpolating(h),
        "no glide was queued, so nothing is being out-ranked"
    );

    t += DT;
    stream.apply_event(
        &position_event(VICTIM, UNLOADED_CELL, lost_at, 2, 1, true),
        LocalTime(t),
    );
    stream.sync_physics(&store, &mut c.world);

    let o = c.world.get(h).expect("the body still exists");
    let out_of_the_world =
        o.cell.is_none() && !o.transient_state.is_active() && o.position.frame.origin == lost_at;

    // Nothing puts it back, and the stale glide does not drag it anywhere.
    for _ in 0..8 {
        t += DT;
        c.update(LocalTime(t));
    }
    let stays_out = body_origin(&c, VICTIM).is_none()
        && c.world
            .get(h)
            .expect("the body still exists")
            .position
            .frame
            .origin
            == lost_at;
    println!("glide: a teleport out of the world: out {out_of_the_world}, stays {stays_out}");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.correction.a-teleport-out-of-the-world-drops-a-glide-that-was-already-in-flight",
        move |_| out_of_the_world && stays_out,
    );
}

#[test]
fn scenario_a_teleport_out_of_the_world_drops_a_glide_in_flight() {
    scenario("a_teleport_out_of_the_world_drops_a_glide_in_flight");
}

// -------------------------------------------------------------------------------------------
// movement.correction.a-destination-in-a-block-the-client-has-not-loaded-takes-the-body-out-of-the-world
// -------------------------------------------------------------------------------------------

/// Both distances, both residencies: four drives of the same correction.
pub fn a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world() {
    use dereth_physics::math::V3 as _;
    use dereth_physics::LandSource as _;
    use dereth_primitives::{CellId, LandblockId, LocalTime, ObjectId, Position, Vec3};

    const VICTIM: ObjectId = ObjectId(0x8000_1140);
    /// The block the local body starts in, and the only one the scene loads.
    const HOME: u16 = dereth_client::world::DEFAULT_LANDBLOCK;
    /// Its neighbour along the y axis, which the scene may or may not load.
    const NEXT: u16 = HOME + 1;
    /// Where the body stands: two metres short of the seam, on a column where the terrain runs
    /// continuously across it.
    const SEAM_X: f32 = 96.0;
    const SEAM_Y: f32 = 190.0;

    /// The land cell for a landblock-relative point, at the shipped 24 m cell pitch.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn land_cell(block: u16, x: f32, y: f32) -> CellId {
        let cx = (x / 24.0).floor() as u32 & 7;
        let cy = (y / 24.0).floor() as u32 & 7;
        CellId((u32::from(block) << 16) | (cx * 8 + cy + 1))
    }

    fn ground(c: &dereth_client::character::Character, block: u16, x: f32, y: f32) -> f32 {
        c.land()
            .ground_height(LandblockId(block), x, y)
            .unwrap_or_else(|| panic!("that block has no terrain under ({x}, {y})"))
    }

    /// Park the body at a landblock-relative point and let it settle.
    fn settle(
        c: &mut dereth_client::character::Character,
        block: u16,
        x: f32,
        y: f32,
        t: &mut f64,
    ) -> Position {
        let g = ground(c, block, x, y);
        let mut p = c.position();
        p.cell = land_cell(block, x, y);
        p.frame.origin = Vec3::new(x, y, g + 1.0);
        c.teleport(p);
        for _ in 0..90 {
            *t += DT;
            c.update(LocalTime(*t));
        }
        let q = c.position();
        assert!(c.on_ground(), "({x}, {y}) does not support a body: {q:?}");
        assert!(
            (q.frame.origin.x - x).abs() < 0.25 && (q.frame.origin.y - y).abs() < 0.25,
            "the body slid off ({x}, {y}) to {:?}",
            q.frame.origin
        );
        q
    }

    /// The scene. `prefetch_next` is the whole variable under test.
    fn scene(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        prefetch_next: bool,
    ) -> (
        dereth_client::character::Character,
        dereth_client::objects::ObjectStream,
        f64,
        Position,
    ) {
        let mut c = fresh_local_body(store);
        let mut t = 3.0;
        let start = settle(&mut c, HOME, SEAM_X, SEAM_Y, &mut t);
        let _ = settle(&mut c, HOME, SEAM_X, SEAM_Y - 30.0, &mut t);

        if prefetch_next {
            c.land().load_block_cells(LandblockId(NEXT));
        }
        assert_eq!(
            c.land().landblock_resident(LandblockId(NEXT)),
            prefetch_next,
            "the residency of the neighbouring block is the one thing this scene varies"
        );
        assert!(
            c.land().landblock_resident(LandblockId(HOME)),
            "the block the body is standing in must be loaded"
        );

        let mut stream = dereth_client::objects::ObjectStream::new();
        stream.apply_event(
            &create_event(VICTIM, Some(start), PLAYER, "Victim"),
            LocalTime(t),
        );
        stream.sync_physics(store, &mut c.world);
        assert!(
            body_position(&c, VICTIM)
                .expect("the create placed the body")
                .frame
                .origin
                .sub(start.frame.origin)
                .mag2()
                .sqrt()
                < 1e-4,
            "the create was moved aside, so the correction does not start where it says"
        );
        give_part_array(&mut c, VICTIM);
        for _ in 0..4 {
            t += DT;
            c.update(LocalTime(t));
        }
        let h = c.world.by_object_id(VICTIM).expect("the body exists");
        assert!(
            c.world.get(h).expect("the body exists").player_distance < 96.0,
            "this body has not been simulated, so the correction would take a different arm"
        );
        (c, stream, t, start)
    }

    /// A destination `dy` metres past the seam, named in the neighbouring block.
    fn across_the_seam(
        c: &dereth_client::character::Character,
        from: &Position,
        dy: f32,
    ) -> Position {
        let x = from.frame.origin.x;
        let mut p = *from;
        p.cell = land_cell(NEXT, x, dy);
        p.frame.origin = Vec3::new(x, dy, ground(c, NEXT, x, dy));
        p
    }

    let store = store();

    // (a) A short correction, block not loaded: the body leaves the world and stays out.
    let (mut c, mut stream, mut t, start) = scene(&store, false);
    let destination = across_the_seam(&c, &start, 1.0);
    let h = c.world.by_object_id(VICTIM).expect("the body exists");
    assert!(
        dereth_physics::math::distance(&start, &destination) < c.world.autonomy_blip_distance(h),
        "that is out of reach, so it is the one-step arm and not the glide"
    );
    stream.apply_event(
        &position_event(
            VICTIM,
            destination.cell.raw(),
            destination.frame.origin,
            1,
            0,
            true,
        ),
        LocalTime(t),
    );
    stream.sync_physics(&store, &mut c.world);
    let o = c.world.get(h).expect("the body exists");
    let short_lost = o.cell.is_none()
        && !o.transient_state.is_active()
        && o.position.frame.origin == destination.frame.origin
        && o.position.cell.landblock() == LandblockId(NEXT)
        && !c.world.is_interpolating(h)
        && stream.physics.is_lost(VICTIM);
    for _ in 0..10 {
        t += DT;
        c.update(LocalTime(t));
    }
    let short_stays_lost = body_position(&c, VICTIM).is_none();
    println!("block: short, not loaded: lost {short_lost}, stays {short_stays_lost}");

    // (b) The same correction with the block loaded: it glides across the seam.
    let (mut c, mut stream, mut t, start) = scene(&store, true);
    let destination = across_the_seam(&c, &start, 1.0);
    let reach = dereth_physics::math::distance(&start, &destination);
    let h = c.world.by_object_id(VICTIM).expect("the body exists");
    stream.apply_event(
        &position_event(
            VICTIM,
            destination.cell.raw(),
            destination.frame.origin,
            1,
            0,
            true,
        ),
        LocalTime(t),
    );
    stream.sync_physics(&store, &mut c.world);
    let queued = body_position(&c, VICTIM).expect("the body is still in a cell");
    let short_glides_premise = !stream.physics.is_lost(VICTIM)
        && c.world.is_interpolating(h)
        && queued.frame.origin.sub(start.frame.origin).mag2().sqrt() < 1e-4
        && queued.cell.landblock() == LandblockId(HOME);
    t += DT;
    c.update(LocalTime(t));
    let one = body_position(&c, VICTIM).expect("still in a cell");
    let one_step = xy_gap(&queued, &one);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (reach / STEP).ceil() as usize + 3;
    for _ in 0..steps {
        t += DT;
        c.update(LocalTime(t));
    }
    let here = body_position(&c, VICTIM).expect("the glide must not lose the body");
    let gap = xy_gap(&here, &destination);
    println!(
        "block: short, loaded: one step {one_step:.4} m of {reach:.3} m, ended {gap:.4} m out"
    );
    let short_glides = short_glides_premise
        && one_step < reach * 0.9
        && one_step > STEP * 0.5
        && here.cell.landblock() == LandblockId(NEXT)
        && gap < 0.5
        && !stream.physics.is_lost(VICTIM);

    // (c) A correction out of reach into the unloaded block: doomed, not deferred.
    let (mut c, mut stream, mut t, start) = scene(&store, false);
    let destination = across_the_seam(&c, &start, 120.0);
    let h = c.world.by_object_id(VICTIM).expect("the body exists");
    assert!(
        dereth_physics::math::distance(&start, &destination) > c.world.autonomy_blip_distance(h),
        "that is within reach, so it is the glide arm and not the one-step arm"
    );
    stream.apply_event(
        &position_event(
            VICTIM,
            destination.cell.raw(),
            destination.frame.origin,
            1,
            0,
            true,
        ),
        LocalTime(t),
    );
    stream.sync_physics(&store, &mut c.world);
    let o = c.world.get(h).expect("the body exists");
    let far_lost = o.cell.is_none()
        && !o.transient_state.is_active()
        && stream.physics.is_lost(VICTIM)
        && !c.world.is_interpolating(h);
    for _ in 0..20 {
        t += DT;
        c.update(LocalTime(t));
    }
    let far_stays_lost = body_position(&c, VICTIM).is_none() && stream.physics.is_lost(VICTIM);
    println!("block: out of reach, not loaded: lost {far_lost}, stays {far_stays_lost}");

    // (d) The same one out of reach with the block loaded: it lands, at once.
    let (mut c, mut stream, t, start) = scene(&store, true);
    let destination = across_the_seam(&c, &start, 120.0);
    let h = c.world.by_object_id(VICTIM).expect("the body exists");
    stream.apply_event(
        &position_event(
            VICTIM,
            destination.cell.raw(),
            destination.frame.origin,
            1,
            0,
            true,
        ),
        LocalTime(t),
    );
    stream.sync_physics(&store, &mut c.world);
    let here = body_position(&c, VICTIM).expect("the step must leave the body in a cell");
    let far_lands = !stream.physics.is_lost(VICTIM)
        && here.cell.landblock() == LandblockId(NEXT)
        && xy_gap(&here, &destination) < 0.1
        && !c.world.is_interpolating(h)
        && xy_gap(&here, &start) > STEP * 2.0;
    println!("block: out of reach, loaded: landed {far_lands}");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.correction.a-destination-in-a-block-the-client-has-not-loaded-takes-the-body-out-of-the-world",
        move |_| {
            short_lost && short_stays_lost && short_glides && far_lost && far_stays_lost
                && far_lands
        },
    );
}

#[test]
fn scenario_a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world() {
    scenario("a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world");
}

// -------------------------------------------------------------------------------------------
// movement.walk.reports-walking-forward-with-no-hold-key
// movement.run.the-report-a-running-body-sends-is-the-one-the-retail-client-sent
// movement.run.turning-run-on-while-already-walking-speeds-the-body-up-and-changes-the-report
// -------------------------------------------------------------------------------------------

/// The walk and run reports a live body sends, driven over real terrain; no corpus count is pinned.
mod run_forward {
    use dereth_client::character::{Character, CharacterInput};
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_primitives::LocalTime;
    use dereth_protocol::actions::unpack_action;
    use dereth_protocol::movement::{MovementMoveToState, RawMotionState};
    use dereth_protocol::Message;

    /// The command a walk forward reports, as a literal: reading it back through the client's own
    /// constant would not notice a wrong constant.
    pub const WALK_FORWARD: u32 = 0x4500_0005;
    /// The run hold key. Invalid is 0, None is 1.
    pub const HOLD_KEY_RUN: u32 = 2;
    /// The opcode a movement report travels under, inside a game action.
    pub const MOVE_TO_STATE: u32 = 0xF61C;
    const GAME_ACTION: u32 = 0xF7B1;

    /// How many frames one [`hold`] consumes.
    pub const HOLD: u32 = 60;

    /// A body standing still on the default landblock's terrain, settled for two seconds.
    pub fn settled_character(store: &std::sync::Arc<dereth_dat::RetailDatStore>) -> Character {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let mut c = Character::new(
            store,
            &region,
            dereth_client::world::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("the character is created");
        for i in 1..=60 {
            c.update(LocalTime(f64::from(i) / 30.0));
        }
        assert!(
            c.on_ground(),
            "the body must settle before anything is measured"
        );
        c
    }

    /// Hold `input` for two seconds of frames from `from`, and answer the distance covered in the
    /// **second** of them -- the settled ground speed. The first second is discarded on purpose:
    /// the animation and the physics ramp the velocity, so a window that starts at a key edge
    /// measures the transition and not the speed.
    pub fn hold(c: &mut Character, input: CharacterInput, from: u32) -> f32 {
        c.input = input;
        for i in from..from + 30 {
            c.update(LocalTime(f64::from(i + 1) / 30.0));
        }
        let before = c.position().frame.origin;
        for i in from + 30..from + 60 {
            c.update(LocalTime(f64::from(i + 1) / 30.0));
        }
        let after = c.position().frame.origin;
        ((after.x - before.x).powi(2) + (after.y - before.y).powi(2)).sqrt()
    }

    /// Every recorded movement report the whole corpus carries, with the blob it came from.
    ///
    /// The corpus is enumerated through its own index, so promoting a recording changes what this
    /// reads and pins no count anywhere.
    pub fn recorded_move_to_states() -> Vec<(String, Vec<u8>, MovementMoveToState)> {
        let mut out = Vec::new();
        for corpus in Corpus::load_all() {
            let mut blobs: Vec<_> = corpus
                .blobs
                .iter()
                .filter(|b| b.dir == Direction::ClientToServer && b.opcode == GAME_ACTION)
                .collect();
            blobs.sort_by_key(|b| b.blob_id);
            for b in blobs {
                let mut action =
                    unpack_action(&b.payload).expect("a recorded game action must unpack");
                if action.sub_type.0 != MOVE_TO_STATE {
                    continue;
                }
                let m =
                    MovementMoveToState::read(&mut action.body).expect("a recorded body decodes");
                action
                    .body
                    .expect_exhausted()
                    .expect("a recorded body is fully consumed");
                out.push((corpus.name.clone(), b.payload.clone(), m));
            }
        }
        out
    }

    /// What the client would put on the wire for the body as it stands.
    pub fn wire(c: &Character) -> RawMotionState {
        dereth_client::app::raw_motion_state_to_wire(&c.driver().movement.interp.raw_state)
    }
}

/// Walking forward with running off reports a walk and no modifier, and releasing reports nothing.
pub fn walking_forward_reports_a_walk_with_no_hold_key() {
    use dereth_client::character::CharacterInput;
    use run_forward::{hold, settled_character, wire, HOLD, WALK_FORWARD};

    let store = store();
    let mut c = settled_character(&store);

    let moved = hold(
        &mut c,
        CharacterInput {
            forward: true,
            ..CharacterInput::default()
        },
        60,
    );
    let s = wire(&c);
    let walking = moved > 0.5
        && s.forward_command == Some(WALK_FORWARD)
        && s.current_holdkey.is_none()
        && s.forward_holdkey.is_none()
        && s.forward_speed.is_none()
        && s.flags().expect("the state re-encodes") == 0x004;

    // Releasing returns to the default, so the edge exists in both directions.
    let coasted = hold(&mut c, CharacterInput::default(), 60 + HOLD);
    let released = wire(&c).flags().expect("the state re-encodes") == 0;
    println!("run: walk {moved:.3} m/s, coast {coasted:.3} m, released {released}");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.walk.reports-walking-forward-with-no-hold-key",
        move |_| walking && released,
    );
}

#[test]
fn scenario_walking_forward_reports_a_walk_with_no_hold_key() {
    scenario("walking_forward_reports_a_walk_with_no_hold_key");
}

/// A live running body's report is byte for byte a recorded one, and it really goes out.
pub fn a_running_bodys_report_is_the_retail_clients_own() {
    use dereth_client::character::CharacterInput;
    use dereth_client_net::client_session::testing::MockTransport;
    use dereth_client_net::client_session::{
        ContactPlane, PlayerMotion, PositionReporter, Session,
    };
    use dereth_primitives::LocalTime;
    use dereth_protocol::actions::unpack_action;
    use dereth_protocol::movement::{MoveTimestamps, MovementMoveToState};
    use dereth_protocol::Message;

    use run_forward::{
        hold, recorded_move_to_states, settled_character, wire, HOLD_KEY_RUN, MOVE_TO_STATE,
        WALK_FORWARD,
    };

    let store = store();
    let mut runner = settled_character(&store);
    hold(
        &mut runner,
        CharacterInput {
            forward: true,
            run: true,
            ..CharacterInput::default()
        },
        60,
    );
    let live = wire(&runner);

    // Take every recorded report whose whole raw state equals this live one, keep the recording's
    // position, timestamps, contact and jump bits, and push the pair through the real producer.
    // The bytes must come back identical. **No count is pinned**: the corpus's own index says how
    // many recordings there are, and counting them is the corpus census's job.
    let recorded = recorded_move_to_states();
    let mut matched = 0usize;
    let mut sessions: Vec<String> = Vec::new();
    let mut identical = true;
    for (name, payload, m) in &recorded {
        if m.0.raw_motion_state != live {
            continue;
        }
        matched += 1;
        if !sessions.contains(name) {
            sessions.push(name.clone());
        }
        let mut rep = PositionReporter::new(0.0);
        rep.active = true;
        let mut session = Session::new(MockTransport::new());
        let motion = PlayerMotion {
            position: m.0.position,
            position_valid: true,
            timestamps: m.0.timestamps,
            contact: m.0.contact,
            longjump_mode: m.0.longjump_mode,
            raw_motion_state: live.clone(),
            contact_plane: ContactPlane::default(),
        };
        rep.send_movement_event(0.0, &motion, &mut session)
            .expect("the producer must encode a running body");
        identical &= session.transport.sent[0].payload[8..] == payload[8..];
    }
    println!(
        "running: {matched} recorded running bodies re-encoded from the live state, {sessions:?}"
    );
    let re_encodes = matched > 0 && sessions.len() >= 2 && identical;

    // And the report really leaves the client when the body starts to run.
    let mut c = settled_character(&store);
    let mut rep = PositionReporter::new(0.0);
    rep.active = true;
    let mut session = Session::new(MockTransport::new());
    let mut frame = 60u32;
    while frame < 90 {
        frame += 1;
        let now = f64::from(frame) / 30.0;
        c.update(LocalTime(now));
        rep.use_time(
            now,
            &dereth_client::app::body_motion(&c, MoveTimestamps::default()),
            &mut session,
        );
    }
    let standing = rep.stats.movement_events;
    c.input = CharacterInput {
        forward: true,
        run: true,
        ..CharacterInput::default()
    };
    while frame < 150 {
        frame += 1;
        let now = f64::from(frame) / 30.0;
        c.update(LocalTime(now));
        rep.use_time(
            now,
            &dereth_client::app::body_motion(&c, MoveTimestamps::default()),
            &mut session,
        );
    }
    let one_edge = standing == 1 && rep.stats.movement_events == standing + 1;
    let blob = session
        .transport
        .sent
        .iter()
        .filter(|b| unpack_action(&b.payload).expect("a game action").sub_type.0 == MOVE_TO_STATE)
        .nth(1)
        .expect("the second report is the run")
        .payload
        .clone();
    let mut action = unpack_action(&blob).expect("it unpacks");
    let m = MovementMoveToState::read(&mut action.body).expect("it decodes");
    let emitted = one_edge
        && m.0.raw_motion_state.forward_command == Some(WALK_FORWARD)
        && m.0.raw_motion_state.current_holdkey == Some(HOLD_KEY_RUN)
        && m.0.contact;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.run.the-report-a-running-body-sends-is-the-one-the-retail-client-sent",
        move |_| re_encodes && emitted,
    );
}

#[test]
fn scenario_a_running_bodys_report_is_the_retail_clients_own() {
    scenario("a_running_bodys_report_is_the_retail_clients_own");
}

/// Turning running on part way through a walk speeds the body up and changes the report.
pub fn turning_run_on_mid_walk_speeds_the_body_and_changes_the_report() {
    use dereth_client::character::CharacterInput;
    use run_forward::{hold, settled_character, wire, HOLD, HOLD_KEY_RUN, WALK_FORWARD};

    let store = store();
    let mut c = settled_character(&store);

    let walked = hold(
        &mut c,
        CharacterInput {
            forward: true,
            ..CharacterInput::default()
        },
        60,
    );
    let was_walking = wire(&c).flags().expect("the state re-encodes") == 0x004;

    // Running goes on. The forward key never moved.
    let ran = hold(
        &mut c,
        CharacterInput {
            forward: true,
            run: true,
            ..CharacterInput::default()
        },
        60 + HOLD,
    );
    let running = wire(&c);
    let sped_up = ran > walked * 1.5
        && running.forward_command == Some(WALK_FORWARD)
        && running.current_holdkey == Some(HOLD_KEY_RUN)
        && running.flags().expect("the state re-encodes") == 0x005;

    // And back down again, which a rising-edge-only harness cannot see.
    let slowed = hold(
        &mut c,
        CharacterInput {
            forward: true,
            ..CharacterInput::default()
        },
        60 + 2 * HOLD,
    );
    let slowed_down = slowed < ran * 0.8
        && wire(&c).current_holdkey.is_none()
        && wire(&c).flags().expect("the state re-encodes") == 0x004;
    println!("run: walk {walked:.3} -> run {ran:.3} -> walk {slowed:.3} m/s");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.run.turning-run-on-while-already-walking-speeds-the-body-up-and-changes-the-report",
        move |_| was_walking && sped_up && slowed_down,
    );
}

#[test]
fn scenario_turning_run_on_mid_walk_speeds_the_body_and_changes_the_report() {
    scenario("turning_run_on_mid_walk_speeds_the_body_and_changes_the_report");
}

// -------------------------------------------------------------------------------------------
// camera.view.slides-through-a-creature-standing-between-you-and-the-camera
// -------------------------------------------------------------------------------------------

/// The camera passes through a creature and is pulled in by the same body that is not one.
///
/// The candidate is a body the recorded corpus carries, re-created here with only its description
/// type changed -- so the creature and the solid control are geometrically identical and the type
/// is the only variable.
pub fn the_camera_slides_through_a_creature_but_not_through_a_solid_twin() {
    use dereth_client::camera::CameraInput;
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{LocalTime, ObjectId, Position, Quat, Vec3};
    use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
    use dereth_protocol::types::physicsdesc::flags;
    use dereth_protocol::types::{PhysicsDesc, PositionWire};
    use dereth_protocol::{Message, Opcode};

    const CANDIDATE: ObjectId = ObjectId(0x8000_09E1);
    /// The recorded creature this scenario borrows a body from.
    const RECORDED: u32 = 0x8000_09D2;
    /// Anything without the creature bit; this one is the shipped "misc" type.
    const NOT_A_CREATURE: u32 = 0x0000_0080;

    /// The body this scenario measures with: no settle and no restrictions -- the camera path is
    /// measured from a body that has just been created, and settling it would move the path under
    /// the obstacle.
    fn plain_body(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
    ) -> dereth_client::character::Character {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        dereth_client::character::Character::new(
            store,
            &region,
            dereth_client::world::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("the ordinary local body is created")
    }

    /// The recorded create for the remote creature, as a payload.
    fn recorded_creature() -> ObjectCreatePayload {
        let corpus = Corpus::load("long-solo-play")
            .expect("the locked corpus decodes")
            .expect("the corpus carries that recording");
        let row = corpus
            .blobs
            .iter()
            .find(|r| {
                r.dir == Direction::ServerToClient
                    && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                    && r.payload.get(4..8) == Some(&RECORDED.to_le_bytes())
            })
            .expect("that recording carries the remote creature's create");
        ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
            .expect("the create decodes")
            .0
    }

    /// The free camera path a body with nothing around it has: from the pivot to the sought eye.
    fn free_camera_path(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
    ) -> (Position, Position) {
        let mut character = plain_body(store);
        character.update_camera(CameraInput::default(), LocalTime(10.0), 1.0 / 30.0);
        let pivot = dereth_client::camera::pivot_state(&character.world, character.handle)
            .expect("the player has a body");
        let from = character.camera.manager.query_pivot_position(&pivot);
        let to = character.camera.sought;
        assert_eq!(
            from.cell, to.cell,
            "the focused camera path must stay in one outdoor cell"
        );
        assert!(
            to.frame.origin.sub(from.frame.origin).mag2().sqrt() > 1.0,
            "the default camera path is too short to hold a discriminating obstacle"
        );
        assert_eq!(
            character.camera.stats.sweeps_blocked, 0,
            "the open-air control was blocked"
        );
        (from, to)
    }

    /// Build the candidate's create, with `obj_type` the only thing that varies.
    fn create_blob(at: Position, obj_type: u32) -> Vec<u8> {
        let recorded = recorded_creature();
        let setup_id = recorded
            .physicsdesc
            .setup_id
            .expect("the recording names a body setup");
        let mut candidate = recorded;
        candidate.id = CANDIDATE;
        candidate.wdesc.obj_type = obj_type;
        candidate.physicsdesc = PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP,
            setup_id: Some(setup_id),
            position: Some(PositionWire {
                objcell_id: at.cell.raw(),
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: at.frame.rotation.into(),
                },
            }),
            state: dereth_physics::PhysicsState::REPORT_COLLISIONS_PS,
            ..Default::default()
        };
        let encoded =
            dereth_protocol::write_body(&ItemCreateObject(candidate)).expect("the create encodes");
        let decoded = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&encoded))
            .expect("the encoded create round trips")
            .0;
        assert_eq!(
            decoded.wdesc.obj_type, obj_type,
            "the description type was not encoded"
        );
        encoded
    }

    /// The top of the candidate's own collision geometry, read back off a placed body rather than
    /// decoded here: the point of the scenario is the body the client builds, not a second copy.
    fn upper_sphere_center(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        near: Position,
    ) -> Vec3 {
        let mut character = plain_body(store);
        character.land().load_block_cells(near.cell.landblock());
        let mut at = near;
        at.frame.origin.x += 10.0;
        at.frame.rotation = Quat::IDENTITY;
        let mut stream = dereth_client::objects::ObjectStream::new();
        stream.apply_event(
            &SessionEvent::WorldObject {
                opcode: ItemCreateObject::OPCODE,
                body: create_blob(at, CREATURE),
            },
            LocalTime(9.0),
        );
        stream.sync_physics(store, &mut character.world);
        let h = character
            .world
            .by_object_id(CANDIDATE)
            .expect("the candidate has a body");
        let o = character.world.get(h).expect("the candidate's body");
        o.geometry
            .spheres
            .iter()
            .max_by(|a, b| a.center.z.total_cmp(&b.center.z))
            .expect("the candidate's setup has a collision sphere")
            .center
    }

    /// `(sweeps blocked, how far short of the sought eye the camera stopped, it ended in a cell)`.
    fn camera_with_candidate(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        upper: Vec3,
        obj_type: u32,
    ) -> (u64, f32, bool) {
        let (from, to) = free_camera_path(store);
        // Put the upper body sphere exactly at the midpoint of the real pivot-to-eye path.
        let midpoint = from.frame.origin.add(to.frame.origin).mul(0.5);
        let mut at = from;
        at.frame.origin = midpoint.sub(upper);
        at.frame.rotation = Quat::IDENTITY;

        let mut stream = dereth_client::objects::ObjectStream::new();
        stream.apply_event(
            &SessionEvent::WorldObject {
                opcode: ItemCreateObject::OPCODE,
                body: create_blob(at, obj_type),
            },
            LocalTime(9.0),
        );
        assert_eq!(
            stream
                .world
                .weenie(CANDIDATE)
                .expect("the create made a weenie")
                .is_creature(),
            obj_type == CREATURE,
            "the accepted description type did not reach the game model"
        );

        let mut character = plain_body(store);
        character.land().load_block_cells(from.cell.landblock());
        stream.sync_physics(store, &mut character.world);
        let h = character
            .world
            .by_object_id(CANDIDATE)
            .expect("the candidate has a body");
        let body = character.world.get(h).expect("the candidate's body");
        assert!(
            body.cell.is_some(),
            "the candidate was not inserted into the cell graph"
        );
        assert!(
            !body.geometry.spheres.is_empty(),
            "the candidate has no collision geometry"
        );
        assert_eq!(
            body.weenie
                .as_ref()
                .expect("the candidate's weenie half")
                .is_creature,
            obj_type == CREATURE,
            "the candidate's description did not reach the collision body"
        );

        character.update_camera(CameraInput::default(), LocalTime(10.0), 1.0 / 30.0);
        let shortfall = character
            .camera
            .sought
            .frame
            .origin
            .sub(character.camera.viewer.frame.origin);
        (
            character.camera.stats.sweeps_blocked,
            shortfall.mag2().sqrt(),
            character.camera.viewer_cell.is_some(),
        )
    }

    let store = store();
    let (from, _) = free_camera_path(&store);
    let upper = upper_sphere_center(&store, from);
    let creature = camera_with_candidate(&store, upper, CREATURE);
    let solid = camera_with_candidate(&store, upper, NOT_A_CREATURE);
    println!("camera: creature {creature:?} vs the same body that is not one {solid:?}");

    let both_ended_somewhere = solid.2 && creature.2;
    let solid_obstructs = solid.0 > 0 && solid.1 > dereth_physics::globals::VIEWER_SPHERE_RADIUS;
    let creature_does_not =
        creature.0 == 0 && creature.1 <= dereth_physics::globals::VIEWER_SPHERE_RADIUS;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "camera.view.slides-through-a-creature-standing-between-you-and-the-camera",
        move |_| both_ended_somewhere && solid_obstructs && creature_does_not,
    );
}

#[test]
fn scenario_the_camera_slides_through_a_creature_but_not_through_a_solid_twin() {
    scenario("the_camera_slides_through_a_creature_but_not_through_a_solid_twin");
}

// -------------------------------------------------------------------------------------------
// world.click.a-click-answered-with-nothing-in-view-reports-nothing-picked
// -------------------------------------------------------------------------------------------

/// A click armed by a world drop and answered on a frame with no scene reports nothing picked.
///
/// The pair is the point: arming writes "nothing, and no part either", and the answer has to *move*
/// the second half from that to "nothing".
pub fn a_scene_less_click_reports_nothing_picked() {
    use dereth_client::interaction::{self, Interaction};
    use dereth_primitives::{LocalTime, ObjectId, ServerTime};
    use dereth_ui_screens::view::{DropTarget, UiRequest};

    const PLAYER_ID: ObjectId = ObjectId(0x5000_0449);

    struct Host {
        store: std::sync::Arc<dereth_dat::RetailDatStore>,
        objects: dereth_client::objects::ObjectStream,
        inter: Interaction,
        clock: f64,
    }

    impl Host {
        fn new(store: &std::sync::Arc<dereth_dat::RetailDatStore>) -> Self {
            let mut objects = dereth_client::objects::ObjectStream::new();
            objects.world.player = Some(PLAYER_ID);
            objects.world.tables.inventories.insert(
                PLAYER_ID,
                dereth_client_model::objects::ObjectInventory::new(PLAYER_ID),
            );
            let mut pw = dereth_client_model::Weenie::new(PLAYER_ID);
            pw.pwd.items_capacity = Some(0xFF);
            pw.pwd.containers_capacity = Some(0xFF);
            objects.world.tables.weenies.insert(PLAYER_ID, pw);
            let mut h = Self {
                store: std::sync::Arc::clone(store),
                objects,
                inter: Interaction::new(),
                clock: 1.0,
            };
            // Prime the viewport the pick reads.
            h.drive();
            h
        }

        /// One frame with **no scene at all**, which is the arm under test.
        fn drive(&mut self) {
            self.clock += 1.0;
            let _ = interaction::use_time(
                &mut self.inter,
                &self.store,
                None,
                &mut self.objects,
                None,
                Vec::new(),
                false,
                (1024, 768),
                LocalTime(self.clock),
            );
        }

        fn carry(&mut self, id: ObjectId) -> ObjectId {
            let mut w = dereth_client_model::Weenie::new(id);
            w.pwd.container_id = Some(PLAYER_ID);
            w.pwd.stack_size = Some(1);
            w.pwd.max_stack_size = Some(1);
            w.pwd.name = format!("thing {:X}", id.0);
            w.waiting = true;
            w.determine_position_state();
            self.objects.world.tables.weenies.insert(id, w);
            assert!(
                self.objects.world.is_owned_by_player(id),
                "the fixture must be carried"
            );
            id
        }

        /// Arm a pick the way the production path does: a world drop through the request pump.
        fn arm_a_drop(&mut self, item: ObjectId) {
            self.inter.queue(
                Vec::new(),
                vec![UiRequest::DragDrop {
                    item,
                    target: DropTarget::World,
                }],
            );
            self.clock += 1.0;
            let unowned =
                self.inter
                    .run_ui_requests(&mut self.objects.world, false, ServerTime(self.clock));
            assert!(
                unowned.is_empty(),
                "the drop is this scenario's own arm: {unowned:?}"
            );
            assert!(
                self.inter.pick.looking_for_object(),
                "the pick must be armed"
            );
        }
    }

    let store = store();
    let mut host = Host::new(&store);

    let mut starts_unanswered = host.inter.pick.click_object() == (ObjectId(0), -1);
    let mut moves_to_nothing = true;
    // Twice, so a single lucky frame cannot produce it.
    for round in 0..2 {
        let item = host.carry(ObjectId(0x4490_0001 + round));
        host.arm_a_drop(item);
        starts_unanswered &= host.inter.pick.click_object() == (ObjectId(0), -1);

        let notices = host.inter.stats.scene_less_notices;
        host.drive();
        moves_to_nothing &= host.inter.stats.scene_less_notices == notices + 1
            && host.inter.pick.click_object() == (ObjectId(0), 0)
            && !host.inter.pick.looking_for_object();
    }

    // The known-negative: with nothing armed, the frame answers nothing at all and the pair is
    // back at the value arming writes -- so the zero above is written by the *answer* and not by
    // the default.
    let mut idle = Host::new(&store);
    let notices = idle.inter.stats.scene_less_notices;
    idle.drive();
    let unarmed_is_never_answered = idle.inter.stats.scene_less_notices == notices
        && idle.inter.pick.click_object() == (ObjectId(0), -1)
        && !idle.inter.pick.looking_for_object();
    println!(
        "click armed answers move to nothing: {moves_to_nothing}; an unarmed frame answers \
         nothing at all: {unarmed_is_never_answered}"
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.click.a-click-answered-with-nothing-in-view-reports-nothing-picked",
        move |_| starts_unanswered && moves_to_nothing && unarmed_is_never_answered,
    );
}

#[test]
fn scenario_a_scene_less_click_reports_nothing_picked() {
    scenario("a_scene_less_click_reports_nothing_picked");
}

// -------------------------------------------------------------------------------------------
// The fixtures the body scenarios share
// -------------------------------------------------------------------------------------------

/// What a scenario in this subject needs before it can measure anything: the retail dats, a settled
/// local body over real terrain, and a create or a position update that has really been through the
/// wire.
///
/// It is shared so that the scenarios that need the same four helpers do not each build their own.
mod support {
    use std::sync::Arc;

    use dereth_client::character::{Character, ALUVIAN_MALE_SETUP};
    use dereth_client::world::{load_region, DEFAULT_LANDBLOCK};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_dat::RetailDatStore;
    use dereth_physics::pmanager::FALLBACK_SPEED;
    use dereth_primitives::{LocalTime, ObjectId, Position, Vec3};
    use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
    use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
    use dereth_protocol::types::physicsdesc::flags;
    use dereth_protocol::types::{PhysicsDesc, PositionWire, PublicWeenieDesc};
    use dereth_protocol::Message;

    /// The description type every body below carries. A player and a monster both have it.
    pub const CREATURE: u32 = 0x0000_0010;
    /// The description bit that says a body is a player.
    pub const PLAYER: u32 = 0x0000_0008;
    /// The bit that says a player is marked for player combat.
    pub const PK: u32 = 0x0000_0020;
    /// The lighter form of the same marking.
    pub const PK_LITE: u32 = 0x0200_0000;
    /// The bit that makes a player collide with everything regardless.
    pub const IMPENETRABLE: u32 = 0x0020_0000;

    /// The sub-step these scenarios drive physics at: over the client's own minimum so the gate
    /// opens every time, and under its maximum so each tick is exactly one sub-step.
    pub const DT: f64 = 1.0 / 20.0;
    /// How far a glide covers in one of those sub-steps for a body with no movement of its own.
    #[allow(clippy::cast_possible_truncation)]
    pub const STEP: f32 = FALLBACK_SPEED * DT as f32;

    pub fn store() -> Arc<RetailDatStore> {
        Arc::new(dereth_dat::testing::open_store_or_fail())
    }

    /// A settled local body that carries its own "this is a player" answer, exactly as the object
    /// stream gives it on the connected path.
    ///
    /// The settle is not a nicety: an unsettled body has no contact plane, and a measurement taken
    /// over one would be measuring the drop.
    pub fn fresh_local_body(store: &Arc<RetailDatStore>) -> Character {
        let region = load_region(store).expect("the region decodes");
        let mut c = Character::new(store, &region, DEFAULT_LANDBLOCK, (96.0, 96.0))
            .expect("the ordinary local body is created");
        let start = c.position();
        c.land().load_block_cells(start.cell.landblock());
        c.teleport(start);
        for i in 0..=60 {
            c.update(LocalTime(f64::from(i) / 30.0));
        }
        assert!(
            c.on_ground(),
            "the outdoor start must support the body: {:?}",
            c.position()
        );
        c.world.set_weenie_restrictions(
            c.handle,
            Some(dereth_physics::obj::WeenieRestrictions {
                is_player: true,
                is_creature: true,
                ..Default::default()
            }),
        );
        c
    }

    /// Put the local body at a point in its own landblock, let it settle, and answer where it came
    /// to rest. Remote bodies wear the same setup, so anything this returns is standable.
    pub fn settled_at(c: &mut Character, x: f32, y: f32, t: &mut f64) -> Position {
        let mut p = c.position();
        p.frame.origin.x = x;
        p.frame.origin.y = y;
        p.frame.origin.z += 2.0;
        c.teleport(p);
        for _ in 0..90 {
            *t += DT;
            c.update(LocalTime(*t));
        }
        assert!(
            c.on_ground(),
            "({x}, {y}) does not support a body: {:?}",
            c.position()
        );
        c.position()
    }

    /// One create for a body wearing the local body's own setup, encoded and decoded so the
    /// description bits really go over the wire.
    ///
    /// `at == None` is the player's own create, which carries a description and no position: the
    /// object stream excludes the player from placement because the local body already owns it.
    pub fn create_event(
        id: ObjectId,
        at: Option<Position>,
        bitfield: u32,
        name: &str,
    ) -> SessionEvent {
        let payload = ObjectCreatePayload {
            id,
            objdesc: Default::default(),
            physicsdesc: match at {
                Some(at) => PhysicsDesc {
                    bitfield: flags::POSITION | flags::SETUP,
                    setup_id: Some(ALUVIAN_MALE_SETUP.0),
                    position: Some(PositionWire {
                        objcell_id: at.cell.raw(),
                        frame: dereth_protocol::types::Frame {
                            origin: at.frame.origin.into(),
                            orientation: at.frame.rotation.into(),
                        },
                    }),
                    state: dereth_physics::PhysicsState::REPORT_COLLISIONS_PS,
                    ..Default::default()
                },
                None => PhysicsDesc::default(),
            },
            wdesc: PublicWeenieDesc {
                name: name.to_owned(),
                obj_type: CREATURE,
                bitfield,
                ..Default::default()
            },
        };
        let body =
            dereth_protocol::write_body(&ItemCreateObject(payload)).expect("the create encodes");
        let decoded = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&body))
            .expect("the encoded create round trips")
            .0;
        assert_eq!(
            decoded.wdesc.bitfield, bitfield,
            "the description bits did not encode"
        );
        SessionEvent::WorldObject {
            opcode: ItemCreateObject::OPCODE,
            body,
        }
    }

    /// One position update, encoded and decoded so the ground-contact flag really goes over the
    /// wire. A non-zero `teleport` stamp is what makes it a teleport rather than a correction.
    pub fn position_event(
        id: ObjectId,
        cell: u32,
        origin: Vec3,
        stamp: u16,
        teleport: u16,
        contact: bool,
    ) -> SessionEvent {
        let mut f = position_flags::ORIENTATION_HAS_NO_W
            | position_flags::ORIENTATION_HAS_NO_X
            | position_flags::ORIENTATION_HAS_NO_Y
            | position_flags::ORIENTATION_HAS_NO_Z;
        if contact {
            f |= position_flags::IS_GROUNDED;
        }
        let msg = MovementPositionEvent {
            id,
            position: PositionPack {
                flags: f,
                origin: dereth_protocol::types::Origin {
                    objcell_id: cell,
                    origin: dereth_protocol::types::Vec3 {
                        x: origin.x,
                        y: origin.y,
                        z: origin.z,
                    },
                },
                instance_timestamp: 0,
                position_timestamp: stamp,
                teleport_timestamp: teleport,
                ..PositionPack::default()
            },
        };
        let body = dereth_protocol::write_body(&msg).expect("the update encodes");
        let decoded = MovementPositionEvent::read(&mut dereth_protocol::Reader::new(&body))
            .expect("the encoded update round trips");
        assert_eq!(
            decoded.position.has_contact(),
            contact,
            "the contact flag did not encode"
        );
        SessionEvent::WorldObject {
            opcode: MovementPositionEvent::OPCODE,
            body,
        }
    }

    /// Where a remote body's **collision** body is standing, which is what these claims are about.
    pub fn body_origin(c: &Character, id: ObjectId) -> Option<Vec3> {
        body_position(c, id).map(|p| p.frame.origin)
    }

    /// The same, with the cell the body is in.
    pub fn body_position(c: &Character, id: ObjectId) -> Option<Position> {
        let h = c.world.by_object_id(id)?;
        let o = c.world.get(h)?;
        o.cell.map(|_| o.position)
    }

    /// Give a remote body a part array, which is what every drawn object in the world has.
    ///
    /// A body with no movement of its own and standing on walkable ground is put back to sleep at
    /// the end of every sub-step, so it runs a sub-step only every other tick and a glide over it
    /// advances at half rate. In a whole client this never arises: the scene hangs a motion driver
    /// on every animated remote body before the physics sweep. A scenario driving the object
    /// stream alone has to say so itself.
    pub fn give_part_array(c: &mut Character, id: ObjectId) {
        let h = c.world.by_object_id(id).expect("the body exists");
        c.world
            .get_mut(h)
            .expect("the body exists")
            .set_motion(Box::new(dereth_physics::NullMotion::with_geometry()));
    }

    /// How far apart two positions are horizontally, across a landblock seam.
    pub fn xy_gap(a: &Position, b: &Position) -> f32 {
        let d = dereth_physics::math::get_offset(a, b);
        (d.x * d.x + d.y * d.y).sqrt()
    }
}

// ===========================================================================================
// The world: reports, jumps, scripts, the camera, server control, doors, the map and the radar
// ===========================================================================================
//
// The claim is about a body, a producer or a script running over the retail data at a clock the
// scenario chooses, and the harness has no gesture for any of that -- so the scenario opens the
// dats itself through [`support::store`], drives the production structure directly, and books the
// claim through `assert_behaviour` on a model client.

// -------------------------------------------------------------------------------------------
// movement.position-report.*
// -------------------------------------------------------------------------------------------

/// What the client hands its position reporter is the body's own state, field for field.
pub fn the_reported_state_is_the_bodys_own() {
    use dereth_client::app::{body_motion, raw_motion_state_to_wire};
    use dereth_protocol::movement::MoveTimestamps;

    let store = support::store();
    let c = world_support::settled_body(&store);
    let m = body_motion(&c, MoveTimestamps::default());
    let pos = c.position();

    // Bit for bit, not approximately: nothing rounds on this path.
    let same_cell = m.position.objcell_id == pos.cell.0;
    let same_origin = m.position.frame.origin.x.to_bits() == pos.frame.origin.x.to_bits()
        && m.position.frame.origin.y.to_bits() == pos.frame.origin.y.to_bits()
        && m.position.frame.origin.z.to_bits() == pos.frame.origin.z.to_bits();
    let same_facing = m.position.frame.orientation.w.to_bits() == pos.frame.rotation.w.to_bits()
        && m.position.frame.orientation.z.to_bits() == pos.frame.rotation.z.to_bits();
    let valid = m.position_valid;
    let contact = m.contact && m.contact == c.on_ground();
    // The plane under a body resting on terrain points up. It is an input to the reporter's
    // third trigger, so it has to be a real value and not a default.
    let plane = m.contact_plane.normal.z > 0.5;
    // An idle body's raw state is the client's own default, which packs to no fields at all.
    let idle = raw_motion_state_to_wire(&c.driver().movement.interp.raw_state)
        .flags()
        .expect("the idle state encodes")
        == 0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.position-report.the-state-the-client-reports-is-the-bodys-own",
        move |_| same_cell && same_origin && same_facing && valid && contact && plane && idle,
    );
}

#[test]
fn scenario_the_reported_state_is_the_bodys_own() {
    scenario("the_reported_state_is_the_bodys_own");
}

/// Walking forward turns the reported motion state into the one the recordings carry, and
/// letting go turns it back.
pub fn walking_forward_reports_the_state_the_recordings_carry() {
    use dereth_client::app::raw_motion_state_to_wire;
    use dereth_client::character::CharacterInput;
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_primitives::LocalTime;

    /// The command a walk forward reports, as a literal: reading it back through the client's
    /// own constant would not notice a wrong constant.
    const WALK_FORWARD: u32 = 0x4500_0005;
    /// `forward_command` alone, and nothing else present.
    const FORWARD_ONLY: u32 = 0x004;

    let store = support::store();
    let mut c = world_support::settled_body(&store);

    c.input = CharacterInput {
        forward: true,
        ..CharacterInput::default()
    };
    for i in 61..=90 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    let s = raw_motion_state_to_wire(&c.driver().movement.interp.raw_state);
    let walking = s.flags().expect("the walking state encodes") == FORWARD_ONLY
        && s.forward_command == Some(WALK_FORWARD)
        // 1.0 and "no hold key" are the defaults and are not sent.
        && s.forward_speed.is_none()
        && s.current_holdkey.is_none()
        && s.actions.is_empty();

    c.input = CharacterInput::default();
    for i in 91..=120 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    let released = raw_motion_state_to_wire(&c.driver().movement.interp.raw_state)
        .flags()
        .expect("the idle state encodes")
        == 0;

    // And the walking word really is one the recorded clients sent -- counted off the corpus at
    // run time rather than written down here.
    let mut recorded = 0usize;
    for name in dereth_client_net::client_session::testing::session_names() {
        let Some(corpus) = Corpus::load(name).expect("the recording parses") else {
            continue;
        };
        for b in &corpus.blobs {
            if b.dir != Direction::ClientToServer {
                continue;
            }
            let Ok(mut a) = dereth_protocol::actions::unpack_action(&b.payload) else {
                continue;
            };
            if a.sub_type.0 != world_support::MOVE_TO_STATE {
                continue;
            }
            let Ok(m) =
                <dereth_protocol::movement::MovementMoveToState as dereth_protocol::Message>::read(
                    &mut a.body,
                )
            else {
                continue;
            };
            if m.0.raw_motion_state.forward_command == Some(WALK_FORWARD) {
                recorded += 1;
            }
        }
    }
    println!("report: {recorded} recorded move-to-state bodies carry the walk-forward word");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.position-report.walking-forward-reports-the-state-the-recordings-carry",
        move |_| walking && released && recorded > 0,
    );
}

#[test]
fn scenario_walking_forward_reports_the_state_the_recordings_carry() {
    scenario("walking_forward_reports_the_state_the_recordings_carry");
}

/// A walking body reports where it is; a still one reports once and then falls silent.
///
/// The pair is the claim: a producer that fired every frame would satisfy the first half alone.
pub fn a_walking_body_reports_where_it_is_and_a_still_one_falls_silent() {
    use dereth_client::app::body_motion;
    use dereth_client::character::CharacterInput;
    use dereth_primitives::LocalTime;
    use dereth_protocol::movement::{
        MoveTimestamps, MovementAutonomousPosition, MovementMoveToState,
    };

    let store = support::store();

    // --- the walking arm -------------------------------------------------------------------
    let mut c = world_support::settled_body(&store);
    let (mut rep, mut session) = world_support::reporter();
    let mut frame = 60u32;
    let mut last_body_origin = c.position().frame.origin;
    // One second standing still first, so that the walk's own state edge happens inside the run
    // and can be attributed rather than being the reporter's own first frame.
    while frame < 60 + 30 {
        frame += 1;
        let now = f64::from(frame) / 30.0;
        c.update(LocalTime(now));
        rep.use_time(
            now,
            &body_motion(&c, MoveTimestamps::default()),
            &mut session,
        );
    }
    let standing_movements = rep.stats.movement_events;

    c.input = CharacterInput {
        forward: true,
        ..CharacterInput::default()
    };
    let start = c.position().frame.origin;
    while frame < 60 + 30 + 6 * 30 {
        frame += 1;
        let now = f64::from(frame) / 30.0;
        c.update(LocalTime(now));
        let before = rep.stats.position_events;
        rep.use_time(
            now,
            &body_motion(&c, MoveTimestamps::default()),
            &mut session,
        );
        if rep.stats.position_events != before {
            last_body_origin = c.position().frame.origin;
        }
    }
    let moved = ((c.position().frame.origin.x - start.x).powi(2)
        + (c.position().frame.origin.y - start.y).powi(2))
    .sqrt();

    let blobs = world_support::emitted(&session);
    let positions: Vec<&Vec<u8>> = blobs
        .iter()
        .filter(|(s, _)| *s == world_support::AUTONOMOUS_POSITION)
        .map(|(_, p)| p)
        .collect();
    // Six seconds on a one-second schedule, plus the immediate reports the cell grid forces.
    let rate = (5..=12).contains(&positions.len());
    let every_one_carries_contact = positions.iter().all(|p| {
        let m: MovementAutonomousPosition = world_support::decode(p);
        m.0.contact == 1
    });
    let last: MovementAutonomousPosition =
        world_support::decode(positions.last().expect("the walk reported something"));
    let carries_the_body = last.0.position.objcell_id == c.position().cell.0
        && last.0.position.frame.origin.x.to_bits() == last_body_origin.x.to_bits()
        && last.0.position.frame.origin.y.to_bits() == last_body_origin.y.to_bits();
    let movements = blobs
        .iter()
        .filter(|(s, _)| *s == world_support::MOVE_TO_STATE)
        .count();
    let one_edge = movements as u64 == standing_movements + 1;
    let the_edge_is_the_walk = {
        let m: MovementMoveToState = world_support::decode(
            blobs
                .iter()
                .filter(|(s, _)| *s == world_support::MOVE_TO_STATE)
                .nth(1)
                .map(|(_, p)| p)
                .expect("the walk is the second state edge"),
        );
        m.0.raw_motion_state.forward_command == Some(0x4500_0005) && m.0.contact
    };

    // --- the still arm ---------------------------------------------------------------------
    let mut still = world_support::settled_body(&store);
    let (mut rep2, mut session2) = world_support::reporter();
    let mut frame = 60u32;
    while frame < 60 + 6 * 30 {
        frame += 1;
        let now = f64::from(frame) / 30.0;
        still.update(LocalTime(now));
        rep2.use_time(
            now,
            &body_motion(&still, MoveTimestamps::default()),
            &mut session2,
        );
    }
    let idle = world_support::emitted(&session2);
    // 180 frames of standing still, two blobs: the reporter's own first cell change and the
    // first state edge. Say the denominator out loud.
    let silent = idle
        .iter()
        .filter(|(s, _)| *s == world_support::AUTONOMOUS_POSITION)
        .count()
        == 1
        && idle
            .iter()
            .filter(|(s, _)| *s == world_support::MOVE_TO_STATE)
            .count()
            == 1
        && idle.len() == 2;

    println!(
        "report: {moved:.2} m walked, {} position reports, {movements} state edges; a still \
         body sent {} blobs over 180 frames",
        positions.len(),
        idle.len()
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.position-report.a-walking-body-reports-where-it-is-and-a-still-one-falls-silent",
        move |_| {
            moved > 3.0
                && rate
                && every_one_carries_contact
                && carries_the_body
                && one_edge
                && the_edge_is_the_walk
                && silent
        },
    );
}

#[test]
fn scenario_a_walking_body_reports_where_it_is_and_a_still_one_falls_silent() {
    scenario("a_walking_body_reports_where_it_is_and_a_still_one_falls_silent");
}

/// Every drawn frame reaches the position producer, and a client with nobody to report about
/// reports nothing.
///
/// A scenario may not skip for want of a graphics device, so this one is a whole headless client
/// through [`ClientSpec::retail`] -- the same client, without the device.
pub fn every_drawn_frame_reaches_the_position_reporter() {
    let mut c = HeadlessClient::new(ClientSpec::retail());
    let before = c.view().expect_app().position_use_times();
    c.tick(3);

    c.assert_behaviour(
        "movement.position-report.every-drawn-frame-reaches-the-reporter-and-invents-nothing",
        move |v| {
            let app = v.expect_app();
            let s = app.position_reporter_stats();
            before == 0
                && app.position_use_times() == 3
                && app.position_use_times() == app.frames_drawn()
                // With no link and no body there is nothing to report, and the reporter must
                // not invent one.
                && (s.position_events, s.movement_events, s.encode_failures) == (0, 0, 0)
        },
    );
    c.shutdown();
}

#[test]
fn scenario_every_drawn_frame_reaches_the_position_reporter() {
    scenario("every_drawn_frame_reaches_the_position_reporter");
}

// -------------------------------------------------------------------------------------------
// movement.jump.*
// -------------------------------------------------------------------------------------------

/// A jump puts the body's own position and speed on the wire.
pub fn a_jump_reports_the_bodys_own_position_and_speed() {
    use dereth_client::app::body_motion;
    use dereth_client::character::{CharacterInput, FULL_JUMP_EXTENT};
    use dereth_protocol::movement::{MoveTimestamps, MovementJump};

    let store = support::store();
    let mut c = world_support::settled_body(&store);
    let (mut rep, mut session) = world_support::reporter();
    let stamps = MoveTimestamps {
        instance: 7,
        server_control: 3,
        teleport: 2,
        force_position: 0,
    };

    let before = c.position();
    let (accepted, velocity) = world_support::jump_frame(
        &mut c,
        61.0 / 30.0,
        CharacterInput {
            jump: true,
            ..CharacterInput::default()
        },
    );
    // The speed is read after the impulse, so it must carry the jump; a standing jump has no
    // horizontal component, like the recorded standing one.
    let impulse = velocity.z > 0.5 && velocity.x.abs() < 0.01 && velocity.y.abs() < 0.01;

    let motion = body_motion(&c, stamps);
    rep.send_jump(accepted, FULL_JUMP_EXTENT, velocity, &motion, &mut session)
        .expect("the jump encodes");
    let one_blob = rep.stats.jump_events == 1 && session.transport.sent.len() == 1;

    let payload = session.transport.sent[0].payload.clone();
    let a = dereth_protocol::actions::unpack_action(&payload).expect("a game action");
    let m: MovementJump = world_support::decode(&payload);
    let pos = c.position();
    let pack = a.sub_type.0 == world_support::JUMP
        && m.0.extent == FULL_JUMP_EXTENT
        && m.0.velocity == velocity
        && m.0.position.objcell_id == pos.cell.0
        && m.0.position.frame.origin.x == pos.frame.origin.x
        && m.0.position.frame.origin.y == pos.frame.origin.y
        && m.0.position.frame.origin.z == pos.frame.origin.z
        && m.0.timestamps == stamps
        // The whole blob is the header plus one pack, as both recorded ones are.
        && payload.len() == 12 + 56;
    let left_the_ground = !c.on_ground() && pos.frame.origin.z >= before.frame.origin.z;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.jump.a-jump-puts-the-bodys-own-position-and-speed-on-the-wire",
        move |_| accepted && impulse && one_blob && pack && left_the_ground,
    );
}

#[test]
fn scenario_a_jump_reports_the_bodys_own_position_and_speed() {
    scenario("a_jump_reports_the_bodys_own_position_and_speed");
}

/// A second jump while the body is still in the air is refused, and nothing goes out for it.
pub fn a_second_jump_in_mid_air_sends_nothing() {
    use dereth_client::app::body_motion;
    use dereth_client::character::{CharacterInput, FULL_JUMP_EXTENT};
    use dereth_protocol::movement::MoveTimestamps;

    let store = support::store();
    let mut c = world_support::settled_body(&store);
    let (mut rep, mut session) = world_support::reporter();
    let stamps = MoveTimestamps::default();
    let jump = CharacterInput {
        jump: true,
        ..CharacterInput::default()
    };

    let (accepted, velocity) = world_support::jump_frame(&mut c, 61.0 / 30.0, jump);
    rep.send_jump(
        accepted,
        FULL_JUMP_EXTENT,
        velocity,
        &body_motion(&c, stamps),
        &mut session,
    )
    .expect("the first jump encodes");
    let first = accepted && rep.stats.jump_events == 1;
    // The subject is alive enough to fail: it really is off the ground.
    let airborne = !c.on_ground();

    let (accepted2, velocity2) = world_support::jump_frame(&mut c, 62.0 / 30.0, jump);
    let nothing = rep
        .send_jump(
            accepted2,
            FULL_JUMP_EXTENT,
            velocity2,
            &body_motion(&c, stamps),
            &mut session,
        )
        .is_none();
    let still_one = rep.stats.jump_events == 1 && session.transport.sent.len() == 1;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.jump.a-second-jump-in-mid-air-is-refused-and-nothing-goes-out",
        move |_| first && airborne && !accepted2 && nothing && still_one,
    );
}

#[test]
fn scenario_a_second_jump_in_mid_air_sends_nothing() {
    scenario("a_second_jump_in_mid_air_sends_nothing");
}

/// A running jump carries its speed in the body's own frame, not the world's.
///
/// The body is turned first, which is the whole point: facing north the two frames coincide and
/// the measurement cannot tell them apart.
pub fn a_running_jump_carries_its_speed_in_the_bodys_own_frame() {
    use dereth_client::character::CharacterInput;
    use dereth_primitives::LocalTime;

    let store = support::store();
    let mut c = world_support::settled_body(&store);

    let turn = CharacterInput {
        turn_left: true,
        ..CharacterInput::default()
    };
    for i in 61..=100 {
        c.input = turn;
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    let heading = c.position().frame.rotation;
    let turned = heading.z.abs() > 0.05;

    let run = CharacterInput {
        forward: true,
        run: true,
        ..CharacterInput::default()
    };
    for i in 101..=190 {
        c.input = run;
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    let on_the_ground = c.on_ground();

    let (accepted, velocity) = world_support::jump_frame(
        &mut c,
        191.0 / 30.0,
        CharacterInput {
            forward: true,
            run: true,
            jump: true,
            ..CharacterInput::default()
        },
    );
    println!(
        "jump: a running jump at heading z={:.3} reports ({:.3}, {:.3}, {:.3})",
        heading.z, velocity.x, velocity.y, velocity.z
    );
    let forward = velocity.y > 1.0;
    let hardly_sideways = velocity.x.abs() < velocity.y.abs() * 0.25;
    let still_a_jump = velocity.z > 0.5;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.jump.a-running-jump-carries-its-speed-in-the-bodys-own-frame",
        move |_| turned && on_the_ground && accepted && forward && hardly_sideways && still_a_jump,
    );
}

#[test]
fn scenario_a_running_jump_carries_its_speed_in_the_bodys_own_frame() {
    scenario("a_running_jump_carries_its_speed_in_the_bodys_own_frame");
}

/// The height the client works out for a jump explains every jump the recordings carry.
///
/// The extents and the speeds are **read out of the recordings at run time**, and no one jump skill
/// can be named for them: the eighteen recordings are not all one character, and the jump skill is
/// an input to the height. So the measurement is the one that does not depend on knowing it -- for
/// every recorded jump there is a jump skill that reproduces its speed from its extent exactly, bit
/// for bit, through the client's own height.
///
/// Both halves of the population are asserted, because they are different measurements: a jump
/// on the shortest height the client allows is explained by a whole range of skills, and one
/// above that floor is explained by exactly one. Without the second the claim would be about the
/// floor and not about the formula.
pub fn the_clients_jump_height_explains_the_recorded_jumps() {
    use dereth_animation::motion::get_jump_height;

    /// The client's own shortest jump.
    const FLOOR: f32 = 0.35;
    /// The range of jump skills searched. A shipped character's is inside it by a wide margin.
    const SKILLS: std::ops::RangeInclusive<i32> = 1..=1000;

    let recorded = world_support::recorded_jumps();
    let fits = |extent: f32, speed: f32| -> Vec<i32> {
        SKILLS
            .filter(|s| {
                (get_jump_height(0.0, *s, extent, 1.0) * 19.6)
                    .sqrt()
                    .to_bits()
                    == speed.to_bits()
            })
            .collect()
    };

    let mut explained = 0usize;
    let mut floored = 0usize;
    let mut pinned = 0usize;
    let mut unexplained = 0usize;
    for (extent, speed) in &recorded {
        let skills = fits(*extent, *speed);
        if skills.is_empty() {
            // Not a failure of the formula: the height also takes what the jumper is carrying
            // and how big he is, and a recording carries neither, so a jump made under a load
            // cannot be reproduced from its extent alone. Counted and printed rather than
            // folded away, because it is the one thing in this measurement a later reader
            // would otherwise have to rediscover.
            unexplained += 1;
            println!(
                "jump height: extent {extent} speed {speed} is explained by no unburdened skill"
            );
            continue;
        }
        explained += 1;
        if get_jump_height(0.0, skills[0], *extent, 1.0) == FLOOR {
            floored += 1;
        } else if skills.len() == 1 {
            pinned += 1;
        }
    }
    println!(
        "jump: {} recorded jumps, {explained} explained by an unburdened jumper's own \
         skill ({floored} on the client's floor, {pinned} pinning one skill exactly), \
         {unexplained} not",
        recorded.len()
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.jump.the-height-the-client-works-out-explains-the-recorded-jumps",
        move |_| explained >= 2 && floored > 0 && pinned > 0,
    );
}

#[test]
fn scenario_the_clients_jump_height_explains_the_recorded_jumps() {
    scenario("the_clients_jump_height_explains_the_recorded_jumps");
}

/// Every drawn frame visits the jump dispatch, and a client nobody is pressing jump on invents
/// none.
pub fn every_drawn_frame_visits_the_jump_dispatch() {
    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.tick(3);

    c.assert_behaviour(
        "movement.jump.every-drawn-frame-visits-the-jump-dispatch-and-invents-none",
        move |v| {
            let app = v.expect_app();
            app.jump_use_times() == 3
                && app.jump_use_times() == app.frames_drawn()
                // Nobody pressed jump, so nothing may have been invented.
                && app.jump_counts() == (0, 0)
                && app.position_reporter_stats().jump_events == 0
        },
    );
    c.shutdown();
}

#[test]
fn scenario_every_drawn_frame_visits_the_jump_dispatch() {
    scenario("every_drawn_frame_visits_the_jump_dispatch");
}

// -------------------------------------------------------------------------------------------
// world.scene-less-frame.*
// -------------------------------------------------------------------------------------------

/// A drop armed on a frame that drew no world is answered on that frame, and the click after it
/// still works.
///
/// The second half is the one that matters and it is invisible to a single-drop measurement: a
/// reason that stays parked refuses the next viewport press outright.
pub fn a_drop_armed_with_nothing_drawn_is_answered() {
    use dereth_client::interaction::SearchReason;
    use dereth_primitives::{ObjectId, ServerTime};
    use dereth_ui_screens::view::{DropTarget, UiRequest};

    let store = support::store();
    let mut host = world_support::SceneLess::new(&store);
    let item = host.carry(ObjectId(0x2884_0001));

    // The drop release, which arms the pick and parks its reason.
    host.inter.queue(
        Vec::new(),
        vec![UiRequest::DragDrop {
            item,
            target: DropTarget::World,
        }],
    );
    host.clock += 1.0;
    let unowned =
        host.inter
            .run_ui_requests(&mut host.objects.world, false, ServerTime(host.clock));
    let armed = unowned.is_empty()
        && host.inter.pick.looking_for_object()
        && host.inter.search_reason() == SearchReason::Drop;
    let notices = host.inter.stats.scene_less_notices;

    // The rest of that same frame, with no world drawn.
    host.drive();
    let answered = host.inter.stats.scene_less_notices == notices + 1
        && !host.inter.pick.looking_for_object()
        // The clearing of the reason is the assertion the whole claim turns on.
        && host.inter.search_reason() == SearchReason::None;
    // The drop resolved rather than vanishing: nothing was named, so it took the ground leg,
    // which a client with no body of its own refuses -- and the item comes back out of the drag.
    let resolved = host.lines()
        == vec![(
            world_support::FEEDBACK_CHANNEL,
            world_support::MID_AIR.to_owned(),
        )]
        && !host
            .objects
            .world
            .weenie(item)
            .expect("the item is seeded")
            .waiting;

    // The next viewport press. This is the symptom the whole row exists for.
    let next_click_works =
        host.viewport_left_press() == 1 && host.inter.search_reason() == SearchReason::Select;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.scene-less-frame.a-drop-armed-with-nothing-drawn-is-answered-and-the-next-click-still-works",
        move |_| armed && answered && resolved && next_click_works,
    );
}

#[test]
fn scenario_a_drop_armed_with_nothing_drawn_is_answered() {
    scenario("a_drop_armed_with_nothing_drawn_is_answered");
}

/// A click and a double-click armed on a frame that drew no world are answered too, so the
/// gestures whose own gates they would otherwise block still work.
pub fn a_click_with_nothing_drawn_is_answered_too() {
    use dereth_client::interaction::SearchReason;
    use dereth_client::ui::UiMouseEvent;
    use dereth_ui_screens::screens::gameplay::window;

    let store = support::store();
    let mut host = world_support::SceneLess::new(&store);

    let first =
        host.viewport_left_press() == 1 && host.inter.search_reason() == SearchReason::Select;
    host.drive();
    let cleared = host.inter.stats.scene_less_notices == 1
        && host.inter.search_reason() == SearchReason::None;

    // The harder edge: a double-click parks a reason that sits above the examine gate.
    host.inter.wrapper_mouse(
        UiMouseEvent {
            action: 0x0A,
            start: true,
            x: 512,
            y: 384,
            over: Some(window::SMART_BOX),
        },
        (1024, 768),
        true,
    );
    let double = host.inter.search_reason() == SearchReason::Use;
    host.drive();
    let double_cleared = host.inter.stats.scene_less_notices == 2
        && host.inter.search_reason() == SearchReason::None;

    let armed = host.inter.pick.stats.requests;
    // The press first: a release with no press is not a gesture the client can produce.
    for start in [true, false] {
        host.inter.wrapper_mouse(
            UiMouseEvent {
                action: dereth_ui::focus::action::SECONDARY_CLICK,
                start,
                x: 512,
                y: 384,
                over: Some(window::SMART_BOX),
            },
            (1024, 768),
            true,
        );
    }
    let examine = host.inter.pick.stats.requests - armed == 1
        && host.inter.search_reason() == SearchReason::Examine;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.scene-less-frame.a-click-and-a-double-click-are-answered-so-the-gestures-after-them-still-work",
        move |_| first && cleared && double && double_cleared && examine,
    );
}

#[test]
fn scenario_a_click_with_nothing_drawn_is_answered_too() {
    scenario("a_click_with_nothing_drawn_is_answered_too");
}

/// A frame that drew no world and has nothing armed answers nothing, and leaves a reason that
/// was deliberately parked exactly where it was.
///
/// This is the control for the two above: an arm that always fired would satisfy every "it
/// fired" assertion they make.
pub fn a_frame_with_nothing_armed_answers_nothing() {
    use dereth_client::interaction::SearchReason;
    use dereth_client::ui::UiMouseEvent;
    use dereth_primitives::ObjectId;
    use dereth_ui_screens::screens::gameplay::window;
    use dereth_ui_screens::view::{DropTarget, UiRequest};

    let store = support::store();
    let mut host = world_support::SceneLess::new(&store);
    let idle_to_start = !host.inter.pick.looking_for_object();

    for _ in 0..5 {
        host.drive();
    }
    let five_idle_frames_raise_nothing = host.inter.stats.scene_less_notices == 0;

    // Now park a reason **without** arming a pick: a drop whose point is outside the viewport.
    // The client latches here too, and this arm must not paper over that.
    let parked = host.carry(ObjectId(0x2884_0020));
    host.inter.queue(
        vec![UiMouseEvent {
            action: 9,
            start: true,
            x: 9000,
            y: 9000,
            over: Some(window::SMART_BOX),
        }],
        vec![UiRequest::DragDrop {
            item: parked,
            target: DropTarget::World,
        }],
    );
    host.drive();
    let rejected = host.inter.pick.stats.outside_viewport == 1;
    let still_nothing = host.inter.stats.scene_less_notices == 0;
    let still_parked = host.inter.search_reason() == SearchReason::Drop;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.scene-less-frame.a-frame-with-nothing-armed-answers-nothing-and-clears-nothing",
        move |_| {
            idle_to_start
                && five_idle_frames_raise_nothing
                && rejected
                && still_nothing
                && still_parked
        },
    );
}

#[test]
fn scenario_a_frame_with_nothing_armed_answers_nothing() {
    scenario("a_frame_with_nothing_armed_answers_nothing");
}

/// The answer a scene-less frame gives names nothing, so a dropped item takes the ground leg
/// rather than being handed to whatever the last sweep found.
pub fn the_scene_less_answer_names_nothing() {
    use dereth_client::interaction::SearchReason;
    use dereth_primitives::{ObjectId, ServerTime};
    use dereth_ui_screens::view::{DropTarget, UiRequest};

    const CREATURE: ObjectId = ObjectId(0x2884_0031);

    let store = support::store();
    let mut host = world_support::SceneLess::new(&store);

    // A creature in the tables and a previous pick that found it, so that a stale answer has
    // something to be stale with: an answer naming it would have produced a give.
    let mut npc = dereth_client_model::Weenie::new(CREATURE);
    npc.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
    npc.pwd.name = "Ulgrim".to_owned();
    host.objects.world.tables.weenies.insert(CREATURE, npc);
    host.inter
        .on_world_object_found(CREATURE, &mut host.objects.world, ServerTime(host.clock));

    let item = host.carry(ObjectId(0x2884_0030));
    host.inter.queue(
        Vec::new(),
        vec![UiRequest::DragDrop {
            item,
            target: DropTarget::World,
        }],
    );
    host.clock += 1.0;
    let _ = host
        .inter
        .run_ui_requests(&mut host.objects.world, false, ServerTime(host.clock));
    host.drive();

    let answered = host.inter.stats.scene_less_notices == 1;
    let ground_leg = host.lines()
        == vec![(
            world_support::FEEDBACK_CHANNEL,
            world_support::MID_AIR.to_owned(),
        )];
    let cleared = host.inter.search_reason() == SearchReason::None;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.scene-less-frame.the-answer-names-nothing-so-a-dropped-item-takes-the-ground-leg",
        move |_| answered && ground_leg && cleared,
    );
}

#[test]
fn scenario_the_scene_less_answer_names_nothing() {
    scenario("the_scene_less_answer_names_nothing");
}

// -------------------------------------------------------------------------------------------
// world.physics-script.* (the delayed call)
// -------------------------------------------------------------------------------------------

/// A shipped script that calls another one after a pause really plays it, after the delay it
/// rolled for itself.
///
/// The child's arrival is watched through the emitter **it** creates at its own start, not
/// through a counter: a counter can be incremented by wiring that schedules nothing.
pub fn a_delayed_script_call_plays_after_its_delay() {
    use dereth_animation::AnimEvent;
    use dereth_assets::{Decode, HookData, PhysicsScript};
    use dereth_dat::DbType;
    use dereth_primitives::ServerTime;

    let store = support::store();

    // The premise, asserted rather than assumed: the shipped script this drives creates an
    // emitter at its start and calls its neighbour half a second later, with a pause to roll
    // the delay over.
    let bytes = store
        .read_typed(DbType::PhysicsScript, world_support::RING_PARENT)
        .expect("the ring script is shipped");
    let script =
        PhysicsScript::decode_payload(world_support::RING_PARENT, &bytes).expect("it decodes");
    let shape: Vec<(f64, u32)> = script
        .script_data
        .iter()
        .map(|s| (s.start_time, s.hook.hook_type))
        .collect();
    let shaped = shape
        == vec![
            (0.0, world_support::CREATE_PARTICLE),
            (0.5, world_support::CALL_PES),
        ];
    let HookData::CallPes { pes, pause } = script.script_data[1].hook.data else {
        panic!("the ring script's second step is not a call")
    };
    let names_the_child = pes == world_support::RING_CHILD && (pause - 0.5).abs() < 1e-6;

    let mut d = world_support::script_driver(&store);
    d.cur_time = ServerTime(0.0);
    assert!(
        d.play_script_internal(world_support::RING_PARENT),
        "the shipped parent script queues"
    );

    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut armed_at: Option<(f64, f32)> = None;
    let mut emitters: Vec<f64> = Vec::new();
    for _ in 0..60 {
        for e in world_support::script_tick(&mut d, t) {
            match e {
                AnimEvent::CallPes { script, pause } => {
                    if armed_at.is_none() && script == world_support::RING_CHILD {
                        armed_at = Some((t, pause));
                    }
                }
                AnimEvent::CreateParticleEmitter { .. } => emitters.push(t),
                _ => {}
            }
        }
        t += dt;
    }

    let (arm_t, delay) = armed_at.expect("the parent's call step never executed at all");
    println!("effect delay: armed at t={arm_t:.4}s for {delay}s; emitters at {emitters:?}");
    let rolled = delay > 0.0 && delay <= 0.5;
    let armed_on_its_own_step = (arm_t - 0.5).abs() < dt;
    let parents_own_emitter = emitters.first().copied().is_some_and(|x| x < dt);
    let after: Vec<f64> = emitters.iter().copied().filter(|x| *x > arm_t).collect();
    let due = arm_t + f64::from(delay);
    let child_played = after
        .first()
        .copied()
        .is_some_and(|fired| fired >= due - 1e-9 && fired <= due + 3.0 * dt);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.physics-script.a-delayed-call-plays-the-script-it-names-after-the-delay-it-rolled",
        move |_| {
            shaped
                && names_the_child
                && rolled
                && armed_on_its_own_step
                && parents_own_emitter
                && child_played
        },
    );
}

#[test]
fn scenario_a_delayed_script_call_plays_after_its_delay() {
    scenario("a_delayed_script_call_plays_after_its_delay");
}

/// An object that leaves the world before its delayed call is due plays nothing, and coming back
/// does not resurrect the timer it spent.
pub fn an_object_that_leaves_before_the_delay_plays_nothing() {
    use dereth_animation::AnimEvent;
    use dereth_primitives::ServerTime;

    let store = support::store();
    let mut d = world_support::script_driver(&store);
    d.cur_time = ServerTime(0.0);
    assert!(
        d.play_script_internal(world_support::RING_PARENT),
        "the shipped parent script queues"
    );

    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut armed: Option<(f64, f32)> = None;
    while armed.is_none() && t < 3.0 {
        for e in world_support::script_tick(&mut d, t) {
            if let AnimEvent::CallPes { pause, .. } = e {
                armed = Some((t, pause));
            }
        }
        t += dt;
    }
    let (arm_t, delay) = armed.expect("the call step never executed");
    let timer_armed = !d.fp_hooks.is_empty();

    // The object leaves its room before the timer is due. The room is read at the moment the
    // timer fires, not at the moment it was armed.
    d.env.in_cell = false;
    let mut emitters = 0u32;
    for _ in 0..90 {
        for e in world_support::script_tick(&mut d, t) {
            if matches!(e, AnimEvent::CreateParticleEmitter { .. }) {
                emitters += 1;
            }
        }
        t += dt;
    }
    let nothing_played = emitters == 0;
    let timer_unlinked = d.fp_hooks.is_empty();

    // ...and it stays cancelled: coming back does not resurrect the timer that was spent.
    d.env.in_cell = true;
    let mut later = 0u32;
    for _ in 0..90 {
        for e in world_support::script_tick(&mut d, t) {
            if matches!(e, AnimEvent::CreateParticleEmitter { .. }) {
                later += 1;
            }
        }
        t += dt;
    }
    println!("effect delay cancel: armed at {arm_t:.4}s for {delay}s, {emitters} then {later}");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.physics-script.an-object-that-leaves-its-room-before-the-delay-is-up-plays-nothing",
        move |_| timer_armed && nothing_played && timer_unlinked && later == 0,
    );
}

#[test]
fn scenario_an_object_that_leaves_before_the_delay_plays_nothing() {
    scenario("an_object_that_leaves_before_the_delay_plays_nothing");
}

/// A shipped call with no pause still plays the script it names on the spot.
pub fn a_script_call_with_no_delay_plays_on_the_spot() {
    use dereth_animation::AnimEvent;
    use dereth_assets::{Decode, HookData, PhysicsScript};
    use dereth_dat::DbType;
    use dereth_primitives::ServerTime;

    let store = support::store();

    // The first shipped script whose call has no pause and names another script that ships.
    // Searched rather than named, so that this arm is measured over whatever the data holds.
    let mut found = None;
    'outer: for id in &store.ids_of(DbType::PhysicsScript) {
        let Ok(bytes) = store.read_typed(DbType::PhysicsScript, *id) else {
            continue;
        };
        let Ok(s) = PhysicsScript::decode_payload(*id, &bytes) else {
            continue;
        };
        for step in &s.script_data {
            if let HookData::CallPes { pes, pause } = step.hook.data {
                if pause < world_support::HOOK_EPSILON
                    && pes != *id
                    && store.read_typed(DbType::PhysicsScript, pes).is_ok()
                {
                    found = Some((*id, pes, step.start_time));
                    break 'outer;
                }
            }
        }
    }
    let (parent, child, at) =
        found.expect("some shipped script calls another one with no pause at all");
    println!(
        "effect delay immediate: {:#010X} calls {:#010X} at t={at:.3}",
        parent.0, child.0
    );

    let mut d = world_support::script_driver(&store);
    d.cur_time = ServerTime(0.0);
    assert!(
        d.play_script_internal(parent),
        "the shipped parent script queues"
    );

    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut on_the_spot = false;
    let mut early = false;
    let mut delayed = false;
    while t < at + 1.0 {
        for e in world_support::script_tick(&mut d, t) {
            if let AnimEvent::CallPes { script, pause } = e {
                if script == child {
                    on_the_spot = true;
                    delayed |= pause != 0.0;
                    early |= t < at - dt;
                }
            }
        }
        t += dt;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.physics-script.a-call-with-no-delay-still-plays-on-the-spot",
        move |_| on_the_spot && !delayed && !early,
    );
}

#[test]
fn scenario_a_script_call_with_no_delay_plays_on_the_spot() {
    scenario("a_script_call_with_no_delay_plays_on_the_spot");
}

// -------------------------------------------------------------------------------------------
// world.physics-script.* (the scale hook)
// -------------------------------------------------------------------------------------------

/// A shipped hook that makes an object bigger makes its collision body bigger with it.
pub fn a_scale_hook_moves_the_collision_radius() {
    use dereth_primitives::Vec3;

    let store = support::store();
    let scale = world_support::scale_the_shipped_script_asks_for(&store);
    let doubles = (scale - 2.0).abs() < 1e-6;

    let mut w = world_support::flat_world();
    let h = world_support::place_obstacle(&mut w, Vec3::new(30.0, 30.0, world_support::GROUND));
    let starts_at_one = {
        let o = w.get(h).expect("the obstacle is live");
        (o.scale - 1.0).abs() < 1e-6
            && (o.radius() - 0.6).abs() < 1e-6
            && (o.height() - 1.2).abs() < 1e-6
    };

    // What the scene does with the hook the script raised.
    w.get_mut(h).expect("the obstacle is live").scale = scale;

    let o = w.get(h).expect("the obstacle is live");
    let followed = (o.radius() - 1.2).abs() < 1e-6 && (o.height() - 2.4).abs() < 1e-6;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.physics-script.a-scale-hook-moves-the-objects-own-collision-radius",
        move |_| doubles && starts_at_one && followed,
    );
}

#[test]
fn scenario_a_scale_hook_moves_the_collision_radius() {
    scenario("a_scale_hook_moves_the_collision_radius");
}

/// A body walking at an object a shipped hook has made bigger stops at the bigger distance.
///
/// A differential: the same world, the same walk and the same object in both arms, and the only
/// variable is whether the hook reached the object. The approach comes in off the normal,
/// because a head-on walk can creep through a surface and would make this a measurement of that
/// instead.
pub fn a_body_walking_at_a_scaled_object_stops_further_away() {
    use dereth_primitives::{LocalTime, Vec3};

    let store = support::store();
    let scale = world_support::scale_the_shipped_script_asks_for(&store);

    let obstacle = Vec3::new(40.0, 40.0, world_support::GROUND);
    // Six metres out, aimed a metre to one side of the centre so the walk grazes rather than
    // wedges.
    let start = Vec3::new(40.0 - 6.0, 40.0 - 1.0, world_support::GROUND);
    let step = Vec3::new(0.05, 0.0, 0.0);

    let closest = |apply: bool| -> f32 {
        let mut w = world_support::flat_world();
        let obj = world_support::place_obstacle(&mut w, obstacle);
        if apply {
            w.get_mut(obj).expect("the obstacle is live").scale = scale;
        }
        let body = world_support::spawn_walker(&mut w, start, step);
        let mut t = 0.0;
        let mut best = f32::MAX;
        for _ in 0..300 {
            t += 1.0 / 30.0;
            w.use_time(LocalTime(t), false);
            let p = w
                .get(body)
                .expect("the walker is live")
                .position
                .frame
                .origin;
            best = best.min(dereth_primitives::num::math::hypotf(
                p.x - obstacle.x,
                p.y - obstacle.y,
            ));
        }
        best
    };

    let plain = closest(false);
    let scaled = closest(true);
    println!("scale hook: closest approach {plain:.4} m plain, {scaled:.4} m scaled ({scale})");

    // Scaling an object scales its sphere's centre as well as its radius, so the distance two
    // spheres touch at, projected into the ground plane, is what the walk should rest at. It is
    // written out rather than fitted to the run, because "stops at the scaled radius" is a
    // number and not a direction.
    let contact = |s: f32| -> f32 {
        let dz = 0.6 * s - 0.5;
        let d = 0.6 * s + 0.5;
        (d * d - dz * dz).sqrt()
    };
    let unscaled_is_right = (plain - contact(1.0)).abs() < 0.03;
    let further = scaled > plain + 0.3;
    let scaled_is_right = (scaled - contact(scale)).abs() < 0.03;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.physics-script.a-body-walking-at-a-scaled-object-stops-at-the-scaled-distance",
        move |_| unscaled_is_right && further && scaled_is_right,
    );
}

#[test]
fn scenario_a_body_walking_at_a_scaled_object_stops_further_away() {
    scenario("a_body_walking_at_a_scaled_object_stops_further_away");
}

// -------------------------------------------------------------------------------------------
// camera.wall.*
// -------------------------------------------------------------------------------------------

/// The camera backed into a wall settles in the same place however often the client ticks.
///
/// The measurement is the instrument's: the same wall-clock trajectory driven at two rates, with
/// the body's position a pure function of time in both, so the only difference between the arms
/// is the step. Over every wall approach the training academy offers, not one.
pub fn the_camera_against_a_wall_settles_the_same_at_any_tick_rate() {
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{CellId, LandblockId};

    /// The client's own early-out distance: a camera that varies by less than this over a whole
    /// second is, by the client's own definition, not moving.
    const SETTLE_DISTANCE: f32 = 0.000_4;
    /// How far behind the pivot an unobstructed camera sits.
    const FREE_CAMERA_DISTANCE: f32 = 2.610_077;
    const RESIDUAL_WINDOW: f64 = 1.0;

    let store = std::sync::Arc::new(dereth_dat::testing::open_store_or_fail());
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let src = std::sync::Arc::new(
        dereth_client::land_source::DatLandSource::new(std::sync::Arc::clone(&store), &region)
            .expect("the retail height table"),
    );
    src.load_block_cells(LandblockId(world_support::TRAINING_DUNGEON));

    let mut loader = dereth_client::env_cells::EnvCellLoader::new();
    let ids: Vec<CellId> = loader
        .load_block(&store, world_support::TRAINING_DUNGEON)
        .iter()
        .map(|d| d.id)
        .collect();
    assert!(!ids.is_empty(), "the training academy has interior cells");

    let walls = world_support::find_walls(&src, &ids, 8);
    // Four, deliberately: a single approach that happens to settle proves nothing about the
    // solver, and the shape this measures did not appear at every heading.
    let enough = walls.len() >= 4;

    let mut worst_slow = 0.0f32;
    let mut worst_fast = 0.0f32;
    let mut worst_separation = 0.0f32;
    let mut pressed = 0usize;
    let mut standing_off = 0usize;
    for (wall, slow) in &walls {
        let fast = world_support::run_camera(&src, wall, 1.0 / 144.0);
        let slow_pull = slow.max_pivot_distance(RESIDUAL_WINDOW);
        let fast_pull = fast.max_pivot_distance(RESIDUAL_WINDOW);
        // Calibration, before anything is concluded from a zero: the wall has to do the work.
        if slow_pull < FREE_CAMERA_DISTANCE * 0.5 && fast_pull < FREE_CAMERA_DISTANCE * 0.5 {
            pressed += 1;
        }
        // A camera collapsed onto the body is a legitimate state and a useless measurement: it
        // cannot move, so it cannot be unsteady.
        if slow_pull.max(fast_pull) > 0.2 {
            standing_off += 1;
        }
        worst_slow = worst_slow.max(slow.residual(RESIDUAL_WINDOW));
        worst_fast = worst_fast.max(fast.residual(RESIDUAL_WINDOW));
        worst_separation = worst_separation.max(slow.last().sub(fast.last()).mag2().sqrt());
    }
    println!(
        "camera wall: {} wall approaches, {pressed} pressed into geometry, {standing_off} holding \
         the camera clear -- slow {worst_slow:.6} m, fast {worst_fast:.6} m, separation \
         {worst_separation:.6} m",
        walls.len()
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "camera.wall.settles-in-the-same-place-however-often-the-client-ticks",
        move |_| {
            enough
                && pressed == walls.len()
                && standing_off >= 4
                && worst_slow < SETTLE_DISTANCE
                && worst_fast < SETTLE_DISTANCE
                && worst_separation < 0.01
        },
    );
}

#[test]
fn scenario_the_camera_against_a_wall_settles_the_same_at_any_tick_rate() {
    scenario("the_camera_against_a_wall_settles_the_same_at_any_tick_rate");
}

// -------------------------------------------------------------------------------------------
// movement.server-control.*, movement.teleport.*, movement.approach.*
// -------------------------------------------------------------------------------------------

/// Letting go of a key stops the body, even after the shard has taken control of it.
///
/// The state it happens in is a player standing still when the shard sends him a movement of
/// its own: nothing in the frame can hand control back from there, so if the key press itself
/// cannot, nothing ever will.
pub fn a_key_release_stops_the_body_after_the_shard_has_taken_control() {
    use dereth_client_runtime::actions::movement::action;

    let stop = world_support::a_recorded_stop();
    let (mut c, mut mc, mut input) = world_support::running_body();

    world_support::server_takes_control(&c, &mut mc, &mut input, &stop);
    let taken = mc.lists.controlled_by_server;

    // The premise, and it is the client's own behaviour rather than a defect: the frame's own
    // retake refuses with three empty lists and no key held.
    let idle = world_support::drive(&mut c, &mut mc, &mut input, 60, 60);
    let no_retake = idle.retakes == 0 && mc.lists.controlled_by_server;

    world_support::key(&mut c, &mut mc, &mut input, action::MOVE_FORWARD, true);
    let press_took_it_back =
        !mc.lists.controlled_by_server && mc.lists.substate.len() == 1 && input.forward;

    // He must actually be running, or "he stopped" is satisfied by a body that never moved.
    let moving = world_support::drive(&mut c, &mut mc, &mut input, 120, 60);
    let running = world_support::over(&moving, 0, 60) > world_support::RUNNING;

    world_support::key(&mut c, &mut mc, &mut input, action::MOVE_FORWARD, false);
    let released = mc.lists.substate.is_empty() && !input.forward;
    let after = world_support::drive(&mut c, &mut mc, &mut input, 180, 90);
    let stopped = world_support::over(&after, 30, 90) < world_support::STOPPED;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.server-control.a-key-release-stops-the-body-after-the-shard-has-taken-control",
        move |_| taken && no_retake && press_took_it_back && running && released && stopped,
    );
}

#[test]
fn scenario_a_key_release_stops_the_body_after_the_shard_has_taken_control() {
    scenario("a_key_release_stops_the_body_after_the_shard_has_taken_control");
}

/// The same for a turn, which the client keeps on a different list: a measurement that only
/// watched the walking list would pass with the other two dead.
pub fn a_turn_release_stops_the_turn_the_same_way() {
    use dereth_client_runtime::actions::movement::action;

    let stop = world_support::a_recorded_stop();
    let (mut c, mut mc, mut input) = world_support::running_body();
    world_support::server_takes_control(&c, &mut mc, &mut input, &stop);
    let idle = world_support::drive(&mut c, &mut mc, &mut input, 60, 60);
    let taken = mc.lists.controlled_by_server && idle.retakes == 0;

    world_support::key(&mut c, &mut mc, &mut input, action::TURN_LEFT, true);
    let press_took_it_back = !mc.lists.controlled_by_server && input.turn_left;
    let before = c.position().frame.rotation;
    let _ = world_support::drive(&mut c, &mut mc, &mut input, 120, 60);
    let turned = c.position().frame.rotation;
    let really_turning = (turned.w - before.w).abs() + (turned.z - before.z).abs() > 0.01;

    world_support::key(&mut c, &mut mc, &mut input, action::TURN_LEFT, false);
    let released = mc.lists.turn.is_empty() && !input.turn_left;
    let a = c.position().frame.rotation;
    let _ = world_support::drive(&mut c, &mut mc, &mut input, 180, 90);
    let b = c.position().frame.rotation;
    let stopped = (b.w - a.w).abs() + (b.z - a.z).abs() < 0.01;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.server-control.a-turn-release-stops-the-turn-the-same-way",
        move |_| taken && press_took_it_back && really_turning && released && stopped,
    );
}

#[test]
fn scenario_a_turn_release_stops_the_turn_the_same_way() {
    scenario("a_turn_release_stops_the_turn_the_same_way");
}

/// After the shard teleports a running player he arrives where it sent him, stands still until
/// he asks to move, and then moves.
///
/// The three halves are asserted separately, because "he is at the destination" is satisfied by
/// a frozen body just as readily as by a working one.
pub fn the_player_can_move_again_after_the_shard_teleports_him() {
    use dereth_client_runtime::actions::movement::action;
    use dereth_primitives::{Frame, Position, Vec3};

    /// Somewhere else in the same piece of land, and high enough that the body falls to it: a
    /// teleport that keeps the departure's height puts the body inside the hill at the far end,
    /// and a body inside a hill stands perfectly still for a reason that is not this one.
    const DEST: (f32, f32) = (24.0, 132.0);

    let (departure, arrival) = world_support::two_recorded_stops();
    let (mut c, mut mc, mut input) = world_support::running_body();

    world_support::key(&mut c, &mut mc, &mut input, action::MOVE_FORWARD, true);
    let moving = world_support::drive(&mut c, &mut mc, &mut input, 60, 60);
    let running_in = world_support::over(&moving, 0, 60) > world_support::RUNNING;

    world_support::server_takes_control(&c, &mut mc, &mut input, &departure);
    let taken_on_the_way_out = mc.lists.controlled_by_server;
    // He lets go of the key while the shard has control. This is the release that is swallowed.
    world_support::key(&mut c, &mut mc, &mut input, action::MOVE_FORWARD, false);
    let release_landed =
        mc.lists.substate.is_empty() && !input.forward && mc.lists.controlled_by_server;

    let here = c.position();
    let dest = Position::new(
        here.cell,
        Frame::new(
            Vec3 {
                x: DEST.0,
                y: DEST.1,
                z: here.frame.origin.z + 10.0,
            },
            here.frame.rotation,
        ),
    );
    c.teleport(dest);
    let falling = world_support::drive(&mut c, &mut mc, &mut input, 120, 90);
    let landed = c.position().frame.origin;
    let miss = ((landed.x - DEST.0).powi(2) + (landed.y - DEST.1).powi(2)).sqrt();
    let arrived = falling.retakes == 0 && miss < 1.0 && c.on_ground();

    world_support::server_takes_control(&c, &mut mc, &mut input, &arrival);
    let arrival_took_control = mc.lists.controlled_by_server;

    // Nothing is held, so he stands where he landed. A body that starts running by itself after
    // a teleport is the other face of the same thing.
    let standing = world_support::drive(&mut c, &mut mc, &mut input, 180, 60);
    let still = world_support::over(&standing, 30, 60) < world_support::STOPPED;

    world_support::key(&mut c, &mut mc, &mut input, action::MOVE_FORWARD, true);
    let after = world_support::drive(&mut c, &mut mc, &mut input, 240, 90);
    let resumed = world_support::over(&after, 30, 90) > world_support::RUNNING;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.teleport.the-player-can-move-again-after-the-shard-teleports-him",
        move |_| {
            running_in
                && taken_on_the_way_out
                && release_landed
                && arrived
                && arrival_took_control
                && still
                && resumed
        },
    );
}

#[test]
fn scenario_the_player_can_move_again_after_the_shard_teleports_him() {
    scenario("the_player_can_move_again_after_the_shard_teleports_him");
}

/// A key press that takes control back re-issues the key that was already held.
///
/// Reaching it needs the retake to come from the press rather than from the frame's own, and the
/// press must be for a different list: with the walk key held the frame would retake first. So
/// the turn key goes down in the same instant the shard's stop arrives, which is an ordinary
/// thing for a player to do and the only order in which the two retakes can be told apart.
pub fn a_press_that_takes_control_back_re_issues_the_key_already_held() {
    use dereth_client_runtime::actions::movement::action;

    let stop = world_support::a_recorded_stop();
    let (mut c, mut mc, mut input) = world_support::running_body();

    world_support::key(&mut c, &mut mc, &mut input, action::MOVE_FORWARD, true);
    let moving = world_support::drive(&mut c, &mut mc, &mut input, 60, 60);
    let running = world_support::over(&moving, 0, 60) > world_support::RUNNING;

    // The shard stops the body. What the player is holding is untouched.
    world_support::server_takes_control(&c, &mut mc, &mut input, &stop);
    let held = mc.lists.controlled_by_server && input.forward && mc.lists.substate.len() == 1;

    world_support::key(&mut c, &mut mc, &mut input, action::TURN_LEFT, true);
    let press_took_it_back = !mc.lists.controlled_by_server;

    let run = world_support::drive(&mut c, &mut mc, &mut input, 120, 90);
    // Along the path, not across the chord: he is turning as well as running, and the straight
    // line between the ends understates a body on an arc by half.
    let resumed = world_support::along(&run, 30, 90) > world_support::RUNNING;
    let nothing_left_to_retake = run.retakes == 0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.server-control.a-press-that-takes-control-back-re-issues-the-key-already-held",
        move |_| running && held && press_took_it_back && nothing_left_to_retake && resumed,
    );
}

#[test]
fn scenario_a_press_that_takes_control_back_re_issues_the_key_already_held() {
    scenario("a_press_that_takes_control_back_re_issues_the_key_already_held");
}

/// Neither an approach walk nor the target updates that feed it leaves a motion behind.
///
/// One motion left on the ledger makes the client believe it is still busy for the rest of the
/// session, which shuts the frame's own retake and the readiness check alike.
pub fn neither_an_approach_nor_a_target_update_leaves_a_motion_behind() {
    use dereth_primitives::{Frame, LocalTime, ObjectId, Position, Vec3};

    const TARGET: ObjectId = ObjectId(0x8000_0997);

    let store = support::store();
    let mut c = world_support::settled_body(&store);
    let starts_clean = c.driver().movement.interp.pending_motions.is_empty();

    let here = c.position();
    let target = Position::new(
        here.cell,
        Frame::new(
            Vec3 {
                x: 104.0,
                y: 96.0,
                z: here.frame.origin.z,
            },
            here.frame.rotation,
        ),
    );
    let params = dereth_animation::motion::MovementParameters::default();
    c.perform_move_to(
        &dereth_animation::motion::MoveToRequest::MoveToPosition { pos: target },
        &params,
        None,
    );
    for i in 1..=90 {
        c.update(LocalTime(2.0 + f64::from(i) / 30.0));
    }
    let after_the_walk = c.driver().movement.interp.pending_motions.is_empty()
        && c.driver().motion_table.pending().is_empty()
        && !c.driver().movement.motions_pending();

    // The second half: the target updates the object stream drives for every approach walk.
    let at = c.position();
    let object = Position::new(
        at.cell,
        Frame::new(
            Vec3 {
                x: at.frame.origin.x + 6.0,
                y: at.frame.origin.y,
                z: at.frame.origin.z,
            },
            at.frame.rotation,
        ),
    );
    c.perform_move_to(
        &dereth_animation::motion::MoveToRequest::MoveToObject {
            object_id: TARGET,
            top_level_id: TARGET,
            radius: 0.0,
            height: 0.0,
        },
        &params,
        Some(1.0),
    );
    let watching = c.wanted_target().is_some();
    let mut updates = 0u32;
    for i in 1..=150 {
        c.update(LocalTime(6.0 + f64::from(i) / 30.0));
        if c.wanted_target().is_some() {
            c.update_target(object, Vec3::ZERO, true);
            updates += 1;
        }
    }
    let after_the_updates = c.driver().movement.interp.pending_motions.is_empty()
        && c.driver().motion_table.pending().is_empty();
    println!("endless motion: {updates} target updates, both ledgers drained");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.neither-an-approach-nor-a-target-update-leaves-a-motion-behind",
        move |_| starts_clean && after_the_walk && watching && updates > 0 && after_the_updates,
    );
}

#[test]
fn scenario_neither_an_approach_nor_a_target_update_leaves_a_motion_behind() {
    scenario("neither_an_approach_nor_a_target_update_leaves_a_motion_behind");
}

/// A teleport with no shard involvement at all still leaves a body that can be driven.
///
/// The control for the teleport scenario above, and the discriminating one: if this passes while
/// that fails, what froze the body belongs to the handover of control; if both fail, it belongs
/// to the teleport.
pub fn a_teleport_with_no_shard_involvement_leaves_a_movable_body() {
    use dereth_client_runtime::actions::movement::action;
    use dereth_primitives::{Frame, Position, Vec3};

    const DEST: (f32, f32) = (24.0, 132.0);

    let (mut c, mut mc, mut input) = world_support::running_body();
    let here = c.position();
    c.teleport(Position::new(
        here.cell,
        Frame::new(
            Vec3 {
                x: DEST.0,
                y: DEST.1,
                z: here.frame.origin.z + 10.0,
            },
            here.frame.rotation,
        ),
    ));
    let _ = world_support::drive(&mut c, &mut mc, &mut input, 60, 90);
    let landed = c.on_ground();

    world_support::key(&mut c, &mut mc, &mut input, action::MOVE_FORWARD, true);
    let after = world_support::drive(&mut c, &mut mc, &mut input, 150, 90);
    let moved = world_support::over(&after, 30, 90) > world_support::RUNNING;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.teleport.a-teleport-with-no-shard-involvement-leaves-a-body-that-still-moves",
        move |_| landed && moved,
    );
}

#[test]
fn scenario_a_teleport_with_no_shard_involvement_leaves_a_movable_body() {
    scenario("a_teleport_with_no_shard_involvement_leaves_a_movable_body");
}

// -------------------------------------------------------------------------------------------
// movement.action.*, movement.teleport.* (the ground edge)
// -------------------------------------------------------------------------------------------

/// A swing the shard authored, which the body cannot play, holds every motion behind it for
/// exactly as long as its own animation and not a frame longer.
///
/// Both ends are measured. Inside the animation the body does not move however many keys are
/// held, which is the whole of what a player would report; by the animation's own length the
/// ledger is empty and the held key moves him. The bound comes from the animation the data
/// carries rather than from a number written here, so a different action moves the window
/// instead of breaking the measurement.
pub fn a_shard_authored_swing_holds_the_body_for_its_own_animation() {
    use dereth_client_runtime::actions::movement::action;

    let stop = world_support::a_recorded_stop_with_an_action();
    let (mut c, mut mc, mut input) = world_support::running_body();

    world_support::server_takes_control(&c, &mut mc, &mut input, &stop);
    world_support::key(&mut c, &mut mc, &mut input, action::MOVE_FORWARD, true);
    let held = input.forward && !mc.lists.controlled_by_server;

    let (frames, framerate) = {
        let d = c.driver();
        let n = d
            .sequence
            .nodes()
            .first()
            .expect("the action queued an animation");
        (n.high_frame - n.low_frame + 1, n.framerate)
    };
    // The premise: a long animation at the client's own rate.
    let long = frames > 60 && (framerate - 30.0).abs() < 0.001;

    let early = world_support::drive(&mut c, &mut mc, &mut input, 60, 120);
    let rooted = world_support::over(&early, 60, 120) < world_support::STOPPED
        && c.driver().movement.motions_pending();

    // And then it ends.
    let budget = u32::try_from(frames).expect("a positive frame count") + 1;
    let late = world_support::drive(&mut c, &mut mc, &mut input, 180, budget + 90);
    let freed =
        !c.driver().movement.motions_pending() && c.driver().motion_table.pending().is_empty();
    let resumed = world_support::over(&late, budget - 60, budget + 30) > world_support::RUNNING;
    println!("action freeze: the action's animation is {frames} frames at {framerate} a second");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.action.a-shard-authored-swing-holds-every-motion-behind-it-for-its-own-animation",
        move |_| held && long && rooted && freed && resumed,
    );
}

#[test]
fn scenario_a_shard_authored_swing_holds_the_body_for_its_own_animation() {
    scenario("a_shard_authored_swing_holds_the_body_for_its_own_animation");
}

/// Leaving the ground empties the whole motion ledger and routes what it drained.
///
/// Three facts, not one: the ledger was genuinely full first, the long animation leaves the
/// sequence on take-off, and the action leaves the interpreted state -- which only the routed
/// completion does, so the drained motions were handed on rather than dropped on the floor.
/// The middle sample is taken **in the air**, between the two edges, because either edge would
/// leave the ledger clear by the time the body lands.
pub fn leaving_the_ground_empties_the_motion_ledger_and_routes_it() {
    use dereth_client::character::GroundEdges;
    use dereth_client_runtime::actions::movement::action;

    let stop = world_support::a_recorded_stop_with_an_action();
    let (mut c, mut mc, mut input) = world_support::running_body();
    world_support::server_takes_control(&c, &mut mc, &mut input, &stop);

    let queued: Vec<dereth_animation::MotionCommand> = c
        .driver()
        .movement
        .interp
        .pending_motions
        .iter()
        .map(|n| n.motion)
        .collect();
    let full = queued.iter().any(|m| m.is_action())
        && c.driver().movement.interp.interpreted_state.actions.len() == 1;
    let long = c
        .driver()
        .sequence
        .nodes()
        .iter()
        .map(|n| n.high_frame - n.low_frame + 1)
        .max()
        .expect("the action queued an animation");
    let standing = long > 60 && c.ground_edges() == GroundEdges { hit: 1, left: 0 };

    world_support::key(&mut c, &mut mc, &mut input, action::JUMP, true);
    let _ = world_support::drive(&mut c, &mut mc, &mut input, 60, 5);
    let left = c.ground_edges().left == 1;

    let in_the_air = {
        let airborne = !c.on_ground();
        let d = c.driver();
        let gone = d.movement.interp.interpreted_state.actions.is_empty();
        let longest = d
            .sequence
            .nodes()
            .iter()
            .map(|n| n.high_frame - n.low_frame + 1)
            .max();
        airborne && gone && longest.is_none_or(|n| n < long)
    };

    world_support::key(&mut c, &mut mc, &mut input, action::JUMP, false);
    let _ = world_support::drive(&mut c, &mut mc, &mut input, 65, 60);
    let back_down = c.on_ground();
    let after = {
        let d = c.driver();
        let longest = d
            .sequence
            .nodes()
            .iter()
            .map(|n| n.high_frame - n.low_frame + 1)
            .max();
        d.movement.interp.pending_motions.is_empty()
            && d.motion_table.pending().is_empty()
            && d.movement.interp.interpreted_state.actions.is_empty()
            && longest.is_none_or(|n| n < long)
    };

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.action.leaving-the-ground-empties-the-motion-ledger-and-routes-what-it-drained",
        move |_| full && standing && left && in_the_air && back_down && after,
    );
}

#[test]
fn scenario_leaving_the_ground_empties_the_motion_ledger_and_routes_it() {
    scenario("leaving_the_ground_empties_the_motion_ledger_and_routes_it");
}

/// A teleport that changes the ground under the body raises the edge that empties the ledger,
/// and the key that was held the whole time moves him afterwards.
pub fn a_teleport_that_changes_the_ground_frees_the_body() {
    use dereth_client::character::GroundEdges;
    use dereth_client_runtime::actions::movement::action;

    let stop = world_support::a_recorded_stop_with_an_action();
    let (mut c, mut mc, mut input) = world_support::running_body();
    let only_the_settle = c.ground_edges() == GroundEdges { hit: 1, left: 0 };

    // The premise: a shard-authored action the body cannot play, holding the ledger. Without it
    // this would be asserting an empty ledger that was never full.
    world_support::server_takes_control(&c, &mut mc, &mut input, &stop);
    world_support::key(&mut c, &mut mc, &mut input, action::MOVE_FORWARD, true);
    let stuck = world_support::drive(&mut c, &mut mc, &mut input, 30, 30);
    let rooted = world_support::over(&stuck, 0, 30) < world_support::STOPPED;
    let jammed = c.driver().movement.interp.pending_motions.len();

    world_support::hop(&mut c, &mut mc, &mut input, 10.0);

    // Two more of each than the settle: the teleport's leaving, the landing's arrival, and a
    // second pair during the fall whose cause is not established here. It is asserted as a
    // measurement rather than explained, and a change in it is worth reading.
    let edges = c.ground_edges() == GroundEdges { hit: 3, left: 2 };
    let drained = {
        let d = c.driver();
        d.movement.interp.pending_motions.is_empty()
            && d.motion_table.pending().is_empty()
            && d.movement.interp.interpreted_state.actions.is_empty()
    };
    let after = world_support::drive(&mut c, &mut mc, &mut input, 150, 30);
    let moves = world_support::over(&after, 0, 30) > world_support::RUNNING;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.teleport.a-teleport-that-changes-the-ground-empties-the-ledger-and-the-body-moves-again",
        move |_| only_the_settle && rooted && jammed > 0 && edges && drained && moves,
    );
}

#[test]
fn scenario_a_teleport_that_changes_the_ground_frees_the_body() {
    scenario("a_teleport_that_changes_the_ground_frees_the_body");
}

/// A teleport that arrives on the same kind of ground it left raises no edge at all and empties
/// nothing -- which is why what frees a stuck body is the edge and not the teleport.
///
/// This is the assertion that fails if a later reader "fixes" a stuck body by emptying the
/// ledger from the teleport directly: that would pass the scenario above and redden this one.
pub fn a_teleport_onto_the_same_ground_raises_no_edge() {
    use dereth_client::character::GroundEdges;
    use dereth_client_runtime::actions::movement::action;
    use dereth_primitives::{Frame, Position, Vec3};

    let stop = world_support::a_recorded_stop_with_an_action();
    let (mut c, mut mc, mut input) = world_support::running_body();
    let only_the_settle = c.ground_edges() == GroundEdges { hit: 1, left: 0 };
    world_support::server_takes_control(&c, &mut mc, &mut input, &stop);
    world_support::key(&mut c, &mut mc, &mut input, action::MOVE_FORWARD, true);
    let _ = world_support::drive(&mut c, &mut mc, &mut input, 30, 30);
    let jammed = c.driver().movement.interp.pending_motions.len();
    let premise = jammed > 0 && c.on_ground();

    // Two metres sideways, on the same open ground: the arrival finds the same floor and the
    // walkable answer is set to the value it already held.
    let here = c.position();
    let to = Vec3 {
        x: here.frame.origin.x + 2.0,
        ..here.frame.origin
    };
    c.teleport(Position::new(
        here.cell,
        Frame::new(to, here.frame.rotation),
    ));
    let moved = (c.position().frame.origin.x - here.frame.origin.x).abs() > 1.0;

    let no_edge = c.ground_edges() == GroundEdges { hit: 1, left: 0 };
    let still_standing = c.on_ground();
    let ledger_untouched = c.driver().movement.interp.pending_motions.len() == jammed;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.teleport.a-teleport-onto-the-same-kind-of-ground-raises-no-edge-and-empties-nothing",
        move |_| {
            only_the_settle && premise && moved && no_edge && still_standing && ledger_untouched
        },
    );
}

#[test]
fn scenario_a_teleport_onto_the_same_ground_raises_no_edge() {
    scenario("a_teleport_onto_the_same_ground_raises_no_edge");
}

// -------------------------------------------------------------------------------------------
// movement.approach.*, movement.follow.*
// -------------------------------------------------------------------------------------------

/// A movement the shard hands the body ends the approach it was already walking, on the spot,
/// before the new one is installed -- and the failure is reported once and never again.
pub fn a_replacement_ends_the_old_approach() {
    use dereth_animation::motion::InterpretedMotionState;
    use dereth_animation::MotionCommand;

    let store = support::store();
    let mut ok = true;
    for command in [MotionCommand::READY, MotionCommand::WALK_FORWARD] {
        let mut b = world_support::Approaching::new(&store);
        b.approach();
        let failures = b.c.stats.move_tos_failed;
        let replacement = InterpretedMotionState {
            forward_command: command,
            forward_speed: 0.7,
            ..Default::default()
        };
        b.c.move_to_interpreted_state(&replacement);
        ok &= !b.c.is_moving_to();
        ok &=
            b.c.driver()
                .movement
                .interp
                .interpreted_state
                .forward_command
                == command;
        ok &= (b.c.driver().movement.interp.interpreted_state.forward_speed - 0.7).abs() < 1e-6;
        // The report is synchronous too, and it happens once.
        ok &= b.c.stats.move_tos_failed == failures + 1;
        b.frames(1);
        ok &= b.c.stats.move_tos_failed == failures + 1;
        ok &=
            b.c.driver()
                .movement
                .interp
                .interpreted_state
                .forward_command
                == command;
        b.c.move_to_interpreted_state(&InterpretedMotionState::default());
        b.frames(1);
        ok &= b.c.stats.move_tos_failed == failures + 1;
        // ...and the body is free afterwards: it walks when told to and stops when let go.
        ok &= b.next_input_and_release();
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.a-replacement-ends-the-old-approach-before-the-new-one-is-installed",
        move |_| ok,
    );
}

#[test]
fn scenario_a_replacement_ends_the_old_approach() {
    scenario("a_replacement_ends_the_old_approach");
}

/// A key the player presses ends an approach on the spot, without waiting for the frame's own
/// retake to notice -- and the body is free afterwards.
pub fn a_key_press_ends_an_approach_on_the_spot() {
    use dereth_animation::MotionCommand;

    let store = support::store();
    let mut b = world_support::Approaching::new(&store);
    b.approach();
    b.c.input.forward = true;
    b.frames(1);
    let ended = !b.c.is_moving_to();
    let walking =
        b.c.driver().movement.interp.raw_state.forward_command == MotionCommand::WALK_FORWARD;
    let free = b.next_input_and_release();

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.a-key-press-ends-an-approach-on-the-spot",
        move |_| ended && walking && free,
    );
}

#[test]
fn scenario_a_key_press_ends_an_approach_on_the_spot() {
    scenario("a_key_press_ends_an_approach_on_the_spot");
}

/// A jump ends an approach whether or not the jump itself is allowed, and a refused jump gives
/// the body no push at all.
pub fn a_jump_ends_an_approach_whether_or_not_it_is_allowed() {
    let store = support::store();
    let mut ok = true;
    for constrained in [false, true] {
        let mut b = world_support::Approaching::new(&store);
        b.approach();
        ok &= b.c.on_ground();
        // The one thing the scenario sets by hand: whether the body is free to leave the ground.
        b.c.driver_mut().env.fully_constrained = constrained;
        let refused = b.c.stats.motions_refused;
        b.c.input.jump = true;
        b.frames(1);
        ok &= !b.c.is_moving_to();
        if constrained {
            ok &= b.c.stats.motions_refused == refused + 1;
            ok &= b.c.on_ground();
        } else {
            ok &= b.c.stats.motions_refused == refused;
            ok &= !b.c.on_ground();
            ok &= b.c.velocity().z > 0.5;
        }
        b.c.driver_mut().env.fully_constrained = false;
        ok &= b.next_input_and_release();
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.a-jump-ends-an-approach-whether-or-not-the-jump-is-allowed",
        move |_| ok,
    );
}

#[test]
fn scenario_a_jump_ends_an_approach_whether_or_not_it_is_allowed() {
    scenario("a_jump_ends_an_approach_whether_or_not_it_is_allowed");
}

/// The clean-up of an approach that has ended never outlives the next approach's own subject:
/// what the body is watching after two approaches in a row is the second one's, not nothing.
///
/// Both ways an approach can be replaced are driven -- by a movement the shard hands over and by
/// a change of the way the body carries itself -- and a follow that is replaced rather than
/// cancelled hands its watch straight to the new one.
pub fn the_old_approachs_clean_up_never_outlives_the_next() {
    use dereth_animation::motion::{InterpretedMotionState, MoveToRequest, MovementParameters};
    use dereth_animation::MotionCommand;
    use dereth_primitives::{ObjectId, Vec3};

    const OLD: ObjectId = ObjectId(0x7000_0001);
    const NEW: ObjectId = ObjectId(0x7000_0002);

    let store = support::store();
    let request = |id| MoveToRequest::MoveToObject {
        object_id: id,
        top_level_id: id,
        radius: 0.5,
        height: 1.0,
    };

    let mut ok = true;
    for style_only in [false, true] {
        let mut b = world_support::Approaching::new(&store);
        b.c.perform_move_to(&request(OLD), &MovementParameters::default(), None);
        ok &= b.c.wanted_target().map(|t| t.id) == Some(OLD);
        if style_only {
            b.c.apply_movement_style(MotionCommand::HAND_COMBAT);
        } else {
            b.c.move_to_interpreted_state(&InterpretedMotionState::default());
        }
        ok &= !b.c.is_moving_to();
        b.c.perform_move_to(&request(NEW), &MovementParameters::default(), None);
        ok &= b.c.wanted_target().map(|t| t.id) == Some(NEW);
        b.frames(1);
        // The old approach's clean-up cannot arrive after the new one has said what it watches.
        ok &= b.c.wanted_target().map(|t| t.id) == Some(NEW);
        ok &= b.c.is_moving_to();
        let mut goal = b.c.position();
        goal.frame.origin.y += 20.0;
        b.c.update_target(goal, Vec3::ZERO, true);
        b.frames(20);
        ok &= b.c.driver().movement.moveto.initialized && b.c.is_moving_to();
        b.c.take_control_from_server();
        ok &= b.next_input_and_release();
    }

    // The same question for a follow that is replaced by an approach: the unstick happens before
    // the new subject is installed, and the old follow's own deadline cannot erase it.
    let mut b = world_support::Approaching::new(&store);
    b.c.stick_to_object(OLD, 0.4, 1.0);
    b.c.update_target(b.c.position(), Vec3::ZERO, true);
    ok &= b.c.driver().movement.sticky.initialized;
    b.c.perform_move_to(
        &MoveToRequest::TurnToObject {
            object_id: NEW,
            top_level_id: NEW,
        },
        &MovementParameters::default(),
        None,
    );
    ok &= b.c.sticky_target().is_none();
    ok &= b.c.wanted_target().map(|t| t.id) == Some(NEW);
    // Past the old follow's own deadline; it cannot erase the new subject.
    b.frames(35);
    ok &= b.c.wanted_target().map(|t| t.id) == Some(NEW) && b.c.is_moving_to();
    b.c.take_control_from_server();
    ok &= b.next_input_and_release();

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.approach.the-old-approachs-clean-up-never-outlives-the-next-approachs-subject",
        move |_| ok,
    );
}

#[test]
fn scenario_the_old_approachs_clean_up_never_outlives_the_next() {
    scenario("the_old_approachs_clean_up_never_outlives_the_next");
}

/// Every way a follow can end reports it once, leaves the right subject behind, and leaves the
/// body free to walk.
///
/// The four ways are the player letting go, a second follow replacing the first, the subject
/// being lost, and the follow's own deadline running out. A follow's *first* call has no
/// previous follow to end, so it must not end the approach that was already running -- which is
/// asserted here as the premise of all four.
pub fn every_way_a_follow_ends_reports_it_once() {
    use dereth_animation::motion::{MoveToRequest, MovementParameters};
    use dereth_primitives::{ObjectId, Vec3};

    const APPROACH: ObjectId = ObjectId(0x7000_0001);
    const STICK: ObjectId = ObjectId(0x7000_0002);
    const REPLACEMENT: ObjectId = ObjectId(0x7000_0003);

    let store = support::store();
    let mut ok = true;
    for trigger in ["explicit", "replacement", "lost subject", "deadline"] {
        let mut b = world_support::Approaching::new(&store);
        b.c.perform_move_to(
            &MoveToRequest::TurnToObject {
                object_id: APPROACH,
                top_level_id: APPROACH,
            },
            &MovementParameters::default(),
            None,
        );
        // A first follow has no previous one, so it must not end the approach.
        b.c.stick_to_object(STICK, 0.4, 1.0);
        ok &= b.c.is_moving_to();
        ok &= b.c.wanted_target().map(|t| t.id) == Some(STICK);

        let failures = b.c.stats.move_tos_failed;
        match trigger {
            "explicit" => b.c.unstick_from_object(),
            "replacement" => b.c.stick_to_object(REPLACEMENT, 0.6, 1.2),
            "lost subject" => b.c.update_target(b.c.position(), Vec3::ZERO, false),
            "deadline" => b.frames(35),
            _ => unreachable!(),
        }
        ok &= !b.c.is_moving_to();
        ok &= b.c.stats.move_tos_failed == failures + 1;
        let expected = (trigger == "replacement").then_some(REPLACEMENT);
        ok &= b.c.sticky_target() == expected;
        ok &= b.c.wanted_target().map(|t| t.id) == expected;
        b.frames(1);
        // Once, and not again on the following frame.
        ok &= b.c.stats.move_tos_failed == failures + 1;
        ok &= b.c.wanted_target().map(|t| t.id) == expected;
        b.c.unstick_from_object();
        ok &= b.next_input_and_release();
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.follow.every-way-a-follow-ends-reports-it-once-and-leaves-the-body-free",
        move |_| ok,
    );
}

#[test]
fn scenario_every_way_a_follow_ends_reports_it_once() {
    scenario("every_way_a_follow_ends_reports_it_once");
}

// -------------------------------------------------------------------------------------------
// movement.teleport.* (the shard's own teleports)
// -------------------------------------------------------------------------------------------

/// Every teleport the recordings carry is applied to the player's own body, and no other
/// position message is.
///
/// The ratio is the point: a build that moved the body on every position the shard sent would
/// fight the local simulation hundreds of times over and still satisfy a measurement that only
/// looked for the teleports. Both numbers are read off the recordings at run time.
pub fn every_recorded_teleport_is_applied_and_no_other_position_is() {
    let mut positions = 0u64;
    let mut teleports = 0usize;
    let mut with_teleports = 0usize;
    let mut without = 0usize;
    for name in dereth_client_net::client_session::testing::session_names() {
        let r = world_support::replay_teleports(name, None, None);
        positions += r.player_positions;
        teleports += r.teleports.len();
        if r.teleports.is_empty() {
            without += 1;
        } else {
            with_teleports += 1;
        }
    }
    println!(
        "teleport lands: {teleports} teleports out of {positions} player position messages, over \
         {with_teleports} recordings that carry one and {without} that do not"
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.teleport.every-teleport-the-recordings-carry-is-applied-and-no-other-position-is",
        move |_| {
            // Some teleports, in more than one recording, and far fewer of them than there are
            // position messages -- so what moves the body is the edge and not the traffic.
            teleports > 0
                && with_teleports > 1
                && without > 0
                && positions > 100
                && (teleports as u64) * 20 < positions
        },
    );
}

#[test]
fn scenario_every_recorded_teleport_is_applied_and_no_other_position_is() {
    scenario("every_recorded_teleport_is_applied_and_no_other_position_is");
}

/// A recorded teleport moves the player's own body, and everything the client says about where
/// it is afterwards names the destination.
///
/// The three moments the report asks for: the arrival, the portal space the client stands still
/// through, and the walk away from where it landed -- which is what makes the second report, the
/// one that would have carried a stale position, exist at all.
pub fn a_recorded_teleport_moves_the_body_and_the_reports_follow() {
    use dereth_client::app::{body_motion, player_timestamps};
    use dereth_primitives::LocalTime;

    let store = support::store();
    let (session, _) = world_support::a_recording_with_a_teleport();
    let r = world_support::replay_teleports(session, Some(&store), None);
    let mut character = r
        .body
        .expect("the recording says where the player is, so a body was built");
    let start = r
        .first_position
        .expect("a body was built, so it was built somewhere");
    let destination = r
        .teleports
        .first()
        .copied()
        .expect("the recording carries a teleport");

    // The setup was alive enough to fail: the body started somewhere else entirely, so what
    // follows is a teleport and not a body that was already there.
    let really_moved = start.cell.0 >> 16 != destination.pos.cell.0 >> 16;
    // ...and the last teleport applied is where the body is.
    let last = *r.teleports.last().expect("at least one");
    let arrived = character.position().cell.0 == last.pos.cell.0
        && (character.position().frame.origin.x - last.pos.frame.origin.x).abs() < 1.0
        && (character.position().frame.origin.y - last.pos.frame.origin.y).abs() < 1.0;

    // The reporter is the production one, driven the way the frame drives it and off the same
    // body -- so a build that moved only the drawn body would fail here and pass above.
    let (mut reporter, mut session_out) = world_support::reporter();
    let stamps = player_timestamps(None);
    let block = last.pos.cell.0 >> 16;
    let mut cells: Vec<u32> = Vec::new();
    let mut after_three_seconds = 0usize;
    let mut stayed_in_the_block = true;
    let mut settled = true;
    for i in 1..=150 {
        let now = f64::from(i) / 30.0;
        // Two seconds of portal space -- the client standing still while the shard's answer
        // settles -- and then he walks away from where he landed.
        character.input.forward = now >= 2.0;
        character.update(LocalTime(now));
        let before = session_out.transport.sent.len();
        reporter.use_time(now, &body_motion(&character, stamps), &mut session_out);
        for b in &session_out.transport.sent[before..] {
            cells.push(world_support::reported_cell(&b.payload));
            if now >= 3.0 {
                after_three_seconds += 1;
            }
        }
        stayed_in_the_block &= character.position().cell.0 >> 16 == block;
        if i == 60 {
            // The end of portal space, stated rather than assumed: a body that had fallen
            // through a destination nothing loaded would satisfy every assertion here by
            // accident.
            settled = character.on_ground()
                && (character.position().frame.origin.z - last.pos.frame.origin.z).abs() < 5.0;
        }
    }
    let stale = cells.iter().any(|c| *c == start.cell.0);
    let wrong_block = cells.iter().any(|c| c >> 16 != block);
    println!(
        "teleport lands: {} position reports over five seconds after the teleport, {} of them more \
         than three seconds after it",
        cells.len(),
        after_three_seconds
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.teleport.a-recorded-teleport-moves-the-body-and-the-reports-name-the-destination",
        move |_| {
            really_moved
                && arrived
                && stayed_in_the_block
                && settled
                && cells.len() >= 2
                && after_three_seconds >= 1
                && !stale
                && !wrong_block
        },
    );
}

#[test]
fn scenario_a_recorded_teleport_moves_the_body_and_the_reports_follow() {
    scenario("a_recorded_teleport_moves_the_body_and_the_reports_follow");
}

/// Several teleports in a row each move the body, not only the first.
///
/// The recording's own destinations, in the recording's own order, so a build that applied one
/// of them repeatedly fails here and passes the scenario above.
pub fn several_teleports_in_a_row_each_move_the_body() {
    use dereth_primitives::LocalTime;

    let store = support::store();
    let session = world_support::the_recording_with_the_most_teleports();
    let r = world_support::replay_teleports(session, Some(&store), None);
    let cells: Vec<u32> = r.teleports.iter().map(|t| t.pos.cell.0).collect();
    let several = cells.len() >= 3;
    let mut character = r.body.expect("a body was built");
    let last = *r.teleports.last().expect("at least one teleport");
    let at_the_last = character.position().cell.0 == last.pos.cell.0;
    // Not all the same destination, or "each of them moved the body" is one teleport four times.
    let distinct = cells
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        >= 2;

    for i in 1..=60 {
        character.update(LocalTime(f64::from(i) / 30.0));
    }
    let on_the_ground = character.on_ground();
    println!("teleport lands: {session} teleports to {cells:#010X?}, the body settles at the last");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.teleport.several-teleports-in-a-row-each-move-the-body",
        move |_| several && distinct && at_the_last && on_the_ground,
    );
}

#[test]
fn scenario_several_teleports_in_a_row_each_move_the_body() {
    scenario("several_teleports_in_a_row_each_move_the_body");
}

/// A teleport into a room lands the body on that room's floor, even though nothing has loaded
/// the room's geometry yet.
pub fn a_teleport_into_a_room_lands_the_body_on_its_floor() {
    use dereth_primitives::LocalTime;

    let store = support::store();
    let (session, nth) = world_support::a_recorded_teleport_into_a_room();
    let r = world_support::replay_teleports(session, Some(&store), Some(nth));
    let target = *r
        .teleports
        .last()
        .expect("the replay stopped after a teleport");
    let indoors = target.pos.cell.0 & 0xFFFF >= 0x100;
    let mut character = r.body.expect("a body was built");
    let there = character.position().cell.0 == target.pos.cell.0;

    for i in 1..=60 {
        character.update(LocalTime(f64::from(i) / 30.0));
    }
    let on_the_floor = character.on_ground();
    let still_there = character.position().cell.0 >> 16 == target.pos.cell.0 >> 16;
    println!(
        "teleport lands: {session}'s teleport {nth} into room {:#010X} settles at {:?}",
        target.pos.cell.0,
        character.position().frame.origin
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.teleport.a-teleport-into-a-room-lands-the-body-on-its-floor",
        move |_| indoors && there && on_the_floor && still_there,
    );
}

#[test]
fn scenario_a_teleport_into_a_room_lands_the_body_on_its_floor() {
    scenario("a_teleport_into_a_room_lands_the_body_on_its_floor");
}

/// Every drawn frame applies a teleport before it reports a position, and a client with nothing
/// to teleport teleports nothing.
///
/// The order between the two is load-bearing: a position reported before the teleport was
/// applied names the room the player has just left.
pub fn every_drawn_frame_applies_a_teleport_before_reporting() {
    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.tick(3);

    c.assert_behaviour(
        "movement.teleport.every-drawn-frame-applies-a-teleport-before-it-reports-a-position",
        move |v| {
            let app = v.expect_app();
            app.player_teleport_use_times() == 3
                && app.player_teleport_use_times() == app.frames_drawn()
                && app.position_use_times() == app.frames_drawn()
                // With no shard and no body there is nothing to teleport, and nothing may have
                // been invented.
                && app.player_teleports_applied() == 0
                && app.player_teleports_before_a_body() == 0
        },
    );
    c.shutdown();
}

#[test]
fn scenario_every_drawn_frame_applies_a_teleport_before_reporting() {
    scenario("every_drawn_frame_applies_a_teleport_before_reporting");
}

// -------------------------------------------------------------------------------------------
// movement.contact.*, movement.motion-ledger.*, movement.jump-charge.*, movement.placement.*,
// movement.door.*
// -------------------------------------------------------------------------------------------

/// A full motion ledger stops turning and walking alike, and a fresh key press gets out of it.
///
/// A jammed ledger is not the shape of "he can turn but not walk", because the frame's own retake
/// reads it before it looks at any key at all.
pub fn a_full_ledger_stops_turning_and_walking_alike() {
    use dereth_animation::MotionCommand;

    let mut r = world_support::DoorRig::new();
    // A shard-authored action animation is the longest thing that sits on the ledger.
    r.server_action(MotionCommand::CHEER);
    r.run(2, Some(true));
    let loaded = r.c.driver().movement.motions_pending() && r.mc.lists.controlled_by_server;

    // The retake reads the ledger before it looks at any list, so it refuses for a held walk and
    // a held turn alike: the gate is the same one.
    let mut held = r.input;
    held.forward = true;
    let no_retake_for_a_walk = !r.mc.use_time(true, false, &mut held);
    let mut turning = r.input;
    turning.turn_right = true;
    let no_retake_for_a_turn = !r.mc.use_time(true, false, &mut turning);

    // And the press that does not wait for it.
    let turned = r.probe_turn(45);
    let moved = r.probe_translate_either_way(60);
    let escaped = turned > 10.0 && moved > 0.25 && !r.mc.lists.controlled_by_server;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.motion-ledger.gates-turning-and-walking-alike-and-a-fresh-key-press-escapes-it",
        move |_| loaded && no_retake_for_a_walk && no_retake_for_a_turn && escaped,
    );
}

#[test]
fn scenario_a_full_ledger_stops_turning_and_walking_alike() {
    scenario("a_full_ledger_stops_turning_and_walking_alike");
}

/// A body holding a charged jump turns and does not walk, and only leaving the ground frees it.
///
/// This is the one state of the client's own that really is "he can turn but he cannot walk",
/// and every way out of it is measured rather than guessed: pressing escape does not clear it,
/// emptying the command lists does not clear it, and the three things that do are releasing the
/// jump, any movement the shard sends, and a teleport whose arrival changes the footing.
pub fn a_charged_jump_turns_but_does_not_walk() {
    use dereth_primitives::{Frame, Position, Vec3};

    let charged = |r: &mut world_support::DoorRig| {
        assert_eq!(
            r.c.charge_jump(),
            0,
            "a settled, still body may charge a jump"
        );
        assert!(r.c.driver().movement.interp.standing_longjump);
        r.run(10, Some(true));
    };

    // (a) The state itself.
    let mut r = world_support::DoorRig::new();
    charged(&mut r);
    let turns = r.probe_turn(45) > 10.0;
    let walks_nowhere = r.probe_translate_either_way(60) < 0.05;
    // On the ground the whole time and with nothing on the ledger, so this is not the other
    // shape that looks like it.
    let clean = r.c.on_ground() && !r.c.driver().movement.motions_pending();

    // (b) Pressing escape does not clear it, and (c) nor does emptying the command lists.
    r.c.stop_completely_from_action();
    r.run(10, Some(true));
    let escape_does_not_help =
        r.c.driver().movement.interp.standing_longjump && r.probe_translate_either_way(60) < 0.05;
    {
        let world_support::DoorRig { mc, input, .. } = &mut r;
        mc.clear_all_commands(input);
    }
    r.c.input = r.input;
    r.run(10, Some(true));
    let lists_do_not_help =
        r.c.driver().movement.interp.standing_longjump && r.probe_translate_either_way(60) < 0.05;

    // (d) Releasing the jump does.
    let edges = r.c.ground_edges();
    let launched = r.c.jump_with_extent(0.5) == 0;
    r.run(90, Some(true));
    let release_frees_him = launched
        && r.c.ground_edges().left > edges.left
        && !r.c.driver().movement.interp.standing_longjump
        && r.c.on_ground()
        && r.probe_translate_either_way(60) > 0.25;

    // (e) So does any movement the shard sends -- the door's own answer included.
    let mut r = world_support::DoorRig::new();
    charged(&mut r);
    r.door_turn();
    let the_shard_frees_him = !r.c.driver().movement.interp.standing_longjump && {
        r.run(120, Some(true));
        r.probe_translate_either_way(60) > 0.25
    };

    // (f) And a teleport, because its arrival changes the footing too.
    let mut r = world_support::DoorRig::new();
    charged(&mut r);
    let here = r.c.position();
    r.c.teleport(Position::new(
        here.cell,
        Frame::new(
            Vec3::new(
                here.frame.origin.x,
                here.frame.origin.y,
                here.frame.origin.z + 1.5,
            ),
            here.frame.rotation,
        ),
    ));
    r.run(120, Some(true));
    let a_teleport_frees_him = r.c.on_ground()
        && !r.c.driver().movement.interp.standing_longjump
        && r.probe_translate_either_way(60) > 0.25;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.jump-charge.a-charged-jump-turns-but-does-not-walk-until-the-body-leaves-the-ground",
        move |_| {
            turns
                && walks_nowhere
                && clean
                && escape_does_not_help
                && lists_do_not_help
                && release_frees_him
                && the_shard_frees_him
                && a_teleport_frees_him
        },
    );
}

#[test]
fn scenario_a_charged_jump_turns_but_does_not_walk() {
    scenario("a_charged_jump_turns_but_does_not_walk");
}

/// A body put down inside a building is really put down there, and the change of footing that
/// implies is raised -- which is the only thing that empties a full ledger.
///
/// The outdoor arm is the control: it always worked, and it is what makes the indoor one a
/// measurement rather than a tautology.
pub fn a_placement_inside_a_building_takes_and_raises_its_footing_change() {
    use dereth_animation::MotionCommand;
    use dereth_primitives::{Frame, LocalTime, Position, Vec3};

    let store = support::store();

    // 1. The placement itself, outdoors and indoors, at a spot the body demonstrably stands on.
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let mut out = dereth_client::character::Character::new(
        &store,
        &region,
        dereth_client::world::DEFAULT_LANDBLOCK,
        (96.0, 96.0),
    )
    .expect("the outdoor body is created");
    for i in 1..=60 {
        out.update(LocalTime(f64::from(i) / 30.0));
    }
    let base = out.position();
    let (oc, ou) = (
        out.stats.teleports_committed,
        out.stats.teleports_uncommitted,
    );
    out.teleport(base);
    let outdoors_takes = (
        out.stats.teleports_committed - oc,
        out.stats.teleports_uncommitted - ou,
    ) == (1, 0);

    let mut r = world_support::DoorRig::new();
    let indoors_takes = r.c.on_ground()
        && (
            r.c.stats.teleports_committed,
            r.c.stats.teleports_uncommitted,
        ) == (1, 0);
    let settled_here = r.c.position();
    r.c.teleport(settled_here);
    let again = (
        r.c.stats.teleports_committed,
        r.c.stats.teleports_uncommitted,
    ) == (2, 0);

    // 2. The change of footing. A ledger the client cannot empty any other way, then an arrival
    //    off the floor.
    let mut r = world_support::DoorRig::new();
    r.server_action(MotionCommand::CHEER);
    r.run(2, Some(true));
    let loaded = r.c.driver().movement.motions_pending();
    let edges = r.c.ground_edges();
    let here = r.c.position();
    r.c.teleport(Position::new(
        here.cell,
        Frame::new(
            Vec3::new(
                here.frame.origin.x,
                here.frame.origin.y,
                here.frame.origin.z + 1.5,
            ),
            here.frame.rotation,
        ),
    ));
    let raised = r.c.stats.teleports_uncommitted == 0
        && r.c.ground_edges().left > edges.left
        && !r.c.on_ground();

    // 3. The consequence: he falls, lands, and the body answers a held key again.
    r.run(120, Some(true));
    let fell = r.c.on_ground()
        && r.c.position().frame.origin.z < here.frame.origin.z + 0.75
        && r.c.ground_edges().hit > edges.hit;
    let emptied = !r.c.driver().movement.motions_pending();
    let walks = r.probe_translate_either_way(60) > 0.25;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.placement.a-placement-inside-a-building-takes-and-raises-the-change-of-footing-it-implies",
        move |_| {
            outdoors_takes
                && indoors_takes
                && again
                && loaded
                && raised
                && fell
                && emptied
                && walks
        },
    );
}

#[test]
fn scenario_a_placement_inside_a_building_takes_and_raises_its_footing_change() {
    scenario("a_placement_inside_a_building_takes_and_raises_its_footing_change");
}

/// A body that loses its footing with no change of footing to announce it turns for ever and
/// never walks -- and a placement that takes is the one way out an ordinary session offers.
///
/// This is the shape of the report stated as a mechanism: nothing recomputes the footing except
/// a placement that takes, nothing makes the body fall while it is still touching something,
/// and turning never asks the question at all. Together they are a latch.
pub fn a_body_that_loses_its_footing_with_no_edge_turns_for_ever() {
    let mut r = world_support::DoorRig::new();
    let standing = r.c.on_ground();
    let before = r.c.position();
    let premise = {
        let o = r.c.world.get_mut(r.c.handle).expect("the body is live");
        let was = o.transient_state.in_contact() && o.transient_state.on_walkable();
        // The state a placement that never took leaves behind: the footing of somewhere else,
        // and no announcement, because nothing was asked to recompute it.
        o.transient_state.set_on_walkable_bit(false);
        was
    };
    let edges = r.c.ground_edges();
    r.run(120, Some(true));
    let latched = r.c.ground_edges() == edges
        && !r.c.on_ground()
        && dereth_animation::motion::moveto::distance(&before, &r.c.position()) < 0.05;

    let turns = r.probe_turn(45) > 10.0;
    let walks_nowhere = r.probe_translate_either_way(60) < 0.05;

    // And the one way out an ordinary session offers: a placement that takes.
    let here = r.c.position();
    r.c.teleport(here);
    let freed = r.c.stats.teleports_uncommitted == 0
        && r.c.ground_edges().hit > edges.hit
        && r.c.on_ground()
        && r.probe_translate_either_way(60) > 0.25;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.contact.a-body-that-loses-its-footing-with-nothing-to-announce-it-turns-for-ever",
        move |_| standing && premise && latched && turns && walks_nowhere && freed,
    );
}

#[test]
fn scenario_a_body_that_loses_its_footing_with_no_edge_turns_for_ever() {
    scenario("a_body_that_loses_its_footing_with_no_edge_turns_for_ever");
}

/// No way of opening a door leaves a body that turns but cannot walk.
///
/// Twenty-seven journeys through the same door: the recorded answer the shard sends for a door with
/// seventeen different things happening around it, four approaches and openings in a row, and six
/// journeys with the real door standing in the player's way as a body he can collide with. Every
/// one of them ends with the same two probes, and a journey where they disagree -- he turns and he
/// does not walk -- is the defect reproduced.
pub fn no_door_journey_leaves_a_body_that_turns_but_cannot_walk() {
    use dereth_animation::MotionCommand;
    use dereth_client_runtime::actions::movement::action;

    type Verdict = (&'static str, f32, f32);
    let mut verdicts: Vec<Verdict> = Vec::new();

    // --- seventeen prefixes around the recorded answer ------------------------------------
    type Prefix = (&'static str, fn(&mut world_support::DoorRig));
    let prefixes: Vec<Prefix> = vec![
        (
            "A standing, the door answered",
            |r: &mut world_support::DoorRig| {
                r.door_turn();
                r.run(120, Some(true));
            },
        ),
        (
            "B forward held across it",
            |r: &mut world_support::DoorRig| {
                r.key(action::MOVE_FORWARD, true);
                r.run(20, Some(true));
                r.door_turn();
                r.run(120, Some(true));
            },
        ),
        (
            "C forward held, released while the shard has control",
            |r: &mut world_support::DoorRig| {
                r.key(action::MOVE_FORWARD, true);
                r.run(20, Some(true));
                r.door_turn();
                r.run(10, Some(true));
                r.key(action::MOVE_FORWARD, false);
                r.run(110, Some(true));
            },
        ),
        (
            "D autorun on across it",
            |r: &mut world_support::DoorRig| {
                r.key(action::AUTORUN, true);
                r.run(20, Some(true));
                r.door_turn();
                r.run(120, Some(true));
            },
        ),
        (
            "E nothing ever says where the door is",
            |r: &mut world_support::DoorRig| {
                r.door_turn();
                r.run(120, None);
            },
        ),
        (
            "F the door is said to be gone",
            |r: &mut world_support::DoorRig| {
                r.door_turn();
                r.run(120, Some(false));
            },
        ),
        (
            "G forward held and nothing says where the door is",
            |r: &mut world_support::DoorRig| {
                r.key(action::MOVE_FORWARD, true);
                r.run(20, Some(true));
                r.door_turn();
                r.run(120, None);
            },
        ),
        (
            "H a jump charged first",
            |r: &mut world_support::DoorRig| {
                assert_eq!(r.c.charge_jump(), 0, "a standing body may charge");
                r.run(20, Some(true));
                r.door_turn();
                r.run(120, Some(true));
            },
        ),
        (
            "I a jump charged, forward held, released under control",
            |r: &mut world_support::DoorRig| {
                let _ = r.c.charge_jump();
                r.key(action::MOVE_FORWARD, true);
                r.run(20, Some(true));
                r.door_turn();
                r.run(10, Some(true));
                r.key(action::MOVE_FORWARD, false);
                r.run(110, Some(true));
            },
        ),
        (
            "J a shard-authored animation across it",
            |r: &mut world_support::DoorRig| {
                r.server_action(MotionCommand::CHEER);
                r.run(5, Some(true));
                r.door_turn();
                r.run(120, Some(true));
            },
        ),
        ("K the door used twice", |r: &mut world_support::DoorRig| {
            r.door_turn();
            r.run(20, Some(true));
            r.door_turn();
            r.run(120, Some(true));
        }),
        (
            "L the door used while in the air",
            |r: &mut world_support::DoorRig| {
                assert_eq!(r.c.jump_with_extent(1.0), 0, "a standing body may jump");
                r.run(4, Some(true));
                assert!(!r.c.on_ground(), "the answer lands while he is in the air");
                r.door_turn();
                r.run(150, Some(true));
            },
        ),
        (
            "M a turn key held across it",
            |r: &mut world_support::DoorRig| {
                r.key(action::TURN_RIGHT, true);
                r.run(20, Some(true));
                r.door_turn();
                r.run(60, Some(true));
                r.key(action::TURN_RIGHT, false);
                r.run(60, Some(true));
            },
        ),
        (
            "N forward and a turn held across it",
            |r: &mut world_support::DoorRig| {
                r.key(action::MOVE_FORWARD, true);
                r.key(action::TURN_RIGHT, true);
                r.run(20, Some(true));
                r.door_turn();
                r.run(60, Some(true));
                r.key(action::TURN_RIGHT, false);
                r.key(action::MOVE_FORWARD, false);
                r.run(60, Some(true));
            },
        ),
        (
            "O an animation, forward held, released under control",
            |r: &mut world_support::DoorRig| {
                r.key(action::MOVE_FORWARD, true);
                r.run(20, Some(true));
                r.server_action(MotionCommand::CHEER);
                r.door_turn();
                r.run(10, Some(true));
                r.key(action::MOVE_FORWARD, false);
                r.run(150, Some(true));
            },
        ),
        (
            "P a jump charged and released while under control",
            |r: &mut world_support::DoorRig| {
                r.door_turn();
                r.run(10, Some(true));
                let _ = r.c.charge_jump();
                r.run(10, Some(true));
                let _ = r.c.jump_with_extent(0.5);
                r.run(120, Some(true));
            },
        ),
        (
            "Q a jump charged, the door used, then the release",
            |r: &mut world_support::DoorRig| {
                let _ = r.c.charge_jump();
                r.run(10, Some(true));
                r.door_turn();
                r.run(10, Some(true));
                r.c.finish_jump();
                r.run(120, Some(true));
            },
        ),
    ];
    for (label, prefix) in prefixes {
        let mut r = world_support::DoorRig::new();
        prefix(&mut r);
        let turned = r.probe_turn(45);
        let moved = r.probe_translate(60);
        println!("door {label}: turned {turned:.1} deg, walked {moved:.3} m");
        verdicts.push((label, turned, moved));
    }

    // --- six journeys with the door standing there as a body -------------------------------
    type WithDoor = (
        &'static str,
        fn(&mut world_support::DoorRig, dereth_physics::PhysHandle),
    );
    let with_door: Vec<WithDoor> = vec![
        (
            "a1 open door, walk into the doorway, stay",
            |r: &mut world_support::DoorRig, h| {
                assert!(r
                    .c
                    .world
                    .get(h)
                    .expect("the door is live")
                    .state
                    .is_ethereal());
                r.walk_into_the_doorway();
            },
        ),
        (
            "a2 into the doorway, then it closes on him",
            |r: &mut world_support::DoorRig, h| {
                r.walk_into_the_doorway();
                let _ = r.c.world.set_ethereal(h, false, false);
                r.run(60, Some(true));
            },
        ),
        (
            "a3 into the doorway, close, reopen",
            |r: &mut world_support::DoorRig, h| {
                r.walk_into_the_doorway();
                let _ = r.c.world.set_ethereal(h, false, false);
                r.run(30, Some(true));
                let _ = r.c.world.set_ethereal(h, true, false);
                r.run(30, Some(true));
            },
        ),
        (
            "a4 closed door, walk at it, then it opens",
            |r: &mut world_support::DoorRig, h| {
                let _ = r.c.world.set_ethereal(h, false, false);
                r.walk_into_the_doorway();
                let _ = r.c.world.set_ethereal(h, true, false);
                r.run(30, Some(true));
            },
        ),
        (
            "a5 the recorded journey with the door there",
            |r: &mut world_support::DoorRig, _h| {
                r.door_turn();
                r.run(120, Some(true));
                r.walk_into_the_doorway();
            },
        ),
        (
            "a6 used, doorway, then it closes on him",
            |r: &mut world_support::DoorRig, h| {
                r.door_turn();
                r.run(120, Some(true));
                r.walk_into_the_doorway();
                let _ = r.c.world.set_ethereal(h, false, false);
                r.run(60, Some(true));
            },
        ),
    ];
    for (label, prefix) in with_door {
        let mut r = world_support::DoorRig::new();
        let door = r.door_pos;
        let h = r.register_door(door);
        prefix(&mut r, h);
        let turned = r.probe_turn(45);
        let moved = r.probe_translate_either_way(60);
        println!("door {label}: turned {turned:.1} deg, walked {moved:.3} m");
        verdicts.push((label, turned, moved));
    }

    // --- four approaches and openings in a row ---------------------------------------------
    let mut r = world_support::DoorRig::new();
    let door_at = r.door_pos;
    let h = r.register_door(door_at);
    let radius = r.c.world.get(h).expect("the door is live").radius();
    // Far enough back that each approach is a real walk: the recorded standing spot is already
    // inside the distance the walk arrives at.
    r.key(action::MOVE_FORWARD, true);
    r.run(75, Some(true));
    r.key(action::MOVE_FORWARD, false);
    r.run(20, Some(true));
    let start = r.c.position();
    let mut cycles = 0usize;
    for _ in 1..=4 {
        r.c.teleport(start);
        r.run(30, Some(true));
        r.door_approach(door_at, radius);
        r.door_turn();
        r.run(60, Some(true));
        let _ = r.c.world.set_ethereal(h, true, false);
        r.run(30, Some(true));
        let turned = r.probe_turn(45);
        let moved = r.probe_translate_either_way(60);
        cycles += 1;
        println!("door cycle {cycles}: turned {turned:.1} deg, walked {moved:.3} m");
        verdicts.push(("approach and open", turned, moved));
    }

    let stuck: Vec<&Verdict> = verdicts
        .iter()
        .filter(|(_, t, m)| *t > 10.0 && *m < 0.25)
        .collect();
    println!(
        "door: {} journeys, {} of them stuck",
        verdicts.len(),
        stuck.len()
    );
    let journeys = verdicts.len();
    let none_stuck = stuck.is_empty();

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.door.no-way-of-opening-a-door-leaves-a-body-that-turns-but-cannot-walk",
        move |_| journeys == 27 && cycles == 4 && none_stuck,
    );
}

#[test]
fn scenario_no_door_journey_leaves_a_body_that_turns_but_cannot_walk() {
    scenario("no_door_journey_leaves_a_body_that_turns_but_cannot_walk");
}

// -------------------------------------------------------------------------------------------
// map.window.*
// -------------------------------------------------------------------------------------------

/// Outdoors, the map window shows the date, the coordinates and the player's own mark, placed
/// where the shipped layout says to put it.
///
/// The premise comes first and is part of the claim: the five elements the window is made of
/// are in the shipped layout and the marker area it scales into is a real box. Every one of the
/// window's writes is guarded on its element being there, so a missing element would be a
/// silent nothing rather than a failure.
pub fn outdoors_the_map_shows_the_date_the_coordinates_and_the_mark() {
    use dereth_ui_screens::mapradar::map::{child, MarkerArea};

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);
    let root = *g.roots().first().expect("the gameplay screen has a root");
    let all_five = [
        child::DATE_TIME_TEXT,
        child::MAP_IMAGE,
        child::PLAYER_LOCATION_ICON,
        child::HOUSE_LOCATION_ICON,
        child::COORDINATE_TEXT,
    ]
    .into_iter()
    .all(|id| ui.get_child_recursive(root, id).is_some());
    let all_cached = g.map.date_time_text.is_some()
        && g.map.coordinate_text.is_some()
        && g.map.player_icon.is_some()
        && g.map.house_icon.is_some()
        && g.map.map_image.is_some();
    let area = g.map.marker_area;
    let real_area = area != MarkerArea::default() && area.x1 > area.x0 && area.y1 > area.y0;

    let view = world_support::MapView::outdoors();
    let before = world_support::sample_map(&mut ui, g);
    // The return value is checked after the tree, deliberately: the observable is the three
    // elements, and a build that reports a write it did not make must fail on them and not on
    // its own bookkeeping.
    let reported = g.update_map(&mut ui, &view);
    let after = world_support::sample_map(&mut ui, g);

    let date = !after.date_text.is_empty()
        && after.date_text == world_support::MapView::EXPECTED_DATE_LINE;
    let coords = after.coord_text == world_support::MapView::EXPECTED_COORD_LINE;
    let shown = after.icon_visible;

    // The size of a box is one more than the difference of its edges, which is the client's own
    // arithmetic and was once one short in both the window and the measurement of it.
    let w = before.icon_box.2 - before.icon_box.0 + 1;
    let h = before.icon_box.3 - before.icon_box.1 + 1;
    let (want_x, want_y) = dereth_ui_screens::mapradar::map::place_marker_on_map(
        area,
        world_support::MapView::EAST,
        world_support::MapView::NORTH,
        (w, h),
    );
    let placed = (after.icon_box.0, after.icon_box.1) == (want_x, want_y);
    let inside = (area.x0..=area.x1).contains(&(after.icon_box.0 + w / 2))
        && (area.y0..=area.y1).contains(&(after.icon_box.1 + h / 2));
    // Moving the mark moves it; it does not resize it.
    let same_size = (
        after.icon_box.2 - after.icon_box.0,
        after.icon_box.3 - after.icon_box.1,
    ) == (w - 1, h - 1);
    // The house mark has no valid place, so it stays hidden.
    let no_house = !after.house_visible;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.outdoors-it-shows-the-date-the-coordinates-and-the-players-own-mark",
        move |_| {
            all_five
                && all_cached
                && real_area
                && reported
                && date
                && coords
                && shown
                && placed
                && inside
                && same_size
                && no_house
        },
    );
}

#[test]
fn scenario_outdoors_the_map_shows_the_date_the_coordinates_and_the_mark() {
    scenario("outdoors_the_map_shows_the_date_the_coordinates_and_the_mark");
}

/// A second look at a world that has not changed writes nothing at all.
///
/// This is what makes every other difference in this section attributable: the window's own
/// guards are "write it only if it is not already what it should be", so a build that rewrote
/// unconditionally would make "the text changed" mean nothing.
pub fn a_second_look_at_an_unchanged_world_writes_nothing() {
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);
    let view = world_support::MapView::outdoors();

    let first_wrote = g.update_map(&mut ui, &view);
    let first = world_support::sample_map(&mut ui, g);
    let second_wrote = g.update_map(&mut ui, &view);
    let second = world_support::sample_map(&mut ui, g);
    // And a third, to catch anything that alternates.
    let third_wrote = g.update_map(&mut ui, &view);
    let third = world_support::sample_map(&mut ui, g);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.a-second-look-at-an-unchanged-world-writes-nothing",
        move |_| first_wrote && !second_wrote && !third_wrote && first == second && first == third,
    );
}

#[test]
fn scenario_a_second_look_at_an_unchanged_world_writes_nothing() {
    scenario("a_second_look_at_an_unchanged_world_writes_nothing");
}

/// The player's mark follows him, with east to the right and north up.
pub fn the_players_mark_follows_him_east_right_and_north_up() {
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);

    let at = |g: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
              ui: &mut dereth_ui::UiSystem,
              ns: f32,
              ew: f32| {
        let view = world_support::MapView {
            outside: true,
            coords: Some((ns, ew)),
            date_time: None,
        };
        g.update_map(ui, &view);
        let b = g
            .map
            .player_icon
            .and_then(|h| ui.node(h))
            .expect("the mark")
            .region
            .box_;
        (b.x0, b.y0)
    };
    let centre = at(g, &mut ui, 0.0, 0.0);
    let east = at(g, &mut ui, 0.0, 40.0);
    let west = at(g, &mut ui, 0.0, -40.0);
    let north = at(g, &mut ui, 40.0, 0.0);
    let south = at(g, &mut ui, -40.0, 0.0);

    let signs = east.0 > centre.0 && west.0 < centre.0 && north.1 < centre.1 && south.1 > centre.1;
    // East and west move it sideways only; north and south move it up and down only.
    let axes =
        (east.1, west.1) == (centre.1, centre.1) && (north.0, south.0) == (centre.0, centre.0);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.the-players-own-mark-follows-him-with-east-to-the-right-and-north-up",
        move |_| signs && axes,
    );
}

#[test]
fn scenario_the_players_mark_follows_him_east_right_and_north_up() {
    scenario("the_players_mark_follows_him_east_right_and_north_up");
}

/// Indoors the date keeps going, and only the coordinates and the mark stop.
///
/// "Only outdoors" is two-thirds right, and the third is worth having: the date is written
/// whatever room the player is in, so a player who walks into a dungeon at one time of day does
/// not read that time of day a game-day later.
pub fn indoors_the_date_keeps_going_and_only_the_rest_stops() {
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);

    // Outdoors first, so there is something for the indoor pass to have to clear.
    g.update_map(&mut ui, &world_support::MapView::outdoors());
    let outdoors = world_support::sample_map(&mut ui, g);
    let started_right =
        outdoors.coord_text == world_support::MapView::EXPECTED_COORD_LINE && outdoors.icon_visible;

    // Now into a room, with the clock moved on: a different date entirely.
    let inside = world_support::MapView::indoors_later();
    let reported_inside = g.update_map(&mut ui, &inside);
    let indoors = world_support::sample_map(&mut ui, g);
    let date_kept_going = indoors.date_text == world_support::MapView::EXPECTED_LATER_DATE_LINE
        && indoors.date_text != outdoors.date_text;
    // The coordinates are set to nothing rather than hidden, which is where the map differs from
    // the strip beside the radar.
    let coords_blanked = indoors.coord_text.is_empty();
    let mark_hidden = !indoors.icon_visible;

    // And back outside, everything returns. A one-way repair would pass the half above.
    let out_again = g.update_map(&mut ui, &world_support::MapView::outdoors());
    let again = world_support::sample_map(&mut ui, g);
    let restored = out_again
        && again.coord_text == world_support::MapView::EXPECTED_COORD_LINE
        && again.icon_visible
        && again.icon_box == outdoors.icon_box;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.indoors-the-date-keeps-going-and-only-the-coordinates-and-the-mark-stop",
        move |_| {
            started_right
                && reported_inside
                && date_kept_going
                && coords_blanked
                && mark_hidden
                && restored
        },
    );
}

#[test]
fn scenario_indoors_the_date_keeps_going_and_only_the_rest_stops() {
    scenario("indoors_the_date_keeps_going_and_only_the_rest_stops");
}

/// With no clock yet, the date line keeps its two labels and writes a space after each.
///
/// Worth having because "no clock yet" is the state the window is in between entering the world
/// and the region being unpacked, and the window does not blank the line there.
pub fn with_no_clock_yet_the_date_line_keeps_its_labels() {
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);
    let view = world_support::MapView {
        outside: true,
        coords: Some((42.2, 33.8)),
        date_time: None,
    };
    g.update_map(&mut ui, &view);
    let text = world_support::sample_map(&mut ui, g).date_text;
    let kept = text == "Date:  \nTime:  ";

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.with-no-clock-yet-the-date-line-keeps-its-labels",
        move |_| kept,
    );
}

#[test]
fn scenario_with_no_clock_yet_the_date_line_keeps_its_labels() {
    scenario("with_no_clock_yet_the_date_line_keeps_its_labels");
}

/// The map window is redrawn once every five seconds, and the first look is due at once.
pub fn the_map_is_redrawn_once_every_five_seconds() {
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);
    let due_at_once = g.map_update_due(0.0);

    ui.now = dereth_primitives::LocalTime(100.0);
    let still_due = g.map_update_due(100.0);
    g.update_map(&mut ui, &world_support::MapView::outdoors());
    let not_again = !g.map_update_due(100.0);
    let nearly = !g.map_update_due(104.999);
    let due_again = g.map_update_due(105.0);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.is-redrawn-once-every-five-seconds-and-the-first-look-is-due-at-once",
        move |_| due_at_once && still_due && not_again && nearly && due_again,
    );
}

#[test]
fn scenario_the_map_is_redrawn_once_every_five_seconds() {
    scenario("the_map_is_redrawn_once_every_five_seconds");
}

// -------------------------------------------------------------------------------------------
// map.page.*
// -------------------------------------------------------------------------------------------

/// Every town the shipped table names is on the map, where the table says and with its name.
///
/// The premise is part of the claim, because without it "there are no towns" and "the layout
/// does not say where to put them" would be the same failure: the map element carries the note
/// element, the note layout and the marker box the page reads out of it.
pub fn every_shipped_town_is_on_the_map_where_the_table_says() {
    use dereth_ui_screens::mapradar::map::{child, notes, MapNote, MAP_PAGE, SHIPPED_NOTE_BINDING};

    let (ui, g) = world_support::shipped_map_page();
    let root = g.root().expect("the gameplay root");
    let map = ui
        .get_child_recursive(root, child::MAP_IMAGE)
        .expect("the map element");
    let element = dereth_ui_screens::bind::attr_enum(&ui, map, 0x47).expect("the note element id");
    let layout = dereth_ui_screens::bind::attr_data_id(&ui, map, 0x48).expect("the note layout id");
    let bound = (element, layout.0) == (SHIPPED_NOTE_BINDING.0, SHIPPED_NOTE_BINDING.1)
        && g.map.marker_area == SHIPPED_NOTE_BINDING.2
        && g.map.page == ui.get_child_recursive(root, MAP_PAGE)
        && g.map.page.is_some();

    let got = world_support::map_notes(&ui, &g);
    let want: Vec<world_support::TownNote> = notes(dereth_primitives::EraId::Eor)
        .map(|m: MapNote| world_support::TownNote {
            // A box is inclusive, so a note `w` wide runs from `x` to `x + w - 1`.
            box_: (m.x, m.y, m.x + m.w - 1, m.y + m.h - 1),
            tip: Some(m.name.to_owned()),
        })
        .collect();
    let all_there = got.len() == 53 && got == want;

    // Every town is a child of the map picture, which is what puts its place in the same space
    // the player's own dot and the marker box are expressed in.
    let map_handle = g.map.map_image.expect("the map element");
    let parented = g.map_notes.iter().all(|h| {
        ui.parent(*h) == Some(map_handle)
            && ui.node(*h).map(dereth_ui::ElementNode::element_id)
                == Some(dereth_ui::ElementId(0x1000_01F0))
    });

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.every-town-the-shipped-table-names-is-on-the-map-where-it-says",
        move |_| bound && all_there && parented,
    );
}

#[test]
fn scenario_every_shipped_town_is_on_the_map_where_the_table_says() {
    scenario("every_shipped_town_is_on_the_map_where_the_table_says");
}

/// The page draws the map as one picture, once, over the whole of the map element.
///
/// "Exactly once" is what says the map is a single picture rather than a tiled or multi-level
/// one, which is the other half of there being no zoom.
pub fn the_map_is_one_picture_over_the_whole_element() {
    use dereth_ui_screens::mapradar::map::graphic;

    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);
    let map = g.map.map_image.expect("the map element");
    let shown = world_support::element_is_visible(&ui, Some(map));
    let want = ui.screen_box(map);
    let sized = (want.width(), want.height()) == (257, 267);

    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let mine: Vec<_> = back.calls.iter().filter(|c| c.who == map).collect();
    let once = mine.len() == 1;
    let right_picture = once && mine[0].image == Some(graphic::MAP_IMAGE);
    let covers = once && mine[0].screen == want;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.the-map-is-one-picture-drawn-once-over-the-whole-map-element",
        move |_| shown && sized && once && right_picture && covers,
    );
}

#[test]
fn scenario_the_map_is_one_picture_over_the_whole_element() {
    scenario("the_map_is_one_picture_over_the_whole_element");
}

/// A map the player has just opened highlights no town at all.
pub fn a_freshly_opened_map_highlights_no_town() {
    use dereth_ui_screens::mapradar::map::{graphic, NOTE_REST_STATE};

    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);
    let map_box = ui.screen_box(g.map.map_image.expect("the map element"));

    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);

    let mut ok = !g.map_notes.is_empty();
    let mut edges = 0usize;
    for h in &g.map_notes {
        let b = ui.screen_box(*h);
        ok &= map_box.contains(b.x0, b.y0) && map_box.contains(b.x1, b.y1);
        ok &= ui.node(*h).map(|n| n.state.0) == Some(NOTE_REST_STATE);
        // The one child is the frame around the town; at rest it is hidden.
        ok &= ui
            .children(*h)
            .iter()
            .all(|c| !ui.node(*c).is_some_and(|n| n.region.flags.visible));
        edges += back
            .calls
            .iter()
            .filter(|c| c.image == Some(graphic::NOTE_EDGE) && b.contains(c.screen.x0, c.screen.y0))
            .count();
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.a-freshly-opened-map-highlights-no-town-at-all",
        move |_| ok && edges == 0,
    );
}

#[test]
fn scenario_a_freshly_opened_map_highlights_no_town() {
    scenario("a_freshly_opened_map_highlights_no_town");
}

/// Hovering a town frames that town alone, and moving off it un-frames it.
pub fn hovering_a_town_frames_that_town_alone() {
    use dereth_ui_screens::mapradar::map::{graphic, NOTE_REST_STATE, NOTE_ROLLOVER_STATE};

    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);

    let note = g.map_notes[0];
    let b = ui.screen_box(note);
    ui.mouse_move(
        dereth_primitives::LocalTime(0.0),
        (b.x0 + b.x1) / 2,
        (b.y0 + b.y1) / 2,
    );
    let over = ui.mouse_over() == Some(note);

    let edges = |ui: &mut dereth_ui::UiSystem| {
        let mut back = dereth_ui::RecordingDrawBackend::default();
        ui.draw(&mut back);
        back.calls
            .iter()
            .filter(|c| c.image == Some(graphic::NOTE_EDGE))
            .count()
    };
    let framed = ui.node(note).map(|n| n.state.0) == Some(NOTE_ROLLOVER_STATE);
    let four = edges(&mut ui) == 4;
    let alone = g
        .map_notes
        .iter()
        .skip(1)
        .all(|h| ui.node(*h).map(|n| n.state.0) == Some(NOTE_REST_STATE));

    // Off the town but still on the map.
    let map_box = ui.screen_box(g.map.map_image.expect("the map element"));
    ui.mouse_move(
        dereth_primitives::LocalTime(1.0),
        map_box.x1 - 1,
        map_box.y1 - 1,
    );
    let unframed = ui.node(note).map(|n| n.state.0) == Some(NOTE_REST_STATE) && edges(&mut ui) == 0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.hovering-a-town-frames-that-town-alone-and-leaving-un-frames-it",
        move |_| over && framed && four && alone && unframed,
    );
}

#[test]
fn scenario_hovering_a_town_frames_that_town_alone() {
    scenario("hovering_a_town_frames_that_town_alone");
}

/// Hovering a town shows its name.
///
/// This is the only thing an ordinary player can do on this page, and it asserts the whole
/// chain: a town with no name is not even something the pointer can land on, so the hit test,
/// the hover, the tooltip and the letters in it are all one claim.
pub fn hovering_a_town_shows_its_name() {
    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);

    let note = g.map_notes[0];
    let named = ui
        .node(note)
        .and_then(|n| n.tooltip_text.clone())
        .expect("the town has a name");
    let hit_testable = ui.node(note).is_some_and(|n| n.is_mouse_visible);
    let b = ui.screen_box(note);
    ui.mouse_move(
        dereth_primitives::LocalTime(0.0),
        (b.x0 + b.x1) / 2,
        (b.y0 + b.y1) / 2,
    );
    let over = ui.mouse_over() == Some(note);

    // Ten seconds is past any hover delay the shell has.
    ui.check_tooltip(dereth_primitives::LocalTime(10.0));
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let text: String = back
        .calls
        .iter()
        .flat_map(|c| {
            c.glyphs
                .iter()
                .map(|gl| char::from_u32(u32::from(gl.ch)).unwrap_or('?'))
        })
        .collect();
    let shown = text.contains(&named);

    let mut client = HeadlessClient::model();
    client.assert_behaviour("map.page.hovering-a-town-shows-its-name", move |_| {
        hit_testable && over && shown
    });
}

#[test]
fn scenario_hovering_a_town_shows_its_name() {
    scenario("hovering_a_town_shows_its_name");
}

/// Pressing the map does nothing at all: it does not zoom, it does not move, and it asks the
/// shard for nothing.
pub fn pressing_the_map_does_nothing_for_an_ordinary_player() {
    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);
    g.update_map(&mut ui, &world_support::MapView::outdoors());
    ui.requests.clear();

    let map = g.map.map_image.expect("the map element");
    let before = ui.screen_box(map);
    let before_notes = world_support::map_notes(&ui, &g);
    let a = g.map.marker_area;
    // Dead centre of the marker box, in screen space.
    let (mx, my) = (before.x0 + (a.x0 + a.x1) / 2, before.y0 + (a.y0 + a.y1) / 2);

    ui.mouse_move(dereth_primitives::LocalTime(0.0), mx, my);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, mx, my);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, mx, my, false);
    world_support::pump_screen(&mut ui, &mut g);

    let unmoved = ui.screen_box(map) == before;
    let towns_unmoved = world_support::map_notes(&ui, &g) == before_notes;
    let asked_nothing = ui.requests.take().is_empty();

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.pressing-the-map-does-nothing-at-all-for-an-ordinary-player",
        move |_| unmoved && towns_unmoved && asked_nothing,
    );
}

#[test]
fn scenario_pressing_the_map_does_nothing_for_an_ordinary_player() {
    scenario("pressing_the_map_does_nothing_for_an_ordinary_player");
}

/// The player's own dot is where the shipped arithmetic puts it.
///
/// The arithmetic is written out in the support module from the instructions themselves rather
/// than by calling the thing under test, and one position is also worked by hand so that a
/// change to the written-out version cannot quietly follow a change to the client.
pub fn the_players_dot_is_where_the_arithmetic_puts_it() {
    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);
    let icon = g.map.player_icon.expect("the player's own mark");

    // The shipped mark is seventeen by sixteen -- the size, not the difference of the edges.
    let b0 = ui.node(icon).expect("the mark").region.box_;
    let sized = (b0.width(), b0.height()) == (17, 16);

    let wrote = g.update_map(&mut ui, &world_support::MapView::outdoors());
    let b = ui.node(icon).expect("the mark").region.box_;
    let a = g.map.marker_area;
    let placed = (b.x0, b.y0)
        == world_support::marker_oracle(
            (a.x0, a.x1, a.y0, a.y1),
            world_support::MapView::EAST,
            world_support::MapView::NORTH,
            17,
            16,
        );
    // The same position, worked by hand from the same instructions with the shipped box.
    let by_hand = (b.x0, b.y0) == (158, 73);
    let shown = world_support::element_is_visible(&ui, Some(icon));
    let inside = (a.x0..=a.x1).contains(&(b.x0 + 8)) && (a.y0..=a.y1).contains(&(b.y0 + 8));

    // And it tracks the player: further east moves it right and nowhere else.
    let east = world_support::MapView {
        outside: true,
        coords: Some((42.2, 60.0)),
        date_time: None,
    };
    g.update_map(&mut ui, &east);
    let e = ui.node(icon).expect("the mark").region.box_;
    let tracks = e.x0 > b.x0 && e.y0 == b.y0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.the-players-own-dot-is-where-the-shipped-arithmetic-puts-it",
        move |_| sized && wrote && placed && by_hand && shown && inside && tracks,
    );
}

#[test]
fn scenario_the_players_dot_is_where_the_arithmetic_puts_it() {
    scenario("the_players_dot_is_where_the_arithmetic_puts_it");
}

/// The date and the time are on the page, and drawn rather than merely stored.
pub fn the_date_and_time_are_drawn_on_the_page() {
    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);
    let wrote = g.update_map(&mut ui, &world_support::MapView::outdoors());

    let h = g.map.date_time_text.expect("the date line");
    let text = ui
        .text_element_mut(h)
        .expect("a text element")
        .glyphs
        .inq_text(false);
    let right = text == world_support::MapView::EXPECTED_DATE_LINE;

    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let glyphs: String = back
        .calls
        .iter()
        .filter(|c| c.who == h)
        .flat_map(|c| {
            c.glyphs
                .iter()
                .map(|gl| char::from_u32(u32::from(gl.ch)).unwrap_or('?'))
        })
        .collect();
    let drawn = glyphs.contains("Morningthaw");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.the-date-and-the-time-are-drawn-on-the-page",
        move |_| wrote && right && drawn,
    );
}

#[test]
fn scenario_the_date_and_time_are_drawn_on_the_page() {
    scenario("the_date_and_time_are_drawn_on_the_page");
}

/// Opening the map refreshes it at once, rather than leaving it showing whatever the last look
/// left there; closing it asks for nothing.
pub fn opening_the_map_refreshes_it_at_once() {
    let (mut ui, mut g) = world_support::shipped_map_page();
    ui.now = dereth_primitives::LocalTime(100.0);

    // A look runs while the page is closed; the next one is not due for five seconds.
    g.update_map(&mut ui, &world_support::MapView::outdoors());
    let throttled = !g.map_update_due(ui.now.0) && !g.map_update_due(104.999);

    world_support::open_the_map_page(&mut ui, &mut g);
    let opened = world_support::element_is_visible(&ui, g.map.page);
    let due_at_once = g.map_update_due(ui.now.0);

    // The refresh is real: a position the closed page never saw is on screen after one look.
    let moved = world_support::MapView {
        outside: true,
        coords: Some((10.0, -20.0)),
        date_time: None,
    };
    let wrote = g.update_map(&mut ui, &moved);
    let b = ui
        .node(g.map.player_icon.expect("the mark"))
        .expect("alive")
        .region
        .box_;
    let a = g.map.marker_area;
    let refreshed =
        (b.x0, b.y0) == world_support::marker_oracle((a.x0, a.x1, a.y0, a.y1), -20.0, 10.0, 17, 16);

    // Closing it is the other arm, and it must not ask for an update.
    world_support::close_the_map_page(&mut ui, &mut g);
    let closed = !world_support::element_is_visible(&ui, g.map.page) && !g.map_update_due(ui.now.0);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.opening-the-map-refreshes-it-at-once-and-closing-it-asks-for-nothing",
        move |_| throttled && opened && due_at_once && wrote && refreshed && closed,
    );
}

#[test]
fn scenario_opening_the_map_refreshes_it_at_once() {
    scenario("opening_the_map_refreshes_it_at_once");
}

// -------------------------------------------------------------------------------------------
// radar.*
// -------------------------------------------------------------------------------------------

/// Only what the shard marks for the radar reaches it, and that throws most of the scene away.
///
/// The wanted set is read out of each recording's own descriptions rather than out of the thing
/// under test, so the two can disagree.
pub fn only_what_the_shard_marks_reaches_the_radar() {
    use dereth_primitives::ObjectId;
    use dereth_ui_screens::mapradar::radar::inq_showable_on_radar;

    let mut measured = 0usize;
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        let Some(player) = objects.world.player else {
            continue;
        };

        let mut want: std::collections::BTreeSet<ObjectId> = std::collections::BTreeSet::new();
        let mut total = 0usize;
        for (id, presence) in objects.presences() {
            let Some(w) = objects.world.weenie(id) else {
                continue;
            };
            total += 1;
            // The three values that mean "show this", and nothing else -- an object the shard
            // said nothing about reads as "undefined" and never reaches the radar.
            let showable = matches!(w.pwd.radar_enum.unwrap_or(0), 2..=4);
            if showable && presence.position.is_some() && id != player {
                want.insert(id);
            }
        }
        if total == 0 {
            continue;
        }

        let list = world_support::radar_list(&objects);
        let got: std::collections::BTreeSet<ObjectId> = list
            .iter()
            .filter(|o| !o.is_self && inq_showable_on_radar(o))
            .map(|o| o.id)
            .collect();
        ok &= got == want;
        // ...and it is a filter and not a no-op: most of the scene is thrown away.
        ok &= want.len() * 2 < total;
        println!(
            "radar: {session}: {} of {total} objects reach the radar",
            want.len()
        );
        measured += 1;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.filter.only-what-the-shard-marks-for-the-radar-reaches-it",
        move |_| measured >= 3 && ok,
    );
}

#[test]
fn scenario_only_what_the_shard_marks_reaches_the_radar() {
    scenario("only_what_the_shard_marks_reaches_the_radar");
}

/// The blips drawn are exactly the showable objects inside the radar's own range.
///
/// This is deliberately a separate claim from the filter itself: a filter that is right is worth
/// nothing if the drawing never consults it.
pub fn the_blips_are_the_showable_objects_inside_the_range() {
    use dereth_primitives::ObjectId;
    use dereth_ui_screens::mapradar::radar::{draw_objects, inq_showable_on_radar};

    const RANGE: f32 = 75.0;
    let geom = world_support::radar_geometry();
    let range_sq = (RANGE - 1.0) * (RANGE - 1.0);
    let mut ok = true;
    let mut measured = 0usize;
    let mut with_a_cut = 0usize;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        let Some(player) = objects.world.player else {
            continue;
        };
        let list = world_support::radar_list(&objects);

        let mut want: std::collections::BTreeSet<ObjectId> = std::collections::BTreeSet::new();
        for o in &list {
            let Some(w) = objects.world.weenie(o.id) else {
                continue;
            };
            if !matches!(w.pwd.radar_enum.unwrap_or(0), 2..=4) || o.id == player || !o.in_world {
                continue;
            }
            let (px, py, _) = o.player_space;
            if px * px + py * py >= range_sq {
                continue;
            }
            want.insert(o.id);
        }

        let blips = draw_objects(&list, None, geom, RANGE, None, false);
        let drawn: std::collections::BTreeSet<ObjectId> =
            blips.iter().map(|b| list[b.index].id).collect();
        ok &= drawn == want;
        ok &= !drawn.contains(&player);
        // At least one recording must really exercise the range, or this only restates the
        // filter.
        let showable = list
            .iter()
            .filter(|o| !o.is_self && inq_showable_on_radar(o))
            .count();
        if want.len() < showable {
            with_a_cut += 1;
        }
        measured += 1;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.blips.are-exactly-the-things-the-radar-shows-that-are-inside-its-range",
        move |_| measured >= 3 && with_a_cut > 0 && ok,
    );
}

#[test]
fn scenario_the_blips_are_the_showable_objects_inside_the_range() {
    scenario("the_blips_are_the_showable_objects_inside_the_range");
}

/// The filter is the difference between a crowded radar and a readable one.
///
/// The same scene with the filter taken out is drawn beside it; no count is written down here.
pub fn the_filter_is_the_difference_between_a_crowd_and_a_handful() {
    use dereth_ui_screens::mapradar::radar::draw_objects;
    use dereth_ui_screens::view::RadarEntry;

    let geom = world_support::radar_geometry();
    let mut ok = true;
    let mut measured = 0usize;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        let list = world_support::radar_list(&objects);
        if list.is_empty() {
            continue;
        }
        let after = draw_objects(&list, None, geom, 75.0, None, false).len();
        // The same scene with nothing filtered out at all.
        let unfiltered: Vec<RadarEntry> = list
            .iter()
            .copied()
            .map(|mut o| {
                o.radar_enum = 4;
                o.is_self = false;
                o
            })
            .collect();
        let before = draw_objects(&unfiltered, None, geom, 75.0, None, false).len();
        println!("radar: {session}: {before} things in view, {after} on the radar");
        ok &= after < before;
        measured += 1;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.filter.is-the-difference-between-a-crowded-radar-and-a-readable-one",
        move |_| measured >= 3 && ok,
    );
}

#[test]
fn scenario_the_filter_is_the_difference_between_a_crowd_and_a_handful() {
    scenario("the_filter_is_the_difference_between_a_crowd_and_a_handful");
}

/// Every blip takes the colour its own description asks for.
///
/// Each object is coloured twice -- once by the client, once by a reading of the recording's own
/// description written out separately -- and the two must agree about every object in every
/// recording.
pub fn every_blip_takes_the_colour_its_description_asks_for() {
    use dereth_ui_screens::mapradar::radar::{get_blip_color, inq_showable_on_radar};

    let mut checked = 0usize;
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        for o in &world_support::radar_list(&objects) {
            if o.is_self || !inq_showable_on_radar(o) {
                continue;
            }
            let Some(w) = objects.world.weenie(o.id) else {
                continue;
            };
            let got = get_blip_color(Some(o)).hex;
            let want = world_support::expected_blip_colour(&w.pwd);
            if got != want {
                println!(
                    "radar: {session} {:?}: {got:#08X} against {want:#08X}",
                    o.id
                );
            }
            ok &= got == want;
            checked += 1;
        }
    }
    println!("radar: {checked} things on the radar coloured across the recordings");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.blips.every-one-takes-the-colour-its-own-description-asks-for",
        move |_| checked > 50 && ok,
    );
}

#[test]
fn scenario_every_blip_takes_the_colour_its_description_asks_for() {
    scenario("every_blip_takes_the_colour_its_description_asks_for");
}

/// The only things on the radar that take no colour of their own are ordinary players.
///
/// A radar of plain white crosses where retail draws coloured ones is the failure this guards,
/// so a thing that takes the plain colour is exactly a thing every rule missed -- **except** an
/// ordinary player at peace, whom the client's own last branch leaves plain on purpose. No
/// creature, trader, portal or item may reach it. More than one colour must appear, or a radar
/// that painted everything alike would satisfy this too.
///
/// "Not one at all" would be wrong: three of the eighteen recordings carry other players.
pub fn no_blip_falls_through_to_the_colour_that_means_nothing() {
    use dereth_ui_screens::mapradar::radar::{
        get_blip_color, inq_showable_on_radar, semantic, YELLOW,
    };

    let mut all: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    let mut white = 0usize;
    let mut plain_players = 0usize;
    let mut measured = 0usize;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        for o in &world_support::radar_list(&objects) {
            if o.is_self || !inq_showable_on_radar(o) {
                continue;
            }
            let c = get_blip_color(Some(o)).hex;
            *all.entry(c).or_default() += 1;
            if c == semantic::DEFAULT.hex {
                // Who it is matters: the plain colour is what the client's own last branch gives
                // an ordinary player at peace, and is not what anything else may end up with.
                let is_player = objects
                    .world
                    .weenie(o.id)
                    .is_some_and(|w| w.pwd.bitfield & 0x0000_0008 != 0);
                if is_player {
                    plain_players += 1;
                } else {
                    white += 1;
                    println!("radar: {session} {:?} took no colour of its own", o.id);
                }
            }
        }
        measured += 1;
    }
    println!(
        "radar: the recordings' blip colours are {all:?}; {plain_players} of them are ordinary players taking the plain one"
    );
    let enough_colours = all.len() >= 4;
    // The colour the shard's own marked traders take, which is what a real radar is mostly made
    // of.
    let yellow = all.contains_key(&YELLOW.hex);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.blips.the-only-things-that-take-no-colour-of-their-own-are-ordinary-players",
        move |_| measured >= 3 && white == 0 && plain_players > 0 && enough_colours && yellow,
    );
}

#[test]
fn scenario_no_blip_falls_through_to_the_colour_that_means_nothing() {
    scenario("no_blip_falls_through_to_the_colour_that_means_nothing");
}

/// The player's own place is nine bright-green points on the centre of the ring.
///
/// He is not one of the blips at all -- the radar's own list never holds him -- so the mark
/// could never have appeared by repairing the blips.
pub fn the_players_mark_is_nine_bright_green_points_on_the_centre() {
    use dereth_ui_screens::mapradar::radar::{
        center_marker_fills, center_marker_pixels, CENTER_MARKER_COLOR,
    };

    let px = center_marker_pixels();
    let shape = px
        == vec![
            (0, 0),
            (0, -1),
            (0, 1),
            (-1, 0),
            (1, 0),
            (-2, 0),
            (2, 0),
            (0, -2),
            (0, 2),
        ];
    let colour = CENTER_MARKER_COLOR.hex == 0x00FF00
        && CENTER_MARKER_COLOR == dereth_ui_screens::mapradar::radar::BRIGHT_GREEN;

    let geom = world_support::radar_geometry();
    let fills = center_marker_fills(geom);
    #[allow(clippy::cast_possible_truncation)]
    let cx = geom.center.0 as i32;
    #[allow(clippy::cast_possible_truncation)]
    let cy = geom.center.1 as i32;
    let placed = fills.len() == 9
        && fills.iter().zip(px).all(|(f, (dx, dy))| {
            (f.x, f.y) == (cx + dx, cy + dy) && (f.w, f.h) == (1, 1) && f.color == 0xFF00_FF00
        });

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.player.his-own-place-is-nine-bright-green-points-on-the-centre",
        move |_| shape && colour && placed,
    );
}

#[test]
fn scenario_the_players_mark_is_nine_bright_green_points_on_the_centre() {
    scenario("the_players_mark_is_nine_bright_green_points_on_the_centre");
}

/// The radar's padlock is up from the first frame, drawing its open picture.
///
/// The picture is read out of the shipped layout rather than written down here, and it is the
/// picture that is asserted rather than the bookkeeping behind it -- asserting the bookkeeping
/// is what once let this pass while the corner of the ring stayed blank.
pub fn the_radars_padlock_is_up_and_open_from_the_first_frame() {
    use dereth_ui_screens::mapradar::radar::{child, lock_state};
    use dereth_ui_screens::screens::gameplay::window;

    let (ui, screen) = world_support::shipped_gameplay();
    let root = *screen.roots().first().expect("the screen has a root");
    let radar = ui
        .get_child_recursive(root, window::RADAR)
        .expect("the radar");

    let lock = ui
        .get_child_recursive(radar, child::LOCK_BUTTON)
        .expect("the padlock");
    let drag = ui
        .get_child_recursive(radar, child::DRAG_BUTTON)
        .expect("the drag handle");
    let up = ui
        .node(lock)
        .expect("the padlock is alive")
        .region
        .flags
        .visible
        && !ui
            .node(drag)
            .expect("the handle is alive")
            .region
            .flags
            .visible;

    let node = ui.node(lock).expect("the padlock is alive");
    let media_of = |s: u32| -> dereth_primitives::DataId {
        let sd = node
            .desc
            .access_state(dereth_ui::StateId(s))
            .unwrap_or_else(|| panic!("the shipped layout gives the padlock no state {s:#x}"));
        match sd.media.first().map(|m| &m.fields) {
            Some(dereth_ui::desc::state_desc::MediaFields::Image { file, .. }) => *file,
            other => panic!("that state carries {other:?}, not a picture"),
        }
    };
    let locked = media_of(lock_state::LOCKED);
    let unlocked = media_of(lock_state::UNLOCKED);
    let two_pictures = locked != unlocked;
    let open = node.region.image.as_ref().map(|g| g.did) == Some(unlocked);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.padlock.is-up-and-drawing-its-open-picture-from-the-first-frame",
        move |_| up && two_pictures && open,
    );
}

#[test]
fn scenario_the_radars_padlock_is_up_and_open_from_the_first_frame() {
    scenario("the_radars_padlock_is_up_and_open_from_the_first_frame");
}

/// The player's mark is laid on the radar's own element, after the blips and in the order the
/// client issues them.
///
/// The whole seam in one scenario: a recording, the object stream, the heads-up display's own
/// view of it, and the fills that end up on the element.
pub fn the_players_mark_is_laid_on_the_radar_element() {
    use dereth_ui_screens::mapradar::radar::{center_marker_fills, RadarGeometry};
    use dereth_ui_screens::screens::gameplay::window;

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let session = world_support::the_busiest_recording();
    let objects = world_support::replay_objects_at_peak(session);
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let mut hud = dereth_client::hud::Hud::new();
    hud.sync(
        &objects,
        Some(dereth_client::hud::ViewerFrame {
            position: pos,
            heading_degrees: 0.0,
        }),
    );

    let root = *screen.roots().first().expect("the screen has a root");
    let gameplay = world_support::as_gameplay(&mut screen);
    let wrote = gameplay.update_radar(&mut ui, &hud.view(&objects));
    let geom = RadarGeometry {
        radius: gameplay.radar.radius,
        center: gameplay.radar.center,
    };

    let radar = ui
        .get_child_recursive(root, window::RADAR)
        .expect("the radar");
    let fills = ui
        .node(radar)
        .expect("the radar is alive")
        .region
        .surface_fills
        .clone();
    let want = center_marker_fills(geom);
    let laid = fills.len() >= 9 && fills[fills.len() - 9..] == want[..];
    // ...and the blips are still in front of it: the mark did not replace them.
    let blips_too = fills.len() > 9;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.player.his-mark-is-laid-on-the-radars-own-element-after-the-blips",
        move |_| wrote && want.len() == 9 && laid && blips_too,
    );
}

#[test]
fn scenario_the_players_mark_is_laid_on_the_radar_element() {
    scenario("the_players_mark_is_laid_on_the_radar_element");
}

// -------------------------------------------------------------------------------------------
// radar.range.*
// -------------------------------------------------------------------------------------------

/// There are two radar ranges and one question decides between them: is the player inside?
///
/// The two distances are written out here as numbers rather than fetched through the same call
/// the client makes, so a wrong one is visible. The boundary is driven on both sides, and the
/// piece of land an id belongs to must not reach the question at all.
pub fn there_are_two_radar_ranges_and_one_question() {
    use dereth_physics::landdefs::is_outdoors;
    use dereth_primitives::CellId;
    use dereth_ui_screens::mapradar::radar::radar_range;

    let outdoors_is_75 = (radar_range(true) - world_support::OUTDOOR_RANGE).abs() < f32::EPSILON;
    let indoors_is_25 = (radar_range(false) - world_support::INDOOR_RANGE).abs() < f32::EPSILON;
    // The indoor one must be the smaller: that is the whole of the claim.
    let smaller = radar_range(false) < radar_range(true);

    let block = 0xA9B4_0000u32;
    let outside = [1u32, 0x20, 0x3F, 0x40, 0xFF]
        .into_iter()
        .all(|idx| is_outdoors(CellId(block | idx)));
    let inside = [world_support::ENV_CELL_FLOOR, 0x101, 0x1FF, 0xFFFF]
        .into_iter()
        .all(|idx| !is_outdoors(CellId(block | idx)));
    // A different piece of land, the same answers: the top half of an id is masked off first.
    let masked = is_outdoors(CellId(0x0001_0001)) && !is_outdoors(CellId(0x0001_0100));

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.there-are-two-and-one-question-about-the-players-own-room-decides",
        move |_| outdoors_is_75 && indoors_is_25 && smaller && outside && inside && masked,
    );
}

#[test]
fn scenario_there_are_two_radar_ranges_and_one_question() {
    scenario("there_are_two_radar_ranges_and_one_question");
}

/// Every place a recording puts the player takes the range its own room asks for, and the
/// recordings between them offer both rooms.
pub fn every_recorded_place_takes_the_range_its_room_asks_for() {
    use dereth_physics::landdefs::is_outdoors;
    use dereth_ui_screens::mapradar::radar::radar_range;
    use dereth_ui_screens::view::GameView as _;

    let mut indoors = 0usize;
    let mut outdoors = 0usize;
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let st = world_support::radar_stations(session);
        for (what, station) in [("outdoor", st.outdoor), ("indoor", st.indoor)] {
            let Some((at, recorded, here)) = station else {
                continue;
            };
            let (objects, hud) = world_support::scene_at(session, at);
            let Some(cell) = hud.player_cell else {
                continue;
            };
            // The two passes must reproduce each other, or nothing below is about that room.
            ok &= cell == recorded;
            let view = hud.view(&objects);
            let outside = view.player_outside();
            // Two readings that could disagree: one through the display and its own view, one
            // straight off the recording's own room id.
            ok &= outside == is_outdoors(cell);
            ok &= outside == (what == "outdoor");
            let want = if outside {
                world_support::OUTDOOR_RANGE
            } else {
                world_support::INDOOR_RANGE
            };
            ok &= (radar_range(outside) - want).abs() < f32::EPSILON;
            if outside {
                outdoors += 1;
            } else {
                indoors += 1;
            }
            println!(
                "range: {session} {what}: room {:#010X}, {here} things, range {want}",
                cell.0
            );
        }
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.every-place-a-recording-puts-the-player-takes-the-range-its-room-asks-for",
        move |_| indoors > 0 && outdoors > 0 && ok,
    );
}

#[test]
fn scenario_every_recorded_place_takes_the_range_its_room_asks_for() {
    scenario("every_recorded_place_takes_the_range_its_room_asks_for");
}

/// A dungeon and the inside of an above-ground building are the same case.
///
/// The client has one question and cannot tell them apart; the shipped data agrees, because
/// every room in it is numbered above the boundary and both kinds take the short range. The
/// open land of the same two pieces of land is the control.
pub fn a_dungeon_and_a_building_inside_are_the_same_case() {
    use dereth_assets::{world::LandblockInfo, Decode};
    use dereth_dat::DbType;
    use dereth_physics::landdefs::is_outdoors;
    use dereth_primitives::{CellId, DataId};
    use dereth_ui_screens::mapradar::radar::radar_range;

    let store = support::store();

    // Every room in the shipped data, grouped by the piece of land it belongs to.
    let mut rooms: std::collections::BTreeMap<u16, Vec<u32>> = std::collections::BTreeMap::new();
    let mut below_the_boundary = 0usize;
    for id in store.ids_of(DbType::Cell) {
        let idx = id.0 & 0xFFFF;
        if idx == 0xFFFF || idx == 0xFFFE {
            continue;
        }
        if idx < world_support::ENV_CELL_FLOOR {
            below_the_boundary += 1;
        }
        rooms
            .entry(u16::try_from(id.0 >> 16).expect("sixteen bits"))
            .or_default()
            .push(idx);
    }
    // The premise for the whole claim: the shipped numbering really does put every room above
    // the boundary, which is why one question suffices.
    let numbering = !rooms.is_empty() && below_the_boundary == 0;

    let buildings_of = |block: u16| -> Option<usize> {
        let id = DataId((u32::from(block) << 16) | 0xFFFE);
        let bytes = store.read_typed(DbType::Lbi, id).ok()?;
        LandblockInfo::decode_payload(id, &bytes)
            .ok()
            .map(|l| l.buildings.len())
    };

    let mut building_block = None;
    let mut dungeon_block = None;
    for (&block, cells) in &rooms {
        if cells.is_empty() {
            continue;
        }
        match buildings_of(block) {
            Some(n) if n > 0 && building_block.is_none() => building_block = Some(block),
            Some(0) if dungeon_block.is_none() => dungeon_block = Some(block),
            _ => {}
        }
        if building_block.is_some() && dungeon_block.is_some() {
            break;
        }
    }
    let bb = building_block.expect("some piece of land has both rooms and buildings on it");
    let db = dungeon_block.expect("some piece of land has rooms and no buildings");
    let building_room = CellId((u32::from(bb) << 16) | rooms[&bb][0]);
    let dungeon_room = CellId((u32::from(db) << 16) | rooms[&db][0]);

    let both_short = [building_room, dungeon_room].into_iter().all(|cell| {
        !is_outdoors(cell)
            && (radar_range(is_outdoors(cell)) - world_support::INDOOR_RANGE).abs() < f32::EPSILON
    });
    let same_case =
        (radar_range(is_outdoors(building_room)) - radar_range(is_outdoors(dungeon_room))).abs()
            < f32::EPSILON;
    // The control from the same two pieces of land, so this is not simply answering the short
    // range to everything.
    let control = [bb, db].into_iter().all(|block| {
        let land = CellId((u32::from(block) << 16) | 1);
        is_outdoors(land)
            && (radar_range(is_outdoors(land)) - world_support::OUTDOOR_RANGE).abs() < f32::EPSILON
    });
    println!(
        "range: {} pieces of land carry rooms; a building's inside and a dungeon both take \
         the short range",
        rooms.len()
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.a-dungeon-and-the-inside-of-a-building-are-the-same-case",
        move |_| numbering && both_short && same_case && control,
    );
}

#[test]
fn scenario_a_dungeon_and_a_building_inside_are_the_same_case() {
    scenario("a_dungeon_and_a_building_inside_are_the_same_case");
}

/// The range follows the player as he steps inside and out again.
///
/// One display, one world, and two places that differ only in the room the player is in -- so
/// nothing but the room can be what moves the range. And back out again in the same scenario,
/// because a change that only ever went one way would satisfy every step before it.
pub fn the_range_follows_the_player_in_and_out() {
    use dereth_physics::landdefs::is_outdoors;
    use dereth_primitives::CellId;
    use dereth_ui_screens::mapradar::radar::radar_range;
    use dereth_ui_screens::view::GameView as _;

    let session = world_support::a_recording_with_a_world();
    let st = world_support::radar_stations(session);
    let (objects, _) = world_support::scene_at(session, st.busiest.0);
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let block = pos.cell.0 & 0xFFFF_0000;

    let mut hud = dereth_client::hud::Hud::new();
    // No player at all: the client's own answer is "not outside", which takes the short range.
    let nobody = !hud.player_outside()
        && (radar_range(hud.player_outside()) - world_support::INDOOR_RANGE).abs() < f32::EPSILON;

    let mut outdoor = pos;
    outdoor.cell = CellId(block | 0x0001);
    hud.sync(
        &objects,
        Some(dereth_client::hud::ViewerFrame {
            position: outdoor,
            heading_degrees: 0.0,
        }),
    );
    let out_ok = hud.player_cell == Some(outdoor.cell) && hud.view(&objects).player_outside();
    let out_range = radar_range(hud.view(&objects).player_outside());

    let mut indoor = pos;
    indoor.cell = CellId(block | world_support::ENV_CELL_FLOOR);
    hud.sync(
        &objects,
        Some(dereth_client::hud::ViewerFrame {
            position: indoor,
            heading_degrees: 0.0,
        }),
    );
    let in_ok = hud.player_cell == Some(indoor.cell) && !hud.view(&objects).player_outside();
    let in_range = radar_range(hud.view(&objects).player_outside());

    let ranges = (out_range - world_support::OUTDOOR_RANGE).abs() < f32::EPSILON
        && (in_range - world_support::INDOOR_RANGE).abs() < f32::EPSILON
        && out_range > in_range;

    hud.sync(
        &objects,
        Some(dereth_client::hud::ViewerFrame {
            position: outdoor,
            heading_degrees: 0.0,
        }),
    );
    let back = hud.view(&objects).player_outside() && is_outdoors(outdoor.cell);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.follows-the-player-as-he-steps-inside-and-out-again",
        move |_| nobody && out_ok && in_ok && ranges && back,
    );
}

#[test]
fn scenario_the_range_follows_the_player_in_and_out() {
    scenario("the_range_follows_the_player_in_and_out");
}

/// What the radar shows stops one short of its own range, at either range.
///
/// Four made-up things straddle both edges a third of a metre either side, on a heading that is
/// not a cardinal one so that neither part of it is degenerate. No recorded place has anything
/// in either one-metre band, which is exactly why the recordings cannot see this.
pub fn what_the_radar_shows_stops_one_short_of_its_range() {
    use dereth_ui_screens::mapradar::radar::{draw_objects, radar_enum, RadarGeometry};
    use dereth_ui_screens::view::RadarEntry;

    let geom = RadarGeometry {
        radius: 50,
        center: (60.0, 60.0),
    };
    let heading = 41.7_f32.to_radians();
    let at = |d: f32| RadarEntry {
        id: dereth_primitives::ObjectId(0x8000_0000),
        player_space: (
            d * dereth_primitives::num::math::sinf(heading),
            d * dereth_primitives::num::math::cosf(heading),
            0.0,
        ),
        in_world: true,
        radar_enum: radar_enum::SHOW_ALWAYS,
        ..RadarEntry::default()
    };
    let objects: Vec<RadarEntry> = [23.7_f32, 24.3, 73.7, 74.3].into_iter().map(at).collect();

    let drawn = |range: f32| -> Vec<usize> {
        draw_objects(&objects, None, geom, range, None, false)
            .iter()
            .map(|b| b.index)
            .collect()
    };
    let out = drawn(world_support::OUTDOOR_RANGE);
    let ind = drawn(world_support::INDOOR_RANGE);
    let edges = out == vec![0, 1, 2] && ind == vec![0];

    // The premise: the square the radar also rejects against is not what is doing the work here.
    let round = [
        (world_support::OUTDOOR_RANGE, &out),
        (world_support::INDOOR_RANGE, &ind),
    ]
    .into_iter()
    .all(|(range, kept)| {
        #[allow(clippy::cast_precision_loss)]
        let scale = geom.radius as f32 / range;
        kept.iter().copied().all(|i| {
            let (x, y, _) = objects[i].player_space;
            (x * scale).abs() <= 50.0 && (y * scale).abs() <= 50.0
        })
    });
    println!("range: at the long range the edge keeps {out:?} of four; at the short, {ind:?}");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.what-the-radar-shows-stops-one-short-of-its-own-range-at-either-range",
        move |_| edges && round,
    );
}

#[test]
fn scenario_what_the_radar_shows_stops_one_short_of_its_range() {
    scenario("what_the_radar_shows_stops_one_short_of_its_range");
}

/// The chat window's own sweep for something to talk to uses the same radius as the radar.
pub fn the_chat_sweep_uses_the_same_radius() {
    use dereth_physics::landdefs::is_outdoors;
    use dereth_primitives::CellId;

    let (session, at, recorded) = world_support::a_recorded_indoor_place();
    let (objects, _) = world_support::scene_at(session, at);
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let block = recorded.0 & 0xFFFF_0000;

    let mut counts = Vec::new();
    let mut agreed = true;
    for cell in [CellId(block | 0x0001), recorded] {
        let mut p = pos;
        p.cell = cell;
        let mut hud = dereth_client::hud::Hud::new();
        hud.sync(
            &objects,
            Some(dereth_client::hud::ViewerFrame {
                position: p,
                heading_degrees: 0.0,
            }),
        );
        agreed &= hud.player_outside() == is_outdoors(cell);
        counts.push(
            hud.auto_target_world(&objects.world)
                .in_range_of_player
                .len(),
        );
    }
    let (out, inside) = (counts[0], counts[1]);
    println!(
        "range: the chat sweep found {out} things at the long range and {inside} at the short"
    );
    // Both must be more than nothing: an indoor zero would satisfy "fewer" for the wrong reason.
    let discriminating = out > 0 && inside > 0 && inside < out;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.the-chat-windows-own-sweep-uses-the-same-radius-as-the-radar",
        move |_| agreed && discriminating,
    );
}

#[test]
fn scenario_the_chat_sweep_uses_the_same_radius() {
    scenario("the_chat_sweep_uses_the_same_radius");
}

/// The things on the radar really zoom when the player goes inside.
///
/// The same world, the same place and the same frame in both arms; the only difference is the
/// room the player is said to be in. The premise is asserted too: both arms must draw
/// something, and the scene must hold something in the band between the two ranges, or "the two
/// pictures differ" would be satisfiable by a scene with nothing in it.
pub fn the_radar_zooms_when_the_player_goes_inside() {
    use dereth_physics::landdefs::is_outdoors;
    use dereth_primitives::CellId;
    use dereth_ui_screens::mapradar::radar::{draw_objects, radar_range, Blip, RadarGeometry};
    use dereth_ui_screens::view::GameView as _;

    struct Arm {
        fills: Vec<dereth_ui::UiFill>,
        blips: Vec<Blip>,
        range: f32,
        geom: RadarGeometry,
    }

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let root = *screen.roots().first().expect("the screen has a root");
    let radar = ui
        .get_child_recursive(root, dereth_ui_screens::screens::gameplay::window::RADAR)
        .expect("the radar");

    let mut stations = 0usize;
    let mut projected = 0usize;
    let mut shared_total = 0usize;
    let mut moved_outward = 0usize;
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let st = world_support::radar_stations(session);
        let Some((at, recorded, _)) = st.indoor else {
            continue;
        };
        let (objects, hud) = world_support::scene_at(session, at);
        let player = objects.world.player.expect("the recording named a player");
        let pos = objects
            .presence(player)
            .and_then(|p| p.position)
            .expect("the player has a place");
        if pos.cell != recorded || is_outdoors(recorded) {
            continue;
        }
        let block = recorded.0 & 0xFFFF_0000;

        let mut arm = |cell: CellId,
                       ui: &mut dereth_ui::UiSystem,
                       screen: &mut Box<dyn dereth_ui::framework::Screen>|
         -> Arm {
            let mut p = pos;
            p.cell = cell;
            let mut h = dereth_client::hud::Hud::new();
            h.sync(
                &objects,
                Some(dereth_client::hud::ViewerFrame {
                    position: p,
                    heading_degrees: 0.0,
                }),
            );
            let g = world_support::as_gameplay(screen);
            let view = h.view(&objects);
            g.update_radar(ui, &view);
            let geom = RadarGeometry {
                radius: g.radar.radius,
                center: g.radar.center,
            };
            let range = radar_range(view.player_outside());
            let blips = draw_objects(
                &h.radar,
                h.radar.iter().find(|o| o.is_self),
                geom,
                range,
                None,
                false,
            );
            let fills = ui
                .node(radar)
                .expect("the radar is alive")
                .region
                .surface_fills
                .clone();
            Arm {
                fills,
                blips,
                range,
                geom,
            }
        };

        let inside = arm(recorded, &mut ui, &mut screen);
        let outside = arm(CellId(block | 0x0001), &mut ui, &mut screen);

        // Something **the radar would show** in the band between the two ranges, and something
        // inside the short one, or the difference below could not discriminate them. Counting
        // everything in view instead is not enough: one recorded place has eighteen things in
        // the band and only one of them is a thing the radar shows.
        let (mut within_in, mut band) = (0usize, 0usize);
        for e in &hud.radar {
            if !e.in_world
                || e.is_self
                || !dereth_ui_screens::mapradar::radar::inq_showable_on_radar(e)
            {
                continue;
            }
            let (x, y, _) = e.player_space;
            let d = (x * x + y * y).sqrt();
            if d < world_support::INDOOR_RANGE - 1.0 {
                within_in += 1;
            } else if d < world_support::OUTDOOR_RANGE - 1.0 {
                band += 1;
            }
        }
        if band == 0 || within_in == 0 || outside.blips.is_empty() || inside.blips.is_empty() {
            continue;
        }
        stations += 1;

        ok &= (outside.range - world_support::OUTDOOR_RANGE).abs() < f32::EPSILON;
        ok &= (inside.range - world_support::INDOOR_RANGE).abs() < f32::EPSILON;
        ok &= !outside.fills.is_empty() && !inside.fills.is_empty();
        ok &= outside.fills != inside.fills;
        ok &= inside.blips.len() < outside.blips.len();

        // The projection, thing by thing, at both ranges.
        let (cx, cy) = inside.geom.center;
        #[allow(clippy::cast_precision_loss)]
        let px_radius = inside.geom.radius as f32;
        for a in [&outside, &inside] {
            let scale = px_radius / a.range;
            for b in &a.blips {
                let (x, y, _) = hud.radar[b.index].player_space;
                #[allow(clippy::cast_possible_truncation)]
                let want = (x.mul_add(scale, cx) as i32, (cy - y * scale) as i32);
                ok &= (b.x, b.y) == want;
                projected += 1;
            }
        }

        // The zoom, with no tolerance: everything drawn by both arms sits further from the
        // centre at the shorter range.
        let by_index: std::collections::BTreeMap<usize, &Blip> =
            outside.blips.iter().map(|b| (b.index, b)).collect();
        let (fcx, fcy) = (f64::from(cx), f64::from(cy));
        let radius_of =
            |b: &Blip| ((f64::from(b.x) - fcx).powi(2) + (f64::from(b.y) - fcy).powi(2)).sqrt();
        for b in &inside.blips {
            let Some(o) = by_index.get(&b.index) else {
                continue;
            };
            let (was, now) = (radius_of(o), radius_of(b));
            // Something standing on the player himself is already at the centre and has nowhere
            // further in to be; everything else must move outward.
            if was == 0.0 {
                ok &= now == 0.0;
            } else {
                ok &= now > was;
                moved_outward += 1;
            }
            shared_total += 1;
        }
        println!(
            "range: {session} at {:#010X}: {} things at the long range, {} at the short; \
             {band} in the band between them",
            recorded.0,
            outside.blips.len(),
            inside.blips.len()
        );
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.the-things-on-the-radar-really-zoom-when-the-player-goes-inside",
        move |_| stations >= 2 && projected > 0 && shared_total > 0 && moved_outward > 0 && ok,
    );
}

#[test]
fn scenario_the_radar_zooms_when_the_player_goes_inside() {
    scenario("the_radar_zooms_when_the_player_goes_inside");
}

/// The padlock works the same at either range, and neither moves the other.
pub fn the_padlock_works_the_same_at_either_range() {
    use dereth_primitives::CellId;
    use dereth_ui_screens::mapradar::radar::{child, lock_state, radar_range};
    use dereth_ui_screens::screens::gameplay::window;
    use dereth_ui_screens::view::{GameView as _, UiRequest};

    let session = world_support::a_recording_with_a_world();
    let st = world_support::radar_stations(session);
    let (objects, _) = world_support::scene_at(session, st.busiest.0);
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let block = pos.cell.0 & 0xFFFF_0000;

    let mut ok = true;
    for (idx, want) in [
        (0x0001u32, world_support::OUTDOOR_RANGE),
        (world_support::ENV_CELL_FLOOR, world_support::INDOOR_RANGE),
    ] {
        let (mut ui, mut screen) = world_support::shipped_gameplay();
        let root = *screen.roots().first().expect("the screen has a root");
        let radar = ui
            .get_child_recursive(root, window::RADAR)
            .expect("the radar");
        let lock = ui
            .get_child_recursive(radar, child::LOCK_BUTTON)
            .expect("the padlock");
        let drag = ui
            .get_child_recursive(radar, child::DRAG_BUTTON)
            .expect("the drag handle");
        let closed = world_support::state_image(&ui, lock, lock_state::LOCKED);
        let open = world_support::state_image(&ui, lock, lock_state::UNLOCKED);
        ok &= closed != open;

        let mut p = pos;
        p.cell = CellId(block | idx);
        let mut hud = dereth_client::hud::Hud::new();
        hud.sync(
            &objects,
            Some(dereth_client::hud::ViewerFrame {
                position: p,
                heading_degrees: 0.0,
            }),
        );
        {
            let g = world_support::as_gameplay(&mut screen);
            let view = hud.view(&objects);
            g.update_radar(&mut ui, &view);
            ok &= (radar_range(view.player_outside()) - want).abs() < f32::EPSILON;
        }
        ui.requests.clear();

        let b = ui.screen_box(lock);
        let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        ui.mouse_move(dereth_primitives::LocalTime(1.0), x, y);
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
        world_support::pump_boxed(&mut ui, &mut screen);
        let asked: Vec<bool> = ui
            .requests
            .take()
            .into_iter()
            .filter_map(|r| match r {
                UiRequest::SetLockUi(v) => Some(v),
                _ => None,
            })
            .collect();
        ok &= asked == vec![true];

        {
            let g = world_support::as_gameplay(&mut screen);
            g.set_lock_ui(true);
        }
        ui.broadcast_global(dereth_ui::msg::global::UI_LOCK_TOGGLED, 0);
        world_support::pump_boxed(&mut ui, &mut screen);
        ok &= !ui
            .node(drag)
            .expect("the handle is alive")
            .region
            .flags
            .visible;
        ok &= ui
            .node(lock)
            .expect("the padlock is alive")
            .region
            .image
            .as_ref()
            .map(|g| g.did)
            == closed;

        // ...and the range is untouched by any of it.
        let g = world_support::as_gameplay(&mut screen);
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
        ok &= (radar_range(view.player_outside()) - want).abs() < f32::EPSILON;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.padlock.works-the-same-at-either-range-and-neither-moves-the-other",
        move |_| ok,
    );
}

#[test]
fn scenario_the_padlock_works_the_same_at_either_range() {
    scenario("the_padlock_works_the_same_at_either_range");
}

// -------------------------------------------------------------------------------------------
// radar.click.*, radar.hover.*, radar.lock.*, radar.padlock.*, radar.drag.*
// -------------------------------------------------------------------------------------------

/// A click on a thing on the radar selects the thing it stands for.
///
/// The whole seam runs: a recording, the object stream, the display, the radar's own drawing, a
/// real pointer at a real pixel, and the request the click raises. The answer is worked out
/// again from the recording's own descriptions rather than from the thing under test, so the two
/// can disagree, and no object and no count is written down here.
pub fn a_click_on_a_thing_on_the_radar_selects_it() {
    use dereth_primitives::LocalTime;
    use dereth_ui_screens::view::UiRequest;

    let mut checked = 0usize;
    let mut unambiguous = 0usize;
    let mut occluded = 0usize;
    let mut measured = 0usize;
    let mut covered_by: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let st = world_support::radar_stations(session);
        let (objects, hud) = world_support::scene_at(session, st.busiest.0);
        let (mut ui, mut screen) = world_support::shipped_gameplay();
        let radar = world_support::radar_element(&ui, &screen);
        let origin = ui.screen_box(radar);

        let blips = world_support::radar_blips(&mut ui, &mut screen, &objects, &hud);
        if blips.is_empty() {
            continue;
        }
        // The independent answer, rebuilt from the recording.
        let want = world_support::expected_blip_map(&objects, &hud, &mut screen);
        ok &= want.len() == blips.len();
        measured += 1;

        for b in &blips {
            let (x, y) = (origin.x0 + b.x, origin.y0 + b.y);
            ui.mouse_move(LocalTime(1.0), x, y);
            {
                let g = world_support::as_gameplay(&mut screen);
                let view = hud.view(&objects);
                g.update_radar(&mut ui, &view);
            }
            let _ = ui.requests.take();

            // Everything the pointer is within the client's own reach of at that point. In a
            // crowded scene several things are, and which of them the client names is its own
            // tie-breaking -- so what is asserted is that it names one of them, and, where
            // exactly one thing is in reach, that it names that one.
            let reachable = world_support::things_within_reach(&want, (b.x, b.y));
            if reachable.is_empty() {
                ok = false;
                continue;
            }

            // A thing with something else drawn over it cannot be clicked, and that is the
            // client's behaviour rather than a shortfall: the pointer lands on whatever is in
            // front, which over part of the ring is the radar's own padlock and drag handle and
            // over another part is a neighbouring window of the screen. They are counted and
            // named rather than skipped, and what is asserted is that most of the field is
            // still reachable.
            let hit = ui.hit_test_screen(x, y);
            if hit != Some(radar) {
                let Some(other) = hit else {
                    ok = false;
                    continue;
                };
                *covered_by
                    .entry(
                        ui.node(other)
                            .expect("the thing in front is alive")
                            .desc
                            .element_id
                            .0,
                    )
                    .or_insert(0usize) += 1;
                occluded += 1;
                continue;
            }

            world_support::click_at(&mut ui, x, y);
            world_support::pump_boxed(&mut ui, &mut screen);
            let got: Vec<dereth_primitives::ObjectId> = ui
                .requests
                .take()
                .into_iter()
                .filter_map(|r| match r {
                    UiRequest::Select(id) => Some(id),
                    _ => None,
                })
                .collect();
            ok &= got.len() == 1 && reachable.contains(&got[0]);
            if reachable.len() == 1 {
                ok &= got == reachable;
                unambiguous += 1;
            }
            // ...and it is a thing the recording really created, never the player himself.
            let picked = got.first().copied().unwrap_or_default();
            ok &= objects.world.weenie(picked).is_some();
            ok &= Some(picked) != objects.world.player;
            checked += 1;
        }
    }
    println!(
        "radar ui: {checked} clicks over {measured} recordings, {unambiguous} of them with one \
         thing in reach and no other; {occluded} had something drawn over them, by \
         {covered_by:#010X?}"
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.click.a-click-on-a-thing-on-the-radar-selects-the-thing-it-stands-for",
        // The denominator, so that a run the pointer could reach nothing in would not look like
        // one it reached everything in. Over the eighteen recordings about a fifth is covered,
        // so what is asserted is the property rather than a figure: most of the field is
        // reachable.
        move |_| {
            measured >= 3 && checked >= 50 && unambiguous >= 20 && occluded * 2 < checked && ok
        },
    );
}

#[test]
fn scenario_a_click_on_a_thing_on_the_radar_selects_it() {
    scenario("a_click_on_a_thing_on_the_radar_selects_it");
}

/// An empty part of the radar is transparent to the pointer and selects nothing.
///
/// Both directions in one scenario, on the same element: over nothing it cannot even be hit, so
/// a click there falls through to whatever is behind it, and over a thing it can.
pub fn empty_radar_is_transparent_and_selects_nothing() {
    use dereth_primitives::LocalTime;
    use dereth_ui_screens::view::UiRequest;

    let session = world_support::the_busiest_recording();
    let st = world_support::radar_stations(session);
    let (objects, hud) = world_support::scene_at(session, st.busiest.0);
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let origin = ui.screen_box(radar);

    let blips = world_support::radar_blips(&mut ui, &mut screen, &objects, &hud);
    let drew_something = !blips.is_empty();
    let empty = (0..origin.width())
        .flat_map(|x| (0..origin.height()).map(move |y| (x, y)))
        .find(|&(x, y)| {
            blips.iter().all(|b| {
                let (dx, dy) = (x - b.x, y - b.y);
                dx * dx + dy * dy >= 400
            })
        })
        .expect("the radar has a point twenty away from everything on it");

    let (sx, sy) = (origin.x0 + empty.0, origin.y0 + empty.1);
    ui.mouse_move(LocalTime(1.0), sx, sy);
    let nothing_under = {
        let g = world_support::as_gameplay(&mut screen);
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
        g.radar.object_under_mouse.is_none()
    };
    let _ = ui.requests.take();

    let transparent = !ui.node(radar).expect("the radar is alive").is_mouse_visible
        && ui.hit_test_screen(sx, sy) != Some(radar);

    world_support::click_at(&mut ui, sx, sy);
    world_support::pump_boxed(&mut ui, &mut screen);
    let selected_nothing = !ui
        .requests
        .take()
        .iter()
        .any(|r| matches!(r, UiRequest::Select(_)));

    // The other direction, same element, same scenario.
    let b = blips.first().expect("something is on the radar");
    let (bx, by) = (origin.x0 + b.x, origin.y0 + b.y);
    ui.mouse_move(LocalTime(1.0), bx, by);
    let something_under = {
        let g = world_support::as_gameplay(&mut screen);
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
        g.radar.object_under_mouse.is_some()
    };
    let now_hittable = ui.node(radar).expect("the radar is alive").is_mouse_visible
        && ui.hit_test_screen(bx, by) == Some(radar);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.click.an-empty-part-of-the-radar-is-transparent-and-selects-nothing",
        move |_| {
            drew_something
                && nothing_under
                && transparent
                && selected_nothing
                && something_under
                && now_hittable
        },
    );
}

#[test]
fn scenario_empty_radar_is_transparent_and_selects_nothing() {
    scenario("empty_radar_is_transparent_and_selects_nothing");
}

/// The name shown over a thing on the radar is the shard's own name for it.
pub fn the_name_over_a_thing_is_the_shards_own() {
    use dereth_primitives::LocalTime;

    let session = world_support::the_busiest_recording();
    let st = world_support::radar_stations(session);
    let (objects, hud) = world_support::scene_at(session, st.busiest.0);
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let origin = ui.screen_box(radar);

    let blips = world_support::radar_blips(&mut ui, &mut screen, &objects, &hud);
    let mut named = 0usize;
    let mut ok = !blips.is_empty();
    for b in &blips {
        ui.mouse_move(LocalTime(1.0), origin.x0 + b.x, origin.y0 + b.y);
        let id = {
            let g = world_support::as_gameplay(&mut screen);
            let view = hud.view(&objects);
            g.update_radar(&mut ui, &view);
            g.radar.object_under_mouse
        };
        let Some(id) = id else {
            ok = false;
            continue;
        };
        let want = objects
            .world
            .weenie(id)
            .map(|w| w.pwd.name.clone())
            .expect("the recording named the thing it sent");
        let node = ui.node(radar).expect("the radar is alive");
        // The flag the hover itself reads, as well as the words.
        ok &= node.region.flags.tooltip;
        ok &= node.tooltip_text.as_deref() == Some(want.as_str());
        if !want.is_empty() {
            named += 1;
        }
    }
    println!(
        "radar ui: {named} of {} things on the radar named themselves",
        blips.len()
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.hover.the-name-shown-over-a-thing-is-the-shards-own-name-for-it",
        move |_| ok && named > 0,
    );
}

#[test]
fn scenario_the_name_over_a_thing_is_the_shards_own() {
    scenario("the_name_over_a_thing_is_the_shards_own");
}

/// Whether the interface is locked is one bit of the player's own options, and it survives the
/// blob the shard sends it in.
pub fn the_lock_is_one_bit_of_the_players_options() {
    use dereth_protocol::login::PlayerModule;
    use dereth_protocol::Message as _;

    /// The bit, as a number: reading it back through the client's own name would not notice a
    /// wrong one.
    const LOCK: u32 = 0x0100_0000;

    let bit = LOCK == 1 << 24
        // ...and it is not the bit another option uses, in another word entirely.
        && LOCK != 1 << 21;
    // A fresh character starts with the interface unlocked.
    let fresh = PlayerModule::DEFAULT_OPTIONS2 & LOCK == 0;

    let mut ok = bit && fresh;
    for want in [false, true] {
        let mut m = PlayerModule {
            options2: PlayerModule::DEFAULT_OPTIONS2,
            ..Default::default()
        };
        m.options2 = if want {
            m.options2 | LOCK
        } else {
            m.options2 & !LOCK
        };
        let mut hud = dereth_client::hud::Hud::new();
        hud.player_module = Some(m.clone());
        ok &= hud.lock_ui() == want;

        // And it survives the blob it is kept in. The word only reaches the wire when the blob
        // says it is there, which is why a bit that is never written comes back as the default.
        let m = PlayerModule {
            option_flags: 0x0040,
            spell_bars: vec![Vec::new()],
            ..m
        };
        let mut w = dereth_protocol::Writer::body();
        m.write(&mut w).expect("the module packs");
        let bytes = w.into_inner();
        let mut r = dereth_protocol::Reader::body(&bytes);
        let back = PlayerModule::read(&mut r).expect("and unpacks again");
        ok &= (back.options2 & LOCK != 0) == want;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.lock.whether-the-interface-is-locked-is-one-bit-of-the-players-own-options",
        move |_| ok,
    );
}

#[test]
fn scenario_the_lock_is_one_bit_of_the_players_options() {
    scenario("the_lock_is_one_bit_of_the_players_options");
}

/// Clicking the padlock asks for the lock to be flipped, and does nothing else.
///
/// Both ways round, because a toggle that only ever asked for one of them would satisfy a
/// one-way measurement. The click is a real pointer on the real element.
pub fn clicking_the_padlock_asks_to_flip_the_lock() {
    use dereth_ui_screens::mapradar::radar::child;
    use dereth_ui_screens::view::UiRequest;

    let mut ok = true;
    for start in [false, true] {
        let (mut ui, mut screen) = world_support::shipped_gameplay();
        let radar = world_support::radar_element(&ui, &screen);
        {
            let g = world_support::as_gameplay(&mut screen);
            g.set_lock_ui(start);
            g.cascade_lock(&mut ui, start);
        }
        let lock = ui
            .get_child_recursive(radar, child::LOCK_BUTTON)
            .expect("the padlock");
        let b = ui.screen_box(lock);
        ok &= b.is_valid();
        let _ = ui.requests.take();

        world_support::click_at(&mut ui, (b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        world_support::pump_boxed(&mut ui, &mut screen);

        let got = ui.requests.take();
        ok &= got
            .iter()
            .filter(|r| matches!(r, UiRequest::SetLockUi(_)))
            .count()
            == 1;
        ok &= got.contains(&UiRequest::SetLockUi(!start));
        // The padlock is a child of the radar, so the click must not also select something.
        ok &= !got.iter().any(|r| matches!(r, UiRequest::Select(_)));
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.padlock.a-click-on-it-asks-to-flip-the-lock-and-does-nothing-else",
        move |_| ok,
    );
}

#[test]
fn scenario_clicking_the_padlock_asks_to_flip_the_lock() {
    scenario("clicking_the_padlock_asks_to_flip_the_lock");
}

/// The lock the player asks for reaches his own options, and a screen rebuilt afterwards reads
/// it back.
pub fn the_lock_reaches_the_players_options_and_survives_a_rebuild() {
    use dereth_protocol::login::PlayerModule;
    use dereth_ui_screens::mapradar::radar::{child, lock_state};
    use dereth_ui_screens::view::UiRequest;

    const LOCK: u32 = 0x0100_0000;

    let session = world_support::a_recording_with_a_world();
    let st = world_support::radar_stations(session);
    let (objects, mut hud) = world_support::scene_at(session, st.busiest.0);
    hud.player_module = Some(PlayerModule {
        options2: PlayerModule::DEFAULT_OPTIONS2,
        ..Default::default()
    });
    let starts_unlocked = !hud.lock_ui();

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let lock = ui
        .get_child_recursive(radar, child::LOCK_BUTTON)
        .expect("the padlock");
    let drag = ui
        .get_child_recursive(radar, child::DRAG_BUTTON)
        .expect("the drag handle");
    let b = ui.screen_box(lock);

    // The screen has already run a frame by the time the player reaches for the padlock.
    let seeded = {
        let g = world_support::as_gameplay(&mut screen);
        hud.drive(&mut ui, g, 1, &objects);
        !g.locked
    };
    let _ = ui.requests.take();

    world_support::click_at(&mut ui, (b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    world_support::pump_boxed(&mut ui, &mut screen);
    let asked: Vec<bool> = ui
        .requests
        .take()
        .into_iter()
        .filter_map(|r| match r {
            UiRequest::SetLockUi(v) => Some(v),
            _ => None,
        })
        .collect();
    let asked_for_it = asked == vec![true];

    // What the shell does with that request: write the screen's own mirror, then tell everything
    // else. The order is the client's and it matters.
    {
        let g = world_support::as_gameplay(&mut screen);
        g.set_lock_ui(true);
    }
    ui.broadcast_global(dereth_ui::msg::global::UI_LOCK_TOGGLED, 0);
    world_support::pump_boxed(&mut ui, &mut screen);
    let cascaded = !ui
        .node(drag)
        .expect("the handle is alive")
        .region
        .flags
        .visible
        && ui
            .node(lock)
            .expect("the padlock is alive")
            .region
            .image
            .as_ref()
            .map(|g| g.did)
            == world_support::state_image(&ui, lock, lock_state::LOCKED);

    // And the display writes it into the player's own options.
    let before = hud.stats.lock_ui_writes;
    {
        let g = world_support::as_gameplay(&mut screen);
        hud.drive(&mut ui, g, 1, &objects);
    }
    let written = hud.stats.lock_ui_writes == before + 1
        && hud.lock_ui()
        && hud.player_module.as_ref().expect("the module").options2 & LOCK != 0;

    // A screen rebuilt from the same options comes up locked, with nobody clicking anything.
    let (mut ui2, mut screen2) = world_support::shipped_gameplay();
    let rebuilt = {
        let g = world_support::as_gameplay(&mut screen2);
        let fresh = !g.locked;
        hud.drive(&mut ui2, g, 2, &objects);
        fresh && g.locked
    };
    let radar2 = world_support::radar_element(&ui2, &screen2);
    let lock2 = ui2
        .get_child_recursive(radar2, child::LOCK_BUTTON)
        .expect("the padlock");
    let drew_shut = ui2
        .node(lock2)
        .expect("the padlock is alive")
        .region
        .image
        .as_ref()
        .map(|g| g.did)
        == world_support::state_image(&ui2, lock2, lock_state::LOCKED);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.lock.the-lock-the-player-asks-for-reaches-his-options-and-survives-a-rebuilt-screen",
        move |_| {
            starts_unlocked && seeded && asked_for_it && cascaded && written && rebuilt && drew_shut
        },
    );
}

#[test]
fn scenario_the_lock_reaches_the_players_options_and_survives_a_rebuild() {
    scenario("the_lock_reaches_the_players_options_and_survives_a_rebuild");
}

/// The padlock swaps its two pictures with the lock, and the drag handle follows it.
pub fn the_padlock_swaps_its_pictures_and_the_handle_follows() {
    use dereth_ui_screens::mapradar::radar::{child, lock_state};

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let lock = ui
        .get_child_recursive(radar, child::LOCK_BUTTON)
        .expect("the padlock");
    let drag = ui
        .get_child_recursive(radar, child::DRAG_BUTTON)
        .expect("the drag handle");

    let shut = world_support::state_image(&ui, lock, lock_state::LOCKED);
    let open = world_support::state_image(&ui, lock, lock_state::UNLOCKED);
    let mut ok = shut != open;

    // And back, and again: a change that only went one way would satisfy two of these three.
    for locked in [true, false, true] {
        {
            let g = world_support::as_gameplay(&mut screen);
            g.cascade_lock(&mut ui, locked);
        }
        ok &= ui
            .node(lock)
            .expect("the padlock is alive")
            .region
            .image
            .as_ref()
            .map(|g| g.did)
            == if locked { shut } else { open };
        ok &= ui
            .node(drag)
            .expect("the handle is alive")
            .region
            .flags
            .visible
            == !locked;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.padlock.swaps-its-two-pictures-with-the-lock-and-the-drag-handle-follows",
        move |_| ok,
    );
}

#[test]
fn scenario_the_padlock_swaps_its_pictures_and_the_handle_follows() {
    scenario("the_padlock_swaps_its_pictures_and_the_handle_follows");
}

/// The handle in the corner of the radar moves the window by how far the pointer moved, and
/// keeps it inside the screen.
pub fn the_handle_moves_the_radar_and_keeps_it_on_screen() {
    use dereth_primitives::LocalTime;
    use dereth_ui_screens::mapradar::radar::child;

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let drag = ui
        .get_child_recursive(radar, child::DRAG_BUTTON)
        .expect("the drag handle");

    // What kind of thing it is comes from the shipped layout, not from here.
    let is_a_dragbar = ui.node(drag).expect("the handle is alive").desc.ty
        == dereth_ui::ElementType(dereth_ui::factory::ty::DRAGBAR.0);

    // It is hidden while the interface is locked; the player unlocks it to get at it.
    {
        let g = world_support::as_gameplay(&mut screen);
        g.cascade_lock(&mut ui, false);
    }
    let shown = ui
        .node(drag)
        .expect("the handle is alive")
        .region
        .flags
        .visible;

    let before = ui.node(radar).expect("the radar is alive").region.box_;
    let grab = ui.screen_box(drag);
    let (gx, gy) = ((grab.x0 + grab.x1) / 2, (grab.y0 + grab.y1) / 2);

    // Well inside the screen, so the edge is not what is being measured.
    let (dx, dy) = (-120, 90);
    ui.mouse_move(LocalTime(1.0), gx, gy);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, gx, gy);
    ui.mouse_move(LocalTime(1.1), gx + dx, gy + dy);
    let moved = ui.node(radar).expect("the radar is alive").region.box_;
    let followed = (moved.x0, moved.y0) == (before.x0 + dx, before.y0 + dy)
        && (moved.width(), moved.height()) == (before.width(), before.height());
    ui.mouse_up(
        dereth_ui::focus::action::PRIMARY_CLICK,
        gx + dx,
        gy + dy,
        false,
    );
    let released = !ui
        .node(radar)
        .expect("the radar is alive")
        .flags
        .is_moving();

    // The edges: dragging far past a corner stops at it rather than leaving the screen.
    ui.mouse_move(LocalTime(2.0), gx + dx, gy + dy);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, gx + dx, gy + dy);
    ui.mouse_move(LocalTime(2.1), -5000, -5000);
    let corner = ui.node(radar).expect("the radar is alive").region.box_;
    let clamped_near = (corner.x0, corner.y0) == (0, 0);
    ui.mouse_move(LocalTime(2.2), 5000, 5000);
    let far = ui.node(radar).expect("the radar is alive").region.box_;
    let parent = ui
        .node(ui.parent(radar).expect("the radar hangs off something"))
        .expect("alive")
        .region
        .box_;
    let clamped_far =
        (far.x0, far.y0) == (parent.width() - far.width(), parent.height() - far.height());
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, 5000, 5000, false);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.drag.the-handle-moves-the-window-by-how-far-the-pointer-moved-and-keeps-it-on-screen",
        move |_| {
            is_a_dragbar && shown && followed && released && clamped_near && clamped_far
        },
    );
}

#[test]
fn scenario_the_handle_moves_the_radar_and_keeps_it_on_screen() {
    scenario("the_handle_moves_the_radar_and_keeps_it_on_screen");
}

/// A radar the player has moved writes its new place into his own options, once.
pub fn a_moved_radar_writes_its_new_place_into_the_options() {
    use dereth_primitives::LocalTime;
    use dereth_ui_screens::mapradar::radar::child;
    use dereth_ui_screens::screens::gameplay::placement;
    use dereth_ui_screens::view::UiRequest;

    let session = world_support::a_recording_with_a_world();
    let st = world_support::radar_stations(session);
    let (objects, hud) = world_support::scene_at(session, st.busiest.0);
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let drag = ui
        .get_child_recursive(radar, child::DRAG_BUTTON)
        .expect("the drag handle");
    // The window's own number comes out of the shipped layout, not from here.
    let window_id = world_support::as_gameplay(&mut screen).radar.window_id;
    let has_an_id = window_id != 0;

    {
        let g = world_support::as_gameplay(&mut screen);
        g.cascade_lock(&mut ui, false);
        // The first look only records where the window is; nothing has moved yet.
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
    }
    let _ = ui.requests.take();

    let grab = ui.screen_box(drag);
    let (gx, gy) = ((grab.x0 + grab.x1) / 2, (grab.y0 + grab.y1) / 2);
    let (dx, dy) = (-77, 61);
    ui.mouse_move(LocalTime(1.0), gx, gy);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, gx, gy);
    ui.mouse_move(LocalTime(1.1), gx + dx, gy + dy);
    ui.mouse_up(
        dereth_ui::focus::action::PRIMARY_CLICK,
        gx + dx,
        gy + dy,
        false,
    );

    let at = ui.node(radar).expect("the radar is alive").region.box_;
    {
        let g = world_support::as_gameplay(&mut screen);
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
    }
    let got: Vec<UiRequest> = ui
        .requests
        .take()
        .into_iter()
        .filter(|r| matches!(r, UiRequest::SetChatWindowOption { .. }))
        .collect();
    let wrote = got
        == vec![
            UiRequest::SetChatWindowOption {
                window: window_id,
                property: placement::X,
                value: at.x0,
            },
            UiRequest::SetChatWindowOption {
                window: window_id,
                property: placement::Y,
                value: at.y0,
            },
        ];

    // A frame with no move writes nothing: the place is written when the window moves and not on
    // a timer.
    {
        let g = world_support::as_gameplay(&mut screen);
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
    }
    let quiet = !ui
        .requests
        .take()
        .iter()
        .any(|r| matches!(r, UiRequest::SetChatWindowOption { .. }));

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.drag.a-radar-the-player-has-moved-writes-its-new-place-into-his-own-options",
        move |_| has_an_id && wrote && quiet,
    );
}

#[test]
fn scenario_a_moved_radar_writes_its_new_place_into_the_options() {
    scenario("a_moved_radar_writes_its_new_place_into_the_options");
}

// -------------------------------------------------------------------------------------------
// The fixtures the world scenarios share
// -------------------------------------------------------------------------------------------

/// What the world scenarios need that [`support`] does not hold: a settled body on Holtburg's own
/// terrain, the position reporter over a mock transport, the corpus readers several of the claims
/// measure themselves against, and the rigs and replays the door, map and radar sections use.
pub mod world_support {
    use std::sync::Arc;

    use dereth_client::character::{Character, CharacterInput};
    use dereth_client::world::DEFAULT_LANDBLOCK;
    use dereth_client_net::client_session::testing::{Corpus, Direction, MockTransport};
    use dereth_client_net::client_session::{PositionReporter, Session};
    use dereth_dat::RetailDatStore;
    use dereth_primitives::LocalTime;
    use dereth_protocol::actions::unpack_action;
    use dereth_protocol::Message;

    /// `0xF61C`, the state edge a client reports when what it is doing changes.
    pub const MOVE_TO_STATE: u32 = 0xF61C;
    /// `0xF753`, the periodic "this is where I am".
    pub const AUTONOMOUS_POSITION: u32 = 0xF753;
    /// `0xF61B`, the jump.
    pub const JUMP: u32 = 0xF61B;

    /// The middle of Holtburg's own landblock, which is where every movement scenario spawns.
    const SPAWN: (f32, f32) = (96.0, 96.0);

    /// A body standing still on Holtburg's terrain, settled for two seconds of frames.
    ///
    /// The settle is not a nicety: an unsettled body has no contact plane, and a measurement
    /// taken over one would be measuring the drop.
    pub fn settled_body(store: &Arc<RetailDatStore>) -> Character {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let mut c = Character::new(store, &region, DEFAULT_LANDBLOCK, SPAWN)
            .expect("the character is created");
        for i in 1..=60 {
            c.update(LocalTime(f64::from(i) / 30.0));
        }
        assert!(
            c.on_ground(),
            "the body must settle before anything is measured"
        );
        c
    }

    /// The production position reporter, active, over a transport that binds nothing.
    pub fn reporter() -> (PositionReporter, Session<MockTransport>) {
        let mut r = PositionReporter::new(0.0);
        r.active = true;
        (r, Session::new(MockTransport::new()))
    }

    /// Every blob the session emitted, as `(sub_type, payload)`.
    pub fn emitted(session: &Session<MockTransport>) -> Vec<(u32, Vec<u8>)> {
        session
            .transport
            .sent
            .iter()
            .map(|b| {
                let a = unpack_action(&b.payload).expect("the producer emits game actions");
                (a.sub_type.0, b.payload.clone())
            })
            .collect()
    }

    /// One emitted blob, through the production reader.
    pub fn decode<M: Message>(payload: &[u8]) -> M {
        let mut a = unpack_action(payload).expect("a game action unpacks");
        let m = M::read(&mut a.body).expect("the body decodes");
        a.body
            .expect_exhausted()
            .expect("the body is fully consumed");
        m
    }

    /// Sample the ground edge, run one frame with `input`, then ask the client what the jump did.
    pub fn jump_frame(
        c: &mut Character,
        now: f64,
        input: CharacterInput,
    ) -> (bool, dereth_protocol::types::Vec3) {
        let left_before = dereth_client::app::jump_edge_sample(c);
        c.input = input;
        c.update(LocalTime(now));
        dereth_client::app::body_jump(c, left_before)
    }

    /// Every jump the recordings carry, as `(extent, upward speed)`, read off the corpus.
    ///
    /// Reading them rather than writing pairs into the source means that promoting another
    /// recording extends the measurement instead of leaving it quietly measuring a fraction of what
    /// its message claims.
    pub fn recorded_jumps() -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        for name in dereth_client_net::client_session::testing::session_names() {
            let Some(corpus) = Corpus::load(name).expect("the recording parses") else {
                continue;
            };
            for b in &corpus.blobs {
                if b.dir != Direction::ClientToServer {
                    continue;
                }
                let Ok(mut a) = unpack_action(&b.payload) else {
                    continue;
                };
                if a.sub_type.0 != JUMP {
                    continue;
                }
                let Ok(m) = <dereth_protocol::movement::MovementJump as Message>::read(&mut a.body)
                else {
                    continue;
                };
                out.push((m.0.extent, m.0.velocity.z));
            }
        }
        assert!(
            !out.is_empty(),
            "the recordings carry no jump at all; this scenario would be measuring nothing"
        );
        out
    }

    // ---------------------------------------------------------------------------------------
    // The scene-less frame
    // ---------------------------------------------------------------------------------------

    /// The channel the client's own refusals are written to.
    pub const FEEDBACK_CHANNEL: u32 = 0x1A;
    /// What a client with no body of its own says when a drop takes the ground leg.
    pub const MID_AIR: &str = "You cannot do that in mid air";

    /// A client that draws no world: the object stream, the interaction layer and the retail
    /// data, with the frame's own interaction step driven with no world under it.
    pub struct SceneLess {
        store: Arc<RetailDatStore>,
        pub objects: dereth_client::objects::ObjectStream,
        pub inter: dereth_client::interaction::Interaction,
        player: dereth_primitives::ObjectId,
        pub clock: f64,
    }

    impl SceneLess {
        pub fn new(store: &Arc<RetailDatStore>) -> Self {
            use dereth_primitives::ObjectId;
            let mut objects = dereth_client::objects::ObjectStream::new();
            let player = ObjectId(0x5000_0415);
            objects.world.player = Some(player);
            objects.world.tables.inventories.insert(
                player,
                dereth_client_model::objects::ObjectInventory::new(player),
            );
            let mut pw = dereth_client_model::Weenie::new(player);
            pw.pwd.items_capacity = Some(0xFF);
            pw.pwd.containers_capacity = Some(0xFF);
            objects.world.tables.weenies.insert(player, pw);
            let mut h = Self {
                store: Arc::clone(store),
                objects,
                inter: dereth_client::interaction::Interaction::new(),
                player,
                clock: 1.0,
            };
            h.drive();
            h
        }

        /// One interaction step with **no world**, which is what a frame that drew nothing runs.
        pub fn drive(&mut self) {
            self.clock += 1.0;
            let _ = dereth_client::interaction::use_time(
                &mut self.inter,
                &self.store,
                None,
                &mut self.objects,
                None,
                Vec::new(),
                false,
                (1024, 768),
                LocalTime(self.clock),
            );
        }

        /// An item in the player's own pack.
        pub fn carry(&mut self, id: dereth_primitives::ObjectId) -> dereth_primitives::ObjectId {
            let mut w = dereth_client_model::Weenie::new(id);
            w.pwd.container_id = Some(self.player);
            w.pwd.stack_size = Some(1);
            w.pwd.max_stack_size = Some(1);
            w.pwd.name = format!("thing {:X}", id.0);
            w.waiting = true;
            w.determine_position_state();
            self.objects.world.tables.weenies.insert(id, w);
            assert!(
                self.objects.world.is_owned_by_player(id),
                "the fixture must be carried"
            );
            id
        }

        /// A left press in the middle of the viewport, and how many picks it armed.
        pub fn viewport_left_press(&mut self) -> u64 {
            use dereth_client::ui::UiMouseEvent;
            use dereth_ui_screens::screens::gameplay::window;
            let armed = self.inter.pick.stats.requests;
            self.inter.wrapper_mouse(
                UiMouseEvent {
                    action: dereth_ui::focus::action::PRIMARY_CLICK,
                    start: true,
                    x: 512,
                    y: 384,
                    over: Some(window::SMART_BOX),
                },
                (1024, 768),
                true,
            );
            self.inter.pick.stats.requests - armed
        }

        /// Every line the client has written for the player to read, with its channel.
        pub fn lines(&self) -> Vec<(u32, String)> {
            self.objects
                .world
                .scroll
                .pending()
                .iter()
                .map(|f| (f.chat_type, f.body.clone()))
                .collect()
        }
    }

    // ---------------------------------------------------------------------------------------
    // The physics scripts
    // ---------------------------------------------------------------------------------------

    /// The client's own epsilon: a pause below it is no pause at all.
    pub const HOOK_EPSILON: f32 = 0.0002;
    /// Hook kinds, by the number the shipped data carries.
    pub const CREATE_PARTICLE: u32 = 13;
    pub const CALL_PES: u32 = 19;

    /// One of the four shipped ambient scripts that chase each other round a ring: each makes an
    /// emitter at its start and, part of a second later, calls the next one with a matching
    /// pause. Named rather than searched for, because the scenario asserts its *shape* -- so a
    /// data set that does not carry it fails loudly instead of silently measuring nothing.
    pub const RING_PARENT: dereth_primitives::DataId = dereth_primitives::DataId(0x3300_11C3);
    pub const RING_CHILD: dereth_primitives::DataId = dereth_primitives::DataId(0x3300_11C4);
    /// The shipped script whose only hook doubles the object it is played on.
    pub const DOUBLING: dereth_primitives::DataId = dereth_primitives::DataId(0x3300_0117);

    /// A motion driver over the retail data, for an object that is in a room -- which is what
    /// both arms of a delayed call test.
    pub fn script_driver(store: &Arc<RetailDatStore>) -> dereth_animation::MotionDriver {
        let assets: Arc<dyn dereth_animation::AnimAssets> = Arc::new(
            dereth_client::anim_assets::DatAnimAssets::new(Arc::clone(store)),
        );
        let mut d = dereth_animation::MotionDriver::new(assets);
        d.env.in_cell = true;
        d
    }

    /// One step of what the client runs for an animated static object, in its own order: the
    /// scripts, then the timers. Answers what the step raised.
    pub fn script_tick(
        d: &mut dereth_animation::MotionDriver,
        now: f64,
    ) -> Vec<dereth_animation::AnimEvent> {
        d.cur_time = dereth_primitives::ServerTime(now);
        d.update_scripts();
        d.update_fp_hooks();
        d.take_events()
    }

    /// Play the shipped doubling script and answer the scale it asked for, the way the scene
    /// drains it. The premise -- that it doubles, on the spot -- is asserted here.
    pub fn scale_the_shipped_script_asks_for(store: &Arc<RetailDatStore>) -> f32 {
        use dereth_assets::{Decode, HookData, PhysicsScript};
        use dereth_dat::DbType;

        let bytes = store
            .read_typed(DbType::PhysicsScript, DOUBLING)
            .expect("the script is shipped");
        let s = PhysicsScript::decode_payload(DOUBLING, &bytes).expect("it decodes");
        let (end, time) = s
            .script_data
            .iter()
            .find_map(|st| match st.hook.data {
                HookData::Scale { end, time } => Some((end, time)),
                _ => None,
            })
            .expect("the script carries a scale hook");
        assert!(
            (end - 2.0).abs() < 1e-6,
            "the shipped script doubles: end = {end}"
        );
        assert!(
            time < HOOK_EPSILON,
            "and it does it on the spot: time = {time}"
        );

        let mut d = script_driver(store);
        d.cur_time = dereth_primitives::ServerTime(0.0);
        assert!(
            d.play_script_internal(DOUBLING),
            "the shipped script queues"
        );

        let mut out = None;
        let mut t = 0.0;
        for _ in 0..30 {
            for e in script_tick(&mut d, t) {
                if let dereth_animation::AnimEvent::SetScale(s) = e {
                    assert!(out.is_none(), "the on-the-spot arm raised the event twice");
                    out = Some(s);
                }
            }
            t += 1.0 / 30.0;
        }
        let s = out.expect("the shipped scale hook raised no event at all");
        assert!(
            (d.scale - s).abs() < 1e-6,
            "the part-array half and the event disagree"
        );
        s
    }

    /// The ground everything in the scale scenarios stands on.
    pub const GROUND: f32 = 20.0;
    const SCALE_BLOCK: dereth_primitives::LandblockId = dereth_primitives::LandblockId(0xA9B4);

    pub fn flat_world() -> dereth_physics::PhysicsWorld {
        let mut land = dereth_physics::StaticLandSource::linear();
        for dx in -1_i32..=1 {
            for dy in -1_i32..=1 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let id = dereth_primitives::LandblockId::new((0xA9 + dx) as u8, (0xB4 + dy) as u8);
                land.add_flat_block(id, 10);
            }
        }
        dereth_physics::PhysicsWorld::new(Arc::new(land))
    }

    fn cell_at(p: dereth_primitives::Vec3) -> dereth_primitives::CellId {
        let mut c = SCALE_BLOCK.cell(1);
        let mut o = p;
        assert!(dereth_physics::landdefs::adjust_to_outside(&mut c, &mut o));
        c
    }

    fn walker_geometry() -> Arc<dereth_physics::SetupGeometry> {
        use dereth_physics::geom::Sphere;
        use dereth_primitives::Vec3;
        Arc::new(dereth_physics::SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
            sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0),
            step_up_height: 0.3,
            step_down_height: 0.3,
            radius: 0.5,
            height: 1.0,
            ..dereth_physics::SetupGeometry::default()
        })
    }

    /// The obstacle: one sphere sitting on the ground. Its sorting sphere is deliberately
    /// generous and the **same** in both arms -- an object's cell shadows are registered from it
    /// and scaling does not re-register them, so letting it differ would make the differential
    /// about bookkeeping rather than about the collision radius.
    fn obstacle_geometry() -> Arc<dereth_physics::SetupGeometry> {
        use dereth_physics::geom::Sphere;
        use dereth_primitives::Vec3;
        Arc::new(dereth_physics::SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.6), 0.6)],
            sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.6), 4.0),
            radius: 0.6,
            height: 1.2,
            ..dereth_physics::SetupGeometry::default()
        })
    }

    pub fn spawn_walker(
        w: &mut dereth_physics::PhysicsWorld,
        at: dereth_primitives::Vec3,
        per_substep: dereth_primitives::Vec3,
    ) -> dereth_physics::PhysHandle {
        use dereth_primitives::{Frame, ObjectId, Position, Quat};
        let h = w.create(ObjectId(1), walker_geometry(), true);
        let cell = cell_at(at);
        w.enter_cell(h, cell);
        {
            let o = w.get_mut(h).expect("the walker is live");
            o.position = Position::new(cell, Frame::new(at, Quat::IDENTITY));
            o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
                vec![Frame::new(per_substep, Quat::IDENTITY); 400],
                true,
            )));
            o.transient_state.set_active_bit(true);
            o.calc_acceleration();
            o.update_time = 0.0;
        }
        w.calc_cross_cells(h, false);
        h
    }

    pub fn place_obstacle(
        w: &mut dereth_physics::PhysicsWorld,
        at: dereth_primitives::Vec3,
    ) -> dereth_physics::PhysHandle {
        use dereth_primitives::{Frame, ObjectId, Position, Quat};
        let h = w.create(ObjectId(9), obstacle_geometry(), false);
        let cell = cell_at(at);
        w.enter_cell(h, cell);
        let frame = Frame::new(at, Quat::IDENTITY);
        if let Some(o) = w.get_mut(h) {
            o.set_frame(frame);
            o.position = Position::new(cell, frame);
        }
        w.calc_cross_cells(h, true);
        h
    }

    // ---------------------------------------------------------------------------------------
    // The camera against a wall
    // ---------------------------------------------------------------------------------------

    /// The retail training academy, whose interior rooms are the walls the camera is backed into.
    pub const TRAINING_DUNGEON: u16 = 0x8602;

    const BACK_UP_SPEED: f32 = 0.6;
    const BACK_UP_SECONDS: f64 = 1.5;
    const RUN_SECONDS: f64 = 3.5;

    /// Whether a point of an interior cell is inside the room and outside its masonry.
    ///
    /// Two lines, copied from `dereth/client/tests/dat/common/collision_probe`, which the scenario
    /// crate cannot reach; this is the smallest use of it. The probe module's own calibration for
    /// the predicate stays where it is.
    fn in_the_room(
        cell: &dereth_physics::source::EnvCellGeometry,
        local: dereth_primitives::Vec3,
    ) -> bool {
        cell.cell_bsp
            .as_ref()
            .is_some_and(|b| b.point_inside_cell_bsp(local))
            && !cell
                .physics_bsp
                .as_ref()
                .is_some_and(|b| b.point_intersects_solid(local))
    }

    /// A point of an interior cell a body can stand at, in the block's own space.
    fn a_standable_point(
        cell: &dereth_physics::source::EnvCellGeometry,
    ) -> Option<dereth_primitives::Vec3> {
        use dereth_primitives::Vec3;
        cell.cell_bsp.as_ref()?;
        for &z in &[0.5f32, 1.0] {
            for i in -12i8..=12 {
                for j in -12i8..=12 {
                    let local = Vec3::new(f32::from(i) * 0.5, f32::from(j) * 0.5, z);
                    if in_the_room(cell, local) {
                        return Some(dereth_physics::math::localtoglobal(&cell.frame, local));
                    }
                }
            }
        }
        None
    }

    /// A wall to back into: a standable point, a heading, and the way the body walks so that the
    /// camera behind it is driven into geometry.
    pub struct Wall {
        pub cell: dereth_primitives::CellId,
        start: dereth_primitives::Vec3,
        heading: dereth_primitives::Quat,
        back: dereth_primitives::Vec3,
    }

    /// One camera run: every camera origin and its distance from the pivot, one per step.
    pub struct CameraRun {
        origins: Vec<dereth_primitives::Vec3>,
        pivot_distance: Vec<f32>,
        dt: f64,
    }

    impl CameraRun {
        /// Peak-to-peak spread of the camera origin over the last `seconds`. It does not depend
        /// on the rate: a settled camera reads about zero and a cycle reads its own size however
        /// finely it is sampled.
        pub fn residual(&self, seconds: f64) -> f32 {
            use dereth_physics::math::V3 as _;
            let tail = self.tail(seconds);
            let mut worst = 0.0f32;
            for a in tail {
                for b in tail {
                    worst = worst.max(a.sub(*b).mag2().sqrt());
                }
            }
            worst
        }

        fn tail(&self, seconds: f64) -> &[dereth_primitives::Vec3] {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = ((seconds / self.dt).ceil() as usize).min(self.origins.len());
            &self.origins[self.origins.len() - n..]
        }

        /// The furthest the camera got from the pivot over the last `seconds` -- the calibration:
        /// it must be well under the free-space stand-off or the camera met no wall.
        pub fn max_pivot_distance(&self, seconds: f64) -> f32 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = ((seconds / self.dt).ceil() as usize).min(self.pivot_distance.len());
            self.pivot_distance[self.pivot_distance.len() - n..]
                .iter()
                .copied()
                .fold(0.0f32, f32::max)
        }

        pub fn last(&self) -> dereth_primitives::Vec3 {
            *self.origins.last().expect("the run produced steps")
        }
    }

    pub fn run_camera(
        src: &Arc<dereth_client::land_source::DatLandSource>,
        wall: &Wall,
        dt: f64,
    ) -> CameraRun {
        use dereth_client::camera::{CameraControl, CameraInput};
        use dereth_client::character::PLAYER_OBJECT_ID;
        use dereth_physics::math::V3 as _;
        use dereth_primitives::{Frame, Position, Vec3};

        let mut w = dereth_physics::PhysicsWorld::new(
            Arc::clone(src) as Arc<dyn dereth_physics::LandSource>
        );
        let h = w.create(PLAYER_OBJECT_ID, walker_geometry(), true);
        w.enter_cell(h, wall.cell);
        {
            let o = w.get_mut(h).expect("the body is live");
            o.position = Position::new(wall.cell, Frame::new(wall.start, wall.heading));
            o.update_time = 0.0;
        }
        w.calc_cross_cells(h, false);

        let mut cam = CameraControl::new(PLAYER_OBJECT_ID);
        let mut origins = Vec::new();
        let mut pivot_distance = Vec::new();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (RUN_SECONDS / dt).round() as usize;
        for k in 1..=steps {
            #[allow(clippy::cast_precision_loss)]
            let t = k as f64 * dt;
            // The body's position is a pure function of wall-clock time: it backs toward the
            // wall and then stands still, identically in both arms at equal `t`.
            #[allow(clippy::cast_possible_truncation)]
            let travelled = (t.min(BACK_UP_SECONDS) as f32) * BACK_UP_SPEED;
            let origin = wall.start.add(wall.back.mul(travelled));
            {
                let o = w.get_mut(h).expect("the body is live");
                o.position = Position::new(wall.cell, Frame::new(origin, wall.heading));
            }
            // Every step writes the body's position on the line above, so every step is a tick
            // by construction and the smoother must run on all of them.
            cam.update(
                &mut w,
                h,
                PLAYER_OBJECT_ID,
                CameraInput::default(),
                t,
                dt,
                true,
            );
            let pivot = dereth_physics::math::localtoglobal(
                &Frame::new(origin, wall.heading),
                Vec3::new(0.0, 0.0, 1.5),
            );
            origins.push(cam.viewer.frame.origin);
            pivot_distance.push(cam.viewer.frame.origin.sub(pivot).mag2().sqrt());
        }
        CameraRun {
            origins,
            pivot_distance,
            dt,
        }
    }

    /// Every interior room of the block that has an approach whose control run really presses
    /// the camera into a wall -- at most one heading per room, and at most `want` rooms.
    pub fn find_walls(
        src: &Arc<dereth_client::land_source::DatLandSource>,
        cells: &[dereth_primitives::CellId],
        want: usize,
    ) -> Vec<(Wall, CameraRun)> {
        use dereth_physics::LandSource as _;
        use dereth_primitives::{Quat, Vec3};

        /// The free-space stand-off, halved: a camera that never gets even this far from the
        /// pivot is pressed hard against something.
        const HALF_FREE: f32 = 2.610_077 / 2.0;

        let mut out = Vec::new();
        for &id in cells {
            if out.len() >= want {
                break;
            }
            let Some(geom) = src.as_ref().env_cell(id) else {
                continue;
            };
            let Some(at) = a_standable_point(&geom) else {
                continue;
            };
            let start = Vec3::new(at.x, at.y, at.z - 0.5);
            for k in 0..16_i8 {
                let a = f32::from(k) * std::f32::consts::TAU / 16.0;
                let half = a * 0.5;
                let heading = Quat::new(
                    dereth_primitives::num::math::cosf(half),
                    0.0,
                    0.0,
                    dereth_primitives::num::math::sinf(half),
                );
                let back = Vec3::new(
                    dereth_primitives::num::math::sinf(a),
                    -dereth_primitives::num::math::cosf(a),
                    0.0,
                );
                let wall = Wall {
                    cell: id,
                    start,
                    heading,
                    back,
                };
                let r = run_camera(src, &wall, 1.0 / 30.0);
                if r.max_pivot_distance(1.0) < HALF_FREE {
                    out.push((wall, r));
                    break;
                }
            }
        }
        out
    }

    // ---------------------------------------------------------------------------------------
    // The shard's own movements, and the body they are applied to
    // ---------------------------------------------------------------------------------------

    /// A running body is above this and a stopped one below it. Both are stated rather than
    /// implied, and they are far enough apart that neither is the other's rounding.
    pub const RUNNING: f32 = 3.0;
    pub const STOPPED: f32 = 0.5;

    /// The recording whose ordinary play carries the shard-authored movements these scenarios
    /// are driven from. It is loaded through the decoded corpus, which reassembles the blobs, so
    /// nothing here re-does that by hand.
    const CONTROL_SESSION: &str = "combat-mode-while-moving";

    /// Every movement the recorded shard sent about this session's own character that it authored
    /// rather than echoed -- which is every one that takes control of the body away from the
    /// player.
    fn server_control_buffers() -> Vec<dereth_protocol::movement::MovementBuffer> {
        use dereth_protocol::movement::MovementSetObjectMovement;
        use dereth_protocol::Opcode;

        let corpus = Corpus::load(CONTROL_SESSION)
            .expect("the recording parses")
            .expect("the recording is committed to the repository");
        // The player's own identity, out of the recording's own login.
        let me = corpus
            .blobs
            .iter()
            .find(|b| b.dir == Direction::ServerToClient && b.opcode == 0xF746)
            .map(|b| {
                dereth_primitives::ObjectId(u32::from_le_bytes([
                    b.payload[4],
                    b.payload[5],
                    b.payload[6],
                    b.payload[7],
                ]))
            })
            .expect("the recording carries its own login");

        let mut out = Vec::new();
        let mut about_others = 0usize;
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient
                || b.opcode != Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0
            {
                continue;
            }
            let mut r = dereth_protocol::Reader::new(&b.payload[4..]);
            let Ok(msg) = MovementSetObjectMovement::read(&mut r) else {
                continue;
            };
            if msg.id != me {
                about_others += 1;
                continue;
            }
            let buf = msg.decoded_movement().expect("its movement buffer decodes");
            if buf.autonomous {
                continue;
            }
            out.push(buf);
        }
        // The filter must be able to exclude something, or what it answers is a count of
        // everything.
        assert!(
            about_others > 0,
            "the identity filter excluded nothing at all"
        );
        assert!(
            !out.is_empty(),
            "the recording carries no shard-authored movement of the player"
        );
        out
    }

    /// Turn a recorded movement's interpreted half into the client's own.
    fn interpreted_state(
        wire: &dereth_protocol::movement::InterpretedMotionState,
    ) -> dereth_animation::motion::InterpretedMotionState {
        use dereth_animation::motion::{ActionNode, InterpretedMotionState};
        use dereth_animation::MotionCommand;
        let cmd = |i: Option<u16>, fallback: MotionCommand| {
            i.and_then(MotionCommand::from_index).unwrap_or(fallback)
        };
        let base = InterpretedMotionState::default();
        InterpretedMotionState {
            current_style: cmd(wire.current_style, base.current_style),
            forward_command: cmd(wire.forward_command, base.forward_command),
            forward_speed: wire.forward_speed.unwrap_or(base.forward_speed),
            sidestep_command: cmd(wire.sidestep_command, base.sidestep_command),
            sidestep_speed: wire.sidestep_speed.unwrap_or(base.sidestep_speed),
            turn_command: cmd(wire.turn_command, base.turn_command),
            turn_speed: wire.turn_speed.unwrap_or(base.turn_speed),
            actions: wire
                .actions
                .iter()
                .filter_map(|a| {
                    Some(ActionNode {
                        action: MotionCommand::from_index(a.command_index)?,
                        speed: a.speed,
                        stamp: u32::from(a.stamp()),
                        autonomous: a.autonomous(),
                    })
                })
                .collect(),
        }
    }

    /// `Ready` is the command the shard's own stop carries.
    const READY_COMMAND: u16 = 3;

    /// The **pure** stops among the shard's movements: the ones that say stop and carry no action
    /// of their own.
    ///
    /// Both halves of that filter are load-bearing. A stop that also carries an action leaves the
    /// body standing still for a reason that has nothing to do with control -- the action's own
    /// animation does it -- and folding the two together would make a scenario green or red for
    /// the wrong reason.
    fn recorded_stops() -> Vec<dereth_protocol::movement::MovementBuffer> {
        server_control_buffers()
            .into_iter()
            .filter(|b| {
                b.body.interpreted.as_ref().is_some_and(|s| {
                    s.forward_command == Some(READY_COMMAND) && s.actions.is_empty()
                })
            })
            .collect()
    }

    /// One of them.
    pub fn a_recorded_stop() -> dereth_protocol::movement::MovementBuffer {
        recorded_stops().swap_remove(0)
    }

    /// Two of them, for a scenario that needs a departure and an arrival.
    pub fn two_recorded_stops() -> (
        dereth_protocol::movement::MovementBuffer,
        dereth_protocol::movement::MovementBuffer,
    ) {
        let mut stops = recorded_stops();
        assert!(
            stops.len() >= 2,
            "the recording carries fewer than two pure stops"
        );
        let a = stops.swap_remove(0);
        let b = stops.swap_remove(0);
        (a, b)
    }

    /// The stop that **does** carry a shard-authored action: the discriminating half of the pair,
    /// and the one an action scenario is about.
    pub fn a_recorded_stop_with_an_action() -> dereth_protocol::movement::MovementBuffer {
        let b = server_control_buffers()
            .into_iter()
            .find(|b| {
                b.body.interpreted.as_ref().is_some_and(|s| {
                    s.forward_command == Some(READY_COMMAND) && !s.actions.is_empty()
                })
            })
            .expect("the recording carries a stop with an action of its own");
        assert_eq!(
            b.body.interpreted.as_ref().map(|s| s.actions.len()),
            Some(1),
            "the premise: exactly one shard-authored action"
        );
        b
    }

    /// What one driven run produced.
    pub struct Run {
        pub retakes: u32,
        at: Vec<(f32, f32)>,
    }

    /// The frame, from the client's own retake through the body's own step, thirty times a
    /// second.
    pub fn drive(
        c: &mut Character,
        mc: &mut dereth_client::character::MovementCommands,
        input: &mut CharacterInput,
        from: u32,
        frames: u32,
    ) -> Run {
        let mut r = Run {
            retakes: 0,
            at: Vec::with_capacity(frames as usize),
        };
        for i in 0..frames {
            let (pending, moving) = {
                let d = c.driver();
                (d.movement.motions_pending(), d.movement.is_moving_to())
            };
            if mc.use_time(pending, moving, input) {
                r.retakes += 1;
                c.take_control_from_server();
            }
            c.input = *input;
            c.update(LocalTime(f64::from(from + i + 1) / 30.0));
            let o = c.position().frame.origin;
            r.at.push((o.x, o.y));
        }
        r
    }

    /// Ground speed over a window of a run, measured across the straight line between its ends.
    pub fn over(r: &Run, a: u32, b: u32) -> f32 {
        let (a, b) = (a as usize, b as usize);
        assert!(
            b > a && b <= r.at.len(),
            "window {a}..{b} is outside a {}-step run",
            r.at.len()
        );
        let (p, q) = (r.at[a], r.at[b - 1]);
        #[allow(clippy::cast_precision_loss)]
        let seconds = (b - a) as f32 / 30.0;
        ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt() / seconds
    }

    /// Ground speed **along the path**, summed step by step.
    ///
    /// [`over`] measures the straight line between the ends, which is the right question for a
    /// body running in one direction and the wrong one for a body running *and turning*: the
    /// chord of an arc is shorter than the arc, and a genuinely running body measured that way
    /// reads about half what it is doing.
    pub fn along(r: &Run, a: u32, b: u32) -> f32 {
        let (ai, bi) = (a as usize, b as usize);
        assert!(
            bi > ai && bi <= r.at.len(),
            "window outside a {}-step run",
            r.at.len()
        );
        let d: f32 = r.at[ai..bi]
            .windows(2)
            .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
            .sum();
        #[allow(clippy::cast_precision_loss)]
        let seconds = (bi - ai) as f32 / 30.0;
        d / seconds
    }

    fn movement_event(
        a: dereth_input::ActionId,
        start: bool,
    ) -> dereth_client_runtime::actions::Action {
        dereth_client_runtime::actions::Action {
            id: a,
            phase: if start {
                dereth_client_runtime::actions::ActionPhase::Begin
            } else {
                dereth_client_runtime::actions::ActionPhase::End
            },
            extent: 1.0,
            repeats: 0,
        }
    }

    /// A settled body that runs rather than walks, plus the command interpreter that drives it.
    /// Running is the default on every shipped character, so the key walks you.
    pub fn running_body() -> (
        Character,
        dereth_client::character::MovementCommands,
        CharacterInput,
    ) {
        use dereth_client_runtime::actions::movement::{action, on_action};
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let c = settled_body(&store);
        let mut mc = dereth_client::character::MovementCommands::default();
        let mut input = CharacterInput::default();
        mc.ui_toggles_run = true;
        assert!(mc.on_action(
            on_action(&movement_event(action::TOGGLE_RUN_WALK, false), |_| None),
            &mut input
        ));
        assert!(
            input.run,
            "running is the default state and the key walks you"
        );
        let _ = mc.take_control_retake_pending();
        (c, mc, input)
    }

    /// One key event through the application's own path: the interpreter, then its drain of the
    /// body-side half of taking control back. Both halves, in that order, are what a frame does.
    pub fn key(
        c: &mut Character,
        mc: &mut dereth_client::character::MovementCommands,
        input: &mut CharacterInput,
        a: dereth_input::ActionId,
        down: bool,
    ) {
        use dereth_client_runtime::actions::movement::on_action;
        assert!(
            mc.on_action(on_action(&movement_event(a, down), |_| None), input),
            "the interpreter consumes every movement action it is handed"
        );
        if mc.take_control_retake_pending() {
            c.take_control_from_server();
        }
        c.input = *input;
    }

    /// A shard-authored movement, applied exactly as the object stream applies one: the style
    /// word first, then the interpreted state, then the latch the frame drains into losing
    /// control. Nothing is synthesised; the buffer came off a recording.
    pub fn server_takes_control(
        c: &Character,
        mc: &mut dereth_client::character::MovementCommands,
        input: &mut CharacterInput,
        buf: &dereth_protocol::movement::MovementBuffer,
    ) {
        let state = buf.body.interpreted.as_ref().map(interpreted_state);
        let style = dereth_animation::MotionCommand::from_index(buf.body.current_style)
            .or_else(|| state.as_ref().map(|s| s.current_style));
        if let Some(style) = style {
            c.driver_mut().apply_movement_style(style);
        }
        if let Some(state) = state.as_ref() {
            c.driver_mut().move_to_interpreted_state(state, true);
        }
        mc.lose_control_to_server(input);
    }

    /// Teleport the body `dz` upwards and let it fall back.
    ///
    /// The fall is asserted as well as the landing: a body that kept the contact of wherever it
    /// had been standing would hang in the air for ever while still answering that it is on the
    /// ground, and a landing assertion alone cannot tell that apart from a real fall.
    pub fn hop(
        c: &mut Character,
        mc: &mut dereth_client::character::MovementCommands,
        input: &mut CharacterInput,
        dz: f32,
    ) {
        use dereth_primitives::{Frame, Position, Vec3};
        let here = c.position();
        c.teleport(Position::new(
            here.cell,
            Frame::new(
                Vec3 {
                    x: here.frame.origin.x,
                    y: here.frame.origin.y,
                    z: here.frame.origin.z + dz,
                },
                here.frame.rotation,
            ),
        ));
        let up = c.position().frame.origin.z;
        assert!(
            up > here.frame.origin.z + dz * 0.5,
            "the teleport put him up there: {up}"
        );
        let _ = drive(c, mc, input, 60, 90);
        assert!(c.on_ground(), "he landed");
        let down = c.position().frame.origin.z;
        assert!(
            down < up - dz * 0.5,
            "...and he really fell to get there: {up} -> {down}"
        );
    }

    // ---------------------------------------------------------------------------------------
    // A body walking an approach
    // ---------------------------------------------------------------------------------------

    /// A settled body with a frame counter, for the scenarios about approaches and follows.
    pub struct Approaching {
        pub c: Character,
        frame: u32,
    }

    impl Approaching {
        pub fn new(store: &Arc<RetailDatStore>) -> Self {
            Self {
                c: settled_body(store),
                frame: 60,
            }
        }

        pub fn frames(&mut self, count: u32) {
            for _ in 0..count {
                self.frame += 1;
                self.c.update(LocalTime(f64::from(self.frame) / 30.0));
            }
        }

        /// Start walking to a point twenty metres away and get properly under way, so that what
        /// a scenario interrupts is a live approach and not one that has already finished.
        pub fn approach(&mut self) {
            use dereth_animation::motion::{MoveToRequest, MovementParameters};
            let before = self.c.position();
            let mut goal = before;
            goal.frame.origin.y += 20.0;
            self.c.perform_move_to(
                &MoveToRequest::MoveToPosition { pos: goal },
                &MovementParameters::default(),
                Some(1.0),
            );
            self.frames(20);
            assert!(
                self.c.is_moving_to(),
                "the approach must still be unfinished"
            );
            assert!(
                dereth_animation::motion::moveto::distance(&before, &self.c.position()) > 0.1,
                "and the body must really be walking it"
            );
            assert_ne!(
                self.c
                    .driver()
                    .movement
                    .interp
                    .interpreted_state
                    .forward_command,
                dereth_animation::MotionCommand::READY,
                "the real animation interpreter must be walking"
            );
        }

        /// The body is free afterwards: it walks when the player asks, stops when he lets go, and
        /// stays stopped. Answers whether all of that held.
        pub fn next_input_and_release(&mut self) -> bool {
            use dereth_animation::MotionCommand;
            self.c.input = CharacterInput::default();
            self.frames(90);
            let before = self.c.position();
            self.c.input.forward = true;
            self.frames(45);
            let walked = dereth_animation::motion::moveto::distance(&before, &self.c.position())
                > 0.5
                && self.c.driver().movement.interp.raw_state.forward_command
                    == MotionCommand::WALK_FORWARD;
            self.c.input = CharacterInput::default();
            self.frames(60);
            let stopped = self.c.position();
            let released = !self.c.is_moving_to()
                && self
                    .c
                    .driver()
                    .movement
                    .interp
                    .interpreted_state
                    .forward_command
                    == MotionCommand::READY;
            self.frames(60);
            let stayed =
                dereth_animation::motion::moveto::distance(&stopped, &self.c.position()) < 0.05;
            walked && released && stayed
        }
    }

    // ---------------------------------------------------------------------------------------
    // Replaying a recording for its teleports
    // ---------------------------------------------------------------------------------------

    /// One teleport the object stream reported.
    #[derive(Debug, Clone, Copy)]
    pub struct Teleported {
        pub pos: dereth_primitives::Position,
    }

    /// What one recording's replay measured.
    pub struct Replayed {
        /// Position messages about the player -- the denominator.
        pub player_positions: u64,
        /// The teleports the client applied, in order.
        pub teleports: Vec<Teleported>,
        /// The body, when one was asked for: stood up the way the client stands one up at world
        /// entry, at the first position the shard gives the player, and moved by nothing except
        /// the client's own teleport step.
        pub body: Option<Character>,
        /// Where that body was stood up, i.e. the shard's first word on where the player is.
        ///
        /// Answered rather than logged: a body built at the destination would satisfy every
        /// assertion about a teleport without one ever happening.
        pub first_position: Option<dereth_primitives::Position>,
    }

    /// Replay `session` through the production endpoint, object stream and teleport step,
    /// stopping after the `stop_after`th teleport when one is named.
    ///
    /// **The feed loop is the one in `dereth_testkit::login`, narrowed.** That module is private to
    /// the crate, so this is the smallest copy of it rather than a call. Nothing here binds a
    /// socket: the endpoint is the replay one and only the recording's server-to-client datagrams
    /// are fed.
    pub fn replay_teleports(
        session: &str,
        store: Option<&Arc<RetailDatStore>>,
        stop_after: Option<usize>,
    ) -> Replayed {
        use dereth_client_net::client_session::testing::capture::peer;
        use dereth_client_net::client_session::SessionEvent;

        // The recording and the endpoint are `dereth_testkit::replay`'s. What stays
        // here is the loop *body*, which accumulates this scenario's own state and is not the
        // shared one.
        let recs = dereth_testkit::replay::records(session);
        let mut net = dereth_testkit::replay::recorded_endpoint(&recs);
        let mut objects = dereth_client::objects::ObjectStream::new();
        let mut entered = false;
        let mut out: Vec<Teleported> = Vec::new();
        let mut body: Option<Character> = None;
        let mut first_position = None;

        for r in &recs {
            let now = LocalTime(r.t);
            if !r.c2s {
                net.feed(&r.raw, peer(r.pair), now);
            }
            net.tick(now);
            let _ = net.take_outgoing();
            for e in objects.pump(&mut net, now) {
                if let SessionEvent::CharacterSet(set) = &e {
                    if !entered {
                        if let Some(c) = set.characters.first() {
                            let account = set.account.clone();
                            net.enter_world(c.gid, &account);
                            entered = true;
                        }
                    }
                }
            }
            // The client's own world entry: stand the body where the shard says the player is,
            // once, as soon as it says it.
            if let (Some(store), None) = (store, body.as_ref()) {
                let start = objects
                    .player()
                    .and_then(|id| objects.presence(id))
                    .and_then(|p| p.position);
                if let Some(pos) = start {
                    body = Some(body_at(store, pos));
                    first_position = Some(pos);
                }
            }
            // The client's own teleport step, once per frame.
            let applied = match body.as_mut() {
                Some(c) => dereth_client::app::apply_player_teleport(&mut objects, c),
                None => objects.take_player_teleport(),
            };
            if let Some(pos) = applied {
                out.push(Teleported { pos });
                if stop_after == Some(out.len()) {
                    break;
                }
            }
        }
        Replayed {
            player_positions: objects.stats.player_positions,
            teleports: out,
            body,
            first_position,
        }
    }

    /// Stand a body up the way the client does at world entry: build it in the piece of land the
    /// shard named and put it on the shard's own position.
    fn body_at(store: &Arc<RetailDatStore>, pos: dereth_primitives::Position) -> Character {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a cell id's top sixteen bits are its piece of land. Not a float cast.
        let landblock = (pos.cell.0 >> 16) as u16;
        let mut c = Character::new(store, &region, landblock, (96.0, 96.0))
            .expect("the character is created");
        c.teleport(pos);
        c
    }

    /// The cell a position report names, whichever of the two kinds of report it is.
    pub fn reported_cell(payload: &[u8]) -> u32 {
        let a = unpack_action(payload).expect("the producer emits game actions");
        match a.sub_type.0 {
            AUTONOMOUS_POSITION => {
                let mut body = a.body;
                <dereth_protocol::movement::MovementAutonomousPosition as Message>::read(&mut body)
                    .expect("the report decodes")
                    .0
                    .position
                    .objcell_id
            }
            MOVE_TO_STATE => {
                let mut body = a.body;
                <dereth_protocol::movement::MovementMoveToState as Message>::read(&mut body)
                    .expect("the report decodes")
                    .0
                    .position
                    .objcell_id
            }
            other => panic!("a position report of an unexpected kind: {other:#06X}"),
        }
    }

    /// The first recording that carries a teleport, and how many it carries. Searched rather than
    /// named, so that a corpus that grows is measured rather than ignored.
    pub fn a_recording_with_a_teleport() -> (&'static str, usize) {
        for name in dereth_client_net::client_session::testing::session_names() {
            let n = replay_teleports(name, None, None).teleports.len();
            if n > 0 {
                return (name, n);
            }
        }
        panic!("no recording carries a teleport of the player at all")
    }

    /// The recording that carries the most of them.
    pub fn the_recording_with_the_most_teleports() -> &'static str {
        let mut best = ("", 0usize);
        for name in dereth_client_net::client_session::testing::session_names() {
            let n = replay_teleports(name, None, None).teleports.len();
            if n > best.1 {
                best = (name, n);
            }
        }
        assert!(
            best.1 > 0,
            "no recording carries a teleport of the player at all"
        );
        best.0
    }

    /// A recording and the ordinal of one of its teleports whose destination is **inside a
    /// room** -- a cell of a building rather than a patch of open land. That is the case the
    /// destination's geometry has to be fetched for.
    pub fn a_recorded_teleport_into_a_room() -> (&'static str, usize) {
        for name in dereth_client_net::client_session::testing::session_names() {
            let r = replay_teleports(name, None, None);
            for (i, t) in r.teleports.iter().enumerate() {
                if t.pos.cell.0 & 0xFFFF >= 0x100 {
                    return (name, i + 1);
                }
            }
        }
        panic!("no recording teleports the player into a room")
    }

    // ---------------------------------------------------------------------------------------
    // The training-dungeon door
    // ---------------------------------------------------------------------------------------

    /// The door the recording's own player opens.
    const DOOR: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x77f0_3033);
    /// The recording the door journey is taken from.
    const DOOR_SESSION: &str = "early-inventory-and-casting";
    /// The shipped setup the door's body is built from.
    const DOOR_SETUP: dereth_primitives::DataId = dereth_primitives::DataId(0x0200_05da);
    /// The open state the recording's own create carries.
    const DOOR_OPEN_STATE: u32 = 0x0001_001c;
    /// The piece of land the academy's rooms live in.
    const ACADEMY: u16 = 0x7f03;

    fn wire_position(p: &dereth_protocol::types::PositionWire) -> dereth_primitives::Position {
        use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};
        Position::new(
            CellId(p.objcell_id),
            Frame::new(
                Vec3::new(p.frame.origin.x, p.frame.origin.y, p.frame.origin.z),
                Quat::new(
                    p.frame.orientation.w,
                    p.frame.orientation.x,
                    p.frame.orientation.y,
                    p.frame.orientation.z,
                ),
            ),
        )
    }

    /// The recorded player's own place, the recorded door's place, and the movement the shard
    /// sent when the player used it -- all three out of the recording, none of them written here.
    fn recorded_door() -> (
        dereth_primitives::Position,
        dereth_primitives::Position,
        dereth_protocol::movement::MovementBuffer,
    ) {
        let rows = Corpus::load(DOOR_SESSION)
            .expect("the recording parses")
            .expect("the recording is committed to the repository")
            .blobs;
        let row = |i: usize| {
            rows.iter()
                .find(|r| r.idx == i)
                .unwrap_or_else(|| panic!("the recording has no {i}"))
        };
        let mut a = dereth_protocol::actions::unpack_action(&row(143).payload)
            .expect("the recorded placement unpacks");
        let state = <dereth_protocol::movement::MovementMoveToState as Message>::read(&mut a.body)
            .expect("it decodes")
            .0;
        let create = <dereth_protocol::objects::ItemCreateObject as Message>::read(
            &mut dereth_protocol::Reader::new(&row(52).payload[4..]),
        )
        .expect("the recorded door create decodes")
        .0;
        assert_eq!(create.id, DOOR, "the recorded create is the door's");
        let m = <dereth_protocol::movement::MovementSetObjectMovement as Message>::read(
            &mut dereth_protocol::Reader::new(&row(147).payload[4..]),
        )
        .expect("the recorded answer decodes");
        let buf = m.decoded_movement().expect("its movement buffer decodes");
        (
            wire_position(&state.position),
            wire_position(&create.physicsdesc.position.expect("the door has a place")),
            buf,
        )
    }

    /// A body standing where the recording's own player stood, in the academy, with the command
    /// interpreter that drives it.
    pub struct DoorRig {
        pub c: Character,
        pub mc: dereth_client::character::MovementCommands,
        pub input: CharacterInput,
        t: u32,
        pub door_pos: dereth_primitives::Position,
    }

    impl DoorRig {
        pub fn new() -> Self {
            use dereth_client_runtime::actions::movement::{action, on_action};
            let (player, door_pos, _) = recorded_door();
            let store = Arc::new(dereth_dat::testing::open_store_or_fail());
            let region = dereth_client::world::load_region(&store).expect("the region decodes");
            let mut c = Character::new(&store, &region, ACADEMY, (96.0, 96.0))
                .expect("the body is created");
            c.land().load_block_cells(player.cell.landblock());
            c.teleport(player);
            c.stop_completely_from_action();
            for i in 1..=90 {
                c.update(LocalTime(f64::from(i) / 30.0));
            }
            assert!(
                c.on_ground(),
                "the recorded spot the player stood at must support a body"
            );
            let mut mc = dereth_client::character::MovementCommands::default();
            let mut input = CharacterInput::default();
            mc.ui_toggles_run = true;
            assert!(mc.on_action(
                on_action(&movement_event(action::TOGGLE_RUN_WALK, false), |_| None),
                &mut input
            ));
            let _ = mc.take_control_retake_pending();
            Self {
                c,
                mc,
                input,
                t: 90,
                door_pos,
            }
        }

        /// One key edge, through the application's own two halves.
        pub fn key(&mut self, a: dereth_input::ActionId, down: bool) {
            use dereth_client_runtime::actions::movement::on_action;
            assert!(self.mc.on_action(
                on_action(&movement_event(a, down), |_| None),
                &mut self.input
            ));
            if self.mc.take_control_retake_pending() {
                self.c.take_control_from_server();
            }
            self.c.input = self.input;
        }

        /// One frame: the retake, then the body. `deliver` says where the door is whenever the
        /// client asks, the way the object table answers it.
        fn frame(&mut self, deliver: Option<bool>) {
            let (pending, moving) = {
                let d = self.c.driver();
                (d.movement.motions_pending(), d.movement.is_moving_to())
            };
            if self.mc.use_time(pending, moving, &mut self.input) {
                self.c.take_control_from_server();
            }
            self.c.input = self.input;
            if let Some(ok) = deliver {
                if self.c.wanted_target().is_some() {
                    let p = self.door_pos;
                    self.c.update_target(p, dereth_primitives::Vec3::ZERO, ok);
                }
            }
            self.t += 1;
            self.c.update(LocalTime(f64::from(self.t) / 30.0));
        }

        pub fn run(&mut self, n: u32, deliver: Option<bool>) {
            for _ in 0..n {
                self.frame(deliver);
            }
        }

        /// The shard's recorded answer to using the door, applied the way the object stream
        /// applies one, followed by the handover of control.
        pub fn door_turn(&mut self) {
            use dereth_animation::motion::{MoveToRequest, MovementParameters};
            let (_, _, buf) = recorded_door();
            let style = dereth_animation::MotionCommand::from_index(buf.body.current_style)
                .expect("the recorded answer names a way of carrying the body");
            self.c.apply_movement_style(style);
            let Some(dereth_protocol::movement::MoveToArm::TurnToObject {
                target,
                desired_heading,
                params,
            }) = buf.body.decode_move_to().expect("the recorded arm decodes")
            else {
                panic!("the recorded answer is not the door's turn")
            };
            let dereth_protocol::movement::MovementParameters::TurnTo {
                bitfield, speed, ..
            } = params
            else {
                panic!("the recorded turn carries the wrong parameters")
            };
            let p = MovementParameters {
                flags: bitfield,
                speed,
                desired_heading,
                ..MovementParameters::default()
            };
            self.c.perform_move_to(
                &MoveToRequest::TurnToObject {
                    object_id: target,
                    top_level_id: target,
                },
                &p,
                None,
            );
            self.lose_control();
        }

        /// The shard-directed approach, which is how a player reaches a door out of reach.
        pub fn door_approach(&mut self, at: dereth_primitives::Position, radius: f32) {
            use dereth_animation::motion::{flags, MoveToRequest, MovementParameters};
            self.door_pos = at;
            let p = MovementParameters {
                distance_to_object: 0.6,
                flags: flags::STICKY,
                ..MovementParameters::default()
            };
            self.c.perform_move_to(
                &MoveToRequest::MoveToObject {
                    object_id: DOOR,
                    top_level_id: DOOR,
                    radius,
                    height: 2.0,
                },
                &p,
                Some(1.0),
            );
            self.lose_control();
            assert!(self.c.is_moving_to(), "the approach was installed");
            // The client asks where the door is on a half-second gate, so the walk waits a good
            // fifteen frames for its first answer before it can begin.
            for _ in 0..600 {
                self.frame(Some(true));
                if !self.c.is_moving_to() {
                    break;
                }
            }
        }

        fn lose_control(&mut self) {
            let Self { c, mc, input, .. } = self;
            mc.lose_control_to_server_with_finish(input, || c.finish_jump());
            self.c.input = self.input;
        }

        /// A shard-authored action, the way the object stream delivers one.
        pub fn server_action(&mut self, act: dereth_animation::MotionCommand) {
            use dereth_animation::motion::{ActionNode, InterpretedMotionState};
            let state = InterpretedMotionState {
                actions: vec![ActionNode {
                    action: act,
                    speed: 1.0,
                    stamp: 1,
                    autonomous: false,
                }]
                .into(),
                ..InterpretedMotionState::default()
            };
            self.c.move_to_interpreted_state(&state);
            self.lose_control();
        }

        fn heading(&self) -> f32 {
            dereth_physics::math::get_heading(&self.c.position().frame)
        }

        /// A fresh forward press held for `n` frames: how far did the body actually go?
        pub fn probe_translate(&mut self, n: u32) -> f32 {
            use dereth_client_runtime::actions::movement::action;
            let from = self.c.position();
            self.key(action::MOVE_FORWARD, true);
            self.run(n, Some(true));
            let d = dereth_animation::motion::moveto::distance(&from, &self.c.position());
            self.key(action::MOVE_FORWARD, false);
            self.run(15, Some(true));
            d
        }

        /// Forward, and if that does not move him, backward. A body that can go **neither** way
        /// is the report; one that is merely walled in front of him is not.
        pub fn probe_translate_either_way(&mut self, n: u32) -> f32 {
            use dereth_client_runtime::actions::movement::action;
            let f = self.probe_translate(n);
            if f > 0.25 {
                return f;
            }
            let from = self.c.position();
            self.key(action::MOVE_BACKWARD, true);
            self.run(n, Some(true));
            let b = dereth_animation::motion::moveto::distance(&from, &self.c.position());
            self.key(action::MOVE_BACKWARD, false);
            self.run(15, Some(true));
            f.max(b)
        }

        /// A fresh turn press held for `n` frames: how far did the heading actually go?
        pub fn probe_turn(&mut self, n: u32) -> f32 {
            use dereth_client_runtime::actions::movement::action;
            let from = self.heading();
            self.key(action::TURN_RIGHT, true);
            self.run(n, Some(true));
            let d = dereth_animation::motion::heading_diff(
                self.heading(),
                from,
                dereth_animation::MotionCommand::TURN_RIGHT,
            );
            self.key(action::TURN_RIGHT, false);
            self.run(15, Some(true));
            d.min(360.0 - d).abs().max(d.min(360.0 - d))
        }

        /// Point the body at the door and hold forward until it is in the door's own room.
        pub fn walk_into_the_doorway(&mut self) {
            use dereth_client_runtime::actions::movement::action;
            let to = self.door_pos;
            let want = dereth_animation::motion::moveto::position_heading(&self.c.position(), &to);
            let mut here = self.c.position();
            dereth_physics::math::set_heading(&mut here.frame, want);
            self.c.teleport(here);
            self.run(10, Some(true));
            self.key(action::MOVE_FORWARD, true);
            for _ in 0..120 {
                self.frame(Some(true));
                if self.c.position().cell == to.cell {
                    break;
                }
            }
            self.key(action::MOVE_FORWARD, false);
            self.run(20, Some(true));
        }

        /// The recorded door as a body in the player's own world, so he can collide with it.
        pub fn register_door(
            &mut self,
            at: dereth_primitives::Position,
        ) -> dereth_physics::PhysHandle {
            use dereth_assets::{Decode, Setup};
            let store = Arc::new(dereth_dat::testing::open_store_or_fail());
            let bytes = store
                .read_typed(dereth_dat::DbType::Setup, DOOR_SETUP)
                .expect("the door's shipped setup");
            let s = Setup::decode_payload(DOOR_SETUP, &bytes).expect("it decodes");
            let mut stats = dereth_client::object_physics::SetupPartStats::default();
            let g = Arc::new(dereth_client::object_physics::setup_geometry_with_parts(
                &store, &s, &mut stats,
            ));
            let h = self.c.world.create(DOOR, g, true);
            self.c.world.enter_cell(h, at.cell);
            if let Some(o) = self.c.world.get_mut(h) {
                o.state = dereth_physics::PhysicsState(o.state.0 | DOOR_OPEN_STATE);
                o.set_frame(at.frame);
                o.position = at;
            }
            assert!(
                self.c
                    .world
                    .get(h)
                    .expect("the door is live")
                    .state
                    .has_physics_bsp(),
                "the door keeps the shape it is collided against"
            );
            self.c.world.calc_cross_cells(h, true);
            h
        }
    }

    // ---------------------------------------------------------------------------------------
    // The shipped gameplay screen, and the map window on it
    // ---------------------------------------------------------------------------------------

    /// The real gameplay screen, built from the shipped layout, with no client around it.
    ///
    /// The claims in this section are about what a panel writes into the element tree, which the
    /// screen does for itself; building a whole client around it would only make the run slower.
    pub fn shipped_gameplay() -> (dereth_ui::UiSystem, Box<dyn dereth_ui::framework::Screen>) {
        use dereth_ui::framework::DidMapperResolver;
        use dereth_ui_screens::screens::gameplay::GamePlayScreen;

        let dir = dereth_dat::testing::dat_dir();
        assert!(
            dereth_dat::testing::have_dats(),
            "the shipped layouts are this scenario's oracle and are absent at {}",
            dir.display()
        );
        let store = RetailDatStore::open_dir(&dir).expect("the retail data files open");
        let master_id = dereth_primitives::DataId(0x3900_0001);
        let bytes = dereth_primitives::AssetSource::read(&store, master_id)
            .expect("the shipped property table");
        let master = <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(
            master_id, &bytes,
        )
        .expect("it decodes");

        let mut ui = dereth_ui::UiSystem::new((800, 600));
        ui.property_types = master.property_types();
        let mut flow = dereth_ui::UiFlow::new();
        dereth_ui_screens::register_all(&mut ui, &mut flow);
        let store = std::rc::Rc::new(store);
        let resolver = std::rc::Rc::new(
            DidMapperResolver::load_via_master(store.as_ref()).expect("the id mapper"),
        );
        dereth_ui_screens::env::install(&mut ui, store, resolver);

        let mut screen = GamePlayScreen::create_screen();
        screen
            .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .expect("the gameplay screen creates from its real layout");
        ui.requests.clear();
        (ui, screen)
    }

    pub fn as_gameplay(
        screen: &mut Box<dyn dereth_ui::framework::Screen>,
    ) -> &mut dereth_ui_screens::screens::gameplay::GamePlayScreen {
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen")
    }

    /// Whether the player is outside, where he is and what the clock says -- and nothing else.
    /// Every other question the panels can ask keeps its default answer.
    #[derive(Debug, Clone)]
    pub struct MapView {
        pub outside: bool,
        pub coords: Option<(f32, f32)>,
        pub date_time: Option<(String, String)>,
    }

    impl MapView {
        /// Holtburg's own reading, which is what the coordinate line is calibrated against.
        pub const NORTH: f32 = 42.2;
        pub const EAST: f32 = 33.8;
        pub const EXPECTED_COORD_LINE: &'static str = "42.2N, 33.8E";
        pub const EXPECTED_DATE_LINE: &'static str =
            "Date: Morningthaw 14, 10 P.Y.\nTime: Late Morning";
        pub const EXPECTED_LATER_DATE_LINE: &'static str =
            "Date: Morningthaw 15, 10 P.Y.\nTime: Night";

        pub fn outdoors() -> Self {
            Self {
                outside: true,
                coords: Some((Self::NORTH, Self::EAST)),
                date_time: Some((
                    "Morningthaw 14, 10 P.Y.".to_owned(),
                    "Late Morning".to_owned(),
                )),
            }
        }

        /// In a room, a game-day later. The client cannot work out coordinates for a room, so
        /// they are unavailable as well as not asked for -- both halves, as in the client.
        pub fn indoors_later() -> Self {
            Self {
                outside: false,
                coords: None,
                date_time: Some(("Morningthaw 15, 10 P.Y.".to_owned(), "Night".to_owned())),
            }
        }
    }

    impl dereth_ui_screens::view::GameView for MapView {
        fn player(&self) -> Option<dereth_primitives::ObjectId> {
            Some(dereth_primitives::ObjectId(0x5000_0001))
        }
        fn player_outside(&self) -> bool {
            self.outside
        }
        fn player_coords(&self) -> Option<(f32, f32)> {
            self.coords
        }
        fn game_date_time(&self) -> Option<(String, String)> {
            self.date_time.clone()
        }
    }

    /// The whole observable surface of the map window's three elements, off the live tree.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct MapPixels {
        pub date_text: String,
        pub coord_text: String,
        pub icon_box: (i32, i32, i32, i32),
        pub icon_visible: bool,
        pub house_visible: bool,
    }

    fn text_of(ui: &mut dereth_ui::UiSystem, h: Option<dereth_ui::ElemHandle>) -> String {
        match h.and_then(|h| ui.text_element_mut(h)) {
            Some(t) => t.glyphs.inq_text(false),
            None => String::new(),
        }
    }

    pub fn element_is_visible(ui: &dereth_ui::UiSystem, h: Option<dereth_ui::ElemHandle>) -> bool {
        h.and_then(|h| ui.node(h))
            .is_some_and(|n| n.region.flags.visible)
    }

    pub fn sample_map(
        ui: &mut dereth_ui::UiSystem,
        g: &dereth_ui_screens::screens::gameplay::GamePlayScreen,
    ) -> MapPixels {
        let b = g
            .map
            .player_icon
            .and_then(|h| ui.node(h))
            .map(|n| n.region.box_);
        MapPixels {
            date_text: text_of(ui, g.map.date_time_text),
            coord_text: text_of(ui, g.map.coordinate_text),
            icon_box: b.map_or((0, 0, 0, 0), |b| (b.x0, b.y0, b.x1, b.y1)),
            icon_visible: element_is_visible(ui, g.map.player_icon),
            house_visible: element_is_visible(ui, g.map.house_icon),
        }
    }

    // ---------------------------------------------------------------------------------------
    // The world-map page
    // ---------------------------------------------------------------------------------------

    /// The same shipped gameplay screen as [`shipped_gameplay`], as the concrete screen and with
    /// the layout assets installed, which is what the tooltip window needs to exist at all.
    ///
    /// Without them the shell answers "no tooltip" and a scenario about a town's name cannot
    /// tell that apart from a machine with no data files.
    pub fn shipped_map_page() -> (
        dereth_ui::UiSystem,
        dereth_ui_screens::screens::gameplay::GamePlayScreen,
    ) {
        use dereth_ui::framework::{DidMapperResolver, Screen as _};
        use dereth_ui_screens::screens::gameplay::GamePlayScreen;

        let dir = dereth_dat::testing::dat_dir();
        assert!(
            dereth_dat::testing::have_dats(),
            "the shipped layouts are this scenario's oracle and are absent at {}",
            dir.display()
        );
        let store = RetailDatStore::open_dir(&dir).expect("the retail data files open");
        let master_id = dereth_primitives::DataId(0x3900_0001);
        let bytes = dereth_primitives::AssetSource::read(&store, master_id)
            .expect("the shipped property table");
        let master = <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(
            master_id, &bytes,
        )
        .expect("it decodes");

        let mut ui = dereth_ui::UiSystem::new((800, 600));
        ui.property_types = master.property_types();
        let mut flow = dereth_ui::UiFlow::new();
        dereth_ui_screens::register_all(&mut ui, &mut flow);
        let store = std::rc::Rc::new(store);
        let resolver = std::rc::Rc::new(
            DidMapperResolver::load_via_master(store.as_ref()).expect("the id mapper"),
        );
        dereth_ui_screens::env::install(
            &mut ui,
            std::rc::Rc::clone(&store) as std::rc::Rc<dyn dereth_primitives::AssetSource>,
            resolver,
        );
        ui.assets = Some(std::rc::Rc::clone(&store) as std::rc::Rc<dyn dereth_ui::LayoutAssets>);

        let mut screen = GamePlayScreen::default();
        screen
            .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .expect("the gameplay screen creates from its real layout");
        pump_screen(&mut ui, &mut screen);
        ui.requests.clear();
        (ui, screen)
    }

    /// The shell's own message delivery, bounded.
    pub fn pump_screen(
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
    }

    /// The panel the map page lives on, read off the shipped layout rather than written down.
    fn maps_panel_id(g: &dereth_ui_screens::screens::gameplay::GamePlayScreen) -> u32 {
        g.panels
            .pages
            .iter()
            .find(|p| p.element == dereth_ui::ElementId(0x1000_018C))
            .expect("the maps page is one of the panel's own pages")
            .panel_id
    }

    /// Open the map page the way a toolbar button does.
    pub fn open_the_map_page(
        ui: &mut dereth_ui::UiSystem,
        g: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
    ) {
        let panel = maps_panel_id(g);
        g.recv_set_panel_visibility(ui, panel, true);
        pump_screen(ui, g);
    }

    /// ...and close it again.
    pub fn close_the_map_page(
        ui: &mut dereth_ui::UiSystem,
        g: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
    ) {
        let panel = maps_panel_id(g);
        g.recv_set_panel_visibility(ui, panel, false);
        pump_screen(ui, g);
    }

    /// One town on the map, as the element tree holds it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TownNote {
        pub box_: (i32, i32, i32, i32),
        pub tip: Option<String>,
    }

    pub fn map_notes(
        ui: &dereth_ui::UiSystem,
        g: &dereth_ui_screens::screens::gameplay::GamePlayScreen,
    ) -> Vec<TownNote> {
        g.map_notes
            .iter()
            .filter_map(|h| ui.node(*h))
            .map(|n| TownNote {
                box_: (
                    n.region.box_.x0,
                    n.region.box_.y0,
                    n.region.box_.x1,
                    n.region.box_.y1,
                ),
                tip: n.tooltip_text.clone(),
            })
            .collect()
    }

    /// Where the client puts the player's own mark, written out from the instructions rather
    /// than by calling the thing under test -- so the assertion is not that function spelled
    /// twice.
    ///
    /// The coordinate is scaled by ten and biased by a thousand and twenty-four, multiplied by
    /// the marker box's own size, divided by two thousand and forty-eight and truncated towards
    /// zero; the north arm subtracts the biased value from two thousand and forty-seven first.
    /// The mark's own size -- one more than the difference of its edges -- is halved and taken
    /// off. That last term is the one the client and the measurement of it once both had one
    /// short, where the error cancels sideways and does not vertically.
    pub fn marker_oracle(
        area: (i32, i32, i32, i32),
        east: f32,
        north: f32,
        w: i32,
        h: i32,
    ) -> (i32, i32) {
        let (x0, x1, y0, y1) = area;
        let aw = f64::from(x1 - x0 + 1);
        let ah = f64::from(y1 - y0 + 1);
        let biased = |v: f32| f64::from(v) * 10.0 + 1024.0;
        #[allow(clippy::cast_possible_truncation)]
        let trunc = |v: f64| v.trunc() as i32;
        (
            x0 - w / 2 + trunc(biased(east) * aw / 2048.0),
            y0 - h / 2 + trunc((2047.0 - biased(north)) * ah / 2048.0),
        )
    }

    // ---------------------------------------------------------------------------------------
    // Replaying a recording for the world it built
    // ---------------------------------------------------------------------------------------

    /// The object stream a recording had built at its **busiest** instant.
    ///
    /// The peak and not the end: every recording finishes with a logout that destroys every
    /// object, so a question about what the client had to draw has to be asked while it had it.
    ///
    /// **The feed loop is [`replay_teleports`]'s**, which is `dereth_testkit::login`'s, narrowed
    /// again. Nothing here binds a socket.
    pub fn replay_objects_at_peak(session: &str) -> dereth_client::objects::ObjectStream {
        let peak = feed(session, None).1;
        feed(session, Some(peak)).0
    }

    /// The instants of a recording a scenario may want to ask a question at.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct Stations {
        /// `(record index, how many things the world held)` -- the peak, wherever the player was.
        pub busiest: (usize, usize),
        /// The busiest instant at which the player's own room was a piece of open land.
        pub outdoor: Option<(usize, dereth_primitives::CellId, usize)>,
        /// The busiest instant at which it was inside a building.
        pub indoor: Option<(usize, dereth_primitives::CellId, usize)>,
    }

    /// The three instants above, for one recording.
    pub fn radar_stations(session: &str) -> Stations {
        feed(session, None).3
    }

    /// The world a recording had built at `stop`, with a display synced to the recording's **own**
    /// recorded player position -- which is where every room id a scenario reads comes from.
    pub fn scene_at(
        session: &str,
        stop: usize,
    ) -> (
        dereth_client::objects::ObjectStream,
        dereth_client::hud::Hud,
    ) {
        let objects = feed(session, Some(stop)).0;
        let player = objects.world.player.expect("the recording named a player");
        let pos = objects
            .presence(player)
            .and_then(|p| p.position)
            .expect("the player has a place");
        let mut hud = dereth_client::hud::Hud::new();
        hud.sync(
            &objects,
            Some(dereth_client::hud::ViewerFrame {
                position: pos,
                heading_degrees: 0.0,
            }),
        );
        (objects, hud)
    }

    /// Replay `session` up to `stop` records, and answer the stream, the index and size of the
    /// busiest instant it passed through, and the stations it found on the way.
    fn feed(
        session: &str,
        stop: Option<usize>,
    ) -> (dereth_client::objects::ObjectStream, usize, usize, Stations) {
        use dereth_client_net::client_session::testing::capture::peer;
        use dereth_client_net::client_session::SessionEvent;

        // See `replay_teleports`: the loader and the endpoint are the harness's, the loop is
        // this scenario's.
        let recs = dereth_testkit::replay::records(session);
        let mut net = dereth_testkit::replay::recorded_endpoint(&recs);
        let mut objects = dereth_client::objects::ObjectStream::new();
        let mut entered = false;
        let mut best = (0usize, 0usize);
        let mut st = Stations::default();
        let limit = stop.unwrap_or(recs.len());
        for (i, r) in recs.iter().take(limit).enumerate() {
            let now = LocalTime(r.t);
            if !r.c2s {
                net.feed(&r.raw, peer(r.pair), now);
            }
            net.tick(now);
            let _ = net.take_outgoing();
            for e in objects.pump(&mut net, now) {
                if let SessionEvent::CharacterSet(set) = &e {
                    if !entered {
                        if let Some(c) = set.characters.first() {
                            let account = set.account.clone();
                            net.enter_world(c.gid, &account);
                            entered = true;
                        }
                    }
                }
            }
            let n = objects.world.tables.weenies.len();
            if n > best.1 {
                best = (i + 1, n);
            }
            st.busiest = best;
            // A question about the radar indoors has to be asked at an instant the player really
            // was indoors, which is never the busiest instant of any of these recordings.
            let here = objects.presences().count();
            if let Some(cell) = objects
                .world
                .player
                .and_then(|id| objects.presence(id))
                .and_then(|p| p.position)
                .map(|p| p.cell)
            {
                let slot = if dereth_physics::landdefs::is_outdoors(cell) {
                    &mut st.outdoor
                } else {
                    &mut st.indoor
                };
                if slot.is_none_or(|(_, _, most)| here > most) {
                    *slot = Some((i + 1, cell, here));
                }
            }
        }
        (objects, best.0, best.1, st)
    }

    /// The recording that had the most objects in the world at once.
    ///
    /// Searched rather than named, so that a corpus that grows is measured rather than ignored.
    pub fn the_busiest_recording() -> &'static str {
        let mut best = ("", 0usize);
        for name in dereth_client_net::client_session::testing::session_names() {
            let n = feed(name, None).2;
            if n > best.1 {
                best = (name, n);
            }
        }
        assert!(
            best.1 > 0,
            "no recording puts an object in the world at all"
        );
        best.0
    }

    // ---------------------------------------------------------------------------------------
    // The radar
    // ---------------------------------------------------------------------------------------

    /// Every recording that puts an object in the world and names a player.
    ///
    /// Searched rather than listed, so a corpus that grows is measured rather than ignored. It is
    /// cached because replaying eighteen recordings to find out is the expensive half of every
    /// radar scenario.
    pub fn recordings_with_a_world() -> &'static [&'static str] {
        static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
        NAMES.get_or_init(|| {
            let mut out = Vec::new();
            for name in dereth_client_net::client_session::testing::session_names() {
                let objects = replay_objects_at_peak_uncached(name);
                if objects.world.player.is_some() && objects.presences().count() > 1 {
                    out.push(*name);
                }
            }
            assert!(
                !out.is_empty(),
                "no recording builds a world with a player in it"
            );
            out
        })
    }

    fn replay_objects_at_peak_uncached(session: &str) -> dereth_client::objects::ObjectStream {
        let peak = feed(session, None).1;
        feed(session, Some(peak)).0
    }

    /// A geometry to project into. Nothing asserted about the radar depends on the numbers --
    /// every claim is about *which* things appear -- but they are the shipped ring's, so the
    /// projection runs at its real scale.
    pub fn radar_geometry() -> dereth_ui_screens::mapradar::radar::RadarGeometry {
        dereth_ui_screens::mapradar::radar::RadarGeometry {
            radius: 50,
            center: (60.0, 60.0),
        }
    }

    /// The radar list a recorded scene produces, with the player's own place as the viewer.
    pub fn radar_list(
        objects: &dereth_client::objects::ObjectStream,
    ) -> Vec<dereth_ui_screens::view::RadarEntry> {
        let player = objects.world.player.expect("the recording named a player");
        let pos = objects
            .presence(player)
            .and_then(|p| p.position)
            .expect("the player has a place");
        let mut hud = dereth_client::hud::Hud::new();
        hud.sync(
            objects,
            Some(dereth_client::hud::ViewerFrame {
                position: pos,
                heading_degrees: 0.0,
            }),
        );
        hud.radar.clone()
    }

    /// The colour a thing on the radar should take, worked out from the recording's own
    /// description rather than by asking the client -- so the two can disagree.
    ///
    /// The order is the client's own decision list: the shard's override first, then a portal,
    /// then a trader, then a creature, then the several kinds of player, then the colour that
    /// means nothing matched. The overrides a fellowship or an allegiance would apply are not
    /// modelled, because no recording in the corpus has the player in one.
    pub fn expected_blip_colour(pwd: &dereth_protocol::types::PublicWeenieDesc) -> u32 {
        use dereth_ui_screens::mapradar::radar::{
            semantic, BLUE, BRIGHT_GREEN, CYAN, GOLD, GREEN, PINK, PURPLE, RED, WHITE, YELLOW,
        };
        // A thing the shard hides from the interface takes no colour of its own.
        if pwd.bitfield & 0x80 != 0 {
            return semantic::DEFAULT.hex;
        }
        if let Some(v) = pwd.blip_color.filter(|v| *v != 0) {
            // The order the client's own switch reaches these in, which is **not** the order the
            // ten colours sit in the shipped data. Writing out the data's order instead is the
            // mistake this function exists to be able to make on its own, and it once did.
            return match v {
                1 => BLUE.hex,
                2 => GOLD.hex,
                3 => WHITE.hex,
                4 => PURPLE.hex,
                5 => RED.hex,
                6 => PINK.hex,
                7 => GREEN.hex,
                8 => YELLOW.hex,
                9 => CYAN.hex,
                10 => BRIGHT_GREEN.hex,
                _ => semantic::DEFAULT.hex,
            };
        }
        if pwd.bitfield & 0x0004_0000 != 0 {
            return semantic::PORTAL.hex;
        }
        if pwd.bitfield & 0x0000_0200 != 0 {
            return semantic::VENDOR.hex;
        }
        let is_player = pwd.bitfield & 0x0000_0008 != 0;
        let is_creature = pwd.obj_type & 0x0000_0010 != 0;
        if pwd.bitfield & 0x0000_0010 != 0 && is_creature && !is_player {
            return semantic::CREATURE.hex;
        }
        if !is_player {
            return semantic::DEFAULT.hex;
        }
        if pwd.bitfield & 0x0010_0000 != 0 && pwd.bitfield & 0x0000_0040 == 0 {
            semantic::ADMIN.hex
        } else if pwd.bitfield & 0x0000_0020 != 0 {
            semantic::PLAYER_KILLER.hex
        } else if pwd.bitfield & 0x0200_0000 != 0 {
            semantic::PK_LITE.hex
        } else if pwd.bitfield & 0x0020_0000 != 0 {
            semantic::CREATURE.hex
        } else {
            semantic::DEFAULT.hex
        }
    }

    // ---------------------------------------------------------------------------------------
    // The radar's range
    // ---------------------------------------------------------------------------------------

    /// How far the radar reaches out of doors, and inside. Written out as numbers rather than
    /// fetched through the same call the client makes: a scenario that read both sides through
    /// one symbol could not see a wrong one.
    pub const OUTDOOR_RANGE: f32 = 75.0;
    pub const INDOOR_RANGE: f32 = 25.0;
    /// The first room number that means "inside a building".
    pub const ENV_CELL_FLOOR: u32 = 0x100;

    /// The first recording that builds a world with a player in it.
    pub fn a_recording_with_a_world() -> &'static str {
        recordings_with_a_world()[0]
    }

    /// A recording, an instant of it, and the room -- the busiest moment at which some recorded
    /// player was inside a building.
    pub fn a_recorded_indoor_place() -> (&'static str, usize, dereth_primitives::CellId) {
        let mut best: Option<(&'static str, usize, dereth_primitives::CellId, usize)> = None;
        for session in recordings_with_a_world() {
            if let Some((at, cell, here)) = radar_stations(session).indoor {
                if best.is_none_or(|(_, _, _, most)| here > most) {
                    best = Some((session, at, cell, here));
                }
            }
        }
        let (s, at, cell, _) = best.expect("no recording ever puts the player inside a building");
        (s, at, cell)
    }

    /// The shell's own message delivery for a boxed screen, bounded.
    pub fn pump_boxed(
        ui: &mut dereth_ui::UiSystem,
        screen: &mut Box<dyn dereth_ui::framework::Screen>,
    ) {
        for _ in 0..16 {
            let out = ui.drain_outbox();
            if out.is_empty() {
                return;
            }
            for d in out {
                match d {
                    dereth_ui::Delivery::Element { msg, .. } => screen
                        .on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg),
                    dereth_ui::Delivery::Global { id, param, .. } => {
                        screen.on_global_message(
                            &mut dereth_ui::framework::ScreenCx::new(ui),
                            id,
                            param,
                        );
                    }
                    dereth_ui::Delivery::Notice { id, payload, .. } => {
                        screen.on_notice(
                            &mut dereth_ui::framework::ScreenCx::new(ui),
                            id,
                            &payload,
                        );
                    }
                }
            }
        }
        panic!("the message pump did not settle in sixteen rounds");
    }

    /// The picture one of an element's own states carries, read out of the shipped layout.
    pub fn state_image(
        ui: &dereth_ui::UiSystem,
        h: dereth_ui::ElemHandle,
        state: u32,
    ) -> Option<dereth_primitives::DataId> {
        let sd = ui
            .node(h)
            .expect("the element is alive")
            .desc
            .access_state(dereth_ui::StateId(state))
            .unwrap_or_else(|| panic!("the shipped layout gives it no state {state:#x}"));
        match sd.media.first().map(|m| &m.fields) {
            Some(dereth_ui::desc::state_desc::MediaFields::Image { file, .. }) => Some(*file),
            other => panic!("that state carries {other:?}, not a picture"),
        }
    }

    // ---------------------------------------------------------------------------------------
    // The radar's own controls
    // ---------------------------------------------------------------------------------------

    /// The radar window on a shipped gameplay screen.
    #[allow(clippy::borrowed_box)]
    pub fn radar_element(
        ui: &dereth_ui::UiSystem,
        screen: &Box<dyn dereth_ui::framework::Screen>,
    ) -> dereth_ui::ElemHandle {
        let root = *screen.roots().first().expect("the screen has a root");
        ui.get_child_recursive(root, dereth_ui_screens::screens::gameplay::window::RADAR)
            .expect("the radar")
    }

    /// One press and release of the left button at a point.
    pub fn click_at(ui: &mut dereth_ui::UiSystem, x: i32, y: i32) {
        ui.mouse_move(dereth_primitives::LocalTime(1.0), x, y);
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    }

    /// One pass of the radar's own drawing, and what it drew.
    pub fn radar_blips(
        ui: &mut dereth_ui::UiSystem,
        screen: &mut Box<dyn dereth_ui::framework::Screen>,
        objects: &dereth_client::objects::ObjectStream,
        hud: &dereth_client::hud::Hud,
    ) -> Vec<dereth_ui_screens::mapradar::radar::Blip> {
        let g = as_gameplay(screen);
        let view = hud.view(objects);
        assert!(g.update_radar(ui, &view), "the radar wrote nothing at all");
        let geom = dereth_ui_screens::mapradar::radar::RadarGeometry {
            radius: g.radar.radius,
            center: g.radar.center,
        };
        dereth_ui_screens::mapradar::radar::draw_objects(
            &hud.radar,
            hud.radar.iter().find(|o| o.is_self),
            geom,
            dereth_ui_screens::mapradar::radar::radar_range(true),
            None,
            false,
        )
    }

    /// Where the client would put every thing on the radar, and which thing it is -- worked out
    /// here from the recording's own descriptions rather than by asking the client, so that the
    /// two can disagree.
    pub fn expected_blip_map(
        objects: &dereth_client::objects::ObjectStream,
        hud: &dereth_client::hud::Hud,
        screen: &mut Box<dyn dereth_ui::framework::Screen>,
    ) -> Vec<((i32, i32), dereth_primitives::ObjectId)> {
        let g = as_gameplay(screen);
        let (radius, cx, cy) = (g.radar.radius, g.radar.center.0, g.radar.center.1);
        let range = OUTDOOR_RANGE;
        let range_sq = (range - 1.0) * (range - 1.0);
        #[allow(clippy::cast_precision_loss)]
        let scale = radius as f32 / range;
        let player = objects.world.player.expect("the recording named a player");
        #[allow(clippy::cast_possible_truncation)]
        let trunc = |v: f32| v as i32;
        let mut out = Vec::new();
        for o in &hud.radar {
            let w = objects.world.weenie(o.id);
            let showable = w.is_some_and(|w| matches!(w.pwd.radar_enum.unwrap_or(0), 2..=4));
            if o.id == player || !showable || !o.in_world {
                continue;
            }
            let (px, py, _) = o.player_space;
            if px * px + py * py >= range_sq {
                continue;
            }
            let sx = trunc(px * scale + cx);
            let sy = trunc(cy - py * scale);
            let (bx, by) = (trunc(cx), trunc(cy));
            if sx < bx - radius || sx > bx + radius || sy < by - radius || sy > by + radius {
                continue;
            }
            out.push(((sx, sy), o.id));
        }
        out
    }

    /// Everything a point on the radar is within the client's own reach of.
    ///
    /// In a thin scene there is exactly one and "the thing that mark stands for" is unambiguous.
    /// In a crowded one there are several within a few pixels of each other, and which of them
    /// the client names is its own tie-breaking; the scenario asserts that it names one of
    /// these, and asserts the exact answer where there is only one to give.
    pub fn things_within_reach(
        map: &[((i32, i32), dereth_primitives::ObjectId)],
        at: (i32, i32),
    ) -> Vec<dereth_primitives::ObjectId> {
        let mut out = Vec::new();
        for &((x, y), id) in map {
            let (dx, dy) = (at.0 - x, at.1 - y);
            if dx * dx + dy * dy < dereth_ui_screens::mapradar::radar::HOVER_DIST_SQ {
                out.push(id);
            }
        }
        out
    }
}

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

mod motion {
    use std::sync::Arc;

    use dereth_animation::data::AnimAssets;
    use dereth_animation::motion::{InterpretedMotionState, MoveToRequest, MovementParameters};
    use dereth_animation::{MotionCommand, MotionDriver};
    use dereth_client::anim_assets::DatAnimAssets;
    use dereth_client::character::{
        Character, ALUVIAN_MALE_MOTION_TABLE, ALUVIAN_MALE_SCALE, ALUVIAN_MALE_SETUP,
    };
    use dereth_client::objects::ObjectStream;
    use dereth_client_net::client_session::SessionEvent;
    use dereth_dat::RetailDatStore;
    use dereth_physics::math::V3 as _;
    use dereth_physics::pmanager::FALLBACK_SPEED;
    use dereth_primitives::{LocalTime, ObjectId, Position, Quat, Vec3};
    use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
    use dereth_protocol::Message;
    use dereth_testkit::HeadlessClient;

    use super::support::{
        body_origin, create_event, fresh_local_body, settled_at, store, DT, PLAYER,
    };

    /// The remote body every correction scenario below moves.
    const VICTIM: ObjectId = ObjectId(0x8000_114D);
    /// An id the client never sees, so a move-to issued at it records a target and starts no
    /// motion of its own -- which is the state "this body is on its way somewhere" means, with
    /// nothing else touching the body.
    const NEVER_SEEN: ObjectId = ObjectId(0x7000_0BAD);

    /// An ordinary creature's run rate, and the two factors the walk speed is built from. They
    /// are the client's own numbers, written as independent literals because
    /// reading them back through the constant would not notice a wrong constant.
    const RUN_RATE: f32 = 1.5;
    const RUN_ANIM_SPEED: f32 = 4.0;
    const INTERP_DOUBLE: f32 = 2.0;
    /// The rate a creature the shard has said nothing about moves at.
    const DEFAULT_RUN_RATE: f32 = 1.0;

    fn walk_speed(rate: f32) -> f32 {
        rate * RUN_ANIM_SPEED * INTERP_DOUBLE
    }

    /// A yaw-only rotation, the only one a standing body ever carries.
    fn yaw(degrees: f32) -> Quat {
        let half = degrees.to_radians() * 0.5;
        Quat {
            w: dereth_primitives::num::math::cosf(half),
            x: 0.0,
            y: 0.0,
            z: dereth_primitives::num::math::sinf(half),
        }
    }

    fn body_heading(c: &Character, id: ObjectId) -> f32 {
        let h = c.world.by_object_id(id).expect("the body exists");
        dereth_physics::math::get_heading(&c.world.get(h).expect("the body exists").position.frame)
    }

    /// One position correction carrying a **real** facing: only two components are declared
    /// absent, so the heading under test really crosses the wire.
    ///
    /// `support::position_event` declares the whole orientation absent, which is right for the
    /// fifteen scenarios above and wrong for the two here whose subject *is* the facing.
    fn correction_with_facing(id: ObjectId, at: &Position, stamp: u16) -> SessionEvent {
        let f = position_flags::ORIENTATION_HAS_NO_X
            | position_flags::ORIENTATION_HAS_NO_Y
            | position_flags::IS_GROUNDED;
        let msg = MovementPositionEvent {
            id,
            position: PositionPack {
                flags: f,
                origin: dereth_protocol::types::Origin {
                    objcell_id: at.cell.raw(),
                    origin: dereth_protocol::types::Vec3 {
                        x: at.frame.origin.x,
                        y: at.frame.origin.y,
                        z: at.frame.origin.z,
                    },
                },
                orientation: dereth_protocol::types::Quat {
                    w: at.frame.rotation.w,
                    x: 0.0,
                    y: 0.0,
                    z: at.frame.rotation.z,
                },
                instance_timestamp: 0,
                position_timestamp: stamp,
                teleport_timestamp: 0,
                ..PositionPack::default()
            },
        };
        let body = dereth_protocol::write_body(&msg).expect("the update encodes");
        let decoded = MovementPositionEvent::read(&mut dereth_protocol::Reader::new(&body))
            .expect("the encoded update round trips");
        assert!(
            decoded.position.has_contact(),
            "the contact flag did not encode"
        );
        assert!(
            (decoded.position.orientation.z - at.frame.rotation.z).abs() < 1e-6,
            "the wire facing did not encode"
        );
        SessionEvent::WorldObject {
            opcode: MovementPositionEvent::OPCODE,
            body,
        }
    }

    /// A real motion driver on a remote body: a part array, the shipped animation table, and
    /// whichever of the two roads a rate can reach it by.
    ///
    /// `weenie_rate` is the one the body's own record carries -- which only the player's own body
    /// ever has. `interpreted_run` is the one an ordinary movement message carries for a creature
    /// the shard has running. `moving_to` issues a move-to at an object this client has never
    /// seen, which records the target and starts no motion.
    fn driver_on(
        c: &mut Character,
        store: &Arc<RetailDatStore>,
        id: ObjectId,
        weenie_rate: Option<f32>,
        interpreted_run: Option<f32>,
        moving_to: bool,
    ) {
        let assets = Arc::new(DatAnimAssets::new(Arc::clone(store)));
        let setup = assets
            .setup(ALUVIAN_MALE_SETUP)
            .expect("the shipped setup decodes");
        let mut driver = MotionDriver::new(Arc::clone(&assets) as Arc<dyn AnimAssets>);
        assert!(driver.set_setup(setup), "the part array is built");
        assert!(
            driver.set_motion_table(ALUVIAN_MALE_MOTION_TABLE),
            "the animation table loads"
        );
        driver.scale = ALUVIAN_MALE_SCALE;
        driver.env.run_rate = weenie_rate;
        if let Some(rate) = interpreted_run {
            let state = InterpretedMotionState {
                current_style: MotionCommand::NON_COMBAT,
                forward_command: MotionCommand::RUN_FORWARD,
                forward_speed: rate,
                ..InterpretedMotionState::default()
            };
            driver.unpack_interpreted_movement(MotionCommand::NON_COMBAT, &state, false);
        }
        if moving_to {
            driver.with_movement(|m, ctx| {
                m.perform_movement(
                    &MoveToRequest::MoveToObject {
                        object_id: NEVER_SEEN,
                        top_level_id: NEVER_SEEN,
                        radius: 0.5,
                        height: 1.0,
                    },
                    &MovementParameters::default(),
                    ctx,
                );
            });
            assert!(
                driver.movement.is_moving_to(),
                "the body must be on its way somewhere"
            );
        } else {
            assert!(
                !driver.movement.is_moving_to(),
                "the control body goes nowhere of its own"
            );
        }
        let h = c.world.by_object_id(id).expect("the body exists");
        c.world
            .get_mut(h)
            .expect("the body exists")
            .set_motion(Box::new(driver));
    }

    /// The scene every correction scenario runs in: a remote body standing on real ground, a
    /// destination three metres away, and the local body parked out of the way.
    struct Scene {
        c: Character,
        stream: ObjectStream,
        t: f64,
        start: Position,
        destination: Position,
    }

    fn scene(
        store: &Arc<RetailDatStore>,
        weenie_rate: Option<f32>,
        interpreted_run: Option<f32>,
        moving_to: bool,
        start_yaw: f32,
        wire_yaw: f32,
    ) -> Scene {
        let mut c = fresh_local_body(store);
        let mut t = 3.0;
        let mut start = settled_at(&mut c, 96.0, 96.0, &mut t);
        let mut destination = settled_at(&mut c, 99.0, 96.0, &mut t);
        let _ = settled_at(&mut c, 96.0, 84.0, &mut t);
        start.frame.rotation = yaw(start_yaw);
        destination.frame.rotation = yaw(wire_yaw);

        let mut stream = ObjectStream::new();
        stream.apply_event(
            &create_event(VICTIM, Some(start), PLAYER, "Victim"),
            LocalTime(t),
        );
        stream.sync_physics(store, &mut c.world);
        let gap = body_origin(&c, VICTIM)
            .expect("the body is in a cell")
            .sub(start.frame.origin)
            .mag2()
            .sqrt();
        assert!(gap < 1e-4, "the create was deflected by {gap:.4} m");
        driver_on(
            &mut c,
            store,
            VICTIM,
            weenie_rate,
            interpreted_run,
            moving_to,
        );

        // A body's distance to the player is unknown until it has been stepped once.
        for _ in 0..4 {
            t += DT;
            c.update(LocalTime(t));
        }
        Scene {
            c,
            stream,
            t,
            start,
            destination,
        }
    }

    /// What one sub-step of a correction did.
    struct SubStep {
        moved: f32,
        cached_speed: f32,
        max_speed: Option<f32>,
    }

    fn one_substep(s: &mut Scene, store: &Arc<RetailDatStore>) -> SubStep {
        let before = body_origin(&s.c, VICTIM).expect("the body is in a cell");
        s.stream.apply_event(
            &correction_with_facing(VICTIM, &s.destination, 1),
            LocalTime(s.t),
        );
        s.stream.sync_physics(store, &mut s.c.world);
        s.t += DT;
        s.c.update(LocalTime(s.t));
        let after = body_origin(&s.c, VICTIM).expect("the body is in a cell");
        let h = s.c.world.by_object_id(VICTIM).expect("the body exists");
        SubStep {
            moved: after.sub(before).mag2().sqrt(),
            cached_speed: s
                .c
                .world
                .get(h)
                .expect("the body exists")
                .velocity()
                .mag2()
                .sqrt(),
            max_speed: s
                .c
                .world
                .interpolation(h)
                .expect("a manager was made")
                .max_speed,
        }
    }

    // ---------------------------------------------------------------------------------------
    // movement.correction.a-body-walks-a-correction-at-its-own-speed-and-not-a-default
    // ---------------------------------------------------------------------------------------

    /// **A body walks a correction at its own speed.** One sub-step covers the body's own
    /// quantum and not the client's fallback, and the whole walk is finished in the number of
    /// sub-steps that speed implies -- which is far fewer than the fallback would need.
    pub fn a_correction_runs_at_the_bodys_own_speed() {
        let store = store();
        let native = walk_speed(RUN_RATE);
        #[allow(clippy::cast_possible_truncation)]
        let native_step = native * DT as f32;
        #[allow(clippy::cast_possible_truncation)]
        let fallback_step = FALLBACK_SPEED * DT as f32;

        // One sub-step.
        let mut s = scene(&store, Some(RUN_RATE), None, false, 0.0, 0.0);
        let step = one_substep(&mut s, &store);
        let h = s.c.world.by_object_id(VICTIM).expect("the body exists");
        let still_walking = s.c.world.is_interpolating(h);
        // The sweep achieves a little under the quantum it is handed, because it asks for
        // `speed * quantum` along the straight line and walks the body over real ground: every
        // such walk lands a few per cent short for that reason. The bound is therefore relative,
        // and the discriminator is that it is nowhere near the fallback's quantum.
        let ratio = step.moved / native_step;
        let quantum_is_the_bodys_own = (0.90..=1.001).contains(&ratio)
            && step.moved > fallback_step * 1.3
            && step.max_speed == Some(RUN_RATE * RUN_ANIM_SPEED)
            && (step.cached_speed - native).abs() < native * 0.1
            && step.cached_speed <= dereth_physics::globals::MAX_VELOCITY
            && still_walking;

        // And the whole walk.
        let mut s = scene(&store, Some(RUN_RATE), None, false, 0.0, 0.0);
        let before = body_origin(&s.c, VICTIM).expect("the body is in a cell");
        let distance = before.sub(s.destination.frame.origin).mag2().sqrt();
        assert!(
            distance > 2.0,
            "the walk under test is only {distance:.3} m long"
        );
        s.stream.apply_event(
            &correction_with_facing(VICTIM, &s.destination, 1),
            LocalTime(s.t),
        );
        s.stream.sync_physics(&store, &mut s.c.world);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let native_steps = (distance / native_step).ceil() as usize + 1;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let fallback_steps = (distance / fallback_step).ceil() as usize + 1;
        assert!(
            native_steps + 1 < fallback_steps,
            "the two speeds are not far enough apart to tell apart: {native_steps} vs \
             {fallback_steps}"
        );
        for _ in 0..native_steps {
            s.t += DT;
            s.c.update(LocalTime(s.t));
        }
        let h = s.c.world.by_object_id(VICTIM).expect("the body exists");
        let offset = body_origin(&s.c, VICTIM)
            .expect("the body is in a cell")
            .sub(s.destination.frame.origin)
            .mag2()
            .sqrt();
        let finished_in_its_own_time =
            !s.c.world.is_interpolating(h) && offset < dereth_physics::pmanager::CLOSE_ENOUGH;

        println!(
            "sub-step: one sub-step moved {:.6} m ({native_step:.6} its own, \
             {fallback_step:.6} the fallback), max_speed {:?}; the whole walk finished in \
             {native_steps} sub-steps ({fallback_steps} at the fallback), {offset:.6} m short",
            step.moved, step.max_speed
        );

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.correction.a-body-walks-a-correction-at-its-own-speed-and-not-a-default",
            move |_| quantum_is_the_bodys_own && finished_in_its_own_time,
        );
    }

    #[test]
    fn scenario_a_correction_runs_at_the_bodys_own_speed() {
        super::scenario("a_correction_runs_at_the_bodys_own_speed");
    }

    // ---------------------------------------------------------------------------------------
    // movement.correction.a-body-on-its-way-somewhere-takes-the-place-and-keeps-its-own-facing
    // ---------------------------------------------------------------------------------------

    /// **A body already on its way somewhere keeps its own facing.** It walks onto the place the
    /// shard named without turning to the way the shard was facing; a body that is going nowhere
    /// of its own does turn. Both arms are here, because either alone would pass on a client that
    /// always did one of them.
    pub fn a_body_on_its_way_keeps_its_own_facing() {
        let store = store();

        // `(its own facing, the shard's, the node's, where it ended, it kept its own, it arrived)`
        let arm = |moving_to: bool| -> (f32, f32, f32, f32, bool, bool) {
            let mut s = scene(&store, Some(RUN_RATE), None, moving_to, 30.0, 210.0);
            let own = body_heading(&s.c, VICTIM);
            let wire = dereth_physics::math::get_heading(&s.destination.frame);
            assert!(
                (own - dereth_physics::math::get_heading(&s.start.frame)).abs() < 1.0,
                "the body was not created on the facing the scenario asked for: {own:.3}"
            );
            assert!(
                (own - wire).abs() > 90.0,
                "the two facings are too close to tell apart"
            );

            s.stream.apply_event(
                &correction_with_facing(VICTIM, &s.destination, 1),
                LocalTime(s.t),
            );
            s.stream.sync_physics(&store, &mut s.c.world);

            let h = s.c.world.by_object_id(VICTIM).expect("the body exists");
            let m = s.c.world.interpolation(h).expect("a manager was made");
            let kept = m.keep_heading;
            let node = m
                .position_queue
                .first()
                .copied()
                .expect("a node was queued");
            let node_heading = dereth_physics::math::get_heading(&node.pos.frame);

            for _ in 0..20 {
                s.t += DT;
                s.c.update(LocalTime(s.t));
            }
            let after = body_heading(&s.c, VICTIM);
            let offset = body_origin(&s.c, VICTIM)
                .expect("the body is in a cell")
                .sub(s.destination.frame.origin)
                .mag2()
                .sqrt();
            (own, wire, node_heading, after, kept, offset < 0.2)
        };

        let (own_a, wire_a, node_a, after_a, kept_a, arrived_a) = arm(true);
        let (own_b, wire_b, node_b, after_b, kept_b, arrived_b) = arm(false);
        println!(
            "sub-step facing: on its way -- node {node_a:.3}, body ended {after_a:.3} \
             (its own {own_a:.3}, the shard's {wire_a:.3}); going nowhere -- node {node_b:.3}, \
             body ended {after_b:.3} (its own {own_b:.3}, the shard's {wire_b:.3})"
        );

        // The body on its way somewhere is marked as keeping its facing, the queued node carries
        // the body's own facing rather than the shard's, and the walked body ends on it.
        let keeps_its_own =
            kept_a && arrived_a && (node_a - own_a).abs() < 1.0 && (after_a - own_a).abs() < 1.0;
        // And the control is marked the other way and ends on the shard's.
        let takes_the_shards =
            !kept_b && arrived_b && (node_b - wire_b).abs() < 1.0 && (after_b - wire_b).abs() < 1.0;

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.correction.a-body-on-its-way-somewhere-takes-the-place-and-keeps-its-own-facing",
            move |_| keeps_its_own && takes_the_shards,
        );
    }

    #[test]
    fn scenario_a_body_on_its_way_keeps_its_own_facing() {
        super::scenario("a_body_on_its_way_keeps_its_own_facing");
    }

    // ---------------------------------------------------------------------------------------
    // movement.correction.a-creature-the-shard-says-is-running-walks-at-the-rate-the-shard-gave-it
    // ---------------------------------------------------------------------------------------

    /// **The rate the shard sent is the rate the body walks at.** A remote creature's own record
    /// carries no rate at all -- only the player's own body ever has one -- so the only road a
    /// creature's speed has is the movement message the shard sends about it, and this is that
    /// road end to end: the message is unpacked, the rate is stored, and the next correction runs
    /// at it.
    pub fn a_running_creature_walks_at_the_rate_the_shard_sent() {
        let store = store();
        let mut s = scene(&store, None, Some(RUN_RATE), false, 0.0, 0.0);
        let step = one_substep(&mut s, &store);
        let want = walk_speed(RUN_RATE);
        #[allow(clippy::cast_possible_truncation)]
        let achieved = (f64::from(step.moved) / DT) as f32;

        // And the store itself, in isolation, so a regression in the walk and a regression in the
        // store are told apart.
        let assets = Arc::new(DatAnimAssets::new(Arc::clone(&store)));
        let setup = assets
            .setup(ALUVIAN_MALE_SETUP)
            .expect("the shipped setup decodes");
        let mut driver = MotionDriver::new(Arc::clone(&assets) as Arc<dyn AnimAssets>);
        assert!(driver.set_setup(setup), "the part array is built");
        assert!(
            driver.set_motion_table(ALUVIAN_MALE_MOTION_TABLE),
            "the animation table loads"
        );
        let started_at_the_default =
            (driver.movement.interp.my_run_rate - DEFAULT_RUN_RATE).abs() < 1e-6;
        driver.unpack_interpreted_movement(
            MotionCommand::NON_COMBAT,
            &InterpretedMotionState {
                current_style: MotionCommand::NON_COMBAT,
                forward_command: MotionCommand::RUN_FORWARD,
                forward_speed: RUN_RATE,
                ..InterpretedMotionState::default()
            },
            false,
        );
        let env = driver.env;
        let stored = (driver.movement.interp.my_run_rate - RUN_RATE).abs() < 1e-6
            && (driver.movement.interp.get_adjusted_max_speed(&env) - RUN_RATE * RUN_ANIM_SPEED)
                .abs()
                < 1e-4
            && (driver.movement.interp.get_max_speed(&env) - RUN_RATE * RUN_ANIM_SPEED).abs()
                < 1e-4;

        println!(
            "cached speed running: moved {:.6} m = {achieved:.3} m/s, cached {:.3}, max_speed \
             {:?} (want {want:.3} m/s); the store started at the default: \
             {started_at_the_default}",
            step.moved, step.cached_speed, step.max_speed
        );

        let walked_at_the_shards_rate = step.max_speed == Some(RUN_RATE * RUN_ANIM_SPEED)
            && (step.cached_speed - want).abs() < want * 0.1
            && (achieved - want).abs() < want * 0.1;

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.correction.a-creature-the-shard-says-is-running-walks-at-the-rate-the-shard-gave-it",
            move |_| walked_at_the_shards_rate && started_at_the_default && stored,
        );
    }

    #[test]
    fn scenario_a_running_creature_walks_at_the_rate_the_shard_sent() {
        super::scenario("a_running_creature_walks_at_the_rate_the_shard_sent");
    }

    // ---------------------------------------------------------------------------------------
    // movement.correction.a-creature-the-shard-has-said-nothing-about-walks-at-the-clients-own-rate
    // ---------------------------------------------------------------------------------------

    /// **The control, and the point of the pair.** A creature the shard has said nothing about is
    /// corrected at the client's own starting rate. That is not a rate the client failed to fetch
    /// -- a remote body's record never answers with one -- so it is the right answer and not a
    /// stand-in for a missing push.
    ///
    /// The client's default rate and its no-interpreter fallback are close enough that the number
    /// alone would not tell them apart. What tells them apart is that this body has a real
    /// interpreter and answered with a speed of its own, which the fallback arm never does.
    pub fn a_creature_the_shard_said_nothing_about_walks_at_the_default() {
        let store = store();
        let mut s = scene(&store, None, None, false, 0.0, 0.0);
        let step = one_substep(&mut s, &store);
        let want = walk_speed(DEFAULT_RUN_RATE);
        #[allow(clippy::cast_possible_truncation)]
        let achieved = (f64::from(step.moved) / DT) as f32;

        println!(
            "cached speed idle: moved {:.6} m = {achieved:.3} m/s, cached {:.3}, max_speed {:?} \
             (want {want:.3} m/s)",
            step.moved, step.cached_speed, step.max_speed
        );

        let at_the_default = step.max_speed == Some(DEFAULT_RUN_RATE * RUN_ANIM_SPEED)
            && (step.cached_speed - want).abs() < want * 0.1
            && (step.cached_speed - walk_speed(RUN_RATE)).abs() > 1.0
            && step.max_speed.is_some();

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.correction.a-creature-the-shard-has-said-nothing-about-walks-at-the-clients-own-rate",
            move |_| at_the_default,
        );
    }

    #[test]
    fn scenario_a_creature_the_shard_said_nothing_about_walks_at_the_default() {
        super::scenario("a_creature_the_shard_said_nothing_about_walks_at_the_default");
    }

    // =======================================================================================
    // The approach.
    //
    // The client never walks the player anywhere of its own accord. The player's use goes to the
    // shard, and the shard answers with a movement message addressed to the player's own body
    // telling it to walk to the thing. Everything below is that message being executed.
    // =======================================================================================

    use dereth_animation::motion::flags as move_flags;
    use dereth_client::character::{CharacterInput, MovementCommands};
    use dereth_primitives::{Frame, Position as Pos};
    use dereth_protocol::movement::{
        movement_type, MoveToArm, MovementBody, MovementParameters as WireParams,
        MovementSetObjectMovement,
    };
    use dereth_protocol::Opcode;

    /// The thing the player used, and a second one for the scenario that replaces an approach.
    const TARGET: ObjectId = ObjectId(0x8000_0997);
    const OTHER_TARGET: ObjectId = ObjectId(0x8000_0998);

    /// A settled local body on the default landblock, as every approach scenario starts from.
    fn local_body(store: &Arc<RetailDatStore>) -> Character {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let mut c = Character::new(
            store,
            &region,
            dereth_client::world::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("the body is created");
        for i in 1..=60 {
            c.update(LocalTime(f64::from(i) / 30.0));
        }
        c
    }

    /// A target `metres` due east of the body, at the same height, in the same cell.
    fn target_east(c: &Character, metres: f32) -> Pos {
        let p = c.position();
        let o = p.frame.origin;
        Pos::new(
            p.cell,
            Frame::new(Vec3::new(o.x + metres, o.y, o.z), Quat::IDENTITY),
        )
    }

    /// What the shard sends for a use at `reach`: the client's own default word, with the
    /// target's own reach in it.
    fn use_params(reach: f32) -> MovementParameters {
        MovementParameters {
            distance_to_object: reach,
            ..MovementParameters::default()
        }
    }

    fn move_to(target: ObjectId) -> MoveToRequest {
        // A loose item on the ground has no part array of its own worth speaking of.
        MoveToRequest::MoveToObject {
            object_id: target,
            top_level_id: target,
            radius: 0.0,
            height: 0.0,
        }
    }

    /// Run `seconds` of frames, feeding the target watcher whenever it asks -- which is what the
    /// world does with the answers the shard sends about the thing being walked to.
    fn walk(c: &mut Character, t0: f64, seconds: f64, target: Pos) -> f64 {
        let mut t = t0;
        #[allow(clippy::cast_possible_truncation)]
        let steps = (seconds * 30.0).round() as i64;
        for _ in 0..steps {
            t += 1.0 / 30.0;
            c.update(LocalTime(t));
            if c.wanted_target().is_some() {
                c.update_target(target, Vec3::ZERO, true);
            }
        }
        t
    }

    // ---------------------------------------------------------------------------------------
    // movement.approach.is-the-shards-and-the-client-never-starts-one-itself
    // ---------------------------------------------------------------------------------------

    /// One movement message the shard sent, and the characters that recording's login named --
    /// the only way to tell "the player" from "a monster".
    struct ShardMovement {
        session: &'static str,
        mover: ObjectId,
        autonomous: bool,
        body: MovementBody,
        characters: Vec<ObjectId>,
    }

    /// Every walk-or-turn instruction in the locked corpus, read off the recordings through the
    /// client's own endpoint.
    ///
    /// The replay loop is the smallest one in this crate, built on `dereth_testkit::replay`. The
    /// recordings are the ones the corpus index names, read at run time; no count below is pinned.
    fn corpus_movements() -> Vec<ShardMovement> {
        let mut out = Vec::new();
        for (id, _slug) in dereth_client_net::client_session::testing::session_index() {
            let records = dereth_testkit::replay::records(id);
            // Named with its own account, which is what this walk was written against; the
            // endpoint itself is the harness's.
            let mut net = dereth_testkit::replay::recorded_endpoint_as(
                &records,
                "dereth-testkit",
                "dereth-testkit",
            );
            for r in records.iter().filter(|r| !r.c2s) {
                net.feed(&r.raw, r.peer(), LocalTime(r.t));
            }
            // The character list is the **whole recording's**, attached after the walk below:
            // a login's character set does not have to reach the session layer before the first
            // movement message does, and pairing each instruction with the list as it stood at
            // that moment would file the shard's own walks as somebody else's.
            let mut characters: Vec<ObjectId> = Vec::new();
            let first = out.len();
            while let Some(m) = dereth_primitives::Transport::poll(&mut net.session.transport) {
                if m.opcode == Opcode::LOGIN_LOGIN_CHARACTER_SET.0 {
                    let mut r = dereth_protocol::Reader::new(&m.body);
                    if let Ok(set) = dereth_protocol::login::LoginCharacterSet::read(&mut r) {
                        for c in &set.characters {
                            if !characters.contains(&c.gid) {
                                characters.push(c.gid);
                            }
                        }
                    }
                    continue;
                }
                if m.opcode != Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0 {
                    continue;
                }
                let mut r = dereth_protocol::Reader::new(&m.body);
                let msg = MovementSetObjectMovement::read(&mut r)
                    .expect("a recorded movement message decodes");
                let buf = msg.decoded_movement().expect("its movement buffer decodes");
                if buf.body.movement_type == movement_type::INVALID {
                    continue;
                }
                out.push(ShardMovement {
                    session: id,
                    mover: msg.id,
                    autonomous: buf.autonomous,
                    body: buf.body,
                    characters: Vec::new(),
                });
            }
            for m in &mut out[first..] {
                m.characters.clone_from(&characters);
            }
        }
        assert!(
            !out.is_empty(),
            "the corpus carries no walk-or-turn instruction at all"
        );
        out
    }

    /// **The approach belongs to the shard.** Every walk the corpus records being asked of the
    /// player's own body was asked by the shard and not taken by the client; every one of them
    /// re-encodes to the bytes the recording carries; every one measures its reach as a clearance
    /// rather than a centre distance; every one carries the client's own walk-or-run threshold;
    /// and the reaches differ from thing to thing, so the reach is the *thing's* and not a
    /// constant the client could have assumed.
    ///
    /// **Two clauses are deliberately not asserted**: that every such walk carries the "move
    /// towards" direction, and that it carries the client's own walk-or-run threshold. Neither
    /// holds over the corpus as it now stands, so both are counted and printed here and asserted
    /// nowhere; they are a finding about the recordings, not a claim about the client.
    pub fn the_approach_is_the_shards_own() {
        let all = corpus_movements();
        let mut re_encoded = true;
        let mut player_moves = 0usize;
        let mut all_non_autonomous = true;
        let mut all_cylinder = true;
        let mut reaches: Vec<String> = Vec::new();
        // Two things the corpus does not bear out for every such walk. They are counted here and
        // asserted nowhere.
        let mut moves_towards = 0usize;
        let mut clients_own_threshold = 0usize;

        for m in &all {
            let arm = m
                .body
                .decode_move_to()
                .unwrap_or_else(|e| {
                    panic!(
                        "{}: a recorded instruction does not decode: {e:?}",
                        m.session
                    )
                })
                .expect("a walk-or-turn instruction carries an arm");
            re_encoded &= MovementBody::encode_move_to(&arm) == m.body.unhandled;

            if m.body.movement_type != movement_type::MOVE_TO_OBJECT
                || !m.characters.contains(&m.mover)
            {
                continue;
            }
            let Some(MoveToArm::MoveToObject { params, .. }) =
                m.body.decode_move_to().expect("decodes")
            else {
                panic!("a walk-to-a-thing decoded as something else")
            };
            let WireParams::MoveTo {
                bitfield,
                distance_to_object,
                walk_run_threshold,
                ..
            } = params
            else {
                panic!("a walk-to-a-thing carries the walk parameter block")
            };
            all_non_autonomous &= !m.autonomous;
            all_cylinder &= bitfield & move_flags::USE_SPHERES != 0;
            moves_towards += usize::from(
                bitfield & move_flags::MOVE_TOWARDS != 0 && bitfield & move_flags::MOVE_AWAY == 0,
            );
            clients_own_threshold += usize::from((walk_run_threshold - 15.0).abs() < 1e-6);
            reaches.push(format!("{distance_to_object}"));
            player_moves += 1;
        }
        reaches.sort();
        reaches.dedup();
        println!(
            "approach: {} recorded walk-or-turn instructions, {player_moves} of them the shard \
             walking the player himself, {} distinct reaches; {moves_towards} of those walks \
             move towards their target and {clients_own_threshold} carry the client's own \
             walk-or-run threshold",
            all.len(),
            reaches.len()
        );

        let held = re_encoded
            && player_moves >= 5
            && all_non_autonomous
            && all_cylinder
            && reaches.len() >= 3;

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.approach.is-the-shards-and-the-client-never-starts-one-itself",
            move |_| held,
        );
    }

    #[test]
    fn scenario_the_approach_is_the_shards_own() {
        super::scenario("the_approach_is_the_shards_own");
    }

    // ---------------------------------------------------------------------------------------
    // movement.approach.walks-to-what-is-out-of-reach-and-never-to-what-is-already-in-reach
    // ---------------------------------------------------------------------------------------

    /// **Out of reach walks; in reach does not.** A body told to walk to something eight metres
    /// away closes the distance; told to walk to something already within the reach the shard
    /// named it does not take a step, and the instruction is over on the first answer about the
    /// thing. The test is on the reach itself and it is strict: exactly at the reach is arrived.
    pub fn an_approach_walks_only_to_what_is_out_of_reach() {
        let store = store();

        // Out of reach.
        let mut c = local_body(&store);
        let start = c.position();
        let target = target_east(&c, 8.0);
        c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
        let recorded_before_any_answer = c.is_moving_to()
            && c.stats.move_tos_performed == 1
            && c.wanted_target().is_some()
            && dereth_animation::motion::moveto::distance(&start, &c.position()) < 0.01;
        let _t = walk(&mut c, 2.0, 6.0, target);
        let moved = dereth_animation::motion::moveto::distance(&start, &c.position());
        let closed = dereth_animation::motion::moveto::distance(&c.position(), &target)
            < dereth_animation::motion::moveto::distance(&start, &target);
        let walked = c.stats.target_updates > 0 && moved > 1.0 && closed;

        // Already in reach.
        let mut c = local_body(&store);
        let start = c.position();
        let near = target_east(&c, 0.2);
        c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
        c.update_target(near, Vec3::ZERO, true);
        let over_at_once = !c.is_moving_to() && c.wanted_target().is_none();
        let _t = walk(&mut c, 2.0, 2.0, near);
        let stood_still = dereth_animation::motion::moveto::distance(&start, &c.position()) < 0.05;

        // The boundary, driven directly: placing a body at exactly the reach through real terrain
        // is not reproducible to the last float, and the boundary is what is under test.
        let p = use_params(0.6);
        let mut strict = p.get_command(0.6).0 == MotionCommand::NONE
            && p.get_command(0.599_9).0 == MotionCommand::NONE
            && p.get_command(0.600_1).0 == MotionCommand::WALK_FORWARD;
        for r in [0.5_f32, 0.8, 1.0, 2.0] {
            let p = use_params(r);
            strict &= p.get_command(r).0 == MotionCommand::NONE
                && p.get_command(r + 0.01).0 == MotionCommand::WALK_FORWARD;
        }

        println!(
            "approach reach: walked {moved:.3} m to a thing 8 m away; stood still for one \
             already in reach: {stood_still}; the reach test is strict: {strict}"
        );

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.approach.walks-to-what-is-out-of-reach-and-never-to-what-is-already-in-reach",
            move |_| recorded_before_any_answer && walked && over_at_once && stood_still && strict,
        );
    }

    #[test]
    fn scenario_an_approach_walks_only_to_what_is_out_of_reach() {
        super::scenario("an_approach_walks_only_to_what_is_out_of_reach");
    }

    // ---------------------------------------------------------------------------------------
    // movement.approach.ends-by-stopping-within-reach-and-nothing-else-happens
    // ---------------------------------------------------------------------------------------

    /// **Arriving is just stopping.** The body comes to rest within the reach the shard named,
    /// the thing stops being watched, nothing is counted as a failure, and the client sends
    /// nothing: whatever the player asked for fires on the shard, which is watching for the walk
    /// to be over.
    pub fn an_approach_ends_by_stopping_within_reach() {
        let store = store();
        let mut c = local_body(&store);
        let target = target_east(&c, 3.0);

        c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
        let _t = walk(&mut c, 2.0, 12.0, target);

        let clearance = dereth_animation::motion::moveto::cylinder_distance(
            c.radius(),
            c.height(),
            &c.position(),
            0.0,
            0.0,
            &target,
        );
        println!("approach arrival: stopped with {clearance:.3} m clearance for a 0.6 m reach");

        let held = !c.is_moving_to()
            && c.wanted_target().is_none()
            && c.stats.move_tos_failed == 0
            && use_params(0.6).has(move_flags::USE_SPHERES)
            && clearance <= 0.6;

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.approach.ends-by-stopping-within-reach-and-nothing-else-happens",
            move |_| held,
        );
    }

    #[test]
    fn scenario_an_approach_ends_by_stopping_within_reach() {
        super::scenario("an_approach_ends_by_stopping_within_reach");
    }

    // ---------------------------------------------------------------------------------------
    // movement.approach.once-it-is-over-the-player-can-walk-the-body-himself-again
    // ---------------------------------------------------------------------------------------

    /// **The player gets his body back.** While the shard is walking it, the player's keys do
    /// nothing and the body is not handed back on its own -- not while no key is held. The first
    /// movement key he presses afterwards takes it back, and the body walks.
    pub fn after_an_approach_the_player_has_his_body_back() {
        use dereth_client_runtime::actions::movement::{command, CmdStruct, MovementAction};

        let store = store();
        let mut c = local_body(&store);
        let target = target_east(&c, 3.0);
        let mut commands = MovementCommands::default();
        let mut input = CharacterInput::default();

        c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
        commands.lose_control_to_server(&mut input);
        let mut retakes = 0;
        for frame in 1..=360 {
            if c.wanted_target().is_some() {
                c.update_target(target, Vec3::ZERO, true);
            }
            let (pending, moving) = {
                let d = c.driver();
                (d.movement.motions_pending(), d.movement.is_moving_to())
            };
            if commands.use_time(pending, moving, &mut input) {
                c.take_control_from_server();
                retakes += 1;
            }
            c.input = input;
            c.update(LocalTime(2.0 + f64::from(frame) / 30.0));
        }
        let still_the_shards = retakes == 0
            && commands.lists.controlled_by_server
            && !c.is_moving_to()
            && !c.driver().movement.motions_pending()
            && c.driver().movement.interp.interpreted_state.forward_command == MotionCommand::READY;
        let stopped = c.position();

        assert!(commands.on_action(
            MovementAction::SetMotion(CmdStruct {
                command: command::WALK_FORWARD,
                extent: None,
                start: Some(true),
            }),
            &mut input
        ));
        let retaken = commands.take_control_retake_pending();
        c.take_control_from_server();
        let his_again = retaken
            && !commands.lists.controlled_by_server
            && commands.control_retakes_from_commands == 1;
        c.input = input;
        for frame in 1..=60 {
            c.update(LocalTime(14.0 + f64::from(frame) / 30.0));
        }
        let walked = dereth_animation::motion::moveto::distance(&stopped, &c.position()) > 1.0;

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.approach.once-it-is-over-the-player-can-walk-the-body-himself-again",
            move |_| still_the_shards && his_again && walked,
        );
    }

    #[test]
    fn scenario_after_an_approach_the_player_has_his_body_back() {
        super::scenario("after_an_approach_the_player_has_his_body_back");
    }

    // ---------------------------------------------------------------------------------------
    // movement.approach.a-target-that-leaves-the-world-ends-it-and-says-which-way-it-went
    // ---------------------------------------------------------------------------------------

    /// **A thing that is not there ends the walk, and the two cases are told apart.** Gone before
    /// the first answer about it, the walk ends saying there was no such thing; gone after the
    /// walk had started, it ends saying somebody else got there first.
    pub fn a_target_that_leaves_the_world_ends_the_approach() {
        const NO_SUCH_OBJECT: u32 = 0x38;
        const OBJECT_GONE: u32 = 0x37;

        let store = store();

        let mut c = local_body(&store);
        let target = target_east(&c, 8.0);
        c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
        c.update_target(target, Vec3::ZERO, false);
        let never_there = c.stats.last_move_to_error == NO_SUCH_OBJECT && !c.is_moving_to();

        let mut c = local_body(&store);
        let target = target_east(&c, 8.0);
        c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
        c.update_target(target, Vec3::ZERO, true);
        let started = c.is_moving_to();
        let _t = walk(&mut c, 2.0, 1.0, target);
        c.update_target(target, Vec3::ZERO, false);
        let taken = c.stats.last_move_to_error == OBJECT_GONE && !c.is_moving_to();

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.approach.a-target-that-leaves-the-world-ends-it-and-says-which-way-it-went",
            move |_| never_there && started && taken,
        );
    }

    #[test]
    fn scenario_a_target_that_leaves_the_world_ends_the_approach() {
        super::scenario("a_target_that_leaves_the_world_ends_the_approach");
    }

    // ---------------------------------------------------------------------------------------
    // movement.approach.gives-up-only-when-the-body-has-strayed-further-than-the-shard-allowed
    // ---------------------------------------------------------------------------------------

    /// **The one thing a walk gives up on.** A player shuffling against a wall is never abandoned
    /// by the client -- what stops him is the shard's own patience or his own hands. The client
    /// abandons a walk on exactly one condition: the body has strayed further from where it started
    /// than the shard allowed, and then it says so.
    ///
    /// How many recorded walks carry a finite allowance is a count of the recordings and not a
    /// claim about the client, so it is printed here and asserted nowhere.
    pub fn an_approach_gives_up_only_on_straying_too_far() {
        const YOU_CHARGED_TOO_FAR: u32 = 0x3D;

        let mut finite = 0usize;
        let mut seen = 0usize;
        for m in corpus_movements() {
            let arm = m.body.decode_move_to().expect("decodes").expect("an arm");
            let (MoveToArm::MoveToObject { params, .. } | MoveToArm::MoveToPosition { params, .. }) =
                arm
            else {
                continue;
            };
            let WireParams::MoveTo { fail_distance, .. } = params else {
                continue;
            };
            finite += usize::from(fail_distance != f32::MAX);
            seen += 1;
        }
        println!(
            "approach: {finite} of {seen} recorded walks carry a finite allowance; the rest say \
             never give up"
        );

        let store = store();
        let mut c = local_body(&store);
        let target = target_east(&c, 8.0);
        let params = MovementParameters {
            fail_distance: 1.0,
            ..use_params(0.6)
        };
        c.perform_move_to(&move_to(TARGET), &params, Some(1.0));
        let _t = walk(&mut c, 2.0, 8.0, target);
        let gave_up = c.stats.last_move_to_error == YOU_CHARGED_TOO_FAR && !c.is_moving_to();
        let corpus_read = seen > 0;

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.approach.gives-up-only-when-the-body-has-strayed-further-than-the-shard-allowed",
            move |_| gave_up && corpus_read,
        );
    }

    #[test]
    fn scenario_an_approach_gives_up_only_on_straying_too_far() {
        super::scenario("an_approach_gives_up_only_on_straying_too_far");
    }

    // ---------------------------------------------------------------------------------------
    // movement.approach.a-second-one-replaces-the-first-rather-than-queueing
    // ---------------------------------------------------------------------------------------

    /// **Using a second thing while walking to the first turns the body round.** The first walk
    /// is reported cancelled, the second is the one in flight, and the thing being watched is the
    /// new one.
    pub fn a_second_approach_replaces_the_first() {
        const ACTION_CANCELLED: u32 = 0x36;

        let store = store();
        let mut c = local_body(&store);
        let target = target_east(&c, 8.0);

        c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
        c.update_target(target, Vec3::ZERO, true);
        let first_in_flight = c.is_moving_to() && c.stats.move_tos_failed == 0;

        c.perform_move_to(&move_to(OTHER_TARGET), &use_params(0.6), Some(1.0));
        let replaced = c.stats.move_tos_failed == 1
            && c.stats.last_move_to_error == ACTION_CANCELLED
            && c.is_moving_to()
            && c.wanted_target().map(|t| t.id) == Some(OTHER_TARGET);

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.approach.a-second-one-replaces-the-first-rather-than-queueing",
            move |_| first_in_flight && replaced,
        );
    }

    #[test]
    fn scenario_a_second_approach_replaces_the_first() {
        super::scenario("a_second_approach_replaces_the_first");
    }

    // ---------------------------------------------------------------------------------------
    // movement.approach.the-target-is-re-read-on-a-gate-and-only-when-it-has-moved
    // ---------------------------------------------------------------------------------------

    /// **A walking client does not ask about the thing every frame.** The first answer is never
    /// held back; after that the body asks again only once a gate's worth of time has passed
    /// *and* the thing has actually drifted. A thing that stays where it is says nothing more.
    pub fn the_target_is_re_read_on_a_gate() {
        let store = store();
        let mut c = local_body(&store);
        let target = target_east(&c, 8.0);
        c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));

        let snap = |at: Pos| {
            Some(dereth_animation::motion::moveto::TargetSnapshot {
                object_id: TARGET,
                ok: true,
                position: at,
                velocity: Vec3::ZERO,
            })
        };
        let updates = |c: &Character| c.driver().target_updates;

        let subscribed = c.wanted_target().is_some();
        c.set_target_snapshot(snap(target));
        c.deliver_first_target_update(LocalTime(2.0));
        let first_went_at_once = updates(&c) == 1 && c.is_moving_to();

        let mut further = target;
        further.frame.origin.x += 2.0;
        c.set_target_snapshot(snap(further));
        let mut t = 2.0;
        for _ in 0..8 {
            t += 1.0 / 30.0;
            c.update(LocalTime(t));
        }
        let inside_the_gate = updates(&c) == 1;
        for _ in 0..16 {
            t += 1.0 / 30.0;
            c.update(LocalTime(t));
        }
        let outside_the_gate = updates(&c) == 2;
        for _ in 0..24 {
            t += 1.0 / 30.0;
            c.update(LocalTime(t));
        }
        let a_still_thing_says_nothing = updates(&c) == 2;

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.approach.the-target-is-re-read-on-a-gate-and-only-when-it-has-moved",
            move |_| {
                subscribed
                    && first_went_at_once
                    && inside_the_gate
                    && outside_the_gate
                    && a_still_thing_says_nothing
            },
        );
    }

    #[test]
    fn scenario_the_target_is_re_read_on_a_gate() {
        super::scenario("the_target_is_re_read_on_a_gate");
    }

    // =======================================================================================
    // movement.move-to.an-exact-approach-does-not-carry-the-body-through-what-is-in-the-way
    // Start walking to something, then mash the forward key.
    // =======================================================================================

    /// The obstacle's radius. A chest is smaller; this is sized so that a body which goes through
    /// it cannot be mistaken for one that grazed past.
    const OBSTACLE_RADIUS: f32 = 1.5;
    /// How far ahead the obstacle stands, and on what bearing. **Not a cardinal**: a body starts
    /// facing north, so an obstacle due north would need no turn at all and the calibration below
    /// would be measuring a body that was already pointed the right way.
    const OBSTACLE_OFFSET: (f32, f32) = (3.0, 5.196_152_4);
    /// A flat patch of the default landblock. A body walking off a ridge is airborne, passes
    /// *under* an obstacle anchored to the old height and never touches it -- which reads as "the
    /// obstacle stopped nothing" for a reason that has nothing to do with the obstacle.
    const FLAT_GROUND: (f32, f32) = (20.0, 52.0);
    /// One sweep step of the walk. Under this is the spheres touching; the defect this is about
    /// puts the body metres in.
    const CONTACT_TOLERANCE: f32 = 0.10;

    fn walking_body(store: &Arc<RetailDatStore>) -> Character {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let mut c = Character::new(
            store,
            &region,
            dereth_client::world::DEFAULT_LANDBLOCK,
            FLAT_GROUND,
        )
        .expect("a body on real terrain");
        run_body(&mut c, 0.0, 3.0, CharacterInput::default());
        assert!(
            c.on_ground(),
            "real terrain must support the body before the walk starts"
        );
        c
    }

    /// Drive the body for `seconds` at the client's own rate, returning the new clock and the
    /// path. One clock per scenario, carried across phases: the physics pass steps nothing on a
    /// backwards clock, so restarting at zero is a silent no-op.
    fn run_body(
        c: &mut Character,
        t0: f64,
        seconds: f64,
        input: CharacterInput,
    ) -> (f64, Vec<Vec3>) {
        let dt = 1.0 / 30.0;
        let mut t = t0;
        let end = t0 + seconds;
        let mut path = Vec::new();
        while t < end {
            t += dt;
            c.input = input;
            c.update(LocalTime(t));
            path.push(c.position().frame.origin);
        }
        (t, path)
    }

    /// A solid, immovable piece of world furniture standing on the obstacle's bearing.
    fn obstacle(c: &mut Character) -> Vec3 {
        use dereth_physics::{SetupGeometry, Sphere};
        let here = c.position();
        let at = Vec3::new(
            here.frame.origin.x + OBSTACLE_OFFSET.0,
            here.frame.origin.y + OBSTACLE_OFFSET.1,
            here.frame.origin.z,
        );
        let geometry = Arc::new(SetupGeometry {
            spheres: vec![Sphere::new(
                Vec3::new(0.0, 0.0, OBSTACLE_RADIUS),
                OBSTACLE_RADIUS,
            )],
            sorting_sphere: Sphere::new(
                Vec3::new(0.0, 0.0, OBSTACLE_RADIUS),
                OBSTACLE_RADIUS * 2.0,
            ),
            radius: OBSTACLE_RADIUS,
            height: 2.0 * OBSTACLE_RADIUS,
            ..SetupGeometry::default()
        });
        let h = c.world.create(ObjectId(0x7000_0001), geometry, true);
        let mut cell = here.cell;
        let mut origin = at;
        dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin);
        c.world.enter_cell(h, cell);
        if let Some(o) = c.world.get_mut(h) {
            let frame = Frame::new(at, Quat::IDENTITY);
            o.state = dereth_physics::PhysicsState(o.state.0 | 0x0000_0001);
            o.set_frame(frame);
            o.position = Pos::new(cell, frame);
            o.transient_state.set_active_bit(false);
        }
        c.world.calc_cross_cells(h, true);
        at
    }

    /// The walk the shard sends when the player uses something out of reach: to a point **beyond**
    /// the obstacle, so the approach runs straight through where it stands.
    fn walk_past(c: &mut Character, obstacle_at: Vec3) -> Pos {
        let here = c.position();
        let goal = Pos::new(
            here.cell,
            Frame::new(
                Vec3::new(
                    obstacle_at.x + OBSTACLE_OFFSET.0,
                    obstacle_at.y + OBSTACLE_OFFSET.1,
                    obstacle_at.z,
                ),
                Quat::IDENTITY,
            ),
        );
        c.perform_move_to(
            &MoveToRequest::MoveToPosition { pos: goal },
            &MovementParameters::default(),
            Some(1.0),
        );
        goal
    }

    /// How far the body is **inside** the obstacle, in metres; zero when they do not overlap. A
    /// depth rather than a yes-or-no, because a body sliding round a rounded obstacle rests on its
    /// surface and dips a centimetre or two inside it at the sweep's granularity -- which is
    /// contact and not passing through.
    fn depth_inside(p: Vec3, obstacle_at: Vec3, body_radius: f32) -> f32 {
        let centre = p.add(Vec3::new(0.0, 0.0, body_radius));
        let o = obstacle_at.add(Vec3::new(0.0, 0.0, OBSTACLE_RADIUS));
        let d = centre.sub(o).mag2().sqrt();
        (OBSTACLE_RADIUS + body_radius - d).max(0.0)
    }

    /// **Start walking to something, then mash the forward key.** The body must not end up inside
    /// what is in the way, and must not end up past it.
    ///
    /// A walk the shard sends aims the body *exactly* at where it is going, and that exactness is
    /// what no key press can produce -- which is why the sequence reproduces this on demand and a
    /// driven client holding a key does not. The control is the same obstacle walked at five
    /// degrees off the same line by hand: it is turned aside before this behaviour and after it,
    /// which is what makes the first half a measurement of the exact case rather than of whether
    /// the obstacle is solid at all.
    pub fn an_exact_approach_does_not_go_through_what_is_in_the_way() {
        let store = store();

        // The sequence: a walk the shard sent, then the forward key.
        let mut c = walking_body(&store);
        let radius = c.radius();
        assert!(
            radius > 0.1 && radius < 1.0,
            "the body's own radius is {radius}"
        );
        let at = obstacle(&mut c);
        let start = c.position().frame.origin;
        let _goal = walk_past(&mut c, at);
        let running = c.is_moving_to();
        let (t, _) = run_body(&mut c, 0.0, 1.0, CharacterInput::default());
        let forward = CharacterInput {
            forward: true,
            ..CharacterInput::default()
        };
        let (_, path) = run_body(&mut c, t, 8.0, forward);
        let end = *path.last().expect("the walk produced frames");
        let deepest = path
            .iter()
            .map(|p| depth_inside(*p, at, radius))
            .fold(0.0_f32, f32::max);
        let beyond = end
            .sub(at)
            .dot(Vec3::new(OBSTACLE_OFFSET.0, OBSTACLE_OFFSET.1, 0.0))
            / 6.0;
        let set_off = end.sub(start).mag2().sqrt() > 1.0;

        // The calibration: the walk really did turn the body, and it snapped it onto the bearing.
        let mut c = walking_body(&store);
        let at2 = obstacle(&mut c);
        let before = dereth_physics::math::get_heading(&c.position().frame);
        let goal = walk_past(&mut c, at2);
        let _ = run_body(&mut c, 0.0, 3.0, CharacterInput::default());
        let after = dereth_physics::math::get_heading(&c.position().frame);
        let want = dereth_animation::motion::moveto::position_heading(&c.position(), &goal);
        let wrap = |d: f32| {
            let d = d.rem_euclid(360.0);
            if d > 180.0 {
                360.0 - d
            } else {
                d
            }
        };
        let err = wrap(after - want);
        let turned = wrap(after - before);

        // The control: five degrees off the same bearing, aimed by hand.
        let mut c = walking_body(&store);
        let radius3 = c.radius();
        let at3 = obstacle(&mut c);
        let bearing = dereth_primitives::num::math::atan2f(OBSTACLE_OFFSET.0, OBSTACLE_OFFSET.1);
        let half = (bearing + 5.0_f32.to_radians()) / 2.0;
        let mut here = c.position();
        here.frame.rotation = Quat::new(
            dereth_primitives::num::math::cosf(half),
            0.0,
            0.0,
            dereth_primitives::num::math::sinf(half),
        );
        c.teleport(here);
        let (t, _) = run_body(&mut c, 0.0, 1.0, CharacterInput::default());
        let (_, path3) = run_body(&mut c, t, 8.0, forward);
        let deepest3 = path3
            .iter()
            .map(|p| depth_inside(*p, at3, radius3))
            .fold(0.0_f32, f32::max);

        println!(
            "wall: the exact approach ended {deepest:.4} m inside the obstacle at its worst \
             and {beyond:.3} m beyond it; the walk turned the body {turned:.4} degrees onto the \
             bearing with a {err:.6} degree error; the hand-aimed control got {deepest3:.4} m in"
        );

        let held = running
            && set_off
            && deepest < CONTACT_TOLERANCE
            && beyond < 0.0
            && turned > 20.0
            && err < 0.01
            && deepest3 < CONTACT_TOLERANCE;

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.move-to.an-exact-approach-does-not-carry-the-body-through-what-is-in-the-way",
            move |_| held,
        );
    }

    #[test]
    fn scenario_an_exact_approach_does_not_go_through_what_is_in_the_way() {
        super::scenario("an_exact_approach_does_not_go_through_what_is_in_the_way");
    }

    // =======================================================================================
    // The walk bit: the body's walk-or-run modifier against the player's own run word.
    // =======================================================================================

    use dereth_animation::motion::HoldKey;
    use dereth_client_runtime::actions::movement::{action, on_action};
    use dereth_dat::DbType;
    use dereth_input::ActionId;
    use dereth_primitives::DataId;

    fn key(a: ActionId, start: bool) -> dereth_client_runtime::actions::Action {
        dereth_client_runtime::actions::Action {
            id: a,
            phase: if start {
                dereth_client_runtime::actions::ActionPhase::Begin
            } else {
                dereth_client_runtime::actions::ActionPhase::End
            },
            extent: 1.0,
            repeats: 0,
        }
    }

    /// The modifier the whole subject is about: whether the body thinks it is running or walking.
    fn hold_key(c: &Character) -> HoldKey {
        c.driver().movement.interp.raw_state.current_holdkey
    }

    /// What the body is actually playing, which is the observable the modifier decides.
    fn playing(c: &Character) -> MotionCommand {
        c.driver().movement.interp.interpreted_state.forward_command
    }

    fn settled_body(store: &Arc<RetailDatStore>) -> Character {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let mut c = Character::new(
            store,
            &region,
            dereth_client::world::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("the body");
        for i in 1..=60 {
            c.update(LocalTime(f64::from(i) / 30.0));
        }
        assert!(
            c.on_ground(),
            "the body must settle before anything is measured"
        );
        c
    }

    /// A settled body that runs rather than walks. *Run as default movement* is on, as it is on
    /// every shipped character, so the key-up state is the running one.
    fn running_body(store: &Arc<RetailDatStore>) -> (Character, MovementCommands, CharacterInput) {
        let mut c = settled_body(store);
        let mut mc = MovementCommands::default();
        let mut input = CharacterInput::default();
        mc.ui_toggles_run = true;
        assert!(mc.on_action(
            on_action(&key(action::TOGGLE_RUN_WALK, false), |_| None),
            &mut input
        ));
        assert!(
            input.run,
            "running is the default state and the key walks you"
        );
        let _ = mc.take_control_retake_pending();
        c.input = input;
        for i in 1..=4 {
            c.update(LocalTime(2.0 + f64::from(i) / 30.0));
        }
        assert_eq!(
            hold_key(&c),
            HoldKey::Run,
            "the premise: the body agrees it runs"
        );
        (c, mc, input)
    }

    fn press_forward(
        c: &mut Character,
        mc: &mut MovementCommands,
        input: &mut CharacterInput,
        t0: f64,
    ) {
        use dereth_client_runtime::actions::movement::{command, CmdStruct, MovementAction};
        assert!(mc.on_action(
            MovementAction::SetMotion(CmdStruct {
                command: command::WALK_FORWARD,
                extent: None,
                start: Some(true),
            }),
            input
        ));
        if mc.take_control_retake_pending() {
            c.take_control_from_server();
        }
        c.input = *input;
        for i in 1..=6 {
            c.update(LocalTime(t0 + f64::from(i) / 30.0));
        }
    }

    fn release_forward(
        c: &mut Character,
        mc: &mut MovementCommands,
        input: &mut CharacterInput,
        t0: f64,
    ) {
        use dereth_client_runtime::actions::movement::{command, CmdStruct, MovementAction};
        assert!(mc.on_action(
            MovementAction::SetMotion(CmdStruct {
                command: command::WALK_FORWARD,
                extent: None,
                start: Some(false),
            }),
            input
        ));
        c.input = *input;
        for i in 1..=6 {
            c.update(LocalTime(t0 + f64::from(i) / 30.0));
        }
    }

    /// Another shipped animation table that really loads, so that the scenario writes no id of
    /// its own.
    fn another_animation_table(store: &Arc<RetailDatStore>) -> DataId {
        use dereth_animation::data::AnimAssets as _;
        let assets = DatAnimAssets::new(Arc::clone(store));
        for id in store.ids_of(DbType::MTable) {
            if id != ALUVIAN_MALE_MOTION_TABLE && assets.motion_table(id).is_some() {
                return id;
            }
        }
        panic!("the shipped data holds no second animation table");
    }

    // ---------------------------------------------------------------------------------------
    // movement.run.a-body-whose-animation-table-is-replaced-still-runs-when-the-player-said-run
    // ---------------------------------------------------------------------------------------

    /// **The defect guarded: the body gets stuck walking and nothing clears it.**
    ///
    /// Any description the shard sends that names an animation table rebuilds the body's movement
    /// and puts its *walk or run* modifier back to walk -- while the player's own run word has not
    /// moved, so nothing the player did could tell the two apart. The next forward key must
    /// re-derive the modifier from what the player is actually holding rather than remember what
    /// it last wrote, and then the body runs again without the player touching the run key.
    pub fn a_replaced_animation_table_does_not_strand_the_body_in_walk() {
        let store = store();
        let (mut c, mut mc, mut input) = running_body(&store);

        press_forward(&mut c, &mut mc, &mut input, 3.0);
        let ran_first = playing(&c) == MotionCommand::RUN_FORWARD;
        release_forward(&mut c, &mut mc, &mut input, 4.0);

        // The producer, and it is the client's own: the body is given another animation table and
        // then its own back, so nothing but the movement state has changed.
        let other = another_animation_table(&store);
        assert!(c.set_setup_id(ALUVIAN_MALE_SETUP, Some(other)).is_ok());
        assert!(c
            .set_setup_id(ALUVIAN_MALE_SETUP, Some(ALUVIAN_MALE_MOTION_TABLE))
            .is_ok());
        let back_to_its_own = c.motion_table_id() == ALUVIAN_MALE_MOTION_TABLE;
        let disagreed = hold_key(&c) == HoldKey::None && input.run;

        press_forward(&mut c, &mut mc, &mut input, 5.0);
        let re_derived = hold_key(&c) == HoldKey::Run && playing(&c) == MotionCommand::RUN_FORWARD;

        println!(
            "walk bit: ran first {ran_first}, the table swap put the modifier back to \
             walk with the player's run word unmoved {disagreed}, and the next forward key \
             re-derived it {re_derived}"
        );

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.run.a-body-whose-animation-table-is-replaced-still-runs-when-the-player-said-run",
            move |_| ran_first && back_to_its_own && disagreed && re_derived,
        );
    }

    #[test]
    fn scenario_a_replaced_animation_table_does_not_strand_the_body_in_walk() {
        super::scenario("a_replaced_animation_table_does_not_strand_the_body_in_walk");
    }

    // ---------------------------------------------------------------------------------------
    // movement.approach.a-walked-one-leaves-the-players-own-way-of-moving-alone
    // ---------------------------------------------------------------------------------------

    /// **The guard, and it is green on both sides of the defect above.** The stuck walk was
    /// reported against a shard-driven approach, so the named trigger is checked rather than
    /// assumed: an approach close enough to be walked rather than run chooses its own walk for
    /// the duration and leaves the player's own way of moving exactly as it was.
    pub fn a_walked_approach_leaves_the_players_own_movement_alone() {
        let store = store();
        let (mut c, mut mc, mut input) = running_body(&store);
        let p = c.position();
        let o = p.frame.origin;
        let target = Pos::new(
            p.cell,
            Frame::new(Vec3::new(o.x + 3.0, o.y, o.z), Quat::IDENTITY),
        );

        // Three metres is well inside the threshold at which a walk becomes a run.
        c.perform_move_to(&move_to(TARGET), &use_params(0.6), Some(1.0));
        mc.lose_control_to_server(&mut input);
        let mut t = 3.0;
        for _ in 0..360 {
            if c.wanted_target().is_some() {
                c.update_target(target, Vec3::ZERO, true);
            }
            let (pending, moving) = {
                let d = c.driver();
                (d.movement.motions_pending(), d.movement.is_moving_to())
            };
            if mc.use_time(pending, moving, &mut input) {
                c.take_control_from_server();
            }
            c.input = input;
            t += 1.0 / 30.0;
            c.update(LocalTime(t));
        }
        let finished = !c.is_moving_to();
        let modifier_untouched = hold_key(&c) == HoldKey::Run;

        press_forward(&mut c, &mut mc, &mut input, t);
        let still_runs = playing(&c) == MotionCommand::RUN_FORWARD;

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.approach.a-walked-one-leaves-the-players-own-way-of-moving-alone",
            move |_| finished && modifier_untouched && still_runs,
        );
    }

    #[test]
    fn scenario_a_walked_approach_leaves_the_players_own_movement_alone() {
        super::scenario("a_walked_approach_leaves_the_players_own_movement_alone");
    }

    // =======================================================================================
    // movement.jump.a-body-in-the-air-turns-but-does-not-walk-and-takes-up-the-held-key-when-it-lands
    // =======================================================================================

    /// **What a body may do while its feet are off the ground.** It may turn, and it may not
    /// walk; the forward key it is holding is remembered rather than thrown away, and the moment
    /// the real floor comes back under it the body walks -- with no second press, no timeout and
    /// nothing forcing it back to standing. Let go and it stops; press back and it backs up.
    ///
    /// The floor is a real one, at a place a recording says a player really stood.
    pub fn a_body_in_the_air_turns_but_does_not_walk() {
        use dereth_animation::motion::MotionInterp;

        let store = store();
        let region = dereth_client::world::load_region(&store).expect("the region decodes");
        let mut c = Character::new(&store, &region, 0x7f03, (96.0, 96.0))
            .expect("a body in the training academy's landblock");

        // Where a recorded player stood, taken out of the recording rather than chosen.
        let corpus =
            dereth_client_net::client_session::testing::Corpus::load("early-inventory-and-casting")
                .expect("the corpus parses")
                .expect("the corpus is generated");
        let row = corpus
            .blobs
            .iter()
            .find(|r| r.idx == 143)
            .expect("the recorded position");
        let mut a = dereth_protocol::actions::unpack_action(&row.payload).expect("it unpacks");
        let p = dereth_protocol::movement::MovementMoveToState::read(&mut a.body)
            .expect("it decodes")
            .0
            .position;
        let pos = Pos::new(
            dereth_primitives::CellId(p.objcell_id),
            Frame::new(
                Vec3::new(p.frame.origin.x, p.frame.origin.y, p.frame.origin.z),
                Quat::new(
                    p.frame.orientation.w,
                    p.frame.orientation.x,
                    p.frame.orientation.y,
                    p.frame.orientation.z,
                ),
            ),
        );
        c.land().load_block_cells(pos.cell.landblock());
        c.teleport(pos);
        c.stop_completely_from_action();

        let mut frame = 0u32;
        let mut agreed = true;
        let step = |c: &mut Character, frame: &mut u32, agreed: &mut bool| {
            *frame += 1;
            c.update(LocalTime(f64::from(*frame) / 30.0));
            let body = c.world.get(c.handle).expect("the body is live");
            let in_contact = body.transient_state.in_contact();
            let in_cell = body.cell.is_some();
            let on_ground = c.on_ground();
            let d = c.driver();
            *agreed &= d.env.contact == in_contact
                && d.env.on_ground == on_ground
                && d.env.in_cell == in_cell;
        };
        for _ in 0..60 {
            step(&mut c, &mut frame, &mut agreed);
        }
        let landed_first = c.on_ground();

        c.input.jump = true;
        step(&mut c, &mut frame, &mut agreed);
        c.input.jump = false;
        let left_the_floor = !c.on_ground();

        c.input.forward = true;
        c.input.turn_left = true;
        let airborne_heading = dereth_physics::math::get_heading(&c.position().frame);
        let mut air = 0;
        let mut could_not_walk = true;
        let mut could_turn = true;
        let mut key_remembered = true;
        while !c.on_ground() && air < 120 {
            step(&mut c, &mut frame, &mut agreed);
            air += 1;
            if !c.on_ground() {
                let d = c.driver();
                could_not_walk &=
                    !MotionInterp::contact_allows_move(MotionCommand::WALK_FORWARD, &d.env);
                could_turn &= MotionInterp::contact_allows_move(MotionCommand::TURN_LEFT, &d.env);
                key_remembered &=
                    d.movement.interp.raw_state.forward_command == MotionCommand::WALK_FORWARD;
            }
        }
        let came_back_down = air > 1 && c.on_ground();
        let turned = (dereth_physics::math::get_heading(&c.position().frame) - airborne_heading)
            .abs()
            > f32::EPSILON;

        c.input.turn_left = false;
        let at_landing = c.position();
        for _ in 0..30 {
            step(&mut c, &mut frame, &mut agreed);
        }
        let walked_on_landing =
            dereth_animation::motion::moveto::distance(&at_landing, &c.position()) > 0.5;

        c.input = CharacterInput::default();
        for _ in 0..60 {
            step(&mut c, &mut frame, &mut agreed);
        }
        let stopped = c.position();
        for _ in 0..30 {
            step(&mut c, &mut frame, &mut agreed);
        }
        let stayed_stopped = dereth_animation::motion::moveto::distance(&stopped, &c.position())
            < 0.05
            && !c.driver().movement.motions_pending()
            && c.driver().motion_table.pending_len() == 0;

        c.input.back = true;
        for _ in 0..30 {
            step(&mut c, &mut frame, &mut agreed);
        }
        let backed_up = dereth_animation::motion::moveto::distance(&stopped, &c.position()) > 0.3;
        c.input = CharacterInput::default();
        for _ in 0..60 {
            step(&mut c, &mut frame, &mut agreed);
        }
        let back_to_standing =
            c.driver().movement.interp.interpreted_state.forward_command == MotionCommand::READY;

        println!("door: {air} frames in the air, then the floor came back");

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.jump.a-body-in-the-air-turns-but-does-not-walk-and-takes-up-the-held-key-when-it-lands",
            move |_| {
                agreed
                    && landed_first
                    && left_the_floor
                    && could_not_walk
                    && could_turn
                    && key_remembered
                    && came_back_down
                    && turned
                    && walked_on_landing
                    && stayed_stopped
                    && backed_up
                    && back_to_standing
            },
        );
    }

    #[test]
    fn scenario_a_body_in_the_air_turns_but_does_not_walk() {
        super::scenario("a_body_in_the_air_turns_but_does_not_walk");
    }

    // =======================================================================================
    // The stance a thrown weapon puts the body in.
    //
    // These two rows sit with the motion vocabulary; see the note at the top of this block.
    // =======================================================================================

    /// What the shard sends for a body that is standing in its stance.
    const READY_INDEX: u16 = 3;
    /// The two stances, as the low half of the word the shard sends. A bow's is the same number
    /// in both the shipped vocabulary and the one the reference server writes from; a thrown
    /// weapon's is not, which is what made the defect this came from.
    const BOW_INDEX: u16 = 0x003F;
    const THROWN_INDEX: u16 = 0x013B;

    /// The exact message the shard sends when the player's stance changes, built as **bytes** so
    /// that the word under test is the word that crosses the wire.
    fn stance_message(object: ObjectId, stance_index: u16) -> Vec<u8> {
        use dereth_protocol::archive::Writer;
        let mut w = Writer::new();
        w.u32(0xF74C);
        w.u32(object.0);
        w.u16(1); // the object-instance sequence the message header carries
        w.u16(4); // movement timestamp
        w.u16(2); // the shard's own control timestamp
        w.u8(0); // not an echo of the player's own move: this is the shard moving him
        w.align4();
        w.u16(0); // no walk-or-turn instruction, and no flags
        w.u16(stance_index); // the style word, before the interpreted state
        w.u32(0b11); // the interpreted state carries a style and a forward command
        w.u16(stance_index);
        w.u16(READY_INDEX);
        w.align4();
        w.into_inner()
    }

    /// The two statements the world makes with a stance-carrying message, on a body.
    fn apply_shard_stance(c: &mut Character, blob: &[u8]) {
        use dereth_protocol::archive::Reader as ArchiveReader;
        let mut r = ArchiveReader::new(blob);
        r.u32().expect("opcode");
        r.u32().expect("object");
        r.u16().expect("instance");
        let buf =
            dereth_protocol::movement::MovementBuffer::read(&mut r).expect("the buffer decodes");
        assert!(
            !buf.autonomous,
            "the shard's own move, not an echo of the player's"
        );

        let wire = buf
            .body
            .interpreted
            .as_ref()
            .expect("this message carries one");
        let cmd = |i: Option<u16>, fallback: MotionCommand| {
            i.and_then(MotionCommand::from_index).unwrap_or(fallback)
        };
        let base = InterpretedMotionState::default();
        let state = InterpretedMotionState {
            current_style: cmd(wire.current_style, base.current_style),
            forward_command: cmd(wire.forward_command, base.forward_command),
            ..base
        };
        if let Some(style) = MotionCommand::from_index(buf.body.current_style) {
            c.apply_movement_style(style);
        }
        c.move_to_interpreted_state(&state);
        for i in 1..=30 {
            c.update(LocalTime(2.0 + f64::from(i) / 30.0));
        }
    }

    /// A player in missile combat with something attackable picked out, carrying the combat table
    /// a real character is born with.
    fn world_with_target() -> (dereth_client_model::World, ObjectId) {
        use dereth_client_model::combat::{CombatMode, COMBAT_TABLE_DID};
        use dereth_client_model::inventory::slots::loc;
        let mut w = dereth_client_model::World::new();
        let player = ObjectId(0x5000_0476);
        let monster = ObjectId(0x8000_0777);
        w.player = Some(player);
        let mut pw = dereth_client_model::Weenie::new(player);
        pw.pwd.name = "Aldis".into();
        pw.qualities
            .get_or_insert_with(dereth_client_model::Qualities::new)
            .set(
                dereth_client_model::StatKey::new(
                    dereth_client_model::StatType::Did,
                    COMBAT_TABLE_DID,
                ),
                dereth_client_model::StatValue::Did(DataId(0x3000_0000)),
            );
        w.tables.weenies.insert(player, pw);
        w.tables.inventories.insert(
            player,
            dereth_client_model::objects::ObjectInventory::new(player),
        );
        let mut m = dereth_client_model::Weenie::new(monster);
        m.pwd.name = "Mosswart".into();
        m.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
        m.pwd.bitfield |= dereth_client_model::weenie::bitfield::ATTACKABLE;
        w.tables.weenies.insert(monster, m);
        w.set_selected_object(Some(monster), false, &mut dereth_client_model::NullSink);
        w.inventory_mask = loc::MISSILE_WEAPON | loc::MELEE_WEAPON | loc::HELD;
        w.combat.combat_mode = CombatMode::Missile;
        (w, monster)
    }

    /// What one weapon's whole story produces, so that a bow and a thrown weapon are read the
    /// same way.
    struct Story {
        stance: u32,
        ready_to_change_mode: bool,
        ready_to_attack: bool,
        left_combat: bool,
        mode_changes_sent: usize,
        attacks_sent: usize,
    }

    fn play(stance_index: u16) -> Story {
        use dereth_client_model::combat::{AttackHeight, CombatMode};
        use dereth_client_model::{RecordingRequests, Request};

        let store = store();
        let mut c = settled_body(&store);
        let (mut w, _monster) = world_with_target();
        let player = w.player.expect("seeded");

        apply_shard_stance(&mut c, &stance_message(player, stance_index));
        {
            let d = c.driver();
            w.combat.current_style = d.movement.interp.interpreted_state.current_style.0;
            w.combat.forward_command = d.movement.interp.interpreted_state.forward_command.0;
        }

        let pending = c.driver().movement.motions_pending();
        let ready_to_change_mode = w.player_in_ready_position(false, Some(pending));
        let ready_to_attack = w.player_in_ready_position(true, Some(pending));

        let mut attack = RecordingRequests::default();
        let _ = w.execute_attack(&mut attack, AttackHeight::Medium, true, ready_to_attack);
        let attacks_sent = attack
            .0
            .iter()
            .filter(|r| matches!(r, Request::TargetedMissileAttack(_)))
            .count();

        let mut req = RecordingRequests::default();
        let mut out = dereth_client_model::NullSink;
        let (to, _refusal) = w.toggle_combat_mode_target(false);
        let _ = w.set_combat_mode(&mut req, &mut out, to, true, ready_to_change_mode, false);
        // Ten frames of the retry, because a defect that only delays the send by a frame must not
        // read as the one this came from.
        for _ in 0..10 {
            let _ = w.combat_use_time(&mut req, &mut out, ready_to_change_mode);
        }
        let mode_changes_sent = req
            .0
            .iter()
            .filter(|r| {
                matches!(r, Request::ChangeCombatMode(m) if m.combat_mode == CombatMode::NonCombat.raw())
            })
            .count();

        let stance = c.driver().movement.interp.interpreted_state.current_style.0;
        Story {
            stance,
            ready_to_change_mode,
            ready_to_attack,
            left_combat: w.combat.combat_mode == CombatMode::NonCombat,
            mode_changes_sent,
            attacks_sent,
        }
    }

    // ---------------------------------------------------------------------------------------
    // movement.stance.the-one-the-shard-sends-for-a-thrown-weapon-reaches-the-body
    // ---------------------------------------------------------------------------------------

    /// **The first symptom: "it just shows me standing".** The stance the shard sends for a thrown
    /// weapon has to arrive as a *stance* and not be filed as some unrelated pose, or the player
    /// never gets into it.
    pub fn the_thrown_weapon_stance_reaches_the_body() {
        let s = play(THROWN_INDEX);
        println!(
            "stance: the thrown-weapon stance arrived as {:#010X}",
            s.stance
        );
        let held = s.stance == 0x8000_013B;
        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.stance.the-one-the-shard-sends-for-a-thrown-weapon-reaches-the-body",
            move |_| held,
        );
    }

    #[test]
    fn scenario_the_thrown_weapon_stance_reaches_the_body() {
        super::scenario("the_thrown_weapon_stance_reaches_the_body");
    }

    // ---------------------------------------------------------------------------------------
    // movement.stance.a-body-that-is-in-its-stance-can-attack-and-can-leave-combat-mode
    // ---------------------------------------------------------------------------------------

    /// **The other two symptoms: no throw, and no way out of combat.** Both are decided by the same
    /// question -- is the body in its stance? -- so a body that is gets both, and the observable is
    /// the message that reaches the shard rather than a local field: the way out of combat is
    /// parked rather than refused when the body is not ready, so asking whether the call succeeded
    /// would be green against the defect.
    ///
    /// A bow is the control. Its stance is the same word in both vocabularies, so its half passes
    /// whatever happens to a thrown weapon's -- which is what says this can express a success at
    /// all.
    pub fn a_body_in_its_stance_can_attack_and_leave_combat() {
        let bow = play(BOW_INDEX);
        let thrown = play(THROWN_INDEX);
        println!(
            "stance: the bow control -- stance {:#010X}, {} attack(s) and {} way(s) out sent; \
             the thrown weapon -- stance {:#010X}, {} and {}",
            bow.stance,
            bow.attacks_sent,
            bow.mode_changes_sent,
            thrown.stance,
            thrown.attacks_sent,
            thrown.mode_changes_sent
        );
        let ok = |s: &Story, want: u32| {
            s.stance == want
                && s.ready_to_change_mode
                && s.ready_to_attack
                && s.attacks_sent == 1
                && s.mode_changes_sent == 1
                && s.left_combat
        };
        let held = ok(&bow, 0x8000_003F) && ok(&thrown, 0x8000_013B);

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "movement.stance.a-body-that-is-in-its-stance-can-attack-and-can-leave-combat-mode",
            move |_| held,
        );
    }

    #[test]
    fn scenario_a_body_in_its_stance_can_attack_and_leave_combat() {
        super::scenario("a_body_in_its_stance_can_attack_and_leave_combat");
    }

    // =======================================================================================
    // scenery.animation.a-delayed-effect-still-happens-when-the-frame-empties-the-queue
    // =======================================================================================

    /// **A torch keeps flickering.** A piece of scenery whose shipped animation script schedules
    /// its next effect for later still produces it, although the frame takes everything the
    /// object has raised off it every single tick. The timer and the raised effects are two
    /// different lists, and emptying one must not empty the other -- an implementation that
    /// reached for the wrong one would stop the scenery for ever and nothing else in the suite
    /// would notice.
    ///
    /// The script is a shipped one, and the delayed kind on purpose: the scenery of an ordinary
    /// outdoor landblock uses the immediate kind, so it exercises the re-arming loop and not the
    /// timer.
    pub fn a_delayed_scenery_effect_survives_the_frames_drain() {
        use dereth_animation::{AnimAssets as AnimAssetsTrait, AnimEvent};
        use dereth_primitives::ServerTime;

        /// One of the shipped four-script ambient rings: each makes an effect at once and asks
        /// for the next script after a rolled pause.
        const RING: DataId = DataId(0x3300_11C3);
        const RING_CHILD: DataId = DataId(0x3300_11C4);

        let store = store();
        let assets: Arc<dyn AnimAssetsTrait> = Arc::new(DatAnimAssets::new(Arc::clone(&store)));
        let mut d = MotionDriver::new(assets);
        // A placed piece of scenery is always in a cell, and the delayed arm reads that when it
        // fires.
        d.env.in_cell = true;
        d.cur_time = ServerTime(0.0);
        assert!(
            d.play_script_internal(RING),
            "the shipped ring script queues"
        );

        let dt = 1.0 / 30.0;
        let mut t = 0.0;
        let mut armed: Option<(f64, f32)> = None;
        let mut effects: Vec<f64> = Vec::new();
        let mut high_water = 0usize;
        let mut drained_clean = true;

        for _ in 0..60 {
            d.cur_time = ServerTime(t);
            d.update_scripts();
            d.update_particles(false);
            d.update_fp_hooks();
            // The frame's own drain, verbatim: take everything, every tick, and nothing else.
            high_water = high_water.max(d.pending_events());
            for e in d.take_events() {
                match e {
                    AnimEvent::CallPes { script, pause } => {
                        if armed.is_none() && script == RING_CHILD {
                            assert!(pause > 0.0, "the ring's hook is the delayed kind");
                            armed = Some((t, pause));
                        }
                    }
                    AnimEvent::CreateParticleEmitter { .. } => effects.push(t),
                    _ => {}
                }
            }
            drained_clean &= d.pending_events() == 0;
            t += dt;
        }

        let (arm_t, delay) = armed.expect("the ring script's delayed request never fired");
        let after: Vec<f64> = effects.iter().copied().filter(|x| *x > arm_t).collect();
        println!(
            "host event drain: armed at {arm_t:.4} s for {delay} s, {} effect(s) afterwards, queue \
             high-water {high_water}",
            after.len()
        );
        let due = arm_t + f64::from(delay);
        let fired_on_time =
            !after.is_empty() && after[0] >= due - 1e-9 && after[0] <= due + 3.0 * dt;
        // Draining every tick means the queue never holds more than one tick's worth.
        let keeps_up = high_water <= 8;

        let mut client = HeadlessClient::model();
        client.assert_behaviour(
            "scenery.animation.a-delayed-effect-still-happens-when-the-frame-empties-the-queue",
            move |_| drained_clean && fired_on_time && keeps_up,
        );
    }

    #[test]
    fn scenario_a_delayed_scenery_effect_survives_the_frames_drain() {
        super::scenario("a_delayed_scenery_effect_survives_the_frames_drain");
    }
}
